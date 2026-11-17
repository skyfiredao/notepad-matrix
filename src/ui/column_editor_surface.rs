use super::*;
use super::dialog_font::px;
use crate::editor::{ColumnBase, ColumnFill, ColumnNumbers};

mod numbers;

fn dialog_units(window: &Window) -> (f32, f32) {
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let system = window.text_system();
    let family = dialog_font::font_family();
    let font = system.resolve_font(&gpui::font(family.clone()));
    let width = system.shape_line(alphabet.into(), px(11.), &[gpui::TextRun {
        len: alphabet.len(), font: gpui::font(family), color: rgb(0).into(),
        background_color: None, underline: None, strikethrough: None,
    }], None).width();
    let average = ((dialog_font::logical(width).floor() / 26.).floor() + 1.) / 2.;
    let height = dialog_font::logical(system.ascent(font, px(11.)) + system.descent(font, px(11.))).ceil();
    (average.floor() / 4., height / 8.)
}

pub(super) fn dialog_dimensions(window: &Window) -> gpui::Size<gpui::Pixels> {
    let (x, y) = dialog_units(window);
    size(px(220. * x), px(214. * y))
}

gpui::actions!(
    column_editor,
    [
        ConfirmColumnEditor,
        CloseColumnEditor,
        NextColumnControl,
        PreviousColumnControl,
        ActivateColumnControl,
        PreviousColumnChoice,
        NextColumnChoice
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = column_editor, no_json)]
pub(super) struct ColumnControl(usize);

pub(super) fn bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("escape", CloseColumnEditor, Some("MatrixColumnEditor")),
        KeyBinding::new("enter", ConfirmColumnEditor, Some("MatrixColumnEditor")),
        KeyBinding::new("tab", NextColumnControl, Some("MatrixColumnEditor")),
        KeyBinding::new(
            "shift-tab",
            PreviousColumnControl,
            Some("MatrixColumnEditor"),
        ),
        KeyBinding::new("space", ActivateColumnControl, Some("MatrixColumnButton")),
        KeyBinding::new("left", PreviousColumnChoice, Some("MatrixColumnButton")),
        KeyBinding::new("up", PreviousColumnChoice, Some("MatrixColumnButton")),
        KeyBinding::new("right", NextColumnChoice, Some("MatrixColumnButton")),
        KeyBinding::new("down", NextColumnChoice, Some("MatrixColumnButton")),
    ];
    bindings.extend([
        ("alt-t", 4), ("alt-n", 5), ("alt-d", 6), ("alt-h", 7), ("alt-o", 8),
        ("alt-b", 9), ("alt-i", 1), ("alt-y", 2), ("alt-r", 3), ("alt-l", 14),
    ].into_iter().map(|(key, control)| KeyBinding::new(key, ColumnControl(control), Some("MatrixColumnEditor"))));
    bindings
}

pub(super) struct ColumnEditorState {
    pub(super) visible: bool,
    initialized: bool,
    fields: [gpui::Entity<text_field::TextField>; 4],
    controls: [FocusHandle; 12],
    numbers: bool,
    base: ColumnBase,
    hex_uppercase: bool,
    leading_open: bool,
    hex_open: bool,
    fill: ColumnFill,
    focus: usize,
    owner: Option<(crate::workspace::BufferId, crate::workspace::ViewId)>,
    error: Option<String>,
    error_control: Option<usize>,
    observed_text: [String; 4],
    numeric_values: [Option<u64>; 3],
    field_observers: Vec<gpui::Subscription>,
    validation_flash: Option<gpui::Task<()>>,
    validation_balloon: Option<gpui::Task<()>>,
}

