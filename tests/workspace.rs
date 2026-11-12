use notepad_matrix::workspace::CloseDisposition;
use notepad_matrix::workspace::{BufferId, Workspace};

#[test]
fn large_file_flag_uses_loaded_bytes_and_stays_fixed_after_edits_and_policy_changes()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::workspace::LargeFileRestriction;
    let home = tempfile::tempdir()?;
    let path = home.path().join("wide.txt");
    std::fs::write(&path, [0xff, 0xfe, b'A', 0, b'B', 0])?;
    let mut workspace = Workspace::with_large_file_restriction(LargeFileRestriction {
        enabled: true,
        threshold_bytes: 4,
        allow_auto_completion: false,
    });
    workspace
        .active_mut()
        .editor
        .insert_text("long untitled text")?;
    assert!(!workspace.active().is_large_file());
    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.open_buffer(file, &text)?;
    assert!(workspace.active().is_large_file());
    assert!(!workspace.auto_completion_allowed());
    let policy = LargeFileRestriction::from_settings(
        &serde_json::json!({"large_file_restriction":{"allow_auto_completion":true}}),
    )?;
    workspace.set_large_file_restriction(policy);
    assert!(workspace.auto_completion_allowed());
    workspace
        .active_mut()
        .editor
        .select(notepad_matrix::editor::Selection {
            anchor: notepad_matrix::editor::ByteOffset(0),
            head: notepad_matrix::editor::ByteOffset(text.len()),
        })?;
    workspace.active_mut().editor.insert_text("")?;
    workspace.set_large_file_restriction(LargeFileRestriction {
        enabled: false,
        threshold_bytes: 4,
        allow_auto_completion: false,
    });
    assert!(workspace.active().is_large_file());
    assert!(workspace.auto_completion_allowed());
    let other = home.path().join("disabled.txt");
    std::fs::write(&other, "long enough")?;
    let (file, text) = DiskFile::load_unicode(&other)?;
    workspace.open_buffer(file, &text)?;
    assert!(!workspace.active().is_large_file());
    Ok(())
}

#[test]
fn close_right_targets_capture_farthest_first_and_preserve_the_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    let left = workspace.active().id();
    assert!(workspace.close_right_targets().is_empty());
    let anchor = workspace.new_buffer()?;
    let nearest = workspace.new_buffer()?;
    let middle = workspace.new_buffer()?;
    let farthest = workspace.new_buffer()?;
    workspace.activate(anchor)?;
    let captured = workspace.close_right_targets();
    assert_eq!(captured, vec![farthest, middle, nearest]);
    assert_eq!(workspace.close_left_targets(), vec![left]);
    workspace.close_buffer(farthest, CloseDisposition::IfUnmodified)?;
    assert_eq!(workspace.active().id(), anchor);
    assert_eq!(captured, vec![farthest, middle, nearest]);
    let appended = workspace.new_buffer()?;
    workspace.activate(anchor)?;
    assert_eq!(
        workspace.close_right_targets(),
        vec![appended, middle, nearest]
    );
    workspace.activate(middle)?;
    assert_eq!(workspace.close_right_targets(), vec![appended]);
    assert_eq!(workspace.close_left_targets(), vec![nearest, anchor, left]);
    workspace.activate(appended)?;
    assert!(workspace.close_right_targets().is_empty());
    Ok(())
}

#[test]
fn approved_clean_or_saved_close_target_is_rechecked_for_text_and_format_dirtiness()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, UnicodeFormat};
    use notepad_matrix::workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    for saved in [false, true] {
        for format_change in [false, true] {
            let path = home.path().join(format!("recheck-{saved}-{format_change}"));
            std::fs::write(&path, b"A")?;
            let mut workspace = Workspace::new();
            let (file, text) = DiskFile::load_unicode(&path)?;
            let target = workspace.open_buffer(file, &text)?;
            if saved {
                workspace
                    .active_mut()
                    .editor
                    .move_to_document_edge(true, false)?;
                workspace.active_mut().editor.insert_text("X")?;
                let intent = workspace.active().save_intent();
                let file = workspace
                    .active()
                    .disk_file
                    .as_ref()
                    .ok_or("missing baseline")?
                    .save_with_format(intent.snapshot(), intent.format())?;
                workspace.complete_save(&intent, file, SaveCompletion::Primary)?;
            }
            assert!(!workspace.active().is_modified());
            if format_change {
                workspace
                    .active_mut()
                    .select_format(UnicodeFormat::Utf16LeBom);
            } else {
                workspace.active_mut().editor.insert_text("Y")?;
            }
            let text = workspace.active().editor.committed_text();
            let format = workspace.active().format();
            let anchor = workspace.new_buffer()?;
            assert!(
                workspace
                    .close_buffer(target, CloseDisposition::IfUnmodified)
                    .is_err()
            );
            assert_eq!(workspace.active().id(), anchor);
            workspace.activate(target)?;
            assert_eq!(workspace.active().editor.committed_text(), text);
            assert_eq!(workspace.active().format(), format);
            assert!(workspace.active().is_modified());
            assert_eq!(
                std::fs::read(path)?,
                if saved { &b"AX"[..] } else { &b"A"[..] }
            );
        }
    }
    Ok(())
}

