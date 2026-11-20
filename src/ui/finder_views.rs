use super::*;
use crate::localization::Text;

#[derive(Clone, Copy)]
enum ResultCheck {
    FoundLines,
    WholeWord,
    MatchCase,
}

pub(super) struct State {
    pub active: u64,
    next: u64,
    saved: Vec<(u64, ResultPane)>,
    visible: bool,
    source: u64,
    query: gpui::Entity<text_field::TextField>,
    options: crate::search::LiteralOptions,
    match_line_numbers: bool,
    accepted: (String, crate::search::LiteralOptions, bool),
    history_open: bool,
    history_original: String,
    progress: Option<ProgressView>,
    progress_focus: FocusHandle,
    mode_focus: [FocusHandle; 3],
}

impl State {
    pub fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            active: 0,
            next: 1,
            saved: Vec::new(),
            visible: false,
            source: 0,
            query: cx
                .new(|cx| text_field::TextField::compact("Find what", "find-in-results-query", cx)),
            options: Default::default(),
            match_line_numbers: true,
            accepted: (String::new(), Default::default(), true),
            history_open: false,
            history_original: String::new(),
            progress: None,
            progress_focus: cx.focus_handle(),
            mode_focus: [
                cx.focus_handle().tab_index(4).tab_stop(true),
                cx.focus_handle().tab_index(4).tab_stop(false),
                cx.focus_handle().tab_index(4).tab_stop(false),
            ],
        }
    }
}

struct ProgressView {
    paths: Vec<String>,
    state: Arc<crate::finder::ResultSearchProgress>,
}

struct ResultPane {
    results: crate::finder::Results,
    results_focus: FocusHandle,
    results_scroll: gpui::UniformListScrollHandle,
    wrapped_scroll: gpui::ListState,
    results_wrap: bool,
    wrapped_preferred_x: Option<gpui::Pixels>,
    results_selected: usize,
    results_width_item: Option<usize>,
    results_width: gpui::Pixels,
    style_revision: Option<(u64, bool)>,
    results_cursor_x: Option<gpui::Pixels>,
    caret_affinity: Option<(u64, crate::finder::TextPoint, bool)>,
    text_anchor: Option<crate::finder::TextPoint>,
    text_head: Option<crate::finder::TextPoint>,
    dragging_text: bool,
    context: Option<gpui::Point<gpui::Pixels>>,
    context_focus: FocusHandle,
    context_selected: usize,
    results_purge: bool,
    message: Text,
}

impl ResultPane {
    fn new(wrap: bool, cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            results: Default::default(),
            results_focus: cx.focus_handle(),
            results_scroll: gpui::UniformListScrollHandle::new(),
            wrapped_scroll: gpui::ListState::new(0, gpui::ListAlignment::Top, px(100.)),
            results_wrap: wrap,
            wrapped_preferred_x: None,
            results_selected: 0,
            results_width_item: None,
            results_width: px(0.),
            style_revision: None,
            results_cursor_x: None,
            caret_affinity: None,
            text_anchor: None,
            text_head: None,
            dragging_text: false,
            context: None,
            context_focus: cx.focus_handle(),
            context_selected: 0,
            results_purge: false,
            message: String::new().into(),
        }
    }

    fn swap(&mut self, current: &mut FileSearchState) {
        current.context = None;
        current.dragging_text = false;
        self.context = None;
        self.dragging_text = false;
        macro_rules! swap { ($($field:ident),* $(,)?) => { $(std::mem::swap(&mut self.$field, &mut current.$field);)* }; }
        swap!(
            results,
            results_focus,
            results_scroll,
            wrapped_scroll,
            results_wrap,
            wrapped_preferred_x,
            results_selected,
            results_width_item,
            results_width,
            style_revision,
            results_cursor_x,
            caret_affinity,
            text_anchor,
            text_head,
            dragging_text,
            context,
            context_focus,
            context_selected,
            results_purge,
            message
        );
    }
}

