use super::{ByteOffset, Composition, Document, EditError, EditorSession, Selection, SelectionSet};
use crate::editor::{LineEdits, Utf16Offset};
use std::ops::Range;

pub(super) struct InlineComposition {
    base_selections: SelectionSet,
    pub(super) selections: SelectionSet,
    pub(super) source_selections: SelectionSet,
    edits: LineEdits,
    pub(super) marked_ranges: Vec<Range<ByteOffset>>,
    full_ranges: Vec<Range<ByteOffset>>,
}

impl EditorSession {
    pub fn mark_inline_display_range(
        &mut self,
        range: Option<Range<ByteOffset>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> Result<(), EditError> {
        if self
            .active_composition()
            .is_some_and(|composition| composition.inline.is_some())
        {
            return self.update_inline_range(range, text, selected_utf16);
        }
        let Some(range) = range else {
            return self.mark_inline_composition(text, selected_utf16);
        };
        let range = self.document.projection().source_selection(Selection { anchor: range.start, head: range.end })?.range();
        self.document.text_in(range.clone())?;
        let mut selections = self.document.selections();
        selections.set_range(
            selections.primary_index(),
            Selection {
                anchor: range.start,
                head: range.end,
            }
            .into(),
        );
        self.mark_inline_with_selections(text, selected_utf16, Some(selections))
    }

    pub fn mark_inline_composition(
        &mut self,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> Result<(), EditError> {
        self.mark_inline_with_selections(text, selected_utf16, None)
    }

    fn mark_inline_with_selections(
        &mut self,
        text: &str,
        selected_utf16: Option<Range<usize>>,
        replacement_selections: Option<SelectionSet>,
    ) -> Result<(), EditError> {
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.discard_stale_composition();
        let inserted = Document::new(text);
        let relative = selected_utf16.unwrap_or(inserted.len_utf16()..inserted.len_utf16());
        if relative.start > relative.end {
            return Err(EditError::InvalidRange);
        }
        let anchor_relative = inserted.from_utf16(Utf16Offset(relative.start))?.0;
        let head_relative = inserted.from_utf16(Utf16Offset(relative.end))?.0;
        let base_selections = replacement_selections
            .or_else(|| {
                self.active_composition()
                    .and_then(|composition| composition.inline.as_ref())
                    .map(|inline| inline.base_selections.clone())
            })
            .unwrap_or_else(|| self.document.selections());
        let mut selections = base_selections.clone();
        let mut bytes = self.document.byte_snapshot();
        let mut edits = LineEdits::default();
        let mut order: Vec<_> = (0..selections.ranges().len()).collect();
        for index in 0..selections.ranges().len() {
            let selected = selections.ranges()[index];
            let range = selected.selection.range();
            let padding = " ".repeat(selected.start_virtual());
            edits.change_bytes(&mut bytes, range.start.0..range.end.0, &[])?;
            edits.change_bytes(&mut bytes, range.start.0..range.start.0, padding.as_bytes())?;
            selections.map_edit(range.start.0, range.end.0 - range.start.0, padding.len());
            let caret = ByteOffset(range.start.0 + padding.len());
            selections.set_range(
                index,
                Selection {
                    anchor: caret,
                    head: caret,
                }
                .into(),
            );
        }
        for ch in text.chars() {
            order.sort_by_key(|&index| selections.ranges()[index].selection.head);
            for &index in order.iter().rev() {
                let position = selections.ranges()[index].selection.head.0;
                let mut end = position;
                if self.overwrite && position < bytes.len() && !matches!(bytes.byte(position), Some(b'\r' | b'\n')) {
                    end = bytes.deletion_range(position, false).map_err(|error| EditError::Storage(error.to_string()))?.end;
                }
                let mut encoded = [0; 4];
                let value = ch.encode_utf8(&mut encoded);
                edits.change_bytes(&mut bytes, position..end, &[])?;
                edits.change_bytes(&mut bytes, position..position, value.as_bytes())?;
                selections.map_edit(position, end - position, value.len());
                let caret = ByteOffset(position + value.len());
                selections.set_range(
                    index,
                    Selection {
                        anchor: caret,
                        head: caret,
                    }
                    .into(),
                );
            }
        }
        let primary_end = selections.primary().selection.head.0;
        let mut marked_range =
            ByteOffset(primary_end.saturating_sub(text.len()))..ByteOffset(primary_end);
        let mut marked_ranges: Vec<_> = selections
            .ranges()
            .iter()
            .map(|range| {
                ByteOffset(range.selection.head.0.saturating_sub(text.len()))..range.selection.head
            })
            .collect();
        for index in 0..selections.ranges().len() {
            let start = selections.ranges()[index]
                .selection
                .head
                .0
                .saturating_sub(text.len());
            selections.set_range(
                index,
                Selection {
                    anchor: ByteOffset(start + anchor_relative),
                    head: ByteOffset(start + head_relative),
                }
                .into(),
            );
        }
        let projection = crate::byte_projection::ByteProjection::from_bytes(&bytes);
        let source_selections = selections.clone();
        let mut display_ranges = selections.ranges().to_vec();
        for range in &mut display_ranges { range.selection = projection.display_selection(range.selection)?; }
        selections = SelectionSet::from_parts(display_ranges, selections.primary_index(), selections.is_rectangular())?;
        for range in &mut marked_ranges {
            *range = projection.display_selection(Selection { anchor: range.start, head: range.end })?.range();
        }
        marked_range = projection.display_selection(Selection { anchor: marked_range.start, head: marked_range.end })?.range();
        let display = projection.text();
        let selection = selections.primary().selection;
        self.composition = Some(Composition {
            base_revision: self.document.text_revision(),
            composition_revision: self.document.composition_revision(),
            base_range: base_selections.primary().selection.range(),
            marked_range,
            text: text.to_string(),
            display,
            bytes,
            selection,
            inline: Some(InlineComposition {
                base_selections,
                selections,
                source_selections,
                edits,
                full_ranges: marked_ranges.clone(),
                marked_ranges,
            }),
        });
        self.invalidate_display();
        Ok(())
    }

    pub(super) fn update_inline_range(
        &mut self,
        range: Option<Range<ByteOffset>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> Result<(), EditError> {
        let Some(composition) = self.active_composition() else {
            return self.mark_inline_composition(text, selected_utf16);
        };
        let marked = composition.marked_range.clone();
        let range = range.unwrap_or(marked.clone());
        if range.start > range.end || range.end.0 > composition.display.len() {
            return Err(EditError::InvalidRange);
        }
        if !composition.display.is_char_boundary(range.start.0)
            || !composition.display.is_char_boundary(range.end.0)
        {
            return Err(EditError::InvalidBoundary);
        }
        let inserted = Document::new(text);
        let relative = selected_utf16.unwrap_or(inserted.len_utf16()..inserted.len_utf16());
        if relative.start > relative.end {
            return Err(EditError::InvalidRange);
        }
        inserted.from_utf16(Utf16Offset(relative.start))?;
        inserted.from_utf16(Utf16Offset(relative.end))?;
        let full = composition
            .inline
            .as_ref()
            .map(|inline| inline.full_ranges[inline.selections.primary_index()].clone())
            .unwrap_or(marked);
        if range.start < full.start || range.end > full.end {
            self.commit_composition()?;
            return self.mark_display_range(Some(range), text, Some(relative));
        }
        let projection = crate::byte_projection::ByteProjection::from_bytes(&composition.bytes);
        let source_range = projection.source_selection(Selection { anchor: range.start, head: range.end })?.range();
        let source_full = projection.source_selection(Selection { anchor: full.start, head: full.end })?.range();
        let start = source_range.start.0 - source_full.start.0;
        let end = source_range.end.0 - source_full.start.0;
        if !composition.text.is_char_boundary(start) || !composition.text.is_char_boundary(end) {
            return Err(EditError::InvalidBoundary);
        }
        let mut updated = composition.text.clone();
        updated.replace_range(start..end, text);
        let prefix_utf16 = composition.text[..start].encode_utf16().count();
        self.mark_inline_composition(
            &updated,
            Some(prefix_utf16 + relative.start..prefix_utf16 + relative.end),
        )?;
        if let Some(composition) = &mut self.composition
            && let Some(inline) = &mut composition.inline
        {
            let projection = crate::byte_projection::ByteProjection::from_bytes(&composition.bytes);
            inline.marked_ranges = inline
                .full_ranges
                .iter()
                .map(|range| {
                    let full = projection.source_selection(Selection { anchor: range.start, head: range.end })?.range();
                    projection.display_selection(Selection {
                        anchor: ByteOffset(full.start.0 + start),
                        head: ByteOffset(full.start.0 + start + text.len()),
                    }).map(Selection::range)
                })
                .collect::<Result<Vec<_>, EditError>>()?;
            composition.marked_range =
                inline.marked_ranges[inline.selections.primary_index()].clone();
        }
        Ok(())
    }

    pub(super) fn replace_inline_display_range(
        &mut self,
        range: Range<ByteOffset>,
        text: &str,
    ) -> Result<(), EditError> {
        self.begin_undo_action();
        let result = self
            .update_inline_range(Some(range), text, None)
            .and_then(|()| self.commit_composition());
        self.end_undo_action();
        result
    }

    pub(super) fn commit_inline_preview(&mut self) -> Result<(), EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        if let Some(composition) = self.composition.as_ref() {
            if let Some(inline) = composition.inline.as_ref() {
                self.document.commit_prepared_bytes(
                    composition.bytes.clone(),
                    inline.source_selections.clone(),
                    inline.edits.clone(),
                )?;
            }
            self.composition = None;
            self.invalidate_display();
        }
        Ok(())
    }
}
