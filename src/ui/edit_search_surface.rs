use super::*;
use crate::editor::EditError;

impl MatrixWindow {
    pub(super) fn execute_edit_search_command(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if matrix_surface::locked() || self.matrix_active() { return false; }
        match id {
            42020 | 42089 => {
                let tab_width = 4;
                let result = self
                    .editor_mut()
                    .document_mut()
                    .begin_or_end_selection(id == 42089, tab_width);
                self.editor_changed(result, window, cx);
            }
            42084..=42086 => self.insert_date_time(id, window, cx),
            42028 => {
                let read_only = !self.workspace.active().is_user_read_only();
                self.workspace.active_mut().set_user_read_only(read_only);
            }
            42033 => {
                let user_read_only = self.workspace.active().is_user_read_only();
                let result = self
                    .workspace
                    .active_mut()
                    .disk_file
                    .as_mut()
                    .ok_or_else(|| io::Error::other("当前文件尚未保存"))
                    .and_then(|file| file.toggle_read_only_attribute());
                match result {
                    Ok(()) => self
                        .workspace
                        .active_mut()
                        .set_user_read_only(user_read_only),
                    Err(error) => {
                        self.notify_editor_error(format!("更改文件只读属性失败: {error}"), cx)
                    }
                }
            }
            42073..=42076 => self.selection_action(id, window, cx),
            42102 | 42103 => self.workspace.set_all_user_read_only(id == 42102),
            42037 => {
                let response = window.prompt(gpui::PromptLevel::Info, "列选模式提示", Some(
                    "有3种方法可切换到列选模式:\n\n1.（键盘和鼠标）按住 Alt 键，同时鼠标左键拖动。\n\n2.（只需键盘）按住 Alt+Shift 键，并使用方向键。\n\n3.（键盘或鼠标）\n  将光标放在所需的列块开始位置，\n    执行“开始/结束 按列选取”菜单命令；\n  将光标移动到所需的列块结束位置，\n    再次执行“开始/结束 按列选取”菜单命令。"
                ), &[gpui::PromptButton::ok("确定")], cx);
                cx.spawn(async move |_, _| {
                    if let Err(error) = response.await { eprintln!("column mode prompt failed: {error}"); }
                })
                .detach();
            }
            43048 | 43049 | 43014 | 43015 => self.find_selected_text(id, window, cx),
            43011 => self.open_incremental(window, cx),
            43052 => self.open_character_range(window, cx),
            43045..=43047 => self.navigate_search_results(id, window, cx),
            43009 | 43053 => self.navigate_matching_brace(id == 43053, window, cx),
            43018..=43021 | 43050 | 43051 => self.bookmark_line_command(id, window, cx),
            44030 | 44031 => self.appearance_current_fold(id == 44030, window, cx),
            44011 => self.set_appearance_distraction_free(
                !self.preferences.appearance.distraction_free, window, cx,
            ),
            _ => return false,
        }
        cx.notify();
        true
    }

