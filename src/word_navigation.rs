use super::{ByteOffset, Document, EditError, Selection, SelectionRange, SelectionSet};
use ropey::{LineType, Rope};

#[path = "unicode_class.rs"]
mod unicode_class;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordBoundary {
    Start,
    End,
    Part,
    Line,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharacterClass {
    Space,
    NewLine,
    Word,
    Punctuation,
}

fn class(ch: char) -> CharacterClass {
    if super::selection::word(ch) {
        CharacterClass::Word
    } else if matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}') {
        CharacterClass::NewLine
    } else if unicode_class::is_punctuation(ch) {
        CharacterClass::Punctuation
    } else {
        CharacterClass::Space
    }
}

pub(crate) fn whole_word_at(text: &str, start: usize, end: usize) -> bool {
    if start >= end {
        return false;
    }
    let (Some(before), Some(matched), Some(after)) =
        (text.get(..start), text.get(start..end), text.get(end..))
    else {
        return false;
    };
    let first = matched
        .chars()
        .next()
        .map(class)
        .unwrap_or(CharacterClass::Space);
    let last = matched
        .chars()
        .next_back()
        .map(class)
        .unwrap_or(CharacterClass::Space);
    let previous = before
        .chars()
        .next_back()
        .map(class)
        .unwrap_or(CharacterClass::Space);
    let next = after
        .chars()
        .next()
        .map(class)
        .unwrap_or(CharacterClass::Space);
    matches!(first, CharacterClass::Word | CharacterClass::Punctuation)
        && matches!(last, CharacterClass::Word | CharacterClass::Punctuation)
        && first != previous
        && last != next
}

fn previous(text: &Rope, position: usize) -> usize {
    let mut previous = position.saturating_sub(1);
    while !text.is_char_boundary(previous) {
        previous -= 1;
    }
    previous
}

fn line_end(text: &Rope, position: usize) -> bool {
    position == text.len() || matches!(text.char(position), '\r' | '\n')
}

pub(crate) fn selection_in_snapshot(
    text: &Rope,
    position: ByteOffset,
) -> Result<Selection, EditError> {
    if position.0 > text.len() {
        return Err(EditError::InvalidRange);
    }
    if !text.is_char_boundary(position.0) {
        return Err(EditError::InvalidBoundary);
    }
    let mut at = position.0;
    if at > 0 && at < text.len() && text.byte(at) == b'\n' && text.byte(at - 1) == b'\r' {
        at -= 1;
    }
    if line_end(text, at) {
        let line = text.byte_to_line_idx(at, LineType::LF_CR);
        if at == text.line_to_byte_idx(line, LineType::LF_CR) {
            return Ok(Selection {
                anchor: ByteOffset(at),
                head: ByteOffset(at),
            });
        }
        at = previous(text, at);
    }
    let initial = class(text.char(at));
    let mut start = at;
    while start > 0 && class(text.char(previous(text, start))) == initial {
        start = previous(text, start);
    }
    let mut end = at;
    while end < text.len() && class(text.char(end)) == initial {
        end += text.char(end).len_utf8();
    }
    Ok(Selection {
        anchor: ByteOffset(start),
        head: ByteOffset(end),
    })
}

pub(crate) fn word_range_in_snapshot(
    text: &Rope,
    position: ByteOffset,
) -> Result<Selection, EditError> {
    if position.0 > text.len() {
        return Err(EditError::InvalidRange);
    }
    if !text.is_char_boundary(position.0) {
        return Err(EditError::InvalidBoundary);
    }
    let mut start = position.0;
    let mut end = position.0;
    while start > 0 && super::selection::word(text.char(previous(text, start))) {
        start = previous(text, start);
    }
    while end < text.len() && super::selection::word(text.char(end)) {
        end += text.char(end).len_utf8();
    }
    Ok(Selection {
        anchor: ByteOffset(start),
        head: ByteOffset(end),
    })
}

pub(crate) fn extend_selection_in_snapshot(
    text: &Rope,
    anchored: Selection,
    initial: ByteOffset,
    position: ByteOffset,
) -> Result<Selection, EditError> {
    boundary_in_snapshot(text, position, false, WordBoundary::Line)?;
    if position < anchored.anchor {
        Ok(Selection {
            anchor: anchored.head,
            head: if line_end(text, position.0) {
                position
            } else {
                selection_in_snapshot(text, position)?.anchor
            },
        })
    } else if position > anchored.head {
        let line = text.byte_to_line_idx(position.0, LineType::LF_CR);
        Ok(Selection {
            anchor: anchored.anchor,
            head: if position.0 > text.line_to_byte_idx(line, LineType::LF_CR) {
                selection_in_snapshot(text, ByteOffset(previous(text, position.0)))?.head
            } else {
                position
            },
        })
    } else if position >= initial {
        Ok(anchored)
    } else {
        Ok(Selection {
            anchor: anchored.head,
            head: anchored.anchor,
        })
    }
}

