use super::*;
use crate::input::LineEnding;
use crate::menu::{
    DocumentCommand, DocumentControl, MenuEntry, MultiSelectionCommand, TransformationAvailability,
};

pub(super) const DEFAULT_FONT_NAME: &str = "PingFang SC";
pub(super) const DEFAULT_FONT_SIZE: i64 = 14;
const MENU_MIN_ROW_HEIGHT: f32 = 22.;
const MENU_SEPARATOR_HEIGHT: f32 = 8.;
const MENU_GUTTER_WIDTH: f32 = 24.;
const MENU_COLUMN_GAP: f32 = 8.;
const MENU_SHORTCUT_GAP: f32 = 40.;
const MENU_ARROW_WIDTH: f32 = 6.;
const MENU_BORDER: f32 = 1.;
const MENU_MIN_WIDTH: f32 = 144.;
const MENU_ROOT_MIN_WIDTH: f32 = 192.;

pub(super) fn font_family(view: &MatrixWindow, installed: impl FnOnce() -> Vec<String>) -> String {
    let requested = view.pref_string("MenuFont.fontName", DEFAULT_FONT_NAME).trim().to_owned();
    if let Some((key, family)) = view.menu_font_family.borrow().as_ref()
        && *key == requested {
        return family.clone();
    }
    let mut names = installed();
    let system = dialog_font::system_font_family();
    names.push(system.clone());
    let fallback = names.iter().find(|name| name.eq_ignore_ascii_case(DEFAULT_FONT_NAME)).cloned().unwrap_or(system);
    let family = names.iter().find(|name| name.eq_ignore_ascii_case(&requested)).cloned().unwrap_or(fallback);
    *view.menu_font_family.borrow_mut() = Some((requested, family.clone()));
    family
}

pub(super) fn font_size(view: &MatrixWindow) -> i64 {
    let size = view.pref_int("MenuFont.size", DEFAULT_FONT_SIZE);
    if (8..=24).contains(&size) { size } else { DEFAULT_FONT_SIZE }
}

struct MenuMetrics {
    family: String,
    font: gpui::Font,
    size: gpui::Pixels,
    line_height: gpui::Pixels,
    row: gpui::Pixels,
}

impl MenuMetrics {
    fn new(view: &MatrixWindow, window: &Window) -> Self {
        let family = font_family(view, || window.text_system().all_font_names());
        let size = px(font_size(view) as f32);
        let font = gpui::font(family.clone());
        let id = window.text_system().resolve_font(&font);
        let line_height = px((f32::from(window.text_system().ascent(id, size))
            + f32::from(window.text_system().descent(id, size)).abs()).ceil());
        let row = (line_height + px(6.)).max(px(MENU_MIN_ROW_HEIGHT));
        Self { family, font, size, line_height, row }
    }
}

fn root_label<'a>(filename: &str, selector: &str, fallback: &'a str) -> &'a str {
    let id = match selector {
        "file-menu-trigger" => "file", "edit-menu-trigger" => "edit",
        "search-menu-trigger" => "search", "view-menu-trigger" => "view",
        "encoding-menu-trigger" => "encoding", "language-menu-trigger" => "language",
        "settings-menu-trigger" => "settings", "tools-menu-trigger" => "tools",
        "macro-menu-trigger" => "macro",
        "terminal-menu-trigger" => return if filename == "chineseSimplified.xml" { "终端" } else { "Terminal" },
        "plugins-menu-trigger" => return if filename == "chineseSimplified.xml" { "模块" } else { "Modules" },
        "window-menu-trigger" => "Window",
        _ => return fallback,
    };
    crate::localization::menu_root(filename, id).unwrap_or(fallback)
}

fn entry_label<'a>(filename: &str, entry: &'a MenuEntry, configured: bool) -> &'a str {
    match entry {
        MenuEntry::Action(MenuCommand::TerminalSettings) => if filename == "chineseSimplified.xml" { "终端配置..." } else { "Terminal Configuration..." },
        MenuEntry::Action(command) if command.source_id().and_then(|id| crate::menu::product_command_label(id, filename)).is_some() => {
            command.source_id().and_then(|id| crate::menu::product_command_label(id, filename)).unwrap_or_else(|| entry.label())
        }
        MenuEntry::Action(command) => command.source_id().and_then(|id| crate::localization::menu_command(filename, id)).unwrap_or_else(|| entry.label()),
        MenuEntry::DynamicAction { command: MenuCommand::Source(id @ 48016), .. } if !configured => crate::localization::menu_command(filename, *id).unwrap_or_else(|| entry.label()),
        MenuEntry::DynamicAction { command: MenuCommand::Language(crate::syntax::Language::UserDefined(0)), .. } if !configured => crate::localization::menu_command(filename, 46180).unwrap_or_else(|| entry.label()),
        MenuEntry::Submenu { selector, .. } => {
            let id = match *selector {
                "file-open-folder" => "file-openFolder", "file-menu-close-multiple" => "file-closeMore",
                "file-recent-files" => "file-recentFiles", "edit-date-time" => "edit-insert",
                "edit-copy-to-clipboard" => "edit-copyToClipboard", "edit-indent" => "edit-indent",
                "edit-convert-case" => "edit-convertCaseTo", "edit-line-operations" => "edit-lineOperations",
                "edit-comment" => "edit-comment", "edit-menu-auto-completion" => "edit-autoCompletion",
                "edit-menu-eol" => "edit-eolConversion", "edit-blank" => "edit-blankOperations",
                "edit-paste-special" => "edit-pasteSpecial", "edit-on-selection" => "edit-onSelection",
                "edit-multiselect-all" => "edit-multiSelectALL", "edit-multiselect-next" => "edit-multiSelectNext",
                "edit-read-only" => "edit-readonlyInNotepad++", "search-change-history-menu" => "search-changeHistory",
                "search-style-all" => "search-markAll", "search-style-one" => "search-markOne",
                "search-style-clear" => "search-unmarkAll", "search-style-previous" => "search-jumpUp",
                "search-style-next" => "search-jumpDown", "search-style-copy" => "search-copyStyledText",
                "search-bookmark-menu" => "search-bookmark", "view-show-symbol-menu" => "view-showSymbol",
                "view-zoom-menu" => "view-zoom", "view-move-clone-menu" => "view-moveCloneDocument",
                "view-tab-menu" => "view-tab", "view-project-panels" => "view-project",
                "view-browser-menu" => "view-currentFileIn", "view-fold-level" => "view-collapseLevel",
                "view-unfold-level" => "view-uncollapseLevel", "encoding-character-sets" => "encoding-characterSets",
                "language-user-defined-language" => "language-userDefinedLanguage",
                "encoding-arabic" => "encoding-arabic", "encoding-baltic" => "encoding-baltic",
                "encoding-celtic" => "encoding-celtic", "encoding-cyrillic" => "encoding-cyrillic",
                "encoding-central-european" => "encoding-centralEuropean", "encoding-chinese" => "encoding-chinese",
                "encoding-eastern-european" => "encoding-easternEuropean", "encoding-greek" => "encoding-greek",
                "encoding-hebrew" => "encoding-hebrew", "encoding-japanese" => "encoding-japanese",
                "encoding-korean" => "encoding-korean", "encoding-north-european" => "encoding-northEuropean",
                "encoding-thai" => "encoding-thai", "encoding-turkish" => "encoding-turkish",
                "encoding-western-european" => "encoding-westernEuropean", "encoding-vietnamese" => "encoding-vietnamese",
                "settings-import-submenu" => "settings-import", "tools-md5-menu" => "tools-md5",
                "tools-sha1-menu" => "tools-sha1", "tools-sha256-menu" => "tools-sha256",
                "tools-sha512-menu" => "tools-sha512", "window-sort-tabs" => "window-sortby",
                _ => return entry.label(),
            };
            crate::localization::menu_submenu(filename, id).unwrap_or_else(|| entry.label())
        }
        _ => entry.label(),
    }
}

