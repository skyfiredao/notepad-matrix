use super::{ByteOffset, EditError, Selection};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionRange {
    pub selection: Selection,
    pub anchor_virtual: usize,
    pub head_virtual: usize,
}

impl From<Selection> for SelectionRange {
    fn from(selection: Selection) -> Self {
        Self {
            selection,
            anchor_virtual: 0,
            head_virtual: 0,
        }
    }
}

impl SelectionRange {
    pub(crate) fn set_start(&mut self, position: ByteOffset) {
        if (self.selection.anchor, self.anchor_virtual) <= (self.selection.head, self.head_virtual) {
            self.selection.anchor = position;
            self.anchor_virtual = 0;
            if (self.selection.head, self.head_virtual) < (self.selection.anchor, self.anchor_virtual) {
                self.selection.head = position;
                self.head_virtual = 0;
            }
        } else {
            self.selection.head = position;
            self.head_virtual = 0;
            if (self.selection.anchor, self.anchor_virtual) < (self.selection.head, self.head_virtual) {
                self.selection.anchor = position;
                self.anchor_virtual = 0;
            }
        }
    }

    pub(crate) fn set_end(&mut self, position: ByteOffset) {
        if (self.selection.head, self.head_virtual) >= (self.selection.anchor, self.anchor_virtual) {
            self.selection.head = position;
            self.head_virtual = 0;
            if (self.selection.anchor, self.anchor_virtual) > (self.selection.head, self.head_virtual) {
                self.selection.anchor = position;
                self.anchor_virtual = 0;
            }
        } else {
            self.selection.anchor = position;
            self.anchor_virtual = 0;
            if (self.selection.head, self.head_virtual) > (self.selection.anchor, self.anchor_virtual) {
                self.selection.head = position;
                self.head_virtual = 0;
            }
        }
    }

    pub(crate) fn trim_against(&mut self, added: SelectionRange) -> bool {
        let anchor = (self.selection.anchor, self.anchor_virtual);
        let head = (self.selection.head, self.head_virtual);
        let mut start = anchor.min(head);
        let mut end = anchor.max(head);
        let added_anchor = (added.selection.anchor, added.anchor_virtual);
        let added_head = (added.selection.head, added.head_virtual);
        let added_start = added_anchor.min(added_head);
        let added_end = added_anchor.max(added_head);
        if added_start <= end && added_end >= start {
            if (start > added_start && end < added_end) || (start < added_start && end > added_end)
            {
                end = start;
            } else if start <= added_start {
                end = added_start;
            } else {
                start = added_end;
            }
            let (anchor, head) = if anchor > head {
                (end, start)
            } else {
                (start, end)
            };
            self.selection = Selection {
                anchor: anchor.0,
                head: head.0,
            };
            self.anchor_virtual = anchor.1;
            self.head_virtual = head.1;
            return self.is_empty();
        }
        false
    }

    pub fn is_empty(self) -> bool {
        self.selection.anchor == self.selection.head && self.anchor_virtual == self.head_virtual
    }

