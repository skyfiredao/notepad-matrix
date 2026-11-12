use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::input::EditorSession;

#[test]
fn save_receipt_excludes_preedit_and_does_not_mark_later_edits_saved()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("");
    editor.insert_text("committed")?;
    editor.set_composition("preview")?;
    let saving = editor.save_snapshot();
    assert_eq!(saving.text(), "committed");
    editor.cancel_composition();
    editor.insert_text("later")?;
    editor.acknowledge_save(&saving)?;
    assert!(editor.is_modified());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "committed");
    assert!(!editor.is_modified());
    let mut other = EditorSession::new("");
    assert!(other.acknowledge_save(&saving).is_err());
    Ok(())
}

#[test]
fn default_home_toggles_indentation_and_end_excludes_crlf() -> Result<(), Box<dyn std::error::Error>>
{
    let mut editor = EditorSession::new(" 	中😀\r\nZ");
    editor.move_to_document_edge(false, false)?;
    editor.move_to_line_edge(false, false)?;
    assert_eq!(editor.selection().head, ByteOffset(2));
    editor.move_to_line_edge(false, false)?;
    assert_eq!(editor.selection().head, ByteOffset(0));
    editor.move_to_line_edge(true, true)?;
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(9)
        }
    );
    editor.move_to_line_edge(false, true)?;
    assert_eq!(editor.selection().head, ByteOffset(2));
    editor.move_to_document_edge(true, false)?;
    editor.move_to_line_edge(false, false)?;
    assert_eq!(editor.selection().head, ByteOffset(11));
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    Ok(())
}

#[test]
fn document_edges_use_preview_coordinates_and_preserve_redo()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("Z");
    editor.set_composition("abcdef")?;
    let revision = editor.display_revision();
    editor.move_to_document_edge(false, true)?;
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(0)
        }
    );
    assert_eq!(editor.marked_range(), Some(ByteOffset(0)..ByteOffset(6)));
    assert_eq!(editor.display_revision(), revision);
    assert_eq!(editor.committed_text(), "Z");
    editor.insert_text("final")?;
    assert!(editor.undo());
    editor.move_to_document_edge(true, false)?;
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "finalZ");
    Ok(())
}

#[test]
fn document_edge_navigation_preserves_selection_direction_without_history()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("A😀\r\n中");
    editor.move_to_document_edge(true, false)?;
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(10),
            head: ByteOffset(10)
        }
    );
    editor.move_to_document_edge(false, true)?;
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(10),
            head: ByteOffset(0)
        }
    );
    editor.move_to_document_edge(false, false)?;
    editor.move_to_document_edge(true, true)?;
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(10)
        }
    );
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    let mut empty = EditorSession::new("");
    empty.move_to_document_edge(true, true)?;
    assert_eq!(empty.selection(), Selection::default());
    Ok(())
}

#[test]
fn empty_edit_without_composition_keeps_display_revision_and_history()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("large document");
    let revision = editor.display_revision();
    editor.insert_text("")?;
    assert_eq!(revision, editor.display_revision());
    assert!(!editor.undo());
    assert!(!editor.is_modified());
    Ok(())
}

#[test]
fn empty_preedit_has_no_native_marked_span_and_can_be_cancelled()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("a中z");
    editor.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(4),
    })?;
    editor.set_composition("候选")?;
    editor.mark_display_range(None, "", None)?;
    assert_eq!(editor.marked_range(), None);
    assert_eq!(editor.display_text(), "az");
    assert_eq!(editor.committed_text(), "a中z");
    editor.cancel_composition();
    assert_eq!(editor.display_text(), "a中z");
    assert!(!editor.undo());
    Ok(())
}

#[test]
fn default_confirmation_replaces_only_actual_marked_span() -> Result<(), Box<dyn std::error::Error>>
{
    let mut editor = EditorSession::new("z");
    editor.set_composition("abcd")?;
    editor.mark_display_range(Some(ByteOffset(1)..ByteOffset(2)), "X", None)?;
    editor.insert_text("Y")?;
    assert_eq!(editor.committed_text(), "aYcdz");
    assert_eq!(editor.selection().head, ByteOffset(2));
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "z");
    Ok(())
}

