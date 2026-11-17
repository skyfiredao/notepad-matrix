use super::macro_dialog_surface::ActiveMacroRun;
use super::*;
use crate::macros::{MacroCommand, MacroLibrary, MacroStep, RepeatMode, RepeatTracker};

impl MatrixWindow {
    pub(super) fn initialize_scintilla_shortcut_integration(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.set_scintilla_macro_recorder(Self::record_scintilla_macro_step);
        self.reset_scintilla_shortcut_target();
    }

    fn record_scintilla_macro_step(&mut self, message: u32) {
        if MacroStep::scintilla_recordable(message) {
            self.record_macro_step(MacroStep::Scintilla { message, wparam: 0, lparam: 0 });
        }
    }

    pub(super) fn macro_interaction_blocked(&self) -> bool {
        self.macro_playing && !self.macro_dialogs.executing_step
    }

    pub(super) fn record_macro_step(&mut self, step: MacroStep) {
        if self.macro_recording && !self.macro_playing {
            self.macro_steps.push(step);
            self.macro_step_pending = true;
        }
    }

    pub(super) fn record_macro_text(&mut self, text: &str) {
        if !self.macro_recording || self.macro_playing {
            return;
        }
        if text == "\r" || text == "\n" {
            // NppNotification.cpp:538–573 collapses a CRLF notification pair.
            if text == "\n"
                && self.editor().line_ending() == crate::input::LineEnding::CrLf
                && matches!(self.macro_steps.last(), Some(MacroStep::Newline))
            {
                self.macro_steps.pop();
            }
            self.record_macro_step(MacroStep::Newline);
        } else {
            self.record_macro_step(MacroStep::ReplaceSelection(
                text.split('\0').next().unwrap_or_default().into(),
            ));
        }
    }

    pub(super) fn macro_command(
        &mut self,
        command: MacroCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.macro_command(command, window, cx)
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || self.macro_playing
        {
            return;
        }
        if self.editor().has_composition() {
            self.notify_editor_error(
                "Macro control during active composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        match command {
            MacroCommand::StartRecording if !self.macro_recording => {
                self.macro_steps.clear();
                self.macro_dialogs.recording_identity = std::sync::Arc::new(());
                self.macro_dialogs.current_saved = false;
                self.macro_recording = true;
                self.macro_step_pending = false;
            }
            MacroCommand::StopRecording if self.macro_recording => self.macro_recording = false,
            MacroCommand::SaveCurrent => {
                self.show_save_macro(window, cx);
                return;
            }
            MacroCommand::RunMultiple => {
                self.show_repeat_macro(window, cx);
                return;
            }
            MacroCommand::Playback if !self.macro_recording && !self.macro_steps.is_empty() => {
                self.play_macro_once(self.macro_steps.clone(), window, cx);
            }
            _ => {}
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn play_saved_macro(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.play_saved_macro(index, window, cx)
        }) {
            return;
        }
        if self.macro_recording
            || self.macro_playing
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || self.editor().has_composition()
        {
            return;
        }
        let Some(saved) = self.macro_dialogs.library.macros.get(index) else {
            self.notify_editor_error("The selected saved macro is no longer available".into(), cx);
            return;
        };
        self.play_macro_once(saved.steps.clone(), window, cx);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    fn play_macro_once(
        &mut self,
        steps: Vec<MacroStep>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = MacroLibrary::validate_steps(&steps) {
            self.notify_editor_error(format!("{error}; no steps were executed"), cx);
            return;
        }
        self.macro_playing = true;
        self.macro_dialogs.executing_step = true;
        self.editor_mut().begin_undo_action();
        for step in steps {
            self.play_macro_step(step, window, cx);
        }
        self.editor_mut().end_undo_action();
        self.macro_playing = false;
        self.macro_dialogs.executing_step = false;
    }

    pub(super) fn start_macro_repeat(
        &mut self,
        steps: Vec<MacroStep>,
        mode: RepeatMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed_steps = steps.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.start_macro_repeat(routed_steps, mode, window, cx)
        }) {
            return;
        }
        if self.macro_recording
            || self.macro_playing
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().has_composition() {
            self.set_macro_repeat_message(
                "Commit the document composition before macro playback".into(),
            );
            cx.notify();
            return;
        }
        let snapshot = self.editor().display_snapshot();
        let line =
            snapshot.byte_to_line_idx(self.editor().selection().head.0, ropey::LineType::LF_CR);
        let tracker = MacroLibrary::validate_steps(&steps).and_then(|_| {
            RepeatTracker::new(mode, snapshot.len_lines(ropey::LineType::LF_CR), line)
        });
        let tracker = match tracker {
            Ok(tracker) => tracker,
            Err(error) => {
                self.set_macro_repeat_message(error.to_string());
                cx.notify();
                return;
            }
        };
        let identity = std::sync::Arc::new(());
        self.macro_dialogs.active_run = Some(ActiveMacroRun {
            identity: identity.clone(),
            steps,
            next_step: 0,
            tracker,
            buffer: self.workspace.active().id(),
        });
        self.macro_playing = true;
        self.editor_error = None;
        self.editor_mut().begin_undo_action();
        self.set_macro_repeat_message("Running…".into());
        cx.on_next_frame(window, move |view, window, cx| {
            view.advance_macro_repeat(identity, window, cx)
        });
        cx.notify();
    }

