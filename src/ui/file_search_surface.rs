use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::*;

#[path = "finder_views.rs"]
mod finder_views;

#[derive(Clone, Copy)]
enum FinderCopy {
    Verbatim,
    Lines,
    Pathnames,
}

impl FinderCopy {
    fn label(self) -> &'static str {
        match self {
            Self::Verbatim => "Copy",
            Self::Lines => "Copy Selected Line(s)",
            Self::Pathnames => "Copy Selected Pathname(s)",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::Verbatim => "finder-copy-verbatim",
            Self::Lines => "finder-copy-selected-lines",
            Self::Pathnames => "finder-copy-selected-pathnames",
        }
    }
}

#[derive(Clone, Copy)]
enum FinderCommand {
    FoldAll,
    UnfoldAll,
    Copy(FinderCopy),
    SelectAll,
    ClearAll,
    TogglePurge,
    OpenSelectedPathnames,
    ToggleWrap,
    FindWithin,
    CloseResults,
}

impl FinderCommand {
    const ALL: [Self; 12] = [
        Self::FindWithin,
        Self::CloseResults,
        Self::FoldAll,
        Self::UnfoldAll,
        Self::Copy(FinderCopy::Verbatim),
        Self::Copy(FinderCopy::Lines),
        Self::Copy(FinderCopy::Pathnames),
        Self::SelectAll,
        Self::ClearAll,
        Self::OpenSelectedPathnames,
        Self::ToggleWrap,
        Self::TogglePurge,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::FoldAll => "Fold all",
            Self::UnfoldAll => "Unfold all",
            Self::Copy(copy) => copy.label(),
            Self::SelectAll => "Select all",
            Self::ClearAll => "Clear all",
            Self::TogglePurge => "Purge for every search",
            Self::OpenSelectedPathnames => "Open Selected Pathname(s)",
            Self::ToggleWrap => "Word wrap long lines",
            Self::FindWithin => "Find in these search results...",
            Self::CloseResults => "Close these search results",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::FoldAll => "finder-fold-all",
            Self::UnfoldAll => "finder-unfold-all",
            Self::Copy(copy) => copy.selector(),
            Self::SelectAll => "finder-select-all",
            Self::ClearAll => "finder-clear-all",
            Self::TogglePurge => "finder-purge-every-search",
            Self::OpenSelectedPathnames => "finder-open-selected-pathnames",
            Self::ToggleWrap => "finder-wrap-long-lines",
            Self::FindWithin => "finder-find-in-results",
            Self::CloseResults => "finder-close-results",
        }
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod finder_tests {
    use super::*;

    #[gpui::test]
    fn find_in_results_keyboard_follows_source_tab_check_radio_and_button_semantics(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            cx.bind_keys(find_surface::bindings());
            cx.bind_keys([
                KeyBinding::new("tab", DialogFocusNext, Some("MatrixAuxiliary")),
                KeyBinding::new("shift-tab", DialogFocusPrevious, Some("MatrixAuxiliary")),
            ]);
        });
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar\nbar");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_input("bar");
        within.simulate_keystrokes("tab space tab space tab space tab right tab space");
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-pane-1").is_some());
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert!(
                cx.read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .starts_with(
                        "Search \"bar\" (2 hits in 1 file of 1 searched) [Extended: Case/Word]\r\n"
                    )
            )
        });
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_keystrokes("shift-tab enter");
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-pane-2").is_none());
        view.read_with(cx, |view, _| assert!(!view.find_in_results_visible()));
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "foo");
            assert_eq!(view.editor().committed_text(), "foo bar\nbar");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn find_in_results_one_candidate_does_not_create_the_multi_file_progress_window(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_input("bar");
        within.update(|window, cx| window.dispatch_action(Box::new(FindDialogNext), cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| {
            assert!(view.file_busy);
            assert!(
                !view
                    .floating_windows
                    .contains_key(&floating_windows::DialogKind::FindInResultsProgress)
            );
        });
        assert!(cx.debug_bounds("files-cancel").is_none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-pane-1").is_some());
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert!(
                cx.read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .starts_with("Search \"bar\" (1 hit in 1 file of 1 searched) [Normal]")
            )
        });
    }

    #[gpui::test]
    fn finder_open_selected_completion_uses_document_input_even_if_auxiliary_was_rendered_last(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("one.txt"), "prefix foo suffix").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.configure_find(&serde_json::json!({"Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let open = cx.debug_bounds("finder-open-selected-pathnames").unwrap();
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::InputEvent::to_platform_input(gpui::MouseDownEvent {
                    position: open.center(),
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                gpui::InputEvent::to_platform_input(gpui::MouseUpEvent {
                    position: open.center(),
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        });
        dialog.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("prefix foo suffix")
            )
        });
        view.read_with(cx, |view, _| assert!(!view.editor().document().can_undo()));
    }

    #[gpui::test]
    fn finder_disk_navigation_completion_focuses_the_document_when_an_auxiliary_was_rendered_last(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("one.txt"), "prefix foo suffix").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.configure_find(&serde_json::json!({"Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let position = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::InputEvent::to_platform_input(gpui::MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 2,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                gpui::InputEvent::to_platform_input(gpui::MouseUpEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 2,
                    ..Default::default()
                }),
                cx,
            );
        });
        dialog.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("foo")
            )
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "prefix foo suffix");
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(7)..ByteOffset(10)
            );
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn directory_search_completion_targets_document_results_even_if_find_form_was_rendered_last(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::InputEvent as _;
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("one.txt"), "foo").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.configure_find(&serde_json::json!({"Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        dialog.update(|window, cx| {
            for event in [
                gpui::MouseDownEvent {
                    position: all.center(),
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
                gpui::MouseUpEvent {
                    position: all.center(),
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
            ] {
                window.dispatch_event(event, cx);
            }
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        assert!(dialog.debug_bounds("find-in-files-dialog").is_some());
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(
                copied.starts_with("Search \"foo\" (1 hit in 1 file of 1 searched) [Normal]\r\n")
            );
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "unchanged")
        });
    }

    #[gpui::test]
    fn find_in_results_cannot_replace_the_cancellation_token_of_an_active_directory_search(
        cx: &mut gpui::TestAppContext,
    ) {
        fn raw_click(context: &mut gpui::VisualTestContext, position: gpui::Point<gpui::Pixels>) {
            context.update(|window, cx| {
                window.dispatch_event(
                    gpui::InputEvent::to_platform_input(gpui::MouseDownEvent {
                        position,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    }),
                    cx,
                );
                window.dispatch_event(
                    gpui::InputEvent::to_platform_input(gpui::MouseUpEvent {
                        position,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    }),
                    cx,
                );
                window.draw(cx).clear(cx);
            });
        }
        let files = tempfile::tempdir().unwrap();
        for index in 0..3 {
            std::fs::write(files.path().join(format!("file{index}.txt")), "foo bar").unwrap();
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        let point = gpui::point(header.left() + px(20.), header.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_input("bar");
        let submit = within.debug_bounds("find-in-results-all").unwrap();
        within.simulate_click(submit.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx)
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        raw_click(&mut dialog, all.center());
        view.read_with(cx, |view, _| {
            assert!(view.files_search.cancel.is_some());
            assert!(!view.file_busy);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let secondary = cx.debug_bounds("finder-pane-1").unwrap();
        raw_click(cx, secondary.center());
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::InputEvent::to_platform_input(gpui::MouseDownEvent {
                    position: gpui::point(header.left() + px(20.), header.center().y),
                    button: MouseButton::Right,
                    ..Default::default()
                }),
                cx,
            );
            window.draw(cx).clear(cx);
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        raw_click(cx, find.center());
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.update(|window, cx| window.dispatch_action(Box::new(FindDialogNext), cx));
        view.read_with(cx, |view, _| {
            assert!(
                !view.file_busy,
                "an active directory search must reject a second worker"
            );
            assert!(view.find_in_results_visible());
        });
        let cancel = cx.debug_bounds("files-cancel").unwrap();
        raw_click(cx, cancel.center());
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-pane-2").is_none());
        view.read_with(cx, |view, _| {
            assert!(!view.file_busy);
            assert!(view.files_search.cancel.is_none());
            assert_eq!(view.files_search.results.searches[0].hits, 0);
        });
    }

    #[gpui::test]
    fn find_in_results_cancel_keeps_a_zero_hit_secondary_header_and_total_candidates(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        for index in 0..3 {
            std::fs::write(files.path().join(format!("file{index}.txt")), "foo bar").unwrap();
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_input("bar");
        within.update(|window, cx| window.dispatch_action(Box::new(FindDialogNext), cx));
        view.read_with(cx, |view, _| assert!(view.file_busy));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let progress_handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResultsProgress]
        });
        let mut progress = gpui::VisualTestContext::from_window(progress_handle.into(), &cx.cx);
        assert_eq!(
            progress.window_title().as_deref(),
            Some("Find In Files progress...")
        );
        let form = progress.debug_bounds("find-results-progress").unwrap();
        assert_eq!(form.size, gpui::size(px(565.), px(95.)));
        let cancel = progress
            .debug_bounds("find-results-progress-cancel")
            .unwrap();
        assert_eq!(cancel.left() - form.left(), px(237.));
        assert_eq!(cancel.top() - form.top(), px(59.));
        assert_eq!(cancel.size, gpui::size(px(80.), px(25.)));
        progress.update(|window, cx| {
            let down = gpui::MouseDownEvent {
                position: cancel.center(),
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            };
            let up = gpui::MouseUpEvent {
                position: cancel.center(),
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            };
            window.dispatch_event(gpui::InputEvent::to_platform_input(down), cx);
            window.dispatch_event(gpui::InputEvent::to_platform_input(up), cx);
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("finder-pane-1").is_some());
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().as_deref(), Some("Search \"bar\" (0 hits in 0 files of 3 searched) [Normal] - Line Filter Mode: only display the filtered results\r\n")));
        view.read_with(cx, |view, _| {
            assert!(!view.file_busy);
            assert!(view.files_search.cancel.is_none());
        });
    }

    #[gpui::test]
    fn find_in_results_own_combo_restores_accepted_query_without_touching_primary_history(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        for first in [true, false] {
            let header = cx.debug_bounds("finder-search-0").unwrap();
            cx.simulate_event(gpui::MouseDownEvent {
                position: gpui::point(header.left() + px(20.), header.center().y),
                button: MouseButton::Right,
                ..Default::default()
            });
            let find = cx.debug_bounds("finder-find-in-results").unwrap();
            cx.simulate_click(find.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            let handle = view.read_with(cx, |view, _| {
                view.floating_windows[&floating_windows::DialogKind::FindInResults]
            });
            let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
            within.simulate_input(if first { "bar" } else { "temporary" });
            if first {
                let submit = within.debug_bounds("find-in-results-all").unwrap();
                within.simulate_click(submit.center(), gpui::Modifiers::default());
                cx.run_until_parked();
            } else {
                let arrow = within
                    .debug_bounds("find-in-results-history-toggle")
                    .expect("functional source separate combo");
                within.simulate_click(arrow.center(), gpui::Modifiers::default());
                let item = within.debug_bounds("find-in-results-history-item").unwrap();
                within.simulate_click(item.center(), gpui::Modifiers::default());
                view.read_with(cx, |view, cx| {
                    assert_eq!(view.find_in_results_input().read(cx).text(), "bar")
                });
                within.simulate_input("draft");
                within.simulate_keystrokes("alt-down escape");
                assert!(within.debug_bounds("find-in-results-dialog").is_some());
                assert!(
                    within
                        .debug_bounds("find-in-results-history-item")
                        .is_none()
                );
                within.simulate_keystrokes("alt-down tab");
                assert!(
                    within
                        .debug_bounds("find-in-results-history-item")
                        .is_none(),
                    "Tab must retire the combo popup before moving focus"
                );
                let query = within.debug_bounds("find-in-results-query").unwrap();
                within.simulate_click(query.center(), gpui::Modifiers::default());
                within.simulate_keystrokes("alt-down");
                assert!(
                    within
                        .debug_bounds("find-in-results-history-item")
                        .is_some()
                );
                within.deactivate_window();
                cx.run_until_parked();
                assert!(
                    within
                        .debug_bounds("find-in-results-history-item")
                        .is_none(),
                    "deactivation retires only the dropdown"
                );
                view.read_with(cx, |view, cx| {
                    assert_eq!(view.find_in_results_input().read(cx).text(), "draft");
                    assert_eq!(view.find_query.read(cx).text(), "foo");
                });
            }
        }
    }

    #[gpui::test]
    fn find_in_results_uses_dirty_buffers_and_disk_files_and_counts_missing_candidates(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("one.txt"), "foo\nnothing").unwrap();
        std::fs::write(files.path().join("two.txt"), "foo bar\nbar").unwrap();
        std::fs::write(files.path().join("missing.txt"), "foo").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        std::fs::remove_file(files.path().join("missing.txt")).unwrap();
        view.update_in(cx, |view, window, cx| {
            let (file, text) =
                crate::files::DiskFile::load_unicode(&files.path().join("one.txt")).unwrap();
            view.workspace.open_buffer(file, &text).unwrap();
            view.reset_active_view(window, cx);
            view.editor_mut().insert_text("bar ").unwrap();
        });
        for (match_lines, hits) in [(true, 2), (false, 3)] {
            if !match_lines {
                let main = cx.debug_bounds("finder-pane-0").unwrap();
                cx.simulate_click(main.center(), gpui::Modifiers::default());
            }
            let header = cx.debug_bounds("finder-search-0").unwrap();
            cx.simulate_event(gpui::MouseDownEvent {
                position: gpui::point(header.left() + px(20.), header.center().y),
                button: MouseButton::Right,
                ..Default::default()
            });
            let find = cx.debug_bounds("finder-find-in-results").unwrap();
            cx.simulate_click(find.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            let handle = view.read_with(cx, |view, _| {
                view.floating_windows[&floating_windows::DialogKind::FindInResults]
            });
            let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
            within.simulate_input("bar");
            if !match_lines {
                let toggle = within.debug_bounds("find-in-results-line-filter").unwrap();
                within.simulate_click(toggle.center(), gpui::Modifiers::default());
            }
            let submit = within.debug_bounds("find-in-results-all").unwrap();
            within.simulate_click(submit.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            cx.dispatch_action(EditorSelectAll);
            cx.dispatch_action(EditorCopy);
            cx.update(|_, cx| {
                let copied = cx.read_from_clipboard().unwrap().text().unwrap();
                assert!(copied.starts_with(&format!(
                    "Search \"bar\" ({hits} hits in 2 files of 3 searched) [Normal]"
                )));
                assert_eq!(copied.contains("Line Filter Mode"), match_lines);
                assert!(copied.contains("bar foo"));
            });
            view.read_with(cx, |view, _| {
                assert!(!view.file_busy);
                assert_eq!(view.editor().committed_text(), "bar foo\nnothing");
                assert!(view.editor().document().can_undo());
                assert!(view.workspace.active().is_modified());
            });
        }
        assert_eq!(
            std::fs::read_to_string(files.path().join("one.txt")).unwrap(),
            "foo\nnothing"
        );
    }

    #[gpui::test]
    fn find_in_results_escape_cancels_only_its_own_dialog_when_primary_find_is_visible(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", CloseFind, Some("MatrixFind"))]));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar");
            view.configure_find(&serde_json::json!({"Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_input("discard me");
        within.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert!(!view.find_in_results_visible());
            assert!(view.find_visible);
            assert!(
                view.floating_windows
                    .contains_key(&floating_windows::DialogKind::Find)
            );
            assert!(
                !view
                    .floating_windows
                    .contains_key(&floating_windows::DialogKind::FindInResults)
            );
            assert_eq!(view.find_query.read(cx).text(), "foo");
            assert_eq!(view.editor().committed_text(), "foo bar");
        });
    }

    #[gpui::test]
    fn secondary_publication_does_not_restore_a_source_context_menu_opened_during_search(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        let point = gpui::point(header.left() + px(20.), header.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx.debug_bounds("finder-find-in-results").unwrap();
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        within.simulate_input("bar");
        within.update(|window, cx| window.dispatch_action(Box::new(FindDialogNext), cx));
        view.read_with(cx, |view, _| {
            assert!(
                view.file_busy,
                "the asynchronous search must still be queued"
            )
        });
        cx.update(|window, cx| {
            let event = gpui::MouseDownEvent {
                position: point,
                button: MouseButton::Right,
                ..Default::default()
            };
            window.dispatch_event(gpui::InputEvent::to_platform_input(event), cx);
            window.draw(cx).clear(cx);
        });
        assert!(
            cx.debug_bounds("finder-context-menu").is_some(),
            "the source popup must actually open before publication"
        );
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("finder-pane-1").is_some(),
            "the secondary result must publish before switching back"
        );
        let main = cx.debug_bounds("finder-pane-0").unwrap();
        cx.simulate_click(main.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("finder-context-menu").is_none());
    }

    #[gpui::test]
    fn find_in_results_cancel_discards_unaccepted_text_and_enter_uses_its_own_query(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        for cancel in [true, false] {
            let header = cx.debug_bounds("finder-search-0").unwrap();
            cx.simulate_event(gpui::MouseDownEvent {
                position: gpui::point(header.left() + px(20.), header.center().y),
                button: MouseButton::Right,
                ..Default::default()
            });
            let find = cx.debug_bounds("finder-find-in-results").unwrap();
            cx.simulate_click(find.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            let handle = view.read_with(cx, |view, _| {
                view.floating_windows[&floating_windows::DialogKind::FindInResults]
            });
            let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
            view.read_with(cx, |view, cx| {
                assert_eq!(view.find_in_results_input().read(cx).text(), "")
            });
            within.simulate_input(if cancel { "discard me" } else { "bar" });
            if cancel {
                let dismiss = within.debug_bounds("find-in-results-cancel").unwrap();
                within.simulate_click(dismiss.center(), gpui::Modifiers::default());
                cx.run_until_parked();
                assert!(cx.debug_bounds("finder-pane-1").is_none());
            } else {
                view.read_with(cx, |view, cx| {
                    assert_eq!(view.find_in_results_input().read(cx).text(), "bar")
                });
                within.simulate_keystrokes("enter");
                cx.run_until_parked();
                assert!(cx.debug_bounds("finder-pane-1").is_some());
                view.read_with(cx, |view, cx| {
                    assert_eq!(view.find_query.read(cx).text(), "foo");
                    assert_eq!(view.editor().committed_text(), "foo bar");
                });
            }
        }
    }

    #[gpui::test]
    fn find_in_results_opens_its_own_dialog_and_secondary_result_without_replacing_the_source(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo bar\nbar\nfoo");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let find = cx
            .debug_bounds("finder-find-in-results")
            .expect("functional source Find in these search results");
        cx.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::FindInResults]
        });
        let mut within = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        assert!(within.debug_bounds("find-in-results-dialog").is_some());
        assert_eq!(
            within.window_title().as_deref(),
            Some("Find in search results")
        );
        let form = within.debug_bounds("find-in-results-dialog").unwrap();
        assert_eq!(form.size, gpui::size(px(548.), px(186.)));
        let query = within.debug_bounds("find-in-results-query-combo").unwrap();
        assert_eq!(query.left() - form.left(), px(124.5));
        assert_eq!(query.top() - form.top(), px(13.5));
        assert_eq!(query.size.width, px(267.));
        let find_all = within.debug_bounds("find-in-results-all").unwrap();
        assert_eq!(find_all.left() - form.left(), px(402.));
        assert_eq!(find_all.top() - form.top(), px(13.5));
        let lines = within.debug_bounds("find-in-results-line-filter").unwrap();
        let word = within.debug_bounds("find-in-results-word").unwrap();
        let case = within.debug_bounds("find-in-results-case").unwrap();
        assert_eq!(lines.top() - form.top(), px(46.5));
        assert_eq!(word.top() - form.top(), px(64.5));
        assert_eq!(case.top() - form.top(), px(82.5));
        within.simulate_input("bar");
        let submit = within.debug_bounds("find-in-results-all").unwrap();
        within.simulate_click(submit.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(copied.starts_with("Search \"bar\" (1 hit in 1 file of 1 searched) [Normal] - Line Filter Mode: only display the filtered results\r\n"));
            assert!(copied.ends_with("\tLine 1: foo bar\r\n"));
        });
        let main = cx.debug_bounds("finder-pane-0").unwrap();
        cx.simulate_click(main.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert!(
                cx.read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .starts_with("Search \"foo\" (2 hits")
            )
        });
        let secondary = cx.debug_bounds("finder-pane-1").unwrap();
        cx.simulate_click(secondary.center(), gpui::Modifiers::default());
        let close = cx.debug_bounds("files-results-close").unwrap();
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("finder-pane-1").is_none());
        assert!(cx.debug_bounds("finder-search-0").is_some());
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "foo");
            assert_eq!(view.editor().committed_text(), "foo bar\nbar\nfoo");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn wrapped_finder_up_into_an_offscreen_tall_row_reveals_its_last_subline(
        cx: &mut gpui::TestAppContext,
    ) {
        let content = format!(
            "foo {}\nfoo\nfoo {}",
            "word ".repeat(390),
            "word ".repeat(390)
        );
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(gpui::size(px(700.), px(600.)));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&content);
            view.configure_find(&serde_json::json!({"FinderConfig": {"wrap": true}}));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let panel = cx.debug_bounds("files-results").unwrap();
        let first = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        cx.simulate_event(gpui::ScrollEvent {
            position: panel.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), -(first.size.height + px(34.)))),
            ..Default::default()
        });
        cx.run_until_parked();
        let current = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        cx.simulate_event(gpui::ScrollEvent {
            position: panel.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(
                px(0.),
                -(current.bottom() - panel.top() - px(22.)),
            )),
            ..Default::default()
        });
        cx.run_until_parked();
        let next = cx.debug_bounds("finder-hit-0-0-1").unwrap();
        cx.simulate_click(
            gpui::point(next.left() + px(10.), next.top() + px(8.)),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let top = view.files_search.wrapped_scroll.logical_scroll_top();
            assert!(
                top.item_ix == 2 && top.offset_in_item > px(0.),
                "the last subline, not the beginning, must be revealed: {top:?}"
            );
            assert_eq!(
                view.finder_caret().row,
                Some(crate::finder::Row::Line(0, 0, 0))
            );
        });
    }

    #[gpui::test]
    fn wrapped_finder_arrows_resolve_hidden_caret_rows_before_using_caption_bytes(
        cx: &mut gpui::TestAppContext,
    ) {
        let content = format!("foo {}", "word ".repeat(90));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&content);
            view.configure_find(&serde_json::json!({"FinderConfig": {"wrap": true}}));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let bounds = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let point = gpui::point(bounds.left() + px(100.), bounds.bottom() - px(8.));
        cx.simulate_click(point, gpui::Modifiers::default());
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let fold = cx.debug_bounds("finder-fold-all").unwrap();
        cx.simulate_click(fold.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        cx.simulate_keystrokes("shift-down");
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert_ne!(copied, "sentinel");
            assert!(copied.ends_with("\r\n"));
            assert!(!copied.contains("Search \"foo\""));
        });
    }

    #[gpui::test]
    fn wrapped_finder_tall_row_navigation_keeps_the_visual_caret_visible(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(gpui::size(px(700.), px(600.)));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&format!("foo {}", "word ".repeat(390)));
            view.configure_find(&serde_json::json!({"FinderConfig": {"wrap": true}}));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let panel = cx.debug_bounds("files-results").unwrap();
        let row = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        assert!(row.size.height > panel.size.height);
        cx.simulate_event(gpui::ScrollEvent {
            position: panel.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-130.))),
            ..Default::default()
        });
        cx.run_until_parked();
        cx.simulate_click(
            gpui::point(panel.left() + px(100.), panel.center().y),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("down down down");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let top = view.files_search.wrapped_scroll.logical_scroll_top();
            assert!(
                top.item_ix == 2 && top.offset_in_item > px(0.),
                "navigation must not jump to the beginning of a tall logical row: {top:?}"
            );
            assert!(view.finder_caret().byte > 100);
        });
    }

    #[gpui::test]
    fn wrapped_finder_new_search_resets_preferred_column_to_its_new_caret(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&format!("foo {}", "word ".repeat(90)));
            view.configure_find(&serde_json::json!({"FinderConfig": {"wrap": true}, "Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find(window, cx);
            view.find_query.update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        cx.simulate_click(
            gpui::point(hit.left() + px(100.), hit.top() + px(8.)),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("down");
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.simulate_keystrokes("shift-down");
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Search \"foo\" (1 hit in 1 file of 1 searched) [Normal]\r\n")
            )
        });
    }

    #[gpui::test]
    fn wrapped_finder_mouse_selection_uses_the_clicked_visual_subline(
        cx: &mut gpui::TestAppContext,
    ) {
        let content = format!("foo {}", "word ".repeat(90));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&content);
            view.configure_find(&serde_json::json!({"FinderConfig": {"wrap": true}}));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let bounds = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        assert!(bounds.size.height > px(17.));
        cx.simulate_click(
            gpui::point(bounds.left() + px(8.), bounds.top() + px(8.)),
            gpui::Modifiers::default(),
        );
        cx.simulate_click(
            gpui::point(bounds.right() - px(2.), bounds.bottom() - px(8.)),
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(format!("\tLine 1: {content}").as_str())
            )
        });
        cx.simulate_click(
            gpui::point(bounds.left() + px(8.), bounds.top() + px(8.)),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("shift-down");
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(copied.starts_with("\tLine 1: foo "));
            assert!(!copied.contains('\n'));
            assert!(copied.len() < content.len());
        });
        cx.dispatch_action(EditorSelectAll);
        cx.simulate_keystrokes("shift-up");
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(
                !copied.ends_with("\r\n"),
                "Up from final blank moves into the last visual subline"
            );
            assert!(copied.starts_with("Search \"foo\""));
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), content);
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_wrap_toggle_reflows_real_rows_preserves_copy_and_persists_without_editing(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FinderConfig"] = serde_json::json!({"wrap": false, "future": 42});
                Ok(())
            })
            .unwrap();
        let content = format!("foo {}", "word ".repeat(90));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&content);
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        let point = gpui::point(header.left() + px(20.), header.center().y);
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        let expected = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
        assert_eq!(
            cx.debug_bounds("finder-hit-0-0-0").unwrap().size.height,
            px(17.)
        );
        for enabled in [true, false] {
            cx.simulate_event(gpui::MouseDownEvent {
                position: point,
                button: MouseButton::Right,
                ..Default::default()
            });
            let toggle = cx
                .debug_bounds("finder-wrap-long-lines")
                .expect("functional source word wrap");
            cx.simulate_click(toggle.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            let bounds = cx.debug_bounds("finder-hit-0-0-0").unwrap();
            if enabled {
                assert!(bounds.size.height > px(17.));
            } else {
                assert_eq!(bounds.size.height, px(17.));
            }
            cx.dispatch_action(EditorCopy);
            cx.update(|_, cx| {
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some(expected.as_str())
                )
            });
            let saved = ConfigStore::under_home(home.path())
                .load_settings()
                .unwrap();
            assert_eq!(saved["FinderConfig"]["wrap"], enabled);
            assert_eq!(saved["FinderConfig"]["future"], 42);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), content);
                assert!(!view.editor().document().can_undo());
            });
        }
    }

    #[gpui::test]
    fn finder_open_paths_respects_busy_and_empty_selection_and_continues_after_missing_files(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(files.path().join(name), "foo").unwrap();
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-search-0").unwrap();
        let point = gpui::point(header.left() + px(20.), header.center().y);
        let open = |context: &mut gpui::VisualTestContext| {
            context.simulate_event(gpui::MouseDownEvent {
                position: point,
                button: MouseButton::Right,
                ..Default::default()
            });
            let action = context
                .debug_bounds("finder-open-selected-pathnames")
                .unwrap();
            context.simulate_click(action.center(), gpui::Modifiers::default());
            context.run_until_parked();
        };
        open(cx);
        view.read_with(cx, |view, _| {
            assert!(!view.file_busy);
            assert!(
                view.workspace
                    .buffers()
                    .iter()
                    .all(|buffer| buffer.disk_file.is_none())
            );
        });
        cx.dispatch_action(EditorSelectAll);
        view.update(cx, |view, _| view.file_busy = true);
        open(cx);
        view.read_with(cx, |view, _| {
            assert!(view.file_busy);
            assert!(
                view.workspace
                    .buffers()
                    .iter()
                    .all(|buffer| buffer.disk_file.is_none())
            );
        });
        view.update(cx, |view, _| view.file_busy = false);
        let missing = view.read_with(cx, |view, _| {
            PathBuf::from(&view.files_search.results.searches[0].documents[1].name)
        });
        std::fs::remove_file(&missing).unwrap();
        open(cx);
        view.read_with(cx, |view, _| {
            assert!(!view.file_busy);
            let opened: Vec<_> = view
                .workspace
                .buffers()
                .iter()
                .filter_map(|buffer| {
                    buffer
                        .disk_file
                        .as_ref()
                        .map(|file| file.path().to_path_buf())
                })
                .collect();
            let documents = &view.files_search.results.searches[0].documents;
            assert_eq!(
                opened,
                [
                    PathBuf::from(&documents[0].name),
                    PathBuf::from(&documents[2].name)
                ]
            );
            assert_eq!(
                view.workspace.active().disk_file.as_ref().unwrap().path(),
                opened[1]
            );
            assert!(
                view.editor_error
                    .as_deref()
                    .is_some_and(|error| error.contains(&missing.display().to_string()))
            );
        });
    }

    #[gpui::test]
    fn finder_can_measure_and_copy_a_mac_filename_containing_a_newline_without_changing_its_path(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        let path = files.path().join("two\nname.txt");
        std::fs::write(&path, "foo").unwrap();
        let path = path.canonicalize().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let copy = cx.debug_bounds("finder-copy-selected-pathnames").unwrap();
        cx.simulate_click(copy.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(format!("{}\r\n", path.display()).as_str())
            )
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                PathBuf::from(&view.files_search.results.searches[0].documents[0].name),
                path
            )
        });
    }

    #[gpui::test]
    fn finder_opens_selected_opaque_paths_in_result_order_without_reloading_dirty_buffers(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("one.txt"), "foo").unwrap();
        std::fs::write(files.path().join("two\n'name.txt"), "foo").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_directory(files.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let (existing_path, last_path) = view.read_with(cx, |view, _| {
            let docs = &view.files_search.results.searches[0].documents;
            assert_eq!(docs.len(), 2);
            (PathBuf::from(&docs[0].name), PathBuf::from(&docs[1].name))
        });
        let existing_id = view.update_in(cx, |view, window, cx| {
            let (file, text) = crate::files::DiskFile::load_unicode(&existing_path).unwrap();
            let id = view.workspace.open_buffer(file, &text).unwrap();
            view.reset_active_view(window, cx);
            view.editor_mut().insert_text("changed ").unwrap();
            id
        });
        let header = cx.debug_bounds("finder-search-0").unwrap();
        let point = gpui::point(header.left() + px(20.), header.center().y);
        cx.simulate_click(point, gpui::Modifiers::default());
        cx.dispatch_action(EditorSelectAll);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let open = cx
            .debug_bounds("finder-open-selected-pathnames")
            .expect("functional source Open Selected Pathnames");
        cx.simulate_click(open.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.file_busy);
            assert_eq!(
                view.workspace.active().disk_file.as_ref().unwrap().path(),
                last_path
            );
            assert!(view.editor().selection().range().is_empty());
            assert_eq!(
                view.workspace
                    .buffers()
                    .iter()
                    .filter(|buffer| buffer.disk_file.is_some())
                    .count(),
                2
            );
            let existing = view
                .workspace
                .buffers()
                .iter()
                .find(|buffer| buffer.id() == existing_id)
                .unwrap();
            assert_eq!(existing.editor.committed_text(), "changed foo");
            assert_eq!(existing.editor.selection().head, ByteOffset(8));
            assert!(existing.editor.document().can_undo());
            assert!(existing.is_modified());
        });
        assert_eq!(std::fs::read_to_string(existing_path).unwrap(), "foo");
    }

    #[gpui::test]
    fn finder_delete_after_fold_all_removes_only_the_hidden_logical_result(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-1").unwrap();
        let point = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_click(point, gpui::Modifiers::default());
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let fold = cx.debug_bounds("finder-fold-all").unwrap();
        cx.simulate_click(fold.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("delete");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.files_search.results.searches.len(), 1);
            assert_eq!(view.files_search.results.searches[0].documents.len(), 1);
            assert_eq!(
                view.files_search.results.searches[0].documents[0]
                    .lines
                    .len(),
                1
            );
            assert_eq!(
                view.files_search.results.searches[0].documents[0].lines[0].number,
                1
            );
            assert_eq!(view.files_search.results.searches[0].hits, 2);
            assert_eq!(view.editor().committed_text(), "foo\nfoo");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_fold_all_keeps_the_hidden_logical_result_target_for_enter(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-1").unwrap();
        let point = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_click(point, gpui::Modifiers::default());
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let fold = cx.debug_bounds("finder-fold-all").unwrap();
        cx.simulate_click(fold.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(4)..ByteOffset(7)
            );
            assert_eq!(view.editor().committed_text(), "foo\nfoo");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_fold_commands_preserve_select_all_at_the_logical_end(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        let expected = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
        let selection = view.read_with(cx, |view, _| {
            (view.files_search.text_anchor, view.files_search.text_head)
        });
        for command in ["finder-unfold-all", "finder-fold-all", "finder-unfold-all"] {
            let header = cx.debug_bounds("finder-search-0").unwrap();
            cx.simulate_event(gpui::MouseDownEvent {
                position: gpui::point(header.left() + px(20.), header.center().y),
                button: MouseButton::Right,
                ..Default::default()
            });
            let action = cx.debug_bounds(command).unwrap();
            cx.simulate_click(action.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
            });
            cx.dispatch_action(EditorCopy);
            cx.update(|_, cx| {
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some(expected.as_str())
                )
            });
            view.read_with(cx, |view, _| {
                assert_eq!(
                    (view.files_search.text_anchor, view.files_search.text_head),
                    selection
                );
                assert_eq!(
                    view.files_search.results_selected,
                    view.files_search.results.rows.len()
                );
                assert_eq!(view.editor().committed_text(), "foo\nfoo");
            });
        }
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let point = gpui::point(hit.left() + px(8.), hit.center().y);
        cx.simulate_click(point, gpui::Modifiers::default());
        cx.simulate_keystrokes("shift-down");
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Right,
            ..Default::default()
        });
        let fold = cx.debug_bounds("finder-fold-all").unwrap();
        cx.simulate_click(fold.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("\tLine 1: foo\r\n")
            )
        });
    }

    #[gpui::test]
    fn finder_context_keyboard_skips_disabled_plain_copy_without_running_other_commands(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        let doc = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(doc.left() + px(20.), doc.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        cx.simulate_keystrokes("down down down enter");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("foo\r\n")
            )
        });
        assert!(cx.debug_bounds("finder-context-menu").is_none());
        view.read_with(cx, |view, _| {
            assert!(!view.files_search.results.searches[0].collapsed);
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_purge_toggle_does_not_replace_malformed_saved_configuration(
        cx: &mut gpui::TestAppContext,
    ) {
        for value in [
            serde_json::json!(["future"]),
            serde_json::json!({"purgeBeforeEverySearch": 42}),
            serde_json::Value::Null,
            serde_json::json!({"purgeBeforeEverySearch": null}),
        ] {
            let home = tempfile::tempdir().unwrap();
            let store = ConfigStore::under_home(home.path());
            let settings = store
                .update_settings(|settings| {
                    settings["FinderConfig"] = value;
                    Ok(())
                })
                .unwrap();
            let path = store.root().join("settings.json");
            let before = std::fs::read(&path).unwrap();
            let (view, context) =
                cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
            view.update_in(context, |view, window, cx| {
                *view.editor_mut() = EditorSession::new("foo");
                view.settings_store = Some(store);
                view.configure_find(&settings);
                view.show_find(window, cx);
            });
            context.run_until_parked();
            let handle = view.read_with(context, |view, _| {
                view.floating_windows[&floating_windows::DialogKind::Find]
            });
            let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &context.cx);
            let all = dialog.debug_bounds("find-all-current").unwrap();
            dialog.simulate_click(all.center(), gpui::Modifiers::default());
            context.run_until_parked();
            let baseline = std::fs::read(&path).unwrap();
            let header = context.debug_bounds("finder-search-0").unwrap();
            context.simulate_event(gpui::MouseDownEvent {
                position: gpui::point(header.left() + px(20.), header.center().y),
                button: MouseButton::Right,
                ..Default::default()
            });
            let purge = context.debug_bounds("finder-purge-every-search").unwrap();
            context.simulate_click(purge.center(), gpui::Modifiers::default());
            context.run_until_parked();
            assert_ne!(
                before, baseline,
                "the successful search legitimately records its own history"
            );
            assert_eq!(std::fs::read(path).unwrap(), baseline);
            view.read_with(context, |view, _| {
                assert!(!view.files_search.results_purge);
                assert!(
                    view.editor_error
                        .as_deref()
                        .is_some_and(|error| error.contains("Finder setting could not be saved"))
                );
            });
        }
    }

    #[gpui::test]
    fn finder_purge_menu_persists_and_removes_old_searches_only_when_enabled(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FinderConfig"] =
                    serde_json::json!({"purgeBeforeEverySearch": false, "future": {"keep": 42}});
                settings["Searching"] = serde_json::json!({"findDlgAlwaysVisible": true});
                Ok(())
            })
            .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo");
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("find-all-current").unwrap();
        for _ in 0..2 {
            dialog.simulate_click(search.center(), gpui::Modifiers::default());
            cx.run_until_parked();
        }
        view.read_with(cx, |view, _| {
            assert_eq!(view.files_search.results.searches.len(), 2)
        });
        let header = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(header.left() + px(20.), header.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let purge = cx
            .debug_bounds("finder-purge-every-search")
            .expect("functional source purge toggle");
        cx.simulate_click(purge.center(), gpui::Modifiers::default());
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(saved["FinderConfig"]["purgeBeforeEverySearch"], true);
        assert_eq!(saved["FinderConfig"]["future"]["keep"], 42);
        view.update(cx, |view, _| view.configure_finder(&saved));
        view.read_with(cx, |view, _| {
            assert!(view.files_search.results_purge);
            assert_eq!(view.files_search.results.searches.len(), 1);
            assert_eq!(view.editor().committed_text(), "foo");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_context_folds_unfolds_and_clears_all_without_recounting_or_editing(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.editor_mut().set_read_only(true);
            view.configure_find(&serde_json::json!({"Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("find-all-current").unwrap();
        for _ in 0..2 {
            dialog.simulate_click(search.center(), gpui::Modifiers::default());
            cx.run_until_parked();
        }
        let summary = view.read_with(cx, |view, _| view.files_search.message.clone());
        for command in ["finder-fold-all", "finder-unfold-all", "finder-clear-all"] {
            let header = cx.debug_bounds("finder-search-0").unwrap();
            cx.simulate_event(gpui::MouseDownEvent {
                position: gpui::point(header.left() + px(20.), header.center().y),
                button: MouseButton::Right,
                ..Default::default()
            });
            let action = cx
                .debug_bounds(command)
                .expect("implemented source Finder command");
            cx.simulate_click(action.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(view.files_search.message, summary);
                assert_eq!(view.editor().committed_text(), "foo\nfoo");
                assert!(!view.editor().document().can_undo());
                assert!(!view.workspace.active().is_modified());
                if command == "finder-fold-all" {
                    assert!(
                        view.files_search
                            .results
                            .searches
                            .iter()
                            .all(|search| search.collapsed
                                && search.documents.iter().all(|doc| doc.collapsed))
                    );
                }
            });
            if command == "finder-fold-all" {
                assert!(cx.debug_bounds("finder-document-0-0").is_none());
            }
            if command == "finder-unfold-all" {
                assert!(cx.debug_bounds("finder-hit-1-0-0").is_some());
            }
        }
        assert!(cx.debug_bounds("files-results").is_some());
        assert!(cx.debug_bounds("finder-search-0").is_none());
        view.read_with(cx, |view, _| {
            assert!(view.files_search.results.searches.is_empty())
        });
    }

    #[gpui::test]
    fn finder_plain_verbatim_copy_uses_source_nul_terminated_paste_boundary(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\0tail\nnext foo");
            view.show_find(window, cx);
            view.find_query
                .update(cx, |input, cx| input.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(!copied.contains('\0'));
            assert!(copied.ends_with("\tLine 1: foo"));
        });
        let search = cx.debug_bounds("finder-search-0").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(search.left() + px(10.), search.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let lines = cx.debug_bounds("finder-copy-selected-lines").unwrap();
        cx.simulate_click(lines.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("foo\r\nnext foo\r\n")
            )
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "foo\0tail\nnext foo");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn editor_origin_drag_does_not_take_over_a_previous_finder_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let first = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let second = cx.debug_bounds("finder-hit-0-0-1").unwrap();
        cx.simulate_click(
            gpui::point(first.left() + px(10.), first.center().y),
            gpui::Modifiers::default(),
        );
        let before = view.read_with(cx, |view, _| {
            (view.files_search.text_anchor, view.files_search.text_head)
        });
        let editor = cx.debug_bounds("matrix-editor").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: editor.origin + gpui::point(px(100.), px(10.)),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: gpui::point(second.left() + px(40.), second.center().y),
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: gpui::point(second.left() + px(40.), second.center().y),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                (view.files_search.text_anchor, view.files_search.text_head),
                before
            )
        });
    }

    #[gpui::test]
    fn finder_keyboard_range_copy_excludes_the_following_line_at_column_zero(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        cx.simulate_click(
            gpui::point(hit.left() + px(8.), hit.center().y),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("shift-down");
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("\tLine 1: foo\r\n")
            )
        });
        let next = cx.debug_bounds("finder-hit-0-0-1").unwrap();
        cx.simulate_event(gpui::MouseDownEvent {
            position: gpui::point(next.left() + px(8.), next.center().y),
            button: MouseButton::Right,
            ..Default::default()
        });
        let copy = cx.debug_bounds("finder-copy-selected-lines").unwrap();
        cx.simulate_click(copy.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("foo\r\n")
            )
        });
        cx.simulate_keystrokes("up");
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("sentinel")
            )
        });
    }

    #[gpui::test]
    fn finder_drag_and_right_click_copy_selected_unicode_text_and_full_lines(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("中 foo\n中 bar");
            view.editor_mut().set_read_only(true);
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("中", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let first = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let second = cx.debug_bounds("finder-hit-0-0-1").unwrap();
        let (start, end) = view.update_in(cx, |view, window, _| {
            let (family, size) = view.finder_font();
            let shape = |text: &'static str| {
                window.text_system().shape_line(
                    text.into(),
                    size,
                    &[gpui::TextRun {
                        len: text.len(),
                        font: gpui::font(family),
                        color: rgb(0).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
            };
            (
                gpui::point(
                    first.left() + px(8.) + shape("\tLine 1: 中 foo").x_for_index(9),
                    first.center().y,
                ),
                gpui::point(
                    second.left() + px(8.) + shape("\tLine 2: 中 bar").x_for_index(12),
                    second.center().y,
                ),
            )
        });
        cx.simulate_event(gpui::MouseDownEvent {
            position: start,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: end,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("中 foo\r\n\tLine 2: 中")
            )
        });
        cx.simulate_event(gpui::MouseDownEvent {
            position: end,
            button: MouseButton::Right,
            ..Default::default()
        });
        let copy = cx.debug_bounds("finder-copy-selected-lines").unwrap();
        cx.simulate_click(copy.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("中 foo\r\n中 bar\r\n")
            )
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "中 foo\n中 bar");
            assert!(!view.editor().document().can_undo());
            assert!(!view.workspace.active().is_modified());
        });
    }

    #[gpui::test]
    fn finder_select_all_and_verbatim_copy_include_folded_result_text_not_source_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo foo\nfoo");
            view.editor_mut().set_read_only(true);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let doc = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_click(
            gpui::point(doc.left() + px(3.), doc.center().y),
            gpui::Modifiers::default(),
        );
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorCopy);
        cx.run_until_parked();
        cx.update(|_, cx| {
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(
                copied.starts_with("Search \"foo\" (3 hits in 1 file of 1 searched) [Normal]\r\n")
            );
            assert!(copied.contains("  new 1 (3 hits)\r\n"));
            assert!(copied.ends_with("\tLine 1: foo foo\r\n\tLine 2: foo\r\n"));
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "foo foo\nfoo");
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(3)
                }
            );
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_context_copy_lines_and_pathnames_use_results_not_the_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo foo\nfoo");
            view.editor_mut().set_read_only(true);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let doc = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_click(
            gpui::point(doc.left() + px(3.), doc.center().y),
            gpui::Modifiers::default(),
        );
        let point = gpui::point(doc.left() + px(20.), doc.center().y);
        for (command, expected) in [
            ("finder-copy-selected-lines", "foo foo\r\nfoo\r\n"),
            ("finder-copy-selected-pathnames", "new 1\r\n"),
        ] {
            cx.simulate_event(gpui::MouseDownEvent {
                position: point,
                button: MouseButton::Right,
                ..Default::default()
            });
            let action = cx
                .debug_bounds(command)
                .expect("source Finder copy command");
            cx.simulate_click(action.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some(expected)
                )
            });
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "foo foo\nfoo");
                assert!(view.editor().is_read_only());
                assert!(!view.editor().document().can_undo());
                assert!(!view.workspace.active().is_modified());
            });
        }
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        cx.simulate_click(point, gpui::Modifiers::default());
        cx.dispatch_action(EditorCopy);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("sentinel")
            )
        });
    }

    #[gpui::test]
    fn finder_delete_removes_rows_and_collapsed_groups_without_document_edits(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo foo\nfoo");
            view.editor_mut().insert_text("x").unwrap();
            assert!(view.editor_mut().undo());
            view.editor_mut().set_read_only(true);
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        cx.simulate_click(
            gpui::point(hit.left() + px(10.), hit.center().y),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("delete");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.files_search.results.searches[0].documents[0]
                    .lines
                    .len(),
                1
            );
            assert_eq!(
                view.files_search.results.searches[0].documents[0].lines[0].number,
                2
            );
            assert_eq!(view.files_search.results.searches[0].hits, 3);
        });
        let doc = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_click(
            gpui::point(doc.left() + px(3.), doc.center().y),
            gpui::Modifiers::default(),
        );
        cx.simulate_click(
            gpui::point(doc.left() + px(10.), doc.center().y),
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("delete");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.files_search.results.searches[0].documents.is_empty())
        });
        cx.simulate_keystrokes("up delete delete");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.files_search.results.searches.is_empty());
            assert!(view.files_search.results_visible);
            assert_eq!(view.editor().committed_text(), "foo foo\nfoo");
            assert!(view.editor().is_read_only());
            assert!(view.editor().document().can_redo());
            assert!(!view.editor().document().can_undo());
            assert!(!view.workspace.active().is_modified());
        });
    }

    #[gpui::test]
    fn finder_tracks_width_from_displayed_rows_instead_of_scanning_all_results(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(gpui::size(px(480.), px(500.)));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() =
                EditorSession::new(&format!("{}foo{}foo", "foo\n".repeat(200), " ".repeat(300)));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.files_search
                    .results_scroll
                    .0
                    .borrow()
                    .base_handle
                    .max_offset()
                    .x
                    < px(1000.)
            )
        });
        let panel = cx.debug_bounds("files-results").unwrap();
        cx.simulate_event(gpui::ScrollEvent {
            position: panel.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-20000.))),
            ..Default::default()
        });
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        view.read_with(cx, |view, _| {
            assert!(
                view.files_search
                    .results_scroll
                    .0
                    .borrow()
                    .base_handle
                    .max_offset()
                    .x
                    > px(1000.),
                "offset={:?}, max={:?}, widest={:?}, width={:?}",
                view.files_search
                    .results_scroll
                    .0
                    .borrow()
                    .base_handle
                    .offset(),
                view.files_search
                    .results_scroll
                    .0
                    .borrow()
                    .base_handle
                    .max_offset(),
                view.files_search.results_width_item,
                view.files_search.results_width
            )
        });
    }

    #[gpui::test]
    fn finder_horizontal_scroll_reaches_later_occurrences_and_enter_keeps_the_column(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(gpui::size(px(480.), px(500.)));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&format!("foo{}foo", " ".repeat(150)));
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let panel = cx.debug_bounds("files-results").unwrap();
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        cx.simulate_event(gpui::ScrollEvent {
            position: gpui::point(panel.center().x, hit.center().y),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-2000.), px(0.))),
            ..Default::default()
        });
        cx.run_until_parked();
        let point = gpui::point(panel.right() - px(3.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(153)..ByteOffset(156)
            )
        });
        cx.simulate_click(point, gpui::Modifiers::default());
        view.update(cx, |view, _| {
            view.editor_mut().select(Selection::default()).unwrap()
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(153)..ByteOffset(156)
            );
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_navigation_keeps_the_source_filename_after_save_as(cx: &mut gpui::TestAppContext) {
        let files = tempfile::tempdir().unwrap();
        let original = files.path().join("original.txt");
        let renamed = files.path().join("renamed.txt");
        std::fs::write(&original, "foo").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            let (file, text) = crate::files::DiskFile::load_unicode(&original).unwrap();
            view.workspace.open_buffer(file, &text).unwrap();
            view.reset_active_view(window, cx);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.dispatch_action(SaveFileAs);
        cx.simulate_new_path_selection(|_| Some(renamed.clone()));
        cx.run_until_parked();
        assert!(renamed.is_file());
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let point = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.workspace
                    .active()
                    .disk_file
                    .as_ref()
                    .unwrap()
                    .path()
                    .file_name()
                    .unwrap(),
                "original.txt"
            );
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(3)
            );
        });
    }

    #[gpui::test]
    fn finder_reveals_an_unchanged_match_selection_after_manual_document_scrolling(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&format!("foo\n{}", "line\n".repeat(200)));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let initial_hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let initial_point = gpui::point(initial_hit.left() + px(10.), initial_hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: initial_point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: initial_point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        let editor = cx.debug_bounds("matrix-editor").unwrap();
        cx.simulate_event(gpui::ScrollEvent {
            position: editor.origin + gpui::point(px(100.), px(100.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-2000.))),
            ..Default::default()
        });
        view.read_with(cx, |view, _| assert!(view.editor_top_line > 0));
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let point = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor_top_line, 0);
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(3)
            );
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_fold_click_reconciles_hidden_result_selection_and_keeps_keyboard_focus(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo\nfoo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let point = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        let header = cx.debug_bounds("finder-document-0-0").unwrap();
        cx.simulate_click(
            gpui::point(header.left() + px(3.), header.center().y),
            gpui::Modifiers::default(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.files_search.results_selected, 1)
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.files_search.results.searches[0].documents[0].collapsed);
            assert_eq!(view.editor().committed_text(), "foo\nfoo");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn finder_buffer_navigation_respects_existing_busy_interlock(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("foo")
        });
        cx.dispatch_action(NewFile);
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo");
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-open").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update(cx, |view, _| view.file_busy = true);
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let point = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: point,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), crate::workspace::BufferId(2));
            assert!(view.file_busy);
            view.file_busy = false;
        });
    }

    #[gpui::test]
    fn current_find_all_keeps_selection_scope_full_selection_and_zero_hit_branches(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        for (end, query, expected, visible) in [
            (
                3,
                "foo",
                "Search \"foo\" (1 hit in 1 selection of 1 searched) [Normal]",
                false,
            ),
            (
                7,
                "foo",
                "Search \"foo\" (2 hits in 1 file of 1 searched) [Normal]",
                false,
            ),
            (
                7,
                "missing",
                "Search \"missing\" (0 hits in 0 files of 1 searched) [Normal]",
                true,
            ),
        ] {
            view.update_in(cx, |view, window, cx| {
                *view.editor_mut() = EditorSession::new("foo foo");
                view.editor_mut()
                    .select(Selection {
                        anchor: ByteOffset(end),
                        head: ByteOffset(0),
                    })
                    .unwrap();
                view.configure_find(
                    &serde_json::json!({"Searching": {"inSelectionAutocheckThreshold": 1}}),
                );
                view.show_find(window, cx);
                view.find_query
                    .update(cx, |field, cx| field.set_text(query, window, cx));
            });
            cx.run_until_parked();
            let handle = view.read_with(cx, |view, _| {
                view.floating_windows[&floating_windows::DialogKind::Find]
            });
            let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
            let current = dialog.debug_bounds("find-all-current").unwrap();
            dialog.simulate_click(current.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(view.files_search.results.searches[0].title, expected);
                assert_eq!(view.find_visible, visible);
                assert_eq!(
                    view.editor().selection(),
                    Selection {
                        anchor: ByteOffset(end),
                        head: ByteOffset(0)
                    }
                );
                assert!(!view.editor().document().can_undo());
                assert!(!view.workspace.active().is_modified());
            });
        }
    }

    #[gpui::test]
    fn result_navigation_keeps_source_cached_ranges_after_unrelated_document_changes(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("foo");
            view.editor_mut()
                .move_to_document_edge(true, false)
                .unwrap();
            view.configure_find(
                &serde_json::json!({"Searching": {"fillFindFieldWithSelected": false}}),
            );
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let current = dialog.debug_bounds("find-all-current").unwrap();
        dialog.simulate_click(current.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| view.editor_focus.focus(window, cx));
        cx.simulate_input(" tail");
        let hit = cx.debug_bounds("finder-hit-0-0-0").unwrap();
        let prefix = gpui::point(hit.left() + px(10.), hit.center().y);
        cx.simulate_event(gpui::MouseDownEvent {
            position: prefix,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: prefix,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "foo tail");
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(3)
            );
        });
    }

    #[gpui::test]
    fn all_open_document_find_all_uses_buffer_text_deduplicates_clones_and_keeps_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("foo")
        });
        cx.dispatch_action(CloneToOtherView);
        cx.dispatch_action(NewFile);
        view.update_in(cx, |view, window, cx| {
            view.editor_mut().insert_text("foo foo").unwrap();
            view.editor_mut().select(Selection { anchor: ByteOffset(3), head: ByteOffset(0) }).unwrap();
            view.configure_find(&serde_json::json!({"Searching": {"inSelectionAutocheckThreshold": 1, "findDlgAlwaysVisible": true}}));
            view.find_options.wrap = false;
            view.find_backward = true;
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("find-all-open").unwrap();
        for expected_searches in [1, 2] {
            dialog.simulate_click(all.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                let results = &view.files_search.results;
                assert_eq!(results.searches.len(), expected_searches);
                assert_eq!(
                    results.searches[0].title,
                    "Search \"foo\" (3 hits in 2 files of 2 searched) [Normal]"
                );
                assert_eq!(results.searches[0].documents.len(), 2);
                assert_eq!(results.searches[0].documents[1].lines[0].ranges.len(), 2);
                assert!(view.find_visible);
                assert_eq!(
                    view.editor().selection(),
                    Selection {
                        anchor: ByteOffset(3),
                        head: ByteOffset(0)
                    }
                );
                assert_eq!(view.editor().committed_text(), "foo foo");
                assert!(view.editor().document().can_undo());
                if expected_searches == 2 {
                    assert!(results.searches[1].collapsed);
                }
            });
        }
        cx.simulate_keystrokes("x");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().committed_text(),
                "foo foo",
                "typing in the readonly finder must not edit the active document"
            )
        });
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod folder_command_tests {
    use super::*;

    #[gpui::test]
    fn file_search_form_matches_reference_fields_and_applies_visible_filters(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = tempfile::tempdir().unwrap();
        std::fs::create_dir(fixture.path().join(".secret")).unwrap();
        std::fs::write(fixture.path().join("visible.txt"), "foo_ foo").unwrap();
        std::fs::write(fixture.path().join(".secret/hit.txt"), "foo_ foo").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.configure_find(&serde_json::json!({"Searching": {"findDlgAlwaysVisible": true}}));
            view.show_find_in_directory(fixture.path().to_path_buf(), window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("foo", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let root = dialog.debug_bounds("find-in-files-dialog").unwrap();
        for (id, x, y, width) in [
            ("find-query", 114., 32., 255.),
            ("files-filters", 114., 91., 255.),
            ("files-directory", 75., 120., 294.),
        ] {
            let input = dialog.debug_bounds(id).unwrap();
            assert_eq!(input.left() - root.left(), px(x));
            assert_eq!(input.top() - root.top(), px(y));
            assert_eq!(input.size, size(px(width), px(21.)));
        }
        let whole = dialog.debug_bounds("files-whole-word").unwrap();
        dialog.simulate_click(whole.center(), gpui::Modifiers::default());
        let find = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.files_search.results.searches[0].hits, 1)
        });
        let hidden = dialog
            .debug_bounds("files-hidden")
            .expect("source hidden-folders option");
        dialog.simulate_click(hidden.center(), gpui::Modifiers::default());
        dialog.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.files_search.results.searches[0].hits, 2);
            assert_eq!(view.editor().committed_text(), "unchanged");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn folder_find_command_targets_chosen_directory_with_source_prefill(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = tempfile::tempdir().expect("fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.editor_mut().insert_text("unchanged").expect("edit");
            view.show_find_in_directory(fixture.path().to_path_buf(), window, cx);
            assert!(view.files_search.visible);
            assert_eq!(
                view.files_search.directory.read(cx).text(),
                fixture.path().to_string_lossy()
            );
            assert_eq!(view.editor().committed_text(), "unchanged");
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(9)
                }
            );
            assert_eq!(view.find_query.read(cx).text(), "unchanged");
            assert!(view.editor_mut().undo());
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
        });
    }
}
use crate::find_in_files::FileSearchRequest;

