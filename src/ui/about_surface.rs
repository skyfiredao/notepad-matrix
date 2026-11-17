use super::*;
use super::dialog_font::px;

impl MatrixWindow {
    pub(super) fn show_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.about_visible = true;
        self.present_floating(floating_windows::DialogKind::About, window, cx);
        cx.notify();
    }

    pub(super) fn render_about(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.about_visible {
            return None;
        }
        Some(
            div()
                .id("about-dialog")
                .debug_selector(|| "about-dialog".into())
                .role(Role::Dialog)
                .aria_label("About Notepad Matrix")
                .track_focus(&self.close_focus)
                .w_full()
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(div().text_size(px(20.)).child("Notepad Matrix"))
                .child(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .child("An independent Rust + GPUI editor for macOS. Not an official Notepad++ release.")
                .child("Notepad++ artwork: copyright Don HO and contributors. GNU GPL version 3 and upstream terms.")
                .child("Artwork license: assets/notepad-plus-plus/LICENSE. Distributed without warranty.")
                .child(
                    div()
                        .id("about-close")
                        .debug_selector(|| "about-close".into())
                        .role(Role::Button)
                        .aria_label("Close About")
                        .border_1()
                        .border_color(rgb(0x808080))
                        .px(px(12.))
                        .py(px(4.))
                        .child("Close")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dismiss_floating(floating_windows::DialogKind::About, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}
