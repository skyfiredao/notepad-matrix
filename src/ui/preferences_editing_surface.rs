use super::*;
use super::dialog_font::px;
use super::preferences_surface::{Commit, FieldSpec, Page, pref_group, pref_label, pref_label_right, pref_label_center, pref_indicator, horizontal_px};
use serde_json::{Value, json};
use crate::editor::EditError;

const SCALE: f32 = 1.75;
const PRINT_PARTS: [&str; 6] = [
    "PrintSettings.headerLeft", "PrintSettings.headerMiddle", "PrintSettings.headerRight",
    "PrintSettings.footerLeft", "PrintSettings.footerMiddle", "PrintSettings.footerRight",
];
const PAIR_PARTS: [[&str; 2]; 3] = [
    ["auto-insert.UserDefinePair.0.open", "auto-insert.UserDefinePair.0.close"],
    ["auto-insert.UserDefinePair.1.open", "auto-insert.UserDefinePair.1.close"],
    ["auto-insert.UserDefinePair.2.open", "auto-insert.UserDefinePair.2.close"],
];
const PRINT_VARIABLES: [(&str, &str); 7] = [
    ("$(FULL_CURRENT_PATH)", "完整文件路径"),
    ("$(FILE_NAME)", "文件名"),
    ("$(CURRENT_DIRECTORY)", "文件目录"),
    ("$(CURRENT_PRINTING_PAGE)", "页码"),
    ("$(SHORT_DATE)", "短日期格式"),
    ("$(LONG_DATE)", "长日期格式"),
    ("$(TIME)", "时间"),
];

#[derive(Default)]
pub(super) struct EditingState {
    pub(super) language_available: Option<u16>,
    pub(super) language_disabled: Option<u16>,
    language_moved: Vec<u16>,
    new_document_codepage: Option<i64>,
    pub(super) indentation_language: Option<String>,
    pub(super) print_focus: Option<String>,
    pub(super) print_variable: usize,
    pub(super) find_monospaced: Option<bool>,
    print_variable_open: bool,
    list_scroll: [gpui::ScrollHandle; 3],
    list_scroll_grab: Option<(usize, f32)>,
}

pub(super) type State = EditingState;

impl EditingState {
    pub(super) fn new() -> Self { Self::default() }
}

pub(super) fn scrollbar_accessibility<E: gpui::StatefulInteractiveElement>(mut element: E, scroll: gpui::ScrollHandle) -> E {
    element = element.a11y_synthetic_children({
        let scroll = scroll.clone();
        move |builder| {
            let maximum = f64::from(f32::from(scroll.max_offset().y).max(0.));
            let node = builder.parent_node();
            node.set_min_numeric_value(0.);
            node.set_max_numeric_value(maximum);
            node.set_numeric_value(f64::from(-f32::from(scroll.offset().y)).clamp(0., maximum));
            node.set_numeric_value_step(14. * f64::from(dialog_font::scale()));
        }
    });
    for action in [gpui::accesskit::Action::Increment, gpui::accesskit::Action::Decrement, gpui::accesskit::Action::SetValue] {
        let scroll = scroll.clone();
        element = element.on_a11y_action(action, move |data, window, _| {
            let maximum = f64::from(f32::from(scroll.max_offset().y).max(0.));
            let current = f64::from(-f32::from(scroll.offset().y));
            let step = 14. * f64::from(dialog_font::scale());
            let value = match action {
                gpui::accesskit::Action::Increment => current + step,
                gpui::accesskit::Action::Decrement => current - step,
                gpui::accesskit::Action::SetValue => match data {
                    Some(gpui::accesskit::ActionData::NumericValue(value)) => *value,
                    Some(gpui::accesskit::ActionData::Value(value)) => {
                        let Ok(value) = value.parse::<f64>() else { return; };
                        value
                    }
                    _ => return,
                },
                _ => return,
            };
            if !value.is_finite() { return; }
            scroll.set_offset(gpui::point(scroll.offset().x, gpui::px(-value.clamp(0., maximum) as f32)));
            window.refresh();
        });
    }
    element
}

fn label(text: &str, x: f32, y: f32, w: f32) -> AnyElement {
    pref_label(text, x * SCALE, y * SCALE, w * SCALE)
}

fn label_right(text: &str, x: f32, y: f32, w: f32) -> AnyElement {
    pref_label_right(text, x * SCALE, y * SCALE, w * SCALE)
}

fn label_center(text: &str, x: f32, y: f32, w: f32) -> AnyElement {
    pref_label_center(text, x * SCALE, y * SCALE, w * SCALE)
}

fn group(text: &str, x: f32, y: f32, w: f32, h: f32) -> AnyElement {
    pref_group(text, x * SCALE, y * SCALE, w * SCALE, h * SCALE)
}

fn tab_paths(language: Option<&str>) -> [&'static str; 3] {
    static PATHS: std::sync::OnceLock<std::collections::BTreeMap<String, [&'static str; 3]>> = std::sync::OnceLock::new();
    let paths = PATHS.get_or_init(|| {
        let mut paths = std::collections::BTreeMap::new();
        if let Ok(catalog) = crate::languages::catalog() {
            for language in catalog.profiles().iter().filter(|language| !language.internal) {
                let entries = ["size", "replaceBySpace", "backspaceUnindent"].map(|field| {
                    let path = format!("TabSettings.languages.{}.{field}", language.name);
                    &*Box::leak(path.into_boxed_str())
                });
                paths.insert(language.name.clone(), entries);
            }
        }
        paths
    });
    language.and_then(|name| paths.get(name)).copied()
        .unwrap_or(["TabSettings.size", "TabSettings.replaceBySpace", "TabSettings.backspaceUnindent"])
}

pub(super) fn fields() -> Vec<FieldSpec> {
    let mut fields = vec![
        FieldSpec::text("wordCharList.charsAdded", "", 258),
        FieldSpec::text("delimiterSelection.leftmostDelimiter", "(", 1),
        FieldSpec::text("delimiterSelection.rightmostDelimiter", ")", 1),
        FieldSpec::text("searchEngine.searchEngineCustom", "", 259),
        FieldSpec::integer("Searching.inSelectionAutocheckThreshold", 1024, 0, 1024, Commit::Immediate).unsigned_32(),
        FieldSpec::integer("Searching.fillFindWhatThreshold", 1024, 1, 16383, Commit::Immediate).unsigned_32(),
        FieldSpec::integer("TabSettings.size", 4, 1, i64::from(u32::MAX), Commit::Immediate).unsigned_32(),
    ];
    for path in PRINT_PARTS {
        fields.push(FieldSpec::text(path, "", 255));
    }
    for path in ["PrintSettings.margeLeft", "PrintSettings.margeTop", "PrintSettings.margeRight", "PrintSettings.margeBottom"] {
        fields.push(FieldSpec::integer(path, 0, 0, i64::from(u32::MAX), Commit::Immediate).unsigned_32().retain_text());
    }
    for path in [
        "auto-insert.UserDefinePair.0.open", "auto-insert.UserDefinePair.0.close",
        "auto-insert.UserDefinePair.1.open", "auto-insert.UserDefinePair.1.close",
        "auto-insert.UserDefinePair.2.open", "auto-insert.UserDefinePair.2.close",
    ] {
        fields.push(FieldSpec::text(path, "", 1));
    }
    if let Ok(catalog) = crate::languages::catalog() {
        for language in catalog.profiles().iter().filter(|language| !language.internal) {
            fields.push(FieldSpec::integer(
                tab_paths(Some(&language.name))[0],
                4, 1, i64::from(u32::MAX), Commit::Immediate,
            ).unsigned_32());
        }
    }
    fields
}

fn at<'a>(settings: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(settings, |value, part| {
        if let Some(array) = value.as_array() {
            part.parse::<usize>().ok().and_then(|index| array.get(index))
        } else {
            value.get(part)
        }
    })
}

fn put(settings: &mut Value, path: &str, value: Value) -> io::Result<()> {
    fn descend(current: &mut Value, parts: &[&str], value: Value) -> io::Result<()> {
        let Some((part, rest)) = parts.split_first() else {
            *current = value;
            return Ok(());
        };
        if let Ok(index) = part.parse::<usize>() {
            if index > 2 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "matched pair index exceeds three rows"));
            }
            if current.is_null() { *current = json!([]); }
            let array = current.as_array_mut().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "preference requires an array"))?;
            while array.len() <= index { array.push(Value::Null); }
            descend(&mut array[index], rest, value)
        } else {
            if current.is_null() { *current = json!({}); }
            let object = current.as_object_mut().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "preference requires an object"))?;
            descend(object.entry((*part).to_owned()).or_insert(Value::Null), rest, value)
        }
    }
    descend(settings, &path.split('.').collect::<Vec<_>>(), value)
}

