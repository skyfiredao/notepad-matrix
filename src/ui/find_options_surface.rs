use super::*;
use super::dialog_font::px;
use gpui::{Pixels, point};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SwapOperation {
    Swap,
    FindToReplace,
    ReplaceToFind,
}

impl SwapOperation {
    fn symbol(self) -> &'static str {
        match self { Self::Swap => "⇅", Self::FindToReplace => "⤵", Self::ReplaceToFind => "⤴" }
    }

    fn label(self) -> &'static str {
        let id = match self { Self::Swap => 1726, Self::FindToReplace => 1727, Self::ReplaceToFind => 1728 };
        crate::localization::current_dialog_control("Find/Menu", id).unwrap_or(match self {
            Self::Swap => "⇅ Swap Find and Replace text",
            Self::FindToReplace => "⤵ Copy Find text to Replace",
            Self::ReplaceToFind => "⤴ Copy Replace text to Find",
        })
    }
}

pub(super) struct State {
    pub two_buttons: bool,
    pub compact: bool,
    pub replace_stop: bool,
    pub confirm_open: bool,
    transparency: u8,
    alpha: u8,
    swap_menu: bool,
    swap_operation: SwapOperation,
    slider_dragging: bool,
    slider_drag_origin: u8,
    slider_drag_x: f32,
    swap_focus: FocusHandle,
    swap_menu_focus: FocusHandle,
    swap_menu_selected: Option<usize>,
    swap_menu_bounds: Option<Bounds<Pixels>>,
    transparency_focus: [FocusHandle; 2],
    pub(super) option_focus: [FocusHandle; 9],
    transparency_check_focus: FocusHandle,
    slider_focus: FocusHandle,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            two_buttons: false,
            compact: false,
            replace_stop: false,
            confirm_open: true,
            transparency: 0,
            alpha: 150,
            swap_menu: false,
            swap_operation: SwapOperation::Swap,
            slider_dragging: false,
            slider_drag_origin: 150,
            slider_drag_x: 0.,
            swap_focus: cx.focus_handle().tab_index(2),
            swap_menu_focus: cx.focus_handle().tab_stop(false),
            swap_menu_selected: None,
            swap_menu_bounds: None,
            transparency_focus: [
                cx.focus_handle().tab_index(31),
                cx.focus_handle().tab_index(31).tab_stop(false),
            ],
            option_focus: [14, 15, 16, 17, 20, 23, 12, 13, 19]
                .map(|index| cx.focus_handle().tab_index(index)),
            transparency_check_focus: cx.focus_handle().tab_index(30),
            slider_focus: cx.focus_handle().tab_index(32),
        }
    }
}

impl MatrixWindow {
    pub(super) fn configure_find_controls(&mut self, settings: &serde_json::Value) {
        let history = &settings["FindHistory"];
        self.find_controls.two_buttons = history["isSearch2ButtonsMode"].as_bool().unwrap_or(false);
        self.find_controls.transparency =
            history["transparencyMode"].as_u64().unwrap_or(0).min(2) as u8;
        self.find_controls.alpha = history["transparency"]
            .as_u64()
            .unwrap_or(150)
            .clamp(1, 200) as u8;
        self.find_controls.compact = settings["FindWindowPosition"]["isLessModeOn"]
            .as_bool()
            .unwrap_or(false);
        self.find_controls.replace_stop = settings["Searching"]["replaceStopsWithoutFindingNext"]
            .as_bool()
            .unwrap_or(false);
        self.find_controls.confirm_open = settings["Searching"]["confirmReplaceInAllOpenDocs"]
            .as_bool()
            .unwrap_or(true);
    }

