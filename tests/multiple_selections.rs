use notepad_matrix::editor::{ByteOffset, Document, Selection, SelectionRange, SelectionSet};

fn selection(anchor: usize, head: usize) -> Selection {
    Selection {
        anchor: ByteOffset(anchor),
        head: ByteOffset(head),
    }
}

#[test]
fn per_view_directional_selections_survive_atomic_edit_and_history() {
    let mut doc = Document::new("one two three");
    doc.set_selections(
        SelectionSet::new(
            vec![
                SelectionRange::from(selection(3, 0)),
                SelectionRange::from(selection(8, 13)),
            ],
            1,
        )
        .unwrap(),
    )
    .unwrap();
    let mut peer = doc.clone_view();
    peer.select(selection(4, 7)).unwrap();
    doc.replace_selections("X").unwrap();
    assert_eq!(doc.text(), "X two X");
    assert_eq!(
        doc.selections()
            .ranges()
            .iter()
            .map(|r| r.selection)
            .collect::<Vec<_>>(),
        vec![selection(1, 1), selection(7, 7)]
    );
    assert_eq!(doc.selections().primary_index(), 1);
    assert_eq!(peer.selection(), selection(2, 5));
    assert!(peer.undo());
    assert_eq!(doc.text(), "one two three");
    assert_eq!(doc.selection(), selection(8, 13));
    assert_eq!(doc.selections().ranges()[0].selection, selection(3, 0));
    assert!(doc.redo());
    assert_eq!(doc.text(), "X two X");
    assert!(!doc.redo());
}

#[test]
fn session_typing_edits_every_selection_and_coalesces_only_direct_runs() {
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::new("ab cd");
    editor.select(selection(1, 1)).unwrap();
    editor.type_text("X").unwrap();
    editor.type_text("Y").unwrap();
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab cd");
    assert!(!editor.undo());
    editor
        .set_selections(
            SelectionSet::new(vec![selection(1, 1).into(), selection(4, 4).into()], 1).unwrap(),
        )
        .unwrap();
    editor.type_text("Q").unwrap();
    assert_eq!(editor.committed_text(), "aQb cQd");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab cd");
}

#[test]
fn rectangle_uses_tab_columns_and_realizes_only_needed_virtual_space() {
    use notepad_matrix::editor::ColumnPosition;
    let mut doc = Document::new("\tab\nx\n😀abc");
    doc.select_rectangle(
        ColumnPosition { line: 2, column: 5 },
        ColumnPosition { line: 0, column: 6 },
        4,
    )
    .unwrap();
    let set = doc.selections();
    assert!(set.is_rectangular());
    assert_eq!(set.primary_index(), 0);
    assert_eq!(set.ranges()[0].selection, selection(2, 3));
    assert_eq!(
        (set.ranges()[1].anchor_virtual, set.ranges()[1].head_virtual),
        (4, 5)
    );
    doc.replace_selections("Q").unwrap();
    assert_eq!(doc.text(), "\taQ\nx    Q\n😀abc Q");
    assert!(doc.undo());
    assert_eq!(doc.selections(), set);
}

#[test]
fn multi_delete_keeps_eols_and_backspace_uses_scalar_crlf_and_virtual_boundaries() {
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::new("a\r\nb😀e\u{301}");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(1, 1).into(), selection(4, 4).into()], 1).unwrap(),
        )
        .unwrap();
    editor.delete_forward().unwrap();
    assert_eq!(editor.committed_text(), "a\r\nbe\u{301}");
    assert!(editor.undo());
    editor.select(selection(3, 3)).unwrap();
    editor.backspace().unwrap();
    assert_eq!(editor.committed_text(), "ab😀e\u{301}");
    assert!(editor.undo());
    editor.select(selection(11, 11)).unwrap();
    editor.backspace().unwrap();
    assert_eq!(editor.committed_text(), "a\r\nb😀e");
    assert!(editor.undo());
}

