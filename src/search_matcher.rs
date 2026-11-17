use std::collections::BTreeMap;
use std::sync::Arc;

const SOURCE_PARSE_DEPTH_LIMIT: usize = 400;

pub(crate) struct Matcher {
    program: Program,
    names: BTreeMap<String, Vec<usize>>,
    named_groups: BTreeMap<u32, Vec<usize>>,
    traits: Arc<UnitTraits>,
}

pub(crate) struct Captures<'a> {
    input: ExecutionInput<'a>,
    groups: Vec<CapturedRange<'a>>,
    last_closed: Option<usize>,
    names: &'a BTreeMap<String, Vec<usize>>,
    named_groups: &'a BTreeMap<u32, Vec<usize>>,
    prefix: CapturedRange<'a>,
    suffix: CapturedRange<'a>,
}

struct CapturedRange<'a> {
    first: Cursor<'a>,
    last: Cursor<'a>,
    matched: bool,
    units: std::cell::OnceCell<Vec<u16>>,
}

impl<'a> CapturedRange<'a> {
    fn new(first: Cursor<'a>, last: Cursor<'a>, matched: bool) -> Self {
        Self { first, last, matched, units: std::cell::OnceCell::new() }
    }
    fn units(&self, input: &ExecutionInput<'a>) -> &[u16] {
        self.units.get_or_init(|| input.copy_units(&self.first, &self.last))
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Match<'a> {
    input: ExecutionInput<'a>,
    range: &'a CapturedRange<'a>,
    start: usize,
    end: usize,
}

impl<'a> Match<'a> {
    pub(crate) fn start(&self) -> usize { self.start }
    pub(crate) fn end(&self) -> usize { self.end }
    pub(crate) fn units(&self) -> &'a [u16] { self.range.units(&self.input) }
    pub(crate) fn matched(&self) -> bool { self.range.matched }
}

impl<'a> Captures<'a> {
    pub(crate) fn len(&self) -> usize { self.groups.len() }

    pub(crate) fn get(&self, index: usize) -> Option<Match<'_>> {
        self.get_range(index).filter(|value| index == 0 || value.matched())
    }

    pub(crate) fn get_range(&self, index: usize) -> Option<Match<'_>> {
        let group = self.groups.get(index)?;
        Some(Match { input: self.input, range: group, start: group.first.offset(), end: group.last.offset() })
    }

    pub(crate) fn name(&self, name: &str) -> Option<Match<'_>> {
        self.named_groups.get(&capture_name_hash(name))?.iter().find_map(|&index| self.get(index).filter(|value| value.matched()))
    }

    pub(crate) fn last_closed(&self) -> Option<Match<'_>> {
        self.get_range(self.last_closed?)
    }

    pub(crate) fn names(&self) -> Vec<&str> {
        self.names.keys().map(String::as_str).collect()
    }
    pub(crate) fn prefix(&self) -> &[u16] { self.prefix.units(&self.input) }
    pub(crate) fn suffix(&self) -> &[u16] { self.suffix.units(&self.input) }
}

#[derive(Clone)]
enum Reference {
    Number(usize),
    Name(String),
}

#[derive(Clone)]
enum Condition {
    Capture(Reference),
    Recursion(Option<Reference>),
    Assertion(usize),
    Define,
}

#[derive(Clone, Copy)]
enum Anchor { StartText, EndText, StartLine, EndLine, SoftEnd, Word, NotWord, WordStart, WordEnd, Continue }

#[derive(Clone, Copy)]
enum Verb { Fail, Accept, Commit, Skip, Prune, Then }

#[derive(Clone, Copy)]
enum Restart { Any, Line, Word, Buffer, Continue }

enum Node {
    Empty,
    Unit(u16, bool),
    Any(bool),
    Class(UnitClass, bool),
    Concat(Vec<usize>),
    Scope(usize),
    Alt(Vec<usize>),
    Group(usize, usize),
    Repeat { child: usize, lo: usize, hi: usize, greedy: bool },
    Look { child: usize, positive: bool, behind: Option<usize> },
    Atomic(usize),
    Backref(Reference, bool),
    Call(Reference),
    Conditional(Condition, usize, usize),
    Anchor(Anchor),
    KeepOut,
    Newline,
    Combining(bool),
    Case(bool),
    Verb(Verb),
}

#[derive(Clone)]
struct UnitClass {
    negated: bool,
    items: Vec<ClassItem>,
}

impl UnitClass {
    fn singleton(&self) -> bool {
        self.items.iter().all(|item| match item {
            ClassItem::Literal(units) | ClassItem::Equivalent(units) => units.len() == 1,
            ClassItem::Range(start, end) => start.len() == 1 && end.len() == 1,
            ClassItem::Mask(_, _) => true,
        })
    }
}

#[derive(Clone)]
enum ClassItem {
    Literal(Vec<u16>),
    Range(Vec<u16>, Vec<u16>),
    Mask(u16, bool),
    Equivalent(Vec<u16>),
}

#[derive(Clone, Copy)]
struct ParseFlags { casei: bool, dot: bool, multi: bool, extended: bool }

struct Parser {
    units: Vec<u16>,
    at: usize,
    nodes: Vec<Node>,
    groups: Vec<usize>,
    names: BTreeMap<String, Vec<usize>>,
    name_order: Vec<(String, usize)>,
    group_count: usize,
    depth: usize,
    case_epoch: usize,
    flags: ParseFlags,
    traits: Arc<UnitTraits>,
}

impl Matcher {
    pub(crate) fn new(query: &str, match_case: bool, dot_matches_newline: bool) -> Result<Self, String> {
        let traits = unit_traits()?;
        let mut parser = Parser {
            units: query.encode_utf16().take_while(|&unit| unit != 0).collect(),
            at: 0,
            nodes: Vec::new(),
            groups: vec![0],
            names: BTreeMap::new(),
            name_order: Vec::new(),
            group_count: 0,
            depth: 0,
            case_epoch: 0,
            flags: ParseFlags { casei: !match_case, dot: dot_matches_newline, multi: true, extended: false },
            traits: traits.clone(),
        };
        let root = parser.expression(None)?;
        if parser.at != parser.units.len() { return Err(parser.error("Unmatched closing parenthesis")); }
        parser.groups[0] = root;
        for node in &parser.nodes {
            match node {
                Node::Backref(reference, _) => parser.validate_reference(reference)?,
                _ => {}
            }
        }
        let mut named_groups = BTreeMap::<u32, Vec<usize>>::new();
        for (name, index) in parser.name_order { named_groups.entry(capture_name_hash(&name)).or_default().push(index); }
        let program = Program::compile(&parser.nodes, root, parser.groups.len(), &named_groups, &traits, !match_case)?;
        Ok(Self { program, names: parser.names, named_groups, traits })
    }

    pub(crate) fn captures_in_bytes<'a>(&'a self, bytes: &'a crate::byte_buffer::ByteBuffer, start: usize, end: usize) -> Result<Option<Captures<'a>>, String> {
        if start > end || end > bytes.len() { return Err("The regex range is outside the document".to_owned()); }
        let input = ExecutionInput { bytes, end };
        let first = input.cursor(start)?;
        let mut vm = SourceVm::new(self, input, first.clone(), false, false)?;
        if !vm.find()? { return Ok(None); }
        let groups = vm.results.groups.iter().map(|group| CapturedRange::new(group.start.clone(), group.end.clone(), group.matched)).collect();
        let prefix = CapturedRange::new(first.clone(), vm.results.groups[0].start.clone(), !input.same(&first, &vm.results.groups[0].start));
        let end = input.end_cursor()?;
        let suffix = CapturedRange::new(vm.results.groups[0].end.clone(), end.clone(), !input.same(&vm.results.groups[0].end, &end));
        Ok(Some(Captures { input, groups, last_closed: vm.results.last_closed, names: &self.names, named_groups: &self.named_groups, prefix, suffix }))
    }

}

