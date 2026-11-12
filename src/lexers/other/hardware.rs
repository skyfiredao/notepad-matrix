use std::collections::BTreeSet;

use super::{Scan, operator, start, word};

pub(super) fn asm(s: &mut Scan<'_>) {
    let comment =
        s.p.property("lexer.as.comment.character")
            .and_then(|v| v.bytes().next())
            .unwrap_or(b';');
    while s.more() {
        if s.ch() == comment {
            let a = s.i;
            s.line(1);
            while s.i > a && s.at(s.i - 1) == b'\\' && s.more() {
                s.i += 1;
                s.line(1);
            }
            s.paint(a, 1);
            if s.flag("fold.asm.comment.explicit", false) {
                let open = s.p.property("fold.asm.explicit.start").unwrap_or(";{");
                let close = s.p.property("fold.asm.explicit.end").unwrap_or(";}");
                let opening = !open.is_empty() && s.token(a).contains(open);
                let closing = !close.is_empty() && s.token(a).contains(close);
                if opening {
                    s.event(a, 1);
                }
                if closing {
                    s.event(a, -1);
                }
            }
        } else if matches!(s.ch(), b'"' | b'\'' | b'`') {
            let c = s.ch();
            s.quote(
                c,
                if c == b'"' {
                    3
                } else if c == b'\'' {
                    12
                } else {
                    16
                },
                13,
                true,
                false,
                false,
            );
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            s.number(2, b".?");
        } else if start(s.ch()) || b".%@$?".contains(&s.ch()) {
            let a = s.word(b".?");
            let t = s.token(a).to_ascii_lowercase();
            let style = s.classify(a, 5, &[6, 7, 8, 9, 10, 14], true);
            if s.flag("fold.asm.syntax.based", true) {
                if s.kw(6, &t, true) {
                    s.event(a, 1);
                }
                if s.kw(7, &t, true) {
                    s.event(a, -1);
                }
            }
            if style == 9 && t == "comment" {
                while matches!(s.ch(), b' ' | b'\t') {
                    s.take(1, 0);
                }
                let delimiter =
                    s.p.property("lexer.asm.comment.delimiter")
                        .and_then(|v| v.bytes().next())
                        .unwrap_or(b'~');
                if s.ch() == delimiter {
                    let a = s.i;
                    s.i += 1;
                    while s.more() && s.ch() != delimiter {
                        s.i += 1;
                    }
                    if s.more() {
                        s.i = s.line_end();
                    }
                    s.paint(a, 15);
                    if s.flag("fold.asm.comment.multiline", false) {
                        s.region(a, s.i);
                    }
                }
            }
        } else if b"*/-+()=^[]<&>,|~%:".contains(&s.ch()) {
            s.take(1, 4);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn asn1(s: &mut Scan<'_>) {
    let mut assignment = false;
    let mut oid = false;
    while s.more() {
        if s.is("--") {
            s.line(1);
        } else if s.ch() == b'"' {
            s.quote(b'"', 3, 3, false, false, true);
        } else if s.ch() == b':' {
            let a = s.i;
            while matches!(s.ch(), b':' | b'=' | b' ') {
                s.i += 1;
            }
            s.paint(a, 10);
            assignment = true;
        } else if assignment && s.ch() == b'{' {
            oid = true;
            assignment = false;
            s.take(1, 0);
        } else if s.ch() == b'}' {
            oid = false;
            s.take(1, 0);
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            while s.ch().is_ascii_digit() {
                s.i += 1;
            }
            s.paint(a, if oid || assignment { 4 } else { 5 });
            assignment = false;
        } else if s.ch().is_ascii_alphabetic() || s.ch() == b'-' {
            let a = s.i;
            s.i += 1;
            while s.ch().is_ascii_alphanumeric() || s.ch() == b'-' {
                s.i += 1;
            }
            if oid {
                s.paint(a, 2);
            } else {
                s.classify(a, 2, &[6, 7, 8, 9], false);
            }
            assignment = false;
        } else {
            if !s.ch().is_ascii_whitespace() {
                assignment = false;
            }
            s.take(1, 0);
        }
    }
}

pub(super) fn cobol(s: &mut Scan<'_>) {
    let mut containment = 0;
    while s.more() {
        let col = s.i - s.line_start();
        if ((col == 0 || col == 6) && matches!(s.ch(), b'*' | b'/')) || s.is("*>") {
            let style = if col == 0 && (s.is("**") || s.is("/*")) {
                3
            } else {
                2
            };
            s.line(style);
        } else if col == 0 && s.ch() == b'?' {
            let a = s.i;
            s.line(9);
            while s.i > a && s.at(s.i - 1) == b'\\' && s.more() {
                s.i += 1;
                s.line(9);
            }
            s.paint(a, 9);
        } else if matches!(s.ch(), b'\'' | b'"') {
            let c = s.ch();
            s.quote(
                c,
                if c == b'"' { 6 } else { 7 },
                if c == b'"' { 6 } else { 7 },
                false,
                false,
                false,
            );
        } else if s.ch().is_ascii_alphanumeric()
            || (s.ch() == b'$' && s.next().is_ascii_alphabetic())
        {
            let a = s.word(b"-");
            let t = s.token(a).to_ascii_lowercase();
            let numeric = t.bytes().all(|c| c.is_ascii_digit() || c == b'v')
                && (s.at(a).is_ascii_digit() || s.at(a).eq_ignore_ascii_case(&b'v'));
            if numeric {
                s.paint(a, 4);
            } else {
                s.classify(a, 11, &[5, 16, 8], true);
            }
            let desired = match t.as_str() {
                "division" => 1,
                "section" => 2,
                _ => 0,
            };
            if desired != 0 {
                while containment >= desired {
                    s.event(a, -1);
                    containment -= 1;
                }
                while containment < desired {
                    s.event(a, 1);
                    containment += 1;
                }
            }
        } else if operator(s.ch()) {
            s.take(1, 10);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn structured_text(s: &mut Scan<'_>) {
    while s.more() {
        if s.is("//") {
            s.line(2);
        } else if s.is("(*") {
            s.block("(*", "*)", false, 1);
        } else if s.ch() == b'{' {
            let a = s.i;
            s.i += 1;
            while s.more() && s.ch() != b'}' && !matches!(s.ch(), b'\r' | b'\n') {
                s.i += 1;
            }
            if s.ch() == b'}' {
                s.i += 1;
            }
            s.paint(a, 9);
        } else if matches!(s.ch(), b'\'' | b'"') {
            let a = s.i;
            let quote = s.ch();
            s.i += 1;
            while s.more() && !matches!(s.ch(), b'\r' | b'\n') {
                if s.ch() == quote && s.at(s.i - 1) != b'$' {
                    s.i += 1;
                    break;
                }
                s.i += 1;
            }
            s.paint(a, if quote == b'\'' { 12 } else { 13 });
        } else if s.ch() == b'#' {
            s.take(1, 11);
            let a = s.i;
            while s.more() && (word(s.ch()) || b".-:".contains(&s.ch())) {
                s.i += 1;
            }
            let t = s.token(a);
            let style = if t.bytes().all(|c| c.is_ascii_hexdigit() || c == b'_') {
                8
            } else if t
                .bytes()
                .all(|c| c.is_ascii_digit() || b"_.-:dmshDMSH".contains(&c))
            {
                16
            } else {
                0
            };
            s.paint(a, style);
        } else if s.ch().is_ascii_digit() {
            let a = s.i;
            while s.ch().is_ascii_digit() || b"_.eE".contains(&s.ch()) {
                s.i += 1;
            }
            s.paint(a, 7);
        } else if start(s.ch()) {
            let a = s.word(b"");
            let t = s.token(a).to_ascii_lowercase();
            let style = s.classify(a, 15, &[3, 4, 5, 6, 17, 18], true);
            if style == 3 || style == 17 {
                if t.starts_with("end_") {
                    s.event(a, -1);
                } else if matches!(
                    t.as_str(),
                    "action"
                        | "configuration"
                        | "function"
                        | "function_block"
                        | "if"
                        | "for"
                        | "while"
                        | "repeat"
                        | "case"
                        | "initial_step"
                        | "resource"
                        | "step"
                        | "struct"
                        | "transition"
                        | "type"
                        | "var"
                        | "var_input"
                        | "var_output"
                        | "var_in_out"
                        | "var_temp"
                        | "var_external"
                        | "var_access"
                        | "var_config"
                        | "var_global"
                ) {
                    s.event(a, 1);
                }
            }
        } else if b",.+-*/:;<=>[]()%&".contains(&s.ch()) {
            s.take(1, 10);
        } else {
            s.take(1, 0);
        }
    }
}

pub(super) fn forth(s: &mut Scan<'_>) {
    while s.more() {
        if s.ch().is_ascii_whitespace() {
            s.take(1, 0);
            continue;
        }
        if s.ch() == b'\\' && s.next().is_ascii_whitespace() {
            s.line(1);
        } else if s.ch() == b'(' && s.next().is_ascii_whitespace() {
            s.block("(", ")", false, 2);
        } else if s.ch() == b'{' {
            s.block("{", "}", false, 11);
        } else if s.ch() == b':' && s.next().is_ascii_whitespace() {
            let a = s.i;
            s.i += 1;
            while s.more() && s.ch().is_ascii_whitespace() {
                s.i += 1;
            }
            while s.more() && !s.ch().is_ascii_whitespace() {
                s.i += 1;
            }
            s.paint(a, 6);
        } else {
            let a = s.i;
            while s.more() && !s.ch().is_ascii_whitespace() {
                s.i += 1;
            }
            let t = s.token(a);
            let style = if t == ";" {
                6
            } else {
                let number = if let Some(t) = t.strip_prefix('$') {
                    !t.is_empty() && t.bytes().all(|c| c.is_ascii_hexdigit())
                } else if let Some(t) = t.strip_prefix('%') {
                    !t.is_empty() && t.bytes().all(|c| matches!(c, b'0' | b'1'))
                } else {
                    t.bytes().any(|c| c.is_ascii_hexdigit())
                        && t.bytes()
                            .all(|c| c.is_ascii_hexdigit() || b".-+".contains(&c))
                };
                if number { 9 } else { 3 }
            };
            let class = s.classify(a, style, &[4, 5, 6, 7, 8, 10], true);
            if class == 10 {
                let end = s.i;
                s.until("\"", 10);
                s.styles[end..s.i].fill(10);
            }
        }
    }
    s.events.clear();
}

pub(super) fn mmixal(s: &mut Scan<'_>) {
    while s.more() {
        let begin = s.i;
        let end = s.line_end();
        if s.is("@i") {
            s.line(17);
        } else {
            let label = !s.ch().is_ascii_whitespace();
            while s.i < end && s.ch().is_ascii_whitespace() {
                s.take(1, 0);
            }
            if s.i < end && !(word(s.ch()) || s.ch() == b':') {
                s.line(1);
            } else if s.i < end {
                if label {
                    let a = s.word(b":");
                    s.paint(a, 2);
                }
                while s.i < end && s.ch().is_ascii_whitespace() {
                    s.take(1, 4);
                }
                if s.i < end {
                    let a = s.word(b":");
                    s.paint(a, if s.kw(0, s.token(a), false) { 5 } else { 6 });
                    while s.i < end && s.ch().is_ascii_whitespace() {
                        s.take(1, 7);
                    }
                    while s.i < end {
                        if s.ch().is_ascii_whitespace() {
                            s.line(1);
                            break;
                        } else if matches!(s.ch(), b'\'' | b'"') {
                            let quote = s.ch();
                            s.quote(
                                quote,
                                if quote == b'"' { 12 } else { 11 },
                                if quote == b'"' { 12 } else { 11 },
                                false,
                                false,
                                false,
                            );
                        } else if s.ch() == b'$' || s.ch() == b'#' {
                            let a = s.i;
                            let style = if s.ch() == b'$' { 13 } else { 14 };
                            s.i += 1;
                            while if style == 13 {
                                s.ch().is_ascii_digit()
                            } else {
                                s.ch().is_ascii_hexdigit()
                            } {
                                s.i += 1;
                            }
                            s.paint(a, style);
                        } else if word(s.ch()) || matches!(s.ch(), b':' | b'@') {
                            let a = s.word(b":");
                            let t = s.token(a).trim_start_matches(':');
                            let style = if t.bytes().all(|c| c.is_ascii_digit()) {
                                9
                            } else if s.kw(1, t, false) {
                                13
                            } else if s.kw(2, t, false) {
                                16
                            } else {
                                10
                            };
                            s.paint(a, style);
                        } else if b"+-|^*/%<>&~$,()[]".contains(&s.ch()) {
                            s.take(1, 15);
                        } else {
                            s.take(1, 8);
                        }
                    }
                }
            }
        }
        if s.i == begin && s.i < end {
            s.i += 1;
        }
        if matches!(s.ch(), b'\r' | b'\n') {
            s.take(1, 0);
        }
    }
}

pub(super) fn verilog(s: &mut Scan<'_>) {
    let mut definitions: BTreeSet<String> =
        s.p.keywords(5)
            .split_ascii_whitespace()
            .map(str::to_owned)
            .collect();
    let mut conditional: Vec<(bool, bool)> = Vec::new();
    let mut inactive = false;
    while s.more() {
        let a = s.i;
        let activity = if inactive && s.flag("lexer.verilog.track.preprocessor", false) {
            64
        } else {
            0
        };
        if s.ch() == b'`' {
            s.i += 1;
            let name = s.word(b"");
            let directive = s.token(name).to_owned();
            let directive_end = s.i;
            while matches!(s.ch(), b' ' | b'\t') {
                s.i += 1;
            }
            let arg = s.i;
            while word(s.ch()) || s.ch() == b'$' {
                s.i += 1;
            }
            let argument = s.token(arg).to_owned();
            if s.flag("lexer.verilog.track.preprocessor", false) {
                match directive.as_str() {
                    "ifdef" | "ifndef" => {
                        let enabled = definitions.contains(&argument) == (directive == "ifdef");
                        conditional.push((inactive, enabled));
                        inactive = inactive || !enabled;
                    }
                    "else" => {
                        if let Some((parent, taken)) = conditional.last_mut() {
                            inactive = *parent || *taken;
                            *taken = true;
                        }
                    }
                    "elsif" => {
                        if let Some((parent, taken)) = conditional.last_mut() {
                            let enabled = definitions.contains(&argument);
                            inactive = *parent || *taken || !enabled;
                            *taken |= enabled;
                        }
                    }
                    "endif" => {
                        if let Some((parent, _)) = conditional.pop() {
                            inactive = parent;
                        }
                    }
                    _ => {}
                }
            }
            if !inactive && s.flag("lexer.verilog.update.preprocessor", false) {
                if directive == "define" {
                    definitions.insert(argument.clone());
                }
                if directive == "undef" {
                    definitions.remove(&argument);
                }
                if directive == "undefineall" {
                    definitions.clear();
                }
            }
            if s.flag("fold.preprocessor", false) {
                if matches!(directive.as_str(), "ifdef" | "ifndef") {
                    s.event(a, 1);
                }
                if directive == "endif" {
                    s.event(a, -1);
                }
            }
            s.i = directive_end;
            s.paint(a, 9);
        } else if s.is("/*") {
            s.block("/*", "*/", false, 1);
        } else if s.is("//") {
            let style = if s.is("//!") { 3 } else { 2 };
            s.line(style);
            while s.i > a && s.at(s.i - 1) == b'\\' && s.more() {
                if s.ch() == b'\r' {
                    s.i += 1;
                }
                if s.ch() == b'\n' {
                    s.i += 1;
                }
                s.line(style);
            }
            s.paint(a, style);
        } else if s.ch() == b'"' {
            s.quote(b'"', 6, 12, true, false, false);
        } else if s.ch() == b'\\' && matches!(s.next(), b'\r' | b'\n') {
            s.take(
                if s.next() == b'\r' && s.at(s.i + 2) == b'\n' {
                    3
                } else {
                    2
                },
                0,
            );
        } else if s.ch() == b'\\' {
            let a = s.i;
            while s.more() && !s.ch().is_ascii_whitespace() {
                s.i += 1;
            }
            s.paint(a, 11);
        } else if s.ch().is_ascii_digit()
            || s.ch() == b'\''
            || (s.ch() == b'.' && s.next().is_ascii_digit())
        {
            s.number(4, b"'?$.");
        } else if start(s.ch()) || s.ch() == b'$' {
            let a = s.word(b"$'");
            let t = s.token(a).to_owned();
            let mut style = s.classify(a, 11, &[5, 7, 8, 19], false);
            if s.flag("lexer.verilog.portstyling", false) {
                style = match t.as_str() {
                    "input" => 21,
                    "output" => 22,
                    "inout" => 23,
                    _ => style,
                };
                s.paint(a, style);
            }
            if style == 11
                && s.flag("lexer.verilog.allupperkeywords", false)
                && t.bytes().all(|c| !c.is_ascii_lowercase())
            {
                s.paint(a, 19);
            }
            if style == 5 {
                let opening = matches!(
                    t.as_str(),
                    "case"
                        | "casex"
                        | "casez"
                        | "covergroup"
                        | "function"
                        | "generate"
                        | "interface"
                        | "package"
                        | "primitive"
                        | "program"
                        | "sequence"
                        | "specify"
                        | "table"
                        | "task"
                        | "begin"
                        | "class"
                        | "fork"
                ) || (t == "module" && s.flag("fold.verilog.flags", false));
                let closing = matches!(
                    t.as_str(),
                    "endcase"
                        | "endclass"
                        | "endfunction"
                        | "endgenerate"
                        | "endgroup"
                        | "endinterface"
                        | "endpackage"
                        | "endprimitive"
                        | "endprogram"
                        | "endsequence"
                        | "endspecify"
                        | "endtable"
                        | "endtask"
                        | "join"
                        | "join_any"
                        | "join_none"
                        | "end"
                ) || (t == "endmodule" && s.flag("fold.verilog.flags", false));
                if opening {
                    s.event(a, 1);
                }
                if closing {
                    s.event(a, -1);
                }
            }
        } else if operator(s.ch()) || matches!(s.ch(), b'@' | b'#') {
            s.delimit(10, b"{", b"}");
        } else {
            s.take(1, 0);
        }
        if activity != 0 {
            for style in &mut s.styles[a..s.i] {
                *style |= activity;
            }
        }
    }
}

pub(super) fn vhdl(s: &mut Scan<'_>) {
    let mut end_word = false;
    while s.more() {
        if s.is("--") {
            s.line(if s.is("--!") { 2 } else { 1 });
        } else if s.is("/*") {
            s.block("/*", "*/", false, 15);
        } else if s.ch() == b'"' {
            s.quote(b'"', 4, 7, false, true, false);
        } else if s.ch() == b'\'' && s.at(s.i + 2) == b'\'' {
            s.take(3, 4);
        } else if s.ch() == b'\\' {
            s.quote(b'\\', 6, 6, false, false, false);
        } else if s.ch().is_ascii_digit() || (s.ch() == b'.' && s.next().is_ascii_digit()) {
            let a = s.i;
            while word(s.ch()) || s.ch() == b'.' {
                s.i += 1;
            }
            s.paint(a, 3);
        } else if start(s.ch()) {
            let a = s.word(b"");
            let t = s.token(a).to_ascii_lowercase();
            let style = s.classify(a, 6, &[8, 9, 10, 11, 12, 13, 14], true);
            if style == 8 {
                if t == "end" {
                    s.event(a, -1);
                    end_word = true;
                } else if !end_word
                    && matches!(
                        t.as_str(),
                        "architecture"
                            | "entity"
                            | "process"
                            | "function"
                            | "procedure"
                            | "package"
                            | "record"
                            | "case"
                            | "loop"
                            | "block"
                            | "generate"
                            | "component"
                            | "if"
                    )
                {
                    s.event(a, 1);
                }
            }
        } else if s.ch() == b'\'' {
            s.take(1, 0);
        } else if operator(s.ch()) || s.ch() == b'&' {
            if s.ch() == b';' {
                end_word = false;
            }
            if s.flag("fold.at.Parenthese", true) {
                s.delimit(5, b"(", b")");
            } else {
                s.take(1, 5);
            }
        } else {
            s.take(1, 0);
        }
    }
}
