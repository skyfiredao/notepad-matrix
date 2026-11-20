use super::*;
use objc2::{DefinedClass, MainThreadOnly, class, define_class, msg_send};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2_app_kit::{NSFont, NSPrintInfo, NSPrintOperation, NSPrintingPaginationMode, NSView, NSRectFill};
use objc2_foundation::{NSCopying, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString, NSDate, NSRunLoop, NSDefaultRunLoopMode};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "print_layout.rs"]
mod print_layout;
#[path = "print_measure.rs"]
mod print_measure;

use print_layout::{PrintGeometry, PrintPageIndex, PrintRun, print_language_has_styles, print_unit_is_representation, print_unit_text};
use print_measure::{PrintFontSpec, PrintMeasure, PrintStyleSpan};

enum PrintSyntax {
    None,
    Builtin(crate::languages::LanguageProfile, bool),
    UserDefined(crate::udl::Definition),
}

struct PrintSnapshot {
    bytes: crate::byte_buffer::ByteBuffer,
    range: std::ops::Range<usize>,
    first_line: usize,
    title: String,
    profile: crate::syntax::StyleAppearance,
    controls: crate::syntax::StyleAppearance,
    number: Option<crate::syntax::StyleAppearance>,
    syntax: PrintSyntax,
    styles: Vec<crate::syntax::StyleAppearance>,
    active_fonts: Vec<bool>,
    spans: Vec<PrintStyleSpan>,
    tabs: crate::input::TabPreferences,
    settings: serde_json::Value,
}

struct PrintDocument {
    bytes: crate::byte_buffer::ByteBuffer,
    range: std::ops::Range<usize>,
    first_line: usize,
    fonts: Vec<PrintFontSpec>,
    style_fonts: Vec<usize>,
    style_remap: Vec<usize>,
    spans: Arc<Vec<PrintStyleSpan>>,
    control_font: usize,
}

impl PrintDocument {
    fn metrics(&self) -> io::Result<PrintMeasure<'_>> {
        PrintMeasure::from_specs(&self.fonts, self.style_fonts.clone(), self.style_remap.clone(), self.spans.clone(), self.control_font)
    }

    fn paginate(&self, geometry: PrintGeometry, cancelled: &AtomicBool) -> io::Result<Arc<PrintPageIndex>> {
        let mut metrics = self.metrics()?;
        PrintPageIndex::build_cancellable(&self.bytes, self.range.clone(), self.first_line, geometry,
            &mut metrics, cancelled).map(Arc::new)
    }
}

struct PrintNativeStyle {
    attributes: Retained<AnyObject>,
    control_attributes: Retained<AnyObject>,
    foreground: u32,
    background: u32,
    ascent: f64,
}

struct PrintPending {
    geometry: PrintGeometry,
    result: mpsc::Receiver<io::Result<Arc<PrintPageIndex>>>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for PrintPending {
    fn drop(&mut self) { self.cancelled.store(true, Ordering::Relaxed); }
}

struct PrintState {
    document: Arc<PrintDocument>,
    styles: Vec<PrintNativeStyle>,
    info: Retained<NSPrintInfo>,
    title: String,
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
    ascent: f64,
    control_height: f64,
    control_ascent: f64,
    background: u32,
    layout: RefCell<Option<Arc<PrintPageIndex>>>,
    pending: RefCell<Option<PrintPending>>,
    pagination_generation: Cell<usize>,
    layout_error: RefCell<Option<io::Error>>,
    error: RefCell<Option<io::Error>>,
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
    fn geometry(&self, info: &NSPrintInfo) -> io::Result<PrintGeometry> {
        print_margins(info, &self.settings);
        let width = info.paperSize().width - info.leftMargin() - info.rightMargin();
        let height = info.paperSize().height - info.topMargin() - info.bottomMargin();
        if width <= 0. || height <= 0. { return Err(io::Error::other("print margins exceed the paper size")); }
        let inset = self.footer_height * 1.5;
        let top = inset + if self.headers.iter().any(|text| !text.is_empty()) { self.header_height * 1.5 } else { 0. };
        let bottom = (height - inset - if self.footers.iter().any(|text| !text.is_empty()) { self.footer_height * 1.5 } else { 0. }).max(top + self.line_height);
        PrintGeometry { width, height, inset, body_top: top, body_bottom: bottom,
            number_width: self.number_width, line_height: self.line_height, space_width: self.space_width,
            tab_size: self.tab_size, form_feed_page_break: self.settings["formFeedPageBreak"].as_bool().unwrap_or(false) }.validate()
    }

