use super::*;
use crate::text_transforms::session::apply_command;
pub(super) use crate::text_transforms::session::comment_syntax;
use crate::text_transforms::{TransformCommand, TransformOptions};

fn native_locale_keys(session: &EditorSession) -> Vec<Vec<u8>> {
    use objc2_foundation::{
        NSComparisonResult, NSLocale, NSRange, NSString, NSStringCompareOptions,
    };
    use ropey::LineType;
    let snapshot = session.display_snapshot();
    let selections = session.display_selections();
    let locale = NSLocale::currentLocale();
    let values: Vec<_> = (0..snapshot.len_lines(LineType::LF_CR))
        .map(|row| {
            let start = snapshot.line_to_byte_idx(row, LineType::LF_CR);
            let raw = snapshot.line(row, LineType::LF_CR).to_string();
            let content = raw.trim_end_matches(['\r', '\n']);
            let key = if selections.is_rectangular() {
                selections
                    .ranges()
                    .iter()
                    .find(|range| {
                        snapshot.byte_to_line_idx(range.selection.range().start.0, LineType::LF_CR)
                            == row
                    })
                    .map_or("", |range| {
                        let selected = range.selection.range();
                        let from = selected.start.0.saturating_sub(start).min(content.len());
                        let to = if range.is_empty() {
                            content.len()
                        } else {
                            selected.end.0.saturating_sub(start).min(content.len())
                        };
                        &content[from..to]
                    })
            } else {
                content
            };
            NSString::from_str(key)
        })
        .collect();
    let compare = |a: usize, b: usize| {
        let result = unsafe {
            values[a].compare_options_range_locale(
                &values[b],
                NSStringCompareOptions::CaseInsensitiveSearch
                    | NSStringCompareOptions::NumericSearch,
                NSRange::new(0, values[a].length()),
                Some(&locale),
            )
        };
        if result == NSComparisonResult::Ascending {
            std::cmp::Ordering::Less
        } else if result == NSComparisonResult::Descending {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    };
    let mut order: Vec<_> = (0..values.len()).collect();
    order.sort_by(|&a, &b| compare(a, b));
    let mut keys = vec![Vec::new(); values.len()];
    let mut rank = 0u64;
    for (slot, &index) in order.iter().enumerate() {
        if slot > 0 && compare(order[slot - 1], index) != std::cmp::Ordering::Equal {
            rank += 1;
        }
        keys[index] = rank.to_be_bytes().to_vec();
    }
    keys
}

impl MatrixWindow {
    pub(super) fn text_transform_command(
        &mut self,
        id: u32,
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
        let Some(command) = TransformCommand::from_source_id(id) else {
            return;
        };
        if let Some(step) = crate::macros::MacroStep::text_transform(id) {
            self.record_macro_step(step);
        }
        let language = self.workspace.active().language();
        let definition = match language {
            crate::syntax::Language::UserDefined(id) => self.udl.registry.definition(id),
            _ => None,
        };
        let mut options = TransformOptions::default();
        if matches!(command, TransformCommand::Comment(_)) {
            match comment_syntax(language, definition) {
                Ok(syntax) => options.comment_syntax = syntax,
                Err(error) => {
                    self.notify_editor_error(error, cx);
                    return;
                }
            }
        }
        if command == TransformCommand::SplitLines {
            options.split_breaks = self.editor_measured_split_breaks(window);
        }
        if matches!(
            command,
            TransformCommand::Sort {
                kind: crate::text_transforms::Sort::Locale,
                ..
            }
        ) {
            options.locale_keys = Some(native_locale_keys(self.editor()));
        }
        options.random_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |duration| duration.as_nanos() as u64);
        match apply_command(self.editor_mut(), id, options) {
            Ok(()) => self.editor_changed(Ok(()), window, cx),
            Err(error) => self.notify_editor_error(error, cx),
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }
}

#[cfg(all(
    test,
    target_os = "macos",
    feature = "native-input-tests",
    debug_assertions
))]
mod tests {
    use super::*;

    #[gpui::test]
    fn native_dispatch_executes_every_retained_id_through_the_surface(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Builtin(3));
            for &(name, id, _) in crate::text_transforms::RETAINED_COMMANDS {
                let original = " 2 \n 1 \n 1 \n\n";
                *view.editor_mut() = EditorSession::new(original);
                view.editor_mut()
                    .select_display(Selection {
                        anchor: ByteOffset(0),
                        head: ByteOffset(original.len()),
                    })
                    .unwrap();
                view.editor_error = None;
                view.text_transform_command(id, window, cx);
                assert!(
                    view.editor_error.is_none(),
                    "{name}: {:?}",
                    view.editor_error
                );
                if view.editor().display_text() != original {
                    assert!(view.editor_mut().undo(), "{name}");
                    assert_eq!(view.editor().display_text(), original, "{name}");
                    assert!(!view.editor_mut().undo(), "{name}");
                }
            }
        });
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("one\ntwo");
        });
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorTab);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "\tone\n\ttwo")
        });
        cx.dispatch_action(EditorBackTab);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one\ntwo")
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "\tone\n\ttwo")
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one\ntwo")
        });
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("    one");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::NormalText);
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(7),
                    head: ByteOffset(7),
                })
                .expect("line end");
        });
        cx.dispatch_action(EditorEnter);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "    one\r\n\t")
        });
        cx.simulate_input("x");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "    one\r\n\tx")
        });
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("{}");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Builtin(3));
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(1),
                    head: ByteOffset(1),
                })
                .expect("between braces");
        });
        cx.dispatch_action(EditorEnter);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "{\r\n\t\r\n}")
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "{}")
        });
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("if x:");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Builtin(22));
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(5),
                    head: ByteOffset(5),
                })
                .expect("after colon");
        });
        cx.dispatch_action(EditorEnter);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "if x:\r\n\t")
        });
        view.update_in(cx, |view, _, _| {
            *view.editor_mut() = EditorSession::new("中");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Builtin(3));
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(3),
                    head: ByteOffset(3),
                })
                .expect("Unicode line end");
        });
        cx.simulate_input("{");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "中{")
        });
    }

    #[gpui::test]
    fn native_locale_keys_use_source_numeric_and_case_insensitive_defaults(
        _cx: &mut gpui::TestAppContext,
    ) {
        let session = EditorSession::new("item10\nitem2\nITEM2\né");
        let keys = native_locale_keys(&session);
        assert!(keys[1] < keys[0]);
        assert_eq!(keys[1], keys[2]);
    }
}
