use crate::editor::{EditError, Selection};
use ropey::{LineType, Rope};

pub struct DocumentStatus {
    pub bytes: usize,
    pub lines: usize,
    pub line: usize,
    pub column: usize,
    pub position: usize,
    pub selected_chars: usize,
    pub selected_lines: usize,
}

pub struct DocumentSummary {
    pub bytes: usize,
    pub characters: usize,
    pub words: usize,
    pub lines: usize,
    pub selected_characters: usize,
    pub selected_bytes: usize,
    pub selection_ranges: usize,
}

impl DocumentSummary {
    pub fn description(&self) -> String {
        fn count(value: usize) -> String {
            let digits = value.to_string();
            let mut result = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    result.push(',');
                }
                result.push(digit);
            }
            result
        }
        format!(
            "Characters (without line endings): {}\nWords: {}\nLines: {}\nDocument length: {}\n{} selected characters ({} bytes) in {} ranges",
            count(self.characters),
            count(self.words),
            count(self.lines),
            count(self.bytes),
            count(self.selected_characters),
            count(self.selected_bytes),
            count(self.selection_ranges)
        )
    }
}

pub fn summarize(
    text: &Rope,
    selection: Selection,
    unicode_mode: crate::files::UnicodeFormat,
    line_ending: crate::input::LineEnding,
) -> Result<DocumentSummary, EditError> {
    let status = measure(text, selection, 4)?;
    let range = selection.range();
    let mut characters = 0;
    let mut words = 0;
    let mut in_word = false;
    let delimiters = " \t\\.,;:!?()+\r\n-*/=][{}&~\"'`|@$%<>^";
    for ch in text.chars() {
        if ch != '\r' && ch != '\n' {
            characters += 1;
        }
        let word = !delimiters.contains(ch);
        if word && !in_word {
            words += 1;
        }
        in_word = word;
    }
    let mut selected_characters = text
        .slice(range.start.0..range.end.0)
        .chars()
        .filter(|ch| *ch != '\r' && *ch != '\n')
        .count();
    if matches!(
        unicode_mode,
        crate::files::UnicodeFormat::Utf16LeBom | crate::files::UnicodeFormat::Utf16BeBom
    ) {
        let eol_bytes = line_ending.as_str().len();
        characters = status.bytes.saturating_sub((status.lines - 1) * eol_bytes);
        let selected_line_breaks = text.byte_to_line_idx(range.end.0, LineType::LF_CR)
            - text.byte_to_line_idx(range.start.0, LineType::LF_CR);
        selected_characters = (range.end.0 - range.start.0)
            .wrapping_sub(selected_line_breaks * eol_bytes)
            .wrapping_mul(2);
    }
    Ok(DocumentSummary {
        bytes: status.bytes,
        characters,
        words,
        lines: status.lines,
        selected_characters,
        selected_bytes: range.end.0 - range.start.0,
        selection_ranges: usize::from(!range.is_empty()),
    })
}

pub fn measure(
    text: &Rope,
    selection: Selection,
    tab_width: usize,
) -> Result<DocumentStatus, EditError> {
    let range = selection.range();
    if range.end.0 > text.len() {
        return Err(EditError::InvalidRange);
    }
    if !text.is_char_boundary(range.start.0) || !text.is_char_boundary(range.end.0) {
        return Err(EditError::InvalidBoundary);
    }
    let row = text.byte_to_line_idx(selection.head.0, LineType::LF_CR);
    let start = text.line_to_byte_idx(row, LineType::LF_CR);
    let mut column = 0;
    let width = tab_width.max(1);
    for ch in text.slice(start..selection.head.0).chars() {
        if ch == '\t' {
            column += width - column % width;
        } else if ch == '\r' || ch == '\n' {
            break;
        } else {
            column += 1;
        }
    }
    let selected_lines = if range.is_empty() {
        0
    } else {
        text.byte_to_line_idx(range.end.0 - 1, LineType::LF_CR)
            - text.byte_to_line_idx(range.start.0, LineType::LF_CR)
            + 1
    };
    Ok(DocumentStatus {
        bytes: text.len(),
        lines: text.len_lines(LineType::LF_CR),
        line: row + 1,
        column: column + 1,
        position: selection.head.0 + 1,
        selected_chars: text.slice(range.start.0..range.end.0).len_chars(),
        selected_lines,
    })
}
