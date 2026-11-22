use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use crate::editor::SaveSnapshot;

mod oem_codepages;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteFormatCodePage {
    Unspecified,
    CodePage(u32),
}

impl ByteFormatCodePage {
    pub fn from_value(value: &serde_json::Value) -> io::Result<Option<Self>> {
        if value.is_null() { return Ok(None); }
        let value = value.as_i64().ok_or_else(|| io::Error::other("byte codepage must be an integer"))?;
        if value == -1 { return Ok(Some(Self::Unspecified)); }
        let value = u32::try_from(value).map_err(|_| io::Error::other("byte codepage is out of range"))?;
        if value == 0 { return Err(io::Error::other("Windows ACP is excluded; select an explicit codepage")); }
        Ok(Some(Self::CodePage(value)))
    }

    pub fn value(self) -> serde_json::Value {
        match self { Self::Unspecified => (-1).into(),Self::CodePage(value) => value.into() }
    }

    pub fn decode(self, bytes: &[u8]) -> io::Result<String> {
        if let Self::CodePage(codepage) = self {
            if let Some(encoding) = oem_codepages::for_codepage(codepage) {
                return Ok(encoding.decode(bytes));
            }
        }
        match self {
            Self::Unspecified => String::from_utf8(bytes.to_vec()).map_err(io::Error::other),
            Self::CodePage(codepage) => String::from_utf8(transcode_codepage(bytes,codepage,65001)?).map_err(io::Error::other),
        }
    }

    pub fn encode(self, text: &str) -> io::Result<Vec<u8>> {
        if let Self::CodePage(codepage) = self {
            if let Some(encoding) = oem_codepages::for_codepage(codepage) {
                return Ok(encoding.encode(text));
            }
        }
        match self {
            Self::Unspecified => Ok(text.as_bytes().to_vec()),
            Self::CodePage(codepage) => transcode_codepage(text.as_bytes(),65001,codepage),
        }
    }
}

#[cfg(target_os = "macos")]
fn transcode_codepage(bytes: &[u8], from: u32, to: u32) -> io::Result<Vec<u8>> {
    use std::ffi::c_void;
    #[repr(C)]
    struct CFRange { location: isize,length: isize }
    #[link(name = "CoreFoundation",kind = "framework")]
    unsafe extern "C" {
        fn CFStringConvertWindowsCodepageToEncoding(codepage: u32) -> u32;
        fn CFStringIsEncodingAvailable(encoding: u32) -> u8;
        fn CFStringCreateWithBytes(allocator: *const c_void,bytes: *const u8,length: isize,encoding: u32,external: u8) -> *const c_void;
        fn CFStringGetLength(value: *const c_void) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize,encoding: u32) -> isize;
        fn CFStringGetBytes(value: *const c_void,range: CFRange,encoding: u32,loss_byte: u8,external: u8,buffer: *mut u8,size: isize,used: *mut isize) -> isize;
        fn CFRelease(value: *const c_void);
    }
    let length = isize::try_from(bytes.len()).map_err(|_| io::Error::other("encoded text is too large"))?;
    unsafe {
        let input_encoding = CFStringConvertWindowsCodepageToEncoding(from);
        let output_encoding = CFStringConvertWindowsCodepageToEncoding(to);
        if input_encoding == u32::MAX || output_encoding == u32::MAX
            || CFStringIsEncodingAvailable(input_encoding) == 0 || CFStringIsEncodingAvailable(output_encoding) == 0
        { return Err(io::Error::other(format!("macOS cannot convert codepage {from} to {to}"))); }
        let text = CFStringCreateWithBytes(std::ptr::null(),bytes.as_ptr(),length,input_encoding,0);
        if text.is_null() && from != 65001 && to == 65001 {
            let mut result = Vec::new();
            let mut offset = 0;
            while offset < bytes.len() {
                let mut character_length = None;
                for size in 1..=4.min(bytes.len() - offset) {
                    let size_native = isize::try_from(size).map_err(io::Error::other)?;
                    let character = CFStringCreateWithBytes(std::ptr::null(),bytes[offset..].as_ptr(),size_native,input_encoding,0);
                    if !character.is_null() {
                        CFRelease(character);
                        character_length = Some(size);
                        break;
                    }
                }
                if let Some(size) = character_length {
                    result.extend(transcode_codepage(&bytes[offset..offset + size],from,to)?);
                    offset += size;
                } else {
                    result.extend_from_slice("\u{fffd}".as_bytes());
                    offset += 1;
                }
            }
            return Ok(result);
        }
        if text.is_null() { return Err(io::Error::other(format!("input is not valid codepage {from}"))); }
        let characters = CFStringGetLength(text);
        let capacity = CFStringGetMaximumSizeForEncoding(characters,output_encoding);
        let capacity_usize = match usize::try_from(capacity) {
            Ok(capacity) => capacity,
            Err(error) => { CFRelease(text); return Err(io::Error::other(error)); }
        };
        let mut result = vec![0u8;capacity_usize];
        let mut used = 0isize;
        let converted = CFStringGetBytes(text,CFRange { location:0,length:characters },output_encoding,b'?',0,result.as_mut_ptr(),capacity,&mut used);
        CFRelease(text);
        if converted != characters { return Err(io::Error::other(format!("text cannot be represented in codepage {to}"))); }
        let used = usize::try_from(used).map_err(io::Error::other)?;
        result.truncate(used);
        Ok(result)
    }
}

