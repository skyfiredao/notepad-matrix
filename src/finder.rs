use std::ops::Range;
use std::sync::Arc;

use crate::editor::{ByteOffset, Selection};
use crate::search::{LiteralOptions, SearchError};

pub struct Document {
    pub name: String,
    pub bytes: crate::byte_buffer::ByteBuffer,
    pub selection: Option<Selection>,
}

pub(crate) struct SearchDocument {
    pub document: Document,
    pub regex_state: crate::search::RegexState,
    pub identity: Option<usize>,
}

impl From<Document> for SearchDocument {
    fn from(document: Document) -> Self {
        Self { document, regex_state: Default::default(), identity: None }
    }
}

pub struct FoundLine {
    pub number: usize,
    pub caption: String,
    pub highlights: Vec<Range<usize>>,
    pub ranges: Vec<Range<ByteOffset>>,
}

fn copy_result_lines<'a>(lines: impl Iterator<Item = &'a FoundLine>) -> String {
    let mut text = String::new();
    let mut previous = None;
    for line in lines {
        let caption = line.caption.split('\0').next().unwrap_or_default();
        if previous == Some(caption) {
            continue;
        }
        previous = Some(caption);
        if let Some((_, content)) = caption.split_once(": ") {
            text.push_str(&content.replace(['\r', '\n'], ""));
            text.push_str("\r\n");
        }
    }
    if text.is_empty() {
        text.push_str("\r\n");
    }
    text
}

pub struct FoundDocument {
    pub name: String,
    pub lines: Vec<FoundLine>,
    pub hits: usize,
    pub collapsed: bool,
}

