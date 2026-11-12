use super::*;
use crate::folder_workspace::{
    EntryKind, FolderFile, FolderMutation, FolderScanController, FolderScanJob, FolderShellCommand,
    FolderWatch, FolderWorkspace, existing_buffer,
};

pub(super) struct FolderState {
    pub(super) tree: FolderWorkspace,
    picker_pending: bool,
    message: Option<String>,
    scroll: gpui::UniformListScrollHandle,
    scans: FolderScanController,
    scan_needed: bool,
    pending_root: Option<PathBuf>,
    pending_add: bool,
    scan_task: Option<gpui::Task<()>>,
    watch_task: Option<gpui::Task<()>>,
    watch_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    operation_task: Option<gpui::Task<()>>,
    selected: Option<PathBuf>,
    command_input: Option<gpui::Entity<text_field::TextField>>,
    command_kind: Option<FolderEditKind>,
}

#[derive(Clone, Copy)]
enum FolderEditKind {
    NewFile,
    NewDirectory,
    Rename,
}

impl Drop for FolderState {
    fn drop(&mut self) {
        self.watch_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;

    #[gpui::test]
    fn watch_refreshes_actual_filesystem_and_hidden_panel_does_not_restart(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path().canonicalize().expect("root");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.folder.tree.set_root(&root).expect("root");
            view.show_panel(crate::panel_layout::PanelId::Folder, window, cx);
        });
        std::fs::write(root.join("startup.txt"), "between scan and watch").expect("fixture file");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.folder
                    .tree
                    .entries()
                    .iter()
                    .any(|entry| entry.path.ends_with("startup.txt"))
            )
        });
        std::fs::write(root.join("event.txt"), "filesystem event").expect("fixture file");
        cx.cx
            .dispatcher
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.folder
                    .tree
                    .entries()
                    .iter()
                    .any(|entry| entry.path.ends_with("event.txt"))
            )
        });
        view.update_in(cx, |view, window, cx| {
            view.close_folder_workspace(window, cx)
        });
        std::fs::write(root.join("hidden.txt"), "hidden").expect("fixture file");
        cx.cx
            .dispatcher
            .advance_clock(std::time::Duration::from_secs(3));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.folder.tree.visible());
            assert!(view.folder.watch_task.is_none());
            assert!(
                !view
                    .folder
                    .tree
                    .entries()
                    .iter()
                    .any(|entry| entry.path.ends_with("hidden.txt"))
            );
        });
        view.update_in(cx, |view, window, cx| {
            view.toggle_folder_workspace(window, cx)
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.folder
                    .tree
                    .entries()
                    .iter()
                    .any(|entry| entry.path.ends_with("hidden.txt"))
            )
        });
    }

    #[gpui::test]
    fn newest_scan_keeps_both_expansions_and_does_not_reopen_closed_panel(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path().canonicalize().expect("canonical fixture");
        for directory in ["a", "b"] {
            std::fs::create_dir(root.join(directory)).expect("directory");
            std::fs::write(root.join(directory).join("inside.txt"), "keep").expect("file");
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.folder.tree.set_root(&root).expect("root");
            view.activate_folder_entry(root.join("a"), window, cx);
            view.activate_folder_entry(root.join("b"), window, cx);
            view.refresh_workspace_folder(window, cx);
            view.close_folder_workspace(window, cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.folder.scans.is_busy());
            assert!(
                !view.folder_workspace_visible(),
                "completion must not reopen a closed panel"
            );
            assert_eq!(
                view.folder.tree.entries().len(),
                3,
                "hiding cancels unpublished scans"
            );
            for directory in ["a", "b"] {
                assert!(view.folder.tree.is_expanded(&root.join(directory)));
            }
            assert!(view.folder.message.is_none());
        });
        view.update_in(cx, |view, window, cx| {
            view.toggle_folder_workspace(window, cx)
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.folder.tree.entries().len(), 5);
            for directory in ["a", "b"] {
                assert!(
                    view.folder
                        .tree
                        .entries()
                        .iter()
                        .any(|entry| entry.path == root.join(directory).join("inside.txt"))
                );
            }
        });
    }

    #[gpui::test]
    fn folder_panel_reopens_same_tree_and_reuses_dirty_document(cx: &mut gpui::TestAppContext) {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path().canonicalize().expect("canonical fixture");
        std::fs::create_dir(root.join("child")).expect("directory");
        let path = root.join("child/document.txt");
        std::fs::write(&path, "original").expect("file");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.folder.tree.set_root(&root).expect("root");
            view.folder
                .tree
                .toggle_directory(&root.join("child"))
                .expect("expand");
            let id = FolderFile::load(&path)
                .expect("load")
                .apply(&mut view.workspace)
                .expect("open");
            view.editor_mut().insert_text("unsaved ").expect("edit");
            let edited = view.editor().committed_text();
            let selection = view.editor().document().selection();
            view.workspace.new_buffer().expect("other document");
            let count = view.workspace.buffers().len();
            view.activate_folder_entry(path.clone(), window, cx);
            assert_eq!(view.workspace.active().id(), id);
            assert_eq!(view.workspace.buffers().len(), count);
            assert_eq!(view.editor().committed_text(), edited);
            assert_eq!(view.editor().document().selection(), selection);
            assert!(view.render_folder_workspace(cx).is_some());
            view.close_folder_workspace(window, cx);
            assert!(!view.folder_workspace_visible());
            assert!(view.render_folder_workspace(cx).is_none());
            view.toggle_folder_workspace(window, cx);
            assert!(view.folder_workspace_visible());
            assert_eq!(view.folder.tree.root(), Some(root.as_path()));
            assert!(view.folder.tree.is_expanded(&root.join("child")));
            assert!(
                !view.folder.picker_pending,
                "reopening does not start another picker"
            );
            assert!(view.editor_mut().undo());
            assert_eq!(view.editor().committed_text(), "original");
        });
    }
}

