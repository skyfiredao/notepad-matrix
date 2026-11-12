use super::*;

#[gpui::test]
fn floating_completion_single_click_selects_and_double_click_commits_with_document_focus(
    cx: &mut gpui::TestAppContext,
) {
    let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
    let request = |cx: &mut gpui::TestAppContext| {
        let task = main
            .update(cx, |view, window, cx| {
                view.complete_word(window, cx);
                view.intelligence.task.take().expect("completion task")
            })
            .expect("request words");
        cx.foreground_executor().clone().block_test(task);
        cx.run_until_parked();
        main.update(cx, |view, _, _| {
            view.floating_windows[&floating_windows::DialogKind::Completion]
        })
        .expect("real completion window")
    };
    main.update(cx, |view, window, cx| {
        *view.editor_mut() = EditorSession::new("print printer\npri");
        let end = ByteOffset(view.editor().document().len_bytes());
        view.editor_mut()
            .select(Selection {
                anchor: end,
                head: end,
            })
            .unwrap();
        view.editor_focus.focus(window, cx);
        view.editor_changed(Ok(()), window, cx);
    })
    .unwrap();
    let handle = request(cx);
    let mut popup = gpui::VisualTestContext::from_window(handle.into(), cx);
    let second = popup
        .debug_bounds("intelligence-candidate-1")
        .expect("second candidate")
        .center();
    handle
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    popup.simulate_click(second, gpui::Modifiers::default());
    cx.run_until_parked();
    main.update(cx, |view, window, _| {
        assert_eq!(view.editor().committed_text(), "print printer\npri");
        assert!(view.completion_visible());
        assert_eq!(view.intelligence.model.completion().unwrap().selected, 1);
        assert!(view.editor_focus.is_focused(window));
    })
    .unwrap();
    popup.simulate_event(gpui::MouseDownEvent {
        position: second,
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    popup.simulate_event(gpui::MouseUpEvent {
        position: second,
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::default(),
        click_count: 2,
    });
    cx.run_until_parked();
    main.update(cx, |view, window, _| {
        assert_eq!(view.editor().committed_text(), "print printer\nprinter");
        assert!(!view.completion_visible());
        assert!(view.editor_focus.is_focused(window));
        assert!(view.editor_mut().undo());
        assert_eq!(view.editor().committed_text(), "print printer\npri");
        assert!(!view.editor_mut().undo());
    })
    .unwrap();
}

#[gpui::test]
fn floating_completion_accepts_and_nested_calltip_cancels(cx: &mut gpui::TestAppContext) {
    let home = tempfile::tempdir().expect("isolated document directory");
    let path = home.path().join("large.txt");
    std::fs::write(&path, "printer print\npri").expect("source file");
    let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
    let task = main
        .update(cx, |view, window, cx| {
            view.settings_store = None;
            let mut policy = crate::workspace::LargeFileRestriction {
                threshold_bytes: 1,
                ..Default::default()
            };
            view.workspace.set_large_file_restriction(policy);
            let (file, text) = crate::files::DiskFile::load_unicode(&path).expect("source");
            view.workspace
                .open_buffer(file, &text)
                .expect("open source");
            let end = ByteOffset(text.len());
            view.editor_mut()
                .select(Selection {
                    anchor: end,
                    head: end,
                })
                .expect("caret");
            view.editor_focus.focus(window, cx);
            view.editor_changed(Ok(()), window, cx);
            assert!(!view.document_switcher_visible());
            assert!(view.workspace.active().is_large_file());
            view.on_editor_input(window, cx);
            assert!(
                view.intelligence.task.is_none(),
                "large files do not auto-complete by default"
            );
            policy.allow_auto_completion = true;
            view.workspace.set_large_file_restriction(policy);
            assert!(view.workspace.auto_completion_allowed());
            view.macro_recording = true;
            view.on_editor_input(window, cx);
            assert!(
                view.intelligence.task.is_none(),
                "recording does not start automatic completion"
            );
            view.macro_recording = false;
            view.macro_playing = true;
            view.on_editor_input(window, cx);
            assert!(
                view.intelligence.task.is_none(),
                "playback does not start automatic completion"
            );
            view.macro_playing = false;
            policy.allow_auto_completion = false;
            view.workspace.set_large_file_restriction(policy);
            view.menu_input(MenuEvent::OpenRoot(1), window, cx);
            view.menu_input(
                MenuEvent::OpenSubmenu("edit-menu-auto-completion"),
                window,
                cx,
            );
            assert_eq!(view.menu_bar.popup_count(), 2);
            view.menu_input(MenuEvent::First, window, cx);
            view.menu_input(MenuEvent::Down, window, cx);
            view.menu_input(MenuEvent::Activate, window, cx);
            view.intelligence.task.take().expect("completion task")
        })
        .expect("request completion");
    cx.foreground_executor().clone().block_test(task);
    cx.run_until_parked();
    let task = main
        .update(cx, |view, window, cx| {
            assert!(view.completion_visible());
            assert!(view.editor_focus.is_focused(window));
            assert!(view.capture_intelligence_action(IntelligenceAction::Down, window, cx));
            assert!(view.capture_intelligence_action(IntelligenceAction::Enter, window, cx));
            assert_eq!(view.editor().committed_text(), "printer print\nprinter");
            assert!(!view.capture_intelligence_action(IntelligenceAction::Enter, window, cx));
            let end = ByteOffset(view.editor().document().len_bytes());
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: end,
                })
                .expect("replace source");
            view.editor_mut()
                .insert_text("printf(\"x,(y)\", strlen(\"ab\"), ")
                .expect("nested call");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Builtin(2));
            view.editor_changed(Ok(()), window, cx);
            view.show_calltip(window, cx);
            view.intelligence.task.take().expect("calltip task")
        })
        .expect("accept through editor action seam");
    cx.foreground_executor().clone().block_test(task);
    cx.run_until_parked();
    main.update(cx, |view, window, cx| {
        assert!(view.calltip_visible());
        assert!(view.editor_focus.is_focused(window));
        let help = &view.intelligence.model.calltip().expect("nested help").help;
        assert_eq!(help.function_name, "printf");
        assert_eq!(help.active_parameter, 2);
        assert!(view.capture_intelligence_action(IntelligenceAction::Escape, window, cx));
        assert!(!view.calltip_visible());
    })
    .expect("nested calltip keeps document IME focus");
}
