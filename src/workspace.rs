use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::files::{DiskFile, UnicodeFormat, Utf8Format};
use crate::input::EditorSession;

pub mod closing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BufferId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViewId {
    #[default]
    Primary,
    Secondary,
}

impl ViewId {
    pub fn other(self) -> Self {
        match self {
            Self::Primary => Self::Secondary,
            Self::Secondary => Self::Primary,
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Primary => 0,
            Self::Secondary => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDisposition {
    IfUnmodified,
    DiscardConfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseCondition {
    ExceptCurrent,
    ExceptPinned,
    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabSort {
    Name,
    Path,
    Type,
    Size,
    Modified,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentViewState {
    pub top_line: usize,
    pub viewport: Option<ViewportPosition>,
    pub horizontal_offset: f32,
    pub folded_lines: Vec<usize>,
    pub word_wrap: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewportPosition {
    pub first_visible_line: usize,
    pub wrap_offset: usize,
    pub wrap_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LargeFileRestriction {
    pub enabled: bool,
    pub threshold_bytes: u64,
    pub allow_auto_completion: bool,
    pub allow_brace_match: bool,
    pub allow_smart_highlight: bool,
    pub allow_clickable_link: bool,
    pub deactivate_word_wrap: bool,
    pub suppress_2gb_warning: bool,
}

impl Default for LargeFileRestriction {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_bytes: 200 * 1024 * 1024,
            allow_auto_completion: false,
            allow_brace_match: false,
            allow_smart_highlight: false,
            allow_clickable_link: false,
            deactivate_word_wrap: true,
            suppress_2gb_warning: false,
        }
    }
}

impl LargeFileRestriction {
    pub fn from_settings(settings: &serde_json::Value) -> std::io::Result<Self> {
        let Some(value) = settings.get("largeFileRestriction").or_else(|| settings.get("large_file_restriction")) else {
            return Ok(Self::default());
        };
        if !value.is_object() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "large_file_restriction must be an object",
            ));
        }
        let mut merged = value.clone();
        if let Some(old) = settings.get("large_file_restriction") {
            for (new,old_key) in [("isEnabled","enabled"),("fileSizeMB","file_size_mb"),("allowAutoCompletion","allow_auto_completion")] {
                if merged[new].is_null() && !old[old_key].is_null() { merged[new] = old[old_key].clone(); }
            }
        }
        let value = &merged;
        let enabled = match value.get("isEnabled").or_else(|| value.get("enabled")) {
            None => true,
            Some(value) => value.as_bool().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "large_file_restriction.enabled must be a boolean",
                )
            })?,
        };
        let size_mb = match value.get("fileSizeMB").or_else(|| value.get("file_size_mb")) {
            None => 200,
            Some(value) => {
                let number = value.as_u64().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "large_file_restriction.file_size_mb must be an unsigned integer",
                    )
                })?;
                if number <= 4096 {
                    number
                } else {
                    200
                }
            }
        };
        Ok(Self {
            enabled,
            threshold_bytes: size_mb * 1024 * 1024,
            allow_auto_completion: match value.get("allowAutoCompletion").or_else(|| value.get("allow_auto_completion")) {
                None => false,
                Some(value) => value.as_bool().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "large_file_restriction.allow_auto_completion must be a boolean",
                    )
                })?,
            },
            allow_brace_match: value["allowBraceMatch"].as_bool().unwrap_or(false),
            allow_smart_highlight: value["allowSmartHilite"].as_bool().unwrap_or(false),
            allow_clickable_link: value["allowClickableLink"].as_bool().unwrap_or(false),
            deactivate_word_wrap: value["deactivateWordWrap"].as_bool().unwrap_or(true),
            suppress_2gb_warning: value["suppress2GBWarning"].as_bool().unwrap_or(false),
        })
    }

    fn classifies(self, loaded_bytes: u64) -> bool {
        self.enabled && loaded_bytes >= self.threshold_bytes
    }
}

fn compare_tab_text(left: &str, right: &str) -> std::cmp::Ordering {
    let left = left.encode_utf16().collect::<Vec<_>>();
    let right = right.encode_utf16().collect::<Vec<_>>();
    let (mut l, mut r) = (0, 0);
    while l < left.len() && r < right.len() {
        if (48..=57).contains(&left[l]) && (48..=57).contains(&right[r]) {
            let (start_l, start_r) = (l, r);
            let (mut number_l, mut number_r) = (0_i32, 0_i32);
            while l < left.len() && (48..=57).contains(&left[l]) {
                number_l = number_l
                    .saturating_mul(10)
                    .saturating_add(i32::from(left[l] - 48));
                l += 1;
            }
            while r < right.len() && (48..=57).contains(&right[r]) {
                number_r = number_r
                    .saturating_mul(10)
                    .saturating_add(i32::from(right[r] - 48));
                r += 1;
            }
            let order = number_l
                .cmp(&number_r)
                .then_with(|| (r - start_r).cmp(&(l - start_l)));
            if !order.is_eq() {
                return order;
            }
        } else {
            let fold = |ch: u16| if (65..=90).contains(&ch) { ch + 32 } else { ch };
            let order = fold(left[l]).cmp(&fold(right[r]));
            if !order.is_eq() {
                return order;
            }
            l += 1;
            r += 1;
        }
    }
    (left.len() - l).cmp(&(right.len() - r))
}

pub struct Buffer {
    id: BufferId,
    pub editor: EditorSession,
    pub disk_file: Option<DiskFile>,
    name: String,
    format: UnicodeFormat,
    byte_code_page: Option<crate::files::ByteFormatCodePage>,
    udl_name: Option<String>,
    untitled_tab_renamed: bool,
    initial_placeholder: bool,
    format_identity: Arc<()>,
    language: crate::syntax::Language,
    language_explicit: bool,
    editor_view: ViewId,
    other_editor: Option<EditorSession>,
    pinned: bool,
    monitoring: bool,
    user_read_only: bool,
    backup_modified: std::cell::Cell<Option<std::time::SystemTime>>,
    large_file: bool,
}

impl Buffer {
    pub(crate) fn window_modified_time(&self) -> Option<std::time::SystemTime> {
        self.backup_modified.get().or_else(|| self.disk_file.as_ref().and_then(DiskFile::original_modified))
    }

    pub fn udl_name(&self) -> Option<&str> { self.udl_name.as_deref() }
    pub fn byte_code_page(&self) -> Option<crate::files::ByteFormatCodePage> { self.byte_code_page }

    pub fn set_byte_code_page(&mut self,codepage: Option<crate::files::ByteFormatCodePage>) {
        if self.byte_code_page != codepage {
            self.byte_code_page = codepage;
            self.format_identity = Arc::new(());
            self.editor.document().set_buffer_dirty(true);
        }
    }

    pub(crate) fn restore_byte_code_page(&mut self,codepage: Option<crate::files::ByteFormatCodePage>) { self.byte_code_page = codepage; }

    pub fn select_source_byte_format(&mut self, format: UnicodeFormat, codepage: Option<crate::files::ByteFormatCodePage>) {
        self.format = format;
        self.byte_code_page = codepage;
        self.format_identity = Arc::new(());
    }

