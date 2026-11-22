use crate::config::ConfigStore;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

pub const MAX_RETENTION_DAYS: u64 = (i64::MAX as u64) / 86_400;
const REGISTRY: &str = "backup-retention.json";
const RECOVERY_LOCK: &str = ".backup-recovery-maintenance.lock";
const SAVE_LOCK: &str = ".backup-save-maintenance.lock";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackupRetentionOptions {
    pub enabled: bool,
    pub days: u64,
}

impl Default for BackupRetentionOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            days: 90,
        }
    }
}

impl BackupRetentionOptions {
    pub fn from_settings(settings: &Value) -> io::Result<Self> {
        let Some(value) = settings.get("backup_retention") else {
            return Ok(Self::default());
        };
        if !value.is_object() {
            return Err(invalid("backup_retention must be an object"));
        }
        let enabled = match value.get("enabled") {
            None => true,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| invalid("backup_retention.enabled must be a boolean"))?,
        };
        let days = match value.get("days") {
            None => 90,
            Some(value) => value
                .as_u64()
                .filter(|days| (1..=MAX_RETENTION_DAYS).contains(days))
                .ok_or_else(|| {
                    invalid("backup_retention.days must be a positive supported integer")
                })?,
        };
        Ok(Self { enabled, days })
    }

    fn duration(self) -> io::Result<Duration> {
        if !(1..=MAX_RETENTION_DAYS).contains(&self.days) {
            return Err(invalid(
                "backup_retention.days is outside the supported range",
            ));
        }
        Ok(Duration::from_secs(
            self.days
                .checked_mul(86_400)
                .ok_or_else(|| invalid("backup retention duration overflow"))?,
        ))
    }
}

#[derive(Debug)]
pub struct CleanupFailure {
    pub path: PathBuf,
    pub kind: io::ErrorKind,
}

#[derive(Debug, Default)]
pub struct CleanupReport {
    pub removed_raw: u64,
    pub removed_save_backups: u64,
    pub pruned_entries: u64,
    pub retained: u64,
    pub busy_recovery: u64,
    pub busy_save: u64,
    pub skipped_raw_scans: u64,
    pub cancelled: bool,
    pub failures: Vec<CleanupFailure>,
}

impl CleanupReport {
    fn failure(&mut self, path: &Path, error: &io::Error) {
        self.failures.push(CleanupFailure {
            path: path.to_owned(),
            kind: error.kind(),
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl Identity {
    fn from_metadata(metadata: &Metadata) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
                changed_seconds: metadata.ctime(),
                changed_nanoseconds: metadata.ctime_nsec(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "backup identity requires Unix file metadata",
            ))
        }
    }

    fn parse(value: &Value) -> io::Result<Self> {
        let unsigned = |field| {
            value
                .get(field)
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("managed backup identity is missing"))
        };
        let signed = |field| {
            value
                .get(field)
                .and_then(Value::as_i64)
                .ok_or_else(|| invalid("managed backup change time is missing"))
        };
        let identity = Self {
            device: unsigned("device")?,
            inode: unsigned("inode")?,
            changed_seconds: signed("changed_seconds")?,
            changed_nanoseconds: signed("changed_nanoseconds")?,
        };
        if identity.inode == 0 || !(0..1_000_000_000).contains(&identity.changed_nanoseconds) {
            return Err(invalid("invalid managed backup identity"));
        }
        Ok(identity)
    }

    fn value(&self) -> Value {
        json!({"device":self.device,"inode":self.inode,"changed_seconds":self.changed_seconds,"changed_nanoseconds":self.changed_nanoseconds})
    }

    fn same_file(&self, other: &Self) -> bool {
        self.device == other.device && self.inode == other.inode
    }
}

struct ManagedBackup {
    path: PathBuf,
    identity: Identity,
    parent: Identity,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn regular(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid("backup metadata requires a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid("backup metadata must not be hard linked"));
        }
    }
    Ok(())
}

