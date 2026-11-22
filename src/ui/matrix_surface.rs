use super::*;
use std::cell::Cell;

thread_local! {
    static LOCKED: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn locked() -> bool {
    LOCKED.with(Cell::get)
}

#[derive(Default)]
pub(super) struct State {
    active: bool,
    rain: Option<matrix_rain::Rain>,
    terminal: Option<matrix_terminal::Terminal>,
    about_timer: Option<gpui::Task<()>>,
    about_generation: u64,
    about_seconds: Option<u8>,
    authentication: Option<matrix_auth::Authentication>,
    generation: u64,
    hidden: Vec<gpui::AnyWindowHandle>,
}

fn native_visibility(window: &Window, visible: bool) -> io::Result<bool> {
    MainThreadMarker::new().ok_or_else(|| io::Error::other("Matrix 窗口操作需要主线程"))?;
    let handle = HasWindowHandle::window_handle(window).map_err(|error| io::Error::other(error.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(io::Error::other("Matrix 保护需要 macOS 窗口"));
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let host = view.window().ok_or_else(|| io::Error::other("Matrix 窗口宿主不可用"))?;
    let was_visible = host.isVisible();
    if visible {
        host.orderFront(None);
    } else {
        host.makeFirstResponder(None);
        host.orderOut(None);
    }
    Ok(was_visible)
}

impl MatrixWindow {
    pub(super) fn matrix_style(&self) -> &'static str {
        if self.pref_string("Matrix.style", "matrix") == "terminal" { "terminal" } else { "matrix" }
    }

    pub(super) fn matrix_active(&self) -> bool {
        self.matrix.active
    }

    pub(super) fn open_matrix(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if locked() || self.file_preferences_runtime_busy() || self.file_open_sequence_pending() || self.printing {
            return false;
        }
        let protect = self.pref_bool("Matrix.lockScreen", false);
        if protect {
            let sheet_visible = MainThreadMarker::new().is_some_and(|main_thread|
                objc2_app_kit::NSApplication::sharedApplication(main_thread).windows().iter()
                    .any(|host| host.attachedSheet().is_some()));
            let color_visible: objc2::runtime::Bool = unsafe {
                let panel: *mut objc2::runtime::AnyObject = objc2::msg_send![objc2::class!(NSColorPanel), sharedColorPanel];
                objc2::msg_send![panel, isVisible]
            };
            if sheet_visible || color_visible.as_bool() || self.matrix_style_prompt_pending() || self.udl_work_pending()
                || self.project_workspace_confirmation_pending() {
                self.notify_editor_error("请先完成或关闭当前原生面板，再启动 Matrix 锁屏保护".into(), cx);
                return false;
            }
        }
        if protect && let Err(error) = matrix_auth::Authentication::available() {
            self.notify_editor_error(error.to_string(), cx);
            return false;
        }
        if !self.cancel_view_composition(window, cx) {
            return false;
        }
        self.matrix.active = true;
        self.matrix.about_timer = None;
        self.matrix.about_seconds = None;
        self.matrix.about_generation = self.matrix.about_generation.wrapping_add(1);
        if self.matrix_style() == "terminal" {
            self.matrix.rain = None;
            self.matrix.terminal = Some(matrix_terminal::Terminal::new());
        } else {
            self.matrix.terminal = None;
            self.matrix.rain = Some(matrix_rain::Rain::new());
        }
        self.focus.focus(window, cx);
        if protect {
            self.matrix.generation = self.matrix.generation.wrapping_add(1);
            LOCKED.with(|locked| locked.set(true));
            self.editor_dragging = false;
            self.tab_scroll_repeat = None;
            self.cancel_finder_drag();
            self.hide_matrix_windows(cx);
        }
        cx.notify();
        true
    }

    fn hide_matrix_windows(&mut self, cx: &mut Context<Self>) {
        let main = self.document_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            if !locked() { return; }
            let mut hidden = Vec::new();
            for handle in cx.windows() {
                if handle == main { continue; }
                let about = handle.downcast::<floating_windows::FloatingWindow>()
                    .and_then(|handle| handle.read(cx).ok().map(|view| view.is_about())).unwrap_or(false);
                if about { continue; }
                match handle.update(cx, |_, window, _| native_visibility(window, false)) {
                    Ok(Ok(true)) => hidden.push(handle),
                    Ok(Ok(false)) => {},
                    Ok(Err(error)) => eprintln!("Matrix window hide failed: {error}"),
                    Err(error) => eprintln!("Matrix window hide failed: {error}"),
                }
            }
            if let Err(error) = owner.update(cx, |view, _| view.matrix.hidden.extend(hidden)) {
                eprintln!("Matrix window state unavailable: {error}");
            }
        });
    }

    pub(super) fn close_matrix(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if locked() {
            self.authenticate_matrix(window, cx);
            return;
        }
        self.finish_matrix(cx);
    }

    fn finish_matrix(&mut self, cx: &mut Context<Self>) {
        self.matrix.active = false;
        self.matrix.rain = None;
        self.matrix.terminal = None;
        self.matrix.about_timer = None;
        self.matrix.about_seconds = None;
        self.matrix.about_generation = self.matrix.about_generation.wrapping_add(1);
        self.about_visible = false;
        self.restore_document_focus(cx);
        cx.notify();
    }

    pub(super) fn matrix_about_closed(&mut self, cx: &mut Context<Self>) {
        self.matrix.about_timer = None;
        self.matrix.about_seconds = None;
        self.matrix.about_generation = self.matrix.about_generation.wrapping_add(1);
        self.about_visible = false;
        if self.matrix.active && !locked() {
            self.finish_matrix(cx);
        }
    }

    pub(super) fn start_matrix_about_timer(&mut self, cx: &mut Context<Self>) {
        if !locked() || !self.matrix.active || !self.about_visible || self.matrix.about_timer.is_some() { return; }
        let generation = self.matrix.about_generation;
        let executor = cx.background_executor().clone();
        let started = std::time::Instant::now();
        self.matrix.about_seconds = Some(7);
        self.matrix.about_timer = Some(cx.spawn(async move |owner, cx| {
            for remaining in (0u8..7).rev() {
                let deadline = started + std::time::Duration::from_secs(u64::from(7 - remaining));
                executor.timer(deadline.saturating_duration_since(std::time::Instant::now())).await;
                let keep = owner.update(cx, |view, cx| {
                    if !locked() || !view.matrix.active || !view.about_visible || view.matrix.about_generation != generation {
                        return false;
                    }
                    if remaining == 0 {
                        view.matrix_about_closed(cx);
                    } else {
                        view.matrix.about_seconds = Some(remaining);
                    }
                    cx.notify();
                    remaining > 0
                });
                match keep {
                    Ok(true) => {},
                    Ok(false) => break,
                    Err(error) => { eprintln!("About auto-close owner unavailable: {error}"); break; }
                }
            }
        }));
    }

    pub(super) fn matrix_about_seconds(&self) -> Option<u8> {
        self.matrix.about_seconds
    }

    fn authenticate_matrix(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !locked() || self.matrix.authentication.is_some() { return; }
        let reason = if self.native_locale().starts_with("chinese") { "解锁 Notepad Matrix 并返回文档" } else { "Unlock Notepad Matrix and return to your documents" };
        let (authentication, receiver) = match matrix_auth::Authentication::begin(reason) {
            Ok(value) => value,
            Err(error) => {
                let response = window.prompt(gpui::PromptLevel::Warning, "Notepad Matrix", Some(&error.to_string()), &[gpui::PromptButton::ok("OK")], cx);
                cx.spawn(async move |_, _| {
                    if let Err(error) = response.await { eprintln!("Matrix authentication error prompt failed: {error}"); }
                }).detach();
                return;
            }
        };
        self.matrix.authentication = Some(authentication);
        let generation = self.matrix.generation;
        cx.spawn_in(window, async move |owner, cx| {
            let result = receiver.await.unwrap_or_else(|_| Err("系统认证已中断".into()));
            if let Err(error) = owner.update_in(cx, |view, window, cx| {
                if !locked() || view.matrix.generation != generation { return; }
                view.matrix.authentication = None;
                match result {
                    Ok(()) => {
                        LOCKED.with(|locked| locked.set(false));
                        view.matrix.generation = view.matrix.generation.wrapping_add(1);
                        let hidden = std::mem::take(&mut view.matrix.hidden);
                        cx.defer(move |cx| {
                            for handle in hidden {
                                if locked() { break; }
                                if cx.windows().contains(&handle) {
                                    match handle.update(cx, |_, window, _| native_visibility(window, true)) {
                                        Ok(Ok(_)) => {},
                                        Ok(Err(error)) => eprintln!("Matrix window restore failed: {error}"),
                                        Err(error) => eprintln!("Matrix window restore failed: {error}"),
                                    }
                                }
                            }
                        });
                        view.close_matrix(window, cx);
                    }
                    Err(error) => eprintln!("Matrix remains locked: {error}"),
                }
                cx.notify();
            }) { eprintln!("Matrix authentication owner unavailable: {error}"); }
        }).detach();
        cx.notify();
    }

    pub(super) fn render_matrix(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        window_caption::update_title(window, "Matrix - Notepad Matrix");
        let animation = if let Some(terminal) = &mut self.matrix.terminal {
            terminal.render(window)
        } else {
            self.matrix.rain.get_or_insert_with(matrix_rain::Rain::new).render(window)
        };
        div().id("matrix-rain-page").debug_selector(|| "matrix-rain-page".into())
            .role(Role::Application).aria_label("Matrix").track_focus(&self.focus)
            .key_context("MatrixAuxiliary")
            .on_action(cx.listener(|view, _: &CloseWindow, window, cx| { view.close_matrix(window, cx); cx.stop_propagation(); }))
            .on_action(cx.listener(|view, _: &QuitApplication, window, cx| { if locked() { view.authenticate_matrix(window, cx); } else { view.request_native_quit(window, cx); } cx.stop_propagation(); }))
            .on_click(cx.listener(|view, _, window, cx| {
                if locked() { view.authenticate_matrix(window, cx); }
            }))
            .size_full().overflow_hidden().bg(rgb(0x000000)).child(animation)
            .into_any_element()
    }
}