    fn ensure_layout(&self, info: &NSPrintInfo) -> io::Result<Arc<PrintPageIndex>> {
        let mtm = MainThreadMarker::new().ok_or_else(|| io::Error::other("printing requires the main thread"))?;
        let operation = NSPrintOperation::currentOperation(mtm);
        let generation = self.pagination_generation.get();
        let run_loop = NSRunLoop::currentRunLoop();
        let mode = run_loop.currentMode().unwrap_or_else(|| unsafe { NSDefaultRunLoopMode.to_owned() });
        loop {
            if self.pagination_generation.get() != generation || operation.as_ref().is_some_and(|operation| {
                NSPrintOperation::currentOperation(mtm).as_ref()
                    .is_none_or(|current| !std::ptr::eq(&**operation, &**current))
            }) {
                self.pending.borrow_mut().take();
                return Err(io::Error::new(io::ErrorKind::Interrupted, "native print operation stopped"));
            }
            let geometry = match self.geometry(info) {
                Ok(geometry) => geometry,
                Err(error) => {
                    self.pending.borrow_mut().take();
                    return Err(error);
                }
            };
            if self.pending.borrow().as_ref().is_some_and(|pending| pending.geometry != geometry) {
                self.pending.borrow_mut().take();
            }
            if let Some(layout) = self.layout.borrow().as_ref()
                && layout.matches(&self.document.range, self.document.first_line, geometry) {
                self.layout_error.borrow_mut().take();
                return Ok(layout.clone());
            }
            let ready = {
                let pending = self.pending.borrow();
                if let Some(pending) = pending.as_ref().filter(|pending| pending.geometry == geometry) {
                    match pending.result.try_recv() {
                        Ok(result) => Some(result),
                        Err(mpsc::TryRecvError::Empty) => None,
                        Err(mpsc::TryRecvError::Disconnected) => Some(Err(io::Error::other("print pagination worker stopped"))),
                    }
                } else { None }
            };
            if let Some(result) = ready {
                self.pending.borrow_mut().take();
                let layout = result?;
                *self.layout.borrow_mut() = Some(layout.clone());
                self.layout_error.borrow_mut().take();
                return Ok(layout);
            }
            let needs_worker = self.pending.borrow().as_ref().is_none_or(|pending| pending.geometry != geometry);
            if needs_worker {
                self.pending.borrow_mut().take();
                let document = self.document.clone();
                let cancelled = Arc::new(AtomicBool::new(false));
                let worker_cancelled = cancelled.clone();
                let (sender, result) = mpsc::channel();
                std::thread::Builder::new().name("matrix-print-pagination".into()).spawn(move || {
                    let result = objc2::rc::autoreleasepool(|_| document.paginate(geometry, &worker_cancelled));
                    if sender.send(result).is_err() { worker_cancelled.store(true, Ordering::Relaxed); }
                }).map_err(io::Error::other)?;
                *self.pending.borrow_mut() = Some(PrintPending { geometry, result, cancelled });
            }
            let limit = NSDate::dateWithTimeIntervalSinceNow(0.01);
            if !run_loop.runMode_beforeDate(&mode, &limit) { std::thread::yield_now(); }
        }
    }

    fn draw_parts(&self, parts: &[String; 3], attributes: &AnyObject, page: usize, y: f64, layout: PrintGeometry) {
        for (index, template) in parts.iter().enumerate() {
            let text = NSString::from_str(&template.replacen("$(CURRENT_PRINTING_PAGE)", &page.to_string(), 1));
            let size = print_size(&text, attributes);
            let x = match index { 0 => layout.inset + 5., 1 => (layout.width - size.width) / 2., _ => layout.width - layout.inset - size.width };
            print_draw(&text, attributes, x, y);
        }
    }

