use gpui::{App, Bounds, Element, ElementId, GlobalElementId, IntoElement, LayoutId, Pixels, Point, StyledText, TextAlign, TextLayout, TextRun, Window};
use crate::markdown::{PreparedInline, RenderDocument, RowContent};
use std::{cell::RefCell, collections::BTreeMap, ops::Range, rc::Rc};

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Position {
    pub(super) row: usize,
    pub(super) cell: usize,
    pub(super) byte: usize,
}

#[derive(Default)]
pub(super) struct Selection {
    pub(super) anchor: Position,
    pub(super) head: Position,
    pub(super) active: bool,
    pub(super) dragging: bool,
    pub(super) moved: bool,
    pub(super) down: Option<Point<Pixels>>,
    pub(super) pointer: Option<Point<Pixels>>,
    pub(super) scroll_pending: bool,
    pub(super) generation: u64,
    pub(super) geometry: Rc<RefCell<BTreeMap<(usize, usize), Geometry>>>,
}

pub(super) struct Geometry {
    layout: TextLayout,
    align: TextAlign,
    clip: Bounds<Pixels>,
}

impl Geometry {
    fn bounds(&self) -> Bounds<Pixels> { self.layout.bounds().intersect(&self.clip) }

    fn index_for_position(&self, point: Point<Pixels>, closest: bool) -> Result<usize, usize> {
        let bounds = self.layout.bounds();
        if point.y < bounds.top() { return Err(0); }
        let height = self.layout.line_height();
        let mut y = bounds.top();
        let mut start = 0;
        for line in self.layout.line_layouts() {
            let bottom = y + line.size(height).height;
            if point.y >= bottom {
                y = bottom;
                start += line.len() + 1;
                continue;
            }
            let visual = (f32::from(point.y - y) / f32::from(height)).max(0.) as usize;
            let x_at_boundary = |index: usize| {
                line.wrap_boundaries.get(index).and_then(|boundary| {
                    line.unwrapped_layout.runs.get(boundary.run_ix)?.glyphs.get(boundary.glyph_ix).map(|glyph| glyph.position.x)
                })
            };
            let left = visual.checked_sub(1).and_then(|index| x_at_boundary(index)).unwrap_or(gpui::px(0.));
            let right = x_at_boundary(visual).unwrap_or(line.unwrapped_layout.width);
            let shift = match self.align {
                TextAlign::Left => gpui::px(0.),
                TextAlign::Center => (bounds.size.width - (right - left)) / 2.,
                TextAlign::Right => bounds.size.width - (right - left),
            };
            let position = gpui::point(point.x - bounds.left() - shift, point.y - y);
            let result = if closest { line.closest_index_for_position(position, height) } else { line.index_for_position(position, height) };
            return result.map(|byte| start + byte).map_err(|byte| start + byte);
        }
        Err(self.layout.len())
    }
}

impl Selection {
    pub(super) fn clear(&mut self) {
        self.active = false;
        self.dragging = false;
        self.moved = false;
        self.down = None;
        self.pointer = None;
        self.scroll_pending = false;
        self.generation = self.generation.wrapping_add(1);
        self.geometry.borrow_mut().clear();
    }

    pub(super) fn range(&self, row: usize, cell: usize, len: usize) -> Option<Range<usize>> {
        if !self.active { return None; }
        let start = self.anchor.min(self.head);
        let end = self.anchor.max(self.head);
        let first = Position { row, cell, byte: 0 };
        let last = Position { row, cell, byte: len };
        if end <= first || start >= last { return None; }
        let a = if (start.row, start.cell) == (row, cell) { start.byte.min(len) } else { 0 };
        let b = if (end.row, end.cell) == (row, cell) { end.byte.min(len) } else { len };
        (a < b).then_some(a..b)
    }

    fn covers_empty(&self, row: usize, cell: usize) -> bool {
        let position = Position { row, cell, byte: 0 };
        self.active && self.anchor.min(self.head) <= position && position < self.anchor.max(self.head)
    }

