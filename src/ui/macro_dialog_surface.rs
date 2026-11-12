use super::*;
use crate::macros::{MacroLibrary, MacroSaveTask, MacroStep, RepeatMode, RepeatTracker};
use std::sync::Arc;

pub(super) struct ActiveMacroRun {
    pub(super) identity: std::sync::Arc<()>,
    pub(super) steps: Vec<MacroStep>,
    pub(super) next_step: usize,
    pub(super) tracker: RepeatTracker,
    pub(super) buffer: crate::workspace::BufferId,
}

pub(super) struct MacroDialogs {
    pub(super) save_visible: bool,
    pub(super) repeat_visible: bool,
    pub(super) name_input: gpui::Entity<text_field::TextField>,
    pub(super) count_input: gpui::Entity<text_field::TextField>,
    pub(super) library: MacroLibrary,
    pub(super) current_saved: bool,
    pub(super) executing_step: bool,
    pub(super) active_run: Option<ActiveMacroRun>,
    save_steps: Vec<MacroStep>,
    pub(super) recording_identity: Arc<()>,
    save_recording_identity: Arc<()>,
    save_dialog_identity: Arc<()>,
    pending_save: Option<Arc<()>>,
    save_task: Option<gpui::Task<()>>,
    selected_saved: Option<usize>,
    until_end: bool,
    save_message: String,
    repeat_message: String,
}

impl MacroDialogs {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            save_visible: false,
            repeat_visible: false,
            name_input: cx
                .new(|cx| text_field::TextField::new("Macro name", "macro-name-input", cx)),
            count_input: cx
                .new(|cx| text_field::TextField::new("Repeat count", "macro-count-input", cx)),
            library: MacroLibrary::default(),
            current_saved: false,
            executing_step: false,
            active_run: None,
            save_steps: Vec::new(),
            recording_identity: Arc::new(()),
            save_recording_identity: Arc::new(()),
            save_dialog_identity: Arc::new(()),
            pending_save: None,
            save_task: None,
            selected_saved: None,
            until_end: false,
            save_message: String::new(),
            repeat_message: String::new(),
        }
    }
}

impl MatrixWindow {
    pub(super) fn load_saved_macros(&mut self, cx: &mut Context<Self>) {
        let Some(store) = &self.settings_store else {
            return;
        };
        match MacroLibrary::load(store) {
            Ok(library) => self.macro_dialogs.library = library,
            Err(error) => {
                self.macro_dialogs.library = MacroLibrary::default();
                self.macro_dialogs.repeat_message = format!("Could not load saved macros: {error}");
            }
        }
        self.menu_bar.set_saved_macros(
            self.macro_dialogs
                .library
                .macros
                .iter()
                .map(|saved| saved.name.clone())
                .collect(),
        );
        cx.notify();
    }

    pub(super) fn current_repeat_macro_available(&self) -> bool {
        !self.macro_recording && !self.macro_steps.is_empty() && !self.macro_dialogs.current_saved
    }

    pub(super) fn show_save_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_recording
            || self.macro_playing
            || self.macro_dialogs.current_saved
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if let Err(error) = MacroLibrary::validate_steps(&self.macro_steps) {
            self.notify_editor_error(error.to_string(), cx);
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if !self.macro_dialogs.save_visible {
            self.macro_dialogs.save_steps = self.macro_steps.clone();
            self.macro_dialogs.save_recording_identity =
                self.macro_dialogs.recording_identity.clone();
            self.macro_dialogs.save_dialog_identity = Arc::new(());
            self.macro_dialogs.pending_save = None;
            self.macro_dialogs.save_message.clear();
            self.macro_dialogs
                .name_input
                .update(cx, |field, cx| field.set_text("", window, cx));
        }
        self.macro_dialogs.save_visible = true;
        self.present_floating(floating_windows::DialogKind::MacroSave, window, cx);
        cx.notify();
    }