fn directory_identity(path: &Path) -> io::Result<Identity> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid(
            "backup parent must be a directory, not a symbolic link",
        ));
    }
    Identity::from_metadata(&metadata)
}

fn open_checked(path: &Path, write: bool, create: bool) -> io::Result<File> {
    let previous = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            regular(&metadata)?;
            Some(metadata)
        }
        Err(error) if create && error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut options = OpenOptions::new();
    options.read(true).write(write);
    if previous.is_none() {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    regular(&opened)?;
    let current = fs::symlink_metadata(path)?;
    regular(&current)?;
    let identity = Identity::from_metadata(&opened)?;
    let matches = |other: &Identity| {
        if write {
            identity.same_file(other)
        } else {
            &identity == other
        }
    };
    if !matches(&Identity::from_metadata(&current)?)
        || previous
            .as_ref()
            .map(Identity::from_metadata)
            .transpose()?
            .is_some_and(|previous| !matches(&previous))
    {
        return Err(invalid("backup file identity changed while opening"));
    }
    Ok(file)
}

fn maintenance_file(store: &ConfigStore, name: &str, create_root: bool) -> io::Result<File> {
    if create_root {
        store.document_path(Path::new(REGISTRY))?;
    } else {
        directory_identity(store.root())?;
    }
    let path = store.root().join(name);
    match open_checked(&path, true, true) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            open_checked(&path, true, false)
        }
        result => result,
    }
}

fn verify_lock(store: &ConfigStore, name: &str, file: &File) -> io::Result<()> {
    let current = fs::symlink_metadata(store.root().join(name))?;
    regular(&current)?;
    if !Identity::from_metadata(&current)?.same_file(&Identity::from_metadata(&file.metadata()?)?) {
        return Err(invalid("backup maintenance lock was replaced"));
    }
    Ok(())
}

fn lock(store: &ConfigStore, name: &str) -> io::Result<File> {
    let file = maintenance_file(store, name, true)?;
    file.lock()?;
    verify_lock(store, name, &file)?;
    Ok(file)
}

fn try_lock(store: &ConfigStore, name: &str) -> io::Result<Option<File>> {
    let file = maintenance_file(store, name, false)?;
    match file.try_lock() {
        Ok(()) => {
            verify_lock(store, name, &file)?;
            Ok(Some(file))
        }
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(error)) => Err(error),
    }
}

pub fn recovery_lock(store: &ConfigStore) -> io::Result<File> {
    let file = lock(store, RECOVERY_LOCK)?;
    advance_recovery_generation(store, &file)?;
    Ok(file)
}
pub fn try_recovery_lock(store: &ConfigStore) -> io::Result<Option<File>> {
    try_lock(store, RECOVERY_LOCK)
}
pub(crate) fn save_backup_lock(store: &ConfigStore) -> io::Result<File> {
    lock(store, SAVE_LOCK)
}

fn recovery_generation(store: &ConfigStore, file: &File) -> io::Result<u64> {
    verify_lock(store, RECOVERY_LOCK, file)?;
    let generation = match file.metadata()?.len() {
        0 => 0,
        8 => {
            let mut reader = file;
            reader.rewind()?;
            let mut bytes = [0; 8];
            reader.read_exact(&mut bytes)?;
            u64::from_be_bytes(bytes)
        }
        _ => return Err(invalid("invalid recovery maintenance generation")),
    };
    verify_lock(store, RECOVERY_LOCK, file)?;
    Ok(generation)
}

fn advance_recovery_generation(store: &ConfigStore, file: &File) -> io::Result<u64> {
    let generation = recovery_generation(store, file)?
        .checked_add(1)
        .ok_or_else(|| invalid("recovery maintenance generation exhausted"))?;
    let mut writer = file;
    writer.rewind()?;
    writer.write_all(&generation.to_be_bytes())?;
    file.sync_data()?;
    verify_lock(store, RECOVERY_LOCK, file)?;
    Ok(generation)
}

