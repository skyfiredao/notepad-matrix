use super::{DisplayRow, layout};
use gpui::{App, Pixels, Point, ShapedLine, TextAlign, TextRun, Window, fill, point, px, size};
use std::ops::Range;

enum PartContent {
    Glyphs(Box<ShapedLine>),
    Tab(TextRun),
}

struct Part {
    range: Range<usize>,
    x: Pixels,
    width: Pixels,
    content: PartContent,
}

struct Group {
    range: Range<usize>,
    size: Pixels,
    runs: Vec<TextRun>,
}

pub(super) struct EditorShapedLine {
    parts: Vec<Part>,
    projection: DisplayRow,
    width: Pixels,
    baseline: Pixels,
}

fn flush_group(
    group: &mut Option<Group>,
    projection: &DisplayRow,
    parts: &mut Vec<Part>,
    x: &mut Pixels,
    window: &Window,
) {
    if let Some(group) = group.take() {
        let shape = window.text_system().shape_line(
            projection.text[group.range.clone()].to_string().into(),
            group.size,
            &group.runs,
            None,
        );
        let width = shape.width();
        parts.push(Part {
            range: group.range,
            x: *x,
            width,
            content: PartContent::Glyphs(Box::new(shape)),
        });
        *x += width;
    }
}

impl EditorShapedLine {
    pub(super) fn new(
        projection: DisplayRow,
        raw: &str,
        runs: Vec<(TextRun, Pixels)>,
        space_width: Pixels,
        baseline: Pixels,
        window: &Window,
    ) -> Self {
        let mut tabs = raw
            .char_indices()
            .filter(|(_, ch)| *ch == '\t')
            .map(|(byte, _)| projection.display_index(byte)..projection.display_index(byte + 1))
            .peekable();
        let mut parts = Vec::new();
        let mut group: Option<Group> = None;
        let mut byte = 0;
        let mut x = px(0.);
        for (run, font_size) in runs {
            let end = byte + run.len;
            while byte < end {
                if let Some(tab) = tabs.next_if(|tab| tab.start == byte) {
                    flush_group(&mut group, &projection, &mut parts, &mut x, window);
                    let next = px(layout::next_tab_stop(
                        f32::from(x),
                        f32::from(space_width),
                        4,
                    ));
                    parts.push(Part {
                        range: tab.clone(),
                        x,
                        width: next - x,
                        content: PartContent::Tab(run.clone()),
                    });
                    x = next;
                    byte = tab.end;
                } else {
                    let next = tabs.peek().map_or(end, |tab| end.min(tab.start));
                    if group.as_ref().is_some_and(|group| group.size != font_size) {
                        flush_group(&mut group, &projection, &mut parts, &mut x, window);
                    }
                    let group = group.get_or_insert_with(|| Group {
                        range: byte..byte,
                        size: font_size,
                        runs: Vec::new(),
                    });
                    let mut fragment = run.clone();
                    fragment.len = next - byte;
                    group.range.end = next;
                    group.runs.push(fragment);
                    byte = next;
                }
            }
        }
        flush_group(&mut group, &projection, &mut parts, &mut x, window);
        Self {
            parts,
            projection,
            width: x,
            baseline,
        }
    }

    pub(super) fn width(&self) -> Pixels {
        self.width
    }
    pub(super) fn content_len(&self) -> usize {
        self.projection.content_len()
    }

    pub(super) fn x_for_index(&self, source: usize) -> Pixels {
        let index = self.projection.display_index(source);
        self.parts
            .iter()
            .rev()
            .find(|part| part.range.start <= index)
            .map_or(px(0.), |part| {
                let relative = index.saturating_sub(part.range.start).min(part.range.len());
                part.x
                    + match &part.content {
                        PartContent::Glyphs(shape) => shape.x_for_index(relative),
                        PartContent::Tab(_) => {
                            part.width * (relative as f32 / part.range.len().max(1) as f32)
                        }
                    }
            })
    }

    pub(super) fn closest_index_for_x(&self, x: Pixels) -> usize {
        let index = self
            .parts
            .iter()
            .rev()
            .find(|part| part.x <= x)
            .or_else(|| self.parts.first())
            .map_or(0, |part| match &part.content {
                PartContent::Glyphs(shape) => {
                    part.range.start + shape.closest_index_for_x(x - part.x)
                }
                PartContent::Tab(_) => {
                    if x - part.x < part.width / 2. {
                        part.range.start
                    } else {
                        part.range.end
                    }
                }
            });
        self.projection.source_index(index)
    }

    pub(super) fn paint(
        &self,
        origin: Point<Pixels>,
        height: Pixels,
        _align: TextAlign,
        _width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), String> {
        for part in &self.parts {
            match &part.content {
                PartContent::Glyphs(shape) => {
                    let shift = px(layout::baseline_shift(
                        f32::from(height),
                        f32::from(self.baseline),
                        f32::from(shape.ascent),
                        f32::from(shape.descent),
                    ));
                    shape
                        .paint(
                            point(origin.x + part.x, origin.y + shift),
                            height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        )
                        .map_err(|error| error.to_string())?;
                }
                PartContent::Tab(run) => {
                    if let Some(underline) = &run.underline {
                        window.paint_underline(
                            point(origin.x + part.x, origin.y + self.baseline + px(1.)),
                            part.width,
                            underline,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn paint_background(
        &self,
        origin: Point<Pixels>,
        height: Pixels,
        _align: TextAlign,
        _width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), String> {
        for part in &self.parts {
            match &part.content {
                PartContent::Glyphs(shape) => shape
                    .paint_background(
                        point(origin.x + part.x, origin.y),
                        height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .map_err(|error| error.to_string())?,
                PartContent::Tab(run) => {
                    if let Some(color) = run.background_color {
                        window.paint_quad(fill(
                            gpui::Bounds::new(
                                point(origin.x + part.x, origin.y),
                                size(part.width, height),
                            ),
                            color,
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
