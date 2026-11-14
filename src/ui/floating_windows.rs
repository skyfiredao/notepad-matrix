use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DialogKind {
    Find,
    FindInResults,
    FindInResultsProgress,
    GoTo,
    Preferences,
    About,
    MarkdownPreview,
    MacroSave,
    MacroRepeat,
    Udl,
    UdlStyler,
    ColumnEditor,
    DocumentSwitcher,
    Completion,
    Calltip,
    BackupFailure,
    Run,
    Close,
    SaveAll,
    Reload,
}

impl DialogKind {
    const ALL: [Self; 20] = [
        Self::Find,
        Self::FindInResults,
        Self::FindInResultsProgress,
        Self::GoTo,
        Self::Preferences,
        Self::About,
        Self::MarkdownPreview,
        Self::MacroSave,
        Self::MacroRepeat,
        Self::Udl,
        Self::UdlStyler,
        Self::ColumnEditor,
        Self::DocumentSwitcher,
        Self::Completion,
        Self::Calltip,
        Self::BackupFailure,
        Self::Run,
        Self::Close,
        Self::SaveAll,
        Self::Reload,
    ];

    fn visible(self, owner: &MatrixWindow) -> bool {
        match self {
            Self::Find => owner.find_visible || owner.files_search.visible,
            Self::FindInResults => owner.find_in_results_visible(),
            Self::FindInResultsProgress => owner.find_in_results_progress_visible(),
            Self::GoTo => owner.goto_visible,
            Self::Preferences => owner.preferences_visible,
            Self::About => owner.about_visible,
            Self::MarkdownPreview => owner.markdown_preview.visible,
            Self::MacroSave => owner.macro_dialogs.save_visible,
            Self::MacroRepeat => owner.macro_dialogs.repeat_visible,
            Self::Udl => owner.udl.visible,
            Self::UdlStyler => owner.udl.styler_visible,
            Self::ColumnEditor => owner.column_editor.visible,
            Self::DocumentSwitcher => owner.document_switcher_visible(),
            Self::Completion => owner.completion_visible(),
            Self::Calltip => owner.calltip_visible(),
            Self::BackupFailure => owner.backup_failure_visible(),
            Self::Run => owner.run_visible,
            Self::Close => owner.close_dialog,
            Self::SaveAll => owner.save_all_dialog,
            Self::Reload => owner.reload_dialog,
        }
    }

    fn title(self, owner: &MatrixWindow) -> &'static str {
        match self {
            Self::Find if owner.files_search.visible => "Find in Files",
            Self::Find if owner.find_replace_mode => "Replace",
            Self::Find => "Find",
            Self::FindInResults => "Find in search results",
            Self::FindInResultsProgress => "Find In Files progress...",
            Self::GoTo => "Go To...",
            Self::Preferences => "Preferences",
            Self::About => "About Notepad Matrix",
            Self::MarkdownPreview => "Markdown Preview",
            Self::MacroSave => "Save Current Recorded Macro",
            Self::MacroRepeat => "Run a Macro Multiple Times",
            Self::Udl => "User Defined Language",
            Self::UdlStyler => "UDL Styler",
            Self::ColumnEditor => "Column Editor",
            Self::DocumentSwitcher => "Document switcher",
            Self::Completion => "Completion",
            Self::Calltip => "Function parameters",
            Self::BackupFailure => "Backup error",
            Self::Run => "Run",
            Self::Close => "Save changes?",
            Self::SaveAll => "Save all?",
            Self::Reload => "Reload?",
        }
    }

    fn dimensions(self) -> gpui::Size<gpui::Pixels> {
        match self {
            Self::Find => size(px(617.), px(320.)),
            Self::FindInResults => size(px(548.), px(186.)),
            Self::FindInResultsProgress => size(px(565.), px(95.)),
            Self::GoTo => size(px(360.), px(220.)),
            Self::Preferences => size(px(460.), px(560.)),
            Self::About => size(px(500.), px(280.)),
            Self::MarkdownPreview => size(px(700.), px(560.)),
            Self::MacroSave => size(px(420.), px(200.)),
            Self::MacroRepeat => size(px(480.), px(360.)),
            Self::Udl => size(px(780.), px(600.)),
            Self::UdlStyler => size(px(560.), px(500.)),
            Self::ColumnEditor => size(px(520.), px(500.)),
            Self::DocumentSwitcher => size(px(380.), px(320.)),
            Self::Completion => size(px(360.), px(240.)),
            Self::Calltip => size(px(520.), px(160.)),
            Self::BackupFailure => size(px(520.), px(260.)),
            Self::Run => size(px(480.), px(220.)),
            Self::Close | Self::SaveAll | Self::Reload => size(px(480.), px(220.)),
        }
    }

    fn confirmation(self) -> bool {
        matches!(self, Self::Close | Self::SaveAll | Self::Reload)
    }

    fn hide_modeless(self, owner: &mut MatrixWindow, cx: &mut Context<MatrixWindow>) {
        match self {
            Self::FindInResults => owner.dismiss_find_in_results(cx),
            Self::FindInResultsProgress => owner.cancel_find_in_results_progress(cx),
            Self::Find => {
                owner.close_search_history();
                owner.find_visible = false;
                owner.files_search.visible = false;
            }
            Self::GoTo => owner.goto_visible = false,
            Self::Preferences => owner.preferences_visible = false,
            Self::About => owner.about_visible = false,
            Self::MarkdownPreview => owner.hide_markdown_preview(cx),
            Self::MacroSave => owner.macro_dialogs.save_visible = false,
            Self::MacroRepeat => owner.macro_dialogs.repeat_visible = false,
            Self::Udl => {
                owner.udl.visible = false;
                owner.udl.styler_visible = false;
            }
            Self::UdlStyler => owner.udl.styler_visible = false,
            Self::ColumnEditor => owner.column_editor.visible = false,
            Self::DocumentSwitcher => owner.dismiss_document_switcher(cx),
            Self::Completion => owner.dismiss_completion(cx),
            Self::Calltip => owner.dismiss_calltip(cx),
            Self::BackupFailure => owner.dismiss_backup_failure(cx),
            Self::Run => owner.run_visible = false,
            Self::Close | Self::SaveAll | Self::Reload => {}
        }
    }
}

