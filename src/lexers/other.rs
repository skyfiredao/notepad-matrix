use crate::languages::LanguageDefinition as Profile;
use crate::lexers::{LexError, LexResult, shared::Builder};

#[path = "other/ada.rs"]
mod ada;
#[path = "other/basic.rs"]
mod basic;
#[path = "other/functional.rs"]
mod functional;
#[path = "other/hardware.rs"]
mod hardware;
#[path = "other/scientific.rs"]
mod scientific;

pub(super) fn lex(text: &str, profile: &Profile) -> Result<LexResult, LexError> {
    let mut s = Scan::new(text, profile);
    match profile.key() {
        "ada" => ada::lex(&mut s),
        "caml" => functional::caml(&mut s),
        "haskell" => functional::haskell(&mut s),
        "lisp" | "scheme" => functional::lisp(&mut s),
        "erlang" => functional::erlang(&mut s),
        "nim" => functional::nim(&mut s),
        "smalltalk" => functional::smalltalk(&mut s),
        "visualprolog" => functional::visualprolog(&mut s),
        "blitzbasic" | "purebasic" | "freebasic" => basic::basic(&mut s),
        "vb" => basic::vb(&mut s),
        "hollywood" => basic::hollywood(&mut s),
        "fortran" | "fortran77" => scientific::fortran(&mut s),
        "matlab" => scientific::matlab(&mut s),
        "r" => scientific::r(&mut s),
        "sas" => scientific::sas(&mut s),
        "spice" => scientific::spice(&mut s),
        "csound" => scientific::csound(&mut s),
        "asm" => hardware::asm(&mut s),
        "asn1" => hardware::asn1(&mut s),
        "cobol" => hardware::cobol(&mut s),
        "fcST" => hardware::structured_text(&mut s),
        "forth" => hardware::forth(&mut s),
        "mmixal" => hardware::mmixal(&mut s),
        "verilog" => hardware::verilog(&mut s),
        "vhdl" => hardware::vhdl(&mut s),
        key => return Err(LexError::UnsupportedLanguage(key.to_owned())),
    }
    Ok(s.finish())
}

