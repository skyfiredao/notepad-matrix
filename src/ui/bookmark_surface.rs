use std::sync::{Arc, OnceLock};

use super::*;
use gpui::{fill, point};

#[derive(Default)]
pub(super) struct HistoryState {
    next_session: Option<u8>,
    runtime_flags: u8,
    stopped: bool,
    warning_given: bool,
}

impl HistoryState {
    fn initialize(&mut self, settings: &serde_json::Value) {
        let state = settings["ScintillaPrimaryView"]["isChangeHistoryEnabled"].as_u64().unwrap_or(1).min(3) as u8;
        self.next_session = Some(state);
        self.runtime_flags = if state == 0 { 0 } else { 1 | (state << 1) };
        self.stopped = state == 0;
    }
}

fn fold_art(
    style: &str,
    marker: Option<bool>,
    above: bool,
    below: bool,
    tail: bool,
    foreground: u32,
    background: u32,
) -> AnyElement {
    let style = style.to_owned();
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let x = bounds.center().x.floor();
            let y = bounds.center().y.floor();
            let ink = rgb(background);
            let tree = matches!(style.as_str(), "circle" | "box");
            if tree {
                if above {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(x, bounds.top()),
                            size(px(1.), (y - bounds.top()).max(px(0.))),
                        ),
                        ink,
                    ));
                }
                if below {
                    window.paint_quad(fill(
                        Bounds::new(point(x, y), size(px(1.), (bounds.bottom() - y).max(px(0.)))),
                        ink,
                    ));
                }
                if tail {
                    if style == "circle" {
                        window.paint_quad(fill(
                            Bounds::new(point(x, y - px(2.)), size(px(1.), px(1.))),
                            ink,
                        ));
                        window.paint_quad(fill(
                            Bounds::new(point(x + px(1.), y - px(1.)), size(px(1.), px(1.))),
                            ink,
                        ));
                        window.paint_quad(fill(
                            Bounds::new(
                                point(x + px(2.), y),
                                size((bounds.right() - x - px(2.)).max(px(0.)), px(1.)),
                            ),
                            ink,
                        ));
                    } else {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(x, y),
                                size((bounds.right() - x).max(px(0.)), px(1.)),
                            ),
                            ink,
                        ));
                    }
                }
            }
            if let Some(collapsed) = marker {
                if tree {
                    let dimension = (f32::from(bounds.size.width)
                        .min(f32::from(bounds.size.height) - 2.)
                        .floor()
                        - 1.) as i32;
                    if dimension <= 0 {
                        return;
                    }
                    let dimension = if dimension % 2 == 0 {
                        dimension - 1
                    } else {
                        dimension
                    };
                    let half = (dimension as f32 / 2.).round();
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(x - px(half), y - px(half)),
                                size(px(dimension as f32), px(dimension as f32)),
                            ),
                            rgb(foreground),
                        )
                        .border_widths(px(1.))
                        .border_color(ink)
                        .corner_radii(px(if style == "circle" {
                            dimension as f32 / 2.
                        } else {
                            0.
                        })),
                    );
                }
                if style == "arrow" {
                    for step in 0..4 {
                        let step = step as f32;
                        let rectangle = if collapsed {
                            Bounds::new(
                                point(x - px(2.) + px(step), y - px(3.) + px(step)),
                                size(px(1.), px(7. - step * 2.)),
                            )
                        } else {
                            Bounds::new(
                                point(x - px(3.) + px(step), y - px(2.) + px(step)),
                                size(px(7. - step * 2.), px(1.)),
                            )
                        };
                        window.paint_quad(fill(rectangle, ink));
                    }
                } else {
                    window.paint_quad(fill(
                        Bounds::new(point(x - px(2.), y), size(px(5.), px(1.))),
                        ink,
                    ));
                    if collapsed {
                        window.paint_quad(fill(
                            Bounds::new(point(x, y - px(2.)), size(px(1.), px(5.))),
                            ink,
                        ));
                    }
                }
            }
        },
    )
    .size_full()
    .into_any_element()
}

