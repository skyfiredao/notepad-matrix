use super::split_view_geometry::Viewport;
use super::*;
use crate::workspace::ViewId;

pub(super) struct PaneState {
    editor_focus: FocusHandle,
    editor_lines: Vec<editor_surface::PaintedLine>,
    editor_chrome: editor_surface::EditorChrome,
    editor_bounds: Option<Bounds<gpui::Pixels>>,
    editor_dragging: bool,
    editor_error: Option<String>,
    editor_painted_revision: Option<crate::input::DisplayRevision>,
    editor_accessibility: Option<(crate::input::DisplayRevision, gpui::SharedString)>,
    editor_top_line: usize,
    editor_horizontal_offset: gpui::Pixels,
    editor_follow_caret: bool,
    editor_scroll_remainder: f32,
    editor_observed_revision: crate::input::DisplayRevision,
    editor_observed_selection: Selection,
    editor_preferred_x: Option<gpui::Pixels>,
    pub(super) tab_scroll: gpui::ScrollHandle,
    overwrite_mode: bool,
    syntax_cache: Option<(
        crate::input::DisplayRevision,
        crate::syntax::Language,
        Vec<crate::syntax::StyleSpan>,
    )>,
    udl_syntax_cache: Option<language_surface::UdlSyntaxCache>,
    builtin_syntax_cache: Option<builtin_language_surface::BuiltinSyntaxCache>,
}

impl PaneState {
    pub fn new(editor: &EditorSession, cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            editor_focus: cx.focus_handle(),
            editor_lines: Vec::new(),
            editor_chrome: Default::default(),
            editor_bounds: None,
            editor_dragging: false,
            editor_error: None,
            editor_painted_revision: None,
            editor_accessibility: None,
            editor_top_line: 0,
            editor_horizontal_offset: px(0.),
            editor_follow_caret: true,
            editor_scroll_remainder: 0.,
            editor_observed_revision: editor.display_revision(),
            editor_observed_selection: editor.selection(),
            editor_preferred_x: None,
            tab_scroll: gpui::ScrollHandle::new(),
            overwrite_mode: false,
            syntax_cache: None,
            udl_syntax_cache: None,
            builtin_syntax_cache: None,
        }
    }
}

impl MatrixWindow {
    pub(super) fn retire_editor_session_views(&mut self) {
        self.editor_chrome.retire_document();
        self.other_pane.editor_chrome.retire_document();
        self.syntax_cache = None;
        self.other_pane.syntax_cache = None;
        self.udl.syntax_cache = None;
        self.other_pane.udl_syntax_cache = None;
        self.builtin.syntax_cache = None;
        self.other_pane.builtin_syntax_cache = None;
        self.split_sync.clear();
        self.split_sync_source = None;
    }

    pub(super) fn split_views_visible(&self) -> bool {
        self.workspace.two_views_visible()
    }
    pub(super) fn sync_vertical_enabled(&self) -> bool {
        self.split_sync.vertical
    }
    pub(super) fn sync_horizontal_enabled(&self) -> bool {
        self.split_sync.horizontal
    }

    pub(super) fn scope_editor_pane(&mut self, pane: ViewId) -> bool {
        if pane == self.pane {
            return true;
        }
        if let Err(error) = self.workspace.set_active_view(pane) {
            eprintln!("could not scope editor pane: {error}");
            return false;
        }
        self.swap_pane_fields();
        self.pane = pane;
        true
    }

    fn swap_pane_fields(&mut self) {
        macro_rules! swap { ($($field:ident),* $(,)?) => { $(std::mem::swap(&mut self.$field, &mut self.other_pane.$field);)* }; }
        swap!(
            editor_focus,
            editor_lines,
            editor_chrome,
            editor_bounds,
            editor_dragging,
            editor_error,
            editor_painted_revision,
            editor_accessibility,
            editor_top_line,
            editor_horizontal_offset,
            editor_follow_caret,
            editor_scroll_remainder,
            editor_observed_revision,
            editor_observed_selection,
            editor_preferred_x,
            tab_scroll,
            overwrite_mode,
            syntax_cache
        );
        std::mem::swap(
            &mut self.udl.syntax_cache,
            &mut self.other_pane.udl_syntax_cache,
        );
        std::mem::swap(
            &mut self.builtin.syntax_cache,
            &mut self.other_pane.builtin_syntax_cache,
        );
    }

    pub(super) fn reconcile_editor_panes(&mut self) {
        let pane = self.workspace.active_view();
        if pane != self.pane {
            self.swap_pane_fields();
            self.pane = pane;
        }
        if !self.split_views_visible() {
            self.split_sync.clear();
            self.split_sync_source = None;
            self.other_pane.editor_chrome.retire_document();
        }
    }

