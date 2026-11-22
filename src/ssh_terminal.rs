use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use russh::client;
use russh::keys::ssh_key::private::{Ed25519Keypair, Ed25519PrivateKey};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, ChannelMsg};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc as async_mpsc, watch};
use zeroize::Zeroizing;

use crate::config::ConfigStore;
use crate::file_access;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    System,
    Ssh,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Authentication {
    #[default]
    Password,
    Key,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub mode: Mode,
    pub authentication: Authentication,
    pub font_size: u16,
    pub background_color: u32,
    pub transparency: u8,
}

impl Profile {
    pub fn local() -> Self {
        Self {
            id: 0, name: "Local".into(), host: "localhost".into(), port: 22,
            username: String::new(), mode: Mode::System,
            authentication: Authentication::Password, font_size: 13, background_color: 0x000000, transparency: 0,
        }
    }

    pub fn remote(id: u64) -> Self {
        Self {
            id, name: format!("Server {id}"), host: String::new(), port: 22,
            username: String::new(), mode: Mode::Ssh,
            authentication: Authentication::Password, font_size: 13, background_color: 0x000000, transparency: 0,
        }
    }

    fn validate(&self) -> io::Result<()> {
        if self.port == 0 || !(8..=36).contains(&self.font_size) {
            return Err(invalid("terminal port must be 1..65535 and font size must be 8..36"));
        }
        if self.background_color > 0xffffff || self.transparency > 100 {
            return Err(invalid("terminal background color must be 0x000000..0xffffff and transparency must be 0..100"));
        }
        if self.id == 0 {
            if canonical_host(&self.host)? != "localhost" || self.port != 22 {
                return Err(invalid("local terminal must use localhost:22"));
            }
        } else {
            if self.mode != Mode::Ssh || self.name.trim().is_empty() || self.username.trim().is_empty()
                || self.username.chars().any(char::is_control)
            {
                return Err(invalid("remote terminal requires a name, username, and SSH mode"));
            }
            canonical_host(&self.host)?;
        }
        Ok(())
    }

    fn from_value(value: &serde_json::Value) -> io::Result<Self> {
        let text = |field: &str| value.get(field).and_then(serde_json::Value::as_str)
            .map(str::to_owned).ok_or_else(|| invalid(&format!("terminal profile {field} must be a string")));
        let number = |field: &str| value.get(field).and_then(serde_json::Value::as_u64)
            .ok_or_else(|| invalid(&format!("terminal profile {field} must be an unsigned integer")));
        let profile = Self {
            id: number("id")?, name: text("name")?, host: text("host")?,
            port: u16::try_from(number("port")?).map_err(failed)?, username: text("username")?,
            mode: read_mode(value)?, authentication: read_authentication(value)?,
            font_size: u16::try_from(number("font_size")?).map_err(failed)?,
            background_color: match value.get("background_color") {
                None => 0x000000,
                Some(_) => u32::try_from(number("background_color")?).map_err(failed)?,
            },
            transparency: match value.get("transparency") {
                None => 0,
                Some(_) => u8::try_from(number("transparency")?).map_err(failed)?,
            },
        };
        profile.validate()?;
        Ok(profile)
    }

    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "name": self.name, "host": self.host, "port": self.port,
            "username": self.username, "font_size": self.font_size,
            "background_color": self.background_color, "transparency": self.transparency,
            "mode": match self.mode { Mode::System => "system", Mode::Ssh => "ssh" },
            "authentication": match self.authentication { Authentication::Password => "password", Authentication::Key => "key" },
        })
    }
}

fn read_mode(value: &serde_json::Value) -> io::Result<Mode> {
    Ok(match value.get("mode").and_then(serde_json::Value::as_str) {
        Some("system") => Mode::System,
        Some("ssh") => Mode::Ssh,
        _ => return Err(invalid("ssh_terminal.mode must be system or ssh")),
    })
}

