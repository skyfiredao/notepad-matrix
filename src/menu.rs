#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCommand {
    New,
    Open,
    Reveal,
    Terminal,
    ContainingWorkspace,
    FolderWorkspace,
    DefaultViewer,
    Rename,
    Trash,
    PrintNow,
    Recent(usize),
    OpenAllRecent,
    ClearRecent,
    Save,
    SaveAs,
    SaveCopy,
    SaveAll,
    Close,
    CloseAll,
    Reload,
    CloseLeft,
    CloseRight,
    Print,
    Exit,
}

impl FileCommand {
    pub const ALL: [Self; 23] = [
        Self::New,
        Self::Open,
        Self::Reveal,
        Self::Terminal,
        Self::ContainingWorkspace,
        Self::FolderWorkspace,
        Self::DefaultViewer,
        Self::Rename,
        Self::Trash,
        Self::PrintNow,
        Self::OpenAllRecent,
        Self::ClearRecent,
        Self::Save,
        Self::SaveAs,
        Self::SaveCopy,
        Self::SaveAll,
        Self::Close,
        Self::CloseAll,
        Self::Reload,
        Self::CloseLeft,
        Self::CloseRight,
        Self::Print,
        Self::Exit,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "新建(N)",
            Self::Open => "打开(O)...",
            Self::Reveal => "Finder",
            Self::Terminal => "终端",
            Self::ContainingWorkspace => "文件夹工作区",
            Self::FolderWorkspace => "打开文件夹作为工作区(W)...",
            Self::DefaultViewer => "使用默认程序打开(D)",
            Self::Rename => "重命名(R)...",
            Self::Trash => "从磁盘删除(B)",
            Self::PrintNow => "立即打印(W)",
            Self::Recent(_) => "近期文件",
            Self::OpenAllRecent => "打开所有最近的文件",
            Self::ClearRecent => "清空最近的文件列表",
            Self::Save => "保存(S)",
            Self::SaveAs => "另存为(A)...",
            Self::SaveCopy => "拷贝另存为(Y)...",
            Self::SaveAll => "全部保存(E)",
            Self::Close => "关闭(C)",
            Self::CloseAll => "全部关闭(E)",
            Self::Reload => "重新读取文件(L)",
            Self::CloseLeft => "关闭左侧所有文件",
            Self::CloseRight => "关闭右侧所有文件",
            Self::Print => "打印(P)...",
            Self::Exit => "退出(X)",
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
            Self::Print => "Cmd+P",
            Self::Exit => "Alt+F4",
            _ => "",
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
                    (false, false) => "忽略大小写和全词匹配",
                    (true, false) => "只匹配大小写",
                    (false, true) => "只匹配全词",
                    (true, true) => "匹配大小写和全词",
                }
            }
            Self::Undo => "撤销最近添加的多重选取",
            Self::Skip => "跳过当前并前往下一处多重选取",
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
            Self::Close(crate::workspace::CloseCondition::ExceptCurrent) => "关闭当前以外所有文件",
            Self::Close(crate::workspace::CloseCondition::ExceptPinned) => {
                "关闭固定标签页以外所有文件"
            }
            Self::Close(crate::workspace::CloseCondition::Unchanged) => "关闭所有未修改的文件",
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
            Self::RestoreLastClosed => "恢复最近一个关闭的文件",
            Self::SaveSession => "保存会话(I)...",
            Self::LoadSession => "读取会话(I)...",
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
            Self::Function => "函数自动补全",
            Self::Word => "单词自动补全",
            Self::Parameters => "函数参数提示",
            Self::PreviousParameters => "函数参数上一个提示",
            Self::NextParameters => "函数参数下一个提示",
            Self::Path => "路径自动补全",
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
    Source(u32),
    ActivateDocument(crate::workspace::BufferId),
    DocumentDropdown,
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
    OpenTerminal,
    OpenRemoteTerminal(u64),
    TerminalSettings,
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
    StyleConfigurator,
    ShortcutMapper,
    ImportStyleThemes,
    EditContextMenu,
}

pub(crate) fn product_command_label(id: u32, locale: &str) -> Option<&'static str> {
    match (id, locale == "chineseSimplified.xml") {
        (41020, true) => Some("打开系统终端"),
        (41020, false) => Some("Open System Terminal"),
        (49000, true) => Some("本机终端"),
        (49000, false) => Some("Local Terminal"),
        (42033, true) => Some("设置 macOS 中的只读属性"),
        (42033, false) => Some("Read-Only Permissions in macOS"),
        (47000, true) => Some("关于 Notepad Matrix"),
        (47000, false) => Some("About Notepad Matrix"),
        (47002, true) => Some("Notepad Matrix 项目主页"),
        (47002, false) => Some("Notepad Matrix project"),
        _ => None,
    }
}

