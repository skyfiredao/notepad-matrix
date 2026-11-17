use super::*;
use std::sync::Arc;

use super::intelligence_surface::source;
use crate::intelligence::function_list as model;
use model::{FunctionListModel, ParseOutcome};

#[derive(Clone, PartialEq, Eq)]
struct Target {
    buffer: crate::workspace::BufferId,
    view: crate::workspace::ViewId,
    language: crate::syntax::Language,
    revision: crate::input::DisplayRevision,
    source_key: String,
}

pub(super) struct FunctionListState {
    model: FunctionListModel,
    filter: gpui::Entity<text_field::TextField>,
    tree_focus: FocusHandle,
    target: Option<Target>,
    parse_task: Option<gpui::Task<()>>,
    parse_request: Option<Arc<()>>,
    selected: usize,
    preferences_open: bool,
    preference_root: Option<PathBuf>,
    preference_ready: bool,
    pending_preference: Option<Arc<()>>,
    preference_task: Option<gpui::Task<()>>,
    preference_error: Option<String>,
    _filter_changed: gpui::Subscription,
}

impl FunctionListState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        let filter =
            cx.new(|cx| text_field::TextField::new("Search functions", "function-list-search", cx));
        let changed = cx.observe(&filter, |owner, field, cx| {
            let text = field.read(cx).text();
            if owner.function_list.model.filter() != text {
                owner.function_list.model.set_filter(text);
                owner.function_list.selected = 0;
                cx.notify();
            }
        });
        Self {
            model: FunctionListModel::default(),
            filter,
            tree_focus: cx.focus_handle(),
            target: None,
            parse_task: None,
            parse_request: None,
            selected: 0,
            preferences_open: false,
            preference_root: None,
            preference_ready: false,
            pending_preference: None,
            preference_task: None,
            preference_error: None,
            _filter_changed: changed,
        }
    }
}

impl MatrixWindow {
    pub(super) fn function_list_visible(&self) -> bool {
        self.function_list.model.visible()
    }

