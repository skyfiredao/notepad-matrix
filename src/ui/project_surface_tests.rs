use super::*;

#[gpui::test]
fn find_in_projects_navigates_without_overwriting_changed_document(cx: &mut gpui::TestAppContext) {
    let fixture = tempfile::tempdir().expect("fixture");
    let path = fixture.path().join("find.txt");
    std::fs::write(&path, "one needle two").expect("file");
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let panel = view.projects.model.panel_mut(0).expect("panel");
        let project = panel.add_project("Search").expect("project");
        panel.add_file(project, &path).expect("file");
        view.project_command(0, ProjectCommand::Find, window, cx);
        view.projects.panels[0]
            .name
            .update(cx, |field, cx| field.set_text("needle", window, cx));
        view.apply_project_edit(0, window, cx);
        let hit = view.projects.panels[0]
            .results
            .as_ref()
            .expect("results")
            .matches[0]
            .clone();
        view.activate_project_match(0, hit.clone(), window, cx);
        assert_eq!(
            view.workspace
                .active()
                .editor
                .document()
                .selection()
                .range(),
            ByteOffset(4)..ByteOffset(10)
        );
        view.workspace
            .active_mut()
            .editor
            .insert_text("changed")
            .expect("edit");
        let edited = view.workspace.active().editor.committed_text();
        view.activate_project_match(0, hit, window, cx);
        assert_eq!(view.workspace.active().editor.committed_text(), edited);
        assert!(
            view.projects.panels[0]
                .message
                .as_deref()
                .is_some_and(|message| message.contains("changed since search"))
        );
    });
}

#[gpui::test]
fn replacing_workspace_clears_stale_edit_and_cancelled_open_keeps_dirty_tree(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.project_command(0, ProjectCommand::AddProject, window, cx);
        view.apply_project_edit(0, window, cx);
        view.project_command(0, ProjectCommand::Rename, window, cx);
        view.project_command(0, ProjectCommand::Open, window, cx);
        view.confirm_project_action(0, true, window, cx);
    });
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        assert!(view.projects.model.panel(0).expect("panel").dirty());
        assert_eq!(
            view.projects.model.panel(0).expect("panel").roots().len(),
            1
        );
        view.project_command(0, ProjectCommand::New, window, cx);
        view.confirm_project_action(0, true, window, cx);
        assert!(
            view.projects
                .model
                .panel(0)
                .expect("panel")
                .roots()
                .is_empty()
        );
        assert!(view.projects.panels[0].edit.is_none());
        assert!(view.projects.panels[0].confirm.is_none());
    });
}

#[gpui::test]
fn native_project_workflow_imports_adds_saves_exports_and_reuses_dirty_document(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = tempfile::tempdir().expect("fixture");
    let file = fixture.path().join("document.txt");
    std::fs::write(&file, "original").expect("document");
    let imported = fixture.path().join("import.json");
    let saved = fixture.path().join("saved.json");
    let exported = fixture.path().join("exported.json");
    let mut model = ProjectWorkspace::default();
    let project = model.add_project("Application").expect("project");
    model.save_as(&imported).expect("import fixture");
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.show_project_panel(0, window, cx);
        view.project_command(0, ProjectCommand::Open, window, cx);
    });
    cx.simulate_path_prompt_response(|_| Some(vec![imported.clone()]));
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        assert_eq!(
            view.projects.model.panel(0).expect("panel").roots()[0].name,
            "Application"
        );
        view.projects.model.panel_mut(0).expect("panel").selected = Some(project);
        view.project_command(0, ProjectCommand::AddFiles, window, cx);
    });
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && options.multiple && !options.directories);
        Some(vec![file.clone()])
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        let entry = view.projects.model.panel(0).expect("panel").roots()[0].children[0].id;
        view.activate_project_entry(0, entry, window, cx);
        let id = view.workspace.active().id();
        view.workspace
            .active_mut()
            .editor
            .insert_text("unsaved ")
            .expect("edit");
        let text = view.workspace.active().editor.committed_text();
        view.workspace.new_buffer().expect("new buffer");
        let count = view.workspace.buffers().len();
        view.activate_project_entry(0, entry, window, cx);
        assert_eq!(view.workspace.active().id(), id);
        assert_eq!(view.workspace.buffers().len(), count);
        assert_eq!(view.workspace.active().editor.committed_text(), text);
        view.project_command(0, ProjectCommand::SaveAs, window, cx);
    });
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert!(saved.exists());
    view.update_in(cx, |view, window, cx| {
        view.project_command(0, ProjectCommand::Export, window, cx)
    });
    cx.simulate_new_path_selection(|_| Some(exported.clone()));
    cx.run_until_parked();
    assert!(exported.exists());
    view.update_in(cx, |view, window, cx| {
        let panel = view.projects.model.panel(0).expect("panel");
        assert_eq!(panel.path(), Some(saved.as_path()));
        assert!(!panel.dirty());
        view.project_command(0, ProjectCommand::Remove, window, cx);
        view.confirm_project_action(0, true, window, cx);
        assert!(
            view.projects.model.panel(0).expect("panel").roots()[0]
                .children
                .is_empty()
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("physical file preserved"),
            "original"
        );
        let snapshot = view.projects.snapshot().expect("snapshot");
        view.projects.restore(&snapshot).expect("restart");
        assert!(view.projects.visible(0));
        assert!(view.render_project_panel(0, cx).is_some());
    });
}

#[gpui::test]
fn removal_confirmation_is_bound_to_original_entry(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        let panel = view.projects.model.panel_mut(0).expect("panel");
        let first = panel.add_project("Remove this").expect("project");
        let second = panel.add_project("Keep this").expect("project");
        panel.selected = Some(first);
        view.project_command(0, ProjectCommand::Remove, window, cx);
        view.projects.model.panel_mut(0).expect("panel").selected = Some(second);
        view.confirm_project_action(0, true, window, cx);
        let panel = view.projects.model.panel(0).expect("panel");
        assert!(panel.node(first).is_none());
        assert!(panel.node(second).is_some());
    });
}

#[gpui::test]
fn three_project_surfaces_keep_independent_trees_when_hidden(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        for index in 0..3 {
            view.show_project_panel(index, window, cx);
            view.project_command(index, ProjectCommand::AddProject, window, cx);
            let field = view.projects.panels[index].name.clone();
            field.update(cx, |field, cx| {
                field.set_text(&format!("Project {index}"), window, cx)
            });
            view.apply_project_edit(index, window, cx);
            assert!(view.render_project_panel(index, cx).is_some());
            view.hide_project_panel(index, window, cx);
            assert!(view.render_project_panel(index, cx).is_none());
        }
        for index in 0..3 {
            view.show_project_panel(index, window, cx);
            assert_eq!(
                view.projects.model.panel(index).expect("panel").roots()[0].name,
                format!("Project {index}")
            );
        }
    });
}

#[gpui::test]
fn project_open_and_save_use_native_path_prompts(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.show_project_panel(0, window, cx);
        view.project_command(0, ProjectCommand::Open, window, cx);
    });
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.project_command(0, ProjectCommand::SaveAs, window, cx)
    });
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.projects.picker_pending);
        assert!(!view.file_busy);
    });
}
