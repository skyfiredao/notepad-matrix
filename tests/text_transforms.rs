use notepad_matrix::text_transforms::*;

#[test]
fn line_comment_uses_source_main_selection_scope_not_all_carets() {
    let text = "a\nb";
    let options = TransformOptions {
        main_selection: 1,
        comment_syntax: CommentSyntax {
            line: Some("//".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::Line),
        &[TransformSelection::caret(0), TransformSelection::caret(2)],
        &options,
    )
    .expect("source main selection");
    assert_eq!(apply(text, &plan), "a\n// b");
    assert_eq!(plan.selections, vec![TransformSelection::caret(5)]);
}

#[test]
fn duplicate_selection_and_multiple_carets_on_one_line_keep_each_source_operation() {
    let text = "ab\ncd";
    let plan = plan_transform(
        text,
        TransformCommand::DuplicateSelection,
        &[TransformSelection::new(0, 2)],
        &TransformOptions::default(),
    )
    .expect("duplicate selection");
    assert_eq!(apply(text, &plan), "abab\ncd");
    assert_eq!(plan.selections, vec![TransformSelection::new(0, 2)]);
    let text = "ab";
    let plan = plan_transform(
        text,
        TransformCommand::DuplicateLine,
        &[TransformSelection::caret(0), TransformSelection::caret(1)],
        &TransformOptions::default(),
    )
    .expect("each caret duplicates");
    assert_eq!(apply(text, &plan), "ab\r\nab\r\nab");
    assert_eq!(plan.replacements.len(), 1);
}

fn apply(text: &str, plan: &TransformPlan) -> String {
    let mut result = text.to_owned();
    for edit in plan.replacements.iter().rev() {
        result.replace_range(edit.range.clone(), &edit.text);
    }
    result
}

fn run(text: &str, command: TransformCommand, selections: &[TransformSelection]) -> String {
    let plan = plan_transform(text, command, selections, &TransformOptions::default())
        .expect("valid source vector");
    apply(text, &plan)
}

#[test]
fn source_case_is_selection_only_and_utf16_simple_not_expanding() {
    assert_eq!(
        run(
            "Straße ﬃ é𐐨",
            TransformCommand::Case(Case::Upper),
            &[TransformSelection::caret(0)]
        ),
        "Straße ﬃ é𐐨"
    );
    assert_eq!(
        run(
            "Straße ﬃ é𐐨",
            TransformCommand::Case(Case::Upper),
            &[TransformSelection::new(0, "Straße ﬃ é𐐨".len())]
        ),
        "STRAßE ﬃ É𐐨"
    );
}

#[test]
fn source_whitespace_expands_partial_lines_and_excludes_end_line_start() {
    let text = "  a \r\n\tb \n  c  ";
    assert_eq!(
        run(
            text,
            TransformCommand::Trim(Trim::Both),
            &[TransformSelection::new(2, 10)]
        ),
        "a\r\nb\n  c  "
    );
    assert_eq!(
        run(
            "é\tx\r\n\t z",
            TransformCommand::TabsToSpaces,
            &[TransformSelection::caret(0)]
        ),
        "é   x\r\n     z"
    );
    assert_eq!(
        run(
            "a   b    c",
            TransformCommand::SpacesToTabs {
                leading_only: false
            },
            &[TransformSelection::caret(0)]
        ),
        "a\tb\t c"
    );
    assert_eq!(
        run(
            "    a    b",
            TransformCommand::SpacesToTabs { leading_only: true },
            &[TransformSelection::caret(0)]
        ),
        "\ta    b"
    );
}

#[test]
fn source_line_commands_keep_distinct_scope_and_eol_rules() {
    assert_eq!(
        run(
            "a\r\n b\nc",
            TransformCommand::JoinLines,
            &[TransformSelection::new(1, 7)]
        ),
        "a  b c"
    );
    assert_eq!(
        run(
            "a\nb",
            TransformCommand::JoinLines,
            &[TransformSelection::caret(0)]
        ),
        "a\nb"
    );
    assert_eq!(
        run(
            "a\r\n\tb\nc",
            TransformCommand::EolToSpace,
            &[TransformSelection::caret(1)]
        ),
        "a \tb c"
    );
    assert_eq!(
        run(
            "a\nb",
            TransformCommand::DuplicateLine,
            &[TransformSelection::new(0, 3)]
        ),
        "a\nb\r\nb"
    );
    assert_eq!(
        run(
            "a\r\nb\nc",
            TransformCommand::MoveUp,
            &[TransformSelection::caret(4)]
        ),
        "b\na\r\nc"
    );
    assert_eq!(
        run(
            "a\nb",
            TransformCommand::MoveDown,
            &[TransformSelection::caret(0)]
        ),
        "b\r\na"
    );
    assert_eq!(
        run(
            "a\n\n \r\n\tb\n",
            TransformCommand::RemoveEmptyLines {
                include_whitespace: false
            },
            &[TransformSelection::caret(0)]
        ),
        "a\n \r\n\tb"
    );
    assert_eq!(
        run(
            "a\n \r\n\tb\n",
            TransformCommand::RemoveEmptyLines {
                include_whitespace: true
            },
            &[TransformSelection::caret(0)]
        ),
        "a\n\tb"
    );
    assert_eq!(
        run(
            "a\na",
            TransformCommand::RemoveDuplicates {
                consecutive_only: true
            },
            &[TransformSelection::caret(0)]
        ),
        "a\na"
    );
    assert_eq!(
        run(
            "a\na",
            TransformCommand::RemoveDuplicates {
                consecutive_only: false
            },
            &[TransformSelection::caret(0)]
        ),
        "a"
    );
}

#[test]
fn source_sort_variants_numeric_chunks_decimal_prefixes_and_terminal_empty() {
    let s = [TransformSelection::caret(0)];
    assert_eq!(
        run(
            "b\na\n",
            TransformCommand::Sort {
                kind: Sort::Lexical,
                descending: false
            },
            &s
        ),
        "\na\nb"
    );
    assert_eq!(
        run(
            "b\r\na\nc",
            TransformCommand::Sort {
                kind: Sort::Reverse,
                descending: false
            },
            &s
        ),
        "c\r\na\nb"
    );
    assert_eq!(
        run(
            "x10\nx2\nx001\nx1\nx-2\nx-10",
            TransformCommand::Sort {
                kind: Sort::Integer,
                descending: false
            },
            &s
        ),
        "x-10\nx-2\nx001\nx1\nx2\nx10"
    );
    assert_eq!(
        run(
            "2.5tail\n-3\n \n1.2.3",
            TransformCommand::Sort {
                kind: Sort::DecimalDot,
                descending: false
            },
            &s
        ),
        " \n-3\n1.2.3\n2.5tail"
    );
    assert_eq!(
        run(
            "2,5tail\n-3\n1,2",
            TransformCommand::Sort {
                kind: Sort::DecimalComma,
                descending: true
            },
            &s
        ),
        "2,5tail\n1,2\n-3"
    );
    assert_eq!(
        run(
            "aa\n😀\nb",
            TransformCommand::Sort {
                kind: Sort::Length,
                descending: false
            },
            &s
        ),
        "b\naa\n😀"
    );
    assert_eq!(
        run(
            "z\nA\nb",
            TransformCommand::Sort {
                kind: Sort::CaseInsensitive,
                descending: false
            },
            &s
        ),
        "A\nb\nz"
    );
    let bad = plan_transform(
        "2\n--1",
        TransformCommand::Sort {
            kind: Sort::DecimalDot,
            descending: false,
        },
        &s,
        &TransformOptions::default(),
    );
    assert_eq!(bad, Err(TransformError::InvalidNumber { line: 2 }));
}

#[test]
fn source_indent_and_comment_modes_use_language_metadata_and_per_line_toggle() {
    assert_eq!(
        run(
            "  a\nb",
            TransformCommand::Indent,
            &[TransformSelection::caret(2)]
        ),
        "\t  a\nb"
    );
    assert_eq!(
        run(
            "\ta\n  b\nc",
            TransformCommand::Unindent,
            &[TransformSelection::new(1, 7)]
        ),
        "a\nb\nc"
    );
    let options = TransformOptions {
        comment_syntax: CommentSyntax {
            line: Some("REM".into()),
            stream: Some(("<!--".into(), "-->".into())),
            ..Default::default()
        },
        ..Default::default()
    };
    let text = "  alpha\n rem beta\n \nlast";
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::ToggleLine),
        &[TransformSelection::new(0, 19)],
        &options,
    )
    .expect("toggle");
    assert_eq!(apply(text, &plan), "  REM alpha\n beta\n \nlast");
    let plan = plan_transform(
        "  abc",
        TransformCommand::Comment(Comment::Stream),
        &[TransformSelection::caret(3)],
        &options,
    )
    .expect("stream");
    assert_eq!(apply("  abc", &plan), "  <!-- abc -->");
    assert_eq!(plan.selections, vec![TransformSelection::new(7, 10)]);
    let text = "<!-- one --> and <!-- two -->";
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::UncommentStream),
        &[TransformSelection::new(0, text.len())],
        &options,
    )
    .expect("uncomment");
    assert_eq!(apply(text, &plan), "one and two");
    let plain = plan_transform(
        "hello",
        TransformCommand::Comment(Comment::Line),
        &[TransformSelection::caret(0)],
        &TransformOptions::default(),
    );
    assert_eq!(plain, Err(TransformError::MissingCommentSyntax));
}

