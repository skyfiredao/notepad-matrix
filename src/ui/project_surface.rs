use super::*;
use crate::projects::{
    NodeId, NodeKind, ProjectMatch, ProjectNode, ProjectPanels, ProjectSearchResults,
    ProjectWorkspace,
};


pub(super) struct ProjectState {
    pub(super) model: ProjectPanels,
    pending_workspace_path: Option<PathBuf>,
    workspace_completion: Option<std::sync::mpsc::Sender<bool>>,
    panels: [PanelSurface; 3],
    picker_pending: bool,
}

struct PanelSurface {
    name: gpui::Entity<text_field::TextField>,
    tree_focus: FocusHandle,
    edit: Option<ProjectEdit>,
    confirm: Option<(ProjectCommand, Option<NodeId>)>,
    commands: Option<ProjectMenu>,
    workspace_expanded: bool,
    toolbar_bounds: [Rc<std::cell::Cell<Bounds<gpui::Pixels>>>; 2],
    widths: std::cell::RefCell<ProjectWidths>,
    message: Option<String>,
    results: Option<ProjectSearchResults>,
}

#[derive(Default)]
struct ProjectWidths {
    font: Option<(String, gpui::Pixels)>,
    toolbar: Option<gpui::Pixels>,
    workspace: Option<(String, gpui::Pixels)>,
    labels: std::collections::BTreeMap<NodeId, (String, gpui::Pixels)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectMenu {
    Workspace,
    Edit,
}

#[derive(Clone, Copy)]
enum ProjectEdit {
    Project,
    Group(NodeId),
    Rename(NodeId),
    FilePath(NodeId),
    Find,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectCommand {
    New,
    Open,
    Reload,
    Save,
    SaveAs,
    Export,
    AddProject,
    AddGroup,
    AddFiles,
    AddFolder,
    Rename,
    ModifyPath,
    Remove,
    MoveUp,
    MoveDown,
    Find,
}

#[derive(Clone)]
struct ProjectDrag {
    panel: usize,
    node: NodeId,
    label: String,
}

impl Render for ProjectDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .bg(rgb(0x365a87))
            .text_color(rgb(0xffffff))
            .child(self.label.clone())
    }
}

impl ProjectState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            model: ProjectPanels::new(),
            pending_workspace_path: None,
            workspace_completion: None,
            panels: std::array::from_fn(|index| PanelSurface {
                name: cx.new(|cx| {
                    text_field::TextField::new(
                        "Project entry name or path",
                        match index {
                            0 => "project-name-0",
                            1 => "project-name-1",
                            _ => "project-name-2",
                        },
                        cx,
                    )
                }),
                tree_focus: cx.focus_handle(),
                edit: None,
                confirm: None,
                commands: None,
                workspace_expanded: true,
                toolbar_bounds: std::array::from_fn(|_| Rc::new(std::cell::Cell::new(Bounds::default()))),
                widths: std::cell::RefCell::new(ProjectWidths::default()),
                message: None,
                results: None,
            }),
            picker_pending: false,
        }
    }

    pub(super) fn snapshot(&self) -> io::Result<serde_json::Value> {
        self.model.snapshot()
    }

    pub(super) fn restore(&mut self, value: &serde_json::Value) -> io::Result<()> {
        self.model = ProjectPanels::restore(value)?;
        for panel in &mut self.panels {
            panel.edit = None;
            panel.confirm = None;
            panel.commands = None;
            panel.workspace_expanded = true;
            panel.message = None;
            panel.results = None;
        }
        Ok(())
    }

    pub(super) fn visible(&self, index: usize) -> bool {
        self.model.panel(index).is_ok_and(|panel| panel.visible)
    }
}

impl MatrixWindow {
    pub(super) fn project_workspace_confirmation_pending(&self) -> bool {
        self.projects.pending_workspace_path.is_some()
    }

    pub(super) fn load_project_workspace_path(&mut self,path: PathBuf,window: &mut Window,cx: &mut Context<Self>) {
        self.load_project_workspace_path_guarded(path,false,window,cx);
    }

