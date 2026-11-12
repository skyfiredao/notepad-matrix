use super::*;
use crate::editor::{ColumnBase, ColumnFill, ColumnNumbers};

mod numbers;

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

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
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
    ]
}

pub(super) struct ColumnEditorState {
    pub(super) visible: bool,
    fields: [gpui::Entity<text_field::TextField>; 4],
    controls: [FocusHandle; 12],
    numbers: bool,
    base: ColumnBase,
    hex_uppercase: bool,
    fill: ColumnFill,
    focus: usize,
    owner: Option<(crate::workspace::BufferId, crate::workspace::ViewId)>,
    error: Option<String>,
}

impl ColumnEditorState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            fields: [
                cx.new(|cx| text_field::TextField::new("Text to insert", "column-text-input", cx)),
                cx.new(|cx| text_field::TextField::new("Initial number", "column-start-input", cx)),
                cx.new(|cx| text_field::TextField::new("Increase by", "column-step-input", cx)),
                cx.new(|cx| text_field::TextField::new("Repeat", "column-repeat-input", cx)),
            ],
            controls: std::array::from_fn(|_| cx.focus_handle()),
            numbers: false,
            base: ColumnBase::Decimal,
            hex_uppercase: false,
            fill: ColumnFill::None,
            focus: 0,
            owner: None,
            error: None,
        }
    }
}

impl MatrixWindow {
    pub(super) fn show_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().is_read_only() {
            self.notify_editor_error("Document is read-only".into(), cx);
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.column_editor.visible = true;
        self.column_editor.owner = Some((self.workspace.active().id(), self.pane));
        self.column_editor.error = None;
        for (index, value) in [(1, "1"), (2, "1"), (3, "1")] {
            if self.column_editor.fields[index].read(cx).text().is_empty() {
                self.column_editor.fields[index]
                    .update(cx, |field, cx| field.set_text(value, window, cx));
            }
        }
        self.present_floating(floating_windows::DialogKind::ColumnEditor, window, cx);
        cx.notify();
    }

    pub(super) fn focus_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = if self.column_editor.numbers { 1 } else { 0 };
        self.column_editor.focus = index;
        self.column_editor.fields[index].update(cx, |field, cx| field.focus(window, cx));
    }

    pub(super) fn dismiss_column_editor(&mut self, cx: &mut Context<Self>) {
        self.column_editor.visible = false;
        self.column_editor.owner = None;
        self.column_editor.error = None;
        cx.notify();
    }

    pub(super) fn cancel_column_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let result = if self.column_editor.numbers {
            let values = [1, 2, 3].map(|index| self.column_editor.fields[index].read(cx).text());
            match parse_numbers(&values, self.column_editor.base, self.column_editor.fill) {
                Ok(options) => self.editor_mut().column_numbers(options, 4),
                Err(error) => {
                    self.column_editor.error = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else {
            let text = self.column_editor.fields[0].read(cx).text();
            if text.is_empty() {
                return;
            }
            self.editor_mut().column_text(&text, 4)
        };
        if let Err(error) = &result {
            self.column_editor.error = Some(error.to_string());
        }
        let succeeded = result.is_ok();
        self.editor_changed(result, window, cx);
        if succeeded {
            self.cancel_column_editor(window, cx);
        }
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
            order.extend([14, 10, 11]);
        } else {
            order.push(0);
        }
        order.extend([12, 13]);
        order
    }

    fn move_column_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
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
            Some(14) => self.column_editor.fill = ColumnFill::None,
            Some(15) => {
                let base = if self.column_editor.hex_uppercase {
                    ColumnBase::Hexadecimal
                } else {
                    ColumnBase::HexadecimalUpper
                };
                self.set_column_base(base, window, cx);
                self.column_editor.hex_uppercase =
                    self.column_editor.base == ColumnBase::HexadecimalUpper;
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
            Some(10 | 11 | 14) => &[14, 10, 11],
            Some(12 | 13) => &[12, 13],
            Some(15) => {
                self.activate_column_control(window, cx);
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

    pub(super) fn render_column_editor(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.column_editor.visible {
            return None;
        }
        let control = |index: usize, id: &'static str, label: &'static str, selected: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(if matches!(index, 12 | 13) {
                    Role::Button
                } else if index == 15 {
                    Role::CheckBox
                } else {
                    Role::RadioButton
                })
                .when(!matches!(index, 12 | 13), |control| {
                    control.aria_toggled(selected.into())
                })
                .aria_label(label)
                .track_focus(&self.column_editor.controls[index - 4])
                .key_context("MatrixColumnButton")
                .px(px(8.))
                .py(px(4.))
                .border_1()
                .cursor_pointer()
                .border_color(rgb(
                    if self.column_editor.controls[index - 4].is_focused(window) {
                        0x0067c0
                    } else {
                        0x808080
                    },
                ))
                .bg(rgb(if selected { 0xcce4f7 } else { 0xf0f0f0 }))
                .child(label)
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.column_editor.controls[index - 4].focus(window, cx);
                    view.column_editor.focus = index;
                    view.activate_column_control(window, cx);
                }))
        };
        let numbers = self.column_editor.numbers;
        let mut dialog = div()
            .id("column-editor-dialog")
            .debug_selector(|| "column-editor-dialog".into())
            .role(Role::Dialog)
            .aria_label("Column Editor")
            .key_context("MatrixColumnEditor")
            .w_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .occlude()
            .bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|view, _: &ConfirmColumnEditor, window, cx| {
                view.confirm_column_editor(window, cx)
            }))
            .on_action(cx.listener(|view, _: &CloseColumnEditor, window, cx| {
                view.cancel_column_editor(window, cx)
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
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(control(4, "column-mode-text", "Text to Insert", !numbers))
                    .child(control(
                        5,
                        "column-mode-numbers",
                        "Number to Insert",
                        numbers,
                    )),
            );
        if numbers {
            for (index, label) in [(1, "Initial number"), (2, "Increase by"), (3, "Repeat")] {
                dialog = dialog.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().w(px(110.)).flex_shrink_0().child(label))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .child(self.column_editor.fields[index].clone()),
                        ),
                );
            }
            dialog = dialog
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(control(
                            6,
                            "column-base-decimal",
                            "Dec",
                            self.column_editor.base == ColumnBase::Decimal,
                        ))
                        .child(control(
                            7,
                            "column-base-hex",
                            "Hex",
                            matches!(
                                self.column_editor.base,
                                ColumnBase::Hexadecimal | ColumnBase::HexadecimalUpper
                            ),
                        ))
                        .child(control(
                            8,
                            "column-base-octal",
                            "Oct",
                            self.column_editor.base == ColumnBase::Octal,
                        ))
                        .child(control(
                            9,
                            "column-base-binary",
                            "Bin",
                            self.column_editor.base == ColumnBase::Binary,
                        )),
                )
                .when(
                    matches!(
                        self.column_editor.base,
                        ColumnBase::Hexadecimal | ColumnBase::HexadecimalUpper
                    ),
                    |dialog| {
                        dialog.child(control(
                            15,
                            "column-hex-case",
                            if self.column_editor.hex_uppercase {
                                "A-F"
                            } else {
                                "a-f"
                            },
                            self.column_editor.hex_uppercase,
                        ))
                    },
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child("Leading:")
                        .child(control(
                            14,
                            "column-fill-none",
                            "None",
                            self.column_editor.fill == ColumnFill::None,
                        ))
                        .child(control(
                            10,
                            "column-fill-zero",
                            "Zeros",
                            self.column_editor.fill == ColumnFill::Zero,
                        ))
                        .child(control(
                            11,
                            "column-fill-space",
                            "Spaces",
                            self.column_editor.fill == ColumnFill::Space,
                        )),
                );
        } else {
            dialog = dialog.child(self.column_editor.fields[0].clone());
        }
        if let Some(error) = &self.column_editor.error {
            dialog = dialog.child(
                div()
                    .id("column-editor-error")
                    .debug_selector(|| "column-editor-error".into())
                    .role(Role::Alert)
                    .text_color(rgb(0x800000))
                    .child(error.clone()),
            );
        }
        Some(
            dialog
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(control(12, "column-editor-confirm", "OK", false))
                        .child(control(13, "column-editor-cancel", "Cancel", false)),
                )
                .into_any_element(),
        )
    }
}

