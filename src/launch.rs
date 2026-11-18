use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::editor::{ColumnPosition, Selection};
use crate::files::{ByteFormatCodePage, DiskFile};
use crate::syntax::Language;
use crate::workspace::{CloseDisposition, Workspace};
use ropey::LineType;

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub files: Vec<PathBuf>,
    pub no_session: bool,
    pub multi_instance: bool,
    pub open_session: bool,
    pub language: Option<Language>,
    pub byte_code_page: Option<ByteFormatCodePage>,
    pub udl_name: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub completion: Option<std::sync::Arc<LaunchCompletion>>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[derive(Debug)]
pub struct LaunchCompletion {
    result: std::sync::Mutex<Option<LaunchCompletionTarget>>,
}

#[derive(Debug)]
enum LaunchCompletionTarget {
    File(std::fs::File),
    #[cfg(unix)]
    Socket(std::os::unix::net::UnixStream),
}

impl LaunchCompletion {
    fn complete(&self, accepted: bool) -> io::Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        let mut result = self.result.lock().map_err(|_| io::Error::other("launch completion is unavailable"))?;
        match result.take() {
            Some(LaunchCompletionTarget::File(mut file)) => {
                file.seek(SeekFrom::Start(0))?;
                file.write_all(if accepted { b"1" } else { b"0" })?;
                file.sync_all()?;
            }
            #[cfg(unix)]
            Some(LaunchCompletionTarget::Socket(mut stream)) => {
                stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
                stream.write_all(&[u8::from(accepted)])?;
            }
            None => {},
        }
        Ok(())
    }
}

impl Drop for LaunchCompletion {
    fn drop(&mut self) {
        if let Err(error) = self.complete(false) { eprintln!("launch completion failed: {error}"); }
    }
}

#[cfg(unix)]
fn handoff_root() -> io::Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let home = crate::file_access::home_directory()?.canonicalize()?;
    let store = crate::config::ConfigStore::under_home(&home);
    let path = store.document_path(Path::new("launch-handoff/root.json"))?;
    let root = path.parent().ok_or_else(|| invalid("launch handoff directory is missing"))?.to_owned();
    let owner = std::fs::metadata(&home)?.uid();
    for path in [store.root(), root.as_path()] {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.uid() != owner || metadata.mode() & 0o077 != 0 {
            return Err(invalid("launch handoff directory is not private"));
        }
    }
    Ok(root)
}

#[cfg(unix)]
fn handoff_file(path: &Path, write: bool) -> io::Result<std::fs::File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let expected = std::fs::symlink_metadata(path)?;
    let owner = std::fs::metadata(handoff_root()?)?.uid();
    if !expected.is_file() || expected.file_type().is_symlink() || expected.uid() != owner || expected.mode() & 0o077 != 0 || expected.nlink() != 1 {
        return Err(invalid("launch handoff must be a private regular file"));
    }
    #[cfg(target_os = "macos")]
    let nofollow = 0x100;
    #[cfg(not(target_os = "macos"))]
    let nofollow = 0x20000;
    let file = std::fs::OpenOptions::new().read(true).write(write).custom_flags(nofollow).open(path)?;
    let actual = file.metadata()?;
    if expected.dev() != actual.dev() || expected.ino() != actual.ino() {
        return Err(invalid("launch handoff file changed"));
    }
    Ok(file)
}

#[cfg(unix)]
pub struct NativeHandoff {
    directory: tempfile::TempDir,
    result: std::fs::File,
    _source: std::fs::File,
    _access: crate::file_access::TransferGuard,
}

