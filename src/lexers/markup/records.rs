use super::*;

pub(super) fn scan(text: &str, profile: &LanguageProfile, out: &mut Output) {
    match profile.key() {
        "diff" => diff(text, out),
        "errorlist" => errors(text, profile, out),
        "escseq" => escapes(text, profile, out),
        "registry" => registry(text, out),
        "ihex" | "srec" | "tehex" => hex(text, profile.key(), out),
        "searchResult" => search(text, out),
        "nfo" => out.paint(0, text.len(), 0),
        _ => {}
    }
}

fn diff(text: &str, out: &mut Output) {
    let mut level = 0;
    for range in lines(text) {
        let s = text[range.clone()].trim_end_matches(['\r', '\n']);
        let numeric_header = s
            .get(4..)
            .is_some_and(|tail| tail.starts_with(|c: char| c.is_ascii_digit()) && !s.contains('/'));
        let style = if s.starts_with("diff ") || s.starts_with("Index: ") {
            2
        } else if s == "---"
            || s.starts_with("@@")
            || s.starts_with("********")
            || ((s.starts_with("--- ") || s.starts_with("+++ ") || s.starts_with("*** "))
                && numeric_header)
            || s.starts_with(|c: char| c.is_ascii_digit())
        {
            4
        } else if s.starts_with("--- ")
            || s.starts_with("+++ ")
            || s.starts_with("***")
            || s.starts_with("====")
            || s.starts_with("? ")
        {
            3
        } else if s.starts_with("++") {
            8
        } else if s.starts_with("+-") {
            9
        } else if s.starts_with("-+") {
            10
        } else if s.starts_with("--") {
            11
        } else if s.starts_with(['-', '<']) {
            5
        } else if s.starts_with(['+', '>']) {
            6
        } else if s.starts_with('!') {
            7
        } else if s.starts_with(' ') {
            0
        } else {
            1
        };
        out.paint(range.start, range.end, style);
        let n = out.line(range.start);
        let header = matches!(style, 2..=4);
        if header {
            level = match style {
                2 => 0,
                3 => 1,
                _ => 2,
            };
        }
        out.levels[n] = (level, header);
        if header {
            level += 1;
        }
    }
}

fn errors(text: &str, profile: &LanguageProfile, out: &mut Output) {
    for range in lines(text) {
        let s = text[range.clone()].trim_end_matches(['\r', '\n']);
        let (style, value) = diagnostic(s);
        out.paint(range.start, range.end, style);
        if enabled(profile, "lexer.errorlist.value.separate", false)
            && style == 2
            && let Some(pos) = value
        {
            out.paint(range.start + pos, range.end, 21);
        }
    }
    if enabled(profile, "lexer.errorlist.escape.sequences", false) {
        ansi(text, true, true, out);
    }
}

