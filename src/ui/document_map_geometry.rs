#[derive(Clone, Copy, Debug)]
pub struct MapGeometry {
    total_lines: usize,
    visible_lines: usize,
    top_line: usize,
    first_line: usize,
    rows: usize,
    line_height: f32,
    width: f32,
}

impl MapGeometry {
    pub fn new(
        total_lines: usize,
        visible_lines: usize,
        top_line: usize,
        height: f32,
        line_height: f32,
        previous_first: usize,
        width: f32,
    ) -> Option<Self> {
        if !height.is_finite() || height <= 0. || !line_height.is_finite() || line_height <= 0. || !width.is_finite() || width < 0. {
            return None;
        }
        let total_lines = total_lines.max(1);
        let visible_lines = visible_lines.clamp(1, total_lines);
        let top_line = top_line.min(total_lines - visible_lines);
        let rows = ((height / line_height).ceil() as usize).clamp(1, total_lines);
        let first_line = previous_first.min(top_line)
            .max((top_line + visible_lines.min(rows)).saturating_sub(rows))
            .min(total_lines - rows);
        Some(Self {
            total_lines,
            visible_lines,
            top_line,
            first_line,
            rows,
            line_height,
            width,
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

    pub fn text_width(self, indent: f32) -> f32 {
        (self.width - indent).max(0.)
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

pub struct MiniatureText {
    pub text: String,
    pub source_len: usize,
    changes: Vec<(usize, usize)>,
}

impl MiniatureText {
    pub fn display_offset(&self, source: usize) -> usize {
        let source = source.min(self.source_len);
        let index = self.changes.partition_point(|(offset, _)| *offset <= source);
        let (offset, display) = self.changes[index.saturating_sub(1)];
        (display + source - offset).min(self.text.len())
    }
}

pub fn miniature_text(
    source: impl IntoIterator<Item = char>,
    tab_width: usize,
    space_width: f32,
    width: f32,
    line_height: f32,
    mut advance: impl FnMut(char) -> f32,
    mut measure: impl FnMut(&MiniatureText) -> f32,
) -> MiniatureText {
    let mut miniature = MiniatureText { text: String::new(), source_len: 0, changes: vec![(0, 0)] };
    if !width.is_finite() || width <= 0. || !line_height.is_finite() || line_height <= 0. {
        return miniature;
    }
    let source_limit = (width.ceil() as usize).saturating_mul(line_height.ceil() as usize);
    let output_limit = source_limit.saturating_mul(char::MAX.len_utf8());
    let space_width = space_width.max(f32::EPSILON);
    let tab_width = tab_width.max(1) as f32 * space_width;
    let mut x = 0.;
    for ch in source.into_iter().take(source_limit) {
        let remaining = output_limit.saturating_sub(miniature.text.len());
        if remaining == 0 || x >= width || matches!(ch, '\r' | '\n') {
            break;
        }
        if ch == '\t' {
            x = measure(&miniature);
            miniature.source_len += ch.len_utf8();
            let stop = ((x / tab_width).floor() + 1.) * tab_width;
            let spaces = (((stop.min(width) - x) / space_width).ceil().max(1.) as usize).min(remaining);
            miniature.text.extend(std::iter::repeat_n(' ', spaces));
            miniature.changes.push((miniature.source_len, miniature.text.len()));
            x = measure(&miniature);
        } else {
            let display = if ch.is_control() { '�' } else { ch };
            if display.len_utf8() > remaining { break; }
            miniature.source_len += ch.len_utf8();
            miniature.text.push(display);
            if display.len_utf8() != ch.len_utf8() {
                miniature.changes.push((miniature.source_len, miniature.text.len()));
            }
            x += advance(display).max(0.);
            if x >= width { x = measure(&miniature); }
        }
    }
    miniature
}