fn boolean(settings: &Value, path: &str, default: bool) -> bool {
    at(settings, path).and_then(|value| value.as_bool().or_else(|| match value.as_str() {
        Some("yes") => Some(true), Some("no") => Some(false), _ => None,
    })).unwrap_or(default)
}

pub(super) fn field_display(path: &str, value: &Value) -> Option<String> {
    if path.starts_with("auto-insert.UserDefinePair.")
        || matches!(path, "delimiterSelection.leftmostDelimiter" | "delimiterSelection.rightmostDelimiter")
    {
        if let Some(text) = value.as_str() { return Some(text.to_owned()); }
        return Some(value.as_u64().and_then(|number| u32::try_from(number).ok())
            .and_then(char::from_u32).filter(|ch| *ch != '\0')
            .map(|ch| ch.to_string()).unwrap_or_default());
    }
    None
}

pub(super) fn choice_value(path: &str, settings: &Value) -> Option<Value> {
    if path == "NewDocDefaultSettings.lang" && settings["NewDocDefaultSettings"]["lang"].as_u64() == Some(15) {
        return settings["NewDocDefaultSettings"]["udl"].as_str().map(|name| Value::String(name.to_owned()));
    }
    None
}

pub(super) fn empty_field_value(path: &str) -> Option<Value> {
    match path {
        "Searching.inSelectionAutocheckThreshold" | "Searching.fillFindWhatThreshold" => Some(1024.into()),
        "PrintSettings.margeLeft" | "PrintSettings.margeTop" | "PrintSettings.margeRight" | "PrintSettings.margeBottom" => Some(0.into()),
        _ => None,
    }
}

pub(super) fn reject_integer_edit(path: &str, value: i64) -> bool {
    path.starts_with("TabSettings.") && path.ends_with(".size") && value < 1
}

pub(super) fn editing_changed(path: &str, settings: &mut Value) -> io::Result<()> {
    match path {
        "auto-completion.autoCAction" => {
            let action = at(settings, path).and_then(Value::as_i64).unwrap_or(3);
            if action == 0 || action == 1 {
                put(settings, "auto-completion.autoCBrief", false.into())?;
                put(settings, "auto-completion.autoCIgnoreNumbers", false.into())?;
            }
        }
        "Searching.fillFindFieldWithSelected" => {
            if !boolean(settings, path, true) {
                put(settings, "Searching.fillFindFieldSelectCaret", false.into())?;
            }
        }
        "NewDocDefaultSettings.encoding" => {
            put(settings, "NewDocDefaultSettings.codepage", (-1).into())?;
        }
        "NewDocDefaultSettings.lang" => {
            if let Some(name) = settings["NewDocDefaultSettings"]["lang"].as_str().map(str::to_owned) {
                put(settings, "NewDocDefaultSettings.lang", 15.into())?;
                put(settings, "NewDocDefaultSettings.udl", name.into())?;
            } else {
                put(settings, "NewDocDefaultSettings.udl", "".into())?;
            }
        }
        _ => {}
    }
    if path.starts_with("auto-insert.UserDefinePair.") {
        if let Some(text) = at(settings, path).and_then(Value::as_str) {
            let character = text.chars().next().filter(|ch| ch.is_ascii() && *ch != '\0');
            let value = character.map(|ch| Value::from(u32::from(ch))).unwrap_or(Value::Null);
            put(settings, path, value)?;
        }
    }
    if matches!(path, "delimiterSelection.leftmostDelimiter" | "delimiterSelection.rightmostDelimiter") {
        if let Some(text) = at(settings, path).and_then(Value::as_str) {
            let value = text.encode_utf16().next().map_or(0, |unit| u32::from(unit & 0xff));
            put(settings, path, value.into())?;
        }
    }
    if PRINT_PARTS.contains(&path) {
        if let Some(text) = at(settings, path).and_then(Value::as_str).map(|text| text.trim().to_owned()) {
            put(settings, path, text.into())?;
        }
    }
    Ok(())
}

pub(super) fn language_excluded(settings: &Value, id: u16) -> bool {
    let group = format!("gr{}", id / 8);
    settings["langsExcluded"][group].as_u64().unwrap_or(0) & (1u64 << (id % 8)) != 0
}

pub(super) fn effective_tabs(settings: &Value, language: &str) -> (i64, bool, bool) {
    let defaults = (
        settings["TabSettings"]["size"].as_i64().filter(|size| *size > 0).unwrap_or(4),
        boolean(settings, "TabSettings.replaceBySpace", false),
        boolean(settings, "TabSettings.backspaceUnindent", false),
    );
    let path = format!("TabSettings.languages.{language}");
    let Some(entry) = at(settings, &path) else { return defaults; };
    let size = entry["size"].as_i64().unwrap_or(0);
    if size <= 0 { return defaults; }
    (size, entry["replaceBySpace"].as_bool().unwrap_or(false), entry["backspaceUnindent"].as_bool().unwrap_or(false))
}

fn codepages() -> Vec<(Value, String)> {
    let mut values = Vec::new();
    for code in 1250..=1258 { values.push((code.into(), format!("Windows-{code}"))); }
    for (code, name) in [
        (28591, "ISO-8859-1"), (28592, "ISO-8859-2"), (28593, "ISO-8859-3"),
        (28594, "ISO-8859-4"), (28595, "ISO-8859-5"), (28596, "ISO-8859-6"),
        (28597, "ISO-8859-7"), (28598, "ISO-8859-8"), (28599, "ISO-8859-9"),
        (28603, "ISO-8859-13"), (28604, "ISO-8859-14"), (28605, "ISO-8859-15"),
        (437, "OEM 437"), (720, "OEM 720"), (737, "OEM 737"), (775, "OEM 775"),
        (850, "OEM 850"), (852, "OEM 852"), (855, "OEM 855"), (857, "OEM 857"),
        (858, "OEM 858"), (860, "OEM 860"), (861, "OEM 861"), (862, "OEM 862"),
        (863, "OEM 863"), (865, "OEM 865"), (866, "OEM 866"), (869, "OEM 869"),
        (950, "Big5"), (936, "GB2312"), (932, "Shift JIS"), (949, "Korean Windows-949"),
        (51949, "EUC-KR"), (874, "Thai Windows-874"), (10007, "Mac Cyrillic"),
        (21866, "KOI8-U"), (20866, "KOI8-R"),
    ] { values.push((code.into(), name.to_owned())); }
    values
}

impl MatrixWindow {
    pub(super) fn editing_new_document_language(&mut self) -> io::Result<crate::syntax::Language> {
        self.normalize_editing_new_document_language()?;
        let id = self.pref_int("NewDocDefaultSettings.lang", 0);
        let language = match id {
            0 => crate::syntax::Language::NormalText,
            15 => {
                let name = self.pref_string("NewDocDefaultSettings.udl", "");
                self.udl.registry.named().find(|(_, definition)| definition.name == name)
                    .map_or(crate::syntax::Language::NormalText, |(id, _)| crate::syntax::Language::UserDefined(id))
            }
            57 => crate::syntax::Language::Json,
            _ => {
                let id = u16::try_from(id).map_err(|_| io::Error::other("default language ID is invalid"))?;
                let catalog = crate::languages::catalog().map_err(|error| io::Error::other(error.to_string()))?;
                if catalog.by_id(id).is_none() { return Err(io::Error::other("default language is unavailable")); }
                crate::syntax::Language::Builtin(id)
            }
        };
        Ok(language)
    }

    pub(super) fn editing_tab_preferences(&self, language: crate::syntax::Language) -> crate::input::TabPreferences {
        let name = match language {
            crate::syntax::Language::NormalText => Some("normal"),
            crate::syntax::Language::Json => Some("json"),
            crate::syntax::Language::Builtin(id) => crate::languages::catalog().ok().and_then(|catalog| catalog.by_id(id)).map(|language| language.name.as_str()),
            crate::syntax::Language::UserDefined(_) => None,
        };
        let (size, spaces, backspace_unindent) = effective_tabs(&self.preferences.settings, name.unwrap_or(""));
        crate::input::TabPreferences { size: usize::try_from(size).unwrap_or(4).max(1), spaces, backspace_unindent }
    }

    pub(super) fn apply_editing_language_properties(&self, language: &mut crate::languages::LanguageProfile) {
        if language.name == "sql" {
            language.properties.insert("sql.backslash.escapes".into(), if self.pref_bool("SQL.backSlashIsEscapeCharacterForSql", true) { "1" } else { "0" }.into());
        }
    }

