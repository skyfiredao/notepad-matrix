use super::*;

pub(super) fn scan(text: &str, profile: &LanguageProfile, out: &mut Output) {
    let b = text.as_bytes();
    let ms = profile.key() == "mssql";
    let mut i = 0;
    let mut blocks = Vec::new();
    while i < b.len() {
        let start = i;
        let mut style = 0;
        if b[i..].starts_with(b"/*") {
            let doc = !ms && b.get(i + 2).is_some_and(|c| b"*!".contains(c));
            let mut depth = 1;
            i += 2;
            while i < b.len() && depth > 0 {
                if ms && b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            style = if doc { 3 } else { 1 };
            if enabled(profile, "fold.comment", true) {
                out.block(start, i);
            }
        } else if b[i..].starts_with(b"--")
            || (!ms && b[i] == b'#' && enabled(profile, "lexer.sql.numbersign.comment", false))
        {
            i = line_end(b, i);
            style = if !ms && b[start] == b'#' { 15 } else { 2 };
        } else if !ms
            && matches!(b[i], b'q' | b'Q')
            && b.get(i + 1) == Some(&b'\'')
            && b.get(i + 2).is_some()
        {
            let close = match b[i + 2] {
                b'[' => b']',
                b'(' => b')',
                b'{' => b'}',
                b'<' => b'>',
                c => c,
            };
            i = until(b, i + 3, &[close, b'\'']);
            style = 24;
        } else if b[i] == b'\''
            || b[i] == b'"'
            || (!ms && b[i] == b'`' && enabled(profile, "lexer.sql.backticks.identifier", false))
        {
            let q = b[i];
            i = quoted(
                b,
                i,
                q,
                !ms && enabled(profile, "sql.backslash.escapes", false),
                true,
            );
            style = if ms {
                if q == b'\'' { 4 } else { 8 }
            } else {
                match q {
                    b'\'' => 7,
                    b'"' => 6,
                    _ => 23,
                }
            };
        } else if ms && b[i] == b'[' {
            i = quoted(b, i, b']', false, true);
            style = 16;
        } else if ms && b[i] == b'@' {
            i += 1;
            let global = b.get(i) == Some(&b'@');
            if global {
                i += 1;
            }
            let name_start = i;
            while b.get(i).is_some_and(|c| word(*c)) {
                i += 1;
            }
            style = if global {
                if keyword(profile, 3, &text[name_start..i], true) {
                    12
                } else {
                    6
                }
            } else {
                7
            };
        } else if b[i].is_ascii_digit()
            || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
            || !ms
                && b"+-".contains(&b[i])
                && b.get(i + 1).is_some_and(u8::is_ascii_digit)
                && (i == 0 || !b[i - 1].is_ascii_digit())
        {
            i += 1;
            while b.get(i).is_some_and(|c| {
                if ms {
                    c.is_ascii_alphanumeric() || b"._".contains(c)
                } else {
                    c.is_ascii_digit()
                        || b".eE".contains(c)
                        || (b"+-".contains(c) && b"eE".contains(&b[i - 1]))
                }
            }) {
                i += 1;
            }
            style = if ms { 3 } else { 4 };
        } else if word(b[i]) {
            i += 1;
            while b.get(i).is_some_and(|c| {
                word(*c)
                    || *c == b'$'
                    || (!ms && *c == b'.' && enabled(profile, "lexer.sql.allow.dotted.word", false))
            }) {
                i += 1;
            }
            let token = &text[start..i];
            style = if ms {
                [(6, 5), (0, 9), (2, 11), (4, 13), (5, 14), (1, 10)]
                    .iter()
                    .find(|(index, _)| keyword(profile, *index, token, true))
                    .map_or(6, |(_, style)| *style)
            } else {
                [(0, 5), (1, 16), (4, 19), (5, 20), (6, 21), (7, 22)]
                    .iter()
                    .find(|(index, _)| keyword(profile, *index, token, true))
                    .map_or(11, |(_, style)| *style)
            };
            let lower = token.to_ascii_lowercase();
            if lower == "end" {
                if let Some(open) = blocks.pop() {
                    out.block(open, i);
                }
            } else if lower == "begin"
                || (!enabled(profile, "fold.sql.only.begin", false)
                    && matches!(lower.as_str(), "case" | "loop"))
            {
                blocks.push(start);
            }
            if !ms {
                let plus = profile.keywords(2).split_whitespace().any(|kw| {
                    let clean = kw.replace('~', "");
                    let min = kw.find('~').unwrap_or(clean.len());
                    token.len() >= min && clean.starts_with(&lower)
                });
                if plus {
                    style = 8;
                    if lower.starts_with("rem") || lower.starts_with("pro") {
                        out.paint(start, i, 8);
                        let end = line_end(b, i);
                        out.paint(i, end, if lower.starts_with("rem") { 13 } else { 9 });
                        i = end;
                        continue;
                    }
                }
            }
        } else {
            if b"+-*/%=<>!~^|&(),;.:[]{}".contains(&b[i]) {
                style = if ms { 5 } else { 10 };
            }
            i += 1;
        }
        out.paint(start, i, style);
        if style == 3 && !ms {
            let mut p = start + 3;
            while p < i {
                if b"@\\".contains(&b[p])
                    && (b[p - 1].is_ascii_whitespace() || b[p - 1] == b'*')
                    && b.get(p + 1).is_some_and(|c| !c.is_ascii_whitespace())
                {
                    let at = p;
                    p += 1;
                    while p < i && (b[p].is_ascii_lowercase() || b"$@\\&<>#{}[]".contains(&b[p])) {
                        p += 1;
                    }
                    let valid = b.get(p).is_some_and(u8::is_ascii_whitespace)
                        && keyword(profile, 3, &text[at + 1..p], true);
                    out.paint(at, p, if valid { 17 } else { 18 });
                } else {
                    p += 1;
                }
            }
        }
    }
    for open in blocks {
        out.block(open, text.len());
    }
    if ms {
        for range in lines(text) {
            let line = out.line(range.start);
            let mut indent = 0;
            for c in text[range].bytes() {
                if c == b' ' {
                    indent += 1;
                } else if c == b'\t' {
                    indent = (indent / 8 + 1) * 8;
                } else {
                    break;
                }
            }
            out.levels[line] = (indent, false);
        }
        for line in 0..out.levels.len().saturating_sub(1) {
            out.levels[line].1 = out.levels[line + 1].0 > out.levels[line].0;
        }
    }
}
