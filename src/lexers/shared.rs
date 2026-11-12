use std::ops::Range;

use crate::languages::LanguageDefinition;

use super::{FoldLevel, FoldRange, LexResult, LineState, Span};

pub fn keyword(profile: &LanguageDefinition, index: usize, word: &str) -> bool {
    profile
        .keywords(index)
        .split_ascii_whitespace()
        .any(|entry| entry == word)
}

pub fn property_bool(profile: &LanguageDefinition, name: &str, default: bool) -> bool {
    profile.property(name).map_or(default, |value| {
        value.parse::<i32>().is_ok_and(|value| value != 0)
    })
}

pub fn property_int(profile: &LanguageDefinition, name: &str, default: i32) -> i32 {
    profile
        .property(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

pub fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'\r' || bytes[position] == b'\n' {
            if bytes[position] == b'\r' && bytes.get(position + 1) == Some(&b'\n') {
                position += 1;
            }
            position += 1;
            ranges.push(start..position);
            start = position;
        } else {
            position += 1;
        }
    }
    ranges.push(start..text.len());
    ranges
}

pub struct Cursor<'a> {
    pub text: &'a str,
    pub position: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    pub fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub fn starts_with(&self, value: &str) -> bool {
        self.rest().starts_with(value)
    }

    pub fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.position += ch.len_utf8();
        Some(ch)
    }

    pub fn eat_while(&mut self, predicate: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&predicate) {
            self.bump();
        }
    }

    pub fn is_eof(&self) -> bool {
        self.position == self.text.len()
    }
}

pub struct Builder<'a> {
    text: &'a str,
    styles: Vec<u16>,
    lines: Vec<Range<usize>>,
    states: Vec<u64>,
    events: Vec<(usize, bool)>,
    fold_enabled: bool,
    fold_compact: bool,
}

impl<'a> Builder<'a> {
    pub fn new(text: &'a str) -> Self {
        let lines = line_ranges(text);
        Self {
            text,
            styles: vec![0; text.len()],
            states: vec![0; lines.len()],
            lines,
            events: Vec::new(),
            fold_enabled: true,
            fold_compact: false,
        }
    }

    pub fn configure(&mut self, profile: &LanguageDefinition) {
        self.fold_enabled = property_bool(profile, "fold", true);
        self.fold_compact = property_bool(profile, "fold.compact", true);
    }

    pub fn span(&mut self, range: Range<usize>, style: u16) {
        if range.start <= range.end
            && range.end <= self.styles.len()
            && self.text.is_char_boundary(range.start)
            && self.text.is_char_boundary(range.end)
        {
            self.styles[range].fill(style);
        }
    }

    pub fn set_line_state(&mut self, line: usize, state: u64) {
        if let Some(value) = self.states.get_mut(line) {
            *value = state;
        }
    }

    pub fn line_at(&self, position: usize) -> usize {
        self.lines
            .partition_point(|range| range.end <= position && range.start < range.end)
            .min(self.lines.len().saturating_sub(1))
    }

    pub fn state_in(&mut self, range: Range<usize>, state: u64) {
        let first = self
            .lines
            .partition_point(|bounds| bounds.end <= range.start);
        let end = self.lines.partition_point(|bounds| bounds.end <= range.end);
        for line in first..end {
            self.states[line] = state;
        }
    }

    pub fn fold_open(&mut self, position: usize) {
        self.events.push((position.min(self.text.len()), true));
    }

    pub fn fold_close(&mut self, position: usize) {
        self.events.push((position.min(self.text.len()), false));
    }

    pub fn finish(mut self) -> LexResult {
        let mut spans: Vec<Span> = Vec::new();
        for (position, ch) in self.text.char_indices() {
            let style = self.styles[position];
            if let Some(last) = spans.last_mut().filter(|last| last.style == style) {
                last.range.end = position + ch.len_utf8();
            } else {
                spans.push(Span {
                    range: position..position + ch.len_utf8(),
                    style,
                });
            }
        }
        self.events.sort_by_key(|event| event.0);
        let mut event = 0;
        let mut depth = 0u32;
        let mut stack = Vec::new();
        let mut fold_levels = Vec::new();
        let mut folds = Vec::new();
        let mut line_states = Vec::new();
        for (line, range) in self.lines.into_iter().enumerate() {
            let before = depth;
            while self.fold_enabled
                && event < self.events.len()
                && (self.events[event].0 < range.end || range.is_empty())
            {
                if self.events[event].1 {
                    depth += 1;
                    stack.push(line);
                } else {
                    depth = depth.saturating_sub(1);
                    if let Some(start_line) = stack.pop().filter(|start| *start < line) {
                        folds.push(FoldRange {
                            start_line,
                            end_line: line,
                        });
                    }
                }
                event += 1;
            }
            fold_levels.push(FoldLevel {
                line,
                level: 0x400 + before,
                header: depth > before,
                white: self.fold_compact && self.text[range.clone()].trim().is_empty(),
            });
            line_states.push(LineState {
                line,
                range,
                state: self.states[line],
            });
        }
        let last = line_states.len().saturating_sub(1);
        for start_line in stack.into_iter().filter(|start| *start < last) {
            folds.push(FoldRange {
                start_line,
                end_line: last,
            });
        }
        folds.sort_by_key(|fold| (fold.start_line, fold.end_line));
        LexResult {
            spans,
            fold_levels,
            folds,
            line_states,
        }
    }
}