    pub(super) fn focus_editor_pane(
        &mut self,
        pane: ViewId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return false;
        }
        if pane != self.pane {
            if !self.cancel_view_composition(window, cx) || !self.scope_editor_pane(pane) {
                return false;
            }
            if let Err(error) = self.workspace.focus_view(pane) {
                self.notify_editor_error(error.to_string(), cx);
                return false;
            }
            self.editor_dragging = false;
            self.editor_chrome.clear_drag();
            self.invalidate_incremental_count();
            self.refresh_function_list(window, cx);
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
        true
    }

    fn transfer_editor_pane(&mut self, moving: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || !self.cancel_view_composition(window, cx)
        {
            return;
        }
        let id = self.workspace.active().id();
        let result = if moving {
            self.workspace.move_to_other_view(id)
        } else {
            self.workspace.clone_to_other_view(id)
        };
        match result {
            Ok(()) => {
                self.reconcile_editor_panes();
                self.prepare_editor_document();
                self.restore_current_document_state(window, cx);
                let overwrite = self.overwrite_mode;
                self.editor_mut().set_overwrite(overwrite);
                self.editor_focus.focus(window, cx);
                self.synchronize_recovery(cx);
                cx.notify();
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }

    pub(super) fn clone_to_other_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.matrix_active() || matrix_surface::locked() { return; }
        self.transfer_editor_pane(false, window, cx);
    }
    pub(super) fn move_to_other_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.matrix_active() || matrix_surface::locked() { return; }
        self.transfer_editor_pane(true, window, cx);
    }

    fn pane_viewports(&self) -> (Viewport, Viewport) {
        let current = Viewport::new(
            self.editor_top_line,
            f32::from(self.editor_horizontal_offset),
            self.editor_chrome.p_width(),
        );
        let other = Viewport::new(
            self.other_pane.editor_top_line,
            f32::from(self.other_pane.editor_horizontal_offset),
            self.other_pane.editor_chrome.p_width(),
        );
        if self.pane == ViewId::Primary {
            (current, other)
        } else {
            (other, current)
        }
    }

