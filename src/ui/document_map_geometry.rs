#[derive(Clone, Copy, Debug)]
pub struct MapGeometry {
    total_lines: usize,
    visible_lines: usize,
    top_line: usize,
    first_line: usize,
    rows: usize,
    line_height: f32,
}

impl MapGeometry {
    pub fn new(
        total_lines: usize,
        visible_lines: usize,
        top_line: usize,
        height: f32,
    ) -> Option<Self> {
        if !height.is_finite() || height <= 0. {
            return None;
        }
        let total_lines = total_lines.max(1);
        let visible_lines = visible_lines.clamp(1, total_lines);
        let top_line = top_line.min(total_lines - visible_lines);
        let line_height = 3_f32.min(height / visible_lines as f32);
        if line_height <= 0. {
            return None;
        }
        let rows = ((height / line_height).floor() as usize).clamp(visible_lines, total_lines);
        let max_top = total_lines - visible_lines;
        let first_line = if max_top == 0 {
            0
        } else {
            (top_line as f64 / max_top as f64 * (total_lines - rows) as f64).round() as usize
        };
        Some(Self {
            total_lines,
            visible_lines,
            top_line,
            first_line,
            rows,
            line_height,
        })
    }

    pub fn first_line(self) -> usize {
        self.first_line
    }
    pub fn rows(self) -> usize {
        self.rows
    }
    pub fn line_height(self) -> f32 {
        self.line_height
    }

    pub fn viewport(self) -> (f32, f32) {
        (
            (self.top_line - self.first_line) as f32 * self.line_height,
            self.visible_lines as f32 * self.line_height,
        )
    }

    pub fn top_line_at(self, y: f32) -> usize {
        if !y.is_finite() {
            return self.top_line;
        }
        let (top, height) = self.viewport();
        let delta = ((y - top - height / 2.) / self.line_height).trunc() as f64;
        (self.top_line as f64 + delta).clamp(0., (self.total_lines - self.visible_lines) as f64)
            as usize
    }
}

pub fn miniature_text(chars: impl IntoIterator<Item = char>, max_columns: usize) -> String {
    let mut text = String::new();
    let mut column = 0;
    for ch in chars {
        if column >= max_columns || matches!(ch, '\r' | '\n') {
            break;
        }
        if ch == '\t' {
            let spaces = (4 - column % 4).min(max_columns - column);
            text.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            text.push(if ch.is_control() { '�' } else { ch });
            column += 1;
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_document_keeps_miniature_scale_and_click_centers_the_viewport() {
        let map = MapGeometry::new(100, 20, 10, 600.).expect("positive height");
        assert_eq!(map.first_line(), 0);
        assert_eq!(map.rows(), 100);
        assert_eq!(map.line_height(), 3.);
        assert_eq!(map.viewport(), (30., 60.));
        assert_eq!(map.top_line_at(150.), 40);
        assert_eq!(map.top_line_at(-20.), 0);
        assert_eq!(map.top_line_at(600.), 80);
    }

    #[test]
    fn long_document_follows_editor_and_keeps_viewport_inside_the_map() {
        let map = MapGeometry::new(1_000, 20, 490, 300.).expect("positive height");
        assert_eq!(map.first_line(), 450);
        assert_eq!(map.rows(), 100);
        assert_eq!(map.viewport(), (120., 60.));
        assert_eq!(map.top_line_at(150.), 490);
        assert_eq!(map.top_line_at(180.), 500);
        let end = MapGeometry::new(1_000, 20, usize::MAX, 300.).expect("positive height");
        assert_eq!(end.first_line(), 900);
        assert_eq!(end.viewport(), (240., 60.));
        assert_eq!(end.top_line_at(300.), 980);
    }

    #[test]
    fn empty_document_and_tiny_panels_have_bounded_geometry() {
        let empty = MapGeometry::new(0, 0, 100, 200.).expect("empty editor still has a line");
        assert_eq!(empty.viewport(), (0., 3.));
        assert_eq!(empty.top_line_at(100.), 0);
        let tiny = MapGeometry::new(100, 20, 40, 2.).expect("positive height");
        assert_eq!(tiny.viewport(), (0., 2.));
        assert_eq!(tiny.rows(), 20);
        assert_eq!(tiny.top_line_at(f32::NAN), 40);
        assert!(MapGeometry::new(100, 20, 0, 0.).is_none());
        assert!(MapGeometry::new(100, 20, 0, f32::INFINITY).is_none());
    }

    #[test]
    fn preview_preserves_real_unicode_text_and_tab_columns_with_bounded_work() {
        assert_eq!(miniature_text("a\t中\r\n".chars(), 20), "a   中");
        assert_eq!(miniature_text("\tαβtail".chars(), 6), "    αβ");
        assert_eq!(miniature_text("a\0b".chars(), 20), "a�b");
        assert_eq!(miniature_text("not shown".chars(), 0), "");
    }
}