    pub(super) fn save_current_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.save_current_macro(window, cx)
        }) {
            return;
        }
        if !self.macro_dialogs.save_visible
            || self.macro_dialogs.pending_save.is_some()
            || self.macro_recording
            || self.macro_playing
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.macro_dialogs.name_input.read(cx).has_composition() {
            self.macro_dialogs.save_message =
                "Commit the macro name composition before saving".into();
            cx.notify();
            return;
        }
        let name = self.macro_dialogs.name_input.read(cx).text();
        let Some(store_root) = self
            .settings_store
            .as_ref()
            .map(|store| store.root().to_path_buf())
        else {
            self.macro_dialogs.save_message =
                "The private configuration store is unavailable".into();
            cx.notify();
            return;
        };
        let Some(home) = store_root.parent().map(PathBuf::from) else {
            self.macro_dialogs.save_message =
                "The private configuration root has no home directory".into();
            cx.notify();
            return;
        };
        let steps = self.macro_dialogs.save_steps.clone();
        let request = Arc::new(());
        let dialog = self.macro_dialogs.save_dialog_identity.clone();
        let recording = self.macro_dialogs.save_recording_identity.clone();
        self.macro_dialogs.pending_save = Some(request.clone());
        self.macro_dialogs.save_message = "Saving…".into();
        let previous = self.macro_dialogs.save_task.take();
        self.macro_dialogs.save_task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = MacroSaveTask::after_publication(
                async move {
                    if let Some(previous) = previous {
                        previous.await;
                    }
                },
                home,
                name,
                steps.clone(),
            )
            .await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                let same_store = view
                    .settings_store
                    .as_ref()
                    .is_some_and(|store| store.root() == store_root);
                let owns_dialog = view.macro_dialogs.save_visible
                    && Arc::ptr_eq(&dialog, &view.macro_dialogs.save_dialog_identity)
                    && view
                        .macro_dialogs
                        .pending_save
                        .as_ref()
                        .is_some_and(|pending| Arc::ptr_eq(pending, &request));
                if owns_dialog {
                    view.macro_dialogs.pending_save = None;
                }
                match result {
                    Ok(library) if same_store => {
                        view.macro_dialogs.library = library;
                        view.menu_bar.set_saved_macros(
                            view.macro_dialogs
                                .library
                                .macros
                                .iter()
                                .map(|saved| saved.name.clone())
                                .collect(),
                        );
                        if Arc::ptr_eq(&recording, &view.macro_dialogs.recording_identity)
                            && view.macro_steps == steps
                        {
                            view.macro_dialogs.current_saved = true;
                        }
                        if owns_dialog {
                            view.dismiss_floating(
                                floating_windows::DialogKind::MacroSave,
                                window,
                                cx,
                            );
                        }
                    }
                    Ok(_) => {
                        if owns_dialog {
                            view.macro_dialogs.save_message =
                                "Macro was saved to its original configuration store".into();
                        }
                    }
                    Err(error) => {
                        if owns_dialog {
                            view.macro_dialogs.save_message =
                                format!("Could not save macro: {error}");
                        } else {
                            eprintln!("Closed macro save failed: {error}");
                        }
                    }
                }
                cx.notify();
            }) {
                eprintln!("Macro save completion could not reach its document: {error}");
            }
        }));
        cx.notify();
    }

    pub(super) fn show_repeat_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.macro_recording
            || self.macro_playing
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.macro_dialogs.repeat_message.clear();
        self.load_saved_macros(cx);
        let selection_valid = match self.macro_dialogs.selected_saved {
            Some(index) => index < self.macro_dialogs.library.macros.len(),
            None => self.current_repeat_macro_available(),
        };
        if !selection_valid {
            self.macro_dialogs.selected_saved = if self.current_repeat_macro_available() {
                None
            } else {
                (!self.macro_dialogs.library.macros.is_empty()).then_some(0)
            };
        }
        if self.macro_dialogs.count_input.read(cx).text().is_empty() {
            self.macro_dialogs
                .count_input
                .update(cx, |field, cx| field.set_text("1", window, cx));
        }
        self.macro_dialogs.repeat_visible = true;
        self.present_floating(floating_windows::DialogKind::MacroRepeat, window, cx);
        cx.notify();
    }

    pub(super) fn confirm_macro_repeat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.confirm_macro_repeat(window, cx)
        }) {
            return;
        }
        if !self.macro_dialogs.repeat_visible || self.macro_recording || self.macro_playing {
            return;
        }
        let steps = match self.macro_dialogs.selected_saved {
            Some(index) => self
                .macro_dialogs
                .library
                .macros
                .get(index)
                .map(|saved| saved.steps.clone()),
            None if self.current_repeat_macro_available() => Some(self.macro_steps.clone()),
            None => None,
        };
        let Some(steps) = steps else {
            self.macro_dialogs.repeat_message = "Select a recorded or saved macro".into();
            cx.notify();
            return;
        };
        let mode = if self.macro_dialogs.until_end {
            RepeatMode::UntilEnd
        } else {
            if self.macro_dialogs.count_input.read(cx).has_composition() {
                self.macro_dialogs.repeat_message =
                    "Commit the repeat count composition first".into();
                cx.notify();
                return;
            }
            match RepeatMode::from_count_input(&self.macro_dialogs.count_input.read(cx).text()) {
                Ok(mode) => mode,
                Err(error) => {
                    self.macro_dialogs.repeat_message = error.to_string();
                    cx.notify();
                    return;
                }
            }
        };
        self.start_macro_repeat(steps, mode, window, cx);
    }

    pub(super) fn render_save_macro(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.macro_dialogs.save_visible {
            return None;
        }
        Some(
            div()
                .id("save-macro-dialog")
                .debug_selector(|| "save-macro-dialog".into())
                .role(Role::Dialog)
                .aria_label("Save Current Recorded Macro")
                .key_context("MatrixMacroSave")
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" => view.save_current_macro(window, cx),
                        "escape" => view.dismiss_floating(
                            floating_windows::DialogKind::MacroSave,
                            window,
                            cx,
                        ),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .w_full()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child("Name")
                .child(self.macro_dialogs.name_input.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            macro_button("macro-save-confirm", "Save")
                                .when(self.macro_dialogs.pending_save.is_some(), |button| {
                                    button.opacity(0.5)
                                })
                                .a11y_synthetic_children({
                                    let pending = self.macro_dialogs.pending_save.is_some();
                                    move |builder| {
                                        if pending {
                                            builder.parent_node().set_disabled();
                                        }
                                    }
                                })
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.save_current_macro(window, cx)
                                })),
                        )
                        .child(
                            macro_button("macro-save-cancel", "Cancel").on_click(cx.listener(
                                |view, _, window, cx| {
                                    view.dismiss_floating(
                                        floating_windows::DialogKind::MacroSave,
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        ),
                )
                .when(!self.macro_dialogs.save_message.is_empty(), |dialog| {
                    dialog.child(self.macro_dialogs.save_message.clone())
                })
                .into_any_element(),
        )
    }

    pub(super) fn render_repeat_macro(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.macro_dialogs.repeat_visible {
            return None;
        }
        let mut choices = Vec::new();
        if self.current_repeat_macro_available() {
            choices.push(
                macro_choice(
                    "macro-current-choice".into(),
                    "Current recorded macro".into(),
                    self.macro_dialogs.selected_saved.is_none(),
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    if !view.macro_playing {
                        view.macro_dialogs.selected_saved = None;
                        cx.notify();
                    }
                })),
            );
        }
        for (index, saved) in self.macro_dialogs.library.macros.iter().enumerate() {
            choices.push(
                macro_choice(
                    format!("macro-saved-choice-{index}"),
                    saved.name.clone(),
                    self.macro_dialogs.selected_saved == Some(index),
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    if !view.macro_playing {
                        view.macro_dialogs.selected_saved = Some(index);
                        cx.notify();
                    }
                })),
            );
        }
        Some(
            div()
                .id("repeat-macro-dialog")
                .debug_selector(|| "repeat-macro-dialog".into())
                .role(Role::Dialog)
                .aria_label("Run a Macro Multiple Times")
                .key_context("MatrixMacroRepeat")
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" => view.confirm_macro_repeat(window, cx),
                        "escape" => view.close_repeat_macro(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .w_full()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child("Macro to run")
                .child(
                    div()
                        .id("macro-choices")
                        .max_h(px(120.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .children(choices),
                )
                .child(
                    macro_choice(
                        "macro-repeat-count-mode".into(),
                        "Run a specified number of times".into(),
                        !self.macro_dialogs.until_end,
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        if !view.macro_playing {
                            view.macro_dialogs.until_end = false;
                            cx.notify();
                        }
                    })),
                )
                .when(!self.macro_dialogs.until_end, |dialog| {
                    dialog.child(self.macro_dialogs.count_input.clone())
                })
                .child(
                    macro_choice(
                        "macro-repeat-eof-mode".into(),
                        "Run until the end of file".into(),
                        self.macro_dialogs.until_end,
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        if !view.macro_playing {
                            view.macro_dialogs.until_end = true;
                            cx.notify();
                        }
                    })),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .when(!self.macro_playing, |row| {
                            row.child(macro_button("macro-repeat-run", "Run").on_click(
                                cx.listener(|view, _, window, cx| {
                                    view.confirm_macro_repeat(window, cx)
                                }),
                            ))
                        })
                        .when(self.macro_playing, |row| {
                            row.child(macro_button("macro-repeat-stop", "Stop").on_click(
                                cx.listener(|view, _, window, cx| {
                                    view.cancel_macro_repeat(window, cx)
                                }),
                            ))
                        })
                        .child(macro_button("macro-repeat-close", "Close").on_click(
                            cx.listener(|view, _, window, cx| view.close_repeat_macro(window, cx)),
                        )),
                )
                .when(!self.macro_dialogs.repeat_message.is_empty(), |dialog| {
                    dialog.child(self.macro_dialogs.repeat_message.clone())
                })
                .into_any_element(),
        )
    }

    pub(super) fn close_repeat_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_floating(floating_windows::DialogKind::MacroRepeat, window, cx);
    }

    pub(super) fn set_macro_repeat_message(&mut self, message: String) {
        self.macro_dialogs.repeat_message = message;
    }
}

