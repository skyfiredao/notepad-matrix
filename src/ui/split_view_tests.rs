use super::*;
use crate::workspace::ViewId;

fn redraw(cx: &mut gpui::VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }
}

fn viewport(view: &mut MatrixWindow, pane: ViewId) -> (usize, f32) {
    let previous = view.pane;
    assert!(view.scope_editor_pane(pane));
    let result = (
        view.editor_top_line,
        f32::from(view.editor_horizontal_offset),
    );
    assert!(view.scope_editor_pane(previous));
    result
}

#[gpui::test]
fn clone_panes_have_independent_focus_selection_and_shared_undo(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("a");
    cx.dispatch_action(CloneToOtherView);
    assert!(cx.debug_bounds("matrix-editor-primary").is_some());
    assert!(cx.debug_bounds("matrix-editor-secondary").is_some());
    cx.dispatch_action(EditorDocumentStart);
    cx.simulate_input("b");
    cx.dispatch_action(EditorUndo);
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor().display_text(), "a");
        view.focus_editor_pane(crate::workspace::ViewId::Primary, window, cx);
        assert_eq!(view.editor().selection().head.0, 1);
        assert_eq!(view.editor().display_text(), "a");
    });
    cx.dispatch_action(EditorUndo);
    assert_eq!(
        view.read_with(cx, |view, _| view.editor().display_text()),
        ""
    );
}

#[gpui::test]
fn independent_tabgroups_move_and_closing_a_dirty_clone_keep_the_shared_document(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("unsaved shared text");
    cx.dispatch_action(CloneToOtherView);
    cx.dispatch_action(NewFile);
    let id = view.read_with(cx, |view, _| view.workspace.active().id());
    cx.dispatch_action(MoveToOtherView);
    view.read_with(cx, |view, _| {
        assert_eq!(view.workspace.active_view(), ViewId::Primary);
        assert_eq!(
            view.workspace.view_buffer_ids(ViewId::Primary),
            &[crate::workspace::BufferId(1), id]
        );
        assert_eq!(
            view.workspace.view_buffer_ids(ViewId::Secondary),
            &[crate::workspace::BufferId(1)]
        );
    });
    let clone_tab = cx
        .debug_bounds("buffer-tab-secondary-1")
        .expect("secondary clone tab");
    cx.simulate_click(clone_tab.center(), gpui::Modifiers::default());
    cx.dispatch_action(CloseCurrentFile);
    redraw(cx);
    view.read_with(cx, |view, _| {
        assert!(
            !view.close_dialog,
            "a surviving dirty clone does not need save confirmation"
        );
        assert!(!view.split_views_visible());
        let original = view
            .workspace
            .buffers()
            .iter()
            .find(|buffer| buffer.id().0 == 1)
            .expect("surviving shared document");
        assert_eq!(original.editor.display_text(), "unsaved shared text");
        assert!(original.is_modified());
    });
    assert!(cx.debug_bounds("matrix-editor-secondary").is_none());
}

