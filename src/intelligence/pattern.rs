use fancy_regex::{Regex, RegexBuilder};

use super::IntelligenceError;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

const WORD: &str = "[\\p{L}\\p{Nl}\\p{Nd}_&&\\x{0000}-\\x{ffff}]";

#[derive(Clone)]
pub(crate) enum Pattern {
    Regex(Regex),
    JavaClass,
    JavaMethod,
    CDeclarator,
    NimRoutine,
    PascalRoutine(Regex),
}

impl Pattern {
    pub(crate) fn compile(source: &str) -> Result<Self, IntelligenceError> {
        static CACHE: OnceLock<Mutex<BTreeMap<String, Pattern>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
        if let Some(pattern) = cache
            .lock()
            .map_err(|error| IntelligenceError(error.to_string()))?
            .get(source)
            .cloned()
        {
            return Ok(pattern);
        }
        let pattern = Self::compile_uncached(source)?;
        cache
            .lock()
            .map_err(|error| IntelligenceError(error.to_string()))?
            .insert(source.into(), pattern.clone());
        Ok(pattern)
    }

    fn compile_uncached(source: &str) -> Result<Self, IntelligenceError> {
        static PROFILES: OnceLock<Result<BTreeMap<String, u8>, IntelligenceError>> =
            OnceLock::new();
        let profiles = PROFILES
            .get_or_init(|| {
                let value: serde_json::Value =
                    serde_json::from_str(include_str!("../../assets/intelligence/parsers.json"))
                        .map_err(|e| IntelligenceError(e.to_string()))?;
                let class = &value["java"]["definition"]["children"][0];
                let method = &class["children"][1];
                let mut profiles = BTreeMap::new();
                fn verified(node: &serde_json::Value, profiles: &mut BTreeMap<String, u8>) {
                    if let Some(attributes) = node["attributes"].as_object() {
                        for (key, value) in attributes {
                            if [
                                "mainExpr",
                                "expr",
                                "commentExpr",
                                "openSymbole",
                                "closeSymbole",
                            ]
                            .contains(&key.as_str())
                                && let Some(pattern) = value.as_str()
                            {
                                profiles.insert(pattern.into(), 255);
                            }
                        }
                    }
                    if let Some(children) = node["children"].as_array() {
                        for child in children {
                            verified(child, profiles);
                        }
                    }
                }
                if let Some(parsers) = value.as_object() {
                    for parser in parsers.values() {
                        verified(&parser["definition"], &mut profiles);
                    }
                }
                for (pattern, kind) in [
                    (&class["attributes"]["mainExpr"], 0),
                    (&method["attributes"]["mainExpr"], 1),
                    (
                        &value["c"]["definition"]["children"][0]["attributes"]["mainExpr"],
                        2,
                    ),
                    (
                        &value["nim"]["definition"]["children"][0]["attributes"]["mainExpr"],
                        3,
                    ),
                    (
                        &value["pascal"]["definition"]["children"][1]["attributes"]["mainExpr"],
                        4,
                    ),
                ] {
                    profiles.insert(
                        pattern
                            .as_str()
                            .ok_or_else(|| {
                                IntelligenceError("missing structural pattern profile".into())
                            })?
                            .into(),
                        kind,
                    );
                }
                Ok(profiles)
            })
            .as_ref()
            .map_err(Clone::clone)?;
        match profiles.get(source) {
            Some(0) => return Ok(Self::JavaClass),
            Some(1) => return Ok(Self::JavaMethod),
            Some(2) => return Ok(Self::CDeclarator),
            Some(3) => return Ok(Self::NimRoutine),
            Some(4) => {
                let marker = "(?=                                                                      # only match function/procedure definitions";
                let boundary = source.find(marker).ok_or_else(|| {
                    IntelligenceError("unrecognized Pascal recursive profile".into())
                })?;
                let header = compatibility(&source[..boundary])?;
                return RegexBuilder::new(&format!("(?imsR){header}"))
                    .backtrack_limit(10_000_000)
                    .build()
                    .map(Self::PascalRoutine)
                    .map_err(|error| IntelligenceError(error.to_string()));
            }
            _ => {}
        }
        let subroutine = source.contains("(?&")
            || source.contains("(?R)")
            || source.contains("\\g<")
            || source
                .as_bytes()
                .windows(3)
                .any(|bytes| bytes[0] == b'(' && bytes[1] == b'?' && bytes[2].is_ascii_digit());
        if subroutine && !profiles.contains_key(source) {
            return Err(IntelligenceError("unverified function-list subroutine pattern; no bounded-recursion fallback is permitted".into()));
        }
        let translated = compatibility(source)?;
        RegexBuilder::new(&format!("(?imsR){translated}"))
            .backtrack_limit(10_000_000)
            .build()
            .map(Self::Regex)
            .map_err(|error| {
                IntelligenceError(format!(
                    "unsupported/invalid function-list pattern {source:?}: {error}"
                ))
            })
    }

