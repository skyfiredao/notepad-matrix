use super::{Scan, eol, first, ident, keyword_style};

pub(super) fn inno(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut code = false;
    let mut messages = false;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b'[' && s.blank_prefix(i) {
            let mut end = i + 1;
            while ident(s.at(end)) {
                end += 1;
            }
            if s.at(end) == b']' {
                let name = s.text[i + 1..end].to_ascii_lowercase();
                if s.kw(&name, 0, false) {
                    s.paint(a, end + 1, 4);
                    code = name == "code";
                    messages = matches!(name.as_str(), "messages" | "custommessages");
                    if a > 0 {
                        s.fold(a, -1);
                    }
                    s.fold(a + 1, 1);
                }
                i = end + 1;
            } else {
                i += 1;
            }
        } else if c == b'#' && s.blank_prefix(i) {
            i = s.word(i + 1);
            let word = &s.text[a + 1..i];
            if s.kw(word, 3, true) {
                s.paint(a, i, 5);
            }
        } else if !code && c == b';' && s.blank_prefix(i) {
            i = s.line_end(i);
            s.paint(a, i, 1);
        } else if code && (c == b'{' || s.has(i, b"(*") || s.has(i, b"//")) {
            i = if c == b'{' {
                s.delimited(i, b"{", b"}", false)
            } else if s.has(i, b"(*") {
                s.delimited(i, b"(*", b"*)", false)
            } else {
                s.line_end(i)
            };
            s.paint(a, i, 7);
        } else if !code && c == b'{' && s.at(i + 1) != b'{' && (a == 0 || s.at(a - 1) != b'{') {
            i = s.delimited(i, b"{", b"}", false).min(s.line_end(i));
            if i > a && s.at(i - 1) == b'}' {
                s.paint(a, i, 6);
            }
        } else if !messages && matches!(c, b'\'' | b'"') {
            i = s.quote(i, 1, 0, false, false);
            s.paint(a, i, if c == b'"' { 10 } else { 11 });
        } else if !messages && first(c) && c < 128 {
            i = s.word(i);
            let word = s.text[a..i].to_ascii_lowercase();
            let next = s.at(s.skip_space(i));
            let style = if code && s.kw(&word, 4, false) {
                8
            } else if !code && next == b'=' && s.kw(&word, 1, false) {
                2
            } else if !code && next == b':' && s.kw(&word, 2, false) {
                3
            } else if !code && s.kw(&word, 5, false) {
                9
            } else {
                0
            };
            s.paint(a, i, style);
            if code && style == 8 {
                if word == "begin" {
                    s.fold(a, 1);
                } else if word == "end" {
                    s.fold(a, -1);
                }
            }
        } else {
            i += 1;
        }
    }
}

pub(super) fn kix(s: &mut Scan<'_>) {
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b';' {
            i = s.line_end(i);
            s.paint(a, i, 1);
        } else if s.has(i, b"/*") {
            i = s.delimited(i, b"/*", b"*/", false);
            s.paint(a, i, 10);
        } else if matches!(c, b'\'' | b'"') {
            i = s.quote(i, 1, 0, true, false);
            s.paint(a, i, if c == b'"' { 2 } else { 3 });
        } else if c == b'$' {
            i = s.word(i + 1);
            s.paint(a, i, 5);
        } else if c == b'@' {
            i = s.word(i + 1);
            let style = if s.kw(&s.text[a + 1..i], 2, true) {
                6
            } else {
                0
            };
            s.paint(a, i, style);
        } else if c.is_ascii_digit() || matches!(c, b'.' | b'&') && s.at(i + 1).is_ascii_digit() {
            i += 1;
            while s.at(i).is_ascii_digit() {
                i += 1;
            }
            s.paint(a, i, 4);
        } else if first(c) {
            i = s.word(i);
            let style = keyword_style(s, &s.text[a..i], &[7, 8], 31, true);
            s.paint(a, i, style);
        } else {
            i += 1;
            if b"+-*/&|=<>!()[],".contains(&c) {
                s.paint(a, i, 9);
            }
        }
    }
}

