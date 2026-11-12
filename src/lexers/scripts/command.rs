use super::{Scan, eol, first, ident, keyword_style};

pub(super) fn powershell(s: &mut Scan<'_>) {
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if s.has(i, b"<#") {
            i = s.delimited(i, b"<#", b"#>", false);
            s.paint(a, i, 13);
            let mut j = s.next_line(a);
            while j < i {
                let start = s.skip_space(j);
                if s.at(start) == b'.' {
                    let end = s.word(start + 1).min(i);
                    if s.kw(&s.text[start + 1..end], 5, true) {
                        s.paint(start, end, 16);
                    }
                }
                j = s.next_line(j);
            }
            if s.prop("fold.comment", false) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if c == b'#' {
            i = s.line_end(i);
            s.paint(a, i, 1);
            if s.prop("fold.comment", false) {
                let word = s.text[a + 1..i].trim_start();
                if word.starts_with("region") {
                    s.fold(a, 1);
                } else if word.starts_with("endregion") {
                    s.fold(a, -1);
                }
            }
        } else if c == b'@' && matches!(s.at(i + 1), b'\'' | b'"') {
            let quote = s.at(i + 1);
            i += 2;
            while i < s.b.len() {
                if s.bol(i) && s.at(i) == quote && s.at(i + 1) == b'@' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            s.paint(a, i, if quote == b'"' { 14 } else { 15 });
        } else if matches!(c, b'\'' | b'"') {
            i = s.quote(i, 1, if c == b'"' { b'`' } else { 0 }, true, false);
            s.paint(a, i, if c == b'"' { 2 } else { 3 });
        } else if c == b'$' {
            i += 1;
            while ident(s.at(i)) || matches!(s.at(i), b':' | b'-') {
                i += 1;
            }
            s.paint(a, i, 5);
        } else if c == b'`' {
            i = s.advance(i + 1);
        } else if c.is_ascii_digit() || c == b'.' && s.at(i + 1).is_ascii_digit() {
            i = s.number(i);
            let word = s.text[a..i].to_ascii_lowercase();
            let valid = word
                .bytes()
                .all(|c| c.is_ascii_digit() || b"abcdefx.pnulsydtgbkm+-".contains(&c));
            s.paint(a, i, if valid { 4 } else { 7 });
        } else if first(c) || c == b'-' {
            i += 1;
            while ident(s.at(i)) || matches!(s.at(i), b'-' | b':') {
                i += 1;
            }
            let style = keyword_style(s, &s.text[a..i], &[8, 9, 10, 11, 12], 7, true);
            s.paint(a, i, style);
        } else {
            i += 1;
            if b"+-*/%=<>!&|^~:;.,()[]{}?@".contains(&c) {
                s.paint(a, i, 6);
                if c == b'{' {
                    s.fold(a, 1);
                } else if c == b'}' {
                    s.fold(a, -1);
                }
            }
        }
    }
}

pub(super) fn tcl(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut expected = true;
    let mut quote = false;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b'\\' {
            i = s.advance(i + 1);
            if s.has(a + 1, b"\r\n") {
                i += 1;
            }
            s.paint(a, i, if quote { 5 } else { 0 });
        } else if c == b'"' {
            quote = !quote;
            i += 1;
            s.paint(a, i, 5);
            expected = false;
        } else if quote {
            i += 1;
            s.paint(
                a,
                i,
                if matches!(c, b'$' | b'[' | b']') {
                    6
                } else {
                    5
                },
            );
            if c == b'[' {
                expected = true;
            }
        } else if c == b'#' && (expected || s.blank_prefix(i)) {
            let style = if s.has(i, b"#~") {
                21
            } else if s.bol(i) && matches!(s.at(i + 1), b'#' | b'-') {
                20
            } else if s.blank_prefix(i) {
                2
            } else {
                1
            };
            i = s.line_end(i);
            while i > a && s.at(i - 1) == b'\\' && i < s.b.len() {
                i = s.line_end(s.next_line(i));
            }
            s.paint(a, i, style);
        } else if c == b'$' {
            if s.at(i + 1) == b'{' {
                i = s.delimited(i + 1, b"{", b"}", false);
                s.paint(a, a + 2, 6);
                s.paint(a + 2, i.saturating_sub(1), 9);
                s.paint(i.saturating_sub(1), i, 6);
            } else {
                i = s.word(i + 1);
                while s.at(i) == b':' {
                    i = s.word(i + 1);
                }
                s.paint(a, i, 8);
                if s.at(i) == b'(' {
                    let end = s.delimited(i, b"(", b")", true);
                    s.paint(i, end, 8);
                    s.paint(i, i + 1, 6);
                    if s.at(end.saturating_sub(1)) == b')' {
                        s.paint(end - 1, end, 6);
                    }
                    i = end;
                }
            }
            expected = false;
        } else if c.is_ascii_digit() || c == b'-' && s.at(i + 1).is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 3);
            expected = false;
        } else if first(c) || matches!(c, b':' | b'-') {
            i += 1;
            while ident(s.at(i)) || matches!(s.at(i), b':' | b'-') {
                i += 1;
            }
            let word = s.text[a..i].trim_start_matches(':');
            let mut style = if c == b'-' { 10 } else { 7 };
            if expected {
                style = keyword_style(s, word, &[12, 13, 14, 15, 11, 16, 17, 18, 19], style, false);
                if style == 11 && !(a > 0 && s.at(a - 1) == b'{' && s.at(i) == b'}') {
                    style = 7;
                }
            }
            s.paint(a, i, style);
            expected = false;
        } else {
            i += 1;
            if b"{}[]()".contains(&c) {
                s.paint(a, i, 6);
            }
            if c == b'{' {
                s.fold(a, 1);
            } else if c == b'}' {
                s.fold(a, -1);
            }
            if matches!(c, b'{' | b'}' | b'[' | b';') || eol(c) {
                expected = true;
            }
        }
    }
}

