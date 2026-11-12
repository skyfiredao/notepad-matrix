use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MimeOperation {
    Base64Encode,
    Base64EncodePadded,
    Base64Decode,
}

#[derive(Debug)]
pub enum MimeError {
    UnsupportedBinaryOutput,
}
impl fmt::Display for MimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "Decoded binary bytes are not yet supported by the text document; no text was changed",
        )
    }
}
impl std::error::Error for MimeError {}

fn encode(input: &[u8], padding: bool) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in input.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        result.push(char::from(ALPHABET[((bits >> 18) & 63) as usize]));
        result.push(char::from(ALPHABET[((bits >> 12) & 63) as usize]));
        if chunk.len() > 1 {
            result.push(char::from(ALPHABET[((bits >> 6) & 63) as usize]));
        } else if padding {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(char::from(ALPHABET[(bits & 63) as usize]));
        } else if padding {
            result.push('=');
        }
    }
    result
}

/// MIME Tools' default decoder preserves illegal separator bytes and ignores
/// only TAB/LF/CR/SPACE and padding. It is not the stricter RFC decoder mode.
pub fn decode_base64(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < input.len() {
        let mut bits = 0_u32;
        let mut shift = 18_i32;
        let mut separator = None;
        while shift >= 0 && index < input.len() {
            let byte = input[index];
            index += 1;
            let value = match byte & 0x7f {
                b'A'..=b'Z' => Some(u32::from((byte & 0x7f) - b'A')),
                b'a'..=b'z' => Some(u32::from((byte & 0x7f) - b'a') + 26),
                b'0'..=b'9' => Some(u32::from((byte & 0x7f) - b'0') + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                b'=' | b'\t' | b'\n' | b'\r' | b' ' => None,
                _ => {
                    separator = Some(byte);
                    break;
                }
            };
            if let Some(value) = value {
                bits |= value << shift;
                shift -= 6;
            }
        }
        for output_shift in [16_i32, 8, 0] {
            if output_shift > shift + 3 {
                result.push(((bits >> output_shift) & 255) as u8);
            }
        }
        if let Some(byte) = separator {
            result.push(byte);
        }
    }
    result
}

pub fn transform(text: &str, operation: MimeOperation) -> Result<String, MimeError> {
    match operation {
        MimeOperation::Base64Encode => Ok(encode(text.as_bytes(), false)),
        MimeOperation::Base64EncodePadded => Ok(encode(text.as_bytes(), true)),
        MimeOperation::Base64Decode => String::from_utf8(decode_base64(text.as_bytes()))
            .map_err(|_| MimeError::UnsupportedBinaryOutput),
    }
}
