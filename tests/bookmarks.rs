use notepad_matrix::editor::{ByteOffset, Document, Selection};

#[test]
fn existing_bulk_transactions_keep_document_owned_bookmarks()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::{
        clipboard::ClipboardText,
        input::{EditorSession, LineEnding},
        search::LiteralOptions,
    };
    let mut editor = EditorSession::new("a\nb\nc");
    editor.select(Selection {
        anchor: ByteOffset(2),
        head: ByteOffset(2),
    })?;
    editor.toggle_bookmark(None)?;
    editor.paste_clipboard(&ClipboardText {
        text: "X\n".into(),
        line_copy: true,
    })?;
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![2]
    );
    assert!(editor.undo());
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![1]
    );
    assert!(editor.redo());
    editor.replace_all_literal("a\n", "", LiteralOptions::default())?;
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![1]
    );
    assert!(editor.undo());
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![2]
    );
    editor.convert_line_endings(LineEnding::CrLf)?;
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![2]
    );
    assert!(editor.undo());
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![2]
    );
    Ok(())
}

#[test]
fn bookmarks_follow_line_splices_and_text_undo_without_undoing_independent_toggles()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new("a\nb\nc");
    document.select(Selection {
        anchor: ByteOffset(2),
        head: ByteOffset(2),
    })?;
    document.toggle_bookmark();
    document.replace(ByteOffset(2)..ByteOffset(2), "prefix\n")?;
    assert_eq!(document.bookmarked_lines().collect::<Vec<_>>(), vec![2]);
    document.select(Selection::default())?;
    document.toggle_bookmark();
    assert!(document.undo());
    assert_eq!(document.bookmarked_lines().collect::<Vec<_>>(), vec![0, 1]);
    assert!(document.redo());
    assert_eq!(document.bookmarked_lines().collect::<Vec<_>>(), vec![0, 2]);
    document.replace(ByteOffset(0)..ByteOffset(9), "")?;
    assert_eq!(document.text(), "b\nc");
    assert_eq!(document.bookmarked_lines().collect::<Vec<_>>(), vec![0]);
    Ok(())
}

#[test]
fn bookmark_toggle_navigation_and_clear_follow_source_without_text_history()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new("first\r\n中😀\nlast");
    document.toggle_bookmark();
    document.select(Selection {
        anchor: ByteOffset(15),
        head: ByteOffset(15),
    })?;
    document.toggle_bookmark();
    assert_eq!(document.bookmarked_lines().collect::<Vec<_>>(), vec![0, 2]);
    assert!(!document.is_modified());
    assert!(!document.undo());
    assert!(document.goto_bookmark(false)?);
    assert_eq!(document.selection(), Selection::default());
    assert!(document.goto_bookmark(false)?);
    assert_eq!(
        document.selection(),
        Selection {
            anchor: ByteOffset(15),
            head: ByteOffset(15)
        }
    );
    assert!(document.goto_bookmark(true)?);
    assert_eq!(document.selection(), Selection::default());
    document.clear_bookmarks();
    assert!(!document.goto_bookmark(true)?);
    assert_eq!(document.text(), "first\r\n中😀\nlast");
    Ok(())
}
