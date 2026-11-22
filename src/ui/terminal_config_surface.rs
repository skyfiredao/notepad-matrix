use super::*;
use super::dialog_font::px;
use crate::ssh_terminal::{self, Authentication, Mode, PasteButton, Profile, Settings};
use objc2::{class, msg_send};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::cell::Cell;
use std::ptr::NonNull;
use block2::RcBlock;

pub(super) struct State {
    pub(super) visible: bool,
    pub(super) password_visible: bool,
    settings: Settings,
    saved: Settings,
    original: Settings,
    selected: u64,
    edit: Option<Profile>,
    adding: bool,
    confirm_delete: bool,
    fields: [gpui::Entity<text_field::TextField>; 5],
    field_subscriptions: Vec<gpui::Subscription>,
    grid_focus: FocusHandle,
    scroll: gpui::ScrollHandle,
    scroll_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    scroll_grab: Cell<Option<f32>>,
    message: String,
    public_key: String,
    password: Option<PasswordField>,
    password_profile: Option<Profile>,
    pub(super) focus: FocusHandle,
    options_focus: [FocusHandle; 2],
    password_focus: [FocusHandle; 3],
    paste_focus: [FocusHandle; 2],
    appearance_focus: [FocusHandle; 2],
    color_target: Option<u64>,
    color_task: Option<gpui::Task<()>>,
    transparency_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    transparency_dragging: bool,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            password_visible: false,
            settings: Settings::default(),
            saved: Settings::default(),
            original: Settings::default(),
            selected: 0,
            edit: None,
            adding: false,
            confirm_delete: false,
            fields: [("Name", "terminal-profile-name"), ("Address", "terminal-profile-host"),
                ("Port", "terminal-profile-port"), ("User", "terminal-profile-user"),
                ("Font size", "terminal-profile-font")].map(|(label, id)| {
                    cx.new(|cx| text_field::TextField::compact(label, id, cx).without_history_arrow().with_compact_height(23.))
                }),
            field_subscriptions: Vec::new(),
            grid_focus: cx.focus_handle().tab_stop(true),
            scroll: gpui::ScrollHandle::new(),
            scroll_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            scroll_grab: Cell::new(None),
            message: String::new(),
            public_key: String::new(),
            password: None,
            password_profile: None,
            focus: cx.focus_handle(),
            options_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            password_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            paste_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            appearance_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            color_target: None,
            color_task: None,
            transparency_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            transparency_dragging: false,
        }
    }

    pub(super) fn dismiss_password(&mut self) {
        self.password_visible = false;
        self.password = None;
        self.password_profile = None;
    }

    pub(super) fn cancel_password_for(&mut self, id: u64) {
        if self.password_profile.as_ref().is_some_and(|profile| profile.id == id) { self.dismiss_password(); }
    }

    fn dirty(&self, cx: &App) -> bool {
        if self.settings != self.saved { return true; }
        let Some(edit) = &self.edit else { return false; };
        let Some(profile) = self.settings.profile(edit.id) else { return true; };
        edit != profile || self.fields.iter().zip([
            profile.name.clone(), profile.host.clone(), profile.port.to_string(),
            profile.username.clone(), profile.font_size.to_string(),
        ]).any(|(field, text)| field.read(cx).text() != text)
    }
}

pub(super) fn dimensions(state: &State) -> gpui::Size<gpui::Pixels> {
    let height = 270. + if state.edit.is_some() { 140. } else { 0. }
        + if state.confirm_delete { 80. } else { 0. }
        + if state.message.is_empty() { 0. } else { 30. };
    size(px(780.), px(height))
}

struct PasswordField {
    native: Retained<AnyObject>,
    focus_pending: Rc<Cell<bool>>,
    marker: Rc<()>,
    monitor: Option<PasswordMonitor>,
    focus_subscription: Option<gpui::Subscription>,
}

#[derive(Clone, Copy)]
enum PasswordAction { Submit, Cancel, Next, Previous, Focus }

struct PasswordMonitor {
    token: Retained<AnyObject>,
    _handler: RcBlock<dyn Fn(NonNull<AnyObject>) -> *mut AnyObject>,
    _main_thread: MainThreadMarker,
}

impl Drop for PasswordMonitor {
    fn drop(&mut self) {
        unsafe { let _: () = msg_send![class!(NSEvent), removeMonitor: &*self.token]; }
    }
}

fn finish_password_editing(native: &AnyObject) -> io::Result<()> {
    unsafe {
        let host: Option<Retained<AnyObject>> = msg_send![native, window];
        let parent: Option<Retained<AnyObject>> = msg_send![native, superview];
        if let (Some(host), Some(parent)) = (host, parent) {
            let _: () = msg_send![&*host, endEditingFor: std::ptr::null::<AnyObject>()];
            let focused: Bool = msg_send![&*host, makeFirstResponder: &*parent];
            if !focused.as_bool() { return Err(io::Error::other("无法恢复密码面板焦点")); }
        }
        Ok(())
    }
}

impl PasswordField {
    fn new() -> io::Result<Self> {
        unsafe {
            let field: Allocated<AnyObject> = msg_send![class!(NSSecureTextField), alloc];
            let field: Option<Retained<AnyObject>> = msg_send![field, initWithFrame: NSRect::new(NSPoint::new(0., 0.), NSSize::new(280., 24.))];
            let field = field.ok_or_else(|| io::Error::other("无法创建密码输入框"))?;
            let _: () = msg_send![&*field, setEditable: true];
            let _: () = msg_send![&*field, setSelectable: true];
            let _: () = msg_send![&*field, setFont: &*dialog_font::native_font()];
            let _: () = msg_send![&*field, setAccessibilityLabel: &*NSString::from_str("SSH password")];
            Ok(Self { native: field, focus_pending: Rc::new(Cell::new(true)), marker: Rc::new(()), monitor: None, focus_subscription: None })
        }
    }