    pub fn reload_selected_encoding(&mut self, file: DiskFile, bytes: crate::byte_buffer::ByteBuffer) -> Result<(), WorkspaceError> {
        if !self.disk_file.as_ref().is_some_and(|original| original.path() == file.path()) {
            return Err(WorkspaceError::ReloadTargetMismatch(self.id));
        }
        if self.is_modified() {
            return Err(WorkspaceError::UnsavedBuffer(self.id));
        }
        let clamp = |position: crate::editor::ByteOffset| crate::editor::ByteOffset(position.0.min(bytes.len()));
        let selection = self.editor.document().selection();
        let mut editor = EditorSession::from_file_bytes(bytes.clone());
        editor.select(crate::editor::Selection { anchor: clamp(selection.anchor), head: clamp(selection.head) }).map_err(WorkspaceError::Edit)?;
        editor.set_read_only(file.is_read_only() || self.user_read_only || self.monitoring);
        editor.set_overwrite(self.editor.overwrite());
        let other_editor = if let Some(other) = &self.other_editor {
            let selection = other.document().selection();
            let mut replacement = editor.clone_view();
            replacement.select(crate::editor::Selection { anchor: clamp(selection.anchor), head: clamp(selection.head) }).map_err(WorkspaceError::Edit)?;
            replacement.set_overwrite(other.overwrite());
            Some(replacement)
        } else {
            None
        };
        self.editor = editor;
        self.other_editor = other_editor;
        self.initial_placeholder = false;
        self.select_source_byte_format(file.format(), file.byte_code_page());
        self.set_backup_modified(None);
        self.disk_file = Some(file);
        Ok(())
    }

    pub fn is_large_file(&self) -> bool {
        self.large_file
    }
    pub(crate) fn set_backup_modified(&self, time: Option<std::time::SystemTime>) {
        self.backup_modified.set(time);
    }
    pub fn is_user_read_only(&self) -> bool {
        self.user_read_only
            || (!self.monitoring
                && !self.disk_file.as_ref().is_some_and(DiskFile::is_read_only)
                && self.editor.document().is_read_only())
    }
    pub fn set_user_read_only(&mut self, read_only: bool) {
        self.user_read_only = read_only;
        let effective = read_only
            || self.monitoring
            || self.disk_file.as_ref().is_some_and(DiskFile::is_read_only);
        self.editor.set_read_only(effective);
        if let Some(editor) = &mut self.other_editor {
            editor.set_read_only(effective);
        }
    }
    pub fn is_monitoring(&self) -> bool {
        self.monitoring
    }
    pub(crate) fn set_monitoring(&mut self, monitoring: bool) {
        self.monitoring = monitoring;
    }
    pub fn is_pinned(&self) -> bool {
        self.pinned
    }
    pub fn language(&self) -> crate::syntax::Language {
        self.language
    }
    pub fn set_language(&mut self, language: crate::syntax::Language) {
        self.udl_name = None;
        self.language = language;
        self.language_explicit = true;
    }
    pub fn language_is_explicit(&self) -> bool {
        self.language_explicit
    }
    pub fn set_detected_language(&mut self, language: crate::syntax::Language) {
        self.language = language;
        self.language_explicit = false;
    }
    pub fn format(&self) -> UnicodeFormat {
        self.format
    }

    pub fn is_modified(&self) -> bool {
        self.editor.document().buffer_is_modified()
    }

    pub fn select_format(&mut self, format: UnicodeFormat) -> bool {
        if self.format == format && self.byte_code_page.is_none() {
            return false;
        }
        self.format = format;
        self.byte_code_page = None;
        self.editor.document().set_buffer_dirty(true);
        self.format_identity = Arc::new(());
        true
    }

    pub fn save_intent(&self) -> BufferSaveIntent {
        BufferSaveIntent {
            id: self.id,
            snapshot: self.editor.save_snapshot(),
            format: self.format,
            byte_code_page: self.byte_code_page,
            format_identity: self.format_identity.clone(),
        }
    }

    pub fn convert_format(&mut self, format: UnicodeFormat) -> bool {
        let had_codepage = self.byte_code_page.is_some();
        if !self.select_format(format) {
            return false;
        }
        if !had_codepage {
            self.editor.clear_history();
        }
        true
    }
    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn name(&self) -> String {
        self.disk_file
            .as_ref()
            .and_then(|file| file.path().file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone())
    }
}

#[derive(Clone)]
pub struct BufferSaveIntent {
    id: BufferId,
    snapshot: crate::editor::SaveSnapshot,
    format: UnicodeFormat,
    byte_code_page: Option<crate::files::ByteFormatCodePage>,
    format_identity: Arc<()>,
}

