use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::files::DiskFile;
use crate::workspace::{BufferId, Workspace, WorkspaceError};

pub fn relocate_open_paths(workspace: &mut Workspace, from: &Path, to: &Path) -> usize {
    let mut relocated = 0;
    for buffer in workspace.buffers_mut() {
        if let Some(file) = &mut buffer.disk_file
            && let Ok(suffix) = file.path().strip_prefix(from)
        {
            let path = if suffix.as_os_str().is_empty() {
                to.to_path_buf()
            } else {
                to.join(suffix)
            };
            file.relocate_path(path);
            relocated += 1;
        }
    }
    relocated
}

#[derive(Clone)]
pub enum FolderMutation {
    CreateFile { parent: PathBuf, name: String },
    CreateDirectory { parent: PathBuf, name: String },
    Rename { path: PathBuf, name: String },
    Trash { path: PathBuf },
}

impl FolderMutation {
    pub fn execute(self) -> io::Result<PathBuf> {
        match self {
            Self::CreateFile { parent, name } => {
                let _access = crate::file_access::directory(&parent)?;
                let path = child_path(&parent, &name)?;
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                Ok(path)
            }
            Self::CreateDirectory { parent, name } => {
                let _access = crate::file_access::directory(&parent)?;
                let path = child_path(&parent, &name)?;
                fs::create_dir(&path)?;
                Ok(path)
            }
            Self::Rename { path, name } => {
                let parent = path.parent().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Cannot rename a filesystem root",
                    )
                })?;
                let _access = crate::file_access::directory(parent)?;
                let target = child_path(parent, &name)?;
                match fs::symlink_metadata(&target) {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "Destination already exists",
                        ));
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                rename_without_overwrite(&path, &target)?;
                Ok(target)
            }
            Self::Trash { path } => {
                native_trash(&path)?;
                Ok(path)
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn rename_without_overwrite(from: &Path, to: &Path) -> io::Result<()> {
    let from =
        std::ffi::CString::new(from.as_os_str().as_encoded_bytes()).map_err(io::Error::other)?;
    let to = std::ffi::CString::new(to.as_os_str().as_encoded_bytes()).map_err(io::Error::other)?;
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        fn renameat2(
            from_fd: i32,
            from: *const std::ffi::c_char,
            to_fd: i32,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    #[cfg(target_os = "macos")]
    let result = unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 4) };
    #[cfg(target_os = "linux")]
    let result = unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rename_without_overwrite(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Exclusive native rename is unavailable on this platform",
    ))
}

fn child_path(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
        || name.contains('\0')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter one file or directory name",
        ));
    }
    if !fs::metadata(parent)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "Parent is not a directory",
        ));
    }
    Ok(parent.join(name))
}

#[cfg(all(target_os = "macos", feature = "native-ui"))]
fn native_trash(path: &Path) -> io::Result<()> {
    let path = std::path::absolute(path)?;
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Cannot trash a filesystem root"))?;
    let _access = crate::file_access::directory(parent)?;
    fs::symlink_metadata(&path)?;
    let url = crate::native_app_open::file_url(&path)?;
    objc2_foundation::NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|error| io::Error::other(format!("{} ({}, {})", error.localizedDescription(), error.domain(), error.code())))
}

#[cfg(not(all(target_os = "macos", feature = "native-ui")))]
fn native_trash(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Native Trash requires macOS with native-ui",
    ))
}

#[derive(Clone, Copy)]
pub enum FolderShellCommand {
    Open,
    Reveal,
}