#[test]
fn close_left_targets_capture_nearest_first_without_anchor_or_right_side()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    assert!(workspace.close_left_targets().is_empty());
    workspace.active_mut().editor.insert_text("first")?;
    let second = workspace.new_buffer()?;
    let anchor = workspace.new_buffer()?;
    workspace.active_mut().editor.insert_text("anchor")?;
    let right = workspace.new_buffer()?;
    workspace.active_mut().editor.insert_text("right")?;
    workspace.activate(anchor)?;
    let captured = workspace.close_left_targets();
    assert_eq!(captured, vec![second, first]);
    workspace.close_buffer(second, CloseDisposition::IfUnmodified)?;
    assert_eq!(workspace.active().id(), anchor);
    assert_eq!(workspace.close_left_targets(), vec![first]);
    assert_eq!(captured, vec![second, first]);
    assert_eq!(
        workspace
            .buffers()
            .iter()
            .map(|buffer| buffer.id())
            .collect::<Vec<_>>(),
        vec![first, anchor, right]
    );
    assert_eq!(workspace.active().editor.committed_text(), "anchor");
    Ok(())
}

#[test]
fn unicode_convert_all_format_pairs_clear_only_effective_changes_including_readonly()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{UnicodeFormat, Utf8Format};
    let formats = [
        UnicodeFormat::Utf8(Utf8Format::Plain),
        UnicodeFormat::Utf8(Utf8Format::Bom),
        UnicodeFormat::Utf16BeBom,
        UnicodeFormat::Utf16LeBom,
    ];
    for from in formats {
        for to in formats {
            let mut workspace = Workspace::new();
            workspace.active_mut().select_format(from);
            workspace.active_mut().editor.insert_text("A\0中😀")?;
            workspace.active_mut().editor.insert_text("X")?;
            assert!(workspace.active_mut().editor.undo());
            workspace.active_mut().editor.set_read_only(true);
            assert_eq!(workspace.active_mut().convert_format(to), from != to);
            assert_eq!(workspace.active().format(), to);
            assert_eq!(workspace.active().editor.committed_text(), "A\0中😀");
            assert_eq!(workspace.active().editor.is_modified(), from == to);
            assert!(workspace.active().is_modified());
            workspace.active_mut().editor.set_read_only(false);
            if from == to {
                assert!(workspace.active_mut().editor.redo());
                assert_eq!(workspace.active().editor.committed_text(), "A\0中😀X");
                assert!(workspace.active_mut().editor.undo());
                assert_eq!(workspace.active().editor.committed_text(), "A\0中😀");
                assert!(workspace.active_mut().editor.undo());
                assert_eq!(workspace.active().editor.committed_text(), "");
            } else {
                assert!(!workspace.active_mut().editor.redo());
                assert!(!workspace.active_mut().editor.undo());
                assert_eq!(workspace.active().editor.committed_text(), "A\0中😀");
            }
            let mut empty = Workspace::new();
            empty.active_mut().select_format(from);
            let dirty = empty.active().is_modified();
            empty.active_mut().convert_format(to);
            assert!(!empty.active().editor.is_modified());
            assert_eq!(empty.active().is_modified(), dirty || from != to);
            assert_eq!(empty.active().editor.committed_text(), "");
        }
    }
    Ok(())
}