#[test]
fn column_number_formats_repeat_fill_and_selected_text_are_one_undo() {
    use notepad_matrix::editor::{ColumnBase, ColumnFill, ColumnNumbers, ColumnPosition};
    let mut doc = Document::new("abc\nx\n😀z\nlast");
    doc.select_rectangle(
        ColumnPosition { line: 0, column: 2 },
        ColumnPosition { line: 3, column: 3 },
        4,
    )
    .unwrap();
    let before = doc.selections();
    doc.column_numbers(
        ColumnNumbers {
            start: 15,
            step: 1,
            repeat: 2,
            base: ColumnBase::HexadecimalUpper,
            fill: ColumnFill::Zero,
        },
        4,
    )
    .unwrap();
    assert_eq!(doc.text(), "ab0F\nx 0F\n😀z10\nla10t");
    assert_eq!(doc.selections().ranges()[0].selection, selection(2, 4));
    assert!(doc.undo());
    assert_eq!(doc.text(), "abc\nx\n😀z\nlast");
    assert_eq!(doc.selections(), before);
    assert!(!doc.undo());
    doc.select(selection(2, 2)).unwrap();
    doc.column_text("Q", 4).unwrap();
    assert_eq!(doc.text(), "abQc\nx Q\n😀zQ\nlaQst");
    assert!(doc.undo());
    assert_eq!(doc.text(), "abc\nx\n😀z\nlast");
}

#[test]
fn occurrence_skip_remove_rotate_and_swap_follow_source_selection_order() {
    use notepad_matrix::editor::OccurrenceOptions;
    let mut doc = Document::new("one ONE stone one");
    doc.select(selection(1, 1)).unwrap();
    assert!(
        doc.select_next_occurrence(OccurrenceOptions {
            match_case: false,
            whole_word: true
        })
        .unwrap()
    );
    assert_eq!(doc.selection(), selection(0, 3));
    assert!(
        doc.select_next_occurrence(OccurrenceOptions {
            match_case: false,
            whole_word: true
        })
        .unwrap()
    );
    assert_eq!(doc.selection(), selection(4, 7));
    assert!(doc.skip_occurrence().unwrap());
    assert_eq!(doc.selection(), selection(14, 17));
    assert_eq!(doc.selections().ranges().len(), 2);
    doc.rotate_selection();
    assert_eq!(doc.selection(), selection(0, 3));
    doc.swap_main_anchor_caret();
    assert_eq!(doc.selection(), selection(3, 0));
    doc.remove_last_selection();
    assert_eq!(doc.selections().ranges().len(), 1);
    assert_eq!(doc.selection(), selection(3, 0));
    assert!(!doc.can_undo());
}

#[test]
fn clipboard_copies_in_selection_order_and_cut_is_atomic_and_readonly_safe() {
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::new("one two three");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(13, 8).into(), selection(0, 3).into()], 1).unwrap(),
        )
        .unwrap();
    let payload = editor.clipboard_text();
    assert_eq!(payload.text, "three\r\none\r\n");
    assert!(!payload.line_copy);
    editor.set_read_only(true);
    assert!(editor.cut_selection().is_err());
    assert_eq!(editor.committed_text(), "one two three");
    editor.set_read_only(false);
    assert_eq!(editor.cut_selection().unwrap().text, payload.text);
    assert_eq!(editor.committed_text(), " two ");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "one two three");
    assert!(!editor.undo());
    editor
        .paste_clipboard(&notepad_matrix::clipboard::ClipboardText {
            text: "Q\nR".into(),
            line_copy: false,
        })
        .unwrap();
    assert_eq!(editor.committed_text(), "Q\r\nR two Q\r\nR");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "one two three");
}

#[test]
fn drop_move_copy_overlap_and_context_selection_keep_atomic_history() {
    let mut doc = Document::new("abc DEF xyz");
    doc.select(selection(4, 7)).unwrap();
    let mut peer = doc.clone_view();
    peer.select(selection(8, 11)).unwrap();
    doc.context_select(ByteOffset(5)).unwrap();
    assert_eq!(doc.selection(), selection(4, 7));
    assert!(doc.drop_selection(ByteOffset(11), false).unwrap());
    assert_eq!(doc.text(), "abc  xyzDEF");
    assert_eq!(doc.selection(), selection(8, 11));
    assert_eq!(peer.selection(), selection(5, 8));
    assert!(doc.undo());
    assert_eq!(doc.text(), "abc DEF xyz");
    assert_eq!(doc.selection(), selection(4, 7));
    assert!(!doc.undo());
    assert!(!doc.drop_selection(ByteOffset(5), false).unwrap());
    assert_eq!(doc.selection(), selection(5, 5));
    doc.select(selection(4, 7)).unwrap();
    assert!(doc.drop_selection(ByteOffset(7), true).unwrap());
    assert_eq!(doc.text(), "abc DEFDEF xyz");
    assert!(doc.undo());
    doc.set_read_only(true);
    assert!(doc.drop_selection(ByteOffset(0), false).is_err());
    assert_eq!(doc.selection(), selection(4, 7));
    doc.context_select(ByteOffset(1)).unwrap();
    assert_eq!(doc.selection(), selection(1, 1));
}