impl MatrixWindow {
    fn toggle_result_check(&mut self, check: ResultCheck, cx: &mut Context<Self>) {
        if matches!(check, ResultCheck::WholeWord)
            && matches!(
                self.files_search.views.options.mode,
                crate::search::SearchMode::Regex { .. }
            )
        {
            return;
        }
        let value = match check {
            ResultCheck::FoundLines => &mut self.files_search.views.match_line_numbers,
            ResultCheck::WholeWord => &mut self.files_search.views.options.whole_word,
            ResultCheck::MatchCase => &mut self.files_search.views.options.match_case,
        };
        *value = !*value;
        cx.notify();
    }

    fn set_result_search_mode(
        &mut self,
        mode: crate::search::SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_search.views.options.mode = mode;
        if matches!(mode, crate::search::SearchMode::Regex { .. }) {
            self.files_search.views.options.whole_word = false;
        }
        let index = match mode {
            crate::search::SearchMode::Normal => 0,
            crate::search::SearchMode::Extended => 1,
            crate::search::SearchMode::Regex { .. } => 2,
        };
        self.files_search.views.mode_focus[index].focus(window, cx);
        cx.notify();
    }
    fn activate_finder(&mut self, id: u64) -> bool {
        if self.files_search.views.active == id {
            return true;
        }
        let Some(index) = self
            .files_search
            .views
            .saved
            .iter()
            .position(|(saved, _)| *saved == id)
        else {
            return false;
        };
        self.cancel_finder_drag();
        self.files_search.context = None;
        let (target, mut pane) = self.files_search.views.saved.remove(index);
        pane.swap(&mut self.files_search);
        let previous = std::mem::replace(&mut self.files_search.views.active, target);
        self.files_search.views.saved.push((previous, pane));
        true
    }

    pub(super) fn activate_main_finder(&mut self, cx: &mut Context<Self>) {
        if self.activate_finder(0) {
            cx.notify();
        }
    }