fn recovery_generation_matches(
    store: &ConfigStore,
    file: &File,
    identity: &Identity,
    generation: u64,
) -> io::Result<bool> {
    if !identity.same_file(&Identity::from_metadata(&file.metadata()?)?) {
        return Err(invalid("recovery maintenance lock identity changed"));
    }
    Ok(recovery_generation(store, file)? == generation)
}

fn normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

fn parse_registry(document: &Value) -> io::Result<Vec<ManagedBackup>> {
    if document.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(invalid(
            "backup retention registry requires schema_version 1",
        ));
    }
    let entries = document
        .get("managed")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("managed backup registry must be an array"))?;
    let mut paths = BTreeSet::new();
    let mut identities = BTreeSet::new();
    entries
        .iter()
        .map(|entry| {
            let path = PathBuf::from(
                entry
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("managed backup path is missing"))?,
            );
            if !normalized_absolute(&path)
                || path.extension() != Some(std::ffi::OsStr::new("bak"))
                || !paths.insert(path.clone())
            {
                return Err(invalid("invalid or duplicate managed backup path"));
            }
            let identity = Identity::parse(&entry["identity"])?;
            let parent = Identity::parse(&entry["parent_identity"])?;
            if !identities.insert((identity.device, identity.inode)) {
                return Err(invalid("managed backups have duplicate file identities"));
            }
            Ok(ManagedBackup {
                path,
                identity,
                parent,
            })
        })
        .collect()
}

fn read_metadata_json(path: &Path) -> io::Result<Value> {
    read_metadata_json_with_identity(path).map(|(document, _)| document)
}

fn read_metadata_json_with_identity(path: &Path) -> io::Result<(Value, Identity)> {
    let file = open_checked(path, false, false)?;
    let previous = Identity::from_metadata(&file.metadata()?)?;
    let document = serde_json::from_reader(&file).map_err(|error| invalid(&error.to_string()))?;
    if previous != Identity::from_metadata(&file.metadata()?)?
        || previous != Identity::from_metadata(&fs::symlink_metadata(path)?)?
    {
        return Err(invalid("backup metadata changed while reading"));
    }
    Ok((document, previous))
}

fn load_registry(store: &ConfigStore) -> io::Result<Value> {
    directory_identity(store.root())?;
    let path = store.root().join(REGISTRY);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => regular(&metadata)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(json!({"schema_version":1,"managed":[]}));
        }
        Err(error) => return Err(error),
    }
    let document = read_metadata_json(&path)?;
    parse_registry(&document)?;
    Ok(document)
}

pub(crate) fn validate_registry(store: &ConfigStore) -> io::Result<()> {
    load_registry(store).map(|_| ())
}

pub(crate) fn register_backup(store: &ConfigStore, path: &Path, created: &File) -> io::Result<()> {
    let parent_path = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let _access = crate::file_access::directory(parent_path)?;
    let parent_path = fs::canonicalize(parent_path)?;
    let filename = path
        .file_name()
        .ok_or_else(|| invalid("managed backup filename is missing"))?;
    let path = parent_path.join(filename);
    let parent = directory_identity(&parent_path)?;
    let metadata = created.metadata()?;
    regular(&metadata)?;
    let identity = Identity::from_metadata(&metadata)?;
    let current = fs::symlink_metadata(&path)?;
    regular(&current)?;
    if identity != Identity::from_metadata(&current)? {
        return Err(invalid("created backup was replaced before registration"));
    }
    let path_text = path
        .to_str()
        .ok_or_else(|| invalid("managed backup path must be UTF-8"))?;
    let entry =
        json!({"path":path_text,"identity":identity.value(),"parent_identity":parent.value()});
    let defaults = json!({"schema_version":1,"managed":[]});
    store.update_json(Path::new(REGISTRY), &defaults, |document| {
        parse_registry(document)?;
        let entries = document
            .get_mut("managed")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("managed backup registry must be an array"))?;
        entries.retain(|entry| entry.get("path").and_then(Value::as_str) != Some(path_text));
        entries.push(entry);
        parse_registry(document)?;
        Ok(())
    })?;
    Ok(())
}

