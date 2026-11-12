use super::{Scan, eol, first, ident, keyword_style};

pub(super) fn nncrontab(s: &mut Scan<'_>) {
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if s.has(i, b"#(") || s.has(i, b")#") {
            i = s.line_end(i);
            s.paint(a, i, 2);
        } else if c == b'#' || c == b'\\' && matches!(s.at(i + 1), b' ' | b'\t') {
            i = s.line_end(i);
            s.paint(a, i, 1);
        } else if c == b'"' {
            i = s.quote(i, 1, b'\\', false, false);
            s.paint(a, i, 8);
            let mut j = a + 1;
            while j < i {
                if s.at(j) == b'%' {
                    let end = env_end(s, j, i);
                    s.paint(j, end, 9);
                    j = end;
                } else {
                    j += 1;
                }
            }
        } else if c == b'%' || s.has(i, b"<%") {
            i = env_end(s, i, s.line_end(i));
            s.paint(a, i, 9);
        } else if c == b'*' {
            i += 1;
            s.paint(a, i, 6);
        } else if c.is_ascii_digit() {
            i += 1;
            while s.at(i).is_ascii_digit() {
                i += 1;
            }
            s.paint(a, i, 7);
        } else if c.is_ascii_alphabetic() || c == b'<' {
            i += 1;
            while ident(s.at(i)) || b"-/$ .<>@".contains(&s.at(i)) && s.at(i) != b' ' {
                i += 1;
            }
            let style = keyword_style(s, &s.text[a..i], &[3, 4, 5], 0, false);
            s.paint(a, i, style);
        } else {
            i += 1;
        }
    }
}

fn env_end(s: &Scan<'_>, start: usize, limit: usize) -> usize {
    let mut i = start + if s.at(start) == b'<' { 2 } else { 1 };
    while i < limit {
        if (s.at(i) == b'%' && s.at(i.saturating_sub(1)) != b'\\') || s.at(i) == b'>' {
            return i + 1;
        }
        i += 1;
    }
    i
}

pub(super) fn pascal(s: &mut Scan<'_>) {
    let smart = s.prop("lexer.pascal.smart.highlighting", true);
    let mut asm = false;
    let mut property = false;
    let mut export = false;
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b'{' || s.has(i, b"(*") {
            let curly = c == b'{';
            let directive = s.at(i + if curly { 1 } else { 2 }) == b'$';
            i = if curly {
                s.delimited(i, b"{", b"}", false)
            } else {
                s.delimited(i, b"(*", b"*)", false)
            };
            s.paint(
                a,
                i,
                match (curly, directive) {
                    (true, false) => 2,
                    (false, false) => 3,
                    (true, true) => 5,
                    _ => 6,
                },
            );
            if directive && s.prop("fold.preprocessor", false) {
                let word = s.text[a + if curly { 2 } else { 3 }..i].to_ascii_lowercase();
                if word.starts_with("if") || word.starts_with("region") {
                    s.fold(a, 1);
                } else if word.starts_with("endif")
                    || word.starts_with("ifend")
                    || word.starts_with("endregion")
                {
                    s.fold(a, -1);
                }
            } else if !directive && s.prop("fold.comment", false) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if s.has(i, b"//") {
            i = s.next_line(i);
            s.paint(a, i, 4);
        } else if c == b'\'' {
            let mut count = 1;
            while s.at(a + count) == b'\'' {
                count += 1;
            }
            if count >= 3 && count % 2 == 1 && eol(s.at(a + count)) {
                i = s.next_line(a + count);
                while i < s.b.len() {
                    let start = s.skip_space(i);
                    if (0..count).all(|n| s.at(start + n) == b'\'') {
                        i = start + count;
                        break;
                    }
                    i = s.next_line(i);
                }
                s.paint(a, i, 15);
            } else {
                i = s.quote(i, 1, 0, false, true);
                let closed = i > a + 1 && s.at(i - 1) == b'\'';
                if !closed {
                    i = s.next_line(i);
                }
                s.paint(a, i, if closed { 10 } else { 11 });
            }
        } else if c == b'#' {
            i += 1;
            while s.at(i).is_ascii_hexdigit() || s.at(i) == b'$' {
                i += 1;
            }
            s.paint(a, i, 12);
        } else if c == b'$' && !asm {
            i += 1;
            while s.at(i).is_ascii_hexdigit() {
                i += 1;
            }
            s.paint(a, i, 8);
        } else if c.is_ascii_digit() && !asm {
            i += 1;
            while s.at(i).is_ascii_digit()
                || matches!(s.at(i), b'e' | b'E')
                || s.at(i) == b'.' && s.at(i + 1) != b'.'
                || matches!(s.at(i), b'+' | b'-') && matches!(s.at(i - 1), b'e' | b'E')
            {
                i += 1;
            }
            s.paint(a, i, 7);
        } else if first(c) {
            i = s.word(i);
            let word = s.text[a..i].to_ascii_lowercase();
            let mut style = if asm { 14 } else { 1 };
            if s.kw(&word, 0, false) {
                let ignored = smart
                    && (word == "index" && !property && !export
                        || word == "name" && !export
                        || matches!(
                            word.as_str(),
                            "read"
                                | "write"
                                | "default"
                                | "nodefault"
                                | "stored"
                                | "implements"
                                | "readonly"
                                | "writeonly"
                                | "add"
                                | "remove"
                        ) && !property);
                if asm {
                    if word == "end" && (a == 0 || s.at(a - 1) != b'@') {
                        asm = false;
                        style = 9;
                    }
                } else if !ignored {
                    style = 9;
                    if word == "asm" {
                        asm = true;
                    }
                    if word == "property" {
                        property = true;
                    }
                    if word == "exports" {
                        export = true;
                    }
                }
            }
            s.paint(a, i, style);
            if style == 9 {
                if matches!(
                    word.as_str(),
                    "begin" | "asm" | "record" | "try" | "case" | "repeat"
                ) {
                    s.fold(a, 1);
                } else if matches!(word.as_str(), "end" | "until") {
                    s.fold(a, -1);
                }
            }
        } else {
            i += 1;
            if asm {
                s.paint(a, i, 14);
            } else if b"#$&'()*+,-./:;<=>@[]^{}".contains(&c) {
                s.paint(a, i, 13);
            }
            if c == b';' {
                property = false;
                export = false;
            }
        }
    }
}

