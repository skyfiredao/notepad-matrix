use crate::editor::{
    ByteOffset, EditError, EditSelectionPolicy, Selection, SelectionSet, TextTransform,
};
use crate::input::EditorSession;
use crate::syntax::Language;
use ropey::LineType;

pub const OPTIONS: [(&str, &str); 6] = [
    ("parentheses", "Parentheses ( )"),
    ("brackets", "Brackets [ ]"),
    ("curlyBrackets", "Curly brackets { }"),
    ("quotes", "Single quotes"),
    ("doubleQuotes", "Double quotes"),
    ("htmlXmlTag", "HTML/XML close tags"),
];

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub flags: [bool; 6],
    pub user_pairs: Vec<(u8, u8)>,
}

impl Settings {
    pub fn from_json(settings: &serde_json::Value) -> Self {
        let value = &settings["auto-insert"];
        Self {
            flags: OPTIONS.map(|(key, _)| {
                value[key]
                    .as_bool()
                    .unwrap_or_else(|| value[key].as_str() == Some("yes"))
            }),
            user_pairs: value["UserDefinePair"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|pair| {
                    let open = pair["open"].as_u64().filter(|value| *value <= 127)? as u8;
                    let close = pair["close"].as_u64().filter(|value| *value <= 127)? as u8;
                    Some((open, close))
                })
                .collect(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.flags.iter().any(|flag| *flag) || !self.user_pairs.is_empty()
    }

    pub fn may_edit(&self, character: char) -> bool {
        let flag = match character {
            '(' | ')' => Some(0),
            '[' | ']' => Some(1),
            '{' | '}' => Some(2),
            '\'' => Some(3),
            '"' => Some(4),
            '>' => Some(5),
            _ => None,
        };
        flag.is_some_and(|index| self.flags[index])
            || self
                .user_pairs
                .iter()
                .any(|(open, _)| char::from(*open) == character)
    }
}

#[derive(Default)]
pub struct Tracker {
    inserted: Vec<(u8, usize)>,
}

impl Tracker {
    fn prune(&mut self, text: &str, character: char, position: usize) {
        if matches!(character, '\r' | '\n') {
            self.inserted.clear();
            return;
        }
        let snapshot = ropey::Rope::from_str(text);
        let line = snapshot.byte_to_line_idx(position, LineType::LF_CR);
        self.inserted.retain(|(_, at)| {
            *at < position
                && snapshot.is_char_boundary(*at)
                && snapshot.byte_to_line_idx(*at, LineType::LF_CR) == line
        });
    }

    fn find_close(&mut self, text: &str, start: u8, end: u8, caret: usize) -> Option<usize> {
        let snapshot = ropey::Rope::from_str(text);
        let line = snapshot.byte_to_line_idx(caret, LineType::LF_CR);
        for index in (0..self.inserted.len()).rev() {
            let (character, at) = self.inserted[index];
            if character != start {
                continue;
            }
            self.inserted.remove(index);
            if at >= caret
                || !snapshot.is_char_boundary(at)
                || snapshot.byte_to_line_idx(at, LineType::LF_CR) != line
            {
                continue;
            }
            let next = text.as_bytes()[caret..]
                .iter()
                .position(|byte| *byte != b' ')?
                + caret;
            return (text.as_bytes()[next] == end).then_some(next);
        }
        None
    }

    pub fn after_character(
        &mut self,
        editor: &mut EditorSession,
        character: char,
        settings: &Settings,
        language: Language,
    ) -> Result<(), EditError> {
        let text = editor.committed_text();
        let caret = editor.selection().head.0;
        if caret == 0 {
            return Ok(());
        }
        if !character.is_ascii() {
            self.prune(&text, '\0', caret - 1);
            return Ok(());
        }
        let ch = character as u8;
        let previous = text
            .as_bytes()
            .get(caret.wrapping_sub(2))
            .copied()
            .unwrap_or(0);
        let next = text.as_bytes().get(caret).copied().unwrap_or(0);
        let blank = |ch| matches!(ch, b' ' | b'\t' | b'\n' | b'\r' | 0);
        let next_blank = matches!(next, b' ' | b'\t' | b'\n' | b'\r') || caret == text.len();
        if let Some((_, close)) = settings
            .user_pairs
            .iter()
            .find(|(open, _)| *open == ch && next_blank)
        {
            if *close != 0 {
                change(editor, caret..caret, char::from(*close).to_string(), caret)?;
            }
            return Ok(());
        }
        let opening = match ch {
            b'(' => Some((0, b')')),
            b'[' => Some((1, b']')),
            b'{' => Some((2, b'}')),
            b'\'' => Some((3, b'\'')),
            b'"' => Some((4, b'"')),
            _ => None,
        };
        if let Some((flag, close)) = opening {
            if !settings.flags[flag] {
                return Ok(());
            }
            if flag >= 3
                && let Some(at) = self.find_close(&text, ch, ch, caret)
            {
                return change(editor, at..at + 1, String::new(), at);
            }
            let paired = matches!((previous, next), (b'(', b')') | (b'[', b']') | (b'{', b'}'));
            let can_insert = if flag < 3 {
                next_blank || matches!(next, b')' | b']' | b'}')
            } else {
                (blank(previous) && next_blank)
                    || paired
                    || (matches!(previous, b'(' | b'[' | b'{') && next_blank)
                    || (blank(previous) && matches!(next, b')' | b']' | b'}'))
            };
            if can_insert {
                self.prune(&text, character, caret - 1);
                self.inserted.push((ch, caret - 1));
                change(editor, caret..caret, char::from(close).to_string(), caret)?;
            }
        } else if let Some((flag, open)) = match ch {
            b')' => Some((0, b'(')),
            b']' => Some((1, b'[')),
            b'}' => Some((2, b'{')),
            _ => None,
        } {
            if settings.flags[flag]
                && let Some(at) = self.find_close(&text, open, ch, caret)
            {
                change(editor, at..at + 1, String::new(), at)?;
            }
        } else if ch == b'>' {
            if settings.flags[5]
                && let Some(close) = close_tag(&text, caret, language)?
            {
                change(editor, caret..caret, close, caret)?;
            }
        } else {
            self.prune(&text, character, caret - 1);
        }
        Ok(())
    }
}

fn change(
    editor: &mut EditorSession,
    range: std::ops::Range<usize>,
    text: String,
    caret: usize,
) -> Result<(), EditError> {
    let position = ByteOffset(caret);
    let selections = SelectionSet::new(
        vec![
            Selection {
                anchor: position,
                head: position,
            }
            .into(),
        ],
        0,
    )?;
    editor.apply_multi(
        &[TextTransform {
            range: ByteOffset(range.start)..ByteOffset(range.end),
            text,
        }],
        EditSelectionPolicy::Explicit(selections),
    )
}

fn close_tag(text: &str, caret: usize, language: Language) -> Result<Option<String>, EditError> {
    let Language::Builtin(id) = language else {
        return Ok(None);
    };
    let catalog =
        crate::languages::catalog().map_err(|error| EditError::Storage(error.to_string()))?;
    let Some(profile) = catalog
        .by_id(id)
        .filter(|profile| matches!(profile.name.as_str(), "html" | "xml"))
    else {
        return Ok(None);
    };
    let html = profile.name == "html";
    if html {
        let lexed = crate::lexers::lex(text, profile)
            .map_err(|error| EditError::Storage(error.to_string()))?;
        let span = lexed
            .spans
            .iter()
            .find(|span| span.range.contains(&caret))
            .or_else(|| lexed.spans.last());
        if span.is_some_and(|span| span.style >= 40) {
            return Ok(None);
        }
    }
    let before = &text[..caret.saturating_sub(1)];
    if before.ends_with('/') || before.ends_with("--") {
        return Ok(None);
    }
    let Some(start) = before.rfind('<') else {
        return Ok(None);
    };
    let head = before[start..]
        .split(|ch: char| ch.is_ascii_whitespace() || ch == '>')
        .next()
        .unwrap_or("");
    if head.len() < 2
        || head.len() > 254
        || head.starts_with("</")
        || head.starts_with("<?")
        || head.starts_with("<!--")
    {
        return Ok(None);
    }
    let name = &head[1..];
    if html
        && [
            "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta",
            "param", "source", "track", "wbr", "!doctype",
        ]
        .iter()
        .any(|tag| name.to_ascii_lowercase().starts_with(tag))
    {
        return Ok(None);
    }
    Ok(Some(format!("</{name}>")))
}