#[gpui::test]
fn sync_requires_two_panes_and_tracks_wheel_postpaint_without_reflecting_back(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    let checked = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let bounds = cx.debug_bounds(selector).expect("sync toolbar button");
        let editor = cx.debug_bounds("matrix-editor").expect("editor");
        cx.simulate_mouse_move(editor.center(), None, gpui::Modifiers::default());
        redraw(cx);
        cx.update(|window, _| {
            let bounds = bounds.scale(window.scale_factor());
            window.painted_quads().iter().any(|quad| {
                quad.bounds == bounds && quad.content_mask.bounds.contains(&bounds.center())
            })
        })
    };
    for selector in ["toolbar-sync-vertical", "toolbar-sync-horizontal"] {
        let button = cx.debug_bounds(selector).expect("disabled sync button");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window))
        });
        assert!(!checked(cx, selector));
    }
    assert!(cx.debug_bounds("matrix-editor-secondary").is_none());
    view.read_with(cx, |view, _| {
        assert!(!view.sync_vertical_enabled() && !view.sync_horizontal_enabled())
    });
    cx.simulate_input(&format!(
        "{}\n{}",
        "wide ".repeat(150),
        "line\n".repeat(200)
    ));
    cx.dispatch_action(EditorDocumentStart);
    cx.dispatch_action(CloneToOtherView);
    view.update_in(cx, |view, window, cx| {
        view.set_zoom_delta(5, window, cx);
    });
    redraw(cx);
    let primary = cx.debug_bounds("matrix-editor").expect("primary viewport");
    let secondary = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("secondary viewport");
    cx.simulate_event(gpui::ScrollEvent {
        position: secondary.origin + gpui::point(px(100.), px(30.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-100.))),
        ..Default::default()
    });
    redraw(cx);
    let initial = view.update_in(cx, |view, _, _| {
        (
            viewport(view, ViewId::Primary),
            viewport(view, ViewId::Secondary),
        )
    });
    let p_widths = view.update_in(cx, |view, _, _| {
        let secondary = view.editor_chrome.p_width();
        assert!(view.scope_editor_pane(ViewId::Primary));
        let primary = view.editor_chrome.p_width();
        assert!(view.scope_editor_pane(ViewId::Secondary));
        (primary, secondary)
    });
    assert_eq!(initial.0.0, 0);
    assert!(initial.1.0 > 0);
    let document = |view: &MatrixWindow| {
        (
            view.workspace.active().id(),
            view.workspace.active_view(),
            view.editor().committed_text(),
            view.editor().selections(),
            view.editor().document().can_undo(),
            view.editor().document().can_redo(),
            view.workspace.active().is_modified(),
        )
    };
    let before = view.read_with(cx, |view, _| document(view));
    for (index, selector) in ["toolbar-sync-vertical", "toolbar-sync-horizontal"]
        .into_iter()
        .enumerate()
    {
        let button = cx.debug_bounds(selector).expect("enabled sync button");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        assert!(checked(cx, "toolbar-sync-vertical"));
        assert_eq!(checked(cx, "toolbar-sync-horizontal"), index == 1);
        view.read_with(cx, |view, _| {
            assert!(view.sync_vertical_enabled());
            assert_eq!(view.sync_horizontal_enabled(), index == 1);
            assert_eq!(document(view), before, "sync only changes display linking");
        });
    }
    cx.simulate_event(gpui::ScrollEvent {
        position: primary.origin + gpui::point(px(100.), px(30.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-67.), px(-120.))),
        ..Default::default()
    });
    redraw(cx);
    let changed = view.update_in(cx, |view, _, _| {
        (
            viewport(view, ViewId::Primary),
            viewport(view, ViewId::Secondary),
        )
    });
    assert_eq!(changed.1.0 - initial.1.0, changed.0.0 - initial.0.0);
    assert!(changed.0.1 > 0. && changed.1.1 > 0.);
    assert!((changed.1.1 - (changed.0.1 / p_widths.0).trunc() * p_widths.1).abs() < 0.1);
    assert!(
        (changed.1.1 - changed.0.1).abs() > 1.,
        "horizontal sync uses columns, not mirrored pixels at different zooms"
    );
    redraw(cx);
    let settled = view.update_in(cx, |view, _, _| {
        (
            viewport(view, ViewId::Primary),
            viewport(view, ViewId::Secondary),
        )
    });
    assert_eq!(
        settled, changed,
        "passive target painting must not feed back into the source"
    );
    cx.dispatch_action(CloseCurrentFile);
    redraw(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.sync_vertical_enabled() && !view.sync_horizontal_enabled())
    });
}