fn read_authentication(value: &serde_json::Value) -> io::Result<Authentication> {
    Ok(match value.get("authentication").and_then(serde_json::Value::as_str) {
        Some("password") => Authentication::Password,
        Some("key") => Authentication::Key,
        _ => return Err(invalid("ssh_terminal.authentication must be password or key")),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub local: Profile,
    pub servers: Vec<Profile>,
    pub paste_button: PasteButton,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PasteButton {
    Middle,
    #[default]
    Right,
}

impl Default for Settings {
    fn default() -> Self { Self { local: Profile::local(), servers: Vec::new(), paste_button: PasteButton::Right } }
}

impl Settings {
    pub fn load(store: &ConfigStore) -> io::Result<Self> {
        Self::from_settings(&store.load_settings()?)
    }

    fn from_settings(settings: &serde_json::Value) -> io::Result<Self> {
        let Some(value) = settings.get("ssh_terminal") else { return Ok(Self::default()); };
        let mut result = Self::default();
        result.paste_button = match value.get("paste_button") {
            None => PasteButton::Right,
            Some(button) => match button.as_str() {
                Some("middle") => PasteButton::Middle,
                Some("right") => PasteButton::Right,
                _ => return Err(invalid("ssh_terminal.paste_button must be middle or right")),
            },
        };
        if let Some(profiles) = value.get("profiles") {
            let profiles = profiles.as_array().ok_or_else(|| invalid("ssh_terminal.profiles must be an array"))?;
            let mut profiles = profiles.iter();
            result.local = Profile::from_value(profiles.next().ok_or_else(|| invalid("local terminal profile is required"))?)?;
            result.servers = profiles.map(Profile::from_value).collect::<io::Result<Vec<_>>>()?;
        } else {
            result.local.mode = read_mode(value)?;
            result.local.authentication = read_authentication(value)?;
        }
        result.validate()?;
        Ok(result)
    }

    fn validate(&self) -> io::Result<()> {
        if self.local.id != 0 { return Err(invalid("local terminal profile ID must be zero")); }
        self.local.validate()?;
        let mut ids = std::collections::HashSet::new();
        for profile in &self.servers {
            if profile.id == 0 || !ids.insert(profile.id) {
                return Err(invalid("remote terminal profile IDs must be nonzero and unique"));
            }
            profile.validate()?;
        }
        Ok(())
    }

    pub fn profile(&self, id: u64) -> Option<&Profile> {
        if id == 0 { Some(&self.local) } else { self.servers.iter().find(|profile| profile.id == id) }
    }

    pub fn profiles(&self) -> impl Iterator<Item = &Profile> {
        std::iter::once(&self.local).chain(self.servers.iter())
    }

    pub fn new_profile_id(&self) -> io::Result<u64> {
        loop {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes).map_err(failed)?;
            let id = u64::from_ne_bytes(bytes);
            if id != 0 && self.profile(id).is_none() { return Ok(id); }
        }
    }

    pub fn save(&self, store: &ConfigStore, expected: &Settings) -> io::Result<()> {
        self.validate()?;
        store.update_settings(|settings| {
            if Self::from_settings(settings)? != *expected {
                return Err(io::Error::other("Terminal configurations changed in another window; reopen the terminal configuration panel and try again"));
            }
            let object = settings.as_object_mut().ok_or_else(|| invalid("settings must be an object"))?;
            object.insert("ssh_terminal".into(), serde_json::json!({
                "profiles": self.profiles().map(Profile::to_value).collect::<Vec<_>>(),
                "paste_button": match self.paste_button { PasteButton::Middle => "middle", PasteButton::Right => "right" },
            }));
            Ok(())
        })?;
        Ok(())
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn failed(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

pub fn private_directory() -> io::Result<PathBuf> {
    let store = ConfigStore::under_default_home(&file_access::home_directory()?);
    let path = store.document_path(Path::new("ssh/state.json"))?;
    let directory = path.parent().ok_or_else(|| invalid("SSH directory has no parent"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        for path in [store.root(), directory] {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink()
                || metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "SSH storage must be an account-owned directory"));
            }
        }
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory.to_path_buf())
}

fn private_file(path: &Path, create: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("SSH storage must be a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "SSH files require account ownership and mode 0600"));
        }
    }
    Ok(file)
}

fn read_private(path: &Path) -> io::Result<Zeroizing<Vec<u8>>> {
    let file = private_file(path, false)?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(65_537).read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        return Err(invalid("SSH file is too large"));
    }
    Ok(bytes)
}

fn persist_new(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(directory.join(name)).map_err(|error| error.error)?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn load_key(directory: &Path) -> io::Result<PrivateKey> {
    let bytes = read_private(&directory.join("id_ed25519"))?;
    let key = PrivateKey::from_openssh(&*bytes).map_err(failed)?;
    if key.is_encrypted() || key.algorithm() != Algorithm::Ed25519 {
        return Err(invalid("SSH key must be an unencrypted Ed25519 key"));
    }
    Ok(key)
}

pub fn generate_key() -> io::Result<String> {
    let directory = private_directory()?;
    let lock = private_file(&directory.join("key.lock"), true)?;
    lock.lock()?;
    match fs::symlink_metadata(directory.join("id_ed25519")) {
        Ok(_) => return Err(io::Error::new(io::ErrorKind::AlreadyExists, "SSH key already exists")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut seed = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *seed).map_err(failed)?;
    let key = PrivateKey::from(Ed25519Keypair::from(Ed25519PrivateKey::from_bytes(&seed)));
    let public = key.public_key().to_openssh().map_err(failed)?;
    let encoded = key.to_openssh(LineEnding::LF).map_err(failed)?;
    persist_new(&directory, "id_ed25519", encoded.as_bytes())?;
    Ok(public)
}

pub fn public_key() -> io::Result<String> {
    load_key(&private_directory()?)?.public_key().to_openssh().map_err(failed)
}

pub fn registration_command() -> io::Result<String> {
    let public = public_key()?;
    let quoted = format!("'{}'", public.replace('\'', "'\\''"));
    Ok(format!("mkdir -p ~/.ssh && printf '\\n%s\\n' {quoted} >> ~/.ssh/authorized_keys"))
}

pub fn username() -> io::Result<String> {
    #[cfg(all(target_os = "macos", feature = "native-ui"))]
    {
        let name = objc2_foundation::NSUserName().to_string();
        if name.is_empty() {
            return Err(invalid("current account has no username"));
        }
        return Ok(name);
    }
    #[cfg(all(unix, not(all(target_os = "macos", feature = "native-ui"))))]
    {
        let mut buffer = vec![0u8; 4096];
        loop {
            let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
            let mut result = std::ptr::null_mut();
            let status = unsafe {
                libc::getpwuid_r(libc::getuid(), entry.as_mut_ptr(), buffer.as_mut_ptr().cast(), buffer.len(), &mut result)
            };
            if status == libc::ERANGE && buffer.len() < 65_536 {
                buffer.resize(buffer.len() * 2, 0);
                continue;
            }
            if status != 0 {
                return Err(io::Error::from_raw_os_error(status));
            }
            if result.is_null() {
                return Err(io::Error::new(io::ErrorKind::NotFound, "current account was not found"));
            }
            let entry = unsafe { entry.assume_init() };
            if entry.pw_name.is_null() {
                return Err(invalid("current account has no username"));
            }
            let name = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) }.to_str().map_err(failed)?;
            if name.is_empty() {
                return Err(invalid("current account has no username"));
            }
            return Ok(name.to_owned());
        }
    }
    #[cfg(not(unix))]
    Err(io::Error::new(io::ErrorKind::Unsupported, "SSH terminal requires a Unix account"))
}

#[derive(Debug)]
pub enum Event {
    HostKey { fingerprint: String },
    Connected,
    Data(Vec<u8>),
    Closed,
    Error(String),
}

enum Command {
    Input(Vec<u8>),
    Resize(u16, u16),
}

pub struct Session {
    pub events: mpsc::Receiver<Event>,
    commands: async_mpsc::Sender<Command>,
    trust: async_mpsc::Sender<bool>,
    closed: watch::Sender<bool>,
    host_waiting: watch::Sender<bool>,
}

impl Session {
    pub fn input(&self, data: Vec<u8>) -> io::Result<()> {
        if data.len() > 65_536 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "SSH input must be split into chunks of at most 65536 bytes"));
        }
        self.send(Command::Input(data))
    }

    pub fn resize(&self, columns: u16, rows: u16) -> io::Result<()> {
        dimensions(columns, rows)?;
        self.send(Command::Resize(columns, rows))
    }

    fn send(&self, command: Command) -> io::Result<()> {
        if *self.closed.borrow() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "SSH session is closed"));
        }
        self.commands.try_send(command).map_err(queue_error)
    }

    pub fn trust_host(&self, accept: bool) -> io::Result<()> {
        if *self.closed.borrow() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "SSH session is closed"));
        }
        if !*self.host_waiting.borrow() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "SSH is not waiting for host key approval"));
        }
        self.trust.try_send(accept).map_err(queue_error)
    }

    pub fn close(&self) {
        self.closed.send_replace(true);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

fn queue_error<T>(error: async_mpsc::error::TrySendError<T>) -> io::Error {
    match error {
        async_mpsc::error::TrySendError::Full(_) => io::Error::new(io::ErrorKind::WouldBlock, "SSH command queue is full"),
        async_mpsc::error::TrySendError::Closed(_) => io::Error::new(io::ErrorKind::BrokenPipe, "SSH session is closed"),
    }
}

fn dimensions(columns: u16, rows: u16) -> io::Result<()> {
    if columns == 0 || rows == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "SSH terminal dimensions must be nonzero"));
    }
    Ok(())
}

