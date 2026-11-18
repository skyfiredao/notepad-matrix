use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

pub type NodeId = u64;

#[derive(Clone, Debug)]
pub struct ProjectMatch {
    pub node: NodeId,
    pub path: PathBuf,
    pub line: usize,
    pub range: std::ops::Range<usize>,
    pub preview: String,
    pub searched_text: std::sync::Arc<str>,
}

#[derive(Default)]
pub struct ProjectSearchResults {
    pub matches: Vec<ProjectMatch>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Project,
    Group,
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub children: Vec<ProjectNode>,
    pub expanded: bool,
}

impl ProjectNode {
    pub fn file_path(&self) -> Option<&Path> {
        match &self.kind {
            NodeKind::File(path) => Some(path),
            _ => None,
        }
    }

    pub fn is_missing(&self) -> bool {
        self.file_path().is_some_and(|path| !path.is_file())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectWorkspace {
    roots: Vec<ProjectNode>,
    path: Option<PathBuf>,
    dirty: bool,
    next_id: NodeId,
    pub visible: bool,
    pub selected: Option<NodeId>,
}

impl Default for ProjectWorkspace {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            path: None,
            dirty: false,
            next_id: 1,
            visible: false,
            selected: None,
        }
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn valid_name(name: &str) -> io::Result<()> {
    if name.trim().is_empty() || name.contains(['\0', '\n', '\r']) {
        return Err(invalid("Enter a nonempty single-line name"));
    }
    Ok(())
}

fn find(nodes: &[ProjectNode], id: NodeId) -> Option<&ProjectNode> {
    nodes.iter().find_map(|node| {
        if node.id == id {
            Some(node)
        } else {
            find(&node.children, id)
        }
    })
}

fn find_mut(nodes: &mut [ProjectNode], id: NodeId) -> Option<&mut ProjectNode> {
    nodes.iter_mut().find_map(|node| {
        if node.id == id {
            Some(node)
        } else {
            find_mut(&mut node.children, id)
        }
    })
}

impl ProjectWorkspace {
    pub fn roots(&self) -> &[ProjectNode] {
        &self.roots
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    pub fn node(&self, id: NodeId) -> Option<&ProjectNode> {
        find(&self.roots, id)
    }

    fn insert(&mut self, parent: Option<NodeId>, name: &str, kind: NodeKind) -> io::Result<NodeId> {
        valid_name(name)?;
        if let Some(parent) = parent {
            let node = self.node(parent).ok_or_else(|| invalid("Unknown parent"))?;
            if matches!(node.kind, NodeKind::File(_)) {
                return Err(invalid("Files cannot contain entries"));
            }
        } else if kind != NodeKind::Project {
            return Err(invalid("Only projects belong at the workspace root"));
        }
        let id = self.next_id;
        let next_id = id
            .checked_add(1)
            .ok_or_else(|| invalid("Too many project entries"))?;
        let node = ProjectNode {
            id,
            name: name.into(),
            kind,
            children: Vec::new(),
            expanded: true,
        };
        if let Some(parent) = parent {
            let target =
                find_mut(&mut self.roots, parent).ok_or_else(|| invalid("Unknown parent"))?;
            target.expanded = true;
            target.children.push(node);
        } else {
            self.roots.push(node);
        }
        self.next_id = next_id;
        self.dirty = true;
        self.selected = Some(id);
        Ok(id)
    }

    pub fn add_project(&mut self, name: &str) -> io::Result<NodeId> {
        self.insert(None, name, NodeKind::Project)
    }

    pub fn add_group(&mut self, parent: NodeId, name: &str) -> io::Result<NodeId> {
        self.insert(Some(parent), name, NodeKind::Group)
    }

    pub fn add_file(&mut self, parent: NodeId, path: &Path) -> io::Result<NodeId> {
        let path = self.resolve_file_path(path)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("File path has no UTF-8 filename"))?
            .to_owned();
        self.insert(Some(parent), &name, NodeKind::File(path))
    }

    pub fn add_folder(&mut self, parent: NodeId, path: &Path) -> io::Result<NodeId> {
        let _access = crate::file_access::directory(path)?;
        fn import(
            panel: &mut ProjectWorkspace,
            parent: NodeId,
            path: &Path,
            depth: usize,
        ) -> io::Result<NodeId> {
            if depth > 128 {
                return Err(invalid("Folder tree is too deep"));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid("Directory has no UTF-8 name"))?;
            let group = panel.add_group(parent, name)?;
            let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    import(panel, group, &entry.path(), depth + 1)?;
                } else if kind.is_file() || (kind.is_symlink() && entry.path().is_file()) {
                    panel.add_file(group, &entry.path())?;
                }
            }
            Ok(group)
        }
        let mut candidate = self.clone();
        let id = import(&mut candidate, parent, &self.resolve_file_path(path)?, 0)?;
        candidate.selected = Some(id);
        *self = candidate;
        Ok(id)
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        fn parent(nodes: &[ProjectNode], id: NodeId) -> Option<NodeId> {
            nodes.iter().find_map(|node| {
                if node.children.iter().any(|child| child.id == id) {
                    Some(node.id)
                } else {
                    parent(&node.children, id)
                }
            })
        }
        parent(&self.roots, id)
    }

