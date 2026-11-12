use super::*;

impl MatrixWindow {
    pub(super) fn mime_transform(
        &mut self,
        operation: crate::mime::MimeOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.mime_enabled
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let range = self.editor().document().selection().range();
        if range.is_empty() {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.notify_editor_error(
                "MIME transform during active composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        let selected = self
            .editor()
            .document()
            .text_snapshot()
            .slice(range.start.0..range.end.0)
            .to_string();
        match crate::mime::transform(&selected, operation) {
            Ok(text) => {
                let result = self.editor_mut().replace_target_from_selection(&text);
                self.editor_changed(result, window, cx);
                self.editor_focus.focus(window, cx);
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }
}
