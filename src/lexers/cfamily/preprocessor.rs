use std::collections::BTreeMap;

use crate::languages::LanguageDefinition;
use crate::lexers::shared::{line_ranges, property_bool};
use crate::lexers::{LexResult, Span};

#[derive(Clone)]
struct Symbol {
    value: String,
    arguments: String,
}

struct Sections {
    state: u32,
    taken: u32,
    level: i32,
}

impl Sections {
    fn valid(&self) -> bool {
        (0..31).contains(&self.level)
    }
    fn active(&self) -> bool {
        self.state == 0
    }
    fn start(&mut self, on: bool) {
        self.level += 1;
        if self.valid() {
            let mask = 1u32 << self.level;
            if on {
                self.state &= !mask;
                self.taken |= mask;
            } else {
                self.state |= mask;
                self.taken &= !mask;
            }
        }
    }
    fn end(&mut self) {
        if self.valid() {
            let mask = 1u32 << self.level;
            self.state &= !mask;
            self.taken &= !mask;
        }
        self.level -= 1;
    }
    fn invert(&mut self) {
        if self.valid() {
            let mask = 1u32 << self.level;
            self.state ^= mask;
            self.taken |= mask;
        }
    }
    fn current_taken(&self) -> bool {
        self.valid() && self.taken & (1u32 << self.level) != 0
    }
}

fn parse_define(definition: &str, separators: &[char]) -> (String, Symbol) {
    let definition = definition.trim_start_matches([' ', '\t']);
    if let Some(after_name) = definition.find(separators) {
        let name = definition[..after_name].to_owned();
        let remainder = &definition[after_name + 1..];
        if definition.as_bytes()[after_name] == b'(' {
            if let Some(close) = remainder.find(')') {
                let arguments = remainder[..close].to_owned();
                let mut value = &remainder[close + 1..];
                if value
                    .chars()
                    .next()
                    .is_some_and(|ch| separators.contains(&ch))
                {
                    value = &value[1..];
                }
                (
                    name,
                    Symbol {
                        value: value.to_owned(),
                        arguments,
                    },
                )
            } else {
                (
                    name,
                    Symbol {
                        value: String::new(),
                        arguments: String::new(),
                    },
                )
            }
        } else {
            (
                name,
                Symbol {
                    value: remainder.to_owned(),
                    arguments: String::new(),
                },
            )
        }
    } else {
        (
            definition.to_owned(),
            Symbol {
                value: "1".into(),
                arguments: String::new(),
            },
        )
    }
}

