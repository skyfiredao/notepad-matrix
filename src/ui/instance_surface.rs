use super::*;
use crate::launch::LaunchOptions;
use crate::workspace::{BufferId, CloseDisposition, ViewId};

impl MatrixWindow {
    pub(super) fn apply_launch_options(
        &mut self,
        options: &LaunchOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if options.open_session && options.files.len() != 1 {
            self.notify_editor_error("-openSession requires exactly one session path".into(), cx);
            if let Err(error) = options.complete(false) { self.notify_editor_error(error.to_string(), cx); }
            self.complete_file_preferences_startup(window, cx);
            return;
        }
        if !options.files.is_empty() {
            self.start_open_paths_with_launch(options.files.clone(), Some(options.clone()), window, cx);
            return;
        }
        let mut accepted = true;
        match options.resolved_language(&self.workspace) {
            Ok(Some(language)) => self.workspace.active_mut().set_language(language),
            Ok(None) => {},
            Err(error) => { accepted = false; self.notify_editor_error(error.to_string(), cx); },
        }
        self.workspace.refresh_udl_metadata();
        if let Err(error) = self.apply_launch_document_position(options) { accepted = false; self.notify_editor_error(error.to_string(), cx); }
        self.initialize_active_change_history();
        self.reset_active_view(window, cx);
        if options.line.is_some() || options.column.is_some() { self.editor_follow_caret = true; }
        self.complete_file_preferences_startup(window, cx);
        if let Err(error) = options.complete(accepted) { self.notify_editor_error(error.to_string(), cx); }
    }

    pub(super) fn apply_launch_document_position(&mut self, options: &LaunchOptions) -> io::Result<()> {
        if options.line.is_none() && options.column.is_none() { return Ok(()); }
        let tab_width = self.workspace.active_tab_width();
        let document = self.workspace.active().editor.document();
        let current = document.selection().head;
        let line = options.line.map_or_else(|| document.text_snapshot().byte_to_line_idx(current.0, ropey::LineType::LF_CR), |line| line.saturating_sub(1)).min(document.line_count().saturating_sub(1));
        let column = options.column.map_or(0, |column| column.saturating_sub(1));
        let (position, _) = document.position_at_column(crate::editor::ColumnPosition { line, column }, tab_width).map_err(|error| io::Error::other(error.to_string()))?;
        self.workspace.active_mut().editor.select(crate::editor::Selection { anchor: position, head: position }).map_err(|error| io::Error::other(error.to_string()))
    }

    pub(super) fn begin_file_preferences_startup(&mut self, settings: &serde_json::Value, launch: &LaunchOptions, window: &mut Window, cx: &mut Context<Self>) {
        self.file_prompt.startup_settings = Some(settings.clone());
        let loaded_session_path = self.preferences.files.loaded_session_path.clone();
        self.startup_file_preferences(settings, launch, window, cx);
        if launch.open_session { self.preferences.files.loaded_session_path = loaded_session_path; }
        for buffer in self.workspace.buffers() { self.initialize_buffer_change_history(buffer.editor.document()); }
    }

    pub(super) fn complete_file_preferences_startup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(settings) = self.file_prompt.startup_settings.take() {
            self.finish_file_preferences_startup(&settings, window, cx);
            self.initialize_active_change_history();
        }
    }

    fn finish_instance_move(
        &mut self,
        pane: ViewId,
        buffer: BufferId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .workspace
            .close_in_view(pane, buffer, CloseDisposition::IfUnmodified)
        {
            Ok(_) => {
                self.reset_active_view(window, cx);
                if self.workspace.buffers().len() == 1
                    && self.workspace.active().disk_file.is_none()
                    && self.workspace.active().editor.document().len_bytes() == 0
                    && !self.workspace.active().is_modified()
                {
                    self.request_close(window, cx);
                }
            }
            Err(error) => self
                .notify_editor_error(format!("Transferred tab could not be closed: {error}"), cx),
        }
    }

    pub(super) fn open_new_instance(
        &mut self,
        moving: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.file_open_sequence_pending()
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
        {
            return;
        }
        let options = match LaunchOptions::for_transfer(&self.workspace) {
            Ok(Some(options)) => options,
            Ok(None) => return,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        };
        let pane = self.workspace.active_view();
        let buffer = self.workspace.active().id();
        self.record_macro_step(crate::macros::MacroStep::Unsupported(
            "new-instance document command",
        ));
        self.spawn_launch_options_with_move(options, moving.then_some((pane, buffer)), None, window, cx);
    }

    pub(super) fn spawn_launch_options(
        &mut self,
        options: LaunchOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_launch_options_with_move(options, None, None, window, cx);
    }

    pub(super) fn spawn_launch_options_with_completion(
        &mut self,
        options: LaunchOptions,
        completion: Option<std::sync::mpsc::Sender<bool>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_launch_options_with_move(options, None, completion, window, cx);
    }

    fn spawn_launch_options_with_move(
        &mut self,
        options: LaunchOptions,
        moving: Option<(ViewId, BufferId)>,
        completion: Option<std::sync::mpsc::Sender<bool>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::file_access::sandboxed() || completion.is_some() {
            let executor = cx.background_executor().clone();
            self.file_busy = true;
            cx.spawn_in(window, async move |view, cx| {
                let result = match std::env::current_exe() {
                    Ok(executable) => native_app_launch::launch_handoff(options, executable, executor).await,
                    Err(error) => Err(error),
                };
                if let Err(error) = view.update_in(cx, |view, window, cx| {
                    view.file_busy = false;
                    let accepted = result.is_ok();
                    match result {
                        Ok(()) => if let Some((pane, buffer)) = moving { view.finish_instance_move(pane, buffer, window, cx); },
                        Err(error) => view.notify_editor_error(format!("The new instance could not be started: {error}"), cx),
                    }
                    if let Some(completion) = completion
                        && let Err(error) = completion.send(accepted) {
                        eprintln!("session instance completion unavailable: {error}");
                    }
                    cx.notify();
                }) { eprintln!("native application launch lost its document window: {error}"); }
            }).detach();
            return;
        }
        let launched = std::env::current_exe().and_then(|executable| options.spawn(&executable));
        match launched {
            Ok(child) => {
                let executor = cx.background_executor().clone();
                cx.spawn(async move |_, _| {
                    if let Err(error) = crate::child_process::reap(child, || {
                        executor.timer(std::time::Duration::from_secs(1))
                    })
                    .await
                    {
                        eprintln!("new instance could not be reaped: {error}");
                    }
                })
                .detach();
                if let Some((pane, buffer)) = moving {
                    self.finish_instance_move(pane, buffer, window, cx);
                }
            }
            Err(error) => {
                let detail = error.to_string();
                let prompt = window.prompt(
                    gpui::PromptLevel::Critical,
                    "The new instance could not be started",
                    Some(&detail),
                    &["OK"],
                    cx,
                );
                self.file_busy = true;
                cx.spawn_in(window, async move |view, cx| {
                    let answered = prompt.await;
                    if let Err(error) = view.update_in(cx, |view, _, cx| {
                        view.file_busy = false;
                        if let Err(error) = answered {
                            view.notify_editor_error(
                                format!("Launch error prompt disconnected: {error}"),
                                cx,
                            );
                        }
                        cx.notify();
                    }) {
                        eprintln!("launch error lost its document window: {error}");
                    }
                })
                .detach();
            }
        }
    }
}
