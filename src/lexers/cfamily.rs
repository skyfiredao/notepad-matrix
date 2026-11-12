use crate::languages::LanguageDefinition;

use super::shared::{Builder, Cursor, keyword, property_bool, property_int};
use super::{LexError, LexResult};

mod folding;
mod preprocessor;

pub(super) fn lex(text: &str, profile: &LanguageDefinition) -> Result<LexResult, LexError> {
    Ok(match profile.key() {
        "json" | "json5" => json(text, profile),
        "d" => d(text, profile),
        "rust" => rust(text, profile),
        "avs" => avs(text, profile),
        "oscript" => oscript(text, profile),
        "escript" => escript(text, profile),
        "gui4cli" => gui4cli(text, profile),
        "baanc" => baan(text, profile),
        "objc" => objc(text, profile),
        _ => cpp(text, profile),
    })
}

fn word_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic() || !ch.is_ascii()
}

fn word_char(ch: char) -> bool {
    word_start(ch) || ch.is_ascii_digit()
}

fn ascii_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn operator(ch: char) -> bool {
    "%^&*()-+=|{}[]:;<>?,.!/~".contains(ch)
}

struct Scan<'a> {
    c: Cursor<'a>,
    b: Builder<'a>,
    p: &'a LanguageDefinition,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str, p: &'a LanguageDefinition) -> Self {
        let mut b = Builder::new(text);
        b.configure(p);
        Self {
            c: Cursor::new(text),
            b,
            p,
        }
    }

    fn ch(&self) -> char {
        self.c.peek().unwrap_or('\0')
    }

    fn next(&self) -> char {
        self.c.rest().chars().nth(1).unwrap_or('\0')
    }

    fn first(&self) -> bool {
        let start = self.c.text[..self.c.position]
            .rfind(['\r', '\n'])
            .map_or(0, |p| p + 1);
        self.c.text[start..self.c.position]
            .chars()
            .all(|ch| ch == ' ' || ch == '\t')
    }

    fn mark(&mut self, start: usize, style: u16) {
        self.b.span(start..self.c.position, style);
    }

    fn one(&mut self, style: u16) {
        let start = self.c.position;
        self.c.bump();
        self.mark(start, style);
    }

    fn block(&mut self, open: &str, close: &str, nested: bool, style: u16) {
        let start = self.c.position;
        self.c.position += open.len();
        let mut depth = 1u64;
        let fold = property_bool(self.p, "fold.comment", false);
        if fold {
            self.b.fold_open(start);
        }
        while !self.c.is_eof() {
            let position = self.c.position;
            if nested && self.c.starts_with(open) {
                depth += 1;
                self.c.position += open.len();
            } else if self.c.starts_with(close) {
                depth -= 1;
                self.c.position += close.len();
                if depth == 0 {
                    break;
                }
            } else {
                self.c.bump();
            }
            self.b
                .state_in(position..self.c.position, (depth << 16) | u64::from(style));
        }
        self.mark(start, style);
        if fold && depth == 0 {
            self.b.fold_close(self.c.position.saturating_sub(1));
        }
    }

    fn line(&mut self, style: u16, continuation: bool) {
        let start = self.c.position;
        while !self.c.is_eof() {
            if self.ch() == '\r' || self.ch() == '\n' {
                break;
            }
            if continuation && self.ch() == '\\' && matches!(self.next(), '\r' | '\n') {
                let at = self.c.position;
                self.c.bump();
                self.newline();
                self.b.state_in(at..self.c.position, u64::from(style));
            } else {
                self.c.bump();
            }
        }
        self.mark(start, style);
    }

    fn newline(&mut self) {
        if self.ch() == '\r' {
            self.c.bump();
            if self.ch() == '\n' {
                self.c.bump();
            }
        } else if self.ch() == '\n' {
            self.c.bump();
        }
    }

    fn quoted(
        &mut self,
        opening: (usize, char),
        style: u16,
        multiline: bool,
        escape: bool,
        doubled: bool,
        error: Option<u16>,
    ) {
        let (prefix, quote) = opening;
        let start = self.c.position;
        self.c.position += prefix;
        let mut overlays = Vec::new();
        let mut closed = false;
        while !self.c.is_eof() {
            let at = self.c.position;
            if self.ch() == quote {
                self.c.bump();
                if doubled && self.ch() == quote {
                    self.c.bump();
                } else {
                    closed = true;
                    break;
                }
            } else if escape && self.ch() == '\\' {
                self.c.bump();
                if matches!(self.ch(), '\r' | '\n') {
                    self.newline();
                    self.b.state_in(at..self.c.position, u64::from(style));
                } else {
                    let code = self.ch();
                    self.c.bump();
                    if style == 6 && property_bool(self.p, "lexer.cpp.escape.sequence", false) {
                        let count = match code {
                            'x' => 4,
                            'u' => 4,
                            'U' => 8,
                            '0'..='7' => 2,
                            _ => 0,
                        };
                        for _ in 0..count {
                            if if matches!(code, '0'..='7') {
                                matches!(self.ch(), '0'..='7')
                            } else {
                                self.ch().is_ascii_hexdigit()
                            } {
                                self.c.bump();
                            } else {
                                break;
                            }
                        }
                        overlays.push(at..self.c.position);
                    }
                }
            } else if matches!(self.ch(), '\r' | '\n') {
                if !multiline {
                    if error.is_some() {
                        self.newline();
                    }
                    break;
                }
                self.newline();
                self.b.state_in(at..self.c.position, u64::from(style));
            } else {
                self.c.bump();
            }
        }
        self.mark(
            start,
            if closed {
                style
            } else {
                error.unwrap_or(style)
            },
        );
        for range in overlays {
            self.b.span(range, 27);
        }
    }

    fn raw(&mut self, prefix: usize, end: &str, style: u16) {
        let start = self.c.position;
        self.c.position += prefix;
        let state = end.bytes().fold(u64::from(style), |hash, byte| {
            hash.wrapping_mul(131).wrapping_add(u64::from(byte))
        }) | (1 << 63);
        while !self.c.is_eof() && !self.c.starts_with(end) {
            let at = self.c.position;
            self.c.bump();
            self.b.state_in(at..self.c.position, state);
        }
        if self.c.starts_with(end) {
            self.c.position += end.len();
        }
        self.mark(start, style);
    }

    fn number(&mut self, style: u16) {
        let start = self.c.position;
        self.c.bump();
        while !self.c.is_eof() {
            let ch = self.ch();
            let previous = self.c.text[..self.c.position]
                .chars()
                .next_back()
                .unwrap_or('\0');
            if ch.is_ascii_alphanumeric()
                || ch == '_'
                || ch == '\''
                || (ch == '.' && self.next() != '.')
                || (matches!(ch, '+' | '-') && matches!(previous, 'e' | 'E' | 'p' | 'P'))
            {
                self.c.bump();
            } else {
                break;
            }
        }
        self.mark(start, style);
    }

    fn word(&mut self, predicate: impl Fn(char) -> bool) -> (usize, &'a str) {
        let start = self.c.position;
        self.c.bump();
        self.c.eat_while(predicate);
        (start, &self.c.text[start..self.c.position])
    }

    fn braces(&mut self, style: u16, enabled: bool) {
        if enabled {
            if self.ch() == '{' {
                self.b.fold_open(self.c.position);
            }
            if self.ch() == '}' {
                self.b.fold_close(self.c.position);
            }
        }
        self.one(style);
    }

    fn docs(&mut self, start: usize, end: usize, keyword_style: u16, error_style: u16) {
        let text = self.c.text;
        let mut c = Cursor::new(&text[start..end]);
        while !c.is_eof() {
            let previous = c.text[..c.position].chars().next_back().unwrap_or(' ');
            let boundary = previous.is_ascii_whitespace() || matches!(previous, '*' | '/' | '!');
            if matches!(c.peek(), Some('@' | '\\')) && boundary {
                let at = c.position;
                c.bump();
                c.eat_while(|ch| ch.is_ascii_lowercase() || "$@\\&<>#{}[]".contains(ch));
                let word = &c.text[at + 1..c.position];
                if !word.is_empty() {
                    let delimiter = c.peek().is_none_or(|ch| ch.is_ascii_whitespace());
                    let style = if delimiter && keyword(self.p, 2, word) {
                        keyword_style
                    } else {
                        error_style
                    };
                    self.b.span(start + at..start + c.position, style);
                }
            } else if self.p.lexer() == "cpp"
                && ((c.peek() == Some('<')
                    && c.rest()
                        .chars()
                        .nth(1)
                        .is_some_and(|ch| !ch.is_ascii_whitespace() && ch != '/'))
                    || (c.peek() == Some('/') && previous == '<'))
            {
                c.bump();
                let at = c.position;
                c.eat_while(|ch| ch.is_ascii_alphabetic() || matches!(ch, '/' | '_' | ':'));
                if c.peek() == Some('>') {
                    c.bump();
                }
                let style = if keyword(self.p, 2, &c.text[at..c.position]) {
                    keyword_style
                } else {
                    error_style
                };
                self.b.span(start + at..start + c.position, style);
            } else {
                c.bump();
            }
        }
    }

    fn tasks(&mut self, start: usize, end: usize) {
        let mut c = Cursor::new(&self.c.text[start..end]);
        while !c.is_eof() {
            if c.peek()
                .is_some_and(|ch| ch.is_ascii_whitespace() || operator(ch))
            {
                c.bump();
                continue;
            }
            let at = c.position;
            c.eat_while(|ch| !ch.is_ascii_whitespace() && !operator(ch));
            if keyword(self.p, 5, &c.text[at..c.position]) {
                self.b.span(start + at..start + c.position, 26);
            }
        }
    }
}

