use std::path::Path;

use notepad_matrix::config::ConfigStore;
use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::files::{DiskFile, UnicodeFormat, Utf8Format};
use notepad_matrix::recovery::{
    RecoveryManager, RecoveryOptions, RecoverySession, RecoverySnapshot,
};
use notepad_matrix::workspace::Workspace;

#[test]
fn raw_utf16_recovery_uses_the_source_reader_for_odd_tails_and_bad_high_pairs()
-> Result<(), Box<dyn std::error::Error>> {
    for (format, bytes) in [
        (
            UnicodeFormat::Utf16LeBom,
            b"\xff\xfeA\0\0\xd8B\0C\0\0\xd8!".as_slice(),
        ),
        (
            UnicodeFormat::Utf16BeBom,
            b"\xfe\xff\0A\xd8\0\0B\0C\xd8\0!".as_slice(),
        ),
    ] {
        let home = tempfile::tempdir()?;
        let store = ConfigStore::under_home(home.path());
        let mut workspace = Workspace::new();
        workspace.active_mut().editor.insert_text("AC")?;
        workspace.active_mut().select_format(format);
        RecoverySnapshot::capture(workspace.active())?.save(&store, "reader")?;
        let json = store.load_json(Path::new("backup/reader.snapshot.json"))?;
        let backup = Path::new(json["backupFilePath"].as_str().ok_or("backup path")?);
        std::fs::write(backup, bytes)?;
        let mut recovered = Workspace::new();
        RecoverySnapshot::load(&store, "reader")?.restore_into(&mut recovered)?;
        assert_eq!(recovered.active().editor.committed_text(), "AC");
        assert!(recovered.active().is_modified());
        assert!(!recovered.active().editor.document().can_undo());
        assert_eq!(std::fs::read(backup)?, bytes);
    }
    Ok(())
}

#[test]
fn recovered_loaded_dirty_survives_conversion_history_reset_until_formal_save()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut source = Workspace::new();
    source.active_mut().editor.insert_text("recovered 中")?;
    let snapshot = RecoverySnapshot::capture(source.active())?;
    let mut restored = Workspace::new();
    snapshot.restore_into(&mut restored)?;
    restored
        .active_mut()
        .convert_format(UnicodeFormat::Utf16LeBom);
    assert!(!restored.active().editor.is_modified());
    assert!(restored.active().is_modified());
    restored.active_mut().editor.insert_text("x")?;
    assert!(restored.active_mut().editor.undo());
    assert!(!restored.active_mut().editor.undo());
    assert!(restored.active().is_modified());
    let saved = restored.active().save_intent();
    let file = DiskFile::save_as_unicode(
        &home.path().join("restored"),
        saved.snapshot(),
        saved.format(),
        notepad_matrix::files::OverwritePolicy::Refuse,
    )?;
    restored.complete_save(
        &saved,
        file,
        notepad_matrix::workspace::SaveCompletion::Primary,
    )?;
    assert!(!restored.active().is_modified());
    Ok(())
}

#[test]
fn udl_identity_survives_recovery_without_an_installed_definition()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::syntax::Language;
    let home = tempfile::tempdir()?;
    let mut source = Workspace::new();
    source
        .active_mut()
        .editor
        .insert_text("unsaved UDL text <&> 中")?;
    source.active_mut().set_language(Language::UserDefined(73));
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&source)?;
    drop(session);
    let (_, restored) = RecoveryManager::open(home.path())?;
    assert_eq!(restored.buffers().len(), 1);
    assert_eq!(restored.active().language(), Language::UserDefined(73));
    assert_eq!(
        restored.active().editor.committed_text(),
        "unsaved UDL text <&> 中"
    );
    assert!(restored.active().is_modified());
    Ok(())
}

#[test]
fn guarded_window_exit_remembers_open_files_without_resurrecting_discarded_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut source = Workspace::new();
    for (name, body) in [("A.txt", "clean-A"), ("B.txt", "disk-B")] {
        let path = home.path().join(name);
        std::fs::write(&path, body)?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        source.open_buffer(file, &text)?;
    }
    source.active_mut().editor.insert_text("discard-")?;
    let (mut manager, _) = RecoveryManager::open(home.path())?;
    manager.checkpoint(&source)?;
    let discarded = std::collections::BTreeSet::from([source.active().id()]);
    manager.finish_guarded_exit(&source, &discarded)?;
    drop(manager);
    let (_, restored) = RecoveryManager::open(home.path())?;
    assert_eq!(
        restored
            .buffers()
            .iter()
            .map(|buffer| buffer.name())
            .collect::<Vec<_>>(),
        ["A.txt", "B.txt"]
    );
    assert_eq!(
        restored
            .buffers()
            .iter()
            .map(|buffer| buffer.editor.committed_text())
            .collect::<Vec<_>>(),
        ["clean-A", "disk-B"]
    );
    assert!(
        restored
            .buffers()
            .iter()
            .all(|buffer| !buffer.is_modified())
    );
    assert_eq!(restored.active().name(), "B.txt");
    assert!(
        source.active().is_modified(),
        "exit bookkeeping is not a formal save"
    );
    Ok(())
}

