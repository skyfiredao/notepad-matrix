#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCommand {
    New,
    Open,
    Save,
    SaveAs,
    SaveCopy,
    SaveAll,
    Close,
    CloseAll,
    Reload,
    CloseLeft,
    CloseRight,
    Exit,
}

impl FileCommand {
    pub const ALL: [Self; 12] = [
        Self::New,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::SaveCopy,
        Self::SaveAll,
        Self::Close,
        Self::CloseAll,
        Self::Reload,
        Self::CloseLeft,
        Self::CloseRight,
        Self::Exit,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Open => "Open...",
            Self::Save => "Save",
            Self::SaveAs => "Save As...",
            Self::SaveCopy => "Save a Copy As...",
            Self::SaveAll => "Save All",
            Self::Close => "Close",
            Self::CloseAll => "Close All",
            Self::Reload => "Reload from Disk",
            Self::CloseLeft => "Close All to the Left",
            Self::CloseRight => "Close All to the Right",
            Self::Exit => "Exit",
        }
    }
    pub fn shortcut(self) -> &'static str {
        match self {
            Self::New => "Ctrl+N",
            Self::Open => "Ctrl+O",
            Self::Save => "Ctrl+S",
            Self::SaveAs => "Ctrl+Alt+S",
            Self::SaveCopy => "",
            Self::SaveAll => "Ctrl+Shift+S",
            Self::Close => "Ctrl+W",
            Self::CloseAll => "Ctrl+Shift+W",
            Self::Reload => "Ctrl+R",
            Self::CloseLeft => "",
            Self::CloseRight => "",
            Self::Exit => "Alt+F4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    DuplicateLine,
    ConvertEol(crate::input::LineEnding),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformationAvailability {
    pub editable: bool,
    pub line_comment: bool,
    pub block_comment: bool,
}

impl TransformationAvailability {
    pub fn allows(self, id: u32) -> bool {
        use crate::text_transforms::{Comment, TransformCommand};
        self.editable
            && match TransformCommand::from_source_id(id) {
                Some(TransformCommand::Comment(
                    Comment::ToggleLine | Comment::Line | Comment::UncommentLine,
                )) => self.line_comment,
                Some(TransformCommand::Comment(Comment::Stream | Comment::UncommentStream)) => {
                    self.block_comment
                }
                Some(_) => true,
                None => false,
            }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiSelectionCommand {
    Next(crate::editor::OccurrenceOptions),
    All(crate::editor::OccurrenceOptions),
    Undo,
    Skip,
    Rotate,
    Swap,
}

impl MultiSelectionCommand {
    pub fn from_source_id(id: u32) -> Option<Self> {
        use crate::editor::OccurrenceOptions;
        match id {
            42090..=42097 => {
                let bits = (id - 42090) % 4;
                let options = OccurrenceOptions {
                    match_case: bits & 1 != 0,
                    whole_word: bits & 2 != 0,
                };
                Some(if id < 42094 {
                    Self::All(options)
                } else {
                    Self::Next(options)
                })
            }
            42098 => Some(Self::Undo),
            42099 => Some(Self::Skip),
            2606 => Some(Self::Rotate),
            2607 => Some(Self::Swap),
            _ => None,
        }
    }

    pub fn source_id(self) -> u32 {
        match self {
            Self::Next(options) => {
                42094 + u32::from(options.match_case) + 2 * u32::from(options.whole_word)
            }
            Self::All(options) => {
                42090 + u32::from(options.match_case) + 2 * u32::from(options.whole_word)
            }
            Self::Undo => 42098,
            Self::Skip => 42099,
            Self::Rotate => 2606,
            Self::Swap => 2607,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Next(options) | Self::All(options) => {
                match (options.match_case, options.whole_word) {
                    (false, false) => "Ignore Case & Whole Word",
                    (true, false) => "Match Case Only",
                    (false, true) => "Match Whole Word Only",
                    (true, true) => "Match Case & Whole Word",
                }
            }
            Self::Undo => "Undo the Latest Added Multi-Select",
            Self::Skip => "Skip Current & Go to Next Multi-select",
            Self::Rotate => "Rotate Selection",
            Self::Swap => "Swap Main Anchor/Caret",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentCommand {
    Close(crate::workspace::CloseCondition),
    Pin,
    Color(Option<u8>),
    SortTabs(crate::workspace::TabSort, bool),
    Multiline,
    Vertical,
    Mru,
    Switcher,
    RestoreLastClosed,
    SaveSession,
    LoadSession,
}

impl DocumentCommand {
    pub fn source_id(self) -> Option<u32> {
        match self {
            Self::Close(crate::workspace::CloseCondition::ExceptCurrent) => Some(41005),
            Self::Close(crate::workspace::CloseCondition::ExceptPinned) => Some(41026),
            Self::Close(crate::workspace::CloseCondition::Unchanged) => Some(41024),
            Self::Pin => Some(44048),
            Self::Color(None) => Some(44110),
            Self::Color(Some(color @ 0..=4)) => Some(44111 + u32::from(color)),
            Self::Color(Some(_)) => None,
            Self::SortTabs(sort, descending) => Some(
                11002
                    + 2 * match sort {
                        crate::workspace::TabSort::Name => 0,
                        crate::workspace::TabSort::Path => 1,
                        crate::workspace::TabSort::Type => 2,
                        crate::workspace::TabSort::Size => 3,
                        crate::workspace::TabSort::Modified => 4,
                    }
                    + u32::from(descending),
            ),
            Self::Multiline => Some(6119),
            Self::Vertical => Some(6120),
            Self::Mru => Some(6117),
            Self::Switcher => Some(6114),
            Self::RestoreLastClosed => Some(41021),
            Self::SaveSession => Some(41013),
            Self::LoadSession => Some(41012),
        }
    }

    pub fn label(self) -> &'static str {
        use crate::workspace::TabSort;
        match self {
            Self::Close(crate::workspace::CloseCondition::ExceptCurrent) => {
                "Close All but Active Document"
            }
            Self::Close(crate::workspace::CloseCondition::ExceptPinned) => {
                "Close All but Pinned Documents"
            }
            Self::Close(crate::workspace::CloseCondition::Unchanged) => "Close All Unchanged",
            Self::Pin => "Pin Tab",
            Self::Color(None) => "Remove Color",
            Self::Color(Some(0)) => "Apply Color 1",
            Self::Color(Some(1)) => "Apply Color 2",
            Self::Color(Some(2)) => "Apply Color 3",
            Self::Color(Some(3)) => "Apply Color 4",
            Self::Color(Some(4)) => "Apply Color 5",
            Self::Color(Some(_)) => "Unknown Tab Color",
            Self::SortTabs(TabSort::Name, false) => "Name A to Z",
            Self::SortTabs(TabSort::Name, true) => "Name Z to A",
            Self::SortTabs(TabSort::Path, false) => "Path A to Z",
            Self::SortTabs(TabSort::Path, true) => "Path Z to A",
            Self::SortTabs(TabSort::Type, false) => "Type A to Z",
            Self::SortTabs(TabSort::Type, true) => "Type Z to A",
            Self::SortTabs(TabSort::Size, false) => "Content Length Ascending",
            Self::SortTabs(TabSort::Size, true) => "Content Length Descending",
            Self::SortTabs(TabSort::Modified, false) => "Modified Time Ascending",
            Self::SortTabs(TabSort::Modified, true) => "Modified Time Descending",
            Self::Multiline => "Multi-line",
            Self::Vertical => "Vertical",
            Self::Mru => "Enable MRU behaviour",
            Self::Switcher => "Enable",
            Self::RestoreLastClosed => "Restore Recent Closed File",
            Self::SaveSession => "Save Session...",
            Self::LoadSession => "Load Session...",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntelligenceCommand {
    Function,
    Word,
    Parameters,
    PreviousParameters,
    NextParameters,
    Path,
}

impl IntelligenceCommand {
    pub const ALL: [Self; 6] = [
        Self::Function,
        Self::Word,
        Self::Parameters,
        Self::PreviousParameters,
        Self::NextParameters,
        Self::Path,
    ];

    pub fn source_id(self) -> u32 {
        match self {
            Self::Function => 50000,
            Self::Word => 50001,
            Self::Parameters => 50002,
            Self::PreviousParameters => 50010,
            Self::NextParameters => 50011,
            Self::Path => 50006,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Function => "Function Completion",
            Self::Word => "Word Completion",
            Self::Parameters => "Function Parameters Hint",
            Self::PreviousParameters => "Function Parameters Previous Hint",
            Self::NextParameters => "Function Parameters Next Hint",
            Self::Path => "Path Completion",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Function => "Ctrl+Space",
            Self::Word => "Ctrl+Enter",
            Self::Parameters => "Ctrl+Shift+Space",
            Self::PreviousParameters => "Alt+Up",
            Self::NextParameters => "Alt+Down",
            Self::Path => "Ctrl+Alt+Space",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    Binary(crate::clipboard::BinaryCommand),
    CharacterPanel,
    CopyPath(crate::clipboard::CopyPath),
    Intelligence(IntelligenceCommand),
    TextTransform(u32),
    MultiSelection(MultiSelectionCommand),
    ColumnEditor,
    Document(DocumentCommand),
    File(FileCommand),
    Edit(EditCommand),
    Search(SearchCommand),
    Bookmark(BookmarkCommand),
    ChangeHistory(ChangeHistoryCommand),
    View(ViewCommand),
    Settings(SettingsCommand),
    Mime(crate::mime::MimeOperation),
    Converter(crate::converter::ConverterOperation),
    ExportHtml,
    Window(WindowCommand),
    About,
    Run,
    HashSelection(crate::hashes::HashAlgorithm),
    Macro(crate::macros::MacroCommand),
    SavedMacro(usize),
    Language(crate::syntax::Language),
    DefineUserLanguage,
    SelectEncoding(crate::files::UnicodeFormat),
    ConvertEncoding(crate::files::UnicodeFormat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchCommand {
    Find,
    FindInFiles,
    Next,
    Previous,
    GoTo,
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsCommand {
    Preferences,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookmarkCommand {
    Toggle,
    Next,
    Previous,
    ClearAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeHistoryCommand {
    Next,
    Previous,
    Clear,
}

impl ChangeHistoryCommand {
    pub fn source_id(self) -> u32 {
        match self {
            Self::Next => 43067,
            Self::Previous => 43068,
            Self::Clear => 43069,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Next => "Go to Next Change",
            Self::Previous => "Go to Previous Change",
            Self::Clear => "Clear Change History",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewCommand {
    ProjectPanel(usize),
    AllCharacters,
    IndentGuides,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    WordWrap,
    Summary,
    MarkdownPreview,
    DocumentMap,
    DocumentList,
    FolderWorkspace,
    FunctionList,
    Monitoring,
    SyncVertical,
    SyncHorizontal,
    CloneToOtherView,
    MoveToOtherView,
    MoveToNewInstance,
    OpenInNewInstance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    Zoom,
    BringAllToFront,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentControl {
    New,
    DocumentList,
    Close,
}

impl DocumentControl {
    pub const ALL: [Self; 3] = [Self::New, Self::DocumentList, Self::Close];

    pub fn command(self) -> MenuCommand {
        match self {
            Self::New => MenuCommand::File(FileCommand::New),
            Self::DocumentList => MenuCommand::View(ViewCommand::DocumentList),
            Self::Close => MenuCommand::File(FileCommand::Close),
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::New => "＋",
            Self::DocumentList => "▼",
            Self::Close => "✕",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::New => "New document",
            Self::DocumentList => "Toggle Document List",
            Self::Close => "Close document",
        }
    }
}

impl MenuCommand {
    pub fn source_id(self) -> Option<u32> {
        match self {
            Self::ChangeHistory(command) => Some(command.source_id()),
            Self::View(ViewCommand::MoveToNewInstance) => Some(10003),
            Self::View(ViewCommand::OpenInNewInstance) => Some(10004),
            Self::Binary(command) => Some(command.source_id()),
            Self::CharacterPanel => Some(42051),
            Self::CopyPath(command) => Some(command.source_id()),
            Self::Intelligence(command) => Some(command.source_id()),
            Self::TextTransform(id) => {
                crate::text_transforms::TransformCommand::from_source_id(id).map(|_| id)
            }
            Self::MultiSelection(command) => Some(command.source_id()),
            Self::ColumnEditor => Some(42034),
            Self::Document(command) => command.source_id(),
            Self::View(ViewCommand::ProjectPanel(index @ 0..=2)) => Some(44081 + index as u32),
            Self::Language(language) => {
                let id = match language {
                    crate::syntax::Language::NormalText => 0,
                    crate::syntax::Language::Json => 57,
                    crate::syntax::Language::Builtin(id) => id,
                    crate::syntax::Language::UserDefined(0) => return Some(46180),
                    crate::syntax::Language::UserDefined(_) => return None,
                };
                BUILTIN_LANGUAGE_MENU
                    .iter()
                    .find(|(candidate, _, _)| *candidate == id)
                    .map(|(_, source, _)| *source)
            }
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        use crate::input::LineEnding;
        match self {
            Self::Binary(command) => command.label(),
            Self::CharacterPanel => "Character Panel",
            Self::CopyPath(command) => command.label(),
            Self::Intelligence(command) => command.label(),
            Self::TextTransform(id) => transform_label(id),
            Self::MultiSelection(command) => command.label(),
            Self::ColumnEditor => "Column Editor...",
            Self::Document(command) => command.label(),
            Self::View(ViewCommand::ProjectPanel(0)) => "Project Panel 1",
            Self::View(ViewCommand::ProjectPanel(1)) => "Project Panel 2",
            Self::View(ViewCommand::ProjectPanel(2)) => "Project Panel 3",
            Self::View(ViewCommand::ProjectPanel(_)) => "Unknown Project Panel",
            Self::ConvertEncoding(crate::files::UnicodeFormat::Utf8(
                crate::files::Utf8Format::Plain,
            )) => "Convert to UTF-8",
            Self::ConvertEncoding(crate::files::UnicodeFormat::Utf8(
                crate::files::Utf8Format::Bom,
            )) => "Convert to UTF-8-BOM",
            Self::ConvertEncoding(crate::files::UnicodeFormat::Utf16BeBom) => {
                "Convert to UTF-16 BE BOM"
            }
            Self::ConvertEncoding(crate::files::UnicodeFormat::Utf16LeBom) => {
                "Convert to UTF-16 LE BOM"
            }
            Self::SelectEncoding(crate::files::UnicodeFormat::Utf8(
                crate::files::Utf8Format::Plain,
            )) => "UTF-8",
            Self::SelectEncoding(crate::files::UnicodeFormat::Utf8(
                crate::files::Utf8Format::Bom,
            )) => "UTF-8-BOM",
            Self::SelectEncoding(crate::files::UnicodeFormat::Utf16BeBom) => "UTF-16 BE BOM",
            Self::SelectEncoding(crate::files::UnicodeFormat::Utf16LeBom) => "UTF-16 LE BOM",
            Self::File(command) => command.label(),
            Self::ExportHtml => "Export to HTML",
            Self::Window(WindowCommand::Minimize) => "Minimize",
            Self::Window(WindowCommand::Zoom) => "Zoom",
            Self::Window(WindowCommand::BringAllToFront) => "Bring All to Front",
            Self::About => "About Notepad Matrix",
            Self::Run => "Run...",
            Self::HashSelection(_) => "Generate from selection into clipboard",
            Self::Language(language) => language.label(),
            Self::Macro(crate::macros::MacroCommand::StartRecording) => "Start Recording",
            Self::Macro(crate::macros::MacroCommand::StopRecording) => "Stop Recording",
            Self::Macro(crate::macros::MacroCommand::Playback) => "Playback",
            Self::Macro(crate::macros::MacroCommand::SaveCurrent) => {
                "Save Current Recorded Macro..."
            }
            Self::Macro(crate::macros::MacroCommand::RunMultiple) => {
                "Run a Macro Multiple Times..."
            }
            Self::SavedMacro(_) => "Saved Macro",
            Self::Converter(crate::converter::ConverterOperation::AsciiToHex) => "ASCII -> HEX",
            Self::Converter(crate::converter::ConverterOperation::HexToAscii) => "HEX -> ASCII",
            Self::View(ViewCommand::DocumentList) => "Document List",
            Self::View(ViewCommand::AllCharacters) => "Show All Characters",
            Self::View(ViewCommand::IndentGuides) => "Show Indent Guide",
            Self::View(ViewCommand::ZoomIn) => "Zoom In (Ctrl+Mouse Wheel Up)",
            Self::View(ViewCommand::ZoomOut) => "Zoom Out (Ctrl+Mouse Wheel Down)",
            Self::View(ViewCommand::ZoomReset) => "Restore Default Zoom",
            Self::View(ViewCommand::WordWrap) => "Word wrap",
            Self::View(ViewCommand::Summary) => "Summary...",
            Self::View(ViewCommand::MarkdownPreview) => "Markdown Preview",
            Self::View(ViewCommand::DocumentMap) => "Document Map",
            Self::View(ViewCommand::FolderWorkspace) => "Folder as Workspace",
            Self::View(ViewCommand::FunctionList) => "Function List",
            Self::View(ViewCommand::Monitoring) => "Monitoring (tail -f)",
            Self::View(ViewCommand::SyncVertical) => "Synchronize Vertical Scrolling",
            Self::View(ViewCommand::SyncHorizontal) => "Synchronize Horizontal Scrolling",
            Self::View(ViewCommand::CloneToOtherView) => "Clone to Other View",
            Self::View(ViewCommand::MoveToOtherView) => "Move to Other View",
            Self::View(ViewCommand::MoveToNewInstance) => "Move to New Instance",
            Self::View(ViewCommand::OpenInNewInstance) => "Open in New Instance",
            Self::DefineUserLanguage => "Define your language...",
            Self::Bookmark(BookmarkCommand::Toggle) => "Toggle Bookmark",
            Self::ChangeHistory(command) => command.label(),
            Self::Bookmark(BookmarkCommand::Next) => "Next Bookmark",
            Self::Bookmark(BookmarkCommand::Previous) => "Previous Bookmark",
            Self::Bookmark(BookmarkCommand::ClearAll) => "Clear All Bookmarks",
            Self::Mime(crate::mime::MimeOperation::Base64Encode) => "Base64 Encode",
            Self::Mime(crate::mime::MimeOperation::Base64EncodePadded) => {
                "Base64 Encode with padding"
            }
            Self::Mime(crate::mime::MimeOperation::Base64Decode) => "Base64 Decode",
            Self::Settings(SettingsCommand::Preferences) => "Preferences...",
            Self::Search(SearchCommand::Find) => "Find...",
            Self::Search(SearchCommand::FindInFiles) => "Find in Files...",
            Self::Search(SearchCommand::Next) => "Find Next",
            Self::Search(SearchCommand::Previous) => "Find Previous",
            Self::Search(SearchCommand::GoTo) => "Go to...",
            Self::Search(SearchCommand::Replace) => "Replace...",
            Self::Edit(EditCommand::Undo) => "Undo",
            Self::Edit(EditCommand::Redo) => "Redo",
            Self::Edit(EditCommand::Cut) => "Cut",
            Self::Edit(EditCommand::Copy) => "Copy",
            Self::Edit(EditCommand::Paste) => "Paste",
            Self::Edit(EditCommand::Delete) => "Delete",
            Self::Edit(EditCommand::SelectAll) => "Select All",
            Self::Edit(EditCommand::DuplicateLine) => "Duplicate Current Line",
            Self::Edit(EditCommand::ConvertEol(LineEnding::CrLf)) => "Windows (CR LF)",
            Self::Edit(EditCommand::ConvertEol(LineEnding::Lf)) => "Unix (LF)",
            Self::Edit(EditCommand::ConvertEol(LineEnding::Cr)) => "Macintosh (CR)",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Binary(_) => "",
            Self::CharacterPanel => "",
            Self::CopyPath(_) => "",
            Self::Intelligence(command) => command.shortcut(),
            Self::TextTransform(42010) => "Ctrl+D",
            Self::TextTransform(_) => "",
            Self::MultiSelection(_) => "",
            Self::ColumnEditor => "Alt+C",
            Self::Document(DocumentCommand::RestoreLastClosed) => "Ctrl+Shift+T",
            Self::Document(_) => "",
            Self::File(command) => command.shortcut(),
            Self::View(ViewCommand::ZoomIn) => "Ctrl+Num +",
            Self::View(ViewCommand::ZoomOut) => "Ctrl+Num -",
            Self::View(ViewCommand::ZoomReset) => "Ctrl+Num /",
            Self::View(_) => "",
            Self::Bookmark(BookmarkCommand::Toggle) => "Ctrl+F2",
            Self::Bookmark(BookmarkCommand::Next) => "F2",
            Self::Bookmark(BookmarkCommand::Previous) => "Shift+F2",
            Self::Bookmark(BookmarkCommand::ClearAll) => "",
            Self::ChangeHistory(_) => "",
            Self::Settings(_) => "",
            Self::Mime(_) => "",
            Self::Converter(_) => "",
            Self::ExportHtml => "",
            Self::Window(WindowCommand::Minimize) => "Cmd+M",
            Self::Window(_) => "",
            Self::About => "",
            Self::Run => "F5",
            Self::HashSelection(_) => "",
            Self::Language(_) => "",
            Self::DefineUserLanguage => "",
            Self::Macro(crate::macros::MacroCommand::Playback) => "Ctrl+Shift+P",
            Self::Macro(_) => "",
            Self::SavedMacro(_) => "",
            Self::Search(SearchCommand::Find) => "Ctrl+F",
            Self::Search(SearchCommand::FindInFiles) => "Ctrl+Shift+F",
            Self::Search(SearchCommand::Next) => "F3",
            Self::Search(SearchCommand::Previous) => "Shift+F3",
            Self::Search(SearchCommand::GoTo) => "Ctrl+G",
            Self::Search(SearchCommand::Replace) => "Ctrl+H",
            Self::Edit(EditCommand::Undo) => "Ctrl+Z",
            Self::Edit(EditCommand::Redo) => "Ctrl+Y",
            Self::Edit(EditCommand::Cut) => "Ctrl+X",
            Self::Edit(EditCommand::Copy) => "Ctrl+C",
            Self::Edit(EditCommand::Paste) => "Ctrl+V",
            Self::Edit(EditCommand::Delete) => "Del",
            Self::Edit(EditCommand::SelectAll) => "Ctrl+A",
            Self::Edit(EditCommand::DuplicateLine) => "",
            Self::Edit(EditCommand::ConvertEol(_)) => "",
            Self::SelectEncoding(_) => "",
            Self::ConvertEncoding(_) => "",
        }
    }

    fn mnemonic(self) -> Option<char> {
        match self {
            Self::Binary(_) => None,
            Self::CharacterPanel => Some('p'),
            Self::CopyPath(_) => None,
            Self::Intelligence(_) => None,
            Self::TextTransform(_) => None,
            Self::MultiSelection(_) => None,
            Self::ColumnEditor => Some('n'),
            Self::Document(_) | Self::View(ViewCommand::ProjectPanel(_)) => None,
            Self::View(ViewCommand::DocumentList) => Some('o'),
            Self::View(ViewCommand::ZoomIn) => Some('i'),
            Self::View(ViewCommand::ZoomOut) => Some('o'),
            Self::View(ViewCommand::ZoomReset) => Some('r'),
            Self::View(ViewCommand::WordWrap) => Some('w'),
            Self::View(ViewCommand::Summary) => Some('s'),
            Self::View(ViewCommand::MarkdownPreview) => None,
            Self::View(ViewCommand::DocumentMap) => Some('d'),
            Self::View(ViewCommand::FolderWorkspace) => Some('f'),
            Self::View(ViewCommand::FunctionList) => None,
            Self::View(ViewCommand::Monitoring) => Some('m'),
            Self::View(ViewCommand::MoveToNewInstance) => Some('v'),
            Self::View(ViewCommand::OpenInNewInstance) => Some('o'),
            Self::View(
                ViewCommand::SyncVertical
                | ViewCommand::SyncHorizontal
                | ViewCommand::CloneToOtherView
                | ViewCommand::MoveToOtherView,
            ) => None,
            Self::DefineUserLanguage => None,
            Self::View(ViewCommand::AllCharacters | ViewCommand::IndentGuides) => None,
            Self::Bookmark(_) => None,
            Self::ChangeHistory(_) => None,
            Self::Settings(SettingsCommand::Preferences) => Some('p'),
            Self::Mime(_) => None,
            Self::Converter(_) => None,
            Self::ExportHtml => Some('e'),
            Self::Window(WindowCommand::Minimize) => Some('m'),
            Self::Window(WindowCommand::Zoom) => Some('z'),
            Self::Window(WindowCommand::BringAllToFront) => Some('b'),
            Self::About => Some('a'),
            Self::Run => Some('r'),
            Self::HashSelection(_) => None,
            Self::Language(_) => None,
            Self::Macro(crate::macros::MacroCommand::StartRecording) => Some('c'),
            Self::Macro(crate::macros::MacroCommand::StopRecording) => Some('t'),
            Self::Macro(crate::macros::MacroCommand::Playback) => Some('p'),
            Self::Macro(crate::macros::MacroCommand::SaveCurrent) => Some('s'),
            Self::Macro(crate::macros::MacroCommand::RunMultiple) => Some('r'),
            Self::SavedMacro(_) => None,
            Self::Search(SearchCommand::Find) => Some('f'),
            Self::Search(SearchCommand::FindInFiles) => Some('i'),
            Self::Search(SearchCommand::Next) => Some('n'),
            Self::Search(SearchCommand::Previous) => Some('p'),
            Self::Search(SearchCommand::GoTo) => Some('g'),
            Self::Search(SearchCommand::Replace) => Some('r'),
            Self::File(command) => Some(match command {
                FileCommand::New => 'n',
                FileCommand::Open => 'o',
                FileCommand::Save => 's',
                FileCommand::SaveAs => 'a',
                FileCommand::SaveCopy => 'y',
                FileCommand::SaveAll => 'v',
                FileCommand::Close => 'c',
                FileCommand::CloseAll => 'e',
                FileCommand::Reload => 'l',
                FileCommand::Exit => 'x',
                FileCommand::CloseLeft | FileCommand::CloseRight => return None,
            }),
            Self::Edit(EditCommand::Undo) => Some('u'),
            Self::Edit(EditCommand::Redo) => Some('r'),
            Self::Edit(EditCommand::Cut) => Some('t'),
            Self::Edit(EditCommand::Copy) => Some('c'),
            Self::Edit(EditCommand::Paste) => Some('p'),
            Self::Edit(EditCommand::Delete) => Some('d'),
            Self::Edit(EditCommand::SelectAll) => Some('s'),
            Self::Edit(EditCommand::DuplicateLine) => None,
            Self::Edit(EditCommand::ConvertEol(_)) => None,
            Self::SelectEncoding(_) => None,
            Self::ConvertEncoding(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MenuEntry {
    Action(MenuCommand),
    DynamicAction {
        command: MenuCommand,
        label: String,
        shortcut: String,
    },
    Submenu {
        selector: &'static str,
        label: &'static str,
        mnemonic: char,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

impl MenuEntry {
    pub fn label(&self) -> &str {
        match self {
            Self::Action(command) => command.label(),
            Self::DynamicAction { label, .. } => label,
            Self::Submenu { label, .. } => label,
            Self::Separator => "",
        }
    }

    pub fn shortcut(&self) -> &str {
        match self {
            Self::Action(command) => command.shortcut(),
            Self::DynamicAction { shortcut, .. } => shortcut,
            _ => "",
        }
    }

    pub fn enabled(&self, available: &impl Fn(MenuCommand) -> bool) -> bool {
        match self {
            Self::Action(command) | Self::DynamicAction { command, .. } => available(*command),
            Self::Submenu { entries, .. } => entries.iter().any(|entry| entry.enabled(available)),
            Self::Separator => false,
        }
    }

    fn mnemonic(&self) -> Option<char> {
        match self {
            Self::Action(command) => command.mnemonic(),
            Self::DynamicAction { .. } => None,
            Self::Submenu { mnemonic, .. } => Some(*mnemonic),
            Self::Separator => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MenuDefinition {
    pub selector: &'static str,
    pub label: &'static str,
    pub mnemonic: char,
    pub entries: Vec<MenuEntry>,
}

impl MenuDefinition {
    pub fn file_menu() -> Self {
        Self {
            selector: "file-menu-trigger",
            label: "File",
            mnemonic: 'f',
            entries: FileCommand::ALL
                .into_iter()
                .filter_map(|command| {
                    if command == FileCommand::CloseRight {
                        return None;
                    }
                    if command == FileCommand::CloseLeft {
                        Some(MenuEntry::Submenu {
                            selector: "file-menu-close-multiple",
                            label: "Close Multiple Documents",
                            mnemonic: 'm',
                            entries: vec![
                                MenuEntry::Action(MenuCommand::Document(DocumentCommand::Close(
                                    crate::workspace::CloseCondition::ExceptCurrent,
                                ))),
                                MenuEntry::Action(MenuCommand::Document(DocumentCommand::Close(
                                    crate::workspace::CloseCondition::ExceptPinned,
                                ))),
                                MenuEntry::Action(MenuCommand::File(command)),
                                MenuEntry::Action(MenuCommand::File(FileCommand::CloseRight)),
                                MenuEntry::Action(MenuCommand::Document(DocumentCommand::Close(
                                    crate::workspace::CloseCondition::Unchanged,
                                ))),
                            ],
                        })
                    } else {
                        Some(MenuEntry::Action(MenuCommand::File(command)))
                    }
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuMode {
    #[default]
    Closed,
    BarFocused,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    ToggleBar,
    OpenRoot(usize),
    OpenSubmenu(&'static str),
    OpenSubmenuLimited {
        selector: &'static str,
        limit: usize,
    },
    ToggleRoot(usize),
    HoverRoot(usize),
    Up,
    Down,
    Left,
    Right,
    First,
    Last,
    Activate,
    Escape,
    Dismiss,
    Mnemonic(char),
    Hover {
        root: usize,
        path: Vec<usize>,
    },
    Click {
        root: usize,
        path: Vec<usize>,
    },
    ClickAtRevision {
        root: usize,
        path: Vec<usize>,
        revision: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOutcome {
    None,
    Activate(MenuCommand),
}

pub struct MenuBar {
    definitions: Vec<MenuDefinition>,
    mode: MenuMode,
    root: usize,
    path: Vec<usize>,
    revision: u64,
    popup_limit: Option<(usize, usize)>,
}

impl Default for MenuBar {
    fn default() -> Self {
        use crate::input::LineEnding;
        let mut menu = Self::new(vec![
            MenuDefinition::file_menu(),
            MenuDefinition {
                selector: "edit-menu-trigger",
                label: "Edit",
                mnemonic: 'e',
                entries: vec![
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::Undo)),
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::Redo)),
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::Cut)),
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::Copy)),
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::Paste)),
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::Delete)),
                    MenuEntry::Action(MenuCommand::Edit(EditCommand::SelectAll)),
                    MenuEntry::Separator,
                    MenuEntry::Submenu {
                        selector: "edit-line-operations",
                        label: "Line Operations",
                        mnemonic: 'l',
                        entries: transform_entries(&[
                            42010, 42011, 42079, 42077, 42012, 42013, 42014, 42015, 42055, 42056,
                            42057, 42058, 42083, 42078, 0, 42059, 42080, 42100, 42061, 42063,
                            42065, 42104, 0, 42060, 42081, 42101, 42062, 42064, 42066, 42105,
                        ]),
                    },
                    MenuEntry::Submenu {
                        selector: "edit-menu-eol",
                        label: "EOL Conversion",
                        mnemonic: 'e',
                        entries: [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr]
                            .into_iter()
                            .map(|mode| {
                                MenuEntry::Action(MenuCommand::Edit(EditCommand::ConvertEol(mode)))
                            })
                            .collect(),
                    },
                ],
            },
            MenuDefinition {
                selector: "search-menu-trigger",
                label: "Search",
                mnemonic: 's',
                entries: [
                    SearchCommand::Find,
                    SearchCommand::Next,
                    SearchCommand::Previous,
                    SearchCommand::Replace,
                    SearchCommand::FindInFiles,
                    SearchCommand::GoTo,
                ]
                .into_iter()
                .map(|command| MenuEntry::Action(MenuCommand::Search(command)))
                .chain([
                    MenuEntry::Separator,
                    MenuEntry::Submenu {
                        selector: "search-change-history-menu",
                        label: "Change History",
                        mnemonic: 'c',
                        entries: [
                            ChangeHistoryCommand::Next,
                            ChangeHistoryCommand::Previous,
                            ChangeHistoryCommand::Clear,
                        ]
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::ChangeHistory(command)))
                        .collect(),
                    },
                    MenuEntry::Separator,
                    MenuEntry::Submenu {
                        selector: "search-bookmark-menu",
                        label: "Bookmark",
                        mnemonic: 'b',
                        entries: [
                            BookmarkCommand::Toggle,
                            BookmarkCommand::Next,
                            BookmarkCommand::Previous,
                            BookmarkCommand::ClearAll,
                        ]
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::Bookmark(command)))
                        .collect(),
                    },
                ])
                .collect(),
            },
            MenuDefinition {
                selector: "view-menu-trigger",
                label: "View",
                mnemonic: 'v',
                entries: vec![
                    MenuEntry::Submenu {
                        selector: "view-show-symbol-menu",
                        label: "Show Symbol",
                        mnemonic: 'y',
                        entries: vec![
                            MenuEntry::Action(MenuCommand::View(ViewCommand::AllCharacters)),
                            MenuEntry::Separator,
                            MenuEntry::Action(MenuCommand::View(ViewCommand::IndentGuides)),
                        ],
                    },
                    MenuEntry::Submenu {
                        selector: "view-zoom-menu",
                        label: "Zoom",
                        mnemonic: 'z',
                        entries: [
                            ViewCommand::ZoomIn,
                            ViewCommand::ZoomOut,
                            ViewCommand::ZoomReset,
                        ]
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::View(command)))
                        .collect(),
                    },
                    MenuEntry::Action(MenuCommand::View(ViewCommand::WordWrap)),
                    MenuEntry::Submenu {
                        selector: "view-move-clone-menu",
                        label: "Move/Clone Current Document",
                        mnemonic: 'm',
                        entries: vec![
                            MenuEntry::Action(MenuCommand::View(ViewCommand::MoveToOtherView)),
                            MenuEntry::Action(MenuCommand::View(ViewCommand::CloneToOtherView)),
                            MenuEntry::Action(MenuCommand::View(ViewCommand::MoveToNewInstance)),
                            MenuEntry::Action(MenuCommand::View(ViewCommand::OpenInNewInstance)),
                        ],
                    },
                    MenuEntry::Action(MenuCommand::View(ViewCommand::SyncVertical)),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::SyncHorizontal)),
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::View(ViewCommand::Summary)),
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::View(ViewCommand::DocumentMap)),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::MarkdownPreview)),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::DocumentList)),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::FunctionList)),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::FolderWorkspace)),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::Monitoring)),
                ],
            },
            MenuDefinition {
                selector: "encoding-menu-trigger",
                label: "Encoding",
                mnemonic: 'n',
                entries: [
                    crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
                    crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Bom),
                    crate::files::UnicodeFormat::Utf16BeBom,
                    crate::files::UnicodeFormat::Utf16LeBom,
                ]
                .into_iter()
                .map(|format| MenuEntry::Action(MenuCommand::SelectEncoding(format)))
                .chain(std::iter::once(MenuEntry::Separator))
                .chain(
                    [
                        crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
                        crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Bom),
                        crate::files::UnicodeFormat::Utf16BeBom,
                        crate::files::UnicodeFormat::Utf16LeBom,
                    ]
                    .into_iter()
                    .map(|format| MenuEntry::Action(MenuCommand::ConvertEncoding(format))),
                )
                .collect(),
            },
            MenuDefinition {
                selector: "language-menu-trigger",
                label: "Language",
                mnemonic: 'l',
                entries: vec![
                    MenuEntry::Action(MenuCommand::Language(crate::syntax::Language::NormalText)),
                    MenuEntry::Submenu {
                        selector: "language-json-group",
                        label: "J",
                        mnemonic: 'j',
                        entries: vec![MenuEntry::Action(MenuCommand::Language(
                            crate::syntax::Language::Json,
                        ))],
                    },
                    MenuEntry::Action(MenuCommand::DefineUserLanguage),
                    MenuEntry::Separator,
                    MenuEntry::DynamicAction {
                        command: MenuCommand::Language(crate::syntax::Language::UserDefined(0)),
                        label: "User-Defined".into(),
                        shortcut: String::new(),
                    },
                ],
            },
            MenuDefinition {
                selector: "settings-menu-trigger",
                label: "Settings",
                mnemonic: 't',
                entries: vec![MenuEntry::Action(MenuCommand::Settings(
                    SettingsCommand::Preferences,
                ))],
            },
            MenuDefinition {
                selector: "tools-menu-trigger",
                label: "Tools",
                mnemonic: 'o',
                entries: [
                    crate::hashes::HashAlgorithm::Md5,
                    crate::hashes::HashAlgorithm::Sha1,
                    crate::hashes::HashAlgorithm::Sha256,
                    crate::hashes::HashAlgorithm::Sha512,
                ]
                .into_iter()
                .map(|algorithm| MenuEntry::Submenu {
                    selector: match algorithm {
                        crate::hashes::HashAlgorithm::Md5 => "tools-md5-menu",
                        crate::hashes::HashAlgorithm::Sha1 => "tools-sha1-menu",
                        crate::hashes::HashAlgorithm::Sha256 => "tools-sha256-menu",
                        crate::hashes::HashAlgorithm::Sha512 => "tools-sha512-menu",
                    },
                    label: algorithm.label(),
                    mnemonic: match algorithm {
                        crate::hashes::HashAlgorithm::Md5 => 'm',
                        _ => 's',
                    },
                    entries: vec![MenuEntry::Action(MenuCommand::HashSelection(algorithm))],
                })
                .collect(),
            },
            MenuDefinition {
                selector: "macro-menu-trigger",
                label: "Macro",
                mnemonic: 'm',
                entries: [
                    crate::macros::MacroCommand::StartRecording,
                    crate::macros::MacroCommand::StopRecording,
                    crate::macros::MacroCommand::Playback,
                    crate::macros::MacroCommand::SaveCurrent,
                    crate::macros::MacroCommand::RunMultiple,
                ]
                .into_iter()
                .map(|command| MenuEntry::Action(MenuCommand::Macro(command)))
                .collect(),
            },
            MenuDefinition {
                selector: "run-menu-trigger",
                label: "Run",
                mnemonic: 'r',
                entries: vec![MenuEntry::Action(MenuCommand::Run)],
            },
            MenuDefinition {
                selector: "window-menu-trigger",
                label: "Window",
                mnemonic: 'w',
                entries: vec![
                    MenuEntry::Action(MenuCommand::Window(WindowCommand::Minimize)),
                    MenuEntry::Action(MenuCommand::Window(WindowCommand::Zoom)),
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::Window(WindowCommand::BringAllToFront)),
                ],
            },
            MenuDefinition {
                selector: "help-menu-trigger",
                label: "Help",
                mnemonic: 'h',
                entries: vec![MenuEntry::Action(MenuCommand::About)],
            },
        ]);
        if let Some(edit) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "edit-menu-trigger")
        {
            edit.entries.insert(
                9,
                transform_group("edit-indent", "Indent", 'i', &[42008, 42009]),
            );
            edit.entries.insert(
                10,
                transform_group(
                    "edit-convert-case",
                    "Convert Case to",
                    'v',
                    &[42016, 42017, 42067, 42068, 42069, 42070, 42071, 42072],
                ),
            );
            edit.entries.insert(
                12,
                transform_group(
                    "edit-comment",
                    "Comment/Uncomment",
                    'm',
                    &[42022, 42035, 42036, 42023, 42047],
                ),
            );
            edit.entries.push(transform_group(
                "edit-blank",
                "Blank Operations",
                'b',
                &[42024, 42042, 42043, 42044, 42045, 0, 42046, 42054, 42053],
            ));
            edit.entries.insert(
                13,
                MenuEntry::Submenu {
                    selector: "edit-menu-auto-completion",
                    label: "Auto-Completion",
                    mnemonic: 'a',
                    entries: IntelligenceCommand::ALL
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::Intelligence(command)))
                        .collect(),
                },
            );
            edit.entries.push(transform_group(
                "edit-on-selection",
                "On Selection",
                'o',
                &[42106],
            ));
            if let Some(index) = edit.entries.iter().position(|entry| {
                matches!(
                    entry,
                    MenuEntry::Submenu {
                        selector: "edit-blank",
                        ..
                    }
                )
            }) {
                edit.entries.insert(
                    index + 1,
                    MenuEntry::Submenu {
                        selector: "edit-paste-special",
                        label: "Paste Special",
                        mnemonic: 'p',
                        entries: [
                            crate::clipboard::BinaryCommand::Copy,
                            crate::clipboard::BinaryCommand::Cut,
                            crate::clipboard::BinaryCommand::Paste,
                        ]
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::Binary(command)))
                        .collect(),
                    },
                );
            }
            for all in [true, false] {
                edit.entries.push(MenuEntry::Submenu {
                    selector: if all {
                        "edit-multiselect-all"
                    } else {
                        "edit-multiselect-next"
                    },
                    label: if all {
                        "Multi-select All"
                    } else {
                        "Multi-select Next"
                    },
                    mnemonic: if all { 'a' } else { 'n' },
                    entries: [(false, false), (true, false), (false, true), (true, true)]
                        .into_iter()
                        .map(|(match_case, whole_word)| {
                            let options = crate::editor::OccurrenceOptions {
                                match_case,
                                whole_word,
                            };
                            MenuEntry::Action(MenuCommand::MultiSelection(if all {
                                MultiSelectionCommand::All(options)
                            } else {
                                MultiSelectionCommand::Next(options)
                            }))
                        })
                        .collect(),
                });
            }
            edit.entries.extend(
                [
                    MultiSelectionCommand::Undo,
                    MultiSelectionCommand::Skip,
                    MultiSelectionCommand::Rotate,
                    MultiSelectionCommand::Swap,
                ]
                .into_iter()
                .map(|command| MenuEntry::Action(MenuCommand::MultiSelection(command))),
            );
            edit.entries.push(MenuEntry::Separator);
            edit.entries
                .push(MenuEntry::Action(MenuCommand::ColumnEditor));
            edit.entries
                .push(MenuEntry::Action(MenuCommand::CharacterPanel));
            let mut entries = crate::clipboard::CopyPath::ALL
                .into_iter()
                .map(|command| MenuEntry::Action(MenuCommand::CopyPath(command)))
                .collect::<Vec<_>>();
            entries.insert(3, MenuEntry::Separator);
            edit.entries.insert(
                9,
                MenuEntry::Submenu {
                    selector: "edit-copy-to-clipboard",
                    label: "Copy to Clipboard",
                    mnemonic: 'y',
                    entries,
                },
            );
        }
        if let Some(file) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "file-menu-trigger")
        {
            let exit = file.entries.pop();
            file.entries.extend(
                [
                    DocumentCommand::LoadSession,
                    DocumentCommand::SaveSession,
                    DocumentCommand::RestoreLastClosed,
                ]
                .into_iter()
                .map(|command| MenuEntry::Action(MenuCommand::Document(command))),
            );
            file.entries.extend(exit);
        }
        if let Some(view) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "view-menu-trigger")
        {
            let mut tabs = vec![
                MenuEntry::Action(MenuCommand::Document(DocumentCommand::Pin)),
                MenuEntry::Separator,
            ];
            tabs.extend((0..5).map(|color| {
                MenuEntry::Action(MenuCommand::Document(DocumentCommand::Color(Some(color))))
            }));
            tabs.push(MenuEntry::Action(MenuCommand::Document(
                DocumentCommand::Color(None),
            )));
            tabs.push(MenuEntry::Separator);
            tabs.extend(
                [DocumentCommand::Multiline, DocumentCommand::Vertical]
                    .into_iter()
                    .map(|command| MenuEntry::Action(MenuCommand::Document(command))),
            );
            view.entries.push(MenuEntry::Submenu {
                selector: "view-tab-menu",
                label: "Tab",
                mnemonic: 'b',
                entries: tabs,
            });
            view.entries.push(MenuEntry::Submenu {
                selector: "view-project-panels",
                label: "Project Panels",
                mnemonic: 'j',
                entries: (0..3)
                    .map(|index| {
                        MenuEntry::Action(MenuCommand::View(ViewCommand::ProjectPanel(index)))
                    })
                    .collect(),
            });
        }
        if let Some(window) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "window-menu-trigger")
        {
            window.entries.push(MenuEntry::Submenu {
                selector: "window-sort-tabs",
                label: "Sort By",
                mnemonic: 's',
                entries: [
                    crate::workspace::TabSort::Name,
                    crate::workspace::TabSort::Path,
                    crate::workspace::TabSort::Type,
                    crate::workspace::TabSort::Size,
                    crate::workspace::TabSort::Modified,
                ]
                .into_iter()
                .flat_map(|sort| {
                    [false, true].into_iter().map(move |descending| {
                        MenuEntry::Action(MenuCommand::Document(DocumentCommand::SortTabs(
                            sort, descending,
                        )))
                    })
                })
                .collect(),
            });
        }
        if let Some(settings) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "settings-menu-trigger")
        {
            settings.entries.push(MenuEntry::Submenu {
                selector: "settings-document-switcher",
                label: "Document Switcher",
                mnemonic: 'd',
                entries: [DocumentCommand::Switcher, DocumentCommand::Mru]
                    .into_iter()
                    .map(|command| MenuEntry::Action(MenuCommand::Document(command)))
                    .collect(),
            });
        }
        menu.set_embedded_tools_enabled(true, true, true);
        if let Ok(catalog) = crate::languages::catalog() {
            menu.set_builtin_languages(
                catalog
                    .profiles()
                    .iter()
                    .map(|profile| (profile.id, profile.label.clone()))
                    .collect(),
            );
        }
        menu
    }
}

