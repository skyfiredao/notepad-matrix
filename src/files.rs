use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use crate::editor::SaveSnapshot;

const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";
const UTF16_LE_BOM: &[u8] = b"\xff\xfe";
const UTF16_BE_BOM: &[u8] = b"\xfe\xff";

fn encode_modification_time(time: SystemTime) -> serde_json::Value {
    let (before_epoch, distance) = match time.duration_since(UNIX_EPOCH) {
        Ok(distance) => (false, distance),
        Err(error) => (true, error.duration()),
    };
    serde_json::json!({"before_epoch": before_epoch, "seconds": distance.as_secs(), "nanoseconds": distance.subsec_nanos()})
}

fn decode_modification_time(value: &serde_json::Value) -> io::Result<Option<SystemTime>> {
    if value.is_null() {
        return Ok(None);
    }
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid recovery modification time",
        )
    };
    let before_epoch = value
        .get("before_epoch")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(invalid)?;
    let seconds = value
        .get("seconds")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(invalid)?;
    let nanoseconds = value
        .get("nanoseconds")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value < 1_000_000_000)
        .ok_or_else(invalid)?;
    let distance = Duration::new(seconds, u32::try_from(nanoseconds).map_err(|_| invalid())?);
    let time = if before_epoch {
        UNIX_EPOCH.checked_sub(distance)
    } else {
        UNIX_EPOCH.checked_add(distance)
    };
    Ok(Some(time.ok_or_else(invalid)?))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Utf8Format {
    Plain,
    Bom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnicodeFormat {
    Utf8(Utf8Format),
    Utf16LeBom,
    Utf16BeBom,
}

impl UnicodeFormat {
    fn detect(bytes: &[u8]) -> Self {
        if bytes.starts_with(UTF16_BE_BOM) {
            Self::Utf16BeBom
        } else if bytes.starts_with(UTF16_LE_BOM) {
            // Same priority as Notepad++: a following NUL is not evidence that
            // this valid UTF-16 prefix should instead be interpreted as UTF-32.
            Self::Utf16LeBom
        } else if bytes.starts_with(UTF8_BOM) {
            Self::Utf8(Utf8Format::Bom)
        } else {
            Self::Utf8(Utf8Format::Plain)
        }
    }

    fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8(Utf8Format::Plain) => b"",
            Self::Utf8(Utf8Format::Bom) => UTF8_BOM,
            Self::Utf16LeBom => UTF16_LE_BOM,
            Self::Utf16BeBom => UTF16_BE_BOM,
        }
    }
}

