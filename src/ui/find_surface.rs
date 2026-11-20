use super::*;
use super::dialog_font::px;
use crate::editor::EditError;
use crate::localization::Text;

pub(super) const DIALOG_WIDTH: f32 = 719.;
pub(super) const DIALOG_HEIGHT: f32 = 342.;
pub(super) const COMPACT_DIALOG_HEIGHT: f32 = 122.;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum FindStatus {
    #[default]
    NoMessage,
    NotFound,
    Message,
    TopReached,
    EndReached,
    Warning,
}

impl FindStatus {
    pub(super) fn color(self, view: &MatrixWindow) -> gpui::Rgba {
        let (name, fallback) = match self {
            Self::NoMessage => return rgb(0x000000),
            Self::NotFound => ("Find status: Not found", 0xff0000),
            Self::Message => ("Find status: Message", 0x0000ff),
            Self::TopReached | Self::EndReached => ("Find status: Search end reached", 0x008000),
            Self::Warning => return rgb(0xff8000),
        };
        let colour = view.style_colour(name, "fgColor", fallback);
        rgb(if view.dark { dark_status_colour(colour) } else { colour })
    }
}

fn dark_status_colour(colour: u32) -> u32 {
    let red = ((colour >> 16) & 255) as i32;
    let green = ((colour >> 8) & 255) as i32;
    let blue = (colour & 255) as i32;
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    let lightness = ((high + low) * 240 + 255) / 510;
    let (hue, saturation) = if high == low { (160, 0) } else {
        let delta = high - low;
        let denominator = if lightness <= 120 { high + low } else { 510 - high - low };
        let saturation = (denominator / 2 + delta * 240) / denominator;
        let normal = |value| (delta / 2 + (high - value) * 40) / delta;
        let hue = if high == red { normal(blue) - normal(green) } else if high == green { 80 + normal(red) - normal(blue) } else { 160 + normal(green) - normal(red) };
        (hue.rem_euclid(240), saturation)
    };
    let lightness = i32::from((lightness + 50) as u16);
    let saturation = i32::from((saturation - 20) as u16);
    if saturation == 0 { return ((lightness * 255 / 240) as u32 & 255) * 0x010101; }
    let high = if lightness > 120 { saturation + lightness - (saturation * lightness + 120) / 240 } else { ((saturation + 240) * lightness + 120) / 240 };
    let low = lightness * 2 - high;
    let channel = |mut hue: i32| {
        if hue > 240 { hue -= 240; } else if hue < 0 { hue += 240; }
        let value = if hue > 160 { low } else if hue > 40 && hue <= 120 { high } else {
            let hue = if hue > 120 { 160 - hue } else { hue };
            (hue * (high - low) + 20) / 40 + low
        };
        ((value * 255 + 120) / 240) as u32 & 255
    };
    (channel(hue + 80) << 16) | (channel(hue) << 8) | channel(hue - 80)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FindOutcome {
    Unchanged,
    Found,
    NotFound,
    Error,
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
pub(super) struct FindOptionAction(pub u32);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
pub(super) struct FindButtonAction(pub &'static str, pub bool);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
pub(super) struct FindFieldWarning(pub bool);

#[derive(Clone, Copy)]
pub(super) enum FindCheck {
    Backward,
    WholeWord,
    MatchCase,
    Wrap,
}

fn source_label(id: &str, label: &'static str) -> &'static str {
    let source = match id {
        "find-next" => Some(if label.starts_with('▼') { 1723 } else { 1 }),
        "find-previous" => Some(1721),
        "find-close" | "files-close" => Some(2),
        "find-backward" => Some(1722),
        "find-whole-word" | "files-whole-word" => Some(1603),
        "find-match-case" | "files-match-case" => Some(1604),
        "find-wrap" => Some(1606),
        "find-count" => Some(1614),
        "find-in-selection" => Some(1632),
        "find-all-current" => Some(1641),
        "find-all-open" => Some(1636),
        "replace-one" => Some(1608),
        "replace-all" => Some(1609),
        "replace-all-open" => Some(1635),
        "mark-all" => Some(1615),
        "mark-clear" => Some(1633),
        "mark-copy" => Some(1725),
        "mark-bookmark" => Some(1616),
        "mark-purge" => Some(1618),
        "files-find-all" => Some(1656),
        "files-replace-all" => Some(if label.contains("项目") { 1665 } else { 1660 }),
        "project-search-replace" => Some(1665),
        "files-recursive" => Some(1658),
        "files-hidden" => Some(1659),
        "find-dot-newline" => Some(1703),
        "find-transparency" => Some(1686),
        "project-search-1" => Some(1662),
        "project-search-2" => Some(1663),
        "project-search-3" => Some(1664),
        _ => None,
    };
    source.and_then(|id| crate::localization::current_dialog_control("Find", id))
        .unwrap_or_else(|| crate::localization::current_dialog_text(label))
}

pub(super) fn checkbox(
    id: &'static str,
    label: &'static str,
    checked: bool,
) -> gpui::Stateful<gpui::Div> {
    checkbox_enabled(id, label, checked, true)
}

pub(super) fn checkbox_enabled(
    id: &'static str,
    label: &'static str,
    checked: bool,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let label = source_label(id, label);
    let search_control = id.starts_with("find-") && !id.starts_with("find-in-results-")
        || id.starts_with("mark-")
        || id.starts_with("files-") || id.starts_with("project-search-");
    div()
        .id(id)
        .debug_selector(move || id.into())
        .role(Role::CheckBox)
        .aria_label(label)
        .aria_toggled(checked.into())
        .h(px(20.))
        .flex()
        .items_center()
        .gap(px(3.))
        .child(
            div()
                .size(px(13.))
                .flex_shrink_0()
                .border_1()
                .border_color(rgb(if search_control {
                    if enabled { 0x333333 } else { 0xcccccc }
                } else { 0x767676 }))
                .bg(rgb(0xffffff))
                .text_color(rgb(if enabled { 0x202020 } else { 0x808080 }))
                .flex()
                .items_center()
                .justify_center()
                .child(if checked { "✓" } else { "" }),
        )
        .child(label)
}

pub(super) fn input_row(label: &'static str, top: f32, field: impl IntoElement) -> AnyElement {
    input_row_with_label_width(label, top, 125., field)
}

pub(super) fn input_row_with_label_width(
    label: &'static str,
    top: f32,
    label_width: f32,
    field: impl IntoElement,
) -> AnyElement {
    let label = crate::localization::current_dialog_text(label);
    div()
        .absolute()
        .left(px(3.))
        .right(px(289.))
        .top(px(top))
        .h(px(21.))
        .flex()
        .items_center()
        .gap(px(5.))
        .child(div().w(px(label_width)).flex_shrink_0().text_right().child(label))
        .child(div().flex_1().min_w(px(0.)).child(field))
        .into_any_element()
}

pub(super) fn radio(id: impl Into<gpui::ElementId>, label: &'static str, checked: bool) -> gpui::Stateful<gpui::Div> {
    let label = crate::localization::current_dialog_text(label);
    div().id(id).role(Role::RadioButton).aria_label(label).aria_toggled(checked.into())
        .h(px(20.)).flex().items_center().gap(px(3.))
        .child(div().size(px(13.)).flex_shrink_0().rounded_full().border_1()
            .border_color(rgb(0x767676)).bg(rgb(0xffffff))
            .flex().items_center().justify_center()
            .when(checked, |control| control.child(div().size(px(7.)).rounded_full().bg(rgb(0x404040)))))
        .child(label)
}

pub(super) fn button(id: &'static str, label: &'static str, top: f32) -> gpui::Stateful<gpui::Div> {
    let label = source_label(id, label);
    div()
        .id(id)
        .debug_selector(move || id.into())
        .role(Role::Button)
        .aria_label(label)
        .absolute()
        .right(px(33.))
        .top(px(top))
        .w(px(137.))
        .h(px(21.))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .rounded(px(3.))
        .border_color(rgb(0xadadad))
        .bg(rgb(0xfafafa))
        .child(if matches!(id, "find-all-current" | "find-all-open" | "replace-all-open") {
            div().w_full().min_w(px(0.)).px(px(4.)).text_center().child(label).into_any_element()
        } else {
            label.into_any_element()
        })
}

pub(super) fn source_button(
    id: &'static str,
    label: &'static str,
    top: f32,
) -> gpui::Stateful<gpui::Div> {
    source_button_enabled(id, label, top, true)
}

pub(super) fn source_button_enabled(
    id: &'static str,
    label: &'static str,
    top: f32,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    button(id, label, top)
        .when(enabled, |button| button.focusable()
        .tab_index(match id {
            "files-browse" => 5, "files-directory-from-document" => 6,
            "find-previous" => 21,
            "find-next" | "files-find-all" | "range-find-next" => 22,
            "find-count" | "replace-one" | "mark-all" | "files-replace-all" => 24,
            "find-all-current" | "replace-all" | "mark-clear" => 25,
            "find-all-open" | "replace-all-open" | "mark-copy" => 26,
            "find-compact" => 40,
            _ => 29,
        }))
        .text_color(rgb(if enabled { 0x202020 } else { 0x808080 }))
        .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
        .right(px(40.))
        .w(px(157.))
        .h(if matches!(id, "find-all-current" | "find-all-open" | "replace-all-open") {
            dialog_font::line_height() * 2. + px(7.)
        } else {
            dialog_font::control_height()
        })
        .when(!matches!(id, "find-all-current" | "find-all-open" | "replace-all-open"), |button| button.whitespace_nowrap())
        .rounded(px(0.))
        .bg(rgb(0xe1e1e1))
        .when(enabled, |button| button
            .hover(|style| style.bg(rgb(0xe5f1fb)).border_color(rgb(0x0078d7))))
        .on_key_down(move |event, window, cx| {
            if enabled && event.keystroke.key == "space" {
                window.dispatch_action(Box::new(FindButtonAction(id, event.keystroke.modifiers.shift)), cx);
                cx.stop_propagation();
            }
        })
        .on_action(move |_: &FindDialogNext, window, cx| {
            if enabled { window.dispatch_action(Box::new(FindButtonAction(id, false)), cx); }
            cx.stop_propagation();
        })
        .on_action(move |_: &FindDialogReverse, window, cx| {
            if enabled { window.dispatch_action(Box::new(FindButtonAction(id, true)), cx); }
            cx.stop_propagation();
        })
        .on_action(move |_: &FindAllInFiles, window, cx| {
            if enabled { window.dispatch_action(Box::new(FindButtonAction(id, false)), cx); }
            cx.stop_propagation();
        })
}

pub(super) fn search_mode(
    mode: crate::search::SearchMode,
    focus: &[FocusHandle; 3],
    dot_focus: &FocusHandle,
    cx: &mut Context<MatrixWindow>,
) -> AnyElement {
    let title = crate::localization::current_dialog_control("Find", 1624).unwrap_or("Search mode");
    div()
        .id("find-search-mode")
        .role(Role::Group)
        .aria_label(title)
        .absolute()
        .left(px(11.))
        .top(px(230.))
        .w(px(350.))
        .h(px(82.))
        .child(
            div()
                .absolute()
                .top(px(6.))
                .bottom(px(0.))
                .w_full()
                .border_1()
                .border_color(rgb(0xdcdcdc)),
        )
        .child(
            div()
                .absolute()
                .left(px(6.))
                .top(px(0.))
                .px(px(2.))
                .bg(rgb(0xffffff))
                .child(title),
        )
        .children(
            [
                (
                    crate::search::SearchMode::Normal,
                    "find-normal-mode",
                    "普通(N)",
                    18.,
                ),
                (
                    crate::search::SearchMode::Extended,
                    "find-extended-mode",
                    r"扩展(X) (\n, \r, \t, \0, \x...)",
                    39.,
                ),
                (
                    crate::search::SearchMode::Regex {
                        dot_matches_newline: matches!(
                            mode,
                            crate::search::SearchMode::Regex {
                                dot_matches_newline: true
                            }
                        ),
                    },
                    "find-regex-mode",
                    "正则表达式(G)",
                    60.,
                ),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (value, id, label, top))| {
                let source_id = [1625, 1626, 1605][index];
                let label = crate::localization::current_dialog_control("Find", source_id).unwrap_or(label);
                let selected = mode == value;
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .role(Role::RadioButton)
                    .track_focus(&focus[index].clone().tab_stop(selected))
                    .aria_label(label)
                    .aria_toggled(selected.into())
                    .absolute()
                    .left(px(10.))
                    .top(px(top))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(
                        div()
                            .size(px(13.))
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(0x808080))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |radio| {
                                radio.child(div().size(px(7.)).rounded_full().bg(rgb(0x202020)))
                            }),
                    )
                    .child(label)
                    .on_click(cx.listener(move |view, _, window, cx| view.select_find_mode(value, window, cx)))
                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                        let target = match event.keystroke.key.as_str() {
                            "space" => index,
                            "right" | "down" => (index + 1) % 3,
                            "left" | "up" => (index + 2) % 3,
                            _ => return,
                        };
                        let modes = [crate::search::SearchMode::Normal, crate::search::SearchMode::Extended,
                            crate::search::SearchMode::Regex { dot_matches_newline: view.find_regex_dot }];
                        view.select_find_mode(modes[target], window, cx);
                        cx.stop_propagation();
                    }))
            }),
        )
        .child(
            checkbox_enabled(
                "find-dot-newline",
                ". 号匹配换行符",
                matches!(
                    mode,
                    crate::search::SearchMode::Regex {
                        dot_matches_newline: true
                    }
                ),
                matches!(mode, crate::search::SearchMode::Regex { .. }),
            )
            .absolute()
            .left(px(152.))
            .top(px(60.))
            .when(matches!(mode, crate::search::SearchMode::Regex { .. }), |control| control.track_focus(dot_focus))
            .text_color(rgb(
                if matches!(mode, crate::search::SearchMode::Regex { .. }) {
                    0x202020
                } else {
                    0x808080
                },
            ))
            .a11y_synthetic_children(move |builder| {
                if !matches!(mode, crate::search::SearchMode::Regex { .. }) {
                    builder.parent_node().set_disabled();
                }
            })
            .on_click(cx.listener(|view, _, window, cx| {
                view.focus_find_option(1703, window, cx);
                view.toggle_find_dot(cx);
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "space" { view.toggle_find_dot(cx); cx.stop_propagation(); }
            })),
        )
        .into_any_element()
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("f3", FindNext, None),
        KeyBinding::new("shift-f3", FindPrevious, None),
        KeyBinding::new("enter", FindDialogNext, Some("MatrixFind")),
        KeyBinding::new("shift-enter", FindDialogReverse, Some("MatrixFind")),
        KeyBinding::new("enter", SearchHistoryAccept, Some("MatrixSearchHistory")),
        KeyBinding::new("escape", SearchHistoryCancel, Some("MatrixSearchHistory")),
        KeyBinding::new("up", SearchHistoryUp, Some("MatrixSearchHistory")),
        KeyBinding::new("down", SearchHistoryDown, Some("MatrixSearchHistory")),
        KeyBinding::new("alt-down", SearchHistoryOpen, Some("MatrixTextField")),
        KeyBinding::new("tab", SearchHistoryNextField, Some("MatrixSearchHistory")),
        KeyBinding::new(
            "shift-tab",
            SearchHistoryPreviousField,
            Some("MatrixSearchHistory"),
        ),
    ];
    for context in ["MatrixFind", "MatrixFindInFiles"] {
        for (key, source_id) in [
            ("alt-f", 1620), ("alt-l", 1611), ("alt-w", 1603), ("alt-c", 1604),
            ("alt-p", 1606), ("alt-n", 1625), ("alt-x", 1626), ("alt-g", 1605),
            ("alt-.", 1703), ("alt-i", 1632), ("alt-y", 1686), ("alt-m", 1616),
            ("alt-e", 1), ("alt-t", 1614), ("alt-r", 1608), ("alt-a", 1609),
            ("alt-o", 1636), ("alt-s", 1654), ("alt-b", 1658), ("alt-h", 1659),
        ] {
            bindings.push(KeyBinding::new(key, FindOptionAction(source_id), Some(context)));
        }
    }
    bindings
}

