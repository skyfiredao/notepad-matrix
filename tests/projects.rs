#[path = "../src/projects.rs"]
mod projects;

pub use notepad_matrix::{files, folder_workspace, workspace};
use projects::{NodeKind, ProjectPanels};

#[test]
fn find_in_projects_searches_only_entries_and_uses_unsaved_open_text()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("document.txt");
    std::fs::write(&path, "disk text")?;
    std::fs::write(fixture.path().join("not-in-project.txt"), "needle")?;
    let mut panels = ProjectPanels::new();
    let panel = panels.panel_mut(0)?;
    let project = panel.add_project("Search")?;
    let file = panel.add_file(project, &path)?;
    panel.add_file(project, &fixture.path().join("missing.txt"))?;
    let mut workspace = workspace::Workspace::new();
    panel.open_file(file, &mut workspace)?;
    workspace
        .active_mut()
        .editor
        .insert_text("needle\nneedle\n")?;
    let result = panel.find_text("needle", &workspace)?;
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.matches[0].node, file);
    assert_eq!(result.matches[0].line, 1);
    assert_eq!(result.matches[1].line, 2);
    assert_eq!(result.matches[0].range, 0..6);
    assert_eq!(result.matches[0].path, path);
    assert_eq!(result.matches[0].preview, "needle");
    assert_eq!(
        result.matches[0].searched_text.as_ref(),
        "needle\nneedle\ndisk text"
    );
    assert_eq!(result.errors.len(), 1);
    assert!(panel.find_text("", &workspace).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_working_directory_returns_snapshot_error_without_panicking()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStringExt;
    const MARKER: &str = "NOTEPAD_MATRIX_PROJECT_NON_UTF8_TEST";
    if std::env::var_os(MARKER).is_some() {
        assert!(ProjectPanels::new().snapshot().is_err());
        return Ok(());
    }
    let fixture = tempfile::tempdir()?;
    let directory = fixture
        .path()
        .join(std::ffi::OsString::from_vec(vec![0xff]));
    std::fs::create_dir(&directory)?;
    let status = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "non_utf8_working_directory_returns_snapshot_error_without_panicking",
        ])
        .env(MARKER, "1")
        .current_dir(directory)
        .status()?;
    assert!(status.success());
    Ok(())
}

#[test]
fn relative_edits_use_workspace_directory_and_failed_save_keeps_configuration()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let base = fixture.path();
    let mut panels = ProjectPanels::new();
    let panel = panels.panel_mut(0)?;
    let project = panel.add_project("Paths")?;
    let workspace_path = base.join("workspace.json");
    panel.save_as(&workspace_path)?;
    let file = panel.add_file(project, std::path::Path::new("src/../missing.txt"))?;
    assert_eq!(
        panel.node(file).and_then(|node| node.file_path()),
        Some(base.join("missing.txt").as_path())
    );
    panel.modify_file_path(file, std::path::Path::new("renamed.txt"))?;
    assert_eq!(panel.files(), vec![base.join("renamed.txt")]);
    let before = panel.clone();
    assert!(
        panel
            .save_as(&base.join("missing-directory/workspace.json"))
            .is_err()
    );
    assert_eq!(panel, &before);
    assert!(panel.clear(false).is_err());
    assert!(
        panel
            .add_folder(project, &base.join("not-a-directory"))
            .is_err()
    );
    assert_eq!(panel, &before);
    assert!(panel.rename(file, "../escape").is_err());
    assert_eq!(panel, &before);
    Ok(())
}

#[test]
fn source_drag_rules_reject_projects_leaves_parent_and_descendants_without_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut panels = ProjectPanels::new();
    let panel = panels.panel_mut(0)?;
    let first = panel.add_project("First")?;
    let second = panel.add_project("Second")?;
    let group = panel.add_group(first, "Group")?;
    let child = panel.add_group(group, "Child")?;
    let file = panel.add_file(group, std::path::Path::new("missing.txt"))?;
    let before = panel.clone();
    for (source, target) in [
        (first, second),
        (group, child),
        (group, group),
        (file, group),
        (group, file),
    ] {
        assert!(panel.drop_entry(source, target).is_err());
        assert_eq!(panel, &before);
    }
    panel.drop_entry(group, second)?;
    assert_eq!(panel.parent(group), Some(second));
    assert_eq!(panel.parent(child), Some(group));
    panel.toggle_expanded(group)?;
    assert!(!panel.node(group).expect("group").expanded);
    assert!(panel.toggle_expanded(file).is_err());
    Ok(())
}

#[test]
fn project_activation_reuses_dirty_document_even_when_disk_file_disappears()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("document.txt");
    std::fs::write(&path, "original")?;
    let mut panels = ProjectPanels::new();
    let panel = panels.panel_mut(0)?;
    let project = panel.add_project("Documents")?;
    let file = panel.add_file(project, &path)?;
    let mut workspace = workspace::Workspace::new();
    let id = panel.open_file(file, &mut workspace)?;
    workspace.active_mut().editor.insert_text("unsaved ")?;
    let text = workspace.active().editor.committed_text();
    let selection = workspace.active().editor.document().selection();
    workspace.new_buffer()?;
    let count = workspace.buffers().len();
    std::fs::remove_file(&path)?;
    assert_eq!(panel.open_file(file, &mut workspace)?, id);
    assert_eq!(workspace.buffers().len(), count);
    assert_eq!(workspace.active().editor.committed_text(), text);
    assert_eq!(workspace.active().editor.document().selection(), selection);
    let unknown = panel.add_file(project, &fixture.path().join("never-existed.txt"))?;
    assert!(panel.open_file(unknown, &mut workspace).is_err());
    assert_eq!(workspace.active().id(), id);
    Ok(())
}

