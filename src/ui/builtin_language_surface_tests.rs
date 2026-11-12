use super::*;
use gpui::EntityInputHandler;

fn base_run(len: usize) -> gpui::TextRun {
    gpui::TextRun {
        len,
        font: gpui::font("Menlo"),
        color: rgb(0x000000).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

#[gpui::test]
fn builtin_runtime_styles_and_actual_folds_reach_native_editor(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let text = "int main() {\n  return 7;\n}\n";
        EntityInputHandler::replace_text_in_range(view, None, text, window, cx);
        view.select_language(crate::syntax::Language::Builtin(3), window, cx);
        let runs = view.editor_style_runs(0, text.len(), None, base_run(text.len()));
        assert_eq!(
            runs.iter().map(|(run, _)| run.len).sum::<usize>(),
            text.len()
        );
        let mut byte = 0;
        let mut saw_keyword = false;
        for (run, size) in runs {
            if byte == text.find("return").expect("keyword") {
                saw_keyword = true;
                assert_eq!(run.color, gpui::Hsla::from(rgb(0x0000ff)));
                assert_eq!(run.font.weight, gpui::FontWeight::BOLD);
                assert_eq!(size, view.editor_font_size());
            }
            byte += run.len;
        }
        assert!(saw_keyword);
        assert!(view.builtin_fold_ranges().contains(&(0, 2)));
        assert!(
            view.builtin_style_appearances()
                .contains(&view.editor_style_profile())
        );
        let marked = ByteOffset(1)..ByteOffset(2);
        let runs = view.builtin_style_runs(0, 3, Some(&marked), base_run(3));
        assert_eq!(
            runs.iter().map(|(run, _)| run.len).collect::<Vec<_>>(),
            vec![1, 1, 1]
        );
        assert!(runs[1].0.underline.is_some());
        assert_eq!(runs[0].0.color, runs[1].0.color);
        assert_eq!(runs[0].0.font, runs[1].0.font);
    });
}

#[gpui::test]
fn json_retains_legacy_styles_with_runtime_fold_ranges(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let text = "{\n  \"key\": [\n    true\n  ]\n}\n";
        EntityInputHandler::replace_text_in_range(view, None, text, window, cx);
        view.select_language(crate::syntax::Language::Json, window, cx);
        let folds = view.builtin_fold_ranges();
        assert!(folds.contains(&(0, 4)), "{folds:?}");
        assert!(folds.contains(&(1, 3)), "{folds:?}");
        let runs = view.editor_style_runs(0, text.len(), None, base_run(text.len()));
        assert_eq!(
            runs.iter().map(|(run, _)| run.len).sum::<usize>(),
            text.len()
        );
        let (_, _, spans) = view.syntax_cache.as_ref().expect("legacy JSON cache");
        assert_eq!(spans, &crate::syntax::json_styles(text));
    });
}

#[gpui::test]
fn builtin_cache_refreshes_style_revision_without_revisiting_lexer(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        EntityInputHandler::replace_text_in_range(view, None, "int value;", window, cx);
        view.select_language(crate::syntax::Language::Builtin(3), window, cx);
        view.builtin.style_revision += 1;
        assert_eq!(
            view.refresh_builtin_syntax_update(),
            Some(crate::builtin_language::CacheUpdate::Restyled)
        );
        view.dark = true;
        assert_eq!(
            view.refresh_builtin_syntax_update(),
            Some(crate::builtin_language::CacheUpdate::Restyled)
        );
        let runs = view.builtin_style_runs(0, 10, None, base_run(10));
        assert_eq!(runs.iter().map(|(run, _)| run.len).sum::<usize>(), 10);
        assert!(
            runs.iter()
                .all(|(run, _)| run.background_color != Some(rgb(0xffffff).into()))
        );
    });
}

