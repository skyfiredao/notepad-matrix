use gpui::prelude::*;
use gpui::{Div, Pixels, SharedString, Size, Stateful, Window, div, point, px, rgb, size};
use objc2::{MainThreadOnly, rc::Retained};
use objc2_app_kit::{NSView, NSWindow, NSWindowButton, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::io;

pub(super) const HEIGHT: f32 = 32.;
pub(super) const LEFT_INSET: f32 = 80.;
pub(super) const BACKGROUND: u32 = 0x357ec7;
pub(super) const FOREGROUND: u32 = 0xffffff;

pub(super) fn update_title(window: &mut Window, title: &str) {
    if window.window_title() != title {
        window.set_window_title(title);
    }
}

pub(super) fn frame() -> Div {
    div().h(px(HEIGHT)).pl(px(LEFT_INSET)).flex_shrink_0().flex().items_center()
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

pub(super) fn update_native_buttons(window: &Window, visible: bool) -> io::Result<()> {
    let host = native_window(window)?;
    let buttons = [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton]
        .map(|kind| host.standardWindowButton(kind));
    if visible
        && !host.styleMask().contains(NSWindowStyleMask::FullScreen)
        && let [Some(close), Some(minimize), Some(zoom)] = &buttons
        && let Some(container) = unsafe { close.superview().and_then(|view| view.superview()) }
    {
        let close_frame = close.frame();
        let minimize_frame = minimize.frame();
        let zoom_frame = zoom.frame();
        let position = point(px(12.), px((HEIGHT - close_frame.size.height as f32) / 2.));
        let x = f64::from(f32::from(position.x));
        let y = f64::from(f32::from(position.y));
        let spacing = minimize_frame.origin.x - close_frame.origin.x;
        let moved = [close_frame, minimize_frame, zoom_frame].into_iter()
            .zip([(x, y), (x + spacing, y), (x + spacing * 2., y)])
            .any(|(frame, (x, y))| (frame.origin.x - x).abs() > 0.01 || (frame.origin.y - y).abs() > 0.01);
        let container_frame = container.frame();
        if moved
            || (container_frame.size.height - f64::from(HEIGHT)).abs() > 0.01
            || (container_frame.origin.y - (host.frame().size.height - f64::from(HEIGHT))).abs() > 0.01
        {
            window.set_traffic_light_position(position);
        }
    }
    let hidden = !visible;
    for button in buttons.into_iter().flatten() {
        if button.isHidden() != hidden { button.setHidden(hidden); }
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
