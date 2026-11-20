use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::ConfigStore;
use crate::languages::{self, LanguageProfile, StyleProfile, ThemeDefinition, ThemeLexer};

pub(crate) const FONT_SIZES: &[&str] = &["", "5", "6", "7", "8", "9", "10", "11", "12", "14", "16", "18", "20", "22", "24", "26", "28"];

#[derive(Default)]
pub(crate) struct Node {
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub text: String,
    pub children: NodeChildren,
}

#[derive(Default)]
pub(crate) struct NodeChildren(Vec<Node>);

impl std::ops::Deref for NodeChildren {
    type Target = Vec<Node>;
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl std::ops::DerefMut for NodeChildren {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl From<Vec<Node>> for NodeChildren {
    fn from(nodes: Vec<Node>) -> Self { Self(nodes) }
}

impl FromIterator<Node> for NodeChildren {
    fn from_iter<T: IntoIterator<Item = Node>>(nodes: T) -> Self { Self(nodes.into_iter().collect()) }
}

impl<'a> IntoIterator for &'a NodeChildren {
    type Item = &'a Node;
    type IntoIter = std::slice::Iter<'a, Node>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

impl<'a> IntoIterator for &'a mut NodeChildren {
    type Item = &'a mut Node;
    type IntoIter = std::slice::IterMut<'a, Node>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter_mut() }
}

impl IntoIterator for NodeChildren {
    type Item = Node;
    type IntoIter = std::vec::IntoIter<Node>;
    fn into_iter(mut self) -> Self::IntoIter { std::mem::take(&mut self.0).into_iter() }
}

impl Drop for NodeChildren {
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.0);
        while let Some(mut node) = pending.pop() { pending.append(&mut node.children.0); }
    }
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|node| node.name == name)
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    pub fn parse(xml: &str) -> io::Result<Self> {
        let mut parser = XmlParser { xml: xml.trim_start_matches('\u{feff}'), offset: 0 };
        parser.misc()?;
        let root = parser.node()?;
        parser.misc()?;
        if parser.offset != parser.xml.len() {
            return Err(invalid("XML has content after its root element"));
        }
        Ok(root)
    }

    pub fn xml(&self) -> String {
        let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n");
        self.write(&mut output);
        output.push('\n');
        output
    }

    fn write(&self, output: &mut String) {
        let mut pending = vec![(self, false)];
        while let Some((node, closing)) = pending.pop() {
            if closing {
                output.push_str("</");
                output.push_str(&node.name);
                output.push('>');
                continue;
            }
            output.push('<');
            output.push_str(&node.name);
            for (name, value) in &node.attributes {
                output.push(' ');
                output.push_str(name);
                output.push_str("=\"");
                output.push_str(&escape(value));
                output.push('"');
            }
            if node.text.is_empty() && node.children.is_empty() {
                output.push_str(" />");
            } else {
                output.push('>');
                output.push_str(&escape(&node.text));
                pending.push((node, true));
                pending.extend(node.children.iter().rev().map(|child| (child, false)));
            }
        }
    }

    fn shallow_clone(&self) -> Self {
        Self { name: self.name.clone(), attributes: self.attributes.clone(), text: self.text.clone(), children: NodeChildren::default() }
    }
}

impl Clone for Node {
    fn clone(&self) -> Self {
        let mut source = self;
        let mut target = self.shallow_clone();
        let mut index = 0;
        let mut parents = Vec::new();
        loop {
            if let Some(child) = source.children.get(index) {
                parents.push((source, target, index + 1));
                source = child;
                target = child.shallow_clone();
                index = 0;
            } else if let Some((parent_source, mut parent_target, parent_index)) = parents.pop() {
                parent_target.children.push(target);
                source = parent_source;
                target = parent_target;
                index = parent_index;
            } else {
                return target;
            }
        }
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Node").field("xml", &self.xml()).finish()
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        let mut pending = vec![(self, other)];
        while let Some((left, right)) = pending.pop() {
            if left.name != right.name || left.attributes != right.attributes || left.text != right.text || left.children.len() != right.children.len() { return false; }
            pending.extend(left.children.iter().zip(&right.children));
        }
        true
    }
}

