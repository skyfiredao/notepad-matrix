use std::fs;
use std::io;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

pub struct ConfigStore {
    root: PathBuf,
}

impl ConfigStore {
    pub fn under_home(home: &Path) -> Self {
        Self {
            root: crate::config_directory::active_root(home),
        }
    }

    pub(crate) fn under_default_home(home: &Path) -> Self {
        Self {
            root: home.join(".notepad-matrix"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn ensure_directory(path: &Path) -> io::Result<()> {
        let missing = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => false,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "configuration root must be a directory, not a symbolic link",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error),
        };
        if !missing {
            return Ok(());
        }
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)
    }

    pub(crate) fn document_path(&self, relative: &Path) -> io::Result<PathBuf> {
        if relative.extension() != Some(std::ffi::OsStr::new("json"))
            || !relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "configuration path must be relative JSON within .notepad-matrix",
            ));
        }
        Self::ensure_directory(&self.root)?;
        let mut parent = self.root.clone();
        if let Some(directory) = relative.parent() {
            for component in directory.components() {
                parent.push(component.as_os_str());
                Self::ensure_directory(&parent)?;
            }
        }
        Ok(self.root.join(relative))
    }

    pub fn load_json(&self, relative: &Path) -> io::Result<Value> {
        let path = self.document_path(relative)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if metadata.is_some_and(|metadata| !metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "configuration JSON must be a regular file, not a symbolic link",
            ));
        }
        let document: Value = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Self::validate_document(relative, &document)?;
        Ok(document)
    }

    pub fn update_json(
        &self,
        relative: &Path,
        defaults: &Value,
        update: impl FnOnce(&mut Value) -> io::Result<()>,
    ) -> io::Result<Value> {
        let path = self.document_path(relative)?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(self.root.join(".config-write.lock"))?;
        lock.lock()?;
        let mut document = match self.load_json(relative) {
            Ok(document) => document,
            Err(error) if error.kind() == io::ErrorKind::NotFound => defaults.clone(),
            Err(error) => return Err(error),
        };
        Self::validate_document(relative, &document)?;
        update(&mut document)?;
        Self::validate_document(relative, &document)?;
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("configuration path has no parent"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(&document)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(document)
    }

    pub fn load_settings(&self) -> io::Result<Value> {
        match self.load_json(Path::new("settings.json")) {
            Ok(settings) => Ok(settings),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.update_settings(|_| Ok(()))
            }
            Err(error) => Err(error),
        }
    }

    pub fn update_settings(
        &self,
        update: impl FnOnce(&mut Value) -> io::Result<()>,
    ) -> io::Result<Value> {
        self.update_json(
            Path::new("settings.json"),
            &serde_json::json!({
                "schema_version": 1,
                "theme": "light",
                "save_all_confirm": true,
                "function_list_sort_default": false,
                "docPeekOnTab": false,
                "tabCompactLabelLen": 0,
                "recent_files_limit": 10,
                "whiteSpaceShow": false,
                "eolShow": false,
                "npcShow": false,
                "ccShow": true,
                "muteSounds": false,
                "features": {"html_export": true, "mime_tools": true, "converter": true}
            }),
            update,
        )
    }

    fn validate_document(relative: &Path, document: &Value) -> io::Result<()> {
        if document.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "configuration JSON requires schema_version 1",
            ));
        }
        if relative == Path::new("settings.json") {
            Self::validate_settings(document)?;
        }
        Ok(())
    }

    fn validate_settings(settings: &Value) -> io::Result<()> {
        for field in [
            "save_all_confirm",
            "statusbar_visible",
            "function_list_sort_default",
            "docPeekOnTab",
            "muteSounds",
        ] {
            if settings.get(field).is_some_and(|value| !value.is_boolean()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("settings.json {field} must be a boolean"),
                ));
            }
        }
        if !matches!(
            settings.get("theme").and_then(Value::as_str),
            Some("light" | "dark")
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "settings.json theme must be light or dark",
            ));
        }
        for feature in ["html_export", "mime_tools", "converter"] {
            if settings
                .get("features")
                .and_then(|features| features.get(feature))
                .and_then(Value::as_bool)
                .is_none()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("settings.json features.{feature} must be a boolean"),
                ));
            }
        }
        Ok(())
    }
}
