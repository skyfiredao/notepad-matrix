#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardText {
    pub text: String,
    pub line_copy: bool,
}

pub const BINARY_METADATA: &str = "notepad-matrix/binary-text/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryCommand {
    Copy,
    Cut,
    Paste,
}

impl BinaryCommand {
    pub fn source_id(self) -> u32 {
        match self {
            Self::Copy => 42048,
            Self::Cut => 42049,
            Self::Paste => 42050,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Copy => "复制二进制",
            Self::Cut => "剪切二进制",
            Self::Paste => "粘贴二进制",
        }
    }
}

pub fn binary_selection(
    editor: &mut crate::input::EditorSession,
) -> Result<Option<Vec<u8>>, crate::editor::EditError> {
    let selections = editor.selections();
    if selections.ranges().iter().all(|range| range.is_empty()) { return Ok(None); }
    let separator = if selections.ranges().len() > 1 { editor.line_ending().as_str().len() } else { 0 };
    let length = selections.ranges().iter().try_fold(0usize, |length, selected| {
        length.checked_add(selected.selection.range().end.0 - selected.selection.range().start.0)
            .and_then(|length| length.checked_add(separator))
            .ok_or_else(|| crate::editor::EditError::Storage("binary clipboard length overflow".into()))
    })?;
    if length == 0 { return Ok(None); }
    let mut range = selections.primary().selection.range();
    if selections.is_rectangular() {
        for selected in selections.ranges() {
            let selected = selected.selection.range();
            range.start = range.start.min(selected.start);
            range.end = range.end.max(selected.end);
        }
    }
    let mut bytes = editor.document().byte_snapshot().slice(range.start.0..range.end.0)
        .map_err(|error| crate::editor::EditError::Storage(error.to_string()))?.to_vec();
    bytes.truncate(length);
    bytes.resize(length, 0);
    Ok(Some(bytes))
}

pub fn binary_paste_text(text: &str, recorded_length: bool) -> &str {
    if recorded_length {
        text
    } else {
        text.split_once('\0').map_or(text, |(before, _)| before)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyPath {
    Full,
    Name,
    Directory,
    AllNames,
    AllPaths,
}

impl CopyPath {
    pub const ALL: [Self; 5] = [
        Self::Full,
        Self::Name,
        Self::Directory,
        Self::AllNames,
        Self::AllPaths,
    ];

    pub fn source_id(self) -> u32 {
        match self {
            Self::Full => 42029,
            Self::Name => 42030,
            Self::Directory => 42031,
            Self::AllNames => 42087,
            Self::AllPaths => 42088,
        }
    }

    pub fn from_source_id(id: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|command| command.source_id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "复制当前文件路径",
            Self::Name => "复制当前文件名",
            Self::Directory => "复制当前目录路径",
            Self::AllNames => "复制所有文件名",
            Self::AllPaths => "复制所有文件路径",
        }
    }

    pub fn text(self, workspace: &crate::workspace::Workspace) -> String {
        let value = |buffer: &crate::workspace::Buffer| match self {
            Self::Name | Self::AllNames => buffer.name().to_owned(),
            Self::Directory => buffer
                .disk_file
                .as_ref()
                .and_then(|file| file.path().parent())
                .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
            _ => buffer.disk_file.as_ref().map_or_else(
                || buffer.name().to_owned(),
                |file| file.path().to_string_lossy().into_owned(),
            ),
        };
        if !matches!(self, Self::AllNames | Self::AllPaths) {
            return value(workspace.active());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut text = String::new();
        for view in [
            crate::workspace::ViewId::Primary,
            crate::workspace::ViewId::Secondary,
        ] {
            for id in workspace.view_buffer_ids(view) {
                if seen.insert(*id)
                    && let Some(buffer) =
                        workspace.buffers().iter().find(|buffer| buffer.id() == *id)
                {
                    text.push_str(&value(buffer));
                    if !text.is_empty() && !text.ends_with("\r\n") {
                        text.push_str("\r\n");
                    }
                }
            }
        }
        text
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardKind {
    #[default]
    Stream,
    Line,
    Rectangle,
}

impl ClipboardKind {
    pub fn from_metadata(metadata: Option<&str>) -> Self {
        match metadata {
            Some("notepad-matrix/line-copy/1") => Self::Line,
            Some("notepad-matrix/rectangle-copy/1") => Self::Rectangle,
            _ => Self::Stream,
        }
    }

    pub fn metadata(self) -> Option<&'static str> {
        match self {
            Self::Stream => None,
            Self::Line => Some("notepad-matrix/line-copy/1"),
            Self::Rectangle => Some("notepad-matrix/rectangle-copy/1"),
        }
    }
}

pub(crate) fn convert_line_endings(text: &str, eol: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                result.push_str(eol);
            }
            '\n' => result.push_str(eol),
            _ => result.push(ch),
        }
    }
    result
}
