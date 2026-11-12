use notepad_matrix::{editor, input, syntax, text_transforms, udl};
use text_transforms::session as surface;

use editor::{ByteOffset, Selection, SelectionRange, SelectionSet};
use input::EditorSession;
use text_transforms::TransformOptions;

#[test]
fn transaction_applies_all_ranges_and_restores_direction_in_one_undo() {
    let mut session = EditorSession::new("é x yz");
    session
        .set_selections(
            SelectionSet::rectangle(
                vec![
                    SelectionRange {
                        selection: Selection {
                            anchor: ByteOffset(2),
                            head: ByteOffset(0),
                        },
                        anchor_virtual: 0,
                        head_virtual: 0,
                    },
                    Selection {
                        anchor: ByteOffset(5),
                        head: ByteOffset(7),
                    }
                    .into(),
                ],
                1,
            )
            .unwrap(),
        )
        .unwrap();
    surface::apply_command(&mut session, 42016, TransformOptions::default()).unwrap();
    assert_eq!(session.display_text(), "É x YZ");
    assert!(session.display_selections().is_rectangular());
    assert_eq!(session.display_selections().primary_index(), 1);
    assert_eq!(
        session.display_selections().ranges()[0].selection.anchor,
        ByteOffset(2)
    );
    assert!(session.undo());
    assert_eq!(session.display_text(), "é x yz");
}

#[test]
fn collapsed_source_selection_clamps_primary_without_second_edit() {
    let mut session = EditorSession::new("a\nb");
    session
        .set_selections(
            SelectionSet::new(
                vec![
                    Selection {
                        anchor: ByteOffset(0),
                        head: ByteOffset(0),
                    }
                    .into(),
                    Selection {
                        anchor: ByteOffset(2),
                        head: ByteOffset(2),
                    }
                    .into(),
                ],
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let options = TransformOptions {
        comment_syntax: text_transforms::CommentSyntax {
            line: Some("//".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    surface::apply_command(&mut session, 42035, options).unwrap();
    assert_eq!(session.display_text(), "a\n// b");
    assert_eq!(session.display_selections().ranges().len(), 1);
    assert_eq!(session.display_selections().primary_index(), 0);
    assert!(session.undo());
    assert_eq!(session.display_text(), "a\nb");
    assert_eq!(session.display_selections().ranges().len(), 2);
    assert!(!session.undo());
}

#[test]
fn every_retained_source_id_reaches_atomic_transaction_and_readonly_guard() {
    assert_eq!(text_transforms::RETAINED_COMMANDS.len(), 52);
    for &(name, id, _) in text_transforms::RETAINED_COMMANDS {
        let original = " 2 \n 1 \n 1 \n\n";
        let mut session = EditorSession::new(original);
        session
            .select_display(Selection {
                anchor: ByteOffset(0),
                head: ByteOffset(original.len()),
            })
            .unwrap();
        let options = TransformOptions {
            split_breaks: vec![2],
            locale_keys: Some(vec![vec![2], vec![1], vec![1], vec![0], vec![0]]),
            comment_syntax: text_transforms::CommentSyntax {
                line: Some("//".into()),
                stream: Some(("/*".into(), "*/".into())),
                ..Default::default()
            },
            ..Default::default()
        };
        surface::apply_command(&mut session, id, options.clone())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        if session.display_text() != original {
            assert!(session.undo(), "{name}");
            assert_eq!(session.display_text(), original, "{name}");
            assert!(!session.undo(), "{name} created more than one undo");
        }
        session.set_read_only(true);
        let before = session.display_selections();
        assert!(
            surface::apply_command(&mut session, id, options).is_err(),
            "{name}"
        );
        assert_eq!(session.display_text(), original, "{name}");
        assert_eq!(session.display_selections(), before, "{name}");
    }
}

#[test]
fn comment_commands_use_selected_catalog_or_udl_and_disable_normal_text() {
    use syntax::Language;
    let plain = surface::comment_syntax(Language::NormalText, None).unwrap();
    assert_eq!(plain.line, None);
    assert_eq!(plain.stream, None);
    let cpp = surface::comment_syntax(Language::Builtin(3), None).unwrap();
    assert_eq!(cpp.line.as_deref(), Some("//"));
    assert_eq!(cpp.stream, Some(("/*".into(), "*/".into())));
    let mut definition = udl::Definition::default();
    definition.comments.line_open = "REM".into();
    definition.comments.block_open = "(*".into();
    definition.comments.block_close = "*)".into();
    definition.line_comment_placement = udl::LineCommentPlacement::BeginningOfLine;
    let custom = surface::comment_syntax(Language::UserDefined(9), Some(&definition)).unwrap();
    assert_eq!(custom.line.as_deref(), Some("REM"));
    assert_eq!(custom.stream, Some(("(*".into(), "*)".into())));
    assert!(custom.at_line_start);
    assert!(surface::comment_syntax(Language::UserDefined(9), None).is_err());
}

#[test]
fn measured_document_breaks_are_scoped_to_the_selected_logical_lines() {
    let mut session = EditorSession::new("abcd\nefgh\nijkl");
    session
        .select_display(Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(8),
        })
        .unwrap();
    surface::apply_command(
        &mut session,
        42012,
        TransformOptions {
            split_breaks: vec![2, 7, 12],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(session.display_text(), "abcd\nef\r\ngh\nijkl");
    assert!(session.undo());
    assert_eq!(session.display_text(), "abcd\nefgh\nijkl");
}