fn continue_cleanup(
    store: &ConfigStore,
    options: BackupRetentionOptions,
    cancelled: &AtomicBool,
    report: &mut CleanupReport,
) -> io::Result<bool> {
    if cancelled.load(Ordering::Acquire) {
        report.cancelled = true;
        return Ok(false);
    }
    let current = BackupRetentionOptions::from_settings(&cleanup_settings(store)?)?;
    if !current.enabled || current != options {
        report.cancelled = true;
        return Ok(false);
    }
    Ok(true)
}

fn cleanup_settings(store: &ConfigStore) -> io::Result<Value> {
    let root = directory_identity(store.root())?;
    let settings = read_metadata_json(&store.root().join("settings.json"))?;
    if !root.same_file(&directory_identity(store.root())?) {
        return Err(invalid("configuration root changed during cleanup"));
    }
    if settings.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(invalid("settings.json requires schema_version 1"));
    }
    Ok(settings)
}

fn expired(metadata: &Metadata, now: SystemTime, duration: Duration) -> io::Result<bool> {
    Ok(now
        .duration_since(metadata.modified()?)
        .is_ok_and(|age| age > duration))
}

fn remove_checked(
    store: &ConfigStore,
    path: &Path,
    identity: &Identity,
    parent: &Identity,
    options: BackupRetentionOptions,
    now: Option<SystemTime>,
    cancelled: &AtomicBool,
    report: &mut CleanupReport,
) -> io::Result<bool> {
    if cancelled.load(Ordering::Acquire) {
        report.cancelled = true;
        return Ok(false);
    }
    let directory = path
        .parent()
        .ok_or_else(|| invalid("backup parent is missing"))?;
    if fs::canonicalize(directory)? != directory
        || !parent.same_file(&directory_identity(directory)?)
    {
        return Err(invalid("backup parent identity changed"));
    }
    let current = fs::symlink_metadata(path)?;
    regular(&current)?;
    if identity != &Identity::from_metadata(&current)? {
        return Err(invalid("backup identity changed before deletion"));
    }
    if let Some(now) = now {
        if !expired(&current, now, options.duration()?)? {
            return Ok(false);
        }
    }
    if !continue_cleanup(store, options, cancelled, report)? {
        return Ok(false);
    }
    let rechecked = fs::symlink_metadata(path)?;
    regular(&rechecked)?;
    if identity != &Identity::from_metadata(&rechecked)?
        || fs::canonicalize(directory)? != directory
        || !parent.same_file(&directory_identity(directory)?)
    {
        return Err(invalid("backup identity changed during deletion check"));
    }
    if cancelled.load(Ordering::Acquire) {
        report.cancelled = true;
        return Ok(false);
    }
    fs::remove_file(path)?;
    Ok(true)
}

pub fn cleanup(
    store: &ConfigStore,
    now: SystemTime,
    cancelled: &AtomicBool,
) -> io::Result<CleanupReport> {
    let mut report = CleanupReport::default();
    if cancelled.load(Ordering::Acquire) {
        report.cancelled = true;
        return Ok(report);
    }
    let options = BackupRetentionOptions::from_settings(&cleanup_settings(store)?)?;
    options.duration()?;
    if !options.enabled || !continue_cleanup(store, options, cancelled, &mut report)? {
        return Ok(report);
    }
    cleanup_raw(store, options, now, cancelled, &mut report)?;
    if !continue_cleanup(store, options, cancelled, &mut report)? {
        return Ok(report);
    }
    match try_lock(store, SAVE_LOCK)? {
        Some(_lock) => cleanup_managed(store, options, now, cancelled, &mut report)?,
        None => report.busy_save += 1,
    }
    Ok(report)
}

