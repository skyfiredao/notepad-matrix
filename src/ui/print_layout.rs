use crate::byte_buffer::ByteBuffer;
use crate::byte_text::{ByteUnit, ByteValue};
use std::io;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PrintGeometry {
    pub width: f64,
    pub height: f64,
    pub inset: f64,
    pub body_top: f64,
    pub body_bottom: f64,
    pub number_width: f64,
    pub line_height: f64,
    pub space_width: f64,
    pub tab_size: usize,
    pub form_feed_page_break: bool,
}

impl PrintGeometry {
    pub fn validate(self) -> io::Result<Self> {
        let values = [self.width, self.height, self.inset, self.body_top, self.body_bottom,
            self.number_width, self.line_height, self.space_width];
        if values.iter().any(|value| !value.is_finite()) || self.width <= 0. || self.height <= 0.
            || self.line_height <= 0. || self.space_width <= 0. || self.inset < 0.
            || self.number_width < 0. || self.body_bottom < self.body_top + self.line_height {
            return Err(io::Error::other("print geometry is invalid"));
        }
        Ok(self)
    }

    fn left(self) -> f64 { self.inset + self.number_width }

    fn right(self) -> f64 { (self.width - self.inset).max(self.left() + self.space_width) }

    fn advance(self, value: ByteValue, width: f64, x: f64) -> f64 {
        if value == ByteValue::Scalar('\t') {
            let tab = self.space_width * self.tab_size.max(1) as f64;
            tab - (x - self.left()) % tab
        } else { width }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrintCheckpoint {
    pub source: usize,
    pub line: usize,
    first_row: bool,
    previous_cr: bool,
    previous_whitespace: bool,
}

#[derive(Clone, Copy)]
pub(super) struct PrintUnitMetrics {
    pub style: usize,
    pub width: f64,
}

pub(super) trait PrintMetrics {
    fn measure(&mut self, value: ByteValue, source: Range<usize>) -> io::Result<PrintUnitMetrics>;
}

pub(super) enum PrintItem {
    Unit {
        source: Range<usize>,
        value: ByteValue,
        style: usize,
        x: f64,
        y: f64,
        width: f64,
    },
    Number { line: usize, y: f64 },
}

pub(super) enum PrintRun {
    Text { source: Range<usize>, style: usize, x: f64, y: f64, width: f64 },
    Representation { value: ByteValue, style: usize, x: f64, y: f64, width: f64 },
    Number { line: usize, y: f64 },
}

struct PrintRunBuilder {
    pending: Option<PrintRun>,
}

impl PrintRunBuilder {
    fn flush(&mut self, emit: &mut impl FnMut(PrintRun) -> io::Result<()>) -> io::Result<()> {
        if let Some(run) = self.pending.take() { emit(run)?; }
        Ok(())
    }

    fn push(&mut self, item: PrintItem, emit: &mut impl FnMut(PrintRun) -> io::Result<()>) -> io::Result<()> {
        match item {
            PrintItem::Number { line, y } => {
                self.flush(emit)?;
                emit(PrintRun::Number { line, y })
            }
            PrintItem::Unit { source, value, style, x, y, width } => {
                if print_unit_is_representation(value) {
                    self.flush(emit)?;
                    return emit(PrintRun::Representation { value, style, x, y, width });
                }
                if let Some(PrintRun::Text {
                    source: previous_source, style: previous_style, x: previous_x,
                    y: previous_y, width: previous_width,
                }) = self.pending.as_mut()
                    && previous_source.end == source.start && *previous_style == style
                    && *previous_y == y && (x - (*previous_x + *previous_width)).abs() <= 0.01 {
                    previous_source.end = source.end;
                    *previous_width += width;
                    return Ok(());
                }
                self.flush(emit)?;
                self.pending = Some(PrintRun::Text { source, style, x, y, width });
                Ok(())
            }
        }
    }
}

pub(super) struct PrintPageIndex {
    geometry: PrintGeometry,
    range: Range<usize>,
    first_line: usize,
    pages: Vec<PrintCheckpoint>,
}

struct PrintPageEnd {
    next: Option<PrintCheckpoint>,
    has_items: bool,
    last_was_eol: bool,
}

impl PrintPageIndex {
    pub fn build_cancellable(
        bytes: &ByteBuffer,
        range: Range<usize>,
        first_line: usize,
        geometry: PrintGeometry,
        metrics: &mut impl PrintMetrics,
        cancelled: &AtomicBool,
    ) -> io::Result<Self> {
        let geometry = geometry.validate()?;
        if range.start > range.end || range.end > bytes.len() {
            return Err(io::Error::other("print range is outside the document"));
        }
        let mut checkpoint = PrintCheckpoint {
            source: range.start, line: first_line, first_row: true,
            previous_cr: false, previous_whitespace: true,
        };
        let mut pages = Vec::new();
        let mut previous_was_eol = false;
        loop {
            check_cancelled(Some(cancelled))?;
            let end = scan_page(bytes, range.end, checkpoint, geometry, metrics, Some(cancelled), |_| Ok(()))?;
            if end.next.is_none() && !end.has_items && (end.last_was_eol || previous_was_eol) && !pages.is_empty() {
                break;
            }
            pages.push(checkpoint);
            let Some(next) = end.next else { break; };
            if next.source == checkpoint.source && next.line == checkpoint.line
                && next.first_row == checkpoint.first_row && next.previous_cr == checkpoint.previous_cr
                && next.previous_whitespace == checkpoint.previous_whitespace {
                return Err(io::Error::other("print pagination did not advance"));
            }
            checkpoint = next;
            previous_was_eol = end.last_was_eol;
        }
        Ok(Self { geometry, range, first_line, pages })
    }

    pub fn page_count(&self) -> usize { self.pages.len() }

    pub fn geometry(&self) -> PrintGeometry { self.geometry }

    pub fn matches(&self, range: &Range<usize>, first_line: usize, geometry: PrintGeometry) -> bool {
        self.range == *range && self.first_line == first_line && self.geometry == geometry
    }

    pub fn draw_page(
        &self,
        bytes: &ByteBuffer,
        page: usize,
        metrics: &mut impl PrintMetrics,
        emit: impl FnMut(PrintItem) -> io::Result<()>,
    ) -> io::Result<()> {
        let checkpoint = page.checked_sub(1).and_then(|page| self.pages.get(page)).copied()
            .ok_or_else(|| io::Error::other("print page is outside the document"))?;
        scan_page(bytes, self.range.end, checkpoint, self.geometry, metrics, None, emit)?;
        Ok(())
    }

    pub fn draw_page_runs(
        &self,
        bytes: &ByteBuffer,
        page: usize,
        metrics: &mut impl PrintMetrics,
        mut emit: impl FnMut(PrintRun) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut builder = PrintRunBuilder { pending: None };
        self.draw_page(bytes, page, metrics, |item| builder.push(item, &mut emit))?;
        builder.flush(&mut emit)
    }
}

fn whitespace(value: ByteValue) -> bool {
    matches!(value, ByteValue::Scalar(character) if character.is_whitespace())
}

fn eol(value: ByteValue) -> bool {
    matches!(value, ByteValue::Scalar('\r' | '\n'))
}

fn measured(
    metrics: &mut impl PrintMetrics,
    value: ByteValue,
    source: Range<usize>,
) -> io::Result<PrintUnitMetrics> {
    let result = metrics.measure(value, source)?;
    if !result.width.is_finite() || result.width < 0. {
        return Err(io::Error::other("print text measurement is invalid"));
    }
    Ok(result)
}

fn word_width(
    bytes: &ByteBuffer,
    range: Range<usize>,
    max_width: f64,
    metrics: &mut impl PrintMetrics,
    cancelled: Option<&AtomicBool>,
) -> io::Result<f64> {
    let start = range.start;
    let selected = bytes.units_in(range).map_err(io::Error::other)?;
    let mut width = 0.;
    for unit in selected {
        check_cancelled(cancelled)?;
        if whitespace(unit.value) { break; }
        width += measured(metrics, unit.value, start + unit.range.start..start + unit.range.end)?.width;
        if width > max_width { break; }
    }
    Ok(width)
}

fn scan_page(
    bytes: &ByteBuffer,
    end: usize,
    mut checkpoint: PrintCheckpoint,
    geometry: PrintGeometry,
    metrics: &mut impl PrintMetrics,
    cancelled: Option<&AtomicBool>,
    mut emit: impl FnMut(PrintItem) -> io::Result<()>,
) -> io::Result<PrintPageEnd> {
    let selected = bytes.units_in(checkpoint.source..end).map_err(io::Error::other)?;
    let start = checkpoint.source;
    let left = geometry.left();
    let right = geometry.right();
    let mut x = left;
    let mut y = geometry.body_top;
    let mut has_items = false;
    let mut last_was_eol = false;
    for ByteUnit { range, value } in selected {
        check_cancelled(cancelled)?;
        let source = start + range.start..start + range.end;
        if value == ByteValue::Scalar('\n') && checkpoint.previous_cr {
            checkpoint.source = source.end;
            checkpoint.previous_cr = false;
            checkpoint.previous_whitespace = true;
            last_was_eol = true;
            continue;
        }
        if value == ByteValue::Scalar('\u{000c}') && geometry.form_feed_page_break {
            checkpoint.source = source.end;
            checkpoint.first_row = true;
            checkpoint.previous_cr = false;
            checkpoint.previous_whitespace = true;
            return Ok(PrintPageEnd { next: Some(checkpoint), has_items, last_was_eol: false });
        }
        if eol(value) {
            if checkpoint.first_row && geometry.number_width > 0. {
                emit(PrintItem::Number { line: checkpoint.line, y })?;
                has_items = true;
            }
            checkpoint.line += 1;
            checkpoint.source = source.end;
            checkpoint.first_row = true;
            checkpoint.previous_cr = value == ByteValue::Scalar('\r');
            checkpoint.previous_whitespace = true;
            last_was_eol = true;
            x = left;
            y += geometry.line_height;
            if y + geometry.line_height > geometry.body_bottom {
                return Ok(PrintPageEnd { next: Some(checkpoint), has_items, last_was_eol });
            }
            continue;
        }
        let metric = measured(metrics, value, source.clone())?;
        if !whitespace(value) && checkpoint.previous_whitespace && x > left {
            let width = word_width(bytes, source.start..end, right - left, metrics, cancelled)?;
            if width <= right - left && x + width > right {
                x = left;
                y += geometry.line_height;
                checkpoint.first_row = false;
                if y + geometry.line_height > geometry.body_bottom {
                    checkpoint.source = source.start;
                    return Ok(PrintPageEnd { next: Some(checkpoint), has_items, last_was_eol });
                }
            }
        }
        let mut advance = geometry.advance(value, metric.width, x);
        if x + advance > right && x > left {
            x = left;
            y += geometry.line_height;
            checkpoint.first_row = false;
            if y + geometry.line_height > geometry.body_bottom {
                checkpoint.source = source.start;
                return Ok(PrintPageEnd { next: Some(checkpoint), has_items, last_was_eol });
            }
            advance = geometry.advance(value, metric.width, x);
        }
        if checkpoint.first_row && x == left && geometry.number_width > 0. {
            emit(PrintItem::Number { line: checkpoint.line, y })?;
            checkpoint.first_row = false;
        }
        if value != ByteValue::Scalar('\t') {
            emit(PrintItem::Unit { source: source.clone(), value, style: metric.style,
                x, y, width: metric.width })?;
        }
        has_items = true;
        x += advance;
        checkpoint.source = source.end;
        checkpoint.previous_cr = false;
        checkpoint.previous_whitespace = whitespace(value);
        last_was_eol = false;
    }
    Ok(PrintPageEnd { next: None, has_items, last_was_eol })
}

fn check_cancelled(cancelled: Option<&AtomicBool>) -> io::Result<()> {
    if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Relaxed)) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "print pagination was replaced"));
    }
    Ok(())
}