pub(crate) fn boundary_in_snapshot(
    text: &Rope,
    position: ByteOffset,
    forward: bool,
    boundary: WordBoundary,
) -> Result<ByteOffset, EditError> {
    if position.0 > text.len() {
        return Err(EditError::InvalidRange);
    }
    if !text.is_char_boundary(position.0) {
        return Err(EditError::InvalidBoundary);
    }
    let mut at = position.0;
    match (forward, boundary) {
        (_, WordBoundary::Line) => {
            let line = text.byte_to_line_idx(at, LineType::LF_CR);
            let start = text.line_to_byte_idx(line, LineType::LF_CR);
            at = if forward {
                start
                    + text
                        .line(line, LineType::LF_CR)
                        .to_string()
                        .trim_end_matches(['\r', '\n'])
                        .len()
            } else {
                start
            };
        }
        (_, WordBoundary::Part) => {
            at = word_part(text, at, forward);
        }
        (false, WordBoundary::Start) => {
            while at > 0 && class(text.char(previous(text, at))) == CharacterClass::Space {
                at = previous(text, at);
            }
            if at > 0 {
                let initial = class(text.char(previous(text, at)));
                while at > 0 && class(text.char(previous(text, at))) == initial {
                    at = previous(text, at);
                }
            }
        }
        (true, WordBoundary::Start) => {
            if at < text.len() {
                let initial = class(text.char(at));
                while at < text.len() && class(text.char(at)) == initial {
                    at += text.char(at).len_utf8();
                }
            }
            while at < text.len() && class(text.char(at)) == CharacterClass::Space {
                at += text.char(at).len_utf8();
            }
        }
        (false, WordBoundary::End) => {
            if at > 0 {
                let initial = class(text.char(previous(text, at)));
                if initial != CharacterClass::Space {
                    while at > 0 && class(text.char(previous(text, at))) == initial {
                        at = previous(text, at);
                    }
                }
            }
            while at > 0 && class(text.char(previous(text, at))) == CharacterClass::Space {
                at = previous(text, at);
            }
        }
        (true, WordBoundary::End) => {
            while at < text.len() && class(text.char(at)) == CharacterClass::Space {
                at += text.char(at).len_utf8();
            }
            if at < text.len() {
                let initial = class(text.char(at));
                while at < text.len() && class(text.char(at)) == initial {
                    at += text.char(at).len_utf8();
                }
            }
        }
    }
    Ok(ByteOffset(at))
}

fn word_part(text: &Rope, mut at: usize, forward: bool) -> usize {
    let after = |position| {
        if position < text.len() {
            text.char(position)
        } else {
            '\0'
        }
    };
    let separator = |ch| super::selection::word(ch) && ch.is_ascii_punctuation();
    let step = |at| at + after(at).len_utf8();
    if forward {
        if separator(after(at)) {
            while at < text.len() && separator(after(at)) {
                at = step(at);
            }
        }
        let initial = after(at);
        if !initial.is_ascii() {
            while at < text.len() && !after(at).is_ascii() {
                at = step(at);
            }
        } else if initial.is_ascii_lowercase() {
            while at < text.len() && after(at).is_ascii_lowercase() {
                at = step(at);
            }
        } else if initial.is_ascii_uppercase() {
            if after(step(at)).is_ascii_lowercase() {
                at = step(at);
                while at < text.len() && after(at).is_ascii_lowercase() {
                    at = step(at);
                }
            } else {
                while at < text.len() && after(at).is_ascii_uppercase() {
                    at = step(at);
                }
            }
            if after(at).is_ascii_lowercase()
                && at > 0
                && after(previous(text, at)).is_ascii_uppercase()
            {
                at = previous(text, at);
            }
        } else if initial.is_ascii_digit() {
            while at < text.len() && after(at).is_ascii_digit() {
                at = step(at);
            }
        } else if initial.is_ascii_punctuation() {
            while at < text.len() && after(at).is_ascii_punctuation() {
                at = step(at);
            }
        } else if initial.is_ascii_whitespace() {
            while at < text.len() && after(at).is_ascii_whitespace() {
                at = step(at);
            }
        } else if at < text.len() {
            at = step(at);
        }
    } else if at > 0 {
        at = previous(text, at);
        if separator(after(at)) {
            while at > 0 && separator(after(at)) {
                at = previous(text, at);
            }
        }
        if at > 0 {
            let initial = after(at);
            at = previous(text, at);
            if initial.is_ascii_lowercase() {
                while at > 0 && after(at).is_ascii_lowercase() {
                    at = previous(text, at);
                }
                if !after(at).is_ascii_uppercase() && !after(at).is_ascii_lowercase() {
                    at = step(at);
                }
            } else if initial.is_ascii_uppercase() {
                while at > 0 && after(at).is_ascii_uppercase() {
                    at = previous(text, at);
                }
                if !after(at).is_ascii_uppercase() {
                    at = step(at);
                }
            } else if initial.is_ascii_digit() {
                while at > 0 && after(at).is_ascii_digit() {
                    at = previous(text, at);
                }
                if !after(at).is_ascii_digit() {
                    at = step(at);
                }
            } else if initial.is_ascii_punctuation() {
                while at > 0 && after(at).is_ascii_punctuation() {
                    at = previous(text, at);
                }
                if !after(at).is_ascii_punctuation() {
                    at = step(at);
                }
            } else if initial.is_ascii_whitespace() {
                while at > 0 && after(at).is_ascii_whitespace() {
                    at = previous(text, at);
                }
                if !after(at).is_ascii_whitespace() {
                    at = step(at);
                }
            } else if !initial.is_ascii() {
                while at > 0 && !after(at).is_ascii() {
                    at = previous(text, at);
                }
                if after(at).is_ascii() {
                    at = step(at);
                }
            } else {
                at = step(at);
            }
        }
    }
    at
}

