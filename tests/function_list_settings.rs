use notepad_matrix::config::ConfigStore;

#[test]
fn initial_sort_defaults_to_source_false() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let settings = ConfigStore::under_home(home.path()).load_settings()?;
    assert_eq!(settings["function_list_sort_default"], false);
    Ok(())
}

#[test]
fn initial_sort_rejects_non_booleans_without_replacing_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.load_settings()?;
    let path = store.root().join("settings.json");
    let before = std::fs::read(&path)?;
    for malformed in [
        serde_json::Value::Null,
        serde_json::json!("false"),
        serde_json::json!(0),
        serde_json::json!([]),
        serde_json::json!({}),
    ] {
        assert!(
            store
                .update_settings(|settings| {
                    settings["function_list_sort_default"] = malformed;
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(std::fs::read(&path)?, before);
    }
    Ok(())
}

#[test]
fn legacy_absence_and_saved_preference_preserve_unrelated_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let mut legacy = store.load_settings()?;
    legacy
        .as_object_mut()
        .ok_or("settings object")?
        .remove("function_list_sort_default");
    legacy["user_field"] = serde_json::json!({"keep": [1, 2, 3]});
    let path = store.root().join("settings.json");
    let bytes = serde_json::to_vec(&legacy)?;
    std::fs::write(&path, &bytes)?;
    assert!(
        store
            .load_settings()?
            .get("function_list_sort_default")
            .is_none()
    );
    assert_eq!(std::fs::read(&path)?, bytes);
    store.update_settings(|settings| {
        settings["function_list_sort_default"] = true.into();
        Ok(())
    })?;
    let saved = ConfigStore::under_home(home.path()).load_settings()?;
    assert_eq!(saved["function_list_sort_default"], true);
    assert_eq!(saved["user_field"], legacy["user_field"]);
    assert_eq!(saved["features"], legacy["features"]);
    Ok(())
}
