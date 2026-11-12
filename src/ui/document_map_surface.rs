use super::*;
use gpui::{
    ContentMask, MouseDownEvent, MouseMoveEvent, Pixels, ShapedLine, TextRun, canvas, fill, point,
    rgba,
};

#[path = "document_map_geometry.rs"]
pub(super) mod geometry;
use geometry::{MapGeometry, miniature_text};

#[derive(Default)]
pub(super) struct DocumentMapState {
    pub(super) visible: bool,
    bounds: Option<Bounds<Pixels>>,
    geometry: Option<MapGeometry>,
    painted_buffer: Option<crate::workspace::BufferId>,
    painted_view: Option<crate::workspace::ViewId>,
    drag: Option<MapDrag>,
}

#[derive(Clone, Copy)]
struct MapDrag {
    bounds: Bounds<Pixels>,
    geometry: MapGeometry,
    buffer: crate::workspace::BufferId,
    pane: crate::workspace::ViewId,
}

struct MapPaint {
    geometry: MapGeometry,
    lines: Vec<(ShapedLine, Pixels)>,
}

impl MatrixWindow {
    pub(super) fn document_map_visible(&self) -> bool {
        self.document_map.visible
    }

    pub(super) fn toggle_document_map(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.toggle_document_map(window, cx)
        }) {
            return;
        }
        if self.document_map.visible {
            self.close_document_map(window, cx);
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.document_map = DocumentMapState {
            visible: true,
            ..Default::default()
        };
        self.show_panel(crate::panel_layout::PanelId::DocumentMap, window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_document_map(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.close_document_map(window, cx)
        }) {
            return;
        }
        self.document_map = DocumentMapState::default();
        self.hide_panel(crate::panel_layout::PanelId::DocumentMap, window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    fn document_map_scroll_to(&mut self, top: usize, cx: &mut Context<Self>) {
        let total = self.editor_visual_row_count();
        self.editor_top_line = top.min(total.saturating_sub(self.editor_visible_capacity()));
        self.editor_follow_caret = false;
        self.editor_scroll_remainder = 0.;
        self.editor_painted_revision = None;
        cx.notify();
    }

    fn document_map_navigate(
        &mut self,
        top: usize,
        pane: crate::workspace::ViewId,
        buffer: crate::workspace::BufferId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.document_map_navigate(top, pane, buffer, window, cx)
        }) {
            return;
        }
        if self.workspace.active_view() != pane || self.workspace.active().id() != buffer {
            return;
        }
        self.document_map_scroll_to(top, cx);
        window.activate_window();
        self.editor_focus.focus(window, cx);
    }

    fn document_map_pointer_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let (Some(bounds), Some(geometry), Some(buffer), Some(pane)) = (
            self.document_map.bounds,
            self.document_map.geometry,
            self.document_map.painted_buffer,
            self.document_map.painted_view,
        ) else {
            return;
        };
        if buffer != self.workspace.active().id()
            || pane != self.workspace.active_view()
            || !bounds.contains(&event.position)
        {
            return;
        }
        self.document_map.drag = Some(MapDrag {
            bounds,
            geometry,
            buffer,
            pane,
        });
        self.document_map_navigate(
            geometry.top_line_at(f32::from(event.position.y - bounds.top())),
            pane,
            buffer,
            window,
            cx,
        );
    }

    fn document_map_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.document_map.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left)
            || drag.buffer != self.workspace.active().id()
            || drag.pane != self.workspace.active_view()
        {
            self.document_map.drag = None;
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            self.document_map.drag = None;
            return;
        }
        let y = f32::from(event.position.y - drag.bounds.top());
        self.document_map_navigate(
            drag.geometry.top_line_at(y),
            drag.pane,
            drag.buffer,
            window,
            cx,
        );
        cx.stop_propagation();
    }

    pub(super) fn render_document_map(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.document_map.visible {
            return None;
        }
        let owner = cx.entity();
        let dark = self.dark;
        let preview = canvas(
            move |bounds, window, cx| {
                owner.update(cx, |view, _| {
                    let snapshot = view.editor().display_snapshot();
                    let geometry = MapGeometry::new(
                        view.editor_visual_row_count(),
                        view.editor_visible_capacity(),
                        view.editor_top_line,
                        f32::from(bounds.size.height),
                    );
                    view.document_map.bounds = Some(bounds);
                    view.document_map.geometry = geometry;
                    view.document_map.painted_buffer = Some(view.workspace.active().id());
                    view.document_map.painted_view = Some(view.workspace.active_view());
                    geometry.map(|geometry| {
                        let columns =
                            ((f32::from(bounds.size.width) / 1.5).ceil() as usize).min(512);
                        let lines = (geometry.first_line()
                            ..geometry.first_line() + geometry.rows())
                            .map(|row| {
                                let range =
                                    view.editor_visual_row_source_range(row).unwrap_or(0..0);
                                let text = miniature_text(snapshot.slice(range).chars(), columns);
                                let indent = view.editor_visual_row_indent(row)
                                    * (geometry.line_height() * 0.85
                                        / f32::from(view.editor_font_size()));
                                let run = TextRun {
                                    len: text.len(),
                                    font: gpui::font("Menlo"),
                                    color: rgb(if dark { 0xd4d4d4 } else { 0x303030 }).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                };
                                (
                                    window.text_system().shape_line(
                                        text.into(),
                                        px(geometry.line_height() * 0.85),
                                        &[run],
                                        None,
                                    ),
                                    indent,
                                )
                            })
                            .collect();
                        MapPaint { geometry, lines }
                    })
                })
            },
            move |bounds, paint, window, cx| {
                let Some(MapPaint { geometry, lines }) = paint else {
                    return;
                };
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    let (top, height) = geometry.viewport();
                    let zone = Bounds::new(
                        point(bounds.left(), bounds.top() + px(top)),
                        size(bounds.size.width, px(height)),
                    );
                    window.paint_quad(fill(zone, rgba(if dark { 0xff800050 } else { 0xff800038 })));
                    for (index, (line, indent)) in lines.into_iter().enumerate() {
                        if let Err(error) = line.paint(
                            point(
                                bounds.left() + px(2.) + indent,
                                bounds.top() + px(index as f32 * geometry.line_height()),
                            ),
                            px(geometry.line_height()),
                            gpui::TextAlign::Left,
                            None,
                            window,
                            cx,
                        ) {
                            eprintln!("Document Map text paint failed: {error}");
                        }
                    }
                    let edge = rgb(0xff8000);
                    window.paint_quad(fill(
                        Bounds::new(zone.origin, size(zone.size.width, px(1.))),
                        edge,
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(zone.left(), zone.bottom() - px(1.)),
                            size(zone.size.width, px(1.)),
                        ),
                        edge,
                    ));
                });
            },
        )
        .size_full();
        Some(
            div()
                .id("document-map-panel")
                .debug_selector(|| "document-map-panel".into())
                .role(Role::Group)
                .aria_label("Document Map")
                .w_full()
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(rgb(if dark { 0x282828 } else { 0xf6f6f6 }))
                .border_l_1()
                .border_color(rgb(0x808080))
                .child(
                    div()
                        .h(px(26.))
                        .flex_shrink_0()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("Document Map")
                        .child(
                            div()
                                .id("document-map-close")
                                .debug_selector(|| "document-map-close".into())
                                .role(Role::Button)
                                .aria_label("Close Document Map")
                                .tooltip(move |_, cx| {
                                    icon_tooltip::view("Close Document Map", dark, cx)
                                })
                                .cursor_pointer()
                                .child("×")
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_document_map(window, cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("document-map-preview")
                        .debug_selector(|| "document-map-preview".into())
                        .role(Role::Group)
                        .aria_label(
                            "Document miniature; click or drag to scroll without moving selection",
                        )
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .overflow_hidden()
                        .cursor_pointer()
                        .bg(rgb(if dark { 0x1e1e1e } else { 0xffffff }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::document_map_pointer_down),
                        )
                        .on_mouse_move(cx.listener(Self::document_map_pointer_move))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| {
                                view.document_map.drag = None;
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|view, _, _, _| view.document_map.drag = None),
                        )
                        .on_scroll(cx.listener(|view, event: &gpui::ScrollEvent, _, cx| {
                            cx.stop_propagation();
                            if view.file_busy
                                || view.close_dialog
                                || view.save_all_dialog
                                || view.batch_editing_blocked()
                            {
                                return;
                            }
                            let delta =
                                f32::from(event.delta.pixel_delta(view.editor_line_height()).y);
                            if !delta.is_finite() {
                                return;
                            }
                            view.editor_scroll_remainder += delta;
                            let rows = (view.editor_scroll_remainder
                                / f32::from(view.editor_line_height()))
                            .trunc();
                            let remainder =
                                view.editor_scroll_remainder % f32::from(view.editor_line_height());
                            if rows == 0. {
                                return;
                            }
                            let top = if rows > 0. {
                                view.editor_top_line.saturating_sub(rows as usize)
                            } else {
                                view.editor_top_line.saturating_add((-rows) as usize)
                            };
                            view.document_map_scroll_to(top, cx);
                            view.editor_scroll_remainder = remainder;
                        }))
                        .child(preview),
                )
                .into_any_element(),
        )
    }
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;
    use gpui::{EntityInputHandler, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point};

    #[gpui::test]
    fn document_map_uses_wrapped_rows_for_preview_and_scroll(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let source = (0..100)
            .map(|row| format!("{row} {}\n", "word ".repeat(80)))
            .collect::<String>();
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, &source, window, cx);
            view.toggle_editor_word_wrap(window, cx);
            view.editor_top_line = 0;
            view.editor_follow_caret = false;
            view.toggle_document_map(window, cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.document_map.geometry.expect("map projection").rows() > 101,
                "the wrapped miniature must include continuation rows, not just logical lines"
            );
        });
        let map = cx
            .debug_bounds("document-map-preview")
            .expect("document miniature");
        cx.simulate_click(
            point(map.center().x, map.bottom() - px(2.)),
            gpui::Modifiers::default(),
        );
        view.read_with(cx, |view, _| {
            assert!(
                view.editor_top_line > 100,
                "map scrolling must not clamp visual rows to the logical line count"
            );
            assert_eq!(view.editor().committed_text(), source);
        });
    }

    #[gpui::test]
    fn document_map_click_drag_and_close_preserve_document_selection_and_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let source = (0..400)
            .map(|row| format!("line {row}: real 中 text\n"))
            .collect::<String>();
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, &source, window, cx);
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(4),
                    head: ByteOffset(1),
                })
                .expect("backward selection");
            view.editor_top_line = 0;
            view.editor_follow_caret = false;
            view.toggle_document_map(window, cx);
        });
        cx.run_until_parked();
        let selection = view.read_with(cx, |view, _| view.editor().selection());
        let count = view.read_with(cx, |view, _| view.workspace.buffers().len());
        let map = cx
            .debug_bounds("document-map-preview")
            .expect("bounded map");
        let click = point(map.center().x, map.top() + map.size.height * 0.6);
        cx.simulate_click(click, gpui::Modifiers::default());
        let clicked_top = view.read_with(cx, |view, _| view.editor_top_line);
        assert!(clicked_top > 0, "click scrolls rather than selecting text");
        cx.simulate_event(MouseDownEvent {
            position: click,
            button: MouseButton::Left,
            ..Default::default()
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(click.x, map.bottom() - px(2.)),
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(MouseUpEvent {
            position: point(click.x, map.bottom() - px(2.)),
            button: MouseButton::Left,
            ..Default::default()
        });
        view.read_with(cx, |view, _| {
            assert!(view.editor_top_line > clicked_top, "drag scrolls farther");
            assert_eq!(view.editor().selection(), selection);
            assert_eq!(view.editor().committed_text(), source);
            assert!(view.editor().document().can_undo());
            assert!(!view.editor().document().can_redo());
            assert!(
                !view.editor_follow_caret,
                "repaint must not snap back to caret"
            );
        });
        let close = cx
            .debug_bounds("document-map-close")
            .expect("panel close")
            .center();
        cx.simulate_click(close, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("document-map-panel").is_none());
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.buffers().len(), count);
            assert_eq!(view.editor().selection(), selection);
            assert_eq!(view.editor().committed_text(), source);
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().committed_text(),
                "",
                "one undo removes the original edit"
            );
        });
    }
}
