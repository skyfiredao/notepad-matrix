use super::*;
use objc2::{DefinedClass, MainThreadOnly, class, define_class, msg_send};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2_app_kit::{NSFont, NSPrintInfo, NSPrintOperation, NSPrintingPaginationMode, NSView, NSRectFill};
use objc2_foundation::{NSCopying, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::path::Path;

struct PrintSpan {
    range: std::ops::Range<usize>,
    appearance: crate::syntax::StyleAppearance,
}

struct PrintSnapshot {
    bytes: crate::byte_buffer::ByteBuffer,
    range: std::ops::Range<usize>,
    first_line: usize,
    title: String,
    profile: crate::syntax::StyleAppearance,
    controls: crate::syntax::StyleAppearance,
    number: Option<crate::syntax::StyleAppearance>,
    spans: Vec<PrintSpan>,
    tabs: crate::input::TabPreferences,
    settings: serde_json::Value,
}

struct PrintGlyph {
    character: Option<char>,
    source: std::ops::Range<usize>,
    representation: bool,
    text: Retained<NSString>,
    attributes: Retained<AnyObject>,
    width: f64,
    background: u32,
    foreground: u32,
    ascent: f64,
}

struct PrintPosition { glyph: usize, x: f64, y: f64, page: usize }
struct PrintNumber { text: String, y: f64, page: usize }

struct PrintPage {
    positions: std::ops::Range<usize>,
    numbers: std::ops::Range<usize>,
}

#[derive(Default)]
struct PrintLayout {
    positions: Vec<PrintPosition>,
    numbers: Vec<PrintNumber>,
    page_ranges: Vec<PrintPage>,
    pages: usize,
    width: f64,
    height: f64,
    inset: f64,
    body_top: f64,
    body_bottom: f64,
}

struct PrintState {
    glyphs: Vec<PrintGlyph>,
    line_height: f64,
    space_width: f64,
    tab_size: usize,
    settings: serde_json::Value,
    headers: [String; 3],
    footers: [String; 3],
    header_attributes: Retained<AnyObject>,
    footer_attributes: Retained<AnyObject>,
    number_attributes: Retained<AnyObject>,
    header_height: f64,
    footer_height: f64,
    number_width: f64,
    number_background: u32,
    number_ascent: f64,
    first_line: usize,
    ascent: f64,
    control_height: f64,
    control_ascent: f64,
    background: u32,
    layout: RefCell<PrintLayout>,
}

fn print_color(color: u32, option: i64, background: bool) -> u32 {
    match option {
        1 => {
            let channels = [(color >> 16) & 255, (color >> 8) & 255, color & 255];
            let light = channels.iter().sum::<u32>() / 3;
            if light == 0 { return 0xffffff; }
            let inverted = channels.map(|channel| (channel * (255 - light) / light).min(255));
            (inverted[0] << 16) | (inverted[1] << 8) | inverted[2]
        }
        2 => if background { 0xffffff } else { 0 },
        3 if background => 0xffffff,
        _ => color,
    }
}

fn native_color(color: u32) -> Retained<AnyObject> {
    unsafe { msg_send![class!(NSColor), colorWithSRGBRed: f64::from((color >> 16) & 255) / 255., green: f64::from((color >> 8) & 255) / 255., blue: f64::from(color & 255) / 255., alpha: 1f64] }
}

fn print_font(name: &str, size: f64, bold: bool, italic: bool) -> io::Result<Retained<NSFont>> {
    let font = NSFont::fontWithName_size(&NSString::from_str(name), size)
        .unwrap_or_else(|| NSFont::systemFontOfSize(size));
    if !bold && !italic { return Ok(font); }
    let font = unsafe {
        let manager: Retained<AnyObject> = msg_send![class!(NSFontManager), sharedFontManager];
        let mask: usize = (if bold { 2 } else { 0 }) | (if italic { 1 } else { 0 });
        msg_send![&*manager, convertFont: &*font, toHaveTrait: mask]
    };
    Ok(font)
}

fn print_attributes(font: &NSFont, color: u32, underline: bool) -> Retained<AnyObject> {
    let keys = [NSString::from_str("NSFont"), NSString::from_str("NSColor"), NSString::from_str("NSUnderline")];
    let color = native_color(color);
    unsafe {
        let number: Retained<AnyObject> = msg_send![class!(NSNumber), numberWithInteger: if underline { 1isize } else { 0isize }];
        let objects = [font as *const NSFont as *const AnyObject, &*color as *const AnyObject, &*number as *const AnyObject];
        let keys = keys.each_ref().map(|key| &**key as *const NSString as *const AnyObject);
        msg_send![class!(NSDictionary), dictionaryWithObjects: objects.as_ptr(), forKeys: keys.as_ptr(), count: 3usize]
    }
}

fn print_size(text: &NSString, attributes: &AnyObject) -> NSSize {
    unsafe { msg_send![text, sizeWithAttributes: attributes] }
}

fn print_draw(text: &NSString, attributes: &AnyObject, x: f64, y: f64) {
    unsafe { let _: () = msg_send![text, drawAtPoint: NSPoint::new(x, y), withAttributes: attributes]; }
}

fn print_fill(color: u32, rect: NSRect) {
    let color = native_color(color);
    unsafe { let _: () = msg_send![&*color, setFill]; NSRectFill(rect); }
}

fn print_margins(info: &NSPrintInfo, settings: &serde_json::Value) {
    let scale = 72. / 25.4;
    let imageable = info.imageablePageBounds();
    let paper = info.paperSize();
    info.setLeftMargin((settings["margeLeft"].as_f64().unwrap_or(0.).max(0.) * scale).max(imageable.origin.x));
    info.setRightMargin((settings["margeRight"].as_f64().unwrap_or(0.).max(0.) * scale).max(paper.width - imageable.origin.x - imageable.size.width));
    info.setTopMargin((settings["margeTop"].as_f64().unwrap_or(0.).max(0.) * scale).max(paper.height - imageable.origin.y - imageable.size.height));
    info.setBottomMargin((settings["margeBottom"].as_f64().unwrap_or(0.).max(0.) * scale).max(imageable.origin.y));
}

impl PrintState {
    fn reflow(&self, width: f64, height: f64) {
        let inset = self.footer_height * 1.5;
        let header = self.headers.iter().any(|text| !text.is_empty());
        let footer = self.footers.iter().any(|text| !text.is_empty());
        let top = inset + if header { self.header_height * 1.5 } else { 0. };
        let bottom = (height - inset - if footer { self.footer_height * 1.5 } else { 0. }).max(top + self.line_height);
        let left = inset + self.number_width;
        let right = (width - inset).max(left + self.space_width);
        let mut layout = PrintLayout { pages: 1, width, height, inset, body_top: top, body_bottom: bottom, ..PrintLayout::default() };
        let mut x = left;
        let mut y = top;
        let mut line = self.first_line;
        let mut first_row = true;
        let mut previous_cr = false;
        for (index, glyph) in self.glyphs.iter().enumerate() {
            if glyph.character == Some('\n') && previous_cr { previous_cr = false; continue; }
            previous_cr = glyph.character == Some('\r');
            if glyph.character == Some('\u{000c}') && self.settings["formFeedPageBreak"].as_bool().unwrap_or(false) {
                layout.pages += 1; x = left; y = top; first_row = true; continue;
            }
            if matches!(glyph.character, Some('\r' | '\n')) {
                if first_row && self.number_width > 0. { layout.numbers.push(PrintNumber { text: line.to_string(), y, page: layout.pages }); }
                line += 1; x = left; y += self.line_height; first_row = true;
                if y + self.line_height > bottom { layout.pages += 1; y = top; }
                continue;
            }
            let advance_at = |x: f64| if glyph.character == Some('\t') {
                let tab = self.space_width * self.tab_size as f64;
                tab - (x - left) % tab
            } else { glyph.width };
            let starts_word = !glyph.character.is_some_and(char::is_whitespace)
                && (index == 0 || self.glyphs[index - 1].character.is_some_and(char::is_whitespace));
            if starts_word && x > left {
                let width: f64 = self.glyphs[index..].iter().take_while(|glyph| !glyph.character.is_some_and(char::is_whitespace)).map(|glyph| glyph.width).sum();
                if width <= right - left && x + width > right {
                    x = left; y += self.line_height; first_row = false;
                    if y + self.line_height > bottom { layout.pages += 1; y = top; }
                }
            }
            let mut advance = advance_at(x);
            if x + advance > right && x > left {
                x = left; y += self.line_height; first_row = false;
                if y + self.line_height > bottom { layout.pages += 1; y = top; }
                advance = advance_at(x);
            }
            if first_row && x == left && self.number_width > 0. {
                layout.numbers.push(PrintNumber { text: line.to_string(), y, page: layout.pages });
                first_row = false;
            }
            if glyph.character != Some('\t') { layout.positions.push(PrintPosition { glyph: index, x, y, page: layout.pages }); }
            x += advance;
        }
        if let Some(last) = self.glyphs.last() {
            if matches!(last.character, Some('\r' | '\n')) && layout.positions.iter().all(|position| position.page < layout.pages) && layout.numbers.iter().all(|number| number.page < layout.pages) {
                layout.pages = layout.pages.saturating_sub(1).max(1);
            }
        }
        let mut position = 0;
        let mut number = 0;
        for page in 1..=layout.pages {
            let start_position = position;
            let start_number = number;
            while layout.positions.get(position).is_some_and(|position| position.page == page) { position += 1; }
            while layout.numbers.get(number).is_some_and(|number| number.page == page) { number += 1; }
            layout.page_ranges.push(PrintPage { positions: start_position..position, numbers: start_number..number });
        }
        *self.layout.borrow_mut() = layout;
    }

    fn draw_parts(&self, parts: &[String; 3], attributes: &AnyObject, page: usize, y: f64, layout: &PrintLayout) {
        for (index, template) in parts.iter().enumerate() {
            let text = NSString::from_str(&template.replacen("$(CURRENT_PRINTING_PAGE)", &page.to_string(), 1));
            let size = print_size(&text, attributes);
            let x = match index { 0 => layout.inset + 5., 1 => (layout.width - size.width) / 2., _ => layout.width - layout.inset - size.width };
            print_draw(&text, attributes, x, y);
        }
    }
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PrintState]
    struct MatrixPrintView;

    unsafe impl NSObjectProtocol for MatrixPrintView {}

    impl MatrixPrintView {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> Bool { Bool::YES }

        #[unsafe(method(knowsPageRange:))]
        unsafe fn page_range(&self, range: *mut NSRange) -> Bool {
            let info = NSPrintOperation::currentOperation(self.mtm()).map(|operation| operation.printInfo()).unwrap_or_else(NSPrintInfo::sharedPrintInfo);
            print_margins(&info, &self.ivars().settings);
            let width = (info.paperSize().width - info.leftMargin() - info.rightMargin()).max(1.);
            let height = (info.paperSize().height - info.topMargin() - info.bottomMargin()).max(1.);
            self.ivars().reflow(width, height);
            let pages = self.ivars().layout.borrow().pages;
            self.setFrameSize(NSSize::new(width, height * pages as f64));
            if let Some(range) = unsafe { range.as_mut() } { *range = NSRange::new(1, pages); }
            Bool::YES
        }

        #[unsafe(method(rectForPage:))]
        fn page_rect(&self, page: isize) -> NSRect {
            let layout = self.ivars().layout.borrow();
            NSRect::new(NSPoint::new(0., page.saturating_sub(1).max(0) as f64 * layout.height), NSSize::new(layout.width, layout.height))
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, rect: NSRect) {
            let state = self.ivars();
            let layout = state.layout.borrow();
            if layout.height <= 0. { return; }
            let first = (rect.origin.y / layout.height).floor().max(0.) as usize + 1;
            let last = ((rect.origin.y + rect.size.height) / layout.height).ceil().max(1.) as usize;
            for page in first..=last.min(layout.pages) {
                let Some(ranges) = layout.page_ranges.get(page - 1) else { continue; };
                let offset = (page - 1) as f64 * layout.height;
                print_fill(state.background, NSRect::new(NSPoint::new(layout.inset, offset + layout.body_top), NSSize::new(layout.width - layout.inset * 2., layout.body_bottom - layout.body_top)));
                for number in &layout.numbers[ranges.numbers.clone()] {
                    print_fill(state.number_background, NSRect::new(NSPoint::new(layout.inset, offset + number.y), NSSize::new(state.number_width, state.line_height)));
                    let text = NSString::from_str(&format!("{}   ", number.text));
                    let width = print_size(&text, &state.number_attributes).width;
                    print_draw(&text, &state.number_attributes, layout.inset + state.number_width - width, offset + number.y + state.ascent - state.number_ascent);
                }
                let positions = &layout.positions[ranges.positions.clone()];
                let mut index = 0;
                while let Some(position) = positions.get(index) {
                    let glyph = &state.glyphs[position.glyph];
                    if glyph.representation {
                        let y = offset + position.y + state.ascent - state.control_height;
                        print_fill(glyph.background, NSRect::new(NSPoint::new(position.x, offset + position.y), NSSize::new(glyph.width, state.line_height)));
                        print_fill(glyph.foreground, NSRect::new(NSPoint::new(position.x + 1., y + 1.), NSSize::new((glyph.width - 1.).max(0.), (state.control_height - 1.).max(0.))));
                        print_fill(glyph.foreground, NSRect::new(NSPoint::new(position.x + 2., y), NSSize::new((glyph.width - 3.).max(0.), state.control_height + 1.)));
                        print_draw(&glyph.text, &glyph.attributes, position.x + 2., offset + position.y + state.ascent - state.control_ascent);
                        index += 1;
                        continue;
                    }
                    let mut text = String::new();
                    let mut right = position.x;
                    let mut source_end = glyph.source.start;
                    while let Some(next) = positions.get(index) {
                        let candidate = &state.glyphs[next.glyph];
                        if next.y != position.y || (next.x - right).abs() > 0.01 || candidate.background != glyph.background
                            || candidate.representation || candidate.source.start != source_end
                            || !std::ptr::eq(&*candidate.attributes, &*glyph.attributes) { break; }
                        text.push_str(&candidate.text.to_string());
                        right = next.x + candidate.width;
                        source_end = candidate.source.end;
                        index += 1;
                    }
                    if glyph.background != state.background { print_fill(glyph.background, NSRect::new(NSPoint::new(position.x, offset + position.y), NSSize::new(right - position.x, state.line_height))); }
                    print_draw(&NSString::from_str(&text), &glyph.attributes, position.x, offset + position.y + state.ascent - glyph.ascent);
                }
                if state.headers.iter().any(|part| !part.is_empty()) {
                    state.draw_parts(&state.headers, &state.header_attributes, page, offset + layout.inset, &layout);
                    print_fill(0, NSRect::new(NSPoint::new(layout.inset, offset + layout.body_top - state.header_height / 4.), NSSize::new(layout.width - layout.inset * 2., 0.5)));
                }
                if state.footers.iter().any(|part| !part.is_empty()) {
                    state.draw_parts(&state.footers, &state.footer_attributes, page, offset + layout.body_bottom + state.footer_height / 2., &layout);
                    print_fill(0, NSRect::new(NSPoint::new(layout.inset, offset + layout.body_bottom + state.footer_height / 4.), NSSize::new(layout.width - layout.inset * 2., 0.5)));
                }
            }
        }
    }
);

