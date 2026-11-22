use super::*;
use super::dialog_font::px;
use crate::languages::{LanguageProfile, StyleProfile};
use crate::style_config::{self, StyleConfig};
use super::text_field::RenderPalette;
use std::collections::BTreeMap;
use std::path::Path;
use objc2::{class, msg_send};
use objc2::runtime::{AnyObject, Bool};
use objc2::rc::{Allocated, Retained};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

pub(super) const TITLE: &str = "语言样式配置器";

const NOMINAL_WIDTH: f32 = 875.;

pub(super) fn dimensions() -> gpui::Size<gpui::Pixels> { size(preferences_surface::dimensions().width, px(430.5)) }

fn horizontal_px(value: f32) -> gpui::Pixels { preferences_surface::dimensions().width * (value / NOMINAL_WIDTH) }

#[derive(Clone, Copy)]
pub(super) struct PreferencesTarget {
    pub page: &'static str,
    pub control: &'static str,
}

fn preferences_target(name: &str) -> Option<PreferencesTarget> {
    let (page, control) = match name {
        "Current line background colour" => ("Editing", "IDC_RADIO_CLM_HILITE"),
        "Multi-edit carets color" | "Multi-selected text color" => ("Editing 2", "IDC_CHECK_MULTISELECTION"),
        "Caret colour" => ("Editing", "IDC_CARETSETTING_STATIC"),
        "Edge colour" => ("Margins/Border/Edge", "IDC_COLUMNPOS_EDIT"),
        "Line number margin" => ("Margins/Border/Edge", "IDC_CHECK_LINENUMBERMARGE"),
        "Bookmark margin" => ("Margins/Border/Edge", "IDC_CHECK_BOOKMARKMARGE"),
        "Change History margin" | "Change History modified" | "Change History revert modified" | "Change History revert origin" | "Change History saved" => ("Margins/Border/Edge", "IDC_GB_CHANGHISTORY"),
        "Fold" | "Fold active" | "Fold margin" => ("Margins/Border/Edge", "IDC_FMS_GB_STATIC"),
        "Smart Highlighting" => ("Highlighting", "IDC_CHECK_ENABLSMARTHILITE"),
        "Tags match highlighting" => ("Highlighting", "IDC_CHECK_ENABLTAGSMATCHHILITE"),
        "Tags attribute" => ("Highlighting", "IDC_CHECK_ENABLTAGATTRHILITE"),
        "Mark Style 1" | "Mark Style 2" | "Mark Style 3" | "Mark Style 4" | "Mark Style 5" => ("Highlighting", "IDC_MARKALL_STATIC"),
        "URL hovered" => ("Cloud & Link", "IDC_CHECK_CLICKABLELINK_ENABLE"),
        "EOL custom color" => ("Editing 2", "IDC_CHECK_WITHCUSTOMCOLOR_CRLF"),
        "Active tab focused indicator" | "Active tab unfocused indicator" => ("Tab Bar", "IDC_CHECK_ORANGE"),
        "Inactive tabs" => ("Tab Bar", "IDC_CHECK_DRAWINACTIVE"),
        "Non-printing characters custom color" => ("Editing 2", "IDC_CHECK_NPC_COLOR"),
        _ => return None,
    };
    Some(PreferencesTarget { page, control })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice { Theme, Language, Font, FontSize }

struct BoundField {
    entity: gpui::Entity<text_field::TextField>,
    _changed: gpui::Subscription,
}

struct NativeKeywordField {
    scroll: Retained<AnyObject>,
    text: Retained<AnyObject>,
    read_only: bool,
}

fn native_rgb(value: u32) -> Retained<AnyObject> {
    unsafe {
        msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64]
    }
}

pub(super) fn color_panel_rgb() -> Option<u32> {
    unsafe {
        let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
        let color: *mut AnyObject = msg_send![panel, color];
        let space: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
        let color: *mut AnyObject = msg_send![color, colorUsingColorSpace: space];
        if color.is_null() { return None; }
        let red: f64 = msg_send![color, redComponent];
        let green: f64 = msg_send![color, greenComponent];
        let blue: f64 = msg_send![color, blueComponent];
        Some(((red.clamp(0., 1.) * 255.).round() as u32) << 16 | ((green.clamp(0., 1.) * 255.).round() as u32) << 8 | (blue.clamp(0., 1.) * 255.).round() as u32)
    }
}

impl MatrixWindow {
    pub(super) fn matrix_style_prompt_pending(&self) -> bool {
        self.style_config.prompting
    }
}

impl NativeKeywordField {
    fn new(value: &str, read_only: bool, palette: RenderPalette) -> io::Result<Self> {
        unsafe {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(f64::from(f32::from(horizontal_px(238.))), dialog_font::native_length(91.)));
            let scroll: Allocated<AnyObject> = msg_send![class!(NSScrollView), alloc];
            let scroll: Option<Retained<AnyObject>> = msg_send![scroll, initWithFrame: frame];
            let scroll = scroll.ok_or_else(|| io::Error::other("无法创建关键字滚动区域"))?;
            let text: Allocated<AnyObject> = msg_send![class!(NSTextView), alloc];
            let text: Option<Retained<AnyObject>> = msg_send![text, initWithFrame: frame];
            let text = text.ok_or_else(|| io::Error::other("无法创建关键字文本区域"))?;
            let _: () = msg_send![&*text, setRichText: false];
            let _: () = msg_send![&*text, setEditable: !read_only];
            let _: () = msg_send![&*text, setSelectable: true];
            let _: () = msg_send![&*text, setAllowsUndo: true];
            let _: () = msg_send![&*text, setAutomaticQuoteSubstitutionEnabled: false];
            let _: () = msg_send![&*text, setAutomaticDashSubstitutionEnabled: false];
            let _: () = msg_send![&*text, setAutomaticSpellingCorrectionEnabled: false];
            let _: () = msg_send![&*text, setHorizontallyResizable: false];
            let _: () = msg_send![&*text, setVerticallyResizable: true];
            let _: () = msg_send![&*text, setMinSize: NSSize::new(0., dialog_font::native_length(91.))];
            let _: () = msg_send![&*text, setMaxSize: NSSize::new(f64::from(f32::from(horizontal_px(238.))), f64::MAX)];
            let _: () = msg_send![&*text, setTextContainerInset: NSSize::new(f64::from(f32::from(horizontal_px(3.))), dialog_font::native_length(2.))];
            let container: *mut AnyObject = msg_send![&*text, textContainer];
            let _: () = msg_send![container, setWidthTracksTextView: true];
            let _: () = msg_send![container, setContainerSize: NSSize::new(f64::from(f32::from(horizontal_px(238.))), f64::MAX)];
            let font = dialog_font::native_font();
            let _: () = msg_send![&*text, setFont: &*font];
            let _: () = msg_send![&*text, setString: &*NSString::from_str(value)];
            let _: () = msg_send![&*scroll, setBorderType: 0usize];
            let _: () = msg_send![&*scroll, setHasVerticalScroller: true];
            let _: () = msg_send![&*scroll, setHasHorizontalScroller: false];
            let _: () = msg_send![&*scroll, setScrollerStyle: 1isize];
            let _: () = msg_send![&*scroll, setAutohidesScrollers: false];
            let _: () = msg_send![&*scroll, setDocumentView: &*text];
            let field = Self { scroll, text, read_only };
            field.sync_palette(palette);
            Ok(field)
        }
    }

    fn sync_palette(&self, palette: RenderPalette) {
        let foreground = native_rgb(palette.text);
        let background = native_rgb(if self.read_only { if palette.dark { palette.background } else { 0xf0f0f0 } } else { palette.softer_background });
        let selection_background = native_rgb(if palette.dark { palette.hot_background } else { 0x0078d7 });
        let selection_text = native_rgb(if palette.dark { palette.text } else { 0xffffff });
        unsafe {
            let _: () = msg_send![&*self.text, setTextColor: &*foreground];
            let _: () = msg_send![&*self.text, setAlignment: if crate::localization::current_preference_rtl() { 1usize } else { 0usize }];
            let _: () = msg_send![&*self.text, setInsertionPointColor: &*foreground];
            let _: () = msg_send![&*self.text, setBackgroundColor: &*background];
            let _: () = msg_send![&*self.text, setDrawsBackground: true];
            let _: () = msg_send![&*self.scroll, setBackgroundColor: &*background];
            let _: () = msg_send![&*self.scroll, setDrawsBackground: true];
            let clip: *mut AnyObject = msg_send![&*self.scroll, contentView];
            let _: () = msg_send![clip, setBackgroundColor: &*background];
            let _: () = msg_send![clip, setDrawsBackground: true];
            let attributes: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
            let _: () = msg_send![&*attributes, setObject: &*selection_background, forKey: &*NSString::from_str("NSBackgroundColor")];
            let _: () = msg_send![&*attributes, setObject: &*selection_text, forKey: &*NSString::from_str("NSColor")];
            let _: () = msg_send![&*self.text, setSelectedTextAttributes: &*attributes];
            let background_rgb = if self.read_only { if palette.dark { palette.background } else { 0xf0f0f0 } } else { palette.softer_background };
            let brightness = ((background_rgb >> 16) & 255) * 299 + ((background_rgb >> 8) & 255) * 587 + (background_rgb & 255) * 114;
            let knob = if brightness < 128000 { 2isize } else { 1isize };
            let _: () = msg_send![&*self.scroll, setScrollerKnobStyle: knob];
        }
    }

    fn value(&self) -> Option<String> {
        unsafe {
            let marked: Bool = msg_send![&*self.text, hasMarkedText];
            if marked.as_bool() { return None; }
            let string: *const NSString = msg_send![&*self.text, string];
            string.as_ref().map(ToString::to_string)
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

    fn element(&self, palette: RenderPalette, hidden: bool) -> AnyElement {
        self.sync_palette(palette);
        let scroll = self.scroll.clone();
        let canvas = gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            if let Ok(handle) = HasWindowHandle::window_handle(window) && let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                let top = f64::from(f32::from(bounds.top()));
                let height = f64::from(f32::from(bounds.size.height));
                let y = if view.isFlipped() { top } else { view.bounds().size.height - top - height };
                let frame = NSRect::new(NSPoint::new(f64::from(f32::from(bounds.left())), y), NSSize::new(f64::from(f32::from(bounds.size.width)), height));
                unsafe {
                    let _: () = msg_send![&*scroll, setFrame: frame];
                    let _: () = msg_send![&*scroll, setHidden: hidden];
                    let parent: *mut AnyObject = msg_send![&*scroll, superview];
                    if parent != (view as *const NSView).cast_mut().cast::<AnyObject>() { let _: () = msg_send![view, addSubview: &*scroll]; }
                }
            }
        }).size_full();
        div().size_full().border_1().border_color(rgb(if self.focused() { palette.hot_edge } else { palette.edge }))
            .bg(rgb(if self.read_only { if palette.dark { palette.background } else { 0xf0f0f0 } } else { palette.softer_background }))
            .child(canvas).into_any_element()
    }
}

