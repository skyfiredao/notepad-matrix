use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::search::{LiteralOptions, find_literal};

#[test]
fn streaming_unicode_matcher_preserves_overlaps_and_complete_expansion_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    fn fold(text: &str) -> String {
        text.chars()
            .flat_map(|ch| match ch {
                'S' | 's' => "s".chars(),
                'ß' => "ss".chars(),
                'a' => "a".chars(),
                _ => "".chars(),
            })
            .collect()
    }
    let options = LiteralOptions {
        wrap: false,
        ..Default::default()
    };
    for bits in 0..256 {
        let text: String = (0..4)
            .map(|index| ['s', 'S', 'ß', 'a'][(bits >> (index * 2)) & 3])
            .collect();
        for query in ["s", "ss", "sss", "as", "sa", "aß", "ßa"] {
            let needle = fold(query);
            let boundaries: Vec<_> = text
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(text.len()))
                .collect();
            let mut candidates = Vec::new();
            for &start in &boundaries[..boundaries.len() - 1] {
                for &end in boundaries.iter().filter(|&&end| end > start) {
                    if fold(&text[start..end]) == needle {
                        candidates.push(start..end);
                        break;
                    }
                }
            }
            let found = find_literal(&text, query, Selection::default(), options, false)?;
            assert_eq!(
                found.map(|found| found.range.start.0..found.range.end.0),
                candidates.first().cloned(),
                "{text:?}/{query:?}"
            );
            let mut at = 0;
            let mut count = 0;
            for candidate in &candidates {
                if candidate.start >= at {
                    count += 1;
                    at = candidate.end;
                }
            }
            assert_eq!(
                notepad_matrix::search::count_literal(
                    &text,
                    query,
                    Selection::default(),
                    options,
                    false,
                    false
                )?,
                count
            );
        }
    }
    Ok(())
}

#[test]
fn mixed_unicode_repeated_prefix_search_finishes_with_source_results()
-> Result<(), Box<dyn std::error::Error>> {
    let text = format!("{}中", "a".repeat(200000));
    let query = format!("{}b", "a".repeat(4096));
    let started = std::time::Instant::now();
    assert_eq!(
        notepad_matrix::search::count_literal(
            &text,
            &query,
            Selection::default(),
            LiteralOptions::default(),
            false,
            false
        )?,
        0
    );
    eprintln!(
        "mixed_unicode_repeated_prefix_count_seconds={:.6}",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

#[test]
fn unicode_folded_ranges_keep_whole_word_selection_direction_and_extended_semantics()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "xß ß SS İ i\u{307}";
    let options = LiteralOptions {
        whole_word: true,
        ..Default::default()
    };
    let first = find_literal(text, "ss", Selection::default(), options, false)?
        .ok_or("whole word expansion")?;
    assert_eq!(first.range, ByteOffset(4)..ByteOffset(6));
    let backward = find_literal(
        text,
        "ss",
        Selection {
            anchor: ByteOffset(text.len()),
            head: ByteOffset(text.len()),
        },
        options,
        true,
    )?
    .ok_or("backward expansion")?;
    assert_eq!(backward.range, ByteOffset(7)..ByteOffset(9));
    let selection = Selection {
        anchor: ByteOffset(6),
        head: ByteOffset(4),
    };
    assert_eq!(
        notepad_matrix::search::count_literal(text, "ss", selection, options, true, true)?,
        1
    );
    let mut editor = notepad_matrix::input::EditorSession::new(text);
    editor.select(selection)?;
    assert_eq!(
        editor.replace_all_literal_scoped("SS", "X", options, true, true)?,
        1
    );
    assert_eq!(editor.committed_text(), "xß X SS İ i\u{307}");
    assert_eq!(editor.selection().range(), ByteOffset(4)..ByteOffset(5));
    let extended = LiteralOptions {
        mode: notepad_matrix::search::SearchMode::Extended,
        ..Default::default()
    };
    assert_eq!(
        find_literal("ß", r"\u0053\u0053", Selection::default(), extended, false)?
            .ok_or("decoded then folded")?
            .range,
        ByteOffset(0)..ByteOffset(2)
    );
    Ok(())
}

#[test]
fn in_selection_replace_all_overrides_wrap_and_keeps_source_forward_result_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = notepad_matrix::input::EditorSession::new("one one ONE");
    editor.select(Selection {
        anchor: ByteOffset(7),
        head: ByteOffset(4),
    })?;
    assert_eq!(
        editor.replace_all_literal_scoped("one", "X", LiteralOptions::default(), true, true)?,
        1
    );
    assert_eq!(editor.committed_text(), "one X ONE");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(5)
        }
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "one one ONE");
    assert!(!editor.undo());
    Ok(())
}

