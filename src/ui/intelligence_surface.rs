use super::*;
use crate::intelligence::{
    CompletionCandidate, CompletionEdit, CompletionKind, CompletionMode, CompletionResult,
    IntelligenceError, SignatureHelp, SubmitKey,
};
use gpui::Pixels;

use crate::intelligence::events as model;
#[path = "intelligence_source.rs"]
pub(super) mod source;
pub(super) use model::Action as IntelligenceAction;
use model::{IntelligenceModel, Settings, Snapshot};

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
#[path = "intelligence_surface_tests.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestMode {
    Automatic,
    Words,
    Api,
    Path,
    Calltip,
}

pub(super) struct State {
    model: IntelligenceModel,
    settings: Settings,
    mode: RequestMode,
    task: Option<gpui::Task<()>>,
    scroll: gpui::UniformListScrollHandle,
    auto_insert: [crate::intelligence::auto_insert::Tracker; 2],
}

impl State {
    pub(super) fn new(_: &mut Context<MatrixWindow>) -> Self {
        Self {
            model: IntelligenceModel::default(),
            settings: Settings::default(),
            mode: RequestMode::Automatic,
            task: None,
            scroll: gpui::UniformListScrollHandle::new(),
            auto_insert: Default::default(),
        }
    }
}

struct Response {
    settings: Settings,
    completion: Option<CompletionResult>,
    selected: usize,
    calltip: Option<SignatureHelp>,
}

impl MatrixWindow {
    pub(super) fn configure_intelligence(&mut self, settings: &serde_json::Value) {
        self.intelligence.settings = Settings::from_json(settings);
    }

    pub(super) fn auto_insert_flag(&self, index: usize) -> bool {
        self.intelligence
            .settings
            .auto_insert
            .flags
            .get(index)
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn set_auto_insert_flag(&mut self, index: usize, enabled: bool) {
        if let Some(flag) = self.intelligence.settings.auto_insert.flags.get_mut(index) {
            *flag = enabled;
        }
        self.intelligence.model.dismiss();
        self.intelligence.task = None;
    }

    pub(super) fn auto_indent_mode(&self) -> crate::indentation::Mode {
        self.intelligence.settings.maintain_indent
    }

    pub(super) fn set_auto_indent_mode(&mut self, mode: crate::indentation::Mode) {
        self.intelligence.settings.maintain_indent = mode;
        self.intelligence.model.dismiss();
        self.intelligence.task = None;
    }

    pub(super) fn commit_editor_text(
        &mut self,
        text: &str,
        typing: bool,
    ) -> Result<(), crate::editor::EditError> {
        let settings = self.intelligence.settings.auto_insert.clone();
        let mode = self.auto_indent_mode();
        let indentation = mode != crate::indentation::Mode::None
            && (text == self.editor().line_ending().as_str()
                || (text.chars().count() == 1 && matches!(text, "{" | "}" | "\n" | "\r")));
        let pairs = settings.enabled()
            && self.workspace.auto_completion_allowed()
            && self.editor().selections().ranges().len() == 1;
        if (!pairs && !indentation)
            || self.macro_recording
            || self.macro_playing
            || self.editor().has_composition()
        {
            return if typing {
                self.editor_mut().type_text(text)
            } else {
                self.editor_mut().insert_text(text)
            };
        }
        let pane = self.pane as usize;
        let language = self.workspace.active().language();
        let mut tracker = std::mem::take(&mut self.intelligence.auto_insert[pane]);
        let grouped = indentation || text.chars().any(|character| settings.may_edit(character));
        if grouped {
            self.editor_mut().begin_undo_action();
        }
        let result = (|| {
            for character in text.chars() {
                let mut bytes = [0; 4];
                let text = character.encode_utf8(&mut bytes);
                if typing {
                    self.editor_mut().type_text(text)?;
                } else {
                    self.editor_mut().insert_text(text)?;
                }
                if indentation {
                    crate::indentation::after_character(
                        self.editor_mut(),
                        character,
                        mode,
                        language,
                    )?;
                }
                if pairs {
                    tracker.after_character(self.editor_mut(), character, &settings, language)?;
                }
            }
            Ok(())
        })();
        if grouped {
            self.editor_mut().end_undo_action();
        }
        self.intelligence.auto_insert[pane] = tracker;
        result
    }

    fn intelligence_snapshot(&self) -> Snapshot {
        Snapshot {
            buffer: self.workspace.active().id(),
            view: self.workspace.active_view(),
            revision: self.editor().display_revision(),
            selection: self.editor().selection(),
            language: self.workspace.active().language(),
        }
    }

    fn intelligence_blocked(&self) -> bool {
        self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || self.find_visible
            || self.files_search.visible
            || self.goto_visible
            || self.preferences_visible
            || self.about_visible
            || self.summary_visible
            || self.macro_dialogs.save_visible
            || self.macro_dialogs.repeat_visible
            || self.reload_dialog
            || self.document_switcher_visible()
            || self.udl.visible
            || self.udl.styler_visible
            || self.column_editor.visible
            || self.run_visible
            || self.editor().marked_range().is_some()
    }

    pub(super) fn completion_visible(&self) -> bool {
        !self.intelligence_blocked()
            && self
                .intelligence
                .model
                .completion()
                .is_some_and(|popup| popup.owner == self.intelligence_snapshot())
    }

    pub(super) fn calltip_visible(&self) -> bool {
        !self.intelligence_blocked()
            && self
                .intelligence
                .model
                .calltip()
                .is_some_and(|popup| popup.owner == self.intelligence_snapshot())
    }

    pub(super) fn dismiss_completion(&mut self, cx: &mut Context<Self>) {
        self.intelligence.model.dismiss_completion();
        self.intelligence.mode = RequestMode::Automatic;
        cx.notify();
    }

    pub(super) fn dismiss_calltip(&mut self, cx: &mut Context<Self>) {
        self.intelligence.model.dismiss_calltip();
        cx.notify();
    }

    pub(super) fn on_editor_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_recording
            || self.macro_playing
            || !self.workspace.auto_completion_allowed()
            || self.intelligence_blocked()
        {
            return;
        }
        let current = self.intelligence_snapshot();
        let mode = if self.intelligence.model.completion().is_some_and(|popup| {
            popup.owner.buffer == current.buffer
                && popup.owner.view == current.view
                && popup.owner.language == current.language
        }) {
            self.intelligence.mode
        } else {
            RequestMode::Automatic
        };
        self.request_intelligence(mode, false, window, cx);
    }

