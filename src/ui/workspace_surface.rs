use super::*;

fn tab_style_colour(dark: bool, name: &str, attribute: &str, fallback: u32) -> u32 {
    crate::languages::catalog()
        .ok()
        .and_then(|catalog| {
            catalog
                .themes()
                .theme(dark)
                .global_styles
                .iter()
                .find(|style| {
                    style
                        .attributes
                        .get("name")
                        .is_some_and(|value| value == name)
                })
        })
        .and_then(|style| style.attributes.get(attribute))
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(fallback)
}

fn tab_colour(colour: u32, dark: bool, focused: bool, hovered: bool) -> u32 {
    let red = ((colour >> 16) & 255) as i32;
    let green = ((colour >> 8) & 255) as i32;
    let blue = (colour & 255) as i32;
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    let mut lightness = ((high + low) * 240 + 255) / 510;
    let (hue, mut saturation) = if high == low {
        (160, 0)
    } else {
        let delta = high - low;
        let denominator = if lightness <= 120 {
            high + low
        } else {
            510 - high - low
        };
        let saturation = (denominator / 2 + delta * 240) / denominator;
        let normal = |value| (delta / 2 + (high - value) * 40) / delta;
        let hue = if high == red {
            normal(blue) - normal(green)
        } else if high == green {
            80 + normal(red) - normal(blue)
        } else {
            160 + normal(green) - normal(red)
        };
        (hue.rem_euclid(240), saturation)
    };
    if focused {
        lightness = if dark { 146 } else { 140 };
        saturation = (saturation + if dark { 100 } else { 30 }).min(240);
    } else if hovered {
        lightness += 15;
    }
    if saturation == 0 {
        let gray = (lightness * 255 / 240) as u32 & 255;
        return gray * 0x010101;
    }
    let high = if lightness > 120 {
        saturation + lightness - (saturation * lightness + 120) / 240
    } else {
        ((saturation + 240) * lightness + 120) / 240
    };
    let low = lightness * 2 - high;
    let channel = |mut hue: i32| {
        if hue > 240 {
            hue -= 240;
        } else if hue < 0 {
            hue += 240;
        }
        let value = if hue > 160 {
            low
        } else if hue > 40 && hue <= 120 {
            high
        } else {
            let hue = if hue > 120 { 160 - hue } else { hue };
            (hue * (high - low) + 20) / 40 + low
        };
        ((value * 255 + 120) / 240) as u32 & 255
    };
    (channel(hue + 80) << 16) | (channel(hue) << 8) | channel(hue - 80)
}
use crate::workspace::{BufferId, ViewId};

#[derive(Clone)]
pub(super) struct TabDrag {
    owner: gpui::AnyWindowHandle,
    pane: ViewId,
    buffer: BufferId,
    title: String,
}

impl Render for TabDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .border_1()
            .bg(rgb(0xf0f0f0))
            .text_color(rgb(0x202020))
            .child(self.title.clone())
    }
}

impl MatrixWindow {
    pub(super) fn drop_document_tab(
        &mut self,
        drag: &TabDrag,
        pane: ViewId,
        target: Option<BufferId>,
        over_editor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if drag.owner != self.document_window
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || !self
                .workspace
                .view_buffer_ids(drag.pane)
                .contains(&drag.buffer)
            || (drag.pane == pane && target.is_none() && !over_editor)
        {
            return;
        }
        let target_index = target.and_then(|id| {
            self.workspace
                .view_buffer_ids(pane)
                .iter()
                .position(|candidate| *candidate == id)
        });
        if target.is_some() && target_index.is_none() {
            return;
        }
        self.set_tab_preview(None, cx);
        if !self.focus_editor_pane(drag.pane, window, cx)
            || !self.activate_buffer(drag.buffer, window, cx)
        {
            return;
        }
        if pane == drag.pane && target.is_none() {
            self.menu_input(
                MenuEvent::OpenSubmenuLimited {
                    selector: "view-move-clone-menu",
                    limit: 2,
                },
                window,
                cx,
            );
            if self.menu_bar.mode() == MenuMode::Open {
                self.menu_anchor = Some(window.mouse_position());
                self.menu_submenu_only = true;
                cx.notify();
            }
        } else if pane != drag.pane {
            if window.modifiers().control {
                self.clone_to_other_view(window, cx);
            } else {
                self.move_to_other_view(window, cx);
            }
        } else if let Some(target) = target_index {
            match self.workspace.move_tab(drag.buffer, target) {
                Ok(true) => {
                    self.tab_scroll
                        .scroll_to_item(self.workspace.active_index());
                    self.synchronize_recovery(cx);
                    cx.notify();
                }
                Ok(false) => {}
                Err(error) => self.notify_editor_error(error.to_string(), cx),
            }
        }
    }