fn bookmark_image() -> Arc<gpui::Image> {
    static IMAGE: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                include_bytes!("../../assets/notepad-plus-plus/bookmark14.png").to_vec(),
            ))
        })
        .clone()
}

pub(super) fn change_history_style(edition: u8, view: &MatrixWindow, attribute: &str) -> (u32, &'static str) {
    let (name, style, fallback) = match edition {
        1 => ("revert-origin", "Change History revert origin", 0x40a0bf),
        2 => ("saved", "Change History saved", 0x00a000),
        3 => ("modified", "Change History modified", 0xff8000),
        _ => (
            "revert-modified",
            "Change History revert modified",
            0xa0c000,
        ),
    };
    (view.style_colour(style, attribute, fallback), name)
}

impl MatrixWindow {
    pub(super) fn initialize_change_history(&mut self, settings: &serde_json::Value) {
        if self.preferences.history.next_session.is_none() {
            self.preferences.history.initialize(settings);
        }
        let enabled = self.preferences.history.runtime_flags & 1 != 0;
        for buffer in self.workspace.buffers() {
            buffer.editor.document().set_change_history_capture(enabled);
        }
    }

    pub(super) fn initialize_buffer_change_history(&self, document: &crate::editor::Document) {
        document.set_change_history_capture(self.preferences.history.runtime_flags & 1 != 0);
    }

    pub(super) fn initialize_active_change_history(&self) {
        self.initialize_buffer_change_history(self.editor().document());
    }

    pub(super) fn change_history_runtime_flags(&self) -> u8 {
        self.preferences.history.runtime_flags
    }

    pub(super) fn configure_change_history(
        &mut self,
        settings: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preferences.history.next_session.is_none() {
            self.initialize_change_history(settings);
            return;
        }
        let next = settings["ScintillaPrimaryView"]["isChangeHistoryEnabled"].as_u64().unwrap_or(1).min(3) as u8;
        let previous = self.preferences.history.next_session.replace(next);
        if previous == Some(next) {
            return;
        }
        if previous == Some(0) && next != 0 {
            if !self.preferences.history.warning_given {
                self.preferences.history.warning_given = true;
                let response = window.prompt(
                    gpui::PromptLevel::Info,
                    "需要重新启动 Notepad Matrix",
                    Some("必须重新启动 Notepad Matrix 才能启用变更历史。"),
                    &[gpui::PromptButton::ok("确定")],
                    cx,
                );
                cx.spawn_in(window, async move |view, cx| {
                    if let Err(error) = response.await {
                        if let Err(error) = view.update_in(cx, |view, _, cx| {
                            view.notify_editor_error(error.to_string(), cx);
                        }) {
                            eprintln!("Could not report change history warning: {error}");
                        }
                    }
                }).detach();
            }
            return;
        }
        if self.preferences.history.stopped {
            return;
        }
        self.preferences.history.runtime_flags = if next == 0 { 0 } else { 1 | (next << 1) };
        self.preferences.history.stopped = next == 0;
        let enabled = next != 0;
        for buffer in self.workspace.buffers() {
            buffer.editor.document().set_change_history_capture(enabled);
        }
        self.editor_chrome.invalidate_layout();
        self.editor_painted_revision = None;
        window.notify_text_input_changed(&self.editor_focus);
        cx.notify();
    }

    pub(super) fn change_history_command(
        &mut self,
        command: crate::menu::ChangeHistoryCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::menu::ChangeHistoryCommand;
        if self.change_history_runtime_flags() == 0
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let result = match command {
            ChangeHistoryCommand::Clear => self.editor_mut().clear_change_history(),
            ChangeHistoryCommand::Next | ChangeHistoryCommand::Previous => self
                .editor_mut()
                .goto_change(command == ChangeHistoryCommand::Next)
                .map(|found| {
                    if found {
                        self.editor_follow_caret = true;
                        self.editor_preferred_x = None;
                        self.editor_scroll_remainder = 0.;
                    } else if !self.mute_sounds {
                        objc2_app_kit::NSBeep();
                    }
                }),
        };
        self.editor_focus.focus(window, cx);
        self.editor_changed(result, window, cx);
    }

