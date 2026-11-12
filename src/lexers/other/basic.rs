use super::{Scan, operator, start};

pub(super) fn basic(s: &mut Scan<'_>) {
    let free = s.p.key() == "freebasic";
    let comment = if free { b'\'' } else { b';' };
    while s.more() {
        if s.ch() == comment {
            let a = s.i;
            let style = if free && s.next() == b'$' {
                5
            } else if free && matches!(s.next(), b'*' | b'!') {
                20
            } else {
                1
            };
            s.line(style);
            if style == 20 {
                doc_keywords(s, a);
            }
        } else if s.is("/'") {
            let a = s.i;
            let style = if matches!(s.at(s.i + 2), b'*' | b'!') {
                21
            } else {
                19
            };
            s.block("/'", "'/", false, style);
            if style == 21 {
                doc_keywords(s, a);
            }
        } else if s.ch() == b'"' {
            s.quote(b'"', 4, 16, false, false, false);
        } else if !free && s.ch() == b'.' && s.first() {
            let a = s.word(b"");
            s.paint(a, 15);
        } else if s.ch() == b'#' && !s.first() {
            let a = s.word(b"");
            s.paint(a, 13);
        } else if s.ch() == b'$' || (s.ch() == b'&' && b"hHoO".contains(&s.next())) {
            let a = s.i;
            s.i += if s.ch() == b'$' { 1 } else { 2 };
            while s.ch().is_ascii_hexdigit() {
                s.i += 1;
            }
            s.paint(a, 17);
        } else if s.ch() == b'%' || (s.ch() == b'&' && matches!(s.next(), b'b' | b'B')) {
            let a = s.i;
            s.i += if s.ch() == b'%' { 1 } else { 2 };
            while matches!(s.ch(), b'0' | b'1') {
                s.i += 1;
            }
            s.paint(a, 18);
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            while s.ch().is_ascii_digit() || s.ch() == b'.' {
                s.i += 1;
            }
            s.paint(a, 2);
        } else if start(s.ch()) || (s.ch() == b'#' && s.first()) {
            let first = s.first();
            let a = s.word(b"");
            if first && s.ch() == b':' {
                s.i += 1;
                s.paint(a, 15);
            } else {
                s.classify(a, 7, &[3, 10, 11, 12], true);
                if b".$%#".contains(&s.ch()) {
                    s.take(1, 6);
                }
            }
        } else if operator(s.ch()) || s.ch() == b'\\' {
            s.take(1, 6);
        } else {
            s.take(1, 0);
        }
    }
    basic_folds(s);
}

fn doc_keywords(s: &mut Scan<'_>, start: usize) {
    let mut i = start;
    while i < s.i {
        if matches!(s.at(i), b'@' | b'\\')
            && s.at(i + 1).is_ascii_alphabetic()
            && (i == 0 || s.at(i - 1) != b'\\')
        {
            let a = i;
            i += 1;
            while i < s.i && !s.at(i).is_ascii_whitespace() {
                i += 1;
            }
            s.styles[a..i].fill(22);
        } else {
            i += 1;
        }
    }
}

fn basic_folds(s: &mut Scan<'_>) {
    let mut offset = 0;
    let lines: Vec<_> = s.text.split_inclusive('\n').map(str::to_owned).collect();
    for line in lines {
        let lower = line.to_ascii_lowercase();
        let words: Vec<_> = lower.split_ascii_whitespace().collect();
        let first = words.first().copied().unwrap_or("");
        let first = first
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or("");
        if s.flag("fold.basic.syntax.based", true) {
            let opens: &[&str] = match s.p.key() {
                "blitzbasic" => &["function", "type"],
                "purebasic" => &["procedure", "enumeration", "interface", "structure"],
                _ => &[
                    "function",
                    "sub",
                    "enum",
                    "type",
                    "union",
                    "property",
                    "destructor",
                    "constructor",
                ],
            };
            if opens.contains(&first) {
                s.event(offset, 1);
            }
            if (first == "end" && words.get(1).is_some_and(|w| opens.contains(w)))
                || first
                    .strip_prefix("end")
                    .is_some_and(|w| opens.contains(&w))
            {
                s.event(offset, -1);
            }
        }
        if s.flag("fold.basic.comment.explicit", false) {
            let open =
                s.p.property("fold.basic.explicit.start")
                    .unwrap_or(if s.p.key() == "freebasic" { "'{" } else { ";{" });
            let close =
                s.p.property("fold.basic.explicit.end")
                    .unwrap_or(if s.p.key() == "freebasic" { "'}" } else { ";}" });
            let opening = !open.is_empty() && line.contains(open);
            let closing = !close.is_empty() && line.contains(close);
            if opening {
                s.event(offset, 1);
            }
            if closing {
                s.event(offset, -1);
            }
        }
        offset += line.len();
    }
}