#[test]
fn startup_restores_last_open_files_in_order_using_backups_only_for_dirty_buffers()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut source = Workspace::new();
    let mut ids = Vec::new();
    for (name, body) in [
        ("A.txt", "clean-A"),
        ("B.txt", "disk-B"),
        ("C.txt", "clean-C"),
    ] {
        let path = home.path().join(name);
        std::fs::write(&path, body)?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        ids.push(source.open_buffer(file, &text)?);
        if name != "C.txt" {
            source
                .active_mut()
                .set_language(notepad_matrix::syntax::Language::Json);
        }
        if name == "B.txt" {
            source.active_mut().editor.insert_text("unsaved-")?;
        }
    }
    source.activate(ids[0])?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&source)?;
    drop(session);
    std::fs::write(home.path().join("A.txt"), b"current-A")?;
    let (_, restored) = RecoveryManager::open(home.path())?;
    assert_eq!(
        restored
            .buffers()
            .iter()
            .map(|buffer| buffer.name())
            .collect::<Vec<_>>(),
        ["A.txt", "B.txt", "C.txt"]
    );
    assert_eq!(
        restored
            .buffers()
            .iter()
            .map(|buffer| buffer.editor.committed_text())
            .collect::<Vec<_>>(),
        ["current-A", "unsaved-disk-B", "clean-C"]
    );
    assert_eq!(
        restored
            .buffers()
            .iter()
            .map(|buffer| buffer.is_modified())
            .collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(restored.active().name(), "A.txt");
    assert_eq!(
        restored
            .buffers()
            .iter()
            .map(|buffer| buffer.language())
            .collect::<Vec<_>>(),
        [
            notepad_matrix::syntax::Language::Json,
            notepad_matrix::syntax::Language::Json,
            notepad_matrix::syntax::Language::NormalText
        ]
    );
    assert_eq!(std::fs::read(home.path().join("B.txt"))?, b"disk-B");
    Ok(())
}

#[test]
fn recovery_time_preserves_fractional_precision_on_both_sides_of_unix_epoch()
-> Result<(), Box<dyn std::error::Error>> {
    for before_epoch in [false, true] {
        let home = tempfile::tempdir()?;
        let path = home.path().join("precise.txt");
        std::fs::write(&path, b"disk")?;
        let distance = std::time::Duration::new(12, 345600);
        let time = if before_epoch {
            std::time::UNIX_EPOCH - distance
        } else {
            std::time::UNIX_EPOCH + distance
        };
        std::fs::File::open(&path)?.set_modified(time)?;
        assert_eq!(
            std::fs::metadata(&path)?.modified()?,
            time,
            "fixture requires filesystem timestamp precision"
        );
        let (file, text) = DiskFile::load_unicode(&path)?;
        let mut source = Workspace::new();
        source.open_buffer(file, &text)?;
        source.active_mut().editor.insert_text("unsaved")?;
        let store = ConfigStore::under_home(home.path());
        RecoverySnapshot::capture(source.active())?.save(&store, "precise")?;
        let json = store.load_json(Path::new("backup/precise.snapshot.json"))?;
        assert_eq!(
            json["disk"]["original_modified"],
            serde_json::json!({"before_epoch": before_epoch, "seconds": 12, "nanoseconds": 345600})
        );
        let mut recovered = Workspace::new();
        RecoverySnapshot::load(&store, "precise")?.restore_into(&mut recovered)?;
        let mut session = RecoverySession::create(home.path())?;
        session.checkpoint(&recovered)?;
        drop(session);
        let (manager, recovered) = RecoveryManager::open(home.path())?;
        assert!(manager.changed_buffers().is_empty());
        assert_eq!(recovered.active().editor.committed_text(), "unsaveddisk");
    }
    Ok(())
}

#[test]
fn legacy_missing_time_warns_without_inventing_a_new_disk_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("legacy.txt");
    std::fs::write(&path, b"disk")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut source = Workspace::new();
    source.open_buffer(file, &text)?;
    source.active_mut().editor.insert_text("unsaved")?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&source)?;
    let store = ConfigStore::under_home(home.path());
    let index = store.load_json(Path::new(&format!("backup/{}.session.json", session.key())))?;
    let key = index["snapshots"][0]
        .as_str()
        .ok_or("snapshot key")?
        .to_owned();
    let metadata_path = std::path::PathBuf::from(format!("backup/{key}.snapshot.json"));
    let mut legacy = store.load_json(&metadata_path)?;
    std::fs::remove_file(legacy["backupFilePath"].as_str().ok_or("backup path")?)?;
    legacy
        .as_object_mut()
        .ok_or("snapshot object")?
        .remove("backupFilePath");
    legacy["text"] = "unsaveddisk".into();
    legacy["disk"]["bytes"] = serde_json::json!([100, 105, 115, 107]);
    legacy["disk"]
        .as_object_mut()
        .ok_or("disk object")?
        .remove("original_modified");
    let legacy_path = std::path::PathBuf::from(format!("backup/{key}.json"));
    store.update_json(&legacy_path, &legacy, |_| Ok(()))?;
    std::fs::remove_file(store.root().join(metadata_path))?;
    let legacy_bytes = std::fs::read(store.root().join(&legacy_path))?;
    drop(session);
    std::fs::write(&path, b"external")?;
    let (mut manager, recovered) = RecoveryManager::open(home.path())?;
    assert!(manager.changed_buffers().is_empty());
    assert!(
        manager
            .warnings()
            .iter()
            .any(|message| message.contains("no recorded original modification time"))
    );
    manager.checkpoint(&recovered)?;
    assert_eq!(
        std::fs::read(store.root().join(&legacy_path))?,
        legacy_bytes
    );
    assert_eq!(
        store.load_json(Path::new(&format!("backup/{key}.snapshot.json")))?["disk"]["original_modified"],
        serde_json::Value::Null
    );
    let intent = recovered.active().save_intent();
    assert!(
        recovered
            .active()
            .disk_file
            .as_ref()
            .ok_or("baseline")?
            .save_with_format(intent.snapshot(), intent.format())
            .is_err()
    );
    assert_eq!(std::fs::read(path)?, b"external");
    manager.discard_all()?;
    assert_eq!(std::fs::read(store.root().join(legacy_path))?, legacy_bytes);
    Ok(())
}