#[gpui::test]
fn udl_folds_are_pane_local_and_style_sizes_change_real_native_geometry(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("begin\ninside\nend\ntoken t");
    view.update_in(cx, |view, window, cx| {
        view.udl
            .registry
            .edit(0, |definition| {
                definition.folding[0].open = "begin".into();
                definition.folding[0].close = "end".into();
                definition.keywords[0] = "token".into();
                definition.styles[0].font_size = 13;
                definition.styles[4].font_size = 26;
                definition.styles[4].font_name = "Menlo".into();
                Ok(())
            })
            .expect("configure UDL");
        view.workspace
            .active_mut()
            .set_language(crate::syntax::Language::UserDefined(0));
        view.invalidate_style_layout(window, cx);
    });
    cx.dispatch_action(CloneToOtherView);
    redraw(cx);
    let fold = cx
        .debug_bounds("fold-line-secondary-0")
        .expect("real secondary fold control");
    let snapshot = |view: &MatrixWindow| {
        (
            view.editor().committed_text(),
            view.editor().selections(),
            view.editor().document().can_undo(),
            view.editor().document().can_redo(),
            view.workspace.active().is_modified(),
        )
    };
    let before = view.read_with(cx, |view, _| snapshot(view));
    cx.simulate_click(fold.center(), gpui::Modifiers::default());
    redraw(cx);
    let width = view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor_visual_row_count(), 2);
        assert_eq!(snapshot(view), before, "fold changes visibility only");
        assert!(view.focus_editor_pane(ViewId::Primary, window, cx));
        assert_eq!(view.editor_visual_row_count(), 4);
        use gpui::EntityInputHandler;
        let token =
            EntityInputHandler::bounds_for_range(view, 17..22, Bounds::default(), window, cx)
                .expect("shaped large token");
        let normal =
            EntityInputHandler::bounds_for_range(view, 23..24, Bounds::default(), window, cx)
                .expect("shaped normal letter");
        assert!(token.size.width > normal.size.width * 8.);
        assert!(token.size.width < normal.size.width * 12.);
        token.size.width
    });
    view.update_in(cx, |view, window, cx| {
        view.udl
            .registry
            .edit(0, |definition| {
                definition.styles[4].font_size = 13;
                Ok(())
            })
            .expect("change style only");
        view.invalidate_style_layout(window, cx);
    });
    redraw(cx);
    view.update_in(cx, |view, window, cx| {
        use gpui::EntityInputHandler;
        let token =
            EntityInputHandler::bounds_for_range(view, 17..22, Bounds::default(), window, cx)
                .expect("restyled token");
        assert!(
            token.size.width < width * 0.6,
            "definition-only edits must invalidate actual shaped geometry"
        );
        assert_eq!(view.editor().display_text(), "begin\ninside\nend\ntoken t");
    });
}

#[gpui::test]
fn pane_tab_switch_restores_its_viewport_without_touching_the_peer(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input(&"line\n".repeat(200));
    cx.dispatch_action(EditorDocumentStart);
    cx.dispatch_action(CloneToOtherView);
    redraw(cx);
    let secondary = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("secondary viewport");
    cx.simulate_event(gpui::ScrollEvent {
        position: secondary.origin + gpui::point(px(100.), px(30.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-240.))),
        ..Default::default()
    });
    redraw(cx);
    let before = view.update_in(cx, |view, _, _| {
        (
            viewport(view, ViewId::Primary),
            viewport(view, ViewId::Secondary),
        )
    });
    cx.dispatch_action(NewFile);
    cx.dispatch_action(PreviousBuffer);
    redraw(cx);
    let after = view.update_in(cx, |view, _, _| {
        (
            viewport(view, ViewId::Primary),
            viewport(view, ViewId::Secondary),
        )
    });
    assert_eq!(after, before);
}

