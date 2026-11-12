pub fn parse_value(text: &str, radix: u32) -> Result<u64, &'static str> {
    if !matches!(radix, 2 | 8 | 10 | 16) {
        return Err("Invalid numeric format");
    }
    let text = text.trim_start();
    if text.is_empty() {
        return Ok(0);
    }
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
    let value = u64::from_str_radix(text, radix)
        .map_err(|_| "Invalid numeric entry for the selected format")?;
    if negative && value == 1 {
        return Err("Invalid numeric entry for the selected format");
    }
    Ok(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}

pub fn format_value(value: u64, radix: u32, uppercase: bool) -> String {
    match radix {
        2 => format!("{value:b}"),
        8 => format!("{value:o}"),
        16 if uppercase => format!("{value:X}"),
        16 => format!("{value:x}"),
        _ => value.to_string(),
    }
}
