use super::*;
use crate::clipboard::{ClipboardKind, ClipboardText};

const SCINTILLA_LINE_BYTES: &str = "com.notepad-matrix.scintilla-line-bytes";
const SCINTILLA_LINE_MARKER: &str = "com.notepad-matrix.scintilla-line-copy";

fn write_scintilla_line_bytes(bytes: &[u8], line_copy: bool) -> io::Result<()> {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData, NSString};
    let length = u32::try_from(bytes.len()).map_err(|error| io::Error::other(error.to_string()))?;
    let item = NSPasteboardItem::new();
    let mut terminated = bytes.to_vec();
    terminated.push(0);
    let marker = [u8::from(line_copy)];
    let length_bytes = length.to_le_bytes();
    for (kind, data) in [
        (SCINTILLA_LINE_BYTES, bytes),
        (SCINTILLA_LINE_MARKER, marker.as_slice()),
        ("com.notepad-matrix.binary-content", terminated.as_slice()),
        ("com.notepad-matrix.binary-content-length", length_bytes.as_slice()),
        ("public.utf8-plain-text", bytes),
    ] {
        if !item.setData_forType(&NSData::with_bytes(data), &NSString::from_str(kind)) { return Err(io::Error::other("Could not prepare the raw line clipboard item")); }
    }
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    let item: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*item);
    if !board.writeObjects(&NSArray::from_slice(&[item])) { return Err(io::Error::other("Could not publish the raw line clipboard item")); }
    Ok(())
}

fn read_scintilla_line_bytes() -> io::Result<Option<(Vec<u8>, bool)>> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    let board = NSPasteboard::generalPasteboard();
    let Some(items) = board.pasteboardItems() else { return Ok(None); };
    for item in items.iter() {
        if let Some(bytes) = item.dataForType(&NSString::from_str(SCINTILLA_LINE_BYTES)) {
            let line_copy = match item.dataForType(&NSString::from_str(SCINTILLA_LINE_MARKER)).map(|data| data.to_vec()) {
                Some(marker) if marker == [1] => true,
                Some(marker) if marker == [0] => false,
                None => false,
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid raw line clipboard marker")),
            };
            return Ok(Some((bytes.to_vec(), line_copy)));
        }
    }
    Ok(None)
}

pub(super) fn write_text(text: String, cx: &mut App) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
}

pub(super) fn has_text(cx: &App) -> bool {
    match read_scintilla_line_bytes() { Ok(Some(_)) => return true, Ok(None) => {}, Err(error) => eprintln!("Could not read the raw line clipboard marker: {error}") }
    cx.read_from_clipboard().is_some_and(|item| {
        item.entries
            .iter()
            .any(|entry| matches!(entry, gpui::ClipboardEntry::String(_)))
    })
}

pub(super) fn read_text(cx: &App) -> Option<ClipboardText> {
    let payload = read_payload(cx)?;
    Some(ClipboardText {
        text: payload.text,
        line_copy: payload.line_copy,
    })
}

pub(super) fn read_payload(cx: &App) -> Option<crate::input::SelectionClipboard> {
    let item = cx.read_from_clipboard()?;
    let mut text = String::new();
    let mut found = false;
    for entry in &item.entries {
        if let gpui::ClipboardEntry::String(value) = entry {
            text.push_str(value.text());
            found = true;
        }
    }
    let kind = ClipboardKind::from_metadata(item.metadata().map(String::as_str));
    found.then(|| crate::input::SelectionClipboard {
        text,
        line_copy: kind == ClipboardKind::Line,
        rectangular: kind == ClipboardKind::Rectangle,
    })
}

