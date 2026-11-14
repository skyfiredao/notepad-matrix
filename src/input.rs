use std::ops::Range;
use std::sync::{Arc, Mutex};

use ropey::{LineType, Rope, RopeBuilder};

use crate::editor::{
    ByteOffset, Document, EditError, SaveSnapshot, Selection, SelectionRange, SelectionSet,
    TextTransform,
};

#[path = "ime.rs"]
mod ime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    CrLf,
    Lf,
    Cr,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
        }
    }
}

fn selection_insert(selection: &mut Selection, at: usize, length: usize) {
    let anchor_start = selection.anchor < selection.head;
    let head_start = selection.head < selection.anchor;
    for (position, move_equal) in [
        (&mut selection.anchor, anchor_start),
        (&mut selection.head, head_start),
    ] {
        if position.0 > at || (position.0 == at && move_equal) {
            position.0 += length;
        }
    }
}

fn eol_delete(selection: &mut Selection, at: usize) {
    for position in [&mut selection.anchor, &mut selection.head] {
        if position.0 > at {
            position.0 = (position.0 - 1).max(at);
        }
    }
}

/// Identity of displayed text; selection-only changes retain this identity.
#[derive(Clone, Debug)]
pub struct DisplayRevision(Arc<()>, Arc<()>, bool);

impl PartialEq for DisplayRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) && Arc::ptr_eq(&self.1, &other.1) && self.2 == other.2
    }
}
impl Eq for DisplayRevision {}

struct Composition {
    base_revision: Arc<()>,
    composition_revision: Arc<()>,
    base_range: Range<ByteOffset>,
    marked_range: Range<ByteOffset>,
    text: String,
    display: Rope,
    selection: Selection,
    inline: Option<ime::InlineComposition>,
}

pub struct EditorSession {
    document: Document,
    composition: Option<Composition>,
    display_revision: DisplayRevision,
    line_ending: Arc<Mutex<LineEnding>>,
    overwrite: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionClipboard {
    pub text: String,
    pub rectangular: bool,
    pub line_copy: bool,
}

impl EditorSession {
    pub fn new(text: &str) -> Self {
        Self {
            document: Document::new(text),
            composition: None,
            display_revision: DisplayRevision(Arc::new(()), Arc::new(()), false),
            line_ending: Arc::new(Mutex::new(LineEnding::CrLf)),
            overwrite: false,
        }
    }

    pub fn clone_view(&self) -> Self {
        Self {
            document: self.document.clone_view(),
            composition: None,
            display_revision: DisplayRevision(Arc::new(()), Arc::new(()), false),
            line_ending: self.line_ending.clone(),
            overwrite: self.overwrite,
        }
    }

    pub fn from_file_text(text: &str) -> Self {
        let mut editor = Self::new(text);
        let snapshot = editor.document.text_snapshot();
        if snapshot.len_lines(LineType::LF_CR) > 1 {
            let next_line = snapshot.line_to_byte_idx(1, LineType::LF_CR);
            if let Some(last) = next_line.checked_sub(1) {
                editor.set_line_ending(match snapshot.byte(last) {
                    b'\n' if last > 0 && snapshot.byte(last - 1) == b'\r' => LineEnding::CrLf,
                    b'\n' => LineEnding::Lf,
                    b'\r' => LineEnding::Cr,
                    _ => LineEnding::CrLf,
                });
            }
        }
        editor
    }

