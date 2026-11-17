use super::*;
use crate::search_history::{Field, History};


fn equivalent(a: &str, b: &str) -> bool {
    use objc2_foundation::{NSComparisonResult, NSString};
    NSString::from_str(a).localizedCaseInsensitiveCompare(&NSString::from_str(b))
        == NSComparisonResult::Same
}

pub(super) struct SearchHistoryState {
    histories: [History; 4],
    persisted: [serde_json::Value; 4],
    initialized: bool,
    open: Option<Field>,
    selected: Option<usize>,
    original: String,
    preview: String,
    scroll: gpui::ScrollHandle,
}

impl SearchHistoryState {
    pub(super) fn new() -> Self {
        Self {
            histories: Field::ALL
                .map(|field| History::load(field, &serde_json::Value::Null, equivalent)),
            persisted: Field::ALL.map(|_| serde_json::Value::Null),
            initialized: false,
            open: None,
            selected: None,
            original: String::new(),
            preview: String::new(),
            scroll: gpui::ScrollHandle::new(),
        }
    }
}

impl MatrixWindow {
    pub(super) fn captured_search_input(&self, field: Field, cx: &Context<Self>) -> String {
        field.capture(&self.search_history_input(field).read(cx).text())
    }
    pub(super) fn focus_search_history_input(&self, field: Field, cx: &mut Context<Self>) {
        let Some(handle) = self
            .floating_windows
            .get(&floating_windows::DialogKind::Find)
            .copied()
        else {
            return;
        };
        let input = self.search_history_input(field);
        cx.defer(move |cx| {
            if !cx.windows().contains(&handle.into()) {
                return;
            }
            if let Err(error) = handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx))
            }) {
                eprintln!("search field focus could not reach its window: {error}");
            }
        });
    }

    pub(super) fn configure_search_history(&mut self, settings: &serde_json::Value) {
        self.search_history.histories =
            Field::ALL.map(|field| History::load(field, &settings["FindHistory"], equivalent));
        self.search_history.persisted =
            Field::ALL.map(|field| settings["FindHistory"][field.key()].clone());
        self.search_history.initialized = false;
        self.search_history.open = None;
    }

    pub(super) fn initialize_search_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_history.initialized {
            return;
        }
        self.search_history.initialized = true;
        for field in Field::ALL {
            let value = self.search_history.histories[field.index()].initial_text();
            let input = self.search_history_input(field);
            if input.read(cx).text().is_empty() && !value.is_empty() {
                input.update(cx, |input, cx| input.set_text(value, window, cx));
            }
        }
    }

    pub(super) fn record_search_history(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_search_histories(&[field], window, cx);
    }

    pub(super) fn record_search_histories(
        &mut self,
        fields: &[Field],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut saved = Vec::with_capacity(fields.len());
        for &field in fields {
            let text = self.captured_search_input(field, cx);
            let input = self.search_history_input(field);
            let original = input.read(cx).text();
            if !text.is_empty() && original != text {
                input.update(cx, |input, cx| input.set_text(&text, window, cx));
            }
            self.search_history.histories[field.index()].record(&text, equivalent);
            if self.search_history.open == Some(field) && !text.is_empty() {
                self.search_history.selected = Some(0);
                self.search_history.preview = text.clone();
                self.search_history.scroll.scroll_to_item(0);
            }
            saved.push((
                field,
                self.search_history.histories[field.index()].saved(&text),
            ));
        }
        self.persist_search_histories(&saved, cx);
    }

    pub(super) fn save_search_history_on_close(&mut self, cx: &mut Context<Self>) {
        self.close_search_history();
        if !self.search_history.initialized {
            return;
        }
        let saved = Field::ALL.map(|field| {
            let current = self.search_history_input(field).read(cx).text();
            (
                field,
                self.search_history.histories[field.index()].saved(&current),
            )
        });
        self.persist_search_histories(&saved, cx);
    }

    fn persist_search_histories(&mut self, saved: &[(Field, Vec<String>)], cx: &mut Context<Self>) {
        let Some(store) = &self.settings_store else {
            return;
        };
        match store.update_settings(|settings| {
            if !settings["FindHistory"].is_null() && !settings["FindHistory"].is_object() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "FindHistory must be an object",
                ));
            }
            for (field, entries_to_save) in saved {
                let entries = &settings["FindHistory"][field.key()];
                if !entries.is_null()
                    && entries
                        .as_array()
                        .is_none_or(|entries| entries.iter().any(|entry| !entry.is_string()))
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "search history entries must be an array of strings",
                    ));
                }
                if entries != &self.search_history.persisted[field.index()] {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "search history changed by another window or process",
                    ));
                }
                settings["FindHistory"][field.key()] = serde_json::json!(entries_to_save);
            }
            Ok(())
        }) {
            Ok(settings) => {
                for (field, _) in saved {
                    self.search_history.persisted[field.index()] =
                        settings["FindHistory"][field.key()].clone();
                }
            }
            Err(error) => {
                let message = format!("Search history could not be saved: {error}");
                let owner = cx.weak_entity();
                cx.defer(move |cx| {
                    if let Err(error) = owner.update(cx, |owner, cx| {
                        owner.notify_editor_error(message.clone(), cx)
                    }) {
                        eprintln!("{message}; error report lost its window: {error}");
                    }
                });
            }
        }
    }

    fn open_search_history(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.search_history_input(field);
        let text = input.read(cx).text();
        let entries = self.search_history.histories[field.index()].entries();
        if self.search_history.open == Some(field) || entries.is_empty() {
            self.search_history.open = None;
        } else {
            self.search_history.open = Some(field);
            self.search_history.selected =
                entries.iter().position(|entry| equivalent(entry, &text));
            self.search_history.original = text.clone();
            self.search_history.preview = text;
            self.search_history
                .scroll
                .set_offset(gpui::Point::default());
            if let Some(index) = self.search_history.selected {
                self.search_history.scroll.scroll_to_item(index);
            }
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn move_search_history(
        &mut self,
        field: Field,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.search_history_input(field).read(cx).text();
        if current != self.search_history.preview {
            self.search_history.selected = None;
        }
        let entries = self.search_history.histories[field.index()].entries();
        if entries.is_empty() {
            return;
        }
        let index = if down {
            self.search_history
                .selected
                .map_or(0, |index| (index + 1).min(entries.len() - 1))
        } else {
            self.search_history
                .selected
                .map_or(entries.len() - 1, |index| index.saturating_sub(1))
        };
        let text = entries[index].clone();
        self.search_history.selected = Some(index);
        self.search_history.preview = text.clone();
        self.search_history.scroll.scroll_to_item(index);
        self.search_history_input(field)
            .update(cx, |input, cx| input.set_text(&text, window, cx));
        cx.notify();
    }

    fn accept_search_history(
        &mut self,
        field: Field,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.search_history.histories[field.index()]
            .entries()
            .get(index)
            .cloned()
        else {
            return;
        };
        self.search_history.open = None;
        self.search_history_input(field).update(cx, |input, cx| {
            input.set_text(&text, window, cx);
            input.focus(window, cx);
        });
        if matches!(field, Field::Find | Field::Replace) { self.find_field_warning(false, cx); }
        cx.notify();
    }

    fn accept_current_search_history(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.search_history_input(field);
        if input.read(cx).text() == self.search_history.preview
            && let Some(index) = self.search_history.selected
        {
            self.accept_search_history(field, index, window, cx);
        } else {
            self.search_history.open = None;
            input.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    fn cancel_search_history(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.search_history.original.clone();
        self.search_history.open = None;
        self.search_history_input(field).update(cx, |input, cx| {
            input.set_text(&text, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_search_history(&mut self) -> bool {
        self.search_history.open.take().is_some()
    }

    pub(super) fn search_history_combo(&self, field: Field, cx: &mut Context<Self>) -> AnyElement {
        use super::dialog_font::px;
        let key = field.key();
        let open = self.search_history.open == Some(field);
        let entries = self.search_history.histories[field.index()].entries();
        div()
            .relative()
            .w_full()
            .h(px(21.))
            .on_action(cx.listener(move |view, _: &SearchHistoryOpen, window, cx| {
                view.open_search_history(field, window, cx);
                cx.stop_propagation();
            }))
            .when(open, |combo| {
                combo
                    .key_context("MatrixSearchHistory")
                    .on_action(
                        cx.listener(move |view, _: &SearchHistoryAccept, window, cx| {
                            view.accept_current_search_history(field, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_action(
                        cx.listener(move |view, _: &SearchHistoryCancel, window, cx| {
                            view.cancel_search_history(field, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_action(cx.listener(move |view, _: &SearchHistoryUp, window, cx| {
                        view.move_search_history(field, false, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(move |view, _: &SearchHistoryDown, window, cx| {
                        view.move_search_history(field, true, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_action(
                        cx.listener(move |view, _: &SearchHistoryNextField, window, cx| {
                            view.accept_current_search_history(field, window, cx);
                            window.focus_next(cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_action(cx.listener(
                        move |view, _: &SearchHistoryPreviousField, window, cx| {
                            view.accept_current_search_history(field, window, cx);
                            window.focus_prev(cx);
                            cx.stop_propagation();
                        },
                    ))
            })
            .child(self.search_history_input(field))
            .child(
                div()
                    .id(format!("search-history-button-{key}"))
                    .debug_selector(move || format!("search-history-button-{key}"))
                    .role(Role::Button)
                    .aria_label(format!("{key} history"))
                    .absolute()
                    .right(px(1.))
                    .top(px(1.))
                    .w(px(18.))
                    .h(px(19.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x606060))
                    .child("⌄")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_search_history(field, window, cx);
                        cx.stop_propagation();
                    })),
            )
            .when(open && !entries.is_empty(), |combo| {
                combo.child(
                    gpui::deferred(
                        gpui::anchored()
                            .offset(gpui::point(px(0.), px(21.)))
                            .snap_to_window()
                            .child(
                                div()
                                    .id("search-history-popup")
                                    .debug_selector(|| "search-history-popup".into())
                                    .role(Role::ListBox)
                                    .w(px(255.))
                                    .max_h(px(180.))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.search_history.scroll)
                                    .border_1()
                                    .border_color(rgb(0x808080))
                                    .bg(rgb(0xffffff))
                                    .text_color(rgb(0x202020))
                                    .occlude()
                                    .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                                        view.search_history.open = None;
                                        cx.notify();
                                    }))
                                    .children(entries.iter().enumerate().map(|(index, text)| {
                                        div()
                                            .id(format!("search-history-item-{key}-{index}"))
                                            .debug_selector(move || {
                                                format!("search-history-item-{key}-{index}")
                                            })
                                            .role(Role::ListBoxOption)
                                            .aria_label(text.clone())
                                            .aria_selected(
                                                self.search_history.selected == Some(index),
                                            )
                                            .h(px(20.))
                                            .px(px(4.))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .when(
                                                self.search_history.selected == Some(index),
                                                |item| {
                                                    item.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                                                },
                                            )
                                            .child(text.clone())
                                            .on_click(cx.listener(move |view, _, window, cx| {
                                                view.accept_search_history(
                                                    field, index, window, cx,
                                                );
                                                cx.stop_propagation();
                                            }))
                                    })),
                            ),
                    )
                    .priority(2),
                )
            })
            .into_any_element()
    }
}
