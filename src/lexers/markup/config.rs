use super::*;

pub(super) fn properties(text: &str, profile: &LanguageProfile, out: &mut Output) {
    let mut section = None;
    for range in lines(text) {
        let line = &text[range.clone()];
        let initial = line.len() - line.trim_start_matches([' ', '\t']).len();
        if initial > 0 && !enabled(profile, "lexer.props.allow.initial.spaces", true) {
            continue;
        }
        let body = &line[initial..];
        if body.starts_with(['#', '!', ';']) {
            out.paint(range.start, range.end, 1);
        } else if body.starts_with('[') {
            if let Some(start) = section {
                out.block(start, range.start);
            }
            section = Some(range.start);
            out.paint(range.start, range.end, 2);
        } else if body.starts_with('@') {
            out.paint(range.start, range.start + initial + 1, 4);
        } else if let Some(eq) = line.find(['=', ':']) {
            out.paint(range.start, range.start + eq, 5);
            out.paint(range.start + eq, range.start + eq + 1, 3);
        }
    }
    if let Some(start) = section {
        out.block(start, text.len());
    }
}

pub(super) fn yaml(text: &str, profile: &LanguageProfile, out: &mut Output) {
    let mut block_indent = None;
    for range in lines(text) {
        let line = &text[range.clone()];
        let b = line.as_bytes();
        let indent = b.iter().take_while(|c| **c == b' ').count();
        if let Some(parent) = block_indent {
            if indent > parent || line.trim().is_empty() {
                out.paint(range.start, range.end, 7);
                continue;
            }
            block_indent = None;
        }
        if b.get(indent) == Some(&b'\t') {
            out.paint(range.start, range.end, 8);
            continue;
        }
        if line.starts_with("---") || line.starts_with("...") {
            out.paint(range.start, range.end, 6);
            continue;
        }
        let mut i = indent;
        let mut key_start = i;
        while i < b.len() {
            let start = i;
            match b[i] {
                b'#' if i == indent || b[i - 1].is_ascii_whitespace() => {
                    out.paint(range.start + i, range.end, 1);
                    break;
                }
                b'\'' | b'"' => {
                    i = quoted(b, i, b[i], b[i] == b'"', b[i] == b'\'');
                    let mut next = i;
                    while b.get(next).is_some_and(u8::is_ascii_whitespace) {
                        next += 1;
                    }
                    if b.get(next) == Some(&b':') {
                        out.paint(range.start + start, range.start + i, 2);
                    }
                }
                b':' if b
                    .get(i + 1)
                    .is_none_or(|c| c.is_ascii_whitespace() || b",]}".contains(c)) =>
                {
                    if key_start < i {
                        out.paint(range.start + key_start, range.start + i, 2);
                    }
                    out.paint(range.start + i, range.start + i + 1, 9);
                    i += 1;
                    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
                        i += 1;
                    }
                    key_start = i;
                    if b.get(i).is_some_and(|c| b"|>".contains(c)) {
                        block_indent = Some(indent);
                    }
                }
                b'&' | b'*' => {
                    i += 1;
                    while b.get(i).is_some_and(|c| name(*c)) {
                        i += 1;
                    }
                    out.paint(range.start + start, range.start + i, 5);
                }
                b'[' | b']' | b'{' | b'}' | b',' => {
                    out.paint(range.start + i, range.start + i + 1, 9);
                    i += 1;
                    key_start = i;
                }
                b'-' if b.get(i + 1).is_some_and(u8::is_ascii_whitespace) => {
                    i += 1;
                    key_start = i;
                }
                c if word(c) || b"+-.".contains(&c) => {
                    i += 1;
                    while b.get(i).is_some_and(|c| word(*c) || b".-+".contains(c)) {
                        i += 1;
                    }
                    let token = &line[start..i];
                    let style = if keyword(profile, 0, token, false) {
                        3
                    } else if token.bytes().any(|c| c.is_ascii_digit())
                        && token
                            .bytes()
                            .all(|c| c.is_ascii_digit() || b"-+.,".contains(&c))
                    {
                        4
                    } else {
                        0
                    };
                    out.paint(range.start + start, range.start + i, style);
                }
                _ => i += 1,
            }
        }
        let line_no = out.line(range.start);
        out.levels[line_no].0 = indent;
    }
    for i in 0..out.levels.len().saturating_sub(1) {
        if out.levels[i + 1].0 > out.levels[i].0 {
            out.levels[i].1 = true;
        }
    }
}