pub(super) struct FileSearchState {
    views: finder_views::State,
    pub visible: bool,
    focus: FocusHandle,
    results_visible: bool,
    directory: gpui::Entity<text_field::TextField>,
    filters: gpui::Entity<text_field::TextField>,
    recursive: bool,
    hidden_directories: bool,
    cancel: Option<Arc<AtomicBool>>,
    results: crate::finder::Results,
    results_focus: FocusHandle,
    results_scroll: gpui::UniformListScrollHandle,
    wrapped_scroll: gpui::ListState,
    results_wrap: bool,
    wrapped_preferred_x: Option<gpui::Pixels>,
    results_selected: usize,
    results_width_item: Option<usize>,
    results_width: gpui::Pixels,
    results_cursor_x: Option<gpui::Pixels>,
    text_anchor: Option<crate::finder::TextPoint>,
    text_head: Option<crate::finder::TextPoint>,
    dragging_text: bool,
    context: Option<gpui::Point<gpui::Pixels>>,
    context_focus: FocusHandle,
    context_selected: usize,
    results_purge: bool,
    results_one_per_line: bool,
    always_visible: bool,
    message: String,
}

impl FileSearchState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            views: finder_views::State::new(cx),
            visible: false,
            focus: cx.focus_handle(),
            results_visible: false,
            directory: cx
                .new(|cx| text_field::TextField::compact("Directory", "files-directory", cx)),
            filters: cx.new(|cx| text_field::TextField::compact("Filters", "files-filters", cx)),
            recursive: true,
            hidden_directories: false,
            cancel: None,
            results: crate::finder::Results::default(),
            results_focus: cx.focus_handle(),
            results_scroll: gpui::UniformListScrollHandle::new(),
            wrapped_scroll: gpui::ListState::new(0, gpui::ListAlignment::Top, px(100.)),
            results_wrap: false,
            wrapped_preferred_x: None,
            results_selected: 0,
            results_width_item: None,
            results_width: px(0.),
            results_cursor_x: None,
            text_anchor: None,
            text_head: None,
            dragging_text: false,
            context: None,
            context_focus: cx.focus_handle(),
            context_selected: 0,
            results_purge: false,
            results_one_per_line: true,
            always_visible: false,
            message: String::new(),
        }
    }
}

