use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, FocusHandle, KeyBinding, MouseButton, Role,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, px, rgb, size,
};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSView, NSWindowButton};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::ConfigStore;
use crate::editor::{ByteOffset, Selection};
use crate::input::EditorSession;
use crate::menu::{
    BookmarkCommand, EditCommand, FileCommand, MenuBar, MenuCommand, MenuEvent, MenuMode,
    MenuOutcome, SearchCommand, SettingsCommand, ViewCommand,
};

mod about_surface;
mod bookmark_surface;
mod builtin_language_surface;
mod caption_icons;
mod window_caption;
mod window_size_indicator;
mod character_panel_surface;
mod character_range_surface;
mod clipboard_surface;
mod column_editor_surface;
mod context_menu_config_surface;
mod converter_surface;
mod document_list_surface;
mod document_map_surface;
mod document_session_surface;
mod dialog_font;
mod edit_search_surface;
mod editor_surface;
mod file_actions;
mod file_menu_surface;
mod file_search_surface;
mod find_options_surface;
mod finder_layout;
mod find_surface;
mod floating_windows;
mod folder_surface;
mod function_list_surface;
mod goto_surface;
mod hash_surface;
mod help_surface;
mod encoding_surface;
mod html_export_surface;
mod icon_tooltip;
mod incremental_surface;
mod instance_surface;
mod intelligence_surface;
mod language_surface;
mod launch_request_surface;
mod macro_dialog_surface;
mod macro_surface;
mod mark_surface;
mod markdown_surface;
mod matrix_auth;
mod matrix_rain;
mod matrix_surface;
mod matrix_terminal;
mod menu_surface;
mod mermaid;
mod mime_surface;
mod monitoring_surface;
mod native_quit;
mod native_path_prompt;
mod native_app_launch;
mod panel_surface;
mod preferences_surface;
mod config_directory_ui;
mod preferences_appearance_surface;
mod preferences_editing_surface;
mod preferences_files_surface;
mod print_surface;
mod project_surface;
mod recovery_surface;
mod reload_actions;
mod replace_surface;
mod run_surface;
mod search_history_surface;
mod selection_actions_surface;
mod shortcut_surface;
pub(crate) mod shortcut_layout;
mod split_view;
mod split_view_geometry;
mod status_surface;
mod source_surface;
mod style_config_surface;
mod summary_surface;
mod tab_preview;
mod text_field;
mod text_transform_surface;
mod toolbar_icons;
mod toolbar_surface;
mod udl_surface;
mod windows_surface;
mod window_manager_surface;
mod view_surface;
mod workspace_surface;

actions!(
    notepad_matrix,
    [
        CloseWindow,
        DialogFocusNext,
        DialogFocusPrevious,
        QuitApplication,
        ToggleMenuBar,
        OpenFileMenu,
        OpenEditMenu,
        OpenEncodingMenu,
        OpenViewMenu,
        OpenWindowMenu,
        OpenRunMenu,
        OpenToolsMenu,
        OpenMacroMenu,
        OpenLanguageMenu,
        OpenHelpMenu,
        MinimizeWindow,
        StartMacroRecording,
        StopMacroRecording,
        PlaybackMacro,
        SaveCurrentMacro,
        RunMacroMultiple,
        ShowRun,
        PrintFile,
        ConfirmRun,
        CancelRun,
        RunFocusNext,
        RunFocusPrevious,
        ToggleDocumentList,
        ToggleOverwrite,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToggleAllCharacters,
        ToggleWordWrap,
        ToggleIndentGuides,
        ToggleDocumentMap,
        ToggleFolderWorkspace,
        ToggleFunctionList,
        ToggleMonitoring,
        CloneToOtherView,
        MoveToOtherView,
        ToggleSyncVertical,
        ToggleSyncHorizontal,
        ShowUdl,
        ShowSummary,
        OpenEolMenu,
        OpenSearchMenu,
        OpenSettingsMenu,
        OpenPluginsMenu,
        MimeEncode,
        MimeEncodePadded,
        MimeDecode,
        ConverterEncode,
        ConverterDecode,
        ExportHtml,
        ShowPreferences,
        ShowMarkdownPreview,
        CloseMarkdownPreview,
        ClosePreferences,
        ShowFind,
        CompleteWord,
        CompleteApi,
        CompletePath,
        ShowCalltip,
        PreviousCalltipOverload,
        NextCalltipOverload,
        ToggleBookmark,
        NextBookmark,
        PreviousBookmark,
        ClearBookmarks,
        ShowFindInFiles,
        FindAllInFiles,
        CloseFindInFiles,
        ShowReplace,
        ReplaceOne,
        ReplaceAll,
        CloseFind,
        FindNext,
        FindPrevious,
        FindDialogNext,
        FindDialogReverse,
        CountFind,
        SearchHistoryAccept,
        SearchHistoryCancel,
        SearchHistoryUp,
        SearchHistoryDown,
        SearchHistoryOpen,
        SearchHistoryNextField,
        SearchHistoryPreviousField,
        ShowGoTo,
        ConfirmGoTo,
        CloseGoTo,
        MenuUp,
        MenuDown,
        MenuEnter,
        MenuEscape,
        MenuLeft,
        MenuRight,
        MenuFirst,
        MenuLast,
        EditorBackspace,
        EditorBackspaceNotLine,
        EditorLineDelete,
        EditorDuplicateSelection,
        EditorDelete,
        EditorLeft,
        EditorRight,
        EditorSelectLeft,
        EditorSelectRight,
        EditorSelectAll,
        EditorUndo,
        EditorRedo,
        EditorCut,
        EditorCopy,
        EditorPaste,
        EditorEnter,
        EditorTab,
        EditorBackTab,
        EditorEscape,
        EditorDocumentStart,
        EditorDocumentEnd,
        EditorSelectDocumentStart,
        EditorSelectDocumentEnd,
        EditorHome,
        EditorEnd,
        EditorSelectHome,
        EditorSelectEnd,
        EditorUp,
        EditorDown,
        EditorSelectUp,
        EditorSelectDown,
        SaveFile,
        SaveFileAs,
        CancelClose,
        DiscardClose,
        SaveAndClose,
        SaveAllAndClose,
        DiscardAllAndClose,
        NewFile,
        OpenFile,
        NextBuffer,
        PreviousBuffer,
        MruNextBuffer,
        MruPreviousBuffer,
        CommitDocumentSwitcher,
        CancelDocumentSwitcher,
        ReopenClosedFile,
        PinCurrentTab,
        ToggleMultilineTabs,
        ToggleVerticalTabs,
        SaveAllFiles,
        ConfirmSaveAll,
        CancelSaveAll,
        SaveCopyFile,
        CloseCurrentFile,
        CloseAllFiles,
        CloseLeftFiles,
        CloseRightFiles,
        ReloadFile,
        ConfirmReload,
        CancelReload,
        ToggleReloadChoice,
        ActivateReloadChoice,
        FileDialogNext,
        FileDialogPrevious,
        FileDialogActivate
    ]
);