impl ColumnEditorState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            initialized: false,
            fields: [
                cx.new(|cx| text_field::TextField::compact("插入文本", "column-text-input", cx).without_history_arrow()),
                cx.new(|cx| text_field::TextField::compact("初始值", "column-start-input", cx).without_history_arrow()),
                cx.new(|cx| text_field::TextField::compact("增量", "column-step-input", cx).without_history_arrow()),
                cx.new(|cx| text_field::TextField::compact("重复次数", "column-repeat-input", cx).without_history_arrow()),
            ],
            controls: std::array::from_fn(|_| cx.focus_handle()),
            numbers: true,
            base: ColumnBase::Decimal,
            hex_uppercase: false,
            leading_open: false,
            hex_open: false,
            fill: ColumnFill::None,
            focus: 0,
            owner: None,
            error: None,
            error_control: None,
            observed_text: std::array::from_fn(|_| String::new()),
            numeric_values: [None; 3],
            field_observers: Vec::new(),
            validation_flash: None,
            validation_balloon: None,
        }
    }
}

impl MatrixWindow {
    fn column_mnemonic(&mut self, control: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.column_editor.visible || !matches!(control, 1..=9 | 14) { return; }
        if !matches!(control, 4 | 5) && !self.column_editor.numbers { return; }
        self.column_editor.leading_open = false;
        self.column_editor.hex_open = false;
        self.column_editor.focus = control;
        if control < 4 {
            self.column_editor.fields[control].update(cx, |field, cx| field.focus(window, cx));
        } else {
            self.column_editor.controls[control - 4].focus(window, cx);
            if control != 14 { self.activate_column_control(window, cx); }
        }
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn show_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        if !self.column_editor.initialized {
            let settings = match self.settings_store.as_ref().map(ConfigStore::load_settings).transpose() {
                Ok(settings) => settings.unwrap_or_default(),
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            let settings = &settings["ColumnEditor"];
            self.column_editor.numbers = settings["choice"].as_str() != Some("text");
            let number = &settings["number"];
            self.column_editor.base = match number["formatChoice"].as_str() {
                Some("hex") => ColumnBase::Hexadecimal,
                Some("hexuc") => ColumnBase::HexadecimalUpper,
                Some("oct") => ColumnBase::Octal,
                Some("bin") => ColumnBase::Binary,
                _ => ColumnBase::Decimal,
            };
            self.column_editor.hex_uppercase = self.column_editor.base == ColumnBase::HexadecimalUpper;
            self.column_editor.fill = match number["leadingChoice"].as_str() {
                Some("zeros") => ColumnFill::Zero,
                Some("spaces") => ColumnFill::Space,
                _ => ColumnFill::None,
            };
            let text = settings["text"]["content"].as_str().unwrap_or_default();
            self.column_editor.fields[0].update(cx, |field, cx| field.set_text(text, window, cx));
            for (slot, name) in ["initial", "increase", "repeat"].into_iter().enumerate() {
                let value = number[name].as_i64().filter(|&value| value >= 0 && value <= i64::from(i32::MAX)).map(|value| value as u64);
                self.column_editor.numeric_values[slot] = value;
                let text = value.map(|value| numbers::format_value(value, column_radix(self.column_editor.base), self.column_editor.hex_uppercase)).unwrap_or_default();
                self.column_editor.fields[slot + 1].update(cx, |field, cx| field.set_text(&text, window, cx));
            }
            for (slot, field) in self.column_editor.fields.iter().enumerate() {
                self.column_editor.observed_text[slot] = field.read(cx).text();
            }
            self.column_editor.initialized = true;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.column_editor.visible = true;
        self.column_editor.owner = Some((self.workspace.active().id(), self.pane));
        self.column_editor.error = None;
        self.present_floating(floating_windows::DialogKind::ColumnEditor, window, cx);
        cx.notify();
    }

    pub(super) fn focus_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.column_editor.leading_open = false;
        self.column_editor.hex_open = false;
        if self.column_editor.field_observers.is_empty() {
            for (index, field) in self.column_editor.fields.clone().into_iter().enumerate() {
                let observer = cx.observe_in(&field, window, move |view, field, window, cx| {
                    let text = field.read(cx).text();
                    if text != view.column_editor.observed_text[index] {
                        view.column_editor.observed_text[index] = text.clone();
                        view.column_field_changed(index, &text, window, cx);
                    }
                });
                self.column_editor.field_observers.push(observer);
            }
        }
        for (index, field) in self.column_editor.fields.iter().enumerate() {
            let enabled = (index != 0) == self.column_editor.numbers;
            field.update(cx, |field, cx| field.set_enabled(enabled, cx));
        }
        let index = if self.column_editor.numbers { 1 } else { 0 };
        self.column_editor.focus = index;
        self.column_editor.fields[index].update(cx, |field, cx| field.focus(window, cx));
    }

    pub(super) fn dismiss_column_editor(&mut self, cx: &mut Context<Self>) {
        if self.column_editor.visible {
            let text = self.column_editor.fields[0].read(cx).text();
            let text = String::from_utf16_lossy(&text.split('\0').next().unwrap_or_default().encode_utf16().take(259).collect::<Vec<_>>());
            let numbers = self.column_editor.numeric_values.map(|value| value.map_or(-1, |value| i64::from(value as u32 as i32)));
            let settings = serde_json::json!({
                "choice": if self.column_editor.numbers { "number" } else { "text" },
                "text": { "content": text },
                "number": {
                    "initial": numbers[0], "increase": numbers[1], "repeat": numbers[2],
                    "formatChoice": match self.column_editor.base { ColumnBase::Decimal => "dec", ColumnBase::Hexadecimal => "hex", ColumnBase::HexadecimalUpper => "hexuc", ColumnBase::Octal => "oct", ColumnBase::Binary => "bin" },
                    "leadingChoice": match self.column_editor.fill { ColumnFill::None => "none", ColumnFill::Zero => "zeros", ColumnFill::Space => "spaces" },
                }
            });
            if let Some(store) = &self.settings_store
                && let Err(error) = store.update_settings(|document| { document["ColumnEditor"] = settings; Ok(()) }) {
                self.notify_editor_error(error.to_string(), cx);
            }
        }
        self.column_editor.visible = false;
        self.column_editor.owner = None;
        self.column_editor.error = None;
        self.column_editor.leading_open = false;
        self.column_editor.hex_open = false;
        self.column_editor.field_observers.clear();
        self.column_editor.validation_flash = None;
        self.column_editor.validation_balloon = None;
        self.column_editor.error_control = None;
        for field in &self.column_editor.fields {
            field.update(cx, |field, cx| field.set_search_failure(false, cx));
        }
        cx.notify();
    }

    pub(super) fn cancel_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.column_editor.error_control.is_some() {
            self.column_editor.error = None;
            self.column_editor.error_control = None;
            self.column_editor.validation_balloon = None;
            cx.notify();
            return;
        }
        self.dismiss_floating(floating_windows::DialogKind::ColumnEditor, window, cx);
    }

