use notepad_matrix::mime::{MimeOperation, transform};

#[test]
fn mime_target_replacement_keeps_source_caret_at_start_and_single_undo()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::new("Hello!");
    let selected = Selection {
        anchor: ByteOffset(5),
        head: ByteOffset(0),
    };
    editor.select(selected)?;
    let encoded = transform("Hello", MimeOperation::Base64Encode)?;
    editor.replace_target_from_selection(&encoded)?;
    assert_eq!(editor.committed_text(), "SGVsbG8!");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(0)
        }
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "Hello!");
    assert_eq!(editor.selection(), selected);
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "SGVsbG8!");
    Ok(())
}

#[test]
fn base64_source_defaults_keep_no_padding_and_preserve_illegal_separators_on_decode()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(transform("Hello", MimeOperation::Base64Encode)?, "SGVsbG8");
    assert_eq!(
        transform("Hello", MimeOperation::Base64EncodePadded)?,
        "SGVsbG8="
    );
    assert_eq!(transform("SGVsbG8=", MimeOperation::Base64Decode)?, "Hello");
    assert_eq!(
        transform(
            "TWFyeQ==.aGFk.YQ.bGl0dGxl.bGFtYg==",
            MimeOperation::Base64Decode
        )?,
        "Mary.had.a.little.lamb"
    );
    Ok(())
}