pub struct Search {
    pub title: String,
    pub documents: Vec<FoundDocument>,
    pub hits: usize,
    pub searched: usize,
    pub errors: Vec<String>,
    pub search_error: Option<String>,
    pub collapsed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Search(usize),
    Document(usize, usize),
    Line(usize, usize, usize),
    Error(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPoint {
    pub row: Option<Row>,
    pub byte: usize,
}

impl TextPoint {
    fn key(self) -> (usize, usize, usize, usize) {
        match self.row {
            Some(Row::Search(search)) => (search, 0, 0, self.byte),
            Some(Row::Document(search, doc)) => (search, doc + 1, 0, self.byte),
            Some(Row::Line(search, doc, line)) => (search, doc + 1, line + 1, self.byte),
            Some(Row::Error(search, error)) => (search, usize::MAX, error, self.byte),
            None => (usize::MAX, 0, 0, self.byte),
        }
    }

    pub fn between(self, anchor: Self, head: Self) -> bool {
        let first = anchor.key();
        let second = head.key();
        first != second && first.min(second) <= self.key() && self.key() <= first.max(second)
    }
}

#[derive(Default)]
pub struct Results {
    pub searches: Vec<Search>,
    pub rows: Vec<Row>,
}

impl Results {
    pub fn source_pathnames(&self) -> Vec<&str> {
        let mut seen = std::collections::BTreeSet::new();
        self.searches
            .iter()
            .flat_map(|search| search.documents.iter())
            .filter(|doc| {
                !doc.lines.is_empty() && !doc.name.is_empty() && seen.insert(doc.name.as_str())
            })
            .map(|doc| doc.name.as_str())
            .collect()
    }

    pub fn search_within(
        &self,
        query: &str,
        documents: Vec<Document>,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
    ) -> Result<Search, SearchError> {
        self.line_filter().search(
            query,
            documents,
            options,
            match_line_numbers,
            one_entry_per_line,
        )
    }

    pub fn line_filter(&self) -> ResultLineFilter {
        let mut paths = Vec::new();
        let mut lines: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for doc in self
            .searches
            .iter()
            .flat_map(|search| search.documents.iter())
        {
            if doc.name.is_empty() || doc.lines.is_empty() {
                continue;
            }
            if !lines.contains_key(&doc.name) {
                paths.push(doc.name.clone());
            }
            lines
                .entry(doc.name.clone())
                .or_default()
                .extend(doc.lines.iter().map(|line| line.number));
        }
        ResultLineFilter { paths, lines }
    }
}

pub struct ResultLineFilter {
    paths: Vec<String>,
    lines: std::collections::BTreeMap<String, Vec<usize>>,
}

pub struct ResultSearchProgress {
    cancel: Arc<std::sync::atomic::AtomicBool>,
    completed: std::sync::atomic::AtomicUsize,
    hits: std::sync::atomic::AtomicUsize,
}

impl ResultSearchProgress {
    pub fn new(cancel: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            cancel,
            completed: std::sync::atomic::AtomicUsize::new(0),
            hits: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    pub fn snapshot(&self) -> (usize, usize) {
        let completed = self.completed.load(std::sync::atomic::Ordering::Acquire);
        (
            completed,
            self.hits.load(std::sync::atomic::Ordering::Relaxed),
        )
    }
    pub fn percent(&self, total: usize) -> usize {
        let completed = self.completed.load(std::sync::atomic::Ordering::Acquire);
        if completed == 0 || total == 0 {
            0
        } else {
            (completed - 1) * 100 / total
        }
    }

    pub fn cancel(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn record(&self, completed: usize, hits: usize) {
        self.hits.store(hits, std::sync::atomic::Ordering::Relaxed);
        self.completed
            .store(completed, std::sync::atomic::Ordering::Release);
    }
}

struct ResultSearchControl<'a> {
    cancel: &'a std::sync::atomic::AtomicBool,
    progress: Option<&'a ResultSearchProgress>,
}

impl ResultLineFilter {
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    pub fn search(
        &self,
        query: &str,
        documents: Vec<Document>,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
    ) -> Result<Search, SearchError> {
        let mut documents: std::collections::BTreeMap<_, _> = documents
            .into_iter()
            .map(|doc| (doc.name.clone(), doc))
            .collect();
        self.search_loaded(
            query,
            options,
            match_line_numbers,
            one_entry_per_line,
            &std::sync::atomic::AtomicBool::new(false),
            |path| documents.remove(path),
        )
    }

    pub fn search_loaded(
        &self,
        query: &str,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
        cancel: &std::sync::atomic::AtomicBool,
        load: impl FnMut(&str) -> Option<Document>,
    ) -> Result<Search, SearchError> {
        self.search_controlled(
            query,
            options,
            match_line_numbers,
            one_entry_per_line,
            ResultSearchControl {
                cancel,
                progress: None,
            },
            load,
        )
    }

    pub fn search_with_progress(
        &self,
        query: &str,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
        progress: &ResultSearchProgress,
        load: impl FnMut(&str) -> Option<Document>,
    ) -> Result<Search, SearchError> {
        self.search_controlled(
            query,
            options,
            match_line_numbers,
            one_entry_per_line,
            ResultSearchControl {
                cancel: &progress.cancel,
                progress: Some(progress),
            },
            load,
        )
    }

    pub(crate) fn search_with_progress_and_state(
        &self,
        query: &str,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
        progress: &ResultSearchProgress,
        load: impl FnMut(&str) -> Option<SearchDocument>,
        updates: &mut Vec<(usize, crate::search::RegexState)>,
    ) -> Result<Search, SearchError> {
        self.search_controlled_with_state(query, options, match_line_numbers, one_entry_per_line,
            ResultSearchControl { cancel: &progress.cancel, progress: Some(progress) }, load, updates)
    }

    fn search_controlled(
        &self,
        query: &str,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
        control: ResultSearchControl<'_>,
        mut load: impl FnMut(&str) -> Option<Document>,
    ) -> Result<Search, SearchError> {
        self.search_controlled_with_state(query, options, match_line_numbers, one_entry_per_line,
            control, |path| load(path).map(SearchDocument::from), &mut Vec::new())
    }

    fn search_controlled_with_state(
        &self,
        query: &str,
        options: LiteralOptions,
        match_line_numbers: bool,
        one_entry_per_line: bool,
        control: ResultSearchControl<'_>,
        mut load: impl FnMut(&str) -> Option<SearchDocument>,
        updates: &mut Vec<(usize, crate::search::RegexState)>,
    ) -> Result<Search, SearchError> {
        let paths = &self.paths;
        let mut found = Vec::new();
        let mut hits = 0;
        for (index, path) in paths.iter().enumerate() {
            if control.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let Some(mut source) = load(path) else {
                if let Some(progress) = control.progress {
                    progress.record(index + 1, hits);
                }
                continue;
            };
            if let Some(identity) = source.identity
                && let Some((_, state)) = updates.iter().rev().find(|(id, _)| *id == identity)
            {
                source.regex_state = state.clone();
            }
            let mut doc = source.document;
            doc.selection = None;
            if doc.bytes.is_empty() {
                if let Some(progress) = control.progress { progress.record(index + 1, hits); }
                continue;
            }
            let matches = crate::search::literal_ranges_in_bytes_without_empty_with_state(
                &doc.bytes, query, 0..doc.bytes.len(), options, &mut source.regex_state);
            if matches!(options.mode, crate::search::SearchMode::Regex { .. }) && let Some(identity) = source.identity {
                updates.push((identity, source.regex_state));
            }
            let mut matches = matches?;
            if match_line_numbers && !matches.is_empty() {
                let source_lines = self.lines.get(path).map_or(&[][..], Vec::as_slice);
                let mut from = 0;
                matches.retain(|range| {
                    let Ok(line) = doc.bytes.line_index(range.start.0).map(|line| line + 1) else { return false; };
                    if let Some(index) = source_lines[from..]
                        .iter()
                        .position(|&source| source == line)
                    {
                        from += index;
                        true
                    } else {
                        false
                    }
                });
            }
            if !matches.is_empty() {
                hits += matches.len();
                found.push(found_document(doc, matches, one_entry_per_line));
            }
            if let Some(progress) = control.progress {
                progress.record(index + 1, hits);
            }
        }
        let mut result = finish_search(query, found, paths.len(), false, options, Vec::new());
        if match_line_numbers {
            result
                .title
                .push_str(" - Line Filter Mode: only display the filtered results");
        }
        Ok(result)
    }
}

impl Results {
    pub fn fold_all(&mut self, collapsed: bool) {
        for search in &mut self.searches {
            search.collapsed = collapsed;
            for doc in &mut search.documents {
                doc.collapsed = collapsed;
            }
        }
        self.rebuild();
    }

    pub fn clear(&mut self) {
        self.searches.clear();
        self.rows.clear();
    }
    pub fn selected_bytes(
        &self,
        row: Row,
        anchor: TextPoint,
        head: TextPoint,
    ) -> Option<Range<usize>> {
        let (start, end) = if anchor.key() <= head.key() {
            (anchor, head)
        } else {
            (head, anchor)
        };
        let origin = TextPoint {
            row: Some(row),
            byte: 0,
        };
        let caption = self.caption_row(row)?;
        let from = if start.row == Some(row) {
            start.byte
        } else if origin.key() < start.key() {
            return None;
        } else {
            0
        };
        let to = if end.row == Some(row) {
            end.byte
        } else if origin.key() > end.key() {
            return None;
        } else {
            caption.len()
        };
        (from < to
            && to <= caption.len()
            && caption.is_char_boundary(from)
            && caption.is_char_boundary(to))
        .then_some(from..to)
    }
    fn point_index(&self, point: TextPoint, rows: &[Row]) -> Option<(usize, usize)> {
        let Some(row) = point.row else {
            return (point.byte == 0).then_some((rows.len(), 0));
        };
        let index = rows.iter().position(|candidate| *candidate == row)?;
        let caption = self.caption_row(row)?;
        (point.byte <= caption.len() && caption.is_char_boundary(point.byte))
            .then_some((index, point.byte))
    }

    pub fn selection_rows(&self, anchor: TextPoint, head: TextPoint) -> Option<(Row, Row)> {
        if anchor.row == head.row {
            let row = anchor.row?;
            let caption = self.caption_row(row)?;
            caption.get(anchor.byte.min(head.byte)..anchor.byte.max(head.byte))?;
            return Some((row, row));
        }
        let rows = self.all_rows();
        let first = self.point_index(anchor, &rows)?;
        let second = self.point_index(head, &rows)?;
        let (start, mut end) = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        if start.0 < end.0 && end.1 == 0 {
            end.0 -= 1;
        }
        Some((*rows.get(start.0)?, *rows.get(end.0)?))
    }

    pub fn copy_verbatim(&self, anchor: TextPoint, head: TextPoint) -> Option<String> {
        if anchor == head {
            return None;
        }
        if anchor.row == head.row {
            let caption = self.caption_row(anchor.row?)?;
            return Some(
                caption
                    .get(anchor.byte.min(head.byte)..anchor.byte.max(head.byte))?
                    .to_owned(),
            );
        }
        let rows = self.all_rows();
        let first = self.point_index(anchor, &rows)?;
        let second = self.point_index(head, &rows)?;
        if first == second {
            return None;
        }
        let (start, end) = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        let mut text = String::new();
        for (index, row) in rows
            .iter()
            .enumerate()
            .take(end.0.saturating_add(1))
            .skip(start.0)
        {
            let caption = self.caption_row(*row)?;
            let from = if index == start.0 { start.1 } else { 0 };
            let to = if index == end.0 { end.1 } else { caption.len() };
            text.push_str(caption.get(from..to)?);
            if index < end.0 {
                text.push_str("\r\n");
            }
        }
        Some(text)
    }

    fn all_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (search_index, search) in self.searches.iter().enumerate() {
            rows.push(Row::Search(search_index));
            for (doc_index, doc) in search.documents.iter().enumerate() {
                rows.push(Row::Document(search_index, doc_index));
                rows.extend(
                    (0..doc.lines.len()).map(|line| Row::Line(search_index, doc_index, line)),
                );
            }
            rows.extend((0..search.errors.len()).map(|error| Row::Error(search_index, error)));
        }
        rows
    }

    pub fn copy_lines(&self, anchor: Row, head: Row) -> Option<String> {
        if anchor == head {
            return match anchor {
                Row::Search(search) => {
                    let search = self.searches.get(search)?;
                    search.documents.last()?.lines.last()?;
                    Some(copy_result_lines(
                        search.documents.iter().flat_map(|doc| doc.lines.iter()),
                    ))
                }
                Row::Document(search, doc) => {
                    let doc = self.searches.get(search)?.documents.get(doc)?;
                    doc.lines.last()?;
                    Some(copy_result_lines(doc.lines.iter()))
                }
                Row::Line(search, doc, line) => {
                    let line = self
                        .searches
                        .get(search)?
                        .documents
                        .get(doc)?
                        .lines
                        .get(line)?;
                    Some(copy_result_lines(std::iter::once(line)))
                }
                Row::Error(..) => None,
            };
        }
        let rows = self.all_rows();
        let first = rows.iter().position(|row| *row == anchor)?;
        let second = rows.iter().position(|row| *row == head)?;
        let start = first.min(second);
        let end = first.max(second);
        Some(copy_result_lines(rows[start..=end].iter().filter_map(
            |row| {
                if let Row::Line(search, doc, line) = *row {
                    Some(&self.searches[search].documents[doc].lines[line])
                } else {
                    None
                }
            },
        )))
    }

    pub fn copy_pathnames(&self, anchor: Row, head: Row) -> Option<String> {
        let paths = self.selected_pathnames(anchor, head)?;
        let mut text = String::new();
        for path in paths {
            text.push_str(path);
            text.push_str("\r\n");
        }
        Some(text)
    }

    pub fn selected_pathnames(&self, anchor: Row, head: Row) -> Option<Vec<&str>> {
        if anchor == head {
            let (search, doc) = match anchor {
                Row::Document(search, doc) | Row::Line(search, doc, _) => (search, doc),
                _ => return None,
            };
            let doc = self.searches.get(search)?.documents.get(doc)?;
            if let Row::Line(_, _, line) = anchor {
                doc.lines.get(line)?;
            } else {
                doc.lines.first()?;
            }
            return (!doc.name.is_empty()).then(|| vec![doc.name.as_str()]);
        }
        let rows = self.all_rows();
        let first = rows.iter().position(|row| *row == anchor)?;
        let second = rows.iter().position(|row| *row == head)?;
        let start = first.min(second);
        let mut end = first.max(second);
        if matches!(rows[end], Row::Document(..)) && end + 1 < rows.len() {
            end += 1;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut paths = Vec::new();
        for &row in &rows[start..=end] {
            if let Row::Line(search, doc, _) = row {
                let path = self.searches[search].documents[doc].name.as_str();
                if !path.is_empty() && seen.insert(path) {
                    paths.push(path);
                }
            }
        }
        (!paths.is_empty()).then_some(paths)
    }

    pub fn caption(&self, index: usize) -> Option<std::borrow::Cow<'_, str>> {
        self.caption_row(*self.rows.get(index)?)
    }

    pub fn caption_row(&self, row: Row) -> Option<std::borrow::Cow<'_, str>> {
        use std::borrow::Cow;
        Some(match row {
            Row::Search(search) => Cow::Borrowed(&self.searches.get(search)?.title),
            Row::Document(search, doc) => {
                let doc = self.searches.get(search)?.documents.get(doc)?;
                Cow::Owned(format!(
                    "  {} ({} {})",
                    doc.name,
                    commafy(doc.hits),
                    if doc.hits == 1 { "hit" } else { "hits" }
                ))
            }
            Row::Line(search, doc, line) => Cow::Borrowed(
                &self
                    .searches
                    .get(search)?
                    .documents
                    .get(doc)?
                    .lines
                    .get(line)?
                    .caption,
            ),
            Row::Error(search, error) => Cow::Owned(format!(
                "Error: {}",
                self.searches.get(search)?.errors.get(error)?
            )),
        })
    }

    pub fn begin(&mut self, purge: bool) {
        if purge {
            self.searches.clear();
        }
        for previous in &mut self.searches {
            previous.collapsed = true;
        }
        self.rebuild();
    }

    pub fn push(&mut self, search: Search, purge: bool) {
        self.begin(purge);
        self.searches.insert(0, search);
        self.rebuild();
    }

    pub fn toggle(&mut self, index: usize) {
        if let Some(row) = self.rows.get(index).copied() {
            self.toggle_row(row);
        }
    }

    pub fn toggle_row(&mut self, row: Row) {
        match row {
            Row::Search(index) => {
                let Some(search) = self.searches.get_mut(index) else {
                    return;
                };
                search.collapsed = !search.collapsed;
            }
            Row::Document(search, index) => {
                let Some(doc) = self
                    .searches
                    .get_mut(search)
                    .and_then(|search| search.documents.get_mut(index))
                else {
                    return;
                };
                doc.collapsed = !doc.collapsed;
            }
            _ => return,
        }
        self.rebuild();
    }

    pub fn delete(&mut self, index: usize) -> bool {
        self.rows
            .get(index)
            .copied()
            .is_some_and(|row| self.delete_caret(row).is_some())
    }

    fn row_after_documents(&self, search: usize) -> Option<Row> {
        if !self.searches.get(search)?.errors.is_empty() {
            Some(Row::Error(search, 0))
        } else {
            self.searches
                .get(search + 1)
                .map(|_| Row::Search(search + 1))
        }
    }

    pub fn delete_caret(&mut self, row: Row) -> Option<TextPoint> {
        let next = match row {
            Row::Search(search) => {
                if search >= self.searches.len() {
                    return None;
                }
                self.searches.remove(search);
                self.searches.get(search).map(|_| Row::Search(search))
            }
            Row::Document(search, doc) => {
                let documents = &mut self.searches.get_mut(search)?.documents;
                if doc >= documents.len() {
                    return None;
                }
                documents.remove(doc);
                if doc < documents.len() {
                    Some(Row::Document(search, doc))
                } else {
                    self.row_after_documents(search)
                }
            }
            Row::Line(search, doc, line) => {
                let lines = &mut self.searches.get_mut(search)?.documents.get_mut(doc)?.lines;
                if line >= lines.len() {
                    return None;
                }
                lines.remove(line);
                if line < lines.len() {
                    Some(Row::Line(search, doc, line))
                } else if self.searches[search].documents.get(doc + 1).is_some() {
                    Some(Row::Document(search, doc + 1))
                } else {
                    self.row_after_documents(search)
                }
            }
            Row::Error(search, error) => {
                let errors = &mut self.searches.get_mut(search)?.errors;
                if error >= errors.len() {
                    return None;
                }
                errors.remove(error);
                if error < errors.len() {
                    Some(Row::Error(search, error))
                } else {
                    self.searches
                        .get(search + 1)
                        .map(|_| Row::Search(search + 1))
                }
            }
        };
        self.rebuild();
        Some(TextPoint { row: next, byte: 0 })
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        for (search_index, search) in self.searches.iter().enumerate() {
            self.rows.push(Row::Search(search_index));
            if search.collapsed {
                continue;
            }
            for (doc_index, doc) in search.documents.iter().enumerate() {
                self.rows.push(Row::Document(search_index, doc_index));
                if !doc.collapsed {
                    self.rows.extend(
                        (0..doc.lines.len()).map(|line| Row::Line(search_index, doc_index, line)),
                    );
                }
            }
            self.rows
                .extend((0..search.errors.len()).map(|error| Row::Error(search_index, error)));
        }
    }
}

pub fn commafy(value: usize) -> String {
    let digits = value.to_string();
    let mut result = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(digit);
    }
    result
}