fn parse_numbers(
    values: &[String; 3],
    base: ColumnBase,
    fill: ColumnFill,
) -> Result<ColumnNumbers, String> {
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

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    fn dialog_context(cx: &mut gpui::VisualTestContext) -> gpui::VisualTestContext {
        cx.run_until_parked();
        for handle in cx.windows() {
            let mut dialog = gpui::VisualTestContext::from_window(handle, &cx.cx);
            if dialog.debug_bounds("matrix-editor").is_none()
                && dialog.debug_bounds("column-editor-dialog").is_some()
            {
                return dialog;
            }
        }
        panic!("Column Editor must be a separate native window");
    }

    fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let bounds = cx.debug_bounds(selector).expect("Column Editor control");
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    }

    #[gpui::test]
    fn floating_column_text_inserts_into_every_target_in_one_undo(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab\nx\n");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorRight);
        view.update_in(cx, |view, window, cx| view.show_column_editor(window, cx));
        let mut dialog = dialog_context(cx);
        click(&mut dialog, "column-text-input");
        dialog.simulate_input("中");
        click(&mut dialog, "column-editor-confirm");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "a中b\nx中\n 中"
        );
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab\nx\n"
        );
    }

    #[gpui::test]
    fn floating_column_numbers_reject_invalid_repeat_then_normalize_zero_and_close(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("a\nb\nc");
        cx.dispatch_action(EditorDocumentStart);
        view.update_in(cx, |view, window, cx| view.show_column_editor(window, cx));
        let mut dialog = dialog_context(cx);
        click(&mut dialog, "column-mode-numbers");
        click(&mut dialog, "column-repeat-input");
        dialog.dispatch_action(EditorSelectAll);
        dialog.simulate_input("bad");
        click(&mut dialog, "column-editor-confirm");
        assert!(dialog.debug_bounds("column-editor-error").is_some());
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "a\nb\nc"
        );
        click(&mut dialog, "column-repeat-input");
        dialog.dispatch_action(EditorSelectAll);
        dialog.simulate_input("0");
        click(&mut dialog, "column-editor-confirm");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "1a\n2b\n3c"
        );
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "a\nb\nc"
        );
    }

    #[gpui::test]
    fn floating_column_hex_case_and_none_fill_use_source_numeric_fields(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("a\nb\nc");
        cx.dispatch_action(EditorDocumentStart);
        view.update_in(cx, |view, window, cx| view.show_column_editor(window, cx));
        let mut dialog = dialog_context(cx);
        click(&mut dialog, "column-mode-numbers");
        click(&mut dialog, "column-start-input");
        dialog.dispatch_action(EditorSelectAll);
        dialog.simulate_input("15");
        click(&mut dialog, "column-base-hex");
        click(&mut dialog, "column-hex-case");
        click(&mut dialog, "column-fill-none");
        click(&mut dialog, "column-editor-confirm");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "F a\n10b\n11c"
        );
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "a\nb\nc"
        );
    }
}
