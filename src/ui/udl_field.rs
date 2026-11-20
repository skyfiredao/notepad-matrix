use super::super::*;
use super::super::dialog_font::{self, line_height, px};
use gpui::{ContentMask, ElementInputHandler, EntityInputHandler, Pixels, Point, ShapedLine, TextRun, UTF16Selection, fill, point};
use std::ops::Range;

pub(super) struct Field {
    state: EditorSession,
    focus: FocusHandle,
    label: &'static str,
    selector: &'static str,
    multiline: bool,
    want_return: bool,
    height: f32,
    first_row: usize,
    horizontal: Pixels,
    reveal: bool,
    preferred_x: Option<Pixels>,
    row_hint: Option<usize>,
    dragging_scrollbar: bool,
    layout: Option<Layout>,
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
    viewport: Bounds<Pixels>,
    rows: Vec<Row>,
    first_row: usize,
    visible_rows: usize,
    horizontal: Pixels,
    caret_row: Option<usize>,
    revision: crate::input::DisplayRevision,
}

impl Layout {
    fn row_at(&self, byte: usize) -> Option<usize> {
        self.rows.iter().rposition(|row| row.start <= byte && byte <= row.end)
    }

    fn maximum_scroll(&self) -> usize {
        self.rows.len().saturating_sub(self.visible_rows)
    }

    fn row_for_point(&self, position: Point<Pixels>) -> usize {
        ((f32::from(position.y - self.viewport.top()) / f32::from(line_height())).floor() as isize
            + self.first_row as isize).clamp(0, self.rows.len().saturating_sub(1) as isize) as usize
    }
}

impl Field {
    pub(super) fn new(label: &'static str, selector: &'static str, multiline: bool, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<text_field::AppFieldPalette>(|field, cx| {
            field.layout = None;
            cx.notify();
        }).detach();
        Self {
            state: EditorSession::new(""),
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            label,
            selector,
            multiline,
            want_return: multiline,
            height: if multiline { 72. } else { 21. },
            first_row: 0,
            horizontal: px(0.),
            reveal: true,
            preferred_x: None,
            row_hint: None,
            dragging_scrollbar: false,
            layout: None,
            error: None,
        }
    }

    pub(super) fn text(&self) -> String { self.state.committed_text() }