#[test]
fn selection_mapping_is_atomic_directional_and_utf8_safe() {
    let text = "é one ÿ two";
    let selections = [TransformSelection::new(2, 0), TransformSelection::new(7, 9)];
    let plan = plan_transform(
        text,
        TransformCommand::Case(Case::Upper),
        &selections,
        &TransformOptions::default(),
    )
    .expect("multi case");
    assert_eq!(apply(text, &plan), "É one Ÿ two");
    assert_eq!(
        plan.selections,
        vec![TransformSelection::new(2, 0), TransformSelection::new(7, 9)]
    );
    assert_eq!(plan.replacements.len(), 2);
    let bad = [
        TransformSelection::new(0, 2),
        TransformSelection::new(3, 100),
    ];
    assert_eq!(
        plan_transform(
            text,
            TransformCommand::Case(Case::Lower),
            &bad,
            &TransformOptions::default()
        ),
        Err(TransformError::InvalidSelection)
    );
    let read_only = TransformOptions {
        read_only: true,
        ..Default::default()
    };
    assert_eq!(
        plan_transform(
            text,
            TransformCommand::Case(Case::Lower),
            &selections,
            &read_only
        ),
        Err(TransformError::ReadOnly)
    );
    let disabled = TransformOptions {
        enabled: false,
        ..Default::default()
    };
    assert_eq!(
        plan_transform(
            text,
            TransformCommand::Case(Case::Lower),
            &selections,
            &disabled
        ),
        Err(TransformError::Disabled)
    );
}

