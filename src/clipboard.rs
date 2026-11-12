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
            Self::Copy => "Copy Binary Content",
            Self::Cut => "Cut Binary Content",
            Self::Paste => "Paste Binary Content",
        }
    }
}

pub fn binary_selection(
    editor: &mut crate::input::EditorSession,
) -> Result<Option<Vec<u8>>, crate::editor::EditError> {
    let payload = editor.clipboard_payload();
    if payload.line_copy || payload.text.is_empty() {
        return Ok(None);
    }
    let text_len = payload.text.len();
    if editor.selection_limits().range().is_empty() {
        editor.expand_word_selection()?;
    }
    let range = editor.selection_limits().range();
    let mut bytes = editor
        .display_snapshot()
        .slice(range.start.0..range.end.0)
        .chunks()
        .flat_map(|chunk| chunk.bytes())
        .take(text_len)
        .collect::<Vec<_>>();
    bytes.resize(text_len, 0);
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
            Self::Full => "Copy Current Full File path",
            Self::Name => "Copy Current Filename",
            Self::Directory => "Copy Current Dir. Path",
            Self::AllNames => "Copy All Filenames",
            Self::AllPaths => "Copy All File Paths",
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