    fn draw_run(&self, run: PrintRun, offset: f64, layout: PrintGeometry) -> io::Result<()> {
        match run {
            PrintRun::Number { line, y } => {
                print_fill(self.number_background, NSRect::new(NSPoint::new(layout.inset, offset + y), NSSize::new(self.number_width, self.line_height)));
                let text = NSString::from_str(&format!("{line}   "));
                let width = print_size(&text, &self.number_attributes).width;
                print_draw(&text, &self.number_attributes, layout.inset + self.number_width - width, offset + y + self.ascent - self.number_ascent);
            }
            PrintRun::Representation { value, style, x, y, width } => {
                let style = &self.styles[style];
                let box_y = offset + y + self.ascent - self.control_height;
                print_fill(style.background, NSRect::new(NSPoint::new(x, offset + y), NSSize::new(width, self.line_height)));
                print_fill(style.foreground, NSRect::new(NSPoint::new(x + 1., box_y + 1.), NSSize::new((width - 1.).max(0.), (self.control_height - 1.).max(0.))));
                print_fill(style.foreground, NSRect::new(NSPoint::new(x + 2., box_y), NSSize::new((width - 3.).max(0.), self.control_height + 1.)));
                print_draw(&NSString::from_str(&print_unit_text(value)), &style.control_attributes, x + 2., offset + y + self.ascent - self.control_ascent);
            }
            PrintRun::Text { source, style, x, y, width } => {
                let style = &self.styles[style];
                let selected = self.document.bytes.slice(source).map_err(io::Error::other)?;
                let mut text = String::new();
                for unit in selected.units() {
                    match unit.value {
                        crate::byte_text::ByteValue::Scalar(character) => text.push(character),
                        crate::byte_text::ByteValue::Invalid(_) => return Err(io::Error::other("print text run contains an invalid byte")),
                    }
                }
                if style.background != self.background {
                    print_fill(style.background, NSRect::new(NSPoint::new(x, offset + y), NSSize::new(width, self.line_height)));
                }
                print_draw(&NSString::from_str(&text), &style.attributes, x, offset + y + self.ascent - style.ascent);
            }
        }
        Ok(())
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
            match self.ivars().ensure_layout(&info) {
                Ok(layout) => {
                    let geometry = layout.geometry();
                    self.setFrameSize(NSSize::new(geometry.width, geometry.height * layout.page_count() as f64));
                    if let Some(range) = unsafe { range.as_mut() } { *range = NSRange::new(1, layout.page_count()); }
                }
                Err(error) => {
                    *self.ivars().layout_error.borrow_mut() = Some(error);
                    if let Some(range) = unsafe { range.as_mut() } { *range = NSRange::new(1, 0); }
                }
            }
            Bool::YES
        }

