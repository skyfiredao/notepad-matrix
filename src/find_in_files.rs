use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
};

use crate::editor::ByteOffset;
use crate::byte_buffer::ByteBuffer;
use crate::files::DiskFile;
use crate::search::LiteralOptions;

#[derive(Clone)]
pub struct FileSearchRequest {
    pub mode: crate::search::SearchMode,
    pub directory: PathBuf,
    pub filters: String,
    pub query: String,
    pub match_case: bool,
    pub whole_word: bool,
    pub recursive: bool,
    pub hidden_directories: bool,
    pub open_documents: BTreeMap<PathBuf, ByteBuffer>,
}

pub(crate) struct OpenSearchDocument {
    pub paths: Vec<PathBuf>,
    pub bytes: ByteBuffer,
    pub regex_state: crate::search::RegexState,
}

#[derive(Clone, Debug)]
pub struct FileMatch {
    pub file_index: usize,
    pub path: PathBuf,
    pub range: Range<ByteOffset>,
    pub line: usize,
    pub column: usize,
    pub preview: String,
    pub searched_bytes: ByteBuffer,
}

#[derive(Default)]
pub struct FileSearchResults {
    pub matches: Vec<FileMatch>,
    pub errors: Vec<String>,
    pub files_searched: usize,
    pub cancelled: bool,
    pub search_error: Option<String>,
}

#[derive(Default)]
pub struct FileReplaceResults {
    pub replaced: usize,
    pub errors: Vec<String>,
    pub cancelled: bool,
    pub search_error: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct FileProgressSnapshot {
    pub path: String,
    pub hits: usize,
    pub completed: usize,
    pub total: usize,
    pub discovering: bool,
    pub replacing: bool,
}

pub struct FileSearchProgress {
    state: std::sync::Mutex<FileProgressSnapshot>,
}

impl FileSearchProgress {
    pub fn new(replacing: bool, discovering: bool) -> Self {
        Self { state: std::sync::Mutex::new(FileProgressSnapshot { replacing, discovering, ..Default::default() }) }
    }

    pub fn snapshot(&self) -> FileProgressSnapshot {
        self.state.lock().unwrap_or_else(|error| error.into_inner()).clone()
    }

    pub fn record(&self, path: &Path, completed: usize, total: usize, hits: usize, discovering: bool) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.path = path.display().to_string();
        state.completed = completed;
        state.total = total;
        state.hits = hits;
        state.discovering = discovering;
    }
}

#[derive(Default)]
pub struct FileCandidates {
    pub paths: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

pub fn replace_file_list(
    request: &FileSearchRequest,
    paths: Vec<PathBuf>,
    replacement: &str,
    cancel: &AtomicBool,
) -> FileReplaceResults {
    let mut output = FileReplaceResults::default();
    let options = LiteralOptions { mode: request.mode, match_case: request.match_case, whole_word: request.whole_word, wrap: true };
    for path in paths {
        if cancel.load(Ordering::Relaxed) { output.cancelled = true; break; }
        let (file, bytes) = match DiskFile::load_byte_buffer(&path) {
            Ok(loaded) => loaded,
            Err(error) => { output.errors.push(format!("{}: {error}", path.display())); continue; }
        };
        if file.is_read_only() {
            output.errors.push(format!("{}: 文件为只读，未替换", path.display()));
            continue;
        }
        if bytes.is_empty() { continue; }
        let mut editor = crate::input::EditorSession::new("");
        if let Err(error) = editor.replace_selection_bytes(&bytes.to_vec()) {
            output.errors.push(format!("{}: {error}", path.display()));
            continue;
        }
        match editor.replace_all_literal(&request.query, replacement, options) {
            Ok(count) => {
                output.replaced += count;
                if count > 0 && let Err(error) = file.save_unicode(&editor.save_snapshot()) {
                    output.errors.push(format!("{}: {error}", path.display()));
                }
            }
            Err(crate::search::ReplaceError::Search(error)) => {
                output.search_error = Some(error.to_string());
                break;
            }
            Err(error) => output.errors.push(format!("{}: {error}", path.display())),
        }
    }
    output
}

fn wildcard(name: &str, pattern: &str) -> bool {
    pattern.split(';').any(|pattern| {
        if pattern == "*.*" {
            return true;
        }
        let name: Vec<_> = name.to_uppercase().encode_utf16().collect();
        let pattern: Vec<_> = pattern.to_uppercase().encode_utf16().collect();
        let (mut n, mut p, mut star, mut retry) = (0, 0, None, 0);
        while n < name.len() {
            if p < pattern.len() && (pattern[p] == u16::from(b'?') || pattern[p] == name[n]) {
                n += 1;
                p += 1;
            } else if p < pattern.len() && pattern[p] == u16::from(b'*') {
                star = Some(p);
                p += 1;
                retry = n;
            } else if let Some(position) = star {
                retry += 1;
                n = retry;
                p = position + 1;
            } else {
                return false;
            }
        }
        while p < pattern.len() && pattern[p] == u16::from(b'*') {
            p += 1;
        }
        p == pattern.len()
    })
}

fn filter_patterns(filters: &str) -> Vec<&str> {
    let mut patterns: Vec<_> = filters.split('\0').next().unwrap_or_default()
        .split_ascii_whitespace().collect();
    if patterns.iter().all(|pattern| pattern.starts_with('!')) {
        patterns.insert(0, "*.*");
    }
    patterns
}

fn file_matches(name: &str, patterns: &[&str]) -> bool {
    let mut included = false;
    for pattern in patterns {
        if let Some(excluded) = pattern.strip_prefix('!').filter(|text| !text.is_empty()) {
            if wildcard(name, excluded) {
                return false;
            }
        } else if wildcard(name, pattern) {
            included = true;
        }
    }
    included
}

pub fn matches_filters(path: &Path, filters: &str) -> bool {
    let patterns = filter_patterns(filters);
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| file_matches(name, &patterns))
}

