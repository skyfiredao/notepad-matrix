use super::{ConfigStore, UnicodeFormat, Utf8Format, Value, invalid};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) fn path(store: &ConfigStore, document: &Value) -> io::Result<PathBuf> {
    let path: PathBuf = serde_json::from_value(document["backupFilePath"].clone())?;
    let directory = store
        .document_path(Path::new("backup/prepare.json"))?
        .parent()
        .ok_or_else(|| invalid("backup directory is missing"))?
        .to_owned();
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("invalid backup filename"))?;
    let timestamp = filename.rsplit_once('@').map(|(_, timestamp)| timestamp);
    if path.parent() != Some(directory.as_path())
        || !timestamp.is_some_and(|timestamp| {
            timestamp.len() == 17
                && timestamp
                    .bytes()
                    .enumerate()
                    .all(|(index, byte)| match index {
                        4 | 7 => byte == b'-',
                        10 => byte == b'_',
                        _ => byte.is_ascii_digit(),
                    })
        })
    {
        return Err(invalid("backupFilePath is outside the backup directory"));
    }
    validate_file(&path)?;
    Ok(path)
}

pub(super) fn new_path(store: &ConfigStore, document: &Value) -> io::Result<PathBuf> {
    let directory = store
        .document_path(Path::new("backup/prepare.json"))?
        .parent()
        .ok_or_else(|| invalid("backup directory is missing"))?
        .to_owned();
    let name = document["name"]
        .as_str()
        .ok_or_else(|| invalid("recovery name is missing"))?;
    if name.is_empty() || Path::new(name).file_name() != Some(std::ffi::OsStr::new(name)) {
        return Err(invalid("invalid backup filename"));
    }
    Ok(directory.join(format!(
        "{name}@{}",
        chrono::Local::now().format("%Y-%m-%d_%H%M%S")
    )))
}

fn validate_file(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(invalid("backup must be a regular file"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err(invalid("backup must not be hard linked"));
                }
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn write(
    path: &Path,
    text: &str,
    format: UnicodeFormat,
    untitled: bool,
) -> io::Result<()> {
    if let Some(metadata) = validate_file(path)?
        && metadata.permissions().readonly()
    {
        let mut permissions = metadata.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() | 0o200);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }
    let target = if untitled {
        let mut name = path.as_os_str().to_os_string();
        name.push(".tmp");
        PathBuf::from(name)
    } else {
        path.to_owned()
    };
    validate_file(&target)?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = io::BufWriter::new(options.open(&target)?);
    match format {
        UnicodeFormat::Utf8(Utf8Format::Plain) => file.write_all(text.as_bytes())?,
        UnicodeFormat::Utf8(Utf8Format::Bom) => {
            file.write_all(b"\xef\xbb\xbf")?;
            file.write_all(text.as_bytes())?;
        }
        UnicodeFormat::Utf16LeBom | UnicodeFormat::Utf16BeBom => {
            file.write_all(if format == UnicodeFormat::Utf16LeBom {
                b"\xff\xfe"
            } else {
                b"\xfe\xff"
            })?;
            crate::unicode_writer::write_utf16(
                text.bytes(),
                format == UnicodeFormat::Utf16BeBom,
                &mut file,
            )?;
        }
    }
    file.flush()?;
    drop(file);
    if untitled {
        fs::rename(target, path)?;
    }
    Ok(())
}

pub(super) fn read(path: &Path, format: UnicodeFormat) -> io::Result<String> {
    validate_file(path)?;
    let bytes = fs::read(path)?;
    let (bom, utf16) = match format {
        UnicodeFormat::Utf8(Utf8Format::Plain) => (b"".as_slice(), false),
        UnicodeFormat::Utf8(Utf8Format::Bom) => (b"\xef\xbb\xbf".as_slice(), false),
        UnicodeFormat::Utf16LeBom => (b"\xff\xfe".as_slice(), true),
        UnicodeFormat::Utf16BeBom => (b"\xfe\xff".as_slice(), true),
    };
    let bytes = bytes
        .strip_prefix(bom)
        .ok_or_else(|| invalid("backup encoding does not match its metadata"))?;
    if !utf16 {
        return String::from_utf8(bytes.to_vec()).map_err(|error| invalid(&error.to_string()));
    }
    let decoded = crate::unicode_reader::Utf16Reader::default()
        .convert(bytes, format == UnicodeFormat::Utf16BeBom);
    String::from_utf8(decoded).map_err(|error| invalid(&error.to_string()))
}
