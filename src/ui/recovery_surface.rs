use super::*;

impl MatrixWindow {
    pub(super) fn install_recovery(
        &mut self,
        recovery: crate::recovery::RecoveryManager,
        workspace: crate::workspace::Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let warnings = recovery.warnings().join("\n");
        self.recovery_reload_pending = recovery.changed_buffers().iter().copied().collect();
        self.retire_editor_session_views();
        self.workspace = workspace;
        self.recovery = Some(recovery);
        self.restore_session_ui(window, cx);
        self.restore_session_monitoring(window, cx);
        self.reset_active_view(window, cx);
        self.advance_recovery_reload(window, cx);
        if !warnings.is_empty() {
            self.notify_editor_error(warnings, cx);
        }
    }

    fn advance_recovery_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.reload_target.is_some()
            || self.close_dialog
            || self.save_all_dialog
        {
            return;
        }
        let previous_error = self.editor_error.clone();
        while let Some(id) = self.recovery_reload_pending.pop_front() {
            if !self
                .workspace
                .buffers()
                .iter()
                .any(|buffer| buffer.id() == id && buffer.disk_file.is_some())
            {
                continue;
            }
            if !self.activate_buffer(id, window, cx) {
                self.recovery_reload_pending.push_front(id);
                return;
            }
            let caret = self.editor().document().selection().head;
            let result = self.editor_mut().select(Selection {
                anchor: caret,
                head: caret,
            });
            self.editor_changed(result, window, cx);
            self.reload_external_change = true;
            self.request_reload(window, cx);
            if self.reload_dialog {
                self.reload_choice_yes = false;
                if previous_error.is_some() {
                    self.editor_error = previous_error;
                }
                cx.notify();
                return;
            }
            if self.reload_target.is_some() {
                return;
            }
            self.reload_external_change = false;
        }
    }

    pub(super) fn finish_recovery_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reload_external_change {
            self.reload_external_change = false;
            self.advance_recovery_reload(window, cx);
        }
    }

    pub(super) fn synchronize_recovery(&mut self, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.persist_recent_files() {
            self.notify_editor_error(format!("Recent-file history failed: {error}"), cx);
            return false;
        }
        if let Err(error) = self.capture_session_ui() {
            self.notify_editor_error(format!("Session snapshot failed: {error}"), cx);
            return false;
        }
        let result = match &mut self.recovery {
            Some(recovery) => recovery.checkpoint(&self.workspace),
            None => return true,
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.notify_editor_error(format!("Recovery snapshot failed: {error}"), cx);
                false
            }
        }
    }

    pub(super) fn retire_recovery_on_confirmed_close(&mut self, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.capture_session_ui() {
            self.notify_editor_error(format!("Session snapshot failed: {error}"), cx);
            return false;
        }
        let result = match &mut self.recovery {
            Some(recovery) => recovery.finish_guarded_exit(&self.workspace, &self.close_discarded),
            None => return true,
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.notify_editor_error(format!("Recovery cleanup failed: {error}"), cx);
                false
            }
        }
    }

    pub(super) fn start_recovery_timer(
        &self,
        interval: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |view, cx| {
            loop {
                executor.timer(interval).await;
                let active = view.update(cx, |view, cx| {
                    if !cx.windows().contains(&view.document_window) {
                        return false;
                    }
                    view.synchronize_recovery(cx);
                    true
                });
                if !matches!(active, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
}
