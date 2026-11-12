use super::{Scan, eol, first, ident, keyword_style};

pub(super) fn python(s: &mut Scan<'_>, gd: bool) {
    s.indent = true;
    let prefix = if gd {
        "lexer.gdscript."
    } else {
        "lexer.python."
    };
    let multiline = s.prop(&format!("{prefix}strings.over.newline"), false);
    let unicode = s.prop(&format!("{prefix}unicode.identifiers"), true);
    let mut i = 0;
    let mut expected = 0;
    let mut attribute = false;
    let mut decorator_line = false;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if c == b'#' {
            i = s.line_end(i);
            s.paint(a, i, if s.has(a, b"##") { 12 } else { 1 });
        } else if gd
            && (c == b'$'
                || c == b'%'
                    && (s.blank_prefix(i)
                        || a > 0
                            && b"=([{,:"
                                .contains(&s.at(s.text[..a].trim_end().len().saturating_sub(1)))))
        {
            i += 1;
            loop {
                if matches!(s.at(i), b'\'' | b'"') {
                    i = s.quote(i, 1, 0, true, false);
                } else if ident(s.at(i)) || matches!(s.at(i), b'/' | b'%') {
                    i += 1;
                } else {
                    break;
                }
            }
            s.paint(a, i, 16);
        } else if c == b'@' && (!gd || s.blank_prefix(i)) {
            decorator_line = true;
            i = s.word(i + 1);
            if !gd && !s.prop("lexer.python.decorator.attributes", false) {
                while s.at(i) == b'.' {
                    i = s.word(i + 1);
                }
            }
            s.paint(a, i, 15);
        } else {
            let mut q = i;
            let mut raw = false;
            let mut formatted = false;
            if !gd && let Some((quote, is_raw, is_formatted)) = python_prefix(s, i) {
                q = quote;
                raw = is_raw;
                formatted = is_formatted;
            }
            if matches!(s.at(q), b'\'' | b'"') {
                let triple = s.at(q + 1) == s.at(q) && s.at(q + 2) == s.at(q);
                let width = if triple { 3 } else { 1 };
                i = if formatted {
                    fstring_end(s, q, width, multiline || triple)
                } else {
                    s.quote(q, width, b'\\', multiline || triple, false)
                };
                let double = s.at(q) == b'"';
                let mut style = if triple {
                    if double { 7 } else { 6 }
                } else if double {
                    3
                } else {
                    4
                };
                if formatted {
                    style = match style {
                        3 => 16,
                        4 => 17,
                        6 => 18,
                        _ => 19,
                    };
                }
                if !triple && i > q && s.at(i.saturating_sub(1)) != s.at(q) && !multiline {
                    style = 13;
                }
                s.paint(a, i, style);
                if formatted {
                    f_expressions(s, q + width, i.saturating_sub(width), raw);
                }
                if triple
                    && s.prop(
                        if gd {
                            "fold.gdscript.quotes"
                        } else {
                            "fold.quotes.python"
                        },
                        false,
                    )
                {
                    s.fold(a, 1);
                    s.fold(i.saturating_sub(1), -1);
                }
                expected = 0;
            } else if c.is_ascii_digit() || c == b'.' && s.at(i + 1).is_ascii_digit() {
                i = s.number(i);
                s.paint(a, i, 2);
                if c == b'0'
                    && matches!(s.at(a + 1), b'b' | b'B' | b'o' | b'O')
                    && !s.prop(&format!("{prefix}literals.binary"), true)
                {
                    s.paint(a + 1, i, 11);
                }
            } else if first(c) && (unicode || c < 128) {
                i = s.word(i);
                let word = &s.text[a..i];
                let soft_identifier = !gd
                    && matches!(word, "match" | "case")
                    && (!s.blank_prefix(a)
                        || matches!(s.at(s.skip_space(i)), b'=' | b'#')
                        || s.at(s.skip_space(i)) == b'.'
                            && !s.at(s.skip_space(s.skip_space(i) + 1)).is_ascii_digit());
                let mut style = if s.kw(word, 0, false) && !soft_identifier {
                    5
                } else if expected != 0 {
                    expected
                } else if s.kw(word, 1, false) {
                    14
                } else {
                    11
                };
                if attribute
                    && style == 14
                    && s.prop(&format!("{prefix}keywords2.no.sub.identifiers"), false)
                {
                    style = 11;
                }
                if attribute && style == 11 && !gd {
                    if decorator_line && s.prop("lexer.python.decorator.attributes", false) {
                        style = 15;
                    } else if !decorator_line && s.prop("lexer.python.identifier.attributes", false)
                    {
                        style = 20;
                    }
                }
                s.paint(a, i, style);
                expected = if word == "class" {
                    8
                } else if word == "def" || gd && word == "func" {
                    9
                } else {
                    0
                };
                attribute = false;
            } else {
                i += 1;
                if b"+-*/%=<>!&|^~:;.,()[]{}\\@".contains(&c) {
                    s.paint(a, i, 10);
                    attribute = c == b'.';
                } else if !c.is_ascii_whitespace() {
                    expected = 0;
                    attribute = false;
                }
                if eol(c) {
                    expected = 0;
                    decorator_line = false;
                }
            }
        }
    }
}

