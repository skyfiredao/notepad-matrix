use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

use crate::editor::{ByteOffset, Selection};

#[path = "search_replacement.rs"]
mod replacement;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Normal,
    Extended,
    Regex {
        dot_matches_newline: bool,
    },
}

impl SearchMode {
    pub fn decode(self, query: &str) -> Cow<'_, str> {
        match self {
            Self::Normal | Self::Regex { .. } => Cow::Borrowed(query),
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

#[derive(Clone, Copy, PartialEq, Eq)]
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
    Regex(String),
    RegexRuntime(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSelection => formatter.write_str("search selection is not a valid text boundary"),
            Self::Regex(message) => write!(formatter, "查找: 无效的正则表达式。{message}"),
            Self::RegexRuntime(message) => write!(formatter, "查找: 搜索失败。{message}"),
        }
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

#[derive(Clone, Default)]
pub(crate) struct RegexState {
    last_match: Option<RegexLastMatch>,
    last_direction: i8,
}

#[derive(Clone, Copy)]
pub(crate) enum RegexFindKind {
    FindNext,
    ReplaceNext,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegexEmptyMode {
    None,
    NotAfterMatch,
    All,
}

#[derive(Clone)]
struct RegexLastMatch {
    start: usize,
    end: usize,
    continuation_end: usize,
    modified: bool,
}

impl RegexState {
    pub(crate) fn direction(&mut self, from: i128, to: i128) -> bool {
        let backwards = from > to || from == to && self.last_direction < 0;
        self.last_direction = if backwards { -1 } else { 1 };
        backwards
    }
    fn last_was_empty(&self) -> bool {
        self.last_match.as_ref().is_some_and(|found| found.start == found.end)
    }
    fn is_continuation(&self, position: i128, backwards: bool) -> bool {
        self.last_match.as_ref().is_some_and(|found| !found.modified
            && position == if backwards { found.start as i128 } else { found.continuation_end as i128 })
    }

    fn found(&mut self, range: &Range<ByteOffset>) {
        self.last_match = Some(RegexLastMatch { start: range.start.0, end: range.end.0,
            continuation_end: range.end.0, modified: false });
    }

    pub(crate) fn invalidate(&mut self) {
        if let Some(found) = &mut self.last_match { found.modified = true; }
    }

    pub(crate) fn edit(&mut self, position: usize, removed: usize, inserted: usize) {
        let Some(found) = &mut self.last_match else { return; };
        if removed != 0 {
            if position == found.start && removed == found.end - found.start {
                found.continuation_end = found.start;
            } else { found.modified = true; }
        }
        if inserted != 0 {
            if position == found.start && found.start == found.continuation_end {
                found.continuation_end += inserted;
            } else { found.modified = true; }
        }
    }
}

pub(crate) fn literal_scope_bytes(
    bytes: &crate::byte_buffer::ByteBuffer,
    selection: Selection,
    wrap: bool,
    backwards: bool,
    in_selection: bool,
) -> Result<Range<usize>, SearchError> {
    let selected = selection.range();
    if selected.end.0 > bytes.len() { return Err(SearchError::InvalidSelection); }
    Ok(if in_selection { selected.start.0..selected.end.0 }
        else if wrap { 0..bytes.len() }
        else if backwards { 0..selected.end.0 }
        else { selected.start.0..bytes.len() })
}

pub(crate) fn literal_ranges_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, start: usize, options: LiteralOptions) -> Result<Vec<Range<ByteOffset>>, SearchError> {
    literal_ranges_in_bytes(bytes, query, start..bytes.len(), options)
}

pub(crate) fn literal_ranges_in_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, scope: Range<usize>, options: LiteralOptions) -> Result<Vec<Range<ByteOffset>>, SearchError> {
    if scope.start > scope.end || scope.end > bytes.len() { return Err(SearchError::InvalidSelection); }
    if query.is_empty() { return Ok(Vec::new()); }
    if matches!(options.mode, SearchMode::Regex { .. }) {
        return regex_edits_bytes(bytes, query, None, scope, options, false).map(|edits| edits.into_iter().map(|(range, _)| range).collect());
    }
    let query = options.mode.decode(query);
    if query.is_empty() { return Ok(Vec::new()); }
    let mut ranges = Vec::new();
    let mut at = scope.start;
    while let Some(found) = bytes.find_literal_bytes(query.as_bytes(), at, scope.end, options.match_case, options.whole_word, false)
        .map_err(|_| SearchError::InvalidSelection)? {
        at = found.end;
        ranges.push(ByteOffset(found.start)..ByteOffset(found.end));
    }
    Ok(ranges)
}

pub(crate) fn literal_ranges_in_bytes_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, scope: Range<usize>, options: LiteralOptions, state: &mut RegexState) -> Result<Vec<Range<ByteOffset>>, SearchError> {
    let mut ranges = Vec::new();
    visit_literal_ranges_in_bytes_with_state(bytes, query, scope, options, state, |range| {
        ranges.push(range);
        true
    })?;
    Ok(ranges)
}