impl BufferSaveIntent {
    pub fn byte_code_page(&self) -> Option<crate::files::ByteFormatCodePage> { self.byte_code_page }
    pub fn buffer_id(&self) -> BufferId {
        self.id
    }
    pub fn snapshot(&self) -> &crate::editor::SaveSnapshot {
        &self.snapshot
    }
    pub fn format(&self) -> UnicodeFormat {
        self.format
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveCompletion {
    Primary,
    Copy,
}

#[derive(Debug)]
pub enum WorkspaceError {
    UnknownBuffer(BufferId),
    IdExhausted,
    UnsavedBuffer(BufferId),
    ReloadTargetMismatch(BufferId),
    SavedFormatMismatch(BufferId),
    EmptyView(ViewId),
    NotInView(ViewId, BufferId),
    InvalidTabColor,
    InvalidSessionState,
    Encoding(std::io::Error),
    UnknownUserLanguage(String),
    Edit(crate::editor::EditError),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTabColor => formatter.write_str("tab color must be between 0 and 4"),
            Self::InvalidSessionState => formatter.write_str("invalid session view state"),
            Self::Encoding(error) => write!(formatter,"{error}"),
            Self::UnknownUserLanguage(name) => write!(formatter,"用户语言“{name}”不可用，请在新文档首选项中重新选择语言"),
            Self::UnknownBuffer(id) => write!(formatter, "buffer {} is no longer open", id.0),
            Self::EmptyView(view) => write!(formatter, "view {view:?} has no open buffers"),
            Self::NotInView(view, id) => {
                write!(formatter, "buffer {} is not in view {view:?}", id.0)
            }
            Self::IdExhausted => formatter.write_str("buffer identity counter exhausted"),
            Self::UnsavedBuffer(id) => write!(formatter, "buffer {} has unsaved changes", id.0),
            Self::ReloadTargetMismatch(id) => {
                write!(formatter, "reload target does not match buffer {}", id.0)
            }
            Self::SavedFormatMismatch(id) => write!(
                formatter,
                "saved format does not match buffer {} request",
                id.0
            ),
            Self::Edit(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for WorkspaceError {}

pub struct Workspace {
    // Only this type can mutate membership/active index; there is always at least one buffer.
    buffers: Vec<Buffer>,
    active: usize,
    next_id: u64,
    active_view: ViewId,
    memberships: [Vec<BufferId>; 2],
    view_active: [Option<BufferId>; 2],
    tab_colors: [BTreeMap<BufferId, u8>; 2],
    mru: Vec<(ViewId, BufferId)>,
    mru_cycle: Option<(Vec<(ViewId, BufferId)>, usize)>,
    recently_closed: Vec<std::path::PathBuf>,
    recent_files_limit: usize,
    view_states: [BTreeMap<BufferId, DocumentViewState>; 2],
    session_ui: serde_json::Value,
    large_file_restriction: LargeFileRestriction,
    new_document_settings: serde_json::Value,
    keep_absent_session_files: bool,
    opened_large_file: bool,
    new_document_language: crate::syntax::Language,
    udl_names: BTreeMap<u64,String>,
    file_settings: serde_json::Value,
    initial_document_configured: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_large_file_restriction(LargeFileRestriction::default())
    }

    pub fn with_large_file_restriction(large_file_restriction: LargeFileRestriction) -> Self {
        let mut initial_editor = EditorSession::new("");
        if let Err(error) = initial_editor.convert_line_endings(crate::input::LineEnding::Lf) { eprintln!("new document line ending could not be set: {error}"); }
        Self {
            buffers: vec![Buffer {
                editor_view: ViewId::Primary,
                other_editor: None,
                pinned: false,
                monitoring: false,
                user_read_only: false,
                backup_modified: std::cell::Cell::new(None),
                large_file: false,
                id: BufferId(1),
                editor: initial_editor,
                disk_file: None,
                name: "new 1".into(),
                format: UnicodeFormat::Utf8(Utf8Format::Plain),
                byte_code_page: None,
                udl_name: None,
                untitled_tab_renamed: false,
                initial_placeholder: true,
                format_identity: Arc::new(()),
                language: crate::syntax::Language::NormalText,
                language_explicit: false,
            }],
            active: 0,
            next_id: 2,
            active_view: ViewId::Primary,
            memberships: [vec![BufferId(1)], Vec::new()],
            view_active: [Some(BufferId(1)), None],
            tab_colors: Default::default(),
            mru: vec![(ViewId::Primary, BufferId(1))],
            mru_cycle: None,
            recently_closed: Vec::new(),
            recent_files_limit: 10,
            view_states: Default::default(),
            session_ui: serde_json::json!({}),
            large_file_restriction,
            new_document_settings: serde_json::json!({}),
            keep_absent_session_files: false,
            opened_large_file: false,
            new_document_language: crate::syntax::Language::NormalText,
            udl_names: BTreeMap::new(),
            file_settings: serde_json::json!({}),
            initial_document_configured: false,
        }
    }

    pub fn active(&self) -> &Buffer {
        &self.buffers[self.active]
    }

    pub fn set_large_file_restriction(&mut self, policy: LargeFileRestriction) {
        self.large_file_restriction = policy;
    }

    pub fn auto_completion_allowed(&self) -> bool {
        !self.active().is_large_file()
            || self.large_file_restriction.allow_auto_completion
            || !self.large_file_restriction.enabled
    }

    pub fn large_file_restriction(&self) -> LargeFileRestriction {
        self.large_file_restriction
    }

    pub fn brace_match_allowed(&self) -> bool {
        !self.large_file_restriction.enabled || !self.active().is_large_file() || self.large_file_restriction.allow_brace_match
    }

    pub fn smart_highlight_allowed(&self) -> bool {
        !self.large_file_restriction.enabled || !self.active().is_large_file() || self.large_file_restriction.allow_smart_highlight
    }

    pub fn clickable_links_allowed(&self) -> bool {
        !self.large_file_restriction.enabled || !self.active().is_large_file() || self.large_file_restriction.allow_clickable_link
    }

    pub fn syntax_highlighting_allowed(&self) -> bool {
        !self.active().is_large_file()
    }

    pub fn take_large_file_word_wrap_disable(&mut self) -> bool {
        std::mem::take(&mut self.opened_large_file)
            && self.large_file_restriction.enabled && self.large_file_restriction.deactivate_word_wrap
    }

    pub fn configure_new_document(&mut self, settings: &serde_json::Value) -> std::io::Result<()> {
        let value = settings.get("NewDocDefaultSettings").cloned().unwrap_or_else(|| serde_json::json!({}));
        if !value.is_object() { return Err(std::io::Error::other("NewDocDefaultSettings must be an object")); }
        self.new_document_settings = value;
        self.file_settings = settings.clone();
        self.new_document_language = match self.new_document_settings["lang"].as_u64().unwrap_or(0) {
            0 | 15 => crate::syntax::Language::NormalText,
            57 => crate::syntax::Language::Json,
            id => crate::syntax::Language::Builtin(u16::try_from(id).map_err(|_| std::io::Error::other("new document language is out of range"))?),
        };
        self.keep_absent_session_files = settings["KeepSessionAbsentFileEntries"].as_bool().or_else(|| settings["keepSessionAbsentFileEntries"].as_bool()).unwrap_or(false);
        Ok(())
    }

    pub fn set_new_document_language(&mut self,language: crate::syntax::Language) {
        self.new_document_language = language;
    }

    pub fn file_settings(&self) -> &serde_json::Value { &self.file_settings }

    pub fn active_tab_width(&self) -> usize {
        let name = match self.active().language() {
            crate::syntax::Language::NormalText => Some("normal"),
            crate::syntax::Language::Json => Some("json"),
            crate::syntax::Language::Builtin(id) => crate::languages::catalog().ok().and_then(|catalog| catalog.by_id(id)).map(|language| language.name.as_str()),
            crate::syntax::Language::UserDefined(_) => None,
        };
        let default = self.file_settings["TabSettings"]["size"].as_i64().filter(|size| *size > 0).unwrap_or(4);
        let value = name.and_then(|name| self.file_settings["TabSettings"]["languages"][name]["size"].as_i64()).filter(|size| *size > 0).unwrap_or(default);
        usize::try_from(value).unwrap_or(4)
    }

    pub fn synchronize_udl_names(&mut self,names: impl IntoIterator<Item = (u64,String)>) {
        self.udl_names = names.into_iter().collect();
        self.normalize_new_document_language();
        self.refresh_udl_metadata();
    }

    pub fn refresh_udl_metadata(&mut self) {
        for buffer in &mut self.buffers {
            buffer.udl_name = match buffer.language { crate::syntax::Language::UserDefined(id) => self.udl_names.get(&id).cloned(),_ => None };
        }
    }

    pub fn udl_language_named(&self,name: &str) -> Option<crate::syntax::Language> {
        self.udl_names.iter().find(|(_,stored)| stored.as_str() == name).map(|(id,_)| crate::syntax::Language::UserDefined(*id))
    }

    pub fn udl_language_name(&self,language: crate::syntax::Language) -> Option<&str> {
        match language { crate::syntax::Language::UserDefined(id) => self.udl_names.get(&id).map(String::as_str),_ => None }
    }

    pub fn use_first_line_as_tab_name(&mut self,id: BufferId,modified_line: usize) -> bool {
        if modified_line != 0 || !self.new_document_settings["useContentAsTabName"].as_bool().unwrap_or(false) { return false; }
        let Some(buffer) = self.buffers.iter().find(|buffer| buffer.id() == id) else { return false; };
        if buffer.disk_file.is_some() || buffer.untitled_tab_renamed { return false; }
        let text = buffer.editor.committed_text();
        let line = text.split(['\r','\n']).next().unwrap_or_default().trim();
        let mut length = 0;
        let name: String = line.chars().filter(|ch| !matches!(ch,'\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\t' | '\r' | '\n')).take_while(|ch| { length += ch.len_utf16(); length <= 63 }).collect();
        if name.is_empty() || self.buffers.iter().any(|buffer| buffer.name() == name) { return false; }
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| buffer.id() == id) else { return false; };
        buffer.name = name;
        true
    }

    fn new_document_format(&self) -> Result<(UnicodeFormat,Option<crate::files::ByteFormatCodePage>),WorkspaceError> {
        let requested = self.new_document_settings["encoding"].as_i64().unwrap_or(4);
        let codepage = self.new_document_settings["codepage"].as_i64().unwrap_or(-1);
        let bytes = if codepage >= 0 {
            let format = crate::files::ByteFormatCodePage::from_value(&codepage.into()).map_err(WorkspaceError::Encoding)?;
            if let Some(format) = format { format.encode("").map_err(WorkspaceError::Encoding)?; }
            format
        } else if requested == 0 {
            return Err(WorkspaceError::Encoding(std::io::Error::other("Windows 默认 ACP 已排除，请在新文档首选项选择 UTF-8、UTF-16 或显式 Windows/OEM 字符集")));
        } else { None };
        let format = if bytes.is_some() { UnicodeFormat::Utf8(Utf8Format::Plain) } else { match requested {
            1 => UnicodeFormat::Utf8(Utf8Format::Bom),2 => UnicodeFormat::Utf16BeBom,3 => UnicodeFormat::Utf16LeBom,_ => UnicodeFormat::Utf8(Utf8Format::Plain),
        } };
        Ok((format,bytes))
    }

    fn normalize_new_document_language(&mut self) {
        if self.new_document_settings["lang"].as_i64() != Some(15) { return; }
        let name = self.new_document_settings["udl"].as_str().unwrap_or_default();
        if !name.is_empty() && self.udl_language_named(name).is_some() { return; }
        self.new_document_settings["lang"] = 0.into();
        self.new_document_settings["udl"] = "".into();
        self.file_settings["NewDocDefaultSettings"] = self.new_document_settings.clone();
        self.new_document_language = crate::syntax::Language::NormalText;
    }

    fn resolved_new_document_language(&mut self) -> Result<crate::syntax::Language,WorkspaceError> {
        self.normalize_new_document_language();
        if self.new_document_settings["lang"].as_i64() == Some(15) {
            let name = self.new_document_settings["udl"].as_str().unwrap_or_default();
            Ok(self.udl_language_named(name).unwrap_or(crate::syntax::Language::NormalText))
        } else { Ok(self.new_document_language) }
    }

    pub fn apply_initial_new_document_defaults(&mut self) -> Result<(),WorkspaceError> {
        if self.initial_document_configured || !self.active().initial_placeholder { return Ok(()); }
        if self.buffers.len() != 1 || self.active().disk_file.is_some() || self.active().is_modified()
            || self.active().editor.document().len_bytes() != 0 || self.active().language_is_explicit() { return Ok(()); }
        let (format,codepage) = self.new_document_format()?;
        let ending = match self.new_document_settings["format"].as_i64().unwrap_or(2) { 0 => crate::input::LineEnding::CrLf,1 => crate::input::LineEnding::Cr,_ => crate::input::LineEnding::Lf };
        let language = self.resolved_new_document_language()?;
        let name = self.udl_language_name(language).map(str::to_owned);
        let buffer = self.active_mut();
        buffer.editor.convert_line_endings(ending).map_err(WorkspaceError::Edit)?;
        buffer.format = format;
        buffer.byte_code_page = codepage;
        if language == crate::syntax::Language::NormalText { buffer.set_detected_language(language); } else { buffer.set_language(language); }
        buffer.udl_name = name;
        buffer.initial_placeholder = false;
        self.initial_document_configured = true;
        Ok(())
    }

    pub fn validate_recent_files_at_startup(&mut self,settings: &serde_json::Value) {
        if settings["validate_recent_files_at_startup"].as_bool().unwrap_or(false) { self.recently_closed.retain(|path| path.is_file()); }
    }

    pub fn disable_word_wrap_in_views(&mut self) {
        for view in &mut self.view_states { for state in view.values_mut() { state.word_wrap = false; } }
    }

    pub(crate) fn keep_absent_session_files(&self) -> bool {
        self.keep_absent_session_files
    }

    pub fn session_ui(&self) -> &serde_json::Value {
        &self.session_ui
    }

    pub fn set_session_ui(&mut self, snapshot: serde_json::Value) -> Result<(), WorkspaceError> {
        crate::recovery::validate_session_ui(&snapshot)
            .map_err(|_| WorkspaceError::InvalidSessionState)?;
        self.session_ui = snapshot;
        Ok(())
    }

    pub fn view_state(&self, view: ViewId, id: BufferId) -> Option<&DocumentViewState> {
        self.view_states[view.index()].get(&id)
    }

    pub fn set_view_state(
        &mut self,
        view: ViewId,
        id: BufferId,
        state: DocumentViewState,
    ) -> Result<(), WorkspaceError> {
        if !self.view_buffer_ids(view).contains(&id) {
            return Err(WorkspaceError::NotInView(view, id));
        }
        if !state.horizontal_offset.is_finite()
            || state.horizontal_offset < 0.
            || state.viewport.as_ref().is_some_and(|position| {
                position.wrap_count == 0 || position.wrap_offset >= position.wrap_count
            })
        {
            return Err(WorkspaceError::InvalidSessionState);
        }
        self.view_states[view.index()].insert(id, state);
        Ok(())
    }

    pub(crate) fn restore_view_layout(
        &mut self,
        memberships: [Vec<BufferId>; 2],
        active: [Option<BufferId>; 2],
        view: ViewId,
    ) -> Result<(), WorkspaceError> {
        for (index, members) in memberships.iter().enumerate() {
            if members.iter().enumerate().any(|(position, id)| {
                !self.buffers.iter().any(|buffer| buffer.id == *id)
                    || members[..position].contains(id)
            }) || active[index].is_some_and(|id| !members.contains(&id))
                || members.is_empty() != active[index].is_none()
            {
                return Err(WorkspaceError::InvalidSessionState);
            }
        }
        if active[view.index()].is_none() {
            return Err(WorkspaceError::InvalidSessionState);
        }
        for buffer in &mut self.buffers {
            let primary = memberships[0].contains(&buffer.id);
            let secondary = memberships[1].contains(&buffer.id);
            if !primary && !secondary {
                return Err(WorkspaceError::InvalidSessionState);
            }
            if primary && secondary {
                if buffer.other_editor.is_none() {
                    buffer.other_editor = Some(buffer.editor.clone_view());
                }
            } else {
                let target = if primary {
                    ViewId::Primary
                } else {
                    ViewId::Secondary
                };
                if buffer.editor_view != target
                    && let Some(other) = &mut buffer.other_editor
                {
                    std::mem::swap(&mut buffer.editor, other);
                }
                buffer.editor_view = target;
                buffer.other_editor = None;
            }
        }
        self.memberships = memberships;
        self.view_active = active;
        self.mru.clear();
        self.mru_cycle = None;
        for target in [ViewId::Primary, ViewId::Secondary] {
            for id in self.memberships[target.index()].clone() {
                self.record_activation(target, id);
            }
        }
        self.activate_view(view)
    }

    fn record_activation(&mut self, view: ViewId, id: BufferId) {
        if self.mru_cycle.is_none() {
            self.mru.retain(|entry| *entry != (view, id));
            self.mru.insert(0, (view, id));
        }
    }

    pub fn cycle_mru(&mut self, forward: bool) -> Result<BufferId, WorkspaceError> {
        self.cycle_documents(forward, true)
    }

    pub fn cycle_documents(
        &mut self,
        forward: bool,
        mru_order: bool,
    ) -> Result<BufferId, WorkspaceError> {
        if self.mru_cycle.is_none() {
            let entries = if mru_order {
                self.mru
                    .iter()
                    .copied()
                    .filter(|(view, id)| self.view_buffer_ids(*view).contains(id))
                    .collect::<Vec<_>>()
            } else {
                [ViewId::Primary, ViewId::Secondary]
                    .into_iter()
                    .flat_map(|view| self.view_buffer_ids(view).iter().map(move |id| (view, *id)))
                    .collect()
            };
            let current = entries
                .iter()
                .position(|entry| *entry == (self.active_view, self.active().id))
                .unwrap_or(0);
            self.mru_cycle = Some((entries, current));
        }
        let Some((entries, index)) = &mut self.mru_cycle else {
            return Ok(self.active().id);
        };
        if entries.is_empty() {
            return Ok(self.active().id);
        }
        *index = if forward {
            (*index + 1) % entries.len()
        } else {
            (*index + entries.len() - 1) % entries.len()
        };
        let (_, id) = entries[*index];
        Ok(id)
    }

    pub fn finish_mru_cycle(&mut self) -> Result<Option<BufferId>, WorkspaceError> {
        let Some((entries, index)) = self.mru_cycle.take() else {
            return Ok(None);
        };
        let Some((view, id)) = entries.get(index).copied() else {
            return Ok(None);
        };
        self.activate_in_view(view, id)?;
        Ok(Some(id))
    }

    pub fn cancel_mru_cycle(&mut self) {
        self.mru_cycle = None;
    }

    pub fn mru_cycle_selection(&self) -> Option<(ViewId, BufferId)> {
        self.mru_cycle
            .as_ref()
            .and_then(|(entries, index)| entries.get(*index).copied())
    }

    pub fn mru_cycle_candidates(&self) -> Option<&[(ViewId, BufferId)]> {
        self.mru_cycle
            .as_ref()
            .map(|(entries, _)| entries.as_slice())
    }

    pub fn recently_closed(&self) -> &[std::path::PathBuf] {
        &self.recently_closed
    }

    pub fn clear_recent_files(&mut self) {
        self.recently_closed.clear();
    }

    pub fn rename_untitled(&mut self, id: BufferId, name: &str) -> std::io::Result<()> {
        let name = name.trim();
        if name.is_empty()
            || name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\t', '\0'])
            || name.encode_utf16().count() > 63
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "名称不能为空，且必须是单个文件名",
            ));
        }
        if self
            .buffers
            .iter()
            .any(|buffer| buffer.id != id && buffer.name().eq_ignore_ascii_case(name))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "该名称已被其他标签页使用",
            ));
        }
        let buffer = self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.id == id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "文档已关闭"))?;
        if buffer
            .disk_file
            .as_ref()
            .is_some_and(|file| file.path().is_file())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "磁盘文件需要使用文件重命名",
            ));
        }
        if buffer.disk_file.take().is_some() {
            buffer.editor.document().set_buffer_dirty(true);
            buffer.set_user_read_only(buffer.user_read_only);
        }
        buffer.name = name.to_string();
        buffer.untitled_tab_renamed = true;
        if !buffer.language_is_explicit() {
            buffer.set_detected_language(crate::syntax::Language::from_path(std::path::Path::new(
                name,
            )));
        }
        Ok(())
    }

    pub fn recent_files_limit(&self) -> usize {
        self.recent_files_limit
    }

    pub fn configure_recent_files(
        &mut self,
        settings: &serde_json::Value,
    ) -> Result<(), WorkspaceError> {
        let limit = match settings.get("recent_files_limit") {
            None => self.recent_files_limit,
            Some(value) => value
                .as_u64()
                .map(|limit| limit.min(30) as usize)
                .ok_or(WorkspaceError::InvalidSessionState)?,
        };
        let paths = match settings.get("recent_files") {
            None => self.recently_closed.clone(),
            Some(value) => {
                let values = value
                    .as_array()
                    .ok_or(WorkspaceError::InvalidSessionState)?;
                let mut paths = Vec::new();
                for value in values {
                    let path: std::path::PathBuf = serde_json::from_value(value.clone())
                        .map_err(|_| WorkspaceError::InvalidSessionState)?;
                    if !path.is_absolute() {
                        return Err(WorkspaceError::InvalidSessionState);
                    }
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
                paths
            }
        };
        self.recent_files_limit = limit;
        self.recently_closed = paths;
        self.recently_closed.truncate(limit);
        Ok(())
    }

    pub fn reopen_last_closed(&mut self) -> std::io::Result<Option<BufferId>> {
        let Some(path) = self.recently_closed.first().cloned() else {
            return Ok(None);
        };
        let (file, bytes) = DiskFile::load_buffer_with_preferences(&path,&self.file_settings)?;
        let id = self
            .open_buffer_bytes(file, bytes)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok(Some(id))
    }

    pub fn set_pinned(&mut self, id: BufferId, pinned: bool) -> Result<(), WorkspaceError> {
        let buffer = self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.id == id)
            .ok_or(WorkspaceError::UnknownBuffer(id))?;
        buffer.pinned = pinned;
        self.partition_pinned();
        Ok(())
    }

    fn partition_pinned(&mut self) {
        for members in &mut self.memberships {
            members.sort_by_key(|id| {
                !self
                    .buffers
                    .iter()
                    .any(|buffer| buffer.id == *id && buffer.pinned)
            });
        }
    }

    pub fn tab_color(&self, view: ViewId, id: BufferId) -> Option<u8> {
        self.tab_colors[view.index()].get(&id).copied()
    }

    pub fn set_tab_color(
        &mut self,
        view: ViewId,
        id: BufferId,
        color: Option<u8>,
    ) -> Result<(), WorkspaceError> {
        if !self.view_buffer_ids(view).contains(&id) {
            return Err(WorkspaceError::NotInView(view, id));
        }
        if color.is_some_and(|color| color > 4) {
            return Err(WorkspaceError::InvalidTabColor);
        }
        match color {
            Some(color) => {
                self.tab_colors[view.index()].insert(id, color);
            }
            None => {
                self.tab_colors[view.index()].remove(&id);
            }
        }
        Ok(())
    }

    pub fn move_tab(&mut self, id: BufferId, target: usize) -> Result<bool, WorkspaceError> {
        let members = &mut self.memberships[self.active_view.index()];
        let source = members
            .iter()
            .position(|member| *member == id)
            .ok_or(WorkspaceError::NotInView(self.active_view, id))?;
        let Some(other) = members.get(target) else {
            return Ok(false);
        };
        let pinned = |id| {
            self.buffers
                .iter()
                .any(|buffer| buffer.id == id && buffer.pinned)
        };
        if pinned(id) != pinned(*other) || source == target {
            return Ok(false);
        }
        members.remove(source);
        members.insert(target, id);
        Ok(true)
    }

    pub fn sort_tabs(&mut self, sort: TabSort, descending: bool) {
        let buffers = &self.buffers;
        let mut keys = BTreeMap::new();
        for buffer in buffers {
            let path = buffer.disk_file.as_ref().map(|file| file.path());
            let text = match sort {
                TabSort::Name => buffer.name(),
                TabSort::Path => path
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|| buffer.name()),
                TabSort::Type => match buffer.language() {
                    crate::syntax::Language::NormalText => "normal".into(),
                    crate::syntax::Language::Json => "json".into(),
                    crate::syntax::Language::Builtin(id) => crate::languages::catalog()
                        .ok()
                        .and_then(|catalog| catalog.by_id(id))
                        .map(|profile| profile.name.clone())
                        .unwrap_or_default(),
                    crate::syntax::Language::UserDefined(_) => String::new(),
                },
                _ => String::new(),
            };
            let number = match sort {
                TabSort::Size => buffer.editor.document().len_bytes() as i128,
                TabSort::Modified => buffer
                    .backup_modified
                    .get()
                    .or_else(|| {
                        buffer
                            .disk_file
                            .as_ref()
                            .and_then(DiskFile::original_modified)
                    })
                    .map(|time| match time.duration_since(std::time::UNIX_EPOCH) {
                        Ok(duration) => duration.as_nanos() as i128,
                        Err(error) => -(error.duration().as_nanos() as i128),
                    })
                    .unwrap_or(i128::MIN),
                _ => 0,
            };
            let full_path = path
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| buffer.name());
            keys.insert(buffer.id, (text, number, full_path));
        }
        self.memberships[self.active_view.index()].sort_by(|left, right| {
            let (left_text, left_number, left_path) = &keys[left];
            let (right_text, right_number, right_path) = &keys[right];
            let order = compare_tab_text(left_text, right_text)
                .then_with(|| left_number.cmp(right_number))
                .then_with(|| compare_tab_text(left_path, right_path));
            if descending { order.reverse() } else { order }
        });
        self.partition_pinned();
    }

    pub fn close_targets(&self, condition: CloseCondition) -> Vec<(ViewId, BufferId)> {
        let views: &[ViewId] = if condition == CloseCondition::Unchanged {
            std::slice::from_ref(&self.active_view)
        } else {
            &[ViewId::Primary, ViewId::Secondary]
        };
        views
            .iter()
            .flat_map(|view| {
                self.view_buffer_ids(*view).iter().rev().filter_map(|id| {
                    let buffer = self.buffers.iter().find(|buffer| buffer.id == *id)?;
                    let include = match condition {
                        CloseCondition::ExceptCurrent => {
                            *view != self.active_view || *id != self.active().id
                        }
                        CloseCondition::ExceptPinned => !buffer.pinned,
                        CloseCondition::Unchanged => {
                            !buffer.is_modified()
                                || (buffer.disk_file.is_none()
                                    && buffer.editor.document().len_bytes() == 0)
                        }
                    };
                    include.then_some((*view, *id))
                })
            })
            .collect()
    }

    pub fn active_index(&self) -> usize {
        self.view_buffer_ids(self.active_view)
            .iter()
            .position(|id| *id == self.active().id)
            .unwrap_or(0)
    }

    pub fn close_left_targets(&self) -> Vec<BufferId> {
        self.view_buffer_ids(self.active_view)[..self.active_index()]
            .iter()
            .rev()
            .copied()
            .collect()
    }

    pub fn close_right_targets(&self) -> Vec<BufferId> {
        self.view_buffer_ids(self.active_view)[self.active_index() + 1..]
            .iter()
            .rev()
            .copied()
            .collect()
    }

    pub fn neighbor_id(&self, forward: bool) -> BufferId {
        let members = self.view_buffer_ids(self.active_view);
        let active = self.active_index();
        let index = if forward {
            (active + 1) % members.len()
        } else if active == 0 {
            members.len() - 1
        } else {
            active - 1
        };
        members[index]
    }

    pub fn active_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active]
    }

    pub fn mark_deleted_file(&mut self,id: BufferId) {
        if let Some(buffer) = self.buffers.iter_mut().find(|buffer| buffer.id() == id) {
            if let Some(file) = &mut buffer.disk_file { file.mark_deleted(); }
            buffer.editor.document().set_buffer_dirty(true);
            let user_read_only = buffer.user_read_only;
            buffer.set_user_read_only(user_read_only);
        }
    }

    pub fn complete_save(
        &mut self,
        intent: &BufferSaveIntent,
        file: DiskFile,
        completion: SaveCompletion,
    ) -> Result<(), WorkspaceError> {
        let buffer = self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.id == intent.id)
            .ok_or(WorkspaceError::UnknownBuffer(intent.id))?;
        buffer
            .editor
            .document()
            .validate_save_snapshot(&intent.snapshot)
            .map_err(WorkspaceError::Edit)?;
        if file.format() != intent.format || file.byte_code_page() != intent.byte_code_page {
            return Err(WorkspaceError::SavedFormatMismatch(intent.id));
        }
        if completion == SaveCompletion::Copy {
            if buffer
                .disk_file
                .as_ref()
                .is_some_and(|original| original.path() == file.path())
            {
                buffer.disk_file = Some(file);
            }
            return Ok(());
        }
        buffer
            .editor
            .acknowledge_save(&intent.snapshot)
            .map_err(WorkspaceError::Edit)?;
        buffer
            .editor
            .set_read_only(file.is_read_only() || buffer.user_read_only || buffer.monitoring);
        buffer.disk_file = Some(file);
        buffer.editor.document().set_buffer_dirty(
            buffer.editor.is_modified()
                || !Arc::ptr_eq(&buffer.format_identity, &intent.format_identity),
        );
        if !buffer.is_modified() {
            buffer.set_backup_modified(None);
        }
        Ok(())
    }

    pub fn buffers(&self) -> &[Buffer] {
        &self.buffers
    }
    pub fn set_all_user_read_only(&mut self, read_only: bool) {
        for buffer in &mut self.buffers {
            buffer.set_user_read_only(read_only);
        }
    }

    pub(crate) fn buffers_mut(&mut self) -> impl Iterator<Item = &mut Buffer> {
        self.buffers.iter_mut()
    }

    pub fn activate(&mut self, id: BufferId) -> Result<(), WorkspaceError> {
        let view = if self.view_buffer_ids(self.active_view).contains(&id) {
            self.active_view
        } else {
            self.active_view.other()
        };
        self.activate_in_view(view, id)
    }

    pub fn active_view(&self) -> ViewId {
        self.active_view
    }

    pub fn set_active_view(&mut self, view: ViewId) -> Result<(), WorkspaceError> {
        let id = self
            .active_id_in_view(view)
            .ok_or(WorkspaceError::EmptyView(view))?;
        self.activate_in_view_with_history(view, id, false)
    }

    pub fn focus_view(&mut self, view: ViewId) -> Result<(), WorkspaceError> {
        self.activate_view(view)
    }

    pub fn two_views_visible(&self) -> bool {
        self.memberships.iter().all(|members| !members.is_empty())
    }

    pub fn active_in_view(&self, view: ViewId) -> Option<&Buffer> {
        let id = self.active_id_in_view(view)?;
        self.buffers.iter().find(|buffer| buffer.id == id)
    }

    pub fn view_buffer_ids(&self, view: ViewId) -> &[BufferId] {
        &self.memberships[view.index()]
    }

    pub fn active_id_in_view(&self, view: ViewId) -> Option<BufferId> {
        self.view_active[view.index()]
    }

    pub fn activate_view(&mut self, view: ViewId) -> Result<(), WorkspaceError> {
        let id = self
            .active_id_in_view(view)
            .ok_or(WorkspaceError::EmptyView(view))?;
        self.activate_in_view(view, id)
    }

    pub fn activate_in_view(&mut self, view: ViewId, id: BufferId) -> Result<(), WorkspaceError> {
        self.activate_in_view_with_history(view, id, true)
    }

    fn activate_in_view_with_history(
        &mut self,
        view: ViewId,
        id: BufferId,
        record: bool,
    ) -> Result<(), WorkspaceError> {
        let index = self
            .buffers
            .iter()
            .position(|buffer| buffer.id == id)
            .ok_or(WorkspaceError::UnknownBuffer(id))?;
        if !self.view_buffer_ids(view).contains(&id) {
            return Err(WorkspaceError::NotInView(view, id));
        }
        for buffer in &mut self.buffers {
            if buffer.editor_view != view
                && let Some(other) = &mut buffer.other_editor
            {
                std::mem::swap(&mut buffer.editor, other);
                buffer.editor_view = view;
            }
        }
        self.active_view = view;
        self.view_active[view.index()] = Some(id);
        self.active = index;
        if record {
            self.record_activation(view, id);
        }
        Ok(())
    }

    pub fn editor_in_view(&self, view: ViewId, id: BufferId) -> Option<&EditorSession> {
        if !self.view_buffer_ids(view).contains(&id) {
            return None;
        }
        let buffer = self.buffers.iter().find(|buffer| buffer.id == id)?;
        if buffer.editor_view == view {
            Some(&buffer.editor)
        } else {
            buffer.other_editor.as_ref()
        }
    }

    pub fn editor_in_view_mut(&mut self, view: ViewId, id: BufferId) -> Option<&mut EditorSession> {
        if !self.view_buffer_ids(view).contains(&id) {
            return None;
        }
        let buffer = self.buffers.iter_mut().find(|buffer| buffer.id == id)?;
        if buffer.editor_view == view {
            Some(&mut buffer.editor)
        } else {
            buffer.other_editor.as_mut()
        }
    }

    pub fn clone_to_other_view(&mut self, id: BufferId) -> Result<(), WorkspaceError> {
        let source = self.active_view;
        if !self.view_buffer_ids(source).contains(&id) {
            return Err(WorkspaceError::NotInView(source, id));
        }
        let target = source.other();
        if !self.view_buffer_ids(target).contains(&id) {
            let buffer = self
                .buffers
                .iter_mut()
                .find(|buffer| buffer.id == id)
                .ok_or(WorkspaceError::UnknownBuffer(id))?;
            buffer.other_editor = Some(buffer.editor.clone_view());
            self.memberships[target.index()].push(id);
            if let Some(state) = self.view_states[source.index()].get(&id).cloned() {
                self.view_states[target.index()].insert(id, state);
            }
            self.partition_pinned();
        }
        self.activate_in_view(target, id)
    }

    pub fn move_to_other_view(&mut self, id: BufferId) -> Result<(), WorkspaceError> {
        let source = self.active_view;
        if !self.view_buffer_ids(source).contains(&id) {
            return Err(WorkspaceError::NotInView(source, id));
        }
        if self.view_buffer_ids(source).len() == 1
            && self.view_buffer_ids(source.other()).is_empty()
        {
            return Ok(());
        }
        self.clone_to_other_view(id)?;
        self.remove_membership(source, id);
        let buffer = self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.id == id)
            .ok_or(WorkspaceError::UnknownBuffer(id))?;
        buffer.other_editor = None;
        Ok(())
    }

    fn remove_membership(&mut self, view: ViewId, id: BufferId) {
        self.view_states[view.index()].remove(&id);
        self.tab_colors[view.index()].remove(&id);
        self.mru.retain(|entry| *entry != (view, id));
        self.mru_cycle = None;
        let members = &mut self.memberships[view.index()];
        if let Some(index) = members.iter().position(|member| *member == id) {
            members.remove(index);
            if self.view_active[view.index()] == Some(id) {
                self.view_active[view.index()] = members
                    .get(index.min(members.len().saturating_sub(1)))
                    .copied();
            }
        }
    }

    pub fn close_buffer(
        &mut self,
        id: BufferId,
        disposition: CloseDisposition,
    ) -> Result<BufferId, WorkspaceError> {
        let index = self
            .buffers
            .iter()
            .position(|buffer| buffer.id == id)
            .ok_or(WorkspaceError::UnknownBuffer(id))?;
        let view = if self.view_buffer_ids(self.active_view).contains(&id) {
            self.active_view
        } else {
            self.active_view.other()
        };
        let cloned = self.view_buffer_ids(view.other()).contains(&id);
        if cloned {
            self.remove_membership(view, id);
            let buffer = &mut self.buffers[index];
            if buffer.editor_view == view {
                if let Some(mut other) = buffer.other_editor.take() {
                    std::mem::swap(&mut buffer.editor, &mut other);
                    buffer.editor_view = view.other();
                }
            } else {
                buffer.other_editor = None;
            }
            let focus = if self.active_id_in_view(self.active_view).is_some() {
                self.active_view
            } else {
                self.active_view.other()
            };
            self.activate_view(focus)?;
            return Ok(self.active().id);
        }
        if self.buffers[index].is_modified() && disposition == CloseDisposition::IfUnmodified {
            return Err(WorkspaceError::UnsavedBuffer(id));
        }
        if self.buffers.len() == 1 {
            self.append_buffer(EditorSession::new(""), None, "new 1".into())?;
        }
        self.remove_membership(view, id);
        if let Some(file) = &self.buffers[index].disk_file
            && file.path().is_file()
        {
            self.recently_closed.retain(|path| path != file.path());
            self.recently_closed.insert(0, file.path().to_owned());
            self.recently_closed.truncate(self.recent_files_limit);
        }
        self.buffers.remove(index);
        let focus = if self.active_id_in_view(self.active_view).is_some() {
            self.active_view
        } else {
            self.active_view.other()
        };
        self.activate_view(focus)?;
        Ok(self.active().id)
    }

    pub fn close_in_view(
        &mut self,
        view: ViewId,
        id: BufferId,
        disposition: CloseDisposition,
    ) -> Result<BufferId, WorkspaceError> {
        if !self.view_buffer_ids(view).contains(&id) {
            return Err(WorkspaceError::NotInView(view, id));
        }
        self.activate_view(view)?;
        self.close_buffer(id, disposition)
    }

    pub fn new_buffer(&mut self) -> Result<BufferId, WorkspaceError> {
        let used = self
            .buffers
            .iter()
            .filter(|buffer| buffer.disk_file.is_none())
            .map(|buffer| buffer.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let bound = self
            .buffers
            .len()
            .checked_add(1)
            .ok_or(WorkspaceError::IdExhausted)?;
        let name = (1..=bound)
            .map(|number| format!("new {number}"))
            .find(|name| !used.contains(name.as_str()))
            .ok_or(WorkspaceError::IdExhausted)?;
        let mut editor = EditorSession::new("");
        let ending = match self.new_document_settings["format"].as_i64().unwrap_or(2) {
            0 => crate::input::LineEnding::CrLf,
            1 => crate::input::LineEnding::Cr,
            _ => crate::input::LineEnding::Lf,
        };
        editor.convert_line_endings(ending).map_err(WorkspaceError::Edit)?;
        let (encoding,byte_code_page) = self.new_document_format()?;
        let language = self.resolved_new_document_language()?;
        let udl_name = self.udl_language_name(language).map(str::to_owned);
        let id = self.append_buffer(editor, None, name)?;
        self.active_mut().format = encoding;
        self.active_mut().byte_code_page = byte_code_page;
        if language == crate::syntax::Language::NormalText { self.active_mut().set_detected_language(language); } else { self.active_mut().set_language(language); }
        self.active_mut().udl_name = udl_name;
        Ok(id)
    }

    pub fn reload_buffer(
        &mut self,
        id: BufferId,
        file: DiskFile,
        text: &str,
        disposition: CloseDisposition,
    ) -> Result<(), WorkspaceError> {
        let buffer = self
            .buffers
            .iter_mut()
            .find(|buffer| buffer.id == id)
            .ok_or(WorkspaceError::UnknownBuffer(id))?;
        if !buffer
            .disk_file
            .as_ref()
            .is_some_and(|original| original.path() == file.path())
        {
            return Err(WorkspaceError::ReloadTargetMismatch(id));
        }
        if buffer.is_modified() && disposition == CloseDisposition::IfUnmodified {
            return Err(WorkspaceError::UnsavedBuffer(id));
        }
        let old_selection = buffer.editor.document().selection();
        let clamp = |position: crate::editor::ByteOffset| {
            let mut offset = position.0.min(text.len());
            while !text.is_char_boundary(offset) {
                offset -= 1;
            }
            crate::editor::ByteOffset(offset)
        };
        let mut editor = EditorSession::from_file_text(text);
        editor
            .select(crate::editor::Selection {
                anchor: clamp(old_selection.anchor),
                head: clamp(old_selection.head),
            })
            .map_err(WorkspaceError::Edit)?;
        editor.set_read_only(file.is_read_only() || buffer.user_read_only || buffer.monitoring);
        editor.set_overwrite(buffer.editor.overwrite());
        if let Some(other) = &buffer.other_editor {
            let old = other.document().selection();
            let mut replacement = editor.clone_view();
            replacement
                .select(crate::editor::Selection {
                    anchor: clamp(old.anchor),
                    head: clamp(old.head),
                })
                .map_err(WorkspaceError::Edit)?;
            replacement.set_overwrite(other.overwrite());
            buffer.other_editor = Some(replacement);
        }
        buffer.editor = editor;
        buffer.initial_placeholder = false;
        buffer.format = file.format();
        buffer.byte_code_page = file.byte_code_page();
        buffer.format_identity = Arc::new(());
        buffer.set_backup_modified(None);
        buffer.disk_file = Some(file);
        Ok(())
    }

    pub fn open_buffer(&mut self, file: DiskFile, text: &str) -> Result<BufferId, WorkspaceError> {
        self.open_buffer_bytes(file, crate::byte_buffer::ByteBuffer::from_bytes(text.as_bytes()))
    }

    pub(crate) fn open_buffer_bytes(&mut self, file: DiskFile, bytes: crate::byte_buffer::ByteBuffer) -> Result<BufferId, WorkspaceError> {
        self.recently_closed.retain(|path| path != file.path());
        if let Some(index) = self.buffers.iter().position(|buffer| {
            buffer
                .disk_file
                .as_ref()
                .is_some_and(|opened| opened.path() == file.path())
        }) {
            let id = self.buffers[index].id;
            if self.buffers[index].disk_file.as_ref().is_some_and(DiskFile::is_placeholder) {
                let large = self.large_file_restriction.classifies(file.loaded_len() as u64);
                self.buffers[index].reload_selected_encoding(file, bytes)?;
                self.buffers[index].large_file = large;
                self.opened_large_file |= large;
            }
            self.activate(id)?;
            return Ok(id);
        }
        let mut editor = EditorSession::from_file_bytes(bytes);
        editor.set_read_only(file.is_read_only());
        self.append_buffer(editor, Some(file), String::new())
    }

    pub(crate) fn append_recovered_buffer_with_size(
        &mut self,
        editor: EditorSession,
        disk_file: Option<DiskFile>,
        name: String,
        format: UnicodeFormat,
        loaded_bytes: u64,
    ) -> Result<BufferId, WorkspaceError> {
        let id = self.append_buffer(editor, disk_file, name)?;
        self.initial_document_configured = true;
        self.active_mut().format = format;
        self.active_mut().large_file = self.large_file_restriction.classifies(loaded_bytes);
        self.opened_large_file |= self.active().is_large_file();
        Ok(id)
    }

    pub(crate) fn check_recovery_capacity(&self, count: usize) -> Result<(), WorkspaceError> {
        let count = u64::try_from(count).map_err(|_| WorkspaceError::IdExhausted)?;
        self.next_id
            .checked_add(count)
            .ok_or(WorkspaceError::IdExhausted)?;
        Ok(())
    }

    fn append_buffer(
        &mut self,
        editor: EditorSession,
        disk_file: Option<DiskFile>,
        name: String,
    ) -> Result<BufferId, WorkspaceError> {
        let id = BufferId(self.next_id);
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(WorkspaceError::IdExhausted)?;
        self.buffers.push(Buffer {
            udl_name: None,
            untitled_tab_renamed: false,
            initial_placeholder: false,
            large_file: disk_file.as_ref().is_some_and(|file| {
                self.large_file_restriction
                    .classifies(file.loaded_len() as u64)
            }),
            language_explicit: false,
            editor_view: self.active_view,
            other_editor: None,
            pinned: false,
            monitoring: false,
            user_read_only: false,
            backup_modified: std::cell::Cell::new(None),
            id,
            editor,
            language: disk_file
                .as_ref()
                .map(|file| crate::syntax::Language::from_path(file.path()))
                .unwrap_or_default(),
            format: disk_file
                .as_ref()
                .map(DiskFile::format)
                .unwrap_or(UnicodeFormat::Utf8(Utf8Format::Plain)),
            byte_code_page: disk_file.as_ref().and_then(DiskFile::byte_code_page),
            format_identity: Arc::new(()),
            disk_file,
            name,
        });
        self.next_id = next;
        self.active = self.buffers.len() - 1;
        self.opened_large_file |= self.active().is_large_file();
        self.memberships[self.active_view.index()].push(id);
        self.view_active[self.active_view.index()] = Some(id);
        self.record_activation(self.active_view, id);
        Ok(id)
    }

    pub fn dirty_buffers(&self) -> Vec<BufferId> {
        self.buffers
            .iter()
            .filter(|buffer| buffer.is_modified())
            .map(|buffer| buffer.id)
            .collect()
    }
}