    pub(super) fn hit(&self, point: Point<Pixels>, viewport: Bounds<Pixels>) -> Option<Position> {
        let geometry = self.geometry.borrow();
        let ((row, cell), layout) = geometry.iter()
            .filter(|(_, layout)| {
                let bounds = layout.bounds();
                bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.)
                    && bounds.bottom() > viewport.top() && bounds.top() < viewport.bottom()
                    && bounds.right() > viewport.left() && bounds.left() < viewport.right()
            })
            .min_by(|(_, a), (_, b)| {
                let distance = |layout: &Geometry| {
                    let bounds = layout.bounds();
                    let dy = if point.y < bounds.top() { bounds.top() - point.y } else { (point.y - bounds.bottom()).max(gpui::px(0.)) };
                    let dx = if point.x < bounds.left() { bounds.left() - point.x } else { (point.x - bounds.right()).max(gpui::px(0.)) };
                    (f32::from(dy), f32::from(dx))
                };
                let a = distance(a);
                let b = distance(b);
                a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1))
            })?;
        let byte = match layout.index_for_position(point, true) { Ok(byte) | Err(byte) => byte };
        Some(Position { row: *row, cell: *cell, byte: byte.min(layout.layout.len()) })
    }

    pub(super) fn exact_hit(&self, point: Point<Pixels>) -> Option<Position> {
        self.geometry.borrow().iter().find_map(|(&(row, cell), layout)| {
            if !layout.bounds().contains(&point) { return None; }
            let byte = layout.index_for_position(point, false).ok()?;
            Some(Position { row, cell, byte })
        })
    }
}

pub(super) fn selected_runs(runs: Vec<TextRun>, selection: Option<Range<usize>>) -> Vec<TextRun> {
    let Some(selection) = selection else { return runs; };
    let mut output = Vec::new();
    let mut offset = 0;
    for run in runs {
        let end = offset + run.len;
        let mut cuts = vec![offset, end];
        for cut in [selection.start, selection.end] {
            if cut > offset && cut < end { cuts.push(cut); }
        }
        cuts.sort_unstable();
        for pair in cuts.windows(2) {
            let mut part = run.clone();
            part.len = pair[1] - pair[0];
            if selection.contains(&pair[0]) {
                part.background_color = Some(gpui::rgb(0x0078d4).into());
                part.color = gpui::rgb(0xffffff).into();
                if let Some(underline) = &mut part.underline { underline.color = Some(part.color); }
            }
            output.push(part);
        }
        offset = end;
    }
    output
}

pub(super) struct Text {
    pub(super) text: StyledText,
    pub(super) value: gpui::SharedString,
    pub(super) row: usize,
    pub(super) cell: usize,
    pub(super) links: Vec<Range<usize>>,
    pub(super) geometry: Rc<RefCell<BTreeMap<(usize, usize), Geometry>>>,
}

impl IntoElement for Text {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for Text {
    type RequestLayoutState = ();
    type PrepaintState = gpui::Hitbox;
    fn id(&self) -> Option<ElementId> { Some(("markdown-selection-text", self.cell).into()) }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> { None }
    fn a11y_role(&self) -> Option<gpui::accesskit::Role> { Some(gpui::accesskit::Role::Label) }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) { node.set_value(self.value.to_string()); }
    fn request_layout(&mut self, id: Option<&GlobalElementId>, inspector: Option<&gpui::InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        self.text.request_layout(id, inspector, window, cx)
    }
    fn prepaint(&mut self, id: Option<&GlobalElementId>, inspector: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, state: &mut (), window: &mut Window, cx: &mut App) -> gpui::Hitbox {
        self.text.prepaint(id, inspector, bounds, state, window, cx);
        self.geometry.borrow_mut().insert((self.row, self.cell), Geometry {
            layout: self.text.layout().clone(),
            align: window.text_style().text_align,
            clip: window.content_mask().bounds,
        });
        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
    }
    fn paint(&mut self, id: Option<&GlobalElementId>, inspector: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, state: &mut (), hitbox: &mut gpui::Hitbox, window: &mut Window, cx: &mut App) {
        let pointer = self.geometry.borrow().get(&(self.row, self.cell)).and_then(|geometry| geometry.index_for_position(window.mouse_position(), false).ok());
        let cursor = if pointer.is_some_and(|byte| self.links.iter().any(|range| range.contains(&byte))) {
            gpui::CursorStyle::PointingHand
        } else { gpui::CursorStyle::IBeam };
        window.set_cursor_style(cursor, hitbox);
        self.text.paint(id, inspector, bounds, state, &mut (), window, cx);
    }
}