pub(crate) fn visit_literal_ranges_in_bytes_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, scope: Range<usize>, options: LiteralOptions, state: &mut RegexState, visit: impl FnMut(Range<ByteOffset>) -> bool) -> Result<(), SearchError> {
    visit_literal_ranges_in_bytes_cancellable_with_state(bytes, query, scope, options, state, || false, visit)
}

pub(crate) fn visit_literal_ranges_in_bytes_cancellable_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, scope: Range<usize>, options: LiteralOptions, state: &mut RegexState, mut should_stop: impl FnMut() -> bool, mut visit: impl FnMut(Range<ByteOffset>) -> bool) -> Result<(), SearchError> {
    if scope.start > scope.end || scope.end > bytes.len() { return Err(SearchError::InvalidSelection); }
    if query.is_empty() { return Ok(()); }
    if matches!(options.mode, SearchMode::Regex { .. }) {
        return visit_regex_ranges_with_state(bytes, query, None, scope, options,
            RegexEmptyMode::NotAfterMatch, state, |range, _| visit(range));
    }
    let query = options.mode.decode(query);
    if query.is_empty() { return Ok(()); }
    let mut at = scope.start;
    while let Some(found) = bytes.find_literal_bytes_cancellable(query.as_bytes(), at, scope.end, options.match_case, options.whole_word, false, &mut should_stop)
        .map_err(|_| SearchError::InvalidSelection)? {
        at = found.end;
        if !visit(ByteOffset(found.start)..ByteOffset(found.end)) { break; }
    }
    Ok(())
}

pub(crate) fn literal_ranges_in_bytes_without_empty_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, scope: Range<usize>, options: LiteralOptions, state: &mut RegexState) -> Result<Vec<Range<ByteOffset>>, SearchError> {
    if matches!(options.mode, SearchMode::Regex { .. }) && !query.is_empty() {
        regex_ranges_with_state(bytes, query, None, scope, options, RegexEmptyMode::None, state)
            .map(|edits| edits.into_iter().map(|(range, _)| range).collect())
    } else { literal_ranges_in_bytes(bytes, query, scope, options) }
}

pub(crate) fn find_literal_bytes_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, selection: Selection, options: LiteralOptions, backwards: bool, kind: RegexFindKind, state: &mut RegexState) -> Result<Option<LiteralMatch>, SearchError> {
    let replacing = !matches!(kind, RegexFindKind::FindNext);
    let skip_empty = matches!(kind, RegexFindKind::ReplaceNext);
    if matches!(options.mode, SearchMode::Regex { .. }) && !query.is_empty() {
        regex_find_with_replacement_bytes_with_state(bytes, query, None, selection, options,
            backwards, replacing, skip_empty, state).map(|found| found.map(|(found, _)| found))
    } else { find_in_bytes(bytes, query, selection, options, backwards, replacing, skip_empty) }
}

pub(crate) fn find_literal_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, selection: Selection, options: LiteralOptions, backwards: bool) -> Result<Option<LiteralMatch>, SearchError> {
    find_in_bytes(bytes, query, selection, options, backwards, false, false)
}

pub(crate) fn find_literal_for_replace_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, selection: Selection, options: LiteralOptions, backwards: bool) -> Result<Option<LiteralMatch>, SearchError> {
    find_in_bytes(bytes, query, selection, options, backwards, true, false)
}