fn excluded_directory(name: &str, patterns: &[&str], level: usize) -> bool {
    patterns.iter().any(|pattern| {
        if let Some(pattern) = pattern.strip_prefix("!+\\").filter(|pattern| !pattern.is_empty()) {
            wildcard(name, pattern)
        } else if let Some(pattern) = pattern.strip_prefix("!\\").filter(|pattern| !pattern.is_empty()) {
            level == 1 && wildcard(name, pattern)
        } else {
            false
        }
    })
}

fn hidden_directory(name: &str, metadata: &std::fs::Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        if metadata.st_flags() & 0x8000 != 0 {
            return true;
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = metadata;
    name.starts_with('.')
}

pub fn search_files(request: FileSearchRequest, cancel: &AtomicBool) -> FileSearchResults {
    let candidates = discover_files(&request, cancel, None);
    if candidates.cancelled { return FileSearchResults { cancelled: true, errors: candidates.errors, ..Default::default() }; }
    let mut result = search_file_list(request, candidates.paths, cancel);
    result.errors.extend(candidates.errors);
    result
}

pub fn discover_files(request: &FileSearchRequest, cancel: &AtomicBool, progress: Option<&FileSearchProgress>) -> FileCandidates {
    let mut result = FileCandidates::default();
    let patterns = filter_patterns(&request.filters);
    let root = match request.directory.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            result
                .errors
                .push(format!("{}: {error}", request.directory.display()));
            return result;
        }
    };
    discover_directory(&root, 1, &patterns, request, cancel, progress, &mut result);
    result.cancelled |= cancel.load(Ordering::Relaxed);
    result
}

fn discover_directory(directory: &Path, level: usize, patterns: &[&str], request: &FileSearchRequest, cancel: &AtomicBool, progress: Option<&FileSearchProgress>, result: &mut FileCandidates) {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            return;
        }
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) => {
                result
                    .errors
                    .push(format!("{}: {error}", directory.display()));
                return;
            }
        };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                result.cancelled = true;
                return;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    result
                        .errors
                        .push(format!("{}: {error}", directory.display()));
                    continue;
                }
            };
            let path = entry.path();
            match collect_entry(&path, level, patterns, request, &mut result.paths) {
                Ok(true) => discover_directory(&path, level + 1, patterns, request, cancel, progress, result),
                Ok(false) => {},
                Err(error) => result.errors.push(format!("{}: {error}", path.display())),
            }
            if let Some(progress) = progress { progress.record(directory, 0, 0, result.paths.len(), true); }
        }
}

