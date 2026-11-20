use super::*;

impl MatrixWindow {
    pub(in crate::ui) fn udl_name_input(&self) -> gpui::Entity<text_field::TextField> {
        self.udl.name.clone()
    }

    pub(in crate::ui) fn toggle_udl_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.udl.styler_visible || self.udl_name_visible() {
            return;
        }
        if self.route_to_document(window, cx, |view, window, cx| {
            view.toggle_udl_dock(window, cx);
        }) {
            return;
        }
        self.udl.docked = !self.udl.docked;
        self.udl.visible = true;
        self.sync_floating_windows(window, cx);
        if self.udl.docked {
            self.udl.focus.focus(window, cx);
        } else {
            self.present_floating(floating_windows::DialogKind::Udl, window, cx);
        }
        cx.notify();
    }

    pub(in crate::ui) fn render_docked_udl(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.udl.visible || !self.udl.docked {
            return None;
        }
        let locale = self.native_locale();
        let content = crate::localization::with_dialog(&locale, "UserDefine", || {
            self.render_udl(window, cx)
        });
        Some(
            div()
                .id("udl-docked-surface")
                .key_context("MatrixAuxiliary")
                .tab_group()
                .w(px(WIDTH) + gpui::px(8.))
                .h_full()
                .min_h(px(0.))
                .flex_shrink_0()
                .flex()
                .bg(rgb(0xf0f0f0))
                .text_color(rgb(0x202020))
                .font_family(dialog_font::font_family())
                .text_size(gpui::px(dialog_font::base_size()))
                .line_height(dialog_font::line_height())
                .on_action(cx.listener(|view, _: &CloseWindow, window, cx| {
                    view.dismiss_floating(floating_windows::DialogKind::Udl, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| {
                    window.focus_next(cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                    window.focus_prev(cx);
                    cx.stop_propagation();
                }))
                .child(div().w(gpui::px(8.)).h_full().flex_shrink_0())
                .child(
                    div()
                        .id("udl-docked-content")
                        .w(px(WIDTH))
                        .h_full()
                        .min_h(px(0.))
                        .overflow_y_scroll()
                        .children(content),
                )
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn toggle_udl_transparency(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.udl.docked || self.udl.styler_visible || self.udl_name_visible() {
            return;
        }
        self.udl.transparent = !self.udl.transparent;
        self.update_udl_window(window);
        cx.notify();
    }

    pub(in crate::ui) fn set_udl_transparency_level(
        &mut self,
        opacity: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.udl.docked || !self.udl.transparent || self.udl.styler_visible || self.udl_name_visible() {
            return;
        }
        self.udl.opacity = opacity.clamp(20, 200);
        self.update_udl_window(window);
        cx.notify();
    }

    pub(in crate::ui) fn update_udl_window(&self, window: &Window) {
        if self.udl.docked
            || !self.floating_windows.get(&floating_windows::DialogKind::Udl)
                .is_some_and(|handle| gpui::AnyWindowHandle::from(*handle) == Window::window_handle(window))
        {
            return;
        }
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            if let Some(host) = view.window() {
                host.setAlphaValue(if self.udl.transparent {
                    f64::from(self.udl.opacity) / 255.
                } else {
                    1.
                });
            }
        }
    }
}
