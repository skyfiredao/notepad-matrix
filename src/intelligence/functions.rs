use std::ops::Range;

use super::{DefinitionNode, IntelligenceError, pattern::Pattern};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range<usize>,
    pub name_range: Range<usize>,
    pub jump_byte: usize,
    pub children: Vec<FunctionSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    NoParser,
    Parsed(Vec<FunctionSymbol>),
}

pub(crate) fn matches(
    pattern: &Pattern,
    text: &str,
    bounds: Range<usize>,
) -> Result<Vec<Range<usize>>, IntelligenceError> {
    let mut result = Vec::new();
    let mut start = bounds.start;
    while start <= bounds.end {
        let Some(range) = pattern.find(&text[..bounds.end], start)? else {
            break;
        };
        if range.end > bounds.end {
            break;
        }
        let next = if range.end > start {
            range.end
        } else {
            match text[start..bounds.end].chars().next() {
                Some(ch) => start + ch.len_utf8(),
                None => break,
            }
        };
        result.push(range);
        start = next;
    }
    Ok(result)
}

fn name_range(
    node: &DefinitionNode,
    child: &str,
    text: &str,
    mut range: Range<usize>,
) -> Result<Option<Range<usize>>, IntelligenceError> {
    if let Some(names) = node.child(child) {
        for expression in &names.children {
            if !["nameExpr", "funcNameExpr"].contains(&expression.tag.as_str()) {
                return Err(IntelligenceError(format!(
                    "unknown name expression: {}",
                    expression.tag
                )));
            }
            let pattern = Pattern::compile(expression.attr("expr"))?;
            let Some(found) = pattern.find(&text[range.clone()], 0)? else {
                return Ok(None);
            };
            range = range.start + found.start..range.start + found.end;
        }
    }
    Ok(Some(range))
}

fn excluded(position: usize, zones: &[Range<usize>]) -> bool {
    zones.iter().any(|zone| zone.contains(&position))
}

