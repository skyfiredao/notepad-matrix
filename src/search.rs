use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

use crate::editor::{ByteOffset, Selection};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Normal,
    Extended,
}

impl SearchMode {
    pub fn decode(self, query: &str) -> Cow<'_, str> {
        match self {
            Self::Normal => Cow::Borrowed(query),
            Self::Extended => Cow::Owned(decode_extended(query)),
        }
    }
}

pub fn decode_extended(query: &str) -> String {
    let units: Vec<u16> = query.encode_utf16().collect();
    let mut result = Vec::with_capacity(units.len());
    let mut at = 0;
    while at < units.len() {
        let current = units[at];
        at += 1;
        if current != u16::from(b'\\') || at == units.len() {
            result.push(current);
            continue;
        }
        let escaped = units[at];
        at += 1;
        match char::from_u32(u32::from(escaped)) {
            Some('r') => result.push(u16::from(b'\r')),
            Some('n') => result.push(u16::from(b'\n')),
            Some('0') => result.push(0),
            Some('t') => result.push(u16::from(b'\t')),
            Some('\\') => result.push(u16::from(b'\\')),
            _ => {
                let (size, base) = match char::from_u32(u32::from(escaped)) {
                    Some('b') => (8, 2),
                    Some('o') => (3, 8),
                    Some('d') => (3, 10),
                    Some('x') => (2, 16),
                    Some('u') => (4, 16),
                    _ => (0, 0),
                };
                let value = (size > 0)
                    .then(|| units.get(at..at + size))
                    .flatten()
                    .and_then(|digits| {
                        digits.iter().try_fold(0u16, |value, &digit| {
                            let digit = if digit >= u16::from(b'A') {
                                (digit & 0xdf).wrapping_sub(7)
                            } else if digit > u16::from(b'9') {
                                return None;
                            } else {
                                digit
                            };
                            let digit = digit.checked_sub(u16::from(b'0'))?;
                            if digit >= base {
                                return None;
                            }
                            Some(value * base + digit)
                        })
                    });
                if let Some(value) = value {
                    result.push(value);
                    at += size;
                } else {
                    result.extend([u16::from(b'\\'), escaped]);
                }
            }
        }
    }
    String::from_utf16_lossy(&result)
}

#[derive(Clone, Copy)]
pub struct LiteralOptions {
    pub mode: SearchMode,
    pub match_case: bool,
    pub wrap: bool,
    pub whole_word: bool,
}

impl Default for LiteralOptions {
    fn default() -> Self {
        Self {
            mode: SearchMode::Normal,
            match_case: false,
            wrap: true,
            whole_word: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SearchError {
    InvalidSelection,
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSelection => "search selection is not a valid text boundary",
        })
    }
}
impl std::error::Error for SearchError {}

