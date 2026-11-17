use std::fmt;
use std::io::{self, Write};
use std::ops::Range;
use std::sync::Arc;

use crate::unicode_class::{CharacterClass, classify};
use crate::unicode_grapheme;

const LEAF_BYTES: usize = 4096;
const COPIED_MERGE_BYTES: usize = 256;

#[derive(Clone, Copy, Default)]
struct Lines {
    breaks: usize,
    first: Option<u8>,
    last: Option<u8>,
}

impl Lines {
    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            breaks: bytes
                .iter()
                .enumerate()
                .filter(|(index, byte)| {
                    **byte == b'\r'
                        || (**byte == b'\n' && (*index == 0 || bytes[index - 1] != b'\r'))
                })
                .count(),
            first: bytes.first().copied(),
            last: bytes.last().copied(),
        }
    }

    fn join(self, right: Self) -> Self {
        Self {
            breaks: self.breaks + right.breaks
                - usize::from(self.last == Some(b'\r') && right.first == Some(b'\n')),
            first: self.first.or(right.first),
            last: right.last.or(self.last),
        }
    }
}

#[derive(Clone)]
enum Node {
    Leaf {
        bytes: Arc<[u8]>,
        range: Range<usize>,
        lines: Lines,
    },
    Branch {
        left: Arc<Node>,
        right: Arc<Node>,
        length: usize,
        height: usize,
        lines: Lines,
    },
}

impl Node {
    fn lines(&self) -> Lines {
        match self {
            Self::Leaf { lines, .. } | Self::Branch { lines, .. } => *lines,
        }
    }

    fn prefix_lines(&self, length: usize) -> Lines {
        if length == 0 {
            return Lines::default();
        }
        if length == self.len() {
            return self.lines();
        }
        match self {
            Self::Leaf { bytes, range, .. } => {
                Lines::from_bytes(&bytes[range.start..range.start + length])
            }
            Self::Branch { left, right, .. } => {
                if length <= left.len() {
                    left.prefix_lines(length)
                } else {
                    left.lines().join(right.prefix_lines(length - left.len()))
                }
            }
        }
    }

    fn break_position(&self, index: usize) -> Option<usize> {
        if index == 0 || index > self.lines().breaks {
            return None;
        }
        match self {
            Self::Leaf { bytes, range, .. } => {
                let bytes = &bytes[range.clone()];
                let mut count = 0;
                for (offset, &byte) in bytes.iter().enumerate() {
                    if byte == b'\r'
                        || (byte == b'\n' && (offset == 0 || bytes[offset - 1] != b'\r'))
                    {
                        count += 1;
                        if count == index {
                            return Some(offset);
                        }
                    }
                }
                None
            }
            Self::Branch { left, right, .. } => {
                if index <= left.lines().breaks {
                    left.break_position(index)
                } else {
                    let crossing = usize::from(
                        left.lines().last == Some(b'\r') && right.lines().first == Some(b'\n'),
                    );
                    right
                        .break_position(index - left.lines().breaks + crossing)
                        .map(|offset| left.len() + offset)
                }
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Leaf { range, .. } => range.len(),
            Self::Branch { length, .. } => *length,
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { height, .. } => *height,
        }
    }
}

fn leaf(bytes: Arc<[u8]>, range: Range<usize>) -> Arc<Node> {
    let lines = Lines::from_bytes(&bytes[range.clone()]);
    Arc::new(Node::Leaf {
        bytes,
        range,
        lines,
    })
}

fn branch(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    Arc::new(Node::Branch {
        length: left.len() + right.len(),
        height: left.height().max(right.height()) + 1,
        lines: left.lines().join(right.lines()),
        left,
        right,
    })
}

fn balance(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    if left.height() > right.height() + 1
        && let Node::Branch {
            left: outer,
            right: inner,
            ..
        } = left.as_ref()
    {
        if outer.height() >= inner.height() {
            return branch(outer.clone(), branch(inner.clone(), right));
        }
        if let Node::Branch {
            left: middle_left,
            right: middle_right,
            ..
        } = inner.as_ref()
        {
            return branch(
                branch(outer.clone(), middle_left.clone()),
                branch(middle_right.clone(), right),
            );
        }
    }
    if right.height() > left.height() + 1
        && let Node::Branch {
            left: inner,
            right: outer,
            ..
        } = right.as_ref()
    {
        if outer.height() >= inner.height() {
            return branch(branch(left, inner.clone()), outer.clone());
        }
        if let Node::Branch {
            left: middle_left,
            right: middle_right,
            ..
        } = inner.as_ref()
        {
            return branch(
                branch(left, middle_left.clone()),
                branch(middle_right.clone(), outer.clone()),
            );
        }
    }
    branch(left, right)
}

fn join(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    if let (
        Node::Leaf {
            bytes: a,
            range: ar,
            ..
        },
        Node::Leaf {
            bytes: b,
            range: br,
            ..
        },
    ) = (left.as_ref(), right.as_ref())
        && ar.len() + br.len() <= LEAF_BYTES
    {
        if Arc::ptr_eq(a, b) && ar.end == br.start {
            return leaf(a.clone(), ar.start..br.end);
        }
        if ar.len() + br.len() <= COPIED_MERGE_BYTES {
            let mut bytes = Vec::with_capacity(ar.len() + br.len());
            bytes.extend_from_slice(&a[ar.clone()]);
            bytes.extend_from_slice(&b[br.clone()]);
            let length = bytes.len();
            return leaf(bytes.into(), 0..length);
        }
    }
    if left.height() > right.height() + 1
        && let Node::Branch {
            left: a, right: b, ..
        } = left.as_ref()
    {
        return balance(a.clone(), join(b.clone(), right));
    }
    if right.height() > left.height() + 1
        && let Node::Branch {
            left: a, right: b, ..
        } = right.as_ref()
    {
        return balance(join(left, a.clone()), b.clone());
    }
    branch(left, right)
}

fn concat(left: Option<Arc<Node>>, right: Option<Arc<Node>>) -> Option<Arc<Node>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(join(left, right)),
        (left, right) => left.or(right),
    }
}

