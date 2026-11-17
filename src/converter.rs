use crate::input::LineEnding;
// Behavior rewritten in Rust from Converter v4.7, commit
// d3837f3e0d18d10436401499c6fb3b89b19cd08b, source/PluginDefinition.cpp.
// Upstream Copyright (C)2022 Don HO <don.h@free.fr>.
// SPDX-License-Identifier: GPL-3.0-or-later
// GPL terms are retained in assets/notepad-plus-plus/LICENSE; no DLL is linked.

use std::{fmt, fs, io, io::Write, path::{Path, PathBuf}};

pub const ABOUT: &str = "Version: 4.7\n\nLicense: GPL\n\nAuthor: Don Ho <don.h@free.fr>";

#[derive(Clone, Copy, Debug)]
pub struct Configuration {
    pub insert_space: bool,
    pub uppercase: bool,
    pub characters_per_line: usize,
}

impl Default for Configuration {
    fn default() -> Self {
        Self { insert_space: false, uppercase: true, characters_per_line: 16 }
    }
}

impl Configuration {
    pub fn path(store: &crate::config::ConfigStore) -> io::Result<PathBuf> {
        let settings = store.document_path(Path::new("settings.json"))?;
        let root = settings.parent().ok_or_else(|| io::Error::other("Missing private configuration root"))?;
        Ok(root.join("converter.ini"))
    }

    pub fn load(store: &crate::config::ConfigStore) -> io::Result<Self> {
        let path = Self::path(store)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(io::Error::other("converter.ini must be a regular private configuration file"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&path)?;
                file.write_all(b"[ascii2Hex]\ninsertSpace=0\nuppercase=1\nnbCharPerLine=16\n")?;
                file.sync_all()?;
            }
            Err(error) => return Err(error),
        }
        let bytes = fs::read(&path)?;
        let text = if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
            if bytes.len() % 2 != 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "converter.ini contains incomplete UTF-16"));
            }
            let units = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect::<Vec<_>>();
            String::from_utf16(&units).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        } else {
            String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        };
        let mut section = None;
        let mut values = [None; 3];
        let mut in_section = false;
        for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
            if line.starts_with(';') || line.is_empty() { continue; }
            if line.starts_with('[') && line.ends_with(']') {
                if section.is_none() {
                    section = Some(line[1..line.len() - 1].to_owned());
                    if section.as_deref() != Some("ascii2Hex") { return Ok(Self { characters_per_line: 0, ..Self::default() }); }
                }
                in_section = line[1..line.len() - 1].eq_ignore_ascii_case("ascii2Hex");
                continue;
            }
            if !in_section { continue; }
            let Some((key, value)) = line.split_once('=') else { continue; };
            let index = if key.trim().eq_ignore_ascii_case("insertSpace") { 0 }
                else if key.trim().eq_ignore_ascii_case("uppercase") { 1 }
                else if key.trim().eq_ignore_ascii_case("nbCharPerLine") { 2 }
                else { continue; };
            if values[index].is_none() { values[index] = Some(profile_integer(value)); }
        }
        if section.is_none() { return Ok(Self { characters_per_line: 0, ..Self::default() }); }
        Ok(Self {
            insert_space: values[0].unwrap_or(0) != 0,
            uppercase: values[1].unwrap_or(0) != 0,
            characters_per_line: (values[2].unwrap_or(0) as i32 as isize) as usize,
        })
    }
}

fn profile_integer(value: &str) -> u32 {
    let value = value.trim().trim_matches('"');
    let (negative, value) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let (base, digits) = if let Some(value) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) { (16, value) } else { (10, value) };
    let mut number = 0u32;
    for ch in digits.chars() {
        let Some(digit) = ch.to_digit(base) else { break; };
        number = number.wrapping_mul(base).wrapping_add(digit);
    }
    if negative { number.wrapping_neg() } else { number }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field { Ascii, Decimal, Hexadecimal, Binary, Octal }

