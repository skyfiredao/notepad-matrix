use std::ops::Range;

pub fn baseline_shift(height: f32, baseline: f32, ascent: f32, descent: f32) -> f32 {
    baseline - ((height - ascent - descent) / 2. + ascent)
}

pub fn next_tab_stop(x: f32, default_space_width: f32, columns: usize) -> f32 {
    let interval = (default_space_width * columns.max(1) as f32).max(1.);
    ((x / interval).floor() + 1.) * interval
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualRow {
    pub logical_line: usize,
    pub range: Range<usize>,
    pub indent: f32,
    pub first: bool,
    pub last: bool,
}

#[derive(Clone, Debug, Default)]
pub struct VisualRows(Vec<VisualRow>);

impl VisualRows {
    pub fn push_line(
        &mut self,
        logical_line: usize,
        start: usize,
        ranges: &[Range<usize>],
        indent: f32,
    ) {
        for (index, range) in ranges.iter().enumerate() {
            self.0.push(VisualRow {
                logical_line,
                range: start + range.start..start + range.end,
                indent: if index == 0 { 0. } else { indent },
                first: index == 0,
                last: index + 1 == ranges.len(),
            });
        }
    }
    pub fn rows(&self) -> &[VisualRow] {
        &self.0
    }
    pub fn hide_folded(&mut self, folds: &[(usize, usize)]) {
        self.0.retain(|row| {
            !folds
                .iter()
                .any(|(header, end)| row.logical_line > *header && row.logical_line <= *end)
        });
    }
    pub fn hide_lines(&mut self, hidden: &[(usize, usize)]) {
        self.0.retain(|row| !hidden.iter().any(|&(start, end)| {
            row.logical_line >= start && row.logical_line <= end
        }));
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn row_for_byte(&self, byte: usize) -> usize {
        self.0
            .partition_point(|row| row.range.start <= byte)
            .saturating_sub(1)
    }

    pub fn restored_top(
        &self,
        line: usize,
        offset: usize,
        wrap_count: usize,
        wrapped: bool,
    ) -> usize {
        let last = self
            .0
            .partition_point(|row| row.logical_line <= line)
            .saturating_sub(1);
        let actual_line = self.0.get(last).map_or(0, |row| row.logical_line);
        let first = self.0.partition_point(|row| row.logical_line < actual_line);
        let count = self
            .0
            .partition_point(|row| row.logical_line <= actual_line)
            - first;
        first
            + if actual_line == line && wrapped && count == wrap_count {
                offset.min(count.saturating_sub(1))
            } else {
                0
            }
    }
}

pub fn wrap_ranges(
    text: &str,
    width: f32,
    indent: f32,
    x_for_byte: impl Fn(usize) -> f32,
) -> Vec<Range<usize>> {
    let width = if width.is_finite() { width.max(1.) } else { 1. };
    let indent = if indent.is_finite() {
        indent.max(0.).min(width - 1.)
    } else {
        0.
    };
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut rows = Vec::new();
    let mut start = 0;
    while start + 1 < boundaries.len() {
        let available = width - if start == 0 { 0. } else { indent };
        let origin = x_for_byte(boundaries[start]);
        let mut end = start + 1;
        while end + 1 < boundaries.len() && x_for_byte(boundaries[end + 1]) - origin < available {
            end += 1;
        }
        if end + 1 < boundaries.len() {
            let whitespace_break = (start + 1..=end).rev().find(|index| {
                let before = text[..boundaries[*index]].chars().next_back();
                let after = text[boundaries[*index]..].chars().next();
                before.is_some_and(|ch| matches!(ch, ' ' | '\t'))
                    && !after.is_some_and(|ch| matches!(ch, ' ' | '\t'))
            });
            if let Some(word) = whitespace_break {
                end = word;
            }
        }
        let unsafe_break = |index: usize| {
            let next = text[boundaries[index]..].chars().next();
            let previous = text[..boundaries[index]].chars().next_back();
            next.is_some_and(|ch| {
                matches!(ch, '\u{300}'..='\u{36f}' | '\u{1ab0}'..='\u{1aff}'
                | '\u{1dc0}'..='\u{1dff}' | '\u{20d0}'..='\u{20ff}' | '\u{fe00}'..='\u{fe0f}'
                | '\u{fe20}'..='\u{fe2f}' | '\u{1f3fb}'..='\u{1f3ff}' | '\u{200d}')
            }) || previous == Some('\u{200d}')
        };
        while end > start + 1 && unsafe_break(end) {
            end -= 1;
        }
        while end + 1 < boundaries.len() && unsafe_break(end) {
            end += 1;
        }
        rows.push(boundaries[start]..boundaries[end]);
        start = end;
    }
    if rows.is_empty() {
        rows.push(0..0);
    }
    rows
}

pub(super) fn split_breaks(
    bytes: &crate::byte_buffer::ByteBuffer,
    range: Range<usize>,
    width: f32,
    indent: f32,
    x_for_byte: impl Fn(usize) -> f32,
    style_boundary: impl Fn(usize, usize) -> bool,
) -> Result<Vec<usize>, String> {
    let mut breaks = Vec::new();
    if width > x_for_byte(range.end) { return Ok(breaks); }
    let mut last_start = range.start;
    let mut offset = width;
    let mut position = range.start;
    while position < range.end {
        while position < range.end && x_for_byte(position + 1) < offset { position += 1; }
        if position == range.end { break; }
        let mut good = if position > range.start {
            bytes.move_position_outside_char(position, false, false).map_err(|error| error.to_string())?
        } else { position };
        let mut candidate = good;
        while candidate > last_start {
            if style_boundary(candidate - 1, candidate)
                || bytes.byte(candidate - 1).is_some_and(|byte| byte <= b' ')
                    && !bytes.byte(candidate).is_some_and(|byte| byte <= b' ') {
                break;
            }
            candidate = bytes.next_position(candidate, false).map_err(|error| error.to_string())?;
        }
        if candidate > last_start {
            good = candidate;
        } else {
            let after = bytes.next_position(good, true).map_err(|error| error.to_string())?;
            if let Some(base) = bytes.discard_last_combined_character(last_start..after).map_err(|error| error.to_string())?
                && base != last_start {
                good = base;
            }
            if good == last_start { good = bytes.next_position(good, true).map_err(|error| error.to_string())?; }
        }
        breaks.push(good);
        last_start = good;
        offset = x_for_byte(last_start) + width - indent;
        position = last_start + 1;
    }
    Ok(breaks)
}