#[test]
fn explicit_transaction_selection_policy_validates_before_mutation_and_restores_peers() {
    use notepad_matrix::editor::{EditSelectionPolicy, TextTransform};
    let mut doc = Document::new("abc def");
    doc.set_selections(
        SelectionSet::new(vec![selection(3, 0).into(), selection(4, 7).into()], 1).unwrap(),
    )
    .unwrap();
    let before = doc.selections();
    let mut peer = doc.clone_view();
    peer.select(selection(0, 7)).unwrap();
    let transforms = vec![
        TextTransform {
            range: ByteOffset(0)..ByteOffset(3),
            text: "😀".into(),
        },
        TextTransform {
            range: ByteOffset(4)..ByteOffset(7),
            text: "Q".into(),
        },
    ];
    assert!(
        doc.apply_multi(
            &transforms,
            EditSelectionPolicy::Explicit(SelectionSet::single(selection(1, 1)))
        )
        .is_err()
    );
    assert_eq!(doc.text(), "abc def");
    assert!(!doc.can_undo());
    let after = SelectionSet::new(vec![selection(4, 0).into(), selection(5, 6).into()], 0).unwrap();
    doc.apply_multi(&transforms, EditSelectionPolicy::Explicit(after.clone()))
        .unwrap();
    assert_eq!(doc.text(), "😀 Q");
    assert_eq!(doc.selections(), after);
    assert!(peer.undo());
    assert_eq!(doc.selections(), before);
    assert_eq!(peer.selection(), selection(0, 7));
    assert!(!doc.undo());
    assert!(doc.redo());
    assert_eq!(doc.selections(), after);
}

#[test]
fn explicit_primary_edit_maps_secondary_ranges_and_peer_virtual_space() {
    let mut doc = Document::new("abc\nx");
    doc.set_selections(
        SelectionSet::new(vec![selection(0, 0).into(), selection(4, 5).into()], 0).unwrap(),
    )
    .unwrap();
    let mut peer = doc.clone_view();
    peer.set_selections(
        SelectionSet::new(
            vec![SelectionRange {
                selection: selection(5, 5),
                anchor_virtual: 3,
                head_virtual: 3,
            }],
            0,
        )
        .unwrap(),
    )
    .unwrap();
    doc.replace(ByteOffset(0)..ByteOffset(0), "QQ").unwrap();
    assert_eq!(doc.selections().ranges()[1].selection, selection(6, 7));
    doc.replace(ByteOffset(7)..ByteOffset(7), " ").unwrap();
    assert_eq!(peer.selection(), selection(8, 8));
    assert_eq!(peer.selections().primary().head_virtual, 2);
    assert!(doc.undo());
    assert_eq!(peer.selection(), selection(7, 7));
    assert_eq!(peer.selections().primary().head_virtual, 3);
}

#[test]
fn rectangular_clipboard_pads_short_lines_appends_rows_and_undoes_once() {
    use notepad_matrix::editor::ColumnPosition;
    use notepad_matrix::input::EditorSession;
    let mut source = EditorSession::new("abc\ndef");
    source
        .select_rectangle(
            ColumnPosition { line: 0, column: 1 },
            ColumnPosition { line: 1, column: 3 },
            4,
        )
        .unwrap();
    let payload = source.clipboard_payload();
    assert!(payload.rectangular);
    assert_eq!(payload.text, "bc\r\nef\r\n");
    let mut target = EditorSession::new("ab\nx");
    target.select(selection(2, 2)).unwrap();
    target.paste_payload(&payload, 4).unwrap();
    assert_eq!(target.committed_text(), "abbc\nx ef");
    assert_eq!(target.selection(), selection(2, 2));
    assert!(target.undo());
    assert_eq!(target.committed_text(), "ab\nx");
    target.select(selection(4, 4)).unwrap();
    target.paste_payload(&payload, 4).unwrap();
    assert_eq!(target.committed_text(), "ab\nxbc\r\n ef");
    assert!(target.undo());
    assert_eq!(target.committed_text(), "ab\nx");
    assert!(!target.undo());
}

