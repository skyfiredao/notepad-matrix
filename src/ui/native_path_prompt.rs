use std::cell::Cell;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use block2::RcBlock;
use futures::channel::oneshot;
use gpui::{App, PathPromptOptions, Window};
use objc2::{class, msg_send};
use objc2::rc::Retained;
use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel, NSSavePanel, NSView, NSWindow};
use objc2_foundation::{MainThreadMarker, NSString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(super) struct PathPromptResult<T> {
    pub selection: Option<T>,
    pub directory: Option<PathBuf>,
}

pub(super) type PathPrompt<T> = Pin<Box<dyn Future<Output = io::Result<PathPromptResult<T>>>>>;

pub(super) fn prompt_for_paths_in(
    directory: &Path,
    options: PathPromptOptions,
    window: &mut Window,
    _cx: &mut App,
) -> PathPrompt<Vec<PathBuf>> {
    let started: io::Result<oneshot::Receiver<io::Result<PathPromptResult<Vec<PathBuf>>>>> = (|| {
        let host = host_window(window)?;
        let panel: Option<Retained<NSOpenPanel>> = unsafe { msg_send![class!(NSOpenPanel), openPanel] };
        let panel = panel.ok_or_else(|| io::Error::other("macOS 无法创建文件选择窗口"))?;
        panel.setCanChooseFiles(options.files);
        panel.setCanChooseDirectories(options.directories);
        panel.setAllowsMultipleSelection(options.multiple);
        panel.setCanCreateDirectories(true);
        panel.setResolvesAliases(false);
        let directory_url = objc2_foundation::NSURL::from_directory_path(directory)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件选择目录不是有效路径"))?;
        panel.setDirectoryURL(Some(&directory_url));
        if let Some(prompt) = options.prompt {
            panel.setPrompt(Some(&NSString::from_str(prompt.as_str())));
        }
        let (sender, receiver) = oneshot::channel();
        let sender = Cell::new(Some(sender));
        let completion = RcBlock::new({
            let panel = panel.clone();
            move |response: isize| {
                let Some(sender) = sender.take() else { return; };
                let result = (|| {
                    let directory = panel.directoryURL().and_then(|url| url.to_file_path());
                    let selection = if response == NSModalResponseOK {
                        let paths = panel.URLs().iter().filter(|url| url.isFileURL())
                            .map(|url| url.to_file_path().ok_or_else(|| io::Error::other("文件选择返回了无效路径")))
                            .collect::<io::Result<Vec<_>>>()?;
                        if paths.is_empty() { return Err(io::Error::other("文件选择未返回路径")); }
                        Some(paths)
                    } else if response == NSModalResponseCancel {
                        None
                    } else {
                        return Err(io::Error::other(format!("文件选择窗口未能完成：{response}")));
                    };
                    Ok(PathPromptResult { selection, directory })
                })();
                panel.orderOut(None);
                if sender.send(result).is_err() {
                    eprintln!("open path selection no longer has a document receiver");
                }
            }
        });
        unsafe {
            let _: () = msg_send![&*panel, beginSheetModalForWindow: &*host, completionHandler: &*completion];
        }
        Ok(receiver)
    })();
    Box::pin(async move {
        started?.await.map_err(|error| io::Error::other(format!("文件选择窗口已断开：{error}")))?
    })
}

pub(super) fn prompt_for_new_path_in(
    directory: &Path,
    suggested_name: Option<&str>,
    window: &mut Window,
    _cx: &mut App,
) -> PathPrompt<PathBuf> {
    let started: io::Result<oneshot::Receiver<io::Result<PathPromptResult<PathBuf>>>> = (|| {
        let host = host_window(window)?;
        let panel: Option<Retained<NSSavePanel>> = unsafe { msg_send![class!(NSSavePanel), savePanel] };
        let panel = panel.ok_or_else(|| io::Error::other("macOS 无法创建文件保存窗口"))?;
        let directory_url = objc2_foundation::NSURL::from_directory_path(directory)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件保存目录不是有效路径"))?;
        panel.setDirectoryURL(Some(&directory_url));
        if let Some(name) = suggested_name { panel.setNameFieldStringValue(&NSString::from_str(name)); }
        let (sender, receiver) = oneshot::channel();
        let sender = Cell::new(Some(sender));
        let completion = RcBlock::new({
            let panel = panel.clone();
            move |response: isize| {
                let Some(sender) = sender.take() else { return; };
                let result = (|| {
                    let directory = panel.directoryURL().and_then(|url| url.to_file_path());
                    let selection = if response == NSModalResponseOK {
                        let path = panel.URL().filter(|url| url.isFileURL())
                            .and_then(|url| url.to_file_path()).ok_or_else(|| io::Error::other("文件保存未返回有效路径"))?;
                        Some(path)
                    } else if response == NSModalResponseCancel {
                        None
                    } else {
                        return Err(io::Error::other(format!("文件保存窗口未能完成：{response}")));
                    };
                    Ok(PathPromptResult { selection, directory })
                })();
                panel.orderOut(None);
                if sender.send(result).is_err() {
                    eprintln!("save path selection no longer has a document receiver");
                }
            }
        });
        unsafe {
            let _: () = msg_send![&*panel, beginSheetModalForWindow: &*host, completionHandler: &*completion];
        }
        Ok(receiver)
    })();
    Box::pin(async move {
        started?.await.map_err(|error| io::Error::other(format!("文件保存窗口已断开：{error}")))?
    })
}

fn host_window(window: &Window) -> io::Result<Retained<NSWindow>> {
    MainThreadMarker::new().ok_or_else(|| io::Error::other("文件选择需要主线程"))?;
    let handle = HasWindowHandle::window_handle(window).map_err(|error| io::Error::other(error.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(io::Error::other("文件选择需要 AppKit 窗口"));
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let host = view.window().ok_or_else(|| io::Error::other("文件选择没有所属文档窗口"))?;
    if host.attachedSheet().is_some() {
        return Err(io::Error::other("请先关闭当前文件对话框"));
    }
    Ok(host)
}
