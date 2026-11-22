use super::*;
use crate::folder_workspace::{
    EntryKind, FolderMutation, FolderScanController, FolderScanJob, FolderShellCommand,
    FolderWatch, FolderWorkspace, existing_buffer,
};

pub(super) struct FolderState {
    pub(super) tree: FolderWorkspace,
    picker_pending: bool,
    message: Option<String>,
    scroll: gpui::UniformListScrollHandle,
    horizontal_scroll_bounds: Rc<std::cell::Cell<Bounds<gpui::Pixels>>>,
    horizontal_scroll_grab: std::cell::Cell<Option<f32>>,
    horizontal_scroll_focus: FocusHandle,
    measured_width: RefCell<Option<FolderMeasuredWidth>>,
    scans: FolderScanController,
    scan_needed: bool,
    pending_root: Option<PathBuf>,
    pending_add: bool,
    scan_task: Option<gpui::Task<()>>,
    watch_task: Option<gpui::Task<()>>,
    watch_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    operation_task: Option<gpui::Task<()>>,
    selected: Option<PathBuf>,
    tree_focus: FocusHandle,
    command_input: Option<gpui::Entity<text_field::TextField>>,
    command_kind: Option<FolderEditKind>,
    context_menu: Option<(gpui::Point<gpui::Pixels>, Option<PathBuf>)>,
    context_focus: FocusHandle,
    context_selected: usize,
}

struct FolderMeasuredWidth {
    font_family: String,
    entries: Vec<(PathBuf, usize)>,
    width: gpui::Pixels,
}

#[derive(Clone, Copy)]
enum FolderEditKind {
    NewFile,
    NewDirectory,
    Rename,
}

#[derive(Clone, Copy)]
enum FolderCommand {
    Choose,
    Add,
    Remove,
    RemoveAll,
    Refresh,
    Open,
    OpenNative,
    Reveal,
    Terminal,
    Find,
    CopyPath,
    CopyName,
    NewFile,
    NewDirectory,
    Rename,
    Trash,
    FollowLinks,
}

impl FolderCommand {
    fn id(self) -> &'static str {
        match self {
            Self::Choose => "folder-workspace-choose",
            Self::Add => "folder-add-root",
            Self::Remove => "folder-remove-root",
            Self::RemoveAll => "folder-remove-all-roots",
            Self::Refresh => "folder-workspace-refresh",
            Self::Open => "folder-open",
            Self::OpenNative => "folder-open-native",
            Self::Reveal => "folder-reveal",
            Self::Terminal => "folder-terminal",
            Self::Find => "folder-find-in-files",
            Self::CopyPath => "folder-copy-path",
            Self::CopyName => "folder-copy-name",
            Self::NewFile => "folder-new-file",
            Self::NewDirectory => "folder-new-directory",
            Self::Rename => "folder-rename",
            Self::Trash => "folder-trash",
            Self::FollowLinks => "folder-follow-links",
        }
    }

    fn label(self, follow_links: bool, chinese: bool) -> &'static str {
        match self {
            Self::Choose => "Choose Folder…",
            Self::Add => "Add…",
            Self::Remove => "Remove",
            Self::RemoveAll => "Remove All",
            Self::Refresh => "Refresh",
            Self::Open => "Open",
            Self::OpenNative => "Run by system",
            Self::Reveal => "Reveal in Finder",
            Self::Terminal if chinese => "打开系统终端",
            Self::Terminal => "Open System Terminal",
            Self::Find => "Find in Files…",
            Self::CopyPath => "Copy path",
            Self::CopyName => "Copy file name",
            Self::NewFile => "New File…",
            Self::NewDirectory => "New Folder…",
            Self::Rename => "Rename…",
            Self::Trash => "Move to Trash",
            Self::FollowLinks if follow_links => "☑ Follow directory links",
            Self::FollowLinks => "☐ Follow directory links",
        }
    }
}

#[derive(Clone, Copy)]
enum FolderTreeCommand {
    Locate,
    FoldAll,
    ExpandAll,
}

fn folder_image(name: &str, style: &str) -> std::sync::Arc<gpui::Image> {
    macro_rules! image {
        ($style:literal, $name:literal) => {{
            static IMAGE: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
            IMAGE.get_or_init(|| std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                include_bytes!(concat!("../../assets/ui/folder/", $style, "/", $name, ".svg")).to_vec(),
            ))).clone()
        }};
    }
    macro_rules! images {
        ($style:literal) => {
            match name {
                "locate" => image!($style, "locate"),
                "fold" => image!($style, "fold"),
                "expand" => image!($style, "expand"),
                "root-open" => image!($style, "root-open"),
                "root-close" => image!($style, "root-close"),
                "folder-open" => image!($style, "folder-open"),
                "folder-close" => image!($style, "folder-close"),
                _ => image!($style, "file"),
            }
        };
    }
    match style {
        "dark" => images!("dark"),
        "light" => images!("light"),
        _ => images!("standard"),
    }
}