#[test]
fn display_revision_invalidates_geometry_after_edits_but_not_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("abcdef");
    let painted = editor.display_revision();
    editor.select_display(Selection {
        anchor: ByteOffset(0),
        head: ByteOffset(6),
    })?;
    assert_eq!(painted, editor.display_revision());
    editor.insert_text("")?;
    assert_ne!(painted, editor.display_revision());
    let empty = editor.display_revision();
    editor.set_composition("候")?;
    assert_ne!(empty, editor.display_revision());
    let marked = editor.display_revision();
    editor.cancel_composition();
    assert_ne!(marked, editor.display_revision());
    Ok(())
}

#[test]
fn unmark_preserves_preview_selection_through_redo() -> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("a中z");
    editor.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(4),
    })?;
    editor.mark_display_range(None, "文😀", Some(1..3))?;
    editor.commit_composition()?;
    let selected = Selection {
        anchor: ByteOffset(4),
        head: ByteOffset(8),
    };
    assert_eq!(editor.selection(), selected);
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a中z");
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "a文😀z");
    assert_eq!(editor.selection(), selected);
    Ok(())
}

#[test]
fn partial_composition_updates_keep_only_inserted_span_marked()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("z");
    editor.set_composition("abcd")?;
    editor.mark_display_range(Some(ByteOffset(1)..ByteOffset(2)), "X", None)?;
    assert_eq!(editor.display_text(), "aXcdz");
    assert_eq!(editor.marked_range(), Some(ByteOffset(1)..ByteOffset(2)));
    editor.mark_display_range(None, "Y", None)?;
    assert_eq!(editor.display_text(), "aYcdz");
    assert_eq!(editor.committed_text(), "z");
    editor.commit_composition()?;
    assert_eq!(editor.committed_text(), "aYcdz");
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "z");
    Ok(())
}

#[test]
fn marked_replacement_honors_relative_utf16_selection_and_rejects_split_surrogates()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("a中z");
    editor.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(4),
    })?;
    editor.mark_display_range(None, "文😀", Some(1..3))?;
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(8)
        }
    );
    assert_eq!(editor.committed_text(), "a中z");
    assert!(editor.mark_display_range(None, "😀", Some(1..1)).is_err());
    assert_eq!(editor.display_text(), "a文😀z");
    editor.mark_display_range(None, "", None)?;
    editor.cancel_composition();
    assert_eq!(editor.committed_text(), "a中z");
    Ok(())
}

#[test]
fn composition_preview_never_changes_committed_text_until_confirmed()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("a中z");
    editor.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(4),
    })?;
    editor.set_composition("候选")?;
    assert_eq!(editor.committed_text(), "a中z");
    assert_eq!(editor.display_text(), "a候选z");
    assert!(!editor.is_modified());
    editor.cancel_composition();
    assert_eq!(editor.display_text(), "a中z");
    editor.set_composition("文")?;
    editor.commit_composition()?;
    assert_eq!(editor.committed_text(), "a文z");
    assert!(editor.is_modified());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a中z");
    assert!(!editor.is_modified());
    Ok(())
}

