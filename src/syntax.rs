//! JSON lexical states ported from the frozen Lexilla LexJSON.cxx, not linked to
//! Lexilla. Npp setJsonLexer enables escapes, disables comments for JSON, and
//! supplies the keyword lists below (langs.model.xml).
// Original lexer author: nkmathew. Lexilla license:
// Copyright 1998-2021 by Neil Hodgson <neilh@scintilla.org>
// All Rights Reserved
// Permission to use, copy, modify, and distribute this software and its
// documentation for any purpose and without fee is hereby granted,
// provided that the above copyright notice appear in all copies and that
// both that copyright notice and this permission notice appear in
// supporting documentation.
// NEIL HODGSON DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE,
// INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS, IN NO
// EVENT SHALL NEIL HODGSON BE LIABLE FOR ANY SPECIAL, INDIRECT OR
// CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF
// USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
// OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
// PERFORMANCE OF THIS SOFTWARE.

use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    NormalText,
    Json,
    Builtin(u16),
    UserDefined(u64),
}

impl Language {
    pub fn key(self) -> String {
        match self {
            Self::NormalText => "normal".into(),
            Self::Json => "json".into(),
            Self::Builtin(0) => "normal".into(),
            Self::Builtin(57) => "json".into(),
            Self::Builtin(id) => format!("builtin:{id}"),
            Self::UserDefined(id) => format!("udl:{id}"),
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "normal" => Some(Self::NormalText),
            "json" => Some(Self::Json),
            _ => {
                if let Some(digits) = key.strip_prefix("builtin:") {
                    if digits.is_empty()
                        || (digits.starts_with('0') && digits.len() > 1)
                        || !digits.bytes().all(|byte| byte.is_ascii_digit())
                    {
                        return None;
                    }
                    let id = digits.parse::<u16>().ok()?;
                    crate::languages::catalog().ok()?.by_id(id)?;
                    return Some(match id {
                        0 => Self::NormalText,
                        57 => Self::Json,
                        id => Self::Builtin(id),
                    });
                }
                let digits = key.strip_prefix("udl:")?;
                if (digits.starts_with('0') && digits.len() > 1)
                    || !digits.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return None;
                }
                digits.parse::<u64>().ok().map(Self::UserDefined)
            }
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::NormalText => "None (Normal Text)",
            Self::Json => "JSON",
            Self::Builtin(id) => crate::languages::catalog()
                .ok()
                .and_then(|catalog| catalog.by_id(id))
                .map_or("Unknown language", |profile| profile.label.as_str()),
            Self::UserDefined(_) => "User Defined",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::NormalText => "Normal text file",
            Self::Json => "JSON file",
            Self::Builtin(id) => crate::languages::catalog()
                .ok()
                .and_then(|catalog| catalog.by_id(id))
                .map_or("Unknown language file", |profile| {
                    profile.description.as_str()
                }),
            Self::UserDefined(_) => "User Defined file",
        }
    }
    pub fn from_path(path: &std::path::Path) -> Self {
        match crate::languages::catalog()
            .ok()
            .and_then(|catalog| catalog.by_path(path))
            .map(|profile| profile.id)
        {
            Some(57) => Self::Json,
            Some(0) | None => Self::NormalText,
            Some(id) => Self::Builtin(id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleAppearance {
    pub foreground: u32,
    pub background: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub font_family: String,
    pub font_size: u32,
}

impl StyleAppearance {
    pub fn plain(dark: bool) -> Self {
        Self {
            foreground: if dark { 0xeeeeee } else { 0x202020 },
            background: if dark { 0x202020 } else { 0xffffff },
            bold: false,
            italic: false,
            underline: false,
            font_family: "Menlo".into(),
            font_size: 13,
        }
    }
}

pub struct StyleLineMetrics {
    max_size: f32,
    ascent: f32,
    descent: f32,
    measured: bool,
}

impl StyleLineMetrics {
    pub fn new(default_size: f32) -> Self {
        Self {
            max_size: default_size,
            ascent: 0.,
            descent: 0.,
            measured: false,
        }
    }
    pub fn include_size(&mut self, size: f32) {
        self.max_size = self.max_size.max(size);
    }
    pub fn include(&mut self, size: f32, ascent: f32, descent: f32) {
        self.include_size(size);
        self.ascent = self.ascent.max(ascent);
        self.descent = self.descent.max(descent.abs());
        self.measured = true;
    }
    pub fn line_height(&self) -> f32 {
        (self.max_size * 26. / 20.)
            .max(self.ascent + self.descent)
            .ceil()
    }
    pub fn baseline(&self) -> f32 {
        let (ascent, descent) = if self.measured {
            (self.ascent, self.descent)
        } else {
            (self.max_size * 0.8, self.max_size * 0.2)
        };
        (self.line_height() - ascent - descent) / 2. + ascent
    }
}

pub fn udl_appearance(
    definition: &crate::udl::Definition,
    style: crate::udl::UdlStyleId,
    dark: bool,
) -> StyleAppearance {
    udl_appearance_with_profile(definition, style, &StyleAppearance::plain(dark), 0)
}

pub fn udl_appearance_with_profile(
    definition: &crate::udl::Definition,
    style: crate::udl::UdlStyleId,
    profile: &StyleAppearance,
    zoom_delta: i32,
) -> StyleAppearance {
    fn apply(out: &mut StyleAppearance, style: &crate::udl::Style, zoom_delta: i32) {
        if style.color_style & 1 != 0
            && let Some(color) = style.foreground
        {
            out.foreground = color & 0xffffff;
        }
        if style.color_style & 2 != 0
            && let Some(color) = style.background
        {
            out.background = color & 0xffffff;
        }
        if !style.font_name.is_empty() {
            out.font_family = style.font_name.clone();
        }
        if style.font_style != -1 {
            out.bold = style.font_style & 1 != 0;
            out.italic = style.font_style & 2 != 0;
            out.underline = style.font_style & 4 != 0;
        }
        if style.font_size > 0 {
            out.font_size = style.font_size.saturating_add(zoom_delta).max(1) as u32;
        }
    }
    let mut out = profile.clone();
    if let Some(style) = definition.styles.get(style as usize) {
        apply(&mut out, style, zoom_delta);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum JsonStyle {
    Default = 0,
    Number = 1,
    String = 2,
    StringEol = 3,
    Property = 4,
    Escape = 5,
    LineComment = 6,
    BlockComment = 7,
    Operator = 8,
    Uri = 9,
    CompactIri = 10,
    Keyword = 11,
    LdKeyword = 12,
    Error = 13,
}

impl JsonStyle {
    /// Original stylers.model.xml and DarkModeDefault.xml JSON styles.
    pub fn appearance(self, dark: bool) -> (u32, u32, bool) {
        let foreground = if dark {
            [
                0xdcdccc, 0x8cd0d3, 0xdcdccc, 0x808080, 0xcedf99, 0xdca3a3, 0x7f9f7f, 0x7f9f7f,
                0x9f9d6d, 0xffcfaf, 0xffcfaf, 0xdfc47d, 0xdfc47d, 0xffff80,
            ]
        } else {
            [
                0x000000, 0xff8000, 0x800000, 0x808080, 0x8000ff, 0x0000ff, 0x008000, 0x008000,
                0x000000, 0x0000ff, 0x0000ff, 0x18af8a, 0xff0000, 0x000000,
            ]
        };
        let background = if self == Self::Error {
            if dark { 0xff0000 } else { 0xffa448 }
        } else if dark {
            0x3f3f3f
        } else {
            0xffffff
        };
        (
            foreground[self as usize],
            background,
            matches!(self, Self::Escape | Self::CompactIri | Self::Keyword),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSpan {
    pub range: Range<usize>,
    pub style: JsonStyle,
}

fn space(ch: char) -> bool {
    ('\t'..='\r').contains(&ch) || ch == ' '
}
fn operator(ch: char) -> bool {
    "[{}]:,".contains(ch)
}
fn keyword_char(ch: char) -> bool {
    ch.is_ascii_alphabetic() || "$_".contains(ch)
}
fn ld_char(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ":@".contains(ch)
}
fn url_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,),=".contains(ch)
}

fn property_name(text: &str, start: usize) -> bool {
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
                if !space(char::from(ch)) || ch == b'\n' || (ch == b'\r' && next != b'\n') {
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

struct JsonContext<'a> {
    text: &'a str,
    pos: usize,
    segment: usize,
    previous: char,
    state: JsonStyle,
    spans: Vec<StyleSpan>,
}

impl JsonContext<'_> {
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
    fn at_line_start(&self) -> bool {
        self.pos == 0 || self.previous == '\n' || (self.previous == '\r' && self.ch() != '\n')
    }
    fn at_line_end(&self) -> bool {
        self.pos >= self.text.len()
            || self.ch() == '\n'
            || (self.ch() == '\r' && self.next() != '\n')
    }
    fn match_line_end(&self) -> bool {
        self.pos >= self.text.len()
            || self.ch() == '\r'
            || (self.ch() == '\n' && self.previous != '\r')
    }
    fn matches(&self, prefix: &str) -> bool {
        self.text
            .get(self.pos..)
            .is_some_and(|tail| tail.starts_with(prefix))
    }
    fn word(&self, allowed: fn(char) -> bool) -> &str {
        let count = self
            .text
            .get(self.pos..)
            .unwrap_or_default()
            .chars()
            .take(50)
            .take_while(|&ch| allowed(ch))
            .map(char::len_utf8)
            .sum::<usize>();
        &self.text[self.pos.min(self.text.len())..self.pos.min(self.text.len()) + count]
    }
    fn flush(&mut self) {
        let end = self.pos.min(self.text.len());
        if self.segment < end {
            if let Some(last) = self
                .spans
                .last_mut()
                .filter(|last| last.range.end == self.segment && last.style == self.state)
            {
                last.range.end = end;
            } else {
                self.spans.push(StyleSpan {
                    range: self.segment..end,
                    style: self.state,
                });
            }
        }
        self.segment = end;
    }
    fn set(&mut self, state: JsonStyle) {
        self.flush();
        self.state = state;
    }
    fn forward(&mut self) {
        self.previous = self.ch();
        self.pos += self.previous.len_utf8();
    }
    fn forward_set(&mut self, state: JsonStyle) {
        self.forward();
        self.set(state);
    }
}

fn new_escape(next: char, digits: &mut i32) -> bool {
    *digits = if next == 'u' { 5 } else { 0 };
    "\\\"tnbfru/".contains(next)
}

/// Full-document JSON token styles for the source's default JSON configuration.
/// This is lexical styling, not a JSON validity parser; source lookahead limits
/// and ERROR-until-line-end recovery are deliberately retained.
pub fn json_styles(text: &str) -> Vec<StyleSpan> {
    use JsonStyle::*;
    let mut cx = JsonContext {
        text,
        pos: 0,
        segment: 0,
        previous: '\0',
        state: Default,
        spans: Vec::new(),
    };
    let mut string_before = String;
    let mut digits = 0;
    let mut iri_colons = 0;
    let mut iri_invalid = false;
    // StyleContext processes one virtual NUL at EOF, allowing an unterminated
    // final string segment to become STRINGEOL even without a trailing newline.
    while cx.pos <= text.len() {
        match cx.state {
            BlockComment => {
                if cx.matches("*/") {
                    cx.forward();
                    cx.forward_set(Default);
                }
            }
            LineComment => {
                if cx.match_line_end() {
                    cx.set(Default);
                }
            }
            StringEol => {
                if cx.at_line_start() {
                    cx.set(Default);
                }
            }
            Escape => {
                digits -= 1;
                if digits > 0 {
                    if !cx.ch().is_ascii_hexdigit() {
                        cx.set(Error);
                    }
                } else if cx.ch() == '"' {
                    cx.set(string_before);
                    cx.forward_set(Default);
                } else if cx.ch() == '\\' {
                    if !new_escape(cx.next(), &mut digits) {
                        cx.set(Error);
                    }
                    cx.forward();
                } else {
                    cx.set(string_before);
                    if cx.at_line_end() {
                        cx.state = StringEol;
                    }
                }
            }
            Property | String => {
                if cx.ch() == '"' {
                    if !iri_invalid && iri_colons == 1 {
                        cx.state = CompactIri;
                    }
                    cx.forward_set(Default);
                    iri_colons = 0;
                    iri_invalid = false;
                } else if cx.at_line_end() {
                    cx.state = StringEol;
                } else if cx.ch() == '\\' {
                    string_before = cx.state;
                    cx.set(Escape);
                    if !new_escape(cx.next(), &mut digits) {
                        cx.set(Error);
                    }
                    cx.forward();
                } else if [
                    "https://", "http://", "ssh://", "git://", "svn://", "ftp://", "mailto:",
                ]
                .iter()
                .any(|prefix| cx.matches(prefix))
                {
                    string_before = cx.state;
                    cx.set(Uri);
                } else if cx.ch() == '@' {
                    if [
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
                    ]
                    .contains(&cx.word(ld_char))
                    {
                        string_before = cx.state;
                        cx.set(LdKeyword);
                    }
                } else if cx.ch() == ':' {
                    iri_colons += 1;
                } else {
                    iri_invalid |= !(cx.ch().is_ascii_alphabetic() || "$_-".contains(cx.ch()));
                }
            }
            LdKeyword | Uri => {
                if (cx.state == LdKeyword && !ld_char(cx.ch())) || !url_char(cx.ch()) {
                    cx.set(string_before);
                }
                if cx.ch() == '"' {
                    cx.forward_set(Default);
                } else if cx.ch() == '\\' {
                    cx.set(Escape);
                    if !new_escape(cx.next(), &mut digits) {
                        cx.set(Error);
                    }
                    cx.forward();
                } else if cx.at_line_end() {
                    cx.state = StringEol;
                }
            }
            Operator | Number => cx.set(Default),
            Error => {
                if cx.match_line_end() {
                    cx.set(Default);
                }
            }
            Keyword => {
                if !keyword_char(cx.ch()) {
                    cx.set(Default);
                }
            }
            Default | CompactIri => {}
        }
        if cx.state == Default {
            if cx.ch() == '"' {
                iri_colons = 0;
                iri_invalid = false;
                let style = if property_name(text, cx.pos) {
                    Property
                } else {
                    String
                };
                cx.set(style);
            } else if operator(cx.ch()) {
                cx.set(Operator);
            } else if keyword_char(cx.ch())
                && ["false", "null", "true"].contains(&cx.word(keyword_char))
            {
                cx.set(Keyword);
            }
            let ch = cx.ch();
            let prev = cx.previous;
            let next = cx.next();
            let number_start = ch.is_ascii_digit()
                && (prev == '+'
                    || prev == '-'
                    || cx.at_line_start()
                    || space(prev)
                    || operator(prev));
            let exponent = ch.eq_ignore_ascii_case(&'e')
                && prev.is_ascii_digit()
                && (next.is_ascii_digit() || next == '+' || next == '-');
            let sign = (ch == '-' || ch == '+')
                && ((prev.eq_ignore_ascii_case(&'e') && next.is_ascii_digit())
                    || ((space(prev) || operator(prev)) && next.is_ascii_digit()));
            let adjacent_digit = ch.is_ascii_digit() && prev.is_ascii_digit();
            let after_exponent = ch.is_ascii_digit() && prev.eq_ignore_ascii_case(&'e');
            let dot = ch == '.' && prev.is_ascii_digit() && next.is_ascii_digit();
            let after_dot = ch.is_ascii_digit() && prev == '.';
            if number_start
                || exponent
                || sign
                || adjacent_digit
                || after_exponent
                || dot
                || after_dot
            {
                cx.set(Number);
            } else if cx.state == Default && !space(ch) {
                cx.set(Error);
            }
        }
        cx.forward();
    }
    cx.flush();
    cx.spans
}
