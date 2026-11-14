use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, Bounds, ContentMask, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, GlobalElementId, LayoutId, MouseDownEvent, MouseMoveEvent, Pixels,
    Point, Style, TextRun, UTF16Selection, Window, div, fill, point, px, relative, rgb, rgba, size,
};
use ropey::{LineType, Rope};

use super::*;
use crate::editor::{ByteOffset, EditError, Selection};
use crate::macros::MacroStep;

mod chrome;
mod drag;
pub(super) mod layout;
mod selection_geometry;
mod shaped;
pub(super) use chrome::EditorSymbols;
use chrome::{DisplayRow, EditorDisplay, ScrollAxis};
use layout::VisualRows;
use shaped::EditorShapedLine;

gpui::actions!(
    selection_editing,
    [
        MultiSelectNext,
        MultiSelectNextMatchCase,
        MultiSelectNextWholeWord,
        MultiSelectNextMatchCaseWholeWord,
        MultiSelectAll,
        MultiSelectAllMatchCase,
        MultiSelectAllWholeWord,
        MultiSelectAllMatchCaseWholeWord,
        MultiSelectUndo,
        MultiSelectSkip,
        RotateSelection,
        SwapMainAnchorCaret,
        RectangleLeft,
        RectangleRight,
        RectangleUp,
        RectangleDown,
        RectangleHome,
        RectangleEnd,
        RectanglePageUp,
        RectanglePageDown,
        ShowColumnEditor,
        PageUp,
        PageDown,
        SelectPageUp,
        SelectPageDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteLineLeft,
        DeleteLineRight,
        WordPartLeft,
        WordPartRight,
        SelectWordPartLeft,
        SelectWordPartRight,
        HomeDisplay,
        LineEndDisplay
    ]
);

pub(super) use crate::menu::MultiSelectionCommand;

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("alt-shift-left", RectangleLeft, Some("MatrixEditor")),
        KeyBinding::new("alt-shift-right", RectangleRight, Some("MatrixEditor")),
        KeyBinding::new("alt-shift-up", RectangleUp, Some("MatrixEditor")),
        KeyBinding::new("alt-shift-down", RectangleDown, Some("MatrixEditor")),
        KeyBinding::new("alt-shift-home", RectangleHome, Some("MatrixEditor")),
        KeyBinding::new("alt-shift-end", RectangleEnd, Some("MatrixEditor")),
        KeyBinding::new("alt-shift-pageup", RectanglePageUp, Some("MatrixEditor")),
        KeyBinding::new(
            "alt-shift-pagedown",
            RectanglePageDown,
            Some("MatrixEditor"),
        ),
        KeyBinding::new("alt-c", ShowColumnEditor, Some("MatrixEditor")),
        KeyBinding::new("pageup", PageUp, Some("MatrixEditor")),
        KeyBinding::new("pagedown", PageDown, Some("MatrixEditor")),
        KeyBinding::new("shift-pageup", SelectPageUp, Some("MatrixEditor")),
        KeyBinding::new("shift-pagedown", SelectPageDown, Some("MatrixEditor")),
        KeyBinding::new("ctrl-left", WordLeft, Some("MatrixEditor")),
        KeyBinding::new("ctrl-right", WordRight, Some("MatrixEditor")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("MatrixEditor")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("MatrixEditor")),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, Some("MatrixEditor")),
        KeyBinding::new("ctrl-delete", DeleteWordRight, Some("MatrixEditor")),
        KeyBinding::new("ctrl-shift-backspace", DeleteLineLeft, Some("MatrixEditor")),
        KeyBinding::new("ctrl-shift-delete", DeleteLineRight, Some("MatrixEditor")),
        KeyBinding::new("ctrl-/", WordPartLeft, Some("MatrixEditor")),
        KeyBinding::new("ctrl-\\", WordPartRight, Some("MatrixEditor")),
        KeyBinding::new("ctrl-shift-/", SelectWordPartLeft, Some("MatrixEditor")),
        KeyBinding::new("ctrl-shift-\\", SelectWordPartRight, Some("MatrixEditor")),
        KeyBinding::new("alt-home", HomeDisplay, Some("MatrixEditor")),
        KeyBinding::new("alt-end", LineEndDisplay, Some("MatrixEditor")),
    ]
}

#[cfg(all(test, feature = "native-input-tests"))]
use chrome::TEXT_LEFT;
pub(super) use chrome::{GUTTER_LEFT, SCROLLBAR_SIZE, TEXT_TOP};
use chrome::{TEXT_RIGHT_PADDING, TEXT_VERTICAL_PADDING};

#[derive(Default)]
pub(super) struct EditorChrome {
    text_system: Option<Rc<gpui::WindowTextSystem>>,
    buffer: Option<crate::workspace::BufferId>,
    positions: std::collections::BTreeMap<crate::workspace::BufferId, (usize, Pixels)>,
    p_width: f32,
    line_number_width: Option<Pixels>,
    collapsed: std::collections::BTreeMap<crate::workspace::BufferId, Vec<crate::editor::Document>>,
    content_width: Pixels,
    drag: Option<(bool, f32)>,
    display: EditorDisplay,
    layout: Option<Arc<EditorLayout>>,
    top_anchor: Option<usize>,
    pending_viewport: Option<crate::workspace::ViewportPosition>,
    unwrapped_horizontal_offset: Pixels,
    rectangle_pixels: Option<(usize, Pixels)>,
    selection_drag: Option<usize>,
    observed_selections: Option<crate::editor::SelectionSet>,
    text_drag: Option<drag::SelectionDrag>,
    word_drag: Option<(Selection, ByteOffset)>,
}

impl EditorChrome {
    pub(super) fn p_width(&self) -> f32 {
        self.p_width.max(1.)
    }
    pub(super) fn clear_drag(&mut self) {
        self.drag = None;
        self.rectangle_pixels = None;
        self.selection_drag = None;
        self.text_drag = None;
        self.word_drag = None;
    }
    pub(super) fn invalidate_layout(&mut self) {
        self.layout = None;
    }
    pub(super) fn retire_document(&mut self) {
        self.buffer = None;
        self.positions.clear();
        self.collapsed.clear();
        self.layout = None;
        self.pending_viewport = None;
        self.clear_drag();
        self.observed_selections = None;
    }
}

fn fold_marker_line(marker: &crate::editor::Document, text: &Rope) -> Option<usize> {
    let range = marker.selection().range();
    (!range.is_empty() && range.start.0 < text.len())
        .then(|| text.byte_to_line_idx(range.start.0, LineType::LF_CR))
}

#[derive(PartialEq)]
struct EditorLayoutKey {
    collapsed: Vec<usize>,
    revision: crate::input::DisplayRevision,
    language: crate::syntax::Language,
    display: EditorDisplay,
    symbols: EditorSymbols,
    width: Pixels,
}

struct EditorLayout {
    key: EditorLayoutKey,
    rows: VisualRows,
    shapes: Vec<Arc<EditorShapedLine>>,
    starts: Vec<usize>,
    content_width: Pixels,
}

struct EditorScrollbars {
    vertical: ScrollAxis,
    horizontal: ScrollAxis,
    vertical_bounds: Bounds<Pixels>,
    horizontal_bounds: Bounds<Pixels>,
    vertical_up: Bounds<Pixels>,
    vertical_down: Bounds<Pixels>,
    horizontal_left: Bounds<Pixels>,
    horizontal_right: Bounds<Pixels>,
}

impl EditorScrollbars {
    fn new(
        bounds: Bounds<Pixels>,
        line_height: Pixels,
        total: f32,
        top: usize,
        content_width: Pixels,
        horizontal: Pixels,
        text_inset: Pixels,
    ) -> Self {
        let height = (f32::from(bounds.size.height) - SCROLLBAR_SIZE).max(0.);
        let width = (f32::from(bounds.size.width) - SCROLLBAR_SIZE).max(0.);
        let vertical_track = chrome::scrollbar_track(height);
        let horizontal_track = chrome::scrollbar_track(width);
        let capacity = ((height - TEXT_VERTICAL_PADDING) / f32::from(line_height))
            .floor()
            .max(1.);
        Self {
            vertical: ScrollAxis::new(
                vertical_track.end - vertical_track.start,
                total,
                capacity,
                top as f32,
            ),
            horizontal: ScrollAxis::new(
                horizontal_track.end - horizontal_track.start,
                f32::from(content_width) + 1.,
                (width - f32::from(text_inset)).max(1.),
                f32::from(horizontal),
            ),
            vertical_bounds: Bounds::new(
                point(
                    bounds.left() + px(width),
                    bounds.top() + px(vertical_track.start),
                ),
                size(
                    px(SCROLLBAR_SIZE),
                    px(vertical_track.end - vertical_track.start),
                ),
            ),
            horizontal_bounds: Bounds::new(
                point(
                    bounds.left() + px(horizontal_track.start),
                    bounds.top() + px(height),
                ),
                size(
                    px(horizontal_track.end - horizontal_track.start),
                    px(SCROLLBAR_SIZE),
                ),
            ),
            vertical_up: Bounds::new(
                point(bounds.left() + px(width), bounds.top()),
                size(px(SCROLLBAR_SIZE), px(vertical_track.start)),
            ),
            vertical_down: Bounds::new(
                point(
                    bounds.left() + px(width),
                    bounds.top() + px(vertical_track.end),
                ),
                size(px(SCROLLBAR_SIZE), px(height - vertical_track.end)),
            ),
            horizontal_left: Bounds::new(
                point(bounds.left(), bounds.top() + px(height)),
                size(px(horizontal_track.start), px(SCROLLBAR_SIZE)),
            ),
            horizontal_right: Bounds::new(
                point(
                    bounds.left() + px(horizontal_track.end),
                    bounds.top() + px(height),
                ),
                size(px(width - horizontal_track.end), px(SCROLLBAR_SIZE)),
            ),
        }
    }
}

fn paint_scroll_arrow(
    bounds: Bounds<Pixels>,
    vertical: bool,
    decrement: bool,
    enabled: bool,
    window: &mut Window,
) {
    let center = bounds.center();
    for step in 0..4 {
        let along = if decrement {
            step as f32 - 2.
        } else {
            2. - step as f32
        };
        let (origin, extent) = if vertical {
            (
                point(center.x - px(step as f32), center.y + px(along)),
                size(px((1 + 2 * step) as f32), px(1.)),
            )
        } else {
            (
                point(center.x + px(along), center.y - px(step as f32)),
                size(px(1.), px((1 + 2 * step) as f32)),
            )
        };
        window.paint_quad(fill(
            Bounds::new(origin, extent),
            rgb(if enabled { 0x808080 } else { 0xc0c0c0 }),
        ));
    }
}

pub(super) struct PaintedLine {
    start: usize,
    end: usize,
    logical_start: usize,
    logical_line: usize,
    first: bool,
    last: bool,
    origin: Point<Pixels>,
    shape: Arc<EditorShapedLine>,
    clip: Bounds<Pixels>,
    guides: Vec<Pixels>,
    whitespace: Vec<(Pixels, Pixels, bool)>,
    space_width: Pixels,
}

impl PaintedLine {
    fn geometry(&self) -> selection_geometry::RowGeometry {
        selection_geometry::RowGeometry {
            start: self.start,
            end: self.end,
            last: self.last,
            space_width: f32::from(self.space_width),
        }
    }
    fn x_for_byte(&self, byte: usize) -> Pixels {
        if self.last && byte > self.end {
            return self.shape.width();
        }
        self.shape
            .x_for_index(byte.saturating_sub(self.logical_start))
    }
    fn owns_caret(&self, byte: usize) -> bool {
        byte >= self.start && (byte < self.end || (self.last && byte <= self.end))
    }
}

fn selection_geometry(
    selected: crate::editor::SelectionRange,
) -> selection_geometry::SelectionGeometry {
    selection_geometry::SelectionGeometry {
        anchor: selected.selection.anchor.0,
        head: selected.selection.head.0,
        anchor_virtual: selected.anchor_virtual,
        head_virtual: selected.head_virtual,
    }
}

fn rectangle_limit(
    selections: &crate::editor::SelectionSet,
    forward: bool,
) -> Result<crate::editor::SelectionSet, EditError> {
    let mut positions = selections.ranges().iter().flat_map(|range| {
        [
            (range.selection.anchor, range.anchor_virtual),
            (range.selection.head, range.head_virtual),
        ]
    });
    let first = positions.next().ok_or(EditError::InvalidRange)?;
    let target = positions.fold(first, |target, position| {
        if forward {
            target.max(position)
        } else {
            target.min(position)
        }
    });
    crate::editor::SelectionSet::new(
        vec![crate::editor::SelectionRange {
            selection: Selection {
                anchor: target.0,
                head: target.0,
            },
            anchor_virtual: target.1,
            head_virtual: target.1,
        }],
        0,
    )
}

fn byte_from_utf16(text: &Rope, offset: usize) -> Result<ByteOffset, EditError> {
    if offset > text.len_utf16() {
        return Err(EditError::InvalidRange);
    }
    let byte = text.utf16_to_byte_idx(offset);
    if text.byte_to_utf16_idx(byte) != offset {
        return Err(EditError::InvalidBoundary);
    }
    Ok(ByteOffset(byte))
}

pub(super) fn byte_range(text: &Rope, range: Range<usize>) -> Result<Range<ByteOffset>, EditError> {
    if range.start > range.end {
        return Err(EditError::InvalidRange);
    }
    Ok(byte_from_utf16(text, range.start)?..byte_from_utf16(text, range.end)?)
}

