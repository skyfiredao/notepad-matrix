use crate::files::{UnicodeFormat, Utf8Format};

#[derive(Clone, Copy)]
pub struct CodePage {
    pub id: u32,
    pub label: &'static str,
    pub codepage: u32,
}

pub struct CodePageGroup {
    pub label: &'static str,
    pub entries: &'static [CodePage],
}

pub struct EncodingCommand {
    pub id: u32,
    pub label: &'static str,
}

pub const SOURCE_ENCODINGS: &[EncodingCommand] = &[
    EncodingCommand { id: 45004, label: "ANSI" },
    EncodingCommand { id: 45008, label: "UTF-8" },
    EncodingCommand { id: 45005, label: "UTF-8-BOM" },
    EncodingCommand { id: 45006, label: "UTF-16 BE BOM" },
    EncodingCommand { id: 45007, label: "UTF-16 LE BOM" },
];

pub const CONVERT_ENCODINGS: &[EncodingCommand] = &[
    EncodingCommand { id: 45009, label: "Convert to ANSI" },
    EncodingCommand { id: 45010, label: "Convert to UTF-8" },
    EncodingCommand { id: 45011, label: "Convert to UTF-8-BOM" },
    EncodingCommand { id: 45012, label: "Convert to UTF-16 BE BOM" },
    EncodingCommand { id: 45013, label: "Convert to UTF-16 LE BOM" },
];

pub const GROUPS: &[CodePageGroup] = &[
    CodePageGroup { label: "Arabic", entries: &[
        CodePage { id: 45034, label: "ISO 8859-6", codepage: 28596 },
        CodePage { id: 45045, label: "OEM 720", codepage: 720 },
        CodePage { id: 45026, label: "Windows-1256", codepage: 1256 },
    ] },
    CodePageGroup { label: "Baltic", entries: &[
        CodePage { id: 45032, label: "ISO 8859-4", codepage: 28594 },
        CodePage { id: 45040, label: "ISO 8859-13", codepage: 28603 },
        CodePage { id: 45047, label: "OEM 775", codepage: 775 },
        CodePage { id: 45027, label: "Windows-1257", codepage: 1257 },
    ] },
    CodePageGroup { label: "Celtic", entries: &[
        CodePage { id: 45041, label: "ISO 8859-14", codepage: 28604 },
    ] },
    CodePageGroup { label: "Cyrillic", entries: &[
        CodePage { id: 45033, label: "ISO 8859-5", codepage: 28595 },
        CodePage { id: 45068, label: "KOI8-R", codepage: 20866 },
        CodePage { id: 45067, label: "KOI8-U", codepage: 21866 },
        CodePage { id: 45066, label: "Macintosh", codepage: 10007 },
        CodePage { id: 45050, label: "OEM 855", codepage: 855 },
        CodePage { id: 45058, label: "OEM 866", codepage: 866 },
        CodePage { id: 45021, label: "Windows-1251", codepage: 1251 },
    ] },
    CodePageGroup { label: "Central European", entries: &[
        CodePage { id: 45049, label: "OEM 852", codepage: 852 },
        CodePage { id: 45020, label: "Windows-1250", codepage: 1250 },
    ] },
    CodePageGroup { label: "Chinese", entries: &[
        CodePage { id: 45060, label: "Big5 (Traditional)", codepage: 950 },
        CodePage { id: 45061, label: "GB2312 (Simplified)", codepage: 936 },
    ] },
    CodePageGroup { label: "Eastern European", entries: &[
        CodePage { id: 45030, label: "ISO 8859-2", codepage: 28592 },
    ] },
    CodePageGroup { label: "Greek", entries: &[
        CodePage { id: 45035, label: "ISO 8859-7", codepage: 28597 },
        CodePage { id: 45046, label: "OEM 737", codepage: 737 },
        CodePage { id: 45059, label: "OEM 869", codepage: 869 },
        CodePage { id: 45023, label: "Windows-1253", codepage: 1253 },
    ] },
    CodePageGroup { label: "Hebrew", entries: &[
        CodePage { id: 45036, label: "ISO 8859-8", codepage: 28598 },
        CodePage { id: 45055, label: "OEM 862", codepage: 862 },
        CodePage { id: 45025, label: "Windows-1255", codepage: 1255 },
    ] },
    CodePageGroup { label: "Japanese", entries: &[
        CodePage { id: 45062, label: "Shift-JIS", codepage: 932 },
    ] },
    CodePageGroup { label: "Korean", entries: &[
        CodePage { id: 45063, label: "Windows 949", codepage: 949 },
        CodePage { id: 45064, label: "EUC-KR", codepage: 51949 },
    ] },
    CodePageGroup { label: "North European", entries: &[
        CodePage { id: 45054, label: "OEM 861 : Icelandic", codepage: 861 },
        CodePage { id: 45057, label: "OEM 865 : Nordic", codepage: 865 },
    ] },
    CodePageGroup { label: "Thai", entries: &[
        CodePage { id: 45065, label: "TIS-620", codepage: 874 },
    ] },
    CodePageGroup { label: "Turkish", entries: &[
        CodePage { id: 45031, label: "ISO 8859-3", codepage: 28593 },
        CodePage { id: 45037, label: "ISO 8859-9", codepage: 28599 },
        CodePage { id: 45051, label: "OEM 857", codepage: 857 },
        CodePage { id: 45024, label: "Windows-1254", codepage: 1254 },
    ] },
    CodePageGroup { label: "Western European", entries: &[
        CodePage { id: 45029, label: "ISO 8859-1", codepage: 28591 },
        CodePage { id: 45042, label: "ISO 8859-15", codepage: 28605 },
        CodePage { id: 45048, label: "OEM 850", codepage: 850 },
        CodePage { id: 45052, label: "OEM 858", codepage: 858 },
        CodePage { id: 45053, label: "OEM 860 : Portuguese", codepage: 860 },
        CodePage { id: 45056, label: "OEM 863 : French", codepage: 863 },
        CodePage { id: 45044, label: "OEM-US : CP437", codepage: 437 },
        CodePage { id: 45022, label: "Windows-1252", codepage: 1252 },
    ] },
    CodePageGroup { label: "Vietnamese", entries: &[
        CodePage { id: 45028, label: "Windows-1258", codepage: 1258 },
    ] },
];

pub fn codepage(id: u32) -> Option<&'static CodePage> {
    GROUPS.iter().flat_map(|group| group.entries).find(|entry| entry.id == id)
}

pub fn unicode_format(id: u32) -> Option<UnicodeFormat> {
    match id {
        45008 | 45010 => Some(UnicodeFormat::Utf8(Utf8Format::Plain)),
        45005 | 45011 => Some(UnicodeFormat::Utf8(Utf8Format::Bom)),
        45006 | 45012 => Some(UnicodeFormat::Utf16BeBom),
        45007 | 45013 => Some(UnicodeFormat::Utf16LeBom),
        _ => None,
    }
}

pub fn is_encoding_command(id: u32) -> bool {
    matches!(id, 45004..=45013) || codepage(id).is_some()
}
