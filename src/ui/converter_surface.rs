use super::*;
use super::dialog_font::px;
use crate::converter::{Configuration, Field, PanelValues};
use crate::panel_layout::{PanelId, Placement};
use gpui::Entity;

pub(super) struct State {
    fields: [Entity<text_field::TextField>; 5],
    buttons: [[FocusHandle; 2]; 5],
    values: PanelValues,
    configuration: Option<Configuration>,
    configuration_error: Option<String>,
    initialized: bool,
    changed: Vec<gpui::Subscription>,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            fields: Field::ALL.map(|field| cx.new(|cx| {
                let input = text_field::TextField::compact(field.label(), field.selector(), cx)
                    .without_history_arrow()
                    .with_compact_height(18.)
                    .with_tab_index((field.index() * 3) as isize);
                if field == Field::Ascii { input.with_max_utf16(1) } else { input }
            })),
            buttons: Field::ALL.map(|field| [
                cx.focus_handle().tab_index((field.index() * 3 + 1) as isize).tab_stop(true),
                cx.focus_handle().tab_index((field.index() * 3 + 2) as isize).tab_stop(true),
            ]),
            values: PanelValues::default(),
            configuration: None,
            configuration_error: None,
            initialized: false,
            changed: Vec::new(),
        }
    }
}