fn cleanup_managed(
    store: &ConfigStore,
    options: BackupRetentionOptions,
    now: SystemTime,
    cancelled: &AtomicBool,
    report: &mut CleanupReport,
) -> io::Result<()> {
    let document = load_registry(store)?;
    let entries = parse_registry(&document)?;
    let mut pruned = BTreeSet::new();
    for entry in entries {
        if !continue_cleanup(store, options, cancelled, report)? {
            break;
        }
        let result = (|| -> io::Result<bool> {
            let parent = entry
                .path
                .parent()
                .ok_or_else(|| invalid("managed backup parent is missing"))?;
            let _access = crate::file_access::directory(parent)?;
            if fs::canonicalize(parent)? != parent
                || !entry.parent.same_file(&directory_identity(parent)?)
            {
                return Err(invalid("managed backup parent identity changed"));
            }
            let metadata = match fs::symlink_metadata(&entry.path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    pruned.insert(entry.path.clone());
                    return Ok(false);
                }
                Err(error) => return Err(error),
            };
            regular(&metadata)?;
            if entry.identity != Identity::from_metadata(&metadata)? {
                return Err(invalid("managed backup file identity no longer matches"));
            }
            if !expired(&metadata, now, options.duration()?)? {
                return Ok(false);
            }
            remove_checked(
                store,
                &entry.path,
                &entry.identity,
                &entry.parent,
                options,
                Some(now),
                cancelled,
                report,
            )
        })();
        match result {
            Ok(true) => {
                report.removed_save_backups += 1;
                pruned.insert(entry.path);
            }
            Ok(false) if !pruned.contains(&entry.path) => report.retained += 1,
            Ok(false) => {}
            Err(error) => report.failure(&entry.path, &error),
        }
    }
    if !pruned.is_empty() {
        directory_identity(store.root())?;
        store.update_json(Path::new(REGISTRY), &document, |current| {
            parse_registry(current)?;
            if *current != document {
                return Err(invalid("managed backup registry changed during cleanup"));
            }
            let entries = current
                .get_mut("managed")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| invalid("managed backup registry must be an array"))?;
            entries.retain(|entry| {
                !entry
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| pruned.contains(Path::new(path)))
            });
            Ok(())
        })?;
        report.pruned_entries += pruned.len() as u64;
    }
    Ok(())
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_raw_name(name: &str) -> bool {
    name.rsplit_once('@').is_some_and(|(name, timestamp)| {
        !name.is_empty()
            && timestamp.len() == 17
            && timestamp
                .bytes()
                .enumerate()
                .all(|(index, byte)| match index {
                    4 | 7 => byte == b'-',
                    10 => byte == b'_',
                    _ => byte.is_ascii_digit(),
                })
    })
}

fn metadata_backup_path(directory: &Path, key: &str, document: &Value) -> io::Result<PathBuf> {
    if !valid_key(key)
        || document.get("schema_version").and_then(Value::as_u64) != Some(1)
        || document.get("text").is_some()
        || document
            .get("disk")
            .is_some_and(|disk| disk.get("bytes").is_some())
    {
        return Err(invalid("unknown or legacy recovery metadata"));
    }
    let name = document
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("missing recovery backup name"))?;
    for field in ["anchor", "head"] {
        if document.get(field).and_then(Value::as_u64).is_none() {
            return Err(invalid("invalid recovery metadata position"));
        }
    }
    if !matches!(
        document.get("format").and_then(Value::as_str),
        Some("utf8" | "utf8-bom" | "utf16-le-bom" | "utf16-be-bom")
    ) || !matches!(
        document.get("line_ending").and_then(Value::as_str),
        Some("\r\n" | "\n" | "\r")
    ) || !matches!(document.get("disk"), Some(Value::Null | Value::Object(_)))
    {
        return Err(invalid(
            "invalid recovery metadata encoding or disk association",
        ));
    }
    crate::files::ByteFormatCodePage::from_value(&document["codepage"])?;
    validate_snapshot_fields(document)?;
    let path = PathBuf::from(
        document
            .get("backupFilePath")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("missing recovery backup path"))?,
    );
    if !normalized_absolute(&path)
        || path.parent() != Some(directory)
        || !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|filename| {
                valid_raw_name(filename)
                    && filename
                        .rsplit_once('@')
                        .is_some_and(|(base, _)| base == name)
            })
    {
        return Err(invalid(
            "recovery backup path is outside its owned directory",
        ));
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) => regular(&metadata)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(path)
}

