use super::*;
use std::cell::Cell;
use std::rc::Rc;

pub(super) struct State {
    language_open: bool,
    language_cursor: usize,
    name_action: Option<&'static str>,
    slider_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    slider_dragging: bool,
    remove_prompt: Option<gpui::Task<()>>,
    name_error_prompt: Option<gpui::Task<()>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            language_open: false,
            language_cursor: 0,
            name_action: None,
            slider_bounds: Rc::new(Cell::new(None)),
            slider_dragging: false,
            remove_prompt: None,
            name_error_prompt: None,
        }
    }
}

fn header_at((x, y, width, height): (f32, f32, f32, f32)) -> gpui::Div {
    div().absolute().left(px(x)).top(px(y)).w(px(width)).h(px(height))
}

fn button(
    id: u32,
    label: &'static str,
    rect: (f32, f32, f32, f32),
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    header_at(rect)
        .h(dialog_font::control_height())
        .id(gpui::SharedString::from(format!("udl-resource-{id}")))
        .role(Role::Button)
        .aria_label(label)
        .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
        .tab_index(if enabled { 0 } else { -1 })
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(rgb(0x808080))
        .bg(rgb(0xe1e1e1))
        .when(!enabled, |element| element.opacity(0.5))
        .cursor_pointer()
        .child(label)
}

impl MatrixWindow {
    fn udl_main_enabled(&self) -> bool {
        self.udl.ready
            && !self.udl.styler_visible
            && !self.udl_name_visible()
            && !self.udl.confirm_remove
    }

    fn udl_main_languages(&self) -> Vec<(u64, String)> {
        std::iter::once((0, "User Defined Language".to_owned()))
            .chain(self.udl.registry.named().map(|(id, definition)| (id, definition.name.clone())))
            .collect()
    }

    fn begin_udl_name(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.udl_main_enabled() || (action == "Rename" && self.udl.selected == 0) {
            return;
        }
        let action = if action == "Save As" && self.udl.selected == 0 { "Create" } else { action };
        let name = if action == "Rename" {
            self.udl.registry.definition(self.udl.selected)
                .map(|definition| definition.name.clone()).unwrap_or_default()
        } else {
            String::new()
        };
        self.udl.main.language_open = false;
        self.udl.main.name_action = Some(action);
        self.udl.error = None;
        self.udl.name.update(cx, |field, cx| {
            field.set_text(&name, window, cx);
        });
        self.present_floating(floating_windows::DialogKind::UdlName, window, cx);
        cx.notify();
    }

    pub(in super::super) fn udl_name_visible(&self) -> bool {
        self.udl.main.name_action.is_some()
    }