impl Drop for NativeKeywordField {
    fn drop(&mut self) { unsafe { let _: () = msg_send![&*self.scroll, removeFromSuperview]; } }
}

struct NativeKeywords {
    key: String,
    default: NativeKeywordField,
    user: NativeKeywordField,
    focused: (bool, bool),
}

pub(super) struct State {
    pub visible: bool,
    pub config: StyleConfig,
    pub focus: FocusHandle,
    pub selected_text_foreground_single_color: bool,
    pub preferences_target: Option<PreferencesTarget>,
    root: Option<PathBuf>,
    loaded: bool,
    snapshot: Option<StyleConfig>,
    lexer: Option<String>,
    style: usize,
    style_scroll: gpui::ScrollHandle,
    style_scroll_drag: Option<f32>,
    dirty: bool,
    theme_dirty: bool,
    restore_invalid: bool,
    choice: Option<Choice>,
    choice_cursor: usize,
    choice_scroll: gpui::ScrollHandle,
    fonts: Vec<String>,
    fields: BTreeMap<String, BoundField>,
    keywords: Option<NativeKeywords>,
    keyword_task: Option<gpui::Task<()>>,
    transparent: bool,
    transparency: u8,
    transparency_dragging: bool,
    color_target: Option<(Option<String>, usize, bool)>,
    color_task: Option<gpui::Task<()>>,
    prompt_task: Option<gpui::Task<()>>,
    prompting: bool,
    error: Option<String>,
}

impl State {
    pub fn new(cx: &mut Context<MatrixWindow>) -> Self {
        let mut fonts = cx.text_system().all_font_names();
        fonts.sort();
        fonts.dedup();
        fonts.insert(0, String::new());
        Self {
            visible: false, config: StyleConfig::new(), focus: cx.focus_handle(), selected_text_foreground_single_color: false, preferences_target: None,
            root: None, loaded: false, snapshot: None, lexer: None, style: 0,
            style_scroll: gpui::ScrollHandle::new(), style_scroll_drag: None,
            dirty: false, theme_dirty: false, restore_invalid: false,
            choice: None, choice_cursor: 0, choice_scroll: gpui::ScrollHandle::new(), fonts, fields: BTreeMap::new(), keywords: None, keyword_task: None,
            transparent: false, transparency: 150, transparency_dragging: false,
            color_target: None, color_task: None, prompt_task: None, prompting: false, error: None,
        }
    }

    pub fn global_style(&self, id: u16, dark: bool) -> Option<&StyleProfile> { self.config.global_style(id, dark) }

    pub fn global_style_named(&self, name: &str, dark: bool) -> Option<&StyleProfile> { self.config.global_style_named(name, dark) }

    pub fn appearance(&self, language: Option<&LanguageProfile>, id: u16, dark: bool, base: &crate::syntax::StyleAppearance, zoom_delta: i32) -> crate::syntax::StyleAppearance { self.config.appearance(language, id, dark, base, zoom_delta) }

    pub fn effective_language(&self, language: &LanguageProfile, dark: bool) -> LanguageProfile { self.config.effective_language(language, dark) }

    pub fn language_for_path(&self, path: &Path, dark: bool) -> Option<u16> { self.config.language_for_path(path, dark) }

    pub fn revision(&self) -> u64 { self.config.revision }

    pub fn opacity(&self) -> f64 { if self.transparent { f64::from(self.transparency) / 255. } else { 1. } }

    fn styles(&self, dark: bool) -> &[StyleProfile] {
        self.config.theme(dark).map(|theme| match self.lexer.as_ref() {
            Some(name) => theme.lexers.get(name).map_or(&[][..], |lexer| lexer.styles.as_slice()),
            None => theme.global_styles.as_slice(),
        }).unwrap_or(&[])
    }

    fn current_style(&self, dark: bool) -> Option<&StyleProfile> { self.styles(dark).get(self.style) }

    fn style_order(&self, dark: bool) -> Vec<usize> {
        let styles = self.styles(dark);
        let mut order = (0..styles.len()).collect::<Vec<_>>();
        if self.lexer.is_none() {
            let prefix = [
                "Global override", "Default Style", "Indent guideline style", "Brace highlight style",
                "Bad brace colour", "Current line background colour", "Selected text colour", "Caret colour",
                "Edge colour", "Line number margin", "Bookmark margin", "Fold", "Fold active", "Fold margin",
                "White space symbol",
            ];
            order.sort_by_key(|index| styles[*index].attributes.get("name")
                .and_then(|name| prefix.iter().position(|entry| *entry == name.as_str()))
                .unwrap_or(prefix.len()));
        }
        order
    }

    fn current_style_mut(&mut self, dark: bool) -> Option<&mut StyleProfile> {
        let style = self.style;
        let lexer = self.lexer.clone();
        let theme = self.config.theme_mut(dark)?;
        match lexer { Some(name) => theme.lexers.get_mut(&name)?.styles.get_mut(style), None => theme.global_styles.get_mut(style) }
    }

    fn languages(&self, dark: bool) -> Vec<(Option<String>, String)> {
        let mut entries = self.config.theme(dark).map(|theme| theme.lexers.iter().map(|(name, lexer)| (Some(name.clone()), lexer.attributes.get("desc").cloned().unwrap_or_else(|| name.clone()))).collect::<Vec<_>>()).unwrap_or_default();
        entries.sort_by(|left, right| match (left.1.as_str(), right.1.as_str()) { ("Search result", "Search result") => std::cmp::Ordering::Equal, ("Search result", _) => std::cmp::Ordering::Greater, (_, "Search result") => std::cmp::Ordering::Less, _ => left.1.cmp(&right.1) });
        entries.insert(0, (None, "Global Styles".into()));
        entries
    }
}

