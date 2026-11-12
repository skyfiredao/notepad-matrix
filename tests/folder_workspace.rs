use std::fs;

use notepad_matrix::folder_workspace::{EntryKind, FolderWorkspace};

#[test]
fn rename_rebinds_dirty_open_documents_without_replacing_editor_or_undo() {
    use notepad_matrix::folder_workspace::{FolderFile, FolderMutation, relocate_open_paths};
    use notepad_matrix::workspace::Workspace;
    let fixture = tempfile::tempdir().expect("fixture");
    let path = fixture.path().join("before.txt");
    fs::write(&path, "original").expect("file");
    let mut workspace = Workspace::new();
    FolderFile::load(&path)
        .expect("load")
        .apply(&mut workspace)
        .expect("open");
    workspace
        .active_mut()
        .editor
        .insert_text("unsaved ")
        .expect("edit");
    let selection = workspace.active().editor.selection();
    let revision = workspace.active().editor.display_revision();
    let renamed = FolderMutation::Rename {
        path: path.clone(),
        name: "after.txt".into(),
    }
    .execute()
    .expect("rename");
    assert_eq!(relocate_open_paths(&mut workspace, &path, &renamed), 1);
    assert_eq!(
        workspace
            .active()
            .disk_file
            .as_ref()
            .expect("disk file")
            .path(),
        renamed
    );
    assert_eq!(
        workspace.active().editor.committed_text(),
        "unsaved original"
    );
    assert_eq!(workspace.active().editor.selection(), selection);
    assert_eq!(workspace.active().editor.display_revision(), revision);
    assert!(workspace.active().is_modified());
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.committed_text(), "original");
}

#[test]
fn multiple_roots_and_expansions_restore_without_touching_files() {
    let first = tempfile::tempdir().expect("first");
    let second = tempfile::tempdir().expect("second");
    fs::create_dir(first.path().join("child")).expect("child");
    fs::write(first.path().join("child/a.txt"), "first").expect("first file");
    fs::write(second.path().join("b.txt"), "second").expect("second file");
    let mut tree = FolderWorkspace::new();
    tree.set_root(first.path()).expect("first root");
    tree.add_root(second.path()).expect("second root");
    tree.toggle_directory(
        &first
            .path()
            .canonicalize()
            .expect("canonical")
            .join("child"),
    )
    .expect("expand");
    tree.set_follow_links(true);
    let mut restored =
        FolderWorkspace::restore(&tree.snapshot().expect("snapshot")).expect("restore");
    restored.refresh();
    assert_eq!(restored.roots().count(), 2);
    assert!(restored.follow_links());
    assert!(
        restored
            .entries()
            .iter()
            .any(|entry| entry.path.ends_with("a.txt"))
    );
    assert!(
        restored
            .entries()
            .iter()
            .any(|entry| entry.path.ends_with("b.txt"))
    );
    restored.remove_root(first.path()).expect("remove first");
    assert_eq!(restored.roots().count(), 1);
    assert_eq!(
        fs::read_to_string(first.path().join("child/a.txt")).expect("not deleted"),
        "first"
    );
    restored.remove_all_roots();
    assert!(restored.entries().is_empty());
    assert_eq!(
        fs::read_to_string(second.path().join("b.txt")).expect("not deleted"),
        "second"
    );
}

#[test]
fn folder_mutations_use_real_files_and_refuse_overwrite_or_invalid_names() {
    use notepad_matrix::folder_workspace::FolderMutation;
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path();
    let directory = FolderMutation::CreateDirectory {
        parent: root.into(),
        name: "created".into(),
    }
    .execute()
    .expect("create directory");
    let file = FolderMutation::CreateFile {
        parent: directory.clone(),
        name: "new.txt".into(),
    }
    .execute()
    .expect("create file");
    fs::write(&file, "keep me").expect("fixture contents");
    let renamed = FolderMutation::Rename {
        path: file,
        name: "renamed.txt".into(),
    }
    .execute()
    .expect("rename");
    assert_eq!(
        fs::read_to_string(&renamed).expect("renamed contents"),
        "keep me"
    );
    assert!(
        FolderMutation::CreateFile {
            parent: directory.clone(),
            name: "renamed.txt".into()
        }
        .execute()
        .is_err()
    );
    assert!(
        FolderMutation::Rename {
            path: renamed.clone(),
            name: "../escape".into()
        }
        .execute()
        .is_err()
    );
    assert!(
        FolderMutation::CreateDirectory {
            parent: directory,
            name: "..".into()
        }
        .execute()
        .is_err()
    );
    assert_eq!(fs::read_to_string(renamed).expect("preserved"), "keep me");
}