    pub fn move_entry(
        &mut self,
        id: NodeId,
        parent: Option<NodeId>,
        position: usize,
    ) -> io::Result<()> {
        let source = self
            .node(id)
            .ok_or_else(|| invalid("Unknown drag source"))?;
        match (&source.kind, parent) {
            (NodeKind::Project, Some(_)) => {
                return Err(invalid("Projects can only move at the workspace root"));
            }
            (NodeKind::Project, None) => {}
            (_, None) => return Err(invalid("Groups and files must belong to a project")),
            (_, Some(target)) => {
                if target == id || find(&source.children, target).is_some() {
                    return Err(invalid("Cannot move a group into itself or a descendant"));
                }
                if self
                    .node(target)
                    .is_none_or(|node| matches!(node.kind, NodeKind::File(_)))
                {
                    return Err(invalid("Drop target must be a project or group"));
                }
            }
        }
        let selected = self.selected;
        let node = self.remove(id)?;
        let destination = if let Some(parent) = parent {
            let target =
                find_mut(&mut self.roots, parent).ok_or_else(|| invalid("Unknown target"))?;
            target.expanded = true;
            &mut target.children
        } else {
            &mut self.roots
        };
        destination.insert(position.min(destination.len()), node);
        self.selected = selected;
        Ok(())
    }

    pub fn drop_entry(&mut self, id: NodeId, target: NodeId) -> io::Result<()> {
        let source = self
            .node(id)
            .ok_or_else(|| invalid("Unknown drag source"))?;
        if matches!(source.kind, NodeKind::Project) {
            return Err(invalid("Use Move Up or Move Down to reorder projects"));
        }
        if self.parent(id) == Some(target) {
            return Err(invalid("Entry already belongs to that project or folder"));
        }
        let target_node = self
            .node(target)
            .ok_or_else(|| invalid("Unknown drop target"))?;
        if matches!(target_node.kind, NodeKind::File(_)) {
            return Err(invalid("Files cannot be drop targets"));
        }
        self.move_entry(id, Some(target), target_node.children.len())
    }

    pub fn move_sibling(&mut self, id: NodeId, up: bool) -> io::Result<bool> {
        let parent = self.parent(id);
        let siblings = match parent {
            Some(parent) => {
                &self
                    .node(parent)
                    .ok_or_else(|| invalid("Unknown parent"))?
                    .children
            }
            None => &self.roots,
        };
        let position = siblings
            .iter()
            .position(|node| node.id == id)
            .ok_or_else(|| invalid("Unknown entry"))?;
        let target = if up {
            position.checked_sub(1)
        } else {
            position
                .checked_add(1)
                .filter(|next| *next < siblings.len())
        };
        let Some(target) = target else {
            return Ok(false);
        };
        self.move_entry(id, parent, target)?;
        Ok(true)
    }

    pub fn toggle_expanded(&mut self, id: NodeId) -> io::Result<()> {
        let node = find_mut(&mut self.roots, id).ok_or_else(|| invalid("Unknown entry"))?;
        if matches!(node.kind, NodeKind::File(_)) {
            return Err(invalid("Files have no children"));
        }
        node.expanded = !node.expanded;
        Ok(())
    }

