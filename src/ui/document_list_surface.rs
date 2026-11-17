use super::*;
use super::dialog_font::px;
use super::toolbar_icons::ToolbarIcon;

impl MatrixWindow {
    fn document_list_entries(&self) -> Vec<(crate::workspace::ViewId, crate::workspace::BufferId)> {
        let mut entries: Vec<_> = [
            crate::workspace::ViewId::Primary,
            crate::workspace::ViewId::Secondary,
        ]
        .into_iter()
        .flat_map(|pane| {
            self.workspace
                .view_buffer_ids(pane)
                .iter()
                .copied()
                .map(move |id| (pane, id))
        })
        .collect();
        if self.panels.document_list_sort != 0 {
            entries.sort_by(|(_, left), (_, right)| {
                let name = |id| self.workspace.buffers().iter().find(|buffer| buffer.id() == id).map(|buffer| buffer.name()).unwrap_or_default();
                let order = name(*left).cmp(&name(*right));
                if self.panels.document_list_sort == 1 { order } else { order.reverse() }
            });
        }
        entries
    }

    pub(super) fn document_list_active_index(&self) -> usize {
        self.document_list_entries()
            .iter()
            .position(|(pane, id)| {
                *pane == self.workspace.active_view() && *id == self.workspace.active().id()
            })
            .unwrap_or(0)
    }

    pub(super) fn toggle_document_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.toggle_document_list(window, cx)
        }) {
            return;
        }
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.document_list_visible = !self.document_list_visible;
        if self.document_list_visible {
            self.show_panel(crate::panel_layout::PanelId::DocumentList, window, cx);
            self.document_list_scroll.scroll_to_item(
                self.document_list_active_index(),
                gpui::ScrollStrategy::Nearest,
            );
        } else {
            self.hide_panel(crate::panel_layout::PanelId::DocumentList, window, cx);
        }
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn render_document_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.document_list_visible {
            return None;
        }
        let background = if self.dark { 0x282828 } else { 0xffffff };
        let selected = if self.dark { 0x404b5a } else { 0xcce8ff };
        let dark = self.dark;
        Some(
            div()
                .id("document-list-panel")
                .debug_selector(|| "document-list-panel".into())
                .role(Role::Group)
                .aria_label("Document List")
                .w_full()
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .font_family(dialog_font::font_family())
                .text_size(px(12.))
                .bg(rgb(background))
                .border_r_1()
                .border_color(rgb(0x808080))
                .child(
                    div()
                        .id("document-list-filename")
                        .role(Role::ColumnHeader)
                        .aria_label("Filename")
                        .h(px(24.))
                        .flex_shrink_0()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1().border_color(rgb(if dark { 0x555555 } else { 0xd4d0c8 }))
                        .child("文件名")
                        .child(match self.panels.document_list_sort { 1 => "⌃", 2 => "⌄", _ => "" })
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.panels.document_list_sort = (view.panels.document_list_sort + 1) % 3;
                            view.document_list_scroll.scroll_to_item(view.document_list_active_index(), gpui::ScrollStrategy::Nearest);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("document-list-options")
                        .role(Role::ListBox)
                        .aria_label("Open documents")
                        .flex_1()
                        .min_h(px(0.))
                        .child(
                            gpui::uniform_list(
                                "document-list-items",
                                self.document_list_entries().len(),
                                cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                                    let entries = view.document_list_entries();
                                    range
                                        .filter_map(|index| {
                                            entries
                                                .get(index)
                                                .copied()
                                                .map(|(pane, id)| (index, pane, id))
                                        })
                                        .filter_map(|(index, pane, id)| {
                                            view.workspace
                                                .buffers()
                                                .iter()
                                                .find(|buffer| buffer.id() == id)
                                                .map(|buffer| (index, pane, buffer))
                                        })
                                        .map(|(index, pane, buffer)| {
                                            let id = buffer.id();
                                            let active = id == view.workspace.active().id()
                                                && pane == view.workspace.active_view();
                                             let name = buffer.name();
                                             let (icon, dark_icon) = if buffer.is_monitoring() {
                                                 (ToolbarIcon::MonitoringDocument, ToolbarIcon::MonitoringDocumentDark)
                                             } else if buffer.disk_file.as_ref().is_some_and(crate::files::DiskFile::is_read_only) {
                                                 (ToolbarIcon::SystemReadOnlyDocument, ToolbarIcon::SystemReadOnlyDocumentDark)
                                             } else if buffer.is_user_read_only() {
                                                 (ToolbarIcon::ReadOnlyDocument, ToolbarIcon::ReadOnlyDocumentDark)
                                             } else if buffer.is_modified() {
                                                 (ToolbarIcon::ModifiedDocument, ToolbarIcon::ModifiedDocumentDark)
                                             } else { (ToolbarIcon::SavedDocument, ToolbarIcon::SavedDocumentDark) };
                                             div()
                                                .id(("document-list-item", index))
                                                .debug_selector(move || {
                                                    if pane == crate::workspace::ViewId::Primary {
                                                        format!("document-list-item-{}", id.0)
                                                    } else {
                                                        format!(
                                                            "document-list-item-{}-secondary",
                                                            id.0
                                                        )
                                                    }
                                                })
                                                .role(Role::ListBoxOption)
                                                .aria_label(name.clone())
                                                .aria_selected(active)
                                                .relative()
                                                .h(px(24.))
                                                .px(px(3.))
                                                .flex().items_center().gap(px(2.))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .cursor_pointer()
                                                .when(active, |row| row.bg(rgb(selected)))
                                                .child(gpui::img(if dark { dark_icon } else { icon }.tab_image(false)).size(px(16.)).flex_shrink_0())
                                                .child(name)
                                                .when(active, |row| row.child(gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
                                                    let step = px(2.);
                                                    let count_x = (f32::from(bounds.size.width) / f32::from(step)).ceil() as usize;
                                                    let count_y = (f32::from(bounds.size.height) / f32::from(step)).ceil() as usize;
                                                    for index in 0..count_x {
                                                        for y in [bounds.top(), bounds.bottom() - px(1.)] {
                                                            window.paint_quad(gpui::fill(Bounds::new(gpui::point(bounds.left() + step * index as f32, y), size(px(1.), px(1.))), rgb(0x606060)));
                                                        }
                                                    }
                                                    for index in 0..count_y {
                                                        for x in [bounds.left(), bounds.right() - px(1.)] {
                                                            window.paint_quad(gpui::fill(Bounds::new(gpui::point(x, bounds.top() + step * index as f32), size(px(1.), px(1.))), rgb(0x606060)));
                                                        }
                                                    }
                                                }).absolute().size_full()))
                                                .on_click(cx.listener(
                                                    move |view, _, window, cx| {
                                                        if !window.modifiers().control
                                                            && !window.modifiers().shift
                                                        {
                                                            view.activate_document_list_entry(
                                                                pane, id, window, cx,
                                                            );
                                                        }
                                                        cx.stop_propagation();
                                                    },
                                                ))
                                        })
                                        .collect()
                                }),
                            )
                            .track_scroll(&self.document_list_scroll)
                            .h_full(),
                        ),
                )
                .into_any_element(),
        )
    }

    fn activate_document_list_entry(
        &mut self,
        pane: crate::workspace::ViewId,
        id: crate::workspace::BufferId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.activate_document_list_entry(pane, id, window, cx)
        }) {
            return;
        }
        if self.workspace.view_buffer_ids(pane).contains(&id)
            && self.focus_editor_pane(pane, window, cx)
        {
            self.activate_buffer(id, window, cx);
            window.activate_window();
        }
    }
}
