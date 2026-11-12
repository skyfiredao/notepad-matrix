use super::{FoldLevel, FoldRange, LexError, LexResult, LineState, Span};
use crate::languages::LanguageProfile;
use std::ops::Range;

#[path = "markup/config.rs"]
mod config;
#[path = "markup/document.rs"]
mod document;
#[path = "markup/records.rs"]
mod records;
#[path = "markup/sql.rs"]
mod sql;
#[path = "markup/web.rs"]
mod web;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagToken {
    pub range: Range<usize>,
    pub name_range: Range<usize>,
    pub name: String,
    pub closing: bool,
    pub self_closing: bool,
    pub attributes: Vec<Range<usize>>,
}

pub fn structural_tokens(text: &str, profile: &LanguageProfile) -> Vec<TagToken> {
    web::markup(text, profile, &mut Output::new(text))
}

pub fn search_results(
    text: &str,
    profile: &LanguageProfile,
    matches: &[Range<usize>],
) -> Result<LexResult, LexError> {
    if profile.key() != "searchResult" {
        return Err(LexError::UnsupportedLanguage(profile.key().to_owned()));
    }
    let mut out = Output::new(text);
    records::scan(text, profile, &mut out);
    for range in matches {
        if range.start > range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            return Err(LexError::InvalidRange(range.clone()));
        }
        out.paint(range.start, range.end, 4);
    }
    Ok(out.finish(text, profile))
}

pub fn lex(text: &str, profile: &LanguageProfile) -> Result<LexResult, LexError> {
    let mut out = Output::new(text);
    match profile.key() {
        "html" | "xml" | "asp" | "php" | "jsp" => {
            web::markup(text, profile, &mut out);
        }
        "css" => web::css(text, profile, &mut out, 0),
        "ini" | "props" => config::properties(text, profile, &mut out),
        "yaml" => config::yaml(text, profile, &mut out),
        "toml" => config::toml(text, profile, &mut out),
        "sql" | "mssql" => sql::scan(text, profile, &mut out),
        "diff" | "errorlist" | "escseq" | "registry" | "ihex" | "srec" | "tehex" | "nfo"
        | "searchResult" => records::scan(text, profile, &mut out),
        "latex" | "tex" | "txt2tags" | "postscript" => document::scan(text, profile, &mut out),
        key => return Err(LexError::UnsupportedLanguage(key.to_owned())),
    }
    if matches!(profile.key(), "html" | "xml" | "php" | "asp" | "jsp")
        && !enabled(profile, "fold.html", false)
    {
        out.levels.fill((0, false));
    }
    Ok(out.finish(text, profile))
}

struct Output {
    styles: Vec<u16>,
    levels: Vec<(usize, bool)>,
    starts: Vec<usize>,
    states: Vec<u64>,
}

impl Output {
    fn new(text: &str) -> Self {
        let b = text.as_bytes();
        let mut starts = vec![0];
        for i in 0..b.len() {
            if b[i] == b'\n' || (b[i] == b'\r' && b.get(i + 1) != Some(&b'\n')) {
                starts.push(i + 1);
            }
        }
        Self {
            styles: vec![0; text.len()],
            levels: vec![(0, false); starts.len()],
            states: vec![0; starts.len()],
            starts,
        }
    }

    fn paint(&mut self, start: usize, end: usize, style: u16) {
        let end = end.min(self.styles.len());
        if start < end {
            self.styles[start..end].fill(style);
            for line in self.line(start)..self.starts.len().saturating_sub(1) {
                let next = self.starts[line + 1];
                if next > end {
                    break;
                }
                if next > start {
                    self.states[line] = u64::from(style);
                }
            }
        }
    }

    fn line(&self, pos: usize) -> usize {
        self.starts
            .partition_point(|start| *start <= pos)
            .saturating_sub(1)
    }

