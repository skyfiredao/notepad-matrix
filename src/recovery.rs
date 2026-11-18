//! Committed-buffer snapshots for the session/periodic-backup workflow.
//! Persistence does not acknowledge the document's formal savepoint.

use std::collections::{BTreeMap, BTreeSet};
use std::borrow::Cow;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::path::PathBuf;

use serde_json::Value;

use crate::config::ConfigStore;
use crate::editor::{ByteOffset, Selection, SelectionRange, SelectionSet};
use crate::files::{DiskFile, UnicodeFormat, Utf8Format};
use crate::input::{EditorSession, LineEnding};
use crate::workspace::{Buffer, BufferId, DocumentViewState, ViewId, Workspace};

#[path = "save_backup.rs"]
mod save_backup;
pub use save_backup::{SaveBackupMode, SaveBackupOptions};
#[path = "session_file.rs"]
mod session_file;
pub use session_file::SessionFile;
#[path = "raw_backup.rs"]
mod raw_backup;

fn selection_set_json(selections: &SelectionSet) -> Value {
    serde_json::json!({"ranges":selections.ranges().iter().map(|range| serde_json::json!({"anchor":range.selection.anchor.0,"head":range.selection.head.0,"anchor_virtual":range.anchor_virtual,"head_virtual":range.head_virtual})).collect::<Vec<_>>(),"primary":selections.primary_index(),"rectangular":selections.is_rectangular()})
}

enum RecoveryBoundary<'a> {
    Unchecked,
    Text(Cow<'a, str>),
    Source(crate::byte_buffer::ByteBuffer),
}

impl RecoveryBoundary<'_> {
    fn position(&self, mut offset: usize) -> ByteOffset {
        match self {
            Self::Unchecked | Self::Source(_) => {}
            Self::Text(text) => {
                offset = offset.min(text.len());
                while !text.is_char_boundary(offset) { offset -= 1; }
            }
        }
        ByteOffset(offset)
    }

    fn virtual_space(&self, position: ByteOffset, space: usize) -> usize {
        let at_end = match self {
            Self::Unchecked => true,
            Self::Source(bytes) => bytes.is_line_end(position.0),
            Self::Text(text) => position.0 == text.len()
                || text.as_bytes().get(position.0) == Some(&b'\r')
                || (text.as_bytes().get(position.0) == Some(&b'\n')
                    && (position.0 == 0 || text.as_bytes().get(position.0 - 1) != Some(&b'\r'))),
        };
        if at_end { space } else { 0 }
    }
}

fn parse_selection_set(value: &Value, boundary: &RecoveryBoundary<'_>) -> io::Result<SelectionSet> {
    let number = |value: &Value, name: &str| {
        value
            .get(name)
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| invalid("invalid session selection set"))
    };
    let entries = value
        .get("ranges")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("invalid session selection ranges"))?;
    let mut ranges = Vec::new();
    for entry in entries {
        let anchor = boundary.position(number(entry, "anchor")?);
        let head = boundary.position(number(entry, "head")?);
        let virtual_space = |position: ByteOffset, name| -> io::Result<usize> {
            let space = number(entry, name)?;
            Ok(boundary.virtual_space(position, space))
        };
        ranges.push(SelectionRange {
            selection: Selection { anchor, head },
            anchor_virtual: virtual_space(anchor, "anchor_virtual")?,
            head_virtual: virtual_space(head, "head_virtual")?,
        });
    }
    SelectionSet::from_parts(
        ranges,
        number(value, "primary")?,
        value
            .get("rectangular")
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid("invalid session selection mode"))?,
    )
    .map_err(|error| invalid(&error.to_string()))
}

fn view_state_json(state: &DocumentViewState) -> Value {
    let mut value = serde_json::json!({"top_line":state.top_line,"viewport":null,"horizontal_offset":state.horizontal_offset,"folded_lines":state.folded_lines,"word_wrap":state.word_wrap});
    if let Some(position) = &state.viewport {
        value["viewport"] = serde_json::json!({"first_visible_line":position.first_visible_line,"wrap_offset":position.wrap_offset,"wrap_count":position.wrap_count});
    }
    value
}

fn parse_view_state(value: &Value) -> io::Result<DocumentViewState> {
    let number = |name| {
        value
            .get(name)
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| invalid("invalid session view position"))
    };
    let horizontal = value
        .get("horizontal_offset")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0. && *value <= f32::MAX as f64)
        .ok_or_else(|| invalid("invalid session horizontal position"))?;
    let folded_lines = value
        .get("folded_lines")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("invalid session folds"))?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid("invalid session fold line"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(DocumentViewState {
        top_line: number("top_line")?,
        viewport: match value.get("viewport") {
            None | Some(Value::Null) => None,
            Some(position) => {
                let number = |name| {
                    position
                        .get(name)
                        .and_then(Value::as_u64)
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or_else(|| invalid("invalid session viewport"))
                };
                let position = crate::workspace::ViewportPosition {
                    first_visible_line: number("first_visible_line")?,
                    wrap_offset: number("wrap_offset")?,
                    wrap_count: number("wrap_count")?,
                };
                if position.wrap_count == 0 || position.wrap_offset >= position.wrap_count {
                    return Err(invalid("invalid session wrap position"));
                }
                Some(position)
            }
        },
        horizontal_offset: horizontal as f32,
        folded_lines,
        word_wrap: value
            .get("word_wrap")
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid("invalid session wrap mode"))?,
    })
}

fn validate_views(manifest: &Value, document_keys: &[String]) -> io::Result<()> {
    if let Some(ui) = manifest.get("ui") {
        validate_session_ui(ui)?;
    }
    let Some(views) = manifest.get("views") else {
        return Ok(());
    };
    let views = views
        .as_array()
        .filter(|views| views.len() == 2)
        .ok_or_else(|| invalid("invalid session views"))?;
    let active_view = manifest
        .get("active_view")
        .and_then(Value::as_u64)
        .filter(|view| *view < 2)
        .ok_or_else(|| invalid("invalid active session view"))? as usize;
    let mut referenced = BTreeSet::new();
    for (index, view) in views.iter().enumerate() {
        let members = view
            .get("documents")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("invalid session view documents"))?;
        let mut keys = BTreeSet::new();
        for member in members {
            let key = member
                .get("key")
                .and_then(Value::as_str)
                .filter(|key| document_keys.iter().any(|document| document == key))
                .ok_or_else(|| invalid("unknown view document"))?;
            if !keys.insert(key) {
                return Err(invalid("duplicate view document"));
            }
            referenced.insert(key);
            parse_view_state(
                member
                    .get("state")
                    .ok_or_else(|| invalid("missing session view state"))?,
            )?;
            if let Some(selections) = member.get("selection_set") {
                parse_selection_set(selections, &RecoveryBoundary::Unchecked)?;
            }
            for name in ["anchor", "head"] {
                member
                    .get(name)
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| invalid("invalid view selection"))?;
            }
            match member.get("color") {
                Some(Value::Null) => {}
                Some(color) if color.as_u64().is_some_and(|color| color < 5) => {}
                _ => return Err(invalid("invalid session tab color")),
            }
        }
        match view.get("active") {
            Some(Value::Null) if keys.is_empty() => {}
            Some(Value::String(key)) if keys.contains(key.as_str()) => {}
            _ => return Err(invalid("invalid active view document")),
        }
        if index == active_view && keys.is_empty() && !document_keys.is_empty() {
            return Err(invalid("active session view is empty"));
        }
    }
    if document_keys
        .iter()
        .any(|key| !referenced.contains(key.as_str()))
    {
        return Err(invalid("session document has no view"));
    }
    if manifest.get("ui").is_some_and(|value| !value.is_object()) {
        return Err(invalid("invalid session UI snapshot"));
    }
    Ok(())
}

