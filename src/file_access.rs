use std::io;
use std::path::{Path, PathBuf};

pub struct AccessGuard {
    path: PathBuf,
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    _scopes: Vec<std::sync::Arc<native::Scope>>,
}

pub struct TransferGuard {
    _access: Vec<AccessGuard>,
}

impl AccessGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn unrestricted(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            #[cfg(all(target_os = "macos", feature = "native-ui"))]
            _scopes: Vec::new(),
        }
    }
}

pub fn sandboxed() -> bool {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    {
        return native::sandboxed();
    }
    #[cfg(not(all(target_os = "macos", feature = "native-ui")))]
    false
}

pub fn home_directory() -> io::Result<PathBuf> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return Ok(PathBuf::from(
            objc2_foundation::NSHomeDirectory().to_string(),
        ));
    }
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

pub fn read(path: &Path) -> io::Result<AccessGuard> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return native::acquire(path, false, true);
    }
    Ok(AccessGuard::unrestricted(path))
}

pub fn existing(path: &Path) -> io::Result<AccessGuard> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return native::acquire(path, false, false);
    }
    Ok(AccessGuard::unrestricted(path))
}

pub fn directory(path: &Path) -> io::Result<AccessGuard> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return native::acquire(path, true, true);
    }
    Ok(AccessGuard::unrestricted(path))
}

pub fn selected(path: &Path) -> io::Result<()> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return native::selected(path);
    }
    let _ = path;
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "native-ui"))]
pub fn selected_url(url: objc2::rc::Retained<objc2_foundation::NSURL>) -> io::Result<()> {
    if sandboxed() {
        native::selected_url(url)
    } else {
        Ok(())
    }
}

pub fn shutdown() {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    native::shutdown();
}

pub fn transfer(paths: &[PathBuf]) -> io::Result<(serde_json::Value, TransferGuard)> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return native::transfer(paths);
    }
    let _ = paths;
    Ok((serde_json::json!({"schema_version": 1, "bookmarks": []}), TransferGuard { _access: Vec::new() }))
}