pub fn folder_shell(path: &Path, command: FolderShellCommand) -> io::Result<()> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    {
        match command {
            FolderShellCommand::Open => crate::native_app_open::open_default(path),
            FolderShellCommand::Reveal => crate::native_app_open::reveal(path),
        }
    }
    #[cfg(not(all(target_os = "macos", feature = "native-ui")))]
    {
        let _ = (path, command);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Native folder commands require macOS",
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MetadataStamp {
    length: u64,
    modified: Option<std::time::SystemTime>,
    directory: bool,
    link: bool,
    target: Option<PathBuf>,
}

#[derive(Default)]
pub struct FolderWatch {
    snapshot: Option<BTreeMap<PathBuf, Option<MetadataStamp>>>,
    root: Option<PathBuf>,
    paused: bool,
}

impl FolderWatch {
    pub fn poll(&mut self, tree: &FolderWorkspace) -> io::Result<bool> {
        self.poll_cancellable(tree, &AtomicBool::new(false))
    }

    pub fn poll_cancellable(
        &mut self,
        tree: &FolderWorkspace,
        cancel: &AtomicBool,
    ) -> io::Result<bool> {
        if !tree.visible() {
            self.paused = true;
            return Ok(false);
        }
        let _access = tree.roots()
            .map(crate::file_access::existing)
            .collect::<io::Result<Vec<_>>>()?;
        let mut paths = BTreeSet::new();
        for root in tree.roots() {
            paths.insert(root.to_path_buf());
        }
        for directory in tree
            .entries
            .iter()
            .filter(|entry| {
                entry.kind == EntryKind::Directory && tree.expanded.contains(&entry.path)
            })
            .map(|entry| &entry.path)
        {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let _directory_access = crate::file_access::existing(directory)?;
            if !tree.follow_links
                && fs::symlink_metadata(directory)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                continue;
            }
            paths.insert(directory.clone());
            match fs::read_dir(directory) {
                Ok(children) => {
                    for child in children {
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(false);
                        }
                        paths.insert(child?.path());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let snapshot = paths
            .into_iter()
            .map(|path| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Folder watch cancelled",
                    ));
                }
                let stamp = match fs::symlink_metadata(&path) {
                    Ok(metadata) => Some(MetadataStamp {
                        length: metadata.len(),
                        modified: metadata.modified().ok(),
                        directory: metadata.is_dir(),
                        link: metadata.file_type().is_symlink(),
                        target: if metadata.file_type().is_symlink() {
                            fs::read_link(&path).ok()
                        } else {
                            None
                        },
                    }),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error),
                };
                Ok((path, stamp))
            })
            .collect::<io::Result<BTreeMap<_, _>>>()?;
        let changed = self.paused
            || (self.root.as_deref() == tree.root()
                && self
                    .snapshot
                    .as_ref()
                    .is_some_and(|previous| previous != &snapshot));
        self.root = tree.root.clone();
        self.snapshot = Some(snapshot);
        self.paused = false;
        Ok(changed)
    }
}

pub fn existing_buffer(workspace: &Workspace, path: &Path) -> Option<BufferId> {
    let canonical = crate::file_access::existing(path)
        .and_then(|_access| fs::canonicalize(path)).ok();
    workspace.buffers().iter().find_map(|buffer| {
        let opened = buffer.disk_file.as_ref()?.path();
        (opened == path
            || canonical.as_ref().is_some_and(|target| {
                opened == target || crate::file_access::existing(opened)
                    .and_then(|_access| fs::canonicalize(opened)).is_ok_and(|opened| &opened == target)
            }))
        .then_some(buffer.id())
    })
}

pub fn dropped_folder_files(paths: Vec<PathBuf>) -> io::Result<Vec<PathBuf>> {
    let mut access = Vec::new();
    for path in &paths {
        let guard = crate::file_access::read(path)?;
        if fs::metadata(path)?.is_dir() {
            access.push(crate::file_access::directory(path)?);
        }
        access.push(guard);
    }
    let mut pending = paths;
    let mut files = Vec::new();
    let mut visited = BTreeSet::new();
    while let Some(path) = pending.pop() {
        let _access = crate::file_access::existing(&path)?;
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() { files.push(path); continue; }
        if !metadata.is_dir() { continue; }
        if !visited.insert(fs::canonicalize(&path)?) { continue; }
        let mut children = fs::read_dir(&path)?.map(|entry| entry.map(|entry| entry.path())).collect::<io::Result<Vec<_>>>()?;
        children.sort();
        pending.extend(children.into_iter().rev());
    }
    files.sort();
    files.dedup();
    Ok(files)
}

pub struct FolderFile {
    file: DiskFile,
    text: String,
}

impl FolderFile {
    pub fn load_with_preferences(path: &Path,settings: &serde_json::Value) -> io::Result<Self> {
        let _access = crate::file_access::read(path)?;
        let path = fs::canonicalize(path)?;
        if !fs::metadata(&path)?.is_file() { return Err(io::Error::other("Not a regular file")); }
        let (file,text) = DiskFile::load_with_preferences(&path,settings)?;
        Ok(Self { file,text })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let _access = crate::file_access::read(path)?;
        let path = fs::canonicalize(path)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a regular file",
            ));
        }
        let (file, text) = DiskFile::load_unicode(&path)?;
        Ok(Self { file, text })
    }

    pub fn apply(self, workspace: &mut Workspace) -> Result<BufferId, WorkspaceError> {
        if let Some(id) = existing_buffer(workspace, self.file.path()) {
            workspace.activate(id)?;
            return Ok(id);
        }
        workspace.open_buffer(self.file, &self.text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug)]