pub fn source_command(id: u32) -> Option<(&'static str, &'static str, &'static str)> {
    if let Some(page) = crate::codepages::codepage(id) {
        return Some((page.label, "", page.label));
    }
    Some(match id {
        46300 => ("Open User Defined Language folder...", "", "language-open-udl-folder"),
        46301 => ("Notepad++ User Defined Languages Collection", "", "language-udl-collection"),
        45004 => ("使用 ANSI 编码", "", "encoding-ansi"),
        45008 => ("使用 UTF-8 编码", "", "encoding-utf8"),
        45005 => ("使用 UTF-8-BOM 编码", "", "encoding-utf8-bom"),
        45006 => ("使用 UTF-16 BE BOM 编码", "", "encoding-utf16-be"),
        45007 => ("使用 UTF-16 LE BOM 编码", "", "encoding-utf16-le"),
        45009 => ("转为 ANSI 编码", "", "encoding-convert-ansi"),
        45010 => ("转为 UTF-8 编码", "", "encoding-convert-utf8"),
        45011 => ("转为 UTF-8-BOM 编码", "", "encoding-convert-utf8-bom"),
        45012 => ("转为 UTF-16 BE BOM 编码", "", "encoding-convert-utf16-be"),
        45013 => ("转为 UTF-16 LE BOM 编码", "", "encoding-convert-utf16-le"),
        47002 => ("Notepad Matrix 项目主页", "", "help-project"),
        47010 => ("命令行参数...", "", "help-command-line"),
        47012 => ("调试信息...", "", "help-debug-info"),
        48501 => ("生成...", "", "tools-md5-text"),
        48502 => ("从文件生成...", "", "tools-md5-files"),
        48503 => ("从选区生成并复制到剪贴板", "", "tools-md5-selection"),
        48504 => ("生成...", "", "tools-sha256-text"),
        48505 => ("从文件生成...", "", "tools-sha256-files"),
        48506 => ("从选区生成并复制到剪贴板", "", "tools-sha256-selection"),
        48507 => ("生成...", "", "tools-sha1-text"),
        48508 => ("从文件生成...", "", "tools-sha1-files"),
        48509 => ("从选区生成并复制到剪贴板", "", "tools-sha1-selection"),
        48510 => ("生成...", "", "tools-sha512-text"),
        48511 => ("从文件生成...", "", "tools-sha512-files"),
        48512 => ("从选区生成并复制到剪贴板", "", "tools-sha512-selection"),
        59003 => ("Conversion Panel", "", "converter-panel"),
        59005 => ("Edit Configuration File", "", "converter-config"),
        59006 => ("About", "", "converter-about"),
        11001 => ("文件窗口(W)...", "", "window-file-manager"),
        44034 => ("总在最前", "", "view-always-on-top"),
        44032 => ("切换全屏模式", "F11", "view-fullscreen"),
        44009 => ("便笺模式", "F12", "view-post-it"),
        44011 => ("沉浸模式", "", "view-distraction-free"),
        44072 => ("激活从视图", "F8", "view-switch-pane"),
        44042 => ("隐藏行", "Alt+H", "view-hide-lines"),
        44010 => ("折叠所有层次", "Alt+0", "view-fold-all"),
        44029 => ("展开所有层次", "Alt+Shift+0", "view-unfold-all"),
        44030 => ("折叠当前层次", "Ctrl+Alt+F", "view-fold-current"),
        44031 => ("展开当前层次", "Ctrl+Alt+Shift+F", "view-unfold-current"),
        44051 => ("1", "Alt+1", "view-fold-1"),
        44052 => ("2", "Alt+2", "view-fold-2"),
        44053 => ("3", "Alt+3", "view-fold-3"),
        44054 => ("4", "Alt+4", "view-fold-4"),
        44055 => ("5", "Alt+5", "view-fold-5"),
        44056 => ("6", "Alt+6", "view-fold-6"),
        44057 => ("7", "Alt+7", "view-fold-7"),
        44058 => ("8", "Alt+8", "view-fold-8"),
        44061 => ("1", "Alt+Shift+1", "view-unfold-1"),
        44062 => ("2", "Alt+Shift+2", "view-unfold-2"),
        44063 => ("3", "Alt+Shift+3", "view-unfold-3"),
        44064 => ("4", "Alt+Shift+4", "view-unfold-4"),
        44065 => ("5", "Alt+Shift+5", "view-unfold-5"),
        44066 => ("6", "Alt+Shift+6", "view-unfold-6"),
        44067 => ("7", "Alt+Shift+7", "view-unfold-7"),
        44068 => ("8", "Alt+Shift+8", "view-unfold-8"),
        44100 => ("Firefox", "", "view-browser-firefox"),
        44101 => ("Chrome", "", "view-browser-chrome"),
        44102 => ("Edge", "", "view-browser-edge"),
        42020 => ("开始/结束 选取(S)", "Ctrl+Shift+B", "edit-begin-end-select"),
        42089 => (
            "开始/结束 按列选取",
            "Alt+Shift+B",
            "edit-begin-end-column-select",
        ),
        42084 => ("日期时间(短)", "", "edit-date-short"),
        42085 => ("日期时间(长)", "", "edit-date-long"),
        42086 => ("日期时间(自定义)", "", "edit-date-custom"),
        42037 => ("列选模式...", "", "edit-column-tip"),
        42028 => ("当前文件设为只读", "", "edit-user-read-only"),
        42033 => ("设置 macOS 中的只读属性", "", "edit-file-read-only"),
        42073 => ("打开文件", "", "edit-open-selected-file"),
        42074 => (
            "在文件管理器中打开所在文件夹",
            "",
            "edit-reveal-selected-file",
        ),
        42075 => ("在线搜索", "Alt+F2", "edit-search-online"),
        42076 => ("更改搜索引擎...", "", "edit-change-search-engine"),
        42102 => ("所有文件设为只读", "", "edit-all-read-only"),
        42103 => ("清除所有文件只读状态", "", "edit-all-read-write"),
        43048 => ("选中并查找下一个(S)", "Ctrl+F3", "search-selection-next"),
        43011 => ("增量查找(I)", "Ctrl+Alt+I", "search-incremental"),
        43052 => ("查找范围内的字符(E)...", "", "search-character-range"),
        43049 => (
            "选中并查找上一个(S)",
            "Ctrl+Shift+F3",
            "search-selection-previous",
        ),
        43014 => ("快速查找下一个(V)", "Ctrl+Alt+F3", "search-volatile-next"),
        43015 => (
            "快速查找上一个(V)",
            "Ctrl+Alt+Shift+F3",
            "search-volatile-previous",
        ),
        43045 => ("搜索结果窗口(W)", "F7", "search-results-focus"),
        43046 => ("下一个搜索结果(T)", "F4", "search-result-next"),
        43047 => ("上一个搜索结果(T)", "Shift+F4", "search-result-previous"),
        43009 => ("转到匹配的括号(M)", "Ctrl+B", "search-brace-match"),
        43053 => ("选中匹配括号之间的所有字符(E)", "Ctrl+Alt+B", "search-brace-select"),
        43018 => ("剪切书签行", "", "search-bookmark-cut"),
        43019 => ("复制书签行", "", "search-bookmark-copy"),
        43020 => ("粘贴(替换)书签行", "", "search-bookmark-paste"),
        43021 => ("删除书签行", "", "search-bookmark-remove"),
        43051 => ("删除未标记书签行", "", "search-bookmark-remove-unmarked"),
        43050 => ("反向标记书签", "", "search-bookmark-invert"),
        43054 => ("标记(K)...", "Ctrl+M", "search-mark"),
        43022 => ("使用格式1", "", "search-style-all-1"),
        43024 => ("使用格式2", "", "search-style-all-2"),
        43026 => ("使用格式3", "", "search-style-all-3"),
        43028 => ("使用格式4", "", "search-style-all-4"),
        43030 => ("使用格式5", "", "search-style-all-5"),
        43062 => ("使用格式1", "", "search-style-one-1"),
        43063 => ("使用格式2", "", "search-style-one-2"),
        43064 => ("使用格式3", "", "search-style-one-3"),
        43065 => ("使用格式4", "", "search-style-one-4"),
        43066 => ("使用格式5", "", "search-style-one-5"),
        43023 => ("清除格式1", "", "search-style-clear-1"),
        43025 => ("清除格式2", "", "search-style-clear-2"),
        43027 => ("清除格式3", "", "search-style-clear-3"),
        43029 => ("清除格式4", "", "search-style-clear-4"),
        43031 => ("清除格式5", "", "search-style-clear-5"),
        43032 => ("清除所有格式", "", "search-style-clear-all"),
        43033 => ("格式1", "Ctrl+Shift+1", "search-style-previous-1"),
        43034 => ("格式2", "Ctrl+Shift+2", "search-style-previous-2"),
        43035 => ("格式3", "Ctrl+Shift+3", "search-style-previous-3"),
        43036 => ("格式4", "Ctrl+Shift+4", "search-style-previous-4"),
        43037 => ("格式5", "Ctrl+Shift+5", "search-style-previous-5"),
        43038 => ("寻找标记格式", "Ctrl+Shift+0", "search-mark-previous"),
        43039 => ("格式1", "Ctrl+1", "search-style-next-1"),
        43040 => ("格式2", "Ctrl+2", "search-style-next-2"),
        43041 => ("格式3", "Ctrl+3", "search-style-next-3"),
        43042 => ("格式4", "Ctrl+4", "search-style-next-4"),
        43043 => ("格式5", "Ctrl+5", "search-style-next-5"),
        43044 => ("寻找标记格式", "Ctrl+0", "search-mark-next"),
        43055 => ("格式1", "", "search-style-copy-1"),
        43056 => ("格式2", "", "search-style-copy-2"),
        43057 => ("格式3", "", "search-style-copy-3"),
        43058 => ("格式4", "", "search-style-copy-4"),
        43059 => ("格式5", "", "search-style-copy-5"),
        43060 => ("所有格式", "", "search-style-copy-all"),
        43061 => ("寻找标记格式", "", "search-mark-copy"),
        _ => return None,
    })
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
            Self::Next => "下一处变更",
            Self::Previous => "上一处变更",
            Self::Clear => "清除变更记录",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewCommand {
    ProjectPanel(usize),
    Whitespace,
    EndOfLine,
    Nonprinting,
    ControlCharacters,
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
            Self::DocumentList => MenuCommand::DocumentDropdown,
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
            Self::DocumentList => "Document list",
            Self::Close => "Close document",
        }
    }
}

