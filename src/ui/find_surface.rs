use super::*;

#[derive(Clone, Copy)]
pub(super) enum FindCheck {
    Backward,
    WholeWord,
    MatchCase,
    Wrap,
}

pub(super) fn checkbox(
    id: &'static str,
    label: &'static str,
    checked: bool,
) -> gpui::Stateful<gpui::Div> {
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
                .rounded(px(2.))
                .border_color(rgb(if checked { 0x0078d4 } else { 0x767676 }))
                .bg(rgb(if checked { 0x0078d4 } else { 0xffffff }))
                .text_color(rgb(0xffffff))
                .flex()
                .items_center()
                .justify_center()
                .child(if checked { "✓" } else { "" }),
        )
        .child(label)
}

pub(super) fn input_row(label: &'static str, top: f32, field: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .left(px(3.))
        .right(px(248.))
        .top(px(top))
        .h(px(21.))
        .flex()
        .items_center()
        .gap(px(5.))
        .child(div().w(px(106.)).flex_shrink_0().text_right().child(label))
        .child(div().flex_1().min_w(px(0.)).child(field))
        .into_any_element()
}

pub(super) fn button(id: &'static str, label: &'static str, top: f32) -> gpui::Stateful<gpui::Div> {
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
        .child(label)
}

pub(super) fn search_mode(
    mode: crate::search::SearchMode,
    cx: &mut Context<MatrixWindow>,
) -> AnyElement {
    div()
        .id("find-search-mode")
        .role(Role::Group)
        .aria_label("Search Mode")
        .absolute()
        .left(px(9.))
        .top(px(213.))
        .w(px(300.))
        .h(px(78.))
        .child(
            div()
                .absolute()
                .top(px(6.))
                .bottom(px(0.))
                .w_full()
                .border_1()
                .border_color(rgb(0xd6d6d6)),
        )
        .child(
            div()
                .absolute()
                .left(px(6.))
                .top(px(0.))
                .px(px(2.))
                .bg(rgb(0xf9f9f9))
                .child("Search Mode"),
        )
        .children(
            [
                (
                    crate::search::SearchMode::Normal,
                    "find-normal-mode",
                    "Normal",
                    18.,
                ),
                (
                    crate::search::SearchMode::Extended,
                    "find-extended-mode",
                    r"Extended (\n, \r, \t, \0, \x...)",
                    38.,
                ),
            ]
            .into_iter()
            .map(|(value, id, label, top)| {
                let selected = mode == value;
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .role(Role::RadioButton)
                    .aria_label(label)
                    .aria_toggled(selected.into())
                    .absolute()
                    .left(px(9.))
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
                            .border_color(rgb(if selected { 0x0078d4 } else { 0x808080 }))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |radio| {
                                radio.child(div().size(px(7.)).rounded_full().bg(rgb(0x0078d4)))
                            }),
                    )
                    .child(label)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let previous = view.find_options.mode;
                        if previous == value {
                            return;
                        }
                        view.find_options.mode = value;
                        let number = match value {
                            crate::search::SearchMode::Normal => 0,
                            crate::search::SearchMode::Extended => 1,
                        };
                        if !view.persist_find_option("searchMode", number.into(), cx) {
                            view.find_options.mode = previous;
                        }
                        cx.notify();
                    }))
            }),
        )
        .into_any_element()
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
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
    ]
}

