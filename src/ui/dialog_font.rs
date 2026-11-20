use gpui::{App, Pixels};
use objc2::{class, msg_send};
use objc2::runtime::AnyObject;
use objc2::rc::Retained;
use objc2_foundation::NSString;
use serde_json::Value;
use std::cell::OnceCell;

pub(super) const SIZES: [i64; 11] = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24];

struct DialogFont {
    family: String,
    size: f32,
}

thread_local! {
    static FONT: OnceCell<DialogFont> = const { OnceCell::new() };
}

pub(super) fn system_font_family() -> String {
    unsafe {
        let font: Retained<AnyObject> = msg_send![class!(NSFont), systemFontOfSize: 12f64];
        let family: Retained<NSString> = msg_send![&*font, familyName];
        family.to_string()
    }
}

pub(super) fn initialize(settings: &Value, cx: &App) {
    FONT.with(|font| {
        font.get_or_init(|| {
            let configured = settings["DialogFont"]["fontName"].as_str().unwrap_or_default();
            let family = cx.text_system().all_font_names().into_iter()
                .find(|name| name.eq_ignore_ascii_case(configured))
                .unwrap_or_else(system_font_family);
            let size = settings["DialogFont"]["size"].as_i64()
                .filter(|size| SIZES.contains(size)).unwrap_or(8) as f32;
            DialogFont { family, size }
        });
    });
}

pub(super) fn font_family() -> String {
    FONT.with(|font| font.get().map(|font| font.family.clone()).unwrap_or_else(system_font_family))
}

pub(super) fn base_size() -> f32 { 12. * scale() }

pub(super) fn line_height() -> Pixels { px(14.) }

pub(super) fn control_height() -> Pixels { px(24.) }

pub(super) fn scale() -> f32 {
    FONT.with(|font| font.get().map_or(1., |font| font.size / 8.))
}

pub(super) fn switcher_scale() -> f32 {
    FONT.with(|font| font.get().map_or(1., |font| (font.size + 2.) / 10.))
}

pub(super) fn px(value: f32) -> Pixels { gpui::px(value * scale()) }

pub(super) fn logical(value: Pixels) -> f32 { f32::from(value) / scale() }

pub(super) fn native_length(value: f64) -> f64 { value * f64::from(scale()) }

pub(super) fn native_font() -> Retained<AnyObject> {
    unsafe {
        let name = NSString::from_str(&font_family());
        let font: Option<Retained<AnyObject>> = msg_send![class!(NSFont), fontWithName: &*name, size: f64::from(base_size())];
        font.unwrap_or_else(|| msg_send![class!(NSFont), systemFontOfSize: f64::from(base_size())])
    }
}