pub(super) struct FloatingWindow {
    owner: gpui::WeakEntity<MatrixWindow>,
    kind: DialogKind,
    _changed: gpui::Subscription,
    _released: gpui::Subscription,
}

impl FloatingWindow {
    fn new(
        owner: gpui::Entity<MatrixWindow>,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.set_window_title(kind.title(owner.read(cx)));
        let changed = cx.observe_in(&owner, window, move |_, owner, window, cx| {
            let view = owner.read(cx);
            if kind.visible(view) {
                window.set_window_title(kind.title(view));
                cx.notify();
            } else {
                window.remove_window();
            }
        });
        let released = cx.observe_release_in(&owner, window, |_, _, window, _| {
            window.remove_window();
        });
        let weak = owner.downgrade();
        let activated_owner = owner.downgrade();
        cx.observe_window_activation(window, move |_: &mut Self, window, cx| {
            if matches!(kind, DialogKind::Find | DialogKind::FindInResults) {
                let active = window.is_window_active();
                let owner = activated_owner.clone();
                cx.defer(move |cx| {
                    if let Err(error) = owner.update(cx, |owner, cx| {
                        if kind == DialogKind::FindInResults {
                            if !active {
                                owner.retire_find_in_results_popup(cx);
                            }
                        } else if active {
                            if !std::mem::take(&mut owner.find_activation_prepared) {
                                owner.activate_find_selection(cx);
                            }
                        } else if owner.close_search_history() {
                            cx.notify();
                        }
                    }) {
                        eprintln!("search activation lost its document: {error}");
                    }
                });
            }
        })
        .detach();
        window.on_window_should_close(cx, move |window, cx| {
            if let Err(error) =
                weak.update(cx, |owner, cx| owner.dismiss_floating(kind, window, cx))
            {
                eprintln!("auxiliary close lost its document: {error}");
                return true;
            }
            false
        });
        Self {
            owner: owner.downgrade(),
            kind,
            _changed: changed,
            _released: released,
        }
    }

    fn with_owner(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut MatrixWindow, &mut Window, &mut Context<MatrixWindow>),
    ) {
        if let Err(error) = self.owner.update(cx, |owner, cx| update(owner, window, cx)) {
            eprintln!("auxiliary window lost its document: {error}");
            window.remove_window();
        }
    }
}

