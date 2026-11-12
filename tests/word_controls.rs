use notepad_matrix::editor::{ByteOffset, Document, Selection, SelectionSet, WordBoundary};
use notepad_matrix::input::EditorSession;

fn selection(anchor: usize, head: usize) -> Selection {
    Selection {
        anchor: ByteOffset(anchor),
        head: ByteOffset(head),
    }
}

#[test]
fn source_mouse_word_selection_keeps_scalar_classes_line_ends_and_drag_direction() {
    let editor = EditorSession::new("alpha beta 中文😀\r\n\nnext");
    for (at, start, end) in [
        (0, 0, 5),
        (5, 5, 6),
        (8, 6, 10),
        (11, 11, 17),
        (17, 17, 21),
        (21, 17, 21),
        (23, 23, 23),
        (28, 24, 28),
    ] {
        assert_eq!(
            editor.word_selection_at(ByteOffset(at)).unwrap(),
            selection(start, end)
        );
    }
    assert!(editor.word_selection_at(ByteOffset(12)).is_err());
    assert!(editor.word_selection_at(ByteOffset(29)).is_err());
    for (at, anchor, head) in [
        (2, 10, 0),
        (7, 10, 6),
        (9, 6, 10),
        (14, 6, 17),
        (23, 6, 23),
        (24, 6, 24),
    ] {
        assert_eq!(
            editor
                .extend_word_selection(selection(6, 10), ByteOffset(8), ByteOffset(at))
                .unwrap(),
            selection(anchor, head)
        );
    }
    assert_eq!(
        EditorSession::new("")
            .word_selection_at(ByteOffset(0))
            .unwrap(),
        selection(0, 0)
    );
    assert!(!editor.is_modified());
    assert!(!editor.document().can_undo());
}

#[test]
fn source_word_start_and_end_boundaries_distinguish_punctuation_space_and_crlf() {
    let doc = Document::new("one  two,++three\r\n  four");
    for (position, forward, boundary, expected) in [
        (0, true, WordBoundary::Start, 5),
        (5, true, WordBoundary::Start, 8),
        (8, true, WordBoundary::Start, 11),
        (16, true, WordBoundary::Start, 20),
        (20, false, WordBoundary::Start, 16),
        (16, false, WordBoundary::Start, 11),
        (0, true, WordBoundary::End, 3),
        (3, true, WordBoundary::End, 8),
        (16, true, WordBoundary::End, 18),
        (20, false, WordBoundary::End, 18),
    ] {
        assert_eq!(
            doc.word_boundary(ByteOffset(position), forward, boundary)
                .unwrap(),
            ByteOffset(expected)
        );
    }
    assert!(!doc.is_modified());
    assert!(!doc.can_undo());
}

#[test]
fn npp_word_navigation_moves_all_carets_and_right_extension_uses_word_end_without_editing() {
    let mut editor = EditorSession::new("one  two,++three\r\n  four");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(0, 0).into(), selection(11, 11).into()], 1).unwrap(),
        )
        .unwrap();
    let peer = editor.clone_view();
    let revision = editor.display_revision();
    editor.move_word(true, false).unwrap();
    assert_eq!(editor.selections().ranges()[0].selection, selection(5, 5));
    assert_eq!(editor.selection(), selection(16, 16));
    editor.set_read_only(true);
    editor.move_word(true, true).unwrap();
    assert_eq!(editor.selections().ranges()[0].selection, selection(5, 8));
    assert_eq!(editor.selection(), selection(16, 18));
    assert_eq!(peer.selection(), selection(11, 11));
    assert_eq!(editor.display_revision(), revision);
    assert!(!editor.is_modified());
    assert!(!editor.document().can_undo());
}