struct Scan<'a> {
    text: &'a str,
    b: &'a [u8],
    p: &'a Profile,
    i: usize,
    styles: Vec<u16>,
    events: Vec<(usize, i32)>,
    indent_levels: Vec<u32>,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str, p: &'a Profile) -> Self {
        Self {
            text,
            b: text.as_bytes(),
            p,
            i: 0,
            styles: vec![0; text.len()],
            events: Vec::new(),
            indent_levels: Vec::new(),
        }
    }

    fn ch(&self) -> u8 {
        self.at(self.i)
    }
    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(0)
    }
    fn next(&self) -> u8 {
        self.at(self.i + 1)
    }
    fn more(&self) -> bool {
        self.i < self.b.len()
    }
    fn is(&self, value: &str) -> bool {
        self.b[self.i..].starts_with(value.as_bytes())
    }
    fn paint(&mut self, start: usize, style: u16) {
        self.styles[start..self.i].fill(style);
    }
    fn take(&mut self, n: usize, style: u16) {
        let start = self.i;
        self.i = (self.i + n).min(self.b.len());
        self.paint(start, style);
    }
    fn line_start(&self) -> usize {
        self.b[..self.i]
            .iter()
            .rposition(|&b| b == b'\n' || b == b'\r')
            .map_or(0, |p| p + 1)
    }
    fn first(&self) -> bool {
        self.b[self.line_start()..self.i]
            .iter()
            .all(|b| b.is_ascii_whitespace())
    }
    fn line_end(&self) -> usize {
        self.b[self.i..]
            .iter()
            .position(|&b| b == b'\r' || b == b'\n')
            .map_or(self.b.len(), |p| self.i + p)
    }
    fn line(&mut self, style: u16) {
        let start = self.i;
        self.i = self.line_end();
        self.paint(start, style);
    }
    fn until(&mut self, end: &str, style: u16) {
        let start = self.i;
        while self.more() && !self.is(end) {
            self.i += 1;
        }
        if self.more() {
            self.i += end.len();
        }
        self.paint(start, style);
    }
    fn block(&mut self, open: &str, close: &str, nested: bool, style: u16) {
        let start = self.i;
        self.i += open.len();
        let mut depth = 1usize;
        while self.more() {
            if self.is(close) {
                self.i += close.len();
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if nested && self.is(open) {
                depth += 1;
                self.i += open.len();
            } else {
                self.i += 1;
            }
        }
        self.paint(start, style);
        if self.flag("fold.comment", false) {
            self.region(start, self.i);
        }
    }
    fn quote(
        &mut self,
        quote: u8,
        style: u16,
        error: u16,
        escape: bool,
        doubled: bool,
        multiline: bool,
    ) {
        let start = self.i;
        self.i += 1;
        let mut closed = false;
        while self.more() {
            if !multiline && matches!(self.ch(), b'\r' | b'\n') {
                break;
            }
            if escape && self.ch() == b'\\' {
                self.i = (self.i
                    + if self.next() == b'\r' && self.at(self.i + 2) == b'\n' {
                        3
                    } else {
                        2
                    })
                .min(self.b.len());
            } else if self.ch() == quote {
                self.i += 1;
                if doubled && self.ch() == quote {
                    self.i += 1;
                } else {
                    closed = true;
                    break;
                }
            } else {
                self.i += 1;
            }
        }
        self.paint(start, if closed || multiline { style } else { error });
    }
    fn word(&mut self, extra: &[u8]) -> usize {
        let start = self.i;
        if !self.more() {
            return start;
        }
        self.i += 1;
        while self.more() && (word(self.ch()) || extra.contains(&self.ch())) {
            self.i += 1;
        }
        start
    }
    fn token(&self, start: usize) -> &str {
        self.text.get(start..self.i).unwrap_or("")
    }
    fn kw(&self, index: usize, word: &str, insensitive: bool) -> bool {
        self.p.keywords(index).split_ascii_whitespace().any(|k| {
            if insensitive {
                k.eq_ignore_ascii_case(word)
            } else {
                k == word
            }
        })
    }
    fn classify(&mut self, start: usize, fallback: u16, classes: &[u16], insensitive: bool) -> u16 {
        let token = self.token(start);
        let style = classes
            .iter()
            .enumerate()
            .find_map(|(i, &style)| self.kw(i, token, insensitive).then_some(style))
            .unwrap_or(fallback);
        self.paint(start, style);
        style
    }
    fn flag(&self, property: &str, default: bool) -> bool {
        self.p
            .property(property)
            .map_or(default, |v| v != "0" && !v.is_empty())
    }
    fn number(&mut self, style: u16, extra: &[u8]) {
        let start = self.i;
        self.i += 1;
        while self.more() {
            let c = self.ch();
            if word(c)
                || extra.contains(&c)
                || (matches!(c, b'+' | b'-')
                    && matches!(self.at(self.i - 1), b'e' | b'E' | b'p' | b'P' | b'd' | b'D'))
            {
                self.i += 1;
            } else {
                break;
            }
        }
        self.paint(start, style);
    }
    fn event(&mut self, pos: usize, delta: i32) {
        self.events.push((pos, delta));
    }
    fn region(&mut self, start: usize, end: usize) {
        if self.b[start..end].contains(&b'\n') {
            self.event(start, 1);
            self.event(end.saturating_sub(1), -1);
        }
    }
    fn delimit(&mut self, style: u16, opens: &[u8], closes: &[u8]) {
        if opens.contains(&self.ch()) {
            self.event(self.i, 1);
        }
        if closes.contains(&self.ch()) {
            self.event(self.i, -1);
        }
        self.take(1, style);
    }
    fn finish(self) -> LexResult {
        let mut result = Builder::new(self.text);
        result.configure(self.p);
        let mut start = 0;
        while start < self.styles.len() {
            let style = self.styles[start];
            let mut end = start + 1;
            while end < self.styles.len() && self.styles[end] == style {
                end += 1;
            }
            while end < self.styles.len() && !self.text.is_char_boundary(end) {
                end += 1;
            }
            result.span(start..end, style);
            if self.b[start..end].contains(&b'\n') && style != 0 {
                result.state_in(start..end, u64::from(style));
            }
            start = end;
        }
        let folding = self.flag("fold", true);
        for (pos, delta) in self.events {
            for _ in 0..delta.unsigned_abs() {
                if delta > 0 {
                    result.fold_open(pos);
                } else {
                    result.fold_close(pos);
                }
            }
        }
        let mut result = result.finish();
        if folding {
            for (fold, indent) in result.fold_levels.iter_mut().zip(self.indent_levels) {
                fold.level = 0x400 + indent;
            }
        }
        result
    }
}

fn word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 128
}
fn start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 128
}
fn operator(c: u8) -> bool {
    b"+-*/%=<>!&|^~?:;,.()[]{}".contains(&c)
}
