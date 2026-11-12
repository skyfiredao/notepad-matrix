use notepad_matrix::editor::{ByteOffset, Document, Selection};

#[test]
fn document_byte_snapshots_follow_shared_undo_and_captured_saves()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new("A中\r\n😀");
    let mut peer = document.clone_view();
    let original = document.byte_snapshot();
    document.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(4),
    })?;
    document.replace_selection("ß")?;
    let saved = document.save_snapshot();
    let captured = saved.byte_snapshot();
    let end = ByteOffset(peer.len_bytes());
    peer.select(Selection {
        anchor: end,
        head: end,
    })?;
    peer.replace_selection("\0Z")?;
    document.acknowledge_save(&saved)?;
    assert_eq!(document.byte_snapshot().to_vec(), "Aß\r\n😀\0Z".as_bytes());
    assert_eq!(peer.byte_snapshot().to_vec(), "Aß\r\n😀\0Z".as_bytes());
    assert_eq!(original.to_vec(), "A中\r\n😀".as_bytes());
    assert_eq!(captured.to_vec(), "Aß\r\n😀".as_bytes());
    assert!(document.is_modified());
    assert!(peer.undo());
    assert_eq!(document.byte_snapshot().to_vec(), "Aß\r\n😀".as_bytes());
    assert!(!document.is_modified());
    assert!(document.undo());
    assert_eq!(peer.byte_snapshot().to_vec(), "A中\r\n😀".as_bytes());
    assert!(document.redo());
    assert!(peer.redo());
    let mut output = Vec::new();
    document.save_snapshot().write_to(&mut output)?;
    assert_eq!(output, "Aß\r\n😀\0Z".as_bytes());
    assert_eq!(captured.to_vec(), "Aß\r\n😀".as_bytes());
    Ok(())
}

#[test]
fn byte_save_payloads_follow_evolving_eol_edits_and_exclude_ime_preview()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::input::{EditorSession, LineEnding};
    for (ending, expected) in [
        (LineEnding::Lf, "a\nb\nc\n"),
        (LineEnding::Cr, "a\rb\rc\r"),
        (LineEnding::CrLf, "a\r\nb\r\nc\r\n"),
    ] {
        let mut editor = EditorSession::new("a\r\nb\rc\n");
        editor.convert_line_endings(ending)?;
        assert_eq!(
            editor.document().byte_snapshot().to_vec(),
            expected.as_bytes()
        );
        editor.set_composition("预览😀")?;
        let saved = editor.save_snapshot();
        assert_eq!(saved.byte_snapshot().to_vec(), expected.as_bytes());
        editor.cancel_composition();
        assert!(editor.undo());
        assert_eq!(editor.document().byte_snapshot().to_vec(), b"a\r\nb\rc\n");
        assert!(editor.redo());
        let mut output = Vec::new();
        editor.save_snapshot().write_to(&mut output)?;
        assert_eq!(output, expected.as_bytes());
    }
    Ok(())
}
