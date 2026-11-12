use std::collections::BTreeMap;

use crate::languages::LanguageDefinition;
use crate::syntax::StyleAppearance;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheUpdate {
    Unchanged,
    Restyled,
    Relexed(crate::lexers::RelexReport),
}

pub struct SyntaxCache {
    definition: LanguageDefinition,
    style_revision: u64,
    lexer: crate::lexers::IncrementalLexer,
    tag_tokens: Vec<crate::lexers::markup::TagToken>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledSpan {
    pub range: std::ops::Range<usize>,
    pub style: u16,
    pub appearance: StyleAppearance,
}

impl SyntaxCache {
    pub fn new(
        text: &str,
        definition: &LanguageDefinition,
        style_revision: u64,
    ) -> Result<Self, crate::lexers::LexError> {
        validate_definition(definition)?;
        Ok(Self {
            definition: definition.clone(),
            style_revision,
            lexer: crate::lexers::IncrementalLexer::new(text, definition)?,
            tag_tokens: structural_tokens(text, definition),
        })
    }

    pub fn definition(&self) -> &LanguageDefinition {
        &self.definition
    }

    pub fn text(&self) -> &str {
        self.lexer.text()
    }

    pub fn result(&self) -> &crate::lexers::LexResult {
        self.lexer.result()
    }

    pub fn matching_tags(&self, caret: usize, options: TagMatchOptions) -> Option<TagMatchRanges> {
        if !options.enabled || caret > self.text().len() || !self.text().is_char_boundary(caret) {
            return None;
        }
        matching_tag_tokens(
            self.text(),
            &self.definition,
            &self.tag_tokens,
            caret,
            options,
        )
    }

    pub fn styled_runs(
        &self,
        range: std::ops::Range<usize>,
        dark: bool,
        profile: &StyleAppearance,
        zoom_delta: i32,
    ) -> Result<Vec<StyledSpan>, crate::lexers::LexError> {
        if range.start > range.end
            || range.end > self.text().len()
            || !self.text().is_char_boundary(range.start)
            || !self.text().is_char_boundary(range.end)
        {
            return Err(crate::lexers::LexError::InvalidRange(range));
        }
        let spans = &self.result().spans;
        let first = spans.partition_point(|span| span.range.end <= range.start);
        Ok(spans[first..]
            .iter()
            .take_while(|span| span.range.start < range.end)
            .map(|span| StyledSpan {
                range: span.range.start.max(range.start)..span.range.end.min(range.end),
                style: span.style,
                appearance: style_appearance(
                    &self.definition,
                    span.style,
                    dark,
                    profile,
                    zoom_delta,
                ),
            })
            .collect())
    }