fn transform_entries(ids: &[u32]) -> Vec<MenuEntry> {
    ids.iter()
        .map(|id| {
            if *id == 0 {
                MenuEntry::Separator
            } else {
                MenuEntry::Action(MenuCommand::TextTransform(*id))
            }
        })
        .collect()
}

fn transform_group(
    selector: &'static str,
    label: &'static str,
    mnemonic: char,
    ids: &[u32],
) -> MenuEntry {
    MenuEntry::Submenu {
        selector,
        label,
        mnemonic,
        entries: transform_entries(ids),
    }
}

fn transform_label(id: u32) -> &'static str {
    match id {
        42008 => "Increase Line Indent",
        42009 => "Decrease Line Indent",
        42010 => "Duplicate Current Line",
        42011 => "Transpose Current Line",
        42012 => "Split Lines",
        42013 => "Join Lines",
        42014 => "Move Up Current Line",
        42015 => "Move Down Current Line",
        42016 => "UPPERCASE",
        42017 => "lowercase",
        42022 => "Toggle Single Line Comment",
        42023 => "Block Comment",
        42024 => "Trim Trailing Space",
        42035 => "Single Line Comment",
        42036 => "Single Line Uncomment",
        42042 => "Trim Leading Space",
        42043 => "Trim Leading and Trailing Space",
        42044 => "EOL to Space",
        42045 => "Trim both and EOL to Space",
        42046 => "TAB to Space",
        42047 => "Block Uncomment",
        42053 => "Space to TAB (Leading)",
        42054 => "Space to TAB (All)",
        42055 => "Remove Empty Lines",
        42056 => "Remove Empty Lines (Containing Blank characters)",
        42057 => "Insert Blank Line Above Current",
        42058 => "Insert Blank Line Below Current",
        42059 => "Sort Lines Lexicographically Ascending",
        42060 => "Sort Lines Lexicographically Descending",
        42061 => "Sort Lines As Integers Ascending",
        42062 => "Sort Lines As Integers Descending",
        42063 => "Sort Lines As Decimals (Comma) Ascending",
        42064 => "Sort Lines As Decimals (Comma) Descending",
        42065 => "Sort Lines As Decimals (Dot) Ascending",
        42066 => "Sort Lines As Decimals (Dot) Descending",
        42067 => "Proper Case",
        42068 => "Proper Case (blend)",
        42069 => "Sentence case",
        42070 => "Sentence case (blend)",
        42071 => "iNVERT cASE",
        42072 => "ranDOm CasE",
        42077 => "Remove Consecutive Duplicate Lines",
        42078 => "Randomize Line Order",
        42079 => "Remove Duplicate Lines",
        42080 => "Sort Lines Lex. Ascending Ignoring Case",
        42081 => "Sort Lines Lex. Descending Ignoring Case",
        42083 => "Reverse Line Order",
        42100 => "Sort Lines In Locale Order Ascending",
        42101 => "Sort Lines In Locale Order Descending",
        42104 => "Sort Lines By Length Ascending",
        42105 => "Sort Lines By Length Descending",
        42106 => "Redact Selection █ (Shift: ●)",
        _ => "Unknown Text Transformation",
    }
}