fn split(node: Arc<Node>, position: usize) -> (Option<Arc<Node>>, Option<Arc<Node>>) {
    if position == 0 {
        return (None, Some(node));
    }
    if position == node.len() {
        return (Some(node), None);
    }
    match node.as_ref() {
        Node::Leaf { bytes, range, .. } => (
            Some(leaf(bytes.clone(), range.start..range.start + position)),
            Some(leaf(bytes.clone(), range.start + position..range.end)),
        ),
        Node::Branch { left, right, .. } => {
            if position < left.len() {
                let (before, after) = split(left.clone(), position);
                (before, concat(after, Some(right.clone())))
            } else {
                let (before, after) = split(right.clone(), position - left.len());
                (concat(Some(left.clone()), before), after)
            }
        }
    }
}

fn build(bytes: &[u8]) -> Option<Arc<Node>> {
    if bytes.is_empty() {
        return None;
    }
    if bytes.len() <= LEAF_BYTES {
        return Some(leaf(bytes.into(), 0..bytes.len()));
    }
    let middle = bytes.len() / 2;
    concat(build(&bytes[..middle]), build(&bytes[middle..]))
}

#[derive(Clone, Default)]
pub struct ByteBuffer {
    root: Option<Arc<Node>>,
}

impl fmt::Debug for ByteBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ByteBuffer")
            .field("len", &self.len())
            .field("line_count", &self.line_count())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidByteRange;

impl fmt::Display for InvalidByteRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("byte range is outside the document")
    }
}

impl std::error::Error for InvalidByteRange {}