#[test]
fn retained_source_ids_roundtrip_and_auxiliary_line_transforms_are_real() {
    for (name, id, command) in RETAINED_COMMANDS {
        assert!(name.starts_with("IDM_EDIT_"));
        assert_eq!(TransformCommand::from_source_id(*id), Some(*command));
        assert_eq!(command.source_id(), Some(*id));
    }
    assert_eq!(TransformCommand::from_source_id(42001), None);
    assert_eq!(
        TransformCommand::from_source_id(42011),
        Some(TransformCommand::TransposeLine)
    );
    assert_eq!(
        run(
            "a\r\nb\nc",
            TransformCommand::TransposeLine,
            &[TransformSelection::caret(4)]
        ),
        "b\r\na\nc"
    );
    assert_eq!(
        run(
            "a\nb",
            TransformCommand::BlankLineAbove,
            &[TransformSelection::caret(2)]
        ),
        "a\r\n\nb"
    );
    assert_eq!(
        run(
            "a\nb",
            TransformCommand::BlankLineBelow,
            &[TransformSelection::caret(2)]
        ),
        "a\nb\r\n"
    );
    assert_eq!(
        run(
            "é\r\n😀",
            TransformCommand::Redact { bullet: false },
            &[TransformSelection::new(0, 8)]
        ),
        "█\r\n█"
    );
}

