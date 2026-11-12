use super::*;

#[gpui::test]
fn native_manual_session_choosers_restore_saved_file_without_losing_draft(
    cx: &mut gpui::TestAppContext,
) {
    let home = tempfile::tempdir().expect("isolated session directory");
    let file_path = home.path().join("document.txt");
    let session_path = home.path().join("manual.session.json");
    std::fs::write(&file_path, "saved text").expect("saved document");
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    let named = view.update_in(cx, |view, window, cx| {
        view.settings_store = Some(ConfigStore::under_home(home.path()));
        let (file, text) = crate::files::DiskFile::load_unicode(&file_path).expect("open document");
        let id = view.workspace.open_buffer(file, &text).expect("buffer");
        view.reset_active_view(window, cx);
        id
    });
    cx.dispatch_action(CloneToOtherView);
    view.update_in(cx, |view, window, cx| view.start_save_session(window, cx));
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(session_path.clone()));
    cx.run_until_parked();
    assert!(session_path.is_file());
    let draft = view.update_in(cx, |view, window, cx| {
        assert!(!view.file_busy);
        assert!(view.editor_error.is_none(), "{:?}", view.editor_error);
        let draft = view.workspace.new_buffer().expect("draft");
        view.editor_mut()
            .insert_text("retained draft")
            .expect("draft text");
        view.workspace
            .close_buffer(named, crate::workspace::CloseDisposition::IfUnmodified)
            .expect("close saved file");
        view.reset_active_view(window, cx);
        view.start_load_session(window, cx);
        draft
    });
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| Some(vec![session_path.clone()]));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.file_busy);
        assert!(view.editor_error.is_none(), "{:?}", view.editor_error);
        let restored = view.workspace.active().id();
        assert_eq!(view.editor().committed_text(), "saved text");
        assert!(
            view.workspace
                .view_buffer_ids(crate::workspace::ViewId::Primary)
                .contains(&restored)
        );
        assert!(
            view.workspace
                .view_buffer_ids(crate::workspace::ViewId::Secondary)
                .contains(&restored)
        );
        assert!(view.workspace.buffers().iter().any(
            |buffer| buffer.id() == draft && buffer.editor.committed_text() == "retained draft"
        ));
    });
    view.update_in(cx, |view, window, cx| {
        view.menu_input(MenuEvent::OpenRoot(1), window, cx);
        view.menu_input(MenuEvent::OpenSubmenu("edit-copy-to-clipboard"), window, cx);
        view.menu_input(MenuEvent::First, window, cx);
        view.menu_input(MenuEvent::Activate, window, cx);
    });
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(
            std::fs::canonicalize(&file_path)
                .expect("source path")
                .to_string_lossy()
                .into_owned()
        )
    );
}

#[gpui::test]
fn native_tab_metadata_layout_and_mru_actions_have_visible_effects(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.dispatch_action(NewFile);
    cx.dispatch_action(NewFile);
    cx.dispatch_action(PinCurrentTab);
    assert!(cx.debug_bounds("buffer-tab-pinned-icon-0-3").is_some());
    view.update(cx, |view, _| {
        assert!(view.workspace.active().is_pinned());
        assert_eq!(view.workspace.active_index(), 0);
    });
    cx.dispatch_action(MruNextBuffer);
    view.update(cx, |view, _| {
        assert_eq!(
            view.workspace.mru_cycle_selection().map(|(_, id)| id.0),
            Some(2)
        )
    });
    let switcher = cx
        .windows()
        .into_iter()
        .find(|handle| {
            gpui::VisualTestContext::from_window(*handle, cx)
                .debug_bounds("document-mru-switcher")
                .is_some()
        })
        .expect("independent document switcher");
    let mut popup = gpui::VisualTestContext::from_window(switcher, cx);
    popup.dispatch_action(MruNextBuffer);
    popup.simulate_event(gpui::ModifiersChangedEvent::default());
    cx.run_until_parked();
    view.update(cx, |view, _| assert_eq!(view.workspace.active().id().0, 1));
    cx.dispatch_action(ToggleMultilineTabs);
    view.update(cx, |view, _| assert!(view.tab_multiline));
    cx.dispatch_action(ToggleVerticalTabs);
    view.update(cx, |view, _| assert!(view.tab_vertical));
    let first = cx.debug_bounds("buffer-tab-1").expect("first tab");
    let second = cx.debug_bounds("buffer-tab-2").expect("second tab");
    assert!(second.top() > first.top());
    let from = second.origin + gpui::point(px(25.), px(13.));
    let to = first.origin + gpui::point(px(25.), px(13.));
    cx.simulate_event(gpui::MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        ..Default::default()
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.workspace
                .view_buffer_ids(crate::workspace::ViewId::Primary)
                .iter()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
            [3, 2, 1]
        );
        assert_eq!(view.workspace.active().id().0, 2);
    });
    cx.dispatch_action(CloneToOtherView);
    let from = cx
        .debug_bounds("buffer-tab-1")
        .expect("source view tab")
        .origin
        + gpui::point(px(25.), px(13.));
    let to = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("destination view")
        .origin
        + gpui::point(px(80.), px(20.));
    let modifiers = gpui::Modifiers {
        control: true,
        ..Default::default()
    };
    cx.simulate_event(gpui::ModifiersChangedEvent {
        modifiers,
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        modifiers,
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        modifiers,
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        modifiers,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        modifiers,
        ..Default::default()
    });
    cx.simulate_event(gpui::ModifiersChangedEvent::default());
    view.read_with(cx, |view, _| {
        assert!(
            view.workspace
                .view_buffer_ids(crate::workspace::ViewId::Primary)
                .contains(&crate::workspace::BufferId(1))
        );
        assert!(
            view.workspace
                .view_buffer_ids(crate::workspace::ViewId::Secondary)
                .contains(&crate::workspace::BufferId(1))
        );
        assert_eq!(
            view.workspace.active_view(),
            crate::workspace::ViewId::Secondary
        );
        assert_eq!(view.workspace.active().id().0, 1);
        assert_eq!(view.workspace.buffers().len(), 3);
    });
    let from = cx
        .debug_bounds("buffer-tab-secondary-1")
        .expect("current view tab")
        .origin
        + gpui::point(px(25.), px(13.));
    let to = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("current editor")
        .origin
        + gpui::point(px(80.), px(20.));
    cx.simulate_event(gpui::MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        ..Default::default()
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        ..Default::default()
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu_bar.popup_count(), 2);
        assert!(view.menu_submenu_only);
        assert_eq!(
            view.menu_bar
                .popup_entries(1)
                .expect("source transfer choices")
                .len(),
            2
        );
    });
    cx.dispatch_action(MenuEscape);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu_bar.mode(), MenuMode::Closed)
    });
}