#[cfg(unix)]
#[test]
fn follow_links_setting_traverses_directory_links_without_cycles() {
    let fixture = tempfile::tempdir().expect("fixture");
    let outside = tempfile::tempdir().expect("linked fixture");
    let root = fixture.path().canonicalize().expect("root");
    fs::write(outside.path().join("linked.txt"), "contents").expect("file");
    std::os::unix::fs::symlink(outside.path(), root.join("link")).expect("link");
    std::os::unix::fs::symlink(&root, outside.path().join("back")).expect("cycle");
    let mut tree = FolderWorkspace::new();
    tree.set_root(&root).expect("root");
    assert_eq!(tree.entries()[1].kind, EntryKind::Symlink);
    tree.set_follow_links(true);
    tree.toggle_directory(&root.join("link"))
        .expect("expand linked directory");
    assert!(
        tree.entries()
            .iter()
            .any(|entry| entry.path == root.join("link/linked.txt"))
    );
    tree.toggle_directory(&root.join("link/back"))
        .expect("expand cycle");
    assert!(tree.entries().len() < 8);
    assert!(tree.errors().iter().any(|error| error.contains("repeated")));
    tree.set_follow_links(false);
    tree.refresh();
    assert!(
        !tree
            .entries()
            .iter()
            .any(|entry| entry.path == root.join("link/linked.txt"))
    );
}

#[test]
fn metadata_watch_observes_real_changes_and_pauses_hidden_tree() {
    use notepad_matrix::folder_workspace::FolderWatch;
    let fixture = tempfile::tempdir().expect("fixture");
    let mut tree = FolderWorkspace::new();
    tree.set_root(fixture.path()).expect("root");
    let mut watch = FolderWatch::default();
    assert!(!watch.poll(&tree).expect("baseline"));
    fs::write(fixture.path().join("created.txt"), "new").expect("create");
    assert!(watch.poll(&tree).expect("created"));
    assert!(!watch.poll(&tree).expect("settled"));
    fs::rename(
        fixture.path().join("created.txt"),
        fixture.path().join("renamed.txt"),
    )
    .expect("rename");
    assert!(watch.poll(&tree).expect("renamed"));
    tree.set_visible(false);
    fs::remove_file(fixture.path().join("renamed.txt")).expect("delete fixture");
    assert!(!watch.poll(&tree).expect("hidden"));
    tree.set_visible(true);
    assert!(watch.poll(&tree).expect("reopen rescan"));
}

#[test]
fn scan_controller_runs_one_job_and_coalesces_only_the_latest_root() {
    use notepad_matrix::folder_workspace::FolderScanController;
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().canonicalize().expect("canonical fixture");
    for name in ["original", "intermediate", "latest"] {
        fs::create_dir(root.join(name)).expect("root");
        fs::write(root.join(name).join("file.txt"), name).expect("file");
    }
    let mut tree = FolderWorkspace::new();
    tree.set_root(&root.join("original")).expect("initial root");
    let mut controller = FolderScanController::new();
    let first = controller
        .request(tree.scan_input(None))
        .expect("first job");
    assert!(controller.is_busy());
    let first_result = first.run();
    for _ in 0..200 {
        assert!(
            controller
                .request(tree.scan_input(Some(root.join("intermediate"))))
                .is_none()
        );
    }
    assert!(
        controller
            .request(tree.scan_input(Some(root.join("latest"))))
            .is_none()
    );
    let update = controller.complete(first_result);
    assert!(
        update.result.is_none(),
        "superseded result is not published"
    );
    let latest = update.next.expect("one coalesced successor");
    let update = controller.complete(latest.run());
    assert!(
        update.next.is_none(),
        "intermediate requests did not form a queue"
    );
    let scanned = update.result.expect("current result").expect("scan");
    assert_eq!(scanned.root(), Some(root.join("latest").as_path()));
    assert_eq!(scanned.entries().len(), 2);
    assert!(!controller.is_busy());
}

