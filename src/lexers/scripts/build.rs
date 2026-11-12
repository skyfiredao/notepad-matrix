use super::{Scan, eol, ident};

pub(super) fn makefile(s: &mut Scan<'_>) {
    let mut line = 0;
    while line < s.b.len() {
        let end = s.line_end(line);
        let next = s.next_line(line);
        let start = s.skip_space(line);
        let command = s.at(line) == b'\t';
        if s.at(start) == b'#' || s.at(start) == b'!' {
            s.paint(line, next, if s.at(start) == b'#' { 1 } else { 2 });
            line = next;
            continue;
        }
        let mut i = start;
        if !s.b[line..end].iter().any(|c| matches!(c, b':' | b'=')) {
            let word = s.word(start);
            if s.kw(&s.text[start..word], 0, false) {
                s.paint(line, word, 2);
                i = word;
            }
        }
        let mut special = false;
        let mut last_nonspace = None;
        while i < end {
            if s.at(i) == b'#' && !command && (i == line || s.at(i - 1) != b'\\') {
                s.paint(i, next, 1);
                break;
            } else if s.has(i, b"$(") {
                let a = i;
                let mut depth = 1;
                i += 2;
                while i < end && depth > 0 {
                    if s.has(i, b"$(") {
                        depth += 1;
                        i += 2;
                    } else if s.at(i) == b')' {
                        depth -= 1;
                        i += 1;
                    } else {
                        i += 1;
                    }
                }
                s.paint(
                    a,
                    if depth > 0 { next } else { i },
                    if depth > 0 { 9 } else { 3 },
                );
                last_nonspace = Some(i.saturating_sub(1));
                if depth > 0 {
                    break;
                }
                continue;
            } else if !special && !command && matches!(s.at(i), b':' | b'=') {
                let immediate = s.has(i, b":=");
                if let Some(last) = last_nonspace {
                    s.paint(
                        line,
                        last + 1,
                        if s.at(i) == b':' && !immediate { 5 } else { 3 },
                    );
                }
                let width = if immediate { 2 } else { 1 };
                s.paint(i, i + width, 4);
                special = true;
                i += width;
                continue;
            }
            if !s.at(i).is_ascii_whitespace() {
                last_nonspace = Some(i);
            }
            i += 1;
        }
        line = next;
    }
}

fn cmake_word(s: &Scan<'_>, word: &str) -> u16 {
    match word.to_ascii_lowercase().as_str() {
        "macro" | "endmacro" => 12,
        "if" | "endif" | "else" | "elseif" => 11,
        "while" | "endwhile" => 9,
        "foreach" | "endforeach" => 10,
        _ => {
            if s.kw(word, 0, true) {
                5
            } else if s.kw(word, 1, false) {
                6
            } else if s.kw(word, 2, false) {
                8
            } else if !word.is_empty() && word.bytes().all(|c| c.is_ascii_digit()) {
                14
            } else {
                0
            }
        }
    }
}

pub(super) fn cmake(s: &mut Scan<'_>) {
    let mut i = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b'#' {
            i = s.line_end(i);
            while i > a && s.at(i - 1) == b'\\' && i < s.b.len() {
                s.paint(i - 1, i, 0);
                i = s.line_end(s.next_line(i));
            }
            s.paint(a, i, 1);
            let mut j = a;
            while j < i {
                if eol(s.at(j)) && j > a && s.at(j - 1) == b'\\' {
                    s.paint(j - 1, j, 0);
                }
                j += 1;
            }
        } else if matches!(c, b'"' | b'\'' | b'`') {
            i = cmake_quote(s, i);
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
            cmake_string_vars(s, a + 1, i.saturating_sub(1));
        } else if c == b'$' || ident(c) || c == b'.' {
            i += 1;
            let mut braces = 0;
            while i < s.b.len() {
                if s.at(i) == b'{' && (s.at(i - 1) == b'$' || braces > 0) {
                    braces += 1;
                    i += 1;
                } else if s.at(i) == b'}' && braces > 0 {
                    braces -= 1;
                    i += 1;
                } else if ident(s.at(i)) || matches!(s.at(i), b'.' | b'$') {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &s.text[a..i];
            let style =
                if word.len() > 3 && word.as_bytes().get(1) == Some(&b'{') && word.ends_with('}') {
                    7
                } else {
                    cmake_word(s, word)
                };
            s.paint(a, i, style);
            if matches!(style, 5 | 9..=12) {
                let lower = word.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "if" | "while" | "macro" | "foreach" | "function"
                ) {
                    s.fold(a, 1);
                } else if matches!(
                    lower.as_str(),
                    "endif" | "endwhile" | "endmacro" | "endforeach" | "endfunction"
                ) {
                    s.fold(a, -1);
                } else if matches!(lower.as_str(), "else" | "elseif")
                    && s.prop("fold.at.else", false)
                {
                    s.fold(a, -1);
                    s.fold(a + 1, 1);
                }
            }
        } else {
            i += 1;
        }
    }
}

fn cmake_quote(s: &Scan<'_>, start: usize) -> usize {
    let q = s.at(start);
    let mut i = start + 1;
    while i < s.b.len() {
        if s.at(i) == q && !(i >= start + 2 && s.has(i - 2, b"$\\")) {
            return i + 1;
        }
        if eol(s.at(i)) {
            let mut last = i;
            while last > start && matches!(s.at(last - 1), b' ' | b'\t') {
                last -= 1;
            }
            if last == start || s.at(last - 1) != b'\\' {
                return i;
            }
            i = s.next_line(i);
        } else {
            i = s.advance(i);
        }
    }
    i
}

fn cmake_string_vars(s: &mut Scan<'_>, start: usize, end: usize) {
    let mut i = start;
    while i < end {
        if s.has(i, b"${") {
            let stop = s.delimited(i + 1, b"{", b"}", true).min(end);
            s.paint(i, stop, 13);
            i = stop;
        } else {
            i += 1;
        }
    }
}
