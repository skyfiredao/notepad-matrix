use notepad_matrix::files::{DiskFile, UnicodeFormat};
use notepad_matrix::monitoring::{FileObservation, MonitorUpdate, Monitoring};
use notepad_matrix::workspace::Workspace;

#[test]
fn start_and_restart_capture_current_disk_baseline_without_reloading_pre_start_changes()
-> Result<(), Box<dyn std::error::Error>> {
    for restart in [false, true] {
        let home = tempfile::tempdir()?;
        let path = home.path().join("log");
        std::fs::write(&path, "A\n")?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let mut workspace = Workspace::new();
        let id = workspace.open_buffer(file, &text)?;
        std::fs::write(&path, "A\nB中😀\n")?;
        let mut monitoring = Monitoring::default();
        let revision = workspace.active().editor.display_revision();
        let selection = workspace.active().editor.selection();
        monitoring.start(&mut workspace, id)?;
        for _ in 0..2 {
            let result = monitoring.requests(&mut workspace).remove(0).poll();
            assert_eq!(
                monitoring.apply(&mut workspace, result)?,
                MonitorUpdate::Unchanged
            );
            assert_eq!(workspace.active().editor.committed_text(), "A\n");
            assert_eq!(workspace.active().editor.display_revision(), revision);
            assert_eq!(workspace.active().editor.selection(), selection);
        }
        if restart {
            std::fs::write(&path, "A\nB中😀\nC\n")?;
            let discarded = monitoring.requests(&mut workspace).remove(0).poll();
            monitoring.stop(&mut workspace, id);
            monitoring.start(&mut workspace, id)?;
            assert_eq!(
                monitoring.apply(&mut workspace, discarded)?,
                MonitorUpdate::Stale
            );
            assert_eq!(workspace.active().editor.committed_text(), "A\n");
            let result = monitoring.requests(&mut workspace).remove(0).poll();
            assert_eq!(
                monitoring.apply(&mut workspace, result)?,
                MonitorUpdate::Unchanged
            );
            assert_eq!(workspace.active().editor.display_revision(), revision);
            assert_eq!(workspace.active().editor.selection(), selection);
        }
        std::fs::write(&path, "A\nB中😀\nC\nD\n")?;
        let result = monitoring.requests(&mut workspace).remove(0).poll();
        assert_eq!(
            monitoring.apply(&mut workspace, result)?,
            MonitorUpdate::Reloaded {
                id,
                follow_tail: true,
            },
            "restart={restart}"
        );
        assert_eq!(
            workspace.active().editor.committed_text(),
            "A\nB中😀\nC\nD\n"
        );
        assert_eq!(workspace.active().editor.selection().head.0, 15);
        assert!(workspace.active().editor.document().is_read_only());
        assert!(!workspace.active().is_modified());
        assert!(!workspace.active_mut().editor.undo());
        let result = monitoring.requests(&mut workspace).remove(0).poll();
        assert_eq!(
            monitoring.apply(&mut workspace, result)?,
            MonitorUpdate::Unchanged
        );
    }
    Ok(())
}

#[test]
fn unchanged_start_polls_preserve_caret_revision_and_redo_history()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "A\nB\n")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.insert_text("local")?;
    assert!(workspace.active_mut().editor.undo());
    let revision = workspace.active().editor.display_revision();
    let selection = workspace.active().editor.selection();
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let first = monitoring.requests(&mut workspace).remove(0).poll();
    let second = monitoring.requests(&mut workspace).remove(0).poll();
    assert_eq!(
        monitoring.apply(&mut workspace, first)?,
        MonitorUpdate::Unchanged
    );
    assert_eq!(
        monitoring.apply(&mut workspace, second)?,
        MonitorUpdate::Unchanged
    );
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert_eq!(workspace.active().editor.selection(), selection);
    assert!(!workspace.active().is_modified());
    monitoring.stop(&mut workspace, id);
    assert!(workspace.active_mut().editor.redo());
    assert_eq!(workspace.active().editor.committed_text(), "localA\nB\n");
    Ok(())
}

