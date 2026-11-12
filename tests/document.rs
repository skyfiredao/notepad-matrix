use notepad_matrix::editor::{ByteOffset, Document, Selection};

#[test]
fn visible_rows_follow_caret_and_clamp_at_document_edges() {
    use notepad_matrix::editor::visible_rows;
    assert_eq!(visible_rows(0, 30, 99, 100), 70..100);
    assert_eq!(visible_rows(70, 30, 2, 100), 2..32);
    assert_eq!(visible_rows(90, 30, 99, 100), 70..100);
    assert_eq!(visible_rows(80, 30, 0, 1), 0..1);
    assert_eq!(visible_rows(0, 0, 0, 1), 0..1);
}

#[test]
fn replacement_and_history_restore_bytes_selection_and_saved_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::new("A中文\r\nZ");
    doc.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(7),
    })?;
    assert!(!doc.is_modified());
    doc.replace_selection("😀")?;
    assert_eq!(doc.text(), "A😀\r\nZ");
    assert_eq!(doc.selection().head, ByteOffset(5));
    assert!(doc.is_modified());
    assert!(doc.undo());
    assert_eq!(doc.text(), "A中文\r\nZ");
    assert_eq!(
        doc.selection(),
        Selection {
            anchor: ByteOffset(1),
            head: ByteOffset(7)
        }
    );
    assert!(!doc.is_modified());
    assert!(doc.redo());
    assert_eq!(doc.text(), "A😀\r\nZ");
    assert!(doc.is_modified());
    Ok(())
}

#[test]
fn utf16_and_line_metrics_preserve_mixed_line_endings() -> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::Utf16Offset;
    let doc = Document::new("A😀\r\n中\nZ\r");
    assert_eq!(doc.len_bytes(), 13);
    assert_eq!(doc.len_utf16(), 9);
    assert_eq!(doc.line_count(), 4);
    assert_eq!(doc.line_start(1)?, ByteOffset(7));
    assert_eq!(doc.line_start(2)?, ByteOffset(11));
    assert_eq!(doc.line_start(3)?, ByteOffset(13));
    assert_eq!(doc.line_text(0)?, "A😀\r\n");
    assert_eq!(doc.to_utf16(ByteOffset(5))?, Utf16Offset(3));
    assert_eq!(doc.from_utf16(Utf16Offset(3))?, ByteOffset(5));
    assert!(doc.from_utf16(Utf16Offset(2)).is_err());
    assert!(doc.to_utf16(ByteOffset(2)).is_err());
    Ok(())
}