#[gpui::test]
fn collapsed_header_tracks_shared_edits_and_undo_and_goto_reveals_its_body(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("lead\nbegin\nbody\nend\nafter");
    view.update_in(cx, |view, window, cx| {
        view.udl
            .registry
            .edit(0, |definition| {
                definition.folding[0].open = "begin".into();
                definition.folding[0].close = "end".into();
                Ok(())
            })
            .expect("configure fold");
        view.workspace
            .active_mut()
            .set_language(crate::syntax::Language::UserDefined(0));
        view.invalidate_style_layout(window, cx);
    });
    cx.dispatch_action(CloneToOtherView);
    redraw(cx);
    let fold = cx
        .debug_bounds("fold-line-secondary-1")
        .expect("secondary header");
    cx.simulate_click(fold.center(), gpui::Modifiers::default());
    view.update_in(cx, |view, window, cx| {
        view.focus_editor_pane(ViewId::Primary, window, cx);
        gpui::EntityInputHandler::replace_text_in_range(view, Some(0..0), "\n", window, cx);
    });
    redraw(cx);
    view.update_in(cx, |view, _, _| {
        view.scope_editor_pane(ViewId::Secondary);
        assert_eq!(
            view.editor_visual_row_count(),
            4,
            "collapsed header follows the insertion in the shared document"
        );
        assert!(view.editor_fold_collapsed(2));
        view.scope_editor_pane(ViewId::Primary);
    });
    cx.dispatch_action(EditorUndo);
    redraw(cx);
    view.update_in(cx, |view, window, cx| {
        view.focus_editor_pane(ViewId::Secondary, window, cx);
        assert_eq!(view.editor_visual_row_count(), 3);
        assert!(view.editor_fold_collapsed(1));
        view.goto_visible = true;
        view.goto_input
            .update(cx, |field, cx| field.set_text("3", window, cx));
        view.confirm_goto(window, cx);
    });
    redraw(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.editor_fold_collapsed(1));
        assert_eq!(view.editor_visual_row_count(), 5);
        assert_eq!(view.editor().selection().head.0, 11);
    });
}

#[gpui::test]
fn reference_profile_uses_measured_editor_origin_and_row_pitch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("a\nb");
    redraw(cx);
    let editor = cx.debug_bounds("matrix-editor").expect("editor viewport");
    assert_eq!(
        cx.debug_bounds("buffer-tabs")
            .expect("tab strip")
            .size
            .height,
        px(27.)
    );
    assert_eq!(editor.top(), px(104.));
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor_font_size(), px(20.));
        assert_eq!(view.editor_line_height(), px(26.));
        let first =
            gpui::EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
                .expect("first row");
        let next =
            gpui::EntityInputHandler::bounds_for_range(view, 2..2, Bounds::default(), window, cx)
                .expect("second row");
        assert_eq!(first.left() - editor.left(), px(72.));
        assert_eq!(first.top() - editor.top(), px(3.));
        assert_eq!(next.top() - first.top(), px(26.));
    });
}

#[gpui::test]
fn fold_markers_survive_label_edits_but_not_deleted_header_replacements(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("lead\nbegin label\nbody\nend\nafter");
    view.update_in(cx, |view, window, cx| {
        view.udl
            .registry
            .edit(0, |definition| {
                definition.folding[0].open = "begin".into();
                definition.folding[0].close = "end".into();
                Ok(())
            })
            .expect("configure fold");
        view.workspace
            .active_mut()
            .set_language(crate::syntax::Language::UserDefined(0));
        view.invalidate_style_layout(window, cx);
    });
    cx.dispatch_action(CloneToOtherView);
    redraw(cx);
    let fold = cx
        .debug_bounds("fold-line-secondary-1")
        .expect("secondary header");
    cx.simulate_click(fold.center(), gpui::Modifiers::default());
    view.update_in(cx, |view, window, cx| {
        view.focus_editor_pane(ViewId::Primary, window, cx);
        gpui::EntityInputHandler::replace_text_in_range(view, Some(11..16), "renamed", window, cx);
    });
    redraw(cx);
    view.update_in(cx, |view, _, _| {
        view.scope_editor_pane(ViewId::Secondary);
        assert!(
            view.editor_fold_collapsed(1),
            "line flag survives a non-keyword label edit"
        );
        assert_eq!(view.editor_visual_row_count(), 3);
        view.scope_editor_pane(ViewId::Primary);
    });
    view.update_in(cx, |view, window, cx| {
        gpui::EntityInputHandler::replace_text_in_range(view, Some(5..19), "", window, cx);
        gpui::EntityInputHandler::replace_text_in_range(
            view,
            Some(5..5),
            "begin other\n",
            window,
            cx,
        );
    });
    redraw(cx);
    view.update_in(cx, |view, _, _| {
        view.scope_editor_pane(ViewId::Secondary);
        assert!(
            !view.editor_fold_collapsed(1),
            "a new unrelated header must not inherit a deleted header's flag"
        );
        assert_eq!(view.editor_visual_row_count(), 5);
        view.scope_editor_pane(ViewId::Primary);
    });
}