    pub(in super::super) fn udl_name_title(&self) -> &'static str {
        let (key, fallback) = match self.udl.main.name_action {
            Some("Rename") => ("userdefined-title-rename", "Rename Current Language Name"),
            Some("Save As") => ("userdefined-title-save", "Save Current Language Name As..."),
            _ => ("userdefined-title-new", "Create New Language..."),
        };
        localized_string(&self.native_locale(), key).unwrap_or(fallback)
    }

    pub(in super::super) fn dismiss_udl_name(&mut self, cx: &mut Context<Self>) {
        self.udl.main.name_action = None;
        cx.notify();
    }

    fn commit_udl_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.udl.main.name_action else { return; };
        if self.udl.styler_visible || !self.udl.ready || self.udl.name.read(cx).has_composition() {
            return;
        }
        self.udl.error = None;
        self.udl_named_action(action, window, cx);
        if let Some(error) = self.udl.error.clone() {
            self.notify_udl_error(cx);
            let locale = self.native_locale();
            let title = crate::localization::dialog_title(&locale, "UDLNewNameError").unwrap_or("UDL Error");
            let ok = localized_string(&locale, "common-ok").unwrap_or("OK");
            let response = window.prompt(gpui::PromptLevel::Warning, title, Some(&error.render(&locale)),
                &[gpui::PromptButton::ok(ok)], cx);
            self.udl.main.name_error_prompt = Some(cx.spawn_in(window, async move |owner, cx| {
                let response = response.await;
                if let Err(error) = owner.update_in(cx, |view, window, cx| {
                    if let Err(error) = response {
                        eprintln!("UDL naming error prompt disconnected: {error}");
                    }
                    if view.udl.main.name_action == Some(action) && !view.udl.styler_visible {
                        view.udl.name.update(cx, |field, cx| field.focus(window, cx));
                    }
                }) {
                    eprintln!("UDL naming error prompt lost its owner: {error}");
                }
            }));
        } else {
            self.dismiss_udl_name(cx);
        }
        cx.notify();
    }

    fn udl_main_action(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.udl_main_enabled() {
            return;
        }
        match id {
            20001 => self.toggle_udl_dock(window, cx),
            20002 => self.begin_udl_name("Rename", window, cx),
            20003 => self.begin_udl_name("Create", window, cx),
            20004 if self.udl.selected != 0 => {
                self.udl.main.language_open = false;
                self.confirm_udl_main_remove(window, cx);
            }
            20005 => self.begin_udl_name("Save As", window, cx),
            20015 | 20016 => self.udl_file_action(id == 20016, cx),
            _ => {}
        }
    }

    fn confirm_udl_main_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.udl.selected;
        let generation = self.udl.generation;
        self.udl_named_action("Remove", window, cx);
        if !self.udl.confirm_remove { return; }
        let locale = self.native_locale();
        let title = crate::localization::dialog_title(&locale, "UDLRemoveCurrentLang")
            .unwrap_or("Remove the current language");
        let message = crate::localization::dialog_message(&locale, "UDLRemoveCurrentLang")
            .unwrap_or("Are you sure?");
        let yes = crate::localization::dialog_control(&locale, "DoSaveOrNot", 6).unwrap_or("Yes");
        let no = crate::localization::dialog_control(&locale, "DoSaveOrNot", 7).unwrap_or("No");
        let response = window.prompt(gpui::PromptLevel::Warning, title, Some(message),
            &[gpui::PromptButton::ok(yes), gpui::PromptButton::cancel(no)], cx);
        self.udl.main.remove_prompt = Some(cx.spawn_in(window, async move |owner, cx| {
            let response = response.await;
            if let Err(error) = owner.update_in(cx, |view, window, cx| {
                if view.udl.selected != selected || view.udl.generation != generation {
                    view.udl.confirm_remove = false;
                    cx.notify();
                    return;
                }
                match response {
                    Ok(0) if !view.udl.styler_visible && view.udl.confirm_remove => {
                        view.udl_named_action("Remove", window, cx);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        view.udl.error = Some(Text::resource("matrix-udl-error", "User-defined language error: $ERROR$")
                            .replace("$ERROR$", error.to_string()));
                        view.notify_udl_error(cx);
                    }
                }
                view.udl.confirm_remove = false;
                cx.notify();
            }) {
                eprintln!("UDL removal prompt lost its owner: {error}");
            }
        }));
    }

    fn udl_main_slider_at(
        &mut self,
        x: gpui::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.udl_main_enabled() || self.udl.docked || !self.udl.transparent {
            return;
        }
        let Some(bounds) = self.udl.main.slider_bounds.get() else { return; };
        let width = f32::from(bounds.size.width - px(8.));
        if width <= 0. { return; }
        let fraction = (f32::from(x - bounds.origin.x - px(4.)) / width).clamp(0., 1.);
        self.set_udl_transparency_level((20. + fraction * 180.).round() as u8, window, cx);
    }

    pub(in super::super) fn render_udl(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.udl.visible { return None; }
        let locale = self.native_locale();
        let enabled = self.udl_main_enabled();
        let label = crate::localization::dialog_control(&locale, "UserDefine", 20007)
            .unwrap_or("User language:");
        let entries = self.udl_main_languages();
        let selected_index = entries.iter().position(|(id, _)| *id == self.udl.selected).unwrap_or(0);
        let selected_name = entries.get(selected_index).map(|(_, name)| name.clone()).unwrap_or_default();
        let mut panel = div()
            .id("udl-definition-dialog")
            .role(Role::Dialog)
            .aria_label(crate::localization::dialog_title(&locale, "UserDefine").unwrap_or("User Defined Language v2.1"))
            .track_focus(&self.udl.focus)
            .relative()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .font_family(dialog_font::font_family())
            .text_size(gpui::px(dialog_font::base_size()))
            .line_height(dialog_font::line_height())
            .text_color(rgb(0x202020))
            .bg(rgb(0xf0f0f0))
            .on_action(cx.listener(|view, _: &CloseWindow, window, cx| {
                if view.udl.styler_visible || view.udl_name_visible() {
                    cx.stop_propagation(); return;
                }
                if view.udl.main.language_open {
                    view.udl.main.language_open = false;
                } else if view.udl.confirm_remove {
                    view.udl.confirm_remove = false;
                } else {
                    view.dismiss_floating(floating_windows::DialogKind::Udl, window, cx);
                }
                cx.stop_propagation(); cx.notify();
            }))
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, window, cx| {
                if view.udl.main.slider_dragging && event.pressed_button == Some(gpui::MouseButton::Left) {
                    view.udl_main_slider_at(event.position.x, window, cx);
                } else { view.udl.main.slider_dragging = false; }
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|view, _, _, _| {
                view.udl.main.slider_dragging = false;
            }))
            .child(header_at((12., 17., 84., 14.)).id("udl-resource-20007").whitespace_nowrap().text_right().child(label))
            .child(
                header_at((104., 12., 168., 24.))
                    .h(dialog_font::control_height())
                    .id("udl-resource-20006")
                    .role(Role::ComboBox)
                    .aria_label(label)
                    .aria_value(selected_name.clone())
                    .aria_expanded(self.udl.main.language_open)
                    .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
                    .tab_index(if enabled { 0 } else { -1 })
                    .border_1().border_color(rgb(0x808080)).bg(rgb(0xffffff))
                    .flex().items_center().justify_between().px(px(6.))
                    .child(div().min_w(px(0.)).truncate().child(selected_name))
                    .child("▾")
                    .on_click(cx.listener(|view, event, window, cx| {
                        if !view.udl_main_enabled() { return; }
                        let entries = view.udl_main_languages();
                        if matches!(event, gpui::ClickEvent::Keyboard(_)) && view.udl.main.language_open {
                            view.udl.main.language_open = false;
                            if let Some((id, _)) = entries.get(view.udl.main.language_cursor) {
                                view.udl_select(*id, window, cx);
                            }
                        } else {
                            view.udl.main.language_cursor = entries.iter()
                                .position(|(id, _)| *id == view.udl.selected).unwrap_or(0);
                            view.udl.main.language_open = !view.udl.main.language_open;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                        if !view.udl_main_enabled() { return; }
                        let entries = view.udl_main_languages();
                        let current = if view.udl.main.language_open {
                            view.udl.main.language_cursor
                        } else {
                            entries.iter().position(|(id, _)| *id == view.udl.selected).unwrap_or(0)
                        };
                        let target = match event.keystroke.key.as_str() {
                            "escape" if view.udl.main.language_open => {
                                view.udl.main.language_open = false;
                                cx.stop_propagation(); cx.notify(); return;
                            }
                            "up" => current.saturating_sub(1),
                            "down" => (current + 1).min(entries.len().saturating_sub(1)),
                            "home" => 0,
                            "end" => entries.len().saturating_sub(1),
                            _ => return,
                        };
                        view.udl.main.language_cursor = target;
                        if !view.udl.main.language_open {
                            if let Some((id, _)) = entries.get(target) { view.udl_select(*id, window, cx); }
                        }
                        cx.stop_propagation(); cx.notify();
                    })),
            );
        for (id, fallback, rect) in [
            (20003, "Create new...", (284., 12., 96., 24.)),
            (20005, "Save as...", (388., 12., 88., 24.)),
            (20002, "Rename", (484., 12., 64., 24.)),
            (20004, "Remove", (556., 12., 64., 24.)),
            (20015, "Import...", (12., 48., 88., 24.)),
            (20016, "Export...", (108., 48., 88., 24.)),
            (20001, "Dock", (672., 12., 76., 24.)),
        ] {
            if self.udl.selected == 0 && matches!(id, 20002 | 20004) { continue; }
            let label_id = if id == 20001 && self.udl.docked { 20017 } else { id };
            let fallback = if label_id == 20017 { "Undock" } else { fallback };
            let label = crate::localization::dialog_control(&locale, "UserDefine", label_id).unwrap_or(fallback);
            let active = enabled && (!matches!(id, 20015 | 20016) || !self.udl.prompting);
            panel = panel.child(button(id, label, rect, active)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if active { view.udl_main_action(id, window, cx); }
                }))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                    if active && matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        view.udl_main_action(id, window, cx);
                        cx.stop_propagation();
                    }
                })));
        }
        panel = panel.child(self.udl_source_check(Control {
            group: "Language".into(), label: "Ignore case", path: "/case_ignored".into(), kind: Kind::Boolean,
        }, (144., 32., 80., 16.), cx));
        if self.udl.selected != 0 {
            let label = crate::localization::dialog_control(&locale, "UserDefine", 20009).unwrap_or("Ext.:");
            panel = panel
                .child(header_at((332., 53., 40., 14.)).id("udl-resource-20009").pr(px(6.)).whitespace_nowrap().text_right().child(label))
                .child(self.udl_source_field(udl_controls::extensions_control(), (248., 32., 72., 16.), false, window, cx));
        }
        let mut tabs = header_at((12., 88., WIDTH - 24., 28.))
            .id("udl-source-tabs").role(Role::TabList)
            .border_b_1().border_color(rgb(0xaaaaaa));
        let mut x = px(0.);
        for page in Page::ALL {
            let label = crate::localization::dialog_title(&locale, localization::page_path(page)).unwrap_or(page.label());
            let display = label.to_owned();
            let measured = window.text_system().shape_line(display.clone().into(), gpui::px(dialog_font::base_size()), &[gpui::TextRun {
                len: display.len(), font: gpui::font(dialog_font::font_family()), color: window.text_style().color,
                background_color: None, underline: None, strikethrough: None,
            }], None).width().ceil();
            let width = measured + px(24.);
            tabs = tabs.child(div().absolute().left(x).top_0().w(width).h(px(28.))
                .id(gpui::SharedString::from(format!("udl-tab-{}", page.label())))
                .role(Role::Tab).aria_label(label).aria_selected(self.udl.page == page)
                .a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).tab_index(if enabled { 0 } else { -1 })
                .flex().items_center().justify_center().border_1().border_color(rgb(0xaaaaaa))
                .bg(rgb(if self.udl.page == page { 0xffffff } else { 0xe1e1e1 }))
                .child(display)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if !view.udl_main_enabled() { return; }
                    view.udl.main.language_open = false;
                    view.udl.page = page;
                    cx.notify();
                }))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                    if !view.udl_main_enabled() { return; }
                    let current = Page::ALL.iter().position(|candidate| *candidate == view.udl.page).unwrap_or(0);
                    let target = match event.keystroke.key.as_str() {
                        "left" | "up" => Page::ALL[(current + 3) % 4],
                        "right" | "down" => Page::ALL[(current + 1) % 4],
                        "home" => Page::ALL[0], "end" => Page::ALL[3],
                        "space" | "enter" => page,
                        _ => return,
                    };
                    view.udl.main.language_open = false;
                    view.udl.page = target;
                    cx.stop_propagation(); cx.notify();
                })));
            x += width;
        }
        panel = panel.child(tabs)
            .child(header_at((12., 116., WIDTH - 24., HEIGHT - 128.))
                .id("udl-source-tab-panel").role(Role::TabPanel)
                .border_1().border_color(rgb(0xaaaaaa)).overflow_y_scroll()
                .child(self.render_udl_source_page(window, cx)));
        if cfg!(target_os = "macos") && !self.udl.docked {
            let label = crate::localization::dialog_control(&locale, "UserDefine", 20011).unwrap_or("Transparency");
            panel = panel.child(header_at((500., 48., 118., 24.))
                .id("udl-resource-20011").role(Role::CheckBox).aria_label(label)
                .aria_toggled(self.udl.transparent.into()).a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } })
                .tab_index(if enabled { 0 } else { -1 })
                .flex().items_center().gap(du(2.))
                .child(preferences_surface::pref_indicator_palette(self.udl.transparent, false, enabled, self.preferences_render_palette()))
                .child(label)
                .on_click(cx.listener(|view, _, window, cx| {
                    if view.udl_main_enabled() { view.toggle_udl_transparency(window, cx); }
                }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if view.udl_main_enabled() && event.keystroke.key == "space" {
                        view.toggle_udl_transparency(window, cx); cx.stop_propagation();
                    }
                })));
            let alpha = self.udl.opacity.clamp(20, 200);
            let slider_enabled = enabled && self.udl.transparent;
            let bounds = self.udl.main.slider_bounds.clone();
            panel = panel.child(header_at((628., 48., 120., 24.))
                .id("udl-resource-20010").role(Role::Slider).aria_label(label)
                .a11y_synthetic_children(move |builder| { if !slider_enabled { builder.parent_node().set_disabled(); } }).tab_index(if slider_enabled { 0 } else { -1 })
                .aria_orientation(gpui::accesskit::Orientation::Horizontal)
                .a11y_synthetic_children(move |builder| {
                    let node = builder.parent_node();
                    node.set_min_numeric_value(20.); node.set_max_numeric_value(200.);
                    node.set_numeric_value(f64::from(alpha)); node.set_numeric_value_step(1.);
                })
                .child(gpui::canvas(move |rect, _, _| { bounds.set(Some(rect)); }, |_, _, _, _| {}).size_full())
                .child(header_at((4., 11., 112., 2.)).bg(rgb(0xaaaaaa)))
                .child(header_at((4. + 112. * f32::from(alpha - 20) / 180., 5., 4., 14.))
                    .bg(rgb(if slider_enabled { 0x0078d7 } else { 0x808080 })))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                    if !view.udl_main_enabled() || !view.udl.transparent { return; }
                    view.udl.main.slider_dragging = true;
                    view.udl_main_slider_at(event.position.x, window, cx);
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if !view.udl_main_enabled() || !view.udl.transparent { return; }
                    let current = view.udl.opacity.clamp(20, 200);
                    let level = match event.keystroke.key.as_str() {
                        "left" | "up" => current.saturating_sub(1).max(20),
                        "right" | "down" => current.saturating_add(1).min(200),
                        "pageup" => current.saturating_sub(36).max(20),
                        "pagedown" => current.saturating_add(36).min(200),
                        "home" => 20, "end" => 200,
                        _ => return,
                    };
                    view.set_udl_transparency_level(level, window, cx);
                    cx.stop_propagation();
                })));
        }
        if self.udl.main.language_open && enabled {
            let mut list = header_at((104., 40., 168., 144.))
                .id("udl-language-list").role(Role::ListBox).aria_label(label)
                .overflow_y_scroll().border_1().border_color(rgb(0x808080)).bg(rgb(0xffffff));
            for (index, (id, name)) in entries.into_iter().enumerate() {
                list = list.child(div().id(gpui::SharedString::from(format!("udl-language-{id}")))
                    .role(Role::ListBoxOption).aria_label(name.clone()).aria_selected(id == self.udl.selected)
                    .h(dialog_font::control_height()).w_full().px(px(6.)).flex().items_center()
                    .bg(rgb(if index == self.udl.main.language_cursor { 0xcddff5 } else { 0xffffff }))
                    .child(div().flex_1().min_w(px(0.)).truncate().child(name))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.udl_main_enabled() { return; }
                        view.udl.main.language_open = false;
                        view.udl_select(id, window, cx);
                    })));
            }
            panel = panel.child(list);
        }
        Some(panel.into_any_element())
    }

    pub(in super::super) fn render_udl_name(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.udl_name_visible() { return None; }
        let locale = self.native_locale();
        let name = localized_string(&locale, "common-name").unwrap_or("Name");
        let ok = localized_string(&locale, "common-ok").unwrap_or("OK");
        let cancel = localized_string(&locale, "common-cancel").unwrap_or("Cancel");
        let enabled = !self.udl.styler_visible && self.udl.ready;
        let palette = self.preferences_render_palette();
        self.udl.name.update(cx, |field, cx| {
            field.set_label(name, cx);
            field.set_compact_height(dialog_font::logical(dialog_font::control_height()), cx);
            field.set_render_palette(Some(palette), cx);
            if field.enabled() != enabled { field.set_enabled(enabled, cx); }
        });
        Some(div().id("udl-name-dialog").role(Role::Dialog).aria_label(self.udl_name_title())
            .relative().w(px(NAME_WIDTH)).h(px(NAME_HEIGHT)).text_size(gpui::px(dialog_font::base_size())).line_height(dialog_font::line_height())
            .bg(rgb(0xf0f0f0)).text_color(rgb(0x202020))
            .on_action(cx.listener(|view, _: &CloseWindow, _, cx| {
                view.dismiss_udl_name(cx); cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &EditorEscape, _, cx| {
                view.dismiss_udl_name(cx); cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => view.dismiss_udl_name(cx),
                    "enter" => view.commit_udl_name(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(header_at((12., 12., NAME_WIDTH - 24., 14.)).id("udl-resource-26001").child(name))
            .child(header_at((12., 36., NAME_WIDTH - 24., 24.)).id("udl-resource-26002").child(self.udl.name.clone()))
            .child(button(1, ok, (148., 84., 76., 24.), enabled)
                .on_click(cx.listener(|view, _, window, cx| { view.commit_udl_name(window, cx); }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        view.commit_udl_name(window, cx); cx.stop_propagation();
                    }
                })))
            .child(button(2, cancel, (232., 84., 76., 24.), true)
                .on_click(cx.listener(|view, _, _, cx| { view.dismiss_udl_name(cx); }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        view.dismiss_udl_name(cx); cx.stop_propagation();
                    }
                })))
            .into_any_element())
    }
}