impl Drop for FileSearchState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl MatrixWindow {
    pub(super) fn cancel_finder_drag(&mut self) {
        self.files_search.dragging_text = false;
    }

    fn reveal_finder_row(&self, index: usize, strategy: gpui::ScrollStrategy) {
        self.files_search
            .results_scroll
            .scroll_to_item(index, strategy);
        if self.files_search.results_wrap {
            if matches!(strategy, gpui::ScrollStrategy::Top) {
                self.files_search
                    .wrapped_scroll
                    .scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: px(0.),
                    });
            } else {
                self.files_search
                    .wrapped_scroll
                    .scroll_to_reveal_item(index);
            }
        }
    }
    pub(super) fn search_history_input(
        &self,
        field: crate::search_history::Field,
    ) -> gpui::Entity<text_field::TextField> {
        match field {
            crate::search_history::Field::Find => self.find_query.clone(),
            crate::search_history::Field::Replace => self.replace_input.clone(),
            crate::search_history::Field::Filter => self.files_search.filters.clone(),
            crate::search_history::Field::Directory => self.files_search.directory.clone(),
        }
    }

    pub(super) fn persist_find_option(
        &mut self,
        key: &'static str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(store) = &self.settings_store else {
            return true;
        };
        if let Err(error) = store.update_settings(|settings| {
            if !settings["FindHistory"].is_null() && !settings["FindHistory"].is_object() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "FindHistory must be an object",
                ));
            }
            settings["FindHistory"][key] = value;
            Ok(())
        }) {
            self.find_message = format!("Search settings could not be saved: {error}");
            self.notify_editor_error(self.find_message.clone(), cx);
            return false;
        }
        true
    }

    fn finder_font(&self) -> (&'static str, gpui::Pixels) {
        let style = crate::languages::catalog().ok().and_then(|catalog| {
            catalog
                .themes()
                .theme(self.dark)
                .global_styles
                .iter()
                .find(|style| style.id == 32)
        });
        let family = style
            .and_then(|style| style.attributes.get("fontName"))
            .filter(|value| !value.is_empty())
            .map_or("Courier New", String::as_str);
        let points = style
            .and_then(|style| style.attributes.get("fontSize"))
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(10.);
        (family, px(points * 96. / 72.))
    }

    pub(super) fn configure_finder(&mut self, settings: &serde_json::Value) {
        self.files_search.recursive = settings["FindHistory"]["fifRecuisive"]
            .as_bool()
            .unwrap_or(true);
        self.files_search.hidden_directories = settings["FindHistory"]["fifInHiddenFolder"]
            .as_bool()
            .unwrap_or(false);
        self.files_search.always_visible = settings["Searching"]["findDlgAlwaysVisible"]
            .as_bool()
            .unwrap_or(false);
        self.files_search.results_purge = settings["FinderConfig"]["purgeBeforeEverySearch"]
            .as_bool()
            .unwrap_or(false);
        self.files_search.results_wrap =
            settings["FinderConfig"]["wrap"].as_bool().unwrap_or(false);
        self.files_search.results_one_per_line =
            settings["FinderConfig"]["showOnlyOneEntryPerFoundLine"]
                .as_bool()
                .unwrap_or(true);
    }

    pub(super) fn find_all_documents(
        &mut self,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.find_all_documents(all, window, cx)
        }) {
            return;
        }
        if !self.find_visible
            || self.find_replace_mode
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || self.files_search.cancel.is_some()
        {
            return;
        }
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        self.record_search_history(crate::search_history::Field::Find, window, cx);
        let ids = if all {
            let mut seen = std::collections::BTreeSet::new();
            [
                crate::workspace::ViewId::Primary,
                crate::workspace::ViewId::Secondary,
            ]
            .into_iter()
            .flat_map(|pane| self.workspace.view_buffer_ids(pane).iter().copied())
            .filter(|id| seen.insert(*id))
            .collect::<Vec<_>>()
        } else {
            vec![self.workspace.active().id()]
        };
        let mut documents = Vec::new();
        for id in ids {
            let Some(buffer) = self
                .workspace
                .buffers()
                .iter()
                .find(|buffer| buffer.id() == id)
            else {
                continue;
            };
            if buffer.editor.has_composition() {
                self.find_message =
                    "Find All during active document composition is not implemented yet".into();
                cx.notify();
                return;
            }
            let path = buffer
                .disk_file
                .as_ref()
                .map(|file| file.path().to_path_buf());
            documents.push(crate::finder::Document {
                name: path
                    .as_ref()
                    .map_or_else(|| buffer.name(), |path| path.display().to_string()),
                text: Arc::from(buffer.editor.committed_text()),
                selection: (!all && self.find_in_selection_active)
                    .then(|| self.editor().selection_limits()),
            });
        }
        match crate::finder::search_documents(
            &query,
            documents,
            self.find_options,
            self.files_search.results_one_per_line,
        ) {
            Ok(search) => {
                self.find_message.clear();
                self.publish_search_results(search, window, cx);
            }
            Err(error) => {
                self.find_message = error.to_string();
                cx.notify();
            }
        }
    }

    fn publish_search_results(
        &mut self,
        search: crate::finder::Search,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_main_finder(cx);
        self.cancel_finder_drag();
        let hits = search.hits;
        self.files_search
            .results
            .push(search, self.files_search.results_purge);
        self.files_search.results_selected = 0;
        self.files_search.text_anchor = None;
        self.files_search.text_head = None;
        self.files_search.context = None;
        self.files_search.results_width_item = None;
        self.files_search.results_width = px(0.);
        self.files_search.results_cursor_x = None;
        self.files_search.wrapped_preferred_x = None;
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.reveal_finder_row(0, gpui::ScrollStrategy::Top);
        self.files_search.results_visible = true;
        self.files_search.message = format!(
            "Search results - ({} {})",
            crate::finder::commafy(hits),
            if hits == 1 { "hit" } else { "hits" }
        );
        if hits > 0 {
            if !self.files_search.always_visible {
                self.close_search_history();
                self.find_visible = false;
                self.files_search.visible = false;
            }
            if !self.close_dialog {
                window.activate_window();
                self.files_search.results_focus.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn show_find_in_directory(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = directory.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.show_find_in_directory(routed, window, cx)
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.show_find_in_files(window, cx);
            return;
        }
        self.show_find_in_files(window, cx);
        if self.files_search.visible {
            let directory = directory.to_string_lossy().into_owned();
            self.files_search
                .directory
                .update(cx, |field, cx| field.set_text(&directory, window, cx));
            cx.notify();
        }
    }

    pub(super) fn show_find_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find_in_files(true, window, cx);
    }

    pub(super) fn open_find_in_files(
        &mut self,
        prefill: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.open_find_in_files(prefill, window, cx)
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.notify_editor_error(
                "Find in Files during active composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        self.close_search_history();
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.find_visible = false;
        self.initialize_search_history(window, cx);
        if self.files_search.filters.read(cx).text().is_empty() {
            self.files_search
                .filters
                .update(cx, |field, cx| field.set_text("*.*", window, cx));
        }
        if self.files_search.directory.read(cx).text().is_empty()
            && let Some(parent) = self
                .workspace
                .active()
                .disk_file
                .as_ref()
                .and_then(|file| file.path().parent())
        {
            let directory = parent.to_string_lossy().into_owned();
            self.files_search
                .directory
                .update(cx, |field, cx| field.set_text(&directory, window, cx));
        }
        self.files_search.visible = true;
        if prefill {
            self.prepare_find_activation(cx);
        }
        self.present_floating(floating_windows::DialogKind::Find, window, cx);
        if prefill {
            self.fill_find_query(window, cx);
        }
        cx.notify();
    }

    pub(super) fn start_file_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.start_file_search(window, cx)
        }) {
            return;
        }
        if !self.files_search.visible {
            return;
        }
        if self.files_search.cancel.is_some()
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        self.record_search_histories(
            &[
                crate::search_history::Field::Filter,
                crate::search_history::Field::Find,
            ],
            window,
            cx,
        );
        let directory = self.captured_search_input(crate::search_history::Field::Directory, cx);
        if directory.is_empty() {
            self.files_search.message = "Enter a directory to search".into();
            self.focus_search_history_input(crate::search_history::Field::Directory, cx);
            cx.notify();
            return;
        }
        self.record_search_history(crate::search_history::Field::Directory, window, cx);
        self.files_search
            .directory
            .update(cx, |field, cx| field.set_text(&directory, window, cx));
        if query.is_empty() {
            return;
        }
        if self
            .workspace
            .buffers()
            .iter()
            .any(|buffer| buffer.editor.marked_range().is_some())
        {
            self.files_search.message =
                "Search with active document composition is not implemented yet".into();
            cx.notify();
            return;
        }
        let request = FileSearchRequest {
            mode: self.find_options.mode,
            directory: PathBuf::from(directory),
            filters: self.captured_search_input(crate::search_history::Field::Filter, cx),
            query,
            match_case: self.find_options.match_case,
            whole_word: self.find_options.whole_word,
            recursive: self.files_search.recursive,
            hidden_directories: self.files_search.hidden_directories,
            open_documents: self
                .workspace
                .buffers()
                .iter()
                .filter_map(|buffer| {
                    buffer.disk_file.as_ref().map(|file| {
                        (
                            file.path().to_path_buf(),
                            Arc::from(buffer.editor.committed_text()),
                        )
                    })
                })
                .collect(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let query = request.query.clone();
        let options = self.find_options;
        let one_per_line = self.files_search.results_one_per_line;
        self.files_search.cancel = Some(cancel.clone());
        self.activate_main_finder(cx);
        self.cancel_finder_drag();
        self.files_search
            .results
            .begin(self.files_search.results_purge);
        self.files_search.text_anchor = None;
        self.files_search.text_head = None;
        self.files_search.results_width_item = None;
        self.files_search.results_width = px(0.);
        self.files_search.results_selected = 0;
        self.files_search.results_cursor_x = None;
        self.files_search.wrapped_preferred_x = None;
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.reveal_finder_row(0, gpui::ScrollStrategy::Top);
        self.files_search.results_visible = true;
        self.files_search.message = "Searching files…".into();
        let task = cx.background_executor().spawn(async move {
            let result = crate::find_in_files::search_files(request, &cancel);
            crate::finder::from_file_search(&query, result, options, one_per_line)
        });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let results = task.await;
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.files_search.cancel = None;
                    view.publish_search_results(results, window, cx);
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("file search completion could not reach its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn open_result_target(
        &mut self,
        path: PathBuf,
        range: std::ops::Range<ByteOffset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
        {
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        let opened = self
            .workspace
            .buffers()
            .iter()
            .find(|buffer| {
                buffer.disk_file.as_ref().map_or_else(
                    || std::path::Path::new(&buffer.name()) == path,
                    |file| file.path() == path,
                )
            })
            .map(|buffer| buffer.id());
        if let Some(id) = opened {
            if self.activate_buffer(id, window, cx) {
                self.select_result_range(range, window, cx);
            }
            return;
        }
        self.file_busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { crate::files::DiskFile::load_unicode(&path) });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.file_busy = false;
                    match result {
                        Ok((file, text)) => {
                            if !view.cancel_view_composition(window, cx) {
                                return;
                            }
                            match view.workspace.open_buffer(file, &text) {
                                Ok(_) => {
                                    view.reset_active_view(window, cx);
                                    view.select_result_range(range, window, cx);
                                }
                                Err(error) => view.notify_editor_error(error.to_string(), cx),
                            }
                        }
                        Err(error) => view
                            .notify_editor_error(format!("Open search result failed: {error}"), cx),
                    }
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("search navigation completion could not reach its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn select_result_range(
        &mut self,
        range: std::ops::Range<ByteOffset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor().has_composition() && !self.cancel_view_composition(window, cx) {
            return;
        }
        self.editor_focus.focus(window, cx);
        let range =
            crate::finder::normalize_cached_range(&self.editor().document().text_snapshot(), range);
        let result = self.editor_mut().select(Selection {
            anchor: range.start,
            head: range.end,
        });
        if result.is_ok() {
            self.reveal_editor_selection();
        }
        self.editor_changed(result, window, cx);
    }

    pub(super) fn render_find_in_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.files_search.visible {
            return None;
        }
        let busy = self.files_search.cancel.is_some();
        Some(
            div()
                .id("find-in-files-dialog")
                .debug_selector(|| "find-in-files-dialog".into())
                .role(Role::Dialog)
                .aria_label("Find in Files")
                .track_focus(&self.files_search.focus)
                .key_context("MatrixFindInFiles")
                .size_full()
                .relative()
                .font_family("Helvetica")
                .text_size(px(11.))
                .line_height(px(16.))
                .bg(rgb(0xf9f9f9))
                .text_color(rgb(0x202020))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.render_search_tabs(cx))
                .child(find_surface::input_row(
                    "Find what:",
                    32.,
                    self.search_history_combo(crate::search_history::Field::Find, cx),
                ))
                .child(find_surface::input_row(
                    "Filters:",
                    91.,
                    self.search_history_combo(crate::search_history::Field::Filter, cx),
                ))
                .child(
                    div()
                        .absolute()
                        .left(px(3.))
                        .right(px(248.))
                        .top(px(120.))
                        .h(px(21.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            div()
                                .w(px(68.))
                                .flex_shrink_0()
                                .text_right()
                                .child("Directory:"),
                        )
                        .child(div().flex_1().min_w(px(0.)).child(
                            self.search_history_combo(crate::search_history::Field::Directory, cx),
                        )),
                )
                .child(
                    find_surface::checkbox(
                        "files-recursive",
                        "In all sub-folders",
                        self.files_search.recursive,
                    )
                    .absolute()
                    .left(px(450.))
                    .top(px(142.))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.files_search.recursive = !view.files_search.recursive;
                        if !view.persist_find_option(
                            "fifRecuisive",
                            view.files_search.recursive.into(),
                            cx,
                        ) {
                            view.files_search.recursive = !view.files_search.recursive;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    find_surface::checkbox(
                        "files-hidden",
                        "In hidden folders",
                        self.files_search.hidden_directories,
                    )
                    .absolute()
                    .left(px(450.))
                    .top(px(162.))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.files_search.hidden_directories =
                            !view.files_search.hidden_directories;
                        if !view.persist_find_option(
                            "fifInHiddenFolder",
                            view.files_search.hidden_directories.into(),
                            cx,
                        ) {
                            view.files_search.hidden_directories =
                                !view.files_search.hidden_directories;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(18.))
                        .top(px(142.))
                        .flex()
                        .flex_col()
                        .child(self.render_find_check(
                            "files-whole-word",
                            find_surface::FindCheck::WholeWord,
                            cx,
                        ))
                        .child(self.render_find_check(
                            "files-match-case",
                            find_surface::FindCheck::MatchCase,
                            cx,
                        )),
                )
                .child(
                    find_surface::button("files-find-all", "Find All", 32.)
                        .aria_label("Find All in Files")
                        .a11y_synthetic_children(move |builder| {
                            if busy {
                                builder.parent_node().set_disabled();
                            }
                        })
                        .on_click(
                            cx.listener(|view, _, window, cx| view.start_file_search(window, cx)),
                        ),
                )
                .child(
                    find_surface::button("files-close", "Close", 91.).on_click(cx.listener(
                        |view, _, window, cx| {
                            view.dismiss_floating(floating_windows::DialogKind::Find, window, cx);
                        },
                    )),
                )
                .child(find_surface::search_mode(self.find_options.mode, cx))
                .child(
                    div()
                        .id("files-dialog-message")
                        .role(Role::Group)
                        .aria_label(self.files_search.message.clone())
                        .absolute()
                        .bottom(px(0.))
                        .w_full()
                        .h(px(22.))
                        .px(px(9.))
                        .border_t_1()
                        .border_color(rgb(0xd6d6d6))
                        .truncate()
                        .child(self.files_search.message.clone()),
                )
                .into_any_element(),
        )
    }

    fn toggle_finder_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(toggled) = self.files_search.results.rows.get(index).copied() else {
            return;
        };
        let selected = self
            .files_search
            .results
            .rows
            .get(self.files_search.results_selected)
            .copied();
        let width_row = self
            .files_search
            .results_width_item
            .and_then(|index| self.files_search.results.rows.get(index))
            .copied();
        self.files_search.results.toggle(index);
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.files_search.results_width_item = width_row.and_then(|row| {
            self.files_search
                .results
                .rows
                .iter()
                .position(|candidate| *candidate == row)
        });
        if self.files_search.results_width_item.is_none() {
            self.files_search.results_width = px(0.);
        }
        let retained = selected.and_then(|selected| {
            self.files_search
                .results
                .rows
                .iter()
                .position(|row| *row == selected)
        });
        self.files_search.results_selected = retained
            .or_else(|| {
                self.files_search
                    .results
                    .rows
                    .iter()
                    .position(|row| *row == toggled)
            })
            .unwrap_or(0);
        if retained.is_none() {
            self.files_search.results_cursor_x = None;
            self.files_search.wrapped_preferred_x = None;
            self.files_search.text_anchor = None;
            self.files_search.text_head = None;
        }
        self.reveal_finder_row(
            self.files_search.results_selected,
            gpui::ScrollStrategy::Nearest,
        );
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn finder_caret(&self) -> crate::finder::TextPoint {
        self.files_search
            .text_head
            .unwrap_or_else(|| crate::finder::TextPoint {
                row: self
                    .files_search
                    .results
                    .rows
                    .get(self.files_search.results_selected)
                    .copied(),
                byte: 0,
            })
    }

    fn sync_finder_visible_row(&mut self) {
        let mut candidate = self.finder_caret().row;
        if candidate.is_none() {
            self.files_search.results_selected = self.files_search.results.rows.len();
            return;
        }
        while let Some(row) = candidate {
            if let Some(index) = self
                .files_search
                .results
                .rows
                .iter()
                .position(|visible| *visible == row)
            {
                self.files_search.results_selected = index;
                return;
            }
            candidate = match row {
                crate::finder::Row::Line(search, doc, _) => {
                    Some(crate::finder::Row::Document(search, doc))
                }
                crate::finder::Row::Document(search, _) | crate::finder::Row::Error(search, _) => {
                    Some(crate::finder::Row::Search(search))
                }
                crate::finder::Row::Search(_) => None,
            };
        }
        self.files_search.results_selected = 0;
    }

    fn delete_finder_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.finder_caret().row else {
            return;
        };
        let Some(caret) = self.files_search.results.delete_caret(row) else {
            return;
        };
        self.files_search.text_anchor = Some(caret);
        self.files_search.text_head = Some(caret);
        self.files_search.results_cursor_x = Some(px(8.));
        self.files_search.wrapped_preferred_x = None;
        self.sync_finder_visible_row();
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.cancel_finder_drag();
        self.files_search.results_width_item = None;
        self.files_search.results_width = px(0.);
        self.reveal_finder_row(
            self.files_search.results_selected,
            gpui::ScrollStrategy::Nearest,
        );
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn measure_finder_width(
        &mut self,
        range: std::ops::Range<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let (family, size) = self.finder_font();
        let widest = range
            .filter_map(|index| {
                let row = self.files_search.results.rows.get(index)?;
                let caption = self.files_search.results.caption(index)?;
                let caption = caption.split('\n').next().unwrap_or_default();
                let mut font = gpui::font(family);
                if matches!(
                    row,
                    crate::finder::Row::Search(_) | crate::finder::Row::Document(..)
                ) {
                    font.weight = gpui::FontWeight::BOLD;
                }
                let width = window
                    .text_system()
                    .shape_line(
                        caption.to_string().into(),
                        size,
                        &[gpui::TextRun {
                            len: caption.len(),
                            font,
                            color: rgb(0).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width;
                Some((index, width))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((index, width)) = widest
            && width > self.files_search.results_width
        {
            self.files_search.results_width = width;
            self.files_search.results_width_item = Some(index);
            cx.notify();
        }
    }

    fn activate_finder_row(
        &mut self,
        index: usize,
        x: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.files_search.results.rows.get(index).copied() else {
            return;
        };
        self.activate_finder_result(row, x, window, cx);
    }

    fn activate_finder_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.finder_caret().row;
        if let Some(row) = row {
            self.activate_finder_result(row, self.files_search.results_cursor_x, window, cx);
        }
    }

    fn activate_finder_result(
        &mut self,
        row: crate::finder::Row,
        x: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::finder::Row;
        let Row::Line(search, doc, line) = row else {
            if let Some(index) = self
                .files_search
                .results
                .rows
                .iter()
                .position(|candidate| *candidate == row)
            {
                self.toggle_finder_row(index, window, cx);
            } else {
                self.files_search.results.toggle_row(row);
                self.files_search
                    .wrapped_scroll
                    .reset(self.files_search.results.rows.len());
                self.files_search.results_width_item = None;
                self.files_search.results_width = px(0.);
                self.files_search.results_focus.focus(window, cx);
                cx.notify();
            }
            return;
        };
        let doc = &self.files_search.results.searches[search].documents[doc];
        let line = &doc.lines[line];
        let occurrence = x
            .and_then(|x| {
                let (family, font_size) = self.finder_font();
                let shape = window.text_system().shape_line(
                    line.caption.clone().into(),
                    font_size,
                    &[gpui::TextRun {
                        len: line.caption.len(),
                        font: gpui::font(family),
                        color: rgb(0).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let byte = shape.closest_index_for_x(x - px(8.));
                line.highlights
                    .iter()
                    .position(|range| range.start <= byte && byte <= range.end)
            })
            .unwrap_or(0);
        let Some(range) = line.ranges.get(occurrence).cloned() else {
            return;
        };
        let path = PathBuf::from(&doc.name);
        self.open_result_target(path, range, window, cx);
    }

    fn copy_finder(&mut self, command: FinderCopy, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.context = None;
        let selection = self
            .files_search
            .text_anchor
            .zip(self.files_search.text_head);
        let text = match command {
            FinderCopy::Verbatim => selection
                .filter(|(anchor, head)| anchor != head)
                .and_then(|(anchor, head)| self.files_search.results.copy_verbatim(anchor, head)),
            FinderCopy::Lines => self
                .finder_selected_rows()
                .and_then(|(anchor, head)| self.files_search.results.copy_lines(anchor, head)),
            FinderCopy::Pathnames => self
                .finder_selected_rows()
                .and_then(|(anchor, head)| self.files_search.results.copy_pathnames(anchor, head)),
        };
        if let Some(text) = text {
            let text = if matches!(command, FinderCopy::Verbatim) {
                text.split('\0').next().unwrap_or_default().to_owned()
            } else {
                text
            };
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn finder_selected_rows(&self) -> Option<(crate::finder::Row, crate::finder::Row)> {
        if let Some((anchor, head)) = self
            .files_search
            .text_anchor
            .zip(self.files_search.text_head)
            && anchor != head
        {
            self.files_search.results.selection_rows(anchor, head)
        } else {
            self.finder_caret().row.map(|row| (row, row))
        }
    }

    fn open_finder_selected_paths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.context = None;
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
        {
            self.files_search.results_focus.focus(window, cx);
            cx.notify();
            return;
        }
        let paths: Vec<_> = self
            .finder_selected_rows()
            .and_then(|(anchor, head)| self.files_search.results.selected_pathnames(anchor, head))
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect();
        if paths.is_empty() {
            self.files_search.results_focus.focus(window, cx);
            cx.notify();
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        let targets: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let opened = self.workspace.buffers().iter().any(|buffer| {
                    buffer.disk_file.as_ref().map_or_else(
                        || std::path::Path::new(&buffer.name()) == path,
                        |file| file.path() == path,
                    )
                });
                (path, opened)
            })
            .collect();
        self.file_busy = true;
        let task = cx.background_executor().spawn(async move {
            targets
                .into_iter()
                .map(|(path, opened)| {
                    let result = if opened {
                        Ok(None)
                    } else {
                        crate::files::DiskFile::load_unicode(&path).map(Some)
                    };
                    (path, result)
                })
                .collect::<Vec<_>>()
        });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let loaded = task.await;
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.file_busy = false;
                    let mut errors = Vec::new();
                    let mut opened_any = false;
                    for (path, result) in loaded {
                        let existing = view
                            .workspace
                            .buffers()
                            .iter()
                            .find(|buffer| {
                                buffer.disk_file.as_ref().map_or_else(
                                    || std::path::Path::new(&buffer.name()) == path,
                                    |file| file.path() == path,
                                )
                            })
                            .map(|buffer| buffer.id());
                        if let Some(id) = existing {
                            opened_any |= view.activate_buffer(id, window, cx);
                            continue;
                        }
                        match result {
                            Ok(Some((file, text))) => {
                                if !view.cancel_view_composition(window, cx) {
                                    return;
                                }
                                match view.workspace.open_buffer(file, &text) {
                                    Ok(_) => {
                                        view.reset_active_view(window, cx);
                                        opened_any = true;
                                    }
                                    Err(error) => {
                                        errors.push(format!("{}: {error}", path.display()))
                                    }
                                }
                            }
                            Ok(None) => errors.push(format!(
                                "{}: result document is no longer open",
                                path.display()
                            )),
                            Err(error) => errors.push(format!("{}: {error}", path.display())),
                        }
                    }
                    if !opened_any {
                        view.files_search.results_focus.focus(window, cx);
                    }
                    if !errors.is_empty() {
                        view.notify_editor_error(
                            format!("Open search results failed: {}", errors.join("; ")),
                            cx,
                        );
                    }
                    cx.notify();
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("open selected result paths could not reach its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn select_all_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.wrapped_preferred_x = None;
        self.files_search.text_anchor =
            self.files_search
                .results
                .rows
                .first()
                .copied()
                .map(|row| crate::finder::TextPoint {
                    row: Some(row),
                    byte: 0,
                });
        self.files_search.text_head = Some(crate::finder::TextPoint { row: None, byte: 0 });
        self.files_search.results_selected = self.files_search.results.rows.len();
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn finder_has_selection(&self) -> bool {
        self.files_search
            .text_anchor
            .zip(self.files_search.text_head)
            .is_some_and(|(anchor, head)| anchor != head)
    }

    fn finder_command_enabled(&self, command: FinderCommand) -> bool {
        if self.files_search.views.active != 0
            && matches!(
                command,
                FinderCommand::ClearAll | FinderCommand::TogglePurge
            )
        {
            return false;
        }
        if self.files_search.views.active == 0 && matches!(command, FinderCommand::CloseResults) {
            return false;
        }
        !matches!(command, FinderCommand::Copy(FinderCopy::Verbatim)) || self.finder_has_selection()
    }

    fn run_finder_command(
        &mut self,
        command: FinderCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_search.context = None;
        match command {
            FinderCommand::FindWithin => self.show_find_in_results(window, cx),
            FinderCommand::CloseResults => self.close_active_finder(window, cx),
            FinderCommand::OpenSelectedPathnames => self.open_finder_selected_paths(window, cx),
            FinderCommand::TogglePurge | FinderCommand::ToggleWrap => {
                let wrapping = matches!(command, FinderCommand::ToggleWrap);
                let key = if wrapping {
                    "wrap"
                } else {
                    "purgeBeforeEverySearch"
                };
                let enabled = if wrapping {
                    !self.files_search.results_wrap
                } else {
                    !self.files_search.results_purge
                };
                if self.files_search.views.active == 0
                    && let Some(store) = &self.settings_store
                    && let Err(error) = store.update_settings(|settings| {
                        if settings
                            .get("FinderConfig")
                            .is_some_and(|config| !config.is_object())
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "FinderConfig must be an object",
                            ));
                        }
                        let current = settings
                            .get("FinderConfig")
                            .and_then(|config| config.get(key));
                        if current.is_some_and(|current| !current.is_boolean()) {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("{key} must be a boolean"),
                            ));
                        }
                        settings["FinderConfig"][key] = enabled.into();
                        Ok(())
                    })
                {
                    self.notify_editor_error(
                        format!("Finder setting could not be saved: {error}"),
                        cx,
                    );
                    self.files_search.results_focus.focus(window, cx);
                    return;
                }
                if wrapping {
                    self.files_search.wrapped_preferred_x = None;
                    self.files_search.results_wrap = enabled;
                    self.files_search
                        .wrapped_scroll
                        .reset(self.files_search.results.rows.len());
                    self.files_search
                        .results_scroll
                        .0
                        .borrow()
                        .base_handle
                        .set_offset(gpui::Point::default());
                    self.reveal_finder_row(
                        self.files_search.results_selected,
                        gpui::ScrollStrategy::Nearest,
                    );
                } else {
                    self.files_search.results_purge = enabled;
                }
                self.files_search.results_focus.focus(window, cx);
                cx.notify();
            }
            FinderCommand::Copy(copy) => self.copy_finder(copy, window, cx),
            FinderCommand::SelectAll => self.select_all_finder(window, cx),
            FinderCommand::FoldAll | FinderCommand::UnfoldAll => {
                let caret = self.finder_caret();
                if self.files_search.text_head.is_none() {
                    self.files_search.text_anchor = Some(caret);
                    self.files_search.text_head = Some(caret);
                }
                self.files_search
                    .results
                    .fold_all(matches!(command, FinderCommand::FoldAll));
                self.sync_finder_visible_row();
                self.files_search
                    .wrapped_scroll
                    .reset(self.files_search.results.rows.len());
                self.files_search.results_width_item = None;
                self.files_search.results_width = px(0.);
                self.reveal_finder_row(
                    self.files_search.results_selected,
                    gpui::ScrollStrategy::Nearest,
                );
                self.files_search.results_focus.focus(window, cx);
                self.cancel_finder_drag();
                cx.notify();
            }
            FinderCommand::ClearAll => {
                self.files_search.results.clear();
                self.files_search.wrapped_scroll.reset(0);
                self.files_search.results_selected = 0;
                self.files_search.results_cursor_x = None;
                self.files_search.wrapped_preferred_x = None;
                self.files_search.text_anchor = None;
                self.files_search.text_head = None;
                self.files_search.results_width_item = None;
                self.files_search.results_width = px(0.);
                self.reveal_finder_row(0, gpui::ScrollStrategy::Top);
                self.files_search.results_focus.focus(window, cx);
                self.cancel_finder_drag();
                cx.notify();
            }
        }
    }

    fn finder_point_at(
        &self,
        index: usize,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
    ) -> Option<(crate::finder::TextPoint, gpui::Pixels)> {
        if self.files_search.results_wrap {
            let bounds = self.files_search.wrapped_scroll.bounds_for_item(index)?;
            let row = *self.files_search.results.rows.get(index)?;
            let caption = self.files_search.results.caption(index)?;
            let lines = self.finder_wrapped_lines(index, window)?;
            let x = position.x - bounds.left();
            let mut relative = position - bounds.origin - gpui::point(px(8.), px(0.));
            let mut offset = 0;
            for line in &lines {
                let height = line.size(px(17.)).height;
                if relative.y < height {
                    let byte = line
                        .closest_index_for_position(relative, px(17.))
                        .unwrap_or_else(|index| index);
                    let column = line.unwrapped_layout.x_for_index(byte) + px(8.);
                    return Some((
                        crate::finder::TextPoint {
                            row: Some(row),
                            byte: offset + byte,
                        },
                        column,
                    ));
                }
                offset += line.len() + 1;
                relative.y -= height;
            }
            return Some((
                crate::finder::TextPoint {
                    row: Some(row),
                    byte: caption.len(),
                },
                x,
            ));
        }
        let scroll = self.files_search.results_scroll.0.borrow();
        let x = position.x - scroll.base_handle.bounds().left() - scroll.base_handle.offset().x;
        drop(scroll);
        Some((self.finder_point_for_column(index, x, window)?, x))
    }

    fn finder_wrapped_lines(
        &self,
        index: usize,
        window: &Window,
    ) -> Option<Vec<gpui::WrappedLine>> {
        let row = *self.files_search.results.rows.get(index)?;
        let caption = self.files_search.results.caption(index)?;
        let width = self
            .files_search
            .wrapped_scroll
            .viewport_bounds()
            .size
            .width;
        let (family, size) = self.finder_font();
        let mut font = gpui::font(family);
        if matches!(
            row,
            crate::finder::Row::Search(_) | crate::finder::Row::Document(..)
        ) {
            font.weight = gpui::FontWeight::BOLD;
        }
        match window.text_system().shape_text(
            caption.to_string().into(),
            size,
            &[gpui::TextRun {
                len: caption.len(),
                font,
                color: rgb(0).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            Some((width - px(8.)).max(px(1.))),
            None,
        ) {
            Ok(lines) => Some(lines.into_vec()),
            Err(error) => {
                eprintln!("Finder wrapped text could not be shaped: {error}");
                None
            }
        }
    }

    fn move_wrapped_finder(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let caret = self.finder_caret();
        let at_end = caret.row.is_none();
        if self.files_search.results.rows.is_empty() || (at_end && down) {
            return;
        }
        let index = if at_end {
            self.files_search.results.rows.len() - 1
        } else {
            self.files_search.results_selected
        };
        if !at_end && caret.row != self.files_search.results.rows.get(index).copied() {
            let target = if down {
                (index + 1).min(self.files_search.results.rows.len())
            } else {
                index
            };
            let row = self.files_search.results.rows.get(target).copied();
            let byte = if down {
                0
            } else {
                row.and_then(|row| self.files_search.results.caption_row(row))
                    .map_or(0, |caption| caption.len())
            };
            let point = crate::finder::TextPoint { row, byte };
            if !extend || self.files_search.text_anchor.is_none() {
                self.files_search.text_anchor = Some(if extend { caret } else { point });
            }
            self.files_search.text_head = Some(point);
            self.files_search.results_selected = target;
            self.files_search.wrapped_preferred_x = None;
            self.files_search.results_cursor_x = None;
            self.reveal_wrapped_finder_caret(window);
            cx.notify();
            return;
        }
        let Some(lines) = self.finder_wrapped_lines(index, window) else {
            return;
        };
        let mut at = 0;
        let mut y = px(0.);
        let mut position = at_end.then(|| {
            gpui::point(
                px(0.),
                lines
                    .iter()
                    .map(|line| line.size(px(17.)).height)
                    .sum::<gpui::Pixels>(),
            )
        });
        for line in lines.iter().filter(|_| !at_end) {
            if caret.byte <= at + line.len() {
                position = line
                    .position_for_index(caret.byte - at, px(17.))
                    .map(|point| point + gpui::point(px(0.), y));
                break;
            }
            at += line.len() + 1;
            y += line.size(px(17.)).height;
        }
        let Some(position) = position else {
            return;
        };
        let x = self.files_search.wrapped_preferred_x.unwrap_or(position.x);
        self.files_search.wrapped_preferred_x = Some(x);
        let mut target_y = position.y + if down { px(17.) } else { -px(17.) };
        let total_height: gpui::Pixels = lines.iter().map(|line| line.size(px(17.)).height).sum();
        let target = if target_y >= px(0.) && target_y < total_height {
            index
        } else if down {
            (index + 1).min(self.files_search.results.rows.len())
        } else {
            index.saturating_sub(1)
        };
        let point = if target == self.files_search.results.rows.len() {
            crate::finder::TextPoint { row: None, byte: 0 }
        } else {
            let Some(target_lines) = self.finder_wrapped_lines(target, window) else {
                return;
            };
            if target != index {
                target_y = if down {
                    px(0.)
                } else {
                    target_lines
                        .iter()
                        .map(|line| line.size(px(17.)).height)
                        .sum::<gpui::Pixels>()
                        - px(17.)
                };
            }
            let mut offset = 0;
            let mut selected = None;
            for line in &target_lines {
                if target_y < line.size(px(17.)).height {
                    let byte = line
                        .closest_index_for_position(gpui::point(x, target_y.max(px(0.))), px(17.))
                        .unwrap_or_else(|index| index);
                    selected = Some((
                        offset + byte,
                        line.unwrapped_layout.x_for_index(byte) + px(8.),
                    ));
                    break;
                }
                target_y -= line.size(px(17.)).height;
                offset += line.len() + 1;
            }
            let Some((byte, column)) = selected else {
                return;
            };
            self.files_search.results_cursor_x = Some(column);
            crate::finder::TextPoint {
                row: self.files_search.results.rows.get(target).copied(),
                byte,
            }
        };
        if !extend || self.files_search.text_anchor.is_none() {
            self.files_search.text_anchor = Some(if extend { caret } else { point });
        }
        self.files_search.text_head = Some(point);
        self.files_search.results_selected = target;
        self.reveal_wrapped_finder_caret(window);
        cx.notify();
    }

    fn reveal_wrapped_finder_caret(&self, window: &Window) {
        let index = self.files_search.results_selected;
        if index == self.files_search.results.rows.len() {
            self.files_search.wrapped_scroll.scroll_to_end();
            return;
        }
        let bounds = self.files_search.wrapped_scroll.bounds_for_item(index);
        let Some(lines) = self.finder_wrapped_lines(index, window) else {
            return;
        };
        let mut byte = self.finder_caret().byte;
        let mut y = px(0.);
        for line in &lines {
            if byte <= line.len() {
                let Some(position) = line.position_for_index(byte, px(17.)) else {
                    return;
                };
                y += position.y;
                let viewport = self.files_search.wrapped_scroll.viewport_bounds();
                let caret_top = bounds.map(|bounds| bounds.top() + y);
                let offset = if caret_top.is_none() {
                    Some((y + px(17.) - viewport.size.height).max(px(0.)))
                } else if caret_top.is_some_and(|top| top < viewport.top()) {
                    Some(y)
                } else if caret_top.is_some_and(|top| top + px(17.) > viewport.bottom()) {
                    Some((y + px(17.) - viewport.size.height).max(px(0.)))
                } else {
                    None
                };
                if let Some(offset_in_item) = offset {
                    self.files_search
                        .wrapped_scroll
                        .scroll_to(gpui::ListOffset {
                            item_ix: index,
                            offset_in_item,
                        });
                }
                return;
            }
            byte -= line.len() + 1;
            y += line.size(px(17.)).height;
        }
    }

    fn finder_point_for_column(
        &self,
        index: usize,
        x: gpui::Pixels,
        window: &Window,
    ) -> Option<crate::finder::TextPoint> {
        if index == self.files_search.results.rows.len() {
            return Some(crate::finder::TextPoint { row: None, byte: 0 });
        }
        let row = *self.files_search.results.rows.get(index)?;
        let caption = self.files_search.results.caption(index)?;
        let caption = caption.split('\n').next().unwrap_or_default();
        let (family, size) = self.finder_font();
        let mut font = gpui::font(family);
        if matches!(
            row,
            crate::finder::Row::Search(_) | crate::finder::Row::Document(..)
        ) {
            font.weight = gpui::FontWeight::BOLD;
        }
        let shape = window.text_system().shape_line(
            caption.to_string().into(),
            size,
            &[gpui::TextRun {
                len: caption.len(),
                font,
                color: rgb(0).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        Some(crate::finder::TextPoint {
            row: Some(row),
            byte: shape.closest_index_for_x(x - px(8.)),
        })
    }

    fn select_finder_point(
        &mut self,
        index: usize,
        position: gpui::Point<gpui::Pixels>,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((point, x)) = self.finder_point_at(index, position, window) else {
            return;
        };
        if !extend || self.files_search.text_anchor.is_none() {
            self.files_search.text_anchor = Some(point);
        }
        self.files_search.text_head = Some(point);
        self.files_search.wrapped_preferred_x = None;
        self.files_search.results_selected = index;
        self.files_search.results_cursor_x = Some(x);
        self.files_search.context = None;
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn render_finder_context(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let position = self.files_search.context?;
        Some(
            gpui::deferred(
                gpui::anchored().position(position).snap_to_window().child(
                    div()
                        .id("finder-context-menu")
                        .debug_selector(|| "finder-context-menu".into())
                        .role(Role::Menu)
                        .track_focus(&self.files_search.context_focus)
                        .w(px(250.))
                        .py(px(2.))
                        .bg(rgb(if self.dark { 0x303030 } else { 0xffffff }))
                        .text_color(rgb(if self.dark { 0xe0e0e0 } else { 0x202020 }))
                        .border_1()
                        .border_color(rgb(0x808080))
                        .occlude()
                        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                            view.files_search.context = None;
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                            match event.keystroke.key.as_str() {
                                "up" | "down" => {
                                    let length = FinderCommand::ALL.len();
                                    for _ in 0..length {
                                        view.files_search.context_selected =
                                            if event.keystroke.key == "up" {
                                                (view.files_search.context_selected + length - 1)
                                                    % length
                                            } else {
                                                (view.files_search.context_selected + 1) % length
                                            };
                                        if view.finder_command_enabled(
                                            FinderCommand::ALL[view.files_search.context_selected],
                                        ) {
                                            break;
                                        }
                                    }
                                    cx.notify();
                                }
                                "enter" => {
                                    let command =
                                        FinderCommand::ALL[view.files_search.context_selected];
                                    if view.finder_command_enabled(command) {
                                        view.run_finder_command(command, window, cx);
                                    }
                                }
                                "escape" => {
                                    view.files_search.context = None;
                                    view.files_search.results_focus.focus(window, cx);
                                    cx.notify();
                                }
                                _ => return,
                            }
                            cx.stop_propagation();
                        }))
                        .children(
                            FinderCommand::ALL
                                .into_iter()
                                .enumerate()
                                .filter(|(_, command)| {
                                    self.files_search.views.active != 0
                                        || !matches!(command, FinderCommand::CloseResults)
                                })
                                .map(|(index, command)| {
                                    let selector = command.selector();
                                    let enabled = self.finder_command_enabled(command);
                                    let checked = match command {
                                        FinderCommand::TogglePurge => {
                                            Some(self.files_search.results_purge)
                                        }
                                        FinderCommand::ToggleWrap => {
                                            Some(self.files_search.results_wrap)
                                        }
                                        _ => None,
                                    };
                                    div()
                                        .id(selector)
                                        .debug_selector(move || selector.into())
                                        .role(Role::MenuItem)
                                        .relative()
                                        .aria_label(command.label())
                                        .when_some(checked, |item, checked| {
                                            item.aria_toggled(checked.into())
                                        })
                                        .h(px(24.))
                                        .px(px(20.))
                                        .child(command.label())
                                        .when(checked == Some(true), |item| {
                                            item.child(
                                                div()
                                                    .absolute()
                                                    .left(px(4.))
                                                    .top(px(0.))
                                                    .w(px(12.))
                                                    .h(px(24.))
                                                    .child("✓"),
                                            )
                                        })
                                        .when(!enabled, |item| item.text_color(rgb(0x808080)))
                                        .a11y_synthetic_children(move |builder| {
                                            if !enabled {
                                                builder.parent_node().set_disabled();
                                            }
                                        })
                                        .when(self.files_search.context_selected == index, |item| {
                                            item.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                                        })
                                        .on_mouse_move(cx.listener(move |view, _, _, cx| {
                                            if enabled {
                                                view.files_search.context_selected = index;
                                                cx.notify();
                                            }
                                        }))
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            if enabled {
                                                view.run_finder_command(command, window, cx);
                                            }
                                            cx.stop_propagation();
                                        }))
                                }),
                        ),
                ),
            )
            .priority(2)
            .into_any_element(),
        )
    }

    pub(super) fn render_file_results(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.files_search.results_visible {
            return None;
        }
        let dark = self.dark;
        Some(
            div()
                .id("files-results")
                .debug_selector(|| "files-results".into())
                .role(Role::Group)
                .aria_label("Search results")
                .track_focus(&self.files_search.results_focus)
                .on_mouse_down_out(cx.listener(|view, _, _, _| view.cancel_finder_drag()))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.cancel_finder_drag()),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.cancel_finder_drag()),
                )
                .key_context("MatrixFindResults")
                .h(px(156.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(rgb(0x808080))
                .bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
                .text_color(rgb(if self.dark { 0xe0e0e0 } else { 0x000000 }))
                .on_action(cx.listener(|view, _: &EditorSelectAll, window, cx| {
                    view.select_all_finder(window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &EditorCopy, window, cx| {
                    view.copy_finder(FinderCopy::Verbatim, window, cx);
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" => view.activate_finder_caret(window, cx),
                        "escape" => {
                            view.cancel_finder_drag();
                            view.files_search.results_visible = false;
                            view.editor_focus.focus(window, cx);
                            cx.notify();
                        }
                        "delete" => view.delete_finder_row(window, cx),
                        "up" | "down" => {
                            if view.files_search.results_wrap {
                                view.move_wrapped_finder(
                                    event.keystroke.key == "down",
                                    event.keystroke.modifiers.shift,
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                                return;
                            }
                            let last = view.files_search.results.rows.len();
                            let next = if event.keystroke.key == "up" {
                                view.files_search.results_selected.saturating_sub(1)
                            } else {
                                (view.files_search.results_selected + 1).min(last)
                            };
                            let x = view.files_search.results_cursor_x.unwrap_or(px(8.));
                            if let Some(point) = view.finder_point_for_column(next, x, window) {
                                if view.files_search.text_anchor.is_none() {
                                    view.files_search.text_anchor =
                                        view.files_search.text_head.or_else(|| {
                                            view.finder_point_for_column(
                                                view.files_search.results_selected,
                                                x,
                                                window,
                                            )
                                        });
                                }
                                if !event.keystroke.modifiers.shift {
                                    view.files_search.text_anchor = Some(point);
                                }
                                view.files_search.text_head = Some(point);
                            }
                            view.files_search.results_selected = next;
                            view.reveal_finder_row(
                                view.files_search.results_selected,
                                gpui::ScrollStrategy::Nearest,
                            );
                            cx.notify();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .h(px(22.))
                        .flex()
                        .justify_between()
                        .px(px(4.))
                        .bg(rgb(if self.dark { 0x303030 } else { 0xf0f0f0 }))
                        .child(
                            div()
                                .id("files-results-summary")
                                .role(Role::Status)
                                .aria_label(self.files_search.message.clone())
                                .child(self.files_search.message.clone()),
                        )
                        .when(
                            self.files_search.cancel.is_some()
                                && !self.find_in_results_search_active(),
                            |row| {
                                row.child(
                                    div()
                                        .id("files-cancel")
                                        .debug_selector(|| "files-cancel".into())
                                        .role(Role::Button)
                                        .aria_label("Cancel file search")
                                        .child("Cancel")
                                        .on_click(cx.listener(|view, _, _, _| {
                                            if let Some(cancel) = &view.files_search.cancel {
                                                cancel.store(true, Ordering::Relaxed);
                                            }
                                        })),
                                )
                            },
                        )
                        .child(
                            div()
                                .id("files-results-close")
                                .debug_selector(|| "files-results-close".into())
                                .role(Role::Button)
                                .aria_label("Close search results")
                                .tooltip(move |_, cx| {
                                    icon_tooltip::view("Close search results", dark, cx)
                                })
                                .child("×")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_active_finder(window, cx);
                                })),
                        ),
                )
                .children(self.render_finder_tabs(cx))
                .child(if self.files_search.results_wrap {
                    if self.files_search.wrapped_scroll.item_count()
                        != self.files_search.results.rows.len()
                    {
                        self.files_search
                            .wrapped_scroll
                            .reset(self.files_search.results.rows.len());
                    }
                    gpui::list(
                        self.files_search.wrapped_scroll.clone(),
                        cx.processor(|view, index, _, cx| view.render_finder_row(index, cx)),
                    )
                    .flex_1()
                    .min_h(px(0.))
                    .into_any_element()
                } else {
                    gpui::uniform_list(
                        "files-result-rows",
                        self.files_search.results.rows.len(),
                        cx.processor(|view, range: std::ops::Range<usize>, window, cx| {
                            view.measure_finder_width(range.clone(), window, cx);
                            range
                                .map(|index| view.render_finder_row(index, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&self.files_search.results_scroll)
                    .with_width_from_item(self.files_search.results_width_item)
                    .with_horizontal_sizing_behavior(
                        gpui::ListHorizontalSizingBehavior::Unconstrained,
                    )
                    .flex_1()
                    .min_h(px(0.))
                    .into_any_element()
                })
                .children(self.render_finder_context(cx))
                .into_any_element(),
        )
    }

    fn render_finder_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        use crate::finder::Row;
        let (family, font_size) = self.finder_font();
        let row = self.files_search.results.rows[index];
        let caption = self
            .files_search
            .results
            .caption(index)
            .unwrap_or_default()
            .into_owned();
        let (selector, style, collapsed, highlights) = match row {
            Row::Search(search) => {
                let search = &self.files_search.results.searches[search];
                (
                    format!("finder-search-{index}"),
                    1,
                    Some(search.collapsed),
                    Vec::new(),
                )
            }
            Row::Document(search, doc) => {
                let found = &self.files_search.results.searches[search].documents[doc];
                (
                    format!("finder-document-{search}-{doc}"),
                    2,
                    Some(found.collapsed),
                    Vec::new(),
                )
            }
            Row::Line(search, doc, line) => {
                let found = &self.files_search.results.searches[search].documents[doc].lines[line];
                (
                    format!("finder-hit-{search}-{doc}-{line}"),
                    0,
                    None,
                    found.highlights.clone(),
                )
            }
            Row::Error(search, error) => (
                format!("finder-error-{search}-{error}"),
                0,
                None,
                Vec::new(),
            ),
        };
        let colour = |id, attribute: &str, fallback| {
            crate::languages::catalog()
                .ok()
                .and_then(|catalog| catalog.themes().theme(self.dark).lexers.get("searchResult"))
                .and_then(|lexer| lexer.styles.iter().find(|style| style.id == id))
                .and_then(|style| style.attributes.get(attribute))
                .and_then(|value| u32::from_str_radix(value, 16).ok())
                .unwrap_or(fallback)
        };
        let background = if index == self.files_search.results_selected {
            colour(6, "bgColor", 0xe8e8ff)
        } else {
            colour(
                style,
                "bgColor",
                if self.dark { 0x202020 } else { 0xffffff },
            )
        };
        let mark = gpui::HighlightStyle {
            color: Some(rgb(colour(4, "fgColor", 0xff0000)).into()),
            background_color: Some(rgb(colour(4, "bgColor", 0xffffbf)).into()),
            ..Default::default()
        };
        let mut styles = Vec::new();
        if matches!(row, Row::Line(..))
            && let Some(start) = caption.find(|ch: char| ch.is_ascii_digit())
            && let Some(end) = caption.find(':')
        {
            styles.push((
                start..end,
                gpui::HighlightStyle {
                    color: Some(rgb(colour(3, "fgColor", 0xff8040)).into()),
                    ..Default::default()
                },
            ));
        }
        styles.extend(highlights.into_iter().filter_map(|range| {
            let end = range.end.min(caption.len());
            (range.start < end).then_some((range.start..end, mark))
        }));
        if let Some(selected) = self
            .files_search
            .text_anchor
            .zip(self.files_search.text_head)
            .and_then(|(anchor, head)| self.files_search.results.selected_bytes(row, anchor, head))
        {
            let mut boundaries = vec![0, caption.len(), selected.start, selected.end];
            for (range, _) in &styles {
                boundaries.extend([range.start, range.end]);
            }
            boundaries.sort_unstable();
            boundaries.dedup();
            let selection_style = gpui::HighlightStyle {
                color: Some(rgb(0xffffff).into()),
                background_color: Some(rgb(0x0078d4).into()),
                ..Default::default()
            };
            styles = boundaries
                .windows(2)
                .filter_map(|pair| {
                    let style = if selected.contains(&pair[0]) {
                        Some(selection_style)
                    } else {
                        styles
                            .iter()
                            .find(|(range, _)| range.contains(&pair[0]))
                            .map(|(_, style)| *style)
                    }?;
                    Some((pair[0]..pair[1], style))
                })
                .collect();
        }
        let text = gpui::StyledText::new(gpui::SharedString::from(caption.clone()))
            .with_highlights(styles);
        div()
            .id(("finder-row", index))
            .debug_selector(move || selector)
            .role(Role::Group)
            .aria_label(caption)
            .relative()
            .when(!self.files_search.results_wrap, |row| {
                row.h(px(17.)).whitespace_nowrap()
            })
            .when(self.files_search.results_wrap, |row| {
                row.w_full().min_h(px(17.))
            })
            .pl(px(8.))
            .overflow_hidden()
            .font_family(family)
            .text_size(font_size)
            .line_height(px(17.))
            .bg(rgb(background))
            .text_color(rgb(colour(
                style,
                "fgColor",
                if self.dark { 0xe0e0e0 } else { 0x000000 },
            )))
            .when(style == 1 || style == 2, |row| {
                row.font_weight(gpui::FontWeight::BOLD)
            })
            .when_some(collapsed, |row, collapsed| {
                row.child(
                    div()
                        .id(("finder-fold", index))
                        .role(Role::Button)
                        .aria_label(if collapsed {
                            "Expand results"
                        } else {
                            "Collapse results"
                        })
                        .absolute()
                        .left(px(0.))
                        .top(px(0.))
                        .w(px(7.))
                        .h(px(17.))
                        .child(if collapsed { "+" } else { "−" })
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.toggle_finder_row(index, window, cx);
                            cx.stop_propagation();
                        })),
                )
            })
            .child(text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.select_finder_point(
                        index,
                        event.position,
                        event.modifiers.shift,
                        window,
                        cx,
                    );
                    view.files_search.dragging_text = true;
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(move |view, event: &gpui::MouseMoveEvent, window, cx| {
                    if event.pressed_button == Some(MouseButton::Left)
                        && view.files_search.dragging_text
                    {
                        view.select_finder_point(index, event.position, true, window, cx);
                        cx.stop_propagation();
                    } else if event.pressed_button.is_none() {
                        view.cancel_finder_drag();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.cancel_finder_drag();
                    let inside = view
                        .finder_point_at(index, event.position, window)
                        .is_some_and(|(point, _)| {
                            view.files_search
                                .text_anchor
                                .zip(view.files_search.text_head)
                                .is_some_and(|(anchor, head)| point.between(anchor, head))
                        });
                    if !inside {
                        view.select_finder_point(index, event.position, false, window, cx);
                    }
                    view.files_search.context = Some(event.position);
                    view.files_search.context_selected = 0;
                    view.files_search.context_focus.focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .on_click(
                cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                    view.files_search.results_selected = index;
                    let Some((_, x)) = view.finder_point_at(index, event.position(), window) else {
                        return;
                    };
                    view.files_search.results_cursor_x = Some(x);
                    view.files_search.results_focus.focus(window, cx);
                    if event.click_count() == 2 {
                        view.activate_finder_row(index, Some(x), window, cx);
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}
