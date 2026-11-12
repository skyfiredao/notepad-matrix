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
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn native_prepared_markdown_handles_deep_quotes_and_linked_container_images(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("linked.md");
        let source = format!(
            "{}x\n\n- [![alt](image.png)](https://example.com)\n\n| image |\n| --- |\n| ![table](image.png) |\n",
            "> ".repeat(10_000)
        );
        std::fs::write(&path, &source).unwrap();
        std::fs::write(
            home.path().join("image.png"),
            include_bytes!("../../assets/notepad-plus-plus/saved.png"),
        )
        .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            let (file, text) = crate::files::DiskFile::load_unicode(&path).unwrap();
            view.workspace.open_buffer(file, &text).unwrap();
            view.editor_changed(Ok(()), window, cx);
            view.toggle_markdown_preview(window, cx);
        });
        if let Some(task) = view.update(cx, |view, _| view.markdown_preview.task.take()) {
            cx.foreground_executor().clone().block_test(task);
        }
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::MarkdownPreview]
        });
        let mut preview = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        assert!(preview.debug_bounds("markdown-quote").is_some());
        let image = preview.debug_bounds("markdown-linked-image").unwrap();
        preview.simulate_click(image.center(), gpui::Modifiers::default());
        assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/"));
        view.update(cx, |view, cx| {
            view.markdown_preview.list.scroll_to_reveal_item(3);
            cx.notify();
        });
        preview.debug_bounds("markdown-content").unwrap();
        assert!(preview.debug_bounds("markdown-image").is_some());
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), source);
            assert!(!view.editor().document().can_undo());
        });
    }

    #[gpui::test]
    fn native_markdown_preview_renders_updates_scrolls_and_closes_without_editing_source(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("isolated Markdown document");
        let path = home.path().join("preview.md");
        let source = "# 标题\n\n**bold** and *italic* [top](#标题)\n\n```rust\nlet x = 1;\n```\n\n- first\n- second\n\n| left | right |\n| --- | --- |\n| a | b |\n\n![local](image.png)\n";
        std::fs::write(&path, source).unwrap();
        std::fs::write(
            home.path().join("image.png"),
            include_bytes!("../../assets/notepad-plus-plus/saved.png"),
        )
        .unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            let (file, text) = crate::files::DiskFile::load_unicode(&path).unwrap();
            view.workspace.open_buffer(file, &text).unwrap();
            view.editor_mut()
                .select(Selection {
                    anchor: ByteOffset(0),
                    head: ByteOffset(1),
                })
                .unwrap();
            view.editor_changed(Ok(()), window, cx);
        });
        let initial = view.read_with(cx, |view, _| {
            (view.editor().display_revision(), view.editor().selection())
        });
        cx.dispatch_action(OpenViewMenu);
        let menu = cx
            .debug_bounds("view-markdown-preview")
            .expect("View menu preview entry");
        cx.simulate_click(menu.center(), gpui::Modifiers::default());
        if let Some(task) = view.update(cx, |view, _| view.markdown_preview.task.take()) {
            cx.foreground_executor().clone().block_test(task);
        }
        cx.run_until_parked();
        let handle = view.read_with(cx, |view, _| {
            view.floating_windows[&floating_windows::DialogKind::MarkdownPreview]
        });
        let mut preview = gpui::VisualTestContext::from_window(handle.into(), &cx.cx);
        assert!(preview.debug_bounds("markdown-preview").is_some());
        assert!(preview.debug_bounds("markdown-heading").is_some());
        assert!(preview.debug_bounds("markdown-code-block").is_some());
        assert!(preview.debug_bounds("markdown-list").is_some());
        assert!(preview.debug_bounds("markdown-table").is_some());
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), source);
            assert_eq!(
                (view.editor().display_revision(), view.editor().selection()),
                initial
            );
            assert!(!view.editor().document().can_undo());
        });
        let content = preview.debug_bounds("markdown-content").unwrap();
        preview.simulate_event(gpui::ScrollEvent {
            position: content.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-160.))),
            ..Default::default()
        });
        assert!(preview.debug_bounds("markdown-image").is_some());
        view.update_in(cx, |view, window, cx| {
            view.editor_mut()
                .move_to_document_edge(true, false)
                .unwrap();
            view.editor_mut()
                .insert_text("\n\n## 更新后的标题\n")
                .unwrap();
            view.editor_changed(Ok(()), window, cx);
            view.refresh_markdown_preview(cx);
        });
        if let Some(task) = view.update(cx, |view, _| view.markdown_preview.task.take()) {
            cx.foreground_executor().clone().block_test(task);
        }
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.markdown_preview
                    .document
                    .as_ref()
                    .unwrap()
                    .rows
                    .iter()
                    .any(|row| matches!(&row.content, RowContent::Text { inline, heading: Some(2) } if inline.text.as_ref() == "更新后的标题"))
            )
        });
        let expected = view.read_with(cx, |view, _| view.editor().selection());
        let close = preview.debug_bounds("markdown-preview-close").unwrap();
        preview.simulate_click(close.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor().selection(), expected);
            assert!(
                view.editor()
                    .committed_text()
                    .ends_with("## 更新后的标题\n")
            );
            assert!(!view.markdown_preview.visible);
        });
    }
}

