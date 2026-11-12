use super::*;
use crate::menu::WindowCommand;

impl MatrixWindow {
    pub(super) fn perform_window_command(
        &mut self,
        command: WindowCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_input(MenuEvent::Dismiss, window, cx);
        match command {
            WindowCommand::Minimize => window.minimize_window(),
            WindowCommand::Zoom => window.zoom_window(),
            WindowCommand::BringAllToFront => {
                let Some(main_thread) = MainThreadMarker::new() else {
                    self.notify_editor_error(
                        "Bringing windows to front requires the main thread".into(),
                        cx,
                    );
                    return;
                };
                objc2_app_kit::NSApplication::sharedApplication(main_thread).arrangeInFront(None);
            }
        }
    }
}
