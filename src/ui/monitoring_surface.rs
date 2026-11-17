use super::*;
use crate::monitoring::{MonitorUpdate, Monitoring, PollRequest};

#[derive(Default)]
pub(super) struct MonitoringState {
    core: Monitoring,
    automatic: crate::monitoring::AutoDetection,
    task: Option<gpui::Task<()>>,
    running: bool,
}

impl MatrixWindow {
    pub(super) fn configure_file_detection(&mut self, settings: &serde_json::Value, window: &mut Window, cx: &mut Context<Self>) -> io::Result<()> {
        let options = crate::monitoring::AutoDetectionOptions::from_settings(settings)?;
        self.monitoring.automatic.configure(options);
        if options.mode != 0 || !self.monitoring.core.is_empty() { self.ensure_monitoring_timer(window,cx); }
        Ok(())
    }

    pub(super) fn restore_session_monitoring(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let errors = self.monitoring.core.restore(&mut self.workspace);
        if !self.monitoring.core.is_empty() {
            self.ensure_monitoring_timer(window, cx);
        }
        if !errors.is_empty() {
            self.notify_editor_error(errors.join("\n"), cx);
        }
    }

    pub(super) fn monitoring_active(&self) -> bool {
        self.monitoring
            .core
            .is_monitoring(self.workspace.active().id())
    }

    pub(super) fn toggle_monitoring(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.toggle_monitoring(window, cx);
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_restore.is_some()
            || self.reload_target.is_some()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let id = self.workspace.active().id();
        if self.monitoring.core.is_monitoring(id) {
            self.monitoring.core.stop(&mut self.workspace, id);
            if self.monitoring.core.is_empty() && self.monitoring.automatic.options().mode == 0 {
                self.monitoring.task = None;
                self.monitoring.running = false;
            }
        } else {
            if !self.cancel_view_composition(window, cx) {
                return;
            }
            if let Err(error) = self.monitoring.core.start(&mut self.workspace, id) {
                self.notify_editor_error(format!("Monitoring problem: {error}"), cx);
                return;
            }
            self.ensure_monitoring_timer(window, cx);
        }
        self.editor_changed(Ok(()), window, cx);
        self.synchronize_recovery(cx);
    }