pub struct FolderEntry {
    pub path: PathBuf,
    pub depth: usize,
    pub kind: EntryKind,
}

#[derive(Clone, Default)]
pub struct FolderWorkspace {
    root: Option<PathBuf>,
    additional_roots: Vec<PathBuf>,
    visible: bool,
    expanded: BTreeSet<PathBuf>,
    entries: Vec<FolderEntry>,
    errors: Vec<String>,
    follow_links: bool,
}

pub struct FolderScanInput {
    root: Option<PathBuf>,
    additional_roots: Vec<PathBuf>,
    expanded: BTreeSet<PathBuf>,
    selected_root: Option<PathBuf>,
    follow_links: bool,
    add_root: bool,
}

pub struct FolderScanJob {
    input: FolderScanInput,
    cancel: Arc<AtomicBool>,
}

pub struct FolderScanResult {
    cancel: Arc<AtomicBool>,
    result: io::Result<Option<FolderWorkspace>>,
}

pub struct FolderScanUpdate {
    pub result: Option<io::Result<FolderWorkspace>>,
    pub next: Option<FolderScanJob>,
}

#[derive(Default)]
pub struct FolderScanController {
    active: Option<Arc<AtomicBool>>,
    pending: Option<FolderScanInput>,
}

impl FolderScanController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    pub fn request(&mut self, input: FolderScanInput) -> Option<FolderScanJob> {
        if let Some(active) = &self.active {
            active.store(true, Ordering::Relaxed);
            self.pending = Some(input);
            None
        } else {
            Some(self.start(input))
        }
    }

    pub fn cancel(&mut self) {
        self.pending = None;
        if let Some(active) = &self.active {
            active.store(true, Ordering::Relaxed);
        }
    }

    pub fn complete(&mut self, completed: FolderScanResult) -> FolderScanUpdate {
        if !self
            .active
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &completed.cancel))
        {
            return FolderScanUpdate {
                result: None,
                next: None,
            };
        }
        self.active = None;
        let result = if completed.cancel.load(Ordering::Relaxed) {
            None
        } else {
            match completed.result {
                Ok(Some(tree)) => Some(Ok(tree)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        };
        let next = self.pending.take().map(|input| self.start(input));
        FolderScanUpdate { result, next }
    }

    fn start(&mut self, input: FolderScanInput) -> FolderScanJob {
        let cancel = Arc::new(AtomicBool::new(false));
        self.active = Some(cancel.clone());
        FolderScanJob { input, cancel }
    }
}

impl Drop for FolderScanController {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl FolderScanJob {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn run(self) -> FolderScanResult {
        let Self { input, cancel } = self;
        let result = (|| {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let mut tree = FolderWorkspace {
                root: input.root,
                additional_roots: input.additional_roots,
                expanded: input.expanded,
                follow_links: input.follow_links,
                ..FolderWorkspace::default()
            };
            if let Some(root) = input.selected_root {
                let result = if input.add_root {
                    tree.select_add_root(&root)
                } else {
                    tree.select_root(&root)
                };
                result.map_err(|error| {
                    io::Error::new(error.kind(), format!("{}: {error}", root.display()))
                })?;
            }
            if tree.refresh_cancellable(&cancel) {
                Ok(Some(tree))
            } else {
                Ok(None)
            }
        })();
        FolderScanResult { cancel, result }
    }
}

impl FolderWorkspace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn roots(&self) -> impl Iterator<Item = &Path> {
        self.root
            .iter()
            .chain(self.additional_roots.iter())
            .map(PathBuf::as_path)
    }

    pub fn relocate_paths(&mut self, from: &Path, to: &Path) {
        let relocate = |path: &Path| {
            path.strip_prefix(from)
                .map(|suffix| {
                    if suffix.as_os_str().is_empty() {
                        to.to_path_buf()
                    } else {
                        to.join(suffix)
                    }
                })
                .unwrap_or_else(|_| path.to_path_buf())
        };
        self.root = self.root.as_deref().map(relocate);
        self.additional_roots = self
            .additional_roots
            .iter()
            .map(|path| relocate(path))
            .collect();
        self.expanded = self.expanded.iter().map(|path| relocate(path)).collect();
        for entry in &mut self.entries {
            entry.path = relocate(&entry.path);
        }
    }