#[derive(Debug)]
pub enum ReplaceError {
    Search(SearchError),
    Edit(crate::editor::EditError),
}
impl fmt::Display for ReplaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Search(error) => error.fmt(formatter),
            Self::Edit(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for ReplaceError {}
impl From<SearchError> for ReplaceError {
    fn from(error: SearchError) -> Self {
        Self::Search(error)
    }
}
impl From<crate::editor::EditError> for ReplaceError {
    fn from(error: crate::editor::EditError) -> Self {
        Self::Edit(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LiteralMatch {
    pub range: Range<ByteOffset>,
    pub wrapped: bool,
}

struct Prepared<'a> {
    text: &'a str,
    haystack: Cow<'a, str>,
    needle: Cow<'a, str>,
    unicode_fold: bool,
    prefixes: Vec<usize>,
}

impl<'a> Prepared<'a> {
    fn new(text: &'a str, query: &'a str, match_case: bool) -> Self {
        if match_case {
            return Self {
                text,
                haystack: Cow::Borrowed(text),
                needle: Cow::Borrowed(query),
                unicode_fold: false,
                prefixes: Vec::new(),
            };
        }
        if text.is_ascii() && query.is_ascii() {
            return Self {
                text,
                haystack: Cow::Owned(text.to_ascii_lowercase()),
                needle: Cow::Owned(query.to_ascii_lowercase()),
                unicode_fold: false,
                prefixes: Vec::new(),
            };
        }
        let mut needle = String::with_capacity(query.len());
        for ch in query.chars() {
            if let Some(mapped) = crate::unicode_case::mapping(ch) {
                needle.push_str(mapped);
            } else {
                needle.push(ch);
            }
        }
        let mut prefixes = vec![0; needle.len()];
        let bytes = needle.as_bytes();
        let mut matched = 0;
        for at in 1..bytes.len() {
            while matched > 0 && bytes[at] != bytes[matched] {
                matched = prefixes[matched - 1];
            }
            if bytes[at] == bytes[matched] {
                matched += 1;
            }
            prefixes[at] = matched;
        }
        Self {
            text,
            haystack: Cow::Borrowed(text),
            needle: Cow::Owned(needle),
            unicode_fold: true,
            prefixes,
        }
    }

    fn find(
        &self,
        mut range: Range<usize>,
        backwards: bool,
        whole_word: bool,
    ) -> Option<Range<usize>> {
        if self.unicode_fold {
            let part = self.text.get(range.clone())?;
            let needle = self.needle.as_bytes();
            let mut starts = vec![None; needle.len()];
            let mut matched = 0;
            let mut count = 0usize;
            let mut last = None;
            for (offset, ch) in part.char_indices() {
                let mut utf8 = [0; 4];
                let folded = crate::unicode_case::mapping(ch)
                    .unwrap_or_else(|| ch.encode_utf8(&mut utf8))
                    .as_bytes();
                for (index, &byte) in folded.iter().enumerate() {
                    starts[count % needle.len()] = (index == 0).then_some(range.start + offset);
                    count += 1;
                    while matched > 0 && byte != needle[matched] {
                        matched = self.prefixes[matched - 1];
                    }
                    if byte == needle[matched] {
                        matched += 1;
                    }
                    if matched == needle.len() {
                        if index + 1 == folded.len()
                            && let Some(start) = starts[(count - needle.len()) % needle.len()]
                        {
                            let end = range.start + offset + ch.len_utf8();
                            if !whole_word || crate::editor::whole_word_at(self.text, start, end) {
                                if !backwards {
                                    return Some(start..end);
                                }
                                last = Some(start..end);
                            }
                        }
                        matched = self.prefixes[matched - 1];
                    }
                }
            }
            return last;
        }
        while range.start <= range.end {
            let part = self.haystack.get(range.clone())?;
            let relative = if backwards {
                part.rfind(self.needle.as_ref())
            } else {
                part.find(self.needle.as_ref())
            }?;
            let found = range.start + relative;
            let end = found + self.needle.len();
            if !whole_word || crate::editor::whole_word_at(self.text, found, end) {
                return Some(found..end);
            }
            if backwards {
                range.end = end - 1;
                while !self.haystack.is_char_boundary(range.end) {
                    range.end -= 1;
                }
            } else {
                range.start = found + self.haystack[found..].chars().next()?.len_utf8();
            }
        }
        None
    }
}

pub(crate) fn literal_ranges(
    text: &str,
    query: &str,
    start: usize,
    options: LiteralOptions,
) -> Result<Vec<Range<ByteOffset>>, SearchError> {
    literal_ranges_in(text, query, start..text.len(), options)
}

pub(crate) fn literal_scope(
    text: &str,
    selection: Selection,
    wrap: bool,
    backwards: bool,
    in_selection: bool,
) -> Result<Range<usize>, SearchError> {
    let selected = selection.range();
    if selected.end.0 > text.len()
        || !text.is_char_boundary(selected.start.0)
        || !text.is_char_boundary(selected.end.0)
    {
        return Err(SearchError::InvalidSelection);
    }
    Ok(if in_selection {
        selected.start.0..selected.end.0
    } else if wrap {
        0..text.len()
    } else if backwards {
        0..selected.end.0
    } else {
        selected.start.0..text.len()
    })
}

pub fn count_literal(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
    in_selection: bool,
) -> Result<usize, SearchError> {
    let decoded = options.mode.decode(query);
    let query = decoded.as_ref();
    let scope = literal_scope(text, selection, options.wrap, backwards, in_selection)?;
    if query.is_empty() {
        return Ok(0);
    }
    let prepared = Prepared::new(text, query, options.match_case);
    let mut at = scope.start;
    let mut count = 0;
    while let Some(found) = prepared.find(at..scope.end, false, options.whole_word) {
        count += 1;
        at = found.end;
    }
    Ok(count)
}

pub(crate) fn literal_ranges_in(
    text: &str,
    query: &str,
    scope: Range<usize>,
    options: LiteralOptions,
) -> Result<Vec<Range<ByteOffset>>, SearchError> {
    let decoded = options.mode.decode(query);
    let query = decoded.as_ref();
    if scope.start > scope.end
        || scope.end > text.len()
        || !text.is_char_boundary(scope.start)
        || !text.is_char_boundary(scope.end)
    {
        return Err(SearchError::InvalidSelection);
    }
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let prepared = Prepared::new(text, query, options.match_case);
    let mut ranges = Vec::new();
    let mut at = scope.start;
    while let Some(found) = prepared.find(at..scope.end, false, options.whole_word) {
        at = found.end;
        ranges.push(ByteOffset(found.start)..ByteOffset(found.end));
    }
    Ok(ranges)
}

pub fn find_literal(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
) -> Result<Option<LiteralMatch>, SearchError> {
    find_literal_impl(text, query, selection, options, backwards, false)
}

pub fn find_literal_for_replace(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
) -> Result<Option<LiteralMatch>, SearchError> {
    find_literal_impl(text, query, selection, options, backwards, true)
}

fn find_literal_impl(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
    replacing: bool,
) -> Result<Option<LiteralMatch>, SearchError> {
    let decoded = options.mode.decode(query);
    let query = decoded.as_ref();
    let range = selection.range();
    if range.end.0 > text.len()
        || !text.is_char_boundary(range.start.0)
        || !text.is_char_boundary(range.end.0)
    {
        return Err(SearchError::InvalidSelection);
    }
    if query.is_empty() {
        return Ok(None);
    }
    let prepared = Prepared::new(text, query, options.match_case);
    let found = if backwards {
        let mut limit = if replacing {
            range.end.0
        } else {
            range.end.0.saturating_sub(1)
        };
        while !text.is_char_boundary(limit) {
            limit -= 1;
        }
        prepared.find(0..limit, true, options.whole_word)
    } else {
        let start = if replacing {
            range.start.0
        } else {
            range.end.0
        };
        prepared.find(start..text.len(), false, options.whole_word)
    };
    let (found, wrapped) = match found {
        Some(found) => (Some(found), false),
        None if options.wrap => (
            prepared.find(0..text.len(), backwards, options.whole_word),
            true,
        ),
        None => (None, false),
    };
    Ok(found.map(|found| LiteralMatch {
        range: ByteOffset(found.start)..ByteOffset(found.end),
        wrapped,
    }))
}