fn macro_button(id: &'static str, text: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .role(Role::Button)
        .aria_label(text)
        .px(px(12.))
        .py(px(4.))
        .border_1()
        .border_color(rgb(0x808080))
        .child(text)
}

fn macro_choice(id: String, text: String, selected: bool) -> gpui::Stateful<gpui::Div> {
    let selector = id.clone();
    div()
        .id(gpui::SharedString::from(id))
        .debug_selector(move || selector.clone())
        .role(Role::RadioButton)
        .aria_label(text.clone())
        .aria_toggled(selected.into())
        .px(px(4.))
        .py(px(3.))
        .when(selected, |choice| choice.bg(rgb(0xcce8ff)))
        .child(text)
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;

    fn click_control(
        cx: &mut gpui::TestAppContext,
        window: gpui::AnyWindowHandle,
        selector: &'static str,
    ) {
        let mut visual = gpui::VisualTestContext::from_window(window, cx);
        let bounds = visual
            .debug_bounds(selector)
            .expect("visible macro control");
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
    }

    fn deliver_document_frame(cx: &mut gpui::TestAppContext, document: gpui::AnyWindowHandle) {
        let callbacks = cx
            .update_window(document, |_, window, cx| window.simulate_next_frame(cx))
            .expect("deliver document frame");
        assert!(
            callbacks > 0,
            "playback must have requested a document frame"
        );
        cx.run_until_parked();
    }

    fn open_repeat_from_toolbar(
        cx: &mut gpui::TestAppContext,
        document: gpui::AnyWindowHandle,
    ) -> gpui::AnyWindowHandle {
        click_control(cx, document, "toolbar-macro-repeat");
        cx.windows()
            .into_iter()
            .find(|window| *window != document)
            .expect("independent repeat macro window")
    }

    fn record_a_from_toolbar(cx: &mut gpui::TestAppContext, document: gpui::AnyWindowHandle) {
        click_control(cx, document, "toolbar-macro-start");
        cx.simulate_input(document, "A");
        click_control(cx, document, "toolbar-macro-stop");
        click_control(cx, document, "toolbar-undo");
    }

    fn finish_save_task(cx: &mut gpui::TestAppContext, task: gpui::Task<()>) {
        cx.executor().allow_parking();
        cx.foreground_executor().clone().block_test(task);
        cx.executor().forbid_parking();
        cx.run_until_parked();
    }

    #[gpui::test]
    fn save_macro_has_an_independent_window_and_enter_persists_named_recording(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated macro HOME");
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        main.update(cx, |view, _, _| {
            view.settings_store = Some(ConfigStore::under_home(home.path()));
        })
        .expect("isolated private configuration store");
        record_a_from_toolbar(cx, main.into());
        click_control(cx, main.into(), "toolbar-macro-save");
        cx.run_until_parked();
        let dialog = cx
            .windows()
            .into_iter()
            .find(|handle| *handle != main.into())
            .expect("independent save macro window");
        cx.simulate_input(dialog, "Insert A");
        cx.simulate_keystrokes(dialog, "enter");
        let task = main
            .update(cx, |view, _, _| view.macro_dialogs.save_task.take())
            .expect("save task owner")
            .expect("submitted macro save");
        finish_save_task(cx, task);
        assert_eq!(cx.windows(), vec![main.into()]);
        let library =
            MacroLibrary::load(&ConfigStore::under_home(home.path())).expect("saved private JSON");
        assert_eq!(library.macros.len(), 1);
        assert_eq!(library.macros[0].name, "Insert A");
        assert_eq!(
            library.macros[0].steps,
            vec![MacroStep::ReplaceSelection("A".into())]
        );
    }

    #[gpui::test]
    fn repeated_macro_can_cancel_before_queued_frame_without_stale_replay(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        main.update(cx, |view, window, cx| {
            view.macro_dialogs.repeat_visible = true;
            view.start_macro_repeat(
                vec![MacroStep::ReplaceSelection("must not run".into())],
                RepeatMode::Count(9999),
                window,
                cx,
            );
            view.cancel_macro_repeat(window, cx);
            view.start_macro_repeat(
                vec![MacroStep::ReplaceSelection("A".into())],
                RepeatMode::Count(3),
                window,
                cx,
            );
        })
        .expect("queue, cancel, queue fresh repeat");
        cx.run_until_parked();
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert!(
                view.macro_playing,
                "executor parking does not deliver frames"
            );
        })
        .expect("fresh repeat waits for its requested frame");
        deliver_document_frame(cx, main.into());
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "AAA");
            assert!(!view.macro_playing);
        })
        .expect("only fresh playback completes");
        cx.dispatch_action(main.into(), EditorUndo);
        main.read_with(cx, |view, _| assert_eq!(view.editor().committed_text(), ""))
            .expect("all repeats share one Undo action");
    }

    #[gpui::test]
    fn toolbar_recording_count_dialog_replays_across_frames_with_one_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        record_a_from_toolbar(cx, main.into());
        let dialog = open_repeat_from_toolbar(cx, main.into());
        cx.simulate_input(dialog, "70");
        click_control(cx, dialog, "macro-repeat-run");
        for expected_length in [32, 64, 70] {
            deliver_document_frame(cx, main.into());
            main.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "A".repeat(expected_length));
                assert_eq!(view.macro_playing, expected_length < 70);
            })
            .expect("count dialog advances one bounded batch per document frame");
        }
        assert!(
            cx.windows().contains(&dialog),
            "Run leaves the source modeless dialog open"
        );
        click_control(cx, main.into(), "toolbar-undo");
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.workspace.active().is_modified());
        })
        .expect("one Undo restores the savepoint before all seventy repetitions");
        cx.dispatch_action(dialog, CloseWindow);
        cx.run_until_parked();
        assert_eq!(cx.windows(), vec![main.into()]);
    }

    #[gpui::test]
    fn recorded_delete_line_eof_dialog_finishes_and_undo_restores_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(main.into(), "one\ntwo\nthree");
        cx.dispatch_action(main.into(), EditorDocumentStart);
        click_control(cx, main.into(), "toolbar-macro-start");
        cx.dispatch_action(main.into(), EditorLineDelete);
        click_control(cx, main.into(), "toolbar-macro-stop");
        click_control(cx, main.into(), "toolbar-undo");
        let dialog = open_repeat_from_toolbar(cx, main.into());
        click_control(cx, dialog, "macro-repeat-eof-mode");
        click_control(cx, dialog, "macro-repeat-run");
        deliver_document_frame(cx, main.into());
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert!(
                !view.macro_playing,
                "line deletion at the same caret must terminate at EOF"
            );
        })
        .expect("source EOF mode deletes every line and stops");
        click_control(cx, main.into(), "toolbar-undo");
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one\ntwo\nthree")
        })
        .expect("one Undo restores all lines deleted by the repeat dialog");
    }

    #[gpui::test]
    fn repeat_dialog_stop_between_batches_cancels_the_queued_document_frame(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        record_a_from_toolbar(cx, main.into());
        let dialog = open_repeat_from_toolbar(cx, main.into());
        cx.simulate_input(dialog, "9999");
        click_control(cx, dialog, "macro-repeat-run");
        deliver_document_frame(cx, main.into());
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "A".repeat(32));
            assert!(view.macro_playing);
        })
        .expect("one batch returns control before the requested count finishes");
        click_control(cx, dialog, "macro-repeat-stop");
        deliver_document_frame(cx, main.into());
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "A".repeat(32));
            assert!(!view.macro_playing);
        })
        .expect("Stop prevents every operation in the queued next batch");
        click_control(cx, main.into(), "toolbar-undo");
        main.read_with(cx, |view, _| assert_eq!(view.editor().committed_text(), ""))
            .expect("cancelled partial playback still has one closed Undo action");
    }

    #[gpui::test]
    fn readonly_recorded_insert_then_down_has_single_and_repeat_navigation_parity(
        cx: &mut gpui::TestAppContext,
    ) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(main.into(), "one\ntwo\nthree");
        cx.dispatch_action(main.into(), EditorDocumentStart);
        click_control(cx, main.into(), "toolbar-macro-start");
        cx.simulate_input(main.into(), "x");
        cx.dispatch_action(main.into(), EditorDown);
        click_control(cx, main.into(), "toolbar-macro-stop");
        click_control(cx, main.into(), "toolbar-undo");
        main.update(cx, |view, _, _| view.editor_mut().set_read_only(true))
            .expect("read-only document fixture");
        click_control(cx, main.into(), "toolbar-macro-playback");
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one\ntwo\nthree");
            assert_eq!(view.editor().selection().head, ByteOffset(4));
            assert!(
                view.editor_error.is_none(),
                "successful navigation reports its normal result"
            );
        })
        .expect("single playback skips the rejected edit and still navigates");
        cx.dispatch_action(main.into(), EditorDocumentStart);
        let dialog = open_repeat_from_toolbar(cx, main.into());
        cx.simulate_input(dialog, "2");
        click_control(cx, dialog, "macro-repeat-run");
        deliver_document_frame(cx, main.into());
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "one\ntwo\nthree");
            assert_eq!(view.editor().selection().head, ByteOffset(8));
            assert!(!view.macro_playing);
            assert!(
                view.editor_error.is_none(),
                "repeat uses the same step results as single playback"
            );
        })
        .expect("repeat completes both iterations despite each rejected read-only insertion");
    }

    #[gpui::test]
    fn contended_save_keeps_cancel_responsive_and_cannot_acknowledge_a_new_recording(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated contended macro store");
        ConfigStore::under_home(home.path())
            .load_settings()
            .expect("fixture settings");
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        main.update(cx, |view, _, _| {
            view.settings_store = Some(ConfigStore::under_home(home.path()))
        })
        .expect("isolated configuration fixture");
        record_a_from_toolbar(cx, main.into());
        let lock_home = home.path().to_path_buf();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let locker = std::thread::spawn(move || {
            ConfigStore::under_home(&lock_home).update_json(
                std::path::Path::new("macros.json"),
                &serde_json::json!({"schema_version":1,"macros":[]}),
                |_| {
                    locked_tx.send(()).map_err(io::Error::other)?;
                    release_rx.recv().map_err(io::Error::other)?;
                    Ok(())
                },
            )
        });
        locked_rx
            .recv()
            .expect("another instance holds the configuration lock");
        click_control(cx, main.into(), "toolbar-macro-save");
        let original = cx
            .windows()
            .into_iter()
            .find(|window| *window != main.into())
            .expect("first Save dialog");
        cx.simulate_input(original, "captured name");
        click_control(cx, original, "macro-save-confirm");
        let task = main
            .update(cx, |view, _, _| view.macro_dialogs.save_task.take())
            .expect("pending task owner")
            .expect("off-thread save started while lock is held");
        click_control(cx, original, "macro-save-cancel");
        assert_eq!(
            cx.windows(),
            vec![main.into()],
            "Cancel must work before releasing the disk lock"
        );
        record_a_from_toolbar(cx, main.into());
        click_control(cx, main.into(), "toolbar-macro-save");
        let replacement = cx
            .windows()
            .into_iter()
            .find(|window| *window != main.into())
            .expect("replacement Save dialog");
        cx.simulate_input(replacement, "new dialog draft");
        release_tx.send(()).expect("release contending instance");
        locker
            .join()
            .expect("configuration locker thread")
            .expect("locker writes its original document");
        finish_save_task(cx, task);
        assert!(cx.windows().contains(&replacement));
        main.read_with(cx, |view, cx| {
            assert!(!view.macro_dialogs.current_saved);
            assert_eq!(
                view.macro_dialogs.name_input.read(cx).text(),
                "new dialog draft"
            );
            assert_eq!(view.editor().committed_text(), "");
        })
        .expect("old completion neither closes nor modifies the replacement dialog/recording");
        let library = MacroLibrary::load(&ConfigStore::under_home(home.path()))
            .expect("completed original write");
        assert_eq!(library.macros.len(), 1);
        assert_eq!(library.macros[0].name, "captured name");
        assert_eq!(
            library.macros[0].steps,
            vec![MacroStep::ReplaceSelection("A".into())]
        );
    }

    #[gpui::test]
    fn saved_macro_menu_runs_once_without_replacing_the_current_recording(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated named macro store");
        MacroLibrary::save_current(
            &ConfigStore::under_home(home.path()),
            "Named",
            &[
                MacroStep::ReplaceSelection("saved".into()),
                MacroStep::ReplaceSelection("!".into()),
            ],
        )
        .expect("saved macro fixture");
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        main.update(cx, |view, _, cx| {
            view.settings_store = Some(ConfigStore::under_home(home.path()));
            view.load_saved_macros(cx);
        })
        .expect("load named macros as application startup does");
        record_a_from_toolbar(cx, main.into());
        cx.dispatch_action(main.into(), OpenMacroMenu);
        click_control(cx, main.into(), "macro-saved-0");
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "saved!")
        })
        .expect("dynamic named menu executes every saved step once");
        click_control(cx, main.into(), "toolbar-undo");
        main.read_with(cx, |view, _| assert_eq!(view.editor().committed_text(), ""))
            .expect("one Undo covers both saved steps");
        click_control(cx, main.into(), "toolbar-macro-playback");
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "A")
        })
        .expect("named playback leaves the current recorded macro intact");
    }
}