fn shape_editor_row(
    view: &MatrixWindow,
    text: &Rope,
    row: usize,
    space_width: Pixels,
    baseline: Pixels,
    window: &Window,
) -> (usize, EditorShapedLine) {
    let start = text.line_to_byte_idx(row, LineType::LF_CR);
    let raw = text.line(row, LineType::LF_CR).to_string();
    let projection = DisplayRow::with_symbols(&raw, view.editor_symbols);
    let end = projection.content_len();
    let run = TextRun {
        len: end,
        font: gpui::font(view.editor_font_family()),
        color: window.text_style().color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs = view.editor_style_runs(
        start,
        start + end,
        view.editor().marked_range().as_ref(),
        run.clone(),
    );
    let mut source = 0;
    let mut projected_runs = Vec::new();
    for (mut item, font_size) in runs {
        let next = source + item.len;
        item.len = projection.display_index(next) - projection.display_index(source);
        source = next;
        if item.len > 0 {
            projected_runs.push((item, font_size));
        }
    }
    let suffix = projection.text.len() - projection.display_index(end);
    if suffix > 0 {
        projected_runs.push((
            TextRun {
                len: suffix,
                color: rgb(0x808080).into(),
                ..run
            },
            view.editor_font_size(),
        ));
    }
    (
        start,
        EditorShapedLine::new(
            projection,
            &raw,
            projected_runs,
            space_width,
            baseline,
            (view.editor_font_size() - px(6.)).max(px(1.)),
            window,
        ),
    )
}

impl MatrixWindow {
    pub(super) fn multi_selection_command(
        &mut self,
        command: MultiSelectionCommand,
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
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        if let Some(step) = MacroStep::multi_selection(command.source_id()) {
            self.record_macro_step(step);
        }
        let result = match command {
            MultiSelectionCommand::Next(options) => self
                .editor_mut()
                .select_next_occurrence(options)
                .map(|_| ()),
            MultiSelectionCommand::All(options) => self
                .editor_mut()
                .select_all_occurrences(options)
                .map(|_| ()),
            MultiSelectionCommand::Undo => {
                self.editor_mut().remove_last_selection();
                Ok(())
            }
            MultiSelectionCommand::Skip => self.editor_mut().skip_occurrence().map(|_| ()),
            MultiSelectionCommand::Rotate => {
                self.editor_mut().rotate_selection();
                Ok(())
            }
            MultiSelectionCommand::Swap => {
                self.editor_mut().swap_main_anchor_caret();
                Ok(())
            }
        };
        self.editor_focus.focus(window, cx);
        self.editor_follow_caret = true;
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn extend_editor_rectangle(
        &mut self,
        horizontal: i32,
        vertical: i32,
        edge: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        use crate::macros::RectangleMovement;
        let movement = match (horizontal, vertical, edge) {
            (-1, 0, None) => Some(RectangleMovement::Left),
            (1, 0, None) => Some(RectangleMovement::Right),
            (0, -1, None) => Some(RectangleMovement::Up),
            (0, 1, None) => Some(RectangleMovement::Down),
            (0, 0, Some(false)) => Some(RectangleMovement::Home),
            (0, 0, Some(true)) => Some(RectangleMovement::End),
            _ => None,
        };
        if let Some(movement) = movement {
            self.record_macro_step(MacroStep::Rectangle(movement));
        }
        let result = (|| {
            let selections = self.editor().selections();
            let text = self.editor().display_snapshot();
            let primary = selections.primary();
            let anchor = {
                let first = if selections.is_rectangular() {
                    if selections.primary_index() == 0 {
                        selections.ranges().last().copied().unwrap_or(primary)
                    } else {
                        selections.ranges()[0]
                    }
                } else {
                    primary
                };
                crate::editor::ColumnPosition {
                    line: text.byte_to_line_idx(first.selection.anchor.0, LineType::LF_CR),
                    column: self
                        .editor()
                        .document()
                        .display_column(first.selection.anchor, 4)?
                        + first.anchor_virtual,
                }
            };
            let line = text.byte_to_line_idx(primary.selection.head.0, LineType::LF_CR);
            let column = self
                .editor()
                .document()
                .display_column(primary.selection.head, 4)?
                + primary.head_virtual;
            let line = line
                .saturating_add_signed(vertical as isize)
                .min(text.len_lines(LineType::LF_CR).saturating_sub(1));
            let raw = text.line(line, LineType::LF_CR).to_string();
            let start = text.line_to_byte_idx(line, LineType::LF_CR);
            let content = raw.trim_end_matches(['\r', '\n']);
            let line_end = start + content.len();
            let column = if let Some(end) = edge {
                if end {
                    self.editor()
                        .document()
                        .display_column(ByteOffset(line_end), 4)?
                } else {
                    let indentation_byte = content
                        .char_indices()
                        .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
                        .map_or(content.len(), |(byte, _)| byte);
                    let indentation = self
                        .editor()
                        .document()
                        .display_column(ByteOffset(start + indentation_byte), 4)?;
                    if primary.selection.head.0 == start + indentation_byte {
                        0
                    } else {
                        indentation
                    }
                }
            } else if horizontal != 0 && primary.head_virtual == 0 {
                if horizontal > 0 && primary.selection.head.0 == line_end {
                    column + 1
                } else if horizontal < 0 && primary.selection.head.0 == start {
                    0
                } else {
                    let next = self
                        .editor()
                        .adjacent_position(primary.selection.head, horizontal > 0)?;
                    self.editor().document().display_column(next, 4)?
                }
            } else {
                column.saturating_add_signed(horizontal as isize)
            };
            self.editor_mut().select_rectangle(
                anchor,
                crate::editor::ColumnPosition { line, column },
                4,
            )?;
            Ok::<_, EditError>(anchor)
        })();
        self.editor_changed(result.map(|_| ()), window, cx);
        cx.stop_propagation();
    }

    pub(super) fn prepare_editor_document(&mut self) {
        let id = self.workspace.active().id();
        if self.editor_chrome.buffer == Some(id) {
            return;
        }
        if let Some(previous) = self.editor_chrome.buffer.replace(id) {
            self.editor_chrome.positions.insert(
                previous,
                (self.editor_top_line, self.editor_horizontal_offset),
            );
        }
        let restored = self.editor_chrome.positions.get(&id).copied();
        let (top, horizontal) = restored.unwrap_or((0, px(0.)));
        self.editor_top_line = top;
        self.editor_horizontal_offset = horizontal;
        self.editor_follow_caret = restored.is_none();
        self.editor_lines.clear();
        self.editor_bounds = None;
        self.editor_chrome.layout = None;
        self.editor_chrome.top_anchor = None;
        self.editor_chrome.pending_viewport = None;
        self.editor_chrome.clear_drag();
        self.editor_chrome.observed_selections = None;
        self.editor_painted_revision = None;
        self.editor_accessibility = None;
        self.editor_preferred_x = None;
        self.editor_scroll_remainder = 0.;
        self.editor_observed_revision = self.editor().display_revision();
        self.editor_observed_selection = self.editor().selection();
        self.syntax_cache = None;
        self.udl.syntax_cache = None;
    }

    pub(super) fn capture_editor_viewport(&self) -> Option<crate::workspace::ViewportPosition> {
        if let Some(position) = &self.editor_chrome.pending_viewport {
            return Some(position.clone());
        }
        let layout = self.editor_cached_layout()?;
        let rows = layout.rows.rows();
        let line = rows.get(self.editor_top_line)?.logical_line;
        let first = rows.partition_point(|row| row.logical_line < line);
        let end = rows.partition_point(|row| row.logical_line <= line);
        Some(crate::workspace::ViewportPosition {
            first_visible_line: line,
            wrap_offset: self.editor_top_line - first,
            wrap_count: end - first,
        })
    }

    pub(super) fn restore_editor_viewport(
        &mut self,
        position: Option<crate::workspace::ViewportPosition>,
    ) {
        self.editor_chrome.top_anchor = None;
        self.editor_chrome.pending_viewport = position;
    }

    fn editor_layout_for_width(&self, width: Pixels, window: &Window) -> Arc<EditorLayout> {
        let key = EditorLayoutKey {
            collapsed: self.editor_collapsed_headers(),
            revision: self.editor().display_revision(),
            language: self.workspace.active().language(),
            display: self.editor_chrome.display,
            symbols: self.editor_symbols,
            width: if self.editor_word_wrap_enabled() {
                width
            } else {
                px(0.)
            },
        };
        if let Some(layout) = &self.editor_chrome.layout
            && layout.key == key
        {
            return layout.clone();
        }
        let text = self.editor().display_snapshot();
        let mut rows = VisualRows::default();
        let mut shapes = Vec::new();
        let mut starts = Vec::new();
        let mut content_width = px(0.);
        let space_width = window
            .text_system()
            .shape_line(
                " ".into(),
                self.editor_font_size(),
                &[TextRun {
                    len: 1,
                    font: gpui::font(self.editor_font_family()),
                    color: rgb(0x808080).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width();
        let folds = self.udl_fold_ranges();
        let baseline = self.editor_style_baseline();
        for logical in 0..text.len_lines(LineType::LF_CR) {
            let raw = text.line(logical, LineType::LF_CR).to_string();
            let content = raw.trim_end_matches(['\r', '\n']);
            let (start, shape) =
                shape_editor_row(self, &text, logical, space_width, baseline, window);
            content_width = content_width.max(shape.width());
            let indent_byte = content
                .char_indices()
                .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
                .map_or(content.len(), |(byte, _)| byte);
            let mut indent = shape.x_for_index(indent_byte);
            if indent > width - space_width * 15. {
                indent = px(0.);
            }
            let mut ranges = if self.editor_word_wrap_enabled() {
                layout::wrap_ranges(content, f32::from(width), f32::from(indent), |byte| {
                    f32::from(shape.x_for_index(byte))
                })
            } else {
                std::iter::once(0..content.len()).collect()
            };
            if self.editor_word_wrap_enabled()
                && self.editor_symbols.eol
                && raw.len() > content.len()
                && let Some(last) = ranges.last()
                && !last.is_empty()
                && shape.width() - shape.x_for_index(last.start)
                    + if ranges.len() > 1 { indent } else { px(0.) }
                    > width
            {
                ranges.push(content.len()..content.len());
            }
            rows.push_line(logical, start, &ranges, f32::from(indent));
            starts.push(start);
            shapes.push(Arc::new(shape));
        }
        let collapsed: Vec<_> = folds
            .into_iter()
            .filter(|(header, _)| self.editor_fold_collapsed(*header))
            .collect();
        rows.hide_folded(&collapsed);
        Arc::new(EditorLayout {
            key,
            rows,
            shapes,
            starts,
            content_width,
        })
    }

    fn editor_text_width(&self) -> Pixels {
        self.editor_bounds.map_or(px(600.), |bounds| {
            (bounds.size.width
                - self.editor_text_origin()
                - px(chrome::TEXT_RIGHT_PADDING + SCROLLBAR_SIZE))
            .max(px(1.))
        })
    }

    fn editor_space_width(&self, window: &Window) -> Pixels {
        window
            .text_system()
            .shape_line(
                " ".into(),
                self.editor_font_size(),
                &[TextRun {
                    len: 1,
                    font: gpui::font(self.editor_font_family()),
                    color: rgb(0).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width()
    }

    pub(super) fn editor_measured_split_breaks(&self, window: &Window) -> Vec<usize> {
        let text = self.editor().display_snapshot();
        let width = self.editor_text_width();
        let space_width = self.editor_space_width(window);
        let baseline = self.editor_style_baseline();
        let mut breaks = Vec::new();
        for logical in 0..text.len_lines(LineType::LF_CR) {
            let raw = text.line(logical, LineType::LF_CR).to_string();
            let content = raw.trim_end_matches(['\r', '\n']);
            let (start, shape) =
                shape_editor_row(self, &text, logical, space_width, baseline, window);
            let indentation = content
                .char_indices()
                .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
                .map_or(content.len(), |(byte, _)| byte);
            let mut indent = shape.x_for_index(indentation);
            if indent > width - space_width * 15. {
                indent = px(0.);
            }
            breaks.extend(
                layout::wrap_ranges(content, f32::from(width), f32::from(indent), |byte| {
                    f32::from(shape.x_for_index(byte))
                })
                .into_iter()
                .skip(1)
                .map(|range| start + range.start),
            );
        }
        breaks
    }

    pub(super) fn editor_word_wrap_enabled(&self) -> bool {
        self.editor_chrome.display.word_wrap
    }

    pub(super) fn editor_fold_collapsed(&self, header: usize) -> bool {
        self.editor_collapsed_headers().contains(&header)
    }

    pub(super) fn editor_collapsed_headers(&self) -> Vec<usize> {
        let text = self.editor().display_snapshot();
        self.editor_chrome
            .collapsed
            .get(&self.workspace.active().id())
            .map(|markers| {
                markers
                    .iter()
                    .filter_map(|marker| fold_marker_line(marker, &text))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn toggle_editor_fold(
        &mut self,
        header: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        let Some((_, end)) = self
            .udl_fold_ranges()
            .into_iter()
            .find(|(start, _)| *start == header)
        else {
            return;
        };
        let id = self.workspace.active().id();
        let text = self.editor().display_snapshot();
        let markers = self.editor_chrome.collapsed.entry(id).or_default();
        let existing = markers
            .iter()
            .position(|marker| fold_marker_line(marker, &text) == Some(header));
        if let Some(index) = existing {
            markers.remove(index);
        } else {
            let start = text.line_to_byte_idx(header, LineType::LF_CR);
            let length = text
                .line(header, LineType::LF_CR)
                .to_string()
                .trim_end_matches(['\r', '\n'])
                .len();
            let mut marker = self.editor().document().clone_view();
            if let Err(error) = marker.select(Selection {
                anchor: ByteOffset(start),
                head: ByteOffset(start + length),
            }) {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
            self.editor_chrome
                .collapsed
                .entry(id)
                .or_default()
                .push(marker);
            let caret = text.byte_to_line_idx(self.editor().selection().head.0, LineType::LF_CR);
            if caret > header && caret <= end {
                let position = ByteOffset(text.line_to_byte_idx(header, LineType::LF_CR));
                let result = self.editor_mut().select_display(Selection {
                    anchor: position,
                    head: position,
                });
                self.editor_changed(result, window, cx);
            }
        }
        self.editor_chrome.layout = None;
        self.editor_painted_revision = None;
        self.editor_follow_caret = false;
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_editor_word_wrap(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let anchor = self
            .editor_chrome
            .layout
            .as_ref()
            .and_then(|layout| layout.rows.rows().get(self.editor_top_line))
            .map(|row| row.range.start)
            .or_else(|| self.editor_lines.first().map(|line| line.start));
        let follow = self.editor_follow_caret;
        self.editor_chrome.display.word_wrap = !self.editor_chrome.display.word_wrap;
        if self.editor_word_wrap_enabled() {
            self.editor_chrome.unwrapped_horizontal_offset = self.editor_horizontal_offset;
            self.editor_horizontal_offset = px(0.);
        } else {
            self.editor_horizontal_offset = self.editor_chrome.unwrapped_horizontal_offset;
        }
        self.editor_display_changed(window, cx);
        self.editor_follow_caret = follow;
        self.editor_chrome.top_anchor = anchor;
        self.editor_word_wrap_enabled()
    }

    pub(super) fn editor_visible_gutter_rows(&self) -> Vec<(usize, bool)> {
        if self.editor_cached_layout().is_none() {
            return Vec::new();
        }
        self.editor_lines
            .iter()
            .map(|line| (line.logical_line, line.first))
            .collect()
    }
    pub(super) fn editor_visual_row_count(&self) -> usize {
        self.editor_cached_layout().map_or_else(
            || self.editor().display_snapshot().len_lines(LineType::LF_CR),
            |layout| layout.rows.len(),
        )
    }
    pub(super) fn editor_logical_line_for_visual_row(&self, row: usize) -> usize {
        self.editor_cached_layout()
            .and_then(|layout| {
                layout
                    .rows
                    .rows()
                    .get(row.min(layout.rows.len().saturating_sub(1)))
            })
            .map_or(row, |row| row.logical_line)
    }

    fn editor_cached_layout(&self) -> Option<&EditorLayout> {
        self.editor_chrome.layout.as_deref().filter(|layout| {
            layout.key.revision == self.editor().display_revision()
                && layout.key.language == self.workspace.active().language()
                && layout.key.display == self.editor_chrome.display
        })
    }

    pub(super) fn editor_visual_row_source_range(&self, row: usize) -> Option<Range<usize>> {
        if let Some(layout) = self.editor_cached_layout() {
            return layout.rows.rows().get(row).map(|row| row.range.clone());
        }
        let text = self.editor().display_snapshot();
        if row >= text.len_lines(LineType::LF_CR) {
            return None;
        }
        let start = text.line_to_byte_idx(row, LineType::LF_CR);
        let length = text
            .line(row, LineType::LF_CR)
            .to_string()
            .trim_end_matches(['\r', '\n'])
            .len();
        Some(start..start + length)
    }

    pub(super) fn editor_visual_row_indent(&self, row: usize) -> Pixels {
        self.editor_cached_layout()
            .and_then(|layout| layout.rows.rows().get(row))
            .map_or(px(0.), |row| px(row.indent))
    }
    pub(super) fn editor_font_size(&self) -> Pixels {
        px(self.editor_chrome.display.font_size())
    }
    pub(super) fn editor_font_family(&self) -> &'static str {
        chrome::DEFAULT_EDITOR_FONT_FAMILY
    }
    pub(super) fn editor_text_system(&self) -> Option<&gpui::WindowTextSystem> {
        self.editor_chrome.text_system.as_deref()
    }
    pub(super) fn editor_zoom_delta(&self) -> i32 {
        self.editor_chrome.display.zoom()
    }
    pub(super) fn editor_text_origin(&self) -> Pixels {
        px(chrome::TEXT_LEFT) + self.editor_line_number_width() - px(32.) - px(14.)
            + self.editor_fold_margin_width()
    }
    pub(super) fn reveal_editor_selection(&mut self) {
        self.editor_chrome.pending_viewport = None;
        self.editor_follow_caret = true;
        self.editor_preferred_x = None;
        self.editor_scroll_remainder = 0.;
    }
    fn editor_text_inset(&self) -> Pixels {
        px(chrome::TEXT_INSET) + self.editor_line_number_width() - px(32.) - px(14.)
            + self.editor_fold_margin_width()
    }
    pub(super) fn editor_line_number_width(&self) -> Pixels {
        self.editor_chrome.line_number_width.unwrap_or(px(32.))
    }
    pub(super) fn editor_gutter_width(&self) -> Pixels {
        px(chrome::GUTTER_WIDTH) + self.editor_line_number_width() - px(32.) - px(14.)
            + self.editor_fold_margin_width()
    }
    pub(super) fn editor_fold_margin_width(&self) -> Pixels {
        let hidden = match self.workspace.active().language() {
            crate::syntax::Language::NormalText => true,
            crate::syntax::Language::Builtin(id) => crate::languages::catalog()
                .ok()
                .and_then(|catalog| catalog.by_id(id))
                .is_some_and(|profile| {
                    matches!(
                        profile.source_symbol.as_str(),
                        "L_TEXT"
                            | "L_ASCII"
                            | "L_BATCH"
                            | "L_MAKEFILE"
                            | "L_HASKELL"
                            | "L_SMALLTALK"
                            | "L_KIX"
                            | "L_ADA"
                    )
                }),
            _ => false,
        };
        px(if hidden { 0. } else { 14. })
    }
    pub(super) fn editor_line_number_font(&self) -> (&'static str, Pixels) {
        let theme = crate::languages::catalog()
            .ok()
            .map(|catalog| catalog.themes().theme(self.dark));
        let margin =
            theme.and_then(|theme| theme.global_styles.iter().find(|style| style.id == 33));
        let attribute = |name| {
            margin
                .and_then(|style| style.attributes.get(name))
                .filter(|value| !value.is_empty())
        };
        let family = attribute("fontName").map_or(self.editor_font_family(), String::as_str);
        let points = attribute("fontSize")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|points| *points > 0.);
        (
            family,
            points.map_or(self.editor_font_size(), |points| {
                px((points + self.editor_zoom_delta() as f32).max(1.) * 96. / 72.)
            }),
        )
    }
    fn refresh_editor_gutter(&mut self, cx: &Context<Self>) {
        let row = self.editor_top_line + self.editor_visible_capacity() + 1;
        let line_count = self.editor().document().line_count();
        let last = self
            .editor_cached_layout()
            .map_or(row.min(line_count), |layout| {
                layout
                    .rows
                    .rows()
                    .get(row)
                    .map_or(line_count, |row| row.logical_line)
            });
        let digits = last.to_string().len().max(3);
        let (family, size) = self.editor_line_number_font();
        let text_system = cx.text_system();
        let font = text_system.resolve_font(&gpui::font(family));
        let width = text_system.layout_width(font, size, '8');
        let width = px(8. + digits as f32 * f32::from(width).round());
        if self.editor_chrome.line_number_width != Some(width) {
            self.editor_chrome.line_number_width = Some(width);
        }
    }
    pub(super) fn editor_line_height(&self) -> Pixels {
        self.editor_style_line_height()
            .max(px(self.editor_chrome.display.line_height()))
    }
    pub(super) fn editor_visible_capacity(&self) -> usize {
        self.editor_bounds.map_or(50, |bounds| {
            ((f32::from(bounds.size.height) - SCROLLBAR_SIZE - TEXT_VERTICAL_PADDING)
                / f32::from(self.editor_line_height()))
            .floor()
            .max(1.) as usize
        })
    }
    pub(super) fn editor_all_characters_shown(&self) -> bool {
        self.editor_symbols.all()
    }
    pub(super) fn editor_symbol_shown(&self, command: ViewCommand) -> Option<bool> {
        match command {
            ViewCommand::Whitespace => Some(self.editor_symbols.whitespace),
            ViewCommand::EndOfLine => Some(self.editor_symbols.eol),
            ViewCommand::Nonprinting => Some(self.editor_symbols.nonprinting),
            ViewCommand::ControlCharacters => Some(self.editor_symbols.controls),
            ViewCommand::AllCharacters => Some(self.editor_symbols.all()),
            _ => None,
        }
    }
    pub(super) fn configure_editor_symbols(&mut self, settings: &serde_json::Value) {
        self.editor_symbols = EditorSymbols {
            whitespace: settings["whiteSpaceShow"].as_bool().unwrap_or(false),
            eol: settings["eolShow"].as_bool().unwrap_or(false),
            nonprinting: settings["npcShow"].as_bool().unwrap_or(false),
            controls: settings["ccShow"].as_bool().unwrap_or(true),
        };
    }
    pub(super) fn toggle_editor_symbol(
        &mut self,
        command: ViewCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(shown) = self.editor_symbol_shown(command) else {
            return;
        };
        let shown = !shown;
        match command {
            ViewCommand::Whitespace => self.editor_symbols.whitespace = shown,
            ViewCommand::EndOfLine => self.editor_symbols.eol = shown,
            ViewCommand::Nonprinting => self.editor_symbols.nonprinting = shown,
            ViewCommand::ControlCharacters => self.editor_symbols.controls = shown,
            ViewCommand::AllCharacters => {
                self.editor_symbols = EditorSymbols {
                    whitespace: shown,
                    eol: shown,
                    nonprinting: shown,
                    controls: shown,
                }
            }
            _ => return,
        }
        self.editor_display_changed(window, cx);
        if let Some(store) = &self.settings_store {
            let symbols = self.editor_symbols;
            if let Err(error) = store.update_settings(|settings| {
                settings["whiteSpaceShow"] = symbols.whitespace.into();
                settings["eolShow"] = symbols.eol.into();
                settings["npcShow"] = symbols.nonprinting.into();
                settings["ccShow"] = symbols.controls.into();
                Ok(())
            }) {
                self.notify_editor_error(format!("Could not save symbol settings: {error}"), cx);
            }
        }
    }
    pub(super) fn editor_indent_guides_shown(&self) -> bool {
        self.editor_chrome.display.indent_guides
    }
    fn editor_display_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.split_sync_source = Some(self.pane);
        self.editor_chrome.layout = None;
        self.editor_chrome.drag = None;
        self.editor_painted_revision = None;
        self.editor_preferred_x = None;
        self.editor_follow_caret = true;
        self.editor_scroll_remainder = 0.;
        window.notify_text_input_changed(&self.editor_focus);
        cx.notify();
    }
    pub(super) fn set_zoom_delta(
        &mut self,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> i32 {
        let zoom = self.editor_chrome.display.zoom_delta(delta);
        self.editor_display_changed(window, cx);
        zoom
    }
    pub(super) fn reset_editor_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_chrome.display.reset_zoom();
        self.editor_display_changed(window, cx);
    }
    pub(super) fn toggle_editor_all_characters(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.toggle_editor_symbol(ViewCommand::AllCharacters, window, cx);
        self.editor_all_characters_shown()
    }
    pub(super) fn toggle_editor_indent_guides(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.editor_chrome.display.indent_guides = !self.editor_chrome.display.indent_guides;
        self.editor_display_changed(window, cx);
        self.editor_chrome.display.indent_guides
    }
    fn editor_scrollbars(&self) -> Option<EditorScrollbars> {
        let bounds = self.editor_bounds?;
        Some(EditorScrollbars::new(
            bounds,
            self.editor_line_height(),
            self.editor_visual_row_count() as f32,
            self.editor_top_line,
            self.editor_chrome.content_width,
            self.editor_horizontal_offset,
            self.editor_text_inset(),
        ))
    }

    fn scroll_editor_to(&mut self, vertical: bool, offset: f32, cx: &mut Context<Self>) {
        self.split_sync_source = Some(self.pane);
        if vertical {
            self.editor_top_line = offset.round().max(0.) as usize;
            self.editor_chrome.pending_viewport = None;
            self.editor_chrome.top_anchor = None;
        } else {
            self.editor_horizontal_offset = px(offset);
        }
        self.editor_follow_caret = false;
        self.editor_scroll_remainder = 0.;
        self.editor_painted_revision = None;
        cx.notify();
        cx.stop_propagation();
    }

    fn editor_scrollbar_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(bars) = self.editor_scrollbars() else {
            return false;
        };
        for (vertical, axis, bounds, step) in [
            (true, bars.vertical, bars.vertical_up, -1.),
            (true, bars.vertical, bars.vertical_down, 1.),
            (
                false,
                bars.horizontal,
                bars.horizontal_left,
                -self.editor_chrome.p_width(),
            ),
            (
                false,
                bars.horizontal,
                bars.horizontal_right,
                self.editor_chrome.p_width(),
            ),
        ] {
            if bounds.contains(&event.position) {
                self.scroll_editor_to(vertical, axis.step_by(step), cx);
                return true;
            }
        }
        for (vertical, axis, bounds) in [
            (true, bars.vertical, bars.vertical_bounds),
            (false, bars.horizontal, bars.horizontal_bounds),
        ] {
            if bounds.contains(&event.position) {
                let position = f32::from(if vertical {
                    event.position.y - bounds.top()
                } else {
                    event.position.x - bounds.left()
                });
                let thumb = axis.thumb();
                if position >= thumb.start && position < thumb.end {
                    self.editor_chrome.drag = Some((vertical, position - thumb.start));
                    cx.stop_propagation();
                } else {
                    self.scroll_editor_to(vertical, axis.page_toward(position), cx);
                }
                return true;
            }
        }
        false
    }

    pub(super) fn duplicate_editor(
        &mut self,
        for_line: bool,
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
        self.record_macro_step(MacroStep::Duplicate { line: for_line });
        let result = self.editor_mut().duplicate(for_line);
        self.editor_focus.focus(window, cx);
        self.editor_changed(result, window, cx);
    }

    pub(super) fn editor_changed(
        &mut self,
        result: Result<(), EditError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if result.is_ok() {
            self.split_sync_source = Some(self.pane);
            let revision = self.editor().display_revision();
            if self.macro_recording
                && !self.macro_playing
                && revision != self.editor_observed_revision
                && !self.macro_step_pending
            {
                self.record_macro_step(MacroStep::Unsupported("this editing command"));
            }
            let selection = self.editor().selection();
            if revision != self.editor_observed_revision
                || selection != self.editor_observed_selection
            {
                self.editor_chrome.pending_viewport = None;
                self.editor_follow_caret = true;
                self.editor_scroll_remainder = 0.;
                self.editor_preferred_x = None;
            }
            self.editor_observed_revision = revision;
            self.editor_observed_selection = selection;
            let selections = self.editor().selections();
            if self.editor_chrome.observed_selections.as_ref() != Some(&selections) {
                self.editor_chrome.pending_viewport = None;
                self.editor_follow_caret = true;
                self.editor_chrome.observed_selections = Some(selections);
            }
        }
        self.macro_step_pending = false;
        self.editor_error = result.err().map(|error| error.to_string());
        window.notify_text_input_changed(&self.editor_focus);
        cx.notify();
    }

    fn insert_editor(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_editing_blocked() {
            cx.stop_propagation();
            return;
        }
        let result = self.commit_editor_text(text, false);
        let inserted = result.is_ok() && !text.is_empty();
        self.editor_changed(result, window, cx);
        if inserted {
            self.on_editor_input(window, cx);
        }
        cx.stop_propagation();
    }

    pub(super) fn keyboard_indent(
        &mut self,
        forwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.batch_editing_blocked() {
            cx.stop_propagation();
            return;
        }
        self.record_macro_step(if forwards {
            MacroStep::Tab
        } else {
            MacroStep::BackTab
        });
        if forwards && self.editor().has_composition() {
            self.insert_editor("\t", window, cx);
            return;
        }
        let options = crate::text_transforms::TransformOptions {
            force_line_indent: false,
            ..Default::default()
        };
        match crate::text_transforms::session::apply_command(
            self.editor_mut(),
            if forwards { 42008 } else { 42009 },
            options,
        ) {
            Ok(()) => self.editor_changed(Ok(()), window, cx),
            Err(error) => {
                self.macro_step_pending = false;
                self.notify_editor_error(error, cx);
            }
        }
        cx.stop_propagation();
    }

    fn select_editor_ranges(
        &mut self,
        ranges: Vec<crate::editor::SelectionRange>,
        primary: usize,
    ) -> Result<(), EditError> {
        if self.editor().has_composition() {
            let selected = ranges.get(primary).ok_or(EditError::InvalidRange)?;
            self.editor_mut().select_display(selected.selection)
        } else {
            let selections = crate::editor::SelectionSet::new(ranges, primary)?;
            self.editor_mut().set_selections(selections)
        }
    }

    fn editor_display_selections(&self) -> crate::editor::SelectionSet {
        self.editor().display_selections()
    }

    fn select_editor_set(
        &mut self,
        selections: crate::editor::SelectionSet,
    ) -> Result<(), EditError> {
        if self.editor().has_composition() {
            self.editor_mut()
                .select_display(selections.primary().selection)
        } else {
            self.editor_mut().set_selections(selections)
        }
    }

    pub(super) fn move_editor(
        &mut self,
        right: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_macro_step(MacroStep::Character { right, extend });
        let selections = self.editor_display_selections();
        let moved = if selections.is_rectangular() {
            rectangle_limit(&selections, right)
        } else {
            selections.moved(extend, |_, selected| {
                let selection = selected.selection;
                let anchor = (selection.anchor, selected.anchor_virtual);
                let caret = (selection.head, selected.head_virtual);
                let (head, head_virtual) = if !extend && !selected.is_empty() {
                    if right {
                        anchor.max(caret)
                    } else {
                        anchor.min(caret)
                    }
                } else if !right && selected.head_virtual > 0 {
                    (selection.head, selected.head_virtual - 1)
                } else {
                    (
                        self.visible_editor_position(
                            self.editor().adjacent_position(selection.head, right)?,
                            right,
                        ),
                        0,
                    )
                };
                Ok(crate::editor::SelectionRange {
                    selection: Selection {
                        anchor: if extend { selection.anchor } else { head },
                        head,
                    },
                    anchor_virtual: if extend {
                        selected.anchor_virtual
                    } else {
                        head_virtual
                    },
                    head_virtual,
                })
            })
        };
        let result = moved.and_then(|moved| self.select_editor_set(moved));
        if result.is_ok() {
            self.editor_follow_caret = true;
            self.editor_preferred_x = None;
        }
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn delete_editor(
        &mut self,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.batch_editing_blocked() {
            cx.stop_propagation();
            return;
        }
        self.record_macro_step(MacroStep::Delete { right });
        let result = if right {
            self.editor_mut().delete_forward()
        } else {
            self.editor_mut().backspace()
        };
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn move_document_edge(
        &mut self,
        end: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_macro_step(MacroStep::DocumentEdge { end, extend });
        let selections = self.editor_display_selections();
        let head = ByteOffset(if end {
            self.editor().display_snapshot().len()
        } else {
            0
        });
        let mut ranges = if extend {
            selections.ranges().to_vec()
        } else {
            vec![selections.primary()]
        };
        let primary = if extend {
            selections.primary_index()
        } else {
            0
        };
        let old = ranges[primary];
        ranges[primary] = crate::editor::SelectionRange {
            selection: Selection {
                anchor: if extend { old.selection.anchor } else { head },
                head,
            },
            anchor_virtual: if extend { old.anchor_virtual } else { 0 },
            head_virtual: 0,
        };
        let result = self.select_editor_ranges(ranges, primary);
        if result.is_ok() {
            self.editor_follow_caret = true;
            self.editor_preferred_x = None;
        }
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    fn closest_editor_index_for_x(
        &self,
        shape: &EditorShapedLine,
        start: usize,
        x: Pixels,
    ) -> Result<usize, EditError> {
        let index = shape.closest_index_for_x(x);
        let end_x = shape.x_for_index(index);
        if index == 0 || index != shape.content_len() || x >= end_x {
            return Ok(index);
        }
        let mut previous = self
            .editor()
            .adjacent_position(ByteOffset(start + index), false)?
            .0;
        let mut previous_x = shape.x_for_index(previous - start);
        while previous > start && previous_x == end_x {
            previous = self
                .editor()
                .adjacent_position(ByteOffset(previous), false)?
                .0;
            previous_x = shape.x_for_index(previous - start);
        }
        if previous_x < end_x && (x - previous_x).abs() <= end_x - x {
            Ok(previous - start)
        } else {
            Ok(index)
        }
    }

    fn hit_editor(&self, position: Point<Pixels>) -> Option<ByteOffset> {
        if self.editor_painted_revision.as_ref() != Some(&self.editor().display_revision()) {
            return None;
        }
        let line = self
            .editor_lines
            .iter()
            .rev()
            .find(|line| position.y >= line.origin.y)
            .or_else(|| self.editor_lines.first())?;
        let mut byte = (line.logical_start
            + self
                .closest_editor_index_for_x(
                    &line.shape,
                    line.logical_start,
                    position.x - line.origin.x,
                )
                .ok()?)
        .clamp(line.start, line.end);
        if !line.last && byte == line.end && byte > line.start {
            byte = self
                .editor()
                .adjacent_position(ByteOffset(byte), false)
                .ok()?
                .0;
        }
        Some(ByteOffset(byte))
    }

    fn hit_editor_character(&self, position: Point<Pixels>) -> Option<ByteOffset> {
        let caret = self.hit_editor(position)?;
        let line = self
            .editor_lines
            .iter()
            .rev()
            .find(|line| position.y >= line.origin.y)
            .or_else(|| self.editor_lines.first())?;
        if caret.0 > line.start && line.origin.x + line.x_for_byte(caret.0) > position.x {
            self.editor().adjacent_position(caret, false).ok()
        } else {
            Some(caret)
        }
    }

    fn hit_editor_visual(&self, position: Point<Pixels>) -> Option<(usize, Pixels)> {
        self.hit_editor(position)?;
        let line = self
            .editor_lines
            .iter()
            .rev()
            .find(|line| position.y >= line.origin.y)
            .or_else(|| self.editor_lines.first())?;
        let row = self.editor_cached_layout()?.rows.row_for_byte(line.start);
        let bounds = self.editor_bounds?;
        Some((
            row,
            position.x - bounds.left() - self.editor_text_origin() + self.editor_horizontal_offset,
        ))
    }

    fn select_editor_rectangle_pixels(
        &mut self,
        anchor: (usize, Pixels),
        head: (usize, Pixels),
        window: &Window,
    ) -> Result<(), EditError> {
        let layout = self.editor_layout_for_width(self.editor_text_width(), window);
        if anchor.0 >= layout.rows.len() || head.0 >= layout.rows.len() {
            return Err(EditError::InvalidRange);
        }
        let space_width = self.editor_space_width(window);
        let top = anchor.0.min(head.0);
        let bottom = anchor.0.max(head.0);
        let mut ranges = Vec::new();
        for visual in &layout.rows.rows()[top..=bottom] {
            let start = layout.starts[visual.logical_line];
            let shape = &layout.shapes[visual.logical_line];
            let geometry = selection_geometry::RowGeometry {
                start: visual.range.start,
                end: visual.range.end,
                last: visual.last,
                space_width: f32::from(space_width),
            };
            let hit = |x: Pixels| {
                let offset = shape.x_for_index(visual.range.start - start) - px(visual.indent);
                let nearest = self.closest_editor_index_for_x(shape, start, x + offset)?;
                let (mut byte, virtual_space) = geometry.hit(
                    f32::from(x + offset),
                    |_| start + nearest,
                    |byte| f32::from(shape.x_for_index(byte - start)),
                );
                if !visual.last && byte == visual.range.end && byte > visual.range.start {
                    byte = self.editor().adjacent_position(ByteOffset(byte), false)?.0;
                }
                Ok::<_, EditError>((ByteOffset(byte), virtual_space))
            };
            let (anchor_position, anchor_virtual) = hit(anchor.1)?;
            let (head_position, head_virtual) = hit(head.1)?;
            ranges.push(crate::editor::SelectionRange {
                selection: Selection {
                    anchor: anchor_position,
                    head: head_position,
                },
                anchor_virtual,
                head_virtual,
            });
        }
        let selections = crate::editor::SelectionSet::rectangle(ranges, head.0 - top)?;
        self.editor_mut().set_selections(selections)
    }

    pub(super) fn move_line_edge(
        &mut self,
        end: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_editor_syntax();
        self.record_macro_step(MacroStep::LineEdge { end, extend });
        let selections = self.editor_display_selections();
        let layout = self.editor_layout_for_width(self.editor_text_width(), window);
        let moved = if selections.is_rectangular() {
            rectangle_limit(&selections, end)
        } else {
            selections.moved(extend, |_, selected| {
                let anchor_virtual = selected.anchor_virtual;
                let selected = selected.selection;
                let visual = &layout.rows.rows()[layout.rows.row_for_byte(selected.head.0)];
                let logical_start = layout.starts[visual.logical_line];
                let logical_end = logical_start + layout.shapes[visual.logical_line].content_len();
                let head = if end {
                    let visual_end = if visual.last {
                        visual.range.end
                    } else {
                        self.editor()
                            .adjacent_position(ByteOffset(visual.range.end), false)?
                            .0
                    };
                    if selected.head.0 >= visual_end {
                        logical_end
                    } else {
                        visual_end
                    }
                } else {
                    let text = self.editor().display_snapshot();
                    let raw = text.line(visual.logical_line, LineType::LF_CR).to_string();
                    let indentation = logical_start
                        + raw
                            .trim_end_matches(['\r', '\n'])
                            .char_indices()
                            .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
                            .map_or(logical_end - logical_start, |(byte, _)| byte);
                    let home = if selected.head.0 == indentation {
                        logical_start
                    } else {
                        indentation
                    };
                    if visual.range.start < selected.head.0 && visual.range.start > home {
                        visual.range.start
                    } else {
                        home
                    }
                };
                Ok(crate::editor::SelectionRange {
                    selection: Selection {
                        anchor: if extend {
                            selected.anchor
                        } else {
                            ByteOffset(head)
                        },
                        head: ByteOffset(head),
                    },
                    anchor_virtual: if extend { anchor_virtual } else { 0 },
                    head_virtual: 0,
                })
            })
        };
        let result = moved.and_then(|moved| self.select_editor_set(moved));
        if result.is_ok() {
            self.editor_follow_caret = true;
            self.editor_preferred_x = None;
        }
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn move_vertical(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_editor_syntax();
        self.record_macro_step(MacroStep::Vertical { down, extend });
        let text = self.editor().display_snapshot();
        let selected = self.editor_display_selections();
        let selections = if selected.is_rectangular() {
            match crate::editor::SelectionSet::new(vec![selected.primary()], 0) {
                Ok(selections) => selections,
                Err(error) => {
                    self.editor_changed(Err(error), window, cx);
                    cx.stop_propagation();
                    return;
                }
            }
        } else {
            selected
        };
        let layout = self.editor_layout_for_width(self.editor_text_width(), window);
        let mut primary_x = None;
        let moved = selections.moved(false, |index, selected| {
            let selection = selected.selection;
            let row = layout.rows.row_for_byte(selection.head.0);
            let current = &layout.rows.rows()[row];
            let current_shape = &layout.shapes[current.logical_line];
            let current_start = layout.starts[current.logical_line];
            let x = ((index == selections.primary_index())
                .then_some(self.editor_preferred_x)
                .flatten())
            .unwrap_or_else(|| {
                current_shape.x_for_index(
                    (selection.head.0 - current_start).min(current_shape.content_len()),
                ) - current_shape.x_for_index(current.range.start - current_start)
                    + px(current.indent)
                    + self.editor_space_width(window) * selected.head_virtual as f32
            });
            let head = if !down && row == 0 {
                ByteOffset(0)
            } else if down && row + 1 == layout.rows.len() {
                ByteOffset(text.len())
            } else {
                let target_row = if down { row + 1 } else { row - 1 };
                let target = &layout.rows.rows()[target_row];
                let shape = &layout.shapes[target.logical_line];
                let start = layout.starts[target.logical_line];
                let target_x =
                    x - px(target.indent) + shape.x_for_index(target.range.start - start);
                let mut byte = (start + self.closest_editor_index_for_x(shape, start, target_x)?)
                    .clamp(target.range.start, target.range.end);
                if !target.last && byte == target.range.end && byte > target.range.start {
                    byte = self.editor().adjacent_position(ByteOffset(byte), false)?.0;
                }
                ByteOffset(byte)
            };
            if index == selections.primary_index() {
                primary_x = Some(x);
            }
            Ok(crate::editor::SelectionRange {
                selection: Selection {
                    anchor: if extend { selection.anchor } else { head },
                    head,
                },
                anchor_virtual: if extend { selected.anchor_virtual } else { 0 },
                head_virtual: 0,
            })
        });
        let result = moved.and_then(|moved| self.select_editor_set(moved));
        let succeeded = result.is_ok();
        self.editor_changed(result, window, cx);
        if succeeded {
            self.editor_preferred_x = primary_x;
            self.editor_follow_caret = true;
        }
        cx.stop_propagation();
    }

    pub(super) fn move_editor_word(
        &mut self,
        forward: bool,
        extend: bool,
        part: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_editor_syntax();
        self.record_macro_step(MacroStep::Word {
            forward,
            extend,
            part,
        });
        let selections = self.editor_display_selections();
        let result = if selections.is_rectangular() || self.editor().has_composition() {
            if part {
                self.editor_mut().move_word_boundary(
                    forward,
                    extend,
                    crate::editor::WordBoundary::Part,
                )
            } else {
                self.editor_mut().move_word(forward, extend)
            }
        } else {
            let boundary = if part {
                crate::editor::WordBoundary::Part
            } else if forward && extend {
                crate::editor::WordBoundary::End
            } else {
                crate::editor::WordBoundary::Start
            };
            let moved = selections.moved(extend, |_, selected| {
                let head =
                    self.editor()
                        .word_boundary(selected.selection.head, forward, boundary)?;
                let head = self.visible_editor_position(head, forward);
                Ok(crate::editor::SelectionRange {
                    selection: Selection {
                        anchor: if extend {
                            selected.selection.anchor
                        } else {
                            head
                        },
                        head,
                    },
                    anchor_virtual: if extend { selected.anchor_virtual } else { 0 },
                    head_virtual: 0,
                })
            });
            moved.and_then(|moved| self.select_editor_set(moved))
        };
        if result.is_ok() {
            self.editor_follow_caret = true;
            self.editor_preferred_x = None;
        }
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    fn visible_editor_position(&self, position: ByteOffset, forward: bool) -> ByteOffset {
        let collapsed = self.editor_collapsed_headers();
        let folds = self.udl_fold_ranges();
        let text = self.editor().display_snapshot();
        let line = text.byte_to_line_idx(position.0, LineType::LF_CR);
        let containing = folds
            .iter()
            .filter(|(header, end)| collapsed.contains(header) && line > *header && line <= *end)
            .min_by_key(|(header, _)| *header);
        if let Some(&(header, end)) = containing {
            let position = if forward {
                if end + 1 < text.len_lines(LineType::LF_CR) {
                    text.line_to_byte_idx(end + 1, LineType::LF_CR)
                } else {
                    text.len()
                }
            } else {
                text.line_to_byte_idx(header, LineType::LF_CR)
                    + text
                        .line(header, LineType::LF_CR)
                        .to_string()
                        .trim_end_matches(['\r', '\n'])
                        .len()
            };
            return ByteOffset(position);
        }
        position
    }

    pub(super) fn move_editor_display_edge(
        &mut self,
        end: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_editor_syntax();
        self.record_macro_step(MacroStep::DisplayEdge { end });
        let selections = self.editor_display_selections();
        let layout = self.editor_layout_for_width(self.editor_text_width(), window);
        let moved = if selections.is_rectangular() {
            rectangle_limit(&selections, end)
        } else {
            selections.moved(false, |_, selected| {
                let row = &layout.rows.rows()[layout.rows.row_for_byte(selected.selection.head.0)];
                let head = if end {
                    if row.last {
                        ByteOffset(row.range.end)
                    } else {
                        self.editor()
                            .adjacent_position(ByteOffset(row.range.end), false)?
                    }
                } else {
                    ByteOffset(row.range.start)
                };
                Ok(Selection { anchor: head, head }.into())
            })
        };
        let result = moved.and_then(|moved| self.select_editor_set(moved));
        if result.is_ok() {
            self.editor_follow_caret = true;
            self.editor_preferred_x = None;
        }
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn delete_editor_word(
        &mut self,
        forward: bool,
        to_line_edge: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.batch_editing_blocked() {
            cx.stop_propagation();
            return;
        }
        self.record_macro_step(MacroStep::DeleteWord {
            forward,
            to_line_edge,
        });
        let result = self.editor_mut().delete_word(forward, to_line_edge);
        self.editor_changed(result, window, cx);
        cx.stop_propagation();
    }

    pub(super) fn move_editor_page(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_editor_page_selection(down, extend, false, window, cx);
    }

    pub(super) fn move_editor_page_selection(
        &mut self,
        down: bool,
        extend: bool,
        rectangle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_editor_syntax();
        self.record_macro_step(MacroStep::Page {
            down,
            extend,
            rectangle,
        });
        let result = (|| {
            let selections = self.editor_display_selections();
            let primary = selections.primary_index();
            let selection = selections.primary().selection;
            let layout = self.editor_layout_for_width(self.editor_text_width(), window);
            let row = layout.rows.row_for_byte(selection.head.0);
            let x = self.editor_preferred_x.unwrap_or_else(|| {
                let visual = &layout.rows.rows()[row];
                let start = layout.starts[visual.logical_line];
                let shape = &layout.shapes[visual.logical_line];
                shape.x_for_index(selection.head.0 - start)
                    - shape.x_for_index(visual.range.start - start)
                    + px(visual.indent)
                    + self.editor_space_width(window) * selections.primary().head_virtual as f32
            });
            let capacity = self.editor_visible_capacity();
            let distance = capacity.saturating_sub(1).max(1);
            let (target_row, top) = selection_geometry::page_rows(
                row,
                self.editor_top_line,
                layout.rows.len(),
                capacity,
                down,
            );
            let head = if !down && row < distance {
                ByteOffset(0)
            } else if down && row.saturating_add(distance) >= layout.rows.len() {
                ByteOffset(self.editor().display_snapshot().len())
            } else {
                let target = &layout.rows.rows()[target_row];
                let start = layout.starts[target.logical_line];
                let shape = &layout.shapes[target.logical_line];
                let target_x =
                    x - px(target.indent) + shape.x_for_index(target.range.start - start);
                let mut byte = (start + self.closest_editor_index_for_x(shape, start, target_x)?)
                    .clamp(target.range.start, target.range.end);
                if !target.last && byte == target.range.end && byte > target.range.start {
                    byte = self.editor().adjacent_position(ByteOffset(byte), false)?.0;
                }
                ByteOffset(byte)
            };
            let selection = Selection {
                anchor: if extend { selection.anchor } else { head },
                head,
            };
            if rectangle {
                let anchor = if selections.is_rectangular() && primary == 0 {
                    selections
                        .ranges()
                        .last()
                        .copied()
                        .unwrap_or(selections.primary())
                } else if selections.is_rectangular() {
                    selections.ranges()[0]
                } else {
                    selections.primary()
                };
                let anchor_row = layout.rows.row_for_byte(anchor.selection.anchor.0);
                let visual = &layout.rows.rows()[anchor_row];
                let start = layout.starts[visual.logical_line];
                let shape = &layout.shapes[visual.logical_line];
                let anchor_x = shape.x_for_index(anchor.selection.anchor.0 - start)
                    - shape.x_for_index(visual.range.start - start)
                    + px(visual.indent)
                    + self.editor_space_width(window) * anchor.anchor_virtual as f32;
                self.select_editor_rectangle_pixels(
                    (anchor_row, anchor_x),
                    (target_row, x),
                    window,
                )?;
            } else if extend {
                let mut ranges = selections.ranges().to_vec();
                ranges[primary] = selection.into();
                self.select_editor_ranges(ranges, primary)?;
            } else {
                self.select_editor_ranges(vec![selection.into()], 0)?;
            }
            Ok::<_, EditError>((top, x))
        })();
        match result {
            Ok((top, x)) => {
                self.editor_changed(Ok(()), window, cx);
                if self.editor_top_line != top {
                    self.editor_painted_revision = None;
                }
                self.editor_top_line = top;
                self.editor_preferred_x = Some(x);
                self.editor_follow_caret = true;
                self.editor_scroll_remainder = 0.;
                self.split_sync_source = Some(self.pane);
            }
            Err(error) => self.editor_changed(Err(error), window, cx),
        }
        cx.stop_propagation();
    }

    fn editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_mut().commit_tentative_selection();
        if self.editor_scrollbar_down(event, cx) {
            return;
        }
        if self.editor_bounds.is_some_and(|bounds| {
            event.position.y >= bounds.bottom() - px(SCROLLBAR_SIZE)
                || event.position.x >= bounds.right() - px(SCROLLBAR_SIZE)
        }) {
            return;
        }
        self.editor_focus.focus(window, cx);
        self.editor_chrome.rectangle_pixels = None;
        self.editor_chrome.selection_drag = None;
        self.editor_chrome.text_drag = None;
        self.editor_chrome.word_drag = None;
        if event.modifiers.alt {
            if let Some(head) = self.hit_editor_visual(event.position) {
                let anchor = if event.modifiers.shift {
                    let selection = self.editor().selections().primary();
                    let layout = self.editor_layout_for_width(self.editor_text_width(), window);
                    let row = layout.rows.row_for_byte(selection.selection.anchor.0);
                    let visual = &layout.rows.rows()[row];
                    let start = layout.starts[visual.logical_line];
                    let shape = &layout.shapes[visual.logical_line];
                    (
                        row,
                        shape.x_for_index(selection.selection.anchor.0 - start)
                            - shape.x_for_index(visual.range.start - start)
                            + px(visual.indent)
                            + self.editor_space_width(window) * selection.anchor_virtual as f32,
                    )
                } else {
                    head
                };
                let result = self.select_editor_rectangle_pixels(anchor, head, window);
                self.editor_chrome.rectangle_pixels = Some(anchor);
                self.editor_dragging = true;
                self.editor_changed(result, window, cx);
            }
            return;
        }
        if let Some(head) = self.hit_editor(event.position) {
            if event.modifiers.control {
                let selections = self.editor_display_selections();
                let ranges: Vec<_> = selections
                    .ranges()
                    .iter()
                    .copied()
                    .map(selection_geometry)
                    .collect();
                if let Some(index) = selection_geometry::selection_at_point(&ranges, head.0, 0) {
                    let result = if selections.ranges().len() > 1 {
                        self.editor_mut().remove_selection(index);
                        Ok(())
                    } else {
                        self.editor_mut()
                            .select_display(Selection { anchor: head, head })
                    };
                    self.editor_dragging = false;
                    self.editor_changed(result, window, cx);
                    cx.stop_propagation();
                    return;
                }
                let selected =
                    crate::editor::SelectionRange::from(Selection { anchor: head, head });
                let result = self.editor_mut().tentative_selection(selected);
                if result.is_ok() {
                    self.editor_chrome.selection_drag =
                        Some(self.editor().selections().primary_index());
                }
                self.editor_dragging = true;
                self.editor_changed(result, window, cx);
                return;
            }
            if event.click_count == 2 && event.modifiers == gpui::Modifiers::default() {
                let text = self.editor().display_text();
                if let Some(range) = crate::links::link_at(&text, head.0) {
                    let result = self
                        .editor_mut()
                        .select_display(Selection { anchor: head, head });
                    let selected = result.is_ok();
                    self.editor_dragging = false;
                    self.editor_preferred_x = None;
                    self.editor_changed(result, window, cx);
                    if selected {
                        cx.open_url(&text[range]);
                    }
                    cx.stop_propagation();
                    return;
                }
                let at = self.hit_editor_character(event.position).unwrap_or(head);
                let result = self.editor().word_selection_at(at).and_then(|selection| {
                    self.editor_mut().select_display(selection)?;
                    self.editor_chrome.word_drag = Some((selection, head));
                    Ok(())
                });
                self.editor_dragging = result.is_ok();
                self.editor_preferred_x = None;
                self.editor_changed(result, window, cx);
                cx.stop_propagation();
                return;
            }
            if !event.modifiers.shift
                && event.click_count == 1
                && !self.editor().has_composition()
                && self.editor().selections().ranges().iter().any(|selected| {
                    let range = selected.selection.range();
                    !selected.is_empty() && head >= range.start && head < range.end
                })
            {
                self.editor_chrome.text_drag = Some(drag::SelectionDrag::new(
                    f32::from(event.position.x),
                    f32::from(event.position.y),
                    head.0,
                ));
                self.editor_dragging = false;
                return;
            }
            let anchor = if event.modifiers.shift {
                self.editor().selection().anchor
            } else {
                head
            };
            let result = self.editor_mut().select_display(Selection { anchor, head });
            if result.is_ok() {
                self.editor_preferred_x = None;
            }
            self.editor_changed(result, window, cx);
            self.editor_dragging = true;
        }
    }

    fn editor_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_chrome.text_drag.is_some() {
            if let Some(head) = self.hit_editor(event.position) {
                if let Some(drag) = self.editor_chrome.text_drag.as_mut() {
                    drag.update(
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        head.0,
                    );
                }
                cx.notify();
            }
            return;
        }
        if let Some((vertical, grab)) = self.editor_chrome.drag {
            if let Some(bars) = self.editor_scrollbars() {
                let (axis, position) = if vertical {
                    (bars.vertical, event.position.y - bars.vertical_bounds.top())
                } else {
                    (
                        bars.horizontal,
                        event.position.x - bars.horizontal_bounds.left(),
                    )
                };
                self.scroll_editor_to(
                    vertical,
                    axis.offset_for_thumb(f32::from(position) - grab),
                    cx,
                );
            }
            return;
        }
        if self.editor_dragging
            && let Some(head) = self.hit_editor(event.position)
        {
            if let Some((anchored, initial)) = self.editor_chrome.word_drag {
                let result = self
                    .editor()
                    .extend_word_selection(anchored, initial, head)
                    .and_then(|selection| self.editor_mut().select_display(selection));
                self.editor_changed(result, window, cx);
                return;
            }
            if let Some(anchor) = self.editor_chrome.rectangle_pixels {
                if let Some(head) = self.hit_editor_visual(event.position) {
                    let result = self.select_editor_rectangle_pixels(anchor, head, window);
                    self.editor_changed(result, window, cx);
                }
                return;
            }
            if self.editor_chrome.selection_drag.is_some() {
                let mut selected = self.editor_display_selections().primary();
                selected.selection.head = head;
                selected.head_virtual = 0;
                let result = self.editor_mut().tentative_selection(selected);
                if result.is_ok() {
                    self.editor_chrome.selection_drag =
                        Some(self.editor_display_selections().primary_index());
                }
                self.editor_changed(result, window, cx);
                return;
            }
            let anchor = self.editor().selection().anchor;
            let result = self.editor_mut().select_display(Selection { anchor, head });
            self.editor_changed(result, window, cx);
        }
    }

    fn editor_mouse_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_chrome.selection_drag.is_some() {
            self.editor_mut().commit_tentative_selection();
        }
        if let Some(drag) = self.editor_chrome.text_drag.take() {
            if drag.drop_target().is_some() {
                let target = self
                    .editor_bounds
                    .filter(|bounds| {
                        bounds.contains(&event.position)
                            && event.position.x < bounds.right() - px(SCROLLBAR_SIZE)
                            && event.position.y < bounds.bottom() - px(SCROLLBAR_SIZE)
                    })
                    .and_then(|_| self.hit_editor(event.position));
                if !self.batch_editing_blocked()
                    && let Some(target) = target
                {
                    let result = self
                        .editor_mut()
                        .drop_selection(target, event.modifiers.control)
                        .map(|_| ());
                    self.editor_changed(result, window, cx);
                }
            } else if let Some(head) = self.hit_editor(event.position) {
                let result = self
                    .editor_mut()
                    .select_display(Selection { anchor: head, head });
                self.editor_changed(result, window, cx);
            }
        }
        self.editor_dragging = false;
        self.editor_chrome.clear_drag();
    }

    fn scroll_editor(
        &mut self,
        event: &gpui::ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = f32::from(self.editor_line_height());
        let mut pixels = event.delta.pixel_delta(px(line_height));
        if event.modifiers.shift {
            pixels = point(
                if pixels.x == px(0.) {
                    pixels.y
                } else {
                    pixels.x
                },
                px(0.),
            );
        }
        let delta = f32::from(pixels.y);
        if !delta.is_finite() || !f32::from(pixels.x).is_finite() {
            return;
        }
        if let Some(zoom) = chrome::wheel_zoom_delta(event.modifiers.control, delta) {
            self.set_zoom_delta(zoom, window, cx);
            cx.stop_propagation();
            return;
        }
        self.refresh_editor_syntax();
        let layout = self.editor_layout_for_width(self.editor_text_width(), window);
        let total = layout.rows.len();
        let restored_top = self
            .editor_chrome
            .pending_viewport
            .as_ref()
            .map(|position| {
                layout.rows.restored_top(
                    position.first_visible_line,
                    position.wrap_offset,
                    position.wrap_count,
                    self.editor_word_wrap_enabled(),
                )
            });
        self.editor_chrome.content_width = if self.editor_word_wrap_enabled() {
            (self.editor_text_width() - px(1.)).max(px(0.))
        } else {
            layout.content_width
        };
        self.editor_chrome.layout = Some(layout);
        let viewport_width = self
            .editor_bounds
            .map(|bounds| {
                (bounds.size.width
                    - self.editor_text_origin()
                    - px(TEXT_RIGHT_PADDING + SCROLLBAR_SIZE))
                .max(px(1.))
            })
            .unwrap_or(px(1.));
        let maximum = if self.editor_word_wrap_enabled() {
            px(0.)
        } else {
            (self.editor_chrome.content_width - viewport_width + px(1.)).max(px(0.))
        };
        let horizontal = if pixels.x == px(0.) {
            self.editor_horizontal_offset
        } else {
            (self.editor_horizontal_offset - pixels.x)
                .max(px(0.))
                .min(maximum)
        };
        let horizontal_changed = horizontal != self.editor_horizontal_offset;
        self.editor_horizontal_offset = horizontal;
        self.editor_scroll_remainder += delta;
        let rows = (self.editor_scroll_remainder / line_height).trunc();
        self.editor_scroll_remainder %= line_height;
        if rows == 0. && !horizontal_changed {
            return;
        }
        if rows != 0. {
            self.editor_top_line = restored_top.unwrap_or(self.editor_top_line);
            self.editor_chrome.pending_viewport = None;
            self.editor_chrome.top_anchor = None;
        }
        let first = if rows > 0. {
            self.editor_top_line.saturating_sub(rows as usize)
        } else {
            self.editor_top_line.saturating_add((-rows) as usize)
        };
        let capacity = self.editor_lines.len().max(1);
        self.editor_top_line = first.min(total.saturating_sub(capacity));
        self.split_sync_source = Some(self.pane);
        self.editor_follow_caret = false;
        self.editor_painted_revision = None;
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn render_editor(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.prepare_editor_document();
        self.refresh_editor_gutter(cx);
        let pane = self.pane;
        let revision = self.editor().display_revision();
        if self
            .editor_accessibility
            .as_ref()
            .map(|(revision, _)| revision)
            != Some(&revision)
        {
            self.editor_accessibility = Some((revision, self.editor().display_text().into()));
        }
        let value = self
            .editor_accessibility
            .as_ref()
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        self.refresh_editor_syntax();
        if self.editor_follow_caret {
            let text = self.editor().display_snapshot();
            let carets: Vec<_> = self
                .editor_display_selections()
                .ranges()
                .iter()
                .map(|selected| text.byte_to_line_idx(selected.selection.head.0, LineType::LF_CR))
                .collect();
            let containing: Vec<_> = self
                .udl_fold_ranges()
                .into_iter()
                .filter_map(|(header, end)| {
                    carets
                        .iter()
                        .any(|&caret| caret > header && caret <= end)
                        .then_some(header)
                })
                .collect();
            if let Some(markers) = self
                .editor_chrome
                .collapsed
                .get_mut(&self.workspace.active().id())
            {
                let before = markers.len();
                markers.retain(|marker| {
                    fold_marker_line(marker, &text)
                        .is_none_or(|header| !containing.contains(&header))
                });
                if markers.len() != before {
                    self.editor_chrome.layout = None;
                }
            }
        }
        div()
            .id(if pane == crate::workspace::ViewId::Primary {
                "matrix-editor"
            } else {
                "matrix-editor-secondary"
            })
            .debug_selector(move || {
                if pane == crate::workspace::ViewId::Primary {
                    "matrix-editor".into()
                } else {
                    "matrix-editor-secondary".into()
                }
            })
            .role(Role::MultilineTextInput)
            .bg(rgb(if self.dark {
                if self.workspace.active().language() == crate::syntax::Language::Json {
                    0x3f3f3f
                } else {
                    0x202020
                }
            } else {
                0xffffff
            }))
            .aria_label("Editor")
            .on_drop(cx.listener(
                move |view, drag: &super::workspace_surface::TabDrag, window, cx| {
                    view.drop_document_tab(drag, pane, None, true, window, cx);
                    cx.stop_propagation();
                },
            ))
            .aria_value(value)
            .track_focus(&self.editor_focus)
            .key_context("MatrixEditor")
            .flex_1()
            .relative()
            .min_h(px(0.))
            .min_w(px(0.))
            .w_full()
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .font_family(self.editor_font_family())
            .text_size(self.editor_font_size())
            .line_height(self.editor_line_height())
            .on_action(cx.listener(|view, _: &HomeDisplay, window, cx| {
                view.move_editor_display_edge(false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &LineEndDisplay, window, cx| {
                view.move_editor_display_edge(true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &PageUp, window, cx| {
                view.move_editor_page(false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &PageDown, window, cx| {
                view.move_editor_page(true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectPageUp, window, cx| {
                view.move_editor_page(false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectPageDown, window, cx| {
                view.move_editor_page(true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &WordLeft, window, cx| {
                view.move_editor_word(false, false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &WordRight, window, cx| {
                view.move_editor_word(true, false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectWordLeft, window, cx| {
                view.move_editor_word(false, true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectWordRight, window, cx| {
                view.move_editor_word(true, true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &WordPartLeft, window, cx| {
                view.move_editor_word(false, false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &WordPartRight, window, cx| {
                view.move_editor_word(true, false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectWordPartLeft, window, cx| {
                view.move_editor_word(false, true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectWordPartRight, window, cx| {
                view.move_editor_word(true, true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteWordLeft, window, cx| {
                view.delete_editor_word(false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteWordRight, window, cx| {
                view.delete_editor_word(true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteLineLeft, window, cx| {
                view.delete_editor_word(false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteLineRight, window, cx| {
                view.delete_editor_word(true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &MultiSelectNext, window, cx| {
                view.multi_selection_command(
                    MultiSelectionCommand::Next(crate::editor::OccurrenceOptions {
                        match_case: false,
                        whole_word: false,
                    }),
                    window,
                    cx,
                )
            }))
            .on_action(
                cx.listener(|view, _: &MultiSelectNextMatchCase, window, cx| {
                    view.multi_selection_command(
                        MultiSelectionCommand::Next(crate::editor::OccurrenceOptions {
                            match_case: true,
                            whole_word: false,
                        }),
                        window,
                        cx,
                    )
                }),
            )
            .on_action(
                cx.listener(|view, _: &MultiSelectNextWholeWord, window, cx| {
                    view.multi_selection_command(
                        MultiSelectionCommand::Next(crate::editor::OccurrenceOptions {
                            match_case: false,
                            whole_word: true,
                        }),
                        window,
                        cx,
                    )
                }),
            )
            .on_action(
                cx.listener(|view, _: &MultiSelectNextMatchCaseWholeWord, window, cx| {
                    view.multi_selection_command(
                        MultiSelectionCommand::Next(crate::editor::OccurrenceOptions {
                            match_case: true,
                            whole_word: true,
                        }),
                        window,
                        cx,
                    )
                }),
            )
            .on_action(cx.listener(|view, _: &MultiSelectAll, window, cx| {
                view.multi_selection_command(
                    MultiSelectionCommand::All(crate::editor::OccurrenceOptions {
                        match_case: false,
                        whole_word: false,
                    }),
                    window,
                    cx,
                )
            }))
            .on_action(
                cx.listener(|view, _: &MultiSelectAllMatchCase, window, cx| {
                    view.multi_selection_command(
                        MultiSelectionCommand::All(crate::editor::OccurrenceOptions {
                            match_case: true,
                            whole_word: false,
                        }),
                        window,
                        cx,
                    )
                }),
            )
            .on_action(
                cx.listener(|view, _: &MultiSelectAllWholeWord, window, cx| {
                    view.multi_selection_command(
                        MultiSelectionCommand::All(crate::editor::OccurrenceOptions {
                            match_case: false,
                            whole_word: true,
                        }),
                        window,
                        cx,
                    )
                }),
            )
            .on_action(
                cx.listener(|view, _: &MultiSelectAllMatchCaseWholeWord, window, cx| {
                    view.multi_selection_command(
                        MultiSelectionCommand::All(crate::editor::OccurrenceOptions {
                            match_case: true,
                            whole_word: true,
                        }),
                        window,
                        cx,
                    )
                }),
            )
            .on_action(cx.listener(|view, _: &MultiSelectUndo, window, cx| {
                view.multi_selection_command(MultiSelectionCommand::Undo, window, cx)
            }))
            .on_action(cx.listener(|view, _: &MultiSelectSkip, window, cx| {
                view.multi_selection_command(MultiSelectionCommand::Skip, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RotateSelection, window, cx| {
                view.multi_selection_command(MultiSelectionCommand::Rotate, window, cx)
            }))
            .on_action(cx.listener(|view, _: &SwapMainAnchorCaret, window, cx| {
                view.multi_selection_command(MultiSelectionCommand::Swap, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectangleLeft, window, cx| {
                view.extend_editor_rectangle(-1, 0, None, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectangleRight, window, cx| {
                view.extend_editor_rectangle(1, 0, None, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectangleUp, window, cx| {
                view.extend_editor_rectangle(0, -1, None, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectangleDown, window, cx| {
                view.extend_editor_rectangle(0, 1, None, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectangleHome, window, cx| {
                view.extend_editor_rectangle(0, 0, Some(false), window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectangleEnd, window, cx| {
                view.extend_editor_rectangle(0, 0, Some(true), window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectanglePageUp, window, cx| {
                view.move_editor_page_selection(false, true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &RectanglePageDown, window, cx| {
                view.move_editor_page_selection(true, true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &ShowColumnEditor, window, cx| {
                view.show_column_editor(window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &EditorCut, window, cx| view.copy_editor(true, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &EditorCopy, window, cx| view.copy_editor(false, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &EditorPaste, window, cx| view.paste_editor(window, cx)),
            )
            .on_action(cx.listener(|view, _: &EditorBackspace, window, cx| {
                view.delete_editor(false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorLineDelete, window, cx| {
                if view.batch_editing_blocked() {
                    cx.stop_propagation();
                    return;
                }
                let result = view.editor_mut().delete_current_line();
                view.record_macro_step(MacroStep::DeleteLine);
                view.editor_changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|view, _: &EditorDuplicateSelection, window, cx| {
                    view.duplicate_editor(false, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|view, _: &EditorBackspaceNotLine, window, cx| {
                view.record_macro_step(MacroStep::BackspaceNotLine);
                if view.batch_editing_blocked() {
                    cx.stop_propagation();
                    return;
                }
                let result = view.editor_mut().backspace_not_line();
                view.editor_changed(result, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &EditorDelete, window, cx| {
                view.delete_editor(true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorLeft, window, cx| {
                view.move_editor(false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorRight, window, cx| {
                view.move_editor(true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorSelectLeft, window, cx| {
                view.move_editor(false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorSelectRight, window, cx| {
                view.move_editor(true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorUp, window, cx| {
                view.move_vertical(false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorDown, window, cx| {
                view.move_vertical(true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorSelectUp, window, cx| {
                view.move_vertical(false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorSelectDown, window, cx| {
                view.move_vertical(true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorHome, window, cx| {
                view.move_line_edge(false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorEnd, window, cx| {
                view.move_line_edge(true, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorSelectHome, window, cx| {
                view.move_line_edge(false, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorSelectEnd, window, cx| {
                view.move_line_edge(true, true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorDocumentStart, window, cx| {
                view.move_document_edge(false, false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorDocumentEnd, window, cx| {
                view.move_document_edge(true, false, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &EditorSelectDocumentStart, window, cx| {
                    view.move_document_edge(false, true, window, cx)
                }),
            )
            .on_action(
                cx.listener(|view, _: &EditorSelectDocumentEnd, window, cx| {
                    view.move_document_edge(true, true, window, cx)
                }),
            )
            .on_action(cx.listener(|view, _: &EditorEnter, window, cx| {
                view.record_macro_step(MacroStep::Newline);
                let newline = view.editor().line_ending().as_str();
                view.insert_editor(newline, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &EditorTab, window, cx| {
                    view.keyboard_indent(true, window, cx)
                }),
            )
            .on_action(cx.listener(|view, _: &EditorBackTab, window, cx| {
                view.keyboard_indent(false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &EditorEscape, window, cx| {
                if view.batch_editing_blocked() {
                    cx.stop_propagation();
                    return;
                }
                let composing = view.editor().has_composition();
                view.editor_mut().cancel_composition();
                if !composing {
                    view.editor_mut().clear_additional_selections();
                }
                view.editor_changed(Ok(()), window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event, window, cx| {
                    if view.focus_editor_pane(pane, window, cx) {
                        view.editor_mouse_down(event, window, cx);
                    }
                }),
            )
            .on_mouse_move(cx.listener(move |view, event, window, cx| {
                if view.pane == pane {
                    view.editor_mouse_move(event, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    if !view.focus_editor_pane(pane, window, cx) {
                        return;
                    }
                    view.editor_focus.focus(window, cx);
                    if let Some(head) = view.hit_editor(event.position) {
                        let result = view.editor_mut().context_select(head);
                        view.editor_changed(result, window, cx);
                        view.show_editor_context_menu(event.position, window, cx);
                    }
                }),
            )
            .on_scroll(cx.listener(move |view, event, window, cx| {
                let previous = view.pane;
                if view.scope_editor_pane(pane) {
                    view.scroll_editor(event, window, cx);
                    view.scope_editor_pane(previous);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |view, event, window, cx| {
                    if view.pane == pane {
                        view.editor_mouse_up(event, window, cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |view, event, window, cx| {
                    if view.pane == pane {
                        view.editor_mouse_up(event, window, cx);
                    }
                }),
            )
            .child(EditorElement {
                view: cx.entity(),
                pane,
            })
            .child(self.render_bookmark_gutter(cx))
            .child(
                div()
                    .id("editor-scrollbar-vertical")
                    .debug_selector(move || format!("editor-scrollbar-vertical-{}", pane as usize))
                    .absolute()
                    .right(px(0.))
                    .top(px(0.))
                    .bottom(px(SCROLLBAR_SIZE))
                    .w(px(SCROLLBAR_SIZE))
                    .cursor(CursorStyle::Arrow),
            )
            .child(
                div()
                    .id("editor-scrollbar-horizontal")
                    .debug_selector(move || {
                        format!("editor-scrollbar-horizontal-{}", pane as usize)
                    })
                    .absolute()
                    .left(px(0.))
                    .right(px(0.))
                    .bottom(px(0.))
                    .h(px(SCROLLBAR_SIZE))
                    .cursor(CursorStyle::Arrow),
            )
            .when_some(self.editor_error.clone(), |editor, error| {
                editor.child(
                    div()
                        .id("editor-error")
                        .debug_selector(|| "editor-error".into())
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .absolute()
                        .bottom(px(0.))
                        .left(px(0.))
                        .w_full()
                        .bg(rgb(0xffe6e6))
                        .text_color(rgb(0x800000))
                        .child(error),
                )
            })
            .into_any_element()
    }

    pub(super) fn intelligence_caret_bounds(&self) -> Option<Bounds<Pixels>> {
        let caret = self.editor().selection().head;
        self.range_rectangles(caret..caret).into_iter().next()
    }

    fn range_rectangles(&self, range: Range<ByteOffset>) -> Vec<Bounds<Pixels>> {
        if self.editor_painted_revision.as_ref() != Some(&self.editor().display_revision()) {
            return Vec::new();
        }
        self.editor_lines
            .iter()
            .filter_map(|line| {
                if (range.is_empty() && !line.owns_caret(range.start.0))
                    || (!range.is_empty()
                        && (range.end.0 <= line.start || range.start.0 > line.end))
                {
                    return None;
                }
                let start = range.start.0.max(line.start).min(line.end);
                let end = range.end.0.min(line.end).max(line.start);
                let primary = self.editor_display_selections().primary();
                let virtual_space =
                    if range.is_empty() && range.start == primary.selection.head && line.last {
                        line.space_width * primary.head_virtual as f32
                    } else {
                        px(0.)
                    };
                Some(Bounds::new(
                    point(
                        line.origin.x + line.x_for_byte(start) + virtual_space,
                        line.origin.y,
                    ),
                    size(
                        (line.x_for_byte(end) - line.x_for_byte(start)).max(px(1.)),
                        self.editor_line_height(),
                    ),
                ))
            })
            .collect()
    }

    pub(super) fn notify_editor_error(&mut self, error: String, cx: &mut Context<Self>) {
        if self.editor_error.as_ref() != Some(&error) {
            self.editor_error = Some(error);
            cx.notify();
        }
    }

    fn native_query<T>(
        &mut self,
        result: Result<T, EditError>,
        cx: &mut Context<Self>,
    ) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                None
            }
        }
    }
}

#[cfg(all(test, feature = "native-input-tests", debug_assertions))]
mod tests {
    use super::*;

    #[gpui::test]
    fn builtin_style_and_theme_changes_retire_unchanged_text_shapes(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("<a>x</a>");
        view.update_in(cx, |view, _, _| {
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Builtin(8))
        });
        redraw_editor(cx);
        let revision = view.read_with(cx, |view, _| view.editor().display_revision());
        view.update_in(cx, |view, _, _| {
            assert!(view.editor_cached_layout().is_some());
            view.builtin.style_revision += 1;
            view.refresh_editor_syntax();
            assert_eq!(view.editor().display_revision(), revision);
            assert!(view.editor_cached_layout().is_none());
        });
        redraw_editor(cx);
        view.update_in(cx, |view, _, _| {
            assert!(view.editor_cached_layout().is_some());
            view.dark = !view.dark;
            view.refresh_editor_syntax();
            assert_eq!(view.editor().display_revision(), revision);
            assert!(view.editor_cached_layout().is_none());
        });
    }

    #[gpui::test]
    fn ctrl_click_adds_editing_caret_and_backspace_edits_both(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab\r\ncd");
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        let second = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 4..4, Bounds::default(), window, cx)
            })
            .expect("second line position");
        cx.simulate_click(
            second.center(),
            gpui::Modifiers {
                control: true,
                ..Default::default()
            },
        );
        let before = view.read_with(cx, |view, _| view.editor().selections());
        cx.simulate_input("X");
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "Xab\r\nXcd"
        );
        cx.dispatch_action(EditorBackspace);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab\r\ncd"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().selections().ranges().len()),
            2
        );
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "Xab\r\nXcd");
            assert_eq!(
                view.editor()
                    .selections()
                    .ranges()
                    .iter()
                    .map(|range| range.selection.head.0)
                    .collect::<Vec<_>>(),
                [1, 6]
            );
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "ab\r\ncd");
            assert_eq!(view.editor().selections(), before);
        });
        let fixture = tempfile::tempdir().expect("saved deletion boundaries");
        let path = fixture.path().join("boundaries.txt");
        std::fs::write(&path, "A中😀\r\nZ").expect("source deletion baseline");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        for (caret, backward, expected, after, restored) in [
            (1, false, "A😀\r\nZ", 1, 1),
            (4, false, "A中😀Z", 8, 8),
            (1, true, "中😀\r\nZ", 0, 1),
            (0, false, "中😀\r\nZ", 0, 0),
        ] {
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::set_selected_text_range(view, caret..caret, window, cx)
            });
            if backward {
                cx.dispatch_action(EditorBackspace);
            } else {
                cx.dispatch_action(EditorDelete);
            }
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), expected);
                assert_eq!(
                    view.editor().selection(),
                    Selection {
                        anchor: ByteOffset(after),
                        head: ByteOffset(after)
                    }
                );
                assert!(view.workspace.active().is_modified());
            });
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "A中😀\r\nZ");
                assert_eq!(
                    view.editor().selection(),
                    Selection {
                        anchor: ByteOffset(restored),
                        head: ByteOffset(restored)
                    }
                );
                assert!(!view.workspace.active().is_modified());
            });
        }
        for end in [false, true] {
            if end {
                cx.dispatch_action(EditorDocumentEnd);
            } else {
                cx.dispatch_action(EditorDocumentStart);
            }
            let before = view.read_with(cx, |view, _| {
                (
                    view.editor().selections(),
                    view.editor().document().can_undo(),
                    view.editor().document().can_redo(),
                )
            });
            if end {
                cx.dispatch_action(EditorDelete);
            } else {
                cx.dispatch_action(EditorBackspace);
            }
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "A中😀\r\nZ");
                assert_eq!(
                    (
                        view.editor().selections(),
                        view.editor().document().can_undo(),
                        view.editor().document().can_redo()
                    ),
                    before
                );
                assert!(!view.workspace.active().is_modified());
            });
        }
        cx.dispatch_action(NewFile);
        cx.dispatch_action(EditorBackspace);
        cx.dispatch_action(EditorDelete);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(0)
                }
            );
            assert!(!view.workspace.active().is_modified());
            assert!(!view.editor().document().can_undo() && !view.editor().document().can_redo());
        });
    }

    #[gpui::test]
    fn control_click_existing_caret_removes_it_instead_of_duplicating_typing(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab\ncd");
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        let second = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 3..3, Bounds::default(), window, cx)
            })
            .expect("toggle caret position");
        for _ in 0..2 {
            cx.simulate_click(
                second.center(),
                gpui::Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
        }
        cx.simulate_event(gpui::ModifiersChangedEvent::default());
        cx.simulate_input("X");
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "Xab\ncd"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view
                .editor()
                .display_selections()
                .ranges()
                .len()),
            1
        );
    }

    #[gpui::test]
    fn measured_split_shapes_offscreen_lines_without_enabling_wrap_or_editing(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(320.), px(480.)));
        let prefix = "short\r\n".repeat(100);
        let source = format!("{prefix}{}中😀 end", "word\t".repeat(40));
        cx.simulate_input(&source);
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        view.update_in(cx, |view, window, _| {
            let breaks = view.editor_measured_split_breaks(window);
            assert!(breaks.len() > 2);
            assert!(breaks.iter().all(|&byte| byte > prefix.len()
                && byte < source.len()
                && source.is_char_boundary(byte)));
            assert!(!view.editor_word_wrap_enabled());
            assert_eq!(view.editor().committed_text(), source);
        });
    }

    #[gpui::test]
    fn rectangle_keyboard_crosses_tabs_then_edits_short_rows(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated rectangle source");
        let path = home.path().join("rectangle.txt");
        std::fs::write(&path, "\tab\nx").expect("source bytes");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            let (file, text) =
                crate::files::DiskFile::load_unicode(&path).expect("source document");
            view.workspace.open_buffer(file, &text).expect("buffer");
            view.reset_active_view(window, cx);
        });
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(StartMacroRecording);
        cx.dispatch_action(RectangleRight);
        cx.dispatch_action(RectangleDown);
        cx.simulate_input("Z");
        cx.dispatch_action(StopMacroRecording);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "Zab\nZ"
        );
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "\tab\nx"
        );
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(PlaybackMacro);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "Zab\nZ"
        );
    }

    #[gpui::test]
    fn multi_caret_overwrite_ime_preview_and_read_only_rejection_keep_real_targets(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("😀ab\r\n中cd");
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        let second = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 6..6, Bounds::default(), window, cx)
            })
            .expect("second overwrite target");
        cx.simulate_click(
            second.center(),
            gpui::Modifiers {
                control: true,
                ..Default::default()
            },
        );
        cx.simulate_event(gpui::ModifiersChangedEvent::default());
        view.update_in(cx, |view, window, cx| {
            view.editor_mut().set_overwrite(true);
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "XY", None, window, cx);
            assert_eq!(view.editor().display_text(), "XYb\r\nXYd");
            assert_eq!(view.editor().committed_text(), "😀ab\r\n中cd");
        });
        cx.dispatch_action(EditorEscape);
        view.update_in(cx, |view, _, _| view.editor_mut().set_read_only(true));
        cx.simulate_input("Z");
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "😀ab\r\n中cd"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view
                .editor()
                .display_selections()
                .ranges()
                .len()),
            2
        );
        view.update_in(cx, |view, _, _| view.editor_mut().set_read_only(false));
        cx.simulate_input("Z");
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "Zab\r\nZcd"
        );
    }

    #[gpui::test]
    fn native_inline_ime_renders_every_mark_but_reports_primary_utf16_and_commits_atomically(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab\ncd");
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        let second = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 3..3, Bounds::default(), window, cx)
            })
            .expect("second caret");
        cx.simulate_click(
            second.center(),
            gpui::Modifiers {
                control: true,
                ..Default::default()
            },
        );
        cx.simulate_event(gpui::ModifiersChangedEvent::default());
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(
                view,
                None,
                "拼😀",
                Some(1..3),
                window,
                cx,
            );
            assert_eq!(view.editor().committed_text(), "ab\ncd");
            assert_eq!(view.editor().display_text(), "拼😀ab\n拼😀cd");
            assert_eq!(
                EntityInputHandler::marked_text_range(view, window, cx),
                Some(6..9)
            );
            assert_eq!(
                EntityInputHandler::selected_text_range(view, false, window, cx)
                    .map(|selection| selection.range),
                Some(7..9)
            );
            let runs = view.styled_editor_runs(
                0,
                view.editor().display_snapshot().len(),
                None,
                TextRun {
                    len: view.editor().display_snapshot().len(),
                    font: gpui::font(view.editor_font_family()),
                    color: rgb(0).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                },
            );
            assert_eq!(
                runs.iter()
                    .filter(|run| run.underline.is_some())
                    .map(|run| run.len)
                    .sum::<usize>(),
                14
            );
        });
        redraw_editor(cx);
        view.update_in(cx, |view, window, cx| {
            let first =
                EntityInputHandler::bounds_for_range(view, 0..3, Bounds::default(), window, cx)
                    .expect("first preview geometry");
            let second =
                EntityInputHandler::bounds_for_range(view, 6..9, Bounds::default(), window, cx)
                    .expect("primary preview geometry");
            assert!(second.top() > first.top());
            EntityInputHandler::replace_text_in_range(view, None, "文", window, cx);
            assert_eq!(view.editor().committed_text(), "文ab\n文cd");
        });
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab\ncd"
        );
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "中", None, window, cx)
        });
        cx.dispatch_action(EditorEscape);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab\ncd"
        );
    }

    #[gpui::test]
    fn native_explicit_utf16_ime_range_retargets_primary_and_partial_updates_replicate(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A😀B\nC😀D");
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        let second = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 5..5, Bounds::default(), window, cx)
            })
            .expect("second line caret");
        cx.simulate_click(
            second.center(),
            gpui::Modifiers {
                control: true,
                ..Default::default()
            },
        );
        cx.simulate_event(gpui::ModifiersChangedEvent::default());
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(
                view,
                Some(6..8),
                "拼😀",
                None,
                window,
                cx,
            );
            assert_eq!(view.editor().display_text(), "拼😀A😀B\nC拼😀D");
            assert_eq!(
                EntityInputHandler::marked_text_range(view, window, cx),
                Some(9..12)
            );
            EntityInputHandler::replace_and_mark_text_in_range(
                view,
                Some(10..12),
                "文",
                None,
                window,
                cx,
            );
            assert_eq!(view.editor().display_text(), "拼文A😀B\nC拼文D");
            assert_eq!(
                EntityInputHandler::marked_text_range(view, window, cx),
                Some(9..10)
            );
            let selected = EntityInputHandler::selected_text_range(view, false, window, cx)
                .expect("updated preedit selection");
            assert_eq!(selected.range, 10..10);
            assert!(!selected.reversed);
            EntityInputHandler::unmark_text(view, window, cx);
            assert_eq!(view.editor().committed_text(), "拼文A😀B\nC拼文D");
        });
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "A😀B\nC😀D"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view
                .editor()
                .display_selections()
                .ranges()
                .len()),
            2
        );
    }

    #[gpui::test]
    fn multi_selection_actions_keep_source_duplicate_and_edit_every_match(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("one ONE stone one");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(StartMacroRecording);
        cx.dispatch_action(MultiSelectAllMatchCaseWholeWord);
        cx.simulate_input("X");
        cx.dispatch_action(StopMacroRecording);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "X ONE stone X"
        );
        cx.dispatch_action(EditorUndo);
        view.update_in(cx, |view, _, _| {
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(3),
                })
                .expect("one target selection")
        });
        cx.dispatch_action(PlaybackMacro);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "X ONE stone X"
        );
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(MultiSelectUndo);
        cx.dispatch_action(EditorDuplicateSelection);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "oneone ONE stone one"
        );
    }

    #[gpui::test]
    fn selection_drag_moves_or_control_copies_and_is_atomic(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab cd");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorSelectRight);
        redraw_editor(cx);
        let (start, end) = view.update_in(cx, |view, window, cx| {
            let start =
                EntityInputHandler::bounds_for_range(view, 1..1, Bounds::default(), window, cx)
                    .expect("drag source");
            let end =
                EntityInputHandler::bounds_for_range(view, 5..5, Bounds::default(), window, cx)
                    .expect("drag destination");
            (start.center(), end.center())
        });
        cx.simulate_event(gpui::MouseDownEvent {
            position: start,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: end,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: gpui::Modifiers::default(),
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            " cdab"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(3),
                    head: ByteOffset(5)
                }
            )
        });
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab cd"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(2)
                }
            )
        });
        redraw_editor(cx);
        cx.simulate_event(gpui::MouseDownEvent {
            position: start,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: end,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: gpui::Modifiers {
                control: true,
                ..Default::default()
            },
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab cdab"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(5),
                    head: ByteOffset(7)
                }
            )
        });
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            "ab cd"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(2)
                }
            )
        });
    }

    #[gpui::test]
    fn alt_rectangle_preserves_virtual_space_for_real_typing(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("abcd\nx");
        cx.dispatch_action(EditorDocumentStart);
        redraw_editor(cx);
        let (anchor, target) = view.update_in(cx, |view, window, cx| {
            let first =
                EntityInputHandler::bounds_for_range(view, 3..3, Bounds::default(), window, cx)
                    .expect("first row");
            let last =
                EntityInputHandler::bounds_for_range(view, 6..6, Bounds::default(), window, cx)
                    .expect("last row");
            (first.center(), point(first.center().x, last.center().y))
        });
        let modifiers = gpui::Modifiers {
            alt: true,
            ..Default::default()
        };
        cx.simulate_event(gpui::MouseDownEvent {
            position: anchor,
            button: MouseButton::Left,
            modifiers,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: target,
            pressed_button: Some(MouseButton::Left),
            modifiers,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: target,
            button: MouseButton::Left,
            modifiers,
            click_count: 1,
        });
        cx.simulate_input("Z");
        let text = view.read_with(cx, |view, _| view.editor().committed_text());
        assert!(text.starts_with("abcZd\nx"), "actual={text:?}");
        assert!(text.ends_with('Z'));
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().selections().ranges().len()),
            2
        );
    }

    #[gpui::test]
    fn editor_control_wheel_zooms_without_scrolling_or_editing_and_keeps_horizontal_wheel(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(
                view,
                None,
                &format!("{}\n{}", "wide ".repeat(100), "line\n".repeat(100)),
                window,
                cx,
            );
        });
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        let editor = cx.debug_bounds("matrix-editor").expect("wheel viewport");
        let before = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("original geometry");
        let position = editor.origin + point(px(80.), px(15.));
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(100.))),
            modifiers: gpui::Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        });
        redraw_editor(cx);
        let after = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("Control-wheel geometry");
        assert!(after.size.height > before.size.height);
        let (caret, selection) = view.update_in(cx, |view, window, cx| {
            let caret =
                EntityInputHandler::bounds_for_range(view, 1..1, Bounds::default(), window, cx)
                    .expect("zoomed native caret");
            let selection =
                EntityInputHandler::bounds_for_range(view, 0..1, Bounds::default(), window, cx)
                    .expect("zoomed native selection");
            assert_eq!(
                EntityInputHandler::character_index_for_point(view, caret.center(), window, cx),
                Some(1)
            );
            (caret, selection)
        });
        cx.update(|window, _| {
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            let caret_colour: gpui::Background = rgb(0x8000ff).into();
            let selection_colour: gpui::Background = rgb(0xc0c0c0).into();
            assert!(
                quads.iter().any(
                    |quad| quad.bounds == window.pixel_snap_bounds(caret).scale(scale) && quad.background == caret_colour
                ),
                "zoomed painted caret matches native input geometry: native={caret:?}, painted={:?}",
                quads.iter().filter(|quad| quad.background == caret_colour).map(|quad| quad.bounds).collect::<Vec<_>>()
            );
            assert!(
                quads
                    .iter()
                    .any(|quad| quad.bounds == window.pixel_snap_bounds(selection).scale(scale)
                        && quad.background == selection_colour),
                "zoomed painted selection matches native input geometry"
            );
        });
        assert_eq!(view.read_with(cx, |view, _| view.editor_top_line), 0);
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(-100.), px(0.))),
            modifiers: gpui::Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(view.read_with(cx, |view, _| view.editor_horizontal_offset) > px(0.));
        let horizontal = view.read_with(cx, |view, _| view.editor_horizontal_offset);
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-100.))),
            modifiers: gpui::Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        });
        redraw_editor(cx);
        assert!(view.read_with(cx, |view, _| view.editor_horizontal_offset) > horizontal);
        let shifted = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("Shift-Control wheel keeps text size");
        assert_eq!(shifted.size.height, after.size.height);
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-100.))),
            ..Default::default()
        });
        assert!(view.read_with(cx, |view, _| view.editor_top_line) > 0);
        let selection = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("wheel preserves source selection");
        assert_eq!(selection.range, 0..1);
        cx.dispatch_action(EditorRight);
        redraw_editor(cx);
        view.update_in(cx, |view, window, cx| {
            let selection = EntityInputHandler::selected_text_range(view, false, window, cx)
                .expect("ordinary motion collapses selection");
            assert_eq!(selection.range, 1..1);
            let caret =
                EntityInputHandler::bounds_for_range(view, 1..1, Bounds::default(), window, cx)
                    .expect("revealed native caret");
            assert!(
                caret.left() >= editor.left() + view.editor_text_origin()
                    && caret.right() <= editor.right() - px(16.)
            );
            assert!(caret.top() >= editor.top() && caret.bottom() <= editor.bottom() - px(16.));
            assert_eq!(
                EntityInputHandler::character_index_for_point(view, caret.center(), window, cx),
                Some(1)
            );
        });
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(0),
            "zoom and scroll add no Undo transaction"
        );
    }

    #[gpui::test]
    fn editor_url_opens_only_on_unmodified_double_click_through_wrapped_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(320.), px(480.)));
        let url = "https://example.test/long-path/to/a/wrapped/resource";
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(
                view,
                None,
                &format!("prefix {url} suffix"),
                window,
                cx,
            );
        });
        cx.dispatch_action(EditorDocumentStart);
        view.update_in(cx, |view, window, cx| {
            view.toggle_editor_word_wrap(window, cx);
        });
        redraw_editor(cx);
        let bounds = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 40..40, Bounds::default(), window, cx)
            })
            .expect("URL continuation hit position");
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        assert_eq!(cx.opened_url(), None, "single click never launches");
        for modifiers in [
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
            gpui::Modifiers {
                control: true,
                ..Default::default()
            },
            gpui::Modifiers {
                alt: true,
                ..Default::default()
            },
            gpui::Modifiers {
                platform: true,
                ..Default::default()
            },
            gpui::Modifiers {
                function: true,
                ..Default::default()
            },
        ] {
            cx.simulate_event(gpui::MouseDownEvent {
                position: bounds.center(),
                button: MouseButton::Left,
                click_count: 2,
                modifiers,
                ..Default::default()
            });
            cx.simulate_event(gpui::MouseUpEvent {
                position: bounds.center(),
                button: MouseButton::Left,
                click_count: 2,
                modifiers,
            });
            assert_eq!(
                cx.opened_url(),
                None,
                "modified double click never launches"
            );
        }
        cx.simulate_event(gpui::MouseDownEvent {
            position: bounds.center(),
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: bounds.center(),
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        assert_eq!(
            cx.opened_url().as_deref(),
            Some(url),
            "TestPlatform records, never launches a browser"
        );
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("collapsed URL click selection");
        assert_eq!(selected.range, 40..40);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(0),
            "opening a URL adds no text history"
        );
    }

    fn redraw_editor(cx: &mut gpui::VisualTestContext) {
        for _ in 0..2 {
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
        }
    }

    #[gpui::test]
    fn editor_word_wrap_shares_native_geometry_navigation_and_hit_testing(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(320.), px(480.)));
        let source = format!("{}中😀 end\r\nlast", "word ".repeat(12));
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, &source, window, cx);
        });
        cx.dispatch_action(EditorDocumentStart);
        view.update_in(cx, |view, window, cx| {
            assert!(view.toggle_editor_word_wrap(window, cx))
        });
        redraw_editor(cx);
        let editor = cx.debug_bounds("matrix-editor").expect("wrapped editor");
        let first = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("first wrapped row");
        let wrapped = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 35..35, Bounds::default(), window, cx)
            })
            .expect("continuation native geometry");
        assert!(wrapped.top() > first.top());
        assert!(
            wrapped.left() >= editor.left() + px(TEXT_LEFT) && wrapped.right() < editor.right()
        );
        cx.simulate_click(wrapped.center(), gpui::Modifiers::default());
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("hit on a continuation");
        assert_eq!(selected.range, 35..35);
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorDown);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("visual Down");
        assert!(
            selected.range.start > 0 && selected.range.start < 60,
            "Down must visit the next visual row, not the next logical line"
        );
        cx.dispatch_action(EditorUp);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("visual Up");
        assert_eq!(selected.range, 0..0);
        cx.dispatch_action(EditorEnd);
        let visual_end = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("End of first visual row");
        assert!(visual_end.range.start > 0 && visual_end.range.start < 60);
        cx.dispatch_action(EditorEnd);
        let logical_end = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("repeated End reaches logical EOL");
        assert_eq!(logical_end.range, 67..67);
        let rectangles = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::selection_bounds_for_range(
                view,
                0..60,
                Bounds::default(),
                window,
                cx,
            )
        });
        assert!(rectangles.len() > 1, "selection spans actual visual rows");
        view.update_in(cx, |view, window, cx| {
            assert!(!view.toggle_editor_word_wrap(window, cx))
        });
        redraw_editor(cx);
        cx.debug_bounds("matrix-editor")
            .expect("unwrapped editor redraw");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(
                view,
                0..source.encode_utf16().count(),
                &mut actual,
                window,
                cx,
            )
        });
        assert_eq!(text.as_deref(), Some(source.as_str()));
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(0),
            "wrap/layout/navigation add no text history"
        );
    }

    #[gpui::test]
    fn editor_word_wrap_scrolls_visual_rows_and_exposes_matching_gutter_rows(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(320.), px(480.)));
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(
                view,
                None,
                &format!("{}\nlast", "word ".repeat(300)),
                window,
                cx,
            );
        });
        cx.dispatch_action(EditorDocumentStart);
        view.update_in(cx, |view, window, cx| {
            view.toggle_editor_word_wrap(window, cx);
        });
        redraw_editor(cx);
        let editor = cx.debug_bounds("matrix-editor").expect("wrap viewport");
        assert_eq!(
            view.read_with(cx, |view, _| view
                .editor_visible_gutter_rows()
                .first()
                .copied()),
            Some((0, true))
        );
        cx.simulate_event(gpui::ScrollEvent {
            position: editor.origin + point(px(80.), px(15.)),
            delta: gpui::ScrollDelta::Pixels(point(px(-100.), px(-100.))),
            ..Default::default()
        });
        assert_eq!(
            view.read_with(cx, |view, _| view
                .editor_visible_gutter_rows()
                .first()
                .copied()),
            Some((0, false))
        );
        let before = view.read_with(cx, |view, _| view.editor_top_line);
        cx.simulate_click(
            point(editor.right() - px(7.), editor.bottom() - px(35.)),
            gpui::Modifiers::default(),
        );
        assert!(view.read_with(cx, |view, _| view.editor_top_line) > before);
        let selection = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("scroll leaves selection alone");
        assert_eq!(selection.range, 0..0);
        assert_eq!(
            view.read_with(cx, |view, _| view.editor_horizontal_offset),
            px(0.)
        );
        view.update_in(cx, |view, window, cx| {
            view.toggle_editor_word_wrap(window, cx);
        });
        redraw_editor(cx);
        cx.debug_bounds("matrix-editor")
            .expect("unwrapped anchor redraw");
        assert_eq!(
            view.read_with(cx, |view, _| view.editor_top_line),
            0,
            "unwrapping retains the top source line, not a stale visual index"
        );
    }

    #[gpui::test]
    fn editor_word_wrap_reflows_active_ime_geometry_on_resize_without_committing(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(400.), px(480.)));
        let source = "word ".repeat(30);
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, &source, window, cx);
        });
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(
                view, None, "拼😀", None, window, cx,
            );
            view.toggle_editor_word_wrap(window, cx);
        });
        redraw_editor(cx);
        cx.debug_bounds("matrix-editor")
            .expect("wrapped composition redraw");
        let before = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::marked_text_range(view, window, cx)
        });
        cx.simulate_resize(size(px(280.), px(480.)));
        redraw_editor(cx);
        let editor = cx
            .debug_bounds("matrix-editor")
            .expect("narrow wrap viewport");
        let after = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::marked_text_range(view, window, cx)
        });
        assert_eq!(after, before);
        assert_eq!(after, Some(150..153));
        let caret = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 153..153, Bounds::default(), window, cx)
            })
            .expect("composition caret after resize");
        assert!(caret.left() >= editor.left() + px(TEXT_LEFT) && caret.right() < editor.right());
        assert!(caret.top() >= editor.top() && caret.bottom() < editor.bottom());
        let hit = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::character_index_for_point(view, caret.center(), window, cx)
        });
        assert_eq!(hit, Some(153));
        assert_eq!(
            view.read_with(cx, |view, _| view.editor().committed_text()),
            source,
            "layout must not commit provisional IME text"
        );
    }

    fn floating_context(
        cx: &mut gpui::VisualTestContext,
        selector: &'static str,
    ) -> gpui::VisualTestContext {
        cx.run_until_parked();
        for handle in cx.windows() {
            let mut candidate = gpui::VisualTestContext::from_window(handle, &cx.cx);
            if candidate.debug_bounds("matrix-editor").is_none()
                && candidate.debug_bounds(selector).is_some()
            {
                return candidate;
            }
        }
        panic!("expected independent dialog containing {selector}");
    }

    fn floating_bounds(
        cx: &mut gpui::VisualTestContext,
        selector: &'static str,
    ) -> Option<Bounds<Pixels>> {
        cx.run_until_parked();
        cx.windows().into_iter().find_map(|handle| {
            let mut candidate = gpui::VisualTestContext::from_window(handle, &cx.cx);
            if candidate.debug_bounds("matrix-editor").is_some() {
                return None;
            }
            candidate.debug_bounds(selector)
        })
    }

    #[gpui::test]
    fn native_committed_typing_honors_overwrite_but_explicit_ranges_still_insert(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("abcd");
        cx.dispatch_action(EditorDocumentStart);
        let mode = cx
            .debug_bounds("status-insert-mode")
            .expect("INS/OVR status");
        cx.simulate_click(mode.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert!(
                view.editor().overwrite(),
                "status mode toggles on single click"
            );
            assert_eq!(view.editor().committed_text(), "abcd");
        });
        cx.simulate_input("X");
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, Some(1..1), "Y", window, cx);
        });
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("XYbcd"));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("Xbcd"));
        cx.simulate_click(mode.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| assert!(!view.editor().overwrite()));
        cx.simulate_input("Z");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "XZbcd")
        });
    }

    #[gpui::test]
    fn editor_display_commands_update_native_geometry_without_document_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let source = "    \t中\0\r\nnext";
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, source, window, cx);
        });
        cx.dispatch_action(EditorDocumentStart);
        let selection = view.read_with(cx, |view, _| view.editor().selections());
        let wrap = view.read_with(cx, |view, _| view.editor_word_wrap_enabled());
        let before = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("original native geometry");
        for _ in 0..3 {
            let button = cx.debug_bounds("toolbar-zoom-in").expect("Zoom In toolbar");
            cx.simulate_click(button.center(), gpui::Modifiers::default());
        }
        for selector in [
            "toolbar-all-characters",
            "toolbar-indent-guides",
            "toolbar-word-wrap",
        ] {
            let button = cx.debug_bounds(selector).expect("display toolbar");
            cx.simulate_click(button.center(), gpui::Modifiers::default());
        }
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor_zoom_delta(), 3);
            assert!(view.editor_all_characters_shown());
            assert!(!view.editor_indent_guides_shown());
            assert_eq!(view.editor_word_wrap_enabled(), !wrap);
            assert_eq!(view.editor().selections(), selection);
        });
        redraw_editor(cx);
        let after = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("zoomed native geometry");
        assert!(after.size.height > before.size.height);
        let mut actual = None;
        let observed = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(
                view,
                0..source.encode_utf16().count(),
                &mut actual,
                window,
                cx,
            )
        });
        assert_eq!(observed.as_deref(), Some(source));
        for _ in 0..3 {
            let button = cx
                .debug_bounds("toolbar-zoom-out")
                .expect("Zoom Out toolbar");
            cx.simulate_click(button.center(), gpui::Modifiers::default());
        }
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor_zoom_delta(), 0);
            assert_eq!(view.editor().selections(), selection);
        });
        redraw_editor(cx);
        let reset = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("reset geometry");
        assert_eq!(reset.size.height, before.size.height);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(0),
            "display commands add no history"
        );
    }

    #[gpui::test]
    fn editor_display_projection_preserves_active_ime_marked_range(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("\t中\0");
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(
                view, None, "拼😀", None, window, cx,
            );
        });
        let before = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::marked_text_range(view, window, cx)
        });
        view.update_in(cx, |view, window, cx| {
            view.toggle_editor_all_characters(window, cx);
            view.toggle_editor_indent_guides(window, cx);
            view.set_zoom_delta(1, window, cx);
        });
        redraw_editor(cx);
        let after = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::marked_text_range(view, window, cx)
        });
        assert_eq!(after, before);
        assert_eq!(after, Some(3..6));
        let bounds = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 6..6, Bounds::default(), window, cx)
            })
            .expect("projected composition caret");
        assert!(bounds.size.width > px(0.));
    }

    #[gpui::test]
    fn editor_double_click_selects_word_and_drag_extends_by_source_word_boundaries(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            *view.editor_mut() = EditorSession::new("alpha beta 中文😀\r\nnext");
            view.editor_changed(Ok(()), window, cx);
        });
        cx.debug_bounds("matrix-editor").expect("painted editor");
        let (word, earlier) = view.read_with(cx, |view, _| {
            let line = &view.editor_lines[0];
            let y = line.origin.y + view.editor_line_height() / 2.;
            (
                point(line.origin.x + line.x_for_byte(8) + px(1.), y),
                point(line.origin.x + line.x_for_byte(2) + px(1.), y),
            )
        });
        cx.simulate_event(MouseDownEvent {
            position: word,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(6)..ByteOffset(10)
            )
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: word,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection().range(),
                ByteOffset(6)..ByteOffset(10)
            )
        });
        cx.simulate_event(MouseDownEvent {
            position: word,
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(MouseMoveEvent {
            position: earlier,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: earlier,
            button: MouseButton::Left,
            ..Default::default()
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(10),
                    head: ByteOffset(0)
                }
            );
            assert_eq!(view.editor().committed_text(), "alpha beta 中文😀\r\nnext");
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn editor_scrollbar_track_and_thumb_preserve_native_selection_and_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(
                view,
                None,
                &format!("{}\n{}", "wide ".repeat(300), "line\n".repeat(200)),
                window,
                cx,
            );
        });
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("native selection before scrolling");
        let editor = cx.debug_bounds("matrix-editor").expect("viewport");
        let track = point(editor.right() - px(7.), editor.bottom() - px(35.));
        assert!(
            cx.debug_bounds("editor-scrollbar-vertical-0")
                .expect("vertical arrow cursor region")
                .contains(&track)
        );
        assert!(
            cx.debug_bounds("editor-scrollbar-horizontal-0")
                .expect("horizontal arrow cursor region")
                .contains(&point(editor.right() - px(35.), editor.bottom() - px(7.)))
        );
        cx.simulate_click(track, gpui::Modifiers::default());
        assert!(view.read_with(cx, |view, _| view.editor_top_line) > 0);
        let thumb = view.read_with(cx, |view, _| {
            let bar = view
                .editor_scrollbars()
                .expect("painted scrollbar geometry");
            let range = bar.vertical.thumb();
            point(
                bar.vertical_bounds.center().x,
                bar.vertical_bounds.top() + px((range.start + range.end) / 2.),
            )
        });
        cx.simulate_event(gpui::MouseDownEvent {
            position: thumb,
            button: MouseButton::Left,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseMoveEvent {
            position: point(thumb.x, editor.bottom() - px(16.)),
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: track,
            button: MouseButton::Left,
            ..Default::default()
        });
        assert!(view.read_with(cx, |view, _| view.editor_top_line) > 100);
        cx.simulate_click(
            point(editor.right() - px(35.), editor.bottom() - px(7.)),
            gpui::Modifiers::default(),
        );
        assert!(
            view.read_with(cx, |view, _| view.editor_horizontal_offset) > px(0.),
            "horizontal range retains a wide line after it leaves the viewport"
        );
        let after = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("native selection after scrolling");
        assert_eq!(after.range, selected.range);
        assert_eq!(after.reversed, selected.reversed);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(0),
            "scrolling adds no Undo transaction"
        );
    }

    #[test]
    fn editor_current_line_source_colour_reaches_offscreen_renderer() {
        for dark in [false, true] {
            let mut app = gpui::HeadlessAppContext::with_platform(
                std::sync::Arc::new(gpui::NoopTextSystem),
                std::sync::Arc::new(()),
                gpui_platform::current_headless_renderer,
            );
            let handle = app
                .open_window(size(px(640.), px(480.)), |window, cx| {
                    cx.new(|cx| {
                        let mut view = MatrixWindow::new(dark, window, cx);
                        *view.editor_mut() = EditorSession::new("        one\n        two");
                        view
                    })
                })
                .expect("offscreen editor");
            let draw = |app: &mut gpui::HeadlessAppContext| {
                for _ in 0..3 {
                    app.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                        .expect("draw source chrome");
                    app.run_until_parked();
                }
            };
            let count = |app: &mut gpui::HeadlessAppContext, colour, height: f32, wide: bool| {
                app.update_window(handle.into(), |_, window, _| {
                    let unit = Bounds::new(point(px(72.), px(104.)), size(px(1.), px(height)))
                        .scale(window.scale_factor());
                    let background: gpui::Background = rgb(colour).into();
                    window
                        .painted_quads()
                        .iter()
                        .filter(|quad| {
                            quad.background == background
                                && quad.bounds.origin.x >= unit.origin.x
                                && quad.bounds.origin.y >= unit.origin.y
                                && quad.bounds.right() <= px(624.).scale(window.scale_factor())
                                && quad.bounds.bottom() < px(440.).scale(window.scale_factor())
                                && quad.bounds.size.height == unit.size.height
                                && (if wide {
                                    quad.bounds.size.width > unit.size.width
                                } else {
                                    quad.bounds.size.width == unit.size.width
                                })
                                && quad.content_mask.bounds.contains(&quad.bounds.center())
                        })
                        .count()
                })
                .expect("observe actual editor chrome quads")
            };
            draw(&mut app);
            assert!(
                count(&mut app, if dark { 0x8faf9f } else { 0x8000ff }, 26., false) > 0,
                "source primary caret, dark={dark}"
            );
            assert!(
                count(&mut app, if dark { 0x4f5f5f } else { 0xc0c0c0 }, 1., false) > 0,
                "source indent guides, dark={dark}"
            );
            assert!(
                count(&mut app, if dark { 0x101010 } else { 0xe8e8ff }, 26., true) > 0,
                "source current line, dark={dark}"
            );
            if !dark {
                app.update_window(handle.into(), |_, window, cx| {
                    window.draw(cx).clear(cx);
                })
                .expect("draw current line");
                let image = app.capture_screenshot(handle.into()).expect("own scene");
                assert!(
                    image
                        .pixels()
                        .any(|pixel| pixel.0[..3] == [0xe8, 0xe8, 0xff])
                );
            }
            app.update_window(handle.into(), |_, window, cx| {
                for _ in 0..8 {
                    window.dispatch_action(Box::new(EditorSelectRight), cx);
                }
            })
            .expect("select eight spaces through normal actions");
            draw(&mut app);
            assert!(
                count(&mut app, if dark { 0x585858 } else { 0xc0c0c0 }, 26., true) > 0,
                "source opaque primary selection, dark={dark}"
            );
            app.update_window(handle.into(), |_, window, cx| {
                window.dispatch_action(Box::new(MultiSelectNextMatchCase), cx)
            })
            .expect("add matching indent selection");
            draw(&mut app);
            assert!(count(&mut app, if dark { 0x8faf9f } else { 0x8000ff }, 26., false) > 0);
            assert!(
                count(&mut app, if dark { 0x808040 } else { 0x404040 }, 26., false) > 0,
                "source additional caret, dark={dark}"
            );
            assert!(
                count(&mut app, 0xc0c0c0, 26., true) >= if dark { 1 } else { 2 },
                "source additional selection, dark={dark}"
            );
            handle
                .read_with(&app, |view, _| {
                    assert_eq!(view.editor().committed_text(), "        one\n        two");
                    assert_eq!(view.editor().selections().ranges().len(), 2);
                    assert!(
                        !view.editor().document().can_undo()
                            && !view.workspace.active().is_modified()
                    );
                })
                .expect("chrome input changes selections only");
            let (geometry, guide) = handle
                .update(&mut app, |view, window, cx| {
                    assert_eq!(view.editor().selections().primary_index(), 1);
                    assert_eq!(
                        view.editor()
                            .selections()
                            .ranges()
                            .iter()
                            .map(|range| (range.selection.anchor.0, range.selection.head.0))
                            .collect::<Vec<_>>(),
                        [(0, 8), (12, 20)]
                    );
                    let mut geometry = Vec::new();
                    for (range, colour) in [
                        (20..20, if dark { 0x8faf9f } else { 0x8000ff }),
                        (8..8, if dark { 0x808040 } else { 0x404040 }),
                        (12..20, if dark { 0x585858 } else { 0xc0c0c0 }),
                        (0..8, 0xc0c0c0),
                    ] {
                        let bounds = EntityInputHandler::bounds_for_range(
                            view,
                            range.clone(),
                            Bounds::default(),
                            window,
                            cx,
                        )
                        .expect("source native selection geometry");
                        if range.is_empty() {
                            assert_eq!(
                                EntityInputHandler::character_index_for_point(
                                    view,
                                    bounds.center(),
                                    window,
                                    cx
                                ),
                                Some(range.start)
                            );
                        }
                        geometry.push((bounds, colour));
                    }
                    let guide = EntityInputHandler::bounds_for_range(
                        view,
                        4..4,
                        Bounds::default(),
                        window,
                        cx,
                    )
                    .expect("column four source guide");
                    (geometry, guide)
                })
                .expect("native multi-selection geometry");
            app.update_window(handle.into(), |_, window, _| {
                let scale = window.scale_factor();
                let quads = window.painted_quads();
                for (bounds, colour) in geometry {
                    let bounds = window.pixel_snap_bounds(bounds).scale(scale);
                    let fill: gpui::Background = rgb(colour).into();
                    assert!(
                        quads.iter().any(|quad| quad.bounds == bounds
                            && quad.background == fill
                            && quad.content_mask.bounds.contains(&bounds.center())),
                        "source active-selection artwork and native rectangles agree, dark={dark}"
                    );
                }
                let guide = window.pixel_snap_bounds(guide).scale(scale);
                let fill: gpui::Background = rgb(if dark { 0x4f5f5f } else { 0xc0c0c0 }).into();
                assert!(
                    quads.iter().any(|quad| quad.background == fill
                        && quad.bounds.left() == guide.left()
                        && quad.bounds.top() >= guide.top()
                        && quad.bounds.bottom() <= guide.bottom()
                        && quad.bounds.size.width == px(1.).scale(scale)
                        && quad.bounds.size.height == px(1.).scale(scale)),
                    "guide follows source column-four text geometry"
                );
            })
            .expect("actual painted primary/additional geometry");
        }
    }

    #[test]
    fn language_json_error_background_is_painted_in_the_offscreen_scene() {
        let mut app = gpui::HeadlessAppContext::with_platform(
            std::sync::Arc::new(gpui::NoopTextSystem),
            std::sync::Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        let handle = app
            .open_window(size(px(640.), px(480.)), |window, cx| {
                cx.new(|cx| MatrixWindow::new(false, window, cx))
            })
            .expect("synthetic offscreen window");
        handle
            .update(&mut app, |view, window, cx| {
                EntityInputHandler::replace_text_in_range(view, None, "wrong", window, cx)
            })
            .expect("native committed input");
        let actions: Vec<Box<dyn gpui::Action>> = vec![
            Box::new(OpenLanguageMenu),
            Box::new(MenuDown),
            Box::new(MenuRight),
            Box::new(MenuEnter),
        ];
        for action in actions {
            app.update_window(handle.into(), |_, window, cx| {
                window.dispatch_action(action, cx)
            })
            .expect("public menu action");
            app.run_until_parked();
        }
        app.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .expect("draw source styles");
        let image = app
            .capture_screenshot(handle.into())
            .expect("own offscreen scene, not desktop screen capture");
        assert!(
            image
                .pixels()
                .any(|pixel| pixel.0[..3] == [0xff, 0xa4, 0x48]),
            "source ERROR background must reach the renderer"
        );
    }

    #[gpui::test]
    fn language_json_menu_is_buffer_owned_and_does_not_change_document_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().expect("synthetic styled export directory");
        let target = directory.path().join("styled.html");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, "{\"k\":true}", window, cx)
        });
        cx.dispatch_action(OpenLanguageMenu);
        let group = cx
            .debug_bounds("language-json-group")
            .expect("source J group");
        cx.simulate_click(group.center(), gpui::Modifiers::default());
        let json = cx
            .debug_bounds("language-json")
            .expect("actual JSON lexer command");
        cx.simulate_click(json.center(), gpui::Modifiers::default());
        cx.dispatch_action(ExportHtml);
        cx.simulate_new_path_selection(|_| Some(target.clone()));
        cx.run_until_parked();
        let html = std::fs::read_to_string(target).expect("actual syntax style export");
        assert!(html.contains("<span class=\"sc4\">\"k\"</span>"));
        assert!(html.contains("<span class=\"sc11\">true</span>"));
        cx.dispatch_action(NewFile);
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
    }

    #[gpui::test]
    fn macro_rejects_an_unimplemented_search_selection_before_any_playback_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("one two");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(ShowReplace);
        let mut dialog = floating_context(cx, "replace-input");
        dialog.simulate_input("two");
        cx.dispatch_action(StartMacroRecording);
        dialog.dispatch_action(ReplaceOne);
        cx.simulate_input("X");
        cx.dispatch_action(StopMacroRecording);
        dialog.dispatch_action(CloseFind);
        cx.dispatch_action(NewFile);
        cx.dispatch_action(PlaybackMacro);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        assert!(cx.debug_bounds("editor-error").is_some());
    }

    #[gpui::test]
    fn macro_menu_records_real_editor_steps_and_replays_with_target_eol_in_one_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenMacroMenu);
        cx.dispatch_action(MenuEnter);
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, "中", window, cx)
        });
        cx.dispatch_action(EditorEnter);
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, "tail", window, cx)
        });
        cx.dispatch_action(EditorLeft);
        cx.dispatch_action(EditorBackspace);
        cx.dispatch_action(WordRight);
        cx.dispatch_action(WordPartLeft);
        cx.dispatch_action(HomeDisplay);
        cx.dispatch_action(LineEndDisplay);
        cx.dispatch_action(PageUp);
        cx.dispatch_action(PageDown);
        cx.dispatch_action(DeleteWordLeft);
        cx.simulate_input("tal");
        cx.dispatch_action(EditorBackspaceNotLine);
        cx.simulate_input("l");
        cx.dispatch_action(EditorSelectAll);
        view.update_in(cx, |view, window, cx| {
            view.text_transform_command(42016, window, cx)
        });
        cx.dispatch_action(StopMacroRecording);
        cx.dispatch_action(NewFile);
        cx.dispatch_action(OpenEditMenu);
        let parent = cx.debug_bounds("edit-menu-eol").expect("EOL submenu");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let target = cx.debug_bounds("edit-eol-lf").expect("Unix conversion");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        cx.dispatch_action(PlaybackMacro);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("中\nTAL"));
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        cx.dispatch_action(EditorRedo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("中\nTAL"));
    }

    #[gpui::test]
    fn tools_hash_menu_copies_source_digests_without_editing_selection_or_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        // One public committed-input callback gives this fixture a known Undo
        // boundary, independent of the still-open typing coalescing contract.
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, "abc", window, cx)
        });
        cx.dispatch_action(EditorSelectAll);
        for (parent, command, expected) in [
            (
                "tools-md5-menu",
                "hash-copy-md5",
                "900150983cd24fb0d6963f7d28e17f72",
            ),
            (
                "tools-sha1-menu",
                "hash-copy-sha1",
                "a9993e364706816aba3e25717850c26c9cd0d89d",
            ),
            (
                "tools-sha256-menu",
                "hash-copy-sha256",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                "tools-sha512-menu",
                "hash-copy-sha512",
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
        ] {
            cx.dispatch_action(OpenToolsMenu);
            let parent = cx.debug_bounds(parent).expect("source hash family");
            cx.simulate_click(parent.center(), gpui::Modifiers::default());
            let command = cx
                .debug_bounds(command)
                .expect("real source selection hash command");
            cx.simulate_click(command.center(), gpui::Modifiers::default());
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some(expected)
            );
        }
        cx.simulate_input("X");
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(1),
            "hashing must not collapse source selection"
        );
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0),
            "hashing must add no document history"
        );
    }

    fn assert_run_output(path: &std::path::Path, expected: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match std::fs::read_to_string(path) {
                Ok(text) if text == expected => return,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("synthetic command output: {error}"),
            }
            assert!(
                std::time::Instant::now() < deadline,
                "synthetic Run output did not arrive"
            );
            std::thread::yield_now();
        }
    }

    #[gpui::test]
    fn run_dialog_tab_and_shift_tab_reach_run_and_cancel_with_keyboard_activation(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("tab", RunFocusNext, Some("MatrixRun")),
                KeyBinding::new("shift-tab", RunFocusPrevious, Some("MatrixRun")),
                KeyBinding::new("enter", ConfirmRun, Some("MatrixRun")),
                KeyBinding::new("space", ConfirmRun, Some("MatrixRunButton")),
            ])
        });
        let directory = tempfile::tempdir().expect("synthetic keyboard Run directory");
        let target = directory.path().join("output.txt");
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(ShowRun);
        let mut dialog = floating_context(cx, "run-dialog");
        dialog.simulate_input(&format!("printf output > '{}'", target.display()));
        dialog.simulate_keystrokes("tab tab enter");
        assert!(floating_bounds(cx, "run-dialog").is_none());
        assert!(!target.exists(), "Enter on focused Cancel must not execute");
        cx.dispatch_action(ShowRun);
        let mut dialog = floating_context(cx, "run-dialog");
        dialog.simulate_keystrokes("shift-tab space");
        assert!(floating_bounds(cx, "run-dialog").is_none());
        assert!(!target.exists(), "reverse traversal reaches Cancel");
        cx.dispatch_action(ShowRun);
        let mut dialog = floating_context(cx, "run-dialog");
        dialog.simulate_keystrokes("tab space");
        cx.run_until_parked();
        assert_run_output(&target, "output");
    }

    #[gpui::test]
    fn run_failure_retains_the_command_and_exposes_actionable_feedback(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(ShowRun);
        let mut dialog = floating_context(cx, "run-dialog");
        dialog.simulate_input("printf '%s' '$(FILE_NAME)'");
        dialog.dispatch_action(ConfirmRun);
        assert!(dialog.debug_bounds("run-dialog").is_some());
        assert!(
            dialog.debug_bounds("run-error").is_some(),
            "launch errors must expose visible accessible feedback"
        );
    }

    #[gpui::test]
    fn run_dialog_executes_only_explicitly_and_preserves_source_text_and_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().expect("synthetic Run directory");
        let target = directory.path().join("output.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("source document");
        cx.dispatch_action(OpenRunMenu);
        cx.dispatch_action(MenuEnter);
        let mut dialog = floating_context(cx, "run-dialog");
        assert!(dialog.debug_bounds("run-dialog").is_some());
        dialog.simulate_input(&format!(
            "printf '%s' 'literal result' > '{}'",
            target.display()
        ));
        assert!(
            !target.exists(),
            "typing or opening Run must not execute a command"
        );
        dialog.dispatch_action(CancelRun);
        assert!(!target.exists(), "Cancel must not launch");
        cx.dispatch_action(ShowRun);
        let mut dialog = floating_context(cx, "run-dialog");
        let run = dialog
            .debug_bounds("run-confirm")
            .expect("real Run command");
        dialog.simulate_click(run.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_run_output(&target, "literal result");
        assert!(floating_bounds(cx, "run-dialog").is_none());
        cx.dispatch_action(EditorUndo);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..0, &mut actual, window, cx)
        });
        assert_eq!(
            text.as_deref(),
            Some(""),
            "Run must not edit the document or add Undo entries"
        );
    }

    #[gpui::test]
    fn html_file_export_uses_whole_document_without_saving_or_editing_the_source(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().expect("synthetic export directory");
        let target = directory.path().join("output.html");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first\n<&>中");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectEnd);
        cx.dispatch_action(ExportHtml);
        cx.simulate_new_path_selection(|_| Some(target.clone()));
        cx.run_until_parked();
        let html = std::fs::read_to_string(&target).expect("actual generated output file");
        assert!(html.contains("<span class=\"sc0\">first\r\n&lt;&amp;&gt;中</span>"));
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..10, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first\n<&>中"));
        cx.dispatch_action(CloseCurrentFile);
        assert!(
            floating_bounds(cx, "close-cancel").is_some(),
            "export must not acknowledge the source savepoint"
        );
        cx.dispatch_action(CancelClose);
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..0, &mut actual, window, cx)
        });
        assert_eq!(
            text.as_deref(),
            Some(""),
            "export must not add document Undo entries"
        );
    }

    #[gpui::test]
    fn embedded_tools_disable_only_their_own_subtrees_and_runtime_handlers(
        cx: &mut gpui::TestAppContext,
    ) {
        for mask in 0..8 {
            let mime = mask & 1 != 0;
            let converter = mask & 2 != 0;
            let html = mask & 4 != 0;
            let (_, window_cx) = cx.add_window_view(|window, cx| {
                let mut view = MatrixWindow::new(false, window, cx);
                view.configure_embedded_tools(mime, converter, html);
                view
            });
            assert_eq!(
                window_cx.debug_bounds("plugins-menu-trigger").is_some(),
                mime || converter || html
            );
            if mime || converter || html {
                window_cx.dispatch_action(OpenPluginsMenu);
                assert_eq!(window_cx.debug_bounds("mime-tools-menu").is_some(), mime);
                assert_eq!(
                    window_cx.debug_bounds("converter-menu").is_some(),
                    converter
                );
                assert_eq!(window_cx.debug_bounds("npp-export-menu").is_some(), html);
            }
            window_cx.update(|window, cx| {
                assert_eq!(window.is_action_available(&MimeEncode, cx), mime);
                assert_eq!(window.is_action_available(&ConverterEncode, cx), converter);
                assert_eq!(window.is_action_available(&ExportHtml, cx), html);
                assert_eq!(
                    window.is_action_available(&OpenPluginsMenu, cx),
                    mime || converter || html
                );
            });
        }
    }

    #[gpui::test]
    fn converter_menu_coexists_with_mime_and_selects_its_two_step_result(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("Hello!");
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(OpenPluginsMenu);
        assert!(cx.debug_bounds("mime-tools-menu").is_some());
        let parent = cx
            .debug_bounds("converter-menu")
            .expect("second embedded module");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let encode = cx
            .debug_bounds("converter-ascii-to-hex")
            .expect("real source transform");
        cx.simulate_click(encode.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..12, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("48656C6C6F21"));
        cx.simulate_input("X");
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(ConverterDecode);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("Hello!"));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..0, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some(""));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..12, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("48656C6C6F21"));
    }

    #[gpui::test]
    fn line_operations_duplicate_menu_differs_from_ctrl_d_selection(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ab\ncd");
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(OpenEditMenu);
        let parent = cx
            .debug_bounds("edit-line-operations")
            .expect("real Line Operations submenu");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let duplicate = cx
            .debug_bounds("edit-duplicate-line")
            .expect("source current-line command");
        cx.simulate_click(duplicate.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("ab\r\nab\ncd"));
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(EditorDuplicateSelection);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..10, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("ab\ncdab\ncd"));
    }

    #[gpui::test]
    fn document_list_routes_to_existing_buffers_and_preserves_independent_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = tempfile::tempdir().expect("isolated document-list files");
        let alpha = fixture.path().join("alpha.txt");
        let beta = fixture.path().join("beta.txt");
        std::fs::write(&alpha, "alpha").expect("saved alpha baseline");
        std::fs::write(&beta, "beta").expect("saved beta baseline");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![alpha.clone(), beta.clone()]));
        cx.run_until_parked();
        let snapshot = |view: &MatrixWindow| {
            (
                view.workspace.active().id(),
                view.workspace.active_view(),
                view.workspace
                    .buffers()
                    .iter()
                    .map(|buffer| {
                        (
                            buffer.id(),
                            buffer.editor.committed_text(),
                            buffer.editor.selections(),
                            buffer.editor.document().can_undo(),
                            buffer.editor.document().can_redo(),
                            buffer.is_modified(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let first_open = view.read_with(cx, |view, _| snapshot(view));
        let windows = cx.windows().len();
        let toggle = cx
            .debug_bounds("toolbar-document-list")
            .expect("Document List toolbar");
        cx.simulate_click(toggle.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("document-list-panel").is_some());
        assert_eq!(cx.windows().len(), windows);
        view.read_with(cx, |view, _| assert_eq!(snapshot(view), first_open));
        let first = cx
            .debug_bounds("document-list-item-2")
            .expect("alpha buffer in actual list");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("alphaX"));
        let second = cx
            .debug_bounds("document-list-item-3")
            .expect("beta buffer");
        cx.simulate_click(second.center(), gpui::Modifiers::default());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("beta"));
        let first = cx
            .debug_bounds("document-list-item-2")
            .expect("same alpha buffer");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), crate::workspace::BufferId(2));
            assert_eq!(view.editor().committed_text(), "alphaX");
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().committed_text(),
                "alpha",
                "whole-document Undo payload"
            );
        });
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("alpha"));
        let hidden = view.read_with(cx, |view, _| snapshot(view));
        cx.simulate_click(toggle.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("document-list-panel").is_none());
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window))
        });
        cx.simulate_click(toggle.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("document-list-item-3").is_some());
        view.read_with(cx, |view, _| assert_eq!(snapshot(view), hidden));
        let before = view.read_with(cx, |view, _| snapshot(view));
        for (button, panel) in [
            ("toolbar-document-map", "document-map-panel"),
            ("toolbar-function-list", "function-list-panel"),
            ("toolbar-folder-workspace", "folder-workspace-panel"),
        ] {
            let button = cx.debug_bounds(button).expect("panel toolbar button");
            cx.simulate_click(button.center(), gpui::Modifiers::default());
            assert!(
                cx.debug_bounds(panel).is_some(),
                "correct destination {panel}"
            );
            assert_eq!(cx.windows().len(), windows);
            assert!(
                !cx.did_prompt_for_paths()
                    && !cx.did_prompt_for_new_path()
                    && !cx.has_pending_prompt()
            );
            view.read_with(cx, |view, _| assert_eq!(snapshot(view), before));
            cx.simulate_click(button.center(), gpui::Modifiers::default());
            assert!(cx.debug_bounds(panel).is_none());
            view.update_in(cx, |view, window, _| {
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(
                    snapshot(view),
                    before,
                    "panel toggle preserves all documents"
                );
            });
        }
        let before = view.read_with(cx, |view, _| snapshot(view));
        let close = cx
            .debug_bounds("document-list-close")
            .expect("DocList Close");
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("document-list-panel").is_none());
        view.update_in(cx, |view, window, _| {
            assert_eq!(snapshot(view), before);
            assert!(view.editor_focus.is_focused(window));
        });
        cx.simulate_input("!");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "alpha!");
            assert_eq!(view.workspace.buffers()[2].editor.committed_text(), "beta");
        });
    }

    #[gpui::test]
    fn wrapped_bookmark_navigation_reveals_unchanged_caret_after_manual_scroll(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(&"line\n".repeat(200));
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(ToggleBookmark);
        for action in [
            Box::new(NextBookmark) as Box<dyn gpui::Action>,
            Box::new(PreviousBookmark),
        ] {
            let editor = cx.debug_bounds("matrix-editor").expect("editor viewport");
            cx.simulate_event(gpui::ScrollEvent {
                position: editor.origin + point(px(TEXT_LEFT), px(10.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-1000.))),
                ..Default::default()
            });
            redraw_editor(cx);
            assert!(
                cx.debug_bounds("bookmark-marker-0").is_none(),
                "manual scroll moves the marked line out of view"
            );
            cx.update(|window, cx| window.dispatch_action(action, cx));
            cx.run_until_parked();
            redraw_editor(cx);
            assert!(
                cx.debug_bounds("bookmark-marker-0").is_some(),
                "explicit bookmark navigation must reveal even an unchanged caret"
            );
        }
    }

    #[gpui::test]
    fn bookmark_actions_gutter_navigation_and_buffer_ownership_are_real(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first\r\n中😀\nlast");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(ToggleBookmark);
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(ToggleBookmark);
        assert!(cx.debug_bounds("bookmark-marker-0").is_some());
        assert!(cx.debug_bounds("bookmark-marker-2").is_some());
        cx.dispatch_action(NextBookmark);
        let mut actual = None;
        let selected = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::selected_text_range(view, false, window, cx)
        });
        assert_eq!(selected.map(|selection| selection.range), Some(0..0));
        cx.dispatch_action(PreviousBookmark);
        cx.simulate_input("X");
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..16, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first\r\n中😀\nXlast"));
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(NewFile);
        assert!(cx.debug_bounds("bookmark-marker-0").is_none());
        cx.dispatch_action(CloseCurrentFile);
        assert!(cx.debug_bounds("bookmark-marker-0").is_some());
        let gutter = cx
            .debug_bounds("bookmark-line-0")
            .expect("source marker margin");
        cx.simulate_click(gutter.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("bookmark-marker-0").is_none());
        cx.dispatch_action(ClearBookmarks);
        assert!(cx.debug_bounds("bookmark-marker-2").is_none());
        cx.dispatch_action(CloneToOtherView);
        cx.dispatch_action(NewFile);
        cx.simulate_input("other\npane");
        let target = view.read_with(cx, |view, _| view.workspace.active().id());
        let primary = cx.debug_bounds("matrix-editor").expect("primary editor");
        cx.simulate_click(primary.center(), gpui::Modifiers::default());
        let snapshot = |view: &MatrixWindow| {
            view.workspace
                .buffers()
                .iter()
                .map(|buffer| {
                    let pane = if buffer.id() == target {
                        crate::workspace::ViewId::Secondary
                    } else {
                        crate::workspace::ViewId::Primary
                    };
                    let editor = view
                        .workspace
                        .editor_in_view(pane, buffer.id())
                        .expect("owning document view");
                    (
                        buffer.id(),
                        editor.committed_text(),
                        editor.selections(),
                        editor.document().can_undo(),
                        editor.document().can_redo(),
                        buffer.is_modified(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let before = view.read_with(cx, |view, _| {
            assert_eq!(
                view.workspace.active_view(),
                crate::workspace::ViewId::Primary
            );
            snapshot(view)
        });
        for marked in [true, false] {
            let margin = cx
                .debug_bounds("bookmark-line-secondary-1")
                .expect("secondary target line");
            cx.simulate_click(margin.center(), gpui::Modifiers::default());
            assert_eq!(
                cx.debug_bounds("bookmark-marker-secondary-1").is_some(),
                marked
            );
            assert!(cx.debug_bounds("bookmark-marker-secondary-0").is_none());
            assert!(cx.debug_bounds("bookmark-marker-1").is_none());
            view.read_with(cx, |view, _| {
                assert_eq!(
                    view.workspace.active_view(),
                    crate::workspace::ViewId::Secondary
                );
                assert_eq!(view.workspace.active().id(), target);
                assert_eq!(snapshot(view), before);
                assert_eq!(
                    view.editor()
                        .document()
                        .bookmarked_lines()
                        .collect::<Vec<_>>(),
                    if marked { vec![1] } else { vec![] }
                );
                assert!(
                    view.workspace.buffers()[0]
                        .editor
                        .document()
                        .bookmarked_lines()
                        .next()
                        .is_none()
                );
            });
        }
    }

    #[gpui::test]
    fn toolbar_uses_embedded_notepad_plus_plus_icons_in_both_themes(cx: &mut gpui::TestAppContext) {
        for dark in [false, true] {
            let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            for name in [
                "toolbar-new-icon",
                "toolbar-open-icon",
                "toolbar-save-icon",
                "toolbar-save-all-icon",
                "toolbar-close-icon",
                "toolbar-close-all-icon",
                "toolbar-cut-icon",
                "toolbar-copy-icon",
                "toolbar-paste-icon",
                "toolbar-undo-icon",
                "toolbar-redo-icon",
                "toolbar-find-icon",
                "toolbar-replace-icon",
                "toolbar-document-list-icon",
            ] {
                assert!(
                    cx.debug_bounds(name).is_some(),
                    "actual image slot for {name}"
                );
            }
        }
    }

    #[gpui::test]
    fn file_search_completion_does_not_take_focus_from_dirty_close_prompt(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(MatrixWindow::file_dialog_bindings()));
        let directory = tempfile::tempdir().expect("synthetic search directory");
        std::fs::write(directory.path().join("hit.txt"), "needle").expect("fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("unsaved");
        cx.dispatch_action(ShowFindInFiles);
        let mut dialog = floating_context(cx, "find-in-files-dialog");
        dialog.simulate_input("needle");
        let field = dialog
            .debug_bounds("files-directory")
            .expect("directory input");
        dialog.simulate_click(field.center(), gpui::Modifiers::default());
        dialog.simulate_input(directory.path().to_str().expect("test path"));
        cx.update(|window, cx| {
            window.dispatch_action(Box::new(FindAllInFiles), cx);
            window.dispatch_action(Box::new(CloseCurrentFile), cx);
        });
        cx.run_until_parked();
        assert!(
            floating_bounds(cx, "close-cancel").is_some(),
            "close prompt is active when background search completes"
        );
        assert!(
            cx.debug_bounds("finder-hit-0-0-0").is_some(),
            "the background search has completed"
        );
        let close_window = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::Close]
        });
        close_window
            .update(cx, |_, window, _| {
                assert!(
                    window.is_window_active(),
                    "background search completion must leave the close prompt active"
                );
            })
            .expect("close prompt window");
        floating_context(cx, "close-cancel").simulate_keystrokes("escape");
        assert!(
            floating_bounds(cx, "close-cancel").is_none(),
            "Escape must still reach the close prompt"
        );
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..7, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaved"));
    }

    #[gpui::test]
    fn find_in_files_opens_result_and_selects_unicode_without_editing_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().expect("synthetic search directory");
        std::fs::write(directory.path().join("hit.txt"), "first\r\n中😀 end").expect("fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(ShowFindInFiles);
        let mut dialog = floating_context(cx, "find-in-files-dialog");
        dialog.simulate_input("中😀");
        let directory_field = dialog
            .debug_bounds("files-directory")
            .expect("directory input");
        dialog.simulate_click(directory_field.center(), gpui::Modifiers::default());
        dialog.simulate_input(directory.path().to_str().expect("test path"));
        let find = dialog
            .debug_bounds("files-find-all")
            .expect("actual search button");
        dialog.simulate_click(find.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let hit = cx.debug_bounds("finder-hit-0-0-0").expect("search result");
        cx.simulate_event(gpui::MouseDownEvent {
            position: hit.center(),
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: hit.center(),
            button: MouseButton::Left,
            click_count: 2,
            ..Default::default()
        });
        cx.run_until_parked();
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..14, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first\r\n中😀 end"));
        let selected = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::selected_text_range(view, false, window, cx)
        });
        assert_eq!(selected.map(|selection| selection.range), Some(7..10));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..14, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first\r\n中😀 end"));
    }

    #[gpui::test]
    fn embedded_mime_menu_uses_rust_transform_and_disabled_module_has_no_handler(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, window_cx) =
            cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        window_cx.simulate_input("Hello");
        window_cx.dispatch_action(EditorSelectAll);
        window_cx.dispatch_action(OpenPluginsMenu);
        window_cx.dispatch_action(MenuRight);
        let encode = window_cx
            .debug_bounds("mime-base64-encode")
            .expect("embedded MIME Tools command");
        window_cx.simulate_click(encode.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(window_cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..7, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("SGVsbG8"));
        window_cx.dispatch_action(EditorUndo);
        window_cx.dispatch_action(MimeEncodePadded);
        let text = view.update_in(window_cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..8, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("SGVsbG8="));
        window_cx.dispatch_action(EditorSelectAll);
        window_cx.dispatch_action(MimeDecode);
        let text = view.update_in(window_cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("Hello"));
        let (_, disabled_cx) = cx.add_window_view(|window, cx| {
            let mut view = MatrixWindow::new(false, window, cx);
            view.configure_embedded_tools(false, true, false);
            view
        });
        assert!(disabled_cx.debug_bounds("plugins-menu-trigger").is_some());
        disabled_cx.dispatch_action(OpenPluginsMenu);
        assert!(disabled_cx.debug_bounds("mime-tools-menu").is_none());
        assert!(disabled_cx.debug_bounds("converter-menu").is_some());
        disabled_cx.update(|window, cx| {
            assert!(
                !window.is_action_available(&MimeEncode, cx),
                "disabled embedded module must remove its action handler"
            )
        });
    }

    #[gpui::test]
    fn preferences_general_hides_statusbar_and_returns_to_editor(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated preferences");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update(cx, |view, _| {
            view.settings_store = Some(ConfigStore::under_home(home.path()))
        });
        cx.simulate_input("text");
        assert!(cx.debug_bounds("status-bar").is_some());
        cx.dispatch_action(ShowPreferences);
        let mut dialog = floating_context(cx, "preferences-hide-statusbar");
        let hide = dialog
            .debug_bounds("preferences-hide-statusbar")
            .expect("source General setting");
        dialog.simulate_click(hide.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("status-bar").is_none());
        let pairs = dialog
            .debug_bounds("preferences-auto-insert-parentheses")
            .expect("pair preference");
        dialog.simulate_click(pairs.center(), gpui::Modifiers::default());
        assert_eq!(
            ConfigStore::under_home(home.path())
                .load_settings()
                .expect("saved preferences")["auto-insert"]["parentheses"],
            true
        );
        dialog.dispatch_action(ClosePreferences);
        cx.simulate_input("X");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("textX"));
    }

    #[gpui::test]
    fn normal_replace_selects_then_replaces_and_finds_the_next_occurrence(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("one two one");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(ShowReplace);
        let mut dialog = floating_context(cx, "replace-input");
        dialog.dispatch_action(FindNext);
        assert!(
            dialog.debug_bounds("replace-input").is_some(),
            "empty Find Next must not switch the Replace dialog into Find mode"
        );
        dialog.simulate_input("one");
        let replacement = dialog
            .debug_bounds("replace-input")
            .expect("Replace with field");
        dialog.simulate_click(replacement.center(), gpui::Modifiers::default());
        dialog.simulate_input("X");
        dialog.dispatch_action(ReplaceOne);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..11, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("one two one"));
        cx.dispatch_action(ShowReplace);
        dialog.dispatch_action(ReplaceAll);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..7, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("X two X"));
        dialog.dispatch_action(CloseFind);
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..11, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("one two one"));
        cx.dispatch_action(ShowReplace);
        let mut dialog = floating_context(cx, "replace-input");
        dialog.dispatch_action(ReplaceOne);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("X two one"));
        dialog.dispatch_action(CloseFind);
        cx.simulate_input("Y");
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..7, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("X two Y"));
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..11, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("one two one"));
    }

    #[gpui::test]
    fn go_to_line_reuses_dialog_input_and_does_not_add_document_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first\r\n中😀\nlast");
        cx.dispatch_action(ShowGoTo);
        let mut dialog = floating_context(cx, "goto-line-input");
        assert!(dialog.debug_bounds("goto-line-input").is_some());
        dialog.simulate_input("2");
        dialog.dispatch_action(ConfirmGoTo);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("line two start");
        assert_eq!(selected.range, 7..7);
        cx.simulate_input("X");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..16, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first\r\nX中😀\nlast"));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..15, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first\r\n中😀\nlast"));
    }

    #[gpui::test]
    fn find_dialog_input_selects_next_match_and_returns_focus_without_editing_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("one two ONE");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(ShowFind);
        let mut dialog = floating_context(cx, "find-query");
        assert!(dialog.debug_bounds("find-query").is_some());
        dialog.simulate_input("one");
        dialog.dispatch_action(FindNext);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("first match");
        assert_eq!(selected.range, 0..3);
        dialog.dispatch_action(FindNext);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("next match");
        assert_eq!(selected.range, 8..11);
        dialog.dispatch_action(CloseFind);
        cx.simulate_input("X");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("one two X"));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..11, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("one two ONE"));
    }

    #[gpui::test]
    fn clipboard_toolbar_and_actions_share_unicode_transactions(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A中😀");
        cx.dispatch_action(EditorSelectAll);
        let copy = cx
            .debug_bounds("toolbar-copy")
            .expect("source Copy toolbar");
        cx.simulate_click(copy.center(), gpui::Modifiers::default());
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("A中😀")
        );
        cx.dispatch_action(EditorCut);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(EditorPaste);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..8, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A中😀A中😀"));
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(EditorSelectAll);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("X\nY".into()));
        let paste = cx
            .debug_bounds("toolbar-paste")
            .expect("source Paste toolbar");
        cx.simulate_click(paste.center(), gpui::Modifiers::default());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("X\r\nY"));
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(EditorSelectAll);
        let cut = cx.debug_bounds("toolbar-cut").expect("source Cut toolbar");
        cx.simulate_click(cut.center(), gpui::Modifiers::default());
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("A中😀")
        );
    }

    #[gpui::test]
    fn file_toolbar_routes_new_open_save_save_all_close_and_close_all(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("toolbar fixture");
        let a = home.path().join("A.txt");
        let b = home.path().join("B.txt");
        std::fs::write(&a, b"disk-A").expect("A");
        std::fs::write(&b, b"disk-B").expect("B");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let new = cx.debug_bounds("toolbar-new").expect("source New toolbar");
        cx.simulate_click(new.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("buffer-tab-2").is_some());
        let close = cx
            .debug_bounds("toolbar-close")
            .expect("source Close toolbar");
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
        for path in [&a, &b] {
            let open = cx
                .debug_bounds("toolbar-open")
                .expect("source Open toolbar");
            cx.simulate_click(open.center(), gpui::Modifiers::default());
            cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
            cx.run_until_parked();
            if path == &a {
                let snapshot = |view: &MatrixWindow| {
                    (
                        view.workspace.active().id(),
                        view.workspace.active_view(),
                        view.workspace
                            .buffers()
                            .iter()
                            .map(|buffer| {
                                (
                                    buffer.id(),
                                    buffer.editor.committed_text(),
                                    buffer.editor.selections(),
                                    buffer.editor.document().can_undo(),
                                    buffer.editor.document().can_redo(),
                                    buffer.is_modified(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                };
                let before = view.read_with(cx, |view, _| snapshot(view));
                let windows = cx.windows().len();
                for enabled in [true, false] {
                    let button = cx
                        .debug_bounds("toolbar-monitoring")
                        .expect("Monitoring toolbar");
                    cx.simulate_click(button.center(), gpui::Modifiers::default());
                    let editor = cx.debug_bounds("matrix-editor").expect("editor");
                    cx.simulate_mouse_move(editor.center(), None, gpui::Modifiers::default());
                    redraw_editor(cx);
                    let checked = cx.update(|window, _| {
                        let bounds = button.scale(window.scale_factor());
                        window.painted_quads().iter().any(|quad| {
                            quad.bounds == bounds
                                && quad.content_mask.bounds.contains(&bounds.center())
                        })
                    });
                    assert_eq!(checked, enabled);
                    view.read_with(cx, |view, _| {
                        assert_eq!(snapshot(view), before);
                        assert_eq!(
                            view.workspace
                                .active()
                                .disk_file
                                .as_ref()
                                .map(|file| file.path()),
                            Some(a.as_path())
                        );
                        assert_eq!(view.monitoring_active(), enabled);
                        for buffer in view.workspace.buffers() {
                            let expected = enabled && buffer.id() == before.0;
                            assert_eq!(buffer.is_monitoring(), expected);
                            assert_eq!(buffer.editor.is_read_only(), expected);
                        }
                    });
                    assert_eq!(cx.windows().len(), windows);
                    assert!(
                        !cx.did_prompt_for_paths()
                            && !cx.did_prompt_for_new_path()
                            && !cx.has_pending_prompt()
                    );
                    assert_eq!(
                        std::fs::read(&a).expect("monitored source unchanged"),
                        b"disk-A"
                    );
                }
            }
            cx.simulate_input(if path == &a { "X" } else { "Y" });
            if path == &a {
                let save = cx
                    .debug_bounds("toolbar-save")
                    .expect("source Save toolbar");
                cx.simulate_click(save.center(), gpui::Modifiers::default());
                cx.run_until_parked();
                assert_eq!(std::fs::read(&a).expect("saved A"), b"Xdisk-A");
            }
        }
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("Z");
        let save_all = cx
            .debug_bounds("toolbar-save-all")
            .expect("source Save All toolbar");
        cx.simulate_click(save_all.center(), gpui::Modifiers::default());
        let mut dialog = floating_context(cx, "save-all-yes");
        let yes = dialog
            .debug_bounds("save-all-yes")
            .expect("existing Save All confirmation");
        dialog.simulate_click(yes.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(&a).expect("A all"), b"Xdisk-AZ");
        assert_eq!(std::fs::read(&b).expect("B all"), b"Ydisk-B");
        let close_all = cx
            .debug_bounds("toolbar-close-all")
            .expect("source Close All toolbar");
        cx.simulate_click(close_all.center(), gpui::Modifiers::default());
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        let original = view.read_with(cx, |view, _| {
            (view.workspace.active().id(), view.workspace.buffers().len())
        });
        let windows = cx.windows().len();
        let new = cx.debug_bounds("menu-new-document").expect("main-row New");
        cx.simulate_click(new.center(), gpui::Modifiers::default());
        view.update_in(cx, |view, window, _| {
            assert_ne!(view.workspace.active().id(), original.0);
            assert_eq!(view.workspace.buffers().len(), original.1 + 1);
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
            assert!(view.editor_focus.is_focused(window));
        });
        let close = cx
            .debug_bounds("menu-close-document")
            .expect("main-row Close");
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        view.update_in(cx, |view, window, _| {
            assert_eq!(
                (view.workspace.active().id(), view.workspace.buffers().len()),
                original
            );
            assert!(view.editor_focus.is_focused(window));
        });
        assert_eq!(cx.windows().len(), windows);
        assert!(
            !cx.did_prompt_for_paths() && !cx.did_prompt_for_new_path() && !cx.has_pending_prompt()
        );
    }

    #[gpui::test]
    fn recovered_change_queue_keeps_no_and_failed_reload_then_loads_next_yes(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("queue HOME");
        let mut source = crate::workspace::Workspace::new();
        let mut paths = Vec::new();
        for name in ["A", "B", "C"] {
            let path = home.path().join(format!("{name}.txt"));
            std::fs::write(&path, name).expect("original bytes");
            std::fs::File::open(&path)
                .expect("file")
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890))
                .expect("original mtime");
            let (file, text) = crate::files::DiskFile::load_unicode(&path).expect("load");
            source.open_buffer(file, &text).expect("source tab");
            source
                .active_mut()
                .editor
                .insert_text("unsaved-")
                .expect("edit");
            paths.push(path);
        }
        let mut session = crate::recovery::RecoverySession::create(home.path()).expect("session");
        session.checkpoint(&source).expect("snapshots");
        drop(session);
        for (index, path) in paths.iter().enumerate() {
            std::fs::write(
                path,
                if index == 1 {
                    &[0xff, 0xfe, 0][..]
                } else if index == 0 {
                    b"new-A"
                } else {
                    b"new-C"
                },
            )
            .expect("external bytes");
            std::fs::File::open(path)
                .expect("file")
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567900))
                .expect("external mtime");
        }
        let (recovery, workspace) =
            crate::recovery::RecoveryManager::open(home.path()).expect("restore");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = MatrixWindow::new(false, window, cx);
            view.install_recovery(recovery, workspace, window, cx);
            view
        });
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaved-A"));
        floating_context(cx, "reload-no").dispatch_action(ActivateReloadChoice);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaved-B"));
        std::fs::write(&paths[1], [0xff, 0x80]).expect("failed reload bytes");
        floating_context(cx, "reload-yes").dispatch_action(ConfirmReload);
        cx.run_until_parked();
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaved-C"));
        assert!(floating_bounds(cx, "reload-no").is_some());
        assert!(
            cx.debug_bounds("editor-error").is_some(),
            "failed B reload remains visible while C is presented"
        );
        floating_context(cx, "reload-yes").dispatch_action(ConfirmReload);
        cx.run_until_parked();
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("new-C"));
        assert!(floating_bounds(cx, "reload-no").is_none());
        cx.dispatch_action(PreviousBuffer);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaved-B"));
        cx.dispatch_action(PreviousBuffer);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaved-A"));
        assert_eq!(std::fs::read(&paths[0]).expect("A disk"), b"new-A");
        assert_eq!(std::fs::read(&paths[1]).expect("B disk"), [0xff, 0x80]);
        assert_eq!(std::fs::read(&paths[2]).expect("C disk"), b"new-C");
    }

    #[gpui::test]
    fn recovered_external_change_defaults_to_no_and_preserves_backup_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("recovery HOME");
        let path = home.path().join("changed.txt");
        std::fs::write(&path, b"disk").expect("original");
        std::fs::File::open(&path)
            .expect("file")
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890))
            .expect("original mtime");
        let (file, text) = crate::files::DiskFile::load_unicode(&path).expect("load original");
        let mut source = crate::workspace::Workspace::new();
        source.open_buffer(file, &text).expect("source buffer");
        source
            .active_mut()
            .editor
            .insert_text("unsaved")
            .expect("edit");
        source
            .active_mut()
            .editor
            .select(Selection {
                anchor: ByteOffset(0),
                head: ByteOffset(7),
            })
            .expect("directional source selection");
        let mut session = crate::recovery::RecoverySession::create(home.path()).expect("session");
        session.checkpoint(&source).expect("backup");
        drop(session);
        std::fs::write(&path, b"external").expect("external writer");
        std::fs::File::open(&path)
            .expect("file")
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567900))
            .expect("changed mtime");
        let (recovery, workspace) =
            crate::recovery::RecoveryManager::open(home.path()).expect("recover");
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = MatrixWindow::new(false, window, cx);
            view.install_recovery(recovery, workspace, window, cx);
            view
        });
        assert!(
            floating_bounds(cx, "reload-no").is_some(),
            "source changed-file prompt must be shown on startup"
        );
        floating_context(cx, "reload-no").dispatch_action(ActivateReloadChoice);
        assert!(floating_bounds(cx, "reload-no").is_none());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..11, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("unsaveddisk"));
        let selection = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("source caret");
        assert_eq!(
            selection.range,
            7..7,
            "prepareBufferChangedDialog collapses to main caret"
        );
        assert_eq!(std::fs::read(path).expect("external bytes"), b"external");
        cx.dispatch_action(CloseCurrentFile);
        assert!(
            floating_bounds(cx, "close-save").is_some(),
            "No keeps restored document dirty"
        );
    }

    #[gpui::test]
    fn line_delete_action_deletes_only_main_caret_line_and_preserves_selection_direction(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A\r\nB\nC");
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(EditorLineDelete);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("B\nC"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(3)
        );
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("remaining source selection");
        assert_eq!(range.range, 0..3);
        assert!(range.reversed);
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A\r\nB\nC"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(6)
        );
    }

    #[gpui::test]
    fn backspace_not_line_deletes_inside_line_and_stops_at_crlf_lf_and_cr_starts(
        cx: &mut gpui::TestAppContext,
    ) {
        for (text, start) in [("A\r\nB", 3), ("A\nB", 2), ("A\rB", 2), ("A\n", 2)] {
            let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
            cx.simulate_input(text);
            cx.dispatch_action(EditorHome);
            let before = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("line start");
            assert_eq!(before.range, start..start);
            cx.dispatch_action(EditorBackspaceNotLine);
            let after = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("unchanged caret");
            assert_eq!(after.range, before.range);
            let mut actual = None;
            let observed = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_for_range(view, 0..text.len(), &mut actual, window, cx)
            });
            assert_eq!(observed.as_deref(), Some(text));
        }
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A😀");
        cx.dispatch_action(EditorBackspaceNotLine);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(1)
        );
    }

    #[gpui::test]
    fn backspace_not_line_keeps_line_start_but_deletes_a_nonempty_cross_line_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A\r\nB\nC");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorRight);
        cx.dispatch_action(EditorRight);
        cx.dispatch_action(EditorBackspaceNotLine);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A\r\nB\nC"));
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("line-start caret");
        assert_eq!(range.range, 3..3);
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorBackspaceNotLine);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("B\nC"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(3)
        );
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A\r\nB\nC"));
        view.update_in(cx, |view, _, _| {
            view.workspace.active_mut().set_user_read_only(true)
        });
        cx.dispatch_action(EditorSelectAll);
        let unchanged = |view: &MatrixWindow| {
            (
                view.editor().committed_text(),
                view.editor().display_revision(),
                view.editor().document().can_undo(),
                view.editor().document().can_redo(),
                view.workspace.active().is_modified(),
            )
        };
        let before = view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(6),
                    head: ByteOffset(0)
                }
            );
            unchanged(view)
        });
        cx.dispatch_action(EditorBackspaceNotLine);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(0)
                }
            );
            assert_eq!(unchanged(view), before);
        });
        cx.dispatch_action(EditorBackspaceNotLine);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(0)
                }
            );
            assert_eq!(unchanged(view), before);
        });
    }

    #[gpui::test]
    fn character_keys_navigate_readonly_text_without_changing_original_bytes(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("character readonly directory");
        let path = home.path().join("readonly.txt");
        let original = "R😀\r\nZ";
        std::fs::write(&path, original.as_bytes()).expect("fixture");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).expect("readonly");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorRight);
        cx.dispatch_action(EditorRight);
        cx.dispatch_action(EditorLeft);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("readonly caret");
        assert_eq!(range.range, 1..1);
        cx.dispatch_action(EditorSelectRight);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("readonly selection");
        assert_eq!(range.range, 1..3);
        assert!(!range.reversed);
        assert_eq!(
            std::fs::read(path).expect("original bytes"),
            original.as_bytes()
        );
    }

    #[gpui::test]
    fn character_keys_match_source_utf16_boundaries_and_keep_redo(cx: &mut gpui::TestAppContext) {
        let fixture = tempfile::tempdir().expect("saved character navigation baseline");
        let path = fixture.path().join("characters.txt");
        std::fs::write(&path, "A😀\r\ne\u{301}Z").expect("saved Unicode source");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "A😀\r\ne\u{301}Z")
        });
        cx.dispatch_action(EditorDocumentStart);
        for expected in [1, 3, 5, 6, 7, 8] {
            cx.dispatch_action(EditorRight);
            let range = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("right caret");
            assert_eq!(range.range, expected..expected);
            assert!(!range.reversed);
        }
        cx.dispatch_action(EditorRight);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("EOF clamp");
        assert_eq!(range.range, 8..8);
        for expected in [7, 6, 5, 3, 1, 0] {
            cx.dispatch_action(EditorLeft);
            let range = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("left caret");
            assert_eq!(range.range, expected..expected);
        }
        cx.dispatch_action(EditorLeft);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("BOF clamp");
        assert_eq!(range.range, 0..0);
        cx.dispatch_action(EditorRedo);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A😀\r\ne\u{301}ZX"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(9)
        );
    }

    #[gpui::test]
    fn shift_character_keys_preserve_anchor_and_plain_keys_collapse_the_source_edge(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A😀\r\ne\u{301}Z");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorSelectRight);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("forward selection");
        assert_eq!(range.range, 0..3);
        assert!(!range.reversed);
        cx.dispatch_action(EditorLeft);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("collapse left");
        assert_eq!(range.range, 0..0);
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(EditorSelectLeft);
        cx.dispatch_action(EditorSelectLeft);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("reverse selection");
        assert_eq!(range.range, 6..8);
        assert!(range.reversed);
        cx.dispatch_action(EditorRight);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("collapse right");
        assert_eq!(range.range, 8..8);
        cx.dispatch_action(EditorSelectLeft);
        cx.dispatch_action(EditorLeft);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("reverse collapse left");
        assert_eq!(range.range, 7..7);
    }

    #[gpui::test]
    fn character_keys_leave_an_empty_document_empty_and_unselected(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(EditorLeft);
        cx.dispatch_action(EditorRight);
        cx.dispatch_action(EditorSelectLeft);
        cx.dispatch_action(EditorSelectRight);
        let range = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("empty caret");
        assert_eq!(range.range, 0..0);
        assert!(!range.reversed);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
    }

    #[gpui::test]
    fn disabled_toolbar_and_pending_confirmations_preserve_document_ownership(
        app: &mut gpui::TestAppContext,
    ) {
        app.update(|cx| cx.bind_keys(MatrixWindow::file_dialog_bindings()));
        let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ABC");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorDelete);
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(EditorSelectLeft);
        cx.dispatch_action(EditorDelete);
        cx.dispatch_action(SaveFile);
        let saving_focus = cx.update(|window, cx| window.focused(cx));
        let undo = cx.debug_bounds("toolbar-undo").expect("disabled Undo");
        cx.simulate_click(undo.center(), gpui::Modifiers::default());
        assert_eq!(cx.update(|window, cx| window.focused(cx)), saving_focus);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("B"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(1)
        );
        cx.simulate_new_path_selection(|_| None);
        cx.run_until_parked();
        let undo = cx.debug_bounds("toolbar-undo").expect("enabled Undo");
        cx.simulate_click(undo.center(), gpui::Modifiers::default());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("BC"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(2)
        );
        let fixture = tempfile::tempdir().expect("isolated confirmation protection");
        let path = fixture.path().join("project.txt");
        std::fs::write(&path, "disk").expect("project file");
        for save_all in [false, true] {
            for cancellation in 0..3 {
                let (view, cx) =
                    app.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
                cx.simulate_resize(size(px(1080.), px(668.)));
                cx.simulate_input("first");
                cx.dispatch_action(NewFile);
                cx.simulate_input("second");
                view.update_in(cx, |view, window, cx| {
                    let panel = view.projects.model.panel_mut(0).expect("Project1");
                    let root = panel.add_project("Project").expect("project root");
                    panel.add_file(root, &path).expect("project file row");
                    view.show_project_panel(0, window, cx);
                });
                let snapshot = |view: &MatrixWindow| {
                    (
                        view.workspace.active().id(),
                        view.workspace.active_view(),
                        view.workspace
                            .buffers()
                            .iter()
                            .map(|buffer| {
                                (
                                    buffer.id(),
                                    buffer.editor.committed_text(),
                                    buffer.editor.selections(),
                                    buffer.editor.document().can_undo(),
                                    buffer.editor.document().can_redo(),
                                    buffer.is_modified(),
                                )
                            })
                            .collect::<Vec<_>>(),
                        view.menu_bar.mode(),
                        view.projects.model.panel(0).expect("Project1").selected,
                    )
                };
                let before = view.read_with(cx, |view, _| snapshot(view));
                let windows = cx.windows().len();
                let button = cx
                    .debug_bounds(if save_all {
                        "toolbar-save-all"
                    } else {
                        "toolbar-close"
                    })
                    .expect("confirmation entrance");
                cx.simulate_click(button.center(), gpui::Modifiers::default());
                let cancel_selector = if save_all {
                    "save-all-no"
                } else {
                    "close-cancel"
                };
                let mut dialog = floating_context(cx, cancel_selector);
                let main_focus = cx.update(|window, cx| window.focused(cx));
                let target = view.read_with(cx, |view, _| {
                    if save_all {
                        assert!(view.save_all_choice_yes);
                        assert_eq!(view.save_all_restore, Some(before.0));
                    } else {
                        assert_eq!(view.close_choice, 0);
                        assert_eq!(view.close_target, Some(before.0));
                    }
                    (
                        view.close_target,
                        view.save_all_restore,
                        view.close_choice,
                        view.save_all_choice_yes,
                    )
                });
                assert_eq!(cx.windows().len(), windows + 1);
                for selector in [
                    "toolbar-new",
                    "toolbar-find",
                    "file-menu-trigger",
                    "menu-new-document",
                    "buffer-tab-1",
                    "status-position",
                    "project-entry-0-2",
                ]
                .into_iter()
                .filter(|_| cancellation == 0)
                {
                    let position = cx
                        .debug_bounds(selector)
                        .expect("protected underlying target")
                        .center();
                    cx.simulate_click(position, gpui::Modifiers::default());
                    cx.run_until_parked();
                    view.read_with(cx, |view, _| {
                        assert_eq!(snapshot(view), before, "protected target {selector}");
                        assert_eq!(
                            (
                                view.close_target,
                                view.save_all_restore,
                                view.close_choice,
                                view.save_all_choice_yes
                            ),
                            target
                        );
                    });
                    assert_eq!(
                        cx.update(|window, cx| window.focused(cx)),
                        main_focus,
                        "main focus changed: save_all={save_all}, cancellation={cancellation}, target={selector}"
                    );
                    view.update_in(&mut dialog, |view, window, _| {
                        assert!(view.close_focus.is_focused(window))
                    });
                    assert_eq!(cx.windows().len(), windows + 1);
                    assert!(
                        !cx.did_prompt_for_paths()
                            && !cx.did_prompt_for_new_path()
                            && !cx.has_pending_prompt()
                    );
                    assert_eq!(std::fs::read(&path).expect("protected disk bytes"), b"disk");
                }
                match cancellation {
                    0 => dialog.simulate_keystrokes("escape"),
                    1 => {
                        let button = dialog
                            .debug_bounds(cancel_selector)
                            .expect("own cancellation button");
                        dialog.simulate_click(button.center(), gpui::Modifiers::default());
                    }
                    _ => dialog.simulate_keystrokes(if save_all {
                        "tab enter"
                    } else {
                        "tab tab enter"
                    }),
                }
                assert!(cx.debug_bounds("matrix-editor").is_some());
                cx.run_until_parked();
                assert_eq!(cx.windows().len(), windows);
                assert!(
                    !cx.did_prompt_for_paths()
                        && !cx.did_prompt_for_new_path()
                        && !cx.has_pending_prompt()
                );
                assert_eq!(std::fs::read(&path).expect("cancelled disk bytes"), b"disk");
                view.update_in(cx, |view, window, _| {
                    assert_eq!(snapshot(view), before, "cancellation preserves all editors");
                    assert!(view.editor_focus.is_focused(window));
                });
                cx.simulate_input("!");
                view.read_with(cx, |view, _| {
                    assert_eq!(view.editor().committed_text(), "second!");
                    assert_eq!(view.workspace.buffers()[0].editor.committed_text(), "first");
                });
            }
        }
    }

    #[gpui::test]
    fn undo_redo_toolbar_routes_once_and_returns_focus_to_editor(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ABC");
        cx.dispatch_action(EditorDocumentStart);
        cx.dispatch_action(EditorSelectRight);
        cx.dispatch_action(EditorDelete);
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(EditorSelectLeft);
        cx.dispatch_action(EditorDelete);
        let undo = cx
            .debug_bounds("toolbar-undo")
            .expect("source Undo toolbar button");
        cx.simulate_click(undo.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("BC"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(2)
        );
        let redo = cx
            .debug_bounds("toolbar-redo")
            .expect("source Redo toolbar button");
        cx.simulate_click(redo.center(), gpui::Modifiers::default());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("B"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(1)
        );
        let redo = cx
            .debug_bounds("toolbar-redo")
            .expect("disabled Redo remains rendered");
        cx.simulate_click(redo.center(), gpui::Modifiers::default());
        cx.simulate_input("X");
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("BX"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(2)
        );
    }

    #[gpui::test]
    fn edit_delete_source_availability_differs_from_delete_key_and_preserves_readonly_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, window_cx) =
            cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        window_cx.simulate_input("A");
        window_cx.dispatch_action(OpenEditMenu);
        let command = window_cx
            .debug_bounds("edit-menu-delete")
            .expect("disabled Delete row");
        window_cx.simulate_click(command.center(), gpui::Modifiers::default());
        assert!(window_cx.debug_bounds("edit-menu-delete").is_some());
        assert_eq!(
            view.update_in(window_cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(1)
        );
        window_cx.dispatch_action(MenuEscape);
        window_cx.dispatch_action(MenuEscape);
        window_cx.dispatch_action(EditorDocumentStart);
        window_cx.dispatch_action(EditorDelete);
        assert_eq!(
            view.update_in(window_cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            }),
            Some(0)
        );
        let home = tempfile::tempdir().expect("Delete readonly directory");
        let path = home.path().join("readonly.txt");
        std::fs::write(&path, b"A").expect("fixture");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).expect("readonly");
        let (readonly, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(OpenEditMenu);
        let command = cx
            .debug_bounds("edit-menu-delete")
            .expect("selected readonly Delete row");
        cx.simulate_click(command.center(), gpui::Modifiers::default());
        assert!(
            cx.debug_bounds("edit-menu-delete").is_none(),
            "source enables Delete for a nonempty selection; readonly enforcement is the edit guard"
        );
        let mut actual = None;
        let text = readonly.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A"));
        assert_eq!(std::fs::read(path).expect("readonly bytes"), b"A");
    }

    #[gpui::test]
    fn edit_delete_menu_uses_existing_delete_transaction_and_restores_direction_on_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A😀\r\nZ");
        cx.dispatch_action(EditorSelectAll);
        cx.dispatch_action(OpenEditMenu);
        let command = cx
            .debug_bounds("edit-menu-delete")
            .expect("source Edit Delete entry");
        cx.simulate_click(command.center(), gpui::Modifiers::default());
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        cx.dispatch_action(EditorUndo);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A😀\r\nZ"));
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("restored source selection");
        assert_eq!(selected.range, 0..6);
        assert!(selected.reversed);
        cx.dispatch_action(EditorRedo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
    }

    #[gpui::test]
    fn select_all_matches_source_caret_at_start_and_anchor_at_end(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A中😀\r\nZ");
        cx.dispatch_action(EditorSelectAll);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("Select All range");
        assert_eq!(selected.range, 0..7);
        assert!(
            selected.reversed,
            "source SelectAll uses currentPos=0 and anchor=length"
        );
    }

    #[gpui::test]
    fn close_right_yes_then_cancel_saves_farthest_before_committing_prefix(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("Right save directory");
        let path = home.path().join("far.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(NewFile);
        cx.simulate_input("anchor");
        cx.dispatch_action(NewFile);
        cx.simulate_input("near");
        cx.dispatch_action(NewFile);
        cx.simulate_input("far");
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(CloseRightFiles);
        floating_context(cx, "close-save").dispatch_action(SaveAndClose);
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        assert_eq!(std::fs::read(path).expect("far output"), b"far");
        assert!(cx.debug_bounds("buffer-tab-4").is_some());
        assert!(floating_bounds(cx, "close-cancel").is_some());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("near"));
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-4").is_none());
        for id in ["buffer-tab-1", "buffer-tab-2", "buffer-tab-3"] {
            assert!(cx.debug_bounds(id).is_some());
        }
    }

    #[gpui::test]
    fn right_menu_closes_clean_right_and_empty_right_cannot_activate_left(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(NewFile);
        cx.simulate_input("A");
        cx.dispatch_action(NewFile);
        cx.dispatch_action(NewFile);
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(OpenFileMenu);
        let parent = cx
            .debug_bounds("file-menu-close-multiple")
            .expect("conditional parent");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let right = cx
            .debug_bounds("file-menu-close-right")
            .expect("Right action");
        cx.simulate_click(right.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("buffer-tab-3").is_none());
        assert!(cx.debug_bounds("buffer-tab-4").is_none());
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
        assert!(cx.debug_bounds("buffer-tab-2").is_some());
        cx.dispatch_action(OpenFileMenu);
        let parent = cx
            .debug_bounds("file-menu-close-multiple")
            .expect("parent with Left enabled");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let left = cx
            .debug_bounds("file-menu-close-left")
            .expect("previous Left selection");
        cx.simulate_event(gpui::MouseMoveEvent {
            position: left.center(),
            ..Default::default()
        });
        let right = cx
            .debug_bounds("file-menu-close-right")
            .expect("source enabled empty-target Right rendered");
        cx.simulate_click(right.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("file-menu-close-right").is_none());
        assert!(
            cx.debug_bounds("buffer-tab-1").is_some(),
            "empty-target Right activated the previous Left command"
        );
        assert!(cx.debug_bounds("buffer-tab-2").is_some());
        assert!(floating_bounds(cx, "close-cancel").is_none());
        cx.dispatch_action(MenuEscape);
        cx.dispatch_action(MenuEscape);
        cx.dispatch_action(MenuEscape);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("selection");
        assert_eq!(selected.range, 1..1);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        cx.dispatch_action(EditorRedo);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A"));
    }

    #[gpui::test]
    fn close_right_no_then_cancel_uses_farthest_first_and_keeps_anchor_left(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("left");
        cx.dispatch_action(NewFile);
        cx.simulate_input("anchor");
        cx.dispatch_action(NewFile);
        cx.simulate_input("near");
        cx.dispatch_action(NewFile);
        cx.simulate_input("far");
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(CloseRightFiles);
        assert!(floating_bounds(cx, "close-discard").is_some());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("far"));
        floating_context(cx, "close-discard").dispatch_action(DiscardClose);
        assert!(cx.debug_bounds("buffer-tab-4").is_some());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("near"));
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-4").is_none());
        for id in ["buffer-tab-1", "buffer-tab-2", "buffer-tab-3"] {
            assert!(cx.debug_bounds(id).is_some());
        }
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("near"));
        cx.dispatch_action(CloseCurrentFile);
        assert!(floating_bounds(cx, "close-cancel").is_some());
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
        assert!(cx.debug_bounds("buffer-tab-2").is_some());
    }

    #[gpui::test]
    fn file_nested_close_left_click_and_empty_target_are_atomic(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(NewFile);
        cx.dispatch_action(NewFile);
        cx.simulate_input("A");
        cx.dispatch_action(OpenFileMenu);
        let parent = cx
            .debug_bounds("file-menu-close-multiple")
            .expect("File conditional submenu");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let target = cx
            .debug_bounds("file-menu-close-left")
            .expect("Left action");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("buffer-tab-1").is_none());
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
        assert!(cx.debug_bounds("buffer-tab-3").is_some());
        cx.dispatch_action(OpenFileMenu);
        cx.dispatch_action(MenuLast);
        let parent = cx
            .debug_bounds("file-menu-close-multiple")
            .expect("source enabled parent rendered");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let target = cx
            .debug_bounds("file-menu-close-left")
            .expect("source enabled empty-target Left");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("file-menu-close-left").is_none());
        assert!(cx.debug_bounds("buffer-tab-3").is_some());
        assert!(
            floating_bounds(cx, "close-cancel").is_none(),
            "empty-target Left activated the previously selected Exit"
        );
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("selection");
        assert_eq!(selected.range, 1..1);
    }

    #[gpui::test]
    fn close_left_cancelled_save_and_readonly_failure_keep_unapproved_suffix(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_view, window_cx) =
            cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        window_cx.simulate_input("far");
        window_cx.dispatch_action(NewFile);
        window_cx.dispatch_action(NewFile);
        window_cx.simulate_input("anchor");
        window_cx.dispatch_action(CloseLeftFiles);
        floating_context(window_cx, "close-save").dispatch_action(SaveAndClose);
        window_cx.simulate_new_path_selection(|_| None);
        window_cx.run_until_parked();
        assert!(window_cx.debug_bounds("buffer-tab-2").is_none());
        assert!(window_cx.debug_bounds("buffer-tab-1").is_some());
        assert!(window_cx.debug_bounds("buffer-tab-3").is_some());
        assert!(floating_bounds(window_cx, "close-cancel").is_none());
        window_cx.dispatch_action(CloseCurrentFile);
        assert!(floating_bounds(window_cx, "close-cancel").is_some());
        floating_context(window_cx, "close-cancel").dispatch_action(CancelClose);
        let home = tempfile::tempdir().expect("Left readonly directory");
        let original = home.path().join("readonly");
        std::fs::write(&original, b"A").expect("fixture");
        let mut permissions = std::fs::metadata(&original)
            .expect("metadata")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&original, permissions).expect("readonly");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![original.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(OpenEncodingMenu);
        let choice = cx.debug_bounds("encoding-utf8-bom").expect("format");
        cx.simulate_click(choice.center(), gpui::Modifiers::default());
        cx.dispatch_action(NewFile);
        cx.dispatch_action(NewFile);
        cx.simulate_input("anchor");
        cx.dispatch_action(CloseLeftFiles);
        floating_context(cx, "close-save").dispatch_action(SaveAndClose);
        cx.run_until_parked();
        assert!(cx.debug_bounds("buffer-tab-3").is_none());
        for id in ["buffer-tab-1", "buffer-tab-2", "buffer-tab-4"] {
            assert!(cx.debug_bounds(id).is_some());
        }
        assert!(floating_bounds(cx, "close-cancel").is_none());
        assert_eq!(std::fs::read(original).expect("source bytes"), b"A");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A"));
        cx.dispatch_action(CloseCurrentFile);
        assert!(floating_bounds(cx, "close-cancel").is_some());
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
    }

    #[gpui::test]
    fn close_left_save_collision_commits_clean_prefix_and_clears_batch(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("Left collision directory");
        let guarded = home.path().join("guarded.txt");
        std::fs::write(&guarded, b"guarded").expect("guarded fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("far");
        cx.dispatch_action(NewFile);
        cx.dispatch_action(NewFile);
        cx.simulate_input("anchor");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![guarded.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(CloseLeftFiles);
        floating_context(cx, "close-save").dispatch_action(SaveAndClose);
        cx.simulate_new_path_selection(|_| Some(guarded.clone()));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("buffer-tab-2").is_none(),
            "collision failed to commit the earlier clean approval"
        );
        for id in ["buffer-tab-1", "buffer-tab-3", "buffer-tab-4"] {
            assert!(cx.debug_bounds(id).is_some());
        }
        assert!(floating_bounds(cx, "close-cancel").is_none());
        assert_eq!(std::fs::read(&guarded).expect("guarded bytes"), b"guarded");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..7, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("guarded"));
        cx.dispatch_action(CloseCurrentFile);
        assert!(cx.debug_bounds("buffer-tab-4").is_none());
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
        assert!(cx.debug_bounds("buffer-tab-3").is_some());
    }

    #[gpui::test]
    fn close_left_save_approval_waits_for_later_decision_before_removing_saved_tab(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("Left save directory");
        let path = home.path().join("near.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("far");
        cx.dispatch_action(NewFile);
        cx.simulate_input("near");
        cx.dispatch_action(NewFile);
        cx.simulate_input("anchor");
        cx.dispatch_action(CloseLeftFiles);
        floating_context(cx, "close-save").dispatch_action(SaveAndClose);
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(path).expect("acknowledged save bytes"),
            b"near"
        );
        assert!(
            cx.debug_bounds("buffer-tab-2").is_some(),
            "saved target removed before later decision"
        );
        assert!(floating_bounds(cx, "close-cancel").is_some());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("far"));
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
        assert!(cx.debug_bounds("buffer-tab-3").is_some());
    }

    #[gpui::test]
    fn close_left_collects_no_then_cancel_before_committing_only_the_approved_prefix(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("far");
        cx.dispatch_action(NewFile);
        cx.simulate_input("near");
        cx.dispatch_action(NewFile);
        cx.simulate_input("anchor");
        cx.dispatch_action(NewFile);
        cx.simulate_input("right");
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(CloseLeftFiles);
        assert!(floating_bounds(cx, "close-discard").is_some());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("near"));
        floating_context(cx, "close-discard").dispatch_action(DiscardClose);
        assert!(
            cx.debug_bounds("buffer-tab-2").is_some(),
            "approved prefix was deleted before collecting later decisions"
        );
        assert!(floating_bounds(cx, "close-cancel").is_some());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("far"));
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
        for id in ["buffer-tab-1", "buffer-tab-3", "buffer-tab-4"] {
            assert!(cx.debug_bounds(id).is_some());
        }
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("far"));
        cx.dispatch_action(CloseCurrentFile);
        assert!(floating_bounds(cx, "close-cancel").is_some());
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        for id in ["buffer-tab-1", "buffer-tab-3", "buffer-tab-4"] {
            assert!(
                cx.debug_bounds(id).is_some(),
                "stale conditional-close queue removed a survivor"
            );
        }
    }

    #[gpui::test]
    fn close_left_clean_tabs_preserves_anchor_right_and_history(cx: &mut gpui::TestAppContext) {
        let files = tempfile::tempdir().expect("anchor savepoint");
        let anchor = files.path().join("anchor.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(NewFile);
        cx.dispatch_action(NewFile);
        cx.simulate_input("ancho");
        cx.dispatch_action(SaveFile);
        cx.simulate_new_path_selection(|_| Some(anchor.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(anchor).expect("saved anchor baseline"),
            b"ancho"
        );
        cx.simulate_input("r");
        cx.dispatch_action(NewFile);
        cx.simulate_input("right");
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(CloseLeftFiles);
        assert!(cx.debug_bounds("buffer-tab-1").is_none());
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
        assert!(cx.debug_bounds("buffer-tab-3").is_some());
        assert!(cx.debug_bounds("buffer-tab-4").is_some());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("anchor"));
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("selection");
        assert_eq!(selected.range, 6..6);
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(5)
        );
        cx.dispatch_action(EditorRedo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("anchor"));
    }

    #[gpui::test]
    fn readonly_convert_is_available_and_pending_save_blocks_further_commands(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("Convert directory");
        let original = home.path().join("readonly");
        let copy = home.path().join("writable");
        std::fs::write(&original, b"A").expect("readonly fixture");
        let mut permissions = std::fs::metadata(&original)
            .expect("metadata")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&original, permissions).expect("readonly mode");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![original.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(OpenEncodingMenu);
        let target = cx
            .debug_bounds("encoding-convert-utf16-le")
            .expect("readonly Convert action");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(std::fs::read(&original).expect("readonly bytes"), b"A");
        cx.dispatch_action(SaveFileAs);
        cx.dispatch_action(OpenEncodingMenu);
        assert!(cx.debug_bounds("encoding-convert-utf8").is_none());
        cx.simulate_new_path_selection(|_| Some(copy.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&copy).expect("LE new target"),
            [0xff, 0xfe, 0x41, 0]
        );
        cx.simulate_input("X");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(copy).expect("caret and writable state"),
            [0xff, 0xfe, 0x58, 0, 0x41, 0]
        );
        assert_eq!(std::fs::read(original).expect("readonly unchanged"), b"A");
    }

    #[gpui::test]
    fn encoding_convert_menu_clears_history_but_same_target_keeps_later_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A");
        cx.dispatch_action(EditorLeft);
        cx.dispatch_action(EditorRight);
        cx.simulate_input("B");
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(OpenEncodingMenu);
        let target = cx
            .debug_bounds("encoding-convert-utf16-be")
            .expect("Convert to UTF16 BE");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        let mut actual = None;
        for redo in [false, true] {
            if redo {
                cx.dispatch_action(EditorRedo);
            } else {
                cx.dispatch_action(EditorUndo);
            }
            let text = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
            });
            assert_eq!(text.as_deref(), Some("A"));
            assert_eq!(
                view.update_in(cx, EntityInputHandler::text_length_utf16),
                Some(1)
            );
        }
        view.read_with(cx, |view, _| {
            assert!(!view.editor().is_modified());
            assert!(view.workspace.active().is_modified());
        });
        cx.simulate_input("X");
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(OpenEncodingMenu);
        let target = cx
            .debug_bounds("encoding-convert-utf16-be")
            .expect("same target");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorRedo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("AX"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(2)
        );
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(1)
        );
        view.read_with(cx, |view, _| {
            assert!(!view.workspace.active().is_modified())
        });
    }

    #[gpui::test]
    fn encoding_choice_dirty_is_replaced_when_native_undo_reaches_text_savepoint(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenEncodingMenu);
        let target = cx
            .debug_bounds("encoding-utf16-le")
            .expect("UTF16 LE choice");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| assert!(view.workspace.active().is_modified()));
        cx.simulate_input("中😀");
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert!(!view.workspace.active().is_modified());
            assert_eq!(
                view.workspace.active().format(),
                crate::files::UnicodeFormat::Utf16LeBom
            );
            assert_eq!(view.editor().committed_text(), "");
        });
        cx.dispatch_action(EditorRedo);
        view.read_with(cx, |view, _| {
            assert!(view.workspace.active().is_modified());
            assert_eq!(view.editor().committed_text(), "中😀");
        });
    }

    #[gpui::test]
    fn encoding_selection_save_all_includes_metadata_only_files(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("encoding directory");
        let a = home.path().join("a");
        let b = home.path().join("b");
        std::fs::write(&a, b"A").expect("first fixture");
        std::fs::write(&b, b"B").expect("second fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        for (path, selector, format) in [
            (
                a.clone(),
                "encoding-utf16-le",
                crate::files::UnicodeFormat::Utf16LeBom,
            ),
            (
                b.clone(),
                "encoding-utf16-be",
                crate::files::UnicodeFormat::Utf16BeBom,
            ),
        ] {
            cx.dispatch_action(OpenFile);
            cx.simulate_path_prompt_response(|_| Some(vec![path]));
            cx.run_until_parked();
            cx.dispatch_action(OpenEncodingMenu);
            let choice = cx.debug_bounds(selector).expect("encoding choice");
            cx.simulate_click(choice.center(), gpui::Modifiers::default());
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().format(), format);
                assert!(view.workspace.active().is_modified());
            });
        }
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.dirty_buffers().len(), 2)
        });
        cx.dispatch_action(SaveAllFiles);
        let mut dialog = floating_context(cx, "save-all-yes");
        let yes = dialog
            .debug_bounds("save-all-yes")
            .expect("floating Save All confirmation");
        dialog.simulate_click(yes.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(a).expect("LE output"), [0xff, 0xfe, 0x41, 0]);
        assert_eq!(std::fs::read(b).expect("BE output"), [0xfe, 0xff, 0, 0x42]);
        cx.dispatch_action(CloseAllFiles);
        assert!(floating_bounds(cx, "close-cancel").is_none());
    }

    #[gpui::test]
    fn metadata_only_encoding_choice_saves_empty_bom_and_blocks_mutation_while_saving(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("encoding directory");
        let path = home.path().join("empty-bom");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let root = cx
            .debug_bounds("encoding-menu-trigger")
            .expect("Encoding root");
        cx.simulate_click(root.center(), gpui::Modifiers::default());
        let bom = cx
            .debug_bounds("encoding-utf8-bom")
            .expect("UTF-8 BOM choice");
        cx.simulate_click(bom.center(), gpui::Modifiers::default());
        cx.dispatch_action(SaveFile);
        cx.dispatch_action(OpenEncodingMenu);
        assert!(
            cx.debug_bounds("encoding-utf8").is_none(),
            "pending save chooser allowed encoding mutation"
        );
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(path).expect("empty format save"),
            [0xef, 0xbb, 0xbf]
        );
        cx.dispatch_action(CloseCurrentFile);
        assert!(floating_bounds(cx, "close-cancel").is_none());
    }

    #[gpui::test]
    fn alt_edit_release_keeps_popup_open_for_nested_keyboard_navigation(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(menu_surface::bindings()));
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("a\r\nb");
        cx.simulate_event(gpui::ModifiersChangedEvent {
            modifiers: gpui::Modifiers {
                alt: true,
                ..Default::default()
            },
            ..Default::default()
        });
        cx.simulate_keystrokes("alt-e");
        cx.simulate_event(gpui::ModifiersChangedEvent::default());
        assert!(
            cx.debug_bounds("menu-popup-0").is_some(),
            "Alt release preserves the actual Edit root"
        );
        assert!(cx.debug_bounds("edit-menu-eol").is_some());
        cx.simulate_keystrokes("e");
        assert!(
            cx.debug_bounds("edit-eol-lf").is_some(),
            "Alt release closed Edit before nested navigation"
        );
    }

    #[gpui::test]
    fn nested_keyboard_menu_fits_minimum_viewport_and_undo_routes_to_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(menu_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(480.), px(320.)));
        cx.run_until_parked();
        cx.simulate_input("a\r\nb");
        cx.simulate_keystrokes("alt-e end right");
        for id in ["edit-eol-crlf", "edit-eol-lf", "edit-eol-cr"] {
            let bounds = cx.debug_bounds(id).expect("nested choice visible");
            assert!(
                bounds.left() >= px(0.)
                    && bounds.right() <= px(480.)
                    && bounds.top() >= px(0.)
                    && bounds.bottom() <= px(320.),
                "popup escaped minimum viewport: {bounds:?}"
            );
        }
        cx.simulate_keystrokes("down enter");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("a\nb"));
        cx.simulate_keystrokes("alt-e home enter");
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("a\r\nb"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(4)
        );
    }

    #[gpui::test]
    fn readonly_disabled_submenu_click_does_not_activate_previous_select_all(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated readonly directory");
        let path = home.path().join("readonly.txt");
        std::fs::write(&path, b"a\r\nb").expect("readonly fixture");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).expect("readonly mode");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(OpenEditMenu);
        let parent = cx
            .debug_bounds("edit-menu-eol")
            .expect("disabled parent rendered");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("edit-eol-lf").is_none());
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("selection");
        assert_eq!(selected.range, 0..0);
        assert_eq!(std::fs::read(path).expect("readonly bytes"), b"a\r\nb");
    }

    #[gpui::test]
    fn nested_edit_menu_eol_conversion_preserves_caret_and_one_step_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("a\r\nb\rc\n中");
        let edit = cx
            .debug_bounds("edit-menu-trigger")
            .expect("Edit menu trigger");
        cx.simulate_click(edit.center(), gpui::Modifiers::default());
        let parent = cx.debug_bounds("edit-menu-eol").expect("EOL submenu");
        cx.simulate_click(parent.center(), gpui::Modifiers::default());
        let target = cx.debug_bounds("edit-eol-lf").expect("Unix conversion");
        cx.simulate_click(target.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..7, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("a\nb\nc\n中"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(7)
        );
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("selection");
        assert_eq!(selected.range, 7..7);
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..8, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("a\r\nb\rc\n中"));
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(8)
        );
    }

    #[gpui::test]
    fn enter_uses_loaded_eol_for_utf8_and_both_utf16_endians(cx: &mut gpui::TestAppContext) {
        let fixtures: [(&[u8], &[u8]); 4] = [
            (b"a\nb", b"a\nb\nnext"),
            (b"a\rb", b"a\rb\rnext"),
            (
                &[0xff, 0xfe, 0x61, 0, 0x0a, 0, 0x62, 0],
                &[
                    0xff, 0xfe, 0x61, 0, 0x0a, 0, 0x62, 0, 0x0a, 0, 0x6e, 0, 0x65, 0, 0x78, 0,
                    0x74, 0,
                ],
            ),
            (
                &[0xfe, 0xff, 0, 0x61, 0, 0x0d, 0, 0x62],
                &[
                    0xfe, 0xff, 0, 0x61, 0, 0x0d, 0, 0x62, 0, 0x0d, 0, 0x6e, 0, 0x65, 0, 0x78, 0,
                    0x74,
                ],
            ),
        ];
        for (original, expected) in fixtures {
            let home = tempfile::tempdir().expect("isolated EOL directory");
            let path = home.path().join("eol.txt");
            std::fs::write(&path, original).expect("EOL fixture");
            let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
            cx.dispatch_action(OpenFile);
            cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
            cx.run_until_parked();
            cx.dispatch_action(EditorDocumentEnd);
            cx.dispatch_action(EditorEnter);
            let caret = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("loaded EOL Enter caret");
            assert_eq!(caret.range, 4..4);
            cx.simulate_input("next");
            cx.dispatch_action(SaveFile);
            cx.run_until_parked();
            assert_eq!(std::fs::read(path).expect("saved EOL bytes"), expected);
            cx.dispatch_action(EditorUndo);
            let eol = if original.contains(&b'\n') {
                "\n"
            } else {
                "\r"
            };
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), format!("a{eol}b{eol}"))
            });
            cx.dispatch_action(EditorUndo);
            view.update_in(cx, |view, window, cx| {
                assert_eq!(view.editor().committed_text(), format!("a{eol}b"));
                assert_eq!(
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                        .expect("restored Enter caret")
                        .range,
                    3..3
                );
            });
        }
    }

    #[gpui::test]
    fn utf16_open_copy_save_as_reload_and_readonly_transitions_keep_formats(
        cx: &mut gpui::TestAppContext,
    ) {
        for (index, fixture, ax_bytes, axy_bytes) in [
            (
                0,
                vec![0xff, 0xfe, 0x41, 0],
                vec![0xff, 0xfe, 0x41, 0, 0x58, 0],
                vec![0xff, 0xfe, 0x41, 0, 0x58, 0, 0x59, 0],
            ),
            (
                1,
                vec![0xfe, 0xff, 0, 0x41],
                vec![0xfe, 0xff, 0, 0x41, 0, 0x58],
                vec![0xfe, 0xff, 0, 0x41, 0, 0x58, 0, 0x59],
            ),
        ] {
            let home = tempfile::tempdir().expect("isolated Unicode directory");
            let original = home.path().join(format!("original-{index}"));
            let copy = home.path().join("copy");
            let renamed = home.path().join("save-as");
            let writable = home.path().join("writable");
            std::fs::write(&original, &fixture).expect("UTF-16 fixture");
            let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
            cx.dispatch_action(OpenFile);
            cx.simulate_path_prompt_response(|_| Some(vec![original.clone()]));
            cx.run_until_parked();
            cx.dispatch_action(EditorDocumentEnd);
            cx.simulate_input("X");
            cx.dispatch_action(SaveCopyFile);
            cx.simulate_new_path_selection(|_| Some(copy.clone()));
            cx.run_until_parked();
            assert_eq!(std::fs::read(&copy).expect("Unicode copy"), ax_bytes);
            assert_eq!(std::fs::read(&original).expect("source unchanged"), fixture);
            cx.dispatch_action(CloseCurrentFile);
            assert!(floating_bounds(cx, "close-cancel").is_some());
            let mut close = floating_context(cx, "close-cancel");
            let cancel = close
                .debug_bounds("close-cancel")
                .expect("close cancel button");
            close.simulate_click(cancel.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert!(
                    !view.close_dialog,
                    "cancel must retire the close decision before saving"
                );
                assert!(!view.file_busy, "copy operation must have completed");
                assert_eq!(view.editor().committed_text(), "AX");
            });
            cx.dispatch_action(SaveFile);
            cx.run_until_parked();
            assert_eq!(std::fs::read(&original).expect("Unicode Save"), ax_bytes);
            cx.dispatch_action(SaveFileAs);
            cx.simulate_new_path_selection(|_| Some(renamed.clone()));
            cx.run_until_parked();
            cx.simulate_input("Y");
            cx.dispatch_action(SaveFile);
            cx.run_until_parked();
            assert_eq!(
                std::fs::read(&renamed).expect("Unicode As target"),
                axy_bytes
            );
            assert_eq!(
                std::fs::read(&original).expect("old target unchanged"),
                ax_bytes
            );
            std::fs::write(&renamed, [0xfe, 0xff, 0, 0x42]).expect("external encoding change");
            let mut permissions = std::fs::metadata(&renamed)
                .expect("readonly metadata")
                .permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&renamed, permissions).expect("readonly mode");
            cx.dispatch_action(ReloadFile);
            cx.run_until_parked();
            cx.simulate_input("ignored");
            let mut actual = None;
            let text = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
            });
            assert_eq!(text.as_deref(), Some("B"));
            assert_eq!(
                view.update_in(cx, EntityInputHandler::text_length_utf16),
                Some(1)
            );
            cx.dispatch_action(SaveFileAs);
            cx.simulate_new_path_selection(|_| Some(writable.clone()));
            cx.run_until_parked();
            cx.dispatch_action(EditorDocumentEnd);
            cx.simulate_input("!");
            cx.dispatch_action(SaveFile);
            cx.run_until_parked();
            assert_eq!(
                std::fs::read(writable).expect("writable new target"),
                [0xfe, 0xff, 0, 0x42, 0, 0x21]
            );
            assert_eq!(
                std::fs::read(renamed).expect("readonly source unchanged"),
                [0xfe, 0xff, 0, 0x42]
            );
        }
    }

    #[gpui::test]
    fn horizontal_caret_follow_preserves_unicode_ime_geometry(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(&"中😀".repeat(1024));
        let editor = cx.debug_bounds("matrix-editor").expect("editor viewport");
        let caret = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(
                    view,
                    3072..3072,
                    Bounds::default(),
                    window,
                    cx,
                )
            })
            .expect("Unicode caret bounds");
        assert!(caret.left() >= editor.left() && caret.right() <= editor.right());
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "拼", None, window, cx)
        });
        cx.run_until_parked();
        let caret = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(
                    view,
                    3073..3073,
                    Bounds::default(),
                    window,
                    cx,
                )
            })
            .expect("composition caret bounds");
        assert!(caret.left() >= editor.left() && caret.right() <= editor.right());
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::unmark_text(view, window, cx)
        });
        cx.run_until_parked();
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(3073)
        );
    }

    #[gpui::test]
    fn horizontal_wheel_preserves_selection_and_updates_native_hit_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(&"a".repeat(4096));
        cx.dispatch_action(EditorHome);
        let editor = cx.debug_bounds("matrix-editor").expect("editor viewport");
        let position = editor.origin + point(px(TEXT_LEFT), px(10.));
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(-200.), px(0.))),
            ..Default::default()
        });
        let index = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::character_index_for_point(view, position, window, cx)
            })
            .expect("visible hit");
        assert!(
            index > 0,
            "horizontal wheel did not change the visible text origin"
        );
        let selection = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("selection");
        assert_eq!(
            selection.range,
            0..0,
            "manual horizontal scroll moved the document caret"
        );
        let boundary = view.update_in(cx, |view, window, cx| {
            let bounds =
                EntityInputHandler::bounds_for_range(view, 32..32, Bounds::default(), window, cx)
                    .expect("fixed source byte boundary after scroll");
            assert!(editor.contains(&bounds.center()));
            assert_eq!(
                EntityInputHandler::character_index_for_point(view, bounds.center(), window, cx),
                Some(32)
            );
            bounds.center()
        });
        cx.simulate_click(boundary, gpui::Modifiers::default());
        cx.simulate_input("X");
        let mut actual = None;
        let text = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::text_for_range(view, 0..4097, &mut actual, window, cx)
            })
            .expect("full edited text");
        assert_eq!(text, format!("{}X{}", "a".repeat(32), "a".repeat(4064)));
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(33),
                    head: ByteOffset(33)
                }
            )
        });
    }

    #[gpui::test]
    fn long_line_end_and_home_caret_geometry_stay_inside_horizontal_viewport(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(&"a".repeat(4096));
        let editor = cx.debug_bounds("matrix-editor").expect("editor viewport");
        let caret = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(
                    view,
                    4096..4096,
                    Bounds::default(),
                    window,
                    cx,
                )
            })
            .expect("end caret geometry");
        assert!(
            caret.left() >= editor.left() && caret.right() <= editor.right(),
            "long-line end caret is outside the actual viewport: {caret:?} / {editor:?}"
        );
        cx.dispatch_action(EditorHome);
        let caret = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("home caret geometry");
        assert!(caret.left() >= editor.left() && caret.right() <= editor.right());
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(EditorSelectAll);
        cx.simulate_input("short");
        let beginning = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 0..0, Bounds::default(), window, cx)
            })
            .expect("replacement beginning");
        assert!(
            beginning.left() >= editor.left(),
            "replacing a long row left a short document scrolled out of view"
        );
        let fixture = tempfile::tempdir().expect("saved smart Home End source");
        let path = fixture.path().join("edges.txt");
        std::fs::write(&path, " \t中😀\r\nZ").expect("source indentation and CRLF");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentStart);
        let actions: [(Box<dyn gpui::Action>, usize, usize); 4] = [
            (Box::new(EditorHome), 2, 2),
            (Box::new(EditorHome), 0, 0),
            (Box::new(EditorSelectEnd), 0, 9),
            (Box::new(EditorSelectHome), 0, 2),
        ];
        for (action, anchor, head) in actions {
            cx.update(|window, cx| window.dispatch_action(action, cx));
            view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor().selection(),
                    Selection {
                        anchor: ByteOffset(anchor),
                        head: ByteOffset(head)
                    }
                );
                assert_eq!(view.editor().committed_text(), " \t中😀\r\nZ");
                assert!(!view.workspace.active().is_modified());
                assert!(!view.editor().document().can_undo());
            });
        }
        view.update_in(cx, |view, window, cx| {
            assert_eq!(
                EntityInputHandler::selected_text_range(view, false, window, cx)
                    .expect("native smartHome selection")
                    .range,
                0..2
            )
        });
        cx.dispatch_action(EditorDocumentEnd);
        cx.dispatch_action(EditorHome);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor().selection(),
                Selection {
                    anchor: ByteOffset(11),
                    head: ByteOffset(11)
                }
            );
            assert_eq!(view.editor().committed_text(), " \t中😀\r\nZ");
            assert!(!view.editor().document().can_undo() && !view.workspace.active().is_modified());
        });
    }

    #[gpui::test]
    fn save_copy_cancel_readonly_failure_and_same_primary_keep_document_identity(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated copy directory");
        let original = home.path().join("original.txt");
        let locked = home.path().join("locked.txt");
        std::fs::write(&original, b"A").expect("original fixture");
        std::fs::write(&locked, b"locked").expect("readonly fixture");
        let mut permissions = std::fs::metadata(&locked)
            .expect("locked metadata")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&locked, permissions).expect("readonly attribute");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![original.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveCopyFile);
        cx.simulate_new_path_selection(|_| None);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&original).expect("cancel did not write"),
            b"A"
        );
        cx.dispatch_action(SaveCopyFile);
        cx.simulate_new_path_selection(|_| Some(locked.clone()));
        cx.run_until_parked();
        assert!(cx.debug_bounds("editor-error").is_some());
        assert_eq!(
            std::fs::read(&locked).expect("readonly target untouched"),
            b"locked"
        );
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("AX"));
        cx.dispatch_action(SaveCopyFile);
        cx.simulate_new_path_selection(|_| Some(original.clone()));
        cx.run_until_parked();
        assert_eq!(std::fs::read(&original).expect("same-primary copy"), b"AX");
        cx.dispatch_action(CloseCurrentFile);
        assert!(
            floating_bounds(cx, "close-cancel").is_some(),
            "copy acknowledged savepoint"
        );
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        cx.dispatch_action(EditorLeft);
        cx.dispatch_action(EditorRight);
        cx.simulate_input("Y");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&original).expect("fresh baseline saves normally"),
            b"AXY"
        );
        cx.dispatch_action(EditorUndo);
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(2)
        );
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("AX"));
    }

    #[gpui::test]
    fn save_all_keyboard_no_does_not_save_or_hide_dirty_documents(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.bind_keys(MatrixWindow::file_dialog_bindings()));
        let home = tempfile::tempdir().expect("isolated save directory");
        let a = home.path().join("a.txt");
        let b = home.path().join("b.txt");
        std::fs::write(&a, b"A").expect("a fixture");
        std::fs::write(&b, b"B").expect("b fixture");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("Y");
        cx.dispatch_action(PreviousBuffer);
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        for keys in ["tab enter", "shift-tab enter", "right enter", "left enter"] {
            cx.dispatch_action(SaveAllFiles);
            floating_context(cx, "save-all-yes").simulate_keystrokes(keys);
            assert!(floating_bounds(cx, "save-all-yes").is_none());
            assert_eq!(std::fs::read(&a).expect("unsaved a"), b"A");
            assert_eq!(std::fs::read(&b).expect("unsaved b"), b"B");
        }
        cx.dispatch_action(SaveAllFiles);
        assert!(floating_bounds(cx, "save-all-yes").is_some());
        floating_context(cx, "save-all-yes").simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(std::fs::read(a).expect("saved a"), b"AX");
        assert_eq!(std::fs::read(b).expect("saved b"), b"BY");
    }

    #[gpui::test]
    fn close_dialog_keyboard_cancel_preserves_text_and_no_closes_only_target(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(MatrixWindow::file_dialog_bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first");
        cx.dispatch_action(NewFile);
        cx.simulate_input("second");
        cx.dispatch_action(CloseCurrentFile);
        floating_context(cx, "close-cancel").simulate_keystrokes("tab tab enter");
        assert!(floating_bounds(cx, "close-cancel").is_none());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("second"));
        cx.dispatch_action(CloseCurrentFile);
        floating_context(cx, "close-cancel").simulate_keystrokes("shift-tab enter");
        assert!(floating_bounds(cx, "close-cancel").is_none());
        cx.dispatch_action(CloseCurrentFile);
        floating_context(cx, "close-cancel").simulate_keystrokes("right enter");
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first"));
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
    }

    #[gpui::test]
    fn failed_reload_keeps_document_history_and_other_tabs(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated reload directory");
        let path = home.path().join("reload.txt");
        std::fs::write(&path, b"old").expect("fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("draft");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input(" edits");
        std::fs::write(&path, [0xff, 0x80]).expect("malformed external bytes");
        cx.dispatch_action(ReloadFile);
        floating_context(cx, "reload-yes").dispatch_action(ConfirmReload);
        cx.run_until_parked();
        assert!(cx.debug_bounds("editor-error").is_some());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("old edits"));
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..3, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("old"));
        cx.dispatch_action(PreviousBuffer);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("draft"));
        assert_eq!(
            std::fs::read(path).expect("external bytes untouched"),
            [0xff, 0x80]
        );
    }

    #[gpui::test]
    fn reload_confirmation_preserves_cancelled_edits_and_reloads_after_yes(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.bind_keys(MatrixWindow::reload_bindings()));
        let home = tempfile::tempdir().expect("isolated reload directory");
        let path = home.path().join("reload.txt");
        std::fs::write(&path, b"old").expect("fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input(" edits");
        std::fs::write(&path, "\u{feff}new😀").expect("external change");
        cx.dispatch_action(ReloadFile);
        floating_context(cx, "reload-no").simulate_keystrokes("tab enter");
        assert!(
            floating_bounds(cx, "reload-no").is_none(),
            "keyboard choice did not dismiss reload dialog"
        );
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("old edits"));
        for keys in ["shift-tab enter", "right enter", "left enter"] {
            cx.dispatch_action(ReloadFile);
            floating_context(cx, "reload-no").simulate_keystrokes(keys);
            assert!(floating_bounds(cx, "reload-no").is_none());
            let text = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_for_range(view, 0..9, &mut actual, window, cx)
            });
            assert_eq!(text.as_deref(), Some("old edits"));
        }
        cx.dispatch_action(ReloadFile);
        floating_context(cx, "reload-yes").dispatch_action(ConfirmReload);
        cx.run_until_parked();
        cx.dispatch_action(EditorUndo);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("new😀"));
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(path).expect("updated tracked target"),
            "\u{feff}new😀X".as_bytes()
        );
    }

    #[gpui::test]
    fn aborted_close_all_collision_does_not_close_tabs_on_later_ctrl_w(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let b = home.path().join("b.txt");
        let c = home.path().join("c.txt");
        std::fs::write(&b, b"B").expect("b fixture");
        std::fs::write(&c, b"C").expect("c fixture");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("unsaved A");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![b.clone(), c.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(CloseAllFiles);
        floating_context(cx, "close-save").dispatch_action(SaveAndClose);
        cx.simulate_new_path_selection(|_| Some(b.clone()));
        cx.run_until_parked();
        cx.dispatch_action(NewFile);
        cx.dispatch_action(CloseCurrentFile);
        assert!(
            cx.debug_bounds("buffer-tab-2").is_some() && cx.debug_bounds("buffer-tab-3").is_some(),
            "abandoned close queue removed unrelated tabs"
        );
        assert_eq!(std::fs::read(b).expect("b unchanged"), b"B");
        assert_eq!(std::fs::read(c).expect("c unchanged"), b"C");
    }

    #[gpui::test]
    fn close_all_can_cancel_remaining_tabs_without_closing_window(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("one");
        cx.dispatch_action(NewFile);
        cx.simulate_input("two");
        cx.dispatch_action(CloseAllFiles);
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("buffer-tab-1").is_some() && cx.debug_bounds("buffer-tab-2").is_some()
        );
        cx.dispatch_action(CloseAllFiles);
        floating_context(cx, "close-discard").dispatch_action(DiscardClose);
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        cx.run_until_parked();
        assert!(cx.debug_bounds("buffer-tab-1").is_some());
        assert!(
            cx.debug_bounds("buffer-tab-2").is_some(),
            "Close All preflights every decision before closing any tab"
        );
        view.read_with(cx, |view, _| {
            let texts: Vec<_> = view
                .workspace
                .buffers()
                .iter()
                .map(|buffer| buffer.editor.committed_text())
                .collect();
            assert_eq!(texts, ["one", "two"]);
        });
        cx.dispatch_action(CloseAllFiles);
        floating_context(cx, "close-discard").dispatch_action(DiscardClose);
        floating_context(cx, "close-discard").dispatch_action(DiscardClose);
        cx.run_until_parked();
        assert_eq!(
            view.update_in(cx, EntityInputHandler::text_length_utf16),
            Some(0)
        );
        assert!(cx.debug_bounds("matrix-editor").is_some());
    }

    #[gpui::test]
    fn close_current_cancel_and_discard_affect_only_target_buffer(cx: &mut gpui::TestAppContext) {
        let click_confirmation = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
            let mut dialog = floating_context(cx, selector);
            let button = dialog
                .debug_bounds(selector)
                .expect("actual Close Current decision");
            dialog.simulate_click(button.center(), gpui::Modifiers::default());
            cx.run_until_parked();
        };
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first");
        cx.dispatch_action(NewFile);
        cx.simulate_input("second");
        cx.dispatch_action(CloseCurrentFile);
        click_confirmation(cx, "close-cancel");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("second"));
        cx.dispatch_action(CloseCurrentFile);
        click_confirmation(cx, "close-discard");
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first"));
        assert!(cx.debug_bounds("buffer-tab-2").is_none());
        cx.dispatch_action(CloseCurrentFile);
        click_confirmation(cx, "close-discard");
        let length = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_length_utf16(view, window, cx)
        });
        assert_eq!(length, Some(0));
        assert!(
            cx.debug_bounds("matrix-editor").is_some(),
            "closing final tab closed the app window"
        );
    }

    #[gpui::test]
    fn save_copy_keeps_current_target_and_dirty_savepoint(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let original = home.path().join("original.txt");
        let copy = home.path().join("copy.txt");
        std::fs::write(&original, "\u{feff}A\r\n".as_bytes()).expect("original fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![original.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveCopyFile);
        cx.simulate_new_path_selection(|_| Some(copy.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&copy).expect("saved copy"),
            "\u{feff}A\r\nX".as_bytes()
        );
        assert_eq!(
            std::fs::read(&original).expect("original unchanged"),
            "\u{feff}A\r\n".as_bytes()
        );
        cx.dispatch_action(CloseWindow);
        assert!(
            floating_bounds(cx, "close-cancel").is_some(),
            "copy incorrectly marked edits saved"
        );
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        cx.dispatch_action(EditorLeft);
        cx.dispatch_action(EditorRight);
        cx.simulate_input("Y");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&original).expect("current target remains original"),
            "\u{feff}A\r\nXY".as_bytes()
        );
        assert_eq!(
            std::fs::read(&copy).expect("copy remains snapshot"),
            "\u{feff}A\r\nX".as_bytes()
        );
        cx.dispatch_action(EditorUndo);
        cx.dispatch_action(CloseWindow);
        assert!(
            floating_bounds(cx, "close-cancel").is_some(),
            "copy incorrectly marked edits saved"
        );
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("A\r\nX"));
    }

    #[gpui::test]
    fn rapid_save_all_confirmation_does_not_drop_queued_documents(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let a = home.path().join("a.txt");
        let b = home.path().join("b.txt");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first");
        cx.dispatch_action(NewFile);
        cx.simulate_input("second");
        cx.dispatch_action(SaveAllFiles);
        let mut dialog = floating_context(cx, "save-all-yes");
        let position = dialog
            .debug_bounds("save-all-yes")
            .expect("confirmation button")
            .center();
        dialog.update(|window, cx| {
            for _ in 0..2 {
                window.dispatch_event(
                    gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                        position,
                        button: MouseButton::Left,
                        click_count: 1,
                        first_mouse: false,
                        modifiers: gpui::Modifiers::default(),
                    }),
                    cx,
                );
                window.dispatch_event(
                    gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                        position,
                        button: MouseButton::Left,
                        click_count: 1,
                        modifiers: gpui::Modifiers::default(),
                    }),
                    cx,
                );
            }
        });
        cx.run_until_parked();
        cx.simulate_new_path_selection(|_| Some(a.clone()));
        cx.run_until_parked();
        cx.simulate_new_path_selection(|_| Some(b.clone()));
        cx.run_until_parked();
        assert_eq!(std::fs::read(a).expect("first queued save"), b"first");
        assert_eq!(std::fs::read(b).expect("second queued save"), b"second");
    }

    #[gpui::test]
    fn batch_save_blocks_mutating_keys_on_temporarily_selected_buffer(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first");
        cx.dispatch_action(NewFile);
        cx.simulate_input("second");
        cx.dispatch_action(SaveAllFiles);
        let mut dialog = floating_context(cx, "save-all-yes");
        let yes = dialog
            .debug_bounds("save-all-yes")
            .expect("actual Save All confirmation");
        dialog.simulate_click(yes.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.did_prompt_for_new_path());
        let commands: [Box<dyn gpui::Action>; 3] = [
            Box::new(EditorBackspace),
            Box::new(EditorUndo),
            Box::new(EditorEnter),
        ];
        for command in commands {
            cx.update(|window, cx| window.dispatch_action(command, cx));
            cx.run_until_parked();
            let length = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_length_utf16(view, window, cx)
            });
            assert_eq!(
                length,
                Some(5),
                "batch keyboard command edited another buffer"
            );
            let mut actual = None;
            let text = view.update_in(cx, |view, window, cx| {
                EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
            });
            assert_eq!(text.as_deref(), Some("first"));
        }
    }

    #[gpui::test]
    fn save_all_continues_after_one_conflict_without_overwriting_external_bytes(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let a = home.path().join("a.txt");
        let b = home.path().join("b.txt");
        std::fs::write(&a, b"A").expect("a fixture");
        std::fs::write(&b, b"B").expect("b fixture");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("Y");
        let first = cx.debug_bounds("buffer-tab-2").expect("a tab drawn");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        std::fs::write(&a, b"external").expect("external conflict");
        cx.dispatch_action(SaveAllFiles);
        let mut dialog = floating_context(cx, "save-all-yes");
        let yes = dialog
            .debug_bounds("save-all-yes")
            .expect("actual conflict-flow Save All approval");
        dialog.simulate_click(yes.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(&a).expect("external a"), b"external");
        assert_eq!(
            std::fs::read(&b).expect("saved b despite a conflict"),
            b"BY"
        );
        assert!(
            cx.debug_bounds("editor-error").is_some(),
            "batch save failure was silently hidden"
        );
    }

    #[gpui::test]
    fn save_all_writes_dirty_buffers_and_restores_active_document(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let a = home.path().join("a.txt");
        let b = home.path().join("b.txt");
        std::fs::write(&a, b"A").expect("a fixture");
        std::fs::write(&b, b"B").expect("b fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("Y");
        let first = cx.debug_bounds("buffer-tab-2").expect("a tab drawn");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        view.read_with(cx, |view, _| {
            let texts = view
                .workspace
                .buffers()
                .iter()
                .map(|buffer| buffer.editor.committed_text())
                .collect::<Vec<_>>();
            assert_eq!(texts, ["", "AX", "BY"]);
        });
        cx.dispatch_action(SaveAllFiles);
        let mut dialog = floating_context(cx, "save-all-yes");
        let confirm = dialog
            .debug_bounds("save-all-yes")
            .expect("floating Save All confirmation");
        dialog.simulate_click(confirm.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read(&a).expect("saved a"), b"AX");
        assert_eq!(std::fs::read(&b).expect("saved b"), b"BY");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("AX"));
    }

    #[gpui::test]
    fn save_as_refuses_another_open_buffer_and_preserves_both_documents(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let a = home.path().join("a.txt");
        let b = home.path().join("b.txt");
        std::fs::write(&a, b"A").expect("a fixture");
        std::fs::write(&b, b"B").expect("b fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
        cx.run_until_parked();
        let first = cx.debug_bounds("buffer-tab-2").expect("a tab drawn");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveFileAs);
        cx.simulate_new_path_selection(|_| Some(b.clone()));
        cx.run_until_parked();
        assert_eq!(std::fs::read(&b).expect("b unchanged"), b"B");
        assert_eq!(std::fs::read(&a).expect("a unchanged"), b"A");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..1, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("B"));
        let first = cx.debug_bounds("buffer-tab-2").expect("a tab preserved");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..2, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("AX"));
    }

    #[gpui::test]
    fn reference_frame_preserves_region_order_title_and_document_identity(
        app: &mut gpui::TestAppContext,
    ) {
        let fixture = tempfile::tempdir().expect("isolated reference frame");
        let a = fixture.path().join("frame-a.txt");
        let b = fixture.path().join("frame-b.txt");
        std::fs::write(&a, "A").expect("first reference document");
        std::fs::write(&b, "B").expect("second reference document");
        for dark in [false, true] {
            let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            cx.simulate_resize(size(px(1080.), px(668.)));
            cx.dispatch_action(OpenFile);
            cx.simulate_path_prompt_response(|_| Some(vec![a.clone(), b.clone()]));
            cx.run_until_parked();
            assert_eq!(
                cx.window_title().as_deref(),
                Some("frame-b.txt - Notepad Matrix")
            );
            let caption = cx.debug_bounds("window-close").expect("caption control");
            let menu = cx.debug_bounds("matrix-menu-bar").expect("menu bar");
            let toolbar = cx.debug_bounds("toolbar-new").expect("toolbar button");
            let toolbar_end = cx
                .debug_bounds("toolbar-macro-save")
                .expect("last toolbar button");
            let tabs = cx.debug_bounds("buffer-tabs").expect("document tabs");
            let editor = cx.debug_bounds("matrix-editor").expect("editor");
            let status = cx.debug_bounds("status-bar").expect("status bar");
            assert!(caption.top() >= px(0.) && caption.bottom() <= menu.top());
            assert!(menu.bottom() <= toolbar.top());
            assert_eq!(toolbar.top(), toolbar_end.top());
            assert!(toolbar.bottom() <= tabs.top());
            assert!(tabs.bottom() <= editor.top());
            assert!(editor.bottom() <= status.top());
            assert!(status.bottom() <= px(668.));
            for bounds in [caption, menu, toolbar, toolbar_end, tabs, editor, status] {
                assert!(bounds.left() >= px(0.) && bounds.right() <= px(1080.));
                assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
            }
            let first = cx.debug_bounds("buffer-tab-2").expect("first named tab");
            cx.simulate_click(first.center(), gpui::Modifiers::default());
            assert_eq!(
                cx.window_title().as_deref(),
                Some("frame-a.txt - Notepad Matrix")
            );
            let selection = view.update_in(cx, |view, _, _| {
                view.editor_mut().set_read_only(true);
                view.editor().selections()
            });
            cx.simulate_input("Y");
            assert_eq!(
                cx.window_title().as_deref(),
                Some("frame-a.txt - Notepad Matrix")
            );
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().id(), crate::workspace::BufferId(2));
                assert_eq!(view.editor().committed_text(), "A");
                assert_eq!(view.editor().selections(), selection);
                assert!(view.editor().is_read_only());
                assert!(
                    !view.editor().document().can_undo() && !view.workspace.active().is_modified()
                );
                let other = &view.workspace.buffers()[2];
                assert_eq!(other.editor.committed_text(), "B");
                assert!(!other.editor.is_read_only() && !other.is_modified());
            });
            view.update_in(cx, |view, _, _| view.editor_mut().set_read_only(false));
            cx.simulate_input("X");
            assert_eq!(
                cx.window_title().as_deref(),
                Some("*frame-a.txt - Notepad Matrix")
            );
            view.read_with(cx, |view, _| {
                assert_eq!(view.workspace.active().id(), crate::workspace::BufferId(2));
                let buffers = view.workspace.buffers();
                assert_eq!(buffers.len(), 3);
                assert_eq!(buffers[0].editor.committed_text(), "");
                assert_eq!(buffers[1].editor.committed_text(), "XA");
                assert_eq!(buffers[2].editor.committed_text(), "B");
                assert!(buffers[1].is_modified());
                assert!(!buffers[0].is_modified() && !buffers[2].is_modified());
            });
        }
    }

    #[gpui::test]
    fn resized_frame_keeps_editor_and_source_status_tail_visible(app: &mut gpui::TestAppContext) {
        for dark in [false, true] {
            let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            cx.simulate_input("A\t中😀\r\nZ");
            let before = view.read_with(cx, |view, _| {
                (
                    view.workspace.active().id(),
                    view.editor().committed_text(),
                    view.editor().selections(),
                    view.editor().document().can_undo(),
                    view.editor().document().can_redo(),
                    view.workspace.active().is_modified(),
                )
            });
            for viewport in [
                size(px(1080.), px(668.)),
                size(px(480.), px(320.)),
                size(px(1080.), px(668.)),
            ] {
                cx.simulate_resize(viewport);
                let editor = cx.debug_bounds("matrix-editor").expect("resized editor");
                let toolbar = cx
                    .debug_bounds("toolbar-new")
                    .expect("visible first toolbar control");
                let status = cx.debug_bounds("status-bar").expect("resized status bar");
                for bounds in [editor, toolbar, status] {
                    assert!(bounds.left() >= px(0.) && bounds.right() <= viewport.width);
                    assert!(bounds.top() >= px(0.) && bounds.bottom() <= viewport.height);
                    assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
                }
                assert!(toolbar.bottom() <= editor.top());
                assert!(editor.bottom() <= status.top());
                let mut right = status.right() - px(20.);
                for (id, width) in [
                    ("status-insert-mode", 45.),
                    ("status-encoding", 120.),
                    ("status-eol", 110.),
                    ("status-position", 260.),
                    ("status-document-size", 220.),
                ] {
                    let part = cx.debug_bounds(id).expect("source status field");
                    assert_eq!(part.size.width, px(width), "source width for {id}");
                    assert_eq!(part.right(), right, "source right anchor for {id}");
                    if matches!(id, "status-insert-mode" | "status-encoding" | "status-eol") {
                        assert!(part.left() >= status.left() && part.right() <= status.right());
                    }
                    right = part.left();
                }
                view.read_with(cx, |view, _| {
                    assert_eq!(
                        (
                            view.workspace.active().id(),
                            view.editor().committed_text(),
                            view.editor().selections(),
                            view.editor().document().can_undo(),
                            view.editor().document().can_redo(),
                            view.workspace.active().is_modified(),
                        ),
                        before,
                        "resizing only changes layout"
                    );
                });
            }
        }
    }

    #[gpui::test]
    fn caption_close_preserves_dirty_document_on_cancel_and_closes_clean_window(
        app: &mut gpui::TestAppContext,
    ) {
        for dark in [false, true] {
            let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            cx.simulate_input("dirty");
            let before = view.read_with(cx, |view, _| {
                (
                    view.workspace.active().id(),
                    view.editor().committed_text(),
                    view.editor().selections(),
                    view.editor().document().can_undo(),
                    view.editor().document().can_redo(),
                    view.workspace.active().is_modified(),
                )
            });
            assert_eq!(cx.windows().len(), 1);
            let close = cx.debug_bounds("window-close").expect("caption close");
            cx.simulate_click(close.center(), gpui::Modifiers::default());
            let mut dialog = floating_context(cx, "close-cancel");
            assert_eq!(cx.windows().len(), 2);
            cx.simulate_click(close.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            assert_eq!(
                cx.windows().len(),
                2,
                "caption cannot duplicate confirmation"
            );
            let cancel = dialog
                .debug_bounds("close-cancel")
                .expect("own Cancel control");
            dialog.simulate_click(cancel.center(), gpui::Modifiers::default());
            assert!(cx.debug_bounds("matrix-editor").is_some());
            cx.run_until_parked();
            assert_eq!(cx.windows().len(), 1);
            view.update_in(cx, |view, window, _| {
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(
                    (
                        view.workspace.active().id(),
                        view.editor().committed_text(),
                        view.editor().selections(),
                        view.editor().document().can_undo(),
                        view.editor().document().can_redo(),
                        view.workspace.active().is_modified(),
                    ),
                    before,
                    "cancelled window close preserves the original editor"
                );
            });
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "");
                assert!(!view.workspace.active().is_modified());
            });
            let close = cx
                .debug_bounds("window-close")
                .expect("clean caption close");
            cx.simulate_click(close.center(), gpui::Modifiers::default());
            cx.run_until_parked();
            assert!(
                cx.windows().is_empty(),
                "clean Close removes its own window"
            );
        }
    }

    #[gpui::test]
    fn toolbar_open_cancel_preserves_documents_history_and_prompt_identity(
        app: &mut gpui::TestAppContext,
    ) {
        for dark in [false, true] {
            let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
            cx.simulate_resize(size(px(1080.), px(668.)));
            cx.simulate_input("first");
            cx.dispatch_action(NewFile);
            cx.simulate_input("second");
            cx.dispatch_action(EditorDocumentStart);
            cx.simulate_input("X");
            cx.dispatch_action(EditorUndo);
            let snapshot = |view: &MatrixWindow| {
                (
                    view.workspace.active().id(),
                    view.workspace.active_view(),
                    view.workspace
                        .buffers()
                        .iter()
                        .map(|buffer| {
                            (
                                buffer.id(),
                                buffer.editor.committed_text(),
                                buffer.editor.selections(),
                                buffer.editor.document().can_undo(),
                                buffer.editor.document().can_redo(),
                                buffer.is_modified(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            };
            let before = view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "second");
                assert!(view.editor().document().can_undo());
                assert!(view.editor().document().can_redo());
                snapshot(view)
            });
            let windows = cx.windows().len();
            assert!(!cx.did_prompt_for_paths());
            assert!(!cx.did_prompt_for_new_path());
            assert!(!cx.has_pending_prompt());
            let open = cx
                .debug_bounds("toolbar-open")
                .expect("Open toolbar button");
            cx.simulate_click(open.center(), gpui::Modifiers::default());
            assert!(cx.did_prompt_for_paths());
            assert!(!cx.did_prompt_for_new_path());
            cx.simulate_click(open.center(), gpui::Modifiers::default());
            cx.simulate_path_prompt_response(|options| {
                assert!(options.files && options.multiple && !options.directories);
                None
            });
            cx.run_until_parked();
            assert!(cx.debug_bounds("matrix-editor").is_some());
            assert!(
                !cx.did_prompt_for_paths(),
                "only one Open picker was queued"
            );
            assert!(!cx.did_prompt_for_new_path());
            assert_eq!(cx.windows().len(), windows);
            assert!(!cx.has_pending_prompt());
            view.update_in(cx, |view, window, _| {
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(
                    snapshot(view),
                    before,
                    "Open cancellation preserves every editor"
                );
            });
            cx.dispatch_action(EditorRedo);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "Xsecond");
                assert_eq!(view.workspace.buffers()[0].editor.committed_text(), "first");
            });
            cx.dispatch_action(EditorUndo);
            view.read_with(cx, |view, _| assert_eq!(snapshot(view), before));
        }
    }

    #[gpui::test]
    fn toolbar_save_uses_active_target_and_preserves_history_across_savepoint(
        app: &mut gpui::TestAppContext,
    ) {
        for dark in [false, true] {
            for named in [false, true] {
                let fixture = tempfile::tempdir().expect("isolated toolbar Save fixture");
                let path = fixture.path().join("toolbar-save.txt");
                let (view, cx) =
                    app.add_window_view(|window, cx| MatrixWindow::new(dark, window, cx));
                cx.simulate_resize(size(px(1080.), px(668.)));
                cx.simulate_input("unrelated");
                if named {
                    std::fs::write(&path, "base").expect("named Save fixture");
                    cx.dispatch_action(OpenFile);
                    cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
                    cx.run_until_parked();
                } else {
                    cx.dispatch_action(NewFile);
                }
                cx.simulate_input("中😀");
                let expected = if named { "中😀base" } else { "中😀" };
                let before = view.read_with(cx, |view, _| {
                    (
                        view.workspace.active().id(),
                        view.editor().committed_text(),
                        view.editor().selections(),
                    )
                });
                assert_eq!(before.1, expected);
                let windows = cx.windows().len();
                assert!(!cx.did_prompt_for_paths() && !cx.did_prompt_for_new_path());
                assert!(!cx.has_pending_prompt());
                let save = cx.debug_bounds("toolbar-save").expect("active Save button");
                cx.simulate_click(save.center(), gpui::Modifiers::default());
                if !named {
                    assert!(cx.did_prompt_for_new_path());
                    assert!(!cx.did_prompt_for_paths());
                    cx.simulate_click(save.center(), gpui::Modifiers::default());
                    cx.simulate_new_path_selection(|_| None);
                    cx.run_until_parked();
                    assert!(!cx.did_prompt_for_new_path());
                    assert!(!path.exists(), "cancelled Save cannot create a file");
                    view.update_in(cx, |view, window, _| {
                        assert!(view.editor_focus.is_focused(window));
                        assert!(view.disk_file().is_none());
                        assert_eq!(
                            (
                                view.workspace.active().id(),
                                view.editor().committed_text(),
                                view.editor().selections(),
                            ),
                            before
                        );
                        assert!(view.workspace.active().is_modified());
                    });
                    cx.simulate_click(save.center(), gpui::Modifiers::default());
                    assert!(cx.did_prompt_for_new_path());
                    cx.simulate_new_path_selection(|_| Some(path.clone()));
                }
                cx.run_until_parked();
                assert_eq!(
                    std::fs::read(&path).expect("first Save bytes"),
                    expected.as_bytes()
                );
                assert!(!cx.did_prompt_for_paths() && !cx.did_prompt_for_new_path());
                assert!(!cx.has_pending_prompt());
                assert_eq!(cx.windows().len(), windows);
                assert_eq!(
                    cx.window_title().as_deref(),
                    Some("toolbar-save.txt - Notepad Matrix")
                );
                view.update_in(cx, |view, window, _| {
                    assert!(view.editor_focus.is_focused(window));
                    assert_eq!(view.workspace.active().id(), before.0);
                    assert_eq!(view.editor().selections(), before.2);
                    assert!(!view.workspace.active().is_modified());
                    assert_eq!(view.workspace.buffers().len(), 2);
                    let other = &view.workspace.buffers()[0];
                    assert_eq!(other.editor.committed_text(), "unrelated");
                    assert!(other.is_modified() && other.editor.document().can_undo());
                    assert_eq!(
                        view.disk_file().expect("saved document path").path(),
                        std::fs::canonicalize(&path).expect("canonical saved path")
                    );
                });
                cx.dispatch_action(EditorUndo);
                view.read_with(cx, |view, _| {
                    assert_eq!(
                        view.editor().committed_text(),
                        if named { "base" } else { "" }
                    );
                    assert!(view.workspace.active().is_modified());
                });
                cx.dispatch_action(EditorRedo);
                view.read_with(cx, |view, _| {
                    assert_eq!(view.editor().committed_text(), expected);
                    assert!(!view.workspace.active().is_modified());
                });
            }
        }
    }

    #[gpui::test]
    fn caption_controls_match_reference_icon_size_and_spacing(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let minimize = cx.debug_bounds("window-minimize").expect("minimize button");
        let maximize = cx.debug_bounds("window-maximize").expect("maximize button");
        let close = cx.debug_bounds("window-close").expect("close button");
        assert!((f32::from(maximize.center().x - minimize.center().x) - 45.).abs() <= 0.5);
        assert_eq!(close.center().x - maximize.center().x, px(46.));
        let width = cx.update(|window, _| window.viewport_size().width);
        for id in [
            "window-minimize-icon",
            "window-maximize-icon",
            "window-close-icon",
        ] {
            let icon = cx.debug_bounds(id).expect("fixed caption icon");
            assert_eq!(icon.size, size(px(10.), px(10.)));
            assert_eq!(icon.top(), px(11.));
        }
        let close_icon = cx.debug_bounds("window-close-icon").unwrap();
        assert_eq!(width - close_icon.right(), px(21.));
    }

    #[gpui::test]
    fn native_save_keeps_linked_file_aliases_and_document_savepoint(cx: &mut gpui::TestAppContext) {
        use std::os::unix::fs::MetadataExt;
        let home = tempfile::tempdir().expect("isolated linked document");
        let path = home.path().join("linked.txt");
        let alias = home.path().join("alias.txt");
        std::fs::write(&path, "A中").expect("linked fixture");
        std::fs::hard_link(&path, &alias).expect("hard link");
        let original = std::fs::metadata(&path).expect("original metadata");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&path).expect("saved primary"),
            "A中X".as_bytes()
        );
        assert_eq!(
            std::fs::read(&alias).expect("saved alias"),
            "A中X".as_bytes()
        );
        let saved = std::fs::metadata(&path).expect("saved metadata");
        assert_eq!((saved.dev(), saved.ino()), (original.dev(), original.ino()));
        view.read_with(cx, |view, _| {
            assert!(!view.workspace.active().is_modified());
            assert!(view.editor().document().can_undo());
            assert_eq!(
                view.disk_file().expect("disk file").path(),
                path.canonicalize().expect("canonical source")
            );
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert!(view.workspace.active().is_modified());
            assert_eq!(view.editor().committed_text(), "A中");
        });
    }

    #[gpui::test]
    fn save_as_preserves_bom_and_changes_target_without_touching_original(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let original = home.path().join("original.txt");
        let copy = home.path().join("copy.txt");
        std::fs::write(&original, "\u{feff}A\r\n中😀".as_bytes()).expect("BOM fixture");
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|_| Some(vec![original.clone()]));
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveFileAs);
        cx.simulate_new_path_selection(|_| Some(copy.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&copy).expect("saved copy"),
            "\u{feff}A\r\n中😀X".as_bytes()
        );
        assert_eq!(
            std::fs::read(&original).expect("original"),
            "\u{feff}A\r\n中😀".as_bytes()
        );
        cx.simulate_input("Y");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&copy).expect("current target"),
            "\u{feff}A\r\n中😀XY".as_bytes()
        );
        assert_eq!(
            std::fs::read(&original).expect("original untouched"),
            "\u{feff}A\r\n中😀".as_bytes()
        );
    }

    #[gpui::test]
    fn document_dropdown_selects_existing_documents_without_toggling_the_dock(
        app: &mut gpui::TestAppContext,
    ) {
        app.update(|cx| cx.bind_keys(menu_surface::bindings()));
        let (view, cx) = app.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first");
        cx.dispatch_action(NewFile);
        cx.simulate_input("second");
        let dropdown = cx
            .debug_bounds("menu-document-list")
            .expect("main-row dropdown");
        cx.simulate_click(dropdown.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert!(
                !view.document_list_visible,
                "dropdown is not the docked panel"
            );
            let entries = view.menu_bar.popup_entries(0).expect("document menu");
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.label())
                    .collect::<Vec<_>>(),
                ["new 1*", "new 2*"]
            );
        });
        assert!(cx.debug_bounds("document-dropdown-item-1").is_some());
        cx.simulate_keystrokes("escape");
        assert!(cx.debug_bounds("document-dropdown-item-1").is_none());
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window));
            assert_eq!(view.editor().committed_text(), "second");
        });
        cx.simulate_click(dropdown.center(), gpui::Modifiers::default());
        let first = cx
            .debug_bounds("document-dropdown-item-1")
            .expect("first document");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        assert!(cx.debug_bounds("document-dropdown-item-1").is_none());
        view.update_in(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window));
            assert_eq!(view.workspace.active().id(), crate::workspace::BufferId(1));
            assert_eq!(view.editor().committed_text(), "first");
            assert_eq!(
                view.workspace.buffers()[1].editor.committed_text(),
                "second"
            );
            assert!(!view.document_list_visible);
        });
        cx.dispatch_action(EditorUndo);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "");
            assert_eq!(
                view.workspace.buffers()[1].editor.committed_text(),
                "second"
            );
        });
        cx.simulate_click(dropdown.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("end enter");
        assert!(cx.debug_bounds("document-dropdown-item-1").is_none());
        view.read_with(cx, |view, _| {
            assert_eq!(view.workspace.active().id(), crate::workspace::BufferId(2));
            assert_eq!(view.editor().committed_text(), "second");
        });
    }

    #[gpui::test]
    fn file_menu_new_uses_the_same_document_route_as_shortcut(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.bind_keys(menu_surface::bindings()));
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(1080.), px(668.)));
        let roots = [
            "File", "Edit", "Search", "View", "Encoding", "Language", "Settings", "Tools", "Macro",
            "Run", "Plugins", "Window", "Help",
        ];
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.menu_bar
                    .definitions()
                    .iter()
                    .map(|root| root.label)
                    .collect::<Vec<_>>(),
                roots
            );
        });
        let bar = cx.debug_bounds("matrix-menu-bar").expect("main menu bar");
        let mut edge = bar.left();
        let selectors = [
            "file-menu-trigger",
            "edit-menu-trigger",
            "search-menu-trigger",
            "view-menu-trigger",
            "encoding-menu-trigger",
            "language-menu-trigger",
            "settings-menu-trigger",
            "tools-menu-trigger",
            "macro-menu-trigger",
            "run-menu-trigger",
            "plugins-menu-trigger",
            "window-menu-trigger",
            "help-menu-trigger",
        ];
        for selector in selectors {
            let root = cx.debug_bounds(selector).expect("runtime root drawn");
            assert!(
                root.left() >= edge && root.right() > root.left() && root.right() <= bar.right()
            );
            assert!(root.top() >= bar.top() && root.bottom() <= bar.bottom());
            edge = root.right();
        }
        cx.simulate_input("first");
        let snapshot = |view: &MatrixWindow| {
            (
                view.workspace.active().id(),
                view.workspace.active_view(),
                view.editor().committed_text(),
                view.editor().selections(),
                view.editor().document().can_undo(),
                view.editor().document().can_redo(),
                view.workspace.active().is_modified(),
            )
        };
        let before = view.read_with(cx, |view, _| snapshot(view));
        for (index, selector) in selectors.into_iter().enumerate() {
            let root = cx.debug_bounds(selector).expect("menu root");
            cx.simulate_click(root.center(), gpui::Modifiers::default());
            assert!(cx.debug_bounds("menu-popup-0").is_some());
            let selected = view.update_in(cx, |view, window, _| {
                assert_eq!(view.menu_bar.active_root(), index);
                assert_eq!(view.menu_bar.mode(), MenuMode::Open);
                assert!(view.menu_focus.is_focused(window));
                view.menu_bar.selected_path().to_vec()
            });
            cx.simulate_keystrokes("down");
            view.read_with(cx, |view, _| {
                if matches!(index, 8 | 9 | 12) {
                    assert_eq!(
                        view.menu_bar.selected_path(),
                        [0],
                        "one available row wraps in {selector}"
                    );
                    if index == 8 {
                        assert!(
                            view.macro_steps.is_empty()
                                && view.macro_dialogs.library.macros.is_empty()
                        );
                    }
                } else {
                    assert_ne!(
                        view.menu_bar.selected_path(),
                        selected,
                        "Down navigates {selector}"
                    );
                }
                assert_eq!(snapshot(view), before, "menu owns Down for {selector}");
            });
            cx.simulate_keystrokes("escape escape");
            view.update_in(cx, |view, window, _| {
                assert_eq!(view.menu_bar.mode(), MenuMode::Closed);
                assert!(view.editor_focus.is_focused(window));
                assert_eq!(snapshot(view), before);
            });
        }
        let file = cx.debug_bounds("file-menu-trigger").expect("File root");
        let edit = cx.debug_bounds("edit-menu-trigger").expect("Edit root");
        cx.simulate_click(file.center(), gpui::Modifiers::default());
        cx.simulate_mouse_move(edit.center(), None, gpui::Modifiers::default());
        view.read_with(cx, |view, _| assert_eq!(view.menu_bar.active_root(), 1));
        cx.simulate_click(edit.center(), gpui::Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu_bar.mode(), MenuMode::Closed)
        });
        cx.simulate_click(file.center(), gpui::Modifiers::default());
        cx.simulate_click(gpui::point(px(1070.), px(60.)), gpui::Modifiers::default());
        view.update_in(cx, |view, window, _| {
            assert_eq!(view.menu_bar.mode(), MenuMode::Closed);
            assert!(view.editor_focus.is_focused(window));
            assert_eq!(snapshot(view), before);
        });
        cx.dispatch_action(OpenFileMenu);
        let item = cx
            .debug_bounds("file-menu-new")
            .expect("New menu command drawn");
        cx.simulate_click(item.center(), gpui::Modifiers::default());
        cx.simulate_input("second");
        let first = cx.debug_bounds("buffer-tab-1").expect("first tab drawn");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first"));
        cx.simulate_input("!");
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "first!")
        });
    }

    #[gpui::test]
    fn close_cancel_reveals_dirty_tab_selected_by_close_guard(cx: &mut gpui::TestAppContext) {
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(400.), px(320.)));
        cx.simulate_input("first");
        for _ in 0..7 {
            cx.dispatch_action(NewFile);
        }
        cx.dispatch_action(CloseWindow);
        cx.dispatch_action(CancelClose);
        let first = cx.debug_bounds("buffer-tab-1").expect("first tab drawn");
        let strip = cx.debug_bounds("buffer-tabs").expect("tab strip drawn");
        assert!(
            first.left() >= strip.left() && first.right() <= strip.right(),
            "close guard selected an invisible document tab"
        );
    }

    #[gpui::test]
    fn neighboring_tab_commands_wrap_and_reveal_target(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(400.), px(320.)));
        cx.simulate_input("first");
        for _ in 0..7 {
            cx.dispatch_action(NewFile);
        }
        cx.simulate_input("last");
        cx.dispatch_action(NextBuffer);
        let first = cx.debug_bounds("buffer-tab-1").expect("first tab drawn");
        let strip = cx.debug_bounds("buffer-tabs").expect("tab strip drawn");
        assert!(first.left() >= strip.left() && first.right() <= strip.right());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first"));
        cx.dispatch_action(PreviousBuffer);
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..4, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("last"));
    }

    #[gpui::test]
    fn newly_active_tab_is_visible_after_tab_strip_overflows(cx: &mut gpui::TestAppContext) {
        let (_view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_resize(size(px(400.), px(320.)));
        for _ in 0..7 {
            cx.dispatch_action(NewFile);
        }
        let strip = cx.debug_bounds("buffer-tabs").expect("tab strip drawn");
        let last = cx.debug_bounds("buffer-tab-8").expect("last tab drawn");
        assert!(
            last.left() >= strip.left() && last.right() <= strip.right(),
            "active tab is unreachable outside the window"
        );
    }

    #[gpui::test]
    fn open_action_keeps_draft_and_saves_loaded_utf8_bom(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let path = home.path().join("opened.txt");
        std::fs::write(&path, "\u{feff}A\r\n中😀".as_bytes()).expect("UTF-8 BOM fixture");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("draft");
        cx.dispatch_action(OpenFile);
        cx.simulate_path_prompt_response(|options| {
            assert!(options.files && options.multiple && !options.directories);
            Some(vec![path.clone()])
        });
        cx.run_until_parked();
        cx.dispatch_action(EditorDocumentEnd);
        cx.simulate_input("X");
        cx.dispatch_action(SaveFile);
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&path).expect("saved BOM file"),
            "\u{feff}A\r\n中😀X".as_bytes()
        );
        let first = cx.debug_bounds("buffer-tab-1").expect("draft tab drawn");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("draft"));
    }

    #[gpui::test]
    fn new_and_tab_click_switch_real_editor_without_losing_history(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("first");
        cx.dispatch_action(NewFile);
        cx.simulate_input("second");
        let first = cx.debug_bounds("buffer-tab-1").expect("first tab drawn");
        cx.simulate_click(first.center(), gpui::Modifiers::default());
        cx.dispatch_action(EditorSelectAll);
        cx.simulate_input("X");
        cx.dispatch_action(EditorUndo);
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..5, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("first"));
        let second = cx.debug_bounds("buffer-tab-2").expect("second tab drawn");
        cx.simulate_click(second.center(), gpui::Modifiers::default());
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..6, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("second"));
    }

    #[gpui::test]
    fn popup_exit_does_not_click_through_to_document(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("alpha\r\n中文😀XY");
        cx.dispatch_action(OpenFileMenu);
        let bounds = cx.debug_bounds("file-menu-exit").expect("file popup drawn");
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("native selection");
        assert_eq!(
            selected.range,
            13..13,
            "popup click changed document selection"
        );
        floating_context(cx, "close-cancel").dispatch_action(CancelClose);
        cx.simulate_input("Z");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..14, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("alpha\r\n中文😀XYZ"));
    }

    #[gpui::test]
    fn cancel_after_save_conflict_preserves_caret(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let path = home.path().join("saved.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("alpha\r\n中文😀X");
        cx.dispatch_action(SaveFile);
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        cx.simulate_input("Y");
        std::fs::write(&path, b"external").expect("external fixture edit");
        cx.dispatch_action(CloseWindow);
        cx.dispatch_action(SaveAndClose);
        cx.run_until_parked();
        let mut dialog = floating_context(cx, "close-cancel");
        let bounds = dialog
            .debug_bounds("close-cancel")
            .expect("close dialog cancel drawn");
        dialog.simulate_click(bounds.center(), gpui::Modifiers::default());
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("native selection");
        assert_eq!(selected.range, 13..13);
        cx.simulate_input("Z");
        let mut actual = None;
        let text = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 0..14, &mut actual, window, cx)
        });
        assert_eq!(text.as_deref(), Some("alpha\r\n中文😀XYZ"));
    }

    #[gpui::test]
    fn save_action_writes_committed_text_after_real_path_selection(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("isolated test directory");
        let path = home.path().join("saved.txt");
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("committed中😀\r\n");
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(
                view, None, "preview", None, window, cx,
            )
        });
        cx.dispatch_action(SaveFile);
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&path).expect("saved file"),
            "committed中😀\r\n".as_bytes()
        );
    }

    #[gpui::test]
    fn vertical_document_boundaries_move_to_endpoints(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("ABCDE");
        let cases: [(Box<dyn gpui::Action>, Range<usize>, bool); 4] = [
            (Box::new(EditorUp), 0..0, false),
            (Box::new(EditorDown), 5..5, false),
            (Box::new(EditorSelectUp), 0..2, true),
            (Box::new(EditorSelectDown), 2..5, false),
        ];
        for (action, expected, reversed) in cases {
            view.update_in(cx, |view, window, cx| {
                EntityInputHandler::set_selected_text_range(view, 2..2, window, cx)
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.dispatch_action(action, cx));
            cx.run_until_parked();
            let selected = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("native selection");
            assert_eq!(selected.range, expected);
            assert_eq!(selected.reversed, reversed);
        }
    }

    #[gpui::test]
    fn click_resets_preferred_x_even_when_caret_does_not_change(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("123456\r\nx\r\nABCDEF");
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::set_selected_text_range(view, 5..5, window, cx)
        });
        cx.run_until_parked();
        cx.dispatch_action(EditorDown);
        let bounds = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::bounds_for_range(view, 9..9, Bounds::default(), window, cx)
            })
            .expect("short row caret bounds");
        cx.simulate_click(
            bounds.origin + point(px(0.), px(10.)),
            gpui::Modifiers::default(),
        );
        cx.dispatch_action(EditorDown);
        let selected = view
            .update_in(cx, |view, window, cx| {
                EntityInputHandler::selected_text_range(view, false, window, cx)
            })
            .expect("native selection");
        assert_eq!(
            selected.range,
            12..12,
            "click retained the previous desired x"
        );
    }

    #[gpui::test]
    fn vertical_navigation_keeps_preferred_x_across_short_rows(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("123456\r\nx\r\nABCDEF");
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::set_selected_text_range(view, 5..5, window, cx)
        });
        cx.run_until_parked();
        let cases: [(Box<dyn gpui::Action>, usize); 4] = [
            (Box::new(EditorDown), 9),
            (Box::new(EditorDown), 16),
            (Box::new(EditorUp), 9),
            (Box::new(EditorUp), 5),
        ];
        for (action, expected) in cases {
            cx.update(|window, cx| window.dispatch_action(action, cx));
            cx.run_until_parked();
            let selected = view
                .update_in(cx, |view, window, cx| {
                    EntityInputHandler::selected_text_range(view, false, window, cx)
                })
                .expect("native selection");
            assert_eq!(selected.range, expected..expected);
            view.read_with(cx, |view, _| {
                assert_eq!(view.editor().committed_text(), "123456\r\nx\r\nABCDEF");
                assert!(
                    view.editor().document().can_undo() && !view.editor().document().can_redo()
                );
            });
        }
    }

    #[gpui::test]
    fn no_effect_native_callbacks_preserve_manual_viewport(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input(
            &(0..100)
                .map(|i| format!("line{i:03}"))
                .collect::<Vec<_>>()
                .join("\r\n"),
        );
        let position = cx
            .debug_bounds("matrix-editor")
            .expect("editor drawn")
            .origin
            + point(px(4.), px(10.));
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Lines(point(0., 200.)),
            ..Default::default()
        });
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::unmark_text(view, window, cx)
        });
        cx.run_until_parked();
        let after_unmark = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::character_index_for_point(view, position, window, cx)
        });
        assert_eq!(after_unmark, Some(0), "no-op unmark moved the viewport");
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::set_selected_text_range(view, usize::MAX..usize::MAX, window, cx)
        });
        cx.run_until_parked();
        let after_failure = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::character_index_for_point(view, position, window, cx)
        });
        assert_eq!(
            after_failure,
            Some(0),
            "rejected selection moved the viewport"
        );
    }

    #[gpui::test]
    fn wheel_scroll_moves_viewport_without_moving_selection(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let fixture = (0..100)
            .map(|i| format!("line{i:03}"))
            .collect::<Vec<_>>()
            .join("\r\n");
        cx.simulate_input(&fixture);
        let bounds = cx.debug_bounds("matrix-editor").expect("editor drawn");
        let selected = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::selected_text_range(view, false, window, cx)
        });
        let position = bounds.origin + point(px(4.), px(10.));
        cx.simulate_event(gpui::ScrollEvent {
            position,
            delta: gpui::ScrollDelta::Lines(point(0., 200.)),
            ..Default::default()
        });
        let byte = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::character_index_for_point(view, position, window, cx)
        });
        assert_eq!(byte, Some(0), "wheel did not reveal first document row");
        let after = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::selected_text_range(view, false, window, cx)
        });
        assert_eq!(
            after.map(|s| (s.range, s.reversed)),
            selected.map(|s| (s.range, s.reversed))
        );
    }

    #[gpui::test]
    fn invalid_native_query_does_not_display_edit_error(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("😀");
        assert!(cx.debug_bounds("editor-error").is_none());
        let mut actual = None;
        let result = view.update_in(cx, |view, window, cx| {
            EntityInputHandler::text_for_range(view, 1..1, &mut actual, window, cx)
        });
        assert_eq!(result, None);
        assert_eq!(actual, None);
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("editor-error").is_none(),
            "read-only native query displayed an edit error"
        );
    }
}

