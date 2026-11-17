use std::ops::Range;
use std::sync::{Arc, Mutex};

use ropey::{LineType, Rope};

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
    bytes: crate::byte_buffer::ByteBuffer,
    selection: Selection,
    inline: Option<ime::InlineComposition>,
}

pub struct EditorSession {
    document: Document,
    composition: Option<Composition>,
    display_revision: DisplayRevision,
    line_ending: Arc<Mutex<LineEnding>>,
    overwrite: bool,
    tabs: TabPreferences,
}

#[derive(Clone, Copy, Debug)]
pub struct TabPreferences {
    pub size: usize,
    pub spaces: bool,
    pub backspace_unindent: bool,
}

impl Default for TabPreferences {
    fn default() -> Self { Self { size: 4, spaces: false, backspace_unindent: false } }
}

static CUSTOM_ASCII_WORD_CHARACTERS: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

pub fn configure_word_characters(characters: &str) {
    let mut ascii = [0u64; 2];
    for byte in characters.bytes().take_while(|byte| *byte != 0).filter(u8::is_ascii) {
        let value = usize::from(byte);
        ascii[value / 64] |= 1u64 << (value % 64);
    }
    for (target, value) in CUSTOM_ASCII_WORD_CHARACTERS.iter().zip(ascii) { target.store(value, std::sync::atomic::Ordering::Release); }
}