#[cfg(unix)]
impl NativeHandoff {
    pub fn create(options: &LaunchOptions) -> io::Result<Self> {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let directory = tempfile::Builder::new().prefix("request-")
            .permissions(std::fs::Permissions::from_mode(0o700)).tempdir_in(handoff_root()?)?;
        let create = |name: &str| std::fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(directory.path().join(name));
        let source = create("source")?;
        source.lock()?;
        let result = create("result")?;
        let mut options = options.clone();
        options.multi_instance = true;
        options.completion = None;
        for path in &mut options.files { *path = std::path::absolute(&*path)?; }
        let (mut payload, access) = options.payload()?;
        payload["version"] = 1.into();
        payload["request"] = directory.path().file_name().and_then(|name| name.to_str()).ok_or_else(|| invalid("launch handoff identifier is invalid"))?.into();
        payload["created"] = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_secs().into();
        let bytes = serde_json::to_vec(&payload)?;
        if bytes.len() > 8 * 1024 * 1024 { return Err(invalid("launch handoff is too large")); }
        let mut request = create("notepad-matrix-launch.json")?;
        request.write_all(&bytes)?;
        request.sync_all()?;
        Ok(Self { directory, result, _source: source, _access: access })
    }

    pub fn path(&self) -> PathBuf { self.directory.path().join("notepad-matrix-launch.json") }

    pub fn result(&mut self) -> io::Result<Option<bool>> {
        use std::io::{Read, Seek, SeekFrom};
        self.result.seek(SeekFrom::Start(0))?;
        let mut byte = [0u8; 1];
        match self.result.read(&mut byte)? {
            0 => Ok(None),
            _ if byte == *b"1" => Ok(Some(true)),
            _ if byte == *b"0" => Ok(Some(false)),
            _ => Err(invalid("invalid launch completion")),
        }
    }
}

#[cfg(unix)]
pub fn take_native_handoff(path: &Path) -> io::Result<Option<LaunchOptions>> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    if path.file_name().is_none_or(|name| name != "notepad-matrix-launch.json") { return Ok(None); }
    let root = handoff_root()?;
    let relative = match path.strip_prefix(&root) {
        Ok(relative) => relative,
        Err(_) => return Ok(None),
    };
    let components = relative.components().collect::<Vec<_>>();
    if components.len() != 2 || !components.iter().all(|component| matches!(component, std::path::Component::Normal(_))) {
        return Err(invalid("invalid launch handoff path"));
    }
    let directory = path.parent().ok_or_else(|| invalid("launch handoff directory is missing"))?;
    let identifier = directory.file_name().and_then(|name| name.to_str()).filter(|name| name.starts_with("request-") && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')).ok_or_else(|| invalid("launch handoff identifier is invalid"))?;
    let metadata = std::fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.mode() & 0o077 != 0 || metadata.uid() != std::fs::metadata(&root)?.uid() {
        return Err(invalid("launch handoff directory is not private"));
    }
    let source = handoff_file(&directory.join("source"), false)?;
    match source.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {},
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
        Ok(()) => return Err(invalid("launch handoff source is no longer available")),
    }
    let mut request = handoff_file(path, false)?;
    request.try_lock().map_err(|error| io::Error::other(error.to_string()))?;
    std::fs::remove_file(path)?;
    let completion = std::sync::Arc::new(LaunchCompletion { result: std::sync::Mutex::new(Some(LaunchCompletionTarget::File(handoff_file(&directory.join("result"), true)?))) });
    if request.metadata()?.len() > 8 * 1024 * 1024 { return Err(invalid("launch handoff is too large")); }
    let mut bytes = Vec::new();
    request.by_ref().take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 { return Err(invalid("launch handoff is too large")); }
    let payload: serde_json::Value = serde_json::from_slice(&bytes)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_secs();
    let created = payload["created"].as_u64().ok_or_else(|| invalid("launch handoff creation time is missing"))?;
    if payload["version"] != 1 || payload["request"].as_str() != Some(identifier) || created > now || now.saturating_sub(created) > 120 {
        return Err(invalid("launch handoff is invalid or expired"));
    }
    let mut options = LaunchOptions::from_payload(&payload)?;
    if !options.multi_instance { return Err(invalid("launch handoff must create a new instance")); }
    options.completion = Some(completion);
    Ok(Some(options))
}

fn language_name(language: Language) -> io::Result<Option<String>> {
    Ok(match language {
        Language::NormalText => Some("normal".into()),
        Language::Json => Some("json".into()),
        Language::Builtin(15) => return Err(invalid("User-defined language requires -udl=name")),
        Language::Builtin(id) => Some(
            crate::languages::catalog()
                .map_err(|error| invalid(error.to_string()))?
                .by_id(id)
                .ok_or_else(|| invalid("Unknown language"))?
                .name
                .clone(),
        ),
        Language::UserDefined(_) => return Err(invalid("User-defined language requires -udl=name")),
    })
}