fn nsis_style(s: &Scan<'_>, word: &str, insensitive: bool) -> u16 {
    let cmp = |key: &str| {
        if insensitive {
            key.eq_ignore_ascii_case(word)
        } else {
            key == word
        }
    };
    let special: &[(&[&str], u16)] = &[
        (&["!macro", "!macroend"], 12),
        (
            &[
                "!if",
                "!ifdef",
                "!ifndef",
                "!endif",
                "!else",
                "!ifmacrodef",
                "!ifmacrondef",
            ],
            11,
        ),
        (&["Section", "SectionEnd"], 9),
        (&["SubSection", "SubSectionEnd"], 10),
        (&["SectionGroup", "SectionGroupEnd"], 15),
        (&["PageEx", "PageExEnd"], 16),
        (&["Function", "FunctionEnd"], 17),
    ];
    for (words, style) in special {
        if words.iter().any(|w| cmp(w)) {
            return *style;
        }
    }
    let style = keyword_style(s, word, &[5, 6, 7, 8], 0, insensitive);
    if style != 0 {
        return style;
    }
    if word.len() > 3 && word.as_bytes().get(1) == Some(&b'{') && word.ends_with('}') {
        return 6;
    }
    if word.starts_with('$')
        && s.prop("nsis.uservars", false)
        && word.as_bytes()[1..].iter().all(|&c| ident(c) || c == b'.')
    {
        return 6;
    }
    if !word.is_empty() && word.bytes().all(|c| c.is_ascii_digit()) {
        return 14;
    }
    0
}

pub(super) fn nsis(s: &mut Scan<'_>) {
    let insensitive = s.prop("nsis.ignorecase", false);
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if matches!(c, b'#' | b';') {
            i = s.line_end(i);
            s.paint(a, i, 1);
        } else if s.has(i, b"/*") {
            i = s.delimited(i, b"/*", b"*/", false);
            s.paint(a, i, 18);
            if s.prop("fold.comment", false) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if matches!(c, b'\'' | b'"' | b'`') {
            i += 1;
            while i < s.b.len() {
                if s.has(i, b"$\\") {
                    i = (i + 3).min(s.b.len());
                } else if s.at(i) == c {
                    i += 1;
                    break;
                } else if eol(s.at(i)) {
                    let mut j = i;
                    while j > a && matches!(s.at(j - 1), b' ' | b'\t') {
                        j -= 1;
                    }
                    if j == a || s.at(j - 1) != b'\\' {
                        break;
                    }
                    i = s.next_line(i);
                } else {
                    i += 1;
                }
            }
            s.paint(
                a,
                i,
                if c == b'"' {
                    2
                } else if c == b'`' {
                    3
                } else {
                    4
                },
            );
            let mut j = a + 1;
            while j < i {
                if s.has(j, b"$\\") {
                    j += 3;
                } else if s.at(j) == b'$' {
                    let end = if s.at(j + 1) == b'{' {
                        s.delimited(j + 1, b"{", b"}", false).min(i)
                    } else {
                        s.word(j + 1).min(i)
                    };
                    if nsis_style(s, &s.text[j..end], insensitive) == 6 {
                        s.paint(j, end, 13);
                    }
                    j = end.max(j + 1);
                } else {
                    j += 1;
                }
            }
        } else if ident(c) || matches!(c, b'$' | b'!' | b'.') {
            i += 1;
            if c == b'$' && s.at(i) == b'{' {
                i = s.delimited(i, b"{", b"}", false);
            } else {
                while ident(s.at(i)) || s.at(i) == b'.' {
                    i += 1;
                }
            }
            let word = &s.text[a..i];
            let style = nsis_style(s, word, insensitive);
            s.paint(a, i, style);
            if matches!(style, 9..=12 | 15..=17)
                && (!matches!(style, 11 | 12) || s.prop("nsis.foldutilcmd", true))
            {
                let lower = word.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "sectionend"
                        | "subsectionend"
                        | "sectiongroupend"
                        | "pageexend"
                        | "functionend"
                        | "!endif"
                        | "!macroend"
                ) {
                    s.fold(a, -1);
                } else if lower == "!else" {
                    if s.prop("fold.at.else", false) {
                        s.fold(a, -1);
                        s.fold(a + 1, 1);
                    }
                } else {
                    s.fold(a, 1);
                }
            }
        } else {
            i += 1;
        }
    }
}