    fn ensure_monitoring_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.monitoring.running {
            return;
        }
        self.monitoring.running = true;
        let executor = cx.background_executor().clone();
        self.monitoring.task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(250)).await;
                let requests = match view.update_in(cx, |view, _window, _cx| {
                    if view.file_busy
                        || view.close_dialog
                        || view.save_all_restore.is_some()
                        || view.reload_target.is_some()
                    {
                        return Some((Vec::new(),Vec::new()));
                    }
                    let requests = view.monitoring.core.requests(&mut view.workspace);
                    let automatic = view.monitoring.automatic.requests(&view.workspace);
                    if view.monitoring.core.is_empty() && view.monitoring.automatic.options().mode == 0 {
                        view.monitoring.running = false;
                        return None;
                    }
                    Some((requests,automatic))
                }) {
                    Ok(Some(requests)) => requests,
                    Ok(None) | Err(_) => break,
                };
                if requests.0.is_empty() && requests.1.is_empty() {
                    continue;
                }
                let results = executor
                    .spawn(async move {
                        let manual = requests.0
                            .into_iter()
                            .map(PollRequest::poll)
                            .collect::<Vec<_>>();
                        let automatic = requests.1.into_iter().map(crate::monitoring::AutoDetectionRequest::poll).collect::<Vec<_>>();
                        (manual,automatic)
                    })
                    .await;
                if view
                    .update_in(cx, |view, window, cx| {
                        if view.file_busy
                            || view.close_dialog
                            || view.save_all_restore.is_some()
                            || view.reload_target.is_some()
                        {
                            return;
                        }
                        let mut changed = false;
                        let mut active_reloaded = false;
                        for result in results.0 {
                            match view.monitoring.core.apply(&mut view.workspace, result) {
                                Ok(MonitorUpdate::Reloaded { follow_tail, .. }) => {
                                    changed = true;
                                    active_reloaded |= follow_tail;
                                }
                                Ok(MonitorUpdate::Stopped { .. }) => changed = true,
                                Ok(MonitorUpdate::Unchanged | MonitorUpdate::Stale) => {}
                                Err(error) => view.notify_editor_error(
                                    format!("Monitoring reload failed: {error}"),
                                    cx,
                                ),
                            }
                        }
                        if window.is_window_active() {
                            for result in results.1 {
                                match view.monitoring.automatic.apply(&mut view.workspace,result) {
                                    Ok(Some(change)) => view.preferences.files.auto_changes.push_back(change),
                                    Ok(None) => {},
                                    Err(error) => view.notify_editor_error(format!("自动检测文件失败: {error}"),cx),
                                }
                            }
                            view.advance_auto_detection(window,cx);
                        }
                        if active_reloaded {
                            view.reset_active_view(window, cx);
                        } else if changed {
                            view.synchronize_recovery(cx);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn advance_auto_detection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::monitoring::AutoChange;
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.reload_target.is_some() || self.reload_dialog || self.preferences.files.auto_prompt_pending { return; }
        if let Some((id,previous)) = self.preferences.files.auto_reload_follow.take()
            && self.workspace.buffers().iter().any(|buffer| buffer.id() == id && buffer.disk_file.as_ref().is_some_and(|file| file.original_modified() != previous))
            && self.monitoring.automatic.options().go_to_end && self.workspace.active().id() == id {
            let result = self.editor_mut().move_to_document_edge(true,false);
            self.editor_changed(result,window,cx);
        }
        let Some(change) = self.preferences.files.auto_changes.pop_front() else { return; };
        match change {
            AutoChange::ReadOnly(_,_) => cx.notify(),
            AutoChange::Modified(id) => {
                let Some(buffer) = self.workspace.buffers().iter().find(|buffer| buffer.id() == id) else { return; };
                let Some(file) = &buffer.disk_file else { return; };
                let path = file.path().to_owned();
                let codepage = buffer.byte_code_page();
                if self.monitoring.automatic.options().update_silently && !buffer.is_modified() {
                    self.file_busy = true;
                    let executor = cx.background_executor().clone();
                    cx.spawn_in(window,async move |view,cx| {
                        let loaded = executor.spawn(async move { crate::files::DiskFile::load_with_code_page(&path,codepage) }).await;
                        if let Err(error) = view.update_in(cx,|view,window,cx| {
                            view.file_busy = false;
                            let result = loaded.and_then(|(file,text)| view.workspace.reload_buffer(id,file,&text,crate::workspace::CloseDisposition::IfUnmodified).map_err(|error| io::Error::other(error.to_string())));
                            match result {
                                Ok(()) => {
                                    if view.workspace.active().id() == id {
                                        if view.monitoring.automatic.options().go_to_end {
                                            let result = view.editor_mut().move_to_document_edge(true,false);
                                            view.editor_changed(result,window,cx);
                                        }
                                        view.reset_active_view(window,cx);
                                    }
                                    view.synchronize_recovery(cx);
                                }
                                Err(error) => view.notify_editor_error(format!("自动更新文件失败: {error}"),cx),
                            }
                            cx.notify();
                        }) { eprintln!("automatic reload window unavailable: {error}"); }
                    }).detach();
                } else {
                    let previous = file.original_modified();
                    if self.activate_buffer(id,window,cx) {
                        self.preferences.files.auto_reload_follow = Some((id,previous));
                        self.request_reload(window,cx);
                    }
                }
            }
            AutoChange::Deleted(id) => {
                let Some(buffer) = self.workspace.buffers().iter().find(|buffer| buffer.id() == id) else { return; };
                if buffer.disk_file.as_ref().is_some_and(crate::files::DiskFile::is_placeholder) { return; }
                let message = format!("{} 已从磁盘删除。是否保留当前文档？",buffer.name());
                self.preferences.files.auto_prompt_pending = true;
                let prompt = window.prompt(gpui::PromptLevel::Warning,"文件不存在",Some(&message),&[gpui::PromptButton::ok("是"),gpui::PromptButton::cancel("否")],cx);
                cx.spawn_in(window,async move |view,cx| {
                    let response = prompt.await;
                    if let Err(error) = view.update_in(cx,|view,window,cx| {
                        view.preferences.files.auto_prompt_pending = false;
                        match response {
                            Ok(0) => view.workspace.mark_deleted_file(id),
                            Ok(_) => {
                                if let Err(error) = view.workspace.close_buffer(id,crate::workspace::CloseDisposition::DiscardConfirmed) { view.notify_editor_error(error.to_string(),cx); }
                                view.reset_active_view(window,cx);
                            }
                            Err(error) => view.notify_editor_error(format!("文件删除确认失败: {error}"),cx),
                        }
                        view.synchronize_recovery(cx);
                        cx.notify();
                    }) { eprintln!("deleted file prompt window unavailable: {error}"); }
                }).detach();
            }
        }
    }
}
