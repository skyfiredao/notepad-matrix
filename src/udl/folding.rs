use super::{FoldLevel, FoldRange};
use std::ops::Range;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Open,
    Middle,
    Close,
}
pub(super) struct Event {
    pub byte: usize,
    pub action: Action,
}

pub(super) fn lines(text: &str) -> Vec<Range<usize>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut p = 0;
    while p < b.len() {
        if matches!(b[p], b'\r' | b'\n') {
            p += if b[p] == b'\r' && b.get(p + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
            out.push(start..p);
            start = p;
        } else {
            p += 1;
        }
    }
    out.push(start..b.len());
    out
}

pub(super) fn build(
    text: &str,
    mut events: Vec<Event>,
    comment_cover: &[Range<usize>],
    fold_comments: bool,
    compact: bool,
) -> (Vec<FoldRange>, Vec<FoldLevel>) {
    let lines = lines(text);
    let line_at = |p: usize| lines.partition_point(|r| r.start <= p).saturating_sub(1);
    if fold_comments {
        let in_comment = |p: usize| {
            let idx = comment_cover.partition_point(|r| r.start <= p);
            idx > 0 && comment_cover[idx - 1].contains(&p)
        };
        let pure: Vec<bool> = lines
            .iter()
            .map(|line| {
                let mut visible = false;
                for p in line.clone() {
                    if text.as_bytes()[p] > b' ' {
                        visible = true;
                        if !in_comment(p) {
                            return false;
                        }
                    }
                }
                visible || (!line.is_empty() && in_comment(line.start))
            })
            .collect();
        let mut i = 0;
        while i < pure.len() {
            if !pure[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < pure.len() && pure[i] {
                i += 1;
            }
            let end = i - 1;
            if end > start
                && !events
                    .iter()
                    .any(|e| (start..=end).contains(&line_at(e.byte)))
            {
                events.push(Event {
                    byte: lines[start].start,
                    action: Action::Open,
                });
                events.push(Event {
                    byte: lines[end].start,
                    action: Action::Close,
                });
            }
        }
    }
    events.sort_by_key(|e| e.byte);
    let mut regions = Vec::new();
    let mut levels = Vec::with_capacity(lines.len());
    let mut stack = Vec::new();
    let mut depth = 0usize;
    let mut event = 0;
    for (line, range) in lines.iter().enumerate() {
        let mut min = depth;
        while event < events.len() && line_at(events[event].byte) == line {
            match events[event].action {
                Action::Open => {
                    min = min.min(depth);
                    depth += 1;
                    stack.push(line);
                }
                Action::Middle => {
                    depth = depth.saturating_sub(1);
                    min = min.min(depth);
                    if let Some(start_line) = stack.pop()
                        && line > start_line + 1
                    {
                        regions.push(FoldRange {
                            start_line,
                            end_line: line - 1,
                        });
                    }
                    depth += 1;
                    stack.push(line);
                }
                Action::Close => {
                    depth = depth.saturating_sub(1);
                    if let Some(start_line) = stack.pop()
                        && line > start_line
                    {
                        regions.push(FoldRange {
                            start_line,
                            end_line: line,
                        });
                    }
                }
            }
            event += 1;
        }
        levels.push(FoldLevel {
            level: min,
            next_level: depth,
            header: min < depth,
            white: compact && text.as_bytes()[range.clone()].iter().all(|b| *b <= b' '),
        });
    }
    let end = lines.iter().rposition(|r| !r.is_empty()).unwrap_or(0);
    for start_line in stack {
        if end > start_line {
            regions.push(FoldRange {
                start_line,
                end_line: end,
            });
        }
    }
    regions.sort_by_key(|r| (r.start_line, r.end_line));
    (regions, levels)
}
