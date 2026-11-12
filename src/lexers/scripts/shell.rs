use super::{Scan, eol, first, ident};
use std::collections::VecDeque;

struct Here {
    delimiter: Vec<u8>,
    tabs: bool,
    expand: bool,
}

pub(super) fn bash(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut command = true;
    let mut pending = VecDeque::<Here>::new();
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if eol(c) {
            i = s.next_line(i);
            while let Some(here) = pending.pop_front() {
                let begin = i;
                while i < s.b.len() {
                    let end = s.line_end(i);
                    let mut content = i;
                    if here.tabs {
                        while s.at(content) == b'\t' {
                            content += 1;
                        }
                    }
                    if s.b[content..end] == here.delimiter {
                        s.paint(i, end, 12);
                        i = s.next_line(i);
                        break;
                    }
                    s.paint(i, s.next_line(i), 13);
                    if here.expand && s.prop("lexer.bash.styling.inside.heredoc", false) {
                        bash_expansions(s, i, end);
                    }
                    i = s.next_line(i);
                }
                s.fold(begin.saturating_sub(1), 1);
                s.fold(i.saturating_sub(1), -1);
            }
            command = true;
        } else if c == b'#'
            && (a == 0 || s.at(a - 1).is_ascii_whitespace() || b";|&()".contains(&s.at(a - 1)))
        {
            i = s.line_end(i);
            s.paint(a, i, 2);
        } else if s.has(i, b"<<") && !s.has(i, b"<<<") {
            s.paint(i, i + 2, 7);
            i += 2;
            let tabs = s.at(i) == b'-';
            if tabs {
                s.paint(i, i + 1, 7);
                i += 1;
            }
            i = s.skip_space(i);
            let start = i;
            let mut delimiter = Vec::new();
            let mut quoted = false;
            let mut quote = 0;
            while i < s.b.len() && !eol(s.at(i)) {
                let c = s.at(i);
                if quote == 0 && (c.is_ascii_whitespace() || b";|&<>".contains(&c)) {
                    break;
                }
                if matches!(c, b'\'' | b'"') && (quote == 0 || quote == c) {
                    quote = if quote == 0 { c } else { 0 };
                    quoted = true;
                    i += 1;
                } else if c == b'\\' && quote != b'\'' {
                    quoted = true;
                    i += 1;
                    if i < s.b.len() {
                        delimiter.push(s.at(i));
                        i += 1;
                    }
                } else {
                    delimiter.push(c);
                    i += 1;
                }
            }
            s.paint(start, i, 12);
            s.paint(a, i, 12);
            if !delimiter.is_empty() {
                pending.push_back(Here {
                    delimiter,
                    tabs,
                    expand: !quoted,
                });
            }
            command = false;
        } else if matches!(c, b'\'' | b'"' | b'`')
            || c == b'$' && matches!(s.at(i + 1), b'\'' | b'"')
        {
            let q = i + usize::from(c == b'$');
            let quote = s.at(q);
            i = s.quote(
                q,
                1,
                if quote == b'\'' && c != b'$' {
                    0
                } else {
                    b'\\'
                },
                true,
                false,
            );
            s.paint(
                a,
                i,
                if quote == b'`' {
                    11
                } else if quote == b'\'' && c != b'$' {
                    6
                } else {
                    5
                },
            );
            let prop = if quote == b'`' {
                "lexer.bash.styling.inside.backticks"
            } else {
                "lexer.bash.styling.inside.string"
            };
            if quote != b'\'' && s.prop(prop, false) {
                bash_expansions(s, q + 1, i.saturating_sub(1));
            }
            command = false;
        } else if c == b'$' {
            i = bash_expansion(s, i, s.b.len());
            command = false;
        } else if c.is_ascii_digit() {
            i = s.number(i);
            if s.at(i) == b'#' {
                i += 1;
                while ident(s.at(i)) || s.at(i) == b'@' {
                    i += 1;
                }
            }
            s.paint(a, i, 3);
            command = false;
        } else if first(c) || c == b'-' || c == b'#' {
            i += 1;
            while ident(s.at(i)) || matches!(s.at(i), b'-' | b'+' | b'.' | b'/') {
                i += 1;
            }
            let word = &s.text[a..i];
            let structural = matches!(
                word,
                "if" | "then"
                    | "else"
                    | "elif"
                    | "fi"
                    | "for"
                    | "while"
                    | "until"
                    | "do"
                    | "done"
                    | "case"
                    | "esac"
                    | "in"
                    | "function"
                    | "select"
            );
            let style = if s.kw(word, 0, false) && (command || structural) {
                4
            } else {
                8
            };
            s.paint(a, i, style);
            if structural {
                if matches!(word, "if" | "case" | "do") {
                    s.fold(a, 1);
                } else if matches!(word, "fi" | "esac" | "done") {
                    s.fold(a, -1);
                }
            }
            command = matches!(
                word,
                "then" | "else" | "elif" | "do" | "if" | "while" | "until"
            );
        } else if c == b'\\' {
            i = s.advance(i + 1);
            if s.has(a + 1, b"\r\n") {
                i += 1;
            }
            s.paint(a, i, 7);
        } else {
            i += 1;
            if b"|&;(){}[]<>!=:+-*/%~^,".contains(&c) {
                s.paint(a, i, 7);
                if c == b'{' {
                    s.fold(a, 1);
                } else if c == b'}' {
                    s.fold(a, -1);
                }
                if b"|&;({".contains(&c) {
                    command = true;
                }
            }
        }
    }
    if s.prop("fold.comment", false) {
        let mut line = 0;
        let mut group = None;
        let mut last_comment = 0;
        let mut count = 0;
        while line < s.b.len() {
            let start = s.skip_space(line);
            if s.at(start) == b'#' && s.styles.get(start).is_some_and(|&style| style & !0x40 == 2) {
                if group.is_none() {
                    group = Some(line);
                }
                last_comment = line;
                count += 1;
            } else if let Some(begin) = group.take() {
                if count > 1 {
                    s.fold(s.line_end(begin), 1);
                    s.fold(s.line_end(last_comment), -1);
                }
                count = 0;
            }
            line = s.next_line(line);
        }
        if let Some(begin) = group
            && count > 1
        {
            s.fold(s.line_end(begin), 1);
            s.fold(s.line_end(last_comment), -1);
        }
    }
}

