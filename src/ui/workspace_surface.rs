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
    pub(super) fn actual_first_line_edit(
        &mut self,
        id: BufferId,
        modified_line: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.workspace.buffers().iter().find(|buffer| buffer.id() == id) else {
            return;
        };
        let old_name = buffer.name();
        if self.workspace.use_first_line_as_tab_name(id, modified_line) {
            self.synchronize_renamed_tab_backup(id, &old_name, cx);
            cx.notify();
        }
    }

    fn scrollable_tab_index(&self, pane: ViewId, id: BufferId) -> Option<usize> {
        self.workspace
            .view_buffer_ids(pane)
            .iter()
            .filter(|id| {
                self.workspace
                    .buffers()
                    .iter()
                    .any(|buffer| buffer.id() == **id && !buffer.is_pinned())
            })
            .position(|candidate| *candidate == id)
    }

    fn scrollable_tab_count(&self, pane: ViewId) -> usize {
        self.workspace
            .view_buffer_ids(pane)
            .iter()
            .filter(|id| {
                self.workspace
                    .buffers()
                    .iter()
                    .any(|buffer| buffer.id() == **id && !buffer.is_pinned())
            })
            .count()
    }

    pub(super) fn scroll_active_tab_into_view(&self) {
        if let Some(index) = self.scrollable_tab_index(self.pane, self.workspace.active().id()) {
            self.tab_scroll.scroll_to_item(index);
        }
    }

    fn scroll_tabs(&mut self, pane: ViewId, forward: bool, cx: &mut Context<Self>) -> bool {
        let scroll = if pane == self.pane {
            &self.tab_scroll
        } else {
            &self.other_pane.tab_scroll
        };
        let viewport = scroll.bounds();
        let offset = scroll.offset();
        let count = self.scrollable_tab_count(pane);
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
            self.scroll_active_tab_into_view();
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
            || !self.pref_bool("TabBar.dragAndDrop", true)
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
                    self.scroll_active_tab_into_view();
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
        self.initialize_active_change_history();
        self.update_file_prompt_directory();
        let overwrite = self.overwrite_mode;
        self.editor_mut().set_overwrite(overwrite);
        if self.monitoring_active() {
            self.editor_mut().set_read_only(true);
        }
        self.scroll_active_tab_into_view();
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
        if matrix_surface::locked() {
            return false;
        }
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
        if self.pref_bool("TabBar.hide", false) {
            return div().w(px(0.)).h(px(0.)).into_any_element();
        }
        let dark = self.dark;
        let reduce = self.pref_bool("TabBar.reduce", true);
        let pin_button = self.pref_bool("TabBar.pinButton", true);
        let close_button = self.pref_bool("TabBar.closeButton", true);
        let only_pinned = self.pref_bool("TabBar.showOnlyPinnedButton", false);
        let inactive_buttons = self.pref_bool("TabBar.buttonsOninactiveTabs", false);
        let draw_top = self.pref_bool("TabBar.drawTopBar", true);
        let draw_inactive = self.pref_bool("TabBar.drawInactiveTab", true);
        let alternate = self.pref_int(
            if dark {
                "DarkMode.darkTabIconSet"
            } else {
                "DarkMode.lightTabIconSet"
            },
            if dark { 2 } else { 0 },
        ) == 1;
        let tab_extra_height = if reduce { 0. } else { 8. };
        let tab_text_size = px(if reduce { 14. } else { 17.5 });
        let pane = self.pane;
        let active = self.workspace.active().id();
        let background = self.appearance_colour("DarkMode.customColorMain", 0xf0f0f0);
        let selected = self.appearance_colour("DarkMode.customColorActive", 0xf0f0f0);
        let inactive = if !draw_inactive {
            selected
        } else if self.dark {
            background
        } else {
            self.style_colour("Inactive tabs", "bgColor", 0xc0c0c0)
        };
        let inactive_text = if self.dark {
            self.appearance_colour("DarkMode.customColorDarkText", 0xc0c0c0)
        } else {
            self.style_colour("Inactive tabs", "fgColor", 0x808080)
        };
        let active_text = if self.dark {
            self.appearance_colour("DarkMode.customColorText", 0xe0e0e0)
        } else {
            self.style_colour("Active tab text", "fgColor", 0x000000)
        };
        let button_size = if self.dark { 16. } else { 11. };
        let indicator = self.style_colour(
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
        let classic_frame = !dark && !vertical && !multiline;
        let strip_height = if classic_frame {
            if self.pref_bool("ScintillaPrimaryView.borderEdge", true) { 28. } else { 26. }
        } else {
            27.
        } + tab_extra_height;
        let draggable = !self.file_busy
            && self.pref_bool("TabBar.dragAndDrop", true)
            && !self.close_dialog
            && !self.save_all_dialog
            && !self.batch_editing_blocked();
        let owner = self.document_window;
        let dragging = cx.has_active_drag();
        let text_system = cx.text_system();
        let font = text_system.resolve_font(&gpui::font("Helvetica"));
        let text_descent = f32::from(text_system.descent(font, tab_text_size)).ceil() as i32;
        let space_width = f32::from(text_system.layout_width(font, tab_text_size, ' ')).round();
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
                                && let Some(index) = view.scrollable_tab_index(pane, active)
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
                "scrollable-buffer-tabs"
            } else {
                "scrollable-buffer-tabs-secondary"
            })
            .flex_shrink_0()
            .flex()
            .overflow_hidden()
            .when(vertical, |tabs| {
                tabs.flex_col()
                    .w(px(160.))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
            })
            .when(!vertical && !multiline, |tabs| {
                tabs.flex_1()
                    .min_w(px(0.))
                    .h(px(strip_height))
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
            .px(px(if classic_frame { 4. } else { 2. }))
            .when(!classic_frame, |tabs| tabs.bg(rgb(background)))
            .text_size(tab_text_size)
            .on_drop(cx.listener(move |view, drag: &TabDrag, window, cx| {
                view.drop_document_tab(drag, pane, None, false, window, cx);
                cx.stop_propagation();
            }));
        let rendered_tabs: Vec<_> = self
            .workspace
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
                let pinned = buffer.is_pinned();
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
                let tab_height: f32 = (if vertical {
                    27.
                } else if classic_frame {
                    if id == active { 26. } else { 22. }
                } else if id == active {
                    25.
                } else if pinned {
                    24.
                } else {
                    20.
                }) + tab_extra_height;
                let caption_pin_gap = if !dark && !vertical {
                    let drawing_height = if pinned || id == active {
                        18.
                    } else {
                        tab_height
                    };
                    let image_inset = ((drawing_height - 16. + 1.) / 2.).floor();
                    let button_inset = ((drawing_height - button_size + 1.) / 2.).floor();
                    3. * 16. + if pinned || id == active { -4. } else { 4. }
                        - image_inset
                        - button_inset
                        - 2. * button_size
                        - 8.
                        - space_width
                } else {
                    0.
                };
                let dirty = buffer.is_modified();
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
                let tab_group = gpui::SharedString::from(format!("tab-{}-{}", pane as usize, id.0));
                let pin_group =
                    gpui::SharedString::from(format!("tab-pin-{}-{}", pane as usize, id.0));
                let close_group =
                    gpui::SharedString::from(format!("tab-close-{}-{}", pane as usize, id.0));
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
                    self.style_colour(&name, "bgColor", colors[usize::from(color)])
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
                let tab = div()
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
                    } else if classic_frame {
                        if id == active { 0. } else { 2. }
                    } else if pinned || id == active {
                        2.
                    } else {
                        4.
                    }))
                    .when(vertical, |tab| tab.w_full())
                    .when(classic_frame && id == active, |tab| tab.ml(px(-2.)))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(0.))
                    .px(px(if (pinned || id == active) && !vertical {
                        5.
                    } else {
                        3.
                    }))
                    .border_1()
                    .when(!vertical && (classic_frame || id == active), |tab| tab.border_b_0())
                    .border_color(if classic_frame { gpui::rgba(0) } else { rgb(if dark { 0x646464 } else { 0x808080 }) })
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
                        cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                            view.set_tab_preview(None, cx);
                            if view.focus_editor_pane(pane, window, cx) {
                                if view.activate_buffer(id, window, cx)
                                    && event.click_count == 2
                                    && view.pref_bool("TabBar.doubleClick2Close", false)
                                {
                                    view.request_close_current(window, cx);
                                }
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
                        view.set_tab_preview(if *hovered { Some((pane, id)) } else { None }, cx);
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
                            .top(px(if classic_frame { -1. } else { 0. }))
                            .left(px(if classic_frame { -1. } else { 0. }))
                            .bottom(px(0.))
                            .w(px(1.))
                            .bg(rgb(if dark { 0x646464 } else { 0xffffff })),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(if classic_frame { -1. } else { 0. }))
                            .left(px(if classic_frame { -1. } else { 0. }))
                            .right(px(if classic_frame { -1. } else { 0. }))
                            .h(px(1.))
                            .bg(rgb(if dark { 0x646464 } else { 0xffffff })),
                    )
                    .when(classic_frame, |tab| {
                        tab.child(gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                let left = bounds.left();
                                let top = bounds.top();
                                let right = bounds.right();
                                let side_height = bounds.size.height - px(if id == active { 3. } else { 2. });
                                for (origin, extent, color) in [
                                    (gpui::point(left, top), size(bounds.size.width, px(2.)), background),
                                    (gpui::point(left + px(2.), top), size(bounds.size.width - px(4.), px(1.)), 0xffffff),
                                    (gpui::point(left + px(1.), top + px(1.)), size(bounds.size.width - px(2.), px(1.)), 0xffffff),
                                    (gpui::point(left + px(2.), top + px(1.)), size(bounds.size.width - px(4.), px(1.)), 0xe3e3e3),
                                    (gpui::point(right - px(2.), top + px(1.)), size(px(1.), px(1.)), 0x696969),
                                    (gpui::point(left, top + px(2.)), size(px(1.), side_height), 0xffffff),
                                    (gpui::point(left + px(1.), top + px(2.)), size(px(1.), side_height), 0xe3e3e3),
                                    (gpui::point(right - px(2.), top + px(2.)), size(px(1.), side_height), 0xa0a0a0),
                                    (gpui::point(right - px(1.), top + px(2.)), size(px(1.), side_height), 0x696969),
                                ] {
                                    window.paint_quad(gpui::fill(Bounds::new(origin, extent), rgb(color)));
                                }
                                if id == active {
                                    window.paint_quad(gpui::fill(
                                        Bounds::new(gpui::point(left, bounds.bottom() - px(1.)), size(bounds.size.width, px(1.))),
                                        rgb(selected),
                                    ));
                                }
                            },
                        ).absolute().top(px(-1.)).left(px(-1.)).right(px(-1.)).bottom(px(0.)))
                    })
                    .when(id == active && draw_top, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .top(px(if classic_frame { 1. } else { 0. }))
                                .left(px(if classic_frame { 1. } else { 0. }))
                                .right(px(if classic_frame { 1. } else { 0. }))
                                .h(px(4.))
                                .bg(rgb(color.map_or(indicator, |colour| {
                                    tab_colour(colour, dark, focused, false)
                                }))),
                        )
                    })
                    .child(
                        gpui::img(icon.tab_image(alternate))
                            .size(px(16.))
                            .flex_shrink_0(),
                    )
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
                            .line_height(px(20. + tab_extra_height))
                            .when(!vertical, |caption| {
                                caption.relative().top(px(2.
                                    + ((text_descent + i32::from(dark || id != active)) / 2)
                                        as f32))
                            })
                            .child(format!(" {caption}")),
                    )
                    .when(pin_button, |tab| {
                        tab.child(
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
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
                                        .opacity(
                                            if pinned
                                                || (!only_pinned
                                                    && (id == active || inactive_buttons))
                                            {
                                                1.
                                            } else {
                                                0.
                                            },
                                        )
                                        .when(!only_pinned, |icon| {
                                            icon.group_hover(tab_group.clone(), |style| {
                                                style.opacity(1.)
                                            })
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
                                                .group_hover(pin_group, |style| style.opacity(1.))
                                                .child(
                                                    gpui::img(pin_hover_icon.image()).size_full(),
                                                ),
                                        ),
                                ),
                        )
                    })
                    .when(close_button, |tab| {
                        tab.child(
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
                                        .opacity(if id == active || inactive_buttons {
                                            1.
                                        } else {
                                            0.
                                        })
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
                                                    gpui::img(close_hover_icon.image()).size_full(),
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
                    })
                    .into_any_element();
                (pinned, tab)
            })
            .collect();
        let mut pinned_tabs = Vec::new();
        let mut scrollable_tabs = Vec::new();
        for (pinned, tab) in rendered_tabs {
            if pinned {
                pinned_tabs.push(tab);
            } else {
                scrollable_tabs.push(tab);
            }
        }
        let pinned_tabs = div()
            .flex()
            .flex_shrink_0()
            .overflow_hidden()
            .when(vertical, |tabs| tabs.flex_col().w(px(160.)))
            .when(!vertical && multiline, |tabs| tabs.w_full().flex_wrap())
            .when(!vertical && !multiline, |tabs| {
                tabs.h(px(strip_height))
            })
            .when(!pinned_tabs.is_empty(), |tabs| tabs.px(px(if classic_frame { 4. } else { 2. })))
            .when(!classic_frame, |tabs| tabs.bg(rgb(background)))
            .text_size(tab_text_size)
            .children(pinned_tabs);
        let tabs = tabs.children(scrollable_tabs).into_any_element();
        let overflowing = self.tab_overflow[pane as usize];
        let observed_scroll = self.tab_scroll.clone();
        let owner = cx.entity();
        let row = div()
            .when(!vertical && !multiline, |row| {
                row.on_children_prepainted(move |children, window, _| {
                    let width = children.get(1).map_or(px(0.), |bounds| bounds.size.width)
                        + if overflowing {
                            px(2. * TAB_SCROLL_ARROW_SIZE)
                        } else {
                            px(0.)
                        };
                    let overflow = observed_scroll.bounds().size.width
                        + observed_scroll.max_offset().x
                        > width;
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
            .on_scroll(
                cx.listener(move |view, event: &gpui::ScrollEvent, window, cx| {
                    view.tab_wheel(pane, draggable, event, wheel_offset.get(), window, cx);
                }),
            );
        if vertical || multiline {
            return row
                .relative()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .overflow_hidden()
                .when(vertical, |row| row.w(px(160.)).h_full())
                .when(!vertical, |row| row.w_full())
                .child(pinned_tabs)
                .child(tabs)
                .child(capture_scroll)
                .into_any_element();
        }
        let scroll = self.tab_scroll.clone();
        let scrollable_count = self.scrollable_tab_count(pane);
        let arrows = [false, true].map(|forward| {
            let enabled = if forward {
                scroll.offset().x > -scroll.max_offset().x
                    && first_visible_tab(&scroll, scrollable_count)
                        .is_some_and(|index| index + 1 < scrollable_count)
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
        row.flex()
            .relative()
            .w_full()
            .h(px(strip_height))
            .flex_shrink_0()
            .overflow_hidden()
            .child(pinned_tabs)
            .child(tabs)
            .child(capture_scroll)
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