pub(crate) fn validate_session_ui(ui: &Value) -> io::Result<()> {
    if !ui.is_object() {
        return Err(invalid("invalid session UI snapshot"));
    }
    if let Some(tabs) = ui.get("tabs") {
        if !tabs.is_object() {
            return Err(invalid("invalid session tab settings"));
        }
        for name in ["multiline", "vertical", "mru", "switcher"] {
            if tabs.get(name).is_some_and(|value| !value.is_boolean()) {
                return Err(invalid("invalid session tab setting"));
            }
        }
    }
    for name in ["document_list_visible", "document_map_visible"] {
        if ui.get(name).is_some_and(|value| !value.is_boolean()) {
            return Err(invalid("invalid session panel visibility"));
        }
    }
    if let Some(ratio) = ui.get("split_ratio")
        && !ratio
            .as_f64()
            .is_some_and(|value| value.is_finite() && (0.1..=0.9).contains(&value))
    {
        return Err(invalid("invalid session split ratio"));
    }
    if let Some(panels) = ui.get("panels") {
        if panels["version"].as_u64() != Some(1) {
            return Err(invalid("invalid session panel state version"));
        }
        crate::panel_layout::PanelLayout::from_json(&panels["layout"])?;
        crate::folder_workspace::FolderWorkspace::restore(&panels["folder"])?;
        crate::projects::ProjectPanels::restore(&panels["projects"])?;
    }
    Ok(())
}

pub struct RecoverySnapshot {
    document: Value,
    text: String,
    loaded_bytes: u64,
    raw: Option<Vec<u8>>,
}

/// One process-owned recovery session. Holding its lease is required for writes
/// or restoration; another live instance must never consume this session.
pub struct RecoverySession {
    store: ConfigStore,
    key: String,
    _lease: File,
    buffers: BTreeMap<BufferId, String>,
    backup_revisions: BTreeMap<BufferId, (std::sync::Arc<()>, UnicodeFormat, Option<crate::files::ByteFormatCodePage>)>,
    next_snapshot: u64,
    restore_warnings: Vec<String>,
    snapshot_mode: bool,
}

fn parse_language(value: &Value) -> io::Result<Option<crate::syntax::Language>> {
    match value.get("language") {
        None => Ok(None),
        Some(Value::String(name)) => {
            let language = crate::syntax::Language::from_key(name).ok_or_else(|| invalid("unknown recovery language"))?;
            if language == crate::syntax::Language::Builtin(15) {
                if value.get("udl").and_then(Value::as_str).is_none_or(str::is_empty) {
                    return Err(invalid("user-defined recovery language name is missing"));
                }
                return Ok(Some(crate::syntax::Language::UserDefined(0)));
            }
            Ok(Some(language))
        }
        _ => Err(invalid("unknown recovery language")),
    }
}

struct SessionDocument {
    key: String,
    path: Option<PathBuf>,
    selection: Selection,
    language: Option<crate::syntax::Language>,
    udl: Option<String>,
}

impl SessionDocument {
    fn parse(value: &Value) -> io::Result<Self> {
        crate::files::ByteFormatCodePage::from_value(&value["codepage"])?;
        if value.get("udl").is_some_and(|value| !value.is_null() && !value.is_string()) {
            return Err(invalid("invalid session user-defined language name"));
        }
        for name in ["pinned", "user_read_only", "monitoring"] {
            if value.get(name).is_some_and(|value| !value.is_boolean()) {
                return Err(invalid("invalid session document property"));
            }
        }
        if let Some(marks) = value.get("marks") {
            let marks = marks
                .as_array()
                .ok_or_else(|| invalid("invalid session bookmarks"))?;
            if marks.iter().any(|line| {
                line.as_u64()
                    .and_then(|line| usize::try_from(line).ok())
                    .is_none()
            }) {
                return Err(invalid("invalid session bookmark line"));
            }
        }
        let key = value
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("session document identity is missing"))?
            .to_owned();
        let path = match value.get("path") {
            Some(Value::Null) => None,
            Some(value) => {
                let path: PathBuf = serde_json::from_value(value.clone())?;
                if !path.is_absolute() {
                    return Err(invalid("session document path must be absolute"));
                }
                Some(path)
            }
            None => return Err(invalid("session document path is missing")),
        };
        let position = |name| -> io::Result<ByteOffset> {
            let value = value
                .get(name)
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("session document position is missing"))?;
            Ok(ByteOffset(usize::try_from(value).map_err(|_| {
                invalid("session position exceeds platform range")
            })?))
        };
        Ok(Self {
            key,
            path,
            language: parse_language(value)?,
            udl: value.get("udl").and_then(Value::as_str).map(str::to_owned),
            selection: Selection {
                anchor: position("anchor")?,
                head: position("head")?,
            },
        })
    }
}

pub struct RecoveryManager {
    sessions: Vec<RecoverySession>,
    warnings: Vec<String>,
    changed_buffers: Vec<BufferId>,
    snapshot_mode: bool,
    remember_last_session: bool,
}

#[derive(Clone, Copy)]
pub struct RecoveryOptions {
    pub enabled: bool,
    pub remember_last_session: bool,
    pub snapshot_mode: bool,
    pub interval: std::time::Duration,
    pub large_file_restriction: crate::workspace::LargeFileRestriction,
}

impl RecoveryOptions {
    pub fn from_settings(settings: &Value) -> io::Result<Self> {
        let boolean = |key| match settings.get(key) {
            None => Ok(true),
            Some(value) => value
                .as_bool()
                .ok_or_else(|| invalid("snapshot settings must be booleans")),
        };
        let milliseconds = match settings.get("snapshot_backup_ms") {
            None => 7000,
            Some(value) => value
                .as_u64()
                .ok_or_else(|| invalid("snapshot_backup_ms must be an unsigned integer"))?,
        };
        let snapshot_mode = boolean("snapshot_mode")?;
        let remember_last_session = boolean("remember_last_session")?;
        Ok(Self {
            enabled: snapshot_mode && remember_last_session,
            remember_last_session,
            snapshot_mode,
            interval: std::time::Duration::from_millis(milliseconds.max(1000)),
            large_file_restriction: crate::workspace::LargeFileRestriction::from_settings(
                settings,
            )?,
        })
    }
}

impl RecoveryManager {
    pub fn create_for_workspace(home: &Path, options: RecoveryOptions) -> io::Result<Self> {
        let mut session = RecoverySession::create(home)?;
        session.snapshot_mode = options.enabled;
        Ok(Self { sessions: vec![session],warnings: Vec::new(),changed_buffers: Vec::new(),snapshot_mode: options.enabled,remember_last_session: options.remember_last_session })
    }

    pub fn apply_options(&mut self, options: RecoveryOptions) {
        self.snapshot_mode = options.enabled;
        self.remember_last_session = options.remember_last_session;
        for session in &mut self.sessions { session.snapshot_mode = options.enabled; }
    }

