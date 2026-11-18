use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub(super) struct NativeOpenPaths {
    pub paths: Vec<PathBuf>,
    pub launches: Vec<crate::launch::LaunchOptions>,
    pub errors: Vec<String>,
    pub during_launch: bool,
}

pub(super) type NativePaths = Arc<Mutex<VecDeque<NativeOpenPaths>>>;

pub(super) fn take_native_handoff(path: &Path) -> io::Result<Option<crate::launch::LaunchOptions>> {
    if path.file_name().is_none_or(|name| name != "notepad-matrix-launch.json") { return Ok(None); }
    let root = crate::file_access::home_directory()?.canonicalize()?.join(".notepad-matrix/launch-handoff");
    if !path.starts_with(&root) { return Ok(None); }
    crate::launch::take_native_handoff(path)
}

pub(super) fn native_open_urls(urls: Vec<String>, during_launch: bool) -> NativeOpenPaths {
    let mut request = NativeOpenPaths { paths: Vec::new(), launches: Vec::new(), errors: Vec::new(), during_launch };
    for value in urls {
        match url::Url::parse(&value).map_err(|error| format!("打开文件 URL 无效：{error}"))
            .and_then(|url| url.to_file_path().map_err(|_| format!("打开请求不是本地文件 URL：{value}"))) {
            Ok(path) => match take_native_handoff(&path) {
                Ok(Some(options)) => request.launches.push(options),
                Ok(None) => match crate::file_access::selected(&path) {
                    Ok(()) => request.paths.push(path),
                    Err(error) => request.errors.push(format!("无法接收文件授权：{error}")),
                },
                Err(error) => request.errors.push(format!("无法接收实例启动请求：{error}")),
            },
            Err(error) => request.errors.push(error),
        }
    }
    request
}

pub(super) fn take_startup_handoff(native_paths: &NativePaths, launch: &mut crate::launch::LaunchOptions) -> io::Result<bool> {
    if launch.files.len() == 1
        && let Some(options) = take_native_handoff(&launch.files[0])? {
        *launch = options;
        return Ok(true);
    }
    let mut requests = native_paths.lock().map_err(|_| io::Error::other("native launch queue is unavailable"))?;
    let errors = requests.iter().filter(|request| request.during_launch).flat_map(|request| request.errors.iter().cloned()).collect::<Vec<_>>();
    if !errors.is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidInput, errors.join("; "))); }
    let count = requests.iter().map(|request| request.launches.len()).sum::<usize>();
    if count > 1 { return Err(io::Error::new(io::ErrorKind::InvalidInput, "more than one startup handoff was received")); }
    for request in requests.iter_mut() {
        if let Some(options) = request.launches.pop() {
            *launch = options;
            return Ok(true);
        }
    }
    for request in requests.iter_mut().filter(|request| request.during_launch) {
        launch.files.append(&mut request.paths);
    }
    Ok(false)
}

impl MatrixWindow {
    pub(super) fn start_launch_receiver(
        &mut self,
        server: Option<crate::launch::InstanceServer>,
        native_paths: NativePaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let foreground = cx.background_executor().clone();
        self.launch_receiver = Some(cx.spawn_in(window, async move |view, cx| {
            let mut requests = VecDeque::<(crate::launch::LaunchOptions, Option<bool>)>::new();
            loop {
                foreground.timer(std::time::Duration::from_millis(50)).await;
                let received = match &server {
                    Some(server) => match server.accept() {
                        Ok(Some(request)) => Some(foreground.spawn(async move { request.read() }).await),
                        Ok(None) => None,
                        Err(error) => Some(Err(error)),
                    },
                    None => None,
                };
                let paths: Vec<_> = native_paths.lock().unwrap_or_else(std::sync::PoisonError::into_inner).drain(..).collect();
                let keep = view.update_in(cx, |view, window, cx| {
                    if let Some(received) = received {
                        match received {
                            Ok(request) => {
                                requests.push_back((request.into_options(), None));
                            }
                            Err(error) => view.notify_editor_error(format!("无法读取打开请求：{error}"), cx),
                        }
                    }
                    for request in paths {
                        for error in request.errors { view.notify_editor_error(error, cx); }
                        for launch in request.launches { requests.push_back((launch, None)); }
                        if !request.paths.is_empty() {
                            requests.push_back((crate::launch::LaunchOptions { files: request.paths, ..Default::default() }, Some(request.during_launch)));
                        }
                    }
                    if !view.file_preferences_runtime_busy() && !view.file_open_sequence_pending()
                        && !view.project_workspace_confirmation_pending()
                        && let Some((request, during_launch)) = requests.pop_front()
                    {
                        if let Some(during_launch) = during_launch {
                            view.open_native_file_paths(request.files, during_launch, window, cx);
                        } else if request.completion.is_some() || request.language.is_some() || request.udl_name.is_some() || request.byte_code_page.is_some() || request.line.is_some() || request.column.is_some() || request.open_session {
                            view.apply_launch_options(&request, window, cx);
                        } else if !request.files.is_empty() {
                            view.start_open_paths(request.files, window, cx);
                        }
                        window.activate_window();
                    }
                });
                if let Err(error) = keep { eprintln!("打开请求无法送达文档窗口：{error}"); break; }
            }
        }));
    }
}
