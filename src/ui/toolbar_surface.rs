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

pub(super) fn render(view: &MatrixWindow, cx: &mut Context<MatrixWindow>) -> AnyElement {
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
    ];
    let background = if view.dark { 0x2b2b2b } else { 0xffffff };
    let hover = if view.dark { 0x444444 } else { 0xdedede };
    let availability = menu_surface::availability(view, cx);
    let dark = view.dark;
    let buttons = commands
        .into_iter()
        .map(|(command, id, label, icon, action)| {
            let available = enabled(view, command, availability);
            let checked = match command {
                MenuCommand::View(ViewCommand::DocumentList) => Some(view.document_list_visible),
                MenuCommand::View(ViewCommand::DocumentMap) => Some(view.document_map_visible()),
                MenuCommand::View(ViewCommand::FolderWorkspace) => {
                    Some(view.folder_workspace_visible())
                }
                MenuCommand::View(ViewCommand::FunctionList) => Some(view.function_list_visible()),
                MenuCommand::View(ViewCommand::Monitoring) => Some(view.monitoring_active()),
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
                .tooltip(move |_, cx| icon_tooltip::view(command.label(), dark, cx))
                .aria_keyshortcuts(command.shortcut())
                .when_some(checked, |button, checked| {
                    button.aria_toggled(checked.into())
                })
                .a11y_synthetic_children(move |builder| {
                    if !available {
                        builder.parent_node().set_disabled();
                    }
                })
                .w(px(23.))
                .h(px(22.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .when(checked == Some(true), |button| {
                    button.bg(rgb(if view.dark { 0x404b5a } else { 0xcce8ff }))
                })
                .when(available, |button| {
                    button.hover(move |style| style.bg(rgb(hover)))
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if enabled(view, command, menu_surface::availability(view, cx)) {
                        view.editor_focus.focus(window, cx);
                        window.dispatch_action(action.boxed_clone(), cx);
                    }
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .id((id, 0_usize))
                        .debug_selector(move || format!("{id}-icon"))
                        .size(px(16.))
                        .when(!available, |icon| icon.opacity(0.35))
                        .child(gpui::img(icon.image()).size_full()),
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
                                    | ViewCommand::WordWrap
                                    | ViewCommand::DocumentMap
                            )
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
        })
        .collect::<Vec<_>>();
    div()
        .id("editing-toolbar")
        .role(Role::Toolbar)
        .aria_label("Editing toolbar")
        .h(px(25.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(px(2.))
        .bg(rgb(background))
        .border_b_1()
        .border_color(rgb(0x808080))
        .children(buttons)
        .into_any_element()
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn icon_hover_describes_actions_without_activating_or_editing(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        for (id, tooltip) in [
            ("toolbar-new", "icon-tooltip-New"),
            ("toolbar-save", "icon-tooltip-Save"),
            ("window-minimize", "icon-tooltip-Minimize"),
            ("window-maximize", "icon-tooltip-Maximize"),
            ("window-close", "icon-tooltip-Close"),
            ("menu-new-document", "icon-tooltip-New document"),
            ("menu-document-list", "icon-tooltip-Toggle Document List"),
            ("menu-close-document", "icon-tooltip-Close document"),
            ("buffer-tab-pin-0-1", "icon-tooltip-Pin tab"),
            ("buffer-tab-close-1", "icon-tooltip-Close"),
        ] {
            let button = cx.debug_bounds(id).expect("icon button");
            cx.simulate_mouse_move(button.center(), None, gpui::Modifiers::default());
            cx.run_until_parked();
            cx.dispatcher
                .advance_clock(std::time::Duration::from_secs(1));
            cx.run_until_parked();
            assert!(
                cx.debug_bounds(tooltip).is_some(),
                "missing tooltip for {id}"
            );
            let editor = cx.debug_bounds("matrix-editor").unwrap();
            cx.simulate_mouse_move(editor.center(), None, gpui::Modifiers::default());
            cx.run_until_parked();
            assert!(cx.debug_bounds(tooltip).is_none());
        }
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.buffers().len(), 1);
            assert_eq!(view.editor().committed_text(), "");
            assert_eq!(view.editor().selection(), Selection::default());
            assert!(!view.editor().document().can_undo());
            assert!(!view.workspace.active().is_modified());
        });
        assert_eq!(cx.windows().len(), 1);
        let new = cx.debug_bounds("toolbar-new").unwrap();
        cx.simulate_mouse_move(new.center(), None, gpui::Modifiers::default());
        cx.run_until_parked();
        cx.dispatcher
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        assert!(cx.debug_bounds("icon-tooltip-New").is_some());
        cx.simulate_click(new.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert_eq!(view.workspace.buffers().len(), 2));
        assert!(cx.debug_bounds("icon-tooltip-New").is_none());
    }
}