fn cpp(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    let backquoted = property_int(p, "lexer.cpp.backquoted.strings", 0);
    let mut templates: Vec<u32> = Vec::new();
    let mut template_text = false;
    let mut raw_terminator = String::new();
    let mut previous_word = "";
    let mut previous = ' ';
    let only_string_continuations =
        property_bool(p, "lexer.cpp.continuation.only.in.strings", false);
    let mut visible_on_logical_line = false;
    let mut include_directive = false;
    let mut uuid = false;
    while !s.c.is_eof() {
        let start = s.c.position;
        if template_text {
            let mut escapes = Vec::new();
            while !s.c.is_eof() {
                if s.c.starts_with("${") {
                    s.mark(start, 20);
                    if property_bool(p, "fold.cpp.syntax.based", true) {
                        s.b.fold_open(s.c.position + 1);
                    }
                    s.c.position += 2;
                    s.b.span(s.c.position - 2..s.c.position, 10);
                    templates.push(1);
                    template_text = false;
                    break;
                } else if s.c.starts_with(&raw_terminator) {
                    s.c.position += raw_terminator.len();
                    s.mark(start, 20);
                    template_text = false;
                    if templates.is_empty() {
                        raw_terminator.clear();
                    }
                    break;
                } else if s.ch() == '\\' {
                    let at = s.c.position;
                    s.c.bump();
                    let code = s.ch();
                    s.c.bump();
                    if property_bool(p, "lexer.cpp.escape.sequence", false)
                        && !matches!(code, '\r' | '\n')
                    {
                        let count = match code {
                            'U' => 8,
                            'u' | 'x' => 4,
                            '0'..='7' => 2,
                            _ => 0,
                        };
                        for _ in 0..count {
                            let valid = if matches!(code, '0'..='7') {
                                matches!(s.ch(), '0'..='7')
                            } else {
                                s.ch().is_ascii_hexdigit()
                            };
                            if valid {
                                s.c.bump();
                            } else {
                                break;
                            }
                        }
                        escapes.push(at..s.c.position);
                    }
                    s.b.state_in(at..s.c.position, 20 | (1 << 32));
                } else {
                    let at = s.c.position;
                    s.c.bump();
                    s.b.state_in(at..s.c.position, 20 | (1 << 32));
                }
            }
            s.mark(start, 20);
            for range in escapes {
                s.b.span(range, 27);
            }
            if !template_text && s.c.text[start..s.c.position].ends_with("${") {
                s.b.span(s.c.position - 2..s.c.position, 10);
            }
            continue;
        }
        if s.ch() == '\\' && matches!(s.next(), '\r' | '\n') && !only_string_continuations {
            s.c.bump();
            s.newline();
            s.b.state_in(start..s.c.position, 1 << 60);
            continue;
        }
        if s.ch().is_ascii_whitespace() {
            if matches!(s.ch(), '\r' | '\n') {
                visible_on_logical_line = false;
                include_directive = false;
                uuid = false;
            }
            s.c.bump();
            if !templates.is_empty() {
                s.b.state_in(start..s.c.position, (templates.len() as u64) << 32 | 10);
            }
            continue;
        }
        if s.c.starts_with("/*") {
            let doc = s.c.starts_with("/**") || s.c.starts_with("/*!");
            s.block("/*", "*/", false, if doc { 3 } else { 1 });
            if doc {
                s.docs(start, s.c.position, 17, 18);
            } else {
                s.tasks(start, s.c.position);
            }
            if text[start..s.c.position].contains(['\r', '\n']) {
                visible_on_logical_line = false;
                uuid = false;
            }
            continue;
        }
        if s.c.starts_with("//") {
            let doc =
                (s.c.starts_with("///") && !s.c.starts_with("////")) || s.c.starts_with("//!");
            s.line(if doc { 15 } else { 2 }, !only_string_continuations);
            s.newline();
            s.mark(start, if doc { 15 } else { 2 });
            visible_on_logical_line = false;
            uuid = false;
            if doc {
                s.docs(start, s.c.position, 17, 18);
            } else {
                s.tasks(start, s.c.position);
            }
            if property_bool(p, "fold.comment", false)
                && property_bool(p, "fold.cpp.comment.explicit", true)
            {
                if text[start..s.c.position].starts_with("//{") {
                    s.b.fold_open(start);
                }
                if text[start..s.c.position].starts_with("//}") {
                    s.b.fold_close(start);
                }
            }
            continue;
        }
        if s.ch() == '#'
            && !visible_on_logical_line
            && property_bool(p, "lexer.cpp.enable.preprocessor", true)
            && !(property_bool(p, "lexer.cpp.hashquoted.strings", false) && s.c.starts_with("#\""))
        {
            s.c.bump();
            s.c.eat_while(|ch| ch == ' ' || ch == '\t');
            let at = s.c.position;
            s.c.eat_while(|ch| ch.is_ascii_alphabetic());
            let directive = &text[at..s.c.position];
            include_directive = directive.starts_with("include");
            visible_on_logical_line = true;
            if property_bool(p, "fold.preprocessor", false) {
                if matches!(directive, "if" | "ifdef" | "ifndef" | "region") {
                    s.b.fold_open(start);
                }
                if matches!(directive, "endif" | "endregion") {
                    s.b.fold_close(start);
                }
            }
            if !property_bool(p, "styling.within.preprocessor", false) {
                let mut overlays = Vec::new();
                while !s.c.is_eof() && !matches!(s.ch(), '\r' | '\n') {
                    if s.c.starts_with("/*") {
                        let at = s.c.position;
                        let doc = s.c.starts_with("/**") || s.c.starts_with("/*!");
                        s.block("/*", "*/", false, if doc { 24 } else { 23 });
                        overlays.push((at..s.c.position, if doc { 24 } else { 23 }));
                    } else if s.c.starts_with("//") {
                        break;
                    } else if s.ch() == '"' {
                        s.quoted((1, '"'), 9, false, true, false, None);
                    } else if include_directive && s.ch() == '<' {
                        s.quoted((1, '>'), 9, false, false, false, None);
                    } else if s.ch() == '\\'
                        && matches!(s.next(), '\r' | '\n')
                        && !only_string_continuations
                    {
                        let at = s.c.position;
                        s.c.bump();
                        s.newline();
                        s.b.state_in(at..s.c.position, 9);
                    } else {
                        s.c.bump();
                    }
                }
                s.mark(start, 9);
                for (range, style) in overlays {
                    s.b.span(range, style);
                }
            } else {
                while !s.c.is_eof() {
                    if s.ch() == '\\'
                        && matches!(s.next(), '\r' | '\n')
                        && !only_string_continuations
                    {
                        let at = s.c.position;
                        s.c.bump();
                        s.newline();
                        s.b.state_in(at..s.c.position, 9);
                    } else if s.ch().is_ascii_whitespace() || s.ch() == '(' {
                        break;
                    } else {
                        s.c.bump();
                    }
                }
                s.mark(start, 9);
            }
            continue;
        }
        if uuid
            && (word_start(s.ch())
                || s.ch() == '@'
                || s.ch().is_ascii_digit()
                || (s.ch() == '.' && s.next().is_ascii_digit()))
        {
            uuid = false;
            while !s.c.is_eof() && !matches!(s.ch(), ')' | '\r' | '\n') {
                if s.ch() == '\\' && matches!(s.next(), '\r' | '\n') && !only_string_continuations {
                    let at = s.c.position;
                    s.c.bump();
                    s.newline();
                    s.b.state_in(at..s.c.position, 8);
                } else {
                    s.c.bump();
                }
            }
            s.mark(start, 8);
        } else if include_directive && s.ch() == '<' {
            s.quoted((1, '>'), 6, false, false, false, Some(12));
            include_directive = false;
        } else if s.c.starts_with("@\"") {
            s.quoted(
                (2, '"'),
                13,
                true,
                property_bool(p, "lexer.cpp.verbatim.strings.allow.escapes", false),
                true,
                None,
            );
        } else if property_bool(p, "lexer.cpp.triplequoted.strings", false)
            && s.c.starts_with("\"\"\"")
        {
            s.raw(3, "\"\"\"", 21);
            s.c.eat_while(|ch| ch == '"');
            s.mark(start, 21);
        } else if property_bool(p, "lexer.cpp.hashquoted.strings", false) && s.c.starts_with("#\"")
        {
            s.quoted((2, '"'), 22, true, true, false, None);
        } else if backquoted != 0 && s.ch() == '`' {
            if backquoted == 2 {
                s.one(20);
                raw_terminator = "`".into();
                template_text = true;
            } else {
                s.raw(1, "`", 20);
            }
        } else if raw_cpp_prefix(s.c.rest()).is_some() {
            if let Some((prefix, end)) = raw_cpp_prefix(s.c.rest()) {
                if backquoted == 2 {
                    s.c.position += prefix;
                    s.mark(start, 20);
                    raw_terminator = end;
                    template_text = true;
                } else {
                    s.raw(prefix, &end, 20);
                }
            }
        } else if s.ch() == '"' || s.ch() == '\'' || cpp_string_prefix(s.c.rest()).is_some() {
            include_directive = false;
            let (prefix, quote) = cpp_string_prefix(s.c.rest()).unwrap_or((1, s.ch()));
            s.quoted(
                (prefix, quote),
                if quote == '"' { 6 } else { 7 },
                false,
                true,
                false,
                Some(12),
            );
            if s.ch() == '_' {
                s.c.eat_while(word_char);
                s.mark(start, 25);
            }
        } else if s.ch().is_ascii_digit() || (s.ch() == '.' && s.next().is_ascii_digit()) {
            cpp_number(&mut s);
        } else if word_start(s.ch())
            || s.ch() == '@'
            || (s.ch() == '$' && property_bool(p, "lexer.cpp.allow.dollars", true))
            || (s.ch() == '#' && property_bool(p, "lexer.cpp.allow.hashes", false))
        {
            let (at, _) = s.word(|ch| {
                word_char(ch)
                    || (ch == '$' && property_bool(p, "lexer.cpp.allow.dollars", true))
                    || (ch == '#' && property_bool(p, "lexer.cpp.allow.hashes", false))
            });
            while s.ch() == '\\' && matches!(s.next(), '\r' | '\n') && !only_string_continuations {
                let at = s.c.position;
                s.c.bump();
                s.newline();
                s.b.state_in(at..s.c.position, 11);
                s.c.eat_while(|ch| {
                    word_char(ch)
                        || (ch == '$' && property_bool(p, "lexer.cpp.allow.dollars", true))
                        || (ch == '#' && property_bool(p, "lexer.cpp.allow.hashes", false))
                });
            }
            let word = &text[at..s.c.position];
            let style = if word.starts_with('@') {
                11
            } else if keyword(p, 0, word) {
                uuid = word == "uuid";
                5
            } else if keyword(p, 1, word) {
                16
            } else if keyword(p, 3, word) {
                19
            } else {
                11
            };
            s.mark(at, style);
            previous_word = word;
        } else if s.ch() == '/'
            && ("(=,:;!&|?{}[+-*%^~".contains(previous) || previous_word == "return")
        {
            let mut probe = Cursor::new(s.c.rest());
            probe.bump();
            let mut class = false;
            let mut closed = false;
            while !probe.is_eof() && !matches!(probe.peek(), Some('\r' | '\n')) {
                match probe.bump() {
                    Some('\\') => {
                        probe.bump();
                    }
                    Some('[') => class = true,
                    Some(']') => class = false,
                    Some('/') if !class => {
                        closed = true;
                        break;
                    }
                    _ => {}
                }
            }
            if closed {
                probe.eat_while(|ch| ch.is_ascii_lowercase());
                s.c.position += probe.position;
                s.mark(start, 14);
            } else {
                s.one(10);
            }
        } else if operator(s.ch()) {
            let ch = s.ch();
            if let Some(depth) = templates.last_mut() {
                if ch == '{' {
                    *depth += 1;
                }
                if ch == '}' {
                    *depth -= 1;
                    if *depth == 0 {
                        templates.pop();
                        template_text = true;
                    }
                }
            }
            s.braces(10, property_bool(p, "fold.cpp.syntax.based", true));
            previous_word = "";
        } else {
            s.c.bump();
        }
        previous = text[..s.c.position].chars().next_back().unwrap_or(' ');
        visible_on_logical_line = !matches!(previous, '\r' | '\n');
        if !templates.is_empty() {
            s.b.state_in(start..s.c.position, (templates.len() as u64) << 32 | 10);
        }
    }
    let mut result = s.b.finish();
    preprocessor::apply(text, p, &mut result);
    folding::apply(text, p, &mut result);
    result
}

