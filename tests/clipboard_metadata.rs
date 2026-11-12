use notepad_matrix::clipboard::ClipboardKind;

#[test]
fn binary_copy_uses_the_source_primary_range_and_aggregate_selected_length()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection, SelectionSet};
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::from_file_text("ab\ncd");
    editor.set_selections(SelectionSet::new(
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
    )?)?;
    let before = editor.display_selections();
    let copied = notepad_matrix::clipboard::binary_selection(&mut editor)?;
    assert_eq!(copied, Some(b"c\0\0\0".to_vec()));
    assert_eq!(editor.display_selections(), before);
    assert_eq!(editor.committed_text(), "ab\ncd");
    assert!(!editor.document().can_undo());
    Ok(())
}

#[test]
fn binary_copy_preserves_source_rectangle_truncation_and_empty_primary_expansion()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection, SelectionSet};
    use notepad_matrix::input::EditorSession;
    let mut rectangle = EditorSession::from_file_text("a\tB\nxxxx😀Q\n");
    rectangle.set_selections(SelectionSet::rectangle(
        vec![
            Selection {
                anchor: ByteOffset(2),
                head: ByteOffset(3),
            }
            .into(),
            Selection {
                anchor: ByteOffset(8),
                head: ByteOffset(12),
            }
            .into(),
        ],
        1,
    )?)?;
    let before = rectangle.display_selections();
    assert_eq!(
        notepad_matrix::clipboard::binary_selection(&mut rectangle)?,
        Some(b"B\nxxxx\xf0".to_vec())
    );
    assert_eq!(rectangle.display_selections(), before);
    assert!(!rectangle.document().can_undo());
    let mut empty_primary = EditorSession::from_file_text("abc def\n");
    empty_primary.set_selections(SelectionSet::new(
        vec![
            Selection {
                anchor: ByteOffset(0),
                head: ByteOffset(1),
            }
            .into(),
            Selection {
                anchor: ByteOffset(5),
                head: ByteOffset(5),
            }
            .into(),
        ],
        1,
    )?)?;
    assert_eq!(
        notepad_matrix::clipboard::binary_selection(&mut empty_primary)?,
        Some(b"def".to_vec())
    );
    assert_eq!(empty_primary.display_selections().ranges().len(), 2);
    assert_eq!(
        empty_primary.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(7)
        }
    );
    assert_eq!(empty_primary.committed_text(), "abc def\n");
    assert!(!empty_primary.document().can_undo());
    Ok(())
}

#[test]
fn document_paths_follow_view_order_without_duplicate_clones()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::{
        clipboard::CopyPath,
        files::DiskFile,
        workspace::{CloseDisposition, Workspace},
    };
    let home = tempfile::tempdir()?;
    let path = home.path().join("notes.txt");
    std::fs::write(&path, "saved")?;
    let path = std::fs::canonicalize(path)?;
    let mut workspace = Workspace::new();
    let initial = workspace.active().id();
    let (file, text) = DiskFile::load_unicode(&path)?;
    let named = workspace.open_buffer(file, &text)?;
    workspace.close_buffer(initial, CloseDisposition::IfUnmodified)?;
    workspace.clone_to_other_view(named)?;
    workspace.new_buffer()?;
    assert_eq!(
        CopyPath::AllNames.text(&workspace),
        "notes.txt\r\nnew 1\r\n"
    );
    assert_eq!(
        CopyPath::AllPaths.text(&workspace),
        format!("{}\r\nnew 1\r\n", path.display())
    );
    workspace.activate(named)?;
    assert_eq!(CopyPath::Full.text(&workspace), path.to_string_lossy());
    assert_eq!(
        CopyPath::Directory.text(&workspace),
        std::fs::canonicalize(home.path())?.to_string_lossy()
    );
    Ok(())
}

#[test]
fn native_metadata_distinguishes_rectangle_line_and_external_text() {
    assert_eq!(
        notepad_matrix::clipboard::binary_paste_text("a\0b", true),
        "a\0b"
    );
    assert_eq!(
        notepad_matrix::clipboard::binary_paste_text("a\0b", false),
        "a"
    );
    assert_eq!(
        ClipboardKind::from_metadata(Some("notepad-matrix/rectangle-copy/1")),
        ClipboardKind::Rectangle
    );
    assert_eq!(
        ClipboardKind::from_metadata(Some("notepad-matrix/line-copy/1")),
        ClipboardKind::Line
    );
    assert_eq!(
        ClipboardKind::from_metadata(Some("unrelated metadata")),
        ClipboardKind::Stream
    );
    assert_eq!(ClipboardKind::from_metadata(None), ClipboardKind::Stream);
    for kind in [
        ClipboardKind::Rectangle,
        ClipboardKind::Line,
        ClipboardKind::Stream,
    ] {
        assert_eq!(ClipboardKind::from_metadata(kind.metadata()), kind);
    }
}

#[test]
fn stream_multi_cut_and_rectangle_payload_paste_keep_source_shape_contract() {
    use notepad_matrix::editor::{ByteOffset, Selection, SelectionSet};
    use notepad_matrix::input::EditorSession;
    let mut session = EditorSession::new("ab\ncd");
    session
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
    assert!(!session.display_selections().is_rectangular());
    let payload = session.clipboard_payload();
    assert!(payload.rectangular);
    assert_eq!(payload.text, "a\r\nc\r\n");
    session.cut_selection().unwrap();
    assert_eq!(session.display_text(), "b\nd");
    assert!(session.undo());
    assert_eq!(session.display_text(), "ab\ncd");
    assert!(!session.undo());
    let mut target = EditorSession::new("zz\nzz");
    target
        .select_display(Selection {
            anchor: ByteOffset(1),
            head: ByteOffset(1),
        })
        .unwrap();
    target.paste_payload(&payload, 4).unwrap();
    assert_eq!(target.display_text(), "zaz\nzcz");
    assert!(target.undo());
    assert_eq!(target.display_text(), "zz\nzz");
    target
        .paste_clipboard(&notepad_matrix::clipboard::ClipboardText {
            text: "a\nc".into(),
            line_copy: false,
        })
        .unwrap();
    assert_eq!(target.display_text(), "za\r\ncz\nzz");
}
