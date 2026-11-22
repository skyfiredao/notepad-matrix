use super::*;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationPresentationOptions, NSFloatingWindowLevel, NSNormalWindowLevel,
    NSWindow, NSWindowStyleMask, NSWindowWillCloseNotification,
};
use objc2_foundation::{NSNotificationCenter, NSRect};

#[derive(Default)]
struct ApplicationPresentation {
    owners: usize,
    saved: Option<NSApplicationPresentationOptions>,
}

thread_local! {
    static APPLICATION_PRESENTATION: RefCell<ApplicationPresentation> =
        RefCell::new(ApplicationPresentation::default());
}

struct PresentationOwner {
    main_thread: MainThreadMarker,
    active: std::cell::Cell<bool>,
}

impl PresentationOwner {
    fn release(&self) {
        if !self.active.replace(false) {
            return;
        }
        let saved = APPLICATION_PRESENTATION.with(|presentation| {
            let mut presentation = presentation.borrow_mut();
            presentation.owners -= 1;
            if presentation.owners == 0 {
                presentation.saved.take()
            } else {
                None
            }
        });
        if let Some(saved) = saved {
            NSApplication::sharedApplication(self.main_thread).setPresentationOptions(saved);
        }
    }
}

struct PresentationLease {
    owner: Rc<PresentationOwner>,
    observer: Retained<objc2::runtime::AnyObject>,
}

impl PresentationLease {
    fn new(main_thread: MainThreadMarker, host: &NSWindow) -> Self {
        let app = NSApplication::sharedApplication(main_thread);
        let first = APPLICATION_PRESENTATION.with(|presentation| {
            let mut presentation = presentation.borrow_mut();
            let first = presentation.owners == 0;
            if first {
                presentation.saved = Some(app.presentationOptions());
            }
            presentation.owners += 1;
            first
        });
        let owner = Rc::new(PresentationOwner {
            main_thread,
            active: std::cell::Cell::new(true),
        });
        let weak = Rc::downgrade(&owner);
        let block = block2::RcBlock::new(move |_: std::ptr::NonNull<objc2::runtime::AnyObject>| {
            if let Some(owner) = weak.upgrade() {
                owner.release();
            }
        });
        let center = NSNotificationCenter::defaultCenter();
        let observer = unsafe {
            objc2::msg_send![
                &*center,
                addObserverForName: NSWindowWillCloseNotification,
                object: host,
                queue: std::ptr::null::<objc2::runtime::AnyObject>(),
                usingBlock: &*block
            ]
        };
        if first {
            app.setPresentationOptions(NSApplicationPresentationOptions::AutoHideDock
                | NSApplicationPresentationOptions::AutoHideMenuBar);
        }
        Self { owner, observer }
    }
}

impl Drop for PresentationLease {
    fn drop(&mut self) {
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(&self.observer) };
        self.owner.release();
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) always_on_top: bool,
    pub(super) fullscreen: bool,
    pub(super) post_it: bool,
    pub(super) distraction_free: bool,
    style: Option<NSWindowStyleMask>,
    fullscreen_frame: Option<NSRect>,
    presentation: Option<PresentationLease>,
    post_it_on_top: bool,
}

#[derive(Default)]
pub(super) struct ViewState {
    pub(super) hidden: std::collections::BTreeMap<
        crate::workspace::BufferId,
        Vec<crate::editor::Document>,
    >,
}

impl MatrixWindow {
    pub(super) fn view_source_checked(&self, id: u32) -> Option<bool> {
        match id {
            44034 => Some(self.view_state.always_on_top),
            44032 => Some(self.view_state.fullscreen),
            44009 => Some(self.view_state.post_it),
            44011 => Some(self.view_state.distraction_free),
            _ => None,
        }
    }

