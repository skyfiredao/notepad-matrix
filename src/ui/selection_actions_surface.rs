use super::*;

impl MatrixWindow {
    pub(super) fn selection_action(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        if id == 42076 {
            let engine = settings["searchEngine"]["searchEngineChoice"]
                .as_u64()
                .unwrap_or(2)
                .min(5) as usize;
            let custom = settings["searchEngine"]["searchEngineCustom"]
                .as_str()
                .unwrap_or("");
            let engine = match engine {
                0 if custom.is_empty() => 2,
                3 => 1,
                value => value,
            };
            self.show_preferences(window, cx);
            self.preferences.page = preferences_surface::Page::SearchEngine;
            let result = preferences_surface::write_value(
                &mut self.preferences.settings,
                "searchEngine.searchEngineChoice",
                engine.into(),
            ).and_then(|_| preferences_surface::write_value(
                &mut self.preferences.settings,
                "searchEngine.searchEngineCustom",
                custom.into(),
            ));
            if let Err(error) = result {
                self.notify_editor_error(format!("搜索引擎首选项无法读取：{error}"), cx);
            }
            cx.notify();
            return;
        }
        let selections = self.editor().selections();
        if selections.ranges().len() != 1 {
            return;
        }
        if self.editor().document().selection().range().is_empty() {
            let result = self.editor_mut().expand_word_selection().map(|_| ());
            if result.is_err() { self.editor_changed(result, window, cx); return; }
            self.editor_changed(result, window, cx);
        }
        let range = self.editor().document().selection().range();
        let count = match self.editor().document().byte_snapshot().count_characters(range.start.0..range.end.0, usize::MAX) {
            Ok(count) => count,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let selected = match self.editor().document().text_in(range) {
            Ok(text) => text,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let selected = if count == 0 { "" } else { selected.split('\0').next().unwrap_or_default() };
        let selected: Vec<_> = selected.encode_utf16().take(2047).collect();
        if id == 42075 {
            let engine = settings["searchEngine"]["searchEngineChoice"]
                .as_u64()
                .unwrap_or(2);
            let custom: String = settings["searchEngine"]["searchEngineCustom"]
                .as_str()
                .unwrap_or("")
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            let template = match engine {
                0 if custom.starts_with("https://") || custom.starts_with("http://") => {
                    custom.as_str()
                }
                1 | 3 => "https://duckduckgo.com/?q=$(CURRENT_WORD)",
                4 => "https://search.yahoo.com/search?q=$(CURRENT_WORD)",
                5 => "https://stackoverflow.com/search?q=$(CURRENT_WORD)",
                _ => "https://www.google.com/search?q=$(CURRENT_WORD)",
            };
            let target = match self.online_search_target(template, &selected) {
                Ok(target) => target,
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            match url::Url::parse(&target) {
                Ok(url) if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() => {
                    cx.open_url(url.as_str())
                }
                _ => self.notify_editor_error("搜索引擎 URL 无效".into(), cx),
            }
        } else {
            let selected = String::from_utf16_lossy(&selected);
            let selected = if selected.contains('/') || selected.contains('\\') { selected }
                else { match self.filename_at_caret() {
                    Ok(filename) => filename,
                    Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
                } };
            let candidate = PathBuf::from(expand_path_environment(&selected));
            let path = if candidate.exists() {
                candidate
            } else {
                self.disk_file()
                    .and_then(|file| file.path().parent())
                    .map_or(candidate.clone(), |parent| parent.join(&candidate))
            };
            let _access = match crate::file_access::read(&path) {
                Ok(access) => access,
                Err(error) => { self.notify_editor_error(format!("无法访问所选路径: {error}"), cx); return; }
            };
            if (id == 42073 && !path.is_file()) || (id == 42074 && !path.exists()) {
                let response = window.prompt(gpui::PromptLevel::Info, "打开文件",
                    Some("您尝试打开的文件并不存在。"), &[gpui::PromptButton::ok("确定")], cx);
                cx.spawn_in(window, async move |_, _| {
                    if let Err(error) = response.await { eprintln!("selected path warning failed: {error}"); }
                }).detach();
                return;
            }
            if id == 42073 {
                self.open_recent_paths(vec![path], window, cx);
            } else if let Err(error) = crate::folder_workspace::folder_shell(
                &path,
                crate::folder_workspace::FolderShellCommand::Reveal,
            ) {
                self.notify_editor_error(format!("无法打开文件位置: {error}"), cx);
            }
        }
    }

    fn online_search_target(&self, template: &str, selected: &[u16]) -> Result<String, crate::editor::EditError> {
        use crate::editor::EditError;
        let template: Vec<_> = template.split('\0').next().unwrap_or_default().encode_utf16().take(259).collect();
        let template = String::from_utf16(&template).map_err(|_| EditError::InvalidBoundary)?;
        let expanded = expand_path_environment(&template);
        let input: Vec<_> = expanded.split('\0').next().unwrap_or_default().encode_utf16().take(259).collect();
        let mut output = Vec::new();
        let mut at = 0;
        while at < input.len() && output.len() < 259 {
            if input[at] == u16::from(b'$') && input.get(at + 1) == Some(&u16::from(b'('))
                && let Some(end) = input[at + 2..].iter().position(|unit| *unit == u16::from(b')')) {
                let end = at + 2 + end;
                let name = String::from_utf16_lossy(&input[at + 2..end].iter().copied().take(259).collect::<Vec<_>>());
                let value = match name.as_str() {
                    "CURRENT_WORD" => Some(selected.to_vec()),
                    "FULL_CURRENT_PATH" => Some(crate::clipboard::CopyPath::Full.text(&self.workspace).encode_utf16().take(259).collect()),
                    "CURRENT_DIRECTORY" => Some(crate::clipboard::CopyPath::Directory.text(&self.workspace).encode_utf16().take(259).collect()),
                    "FILE_NAME" | "NAME_PART" | "EXT_PART" => {
                        let filename = crate::clipboard::CopyPath::Name.text(&self.workspace);
                        let value = match (name.as_str(), filename.rfind('.')) {
                            ("NAME_PART", Some(dot)) => &filename[..dot],
                            ("EXT_PART", Some(dot)) => &filename[dot..],
                            ("EXT_PART", None) => "",
                            _ => &filename,
                        };
                        Some(value.encode_utf16().take(259).collect())
                    }
                    "NPP_DIRECTORY" | "NPP_FULL_FILE_PATH" => {
                        let executable = std::env::current_exe().map_err(|error| EditError::Storage(error.to_string()))?;
                        let value = if name == "NPP_DIRECTORY" {
                            executable.parent().ok_or_else(|| EditError::Storage("应用路径缺少所在目录".into()))?
                        } else { executable.as_path() };
                        Some(value.to_string_lossy().encode_utf16().take(259).collect())
                    }
                    "CURRENT_LINE" | "CURRENT_COLUMN" | "CURRENT_LINESTR" => {
                        let document = self.editor().document();
                        let bytes = document.byte_snapshot();
                        let caret = document.selection().head.0;
                        let line = bytes.line_index(caret).map_err(|error| EditError::Storage(error.to_string()))?;
                        let value = match name.as_str() {
                            "CURRENT_LINE" => line.to_string(),
                            "CURRENT_COLUMN" => bytes.column(caret, 4).map_err(|error| EditError::Storage(error.to_string()))?.to_string(),
                            _ => document.line_text(line)?,
                        };
                        Some(value.split('\0').next().unwrap_or_default().encode_utf16().take(2047).collect())
                    }
                    _ => None,
                };
                if let Some(value) = value {
                    output.extend(value.into_iter().take(259 - output.len()));
                    at = end + 1;
                    continue;
                }
            }
            output.push(input[at]);
            at += 1;
        }
        String::from_utf16(&output).map_err(|_| EditError::InvalidBoundary)
    }

    fn filename_at_caret(&self) -> Result<String, crate::editor::EditError> {
        let document = self.editor().document();
        let bytes = document.byte_snapshot();
        let caret = document.selection().head.0;
        let row = bytes.line_index(caret).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?;
        let start = document.line_start(row)?.0;
        let line = document.line_text(row)?;
        let line: Vec<_> = line.split('\0').next().unwrap_or_default().encode_utf16().take(2047).collect();
        let column = caret - start;
        if column > line.len() { return Ok(String::new()); }
        let is_delimiter = |at: usize, values: &str| line.get(at)
            .is_some_and(|unit| values.encode_utf16().any(|value| value == *unit));
        let mut from = column;
        while from > 0 && !is_delimiter(from, " \t[(\"<>") { from -= 1; }
        if is_delimiter(from, " \t[(\"<>") { from += 1; }
        let mut to = column;
        while to < line.len() && !is_delimiter(to, " \t:()[]<>\"\r\n") { to += 1; }
        if from > to { return Ok(String::new()); }
        Ok(String::from_utf16_lossy(&line[from..to]))
    }

}

fn expand_path_environment(value: &str) -> String {
    let mut output = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find('%') {
        output.push_str(&remaining[..start]);
        let after = &remaining[start + 1..];
        let Some(end) = after.find('%') else { output.push_str(&remaining[start..]); return output; };
        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) => output.push_str(&value),
            Err(_) => output.push_str(&remaining[start..start + end + 2]),
        }
        remaining = &after[end + 1..];
    }
    output.push_str(remaining);
    output
}