impl Parser {
    fn push(&mut self, node: Node) -> usize { let index = self.nodes.len(); self.nodes.push(node); index }
    fn peek(&self) -> Option<u16> { self.units.get(self.at).copied() }
    fn take(&mut self, unit: u16) -> bool {
        if self.peek() == Some(unit) { self.at += 1; true } else { false }
    }
    fn error(&self, message: &str) -> String {
        let position = self.at.min(self.units.len());
        let start = position.saturating_sub(10);
        let end = position.saturating_add(10).min(self.units.len());
        let mut output = message.to_owned();
        output.push_str(if start != 0 || end != self.units.len() { "  The error occurred while parsing the regular expression fragment: '" } else { "  The error occurred while parsing the regular expression: '" });
        if start != end {
            output.extend(self.units[start..position].iter().map(|&unit| char::from(unit as u8)));
            output.push_str(">>>HERE>>>");
            output.extend(self.units[position..end].iter().map(|&unit| char::from(unit as u8)));
        }
        output.push_str("'.");
        output
    }
    fn expect(&mut self, unit: u16, message: &str) -> Result<(), String> {
        if self.take(unit) { Ok(()) } else { Err(self.error(message)) }
    }
    fn skip_space(&mut self) {
        while self.peek().is_some_and(|unit| self.traits.has(unit, 0x0008)) { self.at += 1; }
    }
    fn class_name(&self, name: &str) -> Option<u16> {
        if let Some(mask) = named_mask(name) { return Some(mask); }
        let mut lowered = Vec::new();
        for unit in name.encode_utf16() {
            let unit = self.traits.translate(unit, true);
            if unit > 127 { return None; }
            lowered.push(unit as u8);
        }
        named_mask(std::str::from_utf8(&lowered).ok()?)
    }
    fn skip_ignored(&mut self) {
        loop {
            if self.flags.extended {
                self.skip_space();
                if self.take(35) {
                    while let Some(unit) = self.peek() { self.at += 1; if separator(unit) { break; } }
                    continue;
                }
            }
            if self.units.get(self.at..self.at + 3) == Some(&[40, 63, 35]) {
                self.at += 3;
                while let Some(unit) = self.peek() { self.at += 1; if unit == 41 { break; } }
                continue;
            }
            break;
        }
    }
    fn validate_reference(&self, reference: &Reference) -> Result<(), String> {
        let valid = match reference {
            Reference::Number(index) if *index < 0x40000000 => *index < self.groups.len(),
            Reference::Number(index) => self.names.keys().any(|candidate| capture_name_hash(candidate) == *index as u32),
            Reference::Name(name) => self.names.keys().any(|candidate| capture_name_hash(candidate) == capture_name_hash(name)),
        };
        if valid { Ok(()) } else { Err(self.error("Found a backreference to a non-existant sub-expression.")) }
    }
    fn expression(&mut self, reset: Option<usize>) -> Result<usize, String> {
        self.enter_scope()?;
        let result = self.expression_body(reset);
        self.depth -= 1;
        result
    }
    fn enter_scope(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > SOURCE_PARSE_DEPTH_LIMIT {
            self.depth -= 1;
            return Err(self.error("Exceeded nested brace limit."));
        }
        Ok(())
    }
    fn expression_body(&mut self, reset: Option<usize>) -> Result<usize, String> {
        let mut branches = Vec::new();
        let mut maximum = self.group_count;
        let case_epoch = self.case_epoch;
        let mut branch_case = None;
        loop {
            let branch = self.sequence()?;
            let branch = if let Some(casei) = branch_case { let case = self.push(Node::Case(casei)); self.push(Node::Concat(vec![case, branch])) } else { branch };
            branches.push(branch);
            maximum = maximum.max(self.group_count);
            if !self.take(124) { break; }
            branch_case = (self.case_epoch != case_epoch).then_some(self.flags.casei);
            if let Some(reset) = reset { self.group_count = reset; }
        }
        self.group_count = maximum;
        if branches.len() == 1 { Ok(branches[0]) } else { Ok(self.push(Node::Alt(branches))) }
    }
    fn sequence(&mut self) -> Result<usize, String> {
        let mut children = Vec::new();
        loop {
            self.skip_ignored();
            if self.peek().is_none() || matches!(self.peek(), Some(41 | 124)) { break; }
            let mut child = self.atom()?;
            if let Node::Concat(parts) = &self.nodes[child] {
                let parts = parts.clone();
                if let Some((&last, prefix)) = parts.split_last() {
                    children.extend_from_slice(prefix);
                    child = last;
                }
            }
            if matches!(self.nodes[child], Node::Empty) {
                if let Some(previous) = children.pop() { child = previous; }
                else {
                    self.skip_ignored();
                    if self.quantifier()?.is_some() { return Err(self.error("Nothing to repeat.")); }
                    continue;
                }
            }
            self.skip_ignored();
            if let Some((lo, hi)) = self.quantifier()? {
                if matches!(self.nodes[child], Node::Anchor(_) | Node::KeepOut) {
                    return Err(self.error("A repetition operator cannot be applied to a zero-width assertion"));
                }
                self.skip_ignored();
                let greedy = !self.take(63);
                let possessive = self.take(43);
                child = self.push(Node::Repeat { child, lo, hi, greedy });
                if possessive { child = self.push(Node::Atomic(child)); }
                self.skip_ignored();
                if matches!(self.peek(), Some(42 | 43 | 63)) || self.valid_brace_quantifier() {
                    return Err(self.error("Invalid preceding regular expression prior to repetition operator"));
                }
            }
            children.push(child);
        }
        Ok(match children.len() { 0 => self.push(Node::Empty), 1 => children[0], _ => self.push(Node::Concat(children)) })
    }
    fn number(&mut self, radix: u32, maximum: usize) -> Result<Option<usize>, String> {
        let begin = self.at;
        let mut value = 0u64;
        let limit = i64::MAX as u64 / u64::from(radix);
        while self.at - begin < maximum {
            let Some(digit) = self.peek().and_then(|unit| char::from_u32(u32::from(unit))).and_then(|unit| unit.to_digit(radix)) else { break; };
            self.at += 1;
            let Some(next) = value.checked_mul(u64::from(radix)).and_then(|value| value.checked_add(u64::from(digit))) else { return Ok(None); };
            value = next;
            if value > limit { return Ok(None); }
        }
        Ok(if self.at > begin { usize::try_from(value).ok() } else { None })
    }
    fn valid_brace_quantifier(&mut self) -> bool {
        let at = self.at;
        let result = self.quantifier().is_ok_and(|value| value.is_some());
        self.at = at;
        result
    }
    fn quantifier(&mut self) -> Result<Option<(usize, usize)>, String> {
        if self.take(42) { return Ok(Some((0, usize::MAX))); }
        if self.take(43) { return Ok(Some((1, usize::MAX))); }
        if self.take(63) { return Ok(Some((0, 1))); }
        let begin = self.at;
        if !self.take(123) { return Ok(None); }
        self.skip_space();
        let Some(lo) = self.number(10, usize::MAX)? else { self.at = begin; return Ok(None); };
        self.skip_space();
        let hi = if self.take(44) {
            self.skip_space();
            let hi = self.number(10, usize::MAX)?.unwrap_or(usize::MAX);
            self.skip_space();
            hi
        } else { lo };
        if !self.take(125) { self.at = begin; return Ok(None); }
        if lo > hi { return Err(self.error("Invalid content of repeat range")); }
        Ok(Some((lo, hi)))
    }
    fn atom(&mut self) -> Result<usize, String> {
        let unit = self.peek().ok_or_else(|| self.error("Premature end of regular expression"))?;
        self.at += 1;
        Ok(match unit {
            40 => return self.group(),
            91 => return self.class(),
            92 => return self.escape(),
            46 => self.push(Node::Any(self.flags.dot)),
            94 => self.push(Node::Anchor(if self.flags.multi { Anchor::StartLine } else { Anchor::StartText })),
            36 => self.push(Node::Anchor(if self.flags.multi { Anchor::EndLine } else { Anchor::EndText })),
            42 | 43 | 63 => return Err(self.error("Nothing to repeat")),
            _ => self.push(Node::Unit(unit, self.flags.casei)),
        })
    }
    fn read_name(&mut self, delimiter: u16) -> Result<String, String> {
        let start = self.at;
        while self.peek().is_some_and(|unit| unit != delimiter) { self.at += 1; }
        let name = String::from_utf16(&self.units[start..self.at]).map_err(|_| self.error("Invalid capture name"))?;
        self.expect(delimiter, "Unterminated named capture")?;
        Ok(name)
    }
    fn capture(&mut self, name: Option<String>) -> Result<usize, String> {
        self.group_count += 1;
        let index = self.group_count;
        if self.groups.len() <= index { self.groups.resize(index + 1, usize::MAX); }
        if let Some(name) = name {
            self.name_order.push((name.clone(), index));
            let indices = self.names.entry(name).or_default();
            if !indices.contains(&index) { indices.push(index); }
        }
        let flags = self.flags;
        let epoch = self.case_epoch;
        let child = self.expression(None)?;
        self.expect(41, "Unmatched marking parenthesis")?;
        let child = self.scope_end(child, flags, epoch);
        let group = self.push(Node::Group(index, child));
        if self.groups[index] == usize::MAX { self.groups[index] = group; }
        Ok(group)
    }
    fn group(&mut self) -> Result<usize, String> {
        if self.take(42) {
            let verb = self.read_name(41)?;
            return Ok(self.push(Node::Verb(match verb.as_str() {
                "F" | "FAIL" => Verb::Fail, "ACCEPT" => Verb::Accept, "COMMIT" => Verb::Commit,
                "SKIP" => Verb::Skip, "PRUNE" => Verb::Prune, "THEN" => Verb::Then,
                _ => return Err(self.error("Invalid or unterminated Perl (*...) sequence")),
            })));
        }
        if !self.take(63) { return self.capture(None); }
        let flags = self.flags;
        let epoch = self.case_epoch;
        if self.take(39) { let name = self.read_name(39)?; return self.capture(Some(name)); }
        if self.take(60) {
            if !matches!(self.peek(), Some(61 | 33)) {
                let name = self.read_name(62)?;
                return self.capture(Some(name));
            }
            let positive = self.take(61);
            if !positive { self.expect(33, "Invalid lookbehind assertion")?; }
            let child = self.expression(None)?;
            self.expect(41, "Unmatched marking parenthesis")?;
            let child = self.scope_end(child, flags, epoch);
            return Ok(self.push(Node::Look { child, positive, behind: Some(0) }));
        }
        if matches!(self.peek(), Some(61 | 33)) {
            let positive = self.take(61);
            if !positive { self.expect(33, "Invalid lookahead assertion")?; }
            let child = self.expression(None)?;
            self.expect(41, "Unmatched marking parenthesis")?;
            let child = self.scope_end(child, flags, epoch);
            if positive && matches!(self.nodes[child], Node::Empty) { return Err(self.error("Invalid or empty zero width assertion")); }
            return Ok(self.push(Node::Look { child, positive, behind: None }));
        }
        if self.take(40) { return self.conditional(flags, epoch); }
        if self.take(82) { self.expect(41, "Invalid recursive sub-expression")?; return Ok(self.push(Node::Call(Reference::Number(0)))); }
        if self.take(38) { let name = self.read_name(41)?; return Ok(self.push(Node::Call(Reference::Name(name)))); }
        if self.take(80) {
            self.expect(62, "Invalid or unterminated Perl (?P...) sequence")?;
            let name = self.read_name(41)?;
            return Ok(self.push(Node::Call(Reference::Name(name))));
        }
        if self.peek().is_some_and(|unit| (48..=57).contains(&unit)) {
            let index = self.number(10, usize::MAX)?.ok_or_else(|| self.error("Invalid recursive sub-expression"))?;
            self.expect(41, "Invalid recursive sub-expression")?;
            return Ok(self.push(Node::Call(Reference::Number(index))));
        }
        if matches!(self.peek(), Some(43 | 45)) && self.units.get(self.at + 1).is_some_and(|unit| (48..=57).contains(unit)) {
            let forward = self.take(43);
            if !forward { self.expect(45, "Invalid relative recursion")?; }
            let amount = self.number(10, usize::MAX)?.ok_or_else(|| self.error("Invalid relative recursion"))?;
            let index = if forward { self.group_count.checked_add(amount) } else { (self.group_count + 1).checked_sub(amount) }
                .filter(|&index| amount > 0 && index > 0).ok_or_else(|| self.error("Invalid relative recursive sub-expression"))?;
            self.expect(41, "Invalid relative recursive sub-expression")?;
            return Ok(self.push(Node::Call(Reference::Number(index))));
        }
        let atomic = self.take(62);
        let reset = if self.take(124) { Some(self.group_count) } else { None };
        if atomic || reset.is_some() || self.take(58) {
            let child = self.expression(reset)?;
            self.expect(41, "Unmatched marking parenthesis")?;
            let child = self.scope_end(child, flags, epoch);
            if atomic && matches!(self.nodes[child], Node::Empty) { return Err(self.error("Invalid or empty independent sub-expression")); }
            return Ok(if atomic { self.push(Node::Atomic(child)) } else { self.push(Node::Scope(child)) });
        }
        let mut negative = false;
        let mut any = false;
        while let Some(unit) = self.peek() {
            if unit == 45 && !negative { negative = true; self.at += 1; continue; }
            let enabled = !negative;
            match unit {
                105 => self.flags.casei = enabled, 109 => self.flags.multi = enabled,
                115 => self.flags.dot = enabled, 120 => self.flags.extended = enabled,
                _ => break,
            }
            any = true;
            self.at += 1;
        }
        if !any && !negative { return Err(self.error("Invalid or unterminated Perl (?...) sequence")); }
        if self.take(41) {
            let child = if self.flags.casei != flags.casei {
                self.case_epoch += 1;
                self.push(Node::Case(self.flags.casei))
            } else { self.push(Node::Empty) };
            return Ok(self.push(Node::Scope(child)));
        }
        self.expect(58, "Invalid or unterminated Perl (?...) sequence")?;
        let change = (self.flags.casei != flags.casei).then_some(self.flags.casei);
        let child = self.expression(None)?;
        self.expect(41, "Unmatched marking parenthesis")?;
        let child = if let Some(casei) = change { let case = self.push(Node::Case(casei)); self.push(Node::Concat(vec![case, child])) } else { child };
        let child = self.scope_end(child, flags, epoch);
        Ok(self.push(Node::Scope(child)))
    }
    fn scope_end(&mut self, child: usize, flags: ParseFlags, epoch: usize) -> usize {
        let child = if self.flags.casei != flags.casei || self.case_epoch != epoch {
            let case = self.push(Node::Case(flags.casei));
            self.push(Node::Concat(vec![child, case]))
        } else { child };
        self.flags = flags;
        self.case_epoch = epoch;
        child
    }
    fn conditional(&mut self, flags: ParseFlags, epoch: usize) -> Result<usize, String> {
        self.enter_scope()?;
        let result = self.conditional_body(flags, epoch);
        self.depth -= 1;
        result
    }
    fn conditional_body(&mut self, flags: ParseFlags, epoch: usize) -> Result<usize, String> {
        let condition = if self.peek() == Some(63) {
            let assertion = self.group()?;
            if !matches!(self.nodes[assertion], Node::Look { .. }) { return Err(self.error("Invalid conditional assertion")); }
            Condition::Assertion(assertion)
        } else {
            let condition = if self.take(82) {
                if self.take(38) { Condition::Recursion(Some(Reference::Name(self.read_name(41)?))) }
                else {
                    let number = self.number(10, usize::MAX)?;
                    self.expect(41, "Invalid recursion condition")?;
                    Condition::Recursion(number.filter(|&number| number != 0).map(Reference::Number))
                }
            } else if matches!(self.peek(), Some(60 | 39)) {
                let delimiter = if self.take(60) { 62 } else { self.expect(39, "Invalid named condition")?; 39 };
                let name = self.read_name(delimiter)?;
                self.expect(41, "Invalid named capture condition")?;
                Condition::Capture(Reference::Name(name))
            } else if self.peek().is_some_and(|unit| (48..=57).contains(&unit)) {
                let number = self.number(10, usize::MAX)?.filter(|&number| number > 0)
                    .ok_or_else(|| self.error("Invalid capture condition"))?;
                self.expect(41, "Invalid capture condition")?;
                Condition::Capture(Reference::Number(number))
            } else {
                let name = self.read_name(41)?;
                if name != "DEFINE" { return Err(self.error("Invalid conditional expression")); }
                Condition::Define
            };
            condition
        };
        let true_branch = self.sequence()?;
        let false_branch = if self.take(124) {
            if matches!(condition, Condition::Define) { return Err(self.error("Alternation operators are not allowed inside a DEFINE block")); }
            let case = (self.case_epoch != epoch).then_some(self.flags.casei);
            let branch = self.sequence()?;
            if let Some(casei) = case { let case = self.push(Node::Case(casei)); self.push(Node::Concat(vec![case, branch])) } else { branch }
        } else { self.push(Node::Empty) };
        self.expect(41, "More than one alternation operator or unmatched conditional parenthesis")?;
        let conditional = self.push(Node::Conditional(condition, true_branch, false_branch));
        let child = self.scope_end(conditional, flags, epoch);
        Ok(self.push(Node::Scope(child)))
    }
    fn escape(&mut self) -> Result<usize, String> {
        let unit = self.peek().ok_or_else(|| self.error("Incomplete escape sequence found"))?;
        if let Some(mask) = class_mask(unit) {
            self.at += 1;
            let negated = (65..=90).contains(&unit);
            return Ok(self.push(Node::Class(UnitClass { negated, items: vec![ClassItem::Mask(mask, false)] }, self.flags.casei)));
        }
        let anchor = match unit {
            65 | 96 => Some(Anchor::StartText), 122 | 39 => Some(Anchor::EndText),
            90 => Some(Anchor::SoftEnd), 98 => Some(Anchor::Word), 66 => Some(Anchor::NotWord),
            60 => Some(Anchor::WordStart), 62 => Some(Anchor::WordEnd), 71 => Some(Anchor::Continue),
            _ => None,
        };
        if let Some(anchor) = anchor { self.at += 1; return Ok(self.push(Node::Anchor(anchor))); }
        match unit {
            49..=57 => { self.at += 1; Ok(self.push(Node::Backref(Reference::Number(usize::from(unit - 48)), self.flags.casei))) }
            103 | 107 => {
                self.at += 1;
                let delimiter = match self.peek() { Some(123) => Some(125), Some(60) => Some(62), Some(39) => Some(39), _ => None };
                if delimiter.is_some() { self.at += 1; }
                let negative = self.take(45);
                let numeric_begin = self.at;
                let reference = if let Some(amount) = self.number(10, usize::MAX)? {
                    let index = if negative { (self.group_count + 1).checked_sub(amount) } else { Some(amount) }
                        .filter(|&index| index > 0).ok_or_else(|| self.error("Invalid extended back reference"))?;
                    if let Some(delimiter) = delimiter { self.expect(delimiter, "Incomplete extended back reference")?; }
                    Reference::Number(index)
                } else {
                    self.at = numeric_begin;
                    if negative { return Err(self.error("Invalid extended back reference")); }
                    Reference::Name(self.read_name(delimiter.ok_or_else(|| self.error("Incomplete extended back reference"))?)?)
                };
                self.validate_reference(&reference).or_else(|error| if matches!(reference, Reference::Number(_)) { Ok(()) } else { Err(error) })?;
                Ok(self.push(Node::Backref(reference, self.flags.casei)))
            }
            112 | 80 => {
                self.at += 1;
                let name = if self.take(123) { self.read_name(125)? } else {
                    let unit = self.peek().ok_or_else(|| self.error("Incomplete property escape"))?;
                    self.at += 1;
                    String::from_utf16(&[unit]).map_err(|_| self.error("Invalid property escape"))?
                };
                let mask = self.class_name(&name).ok_or_else(|| self.error("Escape sequence was neither a valid property nor a valid character class name."))?;
                Ok(self.push(Node::Class(UnitClass { negated: unit == 80, items: vec![ClassItem::Mask(mask, false)] }, self.flags.casei)))
            }
            81 => {
                self.at += 1;
                let mut children = Vec::new();
                while let Some(unit) = self.peek() {
                    if unit == 92 && self.units.get(self.at + 1) == Some(&69) { self.at += 2; break; }
                    if unit == 92 && self.at + 1 == self.units.len() {
                        self.at += 1;
                        return Err(self.error("Unterminated \\Q...\\E sequence."));
                    }
                    self.at += 1;
                    children.push(self.push(Node::Unit(unit, self.flags.casei)));
                }
                Ok(match children.len() { 0 => self.push(Node::Empty), 1 => children[0], _ => self.push(Node::Concat(children)) })
            }
            75 => { self.at += 1; Ok(self.push(Node::KeepOut)) }
            82 => { self.at += 1; Ok(self.push(Node::Newline)) }
            88 => { self.at += 1; Ok(self.push(Node::Combining(self.flags.casei))) }
            67 => { self.at += 1; Ok(self.push(Node::Any(self.flags.dot))) }
            _ => { let unit = self.unescape_unit()?; Ok(self.push(Node::Unit(unit, self.flags.casei))) }
        }
    }
    fn unescape_unit(&mut self) -> Result<u16, String> {
        let unit = self.peek().ok_or_else(|| self.error("Escape sequence terminated prematurely"))?;
        self.at += 1;
        Ok(match unit {
            97 => 7, 101 => 27, 102 => 12, 110 => 10, 114 => 13, 116 => 9, 118 => 11, 98 => 8,
            99 => { let unit = self.peek().ok_or_else(|| self.error("ASCII escape sequence terminated prematurely"))?; self.at += 1; unit % 32 }
            120 => {
                let braced = self.take(123);
                let value = self.number(16, if braced { usize::MAX } else { 2 })?
                    .ok_or_else(|| self.error("Hexadecimal escape sequence was invalid"))?;
                if braced { self.expect(125, "Missing } in hexadecimal escape sequence")?; }
                u16::try_from(value).map_err(|_| self.error("Hexadecimal escape sequence exceeds wchar_t"))?
            }
            48 => { self.at -= 1; let value = self.number(8, 4)?.ok_or_else(|| self.error("Invalid octal escape sequence"))?; u16::try_from(value).map_err(|_| self.error("Invalid octal escape sequence"))? }
            49..=57 => return Err(self.error("Invalid octal escape sequence")),
            78 => {
                self.expect(123, "Incomplete named character escape")?;
                let name = self.read_name(125)?;
                let value = collating_name(&name).ok_or_else(|| self.error("Invalid collation character"))?;
                if value.len() != 1 { return Err(self.error("Named character escape does not encode one wchar_t")); }
                value[0]
            }
            _ => unit,
        })
    }
    fn class(&mut self) -> Result<usize, String> {
        if self.units.get(self.at..self.at + 6) == Some(&[91, 58, 60, 58, 93, 93]) { self.at += 6; return Ok(self.push(Node::Anchor(Anchor::WordStart))); }
        if self.units.get(self.at..self.at + 6) == Some(&[91, 58, 62, 58, 93, 93]) { self.at += 6; return Ok(self.push(Node::Anchor(Anchor::WordEnd))); }
        let mut class = UnitClass { negated: self.take(94), items: Vec::new() };
        loop {
            if self.peek().is_none() { return Err(self.error("Character set declaration terminated prematurely")); }
            if self.peek() == Some(93) && !class.items.is_empty() { self.at += 1; break; }
            if self.peek() == Some(91) && matches!(self.units.get(self.at + 1), Some(58 | 61)) {
                self.at += 1;
                let kind = self.peek().ok_or_else(|| self.error("Incomplete character class"))?;
                self.at += 1;
                let name = self.read_name(kind)?;
                self.expect(93, "Incomplete character class")?;
                if kind == 58 {
                    let (negated, name) = name.strip_prefix('^').map_or((false, name.as_str()), |name| (true, name));
                    let mask = self.class_name(name).ok_or_else(|| self.error("Invalid character class name, collating name, or character range."))?;
                    class.items.push(ClassItem::Mask(mask, negated));
                } else {
                    class.items.push(ClassItem::Equivalent(collating_name(&name).ok_or_else(|| self.error("Invalid collation character"))?));
                }
                continue;
            }
            if self.peek() == Some(92) {
                if let Some(unit) = self.units.get(self.at + 1).copied() {
                    if let Some(mask) = class_mask(unit) {
                        self.at += 2;
                        class.items.push(ClassItem::Mask(mask, (65..=90).contains(&unit)));
                        continue;
                    }
                }
            }
            let start = self.class_literal(class.items.is_empty())?;
            if self.peek() == Some(45) && self.units.get(self.at + 1) != Some(&93) {
                self.at += 1;
                let end = self.class_literal(false)?;
                let translated_start: Vec<_> = start.iter().map(|&unit| self.traits.translate(unit, self.flags.casei)).collect();
                let translated_end: Vec<_> = end.iter().map(|&unit| self.traits.translate(unit, self.flags.casei)).collect();
                if translated_start > translated_end { return Err(self.error("Invalid range end in character class")); }
                class.items.push(ClassItem::Range(start, end));
            } else { class.items.push(ClassItem::Literal(start)); }
        }
        class.items.sort_by(|left, right| {
            let rank = |item: &ClassItem| match item { ClassItem::Literal(_) => 0, ClassItem::Range(_, _) => 1, ClassItem::Equivalent(_) => 2, ClassItem::Mask(_, _) => 3 };
            rank(left).cmp(&rank(right)).then_with(|| match (left, right) {
                (ClassItem::Literal(left), ClassItem::Literal(right)) | (ClassItem::Equivalent(left), ClassItem::Equivalent(right)) => left.cmp(right),
                _ => std::cmp::Ordering::Equal,
            })
        });
        class.items.dedup_by(|left, right| match (left, right) {
            (ClassItem::Literal(left), ClassItem::Literal(right)) | (ClassItem::Equivalent(left), ClassItem::Equivalent(right)) => left == right,
            _ => false,
        });
        Ok(self.push(Node::Class(class, self.flags.casei)))
    }
    fn class_literal(&mut self, first: bool) -> Result<Vec<u16>, String> {
        if self.take(92) { return Ok(vec![self.unescape_unit()?]); }
        if self.units.get(self.at..self.at + 2) == Some(&[91, 46]) {
            self.at += 2;
            let name = self.read_name(46)?;
            self.expect(93, "Incomplete collating element")?;
            return collating_name(&name).ok_or_else(|| self.error("Invalid collation character"));
        }
        let unit = self.peek().ok_or_else(|| self.error("Incomplete character class"))?;
        if unit == 45 && !first && self.units.get(self.at + 1) != Some(&93) { return Err(self.error("Invalid range end in character class")); }
        self.at += 1;
        Ok(vec![unit])
    }
}