#[test]
fn pre_start_malformed_content_is_not_decoded_until_a_post_start_change()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "A")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    std::fs::write(&path, b"\xff\xfe\x00")?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let revision = workspace.active().editor.display_revision();
    for _ in 0..2 {
        let result = monitoring.requests(&mut workspace).remove(0).poll();
        assert_eq!(
            monitoring.apply(&mut workspace, result)?,
            MonitorUpdate::Unchanged
        );
        assert_eq!(workspace.active().editor.display_revision(), revision);
        assert_eq!(workspace.active().editor.committed_text(), "A");
        assert!(!workspace.active().is_modified());
    }
    std::fs::write(&path, b"\xff\xfe\x00\xd8")?;
    let result = monitoring.requests(&mut workspace).remove(0).poll();
    assert!(matches!(
        monitoring.apply(&mut workspace, result)?,
        MonitorUpdate::Reloaded { .. }
    ));
    assert_eq!(workspace.active().editor.committed_text(), "");
    let revision = workspace.active().editor.display_revision();
    assert!(!workspace.active().is_modified());

    std::fs::write(&path, "B\n")?;
    let loaded = monitoring.requests(&mut workspace).remove(0).poll();
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16BeBom);
    assert!(monitoring.apply(&mut workspace, loaded).is_err());
    assert_eq!(workspace.active().editor.committed_text(), "");
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(workspace.active().is_modified());
    assert!(monitoring.requests(&mut workspace).is_empty());
    assert_eq!(std::fs::read_to_string(&path)?, "B\n");
    assert!(!workspace.active().editor.is_modified());
    Ok(())
}

#[test]
fn monitoring_requires_an_existing_clean_file_and_stop_clears_user_read_only()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    let mut workspace = Workspace::new();
    let mut monitoring = Monitoring::default();
    let unnamed = workspace.active().id();
    assert!(monitoring.start(&mut workspace, unnamed).is_err());
    assert!(!monitoring.is_monitoring(unnamed));
    assert!(!workspace.active().editor.document().is_read_only());

    std::fs::write(&path, "line\n")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let id = workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.insert_text("dirty")?;
    let revision = workspace.active().editor.display_revision();
    assert!(monitoring.start(&mut workspace, id).is_err());
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(workspace.active().is_modified());
    assert!(workspace.active_mut().editor.undo());
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16LeBom);
    assert!(monitoring.start(&mut workspace, id).is_err());
    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.reload_buffer(
        id,
        file,
        &text,
        notepad_matrix::workspace::CloseDisposition::DiscardConfirmed,
    )?;
    workspace.active_mut().editor.set_read_only(true);
    let revision = workspace.active().editor.display_revision();
    monitoring.start(&mut workspace, id)?;
    assert!(monitoring.is_monitoring(id));
    assert!(workspace.active().editor.document().is_read_only());
    assert!(!workspace.active().is_modified());
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(!workspace.active_mut().editor.undo());
    monitoring.stop(&mut workspace, id);
    assert!(!monitoring.is_monitoring(id));
    assert!(!workspace.active().editor.document().is_read_only());
    assert_eq!(workspace.active().editor.display_revision(), revision);
    std::fs::remove_file(&path)?;
    assert!(monitoring.start(&mut workspace, id).is_err());
    assert!(!monitoring.is_monitoring(id));
    Ok(())
}

#[test]
fn external_append_reloads_clean_buffers_and_only_active_buffer_follows_tail()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "A\n")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert_eq!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Unchanged
    );
    std::fs::write(&path, "A\n中😀\n")?;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert_eq!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Reloaded {
            id,
            follow_tail: true
        }
    );
    assert_eq!(workspace.active().editor.committed_text(), "A\n中😀\n");
    assert_eq!(workspace.active().editor.selection().head.0, 10);
    assert!(workspace.active().editor.document().is_read_only());
    assert!(!workspace.active().is_modified());
    assert!(!workspace.active_mut().editor.undo());

    let other = workspace.new_buffer()?;
    assert!(!monitoring.is_monitoring(other));
    assert!(!workspace.active().editor.document().is_read_only());
    std::fs::write(&path, "A\n中😀\nnext\n")?;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert_eq!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Reloaded {
            id,
            follow_tail: false
        }
    );
    assert_eq!(workspace.active().id(), other);
    assert_eq!(workspace.active().editor.committed_text(), "");
    workspace.activate(id)?;
    assert_eq!(
        workspace.active().editor.committed_text(),
        "A\n中😀\nnext\n"
    );
    assert_eq!(workspace.active().editor.selection().head.0, 10);
    monitoring.stop(&mut workspace, id);
    assert!(!workspace.active().editor.document().is_read_only());
    Ok(())
}