#[test]
fn malformed_time_is_rejected_without_overwriting_its_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("invalid-time.txt");
    std::fs::write(&path, b"disk")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut source = Workspace::new();
    source.open_buffer(file, &text)?;
    source.active_mut().editor.insert_text("unsaved")?;
    let snapshot = RecoverySnapshot::capture(source.active())?;
    let store = ConfigStore::under_home(home.path());
    snapshot.save(&store, "invalid-time")?;
    store.update_json(
        Path::new("backup/invalid-time.snapshot.json"),
        &serde_json::Value::Null,
        |value| {
            value["disk"]["original_modified"]["nanoseconds"] = 1_000_000_000_u64.into();
            Ok(())
        },
    )?;
    let before = std::fs::read(store.root().join("backup/invalid-time.snapshot.json"))?;
    assert!(RecoverySnapshot::load(&store, "invalid-time").is_err());
    assert!(snapshot.save(&store, "invalid-time").is_err());
    assert_eq!(
        std::fs::read(store.root().join("backup/invalid-time.snapshot.json"))?,
        before
    );
    Ok(())
}

#[test]
fn observed_time_updates_preserve_unknown_fields_inside_timestamp_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("future-time.txt");
    std::fs::write(&path, b"disk")?;
    std::fs::File::open(&path)?
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890))?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut source = Workspace::new();
    source.open_buffer(file, &text)?;
    source.active_mut().editor.insert_text("unsaved")?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&source)?;
    let store = ConfigStore::under_home(home.path());
    let index = store.load_json(Path::new(&format!("backup/{}.session.json", session.key())))?;
    let key = index["snapshots"][0]
        .as_str()
        .ok_or("snapshot reference")?
        .to_owned();
    store.update_json(
        Path::new(&format!("backup/{key}.snapshot.json")),
        &serde_json::Value::Null,
        |value| {
            value["disk"]["original_modified"]["future_clock"] =
                serde_json::json!({"preserve": 42});
            Ok(())
        },
    )?;
    drop(session);
    std::fs::File::open(&path)?
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567900))?;
    let (mut manager, recovered) = RecoveryManager::open(home.path())?;
    manager.checkpoint(&recovered)?;
    let value = store.load_json(Path::new(&format!("backup/{key}.snapshot.json")))?;
    assert_eq!(
        value["disk"]["original_modified"]["seconds"],
        1234567900_u64
    );
    assert_eq!(
        value["disk"]["original_modified"]["future_clock"]["preserve"],
        42
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn restored_system_readonly_follows_live_attributes_in_both_directions()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    for live_readonly in [false, true] {
        let home = tempfile::tempdir()?;
        let path = home.path().join("permissions.txt");
        std::fs::write(&path, b"disk")?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let mut source = Workspace::new();
        source.open_buffer(file, &text)?;
        source.active_mut().editor.insert_text("unsaved")?;
        let store = ConfigStore::under_home(home.path());
        RecoverySnapshot::capture(source.active())?.save(&store, "readonly")?;
        store.update_json(
            Path::new("backup/readonly.snapshot.json"),
            &serde_json::Value::Null,
            |value| {
                value["disk"]["read_only"] = (!live_readonly).into();
                Ok(())
            },
        )?;
        std::fs::set_permissions(
            &path,
            std::fs::Permissions::from_mode(if live_readonly { 0o400 } else { 0o600 }),
        )?;
        let mut recovered = Workspace::new();
        RecoverySnapshot::load(&store, "readonly")?.restore_into(&mut recovered)?;
        assert_eq!(
            recovered.active().editor.document().is_read_only(),
            live_readonly
        );
        assert_eq!(
            recovered
                .active()
                .disk_file
                .as_ref()
                .ok_or("original path")?
                .is_read_only(),
            live_readonly
        );
        assert_eq!(recovered.active().editor.committed_text(), "unsaveddisk");
        assert!(recovered.active().is_modified());
        assert_eq!(std::fs::read(path)?, b"disk");
    }
    Ok(())
}