    pub(super) fn cancel_view_composition(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.capture_current_document_state();
        self.editor_mut().cancel_composition();
        match discard_host_composition(window) {
            Ok(_) => true,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                false
            }
        }
    }
    pub(super) fn cycle_buffer(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.workspace.neighbor_id(forward);
        self.activate_buffer(id, window, cx);
    }
    pub(super) fn reset_active_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reconcile_editor_panes();
        self.prepare_editor_document();
        let overwrite = self.overwrite_mode;
        self.editor_mut().set_overwrite(overwrite);
        if self.monitoring_active() {
            self.editor_mut().set_read_only(true);
        }
        self.tab_scroll
            .scroll_to_item(self.workspace.active_index());
        if self.document_list_visible {
            self.document_list_scroll.scroll_to_item(
                self.document_list_active_index(),
                gpui::ScrollStrategy::Nearest,
            );
        }
        self.editor_changed(Ok(()), window, cx);
        self.restore_current_document_state(window, cx);
        self.synchronize_recovery(cx);
        self.editor_focus.focus(window, cx);
    }

    pub(super) fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.macro_interaction_blocked()
        {
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        match self.workspace.new_buffer() {
            Ok(_) => {
                self.reset_active_view(window, cx);
            }
            Err(error) => self.notify_editor_error(error.to_string(), cx),
        }
    }

    pub(super) fn activate_buffer(
        &mut self,
        id: BufferId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.macro_interaction_blocked()
        {
            return false;
        }
        if id == self.workspace.active().id() {
            self.editor_focus.focus(window, cx);
            return true;
        }
        if !self
            .workspace
            .buffers()
            .iter()
            .any(|buffer| buffer.id() == id)
        {
            self.notify_editor_error(format!("buffer {} is no longer open", id.0), cx);
            return false;
        }
        if !self.cancel_view_composition(window, cx) {
            return false;
        }
        match self.workspace.activate(id) {
            Ok(()) => {
                self.reset_active_view(window, cx);
                true
            }
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                false
            }
        }
    }

    pub(super) fn render_tabs(&mut self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.dark;
        let pane = self.pane;
        let active = self.workspace.active().id();
        let background = if self.dark { 0x202020 } else { 0xf0f0f0 };
        let selected = if self.dark { 0x383838 } else { 0xf0f0f0 };
        let inactive = if self.dark {
            0x202020
        } else {
            tab_style_colour(false, "Inactive tabs", "bgColor", 0xc0c0c0)
        };
        let inactive_text = if self.dark {
            0xc0c0c0
        } else {
            tab_style_colour(false, "Inactive tabs", "fgColor", 0x808080)
        };
        let active_text = if self.dark {
            0xe0e0e0
        } else {
            tab_style_colour(false, "Active tab text", "fgColor", 0x000000)
        };
        let button_size = if self.dark { 16. } else { 11. };
        let indicator = tab_style_colour(
            self.dark,
            if focused {
                "Active tab focused indicator"
            } else {
                "Active tab unfocused indicator"
            },
            "fgColor",
            if focused { 0xfaaa3c } else { 0xfad296 },
        );
        let vertical = self.tab_vertical;
        let multiline = self.tab_multiline;
        let draggable = !self.file_busy
            && !self.close_dialog
            && !self.save_all_dialog
            && !self.batch_editing_blocked();
        let owner = self.document_window;
        let dragging = cx.has_active_drag();
        let text_system = cx.text_system();
        let font = text_system.resolve_font(&gpui::font("Helvetica"));
        let text_descent = f32::from(text_system.descent(font, px(14.))).ceil() as i32;
        div()
            .id(if pane == crate::workspace::ViewId::Primary {
                "buffer-tabs"
            } else {
                "buffer-tabs-secondary"
            })
            .debug_selector(move || {
                if pane == crate::workspace::ViewId::Primary {
                    "buffer-tabs".into()
                } else {
                    "buffer-tabs-secondary".into()
                }
            })
            .role(Role::TabList)
            .aria_label("Documents")
            .flex_shrink_0()
            .flex()
            .overflow_hidden()
            .when(vertical, |tabs| {
                tabs.flex_col().w(px(160.)).h_full().overflow_y_scroll()
            })
            .when(!vertical && !multiline, |tabs| {
                tabs.w_full().h(px(27.)).overflow_x_scroll()
            })
            .when(!vertical && multiline, |tabs| {
                tabs.w_full().flex_wrap().h_auto().overflow_y_scroll()
            })
            .track_scroll(&self.tab_scroll)
            .px(px(2.))
            .bg(rgb(background))
            .text_size(px(14.))
            .on_drop(cx.listener(move |view, drag: &TabDrag, window, cx| {
                view.drop_document_tab(drag, pane, None, false, window, cx);
                cx.stop_propagation();
            }))
            .children(
                self.workspace
                    .view_buffer_ids(pane)
                    .iter()
                    .filter_map(|id| {
                        self.workspace
                            .buffers()
                            .iter()
                            .find(|buffer| buffer.id() == *id)
                    })
                    .map(|buffer| {
                        let id = buffer.id();
                        let name = buffer.name();
                        let dirty = buffer.is_modified();
                        let pinned = buffer.is_pinned();
                        let pin_icon = match (pinned, self.dark) {
                            (false, false) => super::toolbar_icons::ToolbarIcon::PinTab,
                            (true, false) => super::toolbar_icons::ToolbarIcon::PinnedTab,
                            (false, true) => super::toolbar_icons::ToolbarIcon::PinTabDark,
                            (true, true) => super::toolbar_icons::ToolbarIcon::PinnedTabDark,
                        };
                        let pin_hover_icon = match (pinned, self.dark) {
                            (true, false) => super::toolbar_icons::ToolbarIcon::PinTab,
                            (false, false) => super::toolbar_icons::ToolbarIcon::PinnedTab,
                            (true, true) => super::toolbar_icons::ToolbarIcon::PinTabDark,
                            (false, true) => super::toolbar_icons::ToolbarIcon::PinnedTabDark,
                        };
                        use super::toolbar_icons::ToolbarIcon;
                        let close_icon = match (dark, id == active) {
                            (false, true) => ToolbarIcon::CloseTab,
                            (false, false) => ToolbarIcon::CloseTabOnTab,
                            (true, true) => ToolbarIcon::CloseTabDark,
                            (true, false) => ToolbarIcon::CloseTabOnTabDark,
                        };
                        let close_hover_icon = if dark {
                            ToolbarIcon::CloseTabHoverDark
                        } else {
                            ToolbarIcon::CloseTabHover
                        };
                        let close_pressed_icon = if dark {
                            ToolbarIcon::CloseTabPressedDark
                        } else {
                            ToolbarIcon::CloseTabPressed
                        };
                        let close_pressed = self.tab_close_pressed == Some((pane, id));
                        let tab_group =
                            gpui::SharedString::from(format!("tab-{}-{}", pane as usize, id.0));
                        let pin_group =
                            gpui::SharedString::from(format!("tab-pin-{}-{}", pane as usize, id.0));
                        let close_group = gpui::SharedString::from(format!(
                            "tab-close-{}-{}",
                            pane as usize, id.0
                        ));
                        let color = self.workspace.tab_color(pane, id).map(|color| {
                            let colors = if self.dark {
                                [0x807848, 0x568048, 0x507094, 0x804849, 0x754880]
                            } else {
                                [0xf3f0cb, 0xdbf3cb, 0xcbdbf3, 0xf3dbcb, 0xf3cbee]
                            };
                            let name = if dark {
                                format!("Tab color dark mode {}", color + 1)
                            } else {
                                format!("Tab color {}", color + 1)
                            };
                            tab_style_colour(dark, &name, "bgColor", colors[usize::from(color)])
                        });
                        let inactive_background =
                            color.map_or(inactive, |colour| tab_colour(colour, dark, false, false));
                        let hover_background = tab_colour(inactive_background, dark, false, true);
                        let icon = if dirty {
                            super::toolbar_icons::ToolbarIcon::ModifiedDocument
                        } else {
                            super::toolbar_icons::ToolbarIcon::SavedDocument
                        };
                        div()
                            .id(("buffer-tab", id.0))
                            .group(tab_group.clone())
                            .debug_selector(move || {
                                if pane == crate::workspace::ViewId::Primary {
                                    format!("buffer-tab-{}", id.0)
                                } else {
                                    format!("buffer-tab-secondary-{}", id.0)
                                }
                            })
                            .role(Role::Tab)
                            .aria_label(name.clone())
                            .aria_selected(id == active)
                            .aria_description(if dirty {
                                "Modified document"
                            } else {
                                "Unmodified document"
                            })
                            .relative()
                            .h(px(if vertical {
                                27.
                            } else if id == active {
                                24.
                            } else {
                                20.
                            }))
                            .mt(px(if vertical {
                                0.
                            } else if id == active {
                                2.
                            } else {
                                4.
                            }))
                            .when(vertical, |tab| tab.w_full())
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(0.))
                            .px(px(if id == active && !vertical { 5. } else { 3. }))
                            .border_1()
                            .border_color(rgb(if dark { 0x646464 } else { 0x808080 }))
                            .text_color(rgb(if id == active {
                                active_text
                            } else {
                                inactive_text
                            }))
                            .bg(rgb(if id == active {
                                selected
                            } else {
                                inactive_background
                            }))
                            .when(
                                id != active && !dragging && inactive_background != selected,
                                |tab| tab.hover(move |style| style.bg(rgb(hover_background))),
                            )
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.set_tab_preview(None, cx);
                                if view.focus_editor_pane(pane, window, cx) {
                                    view.activate_buffer(id, window, cx);
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _, window, cx| {
                                    view.set_tab_preview(None, cx);
                                    if view.focus_editor_pane(pane, window, cx) {
                                        view.activate_buffer(id, window, cx);
                                    }
                                }),
                            )
                            .when(draggable, |tab| {
                                tab.on_drag(
                                    TabDrag {
                                        owner,
                                        pane,
                                        buffer: id,
                                        title: name.clone(),
                                    },
                                    |drag, _, _, cx| cx.new(|_| drag.clone()),
                                )
                            })
                            .on_drop(cx.listener(move |view, drag: &TabDrag, window, cx| {
                                view.drop_document_tab(drag, pane, Some(id), false, window, cx);
                                cx.stop_propagation();
                            }))
                            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                                view.set_tab_preview(
                                    if *hovered { Some((pane, id)) } else { None },
                                    cx,
                                );
                            }))
                            .on_mouse_down(
                                MouseButton::Middle,
                                cx.listener(move |view, _, window, cx| {
                                    if view.focus_editor_pane(pane, window, cx)
                                        && view.activate_buffer(id, window, cx)
                                    {
                                        view.request_close_current(window, cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(0.))
                                    .left(px(0.))
                                    .bottom(px(0.))
                                    .w(px(1.))
                                    .bg(rgb(if dark { 0x646464 } else { 0xffffff })),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(0.))
                                    .left(px(0.))
                                    .right(px(0.))
                                    .h(px(1.))
                                    .bg(rgb(if dark { 0x646464 } else { 0xffffff })),
                            )
                            .when(id == active, |tab| {
                                tab.child(
                                    div()
                                        .absolute()
                                        .top(px(0.))
                                        .left(px(0.))
                                        .right(px(0.))
                                        .h(px(4.))
                                        .bg(rgb(color.map_or(indicator, |colour| {
                                            tab_colour(colour, dark, focused, false)
                                        }))),
                                )
                            })
                            .child(gpui::img(icon.image()).size(px(16.)).flex_shrink_0())
                            .child(
                                div()
                                    .when(vertical, |caption| {
                                        caption.flex_1().min_w(px(0.)).truncate()
                                    })
                                    .when(!vertical, |caption| {
                                        caption.flex_shrink_0().whitespace_nowrap()
                                    })
                                    .line_height(px(20.))
                                    .when(!vertical, |caption| {
                                        caption.relative().top(px(2.
                                            + ((text_descent + i32::from(dark || id != active)) / 2)
                                                as f32))
                                    })
                                    .child(format!(" {name}")),
                            )
                            .child(
                                div()
                                    .id(("buffer-tab-pin", id.0))
                                    .group(pin_group.clone())
                                    .debug_selector(move || {
                                        format!("buffer-tab-pin-{}-{}", pane as usize, id.0)
                                    })
                                    .role(Role::Button)
                                    .aria_label(if pinned { "Unpin tab" } else { "Pin tab" })
                                    .tooltip(move |_, cx| {
                                        icon_tooltip::view(
                                            if pinned { "Unpin tab" } else { "Pin tab" },
                                            dark,
                                            cx,
                                        )
                                    })
                                    .size(px(button_size))
                                    .mr(px(if self.dark { 4. } else { 8. }))
                                    .flex_shrink_0()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        if view.focus_editor_pane(pane, window, cx)
                                            && view.activate_buffer(id, window, cx)
                                        {
                                            view.toggle_pin_tab(window, cx);
                                        }
                                        cx.stop_propagation();
                                    }))
                                    .child(
                                        div()
                                            .id(("buffer-tab-pin-icon", id.0))
                                            .debug_selector(move || {
                                                format!(
                                                    "buffer-tab-{}-icon-{}-{}",
                                                    if pinned { "pinned" } else { "pin" },
                                                    pane as usize,
                                                    id.0
                                                )
                                            })
                                            .relative()
                                            .size_full()
                                            .opacity(if id == active || pinned { 1. } else { 0. })
                                            .group_hover(tab_group.clone(), |style| {
                                                style.opacity(1.)
                                            })
                                            .child(gpui::img(pin_icon.image()).size_full())
                                            .child(
                                                div()
                                                    .absolute()
                                                    .inset_0()
                                                    .opacity(0.)
                                                    .group_hover(pin_group, |style| {
                                                        style.opacity(1.)
                                                    })
                                                    .child(
                                                        gpui::img(pin_hover_icon.image())
                                                            .size_full(),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id(("buffer-tab-close", id.0))
                                    .group(close_group.clone())
                                    .debug_selector(move || {
                                        if pane == crate::workspace::ViewId::Primary {
                                            format!("buffer-tab-close-{}", id.0)
                                        } else {
                                            format!("buffer-tab-close-secondary-{}", id.0)
                                        }
                                    })
                                    .role(Role::Button)
                                    .aria_label(format!("Close {}", buffer.name()))
                                    .tooltip(move |_, cx| icon_tooltip::view("Close", dark, cx))
                                    .size(px(button_size))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _, _, cx| {
                                            view.tab_close_pressed = Some((pane, id));
                                            cx.notify();
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(|view, _, _, cx| {
                                            view.tab_close_pressed = None;
                                            cx.notify();
                                        }),
                                    )
                                    .on_mouse_up_out(
                                        MouseButton::Left,
                                        cx.listener(|view, _, _, cx| {
                                            if view.tab_close_pressed.take().is_some() {
                                                cx.notify();
                                            }
                                        }),
                                    )
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        if view.focus_editor_pane(pane, window, cx)
                                            && view.activate_buffer(id, window, cx)
                                        {
                                            view.request_close_current(window, cx);
                                        }
                                        cx.stop_propagation();
                                    }))
                                    .child(
                                        div()
                                            .relative()
                                            .size_full()
                                            .opacity(if id == active { 1. } else { 0. })
                                            .group_hover(tab_group, |style| style.opacity(1.))
                                            .child(gpui::img(close_icon.image()).size_full())
                                            .child(
                                                div()
                                                    .absolute()
                                                    .inset_0()
                                                    .opacity(0.)
                                                    .group_hover(close_group.clone(), |style| {
                                                        style.opacity(1.)
                                                    })
                                                    .child(
                                                        gpui::img(close_hover_icon.image())
                                                            .size_full(),
                                                    ),
                                            )
                                            .when(close_pressed, |artwork| {
                                                artwork.child(
                                                    div()
                                                        .absolute()
                                                        .inset_0()
                                                        .opacity(0.)
                                                        .group_hover(close_group, |style| {
                                                            style.opacity(1.)
                                                        })
                                                        .child(
                                                            gpui::img(close_pressed_icon.image())
                                                                .size_full(),
                                                        ),
                                                )
                                            }),
                                    ),
                            )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;

    #[gpui::test]
    fn sorting_overflowed_tabs_reveals_the_same_source_active_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(480.), px(400.)));
        for _ in 0..14 {
            cx.dispatch_action(NewFile);
        }
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.tab_scroll.offset().x < px(0.)));
        view.update_in(cx, |view, window, cx| {
            view.menu_input(MenuEvent::OpenSubmenu("window-sort-tabs"), window, cx);
            view.menu_input(MenuEvent::First, window, cx);
            view.menu_input(MenuEvent::Down, window, cx);
            view.menu_input(MenuEvent::Activate, window, cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(15));
            assert_eq!(view.workspace.active_index(), 0);
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
            assert!(!view.workspace.active().is_modified());
        });
        let strip = cx.debug_bounds("buffer-tabs").unwrap();
        let selected = cx.debug_bounds("buffer-tab-15").unwrap();
        assert!(
            selected.left() >= strip.left() && selected.right() <= strip.right(),
            "source WindowsDlg::doSort reactivates and reveals the selected document: {selected:?}, {strip:?}"
        );
    }

    #[gpui::test]
    fn pinning_an_overflowed_tab_keeps_the_source_active_item_visible(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(480.), px(400.)));
        for _ in 0..14 {
            cx.dispatch_action(NewFile);
        }
        cx.run_until_parked();
        let strip = cx.debug_bounds("buffer-tabs").unwrap();
        view.read_with(cx, |view, _| {
            assert!(
                view.tab_scroll.offset().x < px(0.),
                "the setup must overflow: {strip:?}"
            )
        });
        let selected = cx.debug_bounds("buffer-tab-15").unwrap();
        assert!(
            selected.left() >= strip.left() && selected.right() <= strip.right(),
            "new active tab is revealed: {selected:?}, {strip:?}"
        );
        view.update(cx, |view, _| {
            view.editor_mut().insert_text("temporary edit").unwrap();
            assert!(view.editor_mut().undo());
        });
        cx.dispatch_action(PinCurrentTab);
        cx.run_until_parked();
        let selected = cx.debug_bounds("buffer-tab-15").unwrap();
        assert!(
            selected.left() >= strip.left() && selected.right() <= strip.right(),
            "source tabToStart keeps the pinned active tab visible: {selected:?}, {strip:?}"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(15));
            assert_eq!(view.workspace.active_index(), 0);
            assert!(view.workspace.active().is_pinned());
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
            assert!(view.editor().document().can_redo());
            assert!(!view.workspace.active().is_modified());
        });
        for _ in 0..14 {
            cx.dispatch_action(NextBuffer);
            cx.dispatch_action(PinCurrentTab);
        }
        cx.dispatch_action(NextBuffer);
        cx.dispatch_action(PinCurrentTab);
        cx.run_until_parked();
        let selected = cx.debug_bounds("buffer-tab-15").unwrap();
        assert!(
            selected.left() >= strip.left() && selected.right() <= strip.right(),
            "source tabToEnd reveals the unpinned active tab: {selected:?}, {strip:?}"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(15));
            assert_eq!(view.workspace.active_index(), 14);
            assert!(!view.workspace.active().is_pinned());
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
            assert!(view.editor().document().can_redo());
            assert!(!view.workspace.active().is_modified());
        });
    }

    #[test]
    fn tab_close_pressed_artwork_resets_when_pointer_leaves_before_release() {
        let mut app = gpui::HeadlessAppContext::with_platform(
            std::sync::Arc::new(gpui::NoopTextSystem),
            std::sync::Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        let handle = app
            .open_window(size(px(800.), px(480.)), |window, cx| {
                cx.new(|cx| MatrixWindow::new(false, window, cx))
            })
            .unwrap();
        for _ in 0..3 {
            app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            app.run_until_parked();
        }
        let capture = |app: &mut gpui::HeadlessAppContext| {
            let image = app.capture_screenshot(handle.into()).unwrap();
            let scale = image.width() / 800;
            (85 * scale..101 * scale)
                .flat_map(|y| (85 * scale..110 * scale).map(move |x| (x, y)))
                .map(|(x, y)| image.get_pixel(x, y).0)
                .collect::<Vec<_>>()
        };
        let normal = capture(&mut app);
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                    position: gpui::point(px(100.), px(90.)),
                    ..Default::default()
                }),
                cx,
            );
            window.draw(cx).clear(cx);
        })
        .unwrap();
        app.run_until_parked();
        app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let hovered = capture(&mut app);
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                    position: gpui::point(px(100.), px(90.)),
                    button: MouseButton::Left,
                    ..Default::default()
                }),
                cx,
            );
            window.draw(cx).clear(cx);
        })
        .unwrap();
        app.run_until_parked();
        app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        assert!(
            capture(&mut app) != hovered,
            "source pressed artwork differs from hover artwork"
        );
        assert!(
            capture(&mut app) != normal,
            "mouse down paints the original pressed artwork"
        );
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                    position: gpui::point(px(40.), px(90.)),
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
            window.draw(cx).clear(cx);
        })
        .unwrap();
        assert!(
            capture(&mut app) == normal,
            "source restores normal close artwork before mouse up"
        );
    }

    #[gpui::test]
    fn tab_selection_retains_the_native_two_pixel_edge_expansion(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let active = cx.debug_bounds("buffer-tab-1").unwrap();
        cx.dispatch_action(NewFile);
        let inactive = cx.debug_bounds("buffer-tab-1").unwrap();
        assert_eq!(active.size.width - inactive.size.width, px(4.));
        assert_eq!(active.size.height - inactive.size.height, px(4.));
    }

    #[gpui::test]
    fn tab_width_uses_source_caption_and_button_metrics_without_fixed_caps(
        cx: &mut gpui::TestAppContext,
    ) {
        let files = tempfile::tempdir().unwrap();
        let short = files.path().join("a");
        let long = files
            .path()
            .join(format!("{}.txt", "long-document-name-".repeat(5)));
        for path in [&short, &long] {
            std::fs::write(path, "saved").unwrap();
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            for path in [&short, &long] {
                let (file, text) = crate::files::DiskFile::load_unicode(path).unwrap();
                view.workspace.open_buffer(file, &text).unwrap();
            }
            view.reset_active_view(window, cx);
        });
        let short = cx.debug_bounds("buffer-tab-2").unwrap();
        let long = cx.debug_bounds("buffer-tab-3").unwrap();
        assert!(
            short.size.width < px(80.),
            "no invented 80px minimum: {short:?}"
        );
        assert!(
            long.size.width > px(240.),
            "no invented 240px maximum: {long:?}"
        );
        assert_eq!(
            cx.debug_bounds("buffer-tab-close-3").unwrap().size,
            size(px(11.), px(11.))
        );
        assert_eq!(
            cx.debug_bounds("buffer-tab-pin-0-3").unwrap().size,
            size(px(11.), px(11.))
        );
        view.update_in(cx, |view, window, cx| {
            view.editor_mut().insert_text("modified").unwrap();
            view.editor_changed(Ok(()), window, cx);
        });
        assert_eq!(
            cx.debug_bounds("buffer-tab-3").unwrap().size.width,
            long.size.width,
            "modified icon changes, the source tab caption does not gain an asterisk"
        );
        cx.dispatch_action(ToggleVerticalTabs);
        let strip = cx.debug_bounds("buffer-tabs").unwrap();
        for id in ["buffer-tab-pin-0-3", "buffer-tab-close-3"] {
            let button = cx.debug_bounds(id).unwrap();
            assert!(
                button.right() <= strip.right(),
                "{id} must remain reachable inside vertical tabs: {button:?}, {strip:?}"
            );
        }
    }

    #[test]
    fn tab_source_colours_reach_the_main_window_scene() {
        let mut app = gpui::HeadlessAppContext::with_platform(
            std::sync::Arc::new(gpui::NoopTextSystem),
            std::sync::Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        let handle = app
            .open_window(size(px(800.), px(480.)), |window, cx| {
                cx.new(|cx| {
                    let mut view = MatrixWindow::new(false, window, cx);
                    let first = view.workspace.active().id();
                    view.workspace.new_buffer().unwrap();
                    view.workspace.activate(first).unwrap();
                    view.reset_active_view(window, cx);
                    view
                })
            })
            .unwrap();
        for _ in 0..3 {
            app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            app.run_until_parked();
        }
        let image = app.capture_screenshot(handle.into()).unwrap();
        let scale = image.width() / 800;
        let pixel = |x, y| image.get_pixel(x * scale, y * scale).0[..3].to_vec();
        assert_eq!(
            pixel(600, 90),
            [240, 240, 240],
            "unused tab strip uses COLOR_BTNFACE"
        );
        assert_eq!(
            pixel(50, 81),
            [250, 170, 60],
            "source four-pixel active indicator"
        );
        assert_eq!(
            pixel(50, 99),
            [240, 240, 240],
            "active bottom is not an orange border"
        );
        assert_eq!(
            pixel(600, 456),
            [160, 160, 160],
            "status edge uses source COLOR_3DSHADOW"
        );
        handle
            .update(&mut app, |view, _, cx| {
                let id = view.workspace.active().id();
                view.workspace
                    .set_tab_color(view.pane, id, Some(0))
                    .unwrap();
                cx.notify();
            })
            .unwrap();
        app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let image = app.capture_screenshot(handle.into()).unwrap();
        assert_eq!(
            image.get_pixel(50 * scale, 81 * scale).0[..3],
            [228, 217, 69],
            "source Color 1 focuses the indicator using HLS lightness 140 and saturation +30"
        );
        assert_eq!(
            image.get_pixel(50 * scale, 99 * scale).0[..3],
            [240, 240, 240],
            "individual active color never fills the tab body"
        );
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                    position: gpui::point(px(200.), px(90.)),
                    pressed_button: None,
                    modifiers: gpui::Modifiers::default(),
                }),
                cx,
            );
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let image = app.capture_screenshot(handle.into()).unwrap();
        assert_eq!(
            image.get_pixel(200 * scale, 98 * scale).0[..3],
            [208, 208, 208],
            "inactive hover uses the source HLS lightness +15"
        );
        handle
            .update(&mut app, |view, _, cx| {
                view.dark = true;
                cx.notify();
            })
            .unwrap();
        app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let image = app.capture_screenshot(handle.into()).unwrap();
        assert_eq!(
            image.get_pixel(50 * scale, 81 * scale).0[..3],
            [224, 204, 86]
        );
        assert_eq!(image.get_pixel(50 * scale, 99 * scale).0[..3], [56, 56, 56]);
        assert_eq!(
            image.get_pixel(600 * scale, 90 * scale).0[..3],
            [32, 32, 32]
        );
        handle
            .update(&mut app, |view, _, cx| {
                view.dark = false;
                let id = view.workspace.active().id();
                view.workspace.set_tab_color(view.pane, id, None).unwrap();
                cx.notify();
            })
            .unwrap();
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_action(Box::new(CloneToOtherView), cx);
        })
        .unwrap();
        app.run_until_parked();
        app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let image = app.capture_screenshot(handle.into()).unwrap();
        assert_eq!(
            image.get_pixel(50 * scale, 81 * scale).0[..3],
            [255, 202, 176],
            "the original active-view identity selects the unfocused-pane indicator"
        );
        assert_eq!(
            image.get_pixel(450 * scale, 81 * scale).0[..3],
            [250, 170, 60]
        );
    }

    #[gpui::test]
    fn tab_close_control_targets_background_document_and_honors_cancel(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("unsaved first document");
        cx.dispatch_action(NewFile);
        let close = cx
            .debug_bounds("buffer-tab-close-1")
            .expect("first tab close control");
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        let confirmation = cx
            .windows()
            .into_iter()
            .find(|handle| {
                let mut dialog = gpui::VisualTestContext::from_window(*handle, cx);
                dialog.debug_bounds("close-cancel").is_some()
            })
            .expect("independent save confirmation");
        assert!(
            gpui::VisualTestContext::from_window(confirmation, cx)
                .debug_bounds("close-cancel")
                .is_some(),
            "dirty tab uses the existing close guard"
        );
        cx.dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
        assert!(cx.debug_bounds("buffer-tab-2").is_some());
        let close = cx
            .debug_bounds("buffer-tab-close-1")
            .expect("cancel retained the tab");
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        let confirmation = cx
            .windows()
            .into_iter()
            .find(|handle| {
                gpui::VisualTestContext::from_window(*handle, cx)
                    .debug_bounds("close-discard")
                    .is_some()
            })
            .expect("second independent save confirmation");
        let mut dialog = gpui::VisualTestContext::from_window(confirmation, cx);
        let discard = dialog
            .debug_bounds("close-discard")
            .expect("discard choice")
            .center();
        dialog.simulate_click(discard, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("buffer-tab-1").is_none());
        assert!(
            cx.debug_bounds("buffer-tab-2").is_some(),
            "closing a background tab must not close its neighbor"
        );
    }

    #[gpui::test]
    fn tab_middle_click_closes_only_the_pointed_clean_document(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(NewFile);
        let position = cx.debug_bounds("buffer-tab-1").expect("first tab").center();
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Middle,
            click_count: 1,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position,
            button: MouseButton::Middle,
            click_count: 1,
            ..Default::default()
        });
        assert!(cx.debug_bounds("buffer-tab-1").is_none());
        assert!(cx.debug_bounds("buffer-tab-2").is_some());
        assert!(cx.debug_bounds("close-cancel").is_none());
    }
}
