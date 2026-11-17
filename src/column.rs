use super::{
    ByteOffset, ColumnPosition, Document, EditError, Selection, SelectionRange, SelectionSet,
};
use crate::byte_buffer::ByteBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnBase {
    Decimal,
    Hexadecimal,
    HexadecimalUpper,
    Octal,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnFill {
    None,
    Zero,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnNumbers {
    pub start: u64,
    pub step: u64,
    pub repeat: usize,
    pub base: ColumnBase,
    pub fill: ColumnFill,
}

impl ColumnNumbers {
    fn format(self, number: u64) -> String {
        match self.base {
            ColumnBase::Decimal => format!("{number}"),
            ColumnBase::Hexadecimal => format!("{number:x}"),
            ColumnBase::HexadecimalUpper => format!("{number:X}"),
            ColumnBase::Octal => format!("{number:o}"),
            ColumnBase::Binary => format!("{number:b}"),
        }
    }
}

impl Document {
    pub(crate) fn paste_to_multi_selection(&mut self, text: &str, eol: &str) -> Result<(), EditError> {
        if self.is_read_only() { return Err(EditError::ReadOnly); }
        let text = text.split('\0').next().unwrap_or_default();
        let separator = if text.contains("\r\n") { "\r\n" } else if text.contains('\n') { "\n" }
            else if text.contains('\r') { "\r" } else { eol };
        let mut rows: Vec<_> = text.split(separator).collect();
        rows.pop();
        let mut selections = self.selections();
        let count = selections.ranges().len();
        if count <= 1 || selections.is_rectangular() { return Err(EditError::InvalidRange); }
        let per_selection = if count >= rows.len() { 1 } else { rows.len() / count };
        let destinations = count.min(rows.len());
        let mut bytes = self.byte_snapshot();
        let mut edits = super::LineEdits::default();
        for index in 0..destinations {
            let value = rows[index * per_selection..(index + 1) * per_selection].join(eol);
            let range = selections.ranges()[index].selection.range();
            let start = range.start.0;
            let end = range.end.0;
            selections.map_edit(start, end - start, 0);
            selections.map_edit(start, 0, value.len());
            edits.change_bytes(&mut bytes, start..end, value.as_bytes())?;
            let caret = ByteOffset(start + value.len());
            let range = &mut selections.ranges[index];
            range.set_start(caret);
            range.set_end(caret);
        }
        self.commit_prepared_bytes(bytes, selections, edits)
    }

    pub fn paste_rectangle(
        &mut self,
        text: &str,
        tab_width: usize,
        eol: &str,
    ) -> Result<(), EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let mut candidate = self.byte_snapshot();
        let mut selections = self.selections();
        let mut edits = super::LineEdits::default();
        let mut order: Vec<_> = (0..selections.ranges().len()).collect();
        order.sort_by_key(|&index| selections.ranges()[index].selection.range().start);
        for &index in order.iter().rev() {
            let range = selections.ranges()[index].selection.range();
            edits.change_bytes(&mut candidate, range.start.0..range.end.0, &[])?;
            selections.map_edit(range.start.0, range.end.0 - range.start.0, 0);
        }
        let main = selections.primary();
        let position = main.selection.range().start;
        insert_rectangle(
            &mut candidate,
            &mut edits,
            position,
            main.start_virtual(),
            text,
            tab_width,
            eol,
        )?;
        self.commit_prepared_bytes(
            candidate,
            SelectionSet::single(Selection {
                anchor: position,
                head: position,
            }),
            edits,
        )
    }

    pub fn display_column(
        &self,
        position: ByteOffset,
        tab_width: usize,
    ) -> Result<usize, EditError> {
        self.byte_snapshot().column(position.0, tab_width).map_err(|error| EditError::Storage(error.to_string()))
    }

    pub fn position_at_column(
        &self,
        target: ColumnPosition,
        tab_width: usize,
    ) -> Result<(ByteOffset, usize), EditError> {
        let bytes = self.byte_snapshot();
        let position = bytes.find_column(target.line, target.column, tab_width).map_err(|error| EditError::Storage(error.to_string()))?;
        let space = if bytes.is_line_end(position) {
            target.column.saturating_sub(bytes.column(position, tab_width).map_err(|error| EditError::Storage(error.to_string()))?)
        } else { 0 };
        Ok((ByteOffset(position), space))
    }

    pub fn select_rectangle(
        &mut self,
        anchor: ColumnPosition,
        head: ColumnPosition,
        tab_width: usize,
    ) -> Result<(), EditError> {
        if anchor.line >= self.line_count() || head.line >= self.line_count() {
            return Err(EditError::InvalidRange);
        }
        let top = anchor.line.min(head.line);
        let bottom = anchor.line.max(head.line);
        let mut ranges = Vec::new();
        for line in top..=bottom {
            let (anchor_position, anchor_virtual) = self.position_at_column(
                ColumnPosition {
                    line,
                    column: anchor.column,
                },
                tab_width,
            )?;
            let (head_position, head_virtual) = self.position_at_column(
                ColumnPosition {
                    line,
                    column: head.column,
                },
                tab_width,
            )?;
            ranges.push(SelectionRange {
                selection: Selection {
                    anchor: anchor_position,
                    head: head_position,
                },
                anchor_virtual,
                head_virtual,
            });
        }
        let mut selections = SelectionSet::new(ranges, head.line - top)?;
        selections.rectangular = true;
        self.set_selections(selections)
    }

    fn column_targets(&self, tab_width: usize) -> Result<Vec<SelectionRange>, EditError> {
        let selections = self.selections();
        if selections.is_rectangular() && selections.ranges().len() == 1 {
            return Ok(Vec::new());
        }
        if selections.is_rectangular() || selections.ranges().len() > 1 {
            return Ok(selections.ranges().to_vec());
        }
        let caret = self.selection().head;
        let first = self.byte_snapshot().line_index(caret.0).map_err(|error| EditError::Storage(error.to_string()))?;
        let column = self.display_column(caret, tab_width)?;
        (first..self.line_count())
            .map(|line| {
                let (position, space) =
                    self.position_at_column(ColumnPosition { line, column }, tab_width)?;
                Ok(SelectionRange {
                    selection: Selection {
                        anchor: position,
                        head: position,
                    },
                    anchor_virtual: space,
                    head_virtual: space,
                })
            })
            .collect()
    }

    fn column_replace(
        &mut self,
        ranges: &[SelectionRange],
        values: &[String],
        numeric_width: Option<usize>,
        tab_width: usize,
    ) -> Result<(), EditError> {
        if ranges.is_empty() {
            return Ok(());
        }
        let originals = self.selections();
        let numeric = numeric_width.is_some();
        let preserve_ranges = originals.is_rectangular() || originals.ranges().len() > 1;
        let mut bytes = self.byte_snapshot();
        let read_only = self.is_read_only();
        let mut selections = originals.clone();
        let mut edits = super::LineEdits::default();
        if !preserve_ranges {
            if read_only { return Ok(()); }
            let caret = originals.primary().selection.head.0;
            let first = bytes.line_index(caret).map_err(|error| EditError::Storage(error.to_string()))?;
            let column = bytes.column(caret, tab_width).map_err(|error| EditError::Storage(error.to_string()))?;
            for (index, value) in values.iter().enumerate() {
                let row = first + index;
                let start = bytes.line_start(row).map_err(|error| EditError::Storage(error.to_string()))?;
                let mut end = start;
                while end < bytes.len() && !bytes.is_line_end(end) { end += 1; }
                let end_column = bytes.column(end, tab_width).map_err(|error| EditError::Storage(error.to_string()))?;
                let raw = bytes.slice(start..end).map_err(|error| EditError::Storage(error.to_string()))?.to_vec();
                let raw = &raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())];
                let position = bytes.find_column(row, column, tab_width).map_err(|error| EditError::Storage(error.to_string()))?;
                let offset = position - start;
                let replacement = if numeric {
                    let mut units: Vec<_> = String::from_utf8_lossy(raw).encode_utf16().collect();
                    if end_column < column {
                        units.extend(std::iter::repeat_n(u16::from(b' '), column - end_column));
                        units.extend(value.encode_utf16());
                    } else {
                        let at = offset.min(units.len());
                        units.splice(at..at, value.encode_utf16());
                    }
                    String::from_utf16_lossy(&units).into_bytes()
                } else {
                    let mut text = raw.to_vec();
                    if end_column < column {
                        text.extend(std::iter::repeat_n(b' ', column - end_column));
                        text.extend_from_slice(value.as_bytes());
                    } else {
                        let at = offset.min(text.len());
                        text.splice(at..at, value.bytes());
                    }
                    text
                };
                selections.map_edit(start, end - start, 0);
                selections.map_edit(start, 0, replacement.len());
                edits.change_bytes(&mut bytes, start..end, &replacement)?;
            }
            return self.commit_prepared_bytes(bytes, selections, edits);
        }
        let mut final_ranges = originals.ranges().to_vec();
        let mut order: Vec<_> = (0..ranges.len()).collect();
        order.sort_by_key(|&index| ranges[index].selection.range().start);
        let mut shift = 0isize;
        for (sequence, &index) in order.iter().enumerate() {
            let range = ranges[index];
            let value = &values[sequence];
            let source = range.selection.range();
            let mut start = source.start.0.checked_add_signed(shift).ok_or(EditError::InvalidRange)?;
            let mut end = source.end.0.checked_add_signed(shift).ok_or(EditError::InvalidRange)?;
            let diff = isize::try_from(numeric_width.unwrap_or(value.len())).map_err(|_| EditError::InvalidRange)?
                - isize::try_from(source.end.0 - source.start.0).map_err(|_| EditError::InvalidRange)?;
            let anchor_space = if numeric { range.anchor_virtual.min(range.head_virtual) } else { range.anchor_virtual };
            let caret_space = if numeric { range.anchor_virtual.max(range.head_virtual) } else { range.head_virtual };
            if anchor_space > 0 {
                for space in 0..caret_space {
                    let at = start + space;
                    if at > bytes.len() {
                        if !read_only { self.commit_prepared_bytes(bytes, selections, edits)?; }
                        return Err(EditError::InvalidRange);
                    }
                    if !read_only {
                        edits.change_bytes(&mut bytes, at..at, b" ")?;
                        selections.map_edit(at, 0, 1);
                    }
                }
                start += anchor_space;
                end += caret_space;
            }
            if start > bytes.len() {
                if !read_only { self.commit_prepared_bytes(bytes, selections, edits)?; }
                return Err(EditError::InvalidRange);
            }
            if !read_only {
                if end > start && end <= bytes.len() {
                    edits.change_bytes(&mut bytes, start..end, &[])?;
                    selections.map_edit(start, end - start, 0);
                }
                edits.change_bytes(&mut bytes, start..start, value.as_bytes())?;
                selections.map_edit(start, 0, value.len());
            }
            let delta = if anchor_space > 0 {
                isize::try_from(anchor_space + value.len()).map_err(|_| EditError::InvalidRange)?
            } else { diff };
            shift = shift.checked_add(delta).ok_or(EditError::InvalidRange)?;
            end = end.checked_add_signed(diff).ok_or(EditError::InvalidRange)?;
            let forward = if range.selection.anchor == range.selection.head && originals.is_rectangular() {
                range.anchor_virtual < range.head_virtual
            } else { range.selection.anchor <= range.selection.head };
            final_ranges[index] = SelectionRange {
                selection: Selection {
                    anchor: ByteOffset(if forward { start } else { end }),
                    head: ByteOffset(if forward { end } else { start }),
                },
                anchor_virtual: if anchor_space > 0 { 0 } else { range.anchor_virtual },
                head_virtual: if anchor_space > 0 { 0 } else { range.head_virtual },
            };
        }
        if !read_only { self.commit_prepared_bytes(bytes, selections, edits)?; }
        self.set_multi_selection_ranges(&final_ranges);
        Ok(())
    }

    pub fn column_text(&mut self, text: &str, tab_width: usize) -> Result<(), EditError> {
        let ranges = self.column_targets(tab_width)?;
        let value = text.split('\0').next().unwrap_or("").to_string();
        self.column_replace(&ranges, &vec![value; ranges.len()], None, tab_width)
    }

    pub fn column_numbers(
        &mut self,
        options: ColumnNumbers,
        tab_width: usize,
    ) -> Result<(), EditError> {
        let ranges = self.column_targets(tab_width)?;
        let repeat = options.repeat.max(1);
        let numbers: Vec<_> = (0..ranges.len())
            .map(|index| {
                options
                    .start
                    .wrapping_add(options.step.wrapping_mul((index / repeat) as u64))
            })
            .collect();
        let width = options.format(options.start).len().max(
            numbers
                .last()
                .map_or(0, |&value| options.format(value).len()),
        );
        let values = numbers
            .into_iter()
            .map(|number| {
                let number = options.format(number);
                match options.fill {
                    ColumnFill::None => format!(
                        "{}{}",
                        number,
                        " ".repeat(width.saturating_sub(number.len()))
                    ),
                    ColumnFill::Zero => format!(
                        "{}{}",
                        "0".repeat(width.saturating_sub(number.len())),
                        number
                    ),
                    ColumnFill::Space => format!(
                        "{}{}",
                        " ".repeat(width.saturating_sub(number.len())),
                        number
                    ),
                }
            })
            .collect::<Vec<_>>();
        self.column_replace(&ranges, &values, Some(width), tab_width)
    }
}