impl MenuCommand {
    pub fn source_id(self) -> Option<u32> {
        match self {
            Self::Source(id) => (source_command(id).is_some()
                || id == 48016).then_some(id),
            Self::ChangeHistory(command) => Some(command.source_id()),
            Self::Edit(command) => Some(match command {
                EditCommand::Cut => 42001,
                EditCommand::Copy => 42002,
                EditCommand::Undo => 42003,
                EditCommand::Redo => 42004,
                EditCommand::Paste => 42005,
                EditCommand::Delete => 42006,
                EditCommand::SelectAll => 42007,
                EditCommand::DuplicateLine => 42010,
                EditCommand::ConvertEol(crate::input::LineEnding::CrLf) => 45001,
                EditCommand::ConvertEol(crate::input::LineEnding::Lf) => 45002,
                EditCommand::ConvertEol(crate::input::LineEnding::Cr) => 45003,
            }),
            Self::Search(command) => Some(match command {
                SearchCommand::Find => 43001,
                SearchCommand::Next => 43002,
                SearchCommand::Replace => 43003,
                SearchCommand::GoTo => 43004,
                SearchCommand::Previous => 43010,
                SearchCommand::FindInFiles => 43013,
            }),
            Self::Settings(command) => Some(match command {
                SettingsCommand::Preferences => 48011,
                SettingsCommand::StyleConfigurator => 46001,
                SettingsCommand::ShortcutMapper => 48009,
                SettingsCommand::ImportStyleThemes => 48006,
                SettingsCommand::EditContextMenu => 48018,
            }),
            Self::Bookmark(command) => Some(match command {
                BookmarkCommand::Toggle => 43005,
                BookmarkCommand::Next => 43006,
                BookmarkCommand::Previous => 43007,
                BookmarkCommand::ClearAll => 43008,
            }),
            Self::File(command) => Some(match command {
                FileCommand::New => 41001,
                FileCommand::Open => 41002,
                FileCommand::Close => 41003,
                FileCommand::CloseAll => 41004,
                FileCommand::Save => 41006,
                FileCommand::SaveAll => 41007,
                FileCommand::SaveAs => 41008,
                FileCommand::CloseLeft => 41009,
                FileCommand::Print => 41010,
                FileCommand::Exit => 41011,
                FileCommand::Reload => 41014,
                FileCommand::SaveCopy => 41015,
                FileCommand::Trash => 41016,
                FileCommand::Rename => 41017,
                FileCommand::CloseRight => 41018,
                FileCommand::Reveal => 41019,
                FileCommand::Terminal => 41020,
                FileCommand::FolderWorkspace => 41022,
                FileCommand::DefaultViewer => 41023,
                FileCommand::ContainingWorkspace => 41025,
                FileCommand::PrintNow => 1001,
                FileCommand::OpenAllRecent => 42040,
                FileCommand::ClearRecent => 42041,
                FileCommand::Recent(index @ 0..=29) => 41028 + index as u32,
                FileCommand::Recent(_) => return None,
            }),
            Self::View(ViewCommand::Whitespace) => Some(44025),
            Self::View(ViewCommand::EndOfLine) => Some(44026),
            Self::View(ViewCommand::Nonprinting) => Some(44130),
            Self::View(ViewCommand::ControlCharacters) => Some(44131),
            Self::View(ViewCommand::AllCharacters) => Some(44019),
            Self::View(ViewCommand::IndentGuides) => Some(44020),
            Self::View(ViewCommand::WordWrap) => Some(44022),
            Self::View(ViewCommand::ZoomIn) => Some(44023),
            Self::View(ViewCommand::ZoomOut) => Some(44024),
            Self::View(ViewCommand::ZoomReset) => Some(44033),
            Self::View(ViewCommand::SyncVertical) => Some(44035),
            Self::View(ViewCommand::SyncHorizontal) => Some(44036),
            Self::View(ViewCommand::Summary) => Some(44049),
            Self::View(ViewCommand::DocumentList) => Some(44070),
            Self::View(ViewCommand::DocumentMap) => Some(44080),
            Self::View(ViewCommand::FunctionList) => Some(44084),
            Self::View(ViewCommand::FolderWorkspace) => Some(44085),
            Self::View(ViewCommand::Monitoring) => Some(44097),
            Self::View(ViewCommand::MoveToOtherView) => Some(10001),
            Self::View(ViewCommand::CloneToOtherView) => Some(10002),
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
            Self::About => Some(47000),
            Self::OpenTerminal => Some(49000),
            Self::DefineUserLanguage => Some(46250),
            Self::Macro(command) => Some(match command {
                crate::macros::MacroCommand::StartRecording => 42018,
                crate::macros::MacroCommand::StopRecording => 42019,
                crate::macros::MacroCommand::Playback => 42021,
                crate::macros::MacroCommand::SaveCurrent => 42025,
                crate::macros::MacroCommand::RunMultiple => 42032,
            }),
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
            Self::Source(id) => source_command(id).map_or("", |value| value.0),
            Self::ActivateDocument(_) => "Activate document",
            Self::DocumentDropdown => "Document list",
            Self::Binary(command) => command.label(),
            Self::CharacterPanel => "字符面板(P)",
            Self::CopyPath(command) => command.label(),
            Self::Intelligence(command) => command.label(),
            Self::TextTransform(id) => transform_label(id),
            Self::MultiSelection(command) => command.label(),
            Self::ColumnEditor => "列编辑器(N)...",
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
            Self::OpenTerminal => "Local Terminal",
            Self::OpenRemoteTerminal(_) => "Remote Terminal",
            Self::TerminalSettings => "Terminal Configuration...",
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
            Self::View(ViewCommand::Whitespace) => "显示空格与制表符",
            Self::View(ViewCommand::EndOfLine) => "显示换行符",
            Self::View(ViewCommand::Nonprinting) => "显示非打印字符",
            Self::View(ViewCommand::ControlCharacters) => "显示控制字符和 Unicode 换行符",
            Self::View(ViewCommand::AllCharacters) => "显示所有字符",
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
            Self::Bookmark(BookmarkCommand::Toggle) => "设置/取消书签",
            Self::ChangeHistory(command) => command.label(),
            Self::Bookmark(BookmarkCommand::Next) => "下一个书签",
            Self::Bookmark(BookmarkCommand::Previous) => "上一个书签",
            Self::Bookmark(BookmarkCommand::ClearAll) => "清除所有书签",
            Self::Mime(crate::mime::MimeOperation::Base64Encode) => "Base64 Encode",
            Self::Mime(crate::mime::MimeOperation::Base64EncodePadded) => {
                "Base64 Encode with padding"
            }
            Self::Mime(crate::mime::MimeOperation::Base64Decode) => "Base64 Decode",
            Self::Settings(SettingsCommand::Preferences) => "首选项...",
            Self::Settings(SettingsCommand::StyleConfigurator) => "语言样式配置器...",
            Self::Settings(SettingsCommand::ShortcutMapper) => "管理快捷键...",
            Self::Settings(SettingsCommand::ImportStyleThemes) => "导入主题...",
            Self::Settings(SettingsCommand::EditContextMenu) => "编辑弹出菜单",
            Self::Search(SearchCommand::Find) => "查找(F)...",
            Self::Search(SearchCommand::FindInFiles) => "在文件中查找(L)",
            Self::Search(SearchCommand::Next) => "查找下一个(N)",
            Self::Search(SearchCommand::Previous) => "查找上一个(P)",
            Self::Search(SearchCommand::GoTo) => "定位(G)...",
            Self::Search(SearchCommand::Replace) => "替换(R)...",
            Self::Edit(EditCommand::Undo) => "撤消(U)",
            Self::Edit(EditCommand::Redo) => "恢复(R)",
            Self::Edit(EditCommand::Cut) => "剪切(T)",
            Self::Edit(EditCommand::Copy) => "复制(C)",
            Self::Edit(EditCommand::Paste) => "粘贴(P)",
            Self::Edit(EditCommand::Delete) => "删除(D)",
            Self::Edit(EditCommand::SelectAll) => "全选(S)",
            Self::Edit(EditCommand::DuplicateLine) => "重复当前行",
            Self::Edit(EditCommand::ConvertEol(LineEnding::CrLf)) => "Windows (CR LF)",
            Self::Edit(EditCommand::ConvertEol(LineEnding::Lf)) => "Unix (LF)",
            Self::Edit(EditCommand::ConvertEol(LineEnding::Cr)) => "Macintosh (CR)",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Source(id) => source_command(id).map_or("", |value| value.1),
            Self::ActivateDocument(_) | Self::DocumentDropdown => "",
            Self::Binary(_) => "",
            Self::CharacterPanel => "",
            Self::CopyPath(_) => "",
            Self::Intelligence(command) => command.shortcut(),
            Self::TextTransform(42012) => "Ctrl+I",
            Self::TextTransform(42013) => "Ctrl+J",
            Self::TextTransform(42014) => "Ctrl+Shift+Up",
            Self::TextTransform(42015) => "Ctrl+Shift+Down",
            Self::TextTransform(42016) => "Ctrl+Shift+U",
            Self::TextTransform(42017) => "Ctrl+U",
            Self::TextTransform(42022) => "Ctrl+Q",
            Self::TextTransform(42023) => "Ctrl+Shift+Q",
            Self::TextTransform(42035) => "Ctrl+K",
            Self::TextTransform(42036) => "Ctrl+Shift+K",
            Self::TextTransform(42057) => "Ctrl+Alt+Enter",
            Self::TextTransform(42058) => "Ctrl+Alt+Shift+Enter",
            Self::TextTransform(42067) => "Alt+U",
            Self::TextTransform(42068) => "Alt+Shift+U",
            Self::TextTransform(42069) => "Ctrl+Alt+U",
            Self::TextTransform(42070) => "Ctrl+Alt+Shift+U",
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
            Self::About => "F1",
            Self::OpenTerminal => "F5",
            Self::OpenRemoteTerminal(_) => "",
            Self::TerminalSettings => "",
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
            Self::Edit(EditCommand::Undo) => "Ctrl+Z / Alt+Backspace",
            Self::Edit(EditCommand::Redo) => "Ctrl+Y / Ctrl+Shift+Z",
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
            Self::Source(_) => self
                .label()
                .rsplit_once('(')
                .and_then(|(_, suffix)| suffix.chars().next())
                .filter(char::is_ascii_alphabetic)
                .map(|ch| ch.to_ascii_lowercase()),
            Self::ActivateDocument(_) | Self::DocumentDropdown => None,
            Self::Binary(_) => None,
            Self::CharacterPanel => Some('p'),
            Self::CopyPath(_) => None,
            Self::Intelligence(_) => None,
            Self::TextTransform(42016) => Some('u'),
            Self::TextTransform(42017) => Some('l'),
            Self::TextTransform(42067) => Some('p'),
            Self::TextTransform(42069) => Some('s'),
            Self::TextTransform(42071) => Some('i'),
            Self::TextTransform(42072) => Some('r'),
            Self::TextTransform(_) => None,
            Self::MultiSelection(_) => None,
            Self::ColumnEditor => Some('n'),
            Self::Document(DocumentCommand::LoadSession | DocumentCommand::SaveSession) => {
                Some('i')
            }
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
            Self::View(
                ViewCommand::Whitespace
                | ViewCommand::EndOfLine
                | ViewCommand::Nonprinting
                | ViewCommand::ControlCharacters
                | ViewCommand::AllCharacters
                | ViewCommand::IndentGuides,
            ) => None,
            Self::Bookmark(_) => None,
            Self::ChangeHistory(_) => None,
            Self::Settings(_) => None,
            Self::Mime(_) => None,
            Self::Converter(_) => None,
            Self::ExportHtml => Some('e'),
            Self::Window(WindowCommand::Minimize) => Some('m'),
            Self::Window(WindowCommand::Zoom) => Some('z'),
            Self::Window(WindowCommand::BringAllToFront) => Some('b'),
            Self::About => Some('a'),
            Self::OpenTerminal => Some('r'),
            Self::OpenRemoteTerminal(_) => None,
            Self::TerminalSettings => None,
            Self::HashSelection(_) => None,
            Self::Language(_) => None,
            Self::Macro(crate::macros::MacroCommand::StartRecording) => Some('c'),
            Self::Macro(crate::macros::MacroCommand::StopRecording) => Some('t'),
            Self::Macro(crate::macros::MacroCommand::Playback) => Some('p'),
            Self::Macro(crate::macros::MacroCommand::SaveCurrent) => Some('s'),
            Self::Macro(crate::macros::MacroCommand::RunMultiple) => Some('r'),
            Self::SavedMacro(_) => None,
            Self::Search(SearchCommand::Find) => Some('f'),
            Self::Search(SearchCommand::FindInFiles) => Some('l'),
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
                FileCommand::SaveAll => 'e',
                FileCommand::Close => 'c',
                FileCommand::CloseAll => 'e',
                FileCommand::Reload => 'l',
                FileCommand::Print => 'p',
                FileCommand::Exit => 'x',
                FileCommand::FolderWorkspace | FileCommand::PrintNow => 'w',
                FileCommand::DefaultViewer => 'd',
                FileCommand::Rename => 'r',
                FileCommand::Trash => 'b',
                _ => return None,
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
    DynamicSubmenu {
        selector: String,
        label: String,
        mnemonic: Option<char>,
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
            Self::DynamicSubmenu { label, .. } => label,
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
            Self::Submenu { selector, .. } if selector.starts_with("edit-") => true,
            Self::Submenu { entries, .. } => entries.iter().any(|entry| entry.enabled(available)),
            Self::DynamicSubmenu { entries, .. } => entries.iter().any(|entry| entry.enabled(available)),
            Self::Separator => false,
        }
    }

    fn mnemonic(&self) -> Option<char> {
        match self {
            Self::Action(command) => command.mnemonic(),
            Self::DynamicAction { label, .. } => Self::label_mnemonic(label),
            Self::Submenu { mnemonic, .. } => Some(*mnemonic),
            Self::DynamicSubmenu { mnemonic, .. } => *mnemonic,
            Self::Separator => None,
        }
    }

    pub fn command(&self) -> Option<MenuCommand> {
        match self {
            Self::Action(command) | Self::DynamicAction { command, .. } => Some(*command),
            _ => None,
        }
    }

    pub fn submenu_entries(&self) -> Option<&[MenuEntry]> {
        match self {
            Self::Submenu { entries, .. } | Self::DynamicSubmenu { entries, .. } => Some(entries),
            _ => None,
        }
    }

    pub fn submenu_selector(&self) -> Option<&str> {
        match self {
            Self::Submenu { selector, .. } => Some(selector),
            Self::DynamicSubmenu { selector, .. } => Some(selector),
            _ => None,
        }
    }

    pub(crate) fn label_mnemonic(label: &str) -> Option<char> {
        let mut chars = label.chars();
        while let Some(ch) = chars.next() {
            if ch == '&' && let Some(next) = chars.next() && next != '&' {
                return Some(next.to_ascii_lowercase());
            }
        }
        None
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
    pub fn direct_command(&self) -> Option<MenuCommand> {
        None
    }

    pub fn file_menu() -> Self {
        Self {
            selector: "file-menu-trigger",
            label: "文件(F)",
            mnemonic: 'f',
            entries: vec![
                MenuEntry::Action(MenuCommand::File(FileCommand::New)),
                MenuEntry::Action(MenuCommand::File(FileCommand::Open)),
                MenuEntry::Submenu {
                    selector: "file-open-folder",
                    label: "打开所在文件夹(F)",
                    mnemonic: 'f',
                    entries: vec![
                        MenuEntry::Action(MenuCommand::File(FileCommand::Reveal)),
                        MenuEntry::Action(MenuCommand::File(FileCommand::Terminal)),
                        MenuEntry::Separator,
                        MenuEntry::Action(MenuCommand::File(FileCommand::ContainingWorkspace)),
                    ],
                },
                MenuEntry::Action(MenuCommand::File(FileCommand::DefaultViewer)),
                MenuEntry::Action(MenuCommand::File(FileCommand::FolderWorkspace)),
                MenuEntry::Action(MenuCommand::File(FileCommand::Reload)),
                MenuEntry::Action(MenuCommand::File(FileCommand::Save)),
                MenuEntry::Action(MenuCommand::File(FileCommand::SaveAs)),
                MenuEntry::Action(MenuCommand::File(FileCommand::SaveCopy)),
                MenuEntry::Action(MenuCommand::File(FileCommand::SaveAll)),
                MenuEntry::Action(MenuCommand::File(FileCommand::Rename)),
                MenuEntry::Action(MenuCommand::File(FileCommand::Close)),
                MenuEntry::Action(MenuCommand::File(FileCommand::CloseAll)),
                MenuEntry::Submenu {
                    selector: "file-menu-close-multiple",
                    label: "关闭多个文档(M)",
                    mnemonic: 'm',
                    entries: vec![
                        MenuEntry::Action(MenuCommand::Document(DocumentCommand::Close(
                            crate::workspace::CloseCondition::ExceptCurrent,
                        ))),
                        MenuEntry::Action(MenuCommand::Document(DocumentCommand::Close(
                            crate::workspace::CloseCondition::ExceptPinned,
                        ))),
                        MenuEntry::Action(MenuCommand::File(FileCommand::CloseLeft)),
                        MenuEntry::Action(MenuCommand::File(FileCommand::CloseRight)),
                        MenuEntry::Action(MenuCommand::Document(DocumentCommand::Close(
                            crate::workspace::CloseCondition::Unchanged,
                        ))),
                    ],
                },
                MenuEntry::Action(MenuCommand::File(FileCommand::Trash)),
                MenuEntry::Separator,
                MenuEntry::Action(MenuCommand::Document(DocumentCommand::LoadSession)),
                MenuEntry::Action(MenuCommand::Document(DocumentCommand::SaveSession)),
                MenuEntry::Separator,
                MenuEntry::Action(MenuCommand::File(FileCommand::Print)),
                MenuEntry::Action(MenuCommand::File(FileCommand::PrintNow)),
                MenuEntry::Separator,
                MenuEntry::Action(MenuCommand::File(FileCommand::Exit)),
            ],
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
    OpenDocuments(Vec<(crate::workspace::BufferId, String)>),
    OpenContext(usize, Vec<MenuEntry>),
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

fn source_entries(ids: &[u32]) -> Vec<MenuEntry> {
    ids.iter().map(|id| if *id == 0 { MenuEntry::Separator } else { MenuEntry::Action(MenuCommand::Source(*id)) }).collect()
}

fn encoding_entries() -> Vec<MenuEntry> {
    let groups = crate::codepages::GROUPS.iter().map(|group| {
        let (selector, label) = match group.label {
            "Arabic" => ("encoding-arabic", "阿拉伯语"),
            "Baltic" => ("encoding-baltic", "波罗的海语"),
            "Celtic" => ("encoding-celtic", "凯尔特语"),
            "Cyrillic" => ("encoding-cyrillic", "西里尔语"),
            "Central European" => ("encoding-central-european", "中欧"),
            "Chinese" => ("encoding-chinese", "中文"),
            "Eastern European" => ("encoding-eastern-european", "东欧"),
            "Greek" => ("encoding-greek", "希腊语"),
            "Hebrew" => ("encoding-hebrew", "希伯来语"),
            "Japanese" => ("encoding-japanese", "日语"),
            "Korean" => ("encoding-korean", "韩语"),
            "North European" => ("encoding-north-european", "北欧"),
            "Thai" => ("encoding-thai", "泰语"),
            "Turkish" => ("encoding-turkish", "土耳其语"),
            "Western European" => ("encoding-western-european", "西欧"),
            "Vietnamese" => ("encoding-vietnamese", "越南语"),
            _ => (group.label, group.label),
        };
        MenuEntry::Submenu { selector, label, mnemonic: '\0', entries: group.entries.iter()
            .map(|page| MenuEntry::Action(MenuCommand::Source(page.id))).collect() }
    }).collect();
    crate::codepages::SOURCE_ENCODINGS.iter()
        .map(|entry| MenuEntry::Action(MenuCommand::Source(entry.id)))
        .chain([MenuEntry::Submenu { selector: "encoding-character-sets", label: "编码字符集", mnemonic: '\0', entries: groups }, MenuEntry::Separator])
        .chain(crate::codepages::CONVERT_ENCODINGS.iter().map(|entry| MenuEntry::Action(MenuCommand::Source(entry.id))))
        .collect()
}

fn source_group(
    selector: &'static str,
    label: &'static str,
    mnemonic: char,
    ids: &[u32],
) -> MenuEntry {
    MenuEntry::Submenu {
        selector,
        label,
        mnemonic,
        entries: ids
            .iter()
            .copied()
            .map(|id| MenuEntry::Action(MenuCommand::Source(id)))
            .collect(),
    }
}

pub struct MenuBar {
    definitions: Vec<MenuDefinition>,
    mode: MenuMode,
    root: usize,
    path: Vec<usize>,
    revision: u64,
    popup_limit: Option<(usize, usize)>,
    documents: Option<Vec<MenuEntry>>,
    configured_popup: Option<Vec<MenuEntry>>,
    recent_range: Option<std::ops::Range<usize>>,
}

impl Default for MenuBar {
    fn default() -> Self {
        use crate::input::LineEnding;
        let mut menu = Self::new(vec![
            MenuDefinition::file_menu(),
            MenuDefinition {
                selector: "edit-menu-trigger",
                label: "编辑(E)",
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
                        label: "行操作(L)",
                        mnemonic: 'l',
                        entries: transform_entries(&[
                            42010, 42079, 42077, 42012, 42013, 42014, 42015, 42055, 42056, 42057,
                            42058, 42083, 42078, 0, 42059, 42080, 42100, 42061, 42063, 42065,
                            42104, 0, 42060, 42081, 42101, 42062, 42064, 42066, 42105,
                        ]),
                    },
                    MenuEntry::Submenu {
                        selector: "edit-menu-eol",
                        label: "文档格式转换(E)",
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
                label: "搜索(S)",
                mnemonic: 's',
                entries: [
                    MenuEntry::Action(MenuCommand::Search(SearchCommand::Find)),
                    MenuEntry::Action(MenuCommand::Search(SearchCommand::FindInFiles)),
                    MenuEntry::Action(MenuCommand::Search(SearchCommand::Next)),
                    MenuEntry::Action(MenuCommand::Search(SearchCommand::Previous)),
                    MenuEntry::Action(MenuCommand::Source(43048)),
                    MenuEntry::Action(MenuCommand::Source(43049)),
                    MenuEntry::Action(MenuCommand::Source(43014)),
                    MenuEntry::Action(MenuCommand::Source(43015)),
                    MenuEntry::Action(MenuCommand::Search(SearchCommand::Replace)),
                    MenuEntry::Action(MenuCommand::Source(43011)),
                    MenuEntry::Action(MenuCommand::Source(43045)),
                    MenuEntry::Action(MenuCommand::Source(43046)),
                    MenuEntry::Action(MenuCommand::Source(43047)),
                    MenuEntry::Action(MenuCommand::Search(SearchCommand::GoTo)),
                    MenuEntry::Action(MenuCommand::Source(43009)),
                    MenuEntry::Action(MenuCommand::Source(43053)),
                ]
                .into_iter()
                .chain([
                    MenuEntry::Action(MenuCommand::Source(43054)),
                    MenuEntry::Separator,
                    MenuEntry::Submenu {
                        selector: "search-change-history-menu",
                        label: "变更记录",
                        mnemonic: '\0',
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
                    source_group(
                        "search-style-all",
                        "用格式标记全部(A)",
                        'a',
                        &[43022, 43024, 43026, 43028, 43030],
                    ),
                    source_group(
                        "search-style-one",
                        "用格式标记选定的文本(O)",
                        'o',
                        &[43062, 43063, 43064, 43065, 43066],
                    ),
                    source_group(
                        "search-style-clear",
                        "清除格式标记",
                        '\0',
                        &[43023, 43025, 43027, 43029, 43031, 43032],
                    ),
                    source_group(
                        "search-style-previous",
                        "到上一个格式标记(J)",
                        'j',
                        &[43033, 43034, 43035, 43036, 43037, 43038],
                    ),
                    source_group(
                        "search-style-next",
                        "到下一个格式标记(D)",
                        'd',
                        &[43039, 43040, 43041, 43042, 43043, 43044],
                    ),
                    source_group(
                        "search-style-copy",
                        "复制格式标记文本(C)",
                        'c',
                        &[43055, 43056, 43057, 43058, 43059, 43060, 43061],
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Submenu {
                        selector: "search-bookmark-menu",
                        label: "书签(B)",
                        mnemonic: 'b',
                        entries: [
                            BookmarkCommand::Toggle,
                            BookmarkCommand::Next,
                            BookmarkCommand::Previous,
                            BookmarkCommand::ClearAll,
                        ]
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::Bookmark(command)))
                        .chain([43018, 43019, 43020, 43021, 43051, 43050]
                            .map(|id| MenuEntry::Action(MenuCommand::Source(id))))
                        .collect(),
                    },
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::Source(43052)),
                ])
                .collect(),
            },
            MenuDefinition {
                selector: "view-menu-trigger",
                label: "视图(V)",
                mnemonic: 'v',
                entries: vec![
                    MenuEntry::Submenu {
                        selector: "view-show-symbol-menu",
                        label: "显示符号",
                        mnemonic: 'y',
                        entries: vec![
                            MenuEntry::Action(MenuCommand::View(ViewCommand::Whitespace)),
                            MenuEntry::Action(MenuCommand::View(ViewCommand::EndOfLine)),
                            MenuEntry::Action(MenuCommand::View(ViewCommand::Nonprinting)),
                            MenuEntry::Action(MenuCommand::View(ViewCommand::ControlCharacters)),
                            MenuEntry::Separator,
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
                label: "编码(N)",
                mnemonic: 'n',
                entries: encoding_entries(),
            },
            MenuDefinition {
                selector: "language-menu-trigger",
                label: "语言(L)",
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
                    MenuEntry::Submenu {
                        selector: "language-user-defined-language",
                        label: "User Defined Language",
                        mnemonic: 'u',
                        entries: vec![
                            MenuEntry::Action(MenuCommand::DefineUserLanguage),
                            MenuEntry::Action(MenuCommand::Source(46300)),
                            MenuEntry::Action(MenuCommand::Source(46301)),
                        ],
                    },
                    MenuEntry::DynamicAction {
                        command: MenuCommand::Language(crate::syntax::Language::UserDefined(0)),
                        label: "User-Defined".into(),
                        shortcut: String::new(),
                    },
                ],
            },
            MenuDefinition {
                selector: "settings-menu-trigger",
                label: "设置(T)",
                mnemonic: 't',
                entries: vec![
                    MenuEntry::Action(MenuCommand::Settings(SettingsCommand::Preferences)),
                    MenuEntry::Action(MenuCommand::Settings(SettingsCommand::StyleConfigurator)),
                    MenuEntry::Action(MenuCommand::Settings(SettingsCommand::ShortcutMapper)),
                    MenuEntry::Separator,
                    MenuEntry::Submenu {
                        selector: "settings-import-submenu",
                        label: "导入",
                        mnemonic: 'i',
                        entries: vec![MenuEntry::Action(MenuCommand::Settings(SettingsCommand::ImportStyleThemes))],
                    },
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::Settings(SettingsCommand::EditContextMenu)),
                ],
            },
            MenuDefinition {
                selector: "tools-menu-trigger",
                label: "工具(O)",
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
                    entries: source_entries(match algorithm {
                        crate::hashes::HashAlgorithm::Md5 => &[48501, 48502, 48503],
                        crate::hashes::HashAlgorithm::Sha1 => &[48507, 48508, 48509],
                        crate::hashes::HashAlgorithm::Sha256 => &[48504, 48505, 48506],
                        crate::hashes::HashAlgorithm::Sha512 => &[48510, 48511, 48512],
                    }),
                })
                .collect(),
            },
            MenuDefinition {
                selector: "macro-menu-trigger",
                label: "宏(M)",
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
                selector: "terminal-menu-trigger",
                label: "终端(R)",
                mnemonic: 'r',
                entries: vec![
                    MenuEntry::Action(MenuCommand::OpenTerminal),
                    MenuEntry::Action(MenuCommand::TerminalSettings),
                ],
            },
            MenuDefinition {
                selector: "window-menu-trigger",
                label: "窗口(W)",
                mnemonic: 'w',
                entries: Vec::new(),
            },
            MenuDefinition {
                selector: "help-menu-trigger",
                label: "?",
                mnemonic: '?',
                entries: source_entries(&[47010, 0, 47002, 0, 47012])
                    .into_iter().chain([MenuEntry::Action(MenuCommand::About)]).collect(),
            },
        ]);
        if let Some(edit) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "edit-menu-trigger")
        {
            edit.entries.insert(
                9,
                transform_group("edit-indent", "缩进(I)", 'i', &[42008, 42009]),
            );
            edit.entries.insert(
                10,
                transform_group(
                    "edit-convert-case",
                    "转换大小写(V)",
                    'v',
                    &[42016, 42017, 42067, 42068, 42069, 42070, 42071, 42072],
                ),
            );
            edit.entries.insert(
                12,
                transform_group(
                    "edit-comment",
                    "注释/取消注释(M)",
                    'm',
                    &[42022, 42035, 42036, 42023, 42047],
                ),
            );
            edit.entries.push(transform_group(
                "edit-blank",
                "空白字符操作(B)",
                'b',
                &[42024, 42042, 42043, 42044, 42045, 0, 42046, 42054, 42053],
            ));
            edit.entries.insert(
                13,
                MenuEntry::Submenu {
                    selector: "edit-menu-auto-completion",
                    label: "自动补全(A)",
                    mnemonic: 'a',
                    entries: IntelligenceCommand::ALL
                        .into_iter()
                        .map(|command| MenuEntry::Action(MenuCommand::Intelligence(command)))
                        .collect(),
                },
            );
            edit.entries.push(transform_group(
                "edit-on-selection",
                "对选中内容操作(O)",
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
                        label: "选择性粘贴(P)",
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
            edit.entries.push(MenuEntry::Separator);
            for all in [true, false] {
                edit.entries.push(MenuEntry::Submenu {
                    selector: if all {
                        "edit-multiselect-all"
                    } else {
                        "edit-multiselect-next"
                    },
                    label: if all {
                        "多重选取（全部）"
                    } else {
                        "多重选取（下一处）"
                    },
                    mnemonic: '\0',
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
                [MultiSelectionCommand::Undo, MultiSelectionCommand::Skip]
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
                    label: "复制到剪贴板(Y)",
                    mnemonic: 'y',
                    entries,
                },
            );
        }
        if let Some(edit) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "edit-menu-trigger")
        {
            if let Some(at) = edit
                .entries
                .iter()
                .position(|entry| {
                    matches!(
                        entry,
                        MenuEntry::Action(MenuCommand::Edit(EditCommand::SelectAll))
                    )
                })
            {
            let at = at + 1;
            let end = at + usize::from(matches!(edit.entries.get(at), Some(MenuEntry::Separator)));
            edit.entries.splice(
                at..end,
                [
                    MenuEntry::Action(MenuCommand::Source(42020)),
                    MenuEntry::Action(MenuCommand::Source(42089)),
                    MenuEntry::Separator,
                    source_group("edit-date-time", "插入", '\0', &[42084, 42085, 42086]),
                ],
            );
            }
            if let Some(column) = edit
                .entries
                .iter()
                .position(|entry| matches!(entry, MenuEntry::Action(MenuCommand::ColumnEditor))) {
            edit.entries
                .insert(column, MenuEntry::Action(MenuCommand::Source(42037)));
            }
            edit.entries.extend([
                MenuEntry::Separator,
                source_group(
                    "edit-read-only",
                    "在 Notepad++ 中只读",
                    '\0',
                    &[42028, 42102, 42103],
                ),
                MenuEntry::Action(MenuCommand::Source(42033)),
            ]);
            if let Some(MenuEntry::Submenu { entries, .. }) =
                edit.entries.iter_mut().find(|entry| {
                    matches!(
                        entry,
                        MenuEntry::Submenu {
                            selector: "edit-on-selection",
                            ..
                        }
                    )
                })
            {
                entries.splice(
                    0..0,
                    [
                        MenuEntry::Action(MenuCommand::Source(42073)),
                        MenuEntry::Action(MenuCommand::Source(42074)),
                        MenuEntry::Separator,
                    ],
                );
                entries.extend([
                    MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::Source(42075)),
                    MenuEntry::Action(MenuCommand::Source(42076)),
                ]);
            }
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
        if let Some(view) = menu.definitions.iter_mut().find(|menu| menu.selector == "view-menu-trigger") {
            let mut entries = source_entries(&[44034, 44032, 44009, 44011, 0]);
            entries.push(source_group("view-browser-menu", "在浏览器中查看当前文件", '\0', &[44100, 44101, 44102]));
            entries.push(MenuEntry::Separator);
            for selector in ["view-show-symbol-menu", "view-zoom-menu", "view-move-clone-menu", "view-tab-menu"] {
                if let Some(entry) = view.entries.iter().find(|entry| entry.submenu_selector() == Some(selector)) {
                    entries.push(entry.clone());
                }
            }
            entries.push(MenuEntry::Action(MenuCommand::View(ViewCommand::WordWrap)));
            entries.extend(source_entries(&[44072, 44042, 0, 44010, 44029, 44030, 44031]));
            entries.push(source_group("view-fold-level", "折叠层次", '\0', &[44051, 44052, 44053, 44054, 44055, 44056, 44057, 44058]));
            entries.push(source_group("view-unfold-level", "展开层次", '\0', &[44061, 44062, 44063, 44064, 44065, 44066, 44067, 44068]));
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Action(MenuCommand::View(ViewCommand::Summary)));
            entries.push(MenuEntry::Separator);
            if let Some(entry) = view.entries.iter().find(|entry| matches!(entry, MenuEntry::Submenu { selector: "view-project-panels", .. })) {
                entries.push(entry.clone());
            }
            entries.extend([ViewCommand::FolderWorkspace, ViewCommand::DocumentMap, ViewCommand::DocumentList, ViewCommand::FunctionList, ViewCommand::MarkdownPreview]
                .into_iter().map(|command| MenuEntry::Action(MenuCommand::View(command))));
            entries.push(MenuEntry::Separator);
            entries.extend([ViewCommand::SyncVertical, ViewCommand::SyncHorizontal].into_iter().map(|command| MenuEntry::Action(MenuCommand::View(command))));
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Action(MenuCommand::View(ViewCommand::Monitoring)));
            view.entries = entries;
        }
        if let Some(window) = menu
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "window-menu-trigger")
        {
            window.entries.push(MenuEntry::Action(MenuCommand::Source(11001)));
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
        42008 => "插入制表符(缩进)",
        42009 => "删除制表符(退格)",
        42010 => "重复当前行",
        42011 => "Transpose Current Line",
        42012 => "分割行",
        42013 => "合并行",
        42014 => "上移当前行",
        42015 => "下移当前行",
        42016 => "转成大写(U)",
        42017 => "转成小写(L)",
        42022 => "添加/删除单行注释",
        42023 => "区块注释",
        42024 => "移除行尾空格",
        42035 => "设置行注释",
        42036 => "取消行注释",
        42042 => "移除行首空格",
        42043 => "移除行首和行尾空格",
        42044 => "换行符转空格",
        42045 => "移除行首尾空格并且行尾转空格",
        42046 => "TAB 转空格",
        42047 => "清除区块注释",
        42053 => "空格转 TAB (行首)",
        42054 => "空格转 TAB (全部)",
        42055 => "移除空行",
        42056 => "移除空行(包括空白字符)",
        42057 => "在当前行上方插入空行",
        42058 => "在当前行下方插入空行",
        42059 => "升序排列文本行",
        42060 => "降序排列文本行",
        42061 => "升序排列整数",
        42062 => "降序排列整数",
        42063 => "升序排列小数（逗号作为小数点）",
        42064 => "降序排列小数（逗号作为小数点）",
        42065 => "升序排列小数（句点作为小数点）",
        42066 => "降序排列小数（句点作为小数点）",
        42067 => "每词首字母大写，其他小写(P)",
        42068 => "每词首字母大写，其他不变",
        42069 => "每句首字母大写，其他小写(S)",
        42070 => "每句首字母大写，其他不变",
        42071 => "大小写互换(I)",
        42072 => "随机大小写(R)",
        42077 => "删除连续的重复行",
        42078 => "随机排列行",
        42079 => "删除重复行",
        42080 => "升序排列文本行（忽略大小写）",
        42081 => "降序排列文本行（忽略大小写）",
        42083 => "逆序排列行",
        42100 => "按区域设置升序排列文本行",
        42101 => "按区域设置降序排列文本行",
        42104 => "按行长升序排序",
        42105 => "按行长降序排列",
        42106 => "隐藏选定内容 █（Shift：●）)",
        _ => "Unknown Text Transformation",
    }
}

impl MenuBar {
    pub fn set_window_documents(&mut self, documents: Vec<(crate::workspace::BufferId, String)>) {
        let Some(menu) = self.definitions.iter_mut().find(|menu| menu.selector == "window-menu-trigger") else { return; };
        let unchanged = if documents.is_empty() {
            menu.entries.iter().all(|entry| !matches!(entry,
                MenuEntry::DynamicAction { command: MenuCommand::ActivateDocument(_), .. } | MenuEntry::Separator))
        } else {
            menu.entries.iter().position(|entry| matches!(entry, MenuEntry::Separator)).is_some_and(|start| {
                menu.entries[..start].iter().all(|entry| !matches!(entry,
                    MenuEntry::DynamicAction { command: MenuCommand::ActivateDocument(_), .. }))
                    && menu.entries.len() - start - 1 == documents.len()
                    && menu.entries[start + 1..].iter().zip(&documents).enumerate().all(|(index, (entry, (id, name)))| {
                        match entry {
                            MenuEntry::DynamicAction { command: MenuCommand::ActivateDocument(current), label, shortcut } => {
                                current == id && shortcut.is_empty()
                                    && label.split_once(": ").is_some_and(|(number, text)| number == (index + 1).to_string() && text == name)
                            }
                            _ => false,
                        }
                    })
            })
        };
        if unchanged { return; }
        let mut entries: Vec<_> = menu.entries.iter().filter(|entry| !matches!(entry,
            MenuEntry::DynamicAction { command: MenuCommand::ActivateDocument(_), .. } | MenuEntry::Separator)).cloned().collect();
        if !documents.is_empty() {
            entries.push(MenuEntry::Separator);
            entries.extend(documents.into_iter().enumerate().map(|(index, (id, name))| MenuEntry::DynamicAction {
                command: MenuCommand::ActivateDocument(id), label: format!("{}: {}", index + 1, name), shortcut: String::new(),
            }));
        }
        if menu.entries != entries { menu.entries = entries; self.revision = self.revision.wrapping_add(1); }
    }

    pub fn set_builtin_languages(&mut self, mut languages: Vec<(u16, String)>) {
        if !languages.iter().any(|(id, _)| *id == 57) { languages.push((57, "JSON".into())); }
        self.set_builtin_languages_with_layout(languages, true);
    }
    pub fn set_builtin_languages_with_layout(&mut self, languages: Vec<(u16, String)>, compact: bool) {
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
                MenuEntry::Submenu { selector: "language-user-defined-language", .. }
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
                    || !languages.iter().any(|(candidate, _)| *candidate == id)
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
            if letter == "K" || !compact {
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
        self.configured_popup = None;
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
            ) && !matches!(entry, MenuEntry::DynamicAction { command: MenuCommand::Source(48016), .. })
        });
        while matches!(menu.entries.last(), Some(MenuEntry::Separator)) {
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
        if !matches!(menu.entries.last(), Some(MenuEntry::Action(MenuCommand::Macro(_)))) {
            menu.entries.push(MenuEntry::Separator);
            menu.entries.push(MenuEntry::DynamicAction { command: MenuCommand::Source(48016), label: "管理宏快捷键...".into(), shortcut: String::new() });
        }
        self.revision = self.revision.wrapping_add(1);
        self.mode = MenuMode::Closed;
        self.configured_popup = None;
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
        menu.entries.push(MenuEntry::DynamicAction {
            command: MenuCommand::Language(crate::syntax::Language::UserDefined(0)),
            label: "User-Defined".into(),
            shortcut: String::new(),
        });
        self.revision = self.revision.wrapping_add(1);
        self.mode = MenuMode::Closed;
        self.configured_popup = None;
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
                .chain([MenuEntry::Separator, MenuEntry::Action(MenuCommand::Source(59003)), MenuEntry::Separator,
                    MenuEntry::Action(MenuCommand::Source(59005)), MenuEntry::Action(MenuCommand::Source(59006))])
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
                    label: "模块(P)",
                    mnemonic: 'p',
                    entries,
                },
            );
        }
        self.mode = MenuMode::Closed;
        self.configured_popup = None;
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
            documents: None,
            configured_popup: None,
            recent_range: None,
        }
    }

    pub fn set_recent_files(&mut self, paths: &[std::path::PathBuf]) {
        self.recent_range = None;
        let Some(menu) = self
            .definitions
            .iter_mut()
            .find(|menu| menu.selector == "file-menu-trigger")
        else {
            return;
        };
        let mut entries = MenuDefinition::file_menu().entries;
        if !paths.is_empty() {
            let Some(position) = entries.iter().position(|entry| {
                matches!(
                    entry,
                    MenuEntry::Action(MenuCommand::File(FileCommand::Exit))
                )
            }) else {
                return;
            };
            let mut recent = paths
                .iter()
                .take(30)
                .enumerate()
                .map(|(index, path)| MenuEntry::DynamicAction {
                    command: MenuCommand::File(FileCommand::Recent(index)),
                    label: format!("{}: {}", index + 1, path.display()),
                    shortcut: String::new(),
                })
                .collect::<Vec<_>>();
            recent.push(MenuEntry::Separator);
            recent.push(MenuEntry::Action(MenuCommand::Document(
                DocumentCommand::RestoreLastClosed,
            )));
            recent.push(MenuEntry::Action(MenuCommand::File(
                FileCommand::OpenAllRecent,
            )));
            recent.push(MenuEntry::Action(MenuCommand::File(
                FileCommand::ClearRecent,
            )));
            recent.push(MenuEntry::Separator);
            entries.splice(position..position, recent);
        }
        if menu.entries == entries {
            return;
        }
        menu.entries = entries;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn mode(&self) -> MenuMode {
        self.mode
    }
    pub fn set_recent_file_entries(&mut self, recent: Vec<MenuEntry>) {
        let Some(menu) = self.definitions.iter_mut().find(|menu| menu.selector == "file-menu-trigger") else { return; };
        if self.recent_range.as_ref().is_some_and(|range| menu.entries.get(range.clone()) == Some(recent.as_slice())) { return; }
        self.recent_range = None;
        let mut entries = MenuDefinition::file_menu().entries;
        if let Some(position) = entries.iter().position(|entry| matches!(entry, MenuEntry::Action(MenuCommand::File(FileCommand::Exit)))) {
            self.recent_range = Some(position..position + recent.len());
            entries.splice(position..position, recent);
        }
        if menu.entries != entries { menu.entries = entries; self.revision = self.revision.wrapping_add(1); }
    }
    pub fn set_terminal_servers(&mut self, servers: Vec<(u64, String)>) {
        let Some(menu) = self.definitions.iter_mut().find(|menu| menu.selector == "terminal-menu-trigger") else { return; };
        let mut entries = vec![MenuEntry::Action(MenuCommand::OpenTerminal), MenuEntry::Action(MenuCommand::TerminalSettings)];
        if !servers.is_empty() { entries.push(MenuEntry::Separator); }
        entries.extend(servers.into_iter().map(|(id, label)| MenuEntry::DynamicAction {
            command: MenuCommand::OpenRemoteTerminal(id), label, shortcut: String::new(),
        }));
        if menu.entries != entries {
            menu.entries = entries;
            self.revision = self.revision.wrapping_add(1);
        }
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
    pub fn document_menu_open(&self) -> bool {
        self.mode == MenuMode::Open && self.documents.is_some()
    }

    pub fn configured_context_menu_open(&self) -> bool {
        self.mode == MenuMode::Open && self.configured_popup.is_some()
    }

    fn root_entries(&self) -> Option<&[MenuEntry]> {
        self.configured_popup.as_deref().or(self.documents.as_deref()).or_else(|| {
            self.definitions
                .get(self.root)
                .map(|menu| menu.entries.as_slice())
        })
    }
    pub fn popup_count(&self) -> usize {
        if self.documents.is_none() && self.configured_popup.is_none()
            && self.definitions.get(self.root).and_then(MenuDefinition::direct_command).is_some()
        { return 0; }
        if self.mode == MenuMode::Open {
            self.path.len().max(1)
        } else {
            0
        }
    }

    pub fn popup_entries(&self, depth: usize) -> Option<&[MenuEntry]> {
        let mut entries = self.root_entries()?;
        for &index in self.path.get(..depth)? {
            entries = entries.get(index)?.submenu_entries()?;
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
        let mut entries = self.root_entries()?;
        for &index in parents {
            entries = entries.get(index)?.submenu_entries()?;
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
        if self.configured_popup.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        if self.documents.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
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
        let Some(entries) = self.entry_at(&self.path).and_then(MenuEntry::submenu_entries) else {
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
        if self.mode != MenuMode::Closed && self.documents.is_none() && self.configured_popup.is_none()
            && let Some(command) = self.definitions.get(self.root).and_then(MenuDefinition::direct_command)
        {
            if available(command) {
                self.handle(MenuEvent::Dismiss, available);
                return MenuOutcome::Activate(command);
            }
            return MenuOutcome::None;
        }
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
                self.documents = None;
                self.configured_popup = None;
                self.popup_limit = None;
                self.revision = self.revision.wrapping_add(1);
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
            MenuEvent::OpenContext(root, entries) => {
                if entries.is_empty() {
                    self.open_root(root, available);
                } else if root < self.definitions.len() {
                    self.configured_popup = Some(entries);
                    self.documents = None;
                    self.popup_limit = None;
                    self.revision = self.revision.wrapping_add(1);
                    self.mode = MenuMode::Open;
                    self.root = root;
                    self.path.clear();
                    self.select_edge(false, available);
                }
            }
            MenuEvent::OpenDocuments(documents) => {
                self.configured_popup = None;
                self.documents = Some(
                    documents
                        .into_iter()
                        .map(|(id, label)| MenuEntry::DynamicAction {
                            command: MenuCommand::ActivateDocument(id),
                            label,
                            shortcut: String::new(),
                        })
                        .collect(),
                );
                self.popup_limit = None;
                self.revision = self.revision.wrapping_add(1);
                self.mode = MenuMode::Open;
                self.root = 0;
                self.path = Vec::new();
                self.select_edge(false, available);
            }
            MenuEvent::ToggleBar => {
                self.configured_popup = None;
                self.documents = None;
                self.popup_limit = None;
                self.revision = self.revision.wrapping_add(1);
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
            MenuEvent::OpenRoot(root) => {
                self.open_root(root, available);
                if self.definitions.get(root).and_then(MenuDefinition::direct_command).is_some() {
                    return self.activate(available);
                }
            }
            MenuEvent::OpenSubmenu(selector) => {
                let target = self.definitions.iter().enumerate().find_map(|(root, definition)| {
                    definition.entries.iter().enumerate().find_map(|(index, entry)| {
                        (entry.submenu_selector() == Some(selector))
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
                if self.definitions.get(root).and_then(MenuDefinition::direct_command).is_some() {
                    self.open_root(root, available);
                    return self.activate(available);
                }
                if self.mode == MenuMode::Open && root == self.root && self.documents.is_none() && self.configured_popup.is_none() {
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
                } else if !self.configured_context_menu_open() {
                    self.switch_root(false, available);
                }
            }
            MenuEvent::Right => {
                if self.mode == MenuMode::Open && self.open_submenu(available) {
                } else if self.path.len() <= 1 && !self.configured_context_menu_open() {
                    self.switch_root(true, available);
                }
            }
            MenuEvent::Activate => return self.activate(available),
            MenuEvent::Escape => {
                if self.configured_context_menu_open() && self.path.len() > 1 {
                    self.path.pop();
                } else if self.document_menu_open() || self.configured_context_menu_open() {
                    self.mode = MenuMode::Closed;
                    self.documents = None;
                    self.configured_popup = None;
                    self.popup_limit = None;
                    self.revision = self.revision.wrapping_add(1);
                    self.path.clear();
                } else if self.mode == MenuMode::Open && self.path.len() > 1 {
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
                self.documents = None;
                self.configured_popup = None;
                self.popup_limit = None;
                self.revision = self.revision.wrapping_add(1);
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
                        if self.definitions[index].direct_command().is_some() {
                            return self.activate(available);
                        }
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
