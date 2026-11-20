use super::super::*;
use super::super::dialog_font::{self, px};
use gpui::{ContentMask, ElementInputHandler, EntityInputHandler, Pixels, Point, ShapedLine, TextRun, UTF16Selection, fill, point};
use std::ops::Range;

pub(super) struct Field {
    state: EditorSession,
    pub(super) focus: FocusHandle,
    selector: &'static str,
    read_only: bool,
    first_line: usize,
    horizontal: Pixels,
    layout: Option<Layout>,
    dark: bool,
    locale: String,
    error: Option<String>,
}

struct Row {
    start: usize,
    end: usize,
    line: ShapedLine,
    origin: Point<Pixels>,
}

struct Layout {
    bounds: Bounds<Pixels>,
    rows: Vec<Row>,
    revision: crate::input::DisplayRevision,
    row_count: usize,
    content_width: Pixels,
}

impl Field {
    pub(super) fn new(selector: &'static str, read_only: bool, cx: &mut Context<Self>) -> Self {
        let mut state = EditorSession::new("");
        state.set_read_only(read_only);
        Self { state, focus: cx.focus_handle().tab_stop(true), selector, read_only, first_line: 0,
            horizontal: px(0.), layout: None, dark: false, locale: String::new(), error: None }
    }

    pub(super) fn text(&self) -> String { self.state.committed_text() }