    pub(crate) fn find(
        &self,
        text: &str,
        start: usize,
    ) -> Result<Option<Range<usize>>, IntelligenceError> {
        match self {
            Self::Regex(regex) => {
                let mut cursor = start;
                loop {
                    let found = regex.find_from_pos(text, cursor).map_err(|error| {
                        IntelligenceError(format!("function-list match failed: {error}"))
                    })?;
                    let Some(found) = found else {
                        return Ok(None);
                    };
                    if found.start() != found.end() {
                        return Ok(Some(found.start()..found.end()));
                    }
                    let Some(ch) = text[found.end()..].chars().next() else {
                        return Ok(None);
                    };
                    cursor = found.end() + ch.len_utf8();
                }
            }
            Self::JavaClass => java_find(text, start, true),
            Self::JavaMethod => java_find(text, start, false),
            Self::CDeclarator => super::declarators::c_find(text, start),
            Self::NimRoutine => super::declarators::nim_find(text, start),
            Self::PascalRoutine(header) => pascal_find(header, text, start),
        }
    }
}

fn pascal_find(
    header: &Regex,
    text: &str,
    start: usize,
) -> Result<Option<Range<usize>>, IntelligenceError> {
    let comments = Pattern::compile(r"(?s:\{.*?\})|(?s:\(\*.*?\*\))|(?m-s://.*$)")?;
    let mut at = start;
    while let Some(found) = header
        .find_from_pos(text, at)
        .map_err(|error| IntelligenceError(error.to_string()))?
    {
        let candidate = found.start()..found.end();
        let mut tail = candidate.end;
        loop {
            let next = text[tail..]
                .char_indices()
                .find(|(_, ch)| !ch.is_whitespace())
                .map_or(text.len(), |(offset, _)| tail + offset);
            tail = next;
            if let Some(comment) = comments
                .find(text, tail)?
                .filter(|comment| comment.start == tail)
            {
                tail = comment.end;
                continue;
            }
            break;
        }
        let mut cursor = tail;
        let valid = loop {
            let trimmed = text[cursor..].trim_start();
            if ["CONST", "TYPE", "VAR", "LABEL", "BEGIN"]
                .iter()
                .any(|keyword| {
                    trimmed
                        .get(..keyword.len())
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(keyword))
                })
            {
                break true;
            }
            let line_start = text[..cursor].rfind('\n').map_or(0, |at| at + 1);
            let seek = if text[line_start..cursor].trim().is_empty() {
                line_start
            } else {
                cursor
            };
            let Some(next) = header
                .find_from_pos(text, seek)
                .map_err(|error| IntelligenceError(error.to_string()))?
            else {
                break false;
            };
            if next.start() > cursor || next.end() <= cursor {
                break false;
            }
            cursor = next.end();
            loop {
                cursor = text[cursor..]
                    .char_indices()
                    .find(|(_, ch)| !ch.is_whitespace())
                    .map_or(text.len(), |(offset, _)| cursor + offset);
                if let Some(comment) = comments
                    .find(text, cursor)?
                    .filter(|comment| comment.start == cursor)
                {
                    cursor = comment.end;
                    continue;
                }
                break;
            }
        };
        if valid {
            return Ok(Some(candidate));
        }
        if candidate.end <= at {
            return Err(IntelligenceError(
                "non-advancing Pascal routine profile".into(),
            ));
        }
        at = candidate.end;
    }
    Ok(None)
}

