use super::*;
use super::dialog_font::px;
use crate::config_directory::{Selection, Snapshot};

pub(super) struct ConfigDirectoryState {
    focus: FocusHandle,
    open: bool,
    cursor: usize,
    error: Option<io::Error>,
}

impl ConfigDirectoryState {
    pub(super) fn new(cx: &mut App) -> Self {
        Self { focus: cx.focus_handle().tab_index(0), open: false, cursor: 0, error: None }
    }

    pub(super) fn close(&mut self) -> bool {
        std::mem::replace(&mut self.open, false)
    }
}

impl MatrixWindow {
    fn apply_config_directory_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        self.config_directory.open = false;
        let result = if index == 0 {
            crate::config_directory::select_automatic()
        } else {
            crate::config_directory::choose_custom_directory(&self.native_locale()).map(|_| ())
        };
        self.config_directory.error = result.err();
        cx.notify();
    }

    pub(super) fn render_config_directory_preferences(&self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = crate::config_directory::snapshot();
        let locale = self.native_locale();
        let text = |id: &'static str, fallback: &'static str| crate::localization::localized_string(&locale, id).unwrap_or(fallback);
        let path_text = |id: &'static str, fallback: &'static str, path: &Path| {
            crate::localization::Text::resource(id, fallback).replace("$PATH$", path.display().to_string()).render(&locale)
        };
        let palette = self.preferences_render_palette();
        let focus = self.config_directory.focus.clone();
        let custom_label = text("ConfigDirectoryOptionCustom", "Custom directory…");
        let automatic_label = match &snapshot {
            Ok(snapshot) if snapshot.sandboxed => text("ConfigDirectoryOptionAutomaticSandbox", "Automatic (sandbox directory)"),
            _ => text("ConfigDirectoryOptionAutomaticUser", "Automatic (~/.notepad-matrix)"),
        };
        let label = match &snapshot {
            Ok(Snapshot { pending: Selection::Custom { .. }, .. }) => custom_label,
            _ => automatic_label,
        };
        let paths = match &snapshot {
            Ok(snapshot) => vec![
                path_text("ConfigDirectoryPathDefault", "Automatic: $PATH$", &snapshot.default_root),
                path_text("ConfigDirectoryPathCurrent", "Current: $PATH$", &snapshot.active_root),
                path_text("ConfigDirectoryPathNext", "Next launch: $PATH$", snapshot.pending_root()),
                path_text("ConfigDirectoryBackupCurrent", "Current backup: $PATH$", &snapshot.active_root.join("backup")),
                path_text("ConfigDirectoryBackupNext", "Next launch backup: $PATH$", &snapshot.pending_root().join("backup")),
            ],
            Err(error) => vec![crate::localization::Text::resource("ConfigDirectoryReadFailed", "Could not read the configuration directory: $ERROR$")
                .replace("$ERROR$", crate::config_directory::error_text(error, &locale)).render(&locale)],
        };
        let selected_index = usize::from(matches!(&snapshot, Ok(Snapshot { pending: Selection::Custom { .. }, .. })));
        let rtl = crate::localization::is_rtl(&locale);
        let details = paths.join("\n");
        let mut content = div().id("config-directory-preferences").w_full().min_w(px(0.)).relative()
            .flex().flex_col().text_color(rgb(palette.text))
            .when(rtl, |element| element.text_right())
            .child(div().id("config-directory-choice").role(Role::ComboBox)
                .aria_label(text("ConfigDirectoryTitle", "Configuration directory")).aria_value(label)
                .tooltip(move |_, cx| icon_tooltip::view(details.clone(), palette.dark, cx))
                .w_full().h(px(21.)).flex_shrink_0().border_1().border_color(rgb(palette.control_edge(true)))
                .bg(rgb(palette.control_background(true))).track_focus(&focus)
                .flex().items_center().justify_between().px(preferences_surface::horizontal_px(4.))
                .when(rtl, |element| element.flex_row_reverse())
                .child(div().min_w(px(0.)).text_ellipsis().child(label)).child("⌄")
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.config_directory.open = !view.config_directory.open;
                    view.config_directory.cursor = selected_index;
                    view.config_directory.focus.focus(window, cx);
                    cx.notify();
                }))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                    match event.keystroke.key.as_str() {
                        "space" | "enter" if view.config_directory.open => {
                            view.apply_config_directory_selection(view.config_directory.cursor, cx);
                        }
                        "space" | "enter" | "down" if !view.config_directory.open => {
                            view.config_directory.open = true;
                            view.config_directory.cursor = selected_index;
                        }
                        "up" | "home" if view.config_directory.open => view.config_directory.cursor = 0,
                        "down" | "end" if view.config_directory.open => view.config_directory.cursor = 1,
                        "escape" => view.config_directory.open = false,
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify();
                })));
        if self.config_directory.open {
            let cursor = self.config_directory.cursor;
            content = content.child(gpui::deferred(div().id("config-directory-options").role(Role::ListBox)
                .absolute().bottom(px(35.)).left(px(0.))
                .aria_label(text("ConfigDirectoryOptions", "Configuration directory options")).w_full().border_1().border_color(rgb(palette.edge))
                .bg(rgb(palette.background)).occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children([automatic_label, custom_label].into_iter().enumerate().map(|(index, label)| {
                    div().id(("config-directory-option", index)).role(Role::ListBoxOption)
                         .aria_label(label).aria_selected(index == selected_index).w_full().h(px(22.)).px(preferences_surface::horizontal_px(4.)).flex().items_center()
                        .when(index == cursor, |element| element.bg(rgb(palette.hot_background)))
                        .hover(move |style| style.bg(rgb(palette.hot_background)))
                        .child(label)
                        .on_click(cx.listener(move |view, _, _, cx| view.apply_config_directory_selection(index, cx)))
                }))).priority(2));
        }
        let failure = self.config_directory.error.as_ref().map(|error|
            crate::localization::Text::resource("ConfigDirectorySetFailed", "Could not set the configuration directory: $ERROR$")
                .replace("$ERROR$", crate::config_directory::error_text(error, &locale)).render(&locale));
        let note = failure.clone().unwrap_or_else(|| text("ConfigDirectoryRestartShort", "Restart required; no automatic migration.").to_owned());
        let note_details = failure.unwrap_or_else(|| text("ConfigDirectoryRestartDetails", "Restart the application to apply this change. Existing configuration will not be moved.").to_owned());
        content = content.child(div().id("config-directory-restart-note").w_full().min_w(px(0.)).h(px(14.)).flex_shrink_0()
            .text_ellipsis().when(self.config_directory.error.is_some(), |element| element.role(Role::Alert))
            .tooltip(move |_, cx| icon_tooltip::view(note_details.clone(), palette.dark, cx)).child(note));
        content.into_any_element()
    }
}