pub(super) fn toml(text: &str, profile: &LanguageProfile, out: &mut Output) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut key = true;
    let mut stack = Vec::new();
    let mut tables: Vec<(usize, usize)> = Vec::new();
    while i < b.len() {
        let start = i;
        match b[i] {
            b'\n' | b'\r' => {
                key = stack.is_empty();
                i += 1;
            }
            b'#' => {
                i = line_end(b, i);
                out.paint(start, i, 1);
            }
            b'[' if key && stack.is_empty() => {
                let end = line_end(b, i);
                let mut depth = 0;
                i += 1;
                if b.get(i) == Some(&b'[') {
                    i += 1;
                }
                while i < end && b[i] != b']' {
                    if matches!(b[i], b'\'' | b'"') {
                        i = quoted(&b[..end], i, b[i], b[i] == b'"', false);
                    } else {
                        if b[i] == b'.' {
                            depth += 1;
                        }
                        i += 1;
                    }
                }
                let valid = b.get(i) == Some(&b']');
                while b.get(i) == Some(&b']') {
                    i += 1;
                }
                while tables
                    .last()
                    .is_some_and(|(_, previous)| *previous >= depth)
                {
                    if let Some((previous, _)) = tables.pop() {
                        out.block(previous, start);
                    }
                }
                tables.push((start, depth));
                out.paint(start, i, if valid { 5 } else { 7 });
                key = false;
            }
            b'\'' | b'"' => {
                let q = b[i];
                let triple = b.get(i..i + 3).is_some_and(|s| s.iter().all(|c| *c == q));
                let style = if key {
                    6
                } else {
                    match (q, triple) {
                        (b'\'', false) => 9,
                        (b'"', false) => 10,
                        (b'\'', true) => 11,
                        _ => 12,
                    }
                };
                if triple {
                    i += 3;
                    while i < b.len() {
                        if q == b'"' && b[i] == b'\\' {
                            i = (i + 2).min(b.len());
                        } else if b.get(i..i + 3) == Some(&[q, q, q]) {
                            i += 3;
                            while b.get(i) == Some(&q) {
                                i += 1;
                            }
                            break;
                        } else {
                            i += 1;
                        }
                    }
                } else {
                    let end = line_end(b, i);
                    i = quoted(&b[..end], i, q, q == b'"', false);
                }
                out.paint(start, i, style);
                if !triple && b.get(i.saturating_sub(1)) != Some(&q) {
                    out.paint(start, i, 15);
                }
                if q == b'"' {
                    let mut p = start + if triple { 3 } else { 1 };
                    while p < i {
                        if b[p] == b'\\' {
                            if b.get(p + 1).is_some_and(|c| b"\r\n".contains(c)) {
                                p += 1;
                                continue;
                            }
                            let len = match b.get(p + 1) {
                                Some(b'x') => 4,
                                Some(b'u') => 6,
                                Some(b'U') => 10,
                                _ => 2,
                            };
                            out.paint(p, (p + len).min(i), 13);
                            p += len;
                        } else {
                            p += 1;
                        }
                    }
                }
                if triple {
                    out.block(start, i);
                }
            }
            b'=' => {
                out.paint(i, i + 1, 8);
                key = false;
                i += 1;
            }
            b'[' | b'{' => {
                stack.push((b[i], i));
                out.paint(i, i + 1, 8);
                if b[i] == b'{' {
                    key = true;
                }
                i += 1;
            }
            b']' | b'}' => {
                if let Some((_, open)) = stack.pop() {
                    out.block(open, i + 1);
                }
                out.paint(i, i + 1, 8);
                key = false;
                i += 1;
            }
            b',' | b'.' if key || b[i] == b',' => {
                out.paint(i, i + 1, 8);
                if b[i] == b',' {
                    key = stack.last().is_some_and(|(c, _)| *c == b'{');
                }
                i += 1;
            }
            c if !c.is_ascii_whitespace() => {
                i += 1;
                while b.get(i).is_some_and(|c| {
                    !c.is_ascii_whitespace() && !b"=,]}#".contains(c) && (!key || *c != b'.')
                }) {
                    i += 1;
                }
                let token = &text[start..i];
                let style = if key {
                    6
                } else if keyword(profile, 0, token, false) || matches!(token, "true" | "false") {
                    3
                } else if token.len() >= 10
                    && token.as_bytes().get(4) == Some(&b'-')
                    && token.as_bytes().get(7) == Some(&b'-')
                    || token.as_bytes().get(2) == Some(&b':')
                        && token.as_bytes().first().is_some_and(u8::is_ascii_digit)
                {
                    14
                } else if token.parse::<f64>().is_ok()
                    || ["0x", "0o", "0b"]
                        .iter()
                        .any(|prefix| token.starts_with(prefix))
                    || token.replace('_', "").parse::<f64>().is_ok()
                    || matches!(token, "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan")
                {
                    4
                } else {
                    7
                };
                out.paint(start, i, style);
            }
            _ => i += 1,
        }
    }
    for (start, _) in tables {
        out.block(start, text.len());
    }
}
