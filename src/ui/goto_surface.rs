use super::*;

impl MatrixWindow {
    pub(super) fn show_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.notify_editor_error(
                "Go to during active document composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.goto_input.update(cx, |field, cx| {
            field.set_text("", window, cx);
        });
        self.goto_message.clear();
        self.goto_visible = true;
        self.present_floating(floating_windows::DialogKind::GoTo, window, cx);
        cx.notify();
    }

    pub(super) fn confirm_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.confirm_goto(window, cx)) {
            return;
        }
        if !self.goto_visible
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let raw = self.goto_input.read(cx).text();
        let Ok(line) = raw.trim().parse::<i64>() else {
            self.goto_message = "Enter a line number".into();
            cx.notify();
            return;
        };
        let row = usize::try_from(line.saturating_sub(1).max(0)).unwrap_or(usize::MAX);
        self.record_macro_step(crate::macros::MacroStep::GoToLine(row));
        let document = self.editor().document();
        let target = if row >= document.line_count() {
            Ok(ByteOffset(document.len_bytes()))
        } else {
            document.line_start(row)
        };
        let result = target.and_then(|position| {
            self.editor_mut().select(Selection {
                anchor: position,
                head: position,
            })
        });
        if result.is_ok() {
            self.goto_visible = false;
            self.editor_focus.focus(window, cx);
        }
        self.editor_changed(result, window, cx);
    }

    pub(super) fn render_goto(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.goto_visible {
            return None;
        }
        let button = |id: &'static str, text: &'static str| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(text)
                .px(px(12.))
                .py(px(4.))
                .border_1()
                .border_color(rgb(0x808080))
                .bg(rgb(0xfafafa))
                .child(text)
        };
        Some(
            div()
                .id("goto-dialog")
                .debug_selector(|| "goto-dialog".into())
                .role(Role::Dialog)
                .aria_label("Go to")
                .key_context("MatrixGoTo")
                .w_full()
                .max_w_full()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .bg(rgb(0xf0f0f0))
                .text_color(rgb(0x202020))
                .border_1()
                .border_color(rgb(0x606060))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child("Go to line")
                .child(format!("Lines: {}", self.editor().document().line_count()))
                .child(self.goto_input.clone())
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(button("goto-confirm", "Go").on_click(
                            cx.listener(|view, _, window, cx| view.confirm_goto(window, cx)),
                        ))
                        .child(button("goto-cancel", "Cancel Go to").on_click(cx.listener(
                            |view, _, window, cx| {
                                view.dismiss_floating(
                                    floating_windows::DialogKind::GoTo,
                                    window,
                                    cx,
                                );
                            },
                        ))),
                )
                .child(self.goto_message.clone())
                .into_any_element(),
        )
    }
}
