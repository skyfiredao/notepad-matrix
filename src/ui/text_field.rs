use std::ops::Range;

use super::*;
use crate::editor::EditError;
use gpui::{
    ContentMask, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    GlobalElementId, LayoutId, Pixels, Point, ShapedLine, Style, TextRun, UTF16Selection, fill,
    point, relative,
};

pub(super) struct TextField {
    state: EditorSession,
    focus: FocusHandle,
    label: &'static str,
    selector: &'static str,
    layout: Option<FieldLayout>,
    error: Option<String>,
    compact: bool,
}

struct FieldLayout {
    line: ShapedLine,
    bounds: Bounds<Pixels>,
    offset: Pixels,
    revision: crate::input::DisplayRevision,
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", EditorBackspace, Some("MatrixTextField")),
        KeyBinding::new("delete", EditorDelete, Some("MatrixTextField")),
        KeyBinding::new("left", EditorLeft, Some("MatrixTextField")),
        KeyBinding::new("right", EditorRight, Some("MatrixTextField")),
        KeyBinding::new("shift-left", EditorSelectLeft, Some("MatrixTextField")),
        KeyBinding::new("shift-right", EditorSelectRight, Some("MatrixTextField")),
        KeyBinding::new("home", EditorDocumentStart, Some("MatrixTextField")),
        KeyBinding::new("end", EditorDocumentEnd, Some("MatrixTextField")),
        KeyBinding::new("ctrl-a", EditorSelectAll, Some("MatrixTextField")),
        KeyBinding::new("ctrl-c", EditorCopy, Some("MatrixTextField")),
        KeyBinding::new("ctrl-x", EditorCut, Some("MatrixTextField")),
        KeyBinding::new("ctrl-v", EditorPaste, Some("MatrixTextField")),
        KeyBinding::new("ctrl-z", EditorUndo, Some("MatrixTextField")),
        KeyBinding::new("ctrl-y", EditorRedo, Some("MatrixTextField")),
    ]
}

