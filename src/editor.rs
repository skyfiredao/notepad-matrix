use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use crate::byte_buffer::ByteBuffer;
use crate::byte_projection::ByteProjection;
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
    projection: ByteProjection,
    version: u64,
}

impl Snapshot {
    fn bytes(bytes: ByteBuffer, version: u64) -> Self {
        let projection = ByteProjection::from_bytes(&bytes);
        Self { content: projection.text(), bytes, projection, version }
    }
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
pub(crate) struct LineEdits {
    line_operations: Vec<(MarkerOp, MarkerOp)>,
    changes: Vec<TextEdit>,
    modified_lines: Vec<(usize, usize)>,
    silent_changes: usize,
}

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
    fn record_changes(&self, core: &mut DocumentCore) {
        for (index, (edit, &(modified_line, _))) in self.changes.iter().zip(&self.modified_lines).enumerate() {
            if edit.removed != 0 {
                core.apply_change_event_with_notifications(crate::change_history::Event::Delete(
                    edit.start,
                    edit.removed,
                ), index >= self.silent_changes, Some(modified_line));
            }
            if edit.inserted != 0 {
                core.apply_change_event_with_notifications(crate::change_history::Event::Insert(
                    edit.start,
                    edit.inserted,
                ), index >= self.silent_changes, Some(modified_line));
            }
        }
    }

    pub(crate) fn suppress_current_modification_notifications(&mut self) {
        self.silent_changes = self.changes.len();
    }

    fn replay_changes(&self, core: &mut DocumentCore, undo: bool) {
        if undo {
            for (edit, &(_, undo_modified_line)) in self.changes.iter().zip(&self.modified_lines).rev() {
                if edit.inserted != 0 {
                    core.apply_change_event_with_notifications(crate::change_history::Event::UndoInsert(
                        edit.start,
                        edit.inserted,
                    ), true, Some(undo_modified_line));
                }
                if edit.removed != 0 {
                    core.apply_change_event_with_notifications(crate::change_history::Event::UndoDelete(
                        edit.start,
                        edit.removed,
                    ), true, Some(undo_modified_line));
                }
            }
        } else {
            for (edit, &(modified_line, _)) in self.changes.iter().zip(&self.modified_lines) {
                if edit.removed != 0 {
                    core.apply_change_event_with_notifications(crate::change_history::Event::RedoDelete(
                        edit.start,
                        edit.removed,
                    ), true, Some(modified_line));
                }
                if edit.inserted != 0 {
                    core.apply_change_event_with_notifications(crate::change_history::Event::RedoInsert(
                        edit.start,
                        edit.inserted,
                    ), true, Some(modified_line));
                }
            }
        }
    }