    pub(super) fn load_project_workspace_path_with_completion(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> io::Result<std::sync::mpsc::Receiver<bool>> {
        if self.project_operations_blocked() || self.projects.workspace_completion.is_some() { return Err(io::Error::other("another project operation is pending")); }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.projects.workspace_completion = Some(sender);
        self.load_project_workspace_path(path, window, cx);
        Ok(receiver)
    }

    fn finish_project_workspace_open(&mut self, accepted: bool) {
        if let Some(sender) = self.projects.workspace_completion.take()
            && let Err(error) = sender.send(accepted) { eprintln!("project open completion unavailable: {error}"); }
    }

    fn load_project_workspace_path_guarded(&mut self,path: PathBuf,discard: bool,window: &mut Window,cx: &mut Context<Self>) {
        if self.project_operations_blocked() { self.finish_project_workspace_open(false); return; }
        if !discard && self.projects.model.panel(0).is_ok_and(|panel| panel.dirty()) {
            self.projects.pending_workspace_path = Some(path);
            self.projects.panels[0].confirm = Some((ProjectCommand::Open,None));
            cx.notify();
            return;
        }
        self.file_busy = true;
        let executor = cx.background_executor().clone();
        cx.spawn_in(window,async move |view,cx| {
            let result = executor.spawn(async move {
                let mut workspace = crate::projects::ProjectWorkspace::default();
                workspace.load(&path,false)?;
                Ok::<_,io::Error>(workspace)
            }).await;
            if let Err(error) = view.update_in(cx,|view,window,cx| {
                view.file_busy = false;
                let accepted = match result {
                    Ok(mut workspace) => {
                        workspace.visible = true;
                        match view.projects.model.panel_mut(0) {
                            Ok(panel) => { *panel = workspace; view.clear_project_transients(0); view.show_panel(crate::panel_layout::PanelId::Project1,window,cx); true }
                            Err(error) => { view.notify_editor_error(error.to_string(),cx); false },
                        }
                    }
                    Err(error) => { view.notify_editor_error(format!("项目工作区无法打开: {error}"),cx); false },
                };
                view.finish_project_workspace_open(accepted);
                cx.notify();
            }) { eprintln!("project workspace path window unavailable: {error}"); }
        }).detach();
    }

    pub(super) fn show_project_panel(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.show_project_panel(index, window, cx)
        }) {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if let Ok(panel) = self.projects.model.panel_mut(index) {
            panel.visible = true;
        }
        cx.notify();
    }

