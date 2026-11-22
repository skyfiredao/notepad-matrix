use crate::config::ConfigStore;
use crate::localization::Text;
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const BOOTSTRAP: &str = "config-directory.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Automatic,
    Custom { path: PathBuf, bookmark: Option<String> },
}

pub struct Snapshot {
    pub default_root: PathBuf,
    pub active_root: PathBuf,
    pub pending: Selection,
    pub sandboxed: bool,
}

impl Snapshot {
    pub fn pending_root(&self) -> &Path {
        match &self.pending {
            Selection::Automatic => &self.default_root,
            Selection::Custom { path, .. } => path,
        }
    }
}

struct ActiveDirectory {
    home: PathBuf,
    root: PathBuf,
    pending: Mutex<Selection>,
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    _scope: Option<native::Scope>,
}

static ACTIVE: OnceLock<ActiveDirectory> = OnceLock::new();

#[derive(Debug)]
struct DirectoryError {
    message: Text,
    cause: Option<io::Error>,
}

impl DirectoryError {
    fn render(&self, locale: &str) -> String {
        let message = match &self.cause {
            Some(cause) => self.message.clone().replace("$ERROR$", error_text(cause, locale)),
            None => self.message.clone(),
        };
        message.render(locale)
    }
}

impl std::fmt::Display for DirectoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.render("english.xml"))
    }
}

impl std::error::Error for DirectoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| cause as &dyn std::error::Error)
    }
}

pub(crate) fn error_text(error: &io::Error, locale: &str) -> String {
    error.get_ref().and_then(|cause| cause.downcast_ref::<DirectoryError>())
        .map_or_else(|| error.to_string(), |cause| cause.render(locale))
}

fn error(kind: io::ErrorKind, message: Text) -> io::Error {
    io::Error::new(kind, DirectoryError { message, cause: None })
}

fn with_cause(cause: io::Error, message: Text) -> io::Error {
    io::Error::new(cause.kind(), DirectoryError { message, cause: Some(cause) })
}

fn invalid(id: &'static str, fallback: &'static str) -> io::Error {
    error(io::ErrorKind::InvalidData, Text::resource(id, fallback))
}

fn selection(document: &Value) -> io::Result<Selection> {
    if document["schema_version"].as_u64() != Some(1) {
        return Err(invalid("ConfigDirectoryRecordVersionInvalid", "Invalid configuration directory record version."));
    }
    match document["mode"].as_str() {
        Some("automatic") => Ok(Selection::Automatic),
        Some("custom") => {
            let path = document["path"].as_str().map(PathBuf::from)
                .ok_or_else(|| invalid("ConfigDirectoryPathInvalid", "Invalid custom configuration directory path."))?;
            validate_path(&path)?;
            let bookmark = match document.get("bookmark") {
                None | Some(Value::Null) => None,
                Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
                _ => return Err(invalid("ConfigDirectoryBookmarkInvalid", "Invalid configuration directory bookmark.")),
            };
            Ok(Selection::Custom { path, bookmark })
        }
        _ => Err(invalid("ConfigDirectoryModeInvalid", "Invalid configuration directory mode.")),
    }
}

fn validate_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || path.parent().is_none()
        || path.components().any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || path.to_str().is_none_or(|text| text.contains('\0')) {
        return Err(invalid("ConfigDirectoryAbsolutePathRequired", "Choose a valid absolute local directory other than the filesystem root."));
    }
    Ok(())
}

fn validate_directory(path: &Path) -> io::Result<PathBuf> {
    validate_path(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("ConfigDirectoryExistingDirectoryRequired", "The configuration directory must exist and must not be a symbolic link."));
    }
    let path = fs::canonicalize(path)?;
    validate_path(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != fs::metadata(crate::file_access::home_directory()?)?.uid() {
            return Err(error(io::ErrorKind::PermissionDenied,
                Text::resource("ConfigDirectoryOwnedDirectoryRequired", "Choose a configuration directory owned by the current user.")));
        }
    }
    Ok(path)
}

