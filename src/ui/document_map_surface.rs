use super::*;
use gpui::{
    ContentMask, MouseDownEvent, MouseMoveEvent, Pixels, ShapedLine, TextRun, canvas, fill, point,
    rgba,
};

#[path = "document_map_geometry.rs"]
pub(super) mod geometry;
use geometry::{MapGeometry, MiniatureText, miniature_text};

#[derive(Default)]
pub(super) struct DocumentMapState {
    pub(super) visible: bool,
    bounds: Option<Bounds<Pixels>>,
    geometry: Option<MapGeometry>,
    painted_buffer: Option<crate::workspace::BufferId>,
    painted_view: Option<crate::workspace::ViewId>,
    painted_key: Option<MapKey>,
    cache: Option<(MapKey, usize, usize, Pixels, std::rc::Rc<Vec<(ShapedLine, Pixels)>>)>,
    drag: Option<MapDrag>,
    peek: Option<DocumentMapPeek>,
}

#[derive(Clone)]
struct DocumentMapPeek {
    buffer: crate::workspace::BufferId,
    key: MapKey,
    first_line: usize,
    name: String,
    text: ropey::Rope,
    dark: bool,
    locale: String,
}

impl Render for DocumentMapPeek {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let last = (self.first_line + 12).min(self.text.len_lines(ropey::LineType::LF_CR));
        let font = gpui::font("Menlo");
        let font_size = px(11.);
        let font_id = window.text_system().resolve_font(&font);
        let space_width = f32::from(window.text_system().layout_width(font_id, font_size, ' '));
        div()
            .id("document-map-peek")
            .role(Role::Tooltip)
            .aria_label(crate::localization::Text::resource("document-map-read-only-preview-label", "Read-only preview of $NAME$")
                .replace("$NAME$", self.name.clone()).render(&self.locale))
            .w(px(360.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_1()
            .border_color(rgb(0x808080))
            .bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
            .text_color(rgb(if self.dark { 0xd4d4d4 } else { 0x202020 }))
            .child(div().h(px(24.)).px(px(6.)).child(crate::localization::Text::resource("document-map-preview-title", "$NAME$ · Preview (read-only)")
                .replace("$NAME$", self.name.clone()).render(&self.locale)))
            .children((self.first_line..last).map(|index| {
                let line = miniature_text(self.text.line(index, ropey::LineType::LF_CR).chars(), self.key.tab_width, space_width, 318. + f32::from(font_size), 17.,
                    |ch| f32::from(window.text_system().layout_width(font_id, font_size, ch)), |miniature| {
                    let segment = &miniature.text;
                    f32::from(window.text_system().shape_line(segment.to_owned().into(), font_size, &[TextRun {
                        len: segment.len(), font: font.clone(), color: rgb(if self.dark { 0xd4d4d4 } else { 0x202020 }).into(),
                        background_color: None, underline: None, strikethrough: None,
                    }], None).width())
                }).text;
                div()
                    .id(("document-map-peek-line", index))
                    .h(px(17.))
                    .flex()
                    .font_family("Menlo")
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .child(div().w(px(42.)).flex_shrink_0().text_color(rgb(0x808080)).child(format!("{}", index + 1)))
                    .child(line)
            }))
    }
}

#[derive(Clone, PartialEq)]
struct MapKey {
    buffer: crate::workspace::BufferId,
    pane: crate::workspace::ViewId,
    revision: crate::input::DisplayRevision,
    editor_size: Option<gpui::Size<Pixels>>,
    wrap: bool,
    rows: usize,
    capacity: usize,
    font: String,
    font_size: Pixels,
    style_revision: u64,
    dark: bool,
    tab_width: usize,
    language: crate::syntax::Language,
    collapsed: Vec<usize>,
    hidden: Vec<(usize, usize)>,
    links: crate::links::LinkOptions,
    clickable_links: bool,
    syntax_highlighting: bool,
}

#[derive(Clone)]
struct MapDrag {
    bounds: Bounds<Pixels>,
    geometry: MapGeometry,
    buffer: crate::workspace::BufferId,
    pane: crate::workspace::ViewId,
    key: MapKey,
}

struct MapPaint {
    geometry: MapGeometry,
    lines: std::rc::Rc<Vec<(ShapedLine, Pixels)>>,
}

impl MatrixWindow {
    fn document_map_runs(&self, miniature: &MiniatureText, start: usize, run: TextRun) -> Vec<TextRun> {
        let end = start + miniature.source_len;
        let prefix = self.editor().display_snapshot().slice(start..end).to_string();
        let links = self.editor_link_spans_in_text(&prefix, start, 0..prefix.len());
        let mut offset = 0;
        let mut runs = Vec::new();
        for (mut style, _) in self.editor_style_runs_with_links(start, end, None, run.clone(), &links) {
            let end = (offset + style.len).min(miniature.source_len);
            style.len = miniature.display_offset(end).saturating_sub(miniature.display_offset(offset));
            if style.len > 0 { runs.push(style); }
            offset = end;
        }
        if runs.is_empty() { runs.push(run); }
        runs
    }