#[test]
fn normal_count_uses_source_wrap_direction_selection_and_word_edges()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::search::count_literal;
    let selected = Selection {
        anchor: ByteOffset(7),
        head: ByteOffset(4),
    };
    for (wrap, backwards, in_selection, expected) in [
        (true, false, false, 3),
        (false, false, false, 2),
        (false, true, false, 2),
        (true, true, true, 1),
    ] {
        assert_eq!(
            count_literal(
                "one one ONE",
                "one",
                selected,
                LiteralOptions {
                    wrap,
                    ..LiteralOptions::default()
                },
                backwards,
                in_selection
            )?,
            expected
        );
    }
    assert_eq!(
        count_literal(
            "aaaa",
            "aa",
            Selection::default(),
            LiteralOptions::default(),
            false,
            false
        )?,
        2
    );
    assert_eq!(
        count_literal(
            "foo fooX",
            "foo",
            Selection {
                anchor: ByteOffset(0),
                head: ByteOffset(7)
            },
            LiteralOptions {
                whole_word: true,
                ..LiteralOptions::default()
            },
            false,
            true
        )?,
        1
    );
    assert_eq!(
        count_literal(
            "text",
            "",
            Selection::default(),
            LiteralOptions::default(),
            false,
            false
        )?,
        0
    );
    assert!(
        count_literal(
            "😀",
            "x",
            Selection {
                anchor: ByteOffset(1),
                head: ByteOffset(1)
            },
            LiteralOptions::default(),
            false,
            false
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn replace_all_scope_follows_direction_wrap_and_document_word_edges()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::input::EditorSession;
    let options = LiteralOptions {
        match_case: true,
        wrap: false,
        whole_word: false,
        ..LiteralOptions::default()
    };
    for (backwards, wrap, expected) in [
        (true, false, "Z X Z X one"),
        (false, false, "one X Z X Z"),
        (true, true, "Z X Z X Z"),
    ] {
        let mut editor = EditorSession::new("one X one X one");
        editor.select(Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(9),
        })?;
        let count = editor.replace_all_literal_in_direction(
            "one",
            "Z",
            LiteralOptions { wrap, ..options },
            backwards,
        )?;
        assert_eq!(count, if wrap { 3 } else { 2 });
        assert_eq!(editor.committed_text(), expected);
        assert!(editor.undo());
        assert_eq!(editor.committed_text(), "one X one X one");
        assert!(!editor.undo());
    }
    let mut editor = EditorSession::new("foo fooX");
    editor.select(Selection {
        anchor: ByteOffset(0),
        head: ByteOffset(7),
    })?;
    assert_eq!(
        editor.replace_all_literal_in_direction(
            "foo",
            "Z",
            LiteralOptions {
                whole_word: true,
                ..options
            },
            true
        )?,
        1
    );
    assert_eq!(editor.committed_text(), "Z fooX");
    Ok(())
}

#[test]
fn normal_replace_search_rechecks_the_current_selection_in_both_directions()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "one X one";
    let selected = Selection {
        anchor: ByteOffset(6),
        head: ByteOffset(9),
    };
    let options = LiteralOptions {
        match_case: true,
        wrap: false,
        whole_word: false,
        ..LiteralOptions::default()
    };
    for backwards in [false, true] {
        let found = notepad_matrix::search::find_literal_for_replace(
            text, "one", selected, options, backwards,
        )?
        .ok_or("current replacement target")?;
        assert_eq!(found.range, ByteOffset(6)..ByteOffset(9));
    }
    assert_eq!(
        find_literal(text, "one", selected, options, true)?
            .ok_or("previous search target")?
            .range,
        ByteOffset(0)..ByteOffset(3)
    );
    Ok(())
}

