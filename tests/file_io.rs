use notepad_matrix::files::DiskFile;
use notepad_matrix::files::{OverwritePolicy, UnicodeFormat, Utf8Format};
use notepad_matrix::input::EditorSession;

#[test]
fn explicit_output_format_does_not_change_raw_conflict_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("encoding.txt");
    std::fs::write(&path, b"A\n")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let snapshot = EditorSession::new(&text).save_snapshot();
    let saved = file.save_with_format(&snapshot, UnicodeFormat::Utf16BeBom)?;
    assert_eq!(std::fs::read(&path)?, [0xfe, 0xff, 0, 0x41, 0, 0x0a]);
    assert_eq!(saved.format(), UnicodeFormat::Utf16BeBom);
    assert_eq!(file.format(), UnicodeFormat::Utf8(Utf8Format::Plain));
    assert!(
        file.save_with_format(&snapshot, UnicodeFormat::Utf8(Utf8Format::Bom))
            .is_err(),
        "old baseline accepted a changed file"
    );
    let plain = saved.save_with_format(&snapshot, UnicodeFormat::Utf8(Utf8Format::Plain))?;
    assert_eq!(std::fs::read(path)?, b"A\n");
    assert_eq!(plain.format(), UnicodeFormat::Utf8(Utf8Format::Plain));
    Ok(())
}

#[test]
fn unicode_eol_conversion_preserves_wire_format_and_absent_final_newline()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::input::LineEnding;
    let home = tempfile::tempdir()?;
    let fixtures: [(&[u8], &[u8]); 2] = [
        (
            &[
                0xff, 0xfe, 0x41, 0, 0x0d, 0, 0x0a, 0, 0x42, 0, 0x0d, 0, 0x43, 0, 0x0a, 0, 0x2d,
                0x4e, 0x3d, 0xd8, 0, 0xde,
            ],
            &[
                0xff, 0xfe, 0x41, 0, 0x0a, 0, 0x42, 0, 0x0a, 0, 0x43, 0, 0x0a, 0, 0x2d, 0x4e, 0x3d,
                0xd8, 0, 0xde,
            ],
        ),
        (
            &[
                0xfe, 0xff, 0, 0x41, 0, 0x0d, 0, 0x0a, 0, 0x42, 0, 0x0d, 0, 0x43, 0, 0x0a, 0x4e,
                0x2d, 0xd8, 0x3d, 0xde, 0,
            ],
            &[
                0xfe, 0xff, 0, 0x41, 0, 0x0a, 0, 0x42, 0, 0x0a, 0, 0x43, 0, 0x0a, 0x4e, 0x2d, 0xd8,
                0x3d, 0xde, 0,
            ],
        ),
    ];
    for (index, (bytes, expected)) in fixtures.into_iter().enumerate() {
        let path = home.path().join(format!("conversion-{index}"));
        std::fs::write(&path, bytes)?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let mut editor = EditorSession::from_file_text(&text);
        editor.convert_line_endings(LineEnding::Lf)?;
        let saved = file.save_unicode(&editor.save_snapshot())?;
        assert_eq!(saved.format(), file.format());
        assert_eq!(std::fs::read(path)?, expected);
    }
    Ok(())
}

