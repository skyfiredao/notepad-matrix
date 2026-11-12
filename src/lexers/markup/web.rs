use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Php,
    Js,
    Vb,
    Python,
}

pub(super) fn markup(text: &str, profile: &LanguageProfile, out: &mut Output) -> Vec<TagToken> {
    let b = text.as_bytes();
    let xml = profile.key() == "xml";
    let sensitive = enabled(profile, "html.tags.case.sensitive", xml);
    let mut i = 0;
    let mut tokens = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut asp_language = match profile.property("asp.default.language") {
        Some("1") => Script::Js,
        Some("3") => Script::Python,
        _ => Script::Vb,
    };
    while i < b.len() {
        let start = i;
        if b[i..].starts_with(b"<%@") {
            i = until(b, i + 3, b"%>");
            let directive = text[start..i].to_ascii_lowercase();
            if directive.contains("javascript") {
                asp_language = Script::Js;
            } else if directive.contains("python") {
                asp_language = Script::Python;
            } else if directive.contains("vbscript") {
                asp_language = Script::Vb;
            }
            out.paint(start, i, 16);
            continue;
        }
        if let Some(end) = server(text, i, profile, out, asp_language) {
            i = end;
            continue;
        }
        if b[i..].starts_with(b"<!--") {
            i = until(b, i + 4, b"-->");
            out.paint(start, i, 9);
            if enabled(profile, "fold.hypertext.comment", false) {
                out.block(start, i);
            }
        } else if b[i..].starts_with(b"<![CDATA[") {
            i = until(b, i + 9, b"]]>");
            out.paint(start, i, 17);
            out.block(start, i);
        } else if b[i..].starts_with(b"<!") {
            i = dtd(b, i, out);
            out.block(start, i);
        } else if b[i..].starts_with(b"<?") {
            let end = until(b, i + 2, b"?>");
            let xml_decl = b
                .get(i + 2..i + 5)
                .is_some_and(|s| s.eq_ignore_ascii_case(b"xml"));
            out.paint(i, (i + 2).min(end), if xml_decl { 12 } else { 18 });
            i += 2;
            while i < end.saturating_sub(2) {
                let at = i;
                if matches!(b[i], b'\'' | b'"') {
                    i = quoted(&b[..end], i, b[i], false, false);
                    out.paint(at, i, if b[at] == b'"' { 6 } else { 7 });
                } else if name(b[i]) {
                    while i < end && name(b[i]) {
                        i += 1;
                    }
                    out.paint(at, i, 3);
                } else {
                    out.paint(i, i + 1, 8);
                    i += 1;
                }
            }
            out.paint(end.saturating_sub(2), end, if xml_decl { 13 } else { 18 });
            i = end;
        } else if b[i] == b'&' {
            i = entity(b, i, out);
        } else if b[i] == b'<'
            && b.get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'_')
        {
            i += 1;
            let closing = b.get(i) == Some(&b'/');
            if closing {
                i += 1;
            }
            let name_start = i;
            while b.get(i).is_some_and(|c| name(*c)) {
                i += 1;
            }
            let name_end = i;
            let raw_name = &text[name_start..name_end];
            let tag = if sensitive {
                raw_name.to_owned()
            } else {
                raw_name.to_ascii_lowercase()
            };
            let known = profile.keywords(0).is_empty()
                || keyword(profile, 0, &tag, false)
                || !xml && tag.contains('-');
            let tag_style = if known { 1 } else { 2 };
            out.paint(start, i, tag_style);
            let mut self_closing = false;
            let mut attributes = Vec::new();
            let mut script_language = Script::Js;
            let mut script_enabled = true;
            while i < b.len() {
                if let Some(end) = server(text, i, profile, out, asp_language) {
                    i = end;
                    continue;
                }
                let at = i;
                if b[i..].starts_with(b"/>") {
                    self_closing = true;
                    out.paint(i, i + 2, 11);
                    i += 2;
                    break;
                }
                if b[i] == b'>' {
                    out.paint(i, i + 1, tag_style);
                    i += 1;
                    break;
                }
                if b[i].is_ascii_whitespace() {
                    i += 1;
                    continue;
                }
                if b[i] == b'<' {
                    break;
                }
                if name(b[i]) {
                    while b.get(i).is_some_and(|c| name(*c)) {
                        i += 1;
                    }
                    let attr = text[at..i].to_ascii_lowercase();
                    attributes.push(at..i);
                    let style = if b[at].is_ascii_digit() {
                        5
                    } else if profile.keywords(0).is_empty() || keyword(profile, 0, &attr, false) {
                        3
                    } else {
                        4
                    };
                    out.paint(at, i, style);
                    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
                        i += 1;
                    }
                    if b.get(i) == Some(&b'=') {
                        out.paint(i, i + 1, 8);
                        i += 1;
                        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
                            i += 1;
                        }
                        let value_start = i;
                        if b.get(i).is_some_and(|c| b"\"'".contains(c)) {
                            let q = b[i];
                            i += 1;
                            let mut segment = value_start;
                            while i < b.len() && b[i] != q {
                                if b[i..].starts_with(b"<?") || b[i..].starts_with(b"<%") {
                                    out.paint(segment, i, if q == b'"' { 6 } else { 7 });
                                    if let Some(end) = server(text, i, profile, out, asp_language) {
                                        i = end;
                                        segment = i;
                                        continue;
                                    }
                                }
                                i += 1;
                            }
                            if b.get(i) == Some(&q) {
                                i += 1;
                            }
                            out.paint(segment, i, if q == b'"' { 6 } else { 7 });
                        } else {
                            while b.get(i).is_some_and(|c| {
                                !c.is_ascii_whitespace() && *c != b'>' && !b[i..].starts_with(b"/>")
                            }) {
                                i += 1;
                            }
                            out.paint(value_start, i, 19);
                        }
                        let value = text[value_start..i]
                            .trim_matches(['\'', '"'])
                            .to_ascii_lowercase();
                        if matches!(attr.as_str(), "language" | "type") {
                            if value.contains("vbscript") {
                                script_language = Script::Vb;
                            } else if value.contains("python") {
                                script_language = Script::Python;
                            } else if value.contains("php") {
                                script_language = Script::Php;
                            } else if value.contains("javascript")
                                || value.contains("ecmascript")
                                || value == "module"
                                || value.is_empty()
                            {
                                script_language = Script::Js;
                            } else {
                                script_enabled = false;
                            }
                        }
                        if attr == "runat" && value == "server" {
                            script_language = asp_language;
                        }
                    }
                } else {
                    out.paint(i, i + 1, 8);
                    i += 1;
                }
            }
            let void = !xml
                && matches!(
                    tag.as_str(),
                    "area"
                        | "base"
                        | "br"
                        | "col"
                        | "embed"
                        | "hr"
                        | "img"
                        | "input"
                        | "link"
                        | "meta"
                        | "param"
                        | "source"
                        | "track"
                        | "wbr"
                );
            tokens.push(TagToken {
                range: start..i,
                name_range: name_start..name_end,
                name: tag.clone(),
                closing,
                self_closing: self_closing || void,
                attributes,
            });
            if closing {
                if let Some(n) = stack.iter().rposition(|(name, _)| *name == tag) {
                    let (_, open) = stack.remove(n);
                    out.block(open, i);
                    stack.truncate(n);
                }
            } else if !self_closing && !void {
                stack.push((tag.clone(), start));
                let scripts_allowed = !xml || enabled(profile, "lexer.xml.allow.scripts", false);
                if tag == "script" && scripts_allowed && script_enabled {
                    i = script(text, i, profile, out, script_language, false, "</script");
                } else if tag == "style" && scripts_allowed {
                    let end = find_case_insensitive(b, i, b"</style").unwrap_or(b.len());
                    css(&text[i..end], profile, out, i);
                    i = end;
                }
            }
        } else {
            i += 1;
        }
    }
    for (_, open) in stack {
        out.block(open, text.len());
    }
    tokens
}

