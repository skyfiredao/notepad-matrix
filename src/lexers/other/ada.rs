use super::{Scan, start};

pub(super) fn lex(s: &mut Scan<'_>) {
    let mut attribute = false;
    while s.more() {
        if s.is("--") {
            s.line(10);
        } else if s.ch() == b'"' {
            s.quote(b'"', 7, 8, false, false, false);
            attribute = true;
        } else if s.ch() == b'\'' && !attribute {
            s.quote(b'\'', 5, 6, false, false, false);
            attribute = true;
        } else if s.is("<<") {
            let a = s.i;
            s.i += 2;
            let name = s.word(b"");
            let valid =
                valid_identifier(s.token(name)) && !s.kw(0, s.token(name), true) && s.is(">>");
            if s.is(">>") {
                s.i += 2;
            }
            s.paint(a, if valid { 9 } else { 11 });
            attribute = false;
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            s.i += 1;
            while s.more()
                && (super::word(s.ch())
                    || s.ch() == b'#'
                    || (s.ch() == b'.' && s.next() != b'.')
                    || (matches!(s.ch(), b'+' | b'-') && matches!(s.at(s.i - 1), b'e' | b'E')))
            {
                s.i += 1;
            }
            s.paint(a, if valid_number(s.token(a)) { 3 } else { 11 });
            attribute = true;
        } else if start(s.ch()) {
            let a = s.word(b"");
            let style = if !valid_identifier(s.token(a)) {
                11
            } else if s.kw(0, s.token(a), true) {
                1
            } else {
                2
            };
            attribute = style != 1 || s.token(a).eq_ignore_ascii_case("all");
            s.paint(a, style);
        } else if b"&'()*+,-./:;<=>|".contains(&s.ch()) {
            attribute = s.ch() == b')';
            s.take(1, 4);
        } else {
            s.take(1, 0);
        }
    }
}

fn valid_identifier(t: &str) -> bool {
    t.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && !t.ends_with('_')
        && !t.contains("__")
        && t.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn valid_number(t: &str) -> bool {
    if t.contains("__") || t.ends_with('_') {
        return false;
    }
    let clean = t.replace('_', "");
    let parts: Vec<_> = clean.split('#').collect();
    if parts.len() == 3 {
        let Ok(base) = parts[0].parse::<u32>() else {
            return false;
        };
        if !(2..=16).contains(&base) || parts[1].is_empty() || parts[1].matches('.').count() > 1 {
            return false;
        }
        if !parts[1].chars().all(|c| c == '.' || c.is_digit(base)) {
            return false;
        }
        return parts[2].is_empty() || exponent(parts[2]);
    }
    if parts.len() != 1 {
        return false;
    }
    let n: Vec<_> = clean.split(['e', 'E']).collect();
    n.len() <= 2
        && !n[0].is_empty()
        && n[0].matches('.').count() <= 1
        && n[0].bytes().all(|c| c.is_ascii_digit() || c == b'.')
        && (n.len() == 1 || exponent(&format!("e{}", n[1])))
}

fn exponent(t: &str) -> bool {
    let Some(rest) = t.strip_prefix(['e', 'E']) else {
        return false;
    };
    let rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
    !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_digit())
}
