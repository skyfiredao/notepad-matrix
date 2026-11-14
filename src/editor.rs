use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use crate::byte_buffer::ByteBuffer;
use ropey::{LineType, Rope};

#[path = "selection.rs"]
mod selection;
pub use selection::{
    ColumnPosition, EditSelectionPolicy, OccurrenceOptions, SelectionRange, SelectionSet,
    TextTransform,
};
#[path = "column.rs"]
mod column;
pub use column::{ColumnBase, ColumnFill, ColumnNumbers};
#[path = "word_navigation.rs"]
mod word_navigation;
pub use word_navigation::WordBoundary;
pub(crate) use word_navigation::whole_word_at;
pub(crate) use word_navigation::word_range_in_snapshot;
pub(crate) use word_navigation::{boundary_in_snapshot, move_in_snapshot};
pub(crate) use word_navigation::{
    extend_selection_in_snapshot as extend_word_selection_in_snapshot,
    selection_in_snapshot as word_selection_in_snapshot,
};

/// Visible logical rows after keeping the caret in view (without line wrapping).
pub fn visible_rows(first: usize, capacity: usize, caret: usize, total: usize) -> Range<usize> {
    if total == 0 {
        return 0..0;
    }
    let capacity = capacity.max(1).min(total);
    let caret = caret.min(total - 1);
    let first = first.min(total - capacity);
    let first = if caret < first {
        caret
    } else if caret - first >= capacity {
        caret + 1 - capacity
    } else {
        first
    };
    first..first + capacity
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteOffset(pub usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Utf16Offset(pub usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: ByteOffset,
    pub head: ByteOffset,
}

impl Selection {
    pub fn range(self) -> Range<ByteOffset> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

#[derive(Debug)]
pub enum EditError {
    InvalidRange,
    InvalidBoundary,
    ReadOnly,
    UnsupportedComposition,
    VersionExhausted,
    ForeignSaveSnapshot,
    Storage(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange => formatter.write_str("text range is outside the document"),
            Self::InvalidBoundary => {
                formatter.write_str("text position splits a Unicode character")
            }
            Self::ReadOnly => formatter.write_str("document is read-only"),
            Self::UnsupportedComposition => {
                formatter.write_str("this edit is not yet implemented for active composition")
            }
            Self::VersionExhausted => formatter.write_str("document version counter exhausted"),
            Self::ForeignSaveSnapshot => {
                formatter.write_str("save completion belongs to a different document")
            }
            Self::Storage(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for EditError {}

struct Snapshot {
    content: Rope,
    bytes: ByteBuffer,
    version: u64,
}

struct RememberedSelection {
    view: Weak<Mutex<SelectionSet>>,
    selection: SelectionSet,
}

struct UndoAction {
    snapshot: Snapshot,
    line_edits: LineEdits,
    before_selections: Vec<RememberedSelection>,
    after_selections: Vec<RememberedSelection>,
}

#[derive(Clone, Copy)]
enum MarkerOp {
    Insert { line: usize, count: usize },
    Delete { line: usize, count: usize },
}

impl MarkerOp {
    fn apply(self, markers: &mut BTreeSet<usize>) {
        *markers = markers
            .iter()
            .map(|&row| match self {
                Self::Insert { line, count } if row >= line => row + count,
                Self::Delete { line, count } if row >= line => {
                    if row < line + count {
                        line.saturating_sub(1)
                    } else {
                        row - count
                    }
                }
                _ => row,
            })
            .collect();
    }
}

/// Line-vector changes are replayed on text Undo/Redo; marker toggles themselves
/// are not part of text snapshots, matching the source per-line marker storage.
#[derive(Clone, Default)]
pub(crate) struct LineEdits(
    Vec<(MarkerOp, MarkerOp)>,
    Vec<TextEdit>,
    Vec<Option<Arc<[u8]>>>,
);

#[derive(Clone, Copy)]
struct TextEdit {
    start: usize,
    removed: usize,
    inserted: usize,
}

impl TextEdit {
    fn apply_range(&self, range: &mut SelectionRange, undo: bool) {
        let (removed, inserted) = if undo {
            (self.inserted, self.removed)
        } else {
            (self.removed, self.inserted)
        };
        let selection = &mut range.selection;
        for (position, space) in [
            (&mut selection.anchor, &mut range.anchor_virtual),
            (&mut selection.head, &mut range.head_virtual),
        ] {
            if removed != 0 && position.0 >= self.start && position.0 <= self.start + removed {
                *space = 0;
            }
            if position.0 > self.start {
                position.0 = position.0.saturating_sub(removed).max(self.start);
            }
        }
        let anchor_start = selection.anchor < selection.head;
        let head_start = selection.head < selection.anchor;
        for (position, space, move_equal) in [
            (
                &mut selection.anchor,
                &mut range.anchor_virtual,
                anchor_start,
            ),
            (&mut selection.head, &mut range.head_virtual, head_start),
        ] {
            if position.0 == self.start && *space != 0 {
                let consumed = inserted.min(*space);
                *space -= consumed;
                position.0 += consumed;
                if move_equal {
                    position.0 += inserted - consumed;
                }
                continue;
            }
            if position.0 > self.start || (position.0 == self.start && move_equal) {
                position.0 += inserted;
            }
        }
    }
}

impl LineEdits {
    fn apply_bytes(&mut self, original: &ByteBuffer) -> Result<ByteBuffer, EditError> {
        if self.1.len() != self.2.len() {
            return Err(EditError::Storage(
                "byte edit payload count does not match the transaction".into(),
            ));
        }
        let mut bytes = original.clone();
        for (edit, inserted) in self.1.iter().zip(std::mem::take(&mut self.2)) {
            let inserted = inserted.as_deref().unwrap_or(&[]);
            if edit.inserted != inserted.len() {
                return Err(EditError::Storage(
                    "byte edit payload length does not match the transaction".into(),
                ));
            }
            bytes
                .replace(edit.start..edit.start + edit.removed, inserted)
                .map_err(|error| EditError::Storage(error.to_string()))?;
        }
        Ok(bytes)
    }

    fn record_changes(&self, core: &mut DocumentCore) {
        for edit in &self.1 {
            if edit.removed != 0 {
                core.apply_change_event(crate::change_history::Event::Delete(
                    edit.start,
                    edit.removed,
                ));
            }
            if edit.inserted != 0 {
                core.apply_change_event(crate::change_history::Event::Insert(
                    edit.start,
                    edit.inserted,
                ));
            }
        }
    }

    fn replay_changes(&self, core: &mut DocumentCore, undo: bool) {
        if undo {
            for edit in self.1.iter().rev() {
                if edit.inserted != 0 {
                    core.apply_change_event(crate::change_history::Event::UndoInsert(
                        edit.start,
                        edit.inserted,
                    ));
                }
                if edit.removed != 0 {
                    core.apply_change_event(crate::change_history::Event::UndoDelete(
                        edit.start,
                        edit.removed,
                    ));
                }
            }
        } else {
            for edit in &self.1 {
                if edit.removed != 0 {
                    core.apply_change_event(crate::change_history::Event::RedoDelete(
                        edit.start,
                        edit.removed,
                    ));
                }
                if edit.inserted != 0 {
                    core.apply_change_event(crate::change_history::Event::RedoInsert(
                        edit.start,
                        edit.inserted,
                    ));
                }
            }
        }
    }

    fn history_caret(&self, undo: bool) -> Option<ByteOffset> {
        if !undo {
            return self
                .1
                .iter()
                .rev()
                .find(|edit| edit.removed != 0 || edit.inserted != 0)
                .map(|edit| ByteOffset(edit.start + edit.inserted));
        }
        let mut caret = None;
        let mut coalesced_remove = 0..=0;
        for edit in self.1.iter().rev() {
            if edit.inserted != 0 {
                caret = Some(ByteOffset(edit.start));
                coalesced_remove = 0..=0;
            }
            if edit.removed != 0 {
                let end = if coalesced_remove.contains(&edit.start) {
                    *coalesced_remove.end() + edit.removed
                } else {
                    edit.start + edit.removed
                };
                let start = if coalesced_remove.contains(&edit.start) {
                    *coalesced_remove.start()
                } else {
                    edit.start
                };
                coalesced_remove = start..=end;
                caret = Some(ByteOffset(end));
            }
        }
        caret
    }

    pub(crate) fn record_text_edit(&mut self, start: usize, removed: usize, inserted: &str) {
        self.1.push(TextEdit {
            start,
            removed,
            inserted: inserted.len(),
        });
        self.2
            .push((!inserted.is_empty()).then(|| Arc::from(inserted.as_bytes())));
    }
    pub(crate) fn remove(&mut self, text: &mut Rope, range: Range<usize>) -> Result<(), EditError> {
        if range.is_empty() {
            return Ok(());
        }
        let before = text.clone();
        text.try_remove(range.clone())
            .map_err(|error| EditError::Storage(error.to_string()))?;
        self.1.push(TextEdit {
            start: range.start,
            removed: range.len(),
            inserted: 0,
        });
        self.2.push(None);
        self.record(&before, text, range.start, false);
        Ok(())
    }

    pub(crate) fn insert(
        &mut self,
        text: &mut Rope,
        start: usize,
        inserted: &str,
    ) -> Result<(), EditError> {
        if inserted.is_empty() {
            return Ok(());
        }
        let before = text.clone();
        text.try_insert(start, inserted)
            .map_err(|error| EditError::Storage(error.to_string()))?;
        self.1.push(TextEdit {
            start,
            removed: 0,
            inserted: inserted.len(),
        });
        self.2.push(Some(Arc::from(inserted.as_bytes())));
        self.record(&before, text, start, true);
        Ok(())
    }

    fn record(&mut self, before: &Rope, after: &Rope, start: usize, inserting: bool) {
        let old_count = before.len_lines(LineType::LF_CR);
        let new_count = after.len_lines(LineType::LF_CR);
        if old_count == new_count {
            return;
        }
        let old_row = before.byte_to_line_idx(start, LineType::LF_CR);
        let new_row = after.byte_to_line_idx(start.min(after.len()), LineType::LF_CR);
        let old_start = before.line_to_byte_idx(old_row, LineType::LF_CR) == start;
        let new_start = after.line_to_byte_idx(new_row, LineType::LF_CR) == start;
        let count = old_count.abs_diff(new_count);
        let pair = if inserting {
            (
                MarkerOp::Insert {
                    line: old_row + usize::from(!old_start),
                    count,
                },
                MarkerOp::Delete {
                    line: new_row + 1,
                    count,
                },
            )
        } else {
            (
                MarkerOp::Delete {
                    line: old_row + 1,
                    count,
                },
                MarkerOp::Insert {
                    line: new_row + usize::from(!new_start),
                    count,
                },
            )
        };
        self.0.push(pair);
    }

    fn apply(&self, markers: &mut BTreeSet<usize>, undo: bool) {
        if undo {
            for (_, inverse) in self.0.iter().rev() {
                inverse.apply(markers);
            }
        } else {
            for (forward, _) in &self.0 {
                forward.apply(markers);
            }
        }
    }
}

#[derive(Clone)]
pub struct SaveSnapshot {
    document_id: Arc<()>,
    version: u64,
    content: Rope,
    bytes: ByteBuffer,
    change_history: Arc<crate::change_history::History>,
    history_events: Arc<Mutex<Vec<crate::change_history::Event>>>,
}

impl SaveSnapshot {
    pub(crate) fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.bytes.chunks()
    }

    pub fn byte_snapshot(&self) -> ByteBuffer {
        self.bytes.clone()
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    pub fn write_to(&self, output: &mut impl std::io::Write) -> std::io::Result<()> {
        self.bytes.write_to(output)
    }
}

struct DocumentCore {
    change_history: crate::change_history::History,
    pending_save_history: Vec<Weak<Mutex<Vec<crate::change_history::Event>>>>,
    bookmarks: BTreeSet<usize>,
    identity: Arc<()>,
    current: Snapshot,
    working_selection: Selection,
    working_selection_changed: bool,
    saved_version: u64,
    buffer_dirty: bool,
    loaded_dirty: bool,
    next_version: u64,
    read_only: bool,
    undo: Vec<UndoAction>,
    redo: Vec<UndoAction>,
    undo_depth: usize,
    undo_group_edited: bool,
    views: Vec<Weak<Mutex<SelectionSet>>>,
    editing_view: Weak<Mutex<SelectionSet>>,
    revision: Arc<()>,
    composition_revision: Arc<()>,
    direct_edit: bool,
    may_coalesce: bool,
}

impl DocumentCore {
    pub(crate) fn from_recovery(text: &str) -> Self {
        let mut document = Self::new(text);
        // A loaded backup is dirty even when its bytes happen to match disk.
        document.current.version = 1;
        document.next_version = 2;
        document.buffer_dirty = true;
        document.loaded_dirty = true;
        document
    }

    pub fn new(text: &str) -> Self {
        Self {
            change_history: crate::change_history::History::new(text.len()),
            pending_save_history: Vec::new(),
            views: Vec::new(),
            editing_view: Weak::new(),
            revision: Arc::new(()),
            composition_revision: Arc::new(()),
            direct_edit: false,
            may_coalesce: false,
            bookmarks: BTreeSet::new(),
            undo_depth: 0,
            undo_group_edited: false,
            identity: Arc::new(()),
            current: Snapshot {
                content: Rope::from_str(text),
                bytes: ByteBuffer::from_bytes(text.as_bytes()),
                version: 0,
            },
            working_selection: Selection::default(),
            working_selection_changed: false,
            saved_version: 0,
            buffer_dirty: false,
            loaded_dirty: false,
            next_version: 1,
            read_only: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.current.content.to_string()
    }

    pub fn bookmarked_lines(&self) -> impl Iterator<Item = usize> + '_ {
        self.bookmarks.iter().copied()
    }

    pub fn toggle_bookmark(&mut self) {
        let line = self
            .current
            .content
            .byte_to_line_idx(self.working_selection.head.0, LineType::LF_CR);
        self.toggle_bookmark_at(line);
    }

    pub fn toggle_bookmark_at(&mut self, line: usize) -> bool {
        if line >= self.line_count() {
            return false;
        }
        if !self.bookmarks.remove(&line) {
            self.bookmarks.insert(line);
        }
        true
    }

    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.clear();
    }

    pub fn goto_bookmark(&mut self, forward: bool) -> Result<bool, EditError> {
        let line = self
            .current
            .content
            .byte_to_line_idx(self.working_selection.head.0, LineType::LF_CR);
        let target = if forward {
            self.bookmarks
                .range((std::ops::Bound::Excluded(line), std::ops::Bound::Unbounded))
                .next()
                .copied()
                .or_else(|| self.bookmarks.first().copied())
        } else {
            self.bookmarks
                .range(..line)
                .next_back()
                .copied()
                .or_else(|| self.bookmarks.last().copied())
        };
        let Some(target) = target else {
            return Ok(false);
        };
        let start = self.line_start(target)?;
        self.select(Selection {
            anchor: start,
            head: start,
        })?;
        Ok(true)
    }

    pub fn text_snapshot(&self) -> Rope {
        self.current.content.clone()
    }

    pub fn save_snapshot(&mut self) -> SaveSnapshot {
        self.pending_save_history
            .retain(|events| events.strong_count() > 0);
        let history_events = Arc::new(Mutex::new(Vec::new()));
        self.pending_save_history
            .push(Arc::downgrade(&history_events));
        SaveSnapshot {
            document_id: self.identity.clone(),
            version: self.current.version,
            content: self.current.content.clone(),
            bytes: self.current.bytes.clone(),
            change_history: Arc::new(self.change_history.clone()),
            history_events,
        }
    }

    pub fn acknowledge_save(&mut self, snapshot: &SaveSnapshot) -> Result<(), EditError> {
        self.validate_save_snapshot(snapshot)?;
        let was_modified = self.is_modified();
        self.saved_version = snapshot.version;
        if snapshot.version == self.current.version {
            self.apply_change_event(crate::change_history::Event::Saved);
        } else {
            let mut history = snapshot.change_history.as_ref().clone();
            history.saved();
            for event in lock(&snapshot.history_events).iter().copied() {
                event.replay_after_captured_save(&mut history);
            }
            self.change_history = history;
            self.broadcast_change_event(crate::change_history::Event::Saved);
        }
        self.notify_savepoint_transition(was_modified);
        self.may_coalesce = false;
        Ok(())
    }

    pub(crate) fn validate_save_snapshot(&self, snapshot: &SaveSnapshot) -> Result<(), EditError> {
        if !Arc::ptr_eq(&self.identity, &snapshot.document_id) {
            return Err(EditError::ForeignSaveSnapshot);
        }
        Ok(())
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn len_bytes(&self) -> usize {
        self.current.bytes.len()
    }

    pub fn len_utf16(&self) -> usize {
        self.current.content.len_utf16()
    }

    pub fn to_utf16(&self, position: ByteOffset) -> Result<Utf16Offset, EditError> {
        self.valid_range(position..position)?;
        Ok(Utf16Offset(
            self.current.content.byte_to_utf16_idx(position.0),
        ))
    }

    fn byte_from_utf16(&self, position: Utf16Offset) -> Result<ByteOffset, EditError> {
        if position.0 > self.len_utf16() {
            return Err(EditError::InvalidRange);
        }
        let byte = self.current.content.utf16_to_byte_idx(position.0);
        if self.current.content.byte_to_utf16_idx(byte) != position.0 {
            return Err(EditError::InvalidBoundary);
        }
        Ok(ByteOffset(byte))
    }

    pub fn line_count(&self) -> usize {
        self.current.bytes.line_count()
    }

    pub fn line_start(&self, index: usize) -> Result<ByteOffset, EditError> {
        self.current
            .bytes
            .line_start(index)
            .map(ByteOffset)
            .map_err(|_| EditError::InvalidRange)
    }

    pub fn line_text(&self, index: usize) -> Result<String, EditError> {
        if index >= self.line_count() {
            return Err(EditError::InvalidRange);
        }
        Ok(self
            .current
            .content
            .line(index, LineType::LF_CR)
            .to_string())
    }

    pub fn is_modified(&self) -> bool {
        self.current.version != self.saved_version
    }

    fn change_marker_bits(&self, line: usize) -> u8 {
        if line >= self.line_count() {
            return 0;
        }
        let text = &self.current.content;
        let start = text.line_to_byte_idx(line, LineType::LF_CR);
        let next = if line + 1 < self.line_count() {
            text.line_to_byte_idx(line + 1, LineType::LF_CR)
        } else {
            text.len()
        };
        let mut end = next;
        while end > start && matches!(text.byte(end - 1), b'\r' | b'\n') {
            end -= 1;
        }
        self.change_history.line_bits(start, next, end)
    }

    fn apply_change_event(&mut self, event: crate::change_history::Event) {
        event.apply(&mut self.change_history);
        self.broadcast_change_event(event);
    }

    fn broadcast_change_event(&mut self, event: crate::change_history::Event) {
        self.pending_save_history.retain(|events| {
            if let Some(events) = events.upgrade() {
                lock(&events).push(event);
                true
            } else {
                false
            }
        });
    }

    fn notify_savepoint_transition(&mut self, was_modified: bool) {
        if was_modified != self.is_modified() {
            self.notify_savepoint();
        }
    }

    fn notify_savepoint(&mut self) {
        self.buffer_dirty =
            self.is_modified() || (self.loaded_dirty && !self.can_undo() && self.buffer_dirty);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn begin_undo_action(&mut self) {
        if self.undo_depth == 0 {
            self.may_coalesce = false;
        }
        self.undo_depth = self.undo_depth.saturating_add(1);
        if self.undo_depth == 1 {
            self.undo_group_edited = false;
        }
    }

    pub(crate) fn end_undo_action(&mut self) {
        self.undo_depth = self.undo_depth.saturating_sub(1);
        if self.undo_depth == 0 {
            self.may_coalesce = false;
        }
    }

    fn record_edit_history(
        &mut self,
        previous: Snapshot,
        mut edits: LineEdits,
        before_selections: Vec<RememberedSelection>,
    ) {
        let coalesce = self.direct_edit
            && self.may_coalesce
            && self.current.version != self.saved_version
            && self.undo.last().is_some_and(|action| {
                match (
                    action.line_edits.1.last(),
                    edits
                        .1
                        .iter()
                        .find(|edit| edit.removed != 0 || edit.inserted != 0),
                ) {
                    (Some(previous), Some(next)) if previous.removed == 0 && next.removed == 0 => {
                        next.start == previous.start + previous.inserted
                    }
                    (Some(previous), Some(next))
                        if previous.inserted == 0
                            && next.inserted == 0
                            && (1..=2).contains(&next.removed) =>
                    {
                        next.start == previous.start || next.start + next.removed == previous.start
                    }
                    _ => false,
                }
            });
        if ((self.undo_depth != 0 && self.undo_group_edited) || coalesce)
            && let Some(grouped) = self.undo.last_mut()
        {
            grouped.line_edits.0.append(&mut edits.0);
            grouped.line_edits.1.append(&mut edits.1);
        } else {
            self.undo.push(UndoAction {
                snapshot: previous,
                line_edits: edits,
                before_selections,
                after_selections: Vec::new(),
            });
        }
        if self.undo_depth != 0 {
            self.undo_group_edited = true;
        }
        self.may_coalesce = self.direct_edit && self.undo_depth == 0;
        self.redo.clear();
    }

    fn capture_selections(&self) -> Vec<RememberedSelection> {
        self.views
            .iter()
            .filter_map(|view| {
                let live = view.upgrade()?;
                let selection = lock(&live).clone();
                Some(RememberedSelection {
                    view: view.clone(),
                    selection,
                })
            })
            .collect()
    }

    fn remember_redo_selections(&mut self) {
        let selections = self.capture_selections();
        if let Some(action) = self.undo.last_mut() {
            action.after_selections = selections;
        }
    }

    fn restore_selections(
        &mut self,
        selections: &[RememberedSelection],
        fallback: Option<ByteOffset>,
    ) {
        if let Some(position) = fallback {
            for view in &self.views {
                if let Some(view) = view.upgrade() {
                    *lock(&view) = SelectionSet::single(Selection {
                        anchor: position,
                        head: position,
                    });
                }
            }
        }
        for remembered in selections {
            if let Some(view) = remembered.view.upgrade() {
                *lock(&view) = remembered.selection.clone();
            }
        }
        if let Some(view) = self.editing_view.upgrade() {
            self.working_selection = lock(&view).primary().selection;
        }
    }

    pub(crate) fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.saved_version = self.current.version;
        self.apply_change_event(crate::change_history::Event::ClearUndo);
        self.may_coalesce = false;
    }

    pub fn mark_saved(&mut self) {
        self.saved_version = self.current.version;
        self.apply_change_event(crate::change_history::Event::Saved);
        self.notify_savepoint();
        self.may_coalesce = false;
    }

    pub fn set_read_only(&mut self, value: bool) {
        if value && !self.read_only {
            self.composition_revision = Arc::new(());
        }
        self.read_only = value;
    }

    fn valid_range(&self, range: Range<ByteOffset>) -> Result<Range<usize>, EditError> {
        if range.start > range.end || range.end.0 > self.len_bytes() {
            return Err(EditError::InvalidRange);
        }
        if !self.current.content.is_char_boundary(range.start.0)
            || !self.current.content.is_char_boundary(range.end.0)
        {
            return Err(EditError::InvalidBoundary);
        }
        Ok(range.start.0..range.end.0)
    }

    pub fn select(&mut self, selection: Selection) -> Result<(), EditError> {
        self.valid_range(selection.range())?;
        self.working_selection = selection;
        self.working_selection_changed = true;
        Ok(())
    }

    pub fn text_in(&self, range: Range<ByteOffset>) -> Result<String, EditError> {
        let range = self.valid_range(range)?;
        self.current
            .content
            .try_slice(range)
            .map(|text| text.to_string())
            .map_err(|error| EditError::Storage(error.to_string()))
    }

    pub fn replace_selection(&mut self, text: &str) -> Result<(), EditError> {
        self.replace(self.working_selection.range(), text)
    }

    pub(crate) fn replace_rope(
        &mut self,
        content: Rope,
        selection: Selection,
        mut line_edits: LineEdits,
    ) -> Result<(), EditError> {
        if self.read_only {
            return Err(EditError::ReadOnly);
        }
        let range = selection.range();
        if range.end.0 > content.len() {
            return Err(EditError::InvalidRange);
        }
        if !content.is_char_boundary(range.start.0) || !content.is_char_boundary(range.end.0) {
            return Err(EditError::InvalidBoundary);
        }
        let next_version = self
            .next_version
            .checked_add(1)
            .ok_or(EditError::VersionExhausted)?;
        let bytes = line_edits.apply_bytes(&self.current.bytes)?;
        if bytes.len() != content.len() {
            return Err(EditError::Storage(
                "byte transaction length does not match its Unicode cache".into(),
            ));
        }
        let before_selections = self.capture_selections();
        self.transform_views(&line_edits, false, true);
        line_edits.apply(&mut self.bookmarks, false);
        line_edits.record_changes(self);
        let previous = std::mem::replace(
            &mut self.current,
            Snapshot {
                content,
                bytes,
                version: self.next_version,
            },
        );
        self.notify_savepoint_transition(previous.version != self.saved_version);
        self.normalize_views();
        self.record_edit_history(previous, line_edits, before_selections);
        self.working_selection = selection;
        self.working_selection_changed = true;
        self.next_version = next_version;
        self.revision = Arc::new(());
        Ok(())
    }

    pub fn replace(&mut self, range: Range<ByteOffset>, text: &str) -> Result<(), EditError> {
        if self.read_only {
            return Err(EditError::ReadOnly);
        }
        let range = self.valid_range(range)?;
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        let next_version = self
            .next_version
            .checked_add(1)
            .ok_or(EditError::VersionExhausted)?;
        let mut changed = self.current.content.clone();
        let mut line_edits = LineEdits::default();
        line_edits.remove(&mut changed, range.clone())?;
        line_edits.insert(&mut changed, range.start, text)?;
        let bytes = line_edits.apply_bytes(&self.current.bytes)?;
        let caret = ByteOffset(range.start + text.len());
        let before_selections = self.capture_selections();
        self.transform_views(&line_edits, false, true);
        line_edits.apply(&mut self.bookmarks, false);
        line_edits.record_changes(self);
        let previous = std::mem::replace(
            &mut self.current,
            Snapshot {
                content: changed,
                bytes,
                version: self.next_version,
            },
        );
        self.notify_savepoint_transition(previous.version != self.saved_version);
        self.normalize_views();
        self.record_edit_history(previous, line_edits, before_selections);
        self.working_selection = Selection {
            anchor: caret,
            head: caret,
        };
        self.working_selection_changed = true;
        self.next_version = next_version;
        self.revision = Arc::new(());
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        self.may_coalesce = false;
        if self.read_only {
            return false;
        }
        if let Some(mut action) = self.undo.pop() {
            let was_modified = self.is_modified();
            self.transform_views(&action.line_edits, true, false);
            action.line_edits.apply(&mut self.bookmarks, true);
            action.line_edits.replay_changes(self, true);
            std::mem::swap(&mut self.current, &mut action.snapshot);
            self.notify_savepoint_transition(was_modified);
            self.restore_selections(
                &action.before_selections,
                action.line_edits.history_caret(true),
            );
            self.redo.push(action);
            self.revision = Arc::new(());
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        self.may_coalesce = false;
        if self.read_only {
            return false;
        }
        if let Some(mut action) = self.redo.pop() {
            let was_modified = self.is_modified();
            self.transform_views(&action.line_edits, false, false);
            action.line_edits.apply(&mut self.bookmarks, false);
            action.line_edits.replay_changes(self, false);
            std::mem::swap(&mut self.current, &mut action.snapshot);
            self.notify_savepoint_transition(was_modified);
            self.restore_selections(
                &action.after_selections,
                action.line_edits.history_caret(false),
            );
            self.undo.push(action);
            self.revision = Arc::new(());
            true
        } else {
            false
        }
    }

    fn normalize_views(&self) {
        for view in &self.views {
            if let Some(live) = view.upgrade() {
                lock(&live).normalize_boundaries(
                    &self.current.content,
                    Weak::ptr_eq(view, &self.editing_view),
                );
            }
        }
    }

    fn transform_views(&mut self, edits: &LineEdits, undo: bool, exclude_editing_view: bool) {
        self.views.retain(|view| view.strong_count() > 0);
        for view in &self.views {
            if let Some(view) = view.upgrade() {
                let mut selections = lock(&view);
                let primary = selections.primary;
                for (index, range) in selections.ranges.iter_mut().enumerate() {
                    if exclude_editing_view
                        && Weak::ptr_eq(&Arc::downgrade(&view), &self.editing_view)
                        && index == primary
                    {
                        continue;
                    }
                    if undo {
                        for edit in edits.1.iter().rev() {
                            edit.apply_range(range, true);
                        }
                    } else {
                        for edit in &edits.1 {
                            edit.apply_range(range, false);
                        }
                    }
                }
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn adjacent_in_snapshot(
    text: &Rope,
    position: ByteOffset,
    forward: bool,
) -> Result<ByteOffset, EditError> {
    if position.0 > text.len() {
        return Err(EditError::InvalidRange);
    }
    if !text.is_char_boundary(position.0) {
        return Err(EditError::InvalidBoundary);
    }
    let mut target = position.0;
    if forward && target < text.len() {
        let ch = text.char(target);
        target += ch.len_utf8();
        if ch == '\r' && target < text.len() && text.byte(target) == b'\n' {
            target += 1;
        }
    } else if !forward && target > 0 {
        target -= 1;
        while !text.is_char_boundary(target) {
            target -= 1;
        }
        if target > 0 && text.byte(target) == b'\n' && text.byte(target - 1) == b'\r' {
            target -= 1;
        }
    }
    Ok(ByteOffset(target))
}

pub struct Document {
    core: Arc<Mutex<DocumentCore>>,
    selection: Arc<Mutex<SelectionSet>>,
    occurrence_options: OccurrenceOptions,
    tentative_selections: Option<SelectionSet>,
}

impl Document {
    pub fn new(text: &str) -> Self {
        Self::from_core(DocumentCore::new(text))
    }
    pub(crate) fn from_recovery(text: &str) -> Self {
        Self::from_core(DocumentCore::from_recovery(text))
    }
    fn from_core(core: DocumentCore) -> Self {
        let document = Self {
            core: Arc::new(Mutex::new(core)),
            selection: Arc::new(Mutex::new(SelectionSet::default())),
            occurrence_options: OccurrenceOptions::default(),
            tentative_selections: None,
        };
        lock(&document.core)
            .views
            .push(Arc::downgrade(&document.selection));
        document
    }
    pub fn clone_view(&self) -> Self {
        let mut core = lock(&self.core);
        core.views.retain(|view| view.strong_count() > 0);
        let selection = Arc::new(Mutex::new(lock(&self.selection).clone()));
        core.views.push(Arc::downgrade(&selection));
        Self {
            core: self.core.clone(),
            selection,
            occurrence_options: self.occurrence_options,
            tentative_selections: None,
        }
    }
    pub fn shares_document_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.core, &other.core)
    }
    pub(crate) fn text_revision(&self) -> Arc<()> {
        lock(&self.core).revision.clone()
    }
    pub(crate) fn buffer_is_modified(&self) -> bool {
        lock(&self.core).buffer_dirty
    }
    pub(crate) fn set_buffer_dirty(&self, dirty: bool) {
        lock(&self.core).buffer_dirty = dirty;
    }
    pub(crate) fn composition_revision(&self) -> Arc<()> {
        lock(&self.core).composition_revision.clone()
    }
    fn edit<T>(&mut self, operation: impl FnOnce(&mut DocumentCore) -> T) -> T {
        let mut core = lock(&self.core);
        let next_version = core.next_version;
        core.working_selection = lock(&self.selection).primary().selection;
        core.working_selection_changed = false;
        core.editing_view = Arc::downgrade(&self.selection);
        let result = operation(&mut core);
        if core.working_selection_changed {
            let mut selections = lock(&self.selection);
            let primary = selections.primary;
            selections.ranges[primary] = core.working_selection.into();
        }
        if core.next_version != next_version {
            core.remember_redo_selections();
        }
        result
    }
    pub fn text(&self) -> String {
        lock(&self.core).text()
    }
    pub fn byte_snapshot(&self) -> ByteBuffer {
        lock(&self.core).current.bytes.clone()
    }
    pub fn change_marker_bits(&self, line: usize) -> u8 {
        lock(&self.core).change_marker_bits(line)
    }
    pub fn goto_change(&mut self, forward: bool) -> Result<bool, EditError> {
        let text = self.text_snapshot();
        let current = text.byte_to_line_idx(self.selection().head.0, LineType::LF_CR);
        let count = self.line_count();
        let core = lock(&self.core);
        let target = if forward {
            let mut block = current;
            let mut found = None;
            for line in current..count {
                if core.change_marker_bits(line) != 0 {
                    if line != block {
                        found = Some(line);
                        break;
                    }
                    block += 1;
                }
            }
            found.or_else(|| (0..=current).find(|&line| core.change_marker_bits(line) != 0))
        } else {
            let mut before = current as isize;
            while before >= 0 && core.change_marker_bits(before as usize) != 0 {
                before -= 1;
            }
            (0..=before)
                .rev()
                .map(|line| line as usize)
                .find(|&line| core.change_marker_bits(line) != 0)
                .or_else(|| {
                    (0..count)
                        .rev()
                        .find(|&line| core.change_marker_bits(line) != 0)
                })
        };
        drop(core);
        let Some(line) = target else {
            return Ok(false);
        };
        let position = self.line_start(line)?;
        self.select(Selection {
            anchor: position,
            head: position,
        })?;
        Ok(true)
    }
    pub fn clear_change_history(&mut self) -> Result<(), EditError> {
        let position = self.selection().head;
        self.edit(|core| {
            core.clear_history();
            core.apply_change_event(crate::change_history::Event::ClearChanges(core.len_bytes()));
        });
        self.select(Selection {
            anchor: position,
            head: position,
        })
    }
    pub fn text_snapshot(&self) -> Rope {
        lock(&self.core).text_snapshot()
    }
    pub fn save_snapshot(&self) -> SaveSnapshot {
        lock(&self.core).save_snapshot()
    }
    pub fn acknowledge_save(&mut self, snapshot: &SaveSnapshot) -> Result<(), EditError> {
        self.edit(|c| c.acknowledge_save(snapshot))
    }
    pub(crate) fn validate_save_snapshot(&self, snapshot: &SaveSnapshot) -> Result<(), EditError> {
        lock(&self.core).validate_save_snapshot(snapshot)
    }
    pub fn bookmarked_lines(&self) -> impl Iterator<Item = usize> + '_ {
        lock(&self.core)
            .bookmarked_lines()
            .collect::<Vec<_>>()
            .into_iter()
    }
    pub fn toggle_bookmark(&mut self) {
        self.edit(DocumentCore::toggle_bookmark)
    }
    pub fn toggle_bookmark_at(&mut self, line: usize) -> bool {
        self.edit(|c| c.toggle_bookmark_at(line))
    }
    pub fn clear_bookmarks(&mut self) {
        self.edit(DocumentCore::clear_bookmarks)
    }
    pub fn goto_bookmark(&mut self, forward: bool) -> Result<bool, EditError> {
        self.edit(|c| c.goto_bookmark(forward))
    }
    pub fn is_read_only(&self) -> bool {
        lock(&self.core).is_read_only()
    }
    pub fn len_bytes(&self) -> usize {
        lock(&self.core).len_bytes()
    }
    pub fn len_utf16(&self) -> usize {
        lock(&self.core).len_utf16()
    }
    pub fn to_utf16(&self, position: ByteOffset) -> Result<Utf16Offset, EditError> {
        lock(&self.core).to_utf16(position)
    }
    pub fn from_utf16(&self, position: Utf16Offset) -> Result<ByteOffset, EditError> {
        lock(&self.core).byte_from_utf16(position)
    }
    pub fn line_count(&self) -> usize {
        lock(&self.core).line_count()
    }
    pub fn line_start(&self, index: usize) -> Result<ByteOffset, EditError> {
        lock(&self.core).line_start(index)
    }
    pub fn line_text(&self, index: usize) -> Result<String, EditError> {
        lock(&self.core).line_text(index)
    }
    pub fn selection(&self) -> Selection {
        lock(&self.selection).primary().selection
    }
    pub fn adjacent_position(
        &self,
        position: ByteOffset,
        forward: bool,
    ) -> Result<ByteOffset, EditError> {
        adjacent_in_snapshot(&self.text_snapshot(), position, forward)
    }
    pub fn selections(&self) -> SelectionSet {
        lock(&self.selection).clone()
    }
    pub fn set_selections(&mut self, selections: SelectionSet) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        for range in &selections.ranges {
            core.valid_range(range.selection.range())?;
            for (position, virtual_space) in [
                (range.selection.anchor, range.anchor_virtual),
                (range.selection.head, range.head_virtual),
            ] {
                if virtual_space != 0 {
                    let row = core
                        .current
                        .content
                        .byte_to_line_idx(position.0, LineType::LF_CR);
                    let line = core.line_text(row)?;
                    let end = core.line_start(row)?.0 + line.trim_end_matches(['\r', '\n']).len();
                    if position.0 != end {
                        return Err(EditError::InvalidRange);
                    }
                }
            }
        }
        *lock(&self.selection) = selections;
        core.may_coalesce = false;
        self.tentative_selections = None;
        Ok(())
    }

    pub fn tentative_selection(&mut self, range: SelectionRange) -> Result<(), EditError> {
        let baseline = self
            .tentative_selections
            .clone()
            .unwrap_or_else(|| self.selections());
        let mut selections = baseline.clone();
        selections.add(range);
        selections.trim_primary();
        self.set_selections(selections)?;
        self.tentative_selections = Some(baseline);
        Ok(())
    }

    pub fn commit_tentative_selection(&mut self) {
        self.tentative_selections = None;
    }
    pub fn add_selection(&mut self, range: SelectionRange) -> Result<(), EditError> {
        let mut selections = self.selections();
        selections.add(range);
        self.set_selections(selections)
    }
    pub fn rotate_selection(&mut self) {
        let mut selections = lock(&self.selection);
        selections.primary = (selections.primary + 1) % selections.ranges.len();
    }
    pub fn swap_main_anchor_caret(&mut self) {
        let mut selections = lock(&self.selection);
        let primary = selections.primary;
        let range = &mut selections.ranges[primary];
        std::mem::swap(&mut range.selection.anchor, &mut range.selection.head);
        std::mem::swap(&mut range.anchor_virtual, &mut range.head_virtual);
    }
    pub fn remove_selection(&mut self, index: usize) {
        lock(&self.selection).remove(index);
    }
    pub fn clear_additional_selections(&mut self) {
        let mut selections = lock(&self.selection);
        *selections = SelectionSet::single(selections.primary().selection);
    }
    pub fn apply_transforms(&mut self, transforms: &[TextTransform]) -> Result<(), EditError> {
        self.apply_multi(transforms, EditSelectionPolicy::MapOriginal)
    }
    pub fn apply_multi(
        &mut self,
        transforms: &[TextTransform],
        policy: EditSelectionPolicy,
    ) -> Result<(), EditError> {
        let explicit = match policy {
            EditSelectionPolicy::MapOriginal => None,
            EditSelectionPolicy::Explicit(selections) => Some(selections),
        };
        self.apply_transaction(transforms, None, explicit)
    }
    pub(crate) fn commit_prepared(
        &mut self,
        content: Rope,
        selections: SelectionSet,
        edits: LineEdits,
    ) -> Result<(), EditError> {
        for range in selections.ranges() {
            let range = range.selection.range();
            if range.end.0 > content.len() {
                return Err(EditError::InvalidRange);
            }
            if !content.is_char_boundary(range.start.0) || !content.is_char_boundary(range.end.0) {
                return Err(EditError::InvalidBoundary);
            }
        }
        let mut core = lock(&self.core);
        core.editing_view = Arc::downgrade(&self.selection);
        if edits.1.is_empty() {
            if core.read_only {
                return Err(EditError::ReadOnly);
            }
            *lock(&self.selection) = selections;
            self.tentative_selections = None;
            return Ok(());
        }
        core.replace_rope(content, selections.primary().selection, edits)?;
        *lock(&self.selection) = selections;
        core.remember_redo_selections();
        self.tentative_selections = None;
        Ok(())
    }
    pub(crate) fn direct_transforms(
        &mut self,
        transforms: &[TextTransform],
    ) -> Result<(), EditError> {
        lock(&self.core).direct_edit = true;
        let indices: Vec<_> = (0..transforms.len()).collect();
        let result = self.apply_transaction(transforms, Some(&indices), None);
        lock(&self.core).direct_edit = false;
        result
    }
    fn apply_transaction(
        &mut self,
        transforms: &[TextTransform],
        replacements: Option<&[usize]>,
        explicit: Option<SelectionSet>,
    ) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        if core.read_only {
            return Err(EditError::ReadOnly);
        }
        let mut order: Vec<usize> = (0..transforms.len()).collect();
        order.sort_by_key(|&index| (transforms[index].range.start, transforms[index].range.end));
        let mut last_end = None;
        for &index in &order {
            let range = core.valid_range(transforms[index].range.clone())?;
            if last_end.is_some_and(|end| range.start < end) {
                return Err(EditError::InvalidRange);
            }
            last_end = Some(range.end);
        }
        let mut candidate = core.current.content.clone();
        let mut edits = LineEdits::default();
        let mut selections = lock(&self.selection).clone();
        let mut changed = false;
        for &index in order.iter().rev() {
            let transform = &transforms[index];
            let start = transform.range.start.0;
            let end = transform.range.end.0;
            if start == end && transform.text.is_empty() {
                continue;
            }
            changed = true;
            edits.remove(&mut candidate, start..end)?;
            edits.insert(&mut candidate, start, &transform.text)?;
            let mapping = TextEdit {
                start,
                removed: end - start,
                inserted: transform.text.len(),
            };
            for range in &mut selections.ranges {
                mapping.apply_range(range, false);
            }
            if let Some(replacements) = replacements {
                let caret = ByteOffset(start + transform.text.len());
                selections.ranges[replacements[index]] = Selection {
                    anchor: caret,
                    head: caret,
                }
                .into();
            }
        }
        selections.normalize_boundaries(&candidate, false);
        let preserve_explicit = explicit.is_some();
        if let Some(explicit) = explicit {
            for selected in explicit.ranges() {
                let range = selected.selection.range();
                if range.end.0 > candidate.len() {
                    return Err(EditError::InvalidRange);
                }
                if !candidate.is_char_boundary(range.start.0)
                    || !candidate.is_char_boundary(range.end.0)
                {
                    return Err(EditError::InvalidBoundary);
                }
                for (position, space) in [
                    (selected.selection.anchor, selected.anchor_virtual),
                    (selected.selection.head, selected.head_virtual),
                ] {
                    if space != 0 {
                        let row = candidate.byte_to_line_idx(position.0, LineType::LF_CR);
                        let end = candidate.line_to_byte_idx(row, LineType::LF_CR)
                            + candidate
                                .line(row, LineType::LF_CR)
                                .to_string()
                                .trim_end_matches(['\r', '\n'])
                                .len();
                        if position.0 != end {
                            return Err(EditError::InvalidRange);
                        }
                    }
                }
            }
            selections = explicit;
        }
        if !changed {
            *lock(&self.selection) = selections;
            return Ok(());
        }
        if !preserve_explicit {
            selections.deduplicate();
        }
        core.editing_view = Arc::downgrade(&self.selection);
        core.replace_rope(candidate, selections.primary().selection, edits)?;
        *lock(&self.selection) = selections;
        core.remember_redo_selections();
        Ok(())
    }
    pub fn replace_selections(&mut self, text: &str) -> Result<(), EditError> {
        self.replace_evolving_selections(text, false, false)
    }

    pub(crate) fn type_character(&mut self, ch: char, overwrite: bool) -> Result<(), EditError> {
        let mut encoded = [0; 4];
        self.replace_evolving_selections(ch.encode_utf8(&mut encoded), overwrite, true)
    }

    fn replace_evolving_selections(
        &mut self,
        text: &str,
        overwrite: bool,
        direct: bool,
    ) -> Result<(), EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let mut candidate = self.text_snapshot();
        let mut selections = self.selections();
        let mut edits = LineEdits::default();
        let mut order: Vec<_> = (0..selections.ranges().len()).collect();
        if direct {
            order.sort_by_key(|&index| {
                let range = selections.ranges()[index];
                (
                    range.selection.head,
                    range.head_virtual,
                    range.selection.anchor,
                    range.anchor_virtual,
                )
            });
            order.reverse();
        }
        for index in order {
            let selected = selections.ranges()[index];
            let range = selected.selection.range();
            if !selected.is_empty() {
                edits.remove(&mut candidate, range.start.0..range.end.0)?;
                selections.map_edit(range.start.0, range.end.0 - range.start.0, 0);
                let space = if range.is_empty() {
                    selected.start_virtual()
                } else {
                    0
                };
                selections.set_range(
                    index,
                    SelectionRange {
                        selection: Selection {
                            anchor: range.start,
                            head: range.start,
                        },
                        anchor_virtual: space,
                        head_virtual: space,
                    },
                );
            } else if overwrite
                && selected.head_virtual == 0
                && range.start.0 < candidate.len()
                && !matches!(candidate.char(range.start.0), '\r' | '\n')
            {
                let end = adjacent_in_snapshot(&candidate, range.start, true)?;
                edits.remove(&mut candidate, range.start.0..end.0)?;
                selections.map_edit(range.start.0, end.0 - range.start.0, 0);
            }
            let selected = selections.ranges()[index];
            let start = selected.selection.range().start.0;
            let padding = " ".repeat(selected.start_virtual());
            edits.insert(&mut candidate, start, &padding)?;
            selections.map_edit(start, 0, padding.len());
            let insertion = start + padding.len();
            edits.insert(&mut candidate, insertion, text)?;
            selections.map_edit(insertion, 0, text.len());
            let caret = ByteOffset(insertion + text.len());
            selections.set_range(
                index,
                Selection {
                    anchor: caret,
                    head: caret,
                }
                .into(),
            );
            selections.normalize_boundaries(&candidate, false);
        }
        lock(&self.core).direct_edit = direct;
        let result = self.commit_prepared(candidate, selections, edits);
        lock(&self.core).direct_edit = false;
        result
    }
    pub fn delete_selections(&mut self, backward: bool) -> Result<(), EditError> {
        self.delete_characters(backward, true)
    }

    pub fn backspace_not_line(&mut self) -> Result<(), EditError> {
        self.delete_characters(true, false)
    }

    fn delete_characters(
        &mut self,
        backward: bool,
        allow_line_start: bool,
    ) -> Result<(), EditError> {
        if self.is_read_only() {
            let selections = self.selections();
            let selected = selections.primary();
            if selections.ranges().len() == 1
                && !selections.is_rectangular()
                && selected.anchor_virtual == 0
                && selected.head_virtual == 0
                && !selected.is_empty()
            {
                let start = selected.selection.range().start;
                self.select(Selection {
                    anchor: start,
                    head: start,
                })?;
            }
            return Err(EditError::ReadOnly);
        }
        let selections = self.selections();
        let text = self.text_snapshot();
        let any_selected = selections.ranges().iter().any(|range| !range.is_empty());
        if any_selected {
            let mut candidate = text;
            let mut selections = selections;
            let mut edits = LineEdits::default();
            for index in 0..selections.ranges().len() {
                let selected = selections.ranges()[index];
                if selected.is_empty() {
                    continue;
                }
                let range = selected.selection.range();
                edits.remove(&mut candidate, range.start.0..range.end.0)?;
                selections.map_edit(range.start.0, range.end.0 - range.start.0, 0);
                let space = if range.is_empty() {
                    selected.start_virtual()
                } else {
                    0
                };
                selections.set_range(
                    index,
                    SelectionRange {
                        selection: Selection {
                            anchor: range.start,
                            head: range.start,
                        },
                        anchor_virtual: space,
                        head_virtual: space,
                    },
                );
            }
            selections.normalize_boundaries(&candidate, false);
            selections.deduplicate();
            self.begin_undo_action();
            let result = self.commit_prepared(candidate, selections, edits);
            self.end_undo_action();
            return result;
        }
        let grouped = selections.ranges().len() > 1
            || (!backward && selections.primary().start_virtual() != 0);
        let mut virtual_backspaces = Vec::new();
        let transforms: Vec<_> = selections
            .ranges()
            .iter()
            .enumerate()
            .map(|(index, selected)| {
                let mut range = selected.selection.range();
                let mut inserted = String::new();
                if backward && selected.head_virtual != 0 {
                    virtual_backspaces.push(index);
                } else if backward && range.start.0 > 0 {
                    let position = range.start.0;
                    let row = text.byte_to_line_idx(position, LineType::LF_CR);
                    if (allow_line_start && !selections.is_rectangular())
                        || text.line_to_byte_idx(row, LineType::LF_CR) != position
                    {
                        range.start = adjacent_in_snapshot(&text, range.start, false)?;
                    }
                } else if !backward {
                    let position = range.start.0;
                    if selected.start_virtual() != 0 {
                        inserted = " ".repeat(selected.start_virtual());
                    }
                    if position < text.len() {
                        let ch = text.char(position);
                        if selections.ranges().len() == 1 || !matches!(ch, '\r' | '\n') {
                            range.end = adjacent_in_snapshot(&text, range.end, true)?;
                        }
                    }
                }
                Ok(TextTransform {
                    range,
                    text: inserted,
                })
            })
            .collect::<Result<_, EditError>>()?;
        if grouped {
            self.begin_undo_action();
        }
        let result = self.direct_transforms(&transforms);
        if grouped {
            self.end_undo_action();
        }
        result?;
        let mut final_selections = lock(&self.selection);
        for index in virtual_backspaces {
            if let Some(range) = final_selections.ranges.get_mut(index) {
                range.anchor_virtual = range.anchor_virtual.saturating_sub(1);
                range.head_virtual = range.head_virtual.saturating_sub(1);
            }
        }
        final_selections.deduplicate();
        drop(final_selections);
        lock(&self.core).remember_redo_selections();
        Ok(())
    }
    pub fn is_modified(&self) -> bool {
        lock(&self.core).is_modified()
    }
    pub fn can_undo(&self) -> bool {
        lock(&self.core).can_undo()
    }
    pub fn can_redo(&self) -> bool {
        lock(&self.core).can_redo()
    }
    pub(crate) fn begin_undo_action(&mut self) {
        self.edit(DocumentCore::begin_undo_action)
    }
    pub(crate) fn end_undo_action(&mut self) {
        let completed_group = self.edit(|core| {
            let completed_group = core.undo_depth == 1 && core.undo_group_edited;
            core.end_undo_action();
            completed_group
        });
        if completed_group {
            lock(&self.core).remember_redo_selections();
        }
    }
    pub(crate) fn clear_history(&mut self) {
        self.edit(DocumentCore::clear_history)
    }
    pub fn mark_saved(&mut self) {
        self.edit(DocumentCore::mark_saved)
    }
    pub fn set_read_only(&mut self, value: bool) {
        self.edit(|c| c.set_read_only(value))
    }
    pub fn select(&mut self, selection: Selection) -> Result<(), EditError> {
        self.set_selections(SelectionSet::single(selection))
    }
    pub(crate) fn select_after_edit(
        &mut self,
        selection: Selection,
        previous_revision: &Arc<()>,
    ) -> Result<(), EditError> {
        self.select(selection)?;
        let mut core = lock(&self.core);
        if !Arc::ptr_eq(previous_revision, &core.revision) {
            core.remember_redo_selections();
        }
        Ok(())
    }
    pub fn text_in(&self, range: Range<ByteOffset>) -> Result<String, EditError> {
        lock(&self.core).text_in(range)
    }
    pub fn replace_selection(&mut self, text: &str) -> Result<(), EditError> {
        self.edit(|c| c.replace_selection(text))
    }
    pub(crate) fn replace_rope(
        &mut self,
        content: Rope,
        selection: Selection,
        line_edits: LineEdits,
    ) -> Result<(), EditError> {
        self.edit(|c| c.replace_rope(content, selection, line_edits))
    }
    pub fn replace(&mut self, range: Range<ByteOffset>, text: &str) -> Result<(), EditError> {
        self.edit(|c| c.replace(range, text))
    }
    pub fn undo(&mut self) -> bool {
        let mut core = lock(&self.core);
        core.editing_view = Arc::downgrade(&self.selection);
        core.undo()
    }
    pub fn redo(&mut self) -> bool {
        let mut core = lock(&self.core);
        core.editing_view = Arc::downgrade(&self.selection);
        core.redo()
    }
}