#[test]
fn confirmed_text_replaces_virtual_marked_range_not_committed_coordinates()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("a中z");
    editor.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(4),
    })?;
    editor.set_composition("候选")?;
    editor.replace_display_range(ByteOffset(1)..ByteOffset(7), "文")?;
    assert_eq!(editor.committed_text(), "a文z");
    assert_eq!(editor.marked_range(), None);
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "a中z");
    Ok(())
}
#[test]
fn loaded_text_uses_first_line_ending_without_changing_new_document_default() {
    use notepad_matrix::input::{EditorSession, LineEnding};
    for (text, expected) in [
        ("", LineEnding::CrLf),
        ("no newline", LineEnding::CrLf),
        ("first\nsecond\r\nthird\r\n", LineEnding::Lf),
        ("first\rsecond\nthird\n", LineEnding::Cr),
        ("\0中😀\r\nsecond\n", LineEnding::CrLf),
    ] {
        let editor = EditorSession::from_file_text(text);
        assert_eq!(editor.line_ending(), expected);
        assert_eq!(editor.committed_text(), text);
    }
    assert_eq!(
        EditorSession::new("typed\ntext").line_ending(),
        LineEnding::CrLf
    );
}
#[test]
fn eol_conversion_is_one_transaction_preserving_directional_selection_and_mode()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::input::{EditorSession, LineEnding};
    let original = "a\r\nb\rc\n中😀";
    let before = Selection {
        anchor: ByteOffset(14),
        head: ByteOffset(3),
    };
    let after = Selection {
        anchor: ByteOffset(13),
        head: ByteOffset(2),
    };
    let mut editor = EditorSession::new(original);
    editor.select(before)?;
    editor.convert_line_endings(LineEnding::Lf)?;
    assert_eq!(editor.committed_text(), "a\nb\nc\n中😀");
    assert_eq!(editor.selection(), after);
    assert!(editor.is_modified());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), original);
    assert_eq!(editor.selection(), before);
    assert!(!editor.is_modified());
    assert_eq!(editor.line_ending(), LineEnding::Lf);
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "a\nb\nc\n中😀");
    assert_eq!(editor.selection(), after);
    Ok(())
}
#[test]
fn eol_conversion_uses_reference_boundary_affinity_and_keeps_non_eol_scalars()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::input::{EditorSession, LineEnding};
    let cases = [
        ("a\r\nb", LineEnding::Lf, 2, 2, "a\nb", 1, 1),
        ("a\r\nb", LineEnding::Cr, 2, 2, "a\rb", 2, 2),
        ("a\nb", LineEnding::CrLf, 1, 1, "a\r\nb", 1, 1),
        ("a\nb", LineEnding::CrLf, 1, 3, "a\r\nb", 2, 4),
        ("a\rb", LineEnding::Lf, 1, 3, "a\nb", 2, 3),
        ("a\nb", LineEnding::Cr, 3, 1, "a\rb", 3, 2),
    ];
    for (text, target, anchor, head, expected, after_anchor, after_head) in cases {
        let before = Selection {
            anchor: ByteOffset(anchor),
            head: ByteOffset(head),
        };
        let mut editor = EditorSession::new(text);
        editor.select(before)?;
        editor.convert_line_endings(target)?;
        assert_eq!(editor.committed_text(), expected);
        assert_eq!(
            editor.selection(),
            Selection {
                anchor: ByteOffset(after_anchor),
                head: ByteOffset(after_head)
            }
        );
        assert!(editor.undo());
        assert_eq!(editor.committed_text(), text);
        assert_eq!(editor.selection(), before);
    }
    let original = "\0中\r\n😀\rX\n".repeat(4096);
    let mut editor = EditorSession::new(&original);
    assert!(editor.display_snapshot().chunks().count() > 1);
    editor.set_composition("preview")?;
    editor.convert_line_endings(LineEnding::Lf)?;
    assert_eq!(editor.committed_text(), "\0中\n😀\nX\n".repeat(4096));
    assert_eq!(editor.display_text(), editor.committed_text());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), original);
    Ok(())
}