#[test]
fn unicode_convert_same_target_keeps_intent_identity_but_aba_does_not_clean_newer_choice()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, OverwritePolicy, UnicodeFormat, Utf8Format};
    use notepad_matrix::workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    let path = home.path().join("conversion");
    let bom = UnicodeFormat::Utf8(Utf8Format::Bom);
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("A")?;
    workspace.active_mut().select_format(bom);
    let original = workspace.active().save_intent();
    assert!(!workspace.active_mut().convert_format(bom));
    let file = DiskFile::save_as_unicode(
        &path,
        original.snapshot(),
        original.format(),
        OverwritePolicy::Refuse,
    )?;
    workspace.complete_save(&original, file, SaveCompletion::Primary)?;
    assert!(!workspace.active().is_modified());
    let old = workspace.active().save_intent();
    assert!(
        workspace
            .active_mut()
            .convert_format(UnicodeFormat::Utf16LeBom)
    );
    assert!(workspace.active_mut().convert_format(bom));
    let file = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing baseline")?
        .save_with_format(old.snapshot(), old.format())?;
    workspace.complete_save(&old, file, SaveCompletion::Primary)?;
    assert!(!workspace.active().editor.is_modified());
    assert!(workspace.active().is_modified());
    let current = workspace.active().save_intent();
    let file = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing baseline")?
        .save_with_format(current.snapshot(), current.format())?;
    workspace.complete_save(&current, file, SaveCompletion::Primary)?;
    assert!(!workspace.active().is_modified());
    assert_eq!(std::fs::read(path)?, [0xef, 0xbb, 0xbf, 0x41]);
    Ok(())
}

#[test]
fn unicode_convert_clears_text_history_without_changing_text_or_preview()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::files::{DiskFile, OverwritePolicy, UnicodeFormat};
    use notepad_matrix::workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    let original = "A\0中😀\r\nZ";
    workspace.active_mut().editor.insert_text(original)?;
    let saved = workspace.active().save_intent();
    let file = DiskFile::save_as_unicode(
        &home.path().join("initial"),
        saved.snapshot(),
        saved.format(),
        OverwritePolicy::Refuse,
    )?;
    workspace.complete_save(&saved, file, SaveCompletion::Primary)?;
    workspace.active_mut().editor.insert_text("X")?;
    assert!(workspace.active_mut().editor.undo());
    assert!(!workspace.active().editor.is_modified());
    workspace.active_mut().editor.select(Selection {
        anchor: ByteOffset(12),
        head: ByteOffset(0),
    })?;
    workspace.active_mut().editor.set_composition("preview")?;
    let selection = workspace.active().editor.selection();
    let revision = workspace.active().editor.display_revision();
    assert!(
        workspace
            .active_mut()
            .convert_format(UnicodeFormat::Utf16BeBom)
    );
    assert_eq!(workspace.active().editor.committed_text(), original);
    assert_eq!(workspace.active().editor.display_text(), "preview");
    assert_eq!(workspace.active().editor.selection(), selection);
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(!workspace.active().editor.is_modified());
    assert!(workspace.active().is_modified());
    workspace.active_mut().editor.cancel_composition();
    assert!(!workspace.active_mut().editor.undo());
    assert!(!workspace.active_mut().editor.redo());
    assert_eq!(workspace.active().editor.committed_text(), original);
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16BeBom);
    Ok(())
}

#[test]
fn unicode_convert_resets_engine_savepoint_without_immediately_cleaning_buffer_dirty()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::UnicodeFormat;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("中\r\n😀")?;
    assert!(
        workspace
            .active_mut()
            .convert_format(UnicodeFormat::Utf16LeBom)
    );
    assert!(!workspace.active().editor.is_modified());
    assert!(workspace.active().is_modified());
    assert!(!workspace.active_mut().editor.undo());
    let mut other = workspace.active().editor.clone_view();
    other.insert_text("x")?;
    assert!(other.undo());
    assert!(!workspace.active().is_modified());
    assert_eq!(workspace.active().editor.committed_text(), "中\r\n😀");
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16LeBom);
    Ok(())
}