impl Document {
    pub fn delete_word(&mut self, forward: bool, to_line_edge: bool) -> Result<(), EditError> {
        self.delete_word_boundary(
            forward,
            if to_line_edge {
                WordBoundary::Line
            } else {
                WordBoundary::Start
            },
        )
    }

    pub fn delete_word_end(&mut self) -> Result<(), EditError> {
        self.delete_word_boundary(true, WordBoundary::End)
    }

    fn delete_word_boundary(
        &mut self,
        forward: bool,
        boundary: WordBoundary,
    ) -> Result<(), EditError> {
        if self.is_read_only() {
            return Err(EditError::ReadOnly);
        }
        let mut candidate = self.text_snapshot();
        let mut selections = self.selections();
        let mut edits = super::LineEdits::default();
        for index in 0..selections.ranges().len() {
            let old = selections.ranges()[index];
            let mut position = old.selection.head;
            if forward {
                let padding = " ".repeat(old.head_virtual);
                edits.insert(&mut candidate, position.0, &padding)?;
                selections.map_edit(position.0, 0, padding.len());
                position.0 += padding.len();
                selections.set_range(
                    index,
                    Selection {
                        anchor: position,
                        head: position,
                    }
                    .into(),
                );
            } else {
                selections.set_range(
                    index,
                    SelectionRange {
                        selection: old.selection,
                        anchor_virtual: 0,
                        head_virtual: 0,
                    },
                );
            }
            let target = boundary_in_snapshot(&candidate, position, forward, boundary)?;
            let range = if forward {
                position.0..target.0.max(position.0)
            } else {
                target.0.min(position.0)..position.0
            };
            edits.remove(&mut candidate, range.clone())?;
            selections.map_edit(range.start, range.len(), 0);
        }
        selections.normalize_boundaries(&candidate, false);
        selections.deduplicate();
        let grouped = forward || self.selections().ranges().len() > 1;
        if grouped {
            self.begin_undo_action();
        }
        super::lock(&self.core).direct_edit = !grouped;
        let result = self.commit_prepared(candidate, selections, edits);
        super::lock(&self.core).direct_edit = false;
        if grouped {
            self.end_undo_action();
        }
        result
    }

    pub fn word_boundary(
        &self,
        position: ByteOffset,
        forward: bool,
        boundary: WordBoundary,
    ) -> Result<ByteOffset, EditError> {
        boundary_in_snapshot(&self.text_snapshot(), position, forward, boundary)
    }

    pub fn move_word(&mut self, forward: bool, extend: bool) -> Result<(), EditError> {
        self.move_word_boundary(
            forward,
            extend,
            if forward && extend {
                WordBoundary::End
            } else {
                WordBoundary::Start
            },
        )
    }

    pub fn move_word_boundary(
        &mut self,
        forward: bool,
        extend: bool,
        boundary: WordBoundary,
    ) -> Result<(), EditError> {
        let selections = move_in_snapshot(
            &self.text_snapshot(),
            &self.selections(),
            forward,
            extend,
            boundary,
        )?;
        self.set_selections(selections)
    }

    pub(crate) fn break_undo_sequence(&mut self) {
        super::lock(&self.core).may_coalesce = false;
    }
}

pub(crate) fn move_in_snapshot(
    text: &Rope,
    selections: &SelectionSet,
    forward: bool,
    extend: bool,
    boundary: WordBoundary,
) -> Result<SelectionSet, EditError> {
    if selections.is_rectangular() {
        let mut positions = selections.ranges().iter().flat_map(|range| {
            [
                (range.selection.anchor, range.anchor_virtual),
                (range.selection.head, range.head_virtual),
            ]
        });
        let first = positions.next().ok_or(EditError::InvalidRange)?;
        let target = positions.fold(first, |target, position| {
            if forward {
                target.max(position)
            } else {
                target.min(position)
            }
        });
        return SelectionSet::new(
            vec![SelectionRange {
                selection: Selection {
                    anchor: target.0,
                    head: target.0,
                },
                anchor_virtual: target.1,
                head_virtual: target.1,
            }],
            0,
        );
    }
    selections.moved(extend, |_, old| {
        let head = boundary_in_snapshot(text, old.selection.head, forward, boundary)?;
        Ok(SelectionRange {
            selection: Selection {
                anchor: if extend { old.selection.anchor } else { head },
                head,
            },
            anchor_virtual: if extend { old.anchor_virtual } else { 0 },
            head_virtual: 0,
        })
    })
}