#[test]
fn unicode_bom_only_leading_nul_and_mixed_eol_keep_exact_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let fixtures: [(&[u8], &str); 6] = [
        (&[0xff, 0xfe], ""),
        (&[0xfe, 0xff], ""),
        (&[0xff, 0xfe, 0, 0, 0x41, 0], "\0A"),
        (&[0xfe, 0xff, 0, 0, 0, 0x41], "\0A"),
        (
            &[
                0xff, 0xfe, 0x61, 0, 0x0d, 0, 0x0a, 0, 0x62, 0, 0x0a, 0, 0x63, 0, 0x0d, 0, 0x64, 0,
            ],
            "a\r\nb\nc\rd",
        ),
        (
            &[
                0xfe, 0xff, 0, 0x61, 0, 0x0d, 0, 0x0a, 0, 0x62, 0, 0x0a, 0, 0x63, 0, 0x0d, 0, 0x64,
            ],
            "a\r\nb\nc\rd",
        ),
    ];
    for (index, (bytes, expected)) in fixtures.into_iter().enumerate() {
        let path = home.path().join(format!("fixture-{index}"));
        std::fs::write(&path, bytes)?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        assert_eq!(text, expected);
        file.save_unicode(&EditorSession::new(&text).save_snapshot())?;
        assert_eq!(std::fs::read(path)?, bytes);
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn linked_file_saves_keep_the_inode_and_all_aliases_without_forcing_save_as()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let home = tempfile::tempdir()?;
    let path = home.path().join("linked.txt");
    let alias = home.path().join("alias.txt");
    std::fs::write(&path, "original longer text\n")?;
    std::fs::hard_link(&path, &alias)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))?;
    let before = std::fs::metadata(&path)?;
    let (file, _) = DiskFile::load_unicode(&path)?;
    let mut document = notepad_matrix::editor::Document::new("中\n");
    document.replace_selection("x")?;
    let snapshot = document.save_snapshot();
    let saved = file.save_unicode(&snapshot)?;
    document.acknowledge_save(&snapshot)?;
    assert!(!document.is_modified());
    assert_eq!(std::fs::read(&path)?, "x中\n".as_bytes());
    assert_eq!(std::fs::read(&alias)?, "x中\n".as_bytes());
    let after = std::fs::metadata(&path)?;
    assert_eq!((after.dev(), after.ino()), (before.dev(), before.ino()));
    assert_eq!(after.nlink(), 2);
    assert_eq!(after.permissions().mode() & 0o777, 0o640);
    let bytes = notepad_matrix::byte_buffer::ByteBuffer::from_bytes(b"\xff\0\x80");
    let saved = saved.save_byte_buffer(&bytes, saved.format())?;
    assert_eq!(std::fs::read(&alias)?, b"\xff\0\x80");
    std::fs::write(&alias, b"external change")?;
    assert!(saved.save_byte_buffer(&bytes, saved.format()).is_err());
    assert_eq!(std::fs::read(&path)?, b"external change");
    let (readonly, _) = DiskFile::load_unicode_bytes(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o440))?;
    assert_eq!(
        readonly
            .save_byte_buffer(&bytes, readonly.format())
            .err()
            .ok_or("read-only refusal")?
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(std::fs::read(&alias)?, b"external change");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))?;
    assert!(
        DiskFile::save_as_unicode(
            &alias,
            &snapshot,
            UnicodeFormat::Utf16BeBom,
            notepad_matrix::files::OverwritePolicy::Refuse
        )
        .is_err()
    );
    DiskFile::save_as_unicode(
        &alias,
        &snapshot,
        UnicodeFormat::Utf16BeBom,
        notepad_matrix::files::OverwritePolicy::Confirmed,
    )?;
    assert_eq!(std::fs::read(&path)?, b"\xfe\xff\0x\x4e\x2d\0\n");
    assert_eq!(std::fs::metadata(&alias)?.ino(), before.ino());
    Ok(())
}

#[test]
fn byte_buffer_encoding_streams_scalars_across_leaves_and_preserves_conflict_checks()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("cross-leaf");
    let text = format!("{}😀{}", "A".repeat(4095), "B".repeat(4093));
    std::fs::write(&path, text.as_bytes())?;
    let (file, buffer) = DiskFile::load_byte_buffer(&path)?;
    let expected = [
        b"\xff\xfe".as_slice(),
        &text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    ]
    .concat();
    let saved = file.save_byte_buffer(&buffer, UnicodeFormat::Utf16LeBom)?;
    assert_eq!(std::fs::read(&path)?, expected);
    std::fs::write(&path, b"\xffexternal")?;
    assert!(
        saved
            .save_byte_buffer(&buffer, UnicodeFormat::Utf16LeBom)
            .is_err()
    );
    assert_eq!(std::fs::read(&path)?, b"\xffexternal");
    Ok(())
}

