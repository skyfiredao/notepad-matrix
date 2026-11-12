use crate::languages::LanguageDefinition;
use crate::lexers::shared::{line_ranges, property_bool};
use crate::lexers::{FoldLevel, FoldRange, LexResult};

fn stream(style: u16) -> bool {
    matches!(style, 1 | 3 | 17 | 18)
}
fn space(ch: u8) -> bool {
    ch == b' ' || (b'\t'..=b'\r').contains(&ch)
}

pub(super) fn apply(text: &str, p: &LanguageDefinition, result: &mut LexResult) {
    if !property_bool(p, "fold", false) {
        result.folds.clear();
        for line in &mut result.fold_levels {
            line.level = 1024;
            line.header = false;
            line.white = false;
        }
        return;
    }
    if text.is_empty() {
        for line in &mut result.fold_levels {
            line.level = 1024;
            line.header = false;
            line.white = false;
        }
        return;
    }
    let b = text.as_bytes();
    let mut styles = vec![0u16; b.len() + 1];
    for span in &result.spans {
        styles[span.range.clone()].fill(span.style & !0x40);
    }
    let syntax = property_bool(p, "fold.cpp.syntax.based", true);
    let comments = property_bool(p, "fold.comment", false);
    let multiline = property_bool(p, "fold.cpp.comment.multiline", true);
    let explicit = property_bool(p, "fold.cpp.comment.explicit", true);
    let anywhere = property_bool(p, "fold.cpp.explicit.anywhere", false);
    let at_else = property_bool(p, "fold.at.else", false);
    let preprocessor = property_bool(p, "fold.preprocessor", false);
    let pp_else = property_bool(p, "fold.cpp.preprocessor.at.else", false);
    let compact = property_bool(p, "fold.compact", false);
    let start = p.property("fold.cpp.explicit.start").unwrap_or("");
    let end = p.property("fold.cpp.explicit.end").unwrap_or("");
    let custom = !start.is_empty() && !end.is_empty();
    let mut current = 1024i32;
    let mut previous_style = 0;
    result.fold_levels.clear();
    result.folds.clear();
    let mut headers: Vec<(usize, u32)> = Vec::new();
    for (line, range) in line_ranges(text).into_iter().enumerate() {
        let mut next = current;
        let mut minimum = current;
        let mut visible = 0;
        let mut line_comment = false;
        for position in range.clone() {
            let ch = b[position];
            let style = styles[position];
            let at_eol = position + 1 == range.end && matches!(ch, b'\r' | b'\n');
            if matches!(style, 2 | 15) {
                line_comment = true;
            }
            if comments && multiline && stream(style) && !line_comment {
                if !stream(previous_style) {
                    next += 1;
                } else if !stream(styles[position + 1]) && !at_eol {
                    next -= 1;
                }
            }
            if comments && explicit && (style == 2 || anywhere) {
                if custom {
                    if b[position..].starts_with(start.as_bytes()) {
                        next += 1;
                    } else if b[position..].starts_with(end.as_bytes()) {
                        next -= 1;
                    }
                } else if b[position..].starts_with(b"//{") {
                    next += 1;
                } else if b[position..].starts_with(b"//}") {
                    next -= 1;
                }
            }
            if preprocessor && style == 9 && ch == b'#' {
                let mut offset = position + 1;
                while b.get(offset).is_some_and(|ch| matches!(ch, b' ' | b'\t')) {
                    offset += 1;
                }
                let directive = &b[offset..];
                if directive.starts_with(b"region") || directive.starts_with(b"if") {
                    next += 1;
                } else if directive.starts_with(b"end") {
                    next -= 1;
                } else if directive.starts_with(b"pragma")
                    && b.get(offset + 6)
                        .is_some_and(|ch| matches!(ch, b' ' | b'\t'))
                {
                    offset += 6;
                    while b.get(offset).is_some_and(|ch| matches!(ch, b' ' | b'\t')) {
                        offset += 1;
                    }
                    if b[offset..].starts_with(b"region") {
                        next += 1;
                    } else if b[offset..].starts_with(b"endregion") {
                        next -= 1;
                    }
                }
                if pp_else && (b[offset..].starts_with(b"else") || b[offset..].starts_with(b"elif"))
                {
                    minimum -= 1;
                }
            }
            if syntax && style == 10 {
                if matches!(ch, b'{' | b'[' | b'(') {
                    if at_else {
                        minimum = minimum.min(next);
                    }
                    next += 1;
                } else if matches!(ch, b'}' | b']' | b')') {
                    next -= 1;
                }
            }
            if !space(ch) {
                visible += 1;
            }
            previous_style = style;
        }
        let level_use = if (syntax && at_else) || (preprocessor && pp_else) {
            minimum
        } else {
            current
        };
        let level = (level_use as u32) & 0x0fff;
        let header = level_use < next;
        let white = (visible == 0 && compact) || (range.is_empty() && range.start == text.len());
        while headers
            .last()
            .is_some_and(|(_, open_level)| *open_level >= level)
        {
            if let Some((start_line, _)) = headers.pop()
                && line > start_line + 1
            {
                result.folds.push(FoldRange {
                    start_line,
                    end_line: line - 1,
                });
            }
        }
        if header {
            headers.push((line, level));
        }
        result.fold_levels.push(FoldLevel {
            line,
            level,
            header,
            white,
        });
        current = next;
    }
    let last = result.fold_levels.len().saturating_sub(1);
    for (start_line, _) in headers {
        if last > start_line {
            result.folds.push(FoldRange {
                start_line,
                end_line: last,
            });
        }
    }
    result
        .folds
        .sort_by_key(|fold| (fold.start_line, fold.end_line));
}