fn find_case_insensitive(b: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    b.get(start..)?
        .windows(needle.len())
        .position(|s| s.eq_ignore_ascii_case(needle))
        .map(|n| n + start)
}

fn entity(b: &[u8], start: usize, out: &mut Output) -> usize {
    let mut i = start + 1;
    if b.get(i) == Some(&b'#') {
        i += 1;
    }
    while b
        .get(i)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        i += 1;
    }
    if b.get(i) == Some(&b';') {
        i += 1;
        out.paint(start, i, 10);
    }
    i
}

fn dtd(b: &[u8], start: usize, out: &mut Output) -> usize {
    let mut i = start + 2;
    out.paint(start, i, 21);
    let cmd = i;
    while b.get(i).is_some_and(|c| c.is_ascii_alphabetic()) {
        i += 1;
    }
    out.paint(cmd, i, 22);
    let mut depth = 0;
    let mut first = true;
    while i < b.len() {
        let at = i;
        if b[i..].starts_with(b"<!--") {
            i = until(b, i + 4, b"-->");
            out.paint(at, i, 29);
        } else if b[i..].starts_with(b"--") {
            i = until(b, i + 2, b"--");
            out.paint(at, i, 29);
        } else if matches!(b[i], b'\'' | b'"') {
            i = quoted(b, i, b[i], false, false);
            out.paint(at, i, if b[at] == b'"' { 24 } else { 25 });
        } else if b[i] == b'[' {
            depth += 1;
            out.paint(i, i + 1, 31);
            i += 1;
        } else if b[i] == b']' {
            depth -= 1;
            out.paint(i, i + 1, 31);
            i += 1;
        } else if b[i] == b'>' && depth <= 0 {
            out.paint(i, i + 1, 21);
            return i + 1;
        } else if b[i] == b'%' || b[i] == b'&' {
            i = until(b, i + 1, b";");
            out.paint(at, i, 28);
        } else if b[i] == b'#' {
            i += 1;
            while b.get(i).is_some_and(|c| name(*c)) {
                i += 1;
            }
            out.paint(at, i, 27);
        } else if name(b[i]) {
            while b.get(i).is_some_and(|c| name(*c)) {
                i += 1;
            }
            out.paint(
                at,
                i,
                if first {
                    first = false;
                    23
                } else if depth > 0 {
                    31
                } else {
                    21
                },
            );
        } else {
            out.paint(i, i + 1, if depth > 0 { 31 } else { 21 });
            i += 1;
        }
    }
    i
}