fn lua_long(s: &Scan<'_>, start: usize) -> Option<usize> {
    if s.at(start) != b'[' {
        return None;
    }
    let mut j = start + 1;
    while s.at(j) == b'=' {
        j += 1;
    }
    if s.at(j) != b'[' {
        return None;
    }
    let mut close = vec![b']'];
    close.extend(std::iter::repeat_n(b'=', j - start - 1));
    close.push(b']');
    let mut end = j + 1;
    while end < s.b.len() {
        if s.has(end, &close) {
            return Some(end + close.len());
        }
        end += 1;
    }
    Some(end)
}

pub(super) fn lua(s: &mut Scan<'_>) {
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if s.has(i, b"--") {
            if let Some(end) = lua_long(s, i + 2) {
                i = end;
                s.paint(a, i, 1);
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            } else {
                i = s.line_end(i);
                s.paint(a, i, 2);
            }
        } else if let Some(end) = lua_long(s, i) {
            i = end;
            s.paint(a, i, 8);
            s.fold(a, 1);
            s.fold(i.saturating_sub(1), -1);
        } else if matches!(c, b'\'' | b'"') {
            i = lua_quote(s, i);
            let closed = i > a + 1 && s.at(i - 1) == c;
            s.paint(
                a,
                i,
                if !closed {
                    12
                } else if c == b'"' {
                    6
                } else {
                    7
                },
            );
        } else if c.is_ascii_digit() || c == b'.' && s.at(i + 1).is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 4);
        } else if s.has(i, b"::") {
            let begin = s.skip_space(i + 2);
            let end = s.word(begin);
            let close = s.skip_space(end);
            if s.has(close, b"::") && !s.kw(&s.text[begin..end], 0, false) {
                s.paint(a, a + 2, 20);
                s.paint(begin, end, 20);
                s.paint(close, close + 2, 20);
                i = close + 2;
            } else {
                i += 2;
                s.paint(a, i, 10);
            }
        } else if first(c) {
            i = s.word(i);
            let word = &s.text[a..i];
            let style = keyword_style(s, word, &[5, 13, 14, 15, 16, 17, 18, 19], 11, false);
            s.paint(a, i, style);
            if style == 5 {
                if matches!(word, "then" | "do" | "function" | "repeat") {
                    s.fold(a, 1);
                } else if matches!(word, "end" | "until" | "elseif") {
                    s.fold(a, -1);
                }
            }
        } else if c == b'$' && s.blank_prefix(i) {
            i = s.line_end(i);
            s.paint(a, i, 9);
        } else {
            i += 1;
            if b"+-*/%=<>~^#&|:;.,()[]{}".contains(&c) {
                s.paint(a, i, 10);
                if c == b'{' {
                    s.fold(a, 1);
                } else if c == b'}' {
                    s.fold(a, -1);
                }
            }
        }
    }
}

fn lua_quote(s: &Scan<'_>, start: usize) -> usize {
    let q = s.at(start);
    let mut i = start + 1;
    while i < s.b.len() {
        if s.has(i, b"\\z") {
            i += 2;
            while s.at(i).is_ascii_whitespace() {
                i += 1;
            }
        } else if s.at(i) == b'\\' {
            i = s.advance(i + 1);
            if s.has(i.saturating_sub(1), b"\r\n") {
                i += 1;
            }
        } else if s.at(i) == q {
            return i + 1;
        } else if eol(s.at(i)) {
            return i;
        } else {
            i = s.advance(i);
        }
    }
    i
}