    pub fn snapshot(&self) -> io::Result<serde_json::Value> {
        let roots: Vec<_> = self.roots().collect();
        let mut value = serde_json::Map::new();
        value.insert("version".into(), 1.into());
        value.insert(
            "roots".into(),
            serde_json::to_value(roots).map_err(io::Error::other)?,
        );
        value.insert(
            "expanded".into(),
            serde_json::to_value(&self.expanded).map_err(io::Error::other)?,
        );
        value.insert("visible".into(), self.visible.into());
        value.insert("follow_links".into(), self.follow_links.into());
        Ok(value.into())
    }

    pub fn restore(value: &serde_json::Value) -> io::Result<Self> {
        let invalid =
            || io::Error::new(io::ErrorKind::InvalidData, "Invalid folder workspace state");
        if value["version"].as_u64() != Some(1) {
            return Err(invalid());
        }
        let roots: Vec<PathBuf> =
            serde_json::from_value(value["roots"].clone()).map_err(io::Error::other)?;
        let expanded: BTreeSet<PathBuf> =
            serde_json::from_value(value["expanded"].clone()).map_err(io::Error::other)?;
        if roots.iter().any(|root| !root.is_absolute())
            || expanded
                .iter()
                .any(|path| !roots.iter().any(|root| path.starts_with(root)))
        {
            return Err(invalid());
        }
        let mut roots = roots.into_iter();
        Ok(Self {
            root: roots.next(),
            additional_roots: roots.collect(),
            expanded,
            visible: value["visible"].as_bool().ok_or_else(invalid)?,
            follow_links: value["follow_links"].as_bool().ok_or_else(invalid)?,
            ..Self::default()
        })
    }

    pub fn add_root(&mut self, path: &Path) -> io::Result<()> {
        self.select_add_root(path)?;
        self.visible = true;
        self.refresh();
        Ok(())
    }