fn java_find(
    text: &str,
    start: usize,
    class: bool,
) -> Result<Option<Range<usize>>, IntelligenceError> {
    let candidate = Regex::new(if class {
        r"(?m)^[\t ]*(?:(?:abstract|final|native|private|protected|public|static|strictfp|synchronized|transient|volatile|@[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)\s+)*(?:class|enum|@?interface)\s+[A-Za-z_]\w*"
    } else {
        r"(?m)^[\t ]*(?:(?:abstract|final|native|private|protected|public|static|strictfp|synchronized|transient|volatile|@[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)\s+)*"
    }).map_err(|error| IntelligenceError(error.to_string()))?;
    let mut at = start;
    while at <= text.len() {
        let Some(found) = candidate
            .find_from_pos(text, at)
            .map_err(|error| IntelligenceError(error.to_string()))?
        else {
            return Ok(None);
        };
        let begin = found.start();
        let mut cursor = found.end();
        let valid = if class {
            let mut depth = 0;
            let mut valid = false;
            for (offset, ch) in text[cursor..].char_indices() {
                match ch {
                    '<' => depth += 1,
                    '>' => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    }
                    '{' if depth == 0 => {
                        let name_start = text[..found.end()]
                            .char_indices()
                            .rev()
                            .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_')
                            .last()
                            .map_or(found.end(), |(at, _)| at);
                        valid = java_class_types(&text[name_start..cursor + offset]);
                        cursor += offset + 1;
                        break;
                    }
                    ';' | '=' | '(' | ')' | '}' => break,
                    _ => {}
                }
            }
            valid
        } else {
            java_method_end(text, &mut cursor)
        };
        if valid {
            return Ok(Some(begin..cursor));
        }
        let Some(next_line) = text[begin..].find('\n') else {
            return Ok(None);
        };
        at = begin + next_line + 1;
    }
    Ok(None)
}

fn java_method_end(text: &str, cursor: &mut usize) -> bool {
    let remaining = &text[*cursor..];
    let Some(open) = remaining.find('(') else {
        return false;
    };
    let prefix = remaining[..open].trim();
    if prefix.contains([';', '{', '}', '=', ':', '"', '\'']) {
        return false;
    }
    let mut depth = 0;
    for ch in prefix.chars() {
        if ch == '<' {
            depth += 1;
        }
        if ch == '>' {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    if depth != 0 {
        return false;
    }
    let Some((ty, name)) = prefix.rsplit_once(char::is_whitespace) else {
        return false;
    };
    if ty.trim().is_empty()
        || name.is_empty()
        || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return false;
    }
    if !java_return_type(ty) {
        return false;
    }
    let keywords = [
        "if",
        "for",
        "while",
        "switch",
        "return",
        "new",
        "throw",
        "catch",
        "class",
        "interface",
        "enum",
        "assert",
        "synchronized",
    ];
    if keywords.contains(&name) || ty.split_whitespace().any(|token| keywords.contains(&token)) {
        return false;
    }
    let mut count = 1;
    let mut close = None;
    for (offset, ch) in remaining[open + 1..].char_indices() {
        match ch {
            '(' => count += 1,
            ')' => {
                count -= 1;
                if count == 0 {
                    close = Some(open + 1 + offset + 1);
                    break;
                }
            }
            '{' | '}' | ';' => return false,
            _ => {}
        }
    }
    let Some(close) = close else {
        return false;
    };
    let parameters = &remaining[open + 1..close - 1];
    let mut angle = 0;
    let mut segment = 0;
    for (offset, ch) in parameters
        .char_indices()
        .chain(std::iter::once((parameters.len(), ',')))
    {
        if ch == '<' {
            angle += 1;
        }
        if ch == '>' {
            angle -= 1;
        }
        if ch == ',' && angle == 0 {
            let param = parameters[segment..offset].trim();
            if !param.is_empty() {
                let Some((ty, name)) = param.rsplit_once(char::is_whitespace) else {
                    return false;
                };
                if ty.trim().is_empty()
                    || name.is_empty()
                    || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    return false;
                }
                if !java_return_type(ty.strip_prefix("final ").unwrap_or(ty)) {
                    return false;
                }
            }
            segment = offset + 1;
        }
    }
    let Some(body) = remaining[close..].find(['{', ';']) else {
        return false;
    };
    if remaining.as_bytes()[close + body] != b'{' {
        return false;
    }
    let suffix = remaining[close..close + body].trim();
    if !suffix.is_empty() && !suffix.starts_with("throws ") {
        return false;
    }
    *cursor += close + body + 1;
    true
}

