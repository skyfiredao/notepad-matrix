use std::ops::Range;
use std::sync::Arc;

use ropey::Rope;

use crate::byte_buffer::ByteBuffer;
use crate::editor::{ByteOffset, EditError, Selection, Utf16Offset};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DisplayByteOffset(pub usize);

#[derive(Clone, Copy)]
pub(crate) enum Affinity {
    Before,
    After,
}

#[derive(Clone)]
struct ChangedSpan {
    source: Range<usize>,
    display: Range<usize>,
}

#[derive(Clone)]
pub(crate) struct ByteProjection {
    text: Rope,
    source_length: usize,
    changed: Arc<Vec<ChangedSpan>>,
}

impl ByteProjection {
    pub(crate) fn from_bytes(bytes: &ByteBuffer) -> Self {
        let mut text = String::new();
        let mut changed = Vec::new();
        for unit in bytes.units() {
            match unit.value {
                crate::byte_text::ByteValue::Scalar(ch) => text.push(ch),
                crate::byte_text::ByteValue::Invalid(_) => {
                    let display_start = text.len();
                    text.push('\u{fffd}');
                    changed.push(ChangedSpan { source: unit.range, display: display_start..text.len() });
                }
            }
        }
        Self { text: Rope::from_str(&text), source_length: bytes.len(), changed: Arc::new(changed) }
    }

    pub(crate) fn text(&self) -> Rope {
        self.text.clone()
    }

    pub(crate) fn display_position(&self, source: ByteOffset, affinity: Affinity) -> Result<DisplayByteOffset, EditError> {
        if source.0 > self.source_length { return Err(EditError::InvalidRange); }
        Ok(self.bounded_display_position(source, affinity))
    }

    fn bounded_display_position(&self, source: ByteOffset, affinity: Affinity) -> DisplayByteOffset {
        let source = ByteOffset(source.0.min(self.source_length));
        let index = self.changed.partition_point(|span| span.source.end <= source.0);
        let delta = index.checked_sub(1).map_or(0, |at| self.changed[at].display.end - self.changed[at].source.end);
        let mut display = source.0 + delta;
        while !self.text.is_char_boundary(display) {
            match affinity { Affinity::Before => display -= 1, Affinity::After => display += 1 }
        }
        DisplayByteOffset(display)
    }

    pub(crate) fn source_position(&self, display: DisplayByteOffset, affinity: Affinity) -> Result<ByteOffset, EditError> {
        if display.0 > self.text.len() { return Err(EditError::InvalidRange); }
        let index = self.changed.partition_point(|span| span.display.end <= display.0);
        if let Some(span) = self.changed.get(index)
            && span.display.start < display.0 && display.0 < span.display.end {
            return Ok(ByteOffset(match affinity { Affinity::Before => span.source.start, Affinity::After => span.source.end }));
        }
        let delta = index.checked_sub(1).map_or(0, |at| self.changed[at].display.end - self.changed[at].source.end);
        Ok(ByteOffset(display.0 - delta))
    }

    pub(crate) fn display_selection(&self, source: Selection) -> Result<Selection, EditError> {
        if source.range().end.0 > self.source_length { return Err(EditError::InvalidRange); }
        Ok(self.project_selection(source))
    }

    pub(crate) fn project_selection(&self, source: Selection) -> Selection {
        Selection {
            anchor: ByteOffset(self.bounded_display_position(source.anchor, Affinity::After).0),
            head: ByteOffset(self.bounded_display_position(source.head, Affinity::After).0),
        }
    }

    pub(crate) fn source_selection(&self, display: Selection) -> Result<Selection, EditError> {
        Ok(Selection {
            anchor: self.source_position(DisplayByteOffset(display.anchor.0), Affinity::Before)?,
            head: self.source_position(DisplayByteOffset(display.head.0), Affinity::Before)?,
        })
    }

    pub(crate) fn utf16_position(&self, source: ByteOffset) -> Result<Utf16Offset, EditError> {
        let display = self.display_position(source, Affinity::After)?;
        Ok(Utf16Offset(self.text.byte_to_utf16_idx(display.0)))
    }

    pub(crate) fn source_from_utf16(&self, position: Utf16Offset) -> Result<ByteOffset, EditError> {
        if position.0 > self.text.len_utf16() { return Err(EditError::InvalidRange); }
        let display = self.text.utf16_to_byte_idx(position.0);
        if self.text.byte_to_utf16_idx(display) != position.0 { return Err(EditError::InvalidBoundary); }
        self.source_position(DisplayByteOffset(display), Affinity::Before)
    }
}