    fn install(&mut self, window: &mut Window, cx: &mut Context<MatrixWindow>, focus: &FocusHandle) -> io::Result<()> {
        if self.monitor.is_some() { return Ok(()); }
        let main_thread = MainThreadMarker::new().ok_or_else(|| io::Error::other("密码输入需要 AppKit 主线程"))?;
        let native = self.native.clone();
        let marker = Rc::downgrade(&self.marker);
        let owner = cx.weak_entity();
        let target = Window::window_handle(window);
        let app = Rc::new(RefCell::new(cx.to_async()));
        let pending = self.focus_pending.clone();
        let handler = RcBlock::new(move |event: NonNull<AnyObject>| -> *mut AnyObject {
            let original = event.as_ptr();
            if marker.upgrade().is_none() { return original; }
            let action = unsafe {
                let host: *mut AnyObject = msg_send![&*native, window];
                let event_host: *mut AnyObject = msg_send![event.as_ref(), window];
                if host.is_null() || host != event_host { return original; }
                let editor: *mut AnyObject = msg_send![&*native, currentEditor];
                let responder: *mut AnyObject = msg_send![host, firstResponder];
                let editing = responder == (&*native as *const AnyObject).cast_mut() || !editor.is_null() && responder == editor;
                let kind: usize = msg_send![event.as_ref(), type];
                if kind == 1 {
                    let parent: Option<Retained<AnyObject>> = msg_send![&*native, superview];
                    let Some(parent) = parent else { return original; };
                    let point: NSPoint = msg_send![event.as_ref(), locationInWindow];
                    let point: NSPoint = msg_send![&*parent, convertPoint: point, fromView: std::ptr::null::<AnyObject>()];
                    let frame: NSRect = msg_send![&*native, frame];
                    let inside = point.x >= frame.origin.x && point.x < frame.origin.x + frame.size.width
                        && point.y >= frame.origin.y && point.y < frame.origin.y + frame.size.height;
                    if inside { PasswordAction::Focus } else {
                        if editing && let Err(error) = finish_password_editing(&native) { eprintln!("密码面板焦点：{error}"); }
                        return original;
                    }
                } else {
                    if !editing { return original; }
                    let flags: usize = msg_send![event.as_ref(), modifierFlags];
                    if flags & ((1 << 18) | (1 << 19) | (1 << 20)) != 0 { return original; }
                    let key: u16 = msg_send![event.as_ref(), keyCode];
                    match key {
                        36 | 76 => PasswordAction::Submit,
                        53 => PasswordAction::Cancel,
                        48 if flags & (1 << 17) != 0 => PasswordAction::Previous,
                        48 => PasswordAction::Next,
                        _ => return original,
                    }
                }
            };
            if !matches!(action, PasswordAction::Focus) {
                pending.set(false);
                if let Err(error) = finish_password_editing(&native) { eprintln!("密码面板焦点：{error}"); return original; }
            }
            let owner = owner.clone();
            let marker = marker.clone();
            let app = app.clone();
            let result = app.try_borrow_mut().map_err(|error| error.to_string()).and_then(|mut app| {
                target.update(&mut *app, |_, _, cx| {
                    cx.defer(move |cx| {
                        if marker.upgrade().is_none() || !cx.windows().contains(&target) { return; }
                        if let Err(error) = target.update(cx, |_, window, cx| {
                            owner.update(cx, |view, cx| {
                                if !view.terminal_config.password_visible { return; }
                                match action {
                                    PasswordAction::Submit => view.submit_terminal_password(window, cx),
                                    PasswordAction::Cancel => view.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx),
                                    PasswordAction::Next => view.terminal_config.password_focus[1].focus(window, cx),
                                    PasswordAction::Previous => view.terminal_config.password_focus[2].focus(window, cx),
                                    PasswordAction::Focus => view.terminal_config.password_focus[0].focus(window, cx),
                                }
                            })
                        }).and_then(|result| result) { eprintln!("密码面板操作：{error}"); }
                    });
                }).map_err(|error| error.to_string())
            });
            if let Err(error) = result { eprintln!("密码面板事件：{error}"); return original; }
            if matches!(action, PasswordAction::Focus) { original } else { std::ptr::null_mut() }
        });
        let token: Option<Retained<AnyObject>> = unsafe { msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: (1usize << 10) | (1usize << 1), handler: &*handler] };
        let token = token.ok_or_else(|| io::Error::other("无法注册密码输入事件"))?;
        self.monitor = Some(PasswordMonitor { token, _handler: handler, _main_thread: main_thread });
        let pending = self.focus_pending.clone();
        self.focus_subscription = Some(cx.on_focus(focus, window, move |_, _, cx| { pending.set(true); cx.notify(); }));
        Ok(())
    }

    fn take(&self) -> io::Result<String> {
        finish_password_editing(&self.native)?;
        unsafe {
            let value: *const NSString = msg_send![&*self.native, stringValue];
            let password = value.as_ref().ok_or_else(|| io::Error::other("无法读取密码输入框"))?.to_string();
            let _: () = msg_send![&*self.native, setStringValue: &*NSString::from_str("")];
            Ok(password)
        }
    }

    fn element(&self, palette: text_field::RenderPalette) -> AnyElement {
        unsafe {
            let color = |value: u32| -> Retained<AnyObject> {
                msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64]
            };
            let _: () = msg_send![&*self.native, setTextColor: &*color(palette.text)];
            let _: () = msg_send![&*self.native, setBackgroundColor: &*color(palette.softer_background)];
        }
        let native = self.native.clone();
        let focus_pending = self.focus_pending.clone();
        gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            if let Ok(handle) = HasWindowHandle::window_handle(window)
                && let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                let top = f64::from(f32::from(bounds.top()));
                let height = f64::from(f32::from(bounds.size.height));
                let y = if view.isFlipped() { top } else { view.bounds().size.height - top - height };
                let frame = NSRect::new(NSPoint::new(f64::from(f32::from(bounds.left())), y), NSSize::new(f64::from(f32::from(bounds.size.width)), height));
                unsafe {
                    let _: () = msg_send![&*native, setFrame: frame];
                    let parent: *mut AnyObject = msg_send![&*native, superview];
                    if parent != (view as *const NSView).cast_mut().cast::<AnyObject>() {
                        let _: () = msg_send![view, addSubview: &*native];
                    }
                    if focus_pending.get() {
                        let host: *mut AnyObject = msg_send![&*native, window];
                        if !host.is_null() {
                            let focused: Bool = msg_send![host, makeFirstResponder: &*native];
                            if focused.as_bool() { focus_pending.set(false); }
                        }
                    }
                }
            }
        }).size_full().into_any_element()
    }
}

impl Drop for PasswordField {
    fn drop(&mut self) {
        self.monitor = None;
        self.focus_subscription = None;
        if let Err(error) = finish_password_editing(&self.native) { eprintln!("密码面板焦点：{error}"); }
        unsafe {
            let _: () = msg_send![&*self.native, setStringValue: &*NSString::from_str("")];
            let _: () = msg_send![&*self.native, removeFromSuperview];
        }
    }
}

fn button(id: &'static str, label: &'static str, palette: text_field::RenderPalette, enabled: bool) -> gpui::Stateful<gpui::Div> {
    div().id(id).role(Role::Button).aria_label(label).focusable().tab_stop(enabled)
        .h(px(25.)).min_w(px(90.)).px(px(12.)).flex().items_center().justify_center()
        .border_1().border_color(rgb(palette.edge)).bg(rgb(palette.button_background(enabled)))
        .text_color(rgb(palette.control_text(enabled))).cursor_pointer()
        .hover(move |style| {
            if enabled { style.border_color(rgb(palette.hot_edge)).bg(rgb(palette.hot_background)) }
            else { style.border_color(rgb(palette.edge)).bg(rgb(palette.button_background(false))) }
        })
        .child(label)
}