    pub(super) fn complete_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_intelligence(RequestMode::Words, true, window, cx);
    }

    pub(super) fn complete_api(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_intelligence(RequestMode::Api, true, window, cx);
    }

    pub(super) fn complete_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_intelligence(RequestMode::Path, true, window, cx);
    }

    pub(super) fn show_calltip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_intelligence(RequestMode::Calltip, true, window, cx);
    }

    fn request_intelligence(
        &mut self,
        mode: RequestMode,
        explicit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.request_intelligence(mode, explicit, window, cx)
        }) {
            return;
        }
        if explicit {
            self.menu_input(MenuEvent::Dismiss, window, cx);
        }
        if self.intelligence_blocked() {
            return;
        }
        let owner = self.intelligence_snapshot();
        let language = owner.language;
        let udl_name = match language {
            crate::syntax::Language::UserDefined(id) => self
                .udl
                .registry
                .definition(id)
                .map(|definition| definition.name.as_str()),
            _ => None,
        };
        let key = match source::source_key(language, udl_name) {
            Ok(key) => key,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        };
        let text = self.editor().committed_text();
        let caret = owner.selection.head.0;
        let base = self
            .workspace
            .active()
            .disk_file
            .as_ref()
            .and_then(|file| file.path().parent())
            .map(PathBuf::from);
        let settings_home = self
            .settings_store
            .as_ref()
            .and_then(|store| store.root().parent())
            .map(PathBuf::from);
        let previous_call = self
            .intelligence
            .model
            .calltip()
            .filter(|popup| {
                popup.owner.buffer == owner.buffer
                    && popup.owner.view == owner.view
                    && popup.owner.language == owner.language
            })
            .map(|popup| {
                (
                    popup.help.call_open_byte,
                    popup.help.function_name.clone(),
                    popup.help.active_overload,
                )
            });
        let previous_completion = self
            .intelligence
            .model
            .completion()
            .filter(|popup| {
                popup.owner.buffer == owner.buffer
                    && popup.owner.view == owner.view
                    && popup.owner.language == owner.language
            })
            .map(|popup| popup.result.clone());
        let request = self.intelligence.model.begin_request(owner.clone());
        self.intelligence.mode = mode;
        let target = owner.clone();
        let worker = cx.background_executor().spawn(async move {
            let catalog = source::catalog()?;
            let settings = match settings_home {
                Some(home) => {
                    Settings::from_json(&ConfigStore::under_home(&home).load_settings().map_err(
                        |error| IntelligenceError(format!("Auto-completion settings: {error}")),
                    )?)
                }
                None => Settings::default(),
            };
            let completion_command = explicit
                && matches!(
                    mode,
                    RequestMode::Words | RequestMode::Api | RequestMode::Path
                );
            let mut calltip = if !completion_command
                && (settings.function_parameters
                    || mode == RequestMode::Calltip
                    || previous_call.is_some())
            {
                catalog.signature_help(&key, &text, caret, 0)?
            } else {
                None
            };
            if let (Some(help), Some((open, name, selected))) = (&mut calltip, previous_call)
                && help.call_open_byte == open
                && help.function_name == name
            {
                for _ in 0..selected.min(help.signatures.len()) {
                    help.cycle_overload(true);
                }
            }
            let mut selected = 0;
            let completion = if mode == RequestMode::Calltip || (!explicit && calltip.is_some()) {
                None
            } else if mode == RequestMode::Path {
                let base = match base {
                    Some(base) => base,
                    None => std::env::current_dir().map_err(|error| {
                        IntelligenceError(format!("Path completion directory: {error}"))
                    })?,
                };
                Some(catalog.complete_path(&text, caret, &base)?)
            } else {
                let requested = match mode {
                    RequestMode::Words => Some(CompletionMode::Words),
                    RequestMode::Api => Some(CompletionMode::Api),
                    _ => settings.automatic,
                };
                if let Some(requested) = requested {
                    let mut options = settings.completion.clone();
                    options.mode = requested;
                    options.auto_insert_single = explicit && mode == RequestMode::Words;
                    if explicit {
                        options.minimum_prefix = 1;
                    }
                    let mut result = catalog.complete(&key, &text, caret, &options)?;
                    if !result.candidates.is_empty()
                        && requested == CompletionMode::Api
                        && !settings.brief
                        && let Some(api) = catalog.api(&key)
                    {
                        result.candidates = api
                            .keywords
                            .iter()
                            .map(|keyword| CompletionCandidate {
                                text: keyword.name.clone(),
                                kind: if keyword.is_function {
                                    CompletionKind::Function
                                } else {
                                    CompletionKind::Keyword
                                },
                            })
                            .collect();
                    }
                    let prefix = &text[result.replace_range.clone()];
                    let ignore_case = options.ignore_case.unwrap_or_else(|| {
                        catalog
                            .api(&key)
                            .is_some_and(|api| api.environment.ignore_case)
                    });
                    let matches_prefix = |candidate: &CompletionCandidate| {
                        if ignore_case {
                            candidate
                                .text
                                .to_lowercase()
                                .starts_with(&prefix.to_lowercase())
                        } else {
                            candidate.text.starts_with(prefix)
                        }
                    };
                    if !explicit
                        && !settings.brief
                        && let Some(previous) = previous_completion.filter(|previous| {
                            previous.replace_range.start == result.replace_range.start
                        })
                    {
                        result.candidates = previous.candidates;
                    }
                    if let Some(index) = result.candidates.iter().position(matches_prefix) {
                        selected = index;
                    } else {
                        result.candidates.clear();
                    }
                    Some(result)
                } else {
                    None
                }
            };
            Ok::<_, IntelligenceError>(Response {
                settings,
                completion,
                selected,
                calltip,
            })
        });
        let document_window = self.document_window;
        self.intelligence.task = Some(cx.spawn(async move |entity, cx| {
            let response = worker.await;
            match document_window.update(cx, |_, window, cx| {
                entity.update(cx, |view, cx| {
                    let current = view.intelligence_snapshot();
                    if !view.intelligence.model.finish_request(&request, &current)
                        || view.intelligence_blocked()
                        || !view.editor_focus.is_focused(window)
                    {
                        return;
                    }
                    let response = match response {
                        Ok(response) => response,
                        Err(error) => {
                            view.notify_editor_error(error.to_string(), cx);
                            return;
                        }
                    };
                    view.intelligence.settings = response.settings;
                    view.intelligence.model.accept_calltip(
                        target.clone(),
                        &current,
                        response.calltip,
                    );
                    if let Some(completion) = response.completion {
                        let insert = completion.auto_insert;
                        view.intelligence.model.accept_completion(
                            target.clone(),
                            &current,
                            completion,
                        );
                        view.intelligence.model.select_completion(response.selected);
                        view.intelligence
                            .scroll
                            .scroll_to_item(response.selected, gpui::ScrollStrategy::Center);
                        if insert {
                            let event = view.intelligence.model.choose(
                                0,
                                &current,
                                SubmitKey::Click,
                                &view.intelligence.settings.submit,
                            );
                            if let Some(edit) = event.edit {
                                view.apply_intelligence_edit(edit, &target, window, cx);
                            }
                        }
                    } else {
                        view.intelligence.model.dismiss_completion();
                    }
                    if view.completion_visible() {
                        view.present_floating(floating_windows::DialogKind::Completion, window, cx);
                    }
                    if view.calltip_visible() {
                        view.present_floating(floating_windows::DialogKind::Calltip, window, cx);
                    }
                    cx.notify();
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("Intelligence result lost its document window: {error}")
                }
            }
        }));
        cx.notify();
    }

    fn apply_intelligence_edit(
        &mut self,
        edit: CompletionEdit,
        owner: &Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.intelligence_snapshot() != *owner || self.intelligence_blocked() {
            return;
        }
        let result = self.editor_mut().replace_display_range(
            ByteOffset(edit.replace_range.start)..ByteOffset(edit.replace_range.end),
            &edit.text,
        );
        if result.is_ok() {
            window.activate_window();
            self.editor_focus.focus(window, cx);
        }
        self.intelligence.model.dismiss();
        self.editor_changed(result, window, cx);
    }

    pub(super) fn capture_intelligence_action(
        &mut self,
        action: IntelligenceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.intelligence_blocked() {
            return false;
        }
        let owner = self.intelligence_snapshot();
        let event =
            self.intelligence
                .model
                .handle(action, &owner, &self.intelligence.settings.submit);
        if let Some(edit) = event.edit {
            self.apply_intelligence_edit(edit, &owner, window, cx);
        }
        if event.consumed {
            if let Some(popup) = self.intelligence.model.completion() {
                self.intelligence
                    .scroll
                    .scroll_to_item(popup.selected, gpui::ScrollStrategy::Center);
            }
            cx.notify();
        }
        event.consumed
    }

    fn choose_intelligence_completion(
        &mut self,
        index: usize,
        candidate: String,
        owner: Snapshot,
        submit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = owner.clone();
        let routed_candidate = candidate.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.choose_intelligence_completion(index, routed_candidate, routed, submit, window, cx)
        }) {
            return;
        }
        if self.intelligence_snapshot() != owner || self.intelligence_blocked() {
            return;
        }
        if !self
            .intelligence
            .model
            .completion()
            .and_then(|popup| popup.result.candidates.get(index))
            .is_some_and(|current| current.text == candidate)
        {
            return;
        }
        if !submit {
            self.intelligence.model.select_completion(index);
            window.activate_window();
            self.editor_focus.focus(window, cx);
            cx.notify();
            return;
        }
        let event = self.intelligence.model.choose(
            index,
            &owner,
            SubmitKey::Click,
            &self.intelligence.settings.submit,
        );
        if let Some(edit) = event.edit {
            self.apply_intelligence_edit(edit, &owner, window, cx);
        }
        cx.notify();
    }

    pub(super) fn intelligence_popup_bounds(
        &self,
        calltip: bool,
        window: &Window,
        _: &gpui::App,
    ) -> Option<Bounds<Pixels>> {
        let caret = self.intelligence_caret_bounds()?;
        let origin = window.bounds().origin + gpui::point(caret.left(), caret.bottom());
        Some(Bounds::new(
            origin,
            if calltip {
                size(px(520.), px(160.))
            } else {
                size(px(360.), px(240.))
            },
        ))
    }

    pub(super) fn render_completion(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.completion_visible() {
            return None;
        }
        let popup = self.intelligence.model.completion()?;
        let count = popup.result.candidates.len();
        let dark = self.dark;
        let owner = popup.owner.clone();
        Some(
            div()
                .id("intelligence-completion")
                .debug_selector(|| "intelligence-completion".into())
                .role(Role::ListBox)
                .aria_label("Auto-completion candidates")
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(if dark { 0x202020 } else { 0xffffff }))
                .text_color(rgb(if dark { 0xeeeeee } else { 0x202020 }))
                .child(
                    gpui::uniform_list(
                        "intelligence-completion-list",
                        count,
                        cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                            let Some(popup) = view.intelligence.model.completion() else {
                                return Vec::new();
                            };
                            range
                                .filter_map(|index| {
                                    popup.result.candidates.get(index).map(|candidate| {
                                        let owner = owner.clone();
                                        let candidate_text = candidate.text.clone();
                                        div()
                                            .id(("intelligence-candidate", index))
                                            .debug_selector(move || {
                                                format!("intelligence-candidate-{index}")
                                            })
                                            .role(Role::ListBoxOption)
                                            .aria_label(candidate.text.clone())
                                            .aria_selected(index == popup.selected)
                                            .h(px(24.))
                                            .px(px(6.))
                                            .cursor_pointer()
                                            .when(index == popup.selected, |row| {
                                                row.bg(rgb(if dark { 0x30465a } else { 0xcce8ff }))
                                            })
                                            .child(candidate.text.clone())
                                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                cx.stop_propagation()
                                            })
                                            .on_click(cx.listener(
                                                move |view, event: &gpui::ClickEvent, window, cx| {
                                                    view.choose_intelligence_completion(
                                                        index,
                                                        candidate_text.clone(),
                                                        owner.clone(),
                                                        event.click_count() == 2,
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                },
                                            ))
                                    })
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.intelligence.scroll)
                    .flex_1()
                    .min_h(px(0.)),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_calltip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.calltip_visible() {
            return None;
        }
        let popup = self.intelligence.model.calltip()?;
        let help = &popup.help;
        let signature = help.signatures.get(help.active_overload)?;
        let dark = self.dark;
        let active = help.active_parameter_range.clone();
        let label = if let Some(range) = &active {
            div()
                .child(signature.label[..range.start].to_owned())
                .child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(if dark { 0xffff80 } else { 0x004c99 }))
                        .child(signature.label[range.clone()].to_owned()),
                )
                .child(signature.label[range.end..].to_owned())
                .flex()
                .flex_wrap()
        } else {
            div().child(signature.label.clone())
        };
        Some(
            div()
                .id("intelligence-calltip")
                .debug_selector(|| "intelligence-calltip".into())
                .role(Role::Group)
                .aria_label("Function parameters")
                .size_full()
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .bg(rgb(if dark { 0x282820 } else { 0xffffe1 }))
                .text_color(rgb(if dark { 0xeeeeee } else { 0x202020 }))
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(
                            div()
                                .id("calltip-previous")
                                .debug_selector(|| "calltip-previous".into())
                                .role(Role::Button)
                                .aria_label("Previous overload")
                                .tooltip(move |_, cx| {
                                    icon_tooltip::view("Previous overload", dark, cx)
                                })
                                .cursor_pointer()
                                .child("◀")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.cycle_intelligence_overload(false, window, cx);
                                    cx.stop_propagation();
                                })),
                        )
                        .child(format!(
                            "{} / {}",
                            help.active_overload + 1,
                            help.signatures.len()
                        ))
                        .child(
                            div()
                                .id("calltip-next")
                                .debug_selector(|| "calltip-next".into())
                                .role(Role::Button)
                                .aria_label("Next overload")
                                .tooltip(move |_, cx| icon_tooltip::view("Next overload", dark, cx))
                                .cursor_pointer()
                                .child("▶")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.cycle_intelligence_overload(true, window, cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .child(label)
                .child(signature.description.clone())
                .into_any_element(),
        )
    }

    fn cycle_intelligence_overload(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.cycle_intelligence_overload(forward, window, cx)
        }) {
            return;
        }
        self.capture_intelligence_action(
            if forward {
                IntelligenceAction::NextOverload
            } else {
                IntelligenceAction::PreviousOverload
            },
            window,
            cx,
        );
    }
}