#[test]
fn cancelled_scan_keeps_its_slot_and_reopen_scans_latest_expansion_intent() {
    use notepad_matrix::folder_workspace::FolderScanController;
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().canonicalize().expect("canonical fixture");
    for name in ["a", "b"] {
        fs::create_dir(root.join(name)).expect("directory");
        fs::write(root.join(name).join("inside.txt"), name).expect("file");
    }
    let mut tree = FolderWorkspace::new();
    tree.set_root(&root).expect("root");
    let mut controller = FolderScanController::new();
    let running = controller
        .request(tree.scan_input(None))
        .expect("running job");
    tree.toggle_directory_expansion(&root.join("a"))
        .expect("expand a");
    assert!(controller.request(tree.scan_input(None)).is_none());
    tree.toggle_directory_expansion(&root.join("b"))
        .expect("expand b");
    assert!(controller.request(tree.scan_input(None)).is_none());
    controller.cancel();
    assert!(controller.is_busy());
    assert!(running.is_cancelled());
    let update = controller.complete(running.run());
    assert!(update.result.is_none());
    assert!(
        update.next.is_none(),
        "hidden scans do not start a successor"
    );
    assert!(!controller.is_busy());
    assert_eq!(
        tree.entries().len(),
        3,
        "snapshotting did not mutate the displayed cache"
    );
    let reopened = controller
        .request(tree.scan_input(None))
        .expect("reopen job");
    let update = controller.complete(reopened.run());
    let scanned = update.result.expect("result").expect("scan");
    assert_eq!(scanned.entries().len(), 5);
    assert!(scanned.is_expanded(&root.join("a")));
    assert!(scanned.is_expanded(&root.join("b")));
    assert!(!controller.is_busy());
}

#[test]
fn reopen_before_cancelled_worker_returns_queues_instead_of_starting_another_worker() {
    use notepad_matrix::folder_workspace::FolderScanController;
    let fixture = tempfile::tempdir().expect("fixture");
    let mut tree = FolderWorkspace::new();
    tree.set_root(fixture.path()).expect("root");
    let mut controller = FolderScanController::new();
    let running = controller
        .request(tree.scan_input(None))
        .expect("running job");
    controller.cancel();
    assert!(
        controller.request(tree.scan_input(None)).is_none(),
        "reopen must await cancellation acknowledgement"
    );
    let update = controller.complete(running.run());
    assert!(update.result.is_none());
    let next = update.next.expect("reopen successor");
    assert!(!next.is_cancelled());
    assert!(
        controller
            .complete(next.run())
            .result
            .expect("result")
            .is_ok()
    );
    assert!(!controller.is_busy());
}

#[test]
fn failed_scan_publishes_error_without_changing_cached_tree_or_wedging_scheduler() {
    use notepad_matrix::folder_workspace::FolderScanController;
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().canonicalize().expect("canonical fixture");
    fs::write(root.join("keep.txt"), "keep").expect("file");
    let mut tree = FolderWorkspace::new();
    tree.set_root(&root).expect("root");
    let mut controller = FolderScanController::new();
    let missing = root.join("missing");
    let job = controller
        .request(tree.scan_input(Some(missing.clone())))
        .expect("job");
    let update = controller.complete(job.run());
    let error = update
        .result
        .expect("published failure")
        .err()
        .expect("missing root error");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(error.to_string().contains(&missing.display().to_string()));
    assert!(update.next.is_none());
    assert!(!controller.is_busy());
    assert_eq!(tree.root(), Some(root.as_path()));
    assert_eq!(tree.entries().len(), 2);
    let retry = controller
        .request(tree.scan_input(None))
        .expect("retry job");
    assert!(
        controller
            .complete(retry.run())
            .result
            .expect("retry result")
            .is_ok()
    );
    assert!(!controller.is_busy());
}