pub(super) fn autoit(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut include = false;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b';' {
            i = s.line_end(i);
            s.paint(a, i, 1);
        } else if c == b'#' {
            i += 1;
            while ident(s.at(i)) || s.at(i) == b'-' {
                i += 1;
            }
            let word = s.text[a..i].to_ascii_lowercase();
            if matches!(word.as_str(), "#cs" | "#comments-start") {
                while i < s.b.len() {
                    if s.at(i) == b'#' {
                        let end = {
                            let mut j = i + 1;
                            while ident(s.at(j)) || s.at(j) == b'-' {
                                j += 1;
                            }
                            j
                        };
                        if matches!(
                            s.text[i..end].to_ascii_lowercase().as_str(),
                            "#ce" | "#comments-end"
                        ) {
                            i = end;
                            break;
                        }
                    }
                    i += 1;
                }
                s.paint(a, i, 2);
                if s.prop("fold.comment", false) {
                    s.fold(a, 1);
                    s.fold(i.saturating_sub(1), -1);
                }
            } else {
                let style = if s.kw(&word, 4, false) {
                    11
                } else if s.kw(&word, 5, false) {
                    12
                } else {
                    0
                };
                if style == 12 {
                    i = s.line_end(i);
                }
                s.paint(a, i, style);
                include = word == "#include";
                if s.prop("fold.preprocessor", false) {
                    if word == "#region" {
                        s.fold(a, 1);
                    } else if word == "#endregion" {
                        s.fold(a, -1);
                    }
                }
            }
        } else if matches!(c, b'\'' | b'"') || include && c == b'<' {
            i = if c == b'<' {
                s.delimited(i, b"<", b">", false).min(s.line_end(i))
            } else {
                autoit_quote(s, i)
            };
            s.paint(a, i, 7);
            let mut j = a + 1;
            while j < i {
                if s.at(j) == b'{' {
                    let end = s.delimited(j, b"{", b"}", false).min(i);
                    let key = s.text[j..end].to_ascii_lowercase();
                    let single = key.len() == 3;
                    let normalized = key
                        .split_ascii_whitespace()
                        .next()
                        .map(|k| format!("{}{}", k.trim_end_matches('}'), "}"));
                    if single
                        || s.kw(&key, 3, false)
                        || normalized.is_some_and(|k| s.kw(&k, 3, false))
                    {
                        s.paint(j, end, 10);
                    }
                    j = end;
                } else {
                    j += 1;
                }
            }
            include = false;
        } else if c == b'$' {
            i = s.word(i + 1);
            s.paint(a, i, 9);
        } else if c == b'@' {
            i = s.word(i + 1);
            let style = if s.kw(&s.text[a..i], 2, true) { 6 } else { 0 };
            s.paint(a, i, style);
        } else if c.is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 3);
        } else if first(c) {
            i = s.word(i);
            let word = s.text[a..i].to_ascii_lowercase();
            let style = if a > 0 && s.at(a - 1) == b'.' {
                14
            } else if word == "_" {
                8
            } else if s.kw(&word, 0, false) {
                5
            } else if s.kw(&word, 1, false) {
                4
            } else if s.kw(&word, 6, false) {
                13
            } else if s.kw(&word, 7, false) {
                15
            } else {
                0
            };
            s.paint(a, i, style);
            if style == 5 {
                if matches!(
                    word.as_str(),
                    "func" | "for" | "while" | "do" | "select" | "switch" | "with"
                ) || word == "if"
                    && s.text[i..s.line_end(i)]
                        .trim_end()
                        .to_ascii_lowercase()
                        .ends_with("then")
                {
                    s.fold(a, 1);
                } else if matches!(
                    word.as_str(),
                    "endfunc"
                        | "next"
                        | "wend"
                        | "until"
                        | "endselect"
                        | "endswitch"
                        | "endwith"
                        | "endif"
                ) {
                    s.fold(a, -1);
                }
            }
        } else {
            i += 1;
            if b"+-*/&^=<>(),.[]".contains(&c) {
                s.paint(a, i, 8);
            }
            if eol(c) {
                include = false;
            }
        }
    }
}

fn autoit_quote(s: &Scan<'_>, start: usize) -> usize {
    let q = s.at(start);
    let mut i = start + 1;
    while i < s.b.len() {
        if s.at(i) == q {
            return i + 1;
        }
        if eol(s.at(i)) {
            let mut last = i;
            while last > start && matches!(s.at(last - 1), b' ' | b'\t') {
                last -= 1;
            }
            if last == start || s.at(last - 1) != b'_' {
                return i;
            }
            i = s.next_line(i);
        } else {
            i = s.advance(i);
        }
    }
    i
}
