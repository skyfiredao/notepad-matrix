use std::fmt;
use std::ops::Range;

use crate::languages::LanguageDefinition;

mod cfamily;
pub mod markup;
mod other;
pub mod scripts;
pub mod shared;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: u16,
}

pub type TokenSpan = Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldLevel {
    pub line: usize,
    pub level: u32,
    pub header: bool,
    pub white: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineState {
    pub line: usize,
    pub range: Range<usize>,
    pub state: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LexResult {
    pub spans: Vec<Span>,
    pub fold_levels: Vec<FoldLevel>,
    pub folds: Vec<FoldRange>,
    pub line_states: Vec<LineState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LexError {
    UnsupportedLanguage(String),
    InvalidRange(Range<usize>),
    InvalidState(String),
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLanguage(key) => write!(f, "unsupported runtime lexer: {key}"),
            Self::InvalidRange(range) => write!(f, "invalid lexer byte range: {range:?}"),
            Self::InvalidState(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LexError {}

pub fn lex(text: &str, profile: &LanguageDefinition) -> Result<LexResult, LexError> {
    match profile.key() {
        "normal" => Ok(shared::Builder::new(text).finish()),
        "json" | "json5" | "actionscript" | "c" | "cpp" | "cs" | "java" | "javascript"
        | "javascript.js" | "typescript" | "go" | "objc" | "rc" | "d" | "rust" | "swift"
        | "avs" | "oscript" | "escript" | "gui4cli" | "baanc" => cfamily::lex(text, profile),
        "python" | "gdscript" | "coffeescript" | "bash" | "batch" | "perl" | "raku" | "ruby"
        | "powershell" | "tcl" | "lua" | "autoit" | "makefile" | "cmake" | "inno" | "kix"
        | "nsis" | "nncrontab" | "pascal" | "rebol" => scripts::lex(text, profile),
        "html" | "xml" | "asp" | "php" | "jsp" | "css" | "ini" | "props" | "yaml" | "toml"
        | "sql" | "mssql" | "diff" | "errorlist" | "escseq" | "registry" | "ihex" | "srec"
        | "tehex" | "nfo" | "searchResult" | "latex" | "tex" | "txt2tags" | "postscript" => {
            markup::lex(text, profile)
        }
        "ada" | "asm" | "asn1" | "blitzbasic" | "caml" | "cobol" | "csound" | "erlang"
        | "forth" | "fortran" | "fortran77" | "freebasic" | "haskell" | "hollywood" | "lisp"
        | "matlab" | "mmixal" | "nim" | "purebasic" | "r" | "sas" | "scheme" | "smalltalk"
        | "fcST" | "spice" | "vb" | "verilog" | "vhdl" | "visualprolog" => {
            other::lex(text, profile)
        }
        key => Err(LexError::UnsupportedLanguage(key.to_owned())),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelexReport {
    pub relexed_range: Range<usize>,
    pub reused_prefix_lines: usize,
    pub reused_suffix_lines: usize,
}

pub struct IncrementalLexer {
    text: String,
    profile: LanguageDefinition,
    result: LexResult,
}

impl IncrementalLexer {
    pub fn new(text: &str, profile: &LanguageDefinition) -> Result<Self, LexError> {
        Ok(Self {
            text: text.to_owned(),
            profile: profile.clone(),
            result: lex(text, profile)?,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn result(&self) -> &LexResult {
        &self.result
    }

    pub fn replace(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<RelexReport, LexError> {
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err(LexError::InvalidRange(range));
        }
        let edited_line = self
            .result
            .line_states
            .partition_point(|line| line.range.end <= range.start && !line.range.is_empty())
            .min(self.result.line_states.len().saturating_sub(1));
        let restartable = matches!(
            self.profile.key(),
            "normal"
                | "c"
                | "cpp"
                | "cs"
                | "java"
                | "swift"
                | "actionscript"
                | "javascript"
                | "javascript.js"
                | "typescript"
                | "go"
                | "rc"
                | "d"
                | "rust"
                | "avs"
                | "objc"
        );
        let restart_line = if restartable {
            (1..=edited_line)
                .rev()
                .find(|line| {
                    let state = &self.result.line_states[*line];
                    let content = self.text[state.range.clone()].trim_start_matches([' ', '\t']);
                    self.result.line_states[*line - 1].state == 0
                        && self
                            .result
                            .fold_levels
                            .get(*line)
                            .is_some_and(|level| level.level == 0x400)
                        && content
                            .chars()
                            .next()
                            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
                })
                .unwrap_or(0)
        } else {
            0
        };
        let restart = self.result.line_states[restart_line].range.start;
        let mut text = self.text.clone();
        text.replace_range(range, replacement);
        let mut tail = lex(&text[restart..], &self.profile)?;
        let mut result = LexResult {
            spans: self
                .result
                .spans
                .iter()
                .filter_map(|span| {
                    let end = span.range.end.min(restart);
                    (span.range.start < end).then_some(Span {
                        range: span.range.start..end,
                        style: span.style,
                    })
                })
                .collect(),
            fold_levels: self.result.fold_levels[..restart_line].to_vec(),
            folds: self
                .result
                .folds
                .iter()
                .filter(|fold| fold.end_line < restart_line)
                .cloned()
                .collect(),
            line_states: self.result.line_states[..restart_line].to_vec(),
        };
        for span in &mut tail.spans {
            span.range = span.range.start + restart..span.range.end + restart;
            if let Some(last) = result
                .spans
                .last_mut()
                .filter(|last| last.style == span.style && last.range.end == span.range.start)
            {
                last.range.end = span.range.end;
            } else {
                result.spans.push(span.clone());
            }
        }
        for level in &mut tail.fold_levels {
            level.line += restart_line;
        }
        for fold in &mut tail.folds {
            fold.start_line += restart_line;
            fold.end_line += restart_line;
        }
        for state in &mut tail.line_states {
            state.line += restart_line;
            state.range = state.range.start + restart..state.range.end + restart;
        }
        result.fold_levels.extend(tail.fold_levels);
        result.folds.extend(tail.folds);
        result.line_states.extend(tail.line_states);
        self.text = text;
        self.result = result;
        Ok(RelexReport {
            relexed_range: restart..self.text.len(),
            reused_prefix_lines: restart_line,
            reused_suffix_lines: 0,
        })
    }
}
