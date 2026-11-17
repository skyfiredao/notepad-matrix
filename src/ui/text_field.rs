use std::ops::Range;

use super::*;
use super::dialog_font::px;
use crate::editor::EditError;
use gpui::{
    ContentMask, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    GlobalElementId, LayoutId, Pixels, Point, ShapedLine, Style, TextRun, UTF16Selection, fill,
    point, relative,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderPalette {
    pub dark: bool,
    pub background: u32,
    pub softer_background: u32,
    pub hot_background: u32,
    pub pure_background: u32,
    pub error_background: u32,
    pub text: u32,
    pub darker_text: u32,
    pub disabled_text: u32,
    pub link_text: u32,
    pub edge: u32,
    pub hot_edge: u32,
    pub disabled_edge: u32,
}

impl RenderPalette {
    pub const LIGHT: Self = Self {
        dark: false,
        background: 0xffffff,
        softer_background: 0xffffff,
        hot_background: 0xe5f1fb,
        pure_background: 0xf0f0f0,
        error_background: 0xff6666,
        text: 0x202020,
        darker_text: 0x202020,
        disabled_text: 0x808080,
        link_text: 0x0066cc,
        edge: 0xaaaaaa,
        hot_edge: 0x0078d7,
        disabled_edge: 0xaaaaaa,
    };

    pub fn control_text(self, enabled: bool) -> u32 {
        if enabled { self.text } else { self.disabled_text }
    }

    pub fn control_edge(self, enabled: bool) -> u32 {
        if enabled { self.edge } else { self.disabled_edge }
    }

    pub fn control_background(self, enabled: bool) -> u32 {
        if enabled { self.softer_background } else if self.dark { self.pure_background } else { 0xf0f0f0 }
    }

    pub fn button_background(self, enabled: bool) -> u32 {
        if self.dark { self.control_background(enabled) } else if enabled { 0xe1e1e1 } else { 0xcccccc }
    }

    pub fn group_edge(self) -> u32 {
        if self.dark { self.edge } else { 0xd3d3d3 }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AppFieldPalette(pub Option<RenderPalette>);

impl gpui::Global for AppFieldPalette {}

fn app_render_palette(cx: &App) -> Option<RenderPalette> {
    cx.try_global::<AppFieldPalette>().and_then(|palette| palette.0)
}

pub(super) fn set_app_render_palette(palette: Option<RenderPalette>, cx: &mut App) {
    if app_render_palette(cx) != palette {
        cx.set_global(AppFieldPalette(palette));
    }
}

pub(super) struct TextField {
    state: EditorSession,
    focus: FocusHandle,
    label: &'static str,
    selector: &'static str,
    layout: Option<FieldLayout>,
    error: Option<String>,
    compact: bool,
    compact_height: f32,
    reported_text: String,
    enabled: bool,
    read_only: bool,
    search_failure: bool,
    history_arrow: bool,
    max_utf16: Option<usize>,
    monospaced: bool,
    text_alignment: gpui::TextAlign,
    render_palette: Option<RenderPalette>,
}

struct FieldLayout {
    line: ShapedLine,
    bounds: Bounds<Pixels>,
    offset: Pixels,
    revision: crate::input::DisplayRevision,
}

fn unexpected_invisible(ch: char) -> bool {
    let code = u32::from(ch);
    code <= 0x1f && ch != '\t'
        || matches!(code,
            0x7f | 0xa0 | 0xad | 0x61c | 0x70f | 0x1680 | 0x180e
            | 0x2000..=0x200f | 0x2028..=0x202f | 0x205f | 0x2060..=0x2064
            | 0x2066..=0x206f | 0x3000 | 0xfff9..=0xfffb)
}

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", EditorBackspace, Some("MatrixTextField")),
        KeyBinding::new("delete", EditorDelete, Some("MatrixTextField")),
        KeyBinding::new("left", EditorLeft, Some("MatrixTextField")),
        KeyBinding::new("right", EditorRight, Some("MatrixTextField")),
        KeyBinding::new("shift-left", EditorSelectLeft, Some("MatrixTextField")),
        KeyBinding::new("shift-right", EditorSelectRight, Some("MatrixTextField")),
        KeyBinding::new("home", EditorDocumentStart, Some("MatrixTextField")),
        KeyBinding::new("end", EditorDocumentEnd, Some("MatrixTextField")),
        KeyBinding::new("ctrl-a", EditorSelectAll, Some("MatrixTextField")),
        KeyBinding::new("ctrl-c", EditorCopy, Some("MatrixTextField")),
        KeyBinding::new("ctrl-x", EditorCut, Some("MatrixTextField")),
        KeyBinding::new("ctrl-v", EditorPaste, Some("MatrixTextField")),
        KeyBinding::new("ctrl-z", EditorUndo, Some("MatrixTextField")),
        KeyBinding::new("ctrl-y", EditorRedo, Some("MatrixTextField")),
    ]
}

impl TextField {
    pub(super) fn new(label: &'static str, selector: &'static str, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<AppFieldPalette>(|field, cx| {
            field.layout = None;
            cx.notify();
        }).detach();
        Self {
            state: EditorSession::new(""),
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            label,
            selector,
            layout: None,
            error: None,
            compact: false,
            compact_height: 21.,
            reported_text: String::new(),
            enabled: true,
            read_only: false,
            search_failure: false,
            history_arrow: false,
            max_utf16: None,
            monospaced: false,
            text_alignment: gpui::TextAlign::Left,
            render_palette: None,
        }
    }
    pub(super) fn compact(
        label: &'static str,
        selector: &'static str,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut field = Self::new(label, selector, cx);
        field.compact = true;
        field.history_arrow = true;
        let alignment = match selector {
            "PrintSettings.headerRight" | "PrintSettings.footerRight" | "delimiterSelection.leftmostDelimiter" => gpui::TextAlign::Right,
            "character-range-low" | "character-range-high"
                | "PrintSettings.headerMiddle" | "PrintSettings.footerMiddle"
                | "PrintSettings.margeLeft" | "PrintSettings.margeTop" | "PrintSettings.margeRight" | "PrintSettings.margeBottom"
                | "Searching.inSelectionAutocheckThreshold" | "Searching.fillFindWhatThreshold"
                | "auto-insert.UserDefinePair.0.open" | "auto-insert.UserDefinePair.0.close"
                | "auto-insert.UserDefinePair.1.open" | "auto-insert.UserDefinePair.1.close"
                | "auto-insert.UserDefinePair.2.open" | "auto-insert.UserDefinePair.2.close" => gpui::TextAlign::Center,
            path if path.starts_with("TabSettings.") && path.ends_with(".size") => gpui::TextAlign::Center,
            _ => gpui::TextAlign::Left,
        };
        field.with_text_alignment(alignment)
    }
    pub(super) fn with_text_alignment(mut self, alignment: gpui::TextAlign) -> Self {
        self.text_alignment = alignment;
        self.layout = None;
        self
    }
    pub(super) fn without_history_arrow(mut self) -> Self {
        self.history_arrow = false;
        self
    }
    pub(super) fn text(&self) -> String {
        self.state.committed_text()
    }

    pub(super) fn with_max_utf16(mut self, limit: usize) -> Self {
        self.max_utf16 = Some(limit);
        self
    }

    pub(super) fn focus_handle(&self) -> FocusHandle { self.focus.clone() }
    pub(super) fn set_monospaced(&mut self, monospaced: bool, cx: &mut Context<Self>) {
        if self.monospaced != monospaced { self.monospaced = monospaced; self.layout = None; cx.notify(); }
    }

    pub(super) fn render_palette(&self) -> Option<RenderPalette> { self.render_palette }

    fn effective_render_palette(&self, cx: &App) -> Option<RenderPalette> {
        self.render_palette.or_else(|| app_render_palette(cx))
    }

    pub(super) fn set_render_palette(&mut self, palette: Option<RenderPalette>, cx: &mut Context<Self>) {
        if self.render_palette != palette {
            self.render_palette = palette;
            self.layout = None;
            cx.notify();
        }
    }

    fn background_colour(&self, palette: Option<RenderPalette>) -> u32 {
        if let Some(palette) = palette {
            if !self.enabled { palette.pure_background }
            else if self.search_failure { palette.error_background }
            else if self.read_only { palette.background }
            else { palette.softer_background }
        } else if !self.enabled { 0xf0f0f0 }
        else if self.search_failure { 0xff6666 }
        else { 0xffffff }
    }

    fn text_colour(&self, palette: Option<RenderPalette>) -> u32 {
        if let Some(palette) = palette {
            if !self.enabled { palette.disabled_text }
            else if self.search_failure && !palette.dark { 0xffffff }
            else { palette.text }
        } else if !self.enabled { 0x808080 }
        else if self.search_failure { 0xffffff }
        else { 0x202020 }
    }

    fn border_colour(&self, focused: bool, palette: Option<RenderPalette>) -> u32 {
        if let Some(palette) = palette.filter(|palette| palette.dark) {
            if !self.enabled { palette.disabled_edge }
            else if focused { palette.hot_edge }
            else { palette.edge }
        } else if self.compact {
            if focused { 0x0078d4 } else { 0xadadad }
        } else { 0x808080 }
    }

    pub(super) fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled || self.read_only { return; }
        let range = self.state.selection().range();
        let result = self.range_input_text(range, text).and_then(|text| self.state.insert_text(&text));
        self.changed(result, window, cx);
    }

    fn range_input_text(&self, range: Range<ByteOffset>, text: &str) -> Result<String, EditError> {
        let limit = match self.max_utf16 {
            Some(limit) => limit,
            None if matches!(self.selector, "character-range-low" | "character-range-high") => 3,
            None => return Ok(text.to_owned()),
        };
        let current = self.state.display_snapshot();
        let selected = current.try_slice(range.start.0..range.end.0)
            .map_err(|error| EditError::Storage(error.to_string()))?;
        let mut remaining = limit.saturating_sub(current.len_utf16() - selected.len_utf16());
        let mut inserted = String::new();
        for ch in text.chars() {
            if ch.len_utf16() > remaining { break; }
            inserted.push(ch);
            remaining -= ch.len_utf16();
        }
        Ok(inserted)
    }
    pub(super) fn with_tab_index(mut self, index: isize) -> Self {
        self.focus = self.focus.tab_index(index);
        self
    }

    pub(super) fn with_read_only(mut self) -> Self {
        self.read_only = true;
        self.state.set_read_only(true);
        self
    }
    pub(super) fn with_compact_height(mut self, height: f32) -> Self {
        self.compact_height = height;
        self
    }
    pub(super) fn set_compact_height(&mut self, height: f32, cx: &mut Context<Self>) {
        if self.compact_height != height {
            self.compact_height = height;
            self.layout = None;
            cx.notify();
        }
    }
    pub(super) fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        self.state.set_read_only(read_only);
        cx.notify();
    }
    pub(super) fn set_label(&mut self, label: &'static str, cx: &mut Context<Self>) {
        self.label = label;
        cx.notify();
    }
    pub(super) fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        self.focus = self.focus.clone().tab_stop(enabled);
        cx.notify();
    }
    pub(super) fn enabled(&self) -> bool { self.enabled }
    pub(super) fn set_search_failure(&mut self, failed: bool, cx: &mut Context<Self>) {
        if self.search_failure != failed {
            self.search_failure = failed;
            cx.notify();
        }
    }
    pub(super) fn has_composition(&self) -> bool {
        self.state.has_composition()
    }
    pub(super) fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.state = EditorSession::new(text);
        self.state.set_read_only(self.read_only);
        self.select_all(window, cx);
    }
    pub(super) fn collapse_to_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = ByteOffset(self.state.display_snapshot().len());
        let result = self.state.select_display(Selection { anchor: end, head: end });
        self.changed(result, window, cx);
    }
    pub(super) fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled { return; }
        self.focus.focus(window, cx);
        self.select_all(window, cx);
    }

    pub(super) fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    fn changed(
        &mut self,
        result: Result<(), EditError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = result.err().map(|error| error.to_string());
        let text = self.state.committed_text();
        if text != self.reported_text {
            self.reported_text = text;
            if matches!(self.selector, "find-query" | "replace-input") {
                window.dispatch_action(Box::new(find_surface::FindFieldWarning(false)), cx);
            }
        }
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = ByteOffset(self.state.display_snapshot().len());
        let result = self.state.select_display(Selection {
            anchor: ByteOffset(0),
            head: end,
        });
        self.changed(result, window, cx);
    }
    fn adjacent(&self, byte: usize, right: bool) -> usize {
        let text = self.state.display_snapshot();
        if right && byte < text.len() {
            byte + text.char(byte).len_utf8()
        } else if !right && byte > 0 {
            let mut previous = byte - 1;
            while !text.is_char_boundary(previous) {
                previous -= 1;
            }
            previous
        } else {
            byte
        }
    }
    fn move_cursor(
        &mut self,
        right: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut selection = self.state.selection();
        let range = selection.range();
        selection.head = if !extend && !range.is_empty() {
            if right { range.end } else { range.start }
        } else {
            ByteOffset(self.adjacent(selection.head.0, right))
        };
        if !extend {
            selection.anchor = selection.head;
        }
        let result = self.state.select_display(selection);
        self.changed(result, window, cx);
        cx.stop_propagation();
    }
    fn delete(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled || self.read_only { cx.stop_propagation(); return; }
        let mut range = self.state.selection().range();
        if range.is_empty() {
            if right {
                range.end = ByteOffset(self.adjacent(range.end.0, true));
            } else {
                range.start = ByteOffset(self.adjacent(range.start.0, false));
            }
        }
        let result = self.state.replace_display_range(range, "");
        self.changed(result, window, cx);
        cx.stop_propagation();
    }
    fn copy(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled || cut && self.read_only { cx.stop_propagation(); return; }
        let range = self.state.selection().range();
        if !range.is_empty() {
            let payload = match self.state.clipboard_text() {
                Ok(payload) => payload,
                Err(error) => { self.changed(Err(error), window, cx); cx.stop_propagation(); return; }
            };
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                payload.text,
            ));
            if cut {
                let result = self.state.replace_display_range(range, "");
                self.changed(result, window, cx);
            }
        }
        cx.stop_propagation();
    }
    fn current_layout(&self) -> Option<&FieldLayout> {
        self.layout
            .as_ref()
            .filter(|layout| layout.revision == self.state.display_revision())
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selector = self.selector;
        let palette = self.effective_render_palette(cx);
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .role(Role::TextInput)
            .aria_label(self.label)
            .aria_value(self.state.display_text())
            .when(self.enabled, |field| field.track_focus(&self.focus))
            .a11y_synthetic_children({
                let enabled = self.enabled;
                let read_only = self.read_only;
                move |builder| {
                    if !enabled { builder.parent_node().set_disabled(); }
                    if read_only { builder.parent_node().set_read_only(); }
                }
            })
            .key_context("MatrixTextField")
            .w_full()
            .h(px(if self.compact { self.compact_height } else { 26. }))
            .px(px(4.))
            .when(self.history_arrow, |field| field.pr(px(22.)))
            .border_1()
            .border_color(rgb(self.border_colour(self.focus.is_focused(window), palette)))
            .bg(rgb(self.background_colour(palette)))
            .text_color(rgb(self.text_colour(palette)))
            .when_some(palette.filter(|palette| palette.dark), |field, palette| {
                field.when(self.enabled, |field| field.hover(move |style| style.border_color(rgb(palette.hot_edge))))
            })
            .font_family(if !self.monospaced { dialog_font::font_family() } else { "Menlo".to_owned() })
            .text_size(px(if self.compact { if self.render_palette.is_some() { 12. } else { 11. } } else { 13. }))
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|field, event: &gpui::MouseDownEvent, window, cx| {
                    if !field.enabled { return; }
                    field.focus.focus(window, cx);
                    let byte = field
                        .current_layout()
                        .map(|layout| {
                            layout.line.closest_index_for_x(
                                event.position.x - layout.bounds.left() + layout.offset,
                            )
                        })
                        .unwrap_or_else(|| field.state.display_snapshot().len());
                    let result = field.state.select_display(Selection {
                        anchor: ByteOffset(byte),
                        head: ByteOffset(byte),
                    });
                    field.changed(result, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|field, _: &EditorBackspace, window, cx| {
                    field.delete(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|field, _: &EditorDelete, window, cx| field.delete(true, window, cx)),
            )
            .on_action(cx.listener(|field, _: &EditorLeft, window, cx| {
                field.move_cursor(false, false, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorRight, window, cx| {
                field.move_cursor(true, false, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorSelectLeft, window, cx| {
                field.move_cursor(false, true, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorSelectRight, window, cx| {
                field.move_cursor(true, true, window, cx)
            }))
            .on_action(cx.listener(|field, _: &EditorSelectAll, window, cx| {
                field.select_all(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorDocumentStart, window, cx| {
                let result = field.state.move_to_document_edge(false, false);
                field.changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorDocumentEnd, window, cx| {
                let result = field.state.move_to_document_edge(true, false);
                field.changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|field, _: &EditorCopy, window, cx| field.copy(false, window, cx)),
            )
            .on_action(cx.listener(|field, _: &EditorCut, window, cx| field.copy(true, window, cx)))
            .on_action(cx.listener(|field, _: &EditorPaste, window, cx| {
                if !field.enabled || field.read_only { cx.stop_propagation(); return; }
                if let Some(payload) = clipboard_surface::read_payload(cx) {
                    let search_field = matches!(field.selector, "find-query" | "replace-input");
                    if search_field && payload.rectangular {
                        cx.stop_propagation();
                        return;
                    }
                    let text = if search_field {
                        payload.text.split('\0').next().unwrap_or_default()
                    } else { &payload.text };
                    if search_field && text.is_empty() {
                        cx.stop_propagation();
                        return;
                    }
                    let result = field.range_input_text(field.state.display_selection().range(), text)
                        .and_then(|text| field.state.insert_text(&text));
                    let pasted = result.is_ok();
                    field.changed(result, window, cx);
                    if search_field && pasted && text.chars().any(unexpected_invisible) {
                        window.dispatch_action(Box::new(find_surface::FindFieldWarning(true)), cx);
                    }
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorUndo, window, cx| {
                if !field.enabled || field.read_only { cx.stop_propagation(); return; }
                field.state.undo();
                field.changed(Ok(()), window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|field, _: &EditorRedo, window, cx| {
                if !field.enabled || field.read_only { cx.stop_propagation(); return; }
                field.state.redo();
                field.changed(Ok(()), window, cx);
                cx.stop_propagation();
            }))
            .child(FieldElement { field: cx.entity() })
            .when_some(self.error.clone(), |field, error| {
                field.aria_description(error)
            })
    }
}

impl EntityInputHandler for TextField {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.enabled && self.focus.is_focused(window) && !window.modifiers().control && !window.modifiers().alt
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *actual = None;
        let text = self.state.display_snapshot();
        let bytes = editor_surface::byte_range(&text, range.clone()).ok()?;
        *actual = Some(range);
        Some(text.slice(bytes.start.0..bytes.end.0).to_string())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.state.display_snapshot();
        let selected = self.state.selection();
        let range = selected.range();
        Some(UTF16Selection {
            range: text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0),
            reversed: selected.head < selected.anchor,
        })
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result =
            editor_surface::byte_range(&self.state.display_snapshot(), range).and_then(|range| {
                self.state.select_display(Selection {
                    anchor: range.start,
                    head: range.end,
                })
            });
        self.changed(result, window, cx);
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.state.display_snapshot().len_utf16())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if window.modifiers().control {
            return None;
        }
        let text = self.state.display_snapshot();
        self.state
            .marked_range()
            .map(|range| text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0))
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.state.commit_composition();
        self.changed(result, window, cx);
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled || self.read_only { return; }
        if matches!(self.selector, "character-range-low" | "character-range-high"
            | "recent_files_limit" | "pref-history-custom-length" | "snapshot_backup_ms" | "pref-large-file-size")
            && text.chars().any(|ch| !ch.is_ascii_digit()) { return; }
        let result = if let Some(range) = range {
            editor_surface::byte_range(&self.state.display_snapshot(), range)
                .and_then(|range| self.range_input_text(range.clone(), text).and_then(|text| self.state.replace_display_range(range, &text)))
        } else {
            self.range_input_text(self.state.display_selection().range(), text)
                .and_then(|text| self.state.insert_text(&text))
        };
        self.changed(result, window, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled || self.read_only { return; }
        let character_range = matches!(self.selector, "character-range-low" | "character-range-high");
        if (character_range || matches!(self.selector,
            "recent_files_limit" | "pref-history-custom-length" | "snapshot_backup_ms" | "pref-large-file-size"))
            && text.chars().any(|ch| !ch.is_ascii_digit()) { return; }
        if character_range || self.max_utf16.is_some() {
            let result = range.map(|range| editor_surface::byte_range(&self.state.display_snapshot(), range))
                .transpose().and_then(|range| {
                    let replaced = range.clone().or_else(|| self.state.marked_range())
                        .unwrap_or_else(|| self.state.display_selection().range());
                    let text = self.range_input_text(replaced, text)?;
                    let length = text.encode_utf16().count();
                    let selected = selected.map(|range| range.start.min(length)..range.end.min(length));
                    self.state.mark_display_range(range, &text, selected)
                });
            self.changed(result, window, cx);
            return;
        }
        let result = range
            .map(|range| editor_surface::byte_range(&self.state.display_snapshot(), range))
            .transpose()
            .and_then(|range| self.state.mark_display_range(range, text, selected));
        self.changed(result, window, cx);
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bytes = editor_surface::byte_range(&self.state.display_snapshot(), range).ok()?;
        let layout = self.current_layout()?;
        Some(Bounds::from_corners(
            point(
                layout.bounds.left() - layout.offset + layout.line.x_for_index(bytes.start.0),
                layout.bounds.top(),
            ),
            point(
                layout.bounds.left() - layout.offset + layout.line.x_for_index(bytes.end.0),
                layout.bounds.bottom(),
            ),
        ))
    }
    fn selection_bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range, bounds, window, cx)
            .into_iter()
            .collect()
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.current_layout()?;
        if !layout.bounds.contains(&position) {
            return None;
        }
        let byte = layout
            .line
            .closest_index_for_x(position.x - layout.bounds.left() + layout.offset);
        Some(self.state.display_snapshot().byte_to_utf16_idx(byte))
    }
}

struct FieldElement {
    field: Entity<TextField>,
}
impl IntoElement for FieldElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<FieldLayout>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(if self.field.read(cx).compact {
            self.field.read(cx).compact_height - 2.
        } else {
            24.
        })
        .into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<FieldLayout> {
        let field = self.field.read(cx);
        let palette = field.effective_render_palette(cx);
        let text = field.state.display_text();
        let run = TextRun {
            len: text.len(),
            font: gpui::font(if !field.monospaced { dialog_font::font_family() } else { "Menlo".to_owned() }),
            color: if palette.is_some() { rgb(field.text_colour(palette)).into() } else { window.text_style().color },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let selected = field.state.selection().range();
        let runs = if field.enabled && field.compact && field.focus.is_focused(window) && !selected.is_empty() {
            Some(
                [
                    (0..selected.start.0, run.color),
                    (selected.start.0..selected.end.0, rgb(0xffffff).into()),
                    (selected.end.0..text.len(), run.color),
                ]
                .into_iter()
                .filter(|(range, _)| !range.is_empty())
                .map(|(range, color)| TextRun {
                    len: range.len(),
                    color,
                    ..run.clone()
                })
                .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let line = window.text_system().shape_line(
            text.into(),
            px(if field.compact { if field.render_palette.is_some() { 12. } else { 11. } } else { 13. }),
            runs.as_deref().unwrap_or(std::slice::from_ref(&run)),
            None,
        );
        let offset = match field.text_alignment {
            gpui::TextAlign::Center if line.width() < bounds.size.width => (line.width() - bounds.size.width) / 2.,
            gpui::TextAlign::Right if line.width() < bounds.size.width => (line.width() - bounds.size.width + px(1.)).min(px(0.)),
            _ => (line.x_for_index(field.state.selection().head.0) - bounds.size.width + px(3.)).max(px(0.)),
        };
        Some(FieldLayout {
            line,
            bounds,
            offset,
            revision: field.state.display_revision(),
        })
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        paint: &mut Option<FieldLayout>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layout) = paint.take() else {
            return;
        };
        let field = self.field.read(cx);
        let palette = field.effective_render_palette(cx);
        if layout.revision != field.state.display_revision() {
            return;
        }
        let focus = field.focus.clone();
        let selection = field.state.selection();
        let range = selection.range();
        let line_height = px(if field.compact { field.compact_height - 2. } else { 24. });
        let field_compact = field.compact;
        let enabled = field.enabled;
        let focused = field.enabled && focus.is_focused(window);
        if field.enabled {
            window.handle_input(&focus, ElementInputHandler::new(bounds, self.field.clone()), cx);
        }
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if !range.is_empty() && (!field_compact || focused) {
                window.paint_quad(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() - layout.offset + layout.line.x_for_index(range.start.0),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() - layout.offset + layout.line.x_for_index(range.end.0),
                            bounds.bottom(),
                        ),
                    ),
                    if field_compact {
                        gpui::rgba(0x0078d4ff)
                    } else {
                        gpui::rgba(0x0078d450)
                    },
                ));
            }
            if layout
                .line
                .paint(
                    point(bounds.left() - layout.offset, bounds.top()),
                    line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .is_err()
            {
                eprintln!("Text field paint failed");
            }
            if focus.is_focused(window) {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() - layout.offset
                                + layout.line.x_for_index(selection.head.0),
                            bounds.top() + px(2.),
                        ),
                        size(px(1.), (bounds.size.height - px(4.)).max(px(1.))),
                    ),
                    rgb(palette.map_or(0x202020, |palette| palette.control_text(enabled))),
                ));
            }
        });
        self.field
            .update(cx, |field, _| field.layout = Some(layout));
    }
}