pub(super) fn insert_rectangle(
    candidate: &mut ByteBuffer,
    edits: &mut super::LineEdits,
    position: ByteOffset,
    virtual_space: usize,
    text: &str,
    tab_width: usize,
    eol: &str,
) -> Result<(), EditError> {
    if !matches!(eol, "\r" | "\n" | "\r\n") {
        return Err(EditError::InvalidRange);
    }
    let first = candidate.line_index(position.0).map_err(|error| EditError::Storage(error.to_string()))?;
    let column = candidate.column(position.0, tab_width).map_err(|error| EditError::Storage(error.to_string()))? + virtual_space;
    let normalized = text
        .trim_end_matches(['\r', '\n'])
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if normalized.is_empty() {
        return Ok(());
    }
    for (offset, line) in normalized.split('\n').enumerate() {
        let row = first + offset;
        while row >= candidate.line_count() {
            let end = candidate.len();
            edits.change_bytes(candidate, end..end, eol.as_bytes())?;
        }
        let at = candidate.find_column(row, column, tab_width).map_err(|error| EditError::Storage(error.to_string()))?;
        let current = candidate.column(at, tab_width).map_err(|error| EditError::Storage(error.to_string()))?;
        let value = format!("{}{}", " ".repeat(column.saturating_sub(current)), line);
        edits.change_bytes(candidate, at..at, value.as_bytes())?;
    }
    Ok(())
}