pub fn search_documents(
    query: &str,
    documents: Vec<Document>,
    options: LiteralOptions,
    one_entry_per_line: bool,
) -> Result<Search, SearchError> {
    search_documents_with_state(query, documents.into_iter().map(SearchDocument::from).collect(),
        options, one_entry_per_line, &mut Vec::new())
}

pub(crate) fn search_documents_with_state(
    query: &str,
    documents: Vec<SearchDocument>,
    options: LiteralOptions,
    one_entry_per_line: bool,
    updates: &mut Vec<(usize, crate::search::RegexState)>,
) -> Result<Search, SearchError> {
    let searched = documents.len();
    let selection_scope = documents.iter().any(|source| {
        let doc = &source.document;
        doc.selection.is_some_and(|selection| {
            let range = selection.range();
            range.start.0 != 0 || range.end.0 != doc.bytes.len()
        })
    });
    let mut found = Vec::new();
    for mut source in documents {
        if let Some(identity) = source.identity
            && let Some((_, state)) = updates.iter().rev().find(|(id, _)| *id == identity)
        {
            source.regex_state = state.clone();
        }
        let doc = source.document;
        let scope = crate::search::literal_scope_bytes(&doc.bytes, doc.selection.unwrap_or_default(), true, false, doc.selection.is_some())?;
        if scope.is_empty() { continue; }
        let matches = crate::search::literal_ranges_in_bytes_with_state(&doc.bytes, query, scope, options, &mut source.regex_state);
        if matches!(options.mode, crate::search::SearchMode::Regex { .. }) && let Some(identity) = source.identity {
            updates.push((identity, source.regex_state));
        }
        let matches = matches?;
        if matches.is_empty() {
            continue;
        }
        found.push(found_document(doc, matches, one_entry_per_line));
    }
    Ok(finish_search(
        query,
        found,
        searched,
        selection_scope,
        options,
        Vec::new(),
    ))
}