fn find_in_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, selection: Selection, options: LiteralOptions, backwards: bool, replacing: bool, skip_empty: bool) -> Result<Option<LiteralMatch>, SearchError> {
    let selected = selection.range();
    if selected.end.0 > bytes.len() { return Err(SearchError::InvalidSelection); }
    if query.is_empty() { return Ok(None); }
    if matches!(options.mode, SearchMode::Regex { .. }) {
        return regex_find_bytes(bytes, query, selection, options, backwards, replacing, skip_empty);
    }
    let query = options.mode.decode(query);
    if query.is_empty() { return Ok(None); }
    let from = if backwards {
        if replacing { selected.end.0 } else { selected.end.0.saturating_sub(1) }
    } else if replacing { selected.start.0 } else { selected.end.0 };
    let to = if backwards { 0 } else { bytes.len() };
    let found = if backwards && from == 0 { None } else {
        bytes.find_literal_bytes(query.as_bytes(), from, to, options.match_case, options.whole_word, false)
            .map_err(|_| SearchError::InvalidSelection)?
    };
    let (found, wrapped) = match found {
        Some(found) => (Some(found), false),
        None if options.wrap => (bytes.find_literal_bytes(query.as_bytes(), if backwards { bytes.len() } else { 0 }, if backwards { 0 } else { bytes.len() }, options.match_case, options.whole_word, false)
            .map_err(|_| SearchError::InvalidSelection)?, true),
        None => (None, false),
    };
    Ok(found.map(|range| LiteralMatch { range: ByteOffset(range.start)..ByteOffset(range.end), wrapped }))
}

pub(crate) fn find_incremental_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, selection: Selection, match_case: bool, advance: bool, backwards: bool) -> Result<Option<LiteralMatch>, SearchError> {
    let selected = selection.range();
    if selected.end.0 > bytes.len() { return Err(SearchError::InvalidSelection); }
    if query.is_empty() { return Ok(None); }
    let from = if backwards { selected.end.0.saturating_sub(usize::from(advance)) }
        else { selected.start.0.saturating_add(usize::from(advance)).min(bytes.len()) };
    let to = if backwards { 0 } else { bytes.len() };
    let found = if backwards && from == 0 { None } else {
        bytes.find_literal_bytes(query.as_bytes(), from, to, match_case, false, false).map_err(|_| SearchError::InvalidSelection)?
    };
    if let Some(found) = found { return Ok(Some(LiteralMatch { range: ByteOffset(found.start)..ByteOffset(found.end), wrapped: false })); }
    Ok(bytes.find_literal_bytes(query.as_bytes(), if backwards { bytes.len() } else { 0 }, if backwards { 0 } else { bytes.len() }, match_case, false, false)
        .map_err(|_| SearchError::InvalidSelection)?.map(|found| LiteralMatch { range: ByteOffset(found.start)..ByteOffset(found.end), wrapped: true }))
}

fn regex_scope(bytes: &crate::byte_buffer::ByteBuffer, scope: Range<usize>) -> Result<Range<usize>, SearchError> {
    if scope.start > scope.end || scope.end > bytes.len() { return Err(SearchError::InvalidSelection); }
    let start = bytes.move_position_outside_char(scope.start, true, false).map_err(|_| SearchError::InvalidSelection)?;
    let end = bytes.move_position_outside_char(scope.end, true, false).map_err(|_| SearchError::InvalidSelection)?;
    Ok(start..end)
}

fn next_regex_position(bytes: &crate::byte_buffer::ByteBuffer, position: usize) -> Result<usize, SearchError> {
    if bytes.byte(position) == Some(b'\r') && bytes.byte(position + 1) == Some(b'\n') {
        Ok(position + 2)
    } else {
        bytes.next_position(position, true).map(|next| next.max(position + 1)).map_err(|_| SearchError::InvalidSelection)
    }
}

fn regex_replacement_text(units: &[u16]) -> Result<String, SearchError> {
    let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
    let mut output = Vec::new();
    let mut at = 0;
    while at < end {
        let first = units[at];
        at += 1;
        let code = if (0xd800..=0xdfff).contains(&first) {
            let next = if at < end { units[at] } else { 0 };
            at += 1;
            0x10000 + (u32::from(first & 0x3ff) << 10) + u32::from(next & 0x3ff)
        } else { u32::from(first) };
        if code < 0x80 {
            output.push(code as u8);
        } else if code < 0x800 {
            output.extend([0xc0 | (code >> 6) as u8, 0x80 | (code & 0x3f) as u8]);
        } else if code < 0x10000 {
            output.extend([0xe0 | (code >> 12) as u8, 0x80 | ((code >> 6) & 0x3f) as u8, 0x80 | (code & 0x3f) as u8]);
        } else {
            output.extend([0xf0 | (code >> 18) as u8, 0x80 | ((code >> 12) & 0x3f) as u8,
                0x80 | ((code >> 6) & 0x3f) as u8, 0x80 | (code & 0x3f) as u8]);
        }
    }
    String::from_utf8(output).map_err(|error| SearchError::RegexRuntime(error.to_string()))
}