#[test]
fn foreign_and_post_reload_save_completions_preserve_every_buffer_state()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, OverwritePolicy, UnicodeFormat};
    use notepad_matrix::workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    let source = home.path().join("source");
    let wrong = home.path().join("wrong");
    std::fs::write(&source, b"A")?;
    let mut foreign = Workspace::new();
    foreign.active_mut().editor.insert_text("foreign")?;
    foreign
        .active_mut()
        .select_format(UnicodeFormat::Utf16LeBom);
    let alien = foreign.active().save_intent();
    let wrong_file = DiskFile::save_as_unicode(
        &wrong,
        alien.snapshot(),
        alien.format(),
        OverwritePolicy::Refuse,
    )?;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("local")?;
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16BeBom);
    assert!(
        workspace
            .complete_save(&alien, wrong_file, SaveCompletion::Primary)
            .is_err()
    );
    assert!(workspace.active().disk_file.is_none());
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16BeBom);
    assert!(workspace.active().is_modified());
    assert!(!workspace.active().editor.document().is_read_only());
    assert_eq!(workspace.active().editor.committed_text(), "local");
    assert!(workspace.active_mut().editor.undo());
    assert!(!workspace.active().is_modified());
    assert!(workspace.active_mut().editor.redo());
    let (file, text) = DiskFile::load_unicode(&source)?;
    let id = workspace.open_buffer(file, &text)?;
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16BeBom);
    let old = workspace.active().save_intent();
    let old_output = DiskFile::save_as_unicode(
        &wrong,
        old.snapshot(),
        old.format(),
        OverwritePolicy::Confirmed,
    )?;
    std::fs::write(&source, [0xff, 0xfe, 0x42, 0])?;
    let (file, text) = DiskFile::load_unicode(&source)?;
    workspace.reload_buffer(id, file, &text, CloseDisposition::DiscardConfirmed)?;
    workspace.active_mut().editor.insert_text("X")?;
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16BeBom);
    let selected = workspace.active().editor.selection();
    assert!(
        workspace
            .complete_save(&old, old_output, SaveCompletion::Copy)
            .is_err()
    );
    assert_eq!(
        workspace
            .active()
            .disk_file
            .as_ref()
            .ok_or("missing baseline")?
            .path(),
        std::fs::canonicalize(&source)?
    );
    assert_eq!(
        workspace
            .active()
            .disk_file
            .as_ref()
            .ok_or("missing baseline")?
            .format(),
        UnicodeFormat::Utf16LeBom
    );
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16BeBom);
    assert!(workspace.active().is_modified());
    assert!(workspace.active().editor.is_modified());
    assert_eq!(workspace.active().editor.selection(), selected);
    assert_eq!(workspace.active().editor.committed_text(), "XB");
    assert!(!workspace.active().editor.document().is_read_only());
    assert!(workspace.active_mut().editor.undo());
    assert!(!workspace.active().editor.is_modified());
    assert!(!workspace.active().is_modified());
    Ok(())
}

#[test]
fn same_primary_format_copy_refreshes_baseline_but_not_text_or_metadata_savepoints()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, OverwritePolicy, UnicodeFormat};
    use notepad_matrix::workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    let path = home.path().join("copy-primary");
    std::fs::write(&path, b"A")?;
    let mut workspace = Workspace::new();
    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.open_buffer(file, &text)?;
    workspace
        .active_mut()
        .editor
        .move_to_document_edge(true, false)?;
    workspace.active_mut().editor.insert_text("X")?;
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16LeBom);
    let copy = workspace.active().save_intent();
    let file = DiskFile::save_as_unicode(
        &path,
        copy.snapshot(),
        copy.format(),
        OverwritePolicy::Confirmed,
    )?;
    workspace.complete_save(&copy, file, SaveCompletion::Copy)?;
    assert!(workspace.active().editor.is_modified());
    assert!(workspace.active().is_modified());
    assert_eq!(std::fs::read(&path)?, [0xff, 0xfe, 0x41, 0, 0x58, 0]);
    workspace.active_mut().editor.undo();
    assert!(!workspace.active().editor.is_modified());
    assert!(!workspace.active().is_modified());
    let save = workspace.active().save_intent();
    let file = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing refreshed baseline")?
        .save_with_format(save.snapshot(), save.format())?;
    workspace.complete_save(&save, file, SaveCompletion::Primary)?;
    assert_eq!(std::fs::read(path)?, [0xff, 0xfe, 0x41, 0]);
    assert!(!workspace.active().is_modified());
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16LeBom);
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16BeBom);
    assert!(workspace.active().is_modified());
    workspace
        .active()
        .editor
        .document()
        .clone_view()
        .mark_saved();
    assert!(!workspace.active().is_modified());
    Ok(())
}

