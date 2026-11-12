use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::input::EditorSession;

fn caret(at: usize) -> Selection {
    Selection {
        anchor: ByteOffset(at),
        head: ByteOffset(at),
    }
}

#[test]
fn cloned_sessions_share_text_history_savepoint_and_map_independent_carets()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("abc\ndef");
    first.select(caret(5))?;
    let mut peer = first.clone_view();
    peer.select(caret(1))?;
    peer.set_overwrite(true);
    peer.insert_text("XY")?;
    assert_eq!(first.committed_text(), "aXYbc\ndef");
    assert_eq!(first.selection(), caret(7));
    assert_eq!(peer.selection(), caret(3));
    assert!(!first.overwrite());
    assert!(first.document().can_undo());
    let saved = first.save_snapshot();
    peer.acknowledge_save(&saved)?;
    assert!(!first.is_modified());
    assert!(first.undo());
    assert_eq!(peer.committed_text(), "abc\ndef");
    assert_eq!(first.selection(), caret(5));
    assert_eq!(peer.selection(), caret(1));
    assert!(peer.is_modified());
    assert!(peer.redo());
    assert_eq!(first.committed_text(), "aXYbc\ndef");
    assert_eq!(first.selection(), caret(7));
    assert_eq!(peer.selection(), caret(3));
    assert!(!first.is_modified());
    first.toggle_bookmark(Some(1))?;
    assert_eq!(
        peer.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![1]
    );
    Ok(())
}

#[test]
fn workspace_clone_move_and_close_operate_on_memberships_not_document_copies()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::workspace::{CloseDisposition, ViewId, Workspace};
    let mut workspace = Workspace::new();
    let id = workspace.active().id();
    workspace.active_mut().editor.insert_text("shared")?;
    workspace.clone_to_other_view(id)?;
    assert_eq!(workspace.active_view(), ViewId::Secondary);
    assert_eq!(workspace.view_buffer_ids(ViewId::Primary), &[id]);
    assert_eq!(workspace.view_buffer_ids(ViewId::Secondary), &[id]);
    assert_eq!(workspace.buffers().len(), 1);
    workspace.active_mut().editor.select(caret(0))?;
    workspace.active_mut().editor.insert_text("X")?;
    workspace.activate_view(ViewId::Primary)?;
    assert_eq!(workspace.active().editor.committed_text(), "Xshared");
    assert_eq!(workspace.active().editor.selection(), caret(7));
    workspace.close_buffer(id, CloseDisposition::IfUnmodified)?;
    assert_eq!(workspace.active_view(), ViewId::Secondary);
    assert_eq!(workspace.buffers().len(), 1);
    assert!(
        workspace
            .close_buffer(id, CloseDisposition::IfUnmodified)
            .is_err()
    );
    let other = workspace.new_buffer()?;
    workspace.move_to_other_view(id)?;
    assert_eq!(workspace.active_view(), ViewId::Primary);
    assert_eq!(workspace.view_buffer_ids(ViewId::Secondary), &[other]);
    assert_eq!(workspace.view_buffer_ids(ViewId::Primary), &[id]);
    assert_eq!(workspace.active().editor.committed_text(), "Xshared");
    Ok(())
}

#[test]
fn peer_edits_invalidate_only_stale_preedit_and_display_revision()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("abc");
    first.select(caret(2))?;
    let mut peer = first.clone_view();
    let revision = first.display_revision();
    first.set_composition("候")?;
    assert_eq!(peer.display_text(), "abc");
    assert_eq!(first.display_text(), "ab候c");
    peer.select(caret(0))?;
    peer.insert_text("X")?;
    assert_ne!(first.display_revision(), revision);
    assert!(!first.has_composition());
    assert_eq!(first.display_text(), "Xabc");
    assert_eq!(first.selection(), caret(3));
    first.insert_text("Y")?;
    assert_eq!(peer.committed_text(), "XabYc");
    Ok(())
}