async fn cancelled(closed: &mut watch::Receiver<bool>) {
    loop {
        if *closed.borrow_and_update() || closed.changed().await.is_err() {
            return;
        }
    }
}

#[derive(Clone)]
struct Events {
    sender: mpsc::SyncSender<Event>,
    closed: watch::Receiver<bool>,
}

impl Events {
    async fn send(&self, mut event: Event) -> io::Result<()> {
        let mut closed = self.closed.clone();
        loop {
            if *closed.borrow() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "SSH session was closed"));
            }
            match self.sender.try_send(event) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(_)) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "SSH event receiver is closed")),
                Err(mpsc::TrySendError::Full(pending)) => event = pending,
            }
            tokio::select! {
                _ = cancelled(&mut closed) => return Err(io::Error::new(io::ErrorKind::Interrupted, "SSH session was closed")),
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }
}

#[derive(Debug)]
enum BackendError {
    Io(io::Error),
    Ssh(russh::Error),
}

impl From<io::Error> for BackendError {
    fn from(error: io::Error) -> Self { Self::Io(error) }
}

impl From<russh::Error> for BackendError {
    fn from(error: russh::Error) -> Self { Self::Ssh(error) }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => std::fmt::Display::fmt(error, formatter),
            Self::Ssh(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for BackendError {}

struct HostHandler {
    events: Events,
    trust: async_mpsc::Receiver<bool>,
    directory: PathBuf,
    host_waiting: watch::Sender<bool>,
    known: Option<PublicKey>,
    endpoint: String,
    pin_name: String,
    lock_name: String,
}

fn pinned_key(directory: &Path, name: &str) -> io::Result<Option<PublicKey>> {
    match read_private(&directory.join(name)) {
        Ok(bytes) => {
            let text = std::str::from_utf8(&bytes).map_err(failed)?;
            Ok(Some(PublicKey::from_openssh(text.trim()).map_err(failed)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn compare_host(endpoint: &str, pinned: &PublicKey, presented: &PublicKey) -> io::Result<()> {
    if pinned.key_data() != presented.key_data() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!(
            "{endpoint} host key changed; connection refused (pinned {}, received {})",
            pinned.fingerprint(HashAlg::Sha256), presented.fingerprint(HashAlg::Sha256),
        )));
    }
    Ok(())
}

impl client::Handler for HostHandler {
    type Error = BackendError;

    async fn check_server_key(&mut self, server_key: &russh::keys::PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let key = server_key.public_key();
        if let Some(pinned) = &self.known {
            compare_host(&self.endpoint, pinned, &key)?;
            return Ok(true);
        }
        if let Some(pinned) = pinned_key(&self.directory, &self.pin_name)? {
            compare_host(&self.endpoint, &pinned, &key)?;
            self.known = Some(key);
            self.trust.close();
            return Ok(true);
        }
        self.host_waiting.send_replace(true);
        self.events.send(Event::HostKey { fingerprint: key.fingerprint(HashAlg::Sha256).to_string() }).await?;
        let mut closed = self.events.closed.clone();
        let accepted = tokio::select! {
            biased;
            _ = cancelled(&mut closed) => false,
            reply = self.trust.recv() => reply == Some(true),
        };
        self.host_waiting.send_replace(false);
        self.trust.close();
        if !accepted {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{} host key was not approved", self.endpoint)).into());
        }
        let lock = private_file(&self.directory.join(&self.lock_name), true)?;
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::Error(error)) => return Err(error.into()),
                Err(fs::TryLockError::WouldBlock) => {
                    tokio::select! {
                        _ = cancelled(&mut closed) => return Err(io::Error::new(io::ErrorKind::Interrupted, "SSH session was closed").into()),
                        _ = tokio::time::sleep(Duration::from_millis(5)) => {}
                    }
                }
            }
        }
        if *closed.borrow() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "SSH session was closed").into());
        }
        if let Some(pinned) = pinned_key(&self.directory, &self.pin_name)? {
            compare_host(&self.endpoint, &pinned, &key)?;
        } else {
            let public = key.to_openssh().map_err(failed)?;
            persist_new(&self.directory, &self.pin_name, public.as_bytes())?;
        }
        self.known = Some(key);
        Ok(true)
    }

    async fn disconnected(&mut self, reason: client::DisconnectReason<Self::Error>) -> Result<(), Self::Error> {
        match reason {
            client::DisconnectReason::Error(error) => Err(error),
            client::DisconnectReason::ReceivedDisconnect(info) => {
                if matches!(info.reason_code, russh::Disconnect::ByApplication) {
                    Ok(())
                } else {
                    Err(failed(format!("SSH server disconnected ({:?}): {}", info.reason_code, info.message)).into())
                }
            }
        }
    }
}

