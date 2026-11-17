use super::*;
use gpui::{Pixels, point};
use super::dialog_font::px;
use crate::shortcuts::{CommandKey, KeyCombo};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Popup { Variables, History, Keys }

pub(super) struct State {
    history: Vec<String>,
    popup: Option<Popup>,
    popup_bounds: Option<Bounds<Pixels>>,
    popup_focus: FocusHandle,
    popup_selected: usize,
    popup_scroll: gpui::ScrollHandle,
    controls: [FocusHandle; 3],
    save_visible: bool,
    save_name: gpui::Entity<text_field::TextField>,
    save_command: String,
    save_combo: KeyCombo,
    save_warning: bool,
    save_changed: Option<gpui::Subscription>,
    save_message: String,
    browse_task: Option<gpui::Task<()>>,
    launching: bool,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            history: Vec::new(), popup: None, popup_bounds: None,
            popup_focus: cx.focus_handle(), popup_selected: 0,
            popup_scroll: gpui::ScrollHandle::new(),
            controls: std::array::from_fn(|_| cx.focus_handle().tab_index(0).tab_stop(true)),
            save_visible: false,
            save_name: cx.new(|cx| text_field::TextField::compact("名称", "run-save-name", cx).without_history_arrow().with_max_utf16(127)),
            save_command: String::new(), save_combo: KeyCombo::default(), save_warning: false,
            save_changed: None, save_message: String::new(), browse_task: None, launching: false,
        }
    }
}

pub(super) fn dimensions() -> gpui::Size<Pixels> { size(px(703.5), px(140.)) }
pub(super) fn save_dimensions() -> gpui::Size<Pixels> { size(px(315.), px(190.)) }
pub(super) fn variables_dimensions() -> gpui::Size<Pixels> { size(px(445.), px(248.)) }

const DESCRIPTIONS: [&str; 11] = [
    "Full path to active file", "Active file's directory", "Active file's name",
    "File name without extension", "File extension (with .)", "Selected word or word under caret",
    "Notepad Matrix executable directory", "Full path of Notepad Matrix executable",
    "Line number of caret", "Column number of caret", "Current line text",
];

