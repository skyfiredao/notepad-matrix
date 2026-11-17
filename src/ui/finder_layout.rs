use gpui::{App, AvailableSpace, Bounds, DecorationRun, Element, ElementId, GlobalElementId, IntoElement, LayoutId, LineLayout, Pixels, Point, ShapedLine, ShapedRun, TextAlign, TextRun, Window, fill, point, px, size};
use std::{cell::RefCell, collections::VecDeque, ffi::c_void, ops::Range, ptr::NonNull, rc::Rc};

pub(super) type Runs = Vec<(TextRun, Pixels)>;

struct StyledRange {
    range: Range<usize>,
    run: TextRun,
    font_size: Pixels,
}

struct Part {
    range: Range<usize>,
    x: Pixels,
    width: Pixels,
    shape: Option<LineLayout>,
    decorations: Vec<DecorationRun>,
    rtl: bool,
}

struct VisualLine {
    range: Range<usize>,
    parts: Vec<Part>,
    logical: Vec<usize>,
    width: Pixels,
}

struct Boundary {
    byte: usize,
    utf16: usize,
}

struct Cluster {
    part: Part,
    utf16: Range<usize>,
}

impl Part {
    fn leading(&self) -> Pixels {
        self.x + if self.rtl { self.width } else { px(0.) }
    }

