use notepad_matrix::editor::{ByteOffset, EditError, Selection};
use notepad_matrix::input::EditorSession;

fn selection(anchor: usize, head: usize) -> Selection {
    Selection {
        anchor: ByteOffset(anchor),
        head: ByteOffset(head),
    }
}

#[test]
fn overwrite_mode_is_session_state_not_an_edit() -> Result<(), EditError> {
    let mut editor = EditorSession::new("abc");
    assert!(!editor.overwrite());
    editor.insert_text("X")?;
    assert!(editor.undo());
    let revision = editor.display_revision();
    let selection = editor.selection();
    editor.set_read_only(true);
    editor.set_overwrite(true);
    assert!(editor.overwrite());
    editor.toggle_overwrite();
    assert!(!editor.overwrite());
    editor.toggle_overwrite();
    assert!(editor.overwrite());
    assert!(editor.document().is_read_only());
    assert_eq!(editor.committed_text(), "abc");
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.display_revision(), revision);
    assert!(!editor.is_modified());
    editor.set_read_only(false);
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "Xabc");
    assert!(editor.overwrite());
    assert!(editor.undo());
    assert!(!editor.undo());
    Ok(())
}

#[test]
fn native_typing_matches_insert_character_and_add_wstring_scalar_vectors() -> Result<(), EditError>
{
    for (original, anchor, head, typed, expected, caret) in [
        ("abc", 1, 1, "X", "aXc", 2),
        ("a中😀z", 1, 1, "🦀文", "a🦀文z", 8),
        ("a😀中z", 1, 1, "XY", "aXYz", 3),
        ("e\u{301}z", 0, 0, "X", "X\u{301}z", 1),
        ("e\u{301}z", 1, 1, "X", "eXz", 2),
        ("abc", 0, 0, "e\u{301}", "e\u{301}c", 3),
        ("a\r\nz", 1, 1, "XY", "aXY\r\nz", 3),
        ("a\r\nz", 2, 2, "X", "a\rX\nz", 3),
        ("a\rz", 1, 1, "X", "aX\rz", 2),
        ("a\nz", 1, 1, "X", "aX\nz", 2),
        ("abc", 3, 3, "😀X", "abc😀X", 8),
        ("", 0, 0, "中", "中", 3),
        ("abc", 3, 1, "X", "aX", 2),
        ("a中😀z", 1, 8, "文", "a文z", 4),
        ("abcdef", 3, 1, "XY", "aXYef", 3),
        ("a\0中z", 1, 1, "XY", "aXYz", 3),
    ] {
        let mut editor = EditorSession::new(original);
        editor.set_overwrite(true);
        editor.select(selection(anchor, head))?;
        editor.type_text(typed)?;
        assert_eq!(editor.committed_text(), expected, "{original:?}, {typed:?}");
        assert_eq!(editor.selection(), selection(caret, caret));
        assert!(editor.is_modified());
    }
    let mut insert = EditorSession::new("a😀z");
    insert.select(selection(1, 1))?;
    insert.type_text("中")?;
    assert_eq!(insert.committed_text(), "a中😀z");
    insert.select(selection(1, 8))?;
    insert.type_text("XY")?;
    assert_eq!(insert.committed_text(), "aXYz");
    Ok(())
}

#[test]
fn overwrite_undo_replays_whole_scalars_and_respects_explicit_undo_groups() -> Result<(), EditError>
{
    let mut editor = EditorSession::new("a😀中z");
    editor.set_overwrite(true);
    editor.select(selection(1, 1))?;
    editor.type_text("XY")?;
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "aX中z");
    assert_eq!(editor.selection(), selection(2, 2));
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a😀中z");
    assert_eq!(editor.selection(), selection(1, 1));
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    assert!(editor.redo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "aXYz");
    assert_eq!(editor.selection(), selection(3, 3));

    let mut grouped = EditorSession::new("abcdef");
    grouped.set_overwrite(true);
    grouped.select(selection(3, 1))?;
    grouped.begin_undo_action();
    grouped.type_text("XY")?;
    grouped.end_undo_action();
    assert_eq!(grouped.committed_text(), "aXYef");
    assert!(grouped.undo());
    assert_eq!(grouped.committed_text(), "abcdef");
    assert_eq!(grouped.selection(), selection(3, 1));
    assert!(!grouped.undo());
    Ok(())
}

