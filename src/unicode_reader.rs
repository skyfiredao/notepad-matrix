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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_utf16_iterator_preserves_low_surrogates_and_drops_bad_high_followers() {
        for big_endian in [false, true] {
            for (units, expected) in [
                (
                    vec![0x41, 0, 0x4e2d, 0xd83d, 0xde00],
                    "A\0中😀".as_bytes().to_vec(),
                ),
                (vec![0xdc00], vec![0xed, 0xb0, 0x80]),
                (vec![0xd800], vec![]),
                (vec![0xd800, 0x41, 0x42], vec![0x42]),
                (vec![0xd800, 0xd83d, 0xde00], vec![0xed, 0xb8, 0x80]),
            ] {
                let bytes: Vec<u8> = units
                    .into_iter()
                    .flat_map(|unit: u16| {
                        if big_endian {
                            unit.to_be_bytes()
                        } else {
                            unit.to_le_bytes()
                        }
                    })
                    .collect();
                assert_eq!(Utf16Reader::default().convert(&bytes, big_endian), expected);
            }
        }
        let mut reader = Utf16Reader::default();
        assert!(reader.convert(&[0x3d, 0xd8, 0xff], false).is_empty());
        assert_eq!(reader.convert(&[0, 0xde], false), "😀".as_bytes());
    }
}