#[test]
fn replaced_file_stops_monitoring_like_removed_or_renamed_source_notification()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "old")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let request = monitoring.requests(&mut workspace).remove(0);
    let mut observation = FileObservation::read(&path)?;
    observation.identity = Some((u64::MAX, u64::MAX));
    let update = monitoring.apply(
        &mut workspace,
        request.poll_with_observation(Ok(observation)),
    )?;
    assert_eq!(update, MonitorUpdate::Stopped { id });
    assert!(!monitoring.is_monitoring(id));
    assert!(!workspace.active().editor.document().is_read_only());
    assert_eq!(workspace.active().editor.committed_text(), "old");
    assert!(!workspace.active().is_modified());
    Ok(())
}

#[test]
fn poll_batches_are_bounded_and_round_robin_without_starving_background_buffers()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    let mut monitoring = Monitoring::default();
    let mut ids = Vec::new();
    for index in 0..20 {
        let path = home.path().join(format!("log-{index}"));
        std::fs::write(&path, "log")?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let id = workspace.open_buffer(file, &text)?;
        monitoring.start(&mut workspace, id)?;
        ids.push(id);
    }
    let mut polled = std::collections::BTreeSet::new();
    for _ in 0..2 {
        let requests = monitoring.requests(&mut workspace);
        assert_eq!(
            requests.len(),
            16,
            "at most sixteen file probes in one tick"
        );
        for request in requests {
            polled.insert(request.buffer_id());
            assert_eq!(
                monitoring.apply(&mut workspace, request.poll())?,
                MonitorUpdate::Unchanged
            );
        }
    }
    assert_eq!(polled.into_iter().collect::<Vec<_>>(), ids);
    Ok(())
}

#[test]
fn failures_and_stale_results_never_replace_dirty_text_or_advance_its_savepoint()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "old")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;

    let request = monitoring.requests(&mut workspace).remove(0);
    let revision = workspace.active().editor.display_revision();
    let result = request.poll_with_observation(Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "injected metadata failure",
    )));
    assert!(monitoring.apply(&mut workspace, result).is_err());
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert_eq!(workspace.active().editor.committed_text(), "old");
    assert!(!workspace.active().is_modified());
    assert!(monitoring.is_monitoring(id));

    std::fs::write(&path, b"\xff\xfe\x00")?;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert!(matches!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Reloaded { .. }
    ));
    assert_eq!(workspace.active().editor.committed_text(), "");
    assert!(!workspace.active().is_modified());

    std::fs::write(&path, "external append\n")?;
    let request = monitoring.requests(&mut workspace).remove(0);
    let loaded = request.poll();
    monitoring.stop(&mut workspace, id);
    monitoring.start(&mut workspace, id)?;
    assert_eq!(
        monitoring.apply(&mut workspace, loaded)?,
        MonitorUpdate::Stale
    );
    assert_eq!(workspace.active().editor.committed_text(), "");

    std::fs::write(&path, "external append\nmore\n")?;
    let request = monitoring.requests(&mut workspace).remove(0);
    let loaded = request.poll();
    workspace
        .active_mut()
        .select_format(UnicodeFormat::Utf16BeBom);
    assert!(monitoring.apply(&mut workspace, loaded).is_err());
    assert!(workspace.active().is_modified());
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16BeBom);
    assert_eq!(workspace.active().editor.committed_text(), "");
    assert!(monitoring.requests(&mut workspace).is_empty());

    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.reload_buffer(
        id,
        file,
        &text,
        notepad_matrix::workspace::CloseDisposition::DiscardConfirmed,
    )?;
    monitoring.stop(&mut workspace, id);
    monitoring.start(&mut workspace, id)?;
    let request = monitoring.requests(&mut workspace).remove(0);
    std::fs::write(&path, "replacement text for in-flight read\n")?;
    let loaded = request.poll();
    workspace.active_mut().editor.set_read_only(false);
    workspace.active_mut().editor.insert_text("local")?;
    assert_eq!(
        monitoring.apply(&mut workspace, loaded)?,
        MonitorUpdate::Stale
    );
    assert_eq!(
        workspace.active().editor.committed_text(),
        "localexternal append\nmore\n"
    );
    assert!(workspace.active().is_modified());
    assert!(workspace.active_mut().editor.undo());
    assert!(!workspace.active().is_modified());
    assert!(workspace.active_mut().editor.redo());
    assert!(workspace.active().is_modified());
    Ok(())
}