#[test]
fn eol_mode_and_component_mapping_are_shared_but_carets_are_not()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::input::LineEnding;
    let mut first = EditorSession::from_file_text("a\nb\nc\n");
    let mut peer = first.clone_view();
    peer.select(caret(4))?;
    first.convert_line_endings(LineEnding::CrLf)?;
    assert_eq!(peer.line_ending(), LineEnding::CrLf);
    assert_eq!(peer.selection(), caret(6));
    assert!(first.undo());
    assert_eq!(peer.selection(), caret(4));
    assert_eq!(peer.line_ending(), LineEnding::CrLf);
    Ok(())
}

#[test]
fn cloned_metadata_save_reload_and_final_close_share_one_buffer()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, UnicodeFormat};
    use notepad_matrix::syntax::Language;
    use notepad_matrix::workspace::{CloseDisposition, SaveCompletion, ViewId, Workspace};
    let home = tempfile::tempdir()?;
    let path = home.path().join("shared.txt");
    std::fs::write(&path, "abc😀xyz")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.select(caret(10))?;
    workspace.clone_to_other_view(id)?;
    workspace.active_mut().editor.select(caret(2))?;
    workspace.active_mut().editor.set_overwrite(true);
    workspace.active_mut().set_language(Language::Json);
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16LeBom);
    workspace.active_mut().editor.insert_text("!")?;
    let intent = workspace.active().save_intent();
    let saved = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing file")?
        .save_with_format(intent.snapshot(), intent.format())?;
    workspace.complete_save(&intent, saved, SaveCompletion::Primary)?;
    workspace.focus_view(ViewId::Primary)?;
    assert_eq!(workspace.active().language(), Language::Json);
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16LeBom);
    assert!(!workspace.active().is_modified());
    std::fs::write(&path, "😀")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.reload_buffer(id, file, &text, CloseDisposition::IfUnmodified)?;
    assert_eq!(workspace.active().editor.selection(), caret(4));
    assert_eq!(
        workspace
            .editor_in_view(ViewId::Secondary, id)
            .ok_or("missing peer")?
            .selection(),
        caret(0)
    );
    assert_eq!(
        workspace
            .editor_in_view(ViewId::Secondary, id)
            .ok_or("missing peer")?
            .committed_text(),
        "😀"
    );
    assert!(
        workspace
            .editor_in_view(ViewId::Secondary, id)
            .ok_or("missing peer")?
            .overwrite()
    );
    assert!(
        !workspace
            .editor_in_view(ViewId::Secondary, id)
            .ok_or("missing peer")?
            .document()
            .can_undo()
    );
    workspace.active_mut().editor.insert_text("X")?;
    assert_eq!(
        workspace
            .editor_in_view(ViewId::Secondary, id)
            .ok_or("missing peer")?
            .committed_text(),
        "😀X"
    );
    workspace.close_in_view(ViewId::Primary, id, CloseDisposition::IfUnmodified)?;
    workspace.close_in_view(ViewId::Secondary, id, CloseDisposition::DiscardConfirmed)?;
    assert!(!workspace.buffers().iter().any(|buffer| buffer.id() == id));
    assert!(!workspace.two_views_visible());
    Ok(())
}

#[test]
fn grouped_multiline_edits_and_reversed_peer_selection_replay_once()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("a\nb\nc\nd");
    let mut peer = first.clone_view();
    peer.select(Selection {
        anchor: ByteOffset(7),
        head: ByteOffset(4),
    })?;
    peer.toggle_bookmark(Some(3))?;
    first.begin_undo_action();
    first.insert_text("X\n")?;
    first.insert_text("😀")?;
    first.end_undo_action();
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(13),
            head: ByteOffset(10)
        }
    );
    assert_eq!(
        peer.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![4]
    );
    assert!(peer.undo());
    assert_eq!(first.committed_text(), "a\nb\nc\nd");
    assert_eq!(first.selection(), caret(0));
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(7),
            head: ByteOffset(4)
        }
    );
    assert!(!first.document().can_undo());
    assert_eq!(
        first.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![3]
    );
    assert!(first.redo());
    assert_eq!(peer.committed_text(), "X\n😀a\nb\nc\nd");
    assert_eq!(first.selection(), caret(6));
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(13),
            head: ByteOffset(10)
        }
    );
    assert_eq!(
        peer.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![4]
    );
    assert!(first.document().shares_document_with(peer.document()));
    Ok(())
}

