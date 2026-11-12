use crate::udl::{Definition, nesting};
use serde_json::Value;
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    FolderDefault,
    Keywords,
    CommentsNumbers,
    OperatorsDelimiters,
}
impl Page {
    pub const ALL: [Self; 4] = [
        Self::FolderDefault,
        Self::Keywords,
        Self::CommentsNumbers,
        Self::OperatorsDelimiters,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::FolderDefault => "Folder & Default",
            Self::Keywords => "Keywords Lists",
            Self::CommentsNumbers => "Comment & Number",
            Self::OperatorsDelimiters => "Operators & Delimiters",
        }
    }
}
#[derive(Clone, Debug)]
pub enum Kind {
    Text,
    Boolean,
    Choice(&'static [&'static str]),
    Color,
    Integer { min: i64, max: i64 },
    Bit(u32),
}
#[derive(Clone, Debug)]
pub struct Control {
    pub group: String,
    pub label: &'static str,
    pub path: String,
    pub kind: Kind,
}
impl Control {
    pub fn text(&self, definition: &Definition) -> String {
        let value = definition.to_json();
        let value = value.pointer(&self.path).unwrap_or(&Value::Null);
        match self.kind {
            Kind::Bit(bit) => {
                (value.as_i64().unwrap_or(0).max(0) as u64 & u64::from(bit) != 0).to_string()
            }
            Kind::Color => value
                .as_u64()
                .map(|n| format!("#{n:06X}"))
                .unwrap_or_default(),
            _ => value.as_str().map(str::to_owned).unwrap_or_else(|| {
                if value.is_null() {
                    String::new()
                } else {
                    value.to_string()
                }
            }),
        }
    }
    pub fn apply(&self, definition: &mut Definition, text: &str) -> io::Result<()> {
        let bad = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Invalid {}", self.label),
            )
        };
        let mut candidate = definition.to_json();
        let target = candidate.pointer_mut(&self.path).ok_or_else(bad)?;
        let value = match self.kind {
            Kind::Text => Value::String(text.into()),
            Kind::Boolean => Value::Bool(text.parse::<bool>().map_err(|_| bad())?),
            Kind::Choice(options) => {
                let n = text.parse::<usize>().map_err(|_| bad())?;
                if n >= options.len() {
                    return Err(bad());
                }
                Value::from(n)
            }
            Kind::Color => {
                if text.trim().is_empty() {
                    Value::Null
                } else {
                    let raw = text.trim().trim_start_matches('#');
                    if raw.len() != 6 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(bad());
                    }
                    Value::from(u32::from_str_radix(raw, 16).map_err(|_| bad())?)
                }
            }
            Kind::Integer { min, max } => {
                let n = if text.trim().is_empty() && min == -1 {
                    -1
                } else {
                    text.parse::<i64>().map_err(|_| bad())?
                };
                if n < min || n > max {
                    return Err(bad());
                }
                Value::from(n)
            }
            Kind::Bit(bit) => {
                let enabled = text.parse::<bool>().map_err(|_| bad())?;
                let old = target.as_i64().unwrap_or(0).max(0) as u64;
                Value::from(if enabled {
                    old | u64::from(bit)
                } else {
                    old & !u64::from(bit)
                })
            }
        };
        *target = value;
        *definition = Definition::from_json(&candidate)?;
        Ok(())
    }
}
fn control(
    group: impl Into<String>,
    label: &'static str,
    path: impl Into<String>,
    kind: Kind,
) -> Control {
    Control {
        group: group.into(),
        label,
        path: path.into(),
        kind,
    }
}
pub fn controls(page: Page) -> Vec<Control> {
    let mut out = Vec::new();
    match page {
        Page::FolderDefault => {
            for (label, path) in [
                ("Ignore case", "case_ignored"),
                ("Fold compact (include blank lines)", "fold_compact"),
                ("Dark mode language", "dark_mode_theme"),
            ] {
                out.push(control("Default", label, format!("/{path}"), Kind::Boolean));
            }
            for (i, title) in [
                "Folding in code 1 (forward)",
                "Folding in code 2 (words)",
                "Folding in comments",
            ]
            .into_iter()
            .enumerate()
            {
                for (key, label) in [("open", "Open"), ("middle", "Middle"), ("close", "Close")] {
                    out.push(control(
                        title,
                        label,
                        format!("/folding/{i}/{key}"),
                        Kind::Text,
                    ));
                }
            }
        }
        Page::Keywords => {
            for i in 0..8 {
                out.push(control(
                    format!("Keyword group {}", i + 1),
                    "Keywords (space separated)",
                    format!("/keywords/{i}"),
                    Kind::Text,
                ));
                out.push(control(
                    format!("Keyword group {}", i + 1),
                    "Prefix mode",
                    format!("/prefix_keywords/{i}"),
                    Kind::Boolean,
                ));
            }
        }
        Page::CommentsNumbers => {
            out.push(control(
                "Line comments",
                "Line comment position",
                "/line_comment_placement",
                Kind::Choice(&["Anywhere", "Beginning of line", "After whitespace"]),
            ));
            out.push(control(
                "Line comments",
                "Allow folding of comments",
                "/allow_fold_comments",
                Kind::Boolean,
            ));
            for (key, label) in [
                ("line_open", "Line comment open"),
                ("line_continue", "Line continuation"),
                ("line_close", "Line comment close"),
                ("block_open", "Block comment open"),
                ("block_close", "Block comment close"),
            ] {
                out.push(control(
                    if key.starts_with("line_") {
                        "Line comments"
                    } else {
                        "Block comments"
                    },
                    label,
                    format!("/comments/{key}"),
                    Kind::Text,
                ));
            }
            for (key, label) in [
                ("prefix1", "Prefix 1"),
                ("prefix2", "Prefix 2"),
                ("extras1", "Extras 1"),
                ("extras2", "Extras 2"),
                ("suffix1", "Suffix 1"),
                ("suffix2", "Suffix 2"),
                ("range", "Range"),
            ] {
                out.push(control(
                    "Numbers",
                    label,
                    format!("/numbers/{key}"),
                    Kind::Text,
                ));
            }
            out.push(control(
                "Numbers",
                "Decimal separator",
                "/decimal_separator",
                Kind::Choice(&["Dot", "Comma", "Dot and comma"]),
            ));
        }
        Page::OperatorsDelimiters => {
            out.push(control(
                "Operators",
                "Operators 1 (forward)",
                "/operators/0",
                Kind::Text,
            ));
            out.push(control(
                "Operators",
                "Operators 2 (words)",
                "/operators/1",
                Kind::Text,
            ));
            for i in 0..8 {
                for (key, label) in [("open", "Open"), ("escape", "Escape"), ("close", "Close")] {
                    out.push(control(
                        format!("Delimiter {}", i + 1),
                        label,
                        format!("/delimiters/{i}/{key}"),
                        Kind::Text,
                    ));
                }
            }
        }
    }
    out
}
pub const STYLE_NAMES: [&str; 24] = [
    "Default",
    "Block comment",
    "Line comment",
    "Number",
    "Keyword 1",
    "Keyword 2",
    "Keyword 3",
    "Keyword 4",
    "Keyword 5",
    "Keyword 6",
    "Keyword 7",
    "Keyword 8",
    "Operator",
    "Folding in code 1",
    "Folding in code 2",
    "Folding in comments",
    "Delimiter 1",
    "Delimiter 2",
    "Delimiter 3",
    "Delimiter 4",
    "Delimiter 5",
    "Delimiter 6",
    "Delimiter 7",
    "Delimiter 8",
];
pub fn style_controls(style: usize) -> Vec<Control> {
    if style >= 24 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut add = |group, label, field, kind| {
        out.push(control(
            group,
            label,
            format!("/styles/{style}/{field}"),
            kind,
        ))
    };
    add(
        "Colors",
        "Foreground (#RRGGBB; blank inherits)",
        "foreground",
        Kind::Color,
    );
    add(
        "Colors",
        "Background (#RRGGBB; blank inherits)",
        "background",
        Kind::Color,
    );
    add("Colors", "Use foreground", "color_style", Kind::Bit(1));
    add("Colors", "Use background", "color_style", Kind::Bit(2));
    add("Font", "Enable font", "font_enabled", Kind::Boolean);
    add(
        "Font",
        "Font family (blank inherits)",
        "font_name",
        Kind::Text,
    );
    add(
        "Font",
        "Font size (blank inherits)",
        "font_size",
        Kind::Integer { min: -1, max: 200 },
    );
    for (label, bit) in [("Bold", 1), ("Italic", 2), ("Underline", 4)] {
        add("Font", label, "font_style", Kind::Bit(bit));
    }
    for (label, bit) in [
        ("Allow delimiter 1", nesting::delimiter(0)),
        ("Allow delimiter 2", nesting::delimiter(1)),
        ("Allow delimiter 3", nesting::delimiter(2)),
        ("Allow delimiter 4", nesting::delimiter(3)),
        ("Allow delimiter 5", nesting::delimiter(4)),
        ("Allow delimiter 6", nesting::delimiter(5)),
        ("Allow delimiter 7", nesting::delimiter(6)),
        ("Allow delimiter 8", nesting::delimiter(7)),
        ("Allow block comments", nesting::COMMENT),
        ("Allow line comments", nesting::COMMENT_LINE),
        ("Allow keyword group 1", nesting::keyword(0)),
        ("Allow keyword group 2", nesting::keyword(1)),
        ("Allow keyword group 3", nesting::keyword(2)),
        ("Allow keyword group 4", nesting::keyword(3)),
        ("Allow keyword group 5", nesting::keyword(4)),
        ("Allow keyword group 6", nesting::keyword(5)),
        ("Allow keyword group 7", nesting::keyword(6)),
        ("Allow keyword group 8", nesting::keyword(7)),
        ("Allow operators 1", nesting::OPERATORS1),
        ("Allow operators 2", nesting::OPERATORS2),
        ("Allow numbers", nesting::NUMBERS),
        ("Allow code 2 open", nesting::code2(0)),
        ("Allow code 2 middle", nesting::code2(1)),
        ("Allow code 2 close", nesting::code2(2)),
        ("Allow comment fold open", nesting::comment_fold(0)),
        ("Allow comment fold middle", nesting::comment_fold(1)),
        ("Allow comment fold close", nesting::comment_fold(2)),
    ] {
        add("Nesting", label, "nesting", Kind::Bit(bit));
    }
    out
}

pub fn extensions_control() -> Control {
    control(
        "Language",
        "Extensions (space separated, without dots)",
        "/extensions",
        Kind::Text,
    )
}
