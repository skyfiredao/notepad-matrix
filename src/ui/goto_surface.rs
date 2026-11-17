use super::*;
use super::dialog_font::px;

pub(super) fn dimensions() -> gpui::Size<gpui::Pixels> {
    size(px(452.), px(132.))
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
struct GoToMode(pub bool);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
struct GoToLocate;

pub(super) struct State {
    offset: bool,
    current: gpui::Entity<text_field::TextField>,
    limit: usize,
    mode_focus: [FocusHandle; 2],
}

impl State {
    pub fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            offset: false,
            current: cx.new(|cx| text_field::TextField::compact("当前位置", "goto-current", cx)
                .without_history_arrow().with_tab_index(1).with_read_only()),
            limit: 0,
            mode_focus: std::array::from_fn(|index| cx.focus_handle().tab_index(0).tab_stop(index == 0)),
        }
    }
}

fn goto_number(raw: &str) -> Option<i64> {
    if raw.is_empty() { return None; }
    let bytes = raw.as_bytes();
    let bytes = &bytes[..bytes.len().min(255)];
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() { index += 1; }
    let negative = bytes.get(index) == Some(&b'-');
    if matches!(bytes.get(index), Some(b'+' | b'-')) { index += 1; }
    let limit = if negative { 1u64 << 63 } else { i64::MAX as u64 };
    let mut value = 0u64;
    while let Some(byte) = bytes.get(index).filter(|byte| byte.is_ascii_digit()) {
        value = value.saturating_mul(10).saturating_add(u64::from(*byte - b'0')).min(limit);
        index += 1;
    }
    let value = if negative {
        if value == 1u64 << 63 { i64::MIN } else { -(value as i64) }
    } else { value as i64 };
    (value != -1).then_some(value)
}