    pub(super) fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.state = EditorSession::new(text);
        self.state.set_read_only(self.read_only);
        self.first_line = 0;
        self.horizontal = px(0.);
        self.layout = None;
        cx.notify();
    }

    pub(super) fn set_dark(&mut self, dark: bool, cx: &mut Context<Self>) {
        if self.dark != dark { self.dark = dark; cx.notify(); }
    }

    pub(super) fn set_locale(&mut self, locale: &str, cx: &mut Context<Self>) {
        if self.locale != locale { self.locale = locale.to_owned(); cx.notify(); }
    }

    fn changed(&mut self, result: Result<(), crate::editor::EditError>, window: &mut Window, cx: &mut Context<Self>) {
        self.error = result.err().map(|error| error.to_string());
        let text = self.state.display_text();
        let head = self.state.selection().head.0.min(text.len());
        let row = text[..head].bytes().filter(|byte| *byte == b'\n').count();
        if row < self.first_line { self.first_line = row; }
        if row >= self.first_line + 5 { self.first_line = row.saturating_sub(4); }
        if let Some(layout) = &self.layout
            && let Some(row) = layout.rows.iter().find(|row| row.start <= head && head <= row.end)
        {
            let x = row.line.x_for_index(head - row.start);
            self.horizontal = self.horizontal.min(x).max(x - layout.bounds.size.width + px(12.)).max(px(0.));
        }
        self.layout = None;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn byte_at(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.layout.as_ref().filter(|layout| layout.revision == self.state.display_revision())?;
        let row = layout.rows.iter().min_by(|a, b| {
            let ay = (f32::from(position.y - a.origin.y - px(8.))).abs();
            let by = (f32::from(position.y - b.origin.y - px(8.))).abs();
            ay.total_cmp(&by)
        })?;
        Some((row.start + row.line.closest_index_for_x(position.x - row.origin.x)).min(row.end))
    }

    fn select(&mut self, head: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = if extend { self.state.selection().anchor } else { ByteOffset(head) };
        let result = self.state.select_display(Selection { anchor, head: ByteOffset(head) });
        self.changed(result, window, cx);
    }

    fn horizontal_move(&mut self, right: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.state.display_text();
        let selection = self.state.selection();
        let range = selection.range();
        let head = selection.head.0;
        let next = if !extend && !range.is_empty() {
            if right { range.end.0 } else { range.start.0 }
        } else if right {
            if text[head..].starts_with("\r\n") { head + 2 }
            else { text[head..].chars().next().map_or(head, |ch| head + ch.len_utf8()) }
        } else {
            if text[..head].ends_with("\r\n") { head - 2 }
            else { text[..head].char_indices().next_back().map_or(0, |(byte, _)| byte) }
        };
        self.select(next, extend, window, cx);
        cx.stop_propagation();
    }

    fn delete(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only { cx.stop_propagation(); return; }
        let mut range = self.state.selection().range();
        if range.is_empty() {
            let text = self.state.display_text();
            if right {
                range.end = ByteOffset(if text[range.end.0..].starts_with("\r\n") { range.end.0 + 2 }
                    else { text[range.end.0..].chars().next().map_or(range.end.0, |ch| range.end.0 + ch.len_utf8()) });
            } else {
                range.start = ByteOffset(if text[..range.start.0].ends_with("\r\n") { range.start.0 - 2 }
                    else { text[..range.start.0].char_indices().next_back().map_or(0, |(byte, _)| byte) });
            }
        }
        let result = self.state.replace_display_range(range, "");
        self.changed(result, window, cx);
        cx.stop_propagation();
    }

    fn copy(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.state.selection().range();
        if !range.is_empty() {
            let text = self.state.display_snapshot().slice(range.start.0..range.end.0).to_string();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            if cut && !self.read_only {
                let result = self.state.replace_display_range(range, "");
                self.changed(result, window, cx);
            }
        }
        cx.stop_propagation();
    }

    fn key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "enter" && self.state.has_composition() { return; }
        let extend = event.keystroke.modifiers.shift;
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            match key {
                "a" => {
                    let end = self.state.display_snapshot().len();
                    let result = self.state.select_display(Selection { anchor: ByteOffset(0), head: ByteOffset(end) });
                    self.changed(result, window, cx);
                }
                "c" => self.copy(false, window, cx),
                "x" => self.copy(true, window, cx),
                "v" => {
                    if !self.read_only && let Some(payload) = clipboard_surface::read_payload(cx) {
                        let text = payload.text.split('\0').next().unwrap_or_default();
                        let result = self.state.insert_text(text); self.changed(result, window, cx);
                    }
                }
                "z" if !self.read_only => {
                    if extend { self.state.redo(); } else { self.state.undo(); }
                    self.changed(Ok(()), window, cx);
                }
                _ => return,
            }
            cx.stop_propagation(); return;
        }
        let text = self.state.display_text();
        let head = self.state.selection().head.0;
        let line_start = text[..head].rfind('\n').map_or(0, |byte| byte + 1);
        let line_end = text[head..].find('\n').map_or(text.len(), |byte| head + byte);
        match key {
            "enter" if !self.read_only => {
                let result = self.state.insert_text("\r\n");
                self.changed(result, window, cx);
            }
            "home" => self.select(line_start, extend, window, cx),
            "end" => self.select(if text[..line_end].ends_with('\r') { line_end.saturating_sub(1) } else { line_end }, extend, window, cx),
            "up" | "down" => {
                let column = text[line_start..head].chars().count();
                let (start, end) = if key == "up" {
                    let end = line_start.saturating_sub(1);
                    (text[..end].rfind('\n').map_or(0, |byte| byte + 1), end)
                } else {
                    let start = (line_end + 1).min(text.len());
                    (start, text[start..].find('\n').map_or(text.len(), |byte| start + byte))
                };
                let end = if text[start..end].ends_with('\r') { end - 1 } else { end };
                let next = text[start..end].char_indices().nth(column).map_or(end, |(byte, _)| start + byte);
                self.select(next, extend, window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl Render for Field {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selector = self.selector;
        let read_only = self.read_only;
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let (label_key, fallback) = if selector == "hash-files-paths" { ("matrix-hash-file-paths", "File paths (read only)") }
            else if read_only { ("matrix-hash-digest-result", "Digest result (read only)") }
            else { ("matrix-hash-source-text", "Digest source text") };
        let label = crate::localization::localized_string(&self.locale, label_key).unwrap_or(fallback);
        div().id(selector).debug_selector(move || selector.into())
            .role(Role::TextInput).aria_label(label)
            .aria_value(self.state.display_text()).track_focus(&self.focus)
            .a11y_synthetic_children(move |builder| { if read_only { builder.parent_node().set_read_only(); } })
            .key_context("MatrixTextField").w_full().h(px(96.)).border_1()
            .border_color(rgb(if self.focus.is_focused(window) { 0x0078d4 } else { 0x808080 }))
            .bg(rgb(if self.dark { 0x202020 } else { 0xffffff })).cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(|field, event: &gpui::MouseDownEvent, window, cx| {
                field.focus.focus(window, cx);
                if let Some(layout) = &field.layout {
                    if event.position.x >= layout.bounds.right() - px(12.) {
                        let fraction = f32::from(event.position.y - layout.bounds.top()) / f32::from(layout.bounds.size.height - px(12.));
                        field.first_line = ((fraction.clamp(0., 1.) * layout.row_count as f32) as usize).min(layout.row_count.saturating_sub(1));
                        cx.stop_propagation(); cx.notify(); return;
                    }
                    if event.position.y >= layout.bounds.bottom() - px(12.) {
                        let fraction = f32::from(event.position.x - layout.bounds.left()) / f32::from(layout.bounds.size.width - px(12.));
                        field.horizontal = (layout.content_width - layout.bounds.size.width + px(16.)).max(px(0.)) * fraction.clamp(0., 1.);
                        cx.stop_propagation(); cx.notify(); return;
                    }
                }
                if event.click_count > 1 {
                    let end = field.state.display_snapshot().len();
                    let result = field.state.select_display(Selection { anchor: ByteOffset(0), head: ByteOffset(end) });
                    field.changed(result, window, cx);
                } else if let Some(byte) = field.byte_at(event.position) {
                    field.select(byte, event.modifiers.shift, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|field, event: &gpui::MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some(byte) = field.byte_at(event.position)
                { field.select(byte, true, window, cx); }
            }))
            .on_scroll(cx.listener(|field, event: &gpui::ScrollEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(16.));
                let count = field.state.display_text().bytes().filter(|byte| *byte == b'\n').count();
                if delta.y < px(0.) { field.first_line = (field.first_line + 3).min(count); }
                if delta.y > px(0.) { field.first_line = field.first_line.saturating_sub(3); }
                let maximum = field.layout.as_ref().map_or(px(0.), |layout| {
                    (layout.content_width - layout.bounds.size.width + px(16.)).max(px(0.))
                });
                field.horizontal = (field.horizontal - delta.x).max(px(0.)).min(maximum);
                cx.stop_propagation(); cx.notify();
            }))
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|field, _: &EditorBackspace, window, cx| field.delete(false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorDelete, window, cx| field.delete(true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorLeft, window, cx| field.horizontal_move(false, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorRight, window, cx| field.horizontal_move(true, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorSelectLeft, window, cx| field.horizontal_move(false, true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorSelectRight, window, cx| field.horizontal_move(true, true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorDocumentStart, window, cx| {
                let text = field.state.display_text();
                let head = field.state.selection().head.0;
                let start = text[..head].rfind('\n').map_or(0, |byte| byte + 1);
                field.select(start, false, window, cx); cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorDocumentEnd, window, cx| {
                let text = field.state.display_text();
                let head = field.state.selection().head.0;
                let end = text[head..].find('\n').map_or(text.len(), |byte| head + byte);
                let end = if text[..end].ends_with('\r') { end.saturating_sub(1) } else { end };
                field.select(end, false, window, cx); cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorSelectAll, window, cx| {
                let end = field.state.display_snapshot().len();
                let result = field.state.select_display(Selection { anchor: ByteOffset(0), head: ByteOffset(end) });
                field.changed(result, window, cx); cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorCopy, window, cx| field.copy(false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorCut, window, cx| field.copy(true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorPaste, window, cx| {
                if !field.read_only && let Some(payload) = clipboard_surface::read_payload(cx) {
                    let text = payload.text.split('\0').next().unwrap_or_default();
                    let result = field.state.insert_text(text); field.changed(result, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorUndo, window, cx| {
                if !field.read_only { field.state.undo(); field.changed(Ok(()), window, cx); } cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorRedo, window, cx| {
                if !field.read_only { field.state.redo(); field.changed(Ok(()), window, cx); } cx.stop_propagation();
            }))
            .child(gpui::canvas(move |bounds, window, cx| {
                let field = entity.read(cx);
                let text = field.state.display_text();
                let mut start = 0;
                let mut rows = Vec::new();
                for (index, line) in text.split('\n').enumerate() {
                    if index >= field.first_line && rows.len() < 6 {
                        let visible = line.strip_suffix('\r').unwrap_or(line);
                        let run = TextRun { len: visible.len(), font: gpui::font(dialog_font::font_family()),
                            color: rgb(if field.dark { 0xf0f0f0 } else { 0x202020 }).into(),
                            background_color: None, underline: None, strikethrough: None };
                        let shaped = window.text_system().shape_line(visible.to_owned().into(), px(13.), &[run], None);
                        let origin = point(bounds.left() + px(3.) - field.horizontal,
                            bounds.top() + px(2. + rows.len() as f32 * 16.));
                        rows.push(Row { start, end: start + visible.len(), line: shaped, origin });
                    }
                    start += line.len() + 1;
                    if rows.len() == 6 { break; }
                }
                let content_width = rows.iter().map(|row| row.line.width()).fold(px(0.), |width, next| width.max(next));
                Layout { bounds, rows, revision: field.state.display_revision(),
                    row_count: text.bytes().filter(|byte| *byte == b'\n').count() + 1, content_width }
            }, move |bounds, layout, window, cx| {
                let field = paint_entity.read(cx);
                if layout.revision != field.state.display_revision() { return; }
                let focus = field.focus.clone();
                let dark = field.dark;
                let selection = field.state.selection();
                let selected = selection.range();
                window.handle_input(&focus, ElementInputHandler::new(bounds, paint_entity.clone()), cx);
                let viewport = Bounds::new(bounds.origin, size(bounds.size.width - px(12.), bounds.size.height - px(12.)));
                window.with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
                    for row in &layout.rows {
                        let from = selected.start.0.max(row.start).min(row.end);
                        let to = selected.end.0.min(row.end).max(row.start);
                        if from < to {
                            window.paint_quad(fill(Bounds::from_corners(
                                point(row.origin.x + row.line.x_for_index(from - row.start), row.origin.y),
                                point(row.origin.x + row.line.x_for_index(to - row.start), row.origin.y + px(16.))), gpui::rgba(0x0078d460)));
                        }
                        if let Err(error) = row.line.paint(row.origin, px(16.), gpui::TextAlign::Left, None, window, cx) {
                            eprintln!("Hash field paint failed: {error}");
                        }
                        if focus.is_focused(window) && row.start <= selection.head.0 && selection.head.0 <= row.end {
                            window.paint_quad(fill(Bounds::new(point(row.origin.x + row.line.x_for_index(selection.head.0 - row.start), row.origin.y),
                                size(px(1.), px(16.))), rgb(if dark { 0xffffff } else { 0x202020 })));
                        }
                    }
                });
                let track = if dark { 0x303030 } else { 0xf0f0f0 };
                let thumb = if dark { 0x707070 } else { 0xc0c0c0 };
                let vertical = Bounds::new(point(viewport.right(), bounds.top()), size(px(12.), viewport.size.height));
                let horizontal = Bounds::new(point(bounds.left(), viewport.bottom()), size(viewport.size.width, px(12.)));
                window.paint_quad(fill(vertical, rgb(track)));
                window.paint_quad(fill(horizontal, rgb(track)));
                let height = (viewport.size.height * (5. / layout.row_count.max(5) as f32)).max(px(12.));
                let first = paint_entity.read(cx).first_line;
                let y = (viewport.size.height - height) * (first as f32 / layout.row_count.saturating_sub(5).max(1) as f32).min(1.);
                window.paint_quad(fill(Bounds::new(point(viewport.right() + px(2.), bounds.top() + y), size(px(8.), height)), rgb(thumb)));
                let width = (viewport.size.width * (f32::from(viewport.size.width) / f32::from(layout.content_width.max(viewport.size.width)))).max(px(16.));
                let offset = paint_entity.read(cx).horizontal;
                let x = (viewport.size.width - width) * (f32::from(offset) / f32::from((layout.content_width - viewport.size.width).max(px(1.)))).min(1.);
                window.paint_quad(fill(Bounds::new(point(bounds.left() + x, viewport.bottom() + px(2.)), size(width, px(8.))), rgb(thumb)));
                paint_entity.update(cx, |field, _| field.layout = Some(layout));
            }).w_full().h_full())
            .when_some(self.error.clone(), |field, error| field.aria_description(error))
    }
}

impl EntityInputHandler for Field {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        !self.read_only && self.focus.is_focused(window) && !window.modifiers().control && !window.modifiers().alt
    }

    fn text_for_range(&mut self, range: Range<usize>, actual: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        *actual = None;
        let text = self.state.display_snapshot();
        let bytes = editor_surface::byte_range(&text, range.clone()).ok()?;
        *actual = Some(range);
        Some(text.slice(bytes.start.0..bytes.end.0).to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let text = self.state.display_snapshot();
        let selected = self.state.selection();
        let range = selected.range();
        Some(UTF16Selection { range: text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0), reversed: selected.head < selected.anchor })
    }

    fn set_selected_text_range(&mut self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let result = editor_surface::byte_range(&self.state.display_snapshot(), range)
            .and_then(|range| self.state.select_display(Selection { anchor: range.start, head: range.end }));
        self.changed(result, window, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> { Some(self.state.display_snapshot().len_utf16()) }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let text = self.state.display_snapshot();
        self.state.marked_range().map(|range| text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.state.commit_composition(); self.changed(result, window, cx);
    }

    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only { return; }
        let text = text.split('\0').next().unwrap_or_default();
        let result = if let Some(range) = range {
            editor_surface::byte_range(&self.state.display_snapshot(), range).and_then(|range| self.state.replace_display_range(range, text))
        } else { self.state.insert_text(text) };
        self.changed(result, window, cx);
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only { return; }
        let result = range.map(|range| editor_surface::byte_range(&self.state.display_snapshot(), range)).transpose()
            .and_then(|range| self.state.mark_display_range(range, text, selected));
        self.changed(result, window, cx);
    }

    fn bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let bytes = editor_surface::byte_range(&self.state.display_snapshot(), range).ok()?;
        let layout = self.layout.as_ref()?;
        let row = layout.rows.iter().find(|row| row.start <= bytes.start.0 && bytes.start.0 <= row.end)?;
        Some(Bounds::new(point(row.origin.x + row.line.x_for_index(bytes.start.0 - row.start), row.origin.y), size(px(1.), px(16.))))
    }

    fn selection_bounds_for_range(&mut self, range: Range<usize>, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range, bounds, window, cx).into_iter().collect()
    }

    fn character_index_for_point(&mut self, position: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.byte_at(position).map(|byte| self.state.display_snapshot().byte_to_utf16_idx(byte))
    }
}