    pub fn snapshot_mode(&self) -> bool {
        self.snapshot_mode
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    pub fn changed_buffers(&self) -> &[BufferId] {
        &self.changed_buffers
    }
    pub fn discard_all(&mut self) -> io::Result<()> {
        let empty = Workspace::new();
        let ids = BTreeSet::new();
        for session in &mut self.sessions {
            session.checkpoint_selected(&empty, Some(&ids), &BTreeSet::new())?;
        }
        Ok(())
    }
    pub fn open(home: &Path) -> io::Result<(Self, Workspace)> {
        let settings = ConfigStore::under_home(home).load_settings()?;
        Self::open_with_options(home, RecoveryOptions::from_settings(&settings)?)
    }

    pub fn open_with_options(
        home: &Path,
        options: RecoveryOptions,
    ) -> io::Result<(Self, Workspace)> {
        let store = ConfigStore::under_home(home);
        let directory = store
            .document_path(Path::new("backup/prepare.json"))?
            .parent()
            .ok_or_else(|| invalid("backup directory is missing"))?
            .to_owned();
        let mut keys = Vec::new();
        let mut warnings = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if let Some(key) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".session.json"))
            {
                match snapshot_path(key) {
                    Ok(_) => keys.push(key.to_owned()),
                    Err(error) => warnings.push(format!("Invalid recovery index name: {error}")),
                }
            }
        }
        keys.sort();
        let mut workspace = Workspace::with_large_file_restriction(options.large_file_restriction);
        workspace.configure_new_document(&store.load_settings()?)?;
        let udls = crate::udl_registry::UdlRegistry::load(&store)?;
        workspace.synchronize_udl_names(udls.named().map(|(id,definition)| (id,definition.name.clone())));
        let initial = workspace.active().id();
        let mut sessions = Vec::new();
        let mut active = None;
        for key in keys {
            match RecoverySession::claim(home, &key) {
                Ok(Some(mut session)) => {
                    session.snapshot_mode = options.enabled;
                    match session.restore_into_workspace(&mut workspace) {
                        Ok(id) => {
                            if id.is_some() {
                                active = id;
                            }
                            warnings.append(&mut session.restore_warnings);
                            sessions.push(session);
                        }
                        Err(error) => warnings.push(format!(
                            "Recovery session {key} could not be loaded: {error}"
                        )),
                    }
                }
                Ok(None) => {}
                Err(error) => warnings.push(format!(
                    "Recovery session {key} could not be loaded: {error}"
                )),
            }
        }
        if workspace.buffers().len() > 1 {
            workspace
                .close_buffer(initial, crate::workspace::CloseDisposition::IfUnmodified)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        if let Some(id) = active {
            workspace
                .activate(id)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        if sessions.is_empty() {
            sessions.push(RecoverySession::create(home)?);
        }
        for session in &mut sessions {
            session.snapshot_mode = options.enabled;
        }
        let mut changed_buffers = Vec::new();
        for buffer in workspace.buffers_mut() {
            if let Some(file) = &mut buffer.disk_file {
                match file.observe_original_timestamp() {
                    Ok(Some(true)) => changed_buffers.push(buffer.id()),
                    Ok(Some(false)) => {},
                    Ok(None) => warnings.push(format!("{}: recovery has no recorded original modification time; external-change check is unavailable", buffer.name())),
                    Err(error) => warnings.push(format!("{}: could not check the original file modification time: {error}", buffer.name())),
                }
            }
        }
        Ok((
            Self {
                sessions,
                warnings,
                changed_buffers,
                snapshot_mode: options.enabled,
                remember_last_session: options.remember_last_session,
            },
            workspace,
        ))
    }

    pub fn checkpoint(&mut self, workspace: &Workspace) -> io::Result<()> {
        self.finish_guarded_exit(workspace, &BTreeSet::new())
    }

    pub fn finish_guarded_exit(
        &mut self,
        workspace: &Workspace,
        discarded: &BTreeSet<BufferId>,
    ) -> io::Result<()> {
        if !self.remember_last_session { return self.discard_all(); }
        let owned = self
            .sessions
            .iter()
            .flat_map(|session| session.buffers.keys().copied())
            .collect::<BTreeSet<_>>();
        let primary = self
            .sessions
            .first_mut()
            .ok_or_else(|| invalid("recovery session is missing"))?;
        for buffer in workspace
            .buffers()
            .iter()
            .filter(|buffer| !owned.contains(&buffer.id()))
        {
            primary.assign(buffer.id())?;
        }
        for session in &mut self.sessions {
            let ids = session.buffers.keys().copied().collect::<BTreeSet<_>>();
            session.checkpoint_selected(workspace, Some(&ids), discarded)?;
        }
        Ok(())
    }
}