#[test]
fn eol_mode_only_noop_and_readonly_failure_preserve_history_and_dirty_state()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::EditError;
    use notepad_matrix::input::{EditorSession, LineEnding};
    let mut empty = EditorSession::new("single");
    empty.convert_line_endings(LineEnding::Lf)?;
    assert_eq!(empty.committed_text(), "single");
    assert_eq!(empty.line_ending(), LineEnding::Lf);
    assert!(!empty.is_modified());
    assert!(!empty.undo());
    let mut editor = EditorSession::from_file_text("first\nsecond");
    editor.move_to_document_edge(true, false)?;
    editor.insert_text("X")?;
    assert!(editor.undo());
    let revision = editor.display_revision();
    let selection = editor.selection();
    editor.convert_line_endings(LineEnding::Lf)?;
    assert_eq!(editor.display_revision(), revision);
    assert!(!editor.is_modified());
    editor.set_read_only(true);
    assert!(matches!(
        editor.convert_line_endings(LineEnding::CrLf),
        Err(EditError::ReadOnly)
    ));
    assert_eq!(editor.line_ending(), LineEnding::Lf);
    assert_eq!(editor.committed_text(), "first\nsecond");
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.display_revision(), revision);
    assert!(!editor.is_modified());
    editor.set_read_only(false);
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "first\nsecondX");
    Ok(())
}
#[test]
fn line_delete_uses_main_caret_line_and_rebases_directional_selection_in_one_transaction()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::input::EditorSession;
    let original = "A\r\nB\nC\rD";
    let before = Selection {
        anchor: ByteOffset(8),
        head: ByteOffset(4),
    };
    let mut editor = EditorSession::new(original);
    editor.select(before)?;
    editor.delete_current_line()?;
    assert_eq!(editor.committed_text(), "A\r\nC\rD");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(3)
        }
    );
    assert!(editor.is_modified());
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), original);
    assert_eq!(editor.selection(), before);
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "A\r\nC\rD");
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(3)
        }
    );
    let mut all = EditorSession::new(original);
    all.select(Selection {
        anchor: ByteOffset(8),
        head: ByteOffset(0),
    })?;
    all.delete_current_line()?;
    assert_eq!(all.committed_text(), "B\nC\rD");
    assert_eq!(
        all.selection(),
        Selection {
            anchor: ByteOffset(5),
            head: ByteOffset(0)
        }
    );
    Ok(())
}
#[test]
fn line_delete_empty_last_line_readonly_and_unimplemented_composition_preserve_state()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::EditError;
    use notepad_matrix::input::EditorSession;
    for text in ["", "A\n", "中\r\n"] {
        let mut editor = EditorSession::new(text);
        editor.move_to_document_edge(true, false)?;
        let revision = editor.display_revision();
        let selection = editor.selection();
        editor.delete_current_line()?;
        assert_eq!(editor.committed_text(), text);
        assert_eq!(editor.selection(), selection);
        assert_eq!(editor.display_revision(), revision);
        assert!(!editor.is_modified());
        assert!(!editor.undo());
    }
    let mut readonly = EditorSession::new("A😀\r\nZ");
    readonly.set_read_only(true);
    let selection = readonly.selection();
    let revision = readonly.display_revision();
    assert!(matches!(
        readonly.delete_current_line(),
        Err(EditError::ReadOnly)
    ));
    assert_eq!(readonly.committed_text(), "A😀\r\nZ");
    assert_eq!(readonly.selection(), selection);
    assert_eq!(readonly.display_revision(), revision);
    assert!(!readonly.is_modified());
    let mut composing = EditorSession::new("body");
    composing.set_composition("preview")?;
    let displayed = composing.display_text();
    let selection = composing.selection();
    let revision = composing.display_revision();
    assert!(matches!(
        composing.delete_current_line(),
        Err(EditError::UnsupportedComposition)
    ));
    assert_eq!(composing.committed_text(), "body");
    assert_eq!(composing.display_text(), displayed);
    assert_eq!(composing.selection(), selection);
    assert_eq!(composing.display_revision(), revision);
    assert!(!composing.is_modified());
    Ok(())
}
#[test]
fn pending_composition_is_detectable_even_with_an_empty_marked_range()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = notepad_matrix::input::EditorSession::new("old command");
    assert!(!editor.has_composition());
    editor.set_composition("")?;
    assert!(editor.has_composition());
    editor.cancel_composition();
    assert!(!editor.has_composition());
    Ok(())
}
