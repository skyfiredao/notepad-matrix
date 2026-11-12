use std::ops::Range;

pub const DEFAULT_SCHEMES: &[&str] = &[
    "ftp://",
    "http://",
    "https://",
    "mailto:",
    "file://",
    "svn://",
    "cvs://",
    "git://",
    "imap://",
    "irc://",
    "irc6://",
    "ircs://",
    "ldap://",
    "ldaps://",
    "news:",
    "telnet://",
    "gopher://",
    "ssh://",
    "sftp://",
    "smb://",
    "skype:",
    "snmp://",
    "spotify:",
    "steam://",
    "sms:",
    "slack://",
    "chrome://",
    "bitcoin:",
];

fn scheme_delimiter(ch: char) -> bool {
    !(ch.is_ascii_alphanumeric() || ch == '_')
}

fn text_char(ch: char) -> bool {
    ch > ' '
        && !matches!(
            ch,
            '\u{00a0}' | '\u{2002}'
                ..='\u{200b}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
                    | '"'
                    | '#'
                    | '<'
                    | '>'
                    | '{'
                    | '}'
                    | '?'
                    | '\u{7f}'
        )
}

fn scheme_start(text: &str, from: usize) -> Option<(usize, usize)> {
    let mut previous = text[..from].chars().next_back();
    let mut start = None;
    for (offset, ch) in text[from..].char_indices() {
        let byte = from + offset;
        match start {
            None if ch.is_ascii_alphabetic() && previous.is_none_or(scheme_delimiter) => {
                start = Some(byte)
            }
            Some(begin)
                if ch == ':'
                    && DEFAULT_SCHEMES.iter().any(|prefix| {
                        text.as_bytes()[begin..]
                            .get(..prefix.len())
                            .is_some_and(|candidate| {
                                candidate.eq_ignore_ascii_case(prefix.as_bytes())
                            })
                    }) =>
            {
                return Some((begin, byte + 1));
            }
            Some(_) if !ch.is_ascii_alphabetic() => start = None,
            _ => {}
        }
        previous = Some(ch);
    }
    None
}

fn url_end(text: &str, from: usize) -> usize {
    enum State {
        HostAndPath,
        Query,
        AfterDelimiter,
        Quotes(char),
        AfterQuotes,
        Fragment,
    }
    let mut state = State::HostAndPath;
    for (offset, ch) in text[from..].char_indices() {
        let delimiter = matches!(ch, '&' | '+' | '=' | ';');
        match state {
            State::HostAndPath if ch == '?' => state = State::Query,
            State::HostAndPath if ch == '#' => state = State::Fragment,
            State::HostAndPath if !text_char(ch) => return from + offset,
            State::Query if ch == '#' => state = State::Fragment,
            State::Query if delimiter => state = State::AfterDelimiter,
            State::Query if !text_char(ch) => return from + offset,
            State::AfterDelimiter => {
                state = match ch {
                    '\'' | '"' | '`' => State::Quotes(ch),
                    '(' => State::Quotes(')'),
                    '[' => State::Quotes(']'),
                    '{' => State::Quotes('}'),
                    _ if text_char(ch) => State::Query,
                    _ => return from + offset,
                }
            }
            State::Quotes(_) if ch < ' ' => return from + offset,
            State::Quotes(close) if ch == close => state = State::AfterQuotes,
            State::AfterQuotes if delimiter => state = State::AfterDelimiter,
            State::AfterQuotes => return from + offset,
            State::Fragment if ch != '?' && !text_char(ch) => return from + offset,
            _ => {}
        }
    }
    text.len()
}

fn trimmed_end(text: &str, start: usize, mut end: usize) -> usize {
    let enclosing = text[..start].chars().next_back();
    for quote in ['\'', '`'] {
        if enclosing == Some(quote) && text[start..end].ends_with(quote) {
            end -= 1;
        }
    }
    loop {
        let candidate = &text[start..end];
        let Some(last) = candidate.chars().next_back() else {
            break;
        };
        if matches!(last, '.' | ',' | ':' | ';' | '?' | '!' | '#') {
            end -= 1;
            continue;
        }
        let open = match last {
            ')' => '(',
            ']' => '[',
            _ => break,
        };
        let mut count = 0;
        let mut keep = false;
        for ch in candidate.chars().rev().skip(1) {
            if ch == last {
                count += 1;
            }
            if ch == open {
                if count == 0 {
                    keep = true;
                    break;
                }
                count -= 1;
            }
        }
        if keep || count != 0 {
            break;
        }
        end -= 1;
    }
    end
}

pub fn link_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while let Some((start, scheme_end)) = scheme_start(text, from) {
        let mut end = url_end(text, scheme_end);
        if end > scheme_end {
            end = trimmed_end(text, start, end);
            ranges.push(start..end);
        }
        from = end.max(scheme_end);
    }
    ranges
}

fn line_context(text: &str, range: Range<usize>) -> Range<usize> {
    let start = text[..range.start]
        .rfind(['\r', '\n'])
        .map_or(0, |byte| byte + 1);
    let end = text[range.end..]
        .find(['\r', '\n'])
        .map_or(text.len(), |byte| range.end + byte);
    start..end
}

pub fn link_at(text: &str, byte: usize) -> Option<Range<usize>> {
    if byte >= text.len() || !text.is_char_boundary(byte) {
        return None;
    }
    let context = line_context(text, byte..byte);
    link_ranges(&text[context.clone()])
        .into_iter()
        .map(|range| range.start + context.start..range.end + context.start)
        .find(|range| range.contains(&byte))
}

