use super::toolbar_icons::ToolbarIcon;
use super::*;

type ToolbarBinding = (
    MenuCommand,
    &'static str,
    &'static str,
    ToolbarIcon,
    Box<dyn gpui::Action>,
);

fn enabled(
    view: &MatrixWindow,
    command: MenuCommand,
    available: menu_surface::Availability,
) -> bool {
    !view.file_busy
        && !view.close_dialog
        && !view.save_all_dialog
        && view.reload_target.is_none()
        && !view.batch_editing_blocked()
        && available.allows(command)
        && match command {
            MenuCommand::File(FileCommand::Save) => view.workspace.active().is_modified(),
            MenuCommand::File(FileCommand::SaveAll) => !view.workspace.dirty_buffers().is_empty(),
            _ => true,
        }
}

pub(super) fn render(
    view: &MatrixWindow,
    window: &Window,
    cx: &mut Context<MatrixWindow>,
) -> AnyElement {
    if !view.pref_bool("ToolBar.visible", true) {
        return div().h(px(0.)).into_any_element();
    }
    let large = matches!(
        view.pref_string("ToolBar.iconSet", "standard").as_str(),
        "large" | "large2"
    );
    let icon_size = if large { 32. } else { 16. };
    let button_width = icon_size + 7.;
    let button_height = icon_size + 6.;
    let commands: Vec<ToolbarBinding> = vec![
        (
            MenuCommand::File(FileCommand::New),
            "toolbar-new",
            "New document",
            ToolbarIcon::New,
            Box::new(NewFile),
        ),
        (
            MenuCommand::File(FileCommand::Open),
            "toolbar-open",
            "Open document",
            ToolbarIcon::Open,
            Box::new(OpenFile),
        ),
        (
            MenuCommand::File(FileCommand::Save),
            "toolbar-save",
            "Save document",
            ToolbarIcon::Save,
            Box::new(SaveFile),
        ),
        (
            MenuCommand::File(FileCommand::SaveAll),
            "toolbar-save-all",
            "Save all documents",
            ToolbarIcon::SaveAll,
            Box::new(SaveAllFiles),
        ),
        (
            MenuCommand::File(FileCommand::Close),
            "toolbar-close",
            "Close document",
            ToolbarIcon::Close,
            Box::new(CloseCurrentFile),
        ),
        (
            MenuCommand::File(FileCommand::CloseAll),
            "toolbar-close-all",
            "Close all documents",
            ToolbarIcon::CloseAll,
            Box::new(CloseAllFiles),
        ),
        (
            MenuCommand::File(FileCommand::Print),
            "toolbar-print",
            "打印",
            ToolbarIcon::Print,
            Box::new(PrintFile),
        ),
        (
            MenuCommand::Edit(EditCommand::Cut),
            "toolbar-cut",
            "Cut selection or line",
            ToolbarIcon::Cut,
            Box::new(EditorCut),
        ),
        (
            MenuCommand::Edit(EditCommand::Copy),
            "toolbar-copy",
            "Copy selection or line",
            ToolbarIcon::Copy,
            Box::new(EditorCopy),
        ),
        (
            MenuCommand::Edit(EditCommand::Paste),
            "toolbar-paste",
            "Paste text",
            ToolbarIcon::Paste,
            Box::new(EditorPaste),
        ),
        (
            MenuCommand::Edit(EditCommand::Undo),
            "toolbar-undo",
            "Undo",
            ToolbarIcon::Undo,
            Box::new(EditorUndo),
        ),
        (
            MenuCommand::Edit(EditCommand::Redo),
            "toolbar-redo",
            "Redo",
            ToolbarIcon::Redo,
            Box::new(EditorRedo),
        ),
        (
            MenuCommand::Search(SearchCommand::Find),
            "toolbar-find",
            "Find text",
            ToolbarIcon::Find,
            Box::new(ShowFind),
        ),
        (
            MenuCommand::Search(SearchCommand::Replace),
            "toolbar-replace",
            "Replace text",
            ToolbarIcon::Replace,
            Box::new(ShowReplace),
        ),
        (
            MenuCommand::View(ViewCommand::ZoomIn),
            "toolbar-zoom-in",
            "Zoom in",
            ToolbarIcon::ZoomIn,
            Box::new(ZoomIn),
        ),
        (
            MenuCommand::View(ViewCommand::ZoomOut),
            "toolbar-zoom-out",
            "Zoom out",
            ToolbarIcon::ZoomOut,
            Box::new(ZoomOut),
        ),
        (
            MenuCommand::View(ViewCommand::SyncVertical),
            "toolbar-sync-vertical",
            "Synchronize vertical scrolling",
            ToolbarIcon::SyncVertical,
            Box::new(ToggleSyncVertical),
        ),
        (
            MenuCommand::View(ViewCommand::SyncHorizontal),
            "toolbar-sync-horizontal",
            "Synchronize horizontal scrolling",
            ToolbarIcon::SyncHorizontal,
            Box::new(ToggleSyncHorizontal),
        ),
        (
            MenuCommand::View(ViewCommand::WordWrap),
            "toolbar-word-wrap",
            "Word wrap",
            ToolbarIcon::WordWrap,
            Box::new(ToggleWordWrap),
        ),
        (
            MenuCommand::View(ViewCommand::AllCharacters),
            "toolbar-all-characters",
            "Show all characters",
            ToolbarIcon::AllCharacters,
            Box::new(ToggleAllCharacters),
        ),
        (
            MenuCommand::View(ViewCommand::IndentGuides),
            "toolbar-indent-guides",
            "Show indent guides",
            ToolbarIcon::IndentGuides,
            Box::new(ToggleIndentGuides),
        ),
        (
            MenuCommand::DefineUserLanguage,
            "toolbar-define-user-language",
            "Define your language",
            ToolbarIcon::DefineUserLanguage,
            Box::new(ShowUdl),
        ),
        (
            MenuCommand::View(ViewCommand::DocumentMap),
            "toolbar-document-map",
            "Document Map",
            ToolbarIcon::DocumentMap,
            Box::new(ToggleDocumentMap),
        ),
        (
            MenuCommand::View(ViewCommand::DocumentList),
            "toolbar-document-list",
            "Document List",
            ToolbarIcon::DocumentList,
            Box::new(ToggleDocumentList),
        ),
        (
            MenuCommand::View(ViewCommand::FunctionList),
            "toolbar-function-list",
            "Function List",
            ToolbarIcon::FunctionList,
            Box::new(ToggleFunctionList),
        ),
        (
            MenuCommand::View(ViewCommand::FolderWorkspace),
            "toolbar-folder-workspace",
            "Folder as Workspace",
            ToolbarIcon::FolderWorkspace,
            Box::new(ToggleFolderWorkspace),
        ),
        (
            MenuCommand::View(ViewCommand::Monitoring),
            "toolbar-monitoring",
            "Monitoring (tail -f)",
            ToolbarIcon::Monitoring,
            Box::new(ToggleMonitoring),
        ),
        (
            MenuCommand::Macro(crate::macros::MacroCommand::StartRecording),
            "toolbar-macro-start",
            "Start macro recording",
            ToolbarIcon::StartRecording,
            Box::new(StartMacroRecording),
        ),
        (
            MenuCommand::Macro(crate::macros::MacroCommand::StopRecording),
            "toolbar-macro-stop",
            "Stop macro recording",
            ToolbarIcon::StopRecording,
            Box::new(StopMacroRecording),
        ),
        (
            MenuCommand::Macro(crate::macros::MacroCommand::Playback),
            "toolbar-macro-playback",
            "Playback macro",
            ToolbarIcon::Playback,
            Box::new(PlaybackMacro),
        ),
        (
            MenuCommand::Macro(crate::macros::MacroCommand::RunMultiple),
            "toolbar-macro-repeat",
            "Run macro multiple times",
            ToolbarIcon::RunMacroMultiple,
            Box::new(RunMacroMultiple),
        ),
        (
            MenuCommand::Macro(crate::macros::MacroCommand::SaveCurrent),
            "toolbar-macro-save",
            "Save current recorded macro",
            ToolbarIcon::SaveMacro,
            Box::new(SaveCurrentMacro),
        ),
        (
            MenuCommand::View(ViewCommand::MarkdownPreview),
            "toolbar-markdown-preview",
            "Markdown Preview",
            ToolbarIcon::MarkdownPreview,
            Box::new(ShowMarkdownPreview),
        ),
    ];
    let background = view.appearance_colour("DarkMode.customColorMain", 0xffffff);
    let hover = view.appearance_colour("DarkMode.customColorMenuHotTrack", 0xdedede);
    let availability = menu_surface::availability(view, window, cx);
    let dark = view.dark;
    let locale = view.native_locale();
    let buttons = commands
        .into_iter()
        .map(|(command, id, label, icon, action)| {
            let label = command
                .source_id()
                .and_then(|id| crate::localization::menu_command(&locale, id))
                .unwrap_or(label);
            let available = enabled(view, command, availability);
            let disabled_art = !available
                && icon.has_disabled_fluent()
                && view.pref_string("ToolBar.iconSet", if dark { "small" } else { "standard" })
                    != "standard";
            let checked = match command {
                MenuCommand::DefineUserLanguage => Some(view.udl.visible),
                MenuCommand::View(ViewCommand::DocumentList) => Some(view.document_list_visible),
                MenuCommand::View(ViewCommand::DocumentMap) => Some(view.document_map_visible()),
                MenuCommand::View(ViewCommand::FolderWorkspace) => {
                    Some(view.folder_workspace_visible())
                }
                MenuCommand::View(ViewCommand::FunctionList) => Some(view.function_list_visible()),
                MenuCommand::View(ViewCommand::Monitoring) => Some(view.monitoring_active()),
                MenuCommand::View(ViewCommand::MarkdownPreview) => {
                    Some(view.markdown_preview.visible)
                }
                MenuCommand::View(ViewCommand::AllCharacters) => {
                    Some(view.editor_all_characters_shown())
                }
                MenuCommand::View(ViewCommand::IndentGuides) => {
                    Some(view.editor_indent_guides_shown())
                }
                MenuCommand::View(ViewCommand::WordWrap) => Some(view.editor_word_wrap_enabled()),
                MenuCommand::View(ViewCommand::SyncVertical) => Some(view.sync_vertical_enabled()),
                MenuCommand::View(ViewCommand::SyncHorizontal) => {
                    Some(view.sync_horizontal_enabled())
                }
                _ => None,
            };
            let button = div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(label)
                .tooltip(move |_, cx| icon_tooltip::view(label, dark, cx))
                .aria_keyshortcuts(command.shortcut())
                .when_some(checked, |button, checked| {
                    button.aria_toggled(checked.into())
                })
                .a11y_synthetic_children(move |builder| {
                    if !available {
                        builder.parent_node().set_disabled();
                    }
                })
                .w(px(button_width))
                .h(px(button_height))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .when(checked == Some(true), |button| {
                    button
                        .bg(rgb(
                            view.appearance_colour("DarkMode.customColorActive", 0xcce8ff)
                        ))
                        .when(dark, |button| button.border_1().border_color(rgb(0x646464)))
                })
                .when(available, |button| {
                    button.hover(move |style| {
                        let style = style.bg(rgb(hover));
                        if dark {
                            style.border_1().border_color(rgb(0x9b9b9b))
                        } else {
                            style
                        }
                    })
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if enabled(view, command, menu_surface::availability(view, window, cx)) {
                        view.editor_focus.focus(window, cx);
                        window.dispatch_action(action.boxed_clone(), cx);
                    }
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .id((id, 0_usize))
                        .debug_selector(move || format!("{id}-icon"))
                        .size(px(icon_size))
                        .when(!available && !disabled_art, |icon| icon.opacity(0.35))
                        .child(gpui::img(icon.preference_image(view, cx, available)).size_full()),
                );
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .when(
                    matches!(
                        command,
                        MenuCommand::Edit(EditCommand::Cut | EditCommand::Undo)
                            | MenuCommand::Search(SearchCommand::Find)
                            | MenuCommand::View(
                                ViewCommand::ZoomIn
                                    | ViewCommand::SyncVertical
                                    | ViewCommand::WordWrap
                                    | ViewCommand::Monitoring
                                    | ViewCommand::MarkdownPreview
                            )
                            | MenuCommand::DefineUserLanguage
                            | MenuCommand::Macro(crate::macros::MacroCommand::StartRecording)
                    ),
                    |group| {
                        group.child(
                            div().w(px(7.)).h(px(16.)).flex_shrink_0().child(
                                div()
                                    .ml(px(3.))
                                    .h_full()
                                    .border_l_1()
                                    .border_color(rgb(if view.dark { 0x555555 } else { 0xc0c0c0 })),
                            ),
                        )
                    },
                )
                .child(button)
                .when(
                    command == MenuCommand::View(ViewCommand::AllCharacters),
                    |group| {
                        let anchor =
                            std::rc::Rc::new(std::cell::Cell::new(gpui::point(px(0.), px(0.))));
                        let measured = anchor.clone();
                        group.child(
                            div()
                                .id("toolbar-symbol-dropdown")
                                .debug_selector(|| "toolbar-symbol-dropdown".into())
                                .role(Role::Button)
                                .aria_label("显示符号")
                                .aria_expanded(view.symbol_menu_open())
                                .w(px(14.))
                                .h(px(22.))
                                .flex_shrink_0()
                                .relative()
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(checked == Some(true), |arrow| {
                                    arrow.bg(rgb(if dark { 0x383838 } else { 0xcce8ff }))
                                })
                                .when(available, |arrow| {
                                    arrow.hover(move |style| style.bg(rgb(hover)))
                                })
                                .a11y_synthetic_children(move |builder| {
                                    if !available {
                                        builder.parent_node().set_disabled();
                                    }
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if enabled(
                                        view,
                                        command,
                                        menu_surface::availability(view, window, cx),
                                    ) {
                                        view.open_symbol_dropdown(anchor.get(), window, cx);
                                    }
                                    cx.stop_propagation();
                                }))
                                .child(
                                    gpui::canvas(
                                        move |bounds, _, _| {
                                            measured.set(gpui::point(
                                                bounds.left() - px(23.),
                                                bounds.bottom(),
                                            ));
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .size_full(),
                                )
                                .child("▾"),
                        )
                    },
                )
        })
        .collect::<Vec<_>>();
    div()
        .id("editing-toolbar")
        .debug_selector(|| "editing-toolbar".into())
        .role(Role::Toolbar)
        .aria_label("Editing toolbar")
        .h(px(button_height + 4.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(px(5.))
        .bg(rgb(background))
        .border_b_1()
        .border_color(rgb(0x808080))
        .children(buttons)
        .into_any_element()
}