    pub(super) fn close_active_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files_search.views.active == 0 {
            self.files_search.results_visible = false;
            self.editor_focus.focus(window, cx);
        } else {
            let closed = self.files_search.views.active;
            self.activate_finder(0);
            self.files_search
                .views
                .saved
                .retain(|(id, _)| *id != closed);
            self.files_search.results_focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn render_finder_tabs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.files_search.views.saved.is_empty() {
            return None;
        }
        let mut ids: Vec<_> = self
            .files_search
            .views
            .saved
            .iter()
            .map(|(id, _)| *id)
            .collect();
        ids.push(self.files_search.views.active);
        ids.sort_unstable();
        let caption = crate::localization::localized_string(&self.native_locale(), "find-result-caption").unwrap_or("Search results");
        Some(
            div()
                .flex()
                .h(px(22.))
                .children(ids.into_iter().map(|id| {
                    let selector = format!("finder-pane-{id}");
                    div()
                        .id(("finder-pane", id))
                        .debug_selector(move || selector.clone())
                        .role(Role::Tab)
                        .aria_selected(id == self.files_search.views.active)
                        .px(px(8.))
                        .child(if id == 0 {
                            caption.to_owned()
                        } else {
                            format!("{caption} {id}")
                        })
                        .when(id == self.files_search.views.active, |tab| {
                            tab.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                        })
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.files_search.result_batches.is_some() { cx.stop_propagation(); return; }
                            if view.activate_finder(id) {
                                view.files_search.results_focus.focus(window, cx);
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }))
                }))
                .into_any_element(),
        )
    }

    pub(super) fn show_find_in_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.context = None;
        self.files_search.views.source = self.files_search.views.active;
        self.files_search.views.history_open = false;
        let (query, options, match_line_numbers) = self.files_search.views.accepted.clone();
        self.files_search
            .views
            .query
            .update(cx, |input, cx| input.set_text(&query, window, cx));
        self.files_search.views.options = options;
        self.files_search.views.match_line_numbers = match_line_numbers;
        self.files_search.views.visible = true;
        self.sync_floating_windows(window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn find_in_results_visible(&self) -> bool {
        self.files_search.views.visible
    }

    pub(in crate::ui) fn find_in_results_progress_visible(&self) -> bool {
        self.files_search.progress.as_ref().is_some_and(|progress| {
            let snapshot = progress.snapshot();
            snapshot.discovering || snapshot.total > 1
        }) || self.files_search
            .views
            .progress
            .as_ref()
            .is_some_and(|progress| progress.paths.len() > 1)
    }

    pub(super) fn find_in_results_search_active(&self) -> bool {
        self.files_search.views.progress.is_some()
    }

    pub(in crate::ui) fn cancel_find_in_results_progress(&mut self, cx: &mut Context<Self>) {
        if self.files_search.progress.is_some() { self.cancel_active_file_search(cx); return; }
        if let Some(progress) = &self.files_search.views.progress {
            progress.state.cancel();
            cx.notify();
        }
    }

    pub(in crate::ui) fn render_find_in_results_progress(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use super::super::dialog_font::px;
        let (has_info, hits, cancelled, path, percent) = if let Some(progress) = &self.files_search.progress {
            let snapshot = progress.snapshot();
            let cancelled = self.files_search.cancel.as_ref().is_some_and(|cancel| cancel.load(Ordering::Relaxed));
            let step = if snapshot.replacing && snapshot.total >= 200 { snapshot.total / 100 } else { 1 };
            let percent = if snapshot.completed == 0 || snapshot.total == 0 { 0 } else { ((snapshot.completed - 1) / step * step) * 100 / snapshot.total };
            (snapshot.discovering || snapshot.completed > 0, snapshot.hits, cancelled, snapshot.path, percent as f32)
        } else {
            let progress = self.files_search.views.progress.as_ref()?;
            let (completed, hits) = progress.state.snapshot();
            (completed > 0, hits, progress.state.is_cancelled(), completed.checked_sub(1).and_then(|index| progress.paths.get(index)).cloned().unwrap_or_default(), progress.state.percent(progress.paths.len()) as f32)
        };
        let locale = self.native_locale();
        let cancel = crate::localization::localized_string(&locale, "common-cancel").unwrap_or("Cancel");
        let path = if cancelled { crate::localization::localized_string(&locale, "progress-cancel-info").unwrap_or("Cancelling operation, please wait...").to_owned() } else { path };
        Some(
            div()
                .id("find-results-progress")
                .debug_selector(|| "find-results-progress".into())
                .role(Role::Dialog)
                .aria_label(self.find_in_results_progress_title())
                .relative()
                .size_full()
                .bg(rgb(0xf0f0f0))
                .text_color(rgb(0x202020))
                .text_size(px(13.))
                .line_height(px(18.))
                .child(
                    div()
                        .id("find-results-progress-path")
                        .debug_selector(|| "find-results-progress-path".into())
                        .role(Role::Status)
                        .aria_label(path.to_owned())
                        .absolute()
                        .left(px(5.))
                        .top(px(5.))
                        .w(px(555.))
                        .h(px(18.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(path.to_owned()),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(125.))
                        .top(px(23.))
                        .w(px(150.))
                        .h(px(18.))
                        .text_right()
                        .child(crate::localization::localized_string(&locale, "progress-hits-title").unwrap_or("Hits:")),
                )
                .child(
                    div()
                        .id("find-results-progress-hits")
                        .debug_selector(|| "find-results-progress-hits".into())
                        .role(Role::Status)
                        .absolute()
                        .left(px(280.))
                        .top(px(23.))
                        .w(px(150.))
                        .h(px(18.))
                        .child(if !has_info {
                            String::new()
                        } else {
                            hits.to_string()
                        }),
                )
                .child(
                    div()
                        .id("find-results-progress-bar")
                        .role(Role::ProgressIndicator)
                        .aria_label(crate::localization::localized_string(&locale, "matrix-find-progress-bar").unwrap_or("Progress Bar"))
                        .absolute()
                        .left(px(5.))
                        .top(px(41.))
                        .w(px(555.))
                        .h(px(10.))
                        .border_1()
                        .border_color(rgb(0xa0a0a0))
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(percent / 100.))
                                .bg(rgb(0x06b025)),
                        ),
                )
                .child(
                    div()
                        .id("find-results-progress-cancel")
                        .debug_selector(|| "find-results-progress-cancel".into())
                        .role(Role::Button)
                        .aria_label(cancel)
                        .track_focus(&self.files_search.views.progress_focus)
                        .absolute()
                        .left(px(237.))
                        .top(px(59.))
                        .w(px(80.))
                        .h(px(25.))
                        .border_1()
                        .border_color(rgb(0xadadad))
                        .bg(rgb(0xfafafa))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(cancel)
                        .a11y_synthetic_children(move |builder| {
                            if cancelled {
                                builder.parent_node().set_disabled();
                            }
                        })
                        .when(cancelled, |button| button.text_color(rgb(0x808080)))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if !cancelled {
                                view.cancel_find_in_results_progress(cx);
                            }
                        }))
                        .on_key_down(cx.listener(
                            move |view, event: &gpui::KeyDownEvent, _, cx| {
                                if !cancelled && event.keystroke.key == "space" {
                                    view.cancel_find_in_results_progress(cx);
                                    cx.stop_propagation();
                                }
                            },
                        )),
                )
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn find_in_results_progress_title(&self) -> &'static str {
        if let Some(progress) = &self.files_search.progress {
            let snapshot = progress.snapshot();
            if snapshot.discovering { return crate::localization::localized_string(&self.native_locale(), "discover-file-candidates-title").unwrap_or("Discovering file candidates..."); }
            if snapshot.replacing {
                return crate::localization::localized_string(&self.native_locale(), "replace-in-files-progress-title").unwrap_or("Replace In Files progress...");
            }
        }
        crate::localization::localized_string(&self.native_locale(), "find-in-files-progress-title").unwrap_or("Find In Files progress...")
    }

    pub(in crate::ui) fn focus_find_in_results_progress(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_search.views.progress_focus.focus(window, cx);
    }

    fn refresh_find_in_results_progress(
        &self,
        state: Arc<crate::finder::ResultSearchProgress>,
        cx: &mut Context<Self>,
    ) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |view, cx| {
            let mut last = state.snapshot();
            loop {
                executor.timer(std::time::Duration::from_millis(50)).await;
                match view.update(cx, |view, cx| {
                    if !cx.windows().contains(&view.document_window)
                        || !view
                            .files_search
                            .views
                            .progress
                            .as_ref()
                            .is_some_and(|progress| Arc::ptr_eq(&progress.state, &state))
                    {
                        return false;
                    }
                    let current = state.snapshot();
                    if current != last {
                        last = current;
                        cx.notify();
                    }
                    true
                }) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        eprintln!("Find in results progress lost its window: {error}");
                        break;
                    }
                }
            }
        })
        .detach();
    }
    pub(in crate::ui) fn dismiss_find_in_results(&mut self, cx: &mut Context<Self>) {
        self.files_search.views.visible = false;
        self.files_search.views.history_open = false;
        cx.notify();
    }
    pub(in crate::ui) fn find_in_results_input(&self) -> gpui::Entity<text_field::TextField> {
        self.files_search.views.query.clone()
    }

    pub(in crate::ui) fn retire_find_in_results_popup(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.files_search.views.history_open) {
            cx.notify();
        }
    }

    fn toggle_result_query_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.views.history_original = self.files_search.views.query.read(cx).text();
        self.files_search.views.history_open =
            !self.files_search.views.history_open && !self.files_search.views.accepted.0.is_empty();
        self.files_search
            .views
            .query
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn preview_result_query_history(
        &mut self,
        close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let accepted = self.files_search.views.accepted.0.clone();
        self.files_search.views.query.update(cx, |input, cx| {
            input.set_text(&accepted, window, cx);
            input.focus(window, cx);
        });
        if close {
            self.files_search.views.history_open = false;
        }
        cx.notify();
    }

    fn render_result_query_combo(&self, cx: &mut Context<Self>) -> AnyElement {
        use super::super::dialog_font::px;
        let open = self.files_search.views.history_open;
        div()
            .id("find-in-results-query-combo")
            .debug_selector(|| "find-in-results-query-combo".into())
            .relative()
            .flex()
            .w_full()
            .h(px(21.))
            .role(Role::ComboBox)
            .aria_expanded(open)
            .on_action(cx.listener(|view, _: &SearchHistoryOpen, window, cx| {
                view.toggle_result_query_history(window, cx);
                cx.stop_propagation();
            }))
            .when(open, |combo| {
                combo
                    .key_context("MatrixSearchHistory")
                    .on_action(cx.listener(|view, _: &SearchHistoryAccept, window, cx| {
                        view.files_search.views.history_open = false;
                        view.files_search
                            .views
                            .query
                            .update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(|view, _: &SearchHistoryCancel, window, cx| {
                        let original = view.files_search.views.history_original.clone();
                        view.files_search.views.history_open = false;
                        view.files_search.views.query.update(cx, |input, cx| {
                            input.set_text(&original, window, cx);
                            input.focus(window, cx);
                        });
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(|view, _: &SearchHistoryUp, window, cx| {
                        view.preview_result_query_history(false, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(|view, _: &SearchHistoryDown, window, cx| {
                        view.preview_result_query_history(false, window, cx);
                        cx.stop_propagation();
                    }))
            })
            .when(open, |combo| {
                combo
                    .on_action(cx.listener(|view, _: &SearchHistoryNextField, window, cx| {
                        view.files_search.views.history_open = false;
                        window.focus_next(cx);
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_action(
                        cx.listener(|view, _: &SearchHistoryPreviousField, window, cx| {
                            view.files_search.views.history_open = false;
                            window.focus_prev(cx);
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(self.files_search.views.query.clone()),
            )
            .child(
                div()
                    .id("find-in-results-history-toggle")
                    .debug_selector(|| "find-in-results-history-toggle".into())
                    .role(Role::Button)
                    .aria_label(crate::localization::localized_string(&self.native_locale(), "matrix-find-previous-search").unwrap_or("Show previous search"))
                    .w(px(20.))
                    .child("⌄")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_result_query_history(window, cx);
                        cx.stop_propagation();
                    })),
            )
            .when(open, |combo| {
                combo.child(
                    gpui::deferred(
                        gpui::anchored()
                            .offset(gpui::point(px(0.), px(21.)))
                            .snap_to_window()
                            .child(
                                div()
                                    .id("find-in-results-history-popup")
                                    .w(px(340.))
                                    .bg(rgb(0xffffff))
                                    .text_color(rgb(0))
                                    .border_1()
                                    .border_color(rgb(0x808080))
                                    .occlude()
                                    .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                                        view.files_search.views.history_open = false;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .id("find-in-results-history-item")
                                            .debug_selector(|| {
                                                "find-in-results-history-item".into()
                                            })
                                            .role(Role::ListItem)
                                            .child(self.files_search.views.accepted.0.clone())
                                            .on_click(cx.listener(|view, _, window, cx| {
                                                view.preview_result_query_history(true, window, cx);
                                                cx.stop_propagation();
                                            })),
                                    ),
                            ),
                    )
                    .priority(2),
                )
            })
            .into_any_element()
    }

    pub(in crate::ui) fn render_find_in_results(&self, cx: &mut Context<Self>) -> AnyElement {
        use super::super::dialog_font::px;
        let locale = self.native_locale();
        let label = |id, fallback| crate::localization::dialog_control(&locale, "FindInFinder", id).unwrap_or(fallback);
        self.files_search.views.query.update(cx, |field, cx| field.set_label(label(1711, "Find what:"), cx));
        div()
            .id("find-in-results-dialog")
            .debug_selector(|| "find-in-results-dialog".into())
            .role(Role::Dialog)
            .aria_label(crate::localization::dialog_title(&locale, "FindInFinder").unwrap_or("Find in search results"))
            .key_context("MatrixFind")
            .size_full()
            .relative()
            .font_family(dialog_font::font_family())
            .text_size(px(11.))
            .line_height(px(16.))
            .bg(rgb(0xf9f9f9))
            .text_color(rgb(0x202020))
            .child(
                div()
                    .absolute()
                    .left(px(9.))
                    .top(px(16.5))
                    .w(px(112.5))
                    .text_right()
                    .child(label(1711, "Find what:")),
            )
            .child(
                div()
                    .absolute()
                    .left(px(124.5))
                    .top(px(13.5))
                    .w(px(267.))
                    .h(px(21.))
                    .child(self.render_result_query_combo(cx)),
            )
            .child(
                find_surface::checkbox(
                    "find-in-results-line-filter",
                    label(1713, "Search only in found lines"),
                    self.files_search.views.match_line_numbers,
                )
                .absolute()
                .left(px(18.))
                .top(px(46.5))
                .w(px(375.))
                .h(px(15.))
                .focusable()
                .tab_index(1)
                .on_click(cx.listener(|view, _, _, cx| {
                    view.toggle_result_check(ResultCheck::FoundLines, cx);
                }))
                .on_key_down(cx.listener(
                    |view, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "space" {
                            view.toggle_result_check(ResultCheck::FoundLines, cx);
                            cx.stop_propagation();
                        }
                    },
                )),
            )
            .child(
                find_surface::checkbox(
                    "find-in-results-word",
                    label(1714, "Match whole word only"),
                    self.files_search.views.options.whole_word,
                )
                .absolute()
                .left(px(18.))
                .top(px(64.5))
                .w(px(210.))
                .h(px(15.))
                .focusable()
                .tab_index(2)
                .on_click(cx.listener(|view, _, _, cx| {
                    view.toggle_result_check(ResultCheck::WholeWord, cx);
                }))
                .on_key_down(cx.listener(
                    |view, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "space" {
                            view.toggle_result_check(ResultCheck::WholeWord, cx);
                            cx.stop_propagation();
                        }
                    },
                )),
            )
            .child(
                find_surface::checkbox(
                    "find-in-results-case",
                    label(1715, "Match case"),
                    self.files_search.views.options.match_case,
                )
                .absolute()
                .left(px(18.))
                .top(px(82.5))
                .w(px(210.))
                .h(px(15.))
                .focusable()
                .tab_index(3)
                .on_click(cx.listener(|view, _, _, cx| {
                    view.toggle_result_check(ResultCheck::MatchCase, cx);
                }))
                .on_key_down(cx.listener(
                    |view, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "space" {
                            view.toggle_result_check(ResultCheck::MatchCase, cx);
                            cx.stop_propagation();
                        }
                    },
                )),
            )
            .child(
                div()
                    .id("find-in-results-search-mode")
                    .role(Role::Group)
                    .aria_label(label(1716, "Search Mode"))
                    .absolute()
                    .left(px(9.))
                    .top(px(105.))
                    .w(px(300.))
                    .h(px(72.))
                    .border_1()
                    .border_color(rgb(0xd0d0d0))
                    .child(
                        div()
                            .absolute()
                            .left(px(8.))
                            .top(px(-8.))
                            .px(px(3.))
                            .bg(rgb(0xf9f9f9))
                            .child(label(1716, "Search Mode")),
                    )
                    .children(
                        [
                            (crate::search::SearchMode::Normal, label(1717, "Normal")),
                            (
                                crate::search::SearchMode::Extended,
                                label(1718, r"Extended (\n, \r, \t, \0, \x...)"),
                            ),
                            (
                                crate::search::SearchMode::Regex {
                                    dot_matches_newline: matches!(
                                        self.files_search.views.options.mode,
                                        crate::search::SearchMode::Regex {
                                            dot_matches_newline: true
                                        }
                                    ),
                                },
                                label(1719, "Regular expression"),
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (mode, label))| {
                            let selected = self.files_search.views.options.mode == mode;
                            div()
                                .id(("find-in-results-mode", index))
                                .role(Role::RadioButton)
                                .track_focus(
                                    &self.files_search.views.mode_focus[index]
                                        .clone()
                                        .tab_stop(selected),
                                )
                                .aria_label(label)
                                .aria_toggled(selected.into())
                                .absolute()
                                .left(px(9.))
                                .top(px(18. + index as f32 * 18.))
                                .h(px(15.))
                                .flex()
                                .items_center()
                                .gap(px(3.))
                                .child(
                                    div()
                                        .size(px(13.))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(rgb(0x767676))
                                        .bg(rgb(0xffffff))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(selected, |radio| {
                                            radio.child(
                                                div().size(px(7.)).rounded_full().bg(rgb(0x0078d4)),
                                            )
                                        }),
                                )
                                .child(label)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.set_result_search_mode(mode, window, cx)
                                }))
                                .on_key_down(cx.listener(
                                    move |view, event: &gpui::KeyDownEvent, window, cx| {
                                        let target_index = match event.keystroke.key.as_str() {
                                            "space" => index,
                                            "right" | "down" => (index + 1) % 3,
                                            "left" | "up" => (index + 2) % 3,
                                            _ => return,
                                        };
                                        let target = [
                                            crate::search::SearchMode::Normal,
                                            crate::search::SearchMode::Extended,
                                            crate::search::SearchMode::Regex {
                                                dot_matches_newline: false,
                                            },
                                        ][target_index];
                                        view.set_result_search_mode(target, window, cx);
                                        cx.stop_propagation();
                                    },
                                ))
                        }),
                    )
                    .child(
                        find_surface::checkbox(
                            "results-dot-newline",
                            label(1720, ". matches newline"),
                            matches!(
                                self.files_search.views.options.mode,
                                crate::search::SearchMode::Regex {
                                    dot_matches_newline: true
                                }
                            ),
                        )
                        .absolute()
                        .left(px(146.))
                        .top(px(54.))
                        .on_click(cx.listener(|view, _, _, cx| {
                            if let crate::search::SearchMode::Regex {
                                dot_matches_newline,
                            } = view.files_search.views.options.mode
                            {
                                view.files_search.views.options.mode =
                                    crate::search::SearchMode::Regex {
                                        dot_matches_newline: !dot_matches_newline,
                                    };
                                cx.notify();
                            }
                        })),
                    ),
            )
            .child(
                find_surface::button("find-in-results-all", label(1, "Find All"), 13.5)
                    .right(px(11.))
                    .w(px(135.))
                    .focusable()
                    .tab_index(5)
                    .on_click(
                        cx.listener(|view, _, window, cx| view.submit_find_in_results(window, cx)),
                    )
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "space" {
                            view.submit_find_in_results(window, cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(
                find_surface::button("find-in-results-cancel", label(2, "Close"), 40.5)
                    .right(px(11.))
                    .w(px(135.))
                    .focusable()
                    .tab_index(6)
                    .on_click(cx.listener(|view, _, _, cx| view.dismiss_find_in_results(cx)))
                    .on_action(cx.listener(|view, _: &FindDialogNext, _, cx| {
                        view.dismiss_find_in_results(cx);
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "space" {
                            view.dismiss_find_in_results(cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .into_any_element()
    }

    pub(in crate::ui) fn submit_find_in_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.submit_find_in_results(window, cx)
        }) {
            return;
        }
        if self.file_busy || self.files_search.cancel.is_some() || self.batch_editing_blocked() {
            return;
        }
        let source_id = self.files_search.views.source;
        let source = if source_id == self.files_search.views.active {
            Some(&self.files_search.results)
        } else {
            self.files_search
                .views
                .saved
                .iter()
                .find(|(id, _)| *id == source_id)
                .map(|(_, pane)| &pane.results)
        };
        let Some(source) = source else {
            self.dismiss_find_in_results(cx);
            return;
        };
        let filter = source.line_filter();
        let query = crate::search_history::Field::Find
            .capture(&self.files_search.views.query.read(cx).text());
        let options = self.files_search.views.options;
        let line_numbers = self.files_search.views.match_line_numbers;
        self.files_search.views.accepted = (query.clone(), options, line_numbers);
        let one_per_line = self.files_search.results_one_per_line;
        let (identities, opened) = self.capture_open_search_documents();
        self.files_search.views.visible = false;
        self.file_busy = true;
        let cancel = Arc::new(AtomicBool::new(false));
        self.files_search.cancel = Some(cancel.clone());
        let progress = Arc::new(crate::finder::ResultSearchProgress::new(cancel));
        let show_progress = filter.paths().len() > 1;
        self.files_search.views.progress = Some(ProgressView {
            paths: filter.paths().to_vec(),
            state: progress.clone(),
        });
        if show_progress {
            self.refresh_find_in_results_progress(progress.clone(), cx);
        }
        let task = cx.background_executor().spawn(async move {
            let mut updates = Vec::new();
            let result = filter.search_with_progress_and_state(
                &query,
                options,
                line_numbers,
                one_per_line,
                &progress,
                |path| {
                    let path_buf = std::path::Path::new(path);
                    let canonical = path_buf.canonicalize().ok();
                    if let Some(identity) = opened.iter().position(|doc| doc.paths.iter().any(|candidate| candidate == path_buf
                        || canonical.as_ref().is_some_and(|path| candidate == path))) {
                        let source = &opened[identity];
                        Some(crate::finder::SearchDocument {
                            document: crate::finder::Document { name: path.to_owned(), bytes: source.bytes.clone(), selection: None },
                            regex_state: source.regex_state.clone(), identity: Some(identity),
                        })
                    } else {
                        match crate::files::DiskFile::load_byte_buffer(std::path::Path::new(path)) {
                            Ok((_, bytes)) => Some(crate::finder::Document {
                                name: path.to_owned(), bytes, selection: None,
                            }.into()),
                            Err(error) => {
                                eprintln!("Find in results skipped {path}: {error}");
                                None
                            }
                        }
                    }
                },
                &mut updates,
            );
            (result, updates)
        });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let (result, updates) = task.await;
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.write_search_regex_states(&identities, updates);
                    view.finish_file_search_job();
                    view.files_search.views.progress = None;
                    match result {
                        Ok(search) => {
                            let wrap = if view.files_search.views.active == 0 {
                                view.files_search.results_wrap
                            } else {
                                view.files_search
                                    .views
                                    .saved
                                    .iter()
                                    .find(|(id, _)| *id == 0)
                                    .is_some_and(|(_, pane)| pane.results_wrap)
                            };
                            let mut pane = ResultPane::new(wrap, cx);
                            let hits = search.hits;
                            pane.results.push(search, false);
                            pane.message = Text::resource("matrix-find-results-summary", "Search results - ($INT_REPLACE$ hits)")
                                .replace("$INT_REPLACE$", crate::finder::commafy(hits));
                            pane.swap(&mut view.files_search);
                            let previous = view.files_search.views.active;
                            let id = view.files_search.views.next;
                            view.files_search.views.next += 1;
                            view.files_search.views.saved.push((previous, pane));
                            view.files_search.views.active = id;
                            view.files_search.results_visible = true;
                            view.files_search.results_focus.focus(window, cx);
                        }
                        Err(error) => {
                            let message = match error {
                                crate::search::SearchError::Regex(detail) => Text::resource("find-status-invalid-re", "Find: Invalid regular expression")
                                    .with_suffix(": ").with_suffix(detail),
                                crate::search::SearchError::RegexRuntime(detail) => Text::resource("find-status-search-failed", "Find: Search failed")
                                    .with_suffix(": ").with_suffix(detail),
                                crate::search::SearchError::InvalidSelection => Text::resource("matrix-search-invalid-selection", "Search selection is not a valid text boundary"),
                            };
                            view.notify_localized_editor_error(message, cx);
                        }
                    }
                    cx.notify();
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("Find in results completion lost its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }
}