impl MatrixWindow {
    pub(super) fn configure_find(&mut self, settings: &serde_json::Value) {
        self.configure_finder(settings);
        self.configure_search_history(settings);
        let history = &settings["FindHistory"];
        self.find_options.mode = if history["searchMode"].as_u64() == Some(1) {
            crate::search::SearchMode::Extended
        } else {
            crate::search::SearchMode::Normal
        };
        self.find_options.match_case = history["matchCase"].as_bool().unwrap_or(false);
        self.find_options.whole_word = history["matchWord"].as_bool().unwrap_or(false);
        self.find_options.wrap = history["wrap"].as_bool().unwrap_or(true);
        self.find_backward = !history["directionDown"].as_bool().unwrap_or(true);
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

    fn selected_find_characters(&self) -> usize {
        let range = self.editor().selection_limits().range();
        if range.is_empty() {
            return 0;
        }
        let text = self.editor().display_snapshot();
        let slice = text.slice(range.start.0..range.end.0);
        crate::byte_text::units(slice.chunks().flat_map(str::bytes))
            .take(self.find_autocheck_threshold.max(self.find_fill_threshold) + 1)
            .count()
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
        let count = self.selected_find_characters();
        let selections = self.editor().display_selections();
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
        if self.find_fill_caret
            && self.editor().selection_limits().range().is_empty()
            && let Err(error) = self.editor_mut().expand_word_selection()
        {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        let count = self.selected_find_characters();
        if count != 0 && count <= self.find_fill_threshold {
            let text = self.editor().clipboard_text().text.replace('\0', " ");
            self.find_query
                .update(cx, |field, cx| field.set_text(&text, window, cx));
        }
    }

    fn find_scope_description(&self) -> &'static str {
        if self.find_in_selection_active {
            "in selected text"
        } else if self.find_options.wrap {
            "in entire file"
        } else if self.find_backward {
            "from start-of-file to caret"
        } else {
            "from caret to end-of-file"
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
        let result = crate::search::count_literal(
            &self.editor().committed_text(),
            &query,
            self.editor().selection_limits(),
            self.find_options,
            self.find_backward,
            self.find_in_selection_active,
        );
        self.find_message = match result {
            Ok(count) => {
                let digits = count.to_string();
                let mut grouped = String::new();
                for (index, digit) in digits.chars().enumerate() {
                    if index != 0 && (digits.len() - index).is_multiple_of(3) {
                        grouped.push(',');
                    }
                    grouped.push(digit);
                }
                format!(
                    "Count: {grouped} {} {}",
                    if count == 1 { "match" } else { "matches" },
                    self.find_scope_description()
                )
            }
            Err(error) => error.to_string(),
        };
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
        let prompt = window.prompt(gpui::PromptLevel::Warning, "Search warning",
            Some("The \"In selection\" checkbox state has been automatically modified.\r\nPlease verify the search condition before performing the action."), &["OK"], cx);
        self.file_busy = true;
        cx.spawn_in(window, async move |view, cx| {
            let answered = prompt.await;
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.file_busy = false;
                if let Err(error) = answered {
                    view.notify_editor_error(
                        format!("Search warning prompt disconnected: {error}"),
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
            self.notify_editor_error(
                "Find during active document composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        self.close_search_history();
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.find_visible = true;
        self.files_search.visible = false;
        self.find_replace_mode = replace;
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
        self.find_next_with_origin(backwards, false, window, cx);
    }

    fn find_next_with_origin(
        &mut self,
        backwards: bool,
        replacing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.find_next_with_origin(backwards, replacing, window, cx)
        }) {
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
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        if query.is_empty() {
            if !self.find_visible {
                self.show_find(window, cx);
            }
            return;
        }
        if !replacing {
            self.record_search_history(crate::search_history::Field::Find, window, cx);
        }
        if self.editor().marked_range().is_some() {
            self.find_message =
                "Find during active document composition is not implemented yet".into();
            cx.notify();
            return;
        }
        let text = self.editor().committed_text();
        let find = if replacing {
            crate::search::find_literal_for_replace
        } else {
            crate::search::find_literal
        };
        match find(
            &text,
            &query,
            self.editor().document().selection(),
            self.find_options,
            backwards,
        ) {
            Ok(Some(found)) => {
                let result = self.editor_mut().select(Selection {
                    anchor: found.range.start,
                    head: found.range.end,
                });
                self.editor_changed(result, window, cx);
                self.find_message = if found.wrapped {
                    "Wrapped around"
                } else {
                    "Found"
                }
                .into();
            }
            Ok(None) => self.find_message = "Cannot find the text".into(),
            Err(error) => self.find_message = error.to_string(),
        }
        cx.notify();
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
            self.find_message =
                "Replace during active document composition is not implemented yet".into();
            cx.notify();
            return;
        }
        self.find_message.clear();
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
            self.find_message =
                "Replace: Cannot replace text. The current document is read only.".into();
            cx.notify();
            return;
        }
        let replacement = self.find_options.mode.decode(&replacement);
        let current = self.editor().document().selection().range();
        match crate::search::find_literal_for_replace(
            &self.editor().committed_text(),
            &query,
            self.editor().selection(),
            self.find_options,
            self.find_backward,
        ) {
            Ok(Some(found)) if found.range == current => {
                let result = self
                    .editor_mut()
                    .replace_display_range(current, &replacement);
                if let Err(error) = &result {
                    self.find_message = error.to_string();
                    self.editor_changed(result, window, cx);
                } else {
                    self.editor_changed(result, window, cx);
                    self.find_next_with_origin(self.find_backward, true, window, cx);
                    self.find_message = format!("Replaced 1 occurrence. {}", self.find_message);
                }
            }
            Ok(Some(found)) => {
                let result = self.editor_mut().select(Selection {
                    anchor: found.range.start,
                    head: found.range.end,
                });
                self.editor_changed(result, window, cx);
                self.find_message = "Found".into();
            }
            Ok(None) => self.find_message = "Cannot find the text".into(),
            Err(error) => self.find_message = error.to_string(),
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
            self.find_message =
                "Replace: Cannot replace text. The current document is read only.".into();
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
        match self.editor_mut().replace_all_literal_scoped(
            &query,
            &replacement,
            options,
            backwards,
            in_selection,
        ) {
            Ok(count) => {
                if count > 0 && in_selection && self.editor().selection().range().is_empty() {
                    self.find_in_selection_enabled = false;
                    if self.find_autocheck_threshold != 0 {
                        self.find_in_selection = false;
                    }
                }
                self.editor_changed(Ok(()), window, cx);
                self.find_message = format!(
                    "Replace All: {count} {} {}",
                    if count == 1 {
                        "occurrence was replaced"
                    } else {
                        "occurrences were replaced"
                    },
                    self.find_scope_description()
                );
            }
            Err(error) => self.find_message = error.to_string(),
        }
        cx.notify();
    }

    pub(super) fn render_search_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("search-mode-tabs")
            .role(Role::TabList)
            .aria_label("Search modes")
            .h(px(22.))
            .flex()
            .items_end()
            .border_b_1()
            .border_color(rgb(0xd6d6d6))
            .children(
                [
                    (0_usize, "Find", 48.),
                    (1, "Replace", 52.),
                    (2, "Find in Files", 76.),
                ]
                .into_iter()
                .map(|(index, label, width)| {
                    let selected = match index {
                        0 => self.find_visible && !self.find_replace_mode,
                        1 => self.find_visible && self.find_replace_mode,
                        _ => self.files_search.visible,
                    };
                    div()
                        .id(("search-mode", index))
                        .role(Role::Tab)
                        .aria_label(label)
                        .aria_selected(selected)
                        .w(px(width))
                        .h(px(21.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(rgb(0xd6d6d6))
                        .bg(rgb(if selected { 0xf9f9f9 } else { 0xf0f0f0 }))
                        .on_click(cx.listener(move |view, _, window, cx| match index {
                            0 | 1 => {
                                view.close_search_history();
                                view.find_visible = true;
                                view.files_search.visible = false;
                                view.find_replace_mode = index == 1;
                                cx.notify();
                            }
                            _ => view.open_find_in_files(false, window, cx),
                        }))
                        .child(label)
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_find_check(
        &self,
        id: &'static str,
        option: FindCheck,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, checked) = match option {
            FindCheck::Backward => ("Backward direction", self.find_backward),
            FindCheck::WholeWord => ("Match whole word only", self.find_options.whole_word),
            FindCheck::MatchCase => ("Match case", self.find_options.match_case),
            FindCheck::Wrap => ("Wrap around", self.find_options.wrap),
        };
        checkbox(id, label, checked)
            .on_click(cx.listener(move |view, _, _, cx| {
                let previous = (view.find_options, view.find_backward);
                match option {
                    FindCheck::Backward => view.find_backward = !view.find_backward,
                    FindCheck::WholeWord => {
                        view.find_options.whole_word = !view.find_options.whole_word
                    }
                    FindCheck::MatchCase => {
                        view.find_options.match_case = !view.find_options.match_case
                    }
                    FindCheck::Wrap => view.find_options.wrap = !view.find_options.wrap,
                }
                let (key, value) = match option {
                    FindCheck::Backward => ("directionDown", !view.find_backward),
                    FindCheck::WholeWord => ("matchWord", view.find_options.whole_word),
                    FindCheck::MatchCase => ("matchCase", view.find_options.match_case),
                    FindCheck::Wrap => ("wrap", view.find_options.wrap),
                };
                if !view.persist_find_option(key, value.into(), cx) {
                    (view.find_options, view.find_backward) = previous;
                }
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn render_find(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.find_visible {
            return None;
        }
        Some(
            div()
                .id("find-dialog")
                .debug_selector(|| "find-dialog".into())
                .role(Role::Dialog)
                .aria_label(if self.find_replace_mode {
                    "Replace"
                } else {
                    "Find"
                })
                .key_context("MatrixFind")
                .size_full()
                .relative()
                .font_family("Helvetica")
                .text_size(px(11.))
                .line_height(px(16.))
                .bg(rgb(0xf9f9f9))
                .text_color(rgb(0x202020))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.render_search_tabs(cx))
                .child(input_row(
                    "Find what:",
                    32.,
                    self.search_history_combo(crate::search_history::Field::Find, cx),
                ))
                .when(self.find_replace_mode, |dialog| {
                    dialog.child(input_row(
                        "Replace with:",
                        61.,
                        self.search_history_combo(crate::search_history::Field::Replace, cx),
                    ))
                })
                .child(
                    div()
                        .absolute()
                        .left(px(18.))
                        .top(px(126.))
                        .flex()
                        .flex_col()
                        .child(self.render_find_check("find-backward", FindCheck::Backward, cx))
                        .child(self.render_find_check("find-whole-word", FindCheck::WholeWord, cx))
                        .child(self.render_find_check("find-match-case", FindCheck::MatchCase, cx))
                        .child(self.render_find_check("find-wrap", FindCheck::Wrap, cx)),
                )
                .child(button("find-next", "Find Next", 32.).on_click(cx.listener(
                    |view, _, window, cx| view.find_next(view.find_backward, window, cx),
                )))
                .when(!self.find_replace_mode, |dialog| {
                    dialog
                        .child(button("find-count", "Count", 61.).on_click(cx.listener(
                            |view, event, window, cx| {
                                if !view.warn_find_scope_change(event, window, cx) {
                                    view.count_find(window, cx);
                                }
                            },
                        )))
                        .child(
                            button("find-all-current", "Find All in Current Document", 91.)
                                .h(px(32.))
                                .on_click(cx.listener(|view, event, window, cx| {
                                    if !view.warn_find_scope_change(event, window, cx) {
                                        view.find_all_documents(false, window, cx);
                                    }
                                })),
                        )
                        .child(
                            button("find-all-open", "Find All in All Opened Documents", 132.)
                                .h(px(32.))
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.find_all_documents(true, window, cx);
                                })),
                        )
                })
                .child(
                    div()
                        .absolute()
                        .left(px(275.))
                        .right(px(33.))
                        .top(px(if self.find_replace_mode { 87. } else { 57. }))
                        .h(px(if self.find_replace_mode { 28. } else { 68. }))
                        .border_1()
                        .border_color(rgb(0xd6d6d6)),
                )
                .child(
                    checkbox("find-in-selection", "In selection", self.find_in_selection)
                        .absolute()
                        .left(px(284.))
                        .top(px(90.))
                        .text_color(rgb(if self.find_in_selection_enabled {
                            0x202020
                        } else {
                            0x808080
                        }))
                        .on_click(cx.listener(|view, _, _, cx| {
                            if view.find_in_selection_enabled {
                                view.find_in_selection = !view.find_in_selection;
                                view.find_in_selection_active =
                                    view.find_in_selection_enabled && view.find_in_selection;
                                view.find_scope_auto_changed = false;
                                cx.notify();
                            }
                        })),
                )
                .child(button("find-close", "Close", 172.).on_click(cx.listener(
                    |view, _, window, cx| {
                        view.dismiss_floating(floating_windows::DialogKind::Find, window, cx)
                    },
                )))
                .when(self.find_replace_mode, |dialog| {
                    dialog
                        .child(
                            button("replace-one", "Replace", 61.)
                                .aria_label("Replace current match")
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.replace_one(window, cx)),
                                ),
                        )
                        .child(
                            button("replace-all", "Replace All", 91.).on_click(cx.listener(
                                |view, event, window, cx| {
                                    if !view.warn_find_scope_change(event, window, cx) {
                                        view.replace_all(window, cx);
                                    }
                                },
                            )),
                        )
                })
                .child(search_mode(self.find_options.mode, cx))
                .child(
                    div()
                        .id("find-status")
                        .role(Role::Status)
                        .aria_label(self.find_message.clone())
                        .absolute()
                        .bottom(px(0.))
                        .w_full()
                        .h(px(22.))
                        .px(px(9.))
                        .border_t_1()
                        .border_color(rgb(0xd6d6d6))
                        .truncate()
                        .child(self.find_message.clone()),
                )
                .into_any_element(),
        )
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn unicode_casefold_controls_count_find_and_replace_expanded_document_matches(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("É ß SS ß");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("ss", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let count = dialog.debug_bounds("find-count").unwrap();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.find_message, "Count: 3 matches in entire file")
        });
        let next = dialog.debug_bounds("find-next").unwrap();
        dialog.simulate_click(next.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(3)..ByteOffset(5)
            )
        });
        cx.dispatch_action(ShowReplace);
        view.update_in(cx, |view, window, cx| {
            view.find_query
                .update(cx, |field, cx| field.set_text("ss", window, cx));
            view.replace_input
                .update(cx, |field, cx| field.set_text("X", window, cx));
        });
        cx.run_until_parked();
        let all = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "É X X X")
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "É ß SS ß");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn find_history_records_reuses_and_restores_real_combo_entries_without_editing_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.update_settings(|settings| {
            settings["FindHistory"] = serde_json::json!({"finds": ["Second", "First"], "replaces": ["", "old"], "future": 42});
            settings["Searching"] = serde_json::json!({"fillFindFieldWithSelected": false});
            Ok(())
        }).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.editor_mut().set_read_only(true);
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("FIRST", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let count = dialog.debug_bounds("find-count").unwrap();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        let arrow = dialog
            .debug_bounds("search-history-button-finds")
            .expect("functional combo arrow");
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        let old = dialog.debug_bounds("search-history-item-finds-1").unwrap();
        dialog.simulate_click(old.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "Second");
            assert_eq!(view.editor().committed_text(), "unchanged");
            assert!(!view.editor().document().can_undo());
        });
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(
            saved["FindHistory"]["finds"],
            serde_json::json!(["FIRST", "Second"])
        );
        assert_eq!(saved["FindHistory"]["future"], 42);
    }

    #[gpui::test]
    fn changing_one_search_option_preserves_other_instance_changes(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.load_settings().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        ConfigStore::under_home(home.path())
            .update_settings(|settings| {
                settings["FindHistory"]["searchMode"] = 1.into();
                Ok(())
            })
            .unwrap();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let match_case = dialog.debug_bounds("find-match-case").unwrap();
        dialog.simulate_click(match_case.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(saved["FindHistory"]["searchMode"], 1);
        assert_eq!(saved["FindHistory"]["matchCase"], true);
    }

    #[gpui::test]
    fn invalid_search_settings_do_not_get_overwritten_by_mode_controls(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        store
            .update_settings(|settings| {
                settings["FindHistory"] = serde_json::json!(["future-shape"]);
                Ok(())
            })
            .unwrap();
        let path = store.root().join("settings.json");
        let before = std::fs::read(&path).unwrap();
        let settings = store.load_settings().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let extended = dialog.debug_bounds("find-extended-mode").unwrap();
        dialog.simulate_click(extended.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), before);
        view.read_with(cx, |view, _| {
            assert!(view.find_options.mode == crate::search::SearchMode::Normal);
            assert!(view.find_message.contains("FindHistory must be an object"));
        });
    }

    #[gpui::test]
    fn search_mode_and_existing_flags_restore_from_private_json_without_losing_unknown_fields(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        store.update_settings(|settings| {
            settings["FindHistory"] = serde_json::json!({"searchMode": 0, "matchCase": true, "matchWord": true, "wrap": false, "directionDown": false, "fifRecuisive": false, "fifInHiddenFolder": true, "future": {"keep": 42}});
            Ok(())
        }).unwrap();
        let settings = store.load_settings().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let extended = dialog.debug_bounds("find-extended-mode").unwrap();
        dialog.simulate_click(extended.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(saved["FindHistory"]["searchMode"], 1);
        assert_eq!(saved["FindHistory"]["future"]["keep"], 42);
        view.update(cx, |view, _| view.configure_find(&saved));
        view.read_with(cx, |view, _| {
            assert!(view.find_options.mode == crate::search::SearchMode::Extended);
            assert!(view.find_options.match_case);
            assert!(view.find_options.whole_word);
            assert!(!view.find_options.wrap);
            assert!(view.find_backward);
        });
    }

    #[gpui::test]
    fn extended_mode_controls_find_count_all_and_replacement_across_tabs(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("a\tb\r\nc\tz");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text(r"\t", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let extended = dialog
            .debug_bounds("find-extended-mode")
            .expect("functional Extended radio");
        dialog.simulate_click(extended.center(), gpui::Modifiers::default());
        let count = dialog.debug_bounds("find-count").unwrap();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.find_message, "Count: 2 matches in entire file")
        });
        let next = dialog.debug_bounds("find-next").unwrap();
        dialog.simulate_click(next.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(1)..ByteOffset(2)
            )
        });
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-search-0").is_some());
        assert!(cx.debug_bounds("finder-hit-0-0-1").is_some());
        cx.dispatch_action(ShowReplace);
        view.update_in(cx, |view, window, cx| {
            view.find_query
                .update(cx, |field, cx| field.set_text(r"\t", window, cx));
            view.replace_input
                .update(cx, |field, cx| field.set_text(r"\0", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let replace = dialog.debug_bounds("replace-one").unwrap();
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "a\0b\r\nc\tz")
        });
        let replace_all = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(replace_all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "a\0b\r\nc\0z")
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "a\0b\r\nc\tz")
        });
    }

    #[gpui::test]
    fn current_document_find_all_uses_source_whole_scope_and_readonly_result_navigation(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one one\nONE");
            view.editor_mut().insert_text("x").unwrap();
            assert!(view.editor_mut().undo());
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(4),
                })
                .unwrap();
            view.editor_mut().set_read_only(true);
            view.find_options.wrap = false;
            view.find_backward = true;
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let find_all = dialog
            .debug_bounds("find-all-current")
            .expect("source current document Find All button");
        dialog.simulate_click(find_all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-search-0").is_some());
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        assert!(cx.debug_bounds("finder-hit-0-0-1").is_some());
        assert!(cx.debug_bounds("finder-hit-0-0-2").is_none());
        view.read_with(cx, |view, _| {
            assert!(!view.find_visible);
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(4)
                }
            );
            assert_eq!(view.editor().committed_text(), "one one\nONE");
            assert!(view.editor().document().can_redo());
            assert!(!view.editor().document().can_undo());
            assert!(!view.workspace.active().is_modified());
        });
        cx.simulate_click(hit.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().selection().head, ByteOffset(4))
        });
        let prefix = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: prefix,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: prefix,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(3)
            );
            assert!(view.editor().document().can_redo());
            assert!(view.editor().is_read_only());
        });
        cx.simulate_event(gpui::MouseDownEvent {
            position: hit.center(),
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: hit.center(),
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(4)..ByteOffset(7)
            )
        });
        cx.simulate_click(hit.center(), gpui::Modifiers::default());
        view.update(cx, |view, _| {
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(3),
                })
                .unwrap()
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(4)..ByteOffset(7)
            )
        });
        cx.simulate_click(hit.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.debug_bounds("files-results").is_none());
    }

    #[gpui::test]
    fn find_activation_during_preedit_uses_the_matching_selection_snapshot(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let document_window = cx.window_handle();
        view.update_in(cx, |view, window, cx| view.show_find(window, cx));
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        document_window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        view.update(cx, |view, _| {
            view.editor_mut()
                .mark_display_range(None, "中", Some(0..1))
                .unwrap();
        });
        handle
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert!(view.editor().has_composition());
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn find_prefill_uses_source_character_limits_caret_and_nul_conversion(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("alpha beta");
            view.editor_mut().select(Selection { anchor: ByteOffset(2), head: ByteOffset(2) }).unwrap();
            view.show_find(window, cx);
            assert_eq!(view.find_query.read(cx).text(), "alpha");
            assert_eq!(view.editor().selection(), Selection { anchor: ByteOffset(0), head: ByteOffset(5) });
            assert!(!view.editor().document().can_undo());
            view.configure_find(&serde_json::json!({"Searching": {"fillFindWhatThreshold": 4}}));
            view.find_query.update(cx, |field, cx| field.set_text("kept", window, cx));
            view.show_find(window, cx);
            assert_eq!(view.find_query.read(cx).text(), "kept");
            *view.editor_mut() = EditorSession::new("a\0b");
            view.editor_mut().select(Selection { anchor: ByteOffset(0), head: ByteOffset(3) }).unwrap();
            view.show_find(window, cx);
            assert_eq!(view.find_query.read(cx).text(), "a b");
            view.configure_find(&serde_json::json!({"Searching": {"fillFindWhatThreshold": 0, "inSelectionAutocheckThreshold": 1025}}));
            assert_eq!(view.find_fill_threshold, 1024);
            assert_eq!(view.find_autocheck_threshold, 1024);
            *view.editor_mut() = EditorSession::new(&"a".repeat(1025));
            view.editor_mut().select(Selection { anchor: ByteOffset(0), head: ByteOffset(1025) }).unwrap();
            view.show_find(window, cx);
            assert_eq!(view.find_query.read(cx).text(), "a b");
            view.configure_find(&serde_json::json!({"Searching": {"fillFindWhatThreshold": 16384, "inSelectionAutocheckThreshold": -1}}));
            assert_eq!(view.find_fill_threshold, 1024);
            assert_eq!(view.find_autocheck_threshold, 1024);
        });
    }

    #[gpui::test]
    fn opening_find_evaluates_scope_before_expanding_the_caret_word(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&"a".repeat(1024));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text().len(), 1024);
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(1024)
            );
            assert!(!view.find_in_selection_enabled);
            assert!(!view.find_in_selection);
        });
    }

    #[gpui::test]
    fn find_activation_disables_source_multiple_and_rectangle_selections(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let document_window = cx.window_handle();
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one\none");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let check = dialog.debug_bounds("find-in-selection").unwrap();
        for rectangle in [false, true] {
            view.update(cx, |view, _| {
                view.editor_mut()
                    .set_selections(
                        crate::editor::SelectionSet::from_parts(
                            vec![
                                Selection {
                                    anchor: ByteOffset(0),
                                    head: ByteOffset(3),
                                }
                                .into(),
                                Selection {
                                    anchor: ByteOffset(4),
                                    head: ByteOffset(7),
                                }
                                .into(),
                            ],
                            0,
                            rectangle,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            });
            document_window
                .update(cx, |_, window, _| window.activate_window())
                .unwrap();
            cx.run_until_parked();
            handle
                .update(cx, |_, window, _| window.activate_window())
                .unwrap();
            cx.run_until_parked();
            dialog.simulate_click(check.center(), gpui::Modifiers::default());
            view.read_with(cx, |view, _| {
                assert!(!view.find_in_selection_enabled);
                assert!(!view.find_in_selection);
            });
        }
    }

    #[gpui::test]
    fn activation_click_warns_only_when_source_auto_check_changed(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let document_window = cx.window_handle();
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&"a".repeat(1024));
            view.configure_find(
                &serde_json::json!({"Searching": {"fillFindFieldWithSelected": false}}),
            );
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("a", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let count = dialog.debug_bounds("find-count").unwrap();
        document_window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        view.update(cx, |view, _| {
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(1024),
                    head: ByteOffset(0),
                })
                .unwrap()
        });
        cx.run_until_parked();
        handle
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        dialog.simulate_event(gpui::MouseDownEvent {
            position: count.center(),
            modifiers: gpui::Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: true,
        });
        dialog.simulate_mouse_up(
            count.center(),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        assert_eq!(cx.pending_prompt(), Some(("Search warning".into(), "The \"In selection\" checkbox state has been automatically modified.\r\nPlease verify the search condition before performing the action.".into())));
        view.read_with(cx, |view, _| assert!(view.find_message.is_empty()));
        cx.simulate_prompt_answer("OK");
        cx.run_until_parked();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.find_message, "Count: 1,024 matches in selected text")
        });
        document_window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        view.update(cx, |view, _| {
            view.editor_mut().select(Selection::default()).unwrap()
        });
        cx.run_until_parked();
        handle
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        assert!(!cx.has_pending_prompt());
        view.read_with(cx, |view, _| {
            assert_eq!(view.find_message, "Count: 1,024 matches in entire file")
        });
    }

    #[gpui::test]
    fn selection_replace_all_retains_source_span_and_disables_empty_result(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one one ONE");
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(4),
                })
                .unwrap();
            view.show_replace(window, cx);
            view.replace_input
                .update(cx, |field, cx| field.set_text("X", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let check = dialog.debug_bounds("find-in-selection").unwrap();
        let replace = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(check.center(), gpui::Modifiers::default());
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.editor().committed_text(), "one X ONE");
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(4),
                    head: ByteOffset(5)
                }
            );
            assert!(view.editor_mut().undo());
            assert!(!view.editor().document().can_undo());
            view.replace_input
                .update(cx, |field, cx| field.set_text("", window, cx));
        });
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one  ONE");
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(4),
                    head: ByteOffset(4)
                }
            );
            assert!(!view.find_in_selection);
            assert!(!view.find_in_selection_enabled);
        });
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one  ONE");
            assert_eq!(
                view.find_message,
                "Replace All: 0 occurrences were replaced in selected text"
            );
        });
    }

    #[gpui::test]
    fn search_tab_changes_do_not_refill_the_query_from_document_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("needle", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let root = dialog.debug_bounds("find-dialog").unwrap();
        for x in [75., 140., 25.] {
            dialog.simulate_click(
                root.origin + gpui::point(px(x), px(10.)),
                gpui::Modifiers::default(),
            );
            cx.run_until_parked();
            view.read_with(cx, |view, cx| {
                assert_eq!(view.find_query.read(cx).text(), "needle")
            });
        }
    }

    #[gpui::test]
    fn find_activation_uses_source_threshold_and_preserves_manual_zero_threshold(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let document_window = cx.window_handle();
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&"a".repeat(1024));
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(1023),
                    head: ByteOffset(0),
                })
                .unwrap();
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        view.read_with(cx, |view, _| {
            assert!(view.find_in_selection_enabled);
            assert!(!view.find_in_selection);
        });
        view.update(cx, |view, _| {
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(1024),
                    head: ByteOffset(0),
                })
                .unwrap();
        });
        document_window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        handle
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        handle
            .update(cx, |_, window, _| {
                assert!(window.is_window_active(), "Find activation delivered")
            })
            .unwrap();
        view.read_with(cx, |view, _| {
            assert!(
                view.find_in_selection,
                "selected={} enabled={} threshold={} changed={}",
                view.selected_find_characters(),
                view.find_in_selection_enabled,
                view.find_autocheck_threshold,
                view.find_scope_auto_changed
            )
        });
        let check = dialog.debug_bounds("find-in-selection").unwrap();
        dialog.simulate_click(check.center(), gpui::Modifiers::default());
        let count = dialog.debug_bounds("find-count").unwrap();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| assert!(!view.find_in_selection));
        view.update(cx, |view, _| {
            view.configure_find(
                &serde_json::json!({"Searching": {"inSelectionAutocheckThreshold": 0}}),
            );
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(1),
                })
                .unwrap();
        });
        document_window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        handle
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        dialog.simulate_click(check.center(), gpui::Modifiers::default());
        view.update(cx, |view, _| {
            view.editor_mut().select(Selection::default()).unwrap();
        });
        document_window
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        handle
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert!(view.find_in_selection);
            assert!(!view.find_in_selection_enabled);
            assert!(view.find_message.ends_with("in entire file"));
        });
    }

    #[gpui::test]
    fn count_and_in_selection_use_source_scope_without_editing_readonly_text_or_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one one ONE");
            view.editor_mut().insert_text("x").unwrap();
            assert!(view.editor_mut().undo());
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(4),
                })
                .unwrap();
            view.editor_mut().set_read_only(true);
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("one", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let count = dialog
            .debug_bounds("find-count")
            .expect("source Count button");
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.find_message, "Count: 3 matches in entire file")
        });
        let in_selection = dialog
            .debug_bounds("find-in-selection")
            .expect("source In selection checkbox");
        dialog.simulate_click(in_selection.center(), gpui::Modifiers::default());
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.find_message, "Count: 1 match in selected text");
            assert_eq!(view.editor().committed_text(), "one one ONE");
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(4)
                }
            );
            assert!(!view.editor().document().can_undo());
            assert!(view.editor().document().can_redo());
            assert!(!view.workspace.active().is_modified());
            assert!(view.editor().is_read_only());
        });
    }

    #[gpui::test]
    fn replace_all_uses_backward_scope_when_wrap_is_off(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one X one X one");
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(6),
                    head: ByteOffset(9),
                })
                .unwrap();
            view.find_options.match_case = true;
            view.find_options.wrap = false;
            view.find_backward = true;
            view.replace_input
                .update(cx, |field, cx| field.set_text("Z", window, cx));
            view.show_replace(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let replace = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.editor().committed_text(), "Z X Z X one");
            view.editor_focus.focus(window, cx);
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one X one X one");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn backward_replace_retains_source_self_match_inside_the_replacement(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo foo");
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(4),
                    head: ByteOffset(7),
                })
                .unwrap();
            view.find_options.match_case = true;
            view.find_options.wrap = false;
            view.find_backward = true;
            view.replace_input
                .update(cx, |field, cx| field.set_text("fooX", window, cx));
            view.show_replace(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let replace = dialog.debug_bounds("replace-one").unwrap();
        for expected in ["foo fooX", "foo fooXX"] {
            dialog.simulate_click(replace.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), expected);
                assert_eq!(
                    view.editor().selection().range(),
                    ByteOffset(4)..ByteOffset(7)
                );
            });
        }
        view.update_in(cx, |view, window, cx| view.editor_focus.focus(window, cx));
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "foo fooX")
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "foo foo")
        });
    }

    #[gpui::test]
    fn find_and_replace_use_reference_input_and_button_columns(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| view.show_find(window, cx));
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let root = dialog.debug_bounds("find-dialog").unwrap();
        let query = dialog.debug_bounds("find-query").unwrap();
        let next = dialog.debug_bounds("find-next").unwrap();
        assert_eq!(root.size.width, px(617.));
        assert_eq!(query.left() - root.left(), px(114.));
        assert_eq!(query.top() - root.top(), px(32.));
        assert_eq!(query.size, size(px(255.), px(21.)));
        assert_eq!(next.left() - root.left(), px(447.));
        assert_eq!(next.size, size(px(137.), px(21.)));
        view.update_in(cx, |view, window, cx| view.show_replace(window, cx));
        let root = dialog.debug_bounds("find-dialog").unwrap();
        let replacement = dialog.debug_bounds("replace-input").unwrap();
        let replace = dialog.debug_bounds("replace-one").unwrap();
        let replace_all = dialog.debug_bounds("replace-all").unwrap();
        assert_eq!(replacement.left() - root.left(), px(114.));
        assert_eq!(replacement.top() - root.top(), px(61.));
        assert_eq!(replacement.size, size(px(255.), px(21.)));
        assert_eq!(replace.left(), next.left());
        assert_eq!(replace_all.left(), next.left());
        assert_eq!(replace.top() - root.top(), px(61.));
        assert_eq!(replace_all.top() - root.top(), px(91.));
    }

    #[gpui::test]
    fn backward_dialog_search_keeps_f3_direction_and_replaces_the_selected_match(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("one X one");
            view.editor_mut()
                .move_to_document_edge(true, false)
                .unwrap();
            view.find_options.match_case = true;
            view.find_options.wrap = false;
            view.find_query
                .update(cx, |field, cx| field.set_text("one", window, cx));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let backward = dialog
            .debug_bounds("find-backward")
            .expect("source backward option");
        dialog.simulate_click(backward.center(), gpui::Modifiers::default());
        let next = dialog.debug_bounds("find-next").unwrap();
        dialog.simulate_click(next.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(3)
            )
        });
        for (key, expected) in [("f3", 6..9), ("enter", 0..3), ("shift-enter", 6..9)] {
            dialog.simulate_keystrokes(key);
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor().selection().range(),
                    ByteOffset(expected.start)..ByteOffset(expected.end),
                    "{key}"
                )
            });
        }
        view.update_in(cx, |view, window, cx| {
            view.replace_input
                .update(cx, |field, cx| field.set_text("Z", window, cx));
            view.show_replace(window, cx);
        });
        let replace = dialog.debug_bounds("replace-one").unwrap();
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.editor().committed_text(), "one X Z");
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(3)
            );
            view.editor_focus.focus(window, cx);
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one X one");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn normal_whole_word_control_filters_find_and_replace_without_extra_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("xfoo foo foo_");
            view.find_options.match_case = true;
            view.find_options.wrap = false;
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let whole = dialog
            .debug_bounds("find-whole-word")
            .expect("source whole-word option");
        dialog.simulate_click(whole.center(), gpui::Modifiers::default());
        let next = dialog.debug_bounds("find-next").unwrap();
        dialog.simulate_click(next.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(5),
                    head: ByteOffset(8)
                }
            );
            assert_eq!(view.editor().committed_text(), "xfoo foo foo_");
            assert!(!view.editor().document().can_undo());
        });
        view.update_in(cx, |view, window, cx| {
            view.replace_input
                .update(cx, |field, cx| field.set_text("X", window, cx));
            view.show_replace(window, cx);
        });
        let replace = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            assert_eq!(view.editor().committed_text(), "xfoo X foo_");
            view.editor_focus.focus(window, cx);
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "xfoo foo foo_");
            assert!(!view.editor().document().can_undo());
        });
    }
}
