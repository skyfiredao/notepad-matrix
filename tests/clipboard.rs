use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::input::EditorSession;

#[test]
fn default_copy_without_selection_pastes_whole_line_before_destination_line()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = EditorSession::from_file_text("A\n中😀");
    source.move_to_document_edge(true, false)?;
    let copied = source.clipboard_text();
    assert_eq!(
        copied.text, "中😀\n",
        "CopyLineRange appends the document EOL even on an unterminated final line"
    );
    assert!(copied.line_copy);
    let mut target = EditorSession::from_file_text("first\r\nlast");
    target.select(Selection {
        anchor: ByteOffset(9),
        head: ByteOffset(9),
    })?;
    target.paste_clipboard(&copied)?;
    assert_eq!(target.committed_text(), "first\r\n中😀\r\nlast");
    assert_eq!(
        target.selection(),
        Selection {
            anchor: ByteOffset(18),
            head: ByteOffset(18)
        }
    );
    assert!(target.undo());
    assert_eq!(target.committed_text(), "first\r\nlast");
    Ok(())
}

#[test]
fn selected_clipboard_text_pastes_in_destination_eol_as_one_undoable_edit()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = EditorSession::new("A中😀\nB");
    source.select(Selection {
        anchor: ByteOffset(8),
        head: ByteOffset(1),
    })?;
    let copied = source.clipboard_text();
    assert_eq!(copied.text, "中😀");
    assert!(!copied.line_copy);
    let mut target = EditorSession::new("left\r\nright");
    target.select(Selection {
        anchor: ByteOffset(4),
        head: ByteOffset(6),
    })?;
    target.paste_clipboard(&notepad_matrix::clipboard::ClipboardText {
        text: "中😀\nX\rY".into(),
        line_copy: false,
    })?;
    assert_eq!(target.committed_text(), "left中😀\r\nX\r\nYright");
    assert!(target.undo());
    assert_eq!(target.committed_text(), "left\r\nright");
    assert_eq!(
        target.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(6)
        }
    );
    assert!(!target.undo());
    Ok(())
}