impl EntityInputHandler for MatrixWindow {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        // Alt is the Windows menu modifier, not macOS Option character insertion.
        !self.file_busy
            && !self.editor().document().is_read_only()
            && !window.modifiers().alt
            && !window.modifiers().control
            && self.editor_focus.is_focused(window)
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.editor().display_snapshot();
        *actual = None;
        if range.start > range.end
            || range.start > text.len_utf16()
            || (range.start == text.len_utf16() && !range.is_empty())
        {
            return None;
        }
        let range = range.start..range.end.min(text.len_utf16());
        let bytes = byte_range(&text, range.clone()).ok()?;
        let content = self.native_query(
            text.try_slice(bytes.start.0..bytes.end.0)
                .map(|text| text.to_string())
                .map_err(|error| EditError::Storage(error.to_string())),
            cx,
        )?;
        *actual = Some(range);
        Some(content)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.editor().display_snapshot();
        let selected = self.editor().selection();
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
        let result = byte_range(&self.editor().display_snapshot(), range).and_then(|range| {
            self.editor_mut().select_display(Selection {
                anchor: range.start,
                head: range.end,
            })
        });
        self.editor_changed(result, window, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.editor().display_snapshot().len_utf16())
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        // GPUI routes marked input to AppKit before key bindings. Suspend that
        // priority for Windows-style Ctrl commands without changing the model;
        // the command itself decides whether to cancel or retain composition.
        if window.modifiers().control {
            return None;
        }
        let text = self.editor().display_snapshot();
        self.editor()
            .marked_range()
            .map(|range| text.byte_to_utf16_idx(range.start.0)..text.byte_to_utf16_idx(range.end.0))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_editing_blocked() {
            return;
        }
        let composing = !self.editor().marked_ranges().is_empty();
        let result = self.editor_mut().commit_composition();
        let committed = result.is_ok() && composing;
        self.editor_changed(result, window, cx);
        if committed {
            self.on_editor_input(window, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.batch_editing_blocked() {
            return;
        }
        if range.is_some() {
            self.record_macro_step(MacroStep::Unsupported("native replacement ranges"));
        } else {
            self.record_macro_text(text);
        }
        let result = if let Some(range) = range {
            byte_range(&self.editor().display_snapshot(), range)
                .and_then(|range| self.editor_mut().replace_display_range(range, text))
        } else {
            self.commit_editor_text(text, true)
        };
        let inserted = result.is_ok() && !text.is_empty();
        self.editor_changed(result, window, cx);
        if inserted {
            self.on_editor_input(window, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.batch_editing_blocked() {
            return;
        }
        let result = range
            .map(|range| byte_range(&self.editor().display_snapshot(), range))
            .transpose()
            .and_then(|range| {
                self.editor_mut()
                    .mark_inline_display_range(range, text, selected)
            });
        self.editor_changed(result, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = byte_range(&self.editor().display_snapshot(), range).ok()?;
        self.range_rectangles(range).into_iter().next()
    }

    fn selection_bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        byte_range(&self.editor().display_snapshot(), range)
            .map(|range| self.range_rectangles(range))
            .unwrap_or_default()
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.editor_bounds?.contains(&position) {
            return None;
        }
        let byte = self.hit_editor(position)?;
        let text = self.editor().display_snapshot();
        if byte.0 > text.len() || !text.is_char_boundary(byte.0) {
            self.notify_editor_error(
                "editor geometry contains an invalid character position".into(),
                cx,
            );
            return None;
        }
        Some(text.byte_to_utf16_idx(byte.0))
    }
}

struct EditorElement {
    view: Entity<MatrixWindow>,
    pane: crate::workspace::ViewId,
}
pub(super) struct EditorPaint {
    lines: Vec<PaintedLine>,
    revision: crate::input::DisplayRevision,
    top_line: usize,
    horizontal_offset: Pixels,
    content_width: Pixels,
    layout: Arc<EditorLayout>,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPaint;
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
        style.size.height = relative(1.).into();
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
    ) -> EditorPaint {
        self.view.update(cx, |view, _cx| {
            let previous = view.pane;
            view.scope_editor_pane(self.pane);
            view.editor_chrome.text_system = Some(window.text_system().clone());
            view.refresh_editor_syntax();
            view.capture_udl_text_system(window);
            let text = view.editor().display_snapshot();
            let mut lines = Vec::new();
            let text_origin = view.editor_text_origin();
            let text_inset = view.editor_text_inset();
            let text_width = (bounds.size.width - text_inset - px(SCROLLBAR_SIZE)).max(px(1.));
            let layout = view.editor_layout_for_width(text_width, window);
            let mut content_width = if view.editor_word_wrap_enabled() {
                (text_width - px(1.)).max(px(0.))
            } else {
                layout.content_width
            };
            let mut top = bounds.top() + px(TEXT_TOP);
            let line_height = view.editor_line_height();
            let capacity =
                ((f32::from(bounds.size.height) - SCROLLBAR_SIZE - TEXT_VERTICAL_PADDING)
                    / f32::from(line_height))
                .floor()
                .max(1.) as usize;
            let caret_line = layout.rows.row_for_byte(view.editor().selection().head.0);
            let anchor = view.editor_chrome.top_anchor.or_else(|| {
                view.editor_chrome
                    .layout
                    .as_ref()
                    .filter(|old| old.key.width != layout.key.width)
                    .and_then(|old| old.rows.rows().get(view.editor_top_line))
                    .map(|row| row.range.start)
            });
            let requested_top = if let Some(position) = &view.editor_chrome.pending_viewport {
                layout.rows.restored_top(
                    position.first_visible_line,
                    position.wrap_offset,
                    position.wrap_count,
                    view.editor_word_wrap_enabled(),
                )
            } else {
                anchor.map_or(view.editor_top_line, |byte| layout.rows.row_for_byte(byte))
            };
            let rows = crate::editor::visible_rows(
                requested_top,
                capacity,
                if view.editor_follow_caret {
                    caret_line
                } else {
                    requested_top
                },
                layout.rows.len(),
            );
            let top_line = rows.start;
            for index in rows {
                if top >= bounds.bottom() {
                    break;
                }
                let visual = &layout.rows.rows()[index];
                let logical = visual.logical_line;
                let raw = text.line(logical, LineType::LF_CR).to_string();
                let shape = layout.shapes[logical].clone();
                let logical_start = layout.starts[logical];
                let start = visual.range.start;
                let end = visual.range.end;
                let fragment_x = shape.x_for_index(start - logical_start);
                let fragment_width = if visual.last {
                    shape.width() - fragment_x
                } else {
                    shape.x_for_index(end - logical_start) - fragment_x
                };
                let origin = point(
                    bounds.left() + text_origin + px(visual.indent) - fragment_x,
                    top,
                );
                let clip = Bounds::new(
                    point(bounds.left() + text_origin + px(visual.indent), top),
                    size(
                        if visual.last {
                            fragment_width.max(px(0.)) + px(1.)
                        } else {
                            fragment_width.max(px(1.))
                        },
                        line_height,
                    ),
                );
                let space_width = window
                    .text_system()
                    .shape_line(
                        " ".into(),
                        view.editor_font_size(),
                        &[TextRun {
                            len: 1,
                            font: gpui::font(view.editor_font_family()),
                            color: rgb(0x808080).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width();
                let guides = if view.editor_indent_guides_shown() && visual.first {
                    let blank = |line: &str| line.trim_matches([' ', '\t', '\r', '\n']).is_empty();
                    let previous = if blank(&raw) {
                        (logical.saturating_sub(20)..logical)
                            .rev()
                            .map(|row| text.line(row, LineType::LF_CR).to_string())
                            .find(|line| !blank(line))
                    } else {
                        None
                    };
                    let next = if blank(&raw) {
                        (logical + 1..(logical + 21).min(text.len_lines(LineType::LF_CR)))
                            .map(|row| text.line(row, LineType::LF_CR).to_string())
                            .find(|line| !blank(line))
                    } else {
                        None
                    };
                    (if blank(&raw) {
                        chrome::indent_guides_with_context(
                            &raw,
                            previous.as_deref(),
                            next.as_deref(),
                        )
                    } else {
                        chrome::indent_guide_columns(&raw)
                    })
                    .into_iter()
                    .map(|column| space_width * column as f32)
                    .collect()
                } else {
                    Vec::new()
                };
                let whitespace = if view.editor_symbols.whitespace {
                    raw.trim_end_matches(['\r', '\n'])
                        .char_indices()
                        .filter(|&(byte, ch)| {
                            matches!(ch, ' ' | '\t')
                                && byte + logical_start >= start
                                && byte + logical_start < end
                        })
                        .map(|(byte, ch)| {
                            (
                                shape.x_for_index(byte),
                                shape.x_for_index(byte + ch.len_utf8()),
                                ch == '\t',
                            )
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                lines.push(PaintedLine {
                    start,
                    end,
                    logical_start,
                    logical_line: logical,
                    first: visual.first,
                    last: visual.last,
                    origin,
                    shape,
                    clip,
                    guides,
                    whitespace,
                    space_width,
                });
                top += line_height;
            }
            if !view.editor_word_wrap_enabled() {
                for selected in view.editor_display_selections().ranges() {
                    for (byte, virtual_space) in [
                        (selected.selection.anchor.0, selected.anchor_virtual),
                        (selected.selection.head.0, selected.head_virtual),
                    ] {
                        if let Some(line) = lines.iter().find(|line| line.owns_caret(byte)) {
                            content_width = content_width.max(
                                line.x_for_byte(byte) + line.space_width * virtual_space as f32,
                            );
                        }
                    }
                }
            }
            let mut horizontal_offset = if view.editor_word_wrap_enabled() {
                px(0.)
            } else {
                view.editor_horizontal_offset
            };
            if view.editor_follow_caret && !view.editor_word_wrap_enabled() {
                let caret = view.editor().selection().head.0;
                if let Some(line) = lines.iter().find(|line| line.owns_caret(caret)) {
                    let x = line.x_for_byte(caret)
                        + line.space_width
                            * view.editor_display_selections().primary().head_virtual as f32;
                    let width = (bounds.size.width - text_inset - px(SCROLLBAR_SIZE)).max(px(1.));
                    if x < horizontal_offset {
                        horizontal_offset = x;
                    } else if x - horizontal_offset >= width {
                        horizontal_offset = (x - width + px(1.)).max(px(0.));
                    }
                }
            }
            horizontal_offset = px(ScrollAxis::new(
                0.,
                f32::from(content_width) + 1.,
                (f32::from(bounds.size.width - text_inset) - SCROLLBAR_SIZE).max(1.),
                f32::from(horizontal_offset),
            )
            .clamped_offset());
            for line in &mut lines {
                line.origin.x -= horizontal_offset;
                line.clip.origin.x -= horizontal_offset;
            }
            let paint = EditorPaint {
                lines,
                revision: view.editor().display_revision(),
                top_line,
                horizontal_offset,
                content_width,
                layout,
            };
            view.scope_editor_pane(previous);
            paint
        })
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut EditorPaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, cx| {
            let previous = view.pane;
            view.scope_editor_pane(self.pane);
            let focus = view.editor_focus.clone();
            let selection = view.editor().selection();
            let selections = view.editor_display_selections();
            let tag_indicators = view.builtin_tag_indicators();
            let focused = focus.is_focused(window);
            let dark = view.dark;
            let colour = |name, attribute, fallback| {
                rgb(super::workspace_surface::theme_style_colour(
                    dark, name, attribute, fallback,
                ))
            };
            let caret_colour = colour("Caret colour", "fgColor", 0x000000);
            let additional_caret_colour = colour("Multi-edit carets color", "fgColor", 0x404040);
            let selection_colour = colour("Selected text colour", "bgColor", 0x808080);
            let additional_selection_colour =
                colour("Multi-selected text color", "bgColor", 0xc0c0c0);
            let current_line_colour = colour(
                "Current line background colour",
                "bgColor",
                if dark { 0x101010 } else { 0xe8e8ff },
            );
            let guide_colour = colour("Indent guideline style", "fgColor", 0xc0c0c0);
            let whitespace_colour = colour("White space symbol", "fgColor", 0xffb56a);
            let total = prepaint.layout.rows.len() as f32;
            let caret_logical = prepaint.layout.rows.rows()
                [prepaint.layout.rows.row_for_byte(selection.head.0)]
            .logical_line;
            let line_height = view.editor_line_height();
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.view.clone()),
                cx,
            );
            let mut failure = None;
            let text_bounds = Bounds::new(
                point(bounds.left() + view.editor_text_origin(), bounds.top()),
                size(
                    (bounds.size.width - view.editor_text_origin() - px(SCROLLBAR_SIZE))
                        .max(px(0.)),
                    (bounds.size.height - px(SCROLLBAR_SIZE)).max(px(0.)),
                ),
            );
            window.with_content_mask(
                Some(ContentMask {
                    bounds: text_bounds,
                }),
                |window| {
                    for line in &prepaint.lines {
                        if caret_logical == line.logical_line {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(text_bounds.left(), line.origin.y),
                                    size(text_bounds.size.width, line_height),
                                ),
                                current_line_colour,
                            ));
                        }
                        let mut clip = line.clip;
                        if line.last {
                            clip.size.width = (text_bounds.right() - clip.left()).max(px(0.));
                        }
                        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                            if caret_logical != line.logical_line
                                && let Err(error) = line.shape.paint_background(
                                    line.origin,
                                    line_height,
                                    gpui::TextAlign::Left,
                                    None,
                                    window,
                                    cx,
                                )
                            {
                                failure = Some(error.to_string());
                            }
                            for indicator in &tag_indicators {
                                let selection = selection_geometry::SelectionGeometry {
                                    anchor: indicator.range.start,
                                    head: indicator.range.end,
                                    anchor_virtual: 0,
                                    head_virtual: 0,
                                };
                                if let Some(span) = line
                                    .geometry()
                                    .highlight(selection, |byte| f32::from(line.x_for_byte(byte)))
                                {
                                    let color =
                                        rgba((indicator.color << 8) | u32::from(indicator.alpha));
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                point(
                                                    line.origin.x + px(span.start),
                                                    line.origin.y,
                                                ),
                                                size(px(span.end - span.start), line_height),
                                            ),
                                            color,
                                        )
                                        .corner_radii(px(2.))
                                        .border_widths(px(1.))
                                        .border_color(color),
                                    );
                                }
                            }
                            for x in &line.guides {
                                for y in (0..f32::from(line_height) as usize).step_by(3) {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(line.origin.x + *x, line.origin.y + px(y as f32)),
                                            size(px(1.), px(1.)),
                                        ),
                                        guide_colour,
                                    ));
                                }
                            }
                            for (left, right, tab) in &line.whitespace {
                                let middle = line.origin.y + line_height / 2.;
                                if *tab {
                                    let tip = line.origin.x + *right - px(2.);
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(line.origin.x + *left + px(2.), middle),
                                            size((*right - *left - px(4.)).max(px(1.)), px(1.)),
                                        ),
                                        whitespace_colour,
                                    ));
                                    for step in 0..3 {
                                        for sign in [-1., 1.] {
                                            window.paint_quad(fill(
                                                Bounds::new(
                                                    point(
                                                        tip - px(step as f32),
                                                        middle + px(sign * step as f32),
                                                    ),
                                                    size(px(1.), px(1.)),
                                                ),
                                                whitespace_colour,
                                            ));
                                        }
                                    }
                                } else {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(
                                                line.origin.x + (*left + *right) / 2. - px(1.),
                                                middle,
                                            ),
                                            size(px(2.), px(2.)),
                                        ),
                                        whitespace_colour,
                                    ));
                                }
                            }
                            for (index, selected) in selections.ranges().iter().enumerate() {
                                if let Some(span) = line
                                    .geometry()
                                    .highlight(selection_geometry(*selected), |byte| {
                                        f32::from(line.x_for_byte(byte))
                                    })
                                {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(line.origin.x + px(span.start), line.origin.y),
                                            size(px(span.end - span.start), line_height),
                                        ),
                                        if index == selections.primary_index() {
                                            selection_colour
                                        } else {
                                            additional_selection_colour
                                        },
                                    ));
                                }
                            }
                            if let Err(error) = line.shape.paint(
                                line.origin,
                                line_height,
                                gpui::TextAlign::Left,
                                None,
                                window,
                                cx,
                            ) {
                                failure = Some(error.to_string());
                            }
                            if focused {
                                for (index, selected) in selections.ranges().iter().enumerate() {
                                    if let Some(x) = line
                                        .geometry()
                                        .caret(selection_geometry(*selected), |byte| {
                                            f32::from(line.x_for_byte(byte))
                                        })
                                    {
                                        let overwrite =
                                            view.editor().overwrite() && selected.is_empty();
                                        let width = if overwrite {
                                            let next = match view
                                                .editor()
                                                .adjacent_position(selected.selection.head, true)
                                            {
                                                Ok(next) => next.0.min(line.end),
                                                Err(error) => {
                                                    failure = Some(error.to_string());
                                                    selected.selection.head.0
                                                }
                                            };
                                            (line.x_for_byte(next) - px(x)).max(line.space_width)
                                        } else {
                                            px(1.)
                                        };
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(
                                                    line.origin.x + px(x),
                                                    line.origin.y
                                                        + if overwrite {
                                                            line_height - px(2.)
                                                        } else {
                                                            px(0.)
                                                        },
                                                ),
                                                size(
                                                    width,
                                                    if overwrite { px(2.) } else { line_height },
                                                ),
                                            ),
                                            if index == selections.primary_index() {
                                                caret_colour
                                            } else {
                                                additional_caret_colour
                                            },
                                        ));
                                    }
                                }
                            }
                        });
                    }
                    if let Some(target) = view
                        .editor_chrome
                        .text_drag
                        .and_then(drag::SelectionDrag::drop_target)
                        && let Some(line) =
                            prepaint.lines.iter().find(|line| line.owns_caret(target))
                    {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(line.origin.x + line.x_for_byte(target), line.origin.y),
                                size(px(2.), line_height),
                            ),
                            caret_colour,
                        ));
                    }
                },
            );
            let height = (f32::from(bounds.size.height) - SCROLLBAR_SIZE).max(0.);
            let width = (f32::from(bounds.size.width) - SCROLLBAR_SIZE).max(0.);
            let bars = EditorScrollbars::new(
                bounds,
                line_height,
                total,
                prepaint.top_line,
                prepaint.content_width,
                prepaint.horizontal_offset,
                view.editor_text_inset(),
            );
            let vertical = bars.vertical.thumb();
            let horizontal = bars.horizontal.thumb();
            let track_colour = rgb(if dark { 0x292929 } else { 0xf0f0f0 });
            let thumb_colour = rgb(if dark { 0x606060 } else { 0xcdcdcd });
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left() + px(width), bounds.top()),
                    size(px(SCROLLBAR_SIZE), bounds.size.height),
                ),
                track_colour,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), bounds.top() + px(height)),
                    size(px(width), px(SCROLLBAR_SIZE)),
                ),
                track_colour,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        bounds.left() + px(width + 2.),
                        bars.vertical_bounds.top() + px(vertical.start),
                    ),
                    size(px(SCROLLBAR_SIZE - 4.), px(vertical.end - vertical.start)),
                ),
                thumb_colour,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        bars.horizontal_bounds.left() + px(horizontal.start),
                        bounds.top() + px(height + 2.),
                    ),
                    size(
                        px(horizontal.end - horizontal.start),
                        px(SCROLLBAR_SIZE - 4.),
                    ),
                ),
                thumb_colour,
            ));
            for (vertical, decrement, arrow, axis) in [
                (true, true, bars.vertical_up, bars.vertical),
                (true, false, bars.vertical_down, bars.vertical),
                (false, true, bars.horizontal_left, bars.horizontal),
                (false, false, bars.horizontal_right, bars.horizontal),
            ] {
                let offset = axis.clamped_offset();
                let enabled = axis.step_by(if decrement { -1. } else { 1. }) != offset;
                paint_scroll_arrow(arrow, vertical, decrement, enabled, window);
            }
            let gutter_changed = view
                .editor_chrome
                .layout
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &prepaint.layout))
                || view.editor_painted_revision.as_ref() != Some(&prepaint.revision)
                || view.editor_bounds != Some(bounds)
                || view.editor_top_line != prepaint.top_line
                || view.editor_lines.len() != prepaint.lines.len();
            view.editor_lines = std::mem::take(&mut prepaint.lines);
            view.editor_bounds = Some(bounds);
            view.editor_painted_revision = Some(prepaint.revision.clone());
            view.editor_top_line = prepaint.top_line;
            view.editor_horizontal_offset = prepaint.horizontal_offset;
            view.editor_chrome.content_width = prepaint.content_width;
            view.editor_chrome.layout = Some(prepaint.layout.clone());
            view.editor_chrome.top_anchor = None;
            view.editor_chrome.pending_viewport = None;
            view.editor_chrome.p_width = f32::from(
                window
                    .text_system()
                    .shape_line(
                        "P".into(),
                        view.editor_font_size(),
                        &[TextRun {
                            len: 1,
                            font: gpui::font(view.editor_font_family()),
                            color: rgb(0x808080).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width(),
            );
            if view.split_sync_source == Some(self.pane) {
                view.split_sync_source = None;
                view.sync_pane_viewport(cx);
            }
            if gutter_changed {
                cx.notify();
            }
            if let Some(error) = failure {
                view.notify_editor_error(error, cx);
            }
            view.scope_editor_pane(previous);
        });
    }
}
