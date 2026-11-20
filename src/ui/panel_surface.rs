use super::*;
use crate::panel_layout::{DockSide, FloatBounds, PanelId, PanelLayout, Placement};
use objc2_app_kit::{NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::collections::{BTreeMap, BTreeSet};

fn has_window_caption(id: PanelId) -> bool {
    !matches!(id, PanelId::DocumentList | PanelId::DocumentMap | PanelId::Converter)
}

fn caption_content_size(width: f32, height: f32, mtm: MainThreadMarker) -> gpui::Size<gpui::Pixels> {
    let rect = NSWindow::contentRectForFrameRect_styleMask(
        NSRect::new(NSPoint::new(0., 0.), NSSize::new(f64::from(width), f64::from(height))),
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::FullSizeContentView,
        mtm,
    );
    size(px(rect.size.width as f32), px(rect.size.height as f32))
}

#[derive(Default)]
pub(super) struct PanelState {
    pub(super) layout: PanelLayout,
    windows: BTreeMap<PanelId, gpui::WindowHandle<PanelWindow>>,
    opening: BTreeSet<PanelId>,
    mode_hidden: bool,
    resize: Option<(PanelId, gpui::Point<gpui::Pixels>, f32)>,
    title_drag: Option<(PanelId, gpui::Point<gpui::Pixels>)>,
    dock_regions: Vec<(DockSide, Bounds<gpui::Pixels>)>,
    pub(super) document_list_sort: u8,
    preserved: serde_json::Value,
    pub(super) preview: super::tab_preview::PreviewState,
}

pub(super) struct PanelWindow {
    owner: gpui::WeakEntity<MatrixWindow>,
    id: PanelId,
    focus: FocusHandle,
    _window_size_indicator: Option<window_size_indicator::WindowSizeIndicator>,
    _changed: gpui::Subscription,
    _released: gpui::Subscription,
    _bounds_changed: gpui::Subscription,
}

impl PanelWindow {
    fn new(
        owner: gpui::Entity<MatrixWindow>,
        id: PanelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.set_window_title(&owner.read(cx).panel_title(id));
        if has_window_caption(id)
            && let Err(error) = window_caption::hide_native_buttons(window)
        {
            eprintln!("panel titlebar: {error}");
        }
        let window_size_indicator = match window_size_indicator::WindowSizeIndicator::new(window) {
            Ok(indicator) => Some(indicator),
            Err(error) => {
                eprintln!("panel size indicator: {error}");
                None
            }
        };
        let changed = cx.observe_in(&owner, window, move |_, owner, window, cx| {
            let view = owner.read(cx);
            let p = view.panels.layout.panel(id);
            if p.visible && p.placement == Placement::Floating {
                window.set_window_title(&view.panel_title(id));
                cx.notify();
            } else {
                window.remove_window();
            }
        });
        let released =
            cx.observe_release_in(&owner, window, |_, _, window, _| window.remove_window());
        let weak = owner.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if matrix_surface::locked() {
                return false;
            }
            if let Err(error) = weak.update(cx, |owner, cx| owner.hide_panel(id, window, cx)) {
                eprintln!("panel close lost its document: {error}");
            }
            true
        });
        let observed = owner.downgrade();
        let bounds_changed = cx.observe_window_bounds(window, move |_, window, cx| {
            if matrix_surface::locked() {
                return;
            }
            let bounds = match window.window_bounds() {
                WindowBounds::Windowed(bounds) => bounds,
                _ => return,
            };
            if let Err(error) = observed.update(cx, |owner, cx| {
                let result = owner.panels.layout.set_bounds(
                    id,
                    FloatBounds {
                        x: f32::from(bounds.origin.x),
                        y: f32::from(bounds.origin.y),
                        width: f32::from(bounds.size.width),
                        height: f32::from(bounds.size.height)
                            - if has_window_caption(id) { window_caption::HEIGHT } else { 0. },
                    },
                );
                if let Err(error) = result {
                    eprintln!("panel geometry: {error}");
                }
                cx.notify();
            }) {
                eprintln!("panel geometry lost its document: {error}");
            }
        });
        let focus = cx.focus_handle();
        if id == PanelId::Converter {
            owner.update(cx, |view, cx| view.focus_converter_panel(window, cx));
        } else {
            focus.focus(window, cx);
        }
        Self {
            owner: owner.downgrade(),
            id,
            focus,
            _window_size_indicator: window_size_indicator,
            _changed: changed,
            _released: released,
            _bounds_changed: bounds_changed,
        }
    }
}

