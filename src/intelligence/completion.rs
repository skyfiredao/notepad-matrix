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
    pub api_brief: bool,
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
            api_brief: false,
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
    pub bytes: Vec<u8>,
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
    pub bytes: Vec<u8>,
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
        let mut bytes = candidate.bytes.clone();
        bytes.extend_from_slice(suffix.as_bytes());
        Some(CompletionEdit {
            caret_byte: self.replace_range.start + bytes.len(),
            replace_range: self.replace_range.clone(),
            bytes,
        })
    }
}

pub(crate) fn prefix_matches(word: &[u8], prefix: &[u8], ignore_case: bool) -> bool {
    if ignore_case {
        word.get(..prefix.len()).is_some_and(|word| word.eq_ignore_ascii_case(prefix))
    } else {
        word.starts_with(prefix)
    }
}

impl IntelligenceCatalog {
    pub fn complete(
        &self,
        source_key: &str,
        text: &crate::byte_buffer::ByteBuffer,
        caret: usize,
        options: &CompletionOptions,
    ) -> Result<CompletionResult, IntelligenceError> {
        if caret > text.len() { return Err(IntelligenceError("The caret is outside the document".into())); }
        let api = self.try_api(source_key)?;
        let ignore_case = options
            .ignore_case
            .unwrap_or_else(|| api.is_some_and(|api| api.environment.ignore_case));
        let start = text.extend_word(caret, false).map_err(|error| IntelligenceError(error.to_string()))?;
        let end = text.extend_word(caret, true).map_err(|error| IntelligenceError(error.to_string()))?;
        let prefix = text.slice(start..caret).map_err(|error| IntelligenceError(error.to_string()))?.to_vec();
        let excluded = text.slice(start..end).map_err(|error| IntelligenceError(error.to_string()))?.to_vec();
        let mut result = CompletionResult {
            replace_range: start..caret,
            candidates: Vec::new(),
            auto_insert: false,
        };
        let full_api = options.mode == CompletionMode::Api && !options.api_brief;
        let maximum = if full_api {
            api.map_or(0, |api| api.keywords.iter().map(|keyword| keyword.name.len()).max().unwrap_or(0))
        } else { 256 };
        if prefix.len() < options.minimum_prefix || prefix.len() >= maximum || prefix.is_empty()
            || !full_api && excluded.len() >= 256 {
            return Ok(result);
        }
        let mut seen = BTreeSet::new();
        if options.mode != CompletionMode::Api
            && !(options.ignore_numbers && prefix.iter().all(u8::is_ascii_digit))
        {
            let mut cursor = 0;
            while cursor < text.len() {
                let Some(found) = text.find_literal_bytes(&prefix, cursor, text.len(), !ignore_case, false, true)
                    .map_err(|error| IntelligenceError(error.to_string()))? else { break; };
                let mut end = found.end;
                while let Some(byte) = text.byte(end) {
                    if b" \t\n\r.,;:\"(){}=<>'+!?[]".contains(&byte) { break; }
                    end += 1;
                }
                cursor = end.max(found.end);
                if end == found.end || end - found.start >= 256 { continue; }
                let mut word = text.slice(found.start..end).map_err(|error| IntelligenceError(error.to_string()))?.to_vec();
                if let Some(end) = word.iter().position(|&byte| byte == 0) { word.truncate(end); }
                if word != excluded && seen.insert(word.clone()) {
                    let kind = api
                        .and_then(|api| api.keywords.iter().find(|kw| kw.name.as_bytes() == word))
                        .map_or(CompletionKind::Word, |kw| {
                            if kw.is_function {
                                CompletionKind::Function
                            } else {
                                CompletionKind::Keyword
                            }
                        });
                    result.candidates.push(CompletionCandidate {
                        text: String::from_utf8_lossy(&word).into_owned(),
                        bytes: word,
                        kind,
                    });
                }
            }
        }
        if options.mode != CompletionMode::Words
            && let Some(api) = api
        {
            for keyword in &api.keywords {
                if (full_api || prefix_matches(keyword.name.as_bytes(), &prefix, ignore_case))
                    && seen.insert(keyword.name.as_bytes().to_vec())
                {
                    result.candidates.push(CompletionCandidate {
                        text: keyword.name.clone(),
                        bytes: keyword.name.as_bytes().to_vec(),
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
            result.candidates.sort_unstable_by(|left, right| {
                if ignore_case {
                    left.bytes.iter().map(u8::to_ascii_uppercase).cmp(right.bytes.iter().map(u8::to_ascii_uppercase))
                } else {
                    left.bytes.cmp(&right.bytes)
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
        text: &crate::byte_buffer::ByteBuffer,
        caret: usize,
        base_directory: &Path,
    ) -> Result<CompletionResult, IntelligenceError> {
        if caret > text.len() { return Err(IntelligenceError("The caret is outside the document".into())); }
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
        let authorization_directory = if raw.ends_with('/') {
            resolved_path.as_path()
        } else {
            resolved_path.parent().unwrap_or(base_directory)
        };
        let _parent_access = crate::file_access::directory(authorization_directory)
            .map_err(|error| IntelligenceError(format!("Path completion directory: {error}")))?;
        if Path::new(raw.trim_end_matches('/')).is_file() {
            return Ok(CompletionResult {
                replace_range: range,
                candidates: Vec::new(),
                auto_insert: false,
            });
        }
        let separator = '/';
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
        let _access = crate::file_access::directory(&directory)
            .map_err(|error| IntelligenceError(format!("Path completion directory: {error}")))?;
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => return Ok(CompletionResult { replace_range: range, candidates: Vec::new(), auto_insert: false }),
        };
        let mut candidates = Vec::new();
        for entry in entries.take(2000) {
            let entry = entry
                .map_err(|error| IntelligenceError(format!("path completion entry: {error}")))?;
            let name = entry.file_name();
            let is_directory = entry
                .file_type()
                .map_err(|error| IntelligenceError(error.to_string()))?
                .is_dir();
            let mut value = directory_text.as_bytes().to_vec();
            if !value.ends_with(b"/") && !value.is_empty() {
                value.push(b'/');
            }
            use std::os::unix::ffi::OsStrExt;
            value.extend_from_slice(name.as_bytes());
            if is_directory {
                value.push(b'/');
            }
            candidates.push(CompletionCandidate {
                text: String::from_utf8_lossy(&value).into_owned(),
                bytes: value,
                kind: if is_directory { CompletionKind::Directory } else { CompletionKind::File },
            });
        }
        Ok(CompletionResult {
            replace_range: range,
            candidates,
            auto_insert: false,
        })
    }
}

pub fn path_context(
    text: &crate::byte_buffer::ByteBuffer,
    caret: usize,
) -> Result<Option<(Range<usize>, String)>, IntelligenceError> {
    if caret > text.len() { return Err(IntelligenceError("The caret is outside the document".into())); }
    let raw = text.slice(caret.saturating_sub(260)..caret).map_err(|error| IntelligenceError(error.to_string()))?.to_vec();
    let raw = &raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())];
    let input = String::from_utf8_lossy(raw);
    let start = input.char_indices().rev().find_map(|(at, ch)| {
        if ch != '/' { return None; }
        if at == 0 || input[..at].chars().next_back().is_some_and(|ch| ch.is_ascii_whitespace() || "\"'(".contains(ch)) {
            Some(at)
        } else { None }
    });
    Ok(start.map(|at| {
        let path = input[at..].to_owned();
        (caret.saturating_sub(path.encode_utf16().count())..caret, path)
    }))
}
