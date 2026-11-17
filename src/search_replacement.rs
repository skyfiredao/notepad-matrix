use crate::search_matcher::Captures;

pub(super) fn format(
    template: &str,
    captures: &Captures<'_>,
    names: &[&str],
) -> Vec<u16> {
    let mut formatter = Formatter {
        template: template.encode_utf16().take_while(|&unit| unit != 0).collect(),
        at: 0,
        captures,
        names,
        output: Vec::new(),
        state: OutputState::Copy,
        restore_state: OutputState::Copy,
        have_conditional: false,
    };
    formatter.scope(0);
    formatter.output
}

#[derive(Clone, Copy)]
enum OutputState {
    Copy,
    NextLower,
    NextUpper,
    Lower,
    Upper,
    None,
}

struct Formatter<'a, 't> {
    template: Vec<u16>,
    at: usize,
    captures: &'a Captures<'t>,
    names: &'a [&'a str],
    output: Vec<u16>,
    state: OutputState,
    restore_state: OutputState,
    have_conditional: bool,
}

impl Formatter<'_, '_> {
    fn peek(&self) -> Option<u16> {
        self.template.get(self.at).copied()
    }

    fn put(&mut self, mut unit: u16) {
        match self.state {
            OutputState::None => return,
            OutputState::NextLower | OutputState::Lower => {
                if (u16::from(b'A')..=u16::from(b'Z')).contains(&unit) {
                    unit += u16::from(b'a' - b'A');
                }
                if matches!(self.state, OutputState::NextLower) {
                    self.state = self.restore_state;
                }
            }
            OutputState::NextUpper | OutputState::Upper => {
                if (u16::from(b'a')..=u16::from(b'z')).contains(&unit) {
                    unit -= u16::from(b'a' - b'A');
                }
                if matches!(self.state, OutputState::NextUpper) {
                    self.state = self.restore_state;
                }
            }
            OutputState::Copy => {}
        }
        self.output.push(unit);
    }

    fn put_units(&mut self, units: &[u16]) {
        for &unit in units {
            self.put(unit);
        }
    }

