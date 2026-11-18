use super::*;
use std::io::Write;

pub struct SessionFile {
    document: Value,
    baseline: Option<(PathBuf, Vec<u8>)>,
}

impl RecoveryManager {
    pub fn rename_buffer_backup(&mut self, id: BufferId, old_name: &str, new_name: &str) -> io::Result<()> {
        if !self.snapshot_mode || old_name == new_name { return Ok(()); }
        for session in &mut self.sessions {
            let Some(key) = session.buffers.get(&id) else { continue; };
            let metadata_path = snapshot_metadata_path(key)?;
            let metadata = match session.store.load_json(&metadata_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if metadata["backupFilePath"].is_null() { continue; }
            let previous = raw_backup::path(&session.store, &metadata)?;
            let filename = previous.file_name().and_then(|name| name.to_str()).ok_or_else(|| invalid("invalid backup filename"))?;
            let Some((name, timestamp)) = filename.rsplit_once('@') else { return Err(invalid("invalid backup timestamp")); };
            if name != old_name { return Err(invalid("backup filename does not match the renamed tab")); }
            let target = previous.with_file_name(format!("{new_name}@{timestamp}"));
            let mut renamed = metadata.clone();
            renamed["name"] = new_name.into();
            renamed["backupFilePath"] = serde_json::to_value(&target)?;
            raw_backup::path(&session.store, &renamed)?;
            fs::rename(&previous, &target)?;
            if let Err(error) = session.store.update_json(&metadata_path, &metadata, |document| {
                document["name"] = renamed["name"].clone();
                document["backupFilePath"] = renamed["backupFilePath"].clone();
                Ok(())
            }) {
                return match fs::rename(&target, &previous) {
                    Ok(()) => Err(error),
                    Err(rollback) => Err(io::Error::other(format!("backup metadata could not be renamed: {error}; backup path could not be restored: {rollback}"))),
                };
            }
        }
        Ok(())
    }
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
            documents.push(serde_json::json!({"key":key,"path":path,"anchor":selection.anchor.0,"head":selection.head.0,"language":buffer.language().key(),"udl":workspace.udl_language_name(buffer.language()),"codepage":buffer.byte_code_page().map(crate::files::ByteFormatCodePage::value),"pinned":buffer.is_pinned(),"monitoring":buffer.is_monitoring(),"user_read_only":buffer.is_user_read_only(),"marks":buffer.editor.document().bookmarked_lines().collect::<Vec<_>>()}));
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
        if let Some(documents) = manifest["documents"].as_array() {
            for document in documents {
                if let Some(codepage) = crate::files::ByteFormatCodePage::from_value(&document["codepage"])? { codepage.encode("")?; }
                if document.get("udl").is_some_and(|value| !value.is_null() && !value.is_string()) {
                    return Err(invalid("invalid session user language name"));
                }
            }
        }
        if !keys.is_empty() || self.document.get("payloads").is_some() {
            return Err(invalid(
                "manual session files do not contain recovery payloads",
            ));
        }
        Ok(keys)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let _access = crate::file_access::read(path)?;
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

    pub fn paths(&self) -> io::Result<Vec<PathBuf>> {
        self.document["session"]["documents"].as_array().ok_or_else(|| invalid("invalid session documents"))?.iter().filter_map(|document| {
            (!document["path"].is_null()).then(|| serde_json::from_value(document["path"].clone()).map_err(io::Error::from))
        }).collect()
    }

    pub fn exclude_paths(&mut self, paths: &[PathBuf]) -> io::Result<()> {
        let manifest = &mut self.document["session"];
        let documents = manifest["documents"].as_array_mut().ok_or_else(|| invalid("invalid session documents"))?;
        let mut excluded = BTreeSet::new();
        for document in documents.iter() {
            if !document["path"].is_null() {
                let path: PathBuf = serde_json::from_value(document["path"].clone())?;
                if paths.contains(&path) { excluded.insert(document["key"].as_str().ok_or_else(|| invalid("session document identity is missing"))?.to_owned()); }
            }
        }
        documents.retain(|document| !document["key"].as_str().is_some_and(|key| excluded.contains(key)));
        let current_view = manifest["active_view"].as_u64().and_then(|view| usize::try_from(view).ok()).unwrap_or(0);
        let mut replacement_view = None;
        if let Some(views) = manifest["views"].as_array_mut() {
            for view in views.iter_mut() {
                let members = view["documents"].as_array_mut().ok_or_else(|| invalid("invalid session view documents"))?;
                members.retain(|member| !member["key"].as_str().is_some_and(|key| excluded.contains(key)));
                let first = members.first().map(|member| member["key"].clone()).unwrap_or(Value::Null);
                if view["active"].as_str().is_some_and(|key| excluded.contains(key)) { view["active"] = first; }
            }
            let active_view = views.iter().position(|view| view["active"].is_string()).unwrap_or(0);
            if !views.get(current_view).is_some_and(|view| view["active"].is_string()) {
                replacement_view = Some(active_view);
            }
        }
        if let Some(view) = replacement_view { manifest["active_view"] = view.into(); }
        if manifest["active"].as_str().is_some_and(|key| excluded.contains(key)) { manifest["active"] = Value::Null; }
        self.validate()?;
        Ok(())
    }

    pub fn save(&self, path: &Path, overwrite: crate::files::OverwritePolicy) -> io::Result<()> {
        self.validate()?;
        let _access = crate::file_access::directory(path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
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
        let empty = (workspace.buffers().len() == 1
            && workspace.active().disk_file.is_none()
            && !workspace.active().is_modified()
            && workspace.active().editor.document().len_bytes() == 0)
            .then(|| (workspace.active_view(), workspace.active().id()));
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
        if let Some(documents) = self.document["session"]["documents"].as_array() {
            for document in documents {
                if document.get("codepage").is_none() || document["path"].is_null() { continue; }
                let path: PathBuf = serde_json::from_value(document["path"].clone())?;
                let resolved = fs::canonicalize(&path).unwrap_or(path);
                let codepage = crate::files::ByteFormatCodePage::from_value(&document["codepage"])?;
                if let Some(buffer) = workspace.buffers_mut().find(|buffer| buffer.disk_file.as_ref().is_some_and(|file| file.path() == resolved)) { buffer.restore_byte_code_page(codepage); }
            }
        }
        workspace.refresh_udl_metadata();
        if let Some(id) = active {
            let active_pane = workspace.active_view();
            if let Some((pane, empty)) = empty
                && empty != id
            {
                workspace
                    .close_in_view(
                        pane,
                        empty,
                        crate::workspace::CloseDisposition::IfUnmodified,
                    )
                    .map_err(|error| invalid(&error.to_string()))?;
            }
            workspace
                .set_active_view(active_pane)
                .map_err(|error| invalid(&error.to_string()))?;
            workspace
                .activate(id)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        Ok(warnings)
    }
}