    fn history_caret(&self, undo: bool) -> Option<ByteOffset> {
        if !undo {
            return self
                .changes
                .iter()
                .rev()
                .find(|edit| edit.removed != 0 || edit.inserted != 0)
                .map(|edit| ByteOffset(edit.start + edit.inserted));
        }
        let mut caret = None;
        let mut coalesced_remove = 0..=0;
        for edit in self.changes.iter().rev() {
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

    fn record_byte_edit(&mut self, before: &ByteBuffer, after: &ByteBuffer, start: usize, removed: usize, inserted: &[u8]) -> Result<(), EditError> {
        let modified_line = after.line_index(start).map_err(|error| EditError::Storage(error.to_string()))?;
        let undo_modified_line = before.line_index(start).map_err(|error| EditError::Storage(error.to_string()))?;
        self.changes.push(TextEdit {
            start,
            removed,
            inserted: inserted.len(),
        });
        self.modified_lines.push((modified_line, undo_modified_line));
        Ok(())
    }

    pub(crate) fn change_bytes(&mut self, bytes: &mut ByteBuffer, range: Range<usize>, inserted: &[u8]) -> Result<(), EditError> {
        if !range.is_empty() {
            let before = bytes.clone();
            bytes.replace(range.clone(), &[]).map_err(|error| EditError::Storage(error.to_string()))?;
            self.record_byte_edit(&before, bytes, range.start, range.len(), &[])?;
            self.record_bytes(&before, bytes, range.start, false)?;
        }
        if !inserted.is_empty() {
            let before = bytes.clone();
            bytes.replace(range.start..range.start, inserted).map_err(|error| EditError::Storage(error.to_string()))?;
            self.record_byte_edit(&before, bytes, range.start, 0, inserted)?;
            self.record_bytes(&before, bytes, range.start, true)?;
        }
        Ok(())
    }

    fn record_bytes(&mut self, before: &ByteBuffer, after: &ByteBuffer, start: usize, inserting: bool) -> Result<(), EditError> {
        let old_count = before.line_count();
        let new_count = after.line_count();
        if old_count == new_count { return Ok(()); }
        let old_row = before.line_index(start).map_err(|error| EditError::Storage(error.to_string()))?;
        let new_row = after.line_index(start.min(after.len())).map_err(|error| EditError::Storage(error.to_string()))?;
        let old_start = before.line_start(old_row).map_err(|error| EditError::Storage(error.to_string()))? == start;
        let new_start = after.line_start(new_row).map_err(|error| EditError::Storage(error.to_string()))? == start;
        let count = old_count.abs_diff(new_count);
        self.line_operations.push(if inserting {
            (MarkerOp::Insert { line: old_row + usize::from(!old_start), count }, MarkerOp::Delete { line: new_row + 1, count })
        } else {
            (MarkerOp::Delete { line: old_row + 1, count }, MarkerOp::Insert { line: new_row + usize::from(!new_start), count })
        });
        Ok(())
    }
    fn apply(&self, markers: &mut BTreeSet<usize>, undo: bool) {
        if undo {
            for (_, inverse) in self.line_operations.iter().rev() {
                inverse.apply(markers);
            }
        } else {
            for (forward, _) in &self.line_operations {
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
    change_history: Option<Arc<crate::change_history::History>>,
    change_history_epoch: Arc<()>,
    history_events: Arc<Mutex<Vec<crate::change_history::Event>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeHistorySpan {
    Insertion { range: Range<ByteOffset>, edition: u8 },
    Deletion { position: ByteOffset, editions: u8 },
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

fn merge_indicator_ranges(ranges: &mut Vec<Range<ByteOffset>>) {
    let mut written = 0;
    for read in 0..ranges.len() {
        let range = ranges[read].clone();
        if range.start >= range.end { continue; }
        if written > 0 && range.start <= ranges[written - 1].end {
            ranges[written - 1].end = ranges[written - 1].end.max(range.end);
        } else {
            ranges[written] = range;
            written += 1;
        }
    }
    ranges.truncate(written);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActualFirstLineEdit {
    pub modified_line: usize,
}

struct DocumentCore {
    regex_state: crate::search::RegexState,
    change_history: Option<crate::change_history::History>,
    change_history_epoch: Arc<()>,
    pending_save_history: Vec<Weak<Mutex<Vec<crate::change_history::Event>>>>,
    actual_first_line_edits: Vec<Weak<Mutex<Option<ActualFirstLineEdit>>>>,
    bookmarks: BTreeSet<usize>,
    indicators: [Vec<Range<ByteOffset>>; 7],
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
    begin_selection_positions: Vec<Weak<Mutex<isize>>>,
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
        Self::from_bytes(ByteBuffer::from_bytes(text.as_bytes()))
    }

    fn from_bytes(bytes: ByteBuffer) -> Self {
        Self {
            regex_state: crate::search::RegexState::default(),
            change_history: None,
            change_history_epoch: Arc::new(()),
            pending_save_history: Vec::new(),
            actual_first_line_edits: Vec::new(),
            views: Vec::new(),
            begin_selection_positions: Vec::new(),
            editing_view: Weak::new(),
            revision: Arc::new(()),
            composition_revision: Arc::new(()),
            direct_edit: false,
            may_coalesce: false,
            bookmarks: BTreeSet::new(),
            indicators: Default::default(),
            undo_depth: 0,
            undo_group_edited: false,
            identity: Arc::new(()),
            current: Snapshot::bytes(bytes, 0),
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

    pub fn toggle_bookmark(&mut self) -> Result<(), EditError> {
        let line = self.current.bytes.line_index(self.working_selection.head.0).map_err(|_| EditError::InvalidRange)?;
        self.toggle_bookmark_at(line);
        Ok(())
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
        let line = self.current.bytes.line_index(self.working_selection.head.0).map_err(|_| EditError::InvalidRange)?;
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
            change_history: self.change_history.clone().map(Arc::new),
            change_history_epoch: self.change_history_epoch.clone(),
            history_events,
        }
    }

    pub fn acknowledge_save(&mut self, snapshot: &SaveSnapshot) -> Result<(), EditError> {
        self.validate_save_snapshot(snapshot)?;
        let was_modified = self.is_modified();
        self.saved_version = snapshot.version;
        if Arc::ptr_eq(&self.change_history_epoch, &snapshot.change_history_epoch) {
            if snapshot.version == self.current.version {
                self.apply_change_event(crate::change_history::Event::Saved);
            } else if let Some(captured_history) = &snapshot.change_history {
                let mut history = captured_history.as_ref().clone();
                history.saved();
                for event in lock(&snapshot.history_events).iter().copied() {
                    event.replay_after_captured_save(&mut history);
                }
                self.change_history = Some(history);
                self.broadcast_change_event(crate::change_history::Event::Saved);
            }
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
        self.current.projection.utf16_position(position)
    }

    fn byte_from_utf16(&self, position: Utf16Offset) -> Result<ByteOffset, EditError> {
        self.current.projection.source_from_utf16(position)
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
        let Some(history) = &self.change_history else { return 0; };
        if line >= self.line_count() {
            return 0;
        }
        let bytes = &self.current.bytes;
        let Ok(start) = bytes.line_start(line) else { return 0; };
        let next = if line + 1 < self.line_count() {
            let Ok(next) = bytes.line_start(line + 1) else { return 0; };
            next
        } else { bytes.len() };
        let mut end = next;
        while end > start && matches!(bytes.byte(end - 1), Some(b'\r' | b'\n')) {
            end -= 1;
        }
        history.line_bits(start, next, end)
    }

    fn apply_change_event(&mut self, event: crate::change_history::Event) {
        self.apply_change_event_with_notifications(event, true, None);
    }

    fn apply_change_event_with_notifications(&mut self, event: crate::change_history::Event, notify: bool, modified_line: Option<usize>) {
        use crate::change_history::Event;
        let change = match event {
            Event::Insert(at, length)
            | Event::UndoDelete(at, length)
            | Event::RedoInsert(at, length) => Some((at, length, true)),
            Event::Delete(at, length)
            | Event::UndoInsert(at, length)
            | Event::RedoDelete(at, length) => Some((at, length, false)),
            _ => None,
        };
        if let Some((at, length, insert)) = change {
            if notify {
                if modified_line == Some(0) {
                    self.actual_first_line_edits.retain(|pending| {
                        if let Some(pending) = pending.upgrade() {
                            *lock(&pending) = Some(ActualFirstLineEdit { modified_line: 0 });
                            true
                        } else {
                            false
                        }
                    });
                }
                self.begin_selection_positions.retain(|position| position.strong_count() > 0);
                for position in &self.begin_selection_positions {
                    if let Some(position) = position.upgrade() {
                        let mut position = lock(&position);
                        if *position != -1 && (at as isize) < position.wrapping_sub(1) {
                            if insert { *position = position.wrapping_add(length as isize); }
                            else { *position = position.wrapping_sub(length as isize); }
                        }
                    }
                }
            }
            for ranges in &mut self.indicators {
                for range in ranges.iter_mut() {
                    if insert {
                        if at <= range.start.0 {
                            range.start.0 += length;
                            range.end.0 += length;
                        } else if at < range.end.0 {
                            range.end.0 += length;
                        }
                    } else {
                        for position in [&mut range.start, &mut range.end] {
                            position.0 = if position.0 >= at.saturating_add(length) {
                                position.0 - length
                            } else {
                                position.0.min(at)
                            };
                        }
                    }
                }
                merge_indicator_ranges(ranges);
            }
        }
        if let Some(history) = &mut self.change_history {
            event.apply(history);
            self.broadcast_change_event(event);
        }
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
                    action.line_edits.changes.last(),
                    edits
                        .changes
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
            grouped.line_edits.line_operations.append(&mut edits.line_operations);
            grouped.line_edits.changes.append(&mut edits.changes);
            grouped.line_edits.modified_lines.append(&mut edits.modified_lines);
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

    fn valid_byte_range(&self, range: Range<ByteOffset>) -> Result<Range<usize>, EditError> {
        if range.start > range.end || range.end.0 > self.current.bytes.len() {
            return Err(EditError::InvalidRange);
        }
        Ok(range.start.0..range.end.0)
    }

    pub fn select(&mut self, selection: Selection) -> Result<(), EditError> {
        self.valid_byte_range(selection.range())?;
        self.working_selection = selection;
        self.working_selection_changed = true;
        Ok(())
    }

    pub fn text_in(&self, range: Range<ByteOffset>) -> Result<String, EditError> {
        let range = self.valid_byte_range(range)?;
        let bytes = self.current.bytes.slice(range).map_err(|error| EditError::Storage(error.to_string()))?;
        Ok(String::from_utf8_lossy(&bytes.to_vec()).into_owned())
    }

    pub fn replace_selection(&mut self, text: &str) -> Result<(), EditError> {
        self.replace(self.working_selection.range(), text)
    }

    pub fn replace(&mut self, range: Range<ByteOffset>, text: &str) -> Result<(), EditError> {
        self.replace_bytes(range, text.as_bytes())
    }

    fn replace_bytes(&mut self, range: Range<ByteOffset>, inserted: &[u8]) -> Result<(), EditError> {
        if self.read_only { return Err(EditError::ReadOnly); }
        let range = self.valid_byte_range(range)?;
        if range.is_empty() && inserted.is_empty() { return Ok(()); }
        let mut bytes = self.current.bytes.clone();
        let mut edits = LineEdits::default();
        edits.change_bytes(&mut bytes, range.clone(), inserted)?;
        let caret = ByteOffset(range.start + inserted.len());
        self.commit_bytes(bytes, Selection { anchor: caret, head: caret }, edits)
    }

    fn commit_bytes(&mut self, bytes: ByteBuffer, selection: Selection, edits: LineEdits) -> Result<(), EditError> {
        if self.read_only { return Err(EditError::ReadOnly); }
        if selection.range().end.0 > bytes.len() { return Err(EditError::InvalidRange); }
        let next_version = self.next_version.checked_add(1).ok_or(EditError::VersionExhausted)?;
        let snapshot = Snapshot::bytes(bytes, self.next_version);
        let before_selections = self.capture_selections();
        for edit in &edits.changes {
            self.regex_state.edit(edit.start, edit.removed, edit.inserted);
        }
        self.transform_views(&edits, false, true);
        edits.apply(&mut self.bookmarks, false);
        edits.record_changes(self);
        let previous = std::mem::replace(&mut self.current, snapshot);
        self.notify_savepoint_transition(previous.version != self.saved_version);
        self.normalize_views();
        self.record_edit_history(previous, edits, before_selections);
        self.working_selection = selection;
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
            self.regex_state.invalidate();
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
            self.regex_state.invalidate();
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
                lock(&live).normalize_byte_bounds(&self.current.bytes);
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
                        for edit in edits.changes.iter().rev() {
                            edit.apply_range(range, true);
                        }
                    } else {
                        for edit in &edits.changes {
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
    actual_first_line_edit: Arc<Mutex<Option<ActualFirstLineEdit>>>,
    selection: Arc<Mutex<SelectionSet>>,
    occurrence_options: OccurrenceOptions,
    begin_selection: Option<(bool, Arc<Mutex<isize>>)>,
    tentative_selections: Option<SelectionSet>,
}

impl Document {
    pub(crate) fn regex_state(&self) -> crate::search::RegexState {
        lock(&self.core).regex_state.clone()
    }

    pub(crate) fn set_regex_state(&mut self, state: crate::search::RegexState) {
        lock(&self.core).regex_state = state;
    }

    pub fn new(text: &str) -> Self {
        Self::from_core(DocumentCore::new(text))
    }

    pub(crate) fn from_bytes(bytes: ByteBuffer) -> Self {
        Self::from_core(DocumentCore::from_bytes(bytes))
    }
    pub(crate) fn from_recovery(text: &str) -> Self {
        Self::from_core(DocumentCore::from_recovery(text))
    }
    fn from_core(core: DocumentCore) -> Self {
        let document = Self {
            core: Arc::new(Mutex::new(core)),
            actual_first_line_edit: Arc::new(Mutex::new(None)),
            selection: Arc::new(Mutex::new(SelectionSet::default())),
            occurrence_options: OccurrenceOptions::default(),
            begin_selection: None,
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
            actual_first_line_edit: Arc::new(Mutex::new(None)),
            selection,
            occurrence_options: self.occurrence_options,
            begin_selection: None,
            tentative_selections: None,
        }
    }
    pub fn shares_document_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.core, &other.core)
    }
    pub(crate) fn document_identity(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&lock(&self.core).identity)
    }
    pub fn set_actual_first_line_edit_notifications(&self, enabled: bool) {
        let mut core = lock(&self.core);
        let owner = Arc::downgrade(&self.actual_first_line_edit);
        core.actual_first_line_edits.retain(|pending| {
            pending.strong_count() > 0 && (enabled || !Weak::ptr_eq(pending, &owner))
        });
        if enabled {
            if !core.actual_first_line_edits.iter().any(|pending| Weak::ptr_eq(pending, &owner)) {
                core.actual_first_line_edits.push(owner);
            }
        } else {
            *lock(&self.actual_first_line_edit) = None;
        }
    }
    pub fn take_actual_first_line_edit(&self) -> Option<ActualFirstLineEdit> {
        lock(&self.actual_first_line_edit).take()
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
    pub fn set_indicators(
        &mut self,
        slot: usize,
        ranges: Vec<Range<ByteOffset>>,
        purge: bool,
    ) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        if slot >= core.indicators.len() {
            return Err(EditError::InvalidRange);
        }
        if ranges.iter().any(|range| {
            range.start > range.end
                || range.end.0 > core.current.bytes.len()
        }) {
            return Err(EditError::InvalidRange);
        }
        let values = &mut core.indicators[slot];
        if purge {
            values.clear();
        }
        values.extend(ranges.into_iter().filter(|range| range.start < range.end));
        values.sort_by_key(|range| range.start);
        merge_indicator_ranges(values);
        Ok(())
    }
    pub fn indicator_ranges(&self, slot: usize) -> Vec<Range<ByteOffset>> {
        lock(&self.core)
            .indicators
            .get(slot)
            .cloned()
            .unwrap_or_default()
    }
    pub fn begin_selection_mode(&self) -> Option<bool> {
        self.begin_selection.as_ref().filter(|(_, position)| *lock(position) != -1).map(|(column, _)| *column)
    }
    pub fn begin_or_end_selection(
        &mut self,
        column: bool,
        tab_width: usize,
    ) -> Result<(), EditError> {
        let head = self.selection().head;
        if let Some((_, anchor)) = self.begin_selection.take().filter(|(_, position)| *lock(position) != -1) {
            let anchor = *lock(&anchor);
            let anchor = if anchor < 0 {
                if column { ByteOffset(0) } else { head }
            } else {
                ByteOffset(usize::try_from(anchor).map_err(|_| EditError::InvalidRange)?.min(self.len_bytes()))
            };
            if column {
                let bytes = self.byte_snapshot();
                let position = |offset: ByteOffset| -> Result<ColumnPosition, EditError> {
                    Ok(ColumnPosition {
                        line: bytes.line_index(offset.0).map_err(|_| EditError::InvalidRange)?,
                        column: bytes.column(offset.0, tab_width).map_err(|_| EditError::InvalidRange)?,
                    })
                };
                self.select_rectangle(position(anchor)?, position(head)?, tab_width)
            } else {
                self.select(Selection { anchor, head })
            }
        } else {
            let anchor = Arc::new(Mutex::new(isize::try_from(head.0).map_err(|_| EditError::InvalidRange)?));
            lock(&self.core).begin_selection_positions.push(Arc::downgrade(&anchor));
            self.begin_selection = Some((column, anchor));
            Ok(())
        }
    }
    pub fn clear_indicators_in(
        &mut self,
        slot: usize,
        scope: Range<ByteOffset>,
    ) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        if scope.start > scope.end || scope.end.0 > core.current.bytes.len() {
            return Err(EditError::InvalidRange);
        }
        let values = core
            .indicators
            .get_mut(slot)
            .ok_or(EditError::InvalidRange)?;
        let mut retained = Vec::new();
        for range in values.drain(..) {
            if range.end <= scope.start || range.start >= scope.end {
                retained.push(range);
            } else {
                if range.start < scope.start {
                    retained.push(range.start..scope.start);
                }
                if range.end > scope.end {
                    retained.push(scope.end..range.end);
                }
            }
        }
        *values = retained;
        Ok(())
    }
    pub fn indicators_in(&self, scope: Range<usize>) -> Vec<(usize, Range<ByteOffset>)> {
        let core = lock(&self.core);
        let mut output = Vec::new();
        for (slot, ranges) in core.indicators.iter().enumerate() {
            let first = ranges.partition_point(|range| range.end.0 <= scope.start);
            output.extend(
                ranges[first..]
                    .iter()
                    .take_while(|range| range.start.0 < scope.end)
                    .map(|range| (slot, range.clone())),
            );
        }
        output
    }

    pub(crate) fn display_indicators_in(&self, scope: Range<usize>) -> Result<Vec<(usize, Range<ByteOffset>)>, EditError> {
        let projection = self.projection();
        let source = projection.source_selection(Selection { anchor: ByteOffset(scope.start), head: ByteOffset(scope.end) })?.range();
        self.indicators_in(source.start.0..source.end.0).into_iter().map(|(slot, range)| {
            projection.display_selection(Selection { anchor: range.start, head: range.end }).map(|selection| (slot, selection.range()))
        }).collect()
    }
    pub fn byte_snapshot(&self) -> ByteBuffer {
        lock(&self.core).current.bytes.clone()
    }

    pub(crate) fn projection(&self) -> ByteProjection {
        lock(&self.core).current.projection.clone()
    }

    pub(crate) fn display_selections(&self) -> SelectionSet {
        let core = lock(&self.core);
        let mut selections = lock(&self.selection).clone();
        for range in &mut selections.ranges {
            range.selection = core.current.projection.project_selection(range.selection);
        }
        selections
    }

    pub fn replace_bytes(&mut self, range: Range<ByteOffset>, inserted: &[u8]) -> Result<(), EditError> {
        self.edit(|core| core.replace_bytes(range, inserted))
    }

    pub fn replace_selection_bytes(&mut self, inserted: &[u8]) -> Result<(), EditError> {
        if self.is_read_only() { return Err(EditError::ReadOnly); }
        self.begin_undo_action();
        let selections = self.selections();
        let transforms: Vec<_> = selections.ranges().iter().map(|range| TextTransform {
            range: range.selection.range(), text: String::new(),
        }).collect();
        let result = self.direct_transforms(&transforms).and_then(|()| {
            let caret = self.selection().head;
            self.replace_bytes(caret..caret, inserted)?;
            let caret = self.selection().head;
            self.select(Selection { anchor: caret, head: caret })
        });
        self.end_undo_action();
        result
    }
    pub fn change_marker_bits(&self, line: usize) -> u8 {
        lock(&self.core).change_marker_bits(line)
    }
    pub fn set_change_history_capture(&self, enabled: bool) {
        let mut core = lock(&self.core);
        if enabled {
            if core.change_history.is_none() && !core.can_undo() {
                core.change_history = Some(crate::change_history::History::new(core.len_bytes()));
                core.change_history_epoch = Arc::new(());
                core.pending_save_history.clear();
            }
        } else if core.change_history.take().is_some() {
            core.change_history_epoch = Arc::new(());
            core.pending_save_history.clear();
        }
    }
    pub fn change_history_capture_enabled(&self) -> bool {
        lock(&self.core).change_history.is_some()
    }
    pub fn change_history_spans(&self, scope: Range<ByteOffset>) -> Result<Vec<ChangeHistorySpan>, EditError> {
        let core = lock(&self.core);
        if scope.start > scope.end || scope.end.0 > core.len_bytes() {
            return Err(EditError::InvalidRange);
        }
        let Some(history) = &core.change_history else { return Ok(Vec::new()); };
        let mut spans: Vec<_> = history.insertions_in(scope.start.0..scope.end.0).into_iter().map(|(range, edition)| {
            ChangeHistorySpan::Insertion { range: ByteOffset(range.start)..ByteOffset(range.end), edition }
        }).collect();
        spans.extend(history.deletions_in(scope.start.0..=scope.end.0).into_iter().map(|(position, editions)| {
            ChangeHistorySpan::Deletion { position: ByteOffset(position), editions }
        }));
        Ok(spans)
    }
    pub fn goto_change(&mut self, forward: bool) -> Result<bool, EditError> {
        let current = self.byte_snapshot().line_index(self.selection().head.0)
            .map_err(|error| EditError::Storage(error.to_string()))?;
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
    pub fn toggle_bookmark(&mut self) -> Result<(), EditError> {
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
        self.byte_snapshot().adjacent_position(position.0, forward).map(ByteOffset)
            .map_err(|error| EditError::Storage(error.to_string()))
    }
    pub fn selections(&self) -> SelectionSet {
        lock(&self.selection).clone()
    }
    pub fn set_selections(&mut self, selections: SelectionSet) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        for range in &selections.ranges {
            core.valid_byte_range(range.selection.range())?;
            for (position, virtual_space) in [
                (range.selection.anchor, range.anchor_virtual),
                (range.selection.head, range.head_virtual),
            ] {
                if virtual_space != 0 && !core.current.bytes.is_line_end(position.0) {
                    return Err(EditError::InvalidRange);
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
    pub(crate) fn commit_prepared_bytes(&mut self, bytes: ByteBuffer, mut selections: SelectionSet, edits: LineEdits) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        for range in selections.ranges() {
            if range.selection.range().end.0 > bytes.len() { return Err(EditError::InvalidRange); }
        }
        selections.normalize_byte_bounds(&bytes);
        core.editing_view = Arc::downgrade(&self.selection);
        if edits.changes.is_empty() {
            *lock(&self.selection) = selections;
        } else {
            if core.read_only { return Err(EditError::ReadOnly); }
            core.commit_bytes(bytes, selections.primary().selection, edits)?;
            *lock(&self.selection) = selections;
            core.remember_redo_selections();
        }
        self.tentative_selections = None;
        Ok(())
    }

    pub(crate) fn set_multi_selection_ranges(&mut self, ranges: &[SelectionRange]) {
        {
            let mut selections = lock(&self.selection);
            for (index, range) in ranges.iter().enumerate().take(selections.ranges.len()) {
                let current = &mut selections.ranges[index];
                current.set_start(range.selection.anchor);
                current.set_end(range.selection.head);
                if range.anchor_virtual != 0 { current.anchor_virtual = range.anchor_virtual; }
                if range.head_virtual != 0 { current.head_virtual = range.head_virtual; }
            }
        }
        let mut core = lock(&self.core);
        if !core.read_only { core.remember_redo_selections(); }
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
        if core.read_only { return Err(EditError::ReadOnly); }
        let mut order: Vec<_> = (0..transforms.len()).collect();
        order.sort_by_key(|&index| (transforms[index].range.start, transforms[index].range.end));
        let mut last_end = None;
        for &index in &order {
            let range = core.valid_byte_range(transforms[index].range.clone())?;
            if last_end.is_some_and(|end| range.start < end) { return Err(EditError::InvalidRange); }
            last_end = Some(range.end);
        }
        let mut bytes = core.current.bytes.clone();
        let mut edits = LineEdits::default();
        let mut selections = lock(&self.selection).clone();
        for &index in order.iter().rev() {
            let transform = &transforms[index];
            let range = transform.range.start.0..transform.range.end.0;
            if range.is_empty() && transform.text.is_empty() { continue; }
            edits.change_bytes(&mut bytes, range.clone(), transform.text.as_bytes())?;
            let mapping = TextEdit { start: range.start, removed: range.len(), inserted: transform.text.len() };
            for selected in &mut selections.ranges { mapping.apply_range(selected, false); }
            if let Some(replacements) = replacements {
                let target = replacements.get(index).copied().ok_or(EditError::InvalidRange)?;
                let selected = selections.ranges.get_mut(target).ok_or(EditError::InvalidRange)?;
                let caret = ByteOffset(range.start + transform.text.len());
                *selected = Selection { anchor: caret, head: caret }.into();
            }
        }
        if let Some(explicit) = explicit {
            if explicit.ranges().iter().any(|range| range.selection.range().end.0 > bytes.len()) { return Err(EditError::InvalidRange); }
            for range in explicit.ranges() {
                for (position, space) in [(range.selection.anchor, range.anchor_virtual), (range.selection.head, range.head_virtual)] {
                    if space != 0 && !bytes.is_line_end(position.0) { return Err(EditError::InvalidRange); }
                }
            }
            selections = explicit;
        } else { selections.deduplicate(); }
        selections.normalize_byte_bounds(&bytes);
        if edits.changes.is_empty() { *lock(&self.selection) = selections; return Ok(()); }
        core.editing_view = Arc::downgrade(&self.selection);
        core.commit_bytes(bytes, selections.primary().selection, edits)?;
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
        self.replace_evolving_bytes(text.as_bytes(), overwrite, direct)
    }
    fn replace_evolving_bytes(&mut self, inserted: &[u8], overwrite: bool, direct: bool) -> Result<(), EditError> {
        let mut core = lock(&self.core);
        if core.read_only { return Err(EditError::ReadOnly); }
        let mut bytes = core.current.bytes.clone();
        let mut selections = lock(&self.selection).clone();
        let mut edits = LineEdits::default();
        let mut order: Vec<_> = (0..selections.ranges().len()).collect();
        if direct {
            order.sort_by_key(|&index| {
                let range = selections.ranges()[index];
                (range.selection.head, range.head_virtual, range.selection.anchor, range.anchor_virtual)
            });
            order.reverse();
        }
        for index in order {
            let selected = selections.ranges()[index];
            let mut range = selected.selection.range();
            if range.end.0 > bytes.len() { return Err(EditError::InvalidRange); }
            let mut virtual_space = selected.start_virtual();
            if !selected.is_empty() {
                edits.change_bytes(&mut bytes, range.start.0..range.end.0, &[])?;
                selections.map_edit(range.start.0, range.end.0 - range.start.0, 0);
                if !range.is_empty() { virtual_space = 0; }
                range.end = range.start;
            } else if overwrite && virtual_space == 0 && range.start.0 < bytes.len()
                && !matches!(bytes.byte(range.start.0), Some(b'\r' | b'\n')) {
                let end = bytes.deletion_range(range.start.0, false).map_err(|error| EditError::Storage(error.to_string()))?.end;
                edits.change_bytes(&mut bytes, range.start.0..end, &[])?;
                selections.map_edit(range.start.0, end - range.start.0, 0);
            }
            let padding = vec![b' '; virtual_space];
            edits.change_bytes(&mut bytes, range.start.0..range.start.0, &padding)?;
            selections.map_edit(range.start.0, 0, padding.len());
            let insertion = range.start.0 + padding.len();
            edits.change_bytes(&mut bytes, insertion..insertion, inserted)?;
            selections.map_edit(insertion, 0, inserted.len());
            let caret = ByteOffset(insertion + inserted.len());
            selections.set_range(index, Selection { anchor: caret, head: caret }.into());
            selections.normalize_byte_bounds(&bytes);
        }
        if edits.changes.is_empty() { *lock(&self.selection) = selections; return Ok(()); }
        core.editing_view = Arc::downgrade(&self.selection);
        core.direct_edit = direct;
        let result = core.commit_bytes(bytes, selections.primary().selection, edits);
        core.direct_edit = false;
        result?;
        *lock(&self.selection) = selections;
        core.remember_redo_selections();
        Ok(())
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
        self.delete_byte_characters(backward, allow_line_start)
    }

    fn delete_byte_characters(&mut self, backward: bool, allow_line_start: bool) -> Result<(), EditError> {
        let mut bytes = self.byte_snapshot();
        let mut selections = self.selections();
        let any_selected = selections.ranges().iter().any(|range| !range.is_empty());
        let grouped = any_selected || selections.ranges().len() > 1
            || !backward && selections.primary().start_virtual() != 0;
        let mut edits = LineEdits::default();
        for index in 0..selections.ranges().len() {
            let selected = selections.ranges()[index];
            let mut range = selected.selection.range();
            if any_selected {
                if selected.is_empty() { continue; }
            } else if backward && selected.head_virtual != 0 {
                let selected = &mut selections.ranges[index];
                selected.anchor_virtual = selected.anchor_virtual.saturating_sub(1);
                selected.head_virtual = selected.head_virtual.saturating_sub(1);
                continue;
            } else if backward {
                let row = bytes.line_index(range.start.0).map_err(|error| EditError::Storage(error.to_string()))?;
                let start = bytes.line_start(row).map_err(|error| EditError::Storage(error.to_string()))?;
                if (!allow_line_start || selections.is_rectangular()) && start == range.start.0 { continue; }
                range.start = ByteOffset(bytes.deletion_range(range.start.0, true)
                    .map_err(|error| EditError::Storage(error.to_string()))?.start);
            } else {
                let padding = vec![b' '; selected.start_virtual()];
                edits.change_bytes(&mut bytes, range.start.0..range.start.0, &padding)?;
                selections.map_edit(range.start.0, 0, padding.len());
                range.start.0 += padding.len();
                range.end = range.start;
                if selections.ranges().len() == 1 || !matches!(bytes.byte(range.start.0), Some(b'\r' | b'\n')) {
                    range.end = ByteOffset(bytes.deletion_range(range.start.0, false)
                        .map_err(|error| EditError::Storage(error.to_string()))?.end);
                }
            }
            edits.change_bytes(&mut bytes, range.start.0..range.end.0, &[])?;
            selections.map_edit(range.start.0, range.end.0 - range.start.0, 0);
            let space = if any_selected && range.is_empty() { selected.start_virtual() } else { 0 };
            selections.set_range(index, SelectionRange {
                selection: Selection { anchor: range.start, head: range.start },
                anchor_virtual: space, head_virtual: space,
            });
        }
        selections.normalize_byte_bounds(&bytes);
        selections.deduplicate();
        if edits.changes.is_empty() { return self.set_selections(selections); }
        if grouped { self.begin_undo_action(); }
        let result = {
            let mut core = lock(&self.core);
            core.editing_view = Arc::downgrade(&self.selection);
            core.direct_edit = true;
            let result = core.commit_bytes(bytes, selections.primary().selection, edits);
            core.direct_edit = false;
            if result.is_ok() {
                *lock(&self.selection) = selections;
                core.remember_redo_selections();
            }
            result
        };
        if grouped { self.end_undo_action(); }
        result
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
