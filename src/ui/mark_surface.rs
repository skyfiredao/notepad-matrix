use super::*;
use crate::editor::EditError;

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
pub(super) struct SourceAction(pub u32);

fn indicator_run(ranges: &[std::ops::Range<ByteOffset>], length: usize, position: usize) -> (bool, usize, usize) {
    if ranges.is_empty() || length == 0 { return (false, 0, 0); }
    let position = position.min(length - 1);
    let mut start = 0;
    for range in ranges {
        if position < range.start.0 { return (false, start, range.start.0); }
        if position < range.end.0 { return (true, range.start.0, range.end.0); }
        start = range.end.0;
    }
    (false, start, length)
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    [
        ("ctrl-alt-i", 43011),
        ("alt-f2", 42075),
        ("ctrl-shift-b", 42020),
        ("alt-shift-b", 42089),
        ("ctrl-f3", 43048),
        ("ctrl-shift-f3", 43049),
        ("ctrl-alt-f3", 43014),
        ("ctrl-alt-shift-f3", 43015),
        ("f7", 43045),
        ("f4", 43046),
        ("shift-f4", 43047),
        ("ctrl-b", 43009),
        ("ctrl-alt-b", 43053),
        ("ctrl-m", 43054),
        ("ctrl-1", 43039),
        ("ctrl-2", 43040),
        ("ctrl-3", 43041),
        ("ctrl-4", 43042),
        ("ctrl-5", 43043),
        ("ctrl-0", 43044),
        ("ctrl-shift-1", 43033),
        ("ctrl-shift-2", 43034),
        ("ctrl-shift-3", 43035),
        ("ctrl-shift-4", 43036),
        ("ctrl-shift-5", 43037),
        ("ctrl-shift-0", 43038),
    ]
    .into_iter()
    .map(|(key, id)| KeyBinding::new(key, SourceAction(id), if id >= 43000 { None } else { Some("MatrixEditor") }))
    .chain(
        [("f7", 43045), ("f4", 43046), ("shift-f4", 43047)]
            .into_iter()
            .map(|(key, id)| KeyBinding::new(key, SourceAction(id), Some("MatrixFindResults"))),
    )
    .collect()
}

