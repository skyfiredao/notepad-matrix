use crate::editor::Selection;
use crate::input::DisplayRevision;
use crate::intelligence::{
    CompletionEdit, CompletionMode, CompletionOptions, CompletionResult, SignatureHelp, SubmitKey,
    SubmitOptions,
};
use crate::syntax::Language;
use crate::workspace::{BufferId, ViewId};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub buffer: BufferId,
    pub view: ViewId,
    pub revision: DisplayRevision,
    pub selection: Selection,
    pub language: Language,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Enter,
    Tab,
    Up,
    Down,
    Escape,
    PreviousOverload,
    NextOverload,
}

pub struct CompletionPopup {
    pub owner: Snapshot,
    pub result: CompletionResult,
    pub selected: usize,
}

pub struct CalltipPopup {
    pub owner: Snapshot,
    pub help: SignatureHelp,
}

#[derive(Default)]
pub struct IntelligenceModel {
    completion: Option<CompletionPopup>,
    calltip: Option<CalltipPopup>,
    pending: Option<Request>,
}

#[derive(Clone)]
pub struct Request {
    identity: Arc<()>,
    owner: Snapshot,
}

#[derive(Default)]
pub struct EventResult {
    pub consumed: bool,
    pub edit: Option<CompletionEdit>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub maintain_indent: crate::indentation::Mode,
    pub auto_insert: crate::intelligence::auto_insert::Settings,
    pub automatic: Option<CompletionMode>,
    pub completion: CompletionOptions,
    pub submit: SubmitOptions,
    pub function_parameters: bool,
    pub brief: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_insert: Default::default(),
            maintain_indent: Default::default(),
            automatic: Some(CompletionMode::ApiAndWords),
            completion: CompletionOptions::default(),
            submit: SubmitOptions::default(),
            function_parameters: true,
            brief: false,
        }
    }
}

impl Settings {
    pub fn from_json(settings: &serde_json::Value) -> Self {
        fn boolean(value: &serde_json::Value, fallback: bool) -> bool {
            value
                .as_bool()
                .or_else(|| {
                    value.as_str().and_then(|value| match value {
                        "yes" => Some(true),
                        "no" => Some(false),
                        _ => None,
                    })
                })
                .unwrap_or(fallback)
        }
        let mut result = Self {
            maintain_indent: crate::indentation::Mode::from_json(settings),
            auto_insert: crate::intelligence::auto_insert::Settings::from_json(settings),
            ..Self::default()
        };
        let settings = &settings["auto-completion"];
        result.automatic = match settings["autoCAction"].as_u64() {
            Some(0) => None,
            Some(1) => Some(CompletionMode::Api),
            Some(2) => Some(CompletionMode::Words),
            _ => Some(CompletionMode::ApiAndWords),
        };
        if let Some(length) = settings["triggerFromNbChar"].as_u64() {
            result.completion.minimum_prefix = length.clamp(1, 9) as usize;
        }
        result.completion.ignore_numbers = boolean(&settings["autoCIgnoreNumbers"], true);
        result.submit.enter = boolean(&settings["insertSelectedItemUseENTER"], true);
        result.submit.tab = boolean(&settings["insertSelectedItemUseTAB"], true);
        result.function_parameters = boolean(&settings["funcParams"], true);
        result.brief = boolean(&settings["autoCBrief"], false);
        result
    }
}

impl IntelligenceModel {
    pub fn begin_request(&mut self, owner: Snapshot) -> Request {
        let request = Request {
            identity: Arc::new(()),
            owner,
        };
        self.pending = Some(request.clone());
        request
    }

    pub fn finish_request(&mut self, request: &Request, current: &Snapshot) -> bool {
        if !self
            .pending
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(&pending.identity, &request.identity))
        {
            return false;
        }
        self.pending = None;
        request.owner == *current
    }

    pub fn completion(&self) -> Option<&CompletionPopup> {
        self.completion.as_ref()
    }

    pub fn select_completion(&mut self, selected: usize) {
        if let Some(popup) = self.completion.as_mut() {
            popup.selected = selected.min(popup.result.candidates.len().saturating_sub(1));
        }
    }
    pub fn calltip(&self) -> Option<&CalltipPopup> {
        self.calltip.as_ref()
    }
    pub fn completion_visible(&self) -> bool {
        self.completion.is_some()
    }
    pub fn calltip_visible(&self) -> bool {
        self.calltip.is_some()
    }
    pub fn dismiss_completion(&mut self) {
        self.completion = None;
        self.pending = None;
    }
    pub fn dismiss_calltip(&mut self) {
        self.calltip = None;
        self.pending = None;
    }
    pub fn dismiss(&mut self) {
        self.dismiss_completion();
        self.dismiss_calltip();
    }

    pub fn accept_completion(
        &mut self,
        owner: Snapshot,
        current: &Snapshot,
        result: CompletionResult,
    ) -> bool {
        if owner != *current {
            return false;
        }
        self.completion = if result.candidates.is_empty() {
            None
        } else {
            Some(CompletionPopup {
                owner,
                result,
                selected: 0,
            })
        };
        self.completion_visible()
    }

    pub fn accept_calltip(
        &mut self,
        owner: Snapshot,
        current: &Snapshot,
        help: Option<SignatureHelp>,
    ) -> bool {
        if owner != *current {
            return false;
        }
        self.calltip = help.map(|help| CalltipPopup { owner, help });
        self.calltip_visible()
    }

    pub fn invalidate(&mut self, current: &Snapshot) {
        if self
            .completion
            .as_ref()
            .is_some_and(|popup| popup.owner != *current)
        {
            self.dismiss_completion();
        }
        if self
            .calltip
            .as_ref()
            .is_some_and(|popup| popup.owner != *current)
        {
            self.dismiss_calltip();
        }
    }

    pub fn choose(
        &mut self,
        selected: usize,
        current: &Snapshot,
        key: SubmitKey,
        options: &SubmitOptions,
    ) -> EventResult {
        self.invalidate(current);
        let Some(popup) = &self.completion else {
            return EventResult::default();
        };
        let edit = popup.result.submit(selected, key, options);
        let consumed = edit.is_some();
        if consumed {
            self.dismiss_completion();
        }
        EventResult { consumed, edit }
    }

    pub fn handle(
        &mut self,
        action: Action,
        current: &Snapshot,
        options: &SubmitOptions,
    ) -> EventResult {
        self.invalidate(current);
        if action == Action::Escape {
            let consumed = self.completion_visible() || self.calltip_visible();
            self.dismiss();
            return EventResult {
                consumed,
                edit: None,
            };
        }
        if matches!(action, Action::PreviousOverload | Action::NextOverload) {
            if let Some(popup) = self.calltip.as_mut() {
                popup.help.cycle_overload(action == Action::NextOverload);
                return EventResult {
                    consumed: true,
                    edit: None,
                };
            }
            return EventResult::default();
        }
        let Some(popup) = self.completion.as_mut() else {
            return EventResult::default();
        };
        match action {
            Action::Up | Action::Down => {
                let count = popup.result.candidates.len();
                popup.selected = if action == Action::Down {
                    (popup.selected + 1).min(count - 1)
                } else {
                    popup.selected.saturating_sub(1)
                };
                EventResult {
                    consumed: true,
                    edit: None,
                }
            }
            Action::Enter | Action::Tab => {
                let selected = popup.selected;
                self.choose(
                    selected,
                    current,
                    if action == Action::Enter {
                        SubmitKey::Enter
                    } else {
                        SubmitKey::Tab
                    },
                    options,
                )
            }
            _ => EventResult::default(),
        }
    }
}