fn bash_expansions(s: &mut Scan<'_>, start: usize, end: usize) {
    let mut i = start;
    while i < end {
        if s.at(i) == b'\\' {
            i += 2;
        } else if s.at(i) == b'$' {
            i = bash_expansion(s, i, end);
        } else if s.at(i) == b'`' {
            let j = s.quote(i, 1, b'\\', true, false).min(end);
            s.paint(i, j, 11);
            i = j;
        } else {
            i += 1;
        }
    }
}

fn bash_expansion(s: &mut Scan<'_>, start: usize, end: usize) -> usize {
    let c = s.at(start + 1);
    let special =
        s.p.property("lexer.bash.special.parameter")
            .filter(|v| !v.is_empty())
            .unwrap_or("*@#?-$!")
            .as_bytes()
            .contains(&c);
    let (mut stop, style) = if c == b'{' {
        (s.delimited(start + 1, b"{", b"}", true), 10)
    } else if c == b'(' {
        (
            s.delimited(start + 1, b"(", b")", true),
            if s.at(start + 2) == b'(' { 7 } else { 11 },
        )
    } else if c == b'[' {
        (s.delimited(start + 1, b"[", b"]", true), 7)
    } else if special && !ident(c) {
        (start + 2, 9)
    } else if ident(c) {
        (s.word(start + 1), 9)
    } else {
        (start + 1, 0)
    };
    stop = stop.min(end).max(start + 1);
    s.paint(start, stop, style);
    if c == b'(' && style == 7 {
        let begin = start + 3;
        let close = stop.saturating_sub(2);
        let mut i = begin;
        while i < close {
            let a = i;
            if s.at(i).is_ascii_digit() {
                i = s.number(i).min(close);
                s.paint(a, i, 3);
            } else if first(s.at(i)) {
                i = s.word(i).min(close);
                s.paint(a, i, 8);
            } else if s.at(i) == b'$' {
                i = bash_expansion(s, i, close);
            } else {
                i = s.advance(i);
                s.paint(a, i, if s.at(a).is_ascii_whitespace() { 0 } else { 7 });
            }
        }
    } else if c == b'(' && style == 11 && s.nesting < 64 {
        let mode =
            s.p.property("lexer.bash.command.substitution")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(0);
        if mode > 0 {
            let begin = start + 2;
            let close = if s.at(stop.saturating_sub(1)) == b')' {
                stop - 1
            } else {
                stop
            };
            if begin <= close && s.text.is_char_boundary(begin) && s.text.is_char_boundary(close) {
                let mut nested = Scan::new(&s.text[begin..close], s.p);
                nested.nesting = s.nesting + 1;
                bash(&mut nested);
                s.styles[begin..close].copy_from_slice(&nested.styles);
                s.paint(start, begin, 7);
                s.paint(close, stop, 7);
                if mode == 2 {
                    for style in &mut s.styles[start..stop] {
                        *style |= 0x40;
                    }
                }
            }
        }
    }
    if style == 10 && s.prop("lexer.bash.styling.inside.parameter", false) {
        bash_expansions(s, start + 2, stop.saturating_sub(1));
    }
    stop
}