fn print_dates() -> [String; 3] {
    use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};
    let now = NSDate::date();
    let formatter = NSDateFormatter::new();
    formatter.setDateStyle(NSDateFormatterStyle::ShortStyle);
    formatter.setTimeStyle(NSDateFormatterStyle::NoStyle);
    let short = formatter.stringFromDate(&now).to_string();
    formatter.setDateStyle(NSDateFormatterStyle::LongStyle);
    let long = formatter.stringFromDate(&now).to_string();
    formatter.setDateStyle(NSDateFormatterStyle::NoStyle);
    formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
    [short, long, formatter.stringFromDate(&now).to_string()]
}

fn print_representation(value: crate::byte_text::ByteValue) -> Option<String> {
    const C0: [&str; 32] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR", "SO", "SI",
        "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US",
    ];
    const C1: [&str; 32] = [
        "PAD", "HOP", "BPH", "NBH", "IND", "NEL", "SSA", "ESA", "HTS", "HTJ", "VTS", "PLD", "PLU", "RI", "SS2", "SS3",
        "DCS", "PU1", "PU2", "STS", "CCH", "MW", "SPA", "EPA", "SOS", "SGCI", "SCI", "CSI", "ST", "OSC", "PM", "APC",
    ];
    match value {
        crate::byte_text::ByteValue::Invalid(byte) => Some(format!("x{byte:02X}")),
        crate::byte_text::ByteValue::Scalar(character) => match character {
            '\0'..='\u{1f}' => Some(C0[character as usize].into()),
            '\u{7f}' => Some("DEL".into()),
            '\u{80}'..='\u{9f}' => Some(C1[character as usize - 0x80].into()),
            '\u{2028}' => Some("LS".into()),
            '\u{2029}' => Some("PS".into()),
            _ => None,
        },
    }
}

