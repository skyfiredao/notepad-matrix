use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use serde_json::{Value, json};

use crate::config::ConfigStore;
use crate::menu::BookmarkCommand;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroCommand {
    StartRecording,
    StopRecording,
    Playback,
    SaveCurrent,
    RunMultiple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Count(u16),
    UntilEnd,
}

impl RepeatMode {
    pub fn from_count_input(text: &str) -> io::Result<Self> {
        if text.chars().count() > 4 {
            return Err(invalid("Repeat count accepts at most four digits"));
        }
        Ok(Self::Count(text.parse::<u16>().unwrap_or(0).max(1)))
    }
}

#[derive(Clone, Debug)]
pub struct RepeatTracker {
    mode: RepeatMode,
    completed: usize,
    last_line: i128,
    current_line: i128,
    delta_last: i128,
    delta_current: i128,
    cursor_moved_up: bool,
    finished: bool,
}

impl RepeatTracker {
    pub fn new(mode: RepeatMode, line_count: usize, current_line: usize) -> io::Result<Self> {
        if matches!(mode, RepeatMode::Count(0 | 10000..)) {
            return Err(invalid("Repeat count must be between 1 and 9999"));
        }
        if line_count == 0 || current_line >= line_count {
            return Err(invalid("Invalid macro playback line position"));
        }
        Ok(Self {
            mode,
            completed: 0,
            last_line: i128::try_from(line_count - 1)
                .map_err(|_| invalid("Macro line count overflow"))?,
            current_line: i128::try_from(current_line)
                .map_err(|_| invalid("Macro line position overflow"))?,
            delta_last: 0,
            delta_current: 0,
            cursor_moved_up: false,
            finished: false,
        })
    }

    pub fn completed(&self) -> usize {
        self.completed
    }
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    pub fn cancel(&mut self) {
        self.finished = true;
    }

    pub fn complete_iteration(&mut self, line_count: usize, current_line: usize) -> bool {
        if self.finished {
            return false;
        }
        self.completed += 1;
        if let RepeatMode::Count(count) = self.mode {
            self.finished = self.completed >= usize::from(count);
            return !self.finished;
        }
        let (Ok(last), Ok(current)) = (
            i128::try_from(line_count.saturating_sub(1)),
            i128::try_from(current_line),
        ) else {
            self.finished = true;
            return false;
        };
        if line_count == 0
            || current_line >= line_count
            || (self.completed > 2
                && self.cursor_moved_up != (self.delta_current < 0)
                && self.delta_last >= 0)
        {
            self.finished = true;
            return false;
        }
        self.cursor_moved_up = self.delta_current < 0;
        self.delta_last = last - self.last_line;
        self.delta_current = current - self.current_line;
        if self.delta_current == 0 && self.delta_last >= 0 {
            self.finished = true;
            return false;
        }
        if self.delta_last < self.delta_current {
            self.last_line += self.delta_last;
        }
        self.current_line = current;
        self.finished = self.current_line > self.last_line
            || self.current_line < 0
            || (self.delta_current == 0
                && self.current_line == 0
                && (self.delta_last >= 0 || self.cursor_moved_up));
        !self.finished
    }
}