    pub(super) fn hide_project_panel(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.hide_project_panel(index, window, cx)
        }) {
            return;
        }
        if let Ok(panel) = self.projects.model.panel_mut(index) {
            panel.visible = false;
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    fn project_operations_blocked(&self) -> bool {
        self.projects.picker_pending
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
    }

    fn project_result<T>(&mut self, index: usize, result: io::Result<T>, cx: &mut Context<Self>) {
        if let Some(panel) = self.projects.panels.get_mut(index) {
            panel.message = result.err().map(|error| error.to_string());
        }
        cx.notify();
    }

    fn project_command(
        &mut self,
        index: usize,
        command: ProjectCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.project_command(index, command, window, cx)
        }) {
            return;
        }
        if index >= 3 || self.project_operations_blocked() {
            return;
        }
        self.projects.panels[index].commands = None;
        if command == ProjectCommand::Find {
            self.projects.panels[index].commands = None;
            if !self.set_find_project_panels(std::array::from_fn(|panel| panel == index), cx) { return; }
            self.open_file_search_tab(true, true, window, cx);
            return;
        }
        let Ok(panel) = self.projects.model.panel(index) else {
            return;
        };
        let selected = panel.selected;
        if matches!(
            command,
            ProjectCommand::New | ProjectCommand::Open | ProjectCommand::Reload
        ) && panel.dirty()
        {
            self.projects.panels[index].confirm = Some((command, selected));
            cx.notify();
            return;
        }
        if command == ProjectCommand::Remove {
            if selected.is_some() {
                self.projects.panels[index].confirm = Some((command, selected));
            }
            cx.notify();
            return;
        }
        self.execute_project_command(index, command, selected, false, window, cx);
    }

    fn execute_project_command(
        &mut self,
        index: usize,
        command: ProjectCommand,
        selected: Option<NodeId>,
        discard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.projects.panels[index].commands = None;
        self.projects.panels[index].results = None;
        self.projects.panels[index].message = None;
        match command {
            ProjectCommand::Open | ProjectCommand::AddFiles | ProjectCommand::AddFolder => {
                self.choose_project_paths(index, command, selected, discard, window, cx);
            }
            ProjectCommand::SaveAs | ProjectCommand::Export => {
                self.choose_project_save(index, command, window, cx)
            }
            ProjectCommand::Save => {
                if self
                    .projects
                    .model
                    .panel(index)
                    .is_ok_and(|panel| panel.path().is_none())
                {
                    self.choose_project_save(index, ProjectCommand::SaveAs, window, cx);
                } else {
                    let result = self
                        .projects
                        .model
                        .panel_mut(index)
                        .and_then(ProjectWorkspace::save);
                    self.project_result(index, result, cx);
                }
            }
            ProjectCommand::New => {
                let result = self
                    .projects
                    .model
                    .panel_mut(index)
                    .and_then(|panel| panel.clear(discard));
                if result.is_ok() {
                    self.clear_project_transients(index);
                }
                self.project_result(index, result, cx);
            }
            ProjectCommand::Reload => {
                let result = self
                    .projects
                    .model
                    .panel_mut(index)
                    .and_then(|panel| panel.reload(discard));
                if result.is_ok() {
                    self.clear_project_transients(index);
                }
                self.project_result(index, result, cx);
            }
            ProjectCommand::Remove => {
                if let Some(id) = selected {
                    let result = self
                        .projects
                        .model
                        .panel_mut(index)
                        .and_then(|panel| panel.remove(id));
                    self.project_result(index, result, cx);
                }
            }
            ProjectCommand::MoveUp | ProjectCommand::MoveDown => {
                if let Some(id) = selected {
                    let result = self.projects.model.panel_mut(index).and_then(|panel| {
                        panel.move_sibling(id, command == ProjectCommand::MoveUp)
                    });
                    self.project_result(index, result, cx);
                }
            }
            ProjectCommand::AddProject
            | ProjectCommand::AddGroup
            | ProjectCommand::Rename
            | ProjectCommand::ModifyPath
            | ProjectCommand::Find => {
                let edit = match (command, selected) {
                    (ProjectCommand::Find, _) => Some((ProjectEdit::Find, String::new())),
                    (ProjectCommand::AddProject, _) => {
                        Some((ProjectEdit::Project, "New project".to_owned()))
                    }
                    (ProjectCommand::AddGroup, Some(id)) => {
                        Some((ProjectEdit::Group(id), "New folder".to_owned()))
                    }
                    (ProjectCommand::Rename, Some(id)) => self
                        .projects
                        .model
                        .panel(index)
                        .ok()
                        .and_then(|panel| panel.node(id))
                        .map(|node| (ProjectEdit::Rename(id), node.name.clone())),
                    (ProjectCommand::ModifyPath, Some(id)) => self
                        .projects
                        .model
                        .panel(index)
                        .ok()
                        .and_then(|panel| panel.node(id))
                        .and_then(ProjectNode::file_path)
                        .map(|path| (ProjectEdit::FilePath(id), path.display().to_string())),
                    _ => None,
                };
                if let Some((edit, text)) = edit {
                    self.projects.panels[index].edit = Some(edit);
                    self.projects.panels[index].name.update(cx, |field, cx| {
                        field.set_text(&text, window, cx);
                        field.focus(window, cx);
                    });
                }
                cx.notify();
            }
        }
    }

    fn apply_project_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.apply_project_edit(index, window, cx)
        }) {
            return;
        }
        if self.project_operations_blocked() {
            return;
        }
        let Some(edit) = self.projects.panels[index].edit else {
            return;
        };
        if self.projects.panels[index].name.read(cx).has_composition() {
            return;
        }
        let name = self.projects.panels[index].name.read(cx).text();
        let result = self
            .projects
            .model
            .panel_mut(index)
            .and_then(|panel| match edit {
                ProjectEdit::Project => panel.add_project(&name).map(|_| ()),
                ProjectEdit::Group(parent) => panel.add_group(parent, &name).map(|_| ()),
                ProjectEdit::Rename(id) => panel.rename(id, &name),
                ProjectEdit::FilePath(id) => {
                    panel.modify_file_path(id, std::path::Path::new(&name))
                }
                ProjectEdit::Find => panel.find_text(&name, &self.workspace).map(|results| {
                    self.projects.panels[index].results = Some(results);
                }),
            });
        if result.is_ok() {
            self.projects.panels[index].edit = None;
            if matches!(edit, ProjectEdit::Project) {
                self.projects.panels[index].workspace_expanded = true;
            }
        }
        self.project_result(index, result, cx);
    }

    fn confirm_project_action(
        &mut self,
        index: usize,
        discard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.confirm_project_action(index, discard, window, cx)
        }) {
            return;
        }
        if self.project_operations_blocked() {
            return;
        }
        let Some((command, selected)) = self.projects.panels[index].confirm.take() else {
            return;
        };
        if index == 0 && let Some(path) = self.projects.pending_workspace_path.take() {
            if discard { self.load_project_workspace_path_guarded(path,true,window,cx); }
            else { self.finish_project_workspace_open(false); }
            cx.notify();
            return;
        }
        if discard {
            self.execute_project_command(index, command, selected, true, window, cx);
        }
        cx.notify();
    }

    fn choose_project_paths(
        &mut self,
        index: usize,
        command: ProjectCommand,
        parent: Option<NodeId>,
        discard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if command != ProjectCommand::Open && parent.is_none() {
            self.projects.panels[index].message = Some("Select a project or folder first".into());
            cx.notify();
            return;
        }
        self.projects.picker_pending = true;
        self.file_busy = true;
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: command != ProjectCommand::AddFolder,
            directories: command == ProjectCommand::AddFolder,
            multiple: command == ProjectCommand::AddFiles,
            prompt: Some(
                match command {
                    ProjectCommand::Open => "Open project workspace JSON",
                    ProjectCommand::AddFolder => "Add files from directory",
                    _ => "Add files to project",
                }
                .into(),
            ),
        });
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result = match prompt.await {
                Ok(Ok(paths)) => Ok(paths),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(format!(
                    "Project path picker disconnected: {error}"
                ))),
            };
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.projects.picker_pending = false;
                view.file_busy = false;
                let result = result.and_then(|paths| {
                    let Some(paths) = paths else {
                        return Ok(());
                    };
                    if paths.is_empty() || (command != ProjectCommand::AddFiles && paths.len() != 1)
                    {
                        return Err(io::Error::other("Choose exactly one workspace or folder"));
                    }
                    let panel = view.projects.model.panel_mut(index)?;
                    if command == ProjectCommand::Open {
                        panel.load(&paths[0], discard)?;
                        view.clear_project_transients(index);
                        return Ok(());
                    }
                    let parent = parent.ok_or_else(|| io::Error::other("No project selected"))?;
                    let mut candidate = panel.clone();
                    for path in paths {
                        crate::file_access::selected(&path)?;
                        if command == ProjectCommand::AddFolder {
                            candidate.add_folder(parent, &path)?;
                        } else {
                            candidate.add_file(parent, &path)?;
                        }
                    }
                    *panel = candidate;
                    Ok(())
                });
                view.project_result(index, result, cx);
            }) {
                eprintln!("Project path picker lost its document: {error}");
            }
        })
        .detach();
    }

    fn clear_project_transients(&mut self, index: usize) {
        self.projects.panels[index].edit = None;
        self.projects.panels[index].confirm = None;
        self.projects.panels[index].commands = None;
        self.projects.panels[index].workspace_expanded = true;
    }

    fn choose_project_save(
        &mut self,
        index: usize,
        command: ProjectCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self
            .projects
            .model
            .panel(index)
            .ok()
            .and_then(|panel| panel.path())
            .map(PathBuf::from);
        let directory = path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = path
            .as_deref()
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("workspace-{}.projects.json", index + 1));
        self.projects.picker_pending = true;
        self.file_busy = true;
        let prompt = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result = match prompt.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(format!(
                    "Project save picker disconnected: {error}"
                ))),
            };
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.projects.picker_pending = false;
                view.file_busy = false;
                let result = result.and_then(|path| {
                    let Some(path) = path else {
                        return Ok(());
                    };
                    let panel = view.projects.model.panel_mut(index)?;
                    if command == ProjectCommand::Export {
                        panel.export(&path)
                    } else {
                        panel.save_as(&path)
                    }
                });
                view.project_result(index, result, cx);
            }) {
                eprintln!("Project save picker lost its document: {error}");
            }
        })
        .detach();
    }

    fn activate_project_entry(
        &mut self,
        index: usize,
        id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.activate_project_entry(index, id, window, cx)
        }) {
            return;
        }
        if self.project_operations_blocked() {
            return;
        }
        let Ok(panel) = self.projects.model.panel_mut(index) else {
            return;
        };
        panel.selected = Some(id);
        let path = panel
            .node(id)
            .and_then(ProjectNode::file_path)
            .map(PathBuf::from);
        if let Some(path) = path {
            if let Some(buffer) = crate::folder_workspace::existing_buffer(&self.workspace, &path) {
                if self.activate_buffer(buffer, window, cx) {
                    self.projects.panels[index].message = None;
                }
            } else if self.cancel_view_composition(window, cx) {
                let result = self
                    .projects
                    .model
                    .panel(index)
                    .and_then(|panel| panel.open_file(id, &mut self.workspace));
                if result.is_ok() {
                    self.reset_active_view(window, cx);
                }
                self.project_result(index, result, cx);
            }
        } else {
            let result = panel.toggle_expanded(id);
            self.project_result(index, result, cx);
        }
        cx.notify();
    }

    fn activate_project_match(
        &mut self,
        index: usize,
        hit: ProjectMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = hit.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.activate_project_match(index, routed, window, cx)
        }) {
            return;
        }
        if self.project_operations_blocked() {
            return;
        }
        self.activate_project_entry(index, hit.node, window, cx);
        let active = crate::folder_workspace::existing_buffer(&self.workspace, &hit.path);
        if active != Some(self.workspace.active().id()) {
            return;
        }
        if self.workspace.active().editor.committed_text() != hit.searched_text.as_ref() {
            self.projects.panels[index].message =
                Some("Document changed since search; search again before selecting a match".into());
            cx.notify();
            return;
        }
        let result = self
            .workspace
            .active_mut()
            .editor
            .select(Selection {
                anchor: ByteOffset(hit.range.start),
                head: ByteOffset(hit.range.end),
            })
            .map_err(|error| io::Error::other(error.to_string()));
        if result.is_ok() {
            self.reset_active_view(window, cx);
        }
        self.project_result(index, result, cx);
    }

    fn drop_project_entry(
        &mut self,
        index: usize,
        target: NodeId,
        drag: &ProjectDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = drag.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.drop_project_entry(index, target, &routed, window, cx)
        }) {
            return;
        }
        if self.project_operations_blocked() || drag.panel != index {
            return;
        }
        let result = self
            .projects
            .model
            .panel_mut(index)
            .and_then(|panel| panel.drop_entry(drag.node, target));
        self.project_result(index, result, cx);
    }

    pub(super) fn render_project_panel(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.projects.model.panel(index).ok()?;
        if !panel.visible {
            return None;
        }
        let surface = &self.projects.panels[index];
        let dark = self.dark;
        let mut rows = Vec::new();
        fn flatten<'a>(
            nodes: &'a [ProjectNode],
            depth: usize,
            rows: &mut Vec<(&'a ProjectNode, usize)>,
        ) {
            for node in nodes {
                rows.push((node, depth));
                if node.expanded {
                    flatten(&node.children, depth + 1, rows);
                }
            }
        }
        if surface.workspace_expanded {
            flatten(panel.roots(), 1, &mut rows);
        }
        let title = format!("Project Panel {}", index + 1);
        let icon_style = if self.pref_string("ToolBar.iconSet", if dark { "small" } else { "standard" }) == "standard" {
            "standard"
        } else if dark { "dark" } else { "light" };
        let workspace_name = panel
            .path()
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workspace".into());
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let measure = |text: &str| text_system.shape_line(
            text.to_owned().into(), px(12.), &[gpui::TextRun {
                len: text.len(), font: gpui::font(dialog_font::font_family()),
                color: rgb(if dark { 0xd4d4d4 } else { 0x202020 }).into(),
                background_color: None, underline: None, strikethrough: None,
            }], None,
        ).width().ceil();
        let (workspace_button_width, content_width) = {
            let mut widths = surface.widths.borrow_mut();
            let font = (dialog_font::font_family(), px(12.));
            if widths.font.as_ref() != Some(&font) {
                *widths = ProjectWidths { font: Some(font), ..ProjectWidths::default() };
            }
            let workspace_button_width = *widths.toolbar.get_or_insert_with(|| measure("Workspace")) + px(14.);
            if widths.workspace.as_ref().is_none_or(|(name, _)| name != &workspace_name) {
                widths.workspace = Some((workspace_name.clone(), measure(&workspace_name)));
            }
            let mut content_width = widths.workspace.as_ref().map_or(px(0.), |(_, width)| *width) + px(43.);
            let visible: std::collections::BTreeSet<_> = rows.iter().map(|(node, _)| node.id).collect();
            widths.labels.retain(|id, _| visible.contains(id));
            for (node, depth) in &rows {
                let entry = widths.labels.entry(node.id).or_insert_with(|| (node.name.clone(), measure(&node.name)));
                if entry.0 != node.name {
                    *entry = (node.name.clone(), measure(&node.name));
                }
                content_width = content_width.max(entry.1 + px(43. + *depth as f32 * 19.));
            }
            (workspace_button_width, content_width)
        };
        let mut body =
            div()
                .id(("project-panel", index))
                .debug_selector(move || format!("project-panel-{index}"))
                .on_drop(cx.listener(|view, paths: &gpui::ExternalPaths, window, cx| {
                    view.preferences_drop_paths(paths.paths().to_vec(), window, cx);
                    cx.stop_propagation();
                }))
                .role(Role::Group)
                .aria_label(title.clone())
                .relative()
                .w_full()
                .h_full()
                .min_h(px(0.))
                .min_w(px(0.))
                .flex()
                .flex_col()
                .bg(rgb(if dark { 0x282828 } else { 0xf6f6f6 }))
                .text_color(rgb(if dark { 0xd4d4d4 } else { 0x202020 }))
                .font_family(dialog_font::font_family())
                .text_size(px(12.))
                .child(
                    div()
                        .id(("project-toolbar", index))
                        .h(px(22.))
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .child(project_button(index, "workspace", "Workspace")
                            .relative()
                            .child(project_toolbar_bounds(surface.toolbar_bounds[0].clone()))
                            .w(workspace_button_width).flex_shrink_0()
                            .h_full().px(px(7.)).py(px(0.))
                            .flex().items_center()
                            .hover(|style| style.bg(rgb(if dark { 0x404040 } else { 0xe5f1fb })))
                            .on_click(
                            cx.listener(move |view, _, window, cx| {
                                let menu = &mut view.projects.panels[index].commands;
                                *menu = if *menu == Some(ProjectMenu::Workspace) { None } else { Some(ProjectMenu::Workspace) };
                                view.projects.panels[index].tree_focus.focus(window, cx);
                                cx.notify();
                            }),
                        ))
                        .child(
                            project_button(index, "edit", "Edit")
                                .relative()
                                .child(project_toolbar_bounds(surface.toolbar_bounds[1].clone()))
                                .h_full().px(px(7.)).py(px(0.))
                                .flex().items_center()
                                .hover(|style| style.bg(rgb(if dark { 0x404040 } else { 0xe5f1fb })))
                                .on_click(cx.listener(
                                move |view, _, window, cx| {
                                    let selected = view.projects.model.panel(index).is_ok_and(|panel| panel.selected.is_some());
                                    let menu = &mut view.projects.panels[index].commands;
                                    *menu = if selected && *menu != Some(ProjectMenu::Edit) { Some(ProjectMenu::Edit) } else { None };
                                    view.projects.panels[index].tree_focus.focus(window, cx);
                                    cx.notify();
                                },
                            )),
                        ),
                );
        let command_menu = surface.commands.map(|menu| {
            let selected = panel.selected.and_then(|id| panel.node(id));
            let mut commands = if menu == ProjectMenu::Workspace { vec![
                (ProjectCommand::New, "New Workspace"),
                (ProjectCommand::Open, "Open Workspace"),
                (ProjectCommand::Reload, "Reload Workspace"),
                (ProjectCommand::Save, "Save"),
                (ProjectCommand::SaveAs, "Save As…"),
                (ProjectCommand::Export, "Save a Copy As…"),
                (ProjectCommand::AddProject, "Add New Project"),
                (ProjectCommand::Find, "Find in Projects…"),
            ] } else { Vec::new() };
            if menu == ProjectMenu::Edit && let Some(selected) = selected {
                commands.extend([
                    (ProjectCommand::MoveUp, "Move Up"),
                    (ProjectCommand::MoveDown, "Move Down"),
                    (ProjectCommand::Rename, "Rename…"),
                ]);
                if matches!(selected.kind, NodeKind::File(_)) {
                    commands.push((ProjectCommand::Remove, "Remove Entry…"));
                    commands.push((ProjectCommand::ModifyPath, "Modify File Path…"));
                } else {
                    commands.extend([
                        (ProjectCommand::AddGroup, "Add Folder…"),
                        (ProjectCommand::AddFiles, "Add Files…"),
                        (ProjectCommand::AddFolder, "Add Files from Directory…"),
                        (ProjectCommand::Remove, "Remove Entry…"),
                    ]);
                }
            }
            let toolbar_bounds = surface.toolbar_bounds[if menu == ProjectMenu::Workspace { 0 } else { 1 }].clone();
            let popup = div()
                    .id(("project-commands", index))
                    .overflow_y_scroll()
                    .occlude()
                    .border_1().border_color(rgb(if dark { 0x606060 } else { 0xa0a0a0 }))
                    .bg(rgb(if dark { 0x282828 } else { 0xf0f0f0 }))
                    .py(px(2.))
                    .children(commands.into_iter().enumerate().map(
                        |(position, (command, label))| {
                            let separator = if menu == ProjectMenu::Workspace {
                                matches!(command, ProjectCommand::AddProject | ProjectCommand::Find)
                            } else { command == ProjectCommand::Rename };
                            div()
                                .when(separator, |row| row.child(div().h(px(1.)).mx(px(3.)).my(px(3.))
                                    .bg(rgb(if dark { 0x606060 } else { 0xa0a0a0 }))))
                                .child(
                            project_button(index, format!("command-{position}"), label)
                                .w_full()
                                .h(px(22.))
                                .px(px(12.)).py(px(0.))
                                .flex().items_center()
                                .hover(|style| style.bg(rgb(if dark { 0x404b5a } else { 0xcce8ff })))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.project_command(index, command, window, cx)
                                })))
                        },
                    ));
            gpui::deferred(gpui::canvas(
                move |_, window, cx| {
                    let viewport = window.viewport_size();
                    let inset = window.client_inset().unwrap_or(px(0.));
                    let available = size((viewport.width - inset * 2.).max(px(0.)), (viewport.height - inset * 2.).max(px(0.)));
                    let bounds = toolbar_bounds.get();
                    let mut popup = gpui::anchored()
                        .position(gpui::point(bounds.left(), bounds.bottom()))
                        .snap_to_window()
                        .child(popup.w(px(224.).min(available.width))
                            .min_w(px(0.)).max_w(available.width).min_h(px(0.))
                            .max_h(px(300.).min(available.height)))
                        .into_any_element();
                    popup.prepaint_as_root(gpui::Point::default(), size(
                        gpui::AvailableSpace::Definite(viewport.width),
                        gpui::AvailableSpace::Definite(viewport.height),
                    ), window, cx);
                    popup
                },
                |_, mut popup, window, cx| popup.paint(window, cx),
            ).absolute()).with_priority(1)
        });
        if let Some((command, _)) = surface.confirm {
            body = body.child(div().px(px(6.)).py(px(4.)).flex_shrink_0().child(if command == ProjectCommand::Remove { "Remove entry and its children from the project? Files on disk will not be deleted." } else { "Discard unsaved workspace changes? Save Workspace first to keep them." })
                .child(div().flex().gap(px(5.)).child(project_button(index, "confirm", if command == ProjectCommand::Remove { "Remove Entry" } else { "Discard Changes" }).on_click(cx.listener(move |view, _, window, cx| view.confirm_project_action(index, true, window, cx))))
                .child(project_button(index, "cancel-confirm", "Cancel").on_click(cx.listener(move |view, _, window, cx| view.confirm_project_action(index, false, window, cx))))));
        }
        if surface.edit.is_some() {
            body =
                body.child(
                    div()
                        .px(px(6.))
                        .py(px(4.))
                        .flex_shrink_0()
                        .when(matches!(surface.edit, Some(ProjectEdit::Find)), |row| {
                            row.child("Find literal text (case-sensitive)")
                        })
                        .child(surface.name.clone())
                        .child(
                            div()
                                .flex()
                                .gap(px(5.))
                                .child(project_button(index, "apply", "Apply").on_click(
                                    cx.listener(move |view, _, window, cx| {
                                        view.apply_project_edit(index, window, cx)
                                    }),
                                ))
                                .child(project_button(index, "cancel-edit", "Cancel").on_click(
                                    cx.listener(move |view, _, _, cx| {
                                        view.projects.panels[index].edit = None;
                                        cx.notify();
                                    }),
                                )),
                        ),
                );
        }
        if let Some(results) = &surface.results {
            body = body.child(
                div()
                    .id(("project-search-results", index))
                    .max_h(px(180.))
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .child(format!("{} matches", results.matches.len()))
                    .children(results.matches.iter().enumerate().map(|(position, hit)| {
                        let hit = hit.clone();
                        div()
                            .id(format!("project-hit-{index}-{position}"))
                            .role(Role::Button)
                            .cursor_pointer()
                            .px(px(6.))
                            .child(format!(
                                "{}:{}: {}",
                                hit.path.display(),
                                hit.line,
                                hit.preview
                            ))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.activate_project_match(index, hit.clone(), window, cx)
                            }))
                    }))
                    .children(results.errors.iter().map(|error| {
                        div()
                            .px(px(6.))
                            .text_color(rgb(if dark { 0xffb0a0 } else { 0xa02020 }))
                            .child(error.clone())
                    })),
            );
        }
        if let Some(message) = &surface.message {
            body = body.child(
                div()
                    .px(px(6.))
                    .flex_shrink_0()
                    .text_color(rgb(if dark { 0xffb0a0 } else { 0xa02020 }))
                    .child(message.clone()),
            );
        }
        if self.projects.picker_pending {
            body = body.child(div().px(px(6.)).child("Choosing project path…"));
        }
        Some(
            body.child(
                div()
                    .id(("project-tree", index))
                    .role(Role::Tree)
                    .aria_label("Project entries")
                    .track_focus(&surface.tree_focus)
                    .tab_index(0)
                    .on_key_down(cx.listener(
                        move |view, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" && view.projects.panels[index].commands.is_some() {
                                view.projects.panels[index].commands = None;
                                cx.notify();
                                cx.stop_propagation();
                                return;
                            }
                            if event.keystroke.key != "enter" {
                                return;
                            }
                            if let Some(id) = view
                                .projects
                                .model
                                .panel(index)
                                .ok()
                                .and_then(|panel| panel.selected)
                            {
                                view.activate_project_entry(index, id, window, cx);
                            } else if !view.project_operations_blocked() {
                                view.projects.panels[index].workspace_expanded =
                                    !view.projects.panels[index].workspace_expanded;
                                cx.notify();
                            }
                            cx.stop_propagation();
                        },
                    ))
                    .flex_1()
                    .min_h(px(0.))
                    .min_w(px(0.))
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .mt(px(2.))
                    .border_1()
                    .border_color(rgb(if dark { 0x606060 } else { 0x808080 }))
                    .bg(rgb(if dark { 0x202020 } else { 0xffffff }))
                    .child(
                        div()
                            .id(("project-workspace-root", index))
                            .debug_selector(move || format!("project-workspace-root-{index}"))
                            .role(Role::TreeItem)
                            .aria_label(workspace_name.clone())
                            .aria_selected(panel.selected.is_none())
                            .h(px(18.))
                            .min_w(content_width)
                            .flex_shrink_0()
                            .flex().items_center()
                            .whitespace_nowrap()
                            .cursor_pointer()
                            .tooltip({
                                let tooltip = panel.path().map(|path| path.display().to_string()).unwrap_or_else(|| workspace_name.clone());
                                move |_, cx| icon_tooltip::view(tooltip.clone(), dark, cx)
                            })
                            .child(project_expander(index, "workspace", surface.workspace_expanded, !panel.roots().is_empty(), dark)
                                .when(!panel.roots().is_empty(), |expander| expander.on_click(cx.listener(move |view, _, window, cx| {
                                    if !view.project_operations_blocked() {
                                        view.projects.panels[index].workspace_expanded = !view.projects.panels[index].workspace_expanded;
                                        if let Ok(panel) = view.projects.model.panel_mut(index) { panel.selected = None; }
                                        view.projects.panels[index].tree_focus.focus(window, cx);
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                }))))
                            .child(gpui::img(project_image(if panel.dirty() { "workspace-dirty" } else { "workspace" }, icon_style))
                                .size(px(16.)).mr(px(2.)).flex_shrink_0())
                            .child(div().px(px(2.)).h_full().flex_shrink_0().flex().items_center()
                                .when(panel.selected.is_none(), |label| label.bg(rgb(if dark { 0x404b5a } else { 0xcce8ff })))
                                .child(workspace_name))
                            .on_mouse_down(MouseButton::Left, cx.listener(move |view, _, window, cx| {
                                if !view.project_operations_blocked() {
                                    view.projects.panels[index].tree_focus.focus(window, cx);
                                }
                                cx.stop_propagation();
                            }))
                            .on_mouse_down(MouseButton::Right, cx.listener(move |view, _, _, cx| {
                                if !view.project_operations_blocked() {
                                    if let Ok(panel) = view.projects.model.panel_mut(index) { panel.selected = None; }
                                    view.projects.panels[index].commands = Some(ProjectMenu::Workspace);
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }))
                            .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                                if !view.project_operations_blocked() {
                                    if let Ok(panel) = view.projects.model.panel_mut(index) { panel.selected = None; }
                                    view.projects.panels[index].commands = None;
                                    if event.click_count() == 2 {
                                        view.projects.panels[index].workspace_expanded = !view.projects.panels[index].workspace_expanded;
                                    }
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }))
                    )
                    .children(rows.into_iter().map(|(node, depth)| {
                        let id = node.id;
                        let icon = match node.kind {
                            NodeKind::Project => "project",
                            NodeKind::Group if node.expanded => "folder-open",
                            NodeKind::Group => "folder-close",
                            NodeKind::File(_) if node.is_missing() => "file-invalid",
                            NodeKind::File(_) => "file",
                        };
                        let label = node.name.clone();
                        let tooltip = node
                            .file_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| node.name.clone());
                        let drag = ProjectDrag {
                            panel: index,
                            node: id,
                            label: node.name.clone(),
                        };
                        div()
                            .id(format!("project-entry-{index}-{id}"))
                            .debug_selector(move || format!("project-entry-{index}-{id}"))
                            .role(Role::TreeItem)
                            .aria_label(format!("{label}: {tooltip}"))
                            .aria_selected(panel.selected == Some(id))
                            .h(px(18.))
                            .min_w(content_width)
                            .flex_shrink_0()
                            .pl(px(depth as f32 * 19.))
                            .pr(px(2.))
                            .flex().items_center()
                            .cursor_pointer()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .tooltip(move |_, cx| icon_tooltip::view(tooltip.clone(), dark, cx))
                            .child(project_expander(index, format!("entry-{id}"), node.expanded, !node.children.is_empty(), dark)
                                .when(!node.children.is_empty(), |expander| expander.on_click(cx.listener(move |view, _, window, cx| {
                                    view.activate_project_entry(index, id, window, cx);
                                    cx.stop_propagation();
                                }))))
                            .child(gpui::img(project_image(icon, icon_style))
                                .size(px(16.)).mr(px(2.)).flex_shrink_0())
                            .child(div().px(px(2.)).h_full().flex_shrink_0().flex().items_center()
                                .when(panel.selected == Some(id), |label| label.bg(rgb(if dark { 0x404b5a } else { 0xcce8ff })))
                                .hover(|label| label.bg(rgb(if dark { 0x404040 } else { 0xe0e8f4 })))
                                .child(label))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _, window, cx| {
                                    if !view.project_operations_blocked() {
                                        view.projects.panels[index].tree_focus.focus(window, cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |view, _, _, cx| {
                                    if let Ok(panel) = view.projects.model.panel_mut(index) {
                                        panel.selected = Some(id);
                                    }
                                    view.projects.panels[index].commands = Some(ProjectMenu::Edit);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .on_click(cx.listener(
                                move |view, event: &gpui::ClickEvent, window, cx| {
                                    if !view.project_operations_blocked() {
                                         if let Ok(panel) = view.projects.model.panel_mut(index) {
                                             panel.selected = Some(id);
                                         }
                                         view.projects.panels[index].commands = None;
                                        if event.click_count() == 2 {
                                            view.activate_project_entry(index, id, window, cx);
                                        }
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                },
                            ))
                            .when(!matches!(node.kind, NodeKind::Project), |row| {
                                row.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                            })
                            .on_drop(cx.listener(move |view, drag: &ProjectDrag, window, cx| {
                                view.drop_project_entry(index, id, drag, window, cx)
                            }))
                    })),
            )
            .children(command_menu)
            .into_any_element(),
        )
    }
}