pub fn from_file_search(
    query: &str,
    result: crate::find_in_files::FileSearchResults,
    options: LiteralOptions,
    one_entry_per_line: bool,
) -> Search {
    let mut groups: Vec<(usize, Document, Vec<Range<ByteOffset>>)> = Vec::new();
    for hit in result.matches {
        if groups
            .last()
            .is_none_or(|(file_index, _, _)| *file_index != hit.file_index)
        {
            groups.push((
                hit.file_index,
                Document {
                    name: hit.path.display().to_string(),
                    bytes: hit.searched_bytes,
                    selection: None,
                },
                Vec::new(),
            ));
        }
        if let Some((_, _, matches)) = groups.last_mut() {
            matches.push(hit.range);
        }
    }
    let documents = groups
        .into_iter()
        .map(|(_, doc, matches)| found_document(doc, matches, one_entry_per_line))
        .collect();
    let mut search = finish_search(
        query,
        documents,
        result.files_searched,
        false,
        options,
        result.errors,
    );
    search.search_error = result.search_error;
    search
}

struct LinePreview {
    line: usize,
    start: usize,
    prefix_len: usize,
    original_length: usize,
    caption: String,
}

fn line_preview(bytes: &crate::byte_buffer::ByteBuffer, line: usize, digits: usize) -> LinePreview {
    let start = bytes.line_start(line).unwrap_or(bytes.len());
    let mut end = bytes.line_start(line + 1).unwrap_or(bytes.len());
    while end > start && matches!(bytes.byte(end - 1), Some(b'\r' | b'\n')) { end -= 1; }
    let end = end.min(start.saturating_add(2044));
    let text = bytes.slice(start..end).map(|bytes| crate::byte_projection::ByteProjection::from_bytes(&bytes).text().to_string()).unwrap_or_default();
    let length = text.len();
    let prefix = format!("\tLine {:>digits$}: ", line + 1);
    let original_length = prefix.len() + length + 2;
    let truncated = original_length >= 2048;
    let mut end = length.min(if truncated {
        2041 - prefix.len()
    } else {
        length
    });
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut caption = String::with_capacity(prefix.len() + end + if truncated { 4 } else { 0 });
    caption.push_str(&prefix);
    caption.push_str(&text[..end]);
    if truncated {
        caption.push_str(" ...");
    }
    LinePreview {
        line,
        start,
        prefix_len: prefix.len(),
        original_length,
        caption,
    }
}