fn python_prefix(s: &Scan<'_>, start: usize) -> Option<(usize, bool, bool)> {
    let c = s.at(start).to_ascii_lowercase();
    let next = s.at(start + 1).to_ascii_lowercase();
    let enabled = |c: u8| match c {
        b'b' => s.prop("lexer.python.strings.b", true),
        b'u' => s.prop("lexer.python.strings.u", true),
        b'f' => s.prop("lexer.python.strings.f", true),
        b't' => s.prop("lexer.python.strings.t", true),
        _ => false,
    };
    let is_quote = |c| matches!(c, b'\'' | b'"');
    let length = if (c == b'r' || enabled(c)) && is_quote(s.at(start + 1)) {
        1
    } else if ((enabled(c) && next == b'r') || c == b'r' && next != b'u' && enabled(next))
        && is_quote(s.at(start + 2))
    {
        2
    } else {
        return None;
    };
    Some((
        start + length,
        c == b'r' || length == 2 && next == b'r',
        matches!(c, b'f' | b't') || length == 2 && matches!(next, b'f' | b't'),
    ))
}

fn fstring_end(s: &Scan<'_>, start: usize, width: usize, multiline: bool) -> usize {
    let q = s.at(start);
    let mut i = start + width;
    while i < s.b.len() {
        if s.at(i) == b'\\' {
            i = s.advance(i + 1);
        } else if s.has(i, b"{{") || s.has(i, b"}}") {
            i += 2;
        } else if s.at(i) == b'{' {
            let mut depth = 1;
            i += 1;
            while i < s.b.len() && depth > 0 {
                if matches!(s.at(i), b'\'' | b'"') {
                    let triple = s.at(i + 1) == s.at(i) && s.at(i + 2) == s.at(i);
                    i = s.quote(i, if triple { 3 } else { 1 }, b'\\', triple, false);
                } else if s.at(i) == b'{' {
                    depth += 1;
                    i += 1;
                } else if s.at(i) == b'}' {
                    depth -= 1;
                    i += 1;
                } else if s.at(i) == b'#' && s.prop("lexer.python.strings.f.pep.701", true) {
                    i = s.line_end(i);
                } else {
                    i = s.advance(i);
                }
            }
        } else if (0..width).all(|n| s.at(i + n) == q) {
            return i + width;
        } else if !multiline && eol(s.at(i)) {
            return i;
        } else {
            i = s.advance(i);
        }
    }
    i
}

fn f_expressions(s: &mut Scan<'_>, start: usize, end: usize, raw: bool) {
    let mut i = start;
    while i < end {
        if s.has(i, b"{{") || s.has(i, b"}}") || !raw && s.at(i) == b'\\' {
            i += 2;
        } else if s.at(i) == b'{' {
            let stop = s.expression_end(i).min(end);
            s.paint(i, i + 1, 10);
            let mut j = i + 1;
            while j < stop {
                let a = j;
                if first(s.at(j)) {
                    j = s.word(j).min(stop);
                    let style = keyword_style(s, &s.text[a..j], &[5, 14], 11, false);
                    s.paint(a, j, style);
                } else if s.at(j).is_ascii_digit() {
                    j = s.number(j).min(stop);
                    s.paint(a, j, 2);
                } else if matches!(s.at(j), b'\'' | b'"') {
                    j = s.quote(j, 1, b'\\', false, false).min(stop);
                    s.paint(a, j, if s.at(a) == b'"' { 3 } else { 4 });
                } else if s.at(j) == b':' {
                    break;
                } else {
                    j += 1;
                    s.paint(a, j, if s.at(a).is_ascii_whitespace() { 0 } else { 10 });
                }
            }
            if stop > i && s.at(stop - 1) == b'}' {
                s.paint(stop - 1, stop, 10);
            }
            i = stop;
        } else {
            i += 1;
        }
    }
}

