use super::*;

impl MatrixWindow {
    pub(super) fn confirm_replace_open_documents(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.confirm_replace_open_documents(window, cx)
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if !self.find_controls.confirm_open {
            self.replace_open_documents(window, cx);
            return;
        }
        let response = window.prompt(
            gpui::PromptLevel::Warning,
            "替换确认",
            Some("您是否要在所有打开的文档中替换所有匹配项？"),
            &[
                gpui::PromptButton::ok("确定"),
                gpui::PromptButton::cancel("取消"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            match response.await {
                Ok(0) => if let Err(error) = view.update_in(cx, |view, window, cx| {
                    view.replace_open_documents(window, cx)
                }) { eprintln!("open document replacement confirmation lost its document: {error}"); },
                Ok(_) => {},
                Err(error) => eprintln!("open document replacement confirmation failed: {error}"),
            }
        })
        .detach();
    }

    fn replace_open_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        let replacement = self.captured_search_input(crate::search_history::Field::Replace, cx);
        let options = crate::search::LiteralOptions {
            wrap: true,
            ..self.find_options
        };
        if query.is_empty()
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || self
                .workspace
                .buffers()
                .iter()
                .any(|buffer| buffer.editor.marked_range().is_some())
        {
            return;
        }
        self.record_search_histories(
            &[
                crate::search_history::Field::Find,
                crate::search_history::Field::Replace,
            ],
            window,
            cx,
        );
        let mut count = 0;
        let mut errors = Vec::new();
        for buffer in self.workspace.buffers_mut() {
            if buffer.editor.document().is_read_only() {
                continue;
            }
            match buffer
                .editor
                .replace_all_literal_scoped_with_target(&query, &replacement, options, false, false).result
            {
                Ok(replaced) => count += replaced,
                Err(error) => errors.push(format!("{}: {error}", buffer.name())),
            }
        }
        self.editor_changed(Ok(()), window, cx);
        self.set_find_status(find_surface::FindStatus::Message, format!(
            "替换所有打开文件: 已替换 {count} 个匹配项。{}",
            errors.join("；")
        ));
        cx.notify();
    }

}