    pub(super) fn toggle_sync_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (main, sub) = self.pane_viewports();
        self.split_sync
            .toggle_vertical(self.split_views_visible(), main, sub);
        cx.notify();
    }

    pub(super) fn toggle_sync_horizontal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (main, sub) = self.pane_viewports();
        self.split_sync
            .toggle_horizontal(self.split_views_visible(), main, sub);
        cx.notify();
    }

    pub(super) fn sync_pane_viewport(&mut self, cx: &mut Context<Self>) {
        let pane = self.pane;
        let owner = cx.entity();
        cx.defer(move |cx| {
            owner.update(cx, |view, cx| {
                let previous = view.pane;
                if view.scope_editor_pane(pane) {
                    view.apply_pane_viewport_sync(cx);
                    view.scope_editor_pane(previous);
                }
            });
        });
    }

    fn apply_pane_viewport_sync(&mut self, cx: &mut Context<Self>) {
        if !self.split_views_visible() {
            self.split_sync.clear();
            return;
        }
        let (main, sub) = self.pane_viewports();
        let source_main = self.pane == ViewId::Primary;
        let (source, other) = if source_main {
            (main, sub)
        } else {
            (sub, main)
        };
        let target = self.split_sync.target(source_main, source, other);
        if target.top != other.top || target.horizontal != other.horizontal {
            self.other_pane.editor_top_line = target.top;
            self.other_pane.editor_horizontal_offset = px(target.horizontal);
            self.other_pane.editor_follow_caret = false;
            self.other_pane.editor_painted_revision = None;
            cx.notify();
        }
    }

    pub(super) fn invalidate_style_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_chrome.invalidate_layout();
        self.other_pane.editor_chrome.invalidate_layout();
        self.syntax_cache = None;
        self.other_pane.syntax_cache = None;
        self.udl.syntax_cache = None;
        self.other_pane.udl_syntax_cache = None;
        self.builtin.syntax_cache = None;
        self.other_pane.builtin_syntax_cache = None;
        self.builtin.style_revision = self.builtin.style_revision.saturating_add(1);
        self.refresh_editor_syntax();
        self.capture_udl_text_system(window);
        self.editor_painted_revision = None;
        self.other_pane.editor_painted_revision = None;
        window.notify_text_input_changed(&self.editor_focus);
        cx.notify();
    }

    pub(super) fn render_split_editors(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.pane;
        let panes: Vec<_> = [ViewId::Primary, ViewId::Secondary]
            .into_iter()
            .filter(|pane| self.workspace.active_id_in_view(*pane).is_some())
            .filter(|pane| self.view_panels_other_pane_visible() || *pane == focused)
            .collect();
        let split = panes.len() == 2;
        let classic_frame = !self.dark
            && !self.tab_vertical
            && !self.tab_multiline
            && !self.pref_bool("TabBar.hide", false)
            && self.view_tabs_status_visible();
        let border = px(self.pref_int("ScintillaPrimaryView.borderWidth", 2).max(0) as f32);
        let frame_width = if self.pref_bool("ScintillaPrimaryView.borderEdge", true) { 4 } else { 2 };
        let frame_left = border;
        let frame_right = (border * 2. - px(1.)).max(px(0.));
        let frame_bottom = (border * 2. - px(3.)).max(px(0.));
        let frame_top = px(if self.pref_bool("TabBar.reduce", true) { 24. } else { 32. });
        let mut elements = Vec::new();
        for (index, pane) in panes.into_iter().enumerate() {
            if !self.scope_editor_pane(pane) {
                continue;
            }
            let tabs = self.render_tabs(pane == focused, cx);
            let editor = self.render_editor(cx);
            let editor = if classic_frame {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .ml(frame_left + px(frame_width as f32))
                    .mr(frame_right + px(frame_width as f32))
                    .mb(frame_bottom + px(frame_width as f32))
                    .child(editor)
                    .into_any_element()
            } else {
                editor
            };
            elements.push(
                div()
                    .id(if pane == ViewId::Primary {
                        "editor-pane-primary"
                    } else {
                        "editor-pane-secondary"
                    })
                    .debug_selector(move || {
                        if pane == ViewId::Primary {
                            "matrix-editor-primary".into()
                        } else {
                            "editor-pane-secondary".into()
                        }
                    })
                    .relative()
                    .flex()
                    .when(!self.tab_vertical, |pane| pane.flex_col())
                    .flex_shrink_1()
                    .h_full()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .when(!classic_frame && !self.pref_bool("TabBar.hide", false), |pane| {
                        let border = px(self.pref_int("ScintillaPrimaryView.borderWidth", 2) as f32);
                        pane.ml(border).mr(border * 2.).mt(border).mb(border * 2.)
                    })
                    .when(
                        !classic_frame && self.pref_bool("ScintillaPrimaryView.borderEdge", true),
                        |pane| {
                            pane.border_1().border_color(rgb(
                                self.appearance_colour("DarkMode.customColorEdge", 0x808080)
                            ))
                        },
                    )
                    .when(classic_frame, |pane| {
                        pane.bg(rgb(self.appearance_colour("DarkMode.customColorMain", 0xf0f0f0)))
                            .child(gpui::canvas(
                                |bounds, _, _| bounds,
                                move |bounds, _, window, _| {
                                    let left = bounds.left() + frame_left;
                                    let right = bounds.right() - frame_right;
                                    let top = bounds.top() + frame_top;
                                    let bottom = bounds.bottom() - frame_bottom;
                                    for index in 0..frame_width {
                                        let inset = px(index as f32);
                                        let light = [0xffffff, 0xe3e3e3, 0xa0a0a0, 0x696969][index];
                                        let shade = [0x696969, 0xa0a0a0, 0xffffff, 0xe3e3e3][index];
                                        for (origin, extent, color) in [
                                            (gpui::point(left + inset, top + inset), size(right - left - inset * 2., px(1.)), light),
                                            (gpui::point(left + inset, top + inset), size(px(1.), bottom - top - inset * 2.), light),
                                            (gpui::point(right - inset - px(1.), top + inset), size(px(1.), bottom - top - inset * 2.), shade),
                                            (gpui::point(left + inset, bottom - inset - px(1.)), size(right - left - inset * 2., px(1.)), shade),
                                        ] {
                                            window.paint_quad(gpui::fill(Bounds::new(origin, extent), rgb(color)));
                                        }
                                    }
                                },
                            ).absolute().size_full())
                    })
                    .when(split, |el| {
                        el.w(gpui::relative(if index == 0 {
                            self.split_ratio
                        } else {
                            1. - self.split_ratio
                        }))
                    })
                    .when(!split, |el| el.flex_1())
                    .children(self.view_tabs_status_visible().then_some(tabs))
                    .child(editor)
                    .into_any_element(),
            );
            if split && index == 0 {
                elements.push(
                    div()
                        .id("editor-splitter")
                        .debug_selector(|| "editor-splitter".into())
                        .w(px(5.))
                        .h_full()
                        .flex_shrink_0()
                        .bg(rgb(if self.dark { 0x606060 } else { 0xc0c0c0 }))
                        .cursor(gpui::CursorStyle::ResizeLeftRight)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, _, cx| {
                                view.splitter_dragging = true;
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                );
            }
        }
        self.scope_editor_pane(focused);
        div()
            .id("editor-split-views")
            .relative()
            .flex()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .h_full()
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
                if view.splitter_dragging
                    && event.pressed_button == Some(MouseButton::Left)
                    && let Some(bounds) = view.split_bounds
                {
                    view.split_ratio = (f32::from(event.position.x - bounds.left())
                        / f32::from(bounds.size.width).max(1.))
                    .clamp(0.1, 0.9);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.splitter_dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.splitter_dragging = false),
            )
            .child(
                gpui::canvas(|bounds, _, _| bounds, {
                    let entity = cx.entity();
                    move |bounds, _, _, cx| {
                        entity.update(cx, |view, _| view.split_bounds = Some(bounds));
                    }
                })
                .absolute()
                .size_full(),
            )
            .children(elements)
            .into_any_element()
    }
}