impl Render for PanelWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matrix_surface::locked() {
            return div().size_full().bg(rgb(0x000000)).into_any_element();
        }
        let id = self.id;
        let (content, title) = match self
            .owner
            .update(cx, |owner, cx| (owner.render_panel_frame(id, true, cx), owner.panel_title(id)))
        {
            Ok((content, title)) => (Some(content), title),
            Err(error) => {
                eprintln!("panel render lost its document: {error}");
                window.remove_window();
                (None, id.title().to_owned())
            }
        };
        window.set_window_title(&title);
        let owner = self.owner.clone();
        let caption = has_window_caption(id).then(|| {
            let button = |button_id: &'static str, label: &'static str, icon: caption_icons::Icon, icon_id: &'static str| {
                div().id(button_id).role(Role::Button).aria_label(label)
                    .w(px(45.)).h(px(30.)).flex_shrink_0()
                    .flex().items_center().justify_center()
                    .hover(|style| style.bg(rgb(0x4b8ed0)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(caption_icons::render(icon, icon_id, window_caption::FOREGROUND))
            };
            window_caption::frame()
                .child(window_caption::title(title)
                    .id("panel-window-title")
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, cx| {
                        cx.stop_propagation();
                        if matrix_surface::locked() { return; }
                        window.start_window_move();
                    })))
                .child(button("panel-window-minimize", "Minimize", caption_icons::Icon::Minimize, "panel-window-minimize-icon")
                    .on_click(|_, window, cx| {
                        cx.stop_propagation();
                        if !matrix_surface::locked() { window.minimize_window(); }
                    }))
                .child(button("panel-window-maximize", if window.is_maximized() { "Restore" } else { "Maximize" },
                    if window.is_maximized() { caption_icons::Icon::Restore } else { caption_icons::Icon::Maximize },
                    "panel-window-maximize-icon")
                    .on_click(|_, window, cx| {
                        cx.stop_propagation();
                        if !matrix_surface::locked() { window.zoom_window(); }
                    }))
                .child(window_caption::close_button("panel-window-close", "panel-window-close-icon", "Close")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        cx.stop_propagation();
                        if matrix_surface::locked() { return; }
                        if let Err(error) = view.owner.update(cx, |owner, cx| owner.hide_panel(id, window, cx)) {
                            eprintln!("panel caption close lost its document: {error}");
                        }
                    })))
        });
        div()
            .id("matrix-panel-window")
            .key_context("MatrixAuxiliary")
            .track_focus(&self.focus)
            .tab_group()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family("Helvetica")
            .text_size(px(12.))
            .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| {
                if matrix_surface::locked() {
                    cx.stop_propagation();
                    return;
                }
                window.focus_next(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                if matrix_surface::locked() {
                    cx.stop_propagation();
                    return;
                }
                window.focus_prev(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |_, _: &CloseWindow, window, cx| {
                if matrix_surface::locked() {
                    cx.stop_propagation();
                    return;
                }
                if let Err(error) = owner.update(cx, |owner, cx| owner.hide_panel(id, window, cx)) {
                    eprintln!("panel shortcut lost its document: {error}");
                }
                window.remove_window();
                cx.stop_propagation();
            }))
            .children(caption)
            .child(div().flex_1().min_h(px(0.)).min_w(px(0.)).overflow_hidden().children(content))
            .into_any_element()
    }
}

impl MatrixWindow {
    pub(super) fn panel_title(&self, id: PanelId) -> String {
        let locale = self.native_locale();
        match id {
            PanelId::DocumentList => crate::localization::dialog_name(&locale, "DocList/PanelTitle")
                .unwrap_or("Document List").to_owned(),
            PanelId::DocumentMap => crate::localization::dialog_name(&locale, "DocumentMap/PanelTitle")
                .unwrap_or("Document Map").to_owned(),
            _ => id.title().to_owned(),
        }
    }