impl FolderState {
    pub(super) fn new() -> Self {
        Self {
            tree: FolderWorkspace::new(),
            picker_pending: false,
            message: None,
            scroll: gpui::UniformListScrollHandle::new(),
            scans: FolderScanController::new(),
            scan_needed: false,
            pending_root: None,
            pending_add: false,
            scan_task: None,
            watch_task: None,
            watch_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            operation_task: None,
            selected: None,
            command_input: None,
            command_kind: None,
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
                self.message = None;
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}

impl MatrixWindow {
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
        let tree = FolderWorkspace::restore(value)?;
        self.pause_folder_watch();
        self.folder.tree = tree;
        self.folder.scan_needed = self.folder.tree.root().is_some();
        self.folder.selected = None;
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
        if self.folder.tree.visible() {
            self.close_folder_workspace(window, cx);
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.show_panel(crate::panel_layout::PanelId::Folder, window, cx);
        cx.notify();
        if self.folder.scan_needed {
            self.scan_workspace_folder(window, cx);
        } else if self.folder.tree.root().is_none() {
            self.choose_workspace_folder(window, cx);
        }
    }

    pub(super) fn close_folder_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.close_folder_workspace(window, cx)
        }) {
            return;
        }
        self.hide_panel(crate::panel_layout::PanelId::Folder, window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn choose_workspace_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.choose_workspace_folder_mode(false, window, cx);
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
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        self.file_busy = true;
        let load_path = path.clone();
        let load = cx
            .background_executor()
            .spawn(async move { FolderFile::load(&load_path) });
        cx.notify();
        self.folder.operation_task = Some(cx.spawn_in(window, async move |view, cx| {
            let loaded = load.await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                if view.folder_operations_blocked() {
                    view.folder.message =
                        Some("File open stopped by another document operation; click again".into());
                    cx.notify();
                    return;
                }
                match loaded {
                    Ok(file) => {
                        if !view.cancel_view_composition(window, cx) {
                            return;
                        }
                        match file.apply(&mut view.workspace) {
                            Ok(_) => {
                                view.folder.message = None;
                                view.reset_active_view(window, cx);
                                window.activate_window();
                            }
                            Err(error) => {
                                view.folder.message = Some(format!("{}: {error}", path.display()))
                            }
                        }
                    }
                    Err(error) => {
                        view.folder.message = Some(format!("{}: {error}", path.display()))
                    }
                }
                cx.notify();
            }) {
                eprintln!("folder file completion lost its document: {error}");
            }
        }));
    }

    pub(super) fn render_folder_workspace(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.folder.tree.visible() {
            return None;
        }
        let dark = self.dark;
        let background = rgb(if dark { 0x282828 } else { 0xf6f6f6 });
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .cursor_pointer()
                .px(px(5.))
                .child(label)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let root = self
            .folder
            .tree
            .root()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "No folder selected".into());
        let mut errors = self.folder.tree.errors().to_vec();
        if let Some(message) = &self.folder.message {
            errors.insert(0, message.clone());
        }
        let deepest_row = self
            .folder
            .tree
            .entries()
            .iter()
            .enumerate()
            .max_by_key(|(_, entry)| entry.depth)
            .map(|(index, _)| index);
        Some(
            div()
                .id("folder-workspace-panel")
                .debug_selector(|| "folder-workspace-panel".into())
                .role(Role::Group)
                .aria_label("Folder as Workspace")
                .w_full()
                .flex_shrink_0()
                .h_full()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .bg(background)
                .text_color(rgb(if dark { 0xd4d4d4 } else { 0x202020 }))
                .text_size(px(12.))
                .border_r_1()
                .border_color(rgb(0x808080))
                .child(
                    div()
                        .h(px(26.))
                        .flex_shrink_0()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("Folder as Workspace")
                        .child(
                            button("folder-workspace-close", "×")
                                .aria_label("Close Folder as Workspace")
                                .tooltip(move |_, cx| {
                                    icon_tooltip::view("Close Folder as Workspace", dark, cx)
                                })
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_folder_workspace(window, cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .gap(px(4.))
                        .py(px(4.))
                        .child(
                            button("folder-workspace-choose", "Choose Folder…").on_click(
                                cx.listener(|view, _, window, cx| {
                                    view.choose_workspace_folder(window, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                        .child(button("folder-workspace-refresh", "Refresh").on_click(
                            cx.listener(|view, _, window, cx| {
                                view.refresh_workspace_folder(window, cx);
                                cx.stop_propagation();
                            }),
                        )),
                )
                .child(
                    div()
                        .px(px(6.))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(root),
                )
                .child(self.render_folder_commands(cx))
                .when(self.folder.picker_pending, |panel| {
                    panel.child(div().px(px(6.)).child("Choosing folder…"))
                })
                .when(self.folder.scans.is_busy(), |panel| {
                    panel.child(div().px(px(6.)).child("Loading folder…"))
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
                .child(
                    div()
                        .id("folder-workspace-tree")
                        .debug_selector(|| "folder-workspace-tree".into())
                        .role(Role::Group)
                        .aria_label(
                            "Filesystem tree; click directories to expand and files to open",
                        )
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_hidden()
                        .child(
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
                                                EntryKind::File => "·",
                                                EntryKind::Symlink => "↗",
                                                EntryKind::Other => "?",
                                            };
                                            let label = format!("{marker} {name}");
                                            div()
                                                .id(("folder-entry", index))
                                                .debug_selector(move || {
                                                    format!("folder-entry-{index}")
                                                })
                                                .role(Role::Button)
                                                .aria_label(label.clone())
                                                .h(px(23.))
                                                .flex_shrink_0()
                                                .pl(px(6. + entry.depth as f32 * 14.))
                                                .pr(px(6.))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .cursor_pointer()
                                                .when(
                                                    view.folder.selected.as_ref() == Some(&path),
                                                    |row| {
                                                        row.bg(rgb(if dark {
                                                            0x404b5a
                                                        } else {
                                                            0xcce8ff
                                                        }))
                                                    },
                                                )
                                                .hover(|row| {
                                                    row.bg(rgb(if dark {
                                                        0x404040
                                                    } else {
                                                        0xe0e8f4
                                                    }))
                                                })
                                                .child(label)
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_mouse_down(
                                                    MouseButton::Right,
                                                    cx.listener(move |view, _, _, cx| {
                                                        view.folder.selected =
                                                            Some(select_path.clone());
                                                        cx.notify();
                                                        cx.stop_propagation();
                                                    }),
                                                )
                                                .on_click(cx.listener(
                                                    move |view, _, window, cx| {
                                                        view.activate_folder_entry(
                                                            path.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                        cx.stop_propagation();
                                                    },
                                                ))
                                        })
                                        .collect()
                                }),
                            )
                            .with_width_from_item(deepest_row)
                            .with_horizontal_sizing_behavior(
                                gpui::ListHorizontalSizingBehavior::Unconstrained,
                            )
                            .track_scroll(&self.folder.scroll)
                            .h_full(),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_folder_commands(&self, cx: &mut Context<Self>) -> AnyElement {
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
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(3.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .child(button("folder-add-root", "Add Root…").on_click(cx.listener(
                        |view, _, window, cx| view.choose_workspace_folder_mode(true, window, cx),
                    )))
                    .child(button("folder-new-file", "New File…").on_click(cx.listener(
                        |view, _, window, cx| {
                            view.begin_folder_edit(FolderEditKind::NewFile, window, cx)
                        },
                    )))
                    .child(
                        button("folder-new-directory", "New Folder…").on_click(cx.listener(
                            |view, _, window, cx| {
                                view.begin_folder_edit(FolderEditKind::NewDirectory, window, cx)
                            },
                        )),
                    )
                    .child(button("folder-rename", "Rename…").on_click(cx.listener(
                        |view, _, window, cx| {
                            view.begin_folder_edit(FolderEditKind::Rename, window, cx)
                        },
                    )))
                    .child(
                        button("folder-trash", "Move to Trash").on_click(cx.listener(
                            |view, _, window, cx| {
                                if let Some(path) = view.folder_selected_path() {
                                    view.run_folder_mutation(
                                        FolderMutation::Trash { path },
                                        window,
                                        cx,
                                    );
                                }
                            },
                        )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .child(
                        button("folder-open-native", "Open Externally").on_click(cx.listener(
                            |view, _, window, cx| {
                                view.folder_native_command(FolderShellCommand::Open, window, cx)
                            },
                        )),
                    )
                    .child(
                        button("folder-reveal", "Reveal in Finder").on_click(cx.listener(
                            |view, _, window, cx| {
                                view.folder_native_command(FolderShellCommand::Reveal, window, cx)
                            },
                        )),
                    )
                    .child(
                        button("folder-terminal", "Terminal Here").on_click(cx.listener(
                            |view, _, window, cx| {
                                view.folder_native_command(FolderShellCommand::Terminal, window, cx)
                            },
                        )),
                    )
                    .child(button("folder-find-in-files", "Find in Files…").on_click(
                        cx.listener(|view, _, window, cx| view.find_in_selected_folder(window, cx)),
                    ))
                    .child(
                        button("folder-copy-path", "Copy Path").on_click(cx.listener(
                            |view, _, _, cx| {
                                if let Some(path) = view.folder_selected_path() {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        path.to_string_lossy().into_owned(),
                                    ));
                                }
                            },
                        )),
                    )
                    .child(
                        button("folder-copy-name", "Copy Name").on_click(cx.listener(
                            |view, _, _, cx| {
                                if let Some(name) = view.folder_selected_path().and_then(|path| {
                                    path.file_name()
                                        .map(|name| name.to_string_lossy().into_owned())
                                }) {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(name));
                                }
                            },
                        )),
                    )
                    .child(button("folder-remove-root", "Remove Root").on_click(
                        cx.listener(|view, _, window, cx| view.remove_folder_root(window, cx)),
                    ))
                    .child(
                        button("folder-remove-all-roots", "Remove All Roots").on_click(
                            cx.listener(|view, _, window, cx| {
                                view.remove_all_folder_roots(window, cx)
                            }),
                        ),
                    ),
            )
            .child(
                button(
                    "folder-follow-links",
                    if self.folder.tree.follow_links() {
                        "☑ Follow directory links"
                    } else {
                        "☐ Follow directory links"
                    },
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.folder
                        .tree
                        .set_follow_links(!view.folder.tree.follow_links());
                    view.scan_workspace_folder(window, cx);
                })),
            )
            .when_some(self.folder.command_input.as_ref(), |row, input| {
                row.child(input.clone()).child(
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
            })
            .into_any_element()
    }
}
