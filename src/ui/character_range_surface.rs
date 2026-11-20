use super::*;
use crate::localization;
use super::dialog_font::px;

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
struct RangeOption(pub u32);

pub(super) struct State {
    pub visible: bool,
    mode: usize,
    low: gpui::Entity<text_field::TextField>,
    high: gpui::Entity<text_field::TextField>,
    initialized: bool,
    backwards: bool,
    wrap: bool,
    mode_focus: [FocusHandle; 3],
    direction_focus: [FocusHandle; 2],
}

impl State {
    pub fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            mode: 0,
            low: cx.new(|cx| text_field::TextField::compact("范围起点", "character-range-low", cx).without_history_arrow().with_tab_index(1)),
            high: cx
                .new(|cx| text_field::TextField::compact("范围终点", "character-range-high", cx).without_history_arrow().with_tab_index(2)),
            initialized: false,
            backwards: false,
            wrap: false,
            mode_focus: std::array::from_fn(|index| cx.focus_handle().tab_index(0).tab_stop(index == 0)),
            direction_focus: std::array::from_fn(|index| cx.focus_handle().tab_index(3).tab_stop(index == 1)),
        }
    }
}

impl MatrixWindow {
    pub(super) fn open_character_range(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.character_range.initialized {
            self.character_range.initialized = true;
            self.character_range
                .low
                .update(cx, |field, cx| { field.set_text("0", window, cx); field.set_enabled(false, cx); });
            self.character_range
                .high
                .update(cx, |field, cx| { field.set_text("255", window, cx); field.set_enabled(false, cx); });
        }
        self.character_range.visible = true;
        self.present_floating(floating_windows::DialogKind::CharacterRange, window, cx);
    }
    pub(super) fn find_character_range(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.find_character_range(window, cx)
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let limits = match self.character_range.mode {
            0 => Some((128, 255)),
            1 => Some((0, 127)),
            _ => self
                .character_range
                .low
                .read(cx)
                .text()
                .trim_start_matches(' ')
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u8>()
                .ok()
                .zip(self.character_range.high.read(cx).text().trim_start_matches(' ').chars()
                    .take_while(char::is_ascii_digit).collect::<String>().parse::<u8>().ok()),
        };
        let Some((low, high)) = limits.filter(|(low, high)| low <= high) else {
            let locale = self.native_locale();
            let response = window.prompt(gpui::PromptLevel::Warning,
                localization::dialog_title(&locale, "FindCharRangeValueError").unwrap_or("Range Value problem"),
                Some(localization::dialog_message(&locale, "FindCharRangeValueError").unwrap_or("You should type between 0 and 255.")),
                &[gpui::PromptButton::ok("确定")], cx);
            cx.spawn_in(window, async move |_, _| {
                if let Err(error) = response.await { eprintln!("character range warning failed: {error}"); }
            }).detach();
            return;
        };
        let bytes = self.editor().document().byte_snapshot();
        let caret = self.editor().document().selection().head.0;
        if caret > bytes.len() { return; }
        let backwards = self.character_range.backwards;
        let matches = |index: &usize| bytes.byte(*index).is_some_and(|byte| (low..=high).contains(&byte));
        let found = if backwards {
            (0..caret).rev().find(matches)
        } else {
            (caret..bytes.len()).find(matches)
        }
        .or_else(|| {
            if self.character_range.wrap {
                if backwards {
                    (0..bytes.len()).rev().find(matches)
                } else {
                    (0..bytes.len()).find(matches)
                }
            } else {
                None
            }
        });
        if let Some(found) = found {
            let line = match bytes.line_index(found) {
                Ok(line) => line,
                Err(error) => { self.notify_editor_error(error.to_string(), cx); return; }
            };
            self.ensure_editor_line_visible(line);
            if let Err(error) = self.editor_mut().select(Selection { anchor: ByteOffset(found), head: ByteOffset(found) }) {
                self.editor_changed(Err(error), window, cx);
                return;
            }
            let selection = if backwards {
                Selection {
                    anchor: ByteOffset(found + 1),
                    head: ByteOffset(found),
                }
            } else {
                Selection {
                    anchor: ByteOffset(found),
                    head: ByteOffset(found + 1),
                }
            };
            let result = self.editor_mut().select(selection);
            self.editor_changed(result, window, cx);
        }
        cx.notify();
    }