fn localize_context_menu(xml: &str, filename: &str, definitions: &[crate::menu::MenuDefinition]) -> io::Result<Vec<crate::context_menu::ContextMenuEntry>> {
    fn source_by_name(entries: &[MenuEntry], name: &str) -> Option<u32> {
        entries.iter().find_map(|entry| {
            if let Some(id) = entry.command().and_then(MenuCommand::source_id)
                && crate::localization::menu_command("english.xml", id).is_some_and(|english| english.eq_ignore_ascii_case(name)) {
                Some(id)
            } else { entry.submenu_entries().and_then(|entries| source_by_name(entries, name)) }
        })
    }
    let mut document = crate::style_config::Node::parse(xml)?;
    if let Some(root) = document.children.iter_mut().find(|node| node.name == "ScintillaContextMenu") {
        for item in root.children.iter_mut().filter(|item| item.name == "Item") {
            if !item.attribute("ItemNameAs").is_some_and(|label| !label.is_empty()) {
                let id = item.attribute("id").and_then(|id| id.parse::<u32>().ok()).or_else(|| {
                    let menu = item.attribute("MenuEntryName")?;
                    let name = item.attribute("MenuItemName")?;
                    definitions.iter().find(|root| root_label("english.xml", root.selector, if root.selector == "help-menu-trigger" { "?" } else { root.label }).eq_ignore_ascii_case(menu))
                        .and_then(|root| source_by_name(&root.entries, name))
                });
                if let Some(label) = id.and_then(|id| crate::menu::product_command_label(id, filename).or_else(|| crate::localization::menu_command(filename, id))) {
                    item.attributes.insert("ItemNameAs".into(), label.to_owned());
                }
            }
            if let Some(label) = item.attribute("TranslateID").and_then(|id| crate::localization::localized_string(filename, id)) {
                item.attributes.insert("FolderName".into(), label.to_owned());
                item.attributes.remove("TranslateID");
            }
        }
    }
    Ok(crate::context_menu::ContextMenu::from_document(document)?.entries(definitions))
}

impl MatrixWindow {
    pub(super) fn symbol_menu_open(&self) -> bool {
        self.menu_anchor.is_some()
            && self.menu_submenu_only
            && self
                .menu_bar
                .popup_entries(0)
                .and_then(|entries| {
                    self.menu_bar
                        .selected_path()
                        .first()
                        .and_then(|index| entries.get(*index))
                })
                .is_some_and(|entry| {
                    matches!(
                        entry,
                        MenuEntry::Submenu {
                            selector: "view-show-symbol-menu",
                            ..
                        }
                    )
                })
    }

    pub(super) fn open_symbol_dropdown(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.symbol_menu_open() {
            self.menu_input(MenuEvent::Dismiss, window, cx);
            return;
        }
        self.menu_input(
            MenuEvent::OpenSubmenuLimited {
                selector: "view-show-symbol-menu",
                limit: 6,
            },
            window,
            cx,
        );
        if self.menu_bar.mode() == MenuMode::Open {
            self.menu_anchor = Some(position);
            self.menu_submenu_only = true;
            self.symbol_menu_selection_visible = false;
            cx.notify();
        }
    }

    pub(super) fn open_document_dropdown(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !availability(self, window, cx).allows(MenuCommand::DocumentDropdown)
            || self.reload_target.is_some()
        {
            return;
        }
        if self.menu_bar.document_menu_open() {
            self.menu_input(MenuEvent::Dismiss, window, cx);
            return;
        }
        let documents = self
            .workspace
            .view_buffer_ids(self.workspace.active_view())
            .iter()
            .filter_map(|id| {
                self.workspace
                    .buffers()
                    .iter()
                    .find(|buffer| buffer.id() == *id)
            })
            .map(|buffer| {
                (
                    buffer.id(),
                    format!(
                        "{}{}",
                        buffer.name(),
                        if buffer.is_modified() { "*" } else { "" }
                    ),
                )
            })
            .collect();
        self.menu_input(MenuEvent::OpenDocuments(documents), window, cx);
        self.menu_anchor = Some(position);
        self.menu_submenu_only = false;
        cx.notify();
    }

    pub(super) fn show_editor_context_menu(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let Some(root) = self
            .menu_bar
            .definitions()
            .iter()
            .position(|definition| definition.selector == "edit-menu-trigger")
        else {
            return;
        };
        let context_entries = match localize_context_menu(&self.context_menu.xml(), &self.native_locale(), self.menu_bar.definitions()) {
            Ok(entries) => entries,
            Err(error) => { eprintln!("Context menu localization cannot be read: {error}"); self.editor_context_menu_entries() },
        };
        let entries = crate::context_menu::into_menu_entries(
            context_entries, self.menu_bar.definitions(),
        );
        self.menu_input(MenuEvent::OpenContext(root, entries), window, cx);
        if self.menu_bar.mode() == MenuMode::Open {
            self.menu_anchor = Some(position);
            self.menu_submenu_only = false;
            cx.notify();
        }
    }
}