#[test]
fn sole_tab_move_is_noop_and_existing_clone_transfer_deduplicates()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::workspace::{ViewId, Workspace};
    let mut workspace = Workspace::new();
    let id = workspace.active().id();
    workspace.move_to_other_view(id)?;
    assert_eq!(workspace.active_view(), ViewId::Primary);
    assert!(!workspace.two_views_visible());
    workspace.clone_to_other_view(id)?;
    workspace.active_mut().editor.select(caret(0))?;
    workspace.clone_to_other_view(id)?;
    assert_eq!(workspace.view_buffer_ids(ViewId::Primary), &[id]);
    assert_eq!(workspace.view_buffer_ids(ViewId::Secondary), &[id]);
    assert_eq!(workspace.buffers().len(), 1);
    workspace.move_to_other_view(id)?;
    assert_eq!(workspace.active_view(), ViewId::Secondary);
    assert!(workspace.view_buffer_ids(ViewId::Primary).is_empty());
    assert!(!workspace.two_views_visible());
    Ok(())
}

#[test]
fn replace_all_maps_peer_selection_at_each_edit_and_shared_readonly_blocks_both()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::search::LiteralOptions;
    let mut first = EditorSession::new("aa-mid-aa-end");
    let mut peer = first.clone_view();
    peer.select(Selection {
        anchor: ByteOffset(6),
        head: ByteOffset(3),
    })?;
    assert_eq!(
        first.replace_all_literal(
            "aa",
            "😀",
            LiteralOptions {
                wrap: true,
                ..LiteralOptions::default()
            }
        )?,
        2
    );
    assert_eq!(peer.committed_text(), "😀-mid-😀-end");
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(8),
            head: ByteOffset(5)
        }
    );
    first.select(caret(17))?;
    assert!(peer.undo());
    assert_eq!(first.selection(), caret(0));
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(3)
        }
    );
    assert_eq!(first.committed_text(), "aa-mid-aa-end");
    first.set_read_only(true);
    assert!(peer.document().is_read_only());
    assert!(peer.insert_text("X").is_err());
    assert!(!peer.redo());
    first.set_read_only(false);
    assert!(peer.redo());
    assert_eq!(first.selection(), caret(0));
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(8),
            head: ByteOffset(5)
        }
    );
    Ok(())
}

#[test]
fn scoped_view_switch_preserves_both_active_documents_and_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::workspace::{ViewId, Workspace};
    let mut workspace = Workspace::new();
    let a = workspace.active().id();
    workspace.active_mut().editor.insert_text("aaa")?;
    let b = workspace.new_buffer()?;
    workspace.active_mut().editor.insert_text("bbb")?;
    workspace.activate(a)?;
    workspace.clone_to_other_view(a)?;
    workspace.active_mut().editor.select(caret(1))?;
    let c = workspace.new_buffer()?;
    workspace.active_mut().editor.insert_text("ccc")?;
    workspace.focus_view(ViewId::Primary)?;
    assert_eq!(workspace.active().id(), a);
    assert_eq!(workspace.active().editor.selection(), caret(3));
    let revision = workspace.active().editor.display_revision();
    workspace.set_active_view(ViewId::Secondary)?;
    assert_eq!(workspace.active().id(), c);
    workspace.set_active_view(ViewId::Primary)?;
    assert_eq!(workspace.active().id(), a);
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert_eq!(
        workspace
            .editor_in_view(ViewId::Secondary, a)
            .ok_or("missing clone")?
            .selection(),
        caret(1)
    );
    assert_eq!(workspace.close_right_targets(), vec![b]);
    assert_eq!(workspace.neighbor_id(true), b);
    workspace.activate(b)?;
    workspace.move_to_other_view(b)?;
    assert_eq!(workspace.active().id(), b);
    assert_eq!(workspace.active().editor.committed_text(), "bbb");
    assert_eq!(workspace.active_id_in_view(ViewId::Primary), Some(a));
    assert_eq!(workspace.view_buffer_ids(ViewId::Secondary), &[a, c, b]);
    assert_eq!(workspace.buffers().len(), 3);
    Ok(())
}