#[test]
fn rectangle_drag_moves_rows_without_flattening_and_has_one_history_entry() {
    use notepad_matrix::editor::ColumnPosition;
    let mut doc = Document::new("abc---\ndef---");
    doc.select_rectangle(
        ColumnPosition { line: 0, column: 0 },
        ColumnPosition { line: 1, column: 2 },
        4,
    )
    .unwrap();
    let before = doc.selections();
    assert!(doc.drop_selection(ByteOffset(6), false).unwrap());
    assert_eq!(doc.text(), "c---ab\nf---de");
    assert_eq!(doc.selection(), selection(4, 4));
    assert!(doc.undo());
    assert_eq!(doc.text(), "abc---\ndef---");
    assert_eq!(doc.selections(), before);
    assert!(!doc.undo());
}

#[test]
fn select_all_expands_empty_primary_then_adds_all_source_matches_with_flags() {
    use notepad_matrix::editor::OccurrenceOptions;
    let mut doc = Document::new("one ONE stone one");
    doc.select(selection(1, 1)).unwrap();
    assert!(
        doc.select_all_occurrences(OccurrenceOptions {
            match_case: false,
            whole_word: true
        })
        .unwrap()
    );
    assert_eq!(
        doc.selections()
            .ranges()
            .iter()
            .map(|range| range.selection)
            .collect::<Vec<_>>(),
        vec![selection(0, 3), selection(4, 7), selection(14, 17)]
    );
    assert!(!doc.can_undo());
    let mut unicode = Document::new("Straße STRASSE straße");
    unicode.select(selection(0, 7)).unwrap();
    assert!(
        unicode
            .select_all_occurrences(OccurrenceOptions {
                match_case: false,
                whole_word: true
            })
            .unwrap()
    );
    assert_eq!(unicode.selections().ranges().len(), 3);
}

#[test]
fn undo_preserves_primary_virtual_space_even_when_its_byte_position_never_moved() {
    use notepad_matrix::editor::TextTransform;
    let mut doc = Document::new("a\nb");
    let set = SelectionSet::new(
        vec![
            SelectionRange {
                selection: selection(1, 1),
                anchor_virtual: 3,
                head_virtual: 3,
            },
            selection(2, 3).into(),
        ],
        0,
    )
    .unwrap();
    doc.set_selections(set.clone()).unwrap();
    doc.apply_transforms(&[TextTransform {
        range: ByteOffset(2)..ByteOffset(3),
        text: "Q".into(),
    }])
    .unwrap();
    assert!(doc.undo());
    assert_eq!(doc.selections(), set);
    assert!(doc.redo());
    assert_eq!(doc.selections().primary().head_virtual, 3);
}

#[test]
fn shaped_row_rectangle_constructor_keeps_per_row_hit_test_ranges() {
    let mut doc = Document::new("wide\nshort");
    let rectangle =
        SelectionSet::rectangle(vec![selection(1, 4).into(), selection(5, 7).into()], 1).unwrap();
    doc.set_selections(rectangle).unwrap();
    assert!(doc.selections().is_rectangular());
    assert_eq!(doc.selections().primary_index(), 1);
    assert_eq!(doc.selections().ranges()[1].selection, selection(5, 7));
    assert!(SelectionSet::rectangle(Vec::new(), 0).is_err());
}

#[test]
fn column_number_replacement_keeps_reversed_all_virtual_anchor_direction() {
    use notepad_matrix::editor::{ColumnBase, ColumnFill, ColumnNumbers, ColumnPosition};
    let mut doc = Document::new("x\ny");
    doc.select_rectangle(
        ColumnPosition { line: 0, column: 4 },
        ColumnPosition { line: 1, column: 2 },
        4,
    )
    .unwrap();
    doc.column_numbers(
        ColumnNumbers {
            start: 1,
            step: 1,
            repeat: 1,
            base: ColumnBase::Decimal,
            fill: ColumnFill::None,
        },
        4,
    )
    .unwrap();
    assert_eq!(doc.text(), "x 1\ny 2");
    assert_eq!(doc.selections().ranges()[0].selection, selection(3, 2));
    assert_eq!(doc.selections().ranges()[1].selection, selection(7, 6));
}