#[test]
fn format_save_intents_keep_newer_choices_until_next_text_savepoint_event()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, OverwritePolicy, UnicodeFormat, Utf8Format};
    use notepad_matrix::workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    let path = home.path().join("selected.txt");
    let plain = UnicodeFormat::Utf8(Utf8Format::Plain);
    let bom = UnicodeFormat::Utf8(Utf8Format::Bom);
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("A")?;
    workspace.active_mut().select_format(bom);
    let intent = workspace.active().save_intent();
    let file = DiskFile::save_as_unicode(
        &path,
        intent.snapshot(),
        intent.format(),
        OverwritePolicy::Refuse,
    )?;
    workspace.active_mut().select_format(plain);
    workspace.active_mut().select_format(bom);
    workspace.active_mut().editor.insert_text("X")?;
    workspace.complete_save(&intent, file, SaveCompletion::Primary)?;
    assert_eq!(std::fs::read(&path)?, [0xef, 0xbb, 0xbf, 0x41]);
    assert_eq!(workspace.active().format(), bom);
    assert_eq!(workspace.active().editor.committed_text(), "AX");
    assert!(workspace.active().is_modified());
    workspace.active_mut().editor.undo();
    assert!(!workspace.active().editor.is_modified());
    assert!(!workspace.active().is_modified());
    let intent = workspace.active().save_intent();
    workspace.active_mut().select_format(bom);
    let file = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing baseline")?
        .save_with_format(intent.snapshot(), intent.format())?;
    workspace.complete_save(&intent, file, SaveCompletion::Primary)?;
    assert!(
        !workspace.active().is_modified(),
        "same-target selection invalidated valid acknowledgment"
    );
    assert!(workspace.active_mut().editor.redo());
    assert!(workspace.active().is_modified());
    assert_eq!(workspace.active().format(), bom);
    Ok(())
}

#[test]
fn unicode_selection_keeps_format_but_text_savepoint_notifications_replace_dirty()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{UnicodeFormat, Utf8Format};
    let plain = UnicodeFormat::Utf8(Utf8Format::Plain);
    let bom = UnicodeFormat::Utf8(Utf8Format::Bom);
    let mut workspace = Workspace::new();
    assert_eq!(workspace.active().format(), plain);
    assert!(!workspace.active().is_modified());
    assert!(!workspace.active_mut().select_format(plain));
    assert!(!workspace.active().is_modified());
    workspace.active_mut().editor.insert_text("中😀")?;
    workspace.active_mut().editor.undo();
    let selection = workspace.active().editor.selection();
    let revision = workspace.active().editor.display_revision();
    assert!(workspace.active_mut().select_format(bom));
    assert!(workspace.active().is_modified());
    assert_eq!(workspace.active().editor.committed_text(), "");
    assert!(!workspace.active().editor.is_modified());
    assert_eq!(workspace.active().editor.selection(), selection);
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(workspace.active_mut().editor.redo());
    assert_eq!(workspace.active().editor.committed_text(), "中😀");
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().format(), bom);
    assert!(!workspace.active().is_modified());
    assert!(workspace.active_mut().select_format(plain));
    assert!(
        workspace.active().is_modified(),
        "returning to initial format must not silently clean metadata"
    );
    assert!(
        workspace
            .close_buffer(workspace.active().id(), CloseDisposition::IfUnmodified)
            .is_err()
    );
    assert_eq!(workspace.dirty_buffers(), vec![workspace.active().id()]);
    workspace.active_mut().editor.set_read_only(true);
    assert!(
        workspace
            .active_mut()
            .select_format(UnicodeFormat::Utf16LeBom)
    );
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16LeBom);
    Ok(())
}

#[test]
fn encoding_dirty_follows_shared_savepoint_events_even_without_intermediate_buffer_reads()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{UnicodeFormat, Utf8Format};
    let mut workspace = Workspace::new();
    let mut other = workspace.active().editor.clone_view();
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf8(Utf8Format::Bom));
    assert!(workspace.active().is_modified());
    other.insert_text("中\r\n😀")?;
    assert!(other.undo());
    assert!(!workspace.active().is_modified());
    assert_eq!(
        workspace.active().format(),
        UnicodeFormat::Utf8(Utf8Format::Bom)
    );
    assert!(other.redo());
    assert!(workspace.active().is_modified());
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16LeBom);
    other.insert_text("x")?;
    assert!(other.undo());
    assert!(workspace.active().is_modified());
    assert!(other.undo());
    assert!(!workspace.active().is_modified());
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16LeBom);
    Ok(())
}