pub(super) fn print_language_has_styles(language: crate::syntax::Language) -> bool {
    matches!(language, crate::syntax::Language::Builtin(_) | crate::syntax::Language::Json
        | crate::syntax::Language::UserDefined(_))
}

pub(super) fn print_unit_text(value: ByteValue) -> String {
    const C0: [&str; 32] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR", "SO", "SI",
        "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US",
    ];
    const C1: [&str; 32] = [
        "PAD", "HOP", "BPH", "NBH", "IND", "NEL", "SSA", "ESA", "HTS", "HTJ", "VTS", "PLD", "PLU", "RI", "SS2", "SS3",
        "DCS", "PU1", "PU2", "STS", "CCH", "MW", "SPA", "EPA", "SOS", "SGCI", "SCI", "CSI", "ST", "OSC", "PM", "APC",
    ];
    match value {
        ByteValue::Invalid(byte) => format!("x{byte:02X}"),
        ByteValue::Scalar(character) => match character {
            '\0'..='\u{1f}' => C0[character as usize].into(),
            '\u{7f}' => "DEL".into(),
            '\u{80}'..='\u{9f}' => C1[character as usize - 0x80].into(),
            '\u{2028}' => "LS".into(),
            '\u{2029}' => "PS".into(),
            _ => character.to_string(),
        },
    }
}

pub(super) fn print_unit_is_representation(value: ByteValue) -> bool {
    match value {
        ByteValue::Invalid(_) => true,
        ByteValue::Scalar(character) => matches!(character, '\0'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}'),
    }
}