#[test]
fn virtual_peer_mapping_never_leaves_a_caret_inside_inserted_utf8() {
    let mut doc = Document::new("a\nb");
    let mut peer = doc.clone_view();
    peer.set_selections(
        SelectionSet::new(
            vec![SelectionRange {
                selection: selection(1, 1),
                anchor_virtual: 1,
                head_virtual: 1,
            }],
            0,
        )
        .unwrap(),
    )
    .unwrap();
    doc.replace(ByteOffset(1)..ByteOffset(1), "😀").unwrap();
    assert_eq!(peer.selection(), selection(5, 5));
    assert!(peer.to_utf16(peer.selection().head).is_ok());
    assert_eq!(peer.selections().primary().head_virtual, 0);
    assert!(doc.undo());
    assert_eq!(peer.selection(), selection(1, 1));
    assert_eq!(peer.selections().primary().head_virtual, 1);
}

#[test]
fn occurrence_casefold_handles_source_symbols_ligatures_and_cherokee() {
    use notepad_matrix::editor::OccurrenceOptions;
    for (text, end) in [("µ μ", 2), ("ϐ β", 2), ("Ꭰ ꭰ", 3), ("ﬓ մն", 3), ("ı I", 2)] {
        let mut doc = Document::new(text);
        doc.select(selection(0, end)).unwrap();
        doc.select_all_occurrences(OccurrenceOptions {
            match_case: false,
            whole_word: true,
        })
        .unwrap();
        assert_eq!(
            doc.selections().ranges().len(),
            if text == "ı I" { 1 } else { 2 },
            "{text}"
        );
    }
}

#[test]
fn occurrence_word_expansion_includes_script_marks_but_not_alphabetic_symbols() {
    use notepad_matrix::editor::OccurrenceOptions;
    let mut doc = Document::new("क् क्");
    assert!(
        doc.select_all_occurrences(OccurrenceOptions {
            match_case: true,
            whole_word: true
        })
        .unwrap()
    );
    assert_eq!(
        doc.selections()
            .ranges()
            .iter()
            .map(|range| range.selection)
            .collect::<Vec<_>>(),
        vec![selection(0, 6), selection(7, 13)]
    );
    let mut symbols = Document::new("Ⓐ Ⓐ");
    assert!(
        !symbols
            .select_next_occurrence(OccurrenceOptions::default())
            .unwrap()
    );
    assert_eq!(symbols.selection(), selection(0, 0));
}

#[test]
fn column_no_leading_fill_uses_source_trailing_spaces_to_common_digit_width() {
    use notepad_matrix::editor::{ColumnBase, ColumnFill, ColumnNumbers};
    for (base, start, expected) in [
        (ColumnBase::Decimal, 8, "8 a\n9 b\n10c"),
        (ColumnBase::Binary, 3, "11 a\n100b\n101c"),
        (ColumnBase::Octal, 7, "7 a\n10b\n11c"),
        (ColumnBase::Hexadecimal, 15, "f a\n10b\n11c"),
        (ColumnBase::HexadecimalUpper, 15, "F a\n10b\n11c"),
    ] {
        let mut doc = Document::new("a\nb\nc");
        doc.column_numbers(
            ColumnNumbers {
                start,
                step: 1,
                repeat: 0,
                base,
                fill: ColumnFill::None,
            },
            4,
        )
        .unwrap();
        assert_eq!(doc.text(), expected);
        assert!(doc.undo());
        assert_eq!(doc.text(), "a\nb\nc");
        assert!(!doc.undo());
    }
}

#[test]
fn document_metadata_operations_do_not_clear_virtual_primary_selection() {
    let mut doc = Document::new("x");
    let original = SelectionSet::new(
        vec![SelectionRange {
            selection: selection(1, 1),
            anchor_virtual: 2,
            head_virtual: 4,
        }],
        0,
    )
    .unwrap();
    doc.set_selections(original.clone()).unwrap();
    doc.set_read_only(true);
    assert_eq!(doc.selections(), original);
    doc.set_read_only(false);
    doc.toggle_bookmark();
    doc.mark_saved();
    let saved = doc.save_snapshot();
    doc.acknowledge_save(&saved).unwrap();
    assert_eq!(doc.selections(), original);
}