impl Eq for Node {}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn unescape(text: &str) -> io::Result<String> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        let end = rest.find(';').ok_or_else(|| invalid("Unterminated XML entity"))?;
        let entity = &rest[..end];
        let ch = match entity {
            "amp" => '&', "lt" => '<', "gt" => '>', "quot" => '"', "apos" => '\'',
            _ => {
                let value = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    entity.strip_prefix('#').and_then(|value| value.parse().ok())
                };
                value.and_then(char::from_u32).ok_or_else(|| invalid("Invalid XML entity"))?
            }
        };
        output.push(ch);
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

struct XmlParser<'a> { xml: &'a str, offset: usize }

impl XmlParser<'_> {
    fn rest(&self) -> &str { &self.xml[self.offset..] }

    fn space(&mut self) {
        while self.rest().as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
            self.offset += 1;
        }
    }

    fn through(&mut self, delimiter: &str) -> io::Result<()> {
        let end = self.rest().find(delimiter).ok_or_else(|| invalid("Unterminated XML declaration or comment"))?;
        self.offset += end + delimiter.len();
        Ok(())
    }

    fn misc(&mut self) -> io::Result<()> {
        loop {
            self.space();
            if self.rest().starts_with("<?") { self.through("?>")?; }
            else if self.rest().starts_with("<!--") { self.through("-->")?; }
            else { return Ok(()); }
        }
    }

    fn name(&mut self) -> io::Result<String> {
        let start = |ch: char| matches!(ch,
            ':' | '_' | 'A'..='Z' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}' |
            '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' |
            '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}');
        let mut length = 0;
        for (offset, ch) in self.rest().char_indices() {
            if !start(ch) && !(offset > 0 && matches!(ch, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}')) { break; }
            length = offset + ch.len_utf8();
        }
        if length == 0 { return Err(invalid("Missing XML name")); }
        let name = self.rest()[..length].to_owned();
        self.offset += length;
        Ok(name)
    }

    fn start_node(&mut self) -> io::Result<(Node, bool)> {
        if !self.rest().starts_with('<') { return Err(invalid("Expected XML element")); }
        self.offset += 1;
        let name = self.name()?;
        let mut node = Node { name, ..Node::default() };
        loop {
            self.space();
            if self.rest().starts_with("/>") { self.offset += 2; return Ok((node, true)); }
            if self.rest().starts_with('>') { self.offset += 1; return Ok((node, false)); }
            let name = self.name()?;
            self.space();
            if !self.rest().starts_with('=') { return Err(invalid("Missing XML attribute value")); }
            self.offset += 1;
            self.space();
            let quote = self.rest().chars().next().ok_or_else(|| invalid("Missing XML quote"))?;
            if quote != '\'' && quote != '"' { return Err(invalid("Unquoted XML attribute")); }
            self.offset += 1;
            let end = self.rest().find(quote).ok_or_else(|| invalid("Unterminated XML attribute"))?;
            let value = unescape(&self.rest()[..end])?;
            self.offset += end + 1;
            if node.attributes.insert(name, value).is_some() { return Err(invalid("Duplicate XML attribute")); }
        }
    }

    fn node(&mut self) -> io::Result<Node> {
        let (mut node, mut closed) = self.start_node()?;
        let mut parents = Vec::<Node>::new();
        loop {
            if closed {
                if let Some(mut parent) = parents.pop() {
                    parent.children.push(node);
                    node = parent;
                    closed = false;
                } else { return Ok(node); }
            } else if self.rest().starts_with("</") {
                self.offset += 2;
                let name = self.name()?;
                self.space();
                if name != node.name || !self.rest().starts_with('>') { return Err(invalid("Mismatched XML closing element")); }
                self.offset += 1;
                closed = true;
            } else if self.rest().starts_with("<!--") { self.through("-->")?; }
            else if self.rest().starts_with("<?") { self.through("?>")?; }
            else if self.rest().starts_with("<![CDATA[") {
                self.offset += 9;
                let end = self.rest().find("]]>").ok_or_else(|| invalid("Unterminated CDATA"))?;
                node.text.push_str(&self.rest()[..end]);
                self.offset += end + 3;
            } else if self.rest().starts_with('<') {
                let (child, self_closing) = self.start_node()?;
                parents.push(node);
                node = child;
                closed = self_closing;
            }
            else {
                let end = self.rest().find('<').ok_or_else(|| invalid("Unterminated XML element"))?;
                node.text.push_str(&unescape(&self.rest()[..end])?);
                self.offset += end;
            }
        }
    }
}

