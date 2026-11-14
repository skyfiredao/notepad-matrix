use std::sync::{Arc, OnceLock};

use super::*;

fn bookmark_image() -> Arc<gpui::Image> {
    static IMAGE: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                include_bytes!("../../assets/notepad-plus-plus/bookmark14.png").to_vec(),
            ))
        })
        .clone()
}

fn change_marker_style(edition: u8, dark: bool) -> (u32, &'static str) {
    let (name, style, fallback) = match edition {
        1 => ("revert-origin", "Change History revert origin", 0x40a0bf),
        2 => ("saved", "Change History saved", 0x00a000),
        3 => ("modified", "Change History modified", 0xff8000),
        _ => (
            "revert-modified",
            "Change History revert modified",
            0xa0c000,
        ),
    };
    (change_history_colour(style, dark, fallback), name)
}

fn change_history_colour(style: &str, dark: bool, fallback: u32) -> u32 {
    crate::languages::catalog()
        .ok()
        .and_then(|catalog| {
            catalog
                .themes()
                .theme(dark)
                .global_styles
                .iter()
                .find(|entry| {
                    entry
                        .attributes
                        .get("name")
                        .is_some_and(|name| name == style)
                })
        })
        .and_then(|entry| entry.attributes.get("bgColor"))
        .and_then(|colour| u32::from_str_radix(colour, 16).ok())
        .unwrap_or(fallback)
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn wrapped_line_number_digit_growth_preserves_the_document_viewport_anchor(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(480.), px(500.)));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&format!("{}\n", "x".repeat(64)).repeat(1100));
            view.reset_active_view(window, cx);
        });
        cx.dispatch_action(ToggleWordWrap);
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        let editor = cx.debug_bounds("matrix-editor").unwrap();
        cx.simulate_event(gpui::ScrollEvent {
            position: editor.origin + gpui::point(px(100.), px(100.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-52000.))),
            ..Default::default()
        });
        let mut lines = Vec::new();
        for _ in 0..6 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            lines.push(view.read_with(cx, |view, _| {
                view.editor_logical_line_for_visual_row(view.editor_top_line)
            }));
        }
        assert_eq!(
            lines,
            vec![1000; 6],
            "source margin growth reflows around the existing document position, not the old visual row index"
        );
        view.read_with(cx, |view, _| {
            assert!(!view.editor().document().can_undo());
            assert!(!view.editor_follow_caret);
        });
    }

    #[gpui::test]
    fn wrapped_change_history_bars_keep_source_column_and_follow_zoom(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(480.), px(500.)));
        cx.simulate_input(&"word ".repeat(80));
        cx.dispatch_action(ToggleWordWrap);
        cx.dispatch_action(EditorDocumentStart);
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        let first = cx.debug_bounds("change-marker-modified-0-0").unwrap();
        let wrapped = cx
            .debug_bounds("change-marker-modified-0-0-wrap-1")
            .unwrap();
        assert_eq!(
            first.left(),
            wrapped.left(),
            "wrapped lines retain all preceding margin slots"
        );
        assert_eq!(
            first.bottom(),
            wrapped.top(),
            "source bar continues across visual fragments"
        );
        assert_eq!(wrapped.size, size(px(3.), px(26.)));
        cx.dispatch_action(ZoomIn);
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        assert_eq!(
            cx.debug_bounds("change-marker-modified-0-0").unwrap().size,
            size(px(3.), px(28.))
        );
        assert_eq!(
            cx.debug_bounds("editor-gutter").unwrap().size.width,
            px(74.)
        );
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn line_number_gutter_uses_source_digit_width_order_and_text_origin(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new(&"row\n".repeat(2000));
            view.reset_active_view(window, cx);
        });
        cx.run_until_parked();
        let editor = cx.debug_bounds("matrix-editor").unwrap();
        let gutter = cx.debug_bounds("editor-gutter").unwrap();
        assert_eq!(
            gutter.size.width,
            px(71.),
            "source 8+3*8 line number, 16 symbol, 9 history, 14 fold margins"
        );
        assert_eq!(
            cx.debug_bounds("bookmark-line-0").unwrap().left() - editor.left(),
            px(32.),
            "source line numbers precede the symbol margin"
        );
        view.update_in(cx, |view, window, cx| {
            assert_eq!(
                view.editor_text_origin(),
                px(72.),
                "source one-pixel left text margin follows all margins"
            );
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(3),
                    head: ByteOffset(3),
                })
                .unwrap();
            view.editor_changed(Ok(()), window, cx);
        });
        cx.simulate_click(
            editor.origin + gpui::point(px(72.), px(10.)),
            gpui::Modifiers::default(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().selection().head, ByteOffset(0))
        });
        cx.dispatch_action(EditorDocumentEnd);
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        assert_eq!(
            cx.debug_bounds("editor-gutter").unwrap().size.width,
            px(79.),
            "four visible digits expand the margin by one measured digit"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor_text_origin(), px(80.));
            assert!(!view.editor().document().can_undo());
            assert!(!view.workspace.active().is_modified());
        });
    }

    #[gpui::test]
    fn change_history_markers_use_source_full_line_bars(cx: &mut gpui::TestAppContext) {
        let files = tempfile::tempdir().unwrap();
        let path = files.path().join("history.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first\nsecond");
        let first = cx.debug_bounds("change-marker-modified-0-0").unwrap();
        let second = cx.debug_bounds("change-marker-modified-0-1").unwrap();
        assert_eq!(
            first.size.width,
            px(3.),
            "source SC_MARK_BAR is ceil(9/3) wide"
        );
        assert_eq!(
            first.size.height,
            px(26.),
            "source history bar fills the visual line height"
        );
        assert_eq!(
            first.bottom(),
            second.top(),
            "adjacent modified lines have no dot gaps"
        );
        cx.dispatch_action(SaveFile);
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        let saved = cx.debug_bounds("change-marker-saved-0-0").unwrap();
        assert_eq!(saved.size, first.size);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "first\nsecond");
            assert!(!view.workspace.active().is_modified());
            assert!(view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn native_change_history_menu_navigates_blocks_and_clears_only_source_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("0\n1\n2\n3\n4\n5");
            for line in [0, 1, 4, 5] {
                let start = view.editor().document().line_start(line).unwrap();
                view.editor_mut()
                    .select(Selection {
                        anchor: start,
                        head: start,
                    })
                    .unwrap();
                view.editor_mut().insert_text("x").unwrap();
            }
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(0),
                })
                .unwrap();
            view.editor_changed(Ok(()), window, cx);
        });
        for (command, line) in [
            (crate::menu::ChangeHistoryCommand::Next, 4),
            (crate::menu::ChangeHistoryCommand::Previous, 1),
        ] {
            view.update_in(cx, |view, window, cx| {
                view.menu_input(
                    MenuEvent::OpenSubmenu("search-change-history-menu"),
                    window,
                    cx,
                );
                view.menu_input(MenuEvent::First, window, cx);
                if command == crate::menu::ChangeHistoryCommand::Previous {
                    view.menu_input(MenuEvent::Down, window, cx);
                }
                view.menu_input(MenuEvent::Activate, window, cx);
                assert_eq!(
                    view.editor().selection().head,
                    view.editor().document().line_start(line).unwrap()
                );
            });
        }
        let before = view.read_with(cx, |view, _| view.editor().committed_text());
        view.update_in(cx, |view, window, cx| {
            view.menu_input(
                MenuEvent::OpenSubmenu("search-change-history-menu"),
                window,
                cx,
            );
            view.menu_input(MenuEvent::Last, window, cx);
            view.menu_input(MenuEvent::Activate, window, cx);
            assert!(!view.editor().document().can_undo());
            assert!(!view.editor().document().can_redo());
            assert!(view.workspace.active().is_modified());
            assert_eq!(view.editor().document().change_marker_bits(0), 0);
            assert_eq!(view.editor().committed_text(), before);
        });
    }

    #[test]
    fn change_history_palette_reaches_the_renderer_for_both_themes()
    -> Result<(), Box<dyn std::error::Error>> {
        for dark in [false, true] {
            let mut app = gpui::HeadlessAppContext::with_platform(
                std::sync::Arc::new(gpui::NoopTextSystem),
                std::sync::Arc::new(()),
                gpui_platform::current_headless_renderer,
            );
            let handle = app.open_window(size(px(640.), px(480.)), |window, cx| {
                cx.new(|cx| {
                    let mut view = MatrixWindow::new(dark, window, cx);
                    *view.editor_mut() = EditorSession::new("one\ntwo");
                    view
                })
            })?;
            for (step, (name, colour)) in [
                ("modified", [0xff, 0x80, 0x00]),
                ("saved", [0x00, 0xa0, 0x00]),
                ("revert-origin", [0x40, 0xa0, 0xbf]),
                ("saved", [0x00, 0xa0, 0x00]),
                ("modified", [0xff, 0x80, 0x00]),
                ("revert-modified", [0xa0, 0xc0, 0x00]),
            ]
            .into_iter()
            .enumerate()
            {
                handle.update(&mut app, |view, window, cx| {
                    match step {
                        0 => view.editor_mut().insert_text("x")?,
                        1 => {
                            let saved = view.editor().save_snapshot();
                            view.editor_mut().acknowledge_save(&saved)?;
                        }
                        2 | 5 => assert!(view.editor_mut().undo()),
                        3 => assert!(view.editor_mut().redo()),
                        _ => {
                            view.editor_mut().select(Selection {
                                anchor: ByteOffset(0),
                                head: ByteOffset(8),
                            })?;
                            view.editor_mut().insert_text("replacement")?;
                        }
                    }
                    view.editor_changed(Ok(()), window, cx);
                    Ok::<(), crate::editor::EditError>(())
                })??;
                for _ in 0..3 {
                    app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))?;
                    app.run_until_parked();
                }
                let image = app.capture_screenshot(handle.into())?;
                let scale = image.width() / 640;
                let (x, y, _) = image
                    .enumerate_pixels()
                    .find(|(x, y, pixel)| {
                        *x < 72 * scale
                            && *y >= 110 * scale
                            && *y < 125 * scale
                            && pixel.0[..3] == colour
                    })
                    .ok_or_else(|| {
                        format!("source {name} absent in history gutter, dark={dark}")
                    })?;
                let margin = if dark {
                    [0x0c, 0x0c, 0x0c]
                } else {
                    [0xe0, 0xe0, 0xe0]
                };
                assert_eq!(
                    image.get_pixel(x.saturating_sub(2 * scale), y).0[..3],
                    margin,
                    "source history margin next to {name}, dark={dark}"
                );
            }
        }
        Ok(())
    }

    #[gpui::test]
    fn native_change_markers_turn_from_modified_to_saved_without_adding_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated change-marker save");
        let path = home.path().join("saved.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("alpha\nbeta");
        assert!(cx.debug_bounds("change-marker-modified-0-0").is_some());
        assert!(cx.debug_bounds("change-marker-modified-0-1").is_some());
        cx.dispatch_action(SaveFile);
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        assert!(path.exists());
        assert!(cx.debug_bounds("change-marker-saved-0-0").is_some());
        assert!(cx.debug_bounds("change-marker-saved-0-1").is_some());
        assert!(cx.debug_bounds("change-marker-modified-0-0").is_none());
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "alpha\n");
            assert_eq!(view.editor().document().change_marker_bits(0), 2);
        });
        assert!(cx.debug_bounds("change-marker-revert-origin-0-1").is_some());
        cx.dispatch_action(EditorRedo);
        assert!(cx.debug_bounds("change-marker-saved-0-0").is_some());
    }
}