fn server(
    text: &str,
    start: usize,
    profile: &LanguageProfile,
    out: &mut Output,
    asp: Script,
) -> Option<usize> {
    let b = text.as_bytes();
    let xml = profile.key() == "xml";
    let php_allow = profile
        .property(if xml {
            "lexer.xml.allow.php"
        } else {
            "lexer.html.allow.php"
        })
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(2);
    let php_full = b[start..]
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case(b"<?php"));
    let php = php_allow > 0
        && (php_full
            || b[start..].starts_with(b"<?=")
            || php_allow == 2
                && b[start..].starts_with(b"<?")
                && !b[start..]
                    .get(2..5)
                    .is_some_and(|s| s.eq_ignore_ascii_case(b"xml")));
    let asp_allow = enabled(
        profile,
        if xml {
            "lexer.xml.allow.asp"
        } else {
            "lexer.html.allow.asp"
        },
        true,
    );
    if php {
        let open = if php_full {
            5
        } else if b[start..].starts_with(b"<?=") {
            3
        } else {
            2
        };
        out.paint(start, start + open, 18);
        let end = script(text, start + open, profile, out, Script::Php, false, "?>");
        let closed = b.get(end..end + 2) == Some(b"?>");
        if closed {
            out.paint(end, end + 2, 18);
        }
        if enabled(profile, "fold.html.preprocessor", true) {
            out.block(start, end + if closed { 2 } else { 0 });
        }
        return Some(end + if closed { 2 } else { 0 });
    }
    if asp_allow && b[start..].starts_with(b"<%") {
        if b[start..].starts_with(b"<%--") {
            let end = until(b, start + 4, b"--%>");
            out.paint(start, end, 20);
            return Some(end);
        }
        if b[start..].starts_with(b"<%@") {
            let end = until(b, start + 3, b"%>");
            out.paint(start, end, 16);
            return Some(end);
        }
        let open = if b.get(start + 2).is_some_and(|c| b"=!".contains(c)) {
            3
        } else {
            2
        };
        out.paint(start, start + open, 15);
        let end = script(text, start + open, profile, out, asp, true, "%>");
        let closed = b.get(end..end + 2) == Some(b"%>");
        if closed {
            out.paint(end, end + 2, 15);
        }
        out.block(start, end + if closed { 2 } else { 0 });
        return Some(end + if closed { 2 } else { 0 });
    }
    None
}