fn raw_cpp_prefix(rest: &str) -> Option<(usize, String)> {
    let prefix = ["R\"", "u8R\"", "uR\"", "UR\"", "LR\""]
        .iter()
        .find(|prefix| rest.starts_with(**prefix))?;
    let after = &rest[prefix.len()..];
    if after
        .chars()
        .next()
        .is_some_and(|ch| " )\\\t\u{b}\u{c}\n".contains(ch))
    {
        return None;
    }
    let length = after.find('(').unwrap_or(after.len());
    Some((prefix.len(), format!("){}\"", &after[..length])))
}

fn cpp_number(s: &mut Scan<'_>) {
    let start = s.c.position;
    s.c.bump();
    let mut literal = false;
    while !s.c.is_eof() {
        if s.ch() == '\\'
            && matches!(s.next(), '\r' | '\n')
            && !property_bool(s.p, "lexer.cpp.continuation.only.in.strings", false)
        {
            let at = s.c.position;
            s.c.bump();
            s.newline();
            s.b.state_in(at..s.c.position, if literal { 25 } else { 4 });
            continue;
        }
        if s.ch() == '_' {
            literal = true;
        }
        let previous = s.c.text[..s.c.position].chars().next_back().unwrap_or('\0');
        if word_char(s.ch())
            || s.ch() == '.'
            || (s.ch() == '$' && property_bool(s.p, "lexer.cpp.allow.dollars", true))
            || (s.ch() == '#' && property_bool(s.p, "lexer.cpp.allow.hashes", false))
            || (!literal
                && (s.ch() == '\''
                    || (matches!(previous, 'e' | 'E' | 'p' | 'P') && matches!(s.ch(), '+' | '-'))))
        {
            s.c.bump();
        } else {
            break;
        }
    }
    s.mark(start, if literal { 25 } else { 4 });
}

