use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub(super) struct NativeOpenPaths {
    pub paths: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub during_launch: bool,
}

pub(super) type NativePaths = Arc<Mutex<VecDeque<NativeOpenPaths>>>;

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
                                requests.push_back((request.options.clone(), None));
                                if let Err(error) = request.acknowledge(true) { view.notify_editor_error(format!("无法确认打开请求：{error}"), cx); }
                            }
                            Err(error) => view.notify_editor_error(format!("无法读取打开请求：{error}"), cx),
                        }
                    }
                    for request in paths {
                        for error in request.errors { view.notify_editor_error(error, cx); }
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
                        } else if request.language.is_some() || request.udl_name.is_some() || request.byte_code_page.is_some() || request.line.is_some() || request.column.is_some() || request.open_session {
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
