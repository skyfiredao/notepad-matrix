use super::*;

#[derive(Clone, Copy)]
pub(super) enum StatusMenu {
    Language,
    Eol,
    Encoding,
}

impl MatrixWindow {
    pub(super) fn open_status_menu(
        &mut self,
        target: StatusMenu,
        position: gpui::Point<gpui::Pixels>,
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
        let event = match target {
            StatusMenu::Eol => MenuEvent::OpenSubmenu("edit-menu-eol"),
            StatusMenu::Language | StatusMenu::Encoding => {
                let selector = match target {
                    StatusMenu::Language => "language-menu-trigger",
                    _ => "encoding-menu-trigger",
                };
                let Some(root) = self
                    .menu_bar
                    .definitions()
                    .iter()
                    .position(|definition| definition.selector == selector)
                else {
                    return;
                };
                MenuEvent::OpenRoot(root)
            }
        };
        self.menu_input(event, window, cx);
        if self.menu_bar.mode() == MenuMode::Open {
            self.menu_anchor = Some(position);
            self.menu_submenu_only = matches!(target, StatusMenu::Eol);
            cx.notify();
        }
    }

    pub(super) fn render_status(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = |value: usize| {
            let digits = value.to_string();
            let mut result = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    result.push(',');
                }
                result.push(digit);
            }
            result
        };
        let snapshot = self.editor().display_byte_snapshot();
        let (size_text, position_text) =
            match self.editor().source_selections().and_then(|selections| crate::status::measure_bytes(&snapshot, selections.primary().selection, 4)) {
                Ok(status) => {
                    let selection = if status.selected_chars == 0 {
                        format!("位置: {}", count(status.position))
                    } else {
                        format!(
                            "选中: {} | {}",
                            count(status.selected_chars),
                            count(status.selected_lines)
                        )
                    };
                    (
                        format!(
                            "长度: {}    行数: {}",
                            count(status.bytes),
                            count(status.lines)
                        ),
                        format!(
                            "行: {}    列: {}    {}",
                            count(status.line),
                            count(status.column),
                            selection
                        ),
                    )
                }
                Err(error) => ("Status unavailable".into(), error.to_string()),
            };
        let format = match self.workspace.active().format() {
            crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain) => "UTF-8",
            crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Bom) => "UTF-8-BOM",
            crate::files::UnicodeFormat::Utf16LeBom => "UTF-16 LE BOM",
            crate::files::UnicodeFormat::Utf16BeBom => "UTF-16 BE BOM",
        };
        let eol = match self.editor().line_ending() {
            crate::input::LineEnding::CrLf => "Windows (CR LF)",
            crate::input::LineEnding::Lf => "Unix (LF)",
            crate::input::LineEnding::Cr => "Macintosh (CR)",
        };
        let typing_mode = if self.editor().overwrite() {
            "OVR"
        } else {
            "INS"
        };
        let panel = |id: &'static str,
                     label: String,
                     text: String,
                     action: Option<Box<dyn gpui::Action>>| {
            let interactive = action.is_some();
            let context_menu = match id {
                "status-language" => Some(StatusMenu::Language),
                "status-eol" => Some(StatusMenu::Eol),
                "status-encoding" => Some(StatusMenu::Encoding),
                _ => None,
            };
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Status)
                .aria_label(label)
                .when(interactive, |panel| {
                    panel.aria_description(if id == "status-insert-mode" {
                        "Click to toggle insert/overwrite mode"
                    } else {
                        "Double-click to open"
                    })
                })
                .flex_shrink_0()
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .px(px(2.))
                .when(id == "status-language", |panel| {
                    panel.flex_basis(px(0.)).flex_grow(1.).min_w(px(0.))
                })
                .when(id == "status-document-size", |panel| panel.w(px(220.)))
                .when(id == "status-position", |panel| panel.w(px(260.)))
                .when(id == "status-eol", |panel| panel.w(px(110.)))
                .when(id == "status-encoding", |panel| panel.w(px(120.)))
                .when(id == "status-insert-mode", |panel| panel.w(px(45.)))
                .border_l_1()
                .border_color(rgb(if self.dark { 0x646464 } else { 0xa0a0a0 }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseUpEvent, window, cx| {
                        if let Some(target) = context_menu {
                            view.open_status_menu(target, event.position, window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_click(
                    cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                        let clicks = if id == "status-insert-mode" { 1 } else { 2 };
                        if event.click_count() == clicks {
                            if let Some(target) = context_menu {
                                view.open_status_menu(target, event.position(), window, cx);
                            } else if let Some(action) = &action {
                                window.dispatch_action(action.boxed_clone(), cx);
                            }
                        }
                        cx.stop_propagation();
                    }),
                )
                .child(text)
        };
        div()
            .id("status-bar")
            .debug_selector(|| "status-bar".into())
            .role(Role::Status)
            .aria_label("Status bar")
            .h(px(24.))
            .flex_shrink_0()
            .flex()
            .justify_end()
            .items_center()
            .overflow_hidden()
            .text_size(px(13.))
            .text_color(rgb(if self.dark { 0xe0e0e0 } else { 0x000000 }))
            .bg(rgb(if self.dark { 0x2b2b2b } else { 0xf0f0f0 }))
            .border_t_1()
            .border_color(rgb(if self.dark { 0x646464 } else { 0xa0a0a0 }))
            .child(panel(
                "status-language",
                self.workspace.active().language().description().into(),
                self.workspace.active().language().description().into(),
                Some(Box::new(OpenLanguageMenu)),
            ))
            .child(panel(
                "status-document-size",
                size_text.clone(),
                size_text,
                Some(Box::new(ShowSummary)),
            ))
            .child(panel(
                "status-position",
                position_text.clone(),
                position_text,
                Some(Box::new(ShowGoTo)),
            ))
            .child(panel(
                "status-eol",
                format!("Line endings: {eol}"),
                eol.into(),
                Some(Box::new(OpenEolMenu)),
            ))
            .child(panel(
                "status-encoding",
                format!("Encoding: {format}"),
                format.into(),
                Some(Box::new(OpenEncodingMenu)),
            ))
            .child(panel(
                "status-insert-mode",
                format!("Typing mode: {typing_mode}"),
                typing_mode.into(),
                Some(Box::new(ToggleOverwrite)),
            ))
            .child(
                div()
                    .id("status-resize-indicator")
                    .w(px(24.))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .items_end()
                    .justify_end()
                    .text_color(rgb(0x909090))
                    .child("◢"),
            )
            .into_any_element()
    }
}