#[test]
fn one_row_rectangle_column_dialog_has_no_source_column_mode_infos() {
    use notepad_matrix::editor::{ColumnBase, ColumnFill, ColumnNumbers, ColumnPosition};
    let mut doc = Document::new("abc\ndef");
    doc.select_rectangle(
        ColumnPosition { line: 0, column: 1 },
        ColumnPosition { line: 0, column: 2 },
        4,
    )
    .unwrap();
    let original = doc.selections();
    doc.column_text("Q", 4).unwrap();
    doc.column_numbers(
        ColumnNumbers {
            start: 1,
            step: 1,
            repeat: 1,
            base: ColumnBase::Decimal,
            fill: ColumnFill::None,
        },
        4,
    )
    .unwrap();
    assert_eq!(doc.text(), "abc\ndef");
    assert_eq!(doc.selections(), original);
    assert!(!doc.can_undo());
}

#[test]
fn next_occurrence_wrap_readds_existing_match_as_primary_before_skip_removes_previous() {
    use notepad_matrix::editor::OccurrenceOptions;
    let mut doc = Document::new("one one");
    doc.select(selection(0, 3)).unwrap();
    doc.select_next_occurrence(OccurrenceOptions {
        match_case: true,
        whole_word: false,
    })
    .unwrap();
    assert_eq!(doc.selection(), selection(4, 7));
    assert!(
        doc.select_next_occurrence(OccurrenceOptions {
            match_case: true,
            whole_word: false
        })
        .unwrap()
    );
    assert_eq!(doc.selection(), selection(0, 3));
    assert_eq!(doc.selections().ranges().len(), 2);
    assert!(doc.skip_occurrence().unwrap());
    assert_eq!(doc.selection(), selection(4, 7));
    assert_eq!(doc.selections().ranges().len(), 1);
}

#[test]
fn explicit_transaction_policy_does_not_silently_change_selection_identity_or_primary() {
    use notepad_matrix::editor::{EditSelectionPolicy, TextTransform};
    let mut doc = Document::new("abc");
    let explicit =
        SelectionSet::new(vec![selection(1, 1).into(), selection(1, 1).into()], 1).unwrap();
    doc.apply_multi(
        &[TextTransform {
            range: ByteOffset(0)..ByteOffset(0),
            text: "X".into(),
        }],
        EditSelectionPolicy::Explicit(explicit.clone()),
    )
    .unwrap();
    assert_eq!(doc.selections(), explicit);
    assert!(doc.undo());
    assert!(doc.redo());
    assert_eq!(doc.selections(), explicit);
}

#[test]
fn typing_source_overlapping_ranges_uses_evolving_carets_instead_of_transform_rejection() {
    use notepad_matrix::input::EditorSession;
    let mut editor = EditorSession::new("abcdef");
    let original =
        SelectionSet::new(vec![selection(1, 4).into(), selection(2, 5).into()], 1).unwrap();
    editor.set_selections(original.clone()).unwrap();
    editor.type_text("X").unwrap();
    assert_eq!(editor.committed_text(), "aXXf");
    assert_eq!(editor.selections().ranges()[0].selection, selection(2, 2));
    assert_eq!(editor.selection(), selection(3, 3));
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "abcdef");
    assert_eq!(editor.selections(), original);
    assert!(!editor.undo());
    editor.backspace().unwrap();
    assert_eq!(editor.committed_text(), "af");
    assert!(editor.undo());
    assert_eq!(editor.selections(), original);
}

#[test]
fn tentative_drag_restores_saved_ranges_and_keyboard_extension_trims_in_source_order() {
    let mut doc = Document::new("abcdefghi");
    doc.select(selection(1, 4)).unwrap();
    doc.tentative_selection(selection(6, 3).into()).unwrap();
    assert_eq!(doc.selections().ranges()[0].selection, selection(1, 3));
    assert_eq!(doc.selection(), selection(6, 3));
    doc.tentative_selection(selection(6, 5).into()).unwrap();
    assert_eq!(doc.selections().ranges()[0].selection, selection(1, 4));
    doc.commit_tentative_selection();
    let moved = SelectionSet::new(vec![selection(1, 1).into(), selection(3, 3).into()], 1)
        .unwrap()
        .moved(true, |_, range| {
            Ok(Selection {
                anchor: range.selection.anchor,
                head: ByteOffset(range.selection.head.0 + 3),
            }
            .into())
        })
        .unwrap();
    assert_eq!(moved.ranges()[0].selection, selection(1, 3));
    assert_eq!(moved.ranges()[1].selection, selection(3, 6));
    doc.set_selections(moved).unwrap();
    doc.replace_selections("X").unwrap();
    assert_eq!(doc.text(), "aXXghi");
    assert!(doc.undo());
    assert_eq!(doc.text(), "abcdefghi");
}
