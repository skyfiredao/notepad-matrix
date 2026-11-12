use super::*;
use std::io::Write;

pub struct SessionFile {
    document: Value,
    baseline: Option<(PathBuf, Vec<u8>)>,
}

impl SessionFile {
    pub fn capture(workspace: &Workspace) -> io::Result<Self> {
        let mut documents = Vec::new();
        let mut identities = BTreeMap::new();
        for buffer in workspace.buffers().iter().filter(|buffer| {
            buffer
                .disk_file
                .as_ref()
                .is_some_and(|file| file.path().is_file())
        }) {
            let key = format!("saved-session-buffer-{}", documents.len() + 1);
            identities.insert(buffer.id(), key.clone());
            let selection = buffer.editor.document().selection();
            let path = buffer
                .disk_file
                .as_ref()
                .map(|file| serde_json::to_value(file.path()))
                .transpose()?;
            documents.push(serde_json::json!({"key":key,"path":path,"anchor":selection.anchor.0,"head":selection.head.0,"language":buffer.language().key(),"pinned":buffer.is_pinned(),"monitoring":buffer.is_monitoring(),"user_read_only":buffer.is_user_read_only(),"marks":buffer.editor.document().bookmarked_lines().collect::<Vec<_>>()}));
        }
        let views = [ViewId::Primary,ViewId::Secondary].into_iter().map(|view| {
            let members = workspace.view_buffer_ids(view).iter().filter_map(|id| {
                let key = identities.get(id)?;
                let editor = workspace.editor_in_view(view,*id)?;
                let selection = editor.document().selection();
                Some(serde_json::json!({"key":key,"anchor":selection.anchor.0,"head":selection.head.0,"selection_set":selection_set_json(&editor.document().selections()),"color":workspace.tab_color(view,*id),"state":view_state_json(&workspace.view_state(view,*id).cloned().unwrap_or_default())}))
            }).collect::<Vec<_>>();
            let active = workspace.active_id_in_view(view).and_then(|id| identities.get(&id)).cloned().or_else(|| members.first().and_then(|member| member["key"].as_str()).map(str::to_owned));
            serde_json::json!({"documents":members,"active":active})
        }).collect::<Vec<_>>();
        let active = identities.get(&workspace.active().id());
        let active_view = if views[workspace.active_view() as usize]["active"].is_string() {
            workspace.active_view() as usize
        } else if views[0]["active"].is_string() {
            0
        } else {
            1
        };
        let manifest = serde_json::json!({"snapshots":[],"documents":documents,"views":views,"active_view":active_view,"active":active,"next_snapshot":documents.len()+1,"ui":workspace.session_ui()});
        let archive = Self {
            document: serde_json::json!({"schema_version":1,"session":manifest}),
            baseline: None,
        };
        archive.validate()?;
        Ok(archive)
    }

    fn validate(&self) -> io::Result<Vec<String>> {
        if self.document.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(invalid("session JSON requires schema_version 1"));
        }
        let manifest = self
            .document
            .get("session")
            .ok_or_else(|| invalid("session manifest is missing"))?;
        let keys = RecoverySession::validate_manifest_owner("saved-session", manifest)?;
        if !keys.is_empty() || self.document.get("payloads").is_some() {
            return Err(invalid(
                "manual session files do not contain recovery payloads",
            ));
        }
        Ok(keys)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let document = serde_json::from_slice(&bytes)
            .map_err(|error| invalid(&format!("invalid session JSON: {error}")))?;
        let archive = Self {
            document,
            baseline: Some((path.to_owned(), bytes)),
        };
        archive.validate()?;
        Ok(archive)
    }

    pub fn save(&self, path: &Path, overwrite: crate::files::OverwritePolicy) -> io::Result<()> {
        self.validate()?;
        let previous = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if previous.is_some() && overwrite == crate::files::OverwritePolicy::Refuse {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "session export replacement was not confirmed",
            ));
        }
        if let Some((original, bytes)) = &self.baseline
            && original == path
            && previous.as_ref() != Some(bytes)
        {
            return Err(io::Error::other(
                "session file changed outside the application",
            ));
        }
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(&self.document)?)?;
        temporary.as_file().sync_all()?;
        let current = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if current != previous {
            return Err(io::Error::other("session file changed while saving"));
        }
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn restore_into(&self, workspace: &mut Workspace) -> io::Result<Vec<String>> {
        let keys = self.validate()?;
        let mut warnings = Vec::new();
        let (_, active) = RecoverySession::restore_document_data(
            workspace,
            &self.document["session"],
            &keys,
            false,
            true,
            |_| {
                Err(invalid(
                    "manual session loading does not read recovery payloads",
                ))
            },
            &mut warnings,
        )?;
        if let Some(id) = active {
            workspace
                .activate(id)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        Ok(warnings)
    }
}
