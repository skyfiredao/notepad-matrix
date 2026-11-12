use notepad_matrix::workspace::{CloseCondition, TabSort, ViewId, Workspace};

#[test]
fn clone_copies_source_viewport_only_for_new_membership_and_move_keeps_existing_peer()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::workspace::DocumentViewState;
    let mut workspace = Workspace::new();
    let id = workspace.active().id();
    let source = DocumentViewState {
        top_line: 7,
        viewport: None,
        horizontal_offset: 42.,
        folded_lines: vec![2],
        word_wrap: false,
    };
    workspace.set_view_state(ViewId::Primary, id, source.clone())?;
    workspace.clone_to_other_view(id)?;
    assert_eq!(workspace.view_state(ViewId::Secondary, id), Some(&source));
    let peer = DocumentViewState {
        top_line: 3,
        viewport: None,
        horizontal_offset: 0.,
        folded_lines: vec![],
        word_wrap: true,
    };
    workspace.set_view_state(ViewId::Secondary, id, peer.clone())?;
    workspace.focus_view(ViewId::Primary)?;
    workspace.clone_to_other_view(id)?;
    assert_eq!(workspace.view_state(ViewId::Secondary, id), Some(&peer));
    workspace.focus_view(ViewId::Primary)?;
    workspace.move_to_other_view(id)?;
    assert_eq!(workspace.view_state(ViewId::Secondary, id), Some(&peer));
    assert!(workspace.view_state(ViewId::Primary, id).is_none());
    Ok(())
}

#[test]
fn session_checkpoint_preserves_unknown_view_and_position_fields()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::config::ConfigStore;
    use notepad_matrix::recovery::RecoverySession;
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("draft")?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&workspace)?;
    let store = ConfigStore::under_home(home.path());
    let path = std::path::PathBuf::from(format!("backup/{}.session.json", session.key()));
    store.update_json(&path, &serde_json::Value::Null, |manifest| {
        manifest["views"][0]["extension"] = serde_json::json!({"keep":true});
        manifest["views"][0]["documents"][0]["state"]["extension"] = serde_json::json!(17);
        Ok(())
    })?;
    workspace.active_mut().editor.insert_text(" updated")?;
    session.checkpoint(&workspace)?;
    let manifest = store.load_json(&path)?;
    assert_eq!(manifest["views"][0]["extension"]["keep"], true);
    assert_eq!(
        manifest["views"][0]["documents"][0]["state"]["extension"],
        17
    );
    Ok(())
}

#[test]
fn recent_file_history_survives_restart_and_honors_source_limit_and_missing_close()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::recovery::RecoveryManager;
    use notepad_matrix::workspace::CloseDisposition;
    let home = tempfile::tempdir()?;
    let (mut manager, mut workspace) = RecoveryManager::open(home.path())?;
    workspace.configure_recent_files(&serde_json::json!({"recent_files_limit":2}))?;
    let mut paths = Vec::new();
    for number in 0..3 {
        let path = home.path().join(format!("recent-{number}.txt"));
        std::fs::write(&path, "disk")?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let id = workspace.open_buffer(file, &text)?;
        workspace.close_buffer(id, CloseDisposition::IfUnmodified)?;
        paths.push(path);
    }
    assert_eq!(
        workspace.recently_closed(),
        &[paths[2].clone(), paths[1].clone()]
    );
    manager.checkpoint(&workspace)?;
    drop(manager);
    let (_, mut restored) = RecoveryManager::open(home.path())?;
    assert_eq!(
        restored.recently_closed(),
        &[paths[2].clone(), paths[1].clone()]
    );
    let (file, text) = DiskFile::load_unicode(&paths[0])?;
    let id = restored.open_buffer(file, &text)?;
    std::fs::remove_file(&paths[0])?;
    restored.close_buffer(id, CloseDisposition::IfUnmodified)?;
    assert_eq!(
        restored.recently_closed(),
        &[paths[2].clone(), paths[1].clone()]
    );
    Ok(())
}

