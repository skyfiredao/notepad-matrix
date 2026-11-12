use super::*;
use crate::clipboard::{ClipboardKind, ClipboardText};
use crate::editor::EditError;

pub(super) fn has_text(cx: &App) -> bool {
    cx.read_from_clipboard().is_some_and(|item| {
        item.entries
            .iter()
            .any(|entry| matches!(entry, gpui::ClipboardEntry::String(_)))
    })
}

pub(super) fn read_text(cx: &App) -> Option<ClipboardText> {
    let payload = read_payload(cx)?;
    Some(ClipboardText {
        text: payload.text,
        line_copy: payload.line_copy,
    })
}

fn read_payload(cx: &App) -> Option<crate::input::SelectionClipboard> {
    let item = cx.read_from_clipboard()?;
    let mut text = String::new();
    let mut found = false;
    for entry in &item.entries {
        if let gpui::ClipboardEntry::String(value) = entry {
            text.push_str(value.text());
            found = true;
        }
    }
    let kind = ClipboardKind::from_metadata(item.metadata().map(String::as_str));
    found.then(|| crate::input::SelectionClipboard {
        text,
        line_copy: kind == ClipboardKind::Line,
        rectangular: kind == ClipboardKind::Rectangle,
    })
}

impl MatrixWindow {
    pub(super) fn binary_clipboard(
        &mut self,
        command: crate::clipboard::BinaryCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::clipboard::{BINARY_METADATA, BinaryCommand};
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.binary_clipboard(command, window, cx)
        }) {
            return;
        }
        if !self.clipboard_ready() {
            return;
        }
        if command == BinaryCommand::Paste {
            if !cx.is_test()
                && let Some(payload) = crate::native_binary_clipboard::read(
                    &objc2_app_kit::NSPasteboard::generalPasteboard(),
                )
                && let Ok(text) = String::from_utf8(payload.bytes)
            {
                self.paste_binary_text(text, payload.recorded_length, window, cx);
                return;
            }
            let Some(item) = cx.read_from_clipboard() else {
                return;
            };
            let text = item
                .entries()
                .iter()
                .filter_map(|entry| {
                    if let gpui::ClipboardEntry::String(value) = entry {
                        Some(value.text().as_str())
                    } else {
                        None
                    }
                })
                .collect::<String>();
            if !item
                .entries()
                .iter()
                .any(|entry| matches!(entry, gpui::ClipboardEntry::String(_)))
            {
                return;
            }
            let recorded_length = item
                .metadata()
                .is_some_and(|metadata| metadata == BINARY_METADATA);
            let text = crate::clipboard::binary_paste_text(&text, recorded_length).to_owned();
            self.paste_binary_text(text, recorded_length, window, cx);
        } else {
            if command == BinaryCommand::Cut && self.editor().is_read_only() {
                self.editor_changed(Err(EditError::ReadOnly), window, cx);
                return;
            }
            let bytes = match crate::clipboard::binary_selection(self.editor_mut()) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => return,
                Err(error) => {
                    self.editor_changed(Err(error), window, cx);
                    return;
                }
            };
            if let Ok(text) = std::str::from_utf8(&bytes) {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_metadata(
                    text.to_owned(),
                    BINARY_METADATA.into(),
                ));
            }
            if !cx.is_test()
                && let Err(error) = crate::native_binary_clipboard::write(
                    &objc2_app_kit::NSPasteboard::generalPasteboard(),
                    &bytes,
                )
            {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
            if command == BinaryCommand::Cut {
                self.insert_character_text(String::new(), window, cx);
            } else {
                self.editor_focus.focus(window, cx);
                cx.notify();
            }
        }
    }

    fn paste_binary_text(
        &mut self,
        text: String,
        recorded_length: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !recorded_length {
            self.editor_mut().begin_undo_action();
        }
        self.insert_character_text(text, window, cx);
        if !recorded_length {
            self.editor_mut().end_undo_action();
        }
    }

    pub(super) fn copy_document_path(
        &mut self,
        command: crate::clipboard::CopyPath,
        cx: &mut Context<Self>,
    ) {
        self.record_macro_step(crate::macros::MacroStep::CopyPath(command));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            command.text(&self.workspace),
        ));
    }

    fn clipboard_ready(&self) -> bool {
        !self.file_busy
            && !self.close_dialog
            && !self.save_all_dialog
            && !self.batch_editing_blocked()
    }

    pub(super) fn copy_editor(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clipboard_ready() {
            cx.stop_propagation();
            return;
        }
        if cut && self.editor().document().is_read_only() {
            self.editor_changed(Err(EditError::ReadOnly), window, cx);
            cx.stop_propagation();
            return;
        }
        let payload = self.editor().clipboard_payload();
        if cut && let Err(error) = self.editor_mut().cut_selection() {
            self.editor_changed(Err(error), window, cx);
            cx.stop_propagation();
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Copy { cut });
        let kind = if payload.rectangular {
            ClipboardKind::Rectangle
        } else if payload.line_copy {
            ClipboardKind::Line
        } else {
            ClipboardKind::Stream
        };
        let item = if let Some(metadata) = kind.metadata() {
            gpui::ClipboardItem::new_string_with_metadata(payload.text, metadata.into())
        } else {
            gpui::ClipboardItem::new_string(payload.text)
        };
        cx.write_to_clipboard(item);
        if cut {
            self.editor_changed(Ok(()), window, cx);
        }
        self.editor_focus.focus(window, cx);
        self.macro_step_pending = false;
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn paste_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clipboard_ready()
            && let Some(payload) = read_payload(cx)
        {
            self.record_macro_step(crate::macros::MacroStep::Paste);
            let result = self.editor_mut().paste_payload(&payload, 4);
            self.editor_changed(result, window, cx);
            self.editor_focus.focus(window, cx);
        }
        cx.stop_propagation();
    }
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;
    use crate::editor::{SelectionRange, SelectionSet};

    #[gpui::test]
    fn binary_copy_and_cut_menu_keep_source_primary_bytes_and_zero_padding(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        for cut in [false, true] {
            view.update_in(cx, |view, window, cx| {
                *view.editor_mut() = EditorSession::from_file_text("ab\ncd");
                view.editor_mut()
                    .set_selections(
                        SelectionSet::new(
                            vec![
                                Selection {
                                    anchor: ByteOffset(0),
                                    head: ByteOffset(1),
                                }
                                .into(),
                                Selection {
                                    anchor: ByteOffset(3),
                                    head: ByteOffset(4),
                                }
                                .into(),
                            ],
                            1,
                        )
                        .unwrap(),
                    )
                    .unwrap();
                view.menu_input(MenuEvent::OpenSubmenu("edit-paste-special"), window, cx);
                view.menu_input(MenuEvent::First, window, cx);
                if cut {
                    view.menu_input(MenuEvent::Down, window, cx);
                }
                view.menu_input(MenuEvent::Activate, window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("c\0\0\0")
                );
                assert_eq!(
                    view.editor().committed_text(),
                    if cut { "b\nd" } else { "ab\ncd" }
                );
                if cut {
                    assert!(view.editor_mut().undo());
                    assert_eq!(view.editor().committed_text(), "ab\ncd");
                }
                assert!(!view.editor().document().can_undo());
                *view.editor_mut() = EditorSession::new("");
                view.menu_input(MenuEvent::OpenSubmenu("edit-paste-special"), window, cx);
                view.menu_input(MenuEvent::Last, window, cx);
                view.menu_input(MenuEvent::Activate, window, cx);
                assert_eq!(view.editor().committed_text(), "c\0\0\0");
            });
        }
    }

    #[gpui::test]
    fn binary_paste_menu_uses_source_undo_branch_for_external_and_length_marked_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        for recorded_length in [false, true] {
            view.update_in(cx, |view, _, cx| {
                *view.editor_mut() = EditorSession::new("abcd");
                view.editor_mut()
                    .select(Selection {
                        anchor: ByteOffset(1),
                        head: ByteOffset(3),
                    })
                    .unwrap();
                let text = "X\r\n\0Y".to_owned();
                let item = if recorded_length {
                    gpui::ClipboardItem::new_string_with_metadata(
                        text,
                        crate::clipboard::BINARY_METADATA.into(),
                    )
                } else {
                    gpui::ClipboardItem::new_string(text)
                };
                cx.write_to_clipboard(item);
            });
            view.update_in(cx, |view, window, cx| {
                view.menu_input(MenuEvent::OpenSubmenu("edit-paste-special"), window, cx);
                view.menu_input(MenuEvent::Last, window, cx);
                view.menu_input(MenuEvent::Activate, window, cx);
                assert_eq!(
                    view.editor().committed_text(),
                    if recorded_length {
                        "aX\r\n\0Yd"
                    } else {
                        "aX\r\nd"
                    }
                );
            });
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor().committed_text(),
                    if recorded_length { "ad" } else { "abcd" }
                );
            });
            if recorded_length {
                cx.dispatch_action(EditorUndo);
            }
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "abcd");
                assert!(!view.editor().document().can_undo());
            });
        }
    }

    #[gpui::test]
    fn multi_stream_cut_publishes_source_rectangle_shape_and_undoes_all_targets(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("ab\ncd");
            let ranges: Vec<SelectionRange> = vec![
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(1),
                }
                .into(),
                Selection {
                    anchor: ByteOffset(3),
                    head: ByteOffset(4),
                }
                .into(),
            ];
            view.editor_mut()
                .set_selections(SelectionSet::new(ranges, 1).unwrap())
                .unwrap();
            assert!(!view.editor().display_selections().is_rectangular());
            view.copy_editor(true, window, cx);
            let payload = read_payload(cx).unwrap();
            assert!(payload.rectangular);
            assert!(!payload.line_copy);
            assert_eq!(payload.text, "a\r\nc\r\n");
            assert_eq!(view.editor().display_text(), "b\nd");
            assert!(view.editor_mut().undo());
            assert_eq!(view.editor().display_text(), "ab\ncd");
            assert!(!view.editor_mut().undo());
            *view.editor_mut() = EditorSession::new("a\0b\n");
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(4),
                })
                .expect("binary source selection");
            view.binary_clipboard(crate::clipboard::BinaryCommand::Copy, window, cx);
            assert_eq!(
                cx.read_from_clipboard()
                    .expect("binary clipboard")
                    .metadata()
                    .map(String::as_str),
                Some(crate::clipboard::BINARY_METADATA)
            );
            *view.editor_mut() = EditorSession::new("");
            view.binary_clipboard(crate::clipboard::BinaryCommand::Paste, window, cx);
            assert_eq!(view.editor().committed_text(), "a\0b\n");
            assert!(view.editor_mut().undo());
            assert_eq!(view.editor().committed_text(), "");
        });
    }

    #[gpui::test]
    fn readonly_cut_does_not_change_clipboard_or_any_selection(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("ab\ncd");
            view.editor_mut().set_read_only(true);
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()));
            let before = view.editor().display_selections();
            view.copy_editor(true, window, cx);
            assert_eq!(view.editor().display_text(), "ab\ncd");
            assert_eq!(view.editor().display_selections(), before);
            assert_eq!(read_payload(cx).unwrap().text, "sentinel");
        });
    }

    #[gpui::test]
    fn rectangle_marker_pastes_by_rows_without_changing_external_plain_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("zz\nzz");
            view.editor_mut()
                .select_display(Selection {
                    anchor: ByteOffset(1),
                    head: ByteOffset(1),
                })
                .unwrap();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_metadata(
                "a\r\nc\r\n".into(),
                ClipboardKind::Rectangle.metadata().unwrap().into(),
            ));
            view.paste_editor(window, cx);
            assert_eq!(view.editor().display_text(), "zaz\nzcz");
            assert!(view.editor_mut().undo());
            assert_eq!(view.editor().display_text(), "zz\nzz");
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("a\nc".into()));
            assert!(!read_payload(cx).unwrap().rectangular);
            view.paste_editor(window, cx);
            assert_eq!(view.editor().display_text(), "za\r\ncz\nzz");
        });
    }
}