#[test]
fn rectangle_key_sort_and_locale_have_distinct_source_rules() {
    let text = "z2\na1\nb1";
    let sels = [
        TransformSelection::new(1, 2),
        TransformSelection::new(4, 5),
        TransformSelection::new(7, 8),
    ];
    let rect = TransformOptions {
        selection_mode: TransformSelectionMode::Rectangle {
            from_column: 1,
            to_column: 2,
        },
        ..Default::default()
    };
    let sort = TransformCommand::Sort {
        kind: Sort::Integer,
        descending: false,
    };
    let plan = plan_transform(text, sort, &sels, &rect).expect("rect sort");
    assert_eq!(apply(text, &plan), "a1\nb1\nz2");
    assert_eq!(plan.selections, sels);
    assert_eq!(
        plan_transform(text, sort, &sels, &TransformOptions::default()),
        Err(TransformError::UnsupportedSelection)
    );
    assert_eq!(
        plan_transform(text, TransformCommand::Trim(Trim::Both), &sels, &rect),
        Err(TransformError::UnsupportedSelection)
    );
    let locale = TransformOptions {
        line_ending: "\n".into(),
        locale_keys: Some(vec![vec![2], vec![1], vec![]]),
        ..Default::default()
    };
    let plan = plan_transform(
        "z\na\n",
        TransformCommand::Sort {
            kind: Sort::Locale,
            descending: false,
        },
        &[TransformSelection::caret(0)],
        &locale,
    )
    .expect("locale");
    assert_eq!(apply("z\na\n", &plan), "a\nz\n");
    assert_eq!(plan.selections, vec![TransformSelection::caret(2)]);
    let selection = [TransformSelection::new(4, 0)];
    let plan = plan_transform(
        "z\na\n",
        TransformCommand::Sort {
            kind: Sort::Locale,
            descending: false,
        },
        &selection,
        &locale,
    )
    .expect("locale selected");
    assert_eq!(plan.selections, vec![TransformSelection::new(4, 0)]);
}

#[test]
fn source_case_sentence_apostrophes_and_split_layout_vectors() {
    let s = |text: &str| [TransformSelection::new(0, text.len())];
    let text = "DON’T doN‘t 2ABC 𐐨abc";
    assert_eq!(
        run(text, TransformCommand::Case(Case::ProperForce), &s(text)),
        "Don’t Don‘t 2abc 𐐨Abc"
    );
    assert_eq!(
        run(
            "hELLO aBC",
            TransformCommand::Case(Case::ProperBlend),
            &s("hELLO aBC")
        ),
        "HELLO ABC"
    );
    assert_eq!(
        run(
            "a.BC! dEF\ngHI\n\njKL i am",
            TransformCommand::Case(Case::SentenceForce),
            &s("a.BC! dEF\ngHI\n\njKL i am")
        ),
        "A.bc! Def\nghi\n\nJkl I am"
    );
    assert_eq!(
        run(
            "hELLO. wORLD",
            TransformCommand::Case(Case::SentenceBlend),
            &s("hELLO. wORLD")
        ),
        "HELLO. WORLD"
    );
    assert_eq!(
        run("Abéẞ𐐨", TransformCommand::Case(Case::Invert), &s("Abéẞ𐐨")),
        "aBÉß𐐨"
    );
    let options = TransformOptions {
        line_ending: "\n".into(),
        split_breaks: vec![4, 8],
        ..Default::default()
    };
    let plan = plan_transform(
        "one two three",
        TransformCommand::SplitLines,
        &[TransformSelection::caret(5)],
        &options,
    )
    .expect("layout breaks");
    assert_eq!(apply("one two three", &plan), "one \ntwo \nthree");
    assert_eq!(plan.selections, vec![TransformSelection::new(0, 15)]);
    assert_eq!(
        run(
            " x \n y \nz",
            TransformCommand::TrimAll,
            &[TransformSelection::new(1, 8)]
        ),
        "x y z"
    );
}

