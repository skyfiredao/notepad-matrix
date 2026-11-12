use notepad_matrix::{
    files::DiskFile, launch::LaunchOptions, syntax::Language, workspace::Workspace,
};

#[test]
fn saved_document_transfer_roundtrips_arguments_position_and_language()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("file with spaces.cpp");
    std::fs::write(&path, "one\r\n\t中two")?;
    let mut source = Workspace::new();
    let (file, text) = DiskFile::load_unicode(&path)?;
    source.open_buffer(file, &text)?;
    source.active_mut().set_language(Language::Builtin(3));
    source
        .active_mut()
        .editor
        .select(notepad_matrix::editor::Selection {
            anchor: notepad_matrix::editor::ByteOffset(9),
            head: notepad_matrix::editor::ByteOffset(9),
        })?;
    let options = LaunchOptions::for_transfer(&source)?.ok_or("clean named transfer")?;
    let restored_options = LaunchOptions::parse(options.arguments()?)?;
    assert!(restored_options.no_session);
    assert_eq!(restored_options.line, Some(2));
    assert_eq!(restored_options.column, Some(6));
    let mut destination = Workspace::new();
    assert!(restored_options.open_into(&mut destination)?.is_empty());
    assert_eq!(destination.buffers().len(), 1);
    assert_eq!(
        destination.active().editor.committed_text(),
        "one\r\n\t中two"
    );
    assert_eq!(destination.active().editor.selection().head.0, 9);
    assert_eq!(destination.active().language(), Language::Builtin(3));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let helper = home.path().join("instance helper");
        std::fs::write(&helper, "#!/bin/sh\nprintf '%s\\0' \"$@\"\n")?;
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))?;
        let output = options.command(&helper)?.output()?;
        assert!(output.status.success());
        let bytes = output
            .stdout
            .strip_suffix(&[0])
            .ok_or("argument terminator")?;
        let expected = options.arguments()?;
        assert_eq!(
            bytes.split(|byte| *byte == 0).collect::<Vec<_>>(),
            expected
                .iter()
                .map(|value| value.as_encoded_bytes())
                .collect::<Vec<_>>()
        );
    }
    source.active_mut().set_language(Language::UserDefined(77));
    assert!(
        LaunchOptions::for_transfer(&source)?
            .ok_or("UDL transfer")?
            .language
            .is_none()
    );
    source.active_mut().editor.insert_text("dirty")?;
    assert!(LaunchOptions::for_transfer(&source)?.is_none());
    assert!(LaunchOptions::for_transfer(&Workspace::new())?.is_none());
    assert!(LaunchOptions::parse([std::ffi::OsString::from("-noPlugin")]).is_err());
    Ok(())
}
