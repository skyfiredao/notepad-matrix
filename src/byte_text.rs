use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteValue {
    Scalar(char),
    Invalid(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ByteUnit {
    pub range: Range<usize>,
    pub value: ByteValue,
}

pub struct Units<I> {
    input: I,
    pending: [u8; 4],
    count: usize,
    position: usize,
}

pub fn units(input: impl IntoIterator<Item = u8>) -> Units<impl Iterator<Item = u8>> {
    Units {
        input: input.into_iter(),
        pending: [0; 4],
        count: 0,
        position: 0,
    }
}

fn scalar(bytes: &[u8]) -> Option<char> {
    let lead = *bytes.first()?;
    let width = match lead {
        0..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return None,
    };
    let text = std::str::from_utf8(bytes.get(..width)?).ok()?;
    let character = text.chars().next()?;
    let code = u32::from(character);
    if (0xfdd0..=0xfdef).contains(&code) || code & 0xfffe == 0xfffe {
        None
    } else {
        Some(character)
    }
}

impl<I: Iterator<Item = u8>> Iterator for Units<I> {
    type Item = ByteUnit;

    fn next(&mut self) -> Option<Self::Item> {
        while self.count < self.pending.len() {
            let Some(byte) = self.input.next() else {
                break;
            };
            self.pending[self.count] = byte;
            self.count += 1;
        }
        if self.count == 0 {
            return None;
        }
        let (width, value) = match scalar(&self.pending[..self.count]) {
            Some(character) => (character.len_utf8(), ByteValue::Scalar(character)),
            None => (1, ByteValue::Invalid(self.pending[0])),
        };
        let range = self.position..self.position + width;
        self.pending.copy_within(width..self.count, 0);
        self.count -= width;
        self.position += width;
        Some(ByteUnit { range, value })
    }
}