#[test]
fn moves_reorder_without_cycles_and_folder_import_is_a_static_virtual_tree()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    std::fs::create_dir(fixture.path().join("sources"))?;
    std::fs::write(fixture.path().join("sources/main.rs"), "main")?;
    std::fs::write(fixture.path().join("root.txt"), "root")?;
    let mut panels = ProjectPanels::new();
    let panel = panels.panel_mut(0)?;
    let first = panel.add_project("First")?;
    let second = panel.add_project("Second")?;
    let group = panel.add_group(first, "Virtual")?;
    let imported = panel.add_folder(group, fixture.path())?;
    assert_eq!(
        panel.node(imported).map(|node| node.children.len()),
        Some(2)
    );
    let before = panel.clone();
    assert!(panel.move_entry(first, Some(group), 0).is_err());
    assert!(panel.move_entry(group, Some(imported), 0).is_err());
    assert!(panel.move_entry(group, None, 0).is_err());
    assert_eq!(panel, &before);
    panel.move_entry(group, Some(second), 0)?;
    assert!(
        panel
            .node(first)
            .is_some_and(|node| node.children.is_empty())
    );
    assert_eq!(panel.parent(group), Some(second));
    panel.move_entry(second, None, 0)?;
    assert_eq!(panel.roots()[0].id, second);
    assert!(panel.move_sibling(second, false)?);
    assert_eq!(panel.roots()[0].id, first);
    assert!(!panel.move_sibling(first, true)?);
    std::fs::write(fixture.path().join("new.txt"), "not watched")?;
    assert_eq!(
        panel.node(imported).map(|node| node.children.len()),
        Some(2)
    );
    let saved = fixture.path().join("tree.json");
    panel.save_as(&saved)?;
    panel.rename(first, "Changed")?;
    panel.save()?;
    panel.reload(false)?;
    assert_eq!(
        panel.node(first).map(|node| node.name.as_str()),
        Some("Changed")
    );
    panel.clear(false)?;
    assert!(panel.roots().is_empty());
    assert!(fixture.path().join("sources/main.rs").exists());
    Ok(())
}

#[test]
fn private_json_roundtrip_rebases_relative_files_and_rejects_bad_load_transactionally()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let directory = fixture.path();
    std::fs::create_dir(directory.join("elsewhere"))?;
    let path = directory.join("workspace.projects.json");
    let mut panels = ProjectPanels::new();
    let panel = panels.panel_mut(0)?;
    let project = panel.add_project("Application")?;
    let group = panel.add_group(project, "Sources")?;
    let file = panel.add_file(group, &directory.join("missing.rs"))?;
    panel.save_as(&path)?;
    assert!(!panel.dirty());
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    assert_eq!(json["format"], "notepad-matrix-projects");
    assert_eq!(json["version"], 1);
    assert_eq!(
        json["projects"][0]["children"][0]["children"][0]["path"],
        "missing.rs"
    );
    panel.export(&directory.join("elsewhere/copy.json"))?;
    assert_eq!(panel.path(), Some(path.as_path()));
    assert!(!panel.dirty());
    panel.rename(project, "Unsaved application")?;
    let before = panel.clone();
    assert!(panel.load(&path, false).is_err());
    assert_eq!(panel, &before);
    let invalid = directory.join("invalid.json");
    for data in [
        "{",
        r#"{"format":"notepad-matrix-projects","version":99,"projects":[]}"#,
        r#"{"format":"notepad-matrix-projects","version":1,"projects":[{"kind":"file","path":"x"}]}"#,
    ] {
        std::fs::write(&invalid, data)?;
        assert!(panel.load(&invalid, true).is_err());
        assert_eq!(panel, &before);
    }
    panel.load(&path, true)?;
    assert!(panel.node(file).is_some_and(|node| node.is_missing()));
    assert_eq!(
        panel.node(project).map(|node| node.name.as_str()),
        Some("Application")
    );
    panels
        .panel_mut(1)?
        .add_project("Not saved to an external file")?;
    panels.panel_mut(2)?.visible = true;
    let saved = panels.snapshot()?;
    let restored = ProjectPanels::restore(&saved)?;
    assert_eq!(restored, panels);
    assert!(ProjectPanels::restore(&serde_json::json!({"version": 2})).is_err());
    Ok(())
}

#[test]
fn three_independent_panels_create_rename_remove_without_disk_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let file = fixture.path().join("keep.txt");
    std::fs::write(&file, "keep")?;
    let mut panels = ProjectPanels::new();
    for index in 0..3 {
        let panel = panels.panel_mut(index)?;
        let project = panel.add_project(&format!("Project {index}"))?;
        let group = panel.add_group(project, "Sources")?;
        let entry = panel.add_file(group, &file)?;
        panel.rename(entry, "renamed.txt")?;
        assert_eq!(
            panel.node(entry).and_then(|node| node.file_path()),
            Some(fixture.path().join("renamed.txt").as_path())
        );
        assert!(panel.node(entry).is_some_and(|node| node.is_missing()));
        assert_eq!(
            panel.node(project).map(|node| &node.kind),
            Some(&NodeKind::Project)
        );
        panel.remove(group)?;
        assert!(panel.node(entry).is_none());
        assert_eq!(std::fs::read_to_string(&file)?, "keep");
    }
    assert_eq!(panels.panel(0)?.roots()[0].name, "Project 0");
    assert_eq!(panels.panel(1)?.roots()[0].name, "Project 1");
    assert_eq!(panels.panel(2)?.roots()[0].name, "Project 2");
    assert!(panels.panel_mut(3).is_err());
    Ok(())
}
