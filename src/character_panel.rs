use crate::editor::{EditError, Selection};
use crate::input::EditorSession;
use encoding_rs::{self as enc, Encoding};

#[path = "character_codes.rs"]
mod source;

pub const CODE_PAGES: &[&Encoding] = &[
    enc::UTF_8,
    enc::WINDOWS_1252,
    enc::WINDOWS_1250,
    enc::WINDOWS_1251,
    enc::WINDOWS_1253,
    enc::WINDOWS_1254,
    enc::WINDOWS_1255,
    enc::WINDOWS_1256,
    enc::WINDOWS_1257,
    enc::WINDOWS_1258,
    enc::WINDOWS_874,
    enc::GBK,
    enc::GB18030,
    enc::SHIFT_JIS,
    enc::BIG5,
    enc::EUC_JP,
    enc::EUC_KR,
    enc::ISO_2022_JP,
    enc::IBM866,
    enc::KOI8_R,
    enc::KOI8_U,
    enc::MACINTOSH,
    enc::X_MAC_CYRILLIC,
    enc::ISO_8859_2,
    enc::ISO_8859_3,
    enc::ISO_8859_4,
    enc::ISO_8859_5,
    enc::ISO_8859_6,
    enc::ISO_8859_7,
    enc::ISO_8859_8,
    enc::ISO_8859_10,
    enc::ISO_8859_13,
    enc::ISO_8859_14,
    enc::ISO_8859_15,
    enc::ISO_8859_16,
];

#[derive(Default)]
pub struct Table {
    code_page: usize,
}

pub struct Row {
    pub cells: [String; 6],
    pub character: String,
}

impl Table {
    pub fn encoding(&self) -> &'static str {
        CODE_PAGES[self.code_page].name()
    }

    pub fn select_encoding(&mut self, name: &str) -> std::io::Result<()> {
        self.code_page = CODE_PAGES
            .iter()
            .position(|encoding| encoding.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Unsupported character-panel encoding",
                )
            })?;
        Ok(())
    }

    pub fn row(&self, value: u8) -> Row {
        let encoding = CODE_PAGES[self.code_page];
        let bytes = [value];
        let character = if value < 128 {
            char::from(value).to_string()
        } else {
            encoding.decode_without_bom_handling(&bytes).0.into_owned()
        };
        let label = source::CONTROL_NAMES[usize::from(value)];
        let glyph = if label.is_empty() {
            character.clone()
        } else {
            label.to_owned()
        };
        let mut cells = [
            value.to_string(),
            format!("{value:02X}"),
            glyph,
            String::new(),
            String::new(),
            String::new(),
        ];
        if std::ptr::eq(encoding, enc::WINDOWS_1252) {
            cells[3] = source::HTML_NAMES[usize::from(value)].to_owned();
            let number = if ((32..=126).contains(&value) && value != 45) || value >= 160 {
                i32::from(value)
            } else {
                source::HTML_NUMBERS[usize::from(value)]
            };
            if number >= 0 {
                cells[4] = format!("&#{number};");
                cells[5] = format!("&#x{number:x};");
            }
        }
        Row { cells, character }
    }

    pub fn insertion(&self, value: u8, column: usize) -> Option<String> {
        let row = self.row(value);
        if column == 2 {
            Some(row.character)
        } else {
            row.cells.get(column).cloned()
        }
    }
}

pub fn insert(editor: &mut EditorSession, text: &str) -> Result<(), EditError> {
    editor.insert_text("")?;
    let caret = editor.selection().head;
    editor.select(Selection {
        anchor: caret,
        head: caret,
    })?;
    editor.insert_text(text)
}