fn rest_of_line(text: &str, mut position: usize, allow_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    while position < bytes.len() && !matches!(bytes[position], b'\r' | b'\n') {
        let ch = bytes[position];
        if ch == b'\\'
            && bytes
                .get(position + 1)
                .is_some_and(|ch| matches!(ch, b'\r' | b'\n'))
        {
            position += 2;
            if bytes.get(position - 1) == Some(&b'\r') && bytes.get(position) == Some(&b'\n') {
                position += 1;
            }
        } else {
            if ch == b'/'
                && bytes
                    .get(position + 1)
                    .is_some_and(|next| matches!(next, b'/' | b'*'))
            {
                break;
            }
            if allow_space || ch != b' ' {
                result.push(ch);
            }
            position += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

pub(super) fn apply(text: &str, p: &LanguageDefinition, result: &mut LexResult) {
    if !property_bool(p, "lexer.cpp.track.preprocessor", true) {
        return;
    }
    let mut symbols: BTreeMap<String, Symbol> = p
        .keywords(4)
        .split_ascii_whitespace()
        .map(|definition| parse_define(definition, &['(', '=']))
        .collect();
    let mut sections = Sections {
        state: 0,
        taken: 0,
        level: -1,
    };
    let mut styles = vec![0u16; text.len()];
    for span in &result.spans {
        styles[span.range.clone()].fill(span.style);
    }
    let mut changed = false;
    let mut continued = false;
    let mut continuation_active = true;
    for (line, range) in line_ranges(text).into_iter().enumerate() {
        let entering = if continued {
            continuation_active
        } else {
            sections.active()
        };
        let mut directive_active = entering;
        let mut body_active = entering;
        let mut hash = None;
        if !continued
            && let Some(start) = range
                .clone()
                .find(|position| text.as_bytes()[*position] == b'#' && styles[*position] == 9)
        {
            hash = Some(start);
            let offset = start
                + 1
                + text[start + 1..]
                    .bytes()
                    .take_while(|ch| matches!(ch, b' ' | b'\t'))
                    .count();
            let tail = &text[offset..];
            if tail.starts_with("include") {
            } else if tail.starts_with("ifdef") || tail.starts_with("ifndef") {
                let positive = tail.starts_with("ifdef");
                let skip = if positive { 6 } else { 7 };
                let name = rest_of_line(text, (offset + skip).min(text.len()), false);
                sections.start(positive == symbols.contains_key(&name));
            } else if tail.starts_with("if") {
                sections.start(evaluate(&rest_of_line(text, offset + 2, true), &symbols, p));
            } else if tail.starts_with("else") && sections.valid() {
                if !sections.current_taken() || sections.active() {
                    sections.invert();
                }
                directive_active = entering || sections.active();
                body_active = sections.active();
            } else if tail.starts_with("elif") && sections.valid() {
                if !sections.current_taken() {
                    if evaluate(&rest_of_line(text, offset + 4, true), &symbols, p) {
                        sections.invert();
                    }
                } else if sections.active() {
                    sections.invert();
                }
                directive_active = entering || sections.active();
                body_active = sections.active();
            } else if tail.starts_with("endif") {
                sections.end();
                directive_active = sections.active();
                body_active = sections.active();
            } else if property_bool(p, "lexer.cpp.update.preprocessor", true) && sections.active() {
                if tail.starts_with("define") {
                    let (name, symbol) =
                        parse_define(&rest_of_line(text, offset + 6, true), &['(', ' ', '\t']);
                    symbols.insert(name, symbol);
                    changed = true;
                } else if tail.starts_with("undef")
                    && let Some(name) = tokenize(&rest_of_line(text, offset + 5, false), p).first()
                {
                    symbols.remove(name);
                    changed = true;
                }
            }
        }
        let mut after_comment = false;
        for position in range.clone() {
            let base = styles[position];
            let active = if let Some(hash) = hash {
                if position < hash {
                    entering
                } else if base == 9 && !after_comment {
                    directive_active
                } else {
                    body_active
                }
            } else {
                entering
            };
            if matches!(base, 23 | 24 | 2 | 15) {
                after_comment = true;
            }
            if !active {
                styles[position] |= 0x40;
            }
        }
        let content = text[range.clone()].trim_end_matches(['\r', '\n']);
        continued = content.ends_with('\\')
            && (continued || hash.is_some())
            && !property_bool(p, "lexer.cpp.continuation.only.in.strings", false);
        continuation_active = if after_comment {
            body_active
        } else {
            directive_active
        };
        if changed || sections.level != -1 {
            let mut state = (u64::from(sections.state) << 1)
                ^ (u64::from(sections.taken) << 32)
                ^ u64::from(sections.level.wrapping_add(2) as u32);
            for (name, symbol) in &symbols {
                for byte in name
                    .bytes()
                    .chain(symbol.value.bytes())
                    .chain(symbol.arguments.bytes())
                {
                    state = state.wrapping_mul(131).wrapping_add(u64::from(byte));
                }
            }
            if let Some(line) = result.line_states.get_mut(line) {
                line.state |= state | (1 << 62);
            }
        }
    }
    result.spans.clear();
    for (position, ch) in text.char_indices() {
        let style = styles[position];
        if let Some(last) = result.spans.last_mut().filter(|last| last.style == style) {
            last.range.end = position + ch.len_utf8();
        } else {
            result.spans.push(Span {
                range: position..position + ch.len_utf8(),
                style,
            });
        }
    }
}

fn word(ch: char, p: &LanguageDefinition) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(ch, '.' | '_')
        || !ch.is_ascii()
        || (ch == '$' && property_bool(p, "lexer.cpp.allow.dollars", true))
        || (ch == '#' && property_bool(p, "lexer.cpp.allow.hashes", false))
}

fn identifier(token: &str, p: &LanguageDefinition) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|ch| word(ch, p) && !ch.is_ascii_digit() && ch != '.')
}

fn tokenize(expression: &str, p: &LanguageDefinition) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = expression.chars().peekable();
    while let Some(ch) = chars.next() {
        let mut token = ch.to_string();
        if word(ch, p) {
            while chars.peek().is_some_and(|ch| word(*ch, p)) {
                if let Some(ch) = chars.next() {
                    token.push(ch);
                }
            }
        } else if matches!(ch, ' ' | '\t') {
            while chars.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
                if let Some(ch) = chars.next() {
                    token.push(ch);
                }
            }
        } else if "=!<>".contains(ch) {
            if chars.peek().is_some_and(|ch| "=!<>".contains(*ch))
                && let Some(ch) = chars.next()
            {
                token.push(ch);
            }
        } else if "|&".contains(ch)
            && chars.peek().is_some_and(|ch| "|&".contains(*ch))
            && let Some(ch) = chars.next()
        {
            token.push(ch);
        }
        tokens.push(token);
    }
    tokens
}

fn strip_space(tokens: &mut Vec<String>) {
    tokens.retain(|token| !token.chars().all(|ch| matches!(ch, ' ' | '\t')));
}

