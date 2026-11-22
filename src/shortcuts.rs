use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::config::ConfigStore;
use crate::menu::{MenuCommand, MenuDefinition, MenuEntry};
use crate::style_config::{Node, read_xml};

pub const MACRO_BASE: u32 = 20000;
pub const PAGES: [&str; 4] = ["Main menu", "Macros", "Plugin commands", "Scintilla commands"];
pub const DUAL_MODIFIERS_SETTING: &str = "ShortcutMapper.dualModifiers";

thread_local! {
    static KEY_MAPPING: RefCell<Option<KeyMapping>> = const { RefCell::new(None) };
}

struct KeyMapping {
    names: BTreeMap<u8, String>,
    native: BTreeMap<u8, Option<String>>,
    #[cfg(all(feature = "native-ui", target_os = "macos"))]
    layout: crate::ui::shortcut_layout::Snapshot,
}

impl KeyMapping {
    fn new() -> Self {
        Self {
            names: BTreeMap::new(),
            native: BTreeMap::new(),
            #[cfg(all(feature = "native-ui", target_os = "macos"))]
            layout: crate::ui::shortcut_layout::Snapshot::new(),
        }
    }

    fn key_name(&mut self, key: u8) -> String {
        if let Some(name) = self.names.get(&key) { return name.clone(); }
        #[cfg(all(feature = "native-ui", target_os = "macos"))]
        let name = self.layout.key_name(key).unwrap_or_else(|| source_key_name(key));
        #[cfg(not(all(feature = "native-ui", target_os = "macos")))]
        let name = source_key_name(key);
        self.names.insert(key, name.clone());
        name
    }

    fn native_key(&mut self, key: u8) -> Option<String> {
        if let Some(value) = self.native.get(&key) { return value.clone(); }
        let value = native_key_with_name(key, |key| self.key_name(key));
        self.native.insert(key, value.clone());
        value
    }
}

pub(crate) struct KeyMappingScope {
    previous: Option<KeyMapping>,
}

impl KeyMappingScope {
    pub(crate) fn new() -> Self {
        let mapping = KeyMapping::new();
        Self { previous: KEY_MAPPING.with(|slot| slot.replace(Some(mapping))) }
    }
}

