use notepad_matrix::config::ConfigStore;

#[test]
fn persisted_save_all_confirmation_rejects_malformed_values_and_accepts_legacy_absence()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let mut settings = store.load_settings()?;
    settings
        .as_object_mut()
        .ok_or("settings must be an object")?
        .remove("save_all_confirm");
    let path = store.root().join("settings.json");
    let legacy = serde_json::to_vec(&settings)?;
    std::fs::write(&path, &legacy)?;
    assert!(store.load_settings()?.get("save_all_confirm").is_none());
    assert_eq!(std::fs::read(&path)?, legacy);
    for malformed in [
        serde_json::Value::Null,
        serde_json::json!("false"),
        serde_json::json!(0),
        serde_json::json!([]),
        serde_json::json!({}),
    ] {
        settings["save_all_confirm"] = malformed;
        let bytes = serde_json::to_vec(&settings)?;
        std::fs::write(&path, &bytes)?;
        assert!(store.load_settings().is_err());
        assert_eq!(std::fs::read(&path)?, bytes);
    }
    Ok(())
}

#[test]
fn save_all_confirmation_accepts_boolean_and_rejects_other_types_without_writing()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    store.update_settings(|settings| {
        settings["save_all_confirm"] = false.into();
        Ok(())
    })?;
    assert_eq!(store.load_settings()?["save_all_confirm"], false);
    let path = store.root().join("settings.json");
    let before = std::fs::read(&path)?;
    assert!(
        store
            .update_settings(|settings| {
                settings["save_all_confirm"] = "false".into();
                Ok(())
            })
            .is_err()
    );
    assert_eq!(std::fs::read(path)?, before);
    Ok(())
}

#[test]
fn first_load_creates_versioned_settings_under_home() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let settings = store.load_settings()?;
    assert_eq!(store.root(), home.path().join(".notepad-matrix"));
    assert_eq!(settings["schema_version"], 1);
    assert_eq!(settings["theme"], "light");
    assert_eq!(settings["save_all_confirm"], true);
    assert_eq!(settings["features"]["html_export"], true);
    assert_eq!(settings["features"]["mime_tools"], true);
    assert_eq!(settings["features"]["converter"], true);
    assert!(store.root().join("settings.json").is_file());
    Ok(())
}

#[test]
fn unknown_schema_is_rejected_without_overwriting_config() -> Result<(), Box<dyn std::error::Error>>
{
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    std::fs::create_dir_all(store.root())?;
    let path = store.root().join("settings.json");
    let original = br#"{"schema_version":2,"theme":"light","user_field":"preserve"}"#;
    std::fs::write(&path, original)?;
    let result = store.load_settings();
    assert!(result.is_err());
    assert_eq!(std::fs::read(path)?, original);
    Ok(())
}

#[test]
fn invalid_theme_is_rejected_without_replacing_the_file() -> Result<(), Box<dyn std::error::Error>>
{
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    std::fs::create_dir_all(store.root())?;
    let path = store.root().join("settings.json");
    let original = br#"{"schema_version":1,"theme":42,"features":{"html_export":true,"mime_tools":true,"converter":true}}"#;
    std::fs::write(&path, original)?;
    assert!(store.load_settings().is_err());
    assert_eq!(std::fs::read(path)?, original);
    Ok(())
}

#[test]
fn module_switches_must_be_booleans() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    std::fs::create_dir_all(store.root())?;
    let original = br#"{"schema_version":1,"theme":"light","features":{"html_export":true,"mime_tools":true,"converter":"false"}}"#;
    std::fs::write(store.root().join("settings.json"), original)?;
    assert!(store.load_settings().is_err());
    Ok(())
}

#[test]
fn updates_preserve_unknown_fields_and_reload_latest_values()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    let original = br#"{"schema_version":1,"theme":"light","features":{"html_export":true,"mime_tools":true,"converter":true},"user_field":{"value":9}}"#;
    std::fs::write(store.root().join("settings.json"), original)?;
    store.update_settings(|settings| {
        settings["theme"] = "dark".into();
        Ok(())
    })?;
    let saved = store.load_settings()?;
    assert_eq!(saved["theme"], "dark");
    assert_eq!(saved["user_field"]["value"], 9);
    Ok(())
}

#[test]
fn rejected_update_leaves_original_bytes_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    let path = store.root().join("settings.json");
    let original = std::fs::read(&path)?;
    let result = store.update_settings(|settings| {
        settings["theme"] = "unknown".into();
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(path)?, original);
    Ok(())
}

#[test]
fn cancelled_update_does_not_persist_partial_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    let path = store.root().join("settings.json");
    let original = std::fs::read(&path)?;
    let result = store.update_settings(|settings| {
        settings["theme"] = "dark".into();
        Err(std::io::Error::other("cancelled"))
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(path)?, original);
    Ok(())
}