#[test]
fn normal_whole_word_matches_source_character_class_edges() -> Result<(), Box<dyn std::error::Error>>
{
    let options = LiteralOptions {
        match_case: true,
        wrap: false,
        whole_word: true,
        ..LiteralOptions::default()
    };
    for (text, query, expected) in [
        ("xfoobar foo! foo_", "foo", Some(8..11)),
        ("a.b ... .", ".", Some(1..2)),
        ("..a.a.", ".a.", Some(3..6)),
        ("foo ", "foo ", None),
        ("中。文", "。", Some(3..6)),
        ("...", "..", None),
    ] {
        let found = find_literal(text, query, Selection::default(), options, false)?;
        assert_eq!(
            found.map(|found| found.range.start.0..found.range.end.0),
            expected,
            "{text:?} / {query:?}"
        );
    }
    let mut editor = notepad_matrix::input::EditorSession::new("..a.a.");
    let previous = find_literal(
        "foo foo_",
        "foo",
        Selection {
            anchor: ByteOffset(8),
            head: ByteOffset(8),
        },
        options,
        true,
    )?
    .ok_or("previous whole word")?;
    assert_eq!(previous.range, ByteOffset(0)..ByteOffset(3));
    assert_eq!(editor.replace_all_literal(".a.", "X", options)?, 1);
    assert_eq!(editor.committed_text(), "..aX");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "..a.a.");
    Ok(())
}

#[test]
fn normal_replace_all_wraps_whole_document_and_is_one_source_mapped_transaction()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = notepad_matrix::input::EditorSession::new("one one ONE");
    editor.move_to_document_edge(true, false)?;
    assert_eq!(
        editor.replace_all_literal("one", "X", LiteralOptions::default())?,
        3
    );
    assert_eq!(editor.committed_text(), "X X X");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(4)
        }
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "one one ONE");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(11),
            head: ByteOffset(11)
        }
    );
    assert!(!editor.undo());
    Ok(())
}

#[test]
fn normal_mode_keeps_metacharacters_literal_and_unicode_ranges_exact()
-> Result<(), Box<dyn std::error::Error>> {
    let found = find_literal(
        "x a.* 中😀",
        "a.*",
        Selection::default(),
        LiteralOptions::default(),
        false,
    )?
    .ok_or("literal metacharacters")?;
    assert_eq!(found.range, ByteOffset(2)..ByteOffset(5));
    let found = find_literal(
        "中😀 中😀",
        "中😀",
        Selection::default(),
        LiteralOptions::default(),
        false,
    )?
    .ok_or("uncased Unicode")?;
    assert_eq!(found.range, ByteOffset(0)..ByteOffset(7));
    assert_eq!(
        find_literal(
            "É",
            "é",
            Selection::default(),
            LiteralOptions::default(),
            false
        )?
        .ok_or("source Unicode fold")?
        .range,
        ByteOffset(0)..ByteOffset(2)
    );
    let exact = LiteralOptions {
        mode: notepad_matrix::search::SearchMode::Normal,
        match_case: true,
        wrap: false,
        whole_word: false,
    };
    assert!(find_literal("É", "é", Selection::default(), exact, false)?.is_none());
    Ok(())
}

#[test]
fn unicode_full_case_folding_matches_complete_document_characters_and_original_byte_ranges()
-> Result<(), Box<dyn std::error::Error>> {
    for (text, query, expected) in [
        ("Straße", "STRASSE", Some(0..7)),
        ("STRASSE", "straße", Some(0..7)),
        ("ß", "s", None),
        ("ß", "ss", Some(0..2)),
        ("İ", "i", None),
        ("İ", "i\u{307}", Some(0..2)),
        ("K", "k", Some(0..3)),
        ("ﬃ", "FFI", Some(0..3)),
        ("Σςσ", "σσσ", Some(0..6)),
        ("ı", "I", None),
        ("𐐀", "𐐨", Some(0..4)),
    ] {
        let actual = find_literal(
            text,
            query,
            Selection::default(),
            LiteralOptions::default(),
            false,
        )?;
        assert_eq!(
            actual.map(|found| found.range.start.0..found.range.end.0),
            expected,
            "{text:?} / {query:?}"
        );
    }
    let text = "É ß SS ß";
    assert_eq!(
        notepad_matrix::search::count_literal(
            text,
            "ss",
            Selection::default(),
            LiteralOptions::default(),
            false,
            false
        )?,
        3
    );
    let mut editor = notepad_matrix::input::EditorSession::new(text);
    assert_eq!(
        editor.replace_all_literal("ss", "X", LiteralOptions::default())?,
        3
    );
    assert_eq!(editor.committed_text(), "É X X X");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), text);
    Ok(())
}

