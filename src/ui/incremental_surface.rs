use super::*;
use std::sync::Arc;

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
struct IncrementalRepeat;

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
struct IncrementalOption(pub usize);

fn incremental_button(id: &'static str, label: &'static str, index: isize, cx: &mut Context<MatrixWindow>) -> gpui::Stateful<gpui::Div> {
    div().id(id).role(Role::Button).aria_label(label).focusable().tab_index(index)
        .on_click(cx.listener(move |view, _, window, cx| view.incremental_button_command(id, window, cx)))
        .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "space" {
                view.incremental_button_command(id, window, cx);
                cx.stop_propagation();
            }
        }))
        .on_action(cx.listener(move |view, _: &FindDialogNext, window, cx| {
            view.incremental_button_command(id, window, cx);
            cx.stop_propagation();
        }))
}

pub(super) struct State {
    visible: bool,
    input: gpui::Entity<text_field::TextField>,
    match_case: bool,
    highlight: bool,
    count: bool,
    top: bool,
    previous: String,
    message: String,
    status: Option<&'static str>,
    matches: Vec<std::ops::Range<ByteOffset>>,
    counted_revision: Option<Arc<()>>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<MatrixWindow>) -> Self {
        let input = cx.new(|cx| text_field::TextField::compact("查找", "incremental-query", cx).without_history_arrow().with_tab_index(2));
        cx.observe_in(&input, window, |view, _, window, cx| {
            let query = view.incremental.input.read(cx).text();
            if view.incremental.visible && query != view.incremental.previous {
                view.incremental.previous = query;
                view.incremental.matches.clear();
                view.run_incremental_search(None, view.incremental.match_case, window, cx);
            }
        })
        .detach();
        Self {
            visible: false,
            input,
            match_case: false,
            highlight: false,
            count: false,
            top: false,
            previous: String::new(),
            message: String::new(),
            status: None,
            matches: Vec::new(),
            counted_revision: None,
        }
    }
}

impl MatrixWindow {
    pub(super) fn invalidate_incremental_count(&mut self) {
        if self.incremental.visible && !self.incremental.matches.is_empty() {
            self.incremental.matches.clear();
            self.incremental.counted_revision = None;
            self.incremental.message.clear();
            self.incremental.status = None;
        }
    }

    pub(super) fn invalidate_changed_incremental_count(&mut self) {
        let revision = self.editor().document().text_revision();
        if self.incremental.counted_revision.as_ref().is_some_and(|counted| !Arc::ptr_eq(counted, &revision)) {
            self.invalidate_incremental_count();
        }
    }

    pub(super) fn configure_incremental(&mut self, settings: &serde_json::Value) {
        let history = &settings["FindHistory"];
        self.incremental.match_case = history["incrementalMatchCase"].as_bool().unwrap_or(false);
        self.incremental.highlight = history["incrementalHighlightAll"]
            .as_bool()
            .unwrap_or(false);
        self.incremental.count = history["incrementalCount"].as_bool().unwrap_or(false);
        self.incremental.top = history["incrementalSearchBarAtTop"]
            .as_bool()
            .unwrap_or(false);
    }

    pub(super) fn incremental_at_top(&self) -> bool {
        self.incremental.top
    }

