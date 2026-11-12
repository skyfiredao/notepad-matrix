use notepad_matrix::converter::{ascii_to_hex, hex_to_ascii};
use notepad_matrix::input::LineEnding;

#[test]
fn converter_replacement_selects_output_and_keeps_source_delete_then_add_history()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::new("Hello!");
    editor.select(Selection {
        anchor: ByteOffset(5),
        head: ByteOffset(0),
    })?;
    editor.replace_converter_selection("48656C6C6F")?;
    assert_eq!(editor.committed_text(), "48656C6C6F!");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(10)
        }
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "!");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "Hello!");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(5),
            head: ByteOffset(0)
        }
    );
    assert!(editor.redo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "48656C6C6F!");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(10)
        }
    );
    Ok(())
}

#[test]
fn converter_default_uses_uppercase_hex_current_eol_and_strict_source_spacing()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ascii_to_hex(b"Hello!", LineEnding::CrLf), "48656C6C6F21");
    assert_eq!(
        ascii_to_hex(b"0123456789ABCDEF", LineEnding::CrLf),
        "30313233343536373839414243444546\r\n"
    );
    assert_eq!(
        ascii_to_hex("中😀".as_bytes(), LineEnding::Lf),
        "E4B8ADF09F9880"
    );
    assert_eq!(hex_to_ascii(b"41 42")?, b"AB");
    assert_eq!(hex_to_ascii(b"4142\r\n43")?, b"ABC");
    assert_eq!(hex_to_ascii(b"FF00")?, [255, 0]);
    for bad in [b"41\t42".as_slice(), b"41  42", b"4 1", b"41 "] {
        assert!(hex_to_ascii(bad).is_err());
    }
    Ok(())
}
