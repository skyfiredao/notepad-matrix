use super::*;

const TAB_SCROLL_ARROW_SIZE: f32 = 17.;
const TAB_SCROLL_ARROW_TOP: f32 = 6.;

fn first_visible_tab(scroll: &gpui::ScrollHandle, count: usize) -> Option<usize> {
    (0..count).find(|index| {
        scroll.bounds_for_item(*index).is_some_and(|bounds| {
            bounds.right() + scroll.offset().x > scroll.bounds().left() + px(2.)
        })
    })
}

pub(super) fn theme_style_colour(dark: bool, name: &str, attribute: &str, fallback: u32) -> u32 {
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
    fn scroll_tabs(&mut self, pane: ViewId, forward: bool, cx: &mut Context<Self>) -> bool {
        let scroll = if pane == self.pane {
            &self.tab_scroll
        } else {
            &self.other_pane.tab_scroll
        };
        let viewport = scroll.bounds();
        let offset = scroll.offset();
        let count = self.workspace.view_buffer_ids(pane).len();
        let Some(first) = first_visible_tab(scroll, count) else {
            return false;
        };
        let target = if forward {
            (first + 1).min(count - 1)
        } else {
            first.saturating_sub(1)
        };
        if let Some(bounds) = scroll.bounds_for_item(target) {
            let x =
                (viewport.left() + px(2.) - bounds.left()).clamp(-scroll.max_offset().x, px(0.));
            if x == offset.x {
                return false;
            }
            scroll.set_offset(gpui::point(x, offset.y));
            cx.notify();
            return true;
        }
        false
    }

    fn tab_wheel(
        &mut self,
        pane: ViewId,
        draggable: bool,
        event: &gpui::ScrollEvent,
        original_offset: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modified = event.modifiers.control || event.modifiers.shift;
        if !modified && (self.tab_vertical || self.tab_multiline) {
            return;
        }
        let delta = event.delta.pixel_delta(px(1.));
        let wheel = if event.modifiers.shift && delta.y == px(0.) {
            delta.x
        } else {
            delta.y
        };
        if (delta.x != px(0.) && !event.modifiers.shift)
            || wheel == px(0.)
            || !f32::from(wheel).is_finite()
        {
            return;
        }
        cx.stop_propagation();
        if modified {
            let scroll = if pane == self.pane {
                &self.tab_scroll
            } else {
                &self.other_pane.tab_scroll
            };
            scroll.set_offset(original_offset);
        }
        if cx.has_active_drag() {
            return;
        }
        let forward = wheel < px(0.);
        if !modified {
            self.scroll_tabs(pane, forward, cx);
            return;
        }
        if !self.focus_editor_pane(pane, window, cx) {
            return;
        }
        let count = self.workspace.view_buffer_ids(pane).len();
        let current = self.workspace.active_index();
        let target = if event.modifiers.control && event.modifiers.shift {
            if forward { count - 1 } else { 0 }
        } else if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        if event.modifiers.shift && !event.modifiers.control && draggable {
            if let Err(error) = self
                .workspace
                .move_tab(self.workspace.active().id(), target)
            {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
            self.tab_scroll.scroll_to_item(target);
            self.synchronize_recovery(cx);
            cx.notify();
        } else {
            let id = self.workspace.view_buffer_ids(pane)[target];
            self.activate_buffer(id, window, cx);
        }
    }

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
            theme_style_colour(false, "Inactive tabs", "bgColor", 0xc0c0c0)
        };
        let inactive_text = if self.dark {
            0xc0c0c0
        } else {
            theme_style_colour(false, "Inactive tabs", "fgColor", 0x808080)
        };
        let active_text = if self.dark {
            0xe0e0e0
        } else {
            theme_style_colour(false, "Active tab text", "fgColor", 0x000000)
        };
        let button_size = if self.dark { 16. } else { 11. };
        let indicator = theme_style_colour(
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
        let space_width = f32::from(text_system.layout_width(font, px(14.), ' ')).round();
        let scroll = self.tab_scroll.clone();
        let previous_size = scroll.bounds().size;
        let previous_offset = scroll.offset();
        let wheel_offset = Rc::new(std::cell::Cell::new(previous_offset));
        let captured_offset = wheel_offset.clone();
        let inner_offset = wheel_offset.clone();
        let captured_scroll = scroll.clone();
        let capture_scroll = gpui::canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |_: &gpui::ScrollEvent, phase, _, _| {
                    if phase == gpui::DispatchPhase::Capture {
                        captured_offset.set(captured_scroll.offset());
                    }
                });
            },
        )
        .absolute()
        .size(px(0.));
        let render_owner = cx.entity();
        let tabs = div()
            .on_children_prepainted(move |_, window, _| {
                if scroll.bounds().size != previous_size {
                    let scroll = scroll.clone();
                    let owner = render_owner.clone();
                    window.on_next_frame(move |_, cx| {
                        owner.update(cx, |view, cx| {
                            if let Some(active) = view.workspace.active_id_in_view(pane)
                                && let Some(index) = view
                                    .workspace
                                    .view_buffer_ids(pane)
                                    .iter()
                                    .position(|id| *id == active)
                            {
                                scroll.scroll_to_item(index);
                                cx.notify();
                            }
                        });
                    });
                } else if scroll.offset() != previous_offset {
                    let owner = render_owner.clone();
                    window.on_next_frame(move |_, cx| owner.update(cx, |_, cx| cx.notify()));
                }
            })
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
                tabs.flex_1()
                    .min_w(px(0.))
                    .h(px(27.))
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
            })
            .when(!vertical && multiline, |tabs| {
                tabs.w_full().flex_wrap().h_auto().overflow_y_scroll()
            })
            .track_scroll(&self.tab_scroll)
            .on_scroll(cx.listener(move |view, event, window, cx| {
                view.tab_wheel(pane, draggable, event, inner_offset.get(), window, cx);
            }))
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
                        let limit = self.tab_compact_label_len;
                        let caption = if limit == 0 || name.encode_utf16().count() <= limit {
                            name.clone()
                        } else {
                            let mut caption = String::new();
                            let mut length = 0;
                            for ch in name.chars() {
                                length += ch.len_utf16();
                                if length > limit.saturating_sub(3) {
                                    break;
                                }
                                caption.push(ch);
                            }
                            caption.push_str(&".".repeat(limit.min(3)));
                            caption
                        };
                        let tab_height: f32 = if vertical {
                            27.
                        } else if id == active {
                            24.
                        } else {
                            20.
                        };
                        let caption_pin_gap = if !dark && !vertical {
                            let drawing_height = if id == active {
                                tab_height - 6.
                            } else {
                                tab_height
                            };
                            let image_inset = ((drawing_height - 16. + 1.) / 2.).floor();
                            let button_inset = ((drawing_height - button_size + 1.) / 2.).floor();
                            3. * 16. + if id == active { -4. } else { 4. }
                                - image_inset
                                - button_inset
                                - 2. * button_size
                                - 8.
                                - space_width
                        } else {
                            0.
                        };
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
                            theme_style_colour(dark, &name, "bgColor", colors[usize::from(color)])
                        });
                        let inactive_background =
                            color.map_or(inactive, |colour| tab_colour(colour, dark, false, false));
                        let hover_background = tab_colour(inactive_background, dark, false, true);
                        let (icon, dark_icon) = if buffer.is_monitoring() {
                            (
                                ToolbarIcon::MonitoringDocument,
                                ToolbarIcon::MonitoringDocumentDark,
                            )
                        } else if buffer
                            .disk_file
                            .as_ref()
                            .is_some_and(crate::files::DiskFile::is_read_only)
                        {
                            (
                                ToolbarIcon::SystemReadOnlyDocument,
                                ToolbarIcon::SystemReadOnlyDocumentDark,
                            )
                        } else if buffer.is_user_read_only() {
                            (
                                ToolbarIcon::ReadOnlyDocument,
                                ToolbarIcon::ReadOnlyDocumentDark,
                            )
                        } else if dirty {
                            (
                                ToolbarIcon::ModifiedDocument,
                                ToolbarIcon::ModifiedDocumentDark,
                            )
                        } else {
                            (ToolbarIcon::SavedDocument, ToolbarIcon::SavedDocumentDark)
                        };
                        let icon = if dark { dark_icon } else { icon };
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
                            .h(px(tab_height))
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
                                cx.stop_propagation();
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _, window, cx| {
                                    view.set_tab_preview(None, cx);
                                    if view.focus_editor_pane(pane, window, cx) {
                                        view.activate_buffer(id, window, cx);
                                    }
                                    cx.stop_propagation();
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
                            .on_mouse_up(
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
                                    .when(!vertical && !dark, |caption| {
                                        caption.mr(px(caption_pin_gap))
                                    })
                                    .line_height(px(20.))
                                    .when(!vertical, |caption| {
                                        caption.relative().top(px(2.
                                            + ((text_descent + i32::from(dark || id != active)) / 2)
                                                as f32))
                                    })
                                    .child(format!(" {caption}")),
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
                                            .child(
                                                div()
                                                    .size_full()
                                                    .group_hover(pin_group.clone(), |style| {
                                                        style.opacity(0.)
                                                    })
                                                    .child(gpui::img(pin_icon.image()).size_full()),
                                            )
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
            .into_any_element();
        if vertical || multiline {
            return div()
                .relative()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .when(vertical, |row| row.w(px(160.)).h_full())
                .when(!vertical, |row| row.w_full())
                .child(tabs)
                .child(capture_scroll)
                .into_any_element();
        }
        let overflowing = self.tab_overflow[pane as usize];
        let scroll = self.tab_scroll.clone();
        let observed_scroll = scroll.clone();
        let owner = cx.entity();
        let arrows = [false, true].map(|forward| {
            let enabled = if forward {
                scroll.offset().x > -scroll.max_offset().x
                    && first_visible_tab(&scroll, self.workspace.view_buffer_ids(pane).len())
                        .is_some_and(|index| index + 1 < self.workspace.view_buffer_ids(pane).len())
            } else {
                scroll.offset().x < px(0.)
            };
            div()
                .id(("tab-scroll-arrow", pane as usize * 2 + usize::from(forward)))
                .role(Role::Button)
                .aria_label(if forward {
                    "向右滚动标签"
                } else {
                    "向左滚动标签"
                })
                .a11y_synthetic_children(move |builder| {
                    if !enabled {
                        builder.parent_node().set_disabled();
                    }
                })
                .w(px(TAB_SCROLL_ARROW_SIZE))
                .h(px(TAB_SCROLL_ARROW_SIZE))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(rgb(0xa0a0a0))
                .bg(rgb(0xf0f0f0))
                .text_color(rgb(if enabled { 0x000000 } else { 0xa0a0a0 }))
                .text_size(px(9.))
                .child(if forward { "▶" } else { "◀" })
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.tab_scroll_repeat = None),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.tab_scroll_repeat = None),
                )
                .when(enabled, |button| {
                    button
                        .hover(|style| style.bg(rgb(0xe5f1fb)).border_color(rgb(0x0078d7)))
                        .active(|style| style.bg(rgb(0xcce4f7)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _, window, cx| {
                                view.scroll_tabs(pane, forward, cx);
                                let scroll = if pane == view.pane {
                                    view.tab_scroll.clone()
                                } else {
                                    view.other_pane.tab_scroll.clone()
                                };
                                let executor = cx.background_executor().clone();
                                view.tab_scroll_repeat =
                                    Some(cx.spawn_in(window, async move |view, cx| {
                                        executor.timer(std::time::Duration::from_millis(500)).await;
                                        loop {
                                            let moved = view.update_in(cx, |view, window, cx| {
                                                if !window.is_window_active()
                                                    || !view.tab_overflow[pane as usize]
                                                    || view.tab_multiline
                                                    || view.tab_vertical
                                                    || view
                                                        .workspace
                                                        .active_id_in_view(pane)
                                                        .is_none()
                                                    || view.menu_bar.mode() != MenuMode::Closed
                                                    || view.file_busy
                                                    || view.close_dialog
                                                    || view.save_all_dialog
                                                    || view.reload_target.is_some()
                                                {
                                                    return false;
                                                }
                                                let bounds = scroll.bounds();
                                                let arrow = Bounds::new(
                                                    gpui::point(
                                                        bounds.right()
                                                            + px(if forward {
                                                                TAB_SCROLL_ARROW_SIZE
                                                            } else {
                                                                0.
                                                            }),
                                                        bounds.top() + px(TAB_SCROLL_ARROW_TOP),
                                                    ),
                                                    size(
                                                        px(TAB_SCROLL_ARROW_SIZE),
                                                        px(TAB_SCROLL_ARROW_SIZE),
                                                    ),
                                                );
                                                !arrow.contains(&window.mouse_position())
                                                    || view.scroll_tabs(pane, forward, cx)
                                            });
                                            if !matches!(moved, Ok(true)) {
                                                break;
                                            }
                                            executor
                                                .timer(std::time::Duration::from_millis(50))
                                                .await;
                                        }
                                    }));
                                cx.stop_propagation();
                            }),
                        )
                        .on_click(cx.listener(move |view, event, _, cx| {
                            if !matches!(event, gpui::ClickEvent::Mouse(_)) {
                                view.scroll_tabs(pane, forward, cx);
                            }
                            cx.stop_propagation();
                        }))
                })
        });
        div()
            .on_children_prepainted(move |children, window, _| {
                let width = children
                    .iter()
                    .map(|bounds| bounds.size.width)
                    .fold(px(0.), |sum, width| sum + width);
                let overflow =
                    observed_scroll.bounds().size.width + observed_scroll.max_offset().x > width;
                if overflow != overflowing {
                    let owner = owner.clone();
                    window.on_next_frame(move |_, cx| {
                        owner.update(cx, |view, cx| {
                            if view.tab_overflow[pane as usize] != overflow {
                                view.tab_overflow[pane as usize] = overflow;
                                cx.notify();
                            }
                        });
                    });
                }
            })
            .flex()
            .relative()
            .w_full()
            .h(px(27.))
            .flex_shrink_0()
            .child(tabs)
            .child(capture_scroll)
            .on_scroll(
                cx.listener(move |view, event: &gpui::ScrollEvent, window, cx| {
                    view.tab_wheel(pane, draggable, event, wheel_offset.get(), window, cx);
                }),
            )
            .when(overflowing, |row| {
                row.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .mt(px(TAB_SCROLL_ARROW_TOP))
                        .children(arrows),
                )
            })
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
        cx.simulate_input("moved");
        let selection = view.read_with(cx, |view, _| view.editor().selections());
        let target = cx
            .debug_bounds("buffer-tab-14")
            .expect("visible drag target");
        let origin = gpui::point(selected.left() + px(35.), selected.center().y);
        let destination = gpui::point(target.left() + px(35.), target.center().y);
        let windows = cx.windows().len();
        cx.simulate_event(gpui::MouseDownEvent {
            position: origin,
            button: MouseButton::Left,
            ..Default::default()
        });
        cx.simulate_mouse_move(
            origin + gpui::point(px(12.), px(0.)),
            Some(MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_move(
            destination,
            Some(MouseButton::Left),
            gpui::Modifiers::default(),
        );
        cx.simulate_event(gpui::MouseUpEvent {
            position: destination,
            button: MouseButton::Left,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(15));
            assert_eq!(
                &view.workspace.view_buffer_ids(view.pane)[..2],
                &[BufferId(14), BufferId(15)]
            );
            assert_eq!(view.editor().committed_text(), "moved");
            assert_eq!(view.editor().selections(), selection);
            assert!(view.editor().document().can_undo() && view.workspace.active().is_modified());
            let other = view
                .workspace
                .buffers()
                .iter()
                .find(|buffer| buffer.id() == BufferId(14))
                .expect("unchanged drop target");
            assert_eq!(other.editor.committed_text(), "");
            assert!(!other.editor.document().can_undo() && !other.is_modified());
        });
        let moved = cx.debug_bounds("buffer-tab-15").expect("moved tab");
        assert!(moved.left() >= strip.left() && moved.right() <= strip.right());
        assert_eq!(cx.windows().len(), windows);
        assert!(
            !cx.did_prompt_for_paths() && !cx.did_prompt_for_new_path() && !cx.has_pending_prompt()
        );
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| assert_eq!(view.editor().committed_text(), ""));
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
        let click_pin = |cx: &mut gpui::VisualTestContext, pinned: bool| {
            let position = cx
                .debug_bounds("buffer-tab-pin-0-15")
                .expect("target pin control")
                .center();
            let windows = cx.windows().len();
            cx.simulate_event(gpui::MouseDownEvent {
                position,
                button: MouseButton::Left,
                ..Default::default()
            });
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().id(), BufferId(15));
                assert_eq!(view.workspace.buffers().len(), 15);
                assert_eq!(view.workspace.active().is_pinned(), pinned);
            });
            assert_eq!(cx.windows().len(), windows);
            cx.simulate_event(gpui::MouseUpEvent {
                position,
                button: MouseButton::Left,
                ..Default::default()
            });
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().is_pinned(), !pinned);
                assert_eq!(view.workspace.buffers().len(), 15);
            });
            assert_eq!(cx.windows().len(), windows);
        };
        click_pin(cx, false);
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
        click_pin(cx, true);
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
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                    position: gpui::point(px(40.), px(90.)),
                    button: MouseButton::Left,
                    ..Default::default()
                }),
                cx,
            );
            window.draw(cx).clear(cx);
        })
        .unwrap();
        app.run_until_parked();
        assert_eq!(
            capture(&mut app),
            normal,
            "outside release keeps normal artwork"
        );
        handle
            .read_with(&app, |view, _| {
                assert_eq!(view.workspace.buffers().len(), 1);
                assert_eq!(view.workspace.active().id(), BufferId(1));
                assert_eq!(view.editor().committed_text(), "");
                assert!(!view.workspace.active().is_modified());
            })
            .unwrap();
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
        let files = tempfile::tempdir().expect("three named tab states");
        let mut app = gpui::HeadlessAppContext::with_platform(
            std::sync::Arc::new(gpui::NoopTextSystem),
            std::sync::Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        for dark in [false, true] {
            for (name, text) in [("a.txt", "A"), ("b.txt", "A\t中😀\r\nZ"), ("c.txt", "C")] {
                std::fs::write(files.path().join(name), text).expect("saved tab fixture");
            }
            let icons = app
                .open_window(size(px(1080.), px(668.)), |window, cx| {
                    cx.new(|cx| {
                        let mut view = MatrixWindow::new(dark, window, cx);
                        let empty = view.workspace.active().id();
                        for name in ["a.txt", "b.txt", "c.txt"] {
                            let (file, text) =
                                crate::files::DiskFile::load_utf8(&files.path().join(name))
                                    .expect("named tab source");
                            view.workspace
                                .open_buffer(file, &text)
                                .expect("tab document");
                        }
                        view.workspace
                            .close_buffer(empty, crate::workspace::CloseDisposition::IfUnmodified)
                            .expect("remove unused fixture tab");
                        view.workspace
                            .set_pinned(BufferId(2), true)
                            .expect("pinned first document");
                        view.reset_active_view(window, cx);
                        view
                    })
                })
                .expect("tab state scene");
            let capture_icon = |app: &mut gpui::HeadlessAppContext| {
                for _ in 0..3 {
                    app.update_window(icons.into(), |_, window, cx| window.draw(cx).clear(cx))
                        .expect("draw source tab state");
                    app.run_until_parked();
                }
                let origin = app
                    .update_window(icons.into(), |_, window, _| {
                        let colour: gpui::Background =
                            rgb(if dark { 0x80d4b2 } else { 0xfaaa3c }).into();
                        window
                            .painted_quads()
                            .iter()
                            .find(|quad| {
                                quad.background == colour
                                    && quad.bounds.size.height
                                        == px(4.).scale(window.scale_factor())
                            })
                            .expect("source active indicator locates rendered tab")
                            .bounds
                            .origin
                    })
                    .expect("rendered active tab");
                let image = app
                    .capture_screenshot(icons.into())
                    .expect("actual tab artwork");
                let scale = image.width() / 1080;
                let x = origin.x.as_f32() as u32 + 5 * scale;
                let y = origin.y.as_f32() as u32 + 3 * scale;
                assert!(x + 16 * scale <= image.width() && y + 16 * scale <= image.height());
                let mut pixels = Vec::new();
                for dy in 0..16 * scale {
                    for dx in 0..16 * scale {
                        pixels.push(image.get_pixel(x + dx, y + dy).0);
                    }
                }
                pixels
            };
            let saved = capture_icon(&mut app);
            icons
                .update(&mut app, |view, _, cx| {
                    assert_eq!(view.workspace.active().id(), BufferId(4));
                    view.workspace.active_mut().set_user_read_only(true);
                    cx.notify();
                })
                .expect("application readonly fixture");
            let readonly = capture_icon(&mut app);
            assert!(
                readonly != saved,
                "readonly tab must paint its distinct source document artwork, dark={dark}"
            );
            if dark {
                assert!(
                    readonly.iter().any(|pixel| pixel[..3] == [222, 222, 222]),
                    "source dark readonly opaque fill"
                );
            }
            let position = app
                .update_window(icons.into(), |_, window, _| {
                    let colour: gpui::Background =
                        rgb(if dark { 0x202020 } else { 0xc0c0c0 }).into();
                    let scale = window.scale_factor();
                    let mut tabs = window
                        .painted_quads()
                        .into_iter()
                        .filter(|quad| {
                            quad.background == colour
                                && quad.bounds.size.height == px(20.).scale(scale)
                                && quad.bounds.origin.y >= px(76.).scale(scale)
                                && quad.bounds.origin.y < px(104.).scale(scale)
                        })
                        .collect::<Vec<_>>();
                    tabs.sort_by(|left, right| {
                        left.bounds
                            .origin
                            .x
                            .partial_cmp(&right.bounds.origin.x)
                            .expect("finite tab bounds")
                    });
                    let center = tabs[1].bounds.center();
                    gpui::point(px(center.x.as_f32() / scale), px(center.y.as_f32() / scale))
                })
                .expect("second inactive tab");
            app.update_window(icons.into(), |_, window, cx| {
                window.dispatch_event(
                    gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                        position,
                        button: MouseButton::Left,
                        ..Default::default()
                    }),
                    cx,
                );
                window.dispatch_event(
                    gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                        position,
                        button: MouseButton::Left,
                        ..Default::default()
                    }),
                    cx,
                );
            })
            .expect("activate second named tab");
            let saved = capture_icon(&mut app);
            if dark {
                assert!(
                    saved.iter().enumerate().all(|(index, pixel)| pixel[..3]
                        == if index < (16 * scale * scale) as usize {
                            [128, 212, 178]
                        } else {
                            [56, 56, 56]
                        }),
                    "source saved dark icon preserves the underlying indicator and tab body"
                );
            }
            icons
                .update(&mut app, |view, window, cx| {
                    assert_eq!(view.workspace.active().id(), BufferId(3));
                    gpui::EntityInputHandler::replace_text_in_range(
                        view,
                        Some(8..8),
                        "X",
                        window,
                        cx,
                    );
                })
                .expect("actual native edit of second document");
            assert!(
                capture_icon(&mut app) != saved,
                "edit paints modified source artwork"
            );
            let capture_status = |app: &mut gpui::HeadlessAppContext, state: &str| {
                for _ in 0..3 {
                    app.update_window(icons.into(), |_, window, cx| window.draw(cx).clear(cx))
                        .expect("draw visible status transition");
                    app.run_until_parked();
                }
                let file = tempfile::Builder::new()
                    .prefix(&format!("113-{state}-"))
                    .suffix(".png")
                    .tempfile()
                    .expect("private status artifact");
                let (_, path) = file.keep().expect("retain unique status image");
                app.capture_screenshot(icons.into())
                    .expect("native text status pixels")
                    .save(path)
                    .expect("save private status image");
            };
            if !dark {
                capture_status(&mut app, "input");
            }
            app.update_window(icons.into(), |_, window, cx| {
                let position = gpui::point(px(59.5), px(63.5));
                window.dispatch_event(
                    gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                        position,
                        button: MouseButton::Left,
                        ..Default::default()
                    }),
                    cx,
                );
                window.dispatch_event(
                    gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                        position,
                        button: MouseButton::Left,
                        ..Default::default()
                    }),
                    cx,
                );
            })
            .expect("actual Save route");
            assert!(
                capture_icon(&mut app) == saved,
                "Save restores same document artwork"
            );
            assert_eq!(
                std::fs::read(files.path().join("b.txt")).expect("saved tab document"),
                "A\t中😀\r\nZX".as_bytes()
            );
            icons
                .read_with(&app, |view, _| {
                    assert_eq!(view.workspace.buffers().len(), 3);
                    assert_eq!(view.workspace.active().id(), BufferId(3));
                    assert!(!view.workspace.active().is_modified());
                    assert!(view.workspace.buffers()[0].is_pinned());
                    assert_eq!(view.workspace.buffers()[0].editor.committed_text(), "A");
                    assert_eq!(view.workspace.buffers()[2].editor.committed_text(), "C");
                    assert!(view.workspace.buffers()[2].is_user_read_only());
                })
                .expect("all tab states retain exact document identity");
            let unchanged = |view: &MatrixWindow| {
                (
                    view.workspace.active().id(),
                    view.workspace.active_view(),
                    view.workspace
                        .buffers()
                        .iter()
                        .map(|buffer| {
                            (
                                buffer.id(),
                                buffer.editor.committed_text(),
                                buffer.editor.selections(),
                                buffer.editor.document().can_undo(),
                                buffer.editor.document().can_redo(),
                                buffer.is_modified(),
                                buffer.is_pinned(),
                                buffer.is_user_read_only(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            };
            let before = icons
                .read_with(&app, |view, _| unchanged(view))
                .expect("tab hover owner baseline");
            if !dark {
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_action(Box::new(OpenEditMenu), cx)
                })
                .expect("actual Edit root for initial EOL flags");
                for _ in 0..3 {
                    app.update_window(icons.into(), |_, window, cx| window.draw(cx).clear(cx))
                        .expect("draw Edit menu");
                    app.run_until_parked();
                }
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_event(
                        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("e").expect("source EOL mnemonic"),
                            is_held: false,
                            prefer_character_input: false,
                        }),
                        cx,
                    );
                })
                .expect("native EOL mnemonic");
                capture_status(&mut app, "eol-initial-flags");
                let popup = app
                    .update_window(icons.into(), |_, window, _| {
                        window
                            .painted_quads()
                            .iter()
                            .find(|quad| {
                                quad.bounds.size.width == px(240.).scale(window.scale_factor())
                                    && quad.bounds.size.height
                                        == px(74.).scale(window.scale_factor())
                                    && quad.background == rgb(0xf0f0f0).into()
                            })
                            .expect("actual three-row EOL popup")
                            .bounds
                    })
                    .expect("rendered EOL submenu region");
                let image = app
                    .capture_screenshot(icons.into())
                    .expect("actual EOL marker gutter");
                let scale = image.width() / 1080;
                let left = popup.left().as_f32() as u32;
                let top = popup.top().as_f32() as u32;
                for y in 1..25 * scale {
                    let background = image.get_pixel(left + 4 * scale, top + y).0;
                    for x in 9 * scale..21 * scale {
                        assert_eq!(
                            image.get_pixel(left + x, top + y).0,
                            background,
                            "source current EOL has no check glyph"
                        );
                    }
                }
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_action(Box::new(MenuEscape), cx);
                    window.dispatch_action(Box::new(MenuEscape), cx);
                    window.dispatch_action(Box::new(MenuEscape), cx);
                })
                .expect("dismiss source presentation without conversion");
                icons
                    .update(&mut app, |view, window, _| {
                        assert_eq!(view.menu_bar.mode(), MenuMode::Closed);
                        assert!(view.editor_focus.is_focused(window));
                    })
                    .expect("all normal submenu layers dismissed to the editor");
                icons
                    .read_with(&app, |view, _| assert_eq!(unchanged(view), before))
                    .expect("menu presentation preserves all document owners");
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_action(Box::new(OpenEncodingMenu), cx)
                })
                .expect("actual Encoding root");
                capture_status(&mut app, "encoding-initial-flags");
                let popup = app
                    .update_window(icons.into(), |_, window, _| {
                        window
                            .painted_quads()
                            .iter()
                            .find(|quad| {
                                quad.bounds.size.width == px(240.).scale(window.scale_factor())
                                    && quad.bounds.size.height
                                        > px(100.).scale(window.scale_factor())
                                    && quad.background == rgb(0xf0f0f0).into()
                            })
                            .expect("actual Encoding popup region")
                            .bounds
                    })
                    .expect("rendered encoding marker fields");
                let image = app
                    .capture_screenshot(icons.into())
                    .expect("actual encoding check markers");
                let scale = image.width() / 1080;
                let left = popup.left().as_f32() as u32;
                let top = popup.top().as_f32() as u32;
                for row in 0..4 {
                    let mut ink = false;
                    for y in (1 + row * 24) * scale..(25 + row * 24) * scale {
                        let background = image.get_pixel(left + 4 * scale, top + y).0;
                        for x in 9 * scale..21 * scale {
                            ink |= image.get_pixel(left + x, top + y).0 != background;
                        }
                    }
                    assert!(
                        !ink || row == 0,
                        "non-current source Unicode encoding choices have no marker"
                    );
                }
                icons
                    .read_with(&app, |view, _| {
                        assert_eq!(
                            view.workspace.active().format(),
                            crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain)
                        );
                        assert_eq!(
                            view.menu_bar.definitions()[view.menu_bar.active_root()].selector,
                            "encoding-menu-trigger"
                        );
                        assert_eq!(unchanged(view), before);
                    })
                    .expect("encoding marker belongs to unchanged current document");
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_action(Box::new(MenuEscape), cx);
                    window.dispatch_action(Box::new(MenuEscape), cx);
                })
                .expect("dismiss Encoding without selecting or converting");
                icons
                    .update(&mut app, |view, window, _| {
                        assert_eq!(view.menu_bar.mode(), MenuMode::Closed);
                        assert!(view.editor_focus.is_focused(window));
                        assert_eq!(unchanged(view), before);
                    })
                    .expect("encoding dismissal preserves owning editor");
            }
            let targets = app
                .update_window(icons.into(), |_, window, _| {
                    let scale = window.scale_factor();
                    let mut tabs = window
                        .painted_quads()
                        .into_iter()
                        .filter(|quad| {
                            let fill = quad.background;
                            [
                                if dark { 0x202020 } else { 0xc0c0c0 },
                                if dark { 0x383838 } else { 0xf0f0f0 },
                            ]
                            .into_iter()
                            .any(|colour| fill == rgb(colour).into())
                                && [20., 24.].into_iter().any(|height| {
                                    quad.bounds.size.height == px(height).scale(scale)
                                })
                                && quad.bounds.top() >= px(76.).scale(scale)
                                && quad.bounds.top() < px(104.).scale(scale)
                        })
                        .map(|quad| quad.bounds)
                        .collect::<Vec<_>>();
                    tabs.sort_by(|left, right| {
                        left.left()
                            .partial_cmp(&right.left())
                            .expect("finite shared tab positions")
                    });
                    assert_eq!(tabs.len(), 3);
                    let button = if dark { 16. } else { 11. };
                    let gap = if dark { 4. } else { 8. };
                    let target = |index: usize, pin: bool| {
                        let bounds = tabs[index];
                        let right = bounds.right().as_f32() / scale;
                        let y = bounds.center().y.as_f32() / scale;
                        let padding = if index == 1 { 6. } else { 4. };
                        gpui::point(
                            px(right - padding - button / 2. - if pin { button + gap } else { 0. }),
                            px(y),
                        )
                    };
                    [
                        gpui::point(
                            px(tabs[0].left().as_f32() / scale + 20.),
                            px(tabs[0].center().y.as_f32() / scale),
                        ),
                        target(0, true),
                        target(1, true),
                        target(1, false),
                    ]
                })
                .expect("shared tab control pointer targets");
            let outside = gpui::point(px(-1.), px(-1.));
            for (name, position, down, held, release) in [
                ("normal", outside, false, false, false),
                ("pinned-body", targets[0], false, false, false),
                ("pinned-hover", targets[1], false, false, false),
                ("pinned-down", targets[1], true, false, false),
                ("pinned-cancel", outside, false, true, true),
                ("unpinned-hover", targets[2], false, false, false),
                ("close-hover", targets[3], false, false, false),
                ("close-down", targets[3], true, false, false),
                ("close-cancel", outside, false, true, true),
            ] {
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_event(
                        gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position,
                            pressed_button: held.then_some(MouseButton::Left),
                            ..Default::default()
                        }),
                        cx,
                    );
                    if down {
                        window.dispatch_event(
                            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                                position,
                                button: MouseButton::Left,
                                ..Default::default()
                            }),
                            cx,
                        );
                    }
                    if release {
                        window.dispatch_event(
                            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                                position,
                                button: MouseButton::Left,
                                ..Default::default()
                            }),
                            cx,
                        );
                    }
                })
                .expect("actual tab control pointer lifecycle");
                capture_status(
                    &mut app,
                    &format!("controls-{}-{name}", if dark { "dark" } else { "light" }),
                );
                if dark && matches!(name, "pinned-hover" | "pinned-down") {
                    let image = app
                        .capture_screenshot(icons.into())
                        .expect("source pin hover pixels");
                    let scale = image.width() / 1080;
                    let x = ((f32::from(targets[1].x) - 8. + 5.5) * scale as f32) as u32;
                    let y = ((f32::from(targets[1].y) - 8. + 2.5) * scale as f32) as u32;
                    assert_eq!(
                        image.get_pixel(x, y).0[..3],
                        [47, 47, 47],
                        "source unpin transparent region must not retain pinned artwork"
                    );
                }
                icons
                    .read_with(&app, |view, _| {
                        assert_eq!(
                            unchanged(view),
                            before,
                            "hover/press/cancel cannot activate or mutate another tab: {name}"
                        )
                    })
                    .expect("all document ownership states unchanged");
            }
            if dark {
                capture_status(&mut app, "dark-captions");
            } else {
                let source_bar = |app: &mut gpui::HeadlessAppContext, colour| {
                    icons
                        .update(app, |view, window, cx| {
                            let scale = window.scale_factor();
                            let row = gpui::EntityInputHandler::bounds_for_range(
                                view,
                                9..9,
                                Bounds::default(),
                                window,
                                cx,
                            )
                            .expect("second row native geometry");
                            let row = window.pixel_snap_bounds(row).scale(scale);
                            let fill: gpui::Background = rgb(colour).into();
                            window.painted_quads().iter().any(|quad| {
                                quad.background == fill
                                    && quad.bounds.size.width == px(3.).scale(scale)
                                    && quad.bounds.size.height == px(26.).scale(scale)
                                    && quad.bounds.left() < px(72.).scale(scale)
                                    && quad.bounds.top() == row.top()
                            })
                        })
                        .expect("actual second-row source history bar")
                };
                icons
                    .update(&mut app, |view, window, cx| {
                        gpui::EntityInputHandler::replace_text_in_range(
                            view, None, "Q", window, cx,
                        );
                        assert_eq!(view.editor().committed_text(), "A\t中😀\r\nZXQ");
                        assert!(
                            view.editor().document().is_modified()
                                && view.workspace.active().is_modified()
                        );
                    })
                    .expect("edit after actual Save baseline");
                assert!(capture_icon(&mut app) != saved);
                assert!(source_bar(&mut app, 0xff8000));
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_action(Box::new(EditorUndo), cx)
                })
                .expect("Undo TO savepoint");
                icons
                    .read_with(&app, |view, _| {
                        assert_eq!(view.editor().committed_text(), "A\t中😀\r\nZX");
                        assert_eq!(
                            view.editor().selection(),
                            Selection {
                                anchor: ByteOffset(13),
                                head: ByteOffset(13)
                            }
                        );
                        assert!(!view.editor().document().is_modified());
                        assert!(!view.workspace.active().is_modified());
                        assert!(view.editor().document().can_redo());
                    })
                    .expect("engine and application cleanliness at the same savepoint");
                assert!(capture_icon(&mut app) == saved);
                assert!(source_bar(&mut app, 0x00a000));
                assert!(!source_bar(&mut app, 0xff8000));
                capture_status(&mut app, "savepoint-restored");
                for reversed in [false, true] {
                    icons
                        .update(&mut app, |view, window, cx| {
                            gpui::EntityInputHandler::set_selected_text_range(
                                view,
                                if reversed { 5..5 } else { 0..5 },
                                window,
                                cx,
                            );
                        })
                        .expect("native replacement selection");
                    if reversed {
                        app.update_window(icons.into(), |_, window, cx| {
                            for _ in 0..4 {
                                window.dispatch_action(Box::new(EditorSelectLeft), cx);
                            }
                        })
                        .expect("reverse Unicode selection through normal actions");
                    }
                    let selection = Selection {
                        anchor: ByteOffset(if reversed { 9 } else { 0 }),
                        head: ByteOffset(if reversed { 0 } else { 9 }),
                    };
                    icons
                        .update(&mut app, |view, window, cx| {
                            assert_eq!(view.editor().selection(), selection);
                            gpui::EntityInputHandler::replace_text_in_range(
                                view, None, "文", window, cx,
                            );
                            assert_eq!(view.editor().committed_text(), "文\r\nZX");
                            assert_eq!(
                                view.editor().selection(),
                                Selection {
                                    anchor: ByteOffset(3),
                                    head: ByteOffset(3)
                                }
                            );
                            assert_eq!(
                                gpui::EntityInputHandler::selected_text_range(
                                    view, false, window, cx
                                )
                                .expect("replacement native caret")
                                .range,
                                1..1
                            );
                            assert!(view.workspace.active().is_modified());
                        })
                        .expect("one native callback replaces only the selected source once");
                    capture_status(
                        &mut app,
                        if reversed {
                            "reverse-replacement"
                        } else {
                            "forward-replacement"
                        },
                    );
                    app.update_window(icons.into(), |_, window, cx| {
                        window.dispatch_action(Box::new(EditorUndo), cx)
                    })
                    .expect("restore existing baseline");
                    icons
                        .read_with(&app, |view, _| {
                            assert_eq!(view.editor().committed_text(), "A\t中😀\r\nZX");
                            assert_eq!(view.editor().selection(), selection);
                            assert!(!view.workspace.active().is_modified());
                        })
                        .expect("Undo restores saved Unicode bytes and directional selection");
                }
                icons
                    .update(&mut app, |view, window, cx| {
                        gpui::EntityInputHandler::set_selected_text_range(view, 0..5, window, cx);
                        assert_eq!(
                            view.editor().selection(),
                            Selection {
                                anchor: ByteOffset(0),
                                head: ByteOffset(9)
                            }
                        );
                        assert_eq!(view.editor().committed_text(), "A\t中😀\r\nZX");
                        assert!(!view.workspace.active().is_modified());
                    })
                    .expect("native Unicode selection");
                capture_status(&mut app, "selection");
                app.update_window(icons.into(), |_, window, cx| {
                    window.dispatch_action(Box::new(CloneToOtherView), cx);
                    window.dispatch_action(Box::new(EditorDocumentEnd), cx);
                })
                .expect("distinct secondary pane caret");
                capture_icon(&mut app);
                let click_tab = |app: &mut gpui::HeadlessAppContext, secondary: bool| {
                    let position = app
                        .update_window(icons.into(), |_, window, _| {
                            let scale = window.scale_factor();
                            let fill: gpui::Background =
                                rgb(if secondary { 0xf0f0f0 } else { 0xc0c0c0 }).into();
                            let mut tabs = window
                                .painted_quads()
                                .into_iter()
                                .filter(|quad| {
                                    quad.background == fill
                                        && quad.bounds.size.height
                                            == px(if secondary { 24. } else { 20. }).scale(scale)
                                        && quad.bounds.top() >= px(76.).scale(scale)
                                        && quad.bounds.top() < px(104.).scale(scale)
                                        && (quad.bounds.left() > px(540.).scale(scale)) == secondary
                                })
                                .collect::<Vec<_>>();
                            tabs.sort_by(|left, right| {
                                left.bounds
                                    .left()
                                    .partial_cmp(&right.bounds.left())
                                    .expect("finite tab positions")
                            });
                            let center = tabs.last().expect("actual target tab").bounds.center();
                            gpui::point(
                                px(center.x.as_f32() / scale),
                                px(center.y.as_f32() / scale),
                            )
                        })
                        .expect("painted pane tab target");
                    app.update_window(icons.into(), |_, window, cx| {
                        window.dispatch_event(
                            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                                position,
                                button: MouseButton::Left,
                                ..Default::default()
                            }),
                            cx,
                        );
                        window.dispatch_event(
                            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                                position,
                                button: MouseButton::Left,
                                ..Default::default()
                            }),
                            cx,
                        );
                    })
                    .expect("actual pane tab click");
                };
                click_tab(&mut app, false);
                icons
                    .read_with(&app, |view, _| {
                        assert_eq!(
                            view.workspace.active_view(),
                            crate::workspace::ViewId::Primary
                        );
                        assert_eq!(view.workspace.active().id(), BufferId(4));
                        assert_eq!(view.editor().committed_text(), "C");
                    })
                    .expect("document switch identity");
                capture_status(&mut app, "document-switch");
                click_tab(&mut app, true);
                icons
                    .read_with(&app, |view, _| {
                        assert_eq!(
                            view.workspace.active_view(),
                            crate::workspace::ViewId::Secondary
                        );
                        assert_eq!(view.workspace.active().id(), BufferId(3));
                        assert_eq!(view.editor().selection().head, ByteOffset(13));
                        assert_eq!(view.editor().committed_text(), "A\t中😀\r\nZX");
                        assert!(!view.workspace.active().is_modified());
                        let primary = view
                            .workspace
                            .editor_in_view(crate::workspace::ViewId::Primary, BufferId(3))
                            .expect("primary B selection");
                        assert_eq!(
                            primary.selection(),
                            Selection {
                                anchor: ByteOffset(0),
                                head: ByteOffset(9)
                            }
                        );
                    })
                    .expect("pane switch restores distinct owner selection");
                capture_status(&mut app, "pane-switch");
            }
        }
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
        for target in [Some("buffer-tab-1"), Some("buffer-tab-2"), None] {
            let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
            cx.dispatch_action(NewFile);
            cx.dispatch_action(NewFile);
            let position = cx.debug_bounds("buffer-tab-1").expect("first tab").center();
            let release = match target {
                Some(id) => cx.debug_bounds(id).expect("release tab").center(),
                None => cx.debug_bounds("matrix-editor").expect("editor").center(),
            };
            cx.simulate_event(gpui::MouseDownEvent {
                position,
                button: MouseButton::Middle,
                click_count: 1,
                ..Default::default()
            });
            for id in ["buffer-tab-1", "buffer-tab-2", "buffer-tab-3"] {
                assert!(cx.debug_bounds(id).is_some());
            }
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().id(), BufferId(3));
            });
            cx.simulate_event(gpui::MouseUpEvent {
                position: release,
                button: MouseButton::Middle,
                click_count: 1,
                ..Default::default()
            });
            for id in ["buffer-tab-1", "buffer-tab-2", "buffer-tab-3"] {
                assert_eq!(
                    cx.debug_bounds(id).is_none(),
                    target == Some(id),
                    "only release target closes: {target:?}, tab {id}"
                );
            }
            assert!(cx.debug_bounds("close-cancel").is_none());
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("unsaved");
        cx.dispatch_action(NewFile);
        let snapshot = |view: &MatrixWindow| {
            view.workspace
                .buffers()
                .iter()
                .map(|buffer| {
                    (
                        buffer.id(),
                        buffer.editor.committed_text(),
                        buffer.editor.selections(),
                        buffer.editor.document().can_undo(),
                        buffer.editor.document().can_redo(),
                        buffer.is_modified(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let before = view.read_with(cx, |view, _| snapshot(view));
        let windows = cx.windows();
        let position = cx
            .debug_bounds("buffer-tab-1")
            .expect("dirty release target")
            .center();
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: MouseButton::Middle,
            ..Default::default()
        });
        assert_eq!(cx.windows(), windows);
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), BufferId(2));
            assert_eq!(snapshot(view), before);
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position,
            button: MouseButton::Middle,
            ..Default::default()
        });
        assert_eq!(cx.windows().len(), windows.len() + 1);
        view.read_with(cx, |view, _| {
            assert_eq!(view.close_target, Some(BufferId(1)));
            assert_eq!(snapshot(view), before);
        });
        let confirmation = cx
            .windows()
            .into_iter()
            .find(|handle| !windows.contains(handle))
            .expect("single dirty release confirmation");
        let mut dialog = gpui::VisualTestContext::from_window(confirmation, cx);
        let cancel = dialog.debug_bounds("close-cancel").expect("visible Cancel");
        dialog.simulate_click(cancel.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows(), windows);
        view.update_in(cx, |view, window, _| {
            assert_eq!(view.workspace.active().id(), BufferId(1));
            assert_eq!(snapshot(view), before);
            assert!(view.editor_focus.is_focused(window));
        });
        cx.simulate_input("!");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.workspace.buffers()[0].editor.committed_text(),
                "unsaved!"
            );
            assert_eq!(view.workspace.buffers()[1].editor.committed_text(), "");
        });
    }
}
