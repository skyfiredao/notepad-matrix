use super::preferences_surface::{
    Commit, FieldSpec, Page, pref_group, pref_indicator, pref_label, pref_label_colour,
    pref_label_right, pref_value_label, source_rect, value_at, horizontal_px,
};
use super::*;
use super::dialog_font::px;
use serde_json::Value;

const SCALE: f32 = 1.75;

pub(super) struct State {
    pub(super) caret_epoch: std::time::Instant,
    pub(super) smart_token: Option<String>,
    pub(super) smart_origin: crate::workspace::ViewId,
    pub(super) distraction_free: bool,
    blink_task: Option<gpui::Task<()>>,
    appearance_subscription: Option<gpui::Subscription>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            caret_epoch: std::time::Instant::now(),
            smart_token: None,
            smart_origin: crate::workspace::ViewId::Primary,
            distraction_free: false,
            blink_task: None,
            appearance_subscription: None,
        }
    }
}

const DARK_COLORS: [(&str, &str, u32, f32, f32); 12] = [
    ("DarkMode.customColorMain", "内容背景", 0x202020, 134., 74.),
    (
        "DarkMode.customColorMenuHotTrack",
        "高亮项",
        0x454545,
        134.,
        94.,
    ),
    (
        "DarkMode.customColorActive",
        "控件背景",
        0x383838,
        134.,
        114.,
    ),
    (
        "DarkMode.customColorTop",
        "对话框背景",
        0x202020,
        134.,
        134.,
    ),
    ("DarkMode.customColorError", "错误", 0x0000b0, 134., 154.),
    ("DarkMode.customColorText", "文本", 0xe0e0e0, 244., 74.),
    (
        "DarkMode.customColorDarkText",
        "深色文本",
        0xc0c0c0,
        244.,
        94.,
    ),
    (
        "DarkMode.customColorDisabledText",
        "禁用文本",
        0x808080,
        244.,
        114.,
    ),
    ("DarkMode.customColorLinkText", "链接", 0x00ffff, 244., 134.),
    ("DarkMode.customColorEdge", "边缘", 0x646464, 354., 74.),
    (
        "DarkMode.customColorHotEdge",
        "边缘高亮",
        0x9b9b9b,
        354.,
        94.,
    ),
    (
        "DarkMode.customColorDisabledEdge",
        "边缘禁用",
        0x484848,
        354.,
        114.,
    ),
];

pub(super) fn fields() -> Vec<FieldSpec> {
    vec![
        FieldSpec::integer("tabCompactLabelLen", 10, 0, 257, Commit::Blur).signed_prefix(3),
        FieldSpec::text("ScintillaPrimaryView.edgeMultiColumnPos", "", 259),
    ]
}

fn label(text: &str, x: f32, y: f32, w: f32) -> AnyElement {
    let text = if text == "宽度：" {
        crate::localization::current_preference_control(if y == 60. { 6655 } else { 6217 }).unwrap_or(text)
    } else { text };
    pref_label(
        crate::localization::current_preference_label(text).unwrap_or(text),
        x * SCALE,
        y * SCALE,
        w * SCALE,
    )
}

fn group(text: &str, x: f32, y: f32, w: f32, h: f32) -> AnyElement {
    pref_group(
        crate::localization::current_preference_label(text).unwrap_or(text),
        x * SCALE,
        y * SCALE,
        w * SCALE,
        h * SCALE,
    )
}

fn label_right(text: &str, x: f32, y: f32, w: f32) -> AnyElement {
    let text = if text == "宽度：" {
        crate::localization::current_preference_control(if y == 60. { 6655 } else { 6217 })
            .unwrap_or(text)
    } else {
        text
    };
    pref_label_right(text, x * SCALE, y * SCALE, w * SCALE)
}

fn value_label(text: &str, x: f32, y: f32, w: f32) -> AnyElement {
    pref_value_label(text, x * SCALE, y * SCALE, w * SCALE)
}

fn setting<'a>(settings: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(settings, |value, key| value.get(key))
}

fn bool_setting(settings: &Value, path: &str, default: bool) -> bool {
    setting(settings, path)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn int_setting(settings: &Value, path: &str, default: i64) -> i64 {
    setting(settings, path)
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

fn put(settings: &mut Value, path: &str, value: Value) -> io::Result<()> {
    let mut current = settings;
    let mut keys = path.split('.').peekable();
    while let Some(key) = keys.next() {
        if current.is_null() {
            *current = Value::Object(Default::default());
        }
        let object = current.as_object_mut().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{path} must belong to an object"),
            )
        })?;
        if keys.peek().is_none() {
            object.insert(key.to_owned(), value);
            return Ok(());
        }
        current = object.entry(key.to_owned()).or_insert(Value::Null);
    }
    Ok(())
}

fn take(settings: &mut Value, section: &str, key: &str) -> Option<Value> {
    settings.get_mut(section)?.as_object_mut()?.remove(key)
}

fn invalid(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Invalid preference: {path}"),
    )
}

fn edge_columns(text: &str) -> io::Result<Vec<u16>> {
    if !text
        .bytes()
        .all(|byte| byte == b' ' || byte.is_ascii_digit())
    {
        return Err(invalid("ScintillaPrimaryView.edgeMultiColumnPos"));
    }
    text.split(' ')
        .filter(|part| !part.is_empty() && part.len() < 5)
        .map(|part| {
            part.parse::<u16>()
                .map_err(|_| invalid("ScintillaPrimaryView.edgeMultiColumnPos"))
        })
        .collect()
}

fn restore_theme_icons(settings: &mut Value, dark: bool) -> io::Result<()> {
    let (icon_path, color_path, custom_path, mono_path) = if dark {
        (
            "DarkMode.darkToolBarIconSet",
            "DarkMode.darkTbFluentColor",
            "DarkMode.darkTbFluentCustomColor",
            "DarkMode.darkTbFluentMono",
        )
    } else {
        (
            "DarkMode.lightToolBarIconSet",
            "DarkMode.lightTbFluentColor",
            "DarkMode.lightTbFluentCustomColor",
            "DarkMode.lightTbFluentMono",
        )
    };
    let icon = match int_setting(settings, icon_path, if dark { 0 } else { 4 }) {
        0 => "small",
        1 => "large",
        2 => "small2",
        3 => "large2",
        _ => "standard",
    };
    let color = int_setting(settings, color_path, 0);
    let custom = int_setting(settings, custom_path, 0);
    let mono = bool_setting(settings, mono_path, false);
    put(settings, "ToolBar.iconSet", icon.into())?;
    put(settings, "ToolBar.fluentColor", color.into())?;
    put(settings, "ToolBar.fluentCustomColor", custom.into())?;
    put(settings, "ToolBar.fluentMono", mono.into())
}

