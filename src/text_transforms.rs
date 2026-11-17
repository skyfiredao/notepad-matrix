pub mod session;

pub struct TransformOutcome {
    pub result: Result<(), String>,
    pub last_target: Option<std::ops::Range<i128>>,
}

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
enum SortColumns {
    #[default]
    WholeLine,
    Range {
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
    pub tab_width: usize,
    pub use_tabs: bool,
    pub force_line_indent: bool,
    pub comment_syntax: CommentSyntax,
    pub split_breaks: Vec<crate::editor::ByteOffset>,
    pub random_seed: u64,
    pub locale_keys: Option<Vec<Vec<u8>>>,
}

impl Default for TransformOptions {
    fn default() -> Self {
        Self {
            tab_width: 4,
            use_tabs: true,
            force_line_indent: true,
            comment_syntax: CommentSyntax::default(),
            split_breaks: Vec::new(),
            random_seed: 1,
            locale_keys: None,
        }
    }
}

fn sort_order(lines: &[Vec<u16>], kind: Sort, descending: bool, columns: SortColumns, seed: u64) -> Result<Vec<usize>, usize> {
    let mut order: Vec<_> = (0..lines.len()).collect();
    let keys: Vec<_> = lines.iter().map(|line| sort_key(line, columns)).collect();
    match kind {
        Sort::Reverse => order.reverse(),
        Sort::Random => {
            let mut random = SortRandom::new(seed);
            for index in 1..order.len() {
                let target = random.index((index + 1) as u64) as usize;
                order.swap(index, target);
            }
        }
        Sort::DecimalComma | Sort::DecimalDot => {
            let mut numbers = vec![None; lines.len()];
            for &index in &order {
                numbers[index] = decimal_key(&keys[index], kind == Sort::DecimalComma).map_err(|()| index)?;
            }
            order.sort_by(|&a, &b| {
                let comparison = match (numbers[a], numbers[b]) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, _) => std::cmp::Ordering::Less,
                    (_, None) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
                };
                if descending { comparison.reverse() } else { comparison }
            });
        }
        _ => {
            let compare = |&a: &usize, &b: &usize| {
                let comparison = match kind {
                    Sort::Lexical => keys[a].cmp(&keys[b]),
                    Sort::CaseInsensitive => fold_key(&keys[a]).cmp(&fold_key(&keys[b])),
                    Sort::Integer => integer_compare(&keys[a], &keys[b]),
                    Sort::Length => keys[a].len().cmp(&keys[b].len()),
                    _ => std::cmp::Ordering::Equal,
                };
                if descending { comparison.reverse() } else { comparison }
            };
            if matches!(columns, SortColumns::Range { to_column, .. } if to_column != 0) { order.sort_by(compare); }
            else { order.sort_unstable_by(compare); }
        }
    }
    Ok(order)
}