fn cpp_string_prefix(rest: &str) -> Option<(usize, char)> {
    for prefix in ["u8", "L", "u", "U"] {
        if let Some(tail) = rest.strip_prefix(prefix)
            && let Some(quote @ ('"' | '\'')) = tail.chars().next()
            && (prefix != "u8" || quote == '"')
        {
            return Some((prefix.len() + 1, quote));
        }
    }
    None
}

fn objc(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    let mut previous = ' ';
    while !s.c.is_eof() {
        let start = s.c.position;
        if s.ch().is_ascii_whitespace() {
            s.c.bump();
            continue;
        }
        if s.c.starts_with("/*") {
            let doc = s.c.starts_with("/**") || s.c.starts_with("/*!");
            s.block("/*", "*/", false, if doc { 3 } else { 1 });
            if doc {
                s.docs(start, s.c.position, 17, 18);
            }
        } else if s.c.starts_with("//") {
            let doc = s.c.starts_with("///") || s.c.starts_with("//!");
            s.line(if doc { 15 } else { 2 }, true);
        } else if s.ch() == '#' && s.first() {
            s.c.bump();
            s.c.eat_while(|ch| ch == ' ' || ch == '\t');
            let at = s.c.position;
            s.c.eat_while(|ch| ch.is_ascii_alphabetic());
            let directive = &text[at..s.c.position];
            if property_bool(p, "fold.preprocessor", false) {
                if matches!(directive, "if" | "ifdef" | "ifndef") {
                    s.b.fold_open(start);
                }
                if directive == "endif" {
                    s.b.fold_close(start);
                }
            }
            if !property_bool(p, "styling.within.preprocessor", false) {
                s.line(9, true);
            }
            s.mark(start, 9);
        } else if s.c.starts_with("@\"") {
            s.quoted((2, '"'), 13, true, false, true, None);
        } else if matches!(s.ch(), '"' | '\'') {
            let quote = s.ch();
            s.quoted(
                (1, quote),
                if quote == '"' { 6 } else { 7 },
                false,
                true,
                false,
                Some(12),
            );
        } else if s.ch().is_ascii_digit() || (s.ch() == '.' && s.next().is_ascii_digit()) {
            s.c.bump();
            s.c.eat_while(|ch| ascii_word(ch) || ch == '.');
            s.mark(start, 4);
        } else if s.ch().is_ascii_alphabetic() || matches!(s.ch(), '_' | '@') {
            let (at, word) = s.word(ascii_word);
            let style = if let Some(directive) = word.strip_prefix('@') {
                if keyword(p, 3, directive) { 19 } else { 11 }
            } else if keyword(p, 0, word) {
                5
            } else if keyword(p, 1, word) {
                16
            } else if keyword(p, 4, word) {
                20
            } else {
                11
            };
            s.mark(at, style);
            if style == 19 {
                if matches!(word, "@interface" | "@implementation" | "@protocol") {
                    s.b.fold_open(at);
                }
                if word == "@end" {
                    s.b.fold_close(at);
                }
            }
        } else if s.ch() == '/' && "(=,".contains(previous) {
            s.c.bump();
            while !s.c.is_eof() && !matches!(s.ch(), '\r' | '\n' | '/') {
                if s.ch() == '\\' {
                    s.c.bump();
                }
                s.c.bump();
            }
            if s.ch() == '/' {
                s.c.bump();
            }
            s.mark(start, 14);
        } else if operator(s.ch()) {
            s.braces(10, true);
        } else {
            s.c.bump();
        }
        previous = text[..s.c.position].chars().next_back().unwrap_or(' ');
    }
    s.b.finish()
}

fn json(text: &str, p: &LanguageDefinition) -> LexResult {
    let defaults = p.keywords(0).split_whitespace().collect::<Vec<_>>();
    let exact = !property_bool(p, "lexer.json.allow.comments", false)
        && property_bool(p, "lexer.json.escape.sequence", false)
        && defaults.len() == 3
        && ["true", "false", "null"]
            .iter()
            .all(|word| defaults.contains(word))
        && p.keywords(1).split_whitespace().collect::<Vec<_>>()
            == [
                "@id",
                "@context",
                "@type",
                "@value",
                "@language",
                "@container",
                "@list",
                "@set",
                "@reverse",
                "@index",
                "@base",
                "@vocab",
                "@graph",
            ];
    let mut b = Builder::new(text);
    b.configure(p);
    let spans = if exact {
        crate::syntax::json_styles(text)
            .into_iter()
            .map(|span| super::Span {
                range: span.range,
                style: span.style as u16,
            })
            .collect()
    } else {
        json_configured(text, p)
    };
    for span in spans {
        b.span(span.range.clone(), span.style);
        if span.style == 7 {
            b.state_in(span.range.clone(), 7);
        }
        if span.style == 8 {
            for (offset, ch) in text[span.range.clone()].char_indices() {
                if ch == '{' || ch == '[' {
                    b.fold_open(span.range.start + offset);
                }
                if ch == '}' || ch == ']' {
                    b.fold_close(span.range.start + offset);
                }
            }
        }
    }
    b.finish()
}

struct JsonMachine<'a> {
    text: &'a str,
    pos: usize,
    start: usize,
    previous: char,
    state: u16,
    spans: Vec<super::Span>,
}

impl JsonMachine<'_> {
    fn ch(&self) -> char {
        self.text
            .get(self.pos..)
            .and_then(|tail| tail.chars().next())
            .unwrap_or('\0')
    }
    fn next(&self) -> char {
        self.text
            .get(self.pos..)
            .and_then(|tail| tail.chars().nth(1))
            .unwrap_or('\0')
    }
    fn starts(&self, value: &str) -> bool {
        self.text
            .get(self.pos..)
            .is_some_and(|tail| tail.starts_with(value))
    }
    fn line_start(&self) -> bool {
        self.pos == 0 || self.previous == '\n' || (self.previous == '\r' && self.ch() != '\n')
    }
    fn line_end(&self) -> bool {
        self.pos >= self.text.len()
            || self.ch() == '\n'
            || (self.ch() == '\r' && self.next() != '\n')
    }
    fn match_end(&self) -> bool {
        self.pos >= self.text.len()
            || self.ch() == '\r'
            || (self.ch() == '\n' && self.previous != '\r')
    }
    fn word(&self, predicate: fn(char) -> bool) -> &str {
        let length = self
            .text
            .get(self.pos..)
            .unwrap_or_default()
            .chars()
            .take(50)
            .take_while(|ch| predicate(*ch))
            .map(char::len_utf8)
            .sum::<usize>();
        &self.text[self.pos.min(self.text.len())..self.pos.min(self.text.len()) + length]
    }
    fn flush(&mut self) {
        let end = self.pos.min(self.text.len());
        if self.start < end {
            self.spans.push(super::Span {
                range: self.start..end,
                style: self.state,
            });
        }
        self.start = end;
    }
    fn set(&mut self, state: u16) {
        self.flush();
        self.state = state;
    }
    fn advance(&mut self) {
        self.previous = self.ch();
        self.pos += self.previous.len_utf8();
    }
    fn advance_set(&mut self, state: u16) {
        self.advance();
        self.set(state);
    }
}