    pub fn start_virtual(self) -> usize {
        if self.selection.anchor < self.selection.head {
            self.anchor_virtual
        } else if self.selection.head < self.selection.anchor {
            self.head_virtual
        } else {
            self.anchor_virtual.min(self.head_virtual)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionSet {
    pub(super) ranges: Vec<SelectionRange>,
    pub(super) primary: usize,
    pub(super) rectangular: bool,
}

impl Default for SelectionSet {
    fn default() -> Self {
        Self::single(Selection::default())
    }
}

impl SelectionSet {
    pub fn single(selection: Selection) -> Self {
        Self {
            ranges: vec![selection.into()],
            primary: 0,
            rectangular: false,
        }
    }

    pub fn new(ranges: Vec<SelectionRange>, primary: usize) -> Result<Self, EditError> {
        if ranges.is_empty() || primary >= ranges.len() {
            return Err(EditError::InvalidRange);
        }
        Ok(Self {
            ranges,
            primary,
            rectangular: false,
        })
    }

    pub fn rectangle(ranges: Vec<SelectionRange>, primary: usize) -> Result<Self, EditError> {
        let mut selections = Self::new(ranges, primary)?;
        selections.rectangular = true;
        Ok(selections)
    }

    pub fn from_parts(
        ranges: Vec<SelectionRange>,
        primary: usize,
        rectangular: bool,
    ) -> Result<Self, EditError> {
        let mut selections = Self::new(ranges, primary)?;
        selections.rectangular = rectangular;
        Ok(selections)
    }

    pub fn ranges(&self) -> &[SelectionRange] {
        &self.ranges
    }
    pub fn primary_index(&self) -> usize {
        self.primary
    }
    pub fn primary(&self) -> SelectionRange {
        self.ranges[self.primary]
    }
    pub fn is_rectangular(&self) -> bool {
        self.rectangular
    }

    pub(crate) fn rectangular_endpoints(&self) -> Option<((ByteOffset, usize), (ByteOffset, usize))> {
        if !self.rectangular { return None; }
        let anchor_index = if self.primary == 0 { self.ranges.len() - 1 } else { 0 };
        let anchor = self.ranges[anchor_index];
        let caret = self.primary();
        Some(((anchor.selection.anchor, anchor.anchor_virtual), (caret.selection.head, caret.head_virtual)))
    }

    pub fn moved(
        &self,
        extend: bool,
        mut operation: impl FnMut(usize, SelectionRange) -> Result<SelectionRange, EditError>,
    ) -> Result<Self, EditError> {
        let mut moved = self.clone();
        for index in 0..moved.ranges.len() {
            let next = operation(index, moved.ranges[index])?;
            if extend {
                for other in 0..moved.ranges.len() {
                    if other != index {
                        moved.ranges[other].trim_against(next);
                    }
                }
            }
            moved.ranges[index] = next;
        }
        moved.deduplicate();
        Ok(moved)
    }

    pub(super) fn trim_primary(&mut self) {
        let range = self.primary();
        let mut index = 0;
        while index < self.ranges.len() {
            if index != self.primary && self.ranges[index].trim_against(range) {
                self.remove(index);
            } else {
                index += 1;
            }
        }
    }

    pub(super) fn remove(&mut self, index: usize) {
        if self.ranges.len() > 1 && index < self.ranges.len() {
            if self.primary >= index {
                self.primary = if self.primary == 0 {
                    self.ranges.len() - 2
                } else {
                    self.primary - 1
                };
            }
            self.ranges.remove(index);
        }
    }

    pub(super) fn add(&mut self, range: SelectionRange) {
        let mut i = 0;
        while i < self.ranges.len() {
            if i != self.primary && self.ranges[i].trim_against(range) {
                self.remove(i);
                continue;
            }
            i += 1;
        }
        self.ranges.push(range);
        self.primary = self.ranges.len() - 1;
        self.rectangular = false;
    }

    pub(super) fn deduplicate(&mut self) {
        let mut i = 0;
        while i < self.ranges.len() {
            let mut j = i + 1;
            while j < self.ranges.len() {
                if self.ranges[i].is_empty() && self.ranges[i] == self.ranges[j] {
                    self.remove(j);
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }

    pub(crate) fn map_edit(&mut self, start: usize, removed: usize, inserted: usize) {
        let edit = super::TextEdit {
            start,
            removed,
            inserted,
        };
        for range in &mut self.ranges {
            edit.apply_range(range, false);
        }
    }

    pub(crate) fn set_range(&mut self, index: usize, range: SelectionRange) {
        self.ranges[index] = range;
    }

    pub(crate) fn normalize_byte_bounds(&mut self, bytes: &crate::byte_buffer::ByteBuffer) {
        for range in &mut self.ranges {
            for (position, virtual_space) in [
                (&mut range.selection.anchor, &mut range.anchor_virtual),
                (&mut range.selection.head, &mut range.head_virtual),
            ] {
                position.0 = position.0.min(bytes.len());
                if !bytes.is_line_end(position.0) { *virtual_space = 0; }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnPosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextTransform {
    pub range: std::ops::Range<ByteOffset>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditSelectionPolicy {
    MapOriginal,
    Explicit(SelectionSet),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OccurrenceOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

pub(super) fn word(ch: char) -> bool {
    crate::unicode_class::classify(ch) == crate::unicode_class::CharacterClass::Word
}

impl super::Document {
    pub fn context_select(&mut self, position: ByteOffset) -> Result<(), EditError> {
        self.text_in(position..position)?;
        if !self.selections().ranges.iter().any(|range| {
            let range = range.selection.range();
            range.start <= position && position <= range.end
        }) {
            self.select(Selection {
                anchor: position,
                head: position,
            })?;
        }
        Ok(())
    }

    pub fn drop_selection(&mut self, position: ByteOffset, copy: bool) -> Result<bool, EditError> {
        self.drop_selection_with_options(position, copy, 4, "\r\n")
    }

    pub fn drop_selection_with_options(
        &mut self,
        position: ByteOffset,
        copy: bool,
        tab_width: usize,
        eol: &str,
    ) -> Result<bool, EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        self.text_in(position..position)?;
        let selections = self.selections();
        let main = selections.primary().selection.range();
        let inside = selections.ranges.iter().any(|range| {
            let range = range.selection.range();
            range.start <= position && position <= range.end
        });
        let edge = position == main.start || position == main.end;
        if inside && !(copy && edge) {
            self.select(Selection {
                anchor: position,
                head: position,
            })?;
            return Ok(false);
        }
        let mut core = super::lock(&self.core);
        let mut candidate = core.current.bytes.clone();
        let mut edits = super::LineEdits::default();
        let mut value = String::new();
        let mut ranges: Vec<_> = selections
            .ranges
            .iter()
            .map(|range| range.selection.range())
            .collect();
        ranges.sort_by_key(|range| range.start);
        let rectangular = ranges.len() > 1;
        let copied: Vec<_> = if selections.is_rectangular() {
            ranges.clone()
        } else {
            selections
                .ranges
                .iter()
                .map(|range| range.selection.range())
                .collect()
        };
        for range in &copied {
            value.push_str(&core.text_in(range.clone())?);
            if rectangular {
                value.push_str(eol);
            }
        }
        value = crate::clipboard::convert_line_endings(&value, eol);
        if value.is_empty() {
            return Ok(false);
        }
        let mut target = position.0;
        if !copy {
            for range in ranges.iter().rev() {
                edits.change_bytes(&mut candidate, range.start.0..range.end.0, &[])?;
                if position >= range.end {
                    target -= range.end.0 - range.start.0;
                }
            }
        }
        let selected = if rectangular {
            super::column::insert_rectangle(
                &mut candidate,
                &mut edits,
                ByteOffset(target),
                0,
                &value,
                tab_width,
                eol,
            )?;
            Selection {
                anchor: ByteOffset(target),
                head: ByteOffset(target),
            }
        } else {
            edits.change_bytes(&mut candidate, target..target, value.as_bytes())?;
            Selection {
                anchor: ByteOffset(target),
                head: ByteOffset(target + value.len()),
            }
        };
        core.editing_view = std::sync::Arc::downgrade(&self.selection);
        core.commit_bytes(candidate, selected, edits)?;
        *super::lock(&self.selection) = SelectionSet::single(selected);
        core.remember_redo_selections();
        Ok(true)
    }

    pub fn remove_last_selection(&mut self) {
        let length = self.selections().ranges.len();
        self.remove_selection(length - 1);
    }

    pub fn select_next_occurrence(
        &mut self,
        options: OccurrenceOptions,
    ) -> Result<bool, EditError> {
        self.select_occurrences(options, false)
    }

    pub fn select_all_occurrences(
        &mut self,
        options: OccurrenceOptions,
    ) -> Result<bool, EditError> {
        let expanded = self.selection().range().is_empty();
        if expanded {
            let caret = self.selection();
            self.select(caret)?;
            if !self.select_occurrences(options, false)? {
                return Ok(false);
            }
        }
        self.select_occurrences(options, true)
            .map(|changed| changed || expanded)
    }

    pub fn skip_occurrence(&mut self) -> Result<bool, EditError> {
        let changed = self.select_next_occurrence(self.occurrence_options)?;
        let count = self.selections().ranges.len();
        if count > 1 {
            self.remove_selection(count - 2);
        }
        Ok(changed)
    }

    fn select_occurrences(
        &mut self,
        options: OccurrenceOptions,
        each: bool,
    ) -> Result<bool, EditError> {
        self.occurrence_options = options;
        let bytes = self.byte_snapshot();
        let selected = self.selection().range();
        if self
            .selections()
            .ranges
            .iter()
            .all(|range| range.is_empty())
        {
            let start = bytes.extend_word(self.selection().head.0, false)
                .map_err(|error| EditError::Storage(error.to_string()))?;
            let end = bytes.extend_word(start, true).map_err(|error| EditError::Storage(error.to_string()))?;
            let mut selections = self.selections();
            selections.set_range(selections.primary_index(), Selection {
                anchor: ByteOffset(start),
                head: ByteOffset(end),
            }.into());
            selections.rectangular = false;
            selections.trim_primary();
            self.set_selections(selections)?;
            return Ok(start != end);
        }
        if selected.is_empty() {
            return Ok(false);
        }
        let query = bytes.slice(selected.start.0..selected.end.0)
            .map_err(|error| EditError::Storage(error.to_string()))?.to_vec();
        let mut changed = false;
        for scope in [selected.end.0..bytes.len(), 0..selected.start.0] {
            let mut start = scope.start;
            while let Some(range) = bytes.find_literal_bytes(
                &query, start, scope.end, options.match_case, options.whole_word, false,
            ).map_err(|error| EditError::Storage(error.to_string()))? {
                self.add_selection(Selection {
                    anchor: ByteOffset(range.start), head: ByteOffset(range.end),
                }.into())?;
                changed = true;
                if !each { return Ok(true); }
                start = range.end;
            }
        }
        Ok(changed)
    }
}