#[gpui::test]
fn native_clean_close_and_restart_preserves_named_file_and_both_view_states(
    cx: &mut gpui::TestAppContext,
) {
    let home = tempfile::tempdir().expect("isolated home");
    let path = home.path().join("session.txt");
    std::fs::write(
        &path,
        format!("{}\n{}", "wrapped ".repeat(200), "line\n".repeat(120)),
    )
    .expect("saved file");
    let path = std::fs::canonicalize(path).expect("canonical fixture path");
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let initial = view.workspace.active().id();
        let (file, text) = crate::files::DiskFile::load_unicode(&path).expect("open file");
        view.workspace.open_buffer(file, &text).expect("buffer");
        view.workspace
            .close_buffer(initial, crate::workspace::CloseDisposition::IfUnmodified)
            .expect("remove placeholder");
        view.reset_active_view(window, cx);
    });
    cx.dispatch_action(CloneToOtherView);
    view.update_in(cx, |view, window, cx| {
        view.toggle_editor_word_wrap(window, cx);
        view.editor_mut()
            .select(Selection {
                anchor: ByteOffset(30),
                head: ByteOffset(34),
            })
            .expect("secondary selection");
        view.split_ratio = 0.65;
        view.show_panel(crate::panel_layout::PanelId::DocumentList, window, cx);
        let session = crate::recovery::RecoveryManager::open(home.path())
            .expect("recovery")
            .0;
        view.recovery = Some(session);
        assert!(view.workspace.dirty_buffers().is_empty());
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("secondary editor");
    cx.simulate_event(gpui::ScrollEvent {
        position: bounds.origin + gpui::point(px(100.), px(30.)),
        delta: gpui::ScrollDelta::Lines(gpui::point(0., -6.)),
        ..Default::default()
    });
    view.read_with(cx, |view, _| assert_eq!(view.editor_top_line, 6));
    assert!(cx.simulate_close(), "ordinary native clean close");
    cx.update(|window, _| window.remove_window());
    drop(view);
    cx.run_until_parked();
    let index = std::fs::read_dir(home.path().join(".notepad-matrix/backup"))
        .expect("backup directory")
        .collect::<std::io::Result<Vec<_>>>()
        .expect("backup entries")
        .into_iter()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".session.json"))
        })
        .expect("native close must write the session index");
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&index).expect("session index"))
            .expect("valid session JSON");
    assert_eq!(saved["views"][1]["documents"][0]["state"]["top_line"], 6);
    let (recovery, workspace) =
        crate::recovery::RecoveryManager::open(home.path()).expect("restart");
    assert_eq!(
        workspace
            .view_state(crate::workspace::ViewId::Secondary, workspace.active().id())
            .expect("restored secondary view state")
            .top_line,
        6
    );
    assert_eq!(workspace.buffers().len(), 1);
    assert_eq!(
        workspace
            .active()
            .disk_file
            .as_ref()
            .map(|file| file.path()),
        Some(path.as_path())
    );
    let (restarted, cx) = cx.add_window_view(|window, cx| {
        let mut view = MatrixWindow::new(false, window, cx);
        view.install_recovery(recovery, workspace, window, cx);
        view
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    restarted.update(cx, |view, _| {
        assert_eq!(
            view.workspace.active_view(),
            crate::workspace::ViewId::Secondary
        );
        assert!(view.editor_word_wrap_enabled());
        assert_eq!(view.editor_top_line, 6);
        assert_eq!(view.split_ratio, 0.65);
        assert!(view.document_list_visible);
    });
    let bounds = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("secondary editor");
    cx.update(|window, cx| {
        restarted.update(cx, |view, cx| {
            view.restore_current_document_state(window, cx)
        });
        window.dispatch_event(
            gpui::PlatformInput::Scroll(gpui::ScrollEvent {
                position: bounds.origin + gpui::point(px(100.), px(30.)),
                delta: gpui::ScrollDelta::Lines(gpui::point(0., -3.)),
                ..Default::default()
            }),
            cx,
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    restarted.update(cx, |view, _| assert_eq!(view.editor_top_line, 9));
    restarted.update_in(cx, |view, window, cx| {
        view.split_ratio = 0.4;
        view.restore_current_document_state(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    restarted.update(cx, |view, _| assert_eq!(view.editor_top_line, 0));
}