fn json_space(ch: char) -> bool {
    ch == ' ' || ('\t'..='\r').contains(&ch)
}
fn json_operator(ch: char) -> bool {
    "[{}]:,".contains(ch)
}
fn json_word(ch: char) -> bool {
    ch.is_ascii_alphabetic() || "$_".contains(ch)
}
fn json_ld(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ":@".contains(ch)
}
fn json_url(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,),=".contains(ch)
}
fn json_escape(next: char, digits: &mut i32) -> bool {
    *digits = if next == 'u' { 5 } else { 0 };
    "\\\"tnbfru/".contains(next)
}

fn json_property(text: &str, start: usize) -> bool {
    let bytes = text.as_bytes();
    let mut escaped = false;
    for offset in 1..=100 {
        let ch = bytes.get(start + offset).copied().unwrap_or(0);
        if escaped {
            escaped = false;
            continue;
        }
        escaped = ch == b'\\';
        if ch == b'"' {
            for gap in 1..=50 {
                let at = start + offset + gap;
                let ch = bytes.get(at).copied().unwrap_or(0);
                let next = bytes.get(at + 1).copied().unwrap_or(0);
                if ch == b':' {
                    return true;
                }
                if !json_space(char::from(ch)) || ch == b'\n' || (ch == b'\r' && next != b'\n') {
                    return false;
                }
            }
            return false;
        }
        if ch == 0 {
            return false;
        }
    }
    false
}

fn json_configured(text: &str, p: &LanguageDefinition) -> Vec<super::Span> {
    let mut c = JsonMachine {
        text,
        pos: 0,
        start: 0,
        previous: '\0',
        state: 0,
        spans: Vec::new(),
    };
    let escapes = property_bool(p, "lexer.json.escape.sequence", false);
    let comments = property_bool(p, "lexer.json.allow.comments", false);
    let mut before = 2;
    let mut digits = 0;
    let mut colons = 0;
    let mut invalid = false;
    while c.pos <= text.len() {
        match c.state {
            7 => {
                if c.starts("*/") {
                    c.advance();
                    c.advance_set(0);
                }
            }
            6 => {
                if c.match_end() {
                    c.set(0);
                }
            }
            3 => {
                if c.line_start() {
                    c.set(0);
                }
            }
            5 => {
                digits -= 1;
                if digits > 0 {
                    if !c.ch().is_ascii_hexdigit() {
                        c.set(13);
                    }
                } else if c.ch() == '"' {
                    c.set(before);
                    c.advance_set(0);
                } else if c.ch() == '\\' {
                    if !json_escape(c.next(), &mut digits) {
                        c.set(13);
                    }
                    c.advance();
                } else {
                    c.set(before);
                    if c.line_end() {
                        c.state = 3;
                    }
                }
            }
            2 | 4 => {
                if c.ch() == '"' {
                    if !invalid && colons == 1 {
                        c.state = 10;
                    }
                    c.advance_set(0);
                    colons = 0;
                    invalid = false;
                } else if c.line_end() {
                    c.state = 3;
                } else if c.ch() == '\\' {
                    if escapes {
                        before = c.state;
                        c.set(5);
                        if !json_escape(c.next(), &mut digits) {
                            c.set(13);
                        }
                    }
                    c.advance();
                } else if [
                    "https://", "http://", "ssh://", "git://", "svn://", "ftp://", "mailto:",
                ]
                .iter()
                .any(|prefix| c.starts(prefix))
                {
                    before = c.state;
                    c.set(9);
                } else if c.ch() == '@' {
                    if keyword(p, 1, c.word(json_ld)) {
                        before = c.state;
                        c.set(12);
                    }
                } else if c.ch() == ':' {
                    colons += 1;
                } else {
                    invalid |= !(c.ch().is_ascii_alphabetic() || "$_-".contains(c.ch()));
                }
            }
            9 | 12 => {
                if (c.state == 12 && !json_ld(c.ch())) || !json_url(c.ch()) {
                    c.set(before);
                }
                if c.ch() == '"' {
                    c.advance_set(0);
                } else if c.ch() == '\\' {
                    if escapes {
                        c.set(5);
                        if !json_escape(c.next(), &mut digits) {
                            c.set(13);
                        }
                    }
                    c.advance();
                } else if c.line_end() {
                    c.state = 3;
                }
            }
            1 | 8 => c.set(0),
            13 => {
                if c.match_end() {
                    c.set(0);
                }
            }
            11 if !json_word(c.ch()) => c.set(0),
            _ => {}
        }
        if c.state == 0 {
            if c.ch() == '"' {
                colons = 0;
                invalid = false;
                c.set(if json_property(text, c.pos) { 4 } else { 2 });
            } else if json_operator(c.ch()) {
                c.set(8);
            } else if comments && c.starts("/*") {
                c.set(7);
                c.advance();
            } else if comments && c.starts("//") {
                c.set(6);
            } else if json_word(c.ch()) && keyword(p, 0, c.word(json_word)) {
                c.set(11);
            }
            let ch = c.ch();
            let prev = c.previous;
            let next = c.next();
            let number = ch.is_ascii_digit()
                && (matches!(prev, '+' | '-')
                    || c.line_start()
                    || json_space(prev)
                    || json_operator(prev));
            let exponent = ch.eq_ignore_ascii_case(&'e')
                && prev.is_ascii_digit()
                && (next.is_ascii_digit() || matches!(next, '+' | '-'));
            let sign = matches!(ch, '-' | '+')
                && ((prev.eq_ignore_ascii_case(&'e') && next.is_ascii_digit())
                    || ((json_space(prev) || json_operator(prev)) && next.is_ascii_digit()));
            let adjacent = ch.is_ascii_digit()
                && (prev.is_ascii_digit() || prev.eq_ignore_ascii_case(&'e') || prev == '.');
            let dot = ch == '.' && prev.is_ascii_digit() && next.is_ascii_digit();
            if number || exponent || sign || adjacent || dot {
                c.set(1);
            } else if c.state == 0 && !json_space(ch) {
                c.set(13);
            }
        }
        c.advance();
    }
    c.flush();
    c.spans
}

fn d(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    while !s.c.is_eof() {
        let start = s.c.position;
        if s.c.starts_with("/+") {
            s.block("/+", "+/", true, 4);
        } else if s.c.starts_with("/*") {
            let doc = s.c.starts_with("/**") || s.c.starts_with("/*!");
            s.block("/*", "*/", false, if doc { 3 } else { 1 });
            if doc {
                s.docs(start, s.c.position, 16, 17);
            }
        } else if s.c.starts_with("//") {
            let doc =
                (s.c.starts_with("///") && !s.c.starts_with("////")) || s.c.starts_with("//!");
            s.line(if doc { 15 } else { 2 }, false);
            if doc {
                s.docs(start, s.c.position, 16, 17);
            }
        } else if matches!(s.ch(), 'r' | 'x' | 'q') && s.next() == '"' {
            s.raw(2, "\"", 19);
            if matches!(s.ch(), 'c' | 'w' | 'd') {
                s.one(19);
            }
        } else if s.ch() == '`' {
            s.raw(1, "`", 18);
            if matches!(s.ch(), 'c' | 'w' | 'd') {
                s.one(18);
            }
        } else if s.ch() == '"' {
            s.quoted((1, '"'), 10, true, true, false, None);
            if matches!(s.ch(), 'c' | 'w' | 'd') {
                s.one(10);
            }
        } else if s.ch() == '\'' {
            s.quoted((1, '\''), 12, false, true, false, Some(11));
        } else if s.ch().is_ascii_digit() || (s.ch() == '.' && s.next().is_ascii_digit()) {
            s.number(5);
        } else if word_start(s.ch()) || s.ch() == '$' {
            let (at, word) = s.word(word_char);
            let style = [(0, 6), (1, 7), (3, 9), (4, 20), (5, 21), (6, 22)]
                .into_iter()
                .find(|(index, _)| keyword(p, *index, word))
                .map_or(14, |(_, style)| style);
            s.mark(at, style);
        } else if s.c.starts_with("..") {
            s.c.position += 2;
            s.mark(start, 13);
        } else if operator(s.ch()) {
            s.braces(13, property_bool(p, "fold.d.syntax.based", true));
        } else {
            s.c.bump();
        }
    }
    s.b.finish()
}