pub(crate) fn read_xml(path: &Path) -> io::Result<Node> {
    let bytes = fs::read(path)?;
    parse_xml(&bytes)
}

fn parse_xml(bytes: &[u8]) -> io::Result<Node> {
    let xml = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 { return Err(invalid("Invalid UTF-16 XML")); }
        let little = bytes[0] == 0xff;
        let units = bytes[2..].chunks_exact(2).map(|pair| if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) }).collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|error| invalid(error.to_string()))?
    } else {
        std::str::from_utf8(bytes).map_err(|error| invalid(error.to_string()))?.to_owned()
    };
    Node::parse(&xml)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GlobalOverride {
    pub foreground: bool,
    pub background: bool,
    pub font: bool,
    pub font_size: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl GlobalOverride {
    pub fn flag(&self, index: usize) -> bool {
        [self.foreground, self.background, self.font, self.font_size, self.bold, self.italic, self.underline].get(index).copied().unwrap_or(false)
    }

    pub fn toggle(&mut self, index: usize) {
        let flag = match index { 0 => &mut self.foreground, 1 => &mut self.background, 2 => &mut self.font, 3 => &mut self.font_size, 4 => &mut self.bold, 5 => &mut self.italic, 6 => &mut self.underline, _ => return };
        *flag = !*flag;
    }

    fn apply(&self, target: &mut StyleProfile, global: &StyleProfile) {
        for (enabled, name, mask) in [(self.foreground, "fgColor", 1), (self.background, "bgColor", 2)] {
            if !enabled { continue; }
            let mut bits = number(target, "colorStyle", 3);
            if number(global, "colorStyle", 3) & mask != 0 {
                bits |= mask;
                if let Some(value) = global.attributes.get(name) { target.attributes.insert(name.into(), value.clone()); }
            } else if target.id == 32 { bits |= mask; } else { bits &= !mask; }
            target.attributes.insert("colorStyle".into(), bits.to_string());
        }
        if self.font && let Some(value) = global.attributes.get("fontName").filter(|value| !value.is_empty()) { target.attributes.insert("fontName".into(), value.clone()); }
        if self.font_size && number(global, "fontSize", 0) > 0 && let Some(value) = global.attributes.get("fontSize") { target.attributes.insert("fontSize".into(), value.clone()); }
        if number(global, "fontStyle", -1) == -1 { return; }
        let mut bits = number(target, "fontStyle", -1);
        let global_bits = number(global, "fontStyle", -1);
        for (enabled, mask) in [(self.bold, 1), (self.italic, 2), (self.underline, 4)] {
            if enabled { bits = (bits & !mask) | (global_bits & mask); }
        }
        if self.bold || self.italic || self.underline { target.attributes.insert("fontStyle".into(), bits.to_string()); }
    }
}

pub(crate) fn number(style: &StyleProfile, key: &str, default: i32) -> i32 {
    style.attributes.get(key).and_then(|value| value.parse().ok()).unwrap_or(default)
}

pub(crate) fn keyword_index(class: &str) -> Option<usize> {
    match class {
        "instre1" => Some(0), "instre2" => Some(1),
        "type1" => Some(2), "type2" => Some(3), "type3" => Some(4), "type4" => Some(5), "type5" => Some(6), "type6" => Some(7), "type7" => Some(8),
        _ => match class.as_bytes() {
            [b'0'..=b'8'] => Some(usize::from(class.as_bytes()[0] - b'0')),
            [b's', b'u', b'b', b's', b't', b'y', b'l', b'e', n @ b'1'..=b'8'] => Some(usize::from(*n - b'1') + 9),
            _ => None,
        },
    }
}

#[derive(Clone)]
pub(crate) struct ThemeEntry {
    pub name: String,
    pub filename: String,
    definition: Option<Arc<ThemeDefinition>>,
    document: Option<Arc<Node>>,
    path: Option<PathBuf>,
}

fn style_node(style: &StyleProfile, name: &str) -> Node {
    Node { name: name.into(), attributes: style.attributes.clone(), text: style.keywords.clone(), children: NodeChildren::default() }
}

impl ThemeEntry {
    fn frozen(dark: bool) -> io::Result<Self> {
        let mut definition = languages::catalog().map_err(|error| invalid(error.to_string()))?.themes().theme(dark).clone();
        if let Some(style) = definition.global_styles.iter_mut().find(|style| style.id == 32) {
            style.attributes.insert("fontSize".into(), "16".into());
            style.attributes.insert("fontName".into(), "Menlo".into());
        }
        if !dark {
            for (name, attribute, value) in [
                ("Caret colour", "fgColor", "000000"),
                ("Current line background colour", "bgColor", "FBFCD9"),
            ] {
                if let Some(style) = definition.global_styles.iter_mut().find(|style| {
                    style.attributes.get("name").is_some_and(|value| value == name)
                }) {
                    style.attributes.insert(attribute.into(), value.into());
                }
            }
        }
        let filename = if dark { "themes/DarkModeDefault.xml" } else { "stylers.xml" }.to_owned();
        let mut lexers = Node { name: "LexerStyles".into(), ..Node::default() };
        for lexer in definition.lexers.values() {
            lexers.children.push(Node { name: "LexerType".into(), attributes: lexer.attributes.clone(), text: String::new(), children: lexer.styles.iter().map(|style| style_node(style, "WordsStyle")).collect() });
        }
        let globals = Node { name: "GlobalStyles".into(), children: definition.global_styles.iter().map(|style| style_node(style, "WidgetStyle")).collect(), ..Node::default() };
        Ok(Self { name: if dark { "DarkModeDefault" } else { "Default (stylers.xml)" }.into(), filename, definition: Some(Arc::new(definition)), document: Some(Arc::new(Node { name: "NotepadPlus".into(), children: vec![lexers, globals].into(), ..Node::default() })), path: None })
    }

    fn registered(filename: String, path: PathBuf) -> io::Result<Self> {
        let name = if filename == "stylers.xml" { "Default (stylers.xml)".to_owned() } else {
            Path::new(&filename).file_stem().and_then(|name| name.to_str()).ok_or_else(|| invalid("Invalid theme filename"))?.to_owned()
        };
        Ok(Self { name, filename, definition: None, document: None, path: Some(path) })
    }

    pub fn reload(&self, store: Option<&ConfigStore>) -> io::Result<Self> {
        let user_path = store.map(|store| store.root().join(&self.filename));
        let path = user_path.filter(|path| path.is_file()).or_else(|| self.path.clone());
        if let Some(path) = path {
            let mut entry = Self::from_document(self.filename.clone(), read_xml(&path)?)?;
            entry.path = Some(path);
            Ok(entry)
        } else if self.filename == "stylers.xml" || self.filename == "themes/DarkModeDefault.xml" {
            Self::frozen(self.filename == "themes/DarkModeDefault.xml")
        } else { Ok(self.clone()) }
    }

    pub fn from_document(filename: String, document: Node) -> io::Result<Self> {
        if document.name != "NotepadPlus" { return Err(invalid("Style theme root must be NotepadPlus")); }
        let lexer_root = document.child("LexerStyles").ok_or_else(|| invalid("Style theme has no LexerStyles"))?;
        let global_root = document.child("GlobalStyles").ok_or_else(|| invalid("Style theme has no GlobalStyles"))?;
        let parse_styles = |node: &Node, kind: &str| -> Vec<StyleProfile> {
            node.children.iter().filter(|node| node.name == kind).filter_map(|node| {
                Some(StyleProfile { id: node.attribute("styleID")?.parse().ok()?, attributes: node.attributes.clone(), keywords: node.text.clone() })
            }).collect()
        };
        let mut lexers = BTreeMap::new();
        for node in lexer_root.children.iter().filter(|node| node.name == "LexerType") {
            if let Some(name) = node.attribute("name") {
                lexers.insert(name.to_owned(), ThemeLexer { attributes: node.attributes.clone(), styles: parse_styles(node, "WordsStyle") });
            }
        }
        let mut global_styles = parse_styles(global_root, "WidgetStyle");
        let defaults = languages::catalog().map_err(|error| invalid(error.to_string()))?.themes().theme(false);
        for (name, default) in &defaults.lexers {
            if let Some(lexer) = lexers.get_mut(name) {
                for style in &default.styles {
                    if !lexer.styles.iter().any(|existing| existing.attributes.get("name") == style.attributes.get("name")) { lexer.styles.push(style.clone()); }
                }
            } else { lexers.insert(name.clone(), default.clone()); }
        }
        for style in &defaults.global_styles {
            if !global_styles.iter().any(|existing| existing.attributes.get("name") == style.attributes.get("name")) {
                global_styles.push(style.clone());
            }
        }
        let name = if filename == "stylers.xml" { "Default (stylers.xml)".to_owned() } else {
            Path::new(&filename).file_stem().and_then(|name| name.to_str()).ok_or_else(|| invalid("Invalid theme filename"))?.to_owned()
        };
        Ok(Self { name, definition: Some(Arc::new(ThemeDefinition { source_path: filename.clone(), lexers, global_styles })), filename, document: Some(Arc::new(document)), path: None })
    }

    pub fn xml(&self) -> io::Result<String> {
        let definition = self.definition.as_ref().ok_or_else(|| invalid("Theme has not been loaded"))?;
        let mut document = self.document.as_deref().ok_or_else(|| invalid("Theme XML has not been loaded"))?.clone();
        if let Some(root) = document.children.iter_mut().find(|node| node.name == "LexerStyles") {
            for (name, lexer) in &definition.lexers {
                if !root.children.iter().any(|node| node.name == "LexerType" && node.attribute("name") == Some(name.as_str())) {
                    root.children.push(Node { name: "LexerType".into(), attributes: lexer.attributes.clone(), text: String::new(), children: lexer.styles.iter().map(|style| style_node(style, "WordsStyle")).collect() });
                }
            }
            for node in root.children.iter_mut().filter(|node| node.name == "LexerType") {
                let Some(lexer) = node.attribute("name").and_then(|name| definition.lexers.get(name)) else { continue; };
                node.attributes = lexer.attributes.clone();
                for child in node.children.iter_mut().filter(|node| node.name == "WordsStyle") {
                    if let Some(style) = lexer.styles.iter().find(|style| child.attributes.get("name") == style.attributes.get("name")) {
                        child.attributes = style.attributes.clone();
                        child.text = style.keywords.clone();
                    }
                }
                for style in &lexer.styles {
                    if !node.children.iter().any(|child| child.name == "WordsStyle" && child.attributes.get("name") == style.attributes.get("name")) { node.children.push(style_node(style, "WordsStyle")); }
                }
            }
        }
        if let Some(root) = document.children.iter_mut().find(|node| node.name == "GlobalStyles") {
            for style in &definition.global_styles {
                if let Some(node) = root.children.iter_mut().find(|node| node.name == "WidgetStyle" && node.attributes.get("name") == style.attributes.get("name")) {
                    node.attributes = style.attributes.clone();
                    node.text = style.keywords.clone();
                } else { root.children.push(style_node(style, "WidgetStyle")); }
            }
        }
        Ok(document.xml())
    }
}

#[derive(Clone, Default)]
pub(crate) struct StyleConfig {
    pub themes: Vec<ThemeEntry>,
    pub selected: Option<String>,
    pub global_override: GlobalOverride,
    pub revision: u64,
}

impl StyleConfig {
    pub fn new() -> Self {
        Self { themes: [false, true].into_iter().filter_map(|dark| ThemeEntry::frozen(dark).ok()).collect(), ..Self::default() }
    }

    pub fn load(store: Option<&ConfigStore>, dark: bool) -> io::Result<Self> {
        let mut config = Self::new();
        let mut user = Vec::new();
        if let Some(store) = store {
            let settings = match store.load_json(Path::new("settings.json")) { Ok(settings) => Some(settings), Err(error) if error.kind() == io::ErrorKind::NotFound => None, Err(error) => return Err(error) };
            if let Some(settings) = settings {
                config.selected = settings.get("styleTheme").and_then(|value| value.as_str()).map(str::to_owned);
                let flags = &settings["globalOverride"];
                for (index, key) in ["fg", "bg", "font", "fontSize", "bold", "italic", "underline"].iter().enumerate() {
                    if flags[*key].as_bool().unwrap_or(false) { config.global_override.toggle(index); }
                }
            }
            let default = store.root().join("stylers.xml");
            if default.is_file() { user.push(default); }
            user.extend(xml_files(&store.root().join("themes"))?);
        }
        let mut installed = Vec::new();
        if let Ok(executable) = std::env::current_exe() && let Some(directory) = executable.parent() {
            installed.extend(xml_files(&directory.join("themes"))?);
            if let Some(contents) = directory.parent() { installed.extend(xml_files(&contents.join("Resources/themes"))?); }
        }
        let mut entries = Vec::<ThemeEntry>::new();
        for path in user.into_iter().chain(installed) {
            let basename = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| invalid("Invalid theme filename"))?.to_owned();
            let filename = if store.is_some_and(|store| path == store.root().join("stylers.xml")) { basename } else { format!("themes/{basename}") };
            let name = Path::new(&filename).file_stem().and_then(|name| name.to_str()).unwrap_or("");
            if entries.iter().any(|entry| entry.filename == filename || entry.name == name) { continue; }
            entries.push(ThemeEntry::registered(filename, path)?);
        }
        for entry in &config.themes {
            if !entries.iter().any(|existing| existing.filename == entry.filename || existing.name == entry.name) { entries.push(entry.clone()); }
        }
        if let Some(index) = entries.iter().position(|entry| entry.filename == "stylers.xml") { let entry = entries.remove(index); entries.insert(0, entry); }
        config.themes = entries;
        if config.selected.as_ref().is_some_and(|selected| !config.themes.iter().any(|entry| &entry.filename == selected)) { config.selected = None; }
        let index = config.selected_index(dark);
        if let Some(entry) = config.themes.get(index) { config.themes[index] = entry.reload(store)?; }
        Ok(config)
    }

    pub fn selected_index(&self, dark: bool) -> usize {
        let filename = self.selected.as_deref().unwrap_or(if dark { "themes/DarkModeDefault.xml" } else { "stylers.xml" });
        self.themes.iter().position(|entry| entry.filename == filename).unwrap_or(0)
    }

    pub fn theme(&self, dark: bool) -> Option<&ThemeDefinition> { self.themes.get(self.selected_index(dark)).and_then(|entry| entry.definition.as_deref()) }

    pub fn theme_mut(&mut self, dark: bool) -> Option<&mut ThemeDefinition> {
        let index = self.selected_index(dark);
        self.themes.get_mut(index).and_then(|entry| entry.definition.as_mut()).map(Arc::make_mut)
    }

    pub fn global_style(&self, id: u16, dark: bool) -> Option<&StyleProfile> { self.theme(dark)?.global_styles.iter().find(|style| style.id == id) }

    pub fn global_style_named(&self, name: &str, dark: bool) -> Option<&StyleProfile> { self.theme(dark)?.global_styles.iter().find(|style| style.attributes.get("name").is_some_and(|value| value == name)) }

    pub fn effective_global_style(&self, id: u16, dark: bool) -> Option<StyleProfile> {
        let mut style = self.global_style(id, dark)?.clone();
        if matches!(id, 32 | 34 | 35 | 37) && let Some(global) = self.global_style_named("Global override", dark) { self.global_override.apply(&mut style, global); }
        Some(style)
    }

    pub fn appearance(&self, language: Option<&LanguageProfile>, id: u16, dark: bool, base: &crate::syntax::StyleAppearance, zoom_delta: i32) -> crate::syntax::StyleAppearance {
        let mut appearance = base.clone();
        let apply = |appearance: &mut crate::syntax::StyleAppearance, style: &StyleProfile| {
            let mask = number(style, "colorStyle", 3);
            for (key, bit, target) in [("fgColor", 1, &mut appearance.foreground), ("bgColor", 2, &mut appearance.background)] {
                if mask & bit != 0 && let Some(color) = style.attributes.get(key).and_then(|value| u32::from_str_radix(value, 16).ok()).filter(|color| *color <= 0xffffff) { *target = color; }
            }
            if let Some(name) = style.attributes.get("fontName").filter(|name| !name.is_empty()) { appearance.font_family = name.clone(); }
            let size = number(style, "fontSize", 0);
            if size > 0 { appearance.font_size = size.saturating_add(zoom_delta).max(1) as u32; }
            let bits = number(style, "fontStyle", -1);
            if bits != -1 { appearance.bold = bits & 1 != 0; appearance.italic = bits & 2 != 0; appearance.underline = bits & 4 != 0; }
        };
        if let Some(default) = self.effective_global_style(32, dark) { apply(&mut appearance, &default); }
        let style = match language { Some(language) => self.effective_style(language, id, dark), None => self.effective_global_style(id, dark) };
        if let Some(style) = style { apply(&mut appearance, &style); }
        appearance
    }

    pub fn effective_style(&self, language: &LanguageProfile, id: u16, dark: bool) -> Option<StyleProfile> {
        let theme = self.theme(dark)?;
        let source = language.effective_style_sources.iter().rev().filter_map(|name| theme.lexers.get(name)).find_map(|lexer| lexer.styles.iter().find(|style| style.id == id)).or_else(|| language.style(id));
        let mut style = source.cloned().or_else(|| self.global_style(32, dark).cloned())?;
        if let Some(global) = self.global_style_named("Global override", dark) { self.global_override.apply(&mut style, global); }
        Some(style)
    }

    pub fn effective_language(&self, language: &LanguageProfile, dark: bool) -> LanguageProfile {
        let mut profile = language.clone();
        for style in &mut profile.styles {
            if let Some(effective) = self.effective_style(language, style.id, dark) { *style = effective; }
        }
        if let Some(theme) = self.theme(dark) {
            for name in &language.effective_style_sources {
                if let Some(lexer) = theme.lexers.get(name) {
                    for style in &lexer.styles {
                        if let Some(index) = style.attributes.get("keywordClass").and_then(|class| keyword_index(class)) {
                            if !style.keywords.is_empty() {
                                profile.numbered_keywords.resize(profile.numbered_keywords.len().max(index + 1), String::new());
                                let words = &mut profile.numbered_keywords[index];
                                words.push(' '); words.push_str(&style.keywords);
                                if let Some(class) = style.attributes.get("keywordClass") {
                                    let words = profile.keywords.entry(class.clone()).or_default();
                                    words.push(' '); words.push_str(&style.keywords);
                                }
                            }
                        }
                    }
                    if let Some(ext) = lexer.attributes.get("ext") {
                        profile.extensions.extend(ext.split_whitespace().map(str::to_owned));
                    }
                }
            }
        }
        profile
    }

    pub fn user_extensions(&self, lexer: &str, dark: bool) -> &str { self.theme(dark).and_then(|theme| theme.lexers.get(lexer)).and_then(|lexer| lexer.attributes.get("ext")).map_or("", String::as_str) }

    pub fn language_for_path(&self, path: &Path, dark: bool) -> Option<u16> {
        let extension = path.extension()?.to_str()?;
        let catalog = languages::catalog().ok()?;
        let mut lexers = self.theme(dark)?.lexers.iter().collect::<Vec<_>>();
        lexers.sort_by(|(left_name, left), (right_name, right)| {
            let left = left.attributes.get("desc").unwrap_or(left_name);
            let right = right.attributes.get("desc").unwrap_or(right_name);
            match (left.as_str(), right.as_str()) { ("Search result", "Search result") => std::cmp::Ordering::Equal, ("Search result", _) => std::cmp::Ordering::Greater, (_, "Search result") => std::cmp::Ordering::Less, _ => left.cmp(right) }
        });
        let (name, _) = lexers.into_iter().find(|(_, lexer)| lexer.attributes.get("ext").is_some_and(|exts| exts.split_whitespace().any(|ext| ext.eq_ignore_ascii_case(extension))))?;
        catalog.profiles().iter().find(|language| !language.internal && language.name == *name).map(|language| language.id)
    }

    pub fn select(&mut self, index: usize, store: Option<&ConfigStore>) -> io::Result<()> {
        let entry = self.themes.get(index).ok_or_else(|| invalid("No selected theme"))?.reload(store)?;
        self.selected = Some(entry.filename.clone());
        self.themes[index] = entry;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn import(&mut self, path: &Path, store: &ConfigStore) -> io::Result<()> {
        let _access = crate::file_access::read(path)?;
        if !path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("xml")) { return Err(invalid("Choose a Notepad++ style theme XML file")); }
        let filename = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| invalid("Invalid theme filename"))?.to_owned();
        let bytes = fs::read(path)?;
        let directory = theme_directory(store)?;
        atomic_write(&directory.join(&filename), &bytes)?;
        let entry = ThemeEntry::registered(format!("themes/{filename}"), directory.join(&filename))?;
        if !self.themes.iter().any(|existing| existing.name == entry.name) { self.themes.push(entry); }
        Ok(())
    }

    pub fn save_theme(&self, store: &ConfigStore, dark: bool) -> io::Result<()> {
        let entry = self.themes.get(self.selected_index(dark)).ok_or_else(|| invalid("No selected theme"))?;
        let path = if entry.filename == "stylers.xml" {
            store.document_path(Path::new("style-config.json"))?;
            store.root().join("stylers.xml")
        } else { theme_directory(store)?; store.root().join(&entry.filename) };
        atomic_write(&path, entry.xml()?.as_bytes())
    }

    pub fn save(&self, store: &ConfigStore, dark: bool) -> io::Result<()> {
        self.save_theme(store, dark)?;
        let filename = self.themes.get(self.selected_index(dark)).ok_or_else(|| invalid("No selected theme"))?.filename.clone();
        store.update_settings(|settings| {
            settings["styleTheme"] = filename.into();
            let flags = &self.global_override;
            settings["globalOverride"] = serde_json::json!({ "fg": flags.foreground, "bg": flags.background, "font": flags.font, "fontSize": flags.font_size, "bold": flags.bold, "italic": flags.italic, "underline": flags.underline });
            Ok(())
        })?;
        Ok(())
    }
}

fn xml_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) { Ok(entries) => entries, Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()), Err(error) => return Err(error) };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("xml")) { files.push(path); }
    }
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(files)
}

fn theme_directory(store: &ConfigStore) -> io::Result<PathBuf> {
    store.document_path(Path::new("style-config.json"))?;
    let directory = store.root().join("themes");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {},
        Ok(_) => return Err(invalid("Theme directory must not be a symbolic link")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&directory)?,
        Err(error) => return Err(error),
    }
    Ok(directory)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) && (!metadata.is_file() || metadata.file_type().is_symlink()) { return Err(invalid("Theme destination must be a regular file")); }
    let parent = path.parent().ok_or_else(|| invalid("Theme destination has no directory"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