impl MatrixWindow {
    pub(super) fn find_text(&self, id: &str, fallback: &'static str) -> &'static str {
        crate::localization::localized_string(&self.native_locale(), id).unwrap_or(fallback)
    }

    pub(super) fn find_tab_title(&self, index: usize) -> &'static str {
        let (attribute, fallback) = match index {
            1 => ("titleReplace", "Replace"),
            2 => ("titleFindInFiles", "Find in Files"),
            3 => ("titleFindInProjects", "Find in Projects"),
            4 => ("titleMark", "Mark"),
            _ => ("titleFind", "Find"),
        };
        crate::localization::dialog_title_variant(&self.native_locale(), "Find", attribute).unwrap_or(fallback)
    }

    pub(super) fn find_dialog_title(&self) -> &'static str {
        self.find_tab_title(self.find_tab_index())
    }

    pub(super) fn sync_find_field_labels(&self, cx: &mut Context<Self>) {
        let locale = self.native_locale();
        let find = crate::localization::dialog_control(&locale, "Find", 1620).unwrap_or("Find what:");
        let replace = crate::localization::dialog_control(&locale, "Find", 1611).unwrap_or("Replace with:");
        self.find_query.update(cx, |field, cx| field.set_label(find, cx));
        self.replace_input.update(cx, |field, cx| field.set_label(replace, cx));
    }

    pub(super) fn set_find_status(&mut self, status: FindStatus, message: impl Into<Text>) {
        if status == FindStatus::NotFound && !self.mute_sounds {
            objc2_app_kit::NSBeep();
        }
        self.find_status = status;
        self.find_status_tooltip = "".into();
        self.find_message = if status == FindStatus::NoMessage { "".into() } else { message.into() };
    }

    pub(super) fn find_field_warning(&mut self, show: bool, cx: &mut Context<Self>) {
        if show {
            self.set_find_status(FindStatus::Warning, Text::resource("find-status-invisible-chars-findWhat", "Invisible characters in pasted search content"));
            self.find_status_tooltip = Text::resource("find-status-invisible-chars-findWhat-tip", "Warning: Invisible characters in search or replace field.");
        } else if self.find_status == FindStatus::Warning {
            self.set_find_status(FindStatus::Message, "");
        }
        cx.notify();
    }

    pub(super) fn set_find_error(&mut self, error: &crate::search::SearchError) {
        let (message, detail) = match error {
            crate::search::SearchError::Regex(detail) => (Text::resource("find-status-invalid-re", "Find: Invalid regular expression"), detail),
            crate::search::SearchError::RegexRuntime(detail) => (Text::resource("find-status-search-failed", "Find: Search failed"), detail),
            crate::search::SearchError::InvalidSelection => {
                self.set_find_status(FindStatus::NotFound,
                    Text::resource("matrix-search-invalid-selection", "Search selection is not a valid text boundary"));
                return;
            }
        };
        self.set_find_status(FindStatus::NotFound, message);
        self.find_status_tooltip = detail.clone().into();
    }

    pub(super) fn set_serialized_find_error(&mut self, error: &str) {
        let invalid_regex = crate::search::SearchError::Regex(String::new()).to_string();
        let search_failed = crate::search::SearchError::RegexRuntime(String::new()).to_string();
        if let Some(detail) = error.strip_prefix(invalid_regex.as_str()) {
            self.set_find_error(&crate::search::SearchError::Regex(detail.to_owned()));
        } else if let Some(detail) = error.strip_prefix(search_failed.as_str()) {
            self.set_find_error(&crate::search::SearchError::RegexRuntime(detail.to_owned()));
        } else if error == crate::search::SearchError::InvalidSelection.to_string() {
            self.set_find_error(&crate::search::SearchError::InvalidSelection);
        } else {
            self.set_find_status(FindStatus::NotFound, Text::resource("find-status-search-failed", "Find: Search failed"));
            self.find_status_tooltip = error.into();
        }
    }

    fn set_find_replace_error(&mut self, error: &crate::search::ReplaceError) {
        match error {
            crate::search::ReplaceError::Search(error) => self.set_find_error(error),
            crate::search::ReplaceError::Edit(error) => self.set_find_status(FindStatus::NotFound, error.to_string()),
        }
    }

    pub(super) fn set_find_scope_reason(&mut self, count: usize, empty_string_found: bool) {
        let most_lax = self.find_options.wrap && !self.find_options.match_case && !self.find_options.whole_word;
        if count == 0 && !most_lax {
            self.find_status_tooltip = Text::resource("find-status-cannot-find-pebkac-maybe", "The given occurrence cannot be found.");
        } else if empty_string_found {
            self.find_status_tooltip = Text::resource("find-status-regex-find-empty-string", "The given regular expression matches empty strings.");
        }
    }

    pub(super) fn set_find_not_found(&mut self, query: &str) {
        let escaped = query.replace('&', "&&");
        let units: Vec<_> = escaped.encode_utf16().collect();
        let owner_text = if units.len() > 32 {
            format!("{}...", String::from_utf16_lossy(&units[..28]))
        } else { escaped };
        let mut chars = owner_text.chars().peekable();
        let mut visible = String::new();
        while let Some(ch) = chars.next() {
            if ch == '&' {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    visible.push('&');
                }
            } else {
                visible.push(ch);
            }
        }
        self.set_find_status(FindStatus::NotFound,
            Text::resource("find-status-cannot-find", "Find: Can't find the text \"$STR_REPLACE$\"")
                .replace("$STR_REPLACE$", visible).with_scope(self.find_scope_resource()));
        self.set_find_scope_reason(0, false);
    }

    pub(super) fn render_find_status(&self, id: &'static str) -> AnyElement {
        let locale = self.native_locale();
        let message = self.find_message.render(&locale);
        let tooltip = self.find_status_tooltip.render(&locale);
        div().id(id).role(Role::Status).aria_label(message.clone())
            .absolute().bottom(px(0.)).w_full().h(px(22.)).pl(px(3.)).pr(px(16.))
            .border_t_1().border_color(rgb(0xd7d7d7)).bg(rgb(0xf0f0f0))
            .text_color(self.find_status.color(self)).flex().items_center().gap(px(8.)).overflow_hidden().whitespace_nowrap()
            .child(message)
            .when(!tooltip.is_empty(), |status| {
                status.child(div().id("find-status-info").size(px(16.)).flex_shrink_0()
                    .aria_label(self.find_text("matrix-find-status-details", "Status details"))
                    .tooltip(move |_, cx| icon_tooltip::view(tooltip.clone(), false, cx))
                    .child(gpui::img(toolbar_icons::ToolbarIcon::StatusInfo.image()).size_full()))
            }).into_any_element()
    }
    pub(super) fn find_button_command(&mut self, id: &'static str, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy && id != "files-close" { return; }
        match id {
            "find-next" if self.find_controls.two_buttons => self.find_next(false, window, cx),
            "find-next" => self.activate_find_default(reverse, window, cx),
            "find-previous" => self.find_next(true, window, cx),
            "find-count" => self.count_find(window, cx),
            "find-all-current" => self.find_all_documents(false, window, cx),
            "find-all-open" => self.find_all_documents(true, window, cx),
            "replace-one" => self.replace_one(window, cx),
            "replace-all" => self.replace_all(window, cx),
            "replace-all-open" => self.confirm_replace_open_documents(window, cx),
            "mark-all" => self.mark_all(window, cx),
            "mark-clear" => self.clear_find_marks(window, cx),
            "mark-copy" => self.copy_marked_text(43061, cx),
            "find-close" => self.dismiss_floating(floating_windows::DialogKind::Find, window, cx),
            "find-compact" => self.toggle_find_compact(cx),
            "range-find-next" => self.find_character_range(window, cx),
            "range-close" => self.dismiss_floating(floating_windows::DialogKind::CharacterRange, window, cx),
            _ => self.file_search_button_command(id, window, cx),
        }
    }
    pub(super) fn find_option_command(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        use crate::search_history::Field;
        if self.file_busy || self.close_dialog || self.save_all_dialog { return; }
        if self.find_controls.compact && (matches!(id, 1603 | 1604 | 1606 | 1625 | 1626 | 1605 | 1703 | 1632 | 1616 | 1618 | 1686 | 1654 | 1658 | 1659 | 1662..=1664 | 1609 | 1636)
            || id == 1 && self.files_search.visible) { return; }
        match id {
            1620 | 1611 | 1654 => {
                let field = match id { 1620 => Field::Find, 1611 => Field::Replace, _ => Field::Filter };
                if id == 1611 && !self.find_replace_mode && !self.files_search.visible
                    || id == 1654 && !self.files_search.visible { return; }
                self.close_search_history();
                self.close_find_swap_menu(cx);
                self.search_history_input(field).update(cx, |field, cx| field.focus(window, cx));
            }
            1603 => { if !matches!(self.find_options.mode, crate::search::SearchMode::Regex { .. }) { self.toggle_find_check(FindCheck::WholeWord, cx); } }
            1604 => self.toggle_find_check(FindCheck::MatchCase, cx),
            1606 if !self.files_search.visible => self.toggle_find_check(FindCheck::Wrap, cx),
            1625 => self.select_find_mode(crate::search::SearchMode::Normal, window, cx),
            1626 => self.select_find_mode(crate::search::SearchMode::Extended, window, cx),
            1605 => self.select_find_mode(crate::search::SearchMode::Regex { dot_matches_newline: self.find_regex_dot }, window, cx),
            1703 => self.toggle_find_dot(cx),
            1632 if !self.files_search.visible && self.find_in_selection_enabled => {
                self.find_in_selection = !self.find_in_selection;
                self.find_in_selection_active = self.find_in_selection;
                self.find_scope_auto_changed = false;
            }
            1616 | 1618 if self.find_mark_mode && self.find_visible => {
                let bookmark = id == 1616;
                let old = if bookmark { self.mark_bookmark } else { self.mark_purge };
                if self.persist_find_option(if bookmark { "bookmarkLine" } else { "purge" }, (!old).into(), cx) {
                    if bookmark { self.mark_bookmark = !old; } else { self.mark_purge = !old; }
                }
            }
            1686 => self.toggle_find_transparency(cx),
            1 if self.files_search.visible && !self.find_project_mode => {
                self.close_search_history();
                self.search_history_input(Field::Directory).update(cx, |field, cx| field.focus(window, cx));
            }
            1 if !self.files_search.visible && !self.find_mark_mode => self.activate_find_default(false, window, cx),
            1614 if self.find_visible && !self.find_replace_mode && !self.find_mark_mode => self.count_find(window, cx),
            1608 if self.find_visible && self.find_replace_mode => self.replace_one(window, cx),
            1609 if self.find_visible && self.find_replace_mode => self.replace_all(window, cx),
            1636 if self.find_visible && !self.find_mark_mode && !self.find_controls.compact => {
                if self.find_replace_mode { self.confirm_replace_open_documents(window, cx); }
                else { self.find_all_documents(true, window, cx); }
            }
            1658 | 1659 | 1662..=1664 => self.toggle_file_search_option(id, cx),
            _ => {}
        }
        self.focus_find_option(id, window, cx);
        cx.notify();
    }
    fn toggle_find_dot(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.find_options.mode, crate::search::SearchMode::Regex { .. }) { return; }
        let value = !self.find_regex_dot;
        if self.persist_find_option("dotMatchesNewline", value.into(), cx) {
            self.find_regex_dot = value;
            self.find_options.mode = crate::search::SearchMode::Regex { dot_matches_newline: value };
        }
        cx.notify();
    }

    pub(super) fn activate_find_default(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_mark_mode && self.find_visible {
            self.mark_all(window, cx);
        } else {
            let backwards = self.find_backward;
            self.find_next(if reverse { !backwards } else { backwards }, window, cx);
        }
    }
    fn select_find_mode(&mut self, mode: crate::search::SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        let index = match mode { crate::search::SearchMode::Normal => 0,
            crate::search::SearchMode::Extended => 1, crate::search::SearchMode::Regex { .. } => 2 };
        if self.find_options.mode != mode && !self.persist_find_option("searchMode", index.into(), cx) { return; }
        self.find_options.mode = if index == 2 {
            self.find_options.whole_word = false;
            if !self.find_regex_backward { self.find_backward = false; }
            crate::search::SearchMode::Regex { dot_matches_newline: self.find_regex_dot }
        } else { mode };
        self.find_mode_focus[index].focus(window, cx);
        cx.notify();
    }
    pub(super) fn configure_find(&mut self, settings: &serde_json::Value) {
        self.configure_find_controls(settings);
        self.configure_incremental(settings);
        self.configure_finder(settings);
        self.configure_search_history(settings);
        let history = &settings["FindHistory"];
        self.find_regex_dot = history["dotMatchesNewline"].as_bool().unwrap_or(false);
        self.find_regex_backward = history["regexBackward4PowerUser"]
            .as_bool()
            .unwrap_or(false);
        self.find_options.mode = match history["searchMode"].as_u64() {
            Some(1) => crate::search::SearchMode::Extended,
            Some(2) => crate::search::SearchMode::Regex {
                dot_matches_newline: self.find_regex_dot,
            },
            _ => crate::search::SearchMode::Normal,
        };
        self.find_options.match_case = history["matchCase"].as_bool().unwrap_or(false);
        self.find_options.whole_word = history["matchWord"].as_bool().unwrap_or(false);
        self.find_options.wrap = history["wrap"].as_bool().unwrap_or(true);
        self.find_backward = !history["directionDown"].as_bool().unwrap_or(true);
        if matches!(
            self.find_options.mode,
            crate::search::SearchMode::Regex { .. }
        ) {
            self.find_options.whole_word = false;
            if !self.find_regex_backward {
                self.find_backward = false;
            }
        }
        self.mark_bookmark = history["bookmarkLine"].as_bool().unwrap_or(false);
        self.mark_purge = history["purge"].as_bool().unwrap_or(false);
        self.mark_all_case_sensitive = settings["MarkAll"]["matchCase"].as_bool().unwrap_or(false);
        self.mark_all_word_only = settings["MarkAll"]["wholeWordOnly"].as_bool().unwrap_or(true);
        let searching = &settings["Searching"];
        self.find_fill_selected = searching["fillFindFieldWithSelected"]
            .as_bool()
            .unwrap_or(true);
        self.find_fill_caret = searching["fillFindFieldSelectCaret"]
            .as_bool()
            .unwrap_or(true);
        self.find_autocheck_threshold = searching["inSelectionAutocheckThreshold"]
            .as_u64()
            .filter(|value| *value <= 1024)
            .map_or(1024, |value| value as usize);
        self.find_fill_threshold = searching["fillFindWhatThreshold"]
            .as_u64()
            .filter(|value| (1..16384).contains(value))
            .map_or(1024, |value| value as usize);
    }

    fn selected_find_characters(&self) -> Result<usize, EditError> {
        let range = self.editor().source_selection_limits()?.range();
        if range.is_empty() {
            return Ok(0);
        }
        self.editor().document().byte_snapshot()
            .count_characters(range.start.0..range.end.0, self.find_autocheck_threshold.max(self.find_fill_threshold) + 1)
            .map_err(|error| EditError::Storage(error.to_string()))
    }

    pub(super) fn prepare_find_activation(&mut self, cx: &mut Context<Self>) {
        let active = self
            .floating_windows
            .get(&floating_windows::DialogKind::Find)
            .copied()
            .is_some_and(|handle| {
                match handle.update(cx, |_, window, _| window.is_window_active()) {
                    Ok(active) => active,
                    Err(error) => {
                        eprintln!("search activation lost its window: {error}");
                        false
                    }
                }
            });
        if !active {
            self.activate_find_selection(cx);
            self.find_activation_prepared = true;
        }
    }

    pub(super) fn activate_find_selection(&mut self, cx: &mut Context<Self>) {
        self.sync_find_project_panels(cx);
        let count = match self.selected_find_characters() {
            Ok(count) => count,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let selections = self.editor().document().selections();
        self.find_in_selection_enabled =
            count != 0 && !selections.is_rectangular() && selections.ranges().len() == 1;
        let previous = self.find_in_selection;
        if self.find_autocheck_threshold != 0 {
            self.find_in_selection =
                self.find_in_selection_enabled && count >= self.find_autocheck_threshold;
        }
        self.find_scope_auto_changed = previous != self.find_in_selection;
        self.find_in_selection_active = self.find_in_selection_enabled && self.find_in_selection;
        cx.notify();
    }

    pub(super) fn fill_find_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_fill_selected {
            return;
        }
        let selected = match self.editor().source_selection_limits() {
            Ok(selection) => selection,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        if self.find_fill_caret
            && selected.range().is_empty()
            && let Err(error) = self.editor_mut().expand_word_selection()
        {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        let count = match self.selected_find_characters() {
            Ok(count) => count,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        if count != 0 && count <= self.find_fill_threshold {
            let text = match self.editor().clipboard_text() {
                Ok(payload) => payload.text.split('\0').next().unwrap_or_default().to_owned(),
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            if !text.is_empty() {
                self.find_query.update(cx, |field, cx| field.set_text(&text, window, cx));
            }
        }
    }

    pub(super) fn find_scope_resource(&self) -> (&'static str, &'static str) {
        if self.find_in_selection_active {
            ("find-status-scope-selection", "in selected text")
        } else if self.find_options.wrap {
            ("find-status-scope-all", "in entire file")
        } else if self.find_backward {
            ("find-status-scope-backward", "from start-of-file to caret")
        } else {
            ("find-status-scope-forward", "from caret to end-of-file")
        }
    }

    pub(super) fn count_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.count_find(window, cx)) {
            return;
        }
        if !self.find_visible
            || self.find_replace_mode
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        self.record_search_history(crate::search_history::Field::Find, window, cx);
        let bytes = self.editor().document().byte_snapshot();
        let selection = match self.editor().source_selection_limits() {
            Ok(selection) => selection,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let mut regex_state = self.editor().document().regex_state();
        let result = crate::search::literal_scope_bytes(&bytes, selection, self.find_options.wrap,
            self.find_backward, self.find_in_selection_active).and_then(|scope| {
                if !query.is_empty() {
                    self.set_scintilla_shortcut_raw_target(scope.start as i128..scope.end as i128);
                }
                let mut count = 0;
                let mut empty_string_found = false;
                let mut last = None;
                crate::search::visit_literal_ranges_in_bytes_with_state(&bytes, &query, scope.clone(), self.find_options, &mut regex_state, |range| {
                    count += 1;
                    empty_string_found |= range.is_empty();
                    last = Some(range);
                    true
                }).map(|()| {
                        if let Some(last) = last {
                            let target = if last.end.0 == scope.end { last.start.0..last.end.0 }
                                else { last.end.0..scope.end };
                            self.set_scintilla_shortcut_raw_target(target.start as i128..target.end as i128);
                        }
                        (count, empty_string_found)
                    })
            });
        self.editor_mut().document_mut().set_regex_state(regex_state);
        match result {
            Ok((count, empty_string_found)) => {
                let digits = count.to_string();
                let mut grouped = String::new();
                for (index, digit) in digits.chars().enumerate() {
                    if index != 0 && (digits.len() - index).is_multiple_of(3) {
                        grouped.push(',');
                    }
                    grouped.push(digit);
                }
                self.set_find_status(FindStatus::Message,
                    Text::resource(if count == 1 { "find-status-count-1-match" } else { "find-status-count-nb-matches" },
                        "Count: $INT_REPLACE$ matches").replace("$INT_REPLACE$", grouped).with_scope(self.find_scope_resource()));
                self.set_find_scope_reason(count, empty_string_found);
            }
            Err(error) => self.set_find_error(&error),
        }
        cx.notify();
    }

    fn warn_find_scope_change(
        &mut self,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = std::mem::take(&mut self.find_scope_auto_changed);
        if !event.first_focus() || !changed {
            return false;
        }
        let locale = self.native_locale();
        let prompt = window.prompt(gpui::PromptLevel::Warning,
            crate::localization::dialog_title(&locale, "FindAutoChangeOfInSelectionWarning").unwrap_or("Search warning"),
            Some(crate::localization::dialog_message(&locale, "FindAutoChangeOfInSelectionWarning")
                .unwrap_or("The \"In selection\" checkbox state has been automatically modified.\r\nPlease verify the search condition before performing the action.")),
            &[self.find_text("common-ok", "OK")], cx);
        self.file_busy = true;
        cx.spawn_in(window, async move |view, cx| {
            let answered = prompt.await;
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.file_busy = false;
                if let Err(error) = answered {
                    view.notify_localized_editor_error(
                        Text::resource("matrix-search-warning-prompt-error", "Search warning prompt disconnected: $ERROR$")
                            .replace("$ERROR$", error.to_string()),
                        cx,
                    );
                }
                cx.notify();
            }) {
                eprintln!("search warning lost its document: {error}");
            }
        })
        .detach();
        true
    }

    pub(super) fn show_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.show_find(window, cx)) {
            return;
        }
        self.open_find(false, window, cx);
    }

    pub(super) fn show_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.show_replace(window, cx)) {
            return;
        }
        self.open_find(true, window, cx);
    }

    fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.notify_localized_editor_error(
                Text::resource("matrix-find-composition-unavailable", "Find during active document composition is not implemented yet"),
                cx,
            );
            return;
        }
        self.close_search_history();
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.find_visible = true;
        self.find_created = true;
        self.files_search.visible = false;
        self.find_replace_mode = replace;
        self.find_mark_mode = false;
        self.initialize_search_history(window, cx);
        self.prepare_find_activation(cx);
        self.present_floating(floating_windows::DialogKind::Find, window, cx);
        self.fill_find_query(window, cx);
        cx.notify();
    }

    pub(super) fn find_next(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.find_created { return; }
        self.find_next_with_origin(backwards, false, window, cx);
    }

    fn find_next_with_origin(
        &mut self,
        backwards: bool,
        replacing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FindOutcome {
        if backwards
            && matches!(
                self.find_options.mode,
                crate::search::SearchMode::Regex { .. }
            )
            && !self.find_regex_backward
        {
            self.set_find_status(FindStatus::NotFound, Text::resource("matrix-find-backward-unavailable", "Backward regular expression search is disabled"));
            cx.notify();
            return FindOutcome::Error;
        }
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.find_next_with_origin(backwards, replacing, window, cx);
        }) {
            return FindOutcome::Unchanged;
        }
        self.record_macro_step(crate::macros::MacroStep::Unsupported("search commands"));
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return FindOutcome::Unchanged;
        }
        self.set_find_status(FindStatus::NoMessage, "");
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        if query.is_empty() {
            cx.notify();
            return FindOutcome::Unchanged;
        }
        if !replacing {
            self.record_search_history(crate::search_history::Field::Find, window, cx);
        }
        if self.editor().marked_range().is_some() {
            self.set_find_status(FindStatus::NotFound,
                Text::resource("matrix-find-composition-unavailable", "Find during active document composition is not implemented yet"));
            cx.notify();
            return FindOutcome::Error;
        }
        let bytes = self.editor().document().byte_snapshot();
        let selection = match self.editor().source_selection_limits() {
            Ok(selection) => selection,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return FindOutcome::Error;
            }
        };
        let mut regex_state = self.editor().document().regex_state();
        let kind = if replacing { crate::search::RegexFindKind::ReplaceNext } else { crate::search::RegexFindKind::FindNext };
        let selected = selection.range();
        self.set_scintilla_shortcut_raw_target(if backwards {
            selected.end.0 as i128 - i128::from(!replacing)..0
        } else {
            (if replacing { selected.start.0 } else { selected.end.0 }) as i128..bytes.len() as i128
        });
        let found = crate::search::find_literal_bytes_with_state(&bytes, &query, selection,
            self.find_options, backwards, kind, &mut regex_state);
        self.editor_mut().document_mut().set_regex_state(regex_state);
        let outcome = match found {
            Ok(Some(found)) => {
                if let Err(error) = self.set_scintilla_shortcut_target(found.range.clone()) {
                    self.editor_changed(Err(error), window, cx);
                    return FindOutcome::Error;
                }
                let result = self.select_search_result(found.range.clone(), backwards, window, cx);
                if result.is_err() {
                    self.editor_changed(result, window, cx);
                    return FindOutcome::Error;
                }
                self.editor_changed(result, window, cx);
                let (status, message) = if found.wrapped {
                    if backwards {
                        (FindStatus::TopReached, Text::resource("find-status-top-reached", "Find: Reached document beginning, first occurrence from the bottom found."))
                    } else {
                        (FindStatus::EndReached, Text::resource("find-status-end-reached", "Find: Reached document end, first occurrence from the top found."))
                    }
                } else {
                    (FindStatus::NoMessage, "".into())
                };
                self.set_find_status(status, message);
                FindOutcome::Found
            }
            Ok(None) => {
                if self.find_options.wrap {
                    self.set_scintilla_shortcut_raw_target(if backwards { bytes.len() as i128..0 } else { 0..bytes.len() as i128 });
                }
                if !replacing {
                    self.set_find_not_found(&query);
                    if self.find_visible || self.files_search.visible {
                        self.present_floating(floating_windows::DialogKind::Find, window, cx);
                    } else {
                        self.editor_focus.focus(window, cx);
                    }
                }
                FindOutcome::NotFound
            }
            Err(error) => {
                self.set_find_error(&error);
                FindOutcome::Error
            }
        };
        cx.notify();
        outcome
    }

    pub(super) fn replace_one(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.replace_one(window, cx)) {
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Unsupported("search commands"));
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.set_find_status(FindStatus::NotFound,
                Text::resource("matrix-replace-composition-unavailable", "Replace during active document composition is not implemented yet"));
            cx.notify();
            return;
        }
        self.set_find_status(FindStatus::NoMessage, "");
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        let replacement = self.captured_search_input(crate::search_history::Field::Replace, cx);
        self.record_search_histories(
            &[
                crate::search_history::Field::Replace,
                crate::search_history::Field::Find,
            ],
            window,
            cx,
        );
        if query.is_empty() {
            cx.notify();
            return;
        }
        if self.editor().document().is_read_only() {
            self.set_find_status(FindStatus::NotFound, Text::resource("find-status-replace-readonly", "Replace: Cannot replace text. The current document is read only"));
            cx.notify();
            return;
        }
        let selection = match self.editor().source_selection_limits() {
            Ok(selection) => selection,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let current = selection.range();
        let mut bytes = self.editor().document().byte_snapshot();
        let mut regex_state = self.editor().document().regex_state();
        self.set_scintilla_shortcut_raw_target(if self.find_backward {
            current.end.0 as i128..0
        } else { current.start.0 as i128..bytes.len() as i128 });
        let found = crate::search::find_replacement_bytes_with_state(
            &bytes,
            &query,
            &replacement,
            selection,
            self.find_options,
            self.find_backward,
            &mut regex_state,
        );
        self.editor_mut().document_mut().set_regex_state(regex_state);
        match found {
            Ok(Some((found, replacement))) if found.range == current => {
                if let Err(error) = self.set_scintilla_shortcut_target(found.range.clone()) {
                    self.editor_changed(Err(error), window, cx);
                    return;
                }
                let mut edits = crate::editor::LineEdits::default();
                let caret = ByteOffset(current.start.0 + replacement.len());
                let result = edits.change_bytes(&mut bytes, current.start.0..current.end.0, replacement.as_bytes())
                    .and_then(|()| self.editor_mut().commit_prepared_bytes(bytes,
                        crate::editor::SelectionSet::single(Selection { anchor: caret, head: caret }), edits));
                if let Err(error) = &result {
                    self.set_find_status(FindStatus::NotFound, error.to_string());
                    self.editor_changed(result, window, cx);
                } else {
                    if let Err(error) = self.set_scintilla_shortcut_target(current.start..caret) {
                        self.editor_changed(Err(error), window, cx);
                        return;
                    }
                    self.editor_changed(result, window, cx);
                    if self.find_controls.replace_stop {
                        self.set_find_status(FindStatus::Message, Text::resource("find-status-replaced-without-continuing", "Replace: 1 occurrence was replaced."));
                        cx.notify();
                        return;
                    }
                    let outcome = self.find_next_with_origin(self.find_backward, true, window, cx);
                    if matches!(outcome, FindOutcome::Found | FindOutcome::NotFound) {
                        let (status, message) = match self.find_status {
                            FindStatus::EndReached => (FindStatus::EndReached,
                                Text::resource("find-status-replace-end-reached", "Replace: Reached document end, started from top.")),
                            FindStatus::TopReached => (FindStatus::TopReached,
                                Text::resource("find-status-replace-top-reached", "Replace: Reached document beginning, started from bottom.")),
                            _ if outcome == FindOutcome::NotFound => (FindStatus::Message,
                                Text::resource("find-status-replaced-next-not-found", "Replace: 1 occurrence was replaced. No more occurrences were found.")),
                            _ => (FindStatus::Message, Text::resource("find-status-replaced-next-found", "Replace: 1 occurrence was replaced. The next occurrence found.")),
                        };
                        self.set_find_status(status, message);
                    }
                }
            }
            Ok(Some((found, _))) => {
                if let Err(error) = self.set_scintilla_shortcut_target(found.range.clone()) {
                    self.editor_changed(Err(error), window, cx);
                    return;
                }
                let result = self.select_search_result(found.range, self.find_backward, window, cx);
                if result.is_err() {
                    self.editor_changed(result, window, cx);
                    return;
                }
                self.editor_changed(result, window, cx);
                self.set_find_status(FindStatus::NoMessage, "");
            }
            Ok(None) => {
                if self.find_options.wrap {
                    self.set_scintilla_shortcut_raw_target(if self.find_backward { bytes.len() as i128..0 } else { 0..bytes.len() as i128 });
                }
                self.set_find_status(FindStatus::NotFound,
                    Text::resource("find-status-replace-not-found", "Replace: no occurrence was found").with_scope(self.find_scope_resource()));
                self.set_find_scope_reason(0, false);
            }
            Err(error) => self.set_find_error(&error),
        }
        cx.notify();
    }

    pub(super) fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.replace_all(window, cx)) {
            return;
        }
        if !self.find_visible
            || !self.find_replace_mode
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().document().is_read_only() {
            self.set_find_status(FindStatus::NotFound, Text::resource("find-status-replaceall-readonly", "Replace All: Cannot replace text. The current document is read only"));
            cx.notify();
            return;
        }
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        let replacement = self.captured_search_input(crate::search_history::Field::Replace, cx);
        self.record_search_histories(
            &[
                crate::search_history::Field::Replace,
                crate::search_history::Field::Find,
            ],
            window,
            cx,
        );
        let options = self.find_options;
        let backwards = self.find_backward;
        let in_selection = self.find_in_selection_active;
        let outcome = self.editor_mut().replace_all_literal_scoped_with_target(
            &query,
            &replacement,
            options,
            backwards,
            in_selection,
        );
        if let Some(target) = outcome.last_target { self.set_scintilla_shortcut_raw_target(target); }
        match outcome.result {
            Ok(count) => {
                if count > 0 && in_selection && self.editor().document().selection().range().is_empty() {
                    self.find_in_selection_enabled = false;
                    if self.find_autocheck_threshold != 0 {
                        self.find_in_selection = false;
                    }
                }
                self.editor_changed(Ok(()), window, cx);
                self.set_find_status(FindStatus::Message,
                    Text::resource(if count == 1 { "find-status-replaceall-1-replaced" } else { "find-status-replaceall-nb-replaced" },
                        "Replace All: $INT_REPLACE$ occurrences were replaced").replace("$INT_REPLACE$", crate::finder::commafy(count))
                        .with_scope(self.find_scope_resource()));
                self.set_find_scope_reason(count, false);
            }
            Err(error) => self.set_find_replace_error(&error),
        }
        cx.notify();
    }

    pub(super) fn render_search_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("search-mode-tabs")
            .role(Role::TabList)
            .aria_label(self.find_dialog_title())
            .track_focus(&self.find_tabs_focus)
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                let current = view.find_tab_index();
                let index = match event.keystroke.key.as_str() {
                    "left" => current.saturating_sub(1),
                    "right" => (current + 1).min(4),
                    "home" => 0,
                    "end" => 4,
                    _ => return,
                };
                if index != current { view.select_find_tab(index, window, cx); }
                cx.stop_propagation();
            }))
            .h(px(25.))
            .flex()
            .items_end()
            .border_b_1()
            .border_color(rgb(0xd9d9d9))
            .children(
                [
                    (0_usize, "查找", 46.),
                    (1, "替换", 46.),
                    (2, "文件中查找", 72.),
                    (3, "项目中查找", 72.),
                    (4, "标记", 46.),
                ]
                .into_iter()
                .map(|(index, _, width)| {
                    let label = self.find_tab_title(index);
                    let selected = index == self.find_tab_index();
                    div()
                        .id(("search-mode", index))
                        .role(Role::Tab)
                        .aria_label(label)
                        .aria_selected(selected)
                        .relative()
                        .min_w(px(width + if selected { 4. } else { 0. }))
                        .px(px(6.))
                        .whitespace_nowrap()
                        .flex_shrink_0()
                        .h(px(if selected { 25. } else { 23. }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(rgb(0xd9d9d9))
                        .bg(rgb(if selected { 0xffffff } else { 0xf0f0f0 }))
                        .when(selected, |tab| tab.child(div().absolute().bottom(px(-1.))
                            .left(px(0.)).right(px(0.)).h(px(2.)).bg(rgb(0xffffff))))
                        .on_click(cx.listener(move |view, _, window, cx| view.select_find_tab(index, window, cx)))
                        .child(label)
                }),
            )
            .into_any_element()
    }

    fn find_tab_index(&self) -> usize {
        if self.files_search.visible { if self.find_project_mode { 3 } else { 2 } }
        else if self.find_mark_mode { 4 }
        else if self.find_replace_mode { 1 }
        else { 0 }
    }

    fn select_find_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index > 4 || self.file_busy || self.close_dialog || self.save_all_dialog { return; }
        self.close_search_history();
        self.close_find_swap_menu(cx);
        match index {
            0 | 1 | 4 => {
                self.find_visible = true;
                self.files_search.visible = false;
                self.find_replace_mode = index == 1;
                self.find_mark_mode = index == 4;
            }
            _ => self.open_file_search_tab(false, index == 3, window, cx),
        }
        self.find_query.update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }

    pub(super) fn render_find_check(
        &self,
        id: &'static str,
        option: FindCheck,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, checked) = match option {
            FindCheck::Backward => ("反向查找", self.find_backward),
            FindCheck::WholeWord => ("全词匹配(W)", self.find_options.whole_word),
            FindCheck::MatchCase => ("匹配大小写(C)", self.find_options.match_case),
            FindCheck::Wrap => ("循环查找(P)", self.find_options.wrap),
        };
        let enabled = !matches!(
            self.find_options.mode,
            crate::search::SearchMode::Regex { .. }
        ) || match option {
            FindCheck::WholeWord => false,
            FindCheck::Backward => self.find_regex_backward,
            _ => true,
        };
        let index = match option {
            FindCheck::Backward => 0, FindCheck::WholeWord => 1,
            FindCheck::MatchCase => 2, FindCheck::Wrap => 3,
        };
        let focus = self.find_controls.option_focus[index].clone();
        checkbox_enabled(id, label, checked, enabled)
            .when(enabled, |control| control.track_focus(&focus))
            .text_color(rgb(if enabled { 0x202020 } else { 0x808080 }))
            .a11y_synthetic_children(move |builder| {
                if !enabled {
                    builder.parent_node().set_disabled();
                }
            })
            .on_click(cx.listener(move |view, _, window, cx| {
                if enabled {
                    view.find_controls.option_focus[index].focus(window, cx);
                    view.toggle_find_check(option, cx);
                }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                if enabled && event.keystroke.key == "space" { view.toggle_find_check(option, cx); cx.stop_propagation(); }
            }))
            .into_any_element()
    }

    fn toggle_find_check(&mut self, option: FindCheck, cx: &mut Context<Self>) {
        let previous = (self.find_options, self.find_backward);
        match option {
            FindCheck::Backward => self.find_backward = !self.find_backward,
            FindCheck::WholeWord => self.find_options.whole_word = !self.find_options.whole_word,
            FindCheck::MatchCase => self.find_options.match_case = !self.find_options.match_case,
            FindCheck::Wrap => self.find_options.wrap = !self.find_options.wrap,
        }
        let (key, value) = match option {
            FindCheck::Backward => ("directionDown", !self.find_backward),
            FindCheck::WholeWord => ("matchWord", self.find_options.whole_word),
            FindCheck::MatchCase => ("matchCase", self.find_options.match_case),
            FindCheck::Wrap => ("wrap", self.find_options.wrap),
        };
        if !self.persist_find_option(key, value.into(), cx) { (self.find_options, self.find_backward) = previous; }
        cx.notify();
    }

    pub(super) fn render_find(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.find_visible {
            return None;
        }
        self.sync_find_field_labels(cx);
        Some(
            div()
                .id("find-dialog")
                .debug_selector(|| "find-dialog".into())
                .role(Role::Dialog)
                .aria_label(self.find_dialog_title())
                .key_context("MatrixFind")
                .size_full()
                .relative()
                .font_family(dialog_font::font_family())
                .text_size(gpui::px(dialog_font::base_size()))
                .line_height(dialog_font::line_height())
                .bg(rgb(0xffffff))
                .text_color(rgb(0x202020))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.render_search_tabs(cx))
                .child(input_row(
                    "查找目标(F):",
                    35.,
                    self.search_history_combo(crate::search_history::Field::Find, cx),
                ))
                .when(self.find_replace_mode, |dialog| {
                    dialog.child(self.render_find_swap(cx)).child(input_row(
                        "替换为(L):",
                        67.,
                        self.search_history_combo(crate::search_history::Field::Replace, cx),
                    ))
                })
                .when(self.find_mark_mode, |dialog| {
                    dialog
                        .children(
                            (!self.find_controls.compact)
                                .then_some([
                                    (true, "mark-bookmark", "标记所在行(M)", 93.),
                                    (false, "mark-purge", "清除上次标记", 114.),
                                ])
                                .into_iter()
                                .flatten()
                                .map(|(bookmark, id, label, top)| {
                                    checkbox(
                                        id,
                                        label,
                                        if bookmark {
                                            self.mark_bookmark
                                         } else {
                                             self.mark_purge
                                         },
                                     )
                                      .track_focus(&self.find_controls.option_focus[if bookmark { 6 } else { 7 }])
                                    .absolute()
                                    .left(px(21.))
                                    .top(px(top))
                                     .on_click(cx.listener(move |view, _, window, cx| view.find_option_command(if bookmark { 1616 } else { 1618 }, window, cx)))
                                     .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                                         if event.keystroke.key == "space" {
                                             view.find_option_command(if bookmark { 1616 } else { 1618 }, window, cx);
                                             cx.stop_propagation();
                                         }
                                     }))
                                }),
                        )
                        .child(
                            source_button(
                                "mark-all",
                                "全部标记",
                                if self.find_controls.compact { 36. } else { 68. },
                            )
                            .border_color(rgb(0x0078d7))
                            .on_click(cx.listener(
                                |view, event, window, cx| {
                                    if !view.warn_find_scope_change(event, window, cx) {
                                        view.mark_all(window, cx);
                                    }
                                },
                            )),
                        )
                        .child(
                            source_button(
                                "mark-clear",
                                "清除",
                                if self.find_controls.compact {
                                    68.
                                } else {
                                    99.
                                },
                            )
                            .on_click(cx.listener(
                                |view, event, window, cx| {
                                    if !view.warn_find_scope_change(event, window, cx) {
                                        view.clear_find_marks(window, cx);
                                    }
                                },
                            )),
                        )
                        .when(!self.find_controls.compact, |dialog| {
                            dialog.child(
                                source_button("mark-copy", "复制标记的文本", 131.).on_click(
                                    cx.listener(|view, _, _, cx| view.copy_marked_text(43061, cx)),
                                ),
                            )
                        })
                })
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(
                        div()
                            .absolute()
                            .left(px(21.))
                            .top(px(135.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(self.render_find_check("find-backward", FindCheck::Backward, cx))
                            .child(self.render_find_check(
                                "find-whole-word",
                                FindCheck::WholeWord,
                                cx,
                            ))
                            .child(self.render_find_check(
                                "find-match-case",
                                FindCheck::MatchCase,
                                cx,
                            ))
                            .child(self.render_find_check("find-wrap", FindCheck::Wrap, cx)),
                    )
                })
                .when(!self.find_mark_mode, |dialog| {
                    dialog.child(
                        source_button("find-next", if self.find_controls.two_buttons { "▼ 查找下一个(E)" } else { "查找下一个(E)" }, 36.)
                            .w(px(if self.find_controls.two_buttons {
                                121.
                            } else {
                                157.
                            }))
                            .border_color(rgb(0x0078d7))
                            .tooltip({
                                let text = self.find_text("shift-change-direction-tip", "Use Shift+Enter to search in the opposite direction");
                                move |_, cx| icon_tooltip::view(text, false, cx)
                            })
                            .on_click(cx.listener(|view, _, window, cx| {
                                    view.find_button_command("find-next", window.modifiers().shift, window, cx)
                            })),
                    )
                })
                .when(
                    !self.find_mark_mode && self.find_controls.two_buttons,
                    |dialog| {
                        dialog.child(
                            source_button_enabled("find-previous", "▲", 36., self.find_regex_backward || !matches!(self.find_options.mode, crate::search::SearchMode::Regex { .. }))
                                .right(px(168.))
                                .w(px(29.))
                                .aria_label(crate::localization::menu_command(&self.native_locale(), 43003).unwrap_or("Find Previous"))
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.find_next(true, window, cx)
                                })),
                        )
                    },
                )
                .when(!self.find_mark_mode, |dialog| {
                    dialog.child(
                        checkbox("find-two-buttons", "", self.find_controls.two_buttons)
                            .track_focus(&self.find_controls.option_focus[5])
                            .absolute()
                            .w(px(13.))
                            .gap(px(0.))
                            .right(px(19.))
                            .top(px(37.))
                            .aria_label(self.find_text("two-find-buttons-tip", "2 find buttons mode"))
                            .tooltip({
                                let text = self.find_text("two-find-buttons-tip", "2 find buttons mode");
                                move |_, cx| icon_tooltip::view(text, false, cx)
                            })
                            .on_click(
                                cx.listener(|view, _, window, cx| {
                                    view.find_controls.option_focus[5].focus(window, cx);
                                    view.toggle_find_control(0, cx);
                                }),
                            )
                            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "space" {
                                    view.toggle_find_control(0, cx);
                                    cx.stop_propagation();
                                }
                            })),
                    )
                })
                .when(
                    !self.find_replace_mode && !self.find_mark_mode,
                    |dialog| {
                        dialog
                            .child(source_button("find-count", "计数(T)", 68.).on_click(
                                cx.listener(|view, event, window, cx| {
                                    if !view.warn_find_scope_change(event, window, cx) {
                                        view.count_find(window, cx);
                                    }
                                }),
                            ))
                            .when(!self.find_controls.compact, |dialog| dialog.child(
                                source_button("find-all-current", "在当前文件中查找", 99.)
                                    .on_click(cx.listener(|view, event, window, cx| {
                                        if !view.warn_find_scope_change(event, window, cx) {
                                            view.find_all_documents(false, window, cx);
                                        }
                                    })),
                            ))
                            .when(!self.find_controls.compact, |dialog| dialog.child(
                                source_button("find-all-open", "查找所有打开文件(O)", 143.)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.find_all_documents(true, window, cx);
                                    })),
                            ))
                    },
                )
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(
                        div()
                            .absolute()
                            .right(px(36.))
                            .w(px(360.))
                            .top(px(if self.find_replace_mode { 96. } else { 65. }))
                            .h(px(if self.find_replace_mode { 30. } else if self.find_mark_mode { 60. } else { 72. }))
                            .border_1()
                            .border_color(rgb(0xdcdcdc)),
                    )
                })
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(
                        checkbox_enabled("find-in-selection", "选取范围内(I)", self.find_in_selection,
                            self.find_in_selection_enabled)
                            .when(self.find_in_selection_enabled, |control| control.track_focus(&self.find_controls.option_focus[4]))
                            .absolute()
                            .right(px(229.))
                            .w(px(158.))
                            .top(px(99.))
                            .a11y_synthetic_children({
                                let enabled = self.find_in_selection_enabled;
                                move |builder| {
                                    if !enabled {
                                        builder.parent_node().set_disabled();
                                    }
                                }
                            })
                            .text_color(rgb(if self.find_in_selection_enabled {
                                0x202020
                            } else {
                                0x808080
                            }))
                            .on_click(cx.listener(|view, _, window, cx| view.find_option_command(1632, window, cx)))
                            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                                if event.keystroke.key == "space" { view.find_option_command(1632, window, cx); cx.stop_propagation(); }
                            })),
                    )
                })
                .when(!self.find_controls.compact, |dialog| dialog.child(
                    source_button(
                        "find-close",
                        "关闭",
                        if self.find_mark_mode { 162. } else if self.find_replace_mode { 175. } else { 187. },
                    )
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.dismiss_floating(floating_windows::DialogKind::Find, window, cx)
                    })),
                ))
                .when(self.find_replace_mode, |dialog| {
                    dialog
                        .child(source_button("replace-one", "替换(R)", 68.).on_click(
                            cx.listener(|view, _, window, cx| view.replace_one(window, cx)),
                        ))
                        .when(!self.find_controls.compact, |dialog| dialog.child(source_button("replace-all", "全部替换(A)", 99.).on_click(
                            cx.listener(|view, event, window, cx| {
                                if !view.warn_find_scope_change(event, window, cx) {
                                    view.replace_all(window, cx);
                                }
                            }),
                        )))
                        .when(!self.find_controls.compact, |dialog| {
                            dialog.child(
                                source_button("replace-all-open", "替换所有打开文件(O)", 131.)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.confirm_replace_open_documents(window, cx)
                                    })),
                            )
                        })
                })
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(search_mode(self.find_options.mode, &self.find_mode_focus,
                        &self.find_controls.option_focus[8], cx))
                })
                .child(self.render_find_options(cx))
                .child(self.render_find_status("find-status"))
                .into_any_element(),
        )
    }
}
