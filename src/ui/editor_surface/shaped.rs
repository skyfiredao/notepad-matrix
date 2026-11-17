use super::{DisplayRow, layout};
use gpui::{
    App, Pixels, Point, ShapedLine, TextAlign, TextRun, Window, fill, point, px, rgb, size,
};
use std::ops::Range;

enum PartContent {
    Glyphs(Box<ShapedLine>),
    Tab(TextRun),
    Representation(Box<ShapedLine>, Option<u32>),
}

struct Part {
    range: Range<usize>,
    x: Pixels,
    width: Pixels,
    content: PartContent,
    recolor: Option<(Vec<TextRun>, Pixels)>,
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
            recolor: Some((group.runs, group.size)),
        });
        *x += width;
    }
}

impl EditorShapedLine {
    pub(super) fn baseline(&self) -> Pixels {
        self.baseline
    }

    pub(super) fn new(
        projection: DisplayRow,
        raw: &str,
        runs: Vec<(TextRun, Pixels)>,
        space_width: Pixels,
        baseline: Pixels,
        representation_font_size: Pixels,
        representation_foreground: u32,
        representation_background: u32,
        eol_mode: i64,
        eol_color: u32,
        npc_color: Option<u32>,
        include_controls: bool,
        window: &Window,
    ) -> Self {
        let mut tabs = raw
            .char_indices()
            .filter(|(_, ch)| *ch == '\t')
            .map(|(byte, _)| projection.display_index(byte)..projection.display_index(byte + 1))
            .peekable();
        let mut representations = projection.representations.iter().cloned().peekable();
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
                        recolor: None,
                    });
                    x = next;
                    byte = tab.end;
                } else if let Some(representation) =
                    representations.next_if(|range| range.start == byte)
                {
                    flush_group(&mut group, &projection, &mut parts, &mut x, window);
                    let text = &projection.text[representation.clone()];
                    let eol =
                        representation.start >= projection.display_index(projection.content_len());
                    let rounded = !eol || eol_mode & 1 != 0;
                    let custom = if eol {
                        (eol_mode & 2 != 0).then_some(eol_color)
                    } else if include_controls
                        || !projection.control_representations.contains(&representation)
                    {
                        npc_color
                    } else {
                        None
                    };
                    let shape = window.text_system().shape_line(
                        text.to_string().into(),
                        if rounded {
                            representation_font_size
                        } else {
                            font_size
                        },
                        &[TextRun {
                            len: text.len(),
                            font: run.font.clone(),
                            color: rgb(if rounded {
                                representation_background
                            } else {
                                custom.unwrap_or(representation_foreground)
                            })
                            .into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    let width = shape.width() + px(if rounded { 3. } else { 0. });
                    byte = representation.end;
                    parts.push(Part {
                        range: representation,
                        x,
                        width,
                        content: PartContent::Representation(
                            Box::new(shape),
                            rounded.then_some(custom.unwrap_or(representation_foreground)),
                        ),
                        recolor: None,
                    });
                    x += width;
                } else {
                    let next = tabs.peek().map_or(end, |tab| end.min(tab.start));
                    let next = representations
                        .peek()
                        .map_or(next, |range| next.min(range.start));
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
        let part = self.parts.partition_point(|part| part.range.start <= index);
        self.parts
            .get(part.saturating_sub(1))
            .map_or(px(0.), |part| {
                let relative = index.saturating_sub(part.range.start).min(part.range.len());
                part.x
                    + match &part.content {
                        PartContent::Glyphs(shape) => shape.x_for_index(relative),
                        PartContent::Tab(_) => {
                            part.width * (relative as f32 / part.range.len().max(1) as f32)
                        }
                        PartContent::Representation(_, _) => {
                            if relative == 0 {
                                px(0.)
                            } else {
                                part.width
                            }
                        }
                    }
            })
    }

    pub(super) fn closest_index_for_x(&self, x: Pixels) -> usize {
        let part = self.parts.partition_point(|part| part.x <= x);
        let index = self
            .parts
            .get(part.saturating_sub(1))
            .map_or(0, |part| match &part.content {
                PartContent::Glyphs(shape) => {
                    part.range.start + shape.closest_index_for_x(x - part.x)
                }
                PartContent::Tab(_) | PartContent::Representation(_, _) => {
                    if x - part.x < part.width / 2. {
                        part.range.start
                    } else {
                        part.range.end
                    }
                }
            });
        self.projection.source_index(index)
    }

    fn visible_parts(&self, origin: Point<Pixels>, window: &Window) -> &[Part] {
        let bounds = window.content_mask().bounds;
        let start = self
            .parts
            .partition_point(|part| origin.x + part.x + part.width <= bounds.left());
        let end = self
            .parts
            .partition_point(|part| origin.x + part.x < bounds.right());
        &self.parts[start.min(end)..end]
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
        for part in self.visible_parts(origin, window) {
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
                PartContent::Representation(shape, background) => {
                    let top = origin.y + self.baseline - shape.ascent;
                    let blob_height = (shape.ascent + px(1.)).ceil();
                    if let Some(background) = background {
                        window.paint_quad(
                            fill(
                                gpui::Bounds::new(
                                    point(origin.x + part.x + px(1.), top),
                                    size(part.width - px(1.), blob_height),
                                ),
                                rgb(*background),
                            )
                            .corner_radii(px(1.)),
                        );
                    }
                    shape
                        .paint(
                            point(
                                origin.x + part.x + px(if background.is_some() { 2. } else { 0. }),
                                top,
                            ),
                            shape.ascent + shape.descent,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn paint_recolored(
        &self,
        origin: Point<Pixels>,
        height: Pixels,
        color: u32,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), String> {
        for part in self.visible_parts(origin, window) {
            if let (PartContent::Glyphs(original), Some((runs, font_size))) =
                (&part.content, &part.recolor)
            {
                let runs = runs
                    .iter()
                    .map(|run| {
                        let mut run = run.clone();
                        run.color = run.background_color.unwrap_or_else(|| rgb(color).into());
                        run.background_color = None;
                        run
                    })
                    .collect::<Vec<_>>();
                let shape =
                    window
                        .text_system()
                        .shape_line(original.text.clone(), *font_size, &runs, None);
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
        for part in self.visible_parts(origin, window) {
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
                PartContent::Representation(_, _) => {}
            }
        }
        Ok(())
    }
}