#[test]
fn recovery_observes_new_timestamp_and_uses_current_source_as_save_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("observed.txt");
    std::fs::write(&path, b"disk")?;
    std::fs::File::open(&path)?
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890))?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.insert_text("unsaved")?;
    let mut session = RecoverySession::create(home.path())?;
    session.checkpoint(&workspace)?;
    drop(session);
    std::fs::write(&path, b"external")?;
    std::fs::File::open(&path)?
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567900))?;
    let (mut manager, recovered) = RecoveryManager::open(home.path())?;
    assert_eq!(manager.changed_buffers(), &[recovered.active().id()]);
    manager.checkpoint(&recovered)?;
    drop(manager);
    let (manager, mut recovered) = RecoveryManager::open(home.path())?;
    assert!(
        manager.changed_buffers().is_empty(),
        "Buffer.checkFileState observes the live timestamp before the No decision; unchanged mtime is not a new event"
    );
    let intent = recovered.active().save_intent();
    assert_eq!(std::fs::read(path)?, b"external");
    assert_eq!(recovered.active().editor.committed_text(), "unsaveddisk");
    let file = recovered
        .active()
        .disk_file
        .as_ref()
        .ok_or("current source")?
        .save_with_format(intent.snapshot(), intent.format())?;
    recovered.complete_save(
        &intent,
        file,
        notepad_matrix::workspace::SaveCompletion::Primary,
    )?;
    assert!(!recovered.active().is_modified());
    Ok(())
}

#[test]
fn recovery_detects_earlier_and_later_original_mtime_even_when_disk_bytes_are_unchanged()
-> Result<(), Box<dyn std::error::Error>> {
    for (seconds, changed) in [(1234567890, false), (1234567880, true), (1234567900, true)] {
        let home = tempfile::tempdir()?;
        let path = home.path().join("mtime.txt");
        std::fs::write(&path, b"disk")?;
        std::fs::File::open(&path)?
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890))?;
        let (file, text) = DiskFile::load_unicode(&path)?;
        let mut workspace = Workspace::new();
        workspace.open_buffer(file, &text)?;
        workspace.active_mut().editor.insert_text("unsaved")?;
        let mut session = RecoverySession::create(home.path())?;
        session.checkpoint(&workspace)?;
        drop(session);
        std::fs::File::open(&path)?
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))?;
        let (manager, recovered) = RecoveryManager::open(home.path())?;
        assert_eq!(!manager.changed_buffers().is_empty(), changed);
        if changed {
            assert_eq!(manager.changed_buffers(), &[recovered.active().id()]);
        }
        assert_eq!(recovered.active().editor.committed_text(), "unsaveddisk");
        assert!(recovered.active().is_modified());
        assert_eq!(std::fs::read(path)?, b"disk");
    }
    Ok(())
}

#[test]
fn one_corrupt_session_does_not_prevent_recovery_of_other_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut broken = RecoverySession::create(home.path())?;
    let mut good = RecoverySession::create(home.path())?;
    let mut source = Workspace::new();
    source.active_mut().editor.insert_text("preserved")?;
    broken.checkpoint(&source)?;
    good.checkpoint(&source)?;
    let path = home.path().join(format!(
        ".notepad-matrix/backup/{}.session.json",
        broken.key()
    ));
    std::fs::write(&path, b"corrupt session")?;
    drop(broken);
    drop(good);
    let (manager, recovered) = RecoveryManager::open(home.path())?;
    assert_eq!(manager.warnings().len(), 1);
    assert_eq!(recovered.buffers().len(), 1);
    assert_eq!(recovered.active().editor.committed_text(), "preserved");
    assert_eq!(std::fs::read(path)?, b"corrupt session");
    Ok(())
}

#[test]
fn source_snapshot_options_default_to_seven_seconds_and_validate_disabled_modes()
-> Result<(), Box<dyn std::error::Error>> {
    let defaults = RecoveryOptions::from_settings(&serde_json::json!({}))?;
    assert!(defaults.enabled);
    assert_eq!(defaults.interval, std::time::Duration::from_secs(7));
    let clamped = RecoveryOptions::from_settings(&serde_json::json!({"snapshot_backup_ms": 0}))?;
    assert_eq!(clamped.interval, std::time::Duration::from_secs(1));
    assert!(!RecoveryOptions::from_settings(&serde_json::json!({"snapshot_mode": false}))?.enabled);
    assert!(
        !RecoveryOptions::from_settings(&serde_json::json!({"remember_last_session": false}))?
            .enabled
    );
    assert!(
        RecoveryOptions::from_settings(
            &serde_json::json!({"snapshot_mode": false,"remember_last_session": "bad"})
        )
        .is_err()
    );
    assert!(
        RecoveryOptions::from_settings(&serde_json::json!({"snapshot_backup_ms": -1})).is_err()
    );
    assert!(
        RecoveryOptions::from_settings(&serde_json::json!({"snapshot_backup_ms": "7000"})).is_err()
    );
    let home = tempfile::tempdir()?;
    let path = home.path().join("configured-large.txt");
    std::fs::write(&path, "x".repeat(1024 * 1024))?;
    let mut workspace = Workspace::new();
    let (file, text) = DiskFile::load_unicode(&path)?;
    workspace.open_buffer(file, &text)?;
    let (mut manager, _) = RecoveryManager::open(home.path())?;
    manager.checkpoint(&workspace)?;
    drop(manager);
    let options = RecoveryOptions::from_settings(
        &serde_json::json!({"large_file_restriction":{"file_size_mb":1}}),
    )?;
    let (_, restored) = RecoveryManager::open_with_options(home.path(), options)?;
    assert!(
        restored.active().is_large_file(),
        "apply the configured threshold before loading recovered documents"
    );
    Ok(())
}

