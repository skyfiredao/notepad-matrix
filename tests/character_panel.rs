use notepad_matrix::character_panel::{Table, insert};
use notepad_matrix::input::EditorSession;

#[test]
fn explicit_character_encodings_preserve_source_columns_and_insertion()
-> Result<(), Box<dyn std::error::Error>> {
    let mut table = Table::default();
    assert_eq!(table.encoding(), "UTF-8");
    assert_eq!(table.row(0).cells[2], "NULL");
    assert_eq!(table.insertion(0, 2).as_deref(), Some("\0"));
    assert_eq!(table.row(128).character, "�");
    table.select_encoding("windows-1252")?;
    assert_eq!(
        table.row(128).cells,
        ["128", "80", "€", "&euro;", "&#8364;", "&#x20ac;"]
    );
    assert_eq!(table.row(45).cells[4], "&#8722;");
    let mut editor = EditorSession::new("");
    insert(&mut editor, &table.insertion(128, 2).ok_or("character")?)?;
    insert(&mut editor, &table.insertion(169, 3).ok_or("entity")?)?;
    assert_eq!(editor.committed_text(), "€&copy;");
    table.select_encoding("UTF-8")?;
    assert!(table.row(169).cells[3].is_empty());
    Ok(())
}
