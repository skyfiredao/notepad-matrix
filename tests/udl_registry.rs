#![allow(dead_code, unused_imports)]
#[path = "../src/config.rs"]
mod config;
#[path = "../src/udl.rs"]
mod udl;
#[path = "../src/udl_registry.rs"]
mod udl_registry;
use config::ConfigStore;
use udl_registry::UdlRegistry;

#[test]
fn named_lifecycle_has_stable_ids_and_restarts() {
    let home = tempfile::tempdir().expect("temporary home");
    let store = ConfigStore::under_home(home.path());
    let mut registry = UdlRegistry::load(&store).expect("new registry");
    assert!(registry.definition(0).is_some());
    assert_eq!(
        registry
            .named()
            .map(|(_, d)| d.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Markdown (preinstalled)",
            "Markdown (preinstalled dark mode)"
        ]
    );
    let light = registry
        .match_path(std::path::Path::new("readme.md"))
        .expect("preinstalled Markdown");
    assert_eq!(
        registry.definition(light).expect("Markdown").styles[2].foreground,
        Some(0xff8000)
    );
    let highlighted = udl::lex(
        "# Heading\nplain\n",
        registry.definition(light).expect("Markdown"),
    );
    assert!(
        highlighted
            .spans
            .iter()
            .any(|span| span.range.start == 0 && span.style == udl::UdlStyleId::CommentLine)
    );
    let dark = registry
        .match_path_for_theme(std::path::Path::new("readme.md"), true)
        .expect("dark Markdown");
    assert_ne!(light, dark);
    assert!(
        registry
            .definition(dark)
            .expect("dark Markdown")
            .dark_mode_theme
    );
    assert!(!registry.has_unsaved_changes());
    let id = registry.create("Example").expect("create");
    registry
        .edit(id, |definition| {
            definition.extensions = "ex EX2".into();
            Ok(())
        })
        .expect("extensions");
    registry.rename(id, "Renamed").expect("rename");
    assert_eq!(
        registry.match_path(std::path::Path::new("demo.EX")),
        Some(id)
    );
    assert!(registry.definition_revision(id).expect("revision") > 1);
    registry.save(&store).expect("persist");
    let mut restored = UdlRegistry::load(&store).expect("restart");
    assert_eq!(restored.definition(id).expect("stable id").name, "Renamed");
    let copied = restored.save_as(id, "Copy").expect("save as");
    assert_ne!(id, copied);
    restored.remove(id).expect("remove");
    assert!(restored.definition(id).is_none());
    assert!(restored.definition(copied).is_some());
}

#[test]
fn import_is_atomic_and_roundtrips_all_source_properties() {
    let mut registry = UdlRegistry::default();
    let definition = serde_json::json!({"name":"Source vectors","extensions":"abc", "keywords":["one","two","three","four","five","six","seven","eight"], "comments":{"line_open":"//","block_open":"/*","block_close":"*/","vendor":42}, "numbers":{"prefix1":"0x","range":".."}, "operators":["+ -","and or"], "future":{"keep":true}});
    let ids = registry.import_json(&definition).expect("import");
    let id = ids[0];
    let export = registry.export_json(id).expect("export");
    let mut other = UdlRegistry::default();
    let copied = other.import_json(&export).expect("bundle import")[0];
    assert_eq!(registry.definition(id), other.definition(copied));
    let before = registry.to_json();
    assert!(registry.import_json(&serde_json::json!({"schema_version":1,"definitions":[{"name":"Good"},{"name":"Bad","keywords":[true]}]})).is_err());
    assert_eq!(registry.to_json(), before);
    assert!(registry.rename(id, "").is_err());
    assert_eq!(registry.to_json(), before);
}