#[test]
fn unchanged_checkpoint_does_not_rewrite_backups_or_acknowledge_dirty_text()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut session = RecoverySession::create(home.path())?;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("unsaved")?;
    session.checkpoint(&workspace)?;
    let directory = home.path().join(".notepad-matrix/backup");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "lock") {
            paths.push(path);
        }
    }
    assert_eq!(paths.len(), 3);
    let fixed = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890);
    for path in &paths {
        std::fs::File::open(path)?.set_modified(fixed)?;
    }
    session.checkpoint(&workspace)?;
    for path in &paths {
        assert_eq!(
            std::fs::metadata(path)?.modified()?,
            fixed,
            "unchanged source modified-for-backup state must not rewrite payload/index"
        );
    }
    assert!(workspace.active().is_modified());
    workspace.active_mut().editor.insert_text("-later")?;
    let raw = paths
        .iter()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains('@'))
        })
        .ok_or("raw backup")?;
    std::fs::remove_file(raw)?;
    session.checkpoint(&workspace)?;
    assert_eq!(std::fs::read(raw)?, b"unsaved-later");
    drop(session);
    let (_, restored) = RecoveryManager::open(home.path())?;
    assert_eq!(restored.active().editor.committed_text(), "unsaved-later");
    Ok(())
}

#[test]
fn manager_recovers_orphan_sessions_without_consuming_a_live_instances_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut first = RecoverySession::create(home.path())?;
    let mut second = RecoverySession::create(home.path())?;
    let mut live = RecoverySession::create(home.path())?;
    let mut source = Workspace::new();
    source.active_mut().editor.insert_text("first")?;
    first.checkpoint(&source)?;
    let mut second_source = Workspace::new();
    second_source.new_buffer()?;
    second_source.active_mut().editor.insert_text("second")?;
    second.checkpoint(&second_source)?;
    let mut live_source = Workspace::new();
    for _ in 0..3 {
        live_source.new_buffer()?;
    }
    live_source.active_mut().editor.insert_text("live")?;
    live.checkpoint(&live_source)?;
    drop(first);
    drop(second);
    let (mut manager, mut restored) = RecoveryManager::open(home.path())?;
    let mut texts = restored
        .buffers()
        .iter()
        .map(|buffer| buffer.editor.committed_text())
        .collect::<Vec<_>>();
    texts.sort();
    assert_eq!(texts, ["first", "second"]);
    restored.new_buffer()?;
    restored.active_mut().editor.insert_text("third")?;
    manager.checkpoint(&restored)?;
    drop(manager);
    let (_, restored) = RecoveryManager::open(home.path())?;
    let mut texts = restored
        .buffers()
        .iter()
        .map(|buffer| buffer.editor.committed_text())
        .collect::<Vec<_>>();
    texts.sort();
    assert_eq!(texts, ["first", "second", "third"]);
    assert_eq!(
        live.restore_workspace()?.active().editor.committed_text(),
        "live"
    );
    Ok(())
}

#[test]
fn independent_sessions_do_not_collide_and_formal_save_removes_only_its_backup()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut first = RecoverySession::create(home.path())?;
    let mut second = RecoverySession::create(home.path())?;
    assert_ne!(first.key(), second.key());
    let mut left = Workspace::new();
    let mut right = Workspace::new();
    right.new_buffer()?;
    left.active_mut().editor.insert_text("left")?;
    right.active_mut().editor.insert_text("right")?;
    first.checkpoint(&left)?;
    second.checkpoint(&right)?;
    let output = home.path().join("formal.txt");
    let intent = left.active().save_intent();
    let file = DiskFile::save_as_unicode(
        &output,
        intent.snapshot(),
        intent.format(),
        notepad_matrix::files::OverwritePolicy::Refuse,
    )?;
    left.complete_save(
        &intent,
        file,
        notepad_matrix::workspace::SaveCompletion::Primary,
    )?;
    first.checkpoint(&left)?;
    let first_key = first.key().to_owned();
    let second_key = second.key().to_owned();
    drop(first);
    drop(second);
    assert_eq!(
        RecoverySession::claim(home.path(), &first_key)?
            .ok_or("first lease")?
            .restore_workspace()?
            .buffers()
            .len(),
        1
    );
    let mut second = RecoverySession::claim(home.path(), &second_key)?.ok_or("second lease")?;
    assert_eq!(
        second.restore_workspace()?.active().editor.committed_text(),
        "right"
    );
    assert_eq!(std::fs::read(output)?, b"left");
    let first_index: serde_json::Value = serde_json::from_slice(&std::fs::read(
        home.path()
            .join(format!(".notepad-matrix/backup/{first_key}.session.json")),
    )?)?;
    assert_eq!(first_index["snapshots"], serde_json::json!([]));
    Ok(())
}

