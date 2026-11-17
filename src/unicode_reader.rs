#[derive(Default)]
pub(crate) struct Utf16Reader {
    high: Option<u16>,
}

impl Utf16Reader {
    pub(crate) fn convert(&mut self, input: &[u8], big_endian: bool) -> Vec<u8> {
        let mut output = Vec::new();
        for pair in input.as_chunks::<2>().0 {
            let unit = if big_endian {
                u16::from_be_bytes(*pair)
            } else {
                u16::from_le_bytes(*pair)
            };
            if let Some(high) = self.high.take() {
                if (0xdc00..0xe000).contains(&unit) {
                    let code =
                        0x10000 + ((u32::from(high) & 0x3ff) << 10) + (u32::from(unit) & 0x3ff);
                    output.extend_from_slice(&[
                        (0xf0 | ((code >> 18) & 7)) as u8,
                        (0x80 | ((code >> 12) & 0x3f)) as u8,
                        (0x80 | ((code >> 6) & 0x3f)) as u8,
                        (0x80 | (code & 0x3f)) as u8,
                    ]);
                }
            } else if (0xd800..0xdc00).contains(&unit) {
                self.high = Some(unit);
            } else if unit < 0x80 {
                output.push(unit as u8);
            } else if unit < 0x800 {
                output
                    .extend_from_slice(&[(0xc0 | (unit >> 6)) as u8, (0x80 | (unit & 0x3f)) as u8]);
            } else {
                output.extend_from_slice(&[
                    (0xe0 | (unit >> 12)) as u8,
                    (0x80 | ((unit >> 6) & 0x3f)) as u8,
                    (0x80 | (unit & 0x3f)) as u8,
                ]);
            }
        }
        output
    }
}