fn found_document(
    doc: Document,
    matches: Vec<Range<ByteOffset>>,
    one_entry_per_line: bool,
) -> FoundDocument {
    let digits = doc.bytes.line_count().to_string().len();
    let projection = crate::byte_projection::ByteProjection::from_bytes(&doc.bytes);
    let mut lines: Vec<FoundLine> = Vec::new();
    let mut preview: Option<LinePreview> = None;
    for range in &matches {
        let line = doc.bytes.line_index(range.start.0).unwrap_or(doc.bytes.line_count() - 1);
        let preview = preview.get_or_insert_with(|| line_preview(&doc.bytes, line, digits));
        if preview.line != line {
            *preview = line_preview(&doc.bytes, line, digits);
        }
        let display_start = projection.display_position(ByteOffset(preview.start), crate::byte_projection::Affinity::Before).map_or(preview.start, |at| at.0);
        let highlight = preview.prefix_len + projection.display_position(range.start, crate::byte_projection::Affinity::Before).map_or(range.start.0, |at| at.0) - display_start
            ..preview.prefix_len + projection.display_position(range.end, crate::byte_projection::Affinity::After).map_or(range.end.0, |at| at.0) - display_start;
        if one_entry_per_line
            && lines
                .last()
                .is_some_and(|previous| previous.number == line + 1)
            && (preview.original_length < 2048 || highlight.end < 2048)
        {
            if let Some(previous) = lines.last_mut() {
                previous.highlights.push(highlight);
                previous.ranges.push(range.clone());
            }
        } else {
            lines.push(FoundLine {
                number: line + 1,
                caption: preview.caption.clone(),
                highlights: vec![highlight],
                ranges: vec![range.clone()],
            });
        }
    }
    FoundDocument {
        name: doc.name,
        lines,
        hits: matches.len(),
        collapsed: false,
    }
}