    pub fn modify_file_path(&mut self, id: NodeId, path: &Path) -> io::Result<()> {
        let path = self.resolve_file_path(path)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("File path has no UTF-8 filename"))?
            .to_owned();
        let node = find_mut(&mut self.roots, id)
            .filter(|node| matches!(node.kind, NodeKind::File(_)))
            .ok_or_else(|| invalid("Select a file entry"))?;
        node.name = name;
        node.kind = NodeKind::File(path);
        self.dirty = true;
        Ok(())
    }

    fn resolve_file_path(&self, path: &Path) -> io::Result<PathBuf> {
        if path.as_os_str().is_empty() {
            return Err(invalid("Empty path"));
        }
        if path.is_absolute() {
            return absolute(path);
        }
        match self.path.as_deref().and_then(Path::parent) {
            Some(base) => absolute(&base.join(path)),
            None => absolute(path),
        }
    }

    pub fn files(&self) -> Vec<PathBuf> {
        fn collect(nodes: &[ProjectNode], result: &mut Vec<PathBuf>) {
            for node in nodes {
                if let Some(path) = node.file_path() {
                    result.push(path.to_owned());
                }
                collect(&node.children, result);
            }
        }
        let mut result = Vec::new();
        collect(&self.roots, &mut result);
        result
    }

    pub fn find_text(
        &self,
        query: &str,
        workspace: &crate::workspace::Workspace,
    ) -> io::Result<ProjectSearchResults> {
        if query.is_empty() {
            return Err(invalid("Enter text to find in projects"));
        }
        fn entries<'a>(nodes: &'a [ProjectNode], files: &mut Vec<&'a ProjectNode>) {
            for node in nodes {
                if matches!(node.kind, NodeKind::File(_)) {
                    files.push(node);
                }
                entries(&node.children, files);
            }
        }
        let mut files = Vec::new();
        entries(&self.roots, &mut files);
        let mut result = ProjectSearchResults::default();
        let mut searched = std::collections::BTreeSet::new();
        for node in files {
            let Some(path) = node.file_path() else {
                continue;
            };
            let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
            if !searched.insert(key) {
                continue;
            }
            let open = crate::folder_workspace::existing_buffer(workspace, path)
                .and_then(|id| workspace.buffers().iter().find(|buffer| buffer.id() == id));
            let text = if let Some(buffer) = open {
                Ok(buffer.editor.committed_text())
            } else {
                crate::files::DiskFile::load_unicode(path).map(|(_, text)| text)
            };
            let text: std::sync::Arc<str> = match text {
                Ok(text) => text.into(),
                Err(error) => {
                    result.errors.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            for (start, matched) in text.match_indices(query) {
                let line = text[..start].bytes().filter(|byte| *byte == b'\n').count() + 1;
                let line_start = text[..start].rfind('\n').map_or(0, |position| position + 1);
                let line_end = text[start..]
                    .find('\n')
                    .map_or(text.len(), |position| start + position);
                result.matches.push(ProjectMatch {
                    node: node.id,
                    path: path.to_owned(),
                    line,
                    range: start..start + matched.len(),
                    preview: text[line_start..line_end].trim_end_matches('\r').into(),
                    searched_text: text.clone(),
                });
            }
        }
        Ok(result)
    }

    pub fn open_file(
        &self,
        id: NodeId,
        workspace: &mut crate::workspace::Workspace,
    ) -> io::Result<crate::workspace::BufferId> {
        let path = self
            .node(id)
            .and_then(ProjectNode::file_path)
            .ok_or_else(|| invalid("Select a file entry"))?;
        if let Some(id) = crate::folder_workspace::existing_buffer(workspace, path) {
            workspace
                .activate(id)
                .map_err(|error| io::Error::other(error.to_string()))?;
            return Ok(id);
        }
        crate::folder_workspace::FolderFile::load(path)?
            .apply(workspace)
            .map_err(|error| io::Error::other(error.to_string()))
    }

    pub fn rename(&mut self, id: NodeId, name: &str) -> io::Result<()> {
        valid_name(name)?;
        let node = find_mut(&mut self.roots, id).ok_or_else(|| invalid("Unknown entry"))?;
        if let NodeKind::File(path) = &mut node.kind {
            if name.contains(['/', '\\']) || name == "." || name == ".." {
                return Err(invalid("A filename cannot contain path separators"));
            }
            path.set_file_name(name);
        }
        node.name = name.into();
        self.dirty = true;
        Ok(())
    }

    pub fn remove(&mut self, id: NodeId) -> io::Result<ProjectNode> {
        fn take(nodes: &mut Vec<ProjectNode>, id: NodeId) -> Option<ProjectNode> {
            if let Some(index) = nodes.iter().position(|node| node.id == id) {
                return Some(nodes.remove(index));
            }
            nodes
                .iter_mut()
                .find_map(|node| take(&mut node.children, id))
        }
        let removed = take(&mut self.roots, id).ok_or_else(|| invalid("Unknown entry"))?;
        if self
            .selected
            .is_some_and(|selected| selected == id || find(&removed.children, selected).is_some())
        {
            self.selected = None;
        }
        self.dirty = true;
        Ok(removed)
    }

    pub fn clear(&mut self, discard_changes: bool) -> io::Result<()> {
        self.allow_replace(discard_changes)?;
        let visible = self.visible;
        *self = Self {
            visible,
            ..Self::default()
        };
        Ok(())
    }

    fn allow_replace(&self, discard_changes: bool) -> io::Result<()> {
        if self.dirty && !discard_changes {
            return Err(invalid("Workspace has unsaved changes"));
        }
        Ok(())
    }

    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| invalid("Choose a workspace save path first"))?;
        self.save_as(&path)
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        let path = absolute(path)?;
        self.export(&path)?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    pub fn export(&self, path: &Path) -> io::Result<()> {
        let _access = crate::file_access::directory(path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
        let path = absolute(path)?;
        let base = path
            .parent()
            .ok_or_else(|| invalid("Workspace path has no parent"))?;
        let value = self.encode(base)?;
        let mut temporary = tempfile::NamedTempFile::new_in(base)?;
        serde_json::to_writer_pretty(temporary.as_file_mut(), &value)?;
        temporary.write_all(b"\n")?;
        temporary.as_file_mut().sync_all()?;
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn load(&mut self, path: &Path, discard_changes: bool) -> io::Result<()> {
        self.allow_replace(discard_changes)?;
        let _access = crate::file_access::read(path)?;
        let path = absolute(path)?;
        let input = std::fs::read(&path)?;
        if input.len() > 16 * 1024 * 1024 {
            return Err(invalid("Workspace exceeds 16 MiB"));
        }
        let value: Value = serde_json::from_slice(&input)?;
        let base = path
            .parent()
            .ok_or_else(|| invalid("Workspace path has no parent"))?;
        let mut loaded = Self::decode(&value, base)?;
        loaded.path = Some(path);
        loaded.visible = self.visible;
        *self = loaded;
        Ok(())
    }

    pub fn reload(&mut self, discard_changes: bool) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| invalid("Workspace has not been saved"))?;
        self.load(&path, discard_changes)
    }

    fn encode(&self, base: &Path) -> io::Result<Value> {
        fn encode_node(node: &ProjectNode, base: &Path) -> io::Result<Value> {
            let children = node
                .children
                .iter()
                .map(|child| encode_node(child, base))
                .collect::<io::Result<Vec<_>>>()?;
            let (kind, path) = match &node.kind {
                NodeKind::Project => ("project", Value::Null),
                NodeKind::Group => ("group", Value::Null),
                NodeKind::File(path) => {
                    let stored = path.strip_prefix(base).unwrap_or(path);
                    (
                        "file",
                        json!(
                            stored
                                .to_str()
                                .ok_or_else(|| invalid("Project paths must be UTF-8"))?
                        ),
                    )
                }
            };
            Ok(
                json!({"id": node.id, "name": node.name, "kind": kind, "path": path, "expanded": node.expanded, "children": children}),
            )
        }
        let projects = self
            .roots
            .iter()
            .map(|node| encode_node(node, base))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(json!({"format": "notepad-matrix-projects", "version": 1, "projects": projects}))
    }

    fn decode(value: &Value, base: &Path) -> io::Result<Self> {
        if value["format"] != "notepad-matrix-projects" || value["version"] != 1 {
            return Err(invalid("Unsupported project workspace format or version"));
        }
        let mut ids = std::collections::BTreeSet::new();
        fn decode_node(
            value: &Value,
            base: &Path,
            depth: usize,
            ids: &mut std::collections::BTreeSet<u64>,
        ) -> io::Result<ProjectNode> {
            if depth > 128 || ids.len() >= 100_000 {
                return Err(invalid("Project tree exceeds supported limits"));
            }
            let id = value["id"]
                .as_u64()
                .filter(|id| *id > 0 && *id < u64::MAX)
                .ok_or_else(|| invalid("Invalid project node id"))?;
            if !ids.insert(id) {
                return Err(invalid("Duplicate project node id"));
            }
            let name = value["name"]
                .as_str()
                .ok_or_else(|| invalid("Missing project name"))?;
            valid_name(name)?;
            let kind = match value["kind"].as_str() {
                Some("project") if depth == 0 => NodeKind::Project,
                Some("group") if depth > 0 => NodeKind::Group,
                Some("file") if depth > 0 => {
                    let path = value["path"]
                        .as_str()
                        .filter(|path| !path.is_empty() && !path.contains('\0'))
                        .ok_or_else(|| invalid("Invalid file path"))?;
                    NodeKind::File(absolute(&base.join(path))?)
                }
                _ => return Err(invalid("Invalid node kind or hierarchy")),
            };
            let children = value["children"]
                .as_array()
                .ok_or_else(|| invalid("Missing children array"))?;
            if matches!(kind, NodeKind::File(_)) && !children.is_empty() {
                return Err(invalid("File nodes cannot have children"));
            }
            let children = children
                .iter()
                .map(|child| decode_node(child, base, depth + 1, ids))
                .collect::<io::Result<Vec<_>>>()?;
            let expanded = value["expanded"]
                .as_bool()
                .ok_or_else(|| invalid("Missing expansion state"))?;
            Ok(ProjectNode {
                id,
                name: name.into(),
                kind,
                children,
                expanded,
            })
        }
        let roots = value["projects"]
            .as_array()
            .ok_or_else(|| invalid("Missing projects array"))?
            .iter()
            .map(|node| decode_node(node, base, 0, &mut ids))
            .collect::<io::Result<Vec<_>>>()?;
        let next_id = ids.last().copied().unwrap_or(0) + 1;
        Ok(Self {
            roots,
            next_id,
            ..Self::default()
        })
    }
}

fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(invalid("Empty path"));
    }
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectPanels {
    panels: [ProjectWorkspace; 3],
}

impl ProjectPanels {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn panel(&self, index: usize) -> io::Result<&ProjectWorkspace> {
        self.panels
            .get(index)
            .ok_or_else(|| invalid("Project panel index must be 0, 1, or 2"))
    }
    pub fn panel_mut(&mut self, index: usize) -> io::Result<&mut ProjectWorkspace> {
        self.panels
            .get_mut(index)
            .ok_or_else(|| invalid("Project panel index must be 0, 1, or 2"))
    }

    pub fn snapshot(&self) -> io::Result<Value> {
        let base = std::env::current_dir()?;
        let base_text = base
            .to_str()
            .ok_or_else(|| invalid("Snapshot base must be UTF-8"))?;
        let panels = self.panels.iter().map(|panel| {
            let path = panel.path.as_ref().map(|path| path.to_str().ok_or_else(|| invalid("Workspace path must be UTF-8"))).transpose()?;
            Ok(json!({"workspace": panel.encode(&base)?, "base": base_text, "path": path, "dirty": panel.dirty, "visible": panel.visible, "selected": panel.selected, "next_id": panel.next_id}))
        }).collect::<io::Result<Vec<Value>>>()?;
        Ok(json!({"format": "notepad-matrix-project-panels", "version": 1, "panels": panels}))
    }