impl Drop for FolderState {
    fn drop(&mut self) {
        self.watch_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}


impl FolderState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            tree: FolderWorkspace::new(),
            picker_pending: false,
            message: None,
            scroll: gpui::UniformListScrollHandle::new(),
            horizontal_scroll_bounds: Rc::new(std::cell::Cell::new(Bounds::default())),
            horizontal_scroll_grab: std::cell::Cell::new(None),
            horizontal_scroll_focus: cx.focus_handle(),
            measured_width: RefCell::new(None),
            scans: FolderScanController::new(),
            scan_needed: false,
            pending_root: None,
            pending_add: false,
            scan_task: None,
            watch_task: None,
            watch_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            operation_task: None,
            selected: None,
            tree_focus: cx.focus_handle(),
            command_input: None,
            command_kind: None,
            context_menu: None,
            context_focus: cx.focus_handle(),
            context_selected: 0,
        }
    }

    fn finish_scan(&mut self, result: io::Result<FolderWorkspace>) {
        self.scan_needed = false;
        self.pending_root = None;
        self.pending_add = false;
        match result {
            Ok(mut tree) => {
                tree.set_visible(self.tree.visible());
                if tree.root() != self.tree.root() {
                    self.scroll = gpui::UniformListScrollHandle::new();
                }
                self.tree = tree;
                *self.measured_width.get_mut() = None;
                self.message = None;
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    fn content_width(&self, cx: &Context<MatrixWindow>) -> gpui::Pixels {
        let font_family = dialog_font::font_family();
        let entries = self.tree.entries();
        let mut cached = self.measured_width.borrow_mut();
        if let Some(measured) = cached.as_ref()
            && measured.font_family == font_family
            && measured.entries.len() == entries.len()
            && measured.entries.iter().zip(entries).all(|((path, depth), entry)|
                path == &entry.path && *depth == entry.depth)
        {
            return measured.width;
        }
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let font = gpui::font(font_family.clone());
        let width = entries.iter().map(|entry| {
            let name = entry.path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.path.display().to_string());
            let width = text_system.shape_line(name.clone().into(), px(12.), &[gpui::TextRun {
                len: name.len(), font: font.clone(), color: rgb(0x202020).into(),
                background_color: None, underline: None, strikethrough: None,
            }], None).width();
            width.ceil() + px(entry.depth as f32 * 19. + 43.)
        }).fold(px(0.), |width, row| width.max(row));
        *cached = Some(FolderMeasuredWidth {
            font_family,
            entries: entries.iter().map(|entry| (entry.path.clone(), entry.depth)).collect(),
            width,
        });
        width
    }
}

impl MatrixWindow {
    pub(super) fn preferences_drop_paths(&mut self,paths: Vec<PathBuf>,window: &mut Window,cx: &mut Context<Self>) {
        let routed = paths.clone();
        if self.route_to_document(window, cx, move |view, window, cx| view.preferences_drop_paths(routed, window, cx)) { return; }
        if self.folder_operations_blocked() || paths.is_empty() { return; }
        for path in &paths {
            if let Err(error) = crate::file_access::selected(path) {
                self.notify_editor_error(format!("无法取得拖放项目的访问权限: {error}"), cx);
                return;
            }
        }
        let folder_count = paths.iter().filter(|path| path.is_dir()).count();
        let open_all = self.preferences.files.file_options.open_dropped_folder_files;
        if !open_all && folder_count != 0 && folder_count != paths.len() {
            let prompt = window.prompt(gpui::PromptLevel::Warning,"无效的拖放操作",Some("文件夹工作区模式不能同时拖放文件和文件夹。请在默认目录首选项中启用“打开文件夹内所有文件”。"),&[gpui::PromptButton::ok("确定")],cx);
            cx.spawn(async move |_,_| { if let Err(error) = prompt.await { eprintln!("mixed file folder drop prompt failed: {error}"); } }).detach();
            return;
        }
        if folder_count == 0 { self.start_open_paths(paths,window,cx); return; }
        self.file_busy = true;
        let executor = cx.background_executor().clone();
        if !open_all {
            let mut tree = self.folder.tree.clone();
            self.pause_folder_watch();
            cx.spawn_in(window,async move |view,cx| {
                let result = executor.spawn(async move {
                    for path in paths { tree.add_root(&path)?; }
                    Ok::<_,io::Error>(tree)
                }).await;
                if let Err(error) = view.update_in(cx,|view,window,cx| {
                    view.file_busy = false;
                    view.folder.tree.set_visible(true);
                    view.folder.finish_scan(result);
                    view.show_panel(crate::panel_layout::PanelId::Folder,window,cx);
                    view.resume_folder_watch(window,cx);
                    cx.notify();
                }) { eprintln!("folder drop window unavailable: {error}"); }
            }).detach();
        } else {
            cx.spawn_in(window,async move |view,cx| {
                let result = executor.spawn(async move { crate::folder_workspace::dropped_folder_files(paths) }).await;
                let files = match result {
                    Ok(files) => files,
                    Err(error) => {
                        if let Err(update_error) = view.update_in(cx,|view,_,cx| { view.file_busy = false; view.notify_editor_error(format!("文件夹无法打开: {error}"),cx); }) { eprintln!("folder drop failure window unavailable: {update_error}"); }
                        return;
                    }
                };
                if files.len() > 200 {
                    let message = format!("即将打开 {} 个文件。是否继续？",files.len());
                    let prompt = match view.update_in(cx,|_,window,cx| window.prompt(gpui::PromptLevel::Warning,"打开文件过多",Some(&message),&[gpui::PromptButton::ok("是"),gpui::PromptButton::cancel("否")],cx)) {
                        Ok(prompt) => prompt,
                        Err(error) => { eprintln!("folder drop confirmation unavailable: {error}"); return; }
                    };
                    match prompt.await {
                        Ok(0) => {},
                        Ok(_) => { if let Err(error) = view.update_in(cx,|view,_,cx| { view.file_busy = false; cx.notify(); }) { eprintln!("folder drop cancel unavailable: {error}"); } return; }
                        Err(error) => { if let Err(update_error) = view.update_in(cx,|view,_,cx| { view.file_busy = false; view.notify_editor_error(error.to_string(),cx); }) { eprintln!("folder drop prompt failure unavailable: {update_error}"); } return; }
                    }
                }
                if let Err(error) = view.update_in(cx,|view,window,cx| { view.file_busy = false; view.start_open_paths(files,window,cx); }) { eprintln!("folder files drop unavailable: {error}"); }
            }).detach();
        }
        cx.notify();
    }

    pub(super) fn configure_folder_file_preferences(&mut self,settings: &serde_json::Value,window: &mut Window,cx: &mut Context<Self>) {
        let allowed = settings["isFawSymlinkAllowed"].as_bool().unwrap_or(false);
        let changed = self.folder.tree.follow_links() != allowed;
        self.preferences.files.follow_links = allowed;
        if !changed { return; }
        self.folder.tree.set_follow_links(allowed);
        if allowed && self.preferences.files.applied && !self.preferences.files.symlink_warning_shown {
            self.preferences.files.symlink_warning_shown = true;
            let prompt = window.prompt(gpui::PromptLevel::Warning,"符号链接",Some("载入符号链接可能遇到指向父文件夹或循环引用的目录。本应用不会重复遍历同一实际目录。"),&[gpui::PromptButton::ok("确定")],cx);
            cx.spawn(async move |_,_| { if let Err(error) = prompt.await { eprintln!("symlink preference warning failed: {error}"); } }).detach();
        }
        self.folder.scan_needed = self.folder.tree.root().is_some();
        if self.folder.tree.visible() { self.scan_workspace_folder(window,cx); }
    }

    fn find_in_selected_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.find_in_selected_folder(window, cx)
        }) {
            return;
        }
        let Some(path) = self.folder_selected_path() else {
            return;
        };
        let directory = if path.is_dir() {
            path
        } else {
            path.parent().map(PathBuf::from).unwrap_or_default()
        };
        self.show_find_in_directory(directory, window, cx);
    }

    pub(super) fn capture_folder_workspace(&self) -> io::Result<serde_json::Value> {
        self.folder.tree.snapshot()
    }

    pub(super) fn restore_folder_workspace(
        &mut self,
        value: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        let mut tree = FolderWorkspace::restore(value)?;
        tree.set_follow_links(self.preferences.files.follow_links);
        self.pause_folder_watch();
        self.folder.tree = tree;
        self.folder.scan_needed = self.folder.tree.root().is_some();
        self.folder.selected = None;
        self.folder.context_menu = None;
        self.folder.pending_root = None;
        self.folder.pending_add = false;
        if self.folder.tree.visible() {
            self.show_panel(crate::panel_layout::PanelId::Folder, window, cx);
            self.scan_workspace_folder(window, cx);
        } else {
            self.hide_panel(crate::panel_layout::PanelId::Folder, window, cx);
        }
        cx.notify();
        Ok(())
    }

    fn folder_selected_path(&self) -> Option<PathBuf> {
        self.folder
            .selected
            .clone()
            .or_else(|| self.folder.tree.root().map(PathBuf::from))
    }

    fn begin_folder_edit(
        &mut self,
        kind: FolderEditKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.folder_operations_blocked() || self.folder_selected_path().is_none() {
            return;
        }
        let input = cx.new(|cx| {
            text_field::TextField::new("File or directory name", "folder-command-name", cx)
        });
        let text = if matches!(kind, FolderEditKind::Rename) {
            self.folder_selected_path()
                .and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        input.update(cx, |field, cx| {
            field.set_text(&text, window, cx);
            field.focus(window, cx);
        });
        self.folder.command_input = Some(input);
        self.folder.command_kind = Some(kind);
        cx.notify();
    }

    fn apply_folder_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_operations_blocked() {
            return;
        }
        let (Some(kind), Some(input), Some(path)) = (
            self.folder.command_kind,
            self.folder.command_input.as_ref(),
            self.folder_selected_path(),
        ) else {
            return;
        };
        let name = input.read(cx).text();
        let parent = if path.is_dir() {
            path.clone()
        } else {
            path.parent().map(PathBuf::from).unwrap_or_default()
        };
        let operation = match kind {
            FolderEditKind::NewFile => FolderMutation::CreateFile { parent, name },
            FolderEditKind::NewDirectory => FolderMutation::CreateDirectory { parent, name },
            FolderEditKind::Rename => FolderMutation::Rename { path, name },
        };
        self.run_folder_mutation(operation, window, cx);
    }

    fn run_folder_mutation(
        &mut self,
        operation: FolderMutation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = operation.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.run_folder_mutation(routed, window, cx)
        }) {
            return;
        }
        if self.folder_operations_blocked() {
            return;
        }
        self.file_busy = true;
        let renamed_from = match &operation {
            FolderMutation::Rename { path, .. } => Some(path.clone()),
            _ => None,
        };
        let worker = cx
            .background_executor()
            .spawn(async move { operation.execute() });
        self.folder.operation_task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = worker.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match result {
                    Ok(path) => {
                        if let Some(from) = renamed_from {
                            crate::folder_workspace::relocate_open_paths(
                                &mut view.workspace,
                                &from,
                                &path,
                            );
                            view.folder.tree.relocate_paths(&from, &path);
                        }
                        view.folder.selected = Some(path);
                        view.folder.command_input = None;
                        view.folder.command_kind = None;
                        view.scan_workspace_folder(window, cx);
                    }
                    Err(error) => view.folder.message = Some(error.to_string()),
                }
                cx.notify();
            }) {
                eprintln!("folder command completion lost document: {error}");
            }
        }));
        cx.notify();
    }

    fn folder_native_command(
        &mut self,
        command: FolderShellCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.folder_native_command(command, window, cx)
        }) {
            return;
        }
        let Some(path) = self.folder_selected_path() else {
            return;
        };
        if self.folder_operations_blocked() {
            return;
        }
        self.file_busy = true;
        let worker = cx
            .background_executor()
            .spawn(async move { crate::folder_workspace::folder_shell(&path, command) });
        self.folder.operation_task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = worker.await;
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.file_busy = false;
                if let Err(error) = result {
                    view.folder.message = Some(error.to_string());
                }
                cx.notify();
            }) {
                eprintln!("native folder command lost document: {error}");
            }
        }));
        cx.notify();
    }

    fn remove_folder_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.remove_folder_root(window, cx)
        }) {
            return;
        }
        if self.folder_operations_blocked() {
            return;
        }
        let Some(selected) = self.folder_selected_path() else {
            return;
        };
        let root = self
            .folder
            .tree
            .roots()
            .filter(|root| selected.starts_with(root))
            .max_by_key(|root| root.components().count())
            .map(PathBuf::from);
        let Some(root) = root else {
            return;
        };
        self.pause_folder_watch();
        if let Err(error) = self.folder.tree.remove_root(&root) {
            self.folder.message = Some(error.to_string());
            self.resume_folder_watch(window, cx);
            cx.notify();
            return;
        }
        self.folder.selected = None;
        self.folder.pending_root = None;
        self.folder.scan_needed = false;
        self.folder.message = None;
        self.resume_folder_watch(window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    fn remove_all_folder_roots(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.remove_all_folder_roots(window, cx)
        }) {
            return;
        }
        if self.folder_operations_blocked() {
            return;
        }
        self.pause_folder_watch();
        self.folder.tree.remove_all_roots();
        self.folder.selected = None;
        self.folder.pending_root = None;
        self.folder.scan_needed = false;
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn pause_folder_watch(&mut self) {
        self.folder
            .watch_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.folder.watch_task = None;
        self.folder.scans.cancel();
        self.folder.scan_needed = self.folder.tree.root().is_some();
    }

    pub(super) fn resume_folder_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.folder.tree.visible() || self.folder.tree.root().is_none() {
            return;
        }
        if self.folder.scan_needed && !self.folder.scans.is_busy() {
            self.scan_workspace_folder(window, cx);
        }
        if self.folder.watch_task.is_some() {
            return;
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.folder.watch_cancel = cancel.clone();
        self.folder.watch_task = Some(cx.spawn_in(window, async move |view, cx| {
            let mut watch = FolderWatch::default();
            let mut first_poll = true;
            let mut watched_root = None;
            loop {
                let tree = match view.update_in(cx, |view, _, _| view.folder.tree.clone()) {
                    Ok(tree) if tree.visible() => tree,
                    Ok(_) => return,
                    Err(error) => {
                        eprintln!("folder watch lost document: {error}");
                        return;
                    }
                };
                let root = tree.root().map(PathBuf::from);
                if watched_root != root {
                    first_poll = true;
                    watched_root = root.clone();
                }
                let token = cancel.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let changed = watch.poll_cancellable(&tree, &token);
                        (watch, changed)
                    })
                    .await;
                watch = result.0;
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                let sampled = result.1.is_ok();
                if let Err(error) = view.update_in(cx, |view, window, cx| {
                    if view.folder.tree.root() != root.as_deref() || !view.folder.tree.visible() {
                        return;
                    }
                    match result.1 {
                        Ok(changed) if changed || first_poll => {
                            view.scan_workspace_folder(window, cx)
                        }
                        Ok(_) => {}
                        Err(error) => {
                            view.folder.message = Some(format!("Folder watch: {error}"));
                            cx.notify();
                        }
                    }
                }) {
                    eprintln!("folder watch completion lost document: {error}");
                    return;
                }
                if sampled {
                    first_poll = false;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
            }
        }));
    }

    pub(super) fn folder_workspace_visible(&self) -> bool {
        self.folder.tree.visible()
    }

    pub(super) fn toggle_folder_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.toggle_folder_workspace(window, cx)
        }) {
            return;
        }
        self.folder.context_menu = None;
        if self.folder.tree.visible() {
            self.close_folder_workspace(window, cx);
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.show_panel(crate::panel_layout::PanelId::Folder, window, cx);
        cx.notify();
        if self.folder.scan_needed {
            self.scan_workspace_folder(window, cx);
        }
    }

    pub(super) fn close_folder_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.close_folder_workspace(window, cx)
        }) {
            return;
        }
        self.folder.context_menu = None;
        self.hide_panel(crate::panel_layout::PanelId::Folder, window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn choose_workspace_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choose_workspace_folder_mode(false, window, cx);
    }

    pub(super) fn add_workspace_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choose_workspace_folder_mode(true, window, cx);
    }

    fn choose_workspace_folder_mode(
        &mut self,
        add: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.choose_workspace_folder_mode(add, window, cx)
        }) {
            return;
        }
        if self.folder.picker_pending || self.folder_operations_blocked() {
            return;
        }
        self.folder.picker_pending = true;
        self.file_busy = true;
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose workspace folder".into()),
        });
        cx.notify();
        self.folder.operation_task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = match prompt.await {
                Ok(Ok(paths)) => Ok(paths),
                Ok(Err(error)) => Err(format!("Folder picker: {error}")),
                Err(error) => Err(format!("Folder picker disconnected: {error}")),
            };
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                view.folder.picker_pending = false;
                match result {
                    Ok(Some(paths)) if paths.len() == 1 => {
                        view.folder.pending_root = paths.into_iter().next();
                        view.folder.pending_add = add;
                        view.show_panel(crate::panel_layout::PanelId::Folder, window, cx);
                        view.scan_workspace_folder(window, cx);
                    }
                    Ok(Some(_)) => {
                        view.folder.message = Some("Choose exactly one directory".into())
                    }
                    Ok(None) => {}
                    Err(error) => view.folder.message = Some(error),
                }
                cx.notify();
            }) {
                eprintln!("folder picker completion lost its document: {error}");
            }
        }));
    }

    fn folder_operations_blocked(&self) -> bool {
        self.file_busy
            || self.file_open_sequence_pending()
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
    }

    pub(super) fn refresh_workspace_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.refresh_workspace_folder(window, cx)
        }) {
            return;
        }
        self.scan_workspace_folder(window, cx);
    }

    fn scan_workspace_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.scan_workspace_folder(window, cx)
        }) {
            return;
        }
        self.folder.scan_needed = true;
        if !self.folder.tree.visible() {
            self.folder.scans.cancel();
            cx.notify();
            return;
        }
        let input = match (self.folder.pending_add, self.folder.pending_root.clone()) {
            (true, Some(root)) => self.folder.tree.scan_input_add_root(root),
            (_, root) => self.folder.tree.scan_input(root),
        };
        self.folder.message = None;
        if let Some(job) = self.folder.scans.request(input) {
            self.start_folder_scan(job, window, cx);
        }
        cx.notify();
    }

    fn start_folder_scan(
        &mut self,
        job: FolderScanJob,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scan = cx.background_executor().spawn(async move { job.run() });
        self.folder.scan_task = Some(cx.spawn_in(window, async move |view, cx| {
            let completed = scan.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                let update = view.folder.scans.complete(completed);
                if let Some(result) = update.result {
                    view.folder.finish_scan(result);
                    view.resume_folder_watch(window, cx);
                }
                if let Some(next) = update.next {
                    view.start_folder_scan(next, window, cx);
                }
                cx.notify();
            }) {
                eprintln!("folder scan completion lost its document: {error}");
            }
        }));
    }

    fn activate_folder_entry(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = path.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.activate_folder_entry(routed, window, cx)
        }) {
            return;
        }
        if self.folder.pending_root.is_some() {
            return;
        }
        self.folder.selected = Some(path.clone());
        let Some(kind) = self
            .folder
            .tree
            .entries()
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.kind)
        else {
            return;
        };
        match kind {
            EntryKind::Directory => match self.folder.tree.toggle_directory_expansion(&path) {
                Ok(()) => self.scan_workspace_folder(window, cx),
                Err(error) => {
                    self.folder.message = Some(error.to_string());
                    cx.notify();
                }
            },
            EntryKind::File => self.open_workspace_file(path, window, cx),
            EntryKind::Symlink => match std::fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => self.open_workspace_file(path, window, cx),
                Ok(_) => {
                    self.folder.message =
                        Some("Directory symlinks are not traversed (loop prevention)".into());
                    cx.notify();
                }
                Err(error) => {
                    self.folder.message = Some(format!("{}: {error}", path.display()));
                    cx.notify();
                }
            },
            EntryKind::Other => {
                self.folder.message = Some("Only regular files can be opened".into());
                cx.notify();
            }
        }
    }

    pub(super) fn open_workspace_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = path.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.open_workspace_file(routed, window, cx)
        }) {
            return;
        }
        if self.folder_operations_blocked() {
            return;
        }
        if let Some(id) = existing_buffer(&self.workspace, &path) {
            if self.activate_buffer(id, window, cx) {
                self.folder.message = None;
                window.activate_window();
            }
            return;
        }
        self.folder.message = None;
        self.start_open_paths(vec![path], window, cx);
        window.activate_window();
    }

    fn run_folder_tree_command(
        &mut self,
        command: FolderTreeCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.run_folder_tree_command(command, window, cx)
        }) {
            return;
        }
        if self.folder_operations_blocked() || self.folder.scans.is_busy() || self.folder.pending_root.is_some() {
            return;
        }
        let locate = if matches!(command, FolderTreeCommand::Locate) {
            let Some(file) = self.workspace.active().disk_file.as_ref() else { return; };
            let path = file.path().to_path_buf();
            if !self.folder.tree.roots().any(|root| path.starts_with(root)) {
                return;
            }
            Some(path)
        } else {
            None
        };
        let mut tree = self.folder.tree.clone();
        let target = locate.clone();
        self.pause_folder_watch();
        self.file_busy = true;
        let worker = cx.background_executor().spawn(async move {
            let mut state = tree.snapshot()?;
            let mut expanded: std::collections::BTreeSet<PathBuf> =
                serde_json::from_value(state["expanded"].clone()).map_err(io::Error::other)?;
            match command {
                FolderTreeCommand::Locate => {
                    if let Some(path) = &target {
                        for parent in path.ancestors().skip(1) {
                            if tree.roots().any(|root| parent.starts_with(root)) {
                                expanded.insert(parent.to_path_buf());
                            }
                        }
                    }
                }
                FolderTreeCommand::FoldAll => expanded.clear(),
                FolderTreeCommand::ExpandAll => {}
            }
            state["expanded"] = serde_json::to_value(&expanded).map_err(io::Error::other)?;
            tree = FolderWorkspace::restore(&state)?;
            tree.refresh();
            if matches!(command, FolderTreeCommand::ExpandAll) {
                loop {
                    let previous = expanded.len();
                    expanded.extend(tree.entries().iter()
                        .filter(|entry| entry.kind == EntryKind::Directory)
                        .map(|entry| entry.path.clone()));
                    if expanded.len() == previous { break; }
                    state["expanded"] = serde_json::to_value(&expanded).map_err(io::Error::other)?;
                    tree = FolderWorkspace::restore(&state)?;
                    tree.refresh();
                }
            }
            Ok::<_, io::Error>(tree)
        });
        self.folder.operation_task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = worker.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                let completed = result.is_ok();
                view.folder.finish_scan(result);
                if completed && matches!(command, FolderTreeCommand::FoldAll)
                    && let Some(selected) = view.folder.selected.as_ref()
                {
                    view.folder.selected = selected.ancestors()
                        .find(|path| view.folder.tree.entries().iter().any(|entry| entry.path == *path))
                        .map(PathBuf::from);
                }
                if completed && let Some(path) = locate
                    && let Some(index) = view.folder.tree.entries().iter().position(|entry| entry.path == path)
                {
                    view.folder.selected = Some(path);
                    view.folder.scroll.scroll_to_item(index, gpui::ScrollStrategy::Nearest);
                    view.folder.tree_focus.focus(window, cx);
                }
                view.resume_folder_watch(window, cx);
                cx.notify();
            }) {
                eprintln!("folder toolbar completion lost document: {error}");
            }
        }));
        cx.notify();
    }

    fn folder_context_commands(&self) -> Vec<Option<FolderCommand>> {
        use FolderCommand::*;
        let path = self.folder.context_menu.as_ref().and_then(|(_, path)| path.as_ref());
        let entry = path.and_then(|path| self.folder.tree.entries().iter().find(|entry| &entry.path == path));
        let mut commands = match entry {
            None => vec![Some(Add), Some(RemoveAll)],
            Some(entry) if self.folder.tree.roots().any(|root| root == entry.path.as_path()) => vec![
                Some(Remove), None, Some(CopyPath), Some(Find), None, Some(Reveal), Some(Terminal),
            ],
            Some(entry) if entry.kind == EntryKind::Directory => vec![
                Some(CopyPath), Some(Find), None, Some(Reveal), Some(Terminal),
            ],
            Some(_) => vec![
                Some(Open), None, Some(CopyPath), Some(CopyName), Some(OpenNative), None,
                Some(Reveal), Some(Terminal),
            ],
        };
        commands.extend([None, Some(Choose), Some(Refresh)]);
        if entry.is_some() {
            commands.extend([Some(Add), Some(RemoveAll)]);
        }
        if self.folder_selected_path().is_some() {
            if !entry.is_some_and(|entry| self.folder.tree.roots().any(|root| root == entry.path.as_path())) {
                commands.push(Some(Remove));
            }
            commands.extend([None, Some(NewFile), Some(NewDirectory), Some(Rename), Some(Trash)]);
            if entry.is_some_and(|entry| entry.kind == EntryKind::Directory) {
                commands.extend([Some(OpenNative), Some(CopyName)]);
            } else {
                commands.push(Some(Find));
                if entry.is_none() {
                    commands.extend([Some(OpenNative), Some(Reveal), Some(Terminal), Some(CopyPath), Some(CopyName)]);
                }
            }
        }
        commands.extend([None, Some(FollowLinks)]);
        commands
    }

    fn folder_context_command_enabled(&self, command: FolderCommand) -> bool {
        if self.folder_operations_blocked() { return false; }
        match command {
            FolderCommand::Choose | FolderCommand::Add => !self.folder.picker_pending,
            FolderCommand::Refresh | FolderCommand::RemoveAll => self.folder.tree.root().is_some(),
            FolderCommand::FollowLinks => true,
            _ => self.folder_selected_path().is_some(),
        }
    }

    fn run_folder_context_command(&mut self, command: FolderCommand, window: &mut Window, cx: &mut Context<Self>) {
        if !self.folder_context_command_enabled(command) { return; }
        self.folder.context_menu = None;
        self.folder.tree_focus.focus(window, cx);
        match command {
            FolderCommand::Choose => self.choose_workspace_folder(window, cx),
            FolderCommand::Add => self.add_workspace_folder(window, cx),
            FolderCommand::Remove => self.remove_folder_root(window, cx),
            FolderCommand::RemoveAll => self.remove_all_folder_roots(window, cx),
            FolderCommand::Refresh => self.refresh_workspace_folder(window, cx),
            FolderCommand::Open => {
                if let Some(path) = self.folder_selected_path() { self.activate_folder_entry(path, window, cx); }
            }
            FolderCommand::OpenNative => self.folder_native_command(FolderShellCommand::Open, window, cx),
            FolderCommand::Reveal => self.folder_native_command(FolderShellCommand::Reveal, window, cx),
            FolderCommand::Terminal => self.open_system_terminal(window, cx),
            FolderCommand::Find => self.find_in_selected_folder(window, cx),
            FolderCommand::CopyPath => {
                if let Some(path) = self.folder_selected_path() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.to_string_lossy().into_owned()));
                }
            }
            FolderCommand::CopyName => {
                if let Some(name) = self.folder_selected_path().and_then(|path|
                    path.file_name().map(|name| name.to_string_lossy().into_owned())) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(name));
                }
            }
            FolderCommand::NewFile => self.begin_folder_edit(FolderEditKind::NewFile, window, cx),
            FolderCommand::NewDirectory => self.begin_folder_edit(FolderEditKind::NewDirectory, window, cx),
            FolderCommand::Rename => self.begin_folder_edit(FolderEditKind::Rename, window, cx),
            FolderCommand::Trash => {
                if let Some(path) = self.folder_selected_path() {
                    self.run_folder_mutation(FolderMutation::Trash { path }, window, cx);
                }
            }
            FolderCommand::FollowLinks => {
                self.folder.tree.set_follow_links(!self.folder.tree.follow_links());
                self.scan_workspace_folder(window, cx);
            }
        }
        cx.notify();
    }

    fn render_folder_horizontal_scrollbar(&self, cx: &Context<Self>) -> AnyElement {
        let scroll = self.folder.scroll.0.borrow().base_handle.clone();
        let paint_scroll = scroll.clone();
        let key_scroll = scroll.clone();
        let bounds = self.folder.horizontal_scroll_bounds.clone();
        let paint_bounds = bounds.clone();
        let dark = self.dark;
        let mut bar = div().id("folder-horizontal-scrollbar")
            .debug_selector(|| "folder-horizontal-scrollbar".into())
            .role(Role::ScrollBar).aria_orientation(gpui::accesskit::Orientation::Horizontal)
            .track_focus(&self.folder.horizontal_scroll_focus).tab_index(-1)
            .absolute().left(px(0.)).right(px(0.)).bottom(px(0.)).h(px(17.))
            .bg(rgb(if dark { 0x282828 } else { 0xf0f0f0 }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                view.folder.horizontal_scroll_focus.focus(window, cx);
                let bounds = bounds.get();
                let length = f32::from(bounds.size.width);
                let position = f32::from(event.position.x - bounds.left());
                let viewport = f32::from(scroll.bounds().size.width);
                let maximum = f32::from(scroll.max_offset().x).max(0.);
                let track = (length - 34.).max(0.);
                let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
                let offset = -f32::from(scroll.offset().x);
                let left = 17. + if maximum > 0. { offset / maximum * (track - thumb) } else { 0. };
                let delta = if position < 17. { -19. } else if position > length - 17. { 19. }
                    else if position < left { -viewport } else if position > left + thumb { viewport }
                    else { view.folder.horizontal_scroll_grab.set(Some(position - left)); 0. };
                scroll.set_offset(gpui::point(px(-(offset + delta).clamp(0., maximum)), scroll.offset().y));
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(move |_, event: &gpui::KeyDownEvent, _, cx| {
                let viewport = f32::from(key_scroll.bounds().size.width);
                let maximum = f32::from(key_scroll.max_offset().x).max(0.);
                let offset = -f32::from(key_scroll.offset().x);
                let next = match event.keystroke.key.as_str() {
                    "left" => offset - 19., "right" => offset + 19.,
                    "pageup" => offset - viewport, "pagedown" => offset + viewport,
                    "home" => 0., "end" => maximum, _ => return,
                };
                key_scroll.set_offset(gpui::point(px(-next.clamp(0., maximum)), key_scroll.offset().y));
                cx.stop_propagation();
                cx.notify();
            }))
            .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                paint_bounds.set(bounds);
                let viewport = f32::from(paint_scroll.bounds().size.width);
                let maximum = f32::from(paint_scroll.max_offset().x).max(0.);
                let track = (f32::from(bounds.size.width) - 34.).max(0.);
                if maximum > 0. && track > 0. {
                    let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
                    let offset = -f32::from(paint_scroll.offset().x);
                    let left = 17. + offset / maximum * (track - thumb);
                    let thumb_bounds = Bounds::new(gpui::point(bounds.left() + px(left), bounds.top() + px(2.)), size(px(thumb), px(13.)));
                    window.paint_quad(gpui::fill(thumb_bounds, rgb(if dark { 0x666666 } else { 0xcdcdcd })));
                }
            }).size_full());
        for (end, label) in [(false, "‹"), (true, "›")] {
            bar = bar.child(div().absolute().top(px(0.)).size(px(17.))
                .when(end, |arrow| arrow.right(px(0.)))
                .when(!end, |arrow| arrow.left(px(0.)))
                .text_center().line_height(px(17.))
                .text_color(rgb(if dark { 0xb0b0b0 } else { 0x606060 })).child(label));
        }
        bar.into_any_element()
    }

    fn move_folder_horizontal_scrollbar(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.folder.horizontal_scroll_grab.set(None);
            return;
        }
        let Some(grab) = self.folder.horizontal_scroll_grab.get() else { return; };
        let scroll = self.folder.scroll.0.borrow().base_handle.clone();
        let bounds = self.folder.horizontal_scroll_bounds.get();
        let viewport = f32::from(scroll.bounds().size.width);
        let maximum = f32::from(scroll.max_offset().x).max(0.);
        let track = (f32::from(bounds.size.width) - 34.).max(0.);
        let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
        if track > thumb {
            let position = f32::from(event.position.x - bounds.left());
            let next = px(-maximum * ((position - 17. - grab) / (track - thumb)).clamp(0., 1.));
            scroll.set_offset(gpui::point(next, scroll.offset().y));
            cx.notify();
        }
    }

    pub(super) fn render_folder_workspace(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.folder.tree.visible() {
            return None;
        }
        let dark = self.dark;
        let background = rgb(if dark { 0x282828 } else { 0xf6f6f6 });
        let button = |id: &'static str, label: &'static str, icon: &'static str| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .cursor_pointer()
                .size(px(16.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .hover(|button| button.bg(rgb(if dark { 0x404040 } else { 0xe5e5e5 })))
                .tooltip(move |_, cx| icon_tooltip::view(label, dark, cx))
                .child(gpui::img(folder_image(icon, if dark { "dark" } else { "standard" })).size(px(16.)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let mut errors = self.folder.tree.errors().to_vec();
        if let Some(message) = &self.folder.message {
            errors.insert(0, message.clone());
        }
        let content_width = self.folder.content_width(cx);
        let scroll = self.folder.scroll.0.borrow().base_handle.clone();
        let horizontal = content_width > scroll.bounds().size.width;
        let layout_scroll = scroll.clone();
        let owner = cx.entity();
        Some(
            div()
                .id("folder-workspace-panel")
                .debug_selector(|| "folder-workspace-panel".into())
                .role(Role::Group)
                .aria_label("Folder as Workspace")
                .on_drop(cx.listener(|view, paths: &gpui::ExternalPaths, window, cx| {
                    view.preferences_drop_paths(paths.paths().to_vec(), window, cx);
                    cx.stop_propagation();
                }))
                .w_full()
                .flex_shrink_0()
                .h_full()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .bg(background)
                .text_color(rgb(if dark { 0xd4d4d4 } else { 0x202020 }))
                .text_size(px(12.))
                .relative()
                .font_family(dialog_font::font_family())
                .child(
                    div()
                        .id("folder-workspace-toolbar")
                        .role(Role::Toolbar)
                        .aria_label("Folder as Workspace toolbar")
                        .h(px(16.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_end()
                        .pr(px(8.))
                        .child(
                            button("folder-workspace-expand-all", "Unfold all", "expand")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.run_folder_tree_command(FolderTreeCommand::ExpandAll, window, cx);
                                    cx.stop_propagation();
                                })),
                        )
                        .child(button("folder-workspace-fold-all", "Fold all", "fold").on_click(
                            cx.listener(|view, _, window, cx| {
                                view.run_folder_tree_command(FolderTreeCommand::FoldAll, window, cx);
                                cx.stop_propagation();
                            }),
                        ))
                        .child(button("folder-workspace-locate", "Locate current file", "locate").on_click(
                            cx.listener(|view, _, window, cx| {
                                view.run_folder_tree_command(FolderTreeCommand::Locate, window, cx);
                                cx.stop_propagation();
                            }),
                        )),
                )
                .child(div().h(px(4.)).flex_shrink_0())
                .child(
                    div()
                        .on_children_prepainted(move |_, window, _| {
                            if horizontal != (content_width > layout_scroll.bounds().size.width) {
                                let owner = owner.clone();
                                window.on_next_frame(move |_, cx| owner.update(cx, |_, cx| cx.notify()));
                            }
                        })
                        .id("folder-workspace-tree")
                        .debug_selector(|| "folder-workspace-tree".into())
                        .role(Role::Tree)
                        .aria_label("Filesystem tree; double-click or press Enter to open")
                        .track_focus(&self.folder.tree_focus)
                        .tab_index(0)
                        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                view.folder.context_menu = None;
                                cx.notify();
                                cx.stop_propagation();
                            } else if event.keystroke.key == "enter" {
                                if !view.folder_operations_blocked()
                                    && let Some(path) = view.folder.selected.clone()
                                {
                                    view.activate_folder_entry(path, window, cx);
                                }
                                cx.stop_propagation();
                            }
                        }))
                        .flex_1()
                        .min_h(px(0.))
                        .min_w(px(0.))
                        .relative()
                        .bg(rgb(if dark { 0x202020 } else { 0xffffff }))
                        .border_1()
                        .border_color(rgb(if dark { 0x606060 } else { 0x808080 }))
                        .overflow_hidden()
                        .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
                            view.move_folder_horizontal_scrollbar(event, cx);
                        }))
                        .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| {
                            view.folder.horizontal_scroll_grab.set(None);
                        }))
                        .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, _| {
                            view.folder.horizontal_scroll_grab.set(None);
                        }))
                        .on_mouse_down(MouseButton::Right, cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                            view.folder.context_menu = Some((event.position, None));
                            view.folder.context_selected = 0;
                            view.folder.context_focus.focus(window, cx);
                            cx.notify();
                            cx.stop_propagation();
                        }))
                        .child(
                            div().absolute().inset_0().bottom(px(if horizontal { 17. } else { 0. })).child(
                            gpui::uniform_list(
                                "folder-workspace-items",
                                self.folder.tree.entries().len(),
                                cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                                    range
                                        .filter_map(|index| {
                                            view.folder
                                                .tree
                                                .entries()
                                                .get(index)
                                                .map(|entry| (index, entry))
                                        })
                                        .map(|(index, entry)| {
                                            let path = entry.path.clone();
                                            let select_path = path.clone();
                                            let expand_path = path.clone();
                                            let name = path
                                                .file_name()
                                                .map(|name| name.to_string_lossy().into_owned())
                                                .unwrap_or_else(|| path.display().to_string());
                                            let marker = match entry.kind {
                                                EntryKind::Directory
                                                    if view.folder.tree.is_expanded(&path) =>
                                                {
                                                    "▾"
                                                }
                                                EntryKind::Directory => "▸",
                                                _ => "",
                                            };
                                            let label = format!("{marker} {name}");
                                            let icon = if entry.kind == EntryKind::Directory {
                                                match (entry.depth == 0, view.folder.tree.is_expanded(&path)) {
                                                    (true, true) => "root-open",
                                                    (true, false) => "root-close",
                                                    (false, true) => "folder-open",
                                                    (false, false) => "folder-close",
                                                }
                                            } else {
                                                "file"
                                            };
                                            let icon_style = if view.pref_string("ToolBar.iconSet", if dark { "small" } else { "standard" }) == "standard" {
                                                "standard"
                                            } else if view.dark { "dark" } else { "light" };
                                            let tooltip = path.display().to_string();
                                            div()
                                                .id(("folder-entry", index))
                                                .debug_selector(move || {
                                                    format!("folder-entry-{index}")
                                                })
                                                .role(Role::TreeItem)
                                                .aria_label(label.clone())
                                                .aria_selected(view.folder.selected.as_ref() == Some(&path))
                                                .h(px(18.))
                                                .min_w(content_width)
                                                .flex_shrink_0()
                                                .pl(px(entry.depth as f32 * 19.))
                                                .pr(px(2.))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .flex()
                                                .items_center()
                                                .cursor_pointer()
                                                .tooltip(move |_, cx| icon_tooltip::view(tooltip.clone(), dark, cx))
                                                .child(div().id(("folder-expander", index)).w(px(19.)).h_full().flex_shrink_0()
                                                    .flex().items_center().justify_center().child(marker)
                                                    .when(entry.kind == EntryKind::Directory, |expander| {
                                                        expander.on_click(cx.listener(move |view, _, window, cx| {
                                                            if !view.folder_operations_blocked() {
                                                                view.activate_folder_entry(expand_path.clone(), window, cx);
                                                            }
                                                            cx.stop_propagation();
                                                        }))
                                                    }))
                                                .child(gpui::img(folder_image(icon, icon_style)).size(px(16.)).mr(px(2.)).flex_shrink_0())
                                                .child(div().px(px(2.)).h_full().flex_shrink_0().flex().items_center()
                                                    .when(view.folder.selected.as_ref() == Some(&path), |label|
                                                        label.bg(rgb(if dark { 0x404b5a } else { 0xcce8ff })))
                                                    .hover(|label| label.bg(rgb(if dark { 0x404040 } else { 0xe0e8f4 })))
                                                    .child(name))
                                                .on_mouse_down(MouseButton::Left, cx.listener(|view, _, window, cx| {
                                                    if !view.folder_operations_blocked() {
                                                        view.folder.tree_focus.focus(window, cx);
                                                    }
                                                    cx.stop_propagation();
                                                }))
                                                .on_mouse_down(
                                                    MouseButton::Right,
                                                    cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                                                        view.folder.selected =
                                                            Some(select_path.clone());
                                                        view.folder.context_menu = Some((event.position, Some(select_path.clone())));
                                                        view.folder.context_selected = 0;
                                                        view.folder.context_focus.focus(window, cx);
                                                        cx.notify();
                                                        cx.stop_propagation();
                                                    }),
                                                )
                                                .on_click(cx.listener(
                                                    move |view, event: &gpui::ClickEvent, window, cx| {
                                                        if !view.folder_operations_blocked() && view.folder.pending_root.is_none() {
                                                            view.folder.selected = Some(path.clone());
                                                            view.folder.context_menu = None;
                                                            if event.click_count() == 2 {
                                                                view.activate_folder_entry(path.clone(), window, cx);
                                                            }
                                                            cx.notify();
                                                        }
                                                        cx.stop_propagation();
                                                    },
                                                ))
                                        })
                                        .collect()
                                }),
                            )
                            .with_horizontal_sizing_behavior(
                                gpui::ListHorizontalSizingBehavior::Unconstrained,
                            )
                            .track_scroll(&self.folder.scroll)
                            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                            .size_full(),
                        ))
                        .when(horizontal, |tree| tree.child(self.render_folder_horizontal_scrollbar(cx))),
                )
                .when(self.folder.picker_pending, |panel| {
                    panel.child(div().px(px(6.)).flex_shrink_0().child("Choosing folder…"))
                })
                .when(self.folder.scans.is_busy(), |panel| {
                    panel.child(div().px(px(6.)).flex_shrink_0().child("Loading folder…"))
                })
                .child(
                    div()
                        .id("folder-workspace-errors")
                        .role(Role::Group)
                        .aria_label("Folder errors")
                        .max_h(px(100.))
                        .flex_shrink_0()
                        .overflow_y_scroll()
                        .children(errors.into_iter().map(|error| {
                            div()
                                .px(px(6.))
                                .text_color(rgb(if dark { 0xffb0a0 } else { 0xa02020 }))
                                .child(error)
                        })),
                )
                .children(self.render_folder_edit(cx))
                .children(self.render_folder_context(cx))
                .into_any_element(),
        )
    }

    fn render_folder_edit(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.folder.command_input.as_ref()?;
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .cursor_pointer()
                .px(px(4.))
                .child(label)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        Some(div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(3.))
            .child(input.clone())
            .child(
                div()
                    .flex()
                    .child(button("folder-command-apply", "Apply").on_click(
                        cx.listener(|view, _, window, cx| view.apply_folder_edit(window, cx)),
                    ))
                    .child(
                        button("folder-command-cancel", "Cancel").on_click(cx.listener(
                            |view, _, _, cx| {
                                view.folder.command_input = None;
                                view.folder.command_kind = None;
                                cx.notify();
                            },
                        )),
                    ),
            )
            .into_any_element())
    }

    fn render_folder_context(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (position, _) = self.folder.context_menu.as_ref()?;
        let dark = self.dark;
        let commands = self.folder_context_commands();
        let chinese = self.native_locale() == "chineseSimplified.xml";
        let rows = commands.into_iter().enumerate().map(|(index, command)| {
            let Some(command) = command else {
                return div().h(px(1.)).my(px(3.)).mx(px(4.))
                    .bg(rgb(if dark { 0x606060 } else { 0xd0d0d0 })).into_any_element();
            };
            let enabled = self.folder_context_command_enabled(command);
            let label = command.label(self.folder.tree.follow_links(), chinese);
            div().id(command.id()).debug_selector(move || command.id().into())
                .role(Role::MenuItem).aria_label(label).aria_selected(index == self.folder.context_selected)
                .a11y_synthetic_children(move |builder| {
                    if !enabled { builder.parent_node().set_disabled(); }
                })
                .h(px(22.)).px(px(24.)).flex().items_center().whitespace_nowrap()
                .text_color(rgb(if !enabled { 0x808080 } else if dark { 0xd4d4d4 } else { 0x202020 }))
                .when(enabled && index == self.folder.context_selected, |row|
                    row.bg(rgb(if dark { 0x404b5a } else { 0xcce8ff })))
                .on_mouse_move(cx.listener(move |view, _, _, cx| {
                    if enabled && view.folder.context_selected != index {
                        view.folder.context_selected = index;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.run_folder_context_command(command, window, cx);
                    cx.stop_propagation();
                }))
                .child(label).into_any_element()
        });
        Some(gpui::deferred(gpui::anchored().position(*position).snap_to_window().child(
            div().id("folder-workspace-context-menu")
                .debug_selector(|| "folder-workspace-context-menu".into())
                .role(Role::Menu).aria_label("Folder as Workspace")
                .track_focus(&self.folder.context_focus)
                .font_family(dialog_font::font_family()).text_size(px(12.))
                .py(px(2.)).max_h(px(440.)).overflow_y_scroll()
                .flex().flex_col().border_1().border_color(rgb(0x808080))
                .bg(rgb(if dark { 0x282828 } else { 0xf0f0f0 })).occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                    view.folder.context_menu = None;
                    cx.notify();
                }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            view.folder.context_menu = None;
                            view.folder.tree_focus.focus(window, cx);
                        }
                        "up" | "down" => {
                            let commands = view.folder_context_commands();
                            for _ in 0..commands.len() {
                                view.folder.context_selected = if event.keystroke.key == "up" {
                                    (view.folder.context_selected + commands.len() - 1) % commands.len()
                                } else { (view.folder.context_selected + 1) % commands.len() };
                                if commands[view.folder.context_selected]
                                    .is_some_and(|command| view.folder_context_command_enabled(command)) { break; }
                            }
                        }
                        "enter" | "space" => {
                            if let Some(Some(command)) = view.folder_context_commands().get(view.folder.context_selected) {
                                view.run_folder_context_command(*command, window, cx);
                            }
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify();
                }))
                .children(rows),
        )).into_any_element())
    }
}