#[test]
fn session_distinguishes_user_readonly_from_changing_disk_permissions()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::recovery::RecoverySession;
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir()?;
    for user_readonly in [false, true] {
        let path = home.path().join(format!("readonly-{user_readonly}.txt"));
        std::fs::write(&path, "disk")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400))?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let mut workspace = Workspace::new();
        workspace.open_buffer(file, &text)?;
        workspace.active_mut().set_user_read_only(user_readonly);
        let mut session = RecoverySession::create(home.path())?;
        session.checkpoint(&workspace)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let restored = session.restore_workspace()?;
        assert_eq!(
            restored.active().editor.document().is_read_only(),
            user_readonly
        );
    }
    Ok(())
}

#[test]
fn manual_session_restores_saved_paths_not_unsaved_bodies_and_confirmed_export_replaces()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::{DiskFile, OverwritePolicy};
    use notepad_matrix::recovery::SessionFile;
    let home = tempfile::tempdir()?;
    let path = home.path().join("session.json");
    let mut source = Workspace::new();
    let file_path = home.path().join("document.txt");
    std::fs::write(&file_path, "saved text")?;
    let (file, text) = DiskFile::load_unicode(&file_path)?;
    let id = source.open_buffer(file, &text)?;
    source
        .active_mut()
        .editor
        .insert_text("unsaved named body")?;
    source.new_buffer()?;
    source
        .active_mut()
        .editor
        .insert_text("unsaved untitled body")?;
    source.activate(id)?;
    source.clone_to_other_view(id)?;
    source.set_tab_color(ViewId::Secondary, id, Some(3))?;
    SessionFile::capture(&source)?.save(&path, OverwritePolicy::Refuse)?;
    let exported = std::fs::read_to_string(&path)?;
    assert!(!exported.contains("unsaved named body"));
    assert!(!exported.contains("unsaved untitled body"));
    let mut target = Workspace::new();
    target.active_mut().editor.insert_text("existing draft")?;
    let existing = target.active().id();
    let warnings = SessionFile::load(&path)?.restore_into(&mut target)?;
    assert!(warnings.is_empty());
    assert_eq!(target.buffers().len(), 2);
    assert!(target.buffers().iter().any(
        |buffer| buffer.id() == existing && buffer.editor.committed_text() == "existing draft"
    ));
    assert_eq!(target.active().editor.committed_text(), "saved text");
    assert_eq!(target.active_view(), ViewId::Secondary);
    assert_eq!(
        target.tab_color(ViewId::Secondary, target.active().id()),
        Some(3)
    );
    assert!(!target.active().is_modified());
    let loaded = SessionFile::load(&path)?;
    std::fs::write(&path, b"invalid session")?;
    assert!(loaded.save(&path, OverwritePolicy::Confirmed).is_err());
    assert!(
        SessionFile::capture(&source)?
            .save(&path, OverwritePolicy::Refuse)
            .is_err()
    );
    assert_eq!(std::fs::read(&path)?, b"invalid session");
    SessionFile::capture(&source)?.save(&path, OverwritePolicy::Confirmed)?;
    assert!(SessionFile::load(&path).is_ok());
    Ok(())
}

#[test]
fn session_restores_per_view_rectangular_virtual_selection_sets()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::ColumnPosition;
    use notepad_matrix::recovery::RecoverySession;
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("ab\nx\nlast")?;
    workspace.active_mut().editor.select_rectangle(
        ColumnPosition { line: 0, column: 1 },
        ColumnPosition { line: 2, column: 5 },
        4,
    )?;
    let expected = workspace.active().editor.selections();
    assert!(expected.is_rectangular());
    assert_eq!(expected.ranges()[1].head_virtual, 4);
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&workspace)?;
    let restored = session.restore_workspace()?;
    assert_eq!(restored.active().editor.selections(), expected);
    Ok(())
}

#[test]
fn tab_sort_uses_numeric_ascii_comparison_language_type_and_full_path_tiebreaks()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::syntax::Language;
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    let initial = workspace.active().id();
    let mut ids = Vec::new();
    for (name, language) in [
        ("part10.zzz", Language::NormalText),
        ("part2.zzz", Language::Json),
        ("part02.zzz", Language::NormalText),
    ] {
        let path = home.path().join(name);
        std::fs::write(&path, "text")?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        ids.push(workspace.open_buffer(file, &text)?);
        workspace.active_mut().set_language(language);
    }
    workspace.close_buffer(
        initial,
        notepad_matrix::workspace::CloseDisposition::IfUnmodified,
    )?;
    workspace.sort_tabs(TabSort::Name, false);
    assert_eq!(
        workspace.view_buffer_ids(ViewId::Primary),
        &[ids[2], ids[1], ids[0]]
    );
    workspace.sort_tabs(TabSort::Type, false);
    assert_eq!(
        workspace.view_buffer_ids(ViewId::Primary),
        &[ids[1], ids[2], ids[0]]
    );
    workspace.sort_tabs(TabSort::Size, false);
    assert_eq!(
        workspace.view_buffer_ids(ViewId::Primary),
        &[ids[2], ids[1], ids[0]]
    );
    Ok(())
}

