use super::*;
use super::dialog_font::px;
use objc2::{class, msg_send, rc::{Allocated, Retained}, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRange, NSRect, NSSize, NSString};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Topic { CommandLine, DebugInfo }

#[derive(Default)]
pub(super) struct State {
    pub(super) topic: Option<Topic>,
    field: Option<NativeHelpText>,
    text: String,
}

struct NativeHelpText {
    scroll: Retained<AnyObject>,
    text: Retained<AnyObject>,
}

impl NativeHelpText {
    fn new(value: &str) -> io::Result<Self> {
        unsafe {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(600., 360.));
            let scroll: Allocated<AnyObject> = msg_send![class!(NSScrollView), alloc];
            let scroll: Option<Retained<AnyObject>> = msg_send![scroll, initWithFrame: frame];
            let scroll = scroll.ok_or_else(|| io::Error::other("无法创建帮助滚动区域"))?;
            let text: Allocated<AnyObject> = msg_send![class!(NSTextView), alloc];
            let text: Option<Retained<AnyObject>> = msg_send![text, initWithFrame: frame];
            let text = text.ok_or_else(|| io::Error::other("无法创建帮助文本区域"))?;
            let _: () = msg_send![&*text, setRichText: false];
            let _: () = msg_send![&*text, setEditable: false];
            let _: () = msg_send![&*text, setSelectable: true];
            let _: () = msg_send![&*text, setHorizontallyResizable: false];
            let _: () = msg_send![&*text, setVerticallyResizable: true];
            let _: () = msg_send![&*text, setAutoresizingMask: 2usize];
            let _: () = msg_send![&*text, setMinSize: NSSize::new(0., 0.)];
            let _: () = msg_send![&*text, setMaxSize: NSSize::new(f64::MAX, f64::MAX)];
            let container: *mut AnyObject = msg_send![&*text, textContainer];
            let _: () = msg_send![container, setWidthTracksTextView: true];
            let _: () = msg_send![container, setContainerSize: NSSize::new(600., f64::MAX)];
            let font = dialog_font::native_font();
            let _: () = msg_send![&*text, setFont: &*font];
            let _: () = msg_send![&*text, setString: &*NSString::from_str(value)];
            let _: () = msg_send![&*text, setSelectedRange: NSRange::new(0, value.encode_utf16().count())];
            let _: () = msg_send![&*scroll, setHasVerticalScroller: true];
            let _: () = msg_send![&*scroll, setAutohidesScrollers: true];
            let _: () = msg_send![&*scroll, setDocumentView: &*text];
            Ok(Self { scroll, text })
        }
    }

    fn element(&self) -> AnyElement {
        let scroll = self.scroll.clone();
        let text = self.text.clone();
        gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            if let Ok(handle) = HasWindowHandle::window_handle(window)
                && let RawWindowHandle::AppKit(handle) = handle.as_raw()
            {
                let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                let top = f64::from(f32::from(bounds.top()));
                let height = f64::from(f32::from(bounds.size.height));
                let width = f64::from(f32::from(bounds.size.width));
                let y = if view.isFlipped() { top } else { view.bounds().size.height - top - height };
                let frame = NSRect::new(NSPoint::new(f64::from(f32::from(bounds.left())), y), NSSize::new(width, height));
                unsafe {
                    let _: () = msg_send![&*scroll, setFrame: frame];
                    let _: () = msg_send![&*text, setFrameSize: NSSize::new(width, height)];
                    let parent: *mut AnyObject = msg_send![&*scroll, superview];
                    if parent != (view as *const NSView).cast_mut().cast::<AnyObject>() {
                        let _: () = msg_send![view, addSubview: &*scroll];
                    }
                }
            }
        }).size_full().into_any_element()
    }
}

impl Drop for NativeHelpText {
    fn drop(&mut self) {
        unsafe { let _: () = msg_send![&*self.scroll, removeFromSuperview]; }
    }
}