    fn capture(&self, index: i32) -> Vec<u16> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.captures.get_range(index))
            .map_or_else(Vec::new, |value| value.units().to_vec())
    }

    fn last_paren(&self) -> Vec<u16> {
        self.captures
            .get_range(self.captures.len().saturating_sub(1).max(1))
            .map_or_else(Vec::new, |value| value.units().to_vec())
    }

    fn last_closed(&self) -> Vec<u16> {
        self.captures
            .last_closed()
            .map_or_else(Vec::new, |value| value.units().to_vec())
    }

    fn number(&mut self, radix: i64, limit: usize) -> Option<i32> {
        let start = self.at;
        let mut number = 0i64;
        while self.at - start < limit {
            let Some(unit) = self.peek() else {
                break;
            };
            let digit = match unit {
                0x30..=0x39 => i64::from(unit - 0x30),
                0x41..=0x46 => i64::from(unit - 0x41 + 10),
                0x61..=0x66 => i64::from(unit - 0x61 + 10),
                _ => break,
            };
            if digit >= radix {
                break;
            }
            self.at += 1;
            number = number.checked_mul(radix)?.checked_add(digit)?;
            if number > i64::MAX / radix {
                return None;
            }
        }
        (self.at > start).then_some(number as i32)
    }

    fn braced_name(&mut self) -> Option<String> {
        if self.peek() != Some(u16::from(b'{')) {
            return None;
        }
        let start = self.at;
        self.at += 1;
        let name_start = self.at;
        while self.peek().is_some_and(|unit| unit != u16::from(b'}')) {
            self.at += 1;
        }
        if self.peek() == Some(u16::from(b'}')) {
            let name = String::from_utf16(&self.template[name_start..self.at]).ok();
            self.at += 1;
            name
        } else {
            self.at = start;
            None
        }
    }

    fn scope(&mut self, depth: usize) {
        while let Some(unit) = self.peek() {
            match unit {
                0x29 => return,
                0x3a if self.have_conditional => return,
                0x28 if depth < 100 => {
                    self.at += 1;
                    let have_conditional = self.have_conditional;
                    self.have_conditional = false;
                    self.until_scope_end(depth);
                    self.have_conditional = have_conditional;
                    if self.peek().is_none() {
                        return;
                    }
                    self.at += 1;
                }
                0x3f if depth < 100 => {
                    self.at += 1;
                    self.conditional(depth);
                }
                0x5c => {
                    self.at += 1;
                    self.escape();
                }
                0x24 => {
                    self.at += 1;
                    self.perl();
                }
                _ => {
                    self.at += 1;
                    self.put(unit);
                }
            }
        }
    }

    fn until_scope_end(&mut self, mut depth: usize) {
        loop {
            depth += 1;
            self.scope(depth);
            match self.peek() {
                None | Some(0x29) => return,
                Some(unit) => {
                    self.at += 1;
                    self.put(unit);
                }
            }
        }
    }

    fn conditional(&mut self, depth: usize) {
        let start = self.at;
        let matched = if self.peek() == Some(u16::from(b'{')) {
            self.at += 1;
            let index = self.number(10, usize::MAX);
            if let Some(index) = index.filter(|&index| index >= 0) {
                if self.peek() == Some(u16::from(b'}')) {
                    self.at += 1;
                    Some(self.captures.get_range(index as usize).is_some_and(|value| value.matched()))
                } else {
                    None
                }
            } else {
                while self.peek().is_some_and(|unit| unit != u16::from(b'}')) {
                    self.at += 1;
                }
                let name = String::from_utf16(&self.template[start + 1..self.at]).ok();
                if self.peek() == Some(u16::from(b'}')) {
                    self.at += 1;
                    name.filter(|name| self.names.contains(&name.as_str()))
                        .map(|name| self.captures.name(&name).is_some_and(|value| value.matched()))
                } else {
                    None
                }
            }
        } else {
            self.number(10, 2)
                .filter(|&index| index >= 0)
                .map(|index| self.captures.get_range(index as usize).is_some_and(|value| value.matched()))
        };
        let Some(matched) = matched else {
            self.at = start;
            self.put(u16::from(b'?'));
            return;
        };
        if matched {
            self.have_conditional = true;
            self.scope(depth + 1);
            self.have_conditional = false;
            if self.peek() == Some(u16::from(b':')) {
                self.at += 1;
                let saved_state = self.state;
                self.state = OutputState::None;
                self.until_scope_end(depth + 1);
                self.state = saved_state;
            }
        } else {
            let saved_state = self.state;
            self.state = OutputState::None;
            self.have_conditional = true;
            self.scope(depth + 1);
            self.have_conditional = false;
            self.state = saved_state;
            if self.peek() == Some(u16::from(b':')) {
                self.at += 1;
                self.until_scope_end(depth + 1);
            }
        }
    }

    fn escape(&mut self) {
        let Some(unit) = self.peek() else {
            self.put(u16::from(b'\\'));
            return;
        };
        self.at += 1;
        let value = match unit {
            0x55 => {
                self.state = OutputState::Upper;
                return;
            }
            0x4c => {
                self.state = OutputState::Lower;
                return;
            }
            0x45 => {
                self.state = OutputState::Copy;
                return;
            }
            0x75 => {
                self.restore_state = self.state;
                self.state = OutputState::NextUpper;
                return;
            }
            0x6c => {
                self.restore_state = self.state;
                self.state = OutputState::NextLower;
                return;
            }
            0x61 => 7,
            0x65 => 27,
            0x66 => 12,
            0x6e => 10,
            0x72 => 13,
            0x74 => 9,
            0x76 => 11,
            0x63 => match self.peek() {
                Some(unit) => {
                    self.at += 1;
                    unit % 32
                }
                None => u16::from(b'c'),
            },
            0x78 => {
                let start = self.at;
                if self.peek() == Some(u16::from(b'{')) {
                    self.at += 1;
                    match self.number(16, usize::MAX).filter(|&value| value >= 0) {
                        Some(value) if self.peek() == Some(u16::from(b'}')) => {
                            self.at += 1;
                            value as u16
                        }
                        Some(_) => {
                            self.at = start;
                            u16::from(b'x')
                        }
                        None => {
                            self.put(u16::from(b'x'));
                            self.put(u16::from(b'{'));
                            return;
                        }
                    }
                } else {
                    match self.number(16, 2).filter(|&value| value >= 0) {
                        Some(value) => value as u16,
                        None => u16::from(b'x'),
                    }
                }
            }
            0x30 => {
                self.at -= 1;
                self.number(8, 4).unwrap_or_default() as u16
            }
            0x31..=0x39 => {
                let value = self.capture(i32::from(unit - u16::from(b'0')));
                self.put_units(&value);
                return;
            }
            other => other,
        };
        self.put(value);
    }

    fn verb(&mut self, have_brace: bool) -> Option<Vec<u16>> {
        if have_brace && self.peek() == Some(u16::from(b'^')) {
            self.at += 1;
        }
        for name in [
            "MATCH",
            "PREMATCH",
            "POSTMATCH",
            "LAST_PAREN_MATCH",
            "LAST_SUBMATCH_RESULT",
            "^N",
        ] {
            if !self.template[self.at..]
                .iter()
                .copied()
                .take(name.len())
                .eq(name.encode_utf16())
            {
                continue;
            }
            let end = self.at + name.len();
            if have_brace && self.template.get(end) != Some(&u16::from(b'}')) {
                return None;
            }
            self.at = end + usize::from(have_brace);
            return Some(match name {
                "MATCH" => self.capture(0),
                "PREMATCH" => self.captures.prefix().to_vec(),
                "POSTMATCH" => self.captures.suffix().to_vec(),
                "LAST_PAREN_MATCH" => self.last_paren(),
                _ => self.last_closed(),
            });
        }
        None
    }

    fn perl(&mut self) {
        let start = self.at;
        let value = match self.peek() {
            Some(0x24) => {
                self.at += 1;
                Some(vec![u16::from(b'$')])
            }
            Some(0x26) => {
                self.at += 1;
                Some(self.capture(0))
            }
            Some(0x60) => {
                self.at += 1;
                Some(self.captures.prefix().to_vec())
            }
            Some(0x27) => {
                self.at += 1;
                Some(self.captures.suffix().to_vec())
            }
            Some(0x2b) => {
                self.at += 1;
                if let Some(name) = self.braced_name() {
                    Some(
                        self.captures
                            .name(&name)
                            .map_or_else(Vec::new, |value| value.units().to_vec()),
                    )
                } else {
                    Some(self.last_paren())
                }
            }
            _ => {
                let have_brace = self.peek() == Some(u16::from(b'{'));
                self.at += usize::from(have_brace);
                match self.number(10, usize::MAX) {
                    Some(index)
                        if index >= 0
                            && (!have_brace || self.peek() == Some(u16::from(b'}'))) =>
                    {
                        self.at += usize::from(have_brace);
                        Some(self.capture(index))
                    }
                    _ => self.verb(have_brace),
                }
            }
        };
        match value {
            Some(value) => self.put_units(&value),
            None => {
                self.at = start;
                self.put(u16::from(b'$'));
            }
        }
    }
}