fn print_text(snapshot: PrintSnapshot, show_panel: bool) -> io::Result<bool> {
    let PrintSnapshot { bytes, range, first_line, title, profile, controls, number, spans, tabs, settings } = snapshot;
    let title = title.as_str();
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| io::Error::other("printing requires the main thread"))?;
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setHorizontalPagination(NSPrintingPaginationMode::Automatic);
    info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
    info.setHorizontallyCentered(false);
    info.setVerticallyCentered(false);
    print_margins(&info, &settings);
    let font = print_font(&profile.font_family, f64::from(profile.font_size), profile.bold, profile.italic)?;
    let control_font = print_font(&controls.font_family, f64::from(controls.font_size), controls.bold, controls.italic)?;
    let control_height = control_font.capHeight().ceil().max(1.);
    let control_ascent = control_font.ascender();
    let option = settings["printOption"].as_i64().unwrap_or(3);
    let attributes = print_attributes(&font, print_color(profile.foreground, option, false), profile.underline);
    let space_width = print_size(&NSString::from_str(" "), &attributes).width.max(1.);
    let mut ascent = font.ascender().max(control_ascent);
    let mut line_height = (font.ascender() - font.descender() + font.leading()).ceil()
        .max((control_font.ascender() - control_font.descender() + control_font.leading()).ceil()).max(1.);
    let (number_attributes, number_width, number_background, number_ascent) = if let Some(number) = number {
        let font = print_font(&number.font_family, f64::from(number.font_size), number.bold, number.italic)?;
        ascent = ascent.max(font.ascender());
        line_height = line_height.max((font.ascender() - font.descender() + font.leading()).ceil());
        let attributes = print_attributes(&font, print_color(number.foreground, option, false), number.underline);
        let width = print_size(&NSString::from_str("99999   "), &attributes).width;
        (attributes, width, if option == 0 { number.background } else { 0xffffff }, font.ascender())
    } else { (attributes.clone(), 0., 0xffffff, 0.) };
    let mut glyphs = Vec::new();
    let mut cache = std::collections::BTreeMap::<(String, u32, bool, bool, bool, u32), (Retained<AnyObject>, f64)>::new();
    let mut span_index = 0;
    let selected = bytes.slice(range.clone()).map_err(|error| io::Error::other(error.to_string()))?;
    for unit in selected.units() {
        let source = range.start + unit.range.start..range.start + unit.range.end;
        let offset = source.start;
        let character = match unit.value { crate::byte_text::ByteValue::Scalar(character) => Some(character), crate::byte_text::ByteValue::Invalid(_) => None };
        let representation = print_representation(unit.value);
        while spans.get(span_index).is_some_and(|span| span.range.end <= offset) { span_index += 1; }
        let appearance = spans.get(span_index).filter(|span| span.range.contains(&offset)).map_or(&profile, |span| &span.appearance);
        let foreground = print_color(appearance.foreground, option, false);
        let background = print_color(appearance.background, option, true);
        let glyph_appearance = if representation.is_some() { &controls } else { appearance };
        let glyph_foreground = if representation.is_some() { background } else { foreground };
        let key = (glyph_appearance.font_family.clone(), glyph_appearance.font_size, glyph_appearance.bold, glyph_appearance.italic, glyph_appearance.underline, glyph_foreground);
        let (attributes, glyph_ascent) = if let Some((attributes, ascent)) = cache.get(&key) { (attributes.clone(), *ascent) } else {
            let font = print_font(&glyph_appearance.font_family, f64::from(glyph_appearance.font_size), glyph_appearance.bold, glyph_appearance.italic)?;
            ascent = ascent.max(font.ascender());
            line_height = line_height.max((font.ascender() - font.descender() + font.leading()).ceil());
            let attributes = print_attributes(&font, glyph_foreground, glyph_appearance.underline);
            let glyph_ascent = font.ascender();
            cache.insert(key, (attributes.clone(), glyph_ascent)); (attributes, glyph_ascent)
        };
        let represented = representation.is_some();
        let text = NSString::from_str(&representation.unwrap_or_else(|| character.map(|character| character.to_string()).unwrap_or_default()));
        let width = print_size(&text, &attributes).width + if represented { 3. } else { 0. };
        glyphs.push(PrintGlyph { character, source, representation: represented, text, attributes, width, background, foreground, ascent: glyph_ascent });
    }
    let dates = print_dates();
    let file = Path::new(title);
    let expand = |template: &str| {
        template.replace("$(FULL_CURRENT_PATH)", title)
            .replace("$(FILE_NAME)", &file.file_name().map_or_else(|| title.to_owned(), |name| name.to_string_lossy().into_owned()))
            .replace("$(CURRENT_DIRECTORY)", &file.parent().map_or_else(String::new, |path| path.to_string_lossy().into_owned()))
            .replacen("$(SHORT_DATE)", &dates[0], 1).replacen("$(LONG_DATE)", &dates[1], 1).replacen("$(TIME)", &dates[2], 1)
    };
    let headers = ["headerLeft", "headerMiddle", "headerRight"].map(|key| expand(settings[key].as_str().unwrap_or("")));
    let footers = ["footerLeft", "footerMiddle", "footerRight"].map(|key| expand(settings[key].as_str().unwrap_or("")));
    let header_font = print_font(settings["headerFontName"].as_str().filter(|name| !name.is_empty()).unwrap_or("Arial"), settings["headerFontSize"].as_f64().filter(|size| *size > 0.).unwrap_or(9.), settings["headerFontStyle"].as_i64().unwrap_or(0) & 1 != 0, settings["headerFontStyle"].as_i64().unwrap_or(0) & 2 != 0)?;
    let footer_font = print_font(settings["footerFontName"].as_str().filter(|name| !name.is_empty()).unwrap_or("Arial"), settings["footerFontSize"].as_f64().filter(|size| *size > 0.).unwrap_or(9.), settings["footerFontStyle"].as_i64().unwrap_or(0) & 1 != 0, settings["footerFontStyle"].as_i64().unwrap_or(0) & 2 != 0)?;
    let header_height = (header_font.ascender() - header_font.descender() + header_font.leading()).ceil();
    let footer_height = (footer_font.ascender() - footer_font.descender() + footer_font.leading()).ceil();
    let state = PrintState {
        glyphs, line_height, space_width, tab_size: tabs.size.max(1),
        headers, footers, header_attributes: print_attributes(&header_font, 0, false), footer_attributes: print_attributes(&footer_font, 0, false),
        number_attributes, header_height, footer_height,
        number_width, number_background, number_ascent, first_line, ascent, control_height, control_ascent,
        background: print_color(profile.background, option, true), settings, layout: RefCell::new(PrintLayout::default()),
    };
    let width = info.paperSize().width - info.leftMargin() - info.rightMargin();
    let height = info.paperSize().height - info.topMargin() - info.bottomMargin();
    if width <= 0. || height <= 0. { return Err(io::Error::other("print margins exceed the paper size")); }
    state.reflow(width, height);
    let pages = state.layout.borrow().pages;
    let allocated = MatrixPrintView::alloc(mtm).set_ivars(state);
    let view: Retained<MatrixPrintView> = unsafe { msg_send![super(allocated), initWithFrame: NSRect::new(NSPoint::new(0., 0.), NSSize::new(width, height * pages as f64))] };
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    let title = Path::new(title)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(title);
    operation.setJobTitle(Some(&NSString::from_str(title)));
    operation.setShowsPrintPanel(show_panel);
    operation.setShowsProgressPanel(true);
    Ok(operation.runOperation())
}