pub fn custom_word_character(character: char) -> bool {
    if character.is_ascii() {
        let value = u32::from(character) as usize;
        return CUSTOM_ASCII_WORD_CHARACTERS[value / 64].load(std::sync::atomic::Ordering::Acquire) & (1u64 << (value % 64)) != 0;
    }
    false
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionClipboard {
    pub text: String,
    pub rectangular: bool,
    pub line_copy: bool,
}

pub struct ReplaceAllOutcome {
    pub result: Result<usize, crate::search::ReplaceError>,
    pub last_target: Option<Range<i128>>,
}

impl EditorSession {
    pub fn new(text: &str) -> Self {
        Self::with_document(Document::new(text))
    }

    fn with_document(document: Document) -> Self {
        Self {
            document,
            composition: None,
            display_revision: DisplayRevision(Arc::new(()), Arc::new(()), false),
            line_ending: Arc::new(Mutex::new(LineEnding::CrLf)),
            overwrite: false,
            tabs: TabPreferences::default(),
        }
    }

    pub fn clone_view(&self) -> Self {
        Self {
            document: self.document.clone_view(),
            composition: None,
            display_revision: DisplayRevision(Arc::new(()), Arc::new(()), false),
            line_ending: self.line_ending.clone(),
            overwrite: self.overwrite,
            tabs: self.tabs,
        }
    }

    pub fn from_file_text(text: &str) -> Self {
        Self::from_file_bytes(crate::byte_buffer::ByteBuffer::from_bytes(text.as_bytes()))
    }

    pub(crate) fn from_file_bytes(bytes: crate::byte_buffer::ByteBuffer) -> Self {
        let mut editor = Self::with_document(Document::from_bytes(bytes));
        let snapshot = editor.document.byte_snapshot();
        let first_eol = snapshot.chunks().flat_map(|chunk| chunk.iter().copied()).enumerate()
            .find(|(_, byte)| matches!(byte, b'\r' | b'\n'));
        if let Some((position, byte)) = first_eol {
            editor.set_line_ending(match byte {
                b'\r' if snapshot.byte(position + 1) == Some(b'\n') => LineEnding::CrLf,
                b'\r' => LineEnding::Cr,
                _ => LineEnding::Lf,
            });
        }
        editor
    }

    pub fn line_ending(&self) -> LineEnding {
        *self
            .line_ending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_line_ending(&mut self, value: LineEnding) {
        *self
            .line_ending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
    }

    pub fn overwrite(&self) -> bool {
        self.overwrite
    }

    pub fn tab_preferences(&self) -> TabPreferences { self.tabs }

    pub fn set_tab_preferences(&mut self, preferences: TabPreferences) {
        self.tabs = TabPreferences { size: preferences.size.max(1), ..preferences };
    }

    pub fn insert_tab(&mut self) -> Result<(), EditError> {
        if !self.tabs.spaces { return self.type_characters("\t"); }
        self.commit_composition()?;
        let original = self.selections();
        let bytes = self.document.byte_snapshot();
        let mut edits = Vec::new();
        for range in original.ranges() {
            let selected = range.selection.range();
            let column = bytes.column(selected.start.0, self.tabs.size).map_err(|error| EditError::Storage(error.to_string()))?;
            let count = self.tabs.size - column % self.tabs.size;
            edits.push(TextTransform { range: selected, text: " ".repeat(count) });
        }
        edits.sort_by_key(|edit| edit.range.start);
        let ranges = original.ranges().iter().map(|range| {
            let selected = range.selection.range();
            let mut position = selected.start.0;
            for edit in &edits {
                if edit.range.start < selected.start {
                    position = position.saturating_sub(edit.range.end.0 - edit.range.start.0) + edit.text.len();
                } else if edit.range.start == selected.start {
                    position += edit.text.len();
                    break;
                }
            }
            Selection { anchor: ByteOffset(position), head: ByteOffset(position) }.into()
        }).collect();
        let selections = SelectionSet::from_parts(ranges, original.primary_index(), original.is_rectangular())?;
        self.apply_multi(&edits, crate::editor::EditSelectionPolicy::Explicit(selections))
    }

    pub fn set_overwrite(&mut self, value: bool) {
        self.overwrite = value;
    }

    pub fn toggle_overwrite(&mut self) {
        self.overwrite = !self.overwrite;
    }

    pub fn clipboard_text(&self) -> Result<crate::clipboard::ClipboardText, EditError> {
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
                let bytes = self.display_byte_snapshot();
                let projection = crate::byte_projection::ByteProjection::from_bytes(&bytes);
                for range in &ranges {
                    let range = if self.has_composition() { projection.source_selection(range.selection)?.range() }
                        else { range.selection.range() };
                    let selected = bytes.slice(range.start.0..range.end.0)
                        .map_err(|error| EditError::Storage(error.to_string()))?;
                    copied.push_str(&String::from_utf8_lossy(&selected.to_vec()));
                    if ranges.len() > 1 {
                        copied.push_str(self.line_ending().as_str());
                    }
                }
                return Ok(crate::clipboard::ClipboardText {
                    text: copied,
                    line_copy: false,
                });
            }
        }
        let selection = self.selection();
        if !self.has_composition() {
            let bytes = self.document.byte_snapshot();
            let row = bytes.line_index(selection.head.0).map_err(|error| EditError::Storage(error.to_string()))?;
            let start = bytes.line_start(row).map_err(|error| EditError::Storage(error.to_string()))?;
            let end = if row + 1 < bytes.line_count() {
                bytes.line_start(row + 1).map_err(|error| EditError::Storage(error.to_string()))?
            } else { bytes.len() };
            let line = bytes.slice(start..end).map_err(|error| EditError::Storage(error.to_string()))?.to_vec();
            let line = String::from_utf8_lossy(&line);
            return Ok(crate::clipboard::ClipboardText {
                text: format!("{}{}", line.trim_end_matches(['\r', '\n']), self.line_ending().as_str()),
                line_copy: true,
            });
        }
        if !selection.range().is_empty() {
            let range = selection.range();
            return Ok(crate::clipboard::ClipboardText {
                text: text.slice(range.start.0..range.end.0).to_string(),
                line_copy: false,
            });
        }
        let row = text.byte_to_line_idx(selection.head.0, LineType::LF_CR);
        let line = text.line(row, LineType::LF_CR).to_string();
        Ok(crate::clipboard::ClipboardText {
            text: format!(
                "{}{}",
                line.trim_end_matches(['\r', '\n']),
                self.line_ending().as_str()
            ),
            line_copy: true,
        })
    }

    pub fn cut_selection(&mut self) -> Result<crate::clipboard::ClipboardText, EditError> {
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.commit_composition()?;
        let payload = self.clipboard_text()?;
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

    pub fn clipboard_payload(&self) -> Result<SelectionClipboard, EditError> {
        let payload = self.clipboard_text()?;
        Ok(SelectionClipboard {
            text: payload.text,
            rectangular: self.selections().ranges().len() > 1 && !payload.line_copy,
            line_copy: payload.line_copy,
        })
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
        let selections = self.document.selections();
        if !selections.is_rectangular() && selections.ranges().len() > 1 {
            self.document.paste_to_multi_selection(&payload.text, self.line_ending().as_str())?;
            self.invalidate_display();
            return Ok(());
        }
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
        let mut candidate = self.document.byte_snapshot();
        let mut line_edits = crate::editor::LineEdits::default();
        line_edits.change_bytes(&mut candidate, range.start.0..range.end.0, text.as_bytes())?;
        self.document.commit_prepared_bytes(
            candidate,
            SelectionSet::single(Selection {
                anchor: range.start,
                head: range.start,
            }),
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
        self.replace_all_literal_scoped_with_target(query, replacement, options, backwards, in_selection).result
    }

    pub fn replace_all_literal_scoped_with_target(
        &mut self,
        query: &str,
        replacement: &str,
        options: crate::search::LiteralOptions,
        backwards: bool,
        in_selection: bool,
    ) -> ReplaceAllOutcome {
        let mut last_target = None;
        let result = self.replace_all_literal_scoped_pipeline(query, replacement, options, backwards, in_selection, &mut last_target);
        ReplaceAllOutcome { result, last_target }
    }

    fn replace_all_literal_scoped_pipeline(
        &mut self,
        query: &str,
        replacement: &str,
        options: crate::search::LiteralOptions,
        backwards: bool,
        in_selection: bool,
        last_target: &mut Option<Range<i128>>,
    ) -> Result<usize, crate::search::ReplaceError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition.into());
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly.into());
        }
        if !matches!(options.mode, crate::search::SearchMode::Regex { .. }) {
            return self.replace_all_plain(query, replacement, options, backwards, in_selection, last_target);
        }
        if query.split('\0').next().unwrap_or_default().is_empty() { return Ok(0); }
        let mut selections = self.document.selections();
        let mut candidate = self.document.byte_snapshot();
        let scope = crate::search::literal_scope_bytes(
            &candidate,
            self.source_selection_limits()?,
            options.wrap,
            backwards,
            in_selection,
        )?;
        if scope.is_empty() { return Ok(0); }
        let selected_start = scope.start;
        let mut start = scope.start;
        let mut end = scope.end;
        let mut regex_state = self.document.regex_state();
        regex_state.direction(scope.start as i128, scope.end as i128);
        *last_target = Some(scope.start as i128..scope.end as i128);
        let matcher = match crate::search::regex_matcher(query, options) {
            Ok(matcher) => matcher,
            Err(error) => {
                self.document.set_regex_state(regex_state);
                return Err(error.into());
            }
        };
        let mut line_edits = crate::editor::LineEdits::default();
        let mut count = 0;
        let mut search_error = None;
        while start <= end {
            let found = match crate::search::regex_replacement_in_bytes_with_target(&matcher,
                &candidate, replacement, start..end, &mut regex_state, last_target) {
                Ok(found) => found,
                Err(error) => { search_error = Some(error); break; }
            };
            let Some((range, replacement)) = found else { break; };
            let reached_end = range.end.0 == end;
            let length = range.end.0 - range.start.0;
            line_edits.change_bytes(&mut candidate, range.start.0..range.end.0, replacement.as_bytes())?;
            *last_target = Some(range.start.0 as i128..(range.start.0 + replacement.len()) as i128);
            regex_state.edit(range.start.0, length, replacement.len());
            selections.map_edit(range.start.0, length, 0);
            selections.map_edit(range.start.0, 0, replacement.len());
            start = range.start.0 + replacement.len();
            end = end - length + replacement.len();
            count += 1;
            if reached_end { break; }
        }
        if count == 0 {
            self.document.set_regex_state(regex_state);
            return search_error.map_or(Ok(0), |error| Err(error.into()));
        }
        if in_selection {
            selections = SelectionSet::single(Selection {
                anchor: ByteOffset(selected_start),
                head: ByteOffset(end),
            });
        }
        line_edits.suppress_current_modification_notifications();
        self.document
            .commit_prepared_bytes(candidate, selections, line_edits)?;
        self.document.set_regex_state(regex_state);
        self.invalidate_display();
        search_error.map_or(Ok(count), |error| Err(error.into()))
    }

    fn replace_all_plain(
        &mut self,
        query: &str,
        replacement: &str,
        options: crate::search::LiteralOptions,
        backwards: bool,
        in_selection: bool,
        last_target: &mut Option<Range<i128>>,
    ) -> Result<usize, crate::search::ReplaceError> {
        let query = options.mode.decode(query);
        if query.is_empty() { return Ok(0); }
        let replacement = options.mode.decode(replacement);
        let mut bytes = self.document.byte_snapshot();
        let mut selections = self.document.selections();
        let selected = self.source_selection_limits()?.range();
        let scope = if in_selection { selected.start.0..selected.end.0 }
            else if options.wrap { 0..bytes.len() }
            else if backwards { 0..selected.end.0 }
            else { selected.start.0..bytes.len() };
        let original_start = scope.start;
        let mut start = scope.start;
        let mut end = scope.end;
        let mut edits = crate::editor::LineEdits::default();
        let mut count = 0;
        while let Some(range) = {
            if scope.start != scope.end { *last_target = Some(start as i128..end as i128); }
            bytes.find_literal_bytes(
            query.as_bytes(), start, end, options.match_case, options.whole_word, false,
        ).map_err(|error| EditError::Storage(error.to_string()))?
        } {
            *last_target = Some(range.start as i128..range.end as i128);
            let reached_end = range.end == end;
            end = end - range.len() + replacement.len();
            start = range.start + replacement.len();
            selections.map_edit(range.start, range.len(), 0);
            selections.map_edit(range.start, 0, replacement.len());
            let target_start = range.start;
            edits.change_bytes(&mut bytes, range, replacement.as_bytes())?;
            *last_target = Some(target_start as i128..(target_start + replacement.len()) as i128);
            count += 1;
            if reached_end { break; }
        }
        if count == 0 { return Ok(0); }
        edits.suppress_current_modification_notifications();
        if in_selection {
            selections = SelectionSet::single(Selection { anchor: ByteOffset(original_start), head: ByteOffset(end) });
        }
        self.document.commit_prepared_bytes(bytes, selections, edits)?;
        self.invalidate_display();
        Ok(count)
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
            let mut candidate = self.document.byte_snapshot();
            let row = candidate.line_index(before.head.0).map_err(|error| EditError::Storage(error.to_string()))?;
            let start = candidate.line_start(row).map_err(|error| EditError::Storage(error.to_string()))?;
            let mut line_edits = crate::editor::LineEdits::default();
            line_edits.change_bytes(&mut candidate, start..start, converted.as_bytes())?;
            let caret = ByteOffset(before.head.0 + converted.len());
            self.document.commit_prepared_bytes(
                candidate,
                SelectionSet::single(Selection {
                    anchor: caret,
                    head: caret,
                }),
                line_edits,
            )?;
            self.invalidate_display();
            return Ok(());
        }
        self.insert_text(&converted)
    }

    pub fn replace_selection_bytes(&mut self, bytes: &[u8]) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.replace_selection_bytes(bytes)?;
        self.invalidate_display();
        Ok(())
    }

    pub fn add_bytes(&mut self, bytes: &[u8]) -> Result<(), EditError> {
        self.commit_composition()?;
        let caret = self.document.selection().head;
        self.document.replace_bytes(caret..caret, bytes)?;
        self.invalidate_display();
        Ok(())
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
        let text = self.document.byte_snapshot();
        let selection = self.document.selection();
        let row = text.line_index(selection.head.0).map_err(|error| EditError::Storage(error.to_string()))?;
        let start = text.line_start(row).map_err(|error| EditError::Storage(error.to_string()))?;
        let end = if row + 1 < text.line_count() {
            text.line_start(row + 1).map_err(|error| EditError::Storage(error.to_string()))?
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
        line_edits.change_bytes(&mut candidate, start..end, &[])?;
        self.document.commit_prepared_bytes(candidate, SelectionSet::single(mapped), line_edits)?;
        self.invalidate_display();
        Ok(())
    }

    pub fn duplicate(&mut self, for_line: bool) -> Result<(), EditError> {
        if self.has_composition() {
            return Err(EditError::UnsupportedComposition);
        }
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let tab_size = self.tabs.size;
        crate::text_transforms::session::apply_duplicate(self, for_line, tab_size).map_err(EditError::Storage)
    }

    /// Normalize committed CR/LF terminators in one undoable transaction. The
    /// separate Enter mode intentionally persists when text conversion is undone.
    pub fn convert_line_endings(&mut self, target: LineEnding) -> Result<(), EditError> {
        if self.document.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let mut bytes = self.document.byte_snapshot();
        let mut selections = self.document.selections();
        let mut edits = crate::editor::LineEdits::default();
        let mut changed = false;
        let mut position = 0;
        while position < bytes.len() {
            let byte = bytes.byte(position);
            let pair = byte == Some(b'\r') && bytes.byte(position + 1) == Some(b'\n');
            let operations: &[(usize, usize, &[u8])] = match (byte, pair, target) {
                (Some(b'\r'), true, LineEnding::Cr) => &[(position + 1, 1, b"")],
                (Some(b'\r'), true, LineEnding::Lf) => &[(position, 1, b"")],
                (Some(b'\r'), false, LineEnding::CrLf) => &[(position + 1, 0, b"\n")],
                (Some(b'\r'), false, LineEnding::Lf) => &[(position, 0, b"\n"), (position + 1, 1, b"")],
                (Some(b'\n'), _, LineEnding::CrLf) => &[(position, 0, b"\r")],
                (Some(b'\n'), _, LineEnding::Cr) => &[(position, 0, b"\r"), (position + 1, 1, b"")],
                _ => &[],
            };
            for &(start, removed, inserted) in operations {
                edits.change_bytes(&mut bytes, start..start + removed, inserted)?;
                selections.map_edit(start, removed, inserted.len());
                changed = true;
            }
            position += if matches!(byte, Some(b'\r' | b'\n')) { target.as_str().len() } else { 1 };
        }
        if changed { self.document.commit_prepared_bytes(bytes, selections, edits)?; }
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

    pub(crate) fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    pub fn word_selection_at(&self, position: ByteOffset) -> Result<Selection, EditError> {
        crate::editor::word_selection_in_snapshot(&self.display_snapshot(), position)
    }

    pub fn delimiter_selection_at(&self, position: ByteOffset, left: u8, right: u8, multiline: bool) -> Result<Option<Selection>, EditError> {
        let bytes = self.document.byte_snapshot();
        if position.0 > bytes.len() { return Err(EditError::InvalidRange); }
        let (start, end) = if multiline { (0, bytes.len()) } else {
            let line = bytes.line_index(position.0).map_err(|error| EditError::Storage(error.to_string()))?;
            let start = bytes.line_start(line).map_err(|error| EditError::Storage(error.to_string()))?;
            let mut end = start;
            while end < bytes.len() && !matches!(bytes.byte(end), Some(b'\r' | b'\n')) { end += 1; }
            (start, end)
        };
        let selected = if left == right {
            let escaped = |at: usize| left == b'"' && at > start && bytes.byte(at - 1) == Some(b'\\');
            let open = (start..=position.0.min(end)).rev().find(|at| bytes.byte(*at) == Some(left) && !escaped(*at));
            let close = (position.0..end).find(|at| bytes.byte(*at) == Some(right) && !escaped(*at));
            open.zip(close)
        } else {
            let mut stack = Vec::new();
            let mut selected = None;
            for at in start..end {
                if bytes.byte(at) == Some(left) { stack.push(at); }
                else if bytes.byte(at) == Some(right) {
                    if let Some(open) = stack.pop() {
                        if open <= position.0 && position.0 <= at && selected.is_none_or(|(previous, _)| open > previous) {
                            selected = Some((open, at));
                        }
                    }
                }
            }
            selected
        };
        Ok(selected.map(|(open, close)| Selection { anchor: ByteOffset(open + 1), head: ByteOffset(close) }))
    }

    pub(crate) fn source_selection_limits(&self) -> Result<Selection, EditError> {
        Ok(Self::limits_for_selections(&self.source_selections()?))
    }

    fn limits_for_selections(selections: &SelectionSet) -> Selection {
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
        if self.has_composition() { return Err(EditError::UnsupportedComposition); }
        let bytes = self.document.byte_snapshot();
        let position = self.document.selection().head.0;
        let word = Selection {
            anchor: ByteOffset(bytes.extend_word(position, false).map_err(|error| EditError::Storage(error.to_string()))?),
            head: ByteOffset(bytes.extend_word(position, true).map_err(|error| EditError::Storage(error.to_string()))?),
        };
        if word.range().is_empty() {
            return Ok(false);
        }
        let selections = self.document.selections();
        let mut ranges = selections.ranges().to_vec();
        ranges[selections.primary_index()] = word.into();
        self.document.set_selections(SelectionSet::from_parts(
            ranges,
            selections.primary_index(),
            selections.is_rectangular(),
        )?)?;
        self.invalidate_display();
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
        if self.has_composition() { self.selections() } else { self.document.display_selections() }
    }

    pub fn display_selection(&self) -> Selection {
        self.display_selections().primary().selection
    }

    pub(crate) fn source_selections(&self) -> Result<SelectionSet, EditError> {
        let Some(composition) = self.active_composition() else { return Ok(self.document.selections()); };
        if let Some(inline) = &composition.inline { return Ok(inline.source_selections.clone()); }
        let projection = crate::byte_projection::ByteProjection::from_bytes(&composition.bytes);
        Ok(SelectionSet::single(projection.source_selection(composition.selection)?))
    }

    pub(crate) fn display_byte_snapshot(&self) -> crate::byte_buffer::ByteBuffer {
        self.active_composition().map_or_else(|| self.document.byte_snapshot(), |composition| composition.bytes.clone())
    }

    pub fn adjacent_position(
        &self,
        position: ByteOffset,
        forward: bool,
    ) -> Result<ByteOffset, EditError> {
        if self.has_composition() {
            crate::editor::adjacent_in_snapshot(&self.display_snapshot(), position, forward)
        } else { self.document.adjacent_position(position, forward) }
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

    pub fn set_display_selections(&mut self, selections: SelectionSet) -> Result<(), EditError> {
        if self.has_composition() {
            return self.select_display(selections.primary().selection);
        }
        let mut ranges = selections.ranges().to_vec();
        for range in &mut ranges {
            range.selection = self.source_display_selection(range.selection)?;
        }
        self.set_selections(SelectionSet::from_parts(ranges, selections.primary_index(), selections.is_rectangular())?)
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

    pub(crate) fn tentative_display_selection(&mut self, mut range: SelectionRange) -> Result<(), EditError> {
        if self.has_composition() { return self.select_display(range.selection); }
        range.selection = self.source_display_selection(range.selection)?;
        self.tentative_selection(range)
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

    pub(crate) fn apply_completion(&mut self, edit: crate::intelligence::CompletionEdit) -> Result<(), EditError> {
        self.commit_composition()?;
        let mut bytes = self.document.byte_snapshot();
        let mut edits = crate::editor::LineEdits::default();
        edits.change_bytes(&mut bytes, edit.replace_range, &edit.bytes)?;
        let caret = ByteOffset(edit.caret_byte);
        self.commit_prepared_bytes(bytes, SelectionSet::single(Selection { anchor: caret, head: caret }), edits)
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

    pub(crate) fn commit_prepared_bytes(
        &mut self,
        bytes: crate::byte_buffer::ByteBuffer,
        selections: SelectionSet,
        edits: crate::editor::LineEdits,
    ) -> Result<(), EditError> {
        self.commit_composition()?;
        self.document.commit_prepared_bytes(bytes, selections, edits)?;
        self.invalidate_display();
        Ok(())
    }

    pub(crate) fn set_multi_selection_ranges(&mut self, ranges: &[crate::editor::SelectionRange]) {
        self.document.set_multi_selection_ranges(ranges);
        self.invalidate_display();
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
        if self.tabs.backspace_unindent && !self.has_composition() && !self.is_read_only() {
            let original = self.selections();
            let bytes = self.document.byte_snapshot();
            let mut ranges = original.ranges().to_vec();
            let mut changed = false;
            for range in &mut ranges {
                if !range.is_empty() { continue; }
                let position = range.selection.head.0;
                let line = bytes.line_index(position).map_err(|error| EditError::Storage(error.to_string()))?;
                let start = bytes.line_start(line).map_err(|error| EditError::Storage(error.to_string()))?;
                if position == start || !(start..position).all(|at| matches!(bytes.byte(at), Some(b' ' | b'\t'))) { continue; }
                let column = bytes.column(position, self.tabs.size).map_err(|error| EditError::Storage(error.to_string()))?;
                let target = column.saturating_sub(1) / self.tabs.size * self.tabs.size;
                let anchor = bytes.find_column(line, target, self.tabs.size).map_err(|error| EditError::Storage(error.to_string()))?;
                range.selection.anchor = ByteOffset(anchor);
                changed |= anchor != position;
            }
            if changed { self.set_selections(SelectionSet::from_parts(ranges, original.primary_index(), original.is_rectangular())?)?; }
        }
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
            self.document.toggle_bookmark()?;
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

    fn source_display_selection(&self, selection: Selection) -> Result<Selection, EditError> {
        let display = self.display_snapshot();
        let range = selection.range();
        if range.end.0 > display.len() {
            return Err(EditError::InvalidRange);
        }
        if !display.is_char_boundary(range.start.0) || !display.is_char_boundary(range.end.0) {
            return Err(EditError::InvalidBoundary);
        }
        if let Some(composition) = self.active_composition() {
            crate::byte_projection::ByteProjection::from_bytes(&composition.bytes).source_selection(selection)
        } else { self.document.projection().source_selection(selection) }
    }

    pub fn select_display(&mut self, selection: Selection) -> Result<(), EditError> {
        self.discard_stale_composition();
        let source = self.source_display_selection(selection)?;
        if let Some(composition) = &mut self.composition {
            composition.selection = selection;
            if let Some(inline) = &mut composition.inline {
                inline
                    .selections
                    .set_range(inline.selections.primary_index(), selection.into());
                inline.source_selections.set_range(inline.source_selections.primary_index(), source.into());
            }
            Ok(())
        } else {
        self.document.select(source)
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
                self.display_selection().anchor
            } else {
                head
            },
            head,
        })
    }

    /// Default Home/End behavior without wrapping: smart indentation Home and EOL-excluding End.
    pub fn move_to_line_edge(&mut self, end: bool, extend: bool) -> Result<(), EditError> {
        let text = self.display_snapshot();
        let selected = self.display_selection();
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
    ) -> Result<(Range<ByteOffset>, Vec<u8>), EditError> {
        let display = self.display_snapshot();
        if range.start > range.end || range.end.0 > display.len() {
            return Err(EditError::InvalidRange);
        }
        if !display.is_char_boundary(range.start.0) || !display.is_char_boundary(range.end.0) {
            return Err(EditError::InvalidBoundary);
        }
        if let Some(composition) = self.active_composition() {
            let projection = crate::byte_projection::ByteProjection::from_bytes(&composition.bytes);
            let selected = projection.source_selection(Selection { anchor: range.start, head: range.end })?.range();
            let marked_start = composition.base_range.start.0;
            let inserted_length = composition.bytes.len() + composition.base_range.end.0
                - composition.base_range.start.0 - self.document.len_bytes();
            let marked_end = marked_start + inserted_length;
            let start = selected.start.0.min(marked_start);
            let end = selected.end.0.max(marked_end);
            let mut replacement = composition.bytes.slice(start..selected.start.0)
                .map_err(|error| EditError::Storage(error.to_string()))?.to_vec();
            replacement.extend_from_slice(text.as_bytes());
            replacement.extend_from_slice(&composition.bytes.slice(selected.end.0..end)
                .map_err(|error| EditError::Storage(error.to_string()))?.to_vec());
            Ok((
                ByteOffset(start)..ByteOffset(composition.base_range.end.0 + end - marked_end),
                replacement,
            ))
        } else {
            let source = self.document.projection().source_selection(Selection { anchor: range.start, head: range.end })?;
            Ok((source.range(), text.as_bytes().to_vec()))
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
        let original_selection = range.is_none() && !self.has_composition();
        let range = range
            .or_else(|| self.marked_range())
            .unwrap_or_else(|| self.display_selection().range());
        let inserted = Document::new(text);
        let relative = selected_utf16.unwrap_or(inserted.len_utf16()..inserted.len_utf16());
        if relative.start > relative.end {
            return Err(EditError::InvalidRange);
        }
        let candidate_projection = crate::byte_projection::ByteProjection::from_bytes(&self.display_byte_snapshot());
        let requested = if original_selection { self.document.selection().range() }
            else { candidate_projection.source_selection(Selection { anchor: range.start, head: range.end })?.range() };
        let (base_range, replacement) = if original_selection { (requested.clone(), text.as_bytes().to_vec()) }
            else { self.display_edit(range, text)? };
        let mut bytes = self.document.byte_snapshot();
        bytes.replace(base_range.start.0..base_range.end.0, &replacement)
            .map_err(|error| EditError::Storage(error.to_string()))?;
        let projection = crate::byte_projection::ByteProjection::from_bytes(&bytes);
        let selection = projection.display_selection(Selection {
            anchor: ByteOffset(requested.start.0 + inserted.from_utf16(crate::editor::Utf16Offset(relative.start))?.0),
            head: ByteOffset(requested.start.0 + inserted.from_utf16(crate::editor::Utf16Offset(relative.end))?.0),
        })?;
        let marked_range = projection.display_selection(Selection {
            anchor: requested.start, head: ByteOffset(requested.start.0 + text.len()),
        })?.range();
        let display = projection.text();
        self.composition = Some(Composition {
            base_revision: self.document.text_revision(),
            composition_revision: self.document.composition_revision(),
            base_range,
            marked_range,
            text: text.to_string(),
            display,
            bytes,
            selection,
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
            let inserted_length = composition.bytes.len() + composition.base_range.end.0
                - composition.base_range.start.0 - self.document.len_bytes();
            let replacement = composition.bytes.slice(composition.base_range.start.0..composition.base_range.start.0 + inserted_length)
                .map_err(|error| EditError::Storage(error.to_string()))?.to_vec();
            self.document.replace_bytes(composition.base_range.clone(), &replacement)?;
            let selection = self.document.projection().source_selection(composition.selection)?;
            self.document
                .select_after_edit(selection, &previous_revision)?;
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
        if text == "\t" { return self.insert_tab(); }
        self.type_characters(text)
    }

    fn type_characters(&mut self, text: &str) -> Result<(), EditError> {
        for ch in text.chars() {
            if ch == '\t' && self.tabs.spaces { self.insert_tab()?; continue; }
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
        self.document.replace_bytes(mapped, &replacement)?;
        let caret = self.document.projection().source_position(
            crate::byte_projection::DisplayByteOffset(caret.0), crate::byte_projection::Affinity::After,
        )?;
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