#[test]
fn peer_insertion_affinity_preserves_nonempty_ranges_but_not_empty_carets()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("abcdef");
    let mut peer = first.clone_view();
    peer.select(Selection {
        anchor: ByteOffset(4),
        head: ByteOffset(2),
    })?;
    first.select(caret(2))?;
    first.insert_text("XY")?;
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(4)
        }
    );
    assert!(first.undo());
    assert_eq!(
        peer.selection(),
        Selection {
            anchor: ByteOffset(4),
            head: ByteOffset(2)
        }
    );
    peer.select(caret(2))?;
    first.insert_text("XY")?;
    assert_eq!(peer.selection(), caret(2));
    assert!(first.undo());
    first.replace_display_range(ByteOffset(1)..ByteOffset(5), "😀")?;
    assert_eq!(first.selection(), caret(5));
    assert_eq!(peer.selection(), caret(1));
    assert!(first.undo());
    assert_eq!(first.selection(), caret(2));
    assert_eq!(peer.selection(), caret(2));
    assert!(first.redo());
    assert_eq!(first.selection(), caret(5));
    assert_eq!(peer.selection(), caret(1));
    Ok(())
}

#[test]
fn shared_readonly_cancels_peer_preedit_without_resurrecting_it()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("abc");
    let mut peer = first.clone_view();
    peer.set_composition("候")?;
    first.set_read_only(true);
    assert!(!peer.has_composition());
    first.set_read_only(false);
    assert!(!peer.has_composition());
    assert_eq!(peer.display_text(), "abc");
    peer.commit_composition()?;
    assert_eq!(first.committed_text(), "abc");
    Ok(())
}

#[test]
fn language_detection_and_explicit_choice_are_shared_distinct_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::syntax::Language;
    use notepad_matrix::workspace::{ViewId, Workspace};
    let mut workspace = Workspace::new();
    assert!(!workspace.active().language_is_explicit());
    workspace.new_buffer()?;
    assert!(!workspace.active().language_is_explicit());
    let home = tempfile::tempdir()?;
    let path = home.path().join("automatic.json");
    std::fs::write(&path, "{}")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let id = workspace.open_buffer(file, &text)?;
    assert_eq!(workspace.active().language(), Language::Json);
    assert!(!workspace.active().language_is_explicit());
    workspace.active_mut().set_language(Language::Json);
    assert!(workspace.active().language_is_explicit());
    workspace.clone_to_other_view(id)?;
    assert!(workspace.active().language_is_explicit());
    workspace.active_mut().set_language(Language::NormalText);
    workspace.focus_view(ViewId::Primary)?;
    assert_eq!(workspace.active().language(), Language::NormalText);
    assert!(workspace.active().language_is_explicit());
    workspace
        .active_mut()
        .set_detected_language(Language::UserDefined(42));
    workspace.focus_view(ViewId::Secondary)?;
    assert_eq!(workspace.active().language(), Language::UserDefined(42));
    assert!(!workspace.active().language_is_explicit());
    assert!(!workspace.active().is_modified());
    assert!(!workspace.active().editor.document().can_undo());
    Ok(())
}