pub(crate) fn regex_matcher(query: &str, options: LiteralOptions) -> Result<crate::search_matcher::Matcher, SearchError> {
    crate::search_matcher::Matcher::new(query, options.match_case,
        matches!(options.mode, SearchMode::Regex { dot_matches_newline: true })).map_err(SearchError::Regex)
}

fn regex_edits_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: Option<&str>, scope: Range<usize>, options: LiteralOptions, empty_after_nonempty: bool) -> Result<Vec<(Range<ByteOffset>, String)>, SearchError> {
    regex_edits_bytes_with_state(bytes, query, replacement, scope, options, empty_after_nonempty, &mut RegexState::default())
}

fn regex_forward_capture<'a>(matcher: &'a crate::search_matcher::Matcher, bytes: &'a crate::byte_buffer::ByteBuffer, scope: Range<usize>, allow_empty: bool, allow_empty_at_start: bool, state: &mut RegexState) -> Result<Option<crate::search_matcher::Captures<'a>>, SearchError> {
    let scope = regex_scope(bytes, scope)?;
    let mut start = scope.start;
    while start <= scope.end {
        let Some(captures) = matcher.captures_in_bytes(bytes, start, scope.end).map_err(SearchError::RegexRuntime)? else { break; };
        let Some(found) = captures.get(0) else { break; };
        let range = ByteOffset(found.start())..ByteOffset(found.end());
        if !range.is_empty() || allow_empty && (allow_empty_at_start || range.start.0 > scope.start) {
            state.found(&range);
            return Ok(Some(captures));
        }
        start = next_regex_position(bytes, range.start.0)?;
    }
    state.last_match = None;
    Ok(None)
}

fn regex_edits_bytes_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: Option<&str>, scope: Range<usize>, options: LiteralOptions, empty_after_nonempty: bool, state: &mut RegexState) -> Result<Vec<(Range<ByteOffset>, String)>, SearchError> {
    regex_ranges_with_state(bytes, query, replacement, scope, options,
        if empty_after_nonempty { RegexEmptyMode::All } else { RegexEmptyMode::NotAfterMatch }, state)
}

fn regex_ranges_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: Option<&str>, scope: Range<usize>, options: LiteralOptions, empty_mode: RegexEmptyMode, state: &mut RegexState) -> Result<Vec<(Range<ByteOffset>, String)>, SearchError> {
    let mut edits = Vec::new();
    visit_regex_ranges_with_state(bytes, query, replacement, scope, options, empty_mode, state, |range, value| {
        edits.push((range, value));
        true
    })?;
    Ok(edits)
}

fn visit_regex_ranges_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: Option<&str>, scope: Range<usize>, options: LiteralOptions, empty_mode: RegexEmptyMode, state: &mut RegexState, mut visit: impl FnMut(Range<ByteOffset>, String) -> bool) -> Result<(), SearchError> {
    let limit = scope.end;
    if scope.start > scope.end || scope.end > bytes.len() { return Err(SearchError::InvalidSelection); }
    if scope.is_empty() || query.split('\0').next().unwrap_or_default().is_empty() { return Ok(()); }
    state.direction(scope.start as i128, scope.end as i128);
    let matcher = regex_matcher(query, options)?;
    let mut at = scope.start;
    while at <= scope.end {
        let allow_empty_at_start = !state.is_continuation(at as i128, false) || empty_mode == RegexEmptyMode::All && !state.last_was_empty();
        let Some(captures) = regex_forward_capture(&matcher, bytes, at..scope.end, empty_mode != RegexEmptyMode::None, allow_empty_at_start, state)? else { break; };
        let Some(found) = captures.get(0) else { break; };
        let range = ByteOffset(found.start())..ByteOffset(found.end());
        if range.end.0 > limit { break; }
        let empty = range.is_empty();
        let value = if let Some(template) = replacement {
            let names = captures.names();
            let units = replacement::format(template, &captures, &names);
            regex_replacement_text(&units)?
        } else { String::new() };
        if !visit(range.clone(), value) { break; }
        if range.end.0 == scope.end { break; }
        at = if empty { next_regex_position(bytes, range.end.0)? } else { range.end.0 };
    }
    Ok(())
}