#[test]
fn all_retained_commands_produce_atomic_utf8_safe_plans_at_every_caret() {
    let vectors = [
        "",
        "a",
        "\n",
        "a\n",
        "a\nb",
        "é\nx",
        "x\né",
        "a\r\nb\nc\r",
        " \r\n\t",
        "\n\n",
        "é😀\na",
        "/* a */",
    ];
    for text in vectors {
        let line_count = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .count();
        let options = TransformOptions {
            line_ending: "\n".into(),
            locale_keys: Some((0..line_count).map(|i| vec![i as u8]).collect()),
            comment_syntax: CommentSyntax {
                line: Some("//".into()),
                stream: Some(("/*".into(), "*/".into())),
                ..Default::default()
            },
            ..Default::default()
        };
        for (name, _, command) in RETAINED_COMMANDS {
            for position in text
                .char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(text.len()))
            {
                for selection in [
                    TransformSelection::caret(position),
                    TransformSelection::new(0, text.len()),
                    TransformSelection::new(text.len(), 0),
                ] {
                    let result = plan_transform(text, *command, &[selection], &options);
                    if let Ok(plan) = result {
                        for edit in &plan.replacements {
                            assert!(
                                edit.range.start <= edit.range.end && edit.range.end <= text.len(),
                                "{name} {text:?}"
                            );
                            assert!(
                                text.is_char_boundary(edit.range.start)
                                    && text.is_char_boundary(edit.range.end),
                                "{name} {text:?}"
                            );
                        }
                        assert!(
                            plan.replacements
                                .windows(2)
                                .all(|p| p[0].range.end <= p[1].range.start),
                            "{name} {text:?}"
                        );
                        let output = apply(text, &plan);
                        for selected in plan.selections {
                            assert!(
                                output.is_char_boundary(selected.anchor)
                                    && output.is_char_boundary(selected.caret),
                                "{name} {text:?} {selection:?} => {output:?} {selected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn terminal_empty_line_moves_and_dedup_never_invent_an_eol() {
    let options = TransformOptions {
        line_ending: "\n".into(),
        ..Default::default()
    };
    let text = "a\n";
    let plan = plan_transform(
        text,
        TransformCommand::MoveUp,
        &[TransformSelection::caret(2)],
        &options,
    )
    .expect("move terminal empty");
    assert_eq!(apply(text, &plan), "\na");
    assert_eq!(
        run(
            "\na\n",
            TransformCommand::RemoveDuplicates {
                consecutive_only: false
            },
            &[TransformSelection::caret(0)]
        ),
        "\na"
    );
    assert_eq!(
        run(
            "a\na\n",
            TransformCommand::RemoveDuplicates {
                consecutive_only: true
            },
            &[TransformSelection::caret(0)]
        ),
        "a\n"
    );
}

#[test]
fn multiple_indents_use_scintilla_stops_not_single_caret_delta() {
    let text = "  a\n  b";
    let selections = [TransformSelection::caret(2), TransformSelection::caret(6)];
    let plan = plan_transform(
        text,
        TransformCommand::Indent,
        &selections,
        &TransformOptions::default(),
    )
    .expect("multi indent");
    assert_eq!(apply(text, &plan), "\ta\n\tb");
    assert_eq!(
        plan.selections,
        vec![TransformSelection::caret(1), TransformSelection::caret(4)]
    );
    let text = "abc\ndef";
    let selections = [TransformSelection::caret(2), TransformSelection::caret(6)];
    let plan = plan_transform(
        text,
        TransformCommand::Unindent,
        &selections,
        &TransformOptions::default(),
    )
    .expect("multi backtab");
    assert!(plan.replacements.is_empty());
    assert_eq!(
        plan.selections,
        vec![TransformSelection::caret(0), TransformSelection::caret(4)]
    );
}

#[test]
fn case_byte_deltas_preserve_multi_order_but_single_source_selection_is_forward() {
    let text = "ı ı";
    let selections = [TransformSelection::new(5, 3), TransformSelection::new(2, 0)];
    let plan = plan_transform(
        text,
        TransformCommand::Case(Case::Upper),
        &selections,
        &TransformOptions::default(),
    )
    .expect("shrinking multi");
    assert_eq!(apply(text, &plan), "I I");
    assert_eq!(
        plan.selections,
        vec![TransformSelection::new(3, 2), TransformSelection::new(1, 0)]
    );
    let plan = plan_transform(
        text,
        TransformCommand::Case(Case::Upper),
        &[TransformSelection::new(5, 0)],
        &TransformOptions::default(),
    )
    .expect("single forward");
    assert_eq!(plan.selections, vec![TransformSelection::new(0, 3)]);
}

#[test]
fn language_comment_fallback_advanced_mode_nested_uncomment_and_blank_policies() {
    let stream = TransformOptions {
        comment_syntax: CommentSyntax {
            stream: Some(("<!--".into(), "-->".into())),
            ..Default::default()
        },
        ..Default::default()
    };
    let text = " x\n\n y";
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::ToggleLine),
        &[TransformSelection::new(0, text.len())],
        &stream,
    )
    .expect("advanced");
    assert_eq!(apply(text, &plan), " <!-- x -->\n\n <!-- y -->");
    let wrapped = apply(text, &plan);
    let plan = plan_transform(
        &wrapped,
        TransformCommand::Comment(Comment::ToggleLine),
        &[TransformSelection::new(0, wrapped.len())],
        &stream,
    )
    .expect("advanced undo");
    assert_eq!(apply(&wrapped, &plan), text);
    let text = "<!-- <!-- x --> -->";
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::UncommentStream),
        &[TransformSelection::caret(10)],
        &stream,
    )
    .expect("nested");
    assert_eq!(apply(text, &plan), "x");
    let text = "<!-- unclosed";
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::UncommentStream),
        &[TransformSelection::new(0, text.len())],
        &stream,
    )
    .expect("unmatched");
    assert!(plan.replacements.is_empty());
    let line = TransformOptions {
        comment_syntax: CommentSyntax {
            line: Some("|".into()),
            at_line_start: true,
            comment_empty_lines: true,
            omit_line_space: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let text = " x\n\n";
    let plan = plan_transform(
        text,
        TransformCommand::Comment(Comment::Stream),
        &[TransformSelection::new(0, text.len())],
        &line,
    )
    .expect("line fallback");
    assert_eq!(apply(text, &plan), "| x\n|\n");
}

#[test]
fn malformed_disabled_and_readonly_requests_never_expose_partial_edits() {
    for (_, _, command) in RETAINED_COMMANDS {
        let selection = [TransformSelection::new(0, 3)];
        assert_eq!(
            plan_transform(
                "éx",
                *command,
                &selection,
                &TransformOptions {
                    read_only: true,
                    ..Default::default()
                }
            ),
            Err(TransformError::ReadOnly)
        );
        assert_eq!(
            plan_transform(
                "éx",
                *command,
                &selection,
                &TransformOptions {
                    enabled: false,
                    ..Default::default()
                }
            ),
            Err(TransformError::Disabled)
        );
        assert_eq!(
            plan_transform(
                "éx",
                *command,
                &[TransformSelection::new(0, 1)],
                &TransformOptions::default()
            ),
            Err(TransformError::InvalidSelection)
        );
    }
    let options = TransformOptions {
        split_breaks: vec![2, 1],
        ..Default::default()
    };
    assert_eq!(
        plan_transform(
            "éxy",
            TransformCommand::SplitLines,
            &[TransformSelection::caret(0)],
            &options
        ),
        Err(TransformError::InvalidSplitBreak)
    );
    assert_eq!(
        plan_transform(
            "abc",
            TransformCommand::Case(Case::Upper),
            &[TransformSelection::new(0, 2), TransformSelection::new(1, 3)],
            &TransformOptions::default()
        ),
        Err(TransformError::OverlappingSelections)
    );
    assert_eq!(
        plan_transform(
            "abc",
            TransformCommand::Indent,
            &[TransformSelection::caret(0)],
            &TransformOptions {
                tab_width: 0,
                ..Default::default()
            }
        ),
        Err(TransformError::InvalidTabWidth)
    );
    assert_eq!(
        plan_transform(
            "abc",
            TransformCommand::Indent,
            &[TransformSelection::caret(0)],
            &TransformOptions {
                line_ending: "bad".into(),
                ..Default::default()
            }
        ),
        Err(TransformError::InvalidLineEnding)
    );
}

#[test]
fn random_commands_are_seeded_permutations_or_simple_case_only() {
    let text = "One\nTwo\nThree\nFour";
    let options = TransformOptions {
        random_seed: 9,
        line_ending: "\n".into(),
        ..Default::default()
    };
    let selection = [TransformSelection::new(0, text.len())];
    let plan = plan_transform(
        text,
        TransformCommand::Sort {
            kind: Sort::Random,
            descending: false,
        },
        &selection,
        &options,
    )
    .expect("shuffle");
    let output = apply(text, &plan);
    assert_ne!(output, text);
    let mut actual: Vec<_> = output.split('\n').collect();
    actual.sort();
    assert_eq!(actual, vec!["Four", "One", "Three", "Two"]);
    let plan = plan_transform(
        text,
        TransformCommand::Case(Case::Random),
        &selection,
        &options,
    )
    .expect("random case");
    let output = apply(text, &plan);
    assert_eq!(output.to_lowercase(), text.to_lowercase());
    assert_ne!(output, text);
}
