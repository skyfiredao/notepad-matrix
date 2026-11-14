use super::*;
use crate::panel_layout::{DockSide, FloatBounds, PanelId, PanelLayout, Placement};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct PanelState {
    pub(super) layout: PanelLayout,
    windows: BTreeMap<PanelId, gpui::WindowHandle<PanelWindow>>,
    opening: BTreeSet<PanelId>,
    resize: Option<(PanelId, gpui::Point<gpui::Pixels>, f32)>,
    pub(super) preview: super::tab_preview::PreviewState,
}

pub(super) struct PanelWindow {
    owner: gpui::WeakEntity<MatrixWindow>,
    id: PanelId,
    focus: FocusHandle,
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
        let changed = cx.observe_in(&owner, window, move |_, owner, window, cx| {
            let p = owner.read(cx).panels.layout.panel(id);
            if p.visible && p.placement == Placement::Floating {
                cx.notify();
            } else {
                window.remove_window();
            }
        });
        let released =
            cx.observe_release_in(&owner, window, |_, _, window, _| window.remove_window());
        let weak = owner.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if let Err(error) = weak.update(cx, |owner, cx| owner.hide_panel(id, window, cx)) {
                eprintln!("panel close lost its document: {error}");
            }
            true
        });
        let observed = owner.downgrade();
        let bounds_changed = cx.observe_window_bounds(window, move |_, window, cx| {
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
                        height: f32::from(bounds.size.height),
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
        focus.focus(window, cx);
        Self {
            owner: owner.downgrade(),
            id,
            focus,
            _changed: changed,
            _released: released,
            _bounds_changed: bounds_changed,
        }
    }
}

impl Render for PanelWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id;
        let content = match self
            .owner
            .update(cx, |owner, cx| owner.render_panel_frame(id, true, cx))
        {
            Ok(content) => Some(content),
            Err(error) => {
                eprintln!("panel render lost its document: {error}");
                window.remove_window();
                None
            }
        };
        let owner = self.owner.clone();
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
                window.focus_next(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                window.focus_prev(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |_, _: &CloseWindow, window, cx| {
                if let Err(error) = owner.update(cx, |owner, cx| owner.hide_panel(id, window, cx)) {
                    eprintln!("panel shortcut lost its document: {error}");
                }
                window.remove_window();
                cx.stop_propagation();
            }))
            .children(content)
    }
}

