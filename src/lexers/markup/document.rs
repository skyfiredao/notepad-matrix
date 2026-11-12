use super::*;

pub(super) fn scan(text: &str, profile: &LanguageProfile, out: &mut Output) {
    match profile.key() {
        "latex" => latex(text, out),
        "tex" => tex(text, profile, out),
        "txt2tags" => txt2tags(text, out),
        "postscript" => postscript(text, profile, out),
        _ => {}
    }
}

fn latex(text: &str, out: &mut Output) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut math = 0;
    let mut environments: Vec<(String, usize)> = Vec::new();
    while i < b.len() {
        let start = i;
        if b[i] == b'%' {
            i = line_end(b, i);
            out.paint(start, i, 4);
        } else if b[i] == b'$' {
            let double = b.get(i + 1) == Some(&b'$');
            i += if double { 2 } else { 1 };
            out.paint(start, i, 9);
            math = if math == 0 {
                if double { 6 } else { 3 }
            } else if math == 3 || double {
                0
            } else {
                math
            };
        } else if b[i] == b'\\' {
            i += 1;
            if b.get(i).is_some_and(u8::is_ascii_alphabetic) {
                while b
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'@')
                {
                    i += 1;
                }
                let command = &text[start + 1..i];
                out.paint(start, i, 1);
                if matches!(command, "begin" | "end") {
                    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
                        i += 1;
                    }
                    let tag_start = i;
                    if b.get(i) == Some(&b'{') {
                        i = until(b, i + 1, b"}");
                        out.paint(tag_start, i, if command == "begin" { 2 } else { 5 });
                        let env = text.get(tag_start + 1..i.saturating_sub(1)).unwrap_or("");
                        if command == "begin" {
                            environments.push((env.to_owned(), start));
                            if matches!(env, "verbatim" | "verbatim*" | "lstlisting" | "comment") {
                                let close = format!("\\end{{{env}}}");
                                let end = text[i..].find(&close).map_or(text.len(), |n| i + n);
                                out.paint(i, end, if env == "comment" { 7 } else { 8 });
                                i = end;
                            } else if env == "math" {
                                math = 3;
                            } else if matches!(
                                env.trim_end_matches('*'),
                                "align"
                                    | "alignat"
                                    | "flalign"
                                    | "gather"
                                    | "multiline"
                                    | "displaymath"
                                    | "eqnarray"
                                    | "equation"
                            ) {
                                math = 6;
                            }
                        } else {
                            if let Some(position) =
                                environments.iter().rposition(|(name, _)| name == env)
                            {
                                let (_, open) = environments.remove(position);
                                out.block(open, i);
                            }
                            math = 0;
                        }
                    } else {
                        out.paint(tag_start, i.max(tag_start + 1), 12);
                    }
                } else if command == "verb" {
                    if b.get(i) == Some(&b'*') {
                        out.paint(i, i + 1, 1);
                        i += 1;
                    }
                    if let Some(&delim) = b.get(i) {
                        let end = line_end(b, i);
                        let next = b[i + 1..end]
                            .iter()
                            .position(|c| *c == delim)
                            .map_or(end, |n| i + n + 2);
                        out.paint(
                            i,
                            next,
                            if next == end && b.get(next.saturating_sub(1)) != Some(&delim) {
                                12
                            } else {
                                8
                            },
                        );
                        i = next;
                    }
                } else {
                    let opt_start = i;
                    if b.get(i) == Some(&b'*') {
                        i += 1;
                    }
                    while b.get(i).is_some_and(|c| b" \t".contains(c)) {
                        i += 1;
                    }
                    if b.get(i) == Some(&b'[') {
                        let end = until(b, i + 1, b"]");
                        if !b[i + 1..end.saturating_sub(1)]
                            .iter()
                            .any(|c| b"\\$%".contains(c))
                        {
                            out.paint(opt_start, end, 11);
                            i = end;
                        }
                    }
                }
            } else if let Some(&c) = b.get(i) {
                i += 1;
                out.paint(
                    start,
                    i,
                    if b"#$%&~_^{}".contains(&c) {
                        10
                    } else if b"\r\n".contains(&c) {
                        12
                    } else {
                        9
                    },
                );
                match c {
                    b'(' => math = 3,
                    b'[' => math = 6,
                    b')' | b']' => math = 0,
                    _ => {}
                }
            } else {
                out.paint(start, i, 12);
            }
        } else {
            i += 1;
            while b.get(i).is_some_and(|c| !b"\\$%".contains(c)) {
                i += 1;
            }
            out.paint(start, i, math);
        }
    }
    for (_, open) in environments {
        out.block(open, text.len());
    }
}