fn expand(
    mut tokens: Vec<String>,
    symbols: &BTreeMap<String, Symbol>,
    p: &LanguageDefinition,
) -> Vec<String> {
    strip_space(&mut tokens);
    let mut index = 0;
    while index + 1 < tokens.len() {
        if tokens[index] != "defined" {
            index += 1;
            continue;
        }
        let mut value = false;
        let count = if tokens[index + 1] == "(" {
            if tokens.get(index + 2).is_some_and(|token| token == ")") {
                2
            } else if tokens.get(index + 3).is_some_and(|token| token == ")") {
                value = symbols.contains_key(&tokens[index + 2]);
                3
            } else {
                1
            }
        } else {
            value = symbols.contains_key(&tokens[index + 1]);
            1
        };
        tokens.drain(index + 1..index + 1 + count);
        tokens[index] = if value { "1".into() } else { "0".into() };
    }
    let mut index = 0;
    for _ in 0..100 {
        if index >= tokens.len() {
            break;
        }
        if !identifier(&tokens[index], p) {
            index += 1;
            continue;
        }
        let Some(symbol) = symbols.get(&tokens[index]) else {
            tokens[index] = "0".into();
            continue;
        };
        let mut value = tokenize(&symbol.value, p);
        if symbol.arguments.is_empty() {
            tokens.splice(index..=index, value);
        } else if tokens.get(index + 1).is_some_and(|token| token == "(") {
            let names: Vec<_> = symbol.arguments.split(',').collect();
            let mut arguments = BTreeMap::new();
            let mut argument = 0;
            let mut end = index + 2;
            while end < tokens.len() && argument < names.len() && tokens[end] != ")" {
                if tokens[end] != "," {
                    arguments.insert(names[argument], tokens[end].clone());
                    argument += 1;
                }
                end += 1;
            }
            if end >= tokens.len() {
                tokens.truncate(index);
                continue;
            }
            tokens.drain(index..=end);
            strip_space(&mut value);
            for token in &mut value {
                if let Some(argument) = arguments.get(token.as_str()) {
                    *token = argument.clone();
                }
            }
            tokens.splice(index..index, value);
        } else {
            index += 1;
        }
    }
    tokens
}

fn atoi(text: &str) -> i32 {
    let mut chars = text
        .trim_start_matches(|ch: char| ch.is_ascii_whitespace())
        .chars()
        .peekable();
    let negative = chars.peek() == Some(&'-');
    if matches!(chars.peek(), Some('+' | '-')) {
        chars.next();
    }
    let mut value = 0i32;
    for ch in chars.take_while(char::is_ascii_digit) {
        value = value
            .wrapping_mul(10)
            .wrapping_add((ch as u8 - b'0') as i32);
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

fn reduce(tokens: &mut Vec<String>) {
    let mut index = 0;
    while index + 1 < tokens.len() {
        if tokens[index].starts_with('!') && tokens[index] != "!=" {
            let value = atoi(&tokens[index + 1]) == 0;
            tokens.splice(
                index..index + 2,
                [if value { "1".into() } else { "0".into() }],
            );
        } else {
            index += 1;
        }
    }
    for operators in ["*/%", "+-", "=!<>", "|&"] {
        let mut index = 0;
        while index + 2 < tokens.len() {
            if !tokens[index + 1]
                .chars()
                .next()
                .is_some_and(|ch| operators.contains(ch))
            {
                index += 1;
                continue;
            }
            let a = atoi(&tokens[index]);
            let b = atoi(&tokens[index + 2]);
            let divisor = if b == 0 { 1 } else { b };
            let value = match tokens[index + 1].as_str() {
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "/" => a.wrapping_div(divisor),
                "%" => a.wrapping_rem(divisor),
                "<" => i32::from(a < b),
                "<=" => i32::from(a <= b),
                ">" => i32::from(a > b),
                ">=" => i32::from(a >= b),
                "==" => i32::from(a == b),
                "!=" => i32::from(a != b),
                "||" => i32::from(a != 0 || b != 0),
                "&&" => i32::from(a != 0 && b != 0),
                _ => 0,
            };
            tokens.splice(index..index + 3, [value.to_string()]);
        }
    }
}

fn pair(tokens: &[String]) -> Option<(usize, usize)> {
    let start = tokens.iter().position(|token| token == "(")?;
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token == "(" {
            depth += 1;
        } else if token == ")" {
            depth -= 1;
            if depth == 0 {
                return Some((start, index));
            }
        }
    }
    None
}

fn evaluate(expression: &str, symbols: &BTreeMap<String, Symbol>, p: &LanguageDefinition) -> bool {
    let mut frames = vec![expand(tokenize(expression, p), symbols, p)];
    let mut pending = Vec::new();
    loop {
        let Some(current) = frames.last_mut() else {
            return false;
        };
        if let Some((start, end)) = pair(current) {
            let child = expand(current[start + 1..end].to_vec(), symbols, p);
            pending.push((start, end));
            frames.push(child);
            continue;
        }
        reduce(current);
        let Some(value) = frames.pop() else {
            return false;
        };
        if let Some(parent) = frames.last_mut() {
            if let Some((start, end)) = pending.pop() {
                parent.splice(start..=end, value);
            }
        } else {
            return !(value.is_empty()
                || (value.len() == 1 && (value[0].is_empty() || value[0] == "0")));
        }
    }
}