#[test]
fn loaded_eol_modes_remain_independent_across_switch_history_save_and_rejected_reload()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::input::LineEnding;
    let home = tempfile::tempdir()?;
    let a = home.path().join("lf.txt");
    let b = home.path().join("cr-utf16.txt");
    std::fs::write(&a, b"a\nz")?;
    std::fs::write(&b, [0xfe, 0xff, 0, 0x62, 0, 0x0d, 0, 0x7a])?;
    let mut workspace = Workspace::new();
    let (file, text) = DiskFile::load_unicode(&a)?;
    let first = workspace.open_buffer(file, &text)?;
    workspace
        .active_mut()
        .editor
        .move_to_document_edge(true, false)?;
    workspace.active_mut().editor.insert_text("X")?;
    let snapshot = workspace.active().editor.save_snapshot();
    let saved = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("source file missing")?
        .save_unicode(&snapshot)?;
    workspace.active_mut().disk_file = Some(saved);
    workspace.active_mut().editor.acknowledge_save(&snapshot)?;
    assert_eq!(workspace.active().editor.line_ending(), LineEnding::Lf);
    let (file, text) = DiskFile::load_unicode(&b)?;
    let second = workspace.open_buffer(file, &text)?;
    assert_eq!(workspace.active().editor.line_ending(), LineEnding::Cr);
    workspace.active_mut().editor.insert_text("Y")?;
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.line_ending(), LineEnding::Cr);
    workspace.activate(first)?;
    assert!(workspace.active_mut().editor.undo());
    assert!(workspace.active().editor.is_modified());
    std::fs::write(&a, b"a\rchanged")?;
    let (file, text) = DiskFile::load_unicode(&a)?;
    assert!(
        workspace
            .reload_buffer(first, file, &text, CloseDisposition::IfUnmodified)
            .is_err()
    );
    assert_eq!(workspace.active().editor.line_ending(), LineEnding::Lf);
    assert_eq!(workspace.active().editor.committed_text(), "a\nz");
    assert!(workspace.active_mut().editor.redo());
    assert_eq!(workspace.active().editor.committed_text(), "a\nzX");
    assert_eq!(workspace.active().editor.line_ending(), LineEnding::Lf);
    workspace.activate(second)?;
    assert_eq!(workspace.active().editor.line_ending(), LineEnding::Cr);
    assert_eq!(workspace.active().editor.committed_text(), "b\rz");
    Ok(())
}

#[test]
fn reload_preserves_buffer_identity_but_replaces_history_and_clamps_selection()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::files::DiskFile;
    let home = tempfile::tempdir()?;
    let path = home.path().join("reload.txt");
    std::fs::write(&path, "old text")?;
    let (file, text) = DiskFile::load_utf8(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.select(Selection {
        anchor: ByteOffset(1),
        head: ByteOffset(8),
    })?;
    workspace.active_mut().editor.insert_text("dirty")?;
    std::fs::write(&path, "\u{feff}😀")?;
    let (file, text) = DiskFile::load_utf8(&path)?;
    assert!(
        workspace
            .reload_buffer(id, file.clone(), &text, CloseDisposition::IfUnmodified)
            .is_err()
    );
    assert!(workspace.active().editor.is_modified());
    workspace.reload_buffer(id, file, &text, CloseDisposition::DiscardConfirmed)?;
    assert_eq!(workspace.active().id(), id);
    assert_eq!(workspace.active().editor.committed_text(), "😀");
    assert_eq!(workspace.active().editor.selection().head, ByteOffset(4));
    assert!(!workspace.active().editor.is_modified());
    assert!(!workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.committed_text(), "😀");
    assert!(
        workspace
            .active()
            .disk_file
            .as_ref()
            .is_some_and(|file| file.format()
                == notepad_matrix::files::UnicodeFormat::Utf8(
                    notepad_matrix::files::Utf8Format::Bom
                ))
    );
    Ok(())
}