    pub(super) fn capture_panel_layout(&self) -> serde_json::Value {
        let mut layout = self.panels.layout.clone();
        for id in PanelId::ALL {
            if self.panel_body_visible(id) {
                layout.show(id);
            } else {
                layout.hide(id);
            }
        }
        layout.to_json()
    }

    fn panel_body_visible(&self, id: PanelId) -> bool {
        match id {
            PanelId::DocumentList => self.document_list_visible,
            PanelId::DocumentMap => self.document_map_visible(),
            PanelId::FunctionList => self.function_list_visible(),
            PanelId::Folder => self.folder_workspace_visible(),
            PanelId::Project1 => self.projects.visible(0),
            PanelId::Project2 => self.projects.visible(1),
            PanelId::Project3 => self.projects.visible(2),
            PanelId::Characters => self.characters.visible,
            PanelId::Converter => self.panels.layout.panel(id).visible,
        }
    }

    pub(super) fn capture_panel_state(&self) -> io::Result<serde_json::Value> {
        let mut state = self.panels.preserved.as_object().cloned().unwrap_or_default();
        state.extend(serde_json::json!({
            "version": 1,
            "layout": self.capture_panel_layout(),
            "folder": self.capture_folder_workspace()?,
            "projects": self.projects.snapshot()?,
        }).as_object().cloned().unwrap_or_default());
        Ok(serde_json::Value::Object(state))
    }