fn validate_snapshot_fields(document: &Value) -> io::Result<()> {
    match document.get("language") {
        None => {}
        Some(Value::String(name)) => {
            let language = crate::syntax::Language::from_key(name)
                .ok_or_else(|| invalid("unknown recovery language"))?;
            if language == crate::syntax::Language::Builtin(15)
                && document
                    .get("udl")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
            {
                return Err(invalid("missing recovery user-defined language name"));
            }
        }
        _ => return Err(invalid("invalid recovery language")),
    }
    if document
        .get("udl")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        return Err(invalid("invalid recovery user-defined language name"));
    }
    if let Some(selection) = document.get("selection_set") {
        let ranges = selection
            .get("ranges")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("invalid recovery selection ranges"))?;
        let number = |value: &Value, field: &str| {
            value
                .get(field)
                .and_then(Value::as_u64)
                .and_then(|number| usize::try_from(number).ok())
                .ok_or_else(|| invalid("invalid recovery selection position"))
        };
        let mut parsed = Vec::new();
        for range in ranges {
            parsed.push(crate::editor::SelectionRange {
                selection: crate::editor::Selection {
                    anchor: crate::editor::ByteOffset(number(range, "anchor")?),
                    head: crate::editor::ByteOffset(number(range, "head")?),
                },
                anchor_virtual: number(range, "anchor_virtual")?,
                head_virtual: number(range, "head_virtual")?,
            });
        }
        crate::editor::SelectionSet::from_parts(
            parsed,
            number(selection, "primary")?,
            selection
                .get("rectangular")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("invalid recovery rectangular selection"))?,
        )
        .map_err(|error| invalid(&error.to_string()))?;
    }
    if let Some(disk) = document.get("disk").filter(|disk| !disk.is_null()) {
        if !disk
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| Path::new(path).is_absolute())
            || disk.get("read_only").and_then(Value::as_bool).is_none()
        {
            return Err(invalid("invalid recovery disk baseline"));
        }
        #[cfg(unix)]
        for field in ["device", "inode"] {
            if disk.get(field).and_then(Value::as_u64).is_none() {
                return Err(invalid("missing recovery disk identity"));
            }
        }
        crate::files::ByteFormatCodePage::from_value(&disk["codepage"])?;
        if let Some(time) = disk.get("original_modified").filter(|time| !time.is_null()) {
            let before = time
                .get("before_epoch")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("invalid recovery modification time"))?;
            let seconds = time
                .get("seconds")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("invalid recovery modification time"))?;
            let nanos = time
                .get("nanoseconds")
                .and_then(Value::as_u64)
                .filter(|nanos| *nanos < 1_000_000_000)
                .and_then(|nanos| u32::try_from(nanos).ok())
                .ok_or_else(|| invalid("invalid recovery modification time"))?;
            let distance = Duration::new(seconds, nanos);
            let timestamp = if before {
                SystemTime::UNIX_EPOCH.checked_sub(distance)
            } else {
                SystemTime::UNIX_EPOCH.checked_add(distance)
            };
            if timestamp.is_none() {
                return Err(invalid("unsupported recovery modification time"));
            }
        }
    }
    Ok(())
}

struct OrphanMetadata {
    path: PathBuf,
    identity: Identity,
}

struct RawReferences {
    protected: BTreeSet<PathBuf>,
    candidates: Vec<PathBuf>,
    orphan_metadata: BTreeMap<PathBuf, OrphanMetadata>,
}

enum RawScan {
    Ready(RawReferences),
    Deferred,
}