#[test]
fn byte_buffer_file_edits_preserve_invalid_bytes_snapshots_and_wire_format()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    for (index, (bytes, replacement, expected)) in [
        (
            b"\xffA\0\xc0\xaf\r\nZ".as_slice(),
            b"\x80\xfe".as_slice(),
            b"\xff\x80\xfe\0\xc0\xaf\r\nZ".as_slice(),
        ),
        (
            b"\xff\xfe\0\xdcA\0\r\0\n\0".as_slice(),
            b"B".as_slice(),
            b"\xff\xfe\0\xdcB\0\r\0\n\0".as_slice(),
        ),
        (
            b"\xfe\xff\xdc\0\0A\0\r\0\n".as_slice(),
            b"B".as_slice(),
            b"\xfe\xff\xdc\0\0B\0\r\0\n".as_slice(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let path = home.path().join(format!("byte-buffer-{index}"));
        std::fs::write(&path, bytes)?;
        let (file, mut buffer) = DiskFile::load_byte_buffer(&path)?;
        let snapshot = buffer.clone();
        let original = buffer.to_vec();
        let at = if index == 0 { 1 } else { 3 };
        buffer.replace(at..at + 1, replacement)?;
        assert_eq!(snapshot.to_vec(), original);
        let saved = file.save_byte_buffer(&buffer, file.format())?;
        assert_eq!(std::fs::read(&path)?, expected);
        assert_eq!(saved.decoded_bytes(), buffer.to_vec());
        assert!(file.save_byte_buffer(&snapshot, file.format()).is_err());
    }
    Ok(())
}

#[test]
fn unicode_byte_loading_preserves_isolated_low_surrogates_and_invalid_utf8()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    for (index, (bytes, expected)) in [
        (b"\xff\xfe\0\xdcA\0".as_slice(), b"\xed\xb0\x80A".as_slice()),
        (b"\xfe\xff\xdc\0\0A".as_slice(), b"\xed\xb0\x80A".as_slice()),
        (b"\xef\xbb\xbf\xff\0A".as_slice(), b"\xff\0A".as_slice()),
        (b"\xff\0A".as_slice(), b"\xff\0A".as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        let path = home.path().join(format!("raw-{index}"));
        std::fs::write(&path, bytes)?;
        let (file, decoded) = DiskFile::load_unicode_bytes(&path)?;
        assert_eq!(decoded, expected);
        assert_eq!(file.decoded_bytes(), expected);
        assert_eq!(std::fs::read(path)?, bytes);
    }
    Ok(())
}

#[test]
fn source_utf16_reader_ignores_odd_tail_and_drops_incomplete_or_invalid_high_pairs()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let cases: [&[u8]; 6] = [
        &[0xff, 0xfe, 0x41],
        &[0xfe, 0xff, 0],
        &[0xff, 0xfe, 0, 0xd8],
        &[0xfe, 0xff, 0xd8, 0],
        &[0xff, 0xfe, 0, 0xd8, 0x41, 0],
        &[0xfe, 0xff, 0xd8, 0, 0, 0x41],
    ];
    for (index, bytes) in cases.into_iter().enumerate() {
        let path = home.path().join(format!("malformed-{index}"));
        std::fs::write(&path, bytes)?;
        let (_, text) = DiskFile::load_unicode(&path)?;
        assert_eq!(text, "");
        assert_eq!(std::fs::read(path)?, bytes);
    }
    Ok(())
}