fn at(x: f32, y: f32, w: f32, h: f32) -> gpui::Div {
    div().absolute().left(horizontal_px(if crate::localization::current_preference_rtl() { NOMINAL_WIDTH - (x + w) * 1.75 } else { x * 1.75 })).top(px(y * 1.75)).w(horizontal_px(w * 1.75)).h(px(h * 1.75))
        .when(crate::localization::current_preference_rtl(), |element| element.text_right())
}

fn source_label(text: &str) -> String {
    let label = crate::localization::current_dialog_label("StyleConfig", text);
    crate::localization::current_dialog_label("StyleConfig/SubDialog", label).to_owned()
}

fn label(text: impl Into<gpui::SharedString>, x: f32, y: f32, w: f32) -> gpui::Div { let text: gpui::SharedString = text.into(); at(x, y, w, 10.).child(source_label(text.as_ref())) }

fn group(title: &'static str, x: f32, y: f32, w: f32, h: f32, palette: RenderPalette) -> gpui::Div {
    let mut group = at(x, y, w, h)
        .child(div().absolute().top(px(7.)).left(px(0.)).right(px(0.)).bottom(px(0.)).border_1().border_color(rgb(palette.group_edge())));
    if !title.is_empty() {
        group = group.child(div().absolute().top(px(0.)).h(px(14.)).w_full().flex().justify_center().child(div().h(px(14.)).px(horizontal_px(3.)).line_height(px(14.)).bg(rgb(palette.pure_background)).text_color(rgb(palette.text)).child(source_label(title))));
    }
    group
}

fn indicator(checked: bool, enabled: bool, palette: RenderPalette) -> AnyElement {
    let color = if enabled { palette.text } else { palette.disabled_text };
    let border = if enabled { if palette.dark { palette.edge } else { 0x333333 } }
        else if palette.dark { palette.disabled_edge } else { 0xbfbfbf };
    let mut indicator = div().relative().w(horizontal_px(13.)).h(px(13.)).flex_shrink_0().border_1().border_color(rgb(border))
        .bg(rgb(if palette.dark { palette.softer_background } else { 0xffffff }))
        .flex().items_center().justify_center();
    if checked {
        indicator = indicator.child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            path.move_to(gpui::point(bounds.left() + horizontal_px(1.), bounds.top() + px(5.)));
            path.line_to(gpui::point(bounds.left() + horizontal_px(4.), bounds.top() + px(8.)));
            path.line_to(gpui::point(bounds.left() + horizontal_px(9.), bounds.top() + px(2.)));
            match path.build() {
                Ok(path) => window.paint_path(path, rgb(color)),
                Err(error) => eprintln!("Style check mark could not be drawn: {error}"),
            }
        }).size_full());
    }
    indicator.into_any_element()
}

impl MatrixWindow {
    pub(super) fn refresh_style_config(&mut self, cx: &mut Context<Self>) {
        let root = self.settings_store.as_ref().map(|store| store.root().to_path_buf());
        if self.style_config.loaded && self.style_config.root == root { return; }
        match StyleConfig::load(self.settings_store.as_ref(), self.dark) {
            Ok(config) => {
                self.style_config.config = config;
                self.style_config.root = root;
                self.style_config.loaded = true;
                self.style_config.fields.clear();
                self.style_config.lexer = None;
                self.style_config.style = 0;
                self.style_config.style_scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.style_config.error = None;
            }
            Err(error) => {
                self.style_config.loaded = true;
                self.style_config.root = root;
                self.style_config.error = Some(error.to_string());
                self.notify_editor_error(format!("无法加载样式主题：{error}"), cx);
            }
        }
    }

