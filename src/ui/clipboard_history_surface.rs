use super::*;

pub(super) struct State {
    pub visible: bool,
    watching: bool,
    generation: isize,
    entries: Vec<String>,
    selected: usize,
    focus: FocusHandle,
    context: bool,
}

impl State {
    pub fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            watching: false,
            generation: -1,
            entries: Vec::new(),
            selected: 0,
            focus: cx.focus_handle(),
            context: false,
        }
    }
}

impl MatrixWindow {
    pub(super) fn toggle_clipboard_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::panel_layout::PanelId;
        if self.clipboard_history.visible {
            self.hide_panel(PanelId::Clipboard, window, cx);
        } else {
            self.show_panel(PanelId::Clipboard, window, cx);
        }
        cx.notify();
    }

    pub(super) fn ensure_clipboard_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clipboard_history.watching {
            self.clipboard_history.watching = true;
            self.capture_clipboard_history(cx);
            let executor = cx.background_executor().clone();
            cx.spawn_in(window, async move |view, cx| {
                loop {
                    executor.timer(std::time::Duration::from_millis(200)).await;
                    if view
                        .update_in(cx, |view, _, cx| view.capture_clipboard_history(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    fn capture_clipboard_history(&mut self, cx: &mut Context<Self>) {
        let board = clipboard_surface::pasteboard();
        let generation = board.changeCount();
        if generation == self.clipboard_history.generation {
            return;
        }
        self.clipboard_history.generation = generation;
        let Some(payload) = crate::native_binary_clipboard::read(&board) else {
            return;
        };
        let Ok(text) = String::from_utf8(payload.bytes) else {
            return;
        };
        if text.is_empty() || self.clipboard_history.entries.first() == Some(&text) {
            return;
        }
        if let Some(index) = self
            .clipboard_history
            .entries
            .iter()
            .position(|entry| entry == &text)
        {
            self.clipboard_history.entries.remove(index);
        }
        self.clipboard_history.entries.insert(0, text);
        self.clipboard_history.selected = 0;
        cx.notify();
    }

    fn paste_clipboard_history(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.paste_clipboard_history(index, window, cx)
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
        if let Some(text) = self.clipboard_history.entries.get(index).cloned() {
            let result = self.editor_mut().insert_text(&text);
            self.editor_changed(result, window, cx);
            self.editor_focus.focus(window, cx);
        }
    }

    pub(super) fn render_clipboard_history(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.clipboard_history.visible {
            return None;
        }
        Some(
            div()
                .id("clipboard-history")
                .debug_selector(|| "clipboard-history".into())
                .role(Role::List)
                .aria_label("剪贴板历史")
                .track_focus(&self.clipboard_history.focus)
                .size_full()
                .relative()
                .bg(rgb(0xffffff))
                .text_color(rgb(0x202020))
                .text_size(px(13.))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    let length = view.clipboard_history.entries.len();
                    match event.keystroke.key.as_str() {
                        "enter" => view.paste_clipboard_history(
                            view.clipboard_history.selected,
                            window,
                            cx,
                        ),
                        "delete" | "backspace" if length > 0 => {
                            view.clipboard_history
                                .entries
                                .remove(view.clipboard_history.selected.min(length - 1));
                            view.clipboard_history.selected = view
                                .clipboard_history
                                .selected
                                .min(length.saturating_sub(2));
                        }
                        "up" => {
                            view.clipboard_history.selected =
                                view.clipboard_history.selected.saturating_sub(1)
                        }
                        "down" => {
                            view.clipboard_history.selected =
                                (view.clipboard_history.selected + 1).min(length.saturating_sub(1))
                        }
                        _ => return,
                    }
                    cx.notify();
                    cx.stop_propagation();
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|view, _, _, cx| {
                        view.clipboard_history.context = true;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .id("clipboard-history-entries")
                        .size_full()
                        .overflow_y_scroll()
                        .children(self.clipboard_history.entries.iter().enumerate().map(
                            |(index, text)| {
                                let mut units = 0;
                                let mut caption: String = text
                                    .chars()
                                    .take_while(|ch| {
                                        units += ch.len_utf16();
                                        units <= 256
                                    })
                                    .collect();
                                if caption.len() < text.len() {
                                    caption.push_str("...");
                                }
                                div()
                                    .id(("clipboard-entry", index))
                                    .role(Role::ListItem)
                                    .aria_label(caption.clone())
                                    .aria_selected(self.clipboard_history.selected == index)
                                    .h(px(24.))
                                    .px(px(6.))
                                    .truncate()
                                    .bg(rgb(if self.clipboard_history.selected == index {
                                        0xcce8ff
                                    } else {
                                        0xffffff
                                    }))
                                    .child(caption)
                                    .on_click(cx.listener(
                                        move |view, event: &gpui::ClickEvent, window, cx| {
                                            view.clipboard_history.selected = index;
                                            view.clipboard_history.focus.focus(window, cx);
                                            if event.click_count() >= 2 {
                                                view.paste_clipboard_history(index, window, cx);
                                            }
                                            cx.notify();
                                        },
                                    ))
                            },
                        )),
                )
                .when(self.clipboard_history.context, |element| {
                    element.child(
                        div()
                            .absolute()
                            .top(px(4.))
                            .right(px(4.))
                            .w(px(130.))
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0xadadad))
                            .occlude()
                            .child(
                                div()
                                    .id("clipboard-history-clear")
                                    .role(Role::MenuItem)
                                    .aria_label("清除所有历史")
                                    .h(px(24.))
                                    .child("清除所有历史")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.clipboard_history.entries.clear();
                                        view.clipboard_history.selected = 0;
                                        view.clipboard_history.context = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                })
                .into_any_element(),
        )
    }
}
