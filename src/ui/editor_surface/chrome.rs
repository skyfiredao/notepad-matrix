use std::ops::Range;

pub const BASE_FONT_SIZE: f32 = 20.;
pub const DEFAULT_EDITOR_FONT_FAMILY: &str = "Helvetica";
pub const BASE_LINE_HEIGHT: f32 = 26.;
pub const GUTTER_LEFT: f32 = 0.;
pub const GUTTER_WIDTH: f32 = 32. + 16. + 9. + 14.;
pub const TEXT_LEFT: f32 = GUTTER_LEFT + GUTTER_WIDTH + 1.;
pub const TEXT_RIGHT_PADDING: f32 = 4.;
pub const TEXT_TOP: f32 = 3.;
pub const TEXT_VERTICAL_PADDING: f32 = TEXT_TOP * 2.;
pub const SCROLLBAR_SIZE: f32 = 16.;
pub const TEXT_INSET: f32 = TEXT_LEFT + TEXT_RIGHT_PADDING;

pub fn scrollbar_track(extent: f32) -> Range<f32> {
    let extent = extent.max(0.);
    let cap = SCROLLBAR_SIZE.min(extent / 2.);
    cap..extent - cap
}

pub fn wheel_zoom_delta(control: bool, vertical: f32) -> Option<i32> {
    (control && vertical.is_finite() && vertical != 0.).then_some(if vertical > 0. {
        1
    } else {
        -1
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorDisplay {
    zoom: i32,
    pub indent_guides: bool,
    pub word_wrap: bool,
}

impl Default for EditorDisplay {
    fn default() -> Self {
        Self {
            zoom: 0,
            indent_guides: true,
            word_wrap: false,
        }
    }
}

impl EditorDisplay {
    pub fn zoom_delta(&mut self, delta: i32) -> i32 {
        self.zoom = self.zoom.saturating_add(delta).clamp(-10, 60);
        self.zoom
    }

    pub fn reset_zoom(&mut self) {
        self.zoom = 0;
    }
    pub fn font_size(self) -> f32 {
        (BASE_FONT_SIZE + self.zoom as f32).max(1.)
    }
    pub fn line_height(self) -> f32 {
        (self.font_size() * BASE_LINE_HEIGHT / BASE_FONT_SIZE).ceil()
    }
    pub fn zoom(self) -> i32 {
        self.zoom
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorSymbols {
    pub whitespace: bool,
    pub eol: bool,
    pub nonprinting: bool,
    pub controls: bool,
}

impl Default for EditorSymbols {
    fn default() -> Self {
        Self {
            whitespace: false,
            eol: false,
            nonprinting: false,
            controls: true,
        }
    }
}

impl EditorSymbols {
    pub fn all(self) -> bool {
        self.whitespace && self.eol && self.nonprinting && self.controls
    }
}

pub struct DisplayRow {
    pub text: String,
    pub representations: Vec<Range<usize>>,
    boundaries: Vec<(usize, usize)>,
}

impl DisplayRow {
    pub fn with_symbols(raw: &str, symbols: EditorSymbols) -> Self {
        use crate::byte_text::{ByteUnit, ByteValue};
        if raw.is_ascii() {
            return Self::with_bytes(raw.as_bytes(), symbols);
        }
        let units = raw.char_indices().map(|(start, character)| ByteUnit {
            range: start..start + character.len_utf8(),
            value: ByteValue::Scalar(character),
        });
        Self::project(raw.as_bytes(), units, symbols)
    }

    pub fn with_bytes(raw: &[u8], symbols: EditorSymbols) -> Self {
        Self::project(raw, crate::byte_text::units(raw.iter().copied()), symbols)
    }

    fn project(
        raw: &[u8],
        units: impl Iterator<Item = crate::byte_text::ByteUnit>,
        symbols: EditorSymbols,
    ) -> Self {
        use crate::byte_text::ByteValue;
        let content = &raw[..raw
            .iter()
            .rposition(|byte| !matches!(byte, b'\r' | b'\n'))
            .map_or(0, |index| index + 1)];
        let mut text = String::new();
        let mut representations = Vec::new();
        let mut boundaries = Vec::new();
        let mut column = 0;
        for unit in units.take_while(|unit| unit.range.end <= content.len()) {
            boundaries.push((unit.range.start, text.len()));
            let ch = match unit.value {
                ByteValue::Scalar(character) => character,
                ByteValue::Invalid(byte) => {
                    text.push('x');
                    text.push(char::from(b"0123456789ABCDEF"[usize::from(byte >> 4)]));
                    text.push(char::from(b"0123456789ABCDEF"[usize::from(byte & 0x0f)]));
                    column += 1;
                    continue;
                }
            };
            if ch == '\t' {
                let count = 4 - column % 4;
                text.extend(std::iter::repeat_n(' ', count));
                column += count;
            } else {
                let control = ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}');
                if let Some(label) = control_label(ch) {
                    if if control {
                        symbols.controls
                    } else {
                        symbols.nonprinting
                    } {
                        let start = text.len();
                        text.push_str(label);
                        representations.push(start..text.len());
                    } else if !control {
                        text.push(ch);
                    }
                } else {
                    text.push(ch);
                }
                column += 1;
            }
        }
        boundaries.push((content.len(), text.len()));
        if symbols.eol {
            for byte in &raw[content.len()..] {
                let start = text.len();
                text.push_str(if *byte == b'\r' { "CR" } else { "LF" });
                representations.push(start..text.len());
            }
        }
        Self {
            text,
            representations,
            boundaries,
        }
    }

    pub fn content_len(&self) -> usize {
        self.boundaries.last().map_or(0, |pair| pair.0)
    }
    pub fn display_index(&self, source: usize) -> usize {
        let index = self
            .boundaries
            .partition_point(|pair| pair.0 <= source)
            .saturating_sub(1);
        self.boundaries.get(index).map_or(0, |pair| pair.1)
    }
    pub fn source_index(&self, display: usize) -> usize {
        let index = self.boundaries.partition_point(|pair| pair.1 < display);
        match (
            index.checked_sub(1).and_then(|i| self.boundaries.get(i)),
            self.boundaries.get(index),
        ) {
            (Some(before), Some(after)) if display - before.1 < after.1 - display => before.0,
            (_, Some(after)) => after.0,
            _ => self.content_len(),
        }
    }
}

fn control_label(ch: char) -> Option<&'static str> {
    const C0: [&str; 32] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
        "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB",
        "ESC", "FS", "GS", "RS", "US",
    ];
    const C1: [&str; 32] = [
        "PAD", "HOP", "BPH", "NBH", "IND", "NEL", "SSA", "ESA", "HTS", "HTJ", "VTS", "PLD", "PLU",
        "RI", "SS2", "SS3", "DCS", "PU1", "PU2", "STS", "CCH", "MW", "SPA", "EPA", "SOS", "SGCI",
        "SCI", "CSI", "ST", "OSC", "PM", "APC",
    ];
    match ch {
        '\0'..='\u{1f}' => Some(C0[ch as usize]),
        '\u{7f}' => Some("DEL"),
        '\u{80}'..='\u{9f}' => Some(C1[ch as usize - 0x80]),
        '\u{a0}' => Some("NBSP"),
        '\u{ad}' => Some("SHY"),
        '\u{61c}' => Some("ALM"),
        '\u{70f}' => Some("SAM"),
        '\u{1680}' => Some("OSPM"),
        '\u{180e}' => Some("MVS"),
        '\u{2000}' => Some("NQSP"),
        '\u{2001}' => Some("MQSP"),
        '\u{2002}' => Some("ENSP"),
        '\u{2003}' => Some("EMSP"),
        '\u{2004}' => Some("3/MSP"),
        '\u{2005}' => Some("4/MSP"),
        '\u{2006}' => Some("6/MSP"),
        '\u{2007}' => Some("FSP"),
        '\u{2008}' => Some("PSP"),
        '\u{2009}' => Some("THSP"),
        '\u{200a}' => Some("HSP"),
        '\u{200b}' => Some("ZWSP"),
        '\u{200c}' => Some("ZWNJ"),
        '\u{200d}' => Some("ZWJ"),
        '\u{200e}' => Some("LRM"),
        '\u{200f}' => Some("RLM"),
        '\u{2028}' => Some("LS"),
        '\u{2029}' => Some("PS"),
        '\u{202a}' => Some("LRE"),
        '\u{202b}' => Some("RLE"),
        '\u{202c}' => Some("PDF"),
        '\u{202d}' => Some("LRO"),
        '\u{202e}' => Some("RLO"),
        '\u{202f}' => Some("NNBSP"),
        '\u{205f}' => Some("MMSP"),
        '\u{2060}' => Some("WJ"),
        '\u{2061}' => Some("(FA)"),
        '\u{2062}' => Some("(IT)"),
        '\u{2063}' => Some("(IS)"),
        '\u{2064}' => Some("(IP)"),
        '\u{2066}' => Some("LRI"),
        '\u{2067}' => Some("RLI"),
        '\u{2068}' => Some("FSI"),
        '\u{2069}' => Some("PDI"),
        '\u{206a}' => Some("ISS"),
        '\u{206b}' => Some("ASS"),
        '\u{206c}' => Some("IAFS"),
        '\u{206d}' => Some("AAFS"),
        '\u{206e}' => Some("NADS"),
        '\u{206f}' => Some("NODS"),
        '\u{3000}' => Some("IDSP"),
        '\u{feff}' => Some("ZWNBSP"),
        '\u{fff9}' => Some("IAA"),
        '\u{fffa}' => Some("IAS"),
        '\u{fffb}' => Some("IAT"),
        _ => None,
    }
}

pub fn indent_guide_columns(raw: &str) -> Vec<usize> {
    indent_guides_with_context(raw, None, None)
}

pub fn indent_guides_with_context(
    raw: &str,
    previous: Option<&str>,
    next: Option<&str>,
) -> Vec<usize> {
    let columns_in = |line: &str| {
        line.chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .fold(0, |column, ch| {
                if ch == '\t' {
                    column + 4 - column % 4
                } else {
                    column + 1
                }
            })
    };
    let mut columns = columns_in(raw);
    if raw.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
        columns = columns
            .max(previous.map_or(0, columns_in))
            .max(next.map_or(0, columns_in));
    }
    (4..columns).step_by(4).collect()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollAxis {
    track: f32,
    viewport: f32,
    maximum: f32,
    offset: f32,
    thumb_length: f32,
}

impl ScrollAxis {
    pub fn step_by(self, delta: f32) -> f32 {
        (self.offset + delta).clamp(0., self.maximum)
    }
    pub fn clamped_offset(self) -> f32 {
        self.offset
    }

    pub fn new(track: f32, content: f32, viewport: f32, offset: f32) -> Self {
        let finite = |value: f32| if value.is_finite() { value.max(0.) } else { 0. };
        let track = finite(track);
        let content = finite(content);
        let viewport = finite(viewport);
        let maximum = (content - viewport).max(0.);
        let thumb_length = if content > viewport && content > 0. {
            (track * viewport / content).max(18.).min(track)
        } else {
            track
        };
        Self {
            track,
            viewport,
            maximum,
            offset: finite(offset).min(maximum),
            thumb_length,
        }
    }

    pub fn thumb(self) -> Range<f32> {
        let start = if self.maximum > 0. {
            self.offset / self.maximum * (self.track - self.thumb_length)
        } else {
            0.
        };
        start..start + self.thumb_length
    }

    pub fn offset_for_thumb(self, position: f32) -> f32 {
        let travel = self.track - self.thumb_length;
        if travel <= 0. || !position.is_finite() {
            return 0.;
        }
        (position / travel).clamp(0., 1.) * self.maximum
    }

    pub fn page_toward(self, position: f32) -> f32 {
        let thumb = self.thumb();
        if position < thumb.start {
            (self.offset - self.viewport).max(0.)
        } else if position >= thumb.end {
            (self.offset + self.viewport).min(self.maximum)
        } else {
            self.offset
        }
    }
}