#[derive(Clone)]
enum Instruction {
    Open { mark: i32, casei: bool },
    Close { mark: i32, casei: bool },
    Literal(Arc<Vec<u16>>),
    Any(bool),
    Class(Arc<UnitClass>),
    Anchor(Anchor),
    Jump(usize),
    Alt(usize),
    Repeat { lo: usize, hi: usize, greedy: bool, after: usize, id: i32, single: bool, leading: bool },
    Backstep(usize),
    Backref(i32),
    Assert(i32),
    Call { group: usize, target: usize },
    Combining,
    Case(bool),
    Verb(Verb),
    Match,
}

#[derive(Clone)]
struct StartMap { bytes: [u8; 256], nullable: u8 }

impl StartMap {
    fn empty() -> Self { Self { bytes: [0; 256], nullable: 0 } }
    fn merge(&mut self, other: Self) {
        for (left, right) in self.bytes.iter_mut().zip(other.bytes) { *left |= right; }
        self.nullable |= other.nullable;
    }
    fn all(&mut self, mask: u8, map: bool, nullable: bool) {
        if map { for byte in &mut self.bytes { *byte |= mask; } self.bytes[0] |= 4; }
        if nullable { self.nullable |= mask; }
    }
    fn can_start(&self, unit: Option<u16>, mask: u8) -> bool {
        match unit { Some(unit) if unit >= 256 => true, Some(unit) => self.bytes[usize::from(unit)] & mask != 0, None => self.nullable & mask != 0 }
    }
}

struct Program {
    instructions: Vec<Instruction>,
    maps: Vec<StartMap>,
    root_map: StartMap,
    groups: Vec<usize>,
    restart: Restart,
    initial_case: bool,
    case_at: Vec<bool>,
    call_returns: Vec<Option<usize>>,
}

struct ProgramBuilder<'a> {
    nodes: &'a [Node],
    traits: &'a UnitTraits,
    names: &'a BTreeMap<u32, Vec<usize>>,
    instructions: Vec<Instruction>,
    groups: Vec<usize>,
    casei: bool,
    repeat_id: i32,
    calls: Vec<(usize, Reference)>,
}

enum CompileAction {
    Node(usize),
    Close(i32),
    MarkedEnd { mark: i32, jump: usize },
    RepeatEnd(usize),
    Alt { node: usize, branch: usize, jumps: Vec<usize> },
    AltNext { node: usize, branch: usize, jumps: Vec<usize>, alt: usize },
    AltEnd(Vec<usize>),
    ConditionalMiddle { alt: usize, no: usize },
    ConditionalEnd(usize),
}