pub fn receive(value: &serde_json::Value) -> io::Result<()> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    if sandboxed() {
        return native::receive(value);
    }
    let _ = value;
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "native-ui"))]
mod native {
    use super::*;
    use crate::config::ConfigStore;
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{
        NSData, NSError, NSString, NSURL, NSURLBookmarkCreationOptions,
        NSURLBookmarkResolutionOptions,
    };
    use serde_json::{Value, json};
    use std::ffi::c_void;
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::time::{Duration, Instant};

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecTaskCreateFromSelf(allocator: *const c_void) -> *const c_void;
        fn SecTaskCopyValueForEntitlement(
            task: *const c_void,
            entitlement: *const c_void,
            error: *mut *const c_void,
        ) -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: *const c_void);
        fn CFGetTypeID(value: *const c_void) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(value: *const c_void) -> u8;
    }

    unsafe extern "C" {
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(
            queue: *const c_void,
            context: *mut c_void,
            work: extern "C" fn(*mut c_void),
        );
    }

    static SANDBOXED: OnceLock<bool> = OnceLock::new();
    static HELD: Mutex<Vec<Grant>> = Mutex::new(Vec::new());
    static CLOSED: AtomicBool = AtomicBool::new(false);
    static PANEL_OPEN: AtomicBool = AtomicBool::new(false);

    pub(super) struct Scope {
        url: Retained<NSURL>,
        active: bool,
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            if self.active {
                unsafe { self.url.stopAccessingSecurityScopedResource() };
            }
        }
    }

    #[derive(Debug)]
    struct UnavailableScope(String);

    impl std::fmt::Display for UnavailableScope {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(&self.0)
        }
    }

    impl std::error::Error for UnavailableScope {}

    fn unavailable(error: impl std::fmt::Display) -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            UnavailableScope(error.to_string()),
        )
    }

    #[derive(Clone)]
    struct Grant {
        scope: Arc<Scope>,
        path: PathBuf,
        directory: bool,
        writable: bool,
        identity: Option<(u64, u64)>,
    }

    #[derive(Clone)]
    struct Record {
        path: PathBuf,
        directory: bool,
        writable: bool,
        bookmark: String,
    }

    impl Record {
        fn value(&self) -> io::Result<Value> {
            Ok(json!({
                "path": path_text(&self.path)?,
                "directory": self.directory,
                "writable": self.writable,
                "bookmark": self.bookmark,
            }))
        }
    }

    pub(super) fn sandboxed() -> bool {
        *SANDBOXED.get_or_init(|| unsafe {
            let task = SecTaskCreateFromSelf(std::ptr::null());
            if task.is_null() {
                eprintln!("unable to create task for runtime sandbox entitlement lookup");
                return false;
            }
            let key = NSString::from_str("com.apple.security.app-sandbox");
            let mut error = std::ptr::null();
            let value =
                SecTaskCopyValueForEntitlement(task, (&*key as *const NSString).cast(), &mut error);
            let enabled = !value.is_null()
                && CFGetTypeID(value) == CFBooleanGetTypeID()
                && CFBooleanGetValue(value) != 0;
            if !value.is_null() {
                CFRelease(value);
            }
            if !error.is_null() {
                eprintln!("unable to read runtime sandbox entitlement");
                CFRelease(error);
            }
            CFRelease(task);
            enabled
        })
    }

    fn path_text(path: &Path) -> io::Result<&str> {
        path.to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件路径不是有效 UTF-8"))
    }

    fn interrupted() -> io::Error {
        io::Error::new(io::ErrorKind::Interrupted, "文件授权已取消")
    }

    fn ensure_open() -> io::Result<()> {
        if CLOSED.load(Ordering::Acquire) {
            Err(interrupted())
        } else {
            Ok(())
        }
    }

    fn foundation_error(error: Retained<NSError>) -> io::Error {
        let kind = if error.domain().to_string() == "NSPOSIXErrorDomain" {
            io::Error::from_raw_os_error(error.code() as i32).kind()
        } else {
            io::ErrorKind::PermissionDenied
        };
        io::Error::new(kind, error.localizedDescription().to_string())
    }

    fn resolution_error(error: Retained<NSError>) -> io::Error {
        let domain = error.domain().to_string();
        if domain == "NSPOSIXErrorDomain"
            || (domain == "NSCocoaErrorDomain" && matches!(error.code(), 257 | 513))
        {
            foundation_error(error)
        } else {
            unavailable(error.localizedDescription().to_string())
        }
    }

    fn file_url(path: &Path, directory: bool) -> io::Result<Retained<NSURL>> {
        let url = if directory {
            NSURL::from_directory_path(path)
        } else {
            NSURL::from_file_path(path)
        };
        url.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件路径无效"))
    }

    fn encode(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut result = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 15) as usize] as char);
        }
        result
    }

    fn decode(text: &str) -> io::Result<Vec<u8>> {
        fn digit(byte: u8) -> io::Result<u8> {
            match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                b'A'..=b'F' => Ok(byte - b'A' + 10),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "书签不是十六进制数据",
                )),
            }
        }
        if text.is_empty() || text.len() % 2 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "书签数据长度无效",
            ));
        }
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| Ok((digit(pair[0])? << 4) | digit(pair[1])?))
            .collect()
    }

    fn records(document: &Value) -> io::Result<Vec<Record>> {
        if document.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "文件授权版本无效",
            ));
        }
        let entries = document
            .get("bookmarks")
            .and_then(Value::as_array)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "文件授权列表无效"))?;
        entries
            .iter()
            .map(|entry| {
                let path = entry
                    .get("path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "文件授权路径无效")
                    })?;
                let directory =
                    entry
                        .get("directory")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "文件授权类型无效")
                        })?;
                let bookmark = entry
                    .get("bookmark")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "文件授权书签无效")
                    })?;
                let writable = match entry.get("writable") {
                    Some(Value::Bool(writable)) => *writable,
                    None => true,
                    Some(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "文件授权读写模式无效",
                        ));
                    }
                };
                Ok(Record {
                    path,
                    directory,
                    writable,
                    bookmark: bookmark.to_owned(),
                })
            })
            .collect()
    }

    fn store() -> io::Result<ConfigStore> {
        Ok(ConfigStore::under_home(&home_directory()?))
    }

    fn load() -> io::Result<Vec<Record>> {
        match store()?.load_json(Path::new("file-access.json")) {
            Ok(document) => records(&document),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn persist(old: Option<&Record>, new: &Record) -> io::Result<()> {
        let value = new.value()?;
        store()?.update_json(
            Path::new("file-access.json"),
            &json!({"schema_version": 1, "bookmarks": []}),
            |document| {
                let current = records(document)?;
                if let Some(old) = old {
                    if !current
                        .iter()
                        .any(|entry| entry.path == old.path && entry.bookmark == old.bookmark)
                    {
                        return Ok(());
                    }
                }
                let entries = document
                    .get_mut("bookmarks")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "文件授权列表无效")
                    })?;
                if let Some(old) = old {
                    entries.retain(|entry| {
                        entry.get("path").and_then(Value::as_str) != old.path.to_str()
                    });
                    if old.path != new.path && current.iter().any(|entry| entry.path == new.path) {
                        return Ok(());
                    }
                }
                entries
                    .retain(|entry| entry.get("path").and_then(Value::as_str) != new.path.to_str());
                entries.push(value);
                Ok(())
            },
        )?;
        Ok(())
    }

    fn bookmark(url: &NSURL, explicit: bool, writable: bool) -> io::Result<String> {
        let options = if explicit {
            NSURLBookmarkCreationOptions::WithSecurityScope
                | if writable {
                    NSURLBookmarkCreationOptions::empty()
                } else {
                    NSURLBookmarkCreationOptions::SecurityScopeAllowOnlyReadAccess
                }
        } else {
            NSURLBookmarkCreationOptions::empty()
        };
        let data = url
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                options, None, None,
            )
            .map_err(foundation_error)?;
        Ok(encode(&data.to_vec()))
    }

    fn resolve(record: &Record, explicit: bool) -> io::Result<(Grant, bool)> {
        let data = NSData::with_bytes(&decode(&record.bookmark)?);
        let options = NSURLBookmarkResolutionOptions::WithoutUI
            | if explicit {
                NSURLBookmarkResolutionOptions::WithSecurityScope
            } else {
                NSURLBookmarkResolutionOptions::empty()
            };
        let mut stale = Bool::NO;
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data, options, None, &mut stale,
            )
        }
        .map_err(resolution_error)?;
        if !url.isFileURL() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "书签未指向本地文件",
            ));
        }
        let active = if explicit {
            unsafe { url.startAccessingSecurityScopedResource() }
        } else {
            true
        };
        let scope = Arc::new(Scope { url, active });
        let path = scope
            .url
            .to_file_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "书签路径无效"))?;
        if explicit && !active {
            let home = fs::canonicalize(home_directory()?)?;
            if !path.starts_with(&home) || !canonical_destination(&path)?.starts_with(&home) {
                return Err(unavailable("文件授权已失效"));
            }
        }
        let path = fs::canonicalize(path)?;
        let metadata = fs::metadata(&path)?;
        let directory = metadata.is_dir();
        let identity = Some((metadata.dev(), metadata.ino()));
        Ok((
            Grant {
                scope,
                path,
                directory,
                writable: record.writable,
                identity,
            },
            stale.as_bool(),
        ))
    }

    fn remember(grant: Grant) -> io::Result<()> {
        let mut held = HELD
            .lock()
            .map_err(|_| io::Error::other("文件授权锁不可用"))?;
        ensure_open()?;
        held.retain(|previous| previous.path != grant.path || previous.writable != grant.writable);
        held.push(grant);
        Ok(())
    }

    fn register(url: &NSURL, writable: bool) -> io::Result<Grant> {
        let path = url
            .to_file_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件选择路径无效"))?;
        let path = fs::canonicalize(path)?;
        let record = Record {
            directory: fs::metadata(&path)?.is_dir(),
            writable,
            path,
            bookmark: bookmark(url, true, writable)?,
        };
        let (grant, _) = resolve(&record, true)?;
        persist(None, &record)?;
        Ok(grant)
    }

    fn adopt(path: &Path) -> io::Result<Option<Grant>> {
        let path = fs::canonicalize(path)?;
        let directory = fs::metadata(&path)?.is_dir();
        let url = file_url(&path, directory)?;
        let bookmark = match bookmark(&url, true, false) {
            Ok(bookmark) => bookmark,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(None),
            Err(error) => return Err(error),
        };
        let record = Record {
            path,
            directory,
            writable: false,
            bookmark,
        };
        let (grant, _) = match resolve(&record, true) {
            Ok(resolved) => resolved,
            Err(error)
                if error
                    .get_ref()
                    .is_some_and(|inner| inner.is::<UnavailableScope>()) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        persist(None, &record)?;
        Ok(Some(grant))
    }

    pub(super) fn selected(path: &Path) -> io::Result<()> {
        ensure_open()?;
        let path = std::path::absolute(path)?;
        let directory = match fs::metadata(&path) {
            Ok(metadata) => metadata.is_dir(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let home = fs::canonicalize(home_directory()?)?;
        if fs::canonicalize(&path)?.starts_with(&home) {
            return Ok(());
        }
        let url = file_url(&path, directory)?;
        remember(register(&url, true)?)
    }

    pub(super) fn selected_url(url: Retained<NSURL>) -> io::Result<()> {
        let scope = Arc::new(Scope { url, active: true });
        ensure_open()?;
        let path = scope
            .url
            .to_file_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件选择路径无效"))?;
        let (directory, identity) = match fs::metadata(&path) {
            Ok(metadata) => {
                let grant = register(&scope.url, true)?;
                remember(grant)?;
                (metadata.is_dir(), Some((metadata.dev(), metadata.ino())))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (false, None),
            Err(error) => return Err(error),
        };
        let path = if identity.is_none() {
            std::path::absolute(&path)?
        } else {
            fs::canonicalize(&path)?
        };
        remember(Grant {
            scope,
            path,
            directory,
            writable: true,
            identity,
        })
    }

    fn restored(requested: &Path, directory: bool) -> io::Result<Vec<Grant>> {
        let canonical = fs::canonicalize(requested).ok();
        let relevant = |path: &Path, directory: bool| {
            requested == path || (directory && requested.starts_with(path))
                || canonical.as_ref().is_some_and(|resolved| resolved == path || (directory && resolved.starts_with(path)))
        };
        let held = HELD
            .lock()
            .map_err(|_| io::Error::other("文件授权锁不可用"))?
            .clone();
        let mut grants = Vec::with_capacity(held.len());
        for mut grant in held.into_iter().filter(|grant| relevant(&grant.path, grant.directory)) {
            match fs::metadata(&grant.path) {
                Ok(metadata) => {
                    if grant
                        .identity
                        .is_some_and(|identity| identity != (metadata.dev(), metadata.ino()))
                    {
                        continue;
                    }
                    grant.path = fs::canonicalize(&grant.path)?;
                    grants.push(grant);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if grant.identity.is_none() {
                        grants.push(grant);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
                Err(error) => return Err(error),
            }
        }
        if covered(requested, directory, &grants)?.is_some() {
            return Ok(grants);
        }
        for record in load()?.into_iter().filter(|record| relevant(&record.path, record.directory)) {
            let (grant, stale) = match resolve(&record, true) {
                Ok(resolved) => resolved,
                Err(error)
                    if error
                        .get_ref()
                        .is_some_and(|inner| inner.is::<UnavailableScope>()) =>
                {
                    continue;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
                Err(error) => return Err(error),
            };
            if stale || grant.path != record.path || grant.directory != record.directory {
                let refreshed = Record {
                    path: grant.path.clone(),
                    directory: grant.directory,
                    writable: grant.writable,
                    bookmark: bookmark(&grant.scope.url, true, grant.writable)?,
                };
                persist(Some(&record), &refreshed)?;
            }
            remember(grant.clone())?;
            grants.push(grant);
        }
        Ok(grants)
    }

    fn canonical_destination(path: &Path) -> io::Result<PathBuf> {
        match fs::canonicalize(path) {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if fs::symlink_metadata(path)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    return Err(error);
                }
                let parent = path.parent().ok_or(error)?;
                let name = path.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "文件路径缺少名称")
                })?;
                Ok(canonical_destination(parent)?.join(name))
            }
            Err(error) => Err(error),
        }
    }

    fn covered(path: &Path, directory: bool, grants: &[Grant]) -> io::Result<Option<PathBuf>> {
        let canonical = match canonical_destination(path) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if grants.iter().any(|grant| {
            (grant.directory
                && (!directory || grant.writable)
                && canonical.starts_with(&grant.path))
                || (!directory && canonical == grant.path)
        }) {
            Ok(Some(canonical))
        } else {
            Ok(None)
        }
    }

    fn existing_directory(path: &Path) -> io::Result<PathBuf> {
        let mut candidate = path.to_path_buf();
        loop {
            match fs::metadata(&candidate) {
                Ok(metadata) if metadata.is_dir() => {
                    if candidate.parent().is_none() && candidate != path {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "无法确定待创建目录的授权范围",
                        ));
                    }
                    return Ok(candidate);
                }
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "授权路径不是目录",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if !candidate.pop() {
                        return Err(error);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    return Ok(candidate);
                }
                Err(error) => return Err(error),
            }
        }
    }

    struct PanelSession;

    impl Drop for PanelSession {
        fn drop(&mut self) {
            PANEL_OPEN.store(false, Ordering::Release);
        }
    }

    fn panel(path: &Path, directory: bool, requested: &Path) -> io::Result<Grant> {
        ensure_open()?;
        let mtm = MainThreadMarker::new().ok_or_else(|| io::Error::other("文件授权需要主线程"))?;
        if PANEL_OPEN
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "另一个文件授权窗口尚未关闭",
            ));
        }
        let _session = PanelSession;
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseDirectories(true);
        panel.setCanChooseFiles(!directory);
        panel.setAllowsMultipleSelection(false);
        panel.setCanCreateDirectories(false);
        panel.setResolvesAliases(false);
        panel.setTitle(Some(&NSString::from_str("授权文件访问")));
        let message = if directory && path == requested {
            format!(
                "请选择此目录。若目录尚未创建，请选择最近的现有上级目录。授权包括所选目录的全部子目录：\n{}",
                path.display()
            )
        } else if path == requested {
            format!(
                "请选择此{}以授权访问：\n{}",
                if directory {
                    "目录（包括子目录）"
                } else {
                    "文件或目录"
                },
                path.display()
            )
        } else {
            format!(
                "创建 {} 需要读写下列现有目录（包括子目录）。请选择此目录授权：\n{}",
                requested.display(),
                path.display()
            )
        };
        panel.setMessage(Some(&NSString::from_str(&message)));
        let initial = if directory {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        let initial_url = file_url(initial, true)?;
        panel.setDirectoryURL(Some(&initial_url));
        if !directory {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                panel.setNameFieldStringValue(&NSString::from_str(name));
            }
        }
        let response = panel.runModal();
        panel.orderOut(None);
        if response != NSModalResponseOK {
            return Err(interrupted());
        }
        let url = panel
            .URL()
            .ok_or_else(|| io::Error::other("授权窗口未返回文件"))?;
        let automatic = Scope { url, active: true };
        ensure_open()?;
        let chosen = automatic
            .url
            .to_file_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "授权窗口返回了无效路径"))?;
        let expected = if directory {
            existing_directory(requested)?
        } else {
            path.to_path_buf()
        };
        if fs::canonicalize(&chosen)? != fs::canonicalize(&expected)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "请选择请求授权的准确路径",
            ));
        }
        if directory && !fs::metadata(&chosen)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "所选文件授权类型不匹配",
            ));
        }
        register(&automatic.url, directory)
    }

    fn on_main<T: Send + 'static>(
        work: impl FnOnce() -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        if MainThreadMarker::new().is_some() {
            return work();
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let status = Arc::new(AtomicU8::new(0));
        let dispatched_status = status.clone();
        let job: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(move || {
            if dispatched_status
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err(io::Error::other("原生文件授权未能完成")));
            if sender.send(result).is_err() {
                eprintln!("file authorization receiver disconnected");
            }
        }));
        extern "C" fn run(context: *mut c_void) {
            let job = unsafe { Box::from_raw(context.cast::<Box<dyn FnOnce() + Send>>()) };
            job();
        }
        unsafe {
            dispatch_async_f(
                std::ptr::addr_of!(_dispatch_main_q),
                Box::into_raw(job).cast(),
                run,
            );
        }
        let queued = Instant::now();
        loop {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("文件授权主线程已断开"));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if CLOSED.load(Ordering::Acquire) {
                        let _ = status.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
                        return Err(interrupted());
                    }
                    if queued.elapsed() >= Duration::from_secs(30)
                        && status
                            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "主线程尚未处理文件授权，请在界面空闲后重试",
                        ));
                    }
                }
            }
        }
    }

    pub(super) fn acquire(path: &Path, directory: bool, prompt: bool) -> io::Result<AccessGuard> {
        ensure_open()?;
        let path = std::path::absolute(path)?;
        let home = fs::canonicalize(home_directory()?)?;
        let temporary = canonical_destination(&PathBuf::from(
            objc2_foundation::NSTemporaryDirectory().to_string(),
        ))?;
        if let Ok(destination) = canonical_destination(&path)
            && [&home, &temporary].iter().any(|root| destination.starts_with(root)) {
            if !directory { fs::metadata(&path)?; }
            return Ok(AccessGuard { path, _scopes: Vec::new() });
        }
        let mut grants = restored(&path, directory)?;
        if let Some(canonical) = covered(&path, directory, &grants)? {
            if !directory {
                fs::metadata(&canonical)?;
            }
            if directory && fs::metadata(&canonical).is_ok_and(|metadata| !metadata.is_dir()) {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "授权路径不是目录",
                ));
            }
            return Ok(AccessGuard {
                path: canonical,
                _scopes: grants.into_iter().map(|grant| grant.scope).collect(),
            });
        }
        if !directory {
            match fs::File::open(&path) {
                Ok(_) => {
                    if prompt {
                        if let Some(grant) = adopt(&path)? {
                            grants.push(grant);
                        }
                    }
                    return Ok(AccessGuard {
                        path,
                        _scopes: grants.into_iter().map(|grant| grant.scope).collect(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
                Err(error) => return Err(error),
            }
        }
        if !prompt {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "此路径没有可恢复的文件授权",
            ));
        }
        let target = if directory {
            existing_directory(&path)?
        } else {
            path.clone()
        };
        let requested = path.clone();
        let grant = on_main(move || panel(&target, directory, &requested))?;
        grants.push(grant);
        let canonical = covered(&path, directory, &grants)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::PermissionDenied, "所选授权未覆盖请求路径")
        })?;
        Ok(AccessGuard {
            path: canonical,
            _scopes: grants.into_iter().map(|grant| grant.scope).collect(),
        })
    }

    pub(super) fn transfer(paths: &[PathBuf]) -> io::Result<(Value, TransferGuard)> {
        ensure_open()?;
        let mut entries = Vec::with_capacity(paths.len());
        let mut guards = Vec::with_capacity(paths.len());
        for path in paths {
            let guard = acquire(path, false, true)?;
            let path = fs::canonicalize(&guard.path)?;
            let directory = fs::metadata(&path)?.is_dir();
            let url = file_url(&path, directory)?;
            entries.push(
                Record {
                    path,
                    directory,
                    writable: false,
                    bookmark: bookmark(&url, false, false)?,
                }
                .value()?,
            );
            guards.push(guard);
        }
        ensure_open()?;
        Ok((json!({"schema_version": 1, "bookmarks": entries}), TransferGuard { _access: guards }))
    }

    pub(super) fn receive(value: &Value) -> io::Result<()> {
        ensure_open()?;
        for record in records(value)? {
            let (implicit, _) = resolve(&record, false)?;
            let persisted = Record {
                path: implicit.path.clone(),
                directory: implicit.directory,
                writable: implicit.writable,
                bookmark: bookmark(&implicit.scope.url, true, implicit.writable)?,
            };
            let (explicit, _) = resolve(&persisted, true)?;
            persist(None, &persisted)?;
            remember(implicit)?;
            remember(explicit)?;
        }
        Ok(())
    }

    pub(super) fn shutdown() {
        CLOSED.store(true, Ordering::Release);
        let held = match HELD.lock() {
            Ok(mut held) => std::mem::take(&mut *held),
            Err(error) => {
                eprintln!("file authorization lock was poisoned during shutdown");
                std::mem::take(&mut *error.into_inner())
            }
        };
        drop(held);
    }
}