fn rust(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    while !s.c.is_eof() {
        let start = s.c.position;
        if start == 0 && s.c.starts_with("#!") && !s.c.starts_with("#![") {
            s.line(2, false);
        } else if s.c.starts_with("/*") {
            let doc =
                (s.c.starts_with("/**") && !s.c.starts_with("/***")) || s.c.starts_with("/*!");
            s.block("/*", "*/", true, if doc { 3 } else { 1 });
        } else if s.c.starts_with("//") {
            let doc =
                (s.c.starts_with("///") && !s.c.starts_with("////")) || s.c.starts_with("//!");
            s.line(if doc { 4 } else { 2 }, false);
        } else if let Some((prefix, end, style)) = rust_raw(s.c.rest()) {
            s.raw(prefix, &end, style);
        } else if s.c.starts_with("r#") && s.c.rest()[2..].chars().next().is_some_and(word_start) {
            s.c.position += 2;
            s.c.eat_while(word_char);
            s.mark(start, 17);
        } else if s.c.starts_with("r#") || s.c.starts_with("br#") || s.c.starts_with("cr#") {
            s.c.position += if s.ch() == 'r' { 1 } else { 2 };
            s.c.eat_while(|ch| ch == '#');
            s.mark(start, 20);
        } else if s.c.starts_with("b\"") || s.c.starts_with("c\"") || s.ch() == '"' {
            let (prefix, style) = if s.ch() == 'b' {
                (2, 21)
            } else if s.ch() == 'c' {
                (2, 24)
            } else {
                (1, 13)
            };
            rust_string(&mut s, prefix, style);
        } else if s.ch() == '\'' || s.c.starts_with("b'") {
            rust_character(&mut s);
        } else if word_start(s.ch()) {
            let (at, word) = s.word(word_char);
            let mut style = (0..7)
                .find(|index| keyword(p, *index, word))
                .map_or(17, |index| 6 + index as u16);
            if s.ch() == '!' && s.next() != '=' {
                s.c.bump();
                style = 19;
            }
            s.mark(at, style);
        } else if s.ch().is_ascii_digit() {
            rust_number(&mut s);
        } else if s.c.starts_with("..") {
            s.c.position += 2;
            s.mark(start, 16);
        } else if operator(s.ch()) || matches!(s.ch(), '#' | '@' | '$') {
            s.braces(16, property_bool(p, "fold.rust.syntax.based", true));
        } else if s.ch().is_ascii_whitespace() {
            s.c.bump();
        } else {
            s.one(20);
        }
    }
    s.b.finish()
}

fn rust_raw(rest: &str) -> Option<(usize, String, u16)> {
    let (offset, style) = if rest.starts_with("br") {
        (2, 22)
    } else if rest.starts_with("cr") {
        (2, 25)
    } else if rest.starts_with('r') {
        (1, 14)
    } else {
        return None;
    };
    let hashes = rest[offset..]
        .bytes()
        .take_while(|byte| *byte == b'#')
        .count();
    if rest.as_bytes().get(offset + hashes) != Some(&b'"') {
        return None;
    }
    Some((
        offset + hashes + 1,
        format!("\"{}", "#".repeat(hashes)),
        style,
    ))
}

fn rust_escape(s: &mut Scan<'_>, byte: bool, string: bool) -> bool {
    s.c.bump();
    let code = s.ch();
    if "nrt\\'\"0".contains(code) || (string && matches!(code, '\r' | '\n')) {
        s.c.bump();
        return true;
    }
    if code == 'x' || (!byte && matches!(code, 'u' | 'U')) {
        s.c.bump();
        if code == 'u' && s.ch() == '{' {
            s.c.bump();
            let start = s.c.position;
            s.c.eat_while(|ch| ch.is_ascii_hexdigit());
            let count = s.c.position - start;
            if count > 0 && count <= 6 && s.ch() == '}' {
                s.c.bump();
                return true;
            }
            return false;
        }
        let count = match code {
            'x' => 2,
            'u' => 4,
            _ => 8,
        };
        for _ in 0..count {
            if s.ch().is_ascii_hexdigit() {
                s.c.bump();
            } else {
                return false;
            }
        }
        if !string && s.ch().is_ascii_hexdigit() {
            s.c.eat_while(|ch| ch.is_ascii_hexdigit());
            return false;
        }
        return true;
    }
    false
}

fn rust_string(s: &mut Scan<'_>, prefix: usize, style: u16) {
    let start = s.c.position;
    s.c.position += prefix;
    while !s.c.is_eof() && s.ch() != '"' {
        let at = s.c.position;
        if s.ch() == '\\' {
            if !rust_escape(s, style == 21, true) {
                break;
            }
        } else if style == 21 && !s.ch().is_ascii() {
            break;
        } else {
            s.c.bump();
        }
        s.b.state_in(at..s.c.position, u64::from(style));
    }
    if s.ch() == '"' {
        s.c.bump();
    }
    s.mark(start, style);
}

fn rust_character(s: &mut Scan<'_>) {
    let start = s.c.position;
    let byte = s.ch() == 'b';
    s.c.position += if byte { 2 } else { 1 };
    let lifetime = !byte && word_start(s.ch());
    let mut valid = true;
    let mut first = true;
    while !s.c.is_eof() {
        let ch = s.ch();
        if ch == '\\' {
            valid = rust_escape(s, byte, false);
            break;
        }
        if ch == '\'' {
            valid = !first;
            break;
        }
        if matches!(ch, '\t' | '\r' | '\n') || (byte && !ch.is_ascii()) {
            valid = false;
            break;
        }
        if !word_char(ch) && !first {
            break;
        }
        s.c.bump();
        first = false;
    }
    if s.ch() == '\'' {
        if valid {
            s.c.bump();
            s.mark(start, if byte { 23 } else { 15 });
        } else {
            s.mark(start, 20);
        }
    } else {
        s.mark(start, if lifetime { 18 } else { 20 });
    }
}

fn rust_digits(s: &mut Scan<'_>, base: u32) -> bool {
    let start = s.c.position;
    s.c.eat_while(|ch| ch.is_ascii() && (ch.is_digit(base) || ch == '_'));
    s.c.position > start
}

fn rust_number(s: &mut Scan<'_>) {
    let start = s.c.position;
    let base = if s.c.starts_with("0x") {
        s.c.position += 2;
        16
    } else if s.c.starts_with("0b") {
        s.c.position += 2;
        2
    } else if s.c.starts_with("0o") {
        s.c.position += 2;
        8
    } else {
        10
    };
    let mut error = !rust_digits(s, base);
    if matches!(s.ch(), 'u' | 'i') {
        s.c.bump();
        if let Some(suffix) = ["8", "16", "32", "64", "128", "size"]
            .iter()
            .find(|suffix| s.c.starts_with(suffix))
        {
            s.c.position += suffix.len();
        } else {
            error = true;
        }
    } else if !error {
        if s.ch() == '.' && !word_start(s.next()) && s.next() != '.' {
            error |= base != 10;
            s.c.bump();
            rust_digits(s, 10);
        }
        if matches!(s.ch(), 'e' | 'E') {
            error |= base != 10;
            s.c.bump();
            if matches!(s.ch(), '-' | '+') {
                s.c.bump();
            }
            error |= !rust_digits(s, 10);
        }
        if s.ch() == 'f' {
            error |= base != 10;
            s.c.bump();
            if s.c.starts_with("32") || s.c.starts_with("64") {
                s.c.position += 2;
            } else {
                error = true;
            }
        }
    }
    s.mark(start, if error { 20 } else { 5 });
}