impl Program {
    fn compile(nodes: &[Node], root: usize, groups: usize, names: &BTreeMap<u32, Vec<usize>>, traits: &UnitTraits, casei: bool) -> Result<Self, String> {
        let mut builder = ProgramBuilder { nodes, traits, names, instructions: Vec::new(), groups: vec![usize::MAX; groups], casei, repeat_id: 0, calls: Vec::new() };
        builder.groups[0] = builder.emit(Instruction::Open { mark: 0, casei });
        builder.node(root)?;
        builder.emit(Instruction::Match);
        for (pc, reference) in &builder.calls {
            let group = match reference {
                Reference::Number(group) if *group < 0x40000000 => Some(*group),
                Reference::Number(hash) => builder.names.get(&(*hash as u32)).and_then(|groups| groups.first().copied()),
                Reference::Name(name) => builder.names.get(&capture_name_hash(name)).and_then(|groups| groups.first().copied()),
            }.filter(|&group| group < builder.groups.len() && builder.groups[group] != usize::MAX)
                .ok_or_else(|| "Encountered a forward reference to a recursive sub-expression that does not exist.".to_owned())?;
            builder.instructions[*pc] = Instruction::Call { group, target: builder.groups[group] };
        }
        if !builder.calls.is_empty() {
            for instruction in &builder.instructions {
                if let Instruction::Assert(index) = instruction {
                    if *index < 0 {
                        let group = index.wrapping_add(1).wrapping_neg();
                        if group >= 0x40000000 && !names.contains_key(&(group as u32)) { return Err("Encountered a forward reference to a marked sub-expression that does not exist.".to_owned()); }
                    }
                }
            }
        }
        let mut current_case = casei;
        let case_at = builder.instructions.iter().map(|instruction| { let before = current_case; if let Instruction::Case(casei) = instruction { current_case = *casei; } before }).collect();
        let mut call_returns = vec![None; builder.groups.len()];
        for (pc, instruction) in builder.instructions.iter().enumerate() { if let Instruction::Call { group, .. } = instruction { call_returns[*group].get_or_insert(pc + 1); } }
        let mut program = Self { maps: vec![StartMap::empty(); builder.instructions.len()], instructions: builder.instructions, root_map: StartMap::empty(), groups: builder.groups, restart: Restart::Any, initial_case: casei, case_at, call_returns };
        program.create_startmaps(traits)?;
        let root_map = MapGenerator::new(&program, traits, casei).generate(0, 3, true, true, 0)?;
        program.root_map = root_map;
        for instruction in &program.instructions {
            match instruction {
                Instruction::Open { .. } | Instruction::Close { .. } => continue,
                Instruction::Anchor(Anchor::StartLine) => program.restart = Restart::Line,
                Instruction::Anchor(Anchor::WordStart) => program.restart = Restart::Word,
                Instruction::Anchor(Anchor::StartText) => program.restart = Restart::Buffer,
                Instruction::Anchor(Anchor::Continue) => program.restart = Restart::Continue,
                _ => {}
            }
            break;
        }
        if !program.instructions.iter().any(|instruction| matches!(instruction, Instruction::Backref(_))) {
            let mut pc = 0;
            while pc < program.instructions.len() {
                match program.instructions[pc] {
                    Instruction::Open { mark: -1 | -2, .. } => {
                        let end = program.jump(pc + 1)?;
                        pc = end + 1;
                    }
                    Instruction::Open { mark: -3, .. } => pc += 2,
                    Instruction::Open { mark, .. } if mark >= 0 => pc += 1,
                    Instruction::Close { .. } | Instruction::Anchor(_) => pc += 1,
                    Instruction::Repeat { single: true, .. } => {
                        if let Instruction::Repeat { leading, .. } = &mut program.instructions[pc] { *leading = true; }
                        break;
                    }
                    _ => break,
                }
            }
        }
        Ok(program)
    }
    fn jump(&self, pc: usize) -> Result<usize, String> {
        match self.instructions.get(pc) { Some(Instruction::Jump(target)) => Ok(*target), _ => Err("Invalid regex jump state".to_owned()) }
    }
    fn create_startmaps(&mut self, traits: &UnitTraits) -> Result<(), String> {
        for pc in 0..self.instructions.len() {
            if matches!(self.instructions[pc], Instruction::Backstep(_)) {
                let length = self.calculate_backstep(pc + 1)?;
                self.instructions[pc] = Instruction::Backstep(length);
            }
        }
        for pc in (0..self.instructions.len()).rev() {
            let target = match self.instructions[pc] {
                Instruction::Alt(target) => target,
                Instruction::Repeat { after, .. } => after,
                _ => continue,
            };
            let take = MapGenerator::new(self, traits, self.case_at[pc]).generate(pc + 1, 1, true, true, 0)?;
            let skip = MapGenerator::new(self, traits, self.case_at[pc]).generate(target, 2, true, true, 0)?;
            let mut map = take;
            map.merge(skip);
            self.maps[pc] = map;
            if let Instruction::Repeat { after, .. } = self.instructions[pc] {
                if after == pc + 3 && matches!(self.instructions.get(pc + 2), Some(Instruction::Jump(target)) if *target == pc) {
                    let single = match self.instructions.get(pc + 1) {
                        Some(Instruction::Literal(units)) => units.len() == 1,
                        Some(Instruction::Any(_)) => true,
                        Some(Instruction::Class(class)) => class.singleton(),
                        _ => false,
                    };
                    if let Instruction::Repeat { single: value, .. } = &mut self.instructions[pc] { *value = single; }
                }
            }
        }
        Ok(())
    }
    fn calculate_backstep(&mut self, mut pc: usize) -> Result<usize, String> {
        let error = || "Invalid lookbehind assertion encountered in the regular expression.".to_owned();
        let mut length = 0usize;
        let mut previous = None;
        let mut alternatives = Vec::new();
        while let Some(instruction) = self.instructions.get(pc).cloned() {
            match instruction {
                Instruction::Open { mark: -1 | -2, .. } => { pc = self.jump(pc + 1)? + 1; continue; }
                Instruction::Open { mark: -3, .. } => { pc += 2; continue; }
                Instruction::Close { mark: -1 | -2, .. } => {
                    if previous.is_some_and(|value| value != length) { return Err(error()); }
                    previous = Some(length);
                    if let Some((saved, target)) = alternatives.pop() { length = saved; pc = target; continue; }
                    return Ok(length);
                }
                Instruction::Literal(units) => length = length.checked_add(units.len()).filter(|&length| length <= i32::MAX as usize).ok_or_else(error)?,
                Instruction::Any(_) => length += 1,
                Instruction::Class(class) => { if !class.singleton() { return Err(error()); } length += 1; }
                Instruction::Repeat { lo, hi, after, .. } => {
                    let single = after == pc + 3 && matches!(self.instructions.get(pc + 2), Some(Instruction::Jump(target)) if *target == pc)
                        && match self.instructions.get(pc + 1) { Some(Instruction::Any(_)) => true, Some(Instruction::Literal(units)) => units.len() == 1, Some(Instruction::Class(class)) => class.singleton(), _ => false };
                    if !single || lo != hi { return Err(error()); }
                    if let Instruction::Repeat { single, .. } = &mut self.instructions[pc] { *single = true; }
                    length = length.checked_add(lo).filter(|&length| length <= i32::MAX as usize).ok_or_else(error)?;
                    pc = after;
                    continue;
                }
                Instruction::Backref(_) | Instruction::Combining | Instruction::Backstep(_) => return Err(error()),
                Instruction::Jump(target) => { pc = target; continue; }
                Instruction::Alt(target) => { if alternatives.len() > 1024 { return Err(error()); } alternatives.push((length, target)); }
                _ => {}
            }
            pc += 1;
        }
        Err(error())
    }
}

impl ProgramBuilder<'_> {
    fn emit(&mut self, instruction: Instruction) -> usize {
        let pc = self.instructions.len();
        self.instructions.push(instruction);
        pc
    }
    fn case(&mut self, casei: bool, force: bool) {
        if force || self.casei != casei { self.emit(Instruction::Case(casei)); self.casei = casei; }
    }
    fn class(&self, class: &UnitClass) -> Arc<UnitClass> {
        let mut class = class.clone();
        for item in &mut class.items {
            match item {
                ClassItem::Literal(units) => {
                    for unit in units { *unit = self.traits.translate(*unit, self.casei); }
                }
                ClassItem::Equivalent(_) => {}
                ClassItem::Range(start, end) => for unit in start.iter_mut().chain(end) { *unit = self.traits.translate(*unit, self.casei); },
                ClassItem::Mask(mask, _) => if self.casei && *mask & 3 != 0 { *mask |= 0x0100; },
            }
        }
        Arc::new(class)
    }
    fn node(&mut self, index: usize) -> Result<(), String> {
        let nodes = self.nodes;
        let mut actions = vec![CompileAction::Node(index)];
        while let Some(action) = actions.pop() {
            match action {
                CompileAction::Close(mark) => { self.emit(Instruction::Close { mark, casei: self.casei }); }
                CompileAction::MarkedEnd { mark, jump } => {
                    let end = self.emit(Instruction::Close { mark, casei: self.casei });
                    self.instructions[jump] = Instruction::Jump(end);
                }
                CompileAction::RepeatEnd(repeat) => {
                    self.emit(Instruction::Jump(repeat));
                    let end = self.instructions.len();
                    if let Instruction::Repeat { after, .. } = &mut self.instructions[repeat] { *after = end; }
                }
                CompileAction::Alt { node, branch, jumps } => {
                    let Node::Alt(children) = &nodes[node] else { return Err("Invalid regex compiler alternative".to_owned()); };
                    let child = *children.get(branch).ok_or_else(|| "Invalid regex compiler alternative branch".to_owned())?;
                    if branch + 1 == children.len() {
                        actions.push(CompileAction::AltEnd(jumps));
                    } else {
                        let alt = self.emit(Instruction::Alt(usize::MAX));
                        actions.push(CompileAction::AltNext { node, branch, jumps, alt });
                    }
                    actions.push(CompileAction::Node(child));
                }
                CompileAction::AltNext { node, branch, mut jumps, alt } => {
                    jumps.push(self.emit(Instruction::Jump(usize::MAX)));
                    self.instructions[alt] = Instruction::Alt(self.instructions.len());
                    actions.push(CompileAction::Alt { node, branch: branch + 1, jumps });
                }
                CompileAction::AltEnd(jumps) => {
                    let end = self.instructions.len();
                    for jump in jumps { self.instructions[jump] = Instruction::Jump(end); }
                }
                CompileAction::ConditionalMiddle { alt, no } => {
                    let jump = self.emit(Instruction::Jump(usize::MAX));
                    self.instructions[alt] = Instruction::Alt(self.instructions.len());
                    actions.push(CompileAction::ConditionalEnd(jump));
                    actions.push(CompileAction::Node(no));
                }
                CompileAction::ConditionalEnd(jump) => {
                    let end = self.emit(Instruction::Close { mark: -4, casei: self.casei });
                    self.instructions[jump] = Instruction::Jump(end);
                }
                CompileAction::Node(index) => match &nodes[index] {
                    Node::Empty => {}
                    Node::Unit(unit, casei) => {
                        self.case(*casei, false);
                        let unit = self.traits.translate(*unit, self.casei);
                        if let Some(Instruction::Literal(units)) = self.instructions.last_mut() { Arc::make_mut(units).push(unit); }
                        else { self.emit(Instruction::Literal(Arc::new(vec![unit]))); }
                    }
                    Node::Any(newline) => { self.emit(Instruction::Any(*newline)); }
                    Node::Class(class, casei) => { self.case(*casei, false); self.emit(Instruction::Class(self.class(class))); }
                    Node::Concat(children) => actions.extend(children.iter().rev().copied().map(CompileAction::Node)),
                    Node::Scope(child) => {
                        self.emit(Instruction::Open { mark: 0, casei: self.casei });
                        actions.push(CompileAction::Close(0));
                        actions.push(CompileAction::Node(*child));
                    }
                    Node::Group(group, child) => {
                        let group = i32::try_from(*group).map_err(|_| "Regular expression is too large".to_owned())?;
                        let pc = self.emit(Instruction::Open { mark: group, casei: self.casei });
                        if self.groups[group as usize] == usize::MAX { self.groups[group as usize] = pc; }
                        actions.push(CompileAction::Close(group));
                        actions.push(CompileAction::Node(*child));
                    }
                    Node::Alt(_) => actions.push(CompileAction::Alt { node: index, branch: 0, jumps: Vec::new() }),
                    Node::Repeat { child, lo, hi, greedy } => {
                        let id = self.repeat_id;
                        self.repeat_id = self.repeat_id.checked_add(1).ok_or_else(|| "Regular expression is too large".to_owned())?;
                        let repeat = self.emit(Instruction::Repeat { lo: *lo, hi: *hi, greedy: *greedy, after: usize::MAX, id, single: false, leading: false });
                        actions.push(CompileAction::RepeatEnd(repeat));
                        actions.push(CompileAction::Node(*child));
                    }
                    Node::Look { child, positive, behind } => {
                        let mark = if *positive { -1 } else { -2 };
                        self.emit(Instruction::Open { mark, casei: self.casei });
                        let jump = self.emit(Instruction::Jump(usize::MAX));
                        if let Some(length) = behind { self.emit(Instruction::Backstep(*length)); }
                        actions.push(CompileAction::MarkedEnd { mark, jump });
                        actions.push(CompileAction::Node(*child));
                    }
                    Node::Atomic(child) => {
                        self.emit(Instruction::Open { mark: -3, casei: self.casei });
                        let jump = self.emit(Instruction::Jump(usize::MAX));
                        actions.push(CompileAction::MarkedEnd { mark: -3, jump });
                        actions.push(CompileAction::Node(*child));
                    }
                    Node::Backref(reference, casei) => { self.case(*casei, false); self.emit(Instruction::Backref(reference_index(reference))); }
                    Node::Call(reference) => {
                        self.emit(Instruction::Open { mark: 0, casei: self.casei });
                        let pc = self.emit(Instruction::Call { group: 0, target: usize::MAX });
                        self.calls.push((pc, reference.clone()));
                        self.case(self.casei, true);
                        self.emit(Instruction::Close { mark: 0, casei: self.casei });
                    }
                    Node::Conditional(condition, yes, no) => {
                        self.emit(Instruction::Open { mark: -4, casei: self.casei });
                        let alt = self.emit(Instruction::Alt(usize::MAX));
                        actions.push(CompileAction::ConditionalMiddle { alt, no: *no });
                        actions.push(CompileAction::Node(*yes));
                        match condition {
                            Condition::Assertion(assertion) => actions.push(CompileAction::Node(*assertion)),
                            Condition::Define => { self.emit(Instruction::Assert(9999)); }
                            Condition::Capture(reference) => { self.emit(Instruction::Assert(reference_index(reference))); }
                            Condition::Recursion(reference) => { self.emit(Instruction::Assert(reference.as_ref().map_or(0, |reference| reference_index(reference).wrapping_neg().wrapping_sub(1)))); }
                        }
                    }
                    Node::Anchor(anchor) => { self.emit(Instruction::Anchor(*anchor)); }
                    Node::KeepOut => { self.emit(Instruction::Open { mark: -5, casei: self.casei }); }
                    Node::Newline => self.newline()?,
                    Node::Combining(casei) => { self.case(*casei, false); self.emit(Instruction::Combining); }
                    Node::Case(casei) => self.case(*casei, true),
                    Node::Verb(verb) => { self.emit(Instruction::Verb(*verb)); }
                },
            }
        }
        Ok(())
    }
    fn newline(&mut self) -> Result<(), String> {
        self.emit(Instruction::Open { mark: 0, casei: self.casei });
        self.emit(Instruction::Open { mark: -3, casei: self.casei });
        let end_jump = self.emit(Instruction::Jump(usize::MAX));
        let alt = self.emit(Instruction::Alt(usize::MAX));
        self.emit(Instruction::Literal(Arc::new(vec![13])));
        let repeat = self.emit(Instruction::Repeat { lo: 0, hi: 1, greedy: true, after: usize::MAX, id: self.repeat_id, single: false, leading: false });
        self.repeat_id = self.repeat_id.checked_add(1).ok_or_else(|| "Regular expression is too large".to_owned())?;
        self.emit(Instruction::Literal(Arc::new(vec![10])));
        self.emit(Instruction::Jump(repeat));
        let after = self.instructions.len();
        if let Instruction::Repeat { after: target, .. } = &mut self.instructions[repeat] { *target = after; }
        let jump = self.emit(Instruction::Jump(usize::MAX));
        self.instructions[alt] = Instruction::Alt(self.instructions.len());
        let class = UnitClass { negated: false, items: [10, 11, 12, 0x85, 0x2028, 0x2029].into_iter().map(|unit| ClassItem::Literal(vec![unit])).collect() };
        self.emit(Instruction::Class(Arc::new(class)));
        let end = self.emit(Instruction::Close { mark: -3, casei: self.casei });
        self.instructions[jump] = Instruction::Jump(end);
        self.instructions[end_jump] = Instruction::Jump(end);
        self.emit(Instruction::Close { mark: 0, casei: self.casei });
        Ok(())
    }
}