        #[unsafe(method(rectForPage:))]
        fn page_rect(&self, page: isize) -> NSRect {
            let layout = self.ivars().layout.borrow();
            let Some(layout) = layout.as_ref() else { return NSRect::ZERO; };
            let geometry = layout.geometry();
            NSRect::new(NSPoint::new(0., page.saturating_sub(1).max(0) as f64 * geometry.height), NSSize::new(geometry.width, geometry.height))
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _rect: NSRect) {
            let state = self.ivars();
            let Some(layout) = state.layout.borrow().clone() else { return; };
            let geometry = layout.geometry();
            let result = (|| -> io::Result<()> {
                let operation = NSPrintOperation::currentOperation(self.mtm())
                    .ok_or_else(|| io::Error::other("native print operation is unavailable"))?;
                let page = usize::try_from(operation.currentPage()).ok()
                    .filter(|page| *page > 0 && *page <= layout.page_count())
                    .ok_or_else(|| io::Error::other("native print page is outside the document"))?;
                let mut metrics = state.document.metrics()?;
                let offset = (page - 1) as f64 * geometry.height;
                print_fill(state.background, NSRect::new(NSPoint::new(geometry.inset, offset + geometry.body_top), NSSize::new(geometry.width - geometry.inset * 2., geometry.body_bottom - geometry.body_top)));
                layout.draw_page_runs(&state.document.bytes, page, &mut metrics, |run| state.draw_run(run, offset, geometry))?;
                if state.headers.iter().any(|part| !part.is_empty()) {
                    state.draw_parts(&state.headers, &state.header_attributes, page, offset + geometry.inset, geometry);
                    print_fill(0, NSRect::new(NSPoint::new(geometry.inset, offset + geometry.body_top - state.header_height / 4.), NSSize::new(geometry.width - geometry.inset * 2., 0.5)));
                }
                if state.footers.iter().any(|part| !part.is_empty()) {
                    state.draw_parts(&state.footers, &state.footer_attributes, page, offset + geometry.body_bottom + state.footer_height / 2., geometry);
                    print_fill(0, NSRect::new(NSPoint::new(geometry.inset, offset + geometry.body_bottom + state.footer_height / 4.), NSSize::new(geometry.width - geometry.inset * 2., 0.5)));
                }
                Ok(())
            })();
            if let Err(error) = result { *state.error.borrow_mut() = Some(error); }
        }

        #[unsafe(method(endDocument))]
        fn end_document(&self) {
            self.ivars().pagination_generation.set(self.ivars().pagination_generation.get().wrapping_add(1));
            self.ivars().pending.borrow_mut().take();
            unsafe { let _: () = msg_send![super(self), endDocument]; }
        }
    }
);

