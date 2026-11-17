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

