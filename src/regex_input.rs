use crate::byte_buffer::ByteBuffer;

#[derive(Clone)]
pub(crate) struct Utf8DocumentIterator<'a> {
    bytes: &'a ByteBuffer,
    position: usize,
    end: usize,
    character_index: usize,
    characters: [u16; 2],
    utf8_length: usize,
    utf16_length: usize,
}

impl<'a> Utf8DocumentIterator<'a> {
    pub(crate) fn new(bytes: &'a ByteBuffer, position: usize, end: usize) -> Result<Self, String> {
        if end > bytes.len() {
            return Err("The regex range is outside the document".to_owned());
        }
        let mut iterator = Self {
            bytes,
            position: position.min(end),
            end,
            character_index: 0,
            characters: [0; 2],
            utf8_length: 0,
            utf16_length: 0,
        };
        iterator.read_character();
        Ok(iterator)
    }

    pub(crate) fn position(&self) -> usize {
        self.position
    }

    pub(crate) fn at_end(&self) -> bool {
        self.position >= self.end
    }

    pub(crate) fn value(&self) -> u16 {
        self.characters[self.character_index]
    }

    pub(crate) fn next(&mut self) -> Result<(), String> {
        self.next_unit();
        Ok(())
    }

    pub(crate) fn next_unit(&mut self) {
        if self.at_end() {
            return;
        }
        if self.utf16_length == 2 && self.character_index == 0 {
            self.character_index = 1;
        } else {
            self.position = self.position.saturating_add(self.utf8_length).min(self.end);
            self.character_index = 0;
            self.read_character();
        }
    }

    pub(crate) fn previous(&mut self) -> Result<(), String> {
        if self.utf16_length == 2 && self.character_index == 1 {
            self.character_index = 0;
        } else {
            self.position = self.position.checked_sub(1)
                .ok_or_else(|| "The regex iterator is before the document".to_owned())?;
            while self.position > 0 && self.bytes.byte(self.position).is_some_and(|byte| byte & 0xc0 == 0x80) {
                self.position -= 1;
            }
            self.read_character();
            if self.utf16_length == 2 {
                self.character_index = 1;
            }
        }
        Ok(())
    }

    pub(crate) fn at_line_start(&self) -> Result<bool, String> {
        if self.position == 0 {
            return Ok(true);
        }
        let mut previous = self.clone();
        previous.previous()?;
        Ok(is_separator(previous.value())
            && (self.at_end() || previous.value() != 13 || self.value() != 10))
    }

    pub(crate) fn at_line_end(&self) -> Result<bool, String> {
        if self.at_end() {
            return Ok(true);
        }
        if !is_separator(self.value()) {
            return Ok(false);
        }
        if self.position != 0 {
            let mut previous = self.clone();
            previous.previous()?;
            if previous.value() == 13 && self.value() == 10 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn read_character(&mut self) {
        let first = self.bytes.byte(self.position).unwrap_or(0);
        if first & 0x80 == 0 {
            self.utf8_length = 1;
            self.utf16_length = 1;
            self.character_index = 0;
            self.characters[0] = u16::from(first);
            return;
        }
        let mut mask = 0x40;
        let mut count = 1;
        loop {
            mask >>= 1;
            count += 1;
            if first & mask == 0 {
                break;
            }
        }
        let first_mask = (0x7fu32 >> count) as u8;
        let mut result = i32::from(first & first_mask);
        let mut position = self.position;
        self.utf8_length = 1;
        while count > 1 && position < self.end {
            count -= 1;
            position += 1;
            let byte = self.bytes.byte(position).unwrap_or(0);
            if byte & 0xc0 != 0x80 {
                break;
            }
            result = result.wrapping_shl(6) | i32::from(byte & 0x3f);
            self.utf8_length += 1;
        }
        if result >= 0x10000 {
            result -= 0x10000;
            self.utf16_length = 2;
            self.characters[0] = (0xd800 + (result >> 10)) as u16;
            self.characters[1] = (0xdc00 + (result & 0x3ff)) as u16;
        } else {
            self.utf16_length = 1;
            self.characters[0] = result as u16;
        }
    }
}

fn is_separator(unit: u16) -> bool {
    matches!(unit, 10 | 13 | 12 | 0x85 | 0x2028 | 0x2029)
}
