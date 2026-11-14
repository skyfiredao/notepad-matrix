use super::*;
use crate::input::LineEnding;
use crate::menu::{
    DocumentCommand, DocumentControl, MenuEntry, MultiSelectionCommand, TransformationAvailability,
};

const MENU_BAR_HEIGHT: f32 = 22.;
const MENU_FONT_SIZE: f32 = 13.;

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
        if !availability(self, cx).allows(MenuCommand::DocumentDropdown)
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
        self.menu_input(MenuEvent::OpenRoot(root), window, cx);
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
        KeyBinding::new("alt-r", OpenRunMenu, None),
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
}

impl Availability {
    pub(super) fn allows(self, command: MenuCommand) -> bool {
        match command {
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
            MenuCommand::Binary(crate::clipboard::BinaryCommand::Cut) => {
                self.navigation_ready && self.editable
            }
            MenuCommand::Binary(crate::clipboard::BinaryCommand::Paste) => {
                self.navigation_ready && self.editable && self.paste
            }
            MenuCommand::CharacterPanel => self.navigation_ready,
            MenuCommand::CopyPath(_) => self.navigation_ready,
            MenuCommand::Intelligence(_) => self.navigation_ready,
            MenuCommand::TextTransform(id) => self.transformations.allows(id),
            MenuCommand::ColumnEditor => self.navigation_ready && self.editable,
            MenuCommand::MultiSelection(
                MultiSelectionCommand::Undo | MultiSelectionCommand::Rotate,
            ) => self.navigation_ready && self.multiple_selections,
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
            MenuCommand::Search(_) => true,
            MenuCommand::Bookmark(_) => true,
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
            MenuCommand::Run => true,
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
            MenuCommand::Edit(EditCommand::Cut) => self.editable,
            MenuCommand::Edit(EditCommand::Copy) => true,
            MenuCommand::Edit(EditCommand::Paste) => self.editable && self.paste,
            MenuCommand::Edit(EditCommand::Delete) => self.navigation_ready && self.selection,
            MenuCommand::Edit(EditCommand::ConvertEol(target)) => {
                self.editable && target != self.line_ending
            }
            MenuCommand::Edit(EditCommand::DuplicateLine) => self.editable,
        }
    }
}