    fn close_incremental(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.incremental.visible = false;
        self.incremental.matches.clear();
        let result = self
            .editor_mut()
            .document_mut()
            .set_indicators(6, Vec::new(), true);
        self.editor_changed(result, window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn open_incremental(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.incremental.visible = true;
        let bytes = self.editor().document().byte_snapshot();
        let range = self.editor().document().selection().range();
        if !range.is_empty() {
            match bytes.slice(range.start.0..range.end.0) {
                Ok(selected) => {
                    let selected = selected.to_vec();
                    let text = String::from_utf8_lossy(&selected);
                    if !text.is_empty() {
                        self.incremental.input.update(cx, |field, cx| field.set_text(&text, window, cx));
                    }
                }
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            }
        }
        self.incremental
            .input
            .update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }

    fn run_incremental(
        &mut self,
        direction: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_incremental_search(direction, false, window, cx);
    }

    fn run_incremental_search(
        &mut self,
        direction: Option<bool>,
        case_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.incremental.input.read(cx).text();
        self.incremental.status = None;
        let bytes = self.editor().document().byte_snapshot();
        let options = crate::search::LiteralOptions {
            match_case: self.incremental.match_case,
            ..Default::default()
        };
        if query.is_empty() {
            self.incremental.message.clear();
            self.incremental.input.update(cx, |field, cx| field.set_search_failure(false, cx));
            if case_changed {
                let caret = self.editor().document().selection().range().start;
                let result = self.editor_mut().select(Selection { anchor: caret, head: caret });
                self.editor_changed(result, window, cx);
            }
            if direction.is_none() { self.update_incremental_highlight(window, cx); }
            cx.notify();
            return;
        }
        if self.incremental.count && self.incremental.matches.is_empty() {
            match crate::search::literal_ranges_bytes(&bytes, &query, 0, options) {
                Ok(ranges) => {
                    self.incremental.matches = ranges;
                    self.incremental.counted_revision = Some(self.editor().document().text_revision());
                }
                Err(error) => {
                    self.incremental.message = error.to_string();
                    cx.notify();
                    return;
                }
            }
        }
        let selection = self.editor().document().selection();
        let found = crate::search::find_incremental_bytes(&bytes, &query, selection,
            self.incremental.match_case, direction.is_some(), direction.unwrap_or(false));
        let failed = matches!(&found, Ok(None) | Err(_));
        match found {
            Ok(Some(found)) => {
                let result = self.editor_mut().select(Selection {
                    anchor: found.range.start,
                    head: found.range.end,
                });
                self.editor_changed(result, window, cx);
                let ranges = &self.incremental.matches;
                let nth = ranges.iter().position(|range| *range == found.range).map_or(0, |index| index + 1);
                let message = if ranges.is_empty() { String::new() }
                    else { format!("{}/{}", crate::finder::commafy(nth), crate::finder::commafy(ranges.len())) };
                if found.wrapped {
                    self.incremental.status = Some(if direction == Some(true) {
                        "IncrementalFind-FSTopReached"
                    } else { "IncrementalFind-FSEndReached" });
                }
                self.incremental.message = message;
            }
            Ok(None) => {
                self.incremental.message.clear();
                self.incremental.status = Some("IncrementalFind-FSNotFound");
            }
            Err(error) => self.incremental.message = error.to_string(),
        }
        if failed && case_changed {
            let caret = self.editor().document().selection().range().start;
            let result = self.editor_mut().select(Selection { anchor: caret, head: caret });
            self.editor_changed(result, window, cx);
        }
        self.incremental.input.update(cx, |field, cx| field.set_search_failure(failed, cx));
        if direction.is_none() { self.update_incremental_highlight(window, cx); }
        cx.notify();
    }

    fn update_incremental_highlight(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.editor().document().selection().range();
        let bytes = self.editor().document().byte_snapshot();
        let query = self.incremental.input.read(cx).text();
        let ranges = if self.incremental.highlight && !query.is_empty() && !selection.is_empty() {
            let selected = match bytes.slice(selection.start.0..selection.end.0) {
                Ok(selected) => selected.to_vec(),
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            let selected = String::from_utf8_lossy(&selected);
            match crate::search::literal_ranges_bytes(&bytes, &selected, 0, crate::search::LiteralOptions {
                match_case: self.incremental.match_case, ..Default::default()
            }) {
                Ok(ranges) => ranges,
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            }
        } else { Vec::new() };
        let result = self.editor_mut().document_mut().set_indicators(6, ranges, true);
        self.editor_changed(result, window, cx);
    }

    fn toggle_incremental_option(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (key, value) = match index {
            0 => ("incrementalMatchCase", !self.incremental.match_case),
            1 => ("incrementalHighlightAll", !self.incremental.highlight),
            2 => ("incrementalCount", !self.incremental.count),
            _ => return,
        };
        if !self.persist_find_option(key, value.into(), cx) { return; }
        match index {
            0 => {
                self.incremental.match_case = value;
                self.incremental.matches.clear();
                self.run_incremental_search(None, true, window, cx);
            }
            1 => {
                self.incremental.highlight = value;
                self.update_incremental_highlight(window, cx);
            }
            _ => {
                self.incremental.count = value;
                self.incremental.matches.clear();
            }
        }
        cx.notify();
    }

    fn incremental_button_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "incremental-close" => self.close_incremental(window, cx),
            "incremental-position" => {
                let top = !self.incremental.top;
                if self.persist_find_option("incrementalSearchBarAtTop", top.into(), cx) { self.incremental.top = top; }
                cx.notify();
            }
            "incremental-previous" => self.run_incremental(Some(true), window, cx),
            "incremental-next" => self.run_incremental(Some(false), window, cx),
            _ => {},
        }
    }

    pub(super) fn render_incremental(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use super::dialog_font::px;
        if !self.incremental.visible {
            return None;
        }
        let locale = self.native_locale();
        let label = |id, fallback| crate::localization::dialog_control(&locale, "IncrementalFind", id).unwrap_or(fallback);
        let close = crate::localization::localized_string(&locale, "close-panel-tip").unwrap_or("Close");
        let position = crate::localization::localized_string(&locale, "matrix-incremental-position").unwrap_or("Move incremental search bar");
        let previous = crate::localization::menu_command(&locale, 43010).unwrap_or("Find Previous");
        let next = crate::localization::dialog_control(&locale, "Find", 1).unwrap_or("Find Next");
        self.incremental.input.update(cx, |field, cx| field.set_label(label(1681, "Find:"), cx));
        let mut message = self.incremental.message.clone();
        if let Some(status) = self.incremental.status {
            if !message.is_empty() { message.push_str(" - "); }
            message.push_str(crate::localization::localized_string(&locale, status).unwrap_or(""));
        }
        Some(crate::localization::with_dialog(&locale, "IncrementalFind", || {
            div()
                .id("incremental-search")
                .key_context("MatrixIncremental")
                .tab_group()
                .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| {
                    window.focus_next(cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                    window.focus_prev(cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &IncrementalRepeat, window, cx| {
                    if view.incremental.input.read(cx).is_focused(window) {
                        view.run_incremental(Some(false), window, cx);
                    } else {
                        view.incremental.input.update(cx, |field, cx| field.focus(window, cx));
                    }
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, action: &IncrementalOption, window, cx| {
                    view.toggle_incremental_option(action.0, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &CloseFind, window, cx| {
                    view.close_incremental(window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &FindDialogNext, window, cx| {
                    view.run_incremental(Some(false), window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &FindPrevious, window, cx| {
                    view.run_incremental(Some(true), window, cx);
                    cx.stop_propagation();
                }))
                .flex()
                .items_center()
                .h(px(32.))
                .gap(px(8.))
                .px(px(6.))
                .bg(rgb(0xf0f0f0))
                .border_t_1()
                .border_color(rgb(0xcccccc))
                .font_family(dialog_font::font_family())
                .text_size(px(13.))
                .child(
                    incremental_button("incremental-close", close, 0, cx).child("✕"),
                )
                .child(
                    incremental_button("incremental-position", position, 1, cx)
                        .child(if self.incremental.top { "˅" } else { "˄" }),
                )
                .child(label(1681, "Find:"))
                .child(
                    div()
                        .w(px(230.))
                        .child(self.incremental.input.clone())
                        .on_action(cx.listener(|view, _: &FindDialogNext, window, cx| {
                            view.run_incremental(Some(false), window, cx);
                            cx.stop_propagation();
                        })),
                )
                .children(
                    [
                        (true, "incremental-previous", "<"),
                        (false, "incremental-next", ">"),
                    ]
                    .into_iter()
                    .map(|(backwards, id, label)| {
                        incremental_button(id, if backwards { previous } else { next }, if backwards { 3 } else { 4 }, cx)
                            .w(px(24.))
                            .text_center()
                            .child(label)
                    }),
                )
                .children(
                    [
                        (
                            0,
                            "incremental-case",
                            label(1685, "Match case"),
                            self.incremental.match_case,
                        ),
                        (
                            1,
                            "incremental-highlight",
                            label(1690, "Highlight all"),
                            self.incremental.highlight,
                        ),
                        (2, "incremental-count", label(1691, "Count"), self.incremental.count),
                    ]
                    .into_iter()
                    .map(|(index, id, label, checked)| {
                        find_surface::checkbox(id, label, checked).focusable().tab_index(5 + index as isize)
                            .on_click(cx.listener(move |view, _, window, cx| view.toggle_incremental_option(index, window, cx)))
                            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                                if event.keystroke.key == "space" {
                                    view.toggle_incremental_option(index, window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                    }),
                )
                .child(message)
                .into_any_element()
        }))
    }
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", FindDialogNext, Some("MatrixIncremental")),
        KeyBinding::new("shift-enter", FindPrevious, Some("MatrixIncremental")),
        KeyBinding::new("escape", CloseFind, Some("MatrixIncremental")),
        KeyBinding::new("f3", FindDialogNext, Some("MatrixIncremental")),
        KeyBinding::new("shift-f3", FindPrevious, Some("MatrixIncremental")),
        KeyBinding::new("ctrl-alt-i", IncrementalRepeat, Some("MatrixIncremental")),
        KeyBinding::new("alt-c", IncrementalOption(0), Some("MatrixIncremental")),
        KeyBinding::new("alt-h", IncrementalOption(1), Some("MatrixIncremental")),
        KeyBinding::new("tab", DialogFocusNext, Some("MatrixIncremental")),
        KeyBinding::new("shift-tab", DialogFocusPrevious, Some("MatrixIncremental")),
    ]
}
