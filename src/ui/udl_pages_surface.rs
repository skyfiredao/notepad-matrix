use super::*;

type Rect = (f32, f32, f32, f32);

#[derive(Clone, Copy)]
enum Item {
    Group(u32, &'static str, Rect, bool),
    Label(u32, &'static str, Rect, bool),
    Field(&'static str, Rect),
    Check(&'static str, Rect),
    Radio(&'static str, usize, Rect),
    Styler(usize, u32, Rect),
    Link(Rect),
}

use Item::*;

const FOLDER: &[Item] = &[
    Group(21105, "Documentation", (12., 14., 210., 49.), false),
    Link((19., 35., 200., 13.)),
    Group(21101, "Default style", (12., 71., 210., 49.), false),
    Styler(0, 21102, (87., 94., 54., 14.)),
    Check("/fold_compact", (12., 143., 152., 10.)),
    Group(21420, "Folding in comment style", (241., 14., 210., 156.), true),
    Styler(15, 21427, (252., 30., 54., 14.)),
    Label(21424, "Open:", (252., 53., 113., 8.), false),
    Field("/folding/2/open", (252., 62., 190., 22.)),
    Label(21425, "Middle:", (252., 89., 113., 8.), false),
    Field("/folding/2/middle", (252., 99., 190., 22.)),
    Label(21426, "Close:", (252., 126., 113., 8.), false),
    Field("/folding/2/close", (252., 137., 190., 22.)),
    Group(21220, "Folding in code 1 style", (12., 186., 210., 156.), true),
    Styler(13, 21227, (22., 203., 54., 14.)),
    Label(21224, "Open:", (23., 224., 113., 8.), false),
    Field("/folding/0/open", (22., 236., 190., 22.)),
    Label(21225, "Middle:", (23., 262., 113., 8.), false),
    Field("/folding/0/middle", (22., 273., 190., 22.)),
    Label(21226, "Close:", (23., 299., 113., 8.), false),
    Field("/folding/0/close", (22., 311., 190., 22.)),
    Group(21320, "Folding in code 2 style (separators needed)", (242., 186., 210., 156.), true),
    Styler(14, 21327, (253., 202., 54., 14.)),
    Label(21324, "Open:", (253., 224., 113., 8.), false),
    Field("/folding/1/open", (253., 236., 190., 22.)),
    Label(21325, "Middle:", (253., 261., 113., 8.), false),
    Field("/folding/1/middle", (253., 273., 190., 22.)),
    Label(21326, "Close:", (253., 298., 113., 8.), false),
    Field("/folding/1/close", (253., 311., 190., 22.)),
];

const KEYWORDS: &[Item] = &[
    Group(22101, "1st group", (9., 17., 200., 85.), true),
    Styler(4, 22122, (19., 27., 53., 14.)),
    Check("/prefix_keywords/0", (89., 31., 68., 10.)),
    Field("/keywords/0", (19., 44., 180., 50.)),
    Group(22201, "2nd group", (252., 17., 200., 85.), true),
    Styler(5, 22222, (262., 26., 53., 14.)),
    Check("/prefix_keywords/1", (332., 31., 68., 10.)),
    Field("/keywords/1", (262., 44., 180., 50.)),
    Group(22301, "3rd group", (10., 111., 200., 85.), true),
    Styler(6, 22322, (19., 120., 53., 14.)),
    Check("/prefix_keywords/2", (89., 125., 68., 10.)),
    Field("/keywords/2", (19., 138., 180., 50.)),
    Group(22401, "4th group", (252., 111., 200., 85.), true),
    Styler(7, 22422, (262., 120., 53., 14.)),
    Check("/prefix_keywords/3", (332., 125., 68., 10.)),
    Field("/keywords/3", (262., 138., 180., 50.)),
    Group(22451, "5th group", (10., 206., 200., 85.), true),
    Styler(8, 22472, (19., 215., 53., 14.)),
    Check("/prefix_keywords/4", (89., 220., 68., 10.)),
    Field("/keywords/4", (19., 233., 180., 50.)),
    Group(22501, "6th group", (252., 206., 200., 85.), true),
    Styler(9, 22522, (262., 215., 53., 14.)),
    Check("/prefix_keywords/5", (332., 220., 68., 10.)),
    Field("/keywords/5", (262., 233., 180., 50.)),
    Group(22551, "7th group", (10., 302., 200., 85.), true),
    Styler(10, 22572, (19., 311., 53., 14.)),
    Check("/prefix_keywords/6", (89., 315., 72., 10.)),
    Field("/keywords/6", (19., 329., 180., 50.)),
    Group(22601, "8th group", (252., 302., 200., 85.), true),
    Styler(11, 22622, (262., 311., 53., 14.)),
    Check("/prefix_keywords/7", (332., 315., 72., 10.)),
    Field("/keywords/7", (262., 329., 180., 50.)),
];

const COMMENTS: &[Item] = &[
    Group(23003, "Line comment position", (11., 7., 156., 57.), false),
    Check("/allow_fold_comments", (188., 14., 131., 10.)),
    Radio("/line_comment_placement", 0, (33., 22., 121., 10.)),
    Radio("/line_comment_placement", 1, (33., 34., 121., 10.)),
    Radio("/line_comment_placement", 2, (33., 46., 121., 10.)),
    Group(23301, "Comment line style", (10., 71., 219., 94.), true),
    Styler(2, 23326, (169., 85., 54., 14.)),
    Label(23323, "Open:", (12., 105., 85., 8.), true),
    Field("/comments/line_open", (99., 105., 124., 12.)),
    Label(23324, "Continue character:", (12., 126., 85., 8.), true),
    Field("/comments/line_continue", (99., 124., 124., 12.)),
    Label(23325, "Close:", (12., 145., 85., 8.), true),
    Field("/comments/line_close", (99., 144., 124., 12.)),
    Group(23101, "Comment style", (238., 71., 219., 94.), true),
    Styler(1, 23124, (396., 85., 54., 14.)),
    Label(23122, "Open:", (242., 113., 67., 8.), true),
    Field("/comments/block_open", (312., 112., 138., 12.)),
    Label(23123, "Close:", (242., 141., 68., 8.), true),
    Field("/comments/block_close", (312., 140., 138., 12.)),
    Group(23201, "Number style", (10., 174., 447., 190.), true),
    Styler(3, 23220, (376., 185., 54., 14.)),
    Label(23230, "Prefix 1:", (14., 204., 45., 8.), true),
    Field("/numbers/prefix1", (63., 204., 136., 21.)),
    Label(23232, "Prefix 2:", (245., 204., 45., 8.), true),
    Field("/numbers/prefix2", (294., 204., 136., 21.)),
    Label(23234, "Extras 1:", (14., 235., 45., 8.), true),
    Field("/numbers/extras1", (63., 235., 136., 21.)),
    Label(23236, "Extras 2:", (245., 235., 45., 8.), true),
    Field("/numbers/extras2", (295., 235., 136., 21.)),
    Label(23238, "Suffix 1:", (14., 266., 45., 8.), true),
    Field("/numbers/suffix1", (63., 266., 136., 21.)),
    Label(23240, "Suffix 2:", (245., 266., 45., 8.), true),
    Field("/numbers/suffix2", (295., 266., 136., 21.)),
    Label(23242, "Range:", (14., 298., 45., 8.), true),
    Field("/numbers/range", (63., 298., 136., 21.)),
    Group(23244, "Decimal separator", (295., 299., 136., 53.), true),
    Radio("/decimal_separator", 0, (319., 310., 59., 10.)),
    Radio("/decimal_separator", 1, (319., 322., 59., 10.)),
    Radio("/decimal_separator", 2, (319., 335., 59., 10.)),
];

const OPERATORS: &[Item] = &[
    Group(24101, "Operators style", (8., 3., 451., 55.), true),
    Styler(12, 24113, (14., 12., 54., 14.)),
    Label(24116, "Operators 1", (142., 20., 75., 8.), true),
    Field("/operators/0", (15., 31., 205., 19.)),
    Label(24117, "Operators 2 (separators required)", (247., 20., 206., 8.), true),
    Field("/operators/1", (247., 31., 206., 19.)),
    Group(24201, "Delimiter 1 style", (8., 63., 212., 82.), false),
    Styler(16, 24223, (162., 71., 54., 14.)),
    Label(24220, "Open:", (12., 90., 56., 8.), true),
    Field("/delimiters/0/open", (72., 88., 144., 12.)),
    Label(24221, "Escape:", (12., 109., 56., 8.), true),
    Field("/delimiters/0/escape", (72., 107., 144., 12.)),
    Label(24222, "Close:", (12., 127., 56., 8.), true),
    Field("/delimiters/0/close", (72., 126., 144., 12.)),
    Group(24301, "Delimiter 2 style", (245., 63., 212., 82.), false),
    Styler(17, 24323, (399., 71., 54., 14.)),
    Label(24320, "Open:", (251., 90., 56., 8.), true),
    Field("/delimiters/1/open", (309., 88., 144., 12.)),
    Label(24321, "Escape:", (251., 109., 56., 8.), true),
    Field("/delimiters/1/escape", (309., 107., 144., 12.)),
    Label(24322, "Close:", (251., 127., 56., 8.), true),
    Field("/delimiters/1/close", (309., 126., 144., 12.)),
    Group(24401, "Delimiter 3 style", (8., 148., 212., 82.), false),
    Styler(18, 24423, (162., 156., 54., 14.)),
    Label(24420, "Open:", (12., 176., 56., 8.), true),
    Field("/delimiters/2/open", (72., 174., 144., 12.)),
    Label(24421, "Escape:", (12., 195., 56., 8.), true),
    Field("/delimiters/2/escape", (72., 193., 144., 12.)),
    Label(24422, "Close:", (12., 214., 56., 8.), true),
    Field("/delimiters/2/close", (72., 212., 144., 12.)),
    Group(24451, "Delimiter 4 style", (245., 148., 212., 82.), false),
    Styler(19, 24473, (399., 156., 54., 14.)),
    Label(24470, "Open:", (251., 176., 56., 8.), true),
    Field("/delimiters/3/open", (309., 174., 144., 12.)),
    Label(24471, "Escape:", (251., 195., 56., 8.), true),
    Field("/delimiters/3/escape", (309., 193., 144., 12.)),
    Label(24472, "Close:", (251., 214., 56., 8.), true),
    Field("/delimiters/3/close", (309., 212., 144., 12.)),
    Group(24501, "Delimiter 5 style", (8., 234., 212., 82.), false),
    Styler(20, 24523, (162., 242., 54., 14.)),
    Label(24520, "Open:", (12., 260., 56., 8.), true),
    Field("/delimiters/4/open", (72., 259., 144., 12.)),
    Label(24521, "Escape:", (12., 279., 56., 8.), true),
    Field("/delimiters/4/escape", (72., 278., 144., 12.)),
    Label(24522, "Close:", (12., 299., 56., 8.), true),
    Field("/delimiters/4/close", (72., 297., 144., 12.)),
    Group(24551, "Delimiter 6 style", (245., 234., 212., 82.), false),
    Styler(21, 24573, (399., 242., 54., 14.)),
    Label(24570, "Open:", (251., 261., 56., 8.), true),
    Field("/delimiters/5/open", (309., 259., 144., 12.)),
    Label(24571, "Escape:", (251., 280., 56., 8.), true),
    Field("/delimiters/5/escape", (309., 278., 144., 12.)),
    Label(24572, "Close:", (251., 299., 56., 8.), true),
    Field("/delimiters/5/close", (309., 297., 144., 12.)),
    Group(24601, "Delimiter 7 style", (8., 320., 212., 82.), false),
    Styler(22, 24623, (162., 328., 54., 14.)),
    Label(24620, "Open:", (251., 347., 56., 8.), true),
    Field("/delimiters/6/open", (72., 345., 144., 12.)),
    Label(24621, "Escape:", (251., 365., 56., 8.), true),
    Field("/delimiters/6/escape", (72., 364., 144., 12.)),
    Label(24622, "Close:", (251., 384., 56., 8.), true),
    Field("/delimiters/6/close", (72., 383., 144., 12.)),
    Group(24651, "Delimiter 8 style", (245., 320., 212., 82.), false),
    Styler(23, 24673, (399., 328., 54., 14.)),
    Label(24670, "Open:", (12., 346., 56., 8.), true),
    Field("/delimiters/7/open", (309., 345., 144., 12.)),
    Label(24671, "Escape:", (12., 365., 56., 8.), true),
    Field("/delimiters/7/escape", (309., 364., 144., 12.)),
    Label(24672, "Close:", (12., 385., 56., 8.), true),
    Field("/delimiters/7/close", (309., 383., 144., 12.)),
];

fn source_field_id(control: &Control) -> Option<u32> {
    let (_, id) = localization::control_source(control)?;
    Some(match control.path.as_str() {
        path if path.starts_with("/folding/") || path.starts_with("/delimiters/") => id - 3,
        path if path.starts_with("/keywords/") => id + 19,
        path if path.starts_with("/comments/") => id - if path.contains("/line_") { 3 } else { 2 },
        path if path.starts_with("/numbers/") => id + 1,
        path if path.starts_with("/operators/") => id - 2,
        "/extensions" => 20008,
        _ => id,
    })
}

impl MatrixWindow {
    pub(super) fn udl_source_group(
        &self,
        path: &'static str,
        id: u32,
        fallback: &'static str,
        rect: Rect,
        centered: bool,
    ) -> AnyElement {
        let label = crate::localization::dialog_control(&self.native_locale(), path, id)
            .unwrap_or(fallback);
        let border = if self.dark { 0x686868 } else { 0xa0a0a0 };
        let background = if self.dark { 0x252525 } else { 0xf0f0f0 };
        let heading = div()
            .absolute()
            .top_0()
            .when(centered, |heading| heading.left_0().w_full().flex().justify_center())
            .when(!centered, |heading| heading.left(du(6.)))
            .child(div().px(du(3.)).bg(rgb(background)).child(label));
        at(rect)
            .id(gpui::SharedString::from(format!("udl-resource-{id}")))
            .role(Role::Group)
            .aria_label(label)
            .child(div().absolute().top(du(4.)).bottom_0().left_0().right_0()
                .border_1().border_color(rgb(border)))
            .child(heading)
            .into_any_element()
    }

    fn udl_source_enabled(&self, control: &Control) -> bool {
        self.udl.ready && (!self.udl.styler_visible || control.path.starts_with("/styles/"))
    }

    pub(super) fn udl_source_field(
        &mut self,
        control: Control,
        rect: Rect,
        multiline: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.udl.selected;
        let text = self.udl.registry.definition(selected)
            .map(|definition| control.text(definition)).unwrap_or_default();
        let label = localization::control_label(&control, &self.native_locale());
        let key = format!("{selected}:{}", control.path);
        let enabled = self.udl_source_enabled(&control);
        let height = du(rect.3).max(if multiline && rect.3 > 16. {
            dialog_font::line_height() * 2. + px(8.)
        } else {
            dialog_font::control_height()
        });
        if !self.udl.source_fields.contains_key(&key) {
            let field = cx.new(|cx| field::Field::new(label, "udl-source-field", multiline, cx));
            field.update(cx, |field, cx| {
                field.set_text(&text, cx);
                field.set_height(dialog_font::logical(height), cx);
                field.set_want_return(control.path.starts_with("/keywords/"), cx);
            });
            let bound_control = control.clone();
            let changed = cx.observe_in(&field, window, move |view, field, window, cx| {
                if view.udl.selected != selected
                    || !view.udl_source_enabled(&bound_control)
                    || field.read(cx).has_composition()
                {
                    return;
                }
                let text = field.read(cx).text();
                if view.udl.registry.definition(selected)
                    .is_some_and(|definition| bound_control.text(definition) != text)
                {
                    view.apply_udl_control(&bound_control, &text, window, cx);
                }
            });
            self.udl.source_fields.insert(key.clone(), SourceField { field, _changed: changed });
        }
        let mut element = at(rect).h(height)
            .id(gpui::SharedString::from(source_field_id(&control)
                .map(|id| format!("udl-resource-{id}"))
                .unwrap_or_else(|| format!("udl-source-{}", control.path))))
            .aria_label(label)
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .when(!enabled, |element| element.opacity(0.5))
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                if !enabled { cx.stop_propagation(); }
            });
        if let Some(bound) = self.udl.source_fields.get(&key) {
            bound.field.update(cx, |field, cx| {
                field.set_label(label, cx);
                field.set_height(dialog_font::logical(height), cx);
                if !field.has_composition() && field.text() != text {
                    field.set_text(&text, cx);
                }
            });
            element = element.child(bound.field.clone());
        }
        if !enabled {
            element = element.child(div().absolute().inset_0().occlude().on_mouse_down(
                gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation(),
            ));
        }
        element.into_any_element()
    }

    pub(super) fn udl_source_check(
        &self,
        control: Control,
        rect: Rect,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = localization::control_label(&control, &self.native_locale());
        let checked = self.udl.registry.definition(self.udl.selected)
            .is_some_and(|definition| control.text(definition) == "true");
        let enabled = self.udl_source_enabled(&control);
        let selected = self.udl.selected;
        let keyboard_control = control.clone();
        let id = localization::control_source(&control).map(|(_, id)| id);
        let palette = self.preferences_render_palette();
        at(rect)
            .id(gpui::SharedString::from(id.map(|id| format!("udl-resource-{id}"))
                .unwrap_or_else(|| format!("udl-source-{}-{}", control.path, control.label))))
            .role(Role::CheckBox)
            .aria_label(label)
            .aria_toggled(checked.into())
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .tab_index(if enabled { 0 } else { -1 })
            .cursor_pointer()
            .when(!enabled, |element| element.opacity(0.5))
            .flex().items_center().gap(du(2.))
            .child(preferences_surface::pref_indicator_palette(checked, false, enabled, palette))
            .child(label)
            .on_click(cx.listener(move |view, _, window, cx| {
                if view.udl.selected == selected && view.udl_source_enabled(&control) {
                    let checked = view.udl.registry.definition(selected)
                        .is_some_and(|definition| control.text(definition) == "true");
                    view.apply_udl_control(&control, if checked { "false" } else { "true" }, window, cx);
                }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "space" && view.udl.selected == selected
                    && view.udl_source_enabled(&keyboard_control)
                {
                    let checked = view.udl.registry.definition(selected)
                        .is_some_and(|definition| keyboard_control.text(definition) == "true");
                    view.apply_udl_control(&keyboard_control, if checked { "false" } else { "true" }, window, cx);
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }

    pub(super) fn udl_source_radio(
        &mut self,
        control: Control,
        index: usize,
        rect: Rect,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.native_locale();
        let source = localization::choice_source(&control, index);
        let fallback = match (control.path.as_str(), index) {
            ("/line_comment_placement", 0) => "Allow anywhere",
            ("/line_comment_placement", 1) => "Force at beginning of line",
            ("/line_comment_placement", 2) => "Allow preceding whitespace",
            ("/decimal_separator", 0) => "Dot",
            ("/decimal_separator", 1) => "Comma",
            ("/decimal_separator", 2) => "Both",
            _ => match &control.kind {
                Kind::Choice(options) => options.get(index).copied().unwrap_or(control.label),
                _ => control.label,
            },
        };
        let label = source.and_then(|(path, id)| crate::localization::dialog_control(&locale, path, id))
            .unwrap_or(fallback);
        let value = index.to_string();
        let chosen = self.udl.registry.definition(self.udl.selected)
            .is_some_and(|definition| control.text(definition) == value);
        let enabled = self.udl_source_enabled(&control);
        let selected = self.udl.selected;
        let keyboard_control = control.clone();
        let palette = self.preferences_render_palette();
        let focus = self.udl.radio_focus.entry(control.path.clone())
            .or_insert_with(|| std::array::from_fn(|_| cx.focus_handle())).clone();
        let click_focus = focus[index].clone();
        at(rect)
            .id(gpui::SharedString::from(source.map(|(_, id)| format!("udl-resource-{id}"))
                .unwrap_or_else(|| format!("udl-source-{}-{index}", control.path))))
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_toggled(chosen.into())
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .track_focus(&focus[index])
            .tab_index(if enabled && chosen { 0 } else { -1 })
            .cursor_pointer()
            .when(!enabled, |element| element.opacity(0.5))
            .flex().items_center().gap(du(2.))
            .child(preferences_surface::pref_indicator_palette(chosen, true, enabled, palette))
            .child(label)
            .on_click(cx.listener(move |view, _, window, cx| {
                if view.udl.selected == selected && view.udl_source_enabled(&control) {
                    view.apply_udl_control(&control, &index.to_string(), window, cx);
                    click_focus.focus(window, cx);
                }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if view.udl.selected != selected || !view.udl_source_enabled(&keyboard_control) { return; }
                let current = view.udl.registry.definition(selected)
                    .and_then(|definition| keyboard_control.text(definition).parse::<usize>().ok())
                    .filter(|value| *value < 3).unwrap_or(index);
                let target = match event.keystroke.key.as_str() {
                    "left" | "up" => (current + 2) % 3,
                    "right" | "down" => (current + 1) % 3,
                    "space" => index,
                    _ => return,
                };
                view.apply_udl_control(&keyboard_control, &target.to_string(), window, cx);
                focus[target].focus(window, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    pub(super) fn udl_source_styler_button(
        &self,
        style: usize,
        path: &'static str,
        id: u32,
        rect: Rect,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = crate::localization::dialog_control(&self.native_locale(), path, id).unwrap_or("Styler");
        let enabled = self.udl.ready && !self.udl.styler_visible;
        let selected = self.udl.selected;
        at(rect)
            .id(gpui::SharedString::from(format!("udl-resource-{id}")))
            .role(Role::Button)
            .h(dialog_font::control_height())
            .aria_label(label)
            .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
            .tab_index(if enabled { 0 } else { -1 })
            .border_1()
            .border_color(rgb(if self.dark { 0x707070 } else { 0x999999 }))
            .bg(rgb(if self.dark { 0x383838 } else { 0xf8f8f8 }))
            .flex().items_center().justify_center()
            .cursor_pointer()
            .when(!enabled, |element| element.opacity(0.5))
            .child(label)
            .on_click(cx.listener(move |view, _, window, cx| {
                if view.udl.ready && !view.udl.styler_visible && view.udl.selected == selected {
                    view.open_udl_styler(style, window, cx);
                }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "space" | "enter")
                    && view.udl.ready && !view.udl.styler_visible && view.udl.selected == selected
                {
                    view.open_udl_styler(style, window, cx);
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }

    pub(super) fn render_udl_source_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = match self.udl.page {
            Page::FolderDefault => FOLDER,
            Page::Keywords => KEYWORDS,
            Page::CommentsNumbers => COMMENTS,
            Page::OperatorsDelimiters => OPERATORS,
        };
        let path = localization::page_path(self.udl.page);
        let controls = udl_controls::controls(self.udl.page);
        let locale = self.native_locale();
        let mut page = div().relative().w(du(466.)).h(du(415.)).mx_auto().flex_shrink_0();
        for item in items {
            let element = match *item {
                Group(id, fallback, rect, centered) => self.udl_source_group(path, id, fallback, rect, centered),
                Label(id, fallback, rect, right) => {
                    let label = crate::localization::dialog_control(&locale, path, id).unwrap_or(fallback);
                    at(rect).id(gpui::SharedString::from(format!("udl-resource-{id}")))
                        .whitespace_nowrap()
                        .when(right, |element| element.text_right()).child(label).into_any_element()
                }
                Field(control_path, rect) => {
                    let Some(control) = controls.iter().find(|control| control.path == control_path) else { continue };
                    self.udl_source_field(control.clone(), rect, true, window, cx)
                }
                Check(control_path, rect) => {
                    let Some(control) = controls.iter().find(|control| control.path == control_path) else { continue };
                    self.udl_source_check(control.clone(), rect, cx)
                }
                Radio(control_path, index, rect) => {
                    let Some(control) = controls.iter().find(|control| control.path == control_path) else { continue };
                    self.udl_source_radio(control.clone(), index, rect, cx)
                }
                Styler(style, id, rect) => self.udl_source_styler_button(style, path, id, rect, cx),
                Link(rect) => {
                    let label = crate::localization::dialog_control(&locale, path, 21103)
                        .unwrap_or("User Defined Languages online help");
                    let enabled = self.udl.ready && !self.udl.styler_visible;
                    at(rect).id("udl-resource-21103").role(Role::Link).aria_label(label)
                        .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).tab_index(if enabled { 0 } else { -1 })
                        .text_center().text_color(rgb(if self.dark { 0x80bfff } else { 0x0000ff }))
                        .cursor_pointer().child(label)
                        .on_click(cx.listener(|view, _, _, cx| {
                            if view.udl.ready && !view.udl.styler_visible {
                                cx.open_url("https://npp-user-manual.org/docs/user-defined-language-system/");
                            }
                        }))
                        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                            if event.keystroke.key == "enter" && view.udl.ready && !view.udl.styler_visible {
                                cx.open_url("https://npp-user-manual.org/docs/user-defined-language-system/");
                                cx.stop_propagation();
                            }
                        }))
                        .into_any_element()
                }
            };
            page = page.child(element);
        }
        page.into_any_element()
    }
}
