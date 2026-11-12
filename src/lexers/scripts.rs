use super::{FoldLevel, FoldRange, LexError, LexResult, Span as TokenSpan};
use crate::languages::LanguageDefinition;

mod build;
mod command;
mod data;
mod indent;
mod installer;
mod shell;
mod sigil;

pub(super) fn lex(text: &str, profile: &LanguageDefinition) -> Result<LexResult, LexError> {
    let mut scan = Scan::new(text, profile);
    match profile.key() {
        "python" => indent::python(&mut scan, false),
        "gdscript" => indent::python(&mut scan, true),
        "coffeescript" => indent::coffee(&mut scan),
        "bash" => shell::bash(&mut scan),
        "batch" => shell::batch(&mut scan),
        "perl" => sigil::perl(&mut scan),
        "raku" => sigil::raku(&mut scan),
        "ruby" => sigil::ruby(&mut scan),
        "powershell" => command::powershell(&mut scan),
        "tcl" => command::tcl(&mut scan),
        "lua" => command::lua(&mut scan),
        "autoit" => command::autoit(&mut scan),
        "makefile" => build::makefile(&mut scan),
        "cmake" => build::cmake(&mut scan),
        "inno" => installer::inno(&mut scan),
        "kix" => installer::kix(&mut scan),
        "nsis" => installer::nsis(&mut scan),
        "nncrontab" => data::nncrontab(&mut scan),
        "pascal" => data::pascal(&mut scan),
        "rebol" => data::rebol(&mut scan),
        key => return Err(LexError::UnsupportedLanguage(key.to_owned())),
    }
    Ok(scan.finish())
}