#[test]
fn actual_tree_expands_refreshes_and_preserves_state_when_hidden() {
    let temp = tempfile::tempdir().expect("fixture");
    let root = temp.path().canonicalize().expect("canonical fixture");
    fs::create_dir(root.join("child")).expect("directory");
    fs::write(root.join("child/inside.txt"), "inside").expect("file");
    fs::write(root.join("root.txt"), "root").expect("file");
    let mut folder = FolderWorkspace::new();
    folder.set_root(&root).expect("root");
    assert_eq!(folder.entries().len(), 3);
    assert_eq!(folder.entries()[1].kind, EntryKind::Directory);
    assert!(
        !folder
            .entries()
            .iter()
            .any(|e| e.path.ends_with("inside.txt"))
    );
    folder
        .toggle_directory(&root.join("child"))
        .expect("expand");
    assert!(
        folder
            .entries()
            .iter()
            .any(|e| e.path.ends_with("inside.txt") && e.depth == 2)
    );
    folder.set_visible(false);
    folder.set_visible(true);
    assert!(folder.is_expanded(&root.join("child")));
    fs::write(root.join("new.txt"), "new").expect("new file");
    assert!(!folder.entries().iter().any(|e| e.path.ends_with("new.txt")));
    folder.refresh();
    assert!(folder.entries().iter().any(|e| e.path.ends_with("new.txt")));
    assert!(folder.is_expanded(&root.join("child")));
    assert!(folder.errors().is_empty());
    folder
        .toggle_directory(&root.join("child"))
        .expect("collapse");
    assert!(
        !folder
            .entries()
            .iter()
            .any(|e| e.path.ends_with("inside.txt"))
    );
}

#[test]
fn file_click_reuses_dirty_buffer_and_failed_load_does_not_change_workspace() {
    use notepad_matrix::folder_workspace::{FolderFile, existing_buffer};
    use notepad_matrix::workspace::Workspace;
    let temp = tempfile::tempdir().expect("fixture");
    let path = temp.path().join("document.txt");
    fs::write(&path, "original").expect("file");
    let mut workspace = Workspace::new();
    let id = FolderFile::load(&path)
        .expect("load")
        .apply(&mut workspace)
        .expect("open");
    workspace
        .active_mut()
        .editor
        .insert_text("unsaved ")
        .expect("edit");
    let edited = workspace.active().editor.committed_text();
    let selection = workspace.active().editor.document().selection();
    let count = workspace.buffers().len();
    workspace.new_buffer().expect("new document");
    assert_eq!(
        existing_buffer(&workspace, &temp.path().join("./document.txt")),
        Some(id)
    );
    assert_eq!(
        FolderFile::load(&path)
            .expect("reload payload")
            .apply(&mut workspace)
            .expect("reuse"),
        id
    );
    assert_eq!(workspace.buffers().len(), count + 1);
    assert_eq!(workspace.active().editor.committed_text(), edited);
    assert_eq!(workspace.active().editor.document().selection(), selection);
    assert!(workspace.active().is_modified());
    assert!(FolderFile::load(&temp.path().join("missing.txt")).is_err());
    fs::write(temp.path().join("invalid.txt"), [0xff, 0xff]).expect("invalid encoding");
    assert!(FolderFile::load(&temp.path().join("invalid.txt")).is_err());
    assert!(FolderFile::load(temp.path()).is_err());
    assert_eq!(workspace.active().id(), id);
    assert_eq!(
        fs::read_to_string(&path).expect("unchanged disk"),
        "original"
    );
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.committed_text(), "original");
}