#[gpui::test]
fn native_tag_indicator_api_uses_caret_and_matching_options(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let text = "<div key=\"value\">text</div>";
        EntityInputHandler::replace_text_in_range(view, None, text, window, cx);
        view.select_language(crate::syntax::Language::Builtin(8), window, cx);
        view.editor_mut()
            .select(Selection {
                anchor: ByteOffset(2),
                head: ByteOffset(2),
            })
            .expect("caret inside opening tag");
        let indicators = view.builtin_tag_indicators();
        assert_eq!(indicators.len(), 4);
        assert_eq!(indicators[0].range, 0..4);
        assert_eq!(indicators[3].range, 5..16);
        view.builtin.tag_match_options.attributes = false;
        assert_eq!(view.builtin_tag_indicators().len(), 3);
        view.builtin.tag_match_options.enabled = false;
        assert!(view.builtin_tag_indicators().is_empty());
    });
    let home = tempfile::tempdir().expect("isolated pair settings");
    let store = ConfigStore::under_home(home.path());
    let settings = serde_json::json!({"auto-insert":{"parentheses":true,"htmlXmlTag":true},"auto-completion":{"autoCAction":0,"funcParams":false}});
    store
        .update_settings(|value| {
            value["auto-insert"] = settings["auto-insert"].clone();
            value["auto-completion"] = settings["auto-completion"].clone();
            Ok(())
        })
        .expect("pair preferences");
    view.update_in(cx, |view, window, cx| {
        view.settings_store = Some(store);
        view.configure_intelligence(&settings);
        *view.editor_mut() = EditorSession::new("");
        view.select_language(crate::syntax::Language::Builtin(8), window, cx);
    });
    cx.simulate_input("<div>");
    view.read_with(cx, |view, _| {
        assert_eq!(view.editor().committed_text(), "<div></div>")
    });
    cx.dispatch_action(NewFile);
    cx.simulate_input("(");
    view.read_with(cx, |view, _| {
        assert_eq!(view.editor().committed_text(), "()")
    });
    cx.dispatch_action(EditorUndo);
    view.read_with(cx, |view, _| assert_eq!(view.editor().committed_text(), ""));
    cx.dispatch_action(EditorRedo);
    cx.simulate_input(")");
    view.read_with(cx, |view, _| {
        assert_eq!(view.editor().committed_text(), "()")
    });
}

#[gpui::test]
fn cloned_panes_keep_independent_builtin_display_caches(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        EntityInputHandler::replace_text_in_range(view, None, "int value;", window, cx);
        view.select_language(crate::syntax::Language::Builtin(3), window, cx);
        view.clone_to_other_view(window, cx);
        view.refresh_editor_syntax();
        view.editor_mut()
            .set_composition("pending")
            .expect("secondary display composition");
        view.refresh_editor_syntax();
        assert_eq!(
            view.builtin
                .syntax_cache
                .as_ref()
                .expect("secondary cache")
                .runtime
                .text(),
            "int value;pending"
        );
        assert!(view.scope_editor_pane(crate::workspace::ViewId::Primary));
        assert_eq!(
            view.builtin
                .syntax_cache
                .as_ref()
                .expect("primary cache restored")
                .runtime
                .text(),
            "int value;"
        );
        assert!(view.scope_editor_pane(crate::workspace::ViewId::Secondary));
        assert_eq!(
            view.builtin
                .syntax_cache
                .as_ref()
                .expect("secondary cache restored")
                .runtime
                .text(),
            "int value;pending"
        );
    });
}

#[gpui::test]
fn builtin_multi_ime_marks_all_targets_without_erasing_keyword_style(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let text = "int a;\nint b;";
        EntityInputHandler::replace_text_in_range(view, None, text, window, cx);
        view.select_language(crate::syntax::Language::Builtin(3), window, cx);
        let selections = crate::editor::SelectionSet::new(
            vec![
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(0),
                }
                .into(),
                Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(7),
                }
                .into(),
            ],
            1,
        )
        .expect("two composition targets");
        view.editor_mut()
            .set_selections(selections)
            .expect("multiple carets");
        EntityInputHandler::replace_and_mark_text_in_range(view, None, "const ", None, window, cx);
        assert_eq!(view.editor().committed_text(), text);
        let display = view.editor().display_text();
        assert_eq!(display, "const int a;\nconst int b;");
        let marks = view.editor().marked_ranges();
        assert_eq!(
            marks,
            vec![ByteOffset(0)..ByteOffset(6), ByteOffset(13)..ByteOffset(19)]
        );
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    view.update_in(cx, |view, _, _| {
        let display = view.editor().display_text();
        let marks = view.editor().marked_ranges();
        let runs = view.builtin_style_runs(0, display.len(), None, base_run(display.len()));
        let mut byte = 0;
        let mut type_words = 0;
        for (run, _) in runs {
            if marks
                .iter()
                .any(|mark| byte >= mark.start.0 && byte + run.len <= mark.end.0)
            {
                assert!(run.underline.is_some());
            }
            if display[byte..].starts_with("const") {
                assert_eq!(run.color, gpui::Hsla::from(rgb(0x8000ff)));
                assert_eq!(run.font.weight, gpui::FontWeight::NORMAL);
                type_words += 1;
            }
            byte += run.len;
        }
        assert_eq!(byte, display.len());
        assert_eq!(type_words, 2);
    });
}