pub(crate) fn string_zones(key: &str, text: &str, comments: &[Range<usize>]) -> Vec<Range<usize>> {
    if [
        "ini",
        "toml",
        "xml",
        "nsis",
        "inno",
        "latex",
        "tex",
        "nppexec",
        "makefile",
        "batch",
        "asm",
        "sinumerik",
    ]
    .contains(&key)
    {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut zones = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(comment) = comments.iter().find(|zone| zone.contains(&index)) {
            index = comment.end;
            continue;
        }
        if let Some(end) = raw_string_end(key, text, index) {
            zones.push(index..end);
            index = end;
            continue;
        }
        if let Some(end) = lexical_comment_end(key, text, index) {
            zones.push(index..end);
            index = end;
            continue;
        }
        if bytes[index] == b'/' && ["javascript.js", "typescript"].contains(&key) {
            let prefix = text[..index].trim_end();
            if prefix.ends_with(['=', '(', ':', ',', '[', '!', '&', '|', '?', '{', ';'])
                || prefix.ends_with("return")
            {
                let begin = index;
                index += 1;
                let mut class = false;
                while index < bytes.len() && bytes[index] != b'\n' {
                    if bytes[index] == b'\\' {
                        index = (index + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[index] == b'[' {
                        class = true;
                    }
                    if bytes[index] == b']' {
                        class = false;
                    }
                    if bytes[index] == b'/' && !class {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                while bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
                    index += 1;
                }
                zones.push(begin..index);
                continue;
            }
        }
        let quote = bytes[index];
        if quote != b'"' && quote != b'\'' && quote != b'`' {
            index += text[index..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        if quote == b'`' && !["javascript.js", "typescript", "bash"].contains(&key) {
            index += 1;
            continue;
        }
        if quote == b'\''
            && key == "rust"
            && bytes.get(index + 2) != Some(&b'\'')
            && bytes.get(index + 1).is_some_and(u8::is_ascii_alphabetic)
        {
            index += 1;
            continue;
        }
        let triple = bytes
            .get(index..index + 3)
            .is_some_and(|s| s.iter().all(|b| *b == quote));
        let width = if triple { 3 } else { 1 };
        let begin = index;
        index += width;
        while index < bytes.len() {
            let escape = if key == "powershell" {
                b'`'
            } else if ["sql", "pascal", "vb"].contains(&key) {
                0
            } else {
                b'\\'
            };
            if bytes[index] == escape {
                index = (index + 2).min(bytes.len());
                continue;
            }
            if bytes
                .get(index..index + width)
                .is_some_and(|s| s.iter().all(|b| *b == quote))
            {
                index += width;
                if width == 1
                    && ["sql", "pascal", "vb"].contains(&key)
                    && bytes.get(index) == Some(&quote)
                {
                    index += 1;
                    continue;
                }
                break;
            }
            if !triple
                && bytes[index] == b'\n'
                && ![
                    "javascript.js",
                    "typescript",
                    "python",
                    "powershell",
                    "ruby",
                    "perl",
                ]
                .contains(&key)
            {
                break;
            }
            index += 1;
        }
        zones.push(begin..index);
    }
    zones
}

fn raw_string_end(key: &str, text: &str, at: usize) -> Option<usize> {
    let tail = &text[at..];
    if key == "rust" && tail.starts_with('r') {
        let hashes = tail[1..].bytes().take_while(|byte| *byte == b'#').count();
        if tail.as_bytes().get(hashes + 1) == Some(&b'"') {
            let body = hashes + 2;
            let close = format!("\"{}", "#".repeat(hashes));
            return Some(
                tail[body..]
                    .find(&close)
                    .map_or(text.len(), |offset| at + body + offset + close.len()),
            );
        }
    }
    if key == "cpp" && tail.starts_with("R\"") {
        let open = tail[2..].find('(')? + 2;
        let delimiter = &tail[2..open];
        if delimiter.len() > 16
            || delimiter
                .chars()
                .any(|ch| ch.is_whitespace() || "()\\".contains(ch))
        {
            return None;
        }
        let close = format!("){delimiter}\"");
        return Some(
            tail[open + 1..]
                .find(&close)
                .map_or(text.len(), |offset| at + open + 1 + offset + close.len()),
        );
    }
    if key == "lua" {
        let prefix = if tail.starts_with("--[") { 2 } else { 0 };
        if tail.as_bytes().get(prefix) != Some(&b'[') {
            return None;
        }
        let equals = tail[prefix + 1..]
            .bytes()
            .take_while(|byte| *byte == b'=')
            .count();
        let open = prefix + equals + 2;
        if tail.as_bytes().get(open - 1) != Some(&b'[') {
            return None;
        }
        let close = format!("]{}]", "=".repeat(equals));
        return Some(
            tail[open..]
                .find(&close)
                .map_or(text.len(), |offset| at + open + offset + close.len()),
        );
    }
    None
}

fn lexical_comment_end(key: &str, text: &str, at: usize) -> Option<usize> {
    let tail = &text[at..];
    let c_like = [
        "c",
        "cpp",
        "cs",
        "d",
        "java",
        "javascript.js",
        "typescript",
        "rust",
        "php",
        "css",
    ]
    .contains(&key);
    let block = if c_like && tail.starts_with("/*") {
        Some(("/*", "*/", key == "rust"))
    } else if key == "pascal" && tail.starts_with("(*") {
        Some(("(*", "*)", false))
    } else if key == "pascal" && tail.starts_with('{') {
        Some(("{", "}", false))
    } else if key == "haskell" && tail.starts_with("{-") {
        Some(("{-", "-}", true))
    } else if key == "powershell" && tail.starts_with("<#") {
        Some(("<#", "#>", true))
    } else {
        None
    };
    if let Some((open, close, nested)) = block {
        let mut cursor = at + open.len();
        let mut depth = 1;
        while cursor < text.len() {
            if text[cursor..].starts_with(close) {
                depth -= 1;
                cursor += close.len();
                if depth == 0 {
                    return Some(cursor);
                }
            } else if nested && text[cursor..].starts_with(open) {
                depth += 1;
                cursor += open.len();
            } else {
                cursor += text[cursor..].chars().next()?.len_utf8();
            }
        }
        return Some(text.len());
    }
    let line = c_like && key != "css" && tail.starts_with("//")
        || [
            "python",
            "gdscript",
            "ruby",
            "perl",
            "raku",
            "bash",
            "nim",
            "powershell",
            "php",
        ]
        .contains(&key)
            && tail.starts_with('#')
        || ["ada", "sql", "haskell", "lua"].contains(&key) && tail.starts_with("--")
        || key == "fortran" && tail.starts_with('!')
        || key == "pascal" && tail.starts_with("//")
        || key == "vb" && tail.starts_with('\'');
    if line {
        Some(
            tail.find(['\r', '\n'])
                .map_or(text.len(), |offset| at + offset),
        )
    } else {
        None
    }
}

pub(crate) fn ignored_zones(
    key: &str,
    parser: Option<&DefinitionNode>,
    text: &str,
) -> Result<Vec<Range<usize>>, IntelligenceError> {
    let lexical = string_zones(key, text, &[]);
    let mut source = source_comment_zones(parser, text)?;
    source.retain(|range| {
        !lexical
            .iter()
            .any(|zone| zone.start < range.start && zone.contains(&range.start))
    });
    source.extend(lexical);
    Ok(source)
}

pub(crate) fn source_comment_zones(
    parser: Option<&DefinitionNode>,
    text: &str,
) -> Result<Vec<Range<usize>>, IntelligenceError> {
    let Some(parser) = parser.filter(|parser| !parser.attr("commentExpr").is_empty()) else {
        return Ok(Vec::new());
    };
    match Pattern::compile(parser.attr("commentExpr")) {
        Ok(pattern) => matches(&pattern, text, 0..text.len()),
        Err(_) => Ok(Vec::new()),
    }
}

fn body_end(
    node: &DefinitionNode,
    text: &str,
    header: &Range<usize>,
    zones: &[Range<usize>],
) -> Result<usize, IntelligenceError> {
    if node.attr("openSymbole").is_empty() || node.attr("closeSymbole").is_empty() {
        return Ok(header.end);
    }
    let open = Pattern::compile(node.attr("openSymbole"))?;
    let close = Pattern::compile(node.attr("closeSymbole"))?;
    let both = Pattern::compile(&format!(
        "(?:{})|(?:{})",
        node.attr("openSymbole"),
        node.attr("closeSymbole")
    ))?;
    let mut depth = 1;
    for symbol in matches(&both, text, header.end..text.len())? {
        if excluded(symbol.start, zones) {
            continue;
        }
        if open.find(&text[symbol.clone()], 0)?.is_some() {
            depth += 1;
        } else if close.find(&text[symbol.clone()], 0)?.is_some() {
            depth -= 1;
        }
        if depth == 0 {
            return Ok(symbol.end);
        }
    }
    Ok(text.len())
}

fn function_extent(
    key: &str,
    text: &str,
    header: &Range<usize>,
    zones: &[Range<usize>],
) -> Range<usize> {
    let mut end = header.end;
    let mut brace = text[..end]
        .char_indices()
        .next_back()
        .filter(|(_, c)| *c == '{')
        .map(|(i, _)| i);
    if brace.is_none() {
        brace = text[end..]
            .char_indices()
            .find(|(_, ch)| !ch.is_whitespace())
            .filter(|(_, ch)| *ch == '{')
            .map(|(offset, _)| end + offset);
    }
    if let Some(open) = brace {
        let mut depth = 0;
        for (offset, ch) in text[open..].char_indices() {
            let at = open + offset;
            if excluded(at, zones) {
                continue;
            }
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    end = at + 1;
                    break;
                }
            }
        }
    } else if ["python", "gdscript"].contains(&key) {
        let line = text[..header.start].rfind('\n').map_or(0, |i| i + 1);
        let indent = text[line..header.start]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        let after_line = text[header.end..]
            .find('\n')
            .map_or(text.len(), |i| header.end + i + 1);
        end = text.len();
        let mut at = after_line;
        for next in text[after_line..].split_inclusive('\n') {
            if !next.trim().is_empty()
                && next.chars().take_while(|c| *c == ' ' || *c == '\t').count() <= indent
            {
                end = at;
                break;
            }
            at += next.len();
        }
    }
    header.start..end
}

fn make_symbol(
    kind: SymbolKind,
    text: &str,
    name: Range<usize>,
    range: Range<usize>,
) -> FunctionSymbol {
    FunctionSymbol {
        name: text[name.clone()].to_owned(),
        kind,
        jump_byte: name.start,
        name_range: name,
        range,
        children: Vec::new(),
    }
}

pub(crate) fn parse(
    key: &str,
    parser: &DefinitionNode,
    text: &str,
) -> Result<ParseOutcome, IntelligenceError> {
    let zones = ignored_zones(key, Some(parser), text)?;
    let mut classes = Vec::new();
    let mut scanned = Vec::new();
    if let Some(class) = parser.child("classRange") {
        let class_pattern = Pattern::compile(class.attr("mainExpr"))?;
        for header in matches(&class_pattern, text, 0..text.len())? {
            if excluded(header.start, &zones) {
                continue;
            }
            let Some(name) = name_range(class, "className", text, header.clone())? else {
                continue;
            };
            if excluded(name.start, &zones) || name.is_empty() {
                continue;
            }
            let end = body_end(class, text, &header, &zones)?;
            scanned.push(header.start..end);
            if end == text.len() {
                break;
            }
            classes.push(make_symbol(
                SymbolKind::Class,
                text,
                name,
                header.start..end,
            ));
        }
    }
    let mut functions = Vec::new();
    if let Some(class) = parser.child("classRange")
        && let Some(method) = class.child("function")
    {
        let expression = Pattern::compile(method.attr("mainExpr"))?;
        for class_symbol in &classes {
            for header in matches(&expression, text, class_symbol.range.clone())? {
                if header.end == class_symbol.range.end {
                    break;
                }
                if excluded(header.start, &zones) {
                    continue;
                }
                let Some(name) = name_range(method, "functionName", text, header.clone())? else {
                    continue;
                };
                if name.is_empty() || excluded(name.start, &zones) {
                    continue;
                }
                let symbol = make_symbol(
                    SymbolKind::Function,
                    text,
                    name,
                    function_extent(key, text, &header, &zones),
                );
                if !functions
                    .iter()
                    .any(|s: &FunctionSymbol| s.jump_byte == symbol.jump_byte)
                {
                    functions.push(symbol);
                }
            }
        }
    }
    if let Some(function) = parser.child("function") {
        let expression = Pattern::compile(function.attr("mainExpr"))?;
        for header in matches(&expression, text, 0..text.len())? {
            if header.end == text.len() {
                break;
            }
            if excluded(header.start, &zones) {
                continue;
            }
            let Some(name) = name_range(function, "functionName", text, header.clone())? else {
                continue;
            };
            if name.is_empty()
                || excluded(name.start, &zones)
                || scanned.iter().any(|range| range.contains(&name.start))
            {
                continue;
            }
            let symbol = make_symbol(
                SymbolKind::Function,
                text,
                name,
                function_extent(key, text, &header, &zones),
            );
            if let Some(class_name) = name_range(function, "className", text, header.clone())?
                .filter(|_| function.child("className").is_some())
            {
                let class_text = text[class_name.clone()].trim();
                if !class_text.is_empty() {
                    if let Some(group) = classes.iter_mut().find(|c| c.name == class_text) {
                        group.children.push(symbol);
                    } else {
                        let mut group =
                            make_symbol(SymbolKind::Class, text, class_name, symbol.range.clone());
                        group.children.push(symbol);
                        classes.push(group);
                    }
                    continue;
                }
            }
            functions.push(symbol);
        }
    }
    classes.sort_by_key(|s| s.range.end - s.range.start);
    for symbol in functions {
        if let Some(class) = classes
            .iter_mut()
            .find(|class| class.range.contains(&symbol.jump_byte))
        {
            class.children.push(symbol);
        } else {
            classes.push(symbol);
        }
    }
    let mut roots: Vec<FunctionSymbol> = Vec::new();
    classes.sort_by_key(|s| s.range.end - s.range.start);
    for mut symbol in classes {
        let mut index = 0;
        while index < roots.len() {
            if symbol.kind == SymbolKind::Class
                && symbol.range.start <= roots[index].range.start
                && symbol.range.end >= roots[index].range.end
            {
                symbol.children.push(roots.remove(index));
            } else {
                index += 1;
            }
        }
        symbol.children.sort_by_key(|s| s.jump_byte);
        roots.push(symbol);
    }
    roots.sort_by_key(|s| s.jump_byte);
    Ok(ParseOutcome::Parsed(roots))
}