#[test]
fn empty_typing_and_read_only_failures_preserve_selection_revision_and_redo()
-> Result<(), EditError> {
    for overwrite in [false, true] {
        let mut editor = EditorSession::new("a😀z");
        editor.set_overwrite(overwrite);
        editor.insert_text("X")?;
        assert!(editor.undo());
        editor.select(selection(5, 1))?;
        let revision = editor.display_revision();
        editor.type_text("")?;
        assert_eq!(editor.selection(), selection(5, 1));
        editor.set_read_only(true);
        assert!(matches!(editor.type_text("文😀"), Err(EditError::ReadOnly)));
        assert_eq!(editor.committed_text(), "a😀z");
        assert_eq!(editor.selection(), selection(5, 1));
        assert_eq!(editor.display_revision(), revision);
        assert!(!editor.is_modified());
        editor.set_read_only(false);
        assert!(editor.redo());
        assert_eq!(editor.committed_text(), "Xa😀z");
    }
    Ok(())
}

#[test]
fn programmatic_insertion_and_clipboard_paste_do_not_overstrike() -> Result<(), EditError> {
    let mut editor = EditorSession::new("a😀z");
    editor.set_overwrite(true);
    editor.select(selection(1, 1))?;
    editor.insert_text("文")?;
    assert_eq!(editor.committed_text(), "a文😀z");
    editor.paste_clipboard(&notepad_matrix::clipboard::ClipboardText {
        text: "XY".into(),
        line_copy: false,
    })?;
    assert_eq!(editor.committed_text(), "a文XY😀z");
    editor.select(selection(1, 6))?;
    editor.insert_text("PQ")?;
    assert_eq!(editor.committed_text(), "aPQ😀z");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a文XY😀z");
    Ok(())
}

#[test]
fn composition_confirmation_maps_marked_text_once_without_overstriking_suffix()
-> Result<(), EditError> {
    for selected in [selection(1, 1), selection(5, 1)] {
        let mut editor = EditorSession::new("a😀z");
        editor.set_overwrite(true);
        editor.select(selected)?;
        editor.set_composition("候选")?;
        editor.set_composition("文")?;
        assert_eq!(editor.committed_text(), "a😀z");
        assert!(!editor.is_modified());
        editor.type_text("🦀中")?;
        let expected = if selected.range().is_empty() {
            "a🦀中😀z"
        } else {
            "a🦀中z"
        };
        assert_eq!(editor.committed_text(), expected);
        assert_eq!(editor.selection(), selection(8, 8));
        assert!(!editor.has_composition());
        assert!(editor.undo());
        assert_eq!(editor.committed_text(), "a😀z");
        assert_eq!(editor.selection(), selected);
        assert!(!editor.is_modified());
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.committed_text(), expected);
    }

    let mut partial = EditorSession::new("z");
    partial.set_overwrite(true);
    partial.set_composition("abcd")?;
    partial.mark_display_range(Some(ByteOffset(1)..ByteOffset(2)), "X", None)?;
    partial.type_text("Y")?;
    assert_eq!(partial.committed_text(), "aYcdz");
    assert_eq!(partial.selection(), selection(2, 2));
    assert!(partial.undo());
    assert_eq!(partial.committed_text(), "z");

    partial.mark_display_range(None, "文😀", Some(1..3))?;
    partial.commit_composition()?;
    assert_eq!(partial.committed_text(), "文😀z");
    assert_eq!(partial.selection(), selection(3, 7));
    Ok(())
}

#[test]
fn scalar_overwrite_works_across_rope_chunks() -> Result<(), EditError> {
    let prefix = "中😀".repeat(4096);
    let original = format!("{prefix}😀文\r\nz");
    let mut editor = EditorSession::new(&original);
    assert!(editor.display_snapshot().chunks().count() > 1);
    editor.set_overwrite(true);
    editor.select(selection(prefix.len(), prefix.len()))?;
    editor.type_text("XYQ")?;
    assert_eq!(editor.committed_text(), format!("{prefix}XYQ\r\nz"));
    assert_eq!(
        editor.selection(),
        selection(prefix.len() + 3, prefix.len() + 3)
    );
    Ok(())
}