pub(super) fn appearance_changed(path: &str, settings: &mut Value) -> io::Result<()> {
    let range = match path {
        "MenuFont.size" => Some((8, 24)),
        "ToolBar.fluentColor" => Some((0, 9)),
        "Caret.width" => Some((0, 5)),
        "ScintillaPrimaryView.currentLineIndicator" => Some((0, 2)),
        "ScintillaPrimaryView.currentLineFrameWidth" => Some((1, 6)),
        "ScintillaPrimaryView.eolMode" => Some((0, 3)),
        "ScintillaPrimaryView.npcMode" => Some((0, 2)),
        "ScintillaPrimaryView.borderWidth"
        | "ScintillaPrimaryView.paddingLeft"
        | "ScintillaPrimaryView.paddingRight" => Some((0, 30)),
        "ScintillaPrimaryView.distractionFreeDivPart" => Some((3, 9)),
        "ScintillaPrimaryView.isChangeHistoryEnabled" => Some((0, 3)),
        _ => None,
    };
    if let Some((min, max)) = range {
        let value = setting(settings, path)
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid(path))?;
        if !(min..=max).contains(&value) {
            return Err(invalid(path));
        }
    }
    match path {
        "MenuFont.fontName" => {
            let name = setting(settings, path).and_then(Value::as_str).ok_or_else(|| invalid(path))?;
            if name.trim().is_empty() || name.contains('\0') { return Err(invalid(path)); }
        }
        "NativeLang.filename" => {
            let filename = setting(settings, path)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(path))?;
            let filename = crate::localization::validate(filename)
                .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
            put(settings, path, filename.into())?;
        }
        "ToolBar.iconSet" => {
            let value = setting(settings, path)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(path))?;
            let icon = match value {
                "small" => 0,
                "large" => 1,
                "small2" => 2,
                "large2" => 3,
                "standard" => 4,
                _ => return Err(invalid(path)),
            };
            let dark = settings["theme"] == "dark";
            put(
                settings,
                if dark {
                    "DarkMode.darkToolBarIconSet"
                } else {
                    "DarkMode.lightToolBarIconSet"
                },
                icon.into(),
            )?;
        }
        "ScintillaPrimaryView.lineWrapMethod" => {
            let value = setting(settings, path)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(path))?;
            if !["default", "aligned", "indent"].contains(&value) {
                return Err(invalid(path));
            }
        }
        "ScintillaPrimaryView.folderMarkStyle" => {
            let value = setting(settings, path)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(path))?;
            if !["simple", "arrow", "circle", "box", "none"].contains(&value) {
                return Err(invalid(path));
            }
        }
        "tabCompactLabelLen" => {
            let value = setting(settings, path)
                .and_then(Value::as_i64)
                .ok_or_else(|| invalid(path))?;
            settings[path] = value.clamp(0, 257).into();
        }
        "TabBar.closeButton" | "TabBar.pinButton" => {
            if !bool_setting(settings, "TabBar.closeButton", true)
                && !bool_setting(settings, "TabBar.pinButton", true)
            {
                put(settings, "TabBar.buttonsOninactiveTabs", false.into())?;
            }
        }
        "TabBar.alternateIcons" => {
            let value = take(settings, "TabBar", "alternateIcons")
                .and_then(|value| value.as_bool())
                .ok_or_else(|| invalid(path))?;
            let dark = settings["theme"] == "dark";
            put(
                settings,
                if dark {
                    "DarkMode.darkTabIconSet"
                } else {
                    "DarkMode.lightTabIconSet"
                },
                (if value {
                    1
                } else if dark {
                    2
                } else {
                    0
                })
                .into(),
            )?;
        }
        "ScintillaPrimaryView.multiSelection" => {
            if !bool_setting(settings, path, true) {
                put(
                    settings,
                    "ScintillaPrimaryView.columnSel2MultiEdit",
                    false.into(),
                )?;
            }
        }
        "ScintillaPrimaryView.eolCustomColor" => {
            let value = take(settings, "ScintillaPrimaryView", "eolCustomColor")
                .and_then(|value| value.as_bool())
                .ok_or_else(|| invalid(path))?;
            let mode = int_setting(settings, "ScintillaPrimaryView.eolMode", 1);
            put(
                settings,
                "ScintillaPrimaryView.eolMode",
                ((mode & 1) | if value { 2 } else { 0 }).into(),
            )?;
        }
        "ScintillaPrimaryView.npcAppearance" => {
            let value = take(settings, "ScintillaPrimaryView", "npcAppearance")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| invalid(path))?;
            if !(1..=2).contains(&value) {
                return Err(invalid(path));
            }
            put(settings, "ScintillaPrimaryView.npcMode", value.into())?;
        }
        "Caret.blinkSlider" => {
            let value = take(settings, "Caret", "blinkSlider")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| invalid(path))?;
            if !(50..=2500).contains(&value) {
                return Err(invalid(path));
            }
            put(
                settings,
                "Caret.blinkRate",
                (if value == 2500 { 0 } else { value }).into(),
            )?;
        }
        "Caret.blinkRate" => {
            let value = int_setting(settings, path, -1);
            if value != 0 && !(50..2500).contains(&value) {
                return Err(invalid(path));
            }
        }
        "ScintillaPrimaryView.changeHistoryMargin"
        | "ScintillaPrimaryView.changeHistoryIndicator" => {
            let key = if path.ends_with("Margin") {
                "changeHistoryMargin"
            } else {
                "changeHistoryIndicator"
            };
            let value = take(settings, "ScintillaPrimaryView", key)
                .and_then(|value| value.as_bool())
                .ok_or_else(|| invalid(path))?;
            let bit = if key == "changeHistoryMargin" { 1 } else { 2 };
            let state = int_setting(settings, "ScintillaPrimaryView.isChangeHistoryEnabled", 1);
            put(
                settings,
                "ScintillaPrimaryView.isChangeHistoryEnabled",
                ((state & !bit) | if value { bit } else { 0 }).into(),
            )?;
        }
        "ScintillaPrimaryView.edgeMultiColumnPos" => {
            let text = setting(settings, path)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(path))?;
            if text.encode_utf16().count() > 259 {
                return Err(invalid(path));
            }
            if edge_columns(text)?.len() != 1 {
                put(settings, "ScintillaPrimaryView.isEdgeBgMode", false.into())?;
            }
        }
        "SmartHighLight.matchCase" | "SmartHighLight.wholeWordOnly" => {
            if bool_setting(settings, path, false) {
                put(settings, "SmartHighLight.useFindSettings", false.into())?;
            }
        }
        "SmartHighLight.useFindSettings" => {
            if bool_setting(settings, path, false) {
                put(settings, "SmartHighLight.matchCase", false.into())?;
                put(settings, "SmartHighLight.wholeWordOnly", false.into())?;
            }
        }
        "darkMode.mode" => match int_setting(settings, path, -1) {
            0 => {
                settings["theme"] = "light".into();
                put(settings, "DarkMode.enable", false.into())?;
                put(settings, "DarkMode.enableWindowsMode", false.into())?;
                restore_theme_icons(settings, false)?;
            }
            1 => {
                settings["theme"] = "dark".into();
                put(settings, "DarkMode.enable", true.into())?;
                put(settings, "DarkMode.enableWindowsMode", false.into())?;
                restore_theme_icons(settings, true)?;
            }
            2 => put(settings, "DarkMode.enableWindowsMode", true.into())?,
            _ => return Err(invalid(path)),
        },
        "DarkMode.colorTone" => {
            let tone = int_setting(settings, path, -1);
            if !(0..=6).contains(&tone) && tone != 32 {
                return Err(invalid(path));
            }
        }
        "DarkMode.resetTone" => {
            let tone = take(settings, "DarkMode", "resetTone")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| invalid(path))?;
            let offset = match tone {
                0 => 0,
                1 => 0x000010,
                2 => 0x001000,
                3 => 0x200000,
                4 => 0x200010,
                5 => 0x201000,
                6 => 0x001010,
                _ => return Err(invalid(path)),
            };
            for (path, _, default, _, _) in DARK_COLORS {
                let color = match path {
                    "DarkMode.customColorTop"
                    | "DarkMode.customColorMenuHotTrack"
                    | "DarkMode.customColorActive"
                    | "DarkMode.customColorMain"
                    | "DarkMode.customColorHotEdge"
                    | "DarkMode.customColorDisabledEdge" => default + offset,
                    "DarkMode.customColorEdge" => {
                        default + offset + if tone == 0 { 0 } else { 0x1c1c1c }
                    }
                    _ => default,
                };
                put(settings, path, color.into())?;
            }
        }
        _ => {}
    }
    if path == "ToolBar.fluentCustomColor"
        || DARK_COLORS.iter().any(|(key, _, _, _, _)| *key == path)
    {
        let value = setting(settings, path)
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid(path))?;
        if value > 0xffffff {
            return Err(invalid(path));
        }
    }
    let dark = settings["theme"] == "dark";
    let snapshot_path = match path {
        "ToolBar.fluentColor" => Some(if dark {
            "DarkMode.darkTbFluentColor"
        } else {
            "DarkMode.lightTbFluentColor"
        }),
        "ToolBar.fluentCustomColor" => Some(if dark {
            "DarkMode.darkTbFluentCustomColor"
        } else {
            "DarkMode.lightTbFluentCustomColor"
        }),
        "ToolBar.fluentMono" => Some(if dark {
            "DarkMode.darkTbFluentMono"
        } else {
            "DarkMode.lightTbFluentMono"
        }),
        _ => None,
    };
    if let Some(snapshot_path) = snapshot_path {
        let value = setting(settings, path)
            .cloned()
            .ok_or_else(|| invalid(path))?;
        put(settings, snapshot_path, value)?;
    }
    Ok(())
}