fn project_toolbar_bounds(bounds: Rc<std::cell::Cell<Bounds<gpui::Pixels>>>) -> impl IntoElement {
    gpui::canvas(move |measured, _, _| bounds.set(measured), |_, _, _, _| {})
        .absolute().left_0().top_0().size_full()
}

fn project_expander(index: usize, id: impl Into<String>, expanded: bool, has_children: bool, dark: bool) -> gpui::Stateful<gpui::Div> {
    div().id(format!("project-expander-{index}-{}", id.into()))
        .w(px(19.)).h_full().flex_shrink_0()
        .flex().items_center().justify_center()
        .when(has_children, |expander| expander.child(gpui::canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let origin = bounds.origin;
                let mut path = gpui::PathBuilder::fill();
                let points = if expanded { [(1., 3.), (7., 3.), (4., 6.)] } else { [(3., 1.), (6., 4.), (3., 7.)] };
                path.move_to(origin + gpui::point(px(points[0].0), px(points[0].1)));
                for (x, y) in &points[1..] {
                    path.line_to(origin + gpui::point(px(*x), px(*y)));
                }
                path.close();
                match path.build() {
                    Ok(path) => window.paint_path(path, rgb(if dark { 0xd4d4d4 } else { 0x505050 })),
                    Err(error) => eprintln!("Could not paint project expander: {error}"),
                }
            },
        ).size(px(9.))))
}

fn project_image(name: &str, style: &str) -> std::sync::Arc<gpui::Image> {
    macro_rules! image {
        ($style:literal, $name:literal) => {{
            static IMAGE: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
            IMAGE.get_or_init(|| std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
                include_bytes!(concat!("../../assets/ui/project/", $style, "/", $name, ".svg")).to_vec(),
            ))).clone()
        }};
    }
    macro_rules! images {
        ($style:literal) => {
            match name {
                "workspace" => image!($style, "workspace"),
                "workspace-dirty" => image!($style, "workspace-dirty"),
                "project" => image!($style, "project"),
                "folder-open" => image!($style, "folder-open"),
                "folder-close" => image!($style, "folder-close"),
                "file-invalid" => image!($style, "file-invalid"),
                _ => image!($style, "file"),
            }
        };
    }
    match style {
        "dark" => images!("dark"),
        "light" => images!("light"),
        _ => images!("standard"),
    }
}

fn project_button(
    index: usize,
    id: impl Into<String>,
    label: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(format!("project-button-{index}-{}", id.into()))
        .role(Role::Button)
        .aria_label(label)
        .px(px(5.))
        .py(px(3.))
        .cursor_pointer()
        .child(label)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}