const COMMAND_LINE: &str = "Notepad Matrix 命令行参数\n\nnotepad-matrix [参数] [--] [文件 ...]\n\n-multiInst    启动独立实例。\n-nosession    本次启动不恢复或保存自动会话。\n-openSession  将唯一文件参数作为会话文件打开。\n-l语言        指定语言名称；例如 -lrust。\n-udl=名称     指定已安装的自定义语言。\n-n行号        定位到行（从 1 开始）。\n-c列号        定位到列（从 1 开始）。\n--            后续参数全部作为文件路径。\n\n文件路径含空格时请使用 shell 引号。\n本迁移版不支持 Windows 默认 ACP、DLL 插件或原版全部 Windows 参数。\n";

impl MatrixWindow {
    pub(super) fn execute_help_source(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let url = match id {
            47002 => Some("https://github.com/skyfiredao/notepad-matrix"),
            47010 | 47012 => None,
            _ => return false,
        };
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if let Some(url) = url { cx.open_url(url); return true; }
        let topic = if id == 47010 { Topic::CommandLine } else { Topic::DebugInfo };
        let text = match topic {
            Topic::CommandLine => COMMAND_LINE.to_owned(),
            Topic::DebugInfo => self.debug_information(),
        };
        match NativeHelpText::new(&text) {
            Ok(field) => {
                self.help_state = State { topic: Some(topic), field: Some(field), text };
                self.present_floating(floating_windows::DialogKind::Help, window, cx);
                cx.notify();
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
        true
    }

    fn debug_information(&self) -> String {
        let os = unsafe {
            let process: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
            let version: Retained<NSString> = msg_send![process, operatingSystemVersionString];
            version.to_string()
        };
        let executable = std::env::current_exe().map(|path| path.display().to_string()).unwrap_or_else(|error| error.to_string());
        let config = self.settings_store.as_ref().map(|store| store.root().display().to_string()).unwrap_or_else(|| "未配置".into());
        let arguments = std::env::args_os().skip(1).map(|arg| format!("{:?}", arg)).collect::<Vec<_>>().join(" ");
        let modules = [(self.converter_enabled, "Converter"), (self.mime_enabled, "MIME Tools"), (self.html_export_enabled, "NppExport")]
            .into_iter().filter_map(|(enabled, name)| enabled.then_some(name)).collect::<Vec<_>>().join(", ");
        format!("Notepad Matrix v{}\n架构: {}\n系统: {}\n构建: {}\n可执行文件: {}\n配置目录: {}\n命令行: {}\n渲染: GPUI / macOS\n固定模块: {}\nDLL 插件: 不支持\n", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH, os, if cfg!(debug_assertions) { "Debug" } else { "Release" }, executable, config, arguments, modules)
    }

    pub(super) fn help_title(&self) -> &'static str {
        match self.help_state.topic { Some(Topic::CommandLine) => "命令行参数", _ => "调试信息" }
    }

    pub(super) fn dismiss_help(&mut self, cx: &mut Context<Self>) {
        self.help_state = State::default();
        cx.notify();
    }

    pub(super) fn render_help(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let field = self.help_state.field.as_ref()?;
        Some(div().id("help-dialog").debug_selector(|| "help-dialog".into()).role(Role::Dialog)
            .aria_label(self.help_title()).size_full().p(px(10.)).flex().flex_col().gap(px(10.))
            .child(div().flex_1().min_h_0().border_1().border_color(rgb(0x999999)).child(field.element()))
            .child(div().flex().justify_end().gap(px(8.))
                .child(div().id("help-copy").role(Role::Button).aria_label("复制到剪贴板")
                    .px(px(12.)).py(px(4.)).border_1().border_color(rgb(0xaaaaaa)).cursor_pointer()
                    .on_click(cx.listener(|view, _, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(view.help_state.text.clone()))))
                    .child("复制到剪贴板"))
                .child(div().id("help-close").role(Role::Button).aria_label("确定").track_focus(&self.close_focus)
                    .px(px(20.)).py(px(4.)).border_1().border_color(rgb(0xaaaaaa)).cursor_pointer()
                    .on_click(cx.listener(|view, _, window, cx| view.dismiss_floating(floating_windows::DialogKind::Help, window, cx)))
                    .child("确定")))
            .into_any_element())
    }
}
