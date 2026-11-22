use super::*;
use crate::files::{DiskFile, OverwritePolicy};
use gpui::Pixels;

use crate::workspace::closing as closing_model;
pub(super) use closing_model::CloseBatch as ConditionalClose;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CloseAllAnswer {
    Save,
    Discard,
}

pub(super) struct FilePromptState {
    current_directory: Option<PathBuf>,
    pub(super) recent_in_submenu: bool,
    pub(super) recent_custom_length: i64,
    pub(super) startup_settings: Option<serde_json::Value>,
    open_sequence_pending: bool,
}

impl Default for FilePromptState {
    fn default() -> Self {
        Self { current_directory: None, recent_in_submenu: false, recent_custom_length: -1, startup_settings: None, open_sequence_pending: false }
    }
}

enum OpenPathResult {
    Text(io::Result<(DiskFile, crate::byte_buffer::ByteBuffer)>),
    Session(io::Result<crate::recovery::SessionFile>),
    Workspace,
}

struct OpenLaunchCompletion(Option<crate::launch::LaunchOptions>);

impl Drop for OpenLaunchCompletion {
    fn drop(&mut self) {
        if let Some(options) = &self.0 && let Err(error) = options.complete(false) {
            eprintln!("file open launch completion failed: {error}");
        }
    }
}

fn matches_configured_extension(path: &std::path::Path, extension: &str) -> bool {
    if extension.is_empty() { return false; }
    let configured = if extension.starts_with('.') { extension.to_owned() } else { format!(".{extension}") };
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else { return false; };
    let Some(dot) = filename.rfind('.') else { return false; };
    filename[dot..].to_lowercase() == configured.to_lowercase()
}

fn expanded_directory(value: &str) -> PathBuf {
    let mut expanded = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find('%') {
        expanded.push_str(&remaining[..start]);
        let rest = &remaining[start + 1..];
        let Some(end) = rest.find('%') else { expanded.push_str(&remaining[start..]); return PathBuf::from(expanded); };
        match std::env::var(&rest[..end]) {
            Ok(value) => expanded.push_str(&value),
            Err(_) => expanded.push_str(&remaining[start..start + end + 2]),
        }
        remaining = &rest[end + 1..];
    }
    expanded.push_str(remaining);
    PathBuf::from(expanded)
}

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