impl LaunchOptions {
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> io::Result<Self> {
        let mut options = Self::default();
        let mut arguments = arguments.into_iter();
        let mut paths_only = false;
        while let Some(argument) = arguments.next() {
            let value = argument.to_str();
            if paths_only || value.is_none_or(|value| !value.starts_with('-')) {
                options.files.push(argument.into());
                continue;
            }
            let flag = value.ok_or_else(|| invalid("Invalid option encoding"))?;
            match flag {
                "--" => paths_only = true,
                "-multiInst" => options.multi_instance = true,
                "-openSession" => options.open_session = true,
                "-nosession" => options.no_session = true,
                "-noPlugin" => return Err(invalid("-noPlugin is excluded; no file was opened")),
                _ if flag.starts_with("-udl=") => {
                    let name = &flag[5..];
                    let name = name.strip_prefix('"').and_then(|name| name.strip_suffix('"')).unwrap_or(name);
                    if name.is_empty() { return Err(invalid("User-defined language name is missing")); }
                    options.udl_name = Some(name.to_owned());
                }
                _ => {
                    let (key, tail) = if let Some(tail) = flag.strip_prefix("-l") {
                        ('l', tail)
                    } else if let Some(tail) = flag.strip_prefix("-n") {
                        ('n', tail)
                    } else if let Some(tail) = flag.strip_prefix("-c") {
                        ('c', tail)
                    } else {
                        return Err(invalid(format!("Unsupported launch option: {flag}")));
                    };
                    let tail = if tail.is_empty() {
                        arguments
                            .next()
                            .ok_or_else(|| invalid(format!("Missing value for {flag}")))?
                    } else {
                        OsString::from(tail)
                    };
                    let tail = tail
                        .to_str()
                        .ok_or_else(|| invalid("Invalid option value encoding"))?;
                    if key == 'l' {
                        options.language = Some(match tail {
                            "normal" => Language::NormalText,
                            "json" => Language::Json,
                            _ => {
                                let id = crate::languages::catalog()
                                    .map_err(|error| invalid(error.to_string()))?
                                    .by_name(tail)
                                    .ok_or_else(|| invalid(format!("Unknown language: {tail}")))?
                                    .id;
                                if id == 15 { return Err(invalid("User-defined language requires -udl=name")); }
                                Language::Builtin(id)
                            }
                        });
                    } else {
                        let number = tail
                            .parse::<usize>()
                            .map_err(|_| invalid("Line and column must be unsigned integers"))?
                            .max(1);
                        if key == 'n' {
                            options.line = Some(number);
                        } else {
                            options.column = Some(number);
                        }
                    }
                }
            }
        }
        Ok(options)
    }