impl MatrixWindow {
    pub(super) fn change_history_command(
        &mut self,
        command: crate::menu::ChangeHistoryCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::menu::ChangeHistoryCommand;
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let result = match command {
            ChangeHistoryCommand::Clear => self.editor_mut().clear_change_history(),
            ChangeHistoryCommand::Next | ChangeHistoryCommand::Previous => self
                .editor_mut()
                .goto_change(command == ChangeHistoryCommand::Next)
                .map(|found| {
                    if found {
                        self.editor_follow_caret = true;
                        self.editor_preferred_x = None;
                        self.editor_scroll_remainder = 0.;
                    } else if !self.mute_sounds && !cx.is_test() {
                        objc2_app_kit::NSBeep();
                    }
                }),
        };
        self.editor_focus.focus(window, cx);
        self.editor_changed(result, window, cx);
    }

    pub(super) fn bookmark_command(
        &mut self,
        command: BookmarkCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Bookmark(command));
        let result = match command {
            BookmarkCommand::Toggle => self.editor_mut().toggle_bookmark(None),
            BookmarkCommand::Next | BookmarkCommand::Previous => {
                let result = self
                    .editor_mut()
                    .goto_bookmark(command == BookmarkCommand::Next);
                if matches!(result, Ok(true)) {
                    self.editor_follow_caret = true;
                    self.editor_preferred_x = None;
                    self.editor_scroll_remainder = 0.;
                }
                result.map(|_| ())
            }
            BookmarkCommand::ClearAll => {
                self.editor_mut().clear_bookmarks();
                Ok(())
            }
        };
        self.editor_focus.focus(window, cx);
        self.editor_changed(result, window, cx);
    }

    pub(super) fn render_bookmark_gutter(&self, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.dark;
        let pane = self.pane;
        let fold_headers: std::collections::BTreeSet<_> = self
            .udl_fold_ranges()
            .into_iter()
            .map(|(header, _)| header)
            .collect();
        let owner = self.workspace.active().id();
        let text = self.editor().display_snapshot();
        let capacity = self.editor_visible_capacity();
        let row_height = self.editor_line_height();
        let line_number_width = self.editor_line_number_width();
        let (line_number_family, line_number_size) = self.editor_line_number_font();
        let caret_line =
            text.byte_to_line_idx(self.editor().selection().head.0, ropey::LineType::LF_CR);
        let mut lines = self.editor_visible_gutter_rows();
        if lines.is_empty() {
            lines = crate::editor::visible_rows(
                self.editor_logical_line_for_visual_row(self.editor_top_line),
                capacity,
                if self.editor_follow_caret {
                    caret_line
                } else {
                    self.editor_logical_line_for_visual_row(self.editor_top_line)
                },
                text.len_lines(ropey::LineType::LF_CR),
            )
            .map(|line| (line, true))
            .collect();
        }
        let bookmarks: std::collections::BTreeSet<_> =
            self.editor().document().bookmarked_lines().collect();
        let rows = lines
            .into_iter()
            .enumerate()
            .map(|(index, (line, first))| {
                let marked = bookmarks.contains(&line);
                let changes = self.editor().document().change_marker_bits(line);
                div()
                    .id(("editor-gutter", index))
                    .h(row_height)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(line_number_width)
                            .h(row_height)
                            .flex_shrink_0()
                            .pr(px(3.))
                            .text_right()
                            .font_family(line_number_family)
                            .text_size(line_number_size)
                            .line_height(row_height)
                            .child(if first {
                                (line + 1).to_string()
                            } else {
                                String::new()
                            }),
                    )
                    .child(
                        div()
                            .id(("bookmark-line", line))
                            .debug_selector(move || {
                                if !first {
                                    format!("bookmark-line-wrap-{}-{index}", pane as usize)
                                } else if pane == crate::workspace::ViewId::Primary {
                                    format!("bookmark-line-{line}")
                                } else {
                                    format!("bookmark-line-secondary-{line}")
                                }
                            })
                            .role(Role::CheckBox)
                            .aria_label(format!("Bookmark line {}", line + 1))
                            .aria_toggled(marked.into())
                            .w(px(16.))
                            .h(row_height)
                            .flex_shrink_0()
                            .bg(rgb(if self.dark { 0x303030 } else { 0xe0e0e0 }))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if !view.focus_editor_pane(pane, window, cx) {
                                    return;
                                }
                                if view.workspace.active().id() != owner
                                    || view.file_busy
                                    || view.close_dialog
                                    || view.save_all_dialog
                                    || view.batch_editing_blocked()
                                {
                                    return;
                                }
                                let result = view.editor_mut().toggle_bookmark(Some(line));
                                view.editor_focus.focus(window, cx);
                                view.editor_changed(result, window, cx);
                                cx.stop_propagation();
                            }))
                            .when(first && marked, |margin| {
                                margin.child(
                                    div()
                                        .id(("bookmark-marker", line))
                                        .tooltip(move |_, cx| {
                                            icon_tooltip::view("Toggle bookmark", dark, cx)
                                        })
                                        .debug_selector(move || {
                                            if pane == crate::workspace::ViewId::Primary {
                                                format!("bookmark-marker-{line}")
                                            } else {
                                                format!("bookmark-marker-secondary-{line}")
                                            }
                                        })
                                        .size(px(14.))
                                        .child(gpui::img(bookmark_image()).size_full()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(("change-history-line", index))
                            .w(px(9.))
                            .h(row_height)
                            .flex_shrink_0()
                            .relative()
                            .bg(rgb(change_history_colour(
                                "Change History margin",
                                self.dark,
                                if self.dark { 0x0c0c0c } else { 0xe0e0e0 },
                            )))
                            .children((1u8..=4).filter_map(|edition| {
                                if changes & (1 << (edition - 1)) == 0 {
                                    return None;
                                }
                                let (colour, name) = change_marker_style(edition, self.dark);
                                Some(
                                    div()
                                        .id(("change-marker", usize::from(edition)))
                                        .debug_selector(move || {
                                            let name = format!(
                                                "change-marker-{name}-{}-{line}",
                                                pane as usize
                                            );
                                            if first {
                                                name
                                            } else {
                                                format!("{name}-wrap-{index}")
                                            }
                                        })
                                        .role(Role::Image)
                                        .aria_label(format!(
                                            "{name} change history on line {}",
                                            line + 1
                                        ))
                                        .absolute()
                                        .left(px(3.))
                                        .top(px(0.))
                                        .w(px(3.))
                                        .h(row_height)
                                        .bg(rgb(colour)),
                                )
                            })),
                    )
                    .child(
                        div()
                            .w(self.editor_fold_margin_width())
                            .h(row_height)
                            .flex_shrink_0()
                            .when(first && fold_headers.contains(&line), |margin| {
                                let collapsed = self.editor_fold_collapsed(line);
                                margin.child(
                                    div()
                                        .id(("fold-line", line))
                                        .debug_selector(move || {
                                            if pane == crate::workspace::ViewId::Primary {
                                                format!("fold-line-{line}")
                                            } else {
                                                format!("fold-line-secondary-{line}")
                                            }
                                        })
                                        .role(Role::Button)
                                        .aria_label(format!(
                                            "{} fold on line {}",
                                            if collapsed { "Expand" } else { "Collapse" },
                                            line + 1
                                        ))
                                        .tooltip(move |_, cx| {
                                            icon_tooltip::view(
                                                if collapsed {
                                                    "Expand fold"
                                                } else {
                                                    "Collapse fold"
                                                },
                                                dark,
                                                cx,
                                            )
                                        })
                                        .w(px(14.))
                                        .h(row_height)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            if view.focus_editor_pane(pane, window, cx)
                                                && view.workspace.active().id() == owner
                                                && !view.batch_editing_blocked()
                                            {
                                                view.toggle_editor_fold(line, window, cx);
                                            }
                                            cx.stop_propagation();
                                        }))
                                        .child(if collapsed { "+" } else { "−" }),
                                )
                            }),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("editor-gutter")
            .debug_selector(move || {
                if pane == crate::workspace::ViewId::Primary {
                    "editor-gutter".into()
                } else {
                    "editor-gutter-secondary".into()
                }
            })
            .role(Role::Group)
            .aria_label("Line numbers, bookmarks and change history")
            .absolute()
            .left(px(editor_surface::GUTTER_LEFT))
            .top(px(0.))
            .bottom(px(editor_surface::SCROLLBAR_SIZE))
            .w(self.editor_gutter_width())
            .pt(px(editor_surface::TEXT_TOP))
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(if self.dark { 0x303030 } else { 0xe4e4e4 }))
            .text_color(rgb(if self.dark { 0xb0b0b0 } else { 0x808080 }))
            .text_size(self.editor_font_size())
            .children(rows)
            .into_any_element()
    }
}
