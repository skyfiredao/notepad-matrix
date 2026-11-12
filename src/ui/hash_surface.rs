use super::*;

impl MatrixWindow {
    pub(super) fn copy_selection_hash(
        &mut self,
        algorithm: crate::hashes::HashAlgorithm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().has_composition() {
            self.notify_editor_error(
                "Selection hashing during active composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        let range = self.editor().document().selection().range();
        if range.is_empty() {
            return;
        }
        let selected = self
            .editor()
            .document()
            .text_snapshot()
            .slice(range.start.0..range.end.0)
            .to_string();
        if let Some(digest) = crate::hashes::selection_digest(&selected, algorithm) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(digest));
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }
}
