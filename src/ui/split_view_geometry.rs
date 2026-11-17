#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Viewport {
    pub top: usize,
    pub horizontal: f32,
    pub p_width: f32,
}

impl Viewport {
    pub fn new(top: usize, horizontal: f32, p_width: f32) -> Self {
        Self {
            top,
            horizontal,
            p_width,
        }
    }

    fn column(self) -> i64 {
        (self.horizontal / self.p_width.max(1.)).trunc() as i64
    }
}

#[derive(Default)]
pub(super) struct SyncScroll {
    pub vertical: bool,
    pub horizontal: bool,
    line_difference: i64,
    column_difference: i64,
}

impl SyncScroll {
    pub fn toggle_vertical(&mut self, visible: bool, main: Viewport, sub: Viewport) {
        self.vertical = visible && !self.vertical;
        if self.vertical {
            self.line_difference = main.top as i64 - sub.top as i64;
        }
    }

    pub fn toggle_horizontal(&mut self, visible: bool, main: Viewport, sub: Viewport) {
        self.horizontal = visible && !self.horizontal;
        if self.horizontal {
            self.column_difference = main.column() - sub.column();
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn target(&self, source_main: bool, source: Viewport, other: Viewport) -> Viewport {
        let sign = if source_main { -1 } else { 1 };
        Viewport {
            top: if self.vertical {
                (source.top as i64 + sign * self.line_difference).max(0) as usize
            } else {
                other.top
            },
            horizontal: if self.horizontal {
                let columns = source.column() + sign * self.column_difference - other.column();
                (other.horizontal + columns as f32 * other.p_width).max(0.)
            } else {
                other.horizontal
            },
            ..other
        }
    }
}

