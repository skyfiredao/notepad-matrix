use super::{DecimalSeparator, Numbers};

pub(super) struct NumberTokens {
    lists: [Vec<String>; 7],
}
impl NumberTokens {
    pub fn new(n: &Numbers) -> Self {
        Self {
            lists: [
                &n.prefix1, &n.prefix2, &n.extras1, &n.extras2, &n.suffix1, &n.suffix2, &n.range,
            ]
            .map(|s| {
                s.split(' ')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            }),
        }
    }
    fn token(
        &self,
        index: usize,
        text: &str,
        p: usize,
        case: bool,
        negative: bool,
    ) -> Option<usize> {
        let p2 = p + usize::from(negative);
        if negative && text.as_bytes().get(p) != Some(&b'-') {
            return None;
        }
        self.lists[index].iter().find_map(|s| {
            text.get(p2..p2.saturating_add(s.len()))
                .filter(|t| {
                    if case {
                        t.eq_ignore_ascii_case(s)
                    } else {
                        *t == s
                    }
                })
                .map(|_| s.len() + usize::from(negative))
        })
    }
    pub fn end(
        &self,
        text: &str,
        start: usize,
        case: bool,
        decimal: DecimalSeparator,
        boundary: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let bytes = text.as_bytes();
        let digit = |p: usize| bytes.get(p).is_some_and(u8::is_ascii_digit);
        let decimal_at = |p: usize| match bytes.get(p) {
            Some(b'.') => decimal != DecimalSeparator::Comma,
            Some(b',') => decimal != DecimalSeparator::Dot,
            _ => false,
        };
        let tok = |index, p| self.token(index, text, p, case, false);
        let negative = bytes.get(start) == Some(&b'-');
        let mut p = start;
        let (
            mut dot,
            mut prefix1,
            mut prefix2,
            mut suffix1,
            mut suffix2,
            mut extras2,
            mut range,
            mut exp,
            mut after_range,
        ) = (
            false, false, false, false, false, false, false, false, false,
        );
        if decimal_at(p) {
            if digit(p + 1) {
                dot = true;
                p += 2;
            }
        } else {
            if let Some(n) = self.token(1, text, p, case, negative) {
                if let Some(extra) = tok(2, p + n) {
                    p += n + extra;
                    prefix2 = true;
                    exp = true;
                } else if digit(p + n) {
                    p += n + 1;
                    prefix2 = true;
                    exp = true;
                }
            }
            if !prefix2
                && let Some(n) = self.token(0, text, p, case, negative)
                && digit(p + n)
            {
                p += n + 1;
                prefix1 = true;
                exp = true;
            }
            if !prefix1
                && !prefix2
                && let Some(n) = self.token(3, text, p, case, negative)
            {
                p += n;
                extras2 = true;
                exp = true;
            }
        }
        if p == start {
            if digit(p) {
                p += 1;
            } else if matches!(bytes.get(p), Some(b'-' | b'+'))
                && digit(p + 1)
                && (p == 0 || !digit(p - 1))
            {
                p += 2;
            } else {
                return None;
            }
        }
        loop {
            if boundary(p) {
                return if extras2 && !suffix1 { None } else { Some(p) };
            }
            if !range && let Some(n) = tok(6, p) {
                if extras2 && !suffix1 {
                    return None;
                }
                p += n;
                suffix1 = false;
                suffix2 = false;
                dot = false;
                range = true;
                exp = false;
                extras2 = false;
                after_range = true;
                continue;
            }
            if suffix2 {
                return None;
            }
            if prefix2 && let Some(n) = tok(2, p) {
                p += n;
                continue;
            }
            if !suffix1 && !prefix1 && !prefix2 {
                if let Some(n) = tok(4, p) {
                    p += n;
                    extras2 = false;
                    suffix1 = true;
                    continue;
                }
                if let Some(n) = tok(3, p) {
                    p += n;
                    extras2 = true;
                    exp = true;
                    continue;
                }
            }
            if !suffix2 && let Some(n) = tok(5, p) {
                p += n;
                suffix2 = true;
                continue;
            }
            if after_range {
                if let Some(n) = tok(1, p) {
                    p += n;
                    exp = false;
                    prefix2 = true;
                    continue;
                }
                if let Some(n) = tok(0, p) {
                    p += n;
                    exp = false;
                    prefix1 = true;
                    continue;
                }
            }
            if digit(p) {
                p += 1;
                continue;
            }
            if !dot && decimal_at(p) && digit(p + 1) && (digit(p - 1) || after_range) {
                p += 2;
                dot = true;
                continue;
            }
            if !exp && matches!(bytes.get(p), Some(b'e' | b'E')) && digit(p - 1) {
                let n = if digit(p + 1) {
                    1
                } else if matches!(bytes.get(p + 1), Some(b'+' | b'-')) && digit(p + 2) {
                    2
                } else {
                    0
                };
                if n > 0 {
                    p += n;
                    prefix2 = false;
                    dot = false;
                    exp = true;
                    continue;
                }
            }
            return None;
        }
    }
}