impl MatrixWindow {
    pub(super) fn print_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.print_document_mode(true, window, cx);
    }

    pub(super) fn print_document_mode(
        &mut self,
        show_panel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.print_document_mode(show_panel, window, cx)
        }) {
            return;
        }
        if self.printing
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let bytes = self.editor().document().byte_snapshot();
        let selection = match self.editor().source_selection_limits() {
            Ok(selection) => selection.range(),
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let range = if show_panel && !selection.is_empty() { selection.start.0..selection.end.0 } else { 0..bytes.len() };
        let first_line = match bytes.line_index(range.start) {
            Ok(line) => line + 1,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let title = self.workspace.active().disk_file.as_ref().map(|file| file.path().to_string_lossy().into_owned()).unwrap_or_else(|| self.workspace.active().name());
        let profile = self.editor_style_profile();
        let language = self.workspace.active().language();
        let tabs = self.editing_tab_preferences(language);
        let settings = self.preferences.settings["PrintSettings"].clone();
        let number = if self.editor_line_number_width() > px(0.) && settings["lineNumber"].as_bool().unwrap_or(true) {
            let id = self.style_config.global_style_named("Line number margin", self.dark).map_or(33, |style| style.id);
            Some(self.style_config.appearance(None, id, self.dark, &profile, 0))
        } else { None };
        let controls = self.style_config.appearance(None, 36, self.dark, &profile, 0);
        let spans = match self.print_style_spans(&bytes, language, &profile) {
            Ok(spans) => spans,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let snapshot = PrintSnapshot { bytes, range, first_line, title, profile, controls, number, spans, tabs, settings };
        self.printing = true;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = print_text(snapshot, show_panel);
            if let Err(error) = view.update(cx, |view, cx| {
                view.printing = false;
                match result {
                    Ok(true) => {}
                    Ok(false) => eprintln!("Print operation cancelled or did not complete"),
                    Err(error) => {
                        view.notify_editor_error(format!("Could not print: {error}"), cx);
                    }
                }
                cx.notify();
            }) {
                eprintln!("printing lost its document: {error}");
            }
        })
        .detach();
    }

    fn print_style_spans(&self, bytes: &crate::byte_buffer::ByteBuffer, language: crate::syntax::Language, profile: &crate::syntax::StyleAppearance) -> io::Result<Vec<PrintSpan>> {
        let projection = crate::byte_projection::ByteProjection::from_bytes(bytes);
        let text = projection.text().to_string();
        let spans: Vec<PrintSpan> = if let crate::syntax::Language::UserDefined(id) = language {
            let definition = self.udl.registry.definition(id).ok_or_else(|| io::Error::other("print user language is unavailable"))?;
            crate::udl::lex(&text, definition).spans.into_iter().map(|span| PrintSpan {
                range: span.range, appearance: crate::syntax::udl_appearance_with_profile(definition, span.style, profile, 0),
            }).collect()
        } else {
            let id = match language { crate::syntax::Language::Builtin(id) => id, crate::syntax::Language::Json => 57, _ => return Ok(Vec::new()) };
            let catalog = crate::languages::catalog().map_err(|error| io::Error::other(error.to_string()))?;
            let definition = catalog.by_id(id).ok_or_else(|| io::Error::other("print language is unavailable"))?;
            let mut definition = self.style_config.effective_language(definition, self.dark);
            self.apply_editing_language_properties(&mut definition);
            let result = crate::lexers::lex(&text, &definition).map_err(|error| io::Error::other(error.to_string()))?;
            result.spans.into_iter().map(|span| PrintSpan {
                range: span.range,
                appearance: crate::builtin_language::style_appearance(&definition, span.style, self.dark, profile, 0),
            }).collect()
        };
        spans.into_iter().map(|mut span| {
            let selection = projection.source_selection(Selection { anchor: ByteOffset(span.range.start), head: ByteOffset(span.range.end) })
                .map_err(|error| io::Error::other(error.to_string()))?.range();
            span.range = selection.start.0..selection.end.0;
            Ok(span)
        }).collect()
    }
}