fn script(
    text: &str,
    start: usize,
    profile: &LanguageProfile,
    out: &mut Output,
    language: Script,
    server_side: bool,
    close: &str,
) -> usize {
    if language == Script::Python {
        return python(text, start, profile, out, server_side, close);
    }
    let b = text.as_bytes();
    let mut i = start;
    let php = language == Script::Php;
    let vb = language == Script::Vb;
    let base = if php {
        118
    } else if vb {
        if server_side { 81 } else { 71 }
    } else if server_side {
        56
    } else {
        41
    };
    let mut blocks = Vec::new();
    let mut regex_allowed = true;
    while i < b.len() {
        if b.get(i..i + close.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(close.as_bytes()))
        {
            break;
        }
        let at = i;
        let mut style = base;
        if !php
            && !server_side
            && let Some(end) = server(text, i, profile, out, Script::Vb)
        {
            i = end;
            continue;
        }
        if !vb && b[i..].starts_with(b"/*") {
            i = until(b, i + 2, b"*/");
            style = if php {
                124
            } else {
                base + if b.get(at + 2) == Some(&b'*') { 3 } else { 1 }
            };
            if enabled(profile, "fold.hypertext.comment", false) {
                out.block(at, i);
            }
        } else if !vb && (b[i..].starts_with(b"//") || php && b[i] == b'#') || vb && b[i] == b'\'' {
            let end = line_end(b, i);
            i = if php {
                b[i..end]
                    .windows(close.len())
                    .position(|w| w == close.as_bytes())
                    .map_or(end, |p| at + p)
            } else {
                end
            };
            style = if php {
                125
            } else if vb {
                base + 1
            } else {
                base + 2
            };
        } else if php && b[i..].starts_with(b"<<<") {
            let header_end = line_end(b, i);
            let marker = text[i + 3..header_end].trim().trim_matches(['\'', '"']);
            let nowdoc = text[i + 3..header_end].trim().starts_with('\'');
            i = header_end;
            for range in lines(&text[header_end..]) {
                let candidate = text[header_end + range.start..header_end + range.end].trim();
                if candidate
                    .strip_prefix(marker)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with([';', ',', ')']))
                {
                    i = header_end
                        + range.start
                        + text[header_end + range.start..header_end + range.end]
                            .find(marker)
                            .unwrap_or(0)
                        + marker.len();
                    break;
                }
                i = header_end + range.end;
            }
            style = if nowdoc { 120 } else { 119 };
            out.paint(at, i, style);
            if !nowdoc {
                php_variables(b, at, i, out, 126);
            }
            if enabled(profile, "fold.hypertext.heredoc", false) {
                out.block(at, i);
            }
            continue;
        } else if b[i] == b'"' || !vb && b[i] == b'\'' || !vb && b[i] == b'`' {
            let q = b[i];
            let limit = if vb { line_end(b, i) } else { b.len() };
            i = quoted(&b[..limit], i, q, !vb, vb);
            style = if php {
                if q == b'\'' { 120 } else { 119 }
            } else if vb {
                base + 4
            } else {
                base + match q {
                    b'"' => 7,
                    b'\'' => 8,
                    _ => 12,
                }
            };
            out.paint(at, i, style);
            if php && q != b'\'' {
                php_variables(b, at, i, out, 126);
            }
            regex_allowed = false;
            continue;
        } else if php && b[i] == b'$' {
            i += 1;
            while b.get(i).is_some_and(|c| word(*c)) {
                i += 1;
            }
            style = 123;
        } else if !php && !vb && b[i] == b'/' && regex_allowed {
            i += 1;
            let mut class = false;
            while i < b.len() && !b"\r\n".contains(&b[i]) {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                    continue;
                }
                if b[i] == b'[' {
                    class = true;
                } else if b[i] == b']' {
                    class = false;
                } else if b[i] == b'/' && !class {
                    i += 1;
                    while b.get(i).is_some_and(u8::is_ascii_alphabetic) {
                        i += 1;
                    }
                    break;
                }
                i += 1;
            }
            style = base + 11;
            regex_allowed = false;
        } else if b[i].is_ascii_digit()
            || b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)
        {
            i += 1;
            while b.get(i).is_some_and(|c| {
                word(*c) || *c == b'.' || b"+-".contains(c) && b"eE".contains(&b[i - 1])
            }) {
                i += 1;
            }
            style = if php {
                122
            } else if vb {
                base + 2
            } else {
                base + 4
            };
            regex_allowed = false;
        } else if word(b[i]) {
            i += 1;
            while b.get(i).is_some_and(|c| word(*c) || !php && *c == b'$') {
                i += 1;
            }
            let token = &text[at..i];
            let index = if php {
                4
            } else if vb {
                2
            } else {
                1
            };
            let known = keyword(profile, index, token, php || vb);
            style = if php {
                if known { 121 } else { 118 }
            } else if vb {
                if known { base + 3 } else { base + 5 }
            } else if known {
                base + 6
            } else {
                base + 5
            };
            if vb && token.eq_ignore_ascii_case("rem") {
                i = line_end(b, i);
                style = base + 1;
            }
            regex_allowed = matches!(
                token,
                "return"
                    | "throw"
                    | "case"
                    | "delete"
                    | "typeof"
                    | "void"
                    | "new"
                    | "in"
                    | "of"
                    | "yield"
                    | "await"
            );
        } else {
            if !b[i].is_ascii_whitespace() {
                style = if php {
                    127
                } else if vb {
                    base
                } else {
                    base + 9
                };
                regex_allowed = b"(=:[,!&|?;{}".contains(&b[i]);
                if b[i] == b'{' {
                    blocks.push(i);
                } else if b[i] == b'}'
                    && let Some(open) = blocks.pop()
                {
                    out.block(open, i + 1);
                }
            }
            i += 1;
        }
        out.paint(at, i, style);
    }
    for open in blocks {
        out.block(open, i);
    }
    i
}