#[gpui::test]
fn splitter_resizes_both_real_editors_without_changing_focus_or_document(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("a");
    cx.dispatch_action(CloneToOtherView);
    redraw(cx);
    let before = cx.debug_bounds("matrix-editor").expect("primary viewport");
    let peer = cx
        .debug_bounds("matrix-editor-secondary")
        .expect("secondary viewport");
    let splitter = cx.debug_bounds("editor-splitter").expect("splitter");
    let start = splitter.center();
    let end = start + gpui::point(px(100.), px(0.));
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
        ..Default::default()
    });
    redraw(cx);
    assert!(
        cx.debug_bounds("matrix-editor")
            .expect("resized primary")
            .size
            .width
            > before.size.width + px(50.)
    );
    assert!(
        cx.debug_bounds("matrix-editor-secondary")
            .expect("resized secondary")
            .size
            .width
            < peer.size.width - px(50.)
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.workspace.active_view(), ViewId::Secondary);
        assert_eq!(view.editor().display_text(), "a");
    });
}

#[gpui::test]
fn clone_pane_uses_its_own_overwrite_mode_for_native_input(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("xy");
    cx.dispatch_action(ToggleOverwrite);
    cx.dispatch_action(CloneToOtherView);
    cx.dispatch_action(EditorDocumentStart);
    cx.simulate_input("z");
    view.update_in(cx, |view, window, cx| {
        assert!(!view.overwrite_mode);
        assert!(!view.editor().overwrite());
        assert_eq!(view.editor().display_text(), "zxy");
        view.focus_editor_pane(ViewId::Primary, window, cx);
        assert!(view.overwrite_mode);
        assert!(view.editor().overwrite());
    });
    cx.dispatch_action(EditorDocumentStart);
    cx.simulate_input("q");
    assert_eq!(
        view.read_with(cx, |view, _| view.editor().display_text()),
        "qxy"
    );
}

