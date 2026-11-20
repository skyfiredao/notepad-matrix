use gpui::prelude::*;
use gpui::{Div, Pixels, SharedString, Size, Stateful, Window, div, px, rgb, size};
use objc2::{MainThreadOnly, rc::Retained};
use objc2_app_kit::{NSView, NSWindow, NSWindowButton, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::io;

pub(super) const HEIGHT: f32 = 32.;
pub(super) const BACKGROUND: u32 = 0x357ec7;
pub(super) const FOREGROUND: u32 = 0xffffff;

pub(super) fn frame() -> Div {
    div().h(px(HEIGHT)).flex_shrink_0().flex().items_center()
        .bg(rgb(BACKGROUND)).text_color(rgb(FOREGROUND))
        .font_family("Helvetica").text_size(px(13.)).line_height(px(HEIGHT))
}

pub(super) fn title(title: impl Into<SharedString>) -> Div {
    div().flex_1().min_w(px(0.)).h_full().flex().items_center().overflow_hidden()
        .child(div().w_full().min_w(px(0.)).px(px(8.)).text_ellipsis().child(title.into()))
}

pub(super) fn close_button(id: &'static str, icon_id: &'static str, label: &'static str) -> Stateful<Div> {
    div().id(id).role(gpui::accesskit::Role::Button).aria_label(label)
        .w(px(45.)).h(px(30.)).flex_shrink_0().flex().items_center().justify_center()
        .hover(|style| style.bg(rgb(0xe81123)))
        .child(super::caption_icons::render(super::caption_icons::Icon::Close, icon_id, FOREGROUND))
}

fn native_window(window: &Window) -> io::Result<Retained<NSWindow>> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| io::Error::other(error.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "应用标题栏需要 macOS 窗口"));
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window().ok_or_else(|| io::Error::other("应用标题栏未找到原生窗口"))
}

pub(super) fn hide_native_buttons(window: &Window) -> io::Result<()> {
    let host = native_window(window)?;
    for kind in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        if let Some(button) = host.standardWindowButton(kind) { button.setHidden(true); }
    }
    Ok(())
}

pub(super) fn content_size_for_frame(window: &Window, frame: Size<Pixels>, resizable: bool, minimizable: bool) -> io::Result<Size<Pixels>> {
    let host = native_window(window)?;
    let mut style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::FullSizeContentView;
    if resizable { style |= NSWindowStyleMask::Resizable; }
    if minimizable { style |= NSWindowStyleMask::Miniaturizable; }
    let content = NSWindow::contentRectForFrameRect_styleMask(
        NSRect::new(NSPoint::new(0., 0.), NSSize::new(f64::from(f32::from(frame.width)), f64::from(f32::from(frame.height)))),
        style, host.mtm(),
    );
    Ok(size(px(content.size.width as f32), px(content.size.height as f32)))
}