pub(super) fn batch(s: &mut Scan<'_>) {
    let mut i = 0;
    let mut command = true;
    let mut check_words = true;
    let mut quote = 0;
    while i < s.b.len() {
        let a = i;
        let c = s.at(i);
        if s.bol(i) {
            let start = s.skip_space(i);
            if s.at(start) == b':' {
                let end = s.line_end(start);
                if s.at(start + 1) == b':' {
                    s.paint(i, end, 1);
                } else {
                    let mut j = start + 1;
                    while j < end && !s.at(j).is_ascii_whitespace() {
                        j += 1;
                    }
                    s.paint(i, j, 3);
                    s.paint(j, end, 8);
                }
                i = end;
                continue;
            }
        }
        if eol(c) {
            i = s.next_line(i);
            command = true;
            check_words = true;
            quote = 0;
        } else if c == b'^' {
            if eol(s.at(i + 1)) {
                i = s.next_line(i + 1);
            } else {
                i = s.advance(i + 1);
            }
        } else if c == b'@' && command {
            i += 1;
            s.paint(a, i, 4);
        } else if matches!(c, b'\'' | b'"') {
            if quote == c {
                quote = 0;
            } else if quote == 0 {
                quote = c;
            }
            i += 1;
        } else if c == b'%' || c == b'!' {
            i += 1;
            if c == b'%' && s.at(i) == b'%' {
                i += 1;
                if s.at(i) == b'~' {
                    i += 1;
                    while s.at(i).is_ascii_alphabetic() {
                        i += 1;
                    }
                }
                if ident(s.at(i)) {
                    i += 1;
                }
            } else if c == b'%' && (s.at(i).is_ascii_digit() || s.at(i) == b'*') {
                i += 1;
            } else if c == b'%' && s.at(i) == b'~' {
                i += 1;
                while ident(s.at(i)) || matches!(s.at(i), b'$' | b':') {
                    i += 1;
                }
            } else {
                while i < s.b.len() && !eol(s.at(i)) && s.at(i) != c {
                    i += 1;
                }
                if s.at(i) == c {
                    i += 1;
                } else {
                    i = a + 1;
                }
            }
            if i > a + 1 {
                s.paint(a, i, 6);
            }
        } else if b"=+><|?*&()".contains(&c) && quote == 0 {
            i += 1;
            s.paint(a, i, 7);
            if c == b'(' {
                s.fold(a, 1);
            } else if c == b')' {
                s.fold(a, -1);
            }
            if matches!(c, b'&' | b'|' | b'(') {
                command = true;
                check_words = true;
            }
        } else if first(c) {
            i = s.word(i);
            let word = s.text[a..i].to_ascii_lowercase();
            if command && word == "rem" && (s.at(i).is_ascii_whitespace() || s.at(i) == 0) {
                i = s.line_end(i);
                s.paint(a, i, 1);
                command = false;
            } else {
                let style = if check_words && s.kw(&word, 0, false) {
                    2
                } else if command && (s.p.keywords(1).trim().is_empty() || s.kw(&word, 1, false)) {
                    5
                } else {
                    0
                };
                s.paint(a, i, style);
                if matches!(word.as_str(), "echo" | "goto" | "prompt") {
                    check_words = false;
                }
                command = matches!(word.as_str(), "call" | "do" | "lh" | "loadhigh");
            }
        } else {
            i += 1;
        }
    }
}
