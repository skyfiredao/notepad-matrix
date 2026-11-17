use super::*;
use super::dialog_font::px;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::cell::Cell;
use super::text_field::RenderPalette;
use objc2::{class, msg_send};
use objc2::runtime::{AnyObject, Bool};

pub(super) fn dimensions() -> gpui::Size<gpui::Pixels> { size(px(1015.), px(411.25)) }

pub(super) fn source_x(x: f32, width: f32) -> f32 {
    if crate::localization::current_preference_rtl() { 805. - x - width } else { x }
}

fn source_position(x: f32, y: f32, width: f32) -> gpui::Div {
    div().absolute().left(px(source_x(x, width))).top(px(y)).w(px(width))
        .when(crate::localization::current_preference_rtl(), |element| element.text_right())
}

pub(super) fn source_rect(x: f32, y: f32, width: f32, height: f32) -> gpui::Div {
    source_position(x, y, width).h(px(height))
}

pub(super) fn colorref_to_rgb(value: u32) -> u32 {
    ((value & 0xff) << 16) | (value & 0xff00) | ((value >> 16) & 0xff)
}

pub(super) fn rgb_to_colorref(value: u32) -> u32 {
    colorref_to_rgb(value)
}

thread_local! {
    static RENDER_PALETTE: Cell<RenderPalette> = const { Cell::new(RenderPalette::LIGHT) };
}

pub(super) fn current_render_palette() -> RenderPalette {
    if MainThreadMarker::new().is_none() {
        eprintln!("preference render palette requires the UI main thread");
        return RenderPalette::LIGHT;
    }
    RENDER_PALETTE.with(Cell::get)
}

pub(super) fn sync_text_field_palette(entity: &gpui::Entity<text_field::TextField>, palette: RenderPalette, cx: &mut App) {
    if entity.read(cx).render_palette() != Some(palette) {
        entity.update(cx, |field, cx| field.set_render_palette(Some(palette), cx));
    }
}

struct RenderPaletteScope {
    previous: Option<RenderPalette>,
    _main_thread: Option<MainThreadMarker>,
}

impl RenderPaletteScope {
    fn new(palette: RenderPalette) -> Self {
        let main_thread = MainThreadMarker::new();
        let previous = if main_thread.is_some() {
            Some(RENDER_PALETTE.with(|current| current.replace(palette)))
        } else {
            eprintln!("preference rendering requires the UI main thread");
            None
        };
        Self { previous, _main_thread: main_thread }
    }
}

impl Drop for RenderPaletteScope {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            RENDER_PALETTE.with(|current| current.set(previous));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    General, Toolbar, Tabs, Editing1, Editing2, DarkMode, Margins, Highlight,
    NewDocument, Directories, RecentFiles, Associations, Language, Indentation,
    Printing, Searching, Backup, Completion, Instances, Delimiters, Performance,
    CloudLinks, SearchEngine, Miscellaneous,
}

impl Page {
    pub const ALL: [Self; 24] = [Self::General, Self::Toolbar, Self::Tabs, Self::Editing1, Self::Editing2,
        Self::DarkMode, Self::Margins, Self::NewDocument, Self::Directories,
        Self::RecentFiles, Self::Associations, Self::Language, Self::Indentation, Self::Highlight, Self::Printing,
        Self::Searching, Self::Backup, Self::Completion, Self::Instances, Self::Delimiters,
        Self::Performance, Self::CloudLinks, Self::SearchEngine, Self::Miscellaneous];
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "常用", Self::Toolbar => "工具栏", Self::Tabs => "标签栏",
            Self::Editing1 => "编辑1", Self::Editing2 => "编辑2", Self::DarkMode => "深色模式",
            Self::Margins => "边距/边框/边缘", Self::Highlight => "高亮",
            Self::NewDocument => "新建文件", Self::Directories => "默认目录",
            Self::RecentFiles => "最新文件记录", Self::Associations => "文件关联",
            Self::Language => "语言", Self::Indentation => "缩进", Self::Printing => "打印",
            Self::Searching => "搜索", Self::Backup => "备份", Self::Completion => "自动补全",
            Self::Instances => "多实例与日期", Self::Delimiters => "分割符",
            Self::Performance => "性能", Self::CloudLinks => "云同步与链接",
            Self::SearchEngine => "搜索引擎", Self::Miscellaneous => "其他",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Commit { Immediate, Blur }
#[derive(Clone)]
pub(super) struct FieldSpec {
    pub path: &'static str, pub default: Value, pub max_utf16: usize,
    pub integer: Option<(i64, i64)>, pub commit: Commit, pub scale: i64,
    pub unsigned_32: bool, pub signed_prefix_units: Option<usize>, pub invalid_zero: bool, pub retain_text: bool,
}
impl FieldSpec {
    pub fn text(path: &'static str, default: &str, max_utf16: usize) -> Self {
        Self { path, default: default.into(), max_utf16, integer: None, commit: Commit::Immediate, scale: 1, unsigned_32: false, signed_prefix_units: None, invalid_zero: false, retain_text: false }
    }
    pub fn integer(path: &'static str, default: i64, min: i64, max: i64, commit: Commit) -> Self {
        Self { path, default: default.into(), max_utf16: 20, integer: Some((min, max)), commit, scale: 1, unsigned_32: false, signed_prefix_units: None, invalid_zero: false, retain_text: false }
    }
    pub fn with_scale(mut self, scale: i64) -> Self { self.scale = scale; self }
    pub fn unsigned_32(mut self) -> Self { self.unsigned_32 = true; self.invalid_zero = true; self }
    pub fn signed_prefix(mut self, units: usize) -> Self { self.signed_prefix_units = Some(units); self.invalid_zero = true; self }
    pub fn retain_text(mut self) -> Self { self.retain_text = true; self }
}
struct BoundField {
    spec: FieldSpec, input: FieldInput, last: String,
    _changed: Option<gpui::Subscription>, _blur: Option<gpui::Subscription>,
}
enum FieldInput {
    Text(gpui::Entity<text_field::TextField>),
    Columns(NativeColumnField),
}
const COLUMN_PATH: &str = "ScintillaPrimaryView.edgeMultiColumnPos";

struct NativeColumnField {
    scroll: objc2::rc::Retained<AnyObject>,
    text: objc2::rc::Retained<AnyObject>,
    focus_pending: std::rc::Rc<Cell<bool>>,
}

impl NativeColumnField {
    fn new(value: &str) -> io::Result<Self> {
        use objc2::rc::{Allocated, Retained};
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
        unsafe {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(dialog_font::native_length(222.), dialog_font::native_length(61.)));
            let scroll: Allocated<AnyObject> = msg_send![class!(NSScrollView), alloc];
            let scroll: Option<Retained<AnyObject>> = msg_send![scroll, initWithFrame: frame];
            let scroll = scroll.ok_or_else(|| io::Error::other("无法创建列边线滚动区域"))?;
            let text: Allocated<AnyObject> = msg_send![class!(NSTextView), alloc];
            let text: Option<Retained<AnyObject>> = msg_send![text, initWithFrame: frame];
            let text = text.ok_or_else(|| io::Error::other("无法创建列边线文本区域"))?;
            let _: () = msg_send![&*text, setRichText: false];
            let _: () = msg_send![&*text, setEditable: true];
            let _: () = msg_send![&*text, setSelectable: true];
            let _: () = msg_send![&*text, setAllowsUndo: true];
            let _: () = msg_send![&*text, setAutomaticQuoteSubstitutionEnabled: false];
            let _: () = msg_send![&*text, setAutomaticDashSubstitutionEnabled: false];
            let _: () = msg_send![&*text, setAutomaticSpellingCorrectionEnabled: false];
            let _: () = msg_send![&*text, setHorizontallyResizable: false];
            let _: () = msg_send![&*text, setVerticallyResizable: true];
            let _: () = msg_send![&*text, setMinSize: NSSize::new(0., dialog_font::native_length(61.))];
            let _: () = msg_send![&*text, setMaxSize: NSSize::new(dialog_font::native_length(222.), f64::MAX)];
            let _: () = msg_send![&*text, setTextContainerInset: NSSize::new(dialog_font::native_length(3.), dialog_font::native_length(2.))];
            let container: *mut AnyObject = msg_send![&*text, textContainer];
            let _: () = msg_send![container, setWidthTracksTextView: true];
            let _: () = msg_send![container, setContainerSize: NSSize::new(dialog_font::native_length(222.), f64::MAX)];
            let _: () = msg_send![container, setLineBreakMode: 0usize];
            let font = dialog_font::native_font();
            let _: () = msg_send![&*text, setFont: &*font];
            let _: () = msg_send![&*text, setString: &*NSString::from_str(value)];
            let _: () = msg_send![&*scroll, setBorderType: 0usize];
            let _: () = msg_send![&*scroll, setHasVerticalScroller: false];
            let _: () = msg_send![&*scroll, setHasHorizontalScroller: false];
            let _: () = msg_send![&*scroll, setDocumentView: &*text];
            Ok(Self { scroll, text, focus_pending: std::rc::Rc::new(Cell::new(false)) })
        }
    }

