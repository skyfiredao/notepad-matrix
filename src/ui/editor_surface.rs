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
pub(super) use chrome::{DEFAULT_EDITOR_FONT_FAMILY, EditorSymbols};
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

pub(super) use chrome::{GUTTER_LEFT, SCROLLBAR_SIZE};

#[derive(Default)]
pub(super) struct EditorChrome {
    text_system: Option<Rc<gpui::WindowTextSystem>>,
    buffer: Option<crate::workspace::BufferId>,
    positions: std::collections::BTreeMap<crate::workspace::BufferId, (usize, Pixels, Pixels)>,
    p_width: f32,
    line_number_width: Option<Pixels>,
    collapsed: std::collections::BTreeMap<crate::workspace::BufferId, Vec<crate::editor::Document>>,
    view_state: super::view_surface::ViewState,
    content_width: Pixels,
    drag: Option<(bool, f32)>,
    display: EditorDisplay,
    layout: Option<Arc<EditorLayout>>,
    viewport_bounds: Option<Bounds<Pixels>>,
    top_anchor: Option<usize>,
    pending_viewport: Option<crate::workspace::ViewportPosition>,
    unwrapped_horizontal_offset: Pixels,
    unwrapped_content_width: Pixels,
    rectangle_pixels: Option<(usize, Pixels)>,
    selection_drag: Option<usize>,
    observed_selections: Option<crate::editor::SelectionSet>,
    text_drag: Option<drag::SelectionDrag>,
    word_drag: Option<(Selection, ByteOffset)>,
    hovered_link: Option<Range<ByteOffset>>,
    brace_highlight: Option<BraceHighlight>,
}

struct BraceHighlight {
    revision: crate::input::DisplayRevision,
    caret: usize,
    language: crate::syntax::Language,
    style_revision: u64,
    tags: bool,
    indent_guides: bool,
    ranges: Vec<Range<usize>>,
    style: u16,
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
        self.view_state.hidden.clear();
        self.layout = None;
        self.pending_viewport = None;
        self.clear_drag();
        self.observed_selections = None;
        self.brace_highlight = None;
    }
}

fn fold_marker_line(marker: &crate::editor::Document, text: &Rope) -> Option<usize> {
    let range = marker.projection().display_selection(marker.selection()).ok()?.range();
    (!range.is_empty() && range.start.0 < text.len())
        .then(|| text.byte_to_line_idx(range.start.0, LineType::LF_CR))
}

#[derive(PartialEq)]
struct EditorLayoutKey {
    collapsed: Vec<usize>,
    hidden: Vec<(usize, usize)>,
    revision: crate::input::DisplayRevision,
    language: crate::syntax::Language,
    display: EditorDisplay,
    symbols: EditorSymbols,
    width: Pixels,
    style_revision: u64,
    dark: bool,
    selected_foreground: Option<crate::editor::SelectionSet>,
    syntax_highlighting: bool,
    clickable_links: bool,
    link_options: crate::links::LinkOptions,
}