    pub(super) fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.state.committed_text() == text { return; }
        self.state = EditorSession::new(text);
        self.first_row = 0;
        self.horizontal = px(0.);
        self.preferred_x = None;
        self.row_hint = None;
        self.reveal = true;
        self.layout = None;
        self.error = None;
        cx.notify();
    }

    pub(super) fn set_label(&mut self, label: &'static str, cx: &mut Context<Self>) {
        if self.label != label { self.label = label; cx.notify(); }
    }

    pub(super) fn set_height(&mut self, height: f32, cx: &mut Context<Self>) {
        if self.height != height {
            self.height = height;
            self.layout = None;
            self.reveal = true;
            cx.notify();
        }
    }

    pub(super) fn has_composition(&self) -> bool { self.state.has_composition() }

    pub(super) fn set_want_return(&mut self, want_return: bool, cx: &mut Context<Self>) {
        if self.want_return != want_return { self.want_return = want_return; cx.notify(); }
    }


    fn palette(cx: &App) -> text_field::RenderPalette {
        cx.try_global::<text_field::AppFieldPalette>().and_then(|palette| palette.0)
            .unwrap_or(text_field::RenderPalette::LIGHT)
    }

    fn current_layout(&self) -> Option<&Layout> {
        self.layout.as_ref().filter(|layout| layout.revision == self.state.display_revision())
    }

    fn changed(&mut self, result: Result<(), crate::editor::EditError>, window: &mut Window, cx: &mut Context<Self>) {
        self.error = result.err().map(|error| error.to_string());
        self.reveal = true;
        self.preferred_x = None;
        self.row_hint = None;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn input_text<'a>(&self, text: &'a str) -> &'a str {
        if self.multiline { text }
        else { text.split(['\r', '\n']).next().unwrap_or_default() }
    }

    fn byte_at(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.current_layout()?;
        let row = layout.rows.get(layout.row_for_point(position))?;
        Some((row.start + row.line.closest_index_for_x(position.x - row.origin.x)).min(row.end))
    }

    fn select_at(&mut self, position: Point<Pixels>, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.current_layout().map(|layout| layout.row_for_point(position));
        if let Some(byte) = self.byte_at(position) {
            self.select(byte, extend, window, cx);
            self.row_hint = row;
        }
    }

    fn select(&mut self, head: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = if extend { self.state.selection().anchor } else { ByteOffset(head) };
        let result = self.state.select_display(Selection { anchor, head: ByteOffset(head) });
        self.changed(result, window, cx);
    }

    fn adjacent(&self, head: usize, right: bool) -> usize {
        let text = self.state.display_text();
        if right {
            if text[head..].starts_with("\r\n") { head + 2 }
            else { text[head..].chars().next().map_or(head, |ch| head + ch.len_utf8()) }
        } else if text[..head].ends_with("\r\n") { head - 2 }
        else { text[..head].char_indices().next_back().map_or(0, |(byte, _)| byte) }
    }

    fn horizontal_move(&mut self, right: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.state.selection();
        let range = selection.range();
        let next = if !extend && !range.is_empty() {
            if right { range.end.0 } else { range.start.0 }
        } else { self.adjacent(selection.head.0, right) };
        self.select(next, extend, window, cx);
        cx.stop_propagation();
    }

    fn vertical_move(&mut self, down: bool, page: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.current_layout() else { return; };
        let head = self.state.selection().head.0;
        let Some(index) = layout.caret_row else { return; };
        let row = &layout.rows[index];
        let x = self.preferred_x.unwrap_or_else(|| row.line.x_for_index(head - row.start));
        let distance = if page { layout.visible_rows.max(1) } else { 1 };
        let target = if down { (index + distance).min(layout.rows.len() - 1) }
            else { index.saturating_sub(distance) };
        let row = &layout.rows[target];
        let next = (row.start + row.line.closest_index_for_x(x)).min(row.end);
        self.select(next, extend, window, cx);
        self.preferred_x = Some(x);
        self.row_hint = Some(target);
    }

    fn line_edge(&mut self, end: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let next = self.current_layout().and_then(|layout| layout.caret_row.map(|index| {
            let row = &layout.rows[index];
            (if end { row.end } else { row.start }, index)
        }));
        if let Some((next, index)) = next {
            self.select(next, extend, window, cx);
            self.row_hint = Some(index);
        }
        else {
            let result = self.state.move_to_line_edge(end, extend);
            self.changed(result, window, cx);
        }
        cx.stop_propagation();
    }

    fn delete(&mut self, right: bool, word: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut range = self.state.selection().range();
        if range.is_empty() && word {
            let result = self.state.move_word(right, true);
            if let Err(error) = result { self.changed(Err(error), window, cx); return; }
            range = self.state.selection().range();
        } else if range.is_empty() {
            if right { range.end = ByteOffset(self.adjacent(range.end.0, true)); }
            else { range.start = ByteOffset(self.adjacent(range.start.0, false)); }
        }
        let result = self.state.replace_display_range(range, "");
        self.changed(result, window, cx);
        cx.stop_propagation();
    }

    fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = self.state.display_snapshot().len();
        let result = self.state.select_display(Selection { anchor: ByteOffset(0), head: ByteOffset(end) });
        self.changed(result, window, cx);
        cx.stop_propagation();
    }

    fn copy(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.state.selection().range();
        if !range.is_empty() {
            let text = self.state.display_snapshot().slice(range.start.0..range.end.0).to_string();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            if cut {
                let result = self.state.replace_display_range(range, "");
                self.changed(result, window, cx);
            }
        }
        cx.stop_propagation();
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(payload) = clipboard_surface::read_payload(cx) {
            let text = self.input_text(&payload.text);
            let result = self.state.insert_text(text);
            self.changed(result, window, cx);
        }
        cx.stop_propagation();
    }

    fn scroll_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(layout) = self.current_layout() {
            let fraction = f32::from(position.y - layout.viewport.top()) / f32::from(layout.viewport.size.height.max(px(1.)));
            self.first_row = (fraction.clamp(0., 1.) * layout.maximum_scroll() as f32).round() as usize;
            self.reveal = false;
            cx.notify();
        }
    }

    fn key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if self.state.has_composition() {
            if key == "escape" {
                self.state.cancel_composition();
                self.changed(Ok(()), window, cx);
                cx.stop_propagation();
            }
            return;
        }
        let extend = modifiers.shift;
        if modifiers.platform || modifiers.control {
            match key {
                "a" => self.select_all(window, cx),
                "c" => self.copy(false, window, cx),
                "x" => self.copy(true, window, cx),
                "v" => self.paste(window, cx),
                "z" => {
                    if extend { self.state.redo(); } else { self.state.undo(); }
                    self.changed(Ok(()), window, cx);
                }
                "y" => { self.state.redo(); self.changed(Ok(()), window, cx); }
                "home" | "end" | "up" | "down" => {
                    let result = self.state.move_to_document_edge(matches!(key, "end" | "down"), extend);
                    self.changed(result, window, cx);
                }
                "left" | "right" if modifiers.platform => self.line_edge(key == "right", extend, window, cx),
                "left" | "right" => {
                    let result = self.state.move_word(key == "right", extend);
                    self.changed(result, window, cx);
                }
                "backspace" | "delete" => {
                    if modifiers.platform {
                        if self.state.selection().range().is_empty() {
                            self.line_edge(key == "delete", true, window, cx);
                        }
                        self.delete(key == "delete", false, window, cx);
                    } else { self.delete(key == "delete", true, window, cx); }
                }
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if modifiers.alt {
            match key {
                "left" | "right" => {
                    let result = self.state.move_word(key == "right", extend);
                    self.changed(result, window, cx);
                }
                "backspace" | "delete" => self.delete(key == "delete", true, window, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        match key {
            "enter" => {
                if self.multiline && self.want_return {
                    let result = self.state.insert_text("\r\n");
                    self.changed(result, window, cx);
                }
            }
            "up" | "down" | "pageup" | "pagedown" => self.vertical_move(
                matches!(key, "down" | "pagedown"), matches!(key, "pageup" | "pagedown"), extend, window, cx),
            "home" | "end" => self.line_edge(key == "end", extend, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn shape(&self, text: &str, color: u32, window: &Window) -> ShapedLine {
        let run = TextRun { len: text.len(), font: gpui::font(dialog_font::font_family()),
            color: rgb(color).into(), background_color: None, underline: None, strikethrough: None };
        window.text_system().shape_line(text.to_owned().into(), gpui::px(dialog_font::base_size()), &[run], None)
    }

    fn append_rows(&self, text: &str, start: usize, width: Pixels, color: u32, rows: &mut Vec<Row>, window: &Window) {
        let mut offset = 0;
        loop {
            let remaining = &text[offset..];
            let shaped = self.shape(remaining, color, window);
            let mut end = remaining.len();
            if self.multiline && shaped.width() > width {
                let mut fit = 0;
                for (byte, ch) in remaining.char_indices() {
                    let next = byte + ch.len_utf8();
                    if shaped.x_for_index(next) > width { break; }
                    fit = next;
                }
                if fit == 0 { fit = remaining.chars().next().map_or(0, char::len_utf8); }
                end = unicode_linebreak::linebreaks(remaining).map(|(byte, _)| byte)
                    .take_while(|byte| *byte <= fit).last().unwrap_or(fit);
            }
            let line = if end == remaining.len() { shaped } else { self.shape(&remaining[..end], color, window) };
            rows.push(Row { start: start + offset, end: start + offset + end, line, origin: point(px(0.), px(0.)) });
            offset += end;
            if offset >= text.len() { break; }
        }
    }

    fn make_layout(&self, bounds: Bounds<Pixels>, window: &Window, cx: &App) -> Layout {
        let viewport = Bounds::new(point(bounds.left() + px(4.), bounds.top() + px(3.)),
            size((bounds.size.width - px(if self.multiline { 18. } else { 6. })).max(px(1.)),
                (bounds.size.height - px(6.)).max(px(1.))));
        let visible_rows = ((f32::from(viewport.size.height) / f32::from(line_height())).floor() as usize).max(1);
        let text = self.state.display_text();
        let mut rows = Vec::new();
        let mut start = 0;
        for (byte, ch) in text.char_indices() {
            if ch == '\r' || ch == '\n' && !text[..byte].ends_with('\r') {
                self.append_rows(&text[start..byte], start, viewport.size.width, Self::palette(cx).text, &mut rows, window);
                start = byte + ch.len_utf8();
                if ch == '\r' && text[start..].starts_with('\n') { start += 1; }
            }
        }
        self.append_rows(&text[start..], start, viewport.size.width, Self::palette(cx).text, &mut rows, window);
        let mut layout = Layout { bounds, viewport, rows, first_row: self.first_row, visible_rows,
            horizontal: self.horizontal, caret_row: None, revision: self.state.display_revision() };
        layout.first_row = layout.first_row.min(layout.maximum_scroll());
        let head = self.state.selection().head.0;
        layout.caret_row = self.row_hint.filter(|index| layout.rows.get(*index)
            .is_some_and(|row| row.start <= head && head <= row.end)).or_else(|| layout.row_at(head));
        if self.reveal && let Some(index) = layout.caret_row {
            if index < layout.first_row { layout.first_row = index; }
            if index >= layout.first_row + visible_rows { layout.first_row = index + 1 - visible_rows; }
            if !self.multiline {
                let row = &layout.rows[index];
                let x = row.line.x_for_index(head - row.start);
                layout.horizontal = layout.horizontal.min(x).max(x - viewport.size.width + px(1.)).max(px(0.));
            }
        }
        if self.multiline { layout.horizontal = px(0.); }
        for (index, row) in layout.rows.iter_mut().enumerate() {
            row.origin = point(viewport.left() - layout.horizontal,
                viewport.top() + line_height() * (index as f32 - layout.first_row as f32));
        }
        layout
    }
}

impl Render for Field {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selector = self.selector;
        let palette = Self::palette(cx);
        let entity = cx.entity();
        let paint_entity = entity.clone();
        div().id(selector).debug_selector(move || selector.into())
            .role(if self.multiline { Role::MultilineTextInput } else { Role::TextInput }).aria_label(self.label).aria_value(self.state.display_text())
            .track_focus(&self.focus).key_context("MatrixTextField")
            .w_full().h(px(self.height)).min_w(px(0.)).flex_shrink_0().border_1()
            .border_color(rgb(if self.focus.is_focused(window) { palette.hot_edge } else { palette.edge }))
            .bg(rgb(palette.softer_background)).cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(|field, event: &gpui::MouseDownEvent, window, cx| {
                field.focus.focus(window, cx);
                field.dragging_scrollbar = field.multiline && field.current_layout()
                    .is_some_and(|layout| layout.maximum_scroll() > 0 && event.position.x >= layout.bounds.right() - px(12.));
                if field.dragging_scrollbar { field.scroll_to(event.position, cx); }
                else if event.click_count >= 3 { field.select_all(window, cx); }
                else {
                    field.select_at(event.position, event.modifiers.shift, window, cx);
                    if event.click_count == 2 {
                        let result = field.state.move_word(false, false).and_then(|_| field.state.move_word(true, true));
                        field.changed(result, window, cx);
                    }
                }
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|field, event: &gpui::MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    if field.dragging_scrollbar { field.scroll_to(event.position, cx); }
                    else { field.select_at(event.position, true, window, cx); }
                    cx.stop_propagation();
                } else { field.dragging_scrollbar = false; }
            }))
            .on_scroll(cx.listener(|field, event: &gpui::ScrollEvent, _, cx| {
                if !field.multiline { return; }
                let delta = event.delta.pixel_delta(line_height());
                if let Some(layout) = field.current_layout() {
                    let distance = (f32::from(delta.y).abs() / f32::from(line_height())).ceil() as usize;
                    field.first_row = if delta.y < px(0.) { (layout.first_row + distance).min(layout.maximum_scroll()) }
                        else { layout.first_row.saturating_sub(distance) };
                    field.reveal = false;
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|field, _: &EditorBackspace, window, cx| field.delete(false, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorDelete, window, cx| field.delete(true, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorLeft, window, cx| field.horizontal_move(false, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorRight, window, cx| field.horizontal_move(true, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorSelectLeft, window, cx| field.horizontal_move(false, true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorSelectRight, window, cx| field.horizontal_move(true, true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorDocumentStart, window, cx| field.line_edge(false, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorDocumentEnd, window, cx| field.line_edge(true, false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorSelectAll, window, cx| field.select_all(window, cx)))
            .on_action(cx.listener(|field, _: &EditorCopy, window, cx| field.copy(false, window, cx)))
            .on_action(cx.listener(|field, _: &EditorCut, window, cx| field.copy(true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorPaste, window, cx| field.paste(window, cx)))
            .on_action(cx.listener(|field, _: &EditorUndo, window, cx| {
                field.state.undo(); field.changed(Ok(()), window, cx); cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorRedo, window, cx| {
                field.state.redo(); field.changed(Ok(()), window, cx); cx.stop_propagation();
            }))
            .child(gpui::canvas(move |bounds, window, cx| entity.read(cx).make_layout(bounds, window, cx),
                move |bounds, layout, window, cx| {
                    let field = paint_entity.read(cx);
                    if layout.revision != field.state.display_revision() { return; }
                    let focus = field.focus.clone();
                    let selection = field.state.selection();
                    let selected = selection.range();
                    let marked = field.state.marked_range();
                    let caret_row = layout.caret_row;
                    let multiline = field.multiline;
                    let palette = Self::palette(cx);
                    window.handle_input(&focus, ElementInputHandler::new(bounds, paint_entity.clone()), cx);
                    window.with_content_mask(Some(ContentMask { bounds: layout.viewport }), |window| {
                        for (index, row) in layout.rows.iter().enumerate().skip(layout.first_row).take(layout.visible_rows + 1) {
                            let from = selected.start.0.max(row.start).min(row.end);
                            let to = selected.end.0.min(row.end).max(row.start);
                            if from < to || selected.start.0 <= row.end && selected.end.0 > row.end {
                                let right = if selected.end.0 > row.end { row.line.width() + px(3.) }
                                    else { row.line.x_for_index(to - row.start) };
                                window.paint_quad(fill(Bounds::from_corners(
                                    point(row.origin.x + row.line.x_for_index(from - row.start), row.origin.y),
                                    point(row.origin.x + right, row.origin.y + line_height())), gpui::rgba(0x0078d460)));
                            }
                            if let Err(error) = row.line.paint(row.origin, line_height(), gpui::TextAlign::Left, None, window, cx) {
                                eprintln!("UDL field paint failed: {error}");
                            }
                            if let Some(marked) = &marked {
                                let from = marked.start.0.max(row.start);
                                let to = marked.end.0.min(row.end);
                                if from < to {
                                    window.paint_quad(fill(Bounds::new(
                                        point(row.origin.x + row.line.x_for_index(from - row.start), row.origin.y + line_height() - px(1.)),
                                        size(row.line.x_for_index(to - row.start) - row.line.x_for_index(from - row.start), px(1.))), rgb(palette.text)));
                                }
                            }
                            if focus.is_focused(window) && caret_row == Some(index) {
                                window.paint_quad(fill(Bounds::new(
                                    point(row.origin.x + row.line.x_for_index(selection.head.0 - row.start), row.origin.y),
                                    size(px(1.), line_height())), rgb(palette.text)));
                            }
                        }
                    });
                    if multiline && layout.maximum_scroll() > 0 {
                        let track = Bounds::new(point(bounds.right() - px(12.), bounds.top()), size(px(12.), bounds.size.height));
                        window.paint_quad(fill(track, rgb(palette.pure_background)));
                        let height = (track.size.height * (layout.visible_rows as f32 / layout.rows.len().max(layout.visible_rows) as f32))
                            .max(px(8.)).min(track.size.height);
                        let y = (track.size.height - height) * (layout.first_row as f32 / layout.maximum_scroll().max(1) as f32);
                        window.paint_quad(fill(Bounds::new(point(track.left() + px(2.), track.top() + y),
                            size(px(8.), height)), rgb(palette.disabled_text)));
                    }
                    paint_entity.update(cx, |field, _| {
                        field.first_row = layout.first_row;
                        field.horizontal = layout.horizontal;
                        field.reveal = false;
                        field.layout = Some(layout);
                    });
                }).w_full().h_full())
            .when_some(self.error.clone(), |field, error| field.aria_description(error))
    }
}

impl EntityInputHandler for Field {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && !window.modifiers().control && !window.modifiers().platform
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
        let selection = self.state.selection();
        let range = selection.range();
        Some(UTF16Selection { range: text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0),
            reversed: selection.head < selection.anchor })
    }

    fn set_selected_text_range(&mut self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let result = editor_surface::byte_range(&self.state.display_snapshot(), range)
            .and_then(|range| self.state.select_display(Selection { anchor: range.start, head: range.end }));
        self.changed(result, window, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.state.display_snapshot().len_utf16())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let text = self.state.display_snapshot();
        self.state.marked_range().map(|range| text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.state.commit_composition();
        self.changed(result, window, cx);
    }

    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.input_text(text);
        if input.is_empty() && !text.is_empty() && !self.multiline { return; }
        let text = input;
        let result = if let Some(range) = range {
            editor_surface::byte_range(&self.state.display_snapshot(), range)
                .and_then(|range| self.state.replace_display_range(range, text))
        } else { self.state.insert_text(text) };
        self.changed(result, window, cx);
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_text(text);
        let length = text.encode_utf16().count();
        let selected = selected.map(|range| range.start.min(length)..range.end.min(length));
        let result = range.map(|range| editor_surface::byte_range(&self.state.display_snapshot(), range)).transpose()
            .and_then(|range| self.state.mark_display_range(range, text, selected));
        self.changed(result, window, cx);
    }

    fn bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let bytes = editor_surface::byte_range(&self.state.display_snapshot(), range).ok()?;
        let layout = self.current_layout()?;
        let row = &layout.rows[layout.row_at(bytes.start.0)?];
        let end = bytes.end.0.min(row.end);
        Some(Bounds::new(point(row.origin.x + row.line.x_for_index(bytes.start.0 - row.start), row.origin.y),
            size((row.line.x_for_index(end - row.start) - row.line.x_for_index(bytes.start.0 - row.start)).max(px(1.)), line_height())))
    }

    fn selection_bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Vec<Bounds<Pixels>> {
        let Ok(bytes) = editor_surface::byte_range(&self.state.display_snapshot(), range) else { return Vec::new(); };
        let Some(layout) = self.current_layout() else { return Vec::new(); };
        let caret_row = layout.row_at(bytes.start.0);
        layout.rows.iter().enumerate().skip(layout.first_row).take(layout.visible_rows + 1)
            .filter_map(|(index, row)| {
                let from = bytes.start.0.max(row.start);
                let to = bytes.end.0.min(row.end);
                if from > to || from == to && !(bytes.is_empty() && caret_row == Some(index)) { return None; }
                Some(Bounds::new(point(row.origin.x + row.line.x_for_index(from - row.start), row.origin.y),
                    size((row.line.x_for_index(to - row.start) - row.line.x_for_index(from - row.start)).max(px(1.)), line_height())))
            }).collect()
    }

    fn character_index_for_point(&mut self, position: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.byte_at(position).map(|byte| self.state.display_snapshot().byte_to_utf16_idx(byte))
    }
}
