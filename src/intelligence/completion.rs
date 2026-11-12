use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;

use super::{IntelligenceCatalog, IntelligenceError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionMode {
    Api,
    Words,
    #[default]
    ApiAndWords,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionOrder {
    #[default]
    Sorted,
    Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionOptions {
    pub mode: CompletionMode,
    pub order: CompletionOrder,
    pub ignore_numbers: bool,
    pub minimum_prefix: usize,
    pub ignore_case: Option<bool>,
    pub auto_insert_single: bool,
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            mode: CompletionMode::ApiAndWords,
            order: CompletionOrder::Sorted,
            ignore_numbers: true,
            minimum_prefix: 1,
            ignore_case: None,
            auto_insert_single: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Keyword,
    Word,
    File,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub text: String,
    pub kind: CompletionKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionResult {
    pub replace_range: Range<usize>,
    pub candidates: Vec<CompletionCandidate>,
    pub auto_insert: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitKey {
    Enter,
    Tab,
    Character(char),
    Click,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitOptions {
    pub enter: bool,
    pub tab: bool,
    pub fill_up_characters: String,
}

impl Default for SubmitOptions {
    fn default() -> Self {
        Self {
            enter: true,
            tab: true,
            fill_up_characters: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionEdit {
    pub replace_range: Range<usize>,
    pub text: String,
    pub caret_byte: usize,
}

impl CompletionResult {
    pub fn submit(
        &self,
        selected: usize,
        key: SubmitKey,
        options: &SubmitOptions,
    ) -> Option<CompletionEdit> {
        let suffix = match key {
            SubmitKey::Enter if !options.enter => return None,
            SubmitKey::Tab if !options.tab => return None,
            SubmitKey::Character(ch) if options.fill_up_characters.contains(ch) => ch.to_string(),
            SubmitKey::Character(_) => return None,
            _ => String::new(),
        };
        let candidate = self.candidates.get(selected)?;
        let text = format!("{}{suffix}", candidate.text);
        Some(CompletionEdit {
            caret_byte: self.replace_range.start + text.len(),
            replace_range: self.replace_range.clone(),
            text,
        })
    }
}

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn word_end_char(ch: char) -> bool {
    !ch.is_whitespace() && !".,;:\"(){}=<>'+!?[]".contains(ch)
}

fn prefix_matches(word: &str, prefix: &str, ignore_case: bool) -> bool {
    if ignore_case {
        word.to_lowercase().starts_with(&prefix.to_lowercase())
    } else {
        word.starts_with(prefix)
    }
}

pub(crate) fn check_caret(text: &str, caret: usize) -> Result<(), IntelligenceError> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return Err(IntelligenceError(
            "caret is not a UTF-8 byte boundary".into(),
        ));
    }
    Ok(())
}

impl IntelligenceCatalog {
    pub fn complete(
        &self,
        source_key: &str,
        text: &str,
        caret: usize,
        options: &CompletionOptions,
    ) -> Result<CompletionResult, IntelligenceError> {
        check_caret(text, caret)?;
        let api = self.api(source_key);
        let ignore_case = options
            .ignore_case
            .unwrap_or_else(|| api.is_some_and(|api| api.environment.ignore_case));
        let start = text[..caret]
            .char_indices()
            .rev()
            .take_while(|(_, ch)| word_char(*ch))
            .last()
            .map_or(caret, |(offset, _)| offset);
        let prefix = &text[start..caret];
        let end = text[caret..]
            .char_indices()
            .find(|(_, ch)| !word_char(*ch))
            .map_or(text.len(), |(offset, _)| caret + offset);
        let excluded = &text[start..end];
        let mut result = CompletionResult {
            replace_range: start..caret,
            candidates: Vec::new(),
            auto_insert: false,
        };
        if prefix.chars().count() < options.minimum_prefix
            || prefix.len() >= 256
            || prefix.is_empty()
        {
            return Ok(result);
        }
        let mut seen = BTreeSet::new();
        if options.mode != CompletionMode::Api
            && !(options.ignore_numbers && prefix.chars().all(|c| c.is_ascii_digit()))
        {
            let mut previous_is_word = false;
            for (cursor, ch) in text.char_indices() {
                let is_word = word_char(ch);
                let is_start = is_word && !previous_is_word;
                previous_is_word = is_word;
                if !is_start {
                    continue;
                }
                let length = text[cursor..]
                    .char_indices()
                    .find(|(_, ch)| !word_end_char(*ch))
                    .map_or(text.len() - cursor, |(offset, _)| offset);
                if length == 0 {
                    continue;
                }
                let word = &text[cursor..cursor + length];
                if word.len() > prefix.len()
                    && word.len() < 256
                    && word != excluded
                    && prefix_matches(word, prefix, ignore_case)
                    && seen.insert(word.to_owned())
                {
                    let kind = api
                        .and_then(|api| api.keywords.iter().find(|kw| kw.name == word))
                        .map_or(CompletionKind::Word, |kw| {
                            if kw.is_function {
                                CompletionKind::Function
                            } else {
                                CompletionKind::Keyword
                            }
                        });
                    result.candidates.push(CompletionCandidate {
                        text: word.into(),
                        kind,
                    });
                }
            }
        }
        if options.mode != CompletionMode::Words
            && let Some(api) = api
        {
            for keyword in &api.keywords {
                if prefix_matches(&keyword.name, prefix, ignore_case)
                    && seen.insert(keyword.name.clone())
                {
                    result.candidates.push(CompletionCandidate {
                        text: keyword.name.clone(),
                        kind: if keyword.is_function {
                            CompletionKind::Function
                        } else {
                            CompletionKind::Keyword
                        },
                    });
                }
            }
        }
        if options.order == CompletionOrder::Sorted {
            result.candidates.sort_by(|left, right| {
                if ignore_case {
                    left.text
                        .to_lowercase()
                        .cmp(&right.text.to_lowercase())
                        .then_with(|| left.text.cmp(&right.text))
                } else {
                    left.text.cmp(&right.text)
                }
            });
        }
        result.auto_insert = options.mode == CompletionMode::Words
            && options.auto_insert_single
            && result.candidates.len() == 1;
        Ok(result)
    }

    pub fn complete_path(
        &self,
        text: &str,
        caret: usize,
        base_directory: &Path,
    ) -> Result<CompletionResult, IntelligenceError> {
        check_caret(text, caret)?;
        let context = path_context(text, caret)?;
        let Some((range, raw)) = context else {
            return Ok(CompletionResult {
                replace_range: caret..caret,
                candidates: Vec::new(),
                auto_insert: false,
            });
        };
        let path = Path::new(&raw);
        let resolved_path = if path.is_absolute() {
            path.to_owned()
        } else {
            base_directory.join(path)
        };
        if resolved_path.is_file() {
            return Ok(CompletionResult {
                replace_range: range,
                candidates: Vec::new(),
                auto_insert: false,
            });
        }
        let separator = if raw.contains('\\') { '\\' } else { '/' };
        let last = raw
            .rfind(separator)
            .map_or(0, |offset| offset + separator.len_utf8());
        let directory_text = if resolved_path.is_dir() {
            raw.clone()
        } else {
            raw[..last].to_owned()
        };
        let directory = if Path::new(&directory_text).is_absolute() {
            Path::new(&directory_text).to_owned()
        } else {
            base_directory.join(&directory_text)
        };
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            IntelligenceError(format!("path completion {}: {error}", directory.display()))
        })?;
        let mut candidates = Vec::new();
        for entry in entries.take(2000) {
            let entry = entry
                .map_err(|error| IntelligenceError(format!("path completion entry: {error}")))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_directory = entry
                .file_type()
                .map_err(|error| IntelligenceError(error.to_string()))?
                .is_dir();
            let mut value = directory_text.clone();
            if !value.ends_with(separator) && !value.is_empty() {
                value.push(separator);
            }
            value.push_str(&name);
            if is_directory {
                value.push(separator);
            }
            if prefix_matches(&value, &raw, true) {
                candidates.push(CompletionCandidate {
                    text: value,
                    kind: if is_directory {
                        CompletionKind::Directory
                    } else {
                        CompletionKind::File
                    },
                });
            }
        }
        candidates.sort_by_key(|candidate| candidate.text.to_lowercase());
        Ok(CompletionResult {
            replace_range: range,
            candidates,
            auto_insert: false,
        })
    }
}

pub fn path_context(
    text: &str,
    caret: usize,
) -> Result<Option<(Range<usize>, String)>, IntelligenceError> {
    check_caret(text, caret)?;
    let before = &text[..caret];
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let line = &before[line_start..];
    let drive = line.rfind(':').and_then(|colon| {
        if colon == 0 || !line.as_bytes()[colon - 1].is_ascii_alphabetic() {
            return None;
        }
        if colon >= 2
            && !line[..colon - 1]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace() || "\"'(".contains(c))
        {
            return None;
        }
        Some(colon - 1)
    });
    let start = drive.or_else(|| {
        let quote = line.rfind(['\'', '"']).map(|at| at + 1);
        if let Some(at) = quote
            && line[at..].contains('/')
        {
            return Some(at);
        }
        let at = line.rfind(char::is_whitespace).map_or(0, |at| at + 1);
        if line[at..].contains('/') {
            Some(at)
        } else {
            None
        }
    });
    Ok(start.map(|at| (line_start + at..caret, line[at..].to_owned())))
}
