use notepad_matrix::{
    files::{DiskFile, OverwritePolicy, Utf8Format},
    input::EditorSession,
};
pub use notepad_matrix::{syntax, udl_registry, workspace};
#[path = "../src/udl_association.rs"]
mod association;

fn open(workspace: &mut workspace::Workspace, path: &std::path::Path) -> workspace::BufferId {
    let snapshot = EditorSession::new("unchanged text").save_snapshot();
    let file = DiskFile::save_as_utf8(path, &snapshot, Utf8Format::Plain, OverwritePolicy::Refuse)
        .expect("source file");
    workspace
        .open_buffer(file, "unchanged text")
        .expect("open buffer")
}

#[test]
fn udl_extensions_precede_builtin_detection_but_never_override_explicit_choices() {
    let home = tempfile::tempdir().expect("temporary home");
    let mut workspace = workspace::Workspace::new();
    open(&mut workspace, &home.path().join("example.json"));
    let mut registry = udl_registry::UdlRegistry::default();
    let id = registry.create("Override JSON").expect("definition");
    registry
        .edit(id, |d| {
            d.extensions = "json".into();
            Ok(())
        })
        .expect("extensions");
    assert!(
        association::refresh(&registry, std::iter::once(workspace.active_mut()), false).changed
    );
    assert_eq!(
        workspace.active().language(),
        syntax::Language::UserDefined(id)
    );
    workspace
        .active_mut()
        .set_language(syntax::Language::NormalText);
    assert!(
        !association::refresh(&registry, std::iter::once(workspace.active_mut()), false).changed
    );
    assert_eq!(workspace.active().language(), syntax::Language::NormalText);
    workspace.active_mut().set_language(syntax::Language::Json);
    association::refresh(&registry, std::iter::once(workspace.active_mut()), false);
    assert_eq!(workspace.active().language(), syntax::Language::Json);
    assert_eq!(workspace.active().editor.committed_text(), "unchanged text");
}

#[test]
fn rename_retains_identity_remove_retargets_working_copy_and_missing_recovery_stays_explicit() {
    let mut workspace = workspace::Workspace::new();
    let mut registry = udl_registry::UdlRegistry::default();
    let id = registry.create("Before").expect("definition");
    workspace
        .active_mut()
        .set_language(syntax::Language::UserDefined(id));
    registry.rename(id, "After").expect("rename");
    association::refresh(&registry, std::iter::once(workspace.active_mut()), false);
    assert_eq!(
        workspace.active().language(),
        syntax::Language::UserDefined(id)
    );
    registry.remove(id).expect("remove");
    association::removed(std::iter::once(workspace.active_mut()), id);
    assert_eq!(
        workspace.active().language(),
        syntax::Language::UserDefined(0)
    );
    workspace
        .active_mut()
        .set_language(syntax::Language::UserDefined(999));
    let result = association::refresh(&registry, std::iter::once(workspace.active_mut()), false);
    assert_eq!(result.missing, vec![(workspace.active().id(), 999)]);
    assert_eq!(
        workspace.active().language(),
        syntax::Language::UserDefined(999)
    );
}