pub(super) fn resolve_save_target(path: &std::path::Path) -> io::Result<PathBuf> {
    let _access = crate::file_access::directory(path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new(".")))?;
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
            || self.file_open_sequence_pending()
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
        self.close_all_answer = None;
        self.close_multi = self.conditional_close.as_ref().is_some_and(|batch| batch.dirty_count(&self.workspace) > 1);
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
            || self.file_open_sequence_pending()
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
        self.close_all_answer = None;
        self.close_multi = self.conditional_close.as_ref().is_some_and(|batch| batch.dirty_count(&self.workspace) > 1);
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
            } else if self.close_all_answer == Some(CloseAllAnswer::Discard) {
                if let Some(batch) = &mut self.conditional_close {
                    batch.approve(id, crate::workspace::CloseDisposition::DiscardConfirmed);
                }
            } else {
                if self.conditional_close.as_ref().is_some_and(ConditionalClose::uses_global_prompt_count) {
                    self.close_multi = self.workspace.dirty_buffers().len() > 1;
                }
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
                self.close_dialog = self.close_all_answer != Some(CloseAllAnswer::Save);
                if self.close_all_answer == Some(CloseAllAnswer::Save) {
                    self.save_and_close(window, cx);
                } else {
                    self.close_focus.focus(window, cx);
                    cx.notify();
                }
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
        self.close_all_answer = None;
        self.close_multi = false;
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
                    if self.quit_after_last_document_close(target.id, window, cx) { return; }
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
        self.scroll_active_tab_into_view();
        self.editor_focus.focus(window, cx);
        self.synchronize_recovery(cx);
        cx.notify();
    }
    pub(super) fn request_native_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() {
            self.cancel_native_quit();
            return;
        }
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
            || self.file_open_sequence_pending()
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
        self.close_all_answer = None;
        self.application_quit_pending = false;
        if let Some(control) = &self.native_quit {
            control.cancel();
        }
    }

    pub(super) fn file_dialog_bindings() -> [KeyBinding; 20] {
        [
            KeyBinding::new("enter", FileDialogActivate, Some("MatrixClose")),
            KeyBinding::new("escape", CancelClose, Some("MatrixClose")),
            KeyBinding::new("alt-y", SaveAndClose, Some("MatrixClose")),
            KeyBinding::new("alt-n", DiscardClose, Some("MatrixClose")),
            KeyBinding::new("alt-c", CancelClose, Some("MatrixClose")),
            KeyBinding::new("alt-a", SaveAllAndClose, Some("MatrixClose")),
            KeyBinding::new("alt-o", DiscardAllAndClose, Some("MatrixClose")),
            KeyBinding::new("space", FileDialogActivate, Some("MatrixClose")),
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
            || self.file_open_sequence_pending()
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
        self.close_all_answer = None;
        self.close_multi = self.conditional_close.as_ref().is_some_and(|batch| batch.dirty_count(&self.workspace) > 1);
        self.advance_conditional_close(window, cx);
    }

    pub(super) fn request_close_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.matrix_active() || matrix_surface::locked() {
            self.close_matrix(window, cx);
            return;
        }
        if self.macro_interaction_blocked() {
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Unsupported("file closing"));
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy
            || self.file_open_sequence_pending()
            || self.close_dialog
            || self.save_all_dialog
            || self.save_all_restore.is_some()
        {
            return;
        }
        let id = self.workspace.active().id();
        self.close_all_answer = None;
        self.close_multi = false;
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
        if matrix_surface::locked() { return; }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        match self.workspace.close_buffer(id, disposition) {
            Ok(_) => {
                if self.quit_after_last_document_close(id, window, cx) { return; }
                self.reset_active_view(window, cx);
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }
    pub(super) fn quit_after_last_document_close(&mut self, closed: crate::workspace::BufferId, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.pref_bool("TabBar.quitOnEmpty", false)
            || self.application_quit_pending
            || self.workspace.buffers().iter().any(|buffer| buffer.id() == closed)
            || self.workspace.buffers().len() != 1
            || self.workspace.active().disk_file.is_some()
            || self.workspace.active().is_modified()
            || self.workspace.active().editor.document().len_bytes() != 0 {
            return false;
        }
        self.reset_active_view(window, cx);
        self.request_close(window, cx);
        true
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
            || self.file_open_sequence_pending()
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
                    self.editor_error.as_ref().map(|error| error.render(&self.native_locale())).unwrap_or_else(|| {
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
            Some(crate::localization::Text::from(format!(
                "Save All completed with errors: {}",
                std::mem::take(&mut self.save_all_errors).join("; ")
            )))
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
            self.notify_localized_editor_error(error, cx);
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
        use super::dialog_font::px;
        if !self.save_all_dialog {
            return None;
        }
        let button = |id: &'static str, label: &'static str, selected: bool| {
            let label = crate::localization::current_dialog_control("DoSaveAll", if id == "save-all-yes" { 6 } else { 7 }).unwrap_or(label);
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
                        .aria_label(crate::localization::dialog_title(&self.native_locale(), "DoSaveAll").unwrap_or("Save All Confirmation"))
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
                        .child(crate::localization::current_dialog_control("DoSaveAll", 1766)
                            .unwrap_or("Save all modified documents?").split("\n\n").next().unwrap_or("Save all modified documents?"))
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

    pub(super) fn file_prompt_settings(&self) -> io::Result<serde_json::Value> {
        self.settings_store.as_ref().map(ConfigStore::load_settings).transpose()
            .map(|settings| settings.unwrap_or_else(|| self.preferences.settings.clone()))
    }

    pub(super) fn update_file_prompt_directory(&mut self) {
        if let Some(directory) = self.disk_file().and_then(|file| file.path().parent())
            .filter(|directory| directory.is_dir()).map(PathBuf::from) {
            self.file_prompt.current_directory = Some(directory);
        }
    }

    pub(super) fn file_prompt_directory(&mut self) -> io::Result<PathBuf> {
        let settings = self.file_prompt_settings()?;
        self.update_file_prompt_directory();
        let fallback = || std::env::current_exe().and_then(|path| path.parent().map(PathBuf::from)
            .ok_or_else(|| io::Error::other("应用程序没有所在目录")));
        match settings["openSaveDir"]["value"].as_i64().unwrap_or(0) {
            0 => self.file_prompt.current_directory.clone().map(Ok).unwrap_or_else(fallback),
            1 => {
                let path = PathBuf::from(settings["openSaveDir"]["lastUsedDirPath"].as_str().unwrap_or(""));
                if path.is_dir() { Ok(path) }
                else { self.file_prompt.current_directory.clone().map(Ok).unwrap_or_else(fallback) }
            }
            2 => {
                let path = expanded_directory(settings["openSaveDir"]["defaultDirPath"].as_str().unwrap_or(""));
                if path.is_dir() { Ok(path) } else { fallback() }
            }
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "openSaveDir.value 必须是 0、1 或 2")),
        }
    }

    pub(super) fn remember_file_prompt_directory(&mut self, directory: Option<&std::path::Path>) -> io::Result<()> {
        let Some(directory) = directory else { return Ok(()); };
        let settings = self.file_prompt_settings()?;
        if settings["openSaveDir"]["value"].as_i64().unwrap_or(0) != 1 { return Ok(()); }
        let path = directory.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "文件选择目录不是 UTF-8"))?;
        if let Some(store) = &self.settings_store {
            store.update_settings(|settings| super::preferences_surface::write_value(settings, "openSaveDir.lastUsedDirPath", path.into()))?;
        }
        super::preferences_surface::write_value(&mut self.preferences.settings, "openSaveDir.lastUsedDirPath", path.into())?;
        self.file_prompt.current_directory = Some(directory.to_path_buf());
        Ok(())
    }

    pub(super) fn start_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() { return; }
        if self.macro_interaction_blocked() || self.reload_target.is_some()
            || self.file_busy || self.file_open_sequence_pending() || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() {
            return;
        }
        let directory = match self.file_prompt_directory() {
            Ok(directory) => directory,
            Err(error) => { self.notify_editor_error(format!("无法读取文件选择目录：{error}"), cx); return; }
        };
        let prompt = super::native_path_prompt::prompt_for_paths_in(&directory, gpui::PathPromptOptions {
            files: true, directories: false, multiple: true, prompt: None,
        }, window, cx);
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let selected = prompt.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match selected {
                    Ok(result) => {
                        if let Err(error) = view.remember_file_prompt_directory(result.directory.as_deref()) {
                            view.notify_editor_error(format!("无法记住文件选择目录：{error}"), cx);
                        }
                        if let Some(paths) = result.selection { view.start_open_paths(paths, window, cx); }
                        else { view.editor_focus.focus(window, cx); }
                    }
                    Err(error) => view.notify_editor_error(format!("Open failed: {error}"), cx),
                }
                cx.notify();
            }) { eprintln!("open path chooser lost its document window: {error}"); }
        }).detach();
    }

    pub(super) fn start_open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.start_open_paths_with_launch(paths, None, window, cx);
    }

    pub(super) fn file_open_sequence_pending(&self) -> bool { self.file_prompt.open_sequence_pending }

    pub(super) fn start_open_paths_with_launch(&mut self, paths: Vec<PathBuf>, launch: Option<crate::launch::LaunchOptions>, window: &mut Window, cx: &mut Context<Self>) {
        let completion = OpenLaunchCompletion(launch.clone());
        if matrix_surface::locked() { self.complete_file_preferences_startup(window, cx); return; }
        if paths.is_empty() || self.macro_interaction_blocked() || self.reload_target.is_some()
            || self.file_busy || self.file_prompt.open_sequence_pending || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() {
            self.complete_file_preferences_startup(window, cx);
            return;
        }
        let settings = match self.file_prompt_settings() {
            Ok(settings) => settings,
            Err(error) => { self.notify_editor_error(format!("无法读取文件打开设置：{error}"), cx); self.complete_file_preferences_startup(window, cx); return; }
        };
        if let Some(options) = &launch && let Err(error) = options.resolved_language(&self.workspace) {
            self.notify_editor_error(error.to_string(), cx);
            self.complete_file_preferences_startup(window, cx);
            return;
        }
        let session_extension = settings["sessionExt"].as_str().unwrap_or("").to_owned();
        let workspace_extension = settings["workspaceExt"].as_str().unwrap_or("").to_owned();
        let suppress_warning = settings["largeFileRestriction"]["suppress2GBWarning"].as_bool().unwrap_or(false);
        let existing_paths = self.workspace.buffers().iter().filter_map(|buffer| {
            buffer.disk_file.as_ref().filter(|file| !file.is_placeholder()).map(|file| file.path().to_path_buf())
        }).collect::<Vec<_>>();
        let placeholder = (self.workspace.buffers().len() == 1 && self.workspace.active().disk_file.is_none() && !self.workspace.active().is_modified() && self.workspace.active().editor.document().len_bytes() == 0)
            .then(|| (self.workspace.active_view(), self.workspace.active().id()));
        self.file_busy = true;
        self.file_prompt.open_sequence_pending = true;
        let executor = cx.background_executor().clone();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let _completion = completion;
            let requested = paths.len();
            let mut succeeded = 0;
            let mut opened = false;
            let mut errors = Vec::new();
            for path in paths {
                if !launch.as_ref().is_some_and(|options| options.open_session) {
                    let existing = match view.update_in(cx, |view, window, cx| {
                        let Some(id) = crate::folder_workspace::existing_buffer(&view.workspace, &path) else { return Ok(false); };
                        if view.workspace.buffers().iter().any(|buffer| buffer.id() == id && buffer.disk_file.as_ref().is_some_and(DiskFile::is_placeholder)) {
                            return Ok(false);
                        }
                        if !view.cancel_view_composition(window, cx) { return Err(io::Error::other("could not cancel composition before opening a file")); }
                        view.workspace.activate(id).map_err(|error| io::Error::other(error.to_string()))?;
                        if let Some(language) = launch.as_ref().map(|options| options.resolved_language(&view.workspace)).transpose()?.flatten() { view.workspace.active_mut().set_language(language); }
                        view.initialize_active_change_history();
                        Ok(true)
                    }) {
                        Ok(result) => result,
                        Err(error) => { eprintln!("existing file open window unavailable: {error}"); return; }
                    };
                    match existing {
                        Ok(true) => { succeeded += 1; opened = true; continue; },
                        Ok(false) => {},
                        Err(error) => { errors.push(format!("{}: {error}", path.display())); continue; }
                    }
                }
                let inspect_path = path.clone();
                let session_extension = session_extension.clone();
                let workspace_extension = workspace_extension.clone();
                let existing_paths = existing_paths.clone();
                let force_session = launch.as_ref().is_some_and(|options| options.open_session);
                let inspected = executor.spawn(async move {
                    let path = inspect_path;
                    let _access = crate::file_access::read(&path)?;
                    let already_open = existing_paths.contains(&path) || std::fs::canonicalize(&path)
                        .is_ok_and(|resolved| existing_paths.contains(&resolved));
                    let session = force_session || (!already_open && path.is_file() && matches_configured_extension(&path, &session_extension));
                    let workspace = !session && !already_open && path.is_file() && matches_configured_extension(&path, &workspace_extension);
                    let huge = !session && !workspace && !already_open && !suppress_warning && std::fs::metadata(&path).is_ok_and(|metadata| {
                        let size = metadata.len();
                        size.saturating_add((1_u64 << 20).min(size / 6)) > i32::MAX as u64
                    });
                    Ok::<_, io::Error>((session, workspace, huge))
                }).await;
                let (session, workspace, huge) = match inspected {
                    Ok(inspected) => inspected,
                    Err(error) => { errors.push(format!("{}: {error}", path.display())); continue; }
                };
                if huge {
                    let message = format!("{}\n\n打开 2GB 以上的大文件可能需要几分钟。是否打开？", path.display());
                    let prompt = match view.update_in(cx, |_, window, cx| window.prompt(gpui::PromptLevel::Warning, "打开大文件警告", Some(&message), &[gpui::PromptButton::ok("是"), gpui::PromptButton::cancel("否")], cx)) {
                        Ok(prompt) => prompt,
                        Err(error) => { eprintln!("large file confirmation window unavailable: {error}"); return; }
                    };
                    match prompt.await {
                        Ok(0) => {},
                        Ok(_) => continue,
                        Err(error) => { errors.push(format!("{}: {error}", path.display())); continue; }
                    }
                }
                let load_path = path.clone();
                let settings = settings.clone();
                let load_options = launch.clone();
                let result = executor.spawn(async move {
                    if session { OpenPathResult::Session(crate::recovery::SessionFile::load(&load_path)) }
                    else if workspace { OpenPathResult::Workspace }
                    else { OpenPathResult::Text(match load_options {
                        Some(options) => options.load_file_buffer(&load_path, &settings),
                        None => DiskFile::load_buffer_with_preferences(&load_path, &settings),
                    }) }
                }).await;
                let mut open_completion = None;
                let result = match view.update_in(cx, |view, window, cx| {
                    match result {
                        OpenPathResult::Text(result) => match result {
                            Ok((file, bytes)) => {
                                if !view.cancel_view_composition(window, cx) { return Err(io::Error::other("could not cancel composition before opening a file")); }
                                let id = view.workspace.open_buffer_bytes(file, bytes).map_err(|error| io::Error::other(error.to_string()))?;
                                if let Some((pane, initial)) = placeholder
                                    && initial != id && view.workspace.view_buffer_ids(pane).contains(&initial) {
                                    view.workspace.close_in_view(pane, initial, crate::workspace::CloseDisposition::IfUnmodified).map_err(|error| io::Error::other(error.to_string()))?;
                                    view.workspace.activate(id).map_err(|error| io::Error::other(error.to_string()))?;
                                }
                                let requested_language = launch.as_ref().map(|options| options.resolved_language(&view.workspace)).transpose()?.flatten();
                                if let Some(language) = requested_language {
                                    view.workspace.active_mut().set_language(language);
                                } else if let Some(language) = view.style_config.language_for_path(&path, view.dark)
                                    && let Some(buffer) = view.workspace.buffers_mut().find(|buffer| buffer.id() == id) {
                                    buffer.set_language(crate::syntax::Language::Builtin(language));
                                }
                                view.initialize_active_change_history();
                                Ok(true)
                            }
                            Err(error) => Err(error),
                        },
                        OpenPathResult::Session(result) => match result {
                            Ok(archive) => {
                                view.file_busy = false;
                                open_completion = Some(view.accept_session_archive_with_completion(path.clone(), archive, window, cx));
                                Ok(false)
                            }
                            Err(error) => Err(error),
                        },
                        OpenPathResult::Workspace => {
                            view.file_busy = false;
                            open_completion = Some(view.load_project_workspace_path_with_completion(path.clone(), window, cx)?);
                            Ok(false)
                        }
                    }
                }) {
                    Ok(result) => result,
                    Err(error) => { eprintln!("open completion could not reach its window: {error}"); return; }
                };
                match result {
                    Ok(true) => { succeeded += 1; opened = true; },
                    Ok(false) => {},
                    Err(error) => errors.push(format!("{}: {error}", path.display())),
                }
                if let Some(completion) = open_completion {
                    let mut completion = completion;
                    let completed = loop {
                        let (next, result) = executor.spawn(async move {
                            let result = completion.recv_timeout(std::time::Duration::from_secs(120));
                            (completion, result)
                        }).await;
                        completion = next;
                        match result {
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                if let Some(options) = &launch && let Err(error) = options.complete(false) { eprintln!("file open timeout confirmation failed: {error}"); }
                                if let Err(error) = view.update_in(cx, |_, _, _| {}) { eprintln!("pending open window unavailable: {error}"); return; }
                            }
                            result => break result,
                        }
                    };
                    match completed {
                        Ok(true) => {
                            let applied = match view.update_in(cx, |view, _, _| {
                                if let Some(language) = launch.as_ref().map(|options| options.resolved_language(&view.workspace)).transpose()?.flatten() { view.workspace.active_mut().set_language(language); }
                                Ok::<_, io::Error>(())
                            }) {
                                Ok(result) => result,
                                Err(error) => { eprintln!("workspace launch options window unavailable: {error}"); return; }
                            };
                            match applied {
                                Ok(()) => { succeeded += 1; opened = true; },
                                Err(error) => errors.push(format!("{}: {error}", path.display())),
                            }
                        }
                        Ok(false) => {},
                        Err(error) => { errors.push(format!("{}: {error}", path.display())); break; },
                    }
                    if let Err(error) = view.update_in(cx, |view, _, cx| { view.file_busy = true; cx.notify(); }) { eprintln!("workspace open continuation unavailable: {error}"); return; }
                }
            }
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                view.file_prompt.open_sequence_pending = false;
                view.workspace.refresh_udl_metadata();
                if opened {
                    if let Some(options) = &launch
                        && let Err(error) = view.apply_launch_document_position(options) {
                        errors.push(error.to_string());
                    }
                    view.reset_active_view(window, cx);
                    if launch.as_ref().is_some_and(|options| options.line.is_some() || options.column.is_some()) { view.editor_follow_caret = true; }
                    view.update_file_prompt_directory();
                } else { view.editor_focus.focus(window, cx); cx.notify(); }
                view.preferences_apply_large_file_open_policy(window, cx);
                view.complete_file_preferences_startup(window, cx);
                if let Some(options) = &launch && let Err(error) = options.complete(succeeded == requested && errors.is_empty()) {
                    view.notify_editor_error(format!("无法确认打开请求：{error}"), cx);
                }
                if !errors.is_empty() { view.notify_editor_error(format!("Open failed: {}", errors.join("; ")), cx); }
            }) { eprintln!("open completion could not reach its window: {error}"); }
        }).detach();
    }

    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() { return; }
        if self.macro_interaction_blocked() {
            self.cancel_macro_repeat(window, cx);
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy || self.file_open_sequence_pending() || self.save_all_dialog || self.close_dialog {
            return;
        }
        if !self.prepare_udl_close(window, cx) {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if self
            .recovery
            .as_ref()
            .is_some_and(crate::recovery::RecoveryManager::snapshot_mode)
        {
            if !self.synchronize_recovery(cx) {
                self.cancel_native_quit();
                return;
            }
        }
        self.close_target = None;
        let pending = match self.window_close_pending() {
            Ok(pending) => pending,
            Err(error) => {
                self.notify_localized_editor_error(crate::localization::Text::resource("BackupRecoveryCheckFailed", "Could not check recovery backups: $ERROR$")
                    .replace("$ERROR$", error.to_string()), cx);
                self.cancel_native_quit();
                return;
            }
        };
        self.close_multi = pending.len() > 1;
        if let Some(id) = pending.first().copied() {
            if !self.activate_buffer(id, window, cx) {
                self.close_after_save = false;
                self.close_dialog = false;
                self.close_multi = false;
                self.close_discarded.clear();
                self.cancel_native_quit();
                cx.notify();
                return;
            }
            self.scroll_active_tab_into_view();
            self.editor_lines.clear();
            self.editor_painted_revision = None;
            self.close_dialog = true;
            self.close_choice = 0;
            if self.close_all_answer == Some(CloseAllAnswer::Save) {
                self.close_dialog = false;
                self.save_and_close(window, cx);
            } else {
                self.close_focus.focus(window, cx);
                cx.notify();
            }
        } else {
            if self.retire_recovery_on_confirmed_close(cx) {
                self.finish_window_close(window, cx);
            } else {
                self.cancel_native_quit();
            }
        }
    }

    fn window_close_pending(&self) -> io::Result<Vec<crate::workspace::BufferId>> {
        let snapshot = self.recovery.as_ref().filter(|recovery| recovery.snapshot_mode());
        let mut pending = Vec::new();
        for id in self.workspace.dirty_buffers() {
            if self.close_discarded.contains(&id) { continue; }
            if let Some(recovery) = snapshot {
                if recovery.has_buffer_backup(id)? { continue; }
            }
            pending.push(id);
        }
        Ok(pending)
    }

    fn finish_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() {
            self.cancel_native_quit();
            return;
        }
        self.save_search_history_on_close(cx);
        if let Some(control) = &self.native_quit {
            control.approve();
        }
        window.remove_window();
        cx.quit();
    }

    pub(super) fn apply_close_all(&mut self, answer: CloseAllAnswer, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.apply_close_all(answer, window, cx)) {
            return;
        }
        if !self.close_dialog || !self.close_multi || self.file_busy || self.reload_target.is_some() {
            return;
        }
        self.close_all_answer = Some(answer);
        self.close_dialog = false;
        match answer {
            CloseAllAnswer::Save => self.save_and_close(window, cx),
            CloseAllAnswer::Discard if self.conditional_close.is_some() => self.discard_close(window, cx),
            CloseAllAnswer::Discard => {
                let pending = match self.window_close_pending() {
                    Ok(pending) => pending,
                    Err(error) => {
                        self.notify_localized_editor_error(crate::localization::Text::resource("BackupRecoveryCheckFailed", "Could not check recovery backups: $ERROR$")
                            .replace("$ERROR$", error.to_string()), cx);
                        self.cancel_close(window, cx);
                        return;
                    }
                };
                self.close_discarded.extend(pending);
                self.request_close(window, cx);
            }
        }
    }

    fn activate_close_choice(&mut self, choice: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.close_dialog || self.file_busy { return; }
        match choice {
            0 => self.save_and_close(window, cx),
            1 => self.discard_close(window, cx),
            2 => self.cancel_close(window, cx),
            3 => self.apply_close_all(CloseAllAnswer::Save, window, cx),
            4 => self.apply_close_all(CloseAllAnswer::Discard, window, cx),
            _ => {}
        }
    }

    fn close_message(&self) -> String {
        let filename = self.disk_file().map(|file| file.path().to_string_lossy().into_owned())
            .unwrap_or_else(|| self.workspace.active().name());
        let locale = self.native_locale();
        let message = if self.close_target.is_none()
            && self.recovery.as_ref().is_some_and(crate::recovery::RecoveryManager::snapshot_mode) {
            crate::localization::dialog_message(&locale, "NoBackupDoSaveFile")
                .unwrap_or("Your backup file cannot be found. Save it otherwise your data will be lost.\nDo you want to save file \"$STR_REPLACE$\" ?")
        } else {
            crate::localization::dialog_control(&locale, "DoSaveOrNot", 1761)
                .unwrap_or("Save file \"$STR_REPLACE$\" ?")
        };
        message
            .replace("$STR_REPLACE$", &filename)
            .replace("\r\n", "\n").replace('\r', "\n")
    }

    pub(super) fn close_dialog_dimensions(&self, window: &Window) -> gpui::Size<Pixels> {
        use super::dialog_font::px;
        if self.close_multi { return size(px(468.), px(120.)); }
        let message = self.close_message();
        let font = gpui::font(dialog_font::font_family());
        let font_size = gpui::px(dialog_font::base_size());
        let run = |text: &str| gpui::TextRun {
            len: text.len(), font: font.clone(), color: rgb(0x202020).into(),
            background_color: None, underline: None, strikethrough: None,
        };
        let measured = message.lines().map(|line| window.text_system().shape_line(line.to_owned().into(), font_size, &[run(line)], None).width())
            .fold(px(0.), Pixels::max);
        let width = (measured + px(80.)).max(px(320.)).min(px(600.));
        let measure_height = |text: &str| {
            match window.text_system().shape_text(text.to_owned().into(), font_size, &[run(text)], Some(width - px(80.)), None) {
                Ok(lines) => lines.iter().map(|line| line.size(dialog_font::line_height()).height).fold(px(0.), |total, height| total + height),
                Err(error) => {
                    eprintln!("could not measure save confirmation text: {error}");
                    px(32.)
                }
            }
        };
        let mut body_height = measure_height(&message);
        if let Some(error) = &self.editor_error {
            body_height += px(6.) + measure_height(&error.render(&self.native_locale()));
        }
        size(width, body_height.max(px(32.)).min(px(220.)) + px(76.))
    }

    pub(super) fn render_close_dialog(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use super::dialog_font::px;
        if !self.close_dialog {
            return None;
        }
        let dimensions = self.close_dialog_dimensions(window);
        let rtl = crate::localization::is_rtl(&self.native_locale());
        let count = if self.close_multi { 5 } else { 3 };
        let enabled = !self.file_busy;
        let foreground = if self.dark { 0xe6e6e6 } else { 0x202020 };
        let background = if self.dark { 0x202020 } else { 0xf0f0f0 };
        let specs = if self.close_multi {
            vec![
                ("close-save", "Yes", 6, 0, px(15.), px(75.)),
                ("close-discard", "No", 7, 1, px(94.5), px(75.)),
                ("close-cancel", "Cancel", 2, 2, px(174.), px(90.)),
                ("close-save-all", "Yes to all", 4, 3, px(268.5), px(90.)),
                ("close-discard-all", "No to all", 5, 4, px(363.), px(90.)),
            ]
        } else {
            let left = dimensions.width - px(271.);
            vec![
                ("close-save", "Yes", 6, 0, left, px(75.)),
                ("close-discard", "No", 7, 1, left + px(83.), px(75.)),
                ("close-cancel", "Cancel", 2, 2, left + px(166.), px(90.)),
            ]
        };
        let buttons = specs.into_iter().map(|(id, label, source_id, choice, left, width)| {
            let label = crate::localization::current_dialog_control("DoSaveOrNot", source_id).unwrap_or(label);
            let selected = self.close_choice == choice;
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(label)
                .aria_selected(selected)
                .aria_description(if !enabled {
                    "Unavailable while saving"
                } else {
                    ""
                })
                .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
                .absolute()
                .left(if rtl { dimensions.width - left - width } else { left })
                .top(if self.close_multi { px(90.) } else { dimensions.height - px(36.) })
                .w(width)
                .h(px(21.))
                .border_1()
                .border_color(rgb(if selected { 0x0078d4 } else if self.dark { 0x606060 } else { 0x808080 }))
                .bg(rgb(if self.dark { 0x333333 } else { 0xf0f0f0 }))
                .text_color(rgb(if enabled { foreground } else { 0x808080 }))
                .flex().items_center().justify_center()
                .when(enabled, |button| button.cursor_pointer().hover(|style| style.border_color(rgb(0x0078d4))))
                .child(label)
                .on_click(cx.listener(move |view, _, window, cx| view.activate_close_choice(choice, window, cx)))
        }).collect::<Vec<_>>();
        let message_left = if self.close_multi { px(10.5) } else { px(64.) };
        let message_width = if self.close_multi { px(435.) } else { dimensions.width - px(80.) };
        let mut message = div().id("close-message").absolute()
            .left(if rtl { dimensions.width - message_left - message_width } else { message_left })
            .top(if self.close_multi { px(15.) } else { px(20.) })
            .w(message_width)
            .h(if self.close_multi { px(60.) } else { dimensions.height - px(76.) })
            .overflow_y_scroll().flex().flex_col().gap(px(6.))
            .when(rtl, |message| message.text_right())
            .child(div().w_full().min_w(px(0.)).flex_shrink_0().child(self.close_message()));
        if let Some(error) = &self.editor_error {
            let error = error.render(&self.native_locale());
            message = message.child(div().id("close-save-error").role(Role::Alert).aria_label(error.clone()).flex_shrink_0()
                .text_color(rgb(if self.dark { 0xffa080 } else { 0xb00020 })).child(error));
        }
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
                        .aria_label(crate::localization::dialog_title(&self.native_locale(), "DoSaveOrNot").unwrap_or("Save"))
                        .track_focus(&self.close_focus)
                        .key_context("MatrixClose")
                        .on_action(cx.listener(move |view, _: &FileDialogNext, _, cx| {
                            if view.close_dialog && !view.file_busy {
                                    view.close_choice = (view.close_choice + 1) % count;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(move |view, _: &FileDialogPrevious, _, cx| {
                            if view.close_dialog && !view.file_busy {
                                    view.close_choice = (view.close_choice + count - 1) % count;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &FileDialogActivate, window, cx| {
                            if view.close_dialog && !view.file_busy {
                                view.activate_close_choice(view.close_choice, window, cx);
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
                        .on_action(cx.listener(|view, _: &SaveAllAndClose, window, cx| {
                            view.apply_close_all(CloseAllAnswer::Save, window, cx);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|view, _: &DiscardAllAndClose, window, cx| {
                            view.apply_close_all(CloseAllAnswer::Discard, window, cx);
                            cx.stop_propagation();
                        }))
                        .size_full().relative()
                        .font_family(dialog_font::font_family())
                        .text_size(gpui::px(dialog_font::base_size()))
                        .line_height(dialog_font::line_height())
                        .border_1()
                        .border_color(rgb(if self.dark { 0x606060 } else { 0x808080 }))
                        .bg(rgb(background)).text_color(rgb(foreground))
                        .when(!self.close_multi, |dialog| dialog.child(div().absolute().left(if rtl { dimensions.width - px(48.) } else { px(16.) }).top(px(20.))
                            .size(px(32.)).rounded_full().bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                            .text_size(px(26.)).line_height(px(32.)).flex().items_center().justify_center().child("?")))
                        .child(message).children(buttons),
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
        if self.conditional_close.is_some() && !self.file_busy {
            self.finish_close_batch(true, window, cx);
        } else if self.close_all_answer.is_some() && !self.file_busy {
            self.close_after_save = false;
            self.cancel_native_quit();
            self.close_dialog = true;
            self.close_choice = 0;
            self.close_focus.focus(window, cx);
            cx.notify();
        }
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
        if matrix_surface::locked() || self.matrix_active() { return; }
        if self.macro_interaction_blocked() {
            return;
        }
        if matches!(operation, SaveOperation::Current) {
            self.record_macro_step(crate::macros::MacroStep::Unsupported("file saving"));
        }
        if self.reload_target.is_some() {
            return;
        }
        if self.file_busy || self.file_open_sequence_pending() {
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
        let backup_store = self.settings_store.clone();
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
        let byte_code_page = intent.byte_code_page();
        let needs_path = operation != SaveOperation::Current || original.is_none();
        let directory = if needs_path {
            match self.file_prompt_directory() {
                Ok(directory) => directory,
                Err(error) => { self.notify_editor_error(format!("无法读取文件保存目录：{error}"), cx); return; }
            }
        } else { PathBuf::new() };
        let suggested = original
            .as_ref()
            .and_then(|file| file.path().file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                let name = self.workspace.active().name();
                if std::path::Path::new(&name).extension().is_some() {
                    name
                } else {
                    format!("{name}.txt")
                }
            });
        let prompt = if needs_path {
            Some(super::native_path_prompt::prompt_for_new_path_in(&directory, Some(&suggested), window, cx))
        } else {
            None
        };
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let selected = if let Some(prompt) = prompt {
                match prompt.await {
                    Ok(result) => {
                        match view.update_in(cx, |view, _, cx| {
                            if let Err(error) = view.remember_file_prompt_directory(result.directory.as_deref()) {
                                view.notify_editor_error(format!("无法记住文件保存目录：{error}"), cx);
                            }
                        }) {
                            Ok(()) => Ok(result.selection),
                            Err(error) => Err(io::Error::other(format!("文件保存目录失去文档窗口：{error}"))),
                        }
                    }
                    Err(error) => Err(error),
                }
            } else {
                Ok(original.as_ref().map(|file| file.path().to_path_buf()))
            };
            let selected = if selected.as_ref().is_ok_and(Option::is_some) {
                if let Some(source) = backup_source {
                    match executor.spawn(async move {
                        match backup_store {
                            Some(store) => backup_options.create_tracked_backup(&store, &source, &backup_timestamp),
                            None => backup_options.create_backup(&source, &backup_timestamp),
                        }
                    }).await {
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
                                DiskFile::save_as_with_byte_format(
                                    &path,
                                    &writing,
                                    format,
                                    byte_code_page,
                                    OverwritePolicy::Confirmed,
                                )
                            } else if let Some(file) = original {
                                file.save_with_byte_format(&writing, format, byte_code_page)
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
                    view.encoding_save_finished(intent.buffer_id(), false, window, cx);
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
                if view.encoding_save_finished(intent.buffer_id(), committed, window, cx) {
                    return;
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
                    view.cancel_native_quit();
                    view.close_all_answer = None;
                    view.close_choice = 0;
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