struct MapGenerator<'a> {
    program: &'a Program,
    traits: &'a UnitTraits,
    entry_case: bool,
    recursion_checks: Vec<u8>,
    bad_repeats: u64,
}

impl<'a> MapGenerator<'a> {
    fn new(program: &'a Program, traits: &'a UnitTraits, entry_case: bool) -> Self {
        Self { program, traits, entry_case, recursion_checks: vec![0; program.groups.len()], bad_repeats: 0 }
    }
    fn generate(&mut self, pc: usize, mask: u8, map: bool, nullable: bool, depth: usize) -> Result<StartMap, String> {
        let saved = self.recursion_checks.clone();
        let result = self.generate_inner(pc, mask, map, nullable, depth);
        self.recursion_checks = saved;
        result
    }
    fn generate_inner(&mut self, mut pc: usize, mask: u8, map: bool, nullable: bool, depth: usize) -> Result<StartMap, String> {
        if depth > 100 { return Err("Expression complexity exceeded.".to_owned()); }
        let mut result = StartMap::empty();
        let mut casei = self.entry_case;
        let mut not_last_jump = 1i32;
        let mut recursion = None::<(usize, usize)>;
        let program = self.program;
        while let Some(instruction) = program.instructions.get(pc) {
            match instruction {
                Instruction::Case(value) => casei = *value,
                Instruction::Literal(units) => {
                    if map {
                        result.bytes[0] |= 4;
                        if let Some(&first) = units.first() {
                            for value in 0..256 { if self.traits.translate(value as u16, casei) == first { result.bytes[value] |= mask; } }
                        }
                    }
                    return Ok(result);
                }
                Instruction::Anchor(Anchor::EndLine) => {
                    if map { result.bytes[0] |= 4; for unit in [10, 13, 12, 0x85] { result.bytes[unit] |= mask; } }
                    if nullable { result.merge(self.generate(pc + 1, mask, false, true, depth + 1)?); }
                    return Ok(result);
                }
                Instruction::Any(_) => { result.all(mask, map, false); return Ok(result); }
                Instruction::Backref(_) => { result.all(mask, map, nullable); return Ok(result); }
                Instruction::Verb(Verb::Accept) | Instruction::Match => { result.all(mask, map, nullable); return Ok(result); }
                Instruction::Anchor(Anchor::WordStart | Anchor::WordEnd) => {
                    let start = matches!(instruction, Instruction::Anchor(Anchor::WordStart));
                    result.merge(self.generate(pc + 1, mask, map, nullable, depth + 1)?);
                    if map {
                        result.bytes[0] |= 4;
                        for value in 0..256 { if self.traits.has(value as u16, 0x0504) != start { result.bytes[value] &= !mask; } }
                    }
                    return Ok(result);
                }
                Instruction::Anchor(Anchor::EndText) => { if nullable { result.nullable |= mask; } return Ok(result); }
                Instruction::Class(class) => {
                    if map {
                        if class.singleton() {
                            result.bytes[0] |= 4;
                            for value in 0..256 {
                                if class_single(class, self.traits.translate(value as u16, casei), self.traits)? { result.bytes[value] |= mask; }
                            }
                        } else { result.all(mask, true, false); }
                    }
                    return Ok(result);
                }
                Instruction::Jump(target) => { pc = *target; not_last_jump = -1; not_last_jump += 1; continue; }
                Instruction::Alt(target) | Instruction::Repeat { after: target, .. } => {
                    let cache = &program.maps[pc];
                    if cache.bytes[0] & 4 != 0 {
                        if map { result.bytes[0] |= 4; for value in 0..256 { if cache.bytes[value] & 3 != 0 { result.bytes[value] |= mask; } } }
                        if nullable && cache.nullable & 3 != 0 { result.nullable |= mask; }
                    } else {
                        let bad = match instruction { Instruction::Repeat { id, .. } => *id >= 64 || self.bad_repeats & (1u64 << *id as u32) != 0, _ => false };
                        if bad { result.all(mask, map, nullable); return Ok(result); }
                        if let Instruction::Repeat { id, .. } = instruction { self.bad_repeats |= 1u64 << *id as u32; }
                        let target = *target;
                        let skip = matches!(instruction, Instruction::Alt(_)) || matches!(instruction, Instruction::Repeat { lo: 0, .. }) || not_last_jump == 0;
                        result.merge(self.generate(pc + 1, mask, map, nullable, depth + 1)?);
                        if skip { result.merge(self.generate(target, mask, map, nullable, depth + 1)?); }
                    }
                    return Ok(result);
                }
                Instruction::Anchor(Anchor::SoftEnd) => {
                    if map { result.bytes[0] |= 4; for unit in [10, 13] { result.bytes[unit] |= mask; } }
                    if nullable { result.nullable |= mask; }
                    return Ok(result);
                }
                Instruction::Close { mark, .. } if *mark < 0 => { result.all(mask, map, nullable); return Ok(result); }
                Instruction::Close { mark, .. } => {
                    let group = *mark as usize;
                    if let Some((active, return_pc)) = recursion {
                        if active != 0 && active == group { recursion = None; pc = return_pc; continue; }
                    }
                    if group != 0 && self.recursion_checks[group] & 2 == 0 {
                        if let Some(return_pc) = program.call_returns[group] {
                            self.recursion_checks[group] |= 2;
                            result.merge(self.generate(return_pc, mask, map, nullable, depth + 1)?);
                        }
                    }
                }
                Instruction::Call { group, target } => {
                    if self.recursion_checks[*group] & 1 != 0 { return Err("Encountered an infinite recursion.".to_owned()); }
                    self.recursion_checks[*group] |= 1;
                    if recursion.is_none() { recursion = Some((*group, pc + 1)); pc = *target; continue; }
                    result.all(mask, map, nullable);
                    return Ok(result);
                }
                Instruction::Verb(Verb::Commit | Verb::Skip | Verb::Prune) => result.all(mask, map, false),
                Instruction::Open { mark: -3, .. } => { pc += 2; continue; }
                _ => {}
            }
            pc += 1;
            not_last_jump += 1;
        }
        Ok(result)
    }
}

fn class_single(class: &UnitClass, col: u16, traits: &UnitTraits) -> Result<bool, String> {
    let mut positive = 0;
    let mut negative = 0;
    let mut matched = false;
    for item in &class.items {
        match item {
            ClassItem::Literal(units) => matched |= units.as_slice() == [col].as_slice(),
            ClassItem::Range(start, end) => matched |= start.as_slice() <= [col].as_slice() && [col].as_slice() <= end.as_slice(),
            ClassItem::Equivalent(units) => matched |= primary_equal(&[col], units)?,
            ClassItem::Mask(mask, negated) => if *negated { negative |= *mask; } else { positive |= *mask; },
        }
    }
    matched |= traits.has(col, positive) || negative != 0 && !traits.has(col, negative);
    Ok(matched != class.negated)
}

#[derive(Clone)]
struct Cursor<'a>(crate::regex_input::Utf8DocumentIterator<'a>);

impl Cursor<'_> {
    fn offset(&self) -> usize {
        self.0.position()
    }
}

#[derive(Clone, Copy)]
struct ExecutionInput<'a> { bytes: &'a crate::byte_buffer::ByteBuffer, end: usize }

impl<'a> ExecutionInput<'a> {
    fn begin_cursor(&self) -> Result<Cursor<'a>, String> {
        self.cursor(0)
    }
    fn end_cursor(&self) -> Result<Cursor<'a>, String> {
        self.cursor(self.end)
    }
    fn cursor(&self, position: usize) -> Result<Cursor<'a>, String> {
        Ok(Cursor(crate::regex_input::Utf8DocumentIterator::new(self.bytes, position, self.end)?))
    }
    fn at_end(&self, cursor: &Cursor<'_>) -> bool {
        cursor.0.at_end()
    }
    fn value(&self, cursor: &Cursor<'_>) -> Option<u16> {
        if self.at_end(cursor) { return None; }
        Some(cursor.0.value())
    }
    fn next(&self, cursor: &mut Cursor<'a>) -> Result<(), String> {
        cursor.0.next()
    }
    fn previous(&self, cursor: &mut Cursor<'a>) -> Result<(), String> {
        cursor.0.previous()
    }
    fn same(&self, left: &Cursor<'_>, right: &Cursor<'_>) -> bool {
        left.0.position() == right.0.position() && left.0.at_end() == right.0.at_end()
    }
    fn copy_units(&self, start: &Cursor<'a>, end: &Cursor<'a>) -> Vec<u16> {
        let mut cursor = start.clone();
        let mut units = Vec::new();
        while !self.same(&cursor, end) {
            let Some(unit) = self.value(&cursor) else { break; };
            units.push(unit);
            cursor.0.next_unit();
        }
        units
    }
    fn literal_end(&self, start: &Cursor<'a>, units: &[u16], traits: &UnitTraits, casei: bool) -> Result<Option<Cursor<'a>>, String> {
        let mut cursor = start.clone();
        for &unit in units {
            if !self.value(&cursor).is_some_and(|input| traits.translate(input, casei) == unit) { return Ok(None); }
            self.next(&mut cursor)?;
        }
        Ok(Some(cursor))
    }
}

#[derive(Clone)]
struct GroupState<'a> { start: Cursor<'a>, end: Cursor<'a>, matched: bool }

#[derive(Clone)]
struct CaptureState<'a> { groups: Vec<GroupState<'a>>, last_closed: Option<usize> }

#[derive(Clone)]
struct SourceCall<'a> { group: usize, return_pc: usize, results: CaptureState<'a>, start: Cursor<'a> }

struct SourceCounter<'a> { id: i32, count: usize, start: Cursor<'a>, next: Option<usize> }

enum Saved<'a> {
    End,
    Stop,
    Parent(usize, GroupState<'a>),
    Assertion { pc: usize, position: Cursor<'a>, positive: bool },
    Alternative { pc: usize, position: Cursor<'a> },
    Counter(SourceCounter<'a>),
    Extra,
    Single { pc: usize, position: Cursor<'a>, count: usize, greedy: bool },
    NonGreedy { pc: usize, position: Cursor<'a> },
    Recursion { group: usize, pc: usize, internal: CaptureState<'a>, prior: CaptureState<'a> },
    RecursionPop,
    Commit,
    Then,
    Case(bool),
}

enum UnwindAction {
    Begin(bool),
    Drain,
    CommitAfter(bool),
    ThenAfter(bool),
    ThenFinish,
}

impl Saved<'_> {
    fn source_size(&self) -> usize {
        match self {
            Self::End | Self::Stop | Self::RecursionPop | Self::Commit | Self::Then => 8,
            Self::Parent(_, _) => 120,
            Self::Assertion { .. } => 72,
            Self::Alternative { .. } | Self::NonGreedy { .. } => 64,
            Self::Counter(_) => 88,
            Self::Extra => 24,
            Self::Single { .. } => 72,
            Self::Recursion { .. } => 424,
            Self::Case(_) => 16,
        }
    }
}

struct SourceStack<'a> { states: Vec<Saved<'a>>, used: Vec<usize>, extensions_left: usize }

impl<'a> SourceStack<'a> {
    fn new() -> Self { Self { states: vec![Saved::End], used: vec![8], extensions_left: 1024 } }
    fn push(&mut self, state: Saved<'a>) -> Result<usize, String> {
        let size = state.source_size();
        let used = self.used.last().copied().ok_or_else(|| "Invalid regex stack block".to_owned())?;
        if used + size > 4096 {
            if self.extensions_left == 0 { return Err("Ran out of stack space trying to match the regular expression.".to_owned()); }
            self.extensions_left -= 1;
            self.used.push(24);
            self.states.push(Saved::Extra);
        }
        let block = self.used.last_mut().ok_or_else(|| "Invalid regex stack block".to_owned())?;
        *block += size;
        let index = self.states.len();
        self.states.push(state);
        Ok(index)
    }
    fn pop(&mut self) -> Result<Saved<'a>, String> {
        if self.states.len() == 1 { return Ok(Saved::End); }
        let state = self.states.pop().ok_or_else(|| "Invalid regex stack state".to_owned())?;
        if matches!(state, Saved::Extra) {
            self.used.pop();
            self.extensions_left += 1;
        } else {
            let block = self.used.last_mut().ok_or_else(|| "Invalid regex stack block".to_owned())?;
            *block = block.checked_sub(state.source_size()).ok_or_else(|| "Invalid regex stack allocation".to_owned())?;
        }
        Ok(state)
    }
    fn at_end(&self) -> bool { self.states.len() == 1 }
    fn counter(&self, index: usize) -> Result<&SourceCounter<'a>, String> {
        match self.states.get(index) { Some(Saved::Counter(counter)) => Ok(counter), _ => Err("Invalid regex repeat counter".to_owned()) }
    }
    fn counter_mut(&mut self, index: usize) -> Result<&mut SourceCounter<'a>, String> {
        match self.states.get_mut(index) { Some(Saved::Counter(counter)) => Ok(counter), _ => Err("Invalid regex repeat counter".to_owned()) }
    }
}