#[test]
fn unicode_save_as_keeps_format_confirms_binary_overwrite_and_streams_supplementary_text()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let cases = [
        (
            UnicodeFormat::Utf16LeBom,
            [0xff, 0xfe],
            [0x3d, 0xd8, 0, 0xde, 0x2d, 0x4e],
        ),
        (
            UnicodeFormat::Utf16BeBom,
            [0xfe, 0xff],
            [0xd8, 0x3d, 0xde, 0, 0x4e, 0x2d],
        ),
    ];
    for (index, (format, bom, units)) in cases.into_iter().enumerate() {
        let path = home.path().join(format!("output-{index}"));
        let editor = EditorSession::new(&"😀中".repeat(4096));
        assert!(
            editor.display_snapshot().chunks().count() > 1,
            "supplementary fixture must exercise multiple storage chunks"
        );
        let mut expected = bom.to_vec();
        expected.extend_from_slice(&units.repeat(4096));
        DiskFile::save_as_unicode(
            &path,
            &editor.save_snapshot(),
            format,
            OverwritePolicy::Refuse,
        )?;
        assert_eq!(std::fs::read(&path)?, expected);
        assert!(
            DiskFile::save_as_unicode(
                &path,
                &EditorSession::new("different").save_snapshot(),
                format,
                OverwritePolicy::Refuse
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path)?, expected);
        std::fs::write(&path, [0xff, 0, 0x80])?;
        DiskFile::save_as_unicode(
            &path,
            &editor.save_snapshot(),
            format,
            OverwritePolicy::Confirmed,
        )?;
        assert_eq!(std::fs::read(&path)?, expected);
        let (file, loaded) = DiskFile::load_unicode(&path)?;
        assert_eq!(loaded, "😀中".repeat(4096));
        assert_eq!(file.format(), format);
        std::fs::write(&path, b"external")?;
        assert!(file.save_unicode(&editor.save_snapshot()).is_err());
        assert_eq!(std::fs::read(path)?, b"external");
    }
    Ok(())
}

#[test]
fn readonly_unicode_source_can_save_as_new_writable_target()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    for (index, bytes) in [&[0xff, 0xfe, 0x41, 0][..], &[0xfe, 0xff, 0, 0x41][..]]
        .into_iter()
        .enumerate()
    {
        let original = home.path().join(format!("readonly-{index}"));
        let copy = home.path().join(format!("copy-{index}"));
        std::fs::write(&original, bytes)?;
        let mut permissions = std::fs::metadata(&original)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&original, permissions)?;
        let (file, text) = DiskFile::load_unicode(&original)?;
        assert!(file.is_read_only());
        let snapshot = EditorSession::new(&text).save_snapshot();
        assert!(file.save_unicode(&snapshot).is_err());
        let saved =
            DiskFile::save_as_unicode(&copy, &snapshot, file.format(), OverwritePolicy::Refuse)?;
        assert!(!saved.is_read_only());
        assert_eq!(std::fs::read(copy)?, bytes);
        assert_eq!(std::fs::read(original)?, bytes);
    }
    Ok(())
}

#[test]
fn bom_utf16_round_trips_scalar_text_and_committed_edits_in_both_endians()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let fixtures: [(UnicodeFormat, &[u8], &[u8]); 2] = [
        (
            UnicodeFormat::Utf16LeBom,
            &[
                0xff, 0xfe, 0x41, 0, 0, 0, 0x2d, 0x4e, 0x3d, 0xd8, 0, 0xde, 0x0d, 0, 0x0a, 0, 0x5a,
                0,
            ],
            &[0x21, 0],
        ),
        (
            UnicodeFormat::Utf16BeBom,
            &[
                0xfe, 0xff, 0, 0x41, 0, 0, 0x4e, 0x2d, 0xd8, 0x3d, 0xde, 0, 0, 0x0d, 0, 0x0a, 0,
                0x5a,
            ],
            &[0, 0x21],
        ),
    ];
    for (index, (format, bytes, suffix)) in fixtures.into_iter().enumerate() {
        let path = home.path().join(format!("utf16-{index}.txt"));
        std::fs::write(&path, bytes)?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        assert_eq!(text, "A\0中😀\r\nZ");
        assert_eq!(file.format(), format);
        assert!(DiskFile::load_utf8(&path).is_err());
        let mut editor = EditorSession::new(&text);
        editor.move_to_document_edge(true, false)?;
        editor.insert_text("!")?;
        editor.set_composition("preview")?;
        let snapshot = editor.save_snapshot();
        assert!(file.save_utf8(&snapshot).is_err());
        assert_eq!(std::fs::read(&path)?, bytes);
        let saved = file.save_unicode(&snapshot)?;
        let mut expected = bytes.to_vec();
        expected.extend_from_slice(suffix);
        assert_eq!(std::fs::read(&path)?, expected);
        assert_eq!(saved.format(), format);
        editor.acknowledge_save(&snapshot)?;
        assert!(!editor.is_modified());
        assert!(editor.display_text().ends_with("preview"));
    }
    Ok(())
}