impl MatrixWindow {
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
        }
    }

    pub(super) fn capture_panel_state(&self) -> io::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "version": 1,
            "layout": self.capture_panel_layout(),
            "folder": self.capture_folder_workspace()?,
            "projects": self.projects.snapshot()?,
        }))
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
        self.panels.layout.show(id);
        self.set_panel_body_visible(id, true, window, cx);
        self.sync_panel_windows(window, cx);
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
            PanelId::DocumentList => self.document_list_visible = visible,
            PanelId::DocumentMap => self.document_map.visible = visible,
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
                if !state.visible || state.placement != Placement::Floating {
                    owner.update(cx, |owner, _| {
                        owner.panels.opening.remove(&id);
                    });
                    return;
                }
                let b = state.bounds;
                let options = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: gpui::point(px(b.x), px(b.y)),
                        size: size(px(b.width), px(b.height)),
                    })),
                    titlebar: Some(TitlebarOptions {
                        title: Some(id.title().into()),
                        ..Default::default()
                    }),
                    kind: gpui::WindowKind::Floating,
                    is_movable: true,
                    window_min_size: Some(size(px(140.), px(100.))),
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
                                format!("Could not float {}: {error}", id.title()),
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
        div()
            .id(format!("panel-region-{}", side.key()))
            .flex()
            .flex_shrink_0()
            .min_h(px(0.))
            .min_w(px(0.))
            .when(matches!(side, DockSide::Top | DockSide::Bottom), |region| {
                region.flex_col().w_full()
            })
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
        let p = self.panels.layout.panel(id);
        let dark = self.dark;
        let content = match id {
            PanelId::Characters => self.render_character_panel(cx),
            PanelId::DocumentList => self.render_document_list(cx),
            PanelId::DocumentMap => self.render_document_map(cx),
            PanelId::FunctionList => self.render_function_list(cx),
            PanelId::Folder => self.render_folder_workspace(cx),
            PanelId::Project1 => self.render_project_panel(0, cx),
            PanelId::Project2 => self.render_project_panel(1, cx),
            PanelId::Project3 => self.render_project_panel(2, cx),
        };
        let controls = DockSide::ALL.into_iter().map(|side| {
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
                    if phase != gpui::DispatchPhase::Bubble {
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
                window.on_mouse_event(move |_: &gpui::MouseUpEvent, _, _, cx| {
                    owner.update(cx, |view, _| {
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
            .flex()
            .flex_col()
            .min_h(px(0.))
            .min_w(px(0.))
            .flex_shrink_0()
            .when(floating, |frame| frame.size_full())
            .when(
                !floating && matches!(p.dock, DockSide::Left | DockSide::Right),
                |frame| frame.w(px(p.size)).h_full(),
            )
            .when(
                !floating && matches!(p.dock, DockSide::Top | DockSide::Bottom),
                |frame| frame.h(px(p.size)).w_full(),
            )
            .bg(rgb(if self.dark { 0x282828 } else { 0xf6f6f6 }))
            .text_color(rgb(if self.dark { 0xd4d4d4 } else { 0x202020 }))
            .child(
                div()
                    .h(px(23.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .children(controls)
                    .child(
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
                    )
                    .child(
                        div()
                            .id(format!("{}-hide", id.key()))
                            .debug_selector(move || format!("{}-hide", id.key()))
                            .role(Role::Button)
                            .aria_label(format!("Hide {}", id.title()))
                            .tooltip(move |_, cx| {
                                icon_tooltip::view(format!("Hide {}", id.title()), dark, cx)
                            })
                            .px(px(3.))
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
                        .h(px(5.))
                        .w_full()
                        .cursor_crosshair()
                        .bg(rgb(0x808080))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                                view.panels.resize =
                                    Some((id, event.position, view.panels.layout.panel(id).size));
                                cx.stop_propagation();
                            }),
                        )
                        .child(resize_listener),
                )
            })
            .into_any_element()
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn project_windows_render_real_project_trees_and_restore_common_state(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        let saved = main
            .update(cx, |view, window, cx| {
                view.projects
                    .model
                    .panel_mut(0)
                    .expect("project panel")
                    .add_project("Retained project")
                    .expect("project");
                view.float_panel(PanelId::Project1, window, cx);
                view.capture_panel_state().expect("capture common state")
            })
            .expect("capture");
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
        main.update(cx, |view, window, cx| {
            view.hide_panel(PanelId::Project1, window, cx)
        })
        .expect("hide");
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1);
        main.update(cx, |view, window, cx| {
            view.restore_panel_state(&saved, window, cx)
                .expect("restore")
        })
        .expect("restore state");
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
        main.read_with(cx, |view, _| {
            assert!(view.projects.visible(0));
            assert_eq!(
                view.projects.model.panel(0).expect("panel").roots()[0].name,
                "Retained project"
            );
            assert_eq!(
                view.panels.layout.panel(PanelId::Project1).placement,
                Placement::Floating
            );
        })
        .expect("restored project tree");
    }

    #[gpui::test]
    fn panels_float_independently_close_to_hide_and_redock_retained_content(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, window, cx| {
            view.editor_mut().insert_text("retained").expect("edit");
            view.float_panel(PanelId::DocumentList, window, cx);
            view.float_panel(PanelId::DocumentMap, window, cx);
        })
        .expect("float");
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 3);
        let list = main
            .read_with(cx, |view, _| {
                *view
                    .panels
                    .windows
                    .get(&PanelId::DocumentList)
                    .expect("list window")
            })
            .expect("main");
        let map = main
            .read_with(cx, |view, _| {
                *view
                    .panels
                    .windows
                    .get(&PanelId::DocumentMap)
                    .expect("map window")
            })
            .expect("main");
        cx.dispatch_action(list.into(), CloseWindow);
        cx.run_until_parked();
        assert!(!cx.windows().contains(&list.into()));
        assert!(cx.windows().contains(&map.into()));
        main.update(cx, |view, window, cx| {
            let p = view.panels.layout.panel(PanelId::DocumentList);
            assert!(!p.visible);
            assert_eq!(p.placement, Placement::Floating);
            view.show_panel(PanelId::DocumentList, window, cx);
        })
        .expect("reopen");
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 3);
        main.update(cx, |view, window, cx| {
            view.dock_panel(PanelId::DocumentMap, DockSide::Left, window, cx)
        })
        .expect("dock");
        cx.run_until_parked();
        assert!(!cx.windows().contains(&map.into()));
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "retained");
            assert_eq!(
                view.panels.layout.panel(PanelId::DocumentMap).placement,
                Placement::Docked(DockSide::Left)
            );
        })
        .expect("retained document");
    }
}