    pub fn refresh(
        &mut self,
        text: &str,
        definition: &LanguageDefinition,
        style_revision: u64,
    ) -> Result<CacheUpdate, crate::lexers::LexError> {
        if self.definition != *definition {
            let updated = Self::new(text, definition, style_revision)?;
            *self = updated;
            return Ok(CacheUpdate::Relexed(crate::lexers::RelexReport {
                relexed_range: 0..text.len(),
                reused_prefix_lines: 0,
                reused_suffix_lines: 0,
            }));
        }
        if self.text() == text {
            let changed = self.style_revision != style_revision;
            self.style_revision = style_revision;
            return Ok(if changed {
                CacheUpdate::Restyled
            } else {
                CacheUpdate::Unchanged
            });
        }
        let old = self.text();
        let mut start = old
            .bytes()
            .zip(text.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(start) || !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut suffix = old[start..]
            .bytes()
            .rev()
            .zip(text[start..].bytes().rev())
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(old.len() - suffix)
            || !text.is_char_boundary(text.len() - suffix)
        {
            suffix -= 1;
        }
        let report = self
            .lexer
            .replace(start..old.len() - suffix, &text[start..text.len() - suffix])?;
        self.tag_tokens = structural_tokens(text, definition);
        self.style_revision = style_revision;
        Ok(CacheUpdate::Relexed(report))
    }
}

pub fn validate_definition(definition: &LanguageDefinition) -> Result<(), crate::lexers::LexError> {
    let catalog = crate::languages::catalog()
        .map_err(|error| crate::lexers::LexError::InvalidState(error.to_string()))?;
    let canonical = catalog.by_id(definition.id).ok_or_else(|| {
        crate::lexers::LexError::InvalidState(format!(
            "Built-in language {} is unavailable",
            definition.id
        ))
    })?;
    if definition.name != canonical.name {
        return Err(crate::lexers::LexError::UnsupportedLanguage(
            definition.name.clone(),
        ));
    }
    if definition.effective_lexer != canonical.effective_lexer
        || definition.source_lexer != canonical.source_lexer
    {
        return Err(crate::lexers::LexError::UnsupportedLanguage(format!(
            "{} ({})",
            definition.effective_lexer, definition.source_lexer
        )));
    }
    Ok(())
}

fn apply_style(
    out: &mut StyleAppearance,
    attributes: &BTreeMap<String, String>,
    colors: bool,
    zoom_delta: i32,
) {
    let number = |name: &str| {
        attributes
            .get(name)
            .and_then(|value| value.parse::<i32>().ok())
    };
    let mask = number("colorStyle").unwrap_or(3);
    if colors {
        for (name, bit, target) in [
            ("fgColor", 1, &mut out.foreground),
            ("bgColor", 2, &mut out.background),
        ] {
            if mask & bit != 0
                && let Some(color) = attributes
                    .get(name)
                    .and_then(|value| u32::from_str_radix(value, 16).ok())
            {
                *target = color & 0xffffff;
            }
        }
    }
    if let Some(name) = attributes.get("fontName").filter(|name| !name.is_empty()) {
        out.font_family = name.clone();
    }
    if let Some(bits) = number("fontStyle").filter(|bits| *bits >= 0) {
        out.bold = bits & 1 != 0;
        out.italic = bits & 2 != 0;
        out.underline = bits & 4 != 0;
    }
    if let Some(size) = number("fontSize").filter(|size| *size > 0) {
        out.font_size = size.saturating_add(zoom_delta).max(1) as u32;
    }
}

pub fn style_appearance(
    definition: &LanguageDefinition,
    style: u16,
    dark: bool,
    profile: &StyleAppearance,
    zoom_delta: i32,
) -> StyleAppearance {
    let mut appearance = profile.clone();
    let catalog = crate::languages::catalog().ok();
    if let Some(default) = catalog.and_then(|catalog| catalog.default_style(dark)) {
        let colors = default
            .attributes
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "fgColor" | "bgColor" | "colorStyle"))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        apply_style(&mut appearance, &colors, true, zoom_delta);
    }
    if dark {
        if let Some(themed) =
            catalog.and_then(|catalog| catalog.style_for_theme(definition.id, style, true))
        {
            let mut attributes = themed.attributes.clone();
            if let Some(custom) = definition.style(style)
                && let Some(original) = catalog
                    .and_then(|catalog| catalog.by_id(definition.id))
                    .and_then(|profile| profile.style(style))
            {
                for (name, value) in &custom.attributes {
                    if original.attributes.get(name) != Some(value) {
                        attributes.insert(name.clone(), value.clone());
                    }
                }
            }
            apply_style(&mut appearance, &attributes, true, zoom_delta);
        }
    } else if let Some(style) = definition.style(style) {
        apply_style(&mut appearance, &style.attributes, true, zoom_delta);
    }
    appearance
}

