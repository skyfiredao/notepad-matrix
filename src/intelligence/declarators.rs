use std::ops::Range;

use super::IntelligenceError;

fn whitespace(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map_or(text.len(), |(offset, _)| at + offset)
}

fn identifier(text: &str, at: usize) -> Option<usize> {
    let first = text[at..].chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || ('\u{7f}'..='\u{ff}').contains(&first)) {
        return None;
    }
    let end = text[at + first.len_utf8()..]
        .char_indices()
        .find(|(_, ch)| {
            !(ch.is_alphanumeric() && *ch <= '\u{ffff}'
                || *ch == '_'
                || ('\u{7f}'..='\u{ff}').contains(ch))
        })
        .map_or(text.len(), |(offset, _)| at + first.len_utf8() + offset);
    Some(end)
}

fn valid_identifier(text: &str, at: usize) -> Option<usize> {
    let end = identifier(text, at)?;
    let invalid = [
        "auto",
        "break",
        "case",
        "char",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extern",
        "float",
        "for",
        "goto",
        "if",
        "int",
        "inline",
        "long",
        "while",
        "register",
        "restrict",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "typedef",
        "union",
        "unsigned",
        "void",
        "volatile",
        "_Alignas",
        "_Alignof",
        "_Atomic",
        "_Bool",
        "_Complex",
        "_Generic",
        "_Imaginary",
        "_Noreturn",
        "_Static_assert",
        "_Thread_local",
    ];
    (!invalid.contains(&&text[at..end])).then_some(end)
}

fn declaration(text: &str, start: usize, depth: usize) -> Result<Vec<usize>, IntelligenceError> {
    if depth > 1024 {
        return Err(IntelligenceError(
            "C declarator exceeds structural nesting resource limit (1024)".into(),
        ));
    }
    let mut at = whitespace(text, start);
    while text.as_bytes().get(at) == Some(&b'*') {
        at = whitespace(text, at + 1);
        while let Some(end) = identifier(text, at) {
            if !["const", "volatile"].contains(&&text[at..end]) {
                break;
            }
            at = whitespace(text, end);
        }
    }
    let mut ends = Vec::new();
    if let Some(end) = valid_identifier(text, at) {
        let tail = whitespace(text, end);
        ends.push(tail);
        if let Some(open @ (b'(' | b'[')) = text.as_bytes().get(tail).copied() {
            let close = if open == b'(' { b')' } else { b']' };
            let prohibited = if open == b'(' { "();{" } else { "[];{" };
            let stop = text[tail + 1..]
                .char_indices()
                .find(|(_, ch)| prohibited.contains(*ch))
                .map(|(offset, _)| tail + 1 + offset);
            if let Some(stop) = stop.filter(|at| text.as_bytes()[*at] == close) {
                ends.push(whitespace(text, stop + 1));
            }
        }
    }
    if text.as_bytes().get(at) == Some(&b'(') {
        for end in declaration(text, at + 1, depth + 1)? {
            if text.as_bytes().get(end) == Some(&b')')
                && end > 0
                && text.as_bytes()[end - 1] != b')'
            {
                ends.push(whitespace(text, end + 1));
            }
        }
    }
    Ok(ends)
}

pub(crate) fn c_find(text: &str, start: usize) -> Result<Option<Range<usize>>, IntelligenceError> {
    for (offset, ch) in text[start..].char_indices() {
        let begin = start + offset;
        if !(ch.is_ascii_alphabetic()
            || ch == '_'
            || ch == '*'
            || ch == '('
            || ('\u{7f}'..='\u{ff}').contains(&ch))
        {
            continue;
        }
        if ch != '*'
            && ch != '('
            && text[..begin]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let mut positions = vec![begin];
        let mut cursor = begin;
        while let Some(end) = identifier(text, cursor) {
            let token = &text[cursor..end];
            let mut next = whitespace(text, end);
            let specifier = [
                "auto", "register", "static", "extern", "typedef", "void", "char", "short", "int",
                "long", "float", "double", "signed", "unsigned", "const", "volatile",
            ];
            if ["struct", "union", "enum"].contains(&token) {
                let Some(type_end) = valid_identifier(text, next) else {
                    break;
                };
                next = whitespace(text, type_end);
            } else if !specifier.contains(&token) && valid_identifier(text, cursor).is_none() {
                break;
            }
            if next <= cursor {
                break;
            }
            positions.push(next);
            cursor = next;
        }
        for position in positions.into_iter().rev() {
            for end in declaration(text, position, 0)? {
                if text.as_bytes().get(end) == Some(&b'{') {
                    return Ok(Some(begin..end));
                }
            }
        }
    }
    Ok(None)
}

pub(crate) fn nim_find(
    text: &str,
    start: usize,
) -> Result<Option<Range<usize>>, IntelligenceError> {
    let mut line = if start == 0 || text.as_bytes().get(start.wrapping_sub(1)) == Some(&b'\n') {
        start
    } else {
        text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset + 1)
    };
    while line < text.len() {
        let begin = text[line..]
            .char_indices()
            .find(|(_, ch)| *ch != ' ' && *ch != '\t')
            .map_or(text.len(), |(offset, _)| line + offset);
        let word_end = identifier(text, begin).unwrap_or(begin);
        if [
            "proc",
            "func",
            "method",
            "iterator",
            "macro",
            "template",
            "converter",
        ]
        .iter()
        .any(|keyword| text[begin..word_end].eq_ignore_ascii_case(keyword))
            && text[word_end..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        {
            let name = whitespace(text, word_end);
            let mut at = name;
            if text.as_bytes().get(at) == Some(&b'`') {
                let Some(close) = text[at + 1..].find('`') else {
                    return Ok(None);
                };
                at += close + 2;
            }
            while at < text.len() && !b"={(".contains(&text.as_bytes()[at]) {
                at += 1;
            }
            if text.as_bytes().get(at) == Some(&b'(') {
                let mut depth = 1usize;
                at += 1;
                while at < text.len() && depth > 0 {
                    match text.as_bytes()[at] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    at += 1;
                }
                if depth > 0 {
                    return Err(IntelligenceError(
                        "unterminated Nim routine parameter list".into(),
                    ));
                }
            }
            while at < text.len() && !b"={".contains(&text.as_bytes()[at]) {
                at += 1;
            }
            return Ok(Some(name..at));
        }
        line = text[line..]
            .find('\n')
            .map_or(text.len(), |offset| line + offset + 1);
    }
    Ok(None)
}