fn rebol_end(s: &Scan<'_>, start: usize) -> usize {
    let mut i = start;
    while i < s.b.len() && !s.at(i).is_ascii_whitespace() && !b"[](){}\";".contains(&s.at(i)) {
        i += 1;
    }
    i
}

pub(super) fn rebol(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut comment = false;
    let mut header = false;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if !header {
            let mut j = i;
            while j < s.b.len() {
                if s.b
                    .get(j..j + 5)
                    .is_some_and(|v| v.eq_ignore_ascii_case(b"rebol"))
                    && s.at(s.skip_space(j + 5)) == b'['
                {
                    break;
                }
                j += 1;
            }
            s.paint(i, j, 3);
            i = j;
            header = true;
            if i == s.b.len() {
                break;
            }
            continue;
        }
        if c == b';' {
            i = s.line_end(i);
            s.paint(a, i, 1);
        } else if c == b'"' || s.has(i, b"#\"") {
            i = s.quote(i + usize::from(c == b'#'), 1, b'^', true, false);
            s.paint(a, i, if c == b'#' { 5 } else { 6 });
            comment = false;
        } else if c == b'{' {
            i = s.delimited(i, b"{", b"}", true);
            s.paint(a, i, if comment { 2 } else { 7 });
            comment = false;
        } else if s.has(i, b"#{") || s.has(i, b"2#{") || s.has(i, b"16#{") || s.has(i, b"64#{") {
            while s.at(i) != b'{' {
                i += 1;
            }
            i = s.delimited(i, b"{", b"}", false);
            s.paint(a, i, 11);
            comment = false;
        } else if c == b'<' && s.at(i + 1) != b'=' && !s.at(i + 1).is_ascii_whitespace() {
            i = s.delimited(i, b"<", b">", false);
            s.paint(a, i, 14);
            comment = false;
        } else if matches!(c, b'[' | b']' | b'(' | b')') {
            i += 1;
            if c == b'[' {
                s.fold(a, 1);
            } else if c == b']' {
                s.fold(a, -1);
            }
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else {
            i = rebol_end(s, i).max(i + 1);
            let word = &s.text[a..i];
            let mut style = if c == b'%' {
                15
            } else if c == b'#' {
                13
            } else if word.contains('$') {
                12
            } else if word.contains('@') {
                16
            } else if c.is_ascii_digit()
                || matches!(c, b'+' | b'-' | b'.' | b',') && s.at(a + 1).is_ascii_digit()
            {
                if word.contains('x') {
                    9
                } else if word.contains(':') {
                    19
                } else if word[1..].contains(['-', '/']) {
                    18
                } else if word.matches('.').count() >= 2 {
                    10
                } else {
                    8
                }
            } else if word.contains(':') && !word.ends_with(':') {
                17
            } else if word.bytes().all(|c| b"+-*/=<>?~|&".contains(&c)) {
                4
            } else {
                20
            };
            if style == 20 {
                let base = word.split('/').next().unwrap_or(word).to_ascii_lowercase();
                for set in 0..8 {
                    if s.kw(&base, set, false) {
                        style = 21 + set as u16;
                    }
                }
                comment = base == "comment";
            } else {
                comment = false;
            }
            s.paint(a, i, style);
        }
    }
}