fn python(
    text: &str,
    start: usize,
    profile: &LanguageProfile,
    out: &mut Output,
    server_side: bool,
    close: &str,
) -> usize {
    let b = text.as_bytes();
    let base = if server_side { 106 } else { 91 };
    let mut i = start;
    let mut previous = "";
    while i < b.len() {
        if b.get(i..i + close.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(close.as_bytes()))
        {
            break;
        }
        let at = i;
        let style;
        if b[i] == b'#' {
            i = line_end(b, i);
            style = base + 1;
        } else if matches!(b[i], b'\'' | b'"') {
            let q = b[i];
            let triple = b.get(i..i + 3) == Some(&[q, q, q]);
            if triple {
                i += 3;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i = (i + 2).min(b.len());
                    } else if b.get(i..i + 3) == Some(&[q, q, q]) {
                        i += 3;
                        break;
                    } else {
                        i += 1;
                    }
                }
            } else {
                i = quoted(&b[..line_end(b, i)], i, q, true, false);
            }
            style = base
                + if triple {
                    if q == b'"' { 7 } else { 6 }
                } else if q == b'"' {
                    3
                } else {
                    4
                };
        } else if b[i].is_ascii_digit() {
            i += 1;
            while b.get(i).is_some_and(|c| word(*c) || b"._".contains(c)) {
                i += 1;
            }
            style = base + 2;
        } else if word(b[i]) {
            i += 1;
            while b.get(i).is_some_and(|c| word(*c)) {
                i += 1;
            }
            let token = &text[at..i];
            style = base
                + if keyword(profile, 3, token, false) {
                    5
                } else if previous == "class" {
                    8
                } else if previous == "def" {
                    9
                } else {
                    11
                };
            previous = token;
        } else {
            style = if b[i].is_ascii_whitespace() {
                base
            } else {
                base + 10
            };
            i += 1;
        }
        out.paint(at, i, style);
    }
    i
}