    pub(super) fn render_character_range(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("character-range-dialog")
            .role(Role::Dialog)
            .aria_label(localization::dialog_title(&self.native_locale(), "FindCharsInRange").unwrap_or("Find Characters in Range..."))
            .key_context("MatrixCharacterRange")
            .on_action(cx.listener(|view, action: &RangeOption, window, cx| {
                view.character_range_option(action.0, window, cx);
                cx.stop_propagation();
            }))
            .size_full()
            .relative()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x202020))
            .text_size(px(13.))
            .children(
                [
                    (0, "range-non-ascii", "Non-ASCII characters (128–255)"),
                    (1, "range-ascii", "ASCII characters (0–127)"),
                    (2, "range-custom", "Custom range (0–255):"),
                ]
                .into_iter()
                .map(|(index, id, label)| {
                    find_surface::radio(id, localization::current_dialog_control("FindCharsInRange", 2901 + index as u32).unwrap_or(label), self.character_range.mode == index)
                        .track_focus(&self.character_range.mode_focus[index].clone().tab_stop(self.character_range.mode == index))
                        .absolute()
                        .left(px(12.))
                        .top(px(10. + index as f32 * 26.))
                        .on_click(cx.listener(move |view, _, window, cx| view.character_range_option(2901 + index as u32, window, cx)))
                        .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                            let target = match event.keystroke.key.as_str() {
                                "space" => index,
                                "down" | "right" => (index + 1) % 3,
                                "up" | "left" => (index + 2) % 3,
                                _ => return,
                            };
                            view.character_range_option(2901 + target as u32, window, cx);
                            cx.stop_propagation();
                        }))
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(210.))
                    .top(px(61.))
                    .flex()
                    .gap(px(5.))
                    .child(div().w(px(35.)).child(self.character_range.low.clone()))
                    .child(div().w(px(15.)).text_center()
                        .text_color(rgb(if self.character_range.mode == 2 { 0x202020 } else { 0x808080 })).child("–"))
                    .child(div().w(px(35.)).child(self.character_range.high.clone())),
            )
            .child(
                div().absolute().left(px(12.)).top(px(98.)).w(px(130.)).h(px(63.))
                    .border_1().border_color(rgb(0xd6d6d6))
                    .child(div().absolute().left(px(6.)).top(px(-9.)).bg(rgb(0xffffff)).child(localization::current_dialog_control("FindCharsInRange", 2908).unwrap_or("Direction")))
                    .children([("range-up", "Up", true), ("range-down", "Down", false)]
                        .into_iter().enumerate().map(|(index, (id, label, backward))| {
                            let selected = self.character_range.backwards == backward;
                            find_surface::radio(id, localization::current_dialog_control("FindCharsInRange", 2906 + index as u32).unwrap_or(label), selected)
                                .track_focus(&self.character_range.direction_focus[index].clone().tab_stop(selected))
                                .absolute().left(px(11.)).top(px(21. + index as f32 * 21.))
                                .on_click(cx.listener(move |view, _, window, cx| view.character_range_option(2906 + index as u32, window, cx)))
                                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                                    let target = match event.keystroke.key.as_str() {
                                        "space" => index,
                                        "left" | "right" | "up" | "down" => 1 - index,
                                        _ => return,
                                    };
                                    view.character_range_option(2906 + target as u32, window, cx);
                                    cx.stop_propagation();
                                }))
                        })),
            )
            .child(
                find_surface::checkbox("range-wrap", localization::current_dialog_control("FindCharsInRange", 2909).unwrap_or("Wrap around"), self.character_range.wrap)
                    .focusable().tab_index(4)
                    .absolute()
                    .left(px(150.))
                    .top(px(140.))
                    .on_click(cx.listener(|view, _, window, cx| view.character_range_option(2909, window, cx)))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "space" {
                            view.character_range_option(2909, window, cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(
                find_surface::source_button("range-find-next", localization::current_dialog_control("FindCharsInRange", 2910).unwrap_or("Find"), 10.)
                    .right(px(12.)).w(px(123.))
                    .on_click(
                        cx.listener(|view, _, window, cx| view.find_character_range(window, cx)),
                    ),
            )
            .child(
                find_surface::source_button("range-close", localization::current_dialog_control("FindCharsInRange", 2).unwrap_or("Close"), 42.)
                    .right(px(12.)).w(px(123.))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.dismiss_floating(
                            floating_windows::DialogKind::CharacterRange,
                            window,
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }

    fn character_range_option(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            2901..=2903 => {
                let index = (id - 2901) as usize;
                self.character_range.mode = index;
                self.character_range.low.update(cx, |field, cx| field.set_enabled(index == 2, cx));
                self.character_range.high.update(cx, |field, cx| field.set_enabled(index == 2, cx));
                self.character_range.mode_focus[index].focus(window, cx);
            }
            2906 | 2907 => {
                self.character_range.backwards = id == 2906;
                self.character_range.direction_focus[(id - 2906) as usize].focus(window, cx);
            }
            2909 => self.character_range.wrap = !self.character_range.wrap,
            _ => return,
        }
        cx.notify();
    }
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    let mut keys = [("alt-n", 2901), ("alt-a", 2902), ("alt-r", 2903), ("alt-u", 2906), ("alt-d", 2907), ("alt-p", 2909)]
        .into_iter().map(|(key, id)| KeyBinding::new(key, RangeOption(id), Some("MatrixCharacterRange"))).collect::<Vec<_>>();
    keys.extend([
        KeyBinding::new("enter", FindDialogNext, Some("MatrixCharacterRange")),
        KeyBinding::new("alt-f", find_surface::FindButtonAction("range-find-next", false), Some("MatrixCharacterRange")),
        KeyBinding::new("alt-c", find_surface::FindButtonAction("range-close", false), Some("MatrixCharacterRange")),
        KeyBinding::new("escape", CloseWindow, Some("MatrixCharacterRange")),
    ]);
    keys
}