fn regex_find_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, selection: Selection, options: LiteralOptions, backwards: bool, replacing: bool, skip_empty: bool) -> Result<Option<LiteralMatch>, SearchError> {
    regex_find_with_replacement_bytes(bytes, query, None, selection, options, backwards, replacing, skip_empty)
        .map(|found| found.map(|(found, _)| found))
}

pub(crate) fn find_replacement_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: &str, selection: Selection, options: LiteralOptions, backwards: bool) -> Result<Option<(LiteralMatch, String)>, SearchError> {
    if matches!(options.mode, SearchMode::Regex { .. }) {
        regex_find_with_replacement_bytes(bytes, query, Some(replacement), selection, options, backwards, true, false)
    } else {
        let replacement = options.mode.decode(replacement).into_owned();
        find_literal_for_replace_bytes(bytes, query, selection, options, backwards)
            .map(|found| found.map(|found| (found, replacement)))
    }
}

pub(crate) fn find_replacement_bytes_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: &str, selection: Selection, options: LiteralOptions, backwards: bool, state: &mut RegexState) -> Result<Option<(LiteralMatch, String)>, SearchError> {
    if matches!(options.mode, SearchMode::Regex { .. }) && !query.is_empty() {
        regex_find_with_replacement_bytes_with_state(bytes, query, Some(replacement), selection,
            options, backwards, true, false, state)
    } else { find_replacement_bytes(bytes, query, replacement, selection, options, backwards) }
}

pub(crate) fn regex_replacement_in_bytes_with_target(matcher: &crate::search_matcher::Matcher, bytes: &crate::byte_buffer::ByteBuffer, replacement: &str, scope: Range<usize>, state: &mut RegexState, last_target: &mut Option<Range<i128>>) -> Result<Option<(Range<ByteOffset>, String)>, SearchError> {
    *last_target = Some(scope.start as i128..scope.end as i128);
    if scope.is_empty() { return Ok(None); }
    state.direction(scope.start as i128, scope.end as i128);
    let limit = scope.end;
    let allow_empty_at_start = !state.is_continuation(scope.start as i128, false);
    let Some(captures) = regex_forward_capture(matcher, bytes, scope, true, allow_empty_at_start, state)? else { return Ok(None); };
    let Some(found) = captures.get(0) else { return Ok(None); };
    let range = ByteOffset(found.start())..ByteOffset(found.end());
    if range.end.0 > limit { return Ok(None); }
    *last_target = Some(range.start.0 as i128..range.end.0 as i128);
    let names = captures.names();
    let value = regex_replacement_text(&replacement::format(replacement, &captures, &names))?;
    Ok(Some((range, value)))
}

fn regex_find_with_replacement_bytes(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: Option<&str>, selection: Selection, options: LiteralOptions, backwards: bool, replacing: bool, skip_empty: bool) -> Result<Option<(LiteralMatch, String)>, SearchError> {
    let mut state = RegexState::default();
    if skip_empty { state.found(&selection.range()); }
    regex_find_with_replacement_bytes_with_state(bytes, query, replacement, selection, options,
        backwards, replacing, skip_empty, &mut state)
}