impl MenuBar {
    pub fn set_builtin_languages(&mut self, languages: Vec<(u16, String)>) {
        let Some(menu) = self
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "language-menu-trigger")
        else {
            return;
        };
        let mut tail = Vec::new();
        for entry in &menu.entries {
            if matches!(
                entry,
                MenuEntry::Action(MenuCommand::DefineUserLanguage)
                    | MenuEntry::DynamicAction {
                        command: MenuCommand::Language(crate::syntax::Language::UserDefined(_)),
                        ..
                    }
            ) {
                tail.push(entry.clone());
            }
        }
        let mut entries = vec![
            MenuEntry::Action(MenuCommand::Language(crate::syntax::Language::NormalText)),
            MenuEntry::Separator,
        ];
        for &(letter, selector) in LANGUAGE_GROUPS {
            let mut group = Vec::new();
            for &(id, _, label) in BUILTIN_LANGUAGE_MENU {
                if id == 0
                    || !label[..1].eq_ignore_ascii_case(letter)
                    || (id != 57 && !languages.iter().any(|(candidate, _)| *candidate == id))
                {
                    continue;
                }
                group.push(MenuEntry::DynamicAction {
                    command: MenuCommand::Language(if id == 57 {
                        crate::syntax::Language::Json
                    } else {
                        crate::syntax::Language::Builtin(id)
                    }),
                    label: label.to_owned(),
                    shortcut: String::new(),
                });
            }
            if letter == "K" {
                entries.extend(group);
            } else if !group.is_empty() {
                entries.push(MenuEntry::Submenu {
                    selector,
                    label: letter,
                    mnemonic: letter.as_bytes()[0].to_ascii_lowercase() as char,
                    entries: group,
                });
            }
        }
        entries.push(MenuEntry::Separator);
        entries.extend(tail);
        menu.entries = entries;
        self.revision = self.revision.wrapping_add(1);
        self.mode = MenuMode::Closed;
        self.path.clear();
    }

    pub fn set_saved_macros(&mut self, names: Vec<String>) {
        let Some(menu) = self
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "macro-menu-trigger")
        else {
            return;
        };
        menu.entries.retain(|entry| {
            !matches!(
                entry,
                MenuEntry::DynamicAction {
                    command: MenuCommand::SavedMacro(_),
                    ..
                }
            )
        });
        if matches!(menu.entries.last(), Some(MenuEntry::Separator)) {
            menu.entries.pop();
        }
        if !names.is_empty() {
            menu.entries.push(MenuEntry::Separator);
            menu.entries
                .extend(names.into_iter().enumerate().map(|(index, label)| {
                    MenuEntry::DynamicAction {
                        command: MenuCommand::SavedMacro(index),
                        label,
                        shortcut: String::new(),
                    }
                }));
        }
        self.revision = self.revision.wrapping_add(1);
        self.mode = MenuMode::Closed;
        self.path.clear();
    }

    pub fn set_user_languages(&mut self, languages: Vec<(u64, String)>) {
        let Some(menu) = self
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "language-menu-trigger")
        else {
            return;
        };
        menu.entries.retain(|entry| {
            !matches!(
                entry,
                MenuEntry::DynamicAction {
                    command: MenuCommand::Language(crate::syntax::Language::UserDefined(_)),
                    ..
                }
            )
        });
        if matches!(menu.entries.last(), Some(MenuEntry::Separator)) {
            menu.entries.pop();
        }
        menu.entries.push(MenuEntry::Separator);
        menu.entries.push(MenuEntry::DynamicAction {
            command: MenuCommand::Language(crate::syntax::Language::UserDefined(0)),
            label: "User-Defined".into(),
            shortcut: String::new(),
        });
        menu.entries.extend(
            languages
                .into_iter()
                .filter(|(id, _)| *id != 0)
                .map(|(id, label)| MenuEntry::DynamicAction {
                    command: MenuCommand::Language(crate::syntax::Language::UserDefined(id)),
                    label,
                    shortcut: String::new(),
                }),
        );
        self.revision = self.revision.wrapping_add(1);
        self.mode = MenuMode::Closed;
        self.path.clear();
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set_embedded_tools_enabled(&mut self, mime: bool, converter: bool, html: bool) {
        let position = self
            .definitions
            .iter()
            .position(|definition| definition.selector == "plugins-menu-trigger")
            .or_else(|| {
                self.definitions
                    .iter()
                    .position(|definition| definition.selector == "window-menu-trigger")
            })
            .unwrap_or(self.definitions.len());
        self.definitions
            .retain(|definition| definition.selector != "plugins-menu-trigger");
        let mut entries = Vec::new();
        if mime {
            entries.push(MenuEntry::Submenu {
                selector: "mime-tools-menu",
                label: "MIME Tools",
                mnemonic: 'm',
                entries: [
                    crate::mime::MimeOperation::Base64Encode,
                    crate::mime::MimeOperation::Base64EncodePadded,
                    crate::mime::MimeOperation::Base64Decode,
                ]
                .into_iter()
                .map(|operation| MenuEntry::Action(MenuCommand::Mime(operation)))
                .collect(),
            });
        }
        if converter {
            entries.push(MenuEntry::Submenu {
                selector: "converter-menu",
                label: "Converter",
                mnemonic: 'c',
                entries: [
                    crate::converter::ConverterOperation::AsciiToHex,
                    crate::converter::ConverterOperation::HexToAscii,
                ]
                .into_iter()
                .map(|operation| MenuEntry::Action(MenuCommand::Converter(operation)))
                .collect(),
            });
        }
        if html {
            entries.push(MenuEntry::Submenu {
                selector: "npp-export-menu",
                label: "NppExport",
                mnemonic: 'n',
                entries: vec![MenuEntry::Action(MenuCommand::ExportHtml)],
            });
        }
        if !entries.is_empty() {
            self.definitions.insert(
                position.min(self.definitions.len()),
                MenuDefinition {
                    selector: "plugins-menu-trigger",
                    label: "Plugins",
                    mnemonic: 'p',
                    entries,
                },
            );
        }
        self.mode = MenuMode::Closed;
        self.root = 0;
        self.path.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn new(definitions: Vec<MenuDefinition>) -> Self {
        Self {
            definitions,
            mode: MenuMode::Closed,
            root: 0,
            path: Vec::new(),
            revision: 0,
            popup_limit: None,
        }
    }
    pub fn mode(&self) -> MenuMode {
        self.mode
    }
    pub fn active_root(&self) -> usize {
        self.root
    }
    pub fn selected_path(&self) -> &[usize] {
        &self.path
    }
    pub fn definitions(&self) -> &[MenuDefinition] {
        &self.definitions
    }
    pub fn popup_count(&self) -> usize {
        if self.mode == MenuMode::Open {
            self.path.len().max(1)
        } else {
            0
        }
    }

    pub fn popup_entries(&self, depth: usize) -> Option<&[MenuEntry]> {
        let mut entries = self.definitions.get(self.root)?.entries.as_slice();
        for &index in self.path.get(..depth)? {
            entries = match entries.get(index)? {
                MenuEntry::Submenu { entries, .. } => entries,
                _ => return None,
            };
        }
        Some(
            if let Some((_, count)) = self
                .popup_limit
                .filter(|(limited_depth, _)| *limited_depth == depth)
            {
                &entries[..count.min(entries.len())]
            } else {
                entries
            },
        )
    }

    fn entry_at(&self, path: &[usize]) -> Option<&MenuEntry> {
        if self
            .popup_limit
            .is_some_and(|(depth, count)| path.get(depth).is_some_and(|index| *index >= count))
        {
            return None;
        }
        let (&last, parents) = path.split_last()?;
        let mut entries = self.definitions.get(self.root)?.entries.as_slice();
        for &index in parents {
            entries = match entries.get(index)? {
                MenuEntry::Submenu { entries, .. } => entries,
                _ => return None,
            };
        }
        entries.get(last)
    }

    fn visible_path(&self, root: usize, path: &[usize]) -> bool {
        let Some(depth) = path.len().checked_sub(1) else {
            return false;
        };
        self.mode == MenuMode::Open
            && root == self.root
            && depth < self.popup_count()
            && self.path.get(..depth) == path.get(..depth)
            && self.entry_at(path).is_some()
    }

    fn open_root(&mut self, root: usize, available: &impl Fn(MenuCommand) -> bool) {
        if self.popup_limit.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        let Some(definition) = self.definitions.get(root) else {
            return;
        };
        let first = definition
            .entries
            .iter()
            .position(|entry| entry.enabled(available));
        self.root = root;
        self.mode = MenuMode::Open;
        self.path = first.into_iter().collect();
    }

    fn switch_root(&mut self, forward: bool, available: &impl Fn(MenuCommand) -> bool) {
        let count = self.definitions.len();
        if count <= 1 {
            return;
        }
        let next = if forward {
            (self.root + 1) % count
        } else {
            (self.root + count - 1) % count
        };
        if self.mode == MenuMode::Open {
            self.open_root(next, available);
        } else if self.mode == MenuMode::BarFocused {
            self.root = next;
            self.path.clear();
        }
    }

    fn move_selection(&mut self, forward: bool, available: &impl Fn(MenuCommand) -> bool) {
        if self.mode == MenuMode::BarFocused {
            self.open_root(self.root, available);
            return;
        }
        if self.mode != MenuMode::Open {
            return;
        }
        let depth = self.path.len().saturating_sub(1);
        let Some(entries) = self.popup_entries(depth) else {
            return;
        };
        let choices = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.enabled(available).then_some(index))
            .collect::<Vec<_>>();
        if choices.is_empty() {
            return;
        }
        let current = self
            .path
            .get(depth)
            .and_then(|current| choices.iter().position(|index| index == current));
        let selected = current
            .map(|index| {
                if forward {
                    (index + 1) % choices.len()
                } else {
                    (index + choices.len() - 1) % choices.len()
                }
            })
            .unwrap_or(if forward { 0 } else { choices.len() - 1 });
        self.path.truncate(depth);
        self.path.push(choices[selected]);
    }

    fn select_edge(&mut self, last: bool, available: &impl Fn(MenuCommand) -> bool) {
        if self.mode != MenuMode::Open {
            return;
        }
        let depth = self.path.len().saturating_sub(1);
        let Some(entries) = self.popup_entries(depth) else {
            return;
        };
        let index = if last {
            entries.iter().rposition(|entry| entry.enabled(available))
        } else {
            entries.iter().position(|entry| entry.enabled(available))
        };
        if let Some(index) = index {
            self.path.truncate(depth);
            self.path.push(index);
        }
    }

    fn open_submenu(&mut self, available: &impl Fn(MenuCommand) -> bool) -> bool {
        let Some(MenuEntry::Submenu { entries, .. }) = self.entry_at(&self.path) else {
            return false;
        };
        let first = entries.iter().position(|entry| entry.enabled(available));
        if let Some(first) = first {
            self.path.push(first);
            true
        } else {
            false
        }
    }

    fn activate(&mut self, available: &impl Fn(MenuCommand) -> bool) -> MenuOutcome {
        if self.mode == MenuMode::BarFocused {
            self.open_root(self.root, available);
            return MenuOutcome::None;
        }
        if self.mode != MenuMode::Open {
            return MenuOutcome::None;
        }
        if let Some(MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. }) =
            self.entry_at(&self.path)
        {
            let command = *command;
            if available(command) {
                self.mode = MenuMode::Closed;
                self.path.clear();
                return MenuOutcome::Activate(command);
            }
        } else {
            self.open_submenu(available);
        }
        MenuOutcome::None
    }

    pub fn handle(
        &mut self,
        event: MenuEvent,
        available: &impl Fn(MenuCommand) -> bool,
    ) -> MenuOutcome {
        if let MenuEvent::OpenSubmenuLimited { selector, limit } = event {
            let outcome = self.handle(MenuEvent::OpenSubmenu(selector), available);
            if self.mode == MenuMode::Open && self.path.len() > 1 {
                self.popup_limit = Some((self.path.len() - 1, limit));
                self.revision = self.revision.wrapping_add(1);
                self.select_edge(false, available);
            }
            return outcome;
        }
        if let MenuEvent::ClickAtRevision {
            root,
            path,
            revision,
        } = event
        {
            return if revision == self.revision {
                self.handle(MenuEvent::Click { root, path }, available)
            } else {
                MenuOutcome::None
            };
        }
        let click = matches!(&event, MenuEvent::Click { .. });
        match event {
            MenuEvent::ToggleBar => {
                if self.mode == MenuMode::Closed {
                    self.mode = MenuMode::BarFocused;
                    self.root = 0;
                } else {
                    self.mode = MenuMode::Closed;
                }
                self.path.clear();
            }
            MenuEvent::ClickAtRevision { .. } => {}
            MenuEvent::OpenSubmenuLimited { .. } => {}
            MenuEvent::OpenRoot(root) => self.open_root(root, available),
            MenuEvent::OpenSubmenu(selector) => {
                let target = self.definitions.iter().enumerate().find_map(|(root, definition)| {
                    definition.entries.iter().enumerate().find_map(|(index, entry)| {
                        matches!(entry, MenuEntry::Submenu { selector: candidate, .. } if *candidate == selector)
                            .then_some((root, index))
                            .filter(|_| entry.enabled(available))
                    })
                });
                if let Some((root, index)) = target {
                    self.open_root(root, available);
                    self.path = vec![index];
                    self.open_submenu(available);
                }
            }
            MenuEvent::ToggleRoot(root) => {
                if self.mode == MenuMode::Open && root == self.root {
                    self.mode = MenuMode::Closed;
                    self.path.clear();
                } else {
                    self.open_root(root, available);
                }
            }
            MenuEvent::HoverRoot(root) => {
                if self.mode == MenuMode::Open {
                    self.open_root(root, available);
                } else if self.mode == MenuMode::BarFocused && root < self.definitions.len() {
                    self.root = root;
                }
            }
            MenuEvent::Up => self.move_selection(false, available),
            MenuEvent::Down => self.move_selection(true, available),
            MenuEvent::First => self.select_edge(false, available),
            MenuEvent::Last => self.select_edge(true, available),
            MenuEvent::Left => {
                if self.mode == MenuMode::Open && self.path.len() > 1 {
                    self.path.pop();
                } else {
                    self.switch_root(false, available);
                }
            }
            MenuEvent::Right => {
                if self.mode == MenuMode::Open && self.open_submenu(available) {
                } else if self.path.len() <= 1 {
                    self.switch_root(true, available);
                }
            }
            MenuEvent::Activate => return self.activate(available),
            MenuEvent::Escape => {
                if self.mode == MenuMode::Open && self.path.len() > 1 {
                    self.path.pop();
                } else if self.mode == MenuMode::Open {
                    self.mode = MenuMode::BarFocused;
                    self.path.clear();
                } else {
                    self.mode = MenuMode::Closed;
                    self.path.clear();
                }
            }
            MenuEvent::Dismiss => {
                self.mode = MenuMode::Closed;
                self.path.clear();
            }
            MenuEvent::Hover { root, path } | MenuEvent::Click { root, path } => {
                if self.visible_path(root, &path)
                    && self
                        .entry_at(&path)
                        .is_some_and(|entry| entry.enabled(available))
                {
                    self.path = path;
                    if click {
                        return self.activate(available);
                    } else {
                        self.open_submenu(available);
                    }
                }
            }
            MenuEvent::Mnemonic(key) => {
                let key = key.to_ascii_lowercase();
                if self.mode == MenuMode::BarFocused {
                    if let Some(index) = self
                        .definitions
                        .iter()
                        .position(|definition| definition.mnemonic == key)
                    {
                        self.open_root(index, available);
                    }
                } else if self.mode == MenuMode::Open {
                    let depth = self.path.len().saturating_sub(1);
                    if let Some(entries) = self.popup_entries(depth) {
                        let matches = entries
                            .iter()
                            .enumerate()
                            .filter_map(|(index, entry)| {
                                (entry.mnemonic() == Some(key) && entry.enabled(available))
                                    .then_some(index)
                            })
                            .collect::<Vec<_>>();
                        if !matches.is_empty() {
                            let next = self
                                .path
                                .get(depth)
                                .and_then(|current| {
                                    matches.iter().position(|index| index == current)
                                })
                                .map(|index| (index + 1) % matches.len())
                                .unwrap_or(0);
                            self.path.truncate(depth);
                            self.path.push(matches[next]);
                            if matches.len() == 1 {
                                return self.activate(available);
                            }
                        }
                    }
                }
            }
        }
        MenuOutcome::None
    }
}

