use crate::editor::{ByteOffset, EditError, EditSelectionPolicy, SelectionSet, TextTransform};
use crate::input::{EditorSession, LineEnding};
use crate::syntax::Language;
use ropey::LineType;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    None,
    #[default]
    Advanced,
    Basic,
}

impl Mode {
    pub fn from_json(settings: &serde_json::Value) -> Self {
        match settings["maintain_indent"].as_u64() {
            Some(0) => Self::None,
            Some(2) => Self::Basic,
            _ => match settings["maintain_indent"].as_str() {
                Some("0" | "no") => Self::None,
                Some("2") => Self::Basic,
                _ => Self::Advanced,
            },
        }
    }

    pub fn value(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Advanced => 1,
            Self::Basic => 2,
        }
    }
}

fn content(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn columns(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .fold(0, |column, byte| {
            if byte == b'\t' {
                column + 4 - column % 4
            } else {
                column + 1
            }
        })
}

fn condition(line: &str) -> bool {
    let line = content(line);
    for (at, _) in line.char_indices() {
        let rest = &line[at..];
        let mut candidate = rest;
        if let Some(after) = rest.strip_prefix("else") {
            let trimmed = after.trim_start_matches([' ', '\t']);
            if trimmed.len() < after.len() && trimmed.starts_with("if") {
                candidate = trimmed;
            }
        }
        for word in ["if", "for", "while"] {
            if let Some(after) = candidate.strip_prefix(word)
                && let Some(body) = after.trim_start_matches([' ', '\t']).strip_prefix('(')
                && let Some(close) = body.rfind(')')
            {
                return body[close + 1..]
                    .bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t'));
            }
        }
        if let Some(after) = rest.strip_prefix("else") {
            return after.bytes().all(|byte| matches!(byte, b' ' | b'\t'));
        }
    }
    false
}

fn set_indent(editor: &mut EditorSession, line: usize, amount: usize) -> Result<(), EditError> {
    let snapshot = editor.display_snapshot();
    let start = snapshot.line_to_byte_idx(line, LineType::LF_CR);
    let value = snapshot.line(line, LineType::LF_CR).to_string();
    let prefix = value
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    let text = format!("{}{}", "\t".repeat(amount / 4), " ".repeat(amount % 4));
    if value[..prefix] == text {
        return Ok(());
    }
    let original = editor.selections();
    let mut ranges = original.ranges().to_vec();
    for range in &mut ranges {
        for position in [&mut range.selection.anchor, &mut range.selection.head] {
            if text.len() >= prefix && position.0 >= start + prefix {
                position.0 += text.len() - prefix;
            } else if text.len() < prefix && position.0 >= start + text.len() {
                position.0 = if position.0 >= start + prefix {
                    position.0 - (prefix - text.len())
                } else {
                    start + text.len()
                };
            }
        }
    }
    let selections = if original.is_rectangular() {
        SelectionSet::rectangle(ranges, original.primary_index())
    } else {
        SelectionSet::new(ranges, original.primary_index())
    }?;
    editor.apply_multi(
        &[TextTransform {
            range: ByteOffset(start)..ByteOffset(start + prefix),
            text,
        }],
        EditSelectionPolicy::Explicit(selections),
    )
}

fn insert_below_caret(editor: &mut EditorSession) -> Result<(), EditError> {
    let caret = editor.selection().head;
    let text = editor.line_ending().as_str().to_owned();
    let original = editor.selections();
    let mut ranges = original.ranges().to_vec();
    for (index, range) in ranges.iter_mut().enumerate() {
        if index != original.primary_index() {
            for position in [&mut range.selection.anchor, &mut range.selection.head] {
                if *position >= caret {
                    position.0 += text.len();
                }
            }
        }
    }
    let selections = if original.is_rectangular() {
        SelectionSet::rectangle(ranges, original.primary_index())
    } else {
        SelectionSet::new(ranges, original.primary_index())
    }?;
    editor.apply_multi(
        &[TextTransform {
            range: caret..caret,
            text,
        }],
        EditSelectionPolicy::Explicit(selections),
    )
}

pub fn after_character(
    editor: &mut EditorSession,
    character: char,
    mode: Mode,
    language: Language,
) -> Result<(), EditError> {
    if mode == Mode::None {
        return Ok(());
    }
    let newline = character
        == if editor.line_ending() == LineEnding::Cr {
            '\r'
        } else {
            '\n'
        };
    if !newline && !matches!(character, '{' | '}') {
        return Ok(());
    }
    let snapshot = editor.display_snapshot();
    let caret = editor.selection().head.0;
    let current = snapshot.byte_to_line_idx(caret, LineType::LF_CR);
    if newline
        && (current == 0
            || content(&snapshot.line(current - 1, LineType::LF_CR).to_string()).is_empty())
    {
        return Ok(());
    }
    let previous = current.checked_sub(1);
    let previous_text = previous
        .map(|line| snapshot.line(line, LineType::LF_CR).to_string())
        .unwrap_or_default();
    let amount = columns(&previous_text);
    let profile = if let Language::Builtin(id) = language {
        Some(
            crate::languages::catalog()
                .map_err(|error| EditError::Storage(error.to_string()))?
                .by_id(id),
        )
    } else {
        None
    }
    .flatten();
    let key = if language == Language::Json {
        "json"
    } else {
        profile.map_or("normal", |profile| profile.name.as_str())
    };
    let c_like = matches!(
        key,
        "c" | "cpp"
            | "java"
            | "cs"
            | "objc"
            | "php"
            | "javascript"
            | "javascript.js"
            | "jsp"
            | "css"
            | "perl"
            | "rust"
            | "powershell"
            | "json"
            | "json5"
            | "typescript"
            | "go"
            | "swift"
    );
    if mode == Mode::Basic || (!c_like && key != "python") {
        if newline && amount > 0 {
            set_indent(editor, current, amount)?;
        }
        return Ok(());
    }
    if key == "python" {
        if !newline {
            return Ok(());
        }
        let candidate = content(&previous_text).match_indices(':').find(|(at, _)| {
            let after = content(&previous_text)[at + 1..].trim_start_matches([' ', '\t']);
            after.is_empty() || after.starts_with('#')
        });
        let mut target = amount;
        if let (Some((at, _)), Some(profile), Some(line)) = (candidate, profile, previous) {
            let text = snapshot.to_string();
            let offset = snapshot.line_to_byte_idx(line, LineType::LF_CR) + at;
            let lexed = crate::lexers::lex(&text, profile)
                .map_err(|error| EditError::Storage(error.to_string()))?;
            if lexed
                .spans
                .iter()
                .any(|span| span.range.contains(&offset) && span.style == 10)
            {
                target += 4;
            }
        }
        if target > 0 {
            set_indent(editor, current, target)?;
        }
    } else if newline {
        if content(&previous_text).ends_with('{') {
            if caret < snapshot.len() && snapshot.byte(caret) == b'}' {
                insert_below_caret(editor)?;
                set_indent(editor, current + 1, amount)?;
            }
            set_indent(editor, current, amount + 4)?;
        } else if (caret < snapshot.len() && snapshot.byte(caret) == b'{')
            || matches!(key, "perl" | "rust" | "powershell" | "json" | "json5")
        {
            set_indent(editor, current, amount)?;
        } else if condition(&previous_text) {
            set_indent(editor, current, amount + 4)?;
        } else if amount > 0 {
            let previous_condition = previous.filter(|line| *line > 0).is_some_and(|line| {
                condition(&snapshot.line(line - 1, LineType::LF_CR).to_string())
            });
            set_indent(
                editor,
                current,
                if previous_condition {
                    amount.saturating_sub(4)
                } else {
                    amount
                },
            )?;
        }
    } else if character == '{' {
        let start = snapshot
            .line_to_byte_idx(current, LineType::LF_CR)
            .max(1)
            .min(caret.saturating_sub(1));
        if (start..caret.saturating_sub(1))
            .all(|position| matches!(snapshot.byte(position), b' ' | b'\t'))
        {
            set_indent(
                editor,
                current,
                amount + if previous_text.contains('{') { 4 } else { 0 },
            )?;
        }
    } else {
        let text = snapshot.to_string();
        let mut balance = 0;
        for (at, byte) in text.as_bytes()[..caret.saturating_sub(1)]
            .iter()
            .enumerate()
            .rev()
        {
            if *byte == b'{' {
                if balance == 0 {
                    let line = snapshot.byte_to_line_idx(at, LineType::LF_CR);
                    if line != current {
                        set_indent(
                            editor,
                            current,
                            columns(&snapshot.line(line, LineType::LF_CR).to_string()),
                        )?;
                    }
                    break;
                }
                balance -= 1;
            } else if *byte == b'}' {
                balance += 1;
            }
        }
    }
    Ok(())
}
