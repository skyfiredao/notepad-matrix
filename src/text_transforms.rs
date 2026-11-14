use std::ops::Range;

pub mod session;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Case {
    Upper,
    Lower,
    ProperForce,
    ProperBlend,
    SentenceForce,
    SentenceBlend,
    Invert,
    Random,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trim {
    Leading,
    Trailing,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sort {
    Lexical,
    CaseInsensitive,
    Integer,
    DecimalComma,
    DecimalDot,
    Length,
    Reverse,
    Random,
    Locale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Comment {
    ToggleLine,
    Line,
    UncommentLine,
    Stream,
    UncommentStream,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransformCommand {
    Case(Case),
    Trim(Trim),
    EolToSpace,
    TrimAll,
    TabsToSpaces,
    SpacesToTabs { leading_only: bool },
    Indent,
    Unindent,
    DuplicateLine,
    DuplicateSelection,
    JoinLines,
    SplitLines,
    MoveUp,
    MoveDown,
    RemoveEmptyLines { include_whitespace: bool },
    RemoveDuplicates { consecutive_only: bool },
    Sort { kind: Sort, descending: bool },
    Comment(Comment),
    TransposeLine,
    BlankLineAbove,
    BlankLineBelow,
    Redact { bullet: bool },
}

impl TransformCommand {
    pub fn from_source_id(id: u32) -> Option<Self> {
        RETAINED_COMMANDS
            .iter()
            .find(|(_, source, _)| *source == id)
            .map(|(_, _, command)| *command)
    }
    pub fn source_id(self) -> Option<u32> {
        RETAINED_COMMANDS
            .iter()
            .find(|(_, _, command)| *command == self)
            .map(|(_, id, _)| *id)
    }
}

pub const RETAINED_COMMANDS: &[(&str, u32, TransformCommand)] = &[
    ("IDM_EDIT_INS_TAB", 42008, TransformCommand::Indent),
    ("IDM_EDIT_RMV_TAB", 42009, TransformCommand::Unindent),
    ("IDM_EDIT_DUP_LINE", 42010, TransformCommand::DuplicateLine),
    (
        "IDM_EDIT_TRANSPOSE_LINE",
        42011,
        TransformCommand::TransposeLine,
    ),
    ("IDM_EDIT_SPLIT_LINES", 42012, TransformCommand::SplitLines),
    ("IDM_EDIT_JOIN_LINES", 42013, TransformCommand::JoinLines),
    ("IDM_EDIT_LINE_UP", 42014, TransformCommand::MoveUp),
    ("IDM_EDIT_LINE_DOWN", 42015, TransformCommand::MoveDown),
    (
        "IDM_EDIT_UPPERCASE",
        42016,
        TransformCommand::Case(Case::Upper),
    ),
    (
        "IDM_EDIT_LOWERCASE",
        42017,
        TransformCommand::Case(Case::Lower),
    ),
    (
        "IDM_EDIT_BLOCK_COMMENT",
        42022,
        TransformCommand::Comment(Comment::ToggleLine),
    ),
    (
        "IDM_EDIT_STREAM_COMMENT",
        42023,
        TransformCommand::Comment(Comment::Stream),
    ),
    (
        "IDM_EDIT_TRIMTRAILING",
        42024,
        TransformCommand::Trim(Trim::Trailing),
    ),
    (
        "IDM_EDIT_BLOCK_COMMENT_SET",
        42035,
        TransformCommand::Comment(Comment::Line),
    ),
    (
        "IDM_EDIT_BLOCK_UNCOMMENT",
        42036,
        TransformCommand::Comment(Comment::UncommentLine),
    ),
    (
        "IDM_EDIT_TRIMLINEHEAD",
        42042,
        TransformCommand::Trim(Trim::Leading),
    ),
    (
        "IDM_EDIT_TRIM_BOTH",
        42043,
        TransformCommand::Trim(Trim::Both),
    ),
    ("IDM_EDIT_EOL2WS", 42044, TransformCommand::EolToSpace),
    ("IDM_EDIT_TRIMALL", 42045, TransformCommand::TrimAll),
    ("IDM_EDIT_TAB2SW", 42046, TransformCommand::TabsToSpaces),
    (
        "IDM_EDIT_STREAM_UNCOMMENT",
        42047,
        TransformCommand::Comment(Comment::UncommentStream),
    ),
    (
        "IDM_EDIT_SW2TAB_LEADING",
        42053,
        TransformCommand::SpacesToTabs { leading_only: true },
    ),
    (
        "IDM_EDIT_SW2TAB_ALL",
        42054,
        TransformCommand::SpacesToTabs {
            leading_only: false,
        },
    ),
    (
        "IDM_EDIT_REMOVEEMPTYLINES",
        42055,
        TransformCommand::RemoveEmptyLines {
            include_whitespace: false,
        },
    ),
    (
        "IDM_EDIT_REMOVEEMPTYLINESWITHBLANK",
        42056,
        TransformCommand::RemoveEmptyLines {
            include_whitespace: true,
        },
    ),
    (
        "IDM_EDIT_BLANKLINEABOVECURRENT",
        42057,
        TransformCommand::BlankLineAbove,
    ),
    (
        "IDM_EDIT_BLANKLINEBELOWCURRENT",
        42058,
        TransformCommand::BlankLineBelow,
    ),
    (
        "IDM_EDIT_SORTLINES_LEXICOGRAPHIC_ASCENDING",
        42059,
        TransformCommand::Sort {
            kind: Sort::Lexical,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LEXICOGRAPHIC_DESCENDING",
        42060,
        TransformCommand::Sort {
            kind: Sort::Lexical,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_INTEGER_ASCENDING",
        42061,
        TransformCommand::Sort {
            kind: Sort::Integer,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_INTEGER_DESCENDING",
        42062,
        TransformCommand::Sort {
            kind: Sort::Integer,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_DECIMALCOMMA_ASCENDING",
        42063,
        TransformCommand::Sort {
            kind: Sort::DecimalComma,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_DECIMALCOMMA_DESCENDING",
        42064,
        TransformCommand::Sort {
            kind: Sort::DecimalComma,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_DECIMALDOT_ASCENDING",
        42065,
        TransformCommand::Sort {
            kind: Sort::DecimalDot,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_DECIMALDOT_DESCENDING",
        42066,
        TransformCommand::Sort {
            kind: Sort::DecimalDot,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_PROPERCASE_FORCE",
        42067,
        TransformCommand::Case(Case::ProperForce),
    ),
    (
        "IDM_EDIT_PROPERCASE_BLEND",
        42068,
        TransformCommand::Case(Case::ProperBlend),
    ),
    (
        "IDM_EDIT_SENTENCECASE_FORCE",
        42069,
        TransformCommand::Case(Case::SentenceForce),
    ),
    (
        "IDM_EDIT_SENTENCECASE_BLEND",
        42070,
        TransformCommand::Case(Case::SentenceBlend),
    ),
    (
        "IDM_EDIT_INVERTCASE",
        42071,
        TransformCommand::Case(Case::Invert),
    ),
    (
        "IDM_EDIT_RANDOMCASE",
        42072,
        TransformCommand::Case(Case::Random),
    ),
    (
        "IDM_EDIT_REMOVE_CONSECUTIVE_DUP_LINES",
        42077,
        TransformCommand::RemoveDuplicates {
            consecutive_only: true,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_RANDOMLY",
        42078,
        TransformCommand::Sort {
            kind: Sort::Random,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_REMOVE_ANY_DUP_LINES",
        42079,
        TransformCommand::RemoveDuplicates {
            consecutive_only: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LEXICO_CASE_INSENS_ASCENDING",
        42080,
        TransformCommand::Sort {
            kind: Sort::CaseInsensitive,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LEXICO_CASE_INSENS_DESCENDING",
        42081,
        TransformCommand::Sort {
            kind: Sort::CaseInsensitive,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_REVERSE_ORDER",
        42083,
        TransformCommand::Sort {
            kind: Sort::Reverse,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LOCALE_ASCENDING",
        42100,
        TransformCommand::Sort {
            kind: Sort::Locale,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LOCALE_DESCENDING",
        42101,
        TransformCommand::Sort {
            kind: Sort::Locale,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LENGTH_ASCENDING",
        42104,
        TransformCommand::Sort {
            kind: Sort::Length,
            descending: false,
        },
    ),
    (
        "IDM_EDIT_SORTLINES_LENGTH_DESCENDING",
        42105,
        TransformCommand::Sort {
            kind: Sort::Length,
            descending: true,
        },
    ),
    (
        "IDM_EDIT_REDACT_SELECTION",
        42106,
        TransformCommand::Redact { bullet: false },
    ),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransformSelection {
    pub anchor: usize,
    pub caret: usize,
    pub anchor_virtual_space: usize,
    pub caret_virtual_space: usize,
}

impl TransformSelection {
    pub const fn new(anchor: usize, caret: usize) -> Self {
        Self {
            anchor,
            caret,
            anchor_virtual_space: 0,
            caret_virtual_space: 0,
        }
    }
    pub const fn caret(position: usize) -> Self {
        Self::new(position, position)
    }
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TransformSelectionMode {
    #[default]
    Stream,
    Rectangle {
        from_column: usize,
        to_column: usize,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommentSyntax {
    pub line: Option<String>,
    pub stream: Option<(String, String)>,
    pub at_line_start: bool,
    pub comment_empty_lines: bool,
    pub omit_line_space: bool,
}

#[derive(Clone, Debug)]
pub struct TransformOptions {
    pub line_ending: String,
    pub tab_width: usize,
    pub use_tabs: bool,
    pub force_line_indent: bool,
    pub read_only: bool,
    pub enabled: bool,
    pub main_selection: usize,
    pub selection_mode: TransformSelectionMode,
    pub comment_syntax: CommentSyntax,
    pub split_breaks: Vec<usize>,
    pub random_seed: u64,
    pub locale_keys: Option<Vec<Vec<u8>>>,
}

impl Default for TransformOptions {
    fn default() -> Self {
        Self {
            line_ending: "\r\n".into(),
            tab_width: 4,
            use_tabs: true,
            force_line_indent: true,
            read_only: false,
            enabled: true,
            main_selection: 0,
            selection_mode: TransformSelectionMode::Stream,
            comment_syntax: CommentSyntax::default(),
            split_breaks: Vec::new(),
            random_seed: 1,
            locale_keys: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransformReplacement {
    pub range: Range<usize>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionPolicy {
    MapOriginal,
    Explicit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransformPlan {
    pub replacements: Vec<TransformReplacement>,
    pub selections: Vec<TransformSelection>,
    pub selection_policy: SelectionPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransformError {
    ReadOnly,
    Disabled,
    InvalidSelection,
    OverlappingSelections,
    UnsupportedSelection,
    InvalidTabWidth,
    InvalidLineEnding,
    InvalidSplitBreak,
    MissingCommentSyntax,
    MissingLocaleKeys,
    InvalidNumber { line: usize },
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TransformError {}

pub fn plan_transform(
    text: &str,
    command: TransformCommand,
    selections: &[TransformSelection],
    options: &TransformOptions,
) -> Result<TransformPlan, TransformError> {
    let mut plan = build_plan(text, command, selections, options)?;
    let removed: usize = plan.replacements.iter().map(|e| e.range.len()).sum();
    let added: usize = plan.replacements.iter().map(|e| e.text.len()).sum();
    let length = text.len() - removed + added;
    for selection in &mut plan.selections {
        for position in [&mut selection.anchor, &mut selection.caret] {
            *position = (*position).min(length);
            while !result_is_char_boundary(text, &plan.replacements, *position) {
                *position -= 1;
            }
        }
    }
    Ok(plan)
}

fn result_is_char_boundary(text: &str, edits: &[TransformReplacement], position: usize) -> bool {
    let mut old = 0;
    let mut new = 0;
    for edit in edits {
        let unchanged = edit.range.start - old;
        if position <= new + unchanged {
            return text.is_char_boundary(old + position - new);
        }
        new += unchanged;
        if position <= new + edit.text.len() {
            return edit.text.is_char_boundary(position - new);
        }
        new += edit.text.len();
        old = edit.range.end;
    }
    text.is_char_boundary(old + position - new)
}

fn build_plan(
    text: &str,
    command: TransformCommand,
    selections: &[TransformSelection],
    options: &TransformOptions,
) -> Result<TransformPlan, TransformError> {
    if options.read_only {
        return Err(TransformError::ReadOnly);
    }
    if !options.enabled {
        return Err(TransformError::Disabled);
    }
    validate(text, selections, options)?;
    let mut replacements = Vec::new();
    let lines = parse_lines(text);
    let main = selections[options.main_selection];
    let mut expanded = None;
    match command {
        TransformCommand::Case(case) => {
            for selection in selections {
                let range = selection.range();
                let converted = convert_case(&text[range.clone()], case, options.random_seed);
                if text[range.clone()] != converted {
                    replacements.push(TransformReplacement {
                        range,
                        text: converted,
                    });
                }
            }
            if selections.len() == 1 && main.anchor != main.caret {
                let range = main.range();
                let selected = TransformSelection::new(
                    range.start,
                    map_position(range.end, &replacements, true),
                );
                return finish_plan(replacements, selections, Some(vec![selected]));
            }
        }
        TransformCommand::Trim(_)
        | TransformCommand::TrimAll
        | TransformCommand::TabsToSpaces
        | TransformCommand::SpacesToTabs { .. } => {
            if options.selection_mode != TransformSelectionMode::Stream {
                return Err(TransformError::UnsupportedSelection);
            }
            let (first, last) = scope_lines(&lines, main, true);
            for line in &lines[first..=last] {
                let original = &text[line.start..line.end];
                let transformed = match command {
                    TransformCommand::Trim(trim) => trim_line(original, trim).to_owned(),
                    TransformCommand::TrimAll => trim_line(original, Trim::Both).to_owned(),
                    TransformCommand::TabsToSpaces => tab_to_spaces(original, options.tab_width),
                    TransformCommand::SpacesToTabs { leading_only } => {
                        spaces_to_tabs(original, options.tab_width, leading_only)
                    }
                    _ => original.to_owned(),
                };
                push_diff(text, line.start..line.end, transformed, &mut replacements);
            }
            if command == TransformCommand::TrimAll {
                let end = if main.anchor != main.caret && main.range().end == lines[last].next {
                    lines[last].next
                } else {
                    lines[last].end
                };
                let range = lines[first].start..end;
                let mut trimmed = text[range.clone()].to_owned();
                for edit in replacements.iter().rev() {
                    trimmed.replace_range(
                        edit.range.start - range.start..edit.range.end - range.start,
                        &edit.text,
                    );
                }
                replacements.clear();
                let joined = join_text(&trimmed);
                push_diff(text, range.clone(), joined, &mut replacements);
            }
            if !replacements.is_empty() && main.anchor != main.caret {
                expanded = Some(vec![TransformSelection::new(
                    lines[first].start,
                    if main.range().end == lines[last].next {
                        lines[last].next
                    } else {
                        lines[last].end
                    },
                )]);
            }
        }
        TransformCommand::EolToSpace | TransformCommand::JoinLines => {
            if command == TransformCommand::EolToSpace
                && options.selection_mode != TransformSelectionMode::Stream
            {
                return Err(TransformError::UnsupportedSelection);
            }
            let range = if command == TransformCommand::JoinLines {
                let (first, last) = scope_lines(&lines, main, false);
                if first == last {
                    return finish_plan(replacements, selections, None);
                }
                let range = lines[first].start..lines[last].end;
                expanded = Some(vec![TransformSelection::new(range.start, range.end)]);
                range
            } else if main.anchor == main.caret {
                0..text.len()
            } else {
                main.range()
            };
            let first = line_at(&lines, range.start);
            for line in &lines[first..] {
                if line.end >= range.end {
                    break;
                }
                if line.next > line.end {
                    let space = if line.end > 0 && text.as_bytes()[line.end - 1] == b' ' {
                        ""
                    } else {
                        " "
                    };
                    replacements.push(TransformReplacement {
                        range: line.end..line.next,
                        text: space.into(),
                    });
                }
            }
        }
        TransformCommand::DuplicateLine | TransformCommand::DuplicateSelection => {
            for selection in selections {
                let range = selection.range();
                let (at, duplicate) =
                    if command == TransformCommand::DuplicateLine || range.is_empty() {
                        let line = lines[line_at(&lines, selection.caret)];
                        (
                            line.end,
                            format!("{}{}", options.line_ending, &text[line.start..line.end]),
                        )
                    } else {
                        (range.end, text[range].to_owned())
                    };
                replacements.push(TransformReplacement {
                    range: at..at,
                    text: duplicate,
                });
            }
            replacements.sort_by_key(|e| e.range.start);
            let mapped = selections
                .iter()
                .map(|s| TransformSelection {
                    anchor: map_position(s.anchor, &replacements, false),
                    caret: map_position(s.caret, &replacements, false),
                    ..*s
                })
                .collect();
            return finish_plan(replacements, selections, Some(mapped));
        }
        TransformCommand::MoveUp | TransformCommand::MoveDown => {
            let (first, last) = scope_lines(&lines, main, false);
            let down = command == TransformCommand::MoveDown;
            if (!down && first == 0) || (down && last + 1 == lines.len()) {
                return finish_plan(replacements, selections, None);
            }
            let start = if down { first } else { first - 1 };
            let end = if down { last + 1 } else { last };
            let mut order: Vec<_> = (start..=end).collect();
            if down {
                order.rotate_right(1);
            } else {
                order.rotate_left(1);
            }
            let range = lines[start].start..lines[end].next;
            let replacement = reorder_lines(
                text,
                &lines,
                &order,
                end + 1 == lines.len(),
                &options.line_ending,
            );
            let moved_start = if down {
                lines[start].start
                    + text[lines[end].start..lines[end].end].len()
                    + if lines[end].next > lines[end].end {
                        lines[end].next - lines[end].end
                    } else {
                        options.line_ending.len()
                    }
            } else {
                lines[start].start
            };
            let moved_length = replacement.len()
                - if down {
                    moved_start - range.start
                } else {
                    text[lines[start].start..lines[start].end].len()
                        + if end + 1 < lines.len() {
                            lines[start].next - lines[start].end
                        } else {
                            0
                        }
                };
            replacements.push(TransformReplacement {
                range,
                text: replacement,
            });
            return finish_plan(
                replacements,
                selections,
                Some(vec![TransformSelection::new(
                    moved_start + moved_length,
                    moved_start,
                )]),
            );
        }
        TransformCommand::RemoveEmptyLines { include_whitespace } => {
            let range = if main.anchor == main.caret {
                0..text.len()
            } else {
                main.range()
            };
            for line in &lines {
                if line.start < range.start || line.next > range.end {
                    continue;
                }
                let content = &text[line.start..line.end];
                if (content.is_empty()
                    || (include_whitespace && content.bytes().all(|c| matches!(c, b' ' | b'\t'))))
                    && line.next > line.end
                {
                    replacements.push(TransformReplacement {
                        range: line.start..line.next,
                        text: String::new(),
                    });
                }
            }
            let mut end = text.len();
            if range.end == text.len() {
                for (i, line) in lines.iter().enumerate().rev() {
                    let content = &text[line.start..line.end];
                    if line.start < range.start
                        || !(content.is_empty()
                            || (include_whitespace
                                && content.bytes().all(|c| matches!(c, b' ' | b'\t'))))
                    {
                        break;
                    }
                    end = if i > 0 {
                        lines[i - 1].end.max(range.start)
                    } else {
                        range.start
                    };
                }
                if end < text.len() {
                    replacements.retain(|e| e.range.end <= end);
                    replacements.push(TransformReplacement {
                        range: end..text.len(),
                        text: String::new(),
                    });
                }
            }
        }
        TransformCommand::RemoveDuplicates { consecutive_only } => {
            let (first, last) = scope_lines(&lines, main, true);
            if first < last {
                let mut retained = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut previous = None;
                for i in first..=last {
                    let key = if consecutive_only {
                        &text[lines[i].start..lines[i].next]
                    } else {
                        &text[lines[i].start..lines[i].end]
                    };
                    let keep = if consecutive_only {
                        previous != Some(key)
                    } else {
                        seen.insert(key)
                    };
                    if keep {
                        retained.push(i);
                    }
                    previous = Some(key);
                }
                let range = lines[first].start..lines[last].next;
                let transformed = reorder_lines(
                    text,
                    &lines,
                    &retained,
                    last + 1 == lines.len(),
                    &options.line_ending,
                );
                push_diff(text, range, transformed, &mut replacements);
            }
        }
        TransformCommand::SplitLines => {
            if selections.len() != 1 {
                return Err(TransformError::UnsupportedSelection);
            }
            let (first, last) = scope_lines(&lines, main, false);
            let mut breaks = options.split_breaks.clone();
            breaks.sort_unstable();
            breaks.dedup();
            for at in breaks {
                if at > text.len() || !text.is_char_boundary(at) {
                    return Err(TransformError::InvalidSplitBreak);
                }
                let line = lines[line_at(&lines, at)];
                if at <= line.start
                    || at >= line.end
                    || at < lines[first].start
                    || at > lines[last].end
                {
                    return Err(TransformError::InvalidSplitBreak);
                }
                replacements.push(TransformReplacement {
                    range: at..at,
                    text: options.line_ending.clone(),
                });
            }
            expanded = Some(vec![TransformSelection::new(
                lines[first].start,
                lines[last].end,
            )]);
        }
        TransformCommand::Sort { kind, descending } => {
            if selections.len() > 1 && options.selection_mode == TransformSelectionMode::Stream {
                return Err(TransformError::UnsupportedSelection);
            }
            let (first, mut last) = if options.selection_mode == TransformSelectionMode::Stream {
                scope_lines(&lines, main, true)
            } else {
                let first = selections
                    .iter()
                    .map(|s| line_at(&lines, s.range().start))
                    .min()
                    .ok_or(TransformError::InvalidSelection)?;
                let last = selections
                    .iter()
                    .map(|s| line_at(&lines, s.range().end))
                    .max()
                    .ok_or(TransformError::InvalidSelection)?;
                (first, last)
            };
            if kind == Sort::Locale
                && main.anchor == main.caret
                && options.selection_mode == TransformSelectionMode::Stream
                && lines[last].start == text.len()
                && last > first
            {
                last -= 1;
            }
            if first >= last {
                return finish_plan(replacements, selections, None);
            }
            let mut order: Vec<_> = (first..=last).collect();
            let keys: Vec<Vec<u16>> = lines
                .iter()
                .map(|line| sort_key(&text[line.start..line.end], options.selection_mode))
                .collect();
            match kind {
                Sort::Reverse => order.reverse(),
                Sort::Random => {
                    let mut seed = options.random_seed;
                    for i in (1..order.len()).rev() {
                        seed = random_next(seed);
                        order.swap(i, (seed % (i as u64 + 1)) as usize);
                    }
                }
                Sort::DecimalComma | Sort::DecimalDot => {
                    let mut numbers = vec![None; lines.len()];
                    for &i in &order {
                        numbers[i] = decimal_key(&keys[i], kind == Sort::DecimalComma)
                            .map_err(|()| TransformError::InvalidNumber { line: i + 1 })?;
                    }
                    order.sort_by(|&a, &b| {
                        let cmp = match (numbers[a], numbers[b]) {
                            (None, None) => std::cmp::Ordering::Equal,
                            (None, _) => std::cmp::Ordering::Less,
                            (_, None) => std::cmp::Ordering::Greater,
                            (Some(a), Some(b)) => {
                                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
                            }
                        };
                        if descending { cmp.reverse() } else { cmp }
                    });
                }
                Sort::Locale => {
                    let locale_keys = options
                        .locale_keys
                        .as_ref()
                        .filter(|k| k.len() == lines.len())
                        .ok_or(TransformError::MissingLocaleKeys)?;
                    order.sort_by(|&a, &b| {
                        let cmp = locale_keys[a].cmp(&locale_keys[b]);
                        if descending { cmp.reverse() } else { cmp }
                    });
                }
                _ => {
                    order.sort_by(|&a, &b| {
                        let cmp = match kind {
                            Sort::Lexical => keys[a].cmp(&keys[b]),
                            Sort::CaseInsensitive => fold_key(&keys[a]).cmp(&fold_key(&keys[b])),
                            Sort::Integer => integer_compare(&keys[a], &keys[b]),
                            Sort::Length => keys[a].len().cmp(&keys[b].len()),
                            _ => std::cmp::Ordering::Equal,
                        };
                        if descending { cmp.reverse() } else { cmp }
                    });
                }
            }
            let range = lines[first].start..lines[last].next;
            let mut transformed = String::new();
            if kind == Sort::Locale {
                transformed = reorder_lines(
                    text,
                    &lines,
                    &order,
                    last + 1 == lines.len(),
                    &options.line_ending,
                );
            } else {
                for (slot, &i) in (first..=last).zip(&order) {
                    transformed.push_str(&text[lines[i].start..lines[i].end]);
                    transformed.push_str(&text[lines[slot].end..lines[slot].next]);
                }
            }
            push_diff(text, range.clone(), transformed.clone(), &mut replacements);
            if options.selection_mode == TransformSelectionMode::Stream {
                if main.anchor != main.caret {
                    if kind == Sort::Locale {
                        let end = range.start + transformed.len();
                        let selected = if main.anchor > main.caret {
                            TransformSelection::new(end, range.start)
                        } else {
                            TransformSelection::new(range.start, end)
                        };
                        return finish_plan(replacements, selections, Some(vec![selected]));
                    }
                    let ending = lines[last].next - lines[last].end;
                    let end = range.start + transformed.len() - ending;
                    return finish_plan(
                        replacements,
                        selections,
                        Some(vec![TransformSelection::new(range.start, end)]),
                    );
                }
                if kind == Sort::Locale {
                    let old_line = line_at(&lines, main.caret);
                    if let Some(new_line) = order.iter().position(|&i| i == old_line) {
                        let transformed_lines = parse_lines(&transformed);
                        let new_pos = range.start
                            + transformed_lines[new_line].start
                            + (main.caret - lines[old_line].start).min(
                                transformed_lines[new_line].end - transformed_lines[new_line].start,
                            );
                        return finish_plan(
                            replacements,
                            selections,
                            Some(vec![TransformSelection::caret(new_pos)]),
                        );
                    }
                }
                return finish_plan(replacements, selections, Some(selections.to_vec()));
            }
            let transformed_lines = parse_lines(&transformed);
            let mut mapped = selections.to_vec();
            for selection in &mut mapped {
                for position in [&mut selection.anchor, &mut selection.caret] {
                    let row = line_at(&lines, *position);
                    let offset = *position - lines[row].start;
                    let target = transformed_lines[row - first];
                    let mut column = offset.min(target.end - target.start);
                    while !transformed.is_char_boundary(target.start + column) {
                        column -= 1;
                    }
                    *position = range.start + target.start + column;
                }
            }
            return finish_plan(replacements, selections, Some(mapped));
        }
        TransformCommand::Indent | TransformCommand::Unindent => {
            let forwards = command == TransformCommand::Indent;
            let mut line_changes = std::collections::BTreeMap::new();
            let mut post_selections = Vec::new();
            for selection in selections {
                let (first, last) = scope_lines(&lines, *selection, false);
                if (options.force_line_indent && selections.len() == 1) || first != last {
                    for i in first..=last {
                        line_changes.insert(i, false);
                    }
                    if first != last {
                        let start = lines[first].start;
                        let end = lines[last].next;
                        post_selections.push(if selection.anchor > selection.caret {
                            TransformSelection::new(end, start)
                        } else {
                            TransformSelection::new(start, end)
                        });
                    } else {
                        post_selections.push(*selection);
                    }
                } else {
                    let line = lines[first];
                    let content = &text[line.start..line.end];
                    let indent_bytes = content
                        .bytes()
                        .take_while(|c| matches!(c, b' ' | b'\t'))
                        .count();
                    let indent = display_column(&content[..indent_bytes], options.tab_width);
                    let column = display_column(
                        &text[line.start..selection.caret.min(line.end)],
                        options.tab_width,
                    );
                    if column <= indent {
                        line_changes.insert(first, true);
                        post_selections.push(*selection);
                    } else if forwards {
                        let range = selection.range();
                        let insertion = if options.use_tabs {
                            "\t".to_owned()
                        } else {
                            " ".repeat(options.tab_width - column % options.tab_width)
                        };
                        replacements.push(TransformReplacement {
                            range: range.clone(),
                            text: insertion,
                        });
                        post_selections.push(TransformSelection::caret(range.end));
                    } else {
                        let target_column =
                            column.saturating_sub(1) / options.tab_width * options.tab_width;
                        let mut target = line.start;
                        for (offset, _) in content.char_indices() {
                            if display_column(&content[..offset], options.tab_width) > target_column
                            {
                                break;
                            }
                            target = line.start + offset;
                        }
                        post_selections.push(TransformSelection::caret(target));
                    }
                }
            }
            for (i, round_to_stop) in line_changes {
                let line = lines[i];
                if forwards
                    && line.start == line.end
                    && selections.len() == 1
                    && line_at(&lines, main.anchor) != line_at(&lines, main.caret)
                {
                    continue;
                }
                let content = &text[line.start..line.end];
                let bytes = content
                    .bytes()
                    .take_while(|c| matches!(c, b' ' | b'\t'))
                    .count();
                let indent = display_column(&content[..bytes], options.tab_width);
                let target = if forwards {
                    indent + options.tab_width
                        - if round_to_stop {
                            indent % options.tab_width
                        } else {
                            0
                        }
                } else {
                    indent.saturating_sub(options.tab_width)
                };
                let indentation = if options.use_tabs {
                    format!(
                        "{}{}",
                        "\t".repeat(target / options.tab_width),
                        " ".repeat(target % options.tab_width)
                    )
                } else {
                    " ".repeat(target)
                };
                push_diff(
                    text,
                    line.start..line.start + bytes,
                    indentation,
                    &mut replacements,
                );
            }
            replacements.sort_by_key(|e| e.range.start);
            let explicit = post_selections
                .iter()
                .zip(selections)
                .map(|(s, original)| {
                    let (first, last) = scope_lines(&lines, *original, false);
                    TransformSelection {
                        anchor: map_position(s.anchor, &replacements, first == last),
                        caret: map_position(s.caret, &replacements, first == last),
                        ..*s
                    }
                })
                .collect();
            return finish_plan(replacements, selections, Some(explicit));
        }
        TransformCommand::Comment(comment) => {
            return comment_plan(text, &lines, comment, selections, options);
        }
        TransformCommand::TransposeLine => {
            let current = line_at(&lines, main.caret);
            if current > 0 {
                let previous = lines[current - 1];
                let line = lines[current];
                replacements.push(TransformReplacement {
                    range: previous.start..previous.end,
                    text: text[line.start..line.end].into(),
                });
                replacements.push(TransformReplacement {
                    range: line.start..line.end,
                    text: text[previous.start..previous.end].into(),
                });
                let position =
                    previous.start + (line.end - line.start) + (previous.next - previous.end);
                return finish_plan(
                    replacements,
                    selections,
                    Some(vec![TransformSelection::caret(position)]),
                );
            }
        }
        TransformCommand::BlankLineAbove | TransformCommand::BlankLineBelow => {
            let current = line_at(&lines, main.caret);
            let above = command == TransformCommand::BlankLineAbove;
            let position = if above {
                if current == 0 {
                    0
                } else {
                    lines[current - 1].end
                }
            } else {
                lines[current].next
            };
            replacements.push(TransformReplacement {
                range: position..position,
                text: options.line_ending.clone(),
            });
            let mut changed = text.to_owned();
            changed.insert_str(position, &options.line_ending);
            let result_lines = parse_lines(&changed);
            let caret = result_lines
                .get(current + usize::from(!above))
                .map_or(changed.len(), |line| line.start);
            return finish_plan(
                replacements,
                selections,
                Some(vec![TransformSelection::caret(caret)]),
            );
        }
        TransformCommand::Redact { bullet } => {
            for selection in selections {
                let range = selection.range();
                let mask = if bullet { '●' } else { '█' };
                let replacement = text[range.clone()]
                    .chars()
                    .map(|c| if matches!(c, '\r' | '\n') { c } else { mask })
                    .collect();
                push_diff(text, range, replacement, &mut replacements);
            }
        }
    }
    let explicit = expanded.map(|sels| {
        sels.into_iter()
            .map(|s| {
                TransformSelection::new(
                    map_position(s.anchor, &replacements, false),
                    map_position(s.caret, &replacements, true),
                )
            })
            .collect()
    });
    finish_plan(replacements, selections, explicit)
}

fn display_column(text: &str, width: usize) -> usize {
    text.chars().fold(0, |col, c| {
        col + if c == '\t' { width - col % width } else { 1 }
    })
}

fn comment_plan(
    text: &str,
    lines: &[Line],
    command: Comment,
    selections: &[TransformSelection],
    options: &TransformOptions,
) -> Result<TransformPlan, TransformError> {
    let syntax = &options.comment_syntax;
    let line_symbol = syntax.line.as_deref().filter(|s| !s.is_empty());
    let stream_symbols = syntax
        .stream
        .as_ref()
        .filter(|(s, e)| !s.is_empty() && !e.is_empty());
    if line_symbol.is_none() && stream_symbols.is_none() {
        return Err(TransformError::MissingCommentSyntax);
    }
    let main = selections[options.main_selection];
    let mut edits = Vec::new();
    let command = match command {
        Comment::Stream if stream_symbols.is_none() => Comment::Line,
        Comment::UncommentStream if stream_symbols.is_none() => Comment::UncommentLine,
        Comment::UncommentLine if line_symbol.is_none() => Comment::UncommentStream,
        other => other,
    };
    if command == Comment::Stream {
        let (start, end) = stream_symbols.ok_or(TransformError::MissingCommentSyntax)?;
        let range = if main.anchor == main.caret {
            let line = lines[line_at(lines, main.caret)];
            let indent = text[line.start..line.end]
                .bytes()
                .take_while(|c| matches!(c, b' ' | b'\t'))
                .count();
            line.start + indent..line.end
        } else {
            main.range()
        };
        edits.push(TransformReplacement {
            range: range.start..range.start,
            text: format!("{start} "),
        });
        edits.push(TransformReplacement {
            range: range.end..range.end,
            text: format!(" {end}"),
        });
        let selected = if main.anchor > main.caret {
            TransformSelection::new(range.end + start.len() + 1, range.start + start.len() + 1)
        } else {
            TransformSelection::new(range.start + start.len() + 1, range.end + start.len() + 1)
        };
        return finish_plan(edits, selections, Some(vec![selected]));
    }
    if command == Comment::UncommentStream {
        let (start, end) = stream_symbols.ok_or(TransformError::MissingCommentSyntax)?;
        let edits = uncomment_stream(text, main.range(), start, end);
        let kept = if edits.is_empty() {
            selections
        } else {
            std::slice::from_ref(&main)
        };
        let mut plan = finish_plan(edits, kept, None)?;
        if kept.len() != selections.len() {
            plan.selection_policy = SelectionPolicy::Explicit;
        }
        return Ok(plan);
    }
    let (first, last) = scope_lines(lines, main, false);
    let mut uncommented = false;
    for line in &lines[first..=last] {
        let content = &text[line.start..line.end];
        let indent = content
            .bytes()
            .take_while(|c| matches!(c, b' ' | b'\t'))
            .count();
        if indent == content.len() && !syntax.comment_empty_lines {
            continue;
        }
        let at = line.start + if syntax.at_line_start { 0 } else { indent };
        let body = &text[at..line.end];
        if let Some(symbol) = line_symbol {
            let matched = body
                .get(..symbol.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(symbol));
            if command != Comment::Line && matched {
                let length = symbol.len()
                    + usize::from(
                        !syntax.omit_line_space && body.as_bytes().get(symbol.len()) == Some(&b' '),
                    );
                edits.push(TransformReplacement {
                    range: at..at + length,
                    text: String::new(),
                });
                uncommented = true;
            } else if command != Comment::UncommentLine {
                edits.push(TransformReplacement {
                    range: at..at,
                    text: format!("{symbol}{}", if syntax.omit_line_space { "" } else { " " }),
                });
            }
        } else if let Some((start, end)) = stream_symbols {
            let matched = body
                .get(..start.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(start))
                && body
                    .get(body.len().saturating_sub(end.len())..)
                    .is_some_and(|s| s.eq_ignore_ascii_case(end))
                && body.len() >= start.len() + end.len();
            if command != Comment::Line && matched {
                let left =
                    start.len() + usize::from(body.as_bytes().get(start.len()) == Some(&b' '));
                let right = end.len()
                    + usize::from(body.as_bytes().get(body.len() - end.len() - 1) == Some(&b' '));
                if left + right <= body.len() {
                    edits.push(TransformReplacement {
                        range: at..at + left,
                        text: String::new(),
                    });
                    edits.push(TransformReplacement {
                        range: line.end - right..line.end,
                        text: String::new(),
                    });
                } else {
                    edits.push(TransformReplacement {
                        range: at..line.end,
                        text: String::new(),
                    });
                }
                uncommented = true;
            } else if command != Comment::UncommentLine {
                edits.push(TransformReplacement {
                    range: at..at,
                    text: format!("{start} "),
                });
                edits.push(TransformReplacement {
                    range: line.end..line.end,
                    text: format!(" {end}"),
                });
            }
        }
    }
    if command == Comment::UncommentLine
        && !uncommented
        && let Some((start, end)) = stream_symbols
    {
        edits = uncomment_stream(text, main.range(), start, end);
    }
    let mut plan = finish_plan(edits, std::slice::from_ref(&main), None)?;
    if selections.len() > 1 {
        plan.selection_policy = SelectionPolicy::Explicit;
    }
    Ok(plan)
}

fn uncomment_stream(
    text: &str,
    mut selection: Range<usize>,
    start: &str,
    end: &str,
) -> Vec<TransformReplacement> {
    let mut working = text.to_owned();
    let mut origins: Vec<usize> = (0..text.len()).collect();
    let mut deleted = vec![false; text.len()];
    loop {
        let mut tokens = Vec::new();
        for (i, _) in working.char_indices() {
            if working
                .get(i..i + start.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(start))
            {
                tokens.push((i, true));
            }
            if working
                .get(i..i + end.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(end))
            {
                tokens.push((i, false));
            }
        }
        let pairs: Vec<_> = tokens
            .windows(2)
            .filter_map(|p| {
                if p[0].1 && !p[1].1 {
                    Some((p[0].0, p[1].0))
                } else {
                    None
                }
            })
            .collect();
        let pair = pairs
            .iter()
            .copied()
            .find(|&(s, e)| s < selection.start && e >= selection.start)
            .or_else(|| {
                pairs
                    .iter()
                    .copied()
                    .find(|&(s, e)| s < selection.end && e >= selection.end)
            })
            .or_else(|| {
                pairs
                    .iter()
                    .copied()
                    .find(|&(s, e)| s >= selection.start && e + end.len() <= selection.end)
            });
        let Some((s, e)) = pair else {
            break;
        };
        let left = s..s
            + start.len()
            + usize::from(working.as_bytes().get(s + start.len()) == Some(&b' '));
        let right = e - usize::from(e > left.end && working.as_bytes().get(e - 1) == Some(&b' '))
            ..e + end.len();
        for range in [right, left] {
            for &origin in &origins[range.clone()] {
                deleted[origin] = true;
            }
            working.replace_range(range.clone(), "");
            origins.drain(range.clone());
            selection.start = if selection.start >= range.end {
                selection.start - range.len()
            } else {
                selection.start.min(range.start)
            };
            selection.end = if selection.end >= range.end {
                selection.end - range.len()
            } else {
                selection.end.min(range.start)
            };
        }
    }
    let mut edits = Vec::new();
    let mut i = 0;
    while i < deleted.len() {
        if !deleted[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < deleted.len() && deleted[i] {
            i += 1;
        }
        edits.push(TransformReplacement {
            range: start..i,
            text: String::new(),
        });
    }
    edits
}

fn sort_key(text: &str, mode: TransformSelectionMode) -> Vec<u16> {
    let units: Vec<_> = text.encode_utf16().collect();
    match mode {
        TransformSelectionMode::Stream => units,
        TransformSelectionMode::Rectangle {
            from_column,
            to_column,
        } if to_column != 0 => {
            let start = from_column.min(units.len());
            let end = if from_column == to_column {
                units.len()
            } else {
                to_column.min(units.len())
            };
            units[start..end].to_vec()
        }
        _ => units,
    }
}

fn fold_key(key: &[u16]) -> Vec<u16> {
    key.iter()
        .map(|&u| char::from_u32(u32::from(u)).map_or(u, |c| simple_case(c, false) as u16))
        .collect()
}

fn decimal_key(key: &[u16], comma: bool) -> Result<Option<f64>, ()> {
    let allowed_separator = if comma { ',' as u16 } else { '.' as u16 };
    let prefix: Vec<_> = key
        .iter()
        .copied()
        .take_while(|&u| matches!(u, 9 | 10 | 13 | 32 | 45 | 48..=57) || u == allowed_separator)
        .collect();
    if prefix.iter().all(|u| matches!(u, 9 | 10 | 13 | 32)) {
        return Ok(None);
    }
    let s = String::from_utf16_lossy(&prefix).replace(',', ".");
    let trimmed = s.trim_start_matches([' ', '\t', '\r', '\n']);
    let mut end = usize::from(trimmed.starts_with('-'));
    let mut digits = 0;
    let mut dot = false;
    for &b in &trimmed.as_bytes()[end..] {
        if b.is_ascii_digit() {
            digits += 1;
            end += 1;
        } else if b == b'.' && !dot {
            dot = true;
            end += 1;
        } else {
            break;
        }
    }
    if digits == 0 {
        return Err(());
    }
    let value: f64 = trimmed[..end].parse().map_err(|_| ())?;
    if !value.is_finite() {
        return Err(());
    }
    Ok(Some(value))
}

fn integer_compare(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let digit = |c: u16| (48..=57).contains(&c);
    let mut i = 0;
    let mut j = 0;
    while i < a.len() && j < b.len() {
        let aneg = a[i] == 45 && a.get(i + 1).is_some_and(|c| digit(*c));
        let bneg = b[j] == 45 && b.get(j + 1).is_some_and(|c| digit(*c));
        let anum = digit(a[i]) || aneg;
        let bnum = digit(b[j]) || bneg;
        let cmp = if anum && bnum {
            if aneg != bneg {
                return if aneg {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            i += usize::from(aneg);
            j += usize::from(bneg);
            let ai = i;
            let bj = j;
            while i < a.len() && digit(a[i]) {
                i += 1;
            }
            while j < b.len() && digit(b[j]) {
                j += 1;
            }
            let az = a[ai..i].iter().take_while(|&&c| c == 48).count();
            let bz = b[bj..j].iter().take_while(|&&c| c == 48).count();
            let cmp = (i - ai - az)
                .cmp(&(j - bj - bz))
                .then_with(|| a[ai + az..i].cmp(&b[bj + bz..j]));
            let cmp = if aneg { cmp.reverse() } else { cmp };
            cmp.then_with(|| bz.cmp(&az))
        } else if anum != bnum {
            let cmp = a[i].cmp(&b[j]);
            i += 1;
            j += 1;
            cmp
        } else {
            if a[i] == 45 {
                i += 1;
            }
            if b[j] == 45 {
                j += 1;
            }
            let ai = i;
            let bj = j;
            while i < a.len() && !digit(a[i]) && a[i] != 45 {
                i += 1;
            }
            while j < b.len() && !digit(b[j]) && b[j] != 45 {
                j += 1;
            }
            a[ai..i].cmp(&b[bj..j])
        };
        if cmp != Ordering::Equal {
            return cmp;
        }
    }
    a[i..].cmp(&b[j..])
}

fn reorder_lines(
    text: &str,
    lines: &[Line],
    order: &[usize],
    reaches_end: bool,
    eol: &str,
) -> String {
    let mut result = String::new();
    for (n, &index) in order.iter().enumerate() {
        let line = lines[index];
        result.push_str(&text[line.start..line.end]);
        if n + 1 < order.len() || !reaches_end {
            if line.next > line.end {
                result.push_str(&text[line.end..line.next]);
            } else if n + 1 < order.len() {
                result.push_str(eol);
            }
        }
    }
    result
}

#[derive(Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    next: usize,
}

fn parse_lines(text: &str) -> Vec<Line> {
    let mut result = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            let end = i;
            i += if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
            result.push(Line {
                start,
                end,
                next: i,
            });
            start = i;
        } else {
            i += 1;
        }
    }
    result.push(Line {
        start,
        end: text.len(),
        next: text.len(),
    });
    result
}

fn line_at(lines: &[Line], position: usize) -> usize {
    lines
        .partition_point(|line| line.start <= position)
        .saturating_sub(1)
}

fn scope_lines(
    lines: &[Line],
    selection: TransformSelection,
    entire_if_empty: bool,
) -> (usize, usize) {
    let range = selection.range();
    if range.is_empty() && entire_if_empty {
        return (0, lines.len() - 1);
    }
    let first = line_at(lines, range.start);
    let mut last = line_at(lines, range.end);
    if last > first && range.end == lines[last].start {
        last -= 1;
    }
    (first, last)
}

fn push_diff(
    source: &str,
    range: Range<usize>,
    replacement: String,
    edits: &mut Vec<TransformReplacement>,
) {
    let original = &source[range.clone()];
    if original == replacement {
        return;
    }
    let mut prefix = original
        .bytes()
        .zip(replacement.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !original.is_char_boundary(prefix) || !replacement.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = original[prefix..]
        .bytes()
        .rev()
        .zip(replacement[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !original.is_char_boundary(original.len() - suffix)
        || !replacement.is_char_boundary(replacement.len() - suffix)
    {
        suffix -= 1;
    }
    edits.push(TransformReplacement {
        range: range.start + prefix..range.end - suffix,
        text: replacement[prefix..replacement.len() - suffix].into(),
    });
}

fn trim_line(text: &str, trim: Trim) -> &str {
    let blank = |c| c == ' ' || c == '\t';
    match trim {
        Trim::Leading => text.trim_start_matches(blank),
        Trim::Trailing => text.trim_end_matches(blank),
        Trim::Both => text.trim_matches(blank),
    }
}

fn tab_to_spaces(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut column = 0;
    for c in text.chars() {
        if c == '\t' {
            let n = width - column % width;
            result.extend(std::iter::repeat_n(' ', n));
            column += n;
        } else {
            result.push(c);
            column += 1;
        }
    }
    result
}

fn spaces_to_tabs(text: &str, width: usize, leading_only: bool) -> String {
    let chars: Vec<_> = text.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    let mut column = 0;
    let mut leading = true;
    while i < chars.len() {
        if chars[i] == ' ' && (!leading_only || leading) {
            let end = i + chars[i..].iter().take_while(|c| **c == ' ').count();
            while i < end {
                let to_stop = width - column % width;
                if end - i >= to_stop
                    && (to_stop > 1 || chars.get(i + 1).is_some_and(|c| matches!(c, ' ' | '\t')))
                {
                    result.push('\t');
                    i += to_stop;
                    column += to_stop;
                } else if chars.get(end) == Some(&'\t') && end - i < to_stop {
                    result.push('\t');
                    i = end + 1;
                    column += to_stop;
                    break;
                } else {
                    result.push(' ');
                    i += 1;
                    column += 1;
                }
            }
        } else {
            let c = chars[i];
            result.push(c);
            if c == '\t' {
                column += width - column % width;
            } else {
                column += 1;
                leading = false;
            }
            i += 1;
        }
    }
    result
}

fn join_text(text: &str) -> String {
    let mut result = String::new();
    for line in parse_lines(text) {
        result.push_str(&text[line.start..line.end]);
        if line.next > line.end && !result.ends_with(' ') {
            result.push(' ');
        }
    }
    result
}

fn validate(
    text: &str,
    selections: &[TransformSelection],
    options: &TransformOptions,
) -> Result<(), TransformError> {
    if selections.is_empty() || options.main_selection >= selections.len() {
        return Err(TransformError::InvalidSelection);
    }
    if options.tab_width == 0 {
        return Err(TransformError::InvalidTabWidth);
    }
    if !matches!(options.line_ending.as_str(), "\n" | "\r" | "\r\n") {
        return Err(TransformError::InvalidLineEnding);
    }
    let mut ranges: Vec<_> = selections.iter().map(|s| s.range()).collect();
    for r in &ranges {
        if r.end > text.len() || !text.is_char_boundary(r.start) || !text.is_char_boundary(r.end) {
            return Err(TransformError::InvalidSelection);
        }
    }
    ranges.sort_by_key(|r| (r.start, r.end));
    if ranges.windows(2).any(|p| p[0].end > p[1].start) {
        return Err(TransformError::OverlappingSelections);
    }
    if let TransformSelectionMode::Rectangle {
        from_column,
        to_column,
    } = options.selection_mode
        && from_column > to_column
    {
        return Err(TransformError::InvalidSelection);
    }
    Ok(())
}

fn finish_plan(
    mut replacements: Vec<TransformReplacement>,
    selections: &[TransformSelection],
    explicit: Option<Vec<TransformSelection>>,
) -> Result<TransformPlan, TransformError> {
    replacements.sort_by_key(|e| (e.range.start, e.range.end));
    let mut coalesced: Vec<TransformReplacement> = Vec::new();
    for edit in replacements {
        if let Some(previous) = coalesced.last_mut()
            && previous.range.is_empty()
            && previous.range == edit.range
        {
            previous.text.push_str(&edit.text);
        } else {
            coalesced.push(edit);
        }
    }
    let replacements = coalesced;
    if replacements
        .windows(2)
        .any(|e| e[0].range.end > e[1].range.start)
    {
        return Err(TransformError::OverlappingSelections);
    }
    let policy = if explicit.is_some() {
        SelectionPolicy::Explicit
    } else {
        SelectionPolicy::MapOriginal
    };
    let mapped = explicit.unwrap_or_else(|| {
        selections
            .iter()
            .map(|s| TransformSelection {
                anchor: map_position(s.anchor, &replacements, true),
                caret: map_position(s.caret, &replacements, true),
                ..*s
            })
            .collect()
    });
    Ok(TransformPlan {
        replacements,
        selections: mapped,
        selection_policy: policy,
    })
}

fn map_position(position: usize, edits: &[TransformReplacement], right: bool) -> usize {
    let mut removed = 0;
    let mut added = 0;
    for edit in edits {
        if position < edit.range.start || (position == edit.range.start && !right) {
            break;
        }
        if position == edit.range.start && !edit.range.is_empty() {
            return edit.range.start - removed + added;
        }
        if position <= edit.range.end {
            return edit.range.start - removed + added + if right { edit.text.len() } else { 0 };
        }
        removed += edit.range.len();
        added += edit.text.len();
    }
    position - removed + added
}

fn simple_case(c: char, upper: bool) -> char {
    if c.len_utf16() != 1 {
        return c;
    }
    let mapped: Vec<char> = if upper {
        c.to_uppercase().collect()
    } else {
        c.to_lowercase().collect()
    };
    if mapped.len() == 1 {
        mapped[0]
    } else if c == '\u{130}' && !upper {
        'i'
    } else {
        c
    }
}

fn convert_case(text: &str, case: Case, mut seed: u64) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let mut new_sentence = true;
    let mut cr = false;
    let mut lf = false;
    for i in 0..chars.len() {
        let c = chars[i];
        let alpha = win_alpha(c);
        let previous = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1).copied();
        chars[i] = match case {
            Case::Upper => simple_case(c, true),
            Case::Lower => simple_case(c, false),
            Case::Invert => simple_case(c, c.is_lowercase()),
            Case::Random if alpha => {
                seed = random_next(seed);
                simple_case(c, seed & 1 != 0)
            }
            Case::ProperForce | Case::ProperBlend if alpha => {
                let apostrophe = i >= 2
                    && matches!(chars[i - 1], '\'' | '\u{2019}' | '\u{2018}')
                    && win_alphanumeric(chars[i - 2]);
                if !apostrophe && previous.is_none_or(|p| !win_alphanumeric(p)) {
                    simple_case(c, true)
                } else if case == Case::ProperForce {
                    simple_case(c, false)
                } else {
                    c
                }
            }
            Case::SentenceForce | Case::SentenceBlend => {
                if alpha {
                    let mut converted = if new_sentence {
                        simple_case(c, true)
                    } else if case == Case::SentenceForce {
                        simple_case(c, false)
                    } else {
                        c
                    };
                    new_sentence = false;
                    cr = false;
                    lf = false;
                    if converted == 'i'
                        && previous.is_some_and(|p| p.is_whitespace() || matches!(p, '(' | '"'))
                        && next.is_some_and(|n| n.is_whitespace() || n == '\'')
                    {
                        converted = 'I';
                    }
                    converted
                } else {
                    if matches!(c, '.' | '!' | '?') {
                        new_sentence = next.is_some_and(|n| !win_alphanumeric(n));
                    } else if c == '\r' {
                        if cr {
                            new_sentence = true;
                        } else {
                            cr = true;
                        }
                    } else if c == '\n' {
                        if lf {
                            new_sentence = true;
                        } else {
                            lf = true;
                        }
                    }
                    c
                }
            }
            _ => c,
        };
    }
    chars.into_iter().collect()
}

fn win_alpha(c: char) -> bool {
    c.len_utf16() == 1 && c.is_alphabetic()
}
fn win_alphanumeric(c: char) -> bool {
    c.len_utf16() == 1 && c.is_alphanumeric()
}

fn random_next(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}
