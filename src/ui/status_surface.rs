use super::*;

#[derive(Clone, Copy)]
pub(super) enum StatusMenu {
    Language,
    Eol,
    Encoding,
}

impl MatrixWindow {
    pub(super) fn open_status_menu(
        &mut self,
        target: StatusMenu,
        position: gpui::Point<gpui::Pixels>,
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
        let event = match target {
            StatusMenu::Eol => MenuEvent::OpenSubmenu("edit-menu-eol"),
            StatusMenu::Language | StatusMenu::Encoding => {
                let selector = match target {
                    StatusMenu::Language => "language-menu-trigger",
                    _ => "encoding-menu-trigger",
                };
                let Some(root) = self
                    .menu_bar
                    .definitions()
                    .iter()
                    .position(|definition| definition.selector == selector)
                else {
                    return;
                };
                MenuEvent::OpenRoot(root)
            }
        };
        self.menu_input(event, window, cx);
        if self.menu_bar.mode() == MenuMode::Open {
            self.menu_anchor = Some(position);
            self.menu_submenu_only = matches!(target, StatusMenu::Eol);
            cx.notify();
        }
    }

    pub(super) fn render_status(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = |value: usize| {
            let digits = value.to_string();
            let mut result = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    result.push(',');
                }
                result.push(digit);
            }
            result
        };
        let snapshot = self.editor().display_snapshot();
        let (size_text, position_text) =
            match crate::status::measure(&snapshot, self.editor().selection(), 4) {
                Ok(status) => {
                    let selection = if status.selected_chars == 0 {
                        format!("位置: {}", count(status.position))
                    } else {
                        format!(
                            "选中: {} | {}",
                            count(status.selected_chars),
                            count(status.selected_lines)
                        )
                    };
                    (
                        format!(
                            "长度: {}    行数: {}",
                            count(status.bytes),
                            count(status.lines)
                        ),
                        format!(
                            "行: {}    列: {}    {}",
                            count(status.line),
                            count(status.column),
                            selection
                        ),
                    )
                }
                Err(error) => ("Status unavailable".into(), error.to_string()),
            };
        let format = match self.workspace.active().format() {
            crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain) => "UTF-8",
            crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Bom) => "UTF-8-BOM",
            crate::files::UnicodeFormat::Utf16LeBom => "UTF-16 LE BOM",
            crate::files::UnicodeFormat::Utf16BeBom => "UTF-16 BE BOM",
        };
        let eol = match self.editor().line_ending() {
            crate::input::LineEnding::CrLf => "Windows (CR LF)",
            crate::input::LineEnding::Lf => "Unix (LF)",
            crate::input::LineEnding::Cr => "Macintosh (CR)",
        };
        let typing_mode = if self.editor().overwrite() {
            "OVR"
        } else {
            "INS"
        };
        let panel = |id: &'static str,
                     label: String,
                     text: String,
                     action: Option<Box<dyn gpui::Action>>| {
            let interactive = action.is_some();
            let context_menu = match id {
                "status-language" => Some(StatusMenu::Language),
                "status-eol" => Some(StatusMenu::Eol),
                "status-encoding" => Some(StatusMenu::Encoding),
                _ => None,
            };
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Status)
                .aria_label(label)
                .when(interactive, |panel| {
                    panel.aria_description(if id == "status-insert-mode" {
                        "Click to toggle insert/overwrite mode"
                    } else {
                        "Double-click to open"
                    })
                })
                .flex_shrink_0()
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .px(px(2.))
                .when(id == "status-language", |panel| {
                    panel.flex_basis(px(0.)).flex_grow(1.).min_w(px(0.))
                })
                .when(id == "status-document-size", |panel| panel.w(px(220.)))
                .when(id == "status-position", |panel| panel.w(px(260.)))
                .when(id == "status-eol", |panel| panel.w(px(110.)))
                .when(id == "status-encoding", |panel| panel.w(px(120.)))
                .when(id == "status-insert-mode", |panel| panel.w(px(45.)))
                .border_l_1()
                .border_color(rgb(if self.dark { 0x646464 } else { 0xa0a0a0 }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseUpEvent, window, cx| {
                        if let Some(target) = context_menu {
                            view.open_status_menu(target, event.position, window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_click(
                    cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                        let clicks = if id == "status-insert-mode" { 1 } else { 2 };
                        if event.click_count() == clicks {
                            if let Some(target) = context_menu {
                                view.open_status_menu(target, event.position(), window, cx);
                            } else if let Some(action) = &action {
                                window.dispatch_action(action.boxed_clone(), cx);
                            }
                        }
                        cx.stop_propagation();
                    }),
                )
                .child(text)
        };
        div()
            .id("status-bar")
            .debug_selector(|| "status-bar".into())
            .role(Role::Status)
            .aria_label("Status bar")
            .h(px(24.))
            .flex_shrink_0()
            .flex()
            .justify_end()
            .items_center()
            .overflow_hidden()
            .text_size(px(13.))
            .text_color(rgb(if self.dark { 0xe0e0e0 } else { 0x000000 }))
            .bg(rgb(if self.dark { 0x2b2b2b } else { 0xf0f0f0 }))
            .border_t_1()
            .border_color(rgb(if self.dark { 0x646464 } else { 0xa0a0a0 }))
            .child(panel(
                "status-language",
                self.workspace.active().language().description().into(),
                self.workspace.active().language().description().into(),
                Some(Box::new(OpenLanguageMenu)),
            ))
            .child(panel(
                "status-document-size",
                size_text.clone(),
                size_text,
                Some(Box::new(ShowSummary)),
            ))
            .child(panel(
                "status-position",
                position_text.clone(),
                position_text,
                Some(Box::new(ShowGoTo)),
            ))
            .child(panel(
                "status-eol",
                format!("Line endings: {eol}"),
                eol.into(),
                Some(Box::new(OpenEolMenu)),
            ))
            .child(panel(
                "status-encoding",
                format!("Encoding: {format}"),
                format.into(),
                Some(Box::new(OpenEncodingMenu)),
            ))
            .child(panel(
                "status-insert-mode",
                format!("Typing mode: {typing_mode}"),
                typing_mode.into(),
                Some(Box::new(ToggleOverwrite)),
            ))
            .child(
                div()
                    .id("status-resize-indicator")
                    .w(px(24.))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .items_end()
                    .justify_end()
                    .text_color(rgb(0x909090))
                    .child("◢"),
            )
            .into_any_element()
    }
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;
    use gpui::EntityInputHandler;

    #[gpui::test]
    fn status_context_menus_open_at_pointer_clamp_to_window_and_keep_commands(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(menu_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let fixture = tempfile::tempdir().expect("loaded LF status fixture");
        let path = fixture.path().join("status.txt");
        std::fs::write(&path, "first\nsecond").expect("UTF8 LF source");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "first\nsecond"
        );
        let snapshot = |view: &MatrixWindow| {
            (
                view.workspace.active().id(),
                view.workspace.active_view(),
                view.editor().committed_text(),
                view.editor().selections(),
                view.editor().document().can_undo(),
                view.editor().document().can_redo(),
                view.workspace.active().is_modified(),
                view.workspace.active().language(),
                view.workspace.active().format(),
                view.editor().line_ending(),
                view.editor().overwrite(),
            )
        };
        let mut continuations = String::new();
        for (panel, root_selector, command_selector) in [
            (
                "status-language",
                "language-menu-trigger",
                "language-normal-text",
            ),
            ("status-eol", "edit-menu-trigger", "edit-eol-lf"),
            ("status-encoding", "encoding-menu-trigger", "encoding-utf8"),
        ] {
            for right_click in [true, false] {
                let before = view.read_with(cx, |view, _| snapshot(view));
                let windows = cx.windows();
                let button = if right_click {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                };
                let click_count = if right_click { 1 } else { 2 };
                let pointer = cx.debug_bounds(panel).expect("status panel").center();
                cx.simulate_event(gpui::MouseDownEvent {
                    position: pointer,
                    button,
                    click_count,
                    ..Default::default()
                });
                view.update_in(cx, |view, window, _| {
                    assert!(
                        view.editor_focus.is_focused(window),
                        "status press must preserve editor focus: {panel}, right={right_click}"
                    )
                });
                cx.simulate_event(gpui::MouseUpEvent {
                    position: pointer,
                    button,
                    click_count,
                    ..Default::default()
                });
                cx.run_until_parked();
                let popup = cx.debug_bounds("menu-popup-0").expect("context popup");
                let viewport = view.update_in(cx, |_, window, _| window.viewport_size());
                assert!(
                    (popup.left() - pointer.x.min(viewport.width - popup.size.width)).abs()
                        <= px(1.)
                );
                assert!(
                    (popup.bottom() - viewport.height).abs() <= px(1.),
                    "bottom-clamped popup"
                );
                assert!(
                    cx.debug_bounds(command_selector).is_some(),
                    "source command accessible"
                );
                if panel == "status-eol" && right_click {
                    let current = cx
                        .debug_bounds("edit-eol-lf")
                        .expect("disabled current LF option");
                    cx.simulate_click(current.center(), gpui::Modifiers::default());
                    assert!(
                        cx.debug_bounds("menu-popup-0").is_some(),
                        "inside anchored popup must not be dismissed as outside the menu bar"
                    );
                    view.update_in(cx, |view, window, _| {
                        assert_eq!(snapshot(view), before);
                        assert!(view.menu_focus.is_focused(window));
                    });
                    let toolbar = cx
                        .debug_bounds("editing-toolbar")
                        .expect("actual outside blank region");
                    cx.simulate_click(
                        gpui::point(toolbar.right() - px(1.), toolbar.top() + px(1.)),
                        gpui::Modifiers::default(),
                    );
                    assert!(cx.debug_bounds("menu-popup-0").is_none());
                    view.read_with(cx, |view, _| assert_eq!(snapshot(view), before));
                    cx.simulate_event(gpui::MouseDownEvent {
                        position: pointer,
                        button: MouseButton::Right,
                        click_count: 1,
                        ..Default::default()
                    });
                    cx.simulate_event(gpui::MouseUpEvent {
                        position: pointer,
                        button: MouseButton::Right,
                        click_count: 1,
                        ..Default::default()
                    });
                    cx.run_until_parked();
                }
                view.update_in(cx, |view, _, _| {
                    assert_eq!(view.menu_anchor, Some(pointer));
                    assert_eq!(
                        view.menu_bar.definitions()[view.menu_bar.active_root()].selector,
                        root_selector
                    );
                    assert_eq!(
                        view.menu_bar.popup_count(),
                        if panel == "status-eol" { 2 } else { 1 }
                    );
                });
                let selected = view.update_in(cx, |view, window, _| {
                    assert!(view.menu_focus.is_focused(window));
                    assert_eq!(snapshot(view), before);
                    view.menu_bar.selected_path().to_vec()
                });
                cx.simulate_keystrokes("down");
                view.read_with(cx, |view, _| {
                    if panel == "status-eol" && right_click {
                        assert_eq!(view.editor().line_ending(), crate::input::LineEnding::Lf);
                        assert_eq!(
                            view.menu_bar.selected_path().last(),
                            Some(&2),
                            "source initial-state navigation skips the disabled current LF option"
                        );
                    }
                    assert_ne!(view.menu_bar.selected_path(), selected);
                    assert_eq!(snapshot(view), before, "menu owns Down without conversion");
                });
                cx.simulate_keystrokes("escape");
                cx.run_until_parked();
                view.update_in(cx, |view, window, _| {
                    assert_eq!(
                        view.menu_bar.mode(),
                        MenuMode::Closed,
                        "one Escape dismisses anchored {panel}"
                    );
                    assert!(view.editor_focus.is_focused(window));
                    assert_eq!(snapshot(view), before);
                });
                assert!(cx.debug_bounds("menu-popup-0").is_none());
                assert_eq!(cx.windows(), windows);
                assert!(
                    !cx.did_prompt_for_paths()
                        && !cx.did_prompt_for_new_path()
                        && !cx.has_pending_prompt()
                );
                cx.simulate_input("!");
                continuations.push('!');
                view.read_with(cx, |view, _| {
                    assert_eq!(
                        view.editor().committed_text(),
                        format!("first\nsecond{continuations}")
                    )
                });
            }
        }
        cx.dispatch_action(OpenLanguageMenu);
        cx.run_until_parked();
        view.update_in(cx, |view, _, _| assert_eq!(view.menu_anchor, None));
        let popup = cx
            .debug_bounds("menu-popup-0")
            .expect("normal language menu");
        let menu = cx.debug_bounds("matrix-menu-bar").expect("menu bar");
        assert!(
            (popup.top() - menu.bottom()).abs() <= px(1.),
            "normal top menu remains anchored to bar"
        );
        view.update_in(cx, |view, _, _| {
            assert_eq!(
                view.editor().committed_text(),
                format!("first\nsecond{continuations}")
            )
        });
    }

    #[gpui::test]
    fn status_position_double_click_opens_existing_go_to_without_editing(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", CloseGoTo, Some("MatrixGoTo"))]));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first\nsecond");
        let snapshot = |view: &MatrixWindow| {
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
        let mut continuations = String::new();
        for escape in [true, false] {
            let document = view.read_with(cx, |view, _| snapshot(view));
            let before = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            });
            let document_windows = cx.windows();
            let position = cx
                .debug_bounds("status-position")
                .expect("position panel")
                .center();
            cx.simulate_click(position, gpui::Modifiers::default());
            assert!(
                cx.windows() == document_windows,
                "source uses a double click"
            );
            cx.simulate_event(gpui::MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 2,
                ..Default::default()
            });
            cx.simulate_event(gpui::MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 2,
                ..Default::default()
            });
            cx.run_until_parked();
            let goto = cx
                .windows()
                .into_iter()
                .find(|window| !document_windows.contains(window))
                .expect("Go To must open in its own window");
            let mut goto_cx = gpui::VisualTestContext::from_window(goto, &cx.cx);
            assert_eq!(goto_cx.window_title().as_deref(), Some("Go To..."));
            assert!(goto_cx.debug_bounds("goto-dialog").is_some());
            goto_cx.simulate_input("2");
            view.update_in(&mut goto_cx, |view, window, cx| {
                assert!(view.goto_input.read(cx).is_focused(window));
                assert_eq!(view.goto_input.read(cx).text(), "2");
                assert_eq!(snapshot(view), document);
            });
            if escape {
                goto_cx.simulate_keystrokes("escape");
            } else {
                let cancel = goto_cx
                    .debug_bounds("goto-cancel")
                    .expect("explicit Go To Cancel");
                goto_cx.simulate_click(cancel.center(), gpui::Modifiers::default());
            }
            goto_cx.run_until_parked();
            assert!(!cx.windows().contains(&goto));
            assert_eq!(cx.windows(), document_windows);
            assert_eq!(
                view.update_in(cx, |view, window, cx| {
                    EntityInputHandler::text_length_utf16(view, window, cx)
                }),
                before
            );
            view.update_in(cx, |view, window, _| {
                assert_eq!(snapshot(view), document);
                assert!(view.editor_focus.is_focused(window));
            });
            assert!(
                !cx.did_prompt_for_paths()
                    && !cx.did_prompt_for_new_path()
                    && !cx.has_pending_prompt()
            );
            cx.simulate_input("!");
            continuations.push('!');
            view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor().committed_text(),
                    format!("first\nsecond{continuations}")
                )
            });
        }
        let document = view.read_with(cx, |view, _| snapshot(view));
        let windows = cx.windows();
        let position = cx
            .debug_bounds("status-document-size")
            .expect("Summary status field")
            .center();
        cx.simulate_click(position, gpui::Modifiers::default());
        assert!(!cx.has_pending_prompt());
        assert_eq!(cx.windows(), windows);
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        let (title, detail) = cx
            .pending_prompt()
            .expect("Summary must use a native modal alert");
        assert_eq!(title, "Summary");
        assert!(detail.contains("Lines: 2\nDocument length: 14"));
        assert_eq!(
            cx.windows(),
            windows,
            "no duplicate modeless Summary window"
        );
        view.read_with(cx, |view, _| assert_eq!(snapshot(view), document));
        assert!(!cx.did_prompt_for_paths() && !cx.did_prompt_for_new_path());
        cx.simulate_prompt_answer("OK");
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt());
        view.update_in(cx, |view, window, _| {
            assert_eq!(snapshot(view), document);
            assert!(!view.summary_visible);
            assert!(view.editor_focus.is_focused(window));
        });
        cx.simulate_input("!");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "first\nsecond!!!")
        });
    }
}
