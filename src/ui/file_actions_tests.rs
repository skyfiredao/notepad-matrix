use super::*;
use crate::workspace::ViewId;

fn choose_close(cx: &mut gpui::VisualTestContext, action: impl gpui::Action) {
    cx.run_until_parked();
    let mut dialog = cx
        .windows()
        .into_iter()
        .find_map(|handle| {
            let mut candidate = gpui::VisualTestContext::from_window(handle, &cx.cx);
            (candidate.debug_bounds("matrix-editor").is_none()
                && candidate.debug_bounds("close-discard").is_some())
            .then_some(candidate)
        })
        .expect("independent close dialog");
    dialog.dispatch_action(action);
    cx.run_until_parked();
}

#[gpui::test]
fn window_close_question_scopes_the_document_pane(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("unsaved primary");
    cx.dispatch_action(NewFile);
    view.update_in(cx, |view, window, cx| {
        view.move_to_other_view(window, cx);
        assert_eq!(view.workspace.active_view(), ViewId::Secondary);
        view.request_close(window, cx);
        assert!(view.close_dialog);
        assert_eq!(view.workspace.active_view(), ViewId::Primary);
        assert_eq!(view.pane, ViewId::Primary);
    });
    choose_close(cx, CancelClose);
}

#[gpui::test]
fn close_all_cancelled_save_does_not_apply_earlier_no_or_clean_closures(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("first");
    cx.dispatch_action(NewFile);
    cx.simulate_input("second");
    cx.dispatch_action(NewFile);
    let before = view.update_in(cx, |view, _, _| {
        view.workspace.view_buffer_ids(ViewId::Primary).to_vec()
    });
    cx.dispatch_action(CloseAllFiles);
    let first_target = view.update_in(cx, |view, _, _| view.close_target);
    choose_close(cx, DiscardClose);
    view.update_in(cx, |view, _, _| {
        assert!(view.close_dialog);
        assert_ne!(
            view.close_target, first_target,
            "No must advance to the next document"
        );
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Primary), before);
    });
    choose_close(cx, SaveAndClose);
    view.update_in(cx, |view, _, _| {
        assert!(view.file_busy, "Yes must open the document's save chooser")
    });
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    view.update_in(cx, |view, _, _| {
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Primary), before);
        assert!(!view.close_dialog);
        assert!(view.conditional_close.is_none());
    });
    cx.dispatch_action(NewFile);
    cx.dispatch_action(CloseCurrentFile);
    view.update_in(cx, |view, _, _| {
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Primary), before)
    });
}

#[gpui::test]
fn conditional_close_dirty_clone_retains_peer_without_a_question(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("shared");
    let cloned = view.update_in(cx, |view, window, cx| {
        let id = view.workspace.active().id();
        view.clone_to_other_view(window, cx);
        id
    });
    cx.dispatch_action(NewFile);
    view.update_in(cx, |view, window, cx| view.request_close_left(window, cx));
    view.update_in(cx, |view, _, _| {
        assert!(!view.close_dialog);
        assert!(
            view.workspace
                .view_buffer_ids(ViewId::Primary)
                .contains(&cloned)
        );
        assert!(
            !view
                .workspace
                .view_buffer_ids(ViewId::Secondary)
                .contains(&cloned)
        );
        assert_eq!(
            view.workspace
                .active_in_view(ViewId::Primary)
                .expect("retained pane")
                .editor
                .document()
                .text(),
            "shared"
        );
    });
}

#[gpui::test]
fn empty_untitled_dirty_format_closes_without_a_question(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    let old = view.update_in(cx, |view, _, _| {
        view.workspace
            .active_mut()
            .select_format(crate::files::UnicodeFormat::Utf16LeBom);
        view.workspace.active().id()
    });
    cx.dispatch_action(CloseCurrentFile);
    view.update_in(cx, |view, _, _| {
        assert!(!view.close_dialog);
        assert_ne!(view.workspace.active().id(), old);
        assert!(!view.workspace.active().is_modified());
    });
}