    fn value(&self) -> Option<String> {
        unsafe {
            let marked: Bool = msg_send![&*self.text, hasMarkedText];
            if marked.as_bool() { return None; }
            let value: *const objc2_foundation::NSString = msg_send![&*self.text, string];
            value.as_ref().map(ToString::to_string)
        }
    }

    fn set_value(&self, value: &str) {
        unsafe {
            let selection: objc2_foundation::NSRange = msg_send![&*self.text, selectedRange];
            let _: () = msg_send![&*self.text, setString: &*objc2_foundation::NSString::from_str(value)];
            let length = value.encode_utf16().count();
            let location = selection.location.min(length);
            let selection = objc2_foundation::NSRange::new(location, selection.length.min(length - location));
            let _: () = msg_send![&*self.text, setSelectedRange: selection];
        }
    }

    fn focused(&self) -> bool {
        unsafe {
            let window: *mut AnyObject = msg_send![&*self.text, window];
            if window.is_null() { return false; }
            let responder: *mut AnyObject = msg_send![window, firstResponder];
            responder == (&*self.text as *const AnyObject).cast_mut()
        }
    }

    fn request_focus(&self) { self.focus_pending.set(true); }

    fn detach(&self) {
        self.focus_pending.set(false);
        unsafe {
            if self.focused() {
                let window: *mut AnyObject = msg_send![&*self.text, window];
                let _: Bool = msg_send![window, makeFirstResponder: std::ptr::null::<AnyObject>()];
            }
            let _: () = msg_send![&*self.scroll, removeFromSuperview];
        }
    }

    fn element(&self, palette: RenderPalette) -> AnyElement {
        use objc2::rc::Retained;
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
        unsafe {
            let color = |value: u32| -> Retained<AnyObject> {
                msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64]
            };
            let foreground = color(palette.text);
            let background = color(palette.softer_background);
            let selection_background = color(if palette.dark { palette.hot_background } else { 0x0078d7 });
            let selection_text = color(if palette.dark { palette.text } else { 0xffffff });
            let _: () = msg_send![&*self.text, setTextColor: &*foreground];
            let _: () = msg_send![&*self.text, setInsertionPointColor: &*foreground];
            let _: () = msg_send![&*self.text, setBackgroundColor: &*background];
            let _: () = msg_send![&*self.text, setAlignment: if crate::localization::current_preference_rtl() { 1usize } else { 0usize }];
            let label = crate::localization::current_preference_control(6238).unwrap_or("Column positions");
            let _: () = msg_send![&*self.text, setAccessibilityLabel: &*NSString::from_str(label)];
            let _: () = msg_send![&*self.scroll, setBackgroundColor: &*background];
            let attributes: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
            let _: () = msg_send![&*attributes, setObject: &*selection_background, forKey: &*NSString::from_str("NSBackgroundColor")];
            let _: () = msg_send![&*attributes, setObject: &*selection_text, forKey: &*NSString::from_str("NSColor")];
            let _: () = msg_send![&*self.text, setSelectedTextAttributes: &*attributes];
        }
        let scroll = self.scroll.clone();
        let text = self.text.clone();
        let focus_pending = self.focus_pending.clone();
        let canvas = gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            if let Ok(handle) = HasWindowHandle::window_handle(window) && let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                let top = f64::from(f32::from(bounds.top()));
                let height = f64::from(f32::from(bounds.size.height));
                let y = if view.isFlipped() { top } else { view.bounds().size.height - top - height };
                let frame = NSRect::new(NSPoint::new(f64::from(f32::from(bounds.left())), y), NSSize::new(f64::from(f32::from(bounds.size.width)), height));
                unsafe {
                    let _: () = msg_send![&*scroll, setFrame: frame];
                    let parent: *mut AnyObject = msg_send![&*scroll, superview];
                    if parent != (view as *const NSView).cast_mut().cast::<AnyObject>() { let _: () = msg_send![view, addSubview: &*scroll]; }
                    if focus_pending.get() {
                        let native_window: *mut AnyObject = msg_send![&*text, window];
                        if !native_window.is_null() {
                            let focused: Bool = msg_send![native_window, makeFirstResponder: &*text];
                            if focused.as_bool() { focus_pending.set(false); }
                        }
                    }
                }
            }
        }).size_full();
        div().size_full().border_1().border_color(rgb(if self.focused() { palette.hot_edge } else { palette.edge }))
            .bg(rgb(palette.softer_background)).child(canvas).into_any_element()
    }
}

impl Drop for NativeColumnField {
    fn drop(&mut self) { self.detach(); }
}

impl BoundField {
    fn text_entity(&self) -> Option<gpui::Entity<text_field::TextField>> {
        match &self.input { FieldInput::Text(entity) => Some(entity.clone()), FieldInput::Columns(_) => None }
    }
    fn value(&self, cx: &App) -> Option<String> {
        match &self.input {
            FieldInput::Text(entity) => { let field = entity.read(cx); if field.has_composition() { None } else { Some(field.text()) } }
            FieldInput::Columns(field) => field.value(),
        }
    }
    fn set_text(&self, text: &str, window: &mut Window, cx: &mut Context<MatrixWindow>) {
        match &self.input {
            FieldInput::Text(entity) => entity.update(cx, |field, cx| field.set_text(text, window, cx)),
            FieldInput::Columns(field) => field.set_value(text),
        }
    }
    fn focused(&self, window: &Window, cx: &App) -> bool {
        match &self.input { FieldInput::Text(entity) => entity.read(cx).is_focused(window), FieldInput::Columns(field) => field.focused() }
    }
}
struct Choice {
    path: &'static str, options: Vec<(Value, String)>,
    x: f32, y: f32, width: f32, cursor: usize,
    scroll: gpui::ScrollHandle,
}
#[derive(Clone, Copy)]
struct SliderDrag {
    path: &'static str,
    min: i64,
    max: i64,
    left: f32,
    width: f32,
    rtl: bool,
}
impl SliderDrag {
    fn value(self, x: f32) -> i64 {
        let fraction = ((x / dialog_font::scale() - self.left - 5.) / (self.width - 10.).max(1.)).clamp(0., 1.);
        let fraction = if self.rtl { 1. - fraction } else { fraction };
        self.min + ((self.max - self.min) as f32 * fraction).round() as i64
    }
}