pub(super) fn bindings() -> [KeyBinding; 22] {
    [
        KeyBinding::new("f10", ToggleMenuBar, None),
        KeyBinding::new("alt-f", OpenFileMenu, None),
        KeyBinding::new("alt-e", OpenEditMenu, None),
        KeyBinding::new("alt-s", OpenSearchMenu, None),
        KeyBinding::new("alt-v", OpenViewMenu, None),
        KeyBinding::new("alt-w", OpenWindowMenu, None),
        KeyBinding::new("alt-h", OpenHelpMenu, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        KeyBinding::new("alt-r", ShowTerminal, None),
        KeyBinding::new("alt-o", OpenToolsMenu, None),
        KeyBinding::new("alt-m", OpenMacroMenu, None),
        KeyBinding::new("alt-l", OpenLanguageMenu, None),
        KeyBinding::new("alt-t", OpenSettingsMenu, None),
        KeyBinding::new("alt-n", OpenEncodingMenu, None),
        KeyBinding::new("up", MenuUp, Some("MatrixMenu")),
        KeyBinding::new("down", MenuDown, Some("MatrixMenu")),
        KeyBinding::new("left", MenuLeft, Some("MatrixMenu")),
        KeyBinding::new("right", MenuRight, Some("MatrixMenu")),
        KeyBinding::new("enter", MenuEnter, Some("MatrixMenu")),
        KeyBinding::new("escape", MenuEscape, Some("MatrixMenu")),
        KeyBinding::new("home", MenuFirst, Some("MatrixMenu")),
        KeyBinding::new("end", MenuLast, Some("MatrixMenu")),
    ]
}

#[derive(Clone, Copy)]
pub(super) struct Availability {
    dirty: bool,
    any_dirty: bool,
    recent_count: usize,
    file_exists: bool,
    default_viewer: bool,
    selection: bool,
    line_copy_cut_without_selection: bool,
    finder_copy_available: bool,
    editable: bool,
    line_ending: LineEnding,
    undo: bool,
    redo: bool,
    paste: bool,
    has_disk_file: bool,
    clean_saved: bool,
    macro_recording: bool,
    macro_ready: bool,
    macro_save_ready: bool,
    macro_repeat_ready: bool,
    saved_macro_count: usize,
    saved_macros_idle: bool,
    split_views: bool,
    transformations: TransformationAvailability,
    navigation_ready: bool,
    restore_closed: bool,
    multiple_selections: bool,
    document_switcher: bool,
    brace_match: bool,
    find_created: bool,
    finder_created: bool,
    begin_selection_mode: Option<bool>,
    user_read_only_available: bool,
    all_read_only_available: bool,
    encoding_disabled: u128,
    view_disabled: u128,
}

impl Availability {
    pub(super) fn allows(self, command: MenuCommand) -> bool {
        match command {
            MenuCommand::Source(id) => {
                self.navigation_ready
                    && (crate::menu::source_command(id).is_some() || id == 48016)
                    && (!matches!(
                        id,
                        42084..=42086
                    ) || self.editable)
                    && (id != 42033 || self.file_exists)
                    && (id != 42028 || self.user_read_only_available)
                    && (!matches!(id, 42102 | 42103) || self.all_read_only_available)
                    && (!matches!(id, 42020 | 42089)
                        || self.begin_selection_mode.is_none_or(|column| column == (id == 42089)))
                    && (!matches!(id, 43009 | 43053) || self.brace_match)
                    && (!matches!(id, 43045..=43047) || self.finder_created)
                    && !matches!(id, 45004 | 45009)
                    && (!matches!(id, 48503 | 48506 | 48509 | 48512)
                        || (self.selection && !self.multiple_selections))
                    && (!matches!(id, 44100..=44103) || self.file_exists)
                    && match id {
                        44000..=44099 => self.view_disabled & (1u128 << (id - 44000)) == 0,
                        45000..=45100 => self.encoding_disabled & (1u128 << (id - 45000)) == 0,
                        _ => true,
                    }
            }
            MenuCommand::File(FileCommand::Save) => self.navigation_ready && self.dirty,
            MenuCommand::File(FileCommand::SaveAll) => self.navigation_ready && self.any_dirty,
            MenuCommand::File(
                FileCommand::Reveal
                | FileCommand::Terminal
                | FileCommand::Trash
                | FileCommand::Reload
                | FileCommand::ContainingWorkspace,
            ) => self.navigation_ready && self.file_exists,
            MenuCommand::File(FileCommand::DefaultViewer) => {
                self.navigation_ready && self.file_exists && self.default_viewer
            }
            MenuCommand::File(FileCommand::Recent(index)) => {
                self.navigation_ready && index < self.recent_count
            }
            MenuCommand::File(FileCommand::OpenAllRecent | FileCommand::ClearRecent) => {
                self.navigation_ready && self.recent_count > 0
            }
            MenuCommand::File(FileCommand::CloseLeft | FileCommand::CloseRight) => {
                self.navigation_ready
            }
            MenuCommand::ActivateDocument(_) | MenuCommand::DocumentDropdown => {
                self.navigation_ready
            }
            MenuCommand::Binary(crate::clipboard::BinaryCommand::Copy) => self.navigation_ready,
            MenuCommand::Binary(crate::clipboard::BinaryCommand::Cut | crate::clipboard::BinaryCommand::Paste) => self.navigation_ready,
            MenuCommand::CharacterPanel => self.navigation_ready,
            MenuCommand::CopyPath(_) => self.navigation_ready,
            MenuCommand::Intelligence(_) => self.navigation_ready,
            MenuCommand::TextTransform(id) => self.transformations.allows(id)
                && (!matches!(id, 42016 | 42017 | 42067..=42072 | 42106) || self.selection)
                && (id != 42106 || self.editable),
            MenuCommand::ColumnEditor => self.navigation_ready,
            MenuCommand::MultiSelection(MultiSelectionCommand::Rotate) => self.navigation_ready && self.multiple_selections,
            MenuCommand::MultiSelection(_) => self.navigation_ready,
            MenuCommand::Document(DocumentCommand::RestoreLastClosed) => {
                self.navigation_ready && self.restore_closed
            }
            MenuCommand::Document(DocumentCommand::Color(Some(color))) => {
                self.navigation_ready && color <= 4
            }
            MenuCommand::Document(DocumentCommand::Mru) => {
                self.navigation_ready && self.document_switcher
            }
            MenuCommand::Document(_) => self.navigation_ready,
            MenuCommand::View(ViewCommand::ProjectPanel(index)) => index < 3,
            MenuCommand::View(ViewCommand::MoveToNewInstance | ViewCommand::OpenInNewInstance) => {
                self.navigation_ready && self.clean_saved
            }
            MenuCommand::Search(SearchCommand::Next | SearchCommand::Previous) => self.navigation_ready && self.find_created,
            MenuCommand::Search(_) => self.navigation_ready,
            MenuCommand::Bookmark(_) => self.navigation_ready,
            MenuCommand::ChangeHistory(_) => self.navigation_ready,
            MenuCommand::View(ViewCommand::Monitoring) => self.has_disk_file,
            MenuCommand::View(ViewCommand::MarkdownPreview) => self.navigation_ready,
            MenuCommand::View(ViewCommand::SyncVertical | ViewCommand::SyncHorizontal) => {
                self.split_views
            }
            MenuCommand::View(_) => true,
            MenuCommand::Settings(_) => true,
            MenuCommand::Mime(_) => true,
            MenuCommand::Converter(_) => true,
            MenuCommand::ExportHtml => true,
            MenuCommand::Window(_) => true,
            MenuCommand::About => true,
            MenuCommand::OpenTerminal | MenuCommand::OpenRemoteTerminal(_) | MenuCommand::TerminalSettings => true,
            MenuCommand::HashSelection(_) => true,
            MenuCommand::Language(_) => true,
            MenuCommand::DefineUserLanguage => true,
            MenuCommand::Macro(crate::macros::MacroCommand::StartRecording) => {
                !self.macro_recording
            }
            MenuCommand::Macro(crate::macros::MacroCommand::StopRecording) => self.macro_recording,
            MenuCommand::Macro(crate::macros::MacroCommand::Playback) => {
                !self.macro_recording && self.macro_ready
            }
            MenuCommand::Macro(crate::macros::MacroCommand::SaveCurrent) => self.macro_save_ready,
            MenuCommand::Macro(crate::macros::MacroCommand::RunMultiple) => self.macro_repeat_ready,
            MenuCommand::SavedMacro(index) => {
                self.saved_macros_idle && index < self.saved_macro_count
            }
            MenuCommand::File(_) | MenuCommand::Edit(EditCommand::SelectAll) => true,
            MenuCommand::SelectEncoding(_) | MenuCommand::ConvertEncoding(_) => true,
            MenuCommand::Edit(EditCommand::Undo) => self.editable && self.undo,
            MenuCommand::Edit(EditCommand::Redo) => self.editable && self.redo,
            MenuCommand::Edit(EditCommand::Cut) => self.navigation_ready && (self.selection || self.line_copy_cut_without_selection),
            MenuCommand::Edit(EditCommand::Copy) => self.selection || self.line_copy_cut_without_selection || self.finder_copy_available,
            MenuCommand::Edit(EditCommand::Paste) => self.editable && self.paste,
            MenuCommand::Edit(EditCommand::Delete) => self.navigation_ready && self.selection,
            MenuCommand::Edit(EditCommand::ConvertEol(target)) => {
                self.editable && target != self.line_ending
            }
            MenuCommand::Edit(EditCommand::DuplicateLine) => self.editable,
        }
    }
}

pub(super) fn availability(view: &MatrixWindow, window: &Window, cx: &App) -> Availability {
    let document = view.editor().document();
    let navigation_ready = !view.file_busy
        && !view.close_dialog
        && !view.save_all_dialog
        && !view.batch_editing_blocked();
    let language = view.workspace.active().language();
    let definition = match language {
        crate::syntax::Language::UserDefined(id) => view.udl.registry.definition(id),
        _ => None,
    };
    let syntax = text_transform_surface::comment_syntax(language, definition).ok();
    let disabled_sources = |base: u32, last: u32| (base..=last)
        .filter(|id| view.menu_source_enabled(*id) == Some(false))
        .fold(0u128, |bits, id| bits | (1u128 << (id - base)));
    Availability {
        dirty: view.workspace.active().is_modified(),
        any_dirty: !view.workspace.dirty_buffers().is_empty(),
        recent_count: view.workspace.recently_closed().len(),
        file_exists: view.disk_file().is_some_and(|file| file.path().is_file()),
        default_viewer: view.disk_file().is_some_and(|file| {
            file.path().is_file() && file_menu_surface::has_default_viewer(file.path())
        }),
        transformations: TransformationAvailability {
            editable: navigation_ready,
            line_comment: syntax.as_ref().is_some_and(|syntax| syntax.line.is_some() || syntax.stream.is_some()),
            block_comment: syntax
                .as_ref()
                .is_some_and(|syntax| syntax.stream.is_some() || syntax.line.is_some()),
        },
        navigation_ready,
        restore_closed: !view.workspace.recently_closed().is_empty(),
        multiple_selections: view.editor().display_selections().ranges().len() > 1,
        document_switcher: view.document_switcher,
        selection: view.editor().selections().ranges().iter().any(|range| !range.is_empty()),
        line_copy_cut_without_selection: view.line_copy_cut_without_selection,
        finder_copy_available: view.finder_copy_available(window),
        brace_match: view.matching_brace_positions().is_some(),
        find_created: view.find_created,
        finder_created: view.search_results_created(),
        begin_selection_mode: document.begin_selection_mode(),
        user_read_only_available: !view.monitoring_active()
            && !view.disk_file().is_some_and(|file| file.is_read_only()),
        all_read_only_available: !view.monitoring_active(),
        encoding_disabled: disabled_sources(45000, 45100),
        view_disabled: disabled_sources(44000, 44099),
        editable: !document.is_read_only(),
        line_ending: view.editor().line_ending(),
        undo: document.can_undo(),
        redo: document.can_redo(),
        paste: clipboard_surface::has_text(cx),
        has_disk_file: view.disk_file().is_some(),
        clean_saved: view.disk_file().is_some() && !view.workspace.active().is_modified(),
        macro_recording: view.macro_recording,
        macro_ready: !view.macro_steps.is_empty(),
        macro_save_ready: view.current_repeat_macro_available(),
        macro_repeat_ready: !view.macro_recording
            && (!view.macro_steps.is_empty() || !view.macro_dialogs.library.macros.is_empty()),
        saved_macro_count: view.macro_dialogs.library.macros.len(),
        split_views: view.split_views_visible(),
        saved_macros_idle: !view.macro_recording
            && !view.macro_playing
            && !view.file_busy
            && !view.close_dialog
            && !view.save_all_dialog
            && !view.batch_editing_blocked(),
    }
}

fn selector(command: MenuCommand) -> String {
    match command {
        MenuCommand::Source(id) => {
            crate::menu::source_command(id).map_or("unknown-source-command", |value| value.2)
        }
        MenuCommand::File(FileCommand::Recent(index)) => {
            return format!("file-menu-recent-{index}");
        }
        MenuCommand::ActivateDocument(id) => return format!("document-dropdown-item-{}", id.0),
        MenuCommand::DocumentDropdown => "document-dropdown",
        MenuCommand::Binary(command) => return format!("edit-binary-{}", command.source_id()),
        MenuCommand::CharacterPanel => "edit-character-panel",
        MenuCommand::CopyPath(command) => return format!("edit-copy-path-{}", command.source_id()),
        MenuCommand::Intelligence(command) => {
            return format!("edit-intelligence-{}", command.source_id());
        }
        MenuCommand::TextTransform(42010) => "edit-duplicate-line",
        MenuCommand::TextTransform(id) => return format!("edit-transform-{id}"),
        MenuCommand::MultiSelection(command) => {
            return format!("edit-multiselect-{}", command.source_id());
        }
        MenuCommand::ColumnEditor => "edit-column-editor",
        MenuCommand::Document(command) => {
            return format!(
                "document-command-{}",
                command
                    .source_id()
                    .map_or_else(|| "invalid".into(), |id| id.to_string())
            );
        }
        MenuCommand::View(ViewCommand::ProjectPanel(index)) => {
            return format!("view-project-panel-{}", index + 1);
        }
        MenuCommand::Window(crate::menu::WindowCommand::Minimize) => "window-menu-minimize",
        MenuCommand::Window(crate::menu::WindowCommand::Zoom) => "window-menu-zoom",
        MenuCommand::Window(crate::menu::WindowCommand::BringAllToFront) => {
            "window-menu-bring-all-to-front"
        }
        MenuCommand::About => "help-menu-about",
        MenuCommand::OpenTerminal => "terminal-open",
        MenuCommand::OpenRemoteTerminal(id) => return format!("terminal-server-{id}"),
        MenuCommand::TerminalSettings => "terminal-settings",
        MenuCommand::Language(crate::syntax::Language::NormalText) => "language-normal-text",
        MenuCommand::Language(crate::syntax::Language::Json) => "language-json",
        MenuCommand::Language(crate::syntax::Language::Builtin(0)) => "language-normal-text",
        MenuCommand::Language(crate::syntax::Language::Builtin(57)) => "language-json",
        MenuCommand::Language(crate::syntax::Language::Builtin(id)) => {
            return format!("language-builtin-{id}");
        }
        MenuCommand::Language(crate::syntax::Language::UserDefined(id)) => {
            return format!("language-udl-{id}");
        }
        MenuCommand::DefineUserLanguage => "language-define-user",
        MenuCommand::Macro(crate::macros::MacroCommand::StartRecording) => "macro-start",
        MenuCommand::Macro(crate::macros::MacroCommand::StopRecording) => "macro-stop",
        MenuCommand::Macro(crate::macros::MacroCommand::Playback) => "macro-playback",
        MenuCommand::Macro(crate::macros::MacroCommand::SaveCurrent) => "macro-save-current",
        MenuCommand::Macro(crate::macros::MacroCommand::RunMultiple) => "macro-run-multiple",
        MenuCommand::SavedMacro(index) => return format!("macro-saved-{index}"),
        MenuCommand::HashSelection(crate::hashes::HashAlgorithm::Md5) => "hash-copy-md5",
        MenuCommand::HashSelection(crate::hashes::HashAlgorithm::Sha1) => "hash-copy-sha1",
        MenuCommand::HashSelection(crate::hashes::HashAlgorithm::Sha256) => "hash-copy-sha256",
        MenuCommand::HashSelection(crate::hashes::HashAlgorithm::Sha512) => "hash-copy-sha512",
        MenuCommand::ExportHtml => "npp-export-html",
        MenuCommand::Converter(crate::converter::ConverterOperation::AsciiToHex) => {
            "converter-ascii-to-hex"
        }
        MenuCommand::Converter(crate::converter::ConverterOperation::HexToAscii) => {
            "converter-hex-to-ascii"
        }
        MenuCommand::View(ViewCommand::DocumentList) => "view-document-list",
        MenuCommand::View(ViewCommand::FolderWorkspace) => "view-folder-workspace",
        MenuCommand::View(ViewCommand::FunctionList) => "view-function-list",
        MenuCommand::View(ViewCommand::Monitoring) => "view-monitoring",
        MenuCommand::View(ViewCommand::SyncVertical) => "view-sync-vertical",
        MenuCommand::View(ViewCommand::SyncHorizontal) => "view-sync-horizontal",
        MenuCommand::View(ViewCommand::CloneToOtherView) => "view-clone-other",
        MenuCommand::View(ViewCommand::MoveToOtherView) => "view-move-other",
        MenuCommand::View(ViewCommand::MoveToNewInstance) => "view-move-new-instance",
        MenuCommand::View(ViewCommand::OpenInNewInstance) => "view-open-new-instance",
        MenuCommand::View(ViewCommand::DocumentMap) => "view-document-map",
        MenuCommand::View(ViewCommand::Whitespace) => "view-whitespace",
        MenuCommand::View(ViewCommand::EndOfLine) => "view-eol",
        MenuCommand::View(ViewCommand::Nonprinting) => "view-nonprinting",
        MenuCommand::View(ViewCommand::ControlCharacters) => "view-control-characters",
        MenuCommand::View(ViewCommand::AllCharacters) => "view-all-characters",
        MenuCommand::View(ViewCommand::IndentGuides) => "view-indent-guides",
        MenuCommand::View(ViewCommand::ZoomIn) => "view-zoom-in",
        MenuCommand::View(ViewCommand::ZoomOut) => "view-zoom-out",
        MenuCommand::View(ViewCommand::ZoomReset) => "view-zoom-reset",
        MenuCommand::View(ViewCommand::WordWrap) => "view-word-wrap",
        MenuCommand::View(ViewCommand::Summary) => "view-summary",
        MenuCommand::View(ViewCommand::MarkdownPreview) => "view-markdown-preview",
        MenuCommand::ChangeHistory(crate::menu::ChangeHistoryCommand::Next) => {
            "change-history-next"
        }
        MenuCommand::ChangeHistory(crate::menu::ChangeHistoryCommand::Previous) => {
            "change-history-previous"
        }
        MenuCommand::ChangeHistory(crate::menu::ChangeHistoryCommand::Clear) => {
            "change-history-clear"
        }
        MenuCommand::Search(SearchCommand::Find) => "search-menu-find",
        MenuCommand::Bookmark(BookmarkCommand::Toggle) => "bookmark-toggle",
        MenuCommand::Bookmark(BookmarkCommand::Next) => "bookmark-next",
        MenuCommand::Bookmark(BookmarkCommand::Previous) => "bookmark-previous",
        MenuCommand::Bookmark(BookmarkCommand::ClearAll) => "bookmark-clear-all",
        MenuCommand::Search(SearchCommand::FindInFiles) => "search-menu-find-in-files",
        MenuCommand::Search(SearchCommand::Next) => "search-menu-next",
        MenuCommand::Search(SearchCommand::Previous) => "search-menu-previous",
        MenuCommand::Search(SearchCommand::GoTo) => "search-menu-goto",
        MenuCommand::Settings(SettingsCommand::Preferences) => "settings-menu-preferences",
        MenuCommand::Settings(SettingsCommand::StyleConfigurator) => "settings-menu-style-configurator",
        MenuCommand::Settings(SettingsCommand::ShortcutMapper) => "settings-menu-shortcut-mapper",
        MenuCommand::Settings(SettingsCommand::ImportStyleThemes) => "settings-menu-import-style-themes",
        MenuCommand::Settings(SettingsCommand::EditContextMenu) => "settings-menu-edit-context-menu",
        MenuCommand::Mime(crate::mime::MimeOperation::Base64Encode) => "mime-base64-encode",
        MenuCommand::Mime(crate::mime::MimeOperation::Base64EncodePadded) => {
            "mime-base64-encode-padded"
        }
        MenuCommand::Mime(crate::mime::MimeOperation::Base64Decode) => "mime-base64-decode",
        MenuCommand::Search(SearchCommand::Replace) => "search-menu-replace",
        MenuCommand::ConvertEncoding(crate::files::UnicodeFormat::Utf8(
            crate::files::Utf8Format::Plain,
        )) => "encoding-convert-utf8",
        MenuCommand::ConvertEncoding(crate::files::UnicodeFormat::Utf8(
            crate::files::Utf8Format::Bom,
        )) => "encoding-convert-utf8-bom",
        MenuCommand::ConvertEncoding(crate::files::UnicodeFormat::Utf16BeBom) => {
            "encoding-convert-utf16-be"
        }
        MenuCommand::ConvertEncoding(crate::files::UnicodeFormat::Utf16LeBom) => {
            "encoding-convert-utf16-le"
        }
        MenuCommand::SelectEncoding(crate::files::UnicodeFormat::Utf8(
            crate::files::Utf8Format::Plain,
        )) => "encoding-utf8",
        MenuCommand::SelectEncoding(crate::files::UnicodeFormat::Utf8(
            crate::files::Utf8Format::Bom,
        )) => "encoding-utf8-bom",
        MenuCommand::SelectEncoding(crate::files::UnicodeFormat::Utf16BeBom) => "encoding-utf16-be",
        MenuCommand::SelectEncoding(crate::files::UnicodeFormat::Utf16LeBom) => "encoding-utf16-le",
        MenuCommand::File(command) => match command {
            FileCommand::New => "file-menu-new",
            FileCommand::Open => "file-menu-open",
            FileCommand::Save => "file-menu-save",
            FileCommand::SaveAs => "file-menu-save-as",
            FileCommand::SaveCopy => "file-menu-save-copy",
            FileCommand::SaveAll => "file-menu-save-all",
            FileCommand::Close => "file-menu-close",
            FileCommand::CloseAll => "file-menu-close-all",
            FileCommand::Reload => "file-menu-reload",
            FileCommand::Exit => "file-menu-exit",
            FileCommand::CloseLeft => "file-menu-close-left",
            FileCommand::CloseRight => "file-menu-close-right",
            FileCommand::Print => "file-menu-print",
            FileCommand::PrintNow => "file-menu-print-now",
            FileCommand::Reveal => "file-menu-reveal",
            FileCommand::Terminal => "file-menu-terminal",
            FileCommand::ContainingWorkspace => "file-menu-containing-workspace",
            FileCommand::FolderWorkspace => "file-menu-folder-workspace",
            FileCommand::DefaultViewer => "file-menu-default-viewer",
            FileCommand::Rename => "file-menu-rename",
            FileCommand::Trash => "file-menu-trash",
            FileCommand::OpenAllRecent => "file-menu-open-all-recent",
            FileCommand::ClearRecent => "file-menu-clear-recent",
            FileCommand::Recent(_) => "file-menu-recent",
        },
        MenuCommand::Edit(EditCommand::Undo) => "edit-menu-undo",
        MenuCommand::Edit(EditCommand::Redo) => "edit-menu-redo",
        MenuCommand::Edit(EditCommand::Cut) => "edit-menu-cut",
        MenuCommand::Edit(EditCommand::Copy) => "edit-menu-copy",
        MenuCommand::Edit(EditCommand::Paste) => "edit-menu-paste",
        MenuCommand::Edit(EditCommand::Delete) => "edit-menu-delete",
        MenuCommand::Edit(EditCommand::SelectAll) => "edit-menu-select-all",
        MenuCommand::Edit(EditCommand::DuplicateLine) => "edit-duplicate-line",
        MenuCommand::Edit(EditCommand::ConvertEol(LineEnding::CrLf)) => "edit-eol-crlf",
        MenuCommand::Edit(EditCommand::ConvertEol(LineEnding::Lf)) => "edit-eol-lf",
        MenuCommand::Edit(EditCommand::ConvertEol(LineEnding::Cr)) => "edit-eol-cr",
    }
    .into()
}

fn row_height(entry: &MenuEntry, metrics: &MenuMetrics) -> gpui::Pixels {
    if matches!(entry, MenuEntry::Separator) {
        px(MENU_SEPARATOR_HEIGHT)
    } else {
        metrics.row
    }
}

fn radio_mark(command: Option<MenuCommand>) -> bool {
    matches!(command, Some(MenuCommand::Language(_) | MenuCommand::SelectEncoding(_) | MenuCommand::Source(45000..=45100)))
}

fn submenu_radio_checked(view: &MatrixWindow, entry: &MenuEntry) -> Option<bool> {
    let mut checked = None;
    for child in entry.submenu_entries()? {
        let value = match child.command() {
            Some(MenuCommand::Language(language)) => Some(view.workspace.active().language().key() == language.key()),
            Some(MenuCommand::SelectEncoding(target)) => Some(target == view.workspace.active().format()),
            Some(MenuCommand::Source(id @ 45000..=45100)) => view.menu_source_checked(id),
            _ => submenu_radio_checked(view, child),
        };
        if let Some(value) = value {
            checked = Some(checked.unwrap_or(false) || value);
        }
    }
    checked
}

fn popup_max_height(anchored: bool, viewport: gpui::Size<gpui::Pixels>, metrics: &MenuMetrics) -> gpui::Pixels {
    (viewport.height - if anchored { px(0.) } else { px(32.) + metrics.row })
        .max(metrics.row + px(MENU_BORDER * 2.))
}

pub(super) fn scroll_selected(view: &mut MatrixWindow, window: &Window) {
    let metrics = MenuMetrics::new(view, window);
    while view.menu_scroll.len() < view.menu_bar.popup_count() {
        view.menu_scroll.push(gpui::ScrollHandle::new());
    }
    let max_height = popup_max_height(view.menu_anchor.is_some(), window.viewport_size(), &metrics);
    for (depth, index) in view.menu_bar.selected_path().iter().enumerate() {
        let Some(entries) = view.menu_bar.popup_entries(depth) else { continue; };
        let Some(entry) = entries.get(*index) else { continue; };
        let content_height = entries.iter().map(|entry| row_height(entry, &metrics)).sum::<gpui::Pixels>();
        let visible_height = content_height.min(max_height - px(MENU_BORDER * 2.));
        let item_top = entries.iter().take(*index).map(|entry| row_height(entry, &metrics)).sum::<gpui::Pixels>();
        let scroll = &view.menu_scroll[depth];
        let mut offset = scroll.offset();
        offset.y = offset.y.max(visible_height - content_height).min(px(0.));
        if item_top + offset.y < px(0.) {
            offset.y = -item_top;
        } else if item_top + row_height(entry, &metrics) + offset.y > visible_height {
            offset.y = visible_height - item_top - row_height(entry, &metrics);
        }
        scroll.set_offset(offset);
    }
}

pub(super) fn render(
    view: &mut MatrixWindow,
    window: &Window,
    cx: &mut Context<MatrixWindow>,
) -> AnyElement {
    let metrics = MenuMetrics::new(view, window);
    let mode = view.menu_bar.mode();
    let filename = view.native_locale();
    if !view.pref_bool("MenuBar.visible", true) && mode == MenuMode::Closed { return div().into_any_element(); }
    let root = view.menu_bar.active_root();
    let open = mode == MenuMode::Open;
    if open && view.menu_bar.definitions().get(root).is_some_and(|menu| menu.selector == "search-menu-trigger") {
        view.refresh_builtin_syntax();
    }
    let available = availability(view, window, cx);
    let revision = view.menu_bar.revision();
    let document_controls_enabled = !view.file_busy
        && !view.close_dialog
        && !view.save_all_dialog
        && view.reload_target.is_none()
        && !view.batch_editing_blocked();
    let dark = view.dark;
    let document_controls = DocumentControl::ALL
        .into_iter()
        .filter(|_| !view.menu_right_shortcuts_hidden)
        .map(|control| {
            let id = match control {
                DocumentControl::New => "menu-new-document",
                DocumentControl::DocumentList => "menu-document-list",
                DocumentControl::Close => "menu-close-document",
            };
            let enabled = document_controls_enabled && available.allows(control.command());
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(control.label())
                .tooltip(move |_, cx| icon_tooltip::view(control.label(), dark, cx))
                .a11y_synthetic_children(move |builder| {
                    if !enabled {
                        builder.parent_node().set_disabled();
                    }
                })
                .when(control == DocumentControl::DocumentList, |button| {
                    button.aria_expanded(view.menu_bar.document_menu_open())
                })
                .w(px(24.))
                .h(metrics.row)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .when(enabled, |button| button.cursor_pointer())
                .when(!enabled, |button| button.opacity(0.4))
                .child(control.glyph())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(
                    cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                        if enabled {
                            if control == DocumentControl::DocumentList {
                                view.open_document_dropdown(event.position(), window, cx);
                            } else {
                                view.menu_input(MenuEvent::Dismiss, window, cx);
                                let action: Box<dyn gpui::Action> = match control {
                                    DocumentControl::New => Box::new(NewFile),
                                    _ => Box::new(CloseCurrentFile),
                                };
                                window.dispatch_action(action, cx);
                            }
                        }
                        cx.stop_propagation();
                    }),
                )
        })
        .collect::<Vec<_>>();
    let selected = view.menu_bar.selected_path().to_vec();
    let definitions = view.menu_bar.definitions().to_vec();
    let total_columns = view.menu_bar.popup_count();
    let anchor = view.menu_anchor;
    let symbol_popup = view.symbol_menu_open();
    let configured_popup = view.menu_bar.configured_context_menu_open();
    let symbol_selection_visible = view.symbol_menu_selection_visible;
    let first_depth = usize::from(anchor.is_some() && view.menu_submenu_only && total_columns > 1);
    while view.menu_scroll.len() < total_columns {
        view.menu_scroll.push(gpui::ScrollHandle::new());
    }
    let viewport = window.viewport_size();
    let measure = |text: &str| window.text_system().shape_line(
        text.to_owned().into(), metrics.size, &[gpui::TextRun {
            len: text.len(), font: metrics.font.clone(), color: window.text_style().color,
            background_color: None, underline: None, strikethrough: None,
        }], None,
    ).width().ceil();
    let widths: Vec<_> = (first_depth..total_columns).map(|depth| {
        let mut label_width = px(0.);
        let mut shortcut_width = px(0.);
        let mut minimum_width = MENU_MIN_WIDTH;
        if let Some(entries) = view.menu_bar.popup_entries(depth) {
            if depth == first_depth && entries.iter().any(|entry| entry.command().is_some()) {
                minimum_width = MENU_ROOT_MIN_WIDTH;
            }
            for entry in entries.iter().filter(|entry| !matches!(entry, MenuEntry::Separator)) {
                label_width = label_width.max(measure(entry_label(&filename, entry, configured_popup)));
                let shortcut = match entry {
                    MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => view.shortcut_label(*command).unwrap_or_else(|| entry.shortcut().to_owned()),
                    _ => entry.shortcut().to_owned(),
                };
                shortcut_width = shortcut_width.max(measure(&shortcut));
            }
        }
        let padding = 2. + MENU_GUTTER_WIDTH + MENU_COLUMN_GAP * 3. + MENU_ARROW_WIDTH + 10. + MENU_BORDER * 2.;
        let padding = padding + if shortcut_width > px(0.) { MENU_SHORTCUT_GAP - MENU_COLUMN_GAP } else { 0. };
        (label_width + shortcut_width + px(padding)).max(px(minimum_width)).min(viewport.width)
    }).collect();
    let root_widths = definitions
        .iter()
        .map(|definition| {
            let label = root_label(&filename, definition.selector, definition.label);
            window
                .text_system()
                .shape_line(
                    label.into(),
                    metrics.size,
                    &[gpui::TextRun {
                        len: label.len(),
                        font: metrics.font.clone(),
                        color: rgb(0x6d6d6d).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width()
                .ceil()
                + px(13.)
        })
        .collect::<Vec<_>>();
    let roots_width = (viewport.width - px(if view.menu_right_shortcuts_hidden { 4. } else { 76. })).max(px(1.));
    let total_root_width = root_widths.iter().copied().sum::<gpui::Pixels>();
    let root_prefix = root_widths.iter().take(root).copied().sum::<gpui::Pixels>();
    let mut root_offset = view.menu_root_scroll.offset();
    root_offset.x = root_offset.x.max((roots_width - total_root_width).min(px(0.))).min(px(0.));
    if mode == MenuMode::Closed {
        view.menu_root_reveal = None;
    } else {
        let target = (root, roots_width, total_root_width);
        if view.menu_root_reveal != Some(target) {
            if let Some(width) = root_widths.get(root) {
                if root_prefix + root_offset.x < px(0.) {
                    root_offset.x = -root_prefix;
                } else if root_prefix + *width + root_offset.x > roots_width {
                    root_offset.x = roots_width - root_prefix - *width;
                }
            }
            view.menu_root_reveal = Some(target);
        }
    }
    view.menu_root_scroll.set_offset(root_offset);
    let root_left = (root_prefix + root_offset.x).max(px(0.));
    let left = anchor.map_or(root_left, |position| position.x)
        .min((viewport.width - widths.first().copied().unwrap_or(px(0.))).max(px(0.)))
        .max(px(0.));
    let mut lefts: Vec<gpui::Pixels> = Vec::new();
    for (column, width) in widths.iter().copied().enumerate() {
        let at = if column == 0 { left } else {
            let parent = lefts[column - 1];
            let right = parent + widths[column - 1];
            if right + width <= viewport.width { right }
            else if parent >= width { parent - width }
            else { (viewport.width - width).max(px(0.)) }
        };
        lefts.push(at);
    }
    let max_height = popup_max_height(anchor.is_some(), viewport, &metrics);
    let background = if view.dark { 0x2b2b2b } else { 0xf0f0f0 };
    let foreground = if view.dark { 0xeeeeee } else { 0x202020 };
    let selected_background = view.appearance_colour("DarkMode.customColorMenuHotTrack", if view.dark { 0x444444 } else { 0x91c9f7 });
    let checked_background = if view.dark { 0x555555 } else { 0x9dccf3 };
    let mut popups = Vec::new();
    let mut desired_top = anchor.map_or(px(0.), |position| position.y);
    let mut anchor_top = px(0.);
    let mut extent = metrics.row;
    for depth in first_depth..total_columns {
        let column = depth - first_depth;
        let width = widths[column];
        let Some(entries) = view
            .menu_bar
            .popup_entries(depth)
            .map(<[MenuEntry]>::to_vec)
        else {
            continue;
        };
        let content_height = entries.iter().map(|entry| row_height(entry, &metrics)).sum::<gpui::Pixels>();
        let height = (content_height + px(MENU_BORDER * 2.)).min(max_height);
        let top = desired_top.min((max_height - height).max(px(0.))).max(px(0.));
        if column == 0 { anchor_top = top; }
        let label = if depth == 0 {
            if view.menu_bar.document_menu_open() {
                "Document list".into()
            } else {
                format!(
                    "{} menu",
                    definitions
                        .get(root)
                        .map(|definition| root_label(&filename, definition.selector, definition.label))
                        .unwrap_or("Application")
                )
            }
        } else {
            view.menu_bar
                .popup_entries(depth - 1)
                .and_then(|entries| {
                    selected
                        .get(depth - 1)
                        .and_then(|index| entries.get(*index))
                })
                .map(|entry| format!("{} menu", entry_label(&filename, entry, configured_popup)))
                .unwrap_or_else(|| "Submenu".into())
        };
        let prefix = selected[..depth].to_vec();
        let scroll = view.menu_scroll[depth].clone();
        let mut offset = scroll.offset();
        offset.y = offset.y.max((height - px(MENU_BORDER * 2.) - content_height).min(px(0.))).min(px(0.));
        scroll.set_offset(offset);
        if let Some(index) = selected.get(depth) {
            desired_top = top
                + px(MENU_BORDER)
                + entries
                    .iter()
                    .take(*index)
                    .map(|entry| row_height(entry, &metrics))
                    .fold(px(0.), |sum, row| sum + row)
                + offset.y;
        }
        extent = extent.max(metrics.row + top + height);
        let rows = entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                if matches!(entry, MenuEntry::Separator) {
                    return div()
                        .id(("menu-separator", index))
                        .h(px(MENU_SEPARATOR_HEIGHT))
                        .flex_shrink_0()
                        .pl(px(2. + MENU_GUTTER_WIDTH + MENU_COLUMN_GAP))
                        .pr(px(2.))
                        .child(div().mt(px(3.)).h(px(1.)).bg(rgb(if dark { 0x555555 } else { 0xd7d7d7 })))
                        .into_any_element();
                }
                let enabled = entry.enabled(&|command| available.allows(command));
                let is_selected = depth < view.menu_highlight_depth && selected.get(depth) == Some(&index);
                let checked_action = entry.command().map(MenuEntry::Action);
                let checked_entry = checked_action.as_ref().unwrap_or(&entry);
                let checked = match checked_entry {
                    MenuEntry::Action(MenuCommand::Source(42028)) => Some(view.workspace.active().is_user_read_only()),
                    MenuEntry::Action(MenuCommand::Source(42033)) => Some(view.disk_file().is_some_and(|file| file.is_read_only())),
                    MenuEntry::Action(MenuCommand::Source(id @ (42020 | 42089))) => view.editor().document().begin_selection_mode().map(|column| column == (*id == 42089)),
                    MenuEntry::Action(MenuCommand::Source(id)) => view.menu_source_checked(*id),
                    MenuEntry::Action(MenuCommand::ActivateDocument(id)) => Some(view.workspace.active().id() == *id),
                    MenuEntry::Action(MenuCommand::CharacterPanel) => Some(view.characters.visible),
                    MenuEntry::Action(MenuCommand::Language(language))
                    | MenuEntry::DynamicAction {
                        command: MenuCommand::Language(language),
                        ..
                    } => Some(view.workspace.active().language().key() == language.key()),
                    MenuEntry::Action(MenuCommand::Document(DocumentCommand::Pin)) => {
                        Some(view.workspace.active().is_pinned())
                    }
                    MenuEntry::Action(MenuCommand::Document(DocumentCommand::Color(color))) => {
                        Some(
                            view.workspace.tab_color(
                                view.workspace.active_view(),
                                view.workspace.active().id(),
                            ) == *color,
                        )
                    }
                    MenuEntry::Action(MenuCommand::Document(DocumentCommand::Multiline)) => {
                        Some(view.tab_multiline)
                    }
                    MenuEntry::Action(MenuCommand::Document(DocumentCommand::Vertical)) => {
                        Some(view.tab_vertical)
                    }
                    MenuEntry::Action(MenuCommand::Document(DocumentCommand::Mru)) => {
                        Some(view.mru_switching)
                    }
                    MenuEntry::Action(MenuCommand::Document(DocumentCommand::Switcher)) => {
                        Some(view.document_switcher)
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::ProjectPanel(index))) => {
                        Some(view.projects.visible(*index))
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::DocumentList)) => {
                        Some(view.document_list_visible)
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::DocumentMap)) => {
                        Some(view.document_map_visible())
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::MarkdownPreview)) => {
                        Some(view.markdown_preview.visible)
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::FolderWorkspace)) => {
                        Some(view.folder_workspace_visible())
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::FunctionList)) => {
                        Some(view.function_list_visible())
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::Monitoring)) => {
                        Some(view.monitoring_active())
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::WordWrap)) => {
                        Some(view.editor_word_wrap_enabled())
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::SyncVertical)) => {
                        Some(view.sync_vertical_enabled())
                    }
                    MenuEntry::Action(MenuCommand::View(ViewCommand::SyncHorizontal)) => {
                        Some(view.sync_horizontal_enabled())
                    }
                    MenuEntry::Action(MenuCommand::View(command))
                        if view.editor_symbol_shown(*command).is_some() => view.editor_symbol_shown(*command),
                    MenuEntry::Action(MenuCommand::View(ViewCommand::IndentGuides)) => {
                        Some(view.editor_indent_guides_shown())
                    }
                    MenuEntry::Action(MenuCommand::SelectEncoding(target)) => {
                        Some(*target == view.workspace.active().format())
                    }
                    _ => submenu_radio_checked(view, &entry),
                };
                let id = match &entry {
                    MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => {
                        selector(*command)
                    }
                    MenuEntry::Submenu { selector, .. } => (*selector).into(),
                    MenuEntry::DynamicSubmenu { selector, .. } => selector.clone(),
                    MenuEntry::Separator => "menu-separator".into(),
                };
                let shortcut = match &entry {
                    MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => view.shortcut_label(*command).unwrap_or_else(|| entry.shortcut().to_owned()),
                    _ => entry.shortcut().to_owned(),
                };
                let label = entry_label(&filename, &entry, configured_popup).to_owned();
                let submenu = entry.submenu_entries().is_some();
                let radio = radio_mark(entry.command()) || (submenu && checked.is_some());
                let mut path = prefix.clone();
                path.push(index);
                let hover_path = path.clone();
                div()
                    .id(("menu-row", index))
                    .debug_selector(move || id.clone())
                    .role(if checked.is_some() && radio {
                        Role::MenuItemRadio
                    } else if checked.is_some() {
                        Role::MenuItemCheckBox
                    } else {
                        Role::MenuItem
                    })
                    .aria_label(label.clone())
                    .aria_selected(is_selected && (!symbol_popup || symbol_selection_visible))
                    .a11y_synthetic_children(move |builder| {
                        if !enabled {
                            builder.parent_node().set_disabled();
                        }
                    })
                    .when_some(checked, |row, value| row.aria_toggled(value.into()))
                    .when(submenu, |row| {
                        row.aria_expanded(total_columns > depth + 1 && is_selected)
                    })
                    .when(!shortcut.is_empty(), |row| {
                        row.aria_keyshortcuts(shortcut.clone())
                    })
                    .h(metrics.row)
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(MENU_COLUMN_GAP))
                    .pl(px(2.))
                    .pr(px(10.))
                    .text_color(rgb(if enabled { foreground } else { 0x808080 }))
                    .when(is_selected && enabled && (!symbol_popup || symbol_selection_visible), |row| {
                        row.bg(rgb(selected_background))
                    })
                    .when(
                        is_selected && enabled && depth + 1 == total_columns && (!symbol_popup || symbol_selection_visible),
                        |row| row.aria_active_descendant(),
                    )
                    .on_mouse_move(cx.listener(move |view, _, window, cx| {
                        if !enabled { return; }
                        if view.menu_bar.active_root() == root
                            && view.menu_bar.selected_path().get(..hover_path.len()) == Some(hover_path.as_slice())
                            && (!symbol_popup || view.symbol_menu_selection_visible)
                            && (!submenu || view.menu_bar.popup_count() > hover_path.len())
                        {
                            if view.menu_highlight_depth != hover_path.len() {
                                view.menu_highlight_depth = hover_path.len();
                                cx.notify();
                            }
                            return;
                        }
                        view.menu_input(
                            MenuEvent::Hover {
                                root,
                                path: hover_path.clone(),
                            },
                            window,
                            cx,
                        )
                    }))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.menu_input(
                            MenuEvent::ClickAtRevision {
                                root,
                                path: path.clone(),
                                revision,
                            },
                            window,
                            cx,
                        )
                    }))
                    .child(
                        div()
                            .w(px(MENU_GUTTER_WIDTH))
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(checked == Some(true), |cell| {
                                cell.bg(rgb(checked_background)).child(
                                    div().size(px(12.)).flex().items_center().justify_center()
                                        .when(radio, |mark| mark.child(div().size(px(6.)).rounded_full().bg(rgb(if enabled { foreground } else { 0x808080 }))))
                                        .when(!radio, |mark| mark.child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                                            let mut path = gpui::PathBuilder::stroke(px(1.5));
                                            path.move_to(gpui::point(bounds.left() + px(1.), bounds.top() + px(6.)));
                                            path.line_to(gpui::point(bounds.left() + px(4.), bounds.top() + px(9.)));
                                            path.line_to(gpui::point(bounds.left() + px(11.), bounds.top() + px(2.)));
                                            match path.build() {
                                                Ok(path) => window.paint_path(path, rgb(if enabled { foreground } else { 0x808080 })),
                                                Err(error) => eprintln!("menu check mark could not be drawn: {error}"),
                                            }
                                        }).size(px(12.))))
                                )
                            }),
                    )
                    .child(div().flex_1().min_w(px(0.)).truncate().child(label))
                    .child(div().flex_shrink_0().child(shortcut))
                    .child(div().w(px(MENU_ARROW_WIDTH)).flex_shrink_0()
                        .when(submenu, |cell| cell.child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                            let mut path = gpui::PathBuilder::stroke(px(1.));
                            path.move_to(gpui::point(bounds.left() + px(1.), bounds.top() + px(2.)));
                            path.line_to(gpui::point(bounds.left() + px(4.), bounds.top() + px(5.)));
                            path.line_to(gpui::point(bounds.left() + px(1.), bounds.top() + px(8.)));
                            match path.build() {
                                Ok(path) => window.paint_path(path, rgb(if enabled { foreground } else { 0x808080 })),
                                Err(error) => eprintln!("menu submenu arrow could not be drawn: {error}"),
                            }
                        }).w(px(MENU_ARROW_WIDTH)).h(px(10.)))))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        popups.push(
            div()
                .id(("menu-popup", column))
                .debug_selector(move || format!("menu-popup-{column}"))
                .role(Role::Menu)
                .aria_label(label)
                .absolute()
                .left(if anchor.is_some() {
                    lefts[column] - left
                } else {
                    lefts[column]
                })
                .top(if anchor.is_some() { top - anchor_top } else { top })
                .w(width)
                .h(height)
                .font_family(metrics.family.clone())
                .text_size(metrics.size)
                .line_height(metrics.line_height)
                .flex()
                .flex_col()
                .border_1()
                .border_color(rgb(if dark { 0x555555 } else { 0xcccccc }))
                .bg(rgb(background))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("menu-items")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                        .children(rows),
                )
                .into_any_element(),
        );
    }
    let triggers = definitions
        .into_iter()
        .enumerate()
        .map(|(index, definition)| {
            let id = definition.selector;
            let label = root_label(&filename, id, definition.label);
            let direct = definition.direct_command().is_some();
            div()
                .id(("menu-root", index))
                .debug_selector(move || id.into())
                .role(Role::MenuItem)
                .aria_label(label)
                .when(!direct, |row| row.aria_expanded(
                    open && root == index && !view.menu_bar.document_menu_open() && !symbol_popup && !configured_popup,
                ))
                .h(metrics.row)
                .w(root_widths[index])
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .when(
                    mode != MenuMode::Closed
                        && !symbol_popup
                        && !configured_popup
                        && root == index
                        && !view.menu_bar.document_menu_open(),
                    |row| row.bg(rgb(selected_background)).text_color(rgb(foreground)),
                )
                .when(mode == MenuMode::BarFocused && root == index, |row| {
                    row.aria_active_descendant()
                })
                .on_mouse_move(cx.listener(move |view, _, window, cx| {
                    if view.menu_bar.mode() != MenuMode::Closed
                        && (view.menu_bar.active_root() != index
                            || view.menu_bar.document_menu_open()
                            || view.menu_bar.configured_context_menu_open())
                    {
                        view.menu_input(MenuEvent::HoverRoot(index), window, cx);
                    }
                }))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.menu_input(MenuEvent::ToggleRoot(index), window, cx)
                }))
                .child(label)
        })
        .collect::<Vec<_>>();
    let popup_content = if anchor.is_some() {
        gpui::anchored()
            .position(gpui::point(left, anchor_top))
            .child(
                div()
                    .relative()
                    .w(widths.first().copied().unwrap_or(px(0.)))
                    .h((extent - metrics.row - anchor_top).max(px(0.)))
                    .children(popups),
            )
            .into_any_element()
    } else {
        div().size_full().children(popups).into_any_element()
    };
    div()
        .id("matrix-menu-bar")
        .debug_selector(|| "matrix-menu-bar".into())
        .role(Role::MenuBar)
        .aria_label("Application menu")
        .track_focus(&view.menu_focus)
        .key_context("MatrixMenu")
        .on_action(cx.listener(|view, _: &MenuUp, window, cx| {
            view.menu_input(MenuEvent::Up, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuDown, window, cx| {
            view.menu_input(MenuEvent::Down, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuLeft, window, cx| {
            view.menu_input(MenuEvent::Left, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuRight, window, cx| {
            view.menu_input(MenuEvent::Right, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuEnter, window, cx| {
            view.menu_input(MenuEvent::Activate, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuEscape, window, cx| {
            view.menu_input(MenuEvent::Escape, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuFirst, window, cx| {
            view.menu_input(MenuEvent::First, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|view, _: &MenuLast, window, cx| {
            view.menu_input(MenuEvent::Last, window, cx);
            cx.stop_propagation();
        }))
        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            let key = &event.keystroke;
            if !key.modifiers.alt
                && !key.modifiers.control
                && !key.modifiers.platform
                && key.key.len() == 1
                && let Some(ch) = key.key.chars().next()
                && (ch.is_ascii_alphabetic() || ch == '?')
            {
                view.menu_input(MenuEvent::Mnemonic(ch), window, cx);
                cx.stop_propagation();
            }
        }))
        .relative()
        .h(metrics.row)
        .flex_shrink_0()
        .font_family(metrics.family)
        .text_size(metrics.size)
        .line_height(metrics.line_height)
        .bg(rgb(if view.dark { 0x2b2b2b } else { 0xffffff }))
        .text_color(rgb(if view.dark { 0xeeeeee } else { 0x6d6d6d }))
        .child(
            div()
                .id("menu-region")
                .absolute()
                .top(px(0.))
                .left(px(0.))
                .w(viewport.width)
                .h(if anchor.is_some() {
                    metrics.row
                } else {
                    extent
                })
                .occlude()
                .on_mouse_down_out(cx.listener(|view, _, window, cx| {
                    if view.menu_bar.mode() != MenuMode::Closed
                        && !view.menu_bar.document_menu_open()
                        && view.menu_anchor.is_none()
                    {
                        view.menu_input(MenuEvent::Dismiss, window, cx);
                    }
                }))
                .child(
                    div()
                        .h(metrics.row)
                        .w_full()
                        .flex()
                        .items_center()
                        .child(div()
                            .id("menu-roots")
                            .flex()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_x_scroll()
                            .track_scroll(&view.menu_root_scroll)
                            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(triggers))
                        .child(
                            div()
                                .id("menu-document-controls")
                                .flex()
                                .flex_shrink_0()
                                .pr(px(4.))
                                .children(document_controls),
                        ),
                )
                .when(open, |region| {
                    region.child(
                        gpui::deferred(
                            div()
                                .id("menu-overlay")
                                .absolute()
                                .top(metrics.row)
                                .left(px(0.))
                                .w(viewport.width)
                                .h(viewport.height)
                                .occlude()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, _, window, cx| {
                                        view.menu_input(MenuEvent::Dismiss, window, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(popup_content),
                        )
                        .priority(1),
                    )
                }),
        )
        .when(!view.dark, |bar| {
            bar.child(
                div()
                    .absolute()
                    .top(metrics.row - px(2.))
                    .left(px(0.))
                    .right(px(0.))
                    .h(px(2.))
                    .bg(rgb(0xf0f0f0))
                    .child(div().w_full().h(px(1.)).bg(rgb(0xf2f2f2))),
            )
        })
        .into_any_element()
}
