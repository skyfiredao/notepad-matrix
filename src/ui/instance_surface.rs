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
        match options.open_into(&mut self.workspace) {
            Ok(warnings) => {
                self.reset_active_view(window, cx);
                if options.line.is_some() || options.column.is_some() {
                    self.editor_follow_caret = true;
                }
                if !warnings.is_empty() {
                    self.notify_editor_error(warnings.join("\n"), cx);
                }
            }
            Err(error) => self
                .notify_editor_error(format!("Launch documents could not be opened: {error}"), cx),
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
        let launched = std::env::current_exe().and_then(|executable| options.spawn(&executable));
        match launched {
            Ok(child) => {
                let executor = cx.background_executor().clone();
                cx.spawn(async move |_, _| {
                    if let Err(error) = crate::run_commands::reap(child, || {
                        executor.timer(std::time::Duration::from_secs(1))
                    })
                    .await
                    {
                        eprintln!("new instance could not be reaped: {error}");
                    }
                })
                .detach();
                if moving {
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
                    if let Err(error) = view.update_in(cx, |view, window, cx| {
                        view.file_busy = false;
                        if let Err(error) = answered {
                            view.notify_editor_error(
                                format!("Launch error prompt disconnected: {error}"),
                                cx,
                            );
                        }
                        if moving {
                            view.finish_instance_move(pane, buffer, window, cx);
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

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn transferred_startup_opens_the_saved_document_at_source_position(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated startup");
        let path = home.path().join("source name.cpp");
        std::fs::write(&path, "one\r\n\t中two").expect("source bytes");
        let options = LaunchOptions::parse([
            std::ffi::OsString::from("-multiInst"),
            "-nosession".into(),
            "-lcpp".into(),
            "-n2".into(),
            "-c6".into(),
            path.into_os_string(),
        ])
        .expect("source launch arguments");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.apply_launch_options(&options, window, cx)
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.buffers().len(), 1);
            assert_eq!(view.editor().committed_text(), "one\r\n\t中two");
            assert_eq!(view.editor().selection().head.0, 9);
            assert_eq!(
                view.workspace.active().language(),
                crate::syntax::Language::Builtin(3)
            );
        });
        view.update_in(cx, |view, window, cx| {
            view.clone_to_other_view(window, cx);
            let source = view.workspace.active_view();
            let buffer = view.workspace.active().id();
            view.finish_instance_move(source, buffer, window, cx);
            assert!(!view.workspace.view_buffer_ids(source).contains(&buffer));
            assert!(
                view.workspace
                    .view_buffer_ids(source.other())
                    .contains(&buffer)
            );
            assert_eq!(view.editor().committed_text(), "one\r\n\t中two");
        });
        assert_eq!(cx.windows().len(), 1);
    }
}