fn raw_references(directory: &Path, cancelled: &AtomicBool) -> io::Result<RawScan> {
    let mut protected = BTreeSet::new();
    let mut manifests = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut leases = BTreeMap::new();
    let mut candidates = Vec::new();
    for entry in fs::read_dir(directory)? {
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "backup cleanup cancelled",
            ));
        }
        let entry = entry?;
        let filename = entry.file_name();
        let Some(name) = filename.to_str() else {
            continue;
        };
        if let Some(owner) = name.strip_suffix(".session.json") {
            manifests.insert(owner.to_owned(), entry.path());
        } else if let Some(key) = name.strip_suffix(".snapshot.json") {
            metadata.insert(key.to_owned(), entry.path());
        } else if let Some(owner) = name.strip_suffix(".lock") {
            if !valid_key(owner) || !owner.starts_with("instance-") {
                return Err(invalid("unknown recovery lease"));
            }
            let lease = open_checked(&entry.path(), true, false)?;
            let active = match lease.try_lock() {
                Ok(()) => false,
                Err(fs::TryLockError::WouldBlock) => true,
                Err(fs::TryLockError::Error(error)) => return Err(error),
            };
            let current = fs::symlink_metadata(entry.path())?;
            regular(&current)?;
            if Identity::from_metadata(&current)? != Identity::from_metadata(&lease.metadata()?)? {
                return Err(invalid("recovery lease changed during cleanup"));
            }
            leases.insert(owner.to_owned(), active);
        } else if name.ends_with(".json") {
            return Err(invalid(
                "unknown or legacy recovery JSON; raw cleanup skipped",
            ));
        } else if valid_raw_name(name) {
            candidates.push(entry.path());
        }
    }
    let mut deferred = false;
    for (owner, active) in &leases {
        if !manifests.contains_key(owner) {
            if *active {
                deferred = true;
            } else {
                return Err(invalid("inactive recovery lease has no manifest"));
            }
        }
    }
    let mut referenced_keys = BTreeSet::new();
    let mut counters = BTreeMap::new();
    for (owner, path) in &manifests {
        if !valid_key(owner) || !owner.starts_with("instance-") || !leases.contains_key(owner) {
            return Err(invalid("recovery manifest has no verified owner lease"));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "backup cleanup cancelled",
            ));
        }
        let document = read_metadata_json(path)?;
        if document.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(invalid("unknown recovery manifest schema"));
        }
        referenced_keys.extend(crate::recovery::RecoverySession::validate_manifest_owner(
            owner, &document,
        )?);
        let counter = document
            .get("next_snapshot")
            .and_then(Value::as_u64)
            .filter(|counter| *counter > 0)
            .ok_or_else(|| invalid("invalid recovery snapshot counter"))?;
        counters.insert(owner.clone(), counter);
    }
    for key in &referenced_keys {
        if !metadata.contains_key(key) {
            return Err(invalid("referenced recovery metadata is missing or legacy"));
        }
    }
    let mut orphan_metadata = BTreeMap::new();
    let mut metadata_paths = BTreeSet::new();
    for (key, path) in metadata {
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "backup cleanup cancelled",
            ));
        }
        let Some((owner, sequence)) = key.rsplit_once("-buffer-") else {
            return Err(invalid("unknown recovery metadata owner"));
        };
        let sequence = sequence
            .parse::<u64>()
            .ok()
            .filter(|sequence| *sequence > 0)
            .ok_or_else(|| invalid("unknown recovery metadata sequence"))?;
        if !valid_key(owner) || !owner.starts_with("instance-") || !leases.contains_key(owner) {
            return Err(invalid("unknown recovery metadata identity"));
        }
        let (document, identity) = read_metadata_json_with_identity(&path)?;
        let backup = metadata_backup_path(directory, &key, &document)?;
        let counter = match counters.get(owner) {
            Some(counter) => *counter,
            None if leases.get(owner) == Some(&true) => {
                deferred = true;
                protected.insert(backup);
                continue;
            }
            None => return Err(invalid("recovery metadata has no owner manifest")),
        };
        if !metadata_paths.insert(backup.clone()) {
            protected.insert(backup.clone());
            orphan_metadata.remove(&backup);
        }
        if referenced_keys.contains(&key) || sequence >= counter {
            protected.insert(backup.clone());
            orphan_metadata.remove(&backup);
        } else if !protected.contains(&backup) {
            orphan_metadata.insert(backup, OrphanMetadata { path, identity });
        }
    }
    if deferred {
        return Ok(RawScan::Deferred);
    }
    Ok(RawScan::Ready(RawReferences {
        protected,
        candidates,
        orphan_metadata,
    }))
}

