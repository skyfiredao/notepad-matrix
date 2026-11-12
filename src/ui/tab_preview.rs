use super::*;

#[path = "tab_preview_model.rs"]
mod model;
use model::{DocumentPreview, PreviewHover, PreviewLocation};
use std::sync::Arc;

pub(super) struct PreviewState {
    enabled: bool,
    hover: PreviewHover,
    scroll: gpui::UniformListScrollHandle,
    window: Option<gpui::WindowHandle<PreviewWindow>>,
    opening: Option<Arc<()>>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            enabled: false,
            hover: PreviewHover::default(),
            scroll: gpui::UniformListScrollHandle::new(),
            window: None,
            opening: None,
        }
    }
}

struct PreviewWindow {
    owner: gpui::WeakEntity<MatrixWindow>,
    focus: FocusHandle,
    _changed: gpui::Subscription,
    _released: gpui::Subscription,
}

impl PreviewWindow {
    fn new(owner: gpui::Entity<MatrixWindow>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changed = cx.observe_in(&owner, window, |_, owner, window, cx| {
            if owner.read(cx).tab_preview_visible() {
                cx.notify();
            } else {
                window.remove_window();
            }
        });
        let released =
            cx.observe_release_in(&owner, window, |_, _, window, _| window.remove_window());
        let weak = owner.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            if let Err(error) = weak.update(cx, |owner, cx| owner.dismiss_tab_preview(cx)) {
                eprintln!("preview close lost its document: {error}");
            }
            true
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            owner: owner.downgrade(),
            focus,
            _changed: changed,
            _released: released,
        }
    }
}

impl Render for PreviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self
            .owner
            .update(cx, |owner, cx| owner.render_tab_preview(cx))
        {
            Ok(content) => content,
            Err(error) => {
                eprintln!("preview render lost its document: {error}");
                window.remove_window();
                None
            }
        };
        let close_owner = self.owner.clone();
        div()
            .id("matrix-tab-preview-window")
            .key_context("MatrixAuxiliary")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family("Helvetica")
            .text_size(px(12.))
            .on_action(cx.listener(move |_, _: &CloseWindow, window, cx| {
                if let Err(error) =
                    close_owner.update(cx, |owner, cx| owner.dismiss_tab_preview(cx))
                {
                    eprintln!("preview shortcut lost its document: {error}");
                }
                window.remove_window();
                cx.stop_propagation();
            }))
            .children(content)
    }
}

impl MatrixWindow {
    pub(super) fn tab_preview_enabled(&self) -> bool {
        self.panels.preview.enabled
    }