    pub(super) fn apply_editing_preferences(&mut self, settings: &Value, _window: &mut Window, cx: &mut Context<Self>) {
        self.configure_intelligence(settings);
        self.configure_find(settings);
        let monospaced = *self.preferences.editing.find_monospaced.get_or_insert_with(|| boolean(settings, "Searching.monospacedFontFindDlg", false));
        self.find_query.update(cx, |field, cx| field.set_monospaced(monospaced, cx));
        self.replace_input.update(cx, |field, cx| field.set_monospaced(monospaced, cx));
        self.configure_file_search_preference_font(monospaced, cx);
        if boolean(settings, "wordCharList.useDefault", true) {
            crate::input::configure_word_characters("");
        } else if let Some(characters) = settings["wordCharList"]["charsAdded"].as_str().filter(|characters| !characters.bytes().take_while(|byte| *byte != 0).all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')) {
            crate::input::configure_word_characters(characters);
        }
        let buffers: Vec<_> = self.workspace.buffers().iter().map(|buffer| (buffer.id(), buffer.language())).collect();
        for (id, language) in buffers {
            let tabs = self.editing_tab_preferences(language);
            for view in [crate::workspace::ViewId::Primary, crate::workspace::ViewId::Secondary] {
                if let Some(editor) = self.workspace.editor_in_view_mut(view, id) { editor.set_tab_preferences(tabs); }
            }
        }
        self.builtin.syntax_cache = None;
        self.editor_chrome.invalidate_layout();
        self.editor_painted_revision = None;
        self.editor_follow_caret = true;
        self.preferences.editing.print_variable_open = false;
        self.refresh_editor_syntax();
        cx.notify();
    }

    pub(super) fn editing_delimiter_selection(&self, position: ByteOffset) -> Result<Option<Selection>, EditError> {
        let left = u8::try_from(self.pref_int("delimiterSelection.leftmostDelimiter", 40)).unwrap_or(40);
        let right = u8::try_from(self.pref_int("delimiterSelection.rightmostDelimiter", 41)).unwrap_or(41);
        self.editor().delimiter_selection_at(position, left, right, self.pref_bool("delimiterSelection.delimiterSelectionOnEntireDocument", false))
    }

    fn edit_check(&self, id: &'static str, text: &'static str, path: &'static str, default: bool, invert: bool, enabled: bool, x: f32, y: f32, w: f32, cx: &mut Context<Self>) -> AnyElement {
        self.pref_check(id, text, path, default, invert, enabled, x * SCALE, y * SCALE, w * SCALE, cx)
    }

    fn edit_radio(&self, id: &'static str, text: &'static str, path: &'static str, default: Value, value: Value, enabled: bool, x: f32, y: f32, w: f32, cx: &mut Context<Self>) -> AnyElement {
        self.pref_radio(id, text, path, default, value, enabled, x * SCALE, y * SCALE, w * SCALE, cx)
    }

    fn edit_input(&self, path: &'static str, x: f32, y: f32, w: f32, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        self.pref_input(path, x * SCALE, y * SCALE, w * SCALE, enabled, cx)
    }

    fn edit_choice(&self, path: &'static str, options: Vec<(Value, String)>, default: Value, enabled: bool, x: f32, y: f32, w: f32, cx: &mut Context<Self>) -> AnyElement {
        self.pref_choice(path, options, default, enabled, x * SCALE, y * SCALE, w * SCALE, cx)
    }