    fn select_add_root(&mut self, path: &Path) -> io::Result<()> {
        let _access = crate::file_access::directory(path)?;
        let path = fs::canonicalize(path)?;
        if !fs::metadata(&path)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "Choose a directory",
            ));
        }
        if self.root.is_none() {
            return self.select_root(&path);
        }
        if !self.roots().any(|root| root == path) {
            self.expanded.insert(path.clone());
            self.additional_roots.push(path);
        }
        Ok(())
    }

    pub fn remove_root(&mut self, path: &Path) -> io::Result<()> {
        let path = crate::file_access::existing(path)
            .and_then(|_access| fs::canonicalize(path)).unwrap_or_else(|_| path.to_path_buf());
        if self.root.as_ref() == Some(&path) {
            self.root = if self.additional_roots.is_empty() {
                None
            } else {
                Some(self.additional_roots.remove(0))
            };
        } else if let Some(index) = self.additional_roots.iter().position(|root| root == &path) {
            self.additional_roots.remove(index);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Root is not in this workspace",
            ));
        }
        let roots: Vec<_> = self.roots().map(PathBuf::from).collect();
        self.expanded
            .retain(|entry| roots.iter().any(|root| entry.starts_with(root)));
        self.entries
            .retain(|entry| roots.iter().any(|root| entry.path.starts_with(root)));
        Ok(())
    }

    pub fn remove_all_roots(&mut self) {
        self.root = None;
        self.additional_roots.clear();
        self.expanded.clear();
        self.entries.clear();
        self.errors.clear();
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    pub fn entries(&self) -> &[FolderEntry] {
        &self.entries
    }
    pub fn errors(&self) -> &[String] {
        &self.errors
    }
    pub fn follow_links(&self) -> bool {
        self.follow_links
    }
    pub fn set_follow_links(&mut self, follow: bool) {
        self.follow_links = follow;
    }
    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    pub fn scan_input(&self, selected_root: Option<PathBuf>) -> FolderScanInput {
        FolderScanInput {
            root: self.root.clone(),
            additional_roots: self.additional_roots.clone(),
            expanded: self.expanded.clone(),
            selected_root,
            follow_links: self.follow_links,
            add_root: false,
        }
    }

    pub fn scan_input_add_root(&self, root: PathBuf) -> FolderScanInput {
        let mut input = self.scan_input(Some(root));
        input.add_root = true;
        input
    }

    pub fn set_root(&mut self, path: &Path) -> io::Result<()> {
        self.select_root(path)?;
        self.visible = true;
        self.refresh();
        Ok(())
    }

    fn select_root(&mut self, path: &Path) -> io::Result<()> {
        let _access = crate::file_access::directory(path)?;
        let root = fs::canonicalize(path)?;
        if !fs::metadata(&root)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose a directory",
            ));
        }
        if self.root.as_ref() != Some(&root) {
            self.additional_roots.clear();
            self.expanded.clear();
            self.expanded.insert(root.clone());
            self.root = Some(root);
        }
        Ok(())
    }

    pub fn toggle_directory(&mut self, path: &Path) -> io::Result<()> {
        self.toggle_directory_expansion(path)?;
        self.refresh();
        Ok(())
    }

    pub fn toggle_directory_expansion(&mut self, path: &Path) -> io::Result<()> {
        let _access = crate::file_access::existing(path)?;
        if !self.entries.iter().any(|entry| {
            entry.path == path
                && (entry.kind == EntryKind::Directory
                    || (entry.kind == EntryKind::Symlink
                        && self.follow_links
                        && fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())))
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Directory is not in the visible tree",
            ));
        }
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
        }
        Ok(())
    }

    pub fn refresh(&mut self) {
        self.refresh_cancellable(&AtomicBool::new(false));
    }

    fn refresh_cancellable(&mut self, cancel: &AtomicBool) -> bool {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        self.entries.clear();
        self.errors.clear();
        let roots: Vec<_> = self.roots().map(PathBuf::from).collect();
        for root in roots {
            let _access = match crate::file_access::existing(&root) {
                Ok(access) => access,
                Err(error) => {
                    self.errors.push(format!("{}: {error}", root.display()));
                    continue;
                }
            };
            match fs::symlink_metadata(&root) {
                Ok(metadata) if metadata.is_dir() => {
                    if !self.scan(
                        FolderEntry {
                            path: root.clone(),
                            depth: 0,
                            kind: EntryKind::Directory,
                        },
                        cancel,
                    ) {
                        return false;
                    }
                }
                Ok(_) => self
                    .errors
                    .push(format!("{}: root is no longer a directory", root.display())),
                Err(error) => self.errors.push(format!("{}: {error}", root.display())),
            }
        }
        !cancel.load(Ordering::Relaxed)
    }

    fn scan(&mut self, root: FolderEntry, cancel: &AtomicBool) -> bool {
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(row) = pending.pop() {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            let directory = row.path.clone();
            let depth = row.depth;
            let expanded = row.kind == EntryKind::Directory && self.expanded.contains(&directory);
            self.entries.push(row);
            if !expanded {
                continue;
            }
            let _access = match crate::file_access::existing(&directory) {
                Ok(access) => access,
                Err(error) => {
                    self.errors.push(format!("{}: {error}", directory.display()));
                    continue;
                }
            };
            match fs::symlink_metadata(&directory).and_then(|metadata| {
                if !metadata.is_dir()
                    && !(self.follow_links
                        && fs::metadata(&directory).is_ok_and(|metadata| metadata.is_dir()))
                {
                    return Err(io::Error::other("Directory changed; refresh the tree"));
                }
                fs::canonicalize(&directory)
            }) {
                Ok(canonical)
                    if (self.follow_links || canonical == directory)
                        && !visited.contains(&canonical) =>
                {
                    visited.insert(canonical);
                }
                Ok(_) => {
                    self.errors.push(format!(
                        "{}: directory link or repeated directory not traversed",
                        directory.display()
                    ));
                    continue;
                }
                Err(error) => {
                    self.errors
                        .push(format!("{}: {error}", directory.display()));
                    continue;
                }
            }
            let children = match fs::read_dir(&directory) {
                Ok(children) => children,
                Err(error) => {
                    self.errors
                        .push(format!("{}: {error}", directory.display()));
                    continue;
                }
            };
            let mut rows = Vec::new();
            for child in children {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                match child.and_then(|child| Ok((child.path(), child.file_type()?))) {
                    Ok((_,kind)) if kind.is_symlink() && !self.follow_links => {},
                    Ok((path, kind)) => rows.push(FolderEntry {
                        path: path.clone(),
                        depth: depth + 1,
                        kind: if kind.is_symlink()
                            && self.follow_links
                            && fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir())
                        {
                            EntryKind::Directory
                        } else if kind.is_symlink() && self.follow_links && fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
                            EntryKind::File
                        } else if kind.is_symlink() {
                            EntryKind::Symlink
                        } else if kind.is_dir() {
                            EntryKind::Directory
                        } else if kind.is_file() {
                            EntryKind::File
                        } else {
                            EntryKind::Other
                        },
                    }),
                    Err(error) => self
                        .errors
                        .push(format!("{}: {error}", directory.display())),
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            rows.sort_by(|a, b| {
                (a.kind != EntryKind::Directory, &a.path)
                    .cmp(&(b.kind != EntryKind::Directory, &b.path))
            });
            pending.extend(rows.into_iter().rev());
        }
        !cancel.load(Ordering::Relaxed)
    }
}
