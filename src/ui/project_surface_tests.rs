use super::*;
use crate::workspace::BufferId;

#[gpui::test]
fn project_rows_select_without_activation_and_deliver_enter_after_click(
    app: &mut gpui::TestAppContext,
) {
    let fixture = tempfile::tempdir().expect("isolated project activation");
    let path = fixture.path().join("document.txt");
    std::fs::write(&path, "original").expect("project file");
    for (index, root, file, panel_selector) in [
        (
            0,
            "project-entry-0-1",
            "project-entry-0-2",
            "project-panel-0",
        ),
        (
            1,
            "project-entry-1-1",
            "project-entry-1-2",
            "project-panel-1",
        ),
        (
            2,
            "project-entry-2-1",
            "project-entry-2-2",
            "project-panel-2",
        ),
    ] {
        let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(1080.), px(668.)));
        view.update_in(cx, |view, _, _| {
            let panel = view.projects.model.panel_mut(index).expect("project panel");
            let project = panel.add_project("Project").expect("root");
            panel.add_file(project, &path).expect("file entry");
        });
        let windows = cx.windows();
        for selector in [
            "view-menu-trigger",
            "view-project-panels",
            [
                "view-project-panel-1",
                "view-project-panel-2",
                "view-project-panel-3",
            ][index],
        ] {
            let entry = cx
                .debug_bounds(selector)
                .expect("source Project menu entry");
            assert!(entry.top() >= px(0.) && entry.bottom() <= px(668.));
            cx.simulate_click(entry.center(), gpui::Modifiers::default());
        }
        assert!(cx.debug_bounds(panel_selector).is_some());
        assert_eq!(cx.windows(), windows);
        assert!(
            !cx.did_prompt_for_paths() && !cx.did_prompt_for_new_path() && !cx.has_pending_prompt()
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.buffers().len(), 1);
            assert_eq!(view.workspace.active().id(), BufferId(1));
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo() && !view.workspace.active().is_modified());
        });
        let position = cx.debug_bounds(file).expect("rendered file").center();
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window));
        });
        cx.simulate_click(position, gpui::Modifiers::default());
        view.update_in(cx, |view, window, _| {
            assert!(view.projects.panels[index].tree_focus.is_focused(window));
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.workspace.buffers().len(),
                1,
                "single click only selects"
            );
            assert_eq!(view.workspace.active().id(), BufferId(1));
            assert_eq!(view.editor().committed_text(), "");
            assert_eq!(
                view.projects.model.panel(index).expect("panel").selected,
                Some(2)
            );
        });
        let previous = view.read_with(cx, |view, _| {
            let buffer = view.workspace.active();
            (
                buffer.id(),
                buffer.editor.committed_text(),
                buffer.editor.selections(),
                buffer.editor.document().can_undo(),
                buffer.editor.document().can_redo(),
                buffer.is_modified(),
            )
        });
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.buffers().len(), 2);
            assert_eq!(view.editor().committed_text(), "original");
            let buffer = view
                .workspace
                .buffers()
                .iter()
                .find(|buffer| buffer.id() == previous.0)
                .expect("previous editor retained");
            assert_eq!(
                (
                    buffer.id(),
                    buffer.editor.committed_text(),
                    buffer.editor.selections(),
                    buffer.editor.document().can_undo(),
                    buffer.editor.document().can_redo(),
                    buffer.is_modified(),
                ),
                previous,
                "file Enter cannot edit or add history to the previously focused editor"
            );
        });
        cx.dispatch_action(NewFile);
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window));
        });
        let position = cx.debug_bounds(file).expect("file still visible").center();
        cx.simulate_click(position, gpui::Modifiers::default());
        view.update_in(cx, |view, window, _| {
            assert!(view.projects.panels[index].tree_focus.is_focused(window));
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(3));
            assert_eq!(view.editor().committed_text(), "");
        });
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.buffers().len(), 3);
            assert_eq!(view.workspace.active().id(), BufferId(2));
            assert_eq!(view.editor().committed_text(), "original");
            assert!(!view.editor().document().can_undo());
        });
        let position = cx.debug_bounds(root).expect("root").center();
        cx.simulate_click(position, gpui::Modifiers::default());
        assert!(
            cx.debug_bounds(file).is_some(),
            "single click does not fold"
        );
        cx.simulate_keystrokes("enter");
        assert!(cx.debug_bounds(file).is_none(), "Enter folds selected root");
        cx.simulate_keystrokes("enter");
        assert!(
            cx.debug_bounds(file).is_some(),
            "Enter unfolds selected root"
        );
        let panel = cx.debug_bounds(panel_selector).expect("panel header");
        let selected = view.read_with(cx, |view, _| {
            view.projects
                .model
                .panel(index)
                .expect("selected Project")
                .selected
        });
        cx.simulate_click(
            gpui::point(panel.right() - px(15.), panel.top() + px(13.)),
            gpui::Modifiers::default(),
        );
        assert!(cx.debug_bounds(panel_selector).is_none());
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window))
        });
        cx.simulate_input("Z");
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(2));
            assert_eq!(view.editor().committed_text(), "Zoriginal");
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "original");
            assert!(!view.editor().document().can_undo());
        });
        let document = |view: &MatrixWindow| {
            (
                view.workspace.active().id(),
                view.workspace.active_view(),
                view.editor().committed_text(),
                view.editor().selections(),
                view.editor().document().can_undo(),
                view.editor().document().can_redo(),
                view.workspace.active().is_modified(),
            )
        };
        let restored = view.read_with(cx, |view, _| document(view));
        for selector in [
            "view-menu-trigger",
            "view-project-panels",
            [
                "view-project-panel-1",
                "view-project-panel-2",
                "view-project-panel-3",
            ][index],
        ] {
            let entry = cx
                .debug_bounds(selector)
                .expect("Project reopen menu entry");
            cx.simulate_click(entry.center(), gpui::Modifiers::default());
        }
        assert!(cx.debug_bounds(panel_selector).is_some());
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.projects
                    .model
                    .panel(index)
                    .expect("reopened Project")
                    .selected,
                selected
            );
            assert_eq!(document(view), restored);
            assert_eq!(view.workspace.active().id(), BufferId(2));
            assert_eq!(view.editor().committed_text(), "original");
            assert!(!view.editor().document().can_undo());
        });
        let (root_view, root_cx) =
            app.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        root_view.update_in(root_cx, |view, window, cx| {
            let panel = view
                .projects
                .model
                .panel_mut(index)
                .expect("fresh root panel");
            let project = panel.add_project("Project").expect("root");
            panel.add_file(project, &path).expect("child file");
            view.show_project_panel(index, window, cx);
        });
        root_cx.simulate_input("first");
        root_cx.dispatch_action(NewFile);
        root_cx.simulate_input("second");
        let editor = root_cx.debug_bounds("matrix-editor").expect("fresh editor");
        root_cx.simulate_click(editor.center(), gpui::Modifiers::default());
        root_view.update_in(root_cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window));
        });
        let snapshot = |view: &MatrixWindow| {
            (
                view.workspace.active().id(),
                view.workspace
                    .buffers()
                    .iter()
                    .map(|buffer| {
                        (
                            buffer.id(),
                            buffer.editor.committed_text(),
                            buffer.editor.selections(),
                            buffer.editor.document().can_undo(),
                            buffer.editor.document().can_redo(),
                            buffer.is_modified(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let before = root_view.read_with(root_cx, |view, _| snapshot(view));
        let position = root_cx.debug_bounds(root).expect("fresh root").center();
        root_cx.simulate_click(position, gpui::Modifiers::default());
        root_view.update_in(root_cx, |view, window, _| {
            assert!(view.projects.panels[index].tree_focus.is_focused(window));
        });
        assert!(root_cx.debug_bounds(file).is_some());
        for expanded in [false, true] {
            root_cx.simulate_keystrokes("enter");
            assert_eq!(root_cx.debug_bounds(file).is_some(), expanded);
            assert_eq!(
                root_view.read_with(root_cx, |view, _| snapshot(view)),
                before
            );
        }
    }
}

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
