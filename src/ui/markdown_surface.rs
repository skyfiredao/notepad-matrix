use super::*;
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

struct TableLayout {
    rows: std::ops::Range<usize>,
    columns: std::cell::OnceCell<Vec<(f32, f32)>>,
    scroll: gpui::ScrollHandle,
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
    scroll_hint_width: std::rc::Rc<std::cell::Cell<Option<gpui::Pixels>>>,
    tables: std::collections::BTreeMap<usize, TableLayout>,
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
            scroll_hint_width: Default::default(),
            tables: Default::default(),
        }
    }
}

impl MatrixWindow {
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
        self.markdown_preview.scroll_grab = None;
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
                // Height estimates can grow as off-screen diagrams are measured.
                // The track endpoint must still reach the actual final row.
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
        if self.markdown_preview.scroll_grab.take().is_some() {
            self.markdown_preview.list.scrollbar_drag_ended();
            cx.notify();
        }
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
                            // GPUI invalidates all height hints during a width change.
                            // Seed them after layout, without measuring off-screen rows.
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
        bold: bool,
        owner: &Owner,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let foreground = rgb(if self.dark { 0xe6e6e6 } else { 0x202020 });
        let mut links = Vec::new();
        let mut targets = Vec::new();
        let runs = inline
            .spans
            .iter()
            .map(|span| {
                let font = inline_font(&span.style, bold);
                if let Some(target) = &span.style.link {
                    links.push(span.range.clone());
                    targets.push(target.clone());
                }
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
        let entity = cx.weak_entity();
        let owner = owner.clone();
        gpui::InteractiveText::new(
            ("markdown-inline", id),
            gpui::StyledText::new(gpui::SharedString::from(inline.text.clone())).with_runs(runs),
        )
        .on_click(links, move |index, _, cx| {
            if let Some(target) = targets.get(index)
                && let Err(error) =
                    entity.update(cx, |view, cx| view.open_markdown_link(target, &owner, cx))
            {
                eprintln!("Markdown link lost its document: {error}");
            }
        })
        .into_any_element()
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
            // Keep the loaded image inside the same viewport measured by the list.
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
                cx.listener(move |view, _, _, cx| view.open_markdown_link(&target, &owner, cx)),
            );
        }
        image.into_any_element()
    }

    fn markdown_inline_block(
        &self,
        inline: &PreparedInline,
        id: usize,
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
                block.child(self.markdown_inline(inline, 0, bold, owner, cx))
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
                    .child(self.markdown_inline_block(inline, 0, heading.is_some(), owner, cx))
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
                .child(
                    body[range.clone()]
                        .trim_end_matches(['\r', '\n'])
                        .to_owned(),
                )
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
                            .child(self.markdown_inline_block(inline, index, *heading, owner, cx));
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
                    .w(px(widths.iter().sum::<f32>() + 2.))
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
        let mut content = div()
            .id("markdown-preview")
            .debug_selector(|| "markdown-preview".into())
            .role(Role::Dialog)
            .aria_label("Markdown Preview (read-only)")
            .track_focus(&self.close_focus)
            .key_context("MatrixMarkdown")
            .size_full()
            .overflow_hidden()
            .p(px(12.))
            .flex()
            .flex_col()
            .on_mouse_move(
                cx.listener(|view, event, _, cx| view.markdown_scrollbar_move(event, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.markdown_scrollbar_up(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.markdown_scrollbar_up(cx)),
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
                            .aria_label("Close")
                            .border_1()
                            .border_color(rgb(0xaaaaaa))
                            .bg(rgb(0xf0f0f0))
                            .text_color(rgb(0x202020))
                            .px(dialog_font::px(14.))
                            .py(dialog_font::px(5.))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0xe5f1fb)).border_color(rgb(0x0078d7)))
                            .active(|style| style.bg(rgb(0xcce4f7)))
                            .child("Close")
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
        Some(content.into_any_element())
    }
}
