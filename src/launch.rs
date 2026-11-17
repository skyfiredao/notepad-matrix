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
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
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
            byte_code_page: None,
            udl_name: if matches!(buffer.language(), Language::UserDefined(_)) {
                Some(workspace.udl_language_name(buffer.language()).ok_or_else(|| invalid("Current user-defined language is not in the workspace registry"))?.to_owned())
            } else { None },
            line: Some(line.saturating_add(1)),
            column: Some(column.saturating_add(1)),
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
        use std::os::unix::ffi::OsStrExt;
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
        let arguments = options.arguments()?.into_iter().map(|argument| argument.as_bytes().to_vec()).collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&serde_json::json!({"arguments":arguments,"codepage":options.byte_code_page.map(ByteFormatCodePage::value)}))?;
        let mut connection = None;
        let mut last_error = None;
        for _ in 0..50 {
            match std::os::unix::net::UnixStream::connect(&path) {
                Ok(stream) => { connection = Some(stream); break; }
                Err(error) => { last_error = Some(error); std::thread::sleep(std::time::Duration::from_millis(100)); }
            }
        }
        let mut stream = connection.ok_or_else(|| last_error.unwrap_or_else(|| io::Error::other("running instance is unavailable")))?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
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
        use std::os::unix::ffi::OsStringExt;
        self.stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        let mut size = [0u8;8];
        self.stream.read_exact(&mut size)?;
        let size = usize::try_from(u64::from_le_bytes(size)).map_err(|_| invalid("launch request is too large"))?;
        if size > 8 * 1024 * 1024 { return Err(invalid("launch request is too large")); }
        let mut bytes = vec![0u8;size];
        self.stream.read_exact(&mut bytes)?;
        let payload: serde_json::Value = serde_json::from_slice(&bytes)?;
        let arguments: Vec<Vec<u8>> = serde_json::from_value(if payload.is_array() { payload.clone() } else { payload["arguments"].clone() })?;
        let mut options = LaunchOptions::parse(arguments.into_iter().map(OsString::from_vec))?;
        if payload.is_object() { options.byte_code_page = ByteFormatCodePage::from_value(&payload["codepage"])?; }
        Ok(LaunchRequest { options,stream:self.stream })
    }
}

#[cfg(unix)]
impl LaunchRequest {
    pub fn acknowledge(mut self, accepted: bool) -> io::Result<()> {
        use std::io::Write;
        self.stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
        self.stream.write_all(&[u8::from(accepted)])
    }
}