    fn trailing(&self) -> Pixels {
        self.x + if self.rtl { px(0.) } else { self.width }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Hit {
    pub(super) byte: usize,
    pub(super) trailing: bool,
}

#[link(name = "icucore")]
unsafe extern "C" {
    fn ubidi_open() -> *mut c_void;
    fn ubidi_close(bidi: *mut c_void);
    #[link_name = "ubidi_setPara"]
    fn ubidi_set_para(bidi: *mut c_void, text: *const u16, length: i32, level: u8, embeddings: *mut u8, error: *mut i32);
    #[link_name = "ubidi_setLine"]
    fn ubidi_set_line(paragraph: *const c_void, start: i32, limit: i32, line: *mut c_void, error: *mut i32);
    #[link_name = "ubidi_getParagraph"]
    fn ubidi_get_paragraph(bidi: *const c_void, index: i32, start: *mut i32, limit: *mut i32, level: *mut u8, error: *mut i32) -> i32;
    #[link_name = "ubidi_countRuns"]
    fn ubidi_count_runs(bidi: *mut c_void, error: *mut i32) -> i32;
    #[link_name = "ubidi_getVisualRun"]
    fn ubidi_get_visual_run(bidi: *mut c_void, index: i32, start: *mut i32, length: *mut i32) -> i32;
}

struct NativeBidi(NonNull<c_void>);

impl NativeBidi {
    fn new() -> Result<Self, String> {
        NonNull::new(unsafe { ubidi_open() }).map(Self).ok_or_else(|| "Finder bidi allocation failed".to_owned())
    }
}

impl Drop for NativeBidi {
    fn drop(&mut self) {
        unsafe { ubidi_close(self.0.as_ptr()); }
    }
}

struct ParagraphBidi {
    line: NativeBidi,
    paragraph: NativeBidi,
    text: Vec<u16>,
}

impl ParagraphBidi {
    fn new(text: &str) -> Result<Self, String> {
        let bidi = Self { line: NativeBidi::new()?, paragraph: NativeBidi::new()?, text: text.encode_utf16().collect() };
        let length = i32::try_from(bidi.text.len()).map_err(|error| error.to_string())?;
        let mut error = 0;
        unsafe { ubidi_set_para(bidi.paragraph.0.as_ptr(), bidi.text.as_ptr(), length, 0, std::ptr::null_mut(), &mut error); }
        if error > 0 { return Err(format!("Finder bidi paragraph failed: {error}")); }
        Ok(bidi)
    }

    fn visual_runs(&mut self, range: Range<usize>) -> Result<Vec<(Range<usize>, bool)>, String> {
        let mut start = i32::try_from(range.start).map_err(|error| error.to_string())?;
        let end = i32::try_from(range.end).map_err(|error| error.to_string())?;
        let mut error = 0;
        let mut runs = Vec::new();
        while start < end {
            let mut limit = end;
            unsafe { ubidi_get_paragraph(self.paragraph.0.as_ptr(), start, std::ptr::null_mut(), &mut limit, std::ptr::null_mut(), &mut error); }
            if error > 0 || limit <= start { return Err(format!("Finder bidi paragraph range failed: {error}")); }
            let limit = limit.min(end);
            unsafe { ubidi_set_line(self.paragraph.0.as_ptr(), start, limit, self.line.0.as_ptr(), &mut error); }
            if error > 0 { return Err(format!("Finder bidi line failed: {error}")); }
            let count = unsafe { ubidi_count_runs(self.line.0.as_ptr(), &mut error) };
            if error > 0 { return Err(format!("Finder bidi runs failed: {error}")); }
            let base = usize::try_from(start).map_err(|error| error.to_string())?;
            for index in 0..count {
                let mut offset = 0;
                let mut length = 0;
                let direction = unsafe { ubidi_get_visual_run(self.line.0.as_ptr(), index, &mut offset, &mut length) };
                let offset = usize::try_from(offset).map_err(|error| error.to_string())?;
                let length = usize::try_from(length).map_err(|error| error.to_string())?;
                runs.push((base + offset..base + offset + length, direction == 1));
            }
            start = limit;
        }
        Ok(runs)
    }
}

pub(super) struct FinderLayout {
    lines: Vec<VisualLine>,
    pub(super) line_height: Pixels,
    baseline: Pixels,
    pub(super) width: Pixels,
}

pub(super) struct Source {
    text: String,
    runs: Runs,
    metrics: Runs,
    layouts: RefCell<VecDeque<(Option<Pixels>, Rc<FinderLayout>)>>,
}

struct Entry {
    row: crate::finder::Row,
    revision: (u64, bool),
    source: Rc<Source>,
}

#[derive(Default)]
pub(super) struct Cache(VecDeque<Entry>);

impl Cache {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    pub(super) fn source(&mut self, row: crate::finder::Row, text: &str, runs: Runs, metrics: Runs, revision: (u64, bool)) -> Rc<Source> {
        if let Some(index) = self.0.iter().position(|entry| entry.row == row && entry.revision == revision && entry.source.text == text && entry.source.runs == runs && entry.source.metrics == metrics) {
            if let Some(entry) = self.0.remove(index) {
                let source = entry.source.clone();
                self.0.push_back(entry);
                return source;
            }
        }
        let source = Rc::new(Source { text: text.to_owned(), runs, metrics, layouts: RefCell::new(VecDeque::new()) });
        self.0.push_back(Entry { row, revision, source: source.clone() });
        while self.0.len() > 128 {
            self.0.pop_front();
        }
        source
    }
}

pub(super) fn metrics(runs: &Runs, window: &Window) -> (Pixels, Pixels) {
    let mut ascent = px(0.);
    let mut descent = px(0.);
    let mut height = px(1.);
    for (run, font_size) in runs {
        let mut run = run.clone();
        run.len = 2;
        let shape = window.text_system().shape_line("Mg".into(), *font_size, &[run], None);
        ascent = ascent.max(shape.ascent);
        descent = descent.max(shape.descent);
        height = height.max((*font_size * 1.3).ceil());
    }
    height = height.max((ascent + descent).ceil());
    (height, (height - ascent - descent) / 2. + ascent)
}

impl Source {
    pub(super) fn layout(&self, width: Option<Pixels>, window: &Window) -> Option<Rc<FinderLayout>> {
        let width = width.map(|width| width.max(px(1.)));
        if let Some((_, layout)) = self.layouts.borrow().iter().find(|(cached_width, _)| *cached_width == width) {
            return Some(layout.clone());
        }
        let layout = match FinderLayout::new(self, width, window) {
            Ok(layout) => Rc::new(layout),
            Err(error) => { eprintln!("Finder text could not be laid out: {error}"); return None; }
        };
        let mut layouts = self.layouts.borrow_mut();
        layouts.push_back((width, layout.clone()));
        while layouts.len() > 2 {
            layouts.pop_front();
        }
        Some(layout)
    }
}

fn decorations(runs: &[StyledRange], range: Range<usize>) -> Vec<DecorationRun> {
    let start = runs.partition_point(|run| run.range.end <= range.start);
    runs[start..].iter().take_while(|run| run.range.start < range.end).filter_map(|run| {
        let len = run.range.end.min(range.end).saturating_sub(run.range.start.max(range.start));
        (len > 0).then(|| DecorationRun {
            len: len as u32,
            color: run.run.color,
            background_color: run.run.background_color,
            underline: run.run.underline,
            strikethrough: run.run.strikethrough,
        })
    }).collect()
}

fn append_clusters(output: &mut Vec<Cluster>, shape: ShapedLine, start: usize, boundaries: &[Boundary], runs: &[StyledRange]) {
    let end = start + shape.len();
    let mut starts = vec![start, end];
    for glyph in shape.runs.iter().flat_map(|run| &run.glyphs) {
        let byte = start + glyph.index;
        let boundary = boundaries.partition_point(|boundary| boundary.byte <= byte).saturating_sub(1);
        starts.push(boundaries[boundary].byte);
    }
    starts.sort_unstable();
    starts.dedup();
    let mut clusters = Vec::new();
    for pair in starts.windows(2) {
        let from = boundaries.partition_point(|boundary| boundary.byte <= pair[0]).saturating_sub(1);
        let to = boundaries.partition_point(|boundary| boundary.byte <= pair[1]).saturating_sub(1);
        clusters.push(Cluster {
            part: Part {
                range: pair[0]..pair[1], x: px(0.), width: px(0.), rtl: false,
                shape: Some(LineLayout { font_size: shape.font_size, width: px(0.), ascent: shape.ascent, descent: shape.descent, runs: Vec::new(), len: pair[1] - pair[0] }),
                decorations: decorations(runs, pair[0]..pair[1]),
            },
            utf16: boundaries[from].utf16..boundaries[to].utf16,
        });
    }
    let mut anchors: Vec<Option<Pixels>> = vec![None; clusters.len()];
    for run in &shape.runs {
        for glyph in &run.glyphs {
            let byte = start + glyph.index;
            let index = starts.partition_point(|start| *start <= byte).saturating_sub(1).min(clusters.len().saturating_sub(1));
            let Some(cluster) = clusters.get_mut(index) else { continue; };
            anchors[index] = Some(anchors[index].map_or(glyph.position.x, |x| x.min(glyph.position.x)));
            if let Some(layout) = &mut cluster.part.shape {
                let mut glyph = glyph.clone();
                glyph.index = byte - cluster.part.range.start;
                if let Some(last) = layout.runs.last_mut() && last.font_id == run.font_id {
                    last.glyphs.push(glyph);
                } else {
                    layout.runs.push(ShapedRun { font_id: run.font_id, glyphs: vec![glyph] });
                }
            }
        }
    }
    let mut visual: Vec<_> = anchors.iter().enumerate().filter_map(|(index, anchor)| anchor.map(|anchor| (index, anchor))).collect();
    visual.sort_by(|a, b| f32::from(a.1).total_cmp(&f32::from(b.1)));
    let mut left = px(0.);
    for (index, (logical, _)) in visual.iter().enumerate() {
        let right = visual.get(index + 1).map_or(shape.width(), |(_, anchor)| *anchor).clamp(left, shape.width().max(left));
        let cluster = &mut clusters[*logical];
        cluster.part.width = right - left;
        if let Some(layout) = &mut cluster.part.shape {
            layout.width = right - left;
            for glyph in layout.runs.iter_mut().flat_map(|run| &mut run.glyphs) { glyph.position.x -= left; }
        }
        left = right;
    }
    output.extend(clusters);
}

impl FinderLayout {
    fn new(source: &Source, wrap_width: Option<Pixels>, window: &Window) -> Result<Self, String> {
        let (line_height, baseline) = metrics(&source.metrics, window);
        let mut layout = Self { lines: Vec::new(), line_height, baseline, width: px(0.) };
        let mut paragraph_start = 0;
        for paragraph in source.text.split('\n') {
            let paragraph_end = paragraph_start + paragraph.len();
            let native = objc2_foundation::NSString::from_str(paragraph);
            let mut utf16 = 0;
            let mut next = 0;
            let mut boundaries = Vec::new();
            for (byte, ch) in paragraph.char_indices() {
                if utf16 >= next {
                    boundaries.push(Boundary { byte: paragraph_start + byte, utf16 });
                    let range = native.rangeOfComposedCharacterSequenceAtIndex(utf16);
                    next = range.location + range.length;
                }
                utf16 += ch.len_utf16();
            }
            boundaries.push(Boundary { byte: paragraph_end, utf16 });
            let mut runs = Vec::new();
            let mut byte = 0;
            for (run, font_size) in &source.runs {
                let end = byte + run.len;
                let from = byte.max(paragraph_start).min(paragraph_end);
                let to = end.max(paragraph_start).min(paragraph_end);
                let from = boundaries[boundaries.partition_point(|boundary| boundary.byte <= from).saturating_sub(1)].byte;
                let to = boundaries[boundaries.partition_point(|boundary| boundary.byte <= to).saturating_sub(1)].byte;
                if from < to {
                    runs.push(StyledRange { range: from..to, run: run.clone(), font_size: *font_size });
                }
                byte = end;
            }
            let mut clusters = Vec::new();
            let mut x = px(0.);
            let default = &source.metrics[0];
            let mut space_run = default.0.clone();
            space_run.len = 1;
            let tab_width = (window.text_system().shape_line(" ".into(), default.1, &[space_run], None).width() * 4.).max(px(1.));
            let mut at = paragraph_start;
            let mut run_index = 0;
            while at < paragraph_end {
                while run_index + 1 < runs.len() && runs[run_index].range.end <= at {
                    run_index += 1;
                }
                let Some(run) = runs.get(run_index) else { break; };
                if source.text.as_bytes()[at] == b'\t' {
                    let width = px(((f32::from(x) / f32::from(tab_width)).floor() + 1.) * f32::from(tab_width)) - x;
                    let boundary = boundaries.partition_point(|boundary| boundary.byte <= at).saturating_sub(1);
                    let utf16 = boundaries[boundary].utf16;
                    clusters.push(Cluster {
                        part: Part { range: at..at + 1, x: px(0.), width, shape: None, decorations: decorations(&runs, at..at + 1), rtl: false },
                        utf16: utf16..utf16 + 1,
                    });
                    x += width;
                    at += 1;
                    continue;
                }
                let font_size = run.font_size;
                let start = at;
                let mut shape_runs = Vec::new();
                while at < paragraph_end {
                    let Some(run) = runs.get(run_index) else { break; };
                    if run.font_size != font_size { break; }
                    let end = source.text[at..run.range.end].find('\t').map_or(run.range.end, |offset| at + offset);
                    if end == at { break; }
                    let mut fragment = run.run.clone();
                    fragment.len = end - at;
                    shape_runs.push(fragment);
                    at = end;
                    if at < run.range.end { break; }
                    run_index += 1;
                }
                let shape = window.text_system().shape_line(source.text[start..at].to_owned().into(), font_size, &shape_runs, None);
                let width = shape.width();
                append_clusters(&mut clusters, shape, start, &boundaries, &runs);
                x += width;
            }
            layout.width = layout.width.max(x);
            let breaks: std::collections::BTreeSet<_> = unicode_linebreak::linebreaks(paragraph).map(|(byte, _)| paragraph_start + byte).collect();
            let mut ranges = Vec::new();
            let mut first = 0;
            let mut end = 0;
            let mut width = px(0.);
            let mut preferred = None;
            while first < clusters.len() {
                while end < clusters.len() && (end == first || wrap_width.is_none_or(|limit| width + clusters[end].part.width <= limit)) {
                    width += clusters[end].part.width;
                    end += 1;
                    if breaks.contains(&clusters[end - 1].part.range.end) { preferred = Some(end); }
                }
                let cut = if end < clusters.len() { preferred.unwrap_or(end) } else { end };
                ranges.push(first..cut);
                for cluster in &clusters[first..cut] { width -= cluster.part.width; }
                width = width.max(px(0.));
                first = cut;
                preferred = None;
            }
            if clusters.is_empty() {
                layout.lines.push(VisualLine { range: paragraph_start..paragraph_end, parts: Vec::new(), logical: Vec::new(), width: px(0.) });
            } else {
                let mut bidi = ParagraphBidi::new(paragraph)?;
                let mut clusters: Vec<_> = clusters.into_iter().map(Some).collect();
                for range in ranges {
                    let first = clusters[range.start].as_ref().ok_or_else(|| "Finder cluster was already placed".to_owned())?;
                    let last = clusters[range.end - 1].as_ref().ok_or_else(|| "Finder cluster was already placed".to_owned())?;
                    let bytes = first.part.range.start..last.part.range.end;
                    let utf16 = first.utf16.start..last.utf16.end;
                    let mut line = VisualLine { range: bytes, parts: Vec::new(), logical: Vec::new(), width: px(0.) };
                    let starts: Vec<_> = clusters[range.clone()].iter().map(|cluster| {
                        cluster.as_ref().map(|cluster| cluster.utf16.start).ok_or_else(|| "Finder cluster was already placed".to_owned())
                    }).collect::<Result<_, _>>()?;
                    for (visual, rtl) in bidi.visual_runs(utf16)? {
                        let start = starts.partition_point(|start| *start < visual.start);
                        let end = starts.partition_point(|start| *start < visual.end);
                        let mut place = |index: usize| -> Result<(), String> {
                            let mut cluster = clusters[range.start + index].take().ok_or_else(|| "Finder cluster was already placed".to_owned())?;
                            cluster.part.rtl = rtl;
                            cluster.part.x = line.width;
                            line.width += cluster.part.width;
                            line.parts.push(cluster.part);
                            Ok(())
                        };
                        if rtl {
                            for index in (start..end).rev() { place(index)?; }
                        } else {
                            for index in start..end { place(index)?; }
                        }
                    }
                    if line.parts.len() != range.len() { return Err("Finder bidi runs did not cover the visual line".to_owned()); }
                    line.logical = (0..line.parts.len()).collect();
                    line.logical.sort_unstable_by_key(|index| line.parts[*index].range.start);
                    layout.lines.push(line);
                }
            }
            paragraph_start = paragraph_end + 1;
        }
        Ok(layout)
    }

    pub(super) fn height(&self) -> Pixels {
        self.line_height * self.lines.len() as f32
    }

    pub(super) fn position_for_index(&self, byte: usize, trailing: bool) -> Point<Pixels> {
        let index = self.lines.partition_point(|line| line.range.start <= byte).saturating_sub(1);
        let line = &self.lines[index];
        let at = line.logical.partition_point(|part| line.parts[*part].range.start <= byte).saturating_sub(1);
        let part = if trailing && at > 0 && line.parts[line.logical[at - 1]].range.end == byte {
            line.parts.get(line.logical[at - 1])
        } else {
            line.logical.get(at).and_then(|part| line.parts.get(*part))
        };
        let x = part.map_or(px(0.), |part| if byte >= part.range.end { part.trailing() } else { part.leading() });
        point(x, self.line_height * index as f32)
    }

    pub(super) fn closest_index_for_position(&self, position: Point<Pixels>) -> Hit {
        let index = ((f32::from(position.y) / f32::from(self.line_height)).floor().max(0.) as usize).min(self.lines.len() - 1);
        let line = &self.lines[index];
        let at = line.parts.partition_point(|part| part.x <= position.x).saturating_sub(1);
        let Some(part) = line.parts.get(at) else { return Hit { byte: line.range.start, trailing: false }; };
        let right = position.x - part.x >= part.width / 2.;
        let trailing = right != part.rtl;
        let byte = if trailing { part.range.end } else { part.range.start };
        Hit { byte, trailing: trailing && byte != line.range.end }
    }

    pub(super) fn unwrapped_x_for_index(&self, byte: usize, trailing: bool) -> Pixels {
        self.position_for_index(byte, trailing).x
    }

    fn snap_index(&self, byte: usize) -> usize {
        let index = self.lines.partition_point(|line| line.range.start <= byte).saturating_sub(1);
        let line = &self.lines[index];
        let at = line.logical.partition_point(|part| line.parts[*part].range.start <= byte).saturating_sub(1);
        line.logical.get(at).map_or(line.range.start, |part| {
            let part = &line.parts[*part];
            if byte >= part.range.end { part.range.end } else { part.range.start }
        })
    }

    fn paint(&self, origin: Point<Pixels>, selection: Option<Range<usize>>, foreground: Option<gpui::Hsla>, background: gpui::Hsla, window: &mut Window, cx: &mut App) {
        let selection = selection.map(|range| self.snap_index(range.start)..self.snap_index(range.end));
        let mask = window.content_mask();
        let first = ((f32::from(mask.bounds.top() - origin.y) / f32::from(self.line_height)).floor().max(0.) as usize).min(self.lines.len());
        for (index, line) in self.lines.iter().enumerate().skip(first) {
            let y = origin.y + self.line_height * index as f32;
            if y >= mask.bounds.bottom() { break; }
            let first_part = line.parts.partition_point(|part| origin.x + part.x + part.width <= mask.bounds.left());
            for part in line.parts[first_part..].iter().take_while(|part| origin.x + part.x < mask.bounds.right()) {
                let x = origin.x + part.x;
                let selected = selection.as_ref().map(|range| range.start.max(part.range.start)..range.end.min(part.range.end)).filter(|range| !range.is_empty());
                let mut decorations = Vec::new();
                let mut byte = part.range.start;
                for decoration in &part.decorations {
                    let end = byte + decoration.len as usize;
                    let mut cuts = vec![byte, end];
                    if let Some(range) = &selected {
                        cuts.extend([range.start.clamp(byte, end), range.end.clamp(byte, end)]);
                    }
                    cuts.sort_unstable();
                    cuts.dedup();
                    for pair in cuts.windows(2) {
                        let mut decoration = decoration.clone();
                        decoration.len = (pair[1] - pair[0]) as u32;
                        if selected.as_ref().is_some_and(|range| range.contains(&pair[0])) {
                            decoration.background_color = Some(background);
                            if let Some(color) = foreground {
                                decoration.color = color;
                                if let Some(underline) = &mut decoration.underline { underline.color = Some(color); }
                            }
                        }
                        decorations.push(decoration);
                    }
                    byte = end;
                }
                if let Some(shape) = &part.shape {
                    let top = point(x, y);
                    if let Err(error) = gpui::LineLayout::paint_background(shape, top, self.line_height, TextAlign::Left, None, &decorations, window, cx) {
                        eprintln!("Finder text background could not be painted: {error}");
                    }
                    let shift = self.baseline - ((self.line_height - shape.ascent - shape.descent) / 2. + shape.ascent);
                    if let Err(error) = gpui::LineLayout::paint(shape, point(x, y + shift), self.line_height, TextAlign::Left, None, &decorations, window, cx) {
                        eprintln!("Finder text could not be painted: {error}");
                    }
                } else if let Some(decoration) = decorations.first() {
                    if let Some(color) = decoration.background_color {
                        window.paint_quad(fill(Bounds::new(point(x, y), size(part.width, self.line_height)), color));
                    }
                    if let Some(underline) = &decoration.underline {
                        window.paint_underline(point(x, y + self.baseline + px(1.)), part.width, underline);
                    }
                }
            }
        }
    }
}

pub(super) struct FinderText {
    pub(super) source: Rc<Source>,
    pub(super) wrapped: bool,
    pub(super) selection: Option<Range<usize>>,
    pub(super) foreground: Option<gpui::Hsla>,
    pub(super) background: gpui::Hsla,
    pub(super) caret: Option<(usize, bool, gpui::Hsla)>,
}

impl IntoElement for FinderText {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for FinderText {
    type RequestLayoutState = ();
    type PrepaintState = Option<Rc<FinderLayout>>;

    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> { None }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>, window: &mut Window, _: &mut App) -> (LayoutId, Self::RequestLayoutState) {
        let source = self.source.clone();
        let wrapped = self.wrapped;
        let id = window.request_measured_layout(Default::default(), move |known, available, window, _| {
            let width = wrapped.then(|| known.width.or(match available.width { AvailableSpace::Definite(width) => Some(width), _ => None })).flatten();
            let Some(layout) = source.layout(width, window) else { return size(px(0.), metrics(&source.metrics, window).0); };
            size(width.unwrap_or(layout.width + px(1.)), if wrapped { layout.height() } else { layout.line_height })
        });
        (id, ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, _: &mut Self::RequestLayoutState, window: &mut Window, _: &mut App) -> Self::PrepaintState {
        self.source.layout(self.wrapped.then_some(bounds.size.width), window)
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, _: &mut Self::RequestLayoutState, layout: &mut Self::PrepaintState, window: &mut Window, cx: &mut App) {
        let Some(layout) = layout else { return; };
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            layout.paint(bounds.origin, self.selection.clone(), self.foreground, self.background, window, cx);
            if let Some((byte, trailing, color)) = self.caret {
                let position = layout.position_for_index(byte, trailing);
                let x = (bounds.left() + position.x).min(bounds.right() - px(1.));
                window.paint_quad(fill(Bounds::new(point(x, bounds.top() + position.y), size(px(1.), layout.line_height)), color));
            }
        });
    }
}
