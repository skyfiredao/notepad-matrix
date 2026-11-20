use super::print_layout::{PrintMetrics, PrintUnitMetrics, print_unit_is_representation, print_unit_text};
use crate::byte_text::ByteValue;
use objc2::{ClassType, MainThreadMarker};
use objc2_app_kit::{NSFont, NSFontDescriptor};
use objc2_core_foundation::{CFAttributedString, CFDictionary, CFRetained, CFString};
use objc2_core_text::{CTFont, CTFontDescriptor, CTFontSymbolicTraits, CTLine, kCTFontAttributeName};
use objc2_foundation::{NSData, NSKeyedArchiver, NSKeyedUnarchiver};
use std::collections::BTreeMap;
use std::io;
use std::ops::Range;
use std::ptr;
use std::sync::Arc;

const PRINT_WIDTH_CACHE_ENTRIES: usize = 4096;

#[derive(Clone)]
pub(super) struct PrintStyleSpan {
    pub range: Range<usize>,
    pub style: usize,
}

pub(super) struct PrintFontSpec {
    pub descriptor: Vec<u8>,
    pub name: String,
    pub size: f64,
    pub traits: CTFontSymbolicTraits,
    pub ascii_widths: [Option<f64>; 128],
    pub other_widths: BTreeMap<u32, f64>,
}

impl PrintFontSpec {
    pub fn from_font(
        font: &NSFont,
        ascii_widths: [Option<f64>; 128],
        other_widths: BTreeMap<u32, f64>,
    ) -> io::Result<Self> {
        MainThreadMarker::new().ok_or_else(|| io::Error::other("print font archiving requires the main thread"))?;
        let native_font = unsafe { &*(font as *const NSFont).cast::<CTFont>() };
        let descriptor = unsafe { native_font.font_descriptor() };
        let native_descriptor = unsafe { &*(&*descriptor as *const CTFontDescriptor).cast::<NSFontDescriptor>() };
        let archived = unsafe {
            NSKeyedArchiver::archivedDataWithRootObject_requiringSecureCoding_error(native_descriptor, true)
        }.map_err(|error| io::Error::other(format!("could not archive print font descriptor: {}", error.localizedDescription())))?;
        Ok(Self {
            descriptor: archived.to_vec(), name: unsafe { native_font.post_script_name() }.to_string(),
            size: unsafe { native_font.size() }, traits: unsafe { native_font.symbolic_traits() },
            ascii_widths, other_widths,
        })
    }

    fn create(&self) -> io::Result<CFRetained<CTFont>> {
        let data = NSData::with_bytes(&self.descriptor);
        let descriptor = unsafe {
            NSKeyedUnarchiver::unarchivedObjectOfClass_fromData_error(NSFontDescriptor::class(), &data)
        }.map_err(|error| io::Error::other(format!("could not restore print font descriptor: {}", error.localizedDescription())))?;
        let descriptor = descriptor.downcast::<NSFontDescriptor>()
            .map_err(|_| io::Error::other("restored print font descriptor has an invalid class"))?;
        let native_descriptor = unsafe { &*(&*descriptor as *const NSFontDescriptor).cast::<CTFontDescriptor>() };
        let font = unsafe { CTFont::with_font_descriptor(native_descriptor, self.size, ptr::null()) };
        if unsafe { font.post_script_name() }.to_string() != self.name
            || unsafe { font.size() } != self.size || unsafe { font.symbolic_traits() } != self.traits {
            return Err(io::Error::other("restored print font does not match the drawing font"));
        }
        Ok(font)
    }
}

pub(super) struct PrintMeasure<'a> {
    fonts: &'a [PrintFontSpec],
    attributes: Vec<Option<CFRetained<CFDictionary<CFString, CTFont>>>>,
    ascii_widths: Vec<[Option<f64>; 128]>,
    seeded_widths: Vec<BTreeMap<u32, f64>>,
    other_widths: BTreeMap<(usize, u32), f64>,
    style_fonts: Vec<usize>,
    style_remap: Vec<usize>,
    spans: Arc<Vec<PrintStyleSpan>>,
    control_font: usize,
}