pub(super) fn vb(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b'\'' {
            s.line(1);
        } else if s.ch() == b'"' {
            s.quote(
                b'"',
                4,
                9,
                false,
                true,
                s.flag("lexer.vb.strings.multiline", false),
            );
            if s.ch().eq_ignore_ascii_case(&b'c') {
                s.take(1, 4);
            }
        } else if s.ch() == b'#' && s.first() {
            s.line(5);
        } else if s.ch() == b'#' {
            let a = s.i;
            s.i += 1;
            while s.more() && !matches!(s.ch(), b'\r' | b'\n' | b'#' | b',') {
                s.i += 1;
            }
            if s.ch() == b'#' {
                s.i += 1;
                s.paint(a, 8);
            } else {
                let t = s.token(a).trim_start_matches('#').trim();
                let style = if !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()) {
                    2
                } else {
                    9
                };
                s.paint(a, style);
            }
        } else if s.ch() == b'&' && b"hHoObB".contains(&s.next()) {
            let a = s.i;
            let base = match s.next().to_ascii_lowercase() {
                b'h' => 16,
                b'o' => 8,
                _ => 2,
            };
            s.i += 2;
            while (s.ch() as char).is_digit(base) {
                s.i += 1;
            }
            s.paint(a, 2);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            let a = s.i;
            while s.ch().is_ascii_hexdigit() || b".-+_".contains(&s.ch()) {
                s.i += 1;
            }
            s.paint(a, 2);
        } else if s.ch() == b'[' {
            let a = s.i;
            s.i += 1;
            while s.more() && !matches!(s.ch(), b']' | b'\r' | b'\n') {
                s.i += 1;
            }
            if s.ch() == b']' {
                s.i += 1;
            }
            s.paint(a, 7);
        } else if start(s.ch()) {
            let a = s.word(b"");
            if s.token(a).eq_ignore_ascii_case("rem") {
                s.line(1);
                s.paint(a, 1);
            } else {
                s.classify(a, 7, &[3, 10, 11, 12], true);
                if b"%&@!#$".contains(&s.ch()) {
                    s.take(1, 7);
                }
            }
        } else if operator(s.ch()) || s.ch() == b'\\' {
            s.take(1, 6);
        } else {
            s.take(1, 0);
        }
    }
    super::functional::indent_folds(s);
}

pub(super) fn hollywood(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b';' {
            s.line(1);
        } else if s.is("/*") || s.is("[[") {
            let a = s.i;
            let (close, style) = if s.is("/*") { ("*/", 2) } else { ("]]", 9) };
            s.i += 2;
            let mut quoted = false;
            while s.more() {
                if s.is(close) && !quoted {
                    s.i += 2;
                    break;
                }
                if s.ch() == b'"' && (s.i == 0 || s.at(s.i - 1) != b'\\') {
                    quoted = !quoted;
                }
                s.i += 1;
            }
            s.paint(a, style);
        } else if s.ch() == b'"' {
            s.quote(b'"', 8, 8, false, false, false);
        } else if s.ch() == b'@' || s.ch() == b'#' {
            let style = if s.ch() == b'@' { 10 } else { 13 };
            let a = s.word(b"");
            s.paint(a, style);
        } else if s.ch() == b'$' || s.is("0x") || s.is("0X") {
            let a = s.i;
            s.i += if s.ch() == b'$' { 1 } else { 2 };
            while s.ch().is_ascii_hexdigit() {
                s.i += 1;
            }
            s.paint(a, 14);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            s.number(3, b".");
        } else if start(s.ch()) {
            let first = s.first();
            let a = s.word(b".");
            let t = s.token(a).to_ascii_lowercase();
            let style = s.classify(a, 12, &[4, 5, 6, 7], true);
            if style == 4 && first {
                if t == "function" {
                    s.event(a, 1);
                }
                if t == "endfunction" {
                    s.event(a, -1);
                }
            }
        } else if operator(s.ch()) || s.ch() == b'\\' {
            s.take(1, 11);
        } else {
            s.take(1, 0);
        }
    }
}