#[test]
fn invalid_disk_is_not_overwritten_and_unknowns_survive() {
    let home = tempfile::tempdir().expect("temporary home");
    let store = ConfigStore::under_home(home.path());
    let mut registry = UdlRegistry::default();
    registry.create("Kept").expect("create");
    let mut value = registry.to_json();
    value["future_registry"] = serde_json::json!({"flag": true});
    value["definitions"][1]["future_entry"] = 91.into();
    value["definitions"][1]["definition"]["future_definition"] = "kept".into();
    let mut registry = UdlRegistry::from_json(&value).expect("metadata");
    registry.rename(1, "Changed").expect("rename");
    registry.save(&store).expect("save");
    let restored = UdlRegistry::load(&store).expect("load").to_json();
    assert_eq!(restored["future_registry"]["flag"], true);
    assert_eq!(restored["definitions"][1]["future_entry"], 91);
    assert_eq!(
        restored["definitions"][1]["definition"]["future_definition"],
        "kept"
    );
    let path = store.root().join("udl.json");
    std::fs::write(&path, b"{broken").expect("corrupt fixture");
    assert!(registry.save(&store).is_err());
    assert_eq!(std::fs::read(&path).expect("unchanged disk"), b"{broken");
}

#[test]
fn working_copy_revisions_names_and_extensions_have_unambiguous_semantics() {
    let mut registry = UdlRegistry::default();
    let revision = registry.revision();
    registry
        .edit(0, |d| {
            d.keywords[7] = "last".into();
            Ok(())
        })
        .expect("working keyword 8");
    assert!(registry.revision() > revision);
    let revision = registry.revision();
    registry.edit(0, |_| Ok(())).expect("no-op");
    assert_eq!(registry.revision(), revision);
    assert!(registry.rename(0, "Not named in place").is_err());
    assert!(registry.remove(0).is_err());
    let id = registry.save_as(0, "Language").expect("name working copy");
    assert_eq!(
        registry.definition(id).expect("definition").keywords[7],
        "last"
    );
    assert_eq!(registry.definition(0).expect("working retained").name, "");
    let created = registry
        .create("Created from working")
        .expect("source Create semantics");
    assert_eq!(
        registry
            .definition(created)
            .expect("created definition")
            .keywords[7],
        "last"
    );
    assert!(registry.create("language").is_err());
    registry
        .edit(id, |d| {
            d.extensions = "*.abc .def config.ext".into();
            Ok(())
        })
        .expect("extensions");
    for path in ["file.ABC", "file.def", "my.config.ext"] {
        assert_eq!(registry.match_path(std::path::Path::new(path)), Some(id));
    }
    assert_eq!(registry.match_path(std::path::Path::new("no-match")), None);
    let before = registry.to_json();
    assert!(
        registry
            .edit(id, |d| {
                d.styles[0].id = udl::UdlStyleId::Identifier;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(registry.to_json(), before);
    registry.remove(id).expect("remove language");
    assert!(
        registry.create("Language").expect("recreate") > id,
        "identities must not be reused"
    );
}

#[cfg(unix)]
#[test]
fn udl_storage_is_private_and_rejects_symlink_targets() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let home = tempfile::tempdir().expect("temporary home");
    let store = ConfigStore::under_home(home.path());
    let mut registry = UdlRegistry::default();
    registry.save(&store).expect("save private");
    let path = store.root().join("udl.json");
    assert_eq!(
        std::fs::metadata(&path)
            .expect("file mode")
            .permissions()
            .mode()
            & 0o077,
        0
    );
    assert_eq!(
        std::fs::metadata(store.root())
            .expect("root mode")
            .permissions()
            .mode()
            & 0o077,
        0
    );
    let protected = home.path().join("protected.json");
    std::fs::write(&protected, b"protected").expect("protected fixture");
    std::fs::remove_file(&path).expect("replace own fixture");
    symlink(&protected, &path).expect("symlink fixture");
    assert!(registry.save(&store).is_err());
    assert_eq!(
        std::fs::read(protected).expect("protected unchanged"),
        b"protected"
    );
}

#[test]
fn stale_registry_save_conflicts_without_overwriting_the_first_writer() {
    let home = tempfile::tempdir().expect("temporary home");
    let store = ConfigStore::under_home(home.path());
    let mut a = UdlRegistry::load(&store).expect("first instance");
    let mut b = UdlRegistry::load(&store).expect("second instance");
    a.create("First").expect("first edit");
    b.create("Second").expect("stale edit");
    a.save(&store).expect("first save");
    let bytes = std::fs::read(store.root().join("udl.json")).expect("first bytes");
    assert_eq!(
        b.save(&store).expect_err("stale save must conflict").kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        std::fs::read(store.root().join("udl.json")).expect("disk remains first"),
        bytes
    );
    let mut a = UdlRegistry::load(&store).expect("load existing first");
    let mut b = UdlRegistry::load(&store).expect("load existing second");
    a.rename(1, "First changed").expect("first change");
    a.save(&store).expect("first changed save");
    let bytes = std::fs::read(store.root().join("udl.json")).expect("new bytes");
    b.rename(1, "Stale changed").expect("second change");
    assert!(b.save(&store).is_err());
    assert_eq!(
        std::fs::read(store.root().join("udl.json")).expect("no overwrite"),
        bytes
    );
}

#[test]
fn acknowledging_in_flight_save_preserves_newer_draft_then_commits_both_edits() {
    let home = tempfile::tempdir().expect("temporary home");
    let store = ConfigStore::under_home(home.path());
    let mut draft = UdlRegistry::load(&store).expect("load");
    assert!(!draft.has_unsaved_changes());
    let id = draft.create("Live").expect("create");
    assert!(draft.has_unsaved_changes());
    let mut worker = draft.clone();
    let receipt = worker.save(&store).expect("first queued snapshot");
    draft
        .edit(id, |d| {
            d.keywords[7] = "newer draft".into();
            Ok(())
        })
        .expect("typing during save");
    let revision = draft.revision();
    draft
        .acknowledge_save(&receipt)
        .expect("advance only persistence base");
    assert!(
        draft.has_unsaved_changes(),
        "the acknowledged older snapshot is not the latest draft"
    );
    assert_eq!(draft.revision(), revision);
    assert_eq!(
        draft.definition(id).expect("live definition").keywords[7],
        "newer draft"
    );
    draft.save(&store).expect("next queued snapshot");
    assert!(!draft.has_unsaved_changes());
    assert_eq!(
        UdlRegistry::load(&store)
            .expect("restart")
            .definition(id)
            .expect("both edits persisted")
            .keywords[7],
        "newer draft"
    );
}

#[test]
fn externally_changed_metadata_conflicts_even_without_a_revision_bump() {
    let home = tempfile::tempdir().expect("temporary home");
    let store = ConfigStore::under_home(home.path());
    UdlRegistry::default()
        .save(&store)
        .expect("initial registry");
    let mut stale = UdlRegistry::load(&store).expect("loaded base");
    store
        .update_json(
            std::path::Path::new("udl.json"),
            &serde_json::Value::Null,
            |value| {
                value["external_metadata"] = "preserved".into();
                Ok(())
            },
        )
        .expect("another configuration writer");
    let bytes = std::fs::read(store.root().join("udl.json")).expect("external bytes");
    stale
        .edit(0, |d| {
            d.keywords[0] = "draft retained".into();
            Ok(())
        })
        .expect("stale draft");
    assert_eq!(
        stale
            .save(&store)
            .expect_err("identity CAS conflict")
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        std::fs::read(store.root().join("udl.json")).expect("unchanged disk"),
        bytes
    );
    assert_eq!(
        stale.definition(0).expect("draft retained").keywords[0],
        "draft retained"
    );
}

#[test]
fn simultaneous_writers_have_exactly_one_successful_cas() {
    let home = tempfile::tempdir().expect("temporary home");
    let mut initial = UdlRegistry::default();
    let store = ConfigStore::under_home(home.path());
    initial.save(&store).expect("initial base");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let writers = ["Writer A", "Writer B"].map(|name| {
        let home = home.path().to_path_buf();
        let barrier = barrier.clone();
        let mut registry = initial.clone();
        std::thread::spawn(move || {
            registry.create(name).expect("writer definition");
            barrier.wait();
            registry.save(&ConfigStore::under_home(&home)).map(|_| name)
        })
    });
    let results = writers
        .into_iter()
        .map(|writer| writer.join().expect("writer thread"))
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| matches!(result, Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists)).count(), 1);
    let winner = results
        .into_iter()
        .find_map(Result::ok)
        .expect("one winner");
    let loaded = UdlRegistry::load(&store).expect("valid winning registry");
    assert_eq!(loaded.named().count(), 1);
    assert_eq!(loaded.definition(1).expect("winner identity").name, winner);
}