pub fn link_spans(text: &str, visible: Range<usize>) -> Vec<Range<usize>> {
    if visible.start >= visible.end
        || visible.end > text.len()
        || !text.is_char_boundary(visible.start)
        || !text.is_char_boundary(visible.end)
    {
        return Vec::new();
    }
    let context = line_context(text, visible.clone());
    link_ranges(&text[context.clone()])
        .into_iter()
        .filter_map(|range| {
            let start = (range.start + context.start).max(visible.start);
            let end = (range.end + context.start).min(visible.end);
            (start < end).then_some(start..end)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected(text: &str) -> Vec<&str> {
        link_ranges(text)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn detects_source_default_prefixes_and_scheme_boundaries() {
        assert_eq!(
            detected("HTTP://host/a https://host/b ftp://host/c mailto:a@b file:///tmp/a"),
            [
                "HTTP://host/a",
                "https://host/b",
                "ftp://host/c",
                "mailto:a@b",
                "file:///tmp/a"
            ]
        );
        assert_eq!(
            detected("_https://no 7https://no xhttps://no www.host/path http:bare"),
            Vec::<&str>::new()
        );
        assert_eq!(
            detected("中https://host/a +https://host/b"),
            ["https://host/a", "https://host/b"]
        );
        assert_eq!(
            detected("git://host/repo ssh://host/path slack://channel spotify:track bitcoin:abc"),
            [
                "git://host/repo",
                "ssh://host/path",
                "slack://channel",
                "spotify:track",
                "bitcoin:abc"
            ]
        );
        assert!(detected("irc6://host custom://host javascript:alert(1)").is_empty());
    }

    #[test]
    fn query_quotes_delimiters_and_fragments_follow_source_states() {
        assert_eq!(
            detected("https://host/path?a=\"i j k\"&'l m n'+opq rest"),
            ["https://host/path?a=\"i j k\"&'l m n'+opq"]
        );
        assert_eq!(
            detected("https://host/?x=(a b)&y=[c d];z={e f}+`g h`!tail"),
            ["https://host/?x=(a b)&y=[c d];z={e f}+`g h`"]
        );
        assert_eq!(
            detected("https://host/?'a b' https://host/#part?yes#stop"),
            ["https://host/?'a", "https://host/#part?yes"]
        );
        assert_eq!(
            detected("https://host/?a=#stop https://host/?a=\"line\nbreak\""),
            ["https://host/?a=", "https://host/?a=\"line"]
        );
    }

    #[test]
    fn trims_source_punctuation_and_enclosing_quotes_without_normalization() {
        assert_eq!(
            detected("(https://host/a). [https://host/b], 'https://host/c.' `https://host/d`"),
            [
                "https://host/a",
                "https://host/b",
                "https://host/c",
                "https://host/d"
            ]
        );
        assert_eq!(
            detected("https://host/a(x) https://host/a(x)) https://host/a))"),
            ["https://host/a(x)", "https://host/a(x)", "https://host/a))"]
        );
        assert_eq!(
            detected("https://host/a.,:;?!# https://host/a%20b https://host/a%29"),
            ["https://host/a", "https://host/a%20b", "https://host/a%29"]
        );
    }

    #[test]
    fn unicode_whitespace_and_utf8_offsets_follow_the_source_character_list() {
        for separator in [
            '\u{a0}', '\u{2002}', '\u{200b}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
        ] {
            let text = format!("文 https://例子.test/路径{separator}after");
            assert_eq!(detected(&text), ["https://例子.test/路径"]);
            assert_eq!(
                link_ranges(&text).as_slice(),
                std::slice::from_ref(&(4..30))
            );
        }
        for allowed in ['\u{85}', '\u{2000}', '\u{2028}'] {
            let url = format!("https://host/a{allowed}b");
            assert_eq!(detected(&url), [&url]);
        }
        assert_eq!(
            detected("https://host/?q=\"a\u{a0}b\""),
            ["https://host/?q=\"a\u{a0}b\""]
        );
    }

    #[test]
    fn hit_testing_returns_full_byte_range_but_visible_spans_are_clipped() {
        let text = "文 (https://例子.test/路径). tail\r\nmailto:a@b";
        assert_eq!(link_at(text, 4), None);
        assert_eq!(link_at(text, 5), Some(5..31));
        assert_eq!(link_at(text, 25), Some(5..31));
        assert_eq!(link_at(text, 31), None, "end is exclusive");
        assert_eq!(link_at(text, text.len()), None);
        assert_eq!(link_at(text, usize::MAX), None);
        assert_eq!(
            link_at(text, 14),
            None,
            "inside a UTF-8 scalar is not a caret boundary"
        );
        assert_eq!(
            link_spans(text, 13..32).as_slice(),
            std::slice::from_ref(&(13..31))
        );
        assert_eq!(link_spans(text, 0..text.len()), [5..31, 40..50]);
        assert!(
            link_spans(text, 14..31).is_empty(),
            "invalid UTF-8 slice is rejected"
        );
        assert!(link_spans(text, 31..31).is_empty());
        assert!(link_spans(text, 40..usize::MAX).is_empty());
    }

    #[test]
    fn visible_scan_uses_whole_line_context_and_has_no_invented_2000_character_cap() {
        let text = "prefix_xhttps://not-a-link\n'https://host/a.'\nhttps://next/path";
        assert!(
            link_spans(text, 8..24).is_empty(),
            "cannot manufacture scheme boundary at viewport edge"
        );
        assert_eq!(
            link_spans(text, 30..43).as_slice(),
            std::slice::from_ref(&(30..42))
        );
        let long = format!("https://host/{}", "a".repeat(4_096));
        assert_eq!(
            link_ranges(&long).as_slice(),
            std::slice::from_ref(&(0..4_109))
        );
        assert_eq!(link_at(&long, 3_000), Some(0..4_109));
    }
}