    pub fn line_ending(&self) -> LineEnding {
        *self
            .line_ending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_line_ending(&mut self, value: LineEnding) {
        *self
            .line_ending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
    }

    pub fn overwrite(&self) -> bool {
        self.overwrite
    }

    pub fn set_overwrite(&mut self, value: bool) {
        self.overwrite = value;
    }

    pub fn toggle_overwrite(&mut self) {
        self.overwrite = !self.overwrite;
    }

    pub fn clipboard_text(&self) -> crate::clipboard::ClipboardText {
        let text = self.display_snapshot();
        if !self.has_composition()
            || self
                .active_composition()
                .is_some_and(|composition| composition.inline.is_some())
        {
            let selections = self.selections();
            if selections.ranges().iter().any(|range| !range.is_empty()) {
                let mut ranges = selections.ranges().to_vec();
                if selections.is_rectangular() {
                    ranges.sort_by_key(|range| range.selection.range().start);
                }
                let mut copied = String::new();
                for range in &ranges {
                    let range = range.selection.range();
                    copied.push_str(&text.slice(range.start.0..range.end.0).to_string());
                    if ranges.len() > 1 {
                        copied.push_str(self.line_ending().as_str());
                    }
                }
                return crate::clipboard::ClipboardText {
                    text: copied,
                    line_copy: false,
                };
            }
        }
        let selection = self.selection();
        if !selection.range().is_empty() {
            let range = selection.range();
            return crate::clipboard::ClipboardText {
                text: text.slice(range.start.0..range.end.0).to_string(),
                line_copy: false,
            };
        }
        let row = text.byte_to_line_idx(selection.head.0, LineType::LF_CR);
        let line = text.line(row, LineType::LF_CR).to_string();
        crate::clipboard::ClipboardText {
            text: format!(
                "{}{}",
                line.trim_end_matches(['\r', '\n']),
                self.line_ending().as_str()
            ),
            line_copy: true,
        }
    }

    pub fn cut_selection(&mut self) -> Result<crate::clipboard::ClipboardText, EditError> {
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.commit_composition()?;
        let payload = self.clipboard_text();
        if self
            .selections()
            .ranges()
            .iter()
            .all(|range| range.is_empty())
        {
            self.delete_current_line()?;
        } else {
            self.document.delete_selections(false)?;
            self.invalidate_display();
        }
        Ok(payload)
    }

    pub fn clipboard_payload(&self) -> SelectionClipboard {
        let payload = self.clipboard_text();
        SelectionClipboard {
            text: payload.text,
            rectangular: self.selections().ranges().len() > 1 && !payload.line_copy,
            line_copy: payload.line_copy,
        }
    }

    pub fn paste_payload(
        &mut self,
        payload: &SelectionClipboard,
        tab_width: usize,
    ) -> Result<(), EditError> {
        if !payload.rectangular {
            return self.paste_clipboard(&crate::clipboard::ClipboardText {
                text: payload.text.clone(),
                line_copy: payload.line_copy,
            });
        }
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.commit_composition()?;
        self.document
            .paste_rectangle(&payload.text, tab_width, self.line_ending().as_str())?;
        self.invalidate_display();
        Ok(())
    }

    pub fn replace_target_from_selection(&mut self, text: &str) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let range = self.document.selection().range();
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        let mut candidate = self.document.text_snapshot();
        let mut line_edits = crate::editor::LineEdits::default();
        line_edits.remove(&mut candidate, range.start.0..range.end.0)?;
        line_edits.insert(&mut candidate, range.start.0, text)?;
        self.document.replace_rope(
            candidate,
            Selection {
                anchor: range.start,
                head: range.start,
            },
            line_edits,
        )?;
        self.invalidate_display();
        Ok(())
    }

    pub fn replace_all_literal(
        &mut self,
        query: &str,
        replacement: &str,
        options: crate::search::LiteralOptions,
    ) -> Result<usize, crate::search::ReplaceError> {
        self.replace_all_literal_in_direction(query, replacement, options, false)
    }

    pub fn replace_all_literal_in_direction(
        &mut self,
        query: &str,
        replacement: &str,
        options: crate::search::LiteralOptions,
        backwards: bool,
    ) -> Result<usize, crate::search::ReplaceError> {
        self.replace_all_literal_scoped(query, replacement, options, backwards, false)
    }