struct EditorLayout {
    key: EditorLayoutKey,
    rows: VisualRows,
    shapes: std::cell::RefCell<std::collections::BTreeMap<usize, Arc<EditorShapedLine>>>,
    starts: Vec<usize>,
    ends: Vec<usize>,
    content_width: std::cell::Cell<Pixels>,
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
        vertical_padding: Pixels,
    ) -> Self {
        let height = (f32::from(bounds.size.height) - SCROLLBAR_SIZE).max(0.);
        let width = (f32::from(bounds.size.width) - SCROLLBAR_SIZE).max(0.);
        let vertical_track = chrome::scrollbar_track(height);
        let horizontal_track = chrome::scrollbar_track(width);
        let capacity = ((height - f32::from(vertical_padding)) / f32::from(line_height))
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

fn paint_current_line_frame(
    rectangle: Bounds<Pixels>,
    width: Pixels,
    color: u32,
    window: &mut Window,
) {
    for border in [
        Bounds::new(rectangle.origin, size(rectangle.size.width, width)),
        Bounds::new(
            point(rectangle.left(), rectangle.bottom() - width),
            size(rectangle.size.width, width),
        ),
        Bounds::new(rectangle.origin, size(width, rectangle.size.height)),
        Bounds::new(
            point(rectangle.right() - width, rectangle.top()),
            size(width, rectangle.size.height),
        ),
    ] {
        window.paint_quad(fill(border, rgb(color)));
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

enum PaintedHistorySpan {
    Insertion { range: Range<usize>, edition: u8 },
    Deletion { position: usize, second: Option<usize>, editions: u8 },
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

    fn paint_change_history(
        &self,
        spans: &[PaintedHistorySpan],
        view: &MatrixWindow,
        line_height: Pixels,
        window: &mut Window,
    ) -> Result<(), String> {
        for history in spans {
            match history {
                PaintedHistorySpan::Insertion { range, edition } => {
                    let selected = selection_geometry::SelectionGeometry {
                        anchor: range.start,
                        head: range.end,
                        anchor_virtual: 0,
                        head_virtual: 0,
                    };
                    if let Some(span) = self.geometry().highlight(selected, |byte| f32::from(self.x_for_byte(byte))) {
                        let (color, _) = super::bookmark_surface::change_history_style(*edition, view, "fgColor");
                        let width = px(span.end - span.start - 2.);
                        if width > px(0.) {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(self.origin.x + px(span.start + 1.), self.origin.y + line_height - px(2.)),
                                    size(width, px(2.)),
                                ),
                                rgba((color << 8) | 50),
                            ));
                        }
                    }
                }
                PaintedHistorySpan::Deletion { position, second, editions } => {
                    if !self.owns_caret(*position) {
                        continue;
                    }
                    let x = self.origin.x + self.x_for_byte(*position);
                    let right = second.map_or(x + px(1.), |second| self.origin.x + self.x_for_byte(second));
                    if right - x < px(0.1) {
                        continue;
                    }
                    let height = (line_height - self.shape.baseline()).max(px(3.)).floor();
                    for edition in 1u8..=4 {
                        if editions & (1 << (edition - 1)) == 0 {
                            continue;
                        }
                        let (color, _) = super::bookmark_surface::change_history_style(edition, view, "fgColor");
                        let x = x.round() + px(0.5);
                        let (y, tip) = if edition == 3 {
                            let y = self.origin.y.floor() - px(0.5);
                            (y, y + height)
                        } else {
                            let y = (self.origin.y + self.shape.baseline() + height).ceil() + px(0.5);
                            (y, y - height)
                        };
                        let mut path = gpui::PathBuilder::fill();
                        path.move_to(point(x - height, y));
                        path.line_to(point(x + height, y));
                        path.line_to(point(x, tip));
                        path.close();
                        window.paint_path(path.build().map_err(|error| error.to_string())?, rgb(color));
                    }
                }
            }
        }
        Ok(())
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
    raw_bytes: &[u8],
    space_width: Pixels,
    baseline: Pixels,
    window: &Window,
) -> (usize, EditorShapedLine) {
    let start = text.line_to_byte_idx(row, LineType::LF_CR);
    let raw = text.line(row, LineType::LF_CR).to_string();
    let text_system = gpui::WindowTextSystem::new(
        std::ops::Deref::deref(window.text_system().as_ref()).clone(),
    );
    let projection = DisplayRow::with_preferences(
        raw_bytes,
        view.editor_symbols,
        view.pref_int("ScintillaPrimaryView.npcMode", 1),
        view.pref_bool("ScintillaPrimaryView.npcIncludeCcUniEOL", false),
    );
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
    let runs = view.appearance_selected_runs(start, runs);
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
            view.editor_style_profile().foreground,
            view.editor_style_profile().background,
            view.pref_int("ScintillaPrimaryView.eolMode", 1),
            view.style_colour("EOL custom color", "fgColor", 0xc0c0c0),
            view.pref_bool("ScintillaPrimaryView.npcCustomColor", false)
                .then(|| {
                    view.style_colour("Non-printing characters custom color", "fgColor", 0xc0c0c0)
                }),
            view.pref_bool("ScintillaPrimaryView.npcIncludeCcUniEOL", false),
            &text_system,
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
        self.set_scintilla_geometry_refresher(Self::refresh_scintilla_shortcut_geometry);
        self.ensure_scintilla_target_attachment();
        let id = self.workspace.active().id();
        if self.editor_chrome.buffer == Some(id) {
            return;
        }
        self.invalidate_incremental_count();
        if let Some(previous) = self.editor_chrome.buffer.replace(id) {
            self.editor_chrome.positions.insert(
                previous,
                (
                    self.editor_top_line,
                    self.editor_horizontal_offset,
                    self.editor_chrome.unwrapped_content_width,
                ),
            );
        }
        let restored = self.editor_chrome.positions.get(&id).copied();
        let (top, horizontal, content_width) = restored.unwrap_or((0, px(0.), px(1.)));
        self.editor_top_line = top;
        self.editor_horizontal_offset = horizontal;
        self.editor_chrome.content_width = content_width;
        self.editor_chrome.unwrapped_content_width = content_width;
        self.editor_follow_caret = restored.is_none();
        self.editor_lines.clear();
        self.editor_bounds = None;
        self.editor_chrome.layout = None;
        self.editor_chrome.top_anchor = None;
        self.editor_chrome.pending_viewport = None;
        self.editor_chrome.clear_drag();
        self.editor_chrome.observed_selections = None;
        self.editor_chrome.hovered_link = None;
        self.editor_chrome.brace_highlight = None;
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

    pub(super) fn center_search_result(
        &mut self,
        range: Range<ByteOffset>,
        backwards: bool,
        window: &Window,
    ) {
        let Some(bounds) = self.editor_bounds else {
            return;
        };
        let layout = self.editor_layout_for_width(self.editor_text_width(), window);
        let position = if backwards { range.start } else { range.end };
        let display = self
            .editor()
            .document()
            .projection()
            .project_selection(Selection {
                anchor: position,
                head: position,
            })
            .head;
        let row = layout.rows.row_for_byte(display.0);
        let capacity = ((f32::from(bounds.size.height) - SCROLLBAR_SIZE - f32::from(self.editor_vertical_padding()))
            / f32::from(self.editor_line_height()))
        .floor()
        .max(1.) as usize;
        if row < self.editor_top_line || row >= self.editor_top_line.saturating_add(capacity) {
            self.editor_top_line = row
                .saturating_sub(capacity / 2)
                .min(layout.rows.len().saturating_sub(capacity));
            self.editor_chrome.top_anchor = None;
            self.editor_chrome.pending_viewport = None;
            self.editor_scroll_remainder = 0.;
        }
    }

    fn editor_layout_for_width(&self, width: Pixels, window: &Window) -> Arc<EditorLayout> {
        let key = EditorLayoutKey {
            collapsed: self.editor_collapsed_headers(),
            hidden: self.editor_hidden_lines(),
            revision: self.editor().display_revision(),
            language: self.workspace.active().language(),
            display: self.editor_chrome.display,
            symbols: self.editor_symbols,
            style_revision: self.style_config.revision(),
            dark: self.dark,
            syntax_highlighting: self.workspace.syntax_highlighting_allowed(),
            clickable_links: self.workspace.clickable_links_allowed(),
            link_options: self.preferences.files.links.clone(),
            selected_foreground: self
                .pref_bool(
                    "ScintillaPrimaryView.selectedTextForegroundSingleColor",
                    false,
                )
                .then(|| self.editor_display_selections()),
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
        let mut starts = Vec::new();
        let mut ends = Vec::new();
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
        if self.editor_word_wrap_enabled() {
            for (logical, raw_bytes) in self.editor().display_byte_snapshot().lines().enumerate() {
                let raw = text.line(logical, LineType::LF_CR).to_string();
                let content = raw.trim_end_matches(['\r', '\n']);
                let (start, shape) = shape_editor_row(
                    self,
                    &text,
                    logical,
                    &raw_bytes,
                    space_width,
                    baseline,
                    window,
                );
                starts.push(start);
                ends.push(start + content.len());
                let indent_byte = content
                    .char_indices()
                    .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
                    .map_or(content.len(), |(byte, _)| byte);
                let mut indent = shape.x_for_index(indent_byte);
                indent = match self
                    .pref_string("ScintillaPrimaryView.lineWrapMethod", "aligned")
                    .as_str()
                {
                    "default" => px(0.),
                    "indent" => indent + space_width * self.editor().tab_preferences().size as f32,
                    _ => indent,
                };
                if indent > width - space_width * 15. {
                    indent = px(0.);
                }
                let mut ranges = layout::wrap_ranges(
                    content,
                    f32::from(width),
                    f32::from(indent),
                    |byte| f32::from(shape.x_for_index(byte)),
                );
                if self.editor_symbols.eol
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
            }
        } else {
            let mut start = 0;
            for (logical, raw) in text.lines(LineType::LF_CR).enumerate() {
                let mut content_len = raw.len();
                while content_len > 0 && matches!(raw.byte(content_len - 1), b'\r' | b'\n') {
                    content_len -= 1;
                }
                let end = start + content_len;
                rows.push_line(logical, start, &[0..end - start], 0.);
                starts.push(start);
                ends.push(end);
                start += raw.len();
            }
        }
        let collapsed: Vec<_> = folds
            .into_iter()
            .filter(|(header, _)| self.editor_fold_collapsed(*header))
            .collect();
        rows.hide_folded(&collapsed);
        rows.hide_lines(&key.hidden);
        let content_width = self.editor_chrome.layout.as_ref()
            .filter(|layout| layout.key.revision == key.revision && layout.key.display == key.display)
            .map_or(
                self.editor_chrome.unwrapped_content_width.max(px(1.)),
                |layout| layout.content_width.get(),
            );
        Arc::new(EditorLayout {
            key,
            rows,
            shapes: Default::default(),
            starts,
            ends,
            content_width: std::cell::Cell::new(content_width),
        })
    }

    fn editor_row_bytes(&self, logical: usize) -> Result<Vec<u8>, EditError> {
        let bytes = self.editor().display_byte_snapshot();
        let start = bytes.line_start(logical).map_err(|_| EditError::InvalidRange)?;
        let end = if logical + 1 < bytes.line_count() {
            bytes.line_start(logical + 1).map_err(|_| EditError::InvalidRange)?
        } else {
            bytes.len()
        };
        bytes.slice(start..end)
            .map(|bytes| bytes.to_vec())
            .map_err(|_| EditError::InvalidRange)
    }

    fn editor_row_shape(
        &self,
        layout: &EditorLayout,
        logical: usize,
        window: &Window,
    ) -> Result<Arc<EditorShapedLine>, EditError> {
        if let Some(shape) = layout.shapes.borrow().get(&logical) {
            return Ok(shape.clone());
        }
        let text = self.editor().display_snapshot();
        let bytes = self.editor_row_bytes(logical)?;
        let (_, shape) = shape_editor_row(
            self,
            &text,
            logical,
            &bytes,
            self.editor_space_width(window),
            self.editor_style_baseline(),
            window,
        );
        Ok(Arc::new(shape))
    }

    fn editor_text_width(&self) -> Pixels {
        self.editor_bounds.map_or(px(600.), |bounds| {
            (bounds.size.width - self.editor_text_inset() - px(SCROLLBAR_SIZE)).max(px(1.))
        })
    }

    pub(super) fn refresh_scintilla_shortcut_geometry(&mut self, window: &Window) {
        self.refresh_editor_syntax();
        self.editor_chrome.text_system = Some(window.text_system().clone());
        self.capture_udl_text_system(window);
        if self.editor_bounds.is_none() {
            self.editor_bounds = self.editor_chrome.viewport_bounds;
        }
        self.refresh_editor_gutter_measurement(|family, size| {
            let text_system = window.text_system();
            let font = text_system.resolve_font(&family);
            text_system.layout_width(font, size, '8')
        });
        let width = self.editor_bounds.map(|bounds| bounds.size.width)
            .map(|width| (width - self.editor_text_inset() - px(SCROLLBAR_SIZE)).max(px(1.)));
        let Some(width) = width else {
            self.editor_chrome.invalidate_layout();
            return;
        };
        let layout = self.editor_layout_for_width(width, window);
        let anchor = self.editor_chrome.top_anchor.or_else(|| {
            self.editor_chrome.layout.as_ref()
                .filter(|old| old.key.width != layout.key.width)
                .and_then(|old| old.rows.rows().get(self.editor_top_line))
                .map(|row| row.range.start)
        });
        if let Some(position) = &self.editor_chrome.pending_viewport {
            self.editor_top_line = layout.rows.restored_top(
                position.first_visible_line, position.wrap_offset, position.wrap_count,
                self.editor_word_wrap_enabled(),
            );
        } else if let Some(anchor) = anchor {
            self.editor_top_line = layout.rows.row_for_byte(anchor);
        }
        self.editor_chrome.content_width = if self.editor_word_wrap_enabled() {
            (width - px(1.)).max(px(0.))
        } else {
            layout.content_width.get()
        };
        self.editor_chrome.layout = Some(layout);
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

    pub(super) fn editor_measured_split_breaks(
        &self,
        window: &Window,
    ) -> Result<Vec<ByteOffset>, String> {
        let text = self.editor().display_snapshot();
        let bytes = self.editor().document().byte_snapshot();
        let projection = self.editor().document().projection();
        let width = px(f32::from(self.editor_text_width()).trunc());
        let space_width = self.editor_space_width(window);
        let baseline = self.editor_style_baseline();
        let mut breaks = Vec::new();
        for (logical, raw_bytes) in self.editor().display_byte_snapshot().lines().enumerate() {
            let raw = text.line(logical, LineType::LF_CR).to_string();
            let content = raw.trim_end_matches(['\r', '\n']);
            let (start, shape) = shape_editor_row(
                self,
                &text,
                logical,
                &raw_bytes,
                space_width,
                baseline,
                window,
            );
            let indentation = content
                .char_indices()
                .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
                .map_or(content.len(), |(byte, _)| byte);
            let mut indent = shape.x_for_index(indentation);
            if indent > width - space_width * 15. {
                indent = px(0.);
            }
            let source_start = bytes
                .line_start(logical)
                .map_err(|error| error.to_string())?;
            let source_end = source_start + raw_bytes.len()
                - raw_bytes
                    .iter()
                    .rev()
                    .take_while(|byte| matches!(byte, b'\r' | b'\n'))
                    .count();
            let display_positions = (source_start..=source_end)
                .map(|position| {
                    projection
                        .display_position(
                            ByteOffset(position),
                            crate::byte_projection::Affinity::After,
                        )
                        .map(|position| position.0)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let display = |position: usize| display_positions[position - source_start];
            let style_at = |position: usize| {
                let position = display(position);
                let builtin = self.builtin_brace_style(position);
                let json = self.syntax_cache.as_ref().and_then(|(_, _, spans)| {
                    spans
                        .iter()
                        .find(|span| span.range.contains(&position))
                        .map(|span| span.style)
                });
                let udl = self
                    .udl
                    .syntax_cache
                    .as_ref()
                    .and_then(|cache| cache.style_at(position));
                (builtin, json, udl)
            };
            let source_breaks = layout::split_breaks(
                &bytes,
                source_start..source_end,
                f32::from(width),
                f32::from(indent),
                |position| f32::from(shape.x_for_index(display(position) - start)),
                |left, right| style_at(left) != style_at(right),
            )?;
            breaks.extend(source_breaks.into_iter().map(ByteOffset));
        }
        Ok(breaks)
    }

    pub(super) fn editor_word_wrap_enabled(&self) -> bool {
        self.editor_chrome.display.word_wrap
    }

    pub(super) fn editor_fold_collapsed(&self, header: usize) -> bool {
        self.editor_collapsed_headers().contains(&header)
    }

    pub(super) fn ensure_editor_line_visible(&mut self, line: usize) {
        self.reveal_editor_hidden_line(line);
        let containing: Vec<_> = self
            .udl_fold_ranges()
            .into_iter()
            .filter_map(|(header, end)| (line > header && line <= end).then_some(header))
            .collect();
        let text = self.editor().display_snapshot();
        if let Some(markers) = self
            .editor_chrome
            .collapsed
            .get_mut(&self.workspace.active().id())
        {
            let before = markers.len();
            markers.retain(|marker| {
                fold_marker_line(marker, &text).is_none_or(|header| !containing.contains(&header))
            });
            if markers.len() != before {
                self.editor_chrome.layout = None;
                self.editor_painted_revision = None;
            }
        }
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
            let marker = match self.editor_view_marker(start..start + length) {
                Ok(marker) => marker,
                Err(error) => {
                    self.notify_editor_error(error.to_string(), cx);
                    return;
                }
            };
            self.editor_chrome
                .collapsed
                .entry(id)
                .or_default()
                .push(marker);
            let caret =
                text.byte_to_line_idx(self.editor().display_selection().head.0, LineType::LF_CR);
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

    fn editor_view_marker(&self, range: Range<usize>) -> Result<crate::editor::Document, EditError> {
        let projection = self.editor().document().projection();
        let anchor = projection.source_position(
            crate::byte_projection::DisplayByteOffset(range.start),
            crate::byte_projection::Affinity::After,
        )?;
        let head = projection.source_position(
            crate::byte_projection::DisplayByteOffset(range.end),
            crate::byte_projection::Affinity::Before,
        )?;
        let mut marker = self.editor().document().clone_view();
        marker.select(Selection { anchor, head })?;
        Ok(marker)
    }

    pub(super) fn set_editor_folds(
        &mut self,
        level: Option<usize>,
        collapse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        self.refresh_editor_syntax();
        let mut folds = self.udl_fold_ranges();
        folds.sort_unstable_by_key(|&(header, end)| (header, std::cmp::Reverse(end)));
        folds.dedup_by_key(|fold| fold.0);
        let targets: Vec<_> = if let Some(level) = level {
            let (indentation_based, mut headers) = self.udl_fold_header_levels();
            headers.sort_unstable_by_key(|&(line, _)| line);
            let mut parents = Vec::new();
            let mut skip_through = None;
            headers
                .into_iter()
                .filter_map(|(header, raw_level)| {
                    if skip_through.is_some_and(|end| header <= end) {
                        return None;
                    }
                    let matches = if indentation_based {
                        while parents.last().is_some_and(|parent| raw_level <= *parent) {
                            parents.pop();
                        }
                        parents.push(raw_level);
                        parents.len() == level + 1
                    } else {
                        (raw_level.wrapping_sub(0x400) & 0x0fff) as usize == level
                    };
                    let &(_, end) = folds.iter().find(|&&(line, _)| line == header)?;
                    if matches {
                        if indentation_based {
                            skip_through = Some(end);
                        }
                        Some(header)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            folds.iter().map(|&(header, _)| header).collect()
        };
        let text = self.editor().display_snapshot();
        let mut added = Vec::new();
        if collapse {
            for &header in &targets {
                if self.editor_fold_collapsed(header) {
                    continue;
                }
                let start = text.line_to_byte_idx(header, LineType::LF_CR);
                let length = text.line(header, LineType::LF_CR).to_string()
                    .trim_end_matches(['\r', '\n']).len();
                match self.editor_view_marker(start..start + length) {
                    Ok(marker) => added.push(marker),
                    Err(error) => {
                        self.notify_editor_error(error.to_string(), cx);
                        return;
                    }
                }
            }
        }
        let id = self.workspace.active().id();
        let markers = self.editor_chrome.collapsed.entry(id).or_default();
        if collapse {
            markers.extend(added);
        } else {
            markers.retain(|marker| fold_marker_line(marker, &text)
                .is_some_and(|header| !targets.contains(&header)));
        }
        let selection = self.editor().display_selection();
        let head = self.visible_editor_position(selection.head, false);
        if head != selection.head {
            let result = self.editor_mut().select_display(Selection { anchor: head, head });
            self.editor_changed(result, window, cx);
        }
        self.editor_display_changed(window, cx);
        self.editor_follow_caret = false;
        self.editor_focus.focus(window, cx);
    }

    pub(super) fn editor_hidden_lines(&self) -> Vec<(usize, usize)> {
        let text = self.editor().display_snapshot();
        let count = text.len_lines(LineType::LF_CR);
        let mut hidden: Vec<_> = self.editor_chrome.view_state.hidden
            .get(&self.workspace.active().id()).into_iter().flatten()
            .filter_map(|marker| {
                let range = marker.projection().display_selection(marker.selection()).ok()?.range();
                if range.is_empty() || range.start.0 >= text.len() || count < 3 {
                    return None;
                }
                let start = text.byte_to_line_idx(range.start.0, LineType::LF_CR).max(1);
                let end = text.byte_to_line_idx(range.end.0.saturating_sub(1).min(text.len()), LineType::LF_CR)
                    .min(count - 2);
                (start <= end).then_some((start, end))
            }).collect();
        hidden.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in hidden {
            if let Some(last) = merged.last_mut()
                && start <= last.1 + 1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        merged
    }

    pub(super) fn hide_editor_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        let text = self.editor().display_snapshot();
        let count = text.len_lines(LineType::LF_CR);
        if count < 3 {
            return;
        }
        let selection = self.editor().display_selection().range();
        let start = text.byte_to_line_idx(selection.start.0, LineType::LF_CR).max(1);
        let end = text.byte_to_line_idx(selection.end.0, LineType::LF_CR).min(count - 2);
        if start > end {
            return;
        }
        let range = text.line_to_byte_idx(start, LineType::LF_CR)
            ..text.line_to_byte_idx(end + 1, LineType::LF_CR);
        let marker = match self.editor_view_marker(range) {
            Ok(marker) => marker,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        };
        self.editor_chrome.view_state.hidden.entry(self.workspace.active().id())
            .or_default().push(marker);
        let head = self.visible_editor_position(self.editor().display_selection().head, false);
        let result = self.editor_mut().select_display(Selection { anchor: head, head });
        self.editor_changed(result, window, cx);
        self.editor_display_changed(window, cx);
        self.editor_follow_caret = false;
        self.editor_focus.focus(window, cx);
    }

    fn reveal_editor_hidden_line(&mut self, line: usize) -> bool {
        let Some((start, end)) = self.editor_hidden_lines().into_iter()
            .find(|&(start, end)| line >= start && line <= end) else {
            return false;
        };
        let text = self.editor().display_snapshot();
        let Some(markers) = self.editor_chrome.view_state.hidden.get_mut(&self.workspace.active().id()) else {
            return false;
        };
        markers.retain(|marker| {
            marker.projection().display_selection(marker.selection()).ok().is_some_and(|selection| {
                let range = selection.range();
                let first = text.byte_to_line_idx(range.start.0.min(text.len()), LineType::LF_CR);
                let last = text.byte_to_line_idx(range.end.0.saturating_sub(1).min(text.len()), LineType::LF_CR);
                last < start || first > end
            })
        });
        self.editor_chrome.invalidate_layout();
        self.editor_painted_revision = None;
        true
    }

    fn render_editor_hidden_markers(&self, lines: &[PaintedLine], cx: &mut Context<Self>) -> Vec<AnyElement> {
        let hidden = self.editor_hidden_lines();
        let pane = self.pane;
        let owner = self.workspace.active().id();
        let row_height = self.editor_line_height();
        lines.iter().enumerate()
            .filter_map(|(row, visual)| {
                let line = visual.logical_line;
                let first = visual.first;
                let last = visual.last;
                let &(start, _) = hidden.iter()
                    .find(|&&(start, _)| last && line + 1 == start)
                    .or_else(|| hidden.iter().find(|&&(_, end)| first && line == end + 1))?;
                Some(div()
                    .id(("editor-hidden-lines", row))
                    .absolute()
                    .left(self.editor_line_number_width())
                    .top(self.editor_text_top() + row_height * row as f32)
                    .w(px(12.))
                    .h(row_height)
                    .text_size(px(11.))
                    .line_height(row_height)
                    .text_color(rgb(self.style_colour("Fold", "fgColor", 0x808080)))
                    .cursor_pointer()
                    .child(if line < start { "▾" } else { "▴" })
                    .on_mouse_down(MouseButton::Left, cx.listener(move |view, _, window, cx| {
                        if !view.focus_editor_pane(pane, window, cx)
                            || view.workspace.active().id() != owner
                            || !view.cancel_view_composition(window, cx)
                        {
                            return;
                        }
                        if view.reveal_editor_hidden_line(start) {
                            view.editor_display_changed(window, cx);
                            view.editor_follow_caret = false;
                        }
                        cx.stop_propagation();
                    }))
                    .into_any_element())
            }).collect()
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

    pub(super) fn editor_visual_row_count(&self) -> usize {
        self.editor_cached_layout().map_or_else(
            || self.editor_visible_logical_lines().len(),
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
            .map_or_else(|| {
                let lines = self.editor_visible_logical_lines();
                lines.get(row).or_else(|| lines.last()).copied().unwrap_or(0)
            }, |row| row.logical_line)
    }

    fn editor_visible_logical_lines(&self) -> Vec<usize> {
        let hidden = self.editor_hidden_lines();
        let collapsed = self.editor_collapsed_headers();
        let folds: Vec<_> = self.udl_fold_ranges().into_iter()
            .filter(|(header, _)| collapsed.contains(header)).collect();
        (0..self.editor().display_snapshot().len_lines(LineType::LF_CR))
            .filter(|line| !hidden.iter().any(|&(start, end)| *line >= start && *line <= end)
                && !folds.iter().any(|&(header, end)| *line > header && *line <= end))
            .collect()
    }

    fn editor_cached_layout(&self) -> Option<&EditorLayout> {
        self.editor_chrome.layout.as_deref().filter(|layout| {
            layout.key.revision == self.editor().display_revision()
                && layout.key.language == self.workspace.active().language()
                && layout.key.display == self.editor_chrome.display
                && layout.key.collapsed == self.editor_collapsed_headers()
                && layout.key.hidden == self.editor_hidden_lines()
                && layout.key.symbols == self.editor_symbols
                && layout.key.style_revision == self.style_config.revision()
                && layout.key.dark == self.dark
                && layout.key.syntax_highlighting == self.workspace.syntax_highlighting_allowed()
                && layout.key.clickable_links == self.workspace.clickable_links_allowed()
                && layout.key.link_options == self.preferences.files.links
        })
    }

    pub(super) fn editor_visual_row_source_range(&self, row: usize) -> Option<Range<usize>> {
        if let Some(layout) = self.editor_cached_layout() {
            return layout.rows.rows().get(row).map(|row| row.range.clone());
        }
        let text = self.editor().display_snapshot();
        let logical = *self.editor_visible_logical_lines().get(row)?;
        let start = text.line_to_byte_idx(logical, LineType::LF_CR);
        let length = text
            .line(logical, LineType::LF_CR)
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
    pub(super) fn editor_default_style(&self) -> crate::syntax::StyleAppearance {
        let mut base = crate::syntax::StyleAppearance::plain(self.dark);
        base.font_family = chrome::DEFAULT_EDITOR_FONT_FAMILY.into();
        base.font_size = chrome::EditorDisplay::default().font_size() as u32;
        self.style_config.appearance(None, 32, self.dark, &base, 0)
    }
    pub(super) fn editor_font_size(&self) -> Pixels {
        px((self.editor_default_style().font_size as i32 + self.editor_zoom_delta()).max(1) as f32)
    }
    pub(super) fn editor_font_family(&self) -> String {
        self.editor_default_style().font_family
    }
    pub(super) fn editor_text_system(&self) -> Option<&gpui::WindowTextSystem> {
        self.editor_chrome.text_system.as_deref()
    }
    pub(super) fn editor_zoom_delta(&self) -> i32 {
        self.editor_chrome.display.zoom()
    }
    fn editor_added_padding(&self) -> Pixels {
        self.editor_text_system().map_or(
            self.editor_font_size() / 4.,
            |text_system| {
                let font = text_system.resolve_font(&gpui::font(self.editor_font_family()));
                text_system.layout_width(font, self.editor_font_size(), ' ') / 2.
            },
        )
    }
    pub(super) fn editor_text_origin(&self) -> Pixels {
        px(chrome::TEXT_LEFT) + self.editor_line_number_width() - px(32.) - px(14.)
            + self.editor_fold_margin_width()
            + self.editor_bookmark_margin_width()
            - px(16.)
            + self.editor_history_margin_width()
            - px(9.)
            + px(self.appearance_padding(true))
            + self.editor_added_padding()
    }
    fn editor_text_inset(&self) -> Pixels {
        self.editor_text_origin() + px(chrome::TEXT_RIGHT_PADDING + self.appearance_padding(false))
    }
    pub(super) fn editor_line_number_width(&self) -> Pixels {
        if self.pref_bool("ScintillaPrimaryView.lineNumberMargin", true) {
            self.editor_chrome.line_number_width.unwrap_or(px(32.))
        } else {
            px(0.)
        }
    }
    pub(super) fn editor_gutter_width(&self) -> Pixels {
        px(chrome::GUTTER_WIDTH) + self.editor_line_number_width() - px(32.) - px(14.)
            + self.editor_fold_margin_width()
            + self.editor_bookmark_margin_width()
            - px(16.)
            + self.editor_history_margin_width()
            - px(9.)
    }
    pub(super) fn editor_bookmark_margin_width(&self) -> Pixels {
        px(
            if self.pref_bool("ScintillaPrimaryView.bookMarkMargin", true) {
                16.
            } else {
                0.
            },
        )
    }
    pub(super) fn editor_history_margin_width(&self) -> Pixels {
        px(
            if self.change_history_runtime_flags() & 2 != 0 {
                9.
            } else {
                0.
            },
        )
    }
    pub(super) fn editor_fold_margin_width(&self) -> Pixels {
        if self.pref_string("ScintillaPrimaryView.folderMarkStyle", "box") == "none" {
            return px(0.);
        }
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
    pub(super) fn editor_line_number_font(&self) -> (String, Pixels) {
        let margin = self.style_config.global_style(33, self.dark);
        let attribute = |name| {
            margin
                .and_then(|style| style.attributes.get(name))
                .filter(|value| !value.is_empty())
        };
        let family = attribute("fontName")
            .cloned()
            .unwrap_or_else(|| self.editor_font_family());
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
    pub(super) fn editor_line_number_font_style(&self) -> i32 {
        self.style_config.global_style(33, self.dark)
            .and_then(|style| style.attributes.get("fontStyle"))
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|bits| *bits != -1)
            .unwrap_or_else(|| {
                let inherited = self.editor_style_profile();
                i32::from(inherited.bold) | (i32::from(inherited.italic) << 1) | (i32::from(inherited.underline) << 2)
            })
    }
    fn refresh_editor_brace_highlight(&mut self, cx: &mut Context<Self>) {
        if !self.workspace.brace_match_allowed() {
            self.editor_chrome.brace_highlight = None;
            return;
        }
        let revision = self.editor().display_revision();
        let caret = self.editor().document().selection().head.0;
        let language = self.workspace.active().language();
        let style_revision = self.style_config.revision();
        let tags = self.builtin.tag_match_options.enabled;
        let indent_guides = self.editor_indent_guides_shown();
        if self.editor_chrome.brace_highlight.as_ref().is_some_and(|highlight| {
            highlight.revision == revision && highlight.caret == caret && highlight.language == language
                && highlight.style_revision == style_revision && highlight.tags == tags && highlight.indent_guides == indent_guides
        }) { return; }
        self.editor_chrome.brace_highlight = None;
        let bytes = self.editor().document().byte_snapshot();
        let is_brace = |byte: u8| b"{}[]()".contains(&byte);
        let at = caret.checked_sub(1).filter(|at| bytes.byte(*at).is_some_and(is_brace))
            .or_else(|| bytes.byte(caret).filter(|byte| is_brace(*byte)).map(|_| caret));
        let (positions, mut style) = if let Some((at, opposite)) = self.matching_brace_positions() {
            (vec![at, opposite], 34)
        } else {
            (at.into_iter().collect(), 35)
        };
        let projection = self.editor().document().projection();
        let ranges = positions.into_iter().map(|position| {
            let start = projection.display_position(ByteOffset(position), crate::byte_projection::Affinity::After)
                .map_err(|error| error.to_string())?.0;
            let end = projection.display_position(ByteOffset(position + 1), crate::byte_projection::Affinity::Before)
                .map_err(|error| error.to_string())?.0;
            Ok(start..end)
        }).collect::<Result<Vec<_>, String>>();
        let mut ranges = match ranges {
            Ok(ranges) => ranges,
            Err(error) => { self.notify_editor_error(error, cx); return; }
        };
        if indent_guides
            && let Some(tags) = self.builtin_tag_match_ranges()
            && tags.content.is_some()
            && let (Some(open), Some(close)) = (tags.tags.first(), tags.tags.last())
        {
            let text = self.editor().display_snapshot();
            if text.byte_to_line_idx(open.start, LineType::LF_CR) != text.byte_to_line_idx(close.start, LineType::LF_CR) {
                ranges = vec![open.start..open.start + 1, close.end - 1..close.end];
                style = 34;
            }
        }
        self.editor_chrome.brace_highlight = Some(BraceHighlight {
            revision, caret, language, style_revision, tags, indent_guides, ranges, style,
        });
    }
    fn refresh_editor_gutter(&mut self, cx: &Context<Self>) {
        self.refresh_editor_gutter_measurement(|family, size| {
            let text_system = cx.text_system();
            let font = text_system.resolve_font(&family);
            text_system.layout_width(font, size, '8')
        });
    }

    fn refresh_editor_gutter_measurement(&mut self, measure: impl FnOnce(gpui::Font, Pixels) -> Pixels) {
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
        let digits = if self.pref_bool("ScintillaPrimaryView.lineNumberDynamicWidth", true) {
            last.to_string().len().max(3)
        } else {
            line_count.to_string().len().max(3)
        };
        let (family, size) = self.editor_line_number_font();
        let bits = self.editor_line_number_font_style();
        let mut font = gpui::font(family);
        if bits & 1 != 0 { font = font.bold(); }
        if bits & 2 != 0 { font = font.italic(); }
        let width = measure(font, size);
        let width = px(8. + digits as f32 * f32::from(width).round());
        if self.editor_chrome.line_number_width != Some(width) {
            self.editor_chrome.line_number_width = Some(width);
        }
    }
    pub(super) fn editor_line_height(&self) -> Pixels {
        self.editor_style_line_height().max(px(1.))
    }
    pub(super) fn editor_text_top(&self) -> Pixels {
        px(chrome::TEXT_LEFT - chrome::GUTTER_WIDTH) + self.editor_added_padding() * 0.8
    }
    fn editor_vertical_padding(&self) -> Pixels {
        self.editor_text_top() + px(chrome::TEXT_TOP)
    }
    pub(super) fn editor_visible_capacity(&self) -> usize {
        self.editor_bounds.map_or(50, |bounds| {
            ((f32::from(bounds.size.height) - SCROLLBAR_SIZE - f32::from(self.editor_vertical_padding()))
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
        self.pref_bool("ScintillaPrimaryView.indentGuideLine", true)
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
        let shown = !self.editor_indent_guides_shown();
        self.set_preference("ScintillaPrimaryView.indentGuideLine", shown.into(), window, cx);
        self.editor_indent_guides_shown()
    }
    fn editor_scrollbars(&self) -> Option<EditorScrollbars> {
        let bounds = self.editor_bounds?;
        Some(EditorScrollbars::new(
            bounds,
            self.editor_line_height(),
            (self.editor_visual_row_count()
                + if self.pref_bool("ScintillaPrimaryView.scrollBeyondLastLine", true) {
                    self.editor_visible_capacity().saturating_sub(1)
                } else {
                    0
                }) as f32,
            self.editor_top_line,
            self.editor_chrome.content_width,
            self.editor_horizontal_offset,
            self.editor_text_inset(),
            self.editor_vertical_padding(),
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

    fn dispatch_actual_first_line_edits(&mut self, cx: &mut Context<Self>) {
        let enabled = self.pref_bool("NewDocDefaultSettings.useContentAsTabName", false);
        let mut edits = std::collections::BTreeMap::new();
        for buffer in self.workspace.buffers() {
            for pane in [crate::workspace::ViewId::Primary, crate::workspace::ViewId::Secondary] {
                let Some(editor) = self.workspace.editor_in_view(pane, buffer.id()) else { continue; };
                let visible = self.workspace.active_id_in_view(pane) == Some(buffer.id());
                let document = editor.document();
                document.set_actual_first_line_edit_notifications(enabled && visible);
                if let Some(edit) = document.take_actual_first_line_edit() {
                    edits.insert(buffer.id(), edit);
                }
            }
        }
        for (id, edit) in edits {
            self.actual_first_line_edit(id, edit.modified_line, cx);
        }
    }

    pub(super) fn editor_changed(
        &mut self,
        result: Result<(), EditError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_actual_first_line_edits(cx);
        self.invalidate_changed_incremental_count();
        if result.is_ok() {
            self.restart_appearance_caret(self.pref_int("Caret.blinkRate", 500), window, cx);
            if self.workspace.smart_highlight_allowed() {
                self.update_appearance_smart_token();
            } else {
                self.preferences.appearance.smart_token = None;
            }
            self.split_sync_source = Some(self.pane);
            let revision = self.editor().display_revision();
            if revision != self.editor_observed_revision || !self.workspace.clickable_links_allowed() {
                self.editor_chrome.hovered_link = None;
            }
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
        self.editor_error = result.err().map(|error| crate::localization::Text::from(error.to_string()));
        if self.editor_error.is_some() {
            self.editor_error_revision = self.editor_error_revision.wrapping_add(1);
        }
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
            self.editor_mut().set_display_selections(selections)
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
            self.editor_mut().set_display_selections(selections)
        }
    }

    pub(super) fn move_editor(
        &mut self,
        right: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.prepare_appearance_column_edit(cx) {
            return;
        }
        self.record_macro_step(MacroStep::Character { right, extend });
        let selections = self.editor().selections();
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
                } else if right
                    && self.pref_bool("ScintillaPrimaryView.virtualSpace", false)
                    && (selected.head_virtual > 0
                        || selection.head.0 == self.editor().document().byte_snapshot().len()
                        || matches!(
                            self.editor()
                                .document()
                                .byte_snapshot()
                                .byte(selection.head.0),
                            Some(b'\r' | b'\n')
                        ))
                {
                    (selection.head, selected.head_virtual + 1)
                } else if !right && selected.head_virtual > 0 {
                    (selection.head, selected.head_virtual - 1)
                } else {
                    (
                        self.visible_editor_source_position(
                            self.editor().adjacent_position(selection.head, right)?,
                            right,
                        )?,
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
        let result = moved.and_then(|moved| {
            if self.editor().has_composition() {
                self.select_editor_set(moved)
            } else {
                self.editor_mut().set_selections(moved)
            }
        });
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

    fn hit_editor_virtual(&self, position: Point<Pixels>, head: ByteOffset) -> usize {
        if !self.pref_bool("ScintillaPrimaryView.virtualSpace", false) {
            return 0;
        }
        self.editor_lines
            .iter()
            .rev()
            .find(|line| position.y >= line.origin.y)
            .or_else(|| self.editor_lines.first())
            .filter(|line| line.last && head.0 == line.end)
            .map_or(0, |line| {
                ((f32::from(position.x - line.origin.x - line.x_for_byte(line.end))
                    / f32::from(line.space_width).max(1.))
                .round()
                .max(0.)) as usize
            })
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
            let shape = self.editor_row_shape(&layout, visual.logical_line, window)?;
            let geometry = selection_geometry::RowGeometry {
                start: visual.range.start,
                end: visual.range.end,
                last: visual.last,
                space_width: f32::from(space_width),
            };
            let hit = |x: Pixels| {
                let offset = shape.x_for_index(visual.range.start - start) - px(visual.indent);
                let nearest = self.closest_editor_index_for_x(&shape, start, x + offset)?;
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
        self.editor_mut().set_display_selections(selections)
    }

    pub(super) fn move_line_edge(
        &mut self,
        end: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.prepare_appearance_column_edit(cx) {
            return;
        }
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
                let logical_end = layout.ends[visual.logical_line];
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
        if !self.prepare_appearance_column_edit(cx) {
            return;
        }
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
            let current_shape = self.editor_row_shape(&layout, current.logical_line, window)?;
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
                let shape = self.editor_row_shape(&layout, target.logical_line, window)?;
                let start = layout.starts[target.logical_line];
                let target_x =
                    x - px(target.indent) + shape.x_for_index(target.range.start - start);
                let mut byte = (start + self.closest_editor_index_for_x(&shape, start, target_x)?)
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

    fn visible_editor_source_position(
        &self,
        position: ByteOffset,
        forward: bool,
    ) -> Result<ByteOffset, EditError> {
        if self.editor().has_composition() {
            return Ok(self.visible_editor_position(position, forward));
        }
        let projection = self.editor().document().projection();
        let display =
            projection.display_position(position, crate::byte_projection::Affinity::After)?;
        let visible = self.visible_editor_position(ByteOffset(display.0), forward);
        if visible.0 == display.0 {
            Ok(position)
        } else {
            projection.source_position(
                crate::byte_projection::DisplayByteOffset(visible.0),
                crate::byte_projection::Affinity::After,
            )
        }
    }

    fn visible_editor_position(&self, position: ByteOffset, forward: bool) -> ByteOffset {
        let collapsed = self.editor_collapsed_headers();
        let folds = self.udl_fold_ranges();
        let text = self.editor().display_snapshot();
        let line = text.byte_to_line_idx(position.0, LineType::LF_CR);
        let mut hidden = self.editor_hidden_lines();
        hidden.extend(folds.iter().filter(|(header, _)| collapsed.contains(header))
            .map(|&(header, end)| (header + 1, end)));
        hidden.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in hidden {
            if let Some(last) = merged.last_mut()
                && start <= last.1 + 1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        if let Some(&(start, end)) = merged.iter().find(|&&(start, end)| line >= start && line <= end) {
            let header = start.saturating_sub(1);
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
            let current_shape = self.editor_row_shape(&layout, layout.rows.rows()[row].logical_line, window)?;
            let x = self.editor_preferred_x.unwrap_or_else(|| {
                let visual = &layout.rows.rows()[row];
                let start = layout.starts[visual.logical_line];
                let shape = &current_shape;
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
                let shape = self.editor_row_shape(&layout, target.logical_line, window)?;
                let target_x =
                    x - px(target.indent) + shape.x_for_index(target.range.start - start);
                let mut byte = (start + self.closest_editor_index_for_x(&shape, start, target_x)?)
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
                let shape = self.editor_row_shape(&layout, visual.logical_line, window)?;
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
                    let selection = self.editor_display_selections().primary();
                    let layout = self.editor_layout_for_width(self.editor_text_width(), window);
                    let row = layout.rows.row_for_byte(selection.selection.anchor.0);
                    let visual = &layout.rows.rows()[row];
                    let start = layout.starts[visual.logical_line];
                    let shape = match self.editor_row_shape(&layout, visual.logical_line, window) {
                        Ok(shape) => shape,
                        Err(error) => {
                            self.editor_changed(Err(error), window, cx);
                            return;
                        }
                    };
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
            if event.modifiers.control && event.click_count == 2 {
                let result = self
                    .editor()
                    .document()
                    .projection()
                    .source_selection(Selection { anchor: head, head })
                    .and_then(|selection| self.editing_delimiter_selection(selection.head))
                    .and_then(|selection| {
                        if let Some(selection) = selection {
                            self.editor_mut().select(selection)?;
                        }
                        Ok(())
                    });
                self.editor_dragging = false;
                self.editor_changed(result, window, cx);
                cx.stop_propagation();
                return;
            }
            if event.modifiers.control
                && self.pref_bool("ScintillaPrimaryView.multiSelection", true)
            {
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
                let virtual_space = self.hit_editor_virtual(event.position, head);
                let selected = crate::editor::SelectionRange {
                    selection: Selection { anchor: head, head },
                    anchor_virtual: virtual_space,
                    head_virtual: virtual_space,
                };
                let result = self.editor_mut().tentative_display_selection(selected);
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
                if let Some(range) = self.editor_link_at(head) {
                    let result = self
                        .editor_mut()
                        .select_display(Selection { anchor: head, head });
                    let selected = result.is_ok();
                    self.editor_dragging = false;
                    self.editor_preferred_x = None;
                    self.editor_changed(result, window, cx);
                    if selected {
                        let projection = self.editor().document().projection();
                        if let (Ok(start), Ok(end)) = (
                            projection.display_position(range.start, crate::byte_projection::Affinity::Before),
                            projection.display_position(range.end, crate::byte_projection::Affinity::After),
                        ) {
                            cx.open_url(&text[start.0..end.0]);
                        }
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
                && !self.pref_bool("ScintillaPrimaryView.disableSelectedTextDragDrop", false)
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
            let virtual_space = self.hit_editor_virtual(event.position, head);
            let anchor_virtual = if event.modifiers.shift {
                self.editor_display_selections().primary().anchor_virtual
            } else {
                virtual_space
            };
            let result = crate::editor::SelectionSet::new(
                vec![crate::editor::SelectionRange {
                    selection: Selection { anchor, head },
                    anchor_virtual,
                    head_virtual: virtual_space,
                }],
                0,
            )
            .and_then(|selections| self.editor_mut().set_display_selections(selections));
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
        let hovered_link = self.editor_bounds.filter(|bounds| bounds.contains(&event.position))
            .and_then(|_| self.hit_editor_character(event.position))
            .and_then(|position| self.editor_link_at(position));
        if self.editor_chrome.hovered_link != hovered_link {
            self.editor_chrome.hovered_link = hovered_link;
            cx.notify();
        }
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
                selected.head_virtual = self.hit_editor_virtual(event.position, head);
                let result = self.editor_mut().tentative_display_selection(selected);
                if result.is_ok() {
                    self.editor_chrome.selection_drag =
                        Some(self.editor_display_selections().primary_index());
                }
                self.editor_changed(result, window, cx);
                return;
            }
            let mut selected = self.editor_display_selections().primary();
            selected.selection.head = head;
            selected.head_virtual = self.hit_editor_virtual(event.position, head);
            let result = crate::editor::SelectionSet::new(vec![selected], 0)
                .and_then(|selections| self.editor_mut().set_display_selections(selections));
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
            layout.content_width.get()
        };
        self.editor_chrome.layout = Some(layout);
        let viewport_width = self
            .editor_bounds
            .map(|bounds| {
                (bounds.size.width - self.editor_text_inset() - px(SCROLLBAR_SIZE)).max(px(1.))
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
        self.editor_top_line = first.min(
            if self.pref_bool("ScintillaPrimaryView.scrollBeyondLastLine", true) {
                total.saturating_sub(1)
            } else {
                total.saturating_sub(capacity)
            },
        );
        self.split_sync_source = Some(self.pane);
        self.editor_follow_caret = false;
        self.editor_painted_revision = None;
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn render_editor(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.dispatch_actual_first_line_edits(cx);
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
            .bg(rgb(self.editor_style_profile().background))
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
                if !view.prepare_appearance_column_edit(cx) {
                    return;
                }
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
                if !view.prepare_appearance_column_edit(cx) {
                    return;
                }
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
            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                if view.pane == pane && !hovered && view.editor_chrome.hovered_link.take().is_some() {
                    cx.notify();
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
                        if !view.pref_bool("ScintillaPrimaryView.rightClickKeepsSelection", false) {
                            let result = view.editor_mut().context_select(head);
                            view.editor_changed(result, window, cx);
                        }
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
            .when_some(self.editor_error.as_ref().map(|error| error.render(&self.native_locale())), |editor, error| {
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
        let caret = self.editor().display_selection().head;
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
        self.notify_localized_editor_error(error.into(), cx);
    }

    pub(super) fn notify_localized_editor_error(&mut self, error: crate::localization::Text, cx: &mut Context<Self>) {
        self.editor_error_revision = self.editor_error_revision.wrapping_add(1);
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
        let selected = self.editor().display_selection();
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
        let filtered;
        let text = if self.pref_bool("ScintillaPrimaryView.npcNoInputC0", true) {
            filtered = text.chars().filter(|ch| !matches!(*ch, '\u{0}'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}')).collect::<String>();
            if filtered.is_empty() && !text.is_empty() {
                return;
            }
            filtered.as_str()
        } else {
            text
        };
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
    margins: AnyElement,
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
        self.view.update(cx, |view, cx| {
            let previous = view.pane;
            view.scope_editor_pane(self.pane);
            view.editor_chrome.text_system = Some(window.text_system().clone());
            view.refresh_editor_syntax();
            view.capture_udl_text_system(window);
            if previous == self.pane {
                view.refresh_editor_brace_highlight(cx);
            }
            let text = view.editor().display_snapshot();
            let mut lines = Vec::new();
            let mut visible_shapes = std::collections::BTreeMap::new();
            let text_origin = view.editor_text_origin();
            let text_inset = view.editor_text_inset();
            let text_width = (bounds.size.width - text_inset - px(SCROLLBAR_SIZE)).max(px(1.));
            view.editor_chrome.viewport_bounds = Some(bounds);
            let layout = view.editor_layout_for_width(text_width, window);
            let mut content_width = if view.editor_word_wrap_enabled() {
                (text_width - px(1.)).max(px(0.))
            } else {
                layout.content_width.get()
            };
            let mut top = bounds.top() + view.editor_text_top();
            let line_height = view.editor_line_height();
            let capacity =
                ((f32::from(bounds.size.height) - SCROLLBAR_SIZE - f32::from(view.editor_vertical_padding()))
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
                layout.rows.len()
                    + if view.pref_bool("ScintillaPrimaryView.scrollBeyondLastLine", true) {
                        capacity.saturating_sub(1)
                    } else {
                        0
                    },
            );
            let top_line = rows.start;
            for index in rows {
                if top >= bounds.bottom() || index >= layout.rows.len() {
                    break;
                }
                let visual = &layout.rows.rows()[index];
                let logical = visual.logical_line;
                let raw = text.line(logical, LineType::LF_CR).to_string();
                let shape = if let Some(shape) = visible_shapes.get(&logical) {
                    Arc::clone(shape)
                } else {
                    match view.editor_row_shape(&layout, logical, window) {
                        Ok(shape) => {
                            visible_shapes.insert(logical, shape.clone());
                            shape
                        }
                        Err(error) => {
                            view.notify_editor_error(error.to_string(), cx);
                            break;
                        }
                    }
                };
                if !view.editor_word_wrap_enabled() {
                    content_width = content_width.max(shape.width());
                }
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
            layout.shapes.replace(visible_shapes);
            if !view.editor_word_wrap_enabled() {
                layout.content_width.set(content_width);
                view.editor_chrome.unwrapped_content_width = content_width;
            }
            let mut horizontal_offset = if view.editor_word_wrap_enabled() {
                px(0.)
            } else {
                view.editor_horizontal_offset
            };
            if view.editor_follow_caret && !view.editor_word_wrap_enabled() {
                let caret = view.editor().display_selection().head.0;
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
            let mut margins = div()
                .id(("editor-margins", self.pane as usize))
                .relative()
                .w(bounds.size.width)
                .h(bounds.size.height)
                .child(view.render_bookmark_gutter(lines.iter().map(|line| (line.logical_line, line.first)), cx))
                .children(view.render_editor_hidden_markers(&lines, cx))
                .into_any_element();
            margins.prepaint_as_root(
                bounds.origin,
                size(gpui::AvailableSpace::Definite(bounds.size.width), gpui::AvailableSpace::Definite(bounds.size.height)),
                window,
                cx,
            );
            let paint = EditorPaint {
                margins,
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
            let selection = view.editor().display_selection();
            let hovered_link = if view.workspace.clickable_links_allowed() && view.preferences.files.links.enabled() {
                view.editor_chrome.hovered_link.as_ref().and_then(|range| {
                    let projection = view.editor().document().projection();
                    let start = projection.display_position(range.start, crate::byte_projection::Affinity::Before).ok()?;
                    let end = projection.display_position(range.end, crate::byte_projection::Affinity::After).ok()?;
                    Some(start.0..end.0)
                })
            } else {
                None
            };
            let selections = view.editor_display_selections();
            let history_spans = if view.change_history_runtime_flags() & 4 != 0 {
                if let (Some(first), Some(last)) = (prepaint.lines.first(), prepaint.lines.last()) {
                    let document = view.editor().document();
                    let projection = document.projection();
                    let bytes = document.byte_snapshot();
                    let projected = (|| -> Result<Vec<PaintedHistorySpan>, EditError> {
                        use crate::byte_projection::{Affinity, DisplayByteOffset};
                        let start = projection.source_position(DisplayByteOffset(first.start), Affinity::Before)?;
                        let end = projection.source_position(DisplayByteOffset(last.end), Affinity::After)?;
                        document.change_history_spans(start..end)?.into_iter().map(|span| {
                            match span {
                                crate::editor::ChangeHistorySpan::Insertion { range, edition } => {
                                    let start = projection.display_position(range.start, Affinity::Before)?.0;
                                    let end = projection.display_position(range.end, Affinity::After)?.0;
                                    Ok(PaintedHistorySpan::Insertion { range: start..end, edition })
                                }
                                crate::editor::ChangeHistorySpan::Deletion { position, editions } => {
                                    let second = if position.0 == bytes.len() {
                                        None
                                    } else {
                                        let second = bytes.move_position_outside_char(position.0 + 1, true, false)
                                            .map_err(|error| EditError::Storage(error.to_string()))?;
                                        Some(projection.display_position(ByteOffset(second), Affinity::After)?.0)
                                    };
                                    let position = projection.display_position(position, Affinity::Before)?.0;
                                    Ok(PaintedHistorySpan::Deletion { position, second, editions })
                                }
                            }
                        }).collect()
                    })();
                    match projected {
                        Ok(spans) => spans,
                        Err(error) => {
                            view.notify_editor_error(error.to_string(), cx);
                            Vec::new()
                        }
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            let mut tag_indicators = view.builtin_tag_indicators();
            if let (Some(first), Some(last)) = (prepaint.lines.first(), prepaint.lines.last()) {
                if view.workspace.smart_highlight_allowed() {
                    tag_indicators.extend(view.appearance_smart_indicators(first.start, last.end));
                }
                const COLORS: [u32; 7] = [
                    0xff0000, 0x00ffff, 0xff8000, 0xffff00, 0x8000ff, 0x008000, 0x00ff00,
                ];
                const STYLES: [&str; 7] = [
                    "Find Mark Style",
                    "Mark Style 1",
                    "Mark Style 2",
                    "Mark Style 3",
                    "Mark Style 4",
                    "Mark Style 5",
                    "Find matches highlight all",
                ];
                let indicators = match view
                    .editor()
                    .document()
                    .display_indicators_in(first.start..last.end)
                {
                    Ok(indicators) => indicators,
                    Err(error) => {
                        view.notify_editor_error(error.to_string(), cx);
                        Vec::new()
                    }
                };
                tag_indicators.extend(indicators.into_iter().map(|(slot, range)| {
                    crate::builtin_language::TagIndicator {
                        range: range.start.0..range.end.0,
                        color: view.style_colour(STYLES[slot], "bgColor", COLORS[slot]),
                        alpha: 100,
                    }
                }));
            }
            let focused = focus.is_focused(window);
            let dark = view.dark;
            let colour =
                |name, attribute, fallback| rgb(view.style_colour(name, attribute, fallback));
            let caret_colour = colour("Caret colour", "fgColor", 0x000000);
            let additional_caret_colour = colour("Multi-edit carets color", "fgColor", 0x404040);
            let selection_colour = colour("Selected text colour", "bgColor", 0x808080);
            let additional_selection_colour =
                colour("Multi-selected text color", "bgColor", 0xc0c0c0);
            let current_line_colour = colour(
                "Current line background colour",
                "bgColor",
                if dark { 0x101010 } else { 0xfbfcd9 },
            );
            let guide_style = view.style_config.config.effective_global_style(37, dark);
            let guide_mask = guide_style.as_ref().and_then(|style| style.attributes.get("colorStyle"))
                .and_then(|value| value.parse::<u8>().ok()).unwrap_or(3);
            let guide_colour = |key: &str, bit: u8, fallback: u32| rgb(if guide_mask & bit != 0 {
                guide_style.as_ref().and_then(|style| style.attributes.get(key))
                    .and_then(|value| u32::from_str_radix(value, 16).ok())
                    .filter(|value| *value <= 0xffffff).unwrap_or(fallback)
            } else { fallback });
            let guide_foreground = guide_colour("fgColor", 1, view.editor_style_profile().foreground);
            let guide_background = guide_colour("bgColor", 2, view.editor_style_profile().background);
            let brace_highlight = view.editor_chrome.brace_highlight.as_ref()
                .filter(|highlight| view.workspace.brace_match_allowed() && highlight.revision == view.editor().display_revision())
                .map(|highlight| (highlight.ranges.clone(), view.style_config.appearance(
                    None, highlight.style, dark, &view.editor_style_profile(), view.editor_zoom_delta(),
                )));
            let whitespace_colour = colour("White space symbol", "fgColor", 0xffb56a);
            let total = (prepaint.layout.rows.len()
                + if view.pref_bool("ScintillaPrimaryView.scrollBeyondLastLine", true) {
                    view.editor_visible_capacity().saturating_sub(1)
                } else {
                    0
                }) as f32;
            let caret_logical = prepaint.layout.rows.rows()
                [prepaint.layout.rows.row_for_byte(selection.head.0)]
            .logical_line;
            let line_height = view.editor_line_height();
            let caret_font = window.text_system().resolve_font(&gpui::font(view.editor_font_family()));
            let caret_cap_height = window.text_system().cap_height(caret_font, view.editor_font_size());
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.view.clone()),
                cx,
            );
            let mut failure = None;
            let text_bounds = Bounds::new(
                point(bounds.left() + view.editor_text_origin(), bounds.top()),
                size(
                    (bounds.size.width
                        - view.editor_text_origin()
                        - px(view.appearance_padding(false) + SCROLLBAR_SIZE))
                    .max(px(0.)),
                    (bounds.size.height - px(SCROLLBAR_SIZE)).max(px(0.)),
                ),
            );
            window.with_content_mask(
                Some(ContentMask {
                    bounds: text_bounds,
                }),
                |window| {
                    let columns = view
                        .pref_string("ScintillaPrimaryView.edgeMultiColumnPos", "")
                        .split(' ')
                        .filter(|part| !part.is_empty() && part.len() < 5)
                        .filter_map(|part| part.parse::<u16>().ok())
                        .filter(|column| *column <= 8192)
                        .collect::<Vec<_>>();
                    let space = prepaint
                        .lines
                        .first()
                        .map_or(px(1.), |line| line.space_width);
                    let edge_color = rgb(view.style_colour("Edge colour", "fgColor", 0xc0c0c0));
                    for column in &columns {
                        let x = text_bounds.left() + space * f32::from(*column)
                            - prepaint.horizontal_offset;
                        let width = if columns.len() == 1
                            && view.pref_bool("ScintillaPrimaryView.isEdgeBgMode", false)
                        {
                            (text_bounds.right() - x).max(px(0.))
                        } else {
                            px(1.)
                        };
                        window.paint_quad(fill(
                            Bounds::new(
                                point(x, text_bounds.top()),
                                size(width, text_bounds.size.height),
                            ),
                            edge_color,
                        ));
                    }
                    for line in &prepaint.lines {
                        let indicator_mode =
                            view.pref_int("ScintillaPrimaryView.currentLineIndicator", 1);
                        if caret_logical == line.logical_line && indicator_mode == 1 {
                            let highlight = Bounds::new(
                                point(
                                    text_bounds.left(),
                                    line.clip.top(),
                                ),
                                size(text_bounds.size.width, line.clip.size.height),
                            );
                            window.paint_quad(fill(
                                highlight,
                                current_line_colour,
                            ));
                            for top in [highlight.top(), highlight.bottom() - px(2.)] {
                                window.paint_quad(fill(
                                    Bounds::new(point(highlight.left(), top), size(highlight.size.width, px(2.))),
                                    rgb(if dark { 0x303030 } else { 0xf0f0f0 }),
                                ));
                            }
                        }
                        let mut clip = line.clip;
                        if line.last {
                            clip.size.width = (text_bounds.right() - clip.left()).max(px(0.));
                        }
                        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                            if (caret_logical != line.logical_line || indicator_mode != 1)
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
                            for column in &columns {
                                let x = text_bounds.left() + space * f32::from(*column)
                                    - prepaint.horizontal_offset;
                                let width = if columns.len() == 1
                                    && view.pref_bool("ScintillaPrimaryView.isEdgeBgMode", false)
                                {
                                    (text_bounds.right() - x).max(px(0.))
                                } else {
                                    px(1.)
                                };
                                window.paint_quad(fill(
                                    Bounds::new(point(x, line.origin.y), size(width, line_height)),
                                    edge_color,
                                ));
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
                            if let Some(range) = &hovered_link {
                                let selected = selection_geometry::SelectionGeometry {
                                    anchor: range.start, head: range.end, anchor_virtual: 0, head_virtual: 0,
                                };
                                if let Some(span) = line.geometry().highlight(selected, |byte| f32::from(line.x_for_byte(byte))) {
                                    let color = view.style_colour("URL hovered", "fgColor", 0x808080);
                                    if view.preferences.files.links.full_box() {
                                        window.paint_quad(fill(
                                            Bounds::new(point(px(span.start), line.origin.y), size(px(span.end - span.start), line_height)),
                                            rgba((color << 8) | 70),
                                        ).border_widths(px(1.)).border_color(rgb(color)));
                                    } else {
                                        window.paint_quad(fill(
                                            Bounds::new(point(px(span.start), line.origin.y + line_height - px(1.)), size(px(span.end - span.start), px(1.))),
                                            rgb(color),
                                        ));
                                    }
                                }
                            }
                            for x in &line.guides {
                                window.paint_quad(fill(
                                    Bounds::new(point(line.origin.x + *x, line.origin.y), size(px(1.), line_height)),
                                    guide_background,
                                ));
                                for y in (0..f32::from(line_height) as usize).step_by(3) {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(line.origin.x + *x, line.origin.y + px(y as f32)),
                                            size(px(1.), px(1.)),
                                        ),
                                        guide_foreground,
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
                                            point(line.origin.x + px(span.start), line.clip.top()),
                                            size(px(span.end - span.start), line.clip.size.height),
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
                            if let Some((ranges, appearance)) = &brace_highlight {
                                for range in ranges.iter().filter(|range| range.start >= line.start && range.end <= line.end) {
                                    let geometry = selection_geometry::SelectionGeometry {
                                        anchor: range.start, head: range.end, anchor_virtual: 0, head_virtual: 0,
                                    };
                                    let Some(span) = line.geometry().highlight(geometry, |byte| f32::from(line.x_for_byte(byte))) else { continue; };
                                    let selected = selections.ranges().iter().enumerate().find(|(_, selected)| {
                                        let selected = selected.selection.range();
                                        selected.start.0 <= range.start && range.end <= selected.end.0
                                    });
                                    let foreground = if selected.is_some() && view.pref_bool("ScintillaPrimaryView.selectedTextForegroundSingleColor", false) {
                                        view.style_colour("Selected text colour", "fgColor", 0x000000)
                                    } else { appearance.foreground };
                                    let background = selected.map_or(rgb(appearance.background), |(index, _)| {
                                        if index == selections.primary_index() { selection_colour } else { additional_selection_colour }
                                    });
                                    let mut font = gpui::font(appearance.font_family.clone());
                                    if appearance.bold { font = font.bold(); }
                                    if appearance.italic { font = font.italic(); }
                                    let text = view.editor().display_snapshot().slice(range.clone()).to_string();
                                    let shape = window.text_system().shape_line(text.clone().into(), px(appearance.font_size as f32), &[gpui::TextRun {
                                        len: text.len(), font, color: rgb(foreground).into(), background_color: None,
                                        underline: appearance.underline.then(|| gpui::UnderlineStyle { color: Some(rgb(foreground).into()), thickness: px(1.), wavy: false }),
                                        strikethrough: None,
                                    }], None);
                                    let cell = Bounds::new(point(line.origin.x + px(span.start), line.origin.y), size(px(span.end - span.start), line_height));
                                    window.with_content_mask(Some(ContentMask { bounds: cell }), |window| {
                                        window.paint_quad(fill(cell, background));
                                        let shift = px(layout::baseline_shift(f32::from(line_height), f32::from(line.shape.baseline()), f32::from(shape.ascent), f32::from(shape.descent)));
                                        if let Err(error) = shape.paint(point(cell.left(), cell.top() + shift), line_height, gpui::TextAlign::Left, None, window, cx) {
                                            failure = Some(error.to_string());
                                        }
                                    });
                                }
                            }
                            if let Err(error) = line.paint_change_history(&history_spans, view, line_height, window) {
                                failure = Some(error);
                            }
                        });
                        if focused && view.appearance_caret_visible() {
                            let caret_top_extension = (line_height - line.shape.baseline() * 2. + caret_cap_height).max(px(0.)).round();
                            let caret_height = line_height + caret_top_extension;
                            let caret_trim = caret_height * 0.05;
                            let caret_body_height = caret_height - caret_trim * 2.;
                            let ink = selections.ranges().iter()
                                .any(|selected| line.owns_caret(selected.selection.head.0))
                                .then(|| line.shape.ink_vertical_bounds(window))
                                .flatten()
                                .unwrap_or_else(|| line.shape.baseline() - caret_cap_height..line.shape.baseline());
                            let caret_top = (line.origin.y + (ink.start + ink.end - caret_body_height) / 2.).round();
                            let caret_clip_top = clip.top().min(caret_top);
                            let caret_clip_bottom = clip.bottom().max(caret_top + caret_body_height);
                            let caret_clip = Bounds::new(
                                point(clip.left(), caret_clip_top),
                                size(clip.size.width, caret_clip_bottom - caret_clip_top),
                            );
                            window.with_content_mask(Some(ContentMask { bounds: caret_clip }), |window| {
                                for (index, selected) in selections.ranges().iter().enumerate() {
                                    let caret_width = view.pref_int("Caret.width", 2);
                                    if caret_width == 0 {
                                        continue;
                                    }
                                    let mut caret_selected = *selected;
                                    if caret_width == 4
                                        && (selected.selection.head, selected.head_virtual)
                                            > (selected.selection.anchor, selected.anchor_virtual)
                                    {
                                        if caret_selected.head_virtual > 0 {
                                            caret_selected.head_virtual -= 1;
                                        } else {
                                            let snapshot = view.editor().display_snapshot();
                                            let character = snapshot
                                                .byte_to_char_idx(caret_selected.selection.head.0);
                                            caret_selected.selection.head = ByteOffset(
                                                snapshot
                                                    .char_to_byte_idx(character.saturating_sub(1)),
                                            );
                                        }
                                    }
                                    if let Some(x) = line
                                        .geometry()
                                        .caret(selection_geometry(caret_selected), |byte| {
                                            f32::from(line.x_for_byte(byte))
                                        })
                                    {
                                        let overwrite =
                                            view.editor().overwrite() && selected.is_empty();
                                        let width = if overwrite || caret_width >= 4 {
                                            let snapshot = view.editor().display_snapshot();
                                            let character = snapshot
                                                .byte_to_char_idx(caret_selected.selection.head.0);
                                            let next = snapshot
                                                .char_to_byte_idx(
                                                    (character + 1).min(snapshot.len_chars()),
                                                )
                                                .min(line.end);
                                            (line.x_for_byte(next) - px(x)).max(line.space_width)
                                        } else {
                                            px(caret_width as f32)
                                        };
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(
                                                    line.origin.x + px(x),
                                                    if overwrite {
                                                        line.origin.y + line_height - px(2.)
                                                    } else if caret_width < 4 {
                                                        caret_top
                                                    } else {
                                                        line.origin.y
                                                    },
                                                ),
                                                size(
                                                    width,
                                                    if overwrite {
                                                        px(2.)
                                                    } else if caret_width < 4 {
                                                        caret_body_height
                                                    } else {
                                                        line_height
                                                    },
                                                ),
                                            ),
                                            if index == selections.primary_index() {
                                                caret_colour
                                            } else {
                                                additional_caret_colour
                                            },
                                        ));
                                        if caret_width >= 4 && !overwrite {
                                            let caret_bounds = Bounds::new(
                                                point(line.origin.x + px(x), line.origin.y),
                                                size(width, line_height),
                                            );
                                            let inverse = view.editor_style_profile().background;
                                            window.with_content_mask(
                                                Some(ContentMask {
                                                    bounds: caret_bounds,
                                                }),
                                                |window| {
                                                    if let Err(error) = line.shape.paint_recolored(
                                                        line.origin,
                                                        line_height,
                                                        inverse,
                                                        window,
                                                        cx,
                                                    ) {
                                                        failure = Some(error);
                                                    }
                                                },
                                            );
                                        }
                                    }
                                }
                            });
                        }
                        if caret_logical == line.logical_line && indicator_mode == 2 {
                            paint_current_line_frame(
                                Bounds::new(
                                    point(text_bounds.left(), line.origin.y),
                                    size(text_bounds.size.width, line_height),
                                ),
                                px(
                                    view.pref_int("ScintillaPrimaryView.currentLineFrameWidth", 1)
                                        as f32,
                                ),
                                view.style_colour(
                                    "Current line background colour",
                                    "bgColor",
                                    if dark { 0x101010 } else { 0xfbfcd9 },
                                ),
                                window,
                            );
                        }
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
                view.editor_vertical_padding(),
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
            let layout_changed = view
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
            if layout_changed {
                cx.notify();
            }
            if let Some(error) = failure {
                view.notify_editor_error(error, cx);
            }
            view.scope_editor_pane(previous);
        });
        prepaint.margins.paint(window, cx);
    }
}