impl MatrixWindow {
    pub(super) fn execute_source_command(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.execute_source_command(id, window, cx)
        }) {
            return;
        }
        if crate::menu::source_command(id).is_none()
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.execute_extended_source(id, window, cx) || self.execute_edit_search_command(id, window, cx) {
            return;
        }
        match id {
            46300 => self.open_udl_folder(cx),
            46301 => cx.open_url("https://github.com/notepad-plus-plus/userDefinedLanguages"),
            43054 => {
                self.show_find(window, cx);
                self.find_mark_mode = true;
            }
            43022 | 43024 | 43026 | 43028 | 43030 => {
                let slot = (id - 43022) as usize / 2 + 1;
                if self.editor().source_selection_limits().is_ok_and(|selection| selection.range().is_empty()) {
                    let result = self.editor_mut().expand_word_selection().map(|_| ());
                    if result.is_err() { self.editor_changed(result, window, cx); return; }
                    self.editor_changed(result, window, cx);
                }
                let range = match self.mark_selection_or_word() {
                    Ok(range) => range,
                    Err(error) => { self.editor_changed(Err(error), window, cx); return; }
                };
                let bytes = self.editor().document().byte_snapshot();
                let selected = match self.editor().document().text_in(range) {
                    Ok(text) => text,
                    Err(error) => { self.editor_changed(Err(error), window, cx); return; }
                };
                let query = selected.split('\0').next().unwrap_or_default();
                if !query.is_empty() {
                    let options = crate::search::LiteralOptions {
                        match_case: self.mark_all_case_sensitive,
                        whole_word: self.mark_all_word_only,
                        ..Default::default()
                    };
                    match crate::search::literal_ranges_bytes(&bytes, query, 0, options) {
                        Ok(ranges) => {
                            let result = self
                                .editor_mut()
                                .document_mut()
                                .set_indicators(slot, ranges, false);
                            self.editor_changed(result, window, cx);
                        }
                        Err(error) => self.notify_editor_error(error.to_string(), cx),
                    }
                }
            }
            43062..=43066 => {
                let range = match self.mark_selection_or_word() {
                    Ok(range) => range,
                    Err(error) => { self.editor_changed(Err(error), window, cx); return; }
                };
                let result = self.editor_mut().document_mut().set_indicators(
                    (id - 43061) as usize,
                    vec![range],
                    false,
                );
                self.editor_changed(result, window, cx);
            }
            43023 | 43025 | 43027 | 43029 | 43031 | 43032 => {
                let slots = if id == 43032 {
                    1..6
                } else {
                    let slot = (id - 43023) as usize / 2 + 1;
                    slot..slot + 1
                };
                for slot in slots {
                    let result =
                        self.editor_mut()
                            .document_mut()
                            .set_indicators(slot, Vec::new(), true);
                    self.editor_changed(result, window, cx);
                }
            }
            43033..=43044 => {
                let backwards = id <= 43038;
                let slot = if backwards {
                    (id - 43032) as usize % 6
                } else {
                    (id - 43038) as usize % 6
                };
                let found = self.adjacent_indicator_range(slot, backwards);
                if let Some(range) = found {
                    let line = match self.editor().document().byte_snapshot().line_index(range.end.0) {
                        Ok(line) => line,
                        Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
                    };
                    self.ensure_editor_line_visible(line);
                    let result = self.editor_mut().select(if backwards {
                        Selection {
                            anchor: range.end,
                            head: range.start,
                        }
                    } else {
                        Selection {
                            anchor: range.start,
                            head: range.end,
                        }
                    });
                    self.editor_changed(result, window, cx);
                }
            }
            43055..=43061 => self.copy_marked_text(id, cx),
            _ => unreachable!("source command is registered without implementation"),
        }
        cx.notify();
    }

    fn mark_selection_or_word(&self) -> Result<std::ops::Range<ByteOffset>, EditError> {
        let selection = self.editor().source_selection_limits()?;
        if selection.range().is_empty() {
            let bytes = self.editor().document().byte_snapshot();
            let caret = self.editor().document().selection().head.0;
            let start = bytes.extend_word(caret, false).map_err(|error| EditError::Storage(error.to_string()))?;
            let end = bytes.extend_word(caret, true).map_err(|error| EditError::Storage(error.to_string()))?;
            Ok(ByteOffset(start)..ByteOffset(end))
        } else {
            Ok(selection.range())
        }
    }

    fn adjacent_indicator_range(&self, slot: usize, backwards: bool) -> Option<std::ops::Range<ByteOffset>> {
        let ranges = self.editor().document().indicator_ranges(slot);
        let length = self.editor().document().len_bytes();
        if ranges.is_empty() || length == 0 { return None; }
        let run = |position| indicator_run(&ranges, length, position);
        let (mut inside, mut start, mut end) = run(self.editor().document().selection().head.0);
        if start == 0 && end == length - 1 { return None; }
        let next_position = if backwards {
            if start == 0 { (inside, start, _) = run(length - 1); }
            if inside {
                (_, start, _) = run(start.saturating_sub(1));
                if start == 0 { (_, start, _) = run(length - 1); }
            }
            start.saturating_sub(1)
        } else {
            if end >= length { (inside, _, end) = run(0); }
            if inside {
                (_, _, end) = run(end);
                if end >= length { (_, _, end) = run(0); }
            }
            end
        };
        let (_, start, end) = run(next_position);
        run(start).0.then_some(ByteOffset(start)..ByteOffset(end))
    }

    pub(super) fn copy_marked_text(&mut self, id: u32, cx: &mut Context<Self>) {
        let slots = if id == 43060 {
            1..6
        } else {
            let slot = if id == 43061 {
                0
            } else {
                (id - 43054) as usize
            };
            slot..slot + 1
        };
        let ranges: Vec<_> = slots
            .flat_map(|slot| self.editor().document().indicator_ranges(slot))
            .collect();
        let bytes = self.editor().document().byte_snapshot();
        let mut fragments = Vec::new();
        for range in ranges {
            let raw = match bytes.slice(range.start.0..range.end.0) {
                Ok(raw) => raw.to_vec(),
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            let raw = &raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())];
            fragments.push((range.start, String::from_utf8_lossy(raw).into_owned()));
        }
        if id == 43060 {
            fragments.sort_by(|left, right| left.0.cmp(&right.0)
                .then_with(|| left.1.encode_utf16().cmp(right.1.encode_utf16())));
        }
        let fragments: Vec<_> = fragments.into_iter().map(|(_, text)| text).collect();
        if fragments.is_empty() {
            return;
        }
        let delimiter =
            if fragments.len() > 1 && fragments.iter().any(|text| text.contains(['\r', '\n'])) {
                "\r\n----\r\n"
            } else {
                "\r\n"
            };
        let mut copied = fragments.join(delimiter);
        if fragments.len() > 1 {
            copied.push_str("\r\n");
        }
        clipboard_surface::write_text(copied, cx);
    }

    pub(super) fn mark_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.mark_all(window, cx)) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        self.record_search_history(crate::search_history::Field::Find, window, cx);
        let bytes = self.editor().document().byte_snapshot();
        let selection = match self.editor().source_selection_limits() {
            Ok(selection) => selection,
            Err(error) => { self.editor_changed(Err(error), window, cx); return; }
        };
        let scope = match crate::search::literal_scope_bytes(
            &bytes,
            selection,
            self.find_options.wrap,
            self.find_backward,
            self.find_in_selection_active,
        ) {
            Ok(scope) => scope,
            Err(error) => {
                self.set_find_error(&error);
                cx.notify();
                return;
            }
        };
        if self.mark_purge && !query.is_empty() && !scope.is_empty() { self.clear_find_marks(window, cx); }
        let mut regex_state = self.editor().document().regex_state();
        let matches = if query.is_empty() || scope.is_empty() { Ok(Vec::new()) }
            else { crate::search::literal_ranges_in_bytes_with_state(&bytes, &query, scope, self.find_options, &mut regex_state) };
        self.editor_mut().document_mut().set_regex_state(regex_state);
        match matches {
            Ok(ranges) => {
                let count = ranges.len();
                let empty_string_found = ranges.iter().any(|range| range.is_empty());
                if self.mark_bookmark {
                    let mut lines = std::collections::BTreeSet::new();
                    for range in &ranges {
                        let first = match bytes.line_index(range.start.0) { Ok(line) => line, Err(error) => { self.notify_editor_error(error.to_string(), cx); return; } };
                        let end = if range.is_empty() { range.start.0 } else { range.end.0 - 1 };
                        let last = match bytes.line_index(end) { Ok(line) => line, Err(error) => { self.notify_editor_error(error.to_string(), cx); return; } };
                        lines.extend(first..=last);
                    }
                    for line in lines {
                        if !self
                            .editor()
                            .document()
                            .bookmarked_lines()
                            .any(|marked| marked == line)
                        {
                            let result = self.editor_mut().toggle_bookmark(Some(line));
                            self.editor_changed(result, window, cx);
                        }
                    }
                }
                let result = self
                    .editor_mut()
                    .document_mut()
                    .set_indicators(0, ranges, false);
                self.editor_changed(result, window, cx);
                self.set_find_status(find_surface::FindStatus::Message,
                    crate::localization::Text::resource(if count == 1 { "find-status-mark-1-match" } else { "find-status-mark-nb-matches" },
                        "Mark: $INT_REPLACE$ matches").replace("$INT_REPLACE$", crate::finder::commafy(count)).with_scope(self.find_scope_resource()));
                self.set_find_scope_reason(count, empty_string_found);
            }
            Err(error) => self.set_find_error(&error),
        }
        cx.notify();
    }

    pub(super) fn clear_find_marks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.clear_find_marks(window, cx)
        }) {
            return;
        }
        let range = if self.find_in_selection_active {
            match self.editor().source_selection_limits() {
                Ok(selection) => selection.range(),
                Err(error) => { self.editor_changed(Err(error), window, cx); return; }
            }
        } else {
            ByteOffset(0)..ByteOffset(self.editor().document().len_bytes())
        };
        if self.mark_bookmark {
            let bytes = self.editor().document().byte_snapshot();
            let first = match bytes.line_index(range.start.0) { Ok(line) => line, Err(error) => { self.notify_editor_error(error.to_string(), cx); return; } };
            let end = range.end.0.saturating_sub(1);
            let last = match bytes.line_index(end) { Ok(line) => line, Err(error) => { self.notify_editor_error(error.to_string(), cx); return; } };
            let lines: Vec<_> = self
                .editor()
                .document()
                .bookmarked_lines()
                .filter(|line| (first..=last).contains(line))
                .collect();
            for line in lines {
                let result = self.editor_mut().toggle_bookmark(Some(line));
                self.editor_changed(result, window, cx);
            }
        }
        let result = self
            .editor_mut()
            .document_mut()
            .clear_indicators_in(0, range);
        self.editor_changed(result, window, cx);
        self.set_find_status(find_surface::FindStatus::NoMessage, "");
        cx.notify();
    }
}