#[test]
fn all_2050_files_are_accessible_and_missing_root_reports_error_without_mutating_disk() {
    let temp = tempfile::tempdir().expect("fixture");
    let home = temp.path().canonicalize().expect("canonical fixture");
    let root = home.join("root");
    fs::create_dir(&root).expect("root");
    for index in 0..2050 {
        fs::write(root.join(format!("file-{index:04}")), "keep").expect("file");
    }
    let mut folder = FolderWorkspace::new();
    folder.set_root(&root).expect("select root");
    assert_eq!(folder.entries().len(), 2051);
    for index in 0..2050 {
        assert!(
            folder
                .entries()
                .iter()
                .any(|entry| entry.path == root.join(format!("file-{index:04}")))
        );
    }
    let last = root.join("file-2049");
    let mut workspace = notepad_matrix::workspace::Workspace::new();
    notepad_matrix::folder_workspace::FolderFile::load(&last)
        .expect("last file accessible")
        .apply(&mut workspace)
        .expect("open last file");
    assert_eq!(workspace.active().editor.committed_text(), "keep");
    folder.set_visible(false);
    folder.set_visible(true);
    folder.refresh();
    assert_eq!(folder.entries().len(), 2051);
    assert!(folder.is_expanded(&root));
    assert!(folder.set_root(&root.join("file-0000")).is_err());
    assert_eq!(folder.root(), Some(root.as_path()));
    let moved = home.join("moved");
    fs::rename(&root, &moved).expect("external rename");
    folder.refresh();
    assert!(!folder.errors().is_empty());
    assert!(folder.entries().is_empty());
    assert_eq!(
        fs::read_to_string(moved.join("file-0000")).expect("user file retained"),
        "keep"
    );
}

#[cfg(unix)]
#[test]
fn symlink_loops_are_not_traversed_and_file_alias_reuses_the_buffer() {
    use notepad_matrix::folder_workspace::{FolderFile, existing_buffer};
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().expect("fixture");
    let root = temp.path().canonicalize().expect("canonical fixture");
    fs::create_dir(root.join("child")).expect("child");
    symlink(&root, root.join("child/loop")).expect("loop");
    fs::write(root.join("file.txt"), "keep").expect("file");
    symlink(root.join("file.txt"), root.join("alias.txt")).expect("file alias");
    symlink(root.join("absent"), root.join("broken")).expect("broken symlink");
    let mut folder = FolderWorkspace::new();
    folder.set_root(&root).expect("root");
    folder
        .toggle_directory(&root.join("child"))
        .expect("expand");
    let loop_path = root.join("child/loop");
    assert!(
        folder
            .entries()
            .iter()
            .any(|e| e.path == loop_path && e.kind == EntryKind::Symlink)
    );
    assert!(folder.toggle_directory(&loop_path).is_err());
    assert_eq!(folder.entries().len(), 6);
    assert!(FolderFile::load(&temp.path().join("broken")).is_err());
    let mut workspace = notepad_matrix::workspace::Workspace::new();
    let id = FolderFile::load(&temp.path().join("file.txt"))
        .expect("load")
        .apply(&mut workspace)
        .expect("open");
    assert_eq!(
        existing_buffer(&workspace, &temp.path().join("alias.txt")),
        Some(id)
    );
    assert_eq!(
        FolderFile::load(&temp.path().join("alias.txt"))
            .expect("load alias")
            .apply(&mut workspace)
            .expect("reuse"),
        id
    );
    assert_eq!(workspace.buffers().len(), 2);
}

#[test]
fn tree_expands_past_depth_64_and_collapsing_parent_keeps_descendant_expansion() {
    let depth = 66;
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().canonicalize().expect("canonical fixture");
    let mut path = root.clone();
    for _ in 0..depth {
        path.push("d");
        fs::create_dir(&path).expect("nested directory");
    }
    let mut folder = FolderWorkspace::new();
    folder.set_root(&root).expect("root");
    let mut path = root.clone();
    for _ in 0..depth {
        path.push("d");
        folder
            .toggle_directory(&path)
            .expect("expand visible directory");
    }
    assert_eq!(folder.entries().len(), depth + 1);
    assert_eq!(folder.entries().last().expect("deepest row").depth, depth);
    folder
        .toggle_directory(&root.join("d"))
        .expect("collapse first child");
    assert_eq!(folder.entries().len(), 2);
    folder
        .toggle_directory(&root.join("d"))
        .expect("restore first child");
    assert!(folder.is_expanded(&path));
    assert_eq!(folder.entries().len(), depth + 1);
}
