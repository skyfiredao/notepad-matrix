use super::*;

#[gpui::test]
fn floating_find_has_its_own_window_and_replace_reuses_it(cx: &mut gpui::TestAppContext) {
    let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
    cx.run_until_parked();
    cx.dispatch_action(main.into(), ShowFind);
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 2, "Find must not be an editor overlay");
    let dialog = cx
        .windows()
        .into_iter()
        .find(|handle| *handle != main.into())
        .expect("Find window");
    cx.dispatch_action(main.into(), ShowReplace);
    cx.run_until_parked();
    assert_eq!(
        cx.windows().len(),
        2,
        "Find and Replace share their source dialog"
    );
    assert!(cx.windows().contains(&dialog));
    assert_eq!(
        gpui::VisualTestContext::from_window(dialog, cx)
            .window_title()
            .as_deref(),
        Some("Replace")
    );
    cx.dispatch_action(dialog, CloseFind);
    cx.run_until_parked();
    assert_eq!(
        cx.windows(),
        vec![main.into()],
        "closing Find leaves its document alive"
    );
}

#[gpui::test]
fn floating_close_cancels_confirmation_without_losing_text(cx: &mut gpui::TestAppContext) {
    let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
    cx.run_until_parked();
    cx.simulate_input(main.into(), "unsaved");
    cx.dispatch_action(main.into(), CloseCurrentFile);
    cx.run_until_parked();
    let dialog = cx
        .windows()
        .into_iter()
        .find(|handle| *handle != main.into())
        .expect("save confirmation window");
    cx.dispatch_action(dialog, CloseWindow);
    cx.run_until_parked();
    assert_eq!(cx.windows(), vec![main.into()]);
    main.read_with(cx, |view, _| {
        assert_eq!(view.editor().committed_text(), "unsaved");
        assert!(view.workspace.active().is_modified());
        assert!(!view.close_dialog);
    })
    .expect("document still open");
}

#[gpui::test]
fn floating_search_and_goto_target_the_document_window(cx: &mut gpui::TestAppContext) {
    let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
    cx.run_until_parked();
    cx.simulate_input(main.into(), "one\ntwo\nthree");
    cx.dispatch_action(main.into(), ShowFind);
    cx.run_until_parked();
    let find = cx
        .windows()
        .into_iter()
        .find(|handle| *handle != main.into())
        .expect("Find window");
    cx.simulate_input(find, "two");
    cx.dispatch_action(find, FindNext);
    cx.run_until_parked();
    main.read_with(cx, |view, _| {
        assert_eq!(
            view.editor().selection().range(),
            ByteOffset(4)..ByteOffset(7)
        );
        assert_eq!(view.editor().committed_text(), "one\ntwo\nthree");
    })
    .expect("document selected by Find");
    cx.dispatch_action(main.into(), ShowGoTo);
    cx.run_until_parked();
    let goto = cx
        .windows()
        .into_iter()
        .find(|handle| *handle != main.into() && *handle != find)
        .expect("Go to window");
    cx.simulate_input(goto, "3");
    cx.dispatch_action(goto, ConfirmGoTo);
    cx.run_until_parked();
    assert!(
        cx.windows().contains(&find),
        "Go to does not dismiss modeless Find"
    );
    assert!(!cx.windows().contains(&goto));
    main.read_with(cx, |view, _| {
        assert_eq!(view.editor().selection().head, ByteOffset(8));
        assert_eq!(view.editor().committed_text(), "one\ntwo\nthree");
    })
    .expect("Go to moved document caret");
}

#[gpui::test]
fn floating_windows_retire_when_their_document_window_closes(cx: &mut gpui::TestAppContext) {
    let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
    cx.run_until_parked();
    cx.dispatch_action(main.into(), ShowFind);
    cx.dispatch_action(main.into(), ShowPreferences);
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 3);
    cx.dispatch_action(main.into(), CloseWindow);
    cx.run_until_parked();
    assert!(
        cx.windows().is_empty(),
        "auxiliary windows must not orphan the application"
    );
}