#[test]
fn literal_find_next_previous_and_wrap_use_source_selection_end()
-> Result<(), Box<dyn std::error::Error>> {
    let options = LiteralOptions::default();
    let text = "one TWO one";
    let first =
        find_literal(text, "one", Selection::default(), options, false)?.ok_or("first match")?;
    assert_eq!(first.range, ByteOffset(0)..ByteOffset(3));
    let selected = Selection {
        anchor: first.range.start,
        head: first.range.end,
    };
    let next = find_literal(text, "one", selected, options, false)?.ok_or("next match")?;
    assert_eq!(next.range, ByteOffset(8)..ByteOffset(11));
    let selected = Selection {
        anchor: next.range.start,
        head: next.range.end,
    };
    let previous = find_literal(text, "one", selected, options, true)?.ok_or("previous match")?;
    assert_eq!(previous.range, ByteOffset(0)..ByteOffset(3));
    let wrapped = find_literal(text, "one", selected, options, false)?.ok_or("wrap")?;
    assert_eq!(wrapped.range, ByteOffset(0)..ByteOffset(3));
    assert!(wrapped.wrapped);
    let insensitive = find_literal(text, "two", Selection::default(), options, false)?
        .ok_or("source default case-insensitive normal match")?;
    assert_eq!(insensitive.range, ByteOffset(4)..ByteOffset(7));
    assert!(find_literal(text, "missing", selected, options, false)?.is_none());
    Ok(())
}
#[test]
fn extended_sequences_follow_source_fixed_width_and_literal_fallback() {
    use notepad_matrix::search::decode_extended;
    assert_eq!(decode_extended(r"a\r\n\t\0\\z"), "a\r\n\t\0\\z");
    assert_eq!(decode_extended(r"\b01000001\o101\d065\x41\u0041"), "AAAAA");
    assert_eq!(decode_extended(r"\d999\o777\u4e2d\uD83D\uDE00"), "ϧǿ中😀");
    assert_eq!(
        decode_extended(r"\x4\u123\b0102abcd\q\"),
        r"\x4\u123\b0102abcd\q\"
    );
    assert_eq!(decode_extended(r"\\n\x414"), "\\nA4");
    assert_eq!(decode_extended(r"\uD800x\uDC00"), "�x�");
    assert_eq!(decode_extended("\u{1f600}\\u0141"), "😀Ł");
    assert_eq!(decode_extended("\\xŁ1"), "¡");
}

#[test]
fn extended_find_count_and_replace_use_decoded_lengths_without_redecoding()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::search::{SearchMode, count_literal};
    let options = LiteralOptions {
        mode: SearchMode::Extended,
        match_case: true,
        ..Default::default()
    };
    let text = "a\r\nb\0c\\n";
    let found =
        find_literal(text, r"\r\n", Selection::default(), options, false)?.ok_or("CRLF match")?;
    assert_eq!(found.range, ByteOffset(1)..ByteOffset(3));
    assert_eq!(
        count_literal(text, r"\0", Selection::default(), options, false, false)?,
        1
    );
    let literal_slash =
        find_literal(text, r"\\n", Selection::default(), options, false)?.ok_or("literal slash")?;
    assert_eq!(literal_slash.range, ByteOffset(6)..ByteOffset(8));
    let mut editor = notepad_matrix::input::EditorSession::new("a\r\nb\r\nc");
    assert_eq!(editor.replace_all_literal(r"\r\n", r"\0\t", options)?, 2);
    assert_eq!(editor.committed_text(), "a\0\tb\0\tc");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a\r\nb\r\nc");
    assert!(!editor.document().can_undo());
    Ok(())
}

#[test]
fn extended_hex_punctuation_remains_literal_in_search_and_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::search::{SearchMode, decode_extended};
    let options = LiteralOptions {
        mode: SearchMode::Extended,
        match_case: true,
        ..Default::default()
    };
    for query in [
        r"\x:1", r"\u::::", r"\x;1", r"\x<1", r"\x=1", r"\x>1", r"\x?1",
    ] {
        assert_eq!(decode_extended(query), query);
        let text = format!("x{query}y");
        assert_eq!(
            find_literal(&text, query, Selection::default(), options, false)?
                .ok_or("literal invalid escape")?
                .range,
            ByteOffset(1)..ByteOffset(1 + query.len())
        );
        let mut editor = notepad_matrix::input::EditorSession::new("needle");
        assert_eq!(editor.replace_all_literal("needle", query, options)?, 1);
        assert_eq!(editor.committed_text(), query);
    }
    Ok(())
}