    pub(super) fn view_source_enabled(&self, id: u32) -> Option<bool> {
        let available = !self.file_busy
            && !self.close_dialog
            && !self.save_all_dialog
            && !self.batch_editing_blocked();
        match id {
            44034 => Some(available),
            44032 | 44009 => Some(available && !self.view_state.distraction_free),
            44011 => Some(available && (self.view_state.distraction_free
                || (!self.view_state.fullscreen && !self.view_state.post_it))),
            44072 => Some(available),
            44042 | 44010 | 44029 | 44030 | 44031 | 44051..=44058 | 44061..=44068 => {
                Some(available)
            }
            44100..=44102 => Some(available && self.disk_file().is_some()),
            _ => None,
        }
    }

    pub(super) fn view_chrome_visible(&self) -> bool {
        !self.view_state.fullscreen && !self.view_state.post_it
    }

    pub(super) fn view_tabs_status_visible(&self) -> bool {
        !self.view_state.post_it
    }

    pub(super) fn view_panels_other_pane_visible(&self) -> bool {
        !self.view_state.distraction_free
    }

    pub(super) fn view_restore_command(&self) -> Option<u32> {
        if self.view_state.distraction_free {
            Some(44011)
        } else if self.view_state.post_it {
            Some(44009)
        } else if self.view_state.fullscreen {
            Some(44032)
        } else {
            None
        }
    }

    pub(super) fn execute_view_source(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.view_source_enabled(id) {
            None => return false,
            Some(false) => return true,
            Some(true) => {}
        }
        match id {
            44034 | 44032 | 44009 | 44011 => {
                let result = self.set_view_mode(id, window, cx);
                self.preferences.appearance.distraction_free = self.view_state.distraction_free;
                self.invalidate_style_layout(window, cx);
                if let Err(error) = result {
                    self.notify_editor_error(error.to_string(), cx);
                } else {
                    self.editor_focus.focus(window, cx);
                }
            }
            44072 => {
                let target = if self.editor_focus.is_focused(window) && self.split_views_visible()
                    && !self.view_state.distraction_free
                {
                    self.pane.other()
                } else {
                    self.pane
                };
                self.focus_editor_pane(target, window, cx);
            }
            44042 => self.hide_editor_lines(window, cx),
            44030 | 44031 => {
                self.refresh_editor_syntax();
                self.appearance_current_fold(id == 44030, window, cx);
            }
            44010 | 44029 => self.set_editor_folds(None, id == 44010, window, cx),
            44051..=44058 => self.set_editor_folds(Some((id - 44051) as usize), true, window, cx),
            44061..=44068 => self.set_editor_folds(Some((id - 44061) as usize), false, window, cx),
            44100..=44102 => {
                match self.view_current_file_in_browser(id) {
                    Ok(opened) => {
                        cx.spawn(async move |owner, cx| {
                            let result = opened.await
                                .map_err(|error| io::Error::other(format!("Browser handoff disconnected: {error}")))
                                .and_then(|result| result);
                            if let Err(error) = result {
                                if let Err(error) = owner.update(cx, |view, cx| view.notify_editor_error(error.to_string(), cx)) {
                                    eprintln!("Browser handoff lost its owner: {error}");
                                }
                            }
                        }).detach();
                    }
                    Err(error) => self.notify_editor_error(error.to_string(), cx),
                }
            }
            _ => return false,
        }
        true
    }