struct MatrixWindow {
    document_window: gpui::AnyWindowHandle,
    floating_windows: std::collections::BTreeMap<
        floating_windows::DialogKind,
        gpui::WindowHandle<floating_windows::FloatingWindow>,
    >,
    floating_opening: std::collections::BTreeSet<floating_windows::DialogKind>,
    focus: FocusHandle,
    dark: bool,
    menu_focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    menu_bar: MenuBar,
    menu_scroll: Vec<gpui::ScrollHandle>,
    menu_root_scroll: gpui::ScrollHandle,
    menu_root_reveal: Option<(usize, gpui::Pixels, gpui::Pixels)>,
    menu_font_family: RefCell<Option<(String, String)>>,
    menu_highlight_depth: usize,
    menu_anchor: Option<gpui::Point<gpui::Pixels>>,
    menu_submenu_only: bool,
    menu_right_shortcuts_hidden: bool,
    language_menu_compact: bool,
    alt_pressed: bool,
    alt_used: bool,
    workspace: crate::workspace::Workspace,
    matrix: matrix_surface::State,
    pane: crate::workspace::ViewId,
    other_pane: split_view::PaneState,
    split_sync: split_view_geometry::SyncScroll,
    split_sync_source: Option<crate::workspace::ViewId>,
    split_ratio: f32,
    splitter_dragging: bool,
    split_bounds: Option<Bounds<gpui::Pixels>>,
    editor_focus: FocusHandle,
    editor_lines: Vec<editor_surface::PaintedLine>,
    editor_chrome: editor_surface::EditorChrome,
    editor_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    editor_dragging: bool,
    editor_error: Option<crate::localization::Text>,
    editor_error_revision: u64,
    editor_painted_revision: Option<crate::input::DisplayRevision>,
    editor_accessibility: Option<(crate::input::DisplayRevision, gpui::SharedString)>,
    editor_top_line: usize,
    editor_horizontal_offset: gpui::Pixels,
    editor_follow_caret: bool,
    editor_scroll_remainder: f32,
    editor_observed_revision: crate::input::DisplayRevision,
    editor_observed_selection: crate::editor::Selection,
    editor_preferred_x: Option<gpui::Pixels>,
    file_busy: bool,
    file_prompt: file_actions::FilePromptState,
    launch_receiver: Option<gpui::Task<()>>,
    printing: bool,
    close_dialog: bool,
    close_multi: bool,
    close_all_answer: Option<file_actions::CloseAllAnswer>,
    close_choice: usize,
    close_focus: FocusHandle,
    close_after_save: bool,
    close_target: Option<crate::workspace::BufferId>,
    close_tabs_queue: std::collections::VecDeque<crate::workspace::BufferId>,
    conditional_close: Option<file_actions::ConditionalClose>,
    close_discarded: std::collections::BTreeSet<crate::workspace::BufferId>,
    _window_size_indicator: Option<window_size_indicator::WindowSizeIndicator>,
    tab_scroll: gpui::ScrollHandle,
    tab_overflow: [bool; 2],
    tab_scroll_repeat: Option<gpui::Task<()>>,
    panels: panel_surface::PanelState,
    editor_symbols: editor_surface::EditorSymbols,
    symbol_menu_selection_visible: bool,
    tab_multiline: bool,
    tab_vertical: bool,
    tab_compact_label_len: usize,
    tab_close_pressed: Option<(crate::workspace::ViewId, crate::workspace::BufferId)>,
    mru_switching: bool,
    document_switcher: bool,
    backup_failure: document_session_surface::BackupFailureState,
    mru_focus: FocusHandle,
    document_list_visible: bool,
    document_map: document_map_surface::DocumentMapState,
    folder: folder_surface::FolderState,
    projects: project_surface::ProjectState,
    characters: character_panel_surface::State,
    function_list: function_list_surface::FunctionListState,
    monitoring: monitoring_surface::MonitoringState,
    document_list_scroll: gpui::UniformListScrollHandle,
    save_all_queue: std::collections::VecDeque<crate::workspace::BufferId>,
    save_all_restore: Option<crate::workspace::BufferId>,
    save_all_restore_view: Option<crate::workspace::ViewId>,
    save_all_dialog: bool,
    save_all_choice_yes: bool,
    save_all_confirm: bool,
    save_all_errors: Vec<String>,
    reload_target: Option<crate::workspace::BufferId>,
    reload_dialog: bool,
    reload_choice_yes: bool,
    reload_external_change: bool,
    recovery_reload_pending: std::collections::VecDeque<crate::workspace::BufferId>,
    native_quit: Option<Rc<native_quit::QuitControl>>,
    application_quit_pending: bool,
    recovery: Option<crate::recovery::RecoveryManager>,
    backup_retention: recovery_surface::BackupRetentionState,
    find_visible: bool,
    find_created: bool,
    files_search: file_search_surface::FileSearchState,
    find_replace_mode: bool,
    find_query: gpui::Entity<text_field::TextField>,
    replace_input: gpui::Entity<text_field::TextField>,
    find_options: crate::search::LiteralOptions,
    find_regex_dot: bool,
    find_regex_backward: bool,
    find_mode_focus: [FocusHandle; 3],
    find_tabs_focus: FocusHandle,
    find_mark_mode: bool,
    mark_bookmark: bool,
    mark_purge: bool,
    mark_all_case_sensitive: bool,
    mark_all_word_only: bool,
    incremental: incremental_surface::State,
    character_range: character_range_surface::State,
    find_project_mode: bool,
    find_project_panels: [bool; 3],
    find_controls: find_options_surface::State,
    search_history: search_history_surface::SearchHistoryState,
    find_backward: bool,
    find_in_selection: bool,
    find_in_selection_enabled: bool,
    find_in_selection_active: bool,
    find_activation_prepared: bool,
    find_scope_auto_changed: bool,
    find_autocheck_threshold: usize,
    find_fill_threshold: usize,
    find_fill_selected: bool,
    find_fill_caret: bool,
    find_message: crate::localization::Text,
    find_status: find_surface::FindStatus,
    find_status_tooltip: crate::localization::Text,
    goto_visible: bool,
    goto: goto_surface::State,
    goto_input: gpui::Entity<text_field::TextField>,
    status_visible: bool,
    mute_sounds: bool,
    preferences_visible: bool,
    preferences: preferences_surface::State,
    config_directory: config_directory_ui::ConfigDirectoryState,
    style_config: style_config_surface::State,
    shortcut_mapper: shortcut_surface::State,
    context_menu: crate::context_menu::ContextMenu,
    about_visible: bool,
    help_state: help_surface::State,
    run_state: run_surface::State,
    converter_state: converter_surface::State,
    encoding_state: encoding_surface::EncodingState,
    hash_state: hash_surface::State,
    view_state: view_surface::State,
    window_manager: window_manager_surface::State,
    summary_visible: bool,
    markdown_preview: markdown_surface::State,
    preferences_focus: FocusHandle,
    settings_store: Option<ConfigStore>,
    mime_enabled: bool,
    converter_enabled: bool,
    html_export_enabled: bool,
    run_visible: bool,
    run_input: gpui::Entity<text_field::TextField>,
    run_message: String,
    run_button_focus: [FocusHandle; 2],
    macro_recording: bool,
    macro_dialogs: macro_dialog_surface::MacroDialogs,
    overwrite_mode: bool,
    line_copy_cut_without_selection: bool,
    column_editor: column_editor_surface::ColumnEditorState,
    udl: udl_surface::UdlState,
    builtin: builtin_language_surface::BuiltinState,
    intelligence: intelligence_surface::State,
    macro_playing: bool,
    macro_steps: Vec<crate::macros::MacroStep>,
    macro_step_pending: bool,
    syntax_cache: Option<(
        crate::input::DisplayRevision,
        crate::syntax::Language,
        Vec<crate::syntax::StyleSpan>,
    )>,
}

impl MatrixWindow {
    fn new(dark: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let editor_focus = cx.focus_handle();
        editor_focus.focus(window, cx);
        let workspace = crate::workspace::Workspace::new();
        let editor = &workspace.active().editor;
        let editor_observed_revision = editor.display_revision();
        let editor_observed_selection = editor.selection();
        let other_pane = split_view::PaneState::new(editor, cx);
        let close_view = cx.weak_entity();
        cx.observe_window_activation(window, |view: &mut Self, window, cx| {
            if !window.is_window_active() {
                view.tab_scroll_repeat = None;
                view.cancel_finder_drag();
                view.defer_intelligence_focus_loss(cx);
            }
            cx.notify();
        })
        .detach();
        window.on_window_should_close(cx, move |window, cx| {
            match close_view.update(cx, |view: &mut Self, cx| {
                view.request_close(window, cx);
                false
            }) {
                Ok(allow) => allow,
                Err(error) => {
                    eprintln!("close guard could not reach its document: {error}");
                    false
                }
            }
        });
        let document_window = Window::window_handle(window);
        let owner = cx.weak_entity();
        cx.on_window_closed(move |cx, _| {
            if cx.windows().contains(&document_window) {
                return;
            }
            if let Some(owner) = owner.upgrade() {
                let handles: Vec<_> = owner.update(cx, |view, _| {
                    view.recovery = None;
                    view.floating_windows.values().copied().collect()
                });
                for handle in handles {
                    if cx.windows().contains(&handle.into())
                        && let Err(error) = handle.update(cx, |_, window, _| window.remove_window())
                    {
                        eprintln!("could not close document auxiliary window: {error}");
                    }
                }
            }
        })
        .detach();
        let window_size_indicator = match window_size_indicator::WindowSizeIndicator::new(window) {
            Ok(indicator) => Some(indicator),
            Err(error) => {
                eprintln!("could not install window size indicator: {error}");
                None
            }
        };
        Self {
            document_window: Window::window_handle(window),
            floating_windows: std::collections::BTreeMap::new(),
            floating_opening: std::collections::BTreeSet::new(),
            focus,
            dark,
            menu_focus: cx.focus_handle(),
            previous_focus: None,
            menu_bar: MenuBar::default(),
            menu_scroll: Vec::new(),
            menu_root_scroll: gpui::ScrollHandle::new(),
            menu_root_reveal: None,
            menu_font_family: RefCell::new(None),
            menu_highlight_depth: 0,
            menu_anchor: None,
            menu_submenu_only: false,
            menu_right_shortcuts_hidden: false,
            language_menu_compact: true,
            alt_pressed: false,
            alt_used: false,
            workspace,
            matrix: matrix_surface::State::default(),
            pane: crate::workspace::ViewId::Primary,
            other_pane,
            split_sync: Default::default(),
            split_sync_source: None,
            split_ratio: 0.5,
            splitter_dragging: false,
            split_bounds: None,
            editor_focus,
            editor_lines: Vec::new(),
            editor_chrome: editor_surface::EditorChrome::default(),
            editor_bounds: None,
            editor_dragging: false,
            editor_error: None,
            editor_error_revision: 0,
            editor_painted_revision: None,
            editor_accessibility: None,
            editor_top_line: 0,
            editor_horizontal_offset: px(0.),
            editor_follow_caret: true,
            editor_scroll_remainder: 0.,
            editor_observed_revision,
            editor_observed_selection,
            editor_preferred_x: None,
            file_busy: false,
            file_prompt: file_actions::FilePromptState::default(),
            launch_receiver: None,
            printing: false,
            close_dialog: false,
            close_multi: false,
            close_all_answer: None,
            close_choice: 0,
            close_focus: cx.focus_handle(),
            close_after_save: false,
            close_target: None,
            close_tabs_queue: std::collections::VecDeque::new(),
            conditional_close: None,
            close_discarded: std::collections::BTreeSet::new(),
            _window_size_indicator: window_size_indicator,
            tab_scroll: gpui::ScrollHandle::new(),
            tab_overflow: [false; 2],
            tab_scroll_repeat: None,
            panels: Default::default(),
            editor_symbols: Default::default(),
            symbol_menu_selection_visible: false,
            tab_multiline: false,
            tab_vertical: false,
            tab_compact_label_len: 0,
            tab_close_pressed: None,
            mru_switching: true,
            document_switcher: true,
            backup_failure: document_session_surface::BackupFailureState::new(cx),
            mru_focus: cx.focus_handle(),
            document_list_visible: false,
            document_map: Default::default(),
            folder: folder_surface::FolderState::new(cx),
            projects: project_surface::ProjectState::new(cx),
            characters: character_panel_surface::State::new(cx),
            function_list: function_list_surface::FunctionListState::new(cx),
            monitoring: Default::default(),
            document_list_scroll: gpui::UniformListScrollHandle::new(),
            save_all_queue: std::collections::VecDeque::new(),
            save_all_restore: None,
            save_all_restore_view: None,
            save_all_dialog: false,
            save_all_choice_yes: true,
            save_all_confirm: true,
            save_all_errors: Vec::new(),
            reload_target: None,
            reload_dialog: false,
            reload_choice_yes: true,
            reload_external_change: false,
            recovery_reload_pending: std::collections::VecDeque::new(),
            native_quit: None,
            application_quit_pending: false,
            recovery: None,
            backup_retention: recovery_surface::BackupRetentionState::default(),
            find_visible: false,
            find_created: false,
            files_search: file_search_surface::FileSearchState::new(cx),
            find_replace_mode: false,
            find_query: cx.new(|cx| text_field::TextField::compact("查找目标", "find-query", cx)),
            replace_input: cx
                .new(|cx| text_field::TextField::compact("替换为", "replace-input", cx).with_tab_index(1)),
            find_options: crate::search::LiteralOptions::default(),
            find_regex_dot: false,
            find_regex_backward: false,
            find_mode_focus: [
                cx.focus_handle().tab_index(18).tab_stop(true),
                cx.focus_handle().tab_index(18).tab_stop(false),
                cx.focus_handle().tab_index(18).tab_stop(false),
            ],
            find_tabs_focus: cx.focus_handle().tab_index(-1),
            find_mark_mode: false,
            mark_bookmark: false,
            mark_purge: false,
            mark_all_case_sensitive: false,
            mark_all_word_only: true,
            incremental: incremental_surface::State::new(window, cx),
            character_range: character_range_surface::State::new(cx),
            find_project_mode: false,
            find_project_panels: [false; 3],
            find_controls: find_options_surface::State::new(cx),
            search_history: search_history_surface::SearchHistoryState::new(),
            find_backward: false,
            find_in_selection: false,
            find_in_selection_enabled: false,
            find_in_selection_active: false,
            find_activation_prepared: false,
            find_scope_auto_changed: false,
            find_autocheck_threshold: 1024,
            find_fill_threshold: 1024,
            find_fill_selected: true,
            find_fill_caret: true,
            find_message: "".into(),
            find_status: find_surface::FindStatus::NoMessage,
            find_status_tooltip: "".into(),
            goto_visible: false,
            goto: goto_surface::State::new(cx),
            goto_input: cx
                .new(|cx| text_field::TextField::new("Line number", "goto-line-input", cx)),
            status_visible: true,
            mute_sounds: false,
            preferences_visible: false,
            preferences: preferences_surface::State::new(cx),
            config_directory: config_directory_ui::ConfigDirectoryState::new(cx),
            style_config: style_config_surface::State::new(cx),
            shortcut_mapper: shortcut_surface::State::new(cx),
            context_menu: crate::context_menu::ContextMenu::default(),
            about_visible: false,
            help_state: help_surface::State::default(),
            run_state: run_surface::State::new(cx),
            converter_state: converter_surface::State::new(cx),
            encoding_state: Default::default(),
            hash_state: hash_surface::State::new(cx),
            view_state: view_surface::State::default(),
            window_manager: window_manager_surface::State::new(cx),
            summary_visible: false,
            preferences_focus: cx.focus_handle(),
            settings_store: None,
            mime_enabled: true,
            converter_enabled: true,
            html_export_enabled: true,
            run_visible: false,
            run_input: cx.new(|cx| text_field::TextField::compact("输入运行程序名", "run-input", cx).without_history_arrow().with_compact_height(22.)),
            run_message: String::new(),
            run_button_focus: [cx.focus_handle(), cx.focus_handle()],
            macro_recording: false,
            macro_dialogs: macro_dialog_surface::MacroDialogs::new(cx),
            overwrite_mode: false,
            line_copy_cut_without_selection: true,
            column_editor: column_editor_surface::ColumnEditorState::new(cx),
            udl: udl_surface::UdlState::new(cx),
            builtin: builtin_language_surface::BuiltinState::new(),
            intelligence: intelligence_surface::State::new(cx),
            markdown_preview: Default::default(),
            macro_playing: false,
            macro_steps: Vec::new(),
            macro_step_pending: false,
            syntax_cache: None,
        }
    }

