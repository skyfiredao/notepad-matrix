use super::folding::{Action, Event};
use super::{Definition, LineCommentPlacement, UdlStyleId, nesting};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: UdlStyleId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldLevel {
    pub level: usize,
    pub next_level: usize,
    pub header: bool,
    pub white: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LexResult {
    pub spans: Vec<Span>,
    pub folds: Vec<FoldRange>,
    pub levels: Vec<FoldLevel>,
}

pub fn lex(text: &str, definition: &Definition) -> LexResult {
    Scanner::new(text, definition).scan()
}

fn whitespace(b: u8) -> bool {
    b > 0 && b <= b' '
}
fn advance(text: &str, p: usize) -> usize {
    p + text
        .get(p..)
        .and_then(|s| s.chars().next())
        .map_or(0, char::len_utf8)
}
fn matches(text: &str, p: usize, token: &str, case: bool) -> bool {
    !token.is_empty()
        && text.get(p..p.saturating_add(token.len())).is_some_and(|s| {
            if case {
                s.eq_ignore_ascii_case(token)
            } else {
                s == token
            }
        })
}
fn plain(raw: &str) -> Vec<String> {
    raw.split(' ')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn forward_tokens(raw: &str) -> Vec<String> {
    let raw = raw.strip_prefix("((").unwrap_or(raw);
    let raw = raw.strip_suffix("))").unwrap_or(raw);
    plain(raw)
        .into_iter()
        .flat_map(|t| {
            if t == "EOL" {
                vec!["\r\n".into(), "\n".into(), "\r".into()]
            } else {
                vec![t]
            }
        })
        .collect()
}
fn first(tokens: &[String], text: &str, p: usize, case: bool) -> Option<usize> {
    tokens
        .iter()
        .find(|t| matches(text, p, t, case))
        .map(String::len)
}

fn groups(raw: &str) -> Vec<Vec<String>> {
    let mut groups = Vec::new();
    let mut rest = raw.trim_start_matches(' ');
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("((") {
            let end = after.find("))").unwrap_or(after.len());
            let mut alternatives = Vec::new();
            for token in plain(&after[..end]) {
                if token == "EOL" {
                    alternatives.extend(["\r\n".into(), "\n".into(), "\r".into()]);
                } else {
                    alternatives.push(token);
                }
            }
            if !alternatives.is_empty() {
                groups.push(alternatives);
            }
            rest = after.get(end + 2..).unwrap_or("").trim_start_matches(' ');
        } else {
            let end = rest.find(' ').unwrap_or(rest.len());
            groups.push(vec![rest[..end].to_owned()]);
            rest = rest[end..].trim_start_matches(' ');
        }
    }
    groups
}
struct Delimited {
    open: Vec<Vec<String>>,
    escape: Vec<Vec<String>>,
    close: Vec<Vec<String>>,
}
impl Delimited {
    fn new(open: &str, escape: &str, close: &str) -> Self {
        Self {
            open: groups(open),
            escape: groups(escape),
            close: groups(close),
        }
    }
    fn opening(&self, text: &str, p: usize, case: bool) -> Option<(usize, usize)> {
        self.open
            .iter()
            .enumerate()
            .find_map(|(i, g)| first(g, text, p, case).map(|n| (i, n)))
    }
    fn ending(&self, pair: usize, text: &str, p: usize, case: bool) -> Option<usize> {
        self.close.get(pair).and_then(|g| first(g, text, p, case))
    }
    fn escaping(&self, pair: usize, text: &str, p: usize, case: bool) -> Option<usize> {
        self.escape.get(pair).and_then(|g| first(g, text, p, case))
    }
}
#[derive(Clone, Copy)]
struct Frame {
    kind: usize,
    pair: usize,
}
impl Frame {
    fn style(self) -> UdlStyleId {
        match self.kind {
            8 => UdlStyleId::Comment,
            9 => UdlStyleId::CommentLine,
            i => UdlStyleId::ALL[i + 16],
        }
    }
}
enum Forward {
    Open(Frame, usize),
    Operator(usize),
    Fold(Action, usize),
}

#[derive(Clone, Debug)]
struct Word {
    parts: Vec<String>,
    gaps: Vec<Gap>,
}
#[derive(Clone, Copy, Debug)]
enum Gap {
    Horizontal,
    Multiline,
}
impl Gap {
    fn accepts(self, byte: u8) -> bool {
        match self {
            Self::Horizontal => matches!(byte, b' ' | b'\t'),
            Self::Multiline => whitespace(byte),
        }
    }
}
fn words(raw: &str) -> Vec<Word> {
    let mut encoded = String::new();
    let mut quote = None;
    let mut non_whitespace_found = false;
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let next = chars.get(i + 1).copied().unwrap_or('\0');
        if (c == '"' || c == '\'') && (quote.is_none() || quote == Some(c)) {
            quote = if quote.is_none() { Some(c) } else { None };
        } else if c == '\\' && matches!(next, '\'' | '"' | '\\') {
            encoded.push(next);
            i += 1;
        } else if let Some(quote) = quote {
            if c > ' ' {
                encoded.push(c);
                non_whitespace_found = true;
            } else if non_whitespace_found
                && i > 0
                && chars[i - 1] != '"'
                && next != '"'
                && next > ' '
            {
                encoded.push(if quote == '"' { '\u{b}' } else { '\u{8}' });
            }
        } else {
            encoded.push(c);
        }
        i += 1;
    }
    let mut out = Vec::new();
    for token in encoded
        .split([' ', '\t', '\r', '\n'])
        .filter(|s| !s.is_empty())
    {
        let mut parts = Vec::new();
        let mut gaps = Vec::new();
        let mut part = String::new();
        for c in token.chars() {
            if matches!(c, '\u{8}' | '\u{b}') {
                parts.push(std::mem::take(&mut part));
                gaps.push(if c == '\u{b}' {
                    Gap::Multiline
                } else {
                    Gap::Horizontal
                });
            } else {
                part.push(c);
            }
        }
        parts.push(part);
        if parts.iter().any(|part| !part.is_empty()) {
            out.push(Word { parts, gaps });
        }
    }
    out.sort_by_key(|w| {
        std::cmp::Reverse(w.parts.iter().map(String::len).sum::<usize>() + w.parts.len())
    });
    out
}