#[test]
fn closing_a_buffer_requires_discard_permission_and_never_empties_workspace()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    workspace.active_mut().editor.insert_text("unsaved")?;
    assert!(
        workspace
            .close_buffer(first, CloseDisposition::IfUnmodified)
            .is_err()
    );
    assert_eq!(workspace.active().editor.committed_text(), "unsaved");
    let second = workspace.new_buffer()?;
    workspace.active_mut().editor.insert_text("second")?;
    workspace.close_buffer(first, CloseDisposition::DiscardConfirmed)?;
    assert_eq!(workspace.active().id(), second);
    assert_eq!(workspace.active().editor.committed_text(), "second");
    let replacement = workspace.close_buffer(second, CloseDisposition::DiscardConfirmed)?;
    assert_ne!(replacement, second);
    assert_eq!(workspace.buffers().len(), 1);
    assert_eq!(workspace.active().name(), "new 1");
    assert_eq!(workspace.active().editor.committed_text(), "");
    assert!(!workspace.active().editor.is_modified());
    Ok(())
}

#[test]
fn readonly_open_disables_edits_without_changing_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("readonly.txt");
    std::fs::write(&path, b"read only")?;
    let mut permissions = std::fs::metadata(&path)?.permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions)?;
    let (file, text) = notepad_matrix::files::DiskFile::load_utf8(&path)?;
    let mut workspace = Workspace::new();
    workspace.open_buffer(file, &text)?;
    assert!(
        workspace
            .active_mut()
            .editor
            .insert_text("changed")
            .is_err()
    );
    assert_eq!(workspace.active().editor.committed_text(), "read only");
    assert!(!workspace.active().editor.is_modified());
    assert_eq!(std::fs::read(&path)?, b"read only");
    Ok(())
}

#[test]
fn untitled_numbers_follow_visible_untitled_buffers_not_internal_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("saved.txt");
    let mut workspace = Workspace::new();
    let file = notepad_matrix::files::DiskFile::create_utf8(
        &path,
        &workspace.active().editor.save_snapshot(),
    )?;
    workspace.active_mut().disk_file = Some(file);
    workspace.new_buffer()?;
    assert_eq!(workspace.active().name(), "new 1");
    workspace.new_buffer()?;
    assert_eq!(workspace.active().name(), "new 2");
    Ok(())
}

#[test]
fn opening_utf8_preserves_existing_drafts_and_reopening_deduplicates_the_path()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("opened.txt");
    std::fs::write(&path, "opened中😀\r\n".as_bytes())?;
    let mut workspace = Workspace::new();
    let draft = workspace.active().id();
    workspace.active_mut().editor.insert_text("draft")?;
    let (file, text) = notepad_matrix::files::DiskFile::load_utf8(&path)?;
    let opened = workspace.open_buffer(file, &text)?;
    assert_eq!(workspace.active().editor.committed_text(), "opened中😀\r\n");
    assert!(!workspace.active().editor.is_modified());
    workspace.active_mut().editor.insert_text("edited")?;
    let (file, text) = notepad_matrix::files::DiskFile::load_utf8(&path)?;
    assert_eq!(workspace.open_buffer(file, &text)?, opened);
    assert_eq!(workspace.buffers().len(), 2);
    assert!(
        workspace
            .active()
            .editor
            .committed_text()
            .starts_with("edited")
    );
    workspace.activate(draft)?;
    assert_eq!(workspace.active().editor.committed_text(), "draft");
    assert!(workspace.active().editor.is_modified());
    Ok(())
}

#[test]
fn new_and_switch_keep_each_buffers_text_selection_history_and_dirty_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    workspace.active_mut().editor.insert_text("first中😀")?;
    let selected = workspace.active().editor.selection();
    let second = workspace.new_buffer()?;
    assert_ne!(first, second);
    assert_eq!(workspace.active().editor.committed_text(), "");
    workspace.active_mut().editor.insert_text("second")?;
    workspace.activate(first)?;
    assert_eq!(workspace.active().editor.committed_text(), "first中😀");
    assert_eq!(workspace.active().editor.selection(), selected);
    assert!(workspace.active().editor.is_modified());
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.committed_text(), "");
    workspace.activate(second)?;
    assert_eq!(workspace.active().editor.committed_text(), "second");
    assert!(workspace.active().editor.is_modified());
    assert!(workspace.activate(BufferId(999)).is_err());
    assert_eq!(workspace.active().id(), second);
    Ok(())
}
