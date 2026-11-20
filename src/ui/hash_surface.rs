use super::*;
use super::dialog_font::px;
use crate::hashes::HashAlgorithm;
use crate::localization::Text;
use gpui::Entity;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "hash_field.rs"]
mod field;

struct TextState {
    visible: bool,
    input: Entity<field::Field>,
    result: Entity<field::Field>,
    source: String,
    each_line: bool,
    controls: [FocusHandle; 3],
    _changed: gpui::Subscription,
}

struct FilesState {
    visible: bool,
    paths: Entity<field::Field>,
    result: Entity<field::Field>,
    directory: PathBuf,
    busy: bool,
    message: Option<Text>,
    revision: u64,
    cancelled: Arc<AtomicBool>,
    task: Option<gpui::Task<()>>,
    controls: [FocusHandle; 3],
}

pub(super) struct State {
    text: Vec<TextState>,
    files: Vec<FilesState>,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        let text = HashAlgorithm::ALL.into_iter().map(|algorithm| {
            let input = cx.new(|cx| field::Field::new("hash-text-input", false, cx));
            let result = cx.new(|cx| field::Field::new("hash-text-result", true, cx));
            let changed = cx.observe(&input, move |owner, input, cx| {
                let source = input.read(cx).text();
                let state = &mut owner.hash_state.text[algorithm.index()];
                if state.source == source { return; }
                state.source = source;
                let digest = crate::hashes::text_digest(&state.source, algorithm, state.each_line);
                state.result.update(cx, |field, cx| field.set_text(&digest, cx));
                cx.notify();
            });
            TextState {
                visible: false, input, result, source: String::new(), each_line: false,
                controls: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
                _changed: changed,
            }
        }).collect();
        let files = HashAlgorithm::ALL.into_iter().map(|_| FilesState {
            visible: false,
            paths: cx.new(|cx| field::Field::new("hash-files-paths", true, cx)),
            result: cx.new(|cx| field::Field::new("hash-files-result", true, cx)),
            directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            busy: false, message: None, revision: 0,
            cancelled: Arc::new(AtomicBool::new(false)), task: None,
            controls: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
        }).collect();
        Self { text, files }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        for state in &self.files { state.cancelled.store(true, Ordering::Relaxed); }
    }
}

pub(super) fn dimensions() -> gpui::Size<gpui::Pixels> { size(px(530.), px(294.)) }

fn at(x: f32, y: f32, width: f32, height: f32) -> gpui::Div {
    div().absolute().left(px(x)).top(px(y)).w(px(width)).h(px(height))
}

impl MatrixWindow {
    pub(super) fn show_hash_text(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.hash_state.text[algorithm.index()].visible = true;
        let state = &self.hash_state.text[algorithm.index()];
        for field in [&state.input, &state.result] { field.update(cx, |field, cx| field.set_dark(self.dark, cx)); }
        self.present_floating(floating_windows::DialogKind::HashText(algorithm), window, cx);
        cx.notify();
    }

    pub(super) fn show_hash_files(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.batch_editing_blocked() { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.hash_state.files[algorithm.index()].visible = true;
        let state = &self.hash_state.files[algorithm.index()];
        for field in [&state.paths, &state.result] { field.update(cx, |field, cx| field.set_dark(self.dark, cx)); }
        self.present_floating(floating_windows::DialogKind::HashFiles(algorithm), window, cx);
        cx.notify();
    }

    pub(super) fn hash_text_visible(&self, algorithm: HashAlgorithm) -> bool {
        self.hash_state.text[algorithm.index()].visible
    }

    pub(super) fn hash_files_visible(&self, algorithm: HashAlgorithm) -> bool {
        self.hash_state.files[algorithm.index()].visible
    }

    pub(super) fn hash_text_title(&self, algorithm: HashAlgorithm) -> &'static str {
        crate::localization::dialog_title(&self.native_locale(), algorithm.dialog_key(false)).unwrap_or(match algorithm {
            HashAlgorithm::Md5 => "Generate MD5 digest", HashAlgorithm::Sha1 => "Generate SHA-1 digest",
            HashAlgorithm::Sha256 => "Generate SHA-256 digest", HashAlgorithm::Sha512 => "Generate SHA-512 digest",
        })
    }