#[test]
fn monitoring_session_state_restarts_tail_watch_without_turning_it_into_user_readonly()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::monitoring::Monitoring;
    use notepad_matrix::recovery::RecoverySession;
    let home = tempfile::tempdir()?;
    let path = home.path().join("tail.txt");
    std::fs::write(&path, "log\n")?;
    let mut workspace = Workspace::new();
    let initial = workspace.active().id();
    let (file, text) = DiskFile::load_unicode(&path)?;
    let id = workspace.open_buffer(file, &text)?;
    workspace.close_buffer(
        initial,
        notepad_matrix::workspace::CloseDisposition::IfUnmodified,
    )?;
    let mut monitoring = Monitoring::default();
    monitoring.start(&mut workspace, id)?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&workspace)?;
    let mut restored = session.restore_workspace()?;
    let id = restored.active().id();
    assert!(restored.active().is_monitoring());
    let mut monitoring = Monitoring::default();
    assert!(monitoring.restore(&mut restored).is_empty());
    assert!(monitoring.is_monitoring(id));
    monitoring.stop(&mut restored, id);
    assert!(!restored.active().is_monitoring());
    assert!(!restored.active().editor.document().is_read_only());
    Ok(())
}

#[test]
fn save_backup_modes_copy_previous_disk_bytes_to_source_named_locations()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::recovery::SaveBackupOptions;
    let home = tempfile::tempdir()?;
    let path = home.path().join("file.txt");
    std::fs::write(&path, b"previous\0bytes")?;
    assert!(
        SaveBackupOptions::from_settings(&serde_json::json!({}))?
            .create_backup(&path, "2026-11-10_123456")?
            .is_none()
    );
    let simple = SaveBackupOptions::from_settings(&serde_json::json!({"save_backup":"simple"}))?;
    let backup = simple
        .create_backup(&path, "2026-11-10_123456")?
        .ok_or("backup")?;
    assert_eq!(backup, home.path().join("file.txt.bak"));
    assert_eq!(std::fs::read(&backup)?, b"previous\0bytes");
    std::fs::write(&path, b"new disk")?;
    simple.create_backup(&path, "2026-11-10_123457")?;
    assert_eq!(std::fs::read(&backup)?, b"new disk");
    let verbose = SaveBackupOptions::from_settings(&serde_json::json!({"save_backup":"verbose"}))?;
    let backup = verbose
        .create_backup(&path, "2026-11-10_123456")?
        .ok_or("backup")?;
    assert_eq!(
        backup,
        home.path().join("nppBackup/file.txt.2026-11-10_123456.bak")
    );
    let directory = home.path().join("custom");
    let custom = SaveBackupOptions::from_settings(
        &serde_json::json!({"save_backup":"verbose","backup_use_directory":true,"backup_directory":directory}),
    )?;
    let backup = custom
        .create_backup(&path, "2026-11-10_123457")?
        .ok_or("backup")?;
    assert_eq!(backup, directory.join("file.txt.2026-11-10_123457.bak"));
    assert_eq!(std::fs::read(&path)?, b"new disk");
    assert!(
        SaveBackupOptions::from_settings(&serde_json::json!({"save_backup":"unknown"})).is_err()
    );
    Ok(())
}

#[test]
fn remember_session_without_snapshot_loads_saved_paths_and_requires_guarded_exit()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::recovery::{RecoveryManager, RecoveryOptions};
    let home = tempfile::tempdir()?;
    let path = home.path().join("remember.txt");
    std::fs::write(&path, "saved")?;
    let options = RecoveryOptions::from_settings(&serde_json::json!({"snapshot_mode":false}))?;
    assert!(options.remember_last_session);
    let (mut manager, mut workspace) = RecoveryManager::open_with_options(home.path(), options)?;
    assert!(!manager.snapshot_mode());
    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.insert_text("not a backup")?;
    workspace.new_buffer()?;
    workspace
        .active_mut()
        .editor
        .insert_text("unsaved unnamed")?;
    manager.checkpoint(&workspace)?;
    drop(manager);
    let (_, workspace) = RecoveryManager::open_with_options(home.path(), options)?;
    assert_eq!(workspace.buffers().len(), 1);
    assert_eq!(workspace.active().editor.committed_text(), "saved");
    assert!(!workspace.active().is_modified());
    Ok(())
}

