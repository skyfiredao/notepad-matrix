use super::*;
use crate::character_panel::{CODE_PAGES, Table};
use crate::panel_layout::PanelId;

pub(super) struct State {
    pub(super) visible: bool,
    table: Table,
    selected: Option<u8>,
    choosing_encoding: bool,
    focus: FocusHandle,
    scroll: gpui::UniformListScrollHandle,
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn character_panel_selects_encoding_inserts_and_floats(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated character preferences");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab");
        cx.dispatch_action(EditorSelectAll);
        view.update_in(cx, |view, window, cx| {
            view.settings_store = Some(ConfigStore::under_home(home.path()));
            view.toggle_character_panel(window, cx);
        });
        let picker = cx
            .debug_bounds("character-code-page")
            .expect("encoding selector");
        cx.simulate_click(picker.center(), gpui::Modifiers::default());
        let code_page = cx
            .debug_bounds("character-code-page-1")
            .expect("explicit Windows-1252");
        cx.simulate_click(code_page.center(), gpui::Modifiers::default());
        view.update(cx, |view, cx| {
            view.characters
                .scroll
                .scroll_to_item(65, gpui::ScrollStrategy::Center);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let glyph = cx
            .debug_bounds("character-cell-65-2")
            .expect("character row");
        cx.simulate_click(glyph.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "A")
        });
        view.update_in(cx, |view, window, cx| {
            view.float_panel(PanelId::Characters, window, cx)
        });
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
        view.read_with(cx, |view, _| {
            assert_eq!(view.characters.table.encoding(), "windows-1252")
        });
        let saved = ConfigStore::under_home(home.path())
            .load_settings()
            .expect("saved explicit encoding");
        assert_eq!(saved["character_panel_encoding"], "windows-1252");
    }
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            table: Table::default(),
            selected: None,
            choosing_encoding: false,
            focus: cx.focus_handle(),
            scroll: gpui::UniformListScrollHandle::new(),
        }
    }
}

impl MatrixWindow {
    pub(super) fn configure_character_panel(
        &mut self,
        settings: &serde_json::Value,
    ) -> io::Result<()> {
        if let Some(value) = settings.get("character_panel_encoding") {
            let name = value.as_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "character_panel_encoding must be a name",
                )
            })?;
            self.characters.table.select_encoding(name)?;
        }
        Ok(())
    }

    pub(super) fn toggle_character_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.characters.visible {
            self.hide_panel(PanelId::Characters, window, cx);
        } else {
            self.show_panel(PanelId::Characters, window, cx);
        }
    }

    fn select_character_encoding(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(encoding) = CODE_PAGES.get(index) else {
            return;
        };
        let name = encoding.name();
        if let Some(store) = &self.settings_store
            && let Err(error) = store.update_settings(|settings| {
                settings["character_panel_encoding"] = name.into();
                Ok(())
            })
        {
            self.notify_editor_error(
                format!("Character panel preferences could not be saved: {error}"),
                cx,
            );
            return;
        }
        if let Err(error) = self.characters.table.select_encoding(name) {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        self.characters.choosing_encoding = false;
        cx.notify();
    }

    pub(super) fn insert_character_text(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = text.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.insert_character_text(pending, window, cx)
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
        self.record_macro_step(crate::macros::MacroStep::CharacterText(text.clone().into()));
        let result = crate::character_panel::insert(self.editor_mut(), &text);
        self.editor_changed(result, window, cx);
        self.editor_focus.focus(window, cx);
    }

    fn activate_character(
        &mut self,
        value: u8,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = self.characters.table.insertion(value, column) {
            self.insert_character_text(text, window, cx);
        }
    }

    pub(super) fn render_character_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.characters.visible {
            return None;
        }
        let dark = self.dark;
        Some(div().id("character-panel").debug_selector(|| "character-panel".into())
            .track_focus(&self.characters.focus).key_context("MatrixCharacterPanel")
            .size_full().flex().flex_col().min_h(px(0.)).min_w(px(0.))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "up" => view.characters.selected = Some(view.characters.selected.unwrap_or(0).saturating_sub(1)),
                    "down" => view.characters.selected = Some(view.characters.selected.map_or(0, |value| value.saturating_add(1))),
                    "enter" => { if let Some(value) = view.characters.selected { view.activate_character(value, 2, window, cx); } },
                    _ => return,
                }
                if let Some(value) = view.characters.selected { view.characters.scroll.scroll_to_item(usize::from(value), gpui::ScrollStrategy::Nearest); }
                cx.notify(); cx.stop_propagation();
            }))
            .child(div().id("character-code-page").debug_selector(|| "character-code-page".into()).role(Role::Button).px(px(4.)).py(px(4.))
                .child(format!("Encoding: {} ▾", self.characters.table.encoding()))
                .on_click(cx.listener(|view, _, _, cx| { view.characters.choosing_encoding = !view.characters.choosing_encoding; cx.notify(); })))
            .when(self.characters.choosing_encoding, |panel| panel.child(div().id("character-code-page-options").max_h(px(150.)).overflow_y_scroll()
                .children(CODE_PAGES.iter().enumerate().map(|(index, encoding)| div().id(format!("character-code-page-{index}"))
                    .debug_selector(move || format!("character-code-page-{index}")).role(Role::Button).px(px(4.)).py(px(2.)).child(encoding.name())
                    .on_click(cx.listener(move |view, _, _, cx| view.select_character_encoding(index, cx)))))))
            .child(div().id("character-table-scroll").flex_1().min_h(px(0.)).overflow_x_scroll()
                .child(div().w(px(470.)).h_full().flex().flex_col()
                    .child(div().flex().h(px(24.)).children(["Value", "Hex", "Character", "HTML Name", "HTML Decimal", "HTML Hexadecimal"].into_iter().zip([45.,45.,70.,90.,100.,120.]).map(|(name, width)| div().w(px(width)).flex_shrink_0().child(name))))
                    .child(gpui::uniform_list("character-table", 256, cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        range.map(|index| {
                            let value = index as u8;
                            let row = view.characters.table.row(value);
                            let selected = view.characters.selected == Some(value);
                            div().id(("character-row", index)).role(Role::ListBoxOption).aria_selected(selected).h(px(24.)).flex()
                                .when(selected, |row| row.bg(rgb(if dark { 0x30465a } else { 0xcce8ff })))
                                .children(row.cells.into_iter().zip([45.,45.,70.,90.,100.,120.]).enumerate().map(|(column, (text, width))| {
                                    div().id(format!("character-cell-{index}-{column}"))
                                        .debug_selector(move || format!("character-cell-{index}-{column}"))
                                        .w(px(width)).flex_shrink_0().overflow_hidden().px(px(2.)).child(text)
                                        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                                            view.characters.selected = Some(value);
                                            view.characters.focus.focus(window, cx);
                                            if event.click_count() == 2 { view.activate_character(value, column, window, cx); }
                                            cx.notify(); cx.stop_propagation();
                                        }))
                                })).into_any_element()
                        }).collect::<Vec<_>>()
                    })).track_scroll(&self.characters.scroll).flex_1().min_h(px(0.)))))
            .into_any_element())
    }
}
