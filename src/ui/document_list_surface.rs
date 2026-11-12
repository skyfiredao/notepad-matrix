use super::*;

impl MatrixWindow {
    fn document_list_entries(&self) -> Vec<(crate::workspace::ViewId, crate::workspace::BufferId)> {
        [
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
        .collect()
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
        let background = if self.dark { 0x282828 } else { 0xf6f6f6 };
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
                .bg(rgb(background))
                .border_r_1()
                .border_color(rgb(0x808080))
                .child(
                    div()
                        .h(px(26.))
                        .flex_shrink_0()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("Document List")
                        .child(
                            div()
                                .id("document-list-close")
                                .role(Role::Button)
                                .aria_label("Close Document List")
                                .tooltip(move |_, cx| {
                                    icon_tooltip::view("Close Document List", dark, cx)
                                })
                                .child("×")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.toggle_document_list(window, cx)
                                })),
                        ),
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
                                                .h(px(24.))
                                                .px(px(6.))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .cursor_pointer()
                                                .when(active, |row| row.bg(rgb(selected)))
                                                .child(format!(
                                                    "{}{}",
                                                    if buffer.is_modified() { "*" } else { "" },
                                                    name
                                                ))
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

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn document_list_targets_each_cloned_view_membership(cx: &mut gpui::TestAppContext) {
        use crate::workspace::ViewId;
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        cx.simulate_input("A");
        cx.dispatch_action(EditorDocumentStart);
        view.update_in(cx, |view, window, cx| view.clone_to_other_view(window, cx));
        cx.dispatch_action(EditorDocumentEnd);
        view.update_in(cx, |view, window, cx| view.toggle_document_list(window, cx));
        let primary = cx
            .debug_bounds("document-list-item-1")
            .expect("primary membership")
            .center();
        let secondary = cx
            .debug_bounds("document-list-item-1-secondary")
            .expect("secondary membership")
            .center();
        cx.simulate_click(primary, gpui::Modifiers::default());
        assert_eq!(
            view.read_with(cx, |view, _| (
                view.workspace.active_view(),
                view.editor().selection().head
            )),
            (ViewId::Primary, ByteOffset(0))
        );
        cx.simulate_click(secondary, gpui::Modifiers::default());
        assert_eq!(
            view.read_with(cx, |view, _| (
                view.workspace.active_view(),
                view.editor().selection().head
            )),
            (ViewId::Secondary, ByteOffset(1))
        );
    }
}
