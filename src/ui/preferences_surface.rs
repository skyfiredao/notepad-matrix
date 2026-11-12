use super::*;

impl MatrixWindow {
    pub(super) fn show_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.preferences_visible = true;
        self.present_floating(floating_windows::DialogKind::Preferences, window, cx);
        cx.notify();
    }

    fn toggle_statusbar(&mut self, cx: &mut Context<Self>) {
        let visible = !self.status_visible;
        if let Some(store) = &self.settings_store
            && let Err(error) = store.update_settings(|settings| {
                settings["statusbar_visible"] = visible.into();
                Ok(())
            })
        {
            self.notify_editor_error(format!("Preferences could not be saved: {error}"), cx);
            return;
        }
        self.status_visible = visible;
        cx.notify();
    }

    fn toggle_tab_preview(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.tab_preview_enabled();
        if let Some(store) = &self.settings_store
            && let Err(error) = store.update_settings(|settings| {
                settings["docPeekOnTab"] = enabled.into();
                Ok(())
            })
        {
            self.notify_editor_error(format!("Preferences could not be saved: {error}"), cx);
            return;
        }
        self.set_tab_preview_enabled(enabled, cx);
    }

    fn toggle_auto_insert(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((key, _)) = crate::intelligence::auto_insert::OPTIONS.get(index) else {
            return;
        };
        let enabled = !self.auto_insert_flag(index);
        if let Some(store) = &self.settings_store
            && let Err(error) = store.update_settings(|settings| {
                if !settings["auto-insert"].is_null() && !settings["auto-insert"].is_object() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "auto-insert must be an object",
                    ));
                }
                settings["auto-insert"][*key] = enabled.into();
                Ok(())
            })
        {
            self.notify_editor_error(format!("Preferences could not be saved: {error}"), cx);
            return;
        }
        self.set_auto_insert_flag(index, enabled);
        cx.notify();
    }

    fn select_auto_indent(&mut self, mode: crate::indentation::Mode, cx: &mut Context<Self>) {
        if let Some(store) = &self.settings_store
            && let Err(error) = store.update_settings(|settings| {
                settings["maintain_indent"] = mode.value().into();
                Ok(())
            })
        {
            self.notify_editor_error(format!("Preferences could not be saved: {error}"), cx);
            return;
        }
        self.set_auto_indent_mode(mode);
        cx.notify();
    }

    pub(super) fn render_preferences(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.preferences_visible {
            return None;
        }
        Some(
            div()
                .id("preferences-dialog")
                .debug_selector(|| "preferences-dialog".into())
                .role(Role::Dialog)
                .aria_label("Preferences")
                .track_focus(&self.preferences_focus)
                .key_context("MatrixPreferences")
                .w_full()
                .max_w_full()
                .p(px(12.))
                .bg(rgb(0xf0f0f0))
                .text_color(rgb(0x202020))
                .border_1()
                .border_color(rgb(0x606060))
                .flex()
                .flex_col()
                .gap(px(12.))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child("Preferences — General / Auto-Completion")
                .child(
                    div()
                        .id("preferences-tab-preview")
                        .debug_selector(|| "preferences-tab-preview".into())
                        .role(Role::CheckBox)
                        .aria_label("Enable document peek on tab")
                        .aria_toggled(self.tab_preview_enabled().into())
                        .child(if self.tab_preview_enabled() {
                            "☑ Enable document peek on tab"
                        } else {
                            "☐ Enable document peek on tab"
                        })
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_tab_preview(cx))),
                )
                .child(
                    div()
                        .id("preferences-hide-statusbar")
                        .debug_selector(|| "preferences-hide-statusbar".into())
                        .role(Role::CheckBox)
                        .aria_label("Hide status bar")
                        .aria_toggled((!self.status_visible).into())
                        .child(if self.status_visible {
                            "☐ Hide status bar"
                        } else {
                            "☑ Hide status bar"
                        })
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_statusbar(cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child("Auto-insert matched characters")
                        .children(
                            crate::intelligence::auto_insert::OPTIONS
                                .into_iter()
                                .enumerate()
                                .map(|(index, (key, label))| {
                                    let enabled = self.auto_insert_flag(index);
                                    div()
                                        .id(format!("preferences-auto-insert-{key}"))
                                        .debug_selector(move || {
                                            format!("preferences-auto-insert-{key}")
                                        })
                                        .role(Role::CheckBox)
                                        .aria_label(label)
                                        .aria_toggled(enabled.into())
                                        .child(format!(
                                            "{} {label}",
                                            if enabled { "☑" } else { "☐" }
                                        ))
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.toggle_auto_insert(index, cx)
                                        }))
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child("Auto-indent")
                        .children(
                            [
                                (crate::indentation::Mode::None, "None"),
                                (crate::indentation::Mode::Basic, "Basic"),
                                (crate::indentation::Mode::Advanced, "Advanced"),
                            ]
                            .into_iter()
                            .map(|(mode, label)| {
                                let selected = self.auto_indent_mode() == mode;
                                div()
                                    .id(format!("preferences-auto-indent-{}", mode.value()))
                                    .role(Role::RadioButton)
                                    .aria_label(label)
                                    .aria_toggled(selected.into())
                                    .child(format!("{} {label}", if selected { "●" } else { "○" }))
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.select_auto_indent(mode, cx)
                                    }))
                            }),
                        ),
                )
                .child(
                    div()
                        .id("preferences-close")
                        .role(Role::Button)
                        .aria_label("Close Preferences")
                        .border_1()
                        .border_color(rgb(0x808080))
                        .px(px(12.))
                        .py(px(4.))
                        .child("Close")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dismiss_floating(
                                floating_windows::DialogKind::Preferences,
                                window,
                                cx,
                            );
                        })),
                )
                .into_any_element(),
        )
    }
}