#[test]
fn monitoring_readonly_and_system_readonly_stop_policy_apply_to_both_views()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::EditError;
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::monitoring::Monitoring;
    use notepad_matrix::workspace::{ViewId, Workspace};
    for system_readonly in [false, true] {
        let home = tempfile::tempdir()?;
        let path = home.path().join("shared.log");
        std::fs::write(&path, "log\n")?;
        if system_readonly {
            let mut permissions = std::fs::metadata(&path)?.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&path, permissions)?;
        }
        let (file, text) = DiskFile::load_unicode(&path)?;
        assert_eq!(file.is_read_only(), system_readonly);
        let mut workspace = Workspace::new();
        let id = workspace.open_buffer(file, &text)?;
        workspace.clone_to_other_view(id)?;
        workspace.focus_view(ViewId::Primary)?;
        workspace.active_mut().editor.set_read_only(true);
        let mut monitoring = Monitoring::default();
        monitoring.start(&mut workspace, id)?;
        assert_eq!(monitoring.requests(&mut workspace).len(), 1);
        for view in [ViewId::Primary, ViewId::Secondary] {
            let editor = workspace
                .editor_in_view_mut(view, id)
                .ok_or("missing view")?;
            assert!(editor.document().is_read_only());
            assert!(matches!(editor.insert_text("X"), Err(EditError::ReadOnly)));
            assert_eq!(editor.committed_text(), "log\n");
        }
        workspace.focus_view(ViewId::Secondary)?;
        monitoring.stop(&mut workspace, id);
        assert!(!monitoring.is_monitoring(id));
        for view in [ViewId::Primary, ViewId::Secondary] {
            assert_eq!(
                workspace
                    .editor_in_view(view, id)
                    .ok_or("missing view")?
                    .document()
                    .is_read_only(),
                system_readonly
            );
        }
        assert!(!workspace.active().is_modified());
        if system_readonly {
            assert!(
                workspace
                    .editor_in_view_mut(ViewId::Primary, id)
                    .ok_or("missing view")?
                    .insert_text("X")
                    .is_err()
            );
        } else {
            workspace
                .editor_in_view_mut(ViewId::Primary, id)
                .ok_or("missing view")?
                .insert_text("X")?;
            assert_eq!(workspace.active().editor.committed_text(), "Xlog\n");
        }
    }
    Ok(())
}

#[test]
fn grouped_deletion_and_cross_view_edits_restore_each_directional_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("abcdef\nz");
    let mut peer = first.clone_view();
    let before_first = Selection {
        anchor: ByteOffset(5),
        head: ByteOffset(1),
    };
    let before_peer = Selection {
        anchor: ByteOffset(2),
        head: ByteOffset(4),
    };
    first.select(before_first)?;
    peer.select(before_peer)?;
    first.toggle_bookmark(Some(1))?;
    first.begin_undo_action();
    first.insert_text("😀")?;
    peer.insert_text("\nQ")?;
    first.end_undo_action();
    assert_eq!(first.committed_text(), "a\nQ😀f\nz");
    assert_eq!(first.selection(), caret(7));
    assert_eq!(peer.selection(), caret(3));
    assert_eq!(
        first.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![2]
    );
    first.select(caret(0))?;
    peer.select(caret(8))?;
    assert!(peer.undo());
    assert_eq!(first.committed_text(), "abcdef\nz");
    assert_eq!(first.selection(), before_first);
    assert_eq!(peer.selection(), before_peer);
    assert_eq!(
        peer.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![1]
    );
    assert!(!first.document().can_undo());
    first.select(caret(6))?;
    peer.select(caret(0))?;
    assert!(first.redo());
    assert_eq!(peer.committed_text(), "a\nQ😀f\nz");
    assert_eq!(first.selection(), caret(7));
    assert_eq!(peer.selection(), caret(3));
    assert_eq!(
        peer.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![2]
    );
    assert!(!peer.document().can_redo());
    assert!(first.undo());
    assert_eq!(first.selection(), before_first);
    assert_eq!(peer.selection(), before_peer);
    assert!(peer.redo());
    assert_eq!(first.selection(), caret(7));
    assert_eq!(peer.selection(), caret(3));
    Ok(())
}

#[test]
fn view_attached_after_an_action_uses_source_caret_fallback_not_another_views_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut first = EditorSession::new("abcdef");
    let original = Selection {
        anchor: ByteOffset(5),
        head: ByteOffset(2),
    };
    first.select(original)?;
    first.insert_text("XY")?;
    let mut peer = first.clone_view();
    peer.select(caret(0))?;
    assert!(first.undo());
    assert_eq!(first.selection(), original);
    assert_eq!(peer.selection(), caret(5));
    assert!(peer.redo());
    assert_eq!(first.selection(), caret(4));
    assert_eq!(peer.selection(), caret(4));
    Ok(())
}
