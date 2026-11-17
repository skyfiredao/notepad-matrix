use std::{io, sync::Mutex};

use block2::RcBlock;
use futures::channel::oneshot;
use objc2::{
    MainThreadMarker, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::{NSError, NSString};

#[link(name = "LocalAuthentication", kind = "framework")]
unsafe extern "C" {}

const DEVICE_OWNER_AUTHENTICATION: isize = 2;

pub(super) struct Authentication {
    context: Retained<AnyObject>,
    _main_thread: MainThreadMarker,
}

impl Authentication {
    pub(super) fn available() -> io::Result<()> {
        Self::new()?.preflight()
    }

    pub(super) fn begin(
        reason: &str,
    ) -> io::Result<(Self, oneshot::Receiver<Result<(), String>>)> {
        if reason.trim().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "系统认证原因不能为空"));
        }
        let authentication = Self::new()?;
        authentication.preflight()?;
        let reason = NSString::from_str(reason);
        let (sender, receiver) = oneshot::channel();
        let reply = RcBlock::new(authentication_reply(sender));
        unsafe {
            let _: () = msg_send![
                &*authentication.context,
                evaluatePolicy: DEVICE_OWNER_AUTHENTICATION,
                localizedReason: &*reason,
                reply: &*reply
            ];
        }
        Ok((authentication, receiver))
    }

    pub(super) fn invalidate(&self) {
        unsafe { let _: () = msg_send![&*self.context, invalidate]; }
    }

    fn new() -> io::Result<Self> {
        let main_thread = MainThreadMarker::new()
            .ok_or_else(|| io::Error::other("系统认证需要主线程"))?;
        let class = AnyClass::get(c"LAContext")
            .ok_or_else(|| io::Error::other("系统认证服务 LAContext 不可用"))?;
        let context = unsafe {
            let allocated: Allocated<AnyObject> = msg_send![class, alloc];
            let context: Option<Retained<AnyObject>> = msg_send![allocated, init];
            context.ok_or_else(|| io::Error::other("无法创建系统认证上下文"))?
        };
        Ok(Self { context, _main_thread: main_thread })
    }

    fn preflight(&self) -> io::Result<()> {
        let mut error: Option<Retained<NSError>> = None;
        let available: Bool = unsafe {
            msg_send![&*self.context, canEvaluatePolicy: DEVICE_OWNER_AUTHENTICATION, error: &mut error]
        };
        match error {
            Some(error) => Err(io::Error::other(error_message("系统认证不可用", &error))),
            None if available.as_bool() => Ok(()),
            None => Err(io::Error::other("系统认证不可用，系统未提供错误详情")),
        }
    }
}

impl Drop for Authentication {
    fn drop(&mut self) {
        self.invalidate();
    }
}

fn authentication_reply(
    sender: oneshot::Sender<Result<(), String>>,
) -> impl Fn(Bool, *mut NSError) + Send + Sync {
    let sender = Mutex::new(Some(sender));
    move |success: Bool, error: *mut NSError| {
        let sender = match sender.lock() {
            Ok(mut sender) => sender.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        let Some(sender) = sender else { return; };
        let result = match unsafe { error.as_ref() } {
            Some(error) => Err(error_message("系统认证未成功", error)),
            None if success.as_bool() => Ok(()),
            None => Err("系统认证未成功，系统未提供错误详情".to_owned()),
        };
        drop(sender.send(result));
    }
}

fn error_message(message: &str, error: &NSError) -> String {
    format!("{message}：{}（{}，错误码 {}）", error.localizedDescription(), error.domain(), error.code())
}