impl Render for FloatingWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        let content = self.owner.update(cx, |owner, cx| match kind {
            DialogKind::Find if owner.files_search.visible => owner.render_find_in_files(cx),
            DialogKind::Find => owner.render_find(cx),
            DialogKind::FindInResults => Some(owner.render_find_in_results(cx)),
            DialogKind::FindInResultsProgress => owner.render_find_in_results_progress(cx),
            DialogKind::GoTo => owner.render_goto(cx),
            DialogKind::Preferences => owner.render_preferences(cx),
            DialogKind::About => owner.render_about(cx),
            DialogKind::MarkdownPreview => owner.render_markdown_preview(cx),
            DialogKind::MacroSave => owner.render_save_macro(cx),
            DialogKind::MacroRepeat => owner.render_repeat_macro(cx),
            DialogKind::Udl => owner.render_udl(window, cx),
            DialogKind::UdlStyler => owner.render_udl_styler(window, cx),
            DialogKind::ColumnEditor => owner.render_column_editor(window, cx),
            DialogKind::DocumentSwitcher => owner.render_document_switcher(cx),
            DialogKind::Completion => owner.render_completion(cx),
            DialogKind::Calltip => owner.render_calltip(cx),
            DialogKind::BackupFailure => owner.render_backup_failure(cx),
            DialogKind::Run => owner.render_run(window, cx),
            DialogKind::Close => owner.render_close_dialog(cx),
            DialogKind::SaveAll => owner.render_save_all_dialog(cx),
            DialogKind::Reload => owner.render_reload_dialog(cx),
        });
        let content = match content {
            Ok(content) => content,
            Err(error) => {
                eprintln!("auxiliary render lost its document: {error}");
                window.remove_window();
                None
            }
        };
        div()
            .id("matrix-auxiliary-window")
            .key_context("MatrixAuxiliary")
            .tab_group()
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0xf0f0f0))
            .text_color(rgb(0x202020))
            .font_family("Helvetica")
            .text_size(px(13.))
            .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| {
                window.focus_next(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                window.focus_prev(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &CloseWindow, window, cx| {
                view.with_owner(window, cx, move |owner, window, cx| {
                    owner.dismiss_floating(kind, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &CloseFind, window, cx| {
                view.with_owner(window, cx, move |owner, window, cx| {
                    owner.dismiss_floating(
                        if kind == DialogKind::FindInResults {
                            kind
                        } else {
                            DialogKind::Find
                        },
                        window,
                        cx,
                    )
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &CloseFindInFiles, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.dismiss_floating(DialogKind::Find, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &CloseGoTo, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.dismiss_floating(DialogKind::GoTo, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &ClosePreferences, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.dismiss_floating(DialogKind::Preferences, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CloseMarkdownPreview, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.dismiss_floating(DialogKind::MarkdownPreview, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CancelRun, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.dismiss_floating(DialogKind::Run, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindNext, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.find_next(false, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &FindDialogNext, window, cx| {
                view.with_owner(window, cx, move |owner, window, cx| {
                    if kind == DialogKind::FindInResults {
                        owner.submit_find_in_results(window, cx);
                    } else {
                        owner.find_next(owner.find_backward, window, cx);
                    }
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &CountFind, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| owner.count_find(window, cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindDialogReverse, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.find_next(!owner.find_backward, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindPrevious, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.find_next(true, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ReplaceOne, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.replace_one(window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ReplaceAll, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.replace_all(window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &FindAllInFiles, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.start_file_search(window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmGoTo, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.confirm_goto(window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ConfirmRun, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.activate_run_control(window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &RunFocusNext, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.move_run_focus(true, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &RunFocusPrevious, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.move_run_focus(false, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &QuitApplication, window, cx| {
                view.with_owner(window, cx, |owner, _, cx| {
                    owner.dispatch_to_document(QuitApplication, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowFind, window, cx| {
                view.with_owner(window, cx, |owner, _, cx| {
                    owner.dispatch_to_document(ShowFind, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowReplace, window, cx| {
                view.with_owner(window, cx, |owner, _, cx| {
                    owner.dispatch_to_document(ShowReplace, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, _: &ShowFindInFiles, window, cx| {
                view.with_owner(window, cx, |owner, _, cx| {
                    owner.dispatch_to_document(ShowFindInFiles, cx)
                });
                cx.stop_propagation();
            }))
            .children(content)
    }
}

impl MatrixWindow {
    pub(super) fn route_to_document(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        if Window::window_handle(window) == self.document_window {
            return false;
        }
        let root = self.document_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            if !cx.windows().contains(&root) {
                return;
            }
            match root.update(cx, |_, window, cx| {
                owner.update(cx, |view, cx| operation(view, window, cx))
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("auxiliary command could not reach its document: {error}")
                }
            }
        });
        true
    }

    fn dispatch_to_document<A: gpui::Action>(&self, action: A, cx: &mut Context<Self>) {
        let root = self.document_window;
        cx.defer(move |cx| {
            if !cx.windows().contains(&root) {
                return;
            }
            if let Err(error) = root.update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(action), cx)
            }) {
                eprintln!("auxiliary action could not reach its document: {error}");
            }
        });
    }

    fn restore_document_focus(&self, cx: &mut Context<Self>) {
        let focus = self.editor_focus.clone();
        let root = self.document_window;
        cx.defer(move |cx| {
            if !cx.windows().contains(&root) {
                return;
            }
            if let Err(error) = root.update(cx, |_, window, cx| {
                window.activate_window();
                focus.focus(window, cx);
            }) {
                eprintln!("could not restore document focus: {error}");
            }
        });
    }

    pub(super) fn dismiss_floating(
        &mut self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if kind.confirmation() && self.file_busy {
            return;
        }
        if kind == DialogKind::MacroRepeat
            && self.route_to_document(window, cx, |view, window, cx| {
                view.dismiss_floating(DialogKind::MacroRepeat, window, cx)
            })
        {
            return;
        }
        match kind {
            DialogKind::FindInResults => self.dismiss_find_in_results(cx),
            DialogKind::FindInResultsProgress => self.cancel_find_in_results_progress(cx),
            DialogKind::Close => {
                self.cancel_close(window, cx);
                return;
            }
            DialogKind::SaveAll => {
                self.cancel_save_all(window, cx);
                return;
            }
            DialogKind::Reload => {
                self.cancel_reload(window, cx);
                return;
            }
            DialogKind::Find => {
                self.close_search_history();
                self.find_visible = false;
                self.files_search.visible = false;
            }
            DialogKind::GoTo => self.goto_visible = false,
            DialogKind::Preferences => self.preferences_visible = false,
            DialogKind::About => self.about_visible = false,
            DialogKind::MarkdownPreview => self.hide_markdown_preview(cx),
            DialogKind::MacroSave => self.macro_dialogs.save_visible = false,
            DialogKind::MacroRepeat => {
                self.cancel_macro_repeat(window, cx);
                self.macro_dialogs.repeat_visible = false;
            }
            DialogKind::Udl => {
                self.udl.visible = false;
                self.udl.styler_visible = false;
            }
            DialogKind::UdlStyler => self.udl.styler_visible = false,
            DialogKind::ColumnEditor => self.dismiss_column_editor(cx),
            DialogKind::DocumentSwitcher => self.dismiss_document_switcher(cx),
            DialogKind::Completion => self.dismiss_completion(cx),
            DialogKind::Calltip => self.dismiss_calltip(cx),
            DialogKind::BackupFailure => self.dismiss_backup_failure(cx),
            DialogKind::Run => self.run_visible = false,
        }
        cx.notify();
        self.restore_document_focus(cx);
    }

    pub(super) fn present_floating(
        &mut self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_floating_windows(window, cx);
        if let Some(handle) = self.floating_windows.get(&kind).copied() {
            self.focus_floating(kind, handle, cx);
        }
    }

    fn focus_floating(
        &self,
        kind: DialogKind,
        handle: gpui::WindowHandle<FloatingWindow>,
        cx: &mut Context<Self>,
    ) {
        if matches!(kind, DialogKind::Completion | DialogKind::Calltip) {
            return;
        }
        if matches!(
            kind,
            DialogKind::ColumnEditor
                | DialogKind::DocumentSwitcher
                | DialogKind::BackupFailure
                | DialogKind::FindInResultsProgress
        ) {
            let owner = cx.weak_entity();
            cx.defer(move |cx| {
                if !cx.windows().contains(&handle.into()) {
                    return;
                }
                match handle.update(cx, |_, window, cx| {
                    window.activate_window();
                    owner.update(cx, |view, cx| match kind {
                        DialogKind::ColumnEditor => view.focus_column_editor(window, cx),
                        DialogKind::BackupFailure => view.focus_backup_failure(window, cx),
                        DialogKind::FindInResultsProgress => {
                            view.focus_find_in_results_progress(window, cx)
                        }
                        _ => view.focus_document_switcher(window, cx),
                    })
                }) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) | Err(error) => {
                        eprintln!("Could not focus Column Editor: {error}")
                    }
                }
            });
            return;
        }
        let field = match kind {
            DialogKind::Find => Some(self.find_query.clone()),
            DialogKind::FindInResults => Some(self.find_in_results_input()),
            DialogKind::GoTo => Some(self.goto_input.clone()),
            DialogKind::Run => Some(self.run_input.clone()),
            DialogKind::MacroSave => Some(self.macro_dialogs.name_input.clone()),
            DialogKind::MacroRepeat => Some(self.macro_dialogs.count_input.clone()),
            _ => None,
        };
        let focus = match kind {
            DialogKind::Preferences => self.preferences_focus.clone(),
            DialogKind::Udl => self.udl.focus.clone(),
            DialogKind::UdlStyler => self.udl.styler_focus.clone(),
            _ => self.close_focus.clone(),
        };
        if let Err(error) = handle.update(cx, |_, window, cx| {
            window.activate_window();
            if let Some(field) = field {
                field.update(cx, |field, cx| field.focus(window, cx));
            } else {
                focus.focus(window, cx);
            }
        }) {
            eprintln!("could not focus auxiliary window: {error}");
        }
    }

    pub(super) fn sync_floating_windows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if Window::window_handle(window) != self.document_window {
            return;
        }
        for kind in DialogKind::ALL {
            if !kind.visible(self) {
                self.floating_opening.remove(&kind);
                if let Some(handle) = self.floating_windows.remove(&kind) {
                    if cx.windows().contains(&handle.into())
                        && let Err(error) = handle.update(cx, |_, window, _| window.remove_window())
                    {
                        eprintln!("could not retire auxiliary window: {error}");
                    }
                    if kind.confirmation()
                        && self.close_focus.is_focused(window)
                        && !self.close_dialog
                        && !self.save_all_dialog
                        && !self.reload_dialog
                    {
                        self.editor_focus.focus(window, cx);
                    }
                }
                continue;
            }
            if self
                .floating_windows
                .get(&kind)
                .is_some_and(|handle| cx.windows().contains(&(*handle).into()))
            {
                continue;
            }
            if !self.floating_opening.insert(kind) {
                continue;
            }
            let mut bounds = Bounds::centered(
                window.display(cx).map(|display| display.id()),
                kind.dimensions(),
                cx,
            );
            if matches!(kind, DialogKind::Completion | DialogKind::Calltip) {
                let Some(positioned) =
                    self.intelligence_popup_bounds(kind == DialogKind::Calltip, window, cx)
                else {
                    self.floating_opening.remove(&kind);
                    continue;
                };
                bounds = positioned;
                if let Some(display) = window.display(cx) {
                    let area = display.visible_bounds();
                    bounds.size.width = bounds.size.width.min(area.size.width);
                    bounds.size.height = bounds.size.height.min(area.size.height);
                    bounds.origin.x = bounds
                        .origin
                        .x
                        .max(area.left())
                        .min(area.right() - bounds.size.width);
                    bounds.origin.y = bounds
                        .origin
                        .y
                        .max(area.top())
                        .min(area.bottom() - bounds.size.height);
                }
            }
            let options = gpui::WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: !matches!(kind, DialogKind::Completion | DialogKind::Calltip),
                kind: gpui::WindowKind::Floating,
                titlebar: Some(TitlebarOptions {
                    title: Some(kind.title(self).into()),
                    ..Default::default()
                }),
                is_movable: true,
                is_minimizable: false,
                is_resizable: kind == DialogKind::MarkdownPreview,
                window_min_size: Some(bounds.size),
                ..Default::default()
            };
            let weak = cx.weak_entity();
            let root = self.document_window;
            cx.defer(move |cx| {
                let Some(owner) = weak.upgrade() else {
                    return;
                };
                if !cx.windows().contains(&root) || !kind.visible(owner.read(cx)) {
                    owner.update(cx, |view, _| {
                        view.floating_opening.remove(&kind);
                    });
                    return;
                }
                let observed = owner.clone();
                let opened = cx.open_window(options, move |window, cx| {
                    cx.new(|cx| FloatingWindow::new(observed, kind, window, cx))
                });
                owner.update(cx, |view, cx| {
                    view.floating_opening.remove(&kind);
                    match opened {
                        Ok(handle) => {
                            view.floating_windows.insert(kind, handle);
                            view.focus_floating(kind, handle, cx);
                        }
                        Err(error) => {
                            let title = kind.title(view);
                            view.floating_opening.insert(kind);
                            match kind {
                                DialogKind::Close => view.dispatch_to_document(CancelClose, cx),
                                DialogKind::SaveAll => view.dispatch_to_document(CancelSaveAll, cx),
                                DialogKind::Reload => view.dispatch_to_document(CancelReload, cx),
                                _ => kind.hide_modeless(view, cx),
                            }
                            view.notify_editor_error(
                                format!("Could not open {title}: {error}"),
                                cx,
                            );
                        }
                    }
                });
            });
        }
    }
}