pub(super) struct State {
    pub(super) visible: bool,
    requested: Option<Owner>,
    completed: Option<Owner>,
    document: Option<Arc<RenderDocument>>,
    task: Option<gpui::Task<()>>,
    cancel: Arc<AtomicBool>,
    list: gpui::ListState,
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
                view.markdown_preview.list.reset(document.rows.len());
                if same_buffer {
                    view.markdown_preview.list.scroll_to(gpui::ListOffset {
                        item_ix: matched
                            .unwrap_or(offset.item_ix)
                            .min(document.rows.len().saturating_sub(1)),
                        offset_in_item: offset.offset_in_item,
                    });
                }
                view.markdown_preview.document = Some(Arc::new(document));
                view.markdown_preview.completed = Some(owner);
                view.markdown_preview.task = None;
                cx.notify();
            }) {
                eprintln!("Markdown preview result lost its document: {error}");
            }
        }));
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
                let mut font = gpui::font(if span.style.code {
                    "Menlo"
                } else {
                    "Helvetica"
                });
                if span.style.bold || bold {
                    font.weight = gpui::FontWeight::BOLD;
                }
                if span.style.italic {
                    font.style = gpui::FontStyle::Italic;
                }
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
            image = image.child(
                gpui::img(path)
                    .w_full()
                    .h(px(240.))
                    .object_fit(gpui::ObjectFit::Contain),
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

    fn markdown_row(&self, row: &RenderRow, owner: &Owner, cx: &mut Context<Self>) -> AnyElement {
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
                .when(*first && language.is_some(), |code| {
                    code.child(
                        language
                            .as_ref()
                            .map(|language| language.to_string())
                            .unwrap_or_default(),
                    )
                })
                .child(
                    body[range.clone()]
                        .trim_end_matches(['\r', '\n'])
                        .to_owned(),
                )
                .into_any_element(),
            RowContent::Table {
                cells,
                alignments,
                heading,
            } => {
                let cells = cells
                    .iter()
                    .enumerate()
                    .map(|(index, inline)| {
                        let mut cell = div()
                            .id(("markdown-table-cell", index))
                            .min_w(px(120.))
                            .flex_1()
                            .p(px(6.))
                            .border_r_1()
                            .border_color(rgb(0x909090))
                            .child(self.markdown_inline_block(inline, index, *heading, owner, cx));
                        if let Some(alignment) = alignments.get(index) {
                            cell = match alignment {
                                Alignment::Center => cell.text_center(),
                                Alignment::Right => cell.text_right(),
                                _ => cell,
                            };
                        }
                        cell.into_any_element()
                    })
                    .collect::<Vec<_>>();
                div()
                    .id("markdown-table-row")
                    .debug_selector(|| "markdown-table".into())
                    .flex()
                    .w_full()
                    .min_w(px(0.))
                    .border_1()
                    .border_color(rgb(0x909090))
                    .overflow_x_scroll()
                    .when(*heading, |table| {
                        table.bg(rgb(if self.dark { 0x303030 } else { 0xeeeeee }))
                    })
                    .children(cells)
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
            .bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
            .text_color(rgb(if self.dark { 0xe6e6e6 } else { 0x202020 }))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .mb(px(10.))
                    .child(format!(
                        "{} · Markdown Preview (read-only)",
                        self.workspace.active().name()
                    ))
                    .child(
                        div()
                            .id("markdown-preview-close")
                            .debug_selector(|| "markdown-preview-close".into())
                            .role(Role::Button)
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
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        gpui::list(
                            self.markdown_preview.list.clone(),
                            cx.processor(|view, index: usize, _, cx| {
                                let Some(document) = view.markdown_preview.document.clone() else {
                                    return div().into_any_element();
                                };
                                let Some(owner) = view.markdown_preview.completed.clone() else {
                                    return div().into_any_element();
                                };
                                let Some(row) = document.rows.get(index) else {
                                    return div().into_any_element();
                                };
                                div()
                                    .id(("markdown-block", index))
                                    .p(px(4.))
                                    .w_full()
                                    .min_w(px(0.))
                                    .child(view.markdown_row(row, &owner, cx))
                                    .into_any_element()
                            }),
                        )
                        .flex_1()
                        .min_h(px(0.))
                        .w_full(),
                    ),
            );
        } else {
            content = content.child("Rendering Markdown…");
        }
        Some(content.into_any_element())
    }
}
