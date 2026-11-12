use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::status::measure;
use ropey::Rope;

#[test]
fn summary_uses_source_delimiters_and_counts_selection_without_line_endings()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{UnicodeFormat, Utf8Format};
    use notepad_matrix::input::LineEnding;

    let text = Rope::from_str("one_two 中😀\r\ncan't\u{a0}stop\u{b}word\n");
    let summary = notepad_matrix::status::summarize(
        &text,
        Selection {
            anchor: ByteOffset(17),
            head: ByteOffset(8),
        },
        UnicodeFormat::Utf8(Utf8Format::Plain),
        LineEnding::CrLf,
    )?;
    assert_eq!(
        (
            summary.bytes,
            summary.characters,
            summary.words,
            summary.lines
        ),
        (34, 25, 4, 3)
    );
    assert_eq!(
        (
            summary.selected_characters,
            summary.selected_bytes,
            summary.selection_ranges
        ),
        (2, 9, 1)
    );
    Ok(())
}

#[test]
fn summary_preserves_source_utf16_mode_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::UnicodeFormat;
    use notepad_matrix::input::LineEnding;

    let text = Rope::from_str("中😀\r\nZ");
    let summary = notepad_matrix::status::summarize(
        &text,
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(9),
        },
        UnicodeFormat::Utf16LeBom,
        LineEnding::CrLf,
    )?;
    assert_eq!(
        (
            summary.bytes,
            summary.characters,
            summary.words,
            summary.lines
        ),
        (10, 8, 2, 2)
    );
    assert_eq!(
        (
            summary.selected_characters,
            summary.selected_bytes,
            summary.selection_ranges
        ),
        (14, 9, 1)
    );
    Ok(())
}

#[test]
#[cfg(target_pointer_width = "64")]
fn summary_utf16_selection_preserves_source_size_t_wrapping_on_mixed_eol()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::UnicodeFormat;
    use notepad_matrix::input::LineEnding;

    for (text, start, end, characters) in
        [("\n", 0, 1, 0), ("\r\n", 1, 2, 0), ("a\r\nb\nc", 4, 5, 2)]
    {
        let summary = notepad_matrix::status::summarize(
            &Rope::from_str(text),
            Selection {
                anchor: ByteOffset(start),
                head: ByteOffset(end),
            },
            UnicodeFormat::Utf16BeBom,
            LineEnding::CrLf,
        )?;
        assert_eq!(
            summary.selected_characters, 18_446_744_073_709_551_614_usize,
            "{text:?}"
        );
        assert_eq!(summary.selected_bytes, 1);
        assert_eq!(summary.selection_ranges, 1);
        assert_eq!(summary.characters, characters);
    }
    Ok(())
}

#[test]
fn summary_text_uses_source_labels_and_grouped_counts() -> Result<(), Box<dyn std::error::Error>> {
    let summary = notepad_matrix::status::DocumentSummary {
        bytes: 12345,
        characters: 1234,
        words: 12,
        lines: 34,
        selected_characters: 1000,
        selected_bytes: 2000,
        selection_ranges: 1,
    };
    assert_eq!(
        summary.description(),
        "Characters (without line endings): 1,234\nWords: 12\nLines: 34\nDocument length: 12,345\n1,000 selected characters (2,000 bytes) in 1 ranges"
    );
    Ok(())
}

#[test]
fn summary_empty_and_eol_only_selection_keep_source_line_and_range_counts()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{UnicodeFormat, Utf8Format};
    use notepad_matrix::input::LineEnding;

    let empty = notepad_matrix::status::summarize(
        &Rope::new(),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(0),
        },
        UnicodeFormat::Utf8(Utf8Format::Plain),
        LineEnding::Lf,
    )?;
    assert_eq!(
        (
            empty.bytes,
            empty.characters,
            empty.words,
            empty.lines,
            empty.selection_ranges
        ),
        (0, 0, 0, 1, 0)
    );
    let eol = notepad_matrix::status::summarize(
        &Rope::from_str("\r\n"),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(2),
        },
        UnicodeFormat::Utf8(Utf8Format::Bom),
        LineEnding::CrLf,
    )?;
    assert_eq!(
        (
            eol.characters,
            eol.selected_characters,
            eol.selected_bytes,
            eol.selection_ranges
        ),
        (0, 0, 2, 1)
    );
    assert!(matches!(
        notepad_matrix::status::summarize(
            &Rope::from_str("中"),
            Selection {
                anchor: ByteOffset(0),
                head: ByteOffset(1)
            },
            UnicodeFormat::Utf8(Utf8Format::Plain),
            LineEnding::Lf,
        ),
        Err(notepad_matrix::editor::EditError::InvalidBoundary)
    ));
    Ok(())
}

#[test]
fn status_tracks_source_byte_length_tab_column_and_unicode_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let text = Rope::from_str("A\t中😀\r\nZ");
    let status = measure(
        &text,
        Selection {
            anchor: ByteOffset(12),
            head: ByteOffset(12),
        },
        4,
    )?;
    assert_eq!(
        (
            status.bytes,
            status.lines,
            status.line,
            status.column,
            status.position
        ),
        (12, 2, 2, 2, 13)
    );
    let status = measure(
        &text,
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(9),
        },
        4,
    )?;
    assert_eq!(
        (
            status.line,
            status.column,
            status.selected_chars,
            status.selected_lines
        ),
        (1, 7, 4, 1)
    );
    Ok(())
}