fn java_tokens(text: &str) -> Option<Vec<&str>> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let ch = text[at..].chars().next()?;
        if ch.is_whitespace() {
            at += ch.len_utf8();
            continue;
        }
        if "<>?.,&[]".contains(ch) {
            tokens.push(&text[at..at + 1]);
            at += 1;
            continue;
        }
        if !ch.is_ascii_alphabetic() && ch != '_' {
            return None;
        }
        let end = text[at + 1..]
            .char_indices()
            .find(|(_, ch)| !((ch.is_alphanumeric() && *ch <= '\u{ffff}') || *ch == '_'))
            .map_or(text.len(), |(offset, _)| at + 1 + offset);
        tokens.push(&text[at..end]);
        at = end;
    }
    Some(tokens)
}

fn java_identifier(token: &str) -> bool {
    token
        .as_bytes()
        .first()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || *ch == b'_')
}

fn java_generic(tokens: &[&str], at: &mut usize) -> bool {
    if tokens.get(*at) != Some(&"<") {
        return false;
    }
    *at += 1;
    loop {
        if tokens.get(*at) == Some(&"?") {
            *at += 1;
        } else if !java_type(tokens, at) {
            return false;
        }
        if matches!(tokens.get(*at), Some(&"extends" | &"super")) {
            *at += 1;
            if !java_type(tokens, at) {
                return false;
            }
            while tokens.get(*at) == Some(&"&") {
                *at += 1;
                if !java_type(tokens, at) {
                    return false;
                }
            }
        }
        match tokens.get(*at) {
            Some(&",") => *at += 1,
            Some(&">") => {
                *at += 1;
                return true;
            }
            _ => return false,
        }
    }
}

fn java_type(tokens: &[&str], at: &mut usize) -> bool {
    if !tokens.get(*at).is_some_and(|token| java_identifier(token)) {
        return false;
    }
    *at += 1;
    if tokens.get(*at) == Some(&"<") && !java_generic(tokens, at) {
        return false;
    }
    if tokens.get(*at) == Some(&".") {
        *at += 1;
        if !java_type(tokens, at) {
            return false;
        }
    }
    while tokens.get(*at) == Some(&"[") {
        *at += 1;
        if tokens.get(*at) != Some(&"]") {
            return false;
        }
        *at += 1;
    }
    true
}

fn java_return_type(text: &str) -> bool {
    let Some(tokens) = java_tokens(text) else {
        return false;
    };
    let mut at = 0;
    if tokens.first() == Some(&"<") && !java_generic(&tokens, &mut at) {
        return false;
    }
    java_type(&tokens, &mut at) && at == tokens.len()
}

fn java_class_types(text: &str) -> bool {
    let Some(tokens) = java_tokens(text) else {
        return false;
    };
    let mut at = 0;
    if !java_type(&tokens, &mut at) {
        return false;
    }
    for keyword in ["extends", "implements"] {
        if tokens.get(at) == Some(&keyword) {
            at += 1;
            if !java_type(&tokens, &mut at) {
                return false;
            }
            while tokens.get(at) == Some(&",") {
                at += 1;
                if !java_type(&tokens, &mut at) {
                    return false;
                }
            }
        }
    }
    at == tokens.len()
}