fn encode_snapshot(snapshot: &SaveSnapshot, format: UnicodeFormat) -> io::Result<Vec<u8>> {
    let mut bytes = format.bom().to_vec();
    match format {
        UnicodeFormat::Utf8(_) => snapshot.write_to(&mut bytes)?,
        UnicodeFormat::Utf16LeBom | UnicodeFormat::Utf16BeBom => {
            crate::unicode_writer::write_utf16(
                snapshot.chunks().flat_map(|chunk| chunk.iter().copied()),
                format == UnicodeFormat::Utf16BeBom,
                &mut bytes,
            )?;
        }
    }
    Ok(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwritePolicy {
    Refuse,
    Confirmed,
}

#[derive(Clone)]
pub struct DiskFile {
    path: PathBuf,
    bytes: Arc<[u8]>,
    format: UnicodeFormat,
    read_only: bool,
    original_modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl DiskFile {
    pub(crate) fn loaded_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn relocate_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    pub(crate) fn after_move(&self, path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        let mut file = self.clone();
        file.path = fs::canonicalize(path)?;
        file.read_only = metadata.permissions().readonly();
        file.original_modified = Some(metadata.modified()?);
        #[cfg(unix)]
        {
            file.identity = (metadata.dev(), metadata.ino());
        }
        Ok(file)
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub(crate) fn original_modified(&self) -> Option<SystemTime> {
        self.original_modified
    }

    pub(crate) fn refresh_recovery_read_only(&mut self, metadata: &fs::Metadata) {
        self.read_only = metadata.permissions().readonly();
    }

    pub fn format(&self) -> UnicodeFormat {
        self.format
    }

    pub(crate) fn observe_original_timestamp(&mut self) -> io::Result<Option<bool>> {
        match self.original_modified {
            Some(original) => {
                let observed = fs::metadata(&self.path)?.modified()?;
                self.original_modified = Some(observed);
                Ok(Some(observed != original))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn recovery_state(&self) -> io::Result<serde_json::Value> {
        let mut state = serde_json::json!({
            "path": serde_json::to_value(&self.path)?,
            "bytes": self.bytes.as_ref(),
            "read_only": self.read_only,
            "original_modified": self.original_modified.map(encode_modification_time),
        });
        #[cfg(unix)]
        {
            state["device"] = self.identity.0.into();
            state["inode"] = self.identity.1.into();
        }
        Ok(state)
    }

    pub(crate) fn from_recovery_state(state: &serde_json::Value) -> io::Result<Self> {
        let invalid =
            || io::Error::new(io::ErrorKind::InvalidData, "invalid recovery disk baseline");
        let path: PathBuf = serde_json::from_value(state.get("path").ok_or_else(invalid)?.clone())?;
        if !path.is_absolute() {
            return Err(invalid());
        }
        let bytes: Vec<u8> =
            serde_json::from_value(state.get("bytes").ok_or_else(invalid)?.clone())?;
        let read_only = state
            .get("read_only")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(invalid)?;
        let format = UnicodeFormat::detect(&bytes);
        let file = Self {
            path,
            bytes: bytes.into(),
            format,
            read_only,
            original_modified: decode_modification_time(
                state
                    .get("original_modified")
                    .unwrap_or(&serde_json::Value::Null),
            )?,
            #[cfg(unix)]
            identity: (
                state
                    .get("device")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(invalid)?,
                state
                    .get("inode")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(invalid)?,
            ),
        };
        file.decode_text()?;
        Ok(file)
    }

    pub fn save_as_utf8(
        path: &Path,
        snapshot: &SaveSnapshot,
        format: Utf8Format,
        overwrite: OverwritePolicy,
    ) -> io::Result<Self> {
        Self::save_as_unicode(path, snapshot, UnicodeFormat::Utf8(format), overwrite)
    }

    pub fn save_as_unicode(
        path: &Path,
        snapshot: &SaveSnapshot,
        format: UnicodeFormat,
        overwrite: OverwritePolicy,
    ) -> io::Result<Self> {
        match fs::symlink_metadata(path) {
            Ok(_) if overwrite == OverwritePolicy::Refuse => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "overwrite requires explicit confirmation",
            )),
            Ok(_) => Self::capture(path)?.save_with_format(snapshot, format),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Self::create_unicode(path, snapshot, format)
            }
            Err(error) => Err(error),
        }
    }

    /// Save a new explicit UTF-8 document; an existing path is never overwritten here.
    pub fn create_utf8(path: &Path, snapshot: &SaveSnapshot) -> io::Result<Self> {
        Self::create_unicode(path, snapshot, UnicodeFormat::Utf8(Utf8Format::Plain))
    }

    fn create_unicode(
        path: &Path,
        snapshot: &SaveSnapshot,
        format: UnicodeFormat,
    ) -> io::Result<Self> {
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "document path has no file name",
            )
        })?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = fs::canonicalize(parent)?;
        let path = parent.join(name);
        let mut output = tempfile::Builder::new()
            .prefix(".notepad-matrix-save-")
            .tempfile_in(&parent)?;
        let bytes = encode_snapshot(snapshot, format)?;
        output.write_all(&bytes)?;
        output.as_file().sync_all()?;
        let file = output
            .persist_noclobber(&path)
            .map_err(|error| error.error)?;
        let saved = file.metadata()?;
        Ok(Self {
            path,
            bytes: bytes.into(),
            format,
            read_only: saved.permissions().readonly(),
            original_modified: Some(saved.modified()?),
            #[cfg(unix)]
            identity: (saved.dev(), saved.ino()),
        })
    }

    /// Load explicit UTF-8 only. No locale/ANSI inference and no malformed-byte replacement.
    pub fn load_utf8(path: &Path) -> io::Result<(Self, String)> {
        let file = Self::capture(path)?;
        if !matches!(file.format, UnicodeFormat::Utf8(_)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "explicit UTF-8 loading refuses UTF-16 input",
            ));
        }
        let text = file.decode_text()?;
        Ok((file, text))
    }

    /// BOM-signalled Unicode, or strict UTF-8 when absent. No ANSI or UTF-32 inference.
    pub fn load_unicode(path: &Path) -> io::Result<(Self, String)> {
        let file = Self::capture(path)?;
        let text = file.decode_text()?;
        Ok((file, text))
    }

    pub fn load_unicode_bytes(path: &Path) -> io::Result<(Self, Vec<u8>)> {
        let file = Self::capture(path)?;
        let bytes = file.decoded_bytes();
        Ok((file, bytes))
    }

    pub fn load_byte_buffer(path: &Path) -> io::Result<(Self, crate::byte_buffer::ByteBuffer)> {
        let file = Self::capture(path)?;
        let buffer = match file.format {
            UnicodeFormat::Utf8(_) => {
                crate::byte_buffer::ByteBuffer::from_bytes(&file.bytes[file.format.bom().len()..])
            }
            UnicodeFormat::Utf16LeBom | UnicodeFormat::Utf16BeBom => {
                crate::byte_buffer::ByteBuffer::from_bytes(&file.decoded_bytes())
            }
        };
        Ok((file, buffer))
    }

    fn decode_text(&self) -> io::Result<String> {
        String::from_utf8(self.decoded_bytes())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn decoded_bytes(&self) -> Vec<u8> {
        let content = &self.bytes[self.format.bom().len()..];
        match self.format {
            UnicodeFormat::Utf8(_) => content.to_vec(),
            UnicodeFormat::Utf16LeBom | UnicodeFormat::Utf16BeBom => {
                crate::unicode_reader::Utf16Reader::default()
                    .convert(content, self.format == UnicodeFormat::Utf16BeBom)
            }
        }
    }

    fn capture(path: &Path) -> io::Result<Self> {
        let path = fs::canonicalize(path)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document is not a regular file",
            ));
        }
        let mut input = fs::File::open(&path)?;
        let before = input.metadata()?;
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        let after = input.metadata()?;
        if !after.is_file()
            || before.len() != after.len()
            || before.modified()? != after.modified()?
            || after.len() != bytes.len() as u64
        {
            return Err(io::Error::other("document changed while loading"));
        }
        let format = UnicodeFormat::detect(&bytes);
        Ok(Self {
            path,
            bytes: bytes.into(),
            format,
            read_only: after.permissions().readonly(),
            original_modified: Some(after.modified()?),
            #[cfg(unix)]
            identity: (after.dev(), after.ino()),
        })
    }

    fn check_current(&self) -> io::Result<fs::Metadata> {
        let current = fs::symlink_metadata(&self.path)?;
        if !current.is_file() {
            return Err(io::Error::other(
                "document path no longer names the loaded file",
            ));
        }
        #[cfg(unix)]
        if (current.dev(), current.ino()) != self.identity {
            return Err(io::Error::other("document was replaced externally"));
        }
        if fs::read(&self.path)?.as_slice() != self.bytes.as_ref() {
            return Err(io::Error::other(
                "document changed externally; reload or save under a new name",
            ));
        }
        if current.permissions().readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "document is read-only",
            ));
        }
        Ok(current)
    }

    pub fn save_utf8(&self, snapshot: &SaveSnapshot) -> io::Result<Self> {
        if !matches!(self.format, UnicodeFormat::Utf8(_)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "explicit UTF-8 saving refuses a UTF-16 document",
            ));
        }
        self.save_unicode(snapshot)
    }

    pub fn save_unicode(&self, snapshot: &SaveSnapshot) -> io::Result<Self> {
        self.save_with_format(snapshot, self.format)
    }

    /// Write a selected format while preserving this handle's raw conflict baseline.
    pub fn save_with_format(
        &self,
        snapshot: &SaveSnapshot,
        format: UnicodeFormat,
    ) -> io::Result<Self> {
        self.save_encoded(format, || encode_snapshot(snapshot, format))
    }

    pub fn save_byte_buffer(
        &self,
        buffer: &crate::byte_buffer::ByteBuffer,
        format: UnicodeFormat,
    ) -> io::Result<Self> {
        self.save_encoded(format, || {
            let mut bytes = format.bom().to_vec();
            match format {
                UnicodeFormat::Utf8(_) => buffer.write_to(&mut bytes)?,
                UnicodeFormat::Utf16LeBom | UnicodeFormat::Utf16BeBom => {
                    crate::unicode_writer::write_utf16(
                        buffer.chunks().flat_map(|chunk| chunk.iter().copied()),
                        format == UnicodeFormat::Utf16BeBom,
                        &mut bytes,
                    )?;
                }
            }
            Ok(bytes)
        })
    }

    fn save_encoded(
        &self,
        format: UnicodeFormat,
        encode: impl FnOnce() -> io::Result<Vec<u8>>,
    ) -> io::Result<Self> {
        let metadata = self.check_current()?;
        let bytes = encode()?;
        #[cfg(unix)]
        if metadata.nlink() > 1 {
            return self.save_linked(bytes, format);
        }
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "document has no parent directory",
            )
        })?;
        let mut output = tempfile::Builder::new()
            .prefix(".notepad-matrix-save-")
            .tempfile_in(parent)?;
        output.write_all(&bytes)?;
        output.as_file().set_permissions(metadata.permissions())?;
        output.as_file().sync_all()?;
        let current = self.check_current()?;
        #[cfg(unix)]
        if current.nlink() > 1 {
            return self.save_linked(bytes, format);
        }
        let file = output.persist(&self.path).map_err(|error| error.error)?;
        let saved = file.metadata()?;
        self.saved_baseline(bytes, format, saved)
    }

    #[cfg(unix)]
    fn save_linked(&self, bytes: Vec<u8>, format: UnicodeFormat) -> io::Result<Self> {
        use std::io::{Seek, SeekFrom};
        let mut output = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)?;
        let metadata = output.metadata()?;
        if !metadata.is_file() || (metadata.dev(), metadata.ino()) != self.identity {
            return Err(io::Error::other("document was replaced externally"));
        }
        let mut observed = Vec::new();
        output.read_to_end(&mut observed)?;
        if observed.as_slice() != self.bytes.as_ref() {
            return Err(io::Error::other(
                "document changed externally; reload or save under a new name",
            ));
        }
        self.check_current()?;
        output.seek(SeekFrom::Start(0))?;
        output.set_len(0)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        self.saved_baseline(bytes, format, output.metadata()?)
    }

    fn saved_baseline(
        &self,
        bytes: Vec<u8>,
        format: UnicodeFormat,
        saved: fs::Metadata,
    ) -> io::Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            bytes: bytes.into(),
            format,
            read_only: saved.permissions().readonly(),
            original_modified: Some(saved.modified()?),
            #[cfg(unix)]
            identity: (saved.dev(), saved.ino()),
        })
    }
}