#[test]
fn corrupt_session_reference_is_rejected_without_touching_other_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut first = RecoverySession::create(home.path())?;
    let mut second = RecoverySession::create(home.path())?;
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("preserved")?;
    first.checkpoint(&workspace)?;
    second.checkpoint(&workspace)?;
    let first_path = home.path().join(format!(
        ".notepad-matrix/backup/{}.session.json",
        first.key()
    ));
    let second_path = home.path().join(format!(
        ".notepad-matrix/backup/{}.session.json",
        second.key()
    ));
    let second_bytes = std::fs::read(&second_path)?;
    let second_index: serde_json::Value = serde_json::from_slice(&second_bytes)?;
    let mut first_index: serde_json::Value = serde_json::from_slice(&std::fs::read(&first_path)?)?;
    first_index["snapshots"] = second_index["snapshots"].clone();
    first_index["active"] = second_index["active"].clone();
    let corrupt = serde_json::to_vec(&first_index)?;
    std::fs::write(&first_path, &corrupt)?;
    assert!(first.checkpoint(&workspace).is_err());
    assert!(first.restore_workspace().is_err());
    assert_eq!(std::fs::read(&first_path)?, corrupt);
    assert_eq!(std::fs::read(&second_path)?, second_bytes);
    assert_eq!(
        second.restore_workspace()?.active().editor.committed_text(),
        "preserved"
    );
    Ok(())
}

#[test]
fn snapshot_update_preserves_unknown_fields_inside_the_disk_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let original = home.path().join("future.txt");
    std::fs::write(&original, b"disk")?;
    let (disk, text) = DiskFile::load_unicode(&original)?;
    let mut workspace = Workspace::new();
    workspace.open_buffer(disk, &text)?;
    workspace.active_mut().editor.insert_text("first")?;
    let store = ConfigStore::under_home(home.path());
    RecoverySnapshot::capture(workspace.active())?.save(&store, "future")?;
    store.update_json(
        Path::new("backup/future.snapshot.json"),
        &serde_json::Value::Null,
        |value| {
            value["disk"]["future_identity"] = serde_json::json!({"preserve": 42});
            Ok(())
        },
    )?;
    workspace.active_mut().editor.insert_text("second")?;
    RecoverySnapshot::capture(workspace.active())?.save(&store, "future")?;
    assert_eq!(
        store.load_json(Path::new("backup/future.snapshot.json"))?["disk"]["future_identity"]["preserve"],
        42
    );
    Ok(())
}

#[test]
fn session_lease_prevents_live_restore_and_saved_or_closed_backups_are_retired()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let mut session = RecoverySession::create(home.path())?;
    let key = session.key().to_owned();
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("first")?;
    session.checkpoint(&workspace)?;
    assert!(
        RecoverySession::claim(home.path(), &key)?.is_none(),
        "a live process owns its recovery session"
    );
    let snapshot_files = std::fs::read_dir(home.path().join(".notepad-matrix/backup"))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    assert!(snapshot_files.iter().any(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().contains('@'))
    }));
    drop(session);
    let mut resumed =
        RecoverySession::claim(home.path(), &key)?.ok_or("orphan session not claimable")?;
    let mut recovered = resumed.restore_workspace()?;
    assert_eq!(recovered.buffers().len(), 1);
    assert_eq!(recovered.active().editor.committed_text(), "first");
    assert!(recovered.active().is_modified());
    recovered.close_buffer(
        recovered.active().id(),
        notepad_matrix::workspace::CloseDisposition::DiscardConfirmed,
    )?;
    resumed.checkpoint(&recovered)?;
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(
        home.path()
            .join(format!(".notepad-matrix/backup/{key}.session.json")),
    )?)?;
    assert_eq!(json["snapshots"], serde_json::json!([]));
    for path in snapshot_files.iter().filter(|path| {
        path.file_name().is_some_and(|name| {
            name.to_string_lossy().contains("-buffer-") || name.to_string_lossy().contains('@')
        })
    }) {
        assert!(
            !path.exists(),
            "explicitly discarded buffer backup must be deleted"
        );
    }
    Ok(())
}

#[test]
fn restored_snapshot_uses_current_source_and_preserves_formal_save_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let original = home.path().join("conflict.txt");
    std::fs::write(&original, b"disk")?;
    let (disk, text) = DiskFile::load_unicode(&original)?;
    let mut workspace = Workspace::new();
    workspace.open_buffer(disk, &text)?;
    workspace.active_mut().editor.insert_text("unsaved")?;
    let store = ConfigStore::under_home(home.path());
    RecoverySnapshot::capture(workspace.active())?.save(&store, "conflict")?;
    std::fs::write(&original, b"external")?;
    let mut recovered = Workspace::new();
    RecoverySnapshot::load(&store, "conflict")?.restore_into(&mut recovered)?;
    let buffer = recovered.active();
    let intent = buffer.save_intent();
    assert_eq!(std::fs::read(&original)?, b"external");
    assert!(buffer.is_modified());
    assert_eq!(buffer.editor.committed_text(), "unsaveddisk");
    let file = recovered
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing baseline")?
        .save_with_format(intent.snapshot(), intent.format())?;
    recovered.complete_save(
        &intent,
        file,
        notepad_matrix::workspace::SaveCompletion::Primary,
    )?;
    assert_eq!(std::fs::read(&original)?, b"unsaveddisk");
    assert!(!recovered.active().is_modified());
    Ok(())
}

