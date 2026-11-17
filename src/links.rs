use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkOptions {
    pub mode: u8,
    pub schemes: Vec<String>,
}

impl Default for LinkOptions {
    fn default() -> Self {
        Self { mode: 2, schemes: DEFAULT_SCHEMES.iter().map(|scheme| (*scheme).into()).collect() }
    }
}

impl LinkOptions {
    pub fn from_settings(settings: &serde_json::Value) -> std::io::Result<Self> {
        let mode = settings["URL"].as_u64().or_else(|| settings["URL"]["value"].as_u64()).filter(|mode| *mode <= 4).unwrap_or(2) as u8;
        let schemes = match settings.get("uriCustomizedSchemes") {
            None => DEFAULT_SCHEMES.iter().map(|scheme| (*scheme).to_owned()).collect(),
            Some(value) => {
                let value = value.as_str().ok_or_else(|| std::io::Error::other("uriCustomizedSchemes must be text"))?;
                ["ftp://","http://","https://","mailto:","file://"].into_iter().map(str::to_owned)
                    .chain(value.split(' ').filter(|scheme| !scheme.is_empty()).map(str::to_owned)).collect()
            }
        };
        Ok(Self { mode, schemes })
    }

    pub fn enabled(&self) -> bool { self.mode != 0 }
    pub fn underline(&self) -> bool { matches!(self.mode,2 | 4) }
    pub fn full_box(&self) -> bool { matches!(self.mode,3 | 4) }
    pub fn link_ranges(&self, text: &str) -> Vec<Range<usize>> {
        if !self.enabled() { return Vec::new(); }
        ranges_with_schemes(text,&self.schemes)
    }
    pub fn link_at(&self,text: &str,byte: usize) -> Option<Range<usize>> {
        if byte >= text.len() || !text.is_char_boundary(byte) { return None; }
        let context = line_context(text,byte..byte);
        self.link_ranges(&text[context.clone()]).into_iter()
            .map(|range| range.start + context.start..range.end + context.start)
            .find(|range| range.contains(&byte))
    }
    pub fn link_spans(&self,text: &str,visible: Range<usize>) -> Vec<Range<usize>> {
        if visible.start >= visible.end || visible.end > text.len()
            || !text.is_char_boundary(visible.start) || !text.is_char_boundary(visible.end) { return Vec::new(); }
        let context = line_context(text,visible.clone());
        self.link_ranges(&text[context.clone()]).into_iter().filter_map(|range| {
            let start = (range.start + context.start).max(visible.start);
            let end = (range.end + context.start).min(visible.end);
            (start < end).then_some(start..end)
        }).collect()
    }
}

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

fn scheme_start(text: &str, from: usize, schemes: &[String]) -> Option<(usize, usize)> {
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
                    && schemes.iter().any(|prefix| {
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
    LinkOptions::default().link_ranges(text)
}

fn ranges_with_schemes(text: &str, schemes: &[String]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while let Some((start, scheme_end)) = scheme_start(text, from, schemes) {
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