    pub(super) fn toggle_function_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.toggle_function_list(window, cx)
        }) {
            return;
        }
        if self.function_list_visible() {
            self.close_function_list(window, cx);
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.show_panel(crate::panel_layout::PanelId::FunctionList, window, cx);
    }

    pub(super) fn open_function_list_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.function_list.model.open();
        self.refresh_function_list(window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_function_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_panel(crate::panel_layout::PanelId::FunctionList, window, cx);
    }

    pub(super) fn close_function_list_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.function_list.model.close();
        self.function_list.preferences_open = false;
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn refresh_function_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self
            .settings_store
            .as_ref()
            .map(|store| store.root().to_path_buf());
        if root != self.function_list.preference_root {
            self.function_list.preference_root = root;
            self.function_list.preference_ready = false;
            self.function_list.pending_preference = None;
            self.function_list.preference_error = None;
            self.function_list.model.set_default_sort(false);
            self.start_function_list_preference_io(None, cx);
        }
        if !self.function_list_visible() {
            return;
        }
        if self.editor().marked_range().is_some() {
            return;
        }
        if self.function_list_target().as_ref().ok() != self.function_list.target.as_ref() {
            self.reload_function_list(window, cx);
        }
    }

    fn function_list_target(&self) -> Result<Target, crate::intelligence::IntelligenceError> {
        let language = self.workspace.active().language();
        let udl_name = match language {
            crate::syntax::Language::UserDefined(id) => self
                .udl
                .registry
                .definition(id)
                .map(|definition| definition.name.as_str()),
            _ => None,
        };
        Ok(Target {
            buffer: self.workspace.active().id(),
            view: self.workspace.active_view(),
            language,
            revision: self.editor().display_revision(),
            source_key: source::source_key(language, udl_name)?,
        })
    }

    pub(super) fn reload_function_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.function_list_target() {
            Ok(target) => target,
            Err(error) => {
                self.function_list
                    .model
                    .apply_result(Err(error.to_string()));
                cx.notify();
                return;
            }
        };
        self.function_list
            .model
            .apply_result(Ok(ParseOutcome::NoParser));
        self.function_list.target = Some(target.clone());
        self.function_list.selected = 0;
        self.function_list
            .filter
            .update(cx, |field, cx| field.set_text("", window, cx));
        let request = Arc::new(());
        self.function_list.parse_request = Some(request.clone());
        let text = self.editor().committed_text();
        let worker_target = target.clone();
        let worker = cx.background_executor().spawn(async move {
            source::catalog()
                .and_then(|catalog| catalog.parse_functions(&worker_target.source_key, &text))
                .map_err(|error| error.to_string())
        });
        self.function_list.parse_task = Some(cx.spawn(async move |owner, cx| {
            let result = worker.await;
            if let Err(error) = owner.update(cx, |view, cx| {
                let current = view
                    .function_list
                    .parse_request
                    .as_ref()
                    .is_some_and(|pending| Arc::ptr_eq(pending, &request))
                    && view.function_list_target().as_ref().ok() == Some(&target);
                if !current {
                    return;
                }
                view.function_list.parse_request = None;
                let filter = view.function_list.filter.read(cx).text();
                view.function_list.model.apply_result(result);
                view.function_list.model.set_filter(filter);
                cx.notify();
            }) {
                eprintln!("Function List parser lost its document: {error}");
            }
        }));
        if self.function_list.preference_error.is_some()
            && self.function_list.pending_preference.is_none()
        {
            self.start_function_list_preference_io(None, cx);
        }
        cx.notify();
    }

    fn jump_function_list(
        &mut self,
        jump_byte: usize,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed_target = target.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.jump_function_list(jump_byte, routed_target, window, cx)
        }) {
            return;
        }
        if self.function_list_target().as_ref().ok() != self.function_list.target.as_ref() {
            return;
        }
        let Some(current) = &self.function_list.target else {
            return;
        };
        if *current != target || self.editor().marked_range().is_some() {
            return;
        }
        let position = ByteOffset(jump_byte);
        let result = self.editor_mut().select(Selection {
            anchor: position,
            head: position,
        });
        if result.is_ok() {
            self.editor_focus.focus(window, cx);
        }
        self.editor_changed(result, window, cx);
    }

    fn toggle_function_list_sort(&mut self, cx: &mut Context<Self>) {
        self.function_list.model.toggle_sort();
        cx.notify();
    }

    fn toggle_function_list_default_sort(&mut self, cx: &mut Context<Self>) {
        if !self.function_list.preference_ready || self.function_list.pending_preference.is_some() {
            return;
        }
        self.start_function_list_preference_io(Some(!self.function_list.model.default_sort()), cx);
    }

    fn start_function_list_preference_io(&mut self, write: Option<bool>, cx: &mut Context<Self>) {
        let Some(root) = self.function_list.preference_root.clone() else {
            return;
        };
        let Some(home) = root.parent().map(PathBuf::from) else {
            return;
        };
        let request = Arc::new(());
        self.function_list.pending_preference = Some(request.clone());
        self.function_list.preference_error = None;
        let worker = cx.background_executor().spawn(async move {
            let store = ConfigStore::under_home(&home);
            let settings = if let Some(value) = write {
                store.update_settings(|settings| {
                    settings["function_list_sort_default"] = value.into();
                    Ok(())
                })
            } else {
                store.load_settings()
            }?;
            Ok::<_, io::Error>(
                settings
                    .get("function_list_sort_default")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            )
        });
        if let Some(previous) = self.function_list.preference_task.take() {
            previous.detach();
        }
        self.function_list.preference_task = Some(cx.spawn(async move |owner, cx| {
            let result = worker.await;
            if let Err(error) = owner.update(cx, |view, cx| {
                let same_store = view
                    .settings_store
                    .as_ref()
                    .is_some_and(|store| store.root() == root);
                let current = same_store
                    && view
                        .function_list
                        .pending_preference
                        .as_ref()
                        .is_some_and(|pending| Arc::ptr_eq(pending, &request));
                if !current {
                    if let Err(error) = result {
                        eprintln!("Stale Function List preference operation failed: {error}");
                    }
                    return;
                }
                view.function_list.pending_preference = None;
                match result {
                    Ok(value) => {
                        view.function_list.model.set_default_sort(value);
                        if write.is_none() {
                            view.function_list.model.apply_default_sort();
                        }
                        view.function_list.preference_ready = true;
                    }
                    Err(error) => {
                        view.function_list.preference_error = Some(format!(
                            "Function List preferences could not be {}: {error}",
                            if write.is_some() { "saved" } else { "loaded" }
                        ));
                    }
                }
                cx.notify();
            }) {
                eprintln!("Function List preference completion lost its document: {error}");
            }
        }));
        cx.notify();
    }

    pub(super) fn render_function_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.function_list_visible() {
            return None;
        }
        let sorted = self.function_list.model.sorted();
        let preference_enabled =
            self.function_list.preference_ready && self.function_list.pending_preference.is_none();
        let rows = self.function_list.model.rows();
        let target = self.function_list.target.clone();
        let parsed = matches!(self.function_list.model.outcome(), ParseOutcome::Parsed(_));
        let dark = self.dark;
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(label)
                .tooltip(move |_, cx| icon_tooltip::view(label, dark, cx))
                .cursor_pointer()
                .px(px(5.))
                .py(px(2.))
                .hover(move |style| style.bg(rgb(if dark { 0x444444 } else { 0xdedede })))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        Some(
            div()
                .id("function-list-panel")
                .debug_selector(|| "function-list-panel".into())
                .role(Role::Group)
                .aria_label("Function List")
                .relative()
                .w_full()
                .flex_shrink_0()
                .h_full()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .bg(rgb(if dark { 0x282828 } else { 0xf6f6f6 }))
                .text_color(rgb(if dark { 0xd4d4d4 } else { 0x202020 }))
                .text_size(px(12.))
                .border_l_1()
                .border_color(rgb(0x808080))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        if view.function_list.preferences_open {
                            view.function_list.preferences_open = false;
                        } else {
                            view.function_list.model.set_filter(String::new());
                            view.function_list
                                .filter
                                .update(cx, |field, cx| field.set_text("", window, cx));
                            view.editor_focus.focus(window, cx);
                        }
                        cx.notify();
                        cx.stop_propagation();
                    } else if event.keystroke.key == "tab"
                        && view.function_list.tree_focus.is_focused(window)
                    {
                        view.function_list
                            .filter
                            .update(cx, |field, cx| field.focus(window, cx));
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .h(px(26.))
                        .flex_shrink_0()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("Function List")
                        .child(
                            button("function-list-close", "Close Function List")
                                .child("×")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_function_list(window, cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(2.))
                        .child(
                            button("function-list-sort", "Sort functions (A to Z)")
                                .aria_toggled(sorted.into())
                                .when(sorted, |button| {
                                    button.bg(rgb(if dark { 0x404b5a } else { 0xcce8ff }))
                                })
                                .child("A–Z")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.toggle_function_list_sort(cx);
                                    cx.stop_propagation();
                                })),
                        )
                        .child(
                            button("function-list-reload", "Reload Function List")
                                .child("↻")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.reload_function_list(window, cx);
                                    cx.stop_propagation();
                                })),
                        )
                        .child(
                            button("function-list-preferences", "Function List preferences")
                                .aria_toggled(self.function_list.preferences_open.into())
                                .child("⚙")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.function_list.preferences_open =
                                        !view.function_list.preferences_open;
                                    cx.notify();
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .child(
                    div()
                        .px(px(4.))
                        .py(px(4.))
                        .flex_shrink_0()
                        .child(self.function_list.filter.clone()),
                )
                .child(
                    div()
                        .id("function-list-tree")
                        .debug_selector(|| "function-list-tree".into())
                        .role(Role::Tree)
                        .aria_label("Functions")
                        .track_focus(&self.function_list.tree_focus)
                        .tab_index(0)
                        .aria_description(match self.function_list.model.outcome() {
                            ParseOutcome::NoParser => "No function parser for this language",
                            ParseOutcome::Parsed(_) => {
                                "Functions and classes; Enter toggles groups or jumps to the selected function"
                            }
                        })
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .overflow_y_scroll()
                        .bg(rgb(if dark { 0x1e1e1e } else { 0xffffff }))
                        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                            let rows = view.function_list.model.rows();
                            if rows.is_empty() {
                                return;
                            }
                            match event.keystroke.key.as_str() {
                                "up" => {
                                    view.function_list.selected =
                                        view.function_list.selected.saturating_sub(1)
                                }
                                "down" => {
                                    view.function_list.selected =
                                        (view.function_list.selected + 1).min(rows.len() - 1)
                                }
                                "left" | "right" => {
                                    if let Some(row) =
                                        rows.get(view.function_list.selected).filter(|row| {
                                            row.has_children
                                                && row.expanded != (event.keystroke.key == "right")
                                        })
                                    {
                                        view.function_list.model.toggle_expanded(row.jump_byte);
                                    }
                                }
                                "enter" => {
                                    if let Some(row) = rows.get(view.function_list.selected) {
                                        if row.has_children {
                                            view.function_list.model.toggle_expanded(row.jump_byte);
                                        } else if let Some(target) =
                                            view.function_list.target.clone()
                                        {
                                            view.jump_function_list(
                                                row.jump_byte,
                                                target,
                                                window,
                                                cx,
                                            );
                                        }
                                    }
                                }
                                _ => return,
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }))
                        .when(parsed, |tree| {
                            tree.child(
                                div()
                                    .id("function-list-root")
                                    .debug_selector(|| "function-list-root".into())
                                    .role(Role::TreeItem)
                                    .px(px(4.))
                                    .py(px(2.))
                                    .child(self.workspace.active().name()),
                            )
                        })
                        .children(rows.into_iter().enumerate().map(|(index, row)| {
                            let jump_byte = row.jump_byte;
                            let has_children = row.has_children;
                            let target = target.clone();
                            let prefix = if has_children {
                                if row.expanded { "▾ " } else { "▸ " }
                            } else {
                                "  "
                            };
                            div()
                                .id(("function-list-entry", index))
                                .debug_selector(|| "function-list-entry".into())
                                .role(Role::TreeItem)
                                .aria_label(row.name.clone())
                                .aria_selected(index == self.function_list.selected)
                                .when(has_children, |item| item.aria_expanded(row.expanded))
                                .pl(px(8. + row.depth as f32 * 14.))
                                .py(px(2.))
                                .cursor_pointer()
                                .when(index == self.function_list.selected, |row| {
                                    row.bg(rgb(if dark { 0x30465a } else { 0xcce8ff }))
                                })
                                .child(
                                    div()
                                        .id(("function-list-expander", index))
                                        .w(px(14.))
                                        .child(prefix)
                                        .when(has_children, |expander| {
                                            expander.on_click(cx.listener(move |view, _, _, cx| {
                                                view.function_list.model.toggle_expanded(jump_byte);
                                                cx.notify();
                                                cx.stop_propagation();
                                            }))
                                        }),
                                )
                                .child(row.name)
                                .flex()
                                .items_center()
                                .on_click(cx.listener(
                                    move |view, event: &gpui::ClickEvent, window, cx| {
                                        view.function_list.selected = index;
                                        if event.click_count() == 2 {
                                            if has_children {
                                                view.function_list.model.toggle_expanded(jump_byte);
                                            } else if let Some(target) = target.clone() {
                                                view.jump_function_list(
                                                    jump_byte, target, window, cx,
                                                );
                                            }
                                        }
                                        cx.notify();
                                        cx.stop_propagation();
                                    },
                                ))
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, window, cx| {
                                view.function_list.tree_focus.focus(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .when_some(
                    self.function_list.model.error().map(str::to_owned),
                    |panel, error| {
                        panel.child(
                            div()
                                .id("function-list-parser-error")
                                .role(Role::Status)
                                .px(px(4.))
                                .child(error),
                        )
                    },
                )
                .when(self.function_list.parse_request.is_some(), |panel| {
                    panel.child(
                        div()
                            .id("function-list-parse-status")
                            .role(Role::Status)
                            .px(px(4.))
                            .child("Parsing functions…"),
                    )
                })
                .when_some(
                    self.function_list.preference_error.clone(),
                    |panel, error| {
                        panel.child(
                            div()
                                .id("function-list-error")
                                .role(Role::Status)
                                .px(px(4.))
                                .child(error),
                        )
                    },
                )
                .when(self.function_list.pending_preference.is_some(), |panel| {
                    panel.child(
                        div()
                            .id("function-list-preferences-label")
                            .role(Role::Status)
                            .px(px(4.))
                            .child("Updating preferences…"),
                    )
                })
                .when(self.function_list.preferences_open, |panel| {
                    panel.child(
                        div()
                            .id("function-list-preferences-menu")
                            .role(Role::Menu)
                            .absolute()
                            .top(px(50.))
                            .left(px(4.))
                            .p(px(4.))
                            .border_1()
                            .border_color(rgb(0x808080))
                            .bg(rgb(if dark { 0x282828 } else { 0xf6f6f6 }))
                            .occlude()
                            .child(
                                div()
                                    .id("function-list-default-sort")
                                    .debug_selector(|| "function-list-default-sort".into())
                                    .role(Role::MenuItemCheckBox)
                                    .aria_label("Sort functions (A to Z) by default")
                                    .aria_toggled(self.function_list.model.default_sort().into())
                                    .a11y_synthetic_children(move |builder| {
                                        if !preference_enabled {
                                            builder.parent_node().set_disabled();
                                        }
                                    })
                                    .when(!preference_enabled, |item| item.opacity(0.5))
                                    .child(if self.function_list.model.default_sort() {
                                        "☑ Sort functions (A to Z) by default"
                                    } else {
                                        "☐ Sort functions (A to Z) by default"
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.toggle_function_list_default_sort(cx);
                                        view.function_list.preferences_open = false;
                                        cx.notify();
                                        cx.stop_propagation();
                                    })),
                            ),
                    )
                })
                .into_any_element(),
        )
    }
}

