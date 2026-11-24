use super::*;
#[path = "markdown_selection.rs"]
mod markdown_selection;

gpui::actions!(markdown_surface, [CopyMarkdownPreview, SelectAllMarkdownPreview]);
use crate::markdown::{
    Alignment, Destination, Media, PreparedInline, RenderDocument, RenderRow, RowContent,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, PartialEq, Eq)]
struct Owner {
    buffer: crate::workspace::BufferId,
    view: crate::workspace::ViewId,
    revision: crate::input::DisplayRevision,
    base: Option<PathBuf>,
    dark: bool,
}

enum Diagram {
    Image {
        image: Arc<gpui::Image>,
        width: f32,
        height: f32,
        pixels: u64,
    },
    Error(String),
}

const DIAGRAM_PIXEL_BUDGET: u64 = 32_000_000;

struct PreviewMenu {
    position: gpui::Point<gpui::Pixels>,
    menu: crate::menu::MenuBar,
}

struct TableLayout {
    rows: std::ops::Range<usize>,
    columns: std::cell::OnceCell<Vec<(f32, f32)>>,
    scroll: gpui::ScrollHandle,
    scrollbar_bounds: std::rc::Rc<std::cell::Cell<Bounds<gpui::Pixels>>>,
}

fn table_layouts(document: &RenderDocument) -> std::collections::BTreeMap<usize, TableLayout> {
    let mut tables = std::collections::BTreeMap::new();
    let mut index = 0;
    while index < document.rows.len() {
        if !matches!(document.rows[index].content, RowContent::Table { .. }) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while document
            .rows
            .get(index)
            .is_some_and(|row| matches!(row.content, RowContent::Table { heading: false, .. }))
        {
            index += 1;
        }
        tables.insert(
            start,
            TableLayout {
                rows: start..index,
                columns: Default::default(),
                scroll: gpui::ScrollHandle::new(),
                scrollbar_bounds: Default::default(),
            },
        );
    }
    tables
}

fn inline_font(style: &crate::markdown::InlineStyle, bold: bool) -> gpui::Font {
    let mut font = gpui::font(if style.code { "Menlo" } else { "Helvetica" });
    if style.bold || bold {
        font.weight = gpui::FontWeight::BOLD;
    }
    if style.italic {
        font.style = gpui::FontStyle::Italic;
    }
    font
}

fn table_text_width(
    inline: &PreparedInline,
    range: std::ops::Range<usize>,
    heading: bool,
    window: &Window,
) -> f32 {
    if range.is_empty() {
        return 0.;
    }
    let first = inline
        .spans
        .partition_point(|span| span.range.end <= range.start);
    let runs = inline.spans[first..]
        .iter()
        .take_while(|span| span.range.start < range.end)
        .filter_map(|span| {
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            (start < end).then(|| gpui::TextRun {
                len: end - start,
                font: inline_font(&span.style, heading),
                color: rgb(0x1c1c1c).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            })
        })
        .collect::<Vec<_>>();
    f32::from(
        window
            .text_system()
            .shape_line(inline.text[range].to_owned().into(), px(15.), &runs, None)
            .width(),
    )
}

impl TableLayout {
    fn scroll_thumb(scroll: &gpui::ScrollHandle, width: f32) -> std::ops::Range<f32> {
        let width = width.max(0.);
        let viewport = f32::from(scroll.bounds().size.width).max(0.);
        let maximum = f32::from(scroll.max_offset().x).max(0.);
        let thumb = (width * viewport / (viewport + maximum).max(1.))
            .clamp(18_f32.min(width), width);
        let position = if maximum > 0. {
            (-f32::from(scroll.offset().x) / maximum).clamp(0., 1.)
        } else {
            0.
        };
        let left = position * (width - thumb);
        left..left + thumb
    }

    fn widths(&self, document: &RenderDocument, available: f32, window: &Window) -> Vec<f32> {
        let columns = self.columns.get_or_init(|| {
            let mut columns = Vec::<(f32, f32)>::new();
            for row in &document.rows[self.rows.clone()] {
                let RowContent::Table { cells, heading, .. } = &row.content else {
                    continue;
                };
                columns.resize(columns.len().max(cells.len()), (0., 0.));
                for (column, cell) in columns.iter_mut().zip(cells) {
                    let mut line_start = 0;
                    for line in cell.text.split_inclusive('\n') {
                        let end = line_start + line.trim_end_matches(['\r', '\n']).len();
                        column.1 =
                            column
                                .1
                                .max(table_text_width(cell, line_start..end, *heading, window));
                        line_start += line.len();
                    }
                    let mut word_start = 0;
                    for (offset, _) in unicode_linebreak::linebreaks(&cell.text) {
                        let end = word_start + cell.text[word_start..offset].trim_end().len();
                        column.0 =
                            column
                                .0
                                .max(table_text_width(cell, word_start..end, *heading, window));
                        word_start = offset;
                    }
                    if !cell.media.is_empty() {
                        column.0 = column.0.max(120.);
                        column.1 = column.1.max(240.);
                    }
                }
            }
            for (minimum, maximum) in &mut columns {
                *minimum = minimum.ceil() + 21.;
                *maximum = maximum.max(*minimum - 21.).ceil() + 21.;
            }
            columns
        });
        let minimum = columns.iter().map(|column| column.0).sum::<f32>();
        let maximum = columns.iter().map(|column| column.1).sum::<f32>();
        let extra = (available - 2. - minimum).max(0.).min(maximum - minimum);
        let flexibility = maximum - minimum;
        columns
            .iter()
            .map(|&(min, max)| {
                min + if flexibility > 0. {
                    extra * (max - min) / flexibility
                } else {
                    0.
                }
            })
            .collect()
    }
}

fn decode_diagram(value: &serde_json::Value, remaining_pixels: &mut u64) -> Diagram {
    use objc2::AnyThread;
    use objc2_foundation::{NSData, NSDataBase64DecodingOptions, NSString};
    let decoded = (|| {
        let encoded = value.get("png").and_then(serde_json::Value::as_str)?;
        let width = value.get("width").and_then(serde_json::Value::as_f64)? as f32;
        let height = value.get("height").and_then(serde_json::Value::as_f64)? as f32;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.
            || height <= 0.
            || width > 1_000_000.
            || height > 1_000_000.
        {
            return None;
        }
        let data = NSData::initWithBase64EncodedString_options(
            NSData::alloc(),
            &NSString::from_str(encoded),
            NSDataBase64DecodingOptions::empty(),
        )?;
        let bytes = data.to_vec();
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return None;
        }
        let bitmap_width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
        let bitmap_height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
        let pixels = u64::from(bitmap_width) * u64::from(bitmap_height);
        if bitmap_width == 0 || bitmap_height == 0 || bitmap_width > 8192 || bitmap_height > 8192 {
            return None;
        }
        if pixels > *remaining_pixels {
            return Some(Diagram::Error(
                "Preview diagram image limit reached (32 million pixels)".into(),
            ));
        }
        *remaining_pixels -= pixels;
        Some(Diagram::Image {
            image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)),
            width,
            height,
            pixels,
        })
    })();
    decoded.unwrap_or_else(|| {
        Diagram::Error(
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Mermaid returned an invalid image")
                .to_string(),
        )
    })
}