struct Scan<'a> {
    text: &'a str,
    b: &'a [u8],
    p: &'a LanguageDefinition,
    styles: Vec<u16>,
    changes: Vec<(usize, i32)>,
    indent: bool,
    nesting: usize,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str, p: &'a LanguageDefinition) -> Self {
        Self {
            text,
            b: text.as_bytes(),
            p,
            styles: vec![0; text.len()],
            changes: Vec::new(),
            indent: false,
            nesting: 0,
        }
    }

    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(0)
    }
    fn advance(&self, i: usize) -> usize {
        let mut next = (i + 1).min(self.b.len());
        while next < self.b.len() && self.b[next] & 0xc0 == 0x80 {
            next += 1;
        }
        next
    }
    fn has(&self, i: usize, s: &[u8]) -> bool {
        self.b.get(i..i.saturating_add(s.len())) == Some(s)
    }
    fn paint(&mut self, a: usize, b: usize, style: u16) {
        let end = b.min(self.b.len());
        if a < end {
            self.styles[a..end].fill(style);
        }
    }
    fn prop(&self, key: &str, default: bool) -> bool {
        self.p
            .property(key)
            .map_or(default, |v| v.parse::<i32>().is_ok_and(|n| n != 0))
    }
    fn kw(&self, word: &str, set: usize, insensitive: bool) -> bool {
        self.p.keywords(set).split_ascii_whitespace().any(|k| {
            if insensitive {
                k.eq_ignore_ascii_case(word)
            } else {
                k == word
            }
        })
    }
    fn word(&self, i: usize) -> usize {
        let mut j = i;
        while ident(self.at(j)) {
            j += 1;
        }
        j
    }
    fn number(&self, i: usize) -> usize {
        let mut j = i + 1;
        while self.at(j).is_ascii_alphanumeric()
            || matches!(self.at(j), b'.' | b'_')
            || (matches!(self.at(j), b'+' | b'-')
                && matches!(self.at(j - 1), b'e' | b'E' | b'p' | b'P'))
        {
            j += 1;
        }
        j
    }
    fn line_end(&self, i: usize) -> usize {
        let mut j = i;
        while j < self.b.len() && !eol(self.at(j)) {
            j += 1;
        }
        j
    }
    fn next_line(&self, i: usize) -> usize {
        let j = self.line_end(i);
        (j + if self.has(j, b"\r\n") {
            2
        } else {
            usize::from(j < self.b.len())
        })
        .min(self.b.len())
    }
    fn bol(&self, i: usize) -> bool {
        i == 0 || self.at(i - 1) == b'\n' || self.at(i - 1) == b'\r' && self.at(i) != b'\n'
    }
    fn blank_prefix(&self, i: usize) -> bool {
        let mut j = i;
        while j > 0 && !eol(self.at(j - 1)) {
            j -= 1;
        }
        self.b[j..i].iter().all(|c| matches!(c, b' ' | b'\t'))
    }
    fn skip_space(&self, i: usize) -> usize {
        let mut j = i;
        while matches!(self.at(j), b' ' | b'\t') {
            j += 1;
        }
        j
    }
    fn quote(
        &self,
        start: usize,
        width: usize,
        escape: u8,
        multiline: bool,
        doubled: bool,
    ) -> usize {
        let q = self.at(start);
        let mut i = start + width;
        while i < self.b.len() {
            if self.at(i) == escape && escape != 0 {
                i = self.advance(i + 1);
                if self.has(i.saturating_sub(1), b"\r\n") {
                    i += 1;
                }
            } else if self.at(i) == q && (0..width).all(|n| self.at(i + n) == q) {
                if doubled && self.at(i + width) == q {
                    i += width * 2;
                } else {
                    return i + width;
                }
            } else if !multiline && eol(self.at(i)) {
                return i;
            } else {
                i += 1;
            }
        }
        i
    }
    fn delimited(&self, start: usize, open: &[u8], close: &[u8], nested: bool) -> usize {
        let mut i = start + open.len();
        let mut depth = 1;
        while i < self.b.len() {
            if self.has(i, close) {
                depth -= 1;
                i += close.len();
                if depth == 0 {
                    return i;
                }
            } else if nested && self.has(i, open) {
                depth += 1;
                i += open.len();
            } else {
                i += 1;
            }
        }
        i
    }
    fn expression_end(&self, start: usize) -> usize {
        let mut i = start + 1;
        let mut depth = 1;
        while i < self.b.len() {
            if matches!(self.at(i), b'\'' | b'"' | b'`') {
                let width = if self.at(i + 1) == self.at(i) && self.at(i + 2) == self.at(i) {
                    3
                } else {
                    1
                };
                i = self.quote(i, width, b'\\', true, false);
            } else if self.at(i) == b'#' {
                i = self.line_end(i);
            } else if self.at(i) == b'{' {
                depth += 1;
                i += 1;
            } else if self.at(i) == b'}' {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
            } else if self.at(i) == b'\\' {
                i = self.advance(i + 1);
            } else {
                i = self.advance(i);
            }
        }
        i
    }
    fn interpolated_quote(&self, start: usize, width: usize, multiline: bool) -> usize {
        let q = self.at(start);
        let mut i = start + width;
        while i < self.b.len() {
            if self.at(i) == b'\\' {
                i = self.advance(i + 1);
            } else if self.has(i, b"#{") {
                i = self.expression_end(i + 1);
            } else if (0..width).all(|n| self.at(i + n) == q) {
                return i + width;
            } else if !multiline && eol(self.at(i)) {
                return i;
            } else {
                i = self.advance(i);
            }
        }
        i
    }
    fn fold(&mut self, pos: usize, delta: i32) {
        self.changes.push((pos, delta));
    }
    fn finish(mut self) -> LexResult {
        let mut spans = Vec::new();
        let mut i = 0;
        while i < self.b.len() {
            let a = i;
            let style = self.styles[i];
            while i < self.b.len() && self.styles[i] == style {
                i += 1;
            }
            spans.push(TokenSpan { range: a..i, style });
        }
        let mut starts = vec![0];
        let mut i = 0;
        while i < self.b.len() {
            if self.has(i, b"\r\n") {
                i += 2;
                starts.push(i);
            } else if eol(self.b[i]) {
                i += 1;
                starts.push(i);
            } else {
                i += 1;
            }
        }
        self.changes.sort_by_key(|&(pos, _)| pos);
        let indents: Vec<i32> = starts
            .iter()
            .map(|&start| {
                let mut column = 0;
                for &c in &self.b[start..self.line_end(start)] {
                    match c {
                        b' ' => column += 1,
                        b'\t' => column = (column / 8 + 1) * 8,
                        _ => break,
                    }
                }
                column
            })
            .collect();
        let mut next_indent = vec![0; starts.len()];
        let mut following_indent = 0;
        for line in (0..starts.len()).rev() {
            next_indent[line] = following_indent;
            let start = starts[line];
            let end = self.line_end(start);
            if !self.b[start..end].iter().all(u8::is_ascii_whitespace)
                && self.at(self.skip_space(start)) != b'#'
            {
                following_indent = indents[line];
            }
        }
        let folding = self.prop("fold", self.p.key() == "tcl")
            && !matches!(self.p.key(), "batch" | "makefile" | "nncrontab" | "kix");
        let compact = self.prop("fold.compact", true);
        let mut levels = Vec::new();
        let mut depth = 0i32;
        let mut quote_depth = 0i32;
        let mut quote_indent = 0i32;
        let mut ci = 0;
        for (line, &start) in starts.iter().enumerate() {
            let end = self.line_end(start);
            let white = self.b[start..end].iter().all(u8::is_ascii_whitespace);
            let mut next = depth;
            let mut min = depth;
            let mut delta = 0;
            while ci < self.changes.len()
                && self.changes[ci].0 < starts.get(line + 1).copied().unwrap_or(self.b.len() + 1)
            {
                next = (next + self.changes[ci].1).max(0);
                min = min.min(next);
                delta += self.changes[ci].1;
                ci += 1;
            }
            if !self.indent
                && self.p.key() != "inno"
                && !self.prop("fold.at.else", false)
                && !self.prop("fold.perl.at.else", false)
            {
                min = depth;
            }
            if self.indent && folding && !white {
                let column = indents[line];
                min = column;
                next = next_indent[line];
                let before_quote = quote_depth;
                quote_depth = (quote_depth + delta).max(0);
                if before_quote == 0 && quote_depth > 0 {
                    quote_indent = column;
                }
                if before_quote > 0 {
                    min = quote_indent + before_quote;
                }
                if quote_depth > 0 {
                    next = quote_indent + quote_depth;
                }
            } else if self.indent && folding && quote_depth > 0 {
                min = quote_indent + quote_depth;
                quote_depth = (quote_depth + delta).max(0);
                next = quote_indent + quote_depth;
            }
            levels.push(FoldLevel {
                line,
                level: if folding { 0x400 + min as u32 } else { 0x400 },
                header: folding && !white && next > min,
                white: compact && white,
            });
            depth = next;
        }
        let mut folds = Vec::new();
        for (line, level) in levels.iter().enumerate().filter(|(_, l)| l.header) {
            let mut end = line + 1;
            while end < levels.len() && (levels[end].white || levels[end].level > level.level) {
                end += 1;
            }
            if end > line + 1 {
                folds.push(FoldRange {
                    start_line: line,
                    end_line: end - 1,
                });
            }
        }
        let mut builder = super::shared::Builder::new(self.text);
        builder.configure(self.p);
        for span in spans {
            if multiline_style(self.p.key(), span.style) {
                builder.state_in(span.range.clone(), u64::from(span.style));
            }
            builder.span(span.range, span.style);
        }
        let mut result = builder.finish();
        result.fold_levels = levels;
        result.folds = folds;
        result
    }
}