#[test]
fn unnamed_snapshots_preserve_selected_format_eol_and_unknown_json_fields()
-> Result<(), Box<dyn std::error::Error>> {
    for (format, expected) in [
        (UnicodeFormat::Utf8(Utf8Format::Plain), "utf8"),
        (UnicodeFormat::Utf8(Utf8Format::Bom), "utf8-bom"),
        (UnicodeFormat::Utf16LeBom, "utf16-le-bom"),
        (UnicodeFormat::Utf16BeBom, "utf16-be-bom"),
    ] {
        let home = tempfile::tempdir()?;
        let store = ConfigStore::under_home(home.path());
        let mut workspace = Workspace::new();
        workspace.active_mut().editor.insert_text("中😀\0\n")?;
        workspace
            .active_mut()
            .editor
            .convert_line_endings(notepad_matrix::input::LineEnding::Lf)?;
        workspace.active_mut().select_format(format);
        workspace
            .active_mut()
            .set_language(notepad_matrix::syntax::Language::Json);
        RecoverySnapshot::capture(workspace.active())?.save(&store, "untitled")?;
        store.update_json(
            Path::new("backup/untitled.snapshot.json"),
            &serde_json::Value::Null,
            |value| {
                value["future_extension"] = serde_json::json!({"preserve": 42});
                Ok(())
            },
        )?;
        workspace.active_mut().editor.insert_text("later")?;
        RecoverySnapshot::capture(workspace.active())?.save(&store, "untitled")?;
        let json = store.load_json(Path::new("backup/untitled.snapshot.json"))?;
        assert_eq!(json["future_extension"]["preserve"], 42);
        assert_eq!(json["format"], expected);
        assert!(json.get("text").is_none());
        let raw = std::fs::read(json["backupFilePath"].as_str().ok_or("backup path")?)?;
        match format {
            UnicodeFormat::Utf8(Utf8Format::Plain) => assert_eq!(raw, "中😀\0\nlater".as_bytes()),
            UnicodeFormat::Utf8(Utf8Format::Bom) => {
                assert_eq!(raw, b"\xef\xbb\xbf\xe4\xb8\xad\xf0\x9f\x98\x80\0\nlater")
            }
            UnicodeFormat::Utf16LeBom => assert_eq!(
                raw,
                [
                    0xff, 0xfe, 0x2d, 0x4e, 0x3d, 0xd8, 0, 0xde, 0, 0, 10, 0, 108, 0, 97, 0, 116,
                    0, 101, 0, 114, 0
                ]
            ),
            UnicodeFormat::Utf16BeBom => assert_eq!(
                raw,
                [
                    0xfe, 0xff, 0x4e, 0x2d, 0xd8, 0x3d, 0xde, 0, 0, 0, 0, 10, 0, 108, 0, 97, 0,
                    116, 0, 101, 0, 114
                ]
            ),
        }
        let mut recovered = Workspace::new();
        RecoverySnapshot::load(&store, "untitled")?.restore_into(&mut recovered)?;
        assert_eq!(recovered.active().name(), "new 1");
        assert_eq!(recovered.active().format(), format);
        assert_eq!(
            recovered.active().language(),
            notepad_matrix::syntax::Language::Json
        );
        assert_eq!(recovered.active().editor.line_ending().as_str(), "\n");
        assert_eq!(recovered.active().editor.committed_text(), "中😀\0\nlater");
        assert!(recovered.active().is_modified());
        assert!(recovered.active().disk_file.is_none());
    }
    Ok(())
}

