use super::{Scan, eol, first, ident, interpolation, keyword_style};
use std::collections::VecDeque;

fn pair(c: u8) -> u8 {
    match c {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => c,
    }
}

fn quoted(s: &Scan<'_>, delimiter: usize, regex: bool) -> usize {
    if s.at(delimiter) >= 128
        && let Some(open) = s.text.get(delimiter..).and_then(|v| v.chars().next())
    {
        let close = unicode_pair(open);
        return unicode_delimited(s, delimiter, open, close);
    }
    let open = s.at(delimiter);
    let close = pair(open);
    let mut i = delimiter + 1;
    let mut depth = 1;
    let mut class = false;
    while i < s.b.len() {
        let c = s.at(i);
        if c == b'\\' {
            i = (i + 2).min(s.b.len());
        } else if regex && open == close && c == b'[' {
            class = true;
            i += 1;
        } else if regex && open == close && c == b']' {
            class = false;
            i += 1;
        } else if c == close && !class {
            depth -= 1;
            i += 1;
            if depth == 0 {
                return i;
            }
        } else if open != close && c == open {
            depth += 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    i
}

fn unicode_pair(open: char) -> char {
    match open {
        '‘' => '’',
        '“' | '„' => '”',
        '”' => '“',
        '｢' => '｣',
        '«' => '»',
        '‹' => '›',
        '（' => '）',
        '［' => '］',
        '｛' => '｝',
        '〈' => '〉',
        '《' => '》',
        '「' => '」',
        '『' => '』',
        '【' => '】',
        '〔' => '〕',
        '⟨' => '⟩',
        '⟪' => '⟫',
        _ => open,
    }
}

fn unicode_delimited(s: &Scan<'_>, start: usize, open: char, close: char) -> usize {
    let mut i = start + open.len_utf8();
    let mut depth = 1;
    while i < s.b.len() {
        let Some(ch) = s.text.get(i..).and_then(|v| v.chars().next()) else {
            i = s.advance(i);
            continue;
        };
        if ch == '\\' {
            i = s.advance(i + 1);
            continue;
        }
        if ch == close {
            depth -= 1;
            if depth == 0 {
                return i + ch.len_utf8();
            }
        } else if ch == open && open != close {
            depth += 1;
        }
        i += ch.len_utf8();
    }
    i
}

fn raku_word(s: &Scan<'_>, start: usize) -> usize {
    let mut i = start;
    while i < s.b.len() {
        if s.has(i, b"::") {
            i += 2;
            continue;
        }
        let Some(ch) = s.text.get(i..).and_then(|v| v.chars().next()) else {
            break;
        };
        if ch.is_alphanumeric()
            || matches!(ch, '_' | '-' | '\'')
            || matches!(ch, '\u{300}'..='\u{36f}')
        {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    i
}

struct Heredoc {
    delimiter: Vec<u8>,
    indent: bool,
    style: u16,
    variable: u16,
}

fn heredoc_start(s: &Scan<'_>, start: usize, ruby: bool) -> Option<(usize, Heredoc)> {
    let mut i = start + 2;
    let indent = s.at(i) == b'-' || s.at(i) == b'~';
    if indent {
        i += 1;
    }
    i = s.skip_space(i);
    let q = s.at(i);
    let quoted = matches!(q, b'\'' | b'"' | b'`');
    let begin = i + usize::from(quoted);
    i = begin;
    if quoted {
        while i < s.b.len() && !eol(s.at(i)) && s.at(i) != q {
            i += 1;
        }
    } else {
        i = s.word(i);
    }
    if i == begin || quoted && s.at(i) != q {
        return None;
    }
    let delimiter = s.b[begin..i].to_vec();
    if quoted {
        i += 1;
    }
    let style = match (ruby, q) {
        (true, b'\'') => 21,
        (true, b'`') => 23,
        (true, _) => 22,
        (false, b'\'') => 23,
        (false, b'`') => 25,
        _ => 24,
    };
    let variable = match (ruby, q) {
        (true, _) => 0,
        (false, b'\'') => 0,
        (false, b'`') => 62,
        _ => 61,
    };
    Some((
        i,
        Heredoc {
            delimiter,
            indent,
            style,
            variable,
        },
    ))
}

fn heredoc_body(
    s: &mut Scan<'_>,
    start: usize,
    queue: &mut VecDeque<Heredoc>,
    ruby: bool,
) -> usize {
    let mut i = start;
    while let Some(h) = queue.pop_front() {
        let begin = i;
        while i < s.b.len() {
            let end = s.line_end(i);
            let content = if h.indent { s.skip_space(i) } else { i };
            if s.b[content..end] == h.delimiter {
                s.paint(i, end, h.style);
                i = s.next_line(i);
                break;
            }
            s.paint(i, s.next_line(i), h.style);
            if ruby && h.style != 21 {
                ruby_interpolation(s, i, end);
            } else if h.variable != 0 {
                perl_interpolation(s, i, end, h.variable);
            }
            i = s.next_line(i);
        }
        s.fold(begin.saturating_sub(1), 1);
        s.fold(i.saturating_sub(1), -1);
    }
    i
}

fn perl_interpolation(s: &mut Scan<'_>, start: usize, end: usize, style: u16) {
    interpolation(s, start, end, b'$', style, b'\\');
    interpolation(s, start, end, b'@', style, b'\\');
}

fn perl_variable(s: &Scan<'_>, start: usize) -> usize {
    let mut i = start + 1;
    if s.at(i) == b'{' {
        return s.delimited(i, b"{", b"}", true);
    }
    if s.at(i) == b'^' {
        i += 1;
    }
    if !ident(s.at(i)) && s.at(i) != b':' {
        return (i + 1).min(s.b.len());
    }
    while ident(s.at(i)) || s.at(i) == b':' || s.at(i) == b'\'' {
        i += 1;
    }
    i
}

pub(super) fn perl(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut expression = true;
    let mut pending = VecDeque::new();
    let mut data = false;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if data {
            s.paint(i, s.b.len(), 21);
            break;
        }
        if eol(c) {
            i = s.next_line(i);
            i = heredoc_body(s, i, &mut pending, false);
        } else if c == b'#' {
            i = s.line_end(i);
            s.paint(a, i, 2);
        } else if s.bol(i) && c == b'=' && s.at(i + 1).is_ascii_alphabetic() {
            i = s.next_line(i);
            while i < s.b.len() && !s.has(i, b"=cut") {
                i = s.next_line(i);
            }
            if s.has(i, b"=cut") {
                i = s.line_end(i);
            }
            s.paint(a, i, 3);
            let mut j = s.next_line(a);
            while j < i {
                if matches!(s.at(j), b' ' | b'\t') {
                    s.paint(j, s.line_end(j).min(i), 31);
                }
                j = s.next_line(j);
            }
            if s.prop("fold.perl.pod", true) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if s.has(i, b"<<") && expression {
            if let Some((end, h)) = heredoc_start(s, i, false) {
                i = end;
                s.paint(a, i, 22);
                pending.push_back(h);
                expression = false;
            } else {
                i += 2;
                s.paint(a, i, 10);
                expression = true;
            }
        } else if matches!(c, b'\'' | b'"' | b'`') {
            i = s.quote(i, 1, b'\\', true, false);
            let style = if c == b'"' {
                6
            } else if c == b'\'' {
                7
            } else {
                20
            };
            s.paint(a, i, style);
            if c != b'\'' {
                perl_interpolation(
                    s,
                    a + 1,
                    i.saturating_sub(1),
                    if c == b'`' { 57 } else { 43 },
                );
            }
            expression = false;
        } else if matches!(c, b'$' | b'@' | b'%') && (expression || c != b'%' || ident(s.at(i + 1)))
        {
            i = perl_variable(s, i);
            s.paint(
                a,
                i,
                match c {
                    b'$' => 12,
                    b'@' => 13,
                    _ => 14,
                },
            );
            expression = false;
        } else if c == b'/' && expression {
            i = quoted(s, i, true);
            while s.at(i).is_ascii_alphabetic() {
                i += 1;
            }
            s.paint(a, i, 17);
            perl_interpolation(s, a + 1, i, 54);
            expression = false;
        } else if c.is_ascii_digit() || c == b'.' && s.at(i + 1).is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 4);
            expression = false;
        } else if first(c) {
            i = s.word(i);
            let word = s.text[a..i].to_owned();
            let delim = s.skip_space(i);
            let quote_style = match word.as_str() {
                "q" => 26,
                "qq" => 27,
                "qx" => 28,
                "qr" => 29,
                "qw" => 30,
                "m" => 17,
                "s" => 18,
                "tr" | "y" => 44,
                _ => 0,
            };
            if quote_style != 0
                && s.at(delim) != 0
                && !ident(s.at(delim))
                && !eol(s.at(delim))
                && s.at(delim) != b'='
            {
                let regex = matches!(quote_style, 17 | 18 | 29 | 44);
                i = quoted(s, delim, regex);
                if matches!(quote_style, 18 | 44) && i < s.b.len() {
                    if pair(s.at(delim)) != s.at(delim) {
                        let second = s.skip_space(i);
                        i = quoted(s, second, false);
                    } else {
                        i = quoted(s, i.saturating_sub(1), false);
                    }
                }
                if regex {
                    while s.at(i).is_ascii_alphabetic() {
                        i += 1;
                    }
                }
                s.paint(a, i, quote_style);
                let variable = match quote_style {
                    27 => 64,
                    28 => 65,
                    29 => 66,
                    17 => 54,
                    18 => 55,
                    _ => 0,
                };
                if variable != 0 {
                    perl_interpolation(s, delim + 1, i, variable);
                }
                expression = false;
            } else {
                while s.has(i, b"::") {
                    i = s.word(i + 2);
                }
                let style = if s.kw(&s.text[a..i], 0, false) { 5 } else { 11 };
                s.paint(a, i, style);
                if matches!(word.as_str(), "__DATA__" | "__END__") {
                    data = true;
                }
                expression = matches!(
                    word.as_str(),
                    "return"
                        | "print"
                        | "say"
                        | "grep"
                        | "map"
                        | "split"
                        | "if"
                        | "unless"
                        | "while"
                        | "until"
                        | "my"
                        | "our"
                        | "local"
                );
                if word == "package" && s.prop("fold.perl.package", false) {
                    s.fold(a, 1);
                }
            }
        } else {
            i += 1;
            if !c.is_ascii_whitespace() {
                s.paint(a, i, 10);
                expression = !b")]}.".contains(&c);
                if c == b'{' {
                    s.fold(a, 1);
                } else if c == b'}' {
                    s.fold(a, -1);
                }
            }
        }
    }
}

fn ruby_interpolation(s: &mut Scan<'_>, start: usize, end: usize) {
    let mut i = start;
    while i < end {
        if s.at(i) == b'\\' {
            i += 2;
        } else if s.has(i, b"#{") {
            let stop = s.expression_end(i + 1).min(end);
            s.paint(i, i + 2, 10);
            let begin = i + 2;
            let close = if stop > begin && s.at(stop - 1) == b'}' {
                stop - 1
            } else {
                stop
            };
            if s.nesting < 64 {
                let mut nested = Scan::new(&s.text[begin..close], s.p);
                nested.nesting = s.nesting + 1;
                ruby(&mut nested);
                s.styles[begin..close].copy_from_slice(&nested.styles);
            }
            s.paint(close, stop, 10);
            i = stop;
        } else if s.at(i) == b'#' && matches!(s.at(i + 1), b'$' | b'@') {
            let stop = perl_variable(s, i + 1).min(end);
            s.paint(i, i + 1, 10);
            s.paint(i + 1, stop, if s.at(i + 1) == b'$' { 13 } else { 16 });
            i = stop;
        } else {
            i += 1;
        }
    }
}

pub(super) fn ruby(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut expression = true;
    let mut expected = 0;
    let mut pending = VecDeque::new();
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if eol(c) {
            i = s.next_line(i);
            i = heredoc_body(s, i, &mut pending, true);
            expression = true;
        } else if s.bol(i) && s.has(i, b"__END__") && (eol(s.at(i + 7)) || s.at(i + 7) == 0) {
            s.paint(i, s.b.len(), 19);
            break;
        } else if s.bol(i) && s.has(i, b"=begin") && s.at(i + 6).is_ascii_whitespace() {
            i = s.next_line(i);
            while i < s.b.len() && !s.has(i, b"=end") {
                i = s.next_line(i);
            }
            if s.has(i, b"=end") {
                i = s.line_end(i);
            }
            s.paint(a, i, 3);
            if s.prop("fold.comment", false) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if c == b'#' {
            i = s.line_end(i);
            s.paint(a, i, 2);
        } else if s.has(i, b"<<") && expression {
            if let Some((end, h)) = heredoc_start(s, i, true) {
                i = end;
                s.paint(a, i, 20);
                pending.push_back(h);
                expression = false;
            } else {
                i += 2;
                s.paint(a, i, 10);
                expression = true;
            }
        } else if matches!(c, b'\'' | b'"' | b'`') {
            i = if c == b'\'' {
                s.quote(i, 1, b'\\', true, false)
            } else {
                s.interpolated_quote(i, 1, true)
            };
            s.paint(
                a,
                i,
                if c == b'"' {
                    6
                } else if c == b'\'' {
                    7
                } else {
                    18
                },
            );
            if c != b'\'' {
                ruby_interpolation(s, a + 1, i.saturating_sub(1));
            }
            expression = false;
        } else if c == b'%' && (expression || s.at(i + 1).is_ascii_alphabetic()) {
            let (delimiter, style, expand, regex) = match s.at(i + 1) {
                b'q' => (i + 2, 24, false, false),
                b'Q' => (i + 2, 25, true, false),
                b'x' => (i + 2, 26, true, false),
                b'r' => (i + 2, 27, true, true),
                b'w' => (i + 2, 41, false, false),
                b'W' => (i + 2, 28, true, false),
                b'i' => (i + 2, 42, false, false),
                b'I' => (i + 2, 43, true, false),
                b's' => (i + 2, 44, false, false),
                _ => (i + 1, 25, true, false),
            };
            if s.at(delimiter) != 0
                && !ident(s.at(delimiter))
                && !s.at(delimiter).is_ascii_whitespace()
            {
                i = quoted(s, delimiter, regex);
                if regex {
                    while s.at(i).is_ascii_alphabetic() {
                        i += 1;
                    }
                }
                s.paint(a, i, style);
                if expand {
                    ruby_interpolation(s, delimiter + 1, i.saturating_sub(1));
                }
                expression = false;
            } else {
                i += 1;
                s.paint(a, i, 10);
                expression = true;
            }
        } else if c == b'/' && expression {
            i = quoted(s, i, true);
            while s.at(i).is_ascii_alphabetic() {
                i += 1;
            }
            s.paint(a, i, 12);
            ruby_interpolation(s, a + 1, i);
            expression = false;
        } else if c == b'@' {
            i = s.word(i + 1 + usize::from(s.at(i + 1) == b'@'));
            s.paint(a, i, if s.at(a + 1) == b'@' { 17 } else { 16 });
            expression = false;
        } else if c == b'$' {
            i = perl_variable(s, i);
            s.paint(a, i, 13);
            expression = false;
        } else if c == b':'
            && s.at(i + 1) != b':'
            && (first(s.at(i + 1)) || matches!(s.at(i + 1), b'\'' | b'"'))
        {
            i = if matches!(s.at(i + 1), b'\'' | b'"') {
                s.quote(i + 1, 1, b'\\', true, false)
            } else {
                s.word(i + 1)
            };
            if matches!(s.at(i), b'?' | b'!') {
                i += 1;
            }
            s.paint(a, i, 14);
            expression = false;
        } else if c == b'?' && expression && s.at(i + 1) != 0 && !s.at(i + 1).is_ascii_whitespace()
        {
            i = s.advance(i + if s.at(i + 1) == b'\\' { 2 } else { 1 });
            s.paint(a, i, 7);
            expression = false;
        } else if c.is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 4);
            expression = false;
        } else if first(c) {
            i = s.word(i);
            if matches!(s.at(i), b'?' | b'!') {
                i += 1;
            }
            let word = s.text[a..i].to_owned();
            let style = if expected != 0 {
                expected
            } else {
                keyword_style(s, &word, &[5], 11, false)
            };
            s.paint(a, i, style);
            if style == 5
                && matches!(
                    word.as_str(),
                    "class"
                        | "module"
                        | "def"
                        | "if"
                        | "unless"
                        | "case"
                        | "while"
                        | "until"
                        | "for"
                        | "begin"
                        | "do"
                )
            {
                s.fold(a, 1);
            } else if style == 5 && word == "end" {
                s.fold(a, -1);
            }
            expected = match word.as_str() {
                "class" => 8,
                "module" => 15,
                "def" => 9,
                _ => 0,
            };
            expression = matches!(
                word.as_str(),
                "return"
                    | "yield"
                    | "puts"
                    | "print"
                    | "warn"
                    | "raise"
                    | "when"
                    | "if"
                    | "unless"
                    | "while"
                    | "until"
                    | "and"
                    | "or"
                    | "not"
                    | "then"
                    | "else"
                    | "do"
            );
        } else {
            i += 1;
            if !c.is_ascii_whitespace() {
                s.paint(a, i, 10);
                expression = !b")]}.".contains(&c);
                if c == b'{' {
                    s.fold(a, 1);
                } else if c == b'}' {
                    s.fold(a, -1);
                }
            }
        }
    }
}