#[test]
fn unicode_word_boundaries_use_source_marks_symbols_format_private_and_newline_classes() {
    let doc = Document::new("क्\u{200b}😀—\u{e000}中");
    assert_eq!(
        doc.word_boundary(ByteOffset(0), true, WordBoundary::Start)
            .unwrap(),
        ByteOffset(9)
    );
    assert_eq!(
        doc.word_boundary(ByteOffset(9), true, WordBoundary::End)
            .unwrap(),
        ByteOffset(16)
    );
    assert_eq!(
        doc.word_boundary(ByteOffset(9), true, WordBoundary::Start)
            .unwrap(),
        ByteOffset(19)
    );
    assert_eq!(
        doc.word_boundary(ByteOffset(19), false, WordBoundary::Start)
            .unwrap(),
        ByteOffset(9)
    );
    assert!(
        doc.word_boundary(ByteOffset(1), true, WordBoundary::Start)
            .is_err()
    );
    let separators = Document::new("a\u{2028}  b");
    assert_eq!(
        separators
            .word_boundary(ByteOffset(1), true, WordBoundary::Start)
            .unwrap(),
        ByteOffset(6)
    );
    assert_eq!(
        separators
            .word_boundary(ByteOffset(1), true, WordBoundary::End)
            .unwrap(),
        ByteOffset(4)
    );
}

#[test]
fn source_word_parts_use_ascii_case_digits_separators_and_contiguous_non_ascii() {
    let doc = Document::new("HTTPServer_foo42__बार😀 \r\nZ");
    for (position, forward, expected) in [
        (0, true, 4),
        (4, true, 10),
        (10, true, 14),
        (14, true, 16),
        (16, true, 31),
        (31, true, 34),
        (31, false, 18),
        (18, false, 14),
        (10, false, 4),
        (4, false, 0),
    ] {
        assert_eq!(
            doc.word_boundary(ByteOffset(position), forward, WordBoundary::Part)
                .unwrap(),
            ByteOffset(expected)
        );
    }
    let line = Document::new("ab\r\n xyz");
    assert_eq!(
        line.word_boundary(ByteOffset(6), false, WordBoundary::Line)
            .unwrap(),
        ByteOffset(4)
    );
    assert_eq!(
        line.word_boundary(ByteOffset(6), true, WordBoundary::Line)
            .unwrap(),
        ByteOffset(8)
    );
}

#[test]
fn source_word_delete_edits_all_carets_in_one_history_action_and_maps_peers() {
    let mut editor = EditorSession::new("one two\nred blue");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(7, 7).into(), selection(16, 16).into()], 1).unwrap(),
        )
        .unwrap();
    let original = editor.selections();
    let mut peer = editor.clone_view();
    peer.select(selection(8, 11)).unwrap();
    editor.delete_word(false, false).unwrap();
    assert_eq!(editor.committed_text(), "one \nred ");
    assert_eq!(editor.selections().ranges()[0].selection, selection(4, 4));
    assert_eq!(editor.selection(), selection(9, 9));
    assert_eq!(peer.selection(), selection(5, 8));
    assert!(peer.undo());
    assert_eq!(editor.committed_text(), "one two\nred blue");
    assert_eq!(editor.selections(), original);
    assert_eq!(peer.selection(), selection(8, 11));
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "one \nred ");
}

#[test]
fn backspace_not_line_skips_each_empty_line_start_not_the_whole_selection_set() {
    let mut editor = EditorSession::new("abc\r\nxy");
    editor
        .set_selections(
            SelectionSet::new(vec![selection(0, 0).into(), selection(6, 6).into()], 0).unwrap(),
        )
        .unwrap();
    editor.backspace_not_line().unwrap();
    assert_eq!(editor.committed_text(), "abc\r\ny");
    assert_eq!(editor.selections().ranges()[0].selection, selection(0, 0));
    assert_eq!(editor.selections().ranges()[1].selection, selection(5, 5));
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "abc\r\nxy");
    assert!(!editor.undo());
    editor.select(selection(2, 6)).unwrap();
    editor.backspace_not_line().unwrap();
    assert_eq!(editor.committed_text(), "aby");
}