#[gpui::test]
fn close_all_no_then_cancel_preserves_both_panes_and_every_membership(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("first");
    view.update_in(cx, |view, window, cx| view.clone_to_other_view(window, cx));
    cx.dispatch_action(NewFile);
    cx.simulate_input("second");
    let before = view.update_in(cx, |view, _, _| {
        [
            view.workspace.view_buffer_ids(ViewId::Primary).to_vec(),
            view.workspace.view_buffer_ids(ViewId::Secondary).to_vec(),
        ]
    });
    cx.dispatch_action(CloseAllFiles);
    let first_target = view.update_in(cx, |view, _, _| view.close_target);
    choose_close(cx, DiscardClose);
    view.update_in(cx, |view, _, _| {
        assert_ne!(
            view.close_target, first_target,
            "No must advance past the shared document exactly once"
        );
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Primary), before[0]);
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Secondary), before[1]);
    });
    choose_close(cx, CancelClose);
    view.update_in(cx, |view, _, _| {
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Primary), before[0]);
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Secondary), before[1]);
        assert!(view.workspace.two_views_visible());
        assert!(!view.close_dialog);
        assert!(view.conditional_close.is_none());
    });
}

#[gpui::test]
fn close_current_dirty_clone_needs_no_dialog_and_keeps_shared_undo(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("A");
    view.update_in(cx, |view, window, cx| view.clone_to_other_view(window, cx));
    cx.dispatch_action(CloseCurrentFile);
    view.update_in(cx, |view, _, _| {
        assert!(!view.close_dialog);
        assert!(!view.workspace.two_views_visible());
        assert_eq!(view.workspace.active_view(), ViewId::Primary);
        assert_eq!(view.workspace.active().editor.document().text(), "A");
        assert!(view.workspace.active().is_modified());
    });
    cx.dispatch_action(EditorUndo);
    view.update_in(cx, |view, _, _| {
        assert_eq!(view.workspace.active().editor.document().text(), "")
    });
}

#[gpui::test]
fn close_all_clone_gets_one_question_and_leaves_current_pane_visible(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("shared");
    view.update_in(cx, |view, window, cx| view.clone_to_other_view(window, cx));
    cx.dispatch_action(CloseAllFiles);
    choose_close(cx, DiscardClose);
    view.update_in(cx, |view, _, _| {
        assert!(
            !view.close_dialog,
            "a shared document must not be prompted twice"
        );
        assert_eq!(view.workspace.active_view(), ViewId::Secondary);
        assert!(view.workspace.view_buffer_ids(ViewId::Primary).is_empty());
        assert_eq!(view.workspace.view_buffer_ids(ViewId::Secondary).len(), 1);
        assert_eq!(view.workspace.active().editor.document().text(), "");
    });
}

#[gpui::test]
fn save_all_restores_original_document_in_original_clone_pane(cx: &mut gpui::TestAppContext) {
    let home = tempfile::tempdir().expect("save fixtures");
    let a = home.path().join("a.txt");
    let b = home.path().join("b.txt");
    std::fs::write(&a, "A").expect("first file");
    std::fs::write(&b, "B").expect("second file");
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.dispatch_action(OpenFile);
    cx.simulate_path_prompt_response(|_| Some(vec![a.clone()]));
    cx.run_until_parked();
    cx.simulate_input("edit");
    view.update_in(cx, |view, window, cx| view.clone_to_other_view(window, cx));
    let original = view.update_in(cx, |view, _, _| view.workspace.active().id());
    cx.dispatch_action(OpenFile);
    cx.simulate_path_prompt_response(|_| Some(vec![b.clone()]));
    cx.run_until_parked();
    cx.simulate_input("edit");
    view.update_in(cx, |view, window, cx| {
        assert!(view.focus_editor_pane(ViewId::Primary, window, cx));
        assert!(view.activate_buffer(original, window, cx));
        view.save_all_confirm = false;
    });
    cx.dispatch_action(SaveAllFiles);
    cx.run_until_parked();
    view.update_in(cx, |view, _, _| {
        assert_eq!(view.workspace.active_view(), ViewId::Primary);
        assert_eq!(view.workspace.active().id(), original);
        assert!(view.workspace.dirty_buffers().is_empty());
    });
}