    pub(super) fn confirm_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.confirm_column_editor(window, cx)
        }) {
            return;
        }
        if !self.column_editor.visible
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.column_editor.owner != Some((self.workspace.active().id(), self.pane)) {
            self.column_editor.error = Some("The original document and view must be active".into());
            cx.notify();
            return;
        }
        if self
            .column_editor
            .fields
            .iter()
            .any(|field| field.read(cx).has_composition())
        {
            self.column_editor.error =
                Some("Commit the dialog input composition before inserting".into());
            cx.notify();
            return;
        }
        let numbers = if self.column_editor.numbers {
            let values = [1, 2, 3].map(|index| self.column_editor.fields[index].read(cx).text());
            match parse_numbers(&values, self.column_editor.base, self.column_editor.fill) {
                Ok(options) => Some(options),
                Err(error) => {
                    self.column_editor.error = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else { None };
        let text = self.column_editor.fields[0].read(cx).text();
        let text = String::from_utf16_lossy(&text.encode_utf16().take(1023).collect::<Vec<_>>());
        if numbers.is_none() && text.is_empty() { return; }
        self.dismiss_floating(floating_windows::DialogKind::ColumnEditor, window, cx);
        self.editor_mut().begin_undo_action();
        let result = if let Some(options) = numbers {
            self.editor_mut().column_numbers(options, 4)
        } else { self.editor_mut().column_text(&text, 4) };
        self.editor_mut().end_undo_action();
        self.editor_changed(result, window, cx);
    }

    fn column_focus_order(&self) -> Vec<usize> {
        let mut order = vec![4, 5];
        if self.column_editor.numbers {
            order.extend([1, 2, 3, 6, 7, 8, 9]);
            if matches!(
                self.column_editor.base,
                ColumnBase::Hexadecimal | ColumnBase::HexadecimalUpper
            ) {
                order.push(15);
            }
            order.push(14);
        } else {
            order.push(0);
        }
        order.extend([12, 13]);
        order
    }

    fn move_column_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.column_editor.leading_open = false;
        self.column_editor.hex_open = false;
        let order = self.column_focus_order();
        let current = self
            .column_editor
            .controls
            .iter()
            .position(|focus| focus.is_focused(window))
            .map(|index| index + 4)
            .or_else(|| {
                self.column_editor
                    .fields
                    .iter()
                    .position(|field| field.read(cx).is_focused(window))
            })
            .unwrap_or(self.column_editor.focus);
        let index = order.iter().position(|&item| item == current).unwrap_or(0);
        let next = order[(index + if forward { 1 } else { order.len() - 1 }) % order.len()];
        self.column_editor.focus = next;
        if next < 4 {
            self.column_editor.fields[next].update(cx, |field, cx| field.focus(window, cx));
        } else {
            self.column_editor.controls[next - 4].focus(window, cx);
        }
        cx.notify();
    }

    fn activate_column_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .column_editor
            .controls
            .iter()
            .position(|focus| focus.is_focused(window))
            .map(|index| index + 4);
        match current {
            Some(4) => {
                self.column_editor.numbers = false;
                self.focus_column_editor(window, cx);
            }
            Some(5) => {
                self.column_editor.numbers = true;
                self.focus_column_editor(window, cx);
            }
            Some(6) => self.set_column_base(ColumnBase::Decimal, window, cx),
            Some(7) => self.set_column_base(
                if self.column_editor.hex_uppercase {
                    ColumnBase::HexadecimalUpper
                } else {
                    ColumnBase::Hexadecimal
                },
                window,
                cx,
            ),
            Some(8) => self.set_column_base(ColumnBase::Octal, window, cx),
            Some(9) => self.set_column_base(ColumnBase::Binary, window, cx),
            Some(10) => self.column_editor.fill = ColumnFill::Zero,
            Some(11) => self.column_editor.fill = ColumnFill::Space,
            Some(12) => self.confirm_column_editor(window, cx),
            Some(13) => self.cancel_column_editor(window, cx),
            Some(14) => self.column_editor.leading_open = !self.column_editor.leading_open,
            Some(15) => {
                self.column_editor.hex_open = !self.column_editor.hex_open;
            }
            _ => {}
        }
        cx.notify();
    }

    fn cycle_column_choice(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .column_editor
            .controls
            .iter()
            .position(|focus| focus.is_focused(window))
            .map(|index| index + 4);
        let choices: &[usize] = match current {
            Some(4 | 5) => &[4, 5],
            Some(6..=9) => &[6, 7, 8, 9],
            Some(14) => {
                let choices = [ColumnFill::None, ColumnFill::Zero, ColumnFill::Space];
                let current = choices.iter().position(|fill| *fill == self.column_editor.fill).unwrap_or(0);
                self.column_editor.fill = choices[(current + if forward { 1 } else { 2 }) % 3];
                cx.notify();
                return;
            }
            Some(12 | 13) => &[12, 13],
            Some(15) => {
                let base = if self.column_editor.hex_uppercase { ColumnBase::Hexadecimal } else { ColumnBase::HexadecimalUpper };
                self.set_column_base(base, window, cx);
                self.column_editor.hex_uppercase = self.column_editor.base == ColumnBase::HexadecimalUpper;
                cx.notify();
                return;
            }
            _ => return,
        };
        let index = choices
            .iter()
            .position(|item| Some(*item) == current)
            .unwrap_or(0);
        let next = choices[(index + if forward { 1 } else { choices.len() - 1 }) % choices.len()];
        self.column_editor.controls[next - 4].focus(window, cx);
        self.column_editor.focus = next;
        if !matches!(next, 12 | 13) {
            self.activate_column_control(window, cx);
        }
        cx.notify();
    }

    fn set_column_base(&mut self, base: ColumnBase, window: &mut Window, cx: &mut Context<Self>) {
        let radix = column_radix(self.column_editor.base);
        let mut values = Vec::new();
        for field in &self.column_editor.fields[1..] {
            if field.read(cx).has_composition() {
                self.column_editor.error =
                    Some("Commit the numeric input composition before changing format".into());
                return;
            }
            let text = field.read(cx).text();
            match numbers::parse_value(&text, radix) {
                Ok(value) => values.push((text.is_empty(), value)),
                Err(error) => {
                    self.column_editor.error = Some(error.into());
                    return;
                }
            }
        }
        self.column_editor.base = base;
        self.column_editor.error = None;
        for (index, (empty, value)) in values.into_iter().enumerate() {
            if !empty {
                let text = numbers::format_value(
                    value,
                    column_radix(base),
                    base == ColumnBase::HexadecimalUpper,
                );
                self.column_editor.fields[index + 1]
                    .update(cx, |field, cx| field.set_text(&text, window, cx));
            }
        }
    }

    fn column_field_changed(&mut self, index: usize, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if index == 0 { cx.notify(); return; }
        let text = String::from_utf16_lossy(&text.split('\0').next().unwrap_or_default().encode_utf16().take(259).collect::<Vec<_>>());
        if text.is_empty() { self.column_editor.numeric_values[index - 1] = None; return; }
        let radix = column_radix(self.column_editor.base);
        if let Ok(value) = numbers::parse_value(&text, radix) {
            self.column_editor.numeric_values[index - 1] = Some(value);
            return;
        }
        for (slot, value) in self.column_editor.numeric_values.into_iter().enumerate() {
            let formatted = value.map(|value| numbers::format_value(value, radix, self.column_editor.hex_uppercase)).unwrap_or_default();
            self.column_editor.fields[slot + 1].update(cx, |field, cx| field.set_text(&formatted, window, cx));
        }
        let note = match radix {
            16 => "Hex numbers use 0-9, A-F!",
            8 => "Oct numbers only use 0-7!",
            2 => "Bin numbers only use 0-1!",
            _ => "Decimal numbers only use 0-9!",
        };
        self.column_editor.error = Some(format!("Invalid Numeric Entry\nEntered string \"{text}\":\n{note}"));
        self.column_editor.error_control = Some(index);
        for (slot, field) in self.column_editor.fields.iter().enumerate() {
            field.update(cx, |field, cx| field.set_search_failure(slot == index, cx));
        }
        let executor = cx.background_executor().clone();
        self.column_editor.validation_flash = Some(cx.spawn_in(window, async move |view, cx| {
            executor.timer(std::time::Duration::from_millis(250)).await;
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                for field in &view.column_editor.fields {
                    field.update(cx, |field, cx| field.set_search_failure(false, cx));
                }
            }) { eprintln!("column validation flash could not update its window: {error}"); }
        }));
        let executor = cx.background_executor().clone();
        self.column_editor.validation_balloon = Some(cx.spawn_in(window, async move |view, cx| {
            executor.timer(std::time::Duration::from_millis(3500)).await;
            if let Err(error) = view.update_in(cx, |view, _, cx| {
                view.column_editor.error = None;
                view.column_editor.error_control = None;
                cx.notify();
            }) { eprintln!("column validation balloon could not update its window: {error}"); }
        }));
        cx.notify();
    }

    pub(super) fn render_column_editor(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.column_editor.visible {
            return None;
        }
        let (unit_x, unit_y) = dialog_units(window);
        for field in &self.column_editor.fields {
            field.update(cx, |field, cx| field.set_compact_height(12. * unit_y, cx));
        }
        let confirm_enabled = self.column_editor.numbers || !self.column_editor.fields[0].read(cx).text().is_empty();
        let control = |index: usize, id: &'static str, label: &'static str, selected: bool| {
            let enabled = match index {
                6..=9 | 14 => self.column_editor.numbers,
                15 => self.column_editor.numbers && matches!(self.column_editor.base, ColumnBase::Hexadecimal | ColumnBase::HexadecimalUpper),
                12 => confirm_enabled,
                _ => true,
            };
            let (left, top, width, height) = match index {
                4 => (13., 6., 124., 10.),
                5 => (13., 68., 204., 10.),
                6 => (27., 99., 50., 10.),
                7 => (110., 99., 50., 10.),
                8 => (27., 114., 50., 10.),
                9 => (110., 114., 50., 10.),
                12 => (142., 18., 70., 14.),
                13 => (142., 36., 70., 14.),
                14 => (90., 189., 100., 12.),
                15 => (150., 97., 40., 12.),
                _ => (0., 0., 0., 0.),
            };
            let boxed = matches!(index, 12..=15);
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(if matches!(index, 12 | 13) {
                    Role::Button
                } else if matches!(index, 14 | 15) {
                    Role::ComboBox
                } else {
                    Role::RadioButton
                })
                .when(matches!(index, 4..=9), |control| {
                    control.aria_toggled(selected.into())
                })
                .when(matches!(index, 14 | 15), |control| control.aria_expanded(
                    if index == 14 { self.column_editor.leading_open } else { self.column_editor.hex_open },
                ))
                .aria_label(label)
                .a11y_synthetic_children(move |builder| {
                    if !enabled { builder.parent_node().set_disabled(); }
                })
                .track_focus(&self.column_editor.controls[index - 4].clone().tab_stop(enabled))
                .key_context("MatrixColumnButton")
                .absolute()
                .left(px(left * unit_x))
                .top(px(top * unit_y))
                .w(px(width * unit_x))
                .h(px(height * unit_y))
                .flex()
                .items_center()
                .gap(px(4.))
                .whitespace_nowrap()
                .when(boxed, |control| control.border_1().px(px(4.)))
                .when(matches!(index, 12 | 13), |control| control.justify_center())
                .cursor_pointer()
                .border_color(rgb(
                    if self.column_editor.controls[index - 4].is_focused(window) {
                        0x0067c0
                    } else {
                        0x808080
                    },
                ))
                .bg(rgb(if matches!(index, 14 | 15) && enabled { 0xffffff } else { 0xf0f0f0 }))
                .when(!enabled, |control| control.text_color(rgb(0x808080)))
                .when(matches!(index, 4..=9), |control| control.child(
                    div().size(px(12.)).flex_shrink_0().rounded_full().border_1()
                        .border_color(rgb(if enabled { 0x606060 } else { 0xa0a0a0 }))
                        .flex().items_center().justify_center()
                        .when(selected, |radio| radio.child(div().size(px(6.)).rounded_full().bg(rgb(if enabled { 0x202020 } else { 0xa0a0a0 })))),
                ))
                .child(div().flex_1().child(label))
                .when(matches!(index, 14 | 15), |control| control.child("▾"))
                .when(enabled, |control| control.on_click(cx.listener(move |view, _, window, cx| {
                    view.column_editor.controls[index - 4].focus(window, cx);
                    view.column_editor.focus = index;
                    view.activate_column_control(window, cx);
                })))
        };
        let numbers = self.column_editor.numbers;
        let mut dialog = div()
            .id("column-editor-dialog")
            .debug_selector(|| "column-editor-dialog".into())
            .role(Role::Dialog)
            .aria_label("列编辑器")
            .font_family(dialog_font::font_family())
            .text_size(px(11.))
            .key_context("MatrixColumnEditor")
            .w_full()
            .h_full()
            .relative()
            .occlude()
            .bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|view, _: &ConfirmColumnEditor, window, cx| {
                if view.column_editor.leading_open || view.column_editor.hex_open {
                    view.column_editor.leading_open = false;
                    view.column_editor.hex_open = false;
                    cx.notify();
                } else { view.confirm_column_editor(window, cx); }
            }))
            .on_action(cx.listener(|view, _: &CloseColumnEditor, window, cx| {
                if view.column_editor.leading_open || view.column_editor.hex_open {
                    view.column_editor.leading_open = false;
                    view.column_editor.hex_open = false;
                    cx.notify();
                } else { view.cancel_column_editor(window, cx); }
            }))
            .on_action(cx.listener(|view, _: &NextColumnControl, window, cx| {
                view.move_column_focus(true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &PreviousColumnControl, window, cx| {
                view.move_column_focus(false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &ActivateColumnControl, window, cx| {
                view.activate_column_control(window, cx)
            }))
            .on_action(cx.listener(|view, _: &PreviousColumnChoice, window, cx| {
                view.cycle_column_choice(false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &NextColumnChoice, window, cx| {
                view.cycle_column_choice(true, window, cx)
            }))
            .on_action(cx.listener(|view, action: &ColumnControl, window, cx| {
                view.column_mnemonic(action.0, window, cx)
            }))
            .children([(8., 14., 124., 46.), (8., 77., 204., 130.), (16., 86., 188., 44.)].into_iter().map(|(x, y, width, height)| {
                div().absolute().left(px(x * unit_x)).top(px(y * unit_y))
                    .w(px(width * unit_x)).h(px(height * unit_y)).border_1().border_color(rgb(0xa0a0a0))
            }))
            .child(control(4, "column-mode-text", "插入文本(T)", !numbers))
            .child(control(5, "column-mode-numbers", "插入数字(N)", numbers))
            .child(control(6, "column-base-decimal", "十进制(D)", self.column_editor.base == ColumnBase::Decimal))
            .child(control(7, "column-base-hex", "十六进制(H)", matches!(self.column_editor.base, ColumnBase::Hexadecimal | ColumnBase::HexadecimalUpper)))
            .child(control(8, "column-base-octal", "八进制(O)", self.column_editor.base == ColumnBase::Octal))
            .child(control(9, "column-base-binary", "二进制(B)", self.column_editor.base == ColumnBase::Binary))
            .child(control(15, "column-hex-case", if self.column_editor.hex_uppercase { "A-F" } else { "a-f" }, false))
            .child(control(14, "column-leading", match self.column_editor.fill { ColumnFill::None => "无", ColumnFill::Zero => "补零", ColumnFill::Space => "补空格" }, false))
            .child(control(12, "column-editor-confirm", "确定", false))
            .child(control(13, "column-editor-cancel", "取消", false))
            .child(div().absolute().left(px(94. * unit_x)).top(px(82. * unit_y))
                .px(px(3.)).bg(rgb(0xf0f0f0)).text_color(rgb(if numbers { 0x202020 } else { 0x808080 })).child("格式"));
        for (index, x, y, width) in [(0, 20., 32., 97.), (1, 90., 138., 38.), (2, 90., 155., 38.), (3, 90., 172., 38.)] {
            dialog = dialog.child(div().absolute().left(px(x * unit_x)).top(px(y * unit_y))
                .w(px(width * unit_x)).h(px(12. * unit_y)).overflow_hidden().child(self.column_editor.fields[index].clone()));
        }
        for (y, label) in [(140., "初始值(I):"), (157., "增量(Y):"), (174., "重复次数(R):"), (190., "开头补齐(L):")] {
            dialog = dialog.child(div().absolute().left(px(10. * unit_x)).top(px(y * unit_y))
                .w(px(76. * unit_x)).h(px(8. * unit_y)).flex().justify_end().whitespace_nowrap()
                .text_color(rgb(if numbers { 0x202020 } else { 0x808080 })).child(label));
        }
        if let Some(error) = &self.column_editor.error {
            dialog = dialog.child(
                div()
                    .id("column-editor-error")
                    .debug_selector(|| "column-editor-error".into())
                    .role(Role::Alert)
                    .absolute().left(px(8. * unit_x))
                    .top(px(match self.column_editor.error_control { Some(1) => 98., Some(2) => 115., Some(3) => 132., _ => 47. } * unit_y))
                    .w(px(204. * unit_x)).p(px(6.)).border_1().bg(rgb(0xfff6d8)).occlude()
                    .text_color(rgb(0x800000))
                    .child(error.clone()),
            );
        }
        if self.column_editor.leading_open && numbers {
            dialog = dialog.child(div().id("column-leading-options").absolute()
                .left(px(90. * unit_x)).top(px(153. * unit_y)).w(px(100. * unit_x))
                .border_1().bg(rgb(0xffffff)).occlude().children(
                    [(ColumnFill::None, "无"), (ColumnFill::Zero, "补零"), (ColumnFill::Space, "补空格")].into_iter().enumerate().map(|(index, (fill, label))| {
                        div().id(("column-leading-option", index)).role(Role::MenuItem)
                            .h(px(12. * unit_y)).px(px(4.)).child(label)
                            .when(fill == self.column_editor.fill, |item| item.bg(rgb(0xcce4f7)))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.column_editor.fill = fill;
                                view.column_editor.leading_open = false;
                                cx.notify();
                                cx.stop_propagation();
                            }))
                    }),
                ));
        }
        if self.column_editor.hex_open && numbers {
            dialog = dialog.child(div().id("column-hex-options").absolute()
                .left(px(150. * unit_x)).top(px(109. * unit_y)).w(px(40. * unit_x))
                .border_1().bg(rgb(0xffffff)).occlude().children(
                    [(ColumnBase::Hexadecimal, "a-f"), (ColumnBase::HexadecimalUpper, "A-F")].into_iter().enumerate().map(|(index, (base, label))| {
                        div().id(("column-hex-option", index)).role(Role::MenuItem)
                            .h(px(12. * unit_y)).px(px(4.)).child(label)
                            .when(base == self.column_editor.base, |item| item.bg(rgb(0xcce4f7)))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.set_column_base(base, window, cx);
                                view.column_editor.hex_uppercase = view.column_editor.base == ColumnBase::HexadecimalUpper;
                                view.column_editor.hex_open = false;
                                cx.notify();
                                cx.stop_propagation();
                            }))
                    }),
                ));
        }
        Some(dialog.into_any_element())
    }
}

fn parse_numbers(
    values: &[String; 3],
    base: ColumnBase,
    fill: ColumnFill,
) -> Result<ColumnNumbers, String> {
    let values = values.each_ref().map(|value| String::from_utf16_lossy(&value.encode_utf16().take(1023).collect::<Vec<_>>()));
    let radix = column_radix(base);
    let start = numbers::parse_value(&values[0], radix)
        .map_err(|error| format!("Initial number: {error}"))?;
    let step =
        numbers::parse_value(&values[1], radix).map_err(|error| format!("Increase by: {error}"))?;
    let repeat =
        numbers::parse_value(&values[2], radix).map_err(|error| format!("Repeat: {error}"))?;
    let repeat = usize::try_from(repeat)
        .map_err(|_| "Repeat is out of range")?
        .max(1);
    Ok(ColumnNumbers {
        start,
        step,
        repeat,
        base,
        fill,
    })
}

fn column_radix(base: ColumnBase) -> u32 {
    match base {
        ColumnBase::Decimal => 10,
        ColumnBase::Hexadecimal | ColumnBase::HexadecimalUpper => 16,
        ColumnBase::Octal => 8,
        ColumnBase::Binary => 2,
    }
}