impl MatrixWindow {
    pub(super) fn terminal_label(&self, chinese: &'static str, english: &'static str) -> &'static str {
        if self.native_locale() == "chineseSimplified.xml" { chinese } else { english }
    }

    fn terminal_settings(&self) -> io::Result<Settings> {
        let store = self.settings_store.as_ref().ok_or_else(|| io::Error::other("终端配置目录不可用"))?;
        Settings::load(store)
    }

    pub(super) fn open_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_system_terminal(window, cx);
    }

    pub(super) fn open_remote_terminal(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.open_terminal_profile_id(id, window, cx);
    }

    fn terminal_prompt_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.terminal_config.password_visible {
            self.present_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
            return true;
        }
        if self.ssh_terminal.fingerprint.is_some() {
            self.show_terminal_host_key(window, cx);
            return true;
        }
        false
    }

    fn open_terminal_profile_id(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.open_terminal_profile_id(id, window, cx)) { return; }
        if self.batch_editing_blocked() || self.file_busy || self.close_dialog || self.save_all_dialog { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if self.terminal_prompt_pending(window, cx) { return; }
        let settings = match self.terminal_settings() {
            Ok(settings) => settings,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        self.ssh_terminal.paste_button = settings.paste_button;
        let Some(profile) = settings.servers.iter().find(|profile| profile.id == id).cloned() else {
            self.notify_editor_error(self.terminal_label("终端配置已删除。", "This terminal profile was deleted.").to_owned(), cx);
            return;
        };
        self.open_terminal_profile(profile, window, cx);
    }

    fn open_terminal_profile(&mut self, profile: Profile, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_ssh_profile(&profile, window, cx);
        if self.ssh_terminal.session.is_some() {
            self.ssh_terminal.visible = true;
            self.ssh_terminal.focus.focus(window, cx);
            cx.notify();
            return;
        }
        match profile.authentication {
            Authentication::Password => {
                let field = match PasswordField::new() {
                    Ok(field) => field,
                    Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
                };
                self.terminal_config.password = Some(field);
                self.terminal_config.password_profile = Some(profile);
                self.terminal_config.password_visible = true;
                self.present_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
                cx.notify();
            }
            Authentication::Key => self.connect_terminal(profile, None, window, cx),
        }
    }

    pub(super) fn reconnect_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.reconnect_terminal(window, cx)) { return; }
        if self.batch_editing_blocked() || self.file_busy || self.close_dialog || self.save_all_dialog { return; }
        if self.terminal_prompt_pending(window, cx) { return; }
        let id = self.ssh_terminal.profile_id;
        let profile = match self.terminal_settings().and_then(|settings| settings.servers.iter().find(|profile| profile.id == id).cloned()
            .ok_or_else(|| io::Error::other("终端配置已删除 / Terminal profile was deleted"))) {
            Ok(profile) => profile,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        self.ssh_terminal.close();
        self.open_terminal_profile(profile, window, cx);
    }

    fn connect_terminal(&mut self, profile: Profile, password: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if Window::window_handle(window) != self.document_window {
            self.route_to_document(window, cx, move |view, window, cx| view.connect_terminal(profile, password, window, cx));
            return;
        }
        if let Err(error) = self.terminal_settings().and_then(|settings| {
            settings.servers.iter().find(|saved| saved.id == profile.id).map(|_| ()).ok_or_else(|| io::Error::other(
                self.terminal_label("终端配置已删除，连接已取消。", "This terminal profile was deleted. Connection cancelled.")))
        }) {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        let endpoint = format!("{}@{}:{}", profile.username, profile.host, profile.port);
        match ssh_terminal::connect(profile.clone(), password, 80, 24) {
            Ok(session) => self.start_ssh_terminal(profile, session, window, cx),
            Err(error) => self.notify_editor_error(format!("SSH {endpoint}：{error}"), cx),
        }
    }

    fn submit_terminal_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self.terminal_config.password_profile.clone() else { return; };
        let password = match self.terminal_config.password.as_ref().ok_or_else(|| io::Error::other("密码输入框已关闭")).and_then(PasswordField::take) {
            Ok(password) => password,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        self.terminal_config.dismiss_password();
        self.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
        self.connect_terminal(profile, Some(password), window, cx);
    }

    pub(super) fn show_terminal_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.show_terminal_config(window, cx)) { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if self.terminal_config.visible {
            self.present_floating(floating_windows::DialogKind::TerminalConfig, window, cx);
            return;
        }
        match self.terminal_settings() {
            Ok(settings) => {
                self.terminal_config.original = settings.clone();
                self.terminal_config.saved = settings.clone();
                self.terminal_config.settings = settings;
            }
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        }
        self.terminal_config.public_key = match ssh_terminal::public_key() {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        self.terminal_config.message.clear();
        self.terminal_config.selected = self.terminal_config.settings.servers.first().map_or(0, |profile| profile.id);
        self.terminal_config.edit = None;
        self.terminal_config.adding = false;
        self.terminal_config.confirm_delete = false;
        self.terminal_config.scroll.set_offset(gpui::point(px(0.), px(0.)));
        if self.terminal_config.field_subscriptions.is_empty() {
            for field in self.terminal_config.fields.clone() {
                self.terminal_config.field_subscriptions.push(cx.observe(&field, |_, _, cx| cx.notify()));
            }
        }
        self.terminal_config.visible = true;
        self.present_floating(floating_windows::DialogKind::TerminalConfig, window, cx);
        cx.notify();
    }

    fn save_terminal_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_config.confirm_delete { return; }
        if self.terminal_config.edit.is_some() && !self.apply_terminal_profile(window, cx) { return; }
        let result = self.settings_store.as_ref().ok_or_else(|| io::Error::other("终端配置目录不可用"))
            .and_then(|store| self.terminal_config.settings.save(store, &self.terminal_config.original));
        match result {
            Ok(()) => {
                if self.terminal_config.password_profile.as_ref().is_some_and(|profile| {
                    self.terminal_config.settings.profile(profile.id).is_none()
                }) {
                    self.terminal_config.dismiss_password();
                    self.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
                }
                if self.ssh_terminal.fingerprint.is_some()
                    && self.terminal_config.settings.profile(self.ssh_terminal.profile_id).is_none() {
                    self.cancel_terminal_host_key(cx);
                    self.dismiss_floating(floating_windows::DialogKind::TerminalHostKey, window, cx);
                }
                self.ssh_terminal.apply_settings(&self.terminal_config.settings);
                self.ssh_terminal.retain_profiles(&self.terminal_config.settings);
                if let Some(pending) = &mut self.terminal_config.password_profile
                    && let Some(profile) = self.terminal_config.settings.profile(pending.id) {
                    pending.background_color = profile.background_color;
                    pending.transparency = profile.transparency;
                }
                self.terminal_config.saved = self.terminal_config.settings.clone();
                self.terminal_config.original = self.terminal_config.settings.clone();
                self.refresh_terminal_menu(cx);
                self.dismiss_floating(floating_windows::DialogKind::TerminalConfig, window, cx);
                cx.notify();
            }
            Err(error) => { self.terminal_config.message = error.to_string(); cx.notify(); }
        }
    }

    fn generate_terminal_key(&mut self, cx: &mut Context<Self>) {
        if !self.terminal_config.public_key.is_empty() { return; }
        match ssh_terminal::generate_key() {
            Ok(key) => {
                self.terminal_config.public_key = key;
                self.terminal_config.message = self.terminal_label("密钥已生成。在目标账户中手工执行公钥登记命令。", "Key generated. Run the registration command manually in the target account.").to_owned();
            }
            Err(error) => self.terminal_config.message = error.to_string(),
        }
        cx.notify();
    }

    fn begin_terminal_profile_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_config.edit.is_some() || self.terminal_config.confirm_delete { return; }
        let Some(profile) = self.terminal_config.settings.servers.iter().find(|profile| profile.id == self.terminal_config.selected).cloned() else { return; };
        let texts = [profile.name.clone(), profile.host.clone(), profile.port.to_string(),
            profile.username.clone(), profile.font_size.to_string()];
        let labels = [self.terminal_label("名称", "Name"), self.terminal_label("地址", "Address"),
            self.terminal_label("端口", "Port"), self.terminal_label("用户名", "User"),
            self.terminal_label("字号", "Font size")];
        for (index, field) in self.terminal_config.fields.iter().enumerate() {
            field.update(cx, |field, cx| {
                field.set_label(labels[index], cx);
                field.set_text(&texts[index], window, cx);
                field.set_read_only(false, cx);
                field.set_enabled(true, cx);
            });
        }
        self.terminal_config.edit = Some(profile);
        self.terminal_config.transparency_dragging = false;
        self.terminal_config.message.clear();
        self.terminal_config.fields[0].update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }

    fn apply_terminal_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.finish_terminal_color(cx);
        self.terminal_config.transparency_dragging = false;
        let Some(mut profile) = self.terminal_config.edit.clone() else { return true; };
        let texts = self.terminal_config.fields.each_ref().map(|field| field.read(cx).text());
        let validation = (|| -> io::Result<()> {
            let port = texts[2].trim().parse::<u16>().ok().filter(|port| *port > 0)
                .ok_or_else(|| io::Error::other(self.terminal_label("端口必须为 1–65535。", "Port must be 1–65535.")))?;
            let font_size = texts[4].trim().parse::<u16>().ok().filter(|font| (8..=36).contains(font))
                .ok_or_else(|| io::Error::other(self.terminal_label("字号必须为 8–36。", "Font size must be 8–36.")))?;
            if texts[0].trim().is_empty() {
                return Err(io::Error::other(self.terminal_label("请输入名称。", "Enter a profile name.")));
            }
            if texts[1].trim().is_empty() {
                return Err(io::Error::other(self.terminal_label("请输入地址。", "Enter a host address.")));
            }
            if texts[3].trim().is_empty() {
                return Err(io::Error::other(self.terminal_label("请输入用户名。", "Enter a username.")));
            }
            profile.name = texts[0].trim().to_owned();
            profile.host = texts[1].trim().to_owned();
            profile.port = port;
            profile.username = texts[3].trim().to_owned();
            profile.mode = Mode::Ssh;
            profile.font_size = font_size;
            Ok(())
        })();
        if let Err(error) = validation {
            self.terminal_config.message = error.to_string();
            cx.notify();
            return false;
        }
        if let Some(saved) = self.terminal_config.settings.servers.iter_mut().find(|saved| saved.id == profile.id) {
            *saved = profile;
        } else {
            self.terminal_config.message = self.terminal_label("终端配置已删除。", "This terminal profile was deleted.").to_owned();
            cx.notify();
            return false;
        }
        self.terminal_config.edit = None;
        self.terminal_config.adding = false;
        self.terminal_config.message.clear();
        self.terminal_config.grid_focus.focus(window, cx);
        cx.notify();
        true
    }

    fn cancel_terminal_profile_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_terminal_color(cx);
        self.terminal_config.transparency_dragging = false;
        if self.terminal_config.adding {
            let id = self.terminal_config.selected;
            self.terminal_config.settings.servers.retain(|profile| profile.id != id);
            self.terminal_config.selected = self.terminal_config.settings.servers.first().map_or(0, |profile| profile.id);
            self.terminal_config.scroll.set_offset(gpui::point(px(0.), px(0.)));
        }
        self.terminal_config.edit = None;
        self.terminal_config.adding = false;
        self.terminal_config.message.clear();
        self.terminal_config.grid_focus.focus(window, cx);
        cx.notify();
    }

    fn activate_terminal_config_button(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "terminal-profile-add" => {
                if self.terminal_config.edit.is_some() || self.terminal_config.confirm_delete { return; }
                match self.terminal_config.settings.new_profile_id() {
                    Ok(id) => {
                        let mut profile = Profile::remote(id);
                        profile.name = format!("{} {}", self.terminal_label("服务器", "Server"), self.terminal_config.settings.servers.len() + 1);
                        self.terminal_config.settings.servers.push(profile);
                        self.terminal_config.selected = id;
                        self.begin_terminal_profile_edit(window, cx);
                        self.terminal_config.adding = true;
                        self.terminal_config.scroll.scroll_to_item(self.terminal_config.settings.servers.len() - 1);
                    }
                    Err(error) => self.terminal_config.message = error.to_string(),
                }
            }
            "terminal-profile-edit" => self.begin_terminal_profile_edit(window, cx),
            "terminal-profile-delete" => {
                if self.terminal_config.selected == 0 || self.terminal_config.edit.is_some() { return; }
                self.terminal_config.confirm_delete = true;
            }
            "terminal-profile-delete-confirm" => {
                if !self.terminal_config.confirm_delete { return; }
                let id = self.terminal_config.selected;
                if id == 0 { return; }
                self.terminal_config.settings.servers.retain(|profile| profile.id != id);
                self.terminal_config.selected = self.terminal_config.settings.servers.first().map_or(0, |profile| profile.id);
                self.terminal_config.scroll.set_offset(gpui::point(px(0.), px(0.)));
                self.terminal_config.confirm_delete = false;
                self.terminal_config.grid_focus.focus(window, cx);
            }
            "terminal-profile-delete-cancel" => {
                self.terminal_config.confirm_delete = false;
                self.terminal_config.grid_focus.focus(window, cx);
            }
            "terminal-profile-apply" => { self.apply_terminal_profile(window, cx); }
            "terminal-profile-edit-cancel" => self.cancel_terminal_profile_edit(window, cx),
            "terminal-generate-key" => self.generate_terminal_key(cx),
            "terminal-copy-public-key" => {
                if self.terminal_config.public_key.is_empty() { return; }
                match ssh_terminal::registration_command() {
                    Ok(command) => {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(command));
                        self.terminal_config.message = self.terminal_label("已复制。在目标账户的终端粘贴执行。", "Copied. Paste and run in the target account's terminal.").to_owned();
                    }
                    Err(error) => self.terminal_config.message = error.to_string(),
                }
            }
            "terminal-config-save" => self.save_terminal_config(window, cx),
            "terminal-config-cancel" => {
                self.terminal_config.settings = self.terminal_config.saved.clone();
                self.terminal_config.edit = None;
                self.terminal_config.adding = false;
                self.terminal_config.confirm_delete = false;
                self.dismiss_floating(floating_windows::DialogKind::TerminalConfig, window, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    fn terminal_config_button(&self, id: &'static str, label: &'static str, enabled: bool, cx: &Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        button(id, label, palette, enabled)
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .on_click(cx.listener(move |view, _, window, cx| { if enabled { view.activate_terminal_config_button(id, window, cx); } }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if enabled && matches!(event.keystroke.key.as_str(), "space" | "enter") {
                    view.activate_terminal_config_button(id, window, cx);
                    cx.stop_propagation();
                }
            })).into_any_element()
    }

    pub(super) fn show_terminal_host_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ssh_terminal.fingerprint.is_some() {
            self.present_floating(floating_windows::DialogKind::TerminalHostKey, window, cx);
            cx.notify();
        }
    }

    fn confirm_terminal_host_key(&mut self, accept: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &self.ssh_terminal.session {
            if let Err(error) = session.trust_host(accept) {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        }
        self.ssh_terminal.fingerprint = None;
        self.dismiss_floating(floating_windows::DialogKind::TerminalHostKey, window, cx);
        cx.notify();
    }

    pub(super) fn cancel_terminal_host_key(&mut self, cx: &mut Context<Self>) {
        if self.ssh_terminal.fingerprint.take().is_some()
            && let Some(session) = &self.ssh_terminal.session {
            if let Err(error) = session.trust_host(false) { self.ssh_terminal.status = error.to_string(); }
        }
        cx.notify();
    }

    pub(super) fn restore_terminal_focus(&self, cx: &mut Context<Self>) {
        let root = self.document_window;
        let focus = self.ssh_terminal.focus.clone();
        cx.defer(move |cx| {
            if cx.windows().contains(&root)
                && let Err(error) = root.update(cx, |_, window, cx| {
                    window.activate_window();
                    focus.focus(window, cx);
                }) {
                eprintln!("无法恢复终端焦点：{error}");
            }
        });
    }

    fn terminal_dialog(&self, id: &'static str, title: &'static str, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let palette = self.preferences_render_palette();
        div().id(id).role(Role::Dialog).aria_label(title).key_context("MatrixTerminalDialog")
            .track_focus(&self.terminal_config.focus).tab_group()
            .size_full().relative().flex().flex_col().p(px(14.)).gap(px(12.))
            .bg(rgb(palette.pure_background)).text_color(rgb(palette.text))
            .font_family(dialog_font::font_family()).text_size(px(12.))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    match id {
                        "terminal-password-dialog" => view.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx),
                        "terminal-host-key-dialog" => view.dismiss_floating(floating_windows::DialogKind::TerminalHostKey, window, cx),
                        _ if view.terminal_config.confirm_delete => {
                            view.terminal_config.confirm_delete = false;
                            view.terminal_config.grid_focus.focus(window, cx);
                            cx.notify();
                        }
                        _ if view.terminal_config.edit.is_some() => view.cancel_terminal_profile_edit(window, cx),
                        _ => view.activate_terminal_config_button("terminal-config-cancel", window, cx),
                    }
                    cx.stop_propagation();
                }
            }))
    }

    fn select_terminal_profile(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_config.edit.is_some() || self.terminal_config.confirm_delete { return; }
        let Some(index) = self.terminal_config.settings.servers.iter().position(|profile| profile.id == id) else { return; };
        self.terminal_config.selected = id;
        self.terminal_config.grid_focus.focus(window, cx);
        self.terminal_config.scroll.scroll_to_item(index);
        cx.notify();
    }

    fn set_terminal_profile_option(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_terminal_color(cx);
        self.terminal_config.transparency_dragging = false;
        let Some(profile) = &mut self.terminal_config.edit else { return; };
        match index {
            0 => profile.authentication = Authentication::Password,
            1 => profile.authentication = Authentication::Key,
            _ => return,
        }
        self.terminal_config.options_focus[index].focus(window, cx);
        cx.notify();
    }

    fn terminal_profile_radio(&self, index: usize, label: &'static str, checked: bool, enabled: bool, cx: &Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let focus = self.terminal_config.options_focus[index].clone().tab_stop(enabled);
        div().id(["terminal-auth-password", "terminal-auth-key"][index])
            .role(Role::RadioButton).aria_label(label).aria_toggled(checked.into()).track_focus(&focus)
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .h(px(23.)).flex().items_center().gap(px(5.)).text_color(rgb(palette.control_text(enabled)))
            .child(preferences_surface::pref_indicator_palette(checked, true, enabled, palette)).child(label)
            .on_click(cx.listener(move |view, _, window, cx| { if enabled { view.set_terminal_profile_option(index, window, cx); } }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if !enabled { return; }
                let target = match event.keystroke.key.as_str() {
                    "space" => index, "up" | "down" | "left" | "right" => index ^ 1, _ => return,
                };
                view.set_terminal_profile_option(target, window, cx);
                cx.stop_propagation();
            })).into_any_element()
    }

    fn sync_terminal_color(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.terminal_config.color_target else { return; };
        if !self.terminal_config.visible { return; }
        let Some(profile) = self.terminal_config.edit.as_mut().filter(|profile| profile.id == id && profile.mode == Mode::Ssh) else { return; };
        let Some(color) = style_config_surface::color_panel_rgb() else { return; };
        if profile.background_color != color {
            profile.background_color = color;
            cx.notify();
        }
    }

    pub(super) fn finish_terminal_color(&mut self, cx: &mut Context<Self>) {
        self.sync_terminal_color(cx);
        if self.terminal_config.color_target.take().is_some() {
            unsafe {
                let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                let _: () = msg_send![panel, orderOut: std::ptr::null::<AnyObject>()];
            }
        }
        self.terminal_config.color_task = None;
    }

    pub(super) fn hide_terminal_config(&mut self, cx: &mut Context<Self>) {
        self.finish_terminal_color(cx);
        self.terminal_config.transparency_dragging = false;
        self.terminal_config.visible = false;
    }

    fn choose_terminal_color(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal_config.visible || self.terminal_config.edit.as_ref().is_none_or(|profile| profile.mode != Mode::Ssh) { return; }
        self.finish_udl_styler_color(cx);
        if !self.finish_preference_color(window, cx) { return; }
        self.finish_style_color(window, cx);
        self.finish_terminal_color(cx);
        let Some(profile) = &self.terminal_config.edit else { return; };
        let id = profile.id;
        let value = profile.background_color;
        unsafe {
            let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
            let color: *mut AnyObject = msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64];
            let _: () = msg_send![panel, setShowsAlpha: false];
            let _: () = msg_send![panel, setColor: color];
            let _: () = msg_send![panel, orderFront: std::ptr::null::<AnyObject>()];
        }
        self.terminal_config.color_target = Some(id);
        let executor = cx.background_executor().clone();
        self.terminal_config.color_task = Some(cx.spawn(async move |owner, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(50)).await;
                let keep = owner.update(cx, |view, cx| {
                    if view.terminal_config.color_target != Some(id) { return false; }
                    let active = view.terminal_config.visible && view.terminal_config.edit.as_ref().is_some_and(|profile| profile.id == id && profile.mode == Mode::Ssh);
                    if !active { view.finish_terminal_color(cx); return false; }
                    let visible: Bool = unsafe {
                        let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                        msg_send![panel, isVisible]
                    };
                    view.sync_terminal_color(cx);
                    if !visible.as_bool() { view.terminal_config.color_target = None; }
                    visible.as_bool()
                });
                match keep { Ok(true) => {}, Ok(false) => break, Err(error) => { eprintln!("终端颜色板无法更新：{error}"); break; } }
            }
        }));
    }

    fn terminal_background_button(&self, profile: &Profile, cx: &Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let enabled = profile.mode == Mode::Ssh;
        let label = self.terminal_label("背景颜色", "Background");
        div().id("terminal-profile-background").role(Role::Button).aria_label(label)
            .track_focus(&self.terminal_config.appearance_focus[0].clone().tab_stop(enabled))
            .w(px(25.)).h(px(23.)).border_1().border_color(rgb(if palette.dark { palette.control_edge(enabled) } else { 0x000000 }))
            .bg(rgb(if enabled { profile.background_color } else { palette.control_background(false) }))
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .when(enabled, |button| button.cursor_pointer().focus(move |style| style.border_color(rgb(palette.hot_edge))))
            .on_click(cx.listener(move |view, _, window, cx| {
                if enabled { view.terminal_config.appearance_focus[0].focus(window, cx); view.choose_terminal_color(window, cx); }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if enabled && matches!(event.keystroke.key.as_str(), "space" | "enter") { view.choose_terminal_color(window, cx); cx.stop_propagation(); }
            })).into_any_element()
    }

    fn set_terminal_transparency(&mut self, value: u8, cx: &mut Context<Self>) {
        if !self.terminal_config.visible { return; }
        let Some(profile) = self.terminal_config.edit.as_mut().filter(|profile| profile.mode == Mode::Ssh) else { return; };
        let value = value.min(100);
        if profile.transparency != value { profile.transparency = value; cx.notify(); }
    }

    fn set_terminal_transparency_position(&mut self, position: gpui::Pixels, cx: &mut Context<Self>) {
        let bounds = self.terminal_config.transparency_bounds.get();
        let width = f32::from(bounds.size.width - px(10.));
        if width <= 0. { return; }
        let fraction = (f32::from(position - bounds.left() - px(5.)) / width).clamp(0., 1.);
        self.set_terminal_transparency((fraction * 100.).round() as u8, cx);
    }

    fn terminal_transparency_slider(&self, profile: &Profile, cx: &Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let enabled = profile.mode == Mode::Ssh;
        let value = profile.transparency;
        let fraction = f32::from(value) / 100.;
        let bounds = self.terminal_config.transparency_bounds.clone();
        let channel = if palette.dark { palette.control_background(enabled) } else { 0xe7e7e7 };
        let thumb = if palette.dark { if enabled { palette.softer_background } else { palette.disabled_edge } }
            else if enabled { 0x0078d7 } else { 0xcccccc };
        let mut slider = div().id("terminal-profile-transparency").role(Role::Slider).aria_label(self.terminal_label("透明度", "Transparency"))
            .track_focus(&self.terminal_config.appearance_focus[1].clone().tab_stop(enabled))
            .relative().w(px(113.)).h(px(23.)).flex_shrink_0()
            .a11y_synthetic_children(move |builder| {
                let node = builder.parent_node();
                node.set_min_numeric_value(0.); node.set_max_numeric_value(100.);
                node.set_numeric_value(f64::from(value)); node.set_numeric_value_step(1.);
                if !enabled { node.set_disabled(); }
            })
            .when(enabled, |slider| slider.focus(move |style| style.border_1().border_color(rgb(palette.hot_edge))))
            .child(gpui::canvas(move |position, _, _| bounds.set(position), |_, _, _, _| ()).absolute().size_full())
            .child(div().absolute().top(px(10.)).w_full().h(px(4.)).border_1().border_color(rgb(if palette.dark { palette.edge } else { 0xd5d5d5 })).bg(rgb(channel)))
            .child(div().absolute().top(px(1.)).left(px(fraction * 103.)).w(px(10.)).h(px(21.)).bg(rgb(thumb)))
            .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                if !enabled { return; }
                view.terminal_config.appearance_focus[1].focus(window, cx);
                view.terminal_config.transparency_dragging = true;
                view.set_terminal_transparency_position(event.position.x, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                if !enabled { return; }
                let Some(profile) = &view.terminal_config.edit else { return; };
                let value = match event.keystroke.key.as_str() {
                    "left" | "down" => profile.transparency.saturating_sub(1),
                    "right" | "up" => profile.transparency.saturating_add(1),
                    "pageup" => profile.transparency.saturating_add(10), "pagedown" => profile.transparency.saturating_sub(10),
                    "home" => 0, "end" => 100, _ => return,
                };
                view.set_terminal_transparency(value, cx); cx.stop_propagation();
            }));
        for action in [gpui::accesskit::Action::Increment, gpui::accesskit::Action::Decrement, gpui::accesskit::Action::SetValue] {
            let owner = cx.entity().downgrade();
            slider = slider.on_a11y_action(action, move |data, _, cx| {
                if !enabled { return; }
                let requested = match action {
                    gpui::accesskit::Action::SetValue => match data {
                        Some(gpui::accesskit::ActionData::NumericValue(value)) => Some(*value),
                        Some(gpui::accesskit::ActionData::Value(value)) => value.parse::<f64>().ok(), _ => return,
                    }, _ => None,
                };
                if action == gpui::accesskit::Action::SetValue && requested.is_none_or(|value| !value.is_finite()) { return; }
                if let Err(error) = owner.update(cx, |view, cx| {
                    let Some(profile) = &view.terminal_config.edit else { return; };
                    let value = requested.map(|value| value.round().clamp(0., 100.) as u8).unwrap_or_else(|| {
                        if action == gpui::accesskit::Action::Increment { profile.transparency.saturating_add(1) } else { profile.transparency.saturating_sub(1) }
                    });
                    view.set_terminal_transparency(value, cx);
                }) { eprintln!("终端透明度操作无法更新：{error}"); }
            });
        }
        slider.into_any_element()
    }

    fn terminal_profile_scrollbar(&self, cx: &Context<Self>) -> AnyElement {
        let scroll = self.terminal_config.scroll.clone();
        let paint_scroll = scroll.clone();
        let bounds = self.terminal_config.scroll_bounds.clone();
        let paint_bounds = bounds.clone();
        let dark = self.dark;
        preferences_editing_surface::scrollbar_accessibility(
            div().id("terminal-profile-scrollbar").role(Role::ScrollBar)
                .aria_orientation(gpui::accesskit::Orientation::Vertical).tab_index(-1)
                .absolute().right(px(0.)).top(px(26.)).bottom(px(0.)).w(px(17.))
                .bg(rgb(if dark { 0x282828 } else { 0xf0f0f0 }))
                .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                    let bounds = bounds.get();
                    let length = dialog_font::logical(bounds.size.height);
                    let position = dialog_font::logical(event.position.y - bounds.top());
                    let viewport = dialog_font::logical(scroll.bounds().size.height);
                    let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
                    let track = (length - 34.).max(0.);
                    let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
                    let offset = -dialog_font::logical(scroll.offset().y);
                    let top = 17. + if maximum > 0. { offset / maximum * (track - thumb) } else { 0. };
                    let delta = if position < 17. { -24. } else if position > length - 17. { 24. }
                        else if position < top { -viewport } else if position > top + thumb { viewport }
                        else { view.terminal_config.scroll_grab.set(Some(position - top)); 0. };
                    scroll.set_offset(gpui::point(scroll.offset().x, px(-(offset + delta).clamp(0., maximum))));
                    cx.stop_propagation();
                    cx.notify();
                }))
                .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                    paint_bounds.set(bounds);
                    let length = dialog_font::logical(bounds.size.height);
                    let viewport = dialog_font::logical(paint_scroll.bounds().size.height);
                    let maximum = dialog_font::logical(paint_scroll.max_offset().y).max(0.);
                    let track = (length - 34.).max(0.);
                    if maximum > 0. && track > 0. {
                        let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
                        let offset = -dialog_font::logical(paint_scroll.offset().y);
                        let top = 17. + offset / maximum * (track - thumb);
                        let bounds = gpui::Bounds::new(gpui::point(bounds.left() + px(2.), bounds.top() + px(top)), size(px(13.), px(thumb)));
                        window.paint_quad(gpui::fill(bounds, rgb(if dark { 0x666666 } else { 0xcdcdcd })));
                    }
                }).size_full())
                .child(div().absolute().top(px(0.)).w_full().h(px(17.)).text_center().child("⌃"))
                .child(div().absolute().bottom(px(0.)).w_full().h(px(17.)).text_center().child("⌄")),
            self.terminal_config.scroll.clone()).into_any_element()
    }

    fn terminal_paste_radio(&self, value: PasteButton, label: &'static str, cx: &Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let index = usize::from(value == PasteButton::Right);
        let selected = self.terminal_config.settings.paste_button == value;
        let enabled = !self.terminal_config.confirm_delete;
        div().id(if value == PasteButton::Middle { "terminal-paste-middle" } else { "terminal-paste-right" })
            .role(Role::RadioButton).aria_label(label).aria_toggled(selected.into())
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .track_focus(&self.terminal_config.paste_focus[index].clone().tab_stop(enabled && selected))
            .h(px(23.)).flex().items_center().gap(px(5.)).text_color(rgb(palette.control_text(enabled)))
            .child(preferences_surface::pref_indicator_palette(selected, true, enabled, palette)).child(label)
            .on_click(cx.listener(move |view, _, window, cx| {
                if !enabled { return; }
                view.terminal_config.settings.paste_button = value;
                view.terminal_config.paste_focus[index].focus(window, cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if !enabled { return; }
                let value = match event.keystroke.key.as_str() {
                    "space" => value,
                    "left" | "right" | "up" | "down" => if value == PasteButton::Right { PasteButton::Middle } else { PasteButton::Right },
                    _ => return,
                };
                view.terminal_config.settings.paste_button = value;
                view.terminal_config.paste_focus[usize::from(value == PasteButton::Right)].focus(window, cx);
                cx.stop_propagation();
                cx.notify();
            })).into_any_element()
    }

    fn move_terminal_profile_scrollbar(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) { self.terminal_config.scroll_grab.set(None); return; }
        let Some(grab) = self.terminal_config.scroll_grab.get() else { return; };
        let bounds = self.terminal_config.scroll_bounds.get();
        let scroll = &self.terminal_config.scroll;
        let length = dialog_font::logical(bounds.size.height);
        let viewport = dialog_font::logical(scroll.bounds().size.height);
        let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
        let track = (length - 34.).max(0.);
        let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
        if track > thumb {
            let position = dialog_font::logical(event.position.y - bounds.top());
            let offset = -maximum * ((position - 17. - grab) / (track - thumb)).clamp(0., 1.);
            scroll.set_offset(gpui::point(scroll.offset().x, px(offset)));
            cx.notify();
        }
    }

    pub(super) fn render_terminal_config(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.terminal_config.visible { return None; }
        let palette = self.preferences_render_palette();
        for field in self.terminal_config.fields.clone() {
            field.update(cx, |field, cx| field.set_render_palette(Some(palette), cx));
        }
        let state = &self.terminal_config;
        let editing = state.edit.is_some();
        let confirming = state.confirm_delete;
        let idle = !editing && !confirming;
        let selected = state.settings.servers.iter().any(|profile| profile.id == state.selected);
        let widths = [100., 80., 110., 45., 90., 80., 55., 90., 90.];
        let labels = [self.terminal_label("名称", "Name"), self.terminal_label("模式", "Mode"),
            self.terminal_label("地址", "Address"), self.terminal_label("端口", "Port"),
            self.terminal_label("用户名", "User"), self.terminal_label("认证", "Authentication"),
            self.terminal_label("字号", "Font size"), self.terminal_label("背景颜色", "Background"), self.terminal_label("透明度", "Transparency")];
        let column = |text: String, width: f32| div().w(px(width)).min_w(px(width)).flex_shrink_0()
            .h_full().px(px(4.)).overflow_hidden().border_r_1().border_b_1()
            .border_color(rgb(if self.dark { 0x484848 } else { 0xdcdcdc })).child(text);
        let header = div().h(px(26.)).min_h(px(26.)).flex().bg(rgb(palette.button_background(true)))
            .children(labels.iter().zip(widths).map(|(label, width)| column((*label).to_owned(), width)
                .flex().items_center().justify_center().font_weight(gpui::FontWeight::BOLD)));
        let mut rows = div().id("terminal-profile-rows").role(Role::ListBox).aria_label(self.terminal_label("终端配置", "Terminal profiles"))
            .track_focus(&state.grid_focus.clone().tab_stop(idle)).flex().flex_col().flex_1().min_h(px(0.))
            .overflow_y_scroll().track_scroll(&state.scroll).on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if view.terminal_config.edit.is_some() || view.terminal_config.confirm_delete { return; }
                let ids = view.terminal_config.settings.servers.iter().map(|profile| profile.id).collect::<Vec<_>>();
                if ids.is_empty() { return; }
                let current = ids.iter().position(|id| *id == view.terminal_config.selected).unwrap_or(0);
                let page = (dialog_font::logical(view.terminal_config.scroll.bounds().size.height) / 24.).floor().max(1.) as usize;
                let next = match event.keystroke.key.as_str() {
                    "up" => current.saturating_sub(1), "down" => (current + 1).min(ids.len() - 1),
                    "home" => 0, "end" => ids.len() - 1,
                    "pageup" => current.saturating_sub(page), "pagedown" => current.saturating_add(page).min(ids.len() - 1),
                    "enter" | "space" => { view.begin_terminal_profile_edit(window, cx); cx.stop_propagation(); return; }
                    "delete" | "backspace" => { view.activate_terminal_config_button("terminal-profile-delete", window, cx); cx.stop_propagation(); return; }
                    _ => return,
                };
                view.select_terminal_profile(ids[next], window, cx);
                cx.stop_propagation();
            }));
        for (index, profile) in state.settings.servers.iter().enumerate() {
            let id = profile.id;
            let selected = id == state.selected;
            let background = if selected { if self.dark { 0x30465a } else { 0x000080 } }
                else if self.dark { if index % 2 == 0 { 0x202020 } else { 0x282828 } }
                else if index % 2 == 0 { 0xffffff } else { 0xf4f4f4 };
            let texts = [profile.name.clone(), "SSH".into(),
                profile.host.clone(), profile.port.to_string(), profile.username.clone(),
                if profile.authentication == Authentication::Key { self.terminal_label("密钥", "Public key").to_owned() }
                else { self.terminal_label("密码", "Password").to_owned() }, profile.font_size.to_string(),
                format!("#{:06X}", profile.background_color), format!("{}%", profile.transparency)];
            rows = rows.child(div().id(gpui::SharedString::from(format!("terminal-profile-{id}")))
                .role(Role::ListBoxOption).aria_selected(selected).h(px(24.)).min_h(px(24.)).flex_shrink_0()
                .flex().line_height(px(22.)).bg(rgb(background)).text_color(rgb(if selected { 0xffffff } else { palette.text }))
                .children(texts.into_iter().zip(widths).map(|(text, width)| column(text, width)))
                .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    if view.terminal_config.edit.is_some() || view.terminal_config.confirm_delete { return; }
                    view.select_terminal_profile(id, window, cx);
                    if event.click_count == 2 { view.begin_terminal_profile_edit(window, cx); }
                    cx.stop_propagation();
                })));
        }
        let grid = div().relative().h(px(148.)).flex_shrink_0().border_1().border_color(rgb(palette.edge))
            .child(div().size_full().pr(px(17.)).flex().flex_col().child(header).child(rows))
            .child(self.terminal_profile_scrollbar(cx));
        let mut root = self.terminal_dialog("terminal-config-dialog", self.terminal_label("终端配置", "Terminal Configuration"), cx)
            .p(px(10.)).gap(px(6.))
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
                view.move_terminal_profile_scrollbar(event, cx);
                if event.pressed_button != Some(MouseButton::Left) { view.terminal_config.transparency_dragging = false; }
                if view.terminal_config.transparency_dragging { view.set_terminal_transparency_position(event.position.x, cx); cx.stop_propagation(); }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| { view.terminal_config.scroll_grab.set(None); view.terminal_config.transparency_dragging = false; }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, _| { view.terminal_config.scroll_grab.set(None); view.terminal_config.transparency_dragging = false; }))
            .child(grid)
            .child(div().flex().gap(px(8.)).flex_shrink_0()
                .child(self.terminal_config_button("terminal-profile-add", self.terminal_label("新增", "Add"), idle, cx))
                .child(self.terminal_config_button("terminal-profile-edit", self.terminal_label("编辑", "Edit"), idle && selected, cx))
                .child(self.terminal_config_button("terminal-profile-delete", self.terminal_label("删除", "Delete"), idle && selected, cx)));
        if let Some(profile) = &state.edit {
            let key = profile.authentication == Authentication::Key;
            let field_widths = [160., 230., 65., 225.];
            let mut fields = div().flex().gap(px(8.));
            for index in 0..4 {
                fields = fields.child(div().w(px(field_widths[index])).flex().flex_col().gap(px(3.))
                    .child(labels[[0, 2, 3, 4][index]])
                    .child(div().h(px(23.)).child(state.fields[index].clone())));
            }
            root = root.child(div().flex_shrink_0().flex().flex_col().gap(px(6.)).border_1().border_color(rgb(palette.group_edge())).p(px(6.))
                .child(fields)
                .child(div().flex().items_end().gap(px(10.))
                    .child(div().flex().flex_col().gap(px(3.)).child(self.terminal_label("SSH 认证", "SSH authentication"))
                        .child(div().flex().gap(px(10.))
                            .child(self.terminal_profile_radio(0, self.terminal_label("密码", "Password"), !key, true, cx))
                            .child(self.terminal_profile_radio(1, self.terminal_label("密钥", "Public key"), key, true, cx))))
                    .child(div().w(px(70.)).flex().flex_col().gap(px(3.)).child(self.terminal_label("字号", "Font size"))
                        .child(div().h(px(23.)).child(state.fields[4].clone())))
                    .child(div().w(px(90.)).flex().flex_col().gap(px(3.)).child(self.terminal_label("背景颜色", "Background"))
                        .child(self.terminal_background_button(profile, cx)))
                    .child(div().w(px(155.)).flex().flex_col().gap(px(3.)).child(self.terminal_label("透明度", "Transparency"))
                        .child(div().h(px(23.)).flex().items_center().gap(px(6.))
                            .child(self.terminal_transparency_slider(profile, cx))
                            .child(div().w(px(36.)).text_right().child(format!("{}%", profile.transparency))))))
                .child(div().flex().justify_end().gap(px(8.))
                    .child(self.terminal_config_button("terminal-profile-apply", self.terminal_label("应用此行", "Apply row"), true, cx))
                    .child(self.terminal_config_button("terminal-profile-edit-cancel", self.terminal_label("取消编辑", "Cancel edit"), true, cx))));
        }
        if confirming {
            let profile = state.settings.profile(state.selected);
            let name = profile.map_or_else(String::new, |profile| profile.name.clone());
            root = root.child(div().flex_shrink_0().border_1().border_color(rgb(palette.group_edge())).p(px(8.)).flex().flex_col().gap(px(8.))
                .child(format!("{} {name}", self.terminal_label("删除配置？保存后关闭此配置的连接。", "Delete profile? Saving will close its connection.")))
                .child(div().flex().justify_end().gap(px(8.))
                    .child(self.terminal_config_button("terminal-profile-delete-confirm", self.terminal_label("删除", "Delete"), true, cx))
                    .child(self.terminal_config_button("terminal-profile-delete-cancel", self.terminal_label("取消", "Cancel"), true, cx))));
        }
        let key_available = !state.public_key.is_empty();
        root = root
            .child(div().flex().items_center().justify_between().gap(px(12.)).flex_shrink_0()
                .child(div().flex().items_center().gap(px(10.))
                    .child(self.terminal_label("鼠标粘贴：", "Mouse paste:"))
                    .child(self.terminal_paste_radio(PasteButton::Middle, self.terminal_label("鼠标中键", "Middle button"), cx))
                    .child(self.terminal_paste_radio(PasteButton::Right, self.terminal_label("鼠标右键", "Right button"), cx)))
                .child(div().flex().items_center().gap(px(6.))
                    .child(self.terminal_config_button("terminal-generate-key", self.terminal_label("生成密钥", "Generate key"), !key_available && !confirming, cx))
                    .child(self.terminal_config_button("terminal-copy-public-key", self.terminal_label("复制公钥登记命令", "Copy registration command"), key_available && !confirming, cx))))
            .when(!state.message.is_empty(), |root| root.child(div().id("terminal-config-status").min_h(px(24.)).flex_shrink_0().child(state.message.clone())))
            .child(div().flex().justify_end().gap(px(8.)).flex_shrink_0()
                .child(self.terminal_config_button("terminal-config-save", self.terminal_label("保存", "Save"), state.dirty(cx) && !confirming, cx))
                .child(self.terminal_config_button("terminal-config-cancel", self.terminal_label("取消", "Cancel"), true, cx)));
        Some(root.into_any_element())
    }

    pub(super) fn render_terminal_password(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.terminal_config.password_visible { return None; }
        let focus = self.terminal_config.password_focus[0].clone();
        if let Some(field) = &mut self.terminal_config.password
            && let Err(error) = field.install(window, cx, &focus) {
            self.notify_editor_error(error.to_string(), cx);
            self.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
            return None;
        }
        let palette = self.preferences_render_palette();
        let account = self.terminal_config.password_profile.as_ref()
            .map(|profile| format!("{}@{}:{}", profile.username, profile.host, profile.port))?;
        Some(self.terminal_dialog("terminal-password-dialog", self.terminal_label("SSH 密码", "SSH Password"), cx)
            .child(div().child(account))
            .child(div().id("terminal-password-field").track_focus(&focus).h(px(25.)).flex_shrink_0().w_full()
                .children(self.terminal_config.password.as_ref().map(|field| field.element(palette))))
            .child(div().mt_auto().flex().justify_end().gap(px(10.))
                .child(button("terminal-password-connect", self.terminal_label("连接", "Connect"), palette, true)
                    .track_focus(&self.terminal_config.password_focus[1])
                    .on_click(cx.listener(|view, _, window, cx| view.submit_terminal_password(window, cx)))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") { view.submit_terminal_password(window, cx); cx.stop_propagation(); }
                    })))
                .child(button("terminal-password-cancel", self.terminal_label("取消", "Cancel"), palette, true)
                    .track_focus(&self.terminal_config.password_focus[2])
                    .on_click(cx.listener(|view, _, window, cx| view.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx)))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") { view.dismiss_floating(floating_windows::DialogKind::TerminalPassword, window, cx); cx.stop_propagation(); }
                    }))))
            .into_any_element())
    }

    pub(super) fn render_terminal_host_key(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let fingerprint = self.ssh_terminal.fingerprint.as_ref()?;
        let palette = self.preferences_render_palette();
        Some(self.terminal_dialog("terminal-host-key-dialog", self.terminal_label("信任 SSH 主机", "Trust SSH Host"), cx)
            .child(div().child(self.ssh_terminal.endpoint_label()))
            .child(div().child(self.terminal_label("首次连接，请确认主机指纹：", "First connection. Verify the host fingerprint:")))
            .child(div().text_size(px(11.)).child(fingerprint.clone()))
            .child(div().child(self.terminal_label("确认后保存，以后连接自动校验。", "Confirmation saves this key for future connections.")))
            .child(div().mt_auto().flex().justify_end().gap(px(10.))
                .child(button("terminal-host-trust", self.terminal_label("信任并连接", "Trust and Connect"), palette, true)
                    .on_click(cx.listener(|view, _, window, cx| view.confirm_terminal_host_key(true, window, cx)))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") { view.confirm_terminal_host_key(true, window, cx); cx.stop_propagation(); }
                    })))
                .child(button("terminal-host-cancel", self.terminal_label("取消", "Cancel"), palette, true)
                    .on_click(cx.listener(|view, _, window, cx| view.confirm_terminal_host_key(false, window, cx)))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") { view.confirm_terminal_host_key(false, window, cx); cx.stop_propagation(); }
                    }))))
            .into_any_element())
    }
}