struct SourceVm<'m, 'i> {
    engine: Engine<'m, 'i>,
    position: Cursor<'i>,
    restart: Cursor<'i>,
    results: CaptureState<'i>,
    pc: Option<usize>,
    stack: SourceStack<'i>,
    counter: Option<usize>,
    calls: Vec<SourceCall<'i>>,
    casei: bool,
    independent: bool,
    depth: usize,
    recursive_result: bool,
    unwound_assertion: bool,
    unwound_alternative: bool,
    found: bool,
    not_null: bool,
    not_initial_null: bool,
}

impl<'m, 'i> SourceVm<'m, 'i> {
    fn new(matcher: &'m Matcher, input: ExecutionInput<'i>, first: Cursor<'i>, not_null: bool, not_initial_null: bool) -> Result<Self, String> {
        let end = input.end_cursor()?;
        Ok(Self {
            engine: Engine { matcher, input, search_start: first.clone(), visited: 0 },
            position: first.clone(), restart: first,
            results: CaptureState { groups: vec![GroupState { start: end.clone(), end, matched: false }; matcher.program.groups.len()], last_closed: None },
            pc: None, stack: SourceStack::new(), counter: None, calls: Vec::new(), casei: matcher.program.initial_case,
            independent: false, depth: 0, recursive_result: false, unwound_assertion: false, unwound_alternative: false, found: false, not_null, not_initial_null,
        })
    }
    fn find(&mut self) -> Result<bool, String> {
        let mut first = true;
        loop {
            if matches!(self.engine.matcher.program.restart, Restart::Any) {
                while !self.engine.input.at_end(&self.position) && !self.engine.matcher.program.root_map.can_start(self.engine.input.value(&self.position), 3) { self.engine.input.next(&mut self.position)?; }
                if self.engine.input.at_end(&self.position) && self.engine.matcher.program.root_map.nullable == 0 { return Ok(false); }
            } else if !first {
                while !self.engine.matcher.program.root_map.can_start(self.engine.input.value(&self.position), 3) {
                    if !self.engine.advance_candidate(&mut self.position)? { return Ok(false); }
                }
            }
            self.pc = Some(0);
            self.found = false;
            self.restart = self.position.clone();
            self.results.groups[0].start = self.position.clone();
            let end = self.engine.input.end_cursor()?;
            for group in self.results.groups.iter_mut().skip(1) { *group = GroupState { start: end.clone(), end: end.clone(), matched: false }; }
            self.run_all()?;
            if self.found { return Ok(true); }
            first = false;
            self.position = self.restart.clone();
            if !self.engine.advance_candidate(&mut self.position)? { return Ok(false); }
        }
    }
    fn run_all(&mut self) -> Result<bool, String> {
        self.depth += 1;
        if self.depth > 80 { self.depth -= 1; return Err(complexity_error()); }
        let result = self.run_loop();
        self.depth -= 1;
        result
    }
    fn run_loop(&mut self) -> Result<bool, String> {
        self.stack.push(Saved::Stop)?;
        loop {
            while let Some(pc) = self.pc {
                self.engine.visited = self.engine.visited.saturating_add(1);
                if !self.step(pc)? {
                    if self.engine.visited > 100_000_000 { return Err(complexity_error()); }
                    if !self.unwind(false)? { return Ok(self.recursive_result); }
                }
            }
            if !self.unwind(true)? { return Ok(self.recursive_result); }
        }
    }
    fn close(&mut self, group: usize) {
        self.results.groups[group].end = self.position.clone();
        self.results.groups[group].matched = true;
        if group != 0 { self.results.last_closed = Some(group); }
    }
    fn step(&mut self, pc: usize) -> Result<bool, String> {
        let instruction = self.engine.matcher.program.instructions.get(pc).cloned().ok_or_else(|| "Invalid regex instruction".to_owned())?;
        match instruction {
            Instruction::Open { mark, casei } => {
                self.casei = casei;
                match mark {
                    0 => self.pc = Some(pc + 1),
                    -1 | -2 => {
                        let next = self.engine.matcher.program.jump(pc + 1)? + 1;
                        self.stack.push(Saved::Assertion { pc: next, position: self.position.clone(), positive: mark == -1 })?;
                        self.pc = Some(pc + 2);
                    }
                    -3 => {
                        let old = self.independent;
                        self.independent = true;
                        let next = self.engine.matcher.program.jump(pc + 1)? + 1;
                        self.pc = Some(pc + 2);
                        let matched = self.run_all()?;
                        if !matched && !self.independent {
                            while !self.stack.at_end() { self.unwind(false)?; }
                            return Ok(false);
                        }
                        self.pc = Some(next);
                        self.independent = old;
                        return Ok(matched);
                    }
                    -4 => {
                        let alt = match self.engine.matcher.program.instructions.get(pc + 1) { Some(Instruction::Alt(target)) => *target, _ => return Err("Invalid regex conditional alternative".to_owned()) };
                        let condition_pc = pc + 2;
                        match self.engine.matcher.program.instructions.get(condition_pc).cloned() {
                            Some(Instruction::Assert(condition)) => {
                                self.pc = Some(if self.condition(condition) { condition_pc + 1 } else { alt });
                            }
                            Some(Instruction::Open { mark: assertion @ (-1 | -2), .. }) => {
                                let position = self.position.clone();
                                let next = self.engine.matcher.program.jump(condition_pc + 1)? + 1;
                                self.pc = Some(condition_pc + 2);
                                let mut matched = self.run_all()?;
                                self.position = position;
                                if assertion == -2 { matched = !matched; }
                                self.pc = Some(if matched { next } else { alt });
                            }
                            _ => return Err("Invalid regex conditional assertion".to_owned()),
                        }
                    }
                    -5 => {
                        self.stack.push(Saved::Parent(0, self.results.groups[0].clone()))?;
                        self.results.groups[0].start = self.position.clone();
                        self.pc = Some(pc + 1);
                    }
                    group if group > 0 => {
                        let group = group as usize;
                        self.stack.push(Saved::Parent(group, self.results.groups[group].clone()))?;
                        self.results.groups[group].start = self.position.clone();
                        self.pc = Some(pc + 1);
                    }
                    _ => return Err("Invalid regex marking group".to_owned()),
                }
            }
            Instruction::Close { mark, casei } => {
                self.casei = casei;
                if mark > 0 {
                    let group = mark as usize;
                    self.close(group);
                    if self.calls.last().is_some_and(|call| call.group == group) {
                        let call = self.calls.pop().ok_or_else(|| "Invalid regex recursion return".to_owned())?;
                        self.results = call.results.clone();
                        self.stack.push(Saved::Recursion { group, pc: call.return_pc, internal: self.results.clone(), prior: call.results })?;
                        self.push_counter((-2i32).checked_sub(mark).ok_or_else(|| "Regular expression is too large".to_owned())?)?;
                        self.pc = Some(call.return_pc + 1);
                        return Ok(true);
                    }
                } else if mark < 0 && mark != -4 { self.pc = None; return Ok(true); }
                self.pc = Some(pc + 1);
            }
            Instruction::Literal(units) => {
                for &unit in units.iter() {
                    if !self.engine.input.value(&self.position).is_some_and(|input| self.engine.matcher.traits.translate(input, self.casei) == unit) { return Ok(false); }
                    self.engine.input.next(&mut self.position)?;
                }
                self.pc = Some(pc + 1);
            }
            Instruction::Any(newline) => {
                if !self.engine.input.value(&self.position).is_some_and(|unit| newline || !separator(unit)) { return Ok(false); }
                self.engine.input.next(&mut self.position)?;
                self.pc = Some(pc + 1);
            }
            Instruction::Class(class) => {
                let Some(end) = self.engine.class_end(&class, self.casei, &self.position)? else { return Ok(false); };
                if self.engine.input.same(&end, &self.position) { return Ok(false); }
                self.position = end;
                self.pc = Some(pc + 1);
            }
            Instruction::Anchor(anchor) => { if !self.engine.anchor(anchor, &self.position)? { return Ok(false); } self.pc = Some(pc + 1); }
            Instruction::Jump(target) => self.pc = Some(target),
            Instruction::Alt(target) => {
                let map = &self.engine.matcher.program.maps[pc];
                let unit = self.engine.input.value(&self.position);
                let take = map.can_start(unit, 1);
                let skip = map.can_start(unit, 2);
                if take { if skip { self.stack.push(Saved::Alternative { pc: target, position: self.position.clone() })?; } self.pc = Some(pc + 1); }
                else if skip { self.pc = Some(target); }
                else { return Ok(false); }
            }
            Instruction::Repeat { single: true, .. } => return self.single_repeat(pc),
            Instruction::Repeat { lo, hi, greedy, after, id, .. } => {
                let current = self.counter.filter(|&index| matches!(self.stack.states.last(), Some(Saved::Counter(counter)) if counter.id == id && self.stack.counter(index).is_ok_and(|counter| counter.id == id)));
                let counter = match current { Some(counter) => counter, None => self.push_counter(id)? };
                let position = self.position.clone();
                let stalled = { let count = self.stack.counter_mut(counter)?; let stalled = count.count != 0 && self.engine.input.same(&count.start, &position); if stalled { count.count = hi; } else { count.start = position; } stalled };
                let count = self.stack.counter(counter)?.count;
                let map = &self.engine.matcher.program.maps[pc];
                let unit = self.engine.input.value(&self.position);
                let take = map.can_start(unit, 1);
                let skip = map.can_start(unit, 2);
                if count < lo {
                    if !take { return Ok(false); }
                    self.stack.counter_mut(counter)?.count += 1;
                    self.pc = Some(pc + 1);
                } else if greedy {
                    if count < hi && take && !stalled {
                        if skip { self.stack.push(Saved::Alternative { pc: after, position: self.position.clone() })?; }
                        self.stack.counter_mut(counter)?.count += 1;
                        self.pc = Some(pc + 1);
                    } else if skip { self.pc = Some(after); } else { return Ok(false); }
                } else if skip {
                    if count < hi && take { self.stack.push(Saved::NonGreedy { pc: pc + 1, position: self.position.clone() })?; }
                    self.pc = Some(after);
                } else if count < hi && take {
                    self.stack.counter_mut(counter)?.count += 1;
                    self.pc = Some(pc + 1);
                } else { return Ok(false); }
            }
            Instruction::Backstep(length) => {
                let begin = self.engine.input.begin_cursor()?;
                for _ in 0..length { if self.engine.input.same(&begin, &self.position) { return Ok(false); } self.engine.input.previous(&mut self.position)?; }
                self.pc = Some(pc + 1);
            }
            Instruction::Backref(index) => {
                let group = if index >= 0x40000000 {
                    self.engine.matcher.named_groups.get(&(index as u32)).and_then(|groups| groups.iter().find_map(|&group| self.results.groups.get(group).filter(|group| group.matched).cloned()))
                } else { self.results.groups.get(index as usize).filter(|group| group.matched).cloned() };
                let Some(group) = group else { return Ok(false); };
                let mut cursor = group.start;
                while !self.engine.input.same(&cursor, &group.end) {
                    let Some(unit) = self.engine.input.value(&self.position) else { return Ok(false); };
                    if !self.engine.matcher.traits.equal(unit, cursor.0.value(), self.casei) { return Ok(false); }
                    self.engine.input.next(&mut self.position)?;
                    self.engine.input.next(&mut cursor)?;
                }
                self.pc = Some(pc + 1);
            }
            Instruction::Assert(condition) => { if !self.condition(condition) { return Ok(false); } self.pc = Some(pc + 1); }
            Instruction::Call { group, target } => {
                if self.calls.iter().rev().find(|call| call.group == group).is_some_and(|call| self.engine.input.same(&call.start, &self.position)) { return Ok(false); }
                self.stack.push(Saved::RecursionPop)?;
                self.calls.push(SourceCall { group, return_pc: pc + 1, results: self.results.clone(), start: self.position.clone() });
                let group_index = i32::try_from(group).map_err(|_| "Invalid regex recursion group".to_owned())?;
                self.push_counter((-2i32).checked_sub(group_index).ok_or_else(|| "Regular expression is too large".to_owned())?)?;
                self.pc = Some(target);
            }
            Instruction::Combining => {
                if self.engine.input.value(&self.position).is_none_or(|unit| combining(self.engine.matcher.traits.translate(unit, self.casei))) { return Ok(false); }
                self.engine.input.next(&mut self.position)?;
                while self.engine.input.value(&self.position).is_some_and(|unit| combining(self.engine.matcher.traits.translate(unit, self.casei))) { self.engine.input.next(&mut self.position)?; }
                self.pc = Some(pc + 1);
            }
            Instruction::Case(casei) => { self.stack.push(Saved::Case(self.casei))?; self.casei = casei; self.pc = Some(pc + 1); }
            Instruction::Verb(Verb::Fail) => return Ok(false),
            Instruction::Verb(Verb::Accept) => return self.skip_until(self.calls.last().map_or(i32::MAX, |call| call.group as i32), true),
            Instruction::Verb(verb @ (Verb::Commit | Verb::Skip | Verb::Prune)) => {
                if matches!(verb, Verb::Commit) { self.restart = self.engine.input.end_cursor()?; }
                else if matches!(verb, Verb::Skip) && !self.engine.input.same(&self.position, &self.engine.search_start) { self.restart = self.position.clone(); self.engine.input.previous(&mut self.restart)?; }
                self.stack.push(Saved::Commit)?;
                self.pc = Some(pc + 1);
            }
            Instruction::Verb(Verb::Then) => { self.stack.push(Saved::Then)?; self.pc = Some(pc + 1); }
            Instruction::Match => {
                if let Some(call) = self.calls.pop() {
                    if call.group != 0 { return Err("Invalid regex whole-pattern recursion return".to_owned()); }
                    self.stack.push(Saved::Recursion { group: 0, pc: call.return_pc, internal: self.results.clone(), prior: call.results.clone() })?;
                    self.results = call.results;
                    self.pc = Some(call.return_pc);
                } else {
                    if self.not_null && self.engine.input.same(&self.position, &self.results.groups[0].start) || self.not_initial_null && self.engine.input.same(&self.position, &self.engine.search_start) { return Ok(false); }
                    self.close(0);
                    self.found = true;
                    self.pc = None;
                }
            }
        }
        Ok(true)
    }
    fn condition(&self, index: i32) -> bool {
        if index == 9999 { return false; }
        if index > 0 {
            if index >= 0x40000000 { self.engine.matcher.named_groups.get(&(index as u32)).is_some_and(|groups| groups.iter().any(|&group| self.results.groups.get(group).is_some_and(|group| group.matched))) }
            else { self.results.groups.get(index as usize).is_some_and(|group| group.matched) }
        } else {
            self.calls.last().is_some_and(|call| {
                let group = index.wrapping_add(1).wrapping_neg();
                if group >= 0x40000000 { self.engine.matcher.named_groups.get(&(group as u32)).is_some_and(|groups| groups.contains(&call.group)) }
                else { index == 0 || call.group == group as usize }
            })
        }
    }
    fn push_counter(&mut self, id: i32) -> Result<usize, String> {
        let current_id = self.counter.map(|index| self.stack.counter(index).map(|counter| counter.id)).transpose()?.unwrap_or(-1);
        let mut prior = None;
        if !(id > current_id && current_id >= 0) {
            let recursion_id = self.calls.last().map_or(i32::MIN + 3, |call| call.group as i32);
            let mut cursor = self.counter;
            while let Some(index) = cursor {
                let counter = self.stack.counter(index)?;
                if counter.id == id { prior = Some(index); break; }
                if counter.id == (-2i32).wrapping_sub(recursion_id) { break; }
                cursor = counter.next;
                if let Some(index) = cursor { let next = self.stack.counter(index)?; if next.id < 0 { cursor = next.next; } }
            }
        }
        let (count, start) = match prior {
            Some(index) => { let counter = self.stack.counter(index)?; (counter.count, counter.start.clone()) }
            None => (0, self.position.clone()),
        };
        let index = self.stack.push(Saved::Counter(SourceCounter { id, count, start, next: self.counter }))?;
        self.counter = Some(index);
        Ok(index)
    }
    fn single_match(&self, pc: usize, position: &Cursor<'i>) -> Result<bool, String> {
        let Some(unit) = self.engine.input.value(position) else { return Ok(false); };
        match self.engine.matcher.program.instructions.get(pc) {
            Some(Instruction::Literal(units)) if units.len() == 1 => Ok(self.engine.matcher.traits.translate(unit, self.casei) == units[0]),
            Some(Instruction::Any(newline)) => Ok(*newline || !separator(unit)),
            Some(Instruction::Class(class)) => Ok(self.engine.class_end(class, self.casei, position)?.is_some_and(|end| !self.engine.input.same(position, &end))),
            _ => Err("Invalid regex single-character repeat".to_owned()),
        }
    }
    fn single_repeat(&mut self, pc: usize) -> Result<bool, String> {
        let (lo, hi, greedy, after, leading) = match self.engine.matcher.program.instructions.get(pc) {
            Some(Instruction::Repeat { lo, hi, greedy, after, leading, .. }) => (*lo, *hi, *greedy, *after, *leading),
            _ => return Err("Invalid regex single-character repeat".to_owned()),
        };
        let mut count = 0;
        let desired = if greedy { hi } else { lo };
        while count < desired && self.single_match(pc + 1, &self.position)? { self.engine.input.next(&mut self.position)?; count += 1; }
        if count < lo { return Ok(false); }
        if greedy {
            if leading && count < hi { self.restart = self.position.clone(); }
            if count > lo { self.stack.push(Saved::Single { pc, position: self.position.clone(), count, greedy })?; }
            self.pc = Some(after);
            Ok(true)
        } else {
            if count < hi { self.stack.push(Saved::Single { pc, position: self.position.clone(), count, greedy })?; }
            self.pc = Some(after);
            Ok(self.engine.matcher.program.maps[pc].can_start(self.engine.input.value(&self.position), 2))
        }
    }
    fn skip_until(&mut self, target: i32, have_match: bool) -> Result<bool, String> {
        while let Some(pc) = self.pc {
            match self.engine.matcher.program.instructions.get(pc).cloned() {
                Some(Instruction::Close { mark, .. }) if mark == target => {
                    if have_match { return self.step(pc); }
                    self.pc = Some(pc + 1);
                    return Ok(true);
                }
                Some(Instruction::Close { .. }) => {
                    self.step(pc)?;
                    if self.pc.is_none() { self.unwind(true)?; if self.pc.is_none() { self.pc = Some(pc + 1); } }
                }
                Some(Instruction::Match) => return Ok(true),
                Some(Instruction::Open { mark, .. }) => {
                    if mark > 0 { self.step(pc)?; } else { self.pc = Some(pc + 1); }
                    self.skip_until(mark, false)?;
                }
                Some(_) => self.pc = Some(pc + 1),
                None => return Err("Invalid regex ACCEPT continuation".to_owned()),
            }
        }
        Ok(true)
    }
    fn unwind(&mut self, have_match: bool) -> Result<bool, String> {
        let mut actions = vec![UnwindAction::Begin(have_match)];
        let mut returned = false;
        while let Some(action) = actions.pop() {
            match action {
                UnwindAction::Begin(matched) => {
                    self.recursive_result = matched;
                    self.unwound_assertion = false;
                    self.unwound_alternative = false;
                    actions.push(UnwindAction::Drain);
                }
                UnwindAction::CommitAfter(matched) => {
                    if returned && !self.unwound_assertion {
                        actions.push(UnwindAction::CommitAfter(matched));
                        actions.push(UnwindAction::Begin(matched));
                    } else {
                        if self.unwound_assertion && self.pc.is_some() { self.unwound_assertion = false; self.stack.push(Saved::Commit)?; }
                        self.independent = false;
                        returned = self.pc.is_some();
                    }
                }
                UnwindAction::ThenAfter(matched) => {
                    if returned && !self.unwound_alternative {
                        actions.push(UnwindAction::ThenAfter(matched));
                        actions.push(UnwindAction::Begin(matched));
                    } else if returned && self.unwound_alternative {
                        actions.push(UnwindAction::ThenFinish);
                        actions.push(UnwindAction::Begin(matched));
                    } else { returned = self.pc.is_some(); }
                }
                UnwindAction::ThenFinish => returned = self.pc.is_some(),
                UnwindAction::Drain => {
            let matched = self.recursive_result;
            let more = match self.stack.pop()? {
                Saved::End | Saved::Stop => { self.pc = None; false }
                Saved::Parent(group, prior) => {
                    if !matched { self.results.groups[group] = prior; if group != 0 { self.results.last_closed = Some(group); } }
                    true
                }
                Saved::Case(casei) => { self.casei = casei; true }
                Saved::Extra => true,
                Saved::Counter(counter) => { self.counter = counter.next; true }
                Saved::Assertion { pc, position, positive } => {
                    self.pc = Some(pc);
                    self.position = position;
                    self.recursive_result = if positive { matched } else { !matched };
                    self.unwound_assertion = true;
                    matched != positive
                }
                Saved::Alternative { pc, position } => {
                    if !matched { self.pc = Some(pc); self.position = position; }
                    self.unwound_alternative = !matched;
                    matched
                }
                Saved::NonGreedy { pc, position } => {
                    if !matched {
                        self.position = position;
                        self.pc = Some(pc);
                        let counter = self.counter.ok_or_else(|| "Invalid regex non-greedy counter".to_owned())?;
                        self.stack.counter_mut(counter)?.count += 1;
                    }
                    matched
                }
                Saved::Single { pc, position, mut count, greedy } => {
                    if matched { true } else {
                        let (lo, hi, after) = match self.engine.matcher.program.instructions.get(pc) {
                            Some(Instruction::Repeat { lo, hi, after, .. }) => (*lo, *hi, *after),
                            _ => return Err("Invalid regex repeat unwind".to_owned()),
                        };
                        self.position = position;
                        if greedy {
                            loop {
                                self.engine.input.previous(&mut self.position)?;
                                count -= 1;
                                self.engine.visited = self.engine.visited.saturating_add(1);
                                if count == lo || self.engine.matcher.program.maps[pc].can_start(self.engine.input.value(&self.position), 2) { break; }
                            }
                            if count > lo { self.stack.push(Saved::Single { pc, position: self.position.clone(), count, greedy })?; }
                            if !self.engine.matcher.program.maps[pc].can_start(self.engine.input.value(&self.position), 2) { true }
                            else { self.pc = Some(after); false }
                        } else {
                            let mut failed = false;
                            while count < hi && !self.engine.input.at_end(&self.position) {
                                if !self.single_match(pc + 1, &self.position)? { failed = true; break; }
                                self.engine.input.next(&mut self.position)?;
                                count += 1;
                                self.engine.visited = self.engine.visited.saturating_add(1);
                                if self.engine.input.at_end(&self.position) || self.engine.matcher.program.maps[pc].can_start(self.engine.input.value(&self.position), 2) { break; }
                            }
                            let skip = !failed && self.engine.matcher.program.maps[pc].can_start(self.engine.input.value(&self.position), 2);
                            if count < hi && !self.engine.input.at_end(&self.position) && skip {
                                self.stack.push(Saved::Single { pc, position: self.position.clone(), count, greedy })?;
                            }
                            if skip { self.pc = Some(after); false } else { true }
                        }
                    }
                }
                Saved::Recursion { group, pc, internal, prior } => {
                    if !matched {
                        self.calls.push(SourceCall { group, return_pc: pc, results: prior, start: self.position.clone() });
                        self.results = internal;
                    }
                    true
                }
                Saved::RecursionPop => {
                    if !matched {
                        if let Some(call) = self.calls.pop() { self.results = call.results; self.position = call.start; }
                    }
                    true
                }
                Saved::Commit => {
                    actions.push(UnwindAction::CommitAfter(matched));
                    actions.push(UnwindAction::Begin(matched));
                    continue;
                }
                Saved::Then => {
                    actions.push(UnwindAction::ThenAfter(matched));
                    actions.push(UnwindAction::Begin(matched));
                    continue;
                }
            };
            if more { actions.push(UnwindAction::Drain); } else { returned = self.pc.is_some(); }
                }
            }
        }
        Ok(returned)
    }
}