impl MatrixWindow {
    pub(super) fn copy_scintilla_lines(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.copy_scintilla_lines(cut, window, cx)) { return; }
        if !self.clipboard_ready() { return; }
        if let Err(error) = self.editor_mut().commit_composition() { self.editor_changed(Err(error), window, cx); return; }
        let copied = (|| {
            let document = self.editor().document().byte_snapshot();
            let selected = self.editor().selection().range();
            let first = document.line_index(selected.start.0).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?;
            let last = document.line_index(selected.end.0).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?;
            let start = document.line_start(first).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?;
            let end = if last + 1 < document.line_count() { document.line_start(last + 1).map_err(|error| crate::editor::EditError::Storage(error.to_string()))? } else { document.len() };
            let bytes = document.slice(start..end).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?.to_vec();
            Ok::<_, crate::editor::EditError>((ByteOffset(start)..ByteOffset(end), bytes))
        })();
        let (range, bytes) = match copied { Ok(value) => value, Err(error) => { self.editor_changed(Err(error), window, cx); return; } };
        if cut {
            if let Err(error) = self.editor_mut().set_selections(crate::editor::SelectionSet::single(Selection { anchor: range.end, head: range.start })) { self.editor_changed(Err(error), window, cx); return; }
            if self.editor().is_read_only() { self.editor_changed(Ok(()), window, cx); self.editor_focus.focus(window, cx); return; }
        }
        if let Err(error) = write_scintilla_line_bytes(&bytes, false) { self.notify_editor_error(error.to_string(), cx); return; }
        if cut { let result = self.editor_mut().replace_selection_bytes(&[]); self.editor_changed(result, window, cx); }
        self.editor_focus.focus(window, cx);
        cx.notify();
        cx.stop_propagation();
    }
    pub(super) fn binary_clipboard(
        &mut self,
        command: crate::clipboard::BinaryCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::clipboard::{BINARY_METADATA, BinaryCommand};
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.binary_clipboard(command, window, cx)
        }) {
            return;
        }
        if !self.clipboard_ready() {
            return;
        }
        if command == BinaryCommand::Paste {
            if let Some(payload) = crate::native_binary_clipboard::read(
                    &objc2_app_kit::NSPasteboard::generalPasteboard(),
                )
            {
                self.paste_binary_bytes(&payload.bytes, payload.recorded_length, window, cx);
                return;
            }
            let Some(item) = cx.read_from_clipboard() else {
                return;
            };
            let text = item
                .entries()
                .iter()
                .filter_map(|entry| {
                    if let gpui::ClipboardEntry::String(value) = entry {
                        Some(value.text().as_str())
                    } else {
                        None
                    }
                })
                .collect::<String>();
            if !item
                .entries()
                .iter()
                .any(|entry| matches!(entry, gpui::ClipboardEntry::String(_)))
            {
                return;
            }
            let recorded_length = item
                .metadata()
                .is_some_and(|metadata| metadata == BINARY_METADATA);
            let text = crate::clipboard::binary_paste_text(&text, recorded_length).to_owned();
            self.paste_binary_bytes(text.as_bytes(), recorded_length, window, cx);
        } else {
            let bytes = match crate::clipboard::binary_selection(self.editor_mut()) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => return,
                Err(error) => {
                    self.editor_changed(Err(error), window, cx);
                    return;
                }
            };
            if let Ok(text) = std::str::from_utf8(&bytes) {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_metadata(
                    text.to_owned(),
                    BINARY_METADATA.into(),
                ));
            }
            if let Err(error) = crate::native_binary_clipboard::write(
                    &objc2_app_kit::NSPasteboard::generalPasteboard(),
                    &bytes,
                )
            {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
            if command == BinaryCommand::Cut {
                let result = if self.editor().is_read_only() {
                    let caret = self.editor().document().selection().range().start;
                    self.editor_mut().select(Selection { anchor: caret, head: caret })
                } else { self.editor_mut().replace_selection_bytes(&[]) };
                self.editor_changed(result, window, cx);
                self.editor_focus.focus(window, cx);
            } else {
                self.editor_focus.focus(window, cx);
                cx.notify();
            }
        }
    }

    fn paste_binary_bytes(
        &mut self,
        bytes: &[u8],
        recorded_length: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = if self.editor().is_read_only() {
            let caret = self.editor().document().selection().range().start;
            self.editor_mut().select(Selection { anchor: caret, head: caret })
        } else if recorded_length {
            self.editor_mut().replace_selection_bytes(&[])
                .and_then(|()| self.editor_mut().add_bytes(bytes))
        } else { self.editor_mut().replace_selection_bytes(bytes) };
        self.editor_changed(result, window, cx);
        self.editor_focus.focus(window, cx);
    }

    pub(super) fn copy_document_path(
        &mut self,
        command: crate::clipboard::CopyPath,
        cx: &mut Context<Self>,
    ) {
        self.record_macro_step(crate::macros::MacroStep::CopyPath(command));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            command.text(&self.workspace),
        ));
    }

    fn clipboard_ready(&self) -> bool {
        !self.file_busy
            && !self.close_dialog
            && !self.save_all_dialog
            && !self.batch_editing_blocked()
    }

    pub(super) fn copy_editor(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clipboard_ready() {
            cx.stop_propagation();
            return;
        }
        if cut && self.editor().document().is_read_only() {
            cx.stop_propagation();
            return;
        }
        if !self.line_copy_cut_without_selection && self.editor().selections().ranges().iter().all(|range| range.is_empty()) {
            cx.stop_propagation();
            return;
        }
        let payload = match self.editor().clipboard_payload() {
            Ok(payload) => payload,
            Err(error) => { self.editor_changed(Err(error), window, cx); cx.stop_propagation(); return; }
        };
        if cut && let Err(error) = self.editor_mut().cut_selection() {
            self.editor_changed(Err(error), window, cx);
            cx.stop_propagation();
            return;
        }
        self.record_macro_step(crate::macros::MacroStep::Copy { cut });
        let kind = if payload.rectangular {
            ClipboardKind::Rectangle
        } else if payload.line_copy {
            ClipboardKind::Line
        } else {
            ClipboardKind::Stream
        };
        let item = if let Some(metadata) = kind.metadata() {
            gpui::ClipboardItem::new_string_with_metadata(payload.text, metadata.into())
        } else {
            gpui::ClipboardItem::new_string(payload.text)
        };
        cx.write_to_clipboard(item);
        if cut {
            self.editor_changed(Ok(()), window, cx);
        }
        self.editor_focus.focus(window, cx);
        self.macro_step_pending = false;
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn paste_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clipboard_ready() {
            match read_scintilla_line_bytes() {
                Ok(Some((bytes, line_copy))) => {
                    let eol = self.editor().line_ending().as_str().as_bytes();
                    let mut converted = Vec::with_capacity(bytes.len());
                    let mut index = 0;
                    while index < bytes.len() {
                        if matches!(bytes[index], b'\r' | b'\n') {
                            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') { index += 1; }
                            converted.extend_from_slice(eol);
                        } else { converted.push(bytes[index]); }
                        index += 1;
                    }
                    let result = (|| {
                        if self.editor().is_read_only() { return Err(crate::editor::EditError::ReadOnly); }
                        if line_copy && self.editor().selection().range().is_empty() {
                            let document = self.editor().document().byte_snapshot();
                            let line = document.line_index(self.editor().selection().head.0).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?;
                            let start = document.line_start(line).map_err(|error| crate::editor::EditError::Storage(error.to_string()))?;
                            self.editor_mut().set_selections(crate::editor::SelectionSet::single(Selection { anchor: ByteOffset(start), head: ByteOffset(start) }))?;
                        }
                        self.editor_mut().replace_selection_bytes(&converted)
                    })();
                    self.record_macro_step(crate::macros::MacroStep::Paste);
                    self.editor_changed(result, window, cx);
                    self.editor_focus.focus(window, cx);
                    cx.stop_propagation();
                    return;
                }
                Ok(None) => {},
                Err(error) => { self.notify_editor_error(error.to_string(), cx); cx.stop_propagation(); return; }
            }
        }
        if self.clipboard_ready()
            && let Some(payload) = read_payload(cx)
        {
            self.record_macro_step(crate::macros::MacroStep::Paste);
            let result = self.editor_mut().paste_payload(&payload, 4);
            self.editor_changed(result, window, cx);
            self.editor_focus.focus(window, cx);
        }
        cx.stop_propagation();
    }
}
