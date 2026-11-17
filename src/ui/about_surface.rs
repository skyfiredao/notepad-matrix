use super::*;
use super::dialog_font::px;

pub(super) fn dimensions() -> gpui::Size<gpui::Pixels> {
    size(px(420.), px(385.))
}

fn source_rect(x: f32, y: f32, width: f32, height: f32) -> gpui::Div {
    div()
        .absolute()
        .left(px(x * 1.75))
        .top(px(y * 1.75))
        .w(px(width * 1.75))
        .h(px(height * 1.75))
}

fn about_logo(window: &Window) -> std::sync::Arc<gpui::Image> {
    macro_rules! image {
        ($path:literal) => {{
            static IMAGE: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
            IMAGE.get_or_init(|| std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png, include_bytes!($path).to_vec(),
            ))).clone()
        }};
    }
    let pixels = f32::from(px(128.)) * window.scale_factor();
    if pixels <= 128. {
        image!("../../assets/notepad-matrix-about-128.png")
    } else if pixels <= 256. {
        image!("../../assets/notepad-matrix-about-256.png")
    } else {
        image!("../../assets/notepad-matrix-about-768.png")
    }
}

impl MatrixWindow {
    pub(super) fn show_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.show_about(window, cx)) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if !self.open_matrix(window, cx) {
            return;
        }
        self.about_visible = true;
        self.present_floating(floating_windows::DialogKind::About, window, cx);
        cx.notify();
    }

    pub(super) fn render_about(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.about_visible {
            return None;
        }
        self.start_matrix_about_timer(cx);
        let dimensions = dimensions();
        let close_label = match self.matrix_about_seconds() {
            Some(seconds) if self.native_locale().starts_with("chinese") => format!("关闭（{seconds}秒）"),
            Some(seconds) => format!("Close ({seconds}s)"),
            None => "OK".to_owned(),
        };
        Some(
            div()
                .id("about-dialog")
                .debug_selector(|| "about-dialog".into())
                .role(Role::Dialog)
                .aria_label("About Notepad Matrix")
                .relative()
                .w(dimensions.width)
                .h(dimensions.height)
                .flex_shrink_0()
                .bg(rgb(0xf0f0f0))
                .text_color(rgb(0x202020))
                .font_family(dialog_font::font_family())
                .text_size(px(13.))
                .line_height(px(17.5))
                .child(
                    div()
                        .absolute()
                        .left(px(12.25))
                        .top(px(8.75))
                        .size(px(128.))
                        .child(gpui::img(about_logo(window)).size_full()),
                )
                .child(source_rect(90., 25., 138., 11.).child("Notepad Matrix"))
                .child(source_rect(90., 38., 138., 11.)
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION"))))
                .child(
                    source_rect(12., 85., 216., 12.)
                        .id("about-project")
                        .role(Role::Link)
                        .aria_label("Notepad Matrix project")
                        .text_color(rgb(0x0067c0))
                        .cursor_pointer()
                        .child("https://github.com/skyfiredao/notepad-matrix")
                        .on_click(|_, _, cx| {
                            cx.open_url("https://github.com/skyfiredao/notepad-matrix");
                        }),
                )
                .child(source_rect(12., 106., 216., 87.)
                    .border_1().border_color(rgb(0xaaaaaa)))
                .child(source_rect(18., 101., 204., 11.)
                    .bg(rgb(0xf0f0f0)).text_center().child("GNU General Public License"))
                .child(
                    source_rect(18., 118., 204., 42.)
                        .id("about-notice")
                        .overflow_y_scroll()
                        .child("A Rust + GPUI text editor for macOS.\n\nGNU GPL v3. You may redistribute.\nDistributed without warranty."),
                )
                .child(
                    source_rect(18., 162., 204., 12.)
                        .id("about-licenses")
                        .role(Role::Link)
                        .aria_label("Licenses and third-party notices")
                        .text_color(rgb(0x0067c0))
                        .cursor_pointer()
                        .child(if self.native_locale().starts_with("chinese") {
                            "许可证与第三方声明"
                        } else {
                            "Licenses and third-party notices"
                        })
                        .on_click(|_, _, cx| {
                            cx.open_url("https://github.com/skyfiredao/notepad-matrix#license");
                        }),
                )
                .child(
                    source_rect(95., 200., 50., 14.)
                        .id("about-close")
                        .debug_selector(|| "about-close".into())
                        .role(Role::Button)
                        .aria_label(close_label.clone())
                        .track_focus(&self.close_focus)
                        .border_1()
                        .border_color(rgb(0x0067c0))
                        .bg(rgb(0xe1e1e1))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|style| style.bg(rgb(0xe5f1fb)))
                        .child(close_label)
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dismiss_floating(floating_windows::DialogKind::About, window, cx);
                        }))
                        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                view.dismiss_floating(floating_windows::DialogKind::About, window, cx);
                                cx.stop_propagation();
                            }
                        })),
                )
                .into_any_element(),
        )
    }
}