impl Field {
    pub const ALL: [Self; 5] = [Self::Ascii, Self::Decimal, Self::Hexadecimal, Self::Binary, Self::Octal];
    pub fn index(self) -> usize {
        match self { Self::Ascii => 0, Self::Decimal => 1, Self::Hexadecimal => 2, Self::Binary => 3, Self::Octal => 4 }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Ascii => "ASCII:", Self::Decimal => "Decimal:", Self::Hexadecimal => "Hexadecimal:", Self::Binary => "Binary:", Self::Octal => "Octal:" }
    }
    pub fn selector(self) -> &'static str {
        match self { Self::Ascii => "converter-ascii", Self::Decimal => "converter-decimal", Self::Hexadecimal => "converter-hexadecimal", Self::Binary => "converter-binary", Self::Octal => "converter-octal" }
    }
    fn radix(self) -> u32 {
        match self { Self::Ascii => 0, Self::Decimal => 10, Self::Hexadecimal => 16, Self::Binary => 2, Self::Octal => 8 }
    }
}

#[derive(Default)]
pub struct PanelValues {
    pub text: [String; 5],
    pub ascii_info: &'static str,
}

impl PanelValues {
    pub fn set_from(&mut self, field: Field, text: String) {
        let index = field.index();
        self.text[index] = text;
        loop {
            for other in Field::ALL {
                if other != field { self.text[other.index()].clear(); }
            }
            let units = self.text[index].encode_utf16().take(255).take_while(|unit| *unit != 0).collect::<Vec<_>>();
            if units.is_empty() { return; }
            let qualified = field == Field::Ascii || units.iter().all(|unit| char::from_u32(u32::from(*unit)).is_some_and(|ch| ch.is_ascii() && ch.is_digit(field.radix())));
            if !qualified {
                self.text[index] = String::from_utf16_lossy(&units[..units.len() - 1]);
                continue;
            }
            let value = if field == Field::Ascii { u32::from(units[0]) }
                else { units.iter().fold(0u32, |value, unit| value.saturating_mul(field.radix()).saturating_add(char::from_u32(u32::from(*unit)).and_then(|ch| ch.to_digit(field.radix())).unwrap_or(0))) };
            if field == Field::Ascii || value <= 255 {
                self.ascii_info = ascii_info(value as u8);
                if field != Field::Ascii && value != 0 { self.text[0] = char::from(value as u8).to_string(); }
            } else { self.ascii_info = ""; }
            if field != Field::Decimal { self.text[1] = value.to_string(); }
            if field != Field::Hexadecimal { self.text[2] = format!("{value:X}"); }
            if field != Field::Binary { self.text[3] = format!("{value:b}"); }
            if field != Field::Octal { self.text[4] = format!("{value:o}"); }
            return;
        }
    }

    pub fn bytes(&self, field: Field) -> Option<Vec<u8>> {
        if field == Field::Ascii {
            let value = self.text[1].bytes().take(255).take_while(u8::is_ascii_digit).fold(0u32, |value, byte| value.saturating_mul(10).saturating_add(u32::from(byte - b'0'))) as i32;
            if value == -1 || value > 255 { None } else { Some(vec![value as u8]) }
        } else { Some(self.text[field.index()].bytes().take(255).take_while(|byte| *byte != 0).collect()) }
    }
}

fn ascii_info(value: u8) -> &'static str {
    const CONTROL: [&str; 33] = ["NULL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "TAB", "LF", "VT", "FF", "CR", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US", "Space"];
    if value <= 32 { CONTROL[usize::from(value)] } else if value == 127 { "DEL" } else { "" }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConverterOperation {
    AsciiToHex,
    HexToAscii,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConverterError {
    InvalidHex,
}

impl fmt::Display for ConverterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidHex => "Hex format is not conformed",
        })
    }
}
impl std::error::Error for ConverterError {}

pub fn ascii_to_hex(input: &[u8], eol: LineEnding, configuration: Configuration) -> String {
    let digits = if configuration.uppercase { b"0123456789ABCDEF" } else { b"0123456789abcdef" };
    let mut output = String::new();
    for (index, &byte) in input.iter().enumerate() {
        output.push(char::from(digits[usize::from(byte >> 4)]));
        output.push(char::from(digits[usize::from(byte & 15)]));
        let end_of_line = configuration.characters_per_line != 0 && (index + 1) % configuration.characters_per_line == 0;
        if configuration.insert_space && !end_of_line { output.push(' '); }
        if end_of_line {
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

pub fn transform_bytes(
    bytes: &[u8],
    operation: ConverterOperation,
    eol: LineEnding,
    configuration: Configuration,
) -> Result<Vec<u8>, ConverterError> {
    match operation {
        ConverterOperation::AsciiToHex => Ok(ascii_to_hex(bytes, eol, configuration).into_bytes()),
        ConverterOperation::HexToAscii => hex_to_ascii(bytes),
    }
}