pub fn connect(profile: Profile, password: Option<String>, columns: u16, rows: u16) -> io::Result<Session> {
    let password = password.map(Zeroizing::new);
    profile.validate()?;
    let authentication = profile.authentication;
    dimensions(columns, rows)?;
    if authentication == Authentication::Password && password.is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a password is required for each SSH connection"));
    }
    let password = match authentication {
        Authentication::Password => password,
        Authentication::Key => { drop(password); None }
    };
    let (sender, events) = mpsc::sync_channel(64);
    let (commands, command_receiver) = async_mpsc::channel(32);
    let (trust, trust_receiver) = async_mpsc::channel(1);
    let (closed, closed_receiver) = watch::channel(false);
    let thread_closed = closed.clone();
    let (host_waiting, _) = watch::channel(false);
    let thread_host_waiting = host_waiting.clone();
    std::thread::Builder::new().name("ssh-terminal".into()).spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => {
                let result = runtime.block_on(async {
                    let delivery = Events { sender: sender.clone(), closed: closed_receiver.clone() };
                    let mut cancel = closed_receiver.clone();
                    tokio::select! {
                        biased;
                        _ = cancelled(&mut cancel) => Ok(()),
                        result = run(profile, password, columns, rows, command_receiver, trust_receiver, delivery, thread_host_waiting.clone()) => result,
                    }
                });
                runtime.shutdown_background();
                result
            }
            Err(error) => Err(error),
        };
        thread_host_waiting.send_replace(false);
        thread_closed.send_replace(true);
        if let Err(error) = result {
            if sender.send(Event::Error(error.to_string())).is_err() {
                eprintln!("SSH error receiver is closed");
                return;
            }
        }
        if sender.send(Event::Closed).is_err() {
            eprintln!("SSH close receiver is closed");
        }
    })?;
    Ok(Session { events, commands, trust, closed, host_waiting })
}

