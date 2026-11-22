use super::*;
use serde_json::Value;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Default)]
pub(super) struct BackupRetentionState {
    cancelled: Option<Arc<AtomicBool>>,
    task: Option<gpui::Task<()>>,
}

impl Drop for BackupRetentionState {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.cancelled { cancelled.store(true, Ordering::Release); }
    }
}

impl MatrixWindow {
    pub(super) fn configure_backup_retention(&mut self, settings: &Value, cx: &mut Context<Self>) -> io::Result<()> {
        let options = crate::backup_retention::BackupRetentionOptions::from_settings(settings)?;
        let store = self.settings_store.clone();
        if let Some(cancelled) = self.backup_retention.cancelled.take() { cancelled.store(true, Ordering::Release); }
        self.backup_retention.task = None;
        if !options.enabled { return Ok(()); }
        let Some(store) = store else { return Ok(()); };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.backup_retention.cancelled = Some(cancelled.clone());
        let executor = cx.background_executor().clone();
        self.backup_retention.task = Some(cx.spawn(async move |view, cx| {
            loop {
                if cancelled.load(Ordering::Acquire) { break; }
                let worker_store = store.clone();
                let worker_cancelled = cancelled.clone();
                let result = executor.spawn(async move {
                    crate::backup_retention::cleanup(&worker_store, std::time::SystemTime::now(), &worker_cancelled)
                }).await;
                let retry = result.as_ref().is_ok_and(|report| report.busy_recovery > 0 || report.busy_save > 0);
                let active = view.update(cx, |view, cx| {
                    if cancelled.load(Ordering::Acquire)
                        || !view.backup_retention.cancelled.as_ref().is_some_and(|current| Arc::ptr_eq(current, &cancelled))
                        || !cx.windows().contains(&view.document_window) {
                        return false;
                    }
                    match result {
                        Ok(report) if !report.failures.is_empty() => {
                            let paths = report.failures.iter().map(|failure| failure.path.display().to_string()).collect::<Vec<_>>().join("\n");
                            view.notify_localized_editor_error(crate::localization::Text::resource("BackupRetentionCleanupFailed", "Some backups could not be safely cleaned ($COUNT$).\n$PATHS$")
                                .replace("$COUNT$", report.failures.len().to_string()).replace("$PATHS$", paths), cx);
                        }
                        Ok(_) => {}
                        Err(error) => view.notify_localized_editor_error(crate::localization::Text::resource("BackupRetentionInspectionFailed", "Could not check backup cleanup: $ERROR$")
                            .replace("$ERROR$", error.to_string()), cx),
                    }
                    true
                });
                if !matches!(active, Ok(true)) { break; }
                executor.timer(std::time::Duration::from_secs(if retry { 1 } else { 86_400 })).await;
            }
        }));
        Ok(())
    }

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
        self.confirm_absent_session_files(false,window,cx);
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
        if self.preferences.files.placeholder_decision_pending { return true; }
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
        if let Some(path) = self.preferences.files.loaded_session_path.clone() {
            let result = crate::recovery::SessionFile::capture(&self.workspace)
                .and_then(|session| session.save(&path,crate::files::OverwritePolicy::Confirmed));
            if let Err(error) = result {
                self.notify_editor_error(format!("会话自动保存失败: {error}"),cx);
                return false;
            }
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
        &mut self,
        interval: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let executor = cx.background_executor().clone();
        self.preferences.files.recovery_task = Some(cx.spawn(async move |view, cx| {
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
        }));
    }

    pub(super) fn confirm_absent_session_files(&mut self, manually_saved: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<_> = self.workspace.buffers().iter().filter(|buffer| buffer.disk_file.as_ref().is_some_and(crate::files::DiskFile::is_placeholder)).map(|buffer| buffer.id()).collect();
        if ids.is_empty() || self.preferences.files.placeholder_decision_pending { return; }
        self.preferences.files.placeholder_decision_pending = true;
        let message = if manually_saved {
            "手动保存的会话中有无法访问的文件。是否创建空白只读占位文档？\n\n不创建或之后关闭占位文档，不会在退出时修改该手动会话。"
        } else {
            "上次会话中有无法访问的文件。是否创建空白只读占位文档？\n\n不创建或之后关闭占位文档，退出时会修改当前会话记录。建议先备份会话。"
        };
        let prompt = window.prompt(gpui::PromptLevel::Warning,"文件无法访问",Some(message),&[gpui::PromptButton::ok("是"),gpui::PromptButton::cancel("否")],cx);
        cx.spawn_in(window,async move |view,cx| {
            let response = prompt.await;
            if let Err(error) = view.update_in(cx,|view,window,cx| {
                view.preferences.files.placeholder_decision_pending = false;
                match response {
                    Ok(0) => {},
                    Ok(_) => {
                        for id in ids {
                            if let Err(error) = view.workspace.close_buffer(id,crate::workspace::CloseDisposition::IfUnmodified) {
                                view.notify_editor_error(error.to_string(),cx);
                            }
                        }
                        view.reset_active_view(window,cx);
                    }
                    Err(error) => view.notify_editor_error(format!("占位文档确认失败: {error}"),cx),
                }
                view.synchronize_recovery(cx);
                cx.notify();
            }) { eprintln!("absent file prompt window unavailable: {error}"); }
        }).detach();
    }
}