impl MatrixWindow {
    pub(super) fn apply_appearance_preferences(
        &mut self,
        settings: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = int_setting(
            settings,
            "darkMode.mode",
            if settings["theme"] == "dark" { 1 } else { 0 },
        );
        let dark = if mode == 2 {
            matches!(
                window.appearance(),
                gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
            )
        } else {
            mode == 1
        };
        if mode == 2 {
            let mut effective = settings.clone();
            effective["theme"] = if dark { "dark" } else { "light" }.into();
            if let Err(error) = put(&mut effective, "DarkMode.enable", dark.into())
                .and_then(|_| restore_theme_icons(&mut effective, dark))
            {
                self.notify_editor_error(error.to_string(), cx);
            } else {
                self.preferences.settings = effective;
            }
        }
        if self.dark != dark {
            self.dark = dark;
            self.builtin.syntax_cache = None;
            self.syntax_cache = None;
        }
        self.tab_vertical = bool_setting(settings, "tab_vertical", false);
        self.tab_multiline = bool_setting(settings, "tab_multiline", false);
        self.tab_compact_label_len =
            int_setting(settings, "tabCompactLabelLen", 10).clamp(0, 257) as usize;
        self.line_copy_cut_without_selection =
            bool_setting(settings, "lineCopyCutWithoutSelection", true);
        self.builtin.tag_match_options.enabled =
            bool_setting(settings, "TagsMatchHighLight.enabled", true);
        self.builtin.tag_match_options.attributes =
            bool_setting(settings, "TagsMatchHighLight.TagAttrHighLight", true);
        self.update_appearance_smart_token();
        if !bool_setting(settings, "ScintillaPrimaryView.multiSelection", true) {
            self.editor_mut().clear_additional_selections();
            let previous = self.pane;
            if self.scope_editor_pane(previous.other()) {
                self.editor_mut().clear_additional_selections();
                self.scope_editor_pane(previous);
            }
        }
        self.restart_appearance_caret(int_setting(settings, "Caret.blinkRate", 500), window, cx);
        if self
            .preferences
            .appearance
            .appearance_subscription
            .is_none()
        {
            self.preferences.appearance.appearance_subscription =
                Some(cx.observe_window_appearance(window, |view, window, cx| {
                    if view.pref_int("darkMode.mode", 0) == 2 {
                        let settings = view.preferences.settings.clone();
                        view.apply_appearance_preferences(&settings, window, cx);
                    }
                }));
        }
        self.invalidate_style_layout(window, cx);
        window.notify_text_input_changed(&self.editor_focus);
        cx.notify();
    }

    pub(super) fn appearance_caret_visible(&self) -> bool {
        let period = self.pref_int("Caret.blinkRate", 500);
        period == 0
            || (self
                .preferences
                .appearance
                .caret_epoch
                .elapsed()
                .as_millis()
                / period.max(1) as u128)
                .is_multiple_of(2)
    }