#[cfg(all(target_os = "macos", feature = "native-ui"))]
fn detect_native_codepage(bytes: &[u8]) -> io::Result<Option<u32>> {
    use objc2::{class,msg_send};
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSData,NSString};
    #[link(name = "CoreFoundation",kind = "framework")]
    unsafe extern "C" {
        fn CFStringConvertNSStringEncodingToEncoding(encoding: usize) -> u32;
        fn CFStringConvertEncodingToWindowsCodepage(encoding: u32) -> u32;
    }
    if bytes.is_ascii() || std::str::from_utf8(bytes).is_ok() { return Ok(None); }
    objc2::rc::autoreleasepool(|_| unsafe {
        let data = NSData::with_bytes(bytes);
        let options: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let mut converted: *mut NSString = std::ptr::null_mut();
        let mut lossy = objc2::runtime::Bool::NO;
        let encoding: usize = msg_send![class!(NSString), stringEncodingForData: &*data, encodingOptions: options, convertedString: &mut converted, usedLossyConversion: &mut lossy];
        if encoding == 0 || lossy.as_bool() || converted.is_null() { return Ok(None); }
        let codepage = CFStringConvertEncodingToWindowsCodepage(CFStringConvertNSStringEncodingToEncoding(encoding));
        if codepage == 0 || matches!(codepage,65001 | 20127 | 874) { return Ok(None); }
        Ok(Some(codepage))
    })
}

