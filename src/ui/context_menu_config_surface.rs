use super::*;
use crate::context_menu::{ContextMenu, ContextMenuEntry};

impl MatrixWindow {
    pub(super) fn edit_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_interaction_blocked() || self.reload_target.is_some()
            || self.file_busy || self.close_dialog || self.save_all_dialog {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let locale = self.native_locale();
        let title = crate::localization::dialog_title(&locale, "ContextMenuXmlEditWarning").unwrap_or("Editing contextMenu");
        let message = crate::localization::dialog_message(&locale, "ContextMenuXmlEditWarning")
            .unwrap_or("Editing contextMenu.xml allows you to modify the popup context menu in the edit zone.\nRestart Notepad Matrix after modifying contextMenu.xml for the changes to take effect.");
        let response = window.prompt(
            gpui::PromptLevel::Warning,
            title,
            Some(message),
            &[gpui::PromptButton::ok("OK")],
            cx,
        );
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let answered = response.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match answered {
                    Ok(0) => {
                        let path = view.settings_store.as_ref()
                            .ok_or_else(|| io::Error::other("没有可用的私有配置目录"))
                            .and_then(ContextMenu::ensure_file);
                        match path {
                            Ok(path) => view.start_open_paths(vec![path], window, cx),
                            Err(error) => view.notify_editor_error(format!("无法打开 contextMenu.xml：{error}"), cx),
                        }
                    }
                    Ok(_) => view.editor_focus.focus(window, cx),
                    Err(error) => view.notify_editor_error(format!("编辑弹出菜单警告未得到响应：{error}"), cx),
                }
                cx.notify();
            }) {
                eprintln!("context menu warning lost its document window: {error}");
            }
        }).detach();
    }

    pub(super) fn editor_context_menu_entries(&self) -> Vec<ContextMenuEntry> {
        self.context_menu.entries(self.menu_bar.definitions())
    }
}