impl<'a> PrintMeasure<'a> {
    pub fn from_specs(
        fonts: &'a [PrintFontSpec],
        style_fonts: Vec<usize>,
        style_remap: Vec<usize>,
        spans: Arc<Vec<PrintStyleSpan>>,
        control_font: usize,
    ) -> io::Result<Self> {
        if style_fonts.is_empty() || control_font >= fonts.len() || style_remap.len() != style_fonts.len()
            || style_fonts.iter().any(|font| *font >= fonts.len()) {
            return Err(io::Error::other("print font or style mapping is invalid"));
        }
        let mut result = Self {
            fonts, attributes: (0..fonts.len()).map(|_| None).collect(),
            ascii_widths: vec![[None; 128]; fonts.len()], seeded_widths: vec![BTreeMap::new(); fonts.len()],
            other_widths: BTreeMap::new(), style_fonts, style_remap, spans, control_font,
        };
        for (index, font) in fonts.iter().enumerate() {
            result.ascii_widths[index] = font.ascii_widths;
            result.seeded_widths[index] = font.other_widths.clone();
        }
        Ok(result)
    }

    pub fn text_width(&mut self, font: usize, text: &str) -> io::Result<f64> {
        objc2::rc::autoreleasepool(|_| self.native_text_width(font, text))
    }

    fn native_text_width(&mut self, font: usize, text: &str) -> io::Result<f64> {
        if self.attributes.get(font).is_some_and(Option::is_none) {
            let font_spec = self.fonts.get(font).ok_or_else(|| io::Error::other("print font is unavailable"))?;
            let native_font = font_spec.create()?;
            self.attributes[font] = Some(CFDictionary::from_slices(&[unsafe { kCTFontAttributeName }], &[&*native_font]));
        }
        let attributes = self.attributes.get(font).and_then(Option::as_ref)
            .ok_or_else(|| io::Error::other("print font is unavailable"))?;
        let text = CFString::from_str(text);
        let attributed = unsafe { CFAttributedString::new(None, Some(&text), Some(attributes.as_opaque())) }
            .ok_or_else(|| io::Error::other("could not prepare print text"))?;
        let line = unsafe { CTLine::with_attributed_string(&attributed) };
        let width = unsafe { line.typographic_bounds(ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) };
        if !width.is_finite() || width < 0. {
            return Err(io::Error::other("print text measurement is invalid"));
        }
        Ok(width)
    }

    fn unit_width(&mut self, font: usize, value: ByteValue) -> io::Result<f64> {
        let key = match value {
            ByteValue::Scalar(character) => u32::from(character),
            ByteValue::Invalid(byte) => 0x110000 + u32::from(byte),
        };
        if key < 128 {
            if let Some(width) = self.ascii_widths[font][key as usize] { return Ok(width); }
        } else if let Some(width) = self.seeded_widths[font].get(&key) { return Ok(*width); }
        else if let Some(width) = self.other_widths.get(&(font, key)) { return Ok(*width); }
        let width = self.text_width(font, &print_unit_text(value))?;
        if key < 128 {
            self.ascii_widths[font][key as usize] = Some(width);
        } else {
            if self.other_widths.len() >= PRINT_WIDTH_CACHE_ENTRIES { self.other_widths.clear(); }
            self.other_widths.insert((font, key), width);
        }
        Ok(width)
    }
}

impl PrintMetrics for PrintMeasure<'_> {
    fn measure(&mut self, value: ByteValue, source: Range<usize>) -> io::Result<PrintUnitMetrics> {
        let index = self.spans.partition_point(|span| span.range.end <= source.start);
        let style = self.spans.get(index).filter(|span| span.range.contains(&source.start))
            .map_or(0, |span| span.style);
        let representation = print_unit_is_representation(value);
        let font = if representation { self.control_font } else {
            *self.style_fonts.get(style).ok_or_else(|| io::Error::other("print style font is unavailable"))?
        };
        let width = self.unit_width(font, value)? + if representation { 3. } else { 0. };
        let style = *self.style_remap.get(style).ok_or_else(|| io::Error::other("print style is unavailable"))?;
        Ok(PrintUnitMetrics { style, width })
    }
}
