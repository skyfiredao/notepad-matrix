use super::*;

impl MatrixWindow {
    pub(super) fn move_run_focus(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.run_visible
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let current = if self.run_button_focus[0].is_focused(window) {
            1
        } else if self.run_button_focus[1].is_focused(window) {
            2
        } else {
            0
        };
        let next = if forward {
            (current + 1) % 3
        } else {
            (current + 2) % 3
        };
        if next == 0 {
            self.run_input
                .update(cx, |field, cx| field.focus(window, cx));
        } else {
            self.run_button_focus[next - 1].focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn activate_run_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run_button_focus[1].is_focused(window) {
            self.cancel_run(window, cx);
        } else {
            self.confirm_run(window, cx);
        }
    }

    pub(super) fn show_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().has_composition() {
            self.notify_editor_error(
                "Run during active document composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.run_visible = true;
        self.run_message.clear();
        self.present_floating(floating_windows::DialogKind::Run, window, cx);
        cx.notify();
    }

    pub(super) fn cancel_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.run_visible {
            return;
        }
        self.dismiss_floating(floating_windows::DialogKind::Run, window, cx);
    }

    pub(super) fn confirm_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.confirm_run(window, cx)) {
            return;
        }
        if !self.run_visible
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.run_input.read(cx).has_composition() {
            self.run_message = "Commit the Run input composition before launching a command".into();
            cx.notify();
            return;
        }
        let command = self.run_input.read(cx).text();
        // RunDlg::Command::run defaults to '.', not the active document directory.
        // Standard-terminal-in-document-directory is a separate retained command.
        let launch = || crate::run_commands::launch_terminal(&command, std::path::Path::new("."));
        #[cfg(feature = "native-input-tests")]
        let launched = if cx.is_test() {
            crate::run_commands::launch_terminal_in_test(&command, std::path::Path::new("."))
        } else {
            launch()
        };
        #[cfg(not(feature = "native-input-tests"))]
        let launched = launch();
        match launched {
            Ok(child) => {
                let executor = cx.background_executor().clone();
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = crate::run_commands::reap(child, || {
                            executor.timer(std::time::Duration::from_millis(100))
                        })
                        .await
                        {
                            eprintln!("Run child reaping failed: {error}");
                        }
                    })
                    .detach();
                self.cancel_run(window, cx);
            }
            Err(error) => {
                self.run_message = format!("Run failed: {error}");
                cx.notify();
            }
        }
    }

    pub(super) fn render_run(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.run_visible {
            return None;
        }
        let button = |id: &'static str, text: &'static str, index: usize| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(text)
                .track_focus(&self.run_button_focus[index])
                .key_context("MatrixRunButton")
                .px(px(12.))
                .py(px(4.))
                .border_1()
                .border_color(rgb(0x808080))
                .when(self.run_button_focus[index].is_focused(window), |button| {
                    button.border_color(rgb(0x0067c0))
                })
                .child(text)
        };
        Some(
            div()
                .id("run-dialog")
                .debug_selector(|| "run-dialog".into())
                .role(Role::Dialog)
                .aria_label("Run...")
                .key_context("MatrixRun")
                .w_full()
                .max_w_full()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 }))
                .border_1()
                .border_color(rgb(0x606060))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child("The Program to Run")
                .child("POSIX command — runs visibly in macOS Terminal")
                .child(self.run_input.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(button("run-confirm", "Run", 0).on_click(
                            cx.listener(|view, _, window, cx| view.confirm_run(window, cx)),
                        ))
                        .child(button("run-cancel", "Cancel Run", 1).on_click(
                            cx.listener(|view, _, window, cx| view.cancel_run(window, cx)),
                        )),
                )
                .when(!self.run_message.is_empty(), |dialog| {
                    dialog.child(
                        div()
                            .id("run-error")
                            .debug_selector(|| "run-error".into())
                            .role(Role::Alert)
                            .aria_label(self.run_message.clone())
                            .child(self.run_message.clone()),
                    )
                })
                .into_any_element(),
        )
    }
}