    pub(super) fn set_tab_preview_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.panels.preview.enabled = enabled;
        if !enabled {
            self.dismiss_tab_preview(cx);
        }
        cx.notify();
    }

    pub(super) fn set_tab_preview(
        &mut self,
        target: Option<(crate::workspace::ViewId, crate::workspace::BufferId)>,
        cx: &mut Context<Self>,
    ) {
        let target = target.map(|(pane, id)| PreviewLocation { pane, id });
        if !self.tab_preview_enabled()
            || target.is_some_and(|target| !target.allowed(&self.workspace))
        {
            self.dismiss_tab_preview(cx);
            return;
        }
        let previous = self.panels.preview.hover.target();
        self.panels.preview.hover.tab(target);
        if self.panels.preview.hover.target() != previous {
            self.panels.preview.scroll = gpui::UniformListScrollHandle::new();
        }
        self.sync_tab_preview_window(cx);
        cx.notify();
    }

    pub(super) fn tab_preview_visible(&self) -> bool {
        self.tab_preview_enabled()
            && self
                .panels
                .preview
                .hover
                .target()
                .is_some_and(|target| target.allowed(&self.workspace))
    }

    pub(super) fn dismiss_tab_preview(&mut self, cx: &mut Context<Self>) {
        self.panels.preview.hover.dismiss();
        self.sync_tab_preview_window(cx);
        cx.notify();
    }

    fn sync_tab_preview_window(&mut self, cx: &mut Context<Self>) {
        if !self.tab_preview_visible() {
            self.panels.preview.opening = None;
            if let Some(handle) = self.panels.preview.window.take() {
                cx.defer(move |cx| {
                    if cx.windows().contains(&handle.into())
                        && let Err(error) = handle.update(cx, |_, window, _| window.remove_window())
                    {
                        eprintln!("could not close preview: {error}");
                    }
                });
            }
            return;
        }
        if self
            .panels
            .preview
            .window
            .is_some_and(|handle| cx.windows().contains(&handle.into()))
            || self.panels.preview.opening.is_some()
        {
            return;
        }
        let request = Arc::new(());
        self.panels.preview.opening = Some(request.clone());
        let weak = cx.weak_entity();
        let document = self.document_window;
        cx.defer(move |cx| {
            let Some(owner) = weak.upgrade() else {
                return;
            };
            let current = owner
                .read(cx)
                .panels
                .preview
                .opening
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &request));
            if !current || !owner.read(cx).tab_preview_visible() {
                return;
            }
            let bounds = document.update(cx, |_, window, _| {
                let bounds = window.window_bounds().get_bounds();
                let mouse = window.mouse_position();
                Bounds {
                    origin: gpui::point(
                        bounds.origin.x + mouse.x,
                        bounds.origin.y + mouse.y + px(24.),
                    ),
                    size: size(px(440.), px(300.)),
                }
            });
            let bounds = match bounds {
                Ok(bounds) => bounds,
                Err(error) => {
                    owner.update(cx, |owner, cx| {
                        owner.panels.preview.opening = None;
                        owner.notify_editor_error(
                            format!("Could not position document preview: {error}"),
                            cx,
                        );
                    });
                    return;
                }
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: gpui::WindowKind::Floating,
                focus: false,
                is_movable: false,
                is_resizable: false,
                window_min_size: Some(size(px(200.), px(100.))),
                ..Default::default()
            };
            let observed = owner.clone();
            let opened = cx.open_window(options, move |window, cx| {
                cx.new(|cx| PreviewWindow::new(observed, window, cx))
            });
            owner.update(cx, |owner, cx| {
                owner.panels.preview.opening = None;
                match opened {
                    Ok(handle) => {
                        owner.panels.preview.window = Some(handle);
                    }
                    Err(error) => {
                        owner.panels.preview.hover.dismiss();
                        owner.notify_editor_error(
                            format!("Could not open document preview: {error}"),
                            cx,
                        );
                    }
                }
            });
        });
    }

    pub(super) fn render_tab_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self
            .panels
            .preview
            .hover
            .target()
            .filter(|target| target.allowed(&self.workspace))?;
        let preview = DocumentPreview::capture(&self.workspace, target.pane, target.id)?;
        let pane = target.pane;
        let id = target.id;
        let name = self
            .workspace
            .buffers()
            .iter()
            .find(|buffer| buffer.id() == id)?
            .name();
        Some(
            div()
                .id("tab-document-preview")
                .role(Role::Group)
                .aria_label(format!("Read-only preview of {name}"))
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .border_1()
                .border_color(rgb(0x808080))
                .bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
                .text_color(rgb(if self.dark { 0xd4d4d4 } else { 0x202020 }))
                .child(
                    div()
                        .h(px(24.))
                        .flex_shrink_0()
                        .px(px(6.))
                        .child(format!("{name} · Preview (read-only)")),
                )
                .child(
                    gpui::uniform_list(
                        "tab-preview-lines",
                        preview.line_count(),
                        cx.processor(move |view, range: std::ops::Range<usize>, _, _| {
                            let preview = DocumentPreview::capture(&view.workspace, pane, id);
                            range
                                .filter_map(|index| {
                                    preview.as_ref()?.line(index).map(|line| {
                                        div()
                                            .id(("tab-preview-line", index))
                                            .h(px(17.))
                                            .flex()
                                            .font_family("Menlo")
                                            .text_size(px(11.))
                                            .whitespace_nowrap()
                                            .child(
                                                div()
                                                    .w(px(42.))
                                                    .flex_shrink_0()
                                                    .text_color(rgb(0x808080))
                                                    .child(format!("{}", index + 1)),
                                            )
                                            .child(line)
                                    })
                                })
                                .collect()
                        }),
                    )
                    .track_scroll(&self.panels.preview.scroll)
                    .flex_1(),
                )
                .into_any_element(),
        )
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn hovering_inactive_tab_does_not_open_preview_by_default(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated preview settings");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let target = view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(ConfigStore::under_home(home.path()));
            let target = view.workspace.active().id();
            view.new_file(window, cx);
            target
        });
        assert_eq!(target.0, 1);
        let tab = cx.debug_bounds("buffer-tab-1").expect("inactive tab");
        cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1);
        view.read_with(cx, |view, _| assert!(!view.tab_preview_visible()));
        cx.dispatch_action(ShowPreferences);
        let preferences = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Preferences]
        });
        let mut preferences = gpui::VisualTestContext::from_window(preferences.into(), &cx.cx);
        let toggle = preferences
            .debug_bounds("preferences-tab-preview")
            .expect("preview preference");
        preferences.simulate_click(toggle.center(), gpui::Modifiers::default());
        assert_eq!(
            ConfigStore::under_home(home.path())
                .load_settings()
                .unwrap()["docPeekOnTab"],
            true
        );
        preferences.dispatch_action(ClosePreferences);
        cx.run_until_parked();
        let editor = cx.debug_bounds("matrix-editor").expect("editor");
        cx.simulate_mouse_move(editor.center(), None, gpui::Modifiers::default());
        cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
        cx.simulate_mouse_move(editor.center(), None, gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui::test]
    fn enabled_preview_is_nonactivating_and_retires_as_soon_as_tab_is_left(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::workspace::ViewId;
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        let target = main
            .update(cx, |view, _, _| {
                let target = view.workspace.active().id();
                view.editor_mut()
                    .insert_text("preview content")
                    .expect("edit");
                view.workspace.new_buffer().expect("second buffer");
                target
            })
            .expect("target");
        let selection = main
            .read_with(cx, |view, _| view.editor().selection())
            .expect("selection");
        main.update(cx, |view, _, cx| {
            view.set_tab_preview_enabled(true, cx);
            view.set_tab_preview(Some((ViewId::Primary, target)), cx)
        })
        .expect("hover");
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
        let preview = main
            .read_with(cx, |view, _| {
                view.panels.preview.window.expect("preview window")
            })
            .expect("main");
        preview
            .update(cx, |_, window, _| assert!(!window.is_window_active()))
            .expect("nonactivating preview");
        main.update(cx, |view, _, cx| view.set_tab_preview(None, cx))
            .expect("leave tab");
        cx.run_until_parked();
        assert_eq!(cx.windows(), vec![main.into()]);
        main.read_with(cx, |view, _| {
            assert!(!view.tab_preview_visible());
            assert_eq!(view.editor().selection(), selection);
            assert_ne!(view.workspace.active().id(), target);
            assert_eq!(
                view.workspace
                    .editor_in_view(ViewId::Primary, target)
                    .expect("previewed editor")
                    .committed_text(),
                "preview content"
            );
        })
        .expect("unchanged document");
    }
}