fn control_tooltip(id: &str) -> Option<String> {
    let key = match id {
        "pref-eol-style" => "eol-custom-color-tip",
        "pref-npc-help" => "npcNote-tip",
        "pref-npc-abbreviation" => "npcAbbreviation-tip",
        "pref-npc-codepoint" => "npcCodepoint-tip",
        "pref-npc-style" => "npcCustomColor-tip",
        "pref-npc-control-eol" => "npcIncludeCcUniEol-tip",
        _ => return None,
    };
    crate::localization::current_string(key).map(str::to_owned)
}
pub(super) struct State {
    pub page: Page, pub settings: Value, fields: BTreeMap<&'static str, BoundField>,
    pub appearance: preferences_appearance_surface::State,
    pub editing: preferences_editing_surface::State,
    pub files: preferences_files_surface::State,
    pub history: bookmark_surface::HistoryState,
    navigation_focus: FocusHandle,
    choice_focus: FocusHandle,
    controls: RefCell<BTreeMap<&'static str, FocusHandle>>,
    pub(super) radio_groups: RefCell<BTreeMap<&'static str, Vec<(&'static str, Value, bool)>>>,
    choice: Option<Choice>, slider: Option<SliderDrag>, color_task: Option<gpui::Task<()>>,
    color_target: Option<(&'static str, u32)>,
    pub target_control: Option<&'static str>,
    localization_initialized: bool,
    menu_shortcuts_restart_notified: bool,
    column_task: Option<gpui::Task<()>>,
}
impl State {
    pub fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self { page: Page::General, settings: json!({}), fields: BTreeMap::new(),
            appearance: Default::default(), editing: preferences_editing_surface::State::new(), files: preferences_files_surface::State::new(cx),
            history: Default::default(),
            navigation_focus: cx.focus_handle().tab_index(-1), choice_focus: cx.focus_handle(),
            controls: RefCell::new(BTreeMap::new()), radio_groups: RefCell::new(BTreeMap::new()),
            choice: None, slider: None, color_task: None, color_target: None, target_control: None, localization_initialized: false, menu_shortcuts_restart_notified: false, column_task: None }
    }
}
pub(super) fn value_at<'a>(settings: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(settings, |value, key| {
        if value.is_array() { key.parse::<usize>().ok().and_then(|index| value.get(index)).unwrap_or(&Value::Null) }
        else { &value[key] }
    })
}
pub(super) fn write_value(settings: &mut Value, path: &str, value: Value) -> io::Result<()> {
    fn write(current: &mut Value, keys: &[&str], value: Value) -> io::Result<()> {
        let Some((key, rest)) = keys.split_first() else { *current = value; return Ok(()); };
        if current.is_null() { *current = if key.parse::<usize>().is_ok() { json!([]) } else { json!({}) }; }
        if let Some(array) = current.as_array_mut() {
            let index = key.parse::<usize>().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "配置数组索引无效"))?;
            if index >= array.len() { array.resize(index + 1, Value::Null); }
            write(&mut array[index], rest, value)
        } else if let Some(object) = current.as_object_mut() {
            write(object.entry((*key).to_owned()).or_insert(Value::Null), rest, value)
        } else { Err(io::Error::new(io::ErrorKind::InvalidData, "配置父字段不是对象或数组")) }
    }
    if path.is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "配置字段为空")); }
    write(settings, &path.split('.').collect::<Vec<_>>(), value)
}
pub(super) fn pref_label(label: &str, x: f32, y: f32, width: f32) -> AnyElement {
    let palette = current_render_palette();
    pref_label_colour(label, x, y, width, palette.text)
}
pub(super) fn pref_label_colour(label: &str, x: f32, y: f32, width: f32, colour: u32) -> AnyElement {
    let label = crate::localization::current_preference_label(label).unwrap_or(label);
    source_position(x, y, width).text_size(px(12.)).line_height(px(14.)).text_color(rgb(colour)).child(label.to_owned()).into_any_element()
}

pub(super) fn pref_label_right(label: &str, x: f32, y: f32, width: f32) -> AnyElement {
    let label = crate::localization::current_preference_label(label).unwrap_or(label);
    source_position(x, y, width).text_size(px(12.)).line_height(px(14.))
        .text_color(rgb(current_render_palette().text))
        .when(!crate::localization::current_preference_rtl(), |element| element.text_right())
        .when(crate::localization::current_preference_rtl(), |element| element.text_left())
        .child(label.to_owned()).into_any_element()
}

pub(super) fn pref_label_center(label: &str, x: f32, y: f32, width: f32) -> AnyElement {
    let label = crate::localization::current_preference_label(label).unwrap_or(label);
    source_position(x, y, width).text_size(px(12.)).line_height(px(14.)).text_center()
        .text_color(rgb(current_render_palette().text)).child(label.to_owned()).into_any_element()
}

pub(super) fn pref_value_label(value: &str, x: f32, y: f32, width: f32) -> AnyElement {
    source_position(x, y, width).text_size(px(12.)).line_height(px(14.)).text_color(rgb(current_render_palette().text)).child(value.to_owned()).into_any_element()
}
pub(super) fn pref_group(label: &str, x: f32, y: f32, width: f32, height: f32) -> AnyElement {
    let palette = current_render_palette();
    let label = crate::localization::current_preference_label(label).unwrap_or(label);
    source_rect(x, y, width, height).bg(rgb(palette.pure_background)).text_color(rgb(palette.text))
        .child(div().absolute().top(px(7.)).left(px(0.)).right(px(0.)).bottom(px(0.)).border_1().border_color(rgb(palette.group_edge())))
        .child(div().absolute().left(px(0.)).right(px(0.)).top(px(0.)).h(px(14.)).flex().justify_center()
            .child(div().h(px(14.)).px(px(3.)).line_height(px(14.)).bg(rgb(palette.pure_background)).child(label.to_owned()))).into_any_element()
}

pub(super) fn pref_indicator(checked: bool, radio: bool, enabled: bool) -> AnyElement {
    pref_indicator_palette(checked, radio, enabled, current_render_palette())
}

pub(super) fn pref_indicator_palette(checked: bool, radio: bool, enabled: bool, palette: RenderPalette) -> AnyElement {
    let color = if enabled { palette.text } else { palette.disabled_text };
    let border = if enabled { if palette.dark { palette.edge } else { 0x333333 } }
        else if palette.dark { palette.disabled_edge } else { 0xbfbfbf };
    let mut indicator = div().relative().size(px(13.)).flex_shrink_0().border_1().border_color(rgb(border))
        .bg(rgb(if palette.dark { palette.softer_background } else { 0xffffff }))
        .flex().items_center().justify_center().when(radio, |element| element.rounded_full());
    if checked && radio {
        indicator = indicator.child(div().size(px(5.)).rounded_full().bg(rgb(color)));
    } else if checked {
        indicator = indicator.child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            path.move_to(gpui::point(bounds.left() + px(1.), bounds.top() + px(5.)));
            path.line_to(gpui::point(bounds.left() + px(4.), bounds.top() + px(8.)));
            path.line_to(gpui::point(bounds.left() + px(9.), bounds.top() + px(2.)));
            match path.build() {
                Ok(path) => window.paint_path(path, rgb(color)),
                Err(error) => eprintln!("preference check mark could not be drawn: {error}"),
            }
        }).size_full());
    }
    indicator.into_any_element()
}

fn integer_prefix(text: &str, signed: bool) -> Result<i64, ()> {
    let text = text.trim_start_matches(|ch: char| ch.is_ascii_whitespace());
    let (negative, text) = if signed {
        if let Some(text) = text.strip_prefix('-') { (true, text) }
        else { (false, text.strip_prefix('+').unwrap_or(text)) }
    } else { (false, text) };
    let mut number = 0u32;
    let mut digits = 0usize;
    for byte in text.bytes().take_while(u8::is_ascii_digit) {
        number = number.checked_mul(10).and_then(|value| value.checked_add(u32::from(byte - b'0'))).ok_or(())?;
        digits += 1;
    }
    if digits == 0 { return Err(()); }
    let number = if negative { -i64::from(number) } else { i64::from(number) };
    if signed && i32::try_from(number).is_err() { return Err(()); }
    Ok(number)
}

impl MatrixWindow {
    pub(super) fn native_locale(&self) -> String {
        self.pref_string("NativeLang.filename", "english.xml")
    }

    pub(super) fn initialize_localization(&mut self, settings: &mut Value, cx: &mut Context<Self>) {
        if self.preferences.localization_initialized { return; }
        self.preferences.localization_initialized = true;
        if let Some(store) = &self.settings_store {
            match crate::localization::load_private(store) {
                Ok(Some(filename)) => {
                    if let Err(error) = write_value(settings, "NativeLang.filename", filename.into()) {
                        self.notify_editor_error(format!("Interface language cannot be loaded: {error}"), cx);
                    }
                }
                Ok(None) => {
                    let filename = value_at(settings, "NativeLang.filename").as_str().unwrap_or("english.xml");
                    if filename == "english_customizable.xml" {
                        if let Err(error) = crate::localization::select_private(store, filename) {
                            self.notify_editor_error(format!("nativeLang.xml cannot be created: {error}"), cx);
                        }
                    }
                },
                Err(error) => self.notify_editor_error(format!("nativeLang.xml cannot be loaded: {error}"), cx),
            }
        }
        self.preferences.settings = settings.clone();
    }
    pub(super) fn preferences_render_palette(&self) -> RenderPalette {
        let light = RenderPalette { pure_background: 0xffffff, ..RenderPalette::LIGHT };
        RenderPalette {
            dark: self.dark,
            background: self.appearance_colour("DarkMode.customColorMain", light.background),
            softer_background: self.appearance_colour("DarkMode.customColorActive", light.softer_background),
            hot_background: self.appearance_colour("DarkMode.customColorMenuHotTrack", light.hot_background),
            pure_background: self.appearance_colour("DarkMode.customColorTop", light.pure_background),
            error_background: self.appearance_colour("DarkMode.customColorError", light.error_background),
            text: self.appearance_colour("DarkMode.customColorText", light.text),
            darker_text: self.appearance_colour("DarkMode.customColorDarkText", light.darker_text),
            disabled_text: self.appearance_colour("DarkMode.customColorDisabledText", light.disabled_text),
            link_text: self.appearance_colour("DarkMode.customColorLinkText", light.link_text),
            edge: self.appearance_colour("DarkMode.customColorEdge", light.edge),
            hot_edge: self.appearance_colour("DarkMode.customColorHotEdge", light.hot_edge),
            disabled_edge: self.appearance_colour("DarkMode.customColorDisabledEdge", light.disabled_edge),
        }
    }

