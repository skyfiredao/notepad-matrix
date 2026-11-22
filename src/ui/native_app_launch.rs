use super::*;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};

fn file_url(path: &Path) -> io::Result<Retained<NSURL>> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "application path contains a null byte"))?;
    let url: Option<Retained<NSURL>> = unsafe {
        msg_send![class!(NSURL), fileURLWithFileSystemRepresentation: path.as_ptr(), isDirectory: false, relativeToURL: std::ptr::null::<NSURL>()]
    };
    url.ok_or_else(|| io::Error::other("application file URL could not be created"))
}

pub(super) async fn launch_handoff(
    options: crate::launch::LaunchOptions,
    executable: PathBuf,
    executor: gpui::BackgroundExecutor,
) -> io::Result<()> {
    let sandboxed = crate::file_access::sandboxed();
    let mut handoff = executor.spawn(async move { crate::launch::NativeHandoff::create(&options) }).await?;
    if !sandboxed {
        let bootstrap = crate::launch::LaunchOptions {
            files: vec![handoff.path()], multi_instance: true, no_session: true,
            ..Default::default()
        };
        let child = executor.spawn(async move { bootstrap.spawn(&executable) }).await?;
        let reaper = executor.clone();
        executor.spawn(async move {
            if let Err(error) = crate::child_process::reap(child, || reaper.timer(std::time::Duration::from_secs(1))).await {
                eprintln!("instance launcher could not be reaped: {error}");
            }
        }).detach();
    } else {
    let bundle = executable.parent().filter(|path| path.file_name().is_some_and(|name| name == "MacOS"))
        .and_then(Path::parent).filter(|path| path.file_name().is_some_and(|name| name == "Contents"))
        .and_then(Path::parent).filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .ok_or_else(|| io::Error::other("the sandboxed application must run from its application bundle"))?;
    let application = file_url(bundle)?;
    let urls = NSArray::from_retained_slice(&[file_url(&handoff.path())?]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let completion = block2::RcBlock::new(move |application: *mut AnyObject, error: *mut NSError| {
        let result = if !error.is_null() {
            let description: Retained<NSString> = unsafe { msg_send![error, localizedDescription] };
            Err(description.to_string())
        } else if application.is_null() {
            Err("the application did not start".to_owned())
        } else { Ok(()) };
        if sender.try_send(result).is_err() { eprintln!("native application launch confirmation expired"); }
    });
    let _main_thread = objc2::MainThreadMarker::new().ok_or_else(|| io::Error::other("native application launch must run on the UI thread"))?;
    unsafe {
        let configuration: Retained<AnyObject> = msg_send![class!(NSWorkspaceOpenConfiguration), configuration];
        let _: () = msg_send![&*configuration, setCreatesNewApplicationInstance: true];
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let _: () = msg_send![&*workspace, openURLs: &*urls, withApplicationAtURL: &*application, configuration: &*configuration, completionHandler: &*completion];
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match receiver.try_recv() {
            Ok(result) => { result.map_err(io::Error::other)?; break; },
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err(io::Error::other("native application launch confirmation disconnected")),
            Err(std::sync::mpsc::TryRecvError::Empty) => {},
        }
        if std::time::Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "native application launch timed out")); }
        executor.timer(std::time::Duration::from_millis(50)).await;
    }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let (next, result) = executor.spawn(async move {
            let result = handoff.result();
            (handoff, result)
        }).await;
        handoff = next;
        match result? {
            Some(true) => return Ok(()),
            Some(false) => return Err(io::Error::other("the new instance did not finish opening the requested documents")),
            None => {},
        }
        if std::time::Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "the new instance did not confirm document opening before the timeout")); }
        executor.timer(std::time::Duration::from_millis(50)).await;
    }
}