async fn request_reply(channel: &mut Channel<client::Msg>, events: &Events, request: &str) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => return Ok(()),
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => events.send(Event::Data(data.to_vec())).await?,
                Some(ChannelMsg::Failure) => return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("SSH server refused {request}"))),
                Some(ChannelMsg::Close | ChannelMsg::Eof) | None => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, format!("SSH channel closed during {request}"))),
                Some(_) => {}
            }
        }
    }).await.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, format!("SSH {request} timed out")))?
}

async fn handshake(
    config: Arc<client::Config>,
    socket: tokio::net::TcpStream,
    handler: HostHandler,
    mut host_waiting: watch::Receiver<bool>,
) -> io::Result<client::Handle<HostHandler>> {
    let connection = client::connect_stream(config, socket, handler);
    tokio::pin!(connection);
    let mut remaining = Duration::from_secs(30);
    let mut paused = *host_waiting.borrow_and_update();
    loop {
        if paused {
            tokio::select! {
                biased;
                changed = host_waiting.changed() => {
                    changed.map_err(|_| failed("SSH host approval state is closed"))?;
                    paused = *host_waiting.borrow_and_update();
                }
                result = &mut connection => return result.map_err(failed),
            }
        } else {
            let started = tokio::time::Instant::now();
            tokio::select! {
                biased;
                changed = host_waiting.changed() => {
                    remaining = remaining.saturating_sub(started.elapsed());
                    changed.map_err(|_| failed("SSH host approval state is closed"))?;
                    paused = *host_waiting.borrow_and_update();
                }
                _ = tokio::time::sleep(remaining) => {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "SSH protocol handshake timed out after 30 seconds, excluding host key approval wait"));
                }
                result = &mut connection => return result.map_err(failed),
            }
        }
    }
}

