use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::input::EditorSession;

fn caret(position: usize) -> Selection {
    Selection {
        anchor: ByteOffset(position),
        head: ByteOffset(position),
    }
}

#[test]
fn direct_history_follows_adjacent_action_policy_and_save_navigation_boundaries() {
    let mut editor = EditorSession::new("");
    editor.type_text("ab").unwrap();
    editor.select(caret(2)).unwrap();
    editor.type_text("c").unwrap();
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "");
    editor.type_text("ab").unwrap();
    let saved = editor.save_snapshot();
    editor.acknowledge_save(&saved).unwrap();
    editor.type_text("cd").unwrap();
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab");
    assert!(!editor.is_modified());
    editor.type_text("XY").unwrap();
    editor.backspace().unwrap();
    editor.backspace().unwrap();
    assert_eq!(editor.committed_text(), "ab");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "abXY");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab");
}

#[test]
fn editing_after_undo_discards_only_the_abandoned_redo_branch()
-> Result<(), notepad_matrix::editor::EditError> {
    let mut editor = EditorSession::new("a");
    editor.select(caret(1))?;
    let saved = editor.save_snapshot();
    editor.acknowledge_save(&saved)?;
    editor.type_text("中")?;
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a");
    assert!(!editor.is_modified());
    editor.type_text("文")?;
    assert_eq!(editor.committed_text(), "a文");
    assert!(editor.is_modified());
    assert!(!editor.redo());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a");
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "a文");
    assert!(!editor.redo());
    Ok(())
}

#[test]
fn removal_coalescing_uses_source_byte_lengths_not_timer_or_grapheme_guesses() {
    let mut editor = EditorSession::new("Aé中😀");
    editor.select(caret(10)).unwrap();
    editor.backspace().unwrap();
    editor.backspace().unwrap();
    editor.backspace().unwrap();
    assert_eq!(editor.committed_text(), "A");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "Aé中");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "Aé中😀");
    assert!(!editor.undo());
}

#[test]
fn typing_over_a_virtual_selection_ends_the_source_selection_undo_group() {
    use notepad_matrix::editor::{SelectionRange, SelectionSet};
    let mut editor = EditorSession::new("x");
    editor
        .set_selections(
            SelectionSet::new(
                vec![SelectionRange {
                    selection: caret(1),
                    anchor_virtual: 2,
                    head_virtual: 4,
                }],
                0,
            )
            .unwrap(),
        )
        .unwrap();
    editor.type_text("A").unwrap();
    editor.type_text("B").unwrap();
    assert_eq!(editor.committed_text(), "x  AB");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "x  A");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "x");
}