#[test]
fn concurrent_stores_do_not_lose_updates() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.update_settings(|settings| {
        settings["counter"] = 0.into();
        Ok(())
    })?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let mut workers = Vec::new();
    for _ in 0..4 {
        let home = home.path().to_path_buf();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || -> std::io::Result<()> {
            let store = ConfigStore::under_home(&home);
            barrier.wait();
            for _ in 0..16 {
                store.update_settings(|settings| {
                    let count = settings["counter"].as_u64().ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "counter missing")
                    })?;
                    settings["counter"] = (count + 1).into();
                    Ok(())
                })?;
            }
            Ok(())
        }));
    }
    for worker in workers {
        worker
            .join()
            .map_err(|_| std::io::Error::other("worker panicked"))??;
    }
    assert_eq!(store.load_settings()?["counter"], 64);
    Ok(())
}

#[test]
fn malformed_json_is_not_replaced_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    std::fs::create_dir_all(store.root())?;
    let path = store.root().join("settings.json");
    let original = b"{ broken";
    std::fs::write(&path, original)?;
    assert!(store.load_settings().is_err());
    assert_eq!(std::fs::read(path)?, original);
    Ok(())
}

#[cfg(unix)]
#[test]
fn new_configuration_is_private() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    assert_eq!(
        std::fs::metadata(store.root())?.permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(store.root().join("settings.json"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn configuration_root_cannot_redirect_through_a_symlink() -> Result<(), Box<dyn std::error::Error>>
{
    let home = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let original = br#"{"schema_version":1,"theme":"light","features":{"html_export":true,"mime_tools":true,"converter":true}}"#;
    std::fs::write(outside.path().join("settings.json"), original)?;
    let store = ConfigStore::under_home(home.path());
    std::os::unix::fs::symlink(outside.path(), store.root())?;
    assert!(store.load_settings().is_err());
    assert_eq!(
        std::fs::read(outside.path().join("settings.json"))?,
        original
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn settings_file_cannot_redirect_outside_configuration_root()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let original = br#"{"schema_version":1,"theme":"light","features":{"html_export":true,"mime_tools":true,"converter":true}}"#;
    let destination = outside.path().join("settings.json");
    std::fs::write(&destination, original)?;
    let store = ConfigStore::under_home(home.path());
    std::fs::create_dir_all(store.root())?;
    std::os::unix::fs::symlink(&destination, store.root().join("settings.json"))?;
    assert!(store.load_settings().is_err());
    assert_eq!(std::fs::read(destination)?, original);
    Ok(())
}

#[cfg(unix)]
#[test]
fn failed_write_preserves_last_valid_settings() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.update_settings(|_| Ok(()))?;
    let path = store.root().join("settings.json");
    let original = std::fs::read(&path)?;
    std::fs::set_permissions(store.root(), std::fs::Permissions::from_mode(0o500))?;
    let result = store.update_settings(|settings| {
        settings["theme"] = "dark".into();
        Ok(())
    });
    std::fs::set_permissions(store.root(), std::fs::Permissions::from_mode(0o700))?;
    assert!(result.is_err());
    assert_eq!(std::fs::read(path)?, original);
    Ok(())
}

#[test]
fn all_document_types_share_versioned_json_storage_under_home()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    for name in [
        "session.json",
        "macros.json",
        "shortcuts.json",
        "languages/example.json",
        "themes/example.json",
        "context-menu.json",
        "run-commands.json",
        "projects/example.json",
        "features/converter.json",
    ] {
        let relative = std::path::Path::new(name);
        let defaults = serde_json::json!({"schema_version":1,"fixture_payload":[]});
        store.update_json(relative, &defaults, |document| {
            document["fixture_payload"] = serde_json::json!(["preserved"]);
            Ok(())
        })?;
        let loaded = store.load_json(relative)?;
        assert_eq!(loaded["schema_version"], 1);
        assert_eq!(loaded["fixture_payload"], serde_json::json!(["preserved"]));
        assert!(store.root().join(relative).is_file());
    }
    Ok(())
}

#[test]
fn generic_json_paths_cannot_escape_configuration_root() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let defaults = serde_json::json!({"schema_version":1});
    for name in [
        "../outside.json",
        "/tmp/outside.json",
        "languages/../../outside.json",
        "file.xml",
    ] {
        assert!(
            store
                .update_json(std::path::Path::new(name), &defaults, |_| Ok(()))
                .is_err()
        );
    }
    assert!(!home.path().join("outside.json").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn nested_json_directories_cannot_redirect_through_symlinks()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    std::os::unix::fs::symlink(outside.path(), store.root().join("languages"))?;
    let defaults = serde_json::json!({"schema_version":1});
    assert!(
        store
            .update_json(
                std::path::Path::new("languages/example.json"),
                &defaults,
                |_| Ok(())
            )
            .is_err()
    );
    assert!(!outside.path().join("example.json").exists());
    Ok(())
}