/// Recorded editing messages, not document snapshots or OS keystrokes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroStep {
    ReplaceSelection(Arc<str>),
    Newline,
    Tab,
    BackTab,
    Character {
        right: bool,
        extend: bool,
    },
    Vertical {
        down: bool,
        extend: bool,
    },
    LineEdge {
        end: bool,
        extend: bool,
    },
    DocumentEdge {
        end: bool,
        extend: bool,
    },
    Word {
        forward: bool,
        extend: bool,
        part: bool,
    },
    DisplayEdge {
        end: bool,
    },
    Page {
        down: bool,
        extend: bool,
        rectangle: bool,
    },
    DeleteWord {
        forward: bool,
        to_line_edge: bool,
    },
    BackspaceNotLine,
    TextTransform(u32),
    MultiSelection(crate::menu::MultiSelectionCommand),
    CopyPath(crate::clipboard::CopyPath),
    Rectangle(RectangleMovement),
    CharacterText(Arc<str>),
    Delete {
        right: bool,
    },
    ClearSelection,
    DeleteLine,
    Duplicate {
        line: bool,
    },
    SelectAll,
    Copy {
        cut: bool,
    },
    Paste,
    Bookmark(crate::menu::BookmarkCommand),
    GoToLine(usize),
    Unsupported(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectangleMovement {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl RectangleMovement {
    pub fn vector(self) -> (i32, i32, Option<bool>) {
        match self {
            Self::Left => (-1, 0, None),
            Self::Right => (1, 0, None),
            Self::Up => (0, -1, None),
            Self::Down => (0, 1, None),
            Self::Home => (0, 0, Some(false)),
            Self::End => (0, 0, Some(true)),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
            Self::Home => "home",
            Self::End => "end",
        }
    }
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedMacro {
    pub name: String,
    pub steps: Vec<MacroStep>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MacroLibrary {
    pub macros: Vec<SavedMacro>,
}

pub struct MacroSaveTask {
    completion: Arc<Mutex<SaveCompletion>>,
}

struct SaveCompletion {
    result: Option<io::Result<MacroLibrary>>,
    waker: Option<Waker>,
}

impl MacroSaveTask {
    pub async fn after_publication(
        previous: impl Future<Output = ()>,
        home: std::path::PathBuf,
        name: String,
        steps: Vec<MacroStep>,
    ) -> io::Result<MacroLibrary> {
        previous.await;
        Self::start(home, name, steps)?.await
    }

    pub fn start(
        home: std::path::PathBuf,
        name: String,
        steps: Vec<MacroStep>,
    ) -> io::Result<Self> {
        let completion = Arc::new(Mutex::new(SaveCompletion {
            result: None,
            waker: None,
        }));
        let worker_completion = completion.clone();
        let _worker = std::thread::Builder::new()
            .name("macro-save".into())
            .spawn(move || {
                let result =
                    MacroLibrary::save_current(&ConfigStore::under_home(&home), &name, &steps);
                let waker = match worker_completion.lock() {
                    Ok(mut completion) => {
                        completion.result = Some(result);
                        completion.waker.take()
                    }
                    Err(error) => {
                        let mut completion = error.into_inner();
                        completion.result = Some(Err(io::Error::other(
                            "Macro save result handoff was poisoned",
                        )));
                        completion.waker.take()
                    }
                };
                if let Some(waker) = waker {
                    waker.wake();
                }
            })?;
        Ok(Self { completion })
    }
}

impl Future for MacroSaveTask {
    type Output = io::Result<MacroLibrary>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.completion.lock() {
            Ok(mut completion) => {
                if let Some(result) = completion.result.take() {
                    Poll::Ready(result)
                } else {
                    completion.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
            Err(error) => Poll::Ready(Err(io::Error::other(format!(
                "Macro save result handoff failed: {error}"
            )))),
        }
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl MacroLibrary {
    pub fn load(store: &ConfigStore) -> io::Result<Self> {
        match store.load_json(Path::new("macros.json")) {
            Ok(document) => Self::from_json(&document),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn validate_steps(steps: &[MacroStep]) -> io::Result<()> {
        if steps.is_empty() {
            return Err(invalid("There is no recorded macro"));
        }
        for step in steps {
            if let MacroStep::MultiSelection(command) = step
                && MacroStep::multi_selection(command.source_id()).is_none()
            {
                return Err(invalid(
                    "Unknown or unrecordable multiple selection command",
                ));
            }
            if let MacroStep::TextTransform(id) = step
                && MacroStep::text_transform(*id).is_none()
            {
                return Err(invalid("Unknown or unrecordable text transformation"));
            }
            if let MacroStep::Unsupported(name) = step {
                return Err(invalid(format!(
                    "Macro contains unsupported operation: {name}"
                )));
            }
        }
        Ok(())
    }

    pub fn save_current(store: &ConfigStore, name: &str, steps: &[MacroStep]) -> io::Result<Self> {
        if name.is_empty() {
            return Err(invalid("A macro name is required"));
        }
        Self::validate_steps(steps)?;
        let entry = json!({
            "name": name,
            "steps": steps.iter().map(MacroStep::to_json).collect::<io::Result<Vec<_>>>()?
        });
        let document = store.update_json(
            Path::new("macros.json"),
            &json!({"schema_version": 1, "macros": []}),
            |document| {
                Self::from_json(document)?;
                let entries = document
                    .get_mut("macros")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| invalid("macros.json requires a macros array"))?;
                entries.push(entry);
                Ok(())
            },
        )?;
        Self::from_json(&document)
    }

    fn from_json(document: &Value) -> io::Result<Self> {
        if document.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(invalid("macros.json requires schema_version 1"));
        }
        let entries = document
            .get("macros")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("macros.json requires a macros array"))?;
        let mut macros = Vec::with_capacity(entries.len());
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| invalid("A macro name is required"))?;
            let steps = entry
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("A macro requires typed steps"))?
                .iter()
                .map(MacroStep::from_json)
                .collect::<io::Result<Vec<_>>>()?;
            Self::validate_steps(&steps)?;
            macros.push(SavedMacro {
                name: name.into(),
                steps,
            });
        }
        Ok(Self { macros })
    }
}

impl MacroStep {
    pub fn multi_selection(id: u32) -> Option<Self> {
        (42092..=42099)
            .contains(&id)
            .then(|| crate::menu::MultiSelectionCommand::from_source_id(id))
            .flatten()
            .map(Self::MultiSelection)
    }

    pub fn text_transform(id: u32) -> Option<Self> {
        crate::text_transforms::TransformCommand::from_source_id(id)
            .filter(|_| id != 42047)
            .map(|_| Self::TextTransform(id))
    }

    fn to_json(&self) -> io::Result<Value> {
        Ok(match self {
            Self::ReplaceSelection(text) => json!({"type":"text", "text":text.as_ref()}),
            Self::CharacterText(text) => json!({"type":"character_text", "text":text.as_ref()}),
            Self::Newline => json!({"type":"newline"}),
            Self::Tab => json!({"type":"tab"}),
            Self::BackTab => json!({"type":"back_tab"}),
            Self::Character { right, extend } => {
                json!({"type":"character", "right":right, "extend":extend})
            }
            Self::Vertical { down, extend } => {
                json!({"type":"vertical", "down":down, "extend":extend})
            }
            Self::LineEdge { end, extend } => {
                json!({"type":"line_edge", "end":end, "extend":extend})
            }
            Self::DocumentEdge { end, extend } => {
                json!({"type":"document_edge", "end":end, "extend":extend})
            }
            Self::Word {
                forward,
                extend,
                part,
            } => json!({"type":"word", "forward":forward, "extend":extend, "part":part}),
            Self::DisplayEdge { end } => json!({"type":"display_edge", "end":end}),
            Self::Page {
                down,
                extend,
                rectangle,
            } => json!({"type":"page", "down":down, "extend":extend, "rectangle":rectangle}),
            Self::DeleteWord {
                forward,
                to_line_edge,
            } => json!({"type":"delete_word", "forward":forward, "to_line_edge":to_line_edge}),
            Self::BackspaceNotLine => json!({"type":"backspace_not_line"}),
            Self::TextTransform(id) => json!({"type":"text_transform", "source_id":id}),
            Self::MultiSelection(command) => {
                json!({"type":"multi_selection", "source_id":command.source_id()})
            }
            Self::Delete { right } => json!({"type":"delete", "right":right}),
            Self::ClearSelection => json!({"type":"clear_selection"}),
            Self::DeleteLine => json!({"type":"delete_line"}),
            Self::Duplicate { line } => json!({"type":"duplicate", "line":line}),
            Self::SelectAll => json!({"type":"select_all"}),
            Self::Copy { cut } => json!({"type":"copy", "cut":cut}),
            Self::Paste => json!({"type":"paste"}),
            Self::Rectangle(command) => json!({"type":"rectangle", "command":command.name()}),
            Self::CopyPath(command) => json!({"type":"copy_path", "source_id":command.source_id()}),
            Self::Bookmark(command) => json!({"type":"bookmark", "command":match command {
                BookmarkCommand::Toggle => "toggle", BookmarkCommand::Next => "next",
                BookmarkCommand::Previous => "previous", BookmarkCommand::ClearAll => "clear_all",
            }}),
            Self::GoToLine(line) => json!({"type":"goto_line", "line":line}),
            Self::Unsupported(name) => {
                return Err(invalid(format!("Unsupported macro operation: {name}")));
            }
        })
    }

    fn from_json(value: &Value) -> io::Result<Self> {
        let flag = |name| {
            value
                .get(name)
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid(format!("Macro step {name} must be a boolean")))
        };
        Ok(match value.get("type").and_then(Value::as_str) {
            Some("character_text") => Self::CharacterText(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("Character panel text must be a string"))?
                    .into(),
            ),
            Some("text") => Self::ReplaceSelection(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("Macro text must be a string"))?
                    .into(),
            ),
            Some("newline") => Self::Newline,
            Some("tab") => Self::Tab,
            Some("back_tab") => Self::BackTab,
            Some("character") => Self::Character {
                right: flag("right")?,
                extend: flag("extend")?,
            },
            Some("vertical") => Self::Vertical {
                down: flag("down")?,
                extend: flag("extend")?,
            },
            Some("line_edge") => Self::LineEdge {
                end: flag("end")?,
                extend: flag("extend")?,
            },
            Some("document_edge") => Self::DocumentEdge {
                end: flag("end")?,
                extend: flag("extend")?,
            },
            Some("word") => Self::Word {
                forward: flag("forward")?,
                extend: flag("extend")?,
                part: flag("part")?,
            },
            Some("display_edge") => Self::DisplayEdge { end: flag("end")? },
            Some("page") => Self::Page {
                down: flag("down")?,
                extend: flag("extend")?,
                rectangle: flag("rectangle")?,
            },
            Some("delete_word") => Self::DeleteWord {
                forward: flag("forward")?,
                to_line_edge: flag("to_line_edge")?,
            },
            Some("backspace_not_line") => Self::BackspaceNotLine,
            Some("text_transform") => value
                .get("source_id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
                .and_then(Self::text_transform)
                .ok_or_else(|| invalid("Unknown or unrecordable text transformation"))?,
            Some("multi_selection") => value
                .get("source_id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
                .and_then(Self::multi_selection)
                .ok_or_else(|| invalid("Unknown or unrecordable multiple selection command"))?,
            Some("delete") => Self::Delete {
                right: flag("right")?,
            },
            Some("clear_selection") => Self::ClearSelection,
            Some("delete_line") => Self::DeleteLine,
            Some("duplicate") => Self::Duplicate {
                line: flag("line")?,
            },
            Some("select_all") => Self::SelectAll,
            Some("copy") => Self::Copy { cut: flag("cut")? },
            Some("paste") => Self::Paste,
            Some("rectangle") => value
                .get("command")
                .and_then(Value::as_str)
                .and_then(RectangleMovement::from_name)
                .map(Self::Rectangle)
                .ok_or_else(|| invalid("Unknown rectangular movement"))?,
            Some("copy_path") => value
                .get("source_id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
                .and_then(crate::clipboard::CopyPath::from_source_id)
                .map(Self::CopyPath)
                .ok_or_else(|| invalid("Unknown path copy command"))?,
            Some("bookmark") => {
                Self::Bookmark(match value.get("command").and_then(Value::as_str) {
                    Some("toggle") => BookmarkCommand::Toggle,
                    Some("next") => BookmarkCommand::Next,
                    Some("previous") => BookmarkCommand::Previous,
                    Some("clear_all") => BookmarkCommand::ClearAll,
                    _ => return Err(invalid("Unknown bookmark macro operation")),
                })
            }
            Some("goto_line") => Self::GoToLine(
                value
                    .get("line")
                    .and_then(Value::as_u64)
                    .and_then(|line| usize::try_from(line).ok())
                    .ok_or_else(|| invalid("Invalid macro line"))?,
            ),
            _ => {
                return Err(invalid(
                    "Unknown macro step type; no commands were executed",
                ));
            }
        })
    }
}