fn avs(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    while !s.c.is_eof() {
        if s.c.starts_with("/*") {
            s.block("/*", "*/", true, 1);
        } else if s.c.starts_with("[*") {
            s.block("[*", "*]", true, 2);
        } else if s.ch() == '#' {
            s.line(3, false);
        } else if s.c.starts_with("\"\"\"") {
            s.raw(3, "\"\"\"", 8);
        } else if s.ch() == '"' {
            s.quoted((1, '"'), 7, true, false, false, None);
        } else if s.ch().is_ascii_digit() || (s.ch() == '.' && s.next().is_ascii_digit()) {
            let start = s.c.position;
            s.c.bump();
            s.c.eat_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+'));
            s.mark(start, 4);
        } else if s.ch().is_ascii_alphabetic() || s.ch() == '_' {
            let (start, word) = s.word(ascii_word);
            let lowered = word.to_ascii_lowercase();
            let style = (0..6)
                .find(|index| keyword(p, *index, &lowered))
                .map_or(6, |index| 9 + index as u16);
            s.mark(start, style);
        } else if operator(s.ch()) {
            s.braces(5, true);
        } else {
            s.c.bump();
        }
    }
    s.b.finish()
}

fn oscript(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    while !s.c.is_eof() {
        let start = s.c.position;
        if s.c.starts_with("/*") {
            s.block("/*", "*/", false, 2);
        } else if s.c.starts_with("//") {
            s.line(1, false);
        } else if s.first() && s.ch() == '#' {
            if s.c
                .rest()
                .get(..10)
                .is_some_and(|value| value.eq_ignore_ascii_case("#ifdef doc"))
            {
                s.line(3, false);
                while !s.c.is_eof() {
                    let at = s.c.position;
                    s.newline();
                    s.b.state_in(at..s.c.position, 3);
                    let end = s.c.rest().trim_start_matches([' ', '\t']).starts_with("#e");
                    s.line(3, false);
                    if end {
                        break;
                    }
                }
                s.mark(start, 3);
            } else {
                s.line(4, false);
            }
        } else if matches!(s.ch(), '\'' | '"') {
            let quote = s.ch();
            s.quoted(
                (1, quote),
                if quote == '\'' { 6 } else { 7 },
                false,
                false,
                true,
                None,
            );
        } else if s.ch() == '$' {
            s.c.bump();
            if s.ch() == '$' {
                s.c.bump();
            }
            s.c.eat_while(ascii_word);
            s.mark(start, 10);
        } else if s.ch() == '.' && (s.next().is_ascii_alphabetic() || s.next() == '_') {
            s.one(12);
            let (at, _) = s.word(ascii_word);
            s.mark(at, if s.ch() == '(' { 18 } else { 17 });
        } else if s.ch().is_ascii_alphabetic() || s.ch() == '_' {
            let first = s.first();
            let (at, word) = s.word(ascii_word);
            let lower = word.to_ascii_lowercase();
            let style = if first && s.ch() == ':' {
                s.c.bump();
                13
            } else if keyword(p, 0, &lower) {
                11
            } else if keyword(p, 2, &lower) {
                12
            } else if s.ch() == '(' {
                if keyword(p, 4, &lower) { 15 } else { 18 }
            } else if s.ch() == '.' && keyword(p, 5, &lower) {
                16
            } else if keyword(p, 1, &lower) {
                8
            } else if keyword(p, 3, &lower) {
                14
            } else if keyword(p, 4, &lower) {
                15
            } else {
                9
            };
            s.mark(at, style);
            if style == 11 {
                if matches!(
                    lower.as_str(),
                    "if" | "for" | "switch" | "function" | "while" | "repeat"
                ) {
                    s.b.fold_open(at);
                }
                if matches!(lower.as_str(), "end" | "until") {
                    s.b.fold_close(at);
                }
            }
        } else if s.ch().is_ascii_digit()
            || (matches!(s.ch(), '.' | '-' | '+') && s.next().is_ascii_digit())
        {
            s.c.bump();
            while s.ch().is_ascii_digit()
                || matches!(s.ch(), 'e' | 'E' | '.')
                || (matches!(s.ch(), '-' | '+') && matches!(s.next(), 'e' | 'E'))
            {
                s.c.bump();
            }
            s.mark(start, 5);
        } else if operator(s.ch()) || s.ch() == '\\' {
            s.one(12);
        } else {
            s.c.bump();
        }
    }
    s.b.finish()
}

fn escript(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    while !s.c.is_eof() {
        if s.c.starts_with("/*") {
            s.block("/*", "*/", false, 1);
        } else if s.c.starts_with("//") {
            s.line(2, true);
        } else if s.ch() == '"' {
            s.quoted((1, '"'), 6, true, true, false, None);
        } else if s.ch().is_ascii_digit() || (s.ch() == '.' && s.next().is_ascii_digit()) {
            s.one(4);
        } else if s.ch().is_ascii_alphabetic() || matches!(s.ch(), '_' | '#') {
            let (start, word) = s.word(ascii_word);
            let lowered = word.to_ascii_lowercase();
            let word = if property_bool(p, "escript.case.sensitive", false) {
                word
            } else {
                &lowered
            };
            let style = [(0, 5), (1, 10), (2, 11)]
                .into_iter()
                .find(|(index, _)| keyword(p, *index, word))
                .map_or(8, |(_, style)| style);
            s.mark(start, style);
            if style == 11 {
                if matches!(
                    lowered.as_str(),
                    "for" | "foreach" | "program" | "function" | "while" | "case" | "if"
                ) {
                    s.b.fold_open(start);
                }
                if matches!(
                    lowered.as_str(),
                    "endfor"
                        | "endforeach"
                        | "endprogram"
                        | "endfunction"
                        | "endwhile"
                        | "endcase"
                        | "endif"
                ) {
                    s.b.fold_close(start);
                }
            }
        } else if matches!(s.ch(), '{' | '}') {
            s.one(9);
        } else if "+-*/=<>&|!?:".contains(s.ch()) {
            s.one(7);
        } else {
            s.c.bump();
        }
    }
    s.b.finish()
}

fn gui4cli(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    let mut first = true;
    while !s.c.is_eof() {
        if s.c.starts_with("/*") {
            s.block("/*", "*/", false, 2);
            first = false;
        } else if s.c.starts_with("//") {
            s.line(1, false);
        } else if matches!(s.ch(), '\r' | '\n') {
            s.newline();
            first = true;
        } else if s.ch() == ' ' || s.ch() == '\t' {
            s.c.bump();
        } else if matches!(s.ch(), '\'' | '"') {
            let start = s.c.position;
            let quote = s.ch();
            s.quoted((1, quote), 8, true, true, false, None);
            let mut c = Cursor::new(&text[start..s.c.position]);
            while !c.is_eof() {
                let at = c.position;
                if c.bump() == Some('\\') {
                    c.bump();
                    s.b.span(start + at..start + c.position, 9);
                }
            }
            first = false;
        } else if first && (ascii_word(s.ch()) || matches!(s.ch(), '.' | '\\')) {
            let (start, word) = s.word(|ch| ascii_word(ch) || matches!(ch, '.' | '\\'));
            let upper = word.to_ascii_uppercase();
            let style = [(0, 3), (2, 5), (3, 6), (4, 7), (1, 4)]
                .into_iter()
                .find(|(index, _)| keyword(p, *index, &upper))
                .map_or(0, |(_, style)| style);
            s.mark(start, style);
            if style == 3 || style == 4 {
                s.b.fold_open(start);
            }
            first = false;
        } else if s.ch() == ';' {
            s.one(9);
            first = true;
        } else if s.ch() == '\\' {
            let at = s.c.position;
            s.c.bump();
            s.c.bump();
            s.mark(at, 9);
            first = false;
        } else if "+-=!<>&|$".contains(s.ch()) {
            s.one(9);
            first = false;
        } else {
            s.c.bump();
            first = false;
        }
    }
    let mut result = s.b.finish();
    if property_bool(p, "fold", true) {
        let mut headers = Vec::new();
        for line in &mut result.fold_levels {
            let range = &result.line_states[line.line].range;
            let header = result.spans.iter().any(|span| {
                matches!(span.style, 3 | 4)
                    && span.range.start < range.end
                    && span.range.end > range.start
            });
            line.level = if header { 0x400 } else { 0x401 };
            line.header = header;
            if header {
                headers.push(line.line);
            }
        }
        result.folds.clear();
        for (index, start) in headers.iter().enumerate() {
            let end = headers
                .get(index + 1)
                .map_or(result.line_states.len().saturating_sub(1), |next| {
                    next.saturating_sub(1)
                });
            if end > *start {
                result.folds.push(super::FoldRange {
                    start_line: *start,
                    end_line: end,
                });
            }
        }
    }
    result
}

