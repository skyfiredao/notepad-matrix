use std::ops::Range;

use crate::editor::ByteOffset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscapeErrorCode {
    Escape,
    BadBrace,
    Collate,
    Ctype,
    BadRepeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegexEscapeError {
    InvalidPosition(usize),
    Syntax {
        code: EscapeErrorCode,
        position: usize,
        message: &'static str,
    },
    Unsupported {
        position: usize,
        symbol: u16,
    },
}

pub type LiteralEscapeError = RegexEscapeError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatRange {
    Literal {
        end: usize,
    },
    Bounds {
        minimum: usize,
        maximum: Option<usize>,
        end: usize,
    },
}

pub fn decode_repeat_range(
    pattern: &[u16],
    open: usize,
    mut is_class: impl FnMut(u16, CharacterClass) -> bool,
) -> Result<RepeatRange, RegexEscapeError> {
    if pattern.get(open) != Some(&u16::from(b'{')) {
        return Err(RegexEscapeError::InvalidPosition(open));
    }
    let literal = RepeatRange::Literal { end: open + 1 };
    let mut at = open + 1;
    skip_repeat_space(pattern, &mut at, &mut is_class);
    let Some(minimum) = repeat_number(pattern, &mut at) else {
        return Ok(literal);
    };
    skip_repeat_space(pattern, &mut at, &mut is_class);
    let maximum = if pattern.get(at) == Some(&u16::from(b',')) {
        at += 1;
        skip_repeat_space(pattern, &mut at, &mut is_class);
        if at == pattern.len() {
            return Ok(literal);
        }
        repeat_number(pattern, &mut at)
    } else {
        Some(minimum)
    };
    skip_repeat_space(pattern, &mut at, &mut is_class);
    if pattern.get(at) != Some(&u16::from(b'}')) {
        return Ok(literal);
    }
    let end = at + 1;
    if maximum.is_some_and(|maximum| minimum > maximum) {
        let mut position = end - 2;
        while position > 0 && is_class(pattern[position], CharacterClass::Word) {
            position -= 1;
        }
        return Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::BadBrace,
            position: position + 1,
            message: "Invalid content of repeat range.",
        });
    }
    Ok(RepeatRange::Bounds {
        minimum,
        maximum,
        end,
    })
}

fn skip_repeat_space(
    pattern: &[u16],
    at: &mut usize,
    is_class: &mut impl FnMut(u16, CharacterClass) -> bool,
) {
    while pattern
        .get(*at)
        .is_some_and(|&unit| is_class(unit, CharacterClass::Space))
    {
        *at += 1;
    }
}