    fn set_view_mode(&mut self, id: u32, window: &Window, cx: &Context<Self>) -> io::Result<()> {
        let main_thread = MainThreadMarker::new()
            .ok_or_else(|| io::Error::other("View modes require the main thread"))?;
        let host = view_host_window(window)?;
        if id == 44034 {
            self.view_state.always_on_top = !self.view_state.always_on_top;
            host.setLevel(if self.view_state.always_on_top {
                NSFloatingWindowLevel
            } else {
                NSNormalWindowLevel
            });
            return Ok(());
        }
        let fullscreen = if id == 44032 || id == 44011 {
            !self.view_state.fullscreen
        } else {
            self.view_state.fullscreen
        };
        let post_it = if id == 44009 || id == 44011 {
            !self.view_state.post_it
        } else {
            self.view_state.post_it
        };
        let screen_frame = if fullscreen && !self.view_state.fullscreen {
            let screen: Option<Retained<objc2::runtime::AnyObject>> =
                unsafe { objc2::msg_send![&*host, screen] };
            let screen = screen.ok_or_else(|| io::Error::other("Window has no display"))?;
            Some(unsafe { objc2::msg_send![&*screen, frame] })
        } else {
            None
        };
        let previous_frame = host.frame();
        if !self.view_state.fullscreen && !self.view_state.post_it {
            self.view_state.style = Some(host.styleMask());
        }
        let style = if fullscreen || post_it {
            Some(NSWindowStyleMask::Borderless | NSWindowStyleMask::Resizable)
        } else {
            self.view_state.style.take()
        };
        let frame = if let Some(frame) = screen_frame {
            self.view_state.fullscreen_frame = Some(previous_frame);
            self.view_state.presentation = Some(PresentationLease::new(main_thread, &host));
            Some(frame)
        } else if !fullscreen && self.view_state.fullscreen {
            drop(self.view_state.presentation.take());
            self.view_state.fullscreen_frame.take()
        } else {
            None
        };
        if post_it && !self.view_state.post_it {
            self.view_state.post_it_on_top = self.view_state.always_on_top;
            self.view_state.always_on_top = true;
        } else if !post_it && self.view_state.post_it {
            self.view_state.always_on_top = self.view_state.post_it_on_top;
        }
        let level = if self.view_state.always_on_top {
            NSFloatingWindowLevel
        } else {
            NSNormalWindowLevel
        };
        self.view_state.fullscreen = fullscreen;
        self.view_state.post_it = post_it;
        if id == 44011 {
            self.view_state.distraction_free = !self.view_state.distraction_free;
        }
        let handle = window.window_handle();
        let owner = cx.weak_entity();
        let mut async_cx = cx.to_async();
        cx.foreground_executor().spawn(async move {
            let alive = handle.update(&mut async_cx, |_, _, _| ()).is_ok();
            if !alive { return; }
            if let Some(style) = style { host.setStyleMask(style); }
            if let Some(frame) = frame { host.setFrame_display(frame, true); }
            host.setLevel(level);
            host.makeKeyAndOrderFront(None);
            if let Some(view) = host.contentView() { host.makeFirstResponder(Some(&view)); }
            let restored = handle.update(&mut async_cx, |_, window, cx| {
                let owner = owner.upgrade().ok_or_else(|| io::Error::other("document window owner was released"))?;
                let view = owner.read(cx);
                window_caption::update_native_buttons(window,
                    view.view_chrome_visible() && !view.matrix_active() && !matrix_surface::locked())
            }).map_err(|error| io::Error::other(error.to_string())).and_then(|result| result);
            if let Err(error) = restored { eprintln!("could not restore native window buttons: {error}"); }
        }).detach();
        Ok(())
    }

    fn view_current_file_in_browser(&self, id: u32) -> io::Result<futures::channel::oneshot::Receiver<io::Result<()>>> {
        let bundle = match id {
            44100 => "org.mozilla.firefox",
            44101 => "com.google.Chrome",
            44102 => "com.microsoft.edgemac",
            _ => return Err(io::Error::other("Unknown browser command")),
        };
        MainThreadMarker::new()
            .ok_or_else(|| io::Error::other("Browser lookup requires the main thread"))?;
        let file = self.disk_file()
            .ok_or_else(|| io::Error::other("Save the current file before viewing it in a browser"))?;
        crate::native_app_open::open_in_application(file.path(), bundle)
    }
}

fn view_host_window(window: &Window) -> io::Result<Retained<NSWindow>> {
    MainThreadMarker::new()
        .ok_or_else(|| io::Error::other("View modes require the main thread"))?;
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(io::Error::other("Expected a macOS AppKit window"));
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window().ok_or_else(|| io::Error::other("GPUI view has no host window"))
}