fn diagnostic(s: &str) -> (u16, Option<usize>) {
    let b = s.as_bytes();
    let trimmed = s.trim_start();
    let style = if s.starts_with('>') {
        4
    } else if s.starts_with('<') {
        12
    } else if s.starts_with('!') {
        10
    } else if s.starts_with("+++ ") || s.starts_with("--- ") {
        13
    } else if s.starts_with('+') {
        11
    } else if s.starts_with('-') {
        12
    } else if s.starts_with("cf90-") {
        18
    } else if s.starts_with("fortcom:") {
        17
    } else if s.contains("File \"") && s.contains(", line ") {
        1
    } else if s.contains(" in ") && s.contains(" on line ") {
        14
    } else if (s.starts_with("Error ") || s.starts_with("Warning "))
        && s.find(" at (").is_some_and(|p| s[p..].contains(") : "))
    {
        16
    } else if s.starts_with("Error ") || s.starts_with("Warning ") {
        5
    } else if s.contains("at line ") && s.contains("file ") {
        8
    } else if s
        .find(" at ")
        .is_some_and(|p| s[p + 4..].contains(" line "))
    {
        6
    } else if s.starts_with("   at ") && s.contains(":line ") {
        7
    } else if s.starts_with("Line ") && s.contains(", file ") {
        15
    } else if s.starts_with("line ") && s.contains(" column ") {
        19
    } else if trimmed.starts_with("at ") && s.contains('(') && s.contains(".java:") {
        20
    } else if s.starts_with("In file included from ") || s.starts_with("                 from ") {
        22
    } else if s.starts_with("NMAKE : fatal error")
        || s.contains("warning LNK")
        || s.contains("error LNK")
    {
        3
    } else if s.find(": line ").is_some_and(|p| {
        let tail = &b[p + 7..];
        let n = tail.iter().take_while(|c| c.is_ascii_digit()).count();
        n > 0 && tail.get(n) == Some(&b':')
    }) {
        26
    } else if b.iter().enumerate().any(|(i, c)| {
        *c == b'|'
            && i > 0
            && b[i - 1] == b' '
            && b[..i - 1]
                .iter()
                .all(|c| c.is_ascii_digit() || b" +".contains(c))
            && b.get(i + 1).is_some_and(|c| b" +".contains(c))
    }) {
        25
    } else {
        0
    };
    if style != 0 {
        return (style, None);
    }
    for (i, &c) in b.iter().enumerate() {
        if c == b':' {
            let mut j = i + 1;
            if b.get(j) == Some(&b'-') {
                j += 1;
            }
            let digits = j;
            while b.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            if j > digits && b.get(j) == Some(&b':') {
                j += 1;
                let column_start = j;
                while b.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                let value = if j > column_start && b.get(j) == Some(&b':') {
                    j + 1
                } else {
                    column_start
                };
                let lua = s.starts_with('\t') || s[..i].contains(": ") || s[..i].ends_with(".lua");
                return (if lua { 8 } else { 2 }, Some(value));
            }
        } else if c == b'('
            && !s.starts_with('\t')
            && b.get(i + 1).is_some_and(|c| (b'1'..=b'9').contains(c))
        {
            let mut j = i + 1;
            while b
                .get(j)
                .is_some_and(|c| c.is_ascii_digit() || b" ,".contains(c))
            {
                j += 1;
            }
            if b.get(j) == Some(&b')') {
                let location = &s[i + 1..j];
                let message = s[j + 1..].trim_start().trim_start_matches(':').trim_start();
                let severity = message
                    .split(|c: char| !c.is_ascii_alphabetic())
                    .next()
                    .unwrap_or("");
                if location.contains(',')
                    || [
                        "error",
                        "warning",
                        "fatal",
                        "catastrophic",
                        "note",
                        "remark",
                    ]
                    .iter()
                    .any(|word| severity.eq_ignore_ascii_case(word))
                {
                    return (3, None);
                }
            }
        }
    }
    let mut fields = s.splitn(3, '\t');
    if let (Some(symbol), Some(_), Some(address)) = (fields.next(), fields.next(), fields.next())
        && !symbol.is_empty()
        && !symbol.contains(' ')
        && (address.starts_with("/^") || address.starts_with(|c: char| c.is_ascii_digit()))
    {
        return (9, None);
    }
    (0, None)
}

fn escapes(text: &str, profile: &LanguageProfile, out: &mut Output) {
    ansi(
        text,
        enabled(profile, "lexer.escseq.colour.text", false),
        false,
        out,
    );
}

fn ansi(text: &str, colour: bool, errors: bool, out: &mut Output) {
    let b = text.as_bytes();
    for range in lines(text) {
        let mut i = range.start;
        let mut style = 0;
        while i < range.end {
            if b[i..].starts_with(b"\x1b[") {
                let start = i;
                i += 2;
                let params = i;
                while i < range.end && !(b[i] >= b'@' && b[i] <= b'~') && !b"\r\n".contains(&b[i]) {
                    i += 1;
                }
                let end = b.get(i).copied().unwrap_or(0);
                let recognized = matches!(end, b'm' | b'K');
                if i < range.end && !b"\r\n".contains(&end) {
                    i += 1;
                }
                out.paint(
                    start,
                    i,
                    if errors {
                        if recognized { 23 } else { 24 }
                    } else if recognized {
                        171
                    } else {
                        172
                    },
                );
                if !recognized {
                    style = 0;
                }
                if end == b'm' && colour {
                    let mut fore = 0;
                    let mut back = 0;
                    let mut bold = false;
                    for p in text[params..i.saturating_sub(1)].split(';') {
                        match p.parse::<u16>().unwrap_or(0) {
                            0 => {
                                fore = 0;
                                back = 0;
                                bold = false;
                            }
                            1 => bold = true,
                            22 => bold = false,
                            n @ 30..=37 => fore = n - 29,
                            n @ 90..=97 => {
                                fore = n - 89;
                                bold = true;
                            }
                            n @ 40..=47 => back = n - 39,
                            n @ 100..=107 => back = n - 99,
                            _ => {}
                        }
                    }
                    if errors {
                        style = if fore > 0 {
                            39 + fore + if bold { 8 } else { 0 }
                        } else {
                            0
                        };
                    } else {
                        style = fore + back * 9 + if bold { 81 } else { 0 };
                        if style >= 31 {
                            style += 9;
                        }
                    }
                }
            } else {
                let start = i;
                i += 1;
                while i < range.end && !b[i..].starts_with(b"\x1b[") {
                    i += 1;
                }
                if colour && (!errors || style != 0) {
                    out.paint(start, i, style);
                }
            }
        }
    }
}