    pub(super) fn set_document_map_visible(&mut self, visible: bool) {
        self.document_map = DocumentMapState { visible, ..Default::default() };
    }

    fn document_map_key(&self) -> MapKey {
        MapKey {
            buffer: self.workspace.active().id(), pane: self.workspace.active_view(),
            revision: self.editor().display_revision(),
            editor_size: self.editor_bounds.map(|bounds| bounds.size),
            wrap: self.editor_word_wrap_enabled(), rows: self.editor_visual_row_count(),
            capacity: self.editor_visible_capacity(), font: self.editor_font_family(),
            font_size: self.editor_font_size(), style_revision: self.style_config.revision(),
            dark: self.dark, tab_width: usize::from(self.workspace.active_tab_width()),
            language: self.workspace.active().language(),
            collapsed: self.editor_collapsed_headers(),
            hidden: self.editor_hidden_lines(),
            links: self.preferences.files.links.clone(),
            clickable_links: self.workspace.clickable_links_allowed(),
            syntax_highlighting: self.workspace.syntax_highlighting_allowed(),
        }
    }

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
            || self.document_map.painted_key.as_ref() != Some(&self.document_map_key())
            || !bounds.contains(&event.position)
        {
            return;
        }
        self.document_map.drag = Some(MapDrag {
            bounds,
            geometry,
            buffer,
            pane,
            key: self.document_map_key(),
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
        let Some(drag) = self.document_map.drag.clone() else {
            let target = if self.pref_bool("docPeekOnMap", false) {
                self.document_map.bounds.zip(self.document_map.geometry)
                    .filter(|_| self.document_map.painted_key.as_ref() == Some(&self.document_map_key()))
                    .filter(|(bounds, _)| bounds.contains(&event.position))
                    .map(|(bounds, geometry)| {
                        let row = geometry.first_line() + (f32::from(event.position.y - bounds.top()) / geometry.line_height())
                            .floor().max(0.) as usize;
                        let row = row.min(geometry.first_line() + geometry.rows().saturating_sub(1));
                        (self.workspace.active().id(), self.editor_logical_line_for_visual_row(row))
                    })
            } else {
                None
            };
            if self.document_map.peek.as_ref().map(|peek| (peek.buffer, peek.first_line)) != target
                || self.document_map.peek.as_ref().is_some_and(|peek| peek.key != self.document_map_key())
            {
                self.document_map.peek = target.map(|(buffer, first_line)| DocumentMapPeek {
                    buffer,
                    key: self.document_map_key(),
                    first_line,
                    name: self.workspace.active().name(),
                    text: self.editor().display_snapshot(),
                    dark: self.dark,
                    locale: self.native_locale(),
                });
                cx.notify();
            }
            return;
        };
        if event.pressed_button != Some(MouseButton::Left)
            || drag.buffer != self.workspace.active().id()
            || drag.pane != self.workspace.active_view()
            || drag.key != self.document_map_key()
            || self.document_map.bounds != Some(drag.bounds)
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
            self.document_map.geometry.unwrap_or(drag.geometry).top_line_at(y),
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
        let locale = self.native_locale();
        let focus_colour = self.style_colour("Document map", "fgColor", 0xff8000);
        let frost_colour = self.style_colour("Document map", "bgColor", 0xffffff);
        let peek = self.document_map.peek.clone().filter(|peek| peek.key == self.document_map_key())
            .map(|mut peek| { peek.locale = locale.clone(); peek });
        let preview = canvas(
            move |bounds, window, cx| {
                owner.update(cx, |view, _| {
                    view.refresh_scintilla_shortcut_geometry(window);
                    let snapshot = view.editor().display_snapshot();
                    let key = view.document_map_key();
                    let base_size = f32::from(key.font_size) - view.editor_zoom_delta() as f32;
                    let font_size = px((base_size - 10.).max(1.));
                    let font = gpui::font(key.font.clone());
                    let font_id = window.text_system().resolve_font(&font);
                    let line_height = (f32::from(window.text_system().ascent(font_id, font_size)).floor()
                        + f32::from(window.text_system().descent(font_id, font_size)).abs().floor()).max(1.);
                    let previous_first = view.document_map.geometry
                        .filter(|_| view.document_map.painted_key.as_ref() == Some(&key))
                        .map_or(0, MapGeometry::first_line);
                    let geometry = MapGeometry::new(
                        view.editor_visual_row_count(),
                        view.editor_visible_capacity(),
                        view.editor_top_line,
                        f32::from(bounds.size.height),
                        line_height,
                        previous_first,
                        f32::from(bounds.size.width),
                    );
                    view.document_map.bounds = Some(bounds);
                    view.document_map.geometry = geometry;
                    view.document_map.painted_buffer = Some(view.workspace.active().id());
                    view.document_map.painted_view = Some(view.workspace.active_view());
                    view.document_map.painted_key = Some(key.clone());
                    geometry.map(|geometry| {
                        if let Some((cached_key, first, rows, width, lines)) = &view.document_map.cache
                            && *cached_key == key && *first == geometry.first_line() && *rows == geometry.rows() && *width == bounds.size.width
                        { return MapPaint { geometry, lines: lines.clone() }; }
                        let lines: Vec<_> = (geometry.first_line()
                            ..geometry.first_line() + geometry.rows())
                            .map(|row| {
                                let range = if key.wrap {
                                    view.editor_visual_row_source_range(row).unwrap_or(0..0)
                                } else {
                                    let logical = view.editor_logical_line_for_visual_row(row);
                                    let start = snapshot.line_to_byte_idx(logical, ropey::LineType::LF_CR);
                                    start..start + snapshot.line(logical, ropey::LineType::LF_CR).len()
                                };
                                let indent = view.editor_visual_row_indent(row)
                                    * (f32::from(font_size) / f32::from(view.editor_font_size()));
                                   let space_width = f32::from(window.text_system().layout_width(font_id, font_size, ' '));
                                   let width = geometry.text_width(f32::from(indent));
                                   let miniature = miniature_text(snapshot.slice(range.clone()).chars(), key.tab_width, space_width, if width > 0. { width + f32::from(font_size) } else { 0. }, geometry.line_height(),
                                       |ch| f32::from(window.text_system().layout_width(font_id, font_size, ch)), |miniature| {
                                       let run = TextRun {
                                           len: miniature.text.len(), font: font.clone(),
                                           color: rgb(view.editor_style_profile().foreground).into(),
                                           background_color: None, underline: None, strikethrough: None,
                                       };
                                       let runs = view.document_map_runs(miniature, range.start, run);
                                       f32::from(window.text_system().shape_line(miniature.text.clone().into(), font_size, &runs, None).width())
                                  });
                                 let run = TextRun {
                                    len: miniature.text.len(),
                                     font: font.clone(),
                                    color: rgb(view.editor_style_profile().foreground).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                };
                                let projected_runs = view.document_map_runs(&miniature, range.start, run);
                                (
                                    window.text_system().shape_line(
                                        miniature.text.into(),
                                         font_size,
                                        &projected_runs,
                                        None,
                                    ),
                                    indent,
                                )
                            })
                             .collect();
                         let lines = std::rc::Rc::new(lines);
                         view.document_map.cache = Some((key, geometry.first_line(), geometry.rows(), bounds.size.width, lines.clone()));
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
                    for (index, (line, indent)) in lines.iter().enumerate() {
                        if let Err(error) = line.paint(
                            point(
                                bounds.left() + *indent,
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
                    window.paint_quad(fill(
                        Bounds::new(bounds.origin, size(bounds.size.width, (zone.top() - bounds.top()).max(px(0.)))),
                        rgba((frost_colour << 8) | 50),
                    ));
                    window.paint_quad(fill(zone, rgba((focus_colour << 8) | 50)));
                    window.paint_quad(fill(
                        Bounds::new(point(bounds.left(), zone.bottom()), size(bounds.size.width, (bounds.bottom() - zone.bottom()).max(px(0.)))),
                        rgba((frost_colour << 8) | 50),
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
                .aria_label(self.panel_title(crate::panel_layout::PanelId::DocumentMap))
                .w_full()
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(rgb(
                    self.appearance_colour("DarkMode.customColorMain", 0xf6f6f6)
                ))
                .border_l_1()
                .border_color(rgb(0x808080))
                .child(
                    div()
                        .id("document-map-preview")
                        .debug_selector(|| "document-map-preview".into())
                        .role(Role::Group)
                        .aria_label(
                            crate::localization::localized_string(&locale, "document-map-miniature-label")
                                .unwrap_or("Document miniature; click or drag to scroll without moving selection"),
                        )
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .overflow_hidden()
                        .cursor_pointer()
                        .when_some(peek.filter(|_| self.pref_bool("docPeekOnMap", false)), |element, peek| {
                            element.tooltip(move |_, cx| {
                                let peek = peek.clone();
                                cx.new(move |_| peek).into()
                            })
                        })
                        .bg(rgb(if dark { 0x1e1e1e } else { 0xffffff }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(Self::document_map_pointer_down),
                        )
                        .on_mouse_move(cx.listener(Self::document_map_pointer_move))
                        .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                            if !hovered && view.document_map.peek.take().is_some() {
                                cx.notify();
                            }
                        }))
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