fn escape(text: &str) -> String {
    let mut escaped = String::new();
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn inline_html(inline: &PreparedInline, range: Range<usize>, base: Option<&std::path::Path>) -> String {
    let mut html = String::new();
    for span in inline.spans.iter() {
        let start = range.start.max(span.range.start);
        let end = range.end.min(span.range.end);
        let Some(text) = inline.text.get(start..end).filter(|text| !text.is_empty()) else { continue; };
        let style = &span.style;
        let link = style.link.as_deref().and_then(|target| match crate::markdown::resolve_destination(target, base) {
            crate::markdown::Destination::External(url) => Some(url),
            crate::markdown::Destination::Fragment(fragment) => Some(format!("#{fragment}")),
            crate::markdown::Destination::Local(path) => url::Url::from_file_path(path).ok().map(String::from),
            crate::markdown::Destination::Blocked => None,
        });
        if let Some(link) = &link { html.push_str(&format!("<a href=\"{}\">", escape(link))); }
        if style.bold { html.push_str("<strong>"); }
        if style.italic { html.push_str("<em>"); }
        if style.strike { html.push_str("<s>"); }
        if style.code { html.push_str("<code>"); }
        html.push_str(&escape(text).replace('\n', "<br>"));
        if style.code { html.push_str("</code>"); }
        if style.strike { html.push_str("</s>"); }
        if style.italic { html.push_str("</em>"); }
        if style.bold { html.push_str("</strong>"); }
        if link.is_some() { html.push_str("</a>"); }
    }
    html
}

fn block_tags(row: &crate::markdown::RenderRow) -> (String, String) {
    let mut prefix = "<blockquote>".repeat(row.quote_depth);
    let mut suffix = "</blockquote>".repeat(row.quote_depth);
    if let Some(marker) = &row.marker {
        let indent = row.list_depth.saturating_sub(1).saturating_mul(24);
        if let Ok(number) = marker.trim_end_matches(['.', ')']).parse::<u64>() {
            prefix.push_str(&format!("<ol start=\"{number}\" style=\"margin-left:{indent}px\"><li>"));
            suffix.insert_str(0, "</li></ol>");
        } else {
            prefix.push_str(&format!("<ul style=\"margin-left:{indent}px\"><li>"));
            suffix.insert_str(0, "</li></ul>");
        }
    }
    (prefix, suffix)
}

pub(super) fn payload(document: &RenderDocument, selection: &Selection, base: Option<&std::path::Path>) -> Option<(String, String)> {
    if !selection.active || selection.anchor == selection.head { return None; }
    let start = selection.anchor.min(selection.head);
    let end = selection.anchor.max(selection.head);
    let mut plain = String::new();
    let mut html = String::from("<html><head><meta charset=\"utf-8\"></head><body>");
    let mut table_open = false;
    let mut code_open = false;
    let mut emitted = false;
    let mut group_suffix = String::new();
    for (index, row) in document.rows.iter().enumerate().take(end.row.saturating_add(1)).skip(start.row) {
        let table = matches!(row.content, RowContent::Table { .. });
        let code = matches!(row.content, RowContent::Code { .. });
        if table_open && !table { html.push_str("</table>"); html.push_str(&std::mem::take(&mut group_suffix)); table_open = false; }
        if code_open && !code { html.push_str("</code></pre>"); html.push_str(&std::mem::take(&mut group_suffix)); code_open = false; }
        let mut row_plain = String::new();
        let mut row_html = String::new();
        let mut row_selected = false;
        match &row.content {
            RowContent::Text { inline, heading } => {
                if let Some(range) = selection.range(index, 0, inline.text.len()) {
                    row_plain.push_str(inline.text.get(range.clone())?);
                    let tag = heading.map_or_else(|| "p".to_owned(), |level| format!("h{}", level.clamp(1, 6)));
                    row_html = format!("<{tag}>{}</{tag}>", inline_html(inline, range, base));
                }
                if (start.row < index || (start.row == index && start.byte == 0))
                    && (index < end.row || end.byte == inline.text.len()) {
                    for media in &inline.media {
                        if !row_plain.is_empty() { row_plain.push('\n'); }
                        row_plain.push_str(&media.alt);
                        row_html.push_str(&format!("<p>{}</p>", escape(&media.alt)));
                    }
                }
            }
            RowContent::Code { body, range, first, .. } => {
                let text = body[range.clone()].trim_end_matches(['\r', '\n']);
                if let Some(range) = selection.range(index, 0, text.len()).or_else(|| (text.is_empty() && selection.covers_empty(index, 0)).then_some(0..0)) {
                    row_selected = true;
                    row_plain.push_str(text.get(range)?);
                    if code_open && *first { html.push_str("</code></pre>"); html.push_str(&std::mem::take(&mut group_suffix)); code_open = false; }
                    let continuing = code_open;
                    if !code_open {
                        let (prefix, suffix) = block_tags(row);
                        html.push_str(&prefix);
                        group_suffix = suffix;
                        html.push_str("<pre><code>"); code_open = true;
                    }
                    row_html = escape(&row_plain);
                    if continuing { row_html.insert(0, '\n'); }
                }
            }
            RowContent::Mermaid { body } => {
                if let Some(range) = selection.range(index, 0, body.len()) {
                    row_plain.push_str(body.get(range)?);
                    row_html = format!("<pre><code>{}</code></pre>", escape(&row_plain));
                }
            }
            RowContent::Table { cells, heading, alignments } => {
                let tag = if *heading { "th" } else { "td" };
                let mut cell_emitted = false;
                for (cell, inline) in cells.iter().enumerate() {
                    if let Some(range) = selection.range(index, cell, inline.text.len()).or_else(|| (inline.text.is_empty() && selection.covers_empty(index, cell)).then_some(0..0)) {
                        if cell_emitted { row_plain.push('\t'); }
                        cell_emitted = true;
                        let mut cell_plain = inline.text.get(range.clone())?.to_owned();
                        let mut cell_html = inline_html(inline, range.clone(), base);
                        if range.start == 0 && range.end == inline.text.len() {
                            for media in &inline.media {
                                if !cell_plain.is_empty() { cell_plain.push('\n'); }
                                cell_plain.push_str(&media.alt);
                                if !cell_html.is_empty() { cell_html.push_str("<br>"); }
                                cell_html.push_str(&escape(&media.alt));
                            }
                        }
                        row_plain.push_str(&cell_plain);
                        let alignment = match alignments.get(cell) {
                            Some(crate::markdown::Alignment::Left) => "left",
                            Some(crate::markdown::Alignment::Center) => "center",
                            Some(crate::markdown::Alignment::Right) => "right",
                            _ if *heading => "center",
                            _ => "left",
                        };
                        row_html.push_str(&format!("<{tag} style=\"text-align:{alignment}\">{cell_html}</{tag}>"));
                    }
                }
                if !row_html.is_empty() {
                    if !table_open || *heading {
                        if table_open { html.push_str("</table>"); html.push_str(&std::mem::take(&mut group_suffix)); }
                        let (prefix, suffix) = block_tags(row);
                        html.push_str(&prefix);
                        group_suffix = suffix;
                        html.push_str("<table>");
                        table_open = true;
                    }
                    row_html = format!("<tr>{row_html}</tr>");
                }
            }
            RowContent::Rule => {}
        }
        if row_html.is_empty() && row_plain.is_empty() && !row_selected { continue; }
        if emitted { plain.push('\n'); }
        emitted = true;
        if let Some(marker) = &row.marker {
            if start.row < index || (start.cell == 0 && start.byte == 0) {
                plain.push_str(marker);
                plain.push(' ');
            }
        }
        if !table && !code {
            let (prefix, suffix) = block_tags(row);
            row_html = format!("{prefix}{row_html}{suffix}");
        }
        plain.push_str(&row_plain);
        html.push_str(&row_html);
    }
    if table_open { html.push_str("</table>"); }
    if code_open { html.push_str("</code></pre>"); }
    html.push_str(&group_suffix);
    html.push_str("</body></html>");
    (!plain.is_empty()).then_some((plain, html))
}

pub(super) fn write(plain: &str, html: &str) -> std::io::Result<()> {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData, NSString};
    let item = NSPasteboardItem::new();
    for (kind, text) in [("public.utf8-plain-text", plain), ("public.html", html)] {
        if !item.setData_forType(&NSData::with_bytes(text.as_bytes()), &NSString::from_str(kind)) {
            return Err(std::io::Error::other("Could not prepare the Markdown clipboard item"));
        }
    }
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    let item: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*item);
    if !board.writeObjects(&NSArray::from_slice(&[item])) {
        return Err(std::io::Error::other("Could not publish the Markdown clipboard item"));
    }
    Ok(())
}