#[gpui::test]
fn proportional_tabs_use_pixel_stops_and_preserve_native_byte_hit_testing(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    cx.simulate_input("Wi\tX\tY");
    view.update_in(cx, |view, window, cx| {
        view.udl
            .registry
            .edit(0, |definition| {
                definition.keywords[0] = "Wi".into();
                definition.styles[4].font_size = 26;
                Ok(())
            })
            .expect("configure mixed advance widths");
        view.workspace
            .active_mut()
            .set_language(crate::syntax::Language::UserDefined(0));
        view.invalidate_style_layout(window, cx);
    });
    redraw(cx);
    let hit = view.update_in(cx, |view, window, cx| {
        use gpui::EntityInputHandler;
        let origin =
            EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
                .expect("origin");
        let before =
            EntityInputHandler::bounds_for_range(view, 2..2, Bounds::default(), window, cx)
                .expect("before tab");
        let after = EntityInputHandler::bounds_for_range(view, 3..4, Bounds::default(), window, cx)
            .expect("after tab");
        let space = window
            .text_system()
            .shape_line(
                " ".into(),
                view.editor_font_size(),
                &[gpui::TextRun {
                    len: 1,
                    font: gpui::font(view.editor_font_family()),
                    color: rgb(0).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width();
        let interval = f32::from(space) * 4.;
        let expected =
            ((f32::from(before.left() - origin.left()) / interval).floor() + 1.) * interval;
        assert!((f32::from(after.left() - origin.left()) - expected).abs() < 0.01);
        gpui::point(after.left() + after.size.width * 0.1, after.center().y)
    });
    cx.simulate_click(hit, gpui::Modifiers::default());
    view.update_in(cx, |view, window, cx| {
        let selection = gpui::EntityInputHandler::selected_text_range(view, false, window, cx)
            .expect("native caret");
        assert_eq!(selection.range, 3..3);
        assert_eq!(view.editor().display_text(), "Wi\tX\tY");
    });
    let before = view.read_with(cx, |view, _| {
        (
            view.workspace.active().id(),
            view.workspace.active_view(),
            view.editor().document().can_undo(),
            view.editor().document().can_redo(),
            view.workspace.active().is_modified(),
        )
    });
    let origin = view
        .update_in(cx, |view, window, cx| {
            gpui::EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
        })
        .expect("painted start boundary");
    cx.simulate_click(
        origin.center(),
        gpui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor().selection(),
            Selection {
                anchor: ByteOffset(3),
                head: ByteOffset(0)
            }
        );
        assert_eq!(view.editor().committed_text(), "Wi\tX\tY");
        assert_eq!(
            (
                view.workspace.active().id(),
                view.workspace.active_view(),
                view.editor().document().can_undo(),
                view.editor().document().can_redo(),
                view.workspace.active().is_modified()
            ),
            before
        );
    });
}

#[gpui::test]
fn scrollbar_arrow_endcaps_step_one_row_or_default_column_and_reserved_thumb_track_reaches_end(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    let source = format!(
        "{}\r\n{}",
        "wide\t中😀 ".repeat(150),
        "line\r\n".repeat(200)
    );
    view.update_in(cx, |view, window, cx| {
        gpui::EntityInputHandler::replace_text_in_range(view, None, &source, window, cx);
    });
    cx.dispatch_action(EditorDocumentStart);
    cx.dispatch_action(EditorSelectRight);
    redraw(cx);
    let editor = cx.debug_bounds("matrix-editor").expect("editor viewport");
    let click = |cx: &mut gpui::VisualTestContext, x, y| {
        cx.simulate_click(gpui::point(x, y), gpui::Modifiers::default());
        redraw(cx);
    };
    click(cx, editor.right() - px(8.), editor.bottom() - px(24.));
    assert_eq!(view.read_with(cx, |view, _| view.editor_top_line), 1);
    click(cx, editor.right() - px(8.), editor.top() + px(8.));
    assert_eq!(view.read_with(cx, |view, _| view.editor_top_line), 0);
    let row_height = view
        .update_in(cx, |view, window, cx| {
            gpui::EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
        })
        .expect("native visual row height")
        .size
        .height;
    for (direction, expected) in [(-1., 1), (1., 0)] {
        cx.simulate_event(gpui::ScrollEvent {
            position: editor.origin + gpui::point(px(100.), px(30.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), row_height * direction)),
            ..Default::default()
        });
        redraw(cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor_top_line),
            expected,
            "one visual-row wheel equals one arrow step"
        );
    }
    click(cx, editor.right() - px(24.), editor.bottom() - px(8.));
    view.read_with(cx, |view, _| {
        assert!(
            (f32::from(view.editor_horizontal_offset) - view.editor_chrome.p_width()).abs() < 0.01
        )
    });
    click(cx, editor.left() + px(8.), editor.bottom() - px(8.));
    assert_eq!(
        view.read_with(cx, |view, _| view.editor_horizontal_offset),
        px(0.)
    );
    let (total, capacity) = view.read_with(cx, |view, _| {
        (
            view.editor_visual_row_count(),
            view.editor_visible_capacity(),
        )
    });
    let track = f32::from(editor.size.height) - 48.;
    let thumb = (track * capacity as f32 / total as f32).max(18.);
    let start = gpui::point(editor.right() - px(8.), editor.top() + px(16. + thumb / 2.));
    let end = gpui::point(start.x, editor.bottom() - px(32.));
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
        ..Default::default()
    });
    redraw(cx);
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.editor_top_line, total - capacity);
        let selection = gpui::EntityInputHandler::selected_text_range(view, false, window, cx)
            .expect("preserved selection");
        assert_eq!(selection.range, 0..1);
        assert!(!selection.reversed);
        assert_eq!(view.editor().committed_text(), source);
        assert!(view.editor().document().can_undo() && !view.editor().document().can_redo());
    });
    cx.dispatch_action(EditorUndo);
    assert_eq!(
        view.read_with(cx, |view, _| view.editor().display_text()),
        ""
    );
}
