use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::editor::ByteOffset;
use crate::files::DiskFile;
use crate::search::{LiteralOptions, literal_ranges};

pub struct FileSearchRequest {
    pub mode: crate::search::SearchMode,
    pub directory: PathBuf,
    pub filters: String,
    pub query: String,
    pub match_case: bool,
    pub whole_word: bool,
    pub recursive: bool,
    pub hidden_directories: bool,
    /// Canonical paths and committed text, captured on the UI thread.
    pub open_documents: BTreeMap<PathBuf, Arc<str>>,
}

#[derive(Clone, Debug)]
pub struct FileMatch {
    pub path: PathBuf,
    pub range: Range<ByteOffset>,
    pub line: usize,
    pub column: usize,
    pub preview: String,
    pub searched_text: Arc<str>,
}

#[derive(Default)]
pub struct FileSearchResults {
    pub matches: Vec<FileMatch>,
    pub errors: Vec<String>,
    pub files_searched: usize,
    pub cancelled: bool,
}

fn wildcard(name: &str, pattern: &str) -> bool {
    pattern.split(';').any(|pattern| {
        if pattern == "*.*" {
            return true;
        }
        let name: Vec<_> = name.to_ascii_lowercase().encode_utf16().collect();
        let pattern: Vec<_> = pattern.to_ascii_lowercase().encode_utf16().collect();
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

fn supported_name(name: &str) -> bool {
    name.chars()
        .all(|ch| ch.is_ascii() || (!ch.is_lowercase() && !ch.is_uppercase()))
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

fn excluded_directory(name: &str, patterns: &[&str], level: usize) -> bool {
    patterns.iter().any(|pattern| {
        if let Some(pattern) = pattern.strip_prefix("!+\\") {
            wildcard(name, pattern)
        } else if let Some(pattern) = pattern.strip_prefix("!\\") {
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
        } // UF_HIDDEN
    }
    #[cfg(not(target_os = "macos"))]
    let _ = metadata;
    name.starts_with('.')
}

pub fn search_files(request: FileSearchRequest, cancel: &AtomicBool) -> FileSearchResults {
    let mut result = FileSearchResults::default();
    if request.query.is_empty() {
        return result;
    }
    let mut patterns: Vec<_> = request.filters.split_ascii_whitespace().collect();
    if patterns.iter().all(|pattern| pattern.starts_with('!')) {
        patterns.insert(0, "*.*");
    }
    if patterns.iter().any(|pattern| !supported_name(pattern)) {
        result
            .errors
            .push("Unicode case-folding in file filters is not implemented yet".into());
        return result;
    }
    let root = match request.directory.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            result
                .errors
                .push(format!("{}: {error}", request.directory.display()));
            return result;
        }
    };
    let mut pending = vec![(root, 1)];
    let mut visited = BTreeSet::new();
    let mut candidates = Vec::new();
    while let Some((directory, level)) = pending.pop() {
        if !visited.insert(directory.clone()) {
            result.errors.push(format!(
                "{}: repeated directory/link traversal skipped",
                directory.display()
            ));
            continue;
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                result
                    .errors
                    .push(format!("{}: {error}", directory.display()));
                continue;
            }
        };
        for entry in entries {
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
            if let Err(error) = collect_entry(
                &path,
                level,
                &patterns,
                &request,
                &mut pending,
                &mut candidates,
            ) {
                result.errors.push(format!("{}: {error}", path.display()));
            }
        }
    }
    let mut searched = search_file_list(request, candidates, cancel);
    result.errors.append(&mut searched.errors);
    searched.errors = result.errors;
    searched
}

pub fn search_file_list(
    request: FileSearchRequest,
    candidates: Vec<PathBuf>,
    cancel: &AtomicBool,
) -> FileSearchResults {
    let mut result = FileSearchResults {
        files_searched: candidates.len(),
        ..Default::default()
    };
    for path in candidates {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        if let Err(error) = search_file(&path, &request, &mut result) {
            result.errors.push(format!("{}: {error}", path.display()));
        }
    }
    result
}

fn collect_entry(
    path: &Path,
    level: usize,
    patterns: &[&str],
    request: &FileSearchRequest,
    pending: &mut Vec<(PathBuf, usize)>,
    candidates: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("non-Unicode filename is not supported yet")?;
    if !supported_name(name) {
        return Err("Unicode filename case-folding is not implemented yet".into());
    }
    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        if request.recursive
            && (request.hidden_directories || !hidden_directory(name, &metadata))
            && !excluded_directory(name, patterns, level)
        {
            pending.push((path.canonicalize()?, level + 1));
        }
        return Ok(());
    }
    if !metadata.is_file() || !file_matches(name, patterns) {
        return Ok(());
    }
    candidates.push(path.to_path_buf());
    Ok(())
}

fn search_file(
    path: &Path,
    request: &FileSearchRequest,
    result: &mut FileSearchResults,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    let text: Arc<str> = match request.open_documents.get(&path) {
        Some(text) => text.clone(),
        None => Arc::from(DiskFile::load_unicode(&path)?.1),
    };
    let matches = literal_ranges(
        &text,
        &request.query,
        0,
        LiteralOptions {
            mode: request.mode,
            match_case: request.match_case,
            wrap: false,
            whole_word: request.whole_word,
        },
    )?;
    // Rope's CRLF/CR/LF metrics agree with the document model's line numbering.
    let lines = ropey::Rope::from_str(&text);
    for range in matches {
        let line = lines.byte_to_line_idx(range.start.0, ropey::LineType::LF_CR);
        let line_start = lines.line_to_byte_idx(line, ropey::LineType::LF_CR);
        let preview = lines.line(line, ropey::LineType::LF_CR).to_string();
        result.matches.push(FileMatch {
            path: path.clone(),
            column: text[line_start..range.start.0].chars().count() + 1,
            line: line + 1,
            preview: preview
                .trim_end_matches(['\r', '\n'])
                .chars()
                .take(240)
                .collect(),
            range,
            searched_text: text.clone(),
        });
    }
    Ok(())
}