    pub fn replace_all_literal_scoped(
        &mut self,
        query: &str,
        replacement: &str,
        options: crate::search::LiteralOptions,
        backwards: bool,
        in_selection: bool,
    ) -> Result<usize, crate::search::ReplaceError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition.into());
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly.into());
        }
        let mut selection = self.document.selection();
        let text = self.committed_text();
        let scope = crate::search::literal_scope(
            &text,
            self.selection_limits(),
            options.wrap,
            backwards,
            in_selection,
        )?;
        let selected_start = scope.start;
        let selected_length = scope.len();
        let original_length = self.document.len_bytes();
        let matches = crate::search::literal_ranges_in(&text, query, scope, options)?;
        let replacement = options.mode.decode(replacement);
        let replacement = replacement.as_ref();
        if matches.is_empty() {
            return Ok(0);
        }
        let mut candidate = self.document.text_snapshot();
        let mut line_edits = crate::editor::LineEdits::default();
        let mut original_end = 0;
        let mut current_end = 0;
        for range in &matches {
            let start = current_end + (range.start.0 - original_end);
            let length = range.end.0 - range.start.0;
            let end = start + length;
            line_edits.remove(&mut candidate, start..end)?;
            line_edits.insert(&mut candidate, start, replacement)?;
            for position in [&mut selection.anchor, &mut selection.head] {
                if position.0 > start {
                    position.0 = if position.0 >= end {
                        position.0 - length
                    } else {
                        start
                    };
                }
            }
            let head_start = selection.head < selection.anchor;
            let anchor_start = selection.anchor < selection.head;
            for (position, move_equal) in [
                (&mut selection.head, head_start),
                (&mut selection.anchor, anchor_start),
            ] {
                if position.0 > start || (position.0 == start && move_equal) {
                    position.0 += replacement.len();
                }
            }
            original_end = range.end.0;
            current_end = start + replacement.len();
        }
        if in_selection {
            let end = selected_start + (selected_length + candidate.len() - original_length);
            selection = Selection {
                anchor: ByteOffset(selected_start),
                head: ByteOffset(end),
            };
        }
        self.document
            .replace_rope(candidate, selection, line_edits)?;
        self.invalidate_display();
        Ok(matches.len())
    }

    pub fn paste_clipboard(
        &mut self,
        payload: &crate::clipboard::ClipboardText,
    ) -> Result<(), EditError> {
        let mut converted =
            crate::clipboard::convert_line_endings(&payload.text, self.line_ending().as_str());
        if payload.line_copy && self.selection().range().is_empty() {
            if self.has_composition() {
                return Err(EditError::UnsupportedComposition);
            }
            if self.document.is_read_only() {
                return Err(EditError::ReadOnly);
            }
            if converted.is_empty() {
                return Ok(());
            }
            if !converted.ends_with(['\r', '\n']) {
                converted.push_str(self.line_ending().as_str());
            }
            let before = self.document.selection();
            let mut candidate = self.document.text_snapshot();
            let row = candidate.byte_to_line_idx(before.head.0, LineType::LF_CR);
            let start = candidate.line_to_byte_idx(row, LineType::LF_CR);
            let mut line_edits = crate::editor::LineEdits::default();
            line_edits.insert(&mut candidate, start, &converted)?;
            let caret = ByteOffset(before.head.0 + converted.len());
            self.document.replace_rope(
                candidate,
                Selection {
                    anchor: caret,
                    head: caret,
                },
                line_edits,
            )?;
            self.invalidate_display();
            return Ok(());
        }
        self.insert_text(&converted)
    }

    fn delete_selection_then_add(&mut self, text: &str) -> Result<(), EditError> {
        let range = self.document.selection().range();
        if !range.is_empty() {
            self.replace_display_range(range, "")?;
        }
        self.insert_text(text)
    }

    /// Converter issues the same two edit messages but selects the entire result,
    /// preserves embedded NUL, and may validly replace selected text with nothing.
    pub fn replace_converter_selection(&mut self, text: &str) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let range = self.document.selection().range();
        if range.is_empty() {
            return Ok(());
        }
        let previous_revision = self.document.text_revision();
        self.delete_selection_then_add(text)?;
        self.document.select_after_edit(
            Selection {
                anchor: range.start,
                head: ByteOffset(range.start.0 + text.len()),
            },
            &previous_revision,
        )
    }

    pub(crate) fn from_recovery(
        text: &str,
        selection: Selection,
        line_ending: LineEnding,
    ) -> Result<Self, EditError> {
        let mut session = Self::new("");
        session.document = Document::from_recovery(text);
        session.document.select(selection)?;
        session.set_line_ending(line_ending);
        Ok(session)
    }

    /// Delete only the committed main-caret line, preserving source deletion affinity.
    pub fn delete_current_line(&mut self) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        let text = self.document.text_snapshot();
        let selection = self.document.selection();
        let row = text.byte_to_line_idx(selection.head.0, LineType::LF_CR);
        let start = text.line_to_byte_idx(row, LineType::LF_CR);
        let end = if row + 1 < text.len_lines(LineType::LF_CR) {
            text.line_to_byte_idx(row + 1, LineType::LF_CR)
        } else {
            text.len()
        };
        if start == end {
            return Ok(());
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let rebase = |position: ByteOffset| {
            ByteOffset(if position.0 <= start {
                position.0
            } else if position.0 >= end {
                position.0 - (end - start)
            } else {
                start
            })
        };
        let mapped = Selection {
            anchor: rebase(selection.anchor),
            head: rebase(selection.head),
        };
        let mut candidate = text;
        let mut line_edits = crate::editor::LineEdits::default();
        line_edits.remove(&mut candidate, start..end)?;
        self.document.replace_rope(candidate, mapped, line_edits)?;
        self.invalidate_display();
        Ok(())
    }

    /// SCI_LINEDUPLICATE uses the main caret line regardless of selection;
    /// SCI_SELECTIONDUPLICATE uses the selection unless it is empty.
    pub fn duplicate(&mut self, for_line: bool) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let mut selection = self.document.selection();
        let for_line = for_line || selection.range().is_empty();
        let mut candidate = self.document.text_snapshot();
        let range = if for_line {
            let row = candidate.byte_to_line_idx(selection.head.0, LineType::LF_CR);
            let start = candidate.line_to_byte_idx(row, LineType::LF_CR);
            let length = candidate
                .line(row, LineType::LF_CR)
                .to_string()
                .trim_end_matches(['\r', '\n'])
                .len();
            start..start + length
        } else {
            selection.range().start.0..selection.range().end.0
        };
        let text = candidate.slice(range.clone()).to_string();
        let mut line_edits = crate::editor::LineEdits::default();
        let eol = if for_line {
            self.line_ending().as_str()
        } else {
            ""
        };
        line_edits.insert(&mut candidate, range.end, eol)?;
        selection_insert(&mut selection, range.end, eol.len());
        line_edits.insert(&mut candidate, range.end + eol.len(), &text)?;
        selection_insert(&mut selection, range.end + eol.len(), text.len());
        self.document
            .replace_rope(candidate, selection, line_edits)?;
        self.invalidate_display();
        Ok(())
    }

    /// Normalize committed CR/LF terminators in one undoable transaction. The
    /// separate Enter mode intentionally persists when text conversion is undone.
    pub fn convert_line_endings(&mut self, target: LineEnding) -> Result<(), EditError> {
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let text = self.document.text_snapshot();
        let mut changed = false;
        if self.document.line_count() > 1 {
            let mut builder = RopeBuilder::new();
            let mut chars = text.chars().peekable();
            let mut selection = self.document.selection();
            let mut line_edits = crate::editor::LineEdits::default();
            let mut position = 0;
            while let Some(ch) = chars.next() {
                if ch == '\r' || ch == '\n' {
                    let pair = ch == '\r' && chars.peek() == Some(&'\n');
                    if pair {
                        chars.next();
                    }
                    // Rebase in the same evolving component-edit order as the
                    // reference, including insert affinity for nonempty ranges.
                    match (ch, pair, target) {
                        ('\r', true, LineEnding::Cr) => {
                            line_edits.record_text_edit(position + 1, 1, "");
                            eol_delete(&mut selection, position + 1);
                            changed = true;
                        }
                        ('\r', true, LineEnding::Lf) => {
                            line_edits.record_text_edit(position, 1, "");
                            eol_delete(&mut selection, position);
                            changed = true;
                        }
                        ('\r', false, LineEnding::CrLf) => {
                            line_edits.record_text_edit(position + 1, 0, "\n");
                            selection_insert(&mut selection, position + 1, 1);
                            changed = true;
                        }
                        ('\r', false, LineEnding::Lf) => {
                            line_edits.record_text_edit(position, 0, "\n");
                            line_edits.record_text_edit(position + 1, 1, "");
                            selection_insert(&mut selection, position, 1);
                            eol_delete(&mut selection, position + 1);
                            changed = true;
                        }
                        ('\n', _, LineEnding::CrLf) => {
                            line_edits.record_text_edit(position, 0, "\r");
                            selection_insert(&mut selection, position, 1);
                            changed = true;
                        }
                        ('\n', _, LineEnding::Cr) => {
                            line_edits.record_text_edit(position, 0, "\r");
                            line_edits.record_text_edit(position + 1, 1, "");
                            selection_insert(&mut selection, position, 1);
                            eol_delete(&mut selection, position + 1);
                            changed = true;
                        }
                        _ => {}
                    }
                    builder.append(target.as_str());
                    position += target.as_str().len();
                } else {
                    let mut encoded = [0; 4];
                    builder.append(ch.encode_utf8(&mut encoded));
                    position += ch.len_utf8();
                }
            }
            if changed {
                self.document
                    .replace_rope(builder.finish(), selection, line_edits)?;
            }
        }
        self.set_line_ending(target);
        let preview = self.composition.take().is_some();
        if changed || preview {
            self.invalidate_display();
        }
        Ok(())
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn word_selection_at(&self, position: ByteOffset) -> Result<Selection, EditError> {
        crate::editor::word_selection_in_snapshot(&self.display_snapshot(), position)
    }

    pub(crate) fn word_range_at(&self, position: ByteOffset) -> Result<Selection, EditError> {
        crate::editor::word_range_in_snapshot(&self.display_snapshot(), position)
    }

    pub(crate) fn selection_limits(&self) -> Selection {
        let selections = self.display_selections();
        let mut range = selections.primary().selection.range();
        if selections.is_rectangular() {
            for selected in selections.ranges() {
                let selected = selected.selection.range();
                range.start = range.start.min(selected.start);
                range.end = range.end.max(selected.end);
            }
        }
        Selection {
            anchor: range.start,
            head: range.end,
        }
    }

    pub(crate) fn expand_word_selection(&mut self) -> Result<bool, EditError> {
        let word = self.word_range_at(self.selection().head)?;
        if word.range().is_empty() {
            return Ok(false);
        }
        let selections = self.display_selections();
        let mut ranges = selections.ranges().to_vec();
        ranges[selections.primary_index()] = word.into();
        self.set_selections(SelectionSet::from_parts(
            ranges,
            selections.primary_index(),
            selections.is_rectangular(),
        )?)?;
        Ok(true)
    }

    pub fn extend_word_selection(
        &self,
        anchored: Selection,
        initial: ByteOffset,
        position: ByteOffset,
    ) -> Result<Selection, EditError> {
        crate::editor::extend_word_selection_in_snapshot(
            &self.display_snapshot(),
            anchored,
            initial,
            position,
        )
    }

    pub fn is_read_only(&self) -> bool {
        self.document.is_read_only()
    }

    pub fn selections(&self) -> SelectionSet {
        if let Some(composition) = self.active_composition() {
            if let Some(inline) = &composition.inline {
                return inline.selections.clone();
            }
            return SelectionSet::single(composition.selection);
        }
        self.document.selections()
    }

    pub fn display_selections(&self) -> SelectionSet {
        self.selections()
    }

    pub fn adjacent_position(
        &self,
        position: ByteOffset,
        forward: bool,
    ) -> Result<ByteOffset, EditError> {
        crate::editor::adjacent_in_snapshot(&self.display_snapshot(), position, forward)
    }

    pub fn word_boundary(
        &self,
        position: ByteOffset,
        forward: bool,
        boundary: crate::editor::WordBoundary,
    ) -> Result<ByteOffset, EditError> {
        crate::editor::boundary_in_snapshot(&self.display_snapshot(), position, forward, boundary)
    }

    pub fn move_word(&mut self, forward: bool, extend: bool) -> Result<(), EditError> {
        self.move_word_boundary(
            forward,
            extend,
            if forward && extend {
                crate::editor::WordBoundary::End
            } else {
                crate::editor::WordBoundary::Start
            },
        )
    }

    pub fn move_word_boundary(
        &mut self,
        forward: bool,
        extend: bool,
        boundary: crate::editor::WordBoundary,
    ) -> Result<(), EditError> {
        self.discard_stale_composition();
        if self.has_composition() {
            let moved = crate::editor::move_in_snapshot(
                &self.display_snapshot(),
                &self.selections(),
                forward,
                extend,
                boundary,
            )?;
            if let Some(composition) = &mut self.composition {
                composition.selection = moved.primary().selection;
                if let Some(inline) = &mut composition.inline {
                    inline.selections = moved;
                }
            }
            self.document.break_undo_sequence();
            Ok(())
        } else {
            self.document.move_word_boundary(forward, extend, boundary)
        }
    }

    pub fn delete_word(&mut self, forward: bool, to_line_edge: bool) -> Result<(), EditError> {
        self.delete_word_command(forward, to_line_edge, false)
    }

    pub fn delete_word_end(&mut self) -> Result<(), EditError> {
        self.delete_word_command(true, false, true)
    }

    fn delete_word_command(
        &mut self,
        forward: bool,
        to_line_edge: bool,
        end: bool,
    ) -> Result<(), EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let previous = self.document.text_revision();
        let composing = self.has_composition();
        if composing {
            self.begin_undo_action();
        }
        let result = self.commit_composition().and_then(|()| {
            if end {
                self.document.delete_word_end()
            } else {
                self.document.delete_word(forward, to_line_edge)
            }
        });
        if composing {
            self.end_undo_action();
        }
        if result.is_ok() && !Arc::ptr_eq(&previous, &self.document.text_revision()) {
            self.invalidate_display();
        }
        result
    }

    pub fn set_selections(&mut self, selections: SelectionSet) -> Result<(), EditError> {
        self.document.set_selections(selections)?;
        self.cancel_composition();
        Ok(())
    }

    pub fn add_selection(&mut self, range: SelectionRange) -> Result<(), EditError> {
        self.document.add_selection(range)?;
        self.cancel_composition();
        Ok(())
    }

    pub fn tentative_selection(&mut self, range: SelectionRange) -> Result<(), EditError> {
        self.document.tentative_selection(range)?;
        self.cancel_composition();
        Ok(())
    }

    pub fn commit_tentative_selection(&mut self) {
        self.document.commit_tentative_selection();
    }

    pub fn rotate_selection(&mut self) {
        self.document.rotate_selection();
    }
    pub fn swap_main_anchor_caret(&mut self) {
        self.document.swap_main_anchor_caret();
    }
    pub fn remove_selection(&mut self, index: usize) {
        self.document.remove_selection(index);
    }
    pub fn clear_additional_selections(&mut self) {
        self.document.clear_additional_selections();
    }
    pub fn remove_last_selection(&mut self) {
        self.document.remove_last_selection();
    }

    pub fn select_next_occurrence(
        &mut self,
        options: crate::editor::OccurrenceOptions,
    ) -> Result<bool, EditError> {
        self.commit_composition()?;
        self.document.select_next_occurrence(options)
    }

    pub fn select_all_occurrences(
        &mut self,
        options: crate::editor::OccurrenceOptions,
    ) -> Result<bool, EditError> {
        self.commit_composition()?;
        self.document.select_all_occurrences(options)
    }

    pub fn skip_occurrence(&mut self) -> Result<bool, EditError> {
        self.commit_composition()?;
        self.document.skip_occurrence()
    }

    pub fn context_select(&mut self, position: ByteOffset) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.context_select(position)
    }

    pub fn drop_selection(&mut self, position: ByteOffset, copy: bool) -> Result<bool, EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.commit_composition()?;
        let changed = self.document.drop_selection_with_options(
            position,
            copy,
            4,
            self.line_ending().as_str(),
        )?;
        if changed {
            self.invalidate_display();
        }
        Ok(changed)
    }

    pub fn apply_transforms(&mut self, transforms: &[TextTransform]) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.apply_transforms(transforms)?;
        self.invalidate_display();
        Ok(())
    }

    pub fn apply_multi(
        &mut self,
        transforms: &[TextTransform],
        policy: crate::editor::EditSelectionPolicy,
    ) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.apply_multi(transforms, policy)?;
        self.invalidate_display();
        Ok(())
    }

    pub fn select_rectangle(
        &mut self,
        anchor: crate::editor::ColumnPosition,
        head: crate::editor::ColumnPosition,
        tab_width: usize,
    ) -> Result<(), EditError> {
        self.document.select_rectangle(anchor, head, tab_width)?;
        self.cancel_composition();
        Ok(())
    }

    pub fn column_text(&mut self, text: &str, tab_width: usize) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.column_text(text, tab_width)?;
        self.invalidate_display();
        Ok(())
    }

    pub fn column_numbers(
        &mut self,
        options: crate::editor::ColumnNumbers,
        tab_width: usize,
    ) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.column_numbers(options, tab_width)?;
        self.invalidate_display();
        Ok(())
    }

    pub fn delete_forward(&mut self) -> Result<(), EditError> {
        self.delete_character(false)
    }

    pub fn backspace(&mut self) -> Result<(), EditError> {
        self.delete_character(true)
    }

    pub fn backspace_not_line(&mut self) -> Result<(), EditError> {
        if self.is_read_only() {
            return self.document.backspace_not_line();
        }
        let previous = self.document.text_revision();
        let composing = self.has_composition();
        if composing {
            self.begin_undo_action();
        }
        let result = self
            .commit_composition()
            .and_then(|()| self.document.backspace_not_line());
        if composing {
            self.end_undo_action();
        }
        if result.is_ok() && !Arc::ptr_eq(&previous, &self.document.text_revision()) {
            self.invalidate_display();
        }
        result
    }

    fn delete_character(&mut self, backward: bool) -> Result<(), EditError> {
        if self.has_composition() {
            let mut range = self.selection().range();
            if range.is_empty() {
                let adjacent = self.adjacent_position(range.start, !backward)?;
                if backward {
                    range.start = adjacent;
                } else {
                    range.end = adjacent;
                }
            }
            return self.replace_display_range(range, "");
        }
        let previous = self.document.text_revision();
        self.document.delete_selections(backward)?;
        if !Arc::ptr_eq(&previous, &self.document.text_revision()) {
            self.invalidate_display();
        }
        Ok(())
    }

    pub fn begin_undo_action(&mut self) {
        self.document.begin_undo_action();
    }

    pub fn end_undo_action(&mut self) {
        self.document.end_undo_action();
    }

    pub fn toggle_bookmark(&mut self, line: Option<usize>) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        if let Some(line) = line {
            if !self.document.toggle_bookmark_at(line) {
                return Err(EditError::InvalidRange);
            }
        } else {
            self.document.toggle_bookmark();
        }
        Ok(())
    }

    pub fn clear_bookmarks(&mut self) {
        self.document.clear_bookmarks();
    }

    pub fn goto_bookmark(&mut self, forward: bool) -> Result<bool, EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        self.document.goto_bookmark(forward)
    }

    pub fn goto_change(&mut self, forward: bool) -> Result<bool, EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        self.document.goto_change(forward)
    }

    pub fn clear_change_history(&mut self) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        self.document.clear_change_history()
    }

    pub fn save_snapshot(&self) -> SaveSnapshot {
        self.document.save_snapshot()
    }

    pub fn acknowledge_save(&mut self, snapshot: &SaveSnapshot) -> Result<(), EditError> {
        self.document.acknowledge_save(snapshot)
    }

    pub fn set_read_only(&mut self, value: bool) {
        if value {
            self.cancel_composition();
        }
        self.document.set_read_only(value);
    }

    pub fn display_revision(&self) -> DisplayRevision {
        DisplayRevision(
            self.display_revision.0.clone(),
            self.document.text_revision(),
            self.has_composition(),
        )
    }

    fn invalidate_display(&mut self) {
        self.display_revision = DisplayRevision(
            Arc::new(()),
            self.document.text_revision(),
            self.has_composition(),
        );
    }

    pub fn committed_text(&self) -> String {
        self.document.text()
    }

    pub fn display_snapshot(&self) -> Rope {
        match self.active_composition() {
            Some(composition) => composition.display.clone(),
            None => self.document.text_snapshot(),
        }
    }

    pub fn display_text(&self) -> String {
        self.display_snapshot().to_string()
    }

    pub fn selection(&self) -> Selection {
        match self.active_composition() {
            Some(composition) => composition.selection,
            None => self.document.selection(),
        }
    }

    pub fn marked_range(&self) -> Option<Range<ByteOffset>> {
        self.active_composition()
            .filter(|composition| !composition.marked_range.is_empty())
            .map(|composition| composition.marked_range.clone())
    }

    pub fn marked_ranges(&self) -> Vec<Range<ByteOffset>> {
        match self.active_composition() {
            Some(composition) => match &composition.inline {
                Some(inline) => inline
                    .marked_ranges
                    .iter()
                    .filter(|range| !range.is_empty())
                    .cloned()
                    .collect(),
                None => self.marked_range().into_iter().collect(),
            },
            None => Vec::new(),
        }
    }

    pub fn has_composition(&self) -> bool {
        self.active_composition().is_some()
    }

    fn active_composition(&self) -> Option<&Composition> {
        self.composition.as_ref().filter(|composition| {
            !self.document.is_read_only()
                && Arc::ptr_eq(&composition.base_revision, &self.document.text_revision())
                && Arc::ptr_eq(
                    &composition.composition_revision,
                    &self.document.composition_revision(),
                )
        })
    }

    fn discard_stale_composition(&mut self) {
        if !self.has_composition() {
            self.composition = None;
        }
    }

    pub fn is_modified(&self) -> bool {
        self.document.is_modified()
    }

    pub(crate) fn clear_history(&mut self) {
        self.document.clear_history();
    }

    pub fn select(&mut self, selection: Selection) -> Result<(), EditError> {
        self.document.select(selection)?;
        self.cancel_composition();
        Ok(())
    }

    pub fn select_display(&mut self, selection: Selection) -> Result<(), EditError> {
        self.discard_stale_composition();
        let display = self.display_snapshot();
        let range = selection.range();
        if range.end.0 > display.len() {
            return Err(EditError::InvalidRange);
        }
        if !display.is_char_boundary(range.start.0) || !display.is_char_boundary(range.end.0) {
            return Err(EditError::InvalidBoundary);
        }
        if let Some(composition) = &mut self.composition {
            composition.selection = selection;
            if let Some(inline) = &mut composition.inline {
                inline
                    .selections
                    .set_range(inline.selections.primary_index(), selection.into());
            }
            Ok(())
        } else {
            self.document.select(selection)
        }
    }

    pub fn move_to_document_edge(&mut self, end: bool, extend: bool) -> Result<(), EditError> {
        let head = ByteOffset(if end {
            self.display_snapshot().len()
        } else {
            0
        });
        self.select_display(Selection {
            anchor: if extend {
                self.selection().anchor
            } else {
                head
            },
            head,
        })
    }

    /// Default Home/End behavior without wrapping: smart indentation Home and EOL-excluding End.
    pub fn move_to_line_edge(&mut self, end: bool, extend: bool) -> Result<(), EditError> {
        let text = self.display_snapshot();
        let selected = self.selection();
        let row = text.byte_to_line_idx(selected.head.0, LineType::LF_CR);
        let start = text.line_to_byte_idx(row, LineType::LF_CR);
        let mut line_end = if row + 1 < text.len_lines(LineType::LF_CR) {
            text.line_to_byte_idx(row + 1, LineType::LF_CR)
        } else {
            text.len()
        };
        while line_end > start && matches!(text.byte(line_end - 1), b'\r' | b'\n') {
            line_end -= 1;
        }
        let head = if end {
            line_end
        } else {
            let mut indentation = start;
            while indentation < line_end && matches!(text.byte(indentation), b' ' | b'\t') {
                indentation += 1;
            }
            if selected.head.0 == indentation {
                start
            } else {
                indentation
            }
        };
        let head = ByteOffset(head);
        self.select_display(Selection {
            anchor: if extend { selected.anchor } else { head },
            head,
        })
    }

    pub fn set_composition(&mut self, text: &str) -> Result<(), EditError> {
        if self
            .active_composition()
            .is_some_and(|composition| composition.inline.is_some())
        {
            return self.mark_inline_composition(text, None);
        }
        self.mark_display_range(None, text, None)
    }

    fn display_edit(
        &self,
        range: Range<ByteOffset>,
        text: &str,
    ) -> Result<(Range<ByteOffset>, String), EditError> {
        let display = self.display_snapshot();
        if range.start > range.end || range.end.0 > display.len() {
            return Err(EditError::InvalidRange);
        }
        if !display.is_char_boundary(range.start.0) || !display.is_char_boundary(range.end.0) {
            return Err(EditError::InvalidBoundary);
        }
        if let Some(composition) = self.active_composition() {
            let marked_start = composition.base_range.start.0;
            let marked_end = marked_start + composition.text.len();
            let start = range.start.0.min(marked_start);
            let end = range.end.0.max(marked_end);
            let mut replacement = display
                .try_slice(start..range.start.0)
                .map_err(|error| EditError::Storage(error.to_string()))?
                .to_string();
            replacement.push_str(text);
            replacement.push_str(
                &display
                    .try_slice(range.end.0..end)
                    .map_err(|error| EditError::Storage(error.to_string()))?
                    .to_string(),
            );
            Ok((
                ByteOffset(start)..ByteOffset(composition.base_range.end.0 + end - marked_end),
                replacement,
            ))
        } else {
            Ok((range, text.to_string()))
        }
    }

    pub fn mark_display_range(
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
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let range = range
            .or_else(|| self.marked_range())
            .unwrap_or_else(|| self.selection().range());
        let inserted = Document::new(text);
        let relative = selected_utf16.unwrap_or(inserted.len_utf16()..inserted.len_utf16());
        if relative.start > relative.end {
            return Err(EditError::InvalidRange);
        }
        let anchor = ByteOffset(
            range.start.0
                + inserted
                    .from_utf16(crate::editor::Utf16Offset(relative.start))?
                    .0,
        );
        let head = ByteOffset(
            range.start.0
                + inserted
                    .from_utf16(crate::editor::Utf16Offset(relative.end))?
                    .0,
        );
        let marked_range = range.start..ByteOffset(range.start.0 + text.len());
        let (base_range, replacement) = self.display_edit(range, text)?;
        let mut display = self.document.text_snapshot();
        display
            .try_remove(base_range.start.0..base_range.end.0)
            .map_err(|error| EditError::Storage(error.to_string()))?;
        display
            .try_insert(base_range.start.0, &replacement)
            .map_err(|error| EditError::Storage(error.to_string()))?;
        self.composition = Some(Composition {
            base_revision: self.document.text_revision(),
            composition_revision: self.document.composition_revision(),
            base_range,
            marked_range,
            text: replacement,
            display,
            selection: Selection { anchor, head },
            inline: None,
        });
        self.invalidate_display();
        Ok(())
    }

    pub fn cancel_composition(&mut self) {
        if self.composition.take().is_some() {
            self.invalidate_display();
        }
    }

    pub fn commit_composition(&mut self) -> Result<(), EditError> {
        self.discard_stale_composition();
        if self
            .active_composition()
            .is_some_and(|composition| composition.inline.is_some())
        {
            return self.commit_inline_preview();
        }
        if let Some(composition) = &self.composition {
            let previous_revision = self.document.text_revision();
            self.document
                .replace(composition.base_range.clone(), &composition.text)?;
            self.document
                .select_after_edit(composition.selection, &previous_revision)?;
            self.composition = None;
            self.invalidate_display();
        }
        Ok(())
    }

    pub fn type_text(&mut self, text: &str) -> Result<(), EditError> {
        if self.has_composition() {
            return self.insert_text(text);
        }
        if text.is_empty() {
            return Ok(());
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.type_characters(text)
    }

    fn type_characters(&mut self, text: &str) -> Result<(), EditError> {
        for ch in text.chars() {
            let selections = self.document.selections();
            let grouped = selections.ranges().len() > 1
                || selections.ranges().iter().any(|range| !range.is_empty())
                || self.overwrite;
            if grouped {
                self.begin_undo_action();
            }
            let result = self.document.type_character(ch, self.overwrite);
            if grouped {
                self.end_undo_action();
            }
            result?;
        }
        self.invalidate_display();
        Ok(())
    }

    pub fn insert_text(&mut self, text: &str) -> Result<(), EditError> {
        if self
            .active_composition()
            .is_some_and(|composition| composition.inline.is_some())
        {
            self.update_inline_range(None, text, None)?;
            return self.commit_inline_preview();
        }
        if !self.has_composition() {
            self.document.replace_selections(text)?;
            if !text.is_empty() {
                self.invalidate_display();
            }
            return Ok(());
        }
        let range = self
            .marked_range()
            .unwrap_or_else(|| self.selection().range());
        self.replace_display_range(range, text)
    }

    pub fn replace_display_range(
        &mut self,
        range: Range<ByteOffset>,
        text: &str,
    ) -> Result<(), EditError> {
        if self
            .active_composition()
            .is_some_and(|composition| composition.inline.is_some())
        {
            return self.replace_inline_display_range(range, text);
        }
        let unchanged = !self.has_composition() && range.is_empty() && text.is_empty();
        let caret = ByteOffset(range.start.0 + text.len());
        let (mapped, replacement) = self.display_edit(range, text)?;
        let previous_revision = self.document.text_revision();
        self.document.replace(mapped, &replacement)?;
        self.document.select_after_edit(
            Selection {
                anchor: caret,
                head: caret,
            },
            &previous_revision,
        )?;
        self.composition = None;
        if !unchanged {
            self.invalidate_display();
        }
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        self.discard_stale_composition();
        let changed = if self.composition.take().is_some() {
            true
        } else {
            self.document.undo()
        };
        if changed {
            self.invalidate_display();
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        self.cancel_composition();
        let changed = self.document.redo();
        if changed {
            self.invalidate_display();
        }
        changed
    }
}