fn load(home: &Path) -> io::Result<Selection> {
    let root = home.join(".notepad-matrix");
    match fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {},
        Ok(_) => return Err(invalid("ConfigDirectoryDefaultDirectoryInvalid", "The default configuration directory must be a directory, not a symbolic link.")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Selection::Automatic),
        Err(error) => return Err(error),
    }
    let path = root.join(BOOTSTRAP);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {},
        Ok(_) => return Err(invalid("ConfigDirectoryRecordFileInvalid", "The configuration directory record must be a regular file, not a symbolic link.")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Selection::Automatic),
        Err(error) => return Err(error),
    }
    selection(&serde_json::from_slice::<Value>(&fs::read(path)?)?)
}

pub fn initialize(home: &Path) -> io::Result<()> {
    if let Some(active) = ACTIVE.get() {
        if active.home != home {
            return Err(invalid("ConfigDirectoryAlreadyInitialized", "The configuration directory is already initialized for this process."));
        }
        return Ok(());
    }
    let pending = load(home).map_err(|cause| with_cause(cause,
        Text::resource("ConfigDirectorySelectionReadFailed", "Could not read the configuration directory selection $PATH$: $ERROR$")
            .replace("$PATH$", home.join(".notepad-matrix").join(BOOTSTRAP).display().to_string())))?;
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    let scope = match &pending {
        Selection::Custom { bookmark, .. } if crate::file_access::sandboxed() || bookmark.is_some() => {
            Some(native::restore(bookmark.as_deref().ok_or_else(||
                error(io::ErrorKind::PermissionDenied, Text::resource("ConfigDirectoryScopeBookmarkMissing", "The custom configuration directory has no sandbox authorization bookmark.")))?)?)
        }
        _ => None,
    };
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    let pending = match (&pending, scope.as_ref()) {
        (Selection::Custom { bookmark, .. }, Some(scope)) => {
            let refreshed = Selection::Custom {
                path: scope.directory()?,
                bookmark: scope.refreshed_bookmark.clone().or_else(|| bookmark.clone()),
            };
            if refreshed != pending { write_selection(home, &refreshed)?; }
            refreshed
        }
        _ => pending,
    };
    let root = match &pending {
        Selection::Automatic => home.join(".notepad-matrix"),
        Selection::Custom { path, .. } => validate_directory(path).map_err(|cause|
            with_cause(cause, Text::resource("ConfigDirectoryUseFailed", "Could not use configuration directory $PATH$: $ERROR$. No other configuration directory was read.")
                .replace("$PATH$", path.display().to_string())))?,
    };
    ACTIVE.set(ActiveDirectory { home: home.to_owned(), root, pending: Mutex::new(pending),
        #[cfg(all(target_os = "macos", feature = "native-ui"))]
        _scope: scope,
    }).map_err(|_| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryInitConflict", "Configuration directory initialization conflict.")))
}

pub(crate) fn active_root(home: &Path) -> PathBuf {
    ACTIVE.get().map_or_else(|| home.join(".notepad-matrix"), |active| active.root.clone())
}

pub fn snapshot() -> io::Result<Snapshot> {
    let active = ACTIVE.get().ok_or_else(|| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryNotInitialized", "The configuration directory is not initialized.")))?;
    let pending = active.pending.lock().map_err(|_| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryChoiceLockFailed", "The configuration directory selection lock is unavailable.")))?.clone();
    Ok(Snapshot { default_root: active.home.join(".notepad-matrix"), active_root: active.root.clone(),
        pending, sandboxed: crate::file_access::sandboxed() })
}

fn write_selection(home: &Path, selection: &Selection) -> io::Result<()> {
    let value = match selection {
        Selection::Automatic => json!({"schema_version": 1, "mode": "automatic"}),
        Selection::Custom { path, bookmark } => json!({"schema_version": 1, "mode": "custom", "path": path, "bookmark": bookmark}),
    };
    ConfigStore::under_default_home(home).update_json(Path::new(BOOTSTRAP), &value,
        |document| { *document = value.clone(); Ok(()) })?;
    Ok(())
}

fn persist(selection: Selection) -> io::Result<()> {
    let active = ACTIVE.get().ok_or_else(|| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryNotInitialized", "The configuration directory is not initialized.")))?;
    let mut pending = active.pending.lock().map_err(|_| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryChoiceLockFailed", "The configuration directory selection lock is unavailable.")))?;
    write_selection(&active.home, &selection)?;
    *pending = selection;
    Ok(())
}

pub fn select_automatic() -> io::Result<()> {
    persist(Selection::Automatic)
}

#[cfg(all(target_os = "macos", feature = "native-ui"))]
pub fn choose_custom_directory(locale: &str, parent: &objc2_app_kit::NSWindow) -> io::Result<futures::channel::oneshot::Receiver<io::Result<bool>>> {
    native::choose(locale, parent)
}

#[cfg(unix)]
pub(crate) fn instance_socket(home: &Path, root: &Path) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    use sha2::{Digest, Sha256};
    let default_root = home.join(".notepad-matrix");
    let path = if root == default_root {
        root.join("instance.sock")
    } else {
        let digest = Sha256::digest(root.as_os_str().as_bytes());
        let mut value = u64::from_be_bytes([digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7]]);
        let mut name = String::with_capacity(13);
        for _ in 0..13 {
            name.push(b"abcdefghijklmnopqrstuvwxyz234567"[(value & 31) as usize] as char);
            value >>= 5;
        }
        let store = ConfigStore::under_default_home(home);
        let directory = store.document_path(Path::new("instance.json"))?.parent()
            .ok_or_else(|| invalid("ConfigDirectoryInstanceDirectoryMissing", "The default instance directory is missing."))?.to_owned();
        directory.join(name)
    };
    #[cfg(target_os = "macos")]
    let limit = 104;
    #[cfg(not(target_os = "macos"))]
    let limit = 108;
    if path.as_os_str().as_bytes().len() >= limit {
        return Err(error(io::ErrorKind::InvalidData,
            Text::resource("ConfigDirectorySocketPathTooLong", "The instance communication path exceeds the system length limit: $PATH$")
                .replace("$PATH$", path.display().to_string())));
    }
    Ok(path)
}

#[cfg(all(target_os = "macos", feature = "native-ui"))]
mod native {
    use super::*;
    use block2::RcBlock;
    use futures::channel::oneshot;
    use std::cell::Cell;
    use objc2::{MainThreadMarker, rc::Retained, runtime::Bool};
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSWindow};
    use objc2_foundation::{NSData, NSError, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions};

    pub(super) struct Scope {
        url: Retained<NSURL>,
        active: bool,
        pub(super) refreshed_bookmark: Option<String>,
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            if self.active { unsafe { self.url.stopAccessingSecurityScopedResource() }; }
        }
    }

    impl Scope {
        pub(super) fn directory(&self) -> io::Result<PathBuf> {
            validate_directory(&self.url.to_file_path().ok_or_else(|| invalid("ConfigDirectoryBookmarkPathInvalid", "Invalid configuration directory bookmark path."))?)
        }
    }

    fn foundation_error(error: Retained<NSError>) -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, error.localizedDescription().to_string())
    }

    fn bookmark(url: &NSURL) -> io::Result<String> {
        let options = if crate::file_access::sandboxed() {
            NSURLBookmarkCreationOptions::WithSecurityScope
        } else { NSURLBookmarkCreationOptions::empty() };
        let data = url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            options, None, None,
        ).map_err(foundation_error)?;
        Ok(encode(&data.to_vec()))
    }

    fn encode(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            text.push(HEX[(byte >> 4) as usize] as char);
            text.push(HEX[(byte & 15) as usize] as char);
        }
        text
    }

    fn decode(text: &str) -> io::Result<Vec<u8>> {
        if text.is_empty() || text.len() % 2 != 0 { return Err(invalid("ConfigDirectoryBookmarkLengthInvalid", "Invalid configuration directory bookmark length.")); }
        fn digit(byte: u8) -> io::Result<u8> {
            match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                b'A'..=b'F' => Ok(byte - b'A' + 10),
                _ => Err(invalid("ConfigDirectoryBookmarkDataInvalid", "Invalid configuration directory bookmark data.")),
            }
        }
        text.as_bytes().chunks_exact(2).map(|pair| Ok(digit(pair[0])? << 4 | digit(pair[1])?)).collect()
    }

    pub(super) fn restore(bookmark: &str) -> io::Result<Scope> {
        let data = NSData::with_bytes(&decode(bookmark)?);
        let mut stale = Bool::NO;
        let sandboxed = crate::file_access::sandboxed();
        let options = if sandboxed {
            NSURLBookmarkResolutionOptions::WithoutUI | NSURLBookmarkResolutionOptions::WithSecurityScope
        } else { NSURLBookmarkResolutionOptions::WithoutUI };
        let url = unsafe { NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data, options,
            None, &mut stale,
        ) }.map_err(|failure| with_cause(foundation_error(failure),
            Text::resource("ConfigDirectoryScopeRestoreFailed", "Could not restore configuration directory authorization: $ERROR$. No other configuration directory was read.")))?;
        if !url.isFileURL() { return Err(invalid("ConfigDirectoryBookmarkNotLocal", "The configuration directory bookmark is not a local directory.")); }
        let active = sandboxed && unsafe { url.startAccessingSecurityScopedResource() };
        let mut scope = Scope { url, active, refreshed_bookmark: None };
        let resolved = scope.url.to_file_path().ok_or_else(|| invalid("ConfigDirectoryBookmarkPathInvalid", "Invalid configuration directory bookmark path."))?;
        if sandboxed && !active {
            let home = fs::canonicalize(crate::file_access::home_directory()?)?;
            if !fs::canonicalize(&resolved)?.starts_with(home) {
                return Err(error(io::ErrorKind::PermissionDenied, Text::resource("ConfigDirectoryScopeExpired", "Configuration directory authorization has expired. No other configuration directory was read.")));
            }
        }
        validate_directory(&resolved)?;
        if stale.as_bool() {
            scope.refreshed_bookmark = Some(self::bookmark(&scope.url)?);
        }
        Ok(scope)
    }

    pub(super) fn choose(locale: &str, parent: &NSWindow) -> io::Result<oneshot::Receiver<io::Result<bool>>> {
        let mtm = MainThreadMarker::new().ok_or_else(|| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryChooseNeedsMainThread", "Choosing a configuration directory requires the main thread.")))?;
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseDirectories(true);
        panel.setCanChooseFiles(false);
        panel.setAllowsMultipleSelection(false);
        panel.setCanCreateDirectories(true);
        panel.setResolvesAliases(false);
        panel.setShowsHiddenFiles(true);
        panel.setTitle(Some(&NSString::from_str(&Text::resource("ConfigDirectoryPickerTitle", "Choose a custom configuration directory").render(locale))));
        panel.setMessage(Some(&NSString::from_str(&Text::resource("ConfigDirectoryPickerMessage", "Restart to apply; no migration.").render(locale))));
        let (sender, receiver) = oneshot::channel();
        let sender = Cell::new(Some(sender));
        let completion = RcBlock::new({
            let panel = panel.clone();
            move |response: isize| {
                let Some(sender) = sender.take() else { return; };
                panel.orderOut(None);
                let result = finish_choice(&panel, response);
                if sender.send(result).is_err() {
                    eprintln!("configuration directory selection no longer has a receiver");
                }
            }
        });
        panel.beginSheetModalForWindow_completionHandler(parent, &completion);
        Ok(receiver)
    }

    fn finish_choice(panel: &NSOpenPanel, response: isize) -> io::Result<bool> {
        if response != NSModalResponseOK { return Ok(false); }
        let url = panel.URL().ok_or_else(|| error(io::ErrorKind::Other, Text::resource("ConfigDirectoryPickerEmpty", "The directory picker did not return a directory.")))?;
        let sandboxed = crate::file_access::sandboxed();
        let automatic = Scope { url, active: sandboxed, refreshed_bookmark: None };
        let path = automatic.url.to_file_path().ok_or_else(|| invalid("ConfigDirectorySelectedPathInvalid", "Invalid selected configuration directory path."))?;
        let path = validate_directory(&path)?;
        let writable = tempfile::NamedTempFile::new_in(&path).map_err(|cause|
            with_cause(cause, Text::resource("ConfigDirectoryUnwritable", "Configuration directory $PATH$ is not writable: $ERROR$. The directory selection was not changed.")
                .replace("$PATH$", path.display().to_string())))?;
        drop(writable);
        let bookmark = Some(bookmark(&automatic.url)?);
        let _scope = bookmark.as_deref().map(restore).transpose()?;
        let path = match &_scope { Some(scope) => scope.directory()?, None => path };
        let bookmark = _scope.as_ref().and_then(|scope| scope.refreshed_bookmark.clone()).or(bookmark);
        persist(Selection::Custom { path, bookmark })?;
        Ok(true)
    }
}
