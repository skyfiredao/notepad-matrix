use notepad_matrix::editor::{
    ByteOffset, EditSelectionPolicy, Selection, SelectionRange, SelectionSet, TextTransform,
};
use notepad_matrix::input::EditorSession;
use notepad_matrix::text_transforms::*;

fn core_selections(
    selections: &[TransformSelection],
    primary: usize,
) -> Result<SelectionSet, notepad_matrix::editor::EditError> {
    SelectionSet::new(
        selections
            .iter()
            .map(|s| SelectionRange {
                selection: Selection {
                    anchor: ByteOffset(s.anchor),
                    head: ByteOffset(s.caret),
                },
                anchor_virtual: s.anchor_virtual_space,
                head_virtual: s.caret_virtual_space,
            })
            .collect(),
        primary,
    )
}

fn core_edits(plan: &TransformPlan) -> Vec<TextTransform> {
    plan.replacements
        .iter()
        .map(|edit| TextTransform {
            range: ByteOffset(edit.range.start)..ByteOffset(edit.range.end),
            text: edit.text.clone(),
        })
        .collect()
}

#[test]
fn rectangular_key_sort_rows_and_selection_duplicate_apply_as_one_undo()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "z2\na1\nb1";
    let mut editor = EditorSession::from_file_text(text);
    let selections = [
        TransformSelection::new(1, 2),
        TransformSelection::new(5, 4),
        TransformSelection::new(7, 8),
    ];
    editor.set_selections(core_selections(&selections, 1)?)?;
    let options = TransformOptions {
        main_selection: 1,
        selection_mode: TransformSelectionMode::Rectangle {
            from_column: 1,
            to_column: 2,
        },
        ..Default::default()
    };
    let plan = plan_transform(
        text,
        TransformCommand::Sort {
            kind: Sort::Integer,
            descending: false,
        },
        &selections,
        &options,
    )?;
    editor.apply_multi(
        &core_edits(&plan),
        EditSelectionPolicy::Explicit(core_selections(&plan.selections, 1)?),
    )?;
    assert_eq!(editor.committed_text(), "a1\nb1\nz2");
    assert_eq!(editor.selections(), core_selections(&selections, 1)?);
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), text);
    assert!(!editor.undo());
    let mut editor = EditorSession::from_file_text("ab");
    let original = [TransformSelection::new(2, 0)];
    editor.set_selections(core_selections(&original, 0)?)?;
    let plan = plan_transform(
        "ab",
        TransformCommand::DuplicateSelection,
        &original,
        &TransformOptions::default(),
    )?;
    editor.apply_multi(
        &core_edits(&plan),
        EditSelectionPolicy::Explicit(core_selections(&plan.selections, 0)?),
    )?;
    assert_eq!(editor.committed_text(), "abab");
    assert_eq!(editor.selections(), core_selections(&original, 0)?);
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "ab");
    assert!(!editor.undo());
    Ok(())
}

#[test]
fn planner_multi_case_is_one_transaction_restoring_both_views_and_direction()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "ı a ı";
    let mut editor = EditorSession::from_file_text(text);
    let original = [TransformSelection::new(7, 5), TransformSelection::new(2, 0)];
    editor.set_selections(core_selections(&original, 0)?)?;
    let mut peer = editor.clone_view();
    let peer_original = Selection {
        anchor: ByteOffset(4),
        head: ByteOffset(3),
    };
    peer.select(peer_original)?;
    let command = TransformCommand::from_source_id(42016).ok_or("missing uppercase command")?;
    assert_eq!(command.source_id(), Some(42016));
    let plan = plan_transform(text, command, &original, &TransformOptions::default())?;
    assert_eq!(plan.replacements.len(), 2);
    editor.apply_multi(
        &core_edits(&plan),
        EditSelectionPolicy::Explicit(core_selections(&plan.selections, 0)?),
    )?;
    assert_eq!(editor.committed_text(), "I a I");
    assert_eq!(
        editor.selections(),
        core_selections(
            &[TransformSelection::new(5, 4), TransformSelection::new(1, 0)],
            0
        )?
    );
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(3),
            head: ByteOffset(2)
        }
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), text);
    assert_eq!(editor.selections(), core_selections(&original, 0)?);
    assert_eq!(peer.selection(), peer_original);
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "I a I");
    assert_eq!(editor.selections(), core_selections(&plan.selections, 0)?);
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(3),
            head: ByteOffset(2)
        }
    );
    Ok(())
}

#[test]
fn rejected_plan_or_transaction_has_no_partial_text_or_history()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "b\na";
    let mut editor = EditorSession::from_file_text(text);
    let original = [TransformSelection::new(3, 0)];
    editor.set_selections(core_selections(&original, 0)?)?;
    let plan = plan_transform(
        text,
        TransformCommand::Sort {
            kind: Sort::Lexical,
            descending: false,
        },
        &original,
        &TransformOptions::default(),
    )?;
    let mut malformed = core_edits(&plan);
    malformed.push(TextTransform {
        range: ByteOffset(999)..ByteOffset(999),
        text: "bad".into(),
    });
    assert!(
        editor
            .apply_multi(
                &malformed,
                EditSelectionPolicy::Explicit(core_selections(&plan.selections, 0)?)
            )
            .is_err()
    );
    assert_eq!(editor.committed_text(), text);
    assert_eq!(editor.selections(), core_selections(&original, 0)?);
    assert!(!editor.undo());
    let bad_numeric = plan_transform(
        "2\n--1",
        TransformCommand::Sort {
            kind: Sort::DecimalDot,
            descending: false,
        },
        &[TransformSelection::caret(0)],
        &TransformOptions::default(),
    );
    assert_eq!(bad_numeric, Err(TransformError::InvalidNumber { line: 2 }));
    editor.set_read_only(true);
    assert!(
        editor
            .apply_multi(
                &core_edits(&plan),
                EditSelectionPolicy::Explicit(core_selections(&plan.selections, 0)?)
            )
            .is_err()
    );
    assert_eq!(editor.committed_text(), text);
    assert!(!editor.is_modified());
    Ok(())
}