#[test]
fn snapshot_paths_and_failed_writes_cannot_escape_or_destroy_previous_data()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("keep")?;
    let snapshot = RecoverySnapshot::capture(workspace.active())?;
    for key in ["", "../outside", "/absolute", "with/slash", "dot.name"] {
        assert!(snapshot.save(&store, key).is_err());
    }
    snapshot.save(&store, "valid")?;
    let path = store.root().join("backup/valid.snapshot.json");
    let before = std::fs::read(&path)?;
    let mut document: serde_json::Value = serde_json::from_slice(&before)?;
    document["schema_version"] = 999.into();
    let unknown = serde_json::to_vec(&document)?;
    std::fs::write(&path, &unknown)?;
    assert!(RecoverySnapshot::load(&store, "valid").is_err());
    assert!(snapshot.save(&store, "valid").is_err());
    assert_eq!(std::fs::read(&path)?, unknown);
    std::fs::write(&path, b"broken JSON")?;
    assert!(snapshot.save(&store, "valid").is_err());
    assert_eq!(std::fs::read(&path)?, b"broken JSON");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let external = home.path().join("outside.json");
        std::fs::write(&external, &before)?;
        symlink(&external, store.root().join("backup/link.snapshot.json"))?;
        assert!(snapshot.save(&store, "link").is_err());
        assert!(RecoverySnapshot::load(&store, "link").is_err());
        assert_eq!(std::fs::read(external)?, before);
        assert_eq!(
            std::fs::metadata(store.root().join("backup"))?
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        snapshot.save(&store, "private")?;
        assert_eq!(
            std::fs::metadata(store.root().join("backup/private.snapshot.json"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    Ok(())
}

#[test]
fn corrupted_snapshot_is_not_silently_replaced_by_a_new_capture()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("first")?;
    RecoverySnapshot::capture(workspace.active())?.save(&store, "damaged")?;
    store.update_json(
        Path::new("backup/damaged.snapshot.json"),
        &serde_json::Value::Null,
        |value| {
            value["head"] = 99999.into();
            Ok(())
        },
    )?;
    let path = store.root().join("backup/damaged.snapshot.json");
    let before = std::fs::read(&path)?;
    assert!(RecoverySnapshot::load(&store, "damaged").is_err());
    workspace.active_mut().editor.insert_text("second")?;
    assert!(
        RecoverySnapshot::capture(workspace.active())?
            .save(&store, "damaged")
            .is_err()
    );
    assert_eq!(std::fs::read(path)?, before);
    Ok(())
}

#[test]
fn missing_original_recovers_as_unnamed_without_recreating_the_deleted_path()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let original = home.path().join("missing.txt");
    std::fs::write(&original, b"disk")?;
    let (disk, text) = DiskFile::load_unicode(&original)?;
    let mut workspace = Workspace::new();
    workspace.open_buffer(disk, &text)?;
    workspace.active_mut().editor.insert_text("unsaved")?;
    let store = ConfigStore::under_home(home.path());
    RecoverySnapshot::capture(workspace.active())?.save(&store, "missing-original")?;
    std::fs::remove_file(&original)?;
    let mut recovered = Workspace::new();
    RecoverySnapshot::load(&store, "missing-original")?.restore_into(&mut recovered)?;
    assert_eq!(recovered.active().editor.committed_text(), "unsaveddisk");
    assert_eq!(recovered.active().name(), "missing.txt");
    assert!(
        recovered.active().disk_file.is_none(),
        "Buffer.cpp1047–1051 restores a missing original as unnamed"
    );
    assert!(recovered.active().is_modified());
    assert!(!original.exists());
    Ok(())
}

#[test]
fn snapshot_restores_committed_dirty_unicode_without_saving_original_or_preedit()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let original = home.path().join("original.txt");
    let bytes = [0xff, 0xfe, 0x41, 0, 0x0d, 0, 0x0a, 0, 0x5a, 0];
    std::fs::write(&original, bytes)?;
    let (disk, text) = DiskFile::load_unicode(&original)?;
    let mut workspace = Workspace::new();
    workspace.open_buffer(disk, &text)?;
    workspace
        .active_mut()
        .editor
        .move_to_document_edge(true, false)?;
    workspace.active_mut().editor.insert_text("中😀")?;
    workspace.active_mut().editor.select(Selection {
        anchor: ByteOffset(11),
        head: ByteOffset(4),
    })?;
    workspace.active_mut().editor.set_composition("未提交")?;
    let store = ConfigStore::under_home(home.path());
    RecoverySnapshot::capture(workspace.active())?.save(&store, "session-1-buffer-2")?;
    assert_eq!(std::fs::read(&original)?, bytes);
    assert!(workspace.active().is_modified());
    assert!(workspace.active().editor.document().can_undo());
    let json = store.load_json(Path::new("backup/session-1-buffer-2.snapshot.json"))?;
    assert!(json.get("text").is_none());
    assert!(json["disk"].get("bytes").is_none());
    let backup = std::path::PathBuf::from(json["backupFilePath"].as_str().ok_or("backup path")?);
    assert!(
        backup
            .file_name()
            .ok_or("backup name")?
            .to_string_lossy()
            .starts_with("original.txt@")
    );
    assert_eq!(
        std::fs::read(backup)?,
        [
            0xff, 0xfe, 0x41, 0, 0x0d, 0, 0x0a, 0, 0x5a, 0, 0x2d, 0x4e, 0x3d, 0xd8, 0, 0xde
        ]
    );
    let snapshot = RecoverySnapshot::load(&store, "session-1-buffer-2")?;
    let mut recovered = Workspace::new();
    snapshot.restore_into(&mut recovered)?;
    let buffer = recovered.active();
    assert_eq!(buffer.editor.committed_text(), "A\r\nZ中😀");
    assert!(
        buffer.is_modified(),
        "source loadSession marks backup-loaded text dirty"
    );
    assert!(
        !buffer.editor.document().can_undo(),
        "restart does not invent an undo history"
    );
    assert!(buffer.editor.marked_range().is_none());
    assert_eq!(buffer.format(), UnicodeFormat::Utf16LeBom);
    assert_eq!(
        buffer.editor.document().selection(),
        Selection {
            anchor: ByteOffset(11),
            head: ByteOffset(4)
        }
    );
    assert_eq!(buffer.editor.line_ending().as_str(), "\r\n");
    assert_eq!(
        buffer.disk_file.as_ref().map(DiskFile::path),
        Some(std::fs::canonicalize(&original)?.as_path())
    );
    assert_eq!(std::fs::read(&original)?, bytes);
    Ok(())
}