    pub(super) fn show_style_configurator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.refresh_style_config(cx);
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if !self.style_config.visible {
            self.style_config.snapshot = Some(self.style_config.config.clone());
            self.style_config.dirty = false;
            self.style_config.theme_dirty = false;
            self.style_config.restore_invalid = false;
        }
        self.style_config.visible = true;
        self.present_floating(floating_windows::DialogKind::StyleConfigurator, window, cx);
        cx.notify();
    }

    pub(super) fn show_style_configurator_target(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.show_style_configurator(window, cx);
        self.sync_style_keywords(window, cx);
        self.finish_style_color(window, cx);
        let Some(index) = self.style_config.config.theme(self.dark).and_then(|theme| theme.global_styles.iter().position(|style| style.attributes.get("name").is_some_and(|value| value == name))) else { return; };
        self.style_config.lexer = None;
        self.style_config.style = index;
        self.style_config.fields.clear();
        self.style_config.keywords = None;
        self.style_config.keyword_task = None;
        self.style_config.choice = None;
        let position = self.style_config.style_order(self.dark).iter().position(|entry| *entry == index).unwrap_or(0);
        self.style_config.style_scroll.set_offset(gpui::point(px(0.), px(-(position as f32 * 14.))));
        cx.notify();
    }

    pub(super) fn close_style_configurator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_style_color(window, cx);
        self.style_config.keywords = None;
        self.style_config.keyword_task = None;
        if let Some(snapshot) = self.style_config.snapshot.take() {
            let revision = self.style_config.config.revision.wrapping_add(1);
            let imports = self.style_config.config.themes.iter().filter(|entry| !snapshot.themes.iter().any(|existing| existing.name == entry.name)).cloned().collect::<Vec<_>>();
            let mut restored = snapshot;
            if self.style_config.restore_invalid {
                let index = restored.selected_index(self.dark);
                if let Some(entry) = restored.themes.get(index) {
                    match entry.reload(self.settings_store.as_ref()) {
                        Ok(entry) => restored.themes[index] = entry,
                        Err(error) => self.style_config.error = Some(format!("无法加载样式主题：{error}")),
                    }
                }
                self.style_config.lexer = None;
                self.style_config.style = 0;
                self.style_config.style_scroll.set_offset(gpui::point(px(0.), px(0.)));
            }
            self.style_config.config = restored;
            for entry in imports {
                if !self.style_config.config.themes.iter().any(|existing| existing.name == entry.name) { self.style_config.config.themes.push(entry); }
            }
            self.style_config.config.revision = revision;
        }
        self.style_config.visible = false;
        self.style_config.choice = None;
        self.style_config.fields.clear();
        self.style_config.dirty = false;
        self.style_config.theme_dirty = false;
        self.style_config.transparency_dragging = false;
        self.style_config.style_scroll_drag = None;
        self.invalidate_style_layout(window, cx);
        self.dismiss_floating(floating_windows::DialogKind::StyleConfigurator, window, cx);
    }

    fn save_style_configurator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_style_keywords(window, cx);
        self.finish_style_color(window, cx);
        let result = self.settings_store.as_ref().ok_or_else(|| io::Error::other("配置目录不可用")).and_then(|store| self.style_config.config.save(store, self.dark));
        if let Err(error) = result { self.style_config.error = Some(error.to_string()); cx.notify(); return; }
        self.style_config.keywords = None;
        self.style_config.keyword_task = None;
        self.style_config.snapshot = None;
        self.style_config.visible = false;
        self.style_config.dirty = false;
        self.style_config.theme_dirty = false;
        self.style_config.choice = None;
        self.style_config.fields.clear();
        self.dismiss_floating(floating_windows::DialogKind::StyleConfigurator, window, cx);
        cx.notify();
    }

    fn style_config_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.style_config.dirty = true;
        self.style_config.theme_dirty = true;
        self.style_config.error = None;
        self.style_config.config.revision = self.style_config.config.revision.wrapping_add(1);
        self.invalidate_style_layout(window, cx);
    }

    fn set_style_attribute(&mut self, name: &str, value: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.style_config.current_style(self.dark).is_none_or(|style| style.attributes.get(name) == Some(&value)) { return; }
        if let Some(style) = self.style_config.current_style_mut(self.dark) {
            style.attributes.insert(name.into(), value);
            self.style_config_changed(window, cx);
        }
    }

    fn toggle_style_font_bit(&mut self, mask: i32, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if enabled && let Some(style) = self.style_config.current_style(self.dark) {
            let bits = style_config::number(style, "fontStyle", 0).max(0) ^ mask;
            self.set_style_attribute("fontStyle", bits.to_string(), window, cx);
        }
    }

    fn toggle_style_global_flag(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.style_config.config.global_override.toggle(index);
        self.style_config_changed(window, cx);
    }

    fn show_style_global_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filename = self.native_locale();
        let title = crate::localization::dialog_control(&filename, "StyleConfig/SubDialog", 2235).unwrap_or("What is Global override?");
        let message = crate::localization::localized_string(&filename, "global-override-tip").unwrap_or("Enabling \"Global override\" here will override that parameter in all language styles. What you probably really want is to use the \"Default Style\" settings instead");
        let response = window.prompt(gpui::PromptLevel::Info, title, Some(message), &[gpui::PromptButton::ok("OK")], cx);
        cx.spawn(async move |_, _| { if let Err(error) = response.await { eprintln!("Global override information cannot be shown: {error}"); } }).detach();
    }

    fn select_style_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_style_keywords(window, cx);
        self.finish_style_color(window, cx);
        if index == self.style_config.config.selected_index(self.dark) { return; }
        if self.style_config.theme_dirty {
            let title = self.style_config.config.themes.get(self.style_config.config.selected_index(self.dark)).and_then(|entry| Path::new(&entry.filename).file_stem()).and_then(|name| name.to_str()).unwrap_or("切换主题").to_owned();
            let filename = self.native_locale();
            let message = crate::localization::dialog_message(&filename, "SwitchUnsavedThemeWarning").unwrap_or("Unsaved changes are about to be discarded!\nDo you want to save your changes before switching themes?");
            let yes = crate::localization::dialog_control(&filename, "DoSaveOrNot", 6).unwrap_or("Yes");
            let no = crate::localization::dialog_control(&filename, "DoSaveOrNot", 7).unwrap_or("No");
            let response = window.prompt(gpui::PromptLevel::Warning, &title, Some(message), &[gpui::PromptButton::ok(yes), gpui::PromptButton::cancel(no)], cx);
            self.style_config.prompting = true;
            self.style_config.prompt_task = Some(cx.spawn_in(window, async move |view, cx| {
                match response.await {
                    Ok(answer) => { let _ = view.update_in(cx, |view, window, cx| {
                        view.style_config.prompting = false;
                        if answer == 0 {
                            let result = view.settings_store.as_ref().ok_or_else(|| io::Error::other("配置目录不可用")).and_then(|store| view.style_config.config.save_theme(store, view.dark));
                            if let Err(error) = result { view.style_config.error = Some(error.to_string()); cx.notify(); return; }
                        }
                        view.apply_style_theme(index, window, cx);
                    }); },
                    Err(error) => { let _ = view.update_in(cx, |view, _, cx| { view.style_config.prompting = false; view.style_config.error = Some(error.to_string()); cx.notify(); }); },
                }
            }));
        } else { self.apply_style_theme(index, window, cx); }
    }

    fn apply_style_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_style_color(window, cx);
        if let Err(error) = self.style_config.config.select(index, self.settings_store.as_ref()) {
            self.style_config.error = Some(format!("无法加载样式主题：{error}"));
            cx.notify();
            return;
        }
        self.style_config.keywords = None;
        self.style_config.keyword_task = None;
        self.style_config.lexer = None;
        self.style_config.style = 0;
        self.style_config.style_scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.style_config.choice = None;
        self.style_config.fields.clear();
        self.style_config.restore_invalid = true;
        self.style_config_changed(window, cx);
        self.style_config.theme_dirty = false;
    }

    pub(super) fn import_style_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_style_config(cx);
        if self.style_config.prompting { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let root = self.style_config.root.clone();
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("导入 Notepad++ 样式主题 XML".into()) });
        self.style_config.prompting = true;
        self.style_config.prompt_task = Some(cx.spawn_in(window, async move |view, cx| {
            let paths = match prompt.await {
                Ok(Ok(paths)) => paths,
                Ok(Err(error)) => { let _ = view.update_in(cx, |view, _, cx| { view.style_config.prompting = false; view.style_config.error = Some(error.to_string()); cx.notify(); }); return; },
                Err(error) => { let _ = view.update_in(cx, |view, _, cx| { view.style_config.prompting = false; view.style_config.error = Some(error.to_string()); cx.notify(); }); return; },
            };
            let Some(paths) = paths else { let _ = view.update_in(cx, |view, _, cx| { view.style_config.prompting = false; cx.notify(); }); return; };
            for path in paths {
                let _ = view.update_in(cx, |view, _, cx| {
                    if view.style_config.root != root { return; }
                    let result = view.settings_store.as_ref().ok_or_else(|| io::Error::other("配置目录不可用")).and_then(|store| view.style_config.config.import(&path, store));
                    if let Err(error) = result {
                        view.style_config.error = Some(error.to_string());
                        view.notify_editor_error(format!("样式主题导入失败：{error}"), cx);
                    }
                });
            }
            let _ = view.update_in(cx, |view, _, cx| { view.style_config.prompting = false; cx.notify(); });
        }));
        cx.notify();
    }

    fn style_choice_options(&self, choice: Choice) -> Vec<String> {
        match choice {
            Choice::Theme => self.style_config.config.themes.iter().map(|entry| entry.name.clone()).collect(),
            Choice::Language => self.style_config.languages(self.dark).into_iter().map(|(_, name)| name).collect(),
            Choice::Font => self.style_config.fonts.clone(),
            Choice::FontSize => style_config::FONT_SIZES.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn style_choice_index(&self, choice: Choice) -> usize {
        match choice {
            Choice::Theme => self.style_config.config.selected_index(self.dark),
            Choice::Language => self.style_config.languages(self.dark).iter().position(|(lexer, _)| *lexer == self.style_config.lexer).unwrap_or(0),
            Choice::Font | Choice::FontSize => {
                let key = if choice == Choice::Font { "fontName" } else { "fontSize" };
                let value = self.style_config.current_style(self.dark).and_then(|style| style.attributes.get(key));
                self.style_choice_options(choice).iter().position(|option| Some(option) == value).unwrap_or(0)
            }
        }
    }

    fn choose_style_value(&mut self, choice: Choice, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_style_keywords(window, cx);
        self.style_config.choice = None;
        match choice {
            Choice::Theme => self.select_style_theme(index, window, cx),
            Choice::Language => {
                self.finish_style_color(window, cx);
                if let Some((lexer, _)) = self.style_config.languages(self.dark).get(index) {
                    self.style_config.lexer = lexer.clone();
                    self.style_config.style = 0;
                    self.style_config.style_scroll.set_offset(gpui::point(px(0.), px(0.)));
                    self.style_config.fields.clear();
                }
            }
            Choice::Font => { if let Some(name) = self.style_config.fonts.get(index).cloned() { self.set_style_attribute("fontName", name, window, cx); } }
            Choice::FontSize => { if let Some(value) = style_config::FONT_SIZES.get(index) { self.set_style_attribute("fontSize", if value.is_empty() { "0".into() } else { value.to_string() }, window, cx); } }
        }
        cx.notify();
    }

    fn style_combo(&self, id: &'static str, choice: Choice, text: String, enabled: bool, x: f32, y: f32, w: f32, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let label = match choice { Choice::Theme => "选择主题", Choice::Language => "语言", Choice::Font => "字体名称", Choice::FontSize => "字体大小" };
        let source_id = match choice { Choice::Theme => 2306, Choice::Language => 2225, Choice::Font => 2208, Choice::FontSize => 2209 };
        let label = crate::localization::current_dialog_control(if choice == Choice::Theme { "StyleConfig" } else { "StyleConfig/SubDialog" }, source_id).unwrap_or(label);
        at(x, y, w, 13.).id(id).role(Role::ComboBox).aria_label(label).aria_value(text.clone()).aria_expanded(self.style_config.choice == Some(choice)).tab_index(if enabled { 0 } else { -1 })
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .border_1().border_color(rgb(palette.control_edge(enabled))).bg(rgb(if palette.dark { palette.control_background(enabled) } else if enabled { 0xe1e1e1 } else { 0xf0f0f0 }))
            .flex().items_center().justify_between().px(horizontal_px(4.)).overflow_hidden()
            .when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .text_color(rgb(palette.control_text(enabled)))
            .when(enabled, |element| element.hover(move |style| style.bg(rgb(palette.hot_background)).border_color(rgb(palette.hot_edge)))
                .focus(move |style| style.border_color(rgb(palette.hot_edge))))
            .child(text).child("⌄")
            .on_click(cx.listener(move |view, _, _, cx| {
                if enabled && !view.style_config.prompting {
                    view.style_config.choice = if view.style_config.choice == Some(choice) { None } else { Some(choice) };
                    view.style_config.choice_cursor = view.style_choice_index(choice);
                    view.style_config.choice_scroll.scroll_to_item(view.style_config.choice_cursor);
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if !enabled { return; }
                if view.style_config.choice != Some(choice) { view.style_config.choice_cursor = view.style_choice_index(choice); }
                let length = view.style_choice_options(choice).len();
                match event.keystroke.key.as_str() {
                    "down" => { view.style_config.choice = Some(choice); view.style_config.choice_cursor = (view.style_config.choice_cursor + 1).min(length.saturating_sub(1)); },
                    "up" => { view.style_config.choice = Some(choice); view.style_config.choice_cursor = view.style_config.choice_cursor.saturating_sub(1); },
                    "enter" | "space" => if view.style_config.choice == Some(choice) { view.choose_style_value(choice, view.style_config.choice_cursor, window, cx); } else { view.style_config.choice = Some(choice); },
                    "escape" => { view.style_config.choice = None; },
                    _ => return,
                }
                view.style_config.choice_scroll.scroll_to_item(view.style_config.choice_cursor);
                cx.stop_propagation(); cx.notify();
            })).into_any_element()
    }

    fn style_field(&mut self, name: &'static str, text: String, read_only: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let key = format!("{:?}:{}:{}", self.style_config.lexer, self.style_config.style, name);
        if !self.style_config.fields.contains_key(&key) {
            let field = cx.new(|cx| {
                let label = match name { "default-ext" => "默认扩展名", "user-ext" => "自定义扩展名", _ => name };
                let label = crate::localization::current_dialog_control("StyleConfig/SubDialog", if name == "default-ext" { 2214 } else { 2216 }).unwrap_or(label);
                let field = text_field::TextField::compact(label, "style-config-input", cx).without_history_arrow();
                if read_only { field.with_read_only() } else { field }
            });
            field.update(cx, |field, cx| field.set_text(&text, window, cx));
            let lexer = self.style_config.lexer.clone();
            let style = self.style_config.style;
            let changed = cx.observe_in(&field, window, move |view, field, window, cx| {
                if read_only || view.style_config.lexer != lexer || view.style_config.style != style || field.read(cx).has_composition() || !view.style_config.visible { return; }
                let text = field.read(cx).text();
                if name == "user-ext" {
                    let Some(lexer) = lexer.as_ref() else { return; };
                    let text = text.chars().scan(0usize, |units, ch| { *units += ch.len_utf16(); (*units <= 255).then_some(ch) }).collect::<String>();
                    if view.style_config.config.user_extensions(lexer, view.dark) == text { return; }
                    let Some(target) = view.style_config.config.theme_mut(view.dark).and_then(|theme| theme.lexers.get_mut(lexer)) else { return; };
                    target.attributes.insert("ext".into(), text);
                } else { return; }
                view.style_config_changed(window, cx);
            });
            self.style_config.fields.insert(key.clone(), BoundField { entity: field, _changed: changed });
        }
        let entity = &self.style_config.fields[&key].entity;
        let mut palette = self.preferences_render_palette();
        if read_only && !palette.dark { palette.background = 0xf0f0f0; }
        preferences_surface::sync_text_field_palette(entity, palette, cx);
        entity.clone().into_any_element()
    }

    pub(super) fn finish_style_color(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_style_color(window, cx);
        if self.style_config.color_target.take().is_some() {
            unsafe {
                let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                let _: () = msg_send![panel, orderOut: std::ptr::null::<AnyObject>()];
            }
        }
        self.style_config.color_task = None;
    }

    fn sync_style_color(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((lexer, style_index, background)) = self.style_config.color_target.clone() else { return; };
        if !self.style_config.visible || self.style_config.lexer != lexer || self.style_config.style != style_index { return; }
        let Some(value) = color_panel_rgb() else { return; };
        let key = if background { "bgColor" } else { "fgColor" };
        self.set_style_attribute(key, format!("{value:06X}"), window, cx);
    }

    fn sync_style_keywords(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = format!("{:?}:{}", self.style_config.lexer, self.style_config.style);
        let Some(value) = self.style_config.keywords.as_ref().filter(|fields| fields.key == key).and_then(|fields| fields.user.value()) else { return; };
        if self.style_config.current_style(self.dark).is_none_or(|style| style.keywords == value) { return; }
        let Some(style) = self.style_config.current_style_mut(self.dark) else { return; };
        style.keywords = value;
        self.style_config_changed(window, cx);
    }

    fn native_style_keywords(&mut self, defaults: &str, user: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<(AnyElement, AnyElement)> {
        let palette = self.preferences_render_palette();
        let key = format!("{:?}:{}", self.style_config.lexer, self.style_config.style);
        if self.style_config.keywords.as_ref().is_none_or(|fields| fields.key != key) {
            self.style_config.keyword_task = None;
            let fields = (|| -> io::Result<NativeKeywords> {
                Ok(NativeKeywords { key: key.clone(), default: NativeKeywordField::new(defaults, true, palette)?, user: NativeKeywordField::new(user, false, palette)?, focused: (false, false) })
            })();
            self.style_config.keywords = match fields {
                Ok(fields) => Some(fields),
                Err(error) => { self.style_config.error = Some(error.to_string()); return None; }
            };
            let executor = cx.background_executor().clone();
            self.style_config.keyword_task = Some(cx.spawn_in(window, async move |view, cx| {
                loop {
                    executor.timer(std::time::Duration::from_millis(50)).await;
                    let keep = view.update_in(cx, |view, window, cx| {
                        if !view.style_config.visible || view.style_config.keywords.as_ref().is_none_or(|fields| fields.key != key) { return false; }
                        view.sync_style_keywords(window, cx);
                        if let Some(fields) = &mut view.style_config.keywords {
                            let focused = (fields.default.focused(), fields.user.focused());
                            if fields.focused != focused { fields.focused = focused; cx.notify(); }
                        }
                        true
                    }).unwrap_or(false);
                    if !keep { break; }
                }
            }));
        }
        let hidden = self.style_config.choice.is_some();
        self.style_config.keywords.as_ref().map(|fields| (fields.default.element(palette, hidden), fields.user.element(palette, hidden)))
    }

    fn choose_style_color(&mut self, background: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_udl_styler_color(cx);
        if !self.finish_preference_color(window, cx) { return; }
        self.finish_style_color(window, cx);
        let key = if background { "bgColor" } else { "fgColor" };
        let Some(value) = self.style_config.current_style(self.dark).and_then(|style| style.attributes.get(key)).and_then(|value| u32::from_str_radix(value, 16).ok()) else { return; };
        unsafe {
            let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
            let color: *mut AnyObject = msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64];
            let _: () = msg_send![panel, setShowsAlpha: false];
            let _: () = msg_send![panel, setColor: color];
            let _: () = msg_send![panel, orderFront: std::ptr::null::<AnyObject>()];
        }
        self.style_config.color_target = Some((self.style_config.lexer.clone(), self.style_config.style, background));
        let executor = cx.background_executor().clone();
        self.style_config.color_task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(50)).await;
                let keep = view.update_in(cx, |view, window, cx| {
                    let Some((lexer, style_index, _)) = view.style_config.color_target.clone() else { return false; };
                    if !view.style_config.visible || view.style_config.lexer != lexer || view.style_config.style != style_index { return false; }
                    let visible: Bool = unsafe {
                        let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                        msg_send![panel, isVisible]
                    };
                    view.sync_style_color(window, cx);
                    if !visible.as_bool() { view.style_config.color_target = None; }
                    visible.as_bool()
                }).unwrap_or(false);
                if !keep { break; }
            }
        }));
    }

    fn apply_style_transparency(&self, window: &mut Window) {
        if let Ok(handle) = HasWindowHandle::window_handle(window) && let RawWindowHandle::AppKit(handle) = handle.as_raw() {
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            if let Some(host) = view.window() {
                let palette = self.preferences_render_palette();
                let background = native_rgb(palette.pure_background);
                host.setAlphaValue(self.style_config.opacity());
                unsafe {
                    let _: () = msg_send![&*host, setBackgroundColor: &*background];
                    let name = NSString::from_str(if palette.dark { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" });
                    let appearance: *mut AnyObject = msg_send![class!(NSAppearance), appearanceNamed: &*name];
                    let _: () = msg_send![&*host, setAppearance: appearance];
                }
            }
        }
    }

    fn style_scrollbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let scroll = self.style_config.style_scroll.clone();
        div().id("style-list-scrollbar").role(Role::ScrollBar).aria_label(source_label("样式:")).aria_orientation(gpui::accesskit::Orientation::Vertical)
            .map(|element| preferences_editing_surface::scrollbar_accessibility(element, scroll.clone()))
            .w(horizontal_px(17.)).h_full().flex_shrink_0().border_y_1().border_r_1().border_color(rgb(palette.edge))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                let scroll = &view.style_config.style_scroll;
                let height = dialog_font::logical(scroll.bounds().size.height);
                let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
                if maximum <= 0. { return; }
                let y = dialog_font::logical(event.position.y - scroll.bounds().top());
                let track = (height - 34.).max(0.);
                let thumb = (track * height / (height + maximum)).max(17.).min(track);
                let offset = -dialog_font::logical(scroll.offset().y);
                let top = 17. + (track - thumb) * offset / maximum;
                let next = if y < 17. { offset - 14. } else if y > height - 17. { offset + 14. }
                    else if y < top { offset - height } else if y > top + thumb { offset + height }
                    else { view.style_config.style_scroll_drag = Some(y - top); offset };
                scroll.set_offset(gpui::point(px(0.), px(-next.clamp(0., maximum))));
                cx.stop_propagation(); cx.notify();
            }))
            .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                let height = dialog_font::logical(bounds.size.height);
                let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
                window.paint_quad(gpui::fill(bounds, rgb(if palette.dark { palette.background } else { 0xf0f0f0 })));
                if maximum > 0. {
                    let track = (height - 34.).max(0.);
                    let viewport = dialog_font::logical(scroll.bounds().size.height);
                    let thumb = (track * viewport / (viewport + maximum)).max(17.).min(track);
                    let top = 17. + (track - thumb) * (-dialog_font::logical(scroll.offset().y) / maximum).clamp(0., 1.);
                    window.paint_quad(gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(px(0.), px(top)), size(bounds.size.width, px(thumb))), rgb(if palette.dark { palette.disabled_edge } else { 0xcdcdcd })));
                }
                for (y, direction) in [(8., -1.), (height - 8., 1.)] {
                    let x = dialog_font::logical(bounds.size.width) / 2.;
                    let mut path = gpui::PathBuilder::stroke(px(1.));
                    path.move_to(bounds.origin + gpui::point(px(x) - horizontal_px(3.), px(y - direction * 1.5)));
                    path.line_to(bounds.origin + gpui::point(px(x), px(y + direction * 1.5)));
                    path.line_to(bounds.origin + gpui::point(px(x) + horizontal_px(3.), px(y - direction * 1.5)));
                    match path.build() {
                        Ok(path) => window.paint_path(path, rgb(palette.control_text(maximum > 0.))),
                        Err(error) => eprintln!("Style scrollbar arrow cannot be drawn: {error}"),
                    }
                }
            }).size_full()).into_any_element()
    }

    fn drag_style_scrollbar(&mut self, y: gpui::Pixels, cx: &mut Context<Self>) {
        let Some(grab) = self.style_config.style_scroll_drag else { return; };
        let scroll = &self.style_config.style_scroll;
        let height = dialog_font::logical(scroll.bounds().size.height);
        let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
        let track = (height - 34.).max(0.);
        let thumb = (track * height / (height + maximum)).max(17.).min(track);
        let travel = track - thumb;
        if travel > 0. {
            let position = (dialog_font::logical(y - scroll.bounds().top()) - 17. - grab) / travel;
            scroll.set_offset(gpui::point(px(0.), px(-position.clamp(0., 1.) * maximum)));
            cx.notify();
        }
    }

    pub(super) fn render_style_configurator(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let filename = self.native_locale();
        crate::localization::with_preference(&filename, "", || self.render_localized_style_configurator(window, cx))
    }

    fn render_localized_style_configurator(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.style_config.visible { return None; }
        let filename = self.native_locale();
        let title = crate::localization::dialog_title(&filename, "StyleConfig").unwrap_or(TITLE);
        window_caption::update_title(window, title);
        self.apply_style_transparency(window);
        let dark = self.dark;
        let palette = self.preferences_render_palette();
        for (key, field) in &self.style_config.fields {
            let mut field_palette = palette;
            if key.ends_with(":default-ext") && !palette.dark { field_palette.background = 0xf0f0f0; }
            preferences_surface::sync_text_field_palette(&field.entity, field_palette, cx);
        }
        let selection_background = self.appearance_colour("DarkMode.customColorMenuHotTrack", 0x0078d7);
        let selection_text = self.appearance_colour("DarkMode.customColorText", 0xffffff);
        let style = self.style_config.current_style(dark).cloned();
        let name = style.as_ref().and_then(|style| style.attributes.get("name")).cloned().unwrap_or_default();
        let languages = self.style_config.languages(dark);
        let language_name = languages.iter().find(|(lexer, _)| *lexer == self.style_config.lexer).map(|(_, name)| name.clone()).unwrap_or_else(|| "Global Styles".into());
        let theme_name = self.style_config.config.themes.get(self.style_config.config.selected_index(dark)).map(|entry| entry.name.clone()).unwrap_or_default();
        let font_enabled = style.as_ref().is_some_and(|style| style.attributes.contains_key("fontName"));
        let mut root = div().id("style-configurator").role(Role::Dialog).aria_label(title)
            .track_focus(&self.style_config.focus).tab_group().size_full().relative()
            .bg(rgb(palette.pure_background)).text_color(rgb(palette.text)).font_family(dialog_font::font_family()).text_size(px(12.)).line_height(px(14.))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    if view.style_config.choice.take().is_none() { view.close_style_configurator(window, cx); }
                    cx.stop_propagation(); cx.notify();
                }
            }))
            .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| { window.focus_next(cx); cx.stop_propagation(); }))
            .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| { window.focus_prev(cx); cx.stop_propagation(); }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|view, _, _, _| { view.style_config.transparency_dragging = false; view.style_config.style_scroll_drag = None; }))
            .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(|view, _, _, _| { view.style_config.transparency_dragging = false; view.style_config.style_scroll_drag = None; }))
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, window, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    view.style_config.transparency_dragging = false;
                    view.style_config.style_scroll_drag = None;
                    return;
                }
                if view.style_config.transparency_dragging { view.set_style_transparency_position(event.position.x, window, cx); }
                if view.style_config.style_scroll_drag.is_some() { view.drag_style_scrollbar(event.position.y, cx); }
            }))
            .child(label("选择主题:", 6., 8., 83.).text_right())
            .child(self.style_combo("style-theme", Choice::Theme, theme_name, true, 95., 7., 104., cx))
            .child(group("", 7., 24., 166., 212., palette))
            .child(group("", 181., 24., 312., 184., palette))
            .child(label("语言:", 20., 35., 137.))
            .child(self.style_combo("style-language", Choice::Language, language_name.clone(), true, 17., 46., 146., cx))
            .child(label("样式:", 20., 63., 137.))
            .child(label(format!("{language_name}: {name}"), 190., 30., 295.).text_color(rgb(palette.link_text)))
            .child(group("色彩样式", 191., 52., 111., 77., palette))
            .child(group("字体样式", 312., 52., 171., 77., palette));
        let style_order = self.style_config.style_order(dark);
        let mut styles = div().id("style-list").role(Role::ListBox).aria_label(source_label("样式:")).tab_index(0).h_full().flex_1().min_w(px(0.))
            .border_1().border_color(rgb(palette.edge)).bg(rgb(palette.softer_background))
            .focus(move |style| style.border_color(rgb(palette.hot_edge))).overflow_y_scroll().track_scroll(&self.style_config.style_scroll)
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()));
        for index in style_order {
            let style = &self.style_config.styles(dark)[index];
            let selected = index == self.style_config.style;
            let style_name = style.attributes.get("name").cloned().unwrap_or_default();
            styles = styles.child(div().id(gpui::SharedString::from(format!("style-list-{index}"))).role(Role::ListBoxOption).aria_label(style_name.clone()).aria_selected(selected).w_full().h(px(14.)).min_h(px(14.)).line_height(px(14.)).px(horizontal_px(1.)).overflow_hidden()
                .bg(rgb(if selected { selection_background } else { palette.softer_background }))
                .text_color(rgb(if selected { selection_text } else { palette.text }))
                .when(!selected, |row| row.hover(move |style| style.bg(rgb(palette.hot_background)).text_color(rgb(palette.text))))
                .child(style_name)
                .on_click(cx.listener(move |view, _, window, cx| { view.sync_style_keywords(window, cx); view.finish_style_color(window, cx); view.style_config.style = index; view.style_config.fields.clear(); view.style_config.choice = None; cx.notify(); })));
        }
        styles = styles.on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            let order = view.style_config.style_order(view.dark);
            let position = order.iter().position(|index| *index == view.style_config.style).unwrap_or(0);
            let position = match event.keystroke.key.as_str() { "down" => (position + 1).min(order.len().saturating_sub(1)), "up" => position.saturating_sub(1), "home" => 0, "end" => order.len().saturating_sub(1), _ => return };
            let Some(index) = order.get(position).copied() else { return; };
            view.sync_style_keywords(window, cx); view.finish_style_color(window, cx); view.style_config.style = index; view.style_config.fields.clear(); view.style_config.style_scroll.scroll_to_item(position); cx.stop_propagation(); cx.notify();
        }));
        root = root.child(at(17., 74., 146., 118.).flex().child(styles).child(self.style_scrollbar(cx)));
        for (background, text, y) in [(false, "前景色", 72.), (true, "背景色", 99.)] {
            let key = if background { "bgColor" } else { "fgColor" };
            let mask = if background { 2 } else { 1 };
            let color = style.as_ref().and_then(|style| style.attributes.get(key)).and_then(|value| u32::from_str_radix(value, 16).ok());
            let enabled = color.is_some_and(|color| color <= 0xffffff) && !(name == "Selected text colour" && !background && !self.style_config.selected_text_foreground_single_color);
            let used = style.as_ref().is_some_and(|style| style_config::number(style, "colorStyle", 3) & mask != 0);
            root = root.child(label(text, 203., y, 59.).text_right().text_color(rgb(palette.control_text(enabled))))
                .child(at(265., y - 3., 14., 14.).id(if background { "style-bg" } else { "style-fg" }).role(Role::Button).aria_label(source_label(text)).tab_index(if enabled { 0 } else { -1 })
                    .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
                    .border_1().border_color(rgb(if enabled && !palette.dark { 0x000000 } else { palette.control_edge(enabled) })).text_color(rgb(palette.control_text(enabled)))
                    .bg(rgb(if enabled { color.filter(|color| *color <= 0xffffff).unwrap_or(palette.softer_background) } else if palette.dark { palette.pure_background } else { 0xf0f0f0 }))
                    .when(enabled, |element| element.hover(move |style| style.border_color(rgb(palette.hot_edge)))
                        .focus(move |style| style.border_color(rgb(palette.hot_edge))))
                    .child(if used { "" } else { "╳" })
                    .on_click(cx.listener(move |view, _, window, cx| { if enabled { view.choose_style_color(background, window, cx); } }))
                    .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |view, _, window, cx| {
                        if enabled && let Some(style) = view.style_config.current_style(view.dark) {
                            let bits = style_config::number(style, "colorStyle", 3) ^ mask;
                            view.set_style_attribute("colorStyle", bits.to_string(), window, cx);
                        }
                    }))
                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { if enabled && event.keystroke.key == "space" { view.choose_style_color(background, window, cx); cx.stop_propagation(); } })));
        }
        let font_name = style.as_ref().and_then(|style| style.attributes.get("fontName")).cloned().unwrap_or_default();
        let font_size = style.as_ref().and_then(|style| style.attributes.get("fontSize")).filter(|value| value.as_str() != "0" && value.as_str() != "-1").cloned().unwrap_or_default();
        let font_name_used = !font_name.is_empty();
        let font_size_used = style.as_ref().is_some_and(|style| { let size = style_config::number(style, "fontSize", -1); size != -1 && size < 100 });
        root = root.child(label("字体名称:", 315., 69., 52.).text_right().text_color(rgb(palette.control_text(font_name_used))))
            .child(self.style_combo("style-font", Choice::Font, font_name, font_enabled, 371., 68., 104., cx))
            .child(label("字体大小:", 388., 96., 51.).text_right().text_color(rgb(palette.control_text(font_size_used))))
            .child(self.style_combo("style-font-size", Choice::FontSize, font_size, font_enabled, 442., 95., 33., cx));
        for (mask, text, y) in [(1, "粗体", 88.), (2, "斜体", 101.), (4, "下划线", 114.)] {
            let text = source_label(text);
            let checked = style.as_ref().is_some_and(|style| style_config::number(style, "fontStyle", 0).max(0) & mask != 0);
            root = root.child(at(327., y, 80., 10.).id(gpui::SharedString::from(format!("style-font-bit-{mask}"))).role(Role::CheckBox).aria_label(text.clone()).aria_toggled(checked.into()).tab_index(if font_enabled { 0 } else { -1 }).flex().items_center().gap(horizontal_px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
                .a11y_synthetic_children(move |builder| { if !font_enabled { builder.parent_node().set_disabled(); } })
                .text_color(rgb(if font_enabled { palette.darker_text } else { palette.disabled_text }))
                .when(font_enabled, |element| element.hover(move |style| style.text_color(rgb(palette.text))).focus(move |style| style.text_color(rgb(palette.text))))
                .child(indicator(checked, font_enabled, palette)).child(text)
                .on_click(cx.listener(move |view, _, window, cx| view.toggle_style_font_bit(mask, font_enabled, window, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { if event.keystroke.key == "space" { view.toggle_style_font_bit(mask, font_enabled, window, cx); cx.stop_propagation(); } })));
        }
        if name == "Global override" {
            self.style_config.keywords = None;
            self.style_config.keyword_task = None;
            for (index, text, x, y) in [(0, "使用全局前景色", 191., 134.), (1, "使用全局背景色", 191., 149.), (2, "使用全局字体", 327., 134.), (3, "使用全局字体大小", 327., 149.), (4, "使用全局粗体样式", 327., 164.), (5, "使用全局斜体样式", 327., 179.), (6, "使用全局下划线样式", 327., 194.)] {
                let text = source_label(text);
                let checked = self.style_config.config.global_override.flag(index);
                root = root.child(at(x, y, 153., 10.).id(gpui::SharedString::from(format!("style-global-flag-{index}"))).role(Role::CheckBox).aria_label(text.clone()).aria_toggled(checked.into()).tab_index(0).flex().items_center().gap(horizontal_px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
                    .text_color(rgb(palette.darker_text)).hover(move |style| style.text_color(rgb(palette.text))).focus(move |style| style.text_color(rgb(palette.text)))
                    .child(indicator(checked, true, palette)).child(text)
                    .on_click(cx.listener(move |view, _, window, cx| view.toggle_style_global_flag(index, window, cx)))
                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { if event.keystroke.key == "space" { view.toggle_style_global_flag(index, window, cx); cx.stop_propagation(); } })));
            }
            let tip = crate::localization::localized_string(&filename, "global-override-tip").unwrap_or("Enabling \"Global override\" here will override that parameter in all language styles. What you probably really want is to use the \"Default Style\" settings instead").to_owned();
            root = root.child(at(191., 173., 132., 10.).id("style-global-help").role(Role::Link).aria_label(source_label("什么是“Global override”？")).text_color(rgb(palette.link_text)).underline().tab_index(0).child(source_label("什么是“Global override”？"))
                .tooltip(move |_, cx| icon_tooltip::view(tip.clone(), dark, cx))
                .on_click(cx.listener(|view, _, window, cx| view.show_style_global_help(window, cx)))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.show_style_global_help(window, cx); cx.stop_propagation(); }
                })));
        } else if let Some(class) = style.as_ref().and_then(|style| style.attributes.get("keywordClass")).and_then(|class| style_config::keyword_index(class)) {
            let language = self.style_config.lexer.as_ref().and_then(|lexer| crate::languages::catalog().ok().and_then(|catalog| catalog.profiles().iter().find(|language| language.name == *lexer || language.effective_style_sources.contains(lexer))));
            let defaults = language.map(|language| language.keywords(class).to_owned()).unwrap_or_default();
            let user = style.as_ref().map(|style| style.keywords.clone()).unwrap_or_default();
            if let Some((default_field, user_field)) = self.native_style_keywords(&defaults, &user, window, cx) {
                root = root.child(label("默认关键字", 194., 135., 122.)).child(label("自定义关键字", 350., 135., 126.))
                    .child(at(191., 146., 136., 52.).child(default_field)).child(label("+", 334., 168., 8.))
                    .child(at(347., 146., 136., 52.).child(user_field));
            }
        } else { self.style_config.keywords = None; self.style_config.keyword_task = None; }
        if let Some(lexer) = self.style_config.lexer.clone() {
            let defaults = crate::languages::catalog().ok().and_then(|catalog| catalog.profiles().iter().find(|language| language.name == lexer || language.effective_style_sources.contains(&lexer))).map(|language| language.extensions.join(" ")).unwrap_or_default();
            let user = self.style_config.config.user_extensions(&lexer, dark).to_owned();
            let default_field = self.style_field("default-ext", defaults, true, window, cx);
            let user_field = self.style_field("user-ext", user, false, window, cx);
            root = root.child(label("默认扩展名:", 20., 203., 61.)).child(label("自定义扩展名:", 96., 203., 71.))
                .child(at(17., 214., 59., 12.).child(default_field)).child(label("+", 82., 216., 8.))
                .child(at(93., 214., 70., 12.).child(user_field));
        }
        if let Some(target) = preferences_target(&name) {
            root = root.child(at(210., 153., 131., 10.).id("style-go-settings").role(Role::Link).aria_label(source_label("前往首选项")).tab_index(0).text_color(rgb(palette.link_text)).underline().child(source_label("前往首选项"))
                .on_click(cx.listener(move |view, _, window, cx| { view.style_config.preferences_target = Some(target); view.show_preferences(window, cx); })));
        }
        let can_save = self.style_config.dirty && !self.style_config.prompting;
        root = root.child(at(264., 222., 69., 14.).h(px(23.)).id("style-save-close").role(Role::Button).aria_label(source_label("保存并关闭")).tab_index(if can_save { 0 } else { -1 })
            .a11y_synthetic_children(move |builder| { if !can_save { builder.parent_node().set_disabled(); } })
            .border_1().border_color(rgb(palette.control_edge(can_save))).bg(rgb(if palette.dark { palette.button_background(can_save) } else if can_save { 0xe1e1e1 } else { 0xcccccc }))
            .text_color(rgb(if can_save { palette.darker_text } else { palette.disabled_text }))
            .when(can_save, |element| element.hover(move |style| style.bg(rgb(palette.hot_background)).border_color(rgb(palette.hot_edge)).text_color(rgb(palette.text)))
                .focus(move |style| style.border_color(rgb(palette.hot_edge))))
            .flex().items_center().justify_center().child(source_label("保存并关闭"))
            .on_click(cx.listener(move |view, _, window, cx| { if can_save { view.save_style_configurator(window, cx); } }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { if can_save && matches!(event.keystroke.key.as_str(), "enter" | "space") { view.save_style_configurator(window, cx); cx.stop_propagation(); } })))
            .child(at(337., 222., 50., 14.).h(px(23.)).id("style-cancel").role(Role::Button).aria_label(source_label("取消")).tab_index(0)
                .border_1().border_color(rgb(palette.edge)).bg(rgb(if palette.dark { palette.button_background(true) } else { 0xe1e1e1 })).text_color(rgb(palette.darker_text))
                .hover(move |style| style.bg(rgb(palette.hot_background)).border_color(rgb(palette.hot_edge)).text_color(rgb(palette.text)))
                .focus(move |style| style.border_color(rgb(palette.hot_edge))).flex().items_center().justify_center().child(source_label("取消"))
                .on_click(cx.listener(|view, _, window, cx| view.close_style_configurator(window, cx)))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| { if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.close_style_configurator(window, cx); cx.stop_propagation(); } })))
            .child(at(431., 221., 63., 10.).id("style-transparency").role(Role::CheckBox).aria_label(source_label("透明度")).aria_toggled(self.style_config.transparent.into()).tab_index(0).flex().items_center().gap(horizontal_px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
                .text_color(rgb(palette.darker_text)).hover(move |style| style.text_color(rgb(palette.text))).focus(move |style| style.text_color(rgb(palette.text)))
                .child(indicator(self.style_config.transparent, true, palette)).child(source_label("透明度"))
                .on_click(cx.listener(|view, _, window, cx| { view.style_config.transparent = !view.style_config.transparent; view.apply_style_transparency(window); cx.notify(); }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| { if event.keystroke.key == "space" { view.style_config.transparent = !view.style_config.transparent; view.apply_style_transparency(window); cx.stop_propagation(); cx.notify(); } })));
        let position = f32::from(self.style_config.transparency.saturating_sub(20)) / 180.;
        let position = if crate::localization::current_preference_rtl() { 1. - position } else { position };
        let transparency_enabled = self.style_config.transparent;
        root = root.child(at(441., 235., 53., 10.).id("style-transparency-slider").role(Role::Slider).aria_label(source_label("透明度")).aria_min_numeric_value(20.).aria_max_numeric_value(200.).aria_numeric_value(f64::from(self.style_config.transparency)).tab_index(if transparency_enabled { 0 } else { -1 })
            .a11y_synthetic_children(move |builder| { if !transparency_enabled { builder.parent_node().set_disabled(); } })
            .when(transparency_enabled, |element| element.focus(move |style| style.border_1().border_color(rgb(palette.hot_edge))))
            .child(div().absolute().top(px(7.)).w_full().h(px(3.)).bg(rgb(palette.control_edge(transparency_enabled))))
            .child(div().absolute().left(horizontal_px(position * 88.)).top(px(2.)).w(horizontal_px(6.)).h(px(13.))
                .border_1().border_color(rgb(palette.control_edge(transparency_enabled)))
                .bg(rgb(if transparency_enabled { palette.softer_background } else { palette.disabled_edge })))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                if !view.style_config.transparent { return; }
                view.style_config.transparency_dragging = true;
                view.set_style_transparency_position(event.position.x, window, cx);
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if !view.style_config.transparent { return; }
                let rtl = crate::localization::is_rtl(&view.native_locale());
                let key = match event.keystroke.key.as_str() { "left" if rtl => "right", "right" if rtl => "left", key => key };
                view.style_config.transparency = match key { "left" | "down" => view.style_config.transparency.saturating_sub(1).max(20), "right" | "up" => view.style_config.transparency.saturating_add(1).min(200), "home" => 20, "end" => 200, _ => return };
                view.apply_style_transparency(window); cx.stop_propagation(); cx.notify();
            })));
        if let Some(error) = &self.style_config.error { root = root.child(label(error.clone(), 181., 211., 240.).bg(rgb(palette.error_background)).text_color(rgb(palette.text))); }
        if let Some(choice) = self.style_config.choice {
            let (x, y, w) = match choice { Choice::Theme => (95., 20., 150.), Choice::Language => (17., 59., 170.), Choice::Font => (371., 81., 121.), Choice::FontSize => (442., 108., 42.) };
            let options = self.style_choice_options(choice);
            let h = ((options.len() as f32 * 14. + 2.) / 1.75).min(130.);
            let mut popup = at(x, y, w, h).id("style-choice-popup").role(Role::ListBox).aria_label(match choice { Choice::Theme => "选择主题", Choice::Language => "语言", Choice::Font => "字体名称", Choice::FontSize => "字体大小" }).border_1().border_color(rgb(palette.edge)).bg(rgb(palette.softer_background)).overflow_y_scroll().track_scroll(&self.style_config.choice_scroll);
            for (index, option) in options.into_iter().enumerate() {
                let current = index == self.style_config.choice_cursor;
                popup = popup.child(div().id(gpui::SharedString::from(format!("style-choice-{index}"))).role(Role::ListBoxOption).aria_label(option.clone()).aria_selected(current).h(px(14.)).min_h(px(14.)).line_height(px(14.)).w_full().px(horizontal_px(4.))
                    .bg(rgb(if current { selection_background } else { palette.softer_background })).text_color(rgb(if current { selection_text } else { palette.text }))
                    .hover(move |style| style.bg(rgb(palette.hot_background)).text_color(rgb(palette.text))).child(option)
                    .on_click(cx.listener(move |view, _, window, cx| view.choose_style_value(choice, index, window, cx))));
            }
            root = root.child(popup);
        }
        Some(root.into_any_element())
    }

    fn set_style_transparency_position(&mut self, x: gpui::Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let rtl = crate::localization::is_rtl(&self.native_locale());
        let left = horizontal_px(if rtl { NOMINAL_WIDTH - (441. + 53.) * 1.75 } else { 441. * 1.75 });
        let ratio = (f32::from(x - left) / f32::from(horizontal_px(53. * 1.75))).clamp(0., 1.);
        let ratio = if rtl { 1. - ratio } else { ratio };
        self.style_config.transparency = (20. + ratio * 180.).round() as u8;
        self.apply_style_transparency(window);
        cx.notify();
    }
}
