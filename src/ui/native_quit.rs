use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationTerminateReply};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol};

/// Main-thread native termination authorization, independent of GPUI's App borrow.
#[derive(Default)]
pub(super) struct QuitControl {
    approved: Cell<bool>,
    queued: Cell<bool>,
    error: RefCell<Option<String>>,
}

impl QuitControl {
    fn can_terminate_now(&self) -> bool {
        self.approved.get()
    }

    pub(super) fn approve(&self) {
        self.approved.set(true);
    }

    pub(super) fn cancel(&self) {
        // A late queued callback may outlive the final window. Cancelling that
        // request must not revoke the already-completed close authorization.
        self.queued.set(false);
    }

    pub(super) fn take_error(&self) -> Option<String> {
        self.error.borrow_mut().take()
    }
}

struct DelegateIvars {
    original: Retained<ProtocolObject<dyn NSApplicationDelegate>>,
    control: Rc<QuitControl>,
    schedule: Box<dyn Fn()>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; owned ivars remain alive
    // while AppKit's weak delegate reference is installed.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    struct MatrixApplicationDelegate;

    // SAFETY: NSObjectProtocol has no additional requirements.
    unsafe impl NSObjectProtocol for MatrixApplicationDelegate {
        #[unsafe(method(respondsToSelector:))]
        fn responds(&self, selector: Sel) -> bool {
            // SAFETY: Exact documented NSObject selector and signature.
            let own: bool = unsafe { msg_send![super(self), respondsToSelector: selector] };
            own || self.ivars().original.respondsToSelector(selector)
        }
    }

    // SAFETY: Typed AppKit methods run only on the main thread. The synchronous
    // decision never borrows GPUI; the callback only schedules foreground work.
    unsafe impl NSApplicationDelegate for MatrixApplicationDelegate {
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _: &NSApplication) -> NSApplicationTerminateReply {
            let control = &self.ivars().control;
            if control.can_terminate_now() {
                match self.restore() {
                    Ok(()) => return NSApplicationTerminateReply::TerminateNow,
                    Err(error) => {
                        control.approved.set(false);
                        control.cancel();
                        *control.error.borrow_mut() = Some(error.to_string());
                    }
                }
            }
            if !control.queued.replace(true) {
                (self.ivars().schedule)();
            }
            // Cancel resolves this native request immediately. There is no
            // NSTerminateLater reply to strand while our nonmodal guard runs.
            NSApplicationTerminateReply::TerminateCancel
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, notification: &NSNotification) {
            if let Err(error) = self.restore() {
                eprintln!("could not restore application delegate before termination: {error}");
            }
            self.ivars().original.applicationWillTerminate(notification);
        }
    }

    impl MatrixApplicationDelegate {
        #[unsafe(method(forwardingTargetForSelector:))]
        fn forwarding_target(&self, selector: Sel) -> Option<&AnyObject> {
            let original: &ProtocolObject<dyn NSApplicationDelegate> = &self.ivars().original;
            if original.respondsToSelector(selector) {
                Some(original.as_ref())
            } else {
                // SAFETY: Documented NSObject fast-forwarding selector. Its +0
                // result is borrowed; the retained original is used above.
                unsafe { msg_send![super(self), forwardingTargetForSelector: selector] }
            }
        }
    }
);

impl MatrixApplicationDelegate {
    fn restore(&self) -> io::Result<()> {
        let application = NSApplication::sharedApplication(self.mtm());
        let current = application
            .delegate()
            .ok_or_else(|| io::Error::other("application delegate disappeared"))?;
        if std::ptr::eq(&*current, &*self.ivars().original) {
            return Ok(());
        }
        if !std::ptr::eq(&*current, ProtocolObject::from_ref(self)) {
            return Err(io::Error::other(
                "application delegate changed; refusing to overwrite its replacement",
            ));
        }
        application.setDelegate(Some(&self.ivars().original));
        Ok(())
    }
}

/// Kept alive until GPUI's run loop ends. Native authorization restores GPUI's
/// original delegate before will-terminate handling and run-loop ivar cleanup.
pub(super) struct QuitGuard {
    delegate: Retained<MatrixApplicationDelegate>,
}

impl QuitGuard {
    pub(super) fn install(
        mtm: MainThreadMarker,
        control: Rc<QuitControl>,
        schedule: Box<dyn Fn()>,
    ) -> io::Result<Self> {
        let application = NSApplication::sharedApplication(mtm);
        let original = application
            .delegate()
            .ok_or_else(|| io::Error::other("GPUI application delegate unavailable"))?;
        if original.respondsToSelector(sel!(applicationShouldTerminate:))
            || !original.respondsToSelector(sel!(applicationWillTerminate:))
            || !original.respondsToSelector(sel!(handleGPUIMenuItem:))
        {
            return Err(io::Error::other(
                "unsupported application delegate or existing termination policy",
            ));
        }
        let allocated = MatrixApplicationDelegate::alloc(mtm).set_ivars(DelegateIvars {
            original,
            control,
            schedule,
        });
        // SAFETY: NSObject init initializes this allocated subclass with the
        // exact inherited signature; no runtime class or method is patched.
        let delegate: Retained<MatrixApplicationDelegate> =
            unsafe { msg_send![super(allocated), init] };
        application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Ok(Self { delegate })
    }
}

impl Drop for QuitGuard {
    fn drop(&mut self) {
        if let Err(error) = self.delegate.restore() {
            eprintln!("could not restore application delegate after run loop: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QuitControl;

    #[test]
    fn final_window_close_authorization_survives_stale_queued_request_cancellation() {
        let control = QuitControl::default();
        assert!(!control.can_terminate_now());
        control.approve();
        control.cancel();
        assert!(
            control.can_terminate_now(),
            "stale foreground work revoked completed window-close authorization"
        );
    }
}
