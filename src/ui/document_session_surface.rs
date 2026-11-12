use super::*;
use crate::workspace::{DocumentViewState, TabSort, ViewId};

gpui::actions!(
    document_session,
    [
        ConfirmBackupFailure,
        CancelBackupFailure,
        ToggleBackupChoice
    ]
);

pub(super) struct BackupFailureState {
    sender: Option<std::sync::mpsc::Sender<bool>>,
    message: String,
    save_anyway: bool,
    focus: FocusHandle,
}

impl BackupFailureState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            sender: None,
            message: String::new(),
            save_anyway: true,
            focus: cx.focus_handle(),
        }
    }
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    let context = Some("MatrixDocumentSwitcher");
    vec![
        KeyBinding::new("enter", ConfirmBackupFailure, Some("MatrixBackupFailure")),
        KeyBinding::new("escape", CancelBackupFailure, Some("MatrixBackupFailure")),
        KeyBinding::new("tab", ToggleBackupChoice, Some("MatrixBackupFailure")),
        KeyBinding::new("shift-tab", ToggleBackupChoice, Some("MatrixBackupFailure")),
        KeyBinding::new("left", ToggleBackupChoice, Some("MatrixBackupFailure")),
        KeyBinding::new("right", ToggleBackupChoice, Some("MatrixBackupFailure")),
        KeyBinding::new("ctrl-tab", MruNextBuffer, context),
        KeyBinding::new("ctrl-shift-tab", MruPreviousBuffer, context),
        KeyBinding::new("ctrl-down", MruNextBuffer, context),
        KeyBinding::new("ctrl-right", MruNextBuffer, context),
        KeyBinding::new("ctrl-up", MruPreviousBuffer, context),
        KeyBinding::new("ctrl-left", MruPreviousBuffer, context),
        KeyBinding::new("escape", CancelDocumentSwitcher, context),
        KeyBinding::new("ctrl-w", CancelDocumentSwitcher, context),
        KeyBinding::new("ctrl-f4", CancelDocumentSwitcher, context),
        KeyBinding::new("enter", CommitDocumentSwitcher, context),
    ]
}

impl MatrixWindow {
    pub(super) fn backup_failure_visible(&self) -> bool {
        self.backup_failure.sender.is_some()
    }