#[test]
fn malformed_session_view_is_rejected_before_changing_existing_workspace()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::config::ConfigStore;
    use notepad_matrix::recovery::RecoverySession;
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("unsaved")?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&workspace)?;
    let path = std::path::PathBuf::from(format!("backup/{}.session.json", session.key()));
    let store = ConfigStore::under_home(home.path());
    let mut manifest = store.load_json(&path)?;
    manifest["documents"][0]["pinned"] = serde_json::json!("bad");
    std::fs::write(store.root().join(&path), serde_json::to_vec(&manifest)?)?;
    assert!(session.restore_workspace().is_err());
    assert_eq!(workspace.active().editor.committed_text(), "unsaved");
    assert_eq!(store.load_json(&path)?, manifest);
    Ok(())
}

#[test]
fn recovery_round_trip_keeps_shared_buffer_views_positions_and_ui()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::editor::{ByteOffset, Selection};
    use notepad_matrix::recovery::RecoverySession;
    use notepad_matrix::workspace::DocumentViewState;
    let home = tempfile::tempdir()?;
    let mut workspace = Workspace::new();
    workspace
        .active_mut()
        .editor
        .insert_text("first\nsecond\nthird\n")?;
    let shared = workspace.active().id();
    workspace.active_mut().editor.toggle_bookmark(Some(1))?;
    workspace.set_pinned(shared, true)?;
    workspace.set_tab_color(ViewId::Primary, shared, Some(2))?;
    workspace.set_view_state(
        ViewId::Primary,
        shared,
        DocumentViewState {
            top_line: 2,
            viewport: Some(notepad_matrix::workspace::ViewportPosition {
                first_visible_line: 0,
                wrap_offset: 2,
                wrap_count: 3,
            }),
            horizontal_offset: 28.,
            folded_lines: vec![0],
            word_wrap: true,
        },
    )?;
    workspace.clone_to_other_view(shared)?;
    workspace.active_mut().editor.select(Selection {
        anchor: ByteOffset(6),
        head: ByteOffset(12),
    })?;
    workspace.set_tab_color(ViewId::Secondary, shared, Some(4))?;
    workspace.set_view_state(
        ViewId::Secondary,
        shared,
        DocumentViewState {
            top_line: 1,
            viewport: None,
            horizontal_offset: 14.,
            folded_lines: vec![],
            word_wrap: false,
        },
    )?;
    let panel_state = serde_json::json!({"version":1,"layout":notepad_matrix::panel_layout::PanelLayout::default().to_json(),"folder":notepad_matrix::folder_workspace::FolderWorkspace::new().snapshot()?,"projects":notepad_matrix::projects::ProjectPanels::new().snapshot()?});
    workspace.set_session_ui(serde_json::json!({"tabs":{"multiline":true,"vertical":false},"panels":panel_state,"split_ratio":0.6}))?;
    let mut session = RecoverySession::create(home.path())?;
    let key = session.key().to_owned();
    session.checkpoint(&workspace)?;
    drop(session);
    let mut claimed = RecoverySession::claim(home.path(), &key)?.ok_or("lease")?;
    let mut restored = claimed.restore_workspace()?;
    assert_eq!(restored.buffers().len(), 1);
    let id = restored.active().id();
    assert_eq!(restored.active_view(), ViewId::Secondary);
    assert_eq!(restored.view_buffer_ids(ViewId::Primary), &[id]);
    assert_eq!(restored.view_buffer_ids(ViewId::Secondary), &[id]);
    assert_eq!(restored.tab_color(ViewId::Primary, id), Some(2));
    assert_eq!(restored.tab_color(ViewId::Secondary, id), Some(4));
    assert!(restored.active().is_pinned());
    assert_eq!(
        restored
            .editor_in_view(ViewId::Primary, id)
            .ok_or("primary")?
            .selection()
            .head,
        ByteOffset(19)
    );
    assert_eq!(
        restored.active().editor.selection(),
        Selection {
            anchor: ByteOffset(6),
            head: ByteOffset(12)
        }
    );
    assert_eq!(
        restored
            .view_state(ViewId::Primary, id)
            .ok_or("position")?
            .folded_lines,
        vec![0]
    );
    assert_eq!(
        restored
            .view_state(ViewId::Secondary, id)
            .ok_or("position")?
            .horizontal_offset,
        14.
    );
    assert_eq!(
        restored
            .active()
            .editor
            .document()
            .bookmarked_lines()
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(restored.session_ui(), workspace.session_ui());
    assert_eq!(
        restored
            .view_state(ViewId::Primary, id)
            .ok_or("position")?
            .viewport,
        Some(notepad_matrix::workspace::ViewportPosition {
            first_visible_line: 0,
            wrap_offset: 2,
            wrap_count: 3,
        })
    );
    restored.active_mut().editor.insert_text("changed")?;
    assert!(
        restored
            .editor_in_view(ViewId::Primary, id)
            .ok_or("shared")?
            .committed_text()
            .contains("changed")
    );
    assert!(
        restored
            .editor_in_view_mut(ViewId::Primary, id)
            .ok_or("undo")?
            .undo()
    );
    assert_eq!(
        restored.active().editor.committed_text(),
        "first\nsecond\nthird\n"
    );
    Ok(())
}