pub(super) fn availability(view: &MatrixWindow, cx: &App) -> Availability {
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
    Availability {
        dirty: view.workspace.active().is_modified(),
        any_dirty: !view.workspace.dirty_buffers().is_empty(),
        recent_count: view.workspace.recently_closed().len(),
        file_exists: view.disk_file().is_some_and(|file| file.path().is_file()),
        default_viewer: view.disk_file().is_some_and(|file| {
            file.path().is_file() && file_menu_surface::has_default_viewer(file.path())
        }),
        transformations: TransformationAvailability {
            editable: navigation_ready && !document.is_read_only(),
            line_comment: syntax.as_ref().is_some_and(|syntax| syntax.line.is_some()),
            block_comment: syntax
                .as_ref()
                .is_some_and(|syntax| syntax.stream.is_some()),
        },
        navigation_ready,
        restore_closed: !view.workspace.recently_closed().is_empty(),
        multiple_selections: view.editor().display_selections().ranges().len() > 1,
        document_switcher: view.document_switcher,
        selection: !view.editor().selection().range().is_empty(),
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
        MenuCommand::Run => "run-command",
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

fn row_height(entry: &MenuEntry) -> gpui::Pixels {
    px(if matches!(entry, MenuEntry::Separator) {
        8.
    } else {
        24.
    })
}

pub(super) fn render(
    view: &mut MatrixWindow,
    window: &Window,
    cx: &mut Context<MatrixWindow>,
) -> AnyElement {
    let mode = view.menu_bar.mode();
    let root = view.menu_bar.active_root();
    let open = mode == MenuMode::Open;
    let available = availability(view, cx);
    let revision = view.menu_bar.revision();
    let document_controls_enabled = !view.file_busy
        && !view.close_dialog
        && !view.save_all_dialog
        && view.reload_target.is_none()
        && !view.batch_editing_blocked();
    let dark = view.dark;
    let document_controls = DocumentControl::ALL
        .into_iter()
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
                .h(px(MENU_BAR_HEIGHT))
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
    let symbol_selection_visible = view.symbol_menu_selection_visible;
    let popup_row_height = |entry: &MenuEntry| {
        if symbol_popup && !matches!(entry, MenuEntry::Separator) {
            px(22.)
        } else {
            row_height(entry)
        }
    };
    let first_depth = usize::from(anchor.is_some() && view.menu_submenu_only && total_columns > 1);
    let columns = total_columns.saturating_sub(first_depth);
    while view.menu_scroll.len() < total_columns {
        view.menu_scroll.push(gpui::ScrollHandle::new());
    }
    let viewport = window.viewport_size();
    let width =
        px(if symbol_popup { 280. } else { 240. }).min(viewport.width / (columns.max(1) as f32));
    let root_widths = definitions
        .iter()
        .map(|definition| {
            window
                .text_system()
                .shape_line(
                    definition.label.into(),
                    px(MENU_FONT_SIZE),
                    &[gpui::TextRun {
                        len: definition.label.len(),
                        font: gpui::font("Helvetica"),
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
    let root_left = root_widths.iter().take(root).copied().sum::<gpui::Pixels>();
    let left = root_left.min((viewport.width - width * columns as f32).max(px(0.)));
    let max_height = if anchor.is_some() {
        viewport.height
    } else {
        viewport.height - px(64.)
    }
    .max(px(26.));
    let background = if view.dark { 0x2b2b2b } else { 0xf0f0f0 };
    let foreground = if view.dark { 0xeeeeee } else { 0x202020 };
    let mut popups = Vec::new();
    let mut desired_top = px(0.);
    let mut extent = px(MENU_BAR_HEIGHT);
    for depth in first_depth..total_columns {
        let column = depth - first_depth;
        let Some(entries) = view
            .menu_bar
            .popup_entries(depth)
            .map(<[MenuEntry]>::to_vec)
        else {
            continue;
        };
        let height = (entries
            .iter()
            .map(popup_row_height)
            .fold(px(2.), |sum, row| sum + row))
        .min(max_height);
        let top = desired_top.min((max_height - height).max(px(0.)));
        let label = if depth == 0 {
            if view.menu_bar.document_menu_open() {
                "Document list".into()
            } else {
                format!(
                    "{} menu",
                    definitions
                        .get(root)
                        .map(|definition| definition.label)
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
                .map(|entry| format!("{} menu", entry.label()))
                .unwrap_or_else(|| "Submenu".into())
        };
        let prefix = selected[..depth].to_vec();
        let scroll = view.menu_scroll[depth].clone();
        if let Some(index) = selected.get(depth) {
            desired_top = top
                + entries
                    .iter()
                    .take(*index)
                    .map(popup_row_height)
                    .fold(px(0.), |sum, row| sum + row)
                + scroll.offset().y;
        }
        extent = extent.max(px(MENU_BAR_HEIGHT) + top + height);
        let rows = entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                if matches!(entry, MenuEntry::Separator) {
                    return div()
                        .id(("menu-separator", index))
                        .h(px(8.))
                        .flex_shrink_0()
                        .px(px(8.))
                        .when(symbol_popup, |row| row.pl(px(24.)).pr(px(2.)))
                        .child(div().mt(px(3.)).h(px(1.)).bg(rgb(if symbol_popup { 0xd7d7d7 } else { 0x808080 })))
                        .into_any_element();
                }
                let enabled = entry.enabled(&|command| available.allows(command));
                let is_selected = selected.get(depth) == Some(&index);
                let checked = match &entry {
                    MenuEntry::DynamicAction {
                        command: MenuCommand::ActivateDocument(id),
                        ..
                    } => Some(view.workspace.active().id() == *id),
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
                    _ => None,
                };
                let id = match &entry {
                    MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => {
                        selector(*command)
                    }
                    MenuEntry::Submenu { selector, .. } => (*selector).into(),
                    MenuEntry::Separator => "menu-separator".into(),
                };
                let shortcut = entry.shortcut().to_owned();
                let label = entry.label().to_owned();
                let submenu = matches!(entry, MenuEntry::Submenu { .. });
                let mut path = prefix.clone();
                path.push(index);
                let hover_path = path.clone();
                div()
                    .id(("menu-row", index))
                    .debug_selector(move || id.clone())
                    .role(if matches!(&entry, MenuEntry::Action(MenuCommand::View(command)) if view.editor_symbol_shown(*command).is_some()) {
                        Role::MenuItemCheckBox
                    } else if checked.is_some() {
                        Role::MenuItemRadio
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
                    .h(px(if symbol_popup { 22. } else { 24. }))
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(8.))
                    .when(symbol_popup, |row| row.pl(px(0.)).gap(px(10.)))
                    .text_color(rgb(if enabled { foreground } else { 0x808080 }))
                    .when(is_selected && enabled && (!symbol_popup || symbol_selection_visible), |row| {
                        if symbol_popup {
                            row.bg(rgb(0xe5f3ff))
                        } else {
                            row.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                        }
                    })
                    .when(
                        is_selected && enabled && depth + 1 == total_columns && (!symbol_popup || symbol_selection_visible),
                        |row| row.aria_active_descendant(),
                    )
                    .on_mouse_move(cx.listener(move |view, _, window, cx| {
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
                            .w(px(if symbol_popup { 24. } else { 12. }))
                            .when(symbol_popup, |cell| {
                                cell.h_full().flex().items_center().justify_center()
                                    .when(checked == Some(true), |cell| cell.bg(rgb(0x9dccf3)))
                            })
                            .child(if checked == Some(true) { "✓" } else { "" }),
                    )
                    .child(div().flex_1().truncate().child(label))
                    .child(shortcut)
                    .child(if submenu { "▶" } else { "" })
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
                    width * column as f32
                } else {
                    left + width * column as f32
                })
                .top(top)
                .w(width)
                .h(height)
                .flex()
                .flex_col()
                .border_1()
                .border_color(rgb(0x808080))
                .bg(rgb(background))
                .occlude()
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(rows)
                .into_any_element(),
        );
    }
    let triggers = definitions
        .into_iter()
        .enumerate()
        .map(|(index, definition)| {
            let id = definition.selector;
            div()
                .id(("menu-root", index))
                .debug_selector(move || id.into())
                .role(Role::MenuItem)
                .aria_label(definition.label)
                .aria_expanded(
                    open && root == index && !view.menu_bar.document_menu_open() && !symbol_popup,
                )
                .h(px(MENU_BAR_HEIGHT))
                .w(root_widths[index])
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .when(
                    mode != MenuMode::Closed
                        && !symbol_popup
                        && root == index
                        && !view.menu_bar.document_menu_open(),
                    |row| row.bg(rgb(0x0078d4)).text_color(rgb(0xffffff)),
                )
                .when(mode == MenuMode::BarFocused && root == index, |row| {
                    row.aria_active_descendant()
                })
                .on_mouse_move(cx.listener(move |view, _, window, cx| {
                    if view.menu_bar.mode() != MenuMode::Closed
                        && (view.menu_bar.active_root() != index
                            || view.menu_bar.document_menu_open())
                    {
                        view.menu_input(MenuEvent::HoverRoot(index), window, cx);
                    }
                }))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.menu_input(MenuEvent::ToggleRoot(index), window, cx)
                }))
                .child(definition.label)
        })
        .collect::<Vec<_>>();
    let popup_content = if let Some(position) = anchor {
        gpui::anchored()
            .position(position)
            .snap_to_window()
            .child(
                div()
                    .relative()
                    .w(width * columns as f32)
                    .h((extent - px(MENU_BAR_HEIGHT)).max(px(0.)))
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
        .h(px(MENU_BAR_HEIGHT))
        .flex_shrink_0()
        .font_family("Helvetica")
        .text_size(px(MENU_FONT_SIZE))
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
                    px(MENU_BAR_HEIGHT)
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
                        .h(px(MENU_BAR_HEIGHT))
                        .w_full()
                        .flex()
                        .items_center()
                        .children(triggers)
                        .child(div().flex_1())
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
                                .top(px(MENU_BAR_HEIGHT))
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
                    .top(px(MENU_BAR_HEIGHT - 2.))
                    .left(px(0.))
                    .right(px(0.))
                    .h(px(2.))
                    .bg(rgb(0xf0f0f0))
                    .child(div().w_full().h(px(1.)).bg(rgb(0xf2f2f2))),
            )
        })
        .into_any_element()
}