    pub(super) fn bookmark_command(
        &mut self,
        command: BookmarkCommand,
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
        self.record_macro_step(crate::macros::MacroStep::Bookmark(command));
        let result = match command {
            BookmarkCommand::Toggle => self.editor_mut().toggle_bookmark(None),
            BookmarkCommand::Next | BookmarkCommand::Previous => {
                let result = self
                    .editor_mut()
                    .goto_bookmark(command == BookmarkCommand::Next);
                if matches!(result, Ok(true)) {
                    let document = self.editor().document();
                    let line = match document
                        .byte_snapshot()
                        .line_index(document.selection().head.0)
                    {
                        Ok(line) => line,
                        Err(error) => {
                            self.notify_editor_error(error.to_string(), cx);
                            return;
                        }
                    };
                    self.ensure_editor_line_visible(line);
                    self.editor_follow_caret = true;
                    self.editor_preferred_x = None;
                    self.editor_scroll_remainder = 0.;
                }
                result.map(|_| ())
            }
            BookmarkCommand::ClearAll => {
                self.editor_mut().clear_bookmarks();
                Ok(())
            }
        };
        self.editor_focus.focus(window, cx);
        self.editor_changed(result, window, cx);
    }

    pub(super) fn render_bookmark_gutter(&self, lines: impl IntoIterator<Item = (usize, bool)>, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.dark;
        let pane = self.pane;
        let folds = self.udl_fold_ranges();
        let fold_headers: std::collections::BTreeSet<_> =
            folds.iter().map(|(header, _)| *header).collect();
        let fold_style = self.pref_string("ScintillaPrimaryView.folderMarkStyle", "box");
        let fold_foreground = self.style_colour("Fold", "bgColor", 0xffffff);
        let fold_background = self.style_colour("Fold", "fgColor", 0x808080);
        let fold_margin = self.style_colour("Fold margin", "bgColor", 0xe9e9e9);
        let fold_margin_highlight = self.style_colour("Fold margin", "fgColor", 0xffffff);
        let owner = self.workspace.active().id();
        let text = self.editor().display_snapshot();
        let row_height = self.editor_line_height();
        let line_number_width = self.editor_line_number_width();
        let (line_number_family, line_number_size) = self.editor_line_number_font();
        let line_number_font_style = self.editor_line_number_font_style();
        let caret_line = text.byte_to_line_idx(
            self.editor().display_selection().head.0,
            ropey::LineType::LF_CR,
        );
        let active_fold = folds
            .iter()
            .filter(|(header, end)| *header <= caret_line && caret_line <= *end)
            .max_by_key(|(header, _)| *header)
            .copied();
        let active_fold_color = self.style_colour("Fold active", "fgColor", 0xff0000);
        let bookmarks: std::collections::BTreeSet<_> =
            self.editor().document().bookmarked_lines().collect();
        let rows = lines
            .into_iter()
            .enumerate()
            .map(|(index, (line, first))| {
                let marked = bookmarks.contains(&line);
                let changes = if self.change_history_runtime_flags() & 2 != 0 {
                    self.editor().document().change_marker_bits(line)
                } else {
                    0
                };
                let header = fold_headers.contains(&line);
                let collapsed = header && self.editor_fold_collapsed(line);
                let above = folds
                    .iter()
                    .any(|(start, end)| *start < line && line <= *end)
                    || (!first && header && !collapsed);
                let below = folds
                    .iter()
                    .any(|(start, end)| *start < line && line < *end)
                    || (header && !collapsed);
                let tail = first && !header && folds.iter().any(|(_, end)| *end == line);
                let marker_background =
                    if active_fold.is_some_and(|(header, end)| header <= line && line <= end) {
                        active_fold_color
                    } else {
                        fold_background
                    };
                div()
                    .id(("editor-gutter", index))
                    .h(row_height)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(line_number_width)
                            .h(row_height)
                            .flex_shrink_0()
                            .pr(px(3.))
                            .text_right()
                            .font_family(line_number_family.clone())
                            .when(line_number_font_style & 1 != 0, |number| number.font_weight(gpui::FontWeight::BOLD))
                            .when(line_number_font_style & 2 != 0, |number| number.italic())
                            .when(line_number_font_style & 4 != 0, |number| number.underline())
                            .text_size(line_number_size)
                            .line_height(row_height)
                            .overflow_hidden()
                            .child(if first && line_number_width > px(0.) {
                                (line + 1).to_string()
                            } else {
                                String::new()
                            }),
                    )
                    .child(
                        div()
                            .id(("bookmark-line", line))
                            .debug_selector(move || {
                                if !first {
                                    format!("bookmark-line-wrap-{}-{index}", pane as usize)
                                } else if pane == crate::workspace::ViewId::Primary {
                                    format!("bookmark-line-{line}")
                                } else {
                                    format!("bookmark-line-secondary-{line}")
                                }
                            })
                            .role(Role::CheckBox)
                            .aria_label(format!("Bookmark line {}", line + 1))
                            .aria_toggled(marked.into())
                            .w(self.editor_bookmark_margin_width())
                            .overflow_hidden()
                            .h(row_height)
                            .flex_shrink_0()
                            .bg(rgb(self.style_colour(
                                "Bookmark margin",
                                "bgColor",
                                self.style_colour(
                                    "Line number margin",
                                    "bgColor",
                                    if self.dark { 0x303030 } else { 0xe0e0e0 },
                                ),
                            )))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if !view.focus_editor_pane(pane, window, cx) {
                                    return;
                                }
                                if view.workspace.active().id() != owner
                                    || view.file_busy
                                    || view.close_dialog
                                    || view.save_all_dialog
                                    || view.batch_editing_blocked()
                                {
                                    return;
                                }
                                let result = view.editor_mut().toggle_bookmark(Some(line));
                                view.editor_focus.focus(window, cx);
                                view.editor_changed(result, window, cx);
                                cx.stop_propagation();
                            }))
                            .when(first && marked, |margin| {
                                margin.child(
                                    div()
                                        .id(("bookmark-marker", line))
                                        .tooltip(move |_, cx| {
                                            icon_tooltip::view("Toggle bookmark", dark, cx)
                                        })
                                        .debug_selector(move || {
                                            if pane == crate::workspace::ViewId::Primary {
                                                format!("bookmark-marker-{line}")
                                            } else {
                                                format!("bookmark-marker-secondary-{line}")
                                            }
                                        })
                                        .size(px(14.))
                                        .child(gpui::img(bookmark_image()).size_full()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(("change-history-line", index))
                            .w(self.editor_history_margin_width())
                            .overflow_hidden()
                            .h(row_height)
                            .flex_shrink_0()
                            .relative()
                            .bg(rgb(self.style_colour(
                                "Change History margin",
                                "bgColor",
                                if self.dark { 0x0c0c0c } else { 0xe0e0e0 },
                            )))
                            .children((1u8..=4).filter_map(|edition| {
                                if changes & (1 << (edition - 1)) == 0 {
                                    return None;
                                }
                                 let (colour, name) = change_history_style(edition, self, "bgColor");
                                Some(
                                    div()
                                        .id(("change-marker", usize::from(edition)))
                                        .debug_selector(move || {
                                            let name = format!(
                                                "change-marker-{name}-{}-{line}",
                                                pane as usize
                                            );
                                            if first {
                                                name
                                            } else {
                                                format!("{name}-wrap-{index}")
                                            }
                                        })
                                        .role(Role::Image)
                                        .aria_label(format!(
                                            "{name} change history on line {}",
                                            line + 1
                                        ))
                                        .absolute()
                                        .left(px(3.))
                                        .top(px(0.))
                                        .w(px(3.))
                                        .h(row_height)
                                        .bg(rgb(colour)),
                                )
                            })),
                    )
                    .child(
                        div()
                            .w(self.editor_fold_margin_width())
                            .h(row_height)
                            .flex_shrink_0()
                            .overflow_hidden()
                            .bg(rgb(fold_margin))
                            .relative()
                            .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                                if fold_margin == fold_margin_highlight { return; }
                                for y in 0..f32::from(bounds.size.height).ceil() as usize {
                                    for x in 0..f32::from(bounds.size.width).ceil() as usize {
                                        if ((f32::from(bounds.left()).floor() as i64 + x as i64)
                                            + (f32::from(bounds.top()).floor() as i64 + y as i64)) & 1 != 0 {
                                            window.paint_quad(fill(
                                                Bounds::new(point(bounds.left() + px(x as f32), bounds.top() + px(y as f32)), size(px(1.), px(1.))),
                                                rgb(fold_margin_highlight),
                                            ));
                                        }
                                    }
                                }
                            }).absolute().size_full())
                            .when(!(first && header), |margin| {
                                margin.child(fold_art(
                                    &fold_style,
                                    None,
                                    above,
                                    below,
                                    tail,
                                    fold_foreground,
                                    marker_background,
                                ))
                            })
                            .when(
                                first
                                    && fold_headers.contains(&line)
                                    && self.editor_fold_margin_width() > px(0.),
                                |margin| {
                                    let collapsed = self.editor_fold_collapsed(line);
                                    margin.child(
                                        div()
                                            .id(("fold-line", line))
                                            .debug_selector(move || {
                                                if pane == crate::workspace::ViewId::Primary {
                                                    format!("fold-line-{line}")
                                                } else {
                                                    format!("fold-line-secondary-{line}")
                                                }
                                            })
                                            .role(Role::Button)
                                            .aria_label(format!(
                                                "{} fold on line {}",
                                                if collapsed { "Expand" } else { "Collapse" },
                                                line + 1
                                            ))
                                            .tooltip(move |_, cx| {
                                                icon_tooltip::view(
                                                    if collapsed {
                                                        "Expand fold"
                                                    } else {
                                                        "Collapse fold"
                                                    },
                                                    dark,
                                                    cx,
                                                )
                                            })
                                            .w(px(14.))
                                            .h(row_height)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                cx.stop_propagation()
                                            })
                                            .on_click(cx.listener(move |view, _, window, cx| {
                                                if view.focus_editor_pane(pane, window, cx)
                                                    && view.workspace.active().id() == owner
                                                    && !view.batch_editing_blocked()
                                                {
                                                    view.toggle_editor_fold(line, window, cx);
                                                }
                                                cx.stop_propagation();
                                            }))
                                            .child(fold_art(
                                                &fold_style,
                                                Some(collapsed),
                                                above,
                                                below,
                                                false,
                                                fold_foreground,
                                                marker_background,
                                            )),
                                    )
                                },
                            ),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("editor-gutter")
            .debug_selector(move || {
                if pane == crate::workspace::ViewId::Primary {
                    "editor-gutter".into()
                } else {
                    "editor-gutter-secondary".into()
                }
            })
            .role(Role::Group)
            .aria_label("Line numbers, bookmarks and change history")
            .absolute()
            .left(px(editor_surface::GUTTER_LEFT))
            .top(px(0.))
            .bottom(px(editor_surface::SCROLLBAR_SIZE))
            .w(self.editor_gutter_width())
            .pt(px(editor_surface::TEXT_TOP))
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(self.style_colour(
                "Line number margin",
                "bgColor",
                self.appearance_colour("DarkMode.customColorActive", 0xe4e4e4),
            )))
            .text_color(rgb(self.style_colour(
                "Line number margin",
                "fgColor",
                self.appearance_colour("DarkMode.customColorDarkText", 0x808080),
            )))
            .text_size(self.editor_font_size())
            .children(rows)
            .into_any_element()
    }
}
