use super::*;

impl MatrixWindow {
    pub(super) fn reload_bindings() -> [KeyBinding; 8] {
        [
            KeyBinding::new("enter", ActivateReloadChoice, Some("MatrixReload")),
            KeyBinding::new("escape", CancelReload, Some("MatrixReload")),
            KeyBinding::new("tab", ToggleReloadChoice, Some("MatrixReload")),
            KeyBinding::new("shift-tab", ToggleReloadChoice, Some("MatrixReload")),
            KeyBinding::new("left", ToggleReloadChoice, Some("MatrixReload")),
            KeyBinding::new("right", ToggleReloadChoice, Some("MatrixReload")),
            KeyBinding::new("alt-y", ConfirmReload, Some("MatrixReload")),
            KeyBinding::new("alt-n", CancelReload, Some("MatrixReload")),
        ]
    }

    pub(super) fn request_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() {
            return;
        }
        if self.file_busy
            || self.file_open_sequence_pending()
            || self.close_dialog
            || self.save_all_restore.is_some()
            || self.reload_target.is_some()
            || self.disk_file().is_none()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.reload_target = Some(self.workspace.active().id());
        self.reload_choice_yes = true;
        if self.workspace.active().is_modified() {
            self.reload_dialog = true;
            self.close_focus.focus(window, cx);
            cx.notify();
        } else {
            self.begin_reload(window, cx);
        }
    }

    pub(super) fn confirm_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.confirm_reload(window, cx)
        }) {
            return;
        }
        if !self.reload_dialog || self.file_busy {
            return;
        }
        self.reload_dialog = false;
        self.begin_reload(window, cx);
    }

    pub(super) fn cancel_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.cancel_reload(window, cx)
        }) {
            return;
        }
        if !self.reload_dialog || self.file_busy {
            return;
        }
        self.reload_dialog = false;
        self.reload_target = None;
        self.editor_focus.focus(window, cx);
        if self.reload_external_change {
            self.synchronize_recovery(cx);
        }
        self.finish_recovery_reload(window, cx);
        cx.notify();
    }

    fn begin_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.reload_target else {
            return;
        };
        let Some(file) = self.disk_file() else {
            return;
        };
        let path = file.path().to_owned();
        let byte_code_page = self.workspace.active().byte_code_page();
        self.file_busy = true;
        let executor = cx.background_executor().clone();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let loaded = executor
                .spawn(async move { crate::files::DiskFile::load_with_code_page(&path, byte_code_page) })
                .await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                view.reload_target = None;
                if view.workspace.active().id() != id {
                    view.notify_editor_error("Reload refused: active document changed".into(), cx);
                    view.finish_recovery_reload(window, cx);
                    return;
                }
                let result = loaded.and_then(|(file, text)| {
                    if !view.cancel_view_composition(window, cx) {
                        return Err(io::Error::other(
                            "could not cancel native composition before reload",
                        ));
                    }
                    view.workspace
                        .reload_buffer(
                            id,
                            file,
                            &text,
                            crate::workspace::CloseDisposition::DiscardConfirmed,
                        )
                        .map_err(|error| io::Error::other(error.to_string()))
                });
                match result {
                    Ok(()) => {
                        view.initialize_active_change_history();
                        let top_line = view.editor_top_line;
                        view.reset_active_view(window, cx);
                        view.preferences_apply_large_file_open_policy(window, cx);
                        view.editor_top_line = top_line;
                        view.editor_follow_caret = false;
                    }
                    Err(error) => {
                        view.notify_editor_error(
                            format!("Reload failed: {error}"),
                            cx,
                        );
                        view.editor_focus.focus(window, cx);
                    }
                }
                view.finish_recovery_reload(window, cx);
            }) {
                eprintln!("reload completion could not reach its window: {error}");
            }
        })
        .detach();
    }

    pub(super) fn render_reload_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use super::dialog_font::px;
        if !self.reload_dialog {
            return None;
        }
        let button = |id: &'static str, label: &'static str, selected: bool| {
            let label = crate::localization::current_dialog_control("DoSaveOrNot", if id == "reload-yes" { 6 } else { 7 }).unwrap_or(label);
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(label)
                .aria_selected(selected)
                .border_1()
                .border_color(rgb(if selected { 0x0078d4 } else { 0x808080 }))
                .when(selected, |style| style.bg(rgb(0xdceeff)))
                .px(px(18.))
                .py(px(5.))
                .child(label)
        };
        Some(
            div()
                .id("reload-overlay")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .child(
                    div()
                        .id("reload-dialog")
                        .role(Role::Dialog)
                        .aria_label(crate::localization::dialog_title(&self.native_locale(), "DoReloadOrNot").unwrap_or("Reload"))
                        .track_focus(&self.close_focus)
                        .key_context("MatrixReload")
                        .on_action(cx.listener(|view, _: &ToggleReloadChoice, _, cx| {
                            if view.reload_dialog && !view.file_busy {
                                view.reload_choice_yes = !view.reload_choice_yes;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &ActivateReloadChoice, window, cx| {
                            if view.reload_choice_yes {
                                view.confirm_reload(window, cx);
                            } else {
                                view.cancel_reload(window, cx);
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &ConfirmReload, window, cx| {
                            view.confirm_reload(window, cx);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &CancelReload, window, cx| {
                            view.cancel_reload(window, cx);
                            cx.stop_propagation();
                        }))
                        .w_full()
                        .p(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .bg(rgb(0xf0f0f0))
                        .text_color(rgb(0x202020))
                        .border_1()
                        .border_color(rgb(0x808080))
                        .child(if self.reload_external_change {
                            crate::localization::dialog_message(&self.native_locale(),
                                if self.workspace.active().is_modified() { "DoReloadOrNotAndLooseChange" } else { "DoReloadOrNot" })
                                .unwrap_or("\"$STR_REPLACE$\"\n\nThis file has been modified by another program.\nDo you want to reload it?")
                                .replace("Notepad++", "Notepad Matrix")
                                .replace("$STR_REPLACE$", &self.disk_file().map(|file| file.path().display().to_string()).unwrap_or_default())
                        } else {
                            "Reload the current file and lose unsaved changes?".into()
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(12.))
                                .child(
                                    button("reload-yes", "Yes", self.reload_choice_yes).on_click(
                                        cx.listener(|view, _, window, cx| {
                                            view.confirm_reload(window, cx)
                                        }),
                                    ),
                                )
                                .child(
                                    button("reload-no", "No", !self.reload_choice_yes).on_click(
                                        cx.listener(|view, _, window, cx| {
                                            view.cancel_reload(window, cx)
                                        }),
                                    ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
