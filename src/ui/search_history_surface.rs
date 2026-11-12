use super::*;
use crate::search_history::{Field, History};

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn find_and_replace_commands_capture_source_maximum_utf16_lengths(
        cx: &mut gpui::TestAppContext,
    ) {
        let query = "q".repeat(16384);
        let replacement = "r".repeat(16384);
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&query[..16383]);
            view.show_replace(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text(&query, window, cx));
            view.replace_input
                .update(cx, |field, cx| field.set_text(&replacement, window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), replacement[..16383]);
            assert_eq!(
                view.search_history.histories[Field::Find.index()].entries()[0].len(),
                16383
            );
            assert_eq!(
                view.search_history.histories[Field::Replace.index()].entries()[0].len(),
                16383
            );
        });
        dialog.dispatch_action(CloseWindow);
        cx.run_until_parked();
        cx.dispatch_action(ShowReplace);
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), query[..16383]);
            assert_eq!(view.replace_input.read(cx).text(), replacement[..16383]);
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), query[..16383])
        });
    }

    #[gpui::test]
    fn file_history_commands_capture_source_filter_and_directory_buffer_lengths(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.load_settings().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find_in_files(window, cx);
            for (field, text) in [
                (Field::Find, String::new()),
                (Field::Filter, "f".repeat(512)),
                (Field::Directory, "d".repeat(260)),
            ] {
                view.search_history_input(field)
                    .update(cx, |input, cx| input.set_text(&text, window, cx));
            }
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(
            saved["FindHistory"]["filters"],
            serde_json::json!(["f".repeat(511)])
        );
        assert_eq!(
            saved["FindHistory"]["paths"],
            serde_json::json!(["d".repeat(259)])
        );
        view.read_with(cx, |view, cx| {
            assert_eq!(
                view.search_history_input(Field::Directory).read(cx).text(),
                "d".repeat(259)
            );
            assert_eq!(
                view.search_history_input(Field::Filter).read(cx).text(),
                "f".repeat(511)
            );
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn replacement_history_prefix_failure_is_atomic_and_remains_reported_after_the_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] =
                    serde_json::json!({"finds": {"future": true}, "replaces": ["old"]});
                Ok(())
            })
            .unwrap();
        let path = store.root().join("settings.json");
        let before = std::fs::read(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("needle");
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_replace(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("needle", window, cx));
            view.replace_input
                .update(cx, |field, cx| field.set_text("new", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let replace = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(replace.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), before);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "new");
            assert!(
                view.editor_error
                    .as_deref()
                    .is_some_and(|error| error.contains("Search history could not be saved"))
            );
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "needle")
        });
    }

    #[gpui::test]
    fn single_replace_records_its_source_prefix_before_empty_and_readonly_processing(
        cx: &mut gpui::TestAppContext,
    ) {
        for (query, readonly) in [("", false), ("needle", true)] {
            let home = tempfile::tempdir().unwrap();
            let store = ConfigStore::under_home(home.path());
            let settings = store
                .update_settings(|settings| {
                    settings["FindHistory"] =
                        serde_json::json!({"finds": ["old find"], "replaces": ["old replacement"]});
                    Ok(())
                })
                .unwrap();
            let (view, context) =
                cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
            view.update_in(context, |view, window, cx| {
                *view.editor_mut() = EditorSession::new("needle");
                view.editor_mut().set_read_only(readonly);
                view.settings_store = Some(store);
                view.configure_find(&settings);
                view.show_replace(window, cx);
                view.find_query
                    .update(cx, |field, cx| field.set_text(query, window, cx));
                view.replace_input
                    .update(cx, |field, cx| field.set_text("new", window, cx));
            });
            context.run_until_parked();
            let handle = view.read_with(context, |view, _| {
                view.floating_windows[&floating_windows::DialogKind::Find]
            });
            let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &context.cx);
            let replace = dialog.debug_bounds("replace-one").unwrap();
            dialog.simulate_click(replace.center(), gpui::Modifiers::default());
            context.run_until_parked();
            let saved = ConfigStore::under_home(home.path())
                .load_settings()
                .unwrap();
            assert_eq!(
                saved["FindHistory"]["replaces"],
                serde_json::json!(["new", "old replacement"])
            );
            assert_eq!(
                saved["FindHistory"]["finds"],
                if query.is_empty() {
                    serde_json::json!(["old find"])
                } else {
                    serde_json::json!(["needle", "old find"])
                }
            );
            view.read_with(context, |view, _| {
                assert_eq!(view.editor().committed_text(), "needle");
                assert!(!view.editor().document().can_undo());
                assert_eq!(
                    view.find_message,
                    if readonly {
                        "Replace: Cannot replace text. The current document is read only."
                    } else {
                        ""
                    }
                );
            });
        }
    }

    #[gpui::test]
    fn empty_find_text_still_records_file_filter_and_focuses_empty_directory(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.update_settings(|settings| {
            settings["FindHistory"] = serde_json::json!({"finds": ["old find"], "filters": ["*.old"], "paths": ["/old"]});
            Ok(())
        }).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find_in_files(window, cx);
            for (field, text) in [
                (Field::Find, ""),
                (Field::Filter, "*.new"),
                (Field::Directory, ""),
            ] {
                view.search_history_input(field)
                    .update(cx, |input, cx| input.set_text(text, window, cx));
            }
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(
            saved["FindHistory"]["filters"],
            serde_json::json!(["*.new", "*.old"])
        );
        assert_eq!(
            saved["FindHistory"]["finds"],
            serde_json::json!(["old find"])
        );
        assert_eq!(saved["FindHistory"]["paths"], serde_json::json!(["/old"]));
        dialog.simulate_input("target");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "");
            assert_eq!(
                view.search_history_input(Field::Directory).read(cx).text(),
                "target"
            );
        });
    }

    #[gpui::test]
    fn snapshot_window_exit_saves_history_without_prompting_or_losing_raw_backup(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] = serde_json::json!({"replaces": ["old replacement"]});
                Ok(())
            })
            .unwrap();
        let (recovery, workspace) = crate::recovery::RecoveryManager::open(home.path()).unwrap();
        assert!(recovery.snapshot_mode());
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.workspace = workspace;
            view.recovery = Some(recovery);
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.reset_active_view(window, cx);
            view.editor_mut().insert_text("unsaved").unwrap();
            view.show_replace(window, cx);
            view.replace_input
                .update(cx, |input, cx| input.set_text("", window, cx));
        });
        cx.run_until_parked();
        cx.dispatch_action(CloseWindow);
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(
            saved["FindHistory"]["replaces"],
            serde_json::json!(["", "old replacement"])
        );
        let raw_backup = std::fs::read_dir(home.path().join(".notepad-matrix/backup"))
            .unwrap()
            .any(|entry| {
                std::fs::read(entry.unwrap().path()).is_ok_and(|bytes| bytes == b"unsaved")
            });
        assert!(raw_backup);
        view.read_with(cx, |view, _| {
            assert!(!view.close_dialog);
            assert_eq!(view.editor().committed_text(), "unsaved");
        });
    }

    #[gpui::test]
    fn empty_file_search_directory_records_only_find_and_filter_then_focuses_directory(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.update_settings(|settings| {
            settings["FindHistory"] = serde_json::json!({"finds": ["old find"], "filters": ["*.old"], "paths": ["/old"]});
            settings["Searching"] = serde_json::json!({"fillFindFieldWithSelected": false});
            Ok(())
        }).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find_in_files(window, cx);
            for (field, text) in [
                (Field::Find, "new find"),
                (Field::Filter, "*.new"),
                (Field::Directory, ""),
            ] {
                view.search_history_input(field)
                    .update(cx, |input, cx| input.set_text(text, window, cx));
            }
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let search = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(search.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        dialog.simulate_input("target");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "new find");
            assert_eq!(
                view.search_history_input(Field::Directory).read(cx).text(),
                "target"
            );
        });
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(
            saved["FindHistory"]["finds"],
            serde_json::json!(["new find", "old find"])
        );
        assert_eq!(
            saved["FindHistory"]["filters"],
            serde_json::json!(["*.new", "*.old"])
        );
        assert_eq!(saved["FindHistory"]["paths"], serde_json::json!(["/old"]));
    }

    #[gpui::test]
    fn executing_find_while_history_is_open_keeps_the_recorded_combo_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("second");
            view.configure_find(&serde_json::json!({"FindHistory": {"finds": ["first", "second"]}, "Searching": {"fillFindFieldWithSelected": false}}));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        dialog.simulate_keystrokes("alt-down down f3 enter");
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "second");
            assert_eq!(
                view.search_history.histories[Field::Find.index()].entries(),
                ["second", "first"]
            );
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(0)..ByteOffset(6)
            );
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn find_all_hiding_the_window_does_not_restore_an_old_open_history_popup(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("first");
            view.configure_find(&serde_json::json!({"FindHistory": {"finds": ["first", "second"]}, "Searching": {"fillFindFieldWithSelected": false}}));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        dialog.simulate_keystrokes("alt-down");
        assert!(dialog.debug_bounds("search-history-popup").is_some());
        view.update_in(cx, |view, window, cx| {
            view.find_all_documents(false, window, cx)
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(!view.find_visible));
        cx.dispatch_action(ShowFind);
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut reopened = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        assert!(reopened.debug_bounds("search-history-popup").is_none());
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "first");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn never_opened_search_dialog_does_not_rewrite_saved_histories_on_close(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] =
                    serde_json::json!({"finds": ["old"], "replaces": ["old replacement"]});
                Ok(())
            })
            .unwrap();
        let path = store.root().join("settings.json");
        let before = std::fs::read(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update(cx, |view, _| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
        });
        cx.dispatch_action(CloseWindow);
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[gpui::test]
    fn history_popup_retires_on_deactivation_and_programmatic_search_page_changes(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.configure_find(
                &serde_json::json!({"FindHistory": {"finds": ["first", "second"]}}),
            );
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let arrow = dialog.debug_bounds("search-history-button-finds").unwrap();
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        assert!(dialog.debug_bounds("search-history-popup").is_some());
        dialog.deactivate_window();
        cx.run_until_parked();
        assert!(dialog.debug_bounds("search-history-popup").is_none());
        dialog.update(|window, _| window.activate_window());
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        dialog.dispatch_action(ShowReplace);
        cx.run_until_parked();
        assert!(dialog.debug_bounds("search-history-popup").is_none());
        assert!(dialog.debug_bounds("replace-input").is_some());
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        dialog.dispatch_action(ShowFindInFiles);
        cx.run_until_parked();
        assert!(dialog.debug_bounds("search-history-popup").is_none());
        assert!(dialog.debug_bounds("find-in-files-dialog").is_some());
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        dialog.dispatch_action(CloseWindow);
        cx.run_until_parked();
        cx.dispatch_action(ShowFind);
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut reopened = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        assert!(reopened.debug_bounds("search-history-popup").is_none());
        view.read_with(cx, |view, _| {
            assert_eq!(view.floating_windows.len(), 1);
            assert!(!view.editor().document().can_undo());
            assert_eq!(
                view.search_history.histories[Field::Find.index()].entries(),
                ["first", "second"]
            );
        });
    }

    #[gpui::test]
    fn closing_document_window_saves_empty_combo_edits_without_recording_unused_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.update_settings(|settings| {
            settings["FindHistory"] = serde_json::json!({"finds": ["old query"], "replaces": ["old replacement"], "filters": ["*.rs"], "paths": ["/old"], "future": 42});
            settings["Searching"] = serde_json::json!({"fillFindFieldWithSelected": false});
            Ok(())
        }).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_replace(window, cx);
            for (field, text) in [
                (Field::Find, "never executed"),
                (Field::Replace, ""),
                (Field::Filter, ""),
            ] {
                view.search_history_input(field)
                    .update(cx, |input, cx| input.set_text(text, window, cx));
            }
        });
        cx.run_until_parked();
        cx.dispatch_action(CloseWindow);
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(
            saved["FindHistory"]["finds"],
            serde_json::json!(["old query"])
        );
        assert_eq!(
            saved["FindHistory"]["replaces"],
            serde_json::json!(["", "old replacement"])
        );
        assert_eq!(
            saved["FindHistory"]["filters"],
            serde_json::json!(["", "*.rs"])
        );
        assert_eq!(saved["FindHistory"]["paths"], serde_json::json!(["/old"]));
        assert_eq!(saved["FindHistory"]["future"], 42);
    }

    #[gpui::test]
    fn history_keyboard_open_cancel_tab_and_scrolling_use_the_editable_combo(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.configure_find(&serde_json::json!({"FindHistory": {"finds": (0..25).map(|index| format!("entry{index:02}")).collect::<Vec<_>>(), "replaces": ["old", "new"]}, "Searching": {"fillFindFieldWithSelected": false}}));
            view.show_replace(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        dialog.simulate_keystrokes("alt-down");
        assert!(dialog.debug_bounds("search-history-popup").is_some());
        dialog.simulate_keystrokes(
            &std::iter::repeat_n("down", 15)
                .collect::<Vec<_>>()
                .join(" "),
        );
        for _ in 0..2 {
            dialog.update(|window, cx| window.draw(cx).clear(cx));
        }
        let popup = dialog.debug_bounds("search-history-popup").unwrap();
        let selected = dialog.debug_bounds("search-history-item-finds-15").unwrap();
        assert!(selected.top() >= popup.top() && selected.bottom() <= popup.bottom());
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "entry15")
        });
        dialog.simulate_keystrokes("escape");
        assert!(dialog.debug_bounds("search-history-popup").is_none());
        assert!(dialog.debug_bounds("find-dialog").is_some());
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "entry00")
        });
        dialog.simulate_keystrokes("alt-down down tab");
        assert!(dialog.debug_bounds("search-history-popup").is_none());
        dialog.simulate_input("replacement");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "entry01");
            assert_eq!(view.replace_input.read(cx).text(), "replacement");
        });
        dialog.simulate_keystrokes("alt-down shift-tab");
        dialog.simulate_input("search");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "search");
            assert_eq!(view.replace_input.read(cx).text(), "replacement");
            assert_eq!(view.editor().committed_text(), "unchanged");
            assert!(!view.editor().document().can_undo());
            assert_eq!(view.find_message, "");
        });
        dialog.simulate_keystrokes("alt-down");
        dialog.simulate_input("draft");
        dialog.simulate_keystrokes("enter");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "draft")
        });
        assert!(dialog.debug_bounds("search-history-popup").is_none());
    }

    #[gpui::test]
    fn reopening_history_keeps_the_current_entry_and_empty_lists_keep_typing_focus(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.configure_find(
                &serde_json::json!({"FindHistory": {"finds": ["first", "second"]}}),
            );
            view.show_replace(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("second", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let arrow = dialog.debug_bounds("search-history-button-finds").unwrap();
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        dialog.simulate_keystrokes("enter");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "second")
        });
        let empty = dialog
            .debug_bounds("search-history-button-replaces")
            .unwrap();
        dialog.simulate_click(empty.center(), gpui::Modifiers::default());
        dialog.simulate_input("replacement");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.replace_input.read(cx).text(), "replacement")
        });
        assert!(dialog.debug_bounds("search-history-popup").is_none());
    }

    #[gpui::test]
    fn history_keyboard_accept_returns_typing_to_the_input_without_executing_search(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(find_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("unchanged");
            view.configure_find(&serde_json::json!({"FindHistory": {"finds": ["first", "second"]}, "Searching": {"fillFindFieldWithSelected": false}}));
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let arrow = dialog.debug_bounds("search-history-button-finds").unwrap();
        dialog.simulate_click(arrow.center(), gpui::Modifiers::default());
        assert!(dialog.debug_bounds("search-history-popup").is_some());
        dialog.simulate_keystrokes("down");
        view.read_with(cx, |view, _| {
            assert_eq!(view.search_history.selected, Some(1))
        });
        dialog.simulate_keystrokes("enter");
        dialog.simulate_input("typed");
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(view.find_query.read(cx).text(), "typed");
            assert_eq!(view.find_message, "");
            assert_eq!(view.editor().committed_text(), "unchanged");
            assert_eq!(
                view.search_history.histories[Field::Find.index()].entries(),
                ["first", "second"]
            );
            assert!(!view.editor().document().can_undo());
        });
        assert!(dialog.debug_bounds("search-history-popup").is_none());
    }

    #[gpui::test]
    fn stale_window_history_conflict_preserves_the_other_windows_file(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] = serde_json::json!({"finds": ["old"]});
                Ok(())
            })
            .unwrap();
        let path = store.root().join("settings.json");
        let mut second_context = cx.clone();
        let (first, first_cx) =
            cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let (second, second_cx) =
            second_context.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        for (view, context, query) in [
            (&first, &mut *first_cx, "alpha"),
            (&second, &mut *second_cx, "beta"),
        ] {
            view.update_in(context, |view, window, cx| {
                *view.editor_mut() = EditorSession::new("unchanged");
                view.settings_store = Some(ConfigStore::under_home(home.path()));
                view.configure_find(&settings);
                view.show_find(window, cx);
                view.find_query
                    .update(cx, |field, cx| field.set_text(query, window, cx));
            });
            context.run_until_parked();
        }
        let first_handle = first.read_with(&first_cx.cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let second_handle = second.read_with(&second_cx.cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut first_dialog =
            gpui::VisualTestContext::from_window(first_handle.into(), &first_cx.cx);
        let mut second_dialog =
            gpui::VisualTestContext::from_window(second_handle.into(), &second_cx.cx);
        let first_count = first_dialog.debug_bounds("find-count").unwrap();
        first_dialog.simulate_click(first_count.center(), gpui::Modifiers::default());
        first_dialog.run_until_parked();
        let before = std::fs::read(&path).unwrap();
        let second_count = second_dialog.debug_bounds("find-count").unwrap();
        second_dialog.simulate_click(second_count.center(), gpui::Modifiers::default());
        second_dialog.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), before);
        second.read_with(&second_cx.cx, |view, _| {
            assert_eq!(
                view.search_history.histories[Field::Find.index()].entries(),
                ["beta", "old"]
            );
            assert!(
                view.editor_error
                    .as_deref()
                    .is_some_and(|error| error.contains("changed by another"))
            );
            assert_eq!(view.editor().committed_text(), "unchanged");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn sequential_history_writes_keep_overlength_live_entries_in_source_slots(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] =
                    serde_json::json!({"nbMaxFindHistoryFind": 2, "finds": ["older"]});
                Ok(())
            })
            .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let count = dialog.debug_bounds("find-count").unwrap();
        for (text, expected) in [
            ("x".repeat(2048), serde_json::json!(["older"])),
            ("short".into(), serde_json::json!(["short"])),
        ] {
            view.update_in(cx, |view, window, cx| {
                view.find_query
                    .update(cx, |field, cx| field.set_text(&text, window, cx))
            });
            dialog.simulate_click(count.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            let saved = ConfigStore::under_home(home.path())
                .load_settings()
                .unwrap();
            assert_eq!(saved["FindHistory"]["finds"], expected);
        }
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.search_history.histories[Field::Find.index()]
                    .entries()
                    .len(),
                3
            )
        });
    }

    #[gpui::test]
    fn readonly_replace_all_does_not_record_rejected_history(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] =
                    serde_json::json!({"finds": ["old"], "replaces": ["old replacement"]});
                Ok(())
            })
            .unwrap();
        let path = store.root().join("settings.json");
        let before = std::fs::read(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("needle");
            view.editor_mut().set_read_only(true);
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_replace(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("needle", window, cx));
            view.replace_input
                .update(cx, |field, cx| field.set_text("new", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), before);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.find_message,
                "Replace: Cannot replace text. The current document is read only."
            );
            assert_eq!(view.editor().committed_text(), "needle");
            assert!(!view.editor().document().can_undo());
            assert_eq!(
                view.search_history.histories[Field::Find.index()].entries(),
                ["old"]
            );
            assert_eq!(
                view.search_history.histories[Field::Replace.index()].entries(),
                ["old replacement"]
            );
        });
    }

    #[gpui::test]
    fn all_history_fields_round_trip_from_real_search_and_replace_commands(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("data");
        std::fs::create_dir(&data).unwrap();
        std::fs::write(data.join("hit.txt"), "needle").unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store.update_settings(|settings| {
            settings["FindHistory"] = serde_json::json!({"finds": ["needle"], "replaces": ["old"], "filters": ["*.rs"], "paths": ["/old"], "future": {"keep": 42}});
            settings["Searching"] = serde_json::json!({"fillFindFieldWithSelected": false, "findDlgAlwaysVisible": true});
            Ok(())
        }).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("needle");
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_replace(window, cx);
            assert_eq!(view.find_query.read(cx).text(), "needle");
            assert_eq!(view.replace_input.read(cx).text(), "old");
            view.replace_input
                .update(cx, |field, cx| field.set_text("new", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let all = dialog.debug_bounds("replace-all").unwrap();
        dialog.simulate_click(all.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            view.show_find_in_files(window, cx);
            for (field, text) in [
                (Field::Find, "needle".to_owned()),
                (Field::Filter, "*.txt".to_owned()),
                (Field::Directory, format!("  {}  ", data.display())),
            ] {
                view.search_history_input(field)
                    .update(cx, |input, cx| input.set_text(&text, window, cx));
            }
        });
        cx.run_until_parked();
        let files = dialog.debug_bounds("files-find-all").unwrap();
        dialog.simulate_click(files.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .unwrap();
        assert_eq!(saved["FindHistory"]["finds"], serde_json::json!(["needle"]));
        assert_eq!(
            saved["FindHistory"]["replaces"],
            serde_json::json!(["new", "old"])
        );
        assert_eq!(
            saved["FindHistory"]["filters"],
            serde_json::json!(["*.txt", "*.rs"])
        );
        assert_eq!(
            saved["FindHistory"]["paths"],
            serde_json::json!([data.display().to_string(), "/old".to_owned()])
        );
        assert_eq!(saved["FindHistory"]["future"]["keep"], 42);
        view.read_with(cx, |view, cx| {
            assert_eq!(view.editor().committed_text(), "new");
            assert_eq!(
                view.search_history_input(Field::Directory).read(cx).text(),
                data.display().to_string()
            );
        });
        let loaded =
            Field::ALL.map(|field| History::load(field, &saved["FindHistory"], equivalent));
        assert_eq!(loaded[0].initial_text(), "needle");
        assert_eq!(loaded[1].initial_text(), "new");
        assert_eq!(loaded[2].initial_text(), "*.txt");
        assert_eq!(loaded[3].initial_text(), data.display().to_string());
        let mut unicode = History::load(
            Field::Find,
            &serde_json::json!({"finds": ["École", "éCOLE"]}),
            equivalent,
        );
        assert_eq!(unicode.entries(), ["École"]);
        unicode.record("école", equivalent);
        assert_eq!(unicode.entries(), ["école"]);
    }

    #[gpui::test]
    fn malformed_history_entries_are_not_overwritten_by_search_execution(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::under_home(home.path());
        let settings = store
            .update_settings(|settings| {
                settings["FindHistory"] = serde_json::json!({"finds": {"future": true}});
                Ok(())
            })
            .unwrap();
        let path = store.root().join("settings.json");
        let before = std::fs::read(&path).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(store);
            view.configure_find(&settings);
            view.show_find(window, cx);
            view.find_query
                .update(cx, |field, cx| field.set_text("needle", window, cx));
        });
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Find]
        });
        let mut dialog = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        let count = dialog.debug_bounds("find-count").unwrap();
        dialog.simulate_click(count.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

fn equivalent(a: &str, b: &str) -> bool {
    use objc2_foundation::{NSComparisonResult, NSString};
    NSString::from_str(a).localizedCaseInsensitiveCompare(&NSString::from_str(b))
        == NSComparisonResult::Same
}

pub(super) struct SearchHistoryState {
    histories: [History; 4],
    persisted: [serde_json::Value; 4],
    initialized: bool,
    open: Option<Field>,
    selected: Option<usize>,
    original: String,
    preview: String,
    scroll: gpui::ScrollHandle,
}

impl SearchHistoryState {
    pub(super) fn new() -> Self {
        Self {
            histories: Field::ALL
                .map(|field| History::load(field, &serde_json::Value::Null, equivalent)),
            persisted: Field::ALL.map(|_| serde_json::Value::Null),
            initialized: false,
            open: None,
            selected: None,
            original: String::new(),
            preview: String::new(),
            scroll: gpui::ScrollHandle::new(),
        }
    }
}

impl MatrixWindow {
    pub(super) fn captured_search_input(&self, field: Field, cx: &Context<Self>) -> String {
        field.capture(&self.search_history_input(field).read(cx).text())
    }
    pub(super) fn focus_search_history_input(&self, field: Field, cx: &mut Context<Self>) {
        let Some(handle) = self
            .floating_windows
            .get(&floating_windows::DialogKind::Find)
            .copied()
        else {
            return;
        };
        let input = self.search_history_input(field);
        cx.defer(move |cx| {
            if !cx.windows().contains(&handle.into()) {
                return;
            }
            if let Err(error) = handle.update(cx, |_, window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx))
            }) {
                eprintln!("search field focus could not reach its window: {error}");
            }
        });
    }

    pub(super) fn configure_search_history(&mut self, settings: &serde_json::Value) {
        self.search_history.histories =
            Field::ALL.map(|field| History::load(field, &settings["FindHistory"], equivalent));
        self.search_history.persisted =
            Field::ALL.map(|field| settings["FindHistory"][field.key()].clone());
        self.search_history.initialized = false;
        self.search_history.open = None;
    }

    pub(super) fn initialize_search_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_history.initialized {
            return;
        }
        self.search_history.initialized = true;
        for field in Field::ALL {
            let value = self.search_history.histories[field.index()].initial_text();
            let input = self.search_history_input(field);
            if input.read(cx).text().is_empty() && !value.is_empty() {
                input.update(cx, |input, cx| input.set_text(value, window, cx));
            }
        }
    }

    pub(super) fn record_search_history(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_search_histories(&[field], window, cx);
    }

    pub(super) fn record_search_histories(
        &mut self,
        fields: &[Field],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut saved = Vec::with_capacity(fields.len());
        for &field in fields {
            let text = self.captured_search_input(field, cx);
            let input = self.search_history_input(field);
            let original = input.read(cx).text();
            if !text.is_empty() && original != text {
                input.update(cx, |input, cx| input.set_text(&text, window, cx));
            }
            self.search_history.histories[field.index()].record(&text, equivalent);
            if self.search_history.open == Some(field) && !text.is_empty() {
                self.search_history.selected = Some(0);
                self.search_history.preview = text.clone();
                self.search_history.scroll.scroll_to_item(0);
            }
            saved.push((
                field,
                self.search_history.histories[field.index()].saved(&text),
            ));
        }
        self.persist_search_histories(&saved, cx);
    }

    pub(super) fn save_search_history_on_close(&mut self, cx: &mut Context<Self>) {
        self.close_search_history();
        if !self.search_history.initialized {
            return;
        }
        let saved = Field::ALL.map(|field| {
            let current = self.search_history_input(field).read(cx).text();
            (
                field,
                self.search_history.histories[field.index()].saved(&current),
            )
        });
        self.persist_search_histories(&saved, cx);
    }

    fn persist_search_histories(&mut self, saved: &[(Field, Vec<String>)], cx: &mut Context<Self>) {
        let Some(store) = &self.settings_store else {
            return;
        };
        match store.update_settings(|settings| {
            if !settings["FindHistory"].is_null() && !settings["FindHistory"].is_object() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "FindHistory must be an object",
                ));
            }
            for (field, entries_to_save) in saved {
                let entries = &settings["FindHistory"][field.key()];
                if !entries.is_null()
                    && entries
                        .as_array()
                        .is_none_or(|entries| entries.iter().any(|entry| !entry.is_string()))
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "search history entries must be an array of strings",
                    ));
                }
                if entries != &self.search_history.persisted[field.index()] {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "search history changed by another window or process",
                    ));
                }
                settings["FindHistory"][field.key()] = serde_json::json!(entries_to_save);
            }
            Ok(())
        }) {
            Ok(settings) => {
                for (field, _) in saved {
                    self.search_history.persisted[field.index()] =
                        settings["FindHistory"][field.key()].clone();
                }
            }
            Err(error) => {
                let message = format!("Search history could not be saved: {error}");
                let owner = cx.weak_entity();
                cx.defer(move |cx| {
                    if let Err(error) = owner.update(cx, |owner, cx| {
                        owner.notify_editor_error(message.clone(), cx)
                    }) {
                        eprintln!("{message}; error report lost its window: {error}");
                    }
                });
            }
        }
    }

    fn open_search_history(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.search_history_input(field);
        let text = input.read(cx).text();
        let entries = self.search_history.histories[field.index()].entries();
        if self.search_history.open == Some(field) || entries.is_empty() {
            self.search_history.open = None;
        } else {
            self.search_history.open = Some(field);
            self.search_history.selected =
                entries.iter().position(|entry| equivalent(entry, &text));
            self.search_history.original = text.clone();
            self.search_history.preview = text;
            self.search_history
                .scroll
                .set_offset(gpui::Point::default());
            if let Some(index) = self.search_history.selected {
                self.search_history.scroll.scroll_to_item(index);
            }
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn move_search_history(
        &mut self,
        field: Field,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.search_history_input(field).read(cx).text();
        if current != self.search_history.preview {
            self.search_history.selected = None;
        }
        let entries = self.search_history.histories[field.index()].entries();
        if entries.is_empty() {
            return;
        }
        let index = if down {
            self.search_history
                .selected
                .map_or(0, |index| (index + 1).min(entries.len() - 1))
        } else {
            self.search_history
                .selected
                .map_or(entries.len() - 1, |index| index.saturating_sub(1))
        };
        let text = entries[index].clone();
        self.search_history.selected = Some(index);
        self.search_history.preview = text.clone();
        self.search_history.scroll.scroll_to_item(index);
        self.search_history_input(field)
            .update(cx, |input, cx| input.set_text(&text, window, cx));
        cx.notify();
    }

    fn accept_search_history(
        &mut self,
        field: Field,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.search_history.histories[field.index()]
            .entries()
            .get(index)
            .cloned()
        else {
            return;
        };
        self.search_history.open = None;
        self.search_history_input(field).update(cx, |input, cx| {
            input.set_text(&text, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn accept_current_search_history(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.search_history_input(field);
        if input.read(cx).text() == self.search_history.preview
            && let Some(index) = self.search_history.selected
        {
            self.accept_search_history(field, index, window, cx);
        } else {
            self.search_history.open = None;
            input.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    fn cancel_search_history(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.search_history.original.clone();
        self.search_history.open = None;
        self.search_history_input(field).update(cx, |input, cx| {
            input.set_text(&text, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_search_history(&mut self) -> bool {
        self.search_history.open.take().is_some()
    }

    pub(super) fn search_history_combo(&self, field: Field, cx: &mut Context<Self>) -> AnyElement {
        let key = field.key();
        let open = self.search_history.open == Some(field);
        let entries = self.search_history.histories[field.index()].entries();
        div()
            .relative()
            .w_full()
            .h(px(21.))
            .on_action(cx.listener(move |view, _: &SearchHistoryOpen, window, cx| {
                view.open_search_history(field, window, cx);
                cx.stop_propagation();
            }))
            .when(open, |combo| {
                combo
                    .key_context("MatrixSearchHistory")
                    .on_action(
                        cx.listener(move |view, _: &SearchHistoryAccept, window, cx| {
                            view.accept_current_search_history(field, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_action(
                        cx.listener(move |view, _: &SearchHistoryCancel, window, cx| {
                            view.cancel_search_history(field, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_action(cx.listener(move |view, _: &SearchHistoryUp, window, cx| {
                        view.move_search_history(field, false, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(move |view, _: &SearchHistoryDown, window, cx| {
                        view.move_search_history(field, true, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_action(
                        cx.listener(move |view, _: &SearchHistoryNextField, window, cx| {
                            view.accept_current_search_history(field, window, cx);
                            window.focus_next(cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_action(cx.listener(
                        move |view, _: &SearchHistoryPreviousField, window, cx| {
                            view.accept_current_search_history(field, window, cx);
                            window.focus_prev(cx);
                            cx.stop_propagation();
                        },
                    ))
            })
            .child(self.search_history_input(field))
            .child(
                div()
                    .id(format!("search-history-button-{key}"))
                    .debug_selector(move || format!("search-history-button-{key}"))
                    .role(Role::Button)
                    .aria_label(format!("{key} history"))
                    .absolute()
                    .right(px(1.))
                    .top(px(1.))
                    .w(px(18.))
                    .h(px(19.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x606060))
                    .child("⌄")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_search_history(field, window, cx);
                        cx.stop_propagation();
                    })),
            )
            .when(open && !entries.is_empty(), |combo| {
                combo.child(
                    gpui::deferred(
                        gpui::anchored()
                            .offset(gpui::point(px(0.), px(21.)))
                            .snap_to_window()
                            .child(
                                div()
                                    .id("search-history-popup")
                                    .debug_selector(|| "search-history-popup".into())
                                    .role(Role::ListBox)
                                    .w(px(255.))
                                    .max_h(px(180.))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.search_history.scroll)
                                    .border_1()
                                    .border_color(rgb(0x808080))
                                    .bg(rgb(0xffffff))
                                    .text_color(rgb(0x202020))
                                    .occlude()
                                    .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                                        view.search_history.open = None;
                                        cx.notify();
                                    }))
                                    .children(entries.iter().enumerate().map(|(index, text)| {
                                        div()
                                            .id(format!("search-history-item-{key}-{index}"))
                                            .debug_selector(move || {
                                                format!("search-history-item-{key}-{index}")
                                            })
                                            .role(Role::ListBoxOption)
                                            .aria_label(text.clone())
                                            .aria_selected(
                                                self.search_history.selected == Some(index),
                                            )
                                            .h(px(20.))
                                            .px(px(4.))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .when(
                                                self.search_history.selected == Some(index),
                                                |item| {
                                                    item.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                                                },
                                            )
                                            .child(text.clone())
                                            .on_click(cx.listener(move |view, _, window, cx| {
                                                view.accept_search_history(
                                                    field, index, window, cx,
                                                );
                                                cx.stop_propagation();
                                            }))
                                    })),
                            ),
                    )
                    .priority(2),
                )
            })
            .into_any_element()
    }
}
