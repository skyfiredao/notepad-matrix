use std::cell::Cell;
use std::io;
use std::rc::{Rc, Weak};

use gpui::Window;
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSTextAlignment, NSTextField, NSView, NSWindow,
    NSWindowDidEndLiveResizeNotification, NSWindowDidResizeNotification,
    NSWindowWillCloseNotification, NSWindowWillMiniaturizeNotification,
    NSWindowWillStartLiveResizeNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSSize, NSString,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

struct IndicatorState {
    host: Retained<NSWindow>,
    parent: Retained<NSView>,
    label: Retained<NSTextField>,
    resizing: Cell<bool>,
    closed: Cell<bool>,
}

impl IndicatorState {
    fn refresh(&self) {
        if !self.resizing.get() || self.closed.get() {
            return;
        }
        let size = self.host.frame().size;
        self.label.setStringValue(&NSString::from_str(&format!(
            "{:.0} × {:.0}",
            size.width, size.height
        )));
        self.label.sizeToFit();
        let label_size = self.label.frame().size;
        let bounds = self.parent.bounds();
        let width = (label_size.width + 16.).min(bounds.size.width);
        let height = (label_size.height + 6.).min(bounds.size.height);
        let inset = 12_f64.min((bounds.size.height - height).max(0.));
        let y = if self.parent.isFlipped() {
            bounds.origin.y + bounds.size.height - height - inset
        } else {
            bounds.origin.y + inset
        };
        self.label.setFrame(NSRect::new(
            NSPoint::new(bounds.origin.x + (bounds.size.width - width) / 2., y),
            NSSize::new(width, height),
        ));
        self.label.setHidden(false);
        NSView::setNeedsDisplay(&self.label, true);
    }

    fn hide(&self) {
        self.resizing.set(false);
        self.label.setHidden(true);
    }
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Weak<IndicatorState>]
    struct MatrixWindowSizeObserver;

    unsafe impl NSObjectProtocol for MatrixWindowSizeObserver {}

    impl MatrixWindowSizeObserver {
        #[unsafe(method(matrixWindowWillStartLiveResize:))]
        fn begin_resize(&self, _: &NSNotification) {
            if let Some(state) = self.ivars().upgrade() {
                state.resizing.set(true);
                state.refresh();
            }
        }

        #[unsafe(method(matrixWindowDidResize:))]
        fn resize(&self, _: &NSNotification) {
            if let Some(state) = self.ivars().upgrade() {
                state.refresh();
            }
        }

        #[unsafe(method(matrixWindowDidEndLiveResize:))]
        fn end_resize(&self, _: &NSNotification) {
            if let Some(state) = self.ivars().upgrade() {
                state.hide();
            }
        }

        #[unsafe(method(matrixWindowWillClose:))]
        fn close(&self, _: &NSNotification) {
            if let Some(state) = self.ivars().upgrade() {
                state.closed.set(true);
                state.hide();
                state.label.removeFromSuperview();
            }
        }
    }
);

pub(super) struct WindowSizeIndicator {
    state: Rc<IndicatorState>,
    center: Retained<NSNotificationCenter>,
    observer: Retained<MatrixWindowSizeObserver>,
}

impl WindowSizeIndicator {
    pub(super) fn new(window: &Window) -> io::Result<Self> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| io::Error::other("window size indicator requires the main thread"))?;
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(io::Error::other("expected a macOS AppKit window"));
        };
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let host = view
            .window()
            .ok_or_else(|| io::Error::other("GPUI view has no host window"))?;
        let parent = host
            .contentView()
            .ok_or_else(|| io::Error::other("host window has no content view"))?;
        let label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        label.setDrawsBackground(true);
        label.setAlignment(NSTextAlignment::Center);
        label.setHidden(true);
        parent.addSubview(&label);
        let state = Rc::new(IndicatorState {
            host,
            parent,
            label,
            resizing: Cell::new(false),
            closed: Cell::new(false),
        });
        let allocated = MatrixWindowSizeObserver::alloc(mtm).set_ivars(Rc::downgrade(&state));
        let observer: Retained<MatrixWindowSizeObserver> =
            unsafe { msg_send![super(allocated), init] };
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            for (name, selector) in [
                (
                    NSWindowWillStartLiveResizeNotification,
                    sel!(matrixWindowWillStartLiveResize:),
                ),
                (NSWindowDidResizeNotification, sel!(matrixWindowDidResize:)),
                (
                    NSWindowDidEndLiveResizeNotification,
                    sel!(matrixWindowDidEndLiveResize:),
                ),
                (
                    NSWindowWillMiniaturizeNotification,
                    sel!(matrixWindowDidEndLiveResize:),
                ),
                (NSWindowWillCloseNotification, sel!(matrixWindowWillClose:)),
            ] {
                center.addObserver_selector_name_object(
                    observer.as_ref(),
                    selector,
                    Some(name),
                    Some(state.host.as_ref()),
                );
            }
        }
        Ok(Self {
            state,
            center,
            observer,
        })
    }
}

impl Drop for WindowSizeIndicator {
    fn drop(&mut self) {
        unsafe { self.center.removeObserver(self.observer.as_ref()) };
        self.state.hide();
        self.state.label.removeFromSuperview();
    }
}
