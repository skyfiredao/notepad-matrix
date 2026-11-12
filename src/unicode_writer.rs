use std::io::{self, Write};

pub(crate) fn write_utf16(
    bytes: impl IntoIterator<Item = u8>,
    big_endian: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut code = 0u32;
    let mut remaining = 0;
    let mut emit = |unit: u16| {
        output.write_all(&if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        })
    };
    for byte in bytes {
        if remaining == 0 {
            if byte < 0x80 {
                emit(u16::from(byte))?;
                continue;
            }
            (code, remaining) = if byte < 0xe0 {
                (u32::from(byte & 0x1f), 1)
            } else if byte < 0xf0 {
                (u32::from(byte & 0x0f), 2)
            } else {
                (u32::from(byte & 7), 3)
            };
        } else {
            code = (code << 6) | u32::from(byte & 0x3f);
            remaining -= 1;
            if remaining == 0 {
                if code < 0x10000 {
                    emit(code as u16)?;
                } else {
                    code -= 0x10000;
                    emit((0xd800 | (code >> 10)) as u16)?;
                    emit((0xdc00 | (code & 0x3ff)) as u16)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_utf8_iterator_keeps_permissive_bit_patterns_and_drops_incomplete_tail() {
        let cases: &[(&[u8], &[u16])] = &[
            (
                "A\0\r\n中😀".as_bytes(),
                &[0x41, 0, 13, 10, 0x4e2d, 0xd83d, 0xde00],
            ),
            (&[0x80, 0x41], &[1]),
            (&[0xdf, 0xff], &[0x7ff]),
            (&[0xc0, 0x80], &[0]),
            (&[0xe2, 0x41, 0x42], &[0x2042]),
            (&[0xed, 0xa0, 0x80], &[0xd800]),
            (&[0xf4, 0x90, 0x80, 0x80], &[0xdc00, 0xdc00]),
            (&[0xff, 0xff, 0xff, 0xff], &[0xdfbf, 0xdfff]),
            (&[b'a', 0xf0, 0x9f, 0x98], &[0x61]),
        ];
        for &(input, expected) in cases {
            for big_endian in [false, true] {
                let mut output = Vec::new();
                write_utf16(input.iter().copied(), big_endian, &mut output).unwrap();
                let expected: Vec<u8> = expected
                    .iter()
                    .flat_map(|unit| {
                        if big_endian {
                            unit.to_be_bytes()
                        } else {
                            unit.to_le_bytes()
                        }
                    })
                    .collect();
                assert_eq!(output, expected, "{input:02x?}, big_endian={big_endian}");
            }
        }
    }

    #[test]
    fn source_utf8_iterator_spans_chunks_and_propagates_write_failure() {
        let chunks: &[&[u8]] = &[&[0xf0], &[0x9f, 0x98], &[0x80], &[0xed, 0xa0], &[0x80]];
        let mut output = Vec::new();
        write_utf16(
            chunks.iter().flat_map(|chunk| chunk.iter().copied()),
            false,
            &mut output,
        )
        .unwrap();
        assert_eq!(output, [0x3d, 0xd8, 0, 0xde, 0, 0xd8]);
        let error =
            write_utf16(*b"x", false, &mut std::io::Cursor::new(&mut [0u8; 1][..])).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WriteZero);
    }
}
