use super::{
    SortColumns, TransformCommand, TransformOptions,
};
use crate::editor::{ByteOffset, Selection, SelectionRange, SelectionSet};
use crate::input::EditorSession;

pub fn comment_syntax(
    language: crate::syntax::Language,
    definition: Option<&crate::udl::Definition>,
) -> Result<super::CommentSyntax, String> {
    use super::CommentSyntax;
    use crate::syntax::Language;
    let nonempty = |value: &str| (!value.is_empty()).then(|| value.to_owned());
    let pair = |start: &str, end: &str| nonempty(start).zip(nonempty(end));
    match language {
        Language::NormalText | Language::Builtin(0) => Ok(CommentSyntax::default()),
        Language::UserDefined(_) => {
            let definition = definition
                .ok_or_else(|| "The selected User Defined language is unavailable".to_owned())?;
            Ok(CommentSyntax {
                line: nonempty(&definition.comments.line_open),
                stream: pair(
                    &definition.comments.block_open,
                    &definition.comments.block_close,
                ),
                ..CommentSyntax::default()
            })
        }
        Language::Json | Language::Builtin(_) => {
            let id = match language {
                Language::Builtin(id) => id,
                _ => 57,
            };
            let profile = crate::languages::catalog()
                .map_err(|error| error.to_string())?
                .by_id(id)
                .ok_or_else(|| "The selected language is unavailable".to_owned())?;
            Ok(CommentSyntax {
                line: nonempty(&profile.comment_line),
                stream: pair(&profile.comment_start, &profile.comment_end),
                at_line_start: matches!(id, 59 | 60),
                comment_empty_lines: id == 60,
                omit_line_space: id == 60,
            })
        }
    }
}

pub fn apply_command(
    session: &mut EditorSession,
    id: u32,
    options: TransformOptions,
) -> Result<(), String> {
    apply_command_with_target(session, id, options).result
}

pub fn apply_command_with_target(
    session: &mut EditorSession,
    id: u32,
    options: TransformOptions,
) -> super::TransformOutcome {
    let mut last_target = None;
    let result = apply_command_pipeline(session, id, options, &mut last_target);
    super::TransformOutcome { result, last_target }
}

fn apply_command_pipeline(
    session: &mut EditorSession,
    id: u32,
    options: TransformOptions,
    last_target: &mut Option<std::ops::Range<i128>>,
) -> Result<(), String> {
    let command =
        TransformCommand::from_source_id(id).ok_or_else(|| format!("Unknown text command {id}"))?;
    if session.has_composition() {
        return Err("Finish text composition before transforming text".into());
    }
    match command {
        TransformCommand::Indent | TransformCommand::Unindent => {
            return apply_indent(session, command == TransformCommand::Indent, &options);
        }
        TransformCommand::TabsToSpaces | TransformCommand::SpacesToTabs { .. } => {
            return apply_whitespace_conversion(session, command, options.tab_width, last_target);
        }
        TransformCommand::Case(case) => return apply_case(session, case, options.random_seed, last_target),
        TransformCommand::DuplicateLine | TransformCommand::DuplicateSelection => {
            return apply_duplicate(session, command == TransformCommand::DuplicateLine, options.tab_width);
        }
        TransformCommand::EolToSpace | TransformCommand::JoinLines => {
            return apply_join(session, command == TransformCommand::JoinLines, last_target);
        }
        TransformCommand::MoveUp | TransformCommand::MoveDown => {
            return apply_move_lines(session, command == TransformCommand::MoveDown);
        }
        TransformCommand::TransposeLine => return apply_transpose_line(session),
        TransformCommand::Redact { bullet } => return apply_redact_pipeline(session, bullet, last_target),
        TransformCommand::RemoveDuplicates { consecutive_only: true } => return apply_consecutive_duplicates(session),
        TransformCommand::Trim(trim) => return apply_trim(session, trim, last_target),
        TransformCommand::RemoveEmptyLines { include_whitespace } => return apply_remove_empty_lines(session, include_whitespace, last_target),
        TransformCommand::Comment(comment @ (super::Comment::Line | super::Comment::ToggleLine | super::Comment::UncommentLine)) => {
            return apply_block_comment(session, comment, &options.comment_syntax, last_target);
        }
        TransformCommand::Comment(super::Comment::UncommentStream) => {
            return apply_uncomment_stream(session, &options.comment_syntax, true, last_target);
        }
        TransformCommand::Comment(super::Comment::Stream) => return apply_stream_comment(session, &options.comment_syntax, last_target),
        TransformCommand::TrimAll => {
            let entire = session.selection().range().is_empty();
            session.begin_undo_action();
            let result = apply_trim(session, super::Trim::Both, last_target).and_then(|()| {
                if entire || !session.selection().range().is_empty() { apply_join(session, false, last_target) }
                else { Ok(()) }
            });
            session.end_undo_action();
            return result;
        }
        TransformCommand::SplitLines => return apply_split_lines(session, &options.split_breaks, last_target),
        TransformCommand::Sort { kind: super::Sort::Locale, descending } => return apply_locale_sort(session, descending, &options, last_target),
        TransformCommand::Sort { kind, descending } => return apply_sort(session, kind, descending, &options, last_target),
        TransformCommand::RemoveDuplicates { consecutive_only: false } => return apply_any_duplicates(session, last_target),
        TransformCommand::BlankLineAbove | TransformCommand::BlankLineBelow => {
            return apply_blank_line(session, command == TransformCommand::BlankLineAbove);
        }
    }
}

fn line_start_or_end(bytes: &crate::byte_buffer::ByteBuffer, line: usize) -> Result<usize, String> {
    if line >= bytes.line_count() { Ok(bytes.len()) }
    else { bytes.line_start(line).map_err(|error| error.to_string()) }
}

fn line_end(bytes: &crate::byte_buffer::ByteBuffer, line: usize) -> Result<usize, String> {
    let mut end = line_start_or_end(bytes, line)?;
    while end < bytes.len() && !bytes.is_line_end(end) { end += 1; }
    Ok(end)
}

fn selected_lines(bytes: &crate::byte_buffer::ByteBuffer, selection: Selection, entire_if_empty: bool) -> Result<(usize, usize), String> {
    let range = selection.range();
    if range.is_empty() && entire_if_empty { return Ok((0, bytes.line_count() - 1)); }
    let first = bytes.line_index(range.start.0).map_err(|error| error.to_string())?;
    let mut last = bytes.line_index(range.end.0).map_err(|error| error.to_string())?;
    if last > first && range.end.0 == line_start_or_end(bytes, last)? { last -= 1; }
    Ok((first, last))
}