fn tex(text: &str, profile: &LanguageProfile, out: &mut Output) {
    let b = text.as_bytes();
    let mut i = 0;
    let interface = profile
        .property("lexer.tex.interface.default")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let mut use_keywords = enabled(profile, "lexer.tex.use.keywords", true) && interface != 0;
    let first = text.lines().next().unwrap_or("");
    if first.starts_with('%') && first.contains("interface=all") {
        use_keywords = false;
    }
    out.paint(0, b.len(), 5);
    let mut groups = Vec::new();
    while i < b.len() {
        let start = i;
        match b[i] {
            b'%' if !enabled(profile, "lexer.tex.comment.process", false) => {
                i = line_end(b, i);
                out.paint(start, start + 1, 3);
                out.paint(start + 1, i, 0);
            }
            b'\\' => {
                i += 1;
                while b
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphabetic() || b"@!?".contains(c))
                {
                    i += 1;
                }
                if i == start + 1 {
                    i += text[i..].chars().next().map_or(0, char::len_utf8);
                }
                let command = &text[start + 1..i];
                let known = !use_keywords
                    || profile.keywords(interface.saturating_sub(1)).is_empty()
                    || keyword(profile, interface.saturating_sub(1), command, false)
                    || command.len() == 1
                    || enabled(profile, "lexer.tex.auto.if", true) && command.starts_with("if");
                out.paint(start, i, if known { 4 } else { 5 });
            }
            c if b"{}$".contains(&c) => {
                out.paint(i, i + 1, 2);
                if c == b'{' {
                    groups.push(i);
                } else if c == b'}'
                    && let Some(open) = groups.pop()
                {
                    out.block(open, i + 1);
                }
                i += 1;
            }
            c if b"[]=#()<>\"".contains(&c) => {
                out.paint(i, i + 1, 1);
                i += 1;
            }
            b'^' if b.get(i + 1) == Some(&b'^') => {
                i += 2;
            }
            c if b"~^_&-+`/|%".contains(&c) => {
                out.paint(i, i + 1, 3);
                i += 1;
            }
            _ => i += 1,
        }
    }
}

fn txt2tags(text: &str, out: &mut Output) {
    let mut code = None;
    let mut heading = None;
    for range in lines(text) {
        let s = &text[range.clone()];
        let trim = s.trim_start_matches([' ', '\t']);
        let pad = s.len() - trim.len();
        if let Some(open) = code {
            out.paint(range.start, range.end, 21);
            if s.starts_with("```") {
                out.block(open, range.end);
                code = None;
            }
            continue;
        }
        let style = if s.starts_with("```") {
            code = Some(range.start);
            21
        } else if s.starts_with("%!preproc") {
            24
        } else if s.starts_with("%!postproc") {
            25
        } else if s.starts_with("%!") {
            23
        } else if s.starts_with('%') {
            22
        } else if s.starts_with(['=', '+']) && !s.starts_with("+ ") {
            let first = s.as_bytes()[0];
            let n = s.bytes().take_while(|c| *c == first).count().min(6);
            if let Some(open) = heading {
                out.block(open, range.start);
            }
            heading = Some(range.start);
            5 + n as u16
        } else if trim.trim().len() >= 20 && trim.trim().bytes().all(|c| c == b'-' || c == b'=') {
            17
        } else {
            0
        };
        if style != 0 {
            out.paint(range.start, range.end, style);
            continue;
        }
        let mut i = pad;
        if trim.starts_with("- ") {
            out.paint(range.start + i, range.start + i + 2, 13);
            i += 2;
        } else if trim.starts_with("+ ") {
            out.paint(range.start + i, range.start + i + 2, 14);
            i += 2;
        } else if s.starts_with('\t') {
            out.paint(range.start, range.start + pad, 15);
        }
        let b = s.as_bytes();
        while i < b.len() {
            if b[i] == b'\\' {
                i = (i + 2).min(b.len());
                continue;
            }
            let pair = b.get(i..i + 2).unwrap_or(&[]);
            let style = match pair {
                b"**" => 2,
                b"//" => 4,
                b"__" => 5,
                b"--" => 16,
                b"``" => 20,
                _ => 0,
            };
            if style != 0 && b.get(i + 2).is_some_and(|c| !c.is_ascii_whitespace()) {
                let end = until(b, i + 2, pair).min(line_end(b, i));
                out.paint(range.start + i, range.start + end, style);
                i = end;
            } else if b[i] == b'[' {
                let end = until(b, i + 1, b"]").min(line_end(b, i));
                out.paint(range.start + i, range.start + end, 18);
                i = end;
            } else if b[i] == b'|' {
                let end = until(b, i + 1, b"|").min(line_end(b, i));
                out.paint(range.start + i, range.start + end, 19);
                i = end;
            } else {
                i += 1;
            }
        }
    }
    if let Some(open) = code {
        out.block(open, text.len());
    }
    if let Some(open) = heading {
        out.block(open, text.len());
    }
}

