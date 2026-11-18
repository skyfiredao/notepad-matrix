use sha2::Digest;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashAlgorithm {
    pub const ALL: [Self; 4] = [Self::Md5, Self::Sha1, Self::Sha256, Self::Sha512];

    pub fn index(self) -> usize {
        match self { Self::Md5 => 0, Self::Sha1 => 1, Self::Sha256 => 2, Self::Sha512 => 3 }
    }

    pub fn dialog_key(self, files: bool) -> &'static str {
        match (self, files) {
            (Self::Md5, false) => "MD5FromTextDlg",
            (Self::Md5, true) => "MD5FromFilesDlg",
            (Self::Sha1, false) => "SHA1FromTextDlg",
            (Self::Sha1, true) => "SHA1FromFilesDlg",
            (Self::Sha256, false) => "SHA256FromTextDlg",
            (Self::Sha256, true) => "SHA256FromFilesDlg",
            (Self::Sha512, false) => "SHA512FromTextDlg",
            (Self::Sha512, true) => "SHA512FromFilesDlg",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
        }
    }
}

/// NppCommands.cpp:3598–3617,3677–3729 hashes one nonempty selection using
/// strlen over the internal bytes, then copies lowercase hex. This reproduces
/// its first-NUL contract rather than hashing the document's disk encoding.
/// MD5/SHA-1 are compatibility checksum tools, not authentication primitives.
pub fn selection_digest(text: &str, algorithm: HashAlgorithm) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let bytes = text.split('\0').next().unwrap_or_default().as_bytes();
    Some(digest_bytes(bytes, algorithm))
}

pub fn selection_buffer_digest(bytes: &crate::byte_buffer::ByteBuffer, algorithm: HashAlgorithm) -> Option<String> {
    if bytes.is_empty() { return None; }
    let mut hash = StreamDigest::new(algorithm);
    for chunk in bytes.chunks() {
        if let Some(end) = chunk.iter().position(|byte| *byte == 0) {
            hash.update(&chunk[..end]);
            break;
        }
        hash.update(chunk);
    }
    Some(hash.finish())
}

fn digest_bytes(bytes: &[u8], algorithm: HashAlgorithm) -> String {
    match algorithm {
        HashAlgorithm::Md5 => format!("{:x}", md5::Md5::digest(bytes)),
        HashAlgorithm::Sha1 => sha1_smol::Sha1::from(bytes).digest().to_string(),
        HashAlgorithm::Sha256 => format!("{:x}", sha2::Sha256::digest(bytes)),
        HashAlgorithm::Sha512 => format!("{:x}", sha2::Sha512::digest(bytes)),
    }
}

pub fn text_digest(text: &str, algorithm: HashAlgorithm, each_line: bool) -> String {
    let text = text.split('\0').next().unwrap_or_default();
    if text.is_empty() { return String::new(); }
    if !each_line { return digest_bytes(text.as_bytes(), algorithm); }
    let mut result = String::new();
    for line in text.split_terminator('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.is_empty() { result.push_str(&digest_bytes(line.as_bytes(), algorithm)); }
        result.push_str("\r\n");
    }
    result
}

enum StreamDigest {
    Md5(md5::Md5),
    Sha1(sha1_smol::Sha1),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl StreamDigest {
    fn new(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Md5 => Self::Md5(md5::Md5::new()),
            HashAlgorithm::Sha1 => Self::Sha1(sha1_smol::Sha1::new()),
            HashAlgorithm::Sha256 => Self::Sha256(sha2::Sha256::new()),
            HashAlgorithm::Sha512 => Self::Sha512(sha2::Sha512::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Md5(hash) => hash.update(bytes),
            Self::Sha1(hash) => hash.update(bytes),
            Self::Sha256(hash) => hash.update(bytes),
            Self::Sha512(hash) => hash.update(bytes),
        }
    }

    fn finish(self) -> String {
        match self {
            Self::Md5(hash) => format!("{:x}", hash.finalize()),
            Self::Sha1(hash) => hash.digest().to_string(),
            Self::Sha256(hash) => format!("{:x}", hash.finalize()),
            Self::Sha512(hash) => format!("{:x}", hash.finalize()),
        }
    }
}

fn file_digest(path: &Path, algorithm: HashAlgorithm, cancelled: &AtomicBool) -> io::Result<String> {
    let _access = crate::file_access::read(path)?;
    let mut file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() { return Err(io::Error::other("不是普通文件")); }
    let mut hash = StreamDigest::new(algorithm);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "摘要计算已取消"));
        }
        let count = match file.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 { break; }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finish())
}

pub struct FileDigests {
    pub paths: String,
    pub results: String,
    pub failures: Vec<String>,
    pub replace: bool,
}

pub fn files_digest(paths: &[PathBuf], algorithm: HashAlgorithm, cancelled: &AtomicBool) -> FileDigests {
    let mut output = FileDigests { paths: String::new(), results: String::new(), failures: Vec::new(), replace: false };
    for path in paths {
        if cancelled.load(Ordering::Relaxed) { return output; }
        match file_digest(path, algorithm, cancelled) {
            Ok(digest) => {
                output.paths.push_str(&path.to_string_lossy());
                output.paths.push_str("\r\n");
                output.results.push_str(&digest);
                output.results.push_str("  ");
                output.results.push_str(&path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
                output.results.push_str("\r\n");
            }
            Err(error) => {
                output.failures.push(format!("{}：{error}", path.display()));
                if algorithm != HashAlgorithm::Md5 { return output; }
            }
        }
    }
    output.replace = !output.paths.is_empty() && !output.results.is_empty();
    output
}