    fn advance_macro_repeat(
        &mut self,
        identity: std::sync::Arc<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut run) = self.macro_dialogs.active_run.take() else {
            return;
        };
        if !std::sync::Arc::ptr_eq(&identity, &run.identity) {
            self.macro_dialogs.active_run = Some(run);
            return;
        }
        if run.buffer != self.workspace.active().id()
            || !self.macro_dialogs.repeat_visible
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
        {
            self.macro_dialogs.active_run = Some(run);
            self.finish_macro_repeat(
                Some("Stopped: the playback document is no longer available"),
                cx,
            );
            return;
        }
        self.macro_dialogs.executing_step = true;
        let mut finished = false;
        for _ in 0..32 {
            let step = run.steps[run.next_step].clone();
            self.play_macro_step(step, window, cx);
            run.next_step += 1;
            if run.next_step == run.steps.len() {
                run.next_step = 0;
                let snapshot = self.editor().display_snapshot();
                let line = snapshot
                    .byte_to_line_idx(self.editor().selection().head.0, ropey::LineType::LF_CR);
                if !run
                    .tracker
                    .complete_iteration(snapshot.len_lines(ropey::LineType::LF_CR), line)
                {
                    finished = true;
                    break;
                }
            }
        }
        self.macro_dialogs.executing_step = false;
        self.macro_dialogs.active_run = Some(run);
        if finished {
            let error = self.editor_error.clone();
            self.finish_macro_repeat(error.as_deref(), cx);
        } else {
            cx.on_next_frame(window, move |view, window, cx| {
                view.advance_macro_repeat(identity, window, cx)
            });
        }
        cx.notify();
    }

    fn finish_macro_repeat(&mut self, message: Option<&str>, cx: &mut Context<Self>) {
        if let Some(run) = self.macro_dialogs.active_run.take() {
            if let Some(buffer) = self
                .workspace
                .buffers_mut()
                .find(|buffer| buffer.id() == run.buffer)
            {
                buffer.editor.end_undo_action();
            }
            self.set_macro_repeat_message(
                message.map(str::to_owned).unwrap_or_else(|| {
                    format!("Completed {} iteration(s)", run.tracker.completed())
                }),
            );
        }
        self.macro_playing = false;
        self.macro_dialogs.executing_step = false;
        cx.notify();
    }

    pub(super) fn cancel_macro_repeat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.cancel_macro_repeat(window, cx)
        }) {
            return;
        }
        if let Some(run) = self.macro_dialogs.active_run.as_mut() {
            run.tracker.cancel();
        }
        self.finish_macro_repeat(Some("Cancelled"), cx);
    }

    fn play_macro_step(&mut self, step: MacroStep, window: &mut Window, cx: &mut Context<Self>) {
        match step {
            MacroStep::Scintilla { message, wparam: _, lparam: _ } => {
                self.execute_scintilla_shortcut(message, window, cx)
            }
            MacroStep::CharacterText(text) => {
                self.insert_character_text(text.to_string(), window, cx)
            }
            MacroStep::Rectangle(command) => {
                let (horizontal, vertical, edge) = command.vector();
                self.extend_editor_rectangle(horizontal, vertical, edge, window, cx);
            }
            MacroStep::CopyPath(command) => self.copy_document_path(command, cx),
            MacroStep::Tab => self.keyboard_indent(true, window, cx),
            MacroStep::BackTab => self.keyboard_indent(false, window, cx),
            MacroStep::TextTransform(id) => self.text_transform_command(id, window, cx),
            MacroStep::MultiSelection(command) => self.multi_selection_command(command, window, cx),
            MacroStep::Character { right, extend } => self.move_editor(right, extend, window, cx),
            MacroStep::Vertical { down, extend } => self.move_vertical(down, extend, window, cx),
            MacroStep::LineEdge { end, extend } => self.move_line_edge(end, extend, window, cx),
            MacroStep::DocumentEdge { end, extend } => {
                self.move_document_edge(end, extend, window, cx)
            }
            MacroStep::Word {
                forward,
                extend,
                part,
            } => self.move_editor_word(forward, extend, part, window, cx),
            MacroStep::DisplayEdge { end } => self.move_editor_display_edge(end, window, cx),
            MacroStep::Page {
                down,
                extend,
                rectangle,
            } => self.move_editor_page_selection(down, extend, rectangle, window, cx),
            MacroStep::DeleteWord {
                forward,
                to_line_edge,
            } => self.delete_editor_word(forward, to_line_edge, window, cx),
            MacroStep::Delete { right } => self.delete_editor(right, window, cx),
            MacroStep::Duplicate { line } => self.duplicate_editor(line, window, cx),
            MacroStep::Copy { cut } => self.copy_editor(cut, window, cx),
            MacroStep::Paste => self.paste_editor(window, cx),
            MacroStep::Bookmark(command) => self.bookmark_command(command, window, cx),
            step => {
                let result = match step {
                    MacroStep::ReplaceSelection(text) => self.editor_mut().insert_text(&text),
                    MacroStep::Newline => {
                        let eol = self.editor().line_ending().as_str();
                        self.editor_mut().insert_text(eol)
                    }
                    MacroStep::DeleteLine => self.editor_mut().delete_current_line(),
                    MacroStep::BackspaceNotLine => self.editor_mut().backspace_not_line(),
                    MacroStep::ClearSelection => self.editor_mut().delete_forward(),
                    MacroStep::SelectAll => {
                        let end = ByteOffset(self.editor().document().len_bytes());
                        self.editor_mut().select(Selection {
                            anchor: end,
                            head: ByteOffset(0),
                        })
                    }
                    MacroStep::GoToLine(row) => {
                        let document = self.editor().document();
                        let position = if row >= document.line_count() {
                            Ok(ByteOffset(document.len_bytes()))
                        } else {
                            document.line_start(row)
                        };
                        position.and_then(|head| {
                            self.editor_mut().select(Selection { anchor: head, head })
                        })
                    }
                    _ => return,
                };
                self.editor_changed(result, window, cx);
            }
        }
    }
}
