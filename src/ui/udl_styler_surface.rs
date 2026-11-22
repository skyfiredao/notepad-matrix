use super::*;
use crate::localization::Text;
use crate::udl::{Style, nesting};
use crate::udl_controls::{Control, Kind, STYLE_NAMES};
use objc2::{class, msg_send};
use objc2::runtime::{AnyObject, Bool};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Font,
    Size,
}

struct Session {
    language: u64,
    style: usize,
    initial_style: Style,
    root: Option<PathBuf>,
    generation: u64,
    token: u64,
}

pub(super) struct State {
    session: Option<Session>,
    serial: u64,
    fonts: Vec<String>,
    choice: Option<Choice>,
    choice_cursor: usize,
    choice_scroll: gpui::ScrollHandle,
    color_target: Option<(u64, bool)>,
    color_last: Option<u32>,
    color_task: Option<gpui::Task<()>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            session: None,
            serial: 0,
            fonts: Vec::new(),
            choice: None,
            choice_cursor: 0,
            choice_scroll: gpui::ScrollHandle::new(),
            color_target: None,
            color_last: None,
            color_task: None,
        }
    }
}

fn source_label(locale: &str, id: u32, fallback: &'static str) -> &'static str {
    crate::localization::dialog_control(locale, "UserDefine/StylerDialog", id)
        .unwrap_or(fallback)
}

fn source_at((x, y, width, height): (f32, f32, f32, f32)) -> gpui::Div {
    div()
        .absolute()
        .left(du(if crate::localization::current_preference_rtl() {
            324. - x - width
        } else {
            x
        }))
        .top(du(y))
        .w(du(width))
        .h(du(height))
        .when(crate::localization::current_preference_rtl(), |element| element.text_right())
}

fn style_control(style: usize, field: &str, label: &'static str, kind: Kind) -> Control {
    Control {
        group: String::new(),
        label,
        path: format!("/styles/{style}/{field}"),
        kind,
    }
}

impl MatrixWindow {
    fn udl_styler_session_valid(&self) -> bool {
        self.udl.styler.session.as_ref().is_some_and(|session| {
            self.udl.styler_visible
                && self.udl.ready
                && session.generation == self.udl.generation
                && session.root == self.udl.root
                && session.root == self.settings_store.as_ref().map(|store| store.root().to_path_buf())
                && self.udl.registry.definition(session.language)
                    .and_then(|definition| definition.styles.get(session.style))
                    .is_some_and(|style| style.id == session.initial_style.id)
        })
    }

    fn udl_styler_token_valid(&self, token: u64) -> bool {
        self.udl_styler_session_valid()
            && self.udl.styler.session.as_ref().is_some_and(|session| session.token == token)
    }

    fn udl_styler_style(&self) -> Option<&Style> {
        let session = self.udl.styler.session.as_ref()?;
        self.udl.registry.definition(session.language)?.styles.get(session.style)
    }

    pub(in super::super) fn open_udl_styler(
        &mut self,
        style: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.udl.styler_visible
            || !self.udl.ready
            || self.udl.busy
            || self.udl.prompting
            || !self.udl.jobs.is_empty()
            || style >= STYLE_NAMES.len()
        {
            return;
        }
        let Some(initial_style) = self.udl.registry.definition(self.udl.selected)
            .and_then(|definition| definition.styles.get(style)).cloned()
        else {
            return;
        };
        self.udl.styler.serial = self.udl.styler.serial.wrapping_add(1);
        self.udl.styler.session = Some(Session {
            language: self.udl.selected,
            style,
            initial_style,
            root: self.udl.root.clone(),
            generation: self.udl.generation,
            token: self.udl.styler.serial,
        });
        let mut fonts = cx.text_system().all_font_names();
        fonts.sort();
        fonts.dedup();
        fonts.insert(0, String::new());
        self.udl.styler.fonts = fonts;
        self.udl.styler.choice = None;
        self.udl.style = style;
        self.udl.styler_visible = true;
        self.udl.source_fields.retain(|key, _| !key.contains(":/styles/"));
        self.udl.error = None;
        self.present_floating(floating_windows::DialogKind::UdlStyler, window, cx);
        cx.notify();
    }

    pub(in super::super) fn preview_udl_styler_control(
        &mut self,
        control: &Control,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !control.path.starts_with("/styles/") {
            return false;
        }
        if !self.udl_styler_session_valid() {
            return true;
        }
        let Some(session) = self.udl.styler.session.as_ref() else { return true; };
        if !control.path.starts_with(&format!("/styles/{}/", session.style)) {
            return true;
        }
        let language = session.language;
        let style = session.style;
        let result = self.udl.registry.definition(language).cloned()
            .ok_or_else(|| std::io::Error::other("Unknown language"))
            .and_then(|mut definition| {
                control.apply(&mut definition, text)?;
                definition.styles.get(style).cloned()
                    .ok_or_else(|| std::io::Error::other("Unknown style"))
            })
            .and_then(|style| self.udl.registry.preview_style(language, style));
        match result {
            Ok(changed) => {
                let cleared_error = self.udl.error.take().is_some();
                if changed {
                    self.udl.needs_style_invalidation = true;
                }
                if changed || cleared_error {
                    cx.notify();
                }
            }
            Err(error) => {
                self.udl.error = Some(Text::resource(
                    "matrix-udl-error", "User-defined language error: $ERROR$",
                ).replace("$ERROR$", error.to_string()));
                cx.notify();
            }
        }
        true
    }

    pub(in super::super) fn finish_udl_styler(&mut self, commit: bool, cx: &mut Context<Self>) {
        if commit {
            self.sync_udl_styler_color(cx);
        }
        let valid = self.udl_styler_session_valid();
        let final_style = self.udl_styler_style().cloned();
        let revision = self.udl.registry.revision();
        self.udl.styler.color_task = None;
        self.udl.styler.color_last = None;
        if self.udl.styler.color_target.take().is_some() {
            unsafe {
                let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                let _: () = msg_send![panel, orderOut: std::ptr::null::<AnyObject>()];
            }
        }
        let Some(session) = self.udl.styler.session.as_ref() else {
            self.udl.styler_visible = false;
            return;
        };
        let language = session.language;
        let style = session.style;
        if valid {
            let result = self.udl.registry.preview_style(language, session.initial_style.clone())
                .and_then(|_| {
                    if !commit { return Ok(()); }
                    let final_style = final_style.as_ref()
                        .ok_or_else(|| std::io::Error::other("Unknown style"))?;
                    self.udl.registry.edit(language, |definition| {
                        let target = definition.styles.get_mut(style)
                            .ok_or_else(|| std::io::Error::other("Unknown style"))?;
                        *target = final_style.clone();
                        Ok(())
                    })
                });
            if let Err(mut error) = result {
                if commit && let Some(final_style) = final_style {
                    if let Err(preview_error) = self.udl.registry.preview_style(language, final_style) {
                        error = std::io::Error::other(format!("{error}; {preview_error}"));
                    }
                }
                self.udl.error = Some(Text::resource(
                    "matrix-udl-error", "User-defined language error: $ERROR$",
                ).replace("$ERROR$", error.to_string()));
                self.udl.needs_style_invalidation = true;
                cx.notify();
                return;
            }
            self.udl.error = None;
        }
        self.udl.styler.session = None;
        self.udl.styler_visible = false;
        self.udl.styler.choice = None;
        self.udl.source_fields.retain(|key, _| !key.contains(":/styles/"));
        self.udl.needs_style_invalidation = true;
        if commit && self.udl.registry.revision() != revision {
            self.udl.pending_save = true;
        }
        self.start_udl_io(cx);
        self.resume_udl_close_if_ready(cx);
        cx.notify();
    }

    fn toggle_udl_styler_control(
        &mut self,
        token: u64,
        control: &Control,
        cx: &mut Context<Self>,
    ) {
        if !self.udl_styler_token_valid(token) {
            return;
        }
        let Some(session) = self.udl.styler.session.as_ref() else { return; };
        let style = session.style;
        let enabled = self.udl.registry.definition(session.language)
            .is_some_and(|definition| control.text(definition) == "true");
        let inherited_color = if !enabled && control.path.ends_with("/color_style") {
            match control.kind {
                Kind::Bit(1) if self.udl_styler_style().is_some_and(|style| style.foreground.is_none()) => {
                    Some(("foreground", self.udl_styler_color_value(false)))
                }
                Kind::Bit(2) if self.udl_styler_style().is_some_and(|style| style.background.is_none()) => {
                    Some(("background", self.udl_styler_color_value(true)))
                }
                _ => None,
            }
        } else {
            None
        };
        self.preview_udl_styler_control(control, if enabled { "false" } else { "true" }, cx);
        if let Some((field, value)) = inherited_color {
            self.preview_udl_styler_control(
                &style_control(style, field, "", Kind::Color), &format!("#{value:06X}"), cx,
            );
        }
    }

    fn udl_styler_check(
        &self,
        token: u64,
        id: u32,
        control: Control,
        rect: (f32, f32, f32, f32),
        enabled: bool,
        inverted: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.preferences_render_palette();
        let label = source_label(&self.native_locale(), id, control.label);
        let checked = self.udl_styler_style().is_some_and(|style| match control.kind {
            Kind::Bit(bit) if control.path.ends_with("/font_style") => style.font_style.max(0) as u32 & bit != 0,
            Kind::Bit(bit) if control.path.ends_with("/color_style") => style.color_style.max(0) as u32 & bit != 0,
            Kind::Bit(bit) => style.nesting & bit != 0,
            _ => false,
        }) ^ inverted;
        let keyboard_control = control.clone();
        source_at(rect)
            .id(gpui::SharedString::from(format!("udl-styler-{id}")))
            .role(Role::CheckBox).aria_label(label).aria_toggled(checked.into())
            .tab_index(if enabled { 0 } else { -1 })
            .a11y_synthetic_children(move |builder| {
                if !enabled { builder.parent_node().set_disabled(); }
            })
            .flex().items_center().gap(du(2.))
            .when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .text_color(rgb(palette.control_text(enabled)))
            .child(preferences_surface::pref_indicator_palette(checked, false, enabled, palette))
            .child(label)
            .on_click(cx.listener(move |view, _, _, cx| {
                if enabled { view.toggle_udl_styler_control(token, &control, cx); }
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                if enabled && event.keystroke.key == "space" {
                    view.toggle_udl_styler_control(token, &keyboard_control, cx);
                    cx.stop_propagation();
                }
            })).into_any_element()
    }

    fn udl_styler_choice_options(&self, choice: Choice) -> Vec<String> {
        match choice {
            Choice::Font => self.udl.styler.fonts.clone(),
            Choice::Size => crate::style_config::FONT_SIZES.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn open_udl_styler_choice(&mut self, token: u64, choice: Choice, cx: &mut Context<Self>) {
        if !self.udl_styler_token_valid(token) { return; }
        if self.udl.styler.choice == Some(choice) {
            self.udl.styler.choice = None;
        } else {
            let text = self.udl_styler_choice_text(choice);
            self.udl.styler.choice_cursor = self.udl_styler_choice_options(choice)
                .iter().position(|option| option == &text).unwrap_or(0);
            self.udl.styler.choice = Some(choice);
            self.udl.styler.choice_scroll.scroll_to_item(self.udl.styler.choice_cursor);
        }
        cx.notify();
    }

    fn udl_styler_choice_text(&self, choice: Choice) -> String {
        self.udl_styler_style().map(|style| match choice {
            Choice::Font => style.font_name.clone(),
            Choice::Size if style.font_size < 0 => String::new(),
            Choice::Size => style.font_size.to_string(),
        }).unwrap_or_default()
    }

    fn choose_udl_styler_value(&mut self, token: u64, choice: Choice, index: usize, cx: &mut Context<Self>) {
        if !self.udl_styler_token_valid(token) { return; }
        let Some(value) = self.udl_styler_choice_options(choice).get(index).cloned() else { return; };
        let Some(session) = self.udl.styler.session.as_ref() else { return; };
        let control = match choice {
            Choice::Font => style_control(session.style, "font_name", "Name:", Kind::Text),
            Choice::Size => style_control(session.style, "font_size", "Size:", Kind::Integer { min: -1, max: 200 }),
        };
        self.udl.styler.choice = None;
        self.preview_udl_styler_control(&control, &value, cx);
        cx.notify();
    }

    fn udl_styler_combo(&self, token: u64, choice: Choice, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let (id, label_id, fallback, y) = match choice {
            Choice::Font => (25004, 25031, "Name:", 29.),
            Choice::Size => (25005, 25032, "Size:", 49.),
        };
        let text = self.udl_styler_choice_text(choice);
        source_at((65., y, 104., 13.))
            .h(dialog_font::control_height())
            .id(gpui::SharedString::from(format!("udl-styler-{id}")))
            .role(Role::ComboBox).aria_label(source_label(&self.native_locale(), label_id, fallback))
            .aria_value(text.clone()).aria_expanded(self.udl.styler.choice == Some(choice)).tab_index(0)
            .border_1().border_color(rgb(palette.edge)).bg(rgb(palette.control_background(true)))
            .text_color(rgb(palette.text)).flex().items_center().justify_between().px(du(2.))
            .when(crate::localization::current_preference_rtl(), |element| element.flex_row_reverse())
            .overflow_hidden().child(text).child("⌄")
            .on_click(cx.listener(move |view, _, _, cx| view.open_udl_styler_choice(token, choice, cx)))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                if !view.udl_styler_token_valid(token) { return; }
                let key = event.keystroke.key.as_str();
                if key == "escape" && view.udl.styler.choice == Some(choice) {
                    view.udl.styler.choice = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !matches!(key, "up" | "down" | "enter" | "space") { return; }
                if view.udl.styler.choice != Some(choice) {
                    view.open_udl_styler_choice(token, choice, cx);
                } else if matches!(key, "enter" | "space") {
                    view.choose_udl_styler_value(token, choice, view.udl.styler.choice_cursor, cx);
                } else {
                    let length = view.udl_styler_choice_options(choice).len();
                    view.udl.styler.choice_cursor = if key == "up" {
                        view.udl.styler.choice_cursor.saturating_sub(1)
                    } else {
                        (view.udl.styler.choice_cursor + 1).min(length.saturating_sub(1))
                    };
                    view.udl.styler.choice_scroll.scroll_to_item(view.udl.styler.choice_cursor);
                }
                cx.stop_propagation();
                cx.notify();
            })).into_any_element()
    }

    fn udl_styler_color_value(&self, background: bool) -> u32 {
        self.udl_styler_style()
            .and_then(|style| if background { style.background } else { style.foreground })
            .unwrap_or_else(|| {
                let profile = self.editor_style_profile();
                if background { profile.background } else { profile.foreground }
            })
    }

    fn sync_udl_styler_color(&mut self, cx: &mut Context<Self>) {
        let Some((token, background)) = self.udl.styler.color_target else { return; };
        if !self.udl_styler_token_valid(token) { return; }
        let Some(value) = style_config_surface::color_panel_rgb() else { return; };
        if self.udl.styler.color_last == Some(value) { return; }
        let Some(session) = self.udl.styler.session.as_ref() else { return; };
        let control = style_control(session.style, if background { "background" } else { "foreground" }, "", Kind::Color);
        self.preview_udl_styler_control(&control, &format!("#{value:06X}"), cx);
        self.udl.styler.color_last = Some(value);
    }

    pub(in crate::ui) fn finish_udl_styler_color(&mut self, cx: &mut Context<Self>) {
        self.sync_udl_styler_color(cx);
        self.udl.styler.color_target = None;
        self.udl.styler.color_task = None;
        self.udl.styler.color_last = None;
    }

    fn choose_udl_styler_color(&mut self, token: u64, background: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.udl_styler_token_valid(token) { return; }
        self.finish_terminal_color(cx);
        if !self.finish_preference_color(window, cx) { return; }
        self.finish_style_color(window, cx);
        self.sync_udl_styler_color(cx);
        let value = self.udl_styler_color_value(background);
        unsafe {
            let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
            let color: *mut AnyObject = msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64];
            let _: () = msg_send![panel, setShowsAlpha: false];
            let _: () = msg_send![panel, setColor: color];
            let _: () = msg_send![panel, orderFront: std::ptr::null::<AnyObject>()];
        }
        self.udl.styler.color_target = Some((token, background));
        self.udl.styler.color_last = Some(value);
        let executor = cx.background_executor().clone();
        self.udl.styler.color_task = Some(cx.spawn(async move |owner, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(50)).await;
                let keep = owner.update(cx, |view, cx| {
                    if !view.udl_styler_token_valid(token)
                        || view.udl.styler.color_target != Some((token, background)) {
                        return false;
                    }
                    let visible: Bool = unsafe {
                        let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
                        msg_send![panel, isVisible]
                    };
                    view.sync_udl_styler_color(cx);
                    if !visible.as_bool() {
                        view.udl.styler.color_target = None;
                        view.udl.styler.color_last = None;
                    }
                    visible.as_bool()
                }).unwrap_or(false);
                if !keep { break; }
            }
        }));
    }

    fn udl_styler_color_square(&self, token: u64, background: bool, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let style = self.udl_styler_style();
        let (id, bit, x, fallback) = if background { (25007, 2, 231.33333, "Background color:") } else { (25006, 1, 92.33333, "Foreground color:") };
        let enabled = style.is_some_and(|style| style.color_style & bit != 0);
        let value = self.udl_styler_color_value(background);
        source_at((x, 70.666664, 16.666666, 16.666666))
            .id(gpui::SharedString::from(format!("udl-styler-color-{id}")))
            .role(Role::Button).aria_label(source_label(&self.native_locale(), id, fallback))
            .tab_index(0)
            .border_1().border_color(rgb(palette.control_edge(enabled)))
            .bg(rgb(if enabled { value } else { palette.control_background(false) }))
            .on_click(cx.listener(move |view, _, window, cx| {
                view.choose_udl_styler_color(token, background, window, cx);
            }))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |view, _, _, cx| {
                if !view.udl_styler_token_valid(token) { return; }
                let Some(session) = view.udl.styler.session.as_ref() else { return; };
                let control = style_control(session.style, "color_style", "Transparent", Kind::Bit(if background { 2 } else { 1 }));
                view.toggle_udl_styler_control(token, &control, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    view.choose_udl_styler_color(token, background, window, cx);
                    cx.stop_propagation();
                }
            })).into_any_element()
    }

    pub(in super::super) fn render_udl_styler(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.udl.styler_visible { return None; }
        if !self.udl_styler_session_valid() {
            self.finish_udl_styler(false, cx);
            return None;
        }
        let session = self.udl.styler.session.as_ref()?;
        let token = session.token;
        let style = session.style;
        let nesting_enabled = matches!(style, 1 | 2 | 16..=23);
        let locale = self.native_locale();
        let palette = self.preferences_render_palette();
        let mut panel = div().id("udl-styler-dialog")
            .role(Role::Dialog)
            .aria_label(crate::localization::dialog_title(&locale, "UserDefine/StylerDialog").unwrap_or("Styler Dialog"))
            .track_focus(&self.udl.styler_focus).relative().w(du(324.)).h(du(305.))
            .bg(rgb(palette.pure_background)).text_color(rgb(palette.text))
            .font_family(dialog_font::font_family())
            .text_size(gpui::px(dialog_font::base_size())).line_height(dialog_font::line_height())
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && view.udl_styler_token_valid(token) {
                    view.finish_udl_styler(true, cx);
                    if !view.udl.styler_visible {
                        view.dismiss_floating(floating_windows::DialogKind::UdlStyler, window, cx);
                    }
                    cx.stop_propagation();
                }
            }));
        for (id, fallback, rect) in [
            (25030, "Font options", (14., 15., 295., 95.)),
            (25029, "Nesting", (14., 116., 295., 157.)),
        ] {
            panel = panel.child(source_at(rect)
                .child(div().absolute().top(du(4.)).left(du(0.)).right(du(0.)).bottom(du(0.))
                    .border_1().border_color(rgb(palette.group_edge())))
                .child(div().absolute().top(du(0.)).left(du(7.)).px(du(2.))
                    .bg(rgb(palette.pure_background)).child(source_label(&locale, id, fallback))));
        }
        for (id, fallback, rect) in [
            (25031, "Name:", (26., 30., 36., 8.)),
            (25032, "Size:", (25., 50., 36., 8.)),
            (25006, "Foreground color:", (14., 75., 75., 8.)),
            (25007, "Background color:", (153., 75., 75., 8.)),
        ] {
            panel = panel.child(source_at(rect).text_right().child(source_label(&locale, id, fallback)));
        }
        panel = panel.child(self.udl_styler_combo(token, Choice::Font, cx))
            .child(self.udl_styler_combo(token, Choice::Size, cx));
        for (id, label, bit, y) in [(25001, "Bold", 1, 23.), (25002, "Italic", 2, 37.), (25003, "Underline", 4, 51.)] {
            panel = panel.child(self.udl_styler_check(token, id,
                style_control(style, "font_style", label, Kind::Bit(bit)),
                (201., y, 90., 10.), true, false, cx));
        }
        for (id, bit, x) in [(25033, 1, 33.), (25034, 2, 172.)] {
            panel = panel.child(self.udl_styler_check(token, id,
                style_control(style, "color_style", "Transparent", Kind::Bit(bit)),
                (x, 92., 78., 10.), true, true, cx));
        }
        panel = panel.child(self.udl_styler_color_square(token, false, cx))
            .child(self.udl_styler_color_square(token, true, cx));
        for index in 0..8 {
            let y = 132. + index as f32 * 17.;
            panel = panel.child(self.udl_styler_check(token, 25008 + index as u32,
                style_control(style, "nesting", ["Delimiter 1", "Delimiter 2", "Delimiter 3", "Delimiter 4", "Delimiter 5", "Delimiter 6", "Delimiter 7", "Delimiter 8"][index], Kind::Bit(nesting::delimiter(index))),
                (38., y, 76., 10.), nesting_enabled, false, cx));
            panel = panel.child(self.udl_styler_check(token, 25018 + index as u32,
                style_control(style, "nesting", ["Keyword 1", "Keyword 2", "Keyword 3", "Keyword 4", "Keyword 5", "Keyword 6", "Keyword 7", "Keyword 8"][index], Kind::Bit(nesting::keyword(index))),
                (119., y, 76., 10.), nesting_enabled, false, cx));
        }
        for (index, (id, label, bit)) in [
            (25016, "Comment", nesting::COMMENT),
            (25017, "Comment line", nesting::COMMENT_LINE),
            (25026, "Operators 1", nesting::OPERATORS1),
            (25027, "Operators 2", nesting::OPERATORS2),
            (25028, "Numbers", nesting::NUMBERS),
        ].into_iter().enumerate() {
            panel = panel.child(self.udl_styler_check(token, id,
                style_control(style, "nesting", label, Kind::Bit(bit)),
                (209., 132. + index as f32 * 17., 76., 10.), nesting_enabled, false, cx));
        }
        for (id, label, x, commit) in [(1, "OK", 100., true), (2, "Cancel", 172., false)] {
            let label = source_label(&locale, id, label);
            panel = panel.child(source_at((x, 280., 50., 14.))
                .h(dialog_font::control_height())
                .id(gpui::SharedString::from(format!("udl-styler-{id}")))
                .role(Role::Button).aria_label(label).tab_index(0)
                .border_1().border_color(rgb(palette.edge)).bg(rgb(palette.button_background(true)))
                .flex().items_center().justify_center().child(label)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !view.udl_styler_token_valid(token) { return; }
                    view.finish_udl_styler(commit, cx);
                    if !view.udl.styler_visible {
                        view.dismiss_floating(floating_windows::DialogKind::UdlStyler, window, cx);
                    }
                }))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                    if view.udl_styler_token_valid(token) && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        view.finish_udl_styler(commit, cx);
                        if !view.udl.styler_visible {
                            view.dismiss_floating(floating_windows::DialogKind::UdlStyler, window, cx);
                        }
                        cx.stop_propagation();
                    }
                })));
        }
        if let Some(error) = &self.udl.error {
            panel = panel.child(source_at((14., 295., 295., 10.)).bg(rgb(palette.error_background)).child(error.render(&locale)));
        }
        if let Some(choice) = self.udl.styler.choice {
            let options = self.udl_styler_choice_options(choice);
            let (id, y, height, label_id, fallback) = match choice {
                Choice::Font => (25004, 46., 67., 25031, "Name:"),
                Choice::Size => (25005, 66., 64., 25032, "Size:"),
            };
            let mut popup = source_at((65., y, 104., height))
                .id(gpui::SharedString::from(format!("udl-styler-list-{id}")))
                .role(Role::ListBox).aria_label(source_label(&locale, label_id, fallback))
                .border_1().border_color(rgb(palette.edge)).bg(rgb(palette.softer_background))
                .overflow_y_scroll().track_scroll(&self.udl.styler.choice_scroll);
            for (index, option) in options.into_iter().enumerate() {
                let selected = index == self.udl.styler.choice_cursor;
                popup = popup.child(div().id(gpui::SharedString::from(format!("udl-styler-option-{id}-{index}")))
                    .role(Role::ListBoxOption).aria_label(option.clone()).aria_selected(selected)
                    .h(dialog_font::control_height()).min_h(dialog_font::control_height())
                    .line_height(dialog_font::line_height()).w_full().px(du(2.))
                    .bg(rgb(if selected { palette.hot_background } else { palette.softer_background }))
                    .hover(move |style| style.bg(rgb(palette.hot_background)))
                    .child(option).on_click(cx.listener(move |view, _, _, cx| {
                        view.choose_udl_styler_value(token, choice, index, cx);
                    })));
            }
            panel = panel.child(popup);
        }
        Some(panel.into_any_element())
    }
}