pub(super) fn coffee(s: &mut Scan<'_>) {
    s.indent = true;
    let mut i = 0;
    let mut expression = true;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if s.has(i, b"###") {
            i = s.delimited(i, b"###", b"###", false);
            s.paint(a, i, 22);
        } else if c == b'#' {
            i = s.line_end(i);
            s.paint(a, i, 2);
        } else if matches!(c, b'\'' | b'"') {
            let triple = s.at(i + 1) == c && s.at(i + 2) == c;
            i = if c == b'"' {
                s.interpolated_quote(i, if triple { 3 } else { 1 }, true)
            } else {
                s.quote(i, if triple { 3 } else { 1 }, b'\\', true, false)
            };
            s.paint(a, i, if c == b'"' { 6 } else { 7 });
            if c == b'"' {
                coffee_interpolation(s, a, i);
            }
            expression = false;
        } else if s.has(i, b"///") {
            i = s.delimited(i, b"///", b"///", false);
            while s.at(i).is_ascii_alphabetic() {
                i += 1;
            }
            s.paint(a, i, 23);
            let mut j = a + 3;
            while j < i {
                if s.at(j) == b'#' && s.at(j.saturating_sub(1)) != b'\\' {
                    let e = s.line_end(j).min(i);
                    s.paint(j, e, 24);
                    j = e;
                } else {
                    j += 1;
                }
            }
            coffee_interpolation(s, a, i);
            expression = false;
        } else if c == b'/' && expression && !s.has(i, b"/=") {
            i = regex_end(s, i);
            s.paint(a, i, 14);
            expression = false;
        } else if c == b'`' {
            i = s.quote(i, 1, b'\\', true, false);
            s.paint(a, i, 13);
            expression = false;
        } else if c == b'@' {
            i = s.word(i + 1);
            s.paint(a, i, 25);
            expression = false;
        } else if c.is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 4);
            expression = false;
        } else if first(c) || c == b'$' {
            i = s.word(i + 1);
            let word = &s.text[a..i];
            let style = keyword_style(s, word, &[5, 16, 0, 19], 11, false);
            s.paint(a, i, style);
            expression = matches!(
                word,
                "return"
                    | "throw"
                    | "new"
                    | "in"
                    | "of"
                    | "is"
                    | "isnt"
                    | "and"
                    | "or"
                    | "not"
                    | "then"
                    | "else"
            );
        } else {
            i += 1;
            if !c.is_ascii_whitespace() {
                s.paint(a, i, 10);
                expression = !b")]}.".contains(&c);
            }
            if eol(c) {
                expression = true;
            }
        }
    }
}

fn coffee_interpolation(s: &mut Scan<'_>, start: usize, end: usize) {
    let mut i = start;
    while i + 1 < end {
        if s.at(i) == b'\\' {
            i += 2;
        } else if s.has(i, b"#{") {
            let e = s.expression_end(i + 1).min(end);
            s.paint(i, i + 2, 10);
            let begin = i + 2;
            let close = if e > begin && s.at(e - 1) == b'}' {
                e - 1
            } else {
                e
            };
            if s.nesting < 64 {
                let mut nested = Scan::new(&s.text[begin..close], s.p);
                nested.nesting = s.nesting + 1;
                coffee(&mut nested);
                s.styles[begin..close].copy_from_slice(&nested.styles);
            }
            s.paint(close, e, 10);
            i = e;
        } else {
            i += 1;
        }
    }
}

fn regex_end(s: &Scan<'_>, start: usize) -> usize {
    let mut i = start + 1;
    let mut class = false;
    while i < s.b.len() && !eol(s.at(i)) {
        if s.at(i) == b'\\' {
            i += 2;
        } else if s.at(i) == b'/' && !class {
            i += 1;
            while s.at(i).is_ascii_alphabetic() {
                i += 1;
            }
            break;
        } else {
            if s.at(i) == b'[' {
                class = true;
            } else if s.at(i) == b']' {
                class = false;
            }
            i += 1;
        }
    }
    i.min(s.b.len())
}