impl MatrixWindow {
    pub(super) fn move_run_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.run_visible || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        if self.run_state.save_visible {
            if forward { window.focus_next(cx); } else { window.focus_prev(cx); }
            return;
        }
        let controls = [
            self.run_input.read(cx).focus_handle(), self.run_state.controls[0].clone(),
            self.run_state.controls[1].clone(), self.run_button_focus[0].clone(),
            self.run_state.controls[2].clone(), self.run_button_focus[1].clone(),
        ];
        let current = controls.iter().position(|focus| focus.is_focused(window)).unwrap_or(0);
        controls[(current + if forward { 1 } else { 5 }) % 6].focus(window, cx);
        cx.notify();
    }

    pub(super) fn activate_run_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run_state.save_visible { self.confirm_run_save(window, cx); }
        else if self.run_button_focus[1].is_focused(window) { self.cancel_run(window, cx); }
        else if self.run_state.controls[0].is_focused(window) { self.browse_run(window, cx); }
        else if self.run_state.controls[1].is_focused(window) { self.open_run_popup(Popup::Variables, window, cx); }
        else if self.run_state.controls[2].is_focused(window) { self.show_run_save(window, cx); }
        else { self.confirm_run(window, cx); }
    }

    pub(super) fn show_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        if self.editor().has_composition() {
            self.notify_editor_error("请先完成文档中的输入法组合".into(), cx);
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.refresh_shortcuts(window, cx);
        self.run_visible = true;
        self.run_message.clear();
        self.present_floating(floating_windows::DialogKind::Run, window, cx);
        cx.notify();
    }

    pub(super) fn dismiss_run(&mut self, cx: &mut Context<Self>) {
        self.run_state.popup = None;
        self.run_state.save_visible = false;
        self.run_visible = false;
        cx.notify();
    }

    pub(super) fn cancel_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run_state.save_visible {
            self.dismiss_floating(floating_windows::DialogKind::RunSave, window, cx);
        } else if self.run_visible {
            self.dismiss_floating(floating_windows::DialogKind::Run, window, cx);
        }
    }

    fn run_values(&self) -> Result<[String; 11], String> {
        let editor = self.editor();
        let selection = editor.selection();
        let bytes = editor.document().byte_snapshot();
        let line = bytes.line_index(selection.head.0).map_err(|error| error.to_string())?;
        let mut range = selection.range();
        if range.is_empty() { range = editor.word_selection_at(selection.head).map_err(|error| error.to_string())?.range(); }
        let word = editor.document().text_in(range).map_err(|error| error.to_string())?;
        let path = self.disk_file().map(|file| file.path().to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from(self.workspace.active().name()));
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let value = |path: &std::path::Path| path.to_string_lossy().into_owned();
        let column = editor.document().display_column(selection.head, editor.tab_preferences().size).map_err(|error| error.to_string())?;
        Ok([
            value(&path), path.parent().map(value).unwrap_or_default(),
            path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
            path.file_stem().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
            path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default(),
            word, exe.parent().map(value).unwrap_or_default(), value(&exe),
            line.to_string(), column.to_string(),
            editor.document().line_text(line).map_err(|error| error.to_string())?,
        ])
    }

    fn add_run_history(&mut self, command: String) {
        if !self.run_state.history.iter().any(|entry| entry.eq_ignore_ascii_case(&command)) {
            self.run_state.history.push(command);
            self.run_state.history.sort_by_key(|entry| entry.to_lowercase());
        }
    }

    pub(super) fn confirm_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.confirm_run(window, cx)) { return; }
        if !self.run_visible || self.run_state.save_visible || self.run_state.launching || self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        if self.run_input.read(cx).has_composition() {
            self.run_message = "请先完成运行命令中的输入法组合".into(); cx.notify(); return;
        }
        let command = self.run_input.read(cx).text();
        let values = if crate::run_commands::VARIABLE_NAMES.iter().any(|name| command.contains(&format!("$({name})"))) {
            self.run_values()
        } else { Ok(std::array::from_fn(|_| String::new())) };
        let launched = values.map_err(io::Error::other)
            .and_then(|values| crate::run_commands::expand(&command, &values))
            .and_then(|command| crate::run_commands::launch_terminal(&command, std::path::Path::new(".")));
        match launched {
            Ok(child) => {
                self.run_state.launching = true;
                let executor = cx.background_executor().clone();
                let launched = cx.background_executor().spawn(async move {
                    crate::run_commands::reap(child, || executor.timer(std::time::Duration::from_millis(100))).await
                });
                cx.spawn(async move |owner, cx| {
                    let result = launched.await;
                    if let Err(error) = owner.update(cx, |view, cx| {
                        view.run_state.launching = false;
                        match result {
                            Ok(status) if status.success() => { view.add_run_history(command); view.dismiss_run(cx); }
                            result => {
                                view.run_state.history.retain(|entry| !entry.eq_ignore_ascii_case(&command));
                                view.run_message = match result {
                                    Ok(status) => format!("运行失败：Terminal 启动器退出（{status}）"),
                                    Err(error) => format!("运行失败：{error}"),
                                };
                            }
                        }
                        cx.notify();
                    }) { eprintln!("Run launcher lost its owner: {error}"); }
                }).detach();
                cx.notify();
            }
            Err(error) => {
                self.run_state.history.retain(|entry| !entry.eq_ignore_ascii_case(&command));
                self.run_message = format!("运行失败：{error}"); cx.notify();
            }
        }
    }

    fn browse_run(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.run_state.browse_task.is_some() || self.run_state.save_visible || self.run_state.launching { return; }
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("选择运行程序或脚本".into()) });
        self.run_state.browse_task = Some(cx.spawn(async move |owner, cx| {
            let selected = match prompt.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            if let Err(error) = owner.update(cx, |view, cx| {
                view.run_state.browse_task = None;
                match selected {
                    Ok(Some(path)) => {
                        let command = crate::run_commands::shell_literal(&path.to_string_lossy());
                        view.add_run_history(command.clone());
                        view.set_run_text(command, cx);
                    }
                    Ok(None) => {}
                    Err(error) => view.run_message = format!("浏览失败：{error}"),
                }
                cx.notify();
            }) { eprintln!("Run file picker lost its owner: {error}"); }
        }));
    }

    fn set_run_text(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(handle) = self.floating_windows.get(&floating_windows::DialogKind::Run).copied() else { return; };
        let field = self.run_input.clone();
        cx.defer(move |cx| {
            if let Err(error) = handle.update(cx, |_, window, cx| {
                field.update(cx, |field, cx| { field.set_text(&text, window, cx); field.focus(window, cx); });
                window.activate_window();
            }) { eprintln!("Run input window is unavailable: {error}"); }
        });
    }

    fn open_run_popup(&mut self, popup: Popup, window: &mut Window, cx: &mut Context<Self>) {
        if (self.run_state.save_visible && popup != Popup::Keys) || self.run_state.launching { return; }
        if self.run_state.popup == Some(popup) { self.dismiss_run_variables(cx); return; }
        let count = match popup { Popup::Variables => 11, Popup::History => self.run_state.history.len(), Popup::Keys => crate::shortcuts::key_choices().len() };
        if count == 0 { return; }
        let (offset, width) = match popup {
            Popup::Variables => (point(px(679.), px(43.75)), px(445.)),
            Popup::History => (point(px(24.5), px(68.25)), px(591.5)),
            Popup::Keys => (point(px(192.5), px(91.)), px(112.)),
        };
        let content_top = (window.bounds().size.height - window.viewport_size().height).max(px(0.));
        let mut bounds = Bounds::new(window.bounds().origin + point(px(0.), content_top) + offset, size(width, px((count as f32 * 22. + 6.).min(280.))));
        if let Some(display) = window.display(cx) {
            let area = display.visible_bounds();
            bounds.origin.x = bounds.origin.x.max(area.left()).min(area.right() - bounds.size.width);
            bounds.origin.y = bounds.origin.y.max(area.top()).min(area.bottom() - bounds.size.height);
        }
        self.run_state.popup_bounds = Some(bounds);
        self.run_state.popup = Some(popup);
        self.run_state.popup_selected = match popup {
            Popup::Variables => usize::MAX,
            Popup::History => self.run_state.history.iter().position(|entry| entry.eq_ignore_ascii_case(&self.run_input.read(cx).text())).unwrap_or(0),
            Popup::Keys => crate::shortcuts::key_choices().iter().position(|key| *key == self.run_state.save_combo.key).unwrap_or(0),
        };
        self.present_floating(floating_windows::DialogKind::RunVariables, window, cx);
        cx.notify();
    }

    pub(super) fn run_variables_visible(&self) -> bool { self.run_visible && self.run_state.popup.is_some() }
    pub(super) fn run_variables_bounds(&self) -> Option<Bounds<Pixels>> { self.run_state.popup_bounds }
    pub(super) fn run_variables_focus(&self) -> FocusHandle { self.run_state.popup_focus.clone() }
    pub(super) fn dismiss_run_variables(&mut self, cx: &mut Context<Self>) { self.run_state.popup = None; cx.notify(); }
    pub(super) fn close_run_variables(&mut self, cx: &mut Context<Self>) {
        let save = self.run_state.popup == Some(Popup::Keys);
        self.dismiss_run_variables(cx);
        self.focus_run_parent(save, cx);
    }

    fn focus_run_parent(&self, save: bool, cx: &mut Context<Self>) {
        let kind = if save { floating_windows::DialogKind::RunSave } else { floating_windows::DialogKind::Run };
        let Some(handle) = self.floating_windows.get(&kind).copied() else { return; };
        let field = if save { self.run_state.save_name.clone() } else { self.run_input.clone() };
        cx.defer(move |cx| {
            if let Err(error) = handle.update(cx, |_, window, cx| {
                window.activate_window();
                field.update(cx, |field, cx| field.focus(window, cx));
            }) { eprintln!("Run parent window is unavailable: {error}"); }
        });
    }

    fn choose_run_popup(&mut self, index: usize, cx: &mut Context<Self>) {
        let save = self.run_state.popup == Some(Popup::Keys);
        match self.run_state.popup {
            Some(Popup::Variables) => if let Some(name) = crate::run_commands::VARIABLE_NAMES.get(index) {
                self.set_run_text(format!("{}$({name})", self.run_input.read(cx).text()), cx);
            },
            Some(Popup::History) => if let Some(command) = self.run_state.history.get(index).cloned() { self.set_run_text(command, cx); },
            Some(Popup::Keys) => if let Some(key) = crate::shortcuts::key_choices().get(index) {
                self.run_state.save_combo.key = *key; self.run_state.save_warning = *key == 0;
            },
            None => {}
        }
        self.dismiss_run_variables(cx);
        self.focus_run_parent(save, cx);
    }

    pub(super) fn render_run_variables(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.run_state.popup?;
        let rows: Vec<(String, String)> = match popup {
            Popup::Variables => crate::run_commands::VARIABLE_NAMES.iter().zip(DESCRIPTIONS).map(|(name, description)| ((*name).into(), description.into())).collect(),
            Popup::History => self.run_state.history.iter().map(|command| (command.clone(), String::new())).collect(),
            Popup::Keys => crate::shortcuts::key_choices().into_iter().map(|key| (crate::shortcuts::key_name(key), String::new())).collect(),
        };
        let count = rows.len();
        Some(div().id("run-variables-popup").role(Role::Menu).track_focus(&self.run_state.popup_focus)
            .key_context("MatrixRunPopup").size_full().bg(rgb(0xf0f0f0)).border_1().border_color(rgb(0xadadad))
            .font_family(dialog_font::font_family()).text_size(px(13.)).py(px(3.)).occlude()
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => { let save = view.run_state.popup == Some(Popup::Keys); view.dismiss_run_variables(cx); view.focus_run_parent(save, cx); }
                    "enter" | "space" => if view.run_state.popup_selected < count { view.choose_run_popup(view.run_state.popup_selected, cx); },
                    "up" | "down" => {
                        view.run_state.popup_selected = if view.run_state.popup_selected >= count {
                            if event.keystroke.key == "down" { 0 } else { count - 1 }
                        } else { (view.run_state.popup_selected + if event.keystroke.key == "down" { 1 } else { count - 1 }) % count };
                        view.run_state.popup_scroll.scroll_to_item(view.run_state.popup_selected);
                        cx.notify();
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(div().id("run-popup-rows").size_full().overflow_y_scroll().track_scroll(&self.run_state.popup_scroll)
                .children(rows.into_iter().enumerate().map(|(index, (name, description))| {
                    div().id(("run-popup-row", index)).role(Role::MenuItem).aria_label(name.clone()).h(px(22.)).pl(px(32.)).pr(px(20.))
                        .flex().items_center().justify_between().gap(px(18.)).whitespace_nowrap()
                        .when(index == self.run_state.popup_selected, |row| row.bg(rgb(0x91c9f7)))
                        .hover(|row| row.bg(rgb(0x91c9f7)))
                        .child(name).when(!description.is_empty(), |row| row.child(description))
                        .on_click(cx.listener(move |view, _, _, cx| view.choose_run_popup(index, cx)))
                }))).into_any_element())
    }

    fn show_run_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run_state.save_visible || self.run_state.launching { return; }
        if self.run_input.read(cx).has_composition() { self.run_message = "请先完成运行命令中的输入法组合".into(); cx.notify(); return; }
        self.refresh_shortcuts(window, cx);
        self.run_state.save_command = self.run_input.read(cx).text();
        self.run_state.save_name.update(cx, |field, cx| field.set_text("", window, cx));
        self.run_state.save_combo = KeyCombo::default();
        self.run_state.save_warning = false;
        self.run_state.save_message.clear();
        self.run_state.save_visible = true;
        self.run_state.popup = None;
        if self.run_state.save_changed.is_none() {
            let field = self.run_state.save_name.clone();
            self.run_state.save_changed = Some(cx.observe(&field, |_, _, cx| cx.notify()));
        }
        self.present_floating(floating_windows::DialogKind::RunSave, window, cx);
        cx.notify();
    }

    pub(super) fn run_save_visible(&self) -> bool { self.run_visible && self.run_state.save_visible }
    pub(super) fn run_save_input(&self) -> gpui::Entity<text_field::TextField> { self.run_state.save_name.clone() }
    pub(super) fn dismiss_run_save(&mut self, cx: &mut Context<Self>) { self.run_state.save_visible = false; self.run_state.popup = None; self.focus_run_parent(false, cx); cx.notify(); }

    fn confirm_run_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.run_save_visible() || self.run_state.save_name.read(cx).has_composition() { return; }
        let name = self.run_state.save_name.read(cx).text();
        let combo = self.run_state.save_combo;
        if name.is_empty() || !combo.valid() { return; }
        self.refresh_shortcuts(window, cx);
        let Some(store) = self.settings_store.as_ref() else { self.run_state.save_message = "私有配置不可用".into(); cx.notify(); return; };
        let catalog = self.shortcut_mapper.bindings.shortcuts.iter().filter(|item| !matches!(item.key, CommandKey::Macro(_) | CommandKey::Run(_))).cloned().collect();
        let names = self.macro_dialogs.library.macros.iter().map(|saved| saved.name.clone()).collect::<Vec<_>>();
        let result = crate::shortcuts::BindingSet::load(store, catalog, &names).and_then(|mut bindings| {
            bindings.add_run(&name, &self.run_state.save_command, combo)?;
            bindings.save(store)?;
            Ok(bindings)
        });
        match result {
            Ok(bindings) => {
                self.shortcut_mapper.bindings = bindings;
                self.refresh_shortcuts(window, cx);
                self.dismiss_floating(floating_windows::DialogKind::RunSave, window, cx);
            }
            Err(error) => { self.run_state.save_message = format!("保存失败：{error}"); cx.notify(); }
        }
    }

    pub(super) fn render_run_save(&self, _window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.run_save_visible() { return None; }
        let combo = self.run_state.save_combo;
        let enabled = !self.run_state.save_name.read(cx).text().is_empty() && combo.valid();
        let conflict = !self.shortcut_mapper.bindings.conflicts(&CommandKey::Run(self.shortcut_mapper.bindings.run_commands.len()), &[combo]).is_empty();
        let check = |id: &'static str, label: &'static str, checked: bool| {
            div().id(id).role(Role::CheckBox).tab_index(0)
                .aria_label(label).aria_toggled(checked.into())
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key != "space" { return; }
                    match id {
                        "run-save-ctrl" => view.run_state.save_combo.ctrl ^= true,
                        "run-save-alt" => view.run_state.save_combo.alt ^= true,
                        "run-save-shift" => view.run_state.save_combo.shift ^= true,
                        "run-save-cmd" => view.run_state.save_combo.cmd ^= true,
                        _ => return,
                    }
                    cx.stop_propagation(); cx.notify();
                }))
                .flex().items_center().gap(px(4.)).child(if checked { "☑" } else { "☐" }).child(label)
        };
        Some(div().id("run-save-dialog").role(Role::Dialog).aria_label("快捷键")
            .key_context("MatrixRunSave").relative().size_full().bg(rgb(0xf0f0f0))
            .font_family(dialog_font::font_family()).text_size(px(13.))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => view.cancel_run(window, cx),
                    "enter" => view.confirm_run_save(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(div().absolute().left(px(17.5)).top(px(14.)).w(px(75.)).text_align(gpui::TextAlign::Right).child("名称："))
            .child(div().absolute().left(px(98.)).top(px(14.)).w(px(206.5)).child(self.run_state.save_name.clone()))
            .child(div().absolute().left(px(21.)).top(px(47.25)).child(check("run-save-ctrl", "Ctrl", combo.ctrl).on_click(cx.listener(|view, _, _, cx| { view.run_state.save_combo.ctrl ^= true; cx.notify(); }))))
            .child(div().absolute().left(px(21.)).top(px(84.)).child(check("run-save-alt", "Alt", combo.alt).on_click(cx.listener(|view, _, _, cx| { view.run_state.save_combo.alt ^= true; cx.notify(); }))))
            .child(div().absolute().left(px(99.75)).top(px(68.25)).child(check("run-save-shift", "Shift", combo.shift).on_click(cx.listener(|view, _, _, cx| { view.run_state.save_combo.shift ^= true; cx.notify(); }))))
            .child(div().absolute().left(px(21.)).top(px(112.)).child(check("run-save-cmd", "Cmd", combo.cmd).on_click(cx.listener(|view, _, _, cx| { view.run_state.save_combo.cmd ^= true; cx.notify(); }))))
            .child(div().id("run-save-key").role(Role::ComboBox).tab_index(0).aria_label("快捷键").aria_value(crate::shortcuts::key_name(combo.key)).absolute().left(px(192.5)).top(px(66.5)).w(px(112.)).h(px(24.)).border_1().border_color(rgb(0x808080)).bg(rgb(0xffffff)).px(px(4.)).flex().justify_between().items_center()
                .child(crate::shortcuts::key_name(combo.key)).child("▾")
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "space" => view.open_run_popup(Popup::Keys, window, cx),
                        "up" | "down" => {
                            let keys = crate::shortcuts::key_choices();
                            let index = keys.iter().position(|key| *key == view.run_state.save_combo.key).unwrap_or(0);
                            let delta = if event.keystroke.key == "up" { -1 } else { 1 };
                            view.run_state.save_combo.key = keys[index.saturating_add_signed(delta).min(keys.len() - 1)];
                            view.run_state.save_warning = view.run_state.save_combo.key == 0;
                            cx.notify();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .on_click(cx.listener(|view, _, window, cx| view.open_run_popup(Popup::Keys, window, cx))))
            .child(div().absolute().left(px(68.25)).top(px(140.)).flex().gap(px(10.5))
                .child(div().id("run-save-ok").role(Role::Button).aria_label("确定").a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).tab_index(0).w(px(87.5)).h(px(24.5)).border_1().border_color(rgb(0x808080)).flex().items_center().justify_center().text_color(rgb(if enabled { 0x202020 } else { 0x808080 })).child("确定")
                    .on_click(cx.listener(|view, _, window, cx| view.confirm_run_save(window, cx))))
                .child(div().id("run-save-cancel").role(Role::Button).aria_label("取消").tab_index(0).w(px(87.5)).h(px(24.5)).border_1().border_color(rgb(0x808080)).flex().items_center().justify_center().child("取消")
                    .on_click(cx.listener(|view, _, window, cx| view.cancel_run(window, cx)))))
            .when(conflict || self.run_state.save_warning || !self.run_state.save_message.is_empty(), |dialog| dialog.child(
                div().absolute().left(px(10.5)).top(px(169.)).text_color(rgb(0xa00000)).child(
                    if !self.run_state.save_message.is_empty() { self.run_state.save_message.clone() }
                    else if conflict { "发现快捷键冲突！".into() } else { "这将禁用快捷键".into() })))
            .into_any_element())
    }

    pub(super) fn render_run(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.run_visible { return None; }
        let button = |id: &'static str, text: &'static str, focus: &FocusHandle, x: f32, y: f32, width: f32| {
            div().id(id).debug_selector(move || id.into()).role(Role::Button).aria_label(text)
                .track_focus(focus).key_context("MatrixRunButton").absolute().left(px(x * 1.75)).top(px(y * 1.75))
                .w(px(width * 1.75)).h(px(24.5)).border_1().border_color(rgb(if focus.is_focused(window) { 0x0067c0 } else { 0xaaaaaa }))
                .bg(rgb(0xe1e1e1)).flex().items_center().justify_center().child(text)
        };
        Some(div().id("run-dialog").debug_selector(|| "run-dialog".into()).role(Role::Dialog).aria_label("运行...")
            .key_context("MatrixRun").relative().w_full().h(px(140.)).bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 }))
            .font_family(dialog_font::font_family()).text_size(px(13.)).occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "down" && !view.run_state.save_visible { view.open_run_popup(Popup::History, window, cx); cx.stop_propagation(); }
            }))
            .child(div().absolute().left(px(12.25)).top(px(10.5)).w(px(679.)).h(px(84.)).border_1().border_color(rgb(0xd3d3d3)))
            .child(div().absolute().left(px(300.)).top(px(2.)).px(px(3.)).bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 })).child("输入运行程序名"))
            .child(div().absolute().left(px(24.5)).top(px(45.5)).w(px(591.5)).h(px(22.)).child(self.run_input.clone()))
            .child(div().id("run-history").role(Role::Button).aria_label("运行历史").absolute().left(px(596.)).top(px(45.5)).w(px(20.)).h(px(22.)).flex().items_center().justify_center().child("⌄")
                .on_click(cx.listener(|view, _, window, cx| view.open_run_popup(Popup::History, window, cx))))
            .when(self.run_message.is_empty(), |dialog| dialog.child(div().absolute().left(px(24.5)).top(px(72.)).text_size(px(10.)).child("POSIX 命令 · 在 macOS Terminal 中运行")))
            .child(button("run-browse", "...", &self.run_state.controls[0], 356., 25., 14.)
                .on_click(cx.listener(|view, _, window, cx| view.browse_run(window, cx))))
            .child(button("run-variables", "+", &self.run_state.controls[1], 372., 25., 14.)
                .on_click(cx.listener(|view, _, window, cx| view.open_run_popup(Popup::Variables, window, cx))))
            .child(button("run-confirm", "运行", &self.run_button_focus[0], 124., 60., 50.)
                .on_click(cx.listener(|view, _, window, cx| view.confirm_run(window, cx))))
            .child(button("run-save", "保存...", &self.run_state.controls[2], 177., 60., 50.)
                .on_click(cx.listener(|view, _, window, cx| view.show_run_save(window, cx))))
            .child(button("run-cancel", "取消", &self.run_button_focus[1], 230., 60., 50.)
                .on_click(cx.listener(|view, _, window, cx| view.cancel_run(window, cx))))
            .when(!self.run_message.is_empty(), |dialog| dialog.child(div().id("run-error").role(Role::Alert).absolute().left(px(16.)).top(px(73.)).w(px(660.)).text_color(rgb(0xb00000)).text_size(px(11.)).child(self.run_message.clone())))
            .into_any_element())
    }
}