struct Engine<'m, 'i> {
    matcher: &'m Matcher,
    input: ExecutionInput<'i>,
    search_start: Cursor<'i>,
    visited: usize,
}

impl<'i> Engine<'_, 'i> {
    fn advance_candidate(&self, cursor: &mut Cursor<'i>) -> Result<bool, String> {
        match self.matcher.program.restart {
            Restart::Buffer | Restart::Continue => Ok(false),
            Restart::Any => {
                if self.input.at_end(cursor) { return Ok(false); }
                self.input.next(cursor)?;
                Ok(true)
            }
            Restart::Line => {
                while self.input.value(cursor).is_some_and(|unit| !separator(unit)) { self.input.next(cursor)?; }
                if self.input.at_end(cursor) { return Ok(false); }
                self.input.next(cursor)?;
                Ok(true)
            }
            Restart::Word => {
                while self.input.value(cursor).is_some_and(|unit| self.matcher.traits.has(unit, 0x0504)) { self.input.next(cursor)?; }
                while self.input.value(cursor).is_some_and(|unit| !self.matcher.traits.has(unit, 0x0504)) { self.input.next(cursor)?; }
                Ok(!self.input.at_end(cursor))
            }
        }
    }
    fn anchor(&self, anchor: Anchor, position: &Cursor<'i>) -> Result<bool, String> {
        let begin = self.input.begin_cursor()?;
        let at_begin = self.input.same(position, &begin);
        let previous = if at_begin { None } else {
            let mut cursor = position.clone();
            self.input.previous(&mut cursor)?;
            self.input.value(&cursor)
        };
        let next = self.input.value(position);
        let previous_word = previous.is_some_and(|unit| self.matcher.traits.has(unit, 0x0504));
        let next_word = next.is_some_and(|unit| self.matcher.traits.has(unit, 0x0504));
        Ok(match anchor {
            Anchor::StartText => at_begin,
            Anchor::EndText => self.input.at_end(position),
            Anchor::Continue => self.input.same(position, &self.search_start),
            Anchor::StartLine => at_begin || previous.is_some_and(separator) && !(previous == Some(13) && next == Some(10)),
            Anchor::EndLine => self.input.at_end(position) || next.is_some_and(separator) && !(previous == Some(13) && next == Some(10)),
            Anchor::SoftEnd => {
                let mut cursor = position.clone();
                while self.input.value(&cursor).is_some_and(separator) { self.input.next(&mut cursor)?; }
                self.input.at_end(&cursor)
            }
            Anchor::Word => previous_word != next_word,
            Anchor::NotWord => previous_word == next_word,
            Anchor::WordStart => !previous_word && next_word,
            Anchor::WordEnd => previous_word && !next_word,
        })
    }
    fn class_end(&self, class: &UnitClass, casei: bool, position: &Cursor<'i>) -> Result<Option<Cursor<'i>>, String> {
        let Some(unit) = self.input.value(position) else { return Ok(None); };
        let mut matched_end = None;
        let mut classes = 0u16;
        let mut negated_classes = 0u16;
        let col = self.matcher.traits.translate(unit, casei);
        for item in class.items.iter().filter(|item| matches!(item, ClassItem::Literal(_))).chain(class.items.iter().filter(|item| !matches!(item, ClassItem::Literal(_)))) {
            let end = match item {
                ClassItem::Literal(units) => self.input.literal_end(position, units, &self.matcher.traits, casei)?,
                ClassItem::Range(start, end) => {
                    if start.as_slice() <= [col].as_slice() && [col].as_slice() <= end.as_slice() { Some(self.one_unit_end(position)?) } else { None }
                }
                ClassItem::Mask(mask, negated) => {
                    if *negated { negated_classes |= *mask; } else { classes |= *mask; }
                    None
                }
                ClassItem::Equivalent(units) => {
                    if primary_equal(&[col], units)? { Some(self.one_unit_end(position)?) } else { None }
                }
            };
            if end.is_some() { matched_end = end; break; }
        }
        if matched_end.is_none() && (self.matcher.traits.has(col, classes) || negated_classes != 0 && !self.matcher.traits.has(col, negated_classes)) { matched_end = Some(self.one_unit_end(position)?); }
        Ok(if class.negated {
            if matched_end.is_none() { Some(self.one_unit_end(position)?) } else { None }
        } else { matched_end })
    }
    fn one_unit_end(&self, position: &Cursor<'i>) -> Result<Cursor<'i>, String> {
        let mut end = position.clone();
        self.input.next(&mut end)?;
        Ok(end)
    }
}

struct UnitTraits { lower: Vec<u16>, masks: Vec<u16> }

impl UnitTraits {
    fn translate(&self, unit: u16, casei: bool) -> u16 { if casei { self.lower[usize::from(unit)] } else { unit } }
    fn equal(&self, left: u16, right: u16, casei: bool) -> bool { self.translate(left, casei) == self.translate(right, casei) }
    fn has(&self, unit: u16, mask: u16) -> bool {
        self.masks[usize::from(unit)] & mask & 0x03ff != 0
            || mask & 0x0400 != 0 && unit == 95
            || mask & 0x0800 != 0 && unit >= 256
            || mask & 0x2000 != 0 && (separator(unit) || unit == 11)
            || mask & 0x1000 != 0 && self.has(unit, 8) && !separator(unit) && unit != 11
    }
}

fn class_mask(unit: u16) -> Option<u16> {
    named_mask(match unit {
        100 | 68 => "d", 115 | 83 => "s", 119 | 87 => "w", 104 | 72 => "h",
        118 | 86 => "v", 108 | 76 => "l", 117 | 85 => "u", _ => return None,
    })
}

fn named_mask(name: &str) -> Option<u16> {
    Some(match name {
        "alnum" => 0x0104, "alpha" => 0x0100, "blank" => 0x0040, "cntrl" => 0x0020,
        "d" | "digit" => 0x0004, "graph" => 0x0597, "h" => 0x1000,
        "l" | "lower" => 0x0002, "print" => 0x05df, "punct" => 0x0010,
        "s" | "space" => 0x0008, "u" | "upper" => 0x0001, "unicode" => 0x0800,
        "v" => 0x2000, "w" | "word" => 0x0504, "xdigit" => 0x0080,
        _ => return None,
    })
}

fn capture_name_hash(name: &str) -> u32 {
    let mut value = 0u64;
    for unit in name.encode_utf16() {
        value ^= u64::from(unit).wrapping_add(0x9e3779b9).wrapping_add(value.wrapping_shl(6)).wrapping_add(value >> 2);
    }
    (value % 0x7fffffff) as u32 | 0x40000000
}

fn reference_index(reference: &Reference) -> i32 {
    match reference { Reference::Number(index) => *index as i32, Reference::Name(name) => capture_name_hash(name) as i32 }
}

fn complexity_error() -> String {
    "The complexity of matching the regular expression exceeded predefined bounds.  Try refactoring the regular expression to make each choice made by the state machine unambiguous.  This exception is thrown to prevent \"eternal\" matches that take an indefinite period time to locate.".to_owned()
}

fn separator(unit: u16) -> bool { matches!(unit, 10 | 13 | 12 | 0x85 | 0x2028 | 0x2029) }

fn combining(unit: u16) -> bool {
    const RANGES: &[(u16, u16)] = &[
        (0x0300, 0x0361), (0x0483, 0x0486), (0x0903, 0x0903), (0x093e, 0x0940),
        (0x0949, 0x094c), (0x0982, 0x0983), (0x09be, 0x09c0), (0x09c7, 0x09cc),
        (0x09d7, 0x09d7), (0x0a3e, 0x0a40), (0x0a83, 0x0a83), (0x0abe, 0x0ac0),
        (0x0ac9, 0x0acc), (0x0b02, 0x0b03), (0x0b3e, 0x0b3e), (0x0b40, 0x0b40),
        (0x0b47, 0x0b4c), (0x0b57, 0x0b57), (0x0b83, 0x0b83), (0x0bbe, 0x0bbf),
        (0x0bc1, 0x0bcc), (0x0bd7, 0x0bd7), (0x0c01, 0x0c03), (0x0c41, 0x0c44),
        (0x0c82, 0x0c83), (0x0cbe, 0x0cbe), (0x0cc0, 0x0cc4), (0x0cc7, 0x0ccb),
        (0x0cd5, 0x0cd6), (0x0d02, 0x0d03), (0x0d3e, 0x0d40), (0x0d46, 0x0d4c),
        (0x0d57, 0x0d57), (0x0f7f, 0x0f7f), (0x20d0, 0x20e1), (0x3099, 0x309a),
        (0xfe20, 0xfe23), (0xffff, 0xffff),
    ];
    RANGES.iter().any(|&(start, end)| start <= unit && unit <= end)
}

fn collating_name(name: &str) -> Option<Vec<u16>> {
    let units: Vec<_> = name.encode_utf16().collect();
    let key: Vec<_> = units.iter().map(|&unit| unit as u8).collect();
    const NAMES: &[&str] = &[
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "alert", "backspace", "tab", "newline",
        "vertical-tab", "form-feed", "carriage-return", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK",
        "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "IS4", "IS3", "IS2", "IS1", "space", "exclamation-mark",
        "quotation-mark", "number-sign", "dollar-sign", "percent-sign", "ampersand", "apostrophe",
        "left-parenthesis", "right-parenthesis", "asterisk", "plus-sign", "comma", "hyphen", "period", "slash",
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "colon", "semicolon",
        "less-than-sign", "equals-sign", "greater-than-sign", "question-mark", "commercial-at",
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
        "left-square-bracket", "backslash", "right-square-bracket", "circumflex", "underscore", "grave-accent",
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
        "left-curly-bracket", "vertical-line", "right-curly-bracket", "tilde", "DEL",
    ];
    if let Some(index) = NAMES.iter().position(|candidate| candidate.as_bytes() == key.as_slice()) { return Some(vec![index as u16]); }
    for candidate in ["ae", "Ae", "AE", "ch", "Ch", "CH", "ll", "Ll", "LL", "ss", "Ss", "SS", "nj", "Nj", "NJ", "dz", "Dz", "DZ", "lj", "Lj", "LJ"] {
        if candidate.as_bytes() == key.as_slice() { return Some(candidate.encode_utf16().collect()); }
    }
    (units.len() == 1).then_some(units)
}

fn unit_traits() -> Result<Arc<UnitTraits>, String> {
    static TRAITS: std::sync::OnceLock<Result<Arc<UnitTraits>, String>> = std::sync::OnceLock::new();
    TRAITS.get_or_init(|| native_traits::build().map(Arc::new)).clone()
}

fn primary_equal(left: &[u16], right: &[u16]) -> Result<bool, String> {
    native_traits::primary_equal(left, right)
}

#[cfg(target_os = "macos")]
mod native_traits {
    use super::UnitTraits;
    use std::ffi::c_void;

    type CfRef = *const c_void;

    #[repr(C)]
    struct CfRange { location: isize, length: isize }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: CfRef);
        fn CFLocaleCopyCurrent() -> CfRef;
        fn CFCharacterSetGetPredefined(index: isize) -> CfRef;
        fn CFCharacterSetIsCharacterMember(set: CfRef, unit: u16) -> u8;
        fn CFStringCreateWithCharacters(allocator: CfRef, units: *const u16, length: isize) -> CfRef;
        fn CFStringCreateMutableCopy(allocator: CfRef, maximum: isize, string: CfRef) -> CfRef;
        fn CFStringLowercase(string: *mut c_void, locale: CfRef);
        fn CFStringGetLength(string: CfRef) -> isize;
        fn CFStringGetCharacterAtIndex(string: CfRef, index: isize) -> u16;
        fn CFStringCompareWithOptionsAndLocale(left: CfRef, right: CfRef, range: CfRange, flags: usize, locale: CfRef) -> isize;
    }

    struct OwnedCf(CfRef);

    impl OwnedCf {
        fn new(value: CfRef) -> Result<Self, String> {
            if value.is_null() { Err("The regex locale operation could not allocate its result".to_owned()) } else { Ok(Self(value)) }
        }
        fn string(units: &[u16]) -> Result<Self, String> {
            let length = isize::try_from(units.len()).map_err(|_| "The regex collation input is too long".to_owned())?;
            Self::new(unsafe { CFStringCreateWithCharacters(std::ptr::null(), units.as_ptr(), length) })
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) { unsafe { CFRelease(self.0); } }
    }

    pub(super) fn build() -> Result<UnitTraits, String> {
        let locale = OwnedCf::new(unsafe { CFLocaleCopyCurrent() })?;
        let categories = [(10, 1u16), (9, 2), (5, 4), (3, 8), (11, 16), (14, 16), (1, 32), (2, 64), (4, 256)];
        let mut sets = Vec::new();
        for (category, mask) in categories {
            let set = unsafe { CFCharacterSetGetPredefined(category) };
            if set.is_null() { return Err("The regex locale character class is unavailable".to_owned()); }
            sets.push((set, mask));
        }
        let mut lower = Vec::with_capacity(65536);
        let mut masks = Vec::with_capacity(65536);
        for value in 0u32..=65535 {
            let unit = value as u16;
            let mut mask = 0u16;
            for &(set, class) in &sets {
                if unsafe { CFCharacterSetIsCharacterMember(set, unit) } != 0 { mask |= class; }
            }
            if matches!(unit, 48..=57 | 65..=70 | 97..=102 | 0xff10..=0xff19 | 0xff21..=0xff26 | 0xff41..=0xff46) { mask |= 128; }
            let translated = if (0xd800..=0xdfff).contains(&unit) { unit } else {
                let string = OwnedCf::string(&[unit])?;
                let mutable = OwnedCf::new(unsafe { CFStringCreateMutableCopy(std::ptr::null(), 0, string.0) })?;
                unsafe { CFStringLowercase(mutable.0.cast_mut(), locale.0); }
                if unsafe { CFStringGetLength(mutable.0) } == 1 { unsafe { CFStringGetCharacterAtIndex(mutable.0, 0) } } else { unit }
            };
            lower.push(translated);
            masks.push(mask);
        }
        Ok(UnitTraits { lower, masks })
    }

    pub(super) fn primary_equal(left: &[u16], right: &[u16]) -> Result<bool, String> {
        if left == right { return Ok(true); }
        if left.iter().chain(right).any(|unit| (0xd800..=0xdfff).contains(unit)) { return Ok(false); }
        let locale = OwnedCf::new(unsafe { CFLocaleCopyCurrent() })?;
        let left_string = OwnedCf::string(left)?;
        let right_string = OwnedCf::string(right)?;
        let length = isize::try_from(left.len()).map_err(|_| "The regex collation input is too long".to_owned())?;
        Ok(unsafe {
            CFStringCompareWithOptionsAndLocale(left_string.0, right_string.0, CfRange { location: 0, length }, 1 | 16 | 32 | 128 | 256, locale.0)
        } == 0)
    }
}