fn scroll_thumb(list: &gpui::ListState, height: gpui::Pixels) -> std::ops::Range<f32> {
    let height = f32::from(height).max(0.);
    let maximum = f32::from(list.max_offset_for_scrollbar().y).max(0.);
    let thumb = if height == 0. {
        0.
    } else {
        (height * height / (height + maximum)).clamp(24_f32.min(height), height)
    };
    let position = if maximum > 0. {
        (-f32::from(list.scroll_px_offset_for_scrollbar().y) / maximum).clamp(0., 1.)
    } else {
        0.
    };
    let top = position * (height - thumb);
    top..top + thumb
}


pub(super) struct State {
    pub(super) visible: bool,
    requested: Option<Owner>,
    completed: Option<Owner>,
    document: Option<Arc<RenderDocument>>,
    task: Option<gpui::Task<()>>,
    cancel: Arc<AtomicBool>,
    list: gpui::ListState,
    diagrams: std::collections::BTreeMap<Arc<str>, Diagram>,
    diagram_task: Option<gpui::Task<()>>,
    renderer: Option<super::mermaid::Renderer>,
    scroll_grab: Option<f32>,
    table_scroll_grab: Option<(usize, f32)>,
    scroll_hint_width: std::rc::Rc<std::cell::Cell<Option<gpui::Pixels>>>,
    tables: std::collections::BTreeMap<usize, TableLayout>,
    selection: markdown_selection::Selection,
    context_menu: Option<PreviewMenu>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            visible: false,
            requested: None,
            completed: None,
            document: None,
            task: None,
            cancel: Arc::new(AtomicBool::new(false)),
            list: gpui::ListState::new(0, gpui::ListAlignment::Top, px(256.)),
            diagrams: Default::default(),
            diagram_task: None,
            renderer: None,
            scroll_grab: None,
            table_scroll_grab: None,
            scroll_hint_width: Default::default(),
            tables: Default::default(),
            selection: Default::default(),
            context_menu: None,
        }
    }
}

impl MatrixWindow {
    fn markdown_menu_enabled(&self, command: MenuCommand) -> bool {
        self.markdown_selection_ready()
            && match command {
                MenuCommand::Edit(EditCommand::Copy) => {
                    let selection = &self.markdown_preview.selection;
                    selection.active && selection.anchor != selection.head
                }
                MenuCommand::Edit(EditCommand::SelectAll) => self
                    .markdown_preview
                    .document
                    .as_ref()
                    .is_some_and(|document| !document.rows.is_empty()),
                _ => false,
            }
    }

