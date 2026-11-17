pub fn parse_value(text: &str, radix: u32) -> Result<u64, &'static str> {
    if !matches!(radix, 2 | 8 | 10 | 16) {
        return Err("Invalid numeric format");
    }
    if text.is_empty() {
        return Ok(0);
    }
    let text = text.trim_start_matches([' ', '\t', '\r', '\n', '\u{b}', '\u{c}']);
    let (negative, text) = if let Some(text) = text.strip_prefix('-') {
        (true, text)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let text = if radix == 16 {
        text.strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .unwrap_or(text)
    } else {
        text
    };
    let magnitude = u32::from_str_radix(text, radix)
        .map_err(|_| "Invalid numeric entry for the selected format")?;
    let signed = if negative { -i64::from(magnitude) } else { i64::from(magnitude) };
    let value = i32::try_from(signed).map_err(|_| "Invalid numeric entry for the selected format")?;
    if value == -1 {
        return Err("Invalid numeric entry for the selected format");
    }
    Ok(i64::from(value) as u64)
}

pub fn format_value(value: u64, radix: u32, uppercase: bool) -> String {
    match radix {
        2 => format!("{value:b}"),
        8 => format!("{value:o}"),
        16 if uppercase => format!("{value:X}"),
        16 => format!("{value:x}"),
        _ => (value as u32).to_string(),
    }
}
