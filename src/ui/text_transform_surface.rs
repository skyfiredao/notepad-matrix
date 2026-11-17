use super::*;
use crate::text_transforms::session::apply_command_with_target;
pub(super) use crate::text_transforms::session::comment_syntax;
use crate::text_transforms::{TransformCommand, TransformOptions};

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
pub(super) struct TransformAction {
    source_id: u32,
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    [
        ("ctrl-i", 42012),
        ("ctrl-j", 42013),
        ("ctrl-shift-up", 42014),
        ("ctrl-shift-down", 42015),
        ("ctrl-shift-u", 42016),
        ("ctrl-u", 42017),
        ("ctrl-q", 42022),
        ("ctrl-shift-q", 42023),
        ("ctrl-k", 42035),
        ("ctrl-shift-k", 42036),
        ("ctrl-alt-enter", 42057),
        ("ctrl-alt-shift-enter", 42058),
        ("alt-u", 42067),
        ("alt-shift-u", 42068),
        ("ctrl-alt-u", 42069),
        ("ctrl-alt-shift-u", 42070),
    ]
    .into_iter()
    .map(|(key, source_id)| {
        KeyBinding::new(key, TransformAction { source_id }, Some("MatrixEditor"))
    })
    .collect()
}

fn native_locale_keys(session: &EditorSession) -> Result<Vec<Vec<u8>>, String> {
    use objc2_foundation::{
        NSComparisonResult, NSLocale, NSRange, NSString, NSStringCompareOptions,
    };
    let snapshot = session.document().byte_snapshot();
    let selections = session.selections();
    let empty = selections.ranges().iter().all(|range| range.is_empty());
    let locale = NSLocale::currentLocale();
    let mut values = Vec::new();
    for row in 0..snapshot.line_count() {
        let start = snapshot.line_start(row).map_err(|error| error.to_string())?;
        let mut end = start;
        while end < snapshot.len() && !snapshot.is_line_end(end) { end += 1; }
        let mut key_range = start..end;
        if selections.is_rectangular() {
            key_range = start..start;
            for range in selections.ranges() {
                let selected = range.selection.range();
                if snapshot.line_index(selected.start.0).map_err(|error| error.to_string())? == row {
                    key_range = selected.start.0..if empty { end } else { selected.end.0 };
                    break;
                }
            }
        }
        key_range.end = key_range.end.min(key_range.start.saturating_add(i32::MAX as usize / 2));
        let raw = snapshot.slice(key_range).map_err(|error| error.to_string())?.to_vec();
        values.push(NSString::from_str(&String::from_utf8_lossy(&raw)));
    }
    let compare = |a: usize, b: usize| {
        let result = unsafe {
            values[a].compare_options_range_locale(
                &values[b],
                NSStringCompareOptions::CaseInsensitiveSearch
                    | NSStringCompareOptions::NumericSearch,
                NSRange::new(0, values[a].length()),
                Some(&locale),
            )
        };
        if result == NSComparisonResult::Ascending {
            std::cmp::Ordering::Less
        } else if result == NSComparisonResult::Descending {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    };
    let mut order: Vec<_> = (0..values.len()).collect();
    order.sort_by(|&a, &b| compare(a, b));
    let mut keys = vec![Vec::new(); values.len()];
    let mut rank = 0u64;
    for (slot, &index) in order.iter().enumerate() {
        if slot > 0 && compare(order[slot - 1], index) != std::cmp::Ordering::Equal {
            rank += 1;
        }
        keys[index] = rank.to_be_bytes().to_vec();
    }
    Ok(keys)
}

impl MatrixWindow {
    pub(super) fn apply_transform_action(
        &mut self,
        action: &TransformAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text_transform_command(action.source_id, window, cx);
    }

    pub(super) fn text_transform_command(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let Some(command) = TransformCommand::from_source_id(id) else {
            return;
        };
        if self.editor().has_composition() {
            self.notify_editor_error("Finish text composition before transforming text".into(), cx);
            return;
        }
        if let Some(step) = crate::macros::MacroStep::text_transform(id) {
            self.record_macro_step(step);
        }
        let language = self.workspace.active().language();
        let definition = match language {
            crate::syntax::Language::UserDefined(id) => self.udl.registry.definition(id),
            _ => None,
        };
        let mut options = TransformOptions::default();
        let tabs = self.editing_tab_preferences(language);
        options.tab_width = tabs.size;
        options.use_tabs = !tabs.spaces;
        if matches!(command, TransformCommand::Comment(_)) {
            match comment_syntax(language, definition) {
                Ok(syntax) => options.comment_syntax = syntax,
                Err(error) => {
                    self.notify_editor_error(error, cx);
                    return;
                }
            }
        }
        if command == TransformCommand::SplitLines {
            self.refresh_editor_syntax();
            match self.editor_measured_split_breaks(window) {
                Ok(breaks) => options.split_breaks = breaks,
                Err(error) => { self.notify_editor_error(error, cx); return; }
            }
        }
        if matches!(
            command,
            TransformCommand::Sort {
                kind: crate::text_transforms::Sort::Locale,
                ..
            }
        ) {
            let selections = self.editor().selections();
            if !selections.is_rectangular() && selections.ranges().len() > 1 {
                let response = window.prompt(gpui::PromptLevel::Warning, "排序未执行", Some("不支持对多重选区进行排序。"), &[gpui::PromptButton::ok("确定")], cx);
                cx.spawn_in(window, async move |_, _| {
                    if let Err(error) = response.await { eprintln!("locale sort warning failed: {error}"); }
                }).detach();
                return;
            }
            match native_locale_keys(self.editor()) {
                Ok(keys) => options.locale_keys = Some(keys),
                Err(error) => { self.notify_editor_error(error, cx); return; }
            }
        }
        options.random_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |duration| duration.as_secs());
        let outcome = if matches!(command, TransformCommand::Redact { .. }) {
            crate::text_transforms::session::apply_redact_with_target(self.editor_mut(), window.modifiers().shift)
        } else {
            apply_command_with_target(self.editor_mut(), id, options)
        };
        if let Some(target) = outcome.last_target { self.set_scintilla_shortcut_raw_target(target); }
        match outcome.result {
            Ok(()) => self.editor_changed(Ok(()), window, cx),
            Err(error) if matches!(command, TransformCommand::Sort { .. }) => {
                self.editor_changed(Ok(()), window, cx);
                let title = if matches!(command, TransformCommand::Sort { kind: crate::text_transforms::Sort::Locale, .. }) { "排序失败" } else { "排序错误" };
                let response = window.prompt(gpui::PromptLevel::Info, title, Some(&error), &[gpui::PromptButton::ok("确定")], cx);
                cx.spawn_in(window, async move |_, _| {
                    if let Err(error) = response.await { eprintln!("sort error prompt failed: {error}"); }
                }).detach();
            }
            Err(error) => self.notify_editor_error(error, cx),
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

}