    fn markdown_menu_input(&mut self, event: MenuEvent, cx: &mut Context<Self>) {
        let copy = self.markdown_menu_enabled(MenuCommand::Edit(EditCommand::Copy));
        let select_all = self.markdown_menu_enabled(MenuCommand::Edit(EditCommand::SelectAll));
        let Some(menu) = &mut self.markdown_preview.context_menu else { return; };
        let outcome = menu.menu.handle(event, &|command| match command {
            MenuCommand::Edit(EditCommand::Copy) => copy,
            MenuCommand::Edit(EditCommand::SelectAll) => select_all,
            _ => false,
        });
        if menu.menu.mode() != MenuMode::Open {
            self.markdown_preview.context_menu = None;
        }
        match outcome {
            crate::menu::MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Copy)) => self.copy_markdown_preview(cx),
            crate::menu::MenuOutcome::Activate(MenuCommand::Edit(EditCommand::SelectAll)) => self.select_all_markdown_preview(cx),
            _ => {}
        }
        cx.notify();
    }

    fn markdown_menu_down(&mut self, event: &gpui::MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.markdown_selection_ready() { return; }
        let entries = [EditCommand::Copy, EditCommand::SelectAll]
            .into_iter()
            .map(|command| crate::menu::MenuEntry::Action(MenuCommand::Edit(command)))
            .collect();
        self.markdown_preview.context_menu = Some(PreviewMenu {
            position: event.position,
            menu: crate::menu::MenuBar::new(vec![crate::menu::MenuDefinition {
                selector: "markdown-preview-context-menu",
                label: "Markdown Preview",
                mnemonic: '\0',
                entries: Vec::new(),
            }]),
        });
        let selection = &mut self.markdown_preview.selection;
        selection.dragging = false;
        selection.down = None;
        selection.scroll_pending = false;
        selection.generation = selection.generation.wrapping_add(1);
        self.markdown_menu_input(MenuEvent::OpenContext(0, entries), cx);
        self.close_focus.focus(window, cx);
        cx.stop_propagation();
    }

    fn render_markdown_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.markdown_preview.context_menu.as_ref()?;
        let family = menu_surface::font_family(self, || cx.text_system().all_font_names());
        let font = gpui::font(family.clone());
        let font_size = px(menu_surface::font_size(self) as f32);
        let font_id = cx.text_system().resolve_font(&font);
        let line_height = px((f32::from(cx.text_system().ascent(font_id, font_size))
            + f32::from(cx.text_system().descent(font_id, font_size)).abs()).ceil());
        let row_height = (line_height + px(6.)).max(px(22.));
        let filename = self.native_locale();
        let foreground = if self.dark { 0xeeeeee } else { 0x202020 };
        let selected_background = self.appearance_colour("DarkMode.customColorMenuHotTrack", if self.dark { 0x444444 } else { 0x91c9f7 });
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let measure = |text: &str| text_system.shape_line(
            text.to_owned().into(), font_size, &[gpui::TextRun {
                len: text.len(), font: font.clone(), color: rgb(foreground).into(),
                background_color: None, underline: None, strikethrough: None,
            }], None,
        ).width().ceil();
        let items = [EditCommand::Copy, EditCommand::SelectAll].map(|edit| {
            let command = MenuCommand::Edit(edit);
            let label = command.source_id()
                .and_then(|id| crate::localization::menu_command(&filename, id))
                .unwrap_or_else(|| command.label()).to_owned();
            let shortcut = if edit == EditCommand::Copy { "⌘C" } else { "⌘A" };
            (command, label, shortcut)
        });
        let label_width = items.iter().map(|(_, label, _)| measure(label)).max().unwrap_or(px(0.));
        let shortcut_width = items.iter().map(|(_, _, shortcut)| measure(shortcut)).max().unwrap_or(px(0.));
        let width = (label_width + shortcut_width + px(100.)).max(px(192.));
        let selected = menu.menu.selected_path().first().copied();
        let revision = menu.menu.revision();
        let rows = items.into_iter().enumerate().map(|(index, (command, label, shortcut))| {
            let enabled = self.markdown_menu_enabled(command);
            let selected = selected == Some(index);
            div()
                .id(("markdown-menu-row", index))
                .debug_selector(move || if index == 0 { "markdown-menu-copy".into() } else { "markdown-menu-select-all".into() })
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .aria_keyshortcuts(shortcut)
                .aria_selected(selected)
                .a11y_synthetic_children(move |builder| {
                    if !enabled { builder.parent_node().set_disabled(); }
                })
                .when(selected && enabled, |row| row.bg(rgb(selected_background)).aria_active_descendant())
                .h(row_height)
                .w_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .pl(px(2.))
                .pr(px(10.))
                .text_color(rgb(if enabled { foreground } else { 0x808080 }))
                .on_mouse_move(cx.listener(move |view, _, _, cx| {
                    if enabled && view.markdown_preview.context_menu.as_ref()
                        .is_some_and(|menu| menu.menu.selected_path().first() != Some(&index)) {
                        view.markdown_menu_input(MenuEvent::Hover { root: 0, path: vec![index] }, cx);
                    }
                }))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.markdown_menu_input(MenuEvent::ClickAtRevision { root: 0, path: vec![index], revision }, cx);
                    cx.stop_propagation();
                }))
                .child(div().w(px(24.)).h_full().flex_shrink_0())
                .child(div().flex_1().min_w(px(0.)).truncate().child(label))
                .child(div().flex_shrink_0().child(shortcut))
                .child(div().w(px(6.)).flex_shrink_0())
                .into_any_element()
        }).collect::<Vec<_>>();
        Some(gpui::deferred(
            gpui::anchored().position(menu.position).snap_to_window().child(
                div()
                    .id("markdown-preview-context-menu")
                    .debug_selector(|| "markdown-preview-context-menu".into())
                    .role(Role::Menu)
                    .aria_label("Markdown Preview")
                    .w(width)
                    .font_family(family)
                    .text_size(font_size)
                    .line_height(line_height)
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(rgb(if self.dark { 0x555555 } else { 0xcccccc }))
                    .bg(rgb(if self.dark { 0x2b2b2b } else { 0xf0f0f0 }))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                        view.markdown_menu_input(MenuEvent::Dismiss, cx);
                    }))
                    .children(rows),
            ),
        ).into_any_element())
    }

    fn markdown_selection_ready(&self) -> bool {
        self.markdown_preview.visible
            && self.markdown_preview.completed == self.markdown_preview.requested
            && self.markdown_preview.completed.as_ref() == Some(&self.markdown_owner())
            && self.markdown_preview.document.is_some()
    }

    pub(super) fn copy_markdown_preview(&mut self, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.markdown_preview.context_menu.take().is_some() { cx.notify(); }
        if !self.markdown_selection_ready() { return; }
        let Some(document) = &self.markdown_preview.document else { return; };
        let base = self.markdown_preview.completed.as_ref().and_then(|owner| owner.base.as_deref());
        let Some((plain, html)) = markdown_selection::payload(document, &self.markdown_preview.selection, base) else { return; };
        if let Err(error) = markdown_selection::write(&plain, &html) {
            self.notify_editor_error(error.to_string(), cx);
        }
    }

    pub(super) fn select_all_markdown_preview(&mut self, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.markdown_preview.context_menu.take().is_some() { cx.notify(); }
        if !self.markdown_selection_ready() { return; }
        let Some(document) = &self.markdown_preview.document else { return; };
        if document.rows.is_empty() { return; }
        let selection = &mut self.markdown_preview.selection;
        selection.anchor = Default::default();
        selection.head = markdown_selection::Position { row: document.rows.len(), cell: 0, byte: 0 };
        selection.active = true;
        selection.dragging = false;
        selection.down = None;
        selection.scroll_pending = false;
        selection.generation = selection.generation.wrapping_add(1);
        cx.notify();
    }

    fn markdown_selection_down(&mut self, event: &gpui::MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.markdown_selection_ready() { return; }
        let viewport = self.markdown_preview.list.viewport_bounds();
        if !viewport.contains(&event.position) { return; }
        self.markdown_preview.selection.moved = false;
        let Some(position) = self.markdown_preview.selection.hit(event.position, viewport) else { return; };
        let selection = &mut self.markdown_preview.selection;
        selection.anchor = position;
        selection.head = position;
        selection.active = true;
        selection.dragging = true;
        selection.moved = false;
        selection.down = Some(event.position);
        selection.pointer = Some(event.position);
        selection.scroll_pending = false;
        selection.generation = selection.generation.wrapping_add(1);
        self.close_focus.focus(window, cx);
        cx.notify();
    }

    fn markdown_selection_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.markdown_preview.selection.dragging { return; }
        if event.pressed_button != Some(MouseButton::Left) {
            self.markdown_preview.selection.dragging = false;
            self.markdown_preview.selection.down = None;
            return;
        }
        let viewport = self.markdown_preview.list.viewport_bounds();
        let position = self.markdown_preview.selection.hit(event.position, viewport);
        let selection = &mut self.markdown_preview.selection;
        selection.pointer = Some(event.position);
        if let Some(down) = selection.down {
            selection.moved |= f32::from(event.position.x - down.x).abs() >= 3.
                || f32::from(event.position.y - down.y).abs() >= 3.;
        }
        if selection.moved {
            if let Some(position) = position { selection.head = position; }
            cx.notify();
        }
    }

    fn markdown_selection_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.markdown_preview.selection.dragging || self.markdown_preview.selection.scroll_pending { return; }
        let Some(pointer) = self.markdown_preview.selection.pointer else { return; };
        let viewport = self.markdown_preview.list.viewport_bounds();
        let amount = if pointer.y < viewport.top() { -12. } else if pointer.y > viewport.bottom() { 12. } else { return; };
        self.markdown_preview.selection.scroll_pending = true;
        let generation = self.markdown_preview.selection.generation;
        let entity = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            if let Err(error) = entity.update(cx, |view, cx| {
                if view.markdown_preview.selection.generation != generation { return; }
                view.markdown_preview.selection.scroll_pending = false;
                if !view.markdown_preview.selection.dragging { return; }
                let viewport = view.markdown_preview.list.viewport_bounds();
                if let Some(pointer) = view.markdown_preview.selection.pointer
                    && let Some(position) = view.markdown_preview.selection.hit(pointer, viewport)
                {
                    view.markdown_preview.selection.head = position;
                }
                if view.markdown_preview.selection.pointer.is_some_and(|pointer| pointer.y < viewport.top() || pointer.y > viewport.bottom()) {
                    view.markdown_preview.list.scroll_by(px(amount));
                }
                cx.notify();
                view.markdown_selection_scroll(window, cx);
            }) {
                eprintln!("Markdown selection lost its preview: {error}");
            }
        });
    }

    fn markdown_selection_link(&self, position: markdown_selection::Position) -> Option<String> {
        let row = self.markdown_preview.document.as_ref()?.rows.get(position.row)?;
        let inline = match &row.content {
            RowContent::Text { inline, .. } => inline,
            RowContent::Table { cells, .. } => cells.get(position.cell)?,
            _ => return None,
        };
        inline.spans.iter().find(|span| span.range.contains(&position.byte))?.style.link.clone()
    }

    fn markdown_selection_up(&mut self, event: &gpui::MouseUpEvent, allow_link: bool, cx: &mut Context<Self>) {
        if !self.markdown_preview.selection.dragging { return; }
        let viewport = self.markdown_preview.list.viewport_bounds();
        let selection = &mut self.markdown_preview.selection;
        if let Some(down) = selection.down {
            selection.moved |= f32::from(event.position.x - down.x).abs() >= 3.
                || f32::from(event.position.y - down.y).abs() >= 3.;
        }
        if selection.moved {
            if let Some(position) = selection.hit(event.position, viewport) { selection.head = position; }
        }
        let clicked = if allow_link && !selection.moved {
            selection.down.and_then(|down| selection.exact_hit(down)).zip(selection.exact_hit(event.position))
        } else { None };
        selection.dragging = false;
        selection.down = None;
        if let Some((down, up)) = clicked
            && let Some(target) = self.markdown_selection_link(down)
            && self.markdown_selection_link(up).as_ref() == Some(&target)
            && let Some(owner) = self.markdown_preview.completed.clone()
        {
            self.open_markdown_link(&target, &owner, cx);
        }
        cx.notify();
    }

    fn markdown_code_text(&self, row: usize, value: &str) -> AnyElement {
        let value = gpui::SharedString::from(value.to_owned());
        let run = gpui::TextRun {
            len: value.len(),
            font: gpui::font("Menlo"),
            color: rgb(if self.dark { 0xe6e6e6 } else { 0x202020 }).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let selected = self.markdown_preview.selection.range(row, 0, value.len());
        markdown_selection::Text {
            text: gpui::StyledText::new(value.clone()).with_runs(markdown_selection::selected_runs(vec![run], selected)),
            value,
            row,
            cell: 0,
            links: Vec::new(),
            geometry: self.markdown_preview.selection.geometry.clone(),
        }.into_any_element()
    }

    fn markdown_owner(&self) -> Owner {
        Owner {
            buffer: self.workspace.active().id(),
            view: self.workspace.active_view(),
            revision: self.editor().display_revision(),
            base: self
                .disk_file()
                .and_then(|file| file.path().parent())
                .map(std::path::Path::to_owned),
            dark: self.dark,
        }
    }

    pub(super) fn toggle_markdown_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.markdown_preview.visible {
            self.dismiss_floating(floating_windows::DialogKind::MarkdownPreview, window, cx);
        } else {
            self.menu_input(MenuEvent::Dismiss, window, cx);
            self.markdown_preview.visible = true;
            self.refresh_markdown_preview(cx);
            self.present_floating(floating_windows::DialogKind::MarkdownPreview, window, cx);
            cx.notify();
        }
    }

    pub(super) fn hide_markdown_preview(&mut self, cx: &mut Context<Self>) {
        self.markdown_preview.visible = false;
        self.markdown_preview.cancel.store(true, Ordering::Relaxed);
        self.markdown_preview.task = None;
        self.markdown_preview.requested = None;
        self.markdown_preview.completed = None;
        self.markdown_preview.document = None;
        self.markdown_preview.diagram_task = None;
        self.markdown_preview.renderer = None;
        self.markdown_preview.diagrams.clear();
        self.markdown_preview.tables.clear();
        self.markdown_preview.selection.clear();
        self.markdown_preview.context_menu = None;
        self.markdown_preview.scroll_grab = None;
        self.markdown_preview.table_scroll_grab = None;
        self.markdown_preview.list.scrollbar_drag_ended();
        cx.notify();
    }

    pub(super) fn refresh_markdown_preview(&mut self, cx: &mut Context<Self>) {
        if !self.markdown_preview.visible {
            return;
        }
        let owner = self.markdown_owner();
        if self.markdown_preview.requested.as_ref() == Some(&owner) {
            return;
        }
        self.markdown_preview.cancel.store(true, Ordering::Relaxed);
        self.markdown_preview.task = None;
        self.markdown_preview.diagram_task = None;
        self.markdown_preview.renderer = None;
        self.markdown_preview.selection.clear();
        self.markdown_preview.context_menu = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.markdown_preview.cancel = cancel.clone();
        if self
            .markdown_preview
            .completed
            .as_ref()
            .is_some_and(|previous| previous.buffer != owner.buffer)
        {
            self.markdown_preview.document = None;
        }
        self.markdown_preview.requested = Some(owner.clone());
        let snapshot = self.editor().display_snapshot();
        let task = cx.background_executor().spawn(async move {
            crate::markdown::render_document(&snapshot.to_string(), || {
                cancel.load(Ordering::Relaxed)
            })
        });
        self.markdown_preview.task = Some(cx.spawn(async move |view, cx| {
            let Some(document) = task.await else {
                return;
            };
            if let Err(error) = view.update(cx, |view, cx| {
                if !view.markdown_preview.visible
                    || view.markdown_preview.requested.as_ref() != Some(&owner)
                {
                    return;
                }
                let offset = view.markdown_preview.list.logical_scroll_top();
                let same_buffer = view
                    .markdown_preview
                    .completed
                    .as_ref()
                    .is_some_and(|previous| previous.buffer == owner.buffer);
                let previous = view
                    .markdown_preview
                    .document
                    .as_ref()
                    .and_then(|document| document.rows.get(offset.item_ix))
                    .map(|node| node.fingerprint);
                let matched = if same_buffer {
                    previous.and_then(|fingerprint| {
                        document
                            .rows
                            .iter()
                            .enumerate()
                            .filter(|(_, node)| node.fingerprint == fingerprint)
                            .min_by_key(|(index, _)| index.abs_diff(offset.item_ix))
                            .map(|(index, _)| index)
                    })
                } else {
                    None
                };
                view.markdown_scrollbar_up(cx);
                view.markdown_preview.list.reset(document.rows.len());
                view.markdown_preview.scroll_hint_width.set(None);
                if same_buffer {
                    view.markdown_preview.list.scroll_to(gpui::ListOffset {
                        item_ix: matched
                            .unwrap_or(offset.item_ix)
                            .min(document.rows.len().saturating_sub(1)),
                        offset_in_item: offset.offset_in_item,
                    });
                }
                if view
                    .markdown_preview
                    .completed
                    .as_ref()
                    .is_some_and(|previous| previous.dark != owner.dark)
                {
                    view.markdown_preview.diagrams.clear();
                }
                view.markdown_preview.tables = table_layouts(&document);
                view.markdown_preview.selection.clear();
                view.markdown_preview.context_menu = None;
                view.markdown_preview.document = Some(Arc::new(document));
                view.markdown_preview.completed = Some(owner);
                view.markdown_preview.task = None;
                view.refresh_mermaid(cx);
                cx.notify();
            }) {
                eprintln!("Markdown preview result lost its document: {error}");
            }
        }));
    }

    fn refresh_mermaid(&mut self, cx: &mut Context<Self>) {
        let Some(document) = &self.markdown_preview.document else {
            return;
        };
        let sources = document
            .rows
            .iter()
            .filter_map(|row| {
                if let RowContent::Mermaid { body } = &row.content {
                    Some(body.clone())
                } else {
                    None
                }
            })
            .collect::<std::collections::BTreeSet<_>>();
        self.markdown_preview.diagrams.retain(|source, diagram| {
            sources.contains(source) && matches!(diagram, Diagram::Image { .. })
        });
        let missing = sources
            .into_iter()
            .filter(|source| !self.markdown_preview.diagrams.contains_key(source))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return;
        }
        let owner = self.markdown_preview.completed.clone();
        let mut remaining_pixels = DIAGRAM_PIXEL_BUDGET.saturating_sub(
            self.markdown_preview
                .diagrams
                .values()
                .map(|diagram| {
                    if let Diagram::Image { pixels, .. } = diagram {
                        *pixels
                    } else {
                        0
                    }
                })
                .sum(),
        );
        let executor = cx.background_executor().clone();
        self.markdown_preview.diagram_task = Some(cx.spawn(async move |view, cx| {
            executor.timer(std::time::Duration::from_millis(150)).await;
            for source in missing {
                let started = view.update(cx, |view, _| {
                    if !view.markdown_preview.visible || view.markdown_preview.requested != owner {
                        return Ok(None);
                    }
                    super::mermaid::Renderer::start(&[source.as_ref()], view.dark).map(
                        |(renderer, receiver)| {
                            view.markdown_preview.renderer = Some(renderer);
                            Some(receiver)
                        },
                    )
                });
                let result = match started {
                    Ok(Ok(Some(receiver))) => {
                        let timeout = Box::pin(executor.timer(std::time::Duration::from_secs(30)));
                        match futures::future::select(receiver, timeout).await {
                            futures::future::Either::Left((Ok(result), _)) => result,
                            futures::future::Either::Left((Err(_), _)) => {
                                Err("Mermaid renderer stopped before completing".to_string())
                            }
                            futures::future::Either::Right(_) => {
                                Err("Mermaid rendering timed out".to_string())
                            }
                        }
                    }
                    Ok(Err(error)) => Err(error),
                    Ok(Ok(None)) | Err(_) => return,
                };
                let diagram = result.and_then(|text| {
                    let value: serde_json::Value =
                        serde_json::from_str(&text).map_err(|error| error.to_string())?;
                    match value.as_array() {
                        Some(rows) if rows.len() == 1 => {
                            Ok(decode_diagram(&rows[0], &mut remaining_pixels))
                        }
                        _ => Err(value
                            .get("error")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("Mermaid returned an invalid response")
                            .to_string()),
                    }
                });
                let applied = view.update(cx, |view, cx| {
                    if !view.markdown_preview.visible || view.markdown_preview.requested != owner {
                        return false;
                    }
                    view.markdown_preview.renderer = None;
                    view.markdown_preview
                        .diagrams
                        .insert(source, diagram.unwrap_or_else(Diagram::Error));
                    view.markdown_preview.list.remeasure();
                    cx.notify();
                    true
                });
                match applied {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        eprintln!("Mermaid result lost its preview: {error}");
                        return;
                    }
                }
            }
            let _ = view.update(cx, |view, _| {
                if view.markdown_preview.requested == owner {
                    view.markdown_preview.diagram_task = None;
                }
            });
        }));
    }

    fn markdown_scrollbar_down(&mut self, event: &gpui::MouseDownEvent, cx: &mut Context<Self>) {
        let list = &self.markdown_preview.list;
        let bounds = list.viewport_bounds();
        let thumb = scroll_thumb(list, bounds.size.height);
        let y = f32::from(event.position.y - bounds.top());
        if y < thumb.start {
            list.scroll_by(-bounds.size.height);
        } else if y > thumb.end {
            list.scroll_by(bounds.size.height);
        } else if list.max_offset_for_scrollbar().y > px(0.) {
            self.markdown_preview.scroll_grab = Some(y - thumb.start);
            list.scrollbar_drag_started();
        }
        cx.notify();
    }

    fn markdown_scrollbar_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if let Some((index, grab)) = self.markdown_preview.table_scroll_grab {
            if event.pressed_button != Some(MouseButton::Left) {
                self.markdown_scrollbar_up(cx);
                return;
            }
            if let Some(table) = self.markdown_preview.tables.get(&index) {
                let bounds = table.scrollbar_bounds.get();
                let thumb = TableLayout::scroll_thumb(&table.scroll, f32::from(bounds.size.width));
                let travel = f32::from(bounds.size.width) - (thumb.end - thumb.start);
                if travel > 0. {
                    let position = ((f32::from(event.position.x - bounds.left()) - grab) / travel)
                        .clamp(0., 1.);
                    table.scroll.set_offset(gpui::point(
                        -table.scroll.max_offset().x * position,
                        table.scroll.offset().y,
                    ));
                    cx.notify();
                }
            }
            return;
        }
        let Some(grab) = self.markdown_preview.scroll_grab else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.markdown_scrollbar_up(cx);
            return;
        }
        let list = &self.markdown_preview.list;
        let bounds = list.viewport_bounds();
        let thumb = scroll_thumb(list, bounds.size.height);
        let travel = f32::from(bounds.size.height) - (thumb.end - thumb.start);
        if travel > 0. {
            let position =
                ((f32::from(event.position.y - bounds.top()) - grab) / travel).clamp(0., 1.);
            if position >= 1. {
                list.scroll_to_end();
            } else {
                list.set_offset_from_scrollbar(gpui::point(
                    px(0.),
                    -list.max_offset_for_scrollbar().y * position,
                ));
            }
            cx.notify();
        }
    }

    fn markdown_scrollbar_up(&mut self, cx: &mut Context<Self>) {
        if self.markdown_preview.table_scroll_grab.take().is_some() {
            cx.notify();
        }
        if self.markdown_preview.scroll_grab.take().is_some() {
            self.markdown_preview.list.scrollbar_drag_ended();
            cx.notify();
        }
    }

    fn markdown_table_scrollbar(&self, table: &TableLayout, cx: &mut Context<Self>) -> AnyElement {
        let index = table.rows.start;
        let scroll = table.scroll.clone();
        let accessible = scroll.clone();
        let bounds = table.scrollbar_bounds.clone();
        let dark = self.dark;
        div()
            .id(("markdown-table-scrollbar", index))
            .accessibility_id(format!("markdown-table-scrollbar-{index}"))
            .role(Role::ScrollBar)
            .aria_label("Markdown table horizontal scrollbar")
            .aria_orientation(gpui::accesskit::Orientation::Horizontal)
            .a11y_synthetic_children(move |builder| {
                let maximum = f64::from(f32::from(accessible.max_offset().x).max(0.));
                let node = builder.parent_node();
                node.set_min_numeric_value(0.);
                node.set_max_numeric_value(maximum);
                node.set_numeric_value(
                    f64::from(-f32::from(accessible.offset().x)).clamp(0., maximum),
                );
                if maximum <= 0. {
                    node.set_disabled();
                }
            })
            .w_full()
            .h(px(14.))
            .flex_shrink_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                    let Some(table) = view.markdown_preview.tables.get(&index) else {
                        return;
                    };
                    let maximum = table.scroll.max_offset().x.max(px(0.));
                    if maximum > px(0.) {
                        let x = f32::from(event.position.x - table.scrollbar_bounds.get().left());
                        let thumb = TableLayout::scroll_thumb(
                            &table.scroll,
                            f32::from(table.scrollbar_bounds.get().size.width),
                        );
                        let offset = table.scroll.offset();
                        let page = table.scroll.bounds().size.width;
                        if x < thumb.start {
                            table.scroll.set_offset(gpui::point(
                                (offset.x + page).min(px(0.)),
                                offset.y,
                            ));
                        } else if x > thumb.end {
                            table.scroll.set_offset(gpui::point(
                                (offset.x - page).max(-maximum),
                                offset.y,
                            ));
                        } else {
                            view.markdown_preview.table_scroll_grab = Some((index, x - thumb.start));
                        }
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |track, _, window, _| {
                        bounds.set(track);
                        let maximum = f32::from(scroll.max_offset().x).max(0.);
                        if maximum <= 0. {
                            return;
                        }
                        let thumb = TableLayout::scroll_thumb(&scroll, f32::from(track.size.width));
                        window.paint_quad(gpui::fill(
                            track,
                            rgb(if dark { 0x282828 } else { 0xf1f1f1 }),
                        ));
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                track.origin + gpui::point(px(thumb.start), px(3.)),
                                size(
                                    px(thumb.end - thumb.start),
                                    (track.size.height - px(6.)).max(px(0.)),
                                ),
                            ),
                            rgb(if dark { 0x888888 } else { 0xaaaaaa }),
                        ));
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    fn markdown_scrollbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let list = self.markdown_preview.list.clone();
        let accessible = list.clone();
        let hint_width = self.markdown_preview.scroll_hint_width.clone();
        div()
            .id("markdown-scrollbar")
            .accessibility_id("markdown-preview-scrollbar")
            .role(Role::ScrollBar)
            .aria_label("Markdown preview vertical scrollbar")
            .aria_orientation(gpui::accesskit::Orientation::Vertical)
            .a11y_synthetic_children(move |builder| {
                let maximum = f64::from(f32::from(accessible.max_offset_for_scrollbar().y));
                let node = builder.parent_node();
                node.set_min_numeric_value(0.);
                node.set_max_numeric_value(maximum);
                node.set_numeric_value(f64::from(-f32::from(
                    accessible.scroll_px_offset_for_scrollbar().y,
                )));
                if maximum <= 0. {
                    node.set_disabled();
                }
            })
            .w(px(14.))
            .h_full()
            .flex_shrink_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event, _, cx| {
                    view.markdown_scrollbar_down(event, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let width = list.viewport_bounds().size.width;
                        if hint_width.get() != Some(width) {
                            list.clone().with_uniform_item_height(px(36.));
                            hint_width.set(Some(width));
                            window.on_next_frame(|window, _| window.refresh());
                        }
                        window.paint_quad(gpui::fill(bounds, rgb(0xf1f1f1)));
                        let thumb = scroll_thumb(&list, bounds.size.height);
                        if thumb.end > thumb.start {
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    bounds.origin + gpui::point(px(3.), px(thumb.start)),
                                    size(
                                        (bounds.size.width - px(6.)).max(px(0.)),
                                        px(thumb.end - thumb.start),
                                    ),
                                ),
                                rgb(0xaaaaaa),
                            ));
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    fn open_markdown_link(&mut self, destination: &str, owner: &Owner, cx: &mut Context<Self>) {
        if !self.markdown_preview.visible || self.markdown_owner() != *owner {
            return;
        }
        match crate::markdown::resolve_destination(destination, owner.base.as_deref()) {
            Destination::External(url) => cx.open_url(&url),
            Destination::Local(path) => {
                if let Ok(url) = url::Url::from_file_path(path) {
                    cx.open_url(url.as_str());
                }
            }
            Destination::Fragment(fragment) => {
                if let Some(document) = &self.markdown_preview.document
                    && let Some(&index) = document.anchors.get(&fragment)
                {
                    self.markdown_preview.list.scroll_to_reveal_item(index);
                    cx.notify();
                }
            }
            Destination::Blocked => {}
        }
    }

    fn markdown_inline(
        &self,
        inline: &PreparedInline,
        id: usize,
        row_index: usize,
        bold: bool,
    ) -> AnyElement {
        let foreground = rgb(if self.dark { 0xe6e6e6 } else { 0x202020 });
        let runs = inline
            .spans
            .iter()
            .map(|span| {
                let font = inline_font(&span.style, bold);
                gpui::TextRun {
                    len: span.range.len(),
                    font,
                    color: if span.style.link.is_some() {
                        rgb(0x0078d4).into()
                    } else {
                        foreground.into()
                    },
                    background_color: span
                        .style
                        .code
                        .then(|| rgb(if self.dark { 0x383838 } else { 0xeeeeee }).into()),
                    underline: span.style.link.as_ref().map(|_| gpui::UnderlineStyle {
                        thickness: px(1.),
                        ..Default::default()
                    }),
                    strikethrough: span.style.strike.then(|| gpui::StrikethroughStyle {
                        thickness: px(1.),
                        ..Default::default()
                    }),
                }
            })
            .collect();
        let value = gpui::SharedString::from(inline.text.clone());
        let selected = self.markdown_preview.selection.range(row_index, id, inline.text.len());
        markdown_selection::Text {
            text: gpui::StyledText::new(value.clone()).with_runs(markdown_selection::selected_runs(runs, selected)),
            value,
            row: row_index,
            cell: id,
            links: inline.spans.iter().filter(|span| span.style.link.is_some()).map(|span| span.range.clone()).collect(),
            geometry: self.markdown_preview.selection.geometry.clone(),
        }.into_any_element()
    }

    fn markdown_image(
        &self,
        media: &Media,
        id: usize,
        owner: &Owner,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let linked = media.navigation.is_some();
        let mut image = div()
            .id(("markdown-image", id))
            .debug_selector(move || {
                if linked {
                    "markdown-linked-image".into()
                } else {
                    "markdown-image".into()
                }
            })
            .role(Role::Image)
            .aria_label(media.alt.clone())
            .w_full()
            .min_w(px(0.));
        if let Destination::Local(path) =
            crate::markdown::resolve_destination(&media.destination, owner.base.as_deref())
        {
            image = image.child(
                div()
                    .w_full()
                    .h(px(240.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(
                        gpui::img(path)
                            .size_full()
                            .min_w(px(0.))
                            .min_h(px(0.))
                            .max_h(px(240.))
                            .object_fit(gpui::ObjectFit::Contain),
                    ),
            );
        } else {
            image = image.child(format!("Image: {} (not loaded automatically)", media.alt));
        }
        if !media.title.is_empty() {
            image = image.child(media.title.clone());
        }
        if let Some(target) = media.navigation.clone() {
            let owner = owner.clone();
            image = image.cursor_pointer().on_click(
                cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                    let dragged = match event {
                        gpui::ClickEvent::Mouse(event) => view.markdown_preview.selection.moved
                            || f32::from(event.up.position.x - event.down.position.x).abs() >= 3.
                            || f32::from(event.up.position.y - event.down.position.y).abs() >= 3.,
                        _ => false,
                    };
                    if !dragged {
                        view.open_markdown_link(&target, &owner, cx);
                    }
                }),
            );
        }
        image.into_any_element()
    }

    fn markdown_inline_block(
        &self,
        inline: &PreparedInline,
        id: usize,
        row_index: usize,
        bold: bool,
        owner: &Owner,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(("markdown-inline-block", id))
            .w_full()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .when(!inline.text.is_empty(), |block| {
                block.child(self.markdown_inline(inline, id, row_index, bold))
            })
            .children(
                inline
                    .media
                    .iter()
                    .enumerate()
                    .map(|(index, media)| self.markdown_image(media, index, owner, cx)),
            )
            .into_any_element()
    }

    fn markdown_row(
        &self,
        row_index: usize,
        row: &RenderRow,
        owner: &Owner,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = match &row.content {
            RowContent::Text { inline, heading } => {
                let size = heading.map_or(14., |level| {
                    [28., 24., 20., 18., 16., 15.][usize::from(level.saturating_sub(1)).min(5)]
                });
                div()
                    .debug_selector(move || {
                        if heading.is_some() {
                            "markdown-heading".into()
                        } else {
                            "markdown-paragraph".into()
                        }
                    })
                    .text_size(px(size))
                    .line_height(px(size * 1.45))
                    .mb(px(8.))
                    .child(self.markdown_inline_block(inline, 0, row_index, heading.is_some(), owner, cx))
                    .into_any_element()
            }
            RowContent::Code {
                body,
                range,
                language,
                first,
                last,
            } => div()
                .id("markdown-code-row")
                .debug_selector(|| "markdown-code-block".into())
                .px(px(10.))
                .bg(rgb(if self.dark { 0x282828 } else { 0xf6f8fa }))
                .font_family("Menlo")
                .text_size(px(13.))
                .line_height(px(20.))
                .overflow_x_scroll()
                .when(*first, |code| code.pt(px(8.)))
                .when(*last, |code| code.pb(px(8.)).mb(px(8.)))
                .when(
                    *first && language.as_deref() == Some("HTML (not executed)"),
                    |code| code.child("HTML (not executed)"),
                )
                .child(self.markdown_code_text(row_index, body[range.clone()].trim_end_matches(['\r', '\n'])))
                .into_any_element(),
            RowContent::Mermaid { body } => match self.markdown_preview.diagrams.get(body) {
                Some(Diagram::Image {
                    image,
                    width,
                    height,
                    ..
                }) => {
                    let indent = (row.list_depth.saturating_add(row.quote_depth))
                        .saturating_mul(12)
                        .min(160);
                    let available = (f32::from(window.viewport_size().width)
                        - 46.
                        - indent as f32
                        - if row.marker.is_some() { 34. } else { 0. }
                        - if row.quote_depth != 0 { 4. } else { 0. })
                    .max(1.);
                    let displayed = available.min(*width);
                    div()
                        .id("markdown-mermaid")
                        .debug_selector(|| "markdown-mermaid".into())
                        .role(Role::Image)
                        .aria_label("Mermaid diagram")
                        .aria_description("Mermaid 12.3.0")
                        .w_full()
                        .flex()
                        .justify_center()
                        .mb(px(8.))
                        .child(
                            gpui::img(image.clone())
                                .w(px(displayed))
                                .h(px(displayed * height / width))
                                .object_fit(gpui::ObjectFit::Contain),
                        )
                        .into_any_element()
                }
                Some(Diagram::Error(error)) => div()
                    .id("markdown-mermaid-error")
                    .role(Role::Alert)
                    .aria_label(format!("Mermaid: {error}"))
                    .border_1()
                    .border_color(rgb(0xcc6666))
                    .p(px(8.))
                    .mb(px(8.))
                    .child(format!("Mermaid: {error}"))
                    .into_any_element(),
                None => div()
                    .h(px(48.))
                    .child("Rendering Mermaid…")
                    .into_any_element(),
            },
            RowContent::Table {
                cells,
                alignments,
                heading,
            } => {
                let Some((_, table)) = self
                    .markdown_preview
                    .tables
                    .range(..=row_index)
                    .next_back()
                    .filter(|(_, table)| table.rows.contains(&row_index))
                else {
                    return div().into_any_element();
                };
                let Some(document) = &self.markdown_preview.document else {
                    return div().into_any_element();
                };
                let marker = document.rows[table.rows.start].marker.is_some();
                let continuation_indent = if marker && row.marker.is_none() {
                    34.
                } else {
                    0.
                };
                let indent = (row.list_depth.saturating_add(row.quote_depth))
                    .saturating_mul(12)
                    .min(160);
                let available = (f32::from(window.viewport_size().width)
                    - 46.
                    - indent as f32
                    - if marker { 34. } else { 0. }
                    - if row.quote_depth != 0 { 4. } else { 0. })
                .max(1.);
                let widths = table.widths(document, available, window);
                let content_width = widths.iter().sum::<f32>() + 2.;
                let border = rgb(if self.dark { 0x484848 } else { 0xd0d0d0 });
                let cells = cells
                    .iter()
                    .enumerate()
                    .map(|(index, inline)| {
                        let mut cell = div()
                            .id(("markdown-table-cell", index))
                            .w(px(widths.get(index).copied().unwrap_or(21.)))
                            .min_w(px(0.))
                            .flex_shrink_0()
                            .px(px(10.))
                            .py(px(6.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .when(index + 1 < widths.len(), |cell| {
                                cell.border_r_1().border_color(border)
                            })
                            .child(self.markdown_inline_block(inline, index, row_index, *heading, owner, cx));
                        if let Some(alignment) = alignments.get(index) {
                            cell = match alignment {
                                Alignment::Left => cell.text_left(),
                                Alignment::Center => cell.text_center(),
                                Alignment::Right => cell.text_right(),
                                _ if *heading => cell.text_center(),
                                _ => cell,
                            };
                        }
                        cell.into_any_element()
                    })
                    .collect::<Vec<_>>();
                let row = div()
                    .id("markdown-table-row")
                    .debug_selector(|| "markdown-table".into())
                    .flex()
                    .w(px(content_width))
                    .flex_shrink_0()
                    .text_size(px(15.))
                    .line_height(px(24.75))
                    .when(*heading, |row| {
                        row.bg(rgb(if self.dark { 0x303030 } else { 0xeeeeee }))
                    })
                    .border_l_1()
                    .border_r_1()
                    .border_b_1()
                    .when(row_index == table.rows.start, |row| row.border_t_1())
                    .border_color(border)
                    .children(cells);
                let viewport = div()
                    .id("markdown-table-viewport")
                    .w_full()
                    .min_w(px(0.))
                    .overflow_x_scroll()
                    .track_scroll(&table.scroll)
                    .child(row);
                div()
                    .w_full()
                    .min_w(px(0.))
                    .pl(px(continuation_indent))
                    .child(viewport)
                    .when(
                        row_index + 1 == table.rows.end && content_width > available + 0.01,
                        |block| block.child(self.markdown_table_scrollbar(table, cx)),
                    )
                    .into_any_element()
            }
            RowContent::Rule => div()
                .h(px(1.))
                .bg(rgb(0x808080))
                .my(px(12.))
                .into_any_element(),
        };
        let indent = row
            .list_depth
            .saturating_mul(12)
            .saturating_add(row.quote_depth.saturating_mul(12))
            .min(160);
        div()
            .id("markdown-row-decoration")
            .w_full()
            .min_w(px(0.))
            .pl(px(indent as f32))
            .aria_description(format!(
                "quote nesting {}, list nesting {}",
                row.quote_depth, row.list_depth
            ))
            .when(row.quote_depth != 0, |row| {
                row.debug_selector(|| "markdown-quote".into())
                    .border_l_4()
                    .border_color(rgb(0x808080))
            })
            .when(row.list_depth != 0, |row| {
                row.debug_selector(|| "markdown-list".into())
            })
            .flex()
            .items_start()
            .gap(px(6.))
            .when_some(row.marker.clone(), |row, marker| {
                row.child(div().w(px(28.)).flex_shrink_0().child(marker))
            })
            .child(div().flex_1().min_w(px(0.)).child(content))
            .into_any_element()
    }

    pub(super) fn render_markdown_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.markdown_preview.visible {
            return None;
        }
        self.markdown_preview.selection.geometry.borrow_mut().clear();
        let filename = self.native_locale();
        let close_label = crate::localization::preference_control(&filename, "", 6001).unwrap_or("Close");
        let palette = self.preferences_render_palette();
        let mut content = div()
            .id("markdown-preview")
            .debug_selector(|| "markdown-preview".into())
            .role(Role::Dialog)
            .aria_label("Markdown Preview (read-only)")
            .track_focus(&self.close_focus)
            .key_context("MatrixMarkdown")
            .on_action(cx.listener(|view, _: &CopyMarkdownPreview, _, cx| view.copy_markdown_preview(cx)))
            .on_action(cx.listener(|view, _: &SelectAllMarkdownPreview, _, cx| view.select_all_markdown_preview(cx)))
            .on_action(cx.listener(|view, _: &CloseMarkdownPreview, window, cx| {
                if view.markdown_preview.context_menu.is_some() {
                    view.markdown_menu_input(MenuEvent::Dismiss, cx);
                } else {
                    view.dismiss_floating(floating_windows::DialogKind::MarkdownPreview, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                if view.markdown_preview.context_menu.is_none() { return; }
                let event = match event.keystroke.key.as_str() {
                    "up" => MenuEvent::Up,
                    "down" => MenuEvent::Down,
                    "left" => MenuEvent::Left,
                    "right" => MenuEvent::Right,
                    "home" => MenuEvent::First,
                    "end" => MenuEvent::Last,
                    "enter" | "space" => MenuEvent::Activate,
                    "escape" => MenuEvent::Dismiss,
                    _ => return,
                };
                view.markdown_menu_input(event, cx);
                cx.stop_propagation();
            }))
            .size_full()
            .overflow_hidden()
            .p(px(12.))
            .flex()
            .flex_col()
            .on_mouse_move(
                cx.listener(|view, event, window, cx| {
                    view.markdown_scrollbar_move(event, cx);
                    view.markdown_selection_move(event, cx);
                    view.markdown_selection_scroll(window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event, _, cx| {
                    view.markdown_selection_up(event, true, cx);
                    view.markdown_scrollbar_up(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, event, _, cx| {
                    view.markdown_selection_up(event, false, cx);
                    view.markdown_scrollbar_up(cx);
                }),
            )
            .bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
            .text_color(rgb(if self.dark { 0xe6e6e6 } else { 0x202020 }))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .font_family(dialog_font::font_family())
                    .text_size(dialog_font::px(13.))
                    .mb(dialog_font::px(10.))
                    .child(format!(
                        "{} · Markdown Preview (read-only)",
                        self.workspace.active().name()
                    ))
                    .child(
                        div()
                            .id("markdown-preview-close")
                            .accessibility_id("markdown-preview-close")
                            .debug_selector(|| "markdown-preview-close".into())
                            .role(Role::Button)
                            .aria_label(close_label)
                            .border_1()
                            .border_color(rgb(palette.control_edge(true)))
                            .bg(rgb(palette.button_background(true)))
                            .text_color(rgb(palette.darker_text))
                            .h(dialog_font::control_height())
                            .px(dialog_font::px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .line_height(dialog_font::line_height())
                            .text_size(px(dialog_font::base_size()))
                            .focusable()
                            .tab_index(0)
                            .cursor_pointer()
                            .hover(move |style| style.bg(rgb(palette.hot_background)).border_color(rgb(palette.hot_edge)).text_color(rgb(palette.text)))
                            .focus(move |style| style.border_color(rgb(palette.hot_edge)))
                            .child(close_label)
                            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    view.dismiss_floating(floating_windows::DialogKind::MarkdownPreview, window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.dismiss_floating(
                                    floating_windows::DialogKind::MarkdownPreview,
                                    window,
                                    cx,
                                )
                            })),
                    ),
            );
        if self.markdown_preview.completed != self.markdown_preview.requested {
            content = content.child("Updating preview…");
        }
        if self.markdown_preview.document.is_some() {
            content = content.child(
                div()
                    .id("markdown-content")
                    .on_mouse_down(MouseButton::Left, cx.listener(|view, event, window, cx| view.markdown_selection_down(event, window, cx)))
                    .on_mouse_down(MouseButton::Right, cx.listener(|view, event, window, cx| view.markdown_menu_down(event, window, cx)))
                    .debug_selector(|| "markdown-content".into())
                    .font_family("Helvetica")
                    .text_size(px(13.))
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_hidden()
                    .flex()
                    .child(
                        gpui::list(
                            self.markdown_preview.list.clone(),
                            cx.processor(|view, index: usize, window, cx| {
                                let Some(document) = view.markdown_preview.document.clone() else {
                                    return div().into_any_element();
                                };
                                let Some(owner) = view.markdown_preview.completed.clone() else {
                                    return div().into_any_element();
                                };
                                let Some(row) = document.rows.get(index) else {
                                    return div().into_any_element();
                                };
                                let table = matches!(row.content, RowContent::Table { .. })
                                    .then(|| {
                                        view.markdown_preview
                                            .tables
                                            .range(..=index)
                                            .next_back()
                                            .map(|(_, table)| table)
                                    })
                                    .flatten();
                                let (top, bottom) = match &row.content {
                                    RowContent::Code { first, last, .. } => {
                                        (if *first { 4. } else { 0. }, if *last { 4. } else { 0. })
                                    }
                                    _ => (
                                        table.map_or(4., |table| {
                                            if table.rows.start == index { 12. } else { 0. }
                                        }),
                                        table.map_or(4., |table| {
                                            if table.rows.end == index + 1 { 12. } else { 0. }
                                        }),
                                    ),
                                };
                                div()
                                    .id(("markdown-block", index))
                                    .px(px(4.))
                                    .pt(px(top))
                                    .pb(px(bottom))
                                    .w_full()
                                    .min_w(px(0.))
                                    .child(view.markdown_row(index, row, &owner, window, cx))
                                    .into_any_element()
                            }),
                        )
                        .flex_1()
                        .min_h(px(0.))
                        .min_w(px(0.)),
                    )
                    .child(self.markdown_scrollbar(cx)),
            );
        } else {
            content = content.child("Rendering Markdown…");
        }
        if let Some(menu) = self.render_markdown_menu(cx) {
            content = content.child(menu);
        }
        Some(content.into_any_element())
    }
}
