use notepad_matrix::editor::{ByteOffset, Document, Selection};

#[test]
fn captured_savepoint_marks_only_captured_changes_and_preserves_later_edits() {
    let mut document = Document::new("");
    document.replace_selection("A").unwrap();
    let saved = document.save_snapshot();
    document.replace_selection("X").unwrap();
    document.acknowledge_save(&saved).unwrap();
    assert!(document.is_modified());
    assert_eq!(document.change_marker_bits(0), 6);
    assert!(document.undo());
    assert_eq!(document.text(), "A");
    assert_eq!(document.change_marker_bits(0), 2);
    assert!(!document.is_modified());
    assert!(document.redo());
    assert_eq!(document.change_marker_bits(0), 6);
}

#[test]
fn source_change_markers_follow_insert_save_undo_redo_and_shared_views() {
    let mut document = Document::new("one\r\ntwo");
    let other = document.clone_view();
    document
        .select(Selection {
            anchor: ByteOffset(1),
            head: ByteOffset(1),
        })
        .unwrap();
    document.replace_selection("x").unwrap();
    assert_eq!(document.change_marker_bits(0), 4);
    assert_eq!(document.change_marker_bits(1), 0);
    assert_eq!(other.change_marker_bits(0), 4);
    document.mark_saved();
    assert_eq!(document.change_marker_bits(0), 2);
    assert!(other.clone_view().undo());
    assert_eq!(document.text(), "one\r\ntwo");
    assert_eq!(document.change_marker_bits(0), 1);
    assert!(document.redo());
    assert_eq!(document.change_marker_bits(0), 2);
    let end = ByteOffset(document.len_bytes());
    document
        .select(Selection {
            anchor: ByteOffset(0),
            head: end,
        })
        .unwrap();
    document.replace_selection("replacement").unwrap();
    assert_eq!(document.change_marker_bits(0), 4);
    assert!(document.undo());
    assert_eq!(document.text(), "oxne\r\ntwo");
    assert_eq!(document.change_marker_bits(0), 9);
    assert_eq!(document.change_marker_bits(1), 1);
}

#[test]
fn source_deletion_history_marks_empty_lines_and_returns_saved_editions_on_undo() {
    let mut document = Document::new("a\r\nb\r\n");
    document
        .select(Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(1),
        })
        .unwrap();
    document.replace_selection("").unwrap();
    assert_eq!(document.text(), "\r\nb\r\n");
    assert_eq!(document.change_marker_bits(0), 4);
    document.mark_saved();
    assert_eq!(document.change_marker_bits(0), 2);
    assert!(document.undo());
    assert_eq!(document.text(), "a\r\nb\r\n");
    assert_eq!(document.change_marker_bits(0), 1);
    assert!(document.redo());
    assert_eq!(document.change_marker_bits(0), 2);
    assert_eq!(document.change_marker_bits(1), 0);
}

#[test]
fn source_change_navigation_skips_contiguous_blocks_and_wraps_asymmetrically() {
    let mut document = Document::new("0\n1\n2\n3\n4\n5");
    for line in [0, 1, 4, 5] {
        let start = document.line_start(line).unwrap();
        document
            .select(Selection {
                anchor: start,
                head: start,
            })
            .unwrap();
        document.replace_selection("x").unwrap();
    }
    document
        .select(Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(0),
        })
        .unwrap();
    for (forward, expected) in [(true, 4), (true, 0), (false, 5), (false, 1)] {
        assert!(document.goto_change(forward).unwrap());
        assert_eq!(
            document.selection().head,
            document.line_start(expected).unwrap()
        );
    }
    document.clear_change_history().unwrap();
    let selection = document.selection();
    assert!(!document.goto_change(true).unwrap());
    assert_eq!(document.selection(), selection);
    assert!(!document.can_undo());
    assert!(!document.can_redo());
}

#[test]
fn source_clear_change_history_resets_engine_undo_without_cleaning_buffer_or_bookmarks() {
    let mut workspace = notepad_matrix::workspace::Workspace::new();
    workspace
        .active_mut()
        .editor
        .insert_text("changed")
        .unwrap();
    let mut document = workspace.active().editor.document().clone_view();
    document.toggle_bookmark_at(0);
    document
        .select(Selection {
            anchor: ByteOffset(0),
            head: ByteOffset(4),
        })
        .unwrap();
    document.set_read_only(true);
    document.clear_change_history().unwrap();
    assert_eq!(
        document.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(4)
        }
    );
    assert_eq!(document.change_marker_bits(0), 0);
    assert!(!document.is_modified());
    assert!(workspace.active().is_modified());
    assert_eq!(document.bookmarked_lines().collect::<Vec<_>>(), [0]);
    assert_eq!(document.text(), "changed");
}