async fn run(
    profile: Profile,
    password: Option<Zeroizing<String>>,
    columns: u16,
    rows: u16,
    mut commands: async_mpsc::Receiver<Command>,
    trust: async_mpsc::Receiver<bool>,
    events: Events,
    host_waiting: watch::Sender<bool>,
) -> io::Result<()> {
    let directory = private_directory()?;
    let authentication = profile.authentication;
    let user = if profile.id == 0 { username()? } else { profile.username.clone() };
    let host = if profile.id == 0 { "localhost".into() } else { canonical_host(&profile.host)? };
    let endpoint = if host.contains(':') { format!("[{host}]:{}", profile.port) } else { format!("{host}:{}", profile.port) };
    let (pin_name, lock_name) = host_files(&host, profile.port);
    let key = if authentication == Authentication::Key { Some(load_key(&directory)?) } else { None };
    let socket = tokio::time::timeout(Duration::from_secs(30), async {
        if host == "localhost" && profile.port == 22 { connect_loopback().await } else { connect_remote(&host, profile.port).await }
    }).await.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, format!("SSH DNS/TCP connection to {endpoint} timed out after 30 seconds")))??;
    socket.set_nodelay(true)?;
    let config = Arc::new(client::Config {
        window_size: 256 * 1024,
        maximum_packet_size: 32 * 1024,
        channel_buffer_size: 16,
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        nodelay: true,
        ..client::Config::default()
    });
    let host_waiting_receiver = host_waiting.subscribe();
    let handler = HostHandler { events: events.clone(), trust, directory, host_waiting, known: None, endpoint, pin_name, lock_name };
    let mut handle = handshake(config, socket, handler, host_waiting_receiver).await?;
    let authenticated = tokio::time::timeout(Duration::from_secs(30), async {
        match authentication {
            Authentication::Password => {
                let secret = password.as_ref().ok_or_else(|| invalid("SSH password is missing"))?;
                handle.authenticate_password(&user, secret.as_str()).await.map_err(failed)
            }
            Authentication::Key => {
                let key = key.ok_or_else(|| invalid("SSH key is missing"))?;
                handle.authenticate_publickey(&user, PrivateKeyWithHashAlg::new(Arc::new(key), None)).await.map_err(failed)
            }
        }
    }).await;
    drop(password);
    let authenticated = authenticated.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "SSH authentication timed out"))??;
    if !authenticated.success() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "SSH authentication failed"));
    }
    let mut channel = tokio::time::timeout(Duration::from_secs(15), handle.channel_open_session()).await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "SSH channel open timed out"))?.map_err(failed)?;
    channel.request_pty(true, "xterm-256color", u32::from(columns), u32::from(rows), 0, 0, &[]).await.map_err(failed)?;
    request_reply(&mut channel, &events, "PTY request").await?;
    channel.request_shell(true).await.map_err(failed)?;
    request_reply(&mut channel, &events, "shell request").await?;
    events.send(Event::Connected).await?;
    let (mut reader, writer) = channel.split();
    let mut writing = tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            match command {
                Command::Input(data) => writer.data_bytes(data).await.map_err(failed)?,
                Command::Resize(columns, rows) => writer.window_change(u32::from(columns), u32::from(rows), 0, 0).await.map_err(failed)?,
            }
        }
        Ok::<(), io::Error>(())
    });
    let mut exit_error = None;
    let result = loop {
        tokio::select! {
            biased;
            message = reader.wait() => match message {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if let Err(error) = events.send(Event::Data(data.to_vec())).await { break Err(error); }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    if exit_status != 0 { exit_error = Some(failed(format!("SSH shell exited with status {exit_status}"))); }
                }
                Some(ChannelMsg::ExitSignal { signal_name, error_message, .. }) => {
                    exit_error = Some(failed(format!("SSH shell ended by {signal_name:?}: {error_message}")));
                }
                Some(ChannelMsg::Close) => break match exit_error { Some(error) => Err(error), None => Ok(()) },
                None => {
                    break match handle.await {
                        Err(error) => Err(failed(error)),
                        Ok(()) => match exit_error { Some(error) => Err(error), None => Ok(()) },
                    };
                }
                Some(ChannelMsg::Failure) => break Err(failed("SSH channel request failed")),
                Some(_) => {}
            },
            result = &mut writing => break match result { Ok(result) => result, Err(error) => Err(failed(error)) },
            result = &mut handle => break match result {
                Err(error) => Err(failed(error)),
                Ok(()) => match exit_error { Some(error) => Err(error), None => Ok(()) },
            },
        }
    };
    writing.abort();
    result
}

