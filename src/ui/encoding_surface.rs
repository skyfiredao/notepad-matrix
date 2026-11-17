use super::*;
use crate::files::DiskFile;
use crate::files::{ByteFormatCodePage, UnicodeFormat, Utf8Format};

#[derive(Default)]
pub(super) struct EncodingState {
    pending_save: Option<(u32, crate::workspace::BufferId)>,
}

impl MatrixWindow {
    pub(super) fn source_encoding_checked(&self, id: u32) -> Option<bool> {
        if !crate::codepages::is_encoding_command(id) {
            return None;
        }
        let buffer = self.workspace.active();
        if let Some(entry) = crate::codepages::codepage(id) {
            return Some(buffer.byte_code_page() == Some(ByteFormatCodePage::CodePage(entry.codepage)));
        }
        Some(id < 45009 && buffer.byte_code_page().is_none()
            && crate::codepages::unicode_format(id) == Some(buffer.format()))
    }

    pub(super) fn source_encoding_enabled(&self, id: u32) -> Option<bool> {
        crate::codepages::is_encoding_command(id).then_some(!matches!(id, 45004 | 45009))
    }

    pub(super) fn select_source_encoding(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !crate::codepages::is_encoding_command(id) {
            return false;
        }
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.select_source_encoding(id, window, cx);
        }) {
            return true;
        }
        if matches!(id, 45004 | 45009) || self.file_busy || self.close_dialog
            || self.save_all_dialog || self.batch_editing_blocked()
            || self.reload_target.is_some() || self.file_open_sequence_pending()
            || self.encoding_state.pending_save.is_some() || self.macro_interaction_blocked()
        {
            return true;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if matches!(id, 45010..=45013) {
            if let Some(format) = crate::codepages::unicode_format(id) {
                self.workspace.active_mut().convert_format(format);
                self.synchronize_recovery(cx);
                cx.notify();
            }
            return true;
        }
        if crate::codepages::codepage(id).is_none() && self.workspace.active().byte_code_page().is_none() {
            if let Some(format) = crate::codepages::unicode_format(id) {
                self.workspace.active_mut().select_format(format);
                self.synchronize_recovery(cx);
                cx.notify();
            }
            return true;
        }
        if let Some(entry) = crate::codepages::codepage(id) {
            if let Err(error) = ByteFormatCodePage::CodePage(entry.codepage).encode("") {
                self.notify_editor_error(format!("无法选择编码：{error}"), cx);
                return true;
            }
        }
        if self.workspace.active().is_modified() {
            self.confirm_encoding_reload(id, true, window, cx);
        } else if self.editor().document().can_undo() {
            self.confirm_encoding_reload(id, false, window, cx);
        } else {
            self.reload_source_encoding(id, window, cx);
        }
        true
    }

    fn confirm_encoding_reload(&mut self, id: u32, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let buffer_id = self.workspace.active().id();
        let response = window.prompt(
            gpui::PromptLevel::Warning,
            if save { "保存当前修改" } else { "失去撤销能力警告" },
            Some("您应该保存当前的修改。\n所有已保存的修改将无法撤销。\n\n是否继续？"),
            &[gpui::PromptButton::ok("是"), gpui::PromptButton::cancel("否")],
            cx,
        );
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let answer = response.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match answer {
                    Ok(0) if view.workspace.active().id() == buffer_id => {
                        if save {
                            view.encoding_state.pending_save = Some((id, buffer_id));
                            view.start_save(false, window, cx);
                            if !view.file_busy {
                                view.encoding_state.pending_save = None;
                            }
                        } else if !view.workspace.active().is_modified() {
                            view.reload_source_encoding(id, window, cx);
                        }
                    }
                    Ok(_) => {},
                    Err(error) => view.notify_editor_error(format!("编码确认失败：{error}"), cx),
                }
                cx.notify();
            }) {
                eprintln!("encoding confirmation lost its window: {error}");
            }
        }).detach();
    }

    pub(super) fn encoding_save_finished(&mut self, buffer_id: crate::workspace::BufferId, committed: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((id, pending_buffer)) = self.encoding_state.pending_save else {
            return false;
        };
        if pending_buffer != buffer_id {
            return false;
        }
        self.encoding_state.pending_save = None;
        if committed && self.workspace.active().id() == buffer_id && !self.workspace.active().is_modified() {
            self.editor_mut().clear_history();
            self.reload_source_encoding(id, window, cx);
        } else {
            self.editor_focus.focus(window, cx);
            cx.notify();
        }
        true
    }

    fn reload_source_encoding(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let buffer_id = self.workspace.active().id();
        if self.workspace.active().is_modified() {
            return;
        }
        let codepage = crate::codepages::codepage(id).map(|entry| ByteFormatCodePage::CodePage(entry.codepage));
        let Some(file) = self.disk_file() else {
            let format = crate::codepages::unicode_format(id).unwrap_or(UnicodeFormat::Utf8(Utf8Format::Plain));
            self.workspace.active_mut().select_source_byte_format(format, codepage);
            self.synchronize_recovery(cx);
            cx.notify();
            return;
        };
        let path = file.path().to_path_buf();
        self.file_busy = true;
        let executor = cx.background_executor().clone();
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let loaded = executor.spawn(async move {
                DiskFile::load_selected_encoding(&path, codepage)
            }).await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                if view.workspace.active().id() != buffer_id {
                    view.notify_editor_error("编码切换已取消：当前文档已改变".into(), cx);
                    return;
                }
                let result = loaded.and_then(|(file, bytes)| {
                    if !view.cancel_view_composition(window, cx) {
                        return Err(io::Error::other("无法在切换编码前结束输入法组合"));
                    }
                    view.workspace.active_mut().reload_selected_encoding(file, bytes)
                        .map_err(|error| io::Error::other(error.to_string()))
                });
                match result {
                    Ok(()) => {
                        view.initialize_active_change_history();
                        let top_line = view.editor_top_line;
                        view.reset_active_view(window, cx);
                        view.preferences_apply_large_file_open_policy(window, cx);
                        view.editor_top_line = top_line;
                        view.editor_follow_caret = false;
                        view.synchronize_recovery(cx);
                    }
                    Err(error) => view.notify_editor_error(format!("编码切换失败：{error}"), cx),
                }
                view.editor_focus.focus(window, cx);
                cx.notify();
            }) {
                eprintln!("encoding reload lost its window: {error}");
            }
        }).detach();
    }
}