    pub fn for_transfer(workspace: &Workspace) -> io::Result<Option<Self>> {
        let buffer = workspace.active();
        let Some(file) = buffer.disk_file.as_ref().filter(|_| !buffer.is_modified()) else {
            return Ok(None);
        };
        let document = buffer.editor.document();
        let caret = document.selection().head;
        let line = document
            .text_snapshot()
            .byte_to_line_idx(caret.0, LineType::LF_CR);
        let column = document
            .display_column(caret, workspace.active_tab_width())
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Some(Self {
            files: vec![file.path().to_owned()],
            no_session: true,
            multi_instance: true,
            open_session: false,
            language: Some(buffer.language()),
            byte_code_page: buffer.byte_code_page(),
            udl_name: if matches!(buffer.language(), Language::UserDefined(_)) {
                Some(workspace.udl_language_name(buffer.language()).ok_or_else(|| invalid("Current user-defined language is not in the workspace registry"))?.to_owned())
            } else { None },
            line: Some(line.saturating_add(1)),
            column: Some(column.saturating_add(1)),
            completion: None,
        }))
    }

    pub fn arguments(&self) -> io::Result<Vec<OsString>> {
        let mut arguments = Vec::new();
        if self.multi_instance { arguments.push(OsString::from("-multiInst")); }
        if self.open_session { arguments.push(OsString::from("-openSession")); }
        if self.no_session {
            arguments.push("-nosession".into());
        }
        if let Some(name) = &self.udl_name {
            if name.is_empty() { return Err(invalid("User-defined language name is missing")); }
            arguments.push(format!("-udl={name}").into());
        } else if let Some(language) = self.language
            && let Some(name) = language_name(language)?
        {
            arguments.push(format!("-l{name}").into());
        }
        if let Some(line) = self.line {
            arguments.push(format!("-n{}", line.max(1)).into());
        }
        if let Some(column) = self.column {
            arguments.push(format!("-c{}", column.max(1)).into());
        }
        arguments.push("--".into());
        arguments.extend(self.files.iter().map(|path| path.as_os_str().to_owned()));
        Ok(arguments)
    }

    pub fn command(&self, executable: &Path) -> io::Result<Command> {
        if crate::file_access::sandboxed() {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "sandbox launches require the native asynchronous launcher"));
        }
        #[cfg(target_os = "macos")]
        if let Some(bundle) = executable.parent().filter(|path| path.file_name().is_some_and(|name| name == "MacOS"))
            .and_then(Path::parent).filter(|path| path.file_name().is_some_and(|name| name == "Contents"))
            .and_then(Path::parent).filter(|path| path.extension().is_some_and(|extension| extension == "app")) {
            let mut command = Command::new("/usr/bin/open");
            command.args(["-n","-a"]).arg(bundle).arg("--args");
            if !self.multi_instance { command.arg("-multiInst"); }
            command.args(self.arguments()?);
            return Ok(command);
        }
        let mut command = Command::new(executable);
        if !self.multi_instance { command.arg("-multiInst"); }
        command.args(self.arguments()?);
        Ok(command)
    }

    pub fn complete(&self, accepted: bool) -> io::Result<()> {
        if let Some(completion) = &self.completion { completion.complete(accepted)?; }
        Ok(())
    }

    #[cfg(unix)]
    fn payload(&self) -> io::Result<(serde_json::Value, crate::file_access::TransferGuard)> {
        use std::os::unix::ffi::OsStrExt;
        let arguments = self.arguments()?.into_iter().map(|argument| argument.as_bytes().to_vec()).collect::<Vec<_>>();
        let (bookmarks, access) = crate::file_access::transfer(&self.files)?;
        Ok((serde_json::json!({"arguments":arguments,"codepage":self.byte_code_page.map(ByteFormatCodePage::value),"bookmarks":bookmarks}), access))
    }

    #[cfg(unix)]
    fn from_payload(payload: &serde_json::Value) -> io::Result<Self> {
        use std::os::unix::ffi::OsStringExt;
        let arguments: Vec<Vec<u8>> = serde_json::from_value(if payload.is_array() { payload.clone() } else { payload["arguments"].clone() })?;
        let mut options = Self::parse(arguments.into_iter().map(OsString::from_vec))?;
        if payload.is_object() {
            options.byte_code_page = ByteFormatCodePage::from_value(&payload["codepage"])?;
            if crate::file_access::sandboxed() { crate::file_access::receive(&payload["bookmarks"])?; }
        } else if crate::file_access::sandboxed() {
            return Err(invalid("sandbox launch request has no file access bookmarks"));
        }
        Ok(options)
    }

    pub fn spawn(&self, executable: &Path) -> io::Result<Child> {
        self.command(executable)?
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }

    pub fn open_into(&self, workspace: &mut Workspace) -> io::Result<Vec<String>> {
        if self.open_session {
            if self.files.len() != 1 { return Err(invalid("-openSession requires exactly one session path")); }
            return crate::recovery::SessionFile::load(&self.files[0])?.restore_into(workspace);
        }
        let language = self.resolved_language(workspace)?;
        let initial = workspace.active();
        let placeholder = (workspace.buffers().len() == 1
            && initial.disk_file.is_none()
            && !initial.is_modified()
            && initial.editor.document().len_bytes() == 0)
            .then_some(initial.id());
        let mut warnings = Vec::new();
        let mut opened = false;
        for path in &self.files {
            match self.load_file_buffer(path,workspace.file_settings()) {
                Ok((file, bytes)) => {
                    workspace
                        .open_buffer_bytes(file, bytes)
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    if let Some(language) = language {
                        workspace.active_mut().set_language(language);
                    }
                    workspace.refresh_udl_metadata();
                    opened = true;
                }
                Err(error) => warnings.push(format!("{}: {error}", path.display())),
            }
        }
        if let Some(initial) = placeholder.filter(|_| opened && workspace.buffers().len() > 1) {
            workspace
                .close_buffer(initial, CloseDisposition::IfUnmodified)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        if opened || self.files.is_empty() {
            if self.files.is_empty()
                && let Some(language) = language
            {
                workspace.active_mut().set_language(language);
            }
            workspace.refresh_udl_metadata();
            if self.line.is_some() || self.column.is_some() {
                let tab_width = workspace.active_tab_width();
                let document = workspace.active().editor.document();
                let current = document.selection().head;
                let line = self
                    .line
                    .map_or_else(
                        || {
                            document
                                .text_snapshot()
                                .byte_to_line_idx(current.0, LineType::LF_CR)
                        },
                        |line| line.saturating_sub(1),
                    )
                    .min(document.line_count().saturating_sub(1));
                let column = self.column.map_or(0, |column| column.saturating_sub(1));
                let (position, _) = document
                    .position_at_column(ColumnPosition { line, column }, tab_width)
                    .map_err(|error| io::Error::other(error.to_string()))?;
                workspace
                    .active_mut()
                    .editor
                    .select(Selection {
                        anchor: position,
                        head: position,
                    })
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
        }
        Ok(warnings)
    }

    pub fn load_file(&self, path: &Path, settings: &serde_json::Value) -> io::Result<(DiskFile, String)> {
        if let Some(codepage) = self.byte_code_page {
            ByteFormatCodePage::from_value(&codepage.value())?;
            DiskFile::load_with_code_page(path,Some(codepage))
        } else { DiskFile::load_with_preferences(path,settings) }
    }

    pub(crate) fn load_file_buffer(&self, path: &Path, settings: &serde_json::Value) -> io::Result<(DiskFile, crate::byte_buffer::ByteBuffer)> {
        if let Some(codepage) = self.byte_code_page {
            ByteFormatCodePage::from_value(&codepage.value())?;
            DiskFile::load_selected_encoding(path, Some(codepage))
        } else {
            DiskFile::load_buffer_with_preferences(path, settings)
        }
    }

    pub fn resolved_language(&self, workspace: &Workspace) -> io::Result<Option<Language>> {
        if let Some(name) = &self.udl_name {
            return workspace.udl_language_named(name).map(Some).ok_or_else(|| invalid(format!("Unknown user-defined language: {name}")));
        }
        if let Some(language) = self.language {
            if language == Language::Builtin(15) { return Err(invalid("User-defined language requires -udl=name")); }
            if let Language::Builtin(id) = language {
                crate::languages::catalog().map_err(|error| invalid(error.to_string()))?.by_id(id).ok_or_else(|| invalid("Unknown language"))?;
            }
            if matches!(language,Language::UserDefined(_)) && workspace.udl_language_name(language).is_none() {
                return Err(invalid("User-defined language is not in the workspace registry"));
            }
        }
        Ok(self.language)
    }
}

#[cfg(unix)]
pub enum InstanceStartup {
    Forwarded,
    Primary(InstanceServer),
    Secondary,
}

#[cfg(unix)]
pub struct InstanceServer {
    listener: std::os::unix::net::UnixListener,
    path: PathBuf,
    _lease: std::fs::File,
}

#[cfg(unix)]
pub struct PendingLaunch {
    stream: std::os::unix::net::UnixStream,
}

#[cfg(unix)]
pub struct LaunchRequest {
    pub options: LaunchOptions,
    stream: std::os::unix::net::UnixStream,
}

#[cfg(unix)]
impl InstanceServer {
    pub fn prepare(home: &Path, options: &mut LaunchOptions, settings: &serde_json::Value) -> io::Result<InstanceStartup> {
        use std::io::{Read,Write};
        use std::os::unix::fs::OpenOptionsExt;
        let store = crate::config::ConfigStore::under_home(home);
        let root = store.document_path(Path::new("instance.json"))?.parent().ok_or_else(|| invalid("instance directory is missing"))?.to_owned();
        for path in &mut options.files { *path = std::path::absolute(&*path)?; }
        let path = root.join("instance.sock");
        let lease = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(root.join("instance.lock"))?;
        match lease.try_lock() {
            Ok(()) => {
                match std::fs::symlink_metadata(&path) {
                    Ok(metadata) => {
                        use std::os::unix::fs::FileTypeExt;
                        if !metadata.file_type().is_socket() { return Err(invalid("instance.sock is not a socket")); }
                        std::fs::remove_file(&path)?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                    Err(error) => return Err(error),
                }
                let listener = std::os::unix::net::UnixListener::bind(&path)?;
                listener.set_nonblocking(true)?;
                return Ok(InstanceStartup::Primary(Self { listener,path,_lease:lease }));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
            Err(std::fs::TryLockError::WouldBlock) => {},
        }
        let mode = settings["multiInst"]["setting"].as_i64().filter(|mode| (0..=2).contains(mode)).unwrap_or(0);
        if options.multi_instance || mode == 2 {
            if mode == 2 { options.no_session = true; }
            return Ok(InstanceStartup::Secondary);
        }
        let (payload, _access) = options.payload()?;
        let bytes = serde_json::to_vec(&payload)?;
        let mut connection = None;
        let mut last_error = None;
        for _ in 0..50 {
            match std::os::unix::net::UnixStream::connect(&path) {
                Ok(stream) => { connection = Some(stream); break; }
                Err(error) => { last_error = Some(error); std::thread::sleep(std::time::Duration::from_millis(100)); }
            }
        }
        let mut stream = connection.ok_or_else(|| last_error.unwrap_or_else(|| io::Error::other("running instance is unavailable")))?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(120)))?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
        stream.write_all(&(bytes.len() as u64).to_le_bytes())?;
        stream.write_all(&bytes)?;
        let mut accepted = [0u8;1];
        stream.read_exact(&mut accepted)?;
        if accepted != [1] { return Err(io::Error::other("running instance rejected the launch request")); }
        Ok(InstanceStartup::Forwarded)
    }

    pub fn accept(&self) -> io::Result<Option<PendingLaunch>> {
        match self.listener.accept() {
            Ok((stream,_)) => Ok(Some(PendingLaunch { stream })),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(unix)]
impl Drop for InstanceServer {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound { eprintln!("instance socket cleanup failed: {error}"); }
    }
}

#[cfg(unix)]
impl PendingLaunch {
    pub fn read(mut self) -> io::Result<LaunchRequest> {
        use std::io::Read;
        self.stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        let mut size = [0u8;8];
        self.stream.read_exact(&mut size)?;
        let size = usize::try_from(u64::from_le_bytes(size)).map_err(|_| invalid("launch request is too large"))?;
        if size > 8 * 1024 * 1024 { return Err(invalid("launch request is too large")); }
        let mut bytes = vec![0u8;size];
        self.stream.read_exact(&mut bytes)?;
        let payload: serde_json::Value = serde_json::from_slice(&bytes)?;
        let options = LaunchOptions::from_payload(&payload)?;
        Ok(LaunchRequest { options,stream:self.stream })
    }
}

#[cfg(unix)]
impl LaunchRequest {
    pub fn into_options(self) -> LaunchOptions {
        let mut options = self.options;
        options.completion = Some(std::sync::Arc::new(LaunchCompletion {
            result: std::sync::Mutex::new(Some(LaunchCompletionTarget::Socket(self.stream))),
        }));
        options
    }

    pub fn acknowledge(mut self, accepted: bool) -> io::Result<()> {
        use std::io::Write;
        self.stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
        self.stream.write_all(&[u8::from(accepted)])
    }
}