#[test]
fn mru_cycle_freezes_order_until_release_and_reopen_uses_saved_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use notepad_matrix::workspace::CloseDisposition;
    let home = tempfile::tempdir()?;
    let path = home.path().join("closed.txt");
    std::fs::write(&path, "saved")?;
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    let second = workspace.new_buffer()?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let third = workspace.open_buffer(file, &text)?;
    workspace.activate(first)?;
    assert_eq!(workspace.cycle_mru(true)?, third);
    assert_eq!(workspace.cycle_mru(true)?, second);
    assert_eq!(workspace.cycle_mru(false)?, third);
    assert_eq!(workspace.active().id(), first);
    workspace.finish_mru_cycle()?;
    assert_eq!(workspace.cycle_mru(true)?, first);
    workspace.finish_mru_cycle()?;
    workspace.activate(third)?;
    workspace.active_mut().editor.insert_text("discarded")?;
    workspace.close_buffer(third, CloseDisposition::DiscardConfirmed)?;
    assert_eq!(workspace.recently_closed(), std::slice::from_ref(&path));
    let reopened = workspace.reopen_last_closed()?.ok_or("reopen missing")?;
    assert_eq!(workspace.active().id(), reopened);
    assert_eq!(workspace.active().editor.committed_text(), "saved");
    assert!(workspace.recently_closed().is_empty());
    workspace.close_buffer(reopened, CloseDisposition::IfUnmodified)?;
    std::fs::remove_file(&path)?;
    assert!(workspace.reopen_last_closed().is_err());
    assert_eq!(workspace.recently_closed(), &[path]);
    Ok(())
}

#[test]
fn pinned_partition_sort_and_conditional_close_follow_view_membership()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    let second = workspace.new_buffer()?;
    let third = workspace.new_buffer()?;
    workspace.set_pinned(third, true)?;
    assert_eq!(
        workspace.view_buffer_ids(ViewId::Primary),
        &[third, first, second]
    );
    workspace.sort_tabs(TabSort::Name, true);
    assert_eq!(
        workspace.view_buffer_ids(ViewId::Primary),
        &[third, second, first]
    );
    assert!(!workspace.move_tab(third, 2)?);
    workspace.set_tab_color(ViewId::Primary, third, Some(4))?;
    workspace.clone_to_other_view(third)?;
    assert_eq!(workspace.tab_color(ViewId::Primary, third), Some(4));
    assert_eq!(workspace.tab_color(ViewId::Secondary, third), None);
    assert_eq!(
        workspace.close_targets(CloseCondition::ExceptPinned),
        vec![(ViewId::Primary, first), (ViewId::Primary, second)]
    );
    assert_eq!(
        workspace.close_targets(CloseCondition::ExceptCurrent),
        vec![
            (ViewId::Primary, first),
            (ViewId::Primary, second),
            (ViewId::Primary, third)
        ]
    );
    Ok(())
}