#[cfg(not(all(target_os = "macos", feature = "native-ui")))]
fn detect_native_codepage(_bytes: &[u8]) -> io::Result<Option<u32>> {
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
fn transcode_codepage(bytes: &[u8], from: u32, to: u32) -> io::Result<Vec<u8>> {
    fn encoding(codepage: u32) -> io::Result<&'static encoding_rs::Encoding> {
        let name = match codepage {
            65001 => "UTF-8",1250 => "windows-1250",1251 => "windows-1251",1252 => "windows-1252",
            1253 => "windows-1253",1254 => "windows-1254",1255 => "windows-1255",1256 => "windows-1256",1257 => "windows-1257",1258 => "windows-1258",
            874 => "windows-874",932 => "shift_jis",936 => "gbk",949 => "euc-kr",950 => "big5",866 => "ibm866",
            20866 => "koi8-r",21866 => "koi8-u",10007 => "x-mac-cyrillic",51949 => "euc-kr",
            28591 => "windows-1252",28592 => "iso-8859-2",28593 => "iso-8859-3",28594 => "iso-8859-4",28595 => "iso-8859-5",28596 => "iso-8859-6",28597 => "iso-8859-7",28598 => "iso-8859-8",28599 => "windows-1254",28603 => "iso-8859-13",28604 => "iso-8859-14",28605 => "iso-8859-15",
            _ => return Err(io::Error::other(format!("codepage {codepage} requires the macOS native converter"))),
        };
        encoding_rs::Encoding::for_label(name.as_bytes()).ok_or_else(|| io::Error::other("codepage is unavailable"))
    }
    let input = encoding(from)?;
    let text = input.decode_without_bom_handling_and_without_replacement(bytes).ok_or_else(|| io::Error::other("encoded input is invalid"))?;
    let (output,_,errors) = encoding(to)?.encode(&text);
    if errors { return Err(io::Error::other("text is not representable in the selected codepage")); }
    Ok(output.into_owned())
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

fn encode_byte_snapshot(snapshot: &SaveSnapshot, format: UnicodeFormat, codepage: Option<ByteFormatCodePage>) -> io::Result<Vec<u8>> {
    match codepage {
        None => encode_snapshot(snapshot,format),
        Some(ByteFormatCodePage::Unspecified) => { let mut bytes = Vec::new(); snapshot.write_to(&mut bytes)?; Ok(bytes) }
        Some(codepage) => {
            let mut bytes = Vec::new(); snapshot.write_to(&mut bytes)?;
            let text = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
            codepage.encode(text)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwritePolicy {
    Refuse,
    Confirmed,
}

#[derive(Clone)]
pub struct DiskFile {
    path: PathBuf,
    _access: Option<Arc<crate::file_access::AccessGuard>>,
    bytes: Arc<[u8]>,
    format: UnicodeFormat,
    read_only: bool,
    original_modified: Option<SystemTime>,
    externally_deleted: bool,
    byte_code_page: Option<ByteFormatCodePage>,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl DiskFile {
    pub(crate) fn placeholder(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() { return Err(io::Error::other("placeholder path must be absolute")); }
        Ok(Self {
            path: path.to_path_buf(),
            _access: None,
            bytes: Arc::from([]),
            format: UnicodeFormat::Utf8(Utf8Format::Plain),
            read_only: true,
            original_modified: None,
            externally_deleted: false,
            byte_code_page: None,
            #[cfg(unix)]
            identity: (0,0),
        })
    }

    pub fn is_placeholder(&self) -> bool {
        #[cfg(unix)]
        { self.identity == (0,0) && self.original_modified.is_none() }
        #[cfg(not(unix))]
        { self.original_modified.is_none() && self.bytes.is_empty() && self.read_only }
    }

    pub(crate) fn mark_deleted(&mut self) {
        self.externally_deleted = true;
        self.read_only = false;
    }

    pub fn byte_code_page(&self) -> Option<ByteFormatCodePage> { self.byte_code_page }

    pub fn set_byte_code_page(&mut self,value: Option<ByteFormatCodePage>) { self.byte_code_page = value; }

    pub(crate) fn identity(&self) -> Option<(u64,u64)> {
        #[cfg(unix)]
        { Some(self.identity) }
        #[cfg(not(unix))]
        { None }
    }

    pub(crate) fn loaded_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn relocate_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    pub(crate) fn after_move(&self, path: &Path, access: crate::file_access::AccessGuard) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        let mut file = self.clone();
        file._access = Some(Arc::new(access));
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

    #[cfg(unix)]
    pub fn toggle_read_only_attribute(&mut self) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let _access = crate::file_access::directory(self.path.parent().unwrap_or_else(|| Path::new(".")))?;
        let file = fs::File::open(&self.path)?;
        let metadata = file.metadata()?;
        if (metadata.dev(), metadata.ino()) != self.identity {
            return Err(io::Error::other("文件已被替换，未更改只读属性"));
        }
        let mode = metadata.permissions().mode();
        let read_only = mode & 0o222 == 0;
        let next = if read_only {
            mode | 0o200
        } else {
            mode & !0o222
        };
        file.set_permissions(fs::Permissions::from_mode(next))?;
        self.read_only = !read_only;
        Ok(())
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
        let _access = crate::file_access::existing(&self.path)?;
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
            "read_only": self.read_only,
            "original_modified": self.original_modified.map(encode_modification_time),
            "codepage": self.byte_code_page.map(ByteFormatCodePage::value),
        });
        #[cfg(unix)]
        {
            state["device"] = self.identity.0.into();
            state["inode"] = self.identity.1.into();
        }
        Ok(state)
    }

    pub(crate) fn from_recovery_state(state: &serde_json::Value, raw_utf8: bool, bytes: Option<Vec<u8>>) -> io::Result<Self> {
        let invalid =
            || io::Error::new(io::ErrorKind::InvalidData, "invalid recovery disk baseline");
        let path: PathBuf = serde_json::from_value(state.get("path").ok_or_else(invalid)?.clone())?;
        if !path.is_absolute() {
            return Err(invalid());
        }
        let access = match crate::file_access::read(&path) {
            Ok(access) => Some(Arc::new(access)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let bytes = match bytes {
            Some(bytes) => bytes,
            None => serde_json::from_value(state.get("bytes").ok_or_else(invalid)?.clone())?,
        };
        let read_only = state
            .get("read_only")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(invalid)?;
        let format = UnicodeFormat::detect(&bytes);
        let file = Self {
            path,
            _access: access,
            bytes: bytes.into(),
            format,
            read_only,
            original_modified: decode_modification_time(
                state
                    .get("original_modified")
                    .unwrap_or(&serde_json::Value::Null),
            )?,
            externally_deleted: false,
            byte_code_page: ByteFormatCodePage::from_value(&state["codepage"] )?,
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
        let raw_baseline = raw_utf8 && file.byte_code_page.is_none()
            && file.format == UnicodeFormat::Utf8(Utf8Format::Plain);
        if !raw_baseline && file.byte_code_page != Some(ByteFormatCodePage::Unspecified) { file.decode_text()?; }
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
        let _access = crate::file_access::directory(path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
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

    pub fn save_as_with_byte_format(path: &Path,snapshot: &SaveSnapshot,format: UnicodeFormat,codepage: Option<ByteFormatCodePage>,overwrite: OverwritePolicy) -> io::Result<Self> {
        let _access = crate::file_access::directory(path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
        match fs::symlink_metadata(path) {
            Ok(_) if overwrite == OverwritePolicy::Refuse => Err(io::Error::new(io::ErrorKind::AlreadyExists,"overwrite requires explicit confirmation")),
            Ok(_) => Self::capture(path)?.save_with_byte_format(snapshot,format,codepage),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Self::create_encoded(path,encode_byte_snapshot(snapshot,format,codepage)?,format,codepage)
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
        Self::create_encoded(path,encode_snapshot(snapshot,format)?,format,None)
    }

    fn create_encoded(path: &Path,bytes: Vec<u8>,format: UnicodeFormat,codepage: Option<ByteFormatCodePage>) -> io::Result<Self> {
        let _access = crate::file_access::directory(path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
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
        output.write_all(&bytes)?;
        output.as_file().sync_all()?;
        let file = output
            .persist_noclobber(&path)
            .map_err(|error| error.error)?;
        let saved = file.metadata()?;
        Ok(Self {
            path,
            _access: Some(Arc::new(_access)),
            bytes: bytes.into(),
            format,
            read_only: saved.permissions().readonly(),
            original_modified: Some(saved.modified()?),
            externally_deleted: false,
            byte_code_page: codepage,
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

    pub fn load_with_preferences(path: &Path,settings: &serde_json::Value) -> io::Result<(Self,String)> {
        let file = Self::capture_with_preferences(path, settings)?;
        let text = file.decode_text()?;
        Ok((file,text))
    }

    pub(crate) fn load_buffer_with_preferences(path: &Path, settings: &serde_json::Value) -> io::Result<(Self, crate::byte_buffer::ByteBuffer)> {
        let file = Self::capture_with_preferences(path, settings)?;
        let buffer = file.editor_buffer()?;
        Ok((file, buffer))
    }

    pub fn load_with_code_page(path: &Path,codepage: Option<ByteFormatCodePage>) -> io::Result<(Self,String)> {
        let mut file = Self::capture(path)?;
        if file.format == UnicodeFormat::Utf8(Utf8Format::Plain) { file.byte_code_page = codepage; }
        let text = file.decode_text()?;
        Ok((file,text))
    }

    pub fn load_selected_encoding(path: &Path, codepage: Option<ByteFormatCodePage>) -> io::Result<(Self, crate::byte_buffer::ByteBuffer)> {
        let mut file = Self::capture(path)?;
        if file.format == UnicodeFormat::Utf8(Utf8Format::Plain) {
            file.byte_code_page = codepage;
        }
        let buffer = file.editor_buffer()?;
        Ok((file, buffer))
    }

    pub fn load_unicode_bytes(path: &Path) -> io::Result<(Self, Vec<u8>)> {
        let file = Self::capture(path)?;
        let bytes = file.decoded_bytes();
        Ok((file, bytes))
    }

    pub fn load_byte_buffer(path: &Path) -> io::Result<(Self, crate::byte_buffer::ByteBuffer)> {
        let file = Self::capture(path)?;
        let buffer = file.editor_buffer()?;
        Ok((file, buffer))
    }

    fn decode_text(&self) -> io::Result<String> {
        if let Some(codepage) = self.byte_code_page { return codepage.decode(&self.bytes); }
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

    fn editor_buffer(&self) -> io::Result<crate::byte_buffer::ByteBuffer> {
        let (bytes, start): (Arc<[u8]>, usize) = match self.byte_code_page {
            Some(ByteFormatCodePage::CodePage(_)) => (self.decode_text()?.into_bytes().into(), 0),
            _ if matches!(self.format, UnicodeFormat::Utf8(_)) => {
                (self.bytes.clone(), self.format.bom().len())
            }
            _ => (self.decoded_bytes().into(), 0),
        };
        let end = bytes.len();
        crate::byte_buffer::ByteBuffer::from_shared_bytes(bytes, start..end).map_err(io::Error::other)
    }

    fn capture_with_preferences(path: &Path, settings: &serde_json::Value) -> io::Result<Self> {
        let mut file = Self::capture(path)?;
        if file.format == UnicodeFormat::Utf8(Utf8Format::Plain) && settings["DetectEncoding"].as_bool().unwrap_or(true) {
            if let Some(codepage) = detect_native_codepage(&file.bytes)? { file.byte_code_page = Some(ByteFormatCodePage::CodePage(codepage)); }
        }
        Ok(file)
    }

    fn capture(path: &Path) -> io::Result<Self> {
        let _access = crate::file_access::read(path)?;
        let path = fs::canonicalize(path)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document is not a regular file",
            ));
        }
        let mut input = fs::File::open(&path)?;
        let before = input.metadata()?;
        let file_size = usize::try_from(before.len()).map_err(io::Error::other)?;
        let capacity = file_size.checked_add((1 << 20).min(file_size / 6))
            .ok_or_else(|| io::Error::other("document is too large"))?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).map_err(io::Error::other)?;
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
            _access: Some(Arc::new(_access)),
            bytes: bytes.into(),
            format,
            read_only: after.permissions().readonly(),
            original_modified: Some(after.modified()?),
            externally_deleted: false,
            byte_code_page: None,
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
        self.save_with_byte_format(snapshot,format,self.byte_code_page)
    }

    pub fn save_with_byte_format(&self,snapshot: &SaveSnapshot,format: UnicodeFormat,codepage: Option<ByteFormatCodePage>) -> io::Result<Self> {
        let mut selected = self.clone();
        selected.byte_code_page = codepage;
        selected.save_encoded(format,|| encode_byte_snapshot(snapshot,format,codepage))
    }

    pub fn save_byte_buffer(
        &self,
        buffer: &crate::byte_buffer::ByteBuffer,
        format: UnicodeFormat,
    ) -> io::Result<Self> {
        self.save_encoded(format, || {
            if let Some(codepage) = self.byte_code_page {
                let mut input = Vec::new(); buffer.write_to(&mut input)?;
                if codepage == ByteFormatCodePage::Unspecified { return Ok(input); }
                return codepage.encode(std::str::from_utf8(&input).map_err(io::Error::other)?);
            }
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
        let access = Arc::new(crate::file_access::directory(self.path.parent().unwrap_or_else(|| Path::new(".")))?);
        if self.externally_deleted {
            let bytes = encode()?;
            let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&self.path)?;
            output.write_all(&bytes)?;
            output.sync_all()?;
            return self.saved_baseline(bytes,format,output.metadata()?,access);
        }
        let metadata = self.check_current()?;
        let bytes = encode()?;
        #[cfg(unix)]
        if metadata.nlink() > 1 {
            return self.save_linked(bytes, format, access);
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
            return self.save_linked(bytes, format, access);
        }
        let file = output.persist(&self.path).map_err(|error| error.error)?;
        let saved = file.metadata()?;
        self.saved_baseline(bytes, format, saved, access)
    }

    #[cfg(unix)]
    fn save_linked(&self, bytes: Vec<u8>, format: UnicodeFormat, access: Arc<crate::file_access::AccessGuard>) -> io::Result<Self> {
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
        self.saved_baseline(bytes, format, output.metadata()?, access)
    }

    fn saved_baseline(
        &self,
        bytes: Vec<u8>,
        format: UnicodeFormat,
        saved: fs::Metadata,
        access: Arc<crate::file_access::AccessGuard>,
    ) -> io::Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            _access: Some(access),
            bytes: bytes.into(),
            format,
            read_only: saved.permissions().readonly(),
            original_modified: Some(saved.modified()?),
            externally_deleted: false,
            byte_code_page: self.byte_code_page,
            #[cfg(unix)]
            identity: (saved.dev(), saved.ino()),
        })
    }
}