    pub(super) fn focus_backup_failure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.backup_failure.focus.focus(window, cx);
    }

    pub(super) fn dismiss_backup_failure(&mut self, cx: &mut Context<Self>) {
        self.resolve_backup_failure(false, cx);
    }

    fn resolve_backup_failure(&mut self, save_anyway: bool, cx: &mut Context<Self>) {
        if let Some(sender) = self.backup_failure.sender.take()
            && let Err(error) = sender.send(save_anyway)
        {
            eprintln!("backup confirmation no longer has a pending save: {error}");
        }
        cx.notify();
    }

    pub(super) fn request_backup_failure(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<std::sync::mpsc::Receiver<bool>> {
        if self.backup_failure.sender.is_some() {
            return Err(io::Error::other(
                "a save-backup confirmation is already open",
            ));
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.backup_failure.sender = Some(sender);
        self.backup_failure.message = message;
        self.backup_failure.save_anyway = true;
        self.present_floating(floating_windows::DialogKind::BackupFailure, window, cx);
        cx.notify();
        Ok(receiver)
    }

    pub(super) fn render_backup_failure(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.backup_failure_visible() {
            return None;
        }
        let choice = self.backup_failure.save_anyway;
        Some(
            div()
                .id("backup-failure-dialog")
                .debug_selector(|| "backup-failure-dialog".into())
                .role(Role::Dialog)
                .aria_label("File Backup Failed")
                .track_focus(&self.backup_failure.focus)
                .key_context("MatrixBackupFailure")
                .w_full()
                .h_full()
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .on_action(cx.listener(|view, _: &ConfirmBackupFailure, _, cx| {
                    view.resolve_backup_failure(view.backup_failure.save_anyway, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &CancelBackupFailure, _, cx| {
                    view.resolve_backup_failure(false, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &ToggleBackupChoice, _, cx| {
                    view.backup_failure.save_anyway = !view.backup_failure.save_anyway;
                    cx.notify();
                    cx.stop_propagation();
                }))
                .child(self.backup_failure.message.clone())
                .child("Save the current file anyway?")
                .child(
                    div()
                        .flex()
                        .gap(px(12.))
                        .children([true, false].into_iter().map(|save_anyway| {
                            div()
                                .id(if save_anyway {
                                    "backup-save-anyway"
                                } else {
                                    "backup-save-cancel"
                                })
                                .role(Role::Button)
                                .aria_label(if save_anyway { "Yes" } else { "No" })
                                .aria_selected(save_anyway == choice)
                                .border_1()
                                .border_color(rgb(if save_anyway == choice {
                                    0x0078d4
                                } else {
                                    0x808080
                                }))
                                .px(px(16.))
                                .py(px(4.))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.resolve_backup_failure(save_anyway, cx)
                                }))
                                .child(if save_anyway { "Yes" } else { "No" })
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn persist_recent_files(&self) -> io::Result<()> {
        let Some(store) = &self.settings_store else {
            return Ok(());
        };
        let paths = serde_json::to_value(self.workspace.recently_closed())?;
        let settings = store.load_settings()?;
        if settings.get("recent_files") == Some(&paths) {
            return Ok(());
        }
        store.update_settings(|settings| {
            settings["recent_files"] = paths;
            settings["recent_files_limit"] = self.workspace.recent_files_limit().into();
            Ok(())
        })?;
        Ok(())
    }

    pub(super) fn start_save_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_navigation_blocked() {
            return;
        }
        if let Err(error) = self.capture_session_ui() {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        let archive = match crate::recovery::SessionFile::capture(&self.workspace) {
            Ok(archive) => archive,
            Err(error) => {
                self.notify_editor_error(format!("Could not capture session: {error}"), cx);
                return;
            }
        };
        let directory = match &self.settings_store {
            Some(store) => match store.document_path(std::path::Path::new("sessions/prepare.json"))
            {
                Ok(path) => path
                    .parent()
                    .map(std::path::Path::to_owned)
                    .unwrap_or_else(|| store.root().to_owned()),
                Err(error) => {
                    self.notify_editor_error(
                        format!("Could not prepare session directory: {error}"),
                        cx,
                    );
                    return;
                }
            },
            None => std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
        };
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let prompt = cx.prompt_for_new_path(&directory, Some("session.json"));
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(path))) => {
                    executor
                        .spawn(async move {
                            archive.save(&path, crate::files::OverwritePolicy::Confirmed)
                        })
                        .await
                }
                Ok(Ok(None)) => Ok(()),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(format!(
                    "session save chooser disconnected: {error}"
                ))),
            };
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.file_busy = false;
                if let Err(error) = result {
                    view.notify_editor_error(format!("Could not save session: {error}"), cx);
                }
                cx.notify();
            }) {
                eprintln!("session save lost its document window: {error}");
            }
        })
        .detach();
    }

    pub(super) fn start_load_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_navigation_blocked() || !self.cancel_view_composition(window, cx) {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Load Session".into()),
        });
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let loaded = match prompt.await {
                Ok(Ok(Some(paths))) => {
                    executor
                        .spawn(async move {
                            paths
                                .first()
                                .map(|path| crate::recovery::SessionFile::load(path))
                                .transpose()
                        })
                        .await
                }
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(format!(
                    "session open chooser disconnected: {error}"
                ))),
            };
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match loaded {
                    Ok(Some(archive)) => match archive.restore_into(&mut view.workspace) {
                        Ok(warnings) => {
                            view.restore_session_ui(window, cx);
                            view.restore_session_monitoring(window, cx);
                            view.reset_active_view(window, cx);
                            if !warnings.is_empty() {
                                view.notify_editor_error(warnings.join("\n"), cx);
                            }
                        }
                        Err(error) => {
                            view.notify_editor_error(format!("Could not load session: {error}"), cx)
                        }
                    },
                    Ok(None) => {}
                    Err(error) => {
                        view.notify_editor_error(format!("Could not load session: {error}"), cx)
                    }
                }
                cx.notify();
            }) {
                eprintln!("session load lost its document window: {error}");
            }
        })
        .detach();
    }

    pub(super) fn document_switcher_visible(&self) -> bool {
        self.workspace.mru_cycle_candidates().is_some()
    }

    pub(super) fn focus_document_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mru_focus.focus(window, cx);
    }

    pub(super) fn dismiss_document_switcher(&mut self, cx: &mut Context<Self>) {
        self.workspace.cancel_mru_cycle();
        cx.notify();
    }

    pub(super) fn render_document_switcher(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let entries = self.workspace.mru_cycle_candidates()?;
        let active = self
            .workspace
            .mru_cycle_selection()
            .unwrap_or((self.workspace.active_view(), self.workspace.active().id()));
        Some(
            div()
                .id("document-mru-switcher")
                .debug_selector(|| "document-mru-switcher".into())
                .track_focus(&self.mru_focus)
                .key_context("MatrixDocumentSwitcher")
                .role(Role::ListBox)
                .aria_label("Document switcher")
                .w_full()
                .h_full()
                .overflow_y_scroll()
                .border_1()
                .border_color(rgb(0x808080))
                .bg(rgb(if self.dark { 0x303030 } else { 0xf0f0f0 }))
                .p(px(4.))
                .on_action(cx.listener(|view, _: &MruNextBuffer, window, cx| {
                    view.cycle_mru_buffer(true, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &MruPreviousBuffer, window, cx| {
                    view.cycle_mru_buffer(false, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &CommitDocumentSwitcher, window, cx| {
                    view.finish_document_switcher(true, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &CancelDocumentSwitcher, window, cx| {
                    view.finish_document_switcher(false, window, cx);
                    cx.stop_propagation();
                }))
                .on_modifiers_changed(cx.listener(
                    |view, event: &gpui::ModifiersChangedEvent, window, cx| {
                        if !event.modifiers.control {
                            view.finish_document_switcher(true, window, cx);
                        }
                    },
                ))
                .children(entries.iter().filter_map(|(pane, id)| {
                    let buffer = self
                        .workspace
                        .buffers()
                        .iter()
                        .find(|buffer| buffer.id() == *id)?;
                    let (pane, id) = (*pane, *id);
                    Some(
                        div()
                            .id(format!("mru-entry-{}-{}", id.0, pane as usize))
                            .role(Role::ListBoxOption)
                            .aria_selected(active == (pane, id))
                            .px(px(6.))
                            .py(px(3.))
                            .when(active == (pane, id), |entry| {
                                entry.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                            })
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if view.route_to_document(window, cx, move |view, window, cx| {
                                    view.workspace.cancel_mru_cycle();
                                    if view.focus_editor_pane(pane, window, cx) {
                                        view.activate_buffer(id, window, cx);
                                    }
                                    cx.notify();
                                }) {
                                    return;
                                }
                                view.workspace.cancel_mru_cycle();
                                if view.focus_editor_pane(pane, window, cx) {
                                    view.activate_buffer(id, window, cx);
                                }
                                cx.notify();
                            }))
                            .child(format!(
                                "{}{}",
                                if buffer.is_modified() { "*" } else { "" },
                                buffer.name()
                            )),
                    )
                }))
                .into_any_element(),
        )
    }

    fn document_navigation_blocked(&self) -> bool {
        self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked()
    }

    pub(super) fn capture_current_document_state(&mut self) {
        let id = self.workspace.active().id();
        let folded_lines = self.editor_collapsed_headers();
        let state = DocumentViewState {
            top_line: self.editor_top_line,
            viewport: self.capture_editor_viewport(),
            horizontal_offset: f32::from(self.editor_horizontal_offset).max(0.),
            folded_lines,
            word_wrap: self.editor_word_wrap_enabled(),
        };
        if let Err(error) = self.workspace.set_view_state(self.pane, id, state) {
            eprintln!("could not capture document view state: {error}");
        }
    }

    pub(super) fn restore_current_document_state(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.workspace.active().id();
        let Some(state) = self.workspace.view_state(self.pane, id).cloned() else {
            return;
        };
        self.refresh_editor_syntax();
        self.capture_udl_text_system(window);
        self.capture_builtin_text_system(window);
        if self.editor_word_wrap_enabled() != state.word_wrap {
            self.toggle_editor_word_wrap(window, cx);
        }
        let headers = self
            .udl_fold_ranges()
            .into_iter()
            .map(|(header, _)| header)
            .collect::<Vec<_>>();
        for header in headers {
            if self.editor_fold_collapsed(header) != state.folded_lines.contains(&header) {
                self.toggle_editor_fold(header, window, cx);
            }
        }
        self.editor_top_line = state.top_line;
        self.restore_editor_viewport(state.viewport.clone());
        self.editor_horizontal_offset = px(state.horizontal_offset);
        self.editor_follow_caret = false;
        self.editor_painted_revision = None;
        if let Err(error) = self.workspace.set_view_state(self.pane, id, state) {
            self.notify_editor_error(
                format!("Could not restore document view state: {error}"),
                cx,
            );
        }
    }

    pub(super) fn capture_session_ui(&mut self) -> io::Result<()> {
        let focused = self.pane;
        for pane in [ViewId::Primary, ViewId::Secondary] {
            if self.workspace.active_id_in_view(pane).is_some() && self.scope_editor_pane(pane) {
                self.capture_current_document_state();
            }
        }
        self.scope_editor_pane(focused);
        let mut snapshot = self.workspace.session_ui().clone();
        if snapshot.get("tabs").is_none() {
            snapshot["tabs"] = serde_json::json!({});
        }
        snapshot["tabs"]["multiline"] = self.tab_multiline.into();
        snapshot["tabs"]["vertical"] = self.tab_vertical.into();
        snapshot["tabs"]["mru"] = self.mru_switching.into();
        snapshot["tabs"]["switcher"] = self.document_switcher.into();
        snapshot["split_ratio"] = serde_json::json!(self.split_ratio);
        snapshot["document_list_visible"] = serde_json::json!(self.document_list_visible);
        snapshot["document_map_visible"] = serde_json::json!(self.document_map.visible);
        snapshot["panels"] = self.capture_panel_state()?;
        self.workspace
            .set_session_ui(snapshot)
            .map_err(|error| io::Error::other(error.to_string()))
    }

    pub(super) fn restore_session_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.workspace.session_ui().clone();
        if let Some(value) = snapshot["tabs"]["multiline"].as_bool() {
            self.tab_multiline = value;
        }
        if let Some(value) = snapshot["tabs"]["vertical"].as_bool() {
            self.tab_vertical = value;
        }
        if let Some(value) = snapshot["tabs"]["mru"].as_bool() {
            self.mru_switching = value;
        }
        if let Some(value) = snapshot["tabs"]["switcher"].as_bool() {
            self.document_switcher = value;
        }
        if let Some(value) = snapshot["split_ratio"]
            .as_f64()
            .filter(|value| value.is_finite() && (0.1..=0.9).contains(value))
        {
            self.split_ratio = value as f32;
        }
        if let Some(value) = snapshot["document_list_visible"].as_bool() {
            self.document_list_visible = value;
        }
        if let Some(value) = snapshot["document_map_visible"].as_bool() {
            self.document_map.visible = value;
        }
        if let Some(panels) = snapshot.get("panels")
            && let Err(error) = self.restore_panel_state(panels, window, cx)
        {
            self.notify_editor_error(format!("Could not restore panel layout: {error}"), cx);
        }
        self.reconcile_editor_panes();
        let focused = self.pane;
        for pane in [ViewId::Primary, ViewId::Secondary] {
            if self.workspace.active_id_in_view(pane).is_some() && self.scope_editor_pane(pane) {
                self.prepare_editor_document();
                self.restore_current_document_state(window, cx);
            }
        }
        self.scope_editor_pane(focused);
    }

    pub(super) fn cycle_mru_buffer(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.cycle_mru_buffer(forward, window, cx)
        }) {
            return;
        }
        if self.document_navigation_blocked() || !self.cancel_view_composition(window, cx) {
            return;
        }
        if !self.document_switcher {
            match self.workspace.cycle_documents(forward, false) {
                Ok(_) => self.finish_document_switcher(true, window, cx),
                Err(error) => self.notify_editor_error(error.to_string(), cx),
            }
            return;
        }
        if self.workspace.buffers().len() == 1 && !self.workspace.two_views_visible() {
            return;
        }
        match self.workspace.cycle_documents(forward, self.mru_switching) {
            Ok(_) => {
                self.present_floating(floating_windows::DialogKind::DocumentSwitcher, window, cx);
                cx.notify();
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }

    pub(super) fn finish_document_switcher(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.finish_document_switcher(commit, window, cx)
        }) {
            return;
        }
        if self.workspace.mru_cycle_candidates().is_none() {
            return;
        }
        if commit {
            match self.workspace.finish_mru_cycle() {
                Ok(Some(_)) => self.reset_active_view(window, cx),
                Ok(None) => {}
                Err(error) => self.notify_editor_error(error.to_string(), cx),
            }
        } else {
            self.workspace.cancel_mru_cycle();
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn restore_last_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_navigation_blocked() || !self.cancel_view_composition(window, cx) {
            return;
        }
        match self.workspace.reopen_last_closed() {
            Ok(Some(_)) => self.reset_active_view(window, cx),
            Ok(None) => {}
            Err(error) => {
                self.notify_editor_error(format!("Could not restore closed file: {error}"), cx)
            }
        }
    }

    pub(super) fn toggle_pin_tab(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.document_navigation_blocked() {
            return;
        }
        let id = self.workspace.active().id();
        let pinned = !self.workspace.active().is_pinned();
        match self.workspace.set_pinned(id, pinned) {
            Ok(()) => {
                self.tab_scroll
                    .scroll_to_item(self.workspace.active_index());
                self.synchronize_recovery(cx);
                cx.notify();
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }

    pub(super) fn set_active_tab_color(&mut self, color: Option<u8>, cx: &mut Context<Self>) {
        if self.document_navigation_blocked() {
            return;
        }
        let id = self.workspace.active().id();
        match self.workspace.set_tab_color(self.pane, id, color) {
            Ok(()) => {
                self.synchronize_recovery(cx);
                cx.notify();
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }

    pub(super) fn sort_document_tabs(
        &mut self,
        sort: TabSort,
        descending: bool,
        cx: &mut Context<Self>,
    ) {
        if self.document_navigation_blocked() {
            return;
        }
        self.workspace.sort_tabs(sort, descending);
        self.tab_scroll
            .scroll_to_item(self.workspace.active_index());
        self.synchronize_recovery(cx);
        cx.notify();
    }

    fn persist_tab_layout(&mut self, cx: &mut Context<Self>) {
        if let Some(store) = &self.settings_store {
            let result = store.update_settings(|settings| {
                settings["tab_multiline"] = self.tab_multiline.into();
                settings["tab_vertical"] = self.tab_vertical.into();
                settings["mru_switching"] = self.mru_switching.into();
                settings["document_switcher"] = self.document_switcher.into();
                Ok(())
            });
            if let Err(error) = result {
                self.notify_editor_error(format!("Could not save tab settings: {error}"), cx);
            }
        }
        self.synchronize_recovery(cx);
        cx.notify();
    }

    pub(super) fn toggle_tab_multiline(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab_multiline = !self.tab_multiline;
        self.persist_tab_layout(cx);
    }

    pub(super) fn toggle_document_mru(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_document_switcher(false, window, cx);
        self.mru_switching = !self.mru_switching;
        self.persist_tab_layout(cx);
    }

    pub(super) fn toggle_document_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_document_switcher(false, window, cx);
        self.document_switcher = !self.document_switcher;
        self.persist_tab_layout(cx);
    }

    pub(super) fn toggle_tab_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.tab_vertical = !self.tab_vertical;
        self.persist_tab_layout(cx);
    }
}