fn regex_find_with_replacement_bytes_with_state(bytes: &crate::byte_buffer::ByteBuffer, query: &str, replacement: Option<&str>, selection: Selection, options: LiteralOptions, backwards: bool, replacing: bool, skip_empty: bool, state: &mut RegexState) -> Result<Option<(LiteralMatch, String)>, SearchError> {
    let selected = selection.range();
    let from = if backwards { selected.end.0 as i128 - i128::from(!replacing) }
        else if replacing { selected.start.0 as i128 } else { selected.end.0 as i128 };
    let suppress_empty = from > 0 && bytes.byte(from as usize - 1) == Some(b'\r') && bytes.byte(from as usize) == Some(b'\n');
    let requested_backwards = backwards;
    let first_to = if requested_backwards { 0 } else { bytes.len() as i128 };
    let first_backwards = state.direction(from, first_to);
    let matcher = regex_matcher(query, options)?;
    let mut wrapped = false;
    loop {
        let current_from = if wrapped { if requested_backwards { bytes.len() as i128 } else { 0 } } else { from };
        let current_to = if wrapped { if requested_backwards { 0 } else { bytes.len() as i128 } } else { first_to };
        let backwards = if wrapped { state.direction(current_from, current_to) } else { first_backwards };
        let allow_empty_at_start = replacing && !skip_empty || !state.is_continuation(current_from, backwards)
            || !skip_empty && !state.last_was_empty();
        let scope = current_from.min(current_to).max(0) as usize..current_from.max(current_to).max(0) as usize;
        let scope = regex_scope(bytes, scope)?;
        let mut start = scope.start;
        let continuation = !wrapped && !allow_empty_at_start;
        let mut best: Option<(Range<ByteOffset>, crate::search_matcher::Captures<'_>)> = None;
        while start <= scope.end {
            let Some(captures) = matcher.captures_in_bytes(bytes, start, scope.end).map_err(SearchError::RegexRuntime)? else { break; };
            let Some(found) = captures.get(0) else { break; };
            let range = ByteOffset(found.start())..ByteOffset(found.end());
            let forbidden = range.is_empty() && (suppress_empty
                || continuation && range.start.0 == if backwards { scope.end } else { scope.start });
            if !forbidden {
                if !backwards {
                    let value = if range == selected && let Some(template) = replacement {
                        let names = captures.names();
                        regex_replacement_text(&replacement::format(template, &captures, &names))?
                    } else { String::new() };
                    state.found(&range);
                    return Ok(Some((LiteralMatch { range, wrapped }, value)));
                }
                if best.as_ref().is_none_or(|(previous, _)| range.end > previous.end) {
                    best = Some((range.clone(), captures));
                }
            }
            start = next_regex_position(bytes, range.start.0)?;
        }
        if let Some((range, captures)) = best {
            let value = if range == selected && let Some(template) = replacement {
                let names = captures.names();
                regex_replacement_text(&replacement::format(template, &captures, &names))?
            } else { String::new() };
            state.found(&range);
            return Ok(Some((LiteralMatch { range, wrapped }, value)));
        }
        state.last_match = None;
        if wrapped || !options.wrap { return Ok(None); }
        wrapped = true;
    }
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
    if matches!(options.mode, SearchMode::Regex { .. }) {
        return literal_ranges_in(text, query, scope, options).map(|ranges| ranges.len());
    }
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
    if matches!(options.mode, SearchMode::Regex { .. }) {
        return regex_edits(text, query, None, scope, options, false)
            .map(|edits| edits.into_iter().map(|(range, _)| range).collect());
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
    find_literal_impl(text, query, selection, options, backwards, false, false)
}

pub fn find_literal_for_replace(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
) -> Result<Option<LiteralMatch>, SearchError> {
    find_literal_impl(text, query, selection, options, backwards, true, false)
}

pub fn find_after_empty_match(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
) -> Result<Option<LiteralMatch>, SearchError> {
    find_literal_impl(text, query, selection, options, backwards, false, true)
}

fn find_literal_impl(
    text: &str,
    query: &str,
    selection: Selection,
    options: LiteralOptions,
    backwards: bool,
    replacing: bool,
    skip_empty: bool,
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
    if matches!(options.mode, SearchMode::Regex { .. }) {
        return regex_find_bytes(&crate::byte_buffer::ByteBuffer::from_bytes(text.as_bytes()), query,
            selection, options, backwards, replacing, skip_empty);
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

fn regex_edits(
    text: &str,
    query: &str,
    replacement: Option<&str>,
    scope: Range<usize>,
    options: LiteralOptions,
    empty_after_nonempty: bool,
) -> Result<Vec<(Range<ByteOffset>, String)>, SearchError> {
    regex_edits_bytes(&crate::byte_buffer::ByteBuffer::from_bytes(text.as_bytes()), query,
        replacement, scope, options, empty_after_nonempty)
}

pub fn replacement_edits(
    text: &str,
    query: &str,
    replacement: &str,
    scope: Range<usize>,
    options: LiteralOptions,
) -> Result<Vec<(Range<ByteOffset>, String)>, SearchError> {
    if scope.start > scope.end
        || scope.end > text.len()
        || !text.is_char_boundary(scope.start)
        || !text.is_char_boundary(scope.end)
    {
        return Err(SearchError::InvalidSelection);
    }
    if matches!(options.mode, SearchMode::Regex { .. }) {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        regex_edits(text, query, Some(replacement), scope, options, false)
    } else {
        let value = options.mode.decode(replacement).into_owned();
        literal_ranges_in(text, query, scope, options).map(|ranges| {
            ranges
                .into_iter()
                .map(|range| (range, value.clone()))
                .collect()
        })
    }
}