fn compatibility(source: &str) -> Result<String, IntelligenceError> {
    let source = condition_compatibility(source)?;
    let mut translated = String::new();
    let mut chars = source.chars().peekable();
    let mut in_class = false;
    let mut class_start = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            class_start = false;
            let escape = chars
                .next()
                .ok_or_else(|| IntelligenceError("trailing pattern escape".into()))?;
            if escape == 'w' {
                translated.push_str(WORD);
            } else if escape == 'W' {
                translated.push_str(&format!("[^{WORD}]"));
            } else if escape == 'd' {
                translated.push_str("[\\p{Nd}&&\\x{0000}-\\x{ffff}]");
            } else if escape == 'D' {
                translated.push_str("[^[\\p{Nd}&&\\x{0000}-\\x{ffff}]]");
            } else if (escape == 'b' || escape == 'B') && !in_class {
                if escape == 'b' {
                    translated
                        .push_str(&format!("(?:(?<={WORD})(?!{WORD})|(?<!{WORD})(?={WORD}))"));
                } else {
                    translated
                        .push_str(&format!("(?:(?<={WORD})(?={WORD})|(?<!{WORD})(?!{WORD}))"));
                }
            } else if escape == 'h' {
                if !in_class {
                    translated.push('[');
                }
                translated.push_str(
                    "\\t\\x20\\x{a0}\\x{1680}\\x{2000}-\\x{200a}\\x{202f}\\x{205f}\\x{3000}",
                );
                if !in_class {
                    translated.push(']');
                }
            } else {
                translated.push('\\');
                translated.push(escape);
            }
        } else if in_class {
            if ch == '[' && chars.peek() == Some(&':') {
                let mut posix = String::new();
                for next in chars.by_ref() {
                    posix.push(next);
                    if next == ']' {
                        break;
                    }
                }
                match posix.as_str() {
                    ":alpha:]" => translated.push_str("[\\p{L}\\p{Nl}&&\\x{0000}-\\x{ffff}]"),
                    ":alnum:]" => {
                        translated.push_str("[\\p{L}\\p{Nl}\\p{Nd}&&\\x{0000}-\\x{ffff}]")
                    }
                    _ => {
                        return Err(IntelligenceError(format!(
                            "unverified POSIX class: [{posix}"
                        )));
                    }
                }
            } else if ch == '[' {
                translated.push_str("\\[");
            } else if ch == ']' && !class_start {
                in_class = false;
                translated.push(ch);
            } else {
                translated.push(ch);
            }
            if ch != '^' {
                class_start = false;
            }
        } else if ch == '(' && chars.peek() == Some(&'?') {
            chars.next();
            if chars.peek() == Some(&'R') {
                chars.next();
                if chars.next() != Some(')') {
                    return Err(IntelligenceError("unknown recursive call syntax".into()));
                }
                translated.push_str("\\g<0>");
            } else if chars.peek() == Some(&'&') || chars.peek().is_some_and(|c| c.is_ascii_digit())
            {
                if chars.peek() == Some(&'&') {
                    chars.next();
                }
                let mut name = String::new();
                for next in chars.by_ref() {
                    if next == ')' {
                        break;
                    }
                    name.push(next);
                }
                if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return Err(IntelligenceError("invalid named subroutine".into()));
                }
                translated.push_str(&format!("\\g<{name}>"));
            } else {
                translated.push_str("(?");
            }
        } else {
            if ch == '[' {
                in_class = true;
                class_start = true;
            }
            translated.push(ch);
        }
    }
    Ok(translated)
}

fn condition_compatibility(source: &str) -> Result<String, IntelligenceError> {
    let mut result = String::new();
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find("(?(?") {
        let start = cursor + offset;
        result.push_str(&source[cursor..start]);
        let mut depth = 1;
        let mut escaped = false;
        let mut class = false;
        let mut end = None;
        for (index, ch) in source[start + 3..].char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '[' => class = true,
                ']' => class = false,
                '(' if !class => depth += 1,
                ')' if !class => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + 3 + index + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.ok_or_else(|| IntelligenceError("unclosed assertion conditional".into()))?;
        result.push_str("(?(");
        result.push_str(&source[start + 2..end]);
        result.push(')');
        cursor = end;
    }
    result.push_str(&source[cursor..]);
    Ok(result)
}