impl MatrixWindow {
    pub(super) fn show_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if !self.goto_visible {
            self.goto_input.update(cx, |field, cx| field.set_text("", window, cx));
        }
        self.update_goto_numbers(window, cx);
        self.goto_visible = true;
        self.present_floating(floating_windows::DialogKind::GoTo, window, cx);
        cx.notify();
    }

    fn update_goto_numbers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let document = self.editor().document();
        let bytes = document.byte_snapshot();
        let caret = document.selection().head.0;
        let current = if self.goto.offset {
            self.goto.limit = bytes.len().saturating_sub(1);
            caret
        } else {
            let current = match bytes.line_index(caret) {
                Ok(line) => line + 1,
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            self.goto.limit = bytes.line_count();
            current
        };
        self.goto.current.update(cx, |field, cx| field.set_text(&current.to_string(), window, cx));
        cx.notify();
    }

    fn set_goto_mode(&mut self, offset: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.goto.offset = offset;
        self.goto.mode_focus[usize::from(offset)].focus(window, cx);
        self.update_goto_numbers(window, cx);
    }

    pub(super) fn clear_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_visible = false;
        self.goto_input.update(cx, |field, cx| field.set_text("", window, cx));
        cx.notify();
    }

    pub(super) fn confirm_goto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.confirm_goto(window, cx)) {
            return;
        }
        if !self.goto_visible || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() {
            return;
        }
        let raw = self.goto_input.read(cx).text();
        let Some(number) = goto_number(&raw) else {
            self.restore_document_focus(cx);
            return;
        };
        let bytes = self.editor().document().byte_snapshot();
        let target = if self.goto.offset {
            if number <= 0 { Ok(0) } else {
                let before = usize::try_from(number - 1).unwrap_or(usize::MAX).min(bytes.len());
                bytes.move_position_outside_char(before, false, true)
                    .and_then(|before| bytes.adjacent_position(before, true))
            }
        } else {
            let row = usize::try_from(number.saturating_sub(1).max(0)).unwrap_or(usize::MAX);
            self.record_macro_step(crate::macros::MacroStep::GoToLine(row));
            if row >= bytes.line_count() { Ok(bytes.len()) } else { bytes.line_start(row) }
        };
        let target = match target {
            Ok(target) => target,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        let line = match bytes.line_index(target) {
            Ok(line) => line,
            Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
        };
        self.clear_goto(window, cx);
        self.ensure_editor_line_visible(line);
        let result = self.editor_mut().select(Selection { anchor: ByteOffset(target), head: ByteOffset(target) });
        self.editor_changed(result, window, cx);
        self.restore_document_focus(cx);
    }

    pub(super) fn render_goto(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.goto_visible { return None; }
        Some(div()
            .id("goto-dialog")
            .debug_selector(|| "goto-dialog".into())
            .role(Role::Dialog)
            .aria_label("转至...")
            .key_context("MatrixGoTo")
            .tab_group()
            .on_action(cx.listener(|view, action: &GoToMode, window, cx| {
                view.set_goto_mode(action.0, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmGoTo, window, cx| {
                view.confirm_goto(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &GoToLocate, window, cx| {
                view.confirm_goto(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseGoTo, window, cx| {
                view.clear_goto(window, cx);
                view.dismiss_floating(floating_windows::DialogKind::GoTo, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| {
                window.focus_next(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                window.focus_prev(cx);
                cx.stop_propagation();
            }))
            .size_full().relative().bg(rgb(0xffffff)).text_color(rgb(0x202020)).font_family(dialog_font::font_family()).text_size(px(13.))
            .children([(false, "goto-line", "行(L)", 14.), (true, "goto-offset", "偏移量(O)", 172.)]
                .into_iter().map(|(offset, id, label, left)| {
                    let index = usize::from(offset);
                    find_surface::radio(id, label, self.goto.offset == offset)
                        .track_focus(&self.goto.mode_focus[index].clone().tab_stop(self.goto.offset == offset))
                        .absolute().left(px(left)).top(px(9.)).w(px(140.))
                        .on_click(cx.listener(move |view, _, window, cx| view.set_goto_mode(offset, window, cx)))
                        .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                            let target = match event.keystroke.key.as_str() {
                                "space" => offset,
                                "left" | "right" | "up" | "down" => !offset,
                                _ => return,
                            };
                            view.set_goto_mode(target, window, cx);
                            cx.stop_propagation();
                        }))
                }))
            .child(div().absolute().left(px(14.)).top(px(44.)).child("当前位置:"))
            .child(div().absolute().left(px(182.)).top(px(42.)).w(px(124.)).child(self.goto.current.clone()))
            .child(div().absolute().left(px(14.)).top(px(70.)).child("目标位置:"))
            .child(div().absolute().left(px(182.)).top(px(67.)).w(px(124.)).child(self.goto_input.clone()))
            .child(div().absolute().left(px(14.)).top(px(96.)).child("截止位置:"))
            .child(div().id("goto-limit").absolute().left(px(186.)).top(px(96.)).child(self.goto.limit.to_string()))
            .child(find_surface::button("goto-confirm", "定位(G)", 65.).right(px(12.)).w(px(123.)).h(px(25.))
                .focusable().tab_index(3)
                .on_click(cx.listener(|view, _, window, cx| view.confirm_goto(window, cx)))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "space" { view.confirm_goto(window, cx); cx.stop_propagation(); }
                })))
            .child(find_surface::button("goto-cancel", "关闭(C)", 96.).right(px(12.)).w(px(123.)).h(px(25.))
                .focusable().tab_index(4)
                .on_click(cx.listener(|view, _, window, cx| {
                    view.clear_goto(window, cx);
                    view.dismiss_floating(floating_windows::DialogKind::GoTo, window, cx);
                }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "space" {
                        view.clear_goto(window, cx);
                        view.dismiss_floating(floating_windows::DialogKind::GoTo, window, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_action(cx.listener(|view, _: &ConfirmGoTo, window, cx| {
                    view.clear_goto(window, cx);
                    view.dismiss_floating(floating_windows::DialogKind::GoTo, window, cx);
                    cx.stop_propagation();
                })))
            .into_any_element())
    }
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("alt-l", GoToMode(false), Some("MatrixGoTo")),
        KeyBinding::new("alt-o", GoToMode(true), Some("MatrixGoTo")),
        KeyBinding::new("alt-g", GoToLocate, Some("MatrixGoTo")),
        KeyBinding::new("alt-c", CloseGoTo, Some("MatrixGoTo")),
        KeyBinding::new("tab", DialogFocusNext, Some("MatrixGoTo")),
        KeyBinding::new("shift-tab", DialogFocusPrevious, Some("MatrixGoTo")),
    ]
}