fn baan(text: &str, p: &LanguageDefinition) -> LexResult {
    let mut s = Scan::new(text, p);
    let mut definition = 0u16;
    while !s.c.is_eof() {
        let start = s.c.position;
        if matches!(s.ch(), '\r' | '\n') {
            s.newline();
            definition = 0;
        } else if s.ch() == '|' {
            s.line(1, false);
        } else if s
            .c
            .rest()
            .get(..8)
            .is_some_and(|word| word.eq_ignore_ascii_case("dllusage"))
            || s.c
                .rest()
                .get(..13)
                .is_some_and(|word| word.eq_ignore_ascii_case("functionusage"))
        {
            let lower = s.c.rest().to_ascii_lowercase();
            let end = if lower.starts_with("dllusage") {
                "enddllusage"
            } else {
                "endfunctionusage"
            };
            if let Some(length) = lower.find(end) {
                s.c.position += length + end.len();
            } else {
                s.c.position = text.len();
            }
            s.mark(start, 2);
            s.b.state_in(start..s.c.position, 2);
            if property_bool(p, "fold.comment", false) {
                s.b.fold_open(start);
                s.b.fold_close(s.c.position.saturating_sub(1));
            }
        } else if s.first() && s.ch() == '#' {
            s.c.bump();
            s.c.eat_while(|ch| ch.is_ascii_alphabetic());
            let directive = text[start..s.c.position].to_ascii_lowercase();
            let valid = matches!(
                directive.as_str(),
                "#define"
                    | "#undef"
                    | "#if"
                    | "#ifdef"
                    | "#ifndef"
                    | "#else"
                    | "#elif"
                    | "#endif"
                    | "#include"
                    | "#pragma"
                    | "#ident"
                    | "#error"
                    | "#assert"
            );
            if !property_bool(p, "lexer.baan.styling.within.preprocessor", false) {
                s.line(6, false);
            }
            s.mark(start, if valid { 6 } else { 8 });
            definition = if matches!(directive.as_str(), "#pragma" | "#include") {
                23
            } else {
                24
            };
            if property_bool(p, "fold.preprocessor", false) {
                if matches!(directive.as_str(), "#if" | "#ifdef" | "#ifndef") {
                    s.b.fold_open(start);
                }
                if directive == "#endif" {
                    s.b.fold_close(start);
                }
            }
        } else if s.ch() == '"' && definition != 23 {
            s.quoted((1, '"'), 5, false, false, false, Some(9));
        } else if s.ch().is_ascii_alphabetic() || s.ch() == '_' {
            let (at, word) = s.word(|ch| ascii_word(ch) || ch == '.');
            let lower = word.to_ascii_lowercase();
            let section = if s.ch() == ':' {
                format!("{lower}:")
            } else {
                lower.clone()
            };
            let matched = (0..9).find(|index| {
                baan_keyword(
                    p,
                    *index,
                    if s.ch() == ':' && p.keywords(*index).contains(':') {
                        &section
                    } else {
                        &lower
                    },
                )
            });
            let style = if matched == Some(0) {
                if lower == "domain" {
                    definition = 21;
                }
                if lower == "function" {
                    definition = 22;
                }
                4
            } else if definition == 21 || definition == 22 {
                let style = definition;
                definition = 0;
                style
            } else if let Some(index) = matched {
                if index == 2 && s.ch() != '(' {
                    8
                } else {
                    9 + index as u16
                }
            } else if definition != 0 {
                let style = definition;
                definition = 0;
                style
            } else {
                baan_identifier(&lower)
            };
            s.mark(at, style);
            if style == 4 && property_bool(p, "fold.baan.keywords.based", false) {
                if matches!(
                    lower.as_str(),
                    "if" | "for" | "while" | "repeat" | "select" | "on" | "case"
                ) {
                    s.b.fold_open(at);
                }
                if matches!(
                    lower.as_str(),
                    "endif" | "endfor" | "endwhile" | "until" | "endselect" | "endon" | "endcase"
                ) {
                    s.b.fold_close(at);
                }
            }
        } else if s.ch().is_ascii_digit()
            || (matches!(s.ch(), '+' | '-' | '.') && s.next().is_ascii_digit())
        {
            s.number(3);
        } else if operator(s.ch()) {
            s.braces(7, property_bool(p, "fold.baan.syntax.based", false));
        } else {
            s.c.bump();
        }
    }
    s.b.finish()
}

fn baan_keyword(p: &LanguageDefinition, index: usize, word: &str) -> bool {
    p.keywords(index).split_ascii_whitespace().any(|entry| {
        if let Some((prefix, suffix)) = entry.split_once('~') {
            word.len() >= prefix.len() + suffix.len()
                && word.starts_with(prefix)
                && word.ends_with(suffix)
        } else {
            entry == word
        }
    })
}

fn baan_identifier(word: &str) -> u16 {
    let b = word.as_bytes();
    let table = |bytes: &[u8]| {
        bytes.len() >= 8
            && bytes[..5].iter().all(u8::is_ascii_alphabetic)
            && bytes[5..8].iter().all(u8::is_ascii_digit)
    };
    if b.len() == 8 && table(b) {
        return 18;
    }
    if b.len() == 9 && b[0] == b't' && table(&b[1..]) {
        return 18;
    }
    if b.len() == 13 && b.starts_with(b"rcd.t") && table(&b[5..]) {
        return 18;
    }
    if matches!(b.len(), 9 | 13 | 14 | 15) && table(b) && b[8] == b'.' {
        return 19;
    }
    if matches!(b.len(), 16 | 17)
        && table(b)
        && ((b[8..].starts_with(b"._index") && b[15].is_ascii_digit())
            || b[8..].starts_with(b"._compnr"))
    {
        return 18;
    }
    if b.len() > 14
        && b[..5].iter().all(u8::is_ascii_alphabetic)
        && &b[5..9] == b".dll"
        && b[9..13].iter().all(u8::is_ascii_digit)
        && b[13] == b'.'
    {
        return 20;
    }
    if b.len() > 15
        && b[..2].iter().all(u8::is_ascii_alphabetic)
        && &b[2..9] == b"int.dll"
        && b[9..14].iter().all(u8::is_ascii_alphabetic)
    {
        return 20;
    }
    if b.len() > 11
        && b[0] == b'i'
        && b[1..6].iter().all(u8::is_ascii_alphabetic)
        && b[6..10].iter().all(u8::is_ascii_digit)
        && b[10] == b'.'
    {
        return 20;
    }
    8
}