    pub(super) fn restart_appearance_caret(
        &mut self,
        period: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.appearance.caret_epoch = std::time::Instant::now();
        self.preferences.appearance.blink_task = None;
        if period <= 0 {
            return;
        }
        let executor = cx.background_executor().clone();
        self.preferences.appearance.blink_task =
            Some(cx.spawn_in(window, async move |view, cx| {
                loop {
                    executor
                        .timer(std::time::Duration::from_millis(period as u64))
                        .await;
                    if view.update_in(cx, |_, _, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
    }

    pub(super) fn prepare_appearance_column_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let selections = self.editor().selections();
        if !selections.is_rectangular()
            || !self.pref_bool("ScintillaPrimaryView.columnSel2MultiEdit", true)
        {
            return true;
        }
        let result = crate::editor::SelectionSet::new(
            selections.ranges().to_vec(),
            selections.primary_index(),
        )
        .and_then(|selections| self.editor_mut().set_selections(selections));
        match result {
            Ok(()) => true,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                false
            }
        }
    }

    pub(super) fn appearance_smart_indicators(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<crate::builtin_language::TagIndicator> {
        if !self.pref_bool("SmartHighLight.enabled", true)
            || (self.preferences.appearance.smart_origin != self.pane
                && !self.pref_bool("SmartHighLight.onAnotherView", false))
        {
            return Vec::new();
        }
        let Some(token) = &self.preferences.appearance.smart_token else {
            return Vec::new();
        };
        let text = self.editor().display_snapshot();
        let bytes = crate::byte_buffer::ByteBuffer::from_bytes(
            text.slice(start..end).to_string().as_bytes(),
        );
        let find = self.pref_bool("SmartHighLight.useFindSettings", false);
        let options = crate::search::LiteralOptions {
            match_case: if find {
                self.find_options.match_case
            } else {
                self.pref_bool("SmartHighLight.matchCase", false)
            },
            whole_word: if find {
                self.find_options.whole_word
            } else {
                self.pref_bool("SmartHighLight.wholeWordOnly", true)
            },
            ..Default::default()
        };
        match crate::search::literal_ranges_bytes(&bytes, token, 0, options) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|range| crate::builtin_language::TagIndicator {
                    range: start + range.start.0..start + range.end.0,
                    color: self.style_colour("Smart Highlighting", "bgColor", 0x00ff00),
                    alpha: 100,
                })
                .collect(),
            Err(error) => {
                eprintln!("Smart highlighting failed: {error}");
                Vec::new()
            }
        }
    }

    pub(super) fn update_appearance_smart_token(&mut self) {
        let selection = self.editor().display_selection().range();
        let snapshot = self.editor().display_snapshot();
        let whole_word = if self.pref_bool("SmartHighLight.useFindSettings", false) {
            self.find_options.whole_word
        } else {
            self.pref_bool("SmartHighLight.wholeWordOnly", true)
        };
        self.preferences.appearance.smart_token = if !selection.is_empty()
            && selection.end.0 <= snapshot.len()
        {
            let token = snapshot
                .slice(selection.start.0..selection.end.0)
                .to_string();
            let word_matches = !whole_word
                || (token.chars().all(|ch| {
                    crate::unicode_class::classify(ch) == crate::unicode_class::CharacterClass::Word
                }) && self
                    .editor()
                    .word_selection_at(selection.start)
                    .is_ok_and(|word| word.range() == selection));
            (!token.contains(['\r', '\n']) && !token.is_empty() && word_matches).then_some(token)
        } else {
            None
        };
        self.preferences.appearance.smart_origin = self.pane;
    }

    pub(super) fn appearance_selected_runs(
        &self,
        start: usize,
        runs: Vec<(gpui::TextRun, gpui::Pixels)>,
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        if !self.pref_bool(
            "ScintillaPrimaryView.selectedTextForegroundSingleColor",
            false,
        ) {
            return runs;
        }
        let selected = self.editor().display_selections();
        let color = rgb(self.style_colour("Selected text colour", "fgColor", 0x000000));
        let mut output = Vec::new();
        let mut at = start;
        for (run, size) in runs {
            let end = at + run.len;
            let mut cuts = vec![at, end];
            for range in selected.ranges() {
                let range = range.selection.range();
                cuts.extend([range.start.0.clamp(at, end), range.end.0.clamp(at, end)]);
            }
            cuts.sort_unstable();
            cuts.dedup();
            for pair in cuts.windows(2) {
                let mut run = run.clone();
                run.len = pair[1] - pair[0];
                if selected.ranges().iter().any(|range| {
                    let range = range.selection.range();
                    pair[0] >= range.start.0 && pair[1] <= range.end.0
                }) {
                    run.color = color.into();
                }
                output.push((run, size));
            }
            at = end;
        }
        output
    }

    pub(super) fn set_appearance_distraction_free(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_state.distraction_free != enabled {
            self.execute_view_source(44011, window, cx);
        }
    }

    pub(super) fn appearance_current_fold(
        &mut self,
        collapse: bool,
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
        self.refresh_editor_syntax();
        let snapshot = self.editor().display_snapshot();
        let line = snapshot.byte_to_line_idx(
            self.editor().display_selection().head.0,
            ropey::LineType::LF_CR,
        );
        let header = self
            .udl_fold_ranges()
            .into_iter()
            .filter(|(header, end)| *header <= line && line <= *end)
            .map(|(header, _)| header)
            .max();
        if let Some(header) = header {
            if self.pref_bool("MISC.enableFoldCmdToggable", false)
                || self.editor_fold_collapsed(header) != collapse
            {
                self.toggle_editor_fold(header, window, cx);
            }
        }
    }

    pub(super) fn appearance_padding(&self, left: bool) -> f32 {
        if self.preferences.appearance.distraction_free {
            self.editor_bounds.map_or(0., |bounds| {
                f32::from(bounds.size.width)
                    / self
                        .pref_int("ScintillaPrimaryView.distractionFreeDivPart", 4)
                        .clamp(3, 9) as f32
            })
        } else {
            self.pref_int(
                if left {
                    "ScintillaPrimaryView.paddingLeft"
                } else {
                    "ScintillaPrimaryView.paddingRight"
                },
                0,
            ) as f32
        }
    }

    pub(super) fn style_colour(&self, name: &str, attribute: &str, fallback: u32) -> u32 {
        self.style_config
            .global_style_named(name, self.dark)
            .and_then(|style| style.attributes.get(attribute))
            .and_then(|value| u32::from_str_radix(value, 16).ok())
            .unwrap_or(fallback)
    }

    pub(super) fn appearance_colour(&self, path: &'static str, fallback: u32) -> u32 {
        if !self.dark {
            return fallback;
        }
        let tone = self.pref_int("DarkMode.colorTone", 0);
        let offset = match tone {
            1 => 0x000010,
            2 => 0x001000,
            3 => 0x200000,
            4 => 0x200010,
            5 => 0x201000,
            6 => 0x001010,
            _ => 0,
        };
        let default = DARK_COLORS
            .iter()
            .find(|(key, _, _, _, _)| *key == path)
            .map_or(0x202020, |(_, _, value, _, _)| *value);
        let value = if tone == 32 {
            self.pref_int(path, i64::from(default)) as u32
        } else {
            default
                + match path {
                    "DarkMode.customColorTop"
                    | "DarkMode.customColorMenuHotTrack"
                    | "DarkMode.customColorActive"
                    | "DarkMode.customColorMain"
                    | "DarkMode.customColorHotEdge"
                    | "DarkMode.customColorDisabledEdge" => offset,
                    "DarkMode.customColorEdge" => offset + if tone == 0 { 0 } else { 0x1c1c1c },
                    _ => 0,
                }
        };
        ((value & 0xff) << 16) | (value & 0xff00) | ((value >> 16) & 0xff)
    }

    fn appearance_check(
        &self,
        id: &'static str,
        text: &'static str,
        path: &'static str,
        default: bool,
        invert: bool,
        enabled: bool,
        x: f32,
        y: f32,
        w: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.pref_check(
            id,
            text,
            path,
            default,
            invert,
            enabled,
            x * SCALE,
            y * SCALE,
            w * SCALE,
            cx,
        )
    }

    fn appearance_radio(
        &self,
        id: &'static str,
        text: &'static str,
        path: &'static str,
        default: Value,
        value: Value,
        enabled: bool,
        x: f32,
        y: f32,
        w: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.pref_radio(
            id,
            text,
            path,
            default,
            value,
            enabled,
            x * SCALE,
            y * SCALE,
            w * SCALE,
            cx,
        )
    }

    fn appearance_slider(
        &self,
        path: &'static str,
        min: i64,
        max: i64,
        default: i64,
        x: f32,
        y: f32,
        w: f32,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.pref_slider(
            path,
            min,
            max,
            default,
            x * SCALE,
            y * SCALE,
            w * SCALE,
            enabled,
            cx,
        )
    }

    fn appearance_toolbar_radio(
        &self,
        id: &'static str,
        text: &'static str,
        default: &'static str,
        value: &'static str,
        y: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = crate::localization::current_preference_source_label(id, text);
        let palette = self.preferences_render_palette();
        let current = value_at(&self.preferences.settings, "ToolBar.iconSet");
        let default = Value::from(default);
        let selected = (if current.is_null() { &default } else { current }) == &Value::from(value);
        let focus = self.preference_control_focus(id, cx).tab_stop(selected);
        self.preferences
            .radio_groups
            .borrow_mut()
            .entry("ToolBar.iconSet")
            .or_default()
            .push((id, value.into(), true));
        let highlights = label.find("Fluent UI").map(|start| {
            (
                start..start + "Fluent UI".len(),
                gpui::HighlightStyle {
                    color: Some(rgb(palette.link_text).into()),
                    ..Default::default()
                },
            )
        });
        let text = gpui::StyledText::new(gpui::SharedString::from(label.to_owned()))
            .with_highlights(highlights);
        source_rect(18. * SCALE, y * SCALE, 112. * SCALE, 10. * SCALE)
            .id(id)
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_toggled(selected.into())
            .track_focus(&focus)
            .flex()
            .items_center()
            .gap(horizontal_px(3.))
            .text_size(px(12.))
            .line_height(px(14.))
            .text_color(rgb(palette.text))
            .when(crate::localization::current_preference_rtl(), |element| {
                element.flex_row_reverse()
            })
            .child(pref_indicator(selected, true, true))
            .child(text)
            .on_click(cx.listener(move |view, _, window, cx| {
                focus.focus(window, cx);
                view.set_preference("ToolBar.iconSet", value.into(), window, cx);
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "space" {
                    view.set_preference("ToolBar.iconSet", value.into(), window, cx);
                    cx.stop_propagation();
                }
                if matches!(
                    event.keystroke.key.as_str(),
                    "left" | "up" | "right" | "down"
                ) {
                    let group: Vec<_> = view
                        .preferences
                        .radio_groups
                        .borrow()
                        .get("ToolBar.iconSet")
                        .into_iter()
                        .flatten()
                        .filter(|(_, _, enabled)| *enabled)
                        .cloned()
                        .collect();
                    if let Some(index) = group.iter().position(|(candidate, _, _)| *candidate == id)
                    {
                        let index = if matches!(event.keystroke.key.as_str(), "left" | "up") {
                            (index + group.len() - 1) % group.len()
                        } else {
                            (index + 1) % group.len()
                        };
                        let (target, value, _) = &group[index];
                        view.preference_control_focus(target, cx)
                            .focus(window, cx);
                        view.set_preference("ToolBar.iconSet", value.clone(), window, cx);
                        cx.stop_propagation();
                    }
                }
            }))
            .into_any_element()
    }

    fn appearance_button(
        &self,
        id: &'static str,
        text: &'static str,
        enabled: bool,
        x: f32,
        y: f32,
        w: f32,
        callback: fn(&mut MatrixWindow, &mut Window, &mut Context<MatrixWindow>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.pref_button(
            id,
            text,
            enabled,
            x * SCALE,
            y * SCALE,
            w * SCALE,
            callback,
            cx,
        )
    }

    fn appearance_help(
        &self,
        id: &'static str,
        key: &'static str,
        text: &'static str,
        enabled: bool,
        x: f32,
        y: f32,
        w: f32,
    ) -> AnyElement {
        let text = crate::localization::current_string(key).unwrap_or(text).to_owned();
        let palette = self.preferences_render_palette();
        source_rect(x * SCALE, y * SCALE, w * SCALE, 10. * SCALE)
            .id(id)
            .text_size(px(12.))
            .line_height(px(14.))
            .whitespace_nowrap()
            .text_color(rgb(if enabled { palette.link_text } else { palette.disabled_text }))
            .child("(?)")
            .when(enabled, |element| element.tooltip(move |_, cx| icon_tooltip::view(text.clone(), palette.dark, cx)))
            .into_any_element()
    }

    fn appearance_choice(
        &self,
        path: &'static str,
        options: Vec<(Value, String)>,
        default: Value,
        enabled: bool,
        x: f32,
        y: f32,
        w: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let options = options
            .into_iter()
            .map(|(value, text)| {
                let label = if path == "DarkMode.resetTone" {
                    crate::localization::current_preference_label(&text)
                        .map(str::to_owned)
                        .unwrap_or(text)
                } else {
                    text
                };
                (value, label)
            })
            .collect();
        self.pref_choice(
            path,
            options,
            default,
            enabled,
            x * SCALE,
            y * SCALE,
            w * SCALE,
            cx,
        )
    }

    pub(super) fn render_preferences_appearance(
        &self,
        page: Page,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filename = self.pref_string("NativeLang.filename", "english.xml");
        let section = match page {
            Page::General => "Global",
            Page::Toolbar => "Toolbar",
            Page::Tabs => "Tabbar",
            Page::Editing1 => "Scintillas",
            Page::Editing2 => "Scintillas2",
            Page::DarkMode => "DarkMode",
            Page::Margins => "MarginsBorderEdge",
            Page::Highlight => "Highlighting",
            _ => "",
        };
        crate::localization::with_preference(&filename, section, || {
            let elements = match page {
                Page::General => self.appearance_general(cx),
                Page::Toolbar => self.appearance_toolbar(cx),
                Page::Tabs => self.appearance_tabs(cx),
                Page::Editing1 => self.appearance_editing1(cx),
                Page::Editing2 => self.appearance_editing2(cx),
                Page::DarkMode => self.appearance_dark_mode(cx),
                Page::Margins => self.appearance_margins(cx),
                Page::Highlight => self.appearance_highlight(cx),
                _ => Vec::new(),
            };
            div()
                .relative()
                .w(horizontal_px(805.))
                .h(px(358.75))
                .children(elements)
                .into_any_element()
        })
    }

    fn appearance_general(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut fonts = cx.text_system().all_font_names();
        fonts.push(dialog_font::system_font_family());
        fonts.sort_unstable_by_key(|name| name.to_lowercase());
        fonts.dedup();
        let chinese = self.native_locale().starts_with("chinese");
        vec![
            group("界面语言", 12., 12., 212., 43.),
            self.appearance_choice(
                "NativeLang.filename",
                crate::localization::languages()
                    .iter()
                    .map(|language| (language.filename.into(), language.name.to_owned()))
                    .collect(),
                "english.xml".into(),
                true,
                24.,
                27.,
                188.,
                cx,
            ),
            group(if chinese { "菜单字体" } else { "Menu font" }, 236., 12., 212., 53.),
            label_right(if chinese { "字体：" } else { "Font:" }, 242., 29., 27.),
            self.appearance_choice("MenuFont.fontName", fonts.iter().map(|name| (name.clone().into(), name.clone())).collect(),
                menu_surface::DEFAULT_FONT_NAME.into(), true, 274., 27., 166., cx),
            label_right(if chinese { "字号：" } else { "Size:" }, 242., 48., 27.),
            self.appearance_choice("MenuFont.size", (8..=24).map(|size: i64| (size.into(), size.to_string())).collect(),
                menu_surface::DEFAULT_FONT_SIZE.into(), true, 274., 46., 48., cx),
            group(crate::localization::localized_string(&self.native_locale(), "ConfigDirectoryTitle").unwrap_or("Configuration directory"), 236., 71., 212., 35.),
            source_rect(242. * SCALE, 83. * SCALE, 200. * SCALE, 35.)
                .child(self.render_config_directory_preferences(cx)).into_any_element(),
            group(if chinese { "Matrix 彩蛋" } else { "Matrix" }, 236., 112., 212., 41.),
            self.appearance_check("pref-matrix-lock", if chinese { "Matrix 锁屏保护" } else { "Matrix screen lock" }, "Matrix.lockScreen", false, false, true, 242., 124., 200., cx),
            label_right(if chinese { "样式：" } else { "Style:" }, 242., 140., 36.),
            self.appearance_choice("Matrix.style", vec![("matrix".into(), "Matrix".into()), ("terminal".into(), if chinese { "伪终端" } else { "Simulated terminal" }.into())], "matrix".into(), true, 284., 137., 156., cx),
            group("菜单栏", 12., 64., 212., 50.),
            self.appearance_check(
                "pref-hide-menu",
                "隐藏菜单栏（按下 Alt 或 F10 显示）",
                "MenuBar.visible",
                true,
                true,
                true,
                18.,
                78.,
                200.,
                cx,
            ),
            self.appearance_check(
                "pref-hide-menu-shortcuts",
                "隐藏菜单栏右侧的 ＋ ▼ ✕ 快捷按钮",
                "MISC.hideMenuRightShortcuts",
                false,
                false,
                true,
                18.,
                94.,
                200.,
                cx,
            ),
            group("状态栏", 12., 120., 212., 33.),
            self.appearance_check(
                "pref-hide-status",
                "隐藏状态栏",
                "statusbar_visible",
                true,
                true,
                true,
                18.,
                132.,
                200.,
                cx,
            ),
            group(crate::localization::current_preference_control(7165).unwrap_or("Dialog Font (Need to restart Notepad Matrix)"), 12., 162., 436., 36.),
            label_right(crate::localization::current_preference_control(7166).unwrap_or("Size:"), 18., 176., 27.),
            self.appearance_choice("DialogFont.size", dialog_font::SIZES.into_iter().map(|size| (size.into(), size.to_string())).collect(), 8.into(), true, 51., 174., 35., cx),
            label_right(crate::localization::current_preference_control(7168).unwrap_or("Name:"), 110., 176., 45.),
            self.appearance_choice("DialogFont.fontName", fonts.into_iter().map(|name| (name.clone().into(), name)).collect(), dialog_font::system_font_family().into(), true, 159., 174., 281., cx),
        ]
    }

    fn appearance_toolbar(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let default_icons = if self.dark { "small" } else { "standard" };
        let colorable = self.pref_string("ToolBar.iconSet", default_icons) != "standard";
        let mut elements = vec![
            self.appearance_check(
                "pref-hide-toolbar",
                "隐藏工具栏",
                "ToolBar.visible",
                true,
                true,
                true,
                18.,
                25.,
                112.,
                cx,
            ),
            group("图标着色", 142., 19., 306., 173.),
            self.appearance_radio(
                "pref-toolbar-complete",
                "完全着色",
                "ToolBar.fluentMono",
                false.into(),
                true.into(),
                colorable,
                154.,
                32.,
                282.,
                cx,
            ),
            self.appearance_radio(
                "pref-toolbar-partial",
                "部分着色",
                "ToolBar.fluentMono",
                false.into(),
                false.into(),
                colorable,
                154.,
                45.,
                282.,
                cx,
            ),
            group("颜色选择", 154., 67., 282., 115.),
        ];
        for (id, text, value, y) in [
            ("pref-toolbar-small", "Fluent UI 图标：小图标", "small", 67.),
            ("pref-toolbar-large", "Fluent UI 图标：大图标", "large", 80.),
            (
                "pref-toolbar-small2",
                "Fluent UI 填充图标：小图标",
                "small2",
                93.,
            ),
            (
                "pref-toolbar-large2",
                "Fluent UI 填充图标：大图标",
                "large2",
                106.,
            ),
            (
                "pref-toolbar-standard",
                "标准图标：小图标",
                "standard",
                119.,
            ),
        ] {
            if value != "standard" {
                elements.push(self.appearance_toolbar_radio(id, text, default_icons, value, y, cx));
                continue;
            }
            elements.push(self.appearance_radio(
                id,
                text,
                "ToolBar.iconSet",
                default_icons.into(),
                value.into(),
                true,
                18.,
                y,
                112.,
                cx,
            ));
        }
        for (id, text, value, x, y, w) in [
            ("pref-toolbar-red", "红色", 1, 162., 80., 100.),
            ("pref-toolbar-green", "绿色", 2, 162., 94., 100.),
            ("pref-toolbar-blue", "蓝色", 3, 162., 108., 100.),
            ("pref-toolbar-purple", "紫色", 4, 162., 122., 100.),
            ("pref-toolbar-cyan", "青色", 5, 162., 136., 100.),
            ("pref-toolbar-olive", "橄榄色", 6, 162., 150., 100.),
            ("pref-toolbar-yellow", "黄色", 7, 162., 164., 100.),
            ("pref-toolbar-default", "默认颜色", 0, 280., 80., 128.),
            ("pref-toolbar-accent", "系统强调色", 8, 280., 94., 128.),
            ("pref-toolbar-custom", "自定义颜色", 9, 280., 108., 128.),
        ] {
            elements.push(self.appearance_radio(
                id,
                text,
                "ToolBar.fluentColor",
                0.into(),
                value.into(),
                colorable,
                x,
                y,
                w,
                cx,
            ));
        }
        elements.push(self.pref_color(
            "ToolBar.fluentCustomColor",
            0,
            colorable && self.pref_int("ToolBar.fluentColor", 0) == 9,
            286. * SCALE,
            121. * SCALE,
            cx,
        ));
        elements.push(self.appearance_help(
            "pref-toolbar-accent-help",
            "toolbar-accent-tip",
            "This option makes your toolbar icons follow the system accent color.",
            colorable,
            416.,
            94.,
            10.,
        ));
        elements
    }

    fn appearance_tabs(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let visible = !self.pref_bool("TabBar.hide", false);
        let pin = self.pref_bool("TabBar.pinButton", true);
        let close = self.pref_bool("TabBar.closeButton", true);
        let dark = self.dark;
        let alternate = self.pref_int(
            if dark {
                "DarkMode.darkTabIconSet"
            } else {
                "DarkMode.lightTabIconSet"
            },
            if dark { 2 } else { 0 },
        ) == 1;
        let mut elements = vec![
            self.appearance_check(
                "pref-hide-tabs",
                "隐藏标签栏",
                "TabBar.hide",
                false,
                false,
                true,
                18.,
                21.,
                200.,
                cx,
            ),
            group("行为", 12., 38., 212., 130.),
            group("外观和体验", 236., 38., 212., 130.),
            label_right("标签最大长度：", 18., 147., 146.),
            self.pref_input(
                "tabCompactLabelLen",
                168. * SCALE,
                146. * SCALE,
                32. * SCALE,
                true,
                cx,
            ),
            self.appearance_help(
                "pref-tabs-label-help",
                "tabbar-tabcompactlabellen-tip",
                "Limits the visible length of long tab names. Enter to apply the given value. Value range: 1-257 characters (0 disables the truncation).",
                true,
                206.,
                147.,
                18.,
            ),
        ];
        for (id, text, path, default, invert, y) in [
            (
                "pref-tabs-vertical",
                "垂直显示",
                "tab_vertical",
                false,
                false,
                49.,
            ),
            (
                "pref-tabs-multiline",
                "多行显示",
                "tab_multiline",
                false,
                false,
                62.,
            ),
            (
                "pref-tabs-lock",
                "锁定（不允许拖放）",
                "TabBar.dragAndDrop",
                true,
                true,
                91.,
            ),
            (
                "pref-tabs-double-close",
                "双击关闭标签",
                "TabBar.doubleClick2Close",
                false,
                false,
                104.,
            ),
            (
                "pref-tabs-quit",
                "关闭最后一个标签时退出",
                "TabBar.quitOnEmpty",
                false,
                false,
                117.,
            ),
        ] {
            elements.push(
                self.appearance_check(id, text, path, default, invert, visible, 18., y, 200., cx),
            );
        }
        for (id, text, path, default, enabled, y) in [
            (
                "pref-tabs-reduce",
                "缩小标签栏",
                "TabBar.reduce",
                true,
                visible,
                49.,
            ),
            (
                "pref-tabs-alt-icons",
                "使用替代图标",
                "TabBar.alternateIcons",
                alternate,
                visible,
                62.,
            ),
            (
                "pref-tabs-inactive-color",
                "非当前标签变暗",
                "TabBar.drawInactiveTab",
                true,
                visible,
                76.,
            ),
            (
                "pref-tabs-top-bar",
                "为当前标签绘制颜色条",
                "TabBar.drawTopBar",
                true,
                visible,
                89.,
            ),
            (
                "pref-tabs-close",
                "标签显示关闭按钮",
                "TabBar.closeButton",
                true,
                visible,
                113.,
            ),
            (
                "pref-tabs-pin",
                "启用固定标签功能",
                "TabBar.pinButton",
                true,
                visible,
                126.,
            ),
            (
                "pref-tabs-only-pinned",
                "仅显示固定按钮",
                "TabBar.showOnlyPinnedButton",
                false,
                visible && pin,
                139.,
            ),
            (
                "pref-tabs-inactive-buttons",
                "在非当前标签上显示按钮",
                "TabBar.buttonsOninactiveTabs",
                false,
                visible && (pin || close),
                152.,
            ),
        ] {
            elements.push(
                self.appearance_check(id, text, path, default, false, enabled, 242., y, 200., cx),
            );
        }
        elements
    }

    fn appearance_editing1(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let frame = self.pref_int("ScintillaPrimaryView.currentLineIndicator", 1) == 2;
        let blink = self.pref_int("Caret.blinkRate", 500);
        let mut elements = vec![
            group("当前行指示器", 11., 3., 150., 74.),
            source_rect(30. * SCALE, 60. * SCALE, 37. * SCALE, 8. * SCALE)
                .text_right().text_size(px(12.)).line_height(px(14.))
                .text_color(rgb(self.preferences_render_palette().control_text(frame)))
                .child(crate::localization::current_preference_control(6655).unwrap_or("宽度："))
                .into_any_element(),
            self.appearance_slider(
                "ScintillaPrimaryView.currentLineFrameWidth",
                1,
                6,
                1,
                68.,
                59.,
                57.,
                frame,
                cx,
            ),
            source_rect(127. * SCALE, 60. * SCALE, 12. * SCALE, 8. * SCALE)
                .text_size(px(12.)).line_height(px(14.))
                .text_color(rgb(self.preferences_render_palette().control_text(frame)))
                .child(self
                    .pref_int("ScintillaPrimaryView.currentLineFrameWidth", 1)
                    .to_string())
                .into_any_element(),
            group("光标设置", 11., 83., 150., 47.),
            label_right("宽度：", 30., 98., 37.),
            self.appearance_choice(
                "Caret.width",
                [
                    (0, "0"),
                    (1, "1"),
                    (2, "2"),
                    (3, "3"),
                    (4, "Block"),
                    (5, "Block After"),
                ]
                .into_iter()
                .map(|(value, text)| (value.into(), text.into()))
                .collect(),
                2.into(),
                true,
                73.,
                97.,
                80.,
                cx,
            ),
            label_right("闪烁频率：", 14., 114., 53.),
            label_right("快", 68., 114., 12.),
            self.appearance_slider(
                "Caret.blinkSlider",
                50,
                2500,
                if blink == 0 { 2500 } else { blink },
                81.,
                113.,
                57.,
                true,
                cx,
            ),
            label("慢", 140., 114., 12.),
            group("折行", 11., 136., 150., 59.),
        ];
        for (id, text, value, y) in [
            ("pref-current-line-none", "无", 0, 16.),
            ("pref-current-line-background", "背景高亮", 1, 31.),
            ("pref-current-line-frame", "边框高亮", 2, 46.),
        ] {
            elements.push(self.appearance_radio(
                id,
                text,
                "ScintillaPrimaryView.currentLineIndicator",
                1.into(),
                value.into(),
                true,
                17.,
                y,
                100.,
                cx,
            ));
        }
        for (id, text, value, y) in [
            ("pref-wrap-default", "默认", "default", 149.),
            ("pref-wrap-aligned", "对齐", "aligned", 164.),
            ("pref-wrap-indent", "缩排", "indent", 179.),
        ] {
            elements.push(self.appearance_radio(
                id,
                text,
                "ScintillaPrimaryView.lineWrapMethod",
                "aligned".into(),
                value.into(),
                true,
                17.,
                y,
                68.,
                cx,
            ));
        }
        for (id, text, path, default, y) in [
            (
                "pref-virtual-space",
                "启用行尾虚拟空格",
                "ScintillaPrimaryView.virtualSpace",
                false,
                28.,
            ),
            (
                "pref-fold-toggle",
                "折叠/展开当前层次命令可以切换",
                "MISC.enableFoldCmdToggable",
                false,
                43.,
            ),
            (
                "pref-right-selection",
                "选区外右键点击时保留选区",
                "ScintillaPrimaryView.rightClickKeepsSelection",
                false,
                58.,
            ),
            (
                "pref-line-copy-cut",
                "启用在未选择内容时复制/剪切整行",
                "lineCopyCutWithoutSelection",
                true,
                73.,
            ),
            (
                "pref-selection-color",
                "将自定义颜色应用于所选文本的前景色",
                "ScintillaPrimaryView.selectedTextForegroundSingleColor",
                false,
                88.,
            ),
            (
                "pref-scroll-last",
                "允许滚动到末尾行之后",
                "ScintillaPrimaryView.scrollBeyondLastLine",
                true,
                103.,
            ),
            (
                "pref-disable-text-drag",
                "禁用选定文本拖放",
                "ScintillaPrimaryView.disableSelectedTextDragDrop",
                false,
                133.,
            ),
        ] {
            elements.push(
                self.appearance_check(id, text, path, default, false, true, 173., y, 270., cx),
            );
        }
        elements
    }

    fn appearance_editing2(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let multi = self.pref_bool("ScintillaPrimaryView.multiSelection", true);
        let eol = self.pref_int("ScintillaPrimaryView.eolMode", 1);
        let npc_appearance = if self.pref_int("ScintillaPrimaryView.npcMode", 1) == 1 {
            1
        } else {
            2
        };
        vec![
            group("多重编辑", 12., 3., 260., 59.),
            self.appearance_check(
                "pref-multiple-selection",
                "启用多重编辑（Ctrl+鼠标点选）",
                "ScintillaPrimaryView.multiSelection",
                true,
                false,
                true,
                18.,
                16.,
                248.,
                cx,
            ),
            self.appearance_check(
                "pref-column-multiple",
                "为多重编辑启用按列选取",
                "ScintillaPrimaryView.columnSel2MultiEdit",
                true,
                false,
                multi,
                30.,
                31.,
                236.,
                cx,
            ),
            group("换行符（CRLF）", 284., 3., 164., 59.),
            self.appearance_radio(
                "pref-eol-rounded",
                "默认",
                "ScintillaPrimaryView.eolMode",
                eol.into(),
                ((eol & 2) | 1).into(),
                true,
                290.,
                16.,
                150.,
                cx,
            ),
            self.appearance_radio(
                "pref-eol-plain",
                "纯文本",
                "ScintillaPrimaryView.eolMode",
                eol.into(),
                (eol & 2).into(),
                true,
                290.,
                31.,
                150.,
                cx,
            ),
            self.appearance_check(
                "pref-eol-color",
                "自定义颜色",
                "ScintillaPrimaryView.eolCustomColor",
                eol & 2 != 0,
                false,
                true,
                290.,
                47.,
                128.,
                cx,
            ),
            self.appearance_button(
                "pref-eol-style",
                "...",
                true,
                424.,
                44.,
                16.,
                |view, window, cx| view.show_style_configurator_target("EOL custom color", window, cx),
                cx,
            ),
            group("非打印字符", 12., 68., 436., 127.),
            group("", 22., 78., 416., 89.),
            group("外观", 32., 88., 202., 59.),
            self.appearance_radio(
                "pref-npc-abbreviation",
                "缩写",
                "ScintillaPrimaryView.npcAppearance",
                npc_appearance.into(),
                1.into(),
                true,
                38.,
                101.,
                66.,
                cx,
            ),
            self.appearance_radio(
                "pref-npc-codepoint",
                "字符码位",
                "ScintillaPrimaryView.npcAppearance",
                npc_appearance.into(),
                2.into(),
                true,
                38.,
                116.,
                66.,
                cx,
            ),
            self.appearance_button(
                "pref-npc-help",
                "?",
                true,
                110.,
                98.,
                16.,
                |_, _, cx| cx.open_url("https://npp-user-manual.org/docs/views/#show-symbol"),
                cx,
            ),
            self.appearance_check(
                "pref-npc-color",
                "自定义颜色",
                "ScintillaPrimaryView.npcCustomColor",
                false,
                false,
                true,
                142.,
                101.,
                58.,
                cx,
            ),
            self.appearance_button(
                "pref-npc-style",
                "...",
                true,
                206.,
                98.,
                16.,
                |view, window, cx| view.show_style_configurator_target("Non-printing characters custom color", window, cx),
                cx,
            ),
            self.appearance_check(
                "pref-npc-control-eol",
                "将外观设置应用到 C0、C1 以及 Unicode 行尾符",
                "ScintillaPrimaryView.npcIncludeCcUniEOL",
                false,
                false,
                true,
                28.,
                153.,
                404.,
                cx,
            ),
            self.appearance_check(
                "pref-npc-block-control",
                "阻止控制字符（C0 代码）输入到文档中",
                "ScintillaPrimaryView.npcNoInputC0",
                true,
                false,
                true,
                18.,
                177.,
                424.,
                cx,
            ),
        ]
    }

    fn appearance_dark_mode(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mode_default = if self.pref_string("theme", "light") == "dark" {
            1
        } else {
            0
        };
        let mode = self.pref_int("darkMode.mode", mode_default);
        let enabled = mode == 1 || (mode == 2 && self.dark);
        let custom = enabled && self.pref_int("DarkMode.colorTone", 0) == 32;
        let mut elements = vec![group("色调", 12., 38., 436., 160.)];
        for (id, text, value, x, w) in [
            ("pref-light-mode", "浅色模式", 0, 18., 136.),
            ("pref-dark-mode", "深色模式", 1, 160., 136.),
            ("pref-system-mode", "跟随系统", 2, 302., 140.),
        ] {
            elements.push(self.appearance_radio(
                id,
                text,
                "darkMode.mode",
                mode_default.into(),
                value.into(),
                true,
                x,
                12.,
                w,
                cx,
            ));
        }
        for (id, text, value, x, y, w) in [
            ("pref-dark-black", "黑色", 0, 18., 54., 90.),
            ("pref-dark-red", "红色", 1, 18., 69., 90.),
            ("pref-dark-green", "绿色", 2, 18., 84., 90.),
            ("pref-dark-blue", "蓝色", 3, 18., 99., 90.),
            ("pref-dark-purple", "紫色", 4, 18., 114., 90.),
            ("pref-dark-cyan", "青色", 5, 18., 129., 90.),
            ("pref-dark-olive", "橄榄色", 6, 18., 144., 90.),
            ("pref-dark-custom", "自定义颜色", 32, 116., 54., 210.),
        ] {
            elements.push(self.appearance_radio(
                id,
                text,
                "DarkMode.colorTone",
                0.into(),
                value.into(),
                enabled,
                x,
                y,
                w,
                cx,
            ));
        }
        for (path, text, default, x, y) in DARK_COLORS {
            elements.push(self.pref_color(
                path,
                default,
                custom,
                (x - 17.) * SCALE,
                (y - 3.) * SCALE,
                cx,
            ));
            elements.push(pref_label_colour(text, x * SCALE, y * SCALE,
                (if x == 354. { 86. } else { 90. }) * SCALE,
                self.preferences_render_palette().control_text(custom)));
        }
        elements.push(self.appearance_button(
            "pref-dark-reset",
            "重置",
            custom,
            304.,
            177.,
            56.,
            |view, window, cx| {
                view.set_preference("DarkMode.resetTone", 0.into(), window, cx);
            },
            cx,
        ));
        elements.push(
            self.appearance_choice(
                "DarkMode.resetTone",
                [
                    (0, "黑色"),
                    (1, "红色"),
                    (2, "绿色"),
                    (3, "蓝色"),
                    (4, "紫色"),
                    (5, "青色"),
                    (6, "橄榄色"),
                ]
                .into_iter()
                .map(|(value, text)| (value.into(), text.into()))
                .collect(),
                Value::Null,
                custom,
                366.,
                177.,
                74.,
                cx,
            ),
        );
        elements
    }

    fn appearance_margins(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let number = self.pref_bool("ScintillaPrimaryView.lineNumberMargin", true);
        let columns = self.pref_string("ScintillaPrimaryView.edgeMultiColumnPos", "");
        let can_be_background = edge_columns(&columns).is_ok_and(|columns| columns.len() == 1);
        let history = self.pref_int("ScintillaPrimaryView.isChangeHistoryEnabled", 1);
        let mut elements = vec![
            group("代码折叠标志", 12., 21., 106., 89.),
            group("边框宽度", 12., 116., 106., 45.),
            self.appearance_slider(
                "ScintillaPrimaryView.borderWidth",
                0,
                30,
                2,
                18.,
                129.,
                76.,
                true,
                cx,
            ),
            value_label(
                &self
                    .pref_int("ScintillaPrimaryView.borderWidth", 2)
                    .to_string(),
                98.,
                130.,
                12.,
            ),
            self.appearance_check(
                "pref-no-border",
                "无边框",
                "ScintillaPrimaryView.borderEdge",
                true,
                true,
                true,
                18.,
                146.,
                94.,
                cx,
            ),
            group("列边线设置", 130., 21., 142., 89.),
            self.appearance_help(
                "pref-edge-help",
                "verticalEdge-tip",
                "Add your column marker by indicating its position with a decimal number. You can define several column markers by using white space to separate the different numbers.",
                true,
                250.,
                33.,
                10.,
            ),
            self.pref_column_input(
                140. * SCALE,
                51. * SCALE,
                122. * SCALE,
                36. * SCALE,
            ),
            self.appearance_check(
                "pref-edge-background",
                "背景色模式",
                "ScintillaPrimaryView.isEdgeBgMode",
                false,
                false,
                can_be_background,
                136.,
                94.,
                130.,
                cx,
            ),
            group("变更历史", 130., 116., 142., 45.),
            self.appearance_check(
                "pref-history-margin",
                "在侧边显示",
                "ScintillaPrimaryView.changeHistoryMargin",
                history & 1 != 0,
                false,
                true,
                136.,
                129.,
                130.,
                cx,
            ),
            self.appearance_check(
                "pref-history-text",
                "在正文显示",
                "ScintillaPrimaryView.changeHistoryIndicator",
                history & 2 != 0,
                false,
                true,
                136.,
                144.,
                130.,
                cx,
            ),
            group("行号", 284., 21., 164., 59.),
            self.appearance_check(
                "pref-line-numbers",
                "显示行号栏",
                "ScintillaPrimaryView.lineNumberMargin",
                true,
                false,
                true,
                290.,
                34.,
                152.,
                cx,
            ),
            self.appearance_radio(
                "pref-line-number-dynamic",
                "变宽",
                "ScintillaPrimaryView.lineNumberDynamicWidth",
                true.into(),
                true.into(),
                number,
                302.,
                49.,
                140.,
                cx,
            ),
            self.appearance_radio(
                "pref-line-number-constant",
                "等宽",
                "ScintillaPrimaryView.lineNumberDynamicWidth",
                true.into(),
                false.into(),
                number,
                302.,
                64.,
                140.,
                cx,
            ),
            group("边距", 284., 86., 164., 63.),
            label_right("左侧", 290., 100., 50.),
            label_right("右侧", 290., 116., 50.),
            label_right("沉浸模式", 290., 132., 80.),
            self.appearance_check(
                "pref-bookmark-margin",
                "显示书签栏",
                "ScintillaPrimaryView.bookMarkMargin",
                true,
                false,
                true,
                290.,
                155.,
                152.,
                cx,
            ),
        ];
        for (id, text, value, y) in [
            ("pref-fold-simple", "简洁", "simple", 34.),
            ("pref-fold-arrow", "箭头", "arrow", 49.),
            ("pref-fold-circle", "圆圈", "circle", 64.),
            ("pref-fold-box", "方框", "box", 79.),
            ("pref-fold-none", "无", "none", 94.),
        ] {
            elements.push(self.appearance_radio(
                id,
                text,
                "ScintillaPrimaryView.folderMarkStyle",
                "box".into(),
                value.into(),
                true,
                18.,
                y,
                94.,
                cx,
            ));
        }
        for (path, min, max, default, x, y, w) in [
            ("ScintillaPrimaryView.paddingLeft", 0, 30, 0, 344., 99., 82.),
            (
                "ScintillaPrimaryView.paddingRight",
                0,
                30,
                0,
                344.,
                115.,
                82.,
            ),
            (
                "ScintillaPrimaryView.distractionFreeDivPart",
                3,
                9,
                4,
                374.,
                131.,
                52.,
            ),
        ] {
            elements.push(self.appearance_slider(path, min, max, default, x, y, w, true, cx));
            elements.push(value_label(
                &self.pref_int(path, default).to_string(),
                430.,
                y + 1.,
                12.,
            ));
        }
        elements
    }

    fn appearance_highlight(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let smart = self.pref_bool("SmartHighLight.enabled", true);
        let tags = self.pref_bool("TagsMatchHighLight.enabled", true);
        let mut elements = vec![
            group("用格式标记全部", 12., 29., 212., 45.),
            group("高亮显示匹配标签", 12., 80., 212., 59.),
            group("智能高亮", 236., 29., 212., 110.),
            group("匹配规则", 242., 72., 200., 59.),
        ];
        for (id, text, path, default, enabled, x, y) in [
            (
                "pref-mark-case",
                "匹配大小写",
                "MarkAll.matchCase",
                false,
                true,
                18.,
                42.,
            ),
            (
                "pref-mark-word",
                "仅全词匹配",
                "MarkAll.wholeWordOnly",
                true,
                true,
                18.,
                57.,
            ),
            (
                "pref-tags-enabled",
                "启用",
                "TagsMatchHighLight.enabled",
                true,
                true,
                18.,
                93.,
            ),
            (
                "pref-tags-attributes",
                "高亮显示标签属性",
                "TagsMatchHighLight.TagAttrHighLight",
                true,
                tags,
                18.,
                108.,
            ),
            (
                "pref-tags-non-html",
                "高亮显示 注释/PHP/ASP 区域",
                "TagsMatchHighLight.HighLightNonHtmlZone",
                false,
                tags,
                18.,
                123.,
            ),
            (
                "pref-smart-enabled",
                "启用",
                "SmartHighLight.enabled",
                true,
                true,
                242.,
                42.,
            ),
            (
                "pref-smart-other-view",
                "高亮延伸至另一窗口",
                "SmartHighLight.onAnotherView",
                false,
                smart,
                242.,
                57.,
            ),
            (
                "pref-smart-case",
                "匹配大小写",
                "SmartHighLight.matchCase",
                false,
                smart,
                248.,
                85.,
            ),
            (
                "pref-smart-word",
                "仅全词匹配",
                "SmartHighLight.wholeWordOnly",
                true,
                smart,
                248.,
                100.,
            ),
            (
                "pref-smart-find-settings",
                "使用查找对话框设置",
                "SmartHighLight.useFindSettings",
                false,
                smart,
                248.,
                115.,
            ),
        ] {
            elements.push(
                self.appearance_check(id, text, path, default, false, enabled, x, y, if x == 248. { 188. } else { 200. }, cx),
            );
        }
        elements
    }
}
