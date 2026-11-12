use std::fmt;
use std::io::{self, Write};
use std::ops::Range;
use std::sync::Arc;

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