    fn block(&mut self, start: usize, end: usize) {
        let first = self.line(start);
        let last = self.line(end.saturating_sub(1));
        if last > first {
            self.levels[first].1 = true;
            for line in first + 1..=last {
                self.levels[line].0 += 1;
            }
        }
    }

    fn finish(self, text: &str, profile: &LanguageProfile) -> LexResult {
        let mut spans = Vec::new();
        let mut start = 0;
        for end in 1..=self.styles.len() {
            if end == self.styles.len()
                || (text.is_char_boundary(end) && self.styles[end] != self.styles[start])
            {
                spans.push(Span {
                    range: start..end,
                    style: self.styles[start],
                });
                start = end;
            }
        }
        let fold_levels: Vec<_> = self
            .levels
            .iter()
            .enumerate()
            .map(|(line, &(level, header))| {
                let end = self.starts.get(line + 1).copied().unwrap_or(text.len());
                let fold = enabled(profile, "fold", true);
                FoldLevel {
                    line,
                    level: 0x400 + if fold { level as u32 } else { 0 },
                    header: fold && header,
                    white: enabled(profile, "fold.compact", true)
                        && text[self.starts[line]..end].trim().is_empty(),
                }
            })
            .collect();
        let mut folds = Vec::new();
        for (i, level) in fold_levels.iter().enumerate().filter(|(_, l)| l.header) {
            let end = fold_levels[i + 1..]
                .iter()
                .position(|l| l.level <= level.level)
                .map_or(fold_levels.len(), |n| i + 1 + n);
            if end > i + 1 {
                folds.push(FoldRange {
                    start_line: i,
                    end_line: end - 1,
                });
            }
        }
        let line_states = self
            .starts
            .iter()
            .enumerate()
            .map(|(line, &start)| LineState {
                line,
                range: start..self.starts.get(line + 1).copied().unwrap_or(text.len()),
                state: self.states[line] | ((self.levels[line].0 as u64) << 16),
            })
            .collect();
        LexResult {
            spans,
            fold_levels,
            folds,
            line_states,
        }
    }
}

fn enabled(profile: &LanguageProfile, property: &str, default: bool) -> bool {
    profile
        .property(property)
        .map_or(default, |value| value != "0" && value != "false")
}

fn keyword(profile: &LanguageProfile, index: usize, word: &str, insensitive: bool) -> bool {
    profile.keywords(index).split_whitespace().any(|key| {
        if let Some(prefix) = key.strip_prefix('^') {
            if insensitive {
                word.get(..prefix.len())
                    .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
            } else {
                word.starts_with(prefix)
            }
        } else if insensitive {
            key.eq_ignore_ascii_case(word)
        } else {
            key == word
        }
    })
}

fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 128
}
fn name(b: u8) -> bool {
    word(b) || matches!(b, b'-' | b':' | b'.')
}
fn line_end(b: &[u8], start: usize) -> usize {
    b[start..]
        .iter()
        .position(|c| matches!(c, b'\r' | b'\n'))
        .map_or(b.len(), |n| start + n)
}
fn until(b: &[u8], start: usize, terminator: &[u8]) -> usize {
    b.get(start..)
        .and_then(|tail| tail.windows(terminator.len()).position(|w| w == terminator))
        .map_or(b.len(), |n| start + n + terminator.len())
}
fn quoted(b: &[u8], start: usize, quote: u8, escapes: bool, doubled: bool) -> usize {
    let mut i = start + 1;
    while i < b.len() {
        if escapes && b[i] == b'\\' {
            i = (i + 2).min(b.len());
        } else if b[i] == quote {
            if doubled && b.get(i + 1) == Some(&quote) {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    i
}
fn lines(text: &str) -> Vec<Range<usize>> {
    let b = text.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    for i in 0..b.len() {
        if b[i] == b'\n' || (b[i] == b'\r' && b.get(i + 1) != Some(&b'\n')) {
            result.push(start..i + 1);
            start = i + 1;
        }
    }
    if start < b.len() {
        result.push(start..b.len());
    }
    result
}