    fn edit_value_radio(&self, id: &'static str, text: &str, selected: bool, path: &'static str, value: Value, x: f32, y: f32, w: f32, cx: &mut Context<Self>) -> AnyElement {
        let text = crate::localization::current_preference_source_label(id, text);
        let focus = self.preference_control_focus(id, cx).tab_stop(true);
        let keyboard = value.clone();
        preferences_surface::source_rect(x * SCALE, y * SCALE, w * SCALE, 10. * SCALE).id(id)
            .h(px(10. * SCALE)).text_size(px(12.)).line_height(px(14.)).flex().items_center().gap(horizontal_px(3.))
            .when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse()).role(Role::RadioButton)
            .aria_label(text.to_owned()).aria_toggled(selected.into())
            .track_focus(&focus)
            .child(pref_indicator(selected, true, true)).child(text.to_owned())
            .on_click(cx.listener(move |view, _, window, cx| { focus.focus(window, cx); view.edit_set_new_encoding(path, value.clone(), window, cx); }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                view.edit_new_encoding_key(id, path, keyboard.clone(), event, window, cx);
            }))
            .into_any_element()
    }

    fn edit_list(&self, id: &'static str, index: usize, x: f32, y: f32, w: f32, h: f32, rows: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let scroll = self.preferences.editing.list_scroll[index].clone();
        let focus = self.preference_control_focus(id, cx).tab_stop(true);
        let overflowing = rows.len() as f32 * 14. > h * SCALE - 2.;
        let painted_scroll = scroll.clone();
        preferences_surface::source_rect(x * SCALE, y * SCALE, w * SCALE, h * SCALE).id(id)
            .role(Role::ListBox).track_focus(&focus)
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, window, cx| { focus.focus(window, cx); }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { view.edit_list_key(index, event, window, cx); }))
            .border_1().border_color(rgb(0x808080)).bg(rgb(0xffffff)).text_size(px(12.)).line_height(px(14.))
            .child(div().id(format!("{id}-rows")).absolute().inset_0()
                .when(overflowing, |element| {
                    if crate::localization::current_preference_rtl() { element.left(horizontal_px(17.)) } else { element.right(horizontal_px(17.)) }
                })
                .overflow_y_scroll().track_scroll(&scroll).children(rows)
                .on_scroll(cx.listener(|_, _, _, cx| cx.notify())))
            .when(overflowing, |element| element.child(
                div().id(format!("{id}-scrollbar")).absolute().top_0().bottom_0().w(horizontal_px(17.))
                    .when(crate::localization::current_preference_rtl(), |element| element.left_0())
                    .when(!crate::localization::current_preference_rtl(), |element| element.right_0())
                    .role(Role::ScrollBar).aria_orientation(gpui::accesskit::Orientation::Vertical)
                    .map(|element| scrollbar_accessibility(element, scroll.clone()))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                        let scroll = &view.preferences.editing.list_scroll[index];
                        let height = dialog_font::logical(scroll.bounds().size.height);
                        let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
                        let track = (height - 34.).max(0.);
                        let thumb = (track * height / (height + maximum).max(1.)).max(17.).min(track);
                        let start = 17. + if maximum > 0. { -dialog_font::logical(scroll.offset().y) / maximum * (track - thumb) } else { 0. };
                        let y = dialog_font::logical(event.position.y - scroll.bounds().top());
                        let delta = if y < 17. { 14. } else if y > height - 17. { -14. } else if y < start { height } else if y > start + thumb { -height } else { 0. };
                        if delta != 0. {
                            scroll.set_offset(gpui::point(px(0.), px((dialog_font::logical(scroll.offset().y) + delta).clamp(-maximum, 0.))));
                        } else {
                            view.preferences.editing.list_scroll_grab = Some((index, y - start));
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                        window.paint_quad(gpui::fill(bounds, rgb(0xf0f0f0)));
                        let height = dialog_font::logical(bounds.size.height);
                        let maximum = dialog_font::logical(painted_scroll.max_offset().y).max(0.);
                        let track = (height - 34.).max(0.);
                        let thumb = (track * height / (height + maximum).max(1.)).max(17.).min(track);
                        let start = 17. + if maximum > 0. { -dialog_font::logical(painted_scroll.offset().y) / maximum * (track - thumb) } else { 0. };
                        window.paint_quad(gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(horizontal_px(1.), px(start)), size(horizontal_px(15.), px(thumb))), rgb(0xcdcdcd)));
                        for row in 0..4 {
                            let width = 1. + row as f32 * 2.;
                            for y in [6. + row as f32, height - 7. - row as f32] {
                                window.paint_quad(gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(horizontal_px((17. - width) / 2.), px(y)), size(horizontal_px(width), px(1.))), rgb(0x606060)));
                            }
                        }
                    }).size_full())
            ))
            .into_any_element()
    }

    fn edit_list_key(&mut self, index: usize, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if index < 2 && matches!(key, "enter" | "space") {
            self.edit_move_language(index == 1, window, cx);
            cx.stop_propagation();
            return;
        }
        if !matches!(key, "up" | "down" | "home" | "end" | "pageup" | "pagedown") { return; }
        let Ok(catalog) = crate::languages::catalog() else { return; };
        let mut languages: Vec<_> = catalog.profiles().iter().filter(|language| !language.internal).collect();
        let current = if index < 2 {
            languages.sort_by(|left, right| {
                let moved = &self.preferences.editing.language_moved;
                moved.iter().position(|id| *id == left.id).cmp(&moved.iter().position(|id| *id == right.id))
                    .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            });
            languages.retain(|language| language_excluded(&self.preferences.settings, language.id) == (index == 1));
            let selected = if index == 0 { self.preferences.editing.language_available } else { self.preferences.editing.language_disabled };
            languages.iter().position(|language| Some(language.id) == selected)
        } else {
            self.preferences.editing.indentation_language.as_deref().map_or(Some(0), |selected| languages.iter().position(|language| language.name == selected).map(|position| position + 1))
        };
        let count = languages.len() + usize::from(index == 2);
        if count == 0 { return; }
        let page = (dialog_font::logical(self.preferences.editing.list_scroll[index].bounds().size.height) / 14.).floor().max(1.) as usize;
        let next = match (key, current) {
            ("home", _) | (_, None) => 0,
            ("end", _) => count - 1,
            ("up", Some(position)) => position.saturating_sub(1),
            ("pageup", Some(position)) => position.saturating_sub(page),
            ("pagedown", Some(position)) => position.saturating_add(page).min(count - 1),
            (_, Some(position)) => (position + 1).min(count - 1),
        };
        if index < 2 {
            let id = languages[next].id;
            self.preferences.editing.language_available = if index == 0 { Some(id) } else { None };
            self.preferences.editing.language_disabled = if index == 1 { Some(id) } else { None };
        } else {
            self.preferences.editing.indentation_language = next.checked_sub(1).map(|position| languages[position].name.clone());
        }
        let scroll = &self.preferences.editing.list_scroll[index];
        let height = dialog_font::logical(scroll.bounds().size.height);
        let top = next as f32 * 14.;
        let offset = -dialog_font::logical(scroll.offset().y);
        let offset = if top < offset { top } else if top + 14. > offset + height { top + 14. - height } else { offset };
        scroll.set_offset(gpui::point(scroll.offset().x, px(-offset.max(0.))));
        cx.stop_propagation();
        cx.notify();
    }

    fn edit_list_scroll_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((index, grab)) = self.preferences.editing.list_scroll_grab else { return; };
        if event.pressed_button != Some(MouseButton::Left) {
            self.preferences.editing.list_scroll_grab = None;
            return;
        }
        let scroll = &self.preferences.editing.list_scroll[index];
        let height = dialog_font::logical(scroll.bounds().size.height);
        let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
        let track = (height - 34.).max(0.);
        let thumb = (track * height / (height + maximum).max(1.)).max(17.).min(track);
        let travel = track - thumb;
        if travel > 0. {
            let fraction = ((dialog_font::logical(event.position.y - scroll.bounds().top()) - 17. - grab) / travel).clamp(0., 1.);
            scroll.set_offset(gpui::point(px(0.), px(-maximum * fraction)));
            cx.notify();
        }
    }

    pub(super) fn render_preferences_editing(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let children = match page {
            Page::NewDocument => self.edit_new_document(cx),
            Page::Language => self.edit_language(cx),
            Page::Indentation => self.edit_indentation(cx),
            Page::Completion => self.edit_completion(cx),
            Page::Delimiters => self.edit_delimiters(cx),
            Page::Searching => self.edit_searching(cx),
            Page::SearchEngine => self.edit_search_engine(cx),
            Page::Printing => self.edit_printing(window, cx),
            _ => Vec::new(),
        };
        div().relative().w(horizontal_px(805.)).h(px(358.75)).children(children)
            .on_mouse_move(cx.listener(|view, event, _, cx| view.edit_list_scroll_move(event, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| { view.preferences.editing.list_scroll_grab = None; }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, _| { view.preferences.editing.list_scroll_grab = None; }))
            .into_any_element()
    }

    fn edit_new_document(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let codepage = self.pref_int("NewDocDefaultSettings.codepage", -1);
        let encoding = self.pref_int("NewDocDefaultSettings.encoding", 4);
        let mut controls = vec![group("新建文档", 8., 4., 444., 194.), group("格式（换行符）", 16., 22., 194., 74.), group("编码", 220., 22., 224., 126.)];
        for (id, name, value, y) in [
            ("pref-new-crlf", "Windows (CR LF)", 0, 40.),
            ("pref-new-lf", "Unix (LF)", 2, 60.),
            ("pref-new-cr", "Macintosh (CR)", 1, 80.),
        ] { controls.push(self.edit_radio(id, name, "NewDocDefaultSettings.format", 2.into(), value.into(), true, 28., y, 170., cx)); }
        for (id, name, value, y) in [
            ("pref-new-utf8", "UTF-8", 4, 40.),
            ("pref-new-utf8-bom", "UTF-8 BOM", 1, 60.),
            ("pref-new-utf16-be", "UTF-16 BE BOM", 2, 80.),
            ("pref-new-utf16-le", "UTF-16 LE BOM", 3, 100.),
        ] { controls.push(self.edit_value_radio(id, name, codepage < 0 && encoding == value, "NewDocDefaultSettings.encoding", value.into(), 232., y, 200., cx)); }
        controls.push(self.edit_value_radio("pref-new-codepage", "", codepage >= 0, "NewDocDefaultSettings.codepage", if codepage >= 0 { codepage.into() } else { self.preferences.editing.new_document_codepage.unwrap_or(1250).into() }, 232., 126., 10., cx));
        controls.push(self.edit_choice("NewDocDefaultSettings.codepage", self.edit_new_document_codepages(codepage), (-1).into(), codepage >= 0, 248., 124., 184., cx));
        let mut languages = vec![(0.into(), crate::localization::menu_command(&self.native_locale(), 46016).unwrap_or("None (Normal Text)").to_owned())];
        if let Ok(catalog) = crate::languages::catalog() {
            let mut items: Vec<_> = catalog.profiles().iter().filter(|language| !language.internal && language.id != 0)
                .map(|language| (Value::from(language.id), language.label.clone())).collect();
            items.extend(self.udl.registry.named().map(|(_, language)| (Value::String(language.name.clone()), language.name.clone())));
            items.sort_by(|left, right| left.1.to_lowercase().cmp(&right.1.to_lowercase()));
            languages.extend(items);
        }
        controls.push(label("默认语言：", 28., 101., 170.));
        controls.push(self.edit_choice("NewDocDefaultSettings.lang", languages, 0.into(), true, 28., 116., 170., cx));
        controls.push(self.edit_check("pref-new-at-startup", "启动时始终打开新文档", "NewDocDefaultSettings.addNewDocumentOnStartup", false, false, true, 22., 160., 416., cx));
        controls.push(self.edit_check("pref-new-tab-name", "使用文档第一行作为未命名标签页的名称", "NewDocDefaultSettings.useContentAsTabName", false, false, true, 22., 181., 416., cx));
        controls
    }

    fn edit_language(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut controls = vec![
            group("程序语言菜单", 8., 4., 444., 175.),
            self.edit_compact_language(cx),
            label_center("可用项", 20., 40., 186.), label_center("禁用项", 254., 40., 186.),
            self.edit_check("pref-language-sql-escape", "将反斜杠视为 SQL 转义字符", "SQL.backSlashIsEscapeCharacterForSql", true, false, true, 20., 187., 420., cx),
        ];
        if let Ok(catalog) = crate::languages::catalog() {
            let mut languages: Vec<_> = catalog.profiles().iter().filter(|language| !language.internal).collect();
            languages.sort_by(|left, right| {
                let moved = &self.preferences.editing.language_moved;
                moved.iter().position(|id| *id == left.id).cmp(&moved.iter().position(|id| *id == right.id))
                    .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            });
            for (disabled, x, selected, list_id) in [
                (false, 20., self.preferences.editing.language_available, "pref-languages-available"),
                (true, 254., self.preferences.editing.language_disabled, "pref-languages-disabled"),
            ] {
                let items = languages.iter().filter(|language| {
                    let mask = self.pref_int(&format!("langsExcluded.gr{}", language.id / 8), 0);
                    (mask & (1i64 << (language.id % 8)) != 0) == disabled
                }).map(|language| {
                    let id = language.id;
                    div().id(format!("{list_id}-{id}")).w_full().h(px(14.)).flex_shrink_0().px(horizontal_px(2.))
                        .role(Role::ListBoxOption).aria_label(language.label.clone()).aria_selected((selected == Some(id)).into())
                        .bg(if selected == Some(id) { rgb(0x0078d7) } else { rgb(0xffffff) })
                        .text_color(if selected == Some(id) { rgb(0xffffff) } else { rgb(0x202020) })
                        .child(language.label.clone()).on_click(cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                            view.preferences.editing.language_disabled = if disabled { Some(id) } else { None };
                            view.preferences.editing.language_available = if disabled { None } else { Some(id) };
                            if event.click_count() == 2 { view.edit_move_language(disabled, window, cx); }
                            cx.notify();
                        })).into_any_element()
                }).collect::<Vec<_>>();
                controls.push(self.edit_list(list_id, usize::from(disabled), x, 54., 186., 112., items, cx));
            }
        }
        controls.push(self.pref_button("pref-language-remove", "->", self.preferences.editing.language_available.is_some(), 214. * SCALE, 86. * SCALE, 32. * SCALE, Self::edit_disable_language, cx));
        controls.push(self.pref_button("pref-language-restore", "<-", self.preferences.editing.language_disabled.is_some(), 214. * SCALE, 118. * SCALE, 32. * SCALE, Self::edit_restore_language, cx));
        controls
    }

    fn edit_move_language(&mut self, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selected = if restore { self.preferences.editing.language_disabled } else { self.preferences.editing.language_available };
        let Some(id) = selected else { return; };
        let path = format!("langsExcluded.gr{}", id / 8);
        let mask = self.pref_int(&path, 0);
        let bit = 1i64 << (id % 8);
        if !self.set_preference(&path, if restore { mask & !bit } else { mask | bit }.into(), window, cx) { return; }
        self.preferences.editing.language_moved.retain(|moved| *moved != id);
        self.preferences.editing.language_moved.push(id);
        self.preferences.editing.language_available = if restore { Some(id) } else { None };
        self.preferences.editing.language_disabled = if restore { None } else { Some(id) };
        self.preferences.editing.list_scroll[usize::from(!restore)].scroll_to_bottom();
        cx.notify();
    }

    fn edit_disable_language(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.edit_move_language(false, window, cx); }
    fn edit_restore_language(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.edit_move_language(true, window, cx); }

    fn edit_indentation(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut controls = vec![group("缩进设置", 8., 4., 280., 194.), group("自动缩进", 298., 4., 154., 78.)];
        let selected = self.preferences.editing.indentation_language.as_deref();
        let mut items = vec![(None, "[Default]".to_owned())];
        if let Ok(catalog) = crate::languages::catalog() {
            items.extend(catalog.profiles().iter().filter(|language| !language.internal).map(|language| (Some(language.name.clone()), language.label.clone())));
        }
        let rows = items.into_iter().enumerate().map(|(index, (name, text))| {
            let chosen = name.as_deref() == selected;
            div().id(format!("pref-indent-language-{index}")).w_full().h(px(14.)).flex_shrink_0().px(horizontal_px(2.))
                .role(Role::ListBoxOption).aria_label(text.clone()).aria_selected(chosen.into())
                .bg(if chosen { rgb(0x0078d7) } else { rgb(0xffffff) })
                .text_color(if chosen { rgb(0xffffff) } else { rgb(0x202020) }).child(text)
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.preferences.editing.indentation_language = name.clone();
                    cx.notify();
                })).into_any_element()
        }).collect::<Vec<_>>();
        controls.push(self.edit_list("pref-indent-languages", 2, 20., 20., 256., 70., rows, cx));
        let inherited = selected.is_some_and(|name| self.pref_int(&format!("TabSettings.languages.{name}.size"), 0) <= 0);
        if selected.is_some() {
            controls.push(group("", 18., 102., 260., 84.));
            let text = crate::localization::current_preference_control(6510).unwrap_or("Use default value");
            let focus = self.preference_control_focus("pref-indent-default", cx).tab_stop(true);
            controls.push(preferences_surface::source_rect(28. * SCALE, 99. * SCALE, 238. * SCALE, 10. * SCALE).id("pref-indent-default")
                .text_size(px(12.)).line_height(px(14.)).flex().items_center().gap(horizontal_px(3.))
                .when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
                .role(Role::CheckBox).aria_label(text).aria_toggled(inherited.into())
                .track_focus(&focus)
                .child(pref_indicator(inherited, false, true)).child(text)
                .on_click(cx.listener(move |view, _, window, cx| { focus.focus(window, cx); view.edit_toggle_default_tabs(window, cx); }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "space" { view.edit_toggle_default_tabs(window, cx); cx.stop_propagation(); }
                })).into_any_element());
        }
        let paths = tab_paths(if inherited { None } else { selected });
        controls.push(label_right("缩进大小：", 28., 119., 90.));
        controls.push(self.edit_input(paths[0], 124., 116., 50., !inherited, cx));
        controls.push(label("缩进方式：", 28., 140., 78.));
        controls.push(self.edit_radio("pref-indent-tabs", "制表符", paths[1], false.into(), false.into(), !inherited, 110., 140., 70., cx));
        controls.push(self.edit_radio("pref-indent-spaces", "空格字符", paths[1], false.into(), true.into(), !inherited, 190., 140., 76., cx));
        controls.push(self.edit_check("pref-indent-backspace", "退格键减少缩进，而不是删除单个空格", paths[2], false, false, !inherited, 28., 163., 238., cx));
        for (id, text, value, y) in [("pref-indent-none", "无", 0, 22.), ("pref-indent-basic", "基本模式", 2, 42.), ("pref-indent-advanced", "高级模式", 1, 62.)] {
            let radio = self.edit_radio(id, text, "maintain_indent", 1.into(), value.into(), true, if crate::localization::current_preference_rtl() { 330. } else { 0. }, 0., 130., cx);
            let tip = match value {
                2 => Some(crate::localization::current_string("autoIndentBasic-tip").unwrap_or("Ensure that the indentation of the current line (i.e. the new line created by pressing the ENTER key) matches the indentation of the previous line.")),
                1 => Some(crate::localization::current_string("autoIndentAdvanced-tip").unwrap_or("Enable smart indentation for \"C-like\" languages and Python. The \"C-like\" languages include:\nC, C++, Java, C#, Objective-C, PHP, JavaScript, JSP, CSS, Perl, Rust, PowerShell, TypeScript, Go, Swift and JSON.\n\nIf you select advanced mode but do not edit files in the aforementioned languages, the indentation will remain in basic mode.")),
                _ => None,
            };
            controls.push(preferences_surface::source_rect(310. * SCALE, y * SCALE, 130. * SCALE, 10. * SCALE)
                .id(format!("{id}-tooltip"))
                .when_some(tip, |element, tip| element.tooltip(move |_, cx| icon_tooltip::view(tip.to_owned(), false, cx)))
                .child(radio).into_any_element());
        }
        controls
    }

    fn edit_toggle_default_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.preferences.editing.indentation_language.clone() else { return; };
        let prefix = format!("TabSettings.languages.{name}");
        let inherit = self.pref_int(&format!("{prefix}.size"), 0) > 0;
        let mut value = at(&self.preferences.settings, &prefix).filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({}));
        value["size"] = if inherit { 0.into() } else { self.pref_int("TabSettings.size", 4).into() };
        value["replaceBySpace"] = (!inherit && self.pref_bool("TabSettings.replaceBySpace", false)).into();
        value["backspaceUnindent"] = (!inherit && self.pref_bool("TabSettings.backspaceUnindent", false)).into();
        self.set_preference(&prefix, value, window, cx);
    }

    fn edit_completion(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let action = self.pref_int("auto-completion.autoCAction", 3);
        let enabled = action != 0;
        let words = action == 2 || action == 3;
        let mut controls = vec![group("自动补全", 8., 4., 444., 108.), group("自动插入成对符号", 8., 118., 328., 80.)];
        let text = crate::localization::current_preference_control(6808).unwrap_or("Enable auto-completion on each input");
        let focus = self.preference_control_focus("pref-completion-enable", cx).tab_stop(true);
        controls.push(preferences_surface::source_rect(20. * SCALE, 18. * SCALE, 250. * SCALE, 10. * SCALE).id("pref-completion-enable")
            .text_size(px(12.)).line_height(px(14.)).flex().items_center().gap(horizontal_px(3.))
            .when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .role(Role::CheckBox).aria_label(text).aria_toggled(enabled.into())
            .track_focus(&focus)
            .child(pref_indicator(enabled, false, true)).child(text)
            .on_click(cx.listener(move |view, _, window, cx| {
                focus.focus(window, cx);
                if !enabled && !view.set_preference("auto-completion.autoCIgnoreNumbers", false.into(), window, cx) { return; }
                view.set_preference("auto-completion.autoCAction", if enabled { 0 } else { 3 }.into(), window, cx);
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "space" {
                    if !enabled && !view.set_preference("auto-completion.autoCIgnoreNumbers", false.into(), window, cx) { return; }
                    view.set_preference("auto-completion.autoCAction", if enabled { 0 } else { 3 }.into(), window, cx);
                    cx.stop_propagation();
                }
            })).into_any_element());
        for (id, text, value, y) in [("pref-completion-functions", "函数自动补全", 1, 36.), ("pref-completion-words", "单词自动补全", 2, 52.), ("pref-completion-both", "函数与单词", 3, 68.)] {
            controls.push(self.edit_radio(id, text, "auto-completion.autoCAction", 3.into(), value.into(), enabled, 32., y, 238., cx));
        }
        controls.push(self.edit_check("pref-completion-brief", "使用简洁版自动补全列表", "auto-completion.autoCBrief", false, false, enabled, 20., 84., 250., cx));
        controls.push(self.edit_check("pref-completion-parameters", "输入时提示函数参数", "auto-completion.funcParams", true, false, true, 20., 100., 250., cx));
        controls.push(label_right(crate::localization::current_preference_control(6811).unwrap_or("From"), 284., 18., 40.));
        controls.push(label_center(&self.pref_int("auto-completion.triggerFromNbChar", 1).to_string(), 330., 18., 12.));
        controls.push(label(crate::localization::current_preference_control(6813).unwrap_or("th character"), 348., 18., 92.));
        controls.push(label_right("1", 294., 36., 10.));
        controls.push(label("9", 432., 36., 8.));
        controls.push(self.pref_slider("auto-completion.triggerFromNbChar", 1, 9, 1, 310. * SCALE, 34. * SCALE, 116. * SCALE, enabled, cx));
        controls.push(group("插入选中项", 284., 54., 156., 34.));
        controls.push(self.edit_check("pref-completion-tab", "TAB（制表键）", "auto-completion.insertSelectedItemUseTAB", true, false, enabled, 294., 70., 64., cx));
        controls.push(self.edit_check("pref-completion-enter", "ENTER（回车键）", "auto-completion.insertSelectedItemUseENTER", true, false, enabled, 366., 70., 68., cx));
        controls.push(self.edit_check("pref-completion-ignore-numbers", "忽略数字", "auto-completion.autoCIgnoreNumbers", true, false, enabled && words, 284., 100., 156., cx));
        for (id, key, open, close, x, y, w) in [
            ("pref-insert-parentheses", "auto-insert.parentheses", "(", "(  )", 24., 142., 34.),
            ("pref-insert-brackets", "auto-insert.brackets", "[", "[  ]", 24., 162., 34.),
            ("pref-insert-curly", "auto-insert.curlyBrackets", "{", "{  }", 24., 182., 34.),
            ("pref-insert-double-quotes", "auto-insert.doubleQuotes", "\"", "\"  \"", 80., 142., 34.),
            ("pref-insert-quotes", "auto-insert.quotes", "'", "'  '", 80., 162., 34.),
            ("pref-insert-tags", "auto-insert.htmlXmlTag", "html/xml 结束标签", "html/xml 结束标签", 80., 182., 88.),
        ] {
            controls.push(self.edit_check(id, if self.pref_bool(key, false) { close } else { open }, key, false, false, true, x, y, w, cx));
        }
        controls.push(label_center("打开", 240., 128., 36.));
        controls.push(label_center("关闭", 294., 128., 36.));
        for index in 0..3 {
            let y = 142. + index as f32 * 20.;
            controls.push(label(&format!("匹配对{}：", index + 1), 180., y + 2., 60.));
            controls.push(self.edit_input(PAIR_PARTS[index][0], 246., y, 24., true, cx));
            controls.push(self.edit_input(PAIR_PARTS[index][1], 300., y, 24., true, cx));
        }
        controls
    }

    fn edit_delimiters(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let custom = !self.pref_bool("wordCharList.useDefault", true);
        let multiline = self.pref_bool("delimiterSelection.delimiterSelectionOnEntireDocument", false);
        let mut controls = vec![
            group("定义单词字符列表", 8., 4., 444., 96.), group("分隔符选择设置（Ctrl + 双击鼠标左键）", 8., 110., 444., 88.),
            self.edit_radio("pref-word-default", "使用默认的单词字符列表", "wordCharList.useDefault", true.into(), true.into(), true, 20., 20., 420., cx),
            self.edit_radio("pref-word-custom", "添加字符列表\n（若不了解请勿选择）", "wordCharList.useDefault", true.into(), false.into(), true, 20., 36., 420., cx),
            self.edit_input("wordCharList.charsAdded", 32., 60., 372., custom, cx),
            label_right("打开", 20., 130., 54.), label("关闭", 296., if multiline { 154. } else { 130. }, 144.),
            self.edit_input("delimiterSelection.leftmostDelimiter", 80., 128., 20., true, cx),
            self.edit_input("delimiterSelection.rightmostDelimiter", 270., if multiline { 152. } else { 128. }, 20., true, cx),
            self.edit_check("pref-delimiter-multiline", "允许选取多行", "delimiterSelection.delimiterSelectionOnEntireDocument", false, false, true, 20., 180., 420., cx),
        ];
        controls.push(self.edit_help("pref-word-help", "添加的字符会影响双击选词和查找中的全词匹配。", true, 414., 62.));
        controls.push(preferences_surface::source_rect(102. * SCALE, 130. * SCALE, 166. * SCALE, 8. * SCALE).bg(rgb(0xd2d2d2)).text_size(px(12.)).line_height(px(14.)).overflow_hidden().child("bla bla bla bla bla bla").into_any_element());
        if multiline { controls.push(preferences_surface::source_rect(62. * SCALE, 154. * SCALE, 206. * SCALE, 8. * SCALE).bg(rgb(0xd2d2d2)).text_size(px(12.)).line_height(px(14.)).overflow_hidden().child("bla bla bla bla bla bla bla bla bla bla bla bla").into_any_element()); }
        if custom {
            let text = self.pref_string("wordCharList.charsAdded", "");
            let spaces = text.chars().filter(|ch| *ch == ' ').count();
            let tabs = text.chars().filter(|ch| *ch == '\t').count();
            if spaces + tabs > 0 {
                let mut parts = Vec::new();
                if spaces > 0 { parts.push(crate::localization::current_string("word-chars-list-space-warning").unwrap_or("$INT_REPLACE$ space(s)").replace("$INT_REPLACE$", &spaces.to_string())); }
                if tabs > 0 { parts.push(crate::localization::current_string("word-chars-list-tab-warning").unwrap_or("$INT_REPLACE$ TAB(s)").replace("$INT_REPLACE$", &tabs.to_string())); }
                controls.push(label(&format!("{}{}{}", crate::localization::current_string("word-chars-list-warning-begin").unwrap_or("Be aware: "), parts.join(" + "), crate::localization::current_string("word-chars-list-warning-end").unwrap_or(" in your character list.")), 32., 80., 408.));
            }
        }
        controls
    }

    fn edit_help(&self, id: &'static str, text: &'static str, enabled: bool, x: f32, y: f32) -> AnyElement {
        let text = match id {
            "pref-word-help" => crate::localization::current_string("word-chars-list-tip").unwrap_or(text).to_owned(),
            "pref-search-selection-help" => crate::localization::current_string("searchingInSelThresh-tip").unwrap_or(text).replace("$INT_REPLACE$", "1024"),
            "pref-search-fill-help" => crate::localization::current_string("searchingFillFindWhat-tip").unwrap_or(text).replace("$INT_REPLACE$", "16383"),
            _ => text.to_owned(),
        };
        preferences_surface::source_rect(x * SCALE, y * SCALE, 12. * SCALE, 10. * SCALE).id(id)
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .text_size(px(12.)).text_color(rgb(if enabled { 0x0066cc } else { 0x808080 })).child("(?)")
            .when(enabled, |element| element.tooltip(move |_, cx| icon_tooltip::view(text.clone(), false, cx))).into_any_element()
    }

    fn edit_searching(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let fill = self.pref_bool("Searching.fillFindFieldWithSelected", true);
        vec![
            group("调用查找对话框时", 8., 4., 444., 104.),
            label("自动勾选“选取范围内”的最小值（选中字符数）：", 20., 20., 174.),
            self.edit_input("Searching.inSelectionAutocheckThreshold", 200., 18., 45., true, cx),
            self.edit_help("pref-search-selection-help", "自动勾选选取范围的最少选中字符数。最大值为 1024；0 禁用自动勾选。", true, 256., 20.),
            self.edit_check("pref-search-directory", "使用当前文档所在目录填充目录字段（文件中查找）", "Searching.fillDirFieldFromActiveDoc", false, false, true, 20., 42., 420., cx),
            self.edit_check("pref-search-selected", "使用选中文本填充查找字段", "Searching.fillFindFieldWithSelected", true, false, true, 20., 59., 420., cx),
            self.edit_help("pref-search-fill-help", "从选区自动填充查找字段的最大字符数：1–16383。", fill, 32., 77.),
            self.edit_input("Searching.fillFindWhatThreshold", 48., 75., 42., fill, cx),
            preferences_surface::pref_label_colour("：从选择内容自动填充查找字段的最大字符数", 96. * SCALE, 77. * SCALE, 344. * SCALE, self.preferences_render_palette().control_text(fill)),
            self.edit_check("pref-search-caret", "未选中内容时，使用光标下的单词填充", "Searching.fillFindFieldSelectCaret", true, false, fill, 32., 94., 408., cx),
            self.edit_check("pref-search-monospace", "在查找对话框中使用等宽字体（需要重启 Notepad++）", "Searching.monospacedFontFindDlg", false, false, true, 20., 124., 420., cx),
            self.edit_check("pref-search-remain-open", "搜索输出到结果窗口后，保持查找对话框开启", "Searching.findDlgAlwaysVisible", false, false, true, 20., 145., 420., cx),
            self.edit_check("pref-search-single-result", "搜索结果窗口中，每个找到的行尽量仅显示一条", "FinderConfig.showOnlyOneEntryPerFoundLine", true, false, true, 20., 166., 420., cx),
            self.edit_check("pref-search-disk-content", "在文件中查找：优先选择磁盘上的文件内容而非打开的缓冲区", "FinderConfig.FiF_ignoreunsavedChangesInOpenedFiles", false, false, true, 20., 187., 420., cx),
        ]
    }

    fn edit_search_engine(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let engine = self.pref_int("searchEngine.searchEngineChoice", 2);
        let mut controls = vec![group("搜索引擎（用于“在线搜索”操作）", 8., 4., 444., 194.)];
        for (id, text, value, y) in [
            ("pref-engine-duckduckgo", "DuckDuckGo", 1, 24.), ("pref-engine-google", "谷歌", 2, 46.),
            ("pref-engine-yahoo", "雅虎", 4, 68.), ("pref-engine-stackoverflow", "Stack Overflow", 5, 90.),
            ("pref-engine-custom", "自定义搜索引擎：", 0, 112.),
        ] { controls.push(self.edit_radio(id, text, "searchEngine.searchEngineChoice", 2.into(), value.into(), true, 20., y, 420., cx)); }
        controls.push(self.edit_input("searchEngine.searchEngineCustom", 32., 134., 408., engine == 0, cx));
        controls.push(label("示例：https://www.baidu.com/s?wd=$(CURRENT_WORD)", 32., 158., 408.));
        controls
    }

    pub(super) fn normalize_editing_on_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.normalize_editing_new_document_language() { self.notify_editor_error(error.to_string(), cx); return; }
        let engine = self.pref_int("searchEngine.searchEngineChoice", 2);
        if engine == 3 { self.set_preference("searchEngine.searchEngineChoice", 1.into(), window, cx); }
        else if engine == 0 && self.pref_string("searchEngine.searchEngineCustom", "").is_empty() {
            self.set_preference("searchEngine.searchEngineChoice", 2.into(), window, cx);
        }
    }

    pub(super) fn editing_field_focused(&mut self, path: &str, cx: &mut Context<Self>) {
        if PRINT_PARTS.contains(&path) {
            self.preferences.editing.print_focus = Some(path.to_owned());
            cx.notify();
        }
    }

    fn edit_printing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        for path in PRINT_PARTS {
            if let Some(field) = self.pref_field(path) {
                if field.read(cx).focus_handle().is_focused(window) {
                    self.preferences.editing.print_focus = Some(path.to_owned());
                }
            }
        }
        let mut controls = vec![group("色彩模式", 6., 3., 133., 74.), group("页边距（单位：毫米）", 6., 83., 133., 83.), group("页眉与页脚", 148., 3., 300., 178.)];
        for (id, text, value, y) in [("pref-print-normal", "所见即所得", 0, 16.), ("pref-print-invert", "反色输出", 1, 31.), ("pref-print-bw", "白底黑字", 2, 46.), ("pref-print-no-bg", "无背景色", 3, 61.)] {
            controls.push(self.edit_radio(id, text, "PrintSettings.printOption", 3.into(), value.into(), true, 12., y, 121., cx));
        }
        controls.extend([label_center(crate::localization::current_preference_control(6613).unwrap_or("Top"), 43., 95., 54.), label_right(crate::localization::current_preference_control(6612).unwrap_or("Left"), 9., 124., 30.), label(crate::localization::current_preference_control(6614).unwrap_or("Right"), 101., 124., 30.), label_center(crate::localization::current_preference_control(6615).unwrap_or("Bottom"), 43., 152., 54.)]);
        for (path, x, y) in [("PrintSettings.margeTop", 61., 105.), ("PrintSettings.margeLeft", 43., 121.), ("PrintSettings.margeRight", 79., 121.), ("PrintSettings.margeBottom", 61., 137.)] {
            controls.push(self.edit_input(path, x, y, 17., true, cx));
        }
        controls.push(self.edit_check("pref-print-line-numbers", "打印行号", "PrintSettings.lineNumber", true, false, true, 11., 170., 133., cx));
        controls.push(self.edit_check("pref-print-form-feed", "Print formfeed as page break", "PrintSettings.formFeedPageBreak", false, false, true, 11., 185., 433., cx));
        controls.push(label_right("变量：", 181., 15., 60.));
        let variable = self.preferences.editing.print_variable.min(PRINT_VARIABLES.len() - 1);
        let variable_label = crate::localization::current_preference_combo(6724, variable).unwrap_or(PRINT_VARIABLES[variable].1);
        let variable_focus = self.preference_control_focus("pref-print-variable", cx).tab_stop(true);
        controls.push(preferences_surface::source_rect(244. * SCALE, 14. * SCALE, 122. * SCALE, 13. * SCALE).id("pref-print-variable")
            .h(px(23.)).px(horizontal_px(3.)).border_1().border_color(rgb(0x808080)).bg(rgb(0xe1e1e1)).text_size(px(12.)).line_height(px(14.)).role(Role::ComboBox)
            .aria_label(crate::localization::current_preference_control(6725).unwrap_or("Variable")).flex().items_center().justify_between().when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse()).child(variable_label).child("▾")
            .track_focus(&variable_focus)
            .on_click(cx.listener(move |view, _, window, cx| { variable_focus.focus(window, cx); view.preferences.editing.print_variable_open = !view.preferences.editing.print_variable_open; cx.notify(); }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                let state = &mut view.preferences.editing;
                match event.keystroke.key.as_str() {
                    "up" => state.print_variable = state.print_variable.saturating_sub(1),
                    "down" => state.print_variable = (state.print_variable + 1).min(PRINT_VARIABLES.len() - 1),
                    "home" => state.print_variable = 0,
                    "end" => state.print_variable = PRINT_VARIABLES.len() - 1,
                    "space" | "enter" => state.print_variable_open = !state.print_variable_open,
                    "escape" if state.print_variable_open => state.print_variable_open = false,
                    _ => return,
                }
                cx.stop_propagation(); cx.notify();
            })).into_any_element());
        controls.push(self.pref_button("pref-print-add-variable", "添加", true, 369. * SCALE, 14. * SCALE, 50. * SCALE, Self::edit_add_print_variable, cx));
        let mut fonts = cx.text_system().all_font_names();
        fonts.push(String::new());
        fonts.sort();
        fonts.dedup();
        let fonts: Vec<_> = fonts.into_iter().map(|name| (Value::String(name.clone()), name)).collect();
        for (index, section, title, font_name, font_size, font_style, y) in [
            (0, "header", "页眉", "PrintSettings.headerFontName", "PrintSettings.headerFontSize", "PrintSettings.headerFontStyle", 30.),
            (3, "footer", "页脚", "PrintSettings.footerFontName", "PrintSettings.footerFontSize", "PrintSettings.footerFontStyle", 90.),
        ] {
            controls.push(group(title, 156., y, 284., 57.));
            for (part, text, x, width) in [(0, "左", 164., 85.), (1, "中", 255., 86.), (2, "右", 347., 85.)] {
                let source_id = (if index == 0 { 6709 } else { 6720 }) + part as u32;
                let label_x = if index == 3 && part == 0 { 168. } else { x + 2. };
                controls.push(label_center(crate::localization::current_preference_control(source_id).unwrap_or(text), label_x, y + 10., if part == 1 { 82. } else { 81. }));
                controls.push(self.edit_input(PRINT_PARTS[index + part], x, y + 19., width, true, cx));
            }
            controls.push(self.edit_choice(font_name, fonts.clone(), "".into(), true, 164., y + 37., 134., cx));
            controls.push(self.edit_choice(font_size, (6..15).map(|size| (Value::from(size), size.to_string())).collect(), 0.into(), true, 302., y + 37., 31., cx));
            for (bold, text, x, bit) in [(true, "粗体", 338., 1), (false, "斜体", 388., 2)] {
                let path = font_style;
                let checked = self.pref_int(path, 0) & bit != 0;
                let id = match (section, bold) {
                    ("header", true) => "pref-print-header-bold", ("header", false) => "pref-print-header-italic",
                    (_, true) => "pref-print-footer-bold", (_, false) => "pref-print-footer-italic",
                };
                let focus = self.preference_control_focus(id, cx).tab_stop(true);
                let source_id = if index == 0 { if bold { 6706 } else { 6707 } } else if bold { 6717 } else { 6718 };
                let text = crate::localization::current_preference_control(source_id).unwrap_or(text);
                controls.push(preferences_surface::source_rect(x * SCALE, (y + 39.) * SCALE, (if bold { 47. } else { 50. }) * SCALE, 10. * SCALE).id(id)
                    .role(Role::CheckBox).aria_label(text).aria_toggled(checked.into()).text_size(px(12.)).line_height(px(14.))
                    .track_focus(&focus)
                    .flex().items_center().gap(horizontal_px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
                    .child(pref_indicator(checked, false, true)).child(text)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        focus.focus(window, cx);
                        let value = view.pref_int(&path, 0) ^ bit;
                        view.set_preference(&path, value.into(), window, cx);
                    }))
                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "space" {
                            let value = view.pref_int(path, 0) ^ bit;
                            view.set_preference(path, value.into(), window, cx); cx.stop_propagation();
                        }
                    })).into_any_element());
            }
        }
        let focus = self.preferences.editing.print_focus.as_deref();
        let title = focus.and_then(|path| PRINT_PARTS.iter().position(|part| *part == path)).map(|index| {
            format!("{} {}", crate::localization::current_preference_control(if index < 3 { 6708 } else { 6719 }).unwrap_or(""), crate::localization::current_preference_control((if index < 3 { 6709 } else { 6720 }) + (index % 3) as u32).unwrap_or(""))
        }).unwrap_or_else(|| crate::localization::current_preference_control(6727).unwrap_or("Your variables will appear here").to_owned());
        controls.push(label_center(&title, 158., 152., 280.));
        let preview = focus.and_then(|path| self.pref_field(path)).map(|field| field.read(cx).text()).unwrap_or_default();
        controls.push(preferences_surface::source_rect(158. * SCALE, 161. * SCALE, 280. * SCALE, 12. * SCALE).id("pref-print-preview")
            .border_1().border_color(rgb(0x808080)).bg(rgb(0xffffff)).text_size(px(12.)).overflow_hidden().child(preview).into_any_element());
        if self.preferences.editing.print_variable_open {
            let rows = PRINT_VARIABLES.into_iter().enumerate().map(|(index, (_, text))| {
                let text = crate::localization::current_preference_combo(6724, index).unwrap_or(text);
                div().id(format!("pref-print-variable-{index}")).w_full().h(px(14.)).px(horizontal_px(3.))
                    .bg(if index == variable { rgb(0x0078d7) } else { rgb(0xffffff) })
                    .text_color(if index == variable { rgb(0xffffff) } else { rgb(0x202020) }).child(text)
                    .on_click(cx.listener(move |view, _, _, cx| { view.preferences.editing.print_variable = index; view.preferences.editing.print_variable_open = false; cx.notify(); }))
            }).collect::<Vec<_>>();
            controls.push(preferences_surface::source_rect(244. * SCALE, 14. * SCALE + 23., 122. * SCALE, 100.).id("pref-print-variables-list")
                .border_1().border_color(rgb(0x808080)).bg(rgb(0xffffff)).text_size(px(12.)).line_height(px(14.)).occlude().children(rows).into_any_element());
        }
        controls
    }

    fn edit_add_print_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.preferences.editing.print_focus.clone() else { return; };
        let Some(field) = self.pref_field(&path) else { return; };
        let Some((variable, _)) = PRINT_VARIABLES.get(self.preferences.editing.print_variable).copied() else { return; };
        field.update(cx, |field, cx| field.insert_text(variable, window, cx));
        let text = field.read(cx).text();
        self.set_preference(&path, text.into(), window, cx);
    }

    fn edit_set_new_encoding(&mut self, path: &'static str, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        let codepage = self.pref_int("NewDocDefaultSettings.codepage", -1);
        if self.set_preference(path, value, window, cx) && codepage >= 0 {
            self.preferences.editing.new_document_codepage = Some(codepage);
        }
    }

    fn edit_new_encoding_key(&mut self, id: &'static str, path: &'static str, value: Value, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "space" { self.edit_set_new_encoding(path, value, window, cx); cx.stop_propagation(); }
        if !matches!(event.keystroke.key.as_str(), "left" | "up" | "right" | "down") { return; }
        let codepage = self.pref_int("NewDocDefaultSettings.codepage", -1);
        let codepage = if codepage >= 0 { codepage } else { self.preferences.editing.new_document_codepage.unwrap_or(1250) };
        let options = [
            ("pref-new-utf8", "NewDocDefaultSettings.encoding", 4),
            ("pref-new-utf8-bom", "NewDocDefaultSettings.encoding", 1),
            ("pref-new-utf16-be", "NewDocDefaultSettings.encoding", 2),
            ("pref-new-utf16-le", "NewDocDefaultSettings.encoding", 3),
            ("pref-new-codepage", "NewDocDefaultSettings.codepage", codepage),
        ];
        if let Some(index) = options.iter().position(|(candidate, _, _)| *candidate == id) {
            let index = if matches!(event.keystroke.key.as_str(), "left" | "up") { (index + options.len() - 1) % options.len() } else { (index + 1) % options.len() };
            let (target, path, value) = options[index];
            self.preference_control_focus(target, cx).focus(window, cx);
            self.edit_set_new_encoding(path, value.into(), window, cx); cx.stop_propagation();
        }
    }

    fn edit_new_document_codepages(&self, codepage: i64) -> Vec<(Value, String)> {
        let mut pages = codepages();
        if codepage < 0 && let Some(previous) = self.preferences.editing.new_document_codepage {
            if let Some((_, label)) = pages.iter().find(|(value, _)| value.as_i64() == Some(previous)).cloned() {
                pages.push(((-1).into(), label));
            }
        }
        pages
    }

    fn edit_compact_language(&self, cx: &mut Context<Self>) -> AnyElement {
        let id = "pref-language-compact";
        let text = crate::localization::current_preference_source_label(id, "折叠显示语言菜单");
        let checked = self.pref_bool("langsExcluded.langMenuCompact", true);
        let focus = self.preference_control_focus(id, cx).tab_stop(true);
        preferences_surface::source_rect(20. * SCALE, 20. * SCALE, 420. * SCALE, 10. * SCALE).id(id)
            .role(Role::CheckBox).aria_label(text).aria_toggled(checked.into()).track_focus(&focus)
            .text_size(px(12.)).line_height(px(14.)).text_color(rgb(self.preferences_render_palette().text))
            .flex().items_center().gap(horizontal_px(3.)).when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .child(pref_indicator(checked, false, true)).child(text)
            .on_click(cx.listener(move |view, _, window, cx| { focus.focus(window, cx); view.edit_toggle_compact_language(window, cx); }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "space" { view.edit_toggle_compact_language(window, cx); cx.stop_propagation(); }
            })).into_any_element()
    }

    fn edit_toggle_compact_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = !self.pref_bool("langsExcluded.langMenuCompact", true);
        if !self.set_preference("langsExcluded.langMenuCompact", value.into(), window, cx) { return; }
        let locale = self.native_locale();
        let title = crate::localization::dialog_title(&locale, "LanguageMenuCompactWarning").unwrap_or("Compact Language Menu");
        let message = crate::localization::dialog_message(&locale, "LanguageMenuCompactWarning").unwrap_or("This option will be changed on the next launch.");
        let response = window.prompt(gpui::PromptLevel::Info, title, Some(message), &[gpui::PromptButton::ok("OK")], cx);
        cx.spawn_in(window, async move |_, _| {
            if let Err(error) = response.await { eprintln!("compact language menu warning failed: {error}"); }
        }).detach();
    }

    fn normalize_editing_new_document_language(&mut self) -> io::Result<()> {
        if self.pref_int("NewDocDefaultSettings.lang", 0) != 15 { return Ok(()); }
        let name = self.pref_string("NewDocDefaultSettings.udl", "");
        if !name.is_empty() && self.udl.registry.named().any(|(_, definition)| definition.name == name) { return Ok(()); }
        let previous_lang = at(&self.preferences.settings, "NewDocDefaultSettings.lang").cloned();
        let previous_udl = at(&self.preferences.settings, "NewDocDefaultSettings.udl").cloned();
        let mut settings = self.preferences.settings.clone();
        put(&mut settings, "NewDocDefaultSettings.lang", 0.into())?;
        put(&mut settings, "NewDocDefaultSettings.udl", "".into())?;
        if let Some(store) = &self.settings_store {
            store.update_settings(|document| {
                if at(document, "NewDocDefaultSettings.lang") != previous_lang.as_ref()
                    || at(document, "NewDocDefaultSettings.udl") != previous_udl.as_ref() {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "default document language changed while normalizing an unavailable user language"));
                }
                put(document, "NewDocDefaultSettings.lang", 0.into())?;
                put(document, "NewDocDefaultSettings.udl", "".into())
            })?;
        }
        self.preferences.settings = settings;
        Ok(())
    }
}