#[test]
fn save_as_keeps_selected_bom_and_requires_explicit_overwrite_confirmation()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let target = home.path().join("target.txt");
    std::fs::write(&target, [0xff, 0x00, 0x80])?;
    let mut editor = EditorSession::new("");
    editor.insert_text("中😀\r\n")?;
    let snapshot = editor.save_snapshot();
    assert!(
        DiskFile::save_as_utf8(&target, &snapshot, Utf8Format::Bom, OverwritePolicy::Refuse)
            .is_err()
    );
    assert_eq!(std::fs::read(&target)?, [0xff, 0x00, 0x80]);
    let saved = DiskFile::save_as_utf8(
        &target,
        &snapshot,
        Utf8Format::Bom,
        OverwritePolicy::Confirmed,
    )?;
    assert_eq!(std::fs::read(&target)?, "\u{feff}中😀\r\n".as_bytes());
    assert_eq!(saved.format(), UnicodeFormat::Utf8(Utf8Format::Bom));
    let copy = home.path().join("copy.txt");
    DiskFile::save_as_unicode(&copy, &snapshot, saved.format(), OverwritePolicy::Refuse)?;
    assert_eq!(std::fs::read(&copy)?, "\u{feff}中😀\r\n".as_bytes());
    Ok(())
}

#[test]
fn external_change_is_not_overwritten_and_failed_save_keeps_dirty_state()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("document.txt");
    std::fs::write(&path, b"original")?;
    let (file, text) = DiskFile::load_utf8(&path)?;
    let mut editor = EditorSession::new(&text);
    editor.insert_text("edited")?;
    std::fs::write(&path, b"external")?;
    assert!(file.save_utf8(&editor.save_snapshot()).is_err());
    assert_eq!(std::fs::read(&path)?, b"external");
    assert!(editor.is_modified());
    assert_eq!(std::fs::read_dir(home.path())?.count(), 1);
    Ok(())
}

#[test]
fn save_as_new_utf8_creates_exact_bytes_without_clobbering_existing_files()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("new.txt");
    let mut editor = EditorSession::new("");
    editor.insert_text("中😀\nno-final-eol")?;
    let snapshot = editor.save_snapshot();
    let saved = DiskFile::create_utf8(&path, &snapshot)?;
    assert_eq!(std::fs::read(&path)?, "中😀\nno-final-eol".as_bytes());
    assert!(DiskFile::create_utf8(&path, &snapshot).is_err());
    assert_eq!(std::fs::read(&path)?, "中😀\nno-final-eol".as_bytes());
    assert_eq!(saved.path(), std::fs::canonicalize(&path)?);
    assert_eq!(std::fs::read_dir(home.path())?.count(), 1);
    Ok(())
}

#[test]
fn explicit_utf8_save_preserves_bom_eol_and_excludes_preedit()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("document.txt");
    std::fs::write(&path, b"\xef\xbb\xbfA\r\nZ")?;
    let (file, text) = DiskFile::load_utf8(&path)?;
    assert_eq!(text, "A\r\nZ");
    let mut editor = EditorSession::new(&text);
    editor.move_to_document_edge(true, false)?;
    editor.insert_text("中😀")?;
    editor.set_composition("candidate")?;
    let snapshot = editor.save_snapshot();
    let saved = file.save_utf8(&snapshot)?;
    assert_eq!(saved.path(), std::fs::canonicalize(&path)?);
    assert_eq!(std::fs::read(&path)?, "\u{feff}A\r\nZ中😀".as_bytes());
    editor.acknowledge_save(&snapshot)?;
    assert!(!editor.is_modified());
    assert!(editor.display_text().ends_with("candidate"));
    Ok(())
}