    fn persist_find_setting(
        &mut self,
        section: &str,
        key: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(store) = &self.settings_store
            && let Err(error) = store.update_settings(|settings| {
                if !settings[section].is_null() && !settings[section].is_object() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{section} must be an object"),
                    ));
                }
                if settings[section].is_null() {
                    settings[section] = serde_json::json!({});
                }
                settings[section][key] = value;
                Ok(())
            })
        {
            self.set_find_status(find_surface::FindStatus::NotFound,
                crate::localization::Text::resource("matrix-search-settings-save-error", "Search settings could not be saved: $ERROR$")
                    .replace("$ERROR$", error.to_string()));
            cx.notify();
            return false;
        }
        true
    }

    pub(super) fn update_find_window(&self, window: &mut Window) {
        let width = find_surface::DIALOG_WIDTH;
        let height = if self.find_controls.compact {
            find_surface::COMPACT_DIALOG_HEIGHT
        } else {
            find_surface::DIALOG_HEIGHT
        };
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            if let Some(host) = view.window() {
                let native_dimensions = objc2_foundation::NSSize::new(dialog_font::native_length(f64::from(width)), dialog_font::native_length(f64::from(height)) + f64::from(window_caption::HEIGHT));
                if host.contentMaxSize().height < native_dimensions.height
                    || host.contentMaxSize().width < native_dimensions.width {
                    host.setContentMaxSize(native_dimensions);
                }
                host.setContentMinSize(native_dimensions);
                host.setContentMaxSize(native_dimensions);
                let transparent = self.find_controls.transparency == 2
                    || self.find_controls.transparency == 1 && !window.is_window_active();
                host.setAlphaValue(if transparent {
                    f64::from(self.find_controls.alpha) / 255.
                } else {
                    1.
                });
            }
        }
        let client = self.auxiliary_client_size(window);
        if (f32::from(client.height - px(height))).abs() >= 1.
            || (f32::from(client.width - px(width))).abs() >= 1. {
            window.resize(size(px(width), px(height) + gpui::px(window_caption::HEIGHT)));
        }
    }

    fn swap_search_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_query.read(cx).has_composition()
            || self.replace_input.read(cx).has_composition()
        {
            return;
        }
        let find = self.find_query.read(cx).text();
        let replacement = self.replace_input.read(cx).text();
        if find != replacement {
            match self.find_controls.swap_operation {
                SwapOperation::Swap => {
                    self.find_query.update(cx, |field, cx| field.set_text(&replacement, window, cx));
                    self.replace_input.update(cx, |field, cx| field.set_text(&find, window, cx));
                }
                SwapOperation::FindToReplace if !find.is_empty() => {
                    self.replace_input.update(cx, |field, cx| field.set_text(&find, window, cx));
                }
                SwapOperation::ReplaceToFind if !replacement.is_empty() => {
                    self.find_query.update(cx, |field, cx| field.set_text(&replacement, window, cx));
                }
                _ => {}
            }
        }
        self.find_controls.swap_menu = false;
        cx.notify();
    }

    pub(super) fn render_find_swap(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("find-swap-control")
            .role(Role::Button)
            .aria_label(self.find_controls.swap_operation.label())
            .track_focus(&self.find_controls.swap_focus)
            .absolute()
            .right(px(241.))
            .top(px(48.))
            .w(px(40.))
            .h(px(28.))
            .flex()
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "space" if !event.keystroke.modifiers.alt => view.swap_search_inputs(window, cx),
                    "down" if event.keystroke.modifiers.alt => view.open_find_swap_menu(true, window, cx),
                    "f4" => view.open_find_swap_menu(true, window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindDialogNext, window, cx| {
                view.swap_search_inputs(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindDialogReverse, window, cx| {
                view.swap_search_inputs(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindAllInFiles, window, cx| {
                view.swap_search_inputs(window, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id("find-swap")
                    .role(Role::Button)
                    .aria_label(self.find_controls.swap_operation.label())
                    .w(px(24.))
                    .h_full()
                    .border_1()
                    .border_color(rgb(0xadadad))
                    .bg(rgb(0xe1e1e1))
                    .flex().items_center().justify_center()
                    .text_center()
                    .text_size(px(17.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(self.find_controls.swap_operation.symbol())
                    .on_click(
                        cx.listener(|view, _, window, cx| {
                            view.find_controls.swap_focus.focus(window, cx);
                            view.swap_search_inputs(window, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .id("find-swap-menu")
                    .role(Role::Button)
                    .aria_label(self.find_text("matrix-find-copy-content", "Copy Find or Replace text"))
                    .w(px(16.))
                    .h_full()
                    .border_1()
                    .border_color(rgb(0xadadad))
                    .bg(rgb(0xe1e1e1))
                    .flex().items_center().justify_center()
                    .child("▾")
                    .on_click(cx.listener(|view, _, window, cx| {
                        if view.close_find_swap_menu(cx) {
                            view.find_controls.swap_focus.focus(window, cx);
                        } else {
                            view.open_find_swap_menu(false, window, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    pub(super) fn close_find_swap_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.find_controls.swap_menu { return false; }
        self.find_controls.swap_menu = false;
        cx.notify();
        true
    }

    fn open_find_swap_menu(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_search_history();
        let mut bounds = Bounds::new(window.bounds().origin
            + point(window.viewport_size().width - px(281.), px(76.) + gpui::px(window_caption::HEIGHT)), size(px(248.), px(74.)));
        if let Some(display) = window.display(cx) {
            let area = display.visible_bounds();
            bounds.origin.x = bounds.origin.x.max(area.left()).min(area.right() - bounds.size.width);
            bounds.origin.y = bounds.origin.y.max(area.top()).min(area.bottom() - bounds.size.height);
        }
        self.find_controls.swap_menu_bounds = Some(bounds);
        self.find_controls.swap_menu_selected = keyboard.then_some(0);
        self.find_controls.swap_menu = true;
        self.present_floating(floating_windows::DialogKind::FindSwap, window, cx);
        cx.notify();
    }

    pub(super) fn find_swap_popup_visible(&self) -> bool {
        self.find_controls.swap_menu && (self.find_visible || self.files_search.visible)
    }

    pub(super) fn find_swap_popup_bounds(&self) -> Option<Bounds<Pixels>> {
        self.find_controls.swap_menu_bounds
    }

    pub(super) fn find_swap_popup_focus(&self) -> FocusHandle {
        self.find_controls.swap_menu_focus.clone()
    }

    pub(super) fn focus_find_swap_button(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.floating_windows.get(&floating_windows::DialogKind::Find).copied() else { return; };
        let focus = self.find_controls.swap_focus.clone();
        cx.defer(move |cx| {
            if cx.windows().contains(&handle.into())
                && let Err(error) = handle.update(cx, |_, window, cx| {
                    window.activate_window();
                    focus.focus(window, cx);
                }) {
                eprintln!("could not restore find swap focus: {error}");
            }
        });
    }

    fn select_find_swap_operation(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = [SwapOperation::Swap, SwapOperation::FindToReplace, SwapOperation::ReplaceToFind]
            .get(index).copied() else { return; };
        self.find_controls.swap_operation = operation;
        self.swap_search_inputs(window, cx);
        self.focus_find_swap_button(cx);
    }

    pub(super) fn render_find_swap_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.find_controls.swap_menu { return None; }
        Some(div().id("find-swap-popup").role(Role::Menu).aria_label(self.find_text("matrix-find-content-operations", "Find and Replace text operations"))
            .track_focus(&self.find_controls.swap_menu_focus)
            .key_context("MatrixFind").w(px(248.))
            .bg(rgb(0xffffff)).border_1().border_color(rgb(0xadadad)).occlude()
            .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                if view.close_find_swap_menu(cx) { view.focus_find_swap_button(cx); }
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                let selected = view.find_controls.swap_menu_selected;
                match event.keystroke.key.as_str() {
                    "up" => view.find_controls.swap_menu_selected = Some(selected.map_or(2, |index| (index + 2) % 3)),
                    "down" => view.find_controls.swap_menu_selected = Some(selected.map_or(0, |index| (index + 1) % 3)),
                    "home" => view.find_controls.swap_menu_selected = Some(0),
                    "end" => view.find_controls.swap_menu_selected = Some(2),
                    "space" => {
                        if let Some(selected) = selected { view.select_find_swap_operation(selected, window, cx); }
                    }
                    _ => return,
                }
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindDialogNext, window, cx| {
                if let Some(selected) = view.find_controls.swap_menu_selected {
                    view.select_find_swap_operation(selected, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindDialogReverse, window, cx| {
                if let Some(selected) = view.find_controls.swap_menu_selected {
                    view.select_find_swap_operation(selected, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindAllInFiles, window, cx| {
                if let Some(selected) = view.find_controls.swap_menu_selected {
                    view.select_find_swap_operation(selected, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseFind, _, cx| {
                view.close_find_swap_menu(cx);
                view.focus_find_swap_button(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseFindInFiles, _, cx| {
                view.close_find_swap_menu(cx);
                view.focus_find_swap_button(cx);
                cx.stop_propagation();
            }))
            .children([
                (SwapOperation::Swap, "find-swap-operation"),
                (SwapOperation::FindToReplace, "find-copy-query"),
                (SwapOperation::ReplaceToFind, "find-copy-replacement"),
            ].into_iter().enumerate().map(|(index, (operation, id))| {
                div().id(id).role(Role::MenuItem).aria_label(operation.label())
                    .aria_selected(Some(index) == self.find_controls.swap_menu_selected)
                    .h(px(24.)).px(px(6.)).hover(|style| style.bg(rgb(0x91c9f7)))
                    .when(Some(index) == self.find_controls.swap_menu_selected, |item| item.bg(rgb(0x91c9f7)))
                    .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                        if *hovered { view.find_controls.swap_menu_selected = Some(index); }
                        else if view.find_controls.swap_menu_selected == Some(index) {
                            view.find_controls.swap_menu_selected = None;
                        }
                        cx.notify();
                    }))
                    .child(operation.label())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.select_find_swap_operation(index, window, cx);
                    }))
            })).into_any_element())
    }

    pub(super) fn render_find_options(&self, cx: &mut Context<Self>) -> AnyElement {
        let compact = self.find_controls.compact;
        div()
            .absolute()
            .inset_0()
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
                if view.find_controls.slider_dragging { view.move_find_transparency(event.position.x, cx); }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, cx| view.finish_find_transparency_drag(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, cx| view.finish_find_transparency_drag(cx)))
            .when(!compact, |element| {
                element.child(
                    div()
                        .absolute()
                        .right(px(39.))
                        .top(px(236.))
                        .w(px(177.))
                        .h(px(76.))
                        .border_1()
                        .border_color(rgb(0xdcdcdc))
                        .child(
                            find_surface::checkbox(
                                "find-transparency",
                                "透明度(Y)",
                                self.find_controls.transparency != 0,
                            )
                            .track_focus(&self.find_controls.transparency_check_focus)
                            .h(px(17.))
                            .absolute()
                            .top(px(-9.))
                            .left(px(-8.))
                            .bg(rgb(0xffffff))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.find_controls.transparency_check_focus.focus(window, cx);
                                view.toggle_find_transparency(cx);
                            }))
                            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "space" {
                                    view.toggle_find_transparency(cx);
                                    cx.stop_propagation();
                                }
                            })),
                        )
                        .children(
                            [
                                (1, "find-transparency-inactive", "失去焦点后"),
                                (2, "find-transparency-always", "始终"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (mode, id, label))| {
                                let label = crate::localization::current_dialog_control("Find", if mode == 1 { 1687 } else { 1688 }).unwrap_or(label);
                                let enabled = self.find_controls.transparency != 0;
                                let selected = self.find_controls.transparency == mode;
                                find_surface::radio(id, label, self.find_controls.transparency == mode)
                                    .h(px(17.))
                                    .when(enabled, |control| control.track_focus(
                                        &self.find_controls.transparency_focus[index].clone().tab_stop(selected)))
                                    .absolute()
                                    .left(px(17.))
                                    .top(px(12. + index as f32 * 21.))
                                    .text_color(rgb(if enabled { 0x202020 } else { 0x808080 }))
                                    .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.select_find_transparency(index, window, cx);
                                    }))
                                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                                        if !enabled { return; }
                                        let target = match event.keystroke.key.as_str() {
                                            "space" => index,
                                            "left" | "right" | "up" | "down" => 1 - index,
                                            _ => return,
                                        };
                                        view.select_find_transparency(target, window, cx);
                                        cx.stop_propagation();
                                    }))
                            }),
                        )
                        .child(
                            div()
                                .id("find-transparency-slider")
                                .role(Role::Slider)
                                .aria_label(crate::localization::current_dialog_control("Find", 1686).unwrap_or("Transparency"))
                                .aria_orientation(gpui::accesskit::Orientation::Horizontal)
                                .when(self.find_controls.transparency != 0, |control| control.track_focus(&self.find_controls.slider_focus))
                                .a11y_synthetic_children({
                                    let enabled = self.find_controls.transparency != 0;
                                    let alpha = self.find_controls.alpha.clamp(20, 200);
                                    move |builder| {
                                        let node = builder.parent_node();
                                        node.set_min_numeric_value(20.);
                                        node.set_max_numeric_value(200.);
                                        node.set_numeric_value(f64::from(alpha));
                                        node.set_numeric_value_step(1.);
                                        if !enabled { node.set_disabled(); }
                                    }
                                })
                                .absolute()
                                .left(px(15.))
                                .top(px(54.))
                                .w(px(141.))
                                .h(px(14.))
                                .child(
                                    div()
                                         .absolute()
                                         .left(px(2.))
                                         .top(px(6.))
                                         .w(px(137.))
                                         .h(px(3.))
                                         .border_1()
                                         .border_color(rgb(0xd6d6d6))
                                         .bg(rgb(0xe7eaea)),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(
                                            2. + (131. * f32::from(self.find_controls.alpha.clamp(20, 200) - 20) / 180.).floor()
                                         ))
                                         .top(px(2.))
                                        .w(px(6.))
                                        .h(px(11.))
                                        .bg(rgb(if self.find_controls.transparency != 0 { 0x007ad9 } else { 0xa0a0a0 })),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                                        if view.find_controls.transparency == 0 {
                                            return;
                                        }
                                        view.find_controls.slider_focus.focus(window, cx);
                                        let current = view.find_controls.alpha.clamp(20, 200);
                                        let thumb = dialog_font::logical(window.viewport_size().width) - 195.
                                            + (131. * f32::from(current - 20) / 180.).floor();
                                        let x = dialog_font::logical(event.position.x);
                                        if x < thumb - 3. || x > thumb + 3. {
                                            view.set_find_transparency(if x < thumb {
                                                current.saturating_sub(36).max(20)
                                            } else {
                                                current.saturating_add(36).min(200)
                                            }, cx);
                                            cx.stop_propagation();
                                            return;
                                        }
                                        view.find_controls.slider_drag_origin = view.find_controls.alpha;
                                        view.find_controls.slider_drag_x = x;
                                        view.find_controls.slider_dragging = true;
                                        cx.stop_propagation();
                                    }),
                                )
                                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                                    if view.find_controls.transparency == 0 { return; }
                                    let current = view.find_controls.alpha.clamp(20, 200);
                                    let alpha = match event.keystroke.key.as_str() {
                                        "left" | "up" => current.saturating_sub(1).max(20),
                                        "right" | "down" => current.saturating_add(1).min(200),
                                        "pageup" => current.saturating_sub(36).max(20),
                                        "pagedown" => current.saturating_add(36).min(200),
                                        "home" => 20, "end" => 200,
                                        _ => return,
                                    };
                                    view.set_find_transparency(alpha, cx);
                                    cx.stop_propagation();
                                })),
                        ),
                )
            })
            .child(
                find_surface::source_button(
                    "find-compact",
                    if compact { "˅" } else { "˄" },
                    if compact { 72. } else { 295. },
                )
                 .right(px(5.))
                 .w(px(26.))
                 .font_family("Courier New")
                 .text_size(px(17.))
                .aria_label(if compact {
                    self.find_text("matrix-find-expand-window", "Expand Find window")
                } else {
                    self.find_text("matrix-find-compact-window", "Compact Find window")
                })
                .on_click(cx.listener(|view, _, _, cx| view.toggle_find_compact(cx))),
            )
            .into_any_element()
    }

    pub(super) fn toggle_find_control(&mut self, index: usize, cx: &mut Context<Self>) {
        match index {
            0 => {
                let value = !self.find_controls.two_buttons;
                if self.persist_find_option("isSearch2ButtonsMode", value.into(), cx) {
                    self.find_controls.two_buttons = value;
                }
            }
            1 => {
                let value = !self.find_controls.replace_stop;
                if self.persist_find_setting(
                    "Searching",
                    "replaceStopsWithoutFindingNext",
                    value.into(),
                    cx,
                ) {
                    self.find_controls.replace_stop = value;
                }
            }
            _ => {
                let value = !self.find_controls.confirm_open;
                if self.persist_find_setting(
                    "Searching",
                    "confirmReplaceInAllOpenDocs",
                    value.into(),
                    cx,
                ) {
                    self.find_controls.confirm_open = value;
                }
            }
        }
        cx.notify();
    }

    pub(super) fn focus_find_option(&self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let index = match id {
            1603 if !matches!(self.find_options.mode, crate::search::SearchMode::Regex { .. }) => 1,
            1604 => 2,
            1606 if !self.files_search.visible => 3,
            1632 if !self.files_search.visible && self.find_in_selection_enabled => 4,
            1616 if self.find_mark_mode => 6,
            1618 if self.find_mark_mode => 7,
            1703 if matches!(self.find_options.mode, crate::search::SearchMode::Regex { .. }) => 8,
            1686 => {
                self.find_controls.transparency_check_focus.focus(window, cx);
                return;
            }
            _ => return,
        };
        self.find_controls.option_focus[index].focus(window, cx);
    }

    pub(super) fn toggle_find_transparency(&mut self, cx: &mut Context<Self>) {
        let mode = if self.find_controls.transparency == 0 { 1 } else { 0 };
        if self.persist_find_option("transparencyMode", mode.into(), cx) { self.find_controls.transparency = mode; }
        cx.notify();
    }

    fn select_find_transparency(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_controls.transparency == 0 || index >= 2 { return; }
        let mode = if index == 0 { 1_u8 } else { 2_u8 };
        if self.find_controls.transparency != mode
            && !self.persist_find_option("transparencyMode", mode.into(), cx) { return; }
        self.find_controls.transparency = mode;
        self.find_controls.transparency_focus[index].focus(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_find_compact(&mut self, cx: &mut Context<Self>) {
        let value = !self.find_controls.compact;
        if self.persist_find_setting("FindWindowPosition", "isLessModeOn", value.into(), cx) { self.find_controls.compact = value; }
        cx.notify();
    }

    fn set_find_transparency(&mut self, alpha: u8, cx: &mut Context<Self>) {
        if self.persist_find_option("transparency", alpha.into(), cx) { self.find_controls.alpha = alpha; }
        cx.notify();
    }

    fn move_find_transparency(&mut self, x: gpui::Pixels, cx: &mut Context<Self>) {
        let delta = (dialog_font::logical(x) - self.find_controls.slider_drag_x) * 180. / 131.;
        let alpha = (f32::from(self.find_controls.slider_drag_origin.clamp(20, 200)) + delta)
            .round().clamp(20., 200.) as u8;
        self.find_controls.alpha = alpha;
        cx.notify();
    }

    pub(super) fn finish_find_transparency_drag(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.find_controls.slider_dragging) { return; }
        let alpha = self.find_controls.alpha;
        if alpha != self.find_controls.slider_drag_origin && !self.persist_find_option("transparency", alpha.into(), cx) {
            self.find_controls.alpha = self.find_controls.slider_drag_origin;
        }
        cx.notify();
    }
}
