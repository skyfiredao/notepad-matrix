use super::*;
use crate::files::{DiskFile, OverwritePolicy};

use crate::workspace::closing as closing_model;
pub(super) use closing_model::CloseBatch as ConditionalClose;

#[path = "file_actions_tests.rs"]
#[cfg(all(test, feature = "native-input-tests"))]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveOperation {
    Current,
    As,
    Copy,
}

enum CloseSide {
    Left,
    Right,
}

fn resolve_save_target(path: &std::path::Path) -> io::Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "save target has no filename")
            })?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."));
            Ok(std::fs::canonicalize(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}

impl MatrixWindow {
    pub(super) fn request_close_condition(
        &mut self,
        condition: crate::workspace::CloseCondition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.close_tabs_queue.clear();
        self.close_discarded.clear();
        self.close_target = None;
        self.close_after_save = false;
        self.conditional_close = Some(
            if condition == crate::workspace::CloseCondition::ExceptCurrent {
                ConditionalClose::except_current(&self.workspace)
            } else {
                ConditionalClose::selected(self.workspace.close_targets(condition))
            },
        );
        self.advance_conditional_close(window, cx);
    }

    fn activate_file_buffer_in_view(
        &mut self,
        view: crate::workspace::ViewId,
        id: crate::workspace::BufferId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace.active_view() == view && self.workspace.active().id() == id {
            return true;
        }
        if !self.cancel_view_composition(window, cx) {
            return false;
        }
        match self.workspace.activate_in_view(view, id) {
            Ok(()) => {
                self.reset_active_view(window, cx);
                true
            }
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                false
            }
        }
    }

    pub(super) fn request_close_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_conditional_close(CloseSide::Left, window, cx);
    }

    pub(super) fn request_close_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_conditional_close(CloseSide::Right, window, cx);
    }

    fn start_conditional_close(
        &mut self,
        side: CloseSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.macro_interaction_blocked() {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.save_all_restore.is_some()
            || self.reload_target.is_some()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.close_tabs_queue.clear();
        self.close_discarded.clear();
        self.close_target = None;
        self.close_after_save = false;
        self.conditional_close = Some(ConditionalClose::conditional(
            self.workspace.active_view(),
            match side {
                CloseSide::Left => self.workspace.close_left_targets(),
                CloseSide::Right => self.workspace.close_right_targets(),
            },
        ));
        self.advance_conditional_close(window, cx);
    }

    fn advance_conditional_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(id) = self
            .conditional_close
            .as_mut()
            .and_then(|batch| batch.next())
        {
            if !self
                .workspace
                .buffers()
                .iter()
                .any(|buffer| buffer.id() == id)
            {
                self.notify_editor_error(format!("buffer {} is no longer open", id.0), cx);
                self.finish_close_batch(true, window, cx);
                return;
            }
            let automatic = self
                .conditional_close
                .as_ref()
                .and_then(|batch| batch.automatic_disposition(&self.workspace, id));
            if let Some(disposition) = automatic {
                if let Some(batch) = &mut self.conditional_close {
                    batch.approve_automatic(id, disposition);
                }
            } else {
                let view = self
                    .conditional_close
                    .as_ref()
                    .map(|batch| batch.prompt_view(id))
                    .unwrap_or(self.workspace.active_view());
                if !self.activate_file_buffer_in_view(view, id, window, cx) {
                    self.finish_close_batch(true, window, cx);
                    return;
                }
                self.close_target = Some(id);
                self.close_choice = 0;
                self.close_dialog = true;
                self.close_focus.focus(window, cx);
                cx.notify();
                return;
            }
        }
        self.finish_conditional_close(window, cx);
    }

    fn finish_conditional_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_close_batch(false, window, cx);
    }

    fn finish_close_batch(&mut self, cancelled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(batch) = self.conditional_close.take() else {
            return;
        };
        self.close_target = None;
        self.close_dialog = false;
        self.close_after_save = false;
        self.close_tabs_queue.clear();
        self.close_discarded.clear();
        if self.file_busy
            || self.save_all_dialog
            || self.save_all_restore.is_some()
            || self.reload_target.is_some()
        {
            self.notify_editor_error("File closing stopped by another file operation".into(), cx);
            return;
        }
        for target in batch.finish(cancelled, self.workspace.active_view()) {
            let active = self.workspace.active().id();
            if !self.cancel_view_composition(window, cx) {
                break;
            }
            let previous_view = self.workspace.active_view();
            match self
                .workspace
                .close_in_view(target.view, target.id, target.disposition)
            {
                Ok(_) => {
                    if self.workspace.active().id() != active
                        || self.workspace.active_view() != previous_view
                    {
                        self.reset_active_view(window, cx);
                    }
                }
                Err(error) => {
                    self.notify_editor_error(error.to_string(), cx);
                    break;
                }
            }
        }
        self.tab_scroll
            .scroll_to_item(self.workspace.active_index());
        self.editor_focus.focus(window, cx);
        self.synchronize_recovery(cx);
        cx.notify();
    }
    pub(super) fn request_native_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(error) = self
            .native_quit
            .as_ref()
            .and_then(|control| control.take_error())
        {
            self.cancel_native_quit();
            self.notify_editor_error(format!("Native Quit refused: {error}"), cx);
            return;
        }
        if self.application_quit_pending {
            return;
        }
        if self.file_busy
            || self.reload_target.is_some()
            || self.save_all_restore.is_some()
            || self.save_all_dialog
            || self.close_dialog
        {
            self.cancel_native_quit();
            self.notify_editor_error(
                "Finish or cancel the current file operation before quitting".into(),
                cx,
            );
            return;
        }
        self.application_quit_pending = true;
        self.request_close(window, cx);
    }

    fn cancel_native_quit(&mut self) {
        self.application_quit_pending = false;
        if let Some(control) = &self.native_quit {
            control.cancel();
        }
    }

    pub(super) fn file_dialog_bindings() -> [KeyBinding; 17] {
        [
            KeyBinding::new("enter", FileDialogActivate, Some("MatrixClose")),
            KeyBinding::new("escape", CancelClose, Some("MatrixClose")),
            KeyBinding::new("alt-y", SaveAndClose, Some("MatrixClose")),
            KeyBinding::new("alt-n", DiscardClose, Some("MatrixClose")),
            KeyBinding::new("alt-c", CancelClose, Some("MatrixClose")),
            KeyBinding::new("tab", FileDialogNext, Some("MatrixClose")),
            KeyBinding::new("shift-tab", FileDialogPrevious, Some("MatrixClose")),
            KeyBinding::new("right", FileDialogNext, Some("MatrixClose")),
            KeyBinding::new("left", FileDialogPrevious, Some("MatrixClose")),
            KeyBinding::new("enter", FileDialogActivate, Some("MatrixSaveAll")),
            KeyBinding::new("escape", CancelSaveAll, Some("MatrixSaveAll")),
            KeyBinding::new("alt-y", ConfirmSaveAll, Some("MatrixSaveAll")),
            KeyBinding::new("alt-n", CancelSaveAll, Some("MatrixSaveAll")),
            KeyBinding::new("tab", FileDialogNext, Some("MatrixSaveAll")),
            KeyBinding::new("shift-tab", FileDialogPrevious, Some("MatrixSaveAll")),
            KeyBinding::new("right", FileDialogNext, Some("MatrixSaveAll")),
            KeyBinding::new("left", FileDialogPrevious, Some("MatrixSaveAll")),
        ]
    }

    pub(super) fn request_close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() {
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Unsupported("file closing"));
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.save_all_restore.is_some()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.close_tabs_queue.clear();
        self.close_discarded.clear();
        self.close_target = None;
        self.close_after_save = false;
        self.conditional_close = Some(ConditionalClose::all(&self.workspace));
        self.advance_conditional_close(window, cx);
    }

    pub(super) fn request_close_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() {
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Unsupported("file closing"));
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.save_all_restore.is_some()
        {
            return;
        }
        let id = self.workspace.active().id();
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let automatic = closing_model::individual_close_disposition(
            &self.workspace,
            self.workspace.active_view(),
            id,
        );
        if let Some(disposition) = automatic {
            self.close_specific_buffer(id, disposition, window, cx);
        } else {
            self.close_target = Some(id);
            self.close_choice = 0;
            self.close_dialog = true;
            self.close_focus.focus(window, cx);
            cx.notify();
        }
    }

    fn close_specific_buffer(
        &mut self,
        id: crate::workspace::BufferId,
        disposition: crate::workspace::CloseDisposition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        match self.workspace.close_buffer(id, disposition) {
            Ok(_) => {
                self.reset_active_view(window, cx);
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }
    pub(super) fn start_save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() {
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Unsupported("Save All"));
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.save_all_restore.is_some()
        {
            return;
        }
        let dirty = self.workspace.dirty_buffers();
        if dirty.is_empty() {
            return;
        }
        self.save_all_restore = Some(self.workspace.active().id());
        self.save_all_restore_view = Some(self.workspace.active_view());
        self.save_all_errors.clear();
        self.save_all_queue = dirty.into();
        if self.save_all_confirm
            && (self.save_all_queue.len() != 1
                || self.save_all_queue.front().copied() != Some(self.workspace.active().id()))
        {
            self.save_all_dialog = true;
            self.save_all_choice_yes = true;
            self.close_focus.focus(window, cx);
            cx.notify();
        } else {
            self.save_next_buffer(window, cx);
        }
    }

    fn save_next_buffer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.save_all_dialog || self.save_all_restore.is_none() {
            return;
        }
        while let Some(id) = self.save_all_queue.pop_front() {
            if !self.activate_buffer(id, window, cx) || self.workspace.active().id() != id {
                self.save_all_errors.push(
                    self.editor_error.clone().unwrap_or_else(|| {
                        format!("Could not activate buffer {} for saving", id.0)
                    }),
                );
                continue;
            }
            self.start_save(false, window, cx);
            return;
        }
        self.finish_save_all(window, cx);
    }

    fn finish_save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_all_queue.clear();
        self.save_all_dialog = false;
        let error = if self.save_all_errors.is_empty() {
            self.editor_error.clone()
        } else {
            Some(format!(
                "Save All completed with errors: {}",
                std::mem::take(&mut self.save_all_errors).join("; ")
            ))
        };
        let restore_view = self.save_all_restore_view.take();
        if let Some(id) = self.save_all_restore.take() {
            if let Some(view) = restore_view {
                self.activate_file_buffer_in_view(view, id, window, cx);
            } else {
                self.activate_buffer(id, window, cx);
            }
        }
        if let Some(error) = error {
            self.notify_editor_error(error, cx);
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn confirm_save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.confirm_save_all(window, cx)
        }) {
            return;
        }
        if !self.save_all_dialog || self.file_busy {
            return;
        }
        self.save_all_dialog = false;
        self.save_next_buffer(window, cx);
    }

    pub(super) fn cancel_save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.cancel_save_all(window, cx)
        }) {
            return;
        }
        if !self.save_all_dialog || self.file_busy {
            return;
        }
        self.finish_save_all(window, cx);
    }

    pub(super) fn render_save_all_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.save_all_dialog {
            return None;
        }
        let button = |id: &'static str, label: &'static str, selected: bool| {
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
                .id("save-all-overlay")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .child(
                    div()
                        .id("save-all-dialog")
                        .role(Role::Dialog)
                        .aria_label("Save all?")
                        .track_focus(&self.close_focus)
                        .key_context("MatrixSaveAll")
                        .on_action(cx.listener(|view, _: &FileDialogNext, _, cx| {
                            if view.save_all_dialog && !view.file_busy {
                                view.save_all_choice_yes = !view.save_all_choice_yes;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &FileDialogPrevious, _, cx| {
                            if view.save_all_dialog && !view.file_busy {
                                view.save_all_choice_yes = !view.save_all_choice_yes;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &FileDialogActivate, window, cx| {
                            if view.save_all_choice_yes {
                                view.confirm_save_all(window, cx);
                            } else {
                                view.cancel_save_all(window, cx);
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &ConfirmSaveAll, window, cx| {
                            view.confirm_save_all(window, cx);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &CancelSaveAll, window, cx| {
                            view.cancel_save_all(window, cx);
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
                        .child("Save all modified documents?")
                        .child(
                            div()
                                .flex()
                                .gap(px(12.))
                                .child(
                                    button("save-all-yes", "Yes", self.save_all_choice_yes)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.confirm_save_all(window, cx);
                                        })),
                                )
                                .child(
                                    button("save-all-no", "No", !self.save_all_choice_yes)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.cancel_save_all(window, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn start_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() {
            return;
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy || self.close_dialog || self.save_all_dialog {
            return;
        }
        self.file_busy = true;
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let executor = cx.background_executor().clone();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let selected = match prompt.await {
                Ok(Ok(paths)) => Ok(paths),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(format!(
                    "open path chooser disconnected: {error}"
                ))),
            };
            let loaded = match selected {
                Ok(Some(paths)) => {
                    executor
                        .spawn(async move {
                            paths
                                .into_iter()
                                .map(|path| {
                                    let result = DiskFile::load_unicode(&path);
                                    (path, result)
                                })
                                .collect::<Vec<_>>()
                        })
                        .await
                }
                Ok(None) => Vec::new(),
                Err(error) => vec![(PathBuf::new(), Err(error))],
            };
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                let mut opened = false;
                let mut errors = Vec::new();
                for (path, result) in loaded {
                    match result {
                        Ok((file, text)) => {
                            if !view.cancel_view_composition(window, cx) {
                                return;
                            }
                            match view.workspace.open_buffer(file, &text) {
                                Ok(_) => opened = true,
                                Err(error) => errors.push(error.to_string()),
                            }
                        }
                        Err(error) => errors.push(format!("{}: {error}", path.display())),
                    }
                }
                if opened {
                    view.reset_active_view(window, cx);
                } else {
                    view.editor_focus.focus(window, cx);
                    cx.notify();
                }
                if !errors.is_empty() {
                    view.notify_editor_error(
                        format!("Open failed (supported Unicode): {}", errors.join("; ")),
                        cx,
                    );
                }
            }) {
                eprintln!("open completion could not reach its window: {error}");
            }
        })
        .detach();
    }

    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() {
            self.cancel_macro_repeat(window, cx);
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy || self.save_all_dialog || self.close_dialog {
            return;
        }
        if !self.prepare_udl_close(window, cx) {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        // Source snapshot-mode window exit preserves the session and backups.
        // Explicit Close Document continues through the guarded tab-close path.
        if self
            .recovery
            .as_ref()
            .is_some_and(crate::recovery::RecoveryManager::snapshot_mode)
        {
            if self.synchronize_recovery(cx) {
                self.save_search_history_on_close(cx);
                window.remove_window();
            } else {
                self.cancel_native_quit();
            }
            return;
        }
        self.close_target = None;
        let pending = self
            .workspace
            .dirty_buffers()
            .into_iter()
            .find(|id| !self.close_discarded.contains(id));
        if let Some(id) = pending {
            if !self.activate_buffer(id, window, cx) {
                return;
            }
            self.tab_scroll
                .scroll_to_item(self.workspace.active_index());
            self.editor_lines.clear();
            self.editor_painted_revision = None;
            self.close_dialog = true;
            self.close_choice = 0;
            self.close_focus.focus(window, cx);
            cx.notify();
        } else {
            if self.retire_recovery_on_confirmed_close(cx) {
                self.save_search_history_on_close(cx);
                window.remove_window();
            } else {
                self.cancel_native_quit();
            }
        }
    }

    pub(super) fn render_close_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.close_dialog {
            return None;
        }
        let button = |id: &'static str, label: &'static str, selected: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(label)
                .aria_selected(selected)
                .aria_description(if self.file_busy {
                    "Unavailable while saving"
                } else {
                    ""
                })
                .border_1()
                .border_color(rgb(if selected { 0x0078d4 } else { 0x808080 }))
                .bg(rgb(if selected { 0xdceeff } else { 0xe8e8e8 }))
                .text_color(rgb(if self.file_busy { 0x808080 } else { 0x202020 }))
                .px(px(16.))
                .py(px(5.))
                .child(label)
        };
        Some(
            div()
                .id("close-overlay")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .child(
                    div()
                        .id("close-dialog")
                        .role(Role::Dialog)
                        .aria_label("Save changes?")
                        .track_focus(&self.close_focus)
                        .key_context("MatrixClose")
                        .on_action(cx.listener(|view, _: &FileDialogNext, _, cx| {
                            if view.close_dialog && !view.file_busy {
                                view.close_choice = (view.close_choice + 1) % 3;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &FileDialogPrevious, _, cx| {
                            if view.close_dialog && !view.file_busy {
                                view.close_choice = (view.close_choice + 2) % 3;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &FileDialogActivate, window, cx| {
                            if view.close_dialog && !view.file_busy {
                                match view.close_choice {
                                    0 => view.save_and_close(window, cx),
                                    1 => view.discard_close(window, cx),
                                    _ => view.cancel_close(window, cx),
                                }
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &CancelClose, window, cx| {
                            view.cancel_close(window, cx);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &DiscardClose, window, cx| {
                            view.discard_close(window, cx);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &SaveAndClose, window, cx| {
                            view.save_and_close(window, cx);
                            cx.stop_propagation();
                        }))
                        .w_full()
                        .p(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .border_1()
                        .border_color(rgb(0x808080))
                        .bg(rgb(0xf0f0f0))
                        .text_color(rgb(0x202020))
                        .child("Save changes before closing?")
                        .when_some(self.editor_error.clone(), |dialog, error| {
                            dialog.child(
                                div()
                                    .id("close-save-error")
                                    .role(Role::Alert)
                                    .aria_label(error.clone())
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(
                                    button("close-save", "Yes", self.close_choice == 0).on_click(
                                        cx.listener(|view, _, window, cx| {
                                            view.save_and_close(window, cx)
                                        }),
                                    ),
                                )
                                .child(
                                    button("close-discard", "No", self.close_choice == 1).on_click(
                                        cx.listener(|view, _, window, cx| {
                                            view.discard_close(window, cx)
                                        }),
                                    ),
                                )
                                .child(
                                    button("close-cancel", "Cancel", self.close_choice == 2)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.cancel_close(window, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn discard_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.discard_close(window, cx)
        }) {
            return;
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy {
            return;
        }
        self.close_dialog = false;
        if self.conditional_close.is_some() {
            if let Some(id) = self.close_target.take() {
                if id != self.workspace.active().id() {
                    self.notify_editor_error("Conditional close target changed".into(), cx);
                    self.finish_close_batch(true, window, cx);
                    return;
                }
                if let Some(batch) = &mut self.conditional_close {
                    batch.approve(id, crate::workspace::CloseDisposition::DiscardConfirmed);
                }
                self.advance_conditional_close(window, cx);
            } else {
                self.finish_close_batch(true, window, cx);
            }
            return;
        }
        if let Some(id) = self.close_target.take() {
            self.close_specific_buffer(
                id,
                crate::workspace::CloseDisposition::DiscardConfirmed,
                window,
                cx,
            );
            return;
        }
        self.close_discarded.insert(self.workspace.active().id());
        self.request_close(window, cx);
    }

    pub(super) fn cancel_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.cancel_close(window, cx)) {
            return;
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy {
            return;
        }
        self.close_dialog = false;
        self.cancel_native_quit();
        if self.conditional_close.is_some() {
            self.finish_close_batch(true, window, cx);
            return;
        }
        self.close_after_save = false;
        self.close_target = None;
        self.close_tabs_queue.clear();
        self.close_discarded.clear();
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn save_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.save_and_close(window, cx)
        }) {
            return;
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy {
            return;
        }
        self.close_after_save = true;
        self.start_save(false, window, cx);
    }

    pub(super) fn start_save(
        &mut self,
        save_as: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_save_operation(
            if save_as {
                SaveOperation::As
            } else {
                SaveOperation::Current
            },
            window,
            cx,
        );
    }

    pub(super) fn start_save_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_dialog || self.save_all_dialog {
            return;
        }
        self.start_save_operation(SaveOperation::Copy, window, cx);
    }

    fn start_save_operation(
        &mut self,
        operation: SaveOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.macro_interaction_blocked() {
            return;
        }
        if matches!(operation, SaveOperation::Current) {
            self.record_macro_step(crate::macros::MacroStep::Unsupported("file saving"));
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy {
            return;
        }
        if operation == SaveOperation::Current && self.editor().document().is_read_only() {
            self.notify_editor_error(
                "The file is read-only and cannot be saved. Remove read-only before saving.".into(),
                cx,
            );
            return;
        }
        let backup_options = match self
            .settings_store
            .as_ref()
            .map(ConfigStore::load_settings)
            .transpose()
            .and_then(|settings| {
                crate::recovery::SaveBackupOptions::from_settings(
                    &settings.unwrap_or_else(|| serde_json::json!({})),
                )
            }) {
            Ok(options) => options,
            Err(error) => {
                self.notify_editor_error(
                    format!("Could not load save-backup settings: {error}"),
                    cx,
                );
                return;
            }
        };
        let backup_source = if operation == SaveOperation::Current
            && self.workspace.active().is_modified()
            && !self.workspace.active().is_large_file()
            && !self.editor().document().is_read_only()
        {
            self.disk_file().map(|file| file.path().to_owned())
        } else {
            None
        };
        let backup_timestamp = {
            let formatter = objc2_foundation::NSDateFormatter::new();
            formatter.setDateFormat(Some(&objc2_foundation::NSString::from_str(
                "yyyy-MM-dd_HHmmss",
            )));
            formatter
                .stringFromDate(&objc2_foundation::NSDate::date())
                .to_string()
        };
        let intent = self.workspace.active().save_intent();
        let original = self.disk_file().cloned();
        let active_id = intent.buffer_id();
        let other_paths = self
            .workspace
            .buffers()
            .iter()
            .filter(|buffer| buffer.id() != active_id)
            .filter_map(|buffer| {
                buffer
                    .disk_file
                    .as_ref()
                    .map(|file| (buffer.id(), file.path().to_path_buf()))
            })
            .collect::<Vec<_>>();
        let format = intent.format();
        let needs_path = operation != SaveOperation::Current || original.is_none();
        let directory = original
            .as_ref()
            .and_then(|file| file.path().parent())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = original
            .as_ref()
            .and_then(|file| file.path().file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.txt", self.workspace.active().name()));
        let prompt = if needs_path {
            Some(cx.prompt_for_new_path(&directory, Some(&suggested)))
        } else {
            None
        };
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let selected = if let Some(prompt) = prompt {
                match prompt.await {
                    Ok(Ok(path)) => Ok(path),
                    Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                    Err(error) => Err(io::Error::other(format!(
                        "save path chooser disconnected: {error}"
                    ))),
                }
            } else {
                Ok(original.as_ref().map(|file| file.path().to_path_buf()))
            };
            let selected = if selected.as_ref().is_ok_and(Option::is_some) {
                if let Some(source) = backup_source {
                    match executor.spawn(async move { backup_options.create_backup(&source, &backup_timestamp) }).await {
                        Ok(_) => selected,
                        Err(error) => {
                            let detail = format!("The previous version could not be saved in the backup directory: {error}");
                            match view.update_in(cx, |view, window, cx| view.request_backup_failure(detail,window,cx)) {
                                Ok(Ok(answer)) => if matches!(executor.spawn(async move { answer.recv() }).await, Ok(true)) { selected } else { Err(io::Error::other(format!("Save canceled after backup failed: {error}"))) },
                                Ok(Err(prompt_error)) => Err(prompt_error),
                                Err(prompt_error) => Err(io::Error::other(format!("Save-backup confirmation failed: {prompt_error}"))),
                            }
                        }
                    }
                } else { selected }
            } else { selected };
            let mut collision = None;
            let result = match selected {
                Ok(Some(path)) => {
                    let writing = intent.snapshot().clone();
                    let (other, result) = executor
                        .spawn(async move {
                            if needs_path {
                                let resolved = match resolve_save_target(&path) {
                                    Ok(path) => path,
                                    Err(error) => return (None, Err(error)),
                                };
                                if let Some((id, _)) =
                                    other_paths.iter().find(|(_, opened)| *opened == resolved)
                                {
                                    return (
                                        Some(*id),
                                        Err(io::Error::other(
                                            "the target file is already open in another buffer",
                                        )),
                                    );
                                }
                            }
                            let result = if needs_path {
                                // The native save chooser confirms replacement before returning an existing path.
                                DiskFile::save_as_unicode(
                                    &path,
                                    &writing,
                                    format,
                                    OverwritePolicy::Confirmed,
                                )
                            } else if let Some(file) = original {
                                file.save_with_format(&writing, format)
                            } else {
                                Err(io::Error::other("save target disappeared"))
                            };
                            (None, result)
                        })
                        .await;
                    collision = other;
                    result.map(Some)
                }
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                let closing = view.close_after_save;
                view.close_after_save = false;
                if let Some(other) = collision {
                    if view.conditional_close.is_some() {
                        view.finish_close_batch(true, window, cx);
                    }
                    if view.save_all_restore.is_some() {
                        view.save_all_errors.push(
                            "Save refused: the target file is already open in another buffer"
                                .into(),
                        );
                        view.save_next_buffer(window, cx);
                        return;
                    }
                    view.save_all_queue.clear();
                    view.save_all_restore = None;
                    view.save_all_restore_view = None;
                    view.save_all_dialog = false;
                    view.close_dialog = false;
                    view.close_target = None;
                    view.close_tabs_queue.clear();
                    view.close_discarded.clear();
                    view.cancel_native_quit();
                    if view
                        .workspace
                        .buffers()
                        .iter()
                        .any(|buffer| buffer.id() == other)
                    {
                        view.activate_buffer(other, window, cx);
                    }
                    view.notify_editor_error(
                        "Save refused: the target file is already open in another buffer".into(),
                        cx,
                    );
                    return;
                }
                let mut committed = false;
                let failure_message = result
                    .as_ref()
                    .err()
                    .map(|error| format!("{}: {error}", view.workspace.active().name()));
                match result {
                    Ok(Some(file)) => {
                        let acknowledged = view
                            .workspace
                            .complete_save(
                                &intent,
                                file,
                                if operation == SaveOperation::Copy {
                                    crate::workspace::SaveCompletion::Copy
                                } else {
                                    crate::workspace::SaveCompletion::Primary
                                },
                            )
                            .map_err(|error| crate::editor::EditError::Storage(error.to_string()));
                        if operation == SaveOperation::Copy {
                            view.editor_changed(acknowledged, window, cx);
                            view.editor_focus.focus(window, cx);
                            return;
                        }
                        if acknowledged.is_ok() {
                            committed = true;
                        }
                        view.editor_changed(acknowledged, window, cx);
                        if committed {
                            view.synchronize_recovery(cx);
                        }
                    }
                    Ok(None) => cx.notify(),
                    Err(error) => view.notify_editor_error(format!("Save failed: {error}"), cx),
                }
                if closing && view.conditional_close.is_some() {
                    view.close_dialog = false;
                    let id = intent.buffer_id();
                    let approved = committed
                        && view.close_target == Some(id)
                        && view
                            .workspace
                            .buffers()
                            .iter()
                            .any(|buffer| buffer.id() == id && !buffer.is_modified());
                    if approved {
                        view.close_target = None;
                        if let Some(batch) = &mut view.conditional_close {
                            batch.approve(id, crate::workspace::CloseDisposition::IfUnmodified);
                        }
                        view.advance_conditional_close(window, cx);
                    } else {
                        view.finish_close_batch(true, window, cx);
                    }
                    return;
                }
                if view.save_all_restore.is_some() {
                    if let Some(error) = failure_message {
                        view.save_all_errors.push(error);
                    }
                    view.save_next_buffer(window, cx);
                    return;
                }
                if closing && committed && !view.workspace.active().is_modified() {
                    view.close_dialog = false;
                    if let Some(id) = view.close_target.take() {
                        view.close_specific_buffer(
                            id,
                            crate::workspace::CloseDisposition::IfUnmodified,
                            window,
                            cx,
                        );
                        return;
                    }
                    view.request_close(window, cx);
                } else if closing {
                    view.close_dialog = true;
                    view.close_focus.focus(window, cx);
                } else {
                    view.editor_focus.focus(window, cx);
                }
            }) {
                eprintln!("save completion could not reach its window: {error}");
            }
        })
        .detach();
    }
}
