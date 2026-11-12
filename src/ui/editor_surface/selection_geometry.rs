use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionGeometry {
    pub anchor: usize,
    pub head: usize,
    pub anchor_virtual: usize,
    pub head_virtual: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowGeometry {
    pub start: usize,
    pub end: usize,
    pub last: bool,
    pub space_width: f32,
}

pub fn page_rows(
    row: usize,
    top: usize,
    total: usize,
    capacity: usize,
    down: bool,
) -> (usize, usize) {
    let distance = capacity.saturating_sub(1).max(1);
    let shift = |position: usize| {
        if down {
            position.saturating_add(distance)
        } else {
            position.saturating_sub(distance)
        }
    };
    (
        shift(row).min(total.saturating_sub(1)),
        shift(top).min(total.saturating_sub(capacity.max(1))),
    )
}

pub fn selection_at_point(
    ranges: &[SelectionGeometry],
    byte: usize,
    virtual_space: usize,
) -> Option<usize> {
    let point = (byte, virtual_space);
    ranges
        .iter()
        .position(|selected| {
            byte >= selected.anchor.min(selected.head) && byte < selected.anchor.max(selected.head)
        })
        .or_else(|| {
            ranges.iter().position(|selected| {
                (selected.anchor, selected.anchor_virtual) == (selected.head, selected.head_virtual)
                    && point == (selected.head, selected.head_virtual)
            })
        })
}

impl RowGeometry {
    pub fn hit(
        self,
        x: f32,
        closest: impl Fn(f32) -> usize,
        x_for_byte: impl Fn(usize) -> f32,
    ) -> (usize, usize) {
        let byte = closest(x).clamp(self.start, self.end);
        let virtual_space = if self.last && byte == self.end {
            ((x - x_for_byte(self.end)).max(0.) / self.space_width.max(1.)).round() as usize
        } else {
            0
        };
        (byte, virtual_space)
    }

    pub fn caret(self, selection: SelectionGeometry, x: impl Fn(usize) -> f32) -> Option<f32> {
        (selection.head >= self.start
            && (selection.head < self.end || (self.last && selection.head == self.end)))
            .then(|| x(selection.head) + selection.head_virtual as f32 * self.space_width)
    }

    pub fn highlight(
        self,
        selection: SelectionGeometry,
        x: impl Fn(usize) -> f32,
    ) -> Option<Range<f32>> {
        let anchor = (selection.anchor, selection.anchor_virtual);
        let head = (selection.head, selection.head_virtual);
        let (start, end) = if anchor <= head {
            (anchor, head)
        } else {
            (head, anchor)
        };
        if start == end
            || end.0 < self.start
            || start.0 > self.end
            || (end.0 == self.start && end.1 == 0)
            || (start.0 == self.end && !self.last)
        {
            return None;
        }
        let left = x(start.0.clamp(self.start, self.end))
            + if start.0 == self.end && self.last {
                start.1 as f32 * self.space_width
            } else {
                0.
            };
        let right = x(end.0.clamp(self.start, self.end))
            + if end.0 == self.end && self.last {
                end.1 as f32 * self.space_width
            } else if end.0 > self.end && self.last {
                (x(end.0) - x(self.end)).max(self.space_width)
            } else {
                0.
            };
        Some(left..right.max(left + 1.))
    }
}