impl ByteBuffer {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self { root: build(bytes) }
    }

    pub fn len(&self) -> usize {
        self.root.as_ref().map_or(0, |node| node.len())
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn line_count(&self) -> usize {
        self.root.as_ref().map_or(0, |root| root.lines().breaks) + 1
    }

    pub fn line_start(&self, line: usize) -> Result<usize, InvalidByteRange> {
        if line == 0 {
            return Ok(0);
        }
        let at = self
            .root
            .as_ref()
            .and_then(|root| root.break_position(line))
            .ok_or(InvalidByteRange)?;
        Ok(at + 1 + usize::from(self.byte(at) == Some(b'\r') && self.byte(at + 1) == Some(b'\n')))
    }

    pub fn line_index(&self, position: usize) -> Result<usize, InvalidByteRange> {
        if position > self.len() {
            return Err(InvalidByteRange);
        }
        let count = self
            .root
            .as_ref()
            .map_or(0, |root| root.prefix_lines(position).breaks);
        let inside_crlf = position != 0
            && self.byte(position - 1) == Some(b'\r')
            && self.byte(position) == Some(b'\n');
        Ok(count - usize::from(inside_crlf))
    }

    pub fn byte(&self, mut position: usize) -> Option<u8> {
        let mut node = self.root.as_deref()?;
        if position >= node.len() {
            return None;
        }
        loop {
            match node {
                Node::Leaf { bytes, range, .. } => return Some(bytes[range.start + position]),
                Node::Branch { left, right, .. } => {
                    node = if position < left.len() {
                        left
                    } else {
                        position -= left.len();
                        right
                    };
                }
            }
        }
    }

    pub fn adjacent_position(&self, position: usize, forward: bool) -> Result<usize, InvalidByteRange> {
        if position > self.len() { return Err(InvalidByteRange); }
        let next = if forward { position.saturating_add(1).min(self.len()) } else { position.saturating_sub(1) };
        self.move_position_outside_char(next, forward, true)
    }

    fn good_utf8_range(&self, position: usize) -> Option<Range<usize>> {
        if !self.byte(position).is_some_and(|byte| byte & 0xc0 == 0x80) { return None; }
        for start in (position.saturating_sub(3)..position).rev() {
            let bytes = (start..start.saturating_add(4).min(self.len())).filter_map(|at| self.byte(at));
            if let Some(unit) = crate::byte_text::units(bytes).next()
                && matches!(unit.value, crate::byte_text::ByteValue::Scalar(_))
                && start + unit.range.end > position {
                return Some(start..start + unit.range.end);
            }
        }
        None
    }

    pub fn move_position_outside_char(&self, position: usize, forward: bool, check_line_end: bool) -> Result<usize, InvalidByteRange> {
        if position > self.len() { return Err(InvalidByteRange); }
        if position == 0 || position == self.len() { return Ok(position); }
        if check_line_end && self.byte(position - 1) == Some(b'\r') && self.byte(position) == Some(b'\n') {
            return Ok(if forward { position + 1 } else { position - 1 });
        }
        Ok(self.good_utf8_range(position).map_or(position, |range| if forward { range.end } else { range.start }))
    }

    pub fn next_position(&self, position: usize, forward: bool) -> Result<usize, InvalidByteRange> {
        if position > self.len() { return Err(InvalidByteRange); }
        if forward {
            if position == self.len() { return Ok(position); }
            let bytes = (position..position.saturating_add(4).min(self.len())).filter_map(|at| self.byte(at));
            let length = crate::byte_text::units(bytes).next().map_or(1, |unit| unit.range.len());
            return Ok((position + length).min(self.len()));
        }
        Ok(self.previous_position(position))
    }

    pub(crate) fn previous_position(&self, position: usize) -> usize {
        let previous = position.saturating_sub(1);
        self.good_utf8_range(previous).map_or(previous, |range| range.start)
    }

    pub fn deletion_range(&self, position: usize, backward: bool) -> Result<Range<usize>, InvalidByteRange> {
        if position > self.len() { return Err(InvalidByteRange); }
        if backward {
            let start = if position >= 2 && self.byte(position - 2) == Some(b'\r') && self.byte(position - 1) == Some(b'\n') {
                position - 2
            } else { self.next_position(position, false)? };
            Ok(start..position)
        } else {
            let end = if self.byte(position) == Some(b'\r') && self.byte(position + 1) == Some(b'\n') {
                position + 2
            } else { self.next_position(position, true)? };
            Ok(position..end)
        }
    }

    pub fn count_characters(&self, range: Range<usize>, limit: usize) -> Result<usize, InvalidByteRange> {
        self.validate(&range)?;
        let mut position = self.move_position_outside_char(range.start, true, false)?;
        let end = self.move_position_outside_char(range.end, false, false)?;
        let mut count = 0;
        while position < end && count < limit {
            count += 1;
            position = self.next_position(position, true)?;
        }
        Ok(count)
    }

    pub fn is_line_end(&self, position: usize) -> bool {
        position == self.len() || self.byte(position) == Some(b'\r')
            || self.byte(position) == Some(b'\n') && (position == 0 || self.byte(position - 1) != Some(b'\r'))
    }

    pub fn column(&self, position: usize, tab_width: usize) -> Result<usize, InvalidByteRange> {
        let line = self.line_index(position)?;
        let mut at = self.line_start(line)?;
        let width = tab_width.max(1);
        let mut column = 0;
        while at < position {
            match self.byte(at) {
                Some(b'\t') => { column += width - column % width; at += 1; }
                Some(b'\r' | b'\n') => break,
                Some(_) => {
                    at = self.next_position(at, true)?;
                    if at > position { break; }
                    column += 1;
                }
                None => return Err(InvalidByteRange),
            }
        }
        Ok(column)
    }

    pub fn find_column(&self, line: usize, target: usize, tab_width: usize) -> Result<usize, InvalidByteRange> {
        let mut position = self.line_start(line)?;
        let mut column = 0;
        let width = tab_width.max(1);
        while position < self.len() && !self.is_line_end(position) && column < target {
            let next = if self.byte(position) == Some(b'\t') {
                column + width - column % width
            } else { column + 1 };
            if next > target { return Ok(position); }
            column = next;
            position = self.next_position(position, true)?;
        }
        Ok(position)
    }

    pub(crate) fn character_at(&self, position: usize) -> (char, usize) {
        let input = (position..position.saturating_add(4).min(self.len())).filter_map(|at| self.byte(at));
        match crate::byte_text::units(input).next() {
            Some(crate::byte_text::ByteUnit { range, value: crate::byte_text::ByteValue::Scalar(character) }) => (character, range.len()),
            Some(_) => ('\u{fffd}', 1),
            None => ('\u{fffd}', 0),
        }
    }

    fn character_before(&self, position: usize) -> char {
        let Some(previous) = position.checked_sub(1) else { return '\u{fffd}'; };
        let byte = self.byte(previous).unwrap_or(0);
        if byte.is_ascii() { return char::from(byte); }
        if byte & 0xc0 != 0x80 { return '\u{fffd}'; }
        let mut start = previous;
        while start > 0 && position - start < 4 && self.byte(start).is_some_and(|byte| byte & 0xc0 == 0x80) { start -= 1; }
        let (character, width) = self.character_at(start);
        if start + width == position { character } else { '\u{fffd}' }
    }

    pub(crate) fn is_word_start(&self, position: usize) -> bool {
        if position >= self.len() { return false; }
        let current = classify(self.character_at(position).0);
        let previous = if position == 0 { CharacterClass::Space } else { classify(self.character_before(position)) };
        matches!(current, CharacterClass::Word | CharacterClass::Punctuation) && current != previous
    }

    pub(crate) fn is_word_end(&self, position: usize) -> bool {
        if position == 0 || position > self.len() { return false; }
        let previous = classify(self.character_before(position));
        let current = if position == self.len() { CharacterClass::Space } else { classify(self.character_at(position).0) };
        matches!(previous, CharacterClass::Word | CharacterClass::Punctuation) && current != previous
    }

    pub(crate) fn extend_word(&self, position: usize, forward: bool) -> Result<usize, InvalidByteRange> {
        let mut position = self.move_position_outside_char(position, forward, false)?;
        if forward {
            while position < self.len() && classify(self.character_at(position).0) == CharacterClass::Word {
                position = self.next_position(position, true)?;
            }
        } else {
            while position > 0 && classify(self.character_before(position)) == CharacterClass::Word {
                position = self.next_position(position, false)?;
            }
        }
        Ok(position)
    }

    pub(crate) fn discard_last_combined_character(&self, range: Range<usize>) -> Result<Option<usize>, InvalidByteRange> {
        self.validate(&range)?;
        let mut end = range.end;
        while end - range.start > 4 {
            let character = self.character_before(end);
            end = self.next_position(end, false)?;
            if unicode_grapheme::is_base(character) { return Ok(Some(end)); }
        }
        Ok(None)
    }

    pub(crate) fn find_literal_bytes(
        &self,
        query: &[u8],
        from: usize,
        to: usize,
        match_case: bool,
        whole_word: bool,
        word_start: bool,
    ) -> Result<Option<Range<usize>>, InvalidByteRange> {
        if query.is_empty() { return Ok(Some(from..from)); }
        let forward = from <= to;
        let start = self.move_position_outside_char(from, forward, false)?;
        let end = self.move_position_outside_char(to, forward, false)?;
        let limit = start.max(end);
        let mut position = if forward { start } else { self.next_position(start, false)? };
        let fold = |input: &[u8]| {
            let mut output = Vec::new();
            for unit in crate::byte_text::units(input.iter().copied()) {
                match unit.value {
                    crate::byte_text::ByteValue::Invalid(byte) => output.push(byte),
                    crate::byte_text::ByteValue::Scalar(character) => {
                        if let Some(mapped) = crate::unicode_case::mapping(character) { output.extend_from_slice(mapped.as_bytes()); }
                        else { output.extend(character.to_string().bytes()); }
                    }
                }
            }
            output
        };
        let key = if match_case { query.to_vec() } else { fold(query) };
        loop {
            if (forward && position >= end) || (!forward && position < end) { break; }
            let mut after = position;
            let mut matched = 0;
            let mut first_width = 1;
            if match_case {
                if query.len() <= limit - position
                    && query.iter().enumerate().all(|(index, byte)| self.byte(position + index) == Some(*byte)) {
                    matched = query.len();
                    after += query.len();
                }
            } else {
                while matched < key.len() && after < limit {
                    let width = self.character_at(after).1;
                    if after == position { first_width = width; }
                    if width == 0 || after + width > limit { break; }
                    let input: Vec<_> = (after..after + width).filter_map(|at| self.byte(at)).collect();
                    let folded = fold(&input);
                    if key.get(matched..matched + folded.len()) != Some(folded.as_slice()) { break; }
                    matched += folded.len();
                    after += width;
                }
            }
            if matched == key.len()
                && (!whole_word || position < after && self.is_word_start(position) && self.is_word_end(after))
                && (!word_start || self.is_word_start(position)) {
                return Ok(Some(position..after));
            }
            let next = if forward && (!match_case || query[0] & 0xc0 != 0x80) { position + first_width }
                else { self.next_position(position, forward)? };
            if next == position { break; }
            position = next;
        }
        Ok(None)
    }

    pub fn lines(&self) -> ByteLines {
        ByteLines { bytes: self.to_vec(), position: 0, finished: false }
    }

    pub fn chunks(&self) -> Chunks<'_> {
        Chunks {
            pending: self.root.as_deref().into_iter().collect(),
        }
    }

    pub fn units(&self) -> impl Iterator<Item = crate::byte_text::ByteUnit> + '_ {
        crate::byte_text::units(self.chunks().flat_map(|chunk| chunk.iter().copied()))
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.len());
        for chunk in self.chunks() {
            bytes.extend_from_slice(chunk);
        }
        bytes
    }

    pub fn write_to(&self, output: &mut impl Write) -> io::Result<()> {
        for chunk in self.chunks() {
            output.write_all(chunk)?;
        }
        Ok(())
    }

    pub fn slice(&self, range: Range<usize>) -> Result<Self, InvalidByteRange> {
        self.validate(&range)?;
        let root = self
            .root
            .clone()
            .and_then(|root| split(root, range.end).0)
            .and_then(|root| split(root, range.start).1);
        Ok(Self { root })
    }

    pub fn replace(&mut self, range: Range<usize>, bytes: &[u8]) -> Result<(), InvalidByteRange> {
        self.validate(&range)?;
        let (before, after) = match &self.root {
            Some(root) => {
                let (through_end, after) = split(root.clone(), range.end);
                let before = through_end.and_then(|root| split(root, range.start).0);
                (before, after)
            }
            None => (None, None),
        };
        self.root = concat(concat(before, build(bytes)), after);
        Ok(())
    }

    fn validate(&self, range: &Range<usize>) -> Result<(), InvalidByteRange> {
        if range.start <= range.end && range.end <= self.len() {
            Ok(())
        } else {
            Err(InvalidByteRange)
        }
    }
}

pub struct Chunks<'a> {
    pending: Vec<&'a Node>,
}

pub struct ByteLines {
    bytes: Vec<u8>,
    position: usize,
    finished: bool,
}

impl Iterator for ByteLines {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished { return None; }
        let start = self.position;
        while self.position < self.bytes.len() {
            let byte = self.bytes[self.position];
            self.position += 1;
            if byte == b'\r' {
                if self.bytes.get(self.position) == Some(&b'\n') { self.position += 1; }
                return Some(self.bytes[start..self.position].to_vec());
            }
            if byte == b'\n' { return Some(self.bytes[start..self.position].to_vec()); }
        }
        self.finished = true;
        Some(self.bytes[start..].to_vec())
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.pending.pop() {
            match node {
                Node::Leaf { bytes, range, .. } => return Some(&bytes[range.clone()]),
                Node::Branch { left, right, .. } => {
                    self.pending.push(right);
                    self.pending.push(left);
                }
            }
        }
        None
    }
}