fn registry(text: &str, out: &mut Output) {
    let b = text.as_bytes();
    let mut key_start = None;
    let mut continued = false;
    for range in lines(text) {
        let end = line_end(b, range.start);
        let mut i = range.start;
        while i < end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut after_eq = continued;
        if b.get(i) == Some(&b';') {
            out.paint(i, range.end, 1);
            continue;
        }
        if b.get(i) == Some(&b'[') {
            if let Some(open) = key_start {
                out.block(open, range.start);
            }
            key_start = Some(i);
            out.paint(i, end, if b.get(i + 1) == Some(&b'-') { 7 } else { 6 });
            guids(b, i, end, 9, out);
            continue;
        }
        while i < end {
            let start = i;
            match b[i] {
                b'"' => {
                    i = quoted(&b[..end], i, b'"', true, false);
                    let value_name = text[i..end].trim_start().starts_with('=');
                    out.paint(start, i, if value_name { 2 } else { 3 });
                    let mut p = start + 1;
                    while p < i {
                        if b[p] == b'\\' {
                            out.paint(p, (p + 2).min(i), 8);
                            p += 2;
                        } else if b[p] == b'%'
                            && b.get(p + 1)
                                .is_some_and(|c| c.is_ascii_digit() || *c == b'*')
                        {
                            out.paint(p, p + 2, 11);
                            p += 2;
                        } else {
                            p += 1;
                        }
                    }
                    guids(b, start, i, 10, out);
                }
                b'=' => {
                    out.paint(i, i + 1, 12);
                    after_eq = true;
                    i += 1;
                }
                b'@' => {
                    out.paint(i, i + 1, 2);
                    i += 1;
                }
                c if c.is_ascii_alphabetic()
                    && after_eq
                    && text[i..end].find(':').is_some_and(|p| p < 10) =>
                {
                    i = text[i..end].find(':').map_or(end, |p| i + p);
                    out.paint(start, i, 5);
                }
                c if c.is_ascii_hexdigit() && after_eq => {
                    while i < end && b[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    out.paint(start, i, 4);
                }
                c if b"-,.:\\()".contains(&c) => {
                    out.paint(i, i + 1, 12);
                    i += 1;
                }
                _ => i += 1,
            }
        }
        continued = text[range.start..end].trim_end().ends_with('\\');
    }
    if let Some(open) = key_start {
        out.block(open, text.len());
    }
}

fn guids(b: &[u8], start: usize, end: usize, style: u16, out: &mut Output) {
    for i in start..end {
        if b[i] == b'{'
            && i + 38 <= end
            && b[i + 37] == b'}'
            && b[i + 1..i + 37].iter().enumerate().all(|(p, c)| {
                if [8, 13, 18, 23].contains(&p) {
                    *c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
        {
            out.paint(i, i + 38, style);
        }
    }
}

fn byte(b: &[u8], i: usize) -> Option<usize> {
    let pair = b.get(i..i + 2)?;
    Some((pair[0] as char).to_digit(16)? as usize * 16 + (pair[1] as char).to_digit(16)? as usize)
}

fn hex(text: &str, key: &str, out: &mut Output) {
    let b = text.as_bytes();
    let mut extended = false;
    for range in lines(text) {
        let start = range.start;
        let end = line_end(b, start);
        let r = &b[start..end];
        if r.first().copied()
            != Some(match key {
                "ihex" => b':',
                "srec" => b'S',
                _ => b'%',
            })
        {
            if key == "ihex" && extended {
                let n = out.line(start);
                out.levels[n].0 = 1;
            }
            continue;
        }
        out.paint(start, start + 1, 1);
        if key == "tehex" {
            let count = byte(r, 1).unwrap_or(0);
            let typ = r.get(3).copied().unwrap_or(0);
            out.paint(
                start + 1,
                (start + 3).min(end),
                if count == r.len().saturating_sub(1) {
                    4
                } else {
                    5
                },
            );
            out.paint(
                start + 3,
                (start + 4).min(end),
                if matches!(typ, b'6' | b'8') { 2 } else { 3 },
            );
            let sum = r
                .iter()
                .enumerate()
                .filter(|(i, _)| *i > 0 && *i != 4 && *i != 5 && *i <= count)
                .try_fold(0usize, |sum, (_, c)| {
                    (*c as char).to_digit(16).map(|n| sum + n as usize)
                });
            let valid = sum.is_some_and(|sum| byte(r, 4) == Some(sum & 255));
            out.paint(start + 4, (start + 6).min(end), if valid { 16 } else { 17 });
            out.paint(
                start + 6,
                (start + 15).min(end),
                match typ {
                    b'6' => 7,
                    b'8' => 9,
                    _ => 10,
                },
            );
            for p in 15..r.len().min(count + 1) {
                out.paint(
                    start + p,
                    start + p + 1,
                    if (p - 15) / 2 % 2 == 0 { 12 } else { 13 },
                );
            }
            out.paint(start + count + 1, end, 18);
            continue;
        }
        let intel = key == "ihex";
        let count_pos = if intel { 1 } else { 2 };
        let count = byte(r, count_pos).unwrap_or(0);
        let typ = if intel {
            byte(r, 7).unwrap_or(255)
        } else {
            r.get(1).copied().unwrap_or(b'?').wrapping_sub(b'0') as usize
        };
        let addr_len = if intel {
            4
        } else {
            match typ {
                0 | 1 | 5 | 9 => 4,
                2 | 6 | 8 => 6,
                3 | 7 => 8,
                _ => 0,
            }
        };
        let addr_start = if intel { 3 } else { 4 };
        let data_start = if intel { 9 } else { addr_start + addr_len };
        let declared_check = if intel { 9 + count * 2 } else { 2 + count * 2 };
        let expected = declared_check + 2;
        let required = if intel {
            match typ {
                1 => 0,
                2 | 4 => 2,
                3 | 5 => 4,
                _ => count,
            }
        } else if matches!(typ, 5..=9) {
            addr_len / 2 + 1
        } else {
            count
        };
        let data_count = if intel {
            required
        } else {
            required.saturating_sub(addr_len / 2 + 1)
        };
        let check = data_start + data_count * 2;
        out.paint(
            start + count_pos,
            (start + count_pos + 2).min(end),
            if count == required && (r.len() == expected || r.len() + 1 == expected) {
                4
            } else {
                5
            },
        );
        let type_pos = if intel { 7 } else { 1 };
        out.paint(
            start + type_pos,
            (start + type_pos + if intel { 2 } else { 1 }).min(end),
            if if intel {
                typ <= 5
            } else {
                typ <= 9 && typ != 4
            } {
                2
            } else {
                3
            },
        );
        let address_style = if intel {
            if typ == 0 {
                7
            } else if typ <= 5 {
                6
            } else {
                10
            }
        } else {
            match typ {
                0 => 6,
                1..=3 => 7,
                5 | 6 => 8,
                7..=9 => 9,
                _ => 10,
            }
        };
        out.paint(
            start + addr_start,
            (start + addr_start + addr_len).min(end),
            address_style,
        );
        let data_style = if intel {
            match typ {
                0 => 12,
                1 => 15,
                2 | 4 => 11,
                3 | 5 => 9,
                _ => 14,
            }
        } else {
            match typ {
                0..=3 => 12,
                5..=9 => 15,
                _ => 14,
            }
        };
        for p in data_start..check.min(r.len()) {
            out.paint(
                start + p,
                start + p + 1,
                if data_style == 12 && (p - data_start) / 2 % 2 != 0 {
                    13
                } else {
                    data_style
                },
            );
        }
        let sum = (count_pos..declared_check)
            .step_by(2)
            .try_fold(0usize, |sum, p| byte(r, p).map(|value| sum + value));
        let checksum = sum
            .and_then(|sum| {
                byte(r, declared_check).map(|c| (sum + c) & 255 == if intel { 0 } else { 255 })
            })
            .unwrap_or(false);
        out.paint(
            start + check,
            (start + check + 2).min(end),
            if checksum { 16 } else { 17 },
        );
        out.paint(start + check + 2, end, 18);
        if intel {
            let n = out.line(start);
            if matches!(typ, 2 | 4) {
                extended = true;
                out.levels[n] = (0, true);
            } else if typ == 0 && extended {
                out.levels[n].0 = 1;
            } else {
                extended = false;
            }
        }
    }
}

fn search(text: &str, out: &mut Output) {
    for range in lines(text) {
        let s = &text[range.clone()];
        let line = out.line(range.start);
        if s.starts_with(' ') {
            out.paint(range.start, range.end, 2);
            out.levels[line] = (1, true);
        } else if s.starts_with('\t') {
            out.levels[line].0 = 2;
            if let Some(first) = s.find(|c: char| c.is_ascii_digit())
                && let Some(last) = s[first..].find(':')
            {
                out.paint(range.start + first, range.start + first + last, 3);
            }
        } else {
            out.paint(range.start, range.end, 1);
            out.levels[line] = (0, true);
        }
    }
}