fn cleanup_raw(
    store: &ConfigStore,
    options: BackupRetentionOptions,
    now: SystemTime,
    cancelled: &AtomicBool,
    report: &mut CleanupReport,
) -> io::Result<()> {
    let directory = store.root().join("backup");
    let parent = match directory_identity(&directory) {
        Ok(parent) => parent,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let (mut generation, lock_identity) = {
        let Some(file) = try_recovery_lock(store)? else {
            report.busy_recovery += 1;
            return Ok(());
        };
        (
            recovery_generation(store, &file)?,
            Identity::from_metadata(&file.metadata()?)?,
        )
    };
    let scanned = raw_references(&directory, cancelled);
    if cancelled.load(Ordering::Acquire) {
        report.cancelled = true;
        return Ok(());
    }
    {
        let Some(file) = try_recovery_lock(store)? else {
            report.busy_recovery += 1;
            return Ok(());
        };
        if !recovery_generation_matches(store, &file, &lock_identity, generation)? {
            report.busy_recovery += 1;
            return Ok(());
        }
    }
    let references = match scanned {
        Ok(RawScan::Ready(references)) => references,
        Ok(RawScan::Deferred) => {
            report.busy_recovery += 1;
            return Ok(());
        }
        Err(error) => {
            report.skipped_raw_scans += 1;
            if error.kind() == io::ErrorKind::Interrupted && cancelled.load(Ordering::Acquire) {
                report.cancelled = true;
            } else {
                report.failure(&directory, &error);
            }
            return Ok(());
        }
    };
    for path in &references.candidates {
        if !continue_cleanup(store, options, cancelled, report)? {
            break;
        }
        let Some(file) = try_recovery_lock(store)? else {
            report.busy_recovery += 1;
            break;
        };
        if !recovery_generation_matches(store, &file, &lock_identity, generation)? {
            report.busy_recovery += 1;
            break;
        }
        if references.protected.contains(path) {
            report.retained += 1;
            continue;
        }
        let eligible = (|| -> io::Result<Option<Identity>> {
            let metadata = fs::symlink_metadata(path)?;
            regular(&metadata)?;
            if !expired(&metadata, now, options.duration()?)? {
                return Ok(None);
            }
            if let Some(associated) = references.orphan_metadata.get(path) {
                let current = fs::symlink_metadata(&associated.path)?;
                regular(&current)?;
                if associated.identity != Identity::from_metadata(&current)? {
                    return Err(invalid("orphan recovery metadata changed before deletion"));
                }
            }
            Ok(Some(Identity::from_metadata(&metadata)?))
        })();
        let identity = match eligible {
            Ok(Some(identity)) => identity,
            Ok(None) => {
                report.retained += 1;
                continue;
            }
            Err(error) => {
                report.failure(path, &error);
                continue;
            }
        };
        if cancelled.load(Ordering::Acquire) {
            report.cancelled = true;
            break;
        }
        generation = advance_recovery_generation(store, &file)?;
        let result = remove_checked(
            store,
            path,
            &identity,
            &parent,
            options,
            Some(now),
            cancelled,
            report,
        );
        match result {
            Ok(true) => {
                report.removed_raw += 1;
                if let Some(metadata) = references.orphan_metadata.get(path) {
                    match remove_checked(
                        store,
                        &metadata.path,
                        &metadata.identity,
                        &parent,
                        options,
                        None,
                        cancelled,
                        report,
                    ) {
                        Ok(true) => report.pruned_entries += 1,
                        Ok(false) => {}
                        Err(error) => report.failure(&metadata.path, &error),
                    }
                }
            }
            Ok(false) => report.retained += 1,
            Err(error) => report.failure(path, &error),
        }
    }
    Ok(())
}