struct Scanner<'a> {
    text: &'a str,
    d: &'a Definition,
    keywords: [Vec<Word>; 8],
    operators: Vec<String>,
    operators2: Vec<Word>,
    folds1: [Vec<String>; 3],
    fold_words: [[Vec<Word>; 3]; 2],
    delimiters: [Delimited; 10],
    line_continue: Vec<Vec<String>>,
    numbers: super::number::NumberTokens,
    root_number_separators: Vec<String>,
    stack: Vec<Frame>,
    events: Vec<Event>,
    comment_cover: Vec<Range<usize>>,
    result: LexResult,
}
impl<'a> Scanner<'a> {
    fn new(text: &'a str, d: &'a Definition) -> Self {
        let mut scanner = Self {
            text,
            d,
            keywords: std::array::from_fn(|i| words(&d.keywords[i])),
            operators: forward_tokens(&d.operators[0]),
            operators2: words(&d.operators[1]),
            folds1: [
                &d.folding[0].open,
                &d.folding[0].middle,
                &d.folding[0].close,
            ]
            .map(|s| forward_tokens(s)),
            fold_words: std::array::from_fn(|i| {
                [
                    &d.folding[i + 1].open,
                    &d.folding[i + 1].middle,
                    &d.folding[i + 1].close,
                ]
                .map(|s| words(s))
            }),
            delimiters: std::array::from_fn(|i| match i {
                8 => Delimited::new(&d.comments.block_open, "", &d.comments.block_close),
                9 => Delimited::new(&d.comments.line_open, "", &d.comments.line_close),
                _ => Delimited::new(
                    &d.delimiters[i].open,
                    &d.delimiters[i].escape,
                    &d.delimiters[i].close,
                ),
            }),
            line_continue: groups(&d.comments.line_continue),
            numbers: super::number::NumberTokens::new(&d.numbers),
            root_number_separators: Vec::new(),
            stack: Vec::new(),
            events: Vec::new(),
            comment_cover: Vec::new(),
            result: LexResult::default(),
        };
        scanner
            .root_number_separators
            .extend(scanner.operators.iter().cloned());
        for groups in [
            &scanner.delimiters[9].open,
            &scanner.line_continue,
            &scanner.delimiters[9].close,
            &scanner.delimiters[8].open,
            &scanner.delimiters[8].close,
        ] {
            scanner
                .root_number_separators
                .extend(groups.iter().flatten().cloned());
        }
        for list in &scanner.folds1 {
            scanner.root_number_separators.extend(list.iter().cloned());
        }
        for delimiter in &scanner.delimiters[..3] {
            scanner
                .root_number_separators
                .extend(delimiter.close.iter().flatten().cloned());
        }
        scanner
    }
    fn mask(&self) -> u32 {
        self.stack.last().map_or(u32::MAX, |f| {
            self.d.style(f.style()).nesting
                | if f.kind >= 8 {
                    nesting::comment_fold(0) | nesting::comment_fold(1) | nesting::comment_fold(2)
                } else {
                    0
                }
        })
    }
    fn line_placement(&self, p: usize) -> bool {
        let start = self.text[..p].rfind(['\r', '\n']).map_or(0, |i| i + 1);
        match self.d.line_comment_placement {
            LineCommentPlacement::Anywhere => true,
            LineCommentPlacement::BeginningOfLine => p == start,
            LineCommentPlacement::AfterWhitespace => self.text[start..p].bytes().all(whitespace),
        }
    }
    fn comment_open(&self, p: usize, kind: usize) -> Option<Forward> {
        let bit = if kind == 8 {
            nesting::COMMENT
        } else {
            nesting::COMMENT_LINE
        };
        if self.mask() & bit == 0 {
            return None;
        }
        let (pair, n) = self.delimiters[kind].opening(self.text, p, self.d.case_ignored)?;
        if kind == 9 && !self.line_placement(p) {
            return None;
        }
        Some(Forward::Open(Frame { kind, pair }, n))
    }
    fn forward(&self, p: usize) -> Option<Forward> {
        let mask = self.mask();
        if self.stack.is_empty()
            && let Some(event) = self.comment_open(p, 8).or_else(|| self.comment_open(p, 9))
        {
            return Some(event);
        }
        for i in 0..8 {
            if mask & nesting::delimiter(i) != 0
                && let Some((pair, n)) =
                    self.delimiters[i].opening(self.text, p, self.d.case_ignored)
            {
                return Some(Forward::Open(Frame { kind: i, pair }, n));
            }
        }
        if !self.stack.is_empty()
            && let Some(event) = self.comment_open(p, 8).or_else(|| self.comment_open(p, 9))
        {
            return Some(event);
        }
        if mask & nesting::OPERATORS1 != 0
            && let Some(n) = first(&self.operators, self.text, p, self.d.case_ignored)
        {
            return Some(Forward::Operator(n));
        }
        if self.stack.is_empty() {
            for i in 0..3 {
                if let Some(n) = first(&self.folds1[i], self.text, p, self.d.case_ignored) {
                    return Some(Forward::Fold(
                        [Action::Open, Action::Middle, Action::Close][i],
                        n,
                    ));
                }
            }
        }
        None
    }
    fn ending(&self, p: usize) -> Option<usize> {
        self.stack
            .last()
            .and_then(|f| self.delimiters[f.kind].ending(f.pair, self.text, p, self.d.case_ignored))
    }
    fn boundary(&self, p: usize) -> bool {
        self.word_boundary(p)
            || self.stack.last().is_some_and(|f| {
                self.delimiters[f.kind]
                    .escaping(f.pair, self.text, p, self.d.case_ignored)
                    .is_some()
            })
    }
    fn word_boundary(&self, p: usize) -> bool {
        p == self.text.len()
            || self.text.as_bytes().get(p).is_some_and(|b| whitespace(*b))
            || self.forward(p).is_some()
            || self.ending(p).is_some()
    }
    fn number_boundary(&self, p: usize) -> bool {
        if p == self.text.len() || self.text.as_bytes().get(p).is_some_and(|b| whitespace(*b)) {
            return true;
        }
        let Some(frame) = self.stack.last() else {
            return first(
                &self.root_number_separators,
                self.text,
                p,
                self.d.case_ignored,
            )
            .is_some();
        };
        if frame.kind < 9
            && self.delimiters[frame.kind]
                .close
                .iter()
                .any(|tokens| first(tokens, self.text, p, self.d.case_ignored).is_some())
        {
            return true;
        }
        let mask = self.mask();
        for kind in 0..10 {
            let bit = match kind {
                8 => nesting::COMMENT,
                9 => nesting::COMMENT_LINE,
                _ => nesting::delimiter(kind),
            };
            if mask & bit != 0
                && self.delimiters[kind]
                    .opening(self.text, p, self.d.case_ignored)
                    .is_some()
            {
                return true;
            }
        }
        mask & nesting::OPERATORS1 != 0
            && first(&self.operators, self.text, p, self.d.case_ignored).is_some()
    }
    fn nested_number_start(&self, p: usize) -> Option<usize> {
        if self.stack.is_empty() || self.mask() & nesting::NUMBERS == 0 {
            return None;
        }
        let mut q = advance(self.text, p);
        while !self.boundary(q) {
            if self
                .numbers
                .end(
                    self.text,
                    q,
                    self.d.case_ignored,
                    self.d.decimal_separator,
                    |end| self.number_boundary(end),
                )
                .is_some()
            {
                return Some(q);
            }
            q = advance(self.text, q);
        }
        None
    }
    fn word(&self, words: &[Word], p: usize, prefix: bool, limit: Option<usize>) -> Option<usize> {
        for word in words {
            let mut end = p;
            let mut found = true;
            for (i, part) in word.parts.iter().enumerate() {
                if i > 0 {
                    let start = end;
                    while self
                        .text
                        .as_bytes()
                        .get(end)
                        .is_some_and(|b| word.gaps[i - 1].accepts(*b))
                    {
                        end += 1;
                    }
                    if start == end {
                        found = false;
                        break;
                    }
                }
                if !matches(self.text, end, part, self.d.case_ignored) {
                    found = false;
                    break;
                }
                end += part.len();
            }
            if limit.is_some_and(|limit| end > limit) {
                continue;
            }
            let multipart = word.parts.len() > 1;
            if found && ((prefix && !multipart) || self.word_boundary(end) || limit == Some(end)) {
                if prefix && !multipart {
                    while !self.word_boundary(end) && limit != Some(end) {
                        end = advance(self.text, end);
                    }
                }
                if prefix
                    && multipart
                    && self
                        .text
                        .as_bytes()
                        .get(end)
                        .is_some_and(|b| whitespace(*b))
                {
                    let saved = end;
                    while self.text.as_bytes().get(end).is_some_and(|b| {
                        word.gaps
                            .last()
                            .copied()
                            .unwrap_or(Gap::Horizontal)
                            .accepts(*b)
                    }) {
                        end += 1;
                    }
                    let next = end;
                    while !self.word_boundary(end) {
                        end = advance(self.text, end);
                    }
                    if next == end {
                        end = saved;
                    }
                }
                return Some(end);
            }
        }
        None
    }
    fn paint(&mut self, start: usize, end: usize, style: UdlStyleId) {
        if start == end {
            return;
        }
        if matches!(style, UdlStyleId::Comment | UdlStyleId::CommentLine)
            || self.stack.iter().any(|f| f.kind >= 8)
        {
            if let Some(last) = self.comment_cover.last_mut().filter(|r| r.end == start) {
                last.end = end;
            } else {
                self.comment_cover.push(start..end);
            }
        }
        if let Some(last) = self.result.spans.last_mut()
            && last.style == style
            && last.range.end == start
        {
            last.range.end = end;
            return;
        }
        self.result.spans.push(Span {
            range: start..end,
            style,
        });
    }
    fn scan(mut self) -> LexResult {
        let mut p = 0;
        let mut word_continuation = false;
        while p < self.text.len() {
            if whitespace(self.text.as_bytes()[p]) {
                word_continuation = false;
            }
            let frame = self.stack.last().copied();
            if let Some(f) = frame {
                if let Some(n) =
                    self.delimiters[f.kind].escaping(f.pair, self.text, p, self.d.case_ignored)
                {
                    word_continuation = self
                        .text
                        .as_bytes()
                        .get(p + n)
                        .is_some_and(|b| !whitespace(*b));
                    let end = advance(self.text, p + n);
                    self.paint(p, end, f.style());
                    p = end;
                    continue;
                }
                if let Some(n) = self.ending(p) {
                    word_continuation = false;
                    if f.kind < 8
                        && matches!(self.text.as_bytes()[p], b'\r' | b'\n')
                        && self.stack.len() > 1
                        && self.stack[self.stack.len() - 2].kind == 9
                    {
                        self.stack.pop();
                        continue;
                    }
                    self.paint(p, p + n, f.style());
                    self.stack.pop();
                    p += n;
                    continue;
                }
                if f.kind == 9 && matches!(self.text.as_bytes()[p], b'\r' | b'\n') {
                    let continued = self.line_continue.get(f.pair).is_some_and(|tokens| {
                        tokens.iter().any(|token| {
                            p >= token.len()
                                && matches(self.text, p - token.len(), token, self.d.case_ignored)
                        })
                    });
                    let n = if self.text[p..].starts_with("\r\n") {
                        2
                    } else {
                        1
                    };
                    self.paint(p, p + n, f.style());
                    p += n;
                    if !continued {
                        self.stack.pop();
                    }
                    continue;
                }
            }
            if self.stack.is_empty() && whitespace(self.text.as_bytes()[p]) {
                self.paint(p, p + 1, UdlStyleId::Identifier);
                p += 1;
                continue;
            }
            let forward = self.forward(p);
            if self.mask() & nesting::NUMBERS != 0
                && (self.stack.is_empty() || forward.is_none())
                && let Some(end) = self.numbers.end(
                    self.text,
                    p,
                    self.d.case_ignored,
                    self.d.decimal_separator,
                    |end| self.number_boundary(end),
                )
            {
                self.paint(p, end, UdlStyleId::Number);
                p = end;
                word_continuation = false;
                continue;
            }
            if let Some(event) = forward {
                word_continuation = false;
                match event {
                    Forward::Open(f, n) => {
                        self.paint(p, p + n, f.style());
                        self.stack.push(f);
                        p += n;
                    }
                    Forward::Operator(n) => {
                        self.paint(p, p + n, UdlStyleId::Operator);
                        p += n;
                    }
                    Forward::Fold(action, n) => {
                        self.events.push(Event { byte: p, action });
                        self.paint(p, p + n, UdlStyleId::FolderInCode1);
                        p += n;
                    }
                }
                continue;
            }
            if whitespace(self.text.as_bytes()[p]) {
                self.paint(p, p + 1, frame.map_or(UdlStyleId::Identifier, Frame::style));
                p += 1;
                continue;
            }
            let mut keyword = None;
            let numeric_start = self.nested_number_start(p);
            if word_continuation {
                let start = p;
                p = advance(self.text, p);
                while !self.boundary(p) && numeric_start != Some(p) {
                    p = advance(self.text, p);
                }
                self.paint(start, p, frame.map_or(UdlStyleId::Default, Frame::style));
                continue;
            }
            if self.mask() & nesting::OPERATORS2 != 0
                && let Some(end) = self.word(&self.operators2, p, false, numeric_start)
            {
                self.paint(p, end, UdlStyleId::Operator);
                p = end;
                continue;
            }
            let mut folder = None;
            for group in 0..2 {
                for action in 0..3 {
                    let bit = if group == 0 {
                        nesting::code2(action)
                    } else {
                        nesting::comment_fold(action)
                    };
                    if self.mask() & bit != 0
                        && let Some(end) =
                            self.word(&self.fold_words[group][action], p, false, numeric_start)
                    {
                        folder = Some((
                            end,
                            group,
                            [Action::Open, Action::Middle, Action::Close][action],
                        ));
                        break;
                    }
                }
                if folder.is_some() {
                    break;
                }
            }
            if let Some((end, group, action)) = folder {
                self.events.push(Event { byte: p, action });
                self.paint(
                    p,
                    end,
                    if group == 0 {
                        UdlStyleId::FolderInCode2
                    } else {
                        UdlStyleId::FolderInComment
                    },
                );
                p = end;
                continue;
            }
            for i in 0..8 {
                if self.mask() & nesting::keyword(i) == 0 {
                    continue;
                }
                if let Some(end) = self.word(
                    &self.keywords[i],
                    p,
                    self.d.prefix_keywords[i],
                    numeric_start,
                ) {
                    keyword = Some((end, UdlStyleId::ALL[i + 4]));
                    break;
                }
            }
            if let Some((end, style)) = keyword {
                self.paint(p, end, style);
                p = end;
                continue;
            }
            let start = p;
            p = advance(self.text, p);
            while !self.boundary(p) && numeric_start != Some(p) {
                p = advance(self.text, p);
            }
            self.paint(start, p, frame.map_or(UdlStyleId::Default, Frame::style));
        }
        let (folds, levels) = super::folding::build(
            self.text,
            self.events,
            &self.comment_cover,
            self.d.allow_fold_comments,
            self.d.fold_compact,
        );
        self.result.folds = folds;
        self.result.levels = levels;
        self.result
    }
}
