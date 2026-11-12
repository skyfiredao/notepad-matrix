use crate::input::LineEnding;
// Behavior rewritten in Rust from Converter v4.7, commit
// d3837f3e0d18d10436401499c6fb3b89b19cd08b, source/PluginDefinition.cpp.
// Upstream Copyright (C)2022 Don HO <don.h@free.fr>.
// SPDX-License-Identifier: GPL-3.0-or-later
// GPL terms are retained in assets/notepad-plus-plus/LICENSE; no DLL is linked.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConverterOperation {
    AsciiToHex,
    HexToAscii,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConverterError {
    InvalidHex,
    UnsupportedBinaryOutput,
}

impl fmt::Display for ConverterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidHex => "Hex format is not conformed",
            Self::UnsupportedBinaryOutput => "Decoded binary bytes are not yet supported by the text document; no text was changed",
        })
    }
}
impl std::error::Error for ConverterError {}

/// Fresh Converter v4.7 defaults: no spaces, uppercase, EOL every 16 input bytes.
pub fn ascii_to_hex(input: &[u8], eol: LineEnding) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for (index, &byte) in input.iter().enumerate() {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 15)]));
        if (index + 1) % 16 == 0 {
            output.push_str(eol.as_str());
        }
    }
    output
}

enum HexState {
    Start,
    Half(u8),
    Byte,
}

/// Source parser's spacing mode is chosen by byte 2 for inputs of length >= 5.
/// TAB is invalid; spaces must occur singly between every pair in spaced mode.
pub fn hex_to_ascii(input: &[u8]) -> Result<Vec<u8>, ConverterError> {
    if input.len() < 2 {
        return Err(ConverterError::InvalidHex);
    }
    let spaced = input.len() >= 5 && input[2] == b' ';
    let mut state = HexState::Start;
    let mut output = Vec::new();
    for &byte in input {
        if byte == 0 {
            break;
        }
        match byte {
            b' ' if spaced && matches!(state, HexState::Byte) => state = HexState::Start,
            b'\r' | b'\n' if !matches!(state, HexState::Half(_)) => state = HexState::Start,
            b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                let nibble = match byte {
                    b'0'..=b'9' => byte - b'0',
                    b'a'..=b'f' => byte - b'a' + 10,
                    _ => byte - b'A' + 10,
                };
                state = match state {
                    HexState::Start => HexState::Half(nibble),
                    HexState::Half(high) => {
                        output.push(high * 16 + nibble);
                        HexState::Byte
                    }
                    HexState::Byte if !spaced => HexState::Half(nibble),
                    HexState::Byte => return Err(ConverterError::InvalidHex),
                };
            }
            _ => return Err(ConverterError::InvalidHex),
        }
    }
    if matches!(state, HexState::Half(_)) {
        return Err(ConverterError::InvalidHex);
    }
    Ok(output)
}

pub fn transform(
    text: &str,
    operation: ConverterOperation,
    eol: LineEnding,
) -> Result<String, ConverterError> {
    match operation {
        ConverterOperation::AsciiToHex => Ok(ascii_to_hex(text.as_bytes(), eol)),
        ConverterOperation::HexToAscii => String::from_utf8(hex_to_ascii(text.as_bytes())?)
            .map_err(|_| ConverterError::UnsupportedBinaryOutput),
    }
}
