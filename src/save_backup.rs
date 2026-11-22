use serde_json::Value;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveBackupMode {
    #[default]
    None,
    Simple,
    Verbose,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveBackupOptions {
    pub mode: SaveBackupMode,
    pub directory: Option<PathBuf>,
}

impl SaveBackupOptions {
    pub fn from_settings(settings: &Value) -> io::Result<Self> {
        let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
        let mode = match settings.get("save_backup").and_then(Value::as_str) {
            None if settings.get("save_backup").is_none() => SaveBackupMode::None,
            Some("none") => SaveBackupMode::None,
            Some("simple") => SaveBackupMode::Simple,
            Some("verbose") => SaveBackupMode::Verbose,
            _ => return Err(invalid("save_backup must be none, simple, or verbose")),
        };
        let use_directory = match settings.get("backup_use_directory") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| invalid("backup_use_directory must be a boolean"))?,
        };
        let directory = match settings.get("backup_directory") {
            None => None,
            Some(value) => {
                let path = value
                    .as_str()
                    .ok_or_else(|| invalid("backup_directory must be a path string"))?;
                (!path.is_empty() && use_directory).then(|| PathBuf::from(expand_environment(path)))
            }
        };
        Ok(Self { mode, directory })
    }

    pub fn create_backup(&self, original: &Path, timestamp: &str) -> io::Result<Option<PathBuf>> {
        Ok(self
            .create_backup_file(original, timestamp)?
            .map(|(path, _)| path))
    }

    pub fn create_tracked_backup(
        &self,
        store: &crate::config::ConfigStore,
        original: &Path,
        timestamp: &str,
    ) -> io::Result<Option<PathBuf>> {
        if self.mode == SaveBackupMode::None {
            return Ok(None);
        }
        let _maintenance = crate::backup_retention::save_backup_lock(store)?;
        crate::backup_retention::validate_registry(store)?;
        let Some((path, file)) = self.create_backup_file(original, timestamp)? else {
            return Ok(None);
        };
        crate::backup_retention::register_backup(store, &path, &file)?;
        Ok(Some(path))
    }

    fn create_backup_file(
        &self,
        original: &Path,
        timestamp: &str,
    ) -> io::Result<Option<(PathBuf, std::fs::File)>> {
        if self.mode == SaveBackupMode::None {
            return Ok(None);
        }
        let _source_access = crate::file_access::read(original)?;
        let name = original.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "backup source has no filename")
        })?;
        let directory = self.directory.clone().unwrap_or_else(|| {
            let parent = original.parent().unwrap_or_else(|| Path::new("."));
            if self.mode == SaveBackupMode::Verbose {
                parent.join("nppBackup")
            } else {
                parent.to_owned()
            }
        });
        let _directory_access = crate::file_access::directory(&directory)?;
        std::fs::create_dir_all(&directory)?;
        let mut filename = name.to_os_string();
        if self.mode == SaveBackupMode::Verbose {
            if timestamp.len() != 17
                || !timestamp
                    .bytes()
                    .enumerate()
                    .all(|(index, byte)| match index {
                        4 | 7 => byte == b'-',
                        10 => byte == b'_',
                        _ => byte.is_ascii_digit(),
                    })
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "backup timestamp must be YYYY-MM-DD_HHMMSS",
                ));
            }
            filename.push(format!(".{timestamp}"));
        }
        filename.push(".bak");
        let target = directory.join(filename);
        let contents = std::fs::read(original)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(&contents)?;
        temporary.as_file().sync_all()?;
        let file = temporary.persist(&target).map_err(|error| error.error)?;
        Ok(Some((target, file)))
    }
}

fn expand_environment(path: &str) -> String {
    let mut expanded = String::new();
    let mut rest = path;
    while let Some(start) = rest.find('%') {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            expanded.push_str(&rest[start..]);
            return expanded;
        };
        let variable = &after[..end];
        match std::env::var(variable) {
            Ok(value) => expanded.push_str(&value),
            Err(_) => expanded.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    expanded.push_str(rest);
    expanded
}