impl RecoverySession {
    pub fn create(home: &Path) -> io::Result<Self> {
        let store = ConfigStore::under_home(home);
        let directory = store
            .document_path(Path::new("backup/prepare.json"))?
            .parent()
            .ok_or_else(|| invalid("backup directory is missing"))?
            .to_owned();
        let temporary = tempfile::Builder::new()
            .prefix("instance-")
            .suffix(".lock")
            .tempfile_in(directory)?;
        let (lease, path) = temporary.keep().map_err(|error| error.error)?;
        lease.lock()?;
        let key = path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid recovery session identity"))?
            .to_owned();
        snapshot_path(&key)?;
        Ok(Self {
            store,
            key,
            _lease: lease,
            buffers: BTreeMap::new(),
            backup_revisions: BTreeMap::new(),
            next_snapshot: 1,
            restore_warnings: Vec::new(),
            snapshot_mode: true,
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn claim(home: &Path, key: &str) -> io::Result<Option<Self>> {
        snapshot_path(key)?;
        let store = ConfigStore::under_home(home);
        let lock_path = store
            .document_path(Path::new(&format!("backup/{key}.lock.json")))?
            .with_extension("");
        let metadata = fs::symlink_metadata(&lock_path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(invalid("recovery lease must be a regular file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 {
                return Err(invalid("recovery lease must not be hard linked"));
            }
        }
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)?;
        // Detect replacement between checking the directory entry and opening it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = lease.metadata()?;
            if (opened.dev(), opened.ino()) != (metadata.dev(), metadata.ino()) {
                return Err(invalid("recovery lease changed while opening"));
            }
        }
        match lease.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        let mut session = Self {
            store,
            key: key.into(),
            _lease: lease,
            buffers: BTreeMap::new(),
            backup_revisions: BTreeMap::new(),
            next_snapshot: 1,
            restore_warnings: Vec::new(),
            snapshot_mode: true,
        };
        let manifest = session.store.load_json(&session.manifest_path())?;
        session.validate_manifest(&manifest)?;
        session.next_snapshot = manifest["next_snapshot"]
            .as_u64()
            .ok_or_else(|| invalid("missing recovery counter"))?;
        Ok(Some(session))
    }

    fn manifest_path(&self) -> PathBuf {
        PathBuf::from("backup").join(format!("{}.session.json", self.key))
    }

    fn validate_manifest(&self, manifest: &Value) -> io::Result<Vec<String>> {
        Self::validate_manifest_owner(&self.key, manifest)
    }

    fn validate_manifest_owner(owner: &str, manifest: &Value) -> io::Result<Vec<String>> {
        if manifest.get("recent_files").is_some() || manifest.get("recent_files_limit").is_some() {
            Workspace::new()
                .configure_recent_files(manifest)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        let entries = manifest
            .get("snapshots")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("invalid recovery session index"))?;
        let counter = manifest
            .get("next_snapshot")
            .and_then(Value::as_u64)
            .filter(|counter| *counter > 0)
            .ok_or_else(|| invalid("invalid recovery snapshot counter"))?;
        let mut keys = Vec::new();
        for entry in entries {
            let key = entry
                .as_str()
                .ok_or_else(|| invalid("invalid recovery snapshot reference"))?;
            snapshot_path(key)?;
            let sequence = key
                .strip_prefix(&format!("{owner}-buffer-"))
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|sequence| *sequence > 0 && *sequence < counter);
            if sequence.is_none() || keys.iter().any(|existing| existing == key) {
                return Err(invalid(
                    "recovery snapshot reference has wrong owner or duplicate identity",
                ));
            }
            keys.push(key.to_owned());
        }
        let document_keys = match manifest.get("documents") {
            None => keys.clone(),
            Some(value) => {
                let entries = value
                    .as_array()
                    .ok_or_else(|| invalid("invalid session document list"))?;
                let mut document_keys = Vec::new();
                for entry in entries {
                    let document = SessionDocument::parse(entry)?;
                    snapshot_path(&document.key)?;
                    let sequence = document
                        .key
                        .strip_prefix(&format!("{owner}-buffer-"))
                        .and_then(|value| value.parse::<u64>().ok())
                        .filter(|sequence| *sequence > 0 && *sequence < counter);
                    if sequence.is_none()
                        || document_keys.contains(&document.key)
                    {
                        return Err(invalid("invalid session document reference"));
                    }
                    document_keys.push(document.key);
                }
                if keys.iter().any(|key| !document_keys.contains(key)) {
                    return Err(invalid("snapshot has no session document reference"));
                }
                document_keys
            }
        };
        match manifest.get("active") {
            Some(Value::Null) => {}
            Some(Value::String(key)) if document_keys.contains(key) => {}
            _ => return Err(invalid("invalid recovery active document")),
        }
        validate_views(manifest, &document_keys)?;
        Ok(keys)
    }

    pub fn restore_workspace(&mut self) -> io::Result<Workspace> {
        let settings = self.store.load_settings()?;
        let policy = crate::workspace::LargeFileRestriction::from_settings(&settings)?;
        let mut workspace = Workspace::with_large_file_restriction(policy);
        workspace.configure_new_document(&settings)?;
        let udls = crate::udl_registry::UdlRegistry::load(&self.store)?;
        workspace.synchronize_udl_names(udls.named().map(|(id,definition)| (id,definition.name.clone())));
        let initial = workspace.active().id();
        let active = self.restore_into_workspace(&mut workspace)?;
        if !self.buffers.is_empty() {
            workspace
                .close_buffer(initial, crate::workspace::CloseDisposition::IfUnmodified)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        if let Some(id) = active {
            workspace
                .activate(id)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        Ok(workspace)
    }

    fn restore_into_workspace(
        &mut self,
        workspace: &mut Workspace,
    ) -> io::Result<Option<BufferId>> {
        let manifest = self.store.load_json(&self.manifest_path())?;
        let keys = self.validate_manifest(&manifest)?;
        let (buffers, active) = Self::restore_document_data(
            workspace,
            &manifest,
            &keys,
            self.snapshot_mode,
            false,
            |key| RecoverySnapshot::load(&self.store, key),
            &mut self.restore_warnings,
        )?;
        for buffer in workspace.buffers() {
            if let Some(key) = buffers.get(&buffer.id()).filter(|key| self.snapshot_mode && keys.contains(key)) {
                let snapshot = match RecoverySnapshot::load(&self.store, key) {
                    Ok(snapshot) => snapshot,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                let path = if snapshot.document.get("backupFilePath").is_some() {
                    raw_backup::path(&self.store, &snapshot.document)?
                } else {
                    self.store.root().join(snapshot_path(key)?)
                };
                match fs::metadata(path).and_then(|metadata| metadata.modified()) {
                    Ok(time) => buffer.set_backup_modified(Some(time)),
                    Err(error) => self.restore_warnings.push(format!(
                        "{}: snapshot modification time is unavailable: {error}",
                        buffer.name()
                    )),
                }
                self.backup_revisions.insert(
                    buffer.id(),
                    (buffer.editor.document().text_revision(), buffer.format(), buffer.byte_code_page()),
                );
            }
        }
        self.buffers = buffers;
        Ok(active)
    }

    fn restore_document_data(
        workspace: &mut Workspace,
        manifest: &Value,
        keys: &[String],
        snapshot_mode: bool,
        deduplicate: bool,
        mut load_snapshot: impl FnMut(&str) -> io::Result<RecoverySnapshot>,
        warnings: &mut Vec<String>,
    ) -> io::Result<(BTreeMap<BufferId, String>, Option<BufferId>)> {
        warnings.clear();
        let documents = match manifest.get("documents") {
            Some(value) => value
                .as_array()
                .ok_or_else(|| invalid("invalid session documents"))?
                .iter()
                .map(SessionDocument::parse)
                .collect::<io::Result<Vec<_>>>()?,
            None => keys
                .iter()
                .map(|key| SessionDocument {
                    key: key.clone(),
                    path: None,
                    selection: Selection::default(),
                    language: None,
                    udl: None,
                })
                .collect(),
        };
        let mut parts = Vec::new();
        let mut codepages = BTreeMap::new();
        let mut source_offsets = BTreeSet::new();
        let mut udl_names = BTreeMap::new();
        for document in documents {
            if let Some(name) = &document.udl { udl_names.insert(document.key.clone(),name.clone()); }
            if snapshot_mode && keys.contains(&document.key) {
                match load_snapshot(&document.key) {
                    Ok(snapshot) => {
                        if snapshot.uses_source_offsets()? { source_offsets.insert(document.key.clone()); }
                        if snapshot.document.get("codepage").is_some() { codepages.insert(document.key.clone(),crate::files::ByteFormatCodePage::from_value(&snapshot.document["codepage"])?); }
                        if let Some(name) = snapshot.document["udl"].as_str() { udl_names.insert(document.key.clone(),name.to_owned()); }
                        parts.push((
                            document.key.clone(),
                            snapshot.restoration_parts()?,
                            document.language.or(parse_language(&snapshot.document)?),
                            snapshot.loaded_bytes,
                        ));
                        continue;
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => warnings.push(format!("{}: session backup is missing: {error}",document.key)),
                    Err(error) => return Err(error),
                }
            }
            if let Some(path) = &document.path {
                let codepage = manifest["documents"].as_array().and_then(|entries| entries.iter().find(|entry| entry["key"].as_str() == Some(document.key.as_str()))).map(|entry| crate::files::ByteFormatCodePage::from_value(&entry["codepage"])).transpose()?.flatten();
                match DiskFile::load_with_code_page(path,codepage) {
                    Ok((file, text)) => {
                        let boundary = RecoveryBoundary::Text(text.as_str().into());
                        let mut editor = EditorSession::from_file_text(&text);
                        editor
                            .select(Selection {
                                anchor: boundary.position(document.selection.anchor.0),
                                head: boundary.position(document.selection.head.0),
                            })
                            .map_err(|error| invalid(&error.to_string()))?;
                        editor.set_read_only(file.is_read_only());
                        let format = file.format();
                        let loaded_bytes = file.loaded_len() as u64;
                        parts.push((
                            document.key,
                            (editor, Some(file), String::new(), format),
                            document.language,
                            loaded_bytes,
                        ));
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => return Err(error),
                    Err(error) => {
                        warnings.push(format!("{}: session file could not be opened: {error}",path.display()));
                        if workspace.keep_absent_session_files() {
                            codepages.insert(document.key.clone(),codepage);
                            let file = DiskFile::placeholder(path)?;
                            let mut editor = EditorSession::from_file_text("");
                            editor.set_read_only(true);
                            parts.push((document.key,(editor,Some(file),String::new(),UnicodeFormat::Utf8(Utf8Format::Plain)),document.language,0));
                        }
                    }
                }
            }
        }
        let mut buffers = BTreeMap::new();
        let mut active = None;
        workspace
            .check_recovery_capacity(parts.len())
            .map_err(|error| io::Error::other(error.to_string()))?;
        for (key, (editor, disk, name, format), language, loaded_bytes) in parts {
            let existing = if deduplicate {
                disk.as_ref().and_then(|file| {
                    workspace
                        .buffers()
                        .iter()
                        .find(|buffer| {
                            buffer
                                .disk_file
                                .as_ref()
                                .is_some_and(|existing| existing.path() == file.path())
                                && (!editor.is_modified()
                                    || buffer.editor.committed_text() == editor.committed_text())
                        })
                        .map(|buffer| buffer.id())
                })
            } else {
                None
            };
            let id = match existing {
                Some(id) => {
                    workspace
                        .activate(id)
                        .map_err(|error| invalid(&error.to_string()))?;
                    id
                }
                None => workspace
                    .append_recovered_buffer_with_size(editor, disk, name, format, loaded_bytes)
                    .map_err(|error| io::Error::other(error.to_string()))?,
            };
            if let Some(language) = language {
                let language = if matches!(language,crate::syntax::Language::UserDefined(_)) {
                    match udl_names.get(&key) {
                        Some(name) => workspace.udl_language_named(name),
                        None => workspace.udl_language_name(language).map(|_| language),
                    }.unwrap_or_else(|| { warnings.push(format!("{key}: user language is no longer available; restored as normal text")); crate::syntax::Language::NormalText })
                } else { language };
                workspace.active_mut().set_language(language);
            }
            if let Some(codepage) = codepages.get(&key) { workspace.active_mut().restore_byte_code_page(*codepage); }
            if manifest.get("active").and_then(Value::as_str) == Some(key.as_str()) {
                active = Some(id);
            }
            buffers.insert(id, key);
        }
        if let Some(views) = manifest.get("views").and_then(Value::as_array) {
            let mut memberships: [Vec<BufferId>; 2] = [
                workspace
                    .view_buffer_ids(ViewId::Primary)
                    .iter()
                    .copied()
                    .filter(|id| !buffers.contains_key(id))
                    .collect(),
                workspace
                    .view_buffer_ids(ViewId::Secondary)
                    .iter()
                    .copied()
                    .filter(|id| !buffers.contains_key(id))
                    .collect(),
            ];
            let mut actives = [None, None];
            for (index, view) in views.iter().enumerate() {
                if let Some(entries) = view.get("documents").and_then(Value::as_array) {
                    for entry in entries {
                        if let Some((id, _)) = buffers.iter().find(|(_, key)| {
                            entry.get("key").and_then(Value::as_str) == Some(key.as_str())
                        }) {
                            memberships[index].push(*id);
                            if entry.get("key") == view.get("active") {
                                actives[index] = Some(*id);
                            }
                        }
                    }
                }
                actives[index] = actives[index].or_else(|| memberships[index].first().copied());
            }
            let desired = if manifest["active_view"].as_u64() == Some(1) {
                ViewId::Secondary
            } else {
                ViewId::Primary
            };
            let desired = if actives[desired as usize].is_some() {
                desired
            } else {
                desired.other()
            };
            if actives.iter().any(Option::is_some) {
                workspace
                    .restore_view_layout(memberships, actives, desired)
                    .map_err(|error| invalid(&error.to_string()))?;
                for (index, view) in views.iter().enumerate() {
                    let target = if index == 0 {
                        ViewId::Primary
                    } else {
                        ViewId::Secondary
                    };
                    if let Some(entries) = view.get("documents").and_then(Value::as_array) {
                        for entry in entries {
                            let Some((id, key)) = buffers.iter().find(|(_, key)| {
                                entry.get("key").and_then(Value::as_str) == Some(key.as_str())
                            }) else {
                                continue;
                            };
                            let state = parse_view_state(&entry["state"])?;
                            workspace
                                .set_view_state(target, *id, state)
                                .map_err(|error| invalid(&error.to_string()))?;
                            workspace
                                .set_tab_color(
                                    target,
                                    *id,
                                    entry["color"].as_u64().map(|color| color as u8),
                                )
                                .map_err(|error| invalid(&error.to_string()))?;
                            if let Some(editor) = workspace.editor_in_view_mut(target, *id) {
                                let boundary = if source_offsets.contains(key) {
                                    RecoveryBoundary::Source(editor.document().byte_snapshot())
                                } else { RecoveryBoundary::Text(editor.committed_text().into()) };
                                if let Some(value) = entry.get("selection_set") {
                                    editor
                                        .set_selections(parse_selection_set(value, &boundary)?)
                                        .map_err(|error| invalid(&error.to_string()))?;
                                    continue;
                                }
                                let clamp = |value: &Value| {
                                    let offset = value
                                        .as_u64()
                                        .and_then(|value| usize::try_from(value).ok())
                                        .unwrap_or(0);
                                    boundary.position(offset)
                                };
                                editor
                                    .select(Selection {
                                        anchor: clamp(&entry["anchor"]),
                                        head: clamp(&entry["head"]),
                                    })
                                    .map_err(|error| invalid(&error.to_string()))?;
                            }
                        }
                    }
                }
                active = Some(workspace.active().id());
            }
        }
        if let Some(entries) = manifest.get("documents").and_then(Value::as_array) {
            for entry in entries {
                let Some((id, _)) = buffers.iter().find(|(_, key)| {
                    entry.get("key").and_then(Value::as_str) == Some(key.as_str())
                }) else {
                    continue;
                };
                workspace
                    .set_pinned(
                        *id,
                        entry
                            .get("pinned")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                    .map_err(|error| invalid(&error.to_string()))?;
                if let Some(buffer) = workspace.buffers_mut().find(|buffer| buffer.id() == *id) {
                    buffer.set_monitoring(
                        entry
                            .get("monitoring")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    );
                    buffer.set_user_read_only(
                        entry
                            .get("user_read_only")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    );
                }
                for view in [ViewId::Primary, ViewId::Secondary] {
                    if let Some(editor) = workspace.editor_in_view_mut(view, *id)
                        && let Some(read_only) =
                            entry.get("user_read_only").and_then(Value::as_bool)
                    {
                        editor.set_read_only(read_only || editor.document().is_read_only());
                    }
                }
                if let Some(marks) = entry.get("marks").and_then(Value::as_array) {
                    let view = if workspace.view_buffer_ids(ViewId::Primary).contains(id) {
                        ViewId::Primary
                    } else {
                        ViewId::Secondary
                    };
                    if let Some(editor) = workspace.editor_in_view_mut(view, *id) {
                        editor.clear_bookmarks();
                        for line in marks.iter().filter_map(Value::as_u64) {
                            if (line as usize) < editor.document().line_count() {
                                editor
                                    .toggle_bookmark(Some(line as usize))
                                    .map_err(|error| invalid(&error.to_string()))?;
                            }
                        }
                    }
                }
            }
        }
        if let Some(ui) = manifest.get("ui") {
            workspace
                .set_session_ui(ui.clone())
                .map_err(|error| invalid(&error.to_string()))?;
        }
        if manifest.get("recent_files").is_some() || manifest.get("recent_files_limit").is_some() {
            workspace
                .configure_recent_files(manifest)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        workspace.refresh_udl_metadata();
        Ok((buffers, active))
    }

    pub fn checkpoint(&mut self, workspace: &Workspace) -> io::Result<()> {
        self.checkpoint_selected(workspace, None, &BTreeSet::new())
    }

    fn assign(&mut self, id: BufferId) -> io::Result<String> {
        if let Some(key) = self.buffers.get(&id) {
            return Ok(key.clone());
        }
        let sequence = self.next_snapshot;
        self.next_snapshot = sequence
            .checked_add(1)
            .ok_or_else(|| invalid("recovery identities exhausted"))?;
        let key = format!("{}-buffer-{sequence}", self.key);
        self.buffers.insert(id, key.clone());
        Ok(key)
    }

    fn checkpoint_selected(
        &mut self,
        workspace: &Workspace,
        ids: Option<&BTreeSet<BufferId>>,
        discarded: &BTreeSet<BufferId>,
    ) -> io::Result<()> {
        validate_session_ui(workspace.session_ui())?;
        let defaults = serde_json::json!({"schema_version": 1, "snapshots": [], "next_snapshot": 1, "active": null});
        let previous_document = match self.store.load_json(&self.manifest_path()) {
            Ok(manifest) => Some(manifest),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let previous = match &previous_document {
            Some(manifest) => self.validate_manifest(manifest)?,
            None => Vec::new(),
        };
        let mut keys = Vec::new();
        let mut documents = Vec::new();
        let mut active = None;
        for buffer in workspace.buffers().iter().filter(|buffer| {
            (buffer.disk_file.is_some()
                || buffer.editor.document().len_bytes() > 0)
                && ids.is_none_or(|ids| ids.contains(&buffer.id()))
                && !(discarded.contains(&buffer.id()) && buffer.disk_file.is_none())
        }) {
            let key = self.assign(buffer.id())?;
            if buffer.is_large_file() && previous.contains(&key) {
                keys.push(key.clone());
            } else if self.snapshot_mode
                && buffer.is_modified()
                && !buffer.is_large_file()
                && !discarded.contains(&buffer.id())
            {
                let revision = buffer.editor.document().text_revision();
                let modified =
                    self.backup_revisions
                        .get(&buffer.id())
                        .is_none_or(|(previous, format, codepage)| {
                            !std::sync::Arc::ptr_eq(previous, &revision)
                                || *format != buffer.format()
                                || *codepage != buffer.byte_code_page()
                        });
                RecoverySnapshot::capture(buffer)?.save_with_modification(
                    &self.store,
                    &key,
                    modified,
                )?;
                self.backup_revisions
                    .insert(buffer.id(), (revision, buffer.format(), buffer.byte_code_page()));
                let metadata = self.store.load_json(&snapshot_metadata_path(&key)?)?;
                buffer.set_backup_modified(Some(
                    fs::metadata(raw_backup::path(&self.store, &metadata)?)?.modified()?,
                ));
                keys.push(key.clone());
            }
            if !keys.contains(&key) { buffer.set_backup_modified(None); }
            let selection = buffer.editor.document().selection();
            let path = buffer
                .disk_file
                .as_ref()
                .map(|file| serde_json::to_value(file.path()))
                .transpose()?;
            let fields = serde_json::json!({"key": key, "path": path, "anchor": selection.anchor.0, "head": selection.head.0, "language": buffer.language().key(), "udl":buffer.udl_name(),"codepage":buffer.byte_code_page().map(crate::files::ByteFormatCodePage::value),"pinned":buffer.is_pinned(),"monitoring":buffer.is_monitoring(),"user_read_only":buffer.is_user_read_only(),"marks":buffer.editor.document().bookmarked_lines().collect::<Vec<_>>()});
            let mut entry = previous_document
                .as_ref()
                .and_then(|manifest| manifest.get("documents"))
                .and_then(Value::as_array)
                .and_then(|entries| {
                    entries.iter().find(|entry| {
                        entry.get("key").and_then(Value::as_str) == Some(key.as_str())
                    })
                })
                .cloned()
                .unwrap_or_else(|| fields.clone());
            merge_fields(&mut entry, &fields);
            documents.push(entry);
            if buffer.id() == workspace.active().id() {
                active = Some(key.clone());
            }
        }
        let views = [ViewId::Primary, ViewId::Secondary].into_iter().map(|view| {
            let members = workspace.view_buffer_ids(view).iter().filter_map(|id| {
                let key = self.buffers.get(id)?;
                if !documents.iter().any(|document| document.get("key").and_then(Value::as_str) == Some(key.as_str())) { return None; }
                let editor = workspace.editor_in_view(view, *id)?;
                let selection = editor.document().selection();
                let state = workspace.view_state(view, *id).cloned().unwrap_or_default();
                Some(serde_json::json!({"key":key,"anchor":selection.anchor.0,"head":selection.head.0,"selection_set":selection_set_json(&editor.document().selections()),"color":workspace.tab_color(view,*id),"state":view_state_json(&state)}))
            }).collect::<Vec<_>>();
            let active = workspace.active_id_in_view(view).and_then(|id| self.buffers.get(&id)).filter(|key| members.iter().any(|member| member["key"].as_str() == Some(key.as_str()))).cloned().or_else(|| members.first().and_then(|member| member["key"].as_str()).map(str::to_owned));
            serde_json::json!({"documents":members,"active":active})
        }).collect::<Vec<_>>();
        let active_view = if views[workspace.active_view() as usize]["documents"]
            .as_array()
            .is_some_and(|members| !members.is_empty())
        {
            workspace.active_view() as usize
        } else if views[0]["documents"]
            .as_array()
            .is_some_and(|members| !members.is_empty())
        {
            0
        } else {
            1
        };
        let mut current = serde_json::json!({"schema_version": 1, "snapshots": keys, "documents": documents, "active": active, "next_snapshot": self.next_snapshot,"views":views,"active_view":active_view,"ui":workspace.session_ui(),"recent_files":workspace.recently_closed(),"recent_files_limit":workspace.recent_files_limit()});
        if let Some(previous) = &previous_document {
            preserve_manifest_extensions(previous, &mut current);
        }
        self.validate_manifest(&current)?;
        if !previous_document
            .as_ref()
            .is_some_and(|previous| contains_fields(previous, &current))
        {
            self.store
                .update_json(&self.manifest_path(), &defaults, |manifest| {
                    self.validate_manifest(manifest)?;
                    manifest["snapshots"] = serde_json::to_value(&keys)?;
                    manifest["documents"] = serde_json::to_value(&documents)?;
                    manifest["active"] = serde_json::to_value(&active)?;
                    manifest["next_snapshot"] = self.next_snapshot.into();
                    manifest["views"] = current["views"].clone();
                    manifest["active_view"] = current["active_view"].clone();
                    manifest["ui"] = current["ui"].clone();
                    manifest["recent_files"] = current["recent_files"].clone();
                    manifest["recent_files_limit"] = current["recent_files_limit"].clone();
                    Ok(())
                })?;
        }
        for key in previous.iter().filter(|key| !keys.contains(key)) {
            let path = snapshot_metadata_path(key)?;
            match self.store.load_json(&path) {
                Ok(document) => {
                    let backup = raw_backup::path(&self.store, &document)?;
                    match fs::remove_file(backup) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                    fs::remove_file(self.store.root().join(path))?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        self.buffers
            .retain(|id, _| workspace.buffers().iter().any(|buffer| buffer.id() == *id));
        self.backup_revisions
            .retain(|id, _| self.buffers.get(id).is_some_and(|key| keys.contains(key)));
        Ok(())
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn contains_fields(existing: &Value, current: &Value) -> bool {
    match (existing.as_object(), current.as_object()) {
        (Some(existing), Some(current)) => current.iter().all(|(key, value)| {
            existing
                .get(key)
                .is_some_and(|previous| contains_fields(previous, value))
        }),
        _ => existing == current,
    }
}

fn merge_fields(existing: &mut Value, current: &Value) {
    if let (Some(previous), Some(fields)) = (existing.as_object_mut(), current.as_object()) {
        for (key, value) in fields {
            match previous.get_mut(key) {
                Some(previous) => merge_fields(previous, value),
                None => {
                    previous.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *existing = current.clone();
    }
}

fn preserve_keyed_entries(previous: &Value, current: &mut Value) {
    let (Some(previous), Some(current)) = (previous.as_array(), current.as_array_mut()) else {
        return;
    };
    for entry in current {
        if let Some(old) = previous
            .iter()
            .find(|old| old.get("key") == entry.get("key"))
        {
            let mut merged = old.clone();
            merge_fields(&mut merged, entry);
            *entry = merged;
        }
    }
}

fn preserve_manifest_extensions(previous: &Value, current: &mut Value) {
    preserve_keyed_entries(&previous["documents"], &mut current["documents"]);
    if let Some(views) = current.get_mut("views").and_then(Value::as_array_mut) {
        for (index, view) in views.iter_mut().enumerate() {
            let old = &previous["views"][index];
            preserve_keyed_entries(&old["documents"], &mut view["documents"]);
            if old.is_object() {
                let mut merged = old.clone();
                merge_fields(&mut merged, view);
                *view = merged;
            }
        }
    }
}

fn format_name(format: UnicodeFormat) -> &'static str {
    match format {
        UnicodeFormat::Utf8(Utf8Format::Plain) => "utf8",
        UnicodeFormat::Utf8(Utf8Format::Bom) => "utf8-bom",
        UnicodeFormat::Utf16LeBom => "utf16-le-bom",
        UnicodeFormat::Utf16BeBom => "utf16-be-bom",
    }
}

fn parse_format(document: &Value) -> io::Result<UnicodeFormat> {
    match document.get("format").and_then(Value::as_str) {
        Some("utf8") => Ok(UnicodeFormat::Utf8(Utf8Format::Plain)),
        Some("utf8-bom") => Ok(UnicodeFormat::Utf8(Utf8Format::Bom)),
        Some("utf16-le-bom") => Ok(UnicodeFormat::Utf16LeBom),
        Some("utf16-be-bom") => Ok(UnicodeFormat::Utf16BeBom),
        _ => Err(invalid("unknown recovery encoding")),
    }
}

fn snapshot_path(key: &str) -> io::Result<PathBuf> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid recovery snapshot identity",
        ));
    }
    Ok(PathBuf::from("backup").join(format!("{key}.json")))
}

fn snapshot_metadata_path(key: &str) -> io::Result<PathBuf> {
    Ok(snapshot_path(key)?.with_extension("snapshot.json"))
}

impl RecoverySnapshot {
    fn uses_source_offsets(&self) -> io::Result<bool> {
        Ok(self.raw.is_some()
            && parse_format(&self.document)? == UnicodeFormat::Utf8(Utf8Format::Plain)
            && crate::files::ByteFormatCodePage::from_value(&self.document["codepage"])?.is_none())
    }

    pub fn capture(buffer: &Buffer) -> io::Result<Self> {
        if buffer.is_large_file() {
            return Err(invalid("large files do not use periodic backups"));
        }
        if !buffer.is_modified() {
            return Err(invalid(
                "a clean buffer does not require a recovery snapshot",
            ));
        }
        let selection = buffer.editor.document().selection();
        let mut disk = buffer
            .disk_file
            .as_ref()
            .map(DiskFile::recovery_state)
            .transpose()?;
        if let Some(state) = disk.as_mut().and_then(Value::as_object_mut) {
            state.remove("bytes");
        }
        let raw_utf8 = buffer.byte_code_page().is_none()
            && buffer.format() == UnicodeFormat::Utf8(Utf8Format::Plain);
        let raw = if raw_utf8 || buffer.byte_code_page() == Some(crate::files::ByteFormatCodePage::Unspecified) {
            Some(buffer.editor.document().byte_snapshot().to_vec())
        } else { None };
        Ok(Self {
            loaded_bytes: buffer
                .disk_file
                .as_ref()
                .map_or(0, |file| file.loaded_len() as u64),
            text: if raw_utf8 { String::new() } else { buffer.editor.document().text_snapshot().to_string() },
            raw,
            document: serde_json::json!({
                "schema_version": 1,
                "name": buffer.name(),
                "anchor": selection.anchor.0,
                "head": selection.head.0,
                "selection_set": selection_set_json(&buffer.editor.document().selections()),
                "line_ending": buffer.editor.line_ending().as_str(),
                "format": format_name(buffer.format()),
                "codepage":buffer.byte_code_page().map(crate::files::ByteFormatCodePage::value),
                "language": buffer.language().key(),
                "udl":buffer.udl_name(),
                "disk": disk,
            }),
        })
    }

    pub fn save(&self, store: &ConfigStore, key: &str) -> io::Result<()> {
        self.save_with_modification(store, key, false)
    }

    fn save_with_modification(
        &self,
        store: &ConfigStore,
        key: &str,
        modified: bool,
    ) -> io::Result<()> {
        let metadata_path = snapshot_metadata_path(key)?;
        let mut backup_missing = false;
        let mut metadata_has_body = false;
        let previous = match store.load_json(&metadata_path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => store.load_json(&snapshot_path(key)?),
            result => result,
        };
        let existing = match previous {
            Ok(document) => {
                metadata_has_body = document.get("text").is_some() || document["disk"].get("bytes").is_some();
                backup_missing = if document.get("backupFilePath").is_some() {
                    !raw_backup::path(store, &document)?.try_exists()?
                } else { true };
                if modified || backup_missing {
                    Some(Self { document,text:self.text.clone(),loaded_bytes:self.loaded_bytes,raw:self.raw.clone() })
                } else { Some(Self::from_document(store, document)?) }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let backup = match &existing {
            Some(snapshot) if snapshot.document.get("backupFilePath").is_some() => raw_backup::path(store, &snapshot.document)?,
            Some(_) => raw_backup::new_path(store, &self.document)?,
            None => raw_backup::new_path(store, &self.document)?,
        };
        if modified || backup_missing
            || existing.as_ref().is_none_or(|snapshot| {
                snapshot.text != self.text || snapshot.raw != self.raw || snapshot.document["format"] != self.document["format"] || snapshot.document["codepage"] != self.document["codepage"]
            })
        {
            if let Some(codepage) = crate::files::ByteFormatCodePage::from_value(&self.document["codepage"])? {
                use std::io::Write;
                let bytes = match &self.raw { Some(bytes) => bytes.clone(),None => codepage.encode(&self.text)? };
                let parent = backup.parent().ok_or_else(|| invalid("backup directory is missing"))?;
                let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
                temporary.write_all(&bytes)?;
                temporary.as_file().sync_all()?;
                temporary.persist(&backup).map_err(|error| error.error)?;
            } else {
                let bytes = self.raw.as_deref().unwrap_or(self.text.as_bytes());
                raw_backup::write(&backup,bytes,parse_format(&self.document)?,self.document["disk"].is_null())?;
            }
        }
        let mut current = self.document.clone();
        current["backupFilePath"] = serde_json::to_value(&backup)?;
        if metadata_has_body || !existing
            .as_ref()
            .is_some_and(|snapshot| contains_fields(&snapshot.document, &current))
        {
            let defaults = existing.as_ref().map(|snapshot| &snapshot.document).unwrap_or(&current);
            store.update_json(&metadata_path, defaults, |document| {
                merge_fields(document, &current);
                if let Some(fields) = document.as_object_mut() { fields.remove("text"); }
                if let Some(fields) = document["disk"].as_object_mut() { fields.remove("bytes"); }
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn load(store: &ConfigStore, key: &str) -> io::Result<Self> {
        let document = match store.load_json(&snapshot_metadata_path(key)?) {
            Ok(document) => document,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                store.load_json(&snapshot_path(key)?)?
            }
            Err(error) => return Err(error),
        };
        Self::from_document(store, document)
    }

    fn from_document(store: &ConfigStore, mut document: Value) -> io::Result<Self> {
        let mut raw = None;
        let (text, backup_bytes) = if let Some(text) = document.get("text").and_then(Value::as_str)
        {
            let bytes = match parse_format(&document)? {
                UnicodeFormat::Utf8(Utf8Format::Plain) => text.len() as u64,
                UnicodeFormat::Utf8(Utf8Format::Bom) => text.len() as u64 + 3,
                UnicodeFormat::Utf16LeBom | UnicodeFormat::Utf16BeBom => {
                    text.encode_utf16().count() as u64 * 2 + 2
                }
            };
            (text.to_owned(), bytes)
        } else {
            let path = raw_backup::path(store, &document)?;
            if let Some(codepage) = crate::files::ByteFormatCodePage::from_value(&document["codepage"])? {
                let bytes = fs::read(&path)?;
                let text = if codepage == crate::files::ByteFormatCodePage::Unspecified {
                    let text = String::from_utf8_lossy(&bytes).into_owned(); raw = Some(bytes.clone()); text
                } else { codepage.decode(&bytes)? };
                (text,bytes.len() as u64)
            } else if parse_format(&document)? == UnicodeFormat::Utf8(Utf8Format::Plain) {
                let bytes = raw_backup::read_bytes(&path)?;
                let length = bytes.len() as u64;
                raw = Some(bytes);
                (String::new(), length)
            } else { (raw_backup::read(&path,parse_format(&document)?)?,fs::metadata(path)?.len()) }
        };
        let loaded_bytes = if let Some(path) = document["disk"].get("path") {
            let path: PathBuf = serde_json::from_value(path.clone())?;
            match crate::file_access::existing(&path) {
                Ok(_access) => match fs::metadata(path) {
                    Ok(metadata) => metadata.len(),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => backup_bytes,
                    Err(error) => return Err(error),
                },
                Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied | io::ErrorKind::Interrupted) => backup_bytes,
                Err(error) => return Err(error),
            }
        } else {
            backup_bytes
        };
        if let Some(fields) = document.as_object_mut() {
            fields.remove("text");
        }
        let snapshot = Self {
            document,
            text,
            loaded_bytes,
            raw,
        };
        snapshot.decode()?;
        Ok(snapshot)
    }

    fn decode(&self) -> io::Result<(EditorSession, Option<DiskFile>, String, UnicodeFormat)> {
        let document = &self.document;
        parse_language(document)?;
        let name = document
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid("recovery name is missing"))?;
        let text = self.text.as_str();
        let offset = |key| -> io::Result<ByteOffset> {
            let value = document
                .get(key)
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("recovery position is missing"))?;
            Ok(ByteOffset(usize::try_from(value).map_err(|_| {
                invalid("recovery position exceeds platform range")
            })?))
        };
        let line_ending = match document.get("line_ending").and_then(Value::as_str) {
            Some("\r\n") => LineEnding::CrLf,
            Some("\n") => LineEnding::Lf,
            Some("\r") => LineEnding::Cr,
            _ => return Err(invalid("unknown recovery line ending")),
        };
        let format = parse_format(document)?;
        let raw_utf8 = self.uses_source_offsets()?;
        let disk = match document.get("disk") {
            Some(Value::Null) => None,
            Some(state) => {
                let mut state = state.clone();
                if state.get("bytes").is_none() {
                    let path: PathBuf = serde_json::from_value(state["path"].clone())?;
                    let _access = match crate::file_access::read(&path) {
                        Ok(access) => Some(access),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                        Err(error) => return Err(error),
                    };
                    match fs::read(&path) {
                        Ok(bytes) => {
                            state["bytes"] = serde_json::to_value(bytes)?;
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::MetadataExt;
                                let metadata = fs::metadata(&path)?;
                                state["device"] = metadata.dev().into();
                                state["inode"] = metadata.ino().into();
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            state["bytes"] = serde_json::json!([]);
                        }
                        Err(error) => return Err(error),
                    }
                }
                Some(DiskFile::from_recovery_state(&state, raw_utf8)?)
            }
            None => return Err(invalid("recovery disk association is missing")),
        };
        let mut editor = if let Some(bytes) = &self.raw {
            let mut editor = EditorSession::from_file_bytes(crate::byte_buffer::ByteBuffer::from_bytes(bytes));
            if raw_utf8 { editor.set_line_ending(line_ending); }
            else { editor.convert_line_endings(line_ending).map_err(|error| invalid(&error.to_string()))?; }
            let mut selection = Selection { anchor:offset("anchor")?,head:offset("head")? };
            if raw_utf8 {
                let boundary = RecoveryBoundary::Source(editor.document().byte_snapshot());
                selection = Selection { anchor:boundary.position(selection.anchor.0),head:boundary.position(selection.head.0) };
            }
            editor.select(selection).map_err(|error| invalid(&error.to_string()))?;
            editor.document().set_buffer_dirty(true);
            editor
        } else { EditorSession::from_recovery(text,Selection { anchor:offset("anchor")?,head:offset("head")? },line_ending).map_err(|error| invalid(&error.to_string()))? };
        if let Some(file) = &disk {
            editor.set_read_only(file.is_read_only());
        }
        if let Some(value) = document.get("selection_set") {
            let boundary = if raw_utf8 { RecoveryBoundary::Source(editor.document().byte_snapshot()) }
                else { RecoveryBoundary::Text(text.into()) };
            editor
                .set_selections(parse_selection_set(value, &boundary)?)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        Ok((editor, disk, name.into(), format))
    }

    pub fn restore_into(&self, workspace: &mut Workspace) -> io::Result<BufferId> {
        let (editor, disk, name, format) = self.restoration_parts()?;
        let language = parse_language(&self.document)?;
        let id = workspace
            .append_recovered_buffer_with_size(editor, disk, name, format, self.loaded_bytes)
            .map_err(|error| io::Error::other(error.to_string()))?;
        if let Some(language) = language {
            let language = if matches!(language,crate::syntax::Language::UserDefined(_)) {
                match self.document["udl"].as_str() { Some(name) => workspace.udl_language_named(name),None => workspace.udl_language_name(language).map(|_| language) }.unwrap_or(crate::syntax::Language::NormalText)
            } else { language };
            workspace.active_mut().set_language(language);
        }
        if self.document.get("codepage").is_some() { workspace.active_mut().restore_byte_code_page(crate::files::ByteFormatCodePage::from_value(&self.document["codepage"])?); }
        workspace.refresh_udl_metadata();
        Ok(id)
    }

    fn restoration_parts(
        &self,
    ) -> io::Result<(EditorSession, Option<DiskFile>, String, UnicodeFormat)> {
        let (mut editor, mut disk, name, format) = self.decode()?;
        if let Some(file) = &mut disk {
            match std::fs::metadata(file.path()) {
                Ok(metadata) => {
                    file.refresh_recovery_read_only(&metadata);
                    editor.set_read_only(file.is_read_only());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    disk = None;
                    editor.set_read_only(false);
                }
                Err(error) => return Err(error),
            }
        }
        Ok((editor, disk, name, format))
    }
}