#[cfg(not(target_os = "macos"))]
mod native_traits {
    use super::UnitTraits;

    pub(super) fn build() -> Result<UnitTraits, String> {
        let mut lower = Vec::with_capacity(65536);
        let mut masks = Vec::with_capacity(65536);
        for value in 0u32..=65535 {
            let unit = value as u16;
            let mut mask = 0u16;
            let translated = if let Some(character) = char::from_u32(value) {
                if character.is_uppercase() { mask |= 1; }
                if character.is_lowercase() { mask |= 2; }
                if decimal_digit(unit) { mask |= 4; }
                if character.is_whitespace() { mask |= 8; }
                if character.is_ascii_punctuation() { mask |= 16; }
                if character.is_control() { mask |= 32; }
                if character.is_whitespace() && !super::separator(unit) && unit != 11 { mask |= 64; }
                if character.is_ascii_hexdigit() { mask |= 128; }
                if character.is_alphabetic() { mask |= 256; }
                let mut lowercase = character.to_lowercase();
                match (lowercase.next(), lowercase.next()) {
                    (Some(value), None) if value.len_utf16() == 1 => value as u16,
                    _ => unit,
                }
            } else { unit };
            lower.push(translated);
            masks.push(mask);
        }
        Ok(UnitTraits { lower, masks })
    }

    fn decimal_digit(unit: u16) -> bool {
        const STARTS: &[u16] = &[0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66, 0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10];
        STARTS.iter().any(|&start| start <= unit && unit <= start + 9)
    }

    pub(super) fn primary_equal(left: &[u16], right: &[u16]) -> Result<bool, String> {
        if left == right { return Ok(true); }
        Err("Locale collation requires the macOS CoreFoundation implementation of Boost w32_regex_traits.hpp:464-506".to_owned())
    }
}