    pub(super) fn preference_control_focus(&self, id: &'static str, cx: &mut Context<Self>) -> FocusHandle {
        self.preferences.controls.borrow_mut().entry(id).or_insert_with(|| cx.focus_handle().tab_index(0)).clone()
    }

    pub(super) fn pref_field(&self, path: &str) -> Option<gpui::Entity<text_field::TextField>> {
        self.preferences.fields.get(path).and_then(BoundField::text_entity)
    }
    pub(super) fn pref_bool(&self, path: &str, default: bool) -> bool { value_at(&self.preferences.settings, path).as_bool().unwrap_or(default) }
    pub(super) fn pref_int(&self, path: &str, default: i64) -> i64 { value_at(&self.preferences.settings, path).as_i64().unwrap_or(default) }
    pub(super) fn pref_string(&self, path: &str, default: &str) -> String { value_at(&self.preferences.settings, path).as_str().unwrap_or(default).to_owned() }
    pub(super) fn show_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if self.preferences_visible {
            if self.finish_preferences(window, cx) { self.present_floating(floating_windows::DialogKind::Preferences, window, cx); cx.notify(); }
            return;
        }
        let mut settings = match self.settings_store.as_ref().map(ConfigStore::load_settings).transpose() {
            Ok(settings) => settings.unwrap_or_else(|| json!({})),
            Err(error) => { self.notify_editor_error(format!("首选项无法读取：{error}"), cx); return; }
        };
        self.initialize_localization(&mut settings, cx);
        let locale = self.native_locale();
        if let Err(error) = write_value(&mut settings, "NativeLang.filename", locale.into()) {
            self.notify_editor_error(format!("Interface language cannot be loaded: {error}"), cx);
            return;
        }
        self.preferences.settings = settings;
        self.preferences.fields.clear();
        self.normalize_editing_on_open(window, cx);
        self.preferences_visible = true;
        self.present_floating(floating_windows::DialogKind::Preferences, window, cx);
        cx.notify();
    }
    fn prepare_preference_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.preferences.fields.is_empty() { return; }
        let choice_focus = self.preferences.choice_focus.clone();
        cx.on_blur(&choice_focus, window, |view, _, cx| {
            if view.preferences.choice.take().is_some() { cx.notify(); }
        }).detach();
        let specs = preferences_appearance_surface::fields().into_iter()
            .chain(preferences_editing_surface::fields()).chain(preferences_files_surface::fields());
        for spec in specs {
            if self.preferences.fields.contains_key(spec.path) { continue; }
            let path = spec.path;
            let value = value_at(&self.preferences.settings, path);
            let value = if value.is_null() { spec.default.clone() } else { value.clone() };
            let text = if let Some(display) = preferences_editing_surface::field_display(path, &value) { display } else if spec.integer.is_some() {
                if value_at(&self.preferences.settings, path).is_null() { spec.default.as_i64().unwrap_or(0).to_string() }
                else { (value.as_i64().unwrap_or(0) / spec.scale.max(1)).to_string() }
             } else { value.as_str().unwrap_or_default().to_owned() };
             let limit = spec.max_utf16;
            if path == COLUMN_PATH {
                match NativeColumnField::new(&text) {
                    Ok(field) => { self.preferences.fields.insert(path, BoundField { spec, input: FieldInput::Columns(field), last: text, _changed: None, _blur: None }); }
                    Err(error) => self.notify_editor_error(format!("列边线输入框无法创建：{error}"), cx),
                }
                continue;
            }
            let entity = cx.new(|cx| text_field::TextField::compact(path, path, cx).without_history_arrow().with_max_utf16(limit));
            entity.update(cx, |field, cx| field.set_text(&text, window, cx));
            let focus = entity.read(cx).focus_handle();
            let changed = cx.observe_in(&entity, window, move |view, field, window, cx| {
                if field.read(cx).has_composition() { return; }
                let text = field.read(cx).text();
                if view.preferences.fields.get(path).is_some_and(|field| field.spec.commit == Commit::Immediate && field.last != text) {
                    view.commit_preference_field(path, false, window, cx);
                }
            });
            let blur = cx.on_blur(&focus, window, move |view, window, cx| { view.commit_preference_field(path, true, window, cx); });
            cx.on_focus(&focus, window, move |view, _, cx| view.editing_field_focused(path, cx)).detach();
            self.preferences.fields.insert(path, BoundField { spec, input: FieldInput::Text(entity), last: text, _changed: Some(changed), _blur: Some(blur) });
        }
    }
    pub(super) fn commit_preference_field(&mut self, path: &'static str, finishing: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.preferences.fields.get(path) else { return true; };
        let Some(text) = field.value(cx) else { return false; };
        if text == field.last { return true; }
        let spec = field.spec.clone();
        if path == COLUMN_PATH && (text.encode_utf16().count() > spec.max_utf16 || !text.bytes().all(|byte| byte == b' ' || byte.is_ascii_digit())) {
            field.set_text(&field.last, window, cx);
            return true;
        }
        let previous_value = value_at(&self.preferences.settings, path);
        let old_text = if spec.integer.is_some() {
            previous_value.as_i64().map(|value| (value / spec.scale.max(1)).to_string()).unwrap_or_else(|| spec.default.as_i64().unwrap_or(0).to_string())
        } else { field.last.clone() };
        if matches!(path, "recent_files_limit" | "snapshot_backup_ms") && text.is_empty() {
            if finishing {
                field.set_text(&old_text, window, cx);
                if let Some(field) = self.preferences.fields.get_mut(path) { field.last = old_text; }
            }
            return true;
        }
        let mut normalize = false;
        let value = if let Some((min, max)) = spec.integer {
            if text.is_empty() && let Some(value) = preferences_editing_surface::empty_field_value(path) { normalize = true; value } else {
            let parsed = if matches!(path, "recent_files_limit" | "snapshot_backup_ms") { Ok(integer_prefix(&text, false).unwrap_or(0)) }
                else if spec.unsigned_32 { integer_prefix(&text, false) }
                else if let Some(units) = spec.signed_prefix_units {
                    integer_prefix(&String::from_utf16_lossy(&text.encode_utf16().take(units).collect::<Vec<_>>()), true)
                } else { text.parse::<i64>().map_err(|_| ()) };
            let parsed = if spec.invalid_zero {
                if parsed.is_err() && spec.signed_prefix_units.is_some() { normalize = true; }
                Ok(parsed.unwrap_or(0))
            } else { parsed };
            match parsed {
                Ok(number) if preferences_editing_surface::reject_integer_edit(path, number) => {
                    if finishing { field.set_text(&old_text, window, cx); }
                    return true;
                }
                Ok(number) => {
                    normalize |= number < min || number > max || (text.is_empty() && spec.signed_prefix_units.is_some());
                    match number.clamp(min, max).checked_mul(spec.scale) { Some(value) => Value::from(value), None => return false }
                }
                Err(_) => {
                    if finishing {
                        field.set_text(&old_text, window, cx);
                    }
                    return true;
                }
            }
            }
        } else { text.clone().into() };
        if !self.set_preference(path, value.clone(), window, cx) { return false; }
        let value = value_at(&self.preferences.settings, path);
        let display = if spec.integer.is_some() && !spec.retain_text && normalize { (value.as_i64().unwrap_or(0) / spec.scale.max(1)).to_string() } else { text };
        if let Some(field) = self.preferences.fields.get_mut(path) {
            field.last = display.clone();
            if spec.integer.is_some() && !spec.retain_text && field.value(cx).as_deref() != Some(display.as_str()) {
                field.set_text(&display, window, cx);
            }
        }
        true
    }
    pub(super) fn finish_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.finish_files_preference_fields(window, cx) { return false; }
        let paths: Vec<_> = self.preferences.fields.keys().copied().collect();
        for path in paths { if !self.commit_preference_field(path, true, window, cx) { return false; } }
        if !self.finish_preference_color(window, cx) { return false; }
        self.preferences.column_task = None;
        if let Some(BoundField { input: FieldInput::Columns(field), .. }) = self.preferences.fields.get(COLUMN_PATH) { field.detach(); }
        self.preferences.choice = None; self.preferences.slider = None;
        true
    }
    pub(super) fn set_preference(&mut self, path: &str, value: Value, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let previous = self.preferences.settings.clone();
        let mut settings = previous.clone();
        let result = write_value(&mut settings, path, value.clone())
            .and_then(|_| preferences_appearance_surface::appearance_changed(path, &mut settings))
            .and_then(|_| preferences_editing_surface::editing_changed(path, &mut settings))
            .and_then(|_| preferences_files_surface::files_changed(path, &mut settings));
        if let Err(error) = result { self.notify_editor_error(format!("首选项无效：{error}"), cx); return false; }
        if let Some(store) = &self.settings_store {
            match store.update_settings(|document| {
                write_value(document, path, value)?;
                preferences_appearance_surface::appearance_changed(path, document)?;
                preferences_editing_surface::editing_changed(path, document)?;
                preferences_files_surface::files_changed(path, document)
            }) {
                Ok(document) => settings = document,
                Err(error) => { self.notify_editor_error(format!("首选项无法保存：{error}"), cx); return false; }
            }
        }
        if path == "NativeLang.filename" && value_at(&previous, path) != value_at(&settings, path) {
            if let Some(store) = &self.settings_store {
                let filename = value_at(&settings, path).as_str().unwrap_or("english.xml");
                if let Err(error) = crate::localization::select_private(store, filename) {
                    if let Err(restore) = store.update_settings(|document| write_value(document, path, value_at(&previous, path).clone())) {
                        self.notify_editor_error(format!("Interface language setting cannot be restored: {restore}"), cx);
                    }
                    self.notify_editor_error(format!("nativeLang.xml cannot be saved: {error}"), cx);
                    return false;
                }
            }
            self.preferences.choice = None;
        }
        self.preferences.settings = settings.clone();
        self.apply_preferences(&settings, window, cx);
        if path == "MISC.hideMenuRightShortcuts" && value_at(&settings, path).as_bool() == Some(false)
            && !self.preferences.menu_shortcuts_restart_notified {
            self.preferences.menu_shortcuts_restart_notified = true;
            let locale = self.native_locale();
            let title = crate::localization::dialog_title(&locale, "Need2Restart2ShowMenuShortcuts").unwrap_or("Restart required");
            let message = crate::localization::dialog_message(&locale, "Need2Restart2ShowMenuShortcuts")
                .unwrap_or("Restart Notepad Matrix to show the right menu shortcuts.");
            let response = window.prompt(gpui::PromptLevel::Info, title, Some(message), &[gpui::PromptButton::ok("OK")], cx);
            cx.spawn_in(window, async move |view, cx| {
                if let Err(error) = response.await {
                    if let Err(update) = view.update(cx, |view, cx| view.notify_editor_error(format!("Restart notice could not be completed: {error}"), cx)) {
                        eprintln!("Restart notice lost its document window: {update}");
                    }
                }
            }).detach();
        }
        let updates: Vec<_> = self.preferences.fields.iter().filter(|(key, field)| **key != path && value_at(&previous, key) != value_at(&settings, key) && !field.focused(window, cx))
            .filter_map(|(key, field)| {
                let value = value_at(&settings, key);
                let text = preferences_editing_surface::field_display(key, value).unwrap_or_else(|| {
                    if field.spec.integer.is_some() { (value.as_i64().unwrap_or_else(|| field.spec.default.as_i64().unwrap_or(0) * field.spec.scale) / field.spec.scale.max(1)).to_string() }
                    else { value.as_str().or_else(|| field.spec.default.as_str()).unwrap_or_default().to_owned() }
                });
                if text == field.last { None } else { Some((*key, text)) }
            }).collect();
        for (key, text) in updates {
            if let Some(field) = self.preferences.fields.get_mut(key) {
                field.last = text.clone(); field.set_text(&text, window, cx);
            }
        }
        cx.notify(); true
    }

    pub(super) fn apply_preferences(&mut self, settings: &Value, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences.settings = settings.clone();
        self.menu_font_family.borrow_mut().take();
        self.menu_root_reveal = None;
        self.menu_right_shortcuts_hidden |= self.pref_bool("MISC.hideMenuRightShortcuts", false);
        self.status_visible = settings["statusbar_visible"].as_bool().unwrap_or(true);
        self.mute_sounds = settings["muteSounds"].as_bool().unwrap_or(false);
        self.save_all_confirm = settings["save_all_confirm"].as_bool().unwrap_or(true);
        self.line_copy_cut_without_selection = settings["lineCopyCutWithoutSelection"].as_bool().unwrap_or(true);
        self.tab_multiline = settings["tab_multiline"].as_bool().unwrap_or(false);
        self.tab_vertical = settings["tab_vertical"].as_bool().unwrap_or(false);
        self.tab_compact_label_len = settings["tabCompactLabelLen"].as_u64().unwrap_or(0).min(257) as usize;
        self.document_switcher = settings["document_switcher"].as_bool().unwrap_or(true);
        self.mru_switching = self.document_switcher && settings["mru_switching"].as_bool().unwrap_or(true);
        self.set_tab_preview_enabled(settings["docPeekOnTab"].as_bool().unwrap_or(false), cx);
        self.apply_file_menu_preferences(settings);
        self.apply_appearance_preferences(settings, window, cx);
        let field_palette = if self.dark { Some(self.preferences_render_palette()) } else { None };
        text_field::set_app_render_palette(field_palette, cx);
        self.apply_editing_preferences(settings, window, cx);
        self.configure_change_history(settings, window, cx);
        self.apply_file_preferences(settings, window, cx);
        if let Ok(catalog) = crate::languages::catalog() {
            let languages = catalog.profiles().iter().filter(|language| !language.internal && !preferences_editing_surface::language_excluded(settings, language.id))
                .map(|language| (language.id, language.label.clone())).collect();
            self.menu_bar.set_builtin_languages_with_layout(languages, self.language_menu_compact);
        }
        self.configure_intelligence(settings);
        self.configure_find(settings);
        self.style_config.selected_text_foreground_single_color = self.pref_bool("ScintillaPrimaryView.selectedTextForegroundSingleColor", false);
        self.editor_lines.clear();
        self.editor_painted_revision = None;
        self.editor_follow_caret = true;
        cx.notify();
    }
    pub(super) fn pref_check(&self, id: &'static str, label: &'static str, path: &'static str, default: bool, invert: bool, enabled: bool, x: f32, y: f32, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let label = crate::localization::current_preference_source_label(id, label);
        let palette = self.preferences_render_palette();
        let checked = self.pref_bool(path, default) != invert;
        let focus = self.preference_control_focus(id, cx).tab_stop(enabled);
        let tooltip = control_tooltip(id).filter(|_| enabled);
        source_rect(x, y, width, 17.5).id(id)
            .role(Role::CheckBox).aria_label(label).aria_toggled(checked.into())
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .track_focus(&focus)
            .when_some(tooltip, |element, text| element.tooltip(move |_, cx| icon_tooltip::view(text.clone(), palette.dark, cx)))
            .flex().items_center().gap(px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .text_color(rgb(palette.control_text(enabled))).child(pref_indicator(checked, false, enabled)).child(label)
            .on_click(cx.listener(move |view, _, window, cx| { if enabled { focus.focus(window, cx); view.set_preference(path, (!view.pref_bool(path, default)).into(), window, cx); } }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if enabled && event.keystroke.key == "space" { view.set_preference(path, (!view.pref_bool(path, default)).into(), window, cx); cx.stop_propagation(); }
            })).into_any_element()
    }
    pub(super) fn pref_radio(&self, id: &'static str, label: &'static str, path: &'static str, default: Value, value: Value, enabled: bool, x: f32, y: f32, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let label = crate::localization::current_preference_source_label(id, label);
        let palette = self.preferences_render_palette();
        let current = value_at(&self.preferences.settings, path);
        let selected = (if current.is_null() { &default } else { current }) == &value;
        let keyboard = value.clone();
        let focus = self.preference_control_focus(id, cx).tab_stop(enabled && selected);
        let tooltip = control_tooltip(id).filter(|_| enabled);
        self.preferences.radio_groups.borrow_mut().entry(path).or_default().push((id, value.clone(), enabled));
        source_rect(x, y, width, 17.5).id(id)
            .role(Role::RadioButton).aria_label(label).aria_toggled(selected.into())
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .track_focus(&focus)
            .when_some(tooltip, |element, text| element.tooltip(move |_, cx| icon_tooltip::view(text.clone(), palette.dark, cx)))
            .flex().items_center().gap(px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .text_color(rgb(palette.control_text(enabled))).child(pref_indicator(selected, true, enabled)).child(label)
            .on_click(cx.listener(move |view, _, window, cx| { if enabled { focus.focus(window, cx); view.set_preference(path, value.clone(), window, cx); } }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if enabled && event.keystroke.key == "space" { view.set_preference(path, keyboard.clone(), window, cx); cx.stop_propagation(); }
                if enabled && matches!(event.keystroke.key.as_str(), "left" | "up" | "right" | "down") {
                    let group: Vec<_> = view.preferences.radio_groups.borrow().get(path).into_iter().flatten().filter(|(_, _, enabled)| *enabled).cloned().collect();
                    if let Some(index) = group.iter().position(|(candidate, _, _)| *candidate == id) {
                        let index = if matches!(event.keystroke.key.as_str(), "left" | "up") { (index + group.len() - 1) % group.len() } else { (index + 1) % group.len() };
                        let (target, value, _) = &group[index];
                        view.preference_control_focus(target, cx).focus(window, cx);
                        view.set_preference(path, value.clone(), window, cx); cx.stop_propagation();
                    }
                }
            })).into_any_element()
    }
    pub(super) fn pref_button(&self, id: &'static str, label: &'static str, enabled: bool, x: f32, y: f32, width: f32, callback: fn(&mut Self, &mut Window, &mut Context<Self>), cx: &mut Context<Self>) -> AnyElement {
        let label = crate::localization::current_preference_source_label(id, label);
        let palette = self.preferences_render_palette();
        let focus = self.preference_control_focus(id, cx).tab_stop(enabled);
        let tooltip = control_tooltip(id).filter(|_| enabled);
        source_rect(x, y, width, 24.).id(id)
            .role(Role::Button).aria_label(label).border_1().border_color(rgb(palette.control_edge(enabled))).bg(rgb(palette.button_background(enabled)))
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .flex().items_center().justify_center().line_height(px(14.)).text_center().text_color(rgb(if enabled { palette.darker_text } else { palette.disabled_text }))
            .when(enabled, |element| element
                .hover(move |style| style.bg(rgb(palette.hot_background)).border_color(rgb(palette.hot_edge)).text_color(rgb(palette.text)))
                .focus(move |style| style.border_color(rgb(palette.hot_edge))))
            .track_focus(&focus).child(label)
            .when_some(tooltip, |element, text| element.tooltip(move |_, cx| icon_tooltip::view(text.clone(), palette.dark, cx)))
            .on_click(cx.listener(move |view, _, window, cx| { if enabled { focus.focus(window, cx); callback(view, window, cx); } }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if enabled && matches!(event.keystroke.key.as_str(), "space" | "enter") { callback(view, window, cx); cx.stop_propagation(); }
            })).into_any_element()
    }
    pub(super) fn pref_input(&self, path: &'static str, x: f32, y: f32, width: f32, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(field) = self.preferences.fields.get(path) else { return div().into_any_element(); };
        let Some(entity) = field.text_entity() else { return div().into_any_element(); };
        if entity.read(cx).enabled() != enabled { entity.update(cx, |field, cx| field.set_enabled(enabled, cx)); }
        source_rect(x, y, width, 23.).id(format!("pref-input-{path}"))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" { view.commit_preference_field(path, true, window, cx); cx.stop_propagation(); }
             })).child(entity).into_any_element()
     }
    pub(super) fn pref_column_input(&self, x: f32, y: f32, width: f32, height: f32) -> AnyElement {
        let Some(BoundField { input: FieldInput::Columns(field), .. }) = self.preferences.fields.get(COLUMN_PATH) else { return div().into_any_element(); };
        source_rect(x, y, width, height).id("pref-column-input").child(field.element(current_render_palette())).into_any_element()
    }
    pub(super) fn pref_choice(&self, path: &'static str, options: Vec<(Value, String)>, default: Value, enabled: bool, x: f32, y: f32, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let rtl = crate::localization::current_preference_rtl();
        let options = if path == "FileAutoDetection" {
            options.into_iter().enumerate().map(|(index, (value, label))| {
                (value, crate::localization::current_preference_combo(6347, index).map(str::to_owned).unwrap_or(label))
            }).collect()
        } else { options };
        let x = source_x(x, width);
        let palette = self.preferences_render_palette();
        let override_value = match path {
            "Matrix.style" => Some(self.matrix_style().into()),
            "MenuFont.fontName" => Some(menu_surface::font_family(self, || cx.text_system().all_font_names()).into()),
            "MenuFont.size" => Some(menu_surface::font_size(self).into()),
            "DialogFont.fontName" => Some(options.iter().find(|(value, _)| {
                value.as_str().zip(value_at(&self.preferences.settings, path).as_str())
                    .is_some_and(|(available, selected)| available.eq_ignore_ascii_case(selected))
            }).map(|(value, _)| value.clone()).unwrap_or_else(|| default.clone())),
            "DialogFont.size" => Some(options.iter().find(|(value, _)| value == value_at(&self.preferences.settings, path))
                .map(|(value, _)| value.clone()).unwrap_or_else(|| default.clone())),
            _ => preferences_editing_surface::choice_value(path, &self.preferences.settings),
        };
        let current = override_value.as_ref().unwrap_or_else(|| value_at(&self.preferences.settings, path));
        let selected = if current.is_null() || (path.starts_with("DialogFont.") && !options.iter().any(|(value, _)| value == current)) { &default } else { current };
        let label = options.iter().find(|(value, _)| value == selected).map(|(_, label)| label.clone()).unwrap_or_default();
        let cursor = options.iter().position(|(value, _)| value == selected).unwrap_or(0);
        let focus = self.preference_control_focus(path, cx).tab_stop(enabled);
        let keyboard_options = options.clone();
        div().id(format!("pref-choice-{path}")).absolute().left(px(x)).top(px(y)).w(px(width)).h(px(22.))
            .role(Role::ComboBox).aria_label(path).border_1().border_color(rgb(palette.control_edge(enabled))).bg(rgb(if palette.dark { palette.control_background(enabled) } else if enabled { 0xe1e1e1 } else { 0xe9e9e9 }))
            .aria_value(label.clone())
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .text_color(rgb(palette.control_text(enabled)))
            .when(enabled, |element| element
                .hover(move |style| style.bg(rgb(palette.hot_background)).border_color(rgb(palette.hot_edge)))
                .focus(move |style| style.border_color(rgb(palette.hot_edge))))
            .track_focus(&focus).flex().items_center().justify_between().line_height(px(14.)).when(rtl, |element| element.flex_row_reverse().text_right())
            .child(div().flex_1().min_w(px(0.)).px(px(3.)).child(label))
            .child(div().w(px(17.)).h_full().flex().items_center().justify_center().text_color(rgb(palette.control_text(enabled))).child("⌄"))
            .on_click(cx.listener(move |view, _, window, cx| {
                if enabled {
                    let scroll = gpui::ScrollHandle::new();
                    scroll.set_offset(gpui::point(px(0.), px(-(cursor as f32 * 22.))));
                    view.preferences.choice = Some(Choice { path, options: options.clone(), x, y, width, cursor, scroll });
                    view.preferences.choice_focus.focus(window, cx); cx.notify();
                }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if enabled && matches!(event.keystroke.key.as_str(), "space" | "enter" | "down") {
                    let scroll = gpui::ScrollHandle::new();
                    scroll.set_offset(gpui::point(px(0.), px(-(cursor as f32 * 22.))));
                    view.preferences.choice = Some(Choice { path, options: keyboard_options.clone(), x, y, width, cursor, scroll });
                    view.preferences.choice_focus.focus(window, cx); cx.notify(); cx.stop_propagation();
                }
            })).into_any_element()
    }
    pub(super) fn pref_slider(&self, path: &'static str, min: i64, max: i64, default: i64, x: f32, y: f32, width: f32, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let rtl = crate::localization::current_preference_rtl();
        let x = source_x(x, width);
        let content_x = if rtl { 1015. - 201.25 - 805. } else { 201.25 };
        let palette = self.preferences_render_palette();
        let channel = if palette.dark { palette.control_background(enabled) } else { 0xe7e7e7 };
        let thumb = if palette.dark { if enabled { palette.softer_background } else { palette.disabled_edge } }
            else if enabled { 0x0078d7 } else { 0xcccccc };
        let current = self.pref_int(path, default).clamp(min, max);
        let fraction = if max > min { (current - min) as f32 / (max - min) as f32 } else { 0. };
        let drag = SliderDrag { path, min, max, left: content_x + x, width, rtl };
        let focus = self.preference_control_focus(path, cx).tab_stop(enabled);
        let mut element = div().id(format!("pref-slider-{path}")).absolute().left(px(x)).top(px(y)).w(px(width)).h(px(22.))
            .role(Role::Slider).aria_label(path).track_focus(&focus)
            .a11y_synthetic_children(move |builder| {
                let node = builder.parent_node();
                node.set_min_numeric_value(min as f64);
                node.set_max_numeric_value(max as f64);
                node.set_numeric_value(current as f64);
                node.set_numeric_value_step(1.);
                if !enabled { node.set_disabled(); }
            })
            .child(div().absolute().top(px(10.)).w_full().h(px(4.)).border_1().border_color(rgb(if palette.dark { palette.edge } else { 0xd5d5d5 })).bg(rgb(channel))
                .when(palette.dark && !enabled, |element| element.border_1().border_color(rgb(palette.disabled_edge))))
            .child(div().absolute().top(px(1.)).left(px((if rtl { 1. - fraction } else { fraction }) * (width - 10.))).w(px(10.)).h(px(21.)).bg(rgb(thumb)))
            .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                if !enabled { return; } view.preferences.slider = Some(drag);
                focus.focus(window, cx);
                view.set_preference(path, drag.value(f32::from(event.position.x)).into(), window, cx);
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if !enabled { return; }
                if matches!(event.keystroke.key.as_str(), "home" | "end") {
                    let value = if event.keystroke.key == "home" { min } else { max };
                    view.set_preference(path, value.into(), window, cx); cx.stop_propagation(); return;
                }
                let page = if path == "Caret.blinkSlider" { 50 } else { 1 };
                let delta = match event.keystroke.key.as_str() { "left" | "up" => -1, "right" | "down" => 1, "pageup" => -page, "pagedown" => page, _ => return };
                let delta = if rtl && matches!(event.keystroke.key.as_str(), "left" | "right") { -delta } else { delta };
                view.set_preference(path, (view.pref_int(path, default) + delta).clamp(min, max).into(), window, cx); cx.stop_propagation();
            }));
        for action in [gpui::accesskit::Action::Increment, gpui::accesskit::Action::Decrement, gpui::accesskit::Action::SetValue] {
            let owner = cx.entity().downgrade();
            element = element.on_a11y_action(action, move |data, window, cx| {
                if !enabled { return; }
                let requested = match action {
                    gpui::accesskit::Action::SetValue => match data {
                        Some(gpui::accesskit::ActionData::NumericValue(value)) => Some(*value),
                        Some(gpui::accesskit::ActionData::Value(value)) => value.parse::<f64>().ok(),
                        _ => return,
                    },
                    _ => None,
                };
                if action == gpui::accesskit::Action::SetValue && requested.is_none_or(|value| !value.is_finite()) { return; }
                if let Err(error) = owner.update(cx, |view, cx| {
                    let value = requested.map(|value| value.round() as i64).unwrap_or_else(|| {
                        view.pref_int(path, default) + if action == gpui::accesskit::Action::Increment { 1 } else { -1 }
                    });
                    view.set_preference(path, value.clamp(min, max).into(), window, cx);
                }) { eprintln!("preference slider action lost its window: {error}"); }
            });
        }
        element.into_any_element()
    }
    pub(super) fn pref_color(&self, path: &'static str, default: u32, enabled: bool, x: f32, y: f32, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let value = colorref_to_rgb(self.pref_int(path, i64::from(default)) as u32);
        source_rect(x, y, 25., 25.).id(format!("pref-color-{path}"))
            .role(Role::Button).aria_label(path).border_1().border_color(rgb(if palette.dark { palette.control_edge(enabled) } else { 0x000000 })).bg(rgb(if enabled { value } else { palette.control_background(false) }))
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .on_click(cx.listener(move |view, _, window, cx| { if enabled { view.choose_preference_color(path, default, window, cx); } })).into_any_element()
    }
    pub(super) fn sync_preference_color(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((path, default)) = self.preferences.color_target else { return true; };
        let Some(value) = style_config_surface::color_panel_rgb().map(rgb_to_colorref) else { return true; };
        self.pref_int(path, i64::from(default)) == i64::from(value) || self.set_preference(path, value.into(), window, cx)
    }

    pub(super) fn finish_preference_color(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.sync_preference_color(window, cx) { return false; }
        let owned = self.preferences.color_target.take().is_some();
        self.preferences.color_task = None;
        if owned {
            unsafe {
                let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                let _: () = msg_send![panel, orderOut: std::ptr::null::<AnyObject>()];
            }
        }
        true
    }

    fn choose_preference_color(&mut self, path: &'static str, default: u32, window: &mut Window, cx: &mut Context<Self>) {
        if !self.finish_preference_color(window, cx) { return; }
        self.finish_style_color(window, cx);
        let value = colorref_to_rgb(self.pref_int(path, i64::from(default)) as u32);
        unsafe {
            let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
            let color: *mut AnyObject = msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64];
            let _: () = msg_send![panel, setShowsAlpha: false]; let _: () = msg_send![panel, setColor: color];
            let _: () = msg_send![panel, orderFront: std::ptr::null::<AnyObject>()];
        }
        self.preferences.color_target = Some((path, default));
        let executor = cx.background_executor().clone();
        self.preferences.color_task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(50)).await;
                let keep = view.update_in(cx, |view, window, cx| {
                    if view.preferences.color_target != Some((path, default)) { return false; }
                    let visible: Bool = unsafe {
                        let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                        msg_send![panel, isVisible]
                    };
                    if !view.sync_preference_color(window, cx) { return false; }
                    let keep = view.preferences_visible && visible.as_bool();
                    if !keep { view.preferences.color_target = None; }
                    keep
                });
                match keep { Ok(true) => {}, Ok(false) => break, Err(error) => { eprintln!("颜色选择器无法更新：{error}"); break; } }
            }
        }));
    }
    pub(super) fn render_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.preferences_visible { return None; }
        let palette = self.preferences_render_palette();
        let _palette_scope = RenderPaletteScope::new(palette);
        self.prepare_preference_fields(window, cx);
        for field in self.preferences.fields.values() {
            if let Some(entity) = field.text_entity() { sync_text_field_palette(&entity, palette, cx); }
        }
        sync_text_field_palette(&self.preferences.files.custom_extension, palette, cx);
        if let Some(target) = self.style_config.preferences_target.take() {
            if !self.finish_preferences(window, cx) { self.style_config.preferences_target = Some(target); return None; }
            self.preferences.page = match target.page {
                "Editing" => Page::Editing1, "Editing 2" => Page::Editing2, "Margins/Border/Edge" => Page::Margins,
                "Highlighting" => Page::Highlight, "Cloud & Link" => Page::CloudLinks, "Tab Bar" => Page::Tabs, _ => self.preferences.page,
            };
            self.preferences.target_control = Some(target.control);
        }
        let page = self.preferences.page;
        if page == Page::Margins && self.preferences.column_task.is_none() {
            let executor = cx.background_executor().clone();
            self.preferences.column_task = Some(cx.spawn_in(window, async move |view, cx| {
                loop {
                    executor.timer(std::time::Duration::from_millis(50)).await;
                    match view.update_in(cx, |view, window, cx| {
                        if !view.preferences_visible || view.preferences.page != Page::Margins {
                            if let Some(BoundField { input: FieldInput::Columns(field), .. }) = view.preferences.fields.get(COLUMN_PATH) { field.detach(); }
                            return false;
                        }
                        view.commit_preference_field(COLUMN_PATH, false, window, cx);
                        true
                    }) {
                        Ok(true) => {},
                        Ok(false) => break,
                        Err(error) => { eprintln!("列边线输入框无法更新：{error}"); break; }
                    }
                }
            }));
        }
        let filename = self.native_locale();
        let rtl = crate::localization::is_rtl(&filename);
        let shell_x = |x: f32, width: f32| if rtl { 1015. - x - width } else { x };
        let title = crate::localization::preference_title(&filename, "").unwrap_or("Preferences");
        window.set_window_title(title);
        self.preferences.radio_groups.borrow_mut().clear();
        let section = crate::localization::preference_section(page as usize).unwrap_or("");
        let content = crate::localization::with_preference(&filename, section, || match page {
            Page::General | Page::Toolbar | Page::Tabs | Page::Editing1 | Page::Editing2 | Page::DarkMode | Page::Margins | Page::Highlight => self.render_preferences_appearance(page, window, cx),
            Page::NewDocument | Page::Language | Page::Indentation | Page::Printing | Page::Searching | Page::Completion | Page::Delimiters | Page::SearchEngine => self.render_preferences_editing(page, window, cx),
            _ => self.render_preferences_files(page, window, cx),
        });
        if let Some(control) = self.preferences.target_control.take() {
            let target = match control {
                "IDC_RADIO_CLM_HILITE" => Some("pref-current-line-background"),
                "IDC_CHECK_MULTISELECTION" => Some("pref-multiple-selection"),
                "IDC_CARETSETTING_STATIC" => Some("Caret.width"),
                "IDC_COLUMNPOS_EDIT" => Some("ScintillaPrimaryView.edgeMultiColumnPos"),
                "IDC_CHECK_LINENUMBERMARGE" => Some("pref-line-numbers"),
                "IDC_CHECK_BOOKMARKMARGE" => Some("pref-bookmark-margin"),
                "IDC_GB_CHANGHISTORY" => Some("pref-history-margin"),
                "IDC_FMS_GB_STATIC" => Some("pref-fold-box"),
                "IDC_CHECK_ENABLSMARTHILITE" => Some("pref-smart-enabled"),
                "IDC_CHECK_ENABLTAGSMATCHHILITE" => Some("pref-tags-enabled"),
                "IDC_CHECK_ENABLTAGATTRHILITE" => Some("pref-tags-attributes"),
                "IDC_MARKALL_STATIC" => Some("pref-mark-case"),
                "IDC_CHECK_CLICKABLELINK_ENABLE" => Some("pref-link-enabled"),
                "IDC_CHECK_WITHCUSTOMCOLOR_CRLF" => Some("pref-eol-color"),
                "IDC_CHECK_ORANGE" => Some("pref-tabs-top-bar"),
                "IDC_CHECK_DRAWINACTIVE" => Some("pref-tabs-inactive-color"),
                "IDC_CHECK_NPC_COLOR" => Some("pref-npc-color"),
                _ => None,
            };
            if let Some(target) = target {
                if target == COLUMN_PATH {
                    if let Some(BoundField { input: FieldInput::Columns(field), .. }) = self.preferences.fields.get(COLUMN_PATH) { field.request_focus(); }
                }
                else if let Some(field) = self.pref_field(target) { field.update(cx, |field, cx| field.focus(window, cx)); }
                else if let Some(focus) = self.preferences.controls.borrow().get(target).cloned() { focus.focus(window, cx); }
            }
        }
        let popup = self.preferences.choice.as_ref().map(|choice| {
            let path = choice.path;
            div().id("preferences-choice-popup").absolute().left(px(shell_x(201.25, 805.) + choice.x)).top(px((17.5 + choice.y + 23.).min(250.)))
                .role(Role::ListBox).aria_label(path).track_scroll(&choice.scroll)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .track_focus(&self.preferences.choice_focus)
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    let Some(choice) = view.preferences.choice.as_mut() else { return; };
                    match event.keystroke.key.as_str() {
                        "up" => choice.cursor = choice.cursor.saturating_sub(1),
                        "down" => choice.cursor = (choice.cursor + 1).min(choice.options.len().saturating_sub(1)),
                        "home" => choice.cursor = 0,
                        "end" => choice.cursor = choice.options.len().saturating_sub(1),
                        "enter" | "space" => {
                            let value = choice.options.get(choice.cursor).map(|(value, _)| (choice.path, value.clone()));
                            view.preferences.choice = None;
                            if let Some((path, value)) = value { view.set_preference(path, value, window, cx); view.preference_control_focus(path, cx).focus(window, cx); }
                        }
                        "escape" => { let path = choice.path; view.preferences.choice = None; view.preference_control_focus(path, cx).focus(window, cx); }
                        _ => return,
                    }
                    if let Some(choice) = &view.preferences.choice { choice.scroll.scroll_to_item(choice.cursor); }
                    cx.notify(); cx.stop_propagation();
                }))
                .w(px(choice.width)).max_h(px(150.)).overflow_y_scroll().bg(rgb(palette.background)).text_color(rgb(palette.text)).border_1().border_color(rgb(palette.edge)).occlude()
                .children(choice.options.iter().enumerate().map(|(index, (value, label))| {
                    let selected = index == choice.cursor; let value = value.clone();
                    div().id(("preference-option", index)).px(px(4.)).h(px(22.)).bg(rgb(if selected { 0x0078d7 } else { palette.background }))
                        .role(Role::ListBoxOption).aria_label(label.clone()).aria_selected(selected)
                        .text_color(rgb(if selected { 0xffffff } else { palette.text })).child(label.clone())
                        .on_click(cx.listener(move |view, _, window, cx| { view.preferences.choice = None; view.set_preference(path, value.clone(), window, cx); }))
                })).into_any_element()
        });
        Some(div().id("preferences-dialog").role(Role::Dialog).aria_label(title)
            .track_focus(&self.preferences_focus).key_context("MatrixPreferences").relative().size_full()
            .bg(rgb(palette.pure_background)).text_color(rgb(palette.text)).font_family(dialog_font::font_family()).text_size(px(12.)).line_height(px(14.)).occlude()
            .when(rtl, |element| element.text_right())
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, window, cx| {
                if event.pressed_button != Some(MouseButton::Left) { view.preferences.slider = None; return; }
                if let Some(drag) = view.preferences.slider {
                    view.set_preference(drag.path, drag.value(f32::from(event.position.x)).into(), window, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| view.preferences.slider = None))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, _| view.preferences.slider = None))
            .on_mouse_down(MouseButton::Left, cx.listener(|view, _, _, cx| {
                if view.preferences.choice.take().is_some() { cx.notify(); }
                cx.stop_propagation();
            }))
            .child(div().id("preferences-navigation").absolute().left(px(shell_x(17.5, 175.))).top(px(17.5)).w(px(175.)).h(px(376.25))
                .role(Role::ListBox).aria_label(title).track_focus(&self.preferences.navigation_focus)
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    let index = Page::ALL.iter().position(|page| *page == view.preferences.page).unwrap_or(0);
                    let index = match event.keystroke.key.as_str() { "up" => index.saturating_sub(1), "down" => (index + 1).min(Page::ALL.len() - 1), "home" => 0, "end" => Page::ALL.len() - 1, _ => return };
                    if view.finish_preferences(window, cx) { view.preferences.page = Page::ALL[index]; view.preferences.target_control = None; cx.notify(); }
                    cx.stop_propagation();
                }))
                .bg(rgb(palette.softer_background)).text_color(rgb(palette.text)).border_1().border_color(rgb(palette.edge))
                .focus(move |style| style.border_color(rgb(palette.hot_edge))).overflow_hidden()
                 .children(Page::ALL.into_iter().map(|item| {
                    let section = crate::localization::preference_section(item as usize).unwrap_or("");
                    let label = crate::localization::preference_title(&filename, section).unwrap_or(item.label());
                    div().id(format!("preferences-page-{}", item.label())).h(px(14.)).line_height(px(14.)).px(px(1.))
                        .role(Role::ListBoxOption).aria_label(label).aria_selected(page == item)
                        .bg(rgb(if page == item { 0x0078d7 } else { palette.softer_background })).text_color(rgb(if page == item { 0xffffff } else { palette.text })).child(label)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.finish_preferences(window, cx) { view.preferences.page = item; view.preferences.target_control = None; view.preferences.navigation_focus.focus(window, cx); cx.notify(); }
                        }))
                })))
            .child(div().absolute().left(px(shell_x(201.25, 805.))).top(px(17.5)).w(px(805.)).h(px(358.75)).child(content))
            .child(self.pref_button("preferences-close", crate::localization::preference_control(&filename, "", 6001).unwrap_or("Close"), true, shell_x(560., 87.5), 376.25, 87.5,
                |view, window, cx| view.dismiss_floating(floating_windows::DialogKind::Preferences, window, cx), cx))
            .children(popup).into_any_element())
    }
}