    fn editor(&self) -> &EditorSession {
        &self.workspace.active().editor
    }

    pub(super) fn configure_embedded_tools(&mut self, mime: bool, converter: bool, html: bool) {
        self.mime_enabled = mime;
        self.converter_enabled = converter;
        self.html_export_enabled = html;
        self.menu_bar
            .set_embedded_tools_enabled(mime, converter, html);
    }

    fn batch_editing_blocked(&self) -> bool {
        matrix_surface::locked()
            || self.save_all_restore.is_some()
            || self.reload_target.is_some()
            || self.application_quit_pending
            || self.macro_interaction_blocked()
            || self.window_manager_blocks_document()
            || self.udl.styler_visible
    }

    fn editor_mut(&mut self) -> &mut EditorSession {
        &mut self.workspace.active_mut().editor
    }

    fn disk_file(&self) -> Option<&crate::files::DiskFile> {
        self.workspace.active().disk_file.as_ref()
    }

    pub(super) fn defer_intelligence_focus_loss(&self, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(owner) = owner.upgrade() else {
                return;
            };
            owner.update(cx, |view, cx| {
                let active = cx.active_window();
                let owned_popup = [
                    floating_windows::DialogKind::Completion,
                    floating_windows::DialogKind::Calltip,
                ]
                .into_iter()
                .any(|kind| {
                    view.floating_windows
                        .get(&kind)
                        .is_some_and(|handle| Some((*handle).into()) == active)
                });
                if active != Some(view.document_window) && !owned_popup {
                    view.dismiss_completion(cx);
                    view.dismiss_calltip(cx);
                }
            });
        });
    }

    fn open_menu_root(&mut self, selector: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.alt_used = true;
        if let Some(root) = self
            .menu_bar
            .definitions()
            .iter()
            .position(|definition| definition.selector == selector)
        {
            self.menu_input(MenuEvent::OpenRoot(root), window, cx);
        }
    }

    fn menu_input(&mut self, input: MenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        if matrix_surface::locked() || (self.matrix_active() && !matches!(&input, MenuEvent::Dismiss)) {
            return;
        }
        if self.close_dialog
            || self.file_busy
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.macro_interaction_blocked()
        {
            return;
        }
        let input = if self.symbol_menu_open() {
            let input = if matches!(input, MenuEvent::Left) {
                MenuEvent::Dismiss
            } else if !self.symbol_menu_selection_visible {
                match input {
                    MenuEvent::Down => MenuEvent::First,
                    MenuEvent::Up => MenuEvent::Last,
                    input => input,
                }
            } else {
                input
            };
            if matches!(
                &input,
                MenuEvent::Up
                    | MenuEvent::Down
                    | MenuEvent::First
                    | MenuEvent::Last
                    | MenuEvent::Mnemonic(_)
                    | MenuEvent::Hover { .. }
            ) {
                self.symbol_menu_selection_visible = true;
            }
            input
        } else {
            input
        };
        let input = if self.menu_anchor.is_some()
            && (self.menu_submenu_only || self.menu_bar.popup_count() == 1)
            && matches!(&input, MenuEvent::Escape)
        {
            MenuEvent::Dismiss
        } else {
            input
        };
        if matches!(
            &input,
            MenuEvent::ToggleBar
                | MenuEvent::OpenContext(_, _)
                | MenuEvent::OpenRoot(_)
                | MenuEvent::OpenSubmenu(_)
                | MenuEvent::OpenSubmenuLimited { .. }
                | MenuEvent::ToggleRoot(_)
                | MenuEvent::HoverRoot(_)
        ) {
            self.menu_anchor = None;
            self.menu_submenu_only = false;
        }
        let old = self.menu_bar.mode();
        let root = match &input {
            MenuEvent::OpenContext(root, _) | MenuEvent::OpenRoot(root) | MenuEvent::ToggleRoot(root) | MenuEvent::HoverRoot(root) => *root,
            _ => self.menu_bar.active_root(),
        };
        if self.menu_bar.definitions().get(root).is_some_and(|menu| menu.selector == "search-menu-trigger") {
            self.refresh_builtin_syntax();
        }
        let availability = menu_surface::availability(self, window, cx);
        let input = if self.menu_bar.mode() == MenuMode::Open
            && self.menu_highlight_depth < self.menu_bar.popup_count()
        {
            match input { MenuEvent::Down => MenuEvent::First, MenuEvent::Up => MenuEvent::Last, other => other }
        } else { input };
        self.menu_highlight_depth = match &input {
            MenuEvent::ToggleRoot(_) | MenuEvent::HoverRoot(_) | MenuEvent::OpenContext(_, _) | MenuEvent::OpenDocuments(_) => 0,
            MenuEvent::Hover { path, .. } | MenuEvent::Click { path, .. } | MenuEvent::ClickAtRevision { path, .. } => path.len(),
            MenuEvent::Dismiss => 0,
            _ => usize::MAX,
        };
        let effect = self
            .menu_bar
            .handle(input, &|command| availability.allows(command));
        let state = self.menu_bar.mode();
        if state != MenuMode::Open {
            self.menu_anchor = None;
            self.menu_submenu_only = false;
        }
        if old == MenuMode::Closed && state != MenuMode::Closed {
            self.editor_dragging = false;
            self.previous_focus = window.focused(cx);
            self.menu_focus.focus(window, cx);
        } else if old != MenuMode::Closed && state == MenuMode::Closed {
            if let Some(focus) = self.previous_focus.take() {
                focus.focus(window, cx);
            } else {
                self.focus.focus(window, cx);
            }
        }
        menu_surface::scroll_selected(self, window);
        cx.notify();
        match effect {
            MenuOutcome::Activate(MenuCommand::ActivateDocument(id)) => {
                if self
                    .workspace
                    .view_buffer_ids(self.workspace.active_view())
                    .contains(&id)
                {
                    self.activate_buffer(id, window, cx);
                }
            }
            MenuOutcome::Activate(MenuCommand::DocumentDropdown) => {
                self.open_document_dropdown(window.mouse_position(), window, cx);
            }
            MenuOutcome::Activate(MenuCommand::Binary(command)) => {
                self.binary_clipboard(command, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::CharacterPanel) => {
                self.toggle_character_panel(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::CopyPath(command)) => {
                self.copy_document_path(command, cx)
            }
            MenuOutcome::Activate(MenuCommand::TextTransform(id)) => {
                self.text_transform_command(id, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Source(id)) => {
                match id {
                    48016 => self.show_shortcut_mapper(1, window, cx),
                    48017 => self.show_shortcut_mapper(2, window, cx),
                    21000..=21999 => self.activate_shortcut_command(crate::shortcuts::CommandKey::Run((id - 21000) as usize), window, cx),
                    _ => self.execute_source_command(id, window, cx),
                }
            }
            MenuOutcome::Activate(MenuCommand::MultiSelection(command)) => {
                self.multi_selection_command(command, window, cx);
            }
            MenuOutcome::Activate(MenuCommand::ColumnEditor) => self.show_column_editor(window, cx),
            MenuOutcome::Activate(MenuCommand::Intelligence(command)) => {
                use crate::menu::IntelligenceCommand;
                match command {
                    IntelligenceCommand::Function => self.complete_api(window, cx),
                    IntelligenceCommand::Word => self.complete_word(window, cx),
                    IntelligenceCommand::Path => self.complete_path(window, cx),
                    IntelligenceCommand::Parameters => self.show_calltip(window, cx),
                    IntelligenceCommand::PreviousParameters
                    | IntelligenceCommand::NextParameters => {
                        let action = if command == IntelligenceCommand::PreviousParameters {
                            intelligence_surface::IntelligenceAction::PreviousOverload
                        } else {
                            intelligence_surface::IntelligenceAction::NextOverload
                        };
                        self.capture_intelligence_action(action, window, cx);
                    }
                }
            }
            MenuOutcome::Activate(MenuCommand::Document(command)) => {
                use crate::menu::DocumentCommand;
                match command {
                    DocumentCommand::Close(condition) => {
                        self.request_close_condition(condition, window, cx)
                    }
                    DocumentCommand::Pin => self.toggle_pin_tab(window, cx),
                    DocumentCommand::Color(color) => self.set_active_tab_color(color, cx),
                    DocumentCommand::SortTabs(sort, descending) => {
                        self.sort_document_tabs(sort, descending, cx)
                    }
                    DocumentCommand::Multiline => self.toggle_tab_multiline(window, cx),
                    DocumentCommand::Vertical => self.toggle_tab_vertical(window, cx),
                    DocumentCommand::Mru => self.toggle_document_mru(window, cx),
                    DocumentCommand::Switcher => self.toggle_document_switcher(window, cx),
                    DocumentCommand::RestoreLastClosed => self.restore_last_closed(window, cx),
                    DocumentCommand::SaveSession => self.start_save_session(window, cx),
                    DocumentCommand::LoadSession => self.start_load_session(window, cx),
                }
            }
            MenuOutcome::Activate(MenuCommand::ConvertEncoding(format)) => {
                let id = match format {
                    crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain) => 45010,
                    crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Bom) => 45011,
                    crate::files::UnicodeFormat::Utf16BeBom => 45012,
                    crate::files::UnicodeFormat::Utf16LeBom => 45013,
                };
                self.select_source_encoding(id, window, cx);
            }
            MenuOutcome::Activate(MenuCommand::SelectEncoding(format)) => {
                let id = match format {
                    crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain) => 45008,
                    crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Bom) => 45005,
                    crate::files::UnicodeFormat::Utf16BeBom => 45006,
                    crate::files::UnicodeFormat::Utf16LeBom => 45007,
                };
                self.select_source_encoding(id, window, cx);
            }
            MenuOutcome::None => {}
            MenuOutcome::Activate(MenuCommand::File(FileCommand::New)) => self.new_file(window, cx),
            MenuOutcome::Activate(MenuCommand::File(FileCommand::Open)) => {
                self.start_open(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::Save)) => {
                self.start_save(false, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::SaveAs)) => {
                self.start_save(true, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::SaveCopy)) => {
                self.start_save_copy(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::SaveAll)) => {
                self.start_save_all(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::Close)) => {
                self.request_close_current(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::CloseAll)) => {
                self.request_close_all_tabs(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::CloseLeft)) => {
                self.request_close_left(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::CloseRight)) => {
                self.request_close_right(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::Reload)) => {
                self.request_reload(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::Print)) => {
                self.print_document(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::File(FileCommand::Exit)) => {
                window.dispatch_action(Box::new(CloseWindow), cx)
            }
            MenuOutcome::Activate(MenuCommand::File(command)) => {
                self.file_menu_command(command, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Undo)) => {
                window.dispatch_action(Box::new(EditorUndo), cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Redo)) => {
                window.dispatch_action(Box::new(EditorRedo), cx)
            }
            MenuOutcome::Activate(MenuCommand::Search(SearchCommand::Find)) => {
                self.show_find(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Bookmark(command)) => {
                self.bookmark_command(command, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::ChangeHistory(command)) => {
                self.change_history_command(command, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::View(command)) => match command {
                ViewCommand::ProjectPanel(index) => {
                    if self.projects.visible(index) {
                        self.hide_project_panel(index, window, cx);
                    } else {
                        self.show_project_panel(index, window, cx);
                    }
                }
                ViewCommand::DocumentList => self.toggle_document_list(window, cx),
                ViewCommand::DocumentMap => self.toggle_document_map(window, cx),
                ViewCommand::FolderWorkspace => self.toggle_folder_workspace(window, cx),
                ViewCommand::FunctionList => self.toggle_function_list(window, cx),
                ViewCommand::Monitoring => self.toggle_monitoring(window, cx),
                ViewCommand::CloneToOtherView => self.clone_to_other_view(window, cx),
                ViewCommand::MoveToOtherView => self.move_to_other_view(window, cx),
                ViewCommand::MoveToNewInstance => self.open_new_instance(true, window, cx),
                ViewCommand::OpenInNewInstance => self.open_new_instance(false, window, cx),
                ViewCommand::SyncVertical => self.toggle_sync_vertical(window, cx),
                ViewCommand::SyncHorizontal => self.toggle_sync_horizontal(window, cx),
                ViewCommand::ZoomIn => {
                    self.set_zoom_delta(1, window, cx);
                }
                ViewCommand::ZoomOut => {
                    self.set_zoom_delta(-1, window, cx);
                }
                ViewCommand::ZoomReset => self.reset_editor_zoom(window, cx),
                ViewCommand::Whitespace
                | ViewCommand::EndOfLine
                | ViewCommand::Nonprinting
                | ViewCommand::ControlCharacters
                | ViewCommand::AllCharacters => {
                    self.toggle_editor_symbol(command, window, cx);
                }
                ViewCommand::IndentGuides => {
                    self.toggle_editor_indent_guides(window, cx);
                }
                ViewCommand::Summary => self.show_summary(window, cx),
                ViewCommand::MarkdownPreview => self.toggle_markdown_preview(window, cx),
                ViewCommand::WordWrap => {
                    self.toggle_editor_word_wrap(window, cx);
                }
            },
            MenuOutcome::Activate(MenuCommand::Search(SearchCommand::FindInFiles)) => {
                self.show_find_in_files(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Search(SearchCommand::Next)) => {
                self.find_next(false, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Search(SearchCommand::Previous)) => {
                self.find_next(true, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Search(SearchCommand::GoTo)) => {
                self.show_goto(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Search(SearchCommand::Replace)) => {
                self.show_replace(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Settings(SettingsCommand::Preferences)) => {
                self.show_preferences(window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Settings(SettingsCommand::StyleConfigurator)) => self.show_style_configurator(window, cx),
            MenuOutcome::Activate(MenuCommand::Settings(SettingsCommand::ShortcutMapper)) => self.show_shortcut_mapper(0, window, cx),
            MenuOutcome::Activate(MenuCommand::Settings(SettingsCommand::ImportStyleThemes)) => self.import_style_theme(window, cx),
            MenuOutcome::Activate(MenuCommand::Settings(SettingsCommand::EditContextMenu)) => self.edit_context_menu(window, cx),
            MenuOutcome::Activate(MenuCommand::Mime(operation)) => {
                self.mime_transform(operation, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Converter(operation)) => {
                self.converter_transform(operation, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::ExportHtml) => self.start_html_export(window, cx),
            MenuOutcome::Activate(MenuCommand::Window(command)) => {
                self.perform_window_command(command, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::About) => self.show_about(window, cx),
            MenuOutcome::Activate(MenuCommand::Run) => self.show_run(window, cx),
            MenuOutcome::Activate(MenuCommand::HashSelection(algorithm)) => {
                self.copy_selection_hash(algorithm, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Macro(command)) => {
                self.macro_command(command, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::SavedMacro(index)) => {
                self.play_saved_macro(index, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Language(language)) => {
                self.select_language(language, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::DefineUserLanguage) => self.show_udl(window, cx),
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Cut)) => {
                window.dispatch_action(Box::new(EditorCut), cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Copy)) => {
                window.dispatch_action(Box::new(EditorCopy), cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Paste)) => {
                window.dispatch_action(Box::new(EditorPaste), cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Delete)) => {
                self.record_macro_step(crate::macros::MacroStep::ClearSelection);
                let result = self.editor_mut().delete_forward();
                self.editor_changed(result, window, cx);
                self.editor_focus.focus(window, cx);
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::SelectAll)) => {
                window.dispatch_action(Box::new(EditorSelectAll), cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::DuplicateLine)) => {
                self.duplicate_editor(true, window, cx)
            }
            MenuOutcome::Activate(MenuCommand::Edit(EditCommand::ConvertEol(target))) => {
                if self.cancel_view_composition(window, cx) {
                    let result = self.editor_mut().convert_line_endings(target);
                    self.editor_changed(result, window, cx);
                }
            }
        }
    }
}

impl Render for MatrixWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.matrix_active() || matrix_surface::locked() {
            return self.render_matrix(window, cx);
        }
        let recent_entries = self.recent_file_menu_entries();
        self.menu_bar.set_recent_file_entries(recent_entries);
        let run_commands = self.shortcut_mapper.bindings.run_commands.iter().enumerate().map(|(index, command)| {
            (command.name.clone(), self.shortcut_mapper.bindings.shortcuts.iter().find(|item| item.key == crate::shortcuts::CommandKey::Run(index)).map(|item| item.label()).unwrap_or_default())
        }).collect();
        self.menu_bar.set_saved_run_commands(run_commands);
        self.refresh_udl_registry(cx);
        self.flush_udl_style_invalidation(window, cx);
        self.sync_window_manager(window, cx);
        self.sync_floating_windows(window, cx);
        self.sync_panel_windows(window, cx);
        self.refresh_function_list(window, cx);
        self.refresh_markdown_preview(cx);
        let title_document = if self.pref_bool("titleBar.short", false) {
            self.workspace.active().name()
        } else {
            self.disk_file().map(|file| file.path().display().to_string()).unwrap_or_else(|| self.workspace.active().name())
        };
        let title = format!(
            "{}{} - Notepad Matrix",
            if self.workspace.active().is_modified() {
                "*"
            } else {
                ""
            },
            title_document
        );
        window_caption::update_title(window, &title);
        let background = self.appearance_colour("DarkMode.customColorTop", if self.dark { 0x202020 } else { 0xffffff });
        let toolbar = self.appearance_colour("DarkMode.customColorMain", if self.dark { 0x2b2b2b } else { 0xffffff });
        let foreground = self.appearance_colour("DarkMode.customColorText", if self.dark { 0xeeeeee } else { 0x202020 });
        let hover_background = self.appearance_colour("DarkMode.customColorMenuHotTrack", if self.dark { 0x444444 } else { 0xdedede });
        let caption_active = window.is_window_active();
        let caption_background = if self.dark {
            toolbar
        } else if caption_active {
            window_caption::BACKGROUND
        } else {
            0xf0f0f0
        };
        let caption_foreground = if self.dark {
            0xeeeeee
        } else if caption_active {
            0xffffff
        } else {
            0x202020
        };
        self.menu_bar.set_window_documents(self.workspace.view_buffer_ids(self.workspace.active_view()).iter().filter_map(|id| {
            self.workspace.buffers().iter().find(|buffer| buffer.id() == *id).map(|buffer|
                (*id, format!("{}{}", buffer.name(), if buffer.is_modified() { "*" } else { "" })))
        }).collect());
        let menu = self.view_chrome_visible().then(|| menu_surface::render(self, window, cx));
        let editing_toolbar = self.view_chrome_visible().then(|| toolbar_surface::render(self, window, cx));
        self.reconcile_editor_panes();
        let editor = self.render_split_editors(window, cx);
        let docked_udl = self.render_docked_udl(window, cx);
        let panels_left = self.view_panels_other_pane_visible().then(|| self.render_panel_region(crate::panel_layout::DockSide::Left, cx));
        let panels_right = self.view_panels_other_pane_visible().then(|| self.render_panel_region(crate::panel_layout::DockSide::Right, cx));
        let panels_top = self.view_panels_other_pane_visible().then(|| self.render_panel_region(crate::panel_layout::DockSide::Top, cx));
        let panels_bottom = self.view_panels_other_pane_visible().then(|| self.render_panel_region(crate::panel_layout::DockSide::Bottom, cx));
        let files_results = self.render_file_results(cx);
        let incremental = self.render_incremental(cx);
        let (incremental_top, incremental_bottom) = if self.incremental_at_top() {
            (incremental, None)
        } else {
            (None, incremental)
        };
        let status_bar = (self.status_visible && self.view_tabs_status_visible()).then(|| self.render_status(cx));
        let button = |id: &'static str,
                      label: &'static str,
                      icon: caption_icons::Icon,
                      icon_id: &'static str,
                      width: f32| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(label)
                .w(px(width))
                .h(px(30.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .hover(move |style| style.bg(rgb(hover_background)))
                .child(caption_icons::render(icon, icon_id, if self.dark { 0xcccccc } else { caption_foreground }))
        };
        div()
            .id("matrix-window")
            .role(Role::Application)
            .aria_label("Notepad Matrix")
            .aria_description(if self.dark {
                "Dark color scheme"
            } else {
                "Light color scheme"
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::apply_shortcut_action))
            .on_action(cx.listener(|view, _: &CompleteWord, window, cx| {
                view.complete_word(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CompleteApi, window, cx| {
                view.complete_api(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CompletePath, window, cx| {
                view.complete_path(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowCalltip, window, cx| {
                view.show_calltip(window, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|view, _: &PreviousCalltipOverload, window, cx| {
                    view.capture_intelligence_action(
                        intelligence_surface::IntelligenceAction::PreviousOverload,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|view, _: &NextCalltipOverload, window, cx| {
                view.capture_intelligence_action(
                    intelligence_surface::IntelligenceAction::NextOverload,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|view, _: &EditorEnter, window, cx| {
                if view.capture_intelligence_action(
                    intelligence_surface::IntelligenceAction::Enter,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_action(cx.listener(|view, _: &EditorTab, window, cx| {
                if view.capture_intelligence_action(
                    intelligence_surface::IntelligenceAction::Tab,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_action(cx.listener(|view, _: &EditorUp, window, cx| {
                if view.capture_intelligence_action(
                    intelligence_surface::IntelligenceAction::Up,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_action(cx.listener(|view, _: &EditorDown, window, cx| {
                if view.capture_intelligence_action(
                    intelligence_surface::IntelligenceAction::Down,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_action(cx.listener(|view, _: &EditorEscape, window, cx| {
                if view.capture_intelligence_action(
                    intelligence_surface::IntelligenceAction::Escape,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|view, _: &ShowUdl, window, cx| {
                view.show_udl(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloneToOtherView, window, cx| {
                view.clone_to_other_view(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &MoveToOtherView, window, cx| {
                view.move_to_other_view(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleSyncVertical, window, cx| {
                view.toggle_sync_vertical(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleSyncHorizontal, window, cx| {
                view.toggle_sync_horizontal(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &EditorSelectAll, window, cx| {
                if view.batch_editing_blocked() {
                    cx.stop_propagation();
                    return;
                }
                view.record_macro_step(crate::macros::MacroStep::SelectAll);
                let end = ByteOffset(view.editor().display_snapshot().len());
                let result = view.editor_mut().select_display(Selection {
                    anchor: end,
                    head: ByteOffset(0),
                });
                view.editor_changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &EditorUndo, window, cx| {
                if !view.batch_editing_blocked() {
                    view.record_macro_step(crate::macros::MacroStep::Unsupported("Undo"));
                    view.editor_mut().undo();
                    view.editor_changed(Ok(()), window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &EditorRedo, window, cx| {
                if !view.batch_editing_blocked() {
                    view.record_macro_step(crate::macros::MacroStep::Unsupported("Redo"));
                    view.editor_mut().redo();
                    view.editor_changed(Ok(()), window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &QuitApplication, window, cx| {
                view.request_native_quit(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ReloadFile, window, cx| {
                view.request_reload(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseAllFiles, window, cx| {
                view.request_close_all_tabs(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseLeftFiles, window, cx| {
                view.request_close_left(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseRightFiles, window, cx| {
                view.request_close_right(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseCurrentFile, window, cx| {
                view.request_close_current(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CancelClose, window, cx| {
                view.cancel_close(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &DiscardClose, window, cx| {
                view.discard_close(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &SaveAndClose, window, cx| {
                view.save_and_close(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CancelSaveAll, window, cx| {
                view.cancel_save_all(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmSaveAll, window, cx| {
                view.confirm_save_all(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CancelReload, window, cx| {
                view.cancel_reload(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmReload, window, cx| {
                view.confirm_reload(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &SaveCopyFile, window, cx| {
                view.start_save_copy(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &SaveAllFiles, window, cx| {
                view.start_save_all(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &NewFile, window, cx| {
                view.new_file(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenFile, window, cx| {
                view.start_open(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &NextBuffer, window, cx| {
                view.cycle_buffer(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &PreviousBuffer, window, cx| {
                view.cycle_buffer(false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &MruNextBuffer, window, cx| {
                view.cycle_mru_buffer(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &MruPreviousBuffer, window, cx| {
                view.cycle_mru_buffer(false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CommitDocumentSwitcher, window, cx| {
                view.finish_document_switcher(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CancelDocumentSwitcher, window, cx| {
                view.finish_document_switcher(false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ReopenClosedFile, window, cx| {
                view.restore_last_closed(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &PinCurrentTab, window, cx| {
                view.toggle_pin_tab(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleMultilineTabs, window, cx| {
                view.toggle_tab_multiline(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleVerticalTabs, window, cx| {
                view.toggle_tab_vertical(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &SaveFile, window, cx| {
                view.start_save(false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &SaveFileAs, window, cx| {
                view.start_save(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleMenuBar, window, cx| {
                view.menu_input(MenuEvent::ToggleBar, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenFileMenu, window, cx| {
                view.open_menu_root("file-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenEditMenu, window, cx| {
                view.open_menu_root("edit-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenEncodingMenu, window, cx| {
                view.open_menu_root("encoding-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenViewMenu, window, cx| {
                view.open_menu_root("view-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleDocumentList, window, cx| {
                view.toggle_document_list(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleOverwrite, window, cx| {
                if !view.file_busy
                    && !view.close_dialog
                    && !view.save_all_dialog
                    && !view.batch_editing_blocked()
                {
                    view.overwrite_mode = !view.overwrite_mode;
                    let overwrite = view.overwrite_mode;
                    view.editor_mut().set_overwrite(overwrite);
                    view.editor_focus.focus(window, cx);
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ZoomIn, window, cx| {
                view.set_zoom_delta(1, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ZoomOut, window, cx| {
                view.set_zoom_delta(-1, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ZoomReset, window, cx| {
                view.reset_editor_zoom(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleAllCharacters, window, cx| {
                view.toggle_editor_all_characters(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleWordWrap, window, cx| {
                view.toggle_editor_word_wrap(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleIndentGuides, window, cx| {
                view.toggle_editor_indent_guides(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleDocumentMap, window, cx| {
                view.toggle_document_map(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleFolderWorkspace, window, cx| {
                view.toggle_folder_workspace(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleFunctionList, window, cx| {
                view.toggle_function_list(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleMonitoring, window, cx| {
                view.toggle_monitoring(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowSummary, window, cx| {
                view.show_summary(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenEolMenu, window, cx| {
                view.menu_input(MenuEvent::OpenSubmenu("edit-menu-eol"), window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenSearchMenu, window, cx| {
                view.open_menu_root("search-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowFind, window, cx| {
                view.show_find(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ToggleBookmark, window, cx| {
                view.bookmark_command(BookmarkCommand::Toggle, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &NextBookmark, window, cx| {
                view.bookmark_command(BookmarkCommand::Next, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &PreviousBookmark, window, cx| {
                view.bookmark_command(BookmarkCommand::Previous, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ClearBookmarks, window, cx| {
                view.bookmark_command(BookmarkCommand::ClearAll, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowFindInFiles, window, cx| {
                view.show_find_in_files(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindAllInFiles, window, cx| {
                view.start_file_search(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseFindInFiles, window, cx| {
                view.files_search.visible = false;
                view.editor_focus.focus(window, cx);
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowReplace, window, cx| {
                view.show_replace(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ReplaceOne, window, cx| {
                view.replace_one(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ReplaceAll, window, cx| {
                view.replace_all(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(
                |view, action: &text_transform_surface::TransformAction, window, cx| {
                    view.apply_transform_action(action, window, cx);
                    cx.stop_propagation();
                },
            ))
            .on_action(cx.listener(|view, _: &FindNext, window, cx| {
                view.find_next(false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|view, action: &mark_surface::SourceAction, window, cx| {
                    view.execute_source_command(action.0, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|view, _: &FindDialogNext, window, cx| {
                view.activate_find_default(false, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &find_surface::FindOptionAction, window, cx| {
                view.find_option_command(action.0, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &find_surface::FindButtonAction, window, cx| {
                view.find_button_command(action.0, action.1, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &find_surface::FindFieldWarning, _, cx| {
                view.find_field_warning(action.0, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CountFind, window, cx| {
                view.count_find(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindDialogReverse, window, cx| {
                view.activate_find_default(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindPrevious, window, cx| {
                view.find_next(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseFind, window, cx| {
                view.find_visible = false;
                view.editor_focus.focus(window, cx);
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowGoTo, window, cx| {
                view.show_goto(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmGoTo, window, cx| {
                view.confirm_goto(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseGoTo, window, cx| {
                view.clear_goto(window, cx);
                view.editor_focus.focus(window, cx);
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenSettingsMenu, window, cx| {
                view.open_menu_root("settings-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowPreferences, window, cx| {
                view.show_preferences(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &PrintFile, window, cx| {
                view.print_document(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowMarkdownPreview, window, cx| {
                view.toggle_markdown_preview(window, cx);
            }))
            .on_action(cx.listener(|view, _: &CloseMarkdownPreview, window, cx| {
                view.dismiss_floating(floating_windows::DialogKind::MarkdownPreview, window, cx);
            }))
            .on_action(cx.listener(|view, _: &ClosePreferences, window, cx| {
                view.dismiss_floating(floating_windows::DialogKind::Preferences, window, cx);
                cx.stop_propagation();
            }))
            .when(
                self.mime_enabled || self.converter_enabled || self.html_export_enabled,
                |root| {
                    root.on_action(cx.listener(|view, _: &OpenPluginsMenu, window, cx| {
                        view.open_menu_root("plugins-menu-trigger", window, cx);
                        cx.stop_propagation();
                    }))
                },
            )
            .when(self.mime_enabled, |root| {
                root.on_action(cx.listener(|view, _: &MimeEncode, window, cx| {
                    view.mime_transform(crate::mime::MimeOperation::Base64Encode, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &MimeEncodePadded, window, cx| {
                    view.mime_transform(crate::mime::MimeOperation::Base64EncodePadded, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &MimeDecode, window, cx| {
                    view.mime_transform(crate::mime::MimeOperation::Base64Decode, window, cx);
                    cx.stop_propagation();
                }))
            })
            .when(self.converter_enabled, |root| {
                root.on_action(cx.listener(|view, _: &ConverterEncode, window, cx| {
                    view.converter_transform(
                        crate::converter::ConverterOperation::AsciiToHex,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(
                    |view, _: &ConverterDecode, window, cx| {
                        view.converter_transform(
                            crate::converter::ConverterOperation::HexToAscii,
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    },
                ))
            })
            .when(self.html_export_enabled, |root| {
                root.on_action(cx.listener(|view, _: &ExportHtml, window, cx| {
                    view.start_html_export(window, cx);
                    cx.stop_propagation();
                }))
            })
            .on_action(cx.listener(|view, _: &OpenWindowMenu, window, cx| {
                view.open_menu_root("window-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenRunMenu, window, cx| {
                view.open_menu_root("run-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenToolsMenu, window, cx| {
                view.open_menu_root("tools-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenMacroMenu, window, cx| {
                view.open_menu_root("macro-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenLanguageMenu, window, cx| {
                view.open_menu_root("language-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &OpenHelpMenu, window, cx| {
                view.open_menu_root("help-menu-trigger", window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &MinimizeWindow, window, cx| {
                window.minimize_window();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &StartMacroRecording, window, cx| {
                view.macro_command(crate::macros::MacroCommand::StartRecording, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &StopMacroRecording, window, cx| {
                view.macro_command(crate::macros::MacroCommand::StopRecording, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &PlaybackMacro, window, cx| {
                view.macro_command(crate::macros::MacroCommand::Playback, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &SaveCurrentMacro, window, cx| {
                view.macro_command(crate::macros::MacroCommand::SaveCurrent, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &RunMacroMultiple, window, cx| {
                view.macro_command(crate::macros::MacroCommand::RunMultiple, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowRun, window, cx| {
                view.show_run(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmRun, window, cx| {
                view.activate_run_control(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CancelRun, window, cx| {
                view.cancel_run(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &RunFocusNext, window, cx| {
                view.move_run_focus(true, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &RunFocusPrevious, window, cx| {
                view.move_run_focus(false, window, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|view, _, _, _| {
                if view.alt_pressed {
                    view.alt_used = true;
                }
            }))
            .on_modifiers_changed(cx.listener(
                |view, event: &gpui::ModifiersChangedEvent, window, cx| {
                    if !event.modifiers.control && !event.modifiers.platform {
                        view.finish_document_switcher(true, window, cx);
                    }
                    if event.modifiers.alt {
                        if !view.alt_pressed {
                            view.alt_pressed = true;
                            view.alt_used = false;
                        }
                        if event.modifiers.control
                            || event.modifiers.shift
                            || event.modifiers.platform
                        {
                            view.alt_used = true;
                        }
                    } else if view.alt_pressed {
                        view.alt_pressed = false;
                        if !view.alt_used {
                            view.menu_input(MenuEvent::ToggleBar, window, cx);
                        }
                    }
                },
            ))
            .on_action(cx.listener(|view, _: &CloseWindow, window, cx| {
                view.request_close(window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|view, event, _, cx| view.resize_file_results(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, _| {
                    view.tab_scroll_repeat = None;
                    view.cancel_file_results_resize();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, _| {
                    view.tab_scroll_repeat = None;
                    view.cancel_file_results_resize();
                }),
            )
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(background))
            .text_color(rgb(foreground))
            .font_family("Helvetica")
            .text_size(px(13.))
            .children(self.view_chrome_visible().then(||
                div()
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .bg(rgb(caption_background))
                    .when(!self.dark, |caption| {
                        caption.border_b_1().border_color(rgb(0xf0f0f0))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .flex()
                            .items_center()
                            .px(px(8.))
                            .gap(px(5.))
                            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, cx| {
                                cx.stop_propagation();
                                window.start_window_move()
                            }))
                            .child(
                                gpui::img(toolbar_icons::ToolbarIcon::Application.image())
                                    .size(px(16.)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(caption_foreground))
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .id("window-caption-controls")
                            .w(px(140.))
                            .h(px(32.))
                            .pr(px(3.))
                            .flex_shrink_0()
                            .flex()
                            .gap(px(0.))
                            .child(
                                button(
                                    "window-minimize",
                                    "Minimize",
                                    caption_icons::Icon::Minimize,
                                    "window-minimize-icon",
                                    45.,
                                )
                                .on_click(|_, window, _| window.minimize_window()),
                            )
                            .child(
                                button(
                                    "window-maximize",
                                    if window.is_maximized() {
                                        "Restore"
                                    } else {
                                        "Maximize"
                                    },
                                    if window.is_maximized() {
                                        caption_icons::Icon::Restore
                                    } else {
                                        caption_icons::Icon::Maximize
                                    },
                                    "window-maximize-icon",
                                    46.,
                                )
                                .on_click(|_, window, _| window.zoom_window()),
                            )
                            .child(
                                button(
                                    "window-close",
                                    "Close",
                                    caption_icons::Icon::Close,
                                    "window-close-icon",
                                    46.,
                                )
                                .on_click(
                                    cx.listener(|view, _, window, cx| {
                                        view.request_close(window, cx)
                                    }),
                                ),
                            ),
                    )
            ))
            .children(menu)
            .children(editing_toolbar)
            .children(incremental_top)
            .child(
                div()
                    .id("editor-results-workspace")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .flex()
                    .flex_col()
                    .relative()
                    .child(gpui::canvas(|bounds, _, _| bounds, {
                        let entity = cx.entity();
                        let incremental_height = if incremental_bottom.is_some() { dialog_font::px(32.) } else { px(0.) };
                        move |bounds, _, _, cx| {
                            entity.update(cx, |view, cx| view.layout_file_results(bounds.size.height - incremental_height, cx));
                        }
                    }).absolute().size_full())
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .w_full()
                            .flex()
                            .flex_col()
                            .children(panels_top)
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h(px(0.))
                                    .w_full()
                                    .children(panels_left)
                                    .child(editor)
                                    .children(docked_udl)
                                    .children(panels_right),
                            )
                            .children(panels_bottom),
                    )
                    .children(incremental_bottom)
                    .when_some(files_results, |root, results| root.child(results)),
            )
            .when_some(status_bar, |root, status| root.child(status))
            .children(self.render_tab_preview(cx))
            .children(self.window_manager_blocks_document().then(|| div().id("window-manager-modal-shield")
                .absolute().inset_0().occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|view, _, window, cx| {
                    view.present_floating(floating_windows::DialogKind::WindowManager, window, cx);
                    cx.stop_propagation();
                }))))
            .children(self.view_restore_command().map(|id| div().id("view-restore-mode")
                .debug_selector(|| "view-restore-mode".into()).role(Role::Button).aria_label("退出当前显示模式")
                .absolute().top(px(2.)).right(px(2.)).px(px(8.)).py(px(3.))
                .bg(rgb(toolbar)).border_1().border_color(rgb(hover_background)).cursor_pointer()
                .on_click(cx.listener(move |view, _, window, cx| { view.execute_view_source(id, window, cx); }))
                .child("↙")))
            .into_any_element()
    }
}

fn hide_host_buttons(window: &Window) -> io::Result<()> {
    MainThreadMarker::new()
        .ok_or_else(|| io::Error::other("window controls require the main thread"))?;
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(io::Error::other("expected a macOS AppKit window"));
    };
    // The borrowed GPUI window handle keeps this NSView alive for this main-thread call.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let host = view
        .window()
        .ok_or_else(|| io::Error::other("GPUI view has no host window"))?;
    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = host.standardWindowButton(kind) {
            button.setHidden(true);
        }
    }
    Ok(())
}

fn discard_host_composition(window: &Window) -> io::Result<bool> {
    let handle = match HasWindowHandle::window_handle(window) {
        Ok(handle) => handle,
        Err(error) => return Err(io::Error::other(error.to_string())),
    };
    MainThreadMarker::new()
        .ok_or_else(|| io::Error::other("native composition requires the main thread"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(io::Error::other("expected a macOS AppKit window"));
    };
    // The GPUI borrow keeps this NSView alive during the main-thread input-context call.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    if let Some(context) = view.inputContext() {
        context.discardMarkedText();
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut launch = crate::launch::LaunchOptions::parse(std::env::args_os().skip(1))?;
    let home = crate::file_access::home_directory()?;
    crate::config_directory::initialize(&home)?;
    let mut settings = ConfigStore::under_home(&home).load_settings()?;
    let normalize_threshold = |settings: &mut serde_json::Value| {
        let mut changed = false;
        for section in ["largeFileRestriction", "large_file_restriction"] {
            for key in ["fileSizeMB", "file_size_mb"] {
                if let Some(value) = settings.get_mut(section).and_then(|section| section.get_mut(key))
                    && value.as_u64() == Some(0) {
                    *value = 200.into();
                    changed = true;
                }
            }
        }
        changed
    };
    if normalize_threshold(&mut settings) {
        settings = ConfigStore::under_home(&home).update_settings(|document| { normalize_threshold(document); Ok(()) })?;
    }
    let dark = settings["theme"] == "dark";
    let mime_enabled = settings["features"]["mime_tools"].as_bool().unwrap_or(true);
    let converter_enabled = settings["features"]["converter"].as_bool().unwrap_or(true);
    let html_export_enabled = settings["features"]["html_export"]
        .as_bool()
        .unwrap_or(true);
    let status_visible = settings
        .get("statusbar_visible")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let save_all_confirm = settings
        .get("save_all_confirm")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let mut recovery_options = crate::recovery::RecoveryOptions::from_settings(&settings)?;
    if launch.no_session {
        recovery_options.remember_last_session = false;
        recovery_options.enabled = false;
    }
    let large_file_restriction = crate::workspace::LargeFileRestriction::from_settings(&settings)?;
    crate::recovery::SaveBackupOptions::from_settings(&settings)?;
    crate::workspace::Workspace::new()
        .configure_recent_files(&settings)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let failure = Rc::new(RefCell::new(None));
    let result = failure.clone();
    let quit_guard: Rc<RefCell<Option<native_quit::QuitGuard>>> = Rc::new(RefCell::new(None));
    let guard_slot = quit_guard.clone();
    let native_paths: launch_request_surface::NativePaths = Default::default();
    let incoming_paths = native_paths.clone();
    let application_launched = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let incoming_launched = application_launched.clone();
    let application = gpui_platform::application();
    application.on_open_urls(move |urls| {
        let request = launch_request_surface::native_open_urls(urls,
            !incoming_launched.load(std::sync::atomic::Ordering::Acquire));
        incoming_paths.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push_back(request);
    });
    application.run(move |cx: &mut App| {
        if let Err(error) = launch_request_surface::take_startup_handoff(&native_paths, &mut launch) {
            *failure.borrow_mut() = Some(error);
            cx.quit();
            return;
        }
        let preparing_home = home.clone();
        let preparing_settings = settings.clone();
        let preparing = cx.background_executor().spawn(async move {
            let prepared = crate::launch::InstanceServer::prepare(&preparing_home, &mut launch, &preparing_settings);
            (prepared, launch)
        });
        cx.spawn(async move |cx| {
            let (prepared, launch) = preparing.await;
            cx.update(|cx| {
        let mut instance_server = match prepared {
            Ok(crate::launch::InstanceStartup::Forwarded) => { cx.quit(); return; }
            Ok(crate::launch::InstanceStartup::Primary(server)) => Some(server),
            Ok(crate::launch::InstanceStartup::Secondary) => None,
            Err(error) => { *failure.borrow_mut() = Some(error); cx.quit(); return; }
        };
        if launch.no_session {
            recovery_options.remember_last_session = false;
            recovery_options.enabled = false;
        }
        let recovered = if recovery_options.remember_last_session {
            crate::recovery::RecoveryManager::open_with_options(&home, recovery_options).map(Some)
        } else {
            Ok(None)
        };
        let mut recovery_bootstrap = match recovered {
            Ok(recovered) => recovered,
            Err(error) => { *failure.borrow_mut() = Some(error); cx.quit(); return; }
        };
        if let Some((_, workspace)) = &mut recovery_bootstrap
            && let Err(error) = workspace.configure_recent_files(&settings) {
            *failure.borrow_mut() = Some(io::Error::other(error.to_string()));
            cx.quit();
            return;
        }
        dialog_font::initialize(&settings, cx);
        let quit_control = Rc::new(native_quit::QuitControl::default());
        let root: Rc<RefCell<Option<gpui::WindowHandle<MatrixWindow>>>> =
            Rc::new(RefCell::new(None));
        let scheduled_root = root.clone();
        let async_cx = cx.to_async();
        let executor = cx.foreground_executor().clone();
        let scheduled_control = quit_control.clone();
        let schedule = Box::new(move || {
            let mut cx = async_cx.clone();
            let root = scheduled_root.clone();
            let control = scheduled_control.clone();
            executor
                .spawn(async move {
                    let handle = *root.borrow();
                    match handle {
                        Some(handle) => {
                            if let Err(error) = handle.update(&mut cx, |view, window, cx| {
                                view.request_native_quit(window, cx)
                            }) {
                                control.cancel();
                                eprintln!(
                                    "native Quit could not reach its document window: {error}"
                                );
                            }
                        }
                        None => {
                            control.cancel();
                            eprintln!(
                                "native Quit requested before document-window initialization"
                            );
                        }
                    }
                })
                .detach();
        });
        let installed = MainThreadMarker::new()
            .ok_or_else(|| io::Error::other("application delegate requires the main thread"))
            .and_then(|mtm| native_quit::QuitGuard::install(mtm, quit_control.clone(), schedule));
        match installed {
            Ok(guard) => *guard_slot.borrow_mut() = Some(guard),
            Err(error) => {
                *failure.borrow_mut() = Some(error);
                cx.quit();
                return;
            }
        }
        let mut original_bindings = Vec::new();
        original_bindings.extend(MatrixWindow::reload_bindings());
        original_bindings.extend(MatrixWindow::file_dialog_bindings());
        original_bindings.extend(menu_surface::bindings());
        if mime_enabled || converter_enabled || html_export_enabled {
            original_bindings.extend([KeyBinding::new("alt-p", OpenPluginsMenu, None)]);
        }
        original_bindings.extend(text_field::bindings());
        original_bindings.extend(find_surface::bindings());
        original_bindings.extend(editor_surface::bindings());
        original_bindings.extend(text_transform_surface::bindings());
        original_bindings.extend(mark_surface::bindings());
        original_bindings.extend(incremental_surface::bindings());
        original_bindings.extend(character_range_surface::bindings());
        original_bindings.extend(goto_surface::bindings());
        original_bindings.extend(column_editor_surface::bindings());
        original_bindings.extend(document_session_surface::bindings());
        original_bindings.extend([
            KeyBinding::new("ctrl-f", ShowFind, None),
            KeyBinding::new("cmd-p", PrintFile, None),
            KeyBinding::new("cmd-w", CloseWindow, Some("MatrixAuxiliary")),
            KeyBinding::new("escape", CloseWindow, Some("MatrixAuxiliary")),
            KeyBinding::new("tab", DialogFocusNext, Some("MatrixAuxiliary")),
            KeyBinding::new("shift-tab", DialogFocusPrevious, Some("MatrixAuxiliary")),
            KeyBinding::new("ctrl-f2", ToggleBookmark, None),
            KeyBinding::new("f2", NextBookmark, None),
            KeyBinding::new("shift-f2", PreviousBookmark, None),
            KeyBinding::new("ctrl-shift-f", ShowFindInFiles, None),
            KeyBinding::new("escape", CloseFindInFiles, Some("MatrixFindInFiles")),
            KeyBinding::new("enter", FindAllInFiles, Some("MatrixFindInFiles")),
            KeyBinding::new("ctrl-h", ShowReplace, None),
            KeyBinding::new("escape", CloseFind, Some("MatrixFind")),
            KeyBinding::new("ctrl-g", ShowGoTo, None),
            KeyBinding::new("ctrl-+", ZoomIn, Some("MatrixEditor")),
            KeyBinding::new("ctrl--", ZoomOut, Some("MatrixEditor")),
            KeyBinding::new("f5", ShowRun, None),
            KeyBinding::new("ctrl-shift-p", PlaybackMacro, None),
            KeyBinding::new("enter", ConfirmRun, Some("MatrixRun")),
            KeyBinding::new("escape", CancelRun, Some("MatrixRun")),
            KeyBinding::new("tab", RunFocusNext, Some("MatrixRun")),
            KeyBinding::new("shift-tab", RunFocusPrevious, Some("MatrixRun")),
            KeyBinding::new("space", ConfirmRun, Some("MatrixRunButton")),
            KeyBinding::new("enter", ConfirmGoTo, Some("MatrixGoTo")),
            KeyBinding::new("escape", CloseGoTo, Some("MatrixGoTo")),
            KeyBinding::new("escape", ClosePreferences, Some("MatrixPreferences")),
            KeyBinding::new("escape", CloseMarkdownPreview, Some("MatrixMarkdown")),
            KeyBinding::new("cmd-c", markdown_surface::CopyMarkdownPreview, Some("MatrixMarkdown")),
            KeyBinding::new("ctrl-c", markdown_surface::CopyMarkdownPreview, Some("MatrixMarkdown")),
            KeyBinding::new("cmd-a", markdown_surface::SelectAllMarkdownPreview, Some("MatrixMarkdown")),
            KeyBinding::new("ctrl-a", markdown_surface::SelectAllMarkdownPreview, Some("MatrixMarkdown")),
        ]);
        original_bindings.extend([
            KeyBinding::new("alt-f4", CloseWindow, None),
            KeyBinding::new("cmd-q", QuitApplication, None),
            KeyBinding::new("backspace", EditorBackspace, Some("MatrixEditor")),
            KeyBinding::new("shift-backspace", EditorBackspace, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-l", EditorLineDelete, Some("MatrixEditor")),
            KeyBinding::new("ctrl-d", EditorDuplicateSelection, Some("MatrixEditor")),
            KeyBinding::new("delete", EditorDelete, Some("MatrixEditor")),
            KeyBinding::new("left", EditorLeft, Some("MatrixEditor")),
            KeyBinding::new("right", EditorRight, Some("MatrixEditor")),
            KeyBinding::new("shift-left", EditorSelectLeft, Some("MatrixEditor")),
            KeyBinding::new("shift-right", EditorSelectRight, Some("MatrixEditor")),
            KeyBinding::new("ctrl-a", EditorSelectAll, Some("MatrixEditor")),
            KeyBinding::new("insert", ToggleOverwrite, Some("MatrixEditor")),
            KeyBinding::new("ctrl-z", EditorUndo, Some("MatrixEditor")),
            KeyBinding::new("alt-backspace", EditorUndo, Some("MatrixEditor")),
            KeyBinding::new("ctrl-y", EditorRedo, Some("MatrixEditor")),
            KeyBinding::new("ctrl-x", EditorCut, Some("MatrixEditor")),
            KeyBinding::new("ctrl-c", EditorCopy, Some("MatrixEditor")),
            KeyBinding::new("ctrl-v", EditorPaste, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-z", EditorRedo, Some("MatrixEditor")),
            KeyBinding::new("ctrl-s", SaveFile, Some("MatrixEditor")),
            KeyBinding::new("ctrl-n", NewFile, Some("MatrixEditor")),
            KeyBinding::new("ctrl-w", CloseCurrentFile, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-w", CloseAllFiles, Some("MatrixEditor")),
            KeyBinding::new("ctrl-r", ReloadFile, Some("MatrixEditor")),
            KeyBinding::new("ctrl-o", OpenFile, Some("MatrixEditor")),
            KeyBinding::new("ctrl-pagedown", NextBuffer, Some("MatrixEditor")),
            KeyBinding::new("ctrl-pageup", PreviousBuffer, Some("MatrixEditor")),
            KeyBinding::new("ctrl-tab", MruNextBuffer, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-tab", MruPreviousBuffer, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-t", ReopenClosedFile, Some("MatrixEditor")),
            KeyBinding::new("ctrl-enter", CompleteWord, Some("MatrixEditor")),
            KeyBinding::new("ctrl-space", CompleteApi, Some("MatrixEditor")),
            KeyBinding::new("ctrl-alt-space", CompletePath, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-space", ShowCalltip, Some("MatrixEditor")),
            KeyBinding::new("alt-up", PreviousCalltipOverload, Some("MatrixEditor")),
            KeyBinding::new("alt-down", NextCalltipOverload, Some("MatrixEditor")),
            KeyBinding::new("ctrl-alt-s", SaveFileAs, Some("MatrixEditor")),
            KeyBinding::new("ctrl-shift-s", SaveAllFiles, Some("MatrixEditor")),
            KeyBinding::new("enter", EditorEnter, Some("MatrixEditor")),
            KeyBinding::new("tab", EditorTab, Some("MatrixEditor")),
            KeyBinding::new("shift-tab", EditorBackTab, Some("MatrixEditor")),
            KeyBinding::new("escape", EditorEscape, Some("MatrixEditor")),
            KeyBinding::new("home", EditorHome, Some("MatrixEditor")),
            KeyBinding::new("end", EditorEnd, Some("MatrixEditor")),
            KeyBinding::new("shift-home", EditorSelectHome, Some("MatrixEditor")),
            KeyBinding::new("shift-end", EditorSelectEnd, Some("MatrixEditor")),
            KeyBinding::new("up", EditorUp, Some("MatrixEditor")),
            KeyBinding::new("down", EditorDown, Some("MatrixEditor")),
            KeyBinding::new("shift-up", EditorSelectUp, Some("MatrixEditor")),
            KeyBinding::new("shift-down", EditorSelectDown, Some("MatrixEditor")),
            KeyBinding::new("ctrl-home", EditorDocumentStart, Some("MatrixEditor")),
            KeyBinding::new("ctrl-end", EditorDocumentEnd, Some("MatrixEditor")),
            KeyBinding::new(
                "ctrl-shift-home",
                EditorSelectDocumentStart,
                Some("MatrixEditor"),
            ),
            KeyBinding::new(
                "ctrl-shift-end",
                EditorSelectDocumentEnd,
                Some("MatrixEditor"),
            ),
        ]);
        cx.bind_keys(original_bindings.clone());
        cx.set_quit_mode(gpui::QuitMode::LastWindowClosed);
        let bounds = Bounds::centered(None, size(px(1080.), px(668.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Notepad Matrix".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                app_owns_titlebar_drag: true,
                window_min_size: Some(size(px(480.), px(320.))),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let mut view = MatrixWindow::new(dark, window, cx);
                    view.save_all_confirm = save_all_confirm;
                    view.status_visible = status_visible;
                    view.mute_sounds = settings["muteSounds"].as_bool().unwrap_or(false);
                    view.settings_store = Some(ConfigStore::under_home(&home));
                    view.preferences.files.launch_no_session = launch.no_session;
                    view.initialize_localization(&mut settings, cx);
                    view.refresh_udl_registry(cx);
                    view.preferences.settings = settings.clone();
                    view.language_menu_compact = settings["langsExcluded"]["langMenuCompact"].as_bool().unwrap_or(true);
                    view.refresh_style_config(cx);
                    match crate::context_menu::ContextMenu::load(view.settings_store.as_ref()) {
                        Ok(menu) => view.context_menu = menu,
                        Err(error) => view.notify_editor_error(format!("弹出菜单无法读取：{error}"), cx),
                    }
                    view.set_tab_preview_enabled(
                        settings["docPeekOnTab"].as_bool().unwrap_or(false),
                        cx,
                    );
                    view.configure_intelligence(&settings);
                    view.configure_find(&settings);
                    if let Err(error) = view.configure_character_panel(&settings) {
                        view.notify_editor_error(
                            format!("Character panel settings could not be loaded: {error}"),
                            cx,
                        );
                    }
                    view.tab_multiline = settings
                        .get("tab_multiline")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    view.tab_vertical = settings
                        .get("tab_vertical")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    view.tab_compact_label_len = settings
                        .get("tabCompactLabelLen")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(10)
                        .min(257) as usize;
                    view.configure_editor_symbols(&settings);
                    view.line_copy_cut_without_selection = settings["lineCopyCutWithoutSelection"].as_bool().unwrap_or(true);
                    view.mru_switching = settings
                        .get("mru_switching")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true);
                    view.document_switcher = settings
                        .get("document_switcher")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true);
                    view.load_saved_macros(cx);
                    view.configure_embedded_tools(
                        mime_enabled,
                        converter_enabled,
                        html_export_enabled,
                    );
                    view.initialize_converter(window, cx);
                    view.native_quit = Some(quit_control.clone());
                    if let Err(error) = view.workspace.configure_recent_files(&settings) {
                        view.notify_editor_error(
                            format!("Could not load recent-file history: {error}"),
                            cx,
                        );
                    }
                    if let Some((recovery, workspace)) = recovery_bootstrap.take() {
                        view.install_recovery(recovery, workspace, window, cx);
                        view.start_recovery_timer(recovery_options.interval, cx);
                    }
                    view.workspace
                        .set_large_file_restriction(large_file_restriction);
                    view.initialize_change_history(&settings);
                    view.apply_preferences(&settings, window, cx);
                    view.workspace.synchronize_udl_names(view.udl.registry.named().map(|(id, definition)| (id, definition.name.clone())));
                    match view.editing_new_document_language() {
                        Ok(language) => view.workspace.set_new_document_language(language),
                        Err(error) => view.notify_editor_error(format!("无法读取新建文档设置：{error}"), cx),
                    }
                    view.begin_file_preferences_startup(&settings, &launch, window, cx);
                    view.initialize_scintilla_shortcut_integration(window, cx);
                    if launch.completion.is_some() || launch.language.is_some() || launch.udl_name.is_some() || launch.byte_code_page.is_some()
                        || launch.line.is_some()
                        || launch.column.is_some()
                        || launch.open_session
                    {
                        view.apply_launch_options(&launch, window, cx);
                    } else if !launch.files.is_empty() {
                        view.start_open_paths(launch.files.clone(), window, cx);
                    } else if native_paths.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty() {
                        view.complete_file_preferences_startup(window, cx);
                    }
                    let defaults = view.annotate_shortcut_bindings(original_bindings.clone());
                    if let Err(error) = view.set_shortcut_runtime_defaults(defaults, Vec::new(), cx) {
                        view.notify_editor_error(format!("快捷键无法初始化：{error}"), cx);
                    }
                    view.refresh_shortcuts(window, cx);
                    view.update_file_prompt_directory();
                    view.start_launch_receiver(instance_server.take(), native_paths.clone(), window, cx);
                    view
                })
            },
        );
        let prepared = opened
            .map_err(|error| io::Error::other(error.to_string()))
            .and_then(|handle| {
                *root.borrow_mut() = Some(handle);
                handle
                    .update(cx, |_, window, _| hide_host_buttons(window))
                    .map_err(|error| io::Error::other(error.to_string()))?
            });
        if let Err(error) = prepared {
            *failure.borrow_mut() = Some(error);
            quit_control.approve();
            cx.quit();
        } else {
            application_launched.store(true, std::sync::atomic::Ordering::Release);
            cx.activate(true);
        }
            });
        }).detach();
    });
    crate::file_access::shutdown();
    let failure = result.borrow_mut().take();
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(())
}