impl TextField {
    pub(super) fn new(label: &'static str, selector: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            state: EditorSession::new(""),
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            label,
            selector,
            layout: None,
            error: None,
            compact: false,
        }
    }
    pub(super) fn compact(
        label: &'static str,
        selector: &'static str,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut field = Self::new(label, selector, cx);
        field.compact = true;
        field
    }
    pub(super) fn text(&self) -> String {
        self.state.committed_text()
    }
    pub(super) fn has_composition(&self) -> bool {
        self.state.has_composition()
    }
    pub(super) fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.state = EditorSession::new(text);
        self.select_all(window, cx);
    }
    pub(super) fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.select_all(window, cx);
    }

    pub(super) fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    fn changed(
        &mut self,
        result: Result<(), EditError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = result.err().map(|error| error.to_string());
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = ByteOffset(self.state.display_snapshot().len());
        let result = self.state.select_display(Selection {
            anchor: ByteOffset(0),
            head: end,
        });
        self.changed(result, window, cx);
    }
    fn adjacent(&self, byte: usize, right: bool) -> usize {
        let text = self.state.display_snapshot();
        if right && byte < text.len() {
            byte + text.char(byte).len_utf8()
        } else if !right && byte > 0 {
            let mut previous = byte - 1;
            while !text.is_char_boundary(previous) {
                previous -= 1;
            }
            previous
        } else {
            byte
        }
    }
    fn move_cursor(
        &mut self,
        right: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut selection = self.state.selection();
        let range = selection.range();
        selection.head = if !extend && !range.is_empty() {
            if right { range.end } else { range.start }
        } else {
            ByteOffset(self.adjacent(selection.head.0, right))
        };
        if !extend {
            selection.anchor = selection.head;
        }
        let result = self.state.select_display(selection);
        self.changed(result, window, cx);
        cx.stop_propagation();
    }
    fn delete(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut range = self.state.selection().range();
        if range.is_empty() {
            if right {
                range.end = ByteOffset(self.adjacent(range.end.0, true));
            } else {
                range.start = ByteOffset(self.adjacent(range.start.0, false));
            }
        }
        let result = self.state.replace_display_range(range, "");
        self.changed(result, window, cx);
        cx.stop_propagation();
    }
    fn copy(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.state.selection().range();
        if !range.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                self.state.clipboard_text().text,
            ));
            if cut {
                let result = self.state.replace_display_range(range, "");
                self.changed(result, window, cx);
            }
        }
        cx.stop_propagation();
    }
    fn current_layout(&self) -> Option<&FieldLayout> {
        self.layout
            .as_ref()
            .filter(|layout| layout.revision == self.state.display_revision())
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selector = self.selector;
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .role(Role::TextInput)
            .aria_label(self.label)
            .aria_value(self.state.display_text())
            .track_focus(&self.focus)
            .tab_index(0)
            .key_context("MatrixTextField")
            .w_full()
            .h(px(if self.compact { 21. } else { 26. }))
            .px(px(4.))
            .when(self.compact, |field| field.pr(px(22.)))
            .border_1()
            .border_color(rgb(if self.compact {
                if self.focus.is_focused(window) {
                    0x0078d4
                } else {
                    0xadadad
                }
            } else {
                0x808080
            }))
            .bg(rgb(0xffffff))
            .text_color(rgb(0x202020))
            .font_family(if self.compact { "Helvetica" } else { "Menlo" })
            .text_size(px(if self.compact { 11. } else { 13. }))
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|field, event: &gpui::MouseDownEvent, window, cx| {
                    field.focus.focus(window, cx);
                    let byte = field
                        .current_layout()
                        .map(|layout| {
                            layout.line.closest_index_for_x(
                                event.position.x - layout.bounds.left() + layout.offset,
                            )
                        })
                        .unwrap_or_else(|| field.state.display_snapshot().len());
                    let result = field.state.select_display(Selection {
                        anchor: ByteOffset(byte),
                        head: ByteOffset(byte),
                    });
                    field.changed(result, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|field, _: &EditorBackspace, window, cx| {
                    field.delete(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|field, _: &EditorDelete, window, cx| field.delete(true, window, cx)),
            )
            .on_action(cx.listener(|field, _: &EditorLeft, window, cx| {
                field.move_cursor(false, false, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorRight, window, cx| {
                field.move_cursor(true, false, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorSelectLeft, window, cx| {
                field.move_cursor(false, true, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorSelectRight, window, cx| {
                field.move_cursor(true, true, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorSelectAll, window, cx| {
                field.select_all(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorDocumentStart, window, cx| {
                let result = field.state.move_to_document_edge(false, false);
                field.changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorDocumentEnd, window, cx| {
                let result = field.state.move_to_document_edge(true, false);
                field.changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|field, _: &EditorCopy, window, cx| field.copy(false, window, cx)),
            )
            .on_action(cx.listener(|field, _: &EditorCut, window, cx| field.copy(true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorPaste, window, cx| {
                if let Some(payload) = clipboard_surface::read_text(cx) {
                    let result = field.state.insert_text(&payload.text);
                    field.changed(result, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorUndo, window, cx| {
                field.state.undo();
                field.changed(Ok(()), window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorRedo, window, cx| {
                field.state.redo();
                field.changed(Ok(()), window, cx);
                cx.stop_propagation();
            }))
            .child(FieldElement { field: cx.entity() })
            .when_some(self.error.clone(), |field, error| {
                field.aria_description(error)
            })
    }
}

impl EntityInputHandler for TextField {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && !window.modifiers().control && !window.modifiers().alt
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *actual = None;
        let text = self.state.display_snapshot();
        let bytes = editor_surface::byte_range(&text, range.clone()).ok()?;
        *actual = Some(range);
        Some(text.slice(bytes.start.0..bytes.end.0).to_string())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.state.display_snapshot();
        let selected = self.state.selection();
        let range = selected.range();
        Some(UTF16Selection {
            range: text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0),
            reversed: selected.head < selected.anchor,
        })
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result =
            editor_surface::byte_range(&self.state.display_snapshot(), range).and_then(|range| {
                self.state.select_display(Selection {
                    anchor: range.start,
                    head: range.end,
                })
            });
        self.changed(result, window, cx);
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.state.display_snapshot().len_utf16())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if window.modifiers().control {
            return None;
        }
        let text = self.state.display_snapshot();
        self.state
            .marked_range()
            .map(|range| text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0))
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.state.commit_composition();
        self.changed(result, window, cx);
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = if let Some(range) = range {
            editor_surface::byte_range(&self.state.display_snapshot(), range)
                .and_then(|range| self.state.replace_display_range(range, text))
        } else {
            self.state.insert_text(text)
        };
        self.changed(result, window, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = range
            .map(|range| editor_surface::byte_range(&self.state.display_snapshot(), range))
            .transpose()
            .and_then(|range| self.state.mark_display_range(range, text, selected));
        self.changed(result, window, cx);
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bytes = editor_surface::byte_range(&self.state.display_snapshot(), range).ok()?;
        let layout = self.current_layout()?;
        Some(Bounds::from_corners(
            point(
                layout.bounds.left() - layout.offset + layout.line.x_for_index(bytes.start.0),
                layout.bounds.top(),
            ),
            point(
                layout.bounds.left() - layout.offset + layout.line.x_for_index(bytes.end.0),
                layout.bounds.bottom(),
            ),
        ))
    }
    fn selection_bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range, bounds, window, cx)
            .into_iter()
            .collect()
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.current_layout()?;
        if !layout.bounds.contains(&position) {
            return None;
        }
        let byte = layout
            .line
            .closest_index_for_x(position.x - layout.bounds.left() + layout.offset);
        Some(self.state.display_snapshot().byte_to_utf16_idx(byte))
    }
}

struct FieldElement {
    field: Entity<TextField>,
}
impl IntoElement for FieldElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<FieldLayout>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(if self.field.read(cx).compact {
            19.
        } else {
            24.
        })
        .into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<FieldLayout> {
        let field = self.field.read(cx);
        let text = field.state.display_text();
        let run = TextRun {
            len: text.len(),
            font: gpui::font(if field.compact { "Helvetica" } else { "Menlo" }),
            color: window.text_style().color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let selected = field.state.selection().range();
        let runs = if field.compact && field.focus.is_focused(window) && !selected.is_empty() {
            Some(
                [
                    (0..selected.start.0, run.color),
                    (selected.start.0..selected.end.0, rgb(0xffffff).into()),
                    (selected.end.0..text.len(), run.color),
                ]
                .into_iter()
                .filter(|(range, _)| !range.is_empty())
                .map(|(range, color)| TextRun {
                    len: range.len(),
                    color,
                    ..run.clone()
                })
                .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let line = window.text_system().shape_line(
            text.into(),
            px(if field.compact { 11. } else { 13. }),
            runs.as_deref().unwrap_or(std::slice::from_ref(&run)),
            None,
        );
        let offset = (line.x_for_index(field.state.selection().head.0) - bounds.size.width
            + px(3.))
        .max(px(0.));
        Some(FieldLayout {
            line,
            bounds,
            offset,
            revision: field.state.display_revision(),
        })
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        paint: &mut Option<FieldLayout>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layout) = paint.take() else {
            return;
        };
        let field = self.field.read(cx);
        if layout.revision != field.state.display_revision() {
            return;
        }
        let focus = field.focus.clone();
        let selection = field.state.selection();
        let range = selection.range();
        let line_height = px(if field.compact { 19. } else { 24. });
        let field_compact = field.compact;
        let focused = focus.is_focused(window);
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if !range.is_empty() && (!field_compact || focused) {
                window.paint_quad(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() - layout.offset + layout.line.x_for_index(range.start.0),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() - layout.offset + layout.line.x_for_index(range.end.0),
                            bounds.bottom(),
                        ),
                    ),
                    if field_compact {
                        gpui::rgba(0x0078d4ff)
                    } else {
                        gpui::rgba(0x0078d450)
                    },
                ));
            }
            if layout
                .line
                .paint(
                    point(bounds.left() - layout.offset, bounds.top()),
                    line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .is_err()
            {
                eprintln!("Text field paint failed");
            }
            if focus.is_focused(window) {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() - layout.offset
                                + layout.line.x_for_index(selection.head.0),
                            bounds.top() + px(2.),
                        ),
                        size(px(1.), px(20.)),
                    ),
                    rgb(0x202020),
                ));
            }
        });
        self.field
            .update(cx, |field, _| field.layout = Some(layout));
    }
}