    pub fn restore(value: &Value) -> io::Result<Self> {
        if value["format"] != "notepad-matrix-project-panels" || value["version"] != 1 {
            return Err(invalid("Unsupported project panel snapshot"));
        }
        let entries = value["panels"]
            .as_array()
            .filter(|entries| entries.len() == 3)
            .ok_or_else(|| invalid("A project snapshot must contain exactly three panels"))?;
        let mut result = Self::new();
        for (panel, entry) in result.panels.iter_mut().zip(entries) {
            let base = Path::new(
                entry["base"]
                    .as_str()
                    .ok_or_else(|| invalid("Missing snapshot base"))?,
            );
            if !base.is_absolute() {
                return Err(invalid("Snapshot base must be absolute"));
            }
            *panel = ProjectWorkspace::decode(&entry["workspace"], base)?;
            panel.path = match &entry["path"] {
                Value::Null => None,
                Value::String(path) if Path::new(path).is_absolute() => Some(PathBuf::from(path)),
                _ => return Err(invalid("Invalid workspace path")),
            };
            panel.visible = entry["visible"]
                .as_bool()
                .ok_or_else(|| invalid("Missing visibility"))?;
            panel.dirty = entry["dirty"]
                .as_bool()
                .ok_or_else(|| invalid("Missing dirty state"))?;
            panel.next_id = entry["next_id"]
                .as_u64()
                .filter(|next| *next >= panel.next_id && *next < u64::MAX)
                .ok_or_else(|| invalid("Invalid next node id"))?;
            panel.selected = match &entry["selected"] {
                Value::Null => None,
                other => Some(
                    other
                        .as_u64()
                        .filter(|id| panel.node(*id).is_some())
                        .ok_or_else(|| invalid("Invalid project selection"))?,
                ),
            };
        }
        Ok(result)
    }
}