fn multiline_style(key: &str, style: u16) -> bool {
    match key {
        "python" => matches!(style, 3 | 4 | 6 | 7 | 16..=19),
        "gdscript" => matches!(style, 3 | 4 | 6 | 7 | 16),
        "coffeescript" => matches!(style, 6 | 7 | 22..=24),
        "bash" => matches!(style & !0x40, 5 | 6 | 10 | 11 | 13),
        "perl" => matches!(style, 3 | 6 | 7 | 17..=31),
        "raku" => matches!(style, 3 | 4 | 6..=14),
        "ruby" => matches!(style, 3 | 6 | 7 | 12 | 18 | 19 | 21..=28 | 41..=44),
        "powershell" => matches!(style, 2 | 3 | 13..=16),
        "tcl" => matches!(style, 5 | 9 | 20),
        "lua" => matches!(style, 1 | 6..=8),
        "autoit" => matches!(style, 2 | 7),
        "cmake" => matches!(style, 2..=4),
        "inno" => style == 7,
        "kix" => matches!(style, 2 | 3 | 10),
        "nsis" => matches!(style, 2..=4 | 18),
        "pascal" => matches!(style, 2 | 3 | 5 | 6 | 15),
        "rebol" => matches!(style, 2 | 3 | 5..=7 | 11 | 14),
        _ => false,
    }
}

fn ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 128
}
fn first(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 128
}
fn eol(c: u8) -> bool {
    matches!(c, b'\r' | b'\n')
}

fn keyword_style(s: &Scan<'_>, word: &str, styles: &[u16], default: u16, insensitive: bool) -> u16 {
    styles
        .iter()
        .enumerate()
        .find_map(|(set, &style)| s.kw(word, set, insensitive).then_some(style))
        .unwrap_or(default)
}

fn interpolation(s: &mut Scan<'_>, start: usize, end: usize, marker: u8, style: u16, escape: u8) {
    let mut i = start;
    while i < end {
        if s.at(i) == escape && escape != 0 {
            i += 2;
        } else if s.at(i) == marker && s.at(i + 1) != marker {
            let mut j = i + 1;
            if s.at(j) == b'{' {
                j = s.delimited(j, b"{", b"}", true).min(end);
            } else {
                while j < end
                    && (ident(s.at(j))
                        || matches!(s.at(j), b':' | b'!' | b'?' | b'@' | b'^' | b'*'))
                {
                    j += 1;
                }
            }
            if j > i + 1 {
                s.paint(i, j, style);
            }
            i = j;
        } else {
            i += 1;
        }
    }
}