const LANGUAGE_GROUPS: &[(&str, &str)] = &[
    ("A", "language-a-group"),
    ("B", "language-b-group"),
    ("C", "language-c-group"),
    ("D", "language-d-group"),
    ("E", "language-e-group"),
    ("F", "language-f-group"),
    ("G", "language-g-group"),
    ("H", "language-h-group"),
    ("I", "language-i-group"),
    ("J", "language-json-group"),
    ("K", "language-k-group"),
    ("L", "language-l-group"),
    ("M", "language-m-group"),
    ("N", "language-n-group"),
    ("O", "language-o-group"),
    ("P", "language-p-group"),
    ("R", "language-r-group"),
    ("S", "language-s-group"),
    ("T", "language-t-group"),
    ("V", "language-v-group"),
    ("X", "language-x-group"),
    ("Y", "language-y-group"),
];

const BUILTIN_LANGUAGE_MENU: &[(u16, u32, &str)] = &[
    (0, 46016, "None (Normal Text)"),
    (27, 46028, "ActionScript"),
    (42, 46042, "Ada"),
    (65, 46064, "ASN.1"),
    (16, 46009, "ASP"),
    (32, 46033, "Assembly"),
    (40, 46044, "AutoIt"),
    (66, 46065, "AviSynth"),
    (60, 46059, "BaanC"),
    (12, 46022, "Batch"),
    (67, 46066, "Blitzbasic"),
    (2, 46002, "C"),
    (4, 46023, "C#"),
    (3, 46003, "C++"),
    (41, 46040, "Caml"),
    (48, 46048, "CMake"),
    (50, 46050, "COBOL"),
    (70, 46069, "CSound"),
    (56, 46056, "CoffeeScript"),
    (20, 46010, "CSS"),
    (52, 46051, "D"),
    (33, 46034, "Diff"),
    (71, 46070, "Erlang"),
    (94, 46093, "ErrorList"),
    (95, 46094, "Escape Sequence (ANSI)"),
    (72, 46071, "ESCRIPT"),
    (73, 46072, "Forth"),
    (25, 46026, "Fortran (free form)"),
    (59, 46058, "Fortran (fixed form)"),
    (69, 46068, "Freebasic"),
    (88, 46087, "GDScript"),
    (90, 46089, "Go"),
    (51, 46052, "Gui4Cli"),
    (45, 46046, "Haskell"),
    (89, 46088, "Hollywood"),
    (8, 46005, "HTML"),
    (13, 46019, "INI file"),
    (46, 46047, "Inno Setup"),
    (62, 46061, "Intel HEX"),
    (6, 46004, "Java"),
    (58, 46007, "JavaScript"),
    (57, 46057, "JSON"),
    (86, 46085, "JSON5"),
    (55, 46055, "JSP"),
    (39, 46041, "KIXtart"),
    (74, 46073, "LaTeX"),
    (30, 46031, "LISP"),
    (23, 46024, "Lua"),
    (10, 46018, "Makefile"),
    (44, 46045, "Matlab"),
    (87, 46086, "Microsoft Transact-SQL"),
    (75, 46074, "MMIXAL"),
    (14, 46015, "MS-DOS Style"),
    (76, 46075, "Nim"),
    (77, 46076, "Nncrontab"),
    (28, 46029, "NSIS"),
    (5, 46014, "Objective-C"),
    (78, 46077, "OScript"),
    (11, 46011, "Pascal"),
    (21, 46013, "Perl"),
    (1, 46008, "PHP"),
    (35, 46036, "PostScript"),
    (53, 46053, "PowerShell"),
    (34, 46035, "Properties"),
    (68, 46067, "Purebasic"),
    (22, 46012, "Python"),
    (54, 46054, "R"),
    (91, 46090, "Raku"),
    (79, 46078, "REBOL"),
    (80, 46079, "Registry"),
    (7, 46017, "Resource file"),
    (36, 46037, "Ruby"),
    (81, 46080, "Rust"),
    (93, 46092, "SAS"),
    (26, 46027, "Shell"),
    (31, 46032, "Scheme"),
    (37, 46038, "Smalltalk"),
    (82, 46081, "Spice"),
    (17, 46020, "SQL"),
    (96, 46095, "Structured Text"),
    (64, 46063, "Swift"),
    (61, 46060, "S-Record"),
    (29, 46030, "TCL"),
    (63, 46062, "Tektronix extended HEX"),
    (24, 46025, "TeX"),
    (92, 46091, "TOML"),
    (83, 46082, "txt2tags"),
    (85, 46084, "TypeScript"),
    (18, 46021, "Visual Basic"),
    (84, 46083, "Visual Prolog"),
    (38, 46039, "VHDL"),
    (43, 46043, "Verilog"),
    (9, 46006, "XML"),
    (49, 46049, "YAML"),
];