fn finish_search(
    query: &str,
    found: Vec<FoundDocument>,
    searched: usize,
    selection_scope: bool,
    options: LiteralOptions,
    errors: Vec<String>,
) -> Search {
    let hits = found.iter().map(|doc| doc.hits).sum::<usize>();
    let noun = if selection_scope { "selection" } else { "file" };
    let mut settings = String::from(match options.mode {
        crate::search::SearchMode::Normal => "Normal",
        crate::search::SearchMode::Extended => "Extended",
        crate::search::SearchMode::Regex { dot_matches_newline: true } => "RegEx.",
        crate::search::SearchMode::Regex { dot_matches_newline: false } => "RegEx",
    });
    if options.match_case || options.whole_word {
        settings.push_str(": ");
        if options.match_case {
            settings.push_str("Case");
        }
        if options.match_case && options.whole_word {
            settings.push('/');
        }
        if options.whole_word {
            settings.push_str("Word");
        }
    }
    let title = format!(
        "Search \"{}\" ({} {} in {} {}{} of {} searched) [{}]",
        query.replace(['\r', '\n'], ""),
        commafy(hits),
        if hits == 1 { "hit" } else { "hits" },
        commafy(found.len()),
        noun,
        if found.len() == 1 { "" } else { "s" },
        commafy(searched),
        settings
    );
    Search {
        title,
        documents: found,
        hits,
        searched,
        errors,
        search_error: None,
        collapsed: false,
    }
}