fn apply_split_lines(session: &mut EditorSession, breaks: &[ByteOffset], last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    let original = session.selections();
    if original.ranges().len() != 1 { return Ok(()); }
    let mut bytes = session.document().byte_snapshot();
    let (first, last) = selected_lines(&bytes, original.primary().selection, false)?;
    let start = line_start_or_end(&bytes, first)?;
    let end = line_end(&bytes, last)?;
    *last_target = Some(start as i128..end as i128);
    let mut selections = SelectionSet::single(Selection { anchor: ByteOffset(start), head: ByteOffset(end) });
    if session.is_read_only() { return session.set_selections(selections).map_err(|error| error.to_string()); }
    let mut positions: Vec<_> = breaks.iter().map(|position| position.0).filter(|&position| position > start && position <= end).collect();
    positions.sort_unstable();
    positions.dedup();
    let eol = session.line_ending().as_str().as_bytes();
    let mut edits = crate::editor::LineEdits::default();
    let mut added = 0;
    for position in positions {
        let row = bytes.line_index(position + added).map_err(|error| error.to_string())?;
        let at = position + added;
        if at <= line_start_or_end(&bytes, row)? || at > line_end(&bytes, row)? { continue; }
        selections.map_edit(at, 0, eol.len());
        edits.change_bytes(&mut bytes, at..at, eol).map_err(|error| error.to_string())?;
        added += eol.len();
        *last_target = Some(start as i128..(end + added) as i128);
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_blank_line(session: &mut EditorSession, above: bool) -> Result<(), String> {
    let mut bytes = session.document().byte_snapshot();
    let main = session.selections().primary().selection;
    let row = bytes.line_index(main.head.0).map_err(|error| error.to_string())?;
    let eol = session.line_ending().as_str().as_bytes();
    let position = if above {
        if row == 0 { 0 } else {
            let start = line_start_or_end(&bytes, row)?;
            match start.checked_sub(eol.len()) {
                Some(position) => position,
                None if start + 1 == eol.len() => main.head.0,
                None => return Err("The insertion position is outside the document".to_owned()),
            }
        }
    } else if row + 1 == bytes.line_count() { bytes.len() }
    else { line_end(&bytes, row)? + eol.len() };
    let mut edits = crate::editor::LineEdits::default();
    if !session.is_read_only() {
        edits.change_bytes(&mut bytes, position..position, eol).map_err(|error| error.to_string())?;
    }
    let caret = ByteOffset(line_start_or_end(&bytes, row + usize::from(!above))?);
    session.commit_prepared_bytes(bytes, SelectionSet::single(Selection { anchor: caret, head: caret }), edits).map_err(|error| error.to_string())
}

fn apply_any_duplicates(session: &mut EditorSession, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let mut bytes = session.document().byte_snapshot();
    let mut selections = session.selections();
    let (first, last) = selected_lines(&bytes, selections.primary().selection, true)?;
    if first >= last { return Ok(()); }
    let start = line_start_or_end(&bytes, first)?;
    let end = line_start_or_end(&bytes, last + 1)?;
    let reaches_end = last + 1 == bytes.line_count();
    let raw = bytes.slice(start..end).map_err(|error| error.to_string())?.to_vec();
    let decoded = String::from_utf8_lossy(&raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())]);
    let eol = session.line_ending().as_str();
    let mut values: Vec<_> = decoded.split(eol).map(|line| line.encode_utf16().collect::<Vec<_>>()).collect();
    if !reaches_end && values.last().is_some_and(Vec::is_empty) { values.pop(); }
    let count = values.len();
    let mut seen = std::collections::HashSet::new();
    values.retain(|line| seen.insert(line.clone()));
    if values.len() == count { return Ok(()); }
    let joined = join_utf16_lines(&values, &(0..values.len()).collect::<Vec<_>>(), eol, !reaches_end);
    if joined == decoded { return Ok(()); }
    let mut edits = crate::editor::LineEdits::default();
    selections.map_edit(start, end - start, 0);
    selections.map_edit(start, 0, joined.len());
    *last_target = Some(start as i128..end as i128);
    edits.change_bytes(&mut bytes, start..end, joined.as_bytes()).map_err(|error| error.to_string())?;
    *last_target = Some(start as i128..(start + joined.len()) as i128);
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn join_utf16_lines(lines: &[Vec<u16>], order: &[usize], eol: &str, trailing_eol: bool) -> String {
    let eol: Vec<_> = eol.encode_utf16().collect();
    let mut joined = Vec::new();
    for (slot, &index) in order.iter().enumerate() {
        if slot != 0 { joined.extend_from_slice(&eol); }
        joined.extend_from_slice(&lines[index]);
    }
    if trailing_eol { joined.extend_from_slice(&eol); }
    String::from_utf16_lossy(&joined)
}

fn apply_sort(session: &mut EditorSession, kind: super::Sort, descending: bool, options: &TransformOptions, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    let mut selections = session.selections();
    let mut bytes = session.document().byte_snapshot();
    let multi = selections.ranges().len() > 1;
    if multi && !selections.is_rectangular() { return Ok(()); }
    let main = selections.primary().selection;
    let has_line_selection = !multi && !main.range().is_empty();
    let mut columns = SortColumns::WholeLine;
    let (first, last) = if multi {
        let (anchor, caret) = selections.rectangular_endpoints().ok_or_else(|| "The rectangular selection is unavailable".to_owned())?;
        let anchor_row = bytes.line_index(anchor.0.0).map_err(|error| error.to_string())?;
        let caret_row = bytes.line_index(caret.0.0).map_err(|error| error.to_string())?;
        let from = anchor.0.0 - line_start_or_end(&bytes, anchor_row)? + anchor.1;
        let to = caret.0.0 - line_start_or_end(&bytes, caret_row)? + caret.1;
        columns = SortColumns::Range { from_column: from.min(to), to_column: from.max(to) };
        (anchor_row.min(caret_row), anchor_row.max(caret_row))
    } else { selected_lines(&bytes, main, true)? };
    if first >= last { return Ok(()); }
    let start = line_start_or_end(&bytes, first)?;
    let end = line_start_or_end(&bytes, last + 1)?;
    let reaches_end = last + 1 == bytes.line_count();
    let raw = bytes.slice(start..end).map_err(|error| error.to_string())?.to_vec();
    let decoded = String::from_utf8_lossy(&raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())]);
    let eol = session.line_ending().as_str();
    let mut values: Vec<_> = decoded.split(eol).map(|line| line.encode_utf16().collect::<Vec<_>>()).collect();
    if !reaches_end && values.last().is_some_and(Vec::is_empty) { values.pop(); }
    let order = match super::sort_order(&values, kind, descending, columns, options.random_seed) {
        Ok(order) => order,
        Err(line) => {
            if has_line_selection {
                session.select(Selection { anchor: ByteOffset(start), head: ByteOffset(line_end(&bytes, last)?) }).map_err(|error| error.to_string())?;
            }
            return Err(format!("由于第 {} 行的问题，无法执行数字排序。", first + line + 1));
        }
    };
    let joined = join_utf16_lines(&values, &order, eol, !reaches_end);
    let mut edits = crate::editor::LineEdits::default();
    if joined != decoded { *last_target = Some(start as i128..end as i128); }
    if joined != decoded && !session.is_read_only() {
        selections.map_edit(start, end - start, 0);
        selections.map_edit(start, 0, joined.len());
        edits.change_bytes(&mut bytes, start..end, joined.as_bytes()).map_err(|error| error.to_string())?;
        *last_target = Some(start as i128..(start + joined.len()) as i128);
    }
    if has_line_selection {
        selections = SelectionSet::single(Selection { anchor: ByteOffset(line_start_or_end(&bytes, first)?), head: ByteOffset(line_end(&bytes, last)?) });
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_locale_sort(session: &mut EditorSession, descending: bool, options: &TransformOptions, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    let source = session.selections();
    let rectangle = source.is_rectangular();
    if rectangle && source.ranges().len() < 2 { return Ok(()); }
    if !rectangle && source.ranges().len() != 1 { return Err("不支持对多重选区进行排序。".to_owned()); }
    let mut bytes = session.document().byte_snapshot();
    let main = source.primary().selection;
    let empty = source.ranges().iter().all(|range| range.is_empty());
    let endpoints = source.rectangular_endpoints();
    let (first, mut last) = if let Some((anchor, caret)) = endpoints {
        let anchor_row = bytes.line_index(anchor.0.0).map_err(|error| error.to_string())?;
        let caret_row = bytes.line_index(caret.0.0).map_err(|error| error.to_string())?;
        (anchor_row.min(caret_row), anchor_row.max(caret_row))
    } else { selected_lines(&bytes, main, true)? };
    if !rectangle && empty && line_start_or_end(&bytes, last)? == bytes.len() && last > first { last -= 1; }
    if first >= last { return Ok(()); }
    let keys = options.locale_keys.as_ref().filter(|keys| keys.len() == bytes.line_count()).ok_or_else(|| "Locale sort keys are unavailable".to_owned())?;
    let start = line_start_or_end(&bytes, first)?;
    let end = line_start_or_end(&bytes, last + 1)?;
    let missing_eol = last + 1 == bytes.line_count();
    let mut order: Vec<_> = (first..=last).collect();
    order.sort_by(|&a, &b| {
        let comparison = keys[a].cmp(&keys[b]);
        if descending { comparison.reverse() } else { comparison }
    });
    let mut sorted = Vec::new();
    for &row in &order {
        let line_start = line_start_or_end(&bytes, row)?;
        let line_next = line_start_or_end(&bytes, row + 1)?;
        sorted.extend_from_slice(&bytes.slice(line_start..line_next).map_err(|error| error.to_string())?.to_vec());
        if missing_eol && row == last { sorted.extend_from_slice(session.line_ending().as_str().as_bytes()); }
    }
    if missing_eol {
        if sorted.last() == Some(&b'\n') { sorted.pop(); }
        if sorted.last() == Some(&b'\r') { sorted.pop(); }
    }
    let old_row = bytes.line_index(main.head.0).map_err(|error| error.to_string())?;
    let caret_offset = main.head.0 - line_start_or_end(&bytes, old_row)?;
    let rectangle_restore = if let Some((anchor, caret)) = endpoints {
        let forward = caret.0 > anchor.0;
        let row_range = |row: usize| -> Result<SelectionRange, String> {
            for &range in source.ranges() {
                if bytes.line_index(range.selection.head.0).map_err(|error| error.to_string())? == row { return Ok(range); }
            }
            Err("The rectangular selection row is unavailable".to_owned())
        };
        let anchor_range = row_range(if forward { order[0] } else { order[order.len() - 1] })?;
        let caret_range = row_range(if forward { order[order.len() - 1] } else { order[0] })?;
        let anchor_row = bytes.line_index(anchor_range.selection.anchor.0).map_err(|error| error.to_string())?;
        let caret_row = bytes.line_index(caret_range.selection.head.0).map_err(|error| error.to_string())?;
        Some((forward, anchor_range.selection.anchor.0 - line_start_or_end(&bytes, anchor_row)?, anchor_range.anchor_virtual,
            caret_range.selection.head.0 - line_start_or_end(&bytes, caret_row)?, caret_range.head_virtual))
    } else { None };
    let mut edits = crate::editor::LineEdits::default();
    *last_target = Some(start as i128..end as i128);
    if !session.is_read_only() {
        edits.change_bytes(&mut bytes, start..end, &sorted).map_err(|error| error.to_string())?;
        *last_target = Some(start as i128..(start + sorted.len()) as i128);
    }
    let selections = if let Some((forward, anchor_offset, anchor_virtual, caret_offset, caret_virtual)) = rectangle_restore {
        let anchor = line_start_or_end(&bytes, if forward { first } else { last })? + anchor_offset;
        let caret = line_start_or_end(&bytes, if forward { last } else { first })? + caret_offset;
        rectangle_from_bytes(&bytes, (anchor.min(bytes.len()), anchor_virtual), (caret.min(bytes.len()), caret_virtual), options.tab_width)?
    } else if empty {
        let row = order.iter().position(|&row| row == old_row).map_or(old_row, |index| first + index);
        let caret = ByteOffset((line_start_or_end(&bytes, row)? + caret_offset).min(bytes.len()));
        SelectionSet::single(Selection { anchor: caret, head: caret })
    } else {
        let end = if session.is_read_only() { start } else { start + sorted.len() };
        SelectionSet::single(if main.anchor > main.head {
            Selection { anchor: ByteOffset(end), head: ByteOffset(start) }
        } else { Selection { anchor: ByteOffset(start), head: ByteOffset(end) } })
    };
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_whitespace_conversion(session: &mut EditorSession, command: TransformCommand, tab_width: usize, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    if tab_width == 0 { return Err("The tab width must be greater than zero".to_owned()); }
    let original = session.selections();
    if original.is_rectangular() { return Ok(()); }
    let main = original.primary().selection;
    let entire = main.range().is_empty();
    let mut bytes = session.document().byte_snapshot();
    let current_line = bytes.line_index(main.head.0).map_err(|error| error.to_string())?;
    let current_offset = main.head.0 - line_start_or_end(&bytes, current_line)?;
    let end_line = bytes.line_index(main.range().end.0).map_err(|error| error.to_string())?;
    let mut last = if entire { bytes.line_count() - 1 } else { end_line };
    let first = if entire { 0 } else { bytes.line_index(main.range().start.0).map_err(|error| error.to_string())? };
    let excludes_last = !entire && main.range().end.0 == line_start_or_end(&bytes, last)?;
    if excludes_last { last -= 1; }
    let mut edits = crate::editor::LineEdits::default();
    let mut selections = if entire { original.clone() } else { SelectionSet::single(main) };
    let mut changed = false;
    let mut new_position = 0;
    for line in first..=last {
        let start = line_start_or_end(&bytes, line)?;
        let mut end = start;
        while end < bytes.len() && !bytes.is_line_end(end) { end += 1; }
        if !bytes.is_empty() { *last_target = Some(start as i128..end as i128); }
        let source = bytes.slice(start..end).map_err(|error| error.to_string())?.to_vec();
        let (text, position, line_changed) = match command {
            TransformCommand::TabsToSpaces => super::tab_to_spaces(&source, tab_width, current_offset),
            TransformCommand::SpacesToTabs { leading_only } => super::spaces_to_tabs(&source, tab_width, leading_only, current_offset),
            _ => return Ok(()),
        };
        if line == current_line { new_position = position; }
        if line_changed {
            selections.map_edit(start, end - start, 0);
            selections.map_edit(start, 0, text.len());
            edits.change_bytes(&mut bytes, start..end, &text).map_err(|error| error.to_string())?;
            *last_target = Some(start as i128..(start + text.len()) as i128);
            changed = true;
        }
    }
    if changed {
        selections = if entire {
            let position = (line_start_or_end(&bytes, current_line)? + new_position).min(bytes.len());
            SelectionSet::single(Selection { anchor: ByteOffset(position), head: ByteOffset(position) })
        } else {
            let start = line_start_or_end(&bytes, first)?;
            let mut end = line_start_or_end(&bytes, end_line)?;
            if !excludes_last {
                while end < bytes.len() && !bytes.is_line_end(end) { end += 1; }
            }
            SelectionSet::single(Selection { anchor: ByteOffset(start), head: ByteOffset(end) })
        };
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn line_indent(bytes: &crate::byte_buffer::ByteBuffer, line: usize, tab_width: usize) -> Result<(usize, usize), String> {
    let mut position = line_start_or_end(bytes, line)?;
    while bytes.byte(position).is_some_and(|byte| matches!(byte, b' ' | b'\t')) { position += 1; }
    let column = bytes.column(position, tab_width).map_err(|error| error.to_string())?;
    Ok((position, column))
}

fn set_line_indent(
    bytes: &mut crate::byte_buffer::ByteBuffer,
    selections: &mut SelectionSet,
    edits: &mut crate::editor::LineEdits,
    line: usize,
    target: usize,
    options: &TransformOptions,
) -> Result<usize, String> {
    let (before, column) = line_indent(bytes, line, options.tab_width)?;
    if target == column { return Ok(before); }
    let start = line_start_or_end(bytes, line)?;
    let text = if options.use_tabs {
        [vec![b'\t'; target / options.tab_width], vec![b' '; target % options.tab_width]].concat()
    } else { vec![b' '; target] };
    selections.map_edit(start, before - start, 0);
    selections.map_edit(start, 0, text.len());
    edits.change_bytes(bytes, start..before, &text).map_err(|error| error.to_string())?;
    Ok(start + text.len())
}

fn apply_indent(session: &mut EditorSession, forwards: bool, options: &TransformOptions) -> Result<(), String> {
    if options.tab_width == 0 { return Err("The tab width must be greater than zero".to_owned()); }
    let mut bytes = session.document().byte_snapshot();
    let original = session.selections();
    let mut selections = original.clone();
    let mut edits = crate::editor::LineEdits::default();
    let main = original.primary().selection.range();
    let first = bytes.line_index(main.start.0).map_err(|error| error.to_string())?;
    let last = bytes.line_index(main.end.0).map_err(|error| error.to_string())?;
    if options.force_line_indent && original.ranges().len() == 1 && first == last {
        if session.is_read_only() {
            return session.set_selections(SelectionSet::single(Selection { anchor: main.start, head: main.end })).map_err(|error| error.to_string());
        }
        let (before, column) = line_indent(&bytes, first, options.tab_width)?;
        let target = if forwards { column + options.tab_width } else { column.saturating_sub(options.tab_width) };
        let after = set_line_indent(&mut bytes, &mut selections, &mut edits, first, target, options)?;
        let position = |position: usize| {
            if after > before && position >= before { position + after - before }
            else if after < before && position >= after {
                if position >= before { position - (before - after) } else { after }
            } else { position }
        };
        selections = SelectionSet::single(Selection {
            anchor: ByteOffset(position(main.start.0)), head: ByteOffset(position(main.end.0)),
        });
    } else {
        if session.is_read_only() { return Ok(()); }
        let mut order: Vec<_> = (0..original.ranges().len()).collect();
        if original.is_rectangular() && original.primary_index() == 0 && order.len() > 1 { order.reverse(); }
        for index in order {
            let selected = selections.ranges()[index].selection;
            let anchor_line = bytes.line_index(selected.anchor.0).map_err(|error| error.to_string())?;
            let caret_line = bytes.line_index(selected.head.0).map_err(|error| error.to_string())?;
            if anchor_line == caret_line {
                let mut caret = selected.head.0;
                if forwards {
                    let range = selected.range();
                    selections.map_edit(range.start.0, range.end.0 - range.start.0, 0);
                    edits.change_bytes(&mut bytes, range.start.0..range.end.0, &[]).map_err(|error| error.to_string())?;
                    caret = selections.ranges()[index].selection.head.0;
                }
                let (_, indentation) = line_indent(&bytes, caret_line, options.tab_width)?;
                let column = bytes.column(caret, options.tab_width).map_err(|error| error.to_string())?;
                let position = if column <= indentation {
                    let target = if forwards {
                        indentation + options.tab_width - indentation % options.tab_width
                    } else { indentation.saturating_sub(options.tab_width) };
                    set_line_indent(&mut bytes, &mut selections, &mut edits, caret_line, target, options)?
                } else if forwards {
                    let text = if options.use_tabs { vec![b'\t'] }
                        else { vec![b' '; options.tab_width - column % options.tab_width] };
                    selections.map_edit(caret, 0, text.len());
                    edits.change_bytes(&mut bytes, caret..caret, &text).map_err(|error| error.to_string())?;
                    caret + text.len()
                } else {
                    let target = column.saturating_sub(1) / options.tab_width * options.tab_width;
                    while bytes.column(caret, options.tab_width).map_err(|error| error.to_string())? > target {
                        caret = caret.checked_sub(1).ok_or_else(|| "The indentation position is before the document".to_owned())?;
                    }
                    caret
                };
                selections.set_range(index, Selection { anchor: ByteOffset(position), head: ByteOffset(position) }.into());
            } else {
                let anchor_offset = selected.anchor.0 - line_start_or_end(&bytes, anchor_line)?;
                let caret_offset = selected.head.0 - line_start_or_end(&bytes, caret_line)?;
                let top = anchor_line.min(caret_line);
                let mut bottom = anchor_line.max(caret_line);
                let bottom_start = line_start_or_end(&bytes, bottom)?;
                if bottom_start == selected.anchor.0 || bottom_start == selected.head.0 { bottom -= 1; }
                for line in (top..=bottom).rev() {
                    let (indent_end, indentation) = line_indent(&bytes, line, options.tab_width)?;
                    if forwards && bytes.is_line_end(indent_end) && indent_end == line_start_or_end(&bytes, line)? { continue; }
                    let target = if forwards { indentation + options.tab_width } else { indentation.saturating_sub(options.tab_width) };
                    set_line_indent(&mut bytes, &mut selections, &mut edits, line, target, options)?;
                }
                let anchor = line_start_or_end(&bytes, anchor_line + usize::from(anchor_line >= caret_line && anchor_offset != 0))?;
                let caret = line_start_or_end(&bytes, caret_line + usize::from(anchor_line < caret_line && caret_offset != 0))?;
                selections.set_range(index, Selection { anchor: ByteOffset(anchor), head: ByteOffset(caret) }.into());
            }
        }
        if original.is_rectangular() {
            let top = selections.ranges()[0];
            let bottom = selections.ranges()[selections.ranges().len() - 1];
            let original_anchor = if original.primary_index() == 0 { original.ranges()[original.ranges().len() - 1] } else { original.ranges()[0] };
            let reverse = (original.primary().selection.head, original.primary().head_virtual)
                < (original_anchor.selection.anchor, original_anchor.anchor_virtual);
            let anchor = if reverse { (bottom.selection.anchor.0, bottom.anchor_virtual) }
                else { (top.selection.head.0, top.head_virtual) };
            let head = if reverse { top.selection.head.0 } else { bottom.selection.anchor.0 };
            let head_line = bytes.line_index(head).map_err(|error| error.to_string())?;
            let column = bytes.column(anchor.0, options.tab_width).map_err(|error| error.to_string())? + anchor.1;
            let head = bytes.find_column(head_line, column, options.tab_width).map_err(|error| error.to_string())?;
            let virtual_space = if bytes.is_line_end(head) {
                column.saturating_sub(bytes.column(head, options.tab_width).map_err(|error| error.to_string())?)
            } else { 0 };
            selections = rectangle_from_bytes(&bytes, anchor, (head, virtual_space), options.tab_width)?;
        }
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_stream_comment(session: &mut EditorSession, syntax: &super::CommentSyntax, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let Some((opening, closing)) = syntax.stream.as_ref().filter(|(start, end)| !start.is_empty() && !end.is_empty()) else {
        return apply_block_comment(session, super::Comment::Line, syntax, last_target);
    };
    let mut bytes = session.document().byte_snapshot();
    let main = session.selections().primary().selection;
    let reversed = main.head < main.range().end;
    let mut range = main.range().start.0..main.range().end.0;
    if range.is_empty() {
        let row = bytes.line_index(range.start).map_err(|error| error.to_string())?;
        range.start = line_indent(&bytes, row, 4)?.0;
        range.end = range.start;
        while range.end < bytes.len() && !bytes.is_line_end(range.end) { range.end += 1; }
    }
    let opening = [opening.as_bytes(), b" "].concat();
    let closing = [b" ".as_slice(), closing.as_bytes()].concat();
    let mut edits = crate::editor::LineEdits::default();
    edits.change_bytes(&mut bytes, range.start..range.start, &opening).map_err(|error| error.to_string())?;
    range.start += opening.len();
    range.end += opening.len();
    edits.change_bytes(&mut bytes, range.end..range.end, &closing).map_err(|error| error.to_string())?;
    let selections = SelectionSet::single(Selection {
        anchor: ByteOffset(if reversed { range.end } else { range.start }),
        head: ByteOffset(if reversed { range.start } else { range.end }),
    });
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_block_comment(
    session: &mut EditorSession,
    mode: super::Comment,
    syntax: &super::CommentSyntax,
    last_target: &mut Option<std::ops::Range<i128>>,
) -> Result<(), String> {
    use super::Comment;
    if session.is_read_only() { return Ok(()); }
    let line_symbol = syntax.line.as_deref().filter(|symbol| !symbol.is_empty());
    let stream = syntax.stream.as_ref().filter(|(start, end)| !start.is_empty() && !end.is_empty());
    if line_symbol.is_none() {
        if stream.is_none() { return Ok(()); }
        if mode == Comment::UncommentLine {
            return apply_uncomment_stream(session, syntax, false, last_target);
        }
    }
    let mut bytes = session.document().byte_snapshot();
    let main = session.selections().primary().selection;
    let mut selection_start = main.range().start.0;
    let mut selection_end = main.range().end.0;
    let reversed = main.head.0 < selection_end;
    let first = bytes.line_index(selection_start).map_err(|error| error.to_string())?;
    let mut last = bytes.line_index(selection_end).map_err(|error| error.to_string())?;
    if last > first && selection_end == bytes.line_start(last).map_err(|error| error.to_string())? { last -= 1; }
    let mut edits = crate::editor::LineEdits::default();
    let mut uncomments = 0;
    for row in first..=last {
        let line_start = bytes.line_start(row).map_err(|error| error.to_string())?;
        let mut line_end = line_start;
        while line_end < bytes.len() && !bytes.is_line_end(line_end) { line_end += 1; }
        let mut indent = line_start;
        while indent < line_end && bytes.byte(indent).is_some_and(|byte| matches!(byte, b' ' | b'\t')) { indent += 1; }
        if indent == line_end && !syntax.comment_empty_lines { continue; }
        if syntax.at_line_start { indent = line_start; }
        let raw = bytes.slice(indent..line_end).map_err(|error| error.to_string())?.to_vec();
        let body = &raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())];
        if let Some(symbol) = line_symbol {
            let symbol = symbol.as_bytes();
            let matched = body.get(..symbol.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(symbol));
            if mode != Comment::Line && matched {
                let length = symbol.len() + usize::from(!syntax.omit_line_space && body.get(symbol.len()) == Some(&b' '));
                edits.change_bytes(&mut bytes, indent..indent + length, &[]).map_err(|error| error.to_string())?;
                if row == first {
                    if selection_start > indent + length { selection_start -= length; }
                    else if selection_start > indent { selection_start = indent; }
                }
                if row == last {
                    if selection_end > indent + length { selection_end -= length; }
                    else if selection_end > indent {
                        selection_end = indent;
                        if indent == line_start && row != first { selection_end += 1; }
                    }
                } else { selection_end -= length; }
                uncomments += 1;
            } else if mode != Comment::UncommentLine {
                let mut comment = symbol.to_vec();
                if !syntax.omit_line_space { comment.push(b' '); }
                edits.change_bytes(&mut bytes, indent..indent, &comment).map_err(|error| error.to_string())?;
                if row == first && selection_start >= indent { selection_start += comment.len(); }
                if row != last || selection_end >= indent { selection_end += comment.len(); }
            }
        } else if let Some((start, end)) = stream {
            let start = start.as_bytes();
            let end = end.as_bytes();
            let starts = mode != Comment::Line && body.get(..start.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(start));
            let suffix = if starts {
                Some(body.len().checked_sub(end.len()).ok_or_else(|| "The stream comment suffix is longer than the line".to_owned())?)
            } else { None };
            let matched = suffix.is_some_and(|at| body[at..].eq_ignore_ascii_case(end));
            if mode != Comment::Line && matched {
                let start_length = start.len() + usize::from(body.get(start.len()) == Some(&b' '));
                let suffix = body.len() - end.len();
                let end_length = end.len() + usize::from(suffix.checked_sub(1).and_then(|at| body.get(at)) == Some(&b' '));
                edits.change_bytes(&mut bytes, indent..indent + start_length, &[]).map_err(|error| error.to_string())?;
                let end_start = line_end.checked_sub(start_length + end_length).ok_or_else(|| "The stream comment range is outside the line".to_owned())?;
                edits.change_bytes(&mut bytes, end_start..line_end - start_length, &[]).map_err(|error| error.to_string())?;
                if row == first {
                    if selection_start > line_end - end_length { selection_start = line_end - start_length - end_length; }
                    else if selection_start > indent + start_length { selection_start -= start_length; }
                    else if selection_start > indent { selection_start = indent; }
                }
                if row == last {
                    if selection_end > line_end { selection_end -= start_length + end_length; }
                    else if selection_end > line_end - end_length { selection_end = line_end - start_length - end_length; }
                    else if selection_end > indent + start_length { selection_end -= start_length; }
                    else if selection_end > indent {
                        selection_end = indent;
                        if indent == line_start && row != first { selection_end += 1; }
                    }
                } else { selection_end -= start_length + end_length; }
                uncomments += 1;
            } else if mode != Comment::UncommentLine {
                let opening = [start, b" "].concat();
                let closing = [b" ".as_slice(), end].concat();
                edits.change_bytes(&mut bytes, indent..indent, &opening).map_err(|error| error.to_string())?;
                let at = line_end + opening.len();
                edits.change_bytes(&mut bytes, at..at, &closing).map_err(|error| error.to_string())?;
                if row == first && selection_start >= indent { selection_start += opening.len(); }
                if row != last || selection_end > line_end { selection_end += opening.len() + closing.len(); }
                else if selection_end >= indent { selection_end += opening.len(); }
            }
        }
    }
    let selection = Selection {
        anchor: ByteOffset(if reversed { selection_end } else { selection_start }),
        head: ByteOffset(if reversed { selection_start } else { selection_end }),
    };
    session.commit_prepared_bytes(bytes, SelectionSet::single(selection), edits).map_err(|error| error.to_string())?;
    if mode == Comment::UncommentLine && uncomments == 0 && stream.is_some() {
        return apply_uncomment_stream(session, syntax, false, last_target);
    }
    Ok(())
}

fn apply_move_lines(session: &mut EditorSession, down: bool) -> Result<(), String> {
    let mut bytes = session.document().byte_snapshot();
    let mut selections = session.selections();
    let rectangular = selections.is_rectangular();
    if rectangular {
        let main = selections.primary();
        let opposite = if selections.primary_index() == 0 { selections.ranges().last() } else { selections.ranges().first() }
            .ok_or_else(|| "The rectangular selection is empty".to_owned())?;
        selections = SelectionSet::new(vec![SelectionRange {
            selection: Selection { anchor: opposite.selection.anchor, head: main.selection.head },
            anchor_virtual: opposite.anchor_virtual,
            head_virtual: main.head_virtual,
        }], 0).map_err(|error| error.to_string())?;
    }
    let range = selections.primary().selection.range();
    let first = bytes.line_index(range.start.0).map_err(|error| error.to_string())?;
    let start = bytes.line_start(first).map_err(|error| error.to_string())?;
    let mut end = range.end.0;
    let mut end_line = bytes.line_index(end).map_err(|error| error.to_string())?;
    let beginning = bytes.line_start(end_line).map_err(|error| error.to_string())?;
    let mut append_eol = false;
    if end > beginning || start == end {
        end = if end_line + 1 < bytes.line_count() {
            bytes.line_start(end_line + 1).map_err(|error| error.to_string())?
        } else { bytes.len() };
        let row = bytes.line_index(end).map_err(|error| error.to_string())?;
        append_eol = end == bytes.len() && row == end_line;
        end_line = row;
    }
    let end_line_empty = bytes.line_start(end_line).map_err(|error| error.to_string())? == bytes.len();
    let mut edits = crate::editor::LineEdits::default();
    if (!down && start == 0)
        || (down && end == bytes.len() && !end_line_empty)
        || (start == end && !(!down && end_line_empty && end == bytes.len())) {
        if rectangular {
            return session.set_selections(selections).map_err(|error| error.to_string());
        }
        return Ok(());
    }
    if session.is_read_only() {
        let row = if down { first + 1 } else { first - 1 };
        let caret = ByteOffset(line_start_or_end(&bytes, row)?);
        return session.set_selections(SelectionSet::single(Selection { anchor: caret, head: caret })).map_err(|error| error.to_string());
    }
    if down && end == bytes.line_start(bytes.line_count() - 1).map_err(|error| error.to_string())? {
        let before = end.checked_sub(1).ok_or_else(|| "The line ending is before the document".to_owned())?;
        let at = bytes.move_position_outside_char(before, false, true).map_err(|error| error.to_string())?;
        edits.change_bytes(&mut bytes, at..end, &[]).map_err(|error| error.to_string())?;
        end = at;
    }
    let text = bytes.slice(start..end).map_err(|error| error.to_string())?.to_vec();
    let delete_start = if append_eol {
        let before = start.checked_sub(1).ok_or_else(|| "The line ending is before the document".to_owned())?;
        bytes.move_position_outside_char(before, false, true).map_err(|error| error.to_string())?
    } else { start };
    edits.change_bytes(&mut bytes, delete_start..end, &[]).map_err(|error| error.to_string())?;
    let target_line = if down { first + 1 } else { first - 1 };
    let eol = session.line_ending().as_str().as_bytes();
    if target_line >= bytes.line_count() {
        let end = bytes.len();
        edits.change_bytes(&mut bytes, end..end, eol).map_err(|error| error.to_string())?;
    }
    let position = bytes.line_start(target_line).map_err(|error| error.to_string())?;
    edits.change_bytes(&mut bytes, position..position, &text).map_err(|error| error.to_string())?;
    let mut length = text.len();
    if append_eol {
        let end = position + length;
        edits.change_bytes(&mut bytes, end..end, eol).map_err(|error| error.to_string())?;
        length += eol.len();
    }
    let selected = SelectionSet::single(Selection {
        anchor: ByteOffset(position + length), head: ByteOffset(position),
    });
    session.commit_prepared_bytes(bytes, selected, edits).map_err(|error| error.to_string())
}

fn apply_transpose_line(session: &mut EditorSession) -> Result<(), String> {
    let mut bytes = session.document().byte_snapshot();
    let row = bytes.line_index(session.selections().primary().selection.head.0).map_err(|error| error.to_string())?;
    if row == 0 { return Ok(()); }
    let previous = bytes.line_start(row - 1).map_err(|error| error.to_string())?;
    let current = bytes.line_start(row).map_err(|error| error.to_string())?;
    let mut previous_end = previous;
    while previous_end < bytes.len() && !bytes.is_line_end(previous_end) { previous_end += 1; }
    let mut current_end = current;
    while current_end < bytes.len() && !bytes.is_line_end(current_end) { current_end += 1; }
    let previous_text = bytes.slice(previous..previous_end).map_err(|error| error.to_string())?.to_vec();
    let current_text = bytes.slice(current..current_end).map_err(|error| error.to_string())?.to_vec();
    if session.is_read_only() {
        let caret = bytes.move_position_outside_char(current - previous_text.len(), false, true).map_err(|error| error.to_string())?;
        return session.select(Selection { anchor: ByteOffset(caret), head: ByteOffset(caret) }).map_err(|error| error.to_string());
    }
    let mut edits = crate::editor::LineEdits::default();
    edits.change_bytes(&mut bytes, current..current_end, &[]).map_err(|error| error.to_string())?;
    edits.change_bytes(&mut bytes, previous..previous_end, &[]).map_err(|error| error.to_string())?;
    let current = current - previous_text.len() + current_text.len();
    edits.change_bytes(&mut bytes, previous..previous, &current_text).map_err(|error| error.to_string())?;
    edits.change_bytes(&mut bytes, current..current, &previous_text).map_err(|error| error.to_string())?;
    let selected = SelectionSet::single(Selection { anchor: ByteOffset(current), head: ByteOffset(current) });
    session.commit_prepared_bytes(bytes, selected, edits).map_err(|error| error.to_string())
}

pub(crate) fn apply_redact_with_target(session: &mut EditorSession, bullet: bool) -> super::TransformOutcome {
    let mut last_target = None;
    let result = apply_redact_pipeline(session, bullet, &mut last_target);
    super::TransformOutcome { result, last_target }
}

fn apply_redact_pipeline(session: &mut EditorSession, bullet: bool, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let mut bytes = session.document().byte_snapshot();
    let mut selections = session.selections();
    let mut edits = crate::editor::LineEdits::default();
    let mask = if bullet { "●" } else { "█" };
    for index in 0..selections.ranges().len() {
        let selected = selections.ranges()[index].selection.range();
        let range = selected.start.0..selected.end.0;
        let count = bytes.count_characters(range.clone(), usize::MAX).map_err(|error| error.to_string())? as i32;
        if count <= 0 { continue; }
        let mut text = Vec::new();
        let mut position = range.start;
        for _ in 0..count {
            if matches!(bytes.byte(position), Some(b'\r' | b'\n')) {
                if let Some(byte) = bytes.byte(position) { text.push(byte); }
            } else { text.extend_from_slice(mask.as_bytes()); }
            position = bytes.next_position(position, true).map_err(|error| error.to_string())?;
        }
        selections.map_edit(range.start, range.len(), 0);
        selections.map_edit(range.start, 0, text.len());
        let start = range.start;
        *last_target = Some(start as i128..range.end as i128);
        edits.change_bytes(&mut bytes, range, &text).map_err(|error| error.to_string())?;
        *last_target = Some(start as i128..(start + text.len()) as i128);
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_uncomment_stream(
    session: &mut EditorSession,
    syntax: &super::CommentSyntax,
    try_block: bool,
    last_target: &mut Option<std::ops::Range<i128>>,
) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let Some((opening, closing)) = syntax.stream.as_ref().filter(|(start, end)| !start.is_empty() && !end.is_empty()) else {
        if try_block && syntax.line.as_ref().is_some_and(|symbol| !symbol.is_empty()) {
            return apply_block_comment(session, super::Comment::UncommentLine, syntax, last_target);
        }
        return Ok(());
    };
    let mut bytes = session.document().byte_snapshot();
    let mut selections = session.selections();
    let mut edits = crate::editor::LineEdits::default();
    loop {
        let main = selections.primary().selection;
        let start = main.range().start.0;
        let end = main.range().end.0;
        let reversed = main.head.0 < end;
        let before_open = find_comment_symbol(&bytes, opening, start, 0, last_target)?;
        let before_close = find_comment_symbol(&bytes, closing, start, 0, last_target)?;
        let after_open = find_comment_symbol(&bytes, opening, start, bytes.len(), last_target)?;
        let after_close = find_comment_symbol(&bytes, closing, start, bytes.len(), last_target)?;
        let around = |open_before: Option<usize>, close_before: Option<usize>, open_after: Option<usize>, close_after: Option<usize>| {
            open_before.zip(close_after).filter(|&(open, close)| {
                close_before.is_none_or(|previous| open >= previous)
                    && open_after.is_none_or(|next| close <= next)
            })
        };
        let pair = if let Some(pair) = around(before_open, before_close, after_open, after_close) {
            Some(pair)
        } else {
            let end_before_open = find_comment_symbol(&bytes, opening, end, 0, last_target)?;
            let end_before_close = find_comment_symbol(&bytes, closing, end, 0, last_target)?;
            let end_after_open = find_comment_symbol(&bytes, opening, end, bytes.len(), last_target)?;
            let end_after_close = find_comment_symbol(&bytes, closing, end, bytes.len(), last_target)?;
            around(end_before_open, end_before_close, end_after_open, end_after_close)
                .or_else(|| after_open.zip(end_before_close).filter(|&(open, close)| open < end && close > start)
                    .and_then(|(open, _)| after_close.map(|close| (open, close))))
        };
        let Some((open, mut close)) = pair else { break; };
        let mut close_length = closing.len();
        if close.checked_sub(1).and_then(|at| bytes.byte(at)) == Some(b' ') {
            close -= 1;
            close_length += 1;
        }
        edits.change_bytes(&mut bytes, close..close + close_length, &[]).map_err(|error| error.to_string())?;
        let mut open_length = opening.len();
        if bytes.byte(open + open_length) == Some(b' ') { open_length += 1; }
        edits.change_bytes(&mut bytes, open..open + open_length, &[]).map_err(|error| error.to_string())?;
        let new_start = if start > open {
            if start >= open + open_length { start - open_length } else { open }
        } else { start };
        let removed_end = if end >= close + close_length {
            open_length + close_length
        } else if end <= close {
            open_length
        } else { open_length + end - close };
        let new_end = end.checked_sub(removed_end).ok_or_else(|| "The uncommented selection is before the document".to_owned())?;
        selections = SelectionSet::single(Selection {
            anchor: ByteOffset(if reversed { new_end } else { new_start }),
            head: ByteOffset(if reversed { new_start } else { new_end }),
        });
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn find_comment_symbol(
    bytes: &crate::byte_buffer::ByteBuffer,
    symbol: &str,
    from: usize,
    to: usize,
    last_target: &mut Option<std::ops::Range<i128>>,
) -> Result<Option<usize>, String> {
    *last_target = Some(from as i128..to as i128);
    let found = bytes.find_literal_bytes(symbol.as_bytes(), from, to, false, false, true)
        .map_err(|error| error.to_string())?;
    if let Some(range) = &found { *last_target = Some(range.start as i128..range.end as i128); }
    Ok(found.map(|range| range.start))
}

fn apply_trim(session: &mut EditorSession, trim: super::Trim, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let original = session.selections();
    if original.is_rectangular() { return Ok(()); }
    let main = original.primary().selection;
    let entire = main.range().is_empty();
    let mut bytes = session.document().byte_snapshot();
    let original_length = bytes.len();
    let mut selections = original;
    let mut scope = 0..bytes.len();
    if !entire {
        let range = main.range();
        let start_line = bytes.line_index(range.start.0).map_err(|error| error.to_string())?;
        let end_line = bytes.line_index(range.end.0).map_err(|error| error.to_string())?;
        let end = if range.end.0 == line_start_or_end(&bytes, end_line)? {
            range.end.0
        } else { range.end.0.max(line_end(&bytes, end_line)?) };
        scope = line_start_or_end(&bytes, start_line)?..end;
        selections = SelectionSet::single(Selection {
            anchor: ByteOffset(scope.start), head: ByteOffset(scope.end),
        });
    }
    let scope_start = scope.start;
    let mut edits = crate::editor::LineEdits::default();
    let mut processed = false;
    while let Some(range) = {
        if scope_start != scope.end || processed { *last_target = Some(scope.start as i128..scope.end as i128); }
        find_trim(&bytes, scope.clone(), trim)?
    } {
        *last_target = Some(range.start as i128..range.end as i128);
        let reached_end = range.end == scope.end;
        let removed = range.len();
        scope.start = range.start;
        scope.end -= removed;
        selections.map_edit(range.start, removed, 0);
        let target_start = range.start;
        edits.change_bytes(&mut bytes, range, &[]).map_err(|error| error.to_string())?;
        *last_target = Some(target_start as i128..target_start as i128);
        processed = true;
        if reached_end { break; }
    }
    if !entire {
        selections = if processed {
            SelectionSet::single(Selection { anchor: ByteOffset(scope_start), head: ByteOffset(scope.end) })
        } else if bytes.len() == original_length {
            SelectionSet::single(main)
        } else { selections };
    }
    edits.suppress_current_modification_notifications();
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn find_trim(
    bytes: &crate::byte_buffer::ByteBuffer,
    scope: std::ops::Range<usize>,
    trim: super::Trim,
) -> Result<Option<std::ops::Range<usize>>, String> {
    use crate::regex_input::Utf8DocumentIterator;
    let start = bytes.move_position_outside_char(scope.start, true, false).map_err(|error| error.to_string())?;
    let end = bytes.move_position_outside_char(scope.end, true, false).map_err(|error| error.to_string())?;
    let mut iterator = Utf8DocumentIterator::new(bytes, start, end)?;
    while !iterator.at_end() {
        if !matches!(iterator.value(), 9 | 32) {
            iterator.next()?;
            continue;
        }
        let start = iterator.position();
        let leading = trim != super::Trim::Trailing && iterator.at_line_start()?;
        let mut after = iterator.clone();
        while !after.at_end() && matches!(after.value(), 9 | 32) { after.next()?; }
        if leading || (trim != super::Trim::Leading && after.at_line_end()?) {
            if after.position() > start {
                return Ok(Some(start..after.position()));
            }
            let next = bytes.next_position(start, true).map_err(|error| error.to_string())?.max(start + 1);
            if next > end { break; }
            iterator = Utf8DocumentIterator::new(bytes, next, end)?;
        } else {
            iterator.next()?;
        }
    }
    Ok(None)
}

fn apply_remove_empty_lines(session: &mut EditorSession, include_whitespace: bool, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let mut bytes = session.document().byte_snapshot();
    let mut selections = session.selections();
    let main = selections.primary().selection;
    let entire = main.range().is_empty();
    let mut scope = if entire { 0..bytes.len() } else { main.range().start.0..main.range().end.0 };
    let scope_start = scope.start;
    let mut edits = crate::editor::LineEdits::default();
    let mut processed = false;
    while let Some(range) = {
        if scope_start != scope.end || processed { *last_target = Some(scope.start as i128..scope.end as i128); }
        find_empty_lines(&bytes, scope.clone(), include_whitespace, false)?
    } {
        *last_target = Some(range.start as i128..range.end as i128);
        let reached_end = range.end == scope.end;
        let removed = range.len();
        scope.start = range.start;
        scope.end -= removed;
        selections.map_edit(range.start, removed, 0);
        let target_start = range.start;
        edits.change_bytes(&mut bytes, range, &[]).map_err(|error| error.to_string())?;
        *last_target = Some(target_start as i128..target_start as i128);
        processed = true;
        if reached_end { break; }
    }
    if !entire && processed {
        selections = SelectionSet::single(Selection { anchor: ByteOffset(scope_start), head: ByteOffset(scope.end) });
    }
    edits.suppress_current_modification_notifications();
    let end = if entire { bytes.len() } else { selections.primary().selection.range().end.0 };
    let start = if entire {
        bytes.line_start(bytes.line_count().saturating_sub(2)).map_err(|error| error.to_string())?
    } else {
        let row = bytes.line_index(end).map_err(|error| error.to_string())?;
        let line_end = if row + 1 < bytes.line_count() {
            bytes.line_start(row + 1).map_err(|error| error.to_string())?
        } else { bytes.len() };
        if end != line_end {
            return session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string());
        }
        selections.primary().selection.range().start.0
    };
    *last_target = Some(start as i128..end as i128);
    if let Some(range) = find_empty_lines(&bytes, start..end, include_whitespace, true)? {
        let range = range.start..end;
        *last_target = Some(range.start as i128..range.end as i128);
        selections.map_edit(range.start, range.len(), 0);
        let target_start = range.start;
        edits.change_bytes(&mut bytes, range, &[]).map_err(|error| error.to_string())?;
        *last_target = Some(target_start as i128..target_start as i128);
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn find_empty_lines(
    bytes: &crate::byte_buffer::ByteBuffer,
    scope: std::ops::Range<usize>,
    include_whitespace: bool,
    tail: bool,
) -> Result<Option<std::ops::Range<usize>>, String> {
    use crate::regex_input::Utf8DocumentIterator;
    let start = bytes.move_position_outside_char(scope.start, true, false).map_err(|error| error.to_string())?;
    let end = bytes.move_position_outside_char(scope.end, true, false).map_err(|error| error.to_string())?;
    let mut iterator = Utf8DocumentIterator::new(bytes, start, end)?;
    while !iterator.at_end() {
        let start = iterator.position();
        let mut matched = None;
        if tail {
            let mut after = iterator.clone();
            let mut endings = Vec::new();
            while !after.at_end() && matches!(after.value(), 10 | 13) {
                after.next()?;
                endings.push(after.clone());
            }
            for mut after in endings.into_iter().rev() {
                if !after.at_line_start()? { continue; }
                if include_whitespace {
                    while !after.at_end() && matches!(after.value(), 9 | 32) { after.next()?; }
                }
                if after.at_line_end()? { matched = Some(after.position()); break; }
            }
            if matched.is_none() && include_whitespace && iterator.at_line_start()? {
                let mut after = iterator.clone();
                let mut consumed = false;
                while !after.at_end() && matches!(after.value(), 9 | 32) {
                    after.next()?;
                    consumed = true;
                }
                if consumed && after.at_line_end()? { matched = Some(after.position()); }
            }
        } else if iterator.at_line_start()? {
            let mut after = iterator.clone();
            loop {
                if include_whitespace {
                    while !after.at_end() && matches!(after.value(), 9 | 32) { after.next()?; }
                }
                if after.at_end() || !matches!(after.value(), 10 | 13) { break; }
                while !after.at_end() && matches!(after.value(), 10 | 13) { after.next()?; }
                matched = Some(after.position());
                if !include_whitespace { break; }
            }
        }
        if let Some(matched_end) = matched {
            if matched_end > start { return Ok(Some(start..matched_end)); }
            let next = bytes.next_position(start, true).map_err(|error| error.to_string())?.max(start + 1);
            if next > end { break; }
            iterator = Utf8DocumentIterator::new(bytes, next, end)?;
        } else {
            iterator.next()?;
        }
    }
    Ok(None)
}

fn apply_consecutive_duplicates(session: &mut EditorSession) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let mut bytes = session.document().byte_snapshot();
    let mut selections = session.selections();
    let selected = selections.primary().selection;
    let (first, mut last) = selected_lines(&bytes, selected, true)?;
    if first == last { return Ok(()); }
    let mut edits = crate::editor::LineEdits::default();
    let mut first_match = first;
    let mut last_match = first;
    let mut first_text = String::new();
    let mut row = first;
    while row <= last {
        let start = bytes.line_start(row).map_err(|error| error.to_string())?;
        let next = if row + 1 < bytes.line_count() {
            bytes.line_start(row + 1).map_err(|error| error.to_string())?
        } else { bytes.len() };
        let raw = bytes.slice(start..next).map_err(|error| error.to_string())?.to_vec();
        let end = raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len());
        let line_text = String::from_utf8_lossy(&raw[..end]).into_owned();
        if first_text.is_empty() {
            first_match = row;
            last_match = row;
            first_text = line_text;
            row += 1;
            continue;
        }
        if first_text == line_text {
            last_match = row;
            if row != last { row += 1; continue; }
        }
        if first_match != last_match {
            let start = bytes.line_start(first_match + 1).map_err(|error| error.to_string())?;
            let end = if last_match + 1 < bytes.line_count() {
                bytes.line_start(last_match + 1).map_err(|error| error.to_string())?
            } else { bytes.len() };
            let range = start..end;
            selections.map_edit(range.start, range.len(), 0);
            edits.change_bytes(&mut bytes, range, &[]).map_err(|error| error.to_string())?;
            let removed = last_match - first_match;
            row -= removed;
            last -= removed;
        }
        first_text = line_text;
        first_match = row;
        last_match = row;
        row += 1;
    }
    let start = line_start_or_end(&bytes, last)?;
    let end = line_end(&bytes, last)?;
    if last > 0 && last + 1 == bytes.line_count() && end > start && end == bytes.len() {
        let previous_start = line_start_or_end(&bytes, last - 1)?;
        let previous_end = line_end(&bytes, last - 1)?;
        let raw = bytes.slice(start..end).map_err(|error| error.to_string())?.to_vec();
        let end = raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len());
        let final_text = String::from_utf8_lossy(&raw[..end]);
        let raw_previous = bytes.slice(previous_start..previous_end).map_err(|error| error.to_string())?.to_vec();
        let end_previous = raw_previous.iter().position(|&byte| byte == 0).unwrap_or(raw_previous.len());
        if final_text == String::from_utf8_lossy(&raw_previous[..end_previous]) {
            let range = previous_start..start;
            selections.map_edit(range.start, range.len(), 0);
            edits.change_bytes(&mut bytes, range, &[]).map_err(|error| error.to_string())?;
        }
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn apply_join(session: &mut EditorSession, expand_lines: bool, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    let mut selections = session.selections();
    if !expand_lines && selections.is_rectangular() { return Ok(()); }
    let mut bytes = session.document().byte_snapshot();
    let main = selections.primary().selection;
    let mut target = main.range().start.0..main.range().end.0;
    if expand_lines {
        let (first, last) = selected_lines(&bytes, main, false)?;
        if first == last { return Ok(()); }
        target = line_start_or_end(&bytes, first)?..line_end(&bytes, last)?;
        selections = SelectionSet::single(Selection { anchor: ByteOffset(target.start), head: ByteOffset(target.end) });
    } else if target.is_empty() {
        target = 0..bytes.len();
    }
    *last_target = Some(target.start as i128..target.end as i128);
    if session.is_read_only() { return session.set_selections(selections).map_err(|error| error.to_string()); }
    let row = bytes.line_index(target.start).map_err(|error| error.to_string())?;
    let mut target_end = i128::try_from(target.end).map_err(|error| error.to_string())?;
    let mut edits = crate::editor::LineEdits::default();
    loop {
        let mut at = bytes.line_start(row).map_err(|error| error.to_string())?;
        while at < bytes.len() && !bytes.is_line_end(at) { at += 1; }
        if i128::try_from(at).map_err(|error| error.to_string())? >= target_end { break; }
        let previous = at.checked_sub(1).and_then(|position| bytes.byte(position));
        let range = bytes.deletion_range(at, false).map_err(|error| error.to_string())?;
        target_end -= i128::try_from(range.len()).map_err(|error| error.to_string())?;
        *last_target = Some(target.start as i128..target_end);
        selections.map_edit(at, range.len(), 0);
        edits.change_bytes(&mut bytes, range, &[]).map_err(|error| error.to_string())?;
        if previous != Some(b' ') {
            selections.map_edit(at, 0, 1);
            edits.change_bytes(&mut bytes, at..at, b" ").map_err(|error| error.to_string())?;
            target_end += 1;
            *last_target = Some(target.start as i128..target_end);
        }
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

pub(crate) fn apply_duplicate(session: &mut EditorSession, for_line: bool, tab_width: usize) -> Result<(), String> {
    if session.is_read_only() { return Ok(()); }
    let mut selections = session.selections();
    let for_line = for_line || selections.ranges().iter().all(|range| range.is_empty());
    let mut bytes = session.document().byte_snapshot();
    let mut edits = crate::editor::LineEdits::default();
    let eol = session.line_ending().as_str().as_bytes();
    for index in 0..selections.ranges().len() {
        let selected = selections.ranges()[index];
        let mut range = selected.selection.range().start.0..selected.selection.range().end.0;
        if for_line {
            let row = bytes.line_index(selected.selection.head.0).map_err(|error| error.to_string())?;
            range.start = bytes.line_start(row).map_err(|error| error.to_string())?;
            range.end = range.start;
            while range.end < bytes.len() && !bytes.is_line_end(range.end) { range.end += 1; }
        }
        let text = bytes.slice(range.clone()).map_err(|error| error.to_string())?.to_vec();
        let mut at = range.end;
        if for_line {
            edits.change_bytes(&mut bytes, at..at, eol).map_err(|error| error.to_string())?;
            selections.map_edit(at, 0, eol.len());
            at += eol.len();
        }
        edits.change_bytes(&mut bytes, at..at, &text).map_err(|error| error.to_string())?;
        selections.map_edit(at, 0, text.len());
    }
    if selections.is_rectangular() {
        let main = selections.primary();
        let anchor_range = if selections.primary_index() == 0 {
            selections.ranges().last()
        } else {
            selections.ranges().first()
        }.ok_or_else(|| "The rectangular selection is empty".to_owned())?;
        let mut anchor = (anchor_range.selection.anchor.0, anchor_range.anchor_virtual);
        let mut head = (main.selection.head.0, main.head_virtual);
        let mut last = selections.ranges().iter().flat_map(|range| [
            (range.selection.anchor.0, range.anchor_virtual),
            (range.selection.head.0, range.head_virtual),
        ]).max().ok_or_else(|| "The rectangular selection is empty".to_owned())?;
        if for_line {
            let row = bytes.line_index(last.0).map_err(|error| error.to_string())?;
            last.0 += bytes.line_start(row + 1).map_err(|error| error.to_string())?
                - bytes.line_start(row).map_err(|error| error.to_string())?;
        }
        if anchor > head { anchor = last; } else { head = last; }
        selections = rectangle_from_bytes(&bytes, anchor, head, tab_width)?;
    }
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}

fn rectangle_from_bytes(
    bytes: &crate::byte_buffer::ByteBuffer,
    anchor: (usize, usize),
    head: (usize, usize),
    tab_width: usize,
) -> Result<SelectionSet, String> {
        let anchor_row = bytes.line_index(anchor.0).map_err(|error| error.to_string())?;
        let target_head_row = bytes.line_index(head.0).map_err(|error| error.to_string())?;
        let anchor_column = bytes.column(anchor.0, tab_width).map_err(|error| error.to_string())? + anchor.1;
        let head_column = bytes.column(head.0, tab_width).map_err(|error| error.to_string())? + head.1;
        let at_column = |row, target| -> Result<(usize, usize), String> {
            let at = bytes.find_column(row, target, tab_width).map_err(|error| error.to_string())?;
            let space = if bytes.is_line_end(at) {
                target.saturating_sub(bytes.column(at, tab_width).map_err(|error| error.to_string())?)
            } else { 0 };
            Ok((at, space))
        };
        let top = anchor_row.min(target_head_row);
        let bottom = anchor_row.max(target_head_row);
        let mut ranges = Vec::new();
        for row in top..=bottom {
            let anchor = at_column(row, anchor_column)?;
            let head = at_column(row, head_column)?;
            ranges.push(SelectionRange {
                selection: Selection { anchor: ByteOffset(anchor.0), head: ByteOffset(head.0) },
                anchor_virtual: anchor.1,
                head_virtual: head.1,
            });
        }
        SelectionSet::rectangle(ranges, target_head_row - top).map_err(|error| error.to_string())
}

fn apply_case(session: &mut EditorSession, case: super::Case, seed: u64, last_target: &mut Option<std::ops::Range<i128>>) -> Result<(), String> {
    use crate::editor::{EditError, LineEdits};
    let original = session.selections();
    if original.ranges().len() == 1 && original.primary().selection.range().is_empty() { return Ok(()); }
    let mut order: Vec<_> = (0..original.ranges().len()).collect();
    let reversed = original.ranges().last().is_some_and(|last| {
        last.selection.range().start < original.ranges()[0].selection.range().start
    });
    order.sort_by_key(|&index| original.ranges()[index].selection.range().start);
    let mut bytes = session.document().byte_snapshot();
    let mut edits = LineEdits::default();
    let mut shift = 0isize;
    let mut ranges = Vec::new();
    let mut working_selections = original.clone();
    for index in order {
        let selected = original.ranges()[index];
        let range = selected.selection.range();
        let start = range.start.0.checked_add_signed(shift).ok_or_else(|| EditError::InvalidRange.to_string())?;
        let mut end = range.end.0.checked_add_signed(shift).ok_or_else(|| EditError::InvalidRange.to_string())?;
        if end > start && end - start <= i32::MAX as usize / 2 {
            let raw = bytes.slice(start..end).map_err(|error| error.to_string())?.to_vec();
            let converted = super::convert_case(&String::from_utf8_lossy(&raw), case, seed);
            if converted.len() <= raw.len() * 2 + 1 {
                *last_target = Some(start as i128..end as i128);
                if !session.is_read_only() {
                    working_selections.map_edit(start, end - start, 0);
                    working_selections.map_edit(start, 0, converted.len());
                    edits.change_bytes(&mut bytes, start..end, converted.as_bytes()).map_err(|error| error.to_string())?;
                    *last_target = Some(start as i128..(start + converted.len()) as i128);
                }
                let delta = isize::try_from(converted.len()).map_err(|_| EditError::InvalidRange.to_string())?
                    - isize::try_from(raw.len()).map_err(|_| EditError::InvalidRange.to_string())?;
                shift = shift.checked_add(delta).ok_or_else(|| EditError::InvalidRange.to_string())?;
                end = start + converted.len();
            }
        }
        let forward = if selected.selection.anchor == selected.selection.head && original.is_rectangular() {
            selected.anchor_virtual < selected.head_virtual
        } else { selected.selection.anchor <= selected.selection.head };
        ranges.push(SelectionRange {
            selection: Selection {
                anchor: ByteOffset((if forward || original.ranges().len() == 1 { start } else { end }).min(bytes.len())),
                head: ByteOffset((if forward || original.ranges().len() == 1 { end } else { start }).min(bytes.len())),
            },
            anchor_virtual: selected.anchor_virtual,
            head_virtual: selected.head_virtual,
        });
    }
    if reversed && ranges.len() > 1 { ranges.reverse(); }
    if ranges.len() > 1 {
        session.commit_prepared_bytes(bytes, working_selections, edits).map_err(|error| error.to_string())?;
        session.set_multi_selection_ranges(&ranges);
        return Ok(());
    }
    let selections = SelectionSet::single(ranges[0].selection);
    session.commit_prepared_bytes(bytes, selections, edits).map_err(|error| error.to_string())
}