impl Drop for KeyMappingScope {
    fn drop(&mut self) {
        KEY_MAPPING.with(|slot| { slot.replace(self.previous.take()); });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandKey {
    Menu { id: u32, nth: u32 },
    Macro(usize),
    Plugin { module: String, internal_id: u32 },
    Scintilla { id: u32, menu_id: u32 },
}

impl CommandKey {
    pub fn page(&self) -> usize {
        match self { Self::Menu { .. } => 0, Self::Macro(_) => 1, Self::Plugin { .. } => 2, Self::Scintilla { .. } => 3 }
    }

    pub fn command_id(&self) -> Option<u32> {
        match self {
            Self::Menu { id, .. } => Some(*id),
            Self::Macro(index) => u32::try_from(*index).ok().and_then(|index| MACRO_BASE.checked_add(index)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
    pub key: u8,
}

impl KeyCombo {
    pub fn valid(self) -> bool {
        self.key == 0 || !matches!(self.key, b'A'..=b'Z' | b'0'..=b'9' | 8 | 13 | 20 | 32) || self.ctrl || self.alt || self.cmd
    }

    pub fn label(self) -> String {
        if self.key == 0 { return String::new(); }
        let mut parts = Vec::new();
        if self.ctrl { parts.push("Ctrl".to_owned()); }
        if self.alt { parts.push("Alt".to_owned()); }
        if self.shift { parts.push("Shift".to_owned()); }
        if self.cmd { parts.push("Cmd".to_owned()); }
        parts.push(key_name(self.key));
        parts.join("+")
    }

    pub fn keystroke(self) -> Option<String> {
        let key = native_key(self.key)?;
        let mut value = String::new();
        for (enabled, modifier) in [(self.ctrl, "ctrl-"), (self.alt, "alt-"), (self.shift, "shift-"), (self.cmd, "cmd-")] {
            if enabled { value.push_str(modifier); }
        }
        value.push_str(&key);
        Some(value)
    }

    pub fn needs_keypad_adapter(self) -> bool { matches!(self.key, 96..=107 | 109..=111) }

    pub fn needs_native_adapter(self) -> bool { self.needs_keypad_adapter() || matches!(self.key, 186..=192 | 219..=222 | 226) }

    pub fn builtin_alias(self) -> Option<Self> {
        if self.key == 0 || self.ctrl == self.cmd || (!self.alt && !self.shift && matches!(self.key, b'Q' | b'M')) { return None; }
        Some(Self { ctrl: self.cmd, cmd: self.ctrl, ..self })
    }

    pub fn conflicts_with(self, other: Self) -> bool {
        if self.key == 0 || other.key == 0 || (self.ctrl, self.alt, self.shift, self.cmd) != (other.ctrl, other.alt, other.shift, other.cmd) { return false; }
        if self.key == other.key { return true; }
        let Some(left) = native_key(self.key) else { return false; };
        native_key(other.key).is_some_and(|right| left == right)
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::parse_with_names(value, key_name)
    }

    pub fn parse_source(value: &str) -> Option<Self> {
        Self::parse_with_names(value, source_key_name)
    }

    fn parse_with_names(value: &str, name: fn(u8) -> String) -> Option<Self> {
        if value.is_empty() || value.eq_ignore_ascii_case("none") { return Some(Self::default()); }
        let mut combo = Self::default();
        let mut rest = value;
        loop {
            let lower = rest.to_ascii_lowercase();
            let mut consumed = false;
            for (name, flag) in [("ctrl", &mut combo.ctrl), ("alt", &mut combo.alt), ("shift", &mut combo.shift), ("cmd", &mut combo.cmd)] {
                if lower.starts_with(&format!("{name}+")) || lower.starts_with(&format!("{name}-")) {
                    *flag = true;
                    rest = &rest[name.len() + 1..];
                    consumed = true;
                    break;
                }
            }
            if !consumed { break; }
        }
        let key = key_choices().into_iter().find(|key| name(*key).eq_ignore_ascii_case(rest)).or_else(|| match rest.to_ascii_lowercase().as_str() { "+" => Some(107), "*" => Some(106), "`" if name(192) == "~" => Some(192), "<" if name(226) == "<>" => Some(226), "escape" => Some(27), "space" => Some(32), "pageup" => Some(33), "pagedown" => Some(34), "insert" => Some(45), "delete" => Some(46), _ => None })?;
        combo.key = key;
        Some(combo)
    }

    fn read(node: &Node) -> io::Result<Self> {
        let flag = |name| match node.attribute(name) { Some("yes") => Ok(true), Some("no") => Ok(false), _ => Err(invalid(format!("Invalid shortcut {name}"))) };
        Ok(Self { ctrl: flag("Ctrl")?, alt: flag("Alt")?, shift: flag("Shift")?, cmd: if node.attribute("Cmd").is_some() { flag("Cmd")? } else { false }, key: number(node, "Key")?.try_into().map_err(|_| invalid("Shortcut Key exceeds 255"))? })
    }

    fn write(self, node: &mut Node) {
        for (name, enabled) in [("Ctrl", self.ctrl), ("Alt", self.alt), ("Shift", self.shift), ("Cmd", self.cmd)] {
            node.attributes.insert(name.into(), if enabled { "yes" } else { "no" }.into());
        }
        node.attributes.insert("Key".into(), self.key.to_string());
    }
}

pub fn key_choices() -> Vec<u8> {
    let mut keys = vec![0, 8, 9, 13, 27, 32, 33, 34, 35, 36, 37, 38, 39, 40, 45, 46];
    keys.extend(b'0'..=b'9');
    keys.extend(b'A'..=b'Z');
    keys.extend(96..=105);
    keys.extend([106, 107, 109, 110, 111]);
    keys.extend(112..=135);
    keys.extend([192, 189, 187, 219, 221, 186, 222, 220, 188, 190, 191, 226]);
    keys
}

pub fn key_name(key: u8) -> String {
    if let Some(name) = KEY_MAPPING.with(|slot| slot.borrow_mut().as_mut().map(|mapping| mapping.key_name(key))) { return name; }
    #[cfg(all(feature = "native-ui", target_os = "macos"))]
    if let Some(name) = crate::ui::shortcut_layout::key_name(key) { return name; }
    source_key_name(key)
}

fn source_key_name(key: u8) -> String {
    match key {
        0 => "None".into(), 8 => "Backspace".into(), 9 => "Tab".into(), 13 => "Enter".into(), 27 => "Esc".into(), 32 => "Spacebar".into(),
        33 => "Page up".into(), 34 => "Page down".into(), 35 => "End".into(), 36 => "Home".into(), 37 => "Left".into(), 38 => "Up".into(), 39 => "Right".into(), 40 => "Down".into(), 45 => "INS".into(), 46 => "DEL".into(),
        b'0'..=b'9' | b'A'..=b'Z' => char::from(key).to_string(), 96..=105 => format!("Numpad {}", key - 96),
        106 => "Num *".into(), 107 => "Num +".into(), 109 => "Num -".into(), 110 => "Num .".into(), 111 => "Num /".into(), 112..=135 => format!("F{}", key - 111),
        186 => ";".into(), 187 => "=".into(), 188 => ",".into(), 189 => "-".into(), 190 => ".".into(), 191 => "/".into(), 192 => "~".into(), 219 => "[".into(), 220 => "\\".into(), 221 => "]".into(), 222 => "'".into(), 226 => "<>".into(),
        _ => format!("Key {key}"),
    }
}

fn native_key(key: u8) -> Option<String> {
    if let Some(value) = KEY_MAPPING.with(|slot| slot.borrow_mut().as_mut().map(|mapping| mapping.native_key(key))) { return value; }
    native_key_with_name(key, key_name)
}

fn native_key_with_name(key: u8, name: impl FnOnce(u8) -> String) -> Option<String> {
    #[cfg(not(all(feature = "native-ui", target_os = "macos")))]
    match key { 192 => return Some("`".into()), 226 => return Some("<".into()), _ => {} }
    Some(match key {
        0 => return None, 8 => "backspace".into(), 9 => "tab".into(), 13 => "enter".into(), 27 => "escape".into(), 32 => "space".into(), 33 => "pageup".into(), 34 => "pagedown".into(),
        35 => "end".into(), 36 => "home".into(), 37 => "left".into(), 38 => "up".into(), 39 => "right".into(), 40 => "down".into(), 45 => "insert".into(), 46 => "delete".into(),
        b'0'..=b'9' | b'A'..=b'Z' => char::from(key).to_ascii_lowercase().to_string(),
        96..=107 | 109..=111 => return None, 112..=135 => format!("f{}", key - 111),
        186..=192 | 219..=222 | 226 => name(key), _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Shortcut {
    pub key: CommandKey,
    pub name: String,
    pub category: String,
    pub bindings: Vec<KeyCombo>,
    pub command: Option<MenuCommand>,
}

impl Shortcut {
    pub fn display_name(&self) -> String {
        let mut name = String::new();
        let mut characters = self.name.chars().peekable();
        while let Some(character) = characters.next() {
            if character != '&' { name.push(character); }
            else if characters.next_if_eq(&'&').is_some() { name.push('&'); }
        }
        name
    }

    pub fn label(&self) -> String { self.bindings.iter().filter(|key| key.key != 0).map(|key| key.label()).collect::<Vec<_>>().join(" / ") }
    pub fn localized_name(&self, locale: &str) -> String {
        if let CommandKey::Menu { id, .. } = self.key {
            if let Some(name) = crate::menu::product_command_label(id, locale) { return name.to_owned(); }
            if let Some(name) = crate::localization::dialog_control(locale, "ShortcutMapper/MainCommandNames", id).or_else(|| crate::localization::menu_command(locale, id)) {
                return name.to_owned();
            }
        }
        self.display_name()
    }

    pub fn matches(&self, filter: &str) -> bool {
        self.matches_name(filter, &self.display_name())
    }

    pub fn matches_localized(&self, filter: &str, locale: &str) -> bool {
        self.matches_name(filter, &self.localized_name(locale))
    }

    fn matches_name(&self, filter: &str, name: &str) -> bool {
        let name = name.to_lowercase();
        let keys = self.label().to_lowercase();
        let module = if self.key.page() == 2 { self.category.to_lowercase() } else { String::new() };
        filter.to_lowercase().split(' ').filter(|word| !word.is_empty()).all(|word| name.contains(word) || keys.contains(word) || module.contains(word))
    }
}

#[derive(Clone, Debug)]
pub struct BindingSet {
    pub shortcuts: Vec<Shortcut>,
    pub dual_modifiers: bool,
    overrides: BTreeMap<CommandKey, Vec<KeyCombo>>,
    document: Node,
}

impl Default for BindingSet {
    fn default() -> Self { Self { shortcuts: Vec::new(), dual_modifiers: true, overrides: BTreeMap::new(), document: Node { name: "NotepadPlus".into(), ..Node::default() } } }
}

impl BindingSet {
    pub fn load(store: &ConfigStore, mut catalog: Vec<Shortcut>, macro_names: &[String]) -> io::Result<Self> {
        let lock = config_write_lock(store)?;
        let path = store.root().join("shortcuts.xml");
        let document = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => read_xml(&path)?,
            Ok(_) => return Err(invalid("Private shortcuts.xml must be a regular file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default().document,
            Err(error) => return Err(error),
        };
        if document.name != "NotepadPlus" { return Err(invalid("shortcuts.xml requires NotepadPlus")); }
        let mut set = Self { document, ..Self::default() };
        let previous_sections = set.document.children.len();
        set.document.children.retain(|node| node.name != "UserDefinedCommands");
        let mut cleaned = set.document.children.len() != previous_sections;
        for root in set.document.children.iter_mut().filter(|node| node.name == "InternalCommands") {
            let previous_shortcuts = root.children.len();
            root.children.retain(|node| node.name != "Shortcut" || !number(node, "id").ok().is_some_and(|id| id == 48017 || (21000..22000).contains(&id)));
            cleaned |= root.children.len() != previous_shortcuts;
        }
        if cleaned { set.save_locked(store)?; }
        drop(lock);
        for (section, tag) in [("InternalCommands", "Shortcut"), ("PluginCommands", "PluginCommand"), ("ScintillaKeys", "ScintKey")] {
            if let Some(root) = set.document.child(section) {
                for node in root.children.iter().filter(|node| node.name == tag) {
                    let key = match section {
                        "InternalCommands" => CommandKey::Menu { id: number(node, "id")?, nth: node.attribute("nth").map(str::parse).transpose().map_err(|_| invalid("Invalid shortcut nth"))?.unwrap_or(0) },
                        "PluginCommands" => CommandKey::Plugin { module: module_name(node.attribute("moduleName").ok_or_else(|| invalid("Missing plugin moduleName"))?), internal_id: number(node, "internalID")? },
                        _ => CommandKey::Scintilla { id: number(node, "ScintID")?, menu_id: number(node, "menuCmdID")? },
                    };
                    let mut bindings = vec![KeyCombo::read(node)?];
                    if key.page() == 3 { for next in node.children.iter().filter(|node| node.name == "NextKey") { bindings.push(KeyCombo::read(next)?); } }
                    dedup(&mut bindings);
                    set.overrides.insert(key, bindings);
                }
            }
        }
        let macros = set.document.child("Macros").cloned();
        let mut macro_nodes = Vec::new();
        let mut used = BTreeSet::new();
        for (index, name) in macro_names.iter().enumerate() {
            let node = macros.as_ref().and_then(|root| root.children.iter().enumerate().find(|(ordinal, node)| node.name == "Macro" && node.attribute("name") == Some(name.as_str()) && !used.contains(ordinal)));
            let bindings = if let Some((ordinal, node)) = node { used.insert(ordinal); vec![KeyCombo::read(node)?] } else { Vec::new() };
            let mut stored = node.map(|(_, node)| node.clone()).unwrap_or_else(|| Node { name: "Macro".into(), ..Node::default() });
            stored.attributes.insert("name".into(), name.clone());
            bindings.first().copied().unwrap_or_default().write(&mut stored);
            macro_nodes.push(stored);
            set.overrides.insert(CommandKey::Macro(index), bindings.clone());
            catalog.push(Shortcut { key: CommandKey::Macro(index), name: name.clone(), category: String::new(), bindings, command: Some(MenuCommand::SavedMacro(index)) });
        }
        if !macro_names.is_empty() {
            if let Some(root) = macros { macro_nodes.extend(root.children.into_iter().enumerate().filter(|(ordinal, _)| !used.contains(ordinal)).map(|(_, node)| node)); }
            section_mut(&mut set.document, "Macros").children = macro_nodes.into();
        }
        for item in &mut catalog { if let Some(keys) = set.overrides.get(&item.key) { item.bindings = keys.clone(); } }
        set.shortcuts = catalog;
        Ok(set)
    }

    pub fn override_for_command(&self, id: u32) -> Option<&[KeyCombo]> {
        self.overrides.get(&CommandKey::Menu { id, nth: 0 }).or_else(|| {
            if (MACRO_BASE..MACRO_BASE + 1000).contains(&id) { self.overrides.get(&CommandKey::Macro((id - MACRO_BASE) as usize)) } else { None }
        }).or_else(|| self.overrides.iter().find_map(|(key, bindings)| match key { CommandKey::Scintilla { menu_id, .. } if *menu_id != 0 && *menu_id == id => Some(bindings), _ => None })).map(Vec::as_slice)
    }

    pub fn override_for(&self, key: &CommandKey) -> Option<&[KeyCombo]> { self.overrides.get(key).map(Vec::as_slice) }

    fn reserved_bindings(&self) -> impl Iterator<Item = KeyCombo> + '_ {
        self.shortcuts.iter().filter_map(|item| self.override_for(&item.key)).flatten().copied().filter(|combo| combo.key != 0)
    }

    fn runtime_alias_reserved(&self, combo: KeyCombo) -> bool {
        self.reserved_bindings().any(|reserved| reserved.conflicts_with(combo))
    }

    pub fn runtime_binding_priority(&self, item: &Shortcut, combo: KeyCombo) -> Option<u8> {
        let overridden = self.override_for(&item.key).is_some();
        if item.bindings.contains(&combo) { return Some(if overridden { 0 } else { 1 }); }
        if !self.dual_modifiers || overridden || matches!(item.key, CommandKey::Macro(_)) || self.runtime_alias_reserved(combo) { return None; }
        item.bindings.iter().any(|source| source.builtin_alias() == Some(combo)).then_some(2)
    }

    pub fn conflicts(&self, excluded: &CommandKey, bindings: &[KeyCombo]) -> Vec<&Shortcut> {
        self.shortcuts.iter().filter(|item| &item.key != excluded && item.bindings.iter().any(|other| bindings.iter().any(|candidate| candidate.conflicts_with(*other)))).collect()
    }

    pub fn modify(&mut self, key: &CommandKey, name: &str, mut bindings: Vec<KeyCombo>) -> io::Result<()> {
        if key.page() != 3 && bindings.len() > 1 { return Err(invalid("Only Scintilla commands accept multiple shortcuts")); }
        let previous = self.shortcuts.iter().find(|item| &item.key == key).ok_or_else(|| invalid("Shortcut command is no longer available"))?;
        if bindings.iter().any(|combo| !combo.valid() && (key.page() != 3 || !previous.bindings.contains(combo))) { return Err(invalid("This key requires Ctrl, Alt or Cmd")); }
        if matches!(key, CommandKey::Macro(_)) && (name.is_empty() || name.chars().count() >= 128) { return Err(invalid("Name must contain 1 to 127 characters")); }
        dedup(&mut bindings);
        let item = self.shortcuts.iter_mut().find(|item| &item.key == key).ok_or_else(|| invalid("Shortcut command is no longer available"))?;
        let previous_name = item.name.clone();
        if matches!(key, CommandKey::Macro(_)) { item.name = name.into(); }
        item.bindings = bindings.clone();
        self.overrides.insert(key.clone(), bindings.clone());
        let (section, tag) = match key { CommandKey::Menu { .. } => ("InternalCommands", "Shortcut"), CommandKey::Macro(_) => ("Macros", "Macro"), CommandKey::Plugin { .. } => ("PluginCommands", "PluginCommand"), CommandKey::Scintilla { .. } => ("ScintillaKeys", "ScintKey") };
        let duplicate = match key { CommandKey::Macro(index) => self.shortcuts.iter().filter(|item| matches!(&item.key, CommandKey::Macro(other) if other < index) && item.name == previous_name).count(), _ => 0 };
        let root = section_mut(&mut self.document, section);
        let ordinal = match key {
            CommandKey::Macro(_) => root.children.iter().enumerate().filter(|(_, node)| node.name == tag && node.attribute("name") == Some(previous_name.as_str())).nth(duplicate).map(|(index, _)| index),
            _ => root.children.iter().position(|node| node_matches(node, key)),
        };
        let ordinal = ordinal.unwrap_or_else(|| { root.children.push(Node { name: tag.into(), ..Node::default() }); root.children.len() - 1 });
        let node = &mut root.children[ordinal];
        match key {
            CommandKey::Menu { id, nth } => { node.attributes.insert("id".into(), id.to_string()); if *nth != 0 { node.attributes.insert("nth".into(), nth.to_string()); } }
            CommandKey::Macro(_) => { node.attributes.insert("name".into(), name.into()); }
            CommandKey::Plugin { module, internal_id } => { node.attributes.entry("moduleName".into()).or_insert_with(|| module.clone()); node.attributes.insert("internalID".into(), internal_id.to_string()); }
            CommandKey::Scintilla { id, menu_id } => { node.attributes.insert("ScintID".into(), id.to_string()); node.attributes.insert("menuCmdID".into(), menu_id.to_string()); }
        }
        bindings.first().copied().unwrap_or_default().write(node);
        if key.page() == 3 {
            node.children.retain(|child| child.name != "NextKey");
            for combo in bindings.iter().skip(1) { let mut next = Node { name: "NextKey".into(), ..Node::default() }; combo.write(&mut next); node.children.push(next); }
        }
        Ok(())
    }

    pub fn delete(&mut self, key: &CommandKey) -> io::Result<()> {
        let index = match key { CommandKey::Macro(index) => *index, _ => return Err(invalid("Only macros can be deleted")) };
        let item = self.shortcuts.iter().find(|item| &item.key == key).ok_or_else(|| invalid("Shortcut command is no longer available"))?;
        let name = item.name.clone();
        let duplicate = self.shortcuts.iter().filter(|item| item.key.page() == key.page() && item.key < *key && item.name == name).count();
        let root = section_mut(&mut self.document, "Macros");
        let ordinal = root.children.iter().enumerate().filter(|(_, node)| node.name == "Macro" && node.attribute("name") == Some(name.as_str())).nth(duplicate).map(|(ordinal, _)| ordinal);
        if let Some(ordinal) = ordinal { root.children.remove(ordinal); }
        self.shortcuts.retain(|item| &item.key != key);
        self.overrides.remove(key);
        let mut remapped = BTreeMap::new();
        for (mut other, bindings) in std::mem::take(&mut self.overrides) { renumber(&mut other, key.page(), index); remapped.insert(other, bindings); }
        self.overrides = remapped;
        for item in &mut self.shortcuts { renumber(&mut item.key, key.page(), index); if let CommandKey::Macro(index) = item.key { item.command = Some(MenuCommand::SavedMacro(index)); } }
        Ok(())
    }

    pub fn save(&self, store: &ConfigStore) -> io::Result<()> {
        let _lock = config_write_lock(store)?;
        self.save_locked(store)
    }

    fn save_locked(&self, store: &ConfigStore) -> io::Result<()> {
        let root = store.document_path(Path::new("settings.json"))?.parent().ok_or_else(|| invalid("Missing private configuration root"))?.to_path_buf();
        let path = root.join("shortcuts.xml");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {},
            Ok(_) => return Err(invalid("Private shortcuts.xml must be a regular file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(self.document.xml().as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

fn config_write_lock(store: &ConfigStore) -> io::Result<fs::File> {
    let root = store.document_path(Path::new("settings.json"))?.parent().ok_or_else(|| invalid("Missing private configuration root"))?.to_path_buf();
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let lock = options.open(root.join(".config-write.lock"))?;
    lock.lock()?;
    Ok(lock)
}

fn renumber(key: &mut CommandKey, page: usize, removed: usize) {
    match key { CommandKey::Macro(index) if page == 1 && *index > removed => *index -= 1, _ => {} }
}

fn dedup(bindings: &mut Vec<KeyCombo>) {
    let mut used = BTreeSet::new();
    bindings.retain(|key| key.key != 0 && used.insert(*key));
}

fn section_mut<'a>(document: &'a mut Node, name: &str) -> &'a mut Node {
    let index = document.children.iter().position(|node| node.name == name).unwrap_or_else(|| { document.children.push(Node { name: name.into(), ..Node::default() }); document.children.len() - 1 });
    &mut document.children[index]
}

fn number(node: &Node, name: &str) -> io::Result<u32> { node.attribute(name).and_then(|value| value.parse().ok()).ok_or_else(|| invalid(format!("Invalid shortcut {name}"))) }
fn invalid(message: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message.into()) }

fn node_matches(node: &Node, key: &CommandKey) -> bool {
    match key {
        CommandKey::Menu { id, nth } => node.name == "Shortcut" && number(node, "id").ok() == Some(*id) && node.attribute("nth").and_then(|value| value.parse().ok()).unwrap_or(0) == *nth,
        CommandKey::Plugin { module, internal_id } => node.name == "PluginCommand" && node.attribute("moduleName").is_some_and(|name| module_name(name).eq_ignore_ascii_case(module)) && number(node, "internalID").ok() == Some(*internal_id),
        CommandKey::Scintilla { id, menu_id } => node.name == "ScintKey" && number(node, "ScintID").ok() == Some(*id) && number(node, "menuCmdID").ok() == Some(*menu_id),
        _ => false,
    }
}

fn module_name(name: &str) -> String {
    let name = if name.to_ascii_lowercase().ends_with(".dll") { &name[..name.len() - 4] } else { name };
    for canonical in ["mimeTools", "Converter", "NppExport"] { if name.eq_ignore_ascii_case(canonical) { return canonical.into(); } }
    name.into()
}

pub fn menu_catalog(definitions: &[MenuDefinition]) -> Vec<Shortcut> {
    const SOURCE_DEFAULTS: &[(u32, u32, &str)] = &[
        (41001, 0, "ctrl-n"), (41002, 0, "ctrl-o"), (41014, 0, "ctrl-r"), (41006, 0, "ctrl-s"),
        (41008, 0, "ctrl-alt-s"), (41007, 0, "ctrl-shift-s"), (41003, 0, "ctrl-w"), (41004, 0, "ctrl-shift-w"),
        (41010, 0, "ctrl-p"), (41021, 0, "ctrl-shift-t"), (41011, 0, "alt-f4"), (42001, 0, "shift-delete"),
        (42001, 1, "ctrl-x"), (42002, 0, "ctrl-insert"), (42002, 1, "ctrl-c"), (42005, 0, "shift-insert"),
        (42005, 1, "ctrl-v"), (42020, 0, "ctrl-shift-b"), (42089, 0, "alt-shift-b"), (42016, 0, "ctrl-shift-u"),
        (42017, 0, "ctrl-u"), (42067, 0, "alt-u"), (42068, 0, "alt-shift-u"), (42069, 0, "ctrl-alt-u"),
        (42070, 0, "ctrl-alt-shift-u"), (42012, 0, "ctrl-i"), (42013, 0, "ctrl-j"), (42014, 0, "ctrl-shift-up"),
        (42015, 0, "ctrl-shift-down"), (42057, 0, "ctrl-alt-enter"), (42058, 0, "ctrl-alt-shift-enter"), (42022, 0, "ctrl-q"),
        (42035, 0, "ctrl-k"), (42036, 0, "ctrl-shift-k"), (42023, 0, "ctrl-shift-q"), (50000, 0, "ctrl-space"),
        (50006, 0, "ctrl-alt-space"), (50001, 0, "ctrl-enter"), (50002, 0, "ctrl-shift-space"), (50010, 0, "alt-up"),
        (50011, 0, "alt-down"), (42034, 0, "alt-c"), (43001, 0, "ctrl-f"), (43013, 0, "ctrl-shift-f"),
        (43002, 0, "f3"), (43010, 0, "shift-f3"), (43048, 0, "ctrl-f3"), (43049, 0, "ctrl-shift-f3"),
        (43014, 0, "ctrl-alt-f3"), (43015, 0, "ctrl-alt-shift-f3"), (43003, 0, "ctrl-h"), (43011, 0, "ctrl-alt-i"),
        (43045, 0, "f7"), (43047, 0, "shift-f4"), (43046, 0, "f4"), (43004, 0, "ctrl-g"),
        (43009, 0, "ctrl-b"), (43053, 0, "ctrl-alt-b"), (43054, 0, "ctrl-m"), (43033, 0, "ctrl-shift-1"),
        (43034, 0, "ctrl-shift-2"), (43035, 0, "ctrl-shift-3"), (43036, 0, "ctrl-shift-4"), (43037, 0, "ctrl-shift-5"),
        (43038, 0, "ctrl-shift-0"), (43039, 0, "ctrl-1"), (43040, 0, "ctrl-2"), (43041, 0, "ctrl-3"),
        (43042, 0, "ctrl-4"), (43043, 0, "ctrl-5"), (43044, 0, "ctrl-0"), (43005, 0, "ctrl-f2"),
        (43006, 0, "f2"), (43007, 0, "shift-f2"), (44032, 0, "f11"), (44009, 0, "f12"),
        (44086, 0, "ctrl-Numpad 1"), (44087, 0, "ctrl-Numpad 2"), (44088, 0, "ctrl-Numpad 3"), (44089, 0, "ctrl-Numpad 4"),
        (44090, 0, "ctrl-Numpad 5"), (44091, 0, "ctrl-Numpad 6"), (44092, 0, "ctrl-Numpad 7"), (44093, 0, "ctrl-Numpad 8"),
        (44094, 0, "ctrl-Numpad 9"), (44095, 0, "ctrl-pagedown"), (44096, 0, "ctrl-pageup"), (44098, 0, "ctrl-shift-pagedown"),
        (44099, 0, "ctrl-shift-pageup"), (50003, 0, "ctrl-shift-tab"), (50004, 0, "ctrl-tab"), (44042, 0, "alt-h"),
        (44072, 0, "f8"), (44010, 0, "alt-0"), (44029, 0, "alt-shift-0"), (44030, 0, "ctrl-alt-f"),
        (44031, 0, "ctrl-alt-shift-f"), (44051, 0, "alt-1"), (44052, 0, "alt-2"), (44053, 0, "alt-3"),
        (44054, 0, "alt-4"), (44055, 0, "alt-5"), (44056, 0, "alt-6"), (44057, 0, "alt-7"),
        (44058, 0, "alt-8"), (44061, 0, "alt-shift-1"), (44062, 0, "alt-shift-2"), (44063, 0, "alt-shift-3"),
        (44064, 0, "alt-shift-4"), (44065, 0, "alt-shift-5"), (44066, 0, "alt-shift-6"), (44067, 0, "alt-shift-7"),
        (44068, 0, "alt-shift-8"), (42026, 0, "ctrl-alt-r"), (42027, 0, "ctrl-alt-l"), (50005, 0, "ctrl-shift-r"),
        (42021, 0, "ctrl-shift-p"), (49000, 0, "f5"), (47000, 0, "f1"),
    ];
    const SOURCE_ORDER: &[u32] = &[
        41001, 41002, 41019, 41020, 41027, 41023, 41022, 41014, 41006, 41008, 41015, 41007, 41017, 41003, 41004, 41005,
        41026, 41009, 41018, 41024, 41016, 41012, 41013, 41010, 1001, 41021, 41011, 42001, 42002, 42005, 42020, 42089,
        42029, 42030, 42031, 42087, 42088, 42008, 42009, 42016, 42017, 42067, 42068, 42069, 42070, 42071, 42072, 42077,
        42079, 42012, 42013, 42014, 42015, 42055, 42056, 42057, 42058, 42104, 42105, 42059, 42060, 42080, 42081, 42100,
        42101, 42061, 42062, 42063, 42064, 42065, 42066, 42083, 42078, 42022, 42035, 42036, 42023, 42047, 50000, 50006,
        50001, 50002, 50010, 50011, 42084, 42085, 42086, 45001, 45002, 45003, 42024, 42042, 42043, 42044, 42045, 42046,
        42054, 42053, 42038, 42039, 42048, 42049, 42050, 42073, 42074, 42075, 42076, 42090, 42091, 42092, 42093, 42094,
        42095, 42096, 42097, 42098, 42099, 42034, 42051, 42052, 42028, 42102, 42103, 42033, 43001, 43013, 43002, 43010,
        43048, 43049, 43014, 43015, 43003, 43011, 43045, 43047, 43046, 43004, 43009, 43053, 43068, 43067, 43069, 43054,
        43022, 43024, 43026, 43028, 43030, 43062, 43063, 43064, 43065, 43066, 43023, 43025, 43027, 43029, 43031, 43032,
        43033, 43034, 43035, 43036, 43037, 43038, 43039, 43040, 43041, 43042, 43043, 43044, 43055, 43056, 43057, 43058,
        43059, 43060, 43061, 43005, 43006, 43007, 43008, 43018, 43019, 43020, 43021, 43051, 43050, 43052, 44034, 44032,
        44009, 44011, 44100, 44101, 44103, 44102, 44025, 44026, 44019, 44130, 44131, 44020, 44041, 10005, 10006, 10001,
        10002, 10003, 10004, 44086, 44087, 44088, 44089, 44090, 44091, 44092, 44093, 44094, 44116, 44117, 44095, 44096,
        44098, 44099, 50003, 50004, 44022, 44042, 44072, 44010, 44029, 44030, 44031, 44051, 44052, 44053, 44054, 44055,
        44056, 44057, 44058, 44061, 44062, 44063, 44064, 44065, 44066, 44067, 44068, 44049, 44081, 44082, 44083, 44085,
        44080, 44070, 44084, 44104, 44105, 44106, 44107, 44108, 44109, 44110, 44111, 44112, 44113, 44114, 44115, 44035,
        44036, 42026, 42027, 44097, 45004, 45008, 45005, 45006, 45007, 45034, 45026, 45040, 45027, 45041, 45033, 45066,
        45068, 45067, 45021, 45020, 45044, 45045, 45046, 45047, 45048, 45049, 45050, 45051, 45052, 45053, 45054, 45055,
        45056, 45057, 45058, 45059, 45060, 45061, 45030, 45035, 45023, 45036, 45025, 45062, 45064, 45042, 45032, 45031,
        45065, 45037, 45024, 45022, 45029, 45028, 45009, 45010, 45011, 45012, 45013, 46250, 46180, 46300, 48011, 46001,
        48009, 48005, 48006, 48018, 50005, 42018, 42019, 42021, 42025, 42032, 49000, 49001, 11001, 11002, 11003, 11004,
        11005, 11006, 11007, 11008, 11009, 47010, 47001, 47002, 47003, 47004, 47006, 47009, 47012, 47000,
    ];
    fn visit(entries: &[MenuEntry], category: &str, result: &mut Vec<Shortcut>) {
        for entry in entries {
            match entry {
                MenuEntry::Submenu { entries, .. } | MenuEntry::DynamicSubmenu { entries, .. } => visit(entries, category, result),
                MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => {
                    if let Some(key) = plugin_key(*command) {
                        let category = match &key { CommandKey::Plugin { module, .. } => module.clone(), _ => String::new() };
                        if !result.iter().any(|item| item.key == key) { result.push(Shortcut { key, name: entry.label().into(), category, bindings: Vec::new(), command: Some(*command) }); }
                        continue;
                    }
                    let id = match command {
                        MenuCommand::Source(48016) => Some(48016),
                        MenuCommand::OpenTerminal => Some(49000), MenuCommand::About => Some(47000), MenuCommand::DefineUserLanguage => Some(46250),
                        MenuCommand::Settings(command) => Some(match command { crate::menu::SettingsCommand::Preferences => 48011, crate::menu::SettingsCommand::StyleConfigurator => 46001, crate::menu::SettingsCommand::ShortcutMapper => 48009, crate::menu::SettingsCommand::ImportStyleThemes => 48006, crate::menu::SettingsCommand::EditContextMenu => 48018 }),
                        MenuCommand::View(command) => match command {
                            crate::menu::ViewCommand::IndentGuides => Some(44020), crate::menu::ViewCommand::WordWrap => Some(44022), crate::menu::ViewCommand::Summary => Some(44049), crate::menu::ViewCommand::DocumentList => Some(44070), crate::menu::ViewCommand::DocumentMap => Some(44080), crate::menu::ViewCommand::FunctionList => Some(44084), crate::menu::ViewCommand::FolderWorkspace => Some(44085), crate::menu::ViewCommand::Monitoring => Some(44097), crate::menu::ViewCommand::SyncVertical => Some(44035), crate::menu::ViewCommand::SyncHorizontal => Some(44036), crate::menu::ViewCommand::MoveToOtherView => Some(10001), crate::menu::ViewCommand::CloneToOtherView => Some(10002), _ => command_id(*command),
                        },
                        MenuCommand::HashSelection(algorithm) => Some(match algorithm { crate::hashes::HashAlgorithm::Md5 => 48503, crate::hashes::HashAlgorithm::Sha256 => 48506, crate::hashes::HashAlgorithm::Sha1 => 48509, crate::hashes::HashAlgorithm::Sha512 => 48512 }),
                        MenuCommand::Macro(command) => Some(match command { crate::macros::MacroCommand::StartRecording => 42018, crate::macros::MacroCommand::StopRecording => 42019, crate::macros::MacroCommand::Playback => 42021, crate::macros::MacroCommand::SaveCurrent => 42025, crate::macros::MacroCommand::RunMultiple => 42032 }),
                        _ => command.source_id(),
                    };
                    if let Some(id) = id {
                        if !SOURCE_ORDER.contains(&id) { continue; }
                        let source_default = |nth| SOURCE_DEFAULTS.iter().find(|(command, ordinal, _)| *command == id && *ordinal == nth).and_then(|(_, _, source)| KeyCombo::parse_source(source));
                        let primary = KeyCombo::parse_source(entry.shortcut()).filter(|key| key.key != 0);
                        let key = CommandKey::Menu { id, nth: 0 };
                        if result.iter().any(|item| item.key == key) { continue; }
                        if matches!(id, 42001 | 42002 | 42005) {
                            result.push(Shortcut { key, name: entry.label().into(), category: category.into(), bindings: source_default(0).into_iter().collect(), command: Some(*command) });
                            result.push(Shortcut { key: CommandKey::Menu { id, nth: 1 }, name: entry.label().into(), category: category.into(), bindings: primary.or_else(|| source_default(1)).into_iter().collect(), command: Some(*command) });
                        } else { result.push(Shortcut { key, name: entry.label().into(), category: category.into(), bindings: primary.or_else(|| source_default(0)).into_iter().collect(), command: Some(*command) }); }
                    }
                }
                MenuEntry::Separator => {},
            }
        }
    }
    let mut result = Vec::new();
    for definition in definitions {
        if let Some(command) = definition.direct_command() { visit(&[MenuEntry::Action(command)], &definition.label, &mut result); }
        visit(&definition.entries, &definition.label, &mut result);
    }
    for (id, name, category) in [(50003, "Switch to previous document", "View"), (50004, "Switch to next document", "View"), (50005, "Toggle macro recording", "Macro")] {
        let key = CommandKey::Menu { id, nth: 0 };
        if result.iter().any(|item| item.key == key) { continue; }
        let bindings = SOURCE_DEFAULTS.iter().find(|(command, _, _)| *command == id).and_then(|(_, _, source)| KeyCombo::parse_source(source)).into_iter().collect();
        result.push(Shortcut { key, name: name.into(), category: category.into(), bindings, command: None });
    }
    result.sort_by_key(|item| match item.key { CommandKey::Menu { id, .. } => SOURCE_ORDER.iter().position(|candidate| *candidate == id).unwrap_or(SOURCE_ORDER.len()), _ => SOURCE_ORDER.len() });
    result
}

fn command_id(command: crate::menu::ViewCommand) -> Option<u32> { MenuCommand::View(command).source_id() }

pub fn plugin_key(command: MenuCommand) -> Option<CommandKey> {
    let (module, internal_id) = match command {
        MenuCommand::Mime(crate::mime::MimeOperation::Base64Encode) => ("mimeTools", 0),
        MenuCommand::Mime(crate::mime::MimeOperation::Base64EncodePadded) => ("mimeTools", 1),
        MenuCommand::Mime(crate::mime::MimeOperation::Base64Decode) => ("mimeTools", 4),
        MenuCommand::Converter(crate::converter::ConverterOperation::AsciiToHex) => ("Converter", 0),
        MenuCommand::Converter(crate::converter::ConverterOperation::HexToAscii) => ("Converter", 1),
        MenuCommand::Source(59003) => ("Converter", 3),
        MenuCommand::Source(59005) => ("Converter", 5),
        MenuCommand::Source(59006) => ("Converter", 6),
        MenuCommand::ExportHtml => ("NppExport", 1),
        _ => return None,
    };
    Some(CommandKey::Plugin { module: module.into(), internal_id })
}

pub const SCINTILLA_COMMANDS: &[(&str, u32, u32, &str, &[&str])] = &[
    ("SCI_SELECTALL", 2013, 42007, "EditorSelectAll", &["ctrl-a"]),
    ("SCI_CLEAR", 2180, 42006, "EditorDelete", &["delete"]),
    ("SCI_UNDO", 2176, 42003, "EditorUndo", &["ctrl-z", "alt-backspace"]),
    ("SCI_REDO", 2011, 42004, "EditorRedo", &["ctrl-y", "ctrl-shift-z"]),
    ("SCI_NEWLINE", 2329, 0, "EditorEnter", &["enter", "shift-enter"]),
    ("SCI_TAB", 2327, 0, "EditorTab", &["tab"]),
    ("SCI_BACKTAB", 2328, 0, "EditorBackTab", &["shift-tab"]),
    ("SCI_ZOOMIN", 2333, 44023, "ZoomIn", &["ctrl-Num +"]),
    ("SCI_ZOOMOUT", 2334, 44024, "ZoomOut", &["ctrl-Num -"]),
    ("SCI_SETZOOM", 2373, 44033, "ZoomReset", &["ctrl-Num /"]),
    ("SCI_SELECTIONDUPLICATE", 2469, 0, "EditorDuplicateSelection", &["ctrl-d"]),
    ("SCI_EDITTOGGLEOVERTYPE", 2324, 0, "ToggleOverwrite", &["insert"]),
    ("SCI_LINEDOWN", 2300, 0, "EditorDown", &["down"]),
    ("SCI_LINEDOWNEXTEND", 2301, 0, "EditorSelectDown", &["shift-down"]),
    ("SCI_LINEDOWNRECTEXTEND", 2426, 0, "RectangleDown", &["alt-shift-down"]),
    ("SCI_LINEUP", 2302, 0, "EditorUp", &["up"]),
    ("SCI_LINEUPEXTEND", 2303, 0, "EditorSelectUp", &["shift-up"]),
    ("SCI_LINEUPRECTEXTEND", 2427, 0, "RectangleUp", &["alt-shift-up"]),
    ("SCI_CHARLEFT", 2304, 0, "EditorLeft", &["left"]),
    ("SCI_CHARLEFTEXTEND", 2305, 0, "EditorSelectLeft", &["shift-left"]),
    ("SCI_CHARLEFTRECTEXTEND", 2428, 0, "RectangleLeft", &["alt-shift-left"]),
    ("SCI_CHARRIGHT", 2306, 0, "EditorRight", &["right"]),
    ("SCI_CHARRIGHTEXTEND", 2307, 0, "EditorSelectRight", &["shift-right"]),
    ("SCI_CHARRIGHTRECTEXTEND", 2429, 0, "RectangleRight", &["alt-shift-right"]),
    ("SCI_WORDLEFT", 2308, 0, "WordLeft", &["ctrl-left"]),
    ("SCI_WORDLEFTEXTEND", 2309, 0, "SelectWordLeft", &["ctrl-shift-left"]),
    ("SCI_WORDRIGHT", 2310, 0, "WordRight", &["ctrl-right"]),
    ("SCI_WORDRIGHTENDEXTEND", 2442, 0, "SelectWordRight", &["ctrl-shift-right"]),
    ("SCI_WORDPARTLEFT", 2390, 0, "WordPartLeft", &["ctrl-/"]),
    ("SCI_WORDPARTLEFTEXTEND", 2391, 0, "SelectWordPartLeft", &["ctrl-shift-/"]),
    ("SCI_WORDPARTRIGHT", 2392, 0, "WordPartRight", &["ctrl-\\"]),
    ("SCI_WORDPARTRIGHTEXTEND", 2393, 0, "SelectWordPartRight", &["ctrl-shift-\\"]),
    ("SCI_HOMEDISPLAY", 2345, 0, "HomeDisplay", &["alt-home"]),
    ("SCI_LINEENDDISPLAY", 2347, 0, "LineEndDisplay", &["alt-end"]),
    ("SCI_VCHOMEWRAP", 2453, 0, "EditorHome", &["home"]),
    ("SCI_VCHOMEWRAPEXTEND", 2454, 0, "EditorSelectHome", &["shift-home"]),
    ("SCI_VCHOMERECTEXTEND", 2431, 0, "RectangleHome", &["alt-shift-home"]),
    ("SCI_LINEENDWRAP", 2451, 0, "EditorEnd", &["end"]),
    ("SCI_LINEENDWRAPEXTEND", 2452, 0, "EditorSelectEnd", &["shift-end"]),
    ("SCI_LINEENDRECTEXTEND", 2432, 0, "RectangleEnd", &["alt-shift-end"]),
    ("SCI_DOCUMENTSTART", 2316, 0, "EditorDocumentStart", &["ctrl-home"]),
    ("SCI_DOCUMENTSTARTEXTEND", 2317, 0, "EditorSelectDocumentStart", &["ctrl-shift-home"]),
    ("SCI_DOCUMENTEND", 2318, 0, "EditorDocumentEnd", &["ctrl-end"]),
    ("SCI_DOCUMENTENDEXTEND", 2319, 0, "EditorSelectDocumentEnd", &["ctrl-shift-end"]),
    ("SCI_PAGEUP", 2320, 0, "PageUp", &["pageup"]),
    ("SCI_PAGEUPEXTEND", 2321, 0, "SelectPageUp", &["shift-pageup"]),
    ("SCI_PAGEUPRECTEXTEND", 2433, 0, "RectanglePageUp", &["alt-shift-pageup"]),
    ("SCI_PAGEDOWN", 2322, 0, "PageDown", &["pagedown"]),
    ("SCI_PAGEDOWNEXTEND", 2323, 0, "SelectPageDown", &["shift-pagedown"]),
    ("SCI_PAGEDOWNRECTEXTEND", 2434, 0, "RectanglePageDown", &["alt-shift-pagedown"]),
    ("SCI_DELETEBACK", 2326, 0, "EditorBackspace", &["backspace", "shift-backspace"]),
    ("SCI_DELWORDLEFT", 2335, 0, "DeleteWordLeft", &["ctrl-backspace"]),
    ("SCI_DELWORDRIGHT", 2336, 0, "DeleteWordRight", &["ctrl-delete"]),
    ("SCI_DELLINELEFT", 2395, 0, "DeleteLineLeft", &["ctrl-shift-backspace"]),
    ("SCI_DELLINERIGHT", 2396, 0, "DeleteLineRight", &["ctrl-shift-delete"]),
    ("SCI_LINEDELETE", 2338, 0, "EditorLineDelete", &["ctrl-shift-l"]),
    ("SCI_CANCEL", 2325, 0, "EditorEscape", &["escape"]),
    ("SCI_SWAPMAINANCHORCARET", 2607, 0, "SwapMainAnchorCaret", &[]),
    ("SCI_ROTATESELECTION", 2606, 0, "RotateSelection", &[]),
    ("SCI_CLEARALL", 2004, 0, "ShortcutAction", &[]),
    ("SCI_FORMFEED", 2330, 0, "ShortcutAction", &[]),
    ("SCI_LINESJOIN", 2288, 0, "ShortcutAction", &[]),
    ("SCI_SCROLLCARET", 2169, 0, "ShortcutAction", &[]),
    ("SCI_MOVECARETINSIDEVIEW", 2401, 0, "ShortcutAction", &[]),
    ("SCI_LINESCROLLDOWN", 2342, 0, "ShortcutAction", &["ctrl-down"]),
    ("SCI_LINESCROLLUP", 2343, 0, "ShortcutAction", &["ctrl-up"]),
    ("SCI_PARADOWN", 2413, 0, "ShortcutAction", &["ctrl-]"]),
    ("SCI_PARADOWNEXTEND", 2414, 0, "ShortcutAction", &["ctrl-shift-]"]),
    ("SCI_PARAUP", 2415, 0, "ShortcutAction", &["ctrl-["]),
    ("SCI_PARAUPEXTEND", 2416, 0, "ShortcutAction", &["ctrl-shift-["]),
    ("SCI_WORDRIGHTEXTEND", 2311, 0, "ShortcutAction", &[]),
    ("SCI_WORDLEFTEND", 2439, 0, "ShortcutAction", &[]),
    ("SCI_WORDLEFTENDEXTEND", 2440, 0, "ShortcutAction", &[]),
    ("SCI_WORDRIGHTEND", 2441, 0, "ShortcutAction", &[]),
    ("SCI_HOME", 2312, 0, "ShortcutAction", &[]),
    ("SCI_HOMEEXTEND", 2313, 0, "ShortcutAction", &[]),
    ("SCI_HOMERECTEXTEND", 2430, 0, "ShortcutAction", &[]),
    ("SCI_HOMEDISPLAYEXTEND", 2346, 0, "ShortcutAction", &[]),
    ("SCI_HOMEWRAP", 2349, 0, "ShortcutAction", &[]),
    ("SCI_HOMEWRAPEXTEND", 2450, 0, "ShortcutAction", &[]),
    ("SCI_VCHOME", 2331, 0, "ShortcutAction", &[]),
    ("SCI_VCHOMEEXTEND", 2332, 0, "ShortcutAction", &[]),
    ("SCI_VCHOMEDISPLAY", 2652, 0, "ShortcutAction", &[]),
    ("SCI_VCHOMEDISPLAYEXTEND", 2653, 0, "ShortcutAction", &[]),
    ("SCI_LINEEND", 2314, 0, "ShortcutAction", &[]),
    ("SCI_LINEENDDISPLAYEXTEND", 2348, 0, "ShortcutAction", &[]),
    ("SCI_LINEENDEXTEND", 2315, 0, "ShortcutAction", &[]),
    ("SCI_STUTTEREDPAGEUP", 2435, 0, "ShortcutAction", &[]),
    ("SCI_STUTTEREDPAGEUPEXTEND", 2436, 0, "ShortcutAction", &[]),
    ("SCI_STUTTEREDPAGEDOWN", 2437, 0, "ShortcutAction", &[]),
    ("SCI_STUTTEREDPAGEDOWNEXTEND", 2438, 0, "ShortcutAction", &[]),
    ("SCI_DELETEBACKNOTLINE", 2344, 0, "ShortcutAction", &[]),
    ("SCI_LINECUT", 2337, 0, "ShortcutAction", &["ctrl-l"]),
    ("SCI_LINECOPY", 2455, 0, "ShortcutAction", &["ctrl-shift-x"]),
    ("SCI_LINETRANSPOSE", 2339, 0, "ShortcutAction", &["ctrl-t"]),
    ("SCI_LINEDUPLICATE", 2404, 42010, "ShortcutAction", &[]),
];

pub fn scintilla_catalog() -> Vec<Shortcut> {
    let mut catalog = SCINTILLA_COMMANDS.iter().map(|(name, id, menu_id, _, defaults)| {
        let bindings = defaults.iter().filter_map(|key| KeyCombo::parse_source(key)).collect::<Vec<_>>();
        Shortcut { key: CommandKey::Scintilla { id: *id, menu_id: *menu_id }, name: (*name).into(), category: String::new(), bindings, command: None }
    }).collect::<Vec<_>>();
    const SOURCE_ORDER: &[u32] = &[
        2013, 2180, 2004, 2176, 2011, 2329, 2327, 2328, 2330, 2333, 2334, 2373, 2469, 2288, 2169, 2324, 2401, 2300, 2301, 2426, 2342, 2302, 2303, 2427, 2343, 2413, 2414, 2415, 2416, 2304, 2305, 2428, 2306, 2307, 2429, 2308, 2309, 2310, 2311, 2439, 2440, 2441, 2442, 2390, 2391, 2392, 2393, 2312, 2313, 2430, 2345, 2346, 2349, 2450, 2331, 2332, 2431, 2652, 2653, 2453, 2454, 2314, 2452, 2432, 2347, 2348, 2451, 2315, 2316, 2317, 2318, 2319, 2320, 2321, 2433, 2322, 2323, 2434, 2435, 2436, 2437, 2438, 2326, 2344, 2335, 2336, 2395, 2396, 2338, 2337, 2455, 2339, 2404, 2325, 2607, 2606,
    ];
    catalog.sort_by_key(|item| match item.key { CommandKey::Scintilla { id, .. } => SOURCE_ORDER.iter().position(|candidate| *candidate == id).unwrap_or(SOURCE_ORDER.len()), _ => SOURCE_ORDER.len() });
    catalog
}

pub fn scintilla_key_for_action(name: &str) -> Option<CommandKey> {
    let name = name.rsplit("::").next().unwrap_or(name);
    SCINTILLA_COMMANDS.iter().find(|(_, _, _, action, _)| *action != "ShortcutAction" && *action == name).map(|(_, id, menu_id, _, _)| CommandKey::Scintilla { id: *id, menu_id: *menu_id })
}

#[cfg(all(feature = "native-ui", target_os = "macos"))]
impl BindingSet {
    pub fn native_bindings(&self, defaults: Vec<(Option<CommandKey>, gpui::KeyBinding)>, extra: Vec<(CommandKey, gpui::KeyBinding)>, cx: &gpui::App) -> io::Result<Vec<gpui::KeyBinding>> {
        let mut result = Vec::new();
        let mut builtin = Vec::new();
        let mut replaced = BTreeSet::new();
        let registered = defaults.iter().filter_map(|(key, _)| key.clone()).collect::<BTreeSet<_>>();
        for (key, binding) in defaults.into_iter().chain(extra.into_iter().filter(|(key, _)| self.override_for(key).is_some() && !registered.contains(key)).map(|(key, binding)| (Some(key), binding))) {
            let Some(combos) = key.as_ref().and_then(|key| self.override_for(key)) else {
                if key.is_some() && binding.keystrokes().len() == 1 && KeyCombo::parse_source(&binding.keystrokes()[0].unparse(gpui::OperatingSystem::Mac)).is_some_and(KeyCombo::needs_native_adapter) { continue; }
                if self.dual_modifiers && !matches!(key, Some(CommandKey::Macro(_))) { builtin.push(binding.clone()); }
                result.push(binding);
                continue;
            };
            let predicate = binding.predicate();
            let context = predicate.as_ref().map(|predicate| format!("{predicate:?}")).unwrap_or_default();
            if !replaced.insert((key, context)) { continue; }
            for combo in combos {
                if combo.key == 0 || combo.needs_native_adapter() { continue; }
                let Some(keystroke) = combo.keystroke() else { return Err(invalid(format!("Unsupported native key {}", key_name(combo.key)))); };
                result.push(gpui::KeyBinding::load(&keystroke, binding.action().boxed_clone(), predicate.clone(), false, binding.action_input(), cx).map_err(|error| invalid(error.to_string()))?);
            }
        }
        for binding in builtin {
            if binding.keystrokes().len() != 1 { continue; }
            let mut stroke = binding.keystrokes()[0].inner().clone();
            let Some(alias) = KeyCombo::parse_source(&stroke.unparse(gpui::OperatingSystem::Mac)).and_then(KeyCombo::builtin_alias) else { continue; };
            if self.runtime_alias_reserved(alias) { continue; }
            stroke.modifiers.control = alias.ctrl;
            stroke.modifiers.platform = alias.cmd;
            if self.reserved_bindings().filter_map(KeyCombo::keystroke).filter_map(|source| gpui::Keystroke::parse_with_os(&source, gpui::OperatingSystem::Mac).ok()).any(|reserved| reserved.key == stroke.key && reserved.modifiers == stroke.modifiers) { continue; }
            let source = stroke.unparse(gpui::OperatingSystem::Mac);
            let mut alias = gpui::KeyBinding::load(&source, binding.action().boxed_clone(), binding.predicate(), false, binding.action_input(), cx).map_err(|error| invalid(error.to_string()))?;
            if let Some(meta) = binding.meta() { alias.set_meta(meta); }
            if result.iter().any(|existing| existing.keystrokes() == alias.keystrokes() && existing.predicate() == alias.predicate() && existing.action().partial_eq(alias.action()) && existing.action_input() == alias.action_input()) { continue; }
            result.push(alias);
        }
        Ok(result)
    }
}
