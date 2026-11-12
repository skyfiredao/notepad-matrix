use super::*;
use crate::monitoring::{MonitorUpdate, Monitoring, PollRequest};

#[derive(Default)]
pub(super) struct MonitoringState {
    core: Monitoring,
    task: Option<gpui::Task<()>>,
    running: bool,
}

impl MatrixWindow {
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
            if self.monitoring.core.is_empty() {
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
                        return Some(Vec::new());
                    }
                    let requests = view.monitoring.core.requests(&mut view.workspace);
                    if view.monitoring.core.is_empty() {
                        view.monitoring.running = false;
                        return None;
                    }
                    Some(requests)
                }) {
                    Ok(Some(requests)) => requests,
                    Ok(None) | Err(_) => break,
                };
                if requests.is_empty() {
                    continue;
                }
                let results = executor
                    .spawn(async move {
                        requests
                            .into_iter()
                            .map(PollRequest::poll)
                            .collect::<Vec<_>>()
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
                        for result in results {
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
}
