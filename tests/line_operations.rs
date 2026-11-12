use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::input::EditorSession;

#[test]
fn duplicate_without_selection_uses_loaded_eol_keeps_caret_and_one_undo()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::from_file_text("a\nb");
    editor.move_to_document_edge(true, false)?;
    editor.toggle_bookmark(None)?;
    editor.duplicate(false)?;
    assert_eq!(editor.committed_text(), "a\nb\nb");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(3),
            head: ByteOffset(3)
        }
    );
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![1]
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a\nb");
    assert!(!editor.is_modified());
    editor.set_read_only(true);
    assert!(editor.duplicate(true).is_err());
    assert_eq!(editor.committed_text(), "a\nb");
    let mut empty = EditorSession::new("");
    empty.duplicate(false)?;
    assert_eq!(empty.committed_text(), "\r\n");
    assert_eq!(empty.selection(), Selection::default());
    assert!(empty.undo());
    assert_eq!(empty.committed_text(), "");
    assert!(!empty.is_modified());
    Ok(())
}

#[test]
fn duplicate_menu_always_uses_caret_line_but_ctrl_d_uses_the_nonempty_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("ab\ncd");
    let reversed = Selection {
        anchor: ByteOffset(5),
        head: ByteOffset(0),
    };
    editor.select(reversed)?;
    editor.duplicate(true)?;
    assert_eq!(editor.committed_text(), "ab\r\nab\ncd");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(9),
            head: ByteOffset(0)
        }
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab\ncd");
    assert_eq!(editor.selection(), reversed);
    editor.duplicate(false)?;
    assert_eq!(editor.committed_text(), "ab\ncdab\ncd");
    assert_eq!(editor.selection(), reversed);
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab\ncd");
    assert!(!editor.undo());
    Ok(())
}