    pub(super) fn hash_files_title(&self, algorithm: HashAlgorithm) -> &'static str {
        crate::localization::dialog_title(&self.native_locale(), algorithm.dialog_key(true)).unwrap_or(match algorithm {
            HashAlgorithm::Md5 => "Generate MD5 digest from files", HashAlgorithm::Sha1 => "Generate SHA-1 digest from files",
            HashAlgorithm::Sha256 => "Generate SHA-256 digest from files", HashAlgorithm::Sha512 => "Generate SHA-512 digest from files",
        })
    }

    fn hash_label(&self, algorithm: HashAlgorithm, files: bool, id: u32, fallback: &'static str) -> &'static str {
        crate::localization::dialog_control(&self.native_locale(), algorithm.dialog_key(files), id).unwrap_or(fallback)
    }

    pub(super) fn focus_hash_text(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.hash_state.text[algorithm.index()].input.read(cx).focus.clone();
        focus.focus(window, cx);
    }

    pub(super) fn focus_hash_files(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        self.hash_state.files[algorithm.index()].controls[0].focus(window, cx);
    }

    pub(super) fn hide_hash_text(&mut self, algorithm: HashAlgorithm, cx: &mut Context<Self>) {
        self.hash_state.text[algorithm.index()].visible = false;
        cx.notify();
    }

    pub(super) fn hide_hash_files(&mut self, algorithm: HashAlgorithm, cx: &mut Context<Self>) {
        let state = &mut self.hash_state.files[algorithm.index()];
        state.visible = false;
        state.cancelled.store(true, Ordering::Relaxed);
        state.revision = state.revision.wrapping_add(1);
        state.busy = false;
        cx.notify();
    }

    pub(super) fn close_hash_text(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_floating(floating_windows::DialogKind::HashText(algorithm), window, cx);
    }

    pub(super) fn close_hash_files(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_floating(floating_windows::DialogKind::HashFiles(algorithm), window, cx);
    }

    fn toggle_hash_lines(&mut self, algorithm: HashAlgorithm, cx: &mut Context<Self>) {
        let state = &mut self.hash_state.text[algorithm.index()];
        state.each_line = !state.each_line;
        let result = crate::hashes::text_digest(&state.source, algorithm, state.each_line);
        state.result.update(cx, |field, cx| field.set_text(&result, cx));
        cx.notify();
    }

    fn copy_hash_result(&mut self, algorithm: HashAlgorithm, files: bool, cx: &mut Context<Self>) {
        let field = if files { &self.hash_state.files[algorithm.index()].result }
            else { &self.hash_state.text[algorithm.index()].result };
        let text = field.read(cx).text();
        if !text.is_empty() { cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)); }
    }

    fn choose_hash_files(&mut self, algorithm: HashAlgorithm, window: &mut Window, cx: &mut Context<Self>) {
        let index = algorithm.index();
        let prompt_label = crate::localization::localized_string(&self.native_locale(), "matrix-hash-select-files").unwrap_or("Select files");
        let state = &mut self.hash_state.files[index];
        if state.busy { return; }
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        state.cancelled.store(true, Ordering::Relaxed);
        state.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = state.cancelled.clone();
        let prompt = super::native_path_prompt::prompt_for_paths_in(&state.directory,
            gpui::PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(prompt_label.into()) }, window, cx);
        state.busy = true;
        state.message = None;
        state.task = Some(cx.spawn(async move |owner, cx| {
            let response = prompt.await;
            let paths = match owner.update(cx, |view, cx| {
                let state = &mut view.hash_state.files[index];
                if state.revision != revision || cancelled.load(Ordering::Relaxed) { return None; }
                match response {
                    Ok(response) => {
                        if let Some(directory) = response.directory { state.directory = directory; }
                        if let Some(paths) = response.selection {
                            state.message = Some(Text::resource("matrix-hash-calculating", "Calculating digest…"));
                            cx.notify();
                            Some(paths)
                        } else { state.busy = false; cx.notify(); None }
                    }
                    Err(error) => { state.busy = false; state.message = Some(Text::resource("matrix-hash-selection-failed", "File selection failed: $ERROR$").replace("$ERROR$", error.to_string())); cx.notify(); None }
                }
            }) {
                Ok(Some(paths)) => paths,
                Ok(None) => return,
                Err(error) => { eprintln!("Hash file selection lost its owner: {error}"); return; }
            };
            let worker_cancelled = cancelled.clone();
            let result = cx.background_executor().spawn(async move {
                crate::hashes::files_digest(&paths, algorithm, &worker_cancelled)
            }).await;
            if let Err(error) = owner.update(cx, |view, cx| {
                let state = &mut view.hash_state.files[index];
                if state.revision != revision || cancelled.load(Ordering::Relaxed) { return; }
                state.busy = false;
                state.message = if result.failures.is_empty() { None }
                    else { Some(Text::resource("matrix-hash-read-failed", "Unable to read the following files:\n$FILES$").replace("$FILES$", result.failures.join("\n"))) };
                if result.replace {
                    state.paths.update(cx, |field, cx| field.set_text(&result.paths, cx));
                    state.result.update(cx, |field, cx| field.set_text(&result.results, cx));
                }
                cx.notify();
            }) { eprintln!("Hash file results lost their owner: {error}"); }
        }));
        cx.notify();
    }

    pub(super) fn render_hash_text(&self, algorithm: HashAlgorithm, _: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.hash_text_visible(algorithm) { return None; }
        let state = &self.hash_state.text[algorithm.index()];
        let locale = self.native_locale();
        for field in [&state.input, &state.result] { field.update(cx, |field, cx| field.set_locale(&locale, cx)); }
        let line_label = self.hash_label(algorithm, false, 1932, "Treat each line as a separate string");
        let copy_label = self.hash_label(algorithm, false, 1934, "Copy to Clipboard");
        let close_label = self.hash_label(algorithm, false, 2, "Close");
        Some(div().id("hash-text-dialog").debug_selector(|| "hash-text-dialog".into())
            .role(Role::Dialog).aria_label(self.hash_text_title(algorithm)).size_full()
            .font_family(dialog_font::font_family()).text_size(px(12.))
            .text_color(rgb(if self.dark { 0xf0f0f0 } else { 0x202020 }))
            .bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 })).relative()
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" { view.close_hash_text(algorithm, window, cx); cx.stop_propagation(); }
            }))
            .child(div().id("hash-each-line").debug_selector(|| "hash-each-line".into())
                .role(Role::CheckBox).aria_label(line_label).aria_toggled(state.each_line.into())
                .absolute().left(px(10.5)).top(px(7.)).h(px(18.))
                .track_focus(&state.controls[0]).flex().gap(px(5.))
                .child(if state.each_line { "☑" } else { "☐" }).child(line_label)
                .on_click(cx.listener(move |view, _, _, cx| view.toggle_hash_lines(algorithm, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "space" { view.toggle_hash_lines(algorithm, cx); cx.stop_propagation(); }
                })))
            .child(at(10.5, 31.5, 510., 96.).child(state.input.clone()))
            .child(at(10.5, 136.5, 510., 96.).child(state.result.clone()))
            .child(at(385.5, 237., 135., 21.).child(self.hash_button(algorithm, "hash-text-copy", copy_label, false, 1)
                .on_click(cx.listener(move |view, _, _, cx| view.copy_hash_result(algorithm, false, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.copy_hash_result(algorithm, false, cx); cx.stop_propagation(); }
                }))))
            .child(at(219., 264., 90., 21.).child(self.hash_button(algorithm, "hash-text-close", close_label, false, 2)
                .on_click(cx.listener(move |view, _, window, cx| view.close_hash_text(algorithm, window, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.close_hash_text(algorithm, window, cx); cx.stop_propagation(); }
                }))))
            .into_any_element())
    }

    fn hash_button(&self, algorithm: HashAlgorithm, id: &'static str, label: &'static str, files: bool, index: usize) -> gpui::Stateful<gpui::Div> {
        let focus = if files { &self.hash_state.files[algorithm.index()].controls[index] } else { &self.hash_state.text[algorithm.index()].controls[index] };
        div().id(id).debug_selector(move || id.into()).role(Role::Button).aria_label(label)
            .track_focus(focus).px(px(6.)).size_full().flex().items_center().justify_center()
            .border_1().border_color(rgb(0x808080)).bg(rgb(if self.dark { 0x383838 } else { 0xe1e1e1 }))
            .child(label)
    }

    pub(super) fn render_hash_files(&self, algorithm: HashAlgorithm, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.hash_files_visible(algorithm) { return None; }
        let state = &self.hash_state.files[algorithm.index()];
        let locale = self.native_locale();
        for field in [&state.paths, &state.result] { field.update(cx, |field, cx| field.set_locale(&locale, cx)); }
        let message = state.message.as_ref().map(|message| message.render(&locale)).unwrap_or_default();
        let calculating = crate::localization::localized_string(&locale, "matrix-hash-calculating").unwrap_or("Calculating digest…");
        let dismiss_error = crate::localization::localized_string(&locale, "matrix-hash-dismiss-error").unwrap_or("Dismiss error");
        let choose_label = self.hash_label(algorithm, true, 1922, match algorithm {
            HashAlgorithm::Md5 => "Choose files to generate MD5...", HashAlgorithm::Sha1 => "Choose files to generate SHA-1...",
            HashAlgorithm::Sha256 => "Choose files to generate SHA-256...", HashAlgorithm::Sha512 => "Choose files to generate SHA-512...",
        });
        let copy_label = self.hash_label(algorithm, true, 1924, "Copy to Clipboard");
        let close_label = self.hash_label(algorithm, true, 2, "Close");
        let choose_width = window.text_system().shape_line(choose_label.into(), px(12.), &[gpui::TextRun {
            len: choose_label.len(), font: gpui::font(dialog_font::font_family()), color: window.text_style().color,
            background_color: None, underline: None, strikethrough: None,
        }], None).width().ceil() + px(14.);
        Some(div().id("hash-files-dialog").debug_selector(|| "hash-files-dialog".into())
            .role(Role::Dialog).aria_label(self.hash_files_title(algorithm)).size_full()
            .font_family(dialog_font::font_family()).text_size(px(12.))
            .text_color(rgb(if self.dark { 0xf0f0f0 } else { 0x202020 }))
            .bg(rgb(if self.dark { 0x282828 } else { 0xf0f0f0 })).relative()
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" { view.close_hash_files(algorithm, window, cx); cx.stop_propagation(); }
            }))
            .child(at(10.5, 6., 196.5, 21.).w(px(196.5).max(choose_width)).child(self.hash_button(algorithm, "hash-files-choose", choose_label, true, 0)
                .whitespace_nowrap()
                .opacity(if state.busy { 0.5 } else { 1. })
                .on_click(cx.listener(move |view, _, window, cx| view.choose_hash_files(algorithm, window, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.choose_hash_files(algorithm, window, cx); cx.stop_propagation(); }
                }))))
            .child(at(10.5, 31.5, 510., 96.).child(state.paths.clone()))
            .child(at(10.5, 136.5, 510., 96.).child(state.result.clone()))
            .child(at(385.5, 237., 135., 21.).child(self.hash_button(algorithm, "hash-files-copy", copy_label, true, 1)
                .on_click(cx.listener(move |view, _, _, cx| view.copy_hash_result(algorithm, true, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.copy_hash_result(algorithm, true, cx); cx.stop_propagation(); }
                }))))
            .child(at(219., 264., 90., 21.).child(self.hash_button(algorithm, "hash-files-close", close_label, true, 2)
                .on_click(cx.listener(move |view, _, window, cx| view.close_hash_files(algorithm, window, cx)))
                .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") { view.close_hash_files(algorithm, window, cx); cx.stop_propagation(); }
                }))))
            .when(state.busy, |dialog| dialog.child(at(264., 6., 255., 21.).child(calculating)))
            .when(!message.is_empty() && !state.busy, |dialog| dialog.child(div().id("hash-files-message")
                .debug_selector(|| "hash-files-message".into()).role(Role::Alert).aria_label(message.clone())
                .absolute().left(px(10.5)).top(px(31.5)).w(px(510.)).h(px(96.)).p(px(5.))
                .bg(rgb(if self.dark { 0x302020 } else { 0xffeeee })).border_1().border_color(rgb(0xb00020))
                .overflow_y_scroll().child(message)
                .child(div().id("hash-files-dismiss-error").role(Role::Button).aria_label(dismiss_error)
                    .tab_index(0).child(dismiss_error)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.hash_state.files[algorithm.index()].message = None; cx.notify();
                    }))
                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            view.hash_state.files[algorithm.index()].message = None; cx.stop_propagation(); cx.notify();
                        }
                    })))))
            .into_any_element())
    }
}

impl MatrixWindow {
    pub(super) fn copy_selection_hash(
        &mut self,
        algorithm: crate::hashes::HashAlgorithm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().has_composition() {
            self.notify_localized_editor_error(Text::resource("matrix-hash-selection-composition", "Selection hashing during active composition is not implemented yet"), cx);
            return;
        }
        if self.editor().document().selections().ranges().len() != 1 { return; }
        let range = self.editor().document().selection().range();
        if range.is_empty() {
            return;
        }
        let selected = match self.editor().document().byte_snapshot().slice(range.start.0..range.end.0) {
            Ok(selected) => selected,
            Err(error) => {
                let detail = error.to_string();
                self.notify_localized_editor_error(Text::resource("matrix-hash-selection-read-failed", "Unable to read selection: $ERROR$").replace("$ERROR$", detail), cx);
                return;
            }
        };
        if let Some(digest) = crate::hashes::selection_buffer_digest(&selected, algorithm) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(digest));
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }
}
