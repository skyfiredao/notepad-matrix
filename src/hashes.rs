use sha2::Digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashAlgorithm {
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
    Some(match algorithm {
        HashAlgorithm::Md5 => format!("{:x}", md5::Md5::digest(bytes)),
        HashAlgorithm::Sha1 => sha1_smol::Sha1::from(bytes).digest().to_string(),
        HashAlgorithm::Sha256 => format!("{:x}", sha2::Sha256::digest(bytes)),
        HashAlgorithm::Sha512 => format!("{:x}", sha2::Sha512::digest(bytes)),
    })
}