pub(super) fn raku(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut expression = true;
    let mut expected = 0;
    let mut pending = VecDeque::<Heredoc>::new();
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if eol(c) {
            i = s.next_line(i);
            while let Some(h) = pending.pop_front() {
                while i < s.b.len() {
                    let end = s.line_end(i);
                    let begin = s.skip_space(i);
                    s.paint(i, s.next_line(i), h.style);
                    if s.b[begin..end] == h.delimiter {
                        i = s.next_line(i);
                        break;
                    }
                    if h.style == 7 {
                        raku_interpolation(s, i, end, false);
                    }
                    i = s.next_line(i);
                }
            }
        } else if s.has(i, b"#`") && b"([{<".contains(&s.at(i + 2)) {
            i = quoted(s, i + 2, false);
            s.paint(a, a + 1, 2);
            s.paint(a + 1, i, 3);
            if s.prop("fold.comment", false) && s.prop("fold.raku.comment.multiline", true) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if c == b'#' {
            i = s.line_end(i);
            s.paint(a, i, 2);
        } else if s.bol(i) && s.has(i, b"=begin pod") {
            i = s.next_line(i);
            while i < s.b.len() && !s.has(i, b"=end pod") {
                i = s.next_line(i);
            }
            if s.has(i, b"=end pod") {
                i = s.line_end(i);
            }
            s.paint(a, i, 4);
            if s.prop("fold.comment", false) && s.prop("fold.raku.comment.pod", true) {
                s.fold(a, 1);
                s.fold(i.saturating_sub(1), -1);
            }
        } else if let Some(open) = s
            .text
            .get(i..)
            .and_then(|v| v.chars().next())
            .filter(|ch| matches!(ch, '‘' | '“' | '”' | '„' | '｢'))
        {
            let close = unicode_pair(open);
            i = unicode_delimited(s, i, open, close);
            s.paint(a, i, 8);
            if matches!(open, '“' | '”' | '„') {
                raku_interpolation(
                    s,
                    a + open.len_utf8(),
                    i.saturating_sub(close.len_utf8()),
                    false,
                );
            }
            expression = false;
        } else if matches!(c, b'\'' | b'"') {
            i = s.quote(i, 1, b'\\', true, false);
            s.paint(a, i, 8);
            if c == b'"' {
                raku_interpolation(s, a + 1, i.saturating_sub(1), false);
            }
            expression = false;
        } else if matches!(c, b'$' | b'@' | b'%' | b'&')
            && (first(s.at(i + 1)) || b".!*?^:<~/=".contains(&s.at(i + 1)))
        {
            i += 1;
            let twigil = b".!*?^:<~".contains(&s.at(i));
            if twigil {
                i += 1;
            }
            while ident(s.at(i)) || matches!(s.at(i), b'-' | b'\'') || s.has(i, b"::") {
                i += if s.has(i, b"::") { 2 } else { 1 };
            }
            if s.at(a + 1) == b'<' && s.at(i) == b'>' {
                i += 1;
            }
            s.paint(
                a,
                i,
                match c {
                    b'$' => 23,
                    b'@' => 24,
                    b'%' => 25,
                    _ => 26,
                },
            );
            if twigil {
                s.paint(a + 1, a + 2, 18);
            }
            expression = false;
        } else if c == b'/' && expression {
            i = quoted(s, i, true);
            s.paint(a, i, 13);
            raku_interpolation(s, a + 1, i.saturating_sub(1), true);
            expression = false;
        } else if c == b':' && first(s.at(i + 1)) {
            i = s.word(i + 1);
            if s.kw(&s.text[a + 1..i], 6, false) {
                s.paint(a, i, 15);
            }
        } else if c.is_ascii_digit() || c == b'.' && s.at(i + 1).is_ascii_digit() {
            i = s.number(i);
            s.paint(a, i, 16);
            expression = false;
        } else if first(c) {
            i = raku_word(s, i).max(s.advance(i));
            let word = s.text[a..i].to_owned();
            let quote = match word.as_str() {
                "Q" => 11,
                "q" | "qw" | "qx" => 9,
                "qq" | "qqw" | "qqx" | "qqww" => 10,
                "rx" | "m" | "s" | "tr" | "y" => 13,
                _ => 0,
            };
            let mut delim = s.skip_space(i);
            let mut to = false;
            while s.at(delim) == b':' {
                let end = s.word(delim + 1);
                if end == delim + 1 {
                    break;
                }
                to |= &s.text[delim + 1..end] == "to";
                delim = s.skip_space(end);
            }
            let delimiter_valid = s
                .text
                .get(delim..)
                .and_then(|v| v.chars().next())
                .is_some_and(|ch| {
                    !ch.is_alphanumeric() && !ch.is_whitespace() && !";=)".contains(ch)
                });
            if quote != 0 && s.at(delim) != 0 && delimiter_valid {
                i = quoted(s, delim, quote == 13);
                if matches!(word.as_str(), "s" | "tr" | "y") && i < s.b.len() {
                    i = quoted(
                        s,
                        if pair(s.at(delim)) == s.at(delim) {
                            i.saturating_sub(1)
                        } else {
                            s.skip_space(i)
                        },
                        false,
                    );
                }
                s.paint(a, i, quote);
                let opener = s
                    .text
                    .get(delim..)
                    .and_then(|v| v.chars().next())
                    .unwrap_or('/');
                let begin = delim + opener.len_utf8();
                let end = i
                    .saturating_sub(unicode_pair(opener).len_utf8())
                    .max(begin)
                    .min(i);
                if quote == 10 || quote == 13 {
                    raku_interpolation(s, begin, end, quote == 13);
                }
                if to && i > begin {
                    pending.push_back(Heredoc {
                        delimiter: s.b[begin..end].to_vec(),
                        indent: true,
                        style: if quote == 10 { 7 } else { 6 },
                        variable: 0,
                    });
                }
                expression = false;
            } else {
                let style = keyword_style(
                    s,
                    &word,
                    &[19, 20, 22, 22, 22, 22],
                    if expected != 0 { expected } else { 21 },
                    false,
                );
                s.paint(a, i, style);
                expected = match word.as_str() {
                    "class" => 28,
                    "grammar" => 27,
                    _ => 0,
                };
                expression = matches!(
                    word.as_str(),
                    "say"
                        | "print"
                        | "return"
                        | "if"
                        | "unless"
                        | "when"
                        | "given"
                        | "my"
                        | "our"
                        | "has"
                );
            }
        } else {
            i += 1;
            if !c.is_ascii_whitespace() {
                s.paint(a, i, 18);
                expression = !b")]}.".contains(&c);
                if c == b'{' {
                    s.fold(a, 1);
                } else if c == b'}' {
                    s.fold(a, -1);
                }
            }
        }
    }
}

fn raku_interpolation(s: &mut Scan<'_>, start: usize, end: usize, regex: bool) {
    for marker in *b"$@%&" {
        interpolation(s, start, end, marker, if regex { 14 } else { 12 }, b'\\');
    }
}