fn postscript(text: &str, profile: &LanguageProfile, out: &mut Output) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut blocks = Vec::new();
    while i < b.len() {
        let start = i;
        let style;
        if b[i] == b'%' {
            i = line_end(b, i);
            if b[start..].starts_with(b"%%") && (start == 0 || b"\r\n".contains(&b[start - 1])) {
                let split = text[start..i].find(':').map_or(i, |n| start + n + 1);
                out.paint(start, split, 2);
                out.paint(split, i, 3);
                continue;
            }
            style = 1;
        } else if b[i] == b'(' {
            i += 1;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                match b[i] {
                    b'\\' => {
                        i = (i + 2).min(b.len());
                        continue;
                    }
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            style = 12;
            out.block(start, i);
        } else if b[i..].starts_with(b"<~") || b[i] == b'<' && !b[i..].starts_with(b"<<") {
            let base85 = b[i..].starts_with(b"<~");
            let begin = i + if base85 { 2 } else { 1 };
            i = until(b, begin, if base85 { b"~>" } else { b">" });
            out.paint(start, i, if base85 { 14 } else { 13 });
            let content_end = i.saturating_sub(if base85 { 2 } else { 1 });
            for (p, &byte) in b.iter().enumerate().take(content_end).skip(begin) {
                if !byte.is_ascii_whitespace()
                    && if base85 {
                        !((b'!'..=b'u').contains(&byte) || byte == b'z')
                    } else {
                        !byte.is_ascii_hexdigit()
                    }
                {
                    out.paint(p, p + 1, 15);
                }
            }
            continue;
        } else if b[i..].starts_with(b"<<") || b[i..].starts_with(b">>") {
            i += 2;
            style = 10;
        } else if b"[]{}".contains(&b[i]) {
            let c = b[i];
            style = if b"[]".contains(&c) { 9 } else { 11 };
            i += 1;
            if c == b'{' {
                blocks.push(start);
            } else if c == b'}'
                && let Some(open) = blocks.pop()
            {
                out.block(open, i);
            }
        } else if b[i] == b'/' {
            i += 1;
            let immediate = b.get(i) == Some(&b'/');
            if immediate {
                i += 1;
            }
            while b
                .get(i)
                .is_some_and(|c| !c.is_ascii_whitespace() && !b"[]{}()/<>%".contains(c))
            {
                i += 1;
            }
            style = if immediate { 8 } else { 7 };
        } else if b[i].is_ascii_whitespace() {
            i += 1;
            style = 0;
        } else if b")>".contains(&b[i]) {
            i += 1;
            style = 15;
        } else {
            i += 1;
            while b
                .get(i)
                .is_some_and(|c| !c.is_ascii_whitespace() && !b"[]{}()/<>%".contains(c))
            {
                i += 1;
            }
            let token = &text[start..i];
            let radix = token.split_once('#').is_some_and(|(base, digits)| {
                base.parse::<u32>()
                    .ok()
                    .filter(|base| (2..=36).contains(base))
                    .is_some_and(|base| {
                        !digits.is_empty() && digits.chars().all(|c| c.is_digit(base))
                    })
            });
            let level = profile
                .property("ps.level")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(3);
            style = if token.parse::<f64>().is_ok() || radix {
                4
            } else if (0..5).any(|n| (n >= 3 || n < level) && keyword(profile, n, token, false)) {
                6
            } else {
                5
            };
        }
        out.paint(start, i, style);
    }
}
