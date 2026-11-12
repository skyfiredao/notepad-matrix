use notepad_matrix::editor::{ByteOffset, Selection, SelectionSet};
use notepad_matrix::input::EditorSession;

fn selection(anchor: usize, head: usize) -> Selection {
    Selection {
        anchor: ByteOffset(anchor),
        head: ByteOffset(head),
    }
}

#[test]
fn inline_ime_previews_every_caret_overstrikes_scalars_and_cancels_without_history() {
    let mut editor = EditorSession::new("a😀z\nb中q");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(1, 1).into(), selection(8, 8).into()], 1).unwrap(),
        )
        .unwrap();
    let original = editor.selections();
    let peer = editor.clone_view();
    editor.set_overwrite(true);
    editor.mark_inline_composition("文", Some(0..1)).unwrap();
    assert_eq!(editor.display_text(), "a文z\nb文q");
    assert_eq!(editor.committed_text(), "a😀z\nb中q");
    assert_eq!(peer.display_text(), "a😀z\nb中q");
    assert_eq!(editor.selections().ranges()[0].selection, selection(1, 4));
    assert_eq!(editor.selection(), selection(7, 10));
    assert!(!editor.is_modified());
    editor.cancel_composition();
    assert_eq!(editor.selections(), original);
    assert!(!editor.undo());
    editor.mark_inline_composition("文", None).unwrap();
    editor.insert_text("🦀中").unwrap();
    assert_eq!(editor.committed_text(), "a🦀中\nb🦀中");
    assert!(!editor.has_composition());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a😀z\nb中q");
    assert_eq!(editor.selections(), original);
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "a🦀中\nb🦀中");
}

#[test]
fn inline_marked_ranges_validate_surrogates_preserve_redo_and_commit_selected_preclear() {
    let mut editor = EditorSession::new("abc\r\ndef");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(2, 1).into(), selection(6, 7).into()], 1).unwrap(),
        )
        .unwrap();
    editor.set_overwrite(true);
    editor.mark_inline_composition("😀", Some(0..2)).unwrap();
    assert_eq!(editor.display_text(), "a😀\r\nd😀");
    assert_eq!(
        editor.marked_ranges(),
        vec![ByteOffset(1)..ByteOffset(5), ByteOffset(8)..ByteOffset(12)]
    );
    let revision = editor.display_revision();
    let selections = editor.selections();
    assert!(editor.mark_inline_composition("😀", Some(1..2)).is_err());
    assert_eq!(editor.display_revision(), revision);
    assert_eq!(editor.selections(), selections);
    editor.commit_composition().unwrap();
    assert_eq!(editor.committed_text(), "a😀\r\nd😀");
    assert_eq!(editor.selection(), selection(8, 12));
    assert!(editor.undo());
    editor.mark_inline_composition("中", None).unwrap();
    editor.cancel_composition();
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "a😀\r\nd😀");
}

#[test]
fn inline_partial_updates_and_confirmation_replace_only_the_marked_subspan_at_each_caret() {
    let mut editor = EditorSession::new("Z\nQ");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(0, 0).into(), selection(2, 2).into()], 1).unwrap(),
        )
        .unwrap();
    editor.mark_inline_composition("abcd", None).unwrap();
    editor
        .mark_display_range(Some(ByteOffset(7)..ByteOffset(8)), "X", None)
        .unwrap();
    assert_eq!(editor.display_text(), "aXcdZ\naXcdQ");
    assert_eq!(
        editor.marked_ranges(),
        vec![ByteOffset(1)..ByteOffset(2), ByteOffset(7)..ByteOffset(8)]
    );
    editor.insert_text("Y").unwrap();
    assert_eq!(editor.committed_text(), "aYcdZ\naYcdQ");
    assert_eq!(editor.selections().ranges()[0].selection, selection(2, 2));
    assert_eq!(editor.selection(), selection(8, 8));
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "Z\nQ");
    assert!(!editor.undo());
}

#[test]
fn inline_virtual_eol_peer_invalidation_and_readonly_cancel_do_not_resurrect_preview() {
    use notepad_matrix::editor::{ColumnPosition, EditError};
    let mut editor = EditorSession::new("x\r\ny");
    editor
        .select_rectangle(
            ColumnPosition { line: 0, column: 3 },
            ColumnPosition { line: 1, column: 3 },
            4,
        )
        .unwrap();
    let original = editor.selections();
    editor.set_overwrite(true);
    editor.mark_inline_composition("e\u{301}", None).unwrap();
    assert_eq!(editor.display_text(), "x  e\u{301}\r\ny  e\u{301}");
    assert_eq!(editor.display_selections().primary_index(), 1);
    editor.cancel_composition();
    assert_eq!(editor.selections(), original);
    editor.mark_inline_composition("😀", None).unwrap();
    let mut peer = editor.clone_view();
    peer.select(selection(0, 0)).unwrap();
    peer.insert_text("Q").unwrap();
    assert!(!editor.has_composition());
    assert!(editor.marked_ranges().is_empty());
    editor.commit_composition().unwrap();
    assert_eq!(editor.committed_text(), "Qx\r\ny");
    editor.mark_inline_composition("文", None).unwrap();
    peer.set_read_only(true);
    assert!(!editor.has_composition());
    assert!(matches!(
        editor.mark_inline_composition("文", None),
        Err(EditError::ReadOnly)
    ));
    peer.set_read_only(false);
    assert!(!editor.has_composition());
    assert_eq!(editor.display_text(), "Qx\r\ny");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "x\r\ny");
    assert!(!editor.undo());
}

#[test]
fn explicit_native_range_changes_only_primary_target_without_changing_cancel_or_undo_baseline() {
    let mut editor = EditorSession::new("abc def xyz");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(0, 0).into(), selection(5, 5).into()], 1).unwrap(),
        )
        .unwrap();
    let original = editor.selections();
    editor
        .mark_inline_display_range(Some(ByteOffset(4)..ByteOffset(7)), "Q", None)
        .unwrap();
    assert_eq!(editor.display_text(), "Qabc Q xyz");
    editor.cancel_composition();
    assert_eq!(editor.selections(), original);
    editor
        .mark_inline_display_range(Some(ByteOffset(4)..ByteOffset(7)), "Q", None)
        .unwrap();
    editor.insert_text("😀").unwrap();
    assert_eq!(editor.committed_text(), "😀abc 😀 xyz");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "abc def xyz");
    assert_eq!(editor.selections(), original);
}

#[test]
fn inline_copy_reads_all_selected_preview_ranges_without_committing() {
    let mut editor = EditorSession::new("a\nb");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(0, 0).into(), selection(2, 2).into()], 1).unwrap(),
        )
        .unwrap();
    editor.mark_inline_composition("文", Some(0..1)).unwrap();
    let payload = editor.clipboard_payload();
    assert_eq!(payload.text, "文\r\n文\r\n");
    assert!(payload.rectangular);
    assert_eq!(editor.committed_text(), "a\nb");
    assert!(!editor.document().can_undo());
    assert!(editor.has_composition());
}
