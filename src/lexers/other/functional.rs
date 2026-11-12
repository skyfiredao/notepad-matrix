use super::{Scan, operator, start, word};

pub(super) fn caml(s: &mut Scan<'_>) {
    let sml = s.kw(0, "andalso", false);
    while s.more() {
        if s.is("(*") {
            let mut depth = 0usize;
            let mut magic = false;
            while s.more() {
                if s.is("(*") {
                    depth += 1;
                    if s.flag("lexer.caml.magic", false) && s.is("(*@rc") {
                        magic = true;
                    }
                    s.take(
                        2,
                        12 + ((depth - 1) % 4) as u16 + if magic { 16 } else { 0 },
                    );
                } else if s.is("*)") {
                    s.take(
                        2,
                        12 + ((depth - 1) % 4) as u16 + if magic { 16 } else { 0 },
                    );
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else {
                    s.take(
                        1,
                        12 + ((depth - 1) % 4) as u16 + if magic { 16 } else { 0 },
                    );
                }
            }
        } else if s.ch() == b'"' {
            s.quote(b'"', 11, 11, true, false, !sml);
        } else if sml && s.is("#\"") {
            let a = s.i;
            s.i += 1;
            s.quote(b'"', 9, 9, true, false, false);
            s.styles[a] = 9;
        } else if !sml && s.ch() == b'\'' {
            let a = s.i;
            let mut end = a + 1;
            if s.at(end) == b'\\' {
                end += 2;
                while s.at(end).is_ascii_digit() {
                    end += 1;
                }
            } else {
                end += 1;
            }
            if s.at(end) == b'\'' {
                s.i = end + 1;
                s.paint(a, 9);
            } else {
                s.word(b"'");
                s.paint(a, 1);
            }
        } else if !sml && s.ch() == b'`' && start(s.next()) {
            let a = s.word(b"'");
            s.paint(a, 2);
        } else if !sml && s.ch() == b'#' && s.next().is_ascii_digit() {
            let a = s.i;
            s.i += 1;
            while s.ch().is_ascii_digit() {
                s.i += 1;
            }
            s.paint(a, 6);
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            let mut base = 10;
            s.i += 1;
            if s.at(a) == b'0' {
                base = match s.ch().to_ascii_lowercase() {
                    b'b' if !sml => 2,
                    b'o' if !sml => 8,
                    b'x' => 16,
                    _ => 10,
                };
                if base != 10 {
                    s.i += 1;
                }
            }
            while s.more() {
                let c = s.ch();
                if (c as char).is_digit(base)
                    || (!sml && c == b'_')
                    || (base == 10
                        && (c == b'.'
                            || matches!(c, b'e' | b'E')
                            || (matches!(c, b'+' | b'-' | b'~')
                                && matches!(s.at(s.i - 1), b'e' | b'E'))))
                    || (!sml && b"lLn".contains(&c))
                {
                    s.i += 1;
                } else {
                    break;
                }
            }
            s.paint(a, 8);
        } else if start(s.ch()) {
            let a = s.word(b"'");
            if s.token(a) == "_" {
                s.paint(a, 3);
            } else {
                s.classify(a, 1, &[3, 4, 5], false);
            }
        } else if s.is("()") || s.is("[]") {
            s.take(2, 3);
        } else if operator(s.ch()) || b"@#$\\`".contains(&s.ch()) {
            s.take(1, 7);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn haskell(s: &mut Scan<'_>) {
    let mut module = false;
    let mut import = false;
    while s.more() {
        if s.is("{-#") {
            s.i += 3;
            let a = s.i - 3;
            s.until("#-}", 17);
            s.styles[a..s.i].fill(17);
        } else if s.is("{-") {
            let a = s.i;
            let mut depth = 0usize;
            while s.more() {
                if s.is("{-") {
                    depth += 1;
                    s.take(2, 14 + ((depth - 1) % 3) as u16);
                } else if s.is("-}") {
                    s.take(2, 14 + ((depth - 1) % 3) as u16);
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else {
                    s.take(1, 14 + ((depth - 1) % 3) as u16);
                }
            }
            if s.flag("fold.comment", false) {
                s.region(a, s.i);
            }
        } else if s.is("--") && !b"!#$%&*+./<=>?@\\^|~:".contains(&s.at(s.i + 2)) {
            s.line(13);
        } else if s.ch() == b'#' && s.i == s.line_start() && s.flag("lexer.haskell.cpp", true) {
            if s.flag("styling.within.preprocessor", false) {
                let a = s.word(b"");
                s.paint(a, 18);
            } else {
                s.line(18);
            }
        } else if s.ch() == b'"' {
            s.quote(b'"', 4, 19, true, false, false);
        } else if s.ch() == b'\'' {
            if s.at(s.i + 2) == b'\'' || s.next() == b'\\' {
                s.quote(b'\'', 5, 19, true, false, false);
            } else if s.flag("lexer.haskell.allow.quotes", true)
                && (start(s.next()) || (s.next() == b'\'' && start(s.at(s.i + 2))))
            {
                let a = s.i;
                s.i += 1;
                if s.ch() == b'\'' {
                    s.i += 1;
                }
                let upper = s.ch().is_ascii_uppercase();
                s.word(b"'");
                s.paint(a, if upper { 8 } else { 1 });
            } else if !s.flag("lexer.haskell.allow.quotes", true) {
                s.quote(b'\'', 5, 19, true, false, false);
            } else {
                s.take(1, 11);
            }
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            let base = if s.is("0x") || s.is("0X") {
                16
            } else if s.is("0o") || s.is("0O") {
                8
            } else {
                10
            };
            if base != 10 {
                s.i += 2;
            }
            while (s.ch() as char).is_digit(base) {
                s.i += 1;
            }
            if base == 10 && s.ch() == b'.' && s.next().is_ascii_digit() {
                s.i += 1;
                while s.ch().is_ascii_digit() {
                    s.i += 1;
                }
            }
            if base == 10
                && matches!(s.ch(), b'e' | b'E')
                && (s.next().is_ascii_digit() || matches!(s.next(), b'+' | b'-'))
            {
                s.i += 1;
                if matches!(s.ch(), b'+' | b'-') {
                    s.i += 1;
                }
                while s.ch().is_ascii_digit() {
                    s.i += 1;
                }
            }
            if s.flag("lexer.haskell.allow.hash", true) {
                for _ in 0..2 {
                    if s.ch() == b'#' {
                        s.i += 1;
                    }
                }
            }
            s.paint(a, 3);
        } else if start(s.ch())
            || (s.ch() == b'?'
                && start(s.next())
                && s.flag("lexer.haskell.allow.questionmark", false))
        {
            let a = s.word(b"'");
            while s.ch() == b'.' && s.at(a).is_ascii_uppercase() && start(s.next()) {
                s.i += 1;
                s.word(b"'");
            }
            if s.flag("lexer.haskell.allow.hash", true) {
                while s.ch() == b'#' {
                    s.i += 1;
                }
            }
            let t = s.token(a).to_owned();
            let capital = t
                .rsplit('.')
                .next()
                .and_then(|part| part.bytes().next())
                .is_some_and(|c| c.is_ascii_uppercase());
            let mut style = if s.kw(0, &t, false) {
                2
            } else if capital {
                8
            } else {
                1
            };
            if module && capital {
                style = 7;
                module = false;
            }
            if import
                && (matches!(t.as_str(), "qualified" | "as" | "hiding")
                    || (t == "safe" && s.flag("lexer.haskell.import.safe", true)))
            {
                style = 2;
                if t == "as" {
                    module = true;
                }
            }
            if t == "module" {
                module = true;
            }
            if t == "import" {
                import = true;
                module = true;
            }
            s.paint(a, style);
        } else if b"!#$%&*+./<=>?@\\^|-~:".contains(&s.ch()) {
            let a = s.i;
            while s.more() && b"!#$%&*+./<=>?@\\^|-~:".contains(&s.ch()) {
                s.i += 1;
            }
            let fallback = if s.token(a).starts_with(':') && s.token(a) != "::" {
                8
            } else {
                11
            };
            s.classify(a, fallback, &[fallback, fallback, 20], false);
        } else if b"()[]{};,".contains(&s.ch()) {
            s.take(1, 11);
        } else {
            if matches!(s.ch(), b'\r' | b'\n') {
                import = false;
            }
            s.take(1, 0);
        }
    }
    indent_folds(s);
}

pub(super) fn indent_folds(s: &mut Scan<'_>) {
    if !s.flag("fold", true) {
        return;
    }
    let mut previous: Option<(usize, usize)> = None;
    let mut stack: Vec<usize> = Vec::new();
    let mut offset = 0;
    let text = s.text;
    let lines: Vec<_> = crate::lexers::shared::line_ranges(text)
        .into_iter()
        .map(|range| &text[range])
        .collect();
    let mut imports = Vec::new();
    for line in lines {
        let indent = line
            .bytes()
            .take_while(|c| matches!(c, b' ' | b'\t'))
            .fold(0, |n, c| if c == b'\t' { (n / 8 + 1) * 8 } else { n + 1 });
        s.indent_levels.push(indent as u32);
        let first = offset
            + line
                .bytes()
                .take_while(|c| matches!(c, b' ' | b'\t'))
                .count();
        let style = s.styles.get(first).copied().unwrap_or(0);
        let comment = match s.p.key() {
            "haskell" => matches!(style, 13..=16),
            "nim" => matches!(style, 1 | 15),
            "vb" => style == 1,
            _ => false,
        };
        if !line.trim().is_empty() && !comment {
            if let Some((prev, amount)) = previous {
                if indent > amount {
                    stack.push(indent);
                    s.event(prev, 1);
                }
                while stack.last().is_some_and(|&n| n > indent) {
                    stack.pop();
                    s.event(offset, -1);
                }
            }
            previous = Some((offset, indent));
        }
        if s.p.key() == "haskell" && s.flag("fold.haskell.imports", false) {
            let import = line
                .trim_start()
                .strip_prefix("import")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace));
            if import {
                imports.push(offset);
            } else if !line.trim().is_empty() && !comment && imports.len() > 1 {
                s.event(imports[0], 1);
                s.event(*imports.last().unwrap_or(&offset), -1);
                imports.clear();
            }
        }
        offset += line.len();
    }
    if imports.len() > 1 {
        s.event(imports[0], 1);
        if let Some(&last) = imports.last() {
            s.event(last, -1);
        }
    }
}

pub(super) fn lisp(s: &mut Scan<'_>) {
    let mut quoted = false;
    while s.more() {
        if s.ch() == b';' {
            s.line(1);
        } else if s.is("#|") {
            s.block("#|", "|#", false, 12);
        } else if s.ch() == b'"' {
            s.quote(b'"', 6, 6, true, false, true);
        } else if s.is("#\\") {
            let a = s.i;
            s.i += 2;
            if s.more() {
                s.i += 1;
            }
            while s.more() && !s.ch().is_ascii_whitespace() && !b"()[]{}".contains(&s.ch()) {
                s.i += 1;
            }
            s.paint(a, 11);
        } else if b"'`()[]{}".contains(&s.ch()) {
            quoted = s.ch() == b'\'' && s.next() != b'(';
            s.delimit(10, b"([{", b")]}");
        } else if !s.ch().is_ascii_whitespace() && s.ch() != b',' {
            let a = s.i;
            while s.more() && !s.ch().is_ascii_whitespace() && !b"'`()[]{};\"".contains(&s.ch()) {
                s.i += 1;
            }
            let t = s.token(a);
            let number = lisp_number(t);
            let style = if quoted || t.starts_with(':') {
                5
            } else if s.kw(0, t, false) {
                3
            } else if s.kw(1, t, false) {
                4
            } else if number {
                2
            } else if t.starts_with('#')
                || ((t.starts_with('*') && t.ends_with('*'))
                    || (t.starts_with('+') && t.ends_with('+')))
            {
                11
            } else {
                9
            };
            s.paint(a, style);
            quoted = false;
        } else {
            s.take(1, 0);
        }
    }
}

fn lisp_number(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit() || c == b'.')
}

pub(super) fn erlang(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b'%' {
            let a = s.i;
            let style = if s.is("%%%") {
                15
            } else if s.is("%%") {
                14
            } else {
                1
            };
            s.line(style);
            let end = s.i;
            let mut p = a;
            while p < end {
                if s.at(p) == b'@' || (s.at(p) == b'{' && s.at(p + 1) == b'@') {
                    let start = p;
                    if s.at(p) == b'{' {
                        p += 1;
                    }
                    p += 1;
                    while p < end && word(s.at(p)) {
                        p += 1;
                    }
                    let macro_doc = s.at(start) == b'{';
                    let token_start = start + usize::from(macro_doc);
                    let token = s.text.get(token_start..p).unwrap_or("");
                    if style != 1 && s.kw(if macro_doc { 5 } else { 4 }, token, false) {
                        if macro_doc {
                            while p < end && s.at(p) != b'}' {
                                p += 1;
                            }
                        }
                        s.styles[token_start..p].fill(if macro_doc { 17 } else { 16 });
                    }
                } else {
                    p += 1;
                }
            }
        } else if s.ch() == b'"' {
            s.quote(b'"', 5, 5, true, false, true);
        } else if s.ch() == b'\'' {
            let a = s.i;
            s.quote(b'\'', 18, 31, true, false, true);
            if s.token(a).contains('@') {
                s.paint(a, 21);
            }
        } else if s.ch() == b'$' {
            let a = s.i;
            s.i += 1;
            if s.ch() == b'\\' {
                s.i += 1;
            }
            if s.more() {
                s.i += 1;
            }
            s.paint(a, 9);
        } else if matches!(s.ch(), b'?' | b'#') && (start(s.next()) || s.next() == b'\'') {
            let a = s.i;
            let style = if s.ch() == b'?' { 10 } else { 11 };
            s.i += 1;
            if s.ch() == b'\'' {
                s.quote(b'\'', style + 9, style + 9, true, false, true);
                s.paint(a, style + 9);
            } else {
                s.word(b"@");
                s.paint(a, style);
            }
        } else if s.ch() == b'-' && start(s.next()) {
            let a = s.word(b"");
            let t = s.token(a);
            let style = if s.kw(2, t, false) {
                12
            } else if s.kw(3, t, false) {
                24
            } else {
                0
            };
            s.paint(a, style);
        } else if s.ch().is_ascii_digit()
            || (matches!(s.ch(), b'+' | b'-') && s.next().is_ascii_digit())
        {
            let a = s.i;
            if matches!(s.ch(), b'+' | b'-') {
                s.i += 1;
            }
            let digits = s.i;
            while s.ch().is_ascii_digit() {
                s.i += 1;
            }
            let base = s
                .text
                .get(digits..s.i)
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(10);
            let mut valid = true;
            if s.ch() == b'#' {
                s.i += 1;
                if (2..=36).contains(&base) {
                    while (s.ch() as char).is_digit(base) {
                        s.i += 1;
                    }
                    valid = !s.ch().is_ascii_alphanumeric();
                } else {
                    valid = false;
                }
            } else {
                if s.ch() == b'.' && s.next().is_ascii_digit() {
                    s.i += 1;
                    while s.ch().is_ascii_digit() {
                        s.i += 1;
                    }
                }
                if matches!(s.ch(), b'e' | b'E') {
                    s.i += 1;
                    if matches!(s.ch(), b'+' | b'-') {
                        s.i += 1;
                    }
                    let exponent = s.i;
                    while s.ch().is_ascii_digit() {
                        s.i += 1;
                    }
                    valid = s.i > exponent;
                }
            }
            s.paint(a, if valid { 3 } else { 31 });
        } else if start(s.ch()) {
            let a = s.word(b"@");
            let t = s.token(a);
            let mut style = if s.at(a).is_ascii_uppercase() || s.at(a) == b'_' {
                2
            } else if t.contains('@') {
                13
            } else {
                7
            };
            if s.kw(0, t, false) {
                style = 4;
            } else if s.kw(1, t, false) {
                style = 22;
            } else if style == 7 && (s.ch() == b'(' || s.ch() == b'/') {
                style = 8;
            }
            if style == 7 && s.ch() == b':' && start(s.next()) {
                s.i += 1;
                style = 23;
            }
            let t = s.token(a);
            let opening = style == 4 && matches!(t, "case" | "if" | "receive" | "query" | "fun");
            let closing = style == 4 && t == "end";
            s.paint(a, style);
            if opening {
                s.event(a, 1);
            }
            if closing {
                s.event(a, -1);
            }
        } else if operator(s.ch()) {
            s.delimit(6, b"([{", b")]}");
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn nim(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b'#' {
            let style = if s.is("##") { 15 } else { 1 };
            s.line(style);
            if s.more() {
                s.take(1, style);
            }
        } else if s.is("\"\"\"") {
            let a = s.i;
            s.i += 3;
            s.until("\"\"\"", 7);
            s.paint(a, 7);
            s.region(a, s.i);
        } else if matches!(s.ch(), b'"' | b'\'' | b'`') {
            let c = s.ch();
            let style = if c == b'`' {
                11
            } else if c == b'\'' {
                4
            } else {
                3
            };
            s.quote(c, style, style, c != b'`', false, false);
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            let base = if s.ch() == b'0' {
                match s.next().to_ascii_lowercase() {
                    b'b' => 2,
                    b'o' | b'c' => 8,
                    b'x' => 16,
                    _ => 10,
                }
            } else {
                10
            };
            s.i += 1;
            if base != 10 {
                s.i += 1;
            }
            while s.more()
                && ((s.ch() as char).is_digit(base)
                    || s.ch() == b'_'
                    || (base == 10
                        && ((s.ch() == b'.' && s.next().is_ascii_digit())
                            || matches!(s.ch(), b'e' | b'E')
                            || (matches!(s.ch(), b'+' | b'-')
                                && matches!(s.at(s.i - 1), b'e' | b'E')))))
            {
                s.i += 1;
            }
            if s.ch() == b'\'' {
                s.word(b"");
            }
            s.paint(a, 2);
        } else if start(s.ch()) {
            let a = s.word(b"");
            if s.ch() == b'"' && s.token(a).eq_ignore_ascii_case("r") {
                s.quote(b'"', 3, 3, false, false, false);
                s.paint(a, 3);
            } else {
                let normalized = s.token(a).replace('_', "").to_ascii_lowercase();
                let style = if s.kw(0, &normalized, false) { 5 } else { 11 };
                s.paint(a, style);
            }
        } else if operator(s.ch()) || b"@$\\".contains(&s.ch()) {
            s.take(1, 10);
        } else {
            s.take(1, 0);
        }
    }
    indent_folds(s);
}

pub(super) fn smalltalk(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b'"' {
            s.quote(b'"', 3, 3, false, false, true);
        } else if s.ch() == b'\'' {
            s.quote(b'\'', 1, 1, false, true, true);
        } else if s.ch() == b'#' {
            let a = s.i;
            s.i += 1;
            if b"()[]{};.^:".contains(&s.ch()) {
                s.paint(a, 12);
            } else if s.ch() == b'\'' {
                s.quote(b'\'', 4, 4, false, true, true);
                s.paint(a, 4);
            } else {
                while s.more() && (word(s.ch()) || b":~@%&*-+=|\\/,<>?!".contains(&s.ch())) {
                    s.i += 1;
                }
                s.paint(a, 4);
            }
        } else if s.ch() == b'$' {
            s.take(if s.next() == 0 { 1 } else { 2 }, 15);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'-' && s.next().is_ascii_digit()) {
            let a = s.i;
            if s.ch() == b'-' {
                s.i += 1;
            }
            let digits = s.i;
            while s.ch().is_ascii_digit() {
                s.i += 1;
            }
            let mut base = 10;
            if s.ch() == b'r' {
                base = s
                    .text
                    .get(digits..s.i)
                    .and_then(|n| n.parse::<u32>().ok())
                    .filter(|n| (2..=36).contains(n))
                    .unwrap_or(10);
                s.i += 1;
                if s.ch() == b'-' {
                    s.i += 1;
                }
                while (s.ch() as char).is_digit(base) {
                    s.i += 1;
                }
            }
            if s.ch() == b'.' && (s.next() as char).is_digit(base) {
                s.i += 1;
                while (s.ch() as char).is_digit(base) {
                    s.i += 1;
                }
            }
            if s.ch() == b's' {
                s.i += 1;
                while s.ch().is_ascii_digit() {
                    s.i += 1;
                }
            } else if matches!(s.ch(), b'e' | b'd' | b'q') {
                s.i += 1;
                if matches!(s.ch(), b'+' | b'-') {
                    s.i += 1;
                }
                while (s.ch() as char).is_digit(base) {
                    s.i += 1;
                }
            }
            s.paint(a, 2);
        } else if start(s.ch()) {
            let a = s.word(b"");
            let keyword_send = s.ch() == b':' && s.next() != b'=';
            if keyword_send {
                s.i += 1;
            }
            let t = s.token(a);
            let style = if s.kw(0, t, false) {
                16
            } else if keyword_send {
                13
            } else if s.at(a).is_ascii_uppercase() {
                10
            } else {
                match t {
                    "self" => 7,
                    "super" => 8,
                    "nil" => 9,
                    "true" | "false" => 6,
                    _ => 0,
                }
            };
            s.paint(a, style);
        } else if s.is(":=") {
            s.take(2, 14);
        } else if s.ch() == b'^' {
            s.take(1, 11);
        } else if b"()[]{};.:".contains(&s.ch()) {
            s.take(1, 12);
        } else if b"~@%&*-+=|\\/,<>?!".contains(&s.ch()) {
            s.take(1, 5);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn visualprolog(s: &mut Scan<'_>) {
    while s.more() {
        if s.is("/*") || s.ch() == b'%' {
            let a = s.i;
            if s.is("/*") {
                s.block("/*", "*/", true, 4);
            } else {
                s.line(5);
            }
            let end = s.i;
            let mut i = a;
            while i < end {
                if s.at(i) == b'@' && start(s.at(i + 1)) {
                    let begin = i;
                    i += 1;
                    while i < end && word(s.at(i)) {
                        i += 1;
                    }
                    let t = s.text.get(begin + 1..i).unwrap_or("");
                    let style = if s.kw(3, t, false) { 6 } else { 7 };
                    s.styles[begin..i].fill(style);
                } else {
                    i += 1;
                }
            }
        } else if s.is("[|") {
            embedded(s);
        } else if s.is("{|") {
            s.block("{|", "|}", false, 24);
        } else if matches!(s.ch(), b'"' | b'\'')
            || (s.ch() == b'`' && s.flag("lexer.visualprolog.backquoted.strings", false))
            || (s.ch() == b'@'
                && s.flag("lexer.visualprolog.verbatim.strings", true)
                && b"\"'`([{<".contains(&s.next()))
        {
            let verbatim = s.ch() == b'@';
            if verbatim {
                s.take(1, 16);
            }
            let quote = s.ch();
            let close = match quote {
                b'(' => b')',
                b'[' => b']',
                b'{' => b'}',
                b'<' => b'>',
                c => c,
            };
            s.take(1, 16);
            while s.more() {
                if s.ch() == close {
                    if verbatim && s.next() == close {
                        s.take(2, 17);
                    } else {
                        s.take(1, 16);
                        break;
                    }
                } else if !verbatim && s.ch() == b'\\' {
                    let a = s.i;
                    s.i += 1;
                    let mut valid = b"\"'\\nlrt".contains(&s.ch());
                    if s.ch() == b'u' {
                        s.i += 1;
                        let digits = s.i;
                        while s.ch().is_ascii_hexdigit() && s.i - digits < 4 {
                            s.i += 1;
                        }
                        valid = s.i - digits == 4;
                    } else if s.more() {
                        s.i += 1;
                    }
                    s.paint(a, if valid { 17 } else { 18 });
                } else if matches!(s.ch(), b'\r' | b'\n') {
                    s.take(1, if verbatim { 22 } else { 18 });
                    if !verbatim {
                        break;
                    }
                } else {
                    s.take(1, 20);
                }
            }
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            s.number(11, b".");
        } else if s.ch() == b'#' {
            let a = s.word(b"");
            let t = s.token(a).strip_prefix('#').unwrap_or("");
            s.paint(a, if s.kw(2, t, false) { 3 } else { 8 });
        } else if start(s.ch()) {
            let a = s.word(b"");
            let style = if s.at(a) == b'_' {
                10
            } else if s.at(a).is_ascii_uppercase() {
                9
            } else {
                8
            };
            if style == 8 {
                s.classify(a, style, &[1, 2], false);
            } else {
                s.paint(a, style);
            }
        } else if operator(s.ch()) || b"\\@".contains(&s.ch()) {
            s.delimit(12, b"{", b"}");
        } else {
            s.take(1, 0);
        }
    }
}

fn embedded(s: &mut Scan<'_>) {
    s.take(2, 23);
    let mut depth = 1;
    while s.more() {
        if s.is("[|") {
            depth += 1;
            s.take(2, 23);
        } else if s.is("|]") {
            depth -= 1;
            s.take(2, 23);
            if depth == 0 {
                break;
            }
        } else if s.is("{|") {
            s.block("{|", "|}", false, 24);
        } else if s.is("/*") {
            s.block("/*", "*/", true, 4);
        } else {
            s.take(1, 23);
        }
    }
}