fn print_dates() -> [String; 3] {
    use objc2_foundation::{NSDateFormatter, NSDateFormatterStyle};
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

fn prepare_print_styles(mut snapshot: PrintSnapshot) -> io::Result<PrintSnapshot> {
    if matches!(snapshot.syntax, PrintSyntax::None) { return Ok(snapshot); }
    let projection = crate::byte_projection::ByteProjection::from_bytes(&snapshot.bytes);
    let text = projection.text().to_string();
    let mut spans = Vec::new();
    match &snapshot.syntax {
        PrintSyntax::None => {}
        PrintSyntax::UserDefined(definition) => {
            let mut styles = BTreeMap::new();
            for span in crate::udl::lex(&text, definition).spans {
                let selected = projection.source_selection(Selection { anchor: ByteOffset(span.range.start), head: ByteOffset(span.range.end) })
                    .map_err(io::Error::other)?.range();
                let range = selected.start.0.max(snapshot.range.start)..selected.end.0.min(snapshot.range.end);
                if range.start >= range.end { continue; }
                let style = *styles.entry(span.style).or_insert_with(|| {
                    let appearance = crate::syntax::udl_appearance_with_profile(definition, span.style, &snapshot.profile, 0);
                    let index = snapshot.styles.iter().position(|style| *style == appearance).unwrap_or(snapshot.styles.len());
                    if index == snapshot.styles.len() { snapshot.styles.push(appearance); }
                    index
                });
                spans.push(PrintStyleSpan { range, style });
            }
        }
        PrintSyntax::Builtin(definition, dark) => {
            let mut styles = BTreeMap::new();
            for span in crate::lexers::lex(&text, definition).map_err(io::Error::other)?.spans {
                let selected = projection.source_selection(Selection { anchor: ByteOffset(span.range.start), head: ByteOffset(span.range.end) })
                    .map_err(io::Error::other)?.range();
                let range = selected.start.0.max(snapshot.range.start)..selected.end.0.min(snapshot.range.end);
                if range.start >= range.end { continue; }
                let style = *styles.entry(span.style).or_insert_with(|| {
                    let appearance = crate::builtin_language::style_appearance(definition, span.style, *dark, &snapshot.profile, 0);
                    let index = snapshot.styles.iter().position(|style| *style == appearance).unwrap_or(snapshot.styles.len());
                    if index == snapshot.styles.len() { snapshot.styles.push(appearance); }
                    index
                });
                spans.push(PrintStyleSpan { range, style });
            }
        }
    }
    snapshot.syntax = PrintSyntax::None;
    snapshot.active_fonts = vec![false; snapshot.styles.len()];
    snapshot.active_fonts[0] = true;
    let selected = snapshot.bytes.slice(snapshot.range.clone()).map_err(io::Error::other)?;
    let mut span_index = 0;
    for unit in selected.units() {
        if print_unit_is_representation(unit.value) { continue; }
        let offset = snapshot.range.start + unit.range.start;
        while spans.get(span_index).is_some_and(|span| span.range.end <= offset) { span_index += 1; }
        let style = spans.get(span_index).filter(|span| span.range.contains(&offset)).map_or(0, |span| span.style);
        snapshot.active_fonts[style] = true;
    }
    snapshot.spans = spans;
    Ok(snapshot)
}

fn font_index(
    appearance: &crate::syntax::StyleAppearance,
    fonts: &mut Vec<Retained<NSFont>>,
    keys: &mut BTreeMap<(String, u32, bool, bool), usize>,
) -> io::Result<usize> {
    let key = (appearance.font_family.clone(), appearance.font_size, appearance.bold, appearance.italic);
    if let Some(index) = keys.get(&key) { return Ok(*index); }
    let font = print_font(&appearance.font_family, f64::from(appearance.font_size), appearance.bold, appearance.italic)?;
    let index = fonts.len();
    fonts.push(font);
    keys.insert(key, index);
    Ok(index)
}

fn prepare_print(snapshot: PrintSnapshot) -> io::Result<PrintState> {
    let PrintSnapshot { bytes, range, first_line, title, profile, controls, number, syntax: _, styles: appearances,
        active_fonts, spans, tabs, settings } = snapshot;
    MainThreadMarker::new().ok_or_else(|| io::Error::other("printing requires the main thread"))?;
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setHorizontalPagination(NSPrintingPaginationMode::Automatic);
    info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
    info.setHorizontallyCentered(false);
    info.setVerticallyCentered(false);
    print_margins(&info, &settings);
    let option = settings["printOption"].as_i64().unwrap_or(3);
    let mut native_fonts = Vec::new();
    let mut font_keys = BTreeMap::new();
    let profile_font = font_index(&profile, &mut native_fonts, &mut font_keys)?;
    let control_font = font_index(&controls, &mut native_fonts, &mut font_keys)?;
    let control_height = native_fonts[control_font].capHeight().ceil().max(1.);
    let control_ascent = native_fonts[control_font].ascender();
    let mut ascent = native_fonts[profile_font].ascender().max(control_ascent);
    let height = |font: &NSFont| (font.ascender() - font.descender() + font.leading()).ceil().max(1.);
    let mut line_height = height(&native_fonts[profile_font]).max(height(&native_fonts[control_font]));
    let attributes = print_attributes(&native_fonts[profile_font], print_color(profile.foreground, option, false), profile.underline);
    let space_width = print_size(&NSString::from_str(" "), &attributes).width.max(1.);
    let (number_attributes, number_width, number_background, number_ascent) = if let Some(number) = number {
        let font = print_font(&number.font_family, f64::from(number.font_size), number.bold, number.italic)?;
        ascent = ascent.max(font.ascender());
        line_height = line_height.max(height(&font));
        let attributes = print_attributes(&font, print_color(number.foreground, option, false), number.underline);
        let width = print_size(&NSString::from_str("99999   "), &attributes).width;
        (attributes, width, if option == 0 { number.background } else { 0xffffff }, font.ascender())
    } else { (attributes.clone(), 0., 0xffffff, 0.) };
    let mut styles = Vec::new();
    let mut style_fonts = Vec::new();
    let mut style_keys = BTreeMap::new();
    let mut remap = Vec::new();
    for (index, appearance) in appearances.iter().enumerate() {
        let font = font_index(appearance, &mut native_fonts, &mut font_keys)?;
        style_fonts.push(font);
        if active_fonts[index] {
            ascent = ascent.max(native_fonts[font].ascender());
            line_height = line_height.max(height(&native_fonts[font]));
        }
        let foreground = print_color(appearance.foreground, option, false);
        let background = print_color(appearance.background, option, true);
        let key = (font, foreground, background, appearance.underline);
        let style = if let Some(style) = style_keys.get(&key) { *style } else {
            let style = styles.len();
            styles.push(PrintNativeStyle {
                attributes: print_attributes(&native_fonts[font], foreground, appearance.underline),
                control_attributes: print_attributes(&native_fonts[control_font], background, controls.underline),
                foreground, background, ascent: native_fonts[font].ascender(),
            });
            style_keys.insert(key, style);
            style
        };
        remap.push(style);
    }
    let mut fonts = Vec::new();
    for font in &native_fonts {
        let attributes = print_attributes(font, 0, false);
        let mut ascii_widths = [None; 128];
        for (index, width) in ascii_widths.iter_mut().enumerate() {
            let text = print_unit_text(crate::byte_text::ByteValue::Scalar(char::from(index as u8)));
            *width = Some(print_size(&NSString::from_str(&text), &attributes).width);
        }
        let mut other_widths = BTreeMap::new();
        for byte in 0..=255u8 {
            let text = print_unit_text(crate::byte_text::ByteValue::Invalid(byte));
            other_widths.insert(0x110000 + u32::from(byte), print_size(&NSString::from_str(&text), &attributes).width);
        }
        for character in ('\u{80}'..='\u{9f}').chain(['\u{2028}', '\u{2029}']) {
            let text = print_unit_text(crate::byte_text::ByteValue::Scalar(character));
            other_widths.insert(u32::from(character), print_size(&NSString::from_str(&text), &attributes).width);
        }
        fonts.push(PrintFontSpec::from_font(font, ascii_widths, other_widths)?);
    }
    let dates = print_dates();
    let file = Path::new(&title);
    let expand = |template: &str| {
        template.replace("$(FULL_CURRENT_PATH)", &title)
            .replace("$(FILE_NAME)", &file.file_name().map_or_else(|| title.clone(), |name| name.to_string_lossy().into_owned()))
            .replace("$(CURRENT_DIRECTORY)", &file.parent().map_or_else(String::new, |path| path.to_string_lossy().into_owned()))
            .replacen("$(SHORT_DATE)", &dates[0], 1).replacen("$(LONG_DATE)", &dates[1], 1).replacen("$(TIME)", &dates[2], 1)
    };
    let headers = ["headerLeft", "headerMiddle", "headerRight"].map(|key| expand(settings[key].as_str().unwrap_or("")));
    let footers = ["footerLeft", "footerMiddle", "footerRight"].map(|key| expand(settings[key].as_str().unwrap_or("")));
    let header_font = print_font(settings["headerFontName"].as_str().filter(|name| !name.is_empty()).unwrap_or("Arial"), settings["headerFontSize"].as_f64().filter(|size| *size > 0.).unwrap_or(9.), settings["headerFontStyle"].as_i64().unwrap_or(0) & 1 != 0, settings["headerFontStyle"].as_i64().unwrap_or(0) & 2 != 0)?;
    let footer_font = print_font(settings["footerFontName"].as_str().filter(|name| !name.is_empty()).unwrap_or("Arial"), settings["footerFontSize"].as_f64().filter(|size| *size > 0.).unwrap_or(9.), settings["footerFontStyle"].as_i64().unwrap_or(0) & 1 != 0, settings["footerFontStyle"].as_i64().unwrap_or(0) & 2 != 0)?;
    let header_height = height(&header_font);
    let footer_height = height(&footer_font);
    let document = Arc::new(PrintDocument { bytes, range, first_line, fonts, style_fonts,
        style_remap: remap, spans: Arc::new(spans), control_font });
    Ok(PrintState {
        document, styles, info, title, line_height, space_width, tab_size: tabs.size.max(1),
        headers, footers, header_attributes: print_attributes(&header_font, 0, false),
        footer_attributes: print_attributes(&footer_font, 0, false), number_attributes,
        header_height, footer_height, number_width, number_background, number_ascent,
        ascent, control_height, control_ascent, background: print_color(profile.background, option, true), settings,
        layout: RefCell::new(None), pending: RefCell::new(None), pagination_generation: Cell::new(0),
        layout_error: RefCell::new(None), error: RefCell::new(None),
    })
}

fn run_print(state: PrintState, show_panel: bool) -> io::Result<bool> {
    let mtm = MainThreadMarker::new().ok_or_else(|| io::Error::other("printing requires the main thread"))?;
    let layout = state.layout.borrow().clone().ok_or_else(|| io::Error::other("print pagination is unavailable"))?;
    let geometry = layout.geometry();
    let info = state.info.clone();
    let title = Path::new(&state.title).file_stem().and_then(|name| name.to_str()).unwrap_or(&state.title).to_owned();
    let allocated = MatrixPrintView::alloc(mtm).set_ivars(state);
    let view: Retained<MatrixPrintView> = unsafe { msg_send![super(allocated), initWithFrame: NSRect::new(NSPoint::new(0., 0.), NSSize::new(geometry.width, geometry.height * layout.page_count() as f64))] };
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setJobTitle(Some(&NSString::from_str(&title)));
    operation.setShowsPrintPanel(show_panel);
    operation.setShowsProgressPanel(true);
    let result = operation.runOperation();
    view.ivars().pending.borrow_mut().take();
    if let Some(error) = view.ivars().error.borrow_mut().take() { return Err(error); }
    if let Some(error) = view.ivars().layout_error.borrow_mut().take() {
        if error.kind() == io::ErrorKind::Interrupted { return Ok(false); }
        return Err(error);
    }
    Ok(result)
}

impl MatrixWindow {
    pub(super) fn print_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.print_document_mode(true, window, cx);
    }

    pub(super) fn print_document_mode(&mut self, show_panel: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.print_document_mode(show_panel, window, cx)) { return; }
        if self.printing || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
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
        let syntax = match self.print_syntax(language) {
            Ok(syntax) => syntax,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let styles = vec![profile.clone()];
        let snapshot = PrintSnapshot { bytes, range, first_line, title, profile, controls, number, syntax,
            styles, active_fonts: vec![true], spans: Vec::new(), tabs, settings };
        self.printing = true;
        cx.notify();
        let executor = cx.background_executor().clone();
        let preparing = executor.spawn(async move { prepare_print_styles(snapshot) });
        cx.spawn(async move |view, cx| {
            let result = async {
                let snapshot = preparing.await?;
                let state = prepare_print(snapshot)?;
                let geometry = state.geometry(&state.info)?;
                let document = state.document.clone();
                let layout = executor.spawn(async move {
                    objc2::rc::autoreleasepool(|_| document.paginate(geometry, &AtomicBool::new(false)))
                }).await?;
                *state.layout.borrow_mut() = Some(layout);
                run_print(state, show_panel)
            }.await;
            if let Err(error) = view.update(cx, |view, cx| {
                view.printing = false;
                if let Err(error) = result { view.notify_editor_error(format!("Could not print: {error}"), cx); }
                cx.notify();
            }) { eprintln!("printing lost its document: {error}"); }
        }).detach();
    }

    fn print_syntax(&self, language: crate::syntax::Language) -> io::Result<PrintSyntax> {
        if !print_language_has_styles(language) { return Ok(PrintSyntax::None); }
        if let crate::syntax::Language::UserDefined(id) = language {
            return self.udl.registry.definition(id).cloned().map(PrintSyntax::UserDefined)
                .ok_or_else(|| io::Error::other("print user language is unavailable"));
        }
        let id = match language { crate::syntax::Language::Builtin(id) => id, crate::syntax::Language::Json => 57, _ => return Ok(PrintSyntax::None) };
        let catalog = crate::languages::catalog().map_err(io::Error::other)?;
        let definition = catalog.by_id(id).ok_or_else(|| io::Error::other("print language is unavailable"))?;
        let mut definition = self.style_config.effective_language(definition, self.dark);
        self.apply_editing_language_properties(&mut definition);
        Ok(PrintSyntax::Builtin(definition, self.dark))
    }
}
