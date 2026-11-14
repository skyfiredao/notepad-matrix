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
    let background = if view.dark { 0x202020 } else { 0xffffff };
    let hover = if view.dark { 0x454545 } else { 0xdedede };
    let availability = menu_surface::availability(view, cx);
    let dark = view.dark;
    let buttons = commands
        .into_iter()
        .map(|(command, id, label, icon, action)| {
            let available = enabled(view, command, availability);
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
                    button
                        .bg(rgb(if dark { 0x383838 } else { 0xcce8ff }))
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
                                    if enabled(view, command, menu_surface::availability(view, cx))
                                    {
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
        .h(px(26.))
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

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn toolbar_order_and_group_boundaries_follow_frozen_source(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let buttons = [
            ("toolbar-new", false),
            ("toolbar-open", false),
            ("toolbar-save", false),
            ("toolbar-save-all", false),
            ("toolbar-close", false),
            ("toolbar-close-all", false),
            ("toolbar-cut", true),
            ("toolbar-copy", false),
            ("toolbar-paste", false),
            ("toolbar-undo", true),
            ("toolbar-redo", false),
            ("toolbar-find", true),
            ("toolbar-replace", false),
            ("toolbar-zoom-in", true),
            ("toolbar-zoom-out", false),
            ("toolbar-sync-vertical", true),
            ("toolbar-sync-horizontal", false),
            ("toolbar-word-wrap", true),
            ("toolbar-all-characters", false),
            ("toolbar-indent-guides", false),
            ("toolbar-define-user-language", true),
            ("toolbar-document-map", false),
            ("toolbar-document-list", false),
            ("toolbar-function-list", false),
            ("toolbar-folder-workspace", false),
            ("toolbar-monitoring", true),
            ("toolbar-macro-start", true),
            ("toolbar-macro-stop", false),
            ("toolbar-macro-playback", false),
            ("toolbar-macro-repeat", false),
            ("toolbar-macro-save", false),
        ];
        for pair in buttons.windows(2) {
            let previous = cx.debug_bounds(pair[0].0).expect("source toolbar button");
            let current = cx.debug_bounds(pair[1].0).expect("source toolbar button");
            assert_eq!(current.top(), previous.top());
            assert_eq!(current.size.width, px(23.));
            assert_eq!(
                current.left() - previous.right(),
                px(if pair[1].1 { 7. } else { 0. }),
                "source group before {}",
                pair[1].0
            );
        }
        assert!(cx.debug_bounds("toolbar-print").is_none());
    }

    #[gpui::test]
    fn every_source_toolbar_tooltip_preserves_document_identity_and_history(
        app: &mut gpui::TestAppContext,
    ) {
        for dark in [false, true] {
            let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            cx.simulate_resize(size(px(1080.), px(668.)));
            cx.simulate_input("first");
            cx.dispatch_action(NewFile);
            cx.simulate_input("second");
            cx.dispatch_action(EditorDocumentStart);
            cx.simulate_input("X");
            cx.dispatch_action(EditorUndo);
            let snapshot = |view: &MatrixWindow| {
                (
                    view.workspace.active().id(),
                    view.workspace.active_view(),
                    view.workspace
                        .buffers()
                        .iter()
                        .map(|buffer| {
                            (
                                buffer.id(),
                                buffer.editor.committed_text(),
                                buffer.editor.selections(),
                                buffer.editor.document().can_undo(),
                                buffer.editor.document().can_redo(),
                                buffer.is_modified(),
                            )
                        })
                        .collect::<Vec<_>>(),
                    (
                        view.editor_zoom_delta(),
                        view.editor_word_wrap_enabled(),
                        view.editor_all_characters_shown(),
                        view.editor_indent_guides_shown(),
                        view.sync_vertical_enabled(),
                        view.sync_horizontal_enabled(),
                    ),
                    (
                        view.udl.visible,
                        view.document_list_visible,
                        view.document_map_visible(),
                        view.function_list_visible(),
                        view.folder_workspace_visible(),
                        view.panels.layout.to_json(),
                    ),
                    (
                        view.macro_recording,
                        view.macro_playing,
                        view.macro_steps.clone(),
                        view.macro_dialogs.save_visible,
                        view.macro_dialogs.repeat_visible,
                        view.monitoring_active(),
                    ),
                )
            };
            let before = view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "second");
                assert!(view.editor().document().can_undo());
                assert!(view.editor().document().can_redo());
                snapshot(view)
            });
            let windows = cx.windows().len();
            let clipboard =
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
            for (id, tooltip) in [
                ("toolbar-new", "icon-tooltip-New"),
                ("toolbar-open", "icon-tooltip-Open..."),
                ("toolbar-save", "icon-tooltip-Save"),
                ("toolbar-save-all", "icon-tooltip-Save All"),
                ("toolbar-close", "icon-tooltip-Close"),
                ("toolbar-close-all", "icon-tooltip-Close All"),
                ("toolbar-cut", "icon-tooltip-Cut"),
                ("toolbar-copy", "icon-tooltip-Copy"),
                ("toolbar-paste", "icon-tooltip-Paste"),
                ("toolbar-undo", "icon-tooltip-Undo"),
                ("toolbar-redo", "icon-tooltip-Redo"),
                ("toolbar-find", "icon-tooltip-Find..."),
                ("toolbar-replace", "icon-tooltip-Replace..."),
                (
                    "toolbar-zoom-in",
                    "icon-tooltip-Zoom In (Ctrl+Mouse Wheel Up)",
                ),
                (
                    "toolbar-zoom-out",
                    "icon-tooltip-Zoom Out (Ctrl+Mouse Wheel Down)",
                ),
                (
                    "toolbar-sync-vertical",
                    "icon-tooltip-Synchronize Vertical Scrolling",
                ),
                (
                    "toolbar-sync-horizontal",
                    "icon-tooltip-Synchronize Horizontal Scrolling",
                ),
                ("toolbar-word-wrap", "icon-tooltip-Word wrap"),
                ("toolbar-all-characters", "icon-tooltip-Show All Characters"),
                ("toolbar-indent-guides", "icon-tooltip-Show Indent Guide"),
                (
                    "toolbar-define-user-language",
                    "icon-tooltip-Define your language...",
                ),
                ("toolbar-document-map", "icon-tooltip-Document Map"),
                ("toolbar-document-list", "icon-tooltip-Document List"),
                ("toolbar-function-list", "icon-tooltip-Function List"),
                (
                    "toolbar-folder-workspace",
                    "icon-tooltip-Folder as Workspace",
                ),
                ("toolbar-monitoring", "icon-tooltip-Monitoring (tail -f)"),
                ("toolbar-macro-start", "icon-tooltip-Start Recording"),
                ("toolbar-macro-stop", "icon-tooltip-Stop Recording"),
                ("toolbar-macro-playback", "icon-tooltip-Playback"),
                (
                    "toolbar-macro-repeat",
                    "icon-tooltip-Run a Macro Multiple Times...",
                ),
                (
                    "toolbar-macro-save",
                    "icon-tooltip-Save Current Recorded Macro...",
                ),
            ] {
                let button = cx.debug_bounds(id).expect("source toolbar button");
                cx.simulate_mouse_move(button.center(), None, gpui::Modifiers::default());
                cx.dispatcher
                    .advance_clock(std::time::Duration::from_secs(1));
                cx.run_until_parked();
                assert!(
                    cx.debug_bounds(tooltip).is_some(),
                    "source tooltip for {id}"
                );
                view.update_in(cx, |view, window, _| {
                    assert!(view.editor_focus.is_focused(window));
                    assert_eq!(snapshot(view), before, "hover cannot edit/activate: {id}");
                });
                assert_eq!(cx.windows().len(), windows);
                assert_eq!(
                    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                    clipboard,
                    "hover cannot copy or cut: {id}"
                );
                let editor = cx.debug_bounds("matrix-editor").expect("own editor");
                cx.simulate_mouse_move(editor.center(), None, gpui::Modifiers::default());
                cx.run_until_parked();
                assert!(cx.debug_bounds(tooltip).is_none());
                view.read_with(cx, |view, _| {
                    assert_eq!(snapshot(view), before, "leave cannot activate: {id}")
                });
            }
            cx.dispatch_action(EditorRedo);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "Xsecond");
            });
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| assert_eq!(snapshot(view), before));
        }
    }

    #[gpui::test]
    fn toolbar_press_release_and_outside_cancel_preserve_original_document(
        app: &mut gpui::TestAppContext,
    ) {
        for dark in [false, true] {
            let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            cx.simulate_resize(size(px(1080.), px(668.)));
            cx.simulate_input("original");
            let original = view.read_with(cx, |view, _| {
                (
                    view.workspace.active().id(),
                    view.editor().committed_text(),
                    view.editor().selections(),
                    view.editor().document().can_undo(),
                    view.editor().document().can_redo(),
                    view.workspace.active().is_modified(),
                )
            });
            let windows = cx.windows().len();
            let button = cx.debug_bounds("toolbar-new").expect("enabled New button");
            let source_quad = |cx: &mut gpui::VisualTestContext,
                               selector: &'static str,
                               colour,
                               edge: Option<u32>| {
                let bounds = cx.debug_bounds(selector).expect("source toolbar region");
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.update(|window, _| {
                    let bounds = window
                        .pixel_snap_bounds(bounds)
                        .scale(window.scale_factor());
                    let fill: gpui::Background = rgb(colour).into();
                    let quads = window.painted_quads();
                    assert!(
                        quads.iter().any(|quad| quad.bounds == bounds && quad.background == fill)
                            && edge.is_none_or(|edge| quads.iter().any(|quad| quad.bounds == bounds
                                && quad.border_color == rgb(edge).into()
                                && quad.border_widths.top > px(0.).scale(window.scale_factor()))),
                        "source default dark toolbar palette: {selector}, expected bounds={bounds:?}, candidates={:?}",
                        window.painted_quads().iter().filter(|quad| quad.bounds.intersects(&bounds)).map(|quad| (quad.bounds, quad.background, quad.border_color)).collect::<Vec<_>>()
                    );
                });
            };
            if dark {
                source_quad(cx, "editing-toolbar", 0x202020, None);
                view.read_with(cx, |view, _| assert!(view.editor_indent_guides_shown()));
                source_quad(cx, "toolbar-indent-guides", 0x383838, Some(0x646464));
                cx.simulate_mouse_move(button.center(), None, gpui::Modifiers::default());
                source_quad(cx, "toolbar-new", 0x454545, Some(0x9b9b9b));
            }
            cx.simulate_event(gpui::MouseDownEvent {
                position: button.center(),
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            });
            if dark {
                source_quad(cx, "toolbar-new", 0x454545, Some(0x9b9b9b));
            }
            assert_eq!(cx.debug_bounds("toolbar-new"), Some(button));
            view.update_in(cx, |view, window, _| {
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(view.workspace.buffers().len(), 1);
                assert_eq!(
                    (
                        view.workspace.active().id(),
                        view.editor().committed_text(),
                        view.editor().selections(),
                        view.editor().document().can_undo(),
                        view.editor().document().can_redo(),
                        view.workspace.active().is_modified(),
                    ),
                    original
                );
            });
            assert_eq!(cx.windows().len(), windows);
            let editor = cx.debug_bounds("matrix-editor").expect("own editor");
            cx.simulate_mouse_move(
                editor.center(),
                Some(MouseButton::Left),
                gpui::Modifiers::default(),
            );
            cx.simulate_event(gpui::MouseUpEvent {
                position: editor.center(),
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            });
            view.update_in(cx, |view, window, _| {
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(view.workspace.buffers().len(), 1);
                assert_eq!(
                    (
                        view.workspace.active().id(),
                        view.editor().committed_text(),
                        view.editor().selections(),
                        view.editor().document().can_undo(),
                        view.editor().document().can_redo(),
                        view.workspace.active().is_modified(),
                    ),
                    original
                );
            });
            assert_eq!(cx.windows().len(), windows);
            cx.simulate_click(button.center(), gpui::Modifiers::default());
            view.update_in(cx, |view, window, _| {
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(view.workspace.buffers().len(), 2);
                assert_ne!(view.workspace.active().id(), original.0);
                assert_eq!(view.editor().committed_text(), "");
                assert!(!view.editor().document().can_undo());
                assert!(!view.workspace.active().is_modified());
                let old = view
                    .workspace
                    .buffers()
                    .iter()
                    .find(|buffer| buffer.id() == original.0)
                    .expect("original retained");
                assert_eq!(
                    (
                        old.id(),
                        old.editor.committed_text(),
                        old.editor.selections(),
                        old.editor.document().can_undo(),
                        old.editor.document().can_redo(),
                        old.is_modified(),
                    ),
                    original
                );
            });
            assert_eq!(cx.windows().len(), windows);
            let created = view.read_with(cx, |view, _| view.workspace.active().id());
            cx.simulate_input("new");
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "new");
                let old = view
                    .workspace
                    .buffers()
                    .iter()
                    .find(|buffer| buffer.id() == original.0)
                    .expect("original still separate");
                assert_eq!(old.editor.committed_text(), "original");
            });
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "");
                assert!(!view.workspace.active().is_modified());
            });
            let old_tab = cx
                .debug_bounds("buffer-tab-1")
                .expect("original document tab");
            cx.simulate_click(old_tab.center(), gpui::Modifiers::default());
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().id(), original.0);
                assert_eq!(view.editor().committed_text(), "");
                assert!(!view.workspace.active().is_modified());
                let new = view
                    .workspace
                    .buffers()
                    .iter()
                    .find(|buffer| buffer.id() == created)
                    .expect("new document retained");
                assert_eq!(new.editor.committed_text(), "");
                assert!(!new.is_modified());
                assert!(new.editor.document().can_redo());
            });
        }
    }

    #[test]
    fn udl_toolbar_checked_state_tracks_its_window() {
        let mut app = gpui::HeadlessAppContext::with_platform(
            std::sync::Arc::new(gpui::NoopTextSystem),
            std::sync::Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        let main = app
            .open_window(size(px(800.), px(480.)), |window, cx| {
                cx.new(|cx| MatrixWindow::new(false, window, cx))
            })
            .expect("offscreen main window");
        let mut strips = Vec::new();
        for step in 0..3 {
            if step != 0 {
                main.update(&mut app, |_, window, cx| {
                    window.dispatch_action(Box::new(ShowUdl), cx)
                })
                .expect("toggle UDL");
            }
            app.run_until_parked();
            app.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
                .expect("draw toolbar");
            let image = app.capture_screenshot(main.into()).expect("toolbar image");
            let scale = image.width() / 800;
            strips.push(
                image
                    .enumerate_pixels()
                    .filter(|(_, y, _)| (52..77).contains(&(y / scale)))
                    .map(|(_, _, pixel)| pixel.0)
                    .collect::<Vec<_>>(),
            );
        }
        assert!(
            strips[0] != strips[1],
            "opening UDL must visibly check its toolbar button"
        );
        assert!(
            strips[0] == strips[2],
            "closing UDL clears the checked feedback"
        );
        assert_eq!(app.update(|cx| cx.windows().len()), 1);
    }

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
            ("menu-document-list", "icon-tooltip-Document list"),
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