pub fn search_file_list(
    request: FileSearchRequest,
    candidates: Vec<PathBuf>,
    cancel: &AtomicBool,
) -> FileSearchResults {
    search_file_list_with_progress(request, candidates, cancel, None)
}

pub fn search_file_list_with_progress(request: FileSearchRequest, candidates: Vec<PathBuf>, cancel: &AtomicBool, progress: Option<&FileSearchProgress>) -> FileSearchResults {
    search_file_list_with_state(request, candidates, cancel, progress, &mut [], &mut Vec::new())
}

pub(crate) fn search_file_list_with_state(
    request: FileSearchRequest,
    candidates: Vec<PathBuf>,
    cancel: &AtomicBool,
    progress: Option<&FileSearchProgress>,
    opened: &mut [OpenSearchDocument],
    updates: &mut Vec<(usize, crate::search::RegexState)>,
) -> FileSearchResults {
    let mut result = FileSearchResults {
        files_searched: candidates.len(),
        ..Default::default()
    };
    let total = candidates.len();
    for (index, path) in candidates.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        if let Err(error) = search_file(&path, index, &request, &mut result, opened, updates) {
            if let Some(error) = error.downcast_ref::<crate::search::SearchError>() {
                result.search_error = Some(error.to_string());
                break;
            }
            result.errors.push(format!("{}: {error}", path.display()));
        }
        if let Some(progress) = progress { progress.record(&path, index + 1, total, result.matches.len(), false); }
    }
    result
}

fn collect_entry(
    path: &Path,
    level: usize,
    patterns: &[&str],
    request: &FileSearchRequest,
    candidates: &mut Vec<PathBuf>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("non-Unicode filename is not supported yet")?;
    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        return Ok(request.recursive
            && (request.hidden_directories || !hidden_directory(name, &metadata))
            && !excluded_directory(name, patterns, level));
    }
    if !metadata.is_file() || !file_matches(name, patterns) {
        return Ok(false);
    }
    candidates.push(path.to_path_buf());
    Ok(false)
}

fn search_file(
    path: &Path,
    file_index: usize,
    request: &FileSearchRequest,
    result: &mut FileSearchResults,
    opened: &mut [OpenSearchDocument],
    updates: &mut Vec<(usize, crate::search::RegexState)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let canonical = path.canonicalize().ok();
    let identity = opened.iter().position(|doc| doc.paths.iter().any(|candidate| candidate == path
        || canonical.as_ref().is_some_and(|path| candidate == path)));
    let bytes = if let Some(identity) = identity {
        opened[identity].bytes.clone()
    } else if let Some(bytes) = request.open_documents.get(path) {
        bytes.clone()
    } else {
        let canonical = path.canonicalize()?;
        match request.open_documents.get(&canonical) {
            Some(bytes) => bytes.clone(),
            None => DiskFile::load_byte_buffer(path)?.1,
        }
    };
    if bytes.is_empty() { return Ok(()); }
    let mut fresh = crate::search::RegexState::default();
    let state = match identity {
        Some(identity) => &mut opened[identity].regex_state,
        None => &mut fresh,
    };
    let matches = crate::search::literal_ranges_in_bytes_with_state(
        &bytes,
        &request.query,
        0..bytes.len(),
        LiteralOptions {
            mode: request.mode,
            match_case: request.match_case,
            wrap: false,
            whole_word: request.whole_word,
        },
        state,
    );
    if matches!(request.mode, crate::search::SearchMode::Regex { .. }) && let Some(identity) = identity {
        updates.push((identity, state.clone()));
    }
    let matches = matches?;
    for range in matches {
        let line = bytes.line_index(range.start.0)?;
        let line_start = bytes.line_start(line)?;
        let line_end = if line + 1 < bytes.line_count() { bytes.line_start(line + 1)? } else { bytes.len() };
        let preview = crate::byte_projection::ByteProjection::from_bytes(&bytes.slice(line_start..line_end)?).text().to_string();
        result.matches.push(FileMatch {
            file_index,
            path: path.to_path_buf(),
            column: bytes.count_characters(line_start..range.start.0, usize::MAX)? + 1,
            line: line + 1,
            preview: preview
                .trim_end_matches(['\r', '\n'])
                .chars()
                .take(240)
                .collect(),
            range,
            searched_bytes: bytes.clone(),
        });
    }
    Ok(())
}