fn canonical_host(host: &str) -> io::Result<String> {
    if host.starts_with('[') || host.ends_with(']') {
        let address = host.strip_prefix('[').and_then(|host| host.strip_suffix(']'))
            .ok_or_else(|| invalid("SSH host has invalid IPv6 brackets"))?;
        return address.parse::<std::net::Ipv6Addr>().map(|address| address.to_string()).map_err(failed);
    }
    if let Ok(address) = host.parse::<std::net::IpAddr>() { return Ok(address.to_string()); }
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if host.is_empty() || host.len() > 253 || host.split('.').any(|label| {
        label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
            || !label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return Err(invalid("SSH host must be a DNS name or IP address, without a URL or port"));
    }
    Ok(host)
}

fn host_files(host: &str, port: u16) -> (String, String) {
    if host == "localhost" && port == 22 {
        return ("localhost_22.pub".into(), "host.lock".into());
    }
    let hash = Sha256::digest(format!("{host}\0{port}").as_bytes());
    let mut name = String::with_capacity(64);
    for byte in hash {
        name.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        name.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    (format!("host_{name}.pub"), format!("host_{name}.lock"))
}

async fn connect_remote(host: &str, port: u16) -> io::Result<tokio::net::TcpStream> {
    let addresses = tokio::net::lookup_host((host, port)).await?;
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, format!("SSH host {host} has no addresses"));
    for address in addresses {
        match tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(address)).await {
            Ok(Ok(socket)) => return Ok(socket),
            Ok(Err(error)) => last_error = error,
            Err(_) => last_error = io::Error::new(io::ErrorKind::TimedOut, format!("SSH connection to {address} timed out")),
        }
    }
    Err(last_error)
}

async fn connect_loopback() -> io::Result<tokio::net::TcpStream> {
    let addresses = [
        std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 22)),
        std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 22)),
    ];
    let mut last_error = io::Error::new(io::ErrorKind::ConnectionRefused, "localhost:22 is unavailable");
    for address in addresses {
        match tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(address)).await {
            Ok(Ok(socket)) => return Ok(socket),
            Ok(Err(error)) => last_error = error,
            Err(_) => last_error = io::Error::new(io::ErrorKind::TimedOut, format!("SSH connection to {address} timed out")),
        }
    }
    Err(last_error)
}