pub fn metric_appearances(
    definition: &LanguageDefinition,
    dark: bool,
    profile: &StyleAppearance,
    zoom_delta: i32,
) -> Vec<StyleAppearance> {
    let mut appearances = vec![
        profile.clone(),
        style_appearance(definition, 0, dark, profile, zoom_delta),
    ];
    appearances.extend(
        definition
            .styles
            .iter()
            .map(|style| style_appearance(definition, style.id, dark, profile, zoom_delta)),
    );
    appearances
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagMatchOptions {
    pub enabled: bool,
    pub attributes: bool,
    pub xhtml: bool,
}

impl Default for TagMatchOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            attributes: true,
            xhtml: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagMatchRanges {
    pub tags: Vec<std::ops::Range<usize>>,
    pub attributes: Vec<std::ops::Range<usize>>,
    pub content: Option<std::ops::Range<usize>>,
}

pub fn matching_tags(
    text: &str,
    definition: &LanguageDefinition,
    caret: usize,
    options: TagMatchOptions,
) -> Option<TagMatchRanges> {
    if !options.enabled
        || caret > text.len()
        || !text.is_char_boundary(caret)
        || !matches!(definition.key(), "html" | "xml" | "php" | "asp" | "jsp")
    {
        return None;
    }
    let tokens = structural_tokens(text, definition);
    matching_tag_tokens(text, definition, &tokens, caret, options)
}

fn matching_tag_tokens(
    text: &str,
    definition: &LanguageDefinition,
    tokens: &[crate::lexers::markup::TagToken],
    caret: usize,
    options: TagMatchOptions,
) -> Option<TagMatchRanges> {
    if matches!(definition.key(), "php" | "asp" | "jsp") {
        let (begin, end) = if definition.key() == "php" {
            ("<?", "?>")
        } else {
            ("<%", "%>")
        };
        let mut limit = caret.saturating_add(1).min(text.len());
        while !text.is_char_boundary(limit) {
            limit += 1;
        }
        let prefix = &text[..limit];
        if let Some(start) = prefix.rfind(begin)
            && prefix
                .rfind(end)
                .is_none_or(|end| end <= start + begin.len())
        {
            return None;
        }
    }
    let selected = tokens
        .iter()
        .position(|token| token.range.contains(&caret))?;
    let token = &tokens[selected];
    if !text[token.range.clone()].ends_with('>') {
        return None;
    }
    let same_name = |other: &crate::lexers::markup::TagToken| {
        let a = &text[token.name_range.clone()];
        let b = &text[other.name_range.clone()];
        if definition.key() == "xml" || options.xhtml {
            a == b
        } else {
            a.eq_ignore_ascii_case(b)
        }
    };
    let self_closed =
        |token: &crate::lexers::markup::TagToken| text[token.range.clone()].ends_with("/>");
    let (open, close) = if self_closed(token) && !token.closing {
        (token, None)
    } else {
        let mut depth = 1;
        let mut partner = None;
        let candidates: Box<dyn Iterator<Item = &crate::lexers::markup::TagToken>> =
            if token.closing {
                Box::new(tokens[..selected].iter().rev())
            } else {
                Box::new(tokens[selected + 1..].iter())
            };
        for other in candidates.filter(|other| same_name(other) && !self_closed(other)) {
            if other.closing == token.closing {
                depth += 1;
            } else {
                depth -= 1;
            }
            if depth == 0 {
                partner = Some(other);
                break;
            }
        }
        let partner = partner?;
        if token.closing {
            (partner, Some(token))
        } else {
            (token, Some(partner))
        }
    };
    let tail = if close.is_some() { 1 } else { 2 };
    let mut tags = vec![
        open.range.start..open.name_range.end,
        open.range.end - tail..open.range.end,
    ];
    if let Some(close) = close {
        tags.push(close.range.clone());
    }
    let attributes = if options.attributes {
        attribute_ranges(text, open, open.range.end - tail)
    } else {
        Vec::new()
    };
    Some(TagMatchRanges {
        tags,
        attributes,
        content: close.map(|close| open.range.end..close.range.start),
    })
}

fn structural_tokens(
    text: &str,
    definition: &LanguageDefinition,
) -> Vec<crate::lexers::markup::TagToken> {
    if matches!(definition.key(), "html" | "xml" | "php" | "asp" | "jsp") {
        crate::lexers::markup::structural_tokens(text, definition)
    } else {
        Vec::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagIndicator {
    pub range: std::ops::Range<usize>,
    pub color: u32,
    pub alpha: u8,
}

pub fn tag_indicators(ranges: &TagMatchRanges, dark: bool) -> Vec<TagIndicator> {
    let theme = crate::languages::catalog()
        .ok()
        .map(|catalog| catalog.themes().theme(dark));
    let color = |id, fallback| {
        theme
            .and_then(|theme| theme.global_styles.iter().find(|style| style.id == id))
            .and_then(|style| style.attributes.get("bgColor"))
            .and_then(|color| u32::from_str_radix(color, 16).ok())
            .unwrap_or(fallback)
    };
    let tag_color = color(27, 0x8000ff);
    let attribute_color = color(26, 0xffff00);
    ranges
        .tags
        .iter()
        .map(|range| TagIndicator {
            range: range.clone(),
            color: tag_color,
            alpha: 100,
        })
        .chain(ranges.attributes.iter().map(|range| TagIndicator {
            range: range.clone(),
            color: attribute_color,
            alpha: 100,
        }))
        .collect()
}

fn attribute_ranges(
    text: &str,
    token: &crate::lexers::markup::TagToken,
    end: usize,
) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    for name in &token.attributes {
        let mut at = name.end;
        while at < end && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= end || bytes[at] != b'=' {
            continue;
        }
        at += 1;
        while at < end && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= end {
            continue;
        }
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            while at < end && bytes[at] != quote {
                at += 1;
            }
            if at >= end {
                continue;
            }
            at += 1;
        } else {
            while at < end && !bytes[at].is_ascii_whitespace() {
                at += 1;
            }
        }
        ranges.push(name.start..at);
    }
    ranges
}
