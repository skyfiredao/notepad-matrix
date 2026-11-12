use super::*;

impl MatrixWindow {
    pub(super) fn converter_transform(
        &mut self,
        operation: crate::converter::ConverterOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.converter_enabled
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
                "Converter during active composition is not implemented yet".into(),
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
        match crate::converter::transform(&selected, operation, self.editor().line_ending()) {
            Ok(text) => {
                let result = self.editor_mut().replace_converter_selection(&text);
                self.editor_changed(result, window, cx);
                self.editor_focus.focus(window, cx);
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }
}
