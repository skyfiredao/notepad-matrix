use super::{
    ByteOffset, ColumnPosition, Document, EditError, Selection, SelectionRange, SelectionSet,
    TextTransform,
};
use ropey::LineType;

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
    pub fn paste_rectangle(
        &mut self,
        text: &str,
        tab_width: usize,
        eol: &str,
    ) -> Result<(), EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let mut candidate = self.text_snapshot();
        let mut selections = self.selections();
        let mut edits = super::LineEdits::default();
        let mut order: Vec<_> = (0..selections.ranges().len()).collect();
        order.sort_by_key(|&index| selections.ranges()[index].selection.range().start);
        for &index in order.iter().rev() {
            let range = selections.ranges()[index].selection.range();
            edits.remove(&mut candidate, range.start.0..range.end.0)?;
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
        self.commit_prepared(
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
        self.text_in(position..position)?;
        let text = self.text_snapshot();
        let line = text.byte_to_line_idx(position.0, LineType::LF_CR);
        let start = text.line_to_byte_idx(line, LineType::LF_CR);
        let mut column = 0;
        for ch in text.slice(start..position.0).chars() {
            if matches!(ch, '\r' | '\n') {
                break;
            }
            column = if ch == '\t' {
                column + tab_width.max(1) - column % tab_width.max(1)
            } else {
                column + 1
            };
        }
        Ok(column)
    }

    pub fn position_at_column(
        &self,
        target: ColumnPosition,
        tab_width: usize,
    ) -> Result<(ByteOffset, usize), EditError> {
        let start = self.line_start(target.line)?.0;
        let line = self.line_text(target.line)?;
        let mut column = 0;
        let mut position = start;
        for ch in line.trim_end_matches(['\r', '\n']).chars() {
            if column >= target.column {
                break;
            }
            let next = if ch == '\t' {
                column + tab_width.max(1) - column % tab_width.max(1)
            } else {
                column + 1
            };
            if next > target.column {
                return Ok((ByteOffset(position), 0));
            }
            column = next;
            position += ch.len_utf8();
        }
        Ok((ByteOffset(position), target.column.saturating_sub(column)))
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
        let text = self.text_snapshot();
        let first = text.byte_to_line_idx(caret.0, LineType::LF_CR);
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
    ) -> Result<(), EditError> {
        if ranges.is_empty() {
            return Ok(());
        }
        let originals = self.selections();
        let preserve_ranges = originals.is_rectangular() || originals.ranges().len() > 1;
        let mut transforms = Vec::new();
        let mut final_ranges = originals.clone();
        let mut order: Vec<_> = (0..ranges.len()).collect();
        order.sort_by_key(|&index| ranges[index].selection.range().start);
        let mut removed = 0;
        let mut inserted = 0;
        for (sequence, &index) in order.iter().enumerate() {
            let range = ranges[index];
            let value = &values[sequence];
            let start =
                range.selection.range().start.0 + inserted - removed + range.start_virtual();
            if preserve_ranges {
                let end = start + value.len();
                let forward = (range.selection.anchor, range.anchor_virtual)
                    <= (range.selection.head, range.head_virtual);
                final_ranges.ranges[index] = Selection {
                    anchor: ByteOffset(if forward { start } else { end }),
                    head: ByteOffset(if forward { end } else { start }),
                }
                .into();
            }
            let text = format!("{}{}", " ".repeat(range.start_virtual()), value);
            removed += range.selection.range().end.0 - range.selection.range().start.0;
            inserted += text.len();
            transforms.push(TextTransform {
                range: range.selection.range(),
                text,
            });
        }
        self.apply_transforms(&transforms)?;
        if preserve_ranges {
            *super::lock(&self.selection) = final_ranges;
            super::lock(&self.core).remember_redo_selections();
        }
        Ok(())
    }

    pub fn column_text(&mut self, text: &str, tab_width: usize) -> Result<(), EditError> {
        let ranges = self.column_targets(tab_width)?;
        let value = text.split('\0').next().unwrap_or("").to_string();
        self.column_replace(&ranges, &vec![value; ranges.len()])
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
        self.column_replace(&ranges, &values)
    }
}

pub(super) fn insert_rectangle(
    candidate: &mut ropey::Rope,
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
    let first = candidate.byte_to_line_idx(position.0, LineType::LF_CR);
    let start = candidate.line_to_byte_idx(first, LineType::LF_CR);
    let mut column = 0;
    for ch in candidate.slice(start..position.0).chars() {
        column = if ch == '\t' {
            column + tab_width.max(1) - column % tab_width.max(1)
        } else {
            column + 1
        };
    }
    column += virtual_space;
    let normalized = text
        .trim_end_matches(['\r', '\n'])
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if normalized.is_empty() {
        return Ok(());
    }
    for (offset, line) in normalized.split('\n').enumerate() {
        let row = first + offset;
        while row >= candidate.len_lines(LineType::LF_CR) {
            let end = candidate.len();
            edits.insert(candidate, end, eol)?;
        }
        let start = candidate.line_to_byte_idx(row, LineType::LF_CR);
        let content = candidate.line(row, LineType::LF_CR).to_string();
        let mut current = 0;
        let mut at = start;
        for ch in content.trim_end_matches(['\r', '\n']).chars() {
            if current >= column {
                break;
            }
            let next = if ch == '\t' {
                current + tab_width.max(1) - current % tab_width.max(1)
            } else {
                current + 1
            };
            if next > column {
                break;
            }
            current = next;
            at += ch.len_utf8();
        }
        let value = format!("{}{}", " ".repeat(column.saturating_sub(current)), line);
        edits.insert(candidate, at, &value)?;
    }
    Ok(())
}