fn repeat_number(pattern: &[u16], at: &mut usize) -> Option<usize> {
    let start = *at;
    let mut result = 0u64;
    while let Some(&unit) = pattern.get(*at) {
        if !(u16::from(b'0')..=u16::from(b'9')).contains(&unit) {
            break;
        }
        result = result * 10 + u64::from(unit - u16::from(b'0'));
        *at += 1;
        if result > i64::MAX.unsigned_abs() / 10 {
            return None;
        }
    }
    if *at == start {
        None
    } else {
        usize::try_from(result).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegexMatchError {
    Complexity,
}

const MAX_MATCH_STATES: u64 = 100_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegexProgram {
    alternatives: Vec<Vec<RegexAtom>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RegexAtom {
    Literal(Vec<u16>),
    Any,
    Anchor(Anchor),
    Repeat(Repetition),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RepeatAtom {
    Literal(u16),
    Any,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Repetition {
    atom: RepeatAtom,
    minimum: usize,
    maximum: Option<usize>,
    greedy: bool,
    possessive: bool,
}

struct RepeatChoice {
    atom: RepeatAtom,
    next_atom: usize,
    current: usize,
    minimum: usize,
    maximum: usize,
    greedy: bool,
}

impl RepeatChoice {
    fn advance(&mut self, units: &[u16], dot_matches_newline: bool) -> Option<usize> {
        if self.greedy && self.current > self.minimum {
            self.current -= 1;
        } else if !self.greedy
            && self.current < self.maximum
            && self.atom.accepts(units[self.current], dot_matches_newline)
        {
            self.current += 1;
        } else {
            return None;
        }
        Some(self.current)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
    LineStart,
    LineEnd,
    BufferStart,
    BufferEnd,
    SoftBufferEnd,
}

impl RegexProgram {
    pub fn compile_utf8(pattern: &str) -> Result<Self, RegexEscapeError> {
        let prefix = pattern
            .split_once('\0')
            .map_or(pattern, |(prefix, _)| prefix);
        Self::compile_utf16(&prefix.encode_utf16().collect::<Vec<_>>())
    }

    pub fn compile_utf16(pattern: &[u16]) -> Result<Self, RegexEscapeError> {
        Self::compile_with_classification(pattern, None)
    }

    pub fn compile_utf16_with_classification(
        pattern: &[u16],
        mut is_class: impl FnMut(u16, CharacterClass) -> bool,
    ) -> Result<Self, RegexEscapeError> {
        Self::compile_with_classification(pattern, Some(&mut is_class))
    }

    fn compile_with_classification(
        pattern: &[u16],
        mut is_class: Option<&mut dyn FnMut(u16, CharacterClass) -> bool>,
    ) -> Result<Self, RegexEscapeError> {
        let mut atoms = Vec::new();
        let mut alternatives = Vec::new();
        let mut at = 0;
        while let Some(&symbol) = pattern.get(at) {
            if symbol == u16::from(b'\\') {
                let (atom, end) = decode_root_escape(pattern, at)?;
                match atom {
                    RootEscape::Literal(unit) => append_literal(&mut atoms, unit),
                    RootEscape::LiteralRun(units) => {
                        for unit in units {
                            append_literal(&mut atoms, unit);
                        }
                    }
                    RootEscape::Any => atoms.push(RegexAtom::Any),
                    RootEscape::Assertion(RegexAssertion::BufferStart) => {
                        atoms.push(RegexAtom::Anchor(Anchor::BufferStart))
                    }
                    RootEscape::Assertion(RegexAssertion::BufferEnd) => {
                        atoms.push(RegexAtom::Anchor(Anchor::BufferEnd))
                    }
                    RootEscape::Assertion(RegexAssertion::SoftBufferEnd) => {
                        atoms.push(RegexAtom::Anchor(Anchor::SoftBufferEnd))
                    }
                    _ => {
                        return Err(RegexEscapeError::Unsupported {
                            position: at,
                            symbol,
                        });
                    }
                }
                at = end;
            } else if symbol == u16::from(b'.') {
                atoms.push(RegexAtom::Any);
                at += 1;
            } else if symbol == u16::from(b'^') || symbol == u16::from(b'$') {
                atoms.push(RegexAtom::Anchor(if symbol == u16::from(b'^') {
                    Anchor::LineStart
                } else {
                    Anchor::LineEnd
                }));
                at += 1;
            } else if b"*+?".iter().any(|&syntax| symbol == u16::from(syntax)) {
                at = append_simple_repeat(&mut atoms, pattern, at)?;
            } else if symbol == u16::from(b'{') {
                let Some(classify) = is_class.as_mut() else {
                    return Err(RegexEscapeError::Unsupported {
                        position: at,
                        symbol,
                    });
                };
                match decode_repeat_range(pattern, at, classify)? {
                    RepeatRange::Literal { end } => {
                        append_literal(&mut atoms, symbol);
                        at = end;
                    }
                    RepeatRange::Bounds {
                        minimum,
                        maximum,
                        end,
                    } => {
                        at = append_repeat(&mut atoms, pattern, end, minimum, maximum)?;
                    }
                }
            } else if let Some(end) = skip_inline_comment(pattern, at) {
                at = end;
            } else if symbol == u16::from(b'|') {
                alternatives.push(std::mem::take(&mut atoms));
                at += 1;
            } else if b"()[".iter().any(|&syntax| symbol == u16::from(syntax)) {
                return Err(RegexEscapeError::Unsupported {
                    position: at,
                    symbol,
                });
            } else {
                append_literal(&mut atoms, symbol);
                at += 1;
            }
        }
        alternatives.push(atoms);
        Ok(Self { alternatives })
    }

    pub fn match_at(
        &self,
        document: &Utf16View,
        unit_start: usize,
    ) -> Result<Option<Range<usize>>, RegexMatchError> {
        self.match_at_with_dot_newline(document, unit_start, false)
    }

    pub fn match_at_with_dot_newline(
        &self,
        document: &Utf16View,
        unit_start: usize,
        dot_matches_newline: bool,
    ) -> Result<Option<Range<usize>>, RegexMatchError> {
        self.match_at_with_state_limit(document, unit_start, dot_matches_newline, MAX_MATCH_STATES)
    }

    pub fn match_at_with_state_limit(
        &self,
        document: &Utf16View,
        unit_start: usize,
        dot_matches_newline: bool,
        max_states: u64,
    ) -> Result<Option<Range<usize>>, RegexMatchError> {
        self.match_at_in_scope(
            document,
            unit_start..document.units().len(),
            dot_matches_newline,
            max_states,
        )
    }

    pub fn match_at_in_scope(
        &self,
        document: &Utf16View,
        scope: Range<usize>,
        dot_matches_newline: bool,
        max_states: u64,
    ) -> Result<Option<Range<usize>>, RegexMatchError> {
        if scope.start > scope.end {
            return Ok(None);
        }
        let Some(units) = document.units().get(..scope.end) else {
            return Ok(None);
        };
        let unit_start = scope.start;
        let mut state_count = 0u64;
        for atoms in &self.alternatives {
            if let Some(found) = Self::match_branch(
                atoms,
                units,
                unit_start,
                dot_matches_newline,
                max_states,
                &mut state_count,
            )? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    fn match_branch(
        atoms: &[RegexAtom],
        units: &[u16],
        unit_start: usize,
        dot_matches_newline: bool,
        max_states: u64,
        state_count: &mut u64,
    ) -> Result<Option<Range<usize>>, RegexMatchError> {
        let mut at = unit_start;
        let mut index = 0;
        let mut choices = Vec::<RepeatChoice>::new();
        loop {
            *state_count = state_count.saturating_add(1);
            let Some(atom) = atoms.get(index) else {
                return Ok(Some(unit_start..at));
            };
            let next = match atom {
                RegexAtom::Literal(literal) => {
                    units[at..].starts_with(literal).then(|| at + literal.len())
                }
                RegexAtom::Any => units
                    .get(at)
                    .filter(|&&unit| dot_matches_newline || !is_line_separator(unit))
                    .map(|_| at + 1),
                RegexAtom::Anchor(anchor) => anchor.matches(units, at).then_some(at),
                RegexAtom::Repeat(repeat) => {
                    let maximum = repeat
                        .maximum
                        .unwrap_or(units.len() - at)
                        .min(units.len() - at);
                    let mut count = 0;
                    let target = if repeat.greedy {
                        maximum
                    } else {
                        repeat.minimum.min(maximum)
                    };
                    while count < target
                        && repeat.atom.accepts(units[at + count], dot_matches_newline)
                    {
                        count += 1;
                    }
                    if count < repeat.minimum {
                        None
                    } else {
                        let minimum = at + repeat.minimum;
                        let maximum = at + if repeat.greedy { count } else { maximum };
                        let current = if repeat.greedy { maximum } else { minimum };
                        if !repeat.possessive && minimum != maximum {
                            choices.push(RepeatChoice {
                                atom: repeat.atom,
                                next_atom: index + 1,
                                current,
                                minimum,
                                maximum,
                                greedy: repeat.greedy,
                            });
                        }
                        Some(current)
                    }
                }
            };
            if let Some(next) = next {
                at = next;
                index += 1;
                continue;
            }
            if *state_count > max_states {
                return Err(RegexMatchError::Complexity);
            }
            loop {
                let Some(mut choice) = choices.pop() else {
                    return Ok(None);
                };
                *state_count = state_count.saturating_add(1);
                if let Some(next) = choice.advance(units, dot_matches_newline) {
                    at = next;
                    index = choice.next_atom;
                    choices.push(choice);
                    break;
                }
            }
        }
    }
}

impl RepeatAtom {
    fn accepts(self, unit: u16, dot_matches_newline: bool) -> bool {
        match self {
            Self::Literal(literal) => literal == unit,
            Self::Any => dot_matches_newline || !is_line_separator(unit),
        }
    }
}

fn append_literal(atoms: &mut Vec<RegexAtom>, unit: u16) {
    if let Some(RegexAtom::Literal(run)) = atoms.last_mut() {
        run.push(unit);
    } else {
        atoms.push(RegexAtom::Literal(vec![unit]));
    }
}

fn append_simple_repeat(
    atoms: &mut Vec<RegexAtom>,
    pattern: &[u16],
    at: usize,
) -> Result<usize, RegexEscapeError> {
    let symbol = pattern[at];
    if at == 0 {
        let message = match symbol {
            42 => "The repeat operator \"*\" cannot start a regular expression.",
            43 => "The repeat operator \"+\" cannot start a regular expression.",
            _ => "The repeat operator \"?\" cannot start a regular expression.",
        };
        return Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::BadRepeat,
            position: 0,
            message,
        });
    }
    append_repeat(
        atoms,
        pattern,
        at + 1,
        usize::from(symbol == u16::from(b'+')),
        (symbol == u16::from(b'?')).then_some(1),
    )
}

fn append_repeat(
    atoms: &mut Vec<RegexAtom>,
    pattern: &[u16],
    mut end: usize,
    minimum: usize,
    maximum: Option<usize>,
) -> Result<usize, RegexEscapeError> {
    let greedy = pattern.get(end) != Some(&u16::from(b'?'));
    if !greedy {
        end += 1;
    }
    let possessive = pattern.get(end) == Some(&u16::from(b'+'));
    if possessive {
        end += 1;
    }
    let bad_repeat = |position| RegexEscapeError::Syntax {
        code: EscapeErrorCode::BadRepeat,
        position,
        message: "Invalid preceding regular expression prior to repetition operator.",
    };
    let atom = match atoms.last_mut() {
        Some(RegexAtom::Literal(run)) => {
            let Some(unit) = run.pop() else {
                return Err(bad_repeat(end));
            };
            if run.is_empty() {
                atoms.pop();
            }
            RepeatAtom::Literal(unit)
        }
        Some(RegexAtom::Any) => {
            atoms.pop();
            RepeatAtom::Any
        }
        _ => return Err(bad_repeat(end)),
    };
    if possessive {
        while let Some(next) = skip_inline_comment(pattern, end) {
            end = next;
        }
        if pattern
            .get(end)
            .is_some_and(|&unit| b"*+?{".iter().any(|&byte| unit == u16::from(byte)))
        {
            return Err(bad_repeat(end));
        }
    }
    atoms.push(RegexAtom::Repeat(Repetition {
        atom,
        minimum,
        maximum,
        greedy,
        possessive,
    }));
    Ok(end)
}

fn skip_inline_comment(pattern: &[u16], at: usize) -> Option<usize> {
    let tail = pattern.get(at..)?;
    if !tail.starts_with(&[u16::from(b'('), u16::from(b'?'), u16::from(b'#')]) {
        return None;
    }
    Some(
        tail[3..]
            .iter()
            .position(|&unit| unit == u16::from(b')'))
            .map_or(pattern.len(), |relative| at + 3 + relative + 1),
    )
}

impl Anchor {
    fn matches(self, units: &[u16], at: usize) -> bool {
        let previous = at
            .checked_sub(1)
            .and_then(|index| units.get(index))
            .copied();
        let current = units.get(at).copied();
        let inside_crlf = previous == Some(0x0d) && current == Some(0x0a);
        match self {
            Self::LineStart => at == 0 || (previous.is_some_and(is_line_separator) && !inside_crlf),
            Self::LineEnd => {
                at == units.len() || (current.is_some_and(is_line_separator) && !inside_crlf)
            }
            Self::BufferStart => at == 0,
            Self::BufferEnd => at == units.len(),
            Self::SoftBufferEnd => units[at..].iter().copied().all(is_line_separator),
        }
    }
}

fn is_line_separator(unit: u16) -> bool {
    matches!(unit, 0x0a | 0x0d | 0x0c | 0x85 | 0x2028 | 0x2029)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterClass {
    Alnum,
    Alpha,
    Blank,
    Control,
    Digit,
    Graph,
    Print,
    Punct,
    Space,
    Unicode,
    Word,
    HexDigit,
    Horizontal,
    Vertical,
    Lower,
    Upper,
}

const CLASS_NAMES: [(&str, CharacterClass); 21] = [
    ("alnum", CharacterClass::Alnum),
    ("alpha", CharacterClass::Alpha),
    ("blank", CharacterClass::Blank),
    ("cntrl", CharacterClass::Control),
    ("d", CharacterClass::Digit),
    ("digit", CharacterClass::Digit),
    ("graph", CharacterClass::Graph),
    ("h", CharacterClass::Horizontal),
    ("l", CharacterClass::Lower),
    ("lower", CharacterClass::Lower),
    ("print", CharacterClass::Print),
    ("punct", CharacterClass::Punct),
    ("s", CharacterClass::Space),
    ("space", CharacterClass::Space),
    ("u", CharacterClass::Upper),
    ("unicode", CharacterClass::Unicode),
    ("upper", CharacterClass::Upper),
    ("v", CharacterClass::Vertical),
    ("w", CharacterClass::Word),
    ("word", CharacterClass::Word),
    ("xdigit", CharacterClass::HexDigit),
];

pub fn lookup_default_character_class(name: &[u16]) -> Option<CharacterClass> {
    CLASS_NAMES.iter().find_map(|(alias, class)| {
        name.iter()
            .copied()
            .eq(alias.bytes().map(u16::from))
            .then_some(*class)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegexAssertion {
    WordBoundary,
    WithinWord,
    WordStart,
    WordEnd,
    BufferStart,
    BufferEnd,
    SoftBufferEnd,
    RestartContinuation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootEscape {
    Literal(u16),
    LiteralRun(Vec<u16>),
    Class {
        class: CharacterClass,
        negative: bool,
    },
    Assertion(RegexAssertion),
    Any,
    ResetMatchStart,
    Combining,
    LineEnding,
}

fn shorthand_class(symbol: u8) -> Option<CharacterClass> {
    Some(match symbol.to_ascii_lowercase() {
        b'd' => CharacterClass::Digit,
        b's' => CharacterClass::Space,
        b'w' => CharacterClass::Word,
        b'h' => CharacterClass::Horizontal,
        b'v' => CharacterClass::Vertical,
        b'l' => CharacterClass::Lower,
        b'u' => CharacterClass::Upper,
        _ => return None,
    })
}

pub fn decode_root_escape(
    pattern: &[u16],
    slash: usize,
) -> Result<(RootEscape, usize), RegexEscapeError> {
    if pattern.get(slash) != Some(&u16::from(b'\\')) {
        return Err(RegexEscapeError::InvalidPosition(slash));
    }
    let at = slash + 1;
    let symbol = pattern.get(at).copied().ok_or(RegexEscapeError::Syntax {
        code: EscapeErrorCode::Escape,
        position: at,
        message: "Incomplete escape sequence found.",
    })?;
    let ascii = u8::try_from(symbol);
    if let Ok(ascii) = ascii
        && let Some(class) = shorthand_class(ascii)
    {
        return Ok((
            RootEscape::Class {
                class,
                negative: ascii.is_ascii_uppercase(),
            },
            at + 1,
        ));
    }
    let atom = match ascii {
        Ok(b'b') => RootEscape::Assertion(RegexAssertion::WordBoundary),
        Ok(b'B') => RootEscape::Assertion(RegexAssertion::WithinWord),
        Ok(b'<') => RootEscape::Assertion(RegexAssertion::WordStart),
        Ok(b'>') => RootEscape::Assertion(RegexAssertion::WordEnd),
        Ok(b'A' | b'`') => RootEscape::Assertion(RegexAssertion::BufferStart),
        Ok(b'z' | b'\'') => RootEscape::Assertion(RegexAssertion::BufferEnd),
        Ok(b'Z') => RootEscape::Assertion(RegexAssertion::SoftBufferEnd),
        Ok(b'G') => RootEscape::Assertion(RegexAssertion::RestartContinuation),
        Ok(b'C') => RootEscape::Any,
        Ok(b'K') => RootEscape::ResetMatchStart,
        Ok(b'X') => RootEscape::Combining,
        Ok(b'R') => RootEscape::LineEnding,
        Ok(b'Q') => return decode_quoted_literals(pattern, slash),
        Ok(b'1'..=b'9' | b'g' | b'k' | b'p' | b'P') => {
            return Err(RegexEscapeError::Unsupported {
                position: slash,
                symbol,
            });
        }
        _ => {
            let (literal, end) = decode_literal_escape(pattern, slash)?;
            return Ok((RootEscape::Literal(literal), end));
        }
    };
    Ok((atom, at + 1))
}

fn decode_quoted_literals(
    pattern: &[u16],
    slash: usize,
) -> Result<(RootEscape, usize), RegexEscapeError> {
    let start = slash + 2;
    let mut at = start;
    loop {
        let Some(relative) = pattern[at..]
            .iter()
            .position(|&unit| unit == u16::from(b'\\'))
        else {
            return Ok((
                RootEscape::LiteralRun(pattern[start..].to_vec()),
                pattern.len(),
            ));
        };
        let escape = at + relative;
        let after = escape + 1;
        let Some(&symbol) = pattern.get(after) else {
            return Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::Escape,
                position: after,
                message: "Unterminated \\Q...\\E sequence.",
            });
        };
        if symbol == u16::from(b'E') {
            return Ok((
                RootEscape::LiteralRun(pattern[start..escape].to_vec()),
                after + 1,
            ));
        }
        at = after;
    }
}

pub fn decode_root_escape_with_class_lookup(
    pattern: &[u16],
    slash: usize,
    lookup_class: impl FnOnce(&[u16]) -> Option<CharacterClass>,
) -> Result<(RootEscape, usize), RegexEscapeError> {
    if pattern.get(slash) == Some(&u16::from(b'\\'))
        && pattern
            .get(slash + 1)
            .is_some_and(|&unit| unit == u16::from(b'p') || unit == u16::from(b'P'))
    {
        decode_property_escape(pattern, slash, lookup_class)
    } else {
        decode_root_escape(pattern, slash)
    }
}

pub fn decode_property_escape(
    pattern: &[u16],
    slash: usize,
    lookup_class: impl FnOnce(&[u16]) -> Option<CharacterClass>,
) -> Result<(RootEscape, usize), RegexEscapeError> {
    if pattern.get(slash) != Some(&u16::from(b'\\')) {
        return Err(RegexEscapeError::InvalidPosition(slash));
    }
    let symbol = pattern.get(slash + 1).copied();
    if !symbol.is_some_and(|unit| unit == u16::from(b'p') || unit == u16::from(b'P')) {
        return Err(RegexEscapeError::InvalidPosition(slash));
    }
    let at = slash + 2;
    let Some(&next) = pattern.get(at) else {
        return Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::Escape,
            position: at,
            message: "Incomplete property escape found.",
        });
    };
    let (name, end) = if next == u16::from(b'{') {
        let Some(relative_end) = pattern[at..]
            .iter()
            .position(|&unit| unit == u16::from(b'}'))
        else {
            return Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::Escape,
                position: pattern.len(),
                message: "Closing } missing from property escape sequence.",
            });
        };
        let close = at + relative_end;
        (&pattern[at + 1..close], close + 1)
    } else {
        (&pattern[at..at + 1], at + 1)
    };
    match lookup_class(name) {
        Some(class) => Ok((
            RootEscape::Class {
                class,
                negative: symbol == Some(u16::from(b'P')),
            },
            end,
        )),
        None => Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::Ctype,
            position: end,
            message: "Escape sequence was neither a valid property nor a valid character class name.",
        }),
    }
}

pub fn decode_literal_escape(
    pattern: &[u16],
    slash: usize,
) -> Result<(u16, usize), LiteralEscapeError> {
    if pattern.get(slash) != Some(&u16::from(b'\\')) {
        return Err(LiteralEscapeError::InvalidPosition(slash));
    }
    let at = slash + 1;
    let symbol = pattern.get(at).copied().ok_or(LiteralEscapeError::Syntax {
        code: EscapeErrorCode::Escape,
        position: at,
        message: "Escape sequence terminated prematurely.",
    })?;
    match u8::try_from(symbol) {
        Ok(b'c') => {
            let control = pattern
                .get(at + 1)
                .copied()
                .ok_or(LiteralEscapeError::Syntax {
                    code: EscapeErrorCode::Escape,
                    position: slash,
                    message: "ASCII escape sequence terminated prematurely.",
                })?;
            Ok((control % 32, at + 2))
        }
        Ok(b'x') => decode_hex_escape(pattern, slash),
        Ok(b'0'..=b'9') => {
            if symbol != u16::from(b'0') {
                return Err(LiteralEscapeError::Syntax {
                    code: EscapeErrorCode::Escape,
                    position: slash,
                    message: "Invalid octal escape sequence.",
                });
            }
            if let Some((value, end)) = digits(pattern, at, 4, 8)
                && let Ok(unit) = u16::try_from(value)
            {
                return Ok((unit, end));
            }
            Err(LiteralEscapeError::Syntax {
                code: EscapeErrorCode::Escape,
                position: slash,
                message: "Octal escape sequence is invalid.",
            })
        }
        Ok(b'N') => decode_named_escape(pattern, slash),
        ascii => {
            let controls = [
                (b'a', 7),
                (b'e', 27),
                (b'f', 12),
                (b'n', 10),
                (b'r', 13),
                (b't', 9),
                (b'v', 11),
                (b'b', 8),
            ];
            let decoded = controls
                .iter()
                .find(|(escape, _)| ascii.as_ref().is_ok_and(|value| value == escape))
                .map_or(symbol, |(_, value)| *value);
            Ok((decoded, at + 1))
        }
    }
}

fn digits(pattern: &[u16], at: usize, maximum: usize, radix: u32) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut end = at;
    for (offset, &unit) in pattern.get(at..)?.iter().take(maximum).enumerate() {
        let digit = match u8::try_from(unit) {
            Ok(digit @ b'0'..=b'9') => u32::from(digit - b'0'),
            Ok(digit @ b'a'..=b'f') => u32::from(digit - b'a') + 10,
            Ok(digit @ b'A'..=b'F') => u32::from(digit - b'A') + 10,
            _ => break,
        };
        if digit >= radix {
            break;
        }
        value = value
            .checked_mul(u64::from(radix))?
            .checked_add(u64::from(digit))?;
        end = at + offset + 1;
    }
    (end != at).then_some((value, end))
}

fn decode_hex_escape(pattern: &[u16], slash: usize) -> Result<(u16, usize), LiteralEscapeError> {
    let at = slash + 2;
    let Some(&next) = pattern.get(at) else {
        return Err(LiteralEscapeError::Syntax {
            code: EscapeErrorCode::Escape,
            position: slash,
            message: "Hexadecimal escape sequence terminated prematurely.",
        });
    };
    if next == u16::from(b'{') {
        if at + 1 == pattern.len() {
            return Err(LiteralEscapeError::Syntax {
                code: EscapeErrorCode::Escape,
                position: slash,
                message: "Missing } in hexadecimal escape sequence.",
            });
        }
        if let Some((value, end)) = digits(pattern, at + 1, pattern.len(), 16)
            && pattern.get(end) == Some(&u16::from(b'}'))
            && let Ok(unit) = u16::try_from(value)
        {
            return Ok((unit, end + 1));
        }
        Err(LiteralEscapeError::Syntax {
            code: EscapeErrorCode::BadBrace,
            position: slash,
            message: "Hexadecimal escape sequence was invalid.",
        })
    } else {
        if let Some((value, end)) = digits(pattern, at, 2, 16)
            && let Ok(unit) = u16::try_from(value)
        {
            return Ok((unit, end));
        }
        Err(LiteralEscapeError::Syntax {
            code: EscapeErrorCode::Escape,
            position: slash,
            message: "Escape sequence did not encode a valid character.",
        })
    }
}

const COLLATING_NAMES: [&str; 128] = [
    "NUL",
    "SOH",
    "STX",
    "ETX",
    "EOT",
    "ENQ",
    "ACK",
    "alert",
    "backspace",
    "tab",
    "newline",
    "vertical-tab",
    "form-feed",
    "carriage-return",
    "SO",
    "SI",
    "DLE",
    "DC1",
    "DC2",
    "DC3",
    "DC4",
    "NAK",
    "SYN",
    "ETB",
    "CAN",
    "EM",
    "SUB",
    "ESC",
    "IS4",
    "IS3",
    "IS2",
    "IS1",
    "space",
    "exclamation-mark",
    "quotation-mark",
    "number-sign",
    "dollar-sign",
    "percent-sign",
    "ampersand",
    "apostrophe",
    "left-parenthesis",
    "right-parenthesis",
    "asterisk",
    "plus-sign",
    "comma",
    "hyphen",
    "period",
    "slash",
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "colon",
    "semicolon",
    "less-than-sign",
    "equals-sign",
    "greater-than-sign",
    "question-mark",
    "commercial-at",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "left-square-bracket",
    "backslash",
    "right-square-bracket",
    "circumflex",
    "underscore",
    "grave-accent",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "left-curly-bracket",
    "vertical-line",
    "right-curly-bracket",
    "tilde",
    "DEL",
];

const MULTI_COLLATING_NAMES: [&str; 21] = [
    "ae", "Ae", "AE", "ch", "Ch", "CH", "ll", "Ll", "LL", "ss", "Ss", "SS", "nj", "Nj", "NJ", "dz",
    "Dz", "DZ", "lj", "Lj", "LJ",
];

fn lookup_collation_name(name: &[u16]) -> Option<Vec<u16>> {
    let narrowed: Vec<_> = name.iter().map(|unit| unit.to_le_bytes()[0]).collect();
    if let Some(index) = COLLATING_NAMES
        .iter()
        .position(|name| name.as_bytes() == narrowed)
        && let Ok(unit) = u16::try_from(index)
    {
        return Some(vec![unit]);
    }
    if let Some(name) = MULTI_COLLATING_NAMES
        .iter()
        .find(|name| name.as_bytes() == narrowed)
    {
        return Some(name.encode_utf16().collect());
    }
    if name.len() == 1 {
        Some(vec![name[0]])
    } else {
        None
    }
}

fn named_escape_error(
    pattern: &[u16],
    slash: usize,
    before: usize,
    code: EscapeErrorCode,
) -> LiteralEscapeError {
    let position = pattern[..before]
        .iter()
        .rposition(|&unit| unit == u16::from(b'\\'))
        .unwrap_or(slash);
    LiteralEscapeError::Syntax {
        code,
        position,
        message: if code == EscapeErrorCode::Collate {
            "Invalid collation character."
        } else {
            "Invalid or unterminated escape sequence."
        },
    }
}

fn decode_named_escape(pattern: &[u16], slash: usize) -> Result<(u16, usize), LiteralEscapeError> {
    let at = slash + 2;
    if pattern.get(at) != Some(&u16::from(b'{')) {
        return Err(named_escape_error(
            pattern,
            slash,
            at,
            EscapeErrorCode::Escape,
        ));
    }
    let start = at + 1;
    let Some(relative_end) = pattern[start..]
        .iter()
        .position(|&unit| unit == u16::from(b'}'))
    else {
        return Err(named_escape_error(
            pattern,
            slash,
            pattern.len(),
            EscapeErrorCode::Escape,
        ));
    };
    let end = start + relative_end;
    match lookup_collation_name(&pattern[start..end]) {
        Some(units) if units.len() == 1 => Ok((units[0], end + 1)),
        Some(_) => Err(named_escape_error(
            pattern,
            slash,
            end + 1,
            EscapeErrorCode::Escape,
        )),
        None => Err(named_escape_error(
            pattern,
            slash,
            end + 1,
            EscapeErrorCode::Collate,
        )),
    }
}

pub struct Utf16View {
    units: Vec<u16>,
    positions: Vec<ByteOffset>,
}

impl Utf16View {
    pub fn new(text: &str) -> Self {
        let mut units = Vec::new();
        let mut positions = Vec::new();
        for (start, ch) in text.char_indices() {
            let mut encoded = [0; 2];
            for &unit in ch.encode_utf16(&mut encoded).iter() {
                units.push(unit);
                positions.push(ByteOffset(start));
            }
        }
        positions.push(ByteOffset(text.len()));
        Self { units, positions }
    }

    pub fn units(&self) -> &[u16] {
        &self.units
    }

    pub fn byte_position(&self, unit: usize) -> Option<ByteOffset> {
        self.positions.get(unit).copied()
    }

    pub fn byte_range(&self, units: Range<usize>) -> Option<Range<ByteOffset>> {
        if units.start > units.end {
            return None;
        }
        Some(self.byte_position(units.start)?..self.byte_position(units.end)?)
    }

    pub fn unit_at_or_after_byte(&self, byte: ByteOffset) -> Option<usize> {
        let index = self.positions.partition_point(|position| *position < byte);
        (index < self.positions.len()).then_some(index)
    }
}