fn php_variables(b: &[u8], start: usize, end: usize, out: &mut Output, style: u16) {
    let mut i = start;
    while i < end {
        if b[i] == b'\\' {
            i = (i + 2).min(end);
            continue;
        }
        if b[i] == b'$' && b.get(i + 1).is_some_and(|c| word(*c)) {
            let at = i;
            i += 1;
            while i < end && word(b[i]) {
                i += 1;
            }
            out.paint(at, i, style);
        } else if b[i..].starts_with(b"{$") {
            let at = i;
            let mut depth = 1;
            i += 2;
            while i < end && depth > 0 {
                if b[i] == b'{' {
                    depth += 1;
                } else if b[i] == b'}' {
                    depth -= 1;
                }
                i += 1;
            }
            out.paint(at, i, 104);
        } else {
            i += 1;
        }
    }
}

pub(super) fn css(text: &str, profile: &LanguageProfile, out: &mut Output, offset: usize) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut braces = Vec::new();
    let mut declaration = false;
    let mut value = false;
    let mut parens = 0;
    let mut group_pending = false;
    let mut selector_until = 0;
    let scss = enabled(profile, "lexer.css.scss.language", false)
        || enabled(profile, "lexer.css.hss.language", false);
    let less = enabled(profile, "lexer.css.less.language", false);
    while i < b.len() {
        if (scss || less)
            && declaration
            && !value
            && i >= selector_until
            && (word(b[i]) || b".:#&".contains(&b[i]))
            && let Some(next) = b[i..].iter().position(|c| b";{}".contains(c))
            && b[i + next] == b'{'
        {
            selector_until = i + next;
        }
        let selector = !declaration || i < selector_until;
        let start = i;
        let mut style = 0;
        let mut operator_prefix = 0;
        if b[i..].starts_with(b"/*") {
            i = until(b, i + 2, b"*/");
            style = 9;
            if enabled(profile, "fold.comment", false) {
                out.block(offset + start, offset + i);
            }
        } else if (scss || less) && b[i..].starts_with(b"//") {
            i = line_end(b, i);
            style = 9;
        } else if matches!(b[i], b'\'' | b'"') {
            let q = b[i];
            i = quoted(b, i, q, true, false);
            style = if q == b'"' { 13 } else { 14 };
        } else if scss && b[i] == b'$'
            || less
                && b[i] == b'@'
                && !b[i..].starts_with(b"@media")
                && !b[i..].starts_with(b"@import")
        {
            i += 1;
            while b.get(i).is_some_and(|c| word(*c) || *c == b'-') {
                i += 1;
            }
            style = 23;
        } else if b[i] == b'@' {
            i += 1;
            while b.get(i).is_some_and(|c| name(*c)) {
                i += 1;
            }
            style = if matches!(
                &text[start..i],
                "@media" | "@supports" | "@document" | "@layer" | "@container" | "@keyframes"
            ) {
                22
            } else {
                12
            };
            group_pending = style == 22;
            operator_prefix = 1;
        } else if b[i] == b'!'
            && text[i..]
                .get(..10)
                .is_some_and(|s| s.eq_ignore_ascii_case("!important"))
        {
            i += 10;
            style = 11;
            operator_prefix = 1;
        } else if selector && matches!(b[i], b'.' | b'#' | b':') {
            let prefix = b[i];
            i += 1;
            let pseudo_element = prefix == b':' && b.get(i) == Some(&b':');
            if pseudo_element {
                i += 1;
            }
            let name_start = i;
            operator_prefix = i - start;
            while b.get(i).is_some_and(|c| word(*c) || *c == b'-') {
                i += 1;
            }
            style = match prefix {
                b'.' => 2,
                b'#' => 10,
                _ => {
                    let token = &text[name_start..i];
                    if pseudo_element {
                        if keyword(profile, 4, token, true) {
                            18
                        } else if keyword(profile, 7, token, true) {
                            21
                        } else {
                            4
                        }
                    } else if keyword(profile, 1, token, true) {
                        3
                    } else if keyword(profile, 6, token, true) {
                        20
                    } else {
                        4
                    }
                }
            };
        } else if selector && b[i] == b'[' {
            out.paint(i + offset, i + offset + 1, 5);
            i += 1;
            while i < b.len() && b[i] != b']' {
                let at = i;
                if matches!(b[i], b'\'' | b'"') {
                    let q = b[i];
                    i = quoted(b, i, q, true, false);
                    out.paint(at + offset, i + offset, if q == b'"' { 13 } else { 14 });
                } else {
                    i += 1;
                    out.paint(at + offset, i + offset, 16);
                }
            }
            if b.get(i) == Some(&b']') {
                out.paint(i + offset, i + offset + 1, 5);
                i += 1;
            }
            continue;
        } else if word(b[i]) || b[i] == b'-' || value && b[i] == b'#' {
            i += 1;
            while b.get(i).is_some_and(|c| word(*c) || b"-.%#".contains(c)) {
                i += 1;
            }
            let token = &text[start..i];
            style = if selector {
                1
            } else if value {
                8
            } else if token.starts_with("--") {
                23
            } else if keyword(profile, 0, token, true) {
                6
            } else if keyword(profile, 2, token, true) {
                15
            } else if keyword(profile, 3, token, true) {
                17
            } else if keyword(profile, 5, token, true) {
                19
            } else {
                7
            };
        } else {
            match b[i] {
                b'{' => {
                    braces.push((i, declaration, value));
                    declaration = !group_pending;
                    group_pending = false;
                    value = false;
                    style = 5;
                }
                b'}' => {
                    if let Some((open, d, v)) = braces.pop() {
                        out.block(offset + open, offset + i + 1);
                        declaration = d;
                        value = v;
                    } else {
                        declaration = false;
                        value = false;
                    }
                    style = 5;
                }
                b':' if declaration && parens == 0 => {
                    value = true;
                    style = 5;
                }
                b';' if parens == 0 => {
                    value = false;
                    style = 5;
                }
                b'(' => {
                    parens += 1;
                    style = if value { 8 } else { 5 };
                }
                b')' => {
                    parens -= 1;
                    style = if value { 8 } else { 5 };
                }
                c if b",>+~=:;*/".contains(&c) => style = 5,
                _ => {}
            }
            i += 1;
        }
        out.paint(offset + start, offset + i, style);
        if operator_prefix > 0 {
            out.paint(offset + start, offset + start + operator_prefix, 5);
        }
    }
    for (open, _, _) in braces {
        out.block(offset + open, offset + text.len());
    }
}