#[test]
fn missing_file_stops_and_closed_buffers_are_pruned_without_loading_another_buffer()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "old")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let request = monitoring.requests(&mut workspace).remove(0);
    let result = request.poll_with_observation(Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "injected removal notification",
    )));
    assert_eq!(
        monitoring.apply(&mut workspace, result)?,
        MonitorUpdate::Stopped { id }
    );
    assert_eq!(workspace.active().editor.committed_text(), "old");
    assert!(!workspace.active().is_modified());
    assert!(!workspace.active().editor.document().is_read_only());

    monitoring.start(&mut workspace, id)?;
    let request = monitoring.requests(&mut workspace).remove(0);
    workspace.close_buffer(
        id,
        notepad_matrix::workspace::CloseDisposition::IfUnmodified,
    )?;
    assert!(monitoring.requests(&mut workspace).is_empty());
    assert!(monitoring.is_empty());
    assert_eq!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Stale
    );
    assert_eq!(workspace.active().editor.committed_text(), "");
    Ok(())
}

#[test]
fn size_changes_reload_even_at_same_timestamp_and_failed_observations_are_retryable()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    std::fs::write(&path, "old")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let original = FileObservation::read(&path)?;
    let revision = workspace.active().editor.display_revision();
    let mut changed_during_read = original.clone();
    changed_during_read.length += 1;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert!(
        monitoring
            .apply(
                &mut workspace,
                request.poll_with_observation(Ok(changed_during_read))
            )
            .is_err()
    );
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(!workspace.active().is_modified());
    std::fs::write(&path, "old append")?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)?
        .set_modified(original.modified)?;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert_eq!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Reloaded {
            id,
            follow_tail: true
        }
    );
    assert_eq!(workspace.active().editor.committed_text(), "old append");
    assert_eq!(workspace.active().editor.selection().head.0, 10);
    assert!(!workspace.active().is_modified());
    Ok(())
}

#[test]
fn utf16_monitoring_reloads_truncation_and_stop_keeps_system_read_only()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("log");
    let encode = |text: &str| {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    };
    std::fs::write(&path, encode("old😀\r\n"))?;
    let writable_permissions = std::fs::metadata(&path)?.permissions();
    let mut permissions = writable_permissions.clone();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions)?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    monitoring.stop(&mut workspace, id);
    assert!(workspace.active().editor.document().is_read_only());
    std::fs::set_permissions(&path, writable_permissions)?;
    monitoring.start(&mut workspace, id)?;
    std::fs::write(&path, encode("中"))?;
    let request = monitoring.requests(&mut workspace).remove(0);
    assert_eq!(
        monitoring.apply(&mut workspace, request.poll())?,
        MonitorUpdate::Reloaded {
            id,
            follow_tail: true
        }
    );
    assert_eq!(workspace.active().editor.committed_text(), "中");
    assert_eq!(workspace.active().editor.selection().head.0, 3);
    assert_eq!(workspace.active().format(), UnicodeFormat::Utf16LeBom);
    assert!(!workspace.active().is_modified());
    assert!(!workspace.active_mut().editor.undo());
    Ok(())
}