    pub(super) fn restore_panel_state(
        &mut self,
        value: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        if value["version"].as_u64() != Some(1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported panel state version",
            ));
        }
        PanelLayout::from_json(&value["layout"])?;
        crate::folder_workspace::FolderWorkspace::restore(&value["folder"])?;
        self.projects.restore(&value["projects"])?;
        self.restore_folder_workspace(&value["folder"], window, cx)?;
        self.panels.preserved = value.clone();
        self.restore_panel_layout(&value["layout"], window, cx)
    }

    pub(super) fn restore_panel_layout(
        &mut self,
        value: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        let layout = PanelLayout::from_json(value)?;
        self.panels.layout = layout;
        for id in PanelId::ALL {
            self.set_panel_body_visible(id, self.panels.layout.panel(id).visible, window, cx);
        }
        self.sync_panel_windows(window, cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn show_panel(&mut self, id: PanelId, window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() {
            return;
        }
        self.panels.layout.show(id);
        self.set_panel_body_visible(id, true, window, cx);
        self.sync_panel_windows(window, cx);
        if id == PanelId::Converter && let Some(handle) = self.panels.windows.get(&id).copied() {
            let input = self.converter_panel_input();
            cx.defer(move |cx| {
                if matrix_surface::locked() {
                    return;
                }
                if let Err(error) = handle.update(cx, |_, window, cx| {
                    window.activate_window();
                    input.update(cx, |input, cx| input.focus(window, cx));
                }) { eprintln!("could not activate Conversion panel: {error}"); }
            });
        }
        cx.notify();
    }

    pub(super) fn hide_panel(&mut self, id: PanelId, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.hide_panel(id, window, cx)
        }) {
            return;
        }
        if !self.focus_editor_pane(self.pane, window, cx) {
            return;
        }
        self.panels.layout.hide(id);
        self.set_panel_body_visible(id, false, window, cx);
        cx.notify();
    }

    fn set_panel_body_visible(
        &mut self,
        id: PanelId,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match id {
            PanelId::Characters => self.characters.visible = visible,
            PanelId::Converter => {
                if visible && self.converter_enabled { self.panels.layout.show(id); } else { self.panels.layout.hide(id); }
            }
            PanelId::DocumentList => self.document_list_visible = visible,
            PanelId::DocumentMap => {
                self.set_document_map_visible(visible);
            }
            PanelId::FunctionList => {
                if visible && !self.function_list_visible() {
                    self.open_function_list_body(window, cx);
                } else if !visible {
                    self.close_function_list_body(window, cx);
                }
            }
            PanelId::Folder => {
                self.folder.tree.set_visible(visible);
                if visible {
                    self.resume_folder_watch(window, cx);
                } else {
                    self.pause_folder_watch();
                }
            }
            PanelId::Project1 | PanelId::Project2 | PanelId::Project3 => {
                let index = match id {
                    PanelId::Project1 => 0,
                    PanelId::Project2 => 1,
                    _ => 2,
                };
                if let Ok(panel) = self.projects.model.panel_mut(index) {
                    panel.visible = visible;
                }
                self.sync_find_project_panels(cx);
            }
        }
    }

    pub(super) fn float_panel(&mut self, id: PanelId, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.float_panel(id, window, cx)
        }) {
            return;
        }
        let bounds = self.panels.layout.panel(id).bounds;
        if let Err(error) = self.panels.layout.float(id, bounds) {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        self.show_panel(id, window, cx);
    }

    pub(super) fn dock_panel(
        &mut self,
        id: PanelId,
        side: DockSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.dock_panel(id, side, window, cx)
        }) {
            return;
        }
        let order = self.panels.layout.docked(side).len();
        self.panels.layout.dock(id, side, order);
        self.show_panel(id, window, cx);
    }

    pub(super) fn sync_panel_windows(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() {
            return;
        }
        let mode_hidden = !self.view_panels_other_pane_visible();
        if self.panels.mode_hidden != mode_hidden {
            self.panels.mode_hidden = mode_hidden;
            for handle in self.panels.windows.values().copied() {
                if let Err(error) = handle.update(cx, |_, window, _| {
                    if let Ok(handle) = HasWindowHandle::window_handle(window)
                        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
                    {
                        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                        if let Some(host) = view.window() {
                            if mode_hidden { host.orderOut(None); } else { host.orderFront(None); }
                        }
                    }
                }) { eprintln!("panel display mode could not reach its window: {error}"); }
            }
        }
        for id in PanelId::ALL {
            if self.panel_body_visible(id) {
                self.panels.layout.show(id);
            } else {
                self.panels.layout.hide(id);
            }
        }
        for id in PanelId::ALL {
            let p = self.panels.layout.panel(id);
            if !p.visible || p.placement != Placement::Floating {
                self.panels.opening.remove(&id);
                if let Some(handle) = self.panels.windows.remove(&id)
                    && cx.windows().contains(&handle.into())
                    && let Err(error) = handle.update(cx, |_, window, _| window.remove_window())
                {
                    eprintln!("could not close docked panel window: {error}");
                }
                continue;
            }
            if mode_hidden { continue; }
            if self
                .panels
                .windows
                .get(&id)
                .is_some_and(|handle| cx.windows().contains(&(*handle).into()))
                || !self.panels.opening.insert(id)
            {
                continue;
            }
            let owner = cx.weak_entity();
            cx.defer(move |cx| {
                let Some(owner) = owner.upgrade() else {
                    return;
                };
                let state = owner.read(cx).panels.layout.panel(id);
                if matrix_surface::locked() || !state.visible || state.placement != Placement::Floating {
                    owner.update(cx, |owner, _| {
                        owner.panels.opening.remove(&id);
                    });
                    return;
                }
                let b = state.bounds;
                let native_caption = has_window_caption(id);
                let Some(mtm) = MainThreadMarker::new() else {
                    owner.update(cx, |owner, cx| {
                        owner.panels.opening.remove(&id);
                        owner.panels.layout.redock(id);
                        owner.notify_editor_error("Panel windows require the main thread".into(), cx);
                    });
                    return;
                };
                let options = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: gpui::point(px(b.x), px(b.y)),
                        size: if id == PanelId::Converter {
                            size(px(b.width).max(dialog_font::px(488.)), px(b.height).max(dialog_font::px(238.)))
                        } else if native_caption {
                            caption_content_size(b.width, b.height + window_caption::HEIGHT, mtm)
                        } else { size(px(b.width), px(b.height)) },
                    })),
                    titlebar: native_caption.then(|| TitlebarOptions {
                        title: Some(owner.read(cx).panel_title(id).into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    kind: gpui::WindowKind::Floating,
                    is_movable: true,
                    app_owns_titlebar_drag: native_caption,
                    window_min_size: Some(if id == PanelId::Converter {
                        size(super::dialog_font::px(488.), super::dialog_font::px(238.))
                    } else if native_caption {
                        size(px(140.), px(100. + window_caption::HEIGHT))
                    } else { size(px(140.), px(100.)) }),
                    ..Default::default()
                };
                let observed = owner.clone();
                let opened = cx.open_window(options, move |window, cx| {
                    cx.new(|cx| PanelWindow::new(observed, id, window, cx))
                });
                owner.update(cx, |owner, cx| {
                    owner.panels.opening.remove(&id);
                    match opened {
                        Ok(handle) => {
                            owner.panels.windows.insert(id, handle);
                        }
                        Err(error) => {
                            owner.panels.layout.redock(id);
                            owner.notify_editor_error(
                                format!("Could not float {}: {error}", owner.panel_title(id)),
                                cx,
                            );
                        }
                    }
                    cx.notify();
                });
            });
        }
    }

    pub(super) fn render_panel_region(&self, side: DockSide, cx: &mut Context<Self>) -> AnyElement {
        if matrix_surface::locked() {
            return div().size_full().bg(rgb(0x000000)).into_any_element();
        }
        let owner = cx.entity();
        div()
            .id(format!("panel-region-{}", side.key()))
            .relative()
            .flex()
            .flex_shrink_0()
            .min_h(px(0.))
            .min_w(px(0.))
            .when(matches!(side, DockSide::Top | DockSide::Bottom), |region| {
                region.flex_col().w_full()
            })
            .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, cx| {
                if matrix_surface::locked() {
                    return;
                }
                let screen = Bounds::new(bounds.origin + window.bounds().origin, bounds.size);
                owner.update(cx, |view, _| {
                    view.panels.dock_regions.retain(|(region, _)| *region != side);
                    view.panels.dock_regions.push((side, screen));
                });
            }).absolute().size_full())
            .children(
                self.panels
                    .layout
                    .docked(side)
                    .into_iter()
                    .map(|id| self.render_panel_frame(id, false, cx)),
            )
            .into_any_element()
    }

    fn render_panel_frame(
        &self,
        id: PanelId,
        floating: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if matrix_surface::locked() {
            return div().size_full().bg(rgb(0x000000)).into_any_element();
        }
        let p = self.panels.layout.panel(id);
        let dark = self.dark;
        let title = self.panel_title(id);
        let hide_label = if matches!(id, PanelId::DocumentList | PanelId::DocumentMap) {
            crate::localization::localized_string(&self.native_locale(), "close-panel-tip")
                .unwrap_or("Close").to_owned()
        } else {
            format!("Hide {}", id.title())
        };
        let hide_tooltip = hide_label.clone();
        let single_title = matches!(id, PanelId::DocumentList | PanelId::DocumentMap | PanelId::Converter);
        let content = match id {
            PanelId::Characters => self.render_character_panel(cx),
            PanelId::DocumentList => self.render_document_list(cx),
            PanelId::DocumentMap => self.render_document_map(cx),
            PanelId::FunctionList => self.render_function_list(cx),
            PanelId::Folder => self.render_folder_workspace(cx),
            PanelId::Project1 => self.render_project_panel(0, cx),
            PanelId::Project2 => self.render_project_panel(1, cx),
            PanelId::Project3 => self.render_project_panel(2, cx),
            PanelId::Converter => self.render_converter_panel(cx),
        };
        let controls = DockSide::ALL.into_iter().filter(move |_| !single_title).map(|side| {
            div()
                .id(format!("{}-dock-{}", id.key(), side.key()))
                .role(Role::Button)
                .aria_label(format!("Dock {} {}", id.title(), side.key()))
                .tooltip(move |_, cx| {
                    icon_tooltip::view(format!("Dock {} {}", id.title(), side.key()), dark, cx)
                })
                .cursor_pointer()
                .px(px(3.))
                .child(match side {
                    DockSide::Left => "←",
                    DockSide::Right => "→",
                    DockSide::Top => "↑",
                    DockSide::Bottom => "↓",
                })
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.dock_panel(id, side, window, cx);
                    cx.stop_propagation();
                }))
        });
        let resize_owner = cx.entity();
        let resize_listener = gpui::canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let owner = resize_owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                    if matrix_surface::locked() || phase != gpui::DispatchPhase::Bubble {
                        return;
                    }
                    owner.update(cx, |view, cx| {
                        let Some((target, origin, size)) = view.panels.resize else {
                            return;
                        };
                        if target != id {
                            return;
                        }
                        if event.pressed_button != Some(MouseButton::Left) {
                            view.panels.resize = None;
                            return;
                        }
                        let delta = match p.dock {
                            DockSide::Left => f32::from(event.position.x - origin.x),
                            DockSide::Right => f32::from(origin.x - event.position.x),
                            DockSide::Top => f32::from(event.position.y - origin.y),
                            DockSide::Bottom => f32::from(origin.y - event.position.y),
                        };
                        if let Err(error) = view.panels.layout.resize(id, (size + delta).max(40.)) {
                            eprintln!("panel resize: {error}");
                        }
                        cx.notify();
                    });
                });
                let owner = resize_owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
                    if matrix_surface::locked() || phase != gpui::DispatchPhase::Bubble { return; }
                    owner.update(cx, |view, cx| {
                        if let Some((target, origin)) = view.panels.title_drag
                            && target == id
                        {
                            view.panels.title_drag = None;
                            let position = event.position + window.bounds().origin;
                            let dx = f32::from(position.x - origin.x);
                            let dy = f32::from(position.y - origin.y);
                            if dx.abs() + dy.abs() > 5. {
                                let side = view.panels.dock_regions.iter().find_map(|(side, bounds)| {
                                    let mut target = *bounds;
                                    if view.panels.layout.docked(*side).is_empty() {
                                        match side {
                                            DockSide::Left | DockSide::Right => {
                                                target.origin.x -= px(20.);
                                                target.size.width = px(40.);
                                            }
                                            DockSide::Top | DockSide::Bottom => {
                                                target.origin.y -= px(20.);
                                                target.size.height = px(40.);
                                            }
                                        }
                                    } else { target.size.height = dialog_font::px(18.); }
                                    target.contains(&position).then_some(*side)
                                });
                                if let Some(side) = side { view.dock_panel(id, side, window, cx); }
                                else {
                                    let mut bounds = view.panels.layout.panel(id).bounds;
                                    bounds.x = f32::from(position.x) - 20.;
                                    bounds.y = f32::from(position.y) - 9.;
                                    if let Err(error) = view.panels.layout.set_bounds(id, bounds) {
                                        view.notify_editor_error(error.to_string(), cx);
                                    } else { view.float_panel(id, window, cx); }
                                }
                            }
                        }
                        if view
                            .panels
                            .resize
                            .is_some_and(|(target, _, _)| target == id)
                        {
                            view.panels.resize = None;
                        }
                    })
                });
            },
        )
        .size_full();
        div()
            .id(format!("{}-dock-frame", id.key()))
            .relative()
            .flex()
            .flex_col()
            .min_h(px(0.))
            .min_w(px(0.))
            .flex_shrink_0()
            .when(floating, |frame| frame.size_full())
            .when(
                !floating && matches!(p.dock, DockSide::Left | DockSide::Right),
                |frame| frame.w(if id == PanelId::Converter { px(p.size).max(dialog_font::px(488.)) } else { px(p.size) }).h_full(),
            )
            .when(
                !floating && matches!(p.dock, DockSide::Top | DockSide::Bottom),
                |frame| frame.h(if id == PanelId::Converter { px(p.size).max(dialog_font::px(238.)) } else { px(p.size) }).w_full(),
            )
            .bg(rgb(if self.dark { 0x282828 } else { 0xf6f6f6 }))
            .text_color(rgb(if self.dark { 0xd4d4d4 } else { 0x202020 }))
            .child(resize_listener.absolute())
            .child(
                div()
                    .id(format!("{}-caption", id.key()))
                    .role(Role::GenericContainer)
                    .aria_label(title.clone())
                    .h(if single_title { dialog_font::px(18.) } else { px(23.) })
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .when(single_title, |caption| caption
                        .font_family(dialog_font::font_family())
                        .text_size(dialog_font::px(12.))
                        .when(!floating, |caption| caption.bg(rgb(if dark { 0x383838 } else { 0xf0f0f0 })))
                        .border_b_1().border_color(rgb(0x808080))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                            if matrix_surface::locked() {
                                cx.stop_propagation();
                                return;
                            }
                            if event.click_count == 2 {
                                view.panels.title_drag = None;
                                if floating {
                                    view.dock_panel(id, view.panels.layout.panel(id).dock, window, cx);
                                } else { view.float_panel(id, window, cx); }
                            } else {
                                view.panels.title_drag = Some((id, event.position + window.bounds().origin));
                                if floating { window.start_window_move(); }
                            }
                            cx.stop_propagation();
                        }))
                        .child(div().flex_1().min_w(px(0.)).px(dialog_font::px(2.)).overflow_hidden().whitespace_nowrap().child(title)))
                    .when(floating, |caption| caption
                        .bg(rgb(window_caption::BACKGROUND))
                        .text_color(rgb(window_caption::FOREGROUND)))
                    .children(controls)
                    .when(!single_title, |caption| caption.child(
                        div()
                            .id(format!("{}-float", id.key()))
                            .role(Role::Button)
                            .aria_label(format!("Float {}", id.title()))
                            .tooltip(move |_, cx| {
                                icon_tooltip::view(format!("Float {}", id.title()), dark, cx)
                            })
                            .px(px(3.))
                            .cursor_pointer()
                            .child("↗")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.float_panel(id, window, cx);
                                cx.stop_propagation();
                            })),
                    ))
                    .child(
                        div()
                            .id(format!("{}-hide", id.key()))
                            .debug_selector(move || format!("{}-hide", id.key()))
                            .role(Role::Button)
                            .aria_label(hide_label)
                            .tooltip(move |_, cx| {
                                icon_tooltip::view(hide_tooltip.clone(), dark, cx)
                            })
                            .px(px(3.))
                            .when(single_title, |button| button
                                .w(dialog_font::px(14.)).h(dialog_font::px(14.))
                                .mr(dialog_font::px(2.)).px(px(0.))
                                .flex().items_center().justify_center()
                                .border_1().border_color(rgb(0x808080))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()))
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.hide_panel(id, window, cx);
                                cx.stop_propagation();
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .min_w(px(0.))
                    .overflow_hidden()
                    .children(content),
            )
            .when(!floating, |frame| {
                frame.child(
                    div()
                        .id(format!("{}-resize", id.key()))
                        .when(single_title, |edge| edge.absolute()
                        .when(matches!(p.dock, DockSide::Left | DockSide::Right), |edge| edge.top_0().h_full().w(px(4.)))
                        .when(matches!(p.dock, DockSide::Top | DockSide::Bottom), |edge| edge.left_0().w_full().h(px(4.)))
                        .when(p.dock == DockSide::Left, |edge| edge.right_0())
                        .when(p.dock == DockSide::Right, |edge| edge.left_0())
                        .when(p.dock == DockSide::Top, |edge| edge.bottom_0())
                        .when(p.dock == DockSide::Bottom, |edge| edge.top_0()))
                        .when(!single_title, |edge| edge.h(px(5.)).w_full())
                        .cursor_crosshair()
                        .bg(rgb(if dark { 0x484848 } else { 0xd4d0c8 }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                                if matrix_surface::locked() {
                                    cx.stop_propagation();
                                    return;
                                }
                                view.panels.resize =
                                    Some((id, event.position, view.panels.layout.panel(id).size));
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .into_any_element()
    }
}