fn sort_key(units: &[u16], columns: SortColumns) -> Vec<u16> {
    match columns {
        SortColumns::WholeLine => units.to_vec(),
        SortColumns::Range {
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
        _ => units.to_vec(),
    }
}

fn fold_key(key: &[u16]) -> Vec<u16> {
    key.iter()
        .copied()
        .take_while(|&unit| unit != 0)
        .map(|unit| if (65..=90).contains(&unit) { unit + 32 } else { unit })
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

fn tab_to_spaces(text: &[u8], width: usize, caret: usize) -> (Vec<u8>, usize, bool) {
    let source = &text[..text.iter().position(|byte| *byte == 0).unwrap_or(text.len())];
    if !source.contains(&b'\t') { return (text.to_vec(), 0, false); }
    let mut result = Vec::new();
    let mut column = 0;
    let mut position = 0;
    for (at, &c) in source.iter().enumerate() {
        if c == b'\t' {
            let n = width - column % width;
            result.extend(std::iter::repeat_n(b' ', n));
            if at < caret { position += n; }
            column += n;
        } else {
            result.push(c);
            if at < caret { position += 1; }
            if matches!(c, b'\r' | b'\n') { column = 0; }
            else if c & 0xc0 != 0x80 { column += 1; }
        }
    }
    (result, position, true)
}

fn spaces_to_tabs(text: &[u8], width: usize, leading_only: bool, caret: usize) -> (Vec<u8>, usize, bool) {
    let source = &text[..text.iter().position(|byte| *byte == 0).unwrap_or(text.len())];
    let byte = |at: usize| source.get(at).copied().unwrap_or(0);
    let mut result = Vec::new();
    let mut i = 0;
    let mut column = 0;
    let mut stop = width - 1;
    let mut non_space = false;
    let mut position = 0;
    let mut changed = false;
    while i < source.len() {
        let mut counter = 0;
        let mut next = false;
        if !non_space {
            while byte(i + counter) == b' ' {
                if column + counter == stop {
                    stop += width;
                    if counter >= 1 {
                        result.push(b'\t');
                        changed = true;
                        i += counter;
                        column += counter + 1;
                        counter = 0;
                        next = true;
                        if i <= caret { position += 1; }
                        break;
                    } else if matches!(byte(i + 1), b' ' | b'\t') {
                        result.push(b'\t');
                        changed = true;
                        i += 1;
                        column += 1;
                        counter = 0;
                        if i <= caret { position += 1; }
                    } else {
                        result.push(byte(i));
                        column += 1;
                        next = true;
                        if i <= caret { position += 1; }
                        break;
                    }
                } else { counter += 1; }
            }
            if next { i += 1; continue; }
            if byte(i) == b' ' && byte(i + counter) == b'\t' {
                result.push(b'\t');
                changed = true;
                i += counter;
                column = stop + 1;
                stop += width;
                if i <= caret { position += 1; }
                i += 1;
                continue;
            }
        }
        if leading_only && !non_space { non_space = true; }
        let c = byte(i);
        if c == 0 { break; }
        result.push(c);
        if matches!(c, b'\r' | b'\n') {
            column = 0;
            stop = width - 1;
            non_space = false;
        } else if c == b'\t' {
            column = stop + 1;
            stop += width;
        } else if c & 0xc0 != 0x80 {
            column += 1;
            if column > 0 && column % width == 0 { stop += width; }
        }
        if i < caret { position += 1; }
        i += 1;
    }
    (if changed { result } else { text.to_vec() }, position, changed)
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

fn convert_case(text: &str, case: Case, _seed: u64) -> String {
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
                simple_case(c, random_case_upper())
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
                        && previous.is_some_and(|p| crt_space(p) || matches!(p, '(' | '"'))
                        && next.is_some_and(|n| crt_space(n) || n == '\'')
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

fn random_case_upper() -> bool {
    std::thread_local! {
        static STATE: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
    }
    STATE.with(|state| {
        let next = state.get().wrapping_mul(214013).wrapping_add(2531011);
        state.set(next);
        (next >> 16) & 1 != 0
    })
}

fn win_alpha(c: char) -> bool {
    c.len_utf16() == 1 && c.is_alphabetic()
}

fn crt_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}
fn win_alphanumeric(c: char) -> bool {
    c.len_utf16() == 1 && c.is_alphanumeric()
}

struct SortRandom {
    state: [u32; 624],
    position: usize,
}

impl SortRandom {
    fn new(seed: u64) -> Self {
        let mut state = [0u32; 624];
        state[0] = seed as u32;
        for index in 1..state.len() {
            state[index] = 1812433253u32.wrapping_mul(state[index - 1] ^ (state[index - 1] >> 30))
                .wrapping_add(index as u32);
        }
        Self { state, position: 624 }
    }

    fn next(&mut self) -> u32 {
        if self.position == self.state.len() {
            for index in 0..self.state.len() {
                let value = (self.state[index] & 0x80000000)
                    | (self.state[(index + 1) % 624] & 0x7fffffff);
                self.state[index] = self.state[(index + 397) % 624] ^ (value >> 1)
                    ^ if value & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.position = 0;
        }
        let mut value = self.state[self.position];
        self.position += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c5680;
        value ^= (value << 15) & 0xefc60000;
        value ^= value >> 18;
        value
    }

    fn index(&mut self, bound: u64) -> u64 {
        let bits = if bound - 1 > u64::from(u32::MAX) { 64 } else { 32 };
        let mask = if bits == 64 { u64::MAX } else { u64::from(u32::MAX) };
        let threshold = mask.wrapping_sub(bound).wrapping_add(1) % bound;
        loop {
            let mut value = u64::from(self.next());
            if bits == 64 { value = (value << 32) | u64::from(self.next()); }
            let product = u128::from(value) * u128::from(bound);
            let remainder = (product & u128::from(mask)) as u64;
            if remainder >= threshold { return (product >> bits) as u64; }
        }
    }
}
