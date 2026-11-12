use super::{Scan, operator, start};

pub(super) fn fortran(s: &mut Scan<'_>) {
    let fixed = s.p.key() == "fortran77";
    let mut quote = 0;
    let mut previous = String::new();
    while s.more() {
        let column = s.i - s.line_start();
        if fixed && column == 0 && b"cC*!".contains(&s.ch()) {
            let directive = [
                "cdec$", "*dec$", "!dec$", "cdir$", "*dir$", "!dir$", "cms$", "*ms$", "!ms$",
                "!gcc$",
            ]
            .iter()
            .any(|d| {
                s.b[s.i..]
                    .get(..d.len())
                    .is_some_and(|p| p.eq_ignore_ascii_case(d.as_bytes()))
            }) || s.next() == b'$';
            s.line(if directive { 11 } else { 1 });
        } else if fixed && column >= 72 && !matches!(s.ch(), b'\r' | b'\n') {
            s.line(1);
        } else if fixed && column < 5 {
            s.take(1, if s.ch().is_ascii_digit() { 13 } else { 0 });
        } else if fixed && column == 5 && !matches!(s.ch(), b'\r' | b'\n') {
            s.take(1, 14);
        } else if s.ch() == b'#' && s.first() {
            s.line(11);
        } else if s.ch() == b'!' {
            s.line(if s.next() == b'$' { 11 } else { 1 });
        } else if !fixed && s.ch() == b'&' {
            let end = s.line_end();
            let rest = &s.text[s.i + 1..end];
            if rest.trim().is_empty() || rest.trim_start().starts_with('!') || s.first() {
                s.take(1, 14);
            } else {
                s.take(1, 6);
            }
        } else if matches!(s.ch(), b'\r' | b'\n') {
            previous.clear();
            s.take(1, 0);
        } else if quote != 0 || matches!(s.ch(), b'\'' | b'"') {
            let a = s.i;
            if quote == 0 {
                quote = s.ch();
                s.i += 1;
            }
            let style = if quote == b'\'' { 3 } else { 4 };
            let mut continued = false;
            let mut closed = false;
            while s.more() && !matches!(s.ch(), b'\r' | b'\n') {
                if fixed && s.i - s.line_start() >= 72 {
                    break;
                }
                if !fixed && s.ch() == b'&' && s.text[s.i + 1..s.line_end()].trim().is_empty() {
                    s.paint(a, style);
                    s.take(1, 14);
                    continued = true;
                    break;
                }
                if s.ch() == quote {
                    s.i += 1;
                    if s.ch() == quote {
                        s.i += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    s.i += 1;
                }
            }
            if !continued {
                s.paint(a, if closed { style } else { 5 });
            }
            if closed || (!continued && !fixed) {
                quote = 0;
            }
        } else if s.ch() == b'.' && s.next().is_ascii_alphabetic() {
            let a = s.i;
            s.i += 1;
            while s.more() && s.ch() != b'.' && !matches!(s.ch(), b'\r' | b'\n') {
                s.i += 1;
            }
            if s.ch() == b'.' {
                s.i += 1;
            }
            s.paint(a, 12);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            let label = !fixed && s.first() && s.ch().is_ascii_digit();
            if label {
                let a = s.i;
                while s.ch().is_ascii_digit() {
                    s.i += 1;
                }
                s.paint(a, 13);
            } else {
                s.number(2, b".'\"");
            }
        } else if b"bBoOzZ".contains(&s.ch()) && matches!(s.next(), b'\'' | b'"') {
            s.number(2, b".'\"");
        } else if start(s.ch()) {
            let a = s.word(b"");
            let t = s.token(a).to_ascii_lowercase();
            let style = s.classify(a, 7, &[8, 9, 10], true);
            if style == 8 {
                let next = s.b[s.i..]
                    .iter()
                    .copied()
                    .find(|c| !matches!(c, b' ' | b'\t'))
                    .unwrap_or(0);
                let opens = matches!(
                    t.as_str(),
                    "associate"
                        | "block"
                        | "blockdata"
                        | "select"
                        | "selecttype"
                        | "selectcase"
                        | "do"
                        | "enum"
                        | "function"
                        | "interface"
                        | "module"
                        | "program"
                        | "subroutine"
                        | "then"
                        | "enumeration"
                        | "critical"
                        | "submodule"
                ) || (t == "type" && next != b'(' && previous != "enumeration");
                if opens
                    && previous != "end"
                    && !(previous == "module" && matches!(t.as_str(), "function" | "subroutine"))
                    && !(t == "do" && next.is_ascii_digit())
                {
                    s.event(a, 1);
                }
                if (t == "end" && !matches!(next, b'=' | b'('))
                    || matches!(
                        t.as_str(),
                        "endassociate"
                            | "endblock"
                            | "endblockdata"
                            | "endselect"
                            | "enddo"
                            | "endenum"
                            | "endif"
                            | "endforall"
                            | "endfunction"
                            | "endinterface"
                            | "endmodule"
                            | "endprogram"
                            | "endsubroutine"
                            | "endtype"
                            | "endwhere"
                            | "endcritical"
                            | "endsubmodule"
                            | "endteam"
                    )
                    || (previous == "module" && t == "procedure")
                    || (previous == "type" && t == "is")
                {
                    s.event(a, -1);
                }
            }
            previous = t;
        } else if operator(s.ch()) || s.ch() == b'%' {
            s.take(1, 6);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn matlab(s: &mut Scan<'_>) {
    let mut transpose = false;
    let mut brackets = 0usize;
    while s.more() {
        if s.ch() == b'%' {
            if s.first() && s.next() == b'{' && s.text[s.i + 2..s.line_end()].trim().is_empty() {
                let a = s.i;
                let mut depth = 1;
                s.i += 2;
                while s.more() {
                    if s.ch() == b'%'
                        && s.first()
                        && s.text
                            .get(s.i + 2..s.line_end())
                            .is_some_and(|r| r.trim().is_empty())
                    {
                        if s.next() == b'{' {
                            depth += 1;
                            s.i += 2;
                            continue;
                        }
                        if s.next() == b'}' {
                            depth -= 1;
                            s.i += 2;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }
                    }
                    s.i += 1;
                }
                s.paint(a, 1);
                if s.flag("fold.comment", false) {
                    s.region(a, s.i);
                }
            } else {
                s.line(1);
            }
        } else if s.is("...") {
            s.line(1);
        } else if s.ch() == b'!' && s.next() != b'=' {
            s.line(2);
        } else if s.ch() == b'\'' {
            if transpose {
                s.take(1, 6);
            } else {
                s.quote(b'\'', 5, 5, false, true, false);
                transpose = true;
            }
        } else if s.ch() == b'"' {
            s.quote(b'"', 8, 8, false, false, false);
            transpose = true;
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            s.number(3, b".");
            transpose = true;
        } else if start(s.ch()) {
            let a = s.word(b"");
            let t = s.token(a).to_owned();
            let mut style = s.classify(a, 7, &[4], false);
            if t == "end" && brackets > 0 {
                style = 6;
                s.paint(a, style);
            }
            transpose = style != 4;
            if style == 4 {
                if matches!(
                    t.as_str(),
                    "function"
                        | "if"
                        | "for"
                        | "parfor"
                        | "while"
                        | "switch"
                        | "try"
                        | "classdef"
                        | "properties"
                        | "methods"
                        | "events"
                        | "enumeration"
                        | "spmd"
                ) {
                    s.event(a, 1);
                }
                if t == "end" {
                    s.event(a, -1);
                }
            }
        } else if operator(s.ch()) || b"@\\".contains(&s.ch()) {
            if b"([{".contains(&s.ch()) {
                brackets += 1;
            }
            if b")]}".contains(&s.ch()) {
                brackets = brackets.saturating_sub(1);
            }
            if !matches!(s.ch(), b'.' | b'\'') {
                transpose = b")]}".contains(&s.ch());
            }
            s.take(1, 6);
        } else {
            if matches!(s.ch(), b'\r' | b'\n') {
                transpose = false;
            }
            s.take(1, 0);
        }
    }
}

pub(super) fn r(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b'#' {
            s.line(1);
        } else if matches!(s.ch(), b'r' | b'R') && matches!(s.next(), b'\'' | b'"') {
            let a = s.i;
            let quote = s.next();
            let mut end = a + 2;
            while s.at(end) == b'-' {
                end += 1;
            }
            let closing = match s.at(end) {
                b'(' => b')',
                b'[' => b']',
                b'{' => b'}',
                _ => 0,
            };
            if closing != 0 {
                let dashes = end - a - 2;
                let close = format!("{}{}{}", closing as char, "-".repeat(dashes), quote as char);
                s.i = end + 1;
                s.until(&close, if quote == b'"' { 13 } else { 14 });
                s.paint(a, if quote == b'"' { 13 } else { 14 });
            } else {
                s.take(1, 9);
            }
        } else if matches!(s.ch(), b'\'' | b'"' | b'`') {
            let a = s.i;
            let quote = s.ch();
            let style = match quote {
                b'"' => 6,
                b'\'' => 7,
                _ => 12,
            };
            s.quote(quote, style, style, true, false, true);
            if s.flag("lexer.r.escape.sequence", false) {
                let mut i = a + 1;
                while i < s.i {
                    if s.at(i) == b'\\' {
                        let start = i;
                        i = (i + 2).min(s.i);
                        let max = match s.at(start + 1) {
                            b'x' => 2,
                            b'u' => 4,
                            b'U' => 8,
                            b'0'..=b'7' => 2,
                            _ => 0,
                        };
                        if s.at(i) == b'{' && matches!(s.at(start + 1), b'u' | b'U') {
                            i += 1;
                        }
                        let begin = i;
                        while i < s.i && i - begin < max && s.at(i).is_ascii_hexdigit() {
                            i += 1;
                        }
                        if s.at(i) == b'}' {
                            i += 1;
                        }
                        s.styles[start..i].fill(15);
                    } else {
                        i += 1;
                    }
                }
            }
        } else if s.ch() == b'%' {
            let a = s.i;
            s.i += 1;
            while s.more() && !matches!(s.ch(), b'%' | b'\r' | b'\n') {
                s.i += 1;
            }
            if s.ch() == b'%' {
                s.i += 1;
                s.paint(a, 10);
            } else {
                s.paint(a, 11);
            }
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            let a = s.i;
            if s.is("0x") || s.is("0X") {
                s.i += 2;
            }
            while s.more() {
                if matches!(s.ch(), b'e' | b'E' | b'p' | b'P')
                    && (s.next().is_ascii_digit() || matches!(s.next(), b'+' | b'-'))
                {
                    s.i += 1;
                    if matches!(s.ch(), b'+' | b'-') {
                        s.i += 1;
                    }
                } else if s.ch().is_ascii_hexdigit()
                    || (s.ch() == b'.' && s.next().is_ascii_digit())
                {
                    s.i += 1;
                } else {
                    break;
                }
            }
            if matches!(s.ch(), b'L' | b'i') {
                s.i += 1;
            }
            s.paint(a, 5);
        } else if start(s.ch()) || s.ch() == b'.' {
            let a = s.word(b".");
            s.classify(a, 9, &[2, 3, 4], false);
        } else if operator(s.ch()) || s.ch() == b'$' {
            s.delimit(8, b"{", b"}");
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn sas(s: &mut Scan<'_>) {
    while s.more() {
        if s.is("/*") {
            s.block("/*", "*/", false, 3);
        } else if (s.ch() == b'*' && s.first()) || s.is("//") {
            let a = s.i;
            let style = if s.ch() == b'*' { 1 } else { 2 };
            while s.more() && s.ch() != b';' {
                s.i += 1;
            }
            s.paint(a, style);
        } else if s.ch() == b'"' {
            s.quote(b'"', 7, 7, false, false, true);
        } else if s.ch() == b'%' {
            let a = s.word(b"");
            let t = s.token(a);
            let style = if s.kw(0, t, true) {
                12
            } else if s.kw(1, t, true) {
                13
            } else if s.kw(2, t, true) {
                14
            } else {
                11
            };
            s.paint(a, style);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            s.number(4, b".");
        } else if start(s.ch()) {
            let a = s.word(b"");
            let t = s.token(a);
            let style = if s.kw(3, t, true) {
                15
            } else if s.kw(1, t, true) {
                13
            } else {
                6
            };
            s.paint(a, style);
        } else if operator(s.ch()) {
            s.take(1, 5);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn spice(s: &mut Scan<'_>) {
    while s.more() {
        if (s.ch() == b'*' && s.i == s.line_start()) || s.is("*~") {
            s.line(8);
        } else if s.ch().is_ascii_digit() || s.ch() == b'#' {
            let a = s.i;
            s.i += 1;
            while s.more()
                && !s.ch().is_ascii_whitespace()
                && (!b"&'()*+,-./:;<=>|".contains(&s.ch())
                    || s.ch() == b'.'
                    || (matches!(s.ch(), b'+' | b'-') && matches!(s.at(s.i - 1), b'e' | b'E')))
            {
                s.i += 1;
            }
            s.paint(a, 5);
        } else if b"&'()*+,-./:;<=>|".contains(&s.ch()) {
            s.take(1, 6);
        } else if !s.ch().is_ascii_whitespace() {
            let a = s.i;
            while s.more()
                && !s.ch().is_ascii_whitespace()
                && !b"&'()*+,-./:;<=>|".contains(&s.ch())
            {
                s.i += 1;
            }
            s.classify(a, 1, &[2, 3, 4], true);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn csound(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch() == b';' {
            let a = s.i;
            s.line(1);
            while s.i > a && s.at(s.i - 1) == b'\\' && s.more() {
                s.i += 1;
                s.line(1);
            }
            s.paint(a, 1);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            s.number(2, b".?");
        } else if start(s.ch()) || b".%@$?".contains(&s.ch()) {
            let a = s.word(b".?");
            let fallback = match s.at(a) {
                b'p' => 10,
                b'a' => 11,
                b'k' => 12,
                b'i' => 13,
                b'g' => 14,
                _ => 5,
            };
            let opening = s.token(a) == "instr";
            let closing = s.token(a) == "endin";
            let style = s.classify(a, fallback, &[6, 7, 8], false);
            if style == 6 {
                if opening {
                    s.event(a, 1);
                }
                if closing {
                    s.event(a, -1);
                }
            }
        } else if b"*/-+()=^[]<&>,|~%:".contains(&s.ch()) {
            s.take(1, 3);
        } else {
            s.take(1, 0);
        }
    }
}