impl MatrixWindow {
    pub(super) fn initialize_converter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.converter_state.initialized { return; }
        self.converter_state.initialized = true;
        for field in Field::ALL {
            let input = self.converter_state.fields[field.index()].clone();
            let changed = cx.observe_in(&input, window, move |view, input, window, cx| {
                if input.read(cx).has_composition() { return; }
                let text = input.read(cx).text();
                if view.converter_state.values.text[field.index()] != text {
                    view.converter_field_changed(field, text, window, cx);
                }
            });
            self.converter_state.changed.push(changed);
        }
        if !self.converter_enabled { return; }
        let configuration = self.settings_store.as_ref()
            .ok_or_else(|| io::Error::other("Converter has no private configuration store"))
            .and_then(Configuration::load);
        match configuration {
            Ok(configuration) => self.converter_state.configuration = Some(configuration),
            Err(error) => {
                let message = format!("Converter configuration could not be loaded: {error}");
                self.converter_state.configuration_error = Some(message.clone());
                self.notify_editor_error(message, cx);
            }
        }
    }

    fn converter_field_changed(&mut self, field: Field, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.converter_state.values.set_from(field, text);
        for other in Field::ALL {
            let input = self.converter_state.fields[other.index()].clone();
            let text = &self.converter_state.values.text[other.index()];
            if input.read(cx).text() != *text {
                input.update(cx, |input, cx| {
                    input.set_text(text, window, cx);
                    if other == field { input.collapse_to_end(window, cx); }
                });
            }
        }
        cx.notify();
    }

    pub(super) fn converter_panel_visible(&self) -> bool {
        self.panels.layout.panel(PanelId::Converter).visible
    }

    pub(super) fn show_converter_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.show_converter_panel(window, cx)) { return; }
        if !self.converter_enabled || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.initialize_converter(window, cx);
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.show_panel(PanelId::Converter, window, cx);
        if self.panels.layout.panel(PanelId::Converter).placement != Placement::Floating {
            self.focus_converter_panel(window, cx);
        }
    }

    pub(super) fn focus_converter_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.converter_state.fields[0].update(cx, |input, cx| input.focus(window, cx));
    }

    pub(super) fn converter_panel_input(&self) -> gpui::Entity<text_field::TextField> {
        self.converter_state.fields[0].clone()
    }

    pub(super) fn edit_converter_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.edit_converter_configuration(window, cx)) { return; }
        if !self.converter_enabled || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.initialize_converter(window, cx);
        let path = self.settings_store.as_ref()
            .ok_or_else(|| io::Error::other("Converter has no private configuration store"))
            .and_then(Configuration::path)
            .and_then(|path| {
                let metadata = std::fs::symlink_metadata(&path)?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(io::Error::other("converter.ini must be a regular private configuration file"));
                }
                Ok(path)
            });
        match path {
            Ok(path) => {
                self.menu_input(MenuEvent::Dismiss, window, cx);
                self.start_open_paths(vec![path], window, cx);
            }
            Err(error) => self.converter_prompt("Configuration file is absent", &format!("converter.ini could not be opened: {error}\nPlease create this file manually."), window, cx),
        }
    }

    pub(super) fn show_converter_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.converter_enabled { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.converter_prompt("Converter Plugin", crate::converter::ABOUT, window, cx);
    }

    fn converter_prompt(&mut self, title: &str, message: &str, window: &mut Window, cx: &mut Context<Self>) {
        let response = window.prompt(gpui::PromptLevel::Info, title, Some(message), &[gpui::PromptButton::ok("OK")], cx);
        cx.spawn(async move |_, _| {
            if let Err(error) = response.await { eprintln!("Converter prompt could not be shown: {error}"); }
        }).detach();
    }

    fn converter_panel_insert(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.converter_panel_insert(field, window, cx)) { return; }
        if !self.converter_enabled || !self.converter_panel_visible() || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        if self.editor().has_composition() || self.converter_state.fields.iter().any(|field| field.read(cx).has_composition()) {
            self.notify_editor_error(crate::editor::EditError::UnsupportedComposition.to_string(), cx);
            return;
        }
        let Some(bytes) = self.converter_state.values.bytes(field) else { return; };
        let result = self.editor_mut().replace_selection_bytes(&bytes);
        self.editor_changed(result, window, cx);
    }

    fn converter_panel_copy(&mut self, field: Field, cx: &mut Context<Self>) {
        if !self.converter_enabled || !self.converter_panel_visible() { return; }
        if self.converter_state.fields.iter().any(|field| field.read(cx).has_composition()) {
            self.notify_editor_error(crate::editor::EditError::UnsupportedComposition.to_string(), cx);
            return;
        }
        let Some(bytes) = self.converter_state.values.bytes(field) else { return; };
        if let Err(error) = crate::native_binary_clipboard::write(&objc2_app_kit::NSPasteboard::generalPasteboard(), &bytes) {
            self.notify_editor_error(error.to_string(), cx);
        }
    }

    pub(super) fn converter_transform(&mut self, operation: crate::converter::ConverterOperation, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.converter_transform(operation, window, cx)) { return; }
        if !self.converter_enabled || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        let range = self.editor().document().selection().range();
        if range.is_empty() { return; }
        if self.editor().has_composition() {
            self.notify_editor_error(crate::editor::EditError::UnsupportedComposition.to_string(), cx);
            return;
        }
        self.initialize_converter(window, cx);
        let Some(configuration) = self.converter_state.configuration else {
            let message = self.converter_state.configuration_error.clone().unwrap_or_else(|| "Converter configuration has not been loaded".into());
            self.converter_prompt("Converter plugin", &message, window, cx);
            return;
        };
        let selected = match self.editor().document().byte_snapshot().slice(range.start.0..range.end.0) {
            Ok(bytes) => bytes.to_vec(),
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        };
        match crate::converter::transform_bytes(&selected, operation, self.editor().line_ending(), configuration) {
            Ok(bytes) => {
                let revision = self.editor().document().text_revision();
                let result = self.editor_mut().replace_selection_bytes(&bytes).and_then(|()| {
                    self.editor_mut().document_mut().select_after_edit(Selection {
                        anchor: range.start,
                        head: ByteOffset(range.start.0 + bytes.len()),
                    }, &revision)
                });
                self.editor_changed(result, window, cx);
                self.editor_focus.focus(window, cx);
            }
            Err(error) => self.converter_prompt("Converter plugin", &error.to_string(), window, cx),
        }
    }

    pub(super) fn render_converter_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.converter_enabled || !self.converter_panel_visible() { return None; }
        let palette = self.preferences_render_palette();
        let mut panel = div()
            .id("converter-panel")
            .debug_selector(|| "converter-panel".into())
            .role(Role::Group)
            .aria_label("Conversion")
            .tab_group()
            .relative()
            .w_full()
            .h(px(213.))
            .min_w(px(486.))
            .flex_shrink_0()
            .font_family(dialog_font::font_family())
            .text_size(px(12.))
            .line_height(px(14.))
            .bg(rgb(palette.pure_background))
            .text_color(rgb(palette.text));
        let rows = [28.5, 64.5, 97.5, 133.5, 168.];
        for field in Field::ALL {
            let index = field.index();
            let y = rows[index];
            panel = panel
                .child(div().absolute().left(px(22.5)).top(px(y + 6.)).w(px(67.5)).text_right().whitespace_nowrap().child(field.label()))
                .child(div().absolute().left(px(97.5)).top(px(y + 1.5)).w(px(if field == Field::Ascii { 22.5 } else { 225. })).child(self.converter_state.fields[index].clone()));
            if field == Field::Ascii {
                panel = panel.child(div().absolute().left(px(127.5)).top(px(y + 6.)).w(px(97.5)).child(self.converter_state.values.ascii_info));
            }
            for insert in [false, true] {
                let id = (if insert { "converter-insert" } else { "converter-copy" }, index);
                let focus = self.converter_state.buttons[index][usize::from(insert)].clone();
                panel = panel.child(div()
                    .id(id)
                    .debug_selector(move || format!("{}-{}", field.selector(), if insert { "insert" } else { "copy" }).into())
                    .role(Role::Button)
                    .track_focus(&self.converter_state.buttons[index][usize::from(insert)])
                    .aria_label(format!("{} {}", if insert { "Insert" } else { "Copy" }, field.label().trim_end_matches(':')))
                    .absolute()
                    .left(px(if insert { 412.5 } else { 343.5 }))
                    .top(px(y))
                    .w(px(60.))
                    .h(px(21.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(rgb(palette.edge))
                    .bg(rgb(palette.button_background(true)))
                    .child(if insert { "Insert" } else { "Copy" })
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus.focus(window, cx);
                        cx.stop_propagation();
                    })
                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            if insert { view.converter_panel_insert(field, window, cx); }
                            else { view.converter_panel_copy(field, cx); }
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if insert { view.converter_panel_insert(field, window, cx); }
                        else { view.converter_panel_copy(field, cx); }
                    })));
            }
        }
        Some(panel.into_any_element())
    }
}