    fn insert_date_time(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};
        let settings = match self
            .settings_store
            .as_ref()
            .map(ConfigStore::load_settings)
            .transpose()
        {
            Ok(settings) => settings.unwrap_or_default(),
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        };
        let now = NSDate::date();
        let date = NSDateFormatter::new();
        let text = if id == 42086 {
            let format = settings["insertDateTime"]["customizedFormat"]
                .as_str()
                .unwrap_or("yyyy-MM-dd HH:mm:ss");
            format_custom_date_time(format, &now)
        } else {
            date.setDateStyle(if id == 42084 {
                NSDateFormatterStyle::ShortStyle
            } else {
                NSDateFormatterStyle::LongStyle
            });
            let time = NSDateFormatter::new();
            time.setTimeStyle(NSDateFormatterStyle::ShortStyle);
            let day = date.stringFromDate(&now);
            let clock = time.stringFromDate(&now);
            if settings["insertDateTime"]["reverseDefaultOrder"]
                .as_bool()
                .unwrap_or(false)
            {
                format!("{day} {clock}")
            } else {
                format!("{clock} {day}")
            }
        };
        self.editor_mut().begin_undo_action();
        let result = self.editor_mut().replace_selection_bytes(&[])
            .and_then(|()| self.editor_mut().add_bytes(text.split('\0').next().unwrap_or_default().as_bytes()));
        self.editor_mut().end_undo_action();
        self.editor_changed(result, window, cx);
    }

    fn find_selected_text(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let persistent = matches!(id, 43048 | 43049);
        if persistent && !self.find_created {
            self.find_created = true;
            self.initialize_search_history(window, cx);
        }
        if self.editor().selection().range().is_empty() {
            let result = self.editor_mut().expand_word_selection().map(|_| ());
            if result.is_err() {
                self.editor_changed(result, window, cx);
                return;
            }
            self.editor_changed(result, window, cx);
        }
        let selection = match self.editor().source_selection_limits() {
            Ok(selection) => selection,
            Err(error) => { self.editor_changed(Err(error), window, cx); return; }
        };
        let range = selection.range();
        let bytes = self.editor().document().byte_snapshot();
        if range.is_empty() { return; }
        let selected_characters = match self.editor().document().byte_snapshot().count_characters(range.start.0..range.end.0, usize::MAX) {
            Ok(count) => count,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let selected = match self.editor().document().text_in(range.clone()) {
            Ok(text) => text,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let query = selected.split('\0').next().unwrap_or_default();
        if query.is_empty() || persistent && selected_characters > self.find_fill_threshold {
            return;
        }
        let backwards = matches!(id, 43049 | 43015);
        let options = if persistent {
            crate::search::LiteralOptions {
                mode: crate::search::SearchMode::Normal,
                ..self.find_options
            }
        } else {
            Default::default()
        };
        if persistent {
            self.initialize_search_history(window, cx);
            self.find_query
                .update(cx, |field, cx| field.set_text(query, window, cx));
        }
        match crate::search::find_literal_bytes(&bytes, query, selection, options, backwards) {
            Ok(Some(found)) => {
                let result = self.select_search_result(found.range, backwards, window, cx);
                self.editor_changed(result, window, cx);
                let (status, message) = if found.wrapped {
                    if backwards {
                        (find_surface::FindStatus::TopReached, crate::localization::Text::resource("find-status-top-reached", "Find: Reached document beginning, first occurrence from the bottom found."))
                    } else {
                        (find_surface::FindStatus::EndReached, crate::localization::Text::resource("find-status-end-reached", "Find: Reached document end, first occurrence from the top found."))
                    }
                } else { (find_surface::FindStatus::NoMessage, "".into()) };
                self.set_find_status(status, message);
            }
            Ok(None) => self.set_find_not_found(&query),
            Err(error) => self.set_find_error(&error),
        }
    }

    pub(super) fn select_search_result(
        &mut self,
        range: std::ops::Range<ByteOffset>,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), EditError> {
        let bytes = self.editor().document().byte_snapshot();
        let normalize = |position: ByteOffset| -> Result<ByteOffset, EditError> {
            let position = position.0.min(bytes.len());
            if position == 0 { return Ok(ByteOffset(0)); }
            bytes.adjacent_position(position, false).and_then(|before| bytes.adjacent_position(before, true))
                .map(ByteOffset).map_err(|error| EditError::Storage(error.to_string()))
        };
        let start = normalize(range.start)?;
        let end = normalize(range.end)?;
        for position in [range.start, range.end] {
            let line = bytes.line_index(position.0.min(bytes.len())).map_err(|error| EditError::Storage(error.to_string()))?;
            self.ensure_editor_line_visible(line);
        }
        self.editor_mut().select(Selection { anchor: start, head: end })?;
        self.center_search_result(start..end, backwards, window);
        self.editor_follow_caret = true;
        self.editor_preferred_x = None;
        cx.notify();
        Ok(())
    }

    fn navigate_matching_brace(
        &mut self,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_builtin_syntax();
        let Some((at, target)) = self.matching_brace_positions() else { return; };
        let selection = if select {
            Selection { anchor: ByteOffset(at.min(target)), head: ByteOffset(at.max(target) + 1) }
        } else {
            Selection { anchor: ByteOffset(target), head: ByteOffset(target) }
        };
        let result = self.editor_mut().select(selection);
        self.editor_changed(result, window, cx);
    }

    pub(super) fn matching_brace_positions(&self) -> Option<(usize, usize)> {
        if !self.workspace.brace_match_allowed() {
            return None;
        }
        let bytes = self.editor().document().byte_snapshot();
        let caret = self.editor().document().selection().head.0;
        let is_brace = |byte: u8| b"{}[]()".contains(&byte);
        let at = caret.checked_sub(1).filter(|&at| bytes.byte(at).is_some_and(is_brace))
            .or_else(|| bytes.byte(caret).filter(|&byte| is_brace(byte)).map(|_| caret))?;
        let brace = bytes.byte(at)?;
        let projection = self.editor().document().projection();
        let style_at = |at| projection.display_position(ByteOffset(at), crate::byte_projection::Affinity::After)
            .ok().and_then(|at| self.builtin_brace_style(at.0));
        let style = style_at(at);
        let (open, close, forward) = match brace {
            b'{' => (b'{', b'}', true),
            b'[' => (b'[', b']', true),
            b'(' => (b'(', b')', true),
            b'}' => (b'{', b'}', false),
            b']' => (b'[', b']', false),
            _ => (b'(', b')', false),
        };
        let mut depth = 1;
        let scan: Box<dyn Iterator<Item = usize>> = if forward { Box::new(at + 1..bytes.len()) }
            else { Box::new((0..at).rev()) };
        for index in scan {
            if style_at(index) != style {
                continue;
            }
            let ch = bytes.byte(index)?;
            if ch == if forward { open } else { close } {
                depth += 1;
            }
            if ch == if forward { close } else { open } {
                depth -= 1;
            }
            if depth == 0 {
                return Some((at, index));
            }
        }
        None
    }

    fn bookmark_line_command(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let mut bytes = self.editor().document().byte_snapshot();
        let bookmarked: std::collections::BTreeSet<_> =
            self.editor().document().bookmarked_lines().collect();
        if id == 43050 {
            for line in 0..bytes.line_count() {
                let result = self.editor_mut().toggle_bookmark(Some(line));
                self.editor_changed(result, window, cx);
            }
            return;
        }
        let editable = !self.editor().document().is_read_only();
        let paste = if id == 43020 {
            let Some(text) = clipboard_surface::read_text(cx) else {
                return;
            };
            text.text.split('\0').next().unwrap_or_default().to_owned()
        } else {
            String::new()
        };
        let mut copied = Vec::new();
        let mut edits = crate::editor::LineEdits::default();
        let mut selections = self.editor().document().selections();
        for line in (0..bytes.line_count()).rev() {
            let marked = bookmarked.contains(&line);
            if marked != (id != 43051) {
                continue;
            }
            let start = match bytes.line_start(line) {
                Ok(start) => start,
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            let next = if line + 1 < bytes.line_count() {
                match bytes.line_start(line + 1) { Ok(next) => next, Err(error) => { self.notify_editor_error(error.to_string(), cx); return; } }
            } else { bytes.len() };
            if matches!(id, 43018 | 43019) {
                let raw = match bytes.slice(start..next) { Ok(raw) => raw.to_vec(), Err(error) => { self.notify_editor_error(error.to_string(), cx); return; } };
                let raw = &raw[..raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len())];
                copied.push(String::from_utf8_lossy(raw).into_owned());
            }
            if marked && matches!(id, 43018 | 43021) {
                let result = self.editor_mut().toggle_bookmark(Some(line));
                self.editor_changed(result, window, cx);
            }
            if editable && id != 43019 {
                let mut end = next;
                if id == 43020 {
                    while end > start && matches!(bytes.byte(end - 1), Some(b'\r' | b'\n')) { end -= 1; }
                }
                selections.map_edit(start, end - start, 0);
                selections.map_edit(start, 0, paste.len());
                if let Err(error) = edits.change_bytes(&mut bytes, start..end, paste.as_bytes()) {
                    self.editor_changed(Err(error), window, cx);
                    return;
                }
            }
        }
        if editable && id != 43019 {
            let result = self.editor_mut().commit_prepared_bytes(bytes, selections, edits);
            self.editor_changed(result, window, cx);
        }
        if matches!(id, 43018 | 43019) {
            copied.reverse();
            clipboard_surface::write_text(copied.concat(), cx);
        }
    }
}

pub(super) fn format_custom_date_time(format: &str, now: &objc2_foundation::NSDate) -> String {
    use objc2_foundation::{NSDateFormatter, NSString};

    fn pass(format: &str, time: bool, field: &impl Fn(&str) -> String) -> String {
        let mut output = String::new();
        let mut chars = format.chars().peekable();
        let mut quoted = false;
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    output.push('\'');
                } else {
                    quoted = !quoted;
                }
                continue;
            }
            if quoted {
                output.push(ch);
                continue;
            }
            let mut count = 1;
            while chars.peek() == Some(&ch) {
                chars.next();
                count += 1;
            }
            let token = if time {
                match (ch, count) {
                    ('h', 1) => Some("h"), ('h', _) => Some("hh"),
                    ('H', 1) => Some("H"), ('H', _) => Some("HH"),
                    ('m', 1) => Some("m"), ('m', _) => Some("mm"),
                    ('s', 1) => Some("s"), ('s', _) => Some("ss"),
                    _ => None,
                }
            } else {
                match (ch, count) {
                    ('d', 1) => Some("d"), ('d', 2) => Some("dd"),
                    ('d', 3) => Some("EEE"), ('d', _) => Some("EEEE"),
                    ('M', 1) => Some("M"), ('M', 2) => Some("MM"),
                    ('M', 3) => Some("MMM"), ('M', _) => Some("MMMM"),
                    ('y', 1 | 2) => Some("yy"), ('y', _) => Some("yyyy"),
                    ('g', _) => Some("G"),
                    _ => None,
                }
            };
            if let Some(token) = token {
                let value = field(token);
                if !time && ch == 'y' && count == 1 {
                    let value = value.trim_start_matches('0');
                    output.push_str(if value.is_empty() { "0" } else { value });
                } else {
                    output.push_str(&value);
                }
            } else {
                output.extend(std::iter::repeat_n(ch, count));
            }
        }
        output
    }

    let formatter = NSDateFormatter::new();
    let field = |token: &str| {
        formatter.setDateFormat(Some(&NSString::from_str(token)));
        formatter.stringFromDate(now).to_string()
    };
    let escaped = format.replace('t', "\u{1}");
    let time = pass(&escaped, true, &field);
    if time.encode_utf16().count() >= 260 { return String::new(); }
    let date = pass(&time, false, &field);
    if date.encode_utf16().count() >= 260 { return String::new(); }
    let midday = field("a");
    let mut output = String::new();
    let mut chars = date.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1}' {
            output.push(ch);
        } else if chars.peek() == Some(&'\u{1}') {
            chars.next();
            output.push_str(&midday);
        } else if let Some(initial) = midday.chars().next() {
            output.push(initial);
        }
    }
    output
}
