use super::*;
use super::dialog_font::px;
use crate::hashes::HashAlgorithm;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DialogKind {
    Find,
    FindSwap,
    FindInResults,
    FindInResultsProgress,
    GoTo,
    CharacterRange,
    Preferences,
    TerminalConfig,
    TerminalPassword,
    TerminalHostKey,
    StyleConfigurator,
    ShortcutMapper,
    About,
    Help,
    HashText(HashAlgorithm),
    HashFiles(HashAlgorithm),
    WindowManager,
    MarkdownPreview,
    MacroSave,
    MacroRepeat,
    Udl,
    UdlStyler,
    UdlName,
    ColumnEditor,
    DocumentSwitcher,
    Completion,
    Calltip,
    BackupFailure,
    Close,
    SaveAll,
    Reload,
}

impl DialogKind {
    const ALL: [Self; 37] = [
        Self::Find,
        Self::FindSwap,
        Self::FindInResults,
        Self::FindInResultsProgress,
        Self::GoTo,
        Self::CharacterRange,
        Self::Preferences,
        Self::TerminalConfig,
        Self::TerminalPassword,
        Self::TerminalHostKey,
        Self::StyleConfigurator,
        Self::ShortcutMapper,
        Self::About,
        Self::Help,
        Self::HashText(HashAlgorithm::Md5),
        Self::HashText(HashAlgorithm::Sha1),
        Self::HashText(HashAlgorithm::Sha256),
        Self::HashText(HashAlgorithm::Sha512),
        Self::HashFiles(HashAlgorithm::Md5),
        Self::HashFiles(HashAlgorithm::Sha1),
        Self::HashFiles(HashAlgorithm::Sha256),
        Self::HashFiles(HashAlgorithm::Sha512),
        Self::WindowManager,
        Self::MarkdownPreview,
        Self::MacroSave,
        Self::MacroRepeat,
        Self::Udl,
        Self::UdlStyler,
        Self::UdlName,
        Self::ColumnEditor,
        Self::DocumentSwitcher,
        Self::Completion,
        Self::Calltip,
        Self::BackupFailure,
        Self::Close,
        Self::SaveAll,
        Self::Reload,
    ];

    fn visible(self, owner: &MatrixWindow) -> bool {
        match self {
            Self::Find => owner.find_visible || owner.files_search.visible,
            Self::FindSwap => owner.find_swap_popup_visible(),
            Self::FindInResults => owner.find_in_results_visible(),
            Self::FindInResultsProgress => owner.find_in_results_progress_visible(),
            Self::GoTo => owner.goto_visible,
            Self::CharacterRange => owner.character_range.visible,
            Self::Preferences => owner.preferences_visible,
            Self::TerminalConfig => owner.terminal_config.visible,
            Self::TerminalPassword => owner.terminal_config.password_visible,
            Self::TerminalHostKey => owner.ssh_terminal.fingerprint.is_some(),
            Self::StyleConfigurator => owner.style_config.visible,
            Self::ShortcutMapper => owner.shortcut_mapper.visible,
            Self::About => owner.about_visible,
            Self::Help => owner.help_state.topic.is_some(),
            Self::HashText(algorithm) => owner.hash_text_visible(algorithm),
            Self::HashFiles(algorithm) => owner.hash_files_visible(algorithm),
            Self::WindowManager => owner.window_manager_visible(),
            Self::MarkdownPreview => owner.markdown_preview.visible,
            Self::MacroSave => owner.macro_dialogs.save_visible,
            Self::MacroRepeat => owner.macro_dialogs.repeat_visible,
            Self::Udl => owner.udl.visible && !owner.udl.docked,
            Self::UdlStyler => owner.udl.styler_visible,
            Self::UdlName => owner.udl_name_visible(),
            Self::ColumnEditor => owner.column_editor.visible,
            Self::DocumentSwitcher => owner.document_switcher_visible(),
            Self::Completion => owner.completion_visible(),
            Self::Calltip => owner.calltip_visible(),
            Self::BackupFailure => owner.backup_failure_visible(),
            Self::Close => owner.close_dialog,
            Self::SaveAll => owner.save_all_dialog,
            Self::Reload => owner.reload_dialog,
        }
    }

    fn translation_path(self) -> &'static str {
        match self {
            Self::Find | Self::FindSwap => "Find",
            Self::FindInResults => "FindInFinder",
            Self::GoTo => "GoToLine",
            Self::CharacterRange => "FindCharsInRange",
            Self::Preferences => "Preference",
            Self::StyleConfigurator => "StyleConfig",
            Self::ShortcutMapper => "ShortcutMapper",
            Self::Help => "DebugInfo",
            Self::HashText(HashAlgorithm::Md5) => "MD5FromTextDlg",
            Self::HashText(HashAlgorithm::Sha1) => "SHA1FromTextDlg",
            Self::HashText(HashAlgorithm::Sha256) => "SHA256FromTextDlg",
            Self::HashText(HashAlgorithm::Sha512) => "SHA512FromTextDlg",
            Self::HashFiles(HashAlgorithm::Md5) => "MD5FromFilesDlg",
            Self::HashFiles(HashAlgorithm::Sha1) => "SHA1FromFilesDlg",
            Self::HashFiles(HashAlgorithm::Sha256) => "SHA256FromFilesDlg",
            Self::HashFiles(HashAlgorithm::Sha512) => "SHA512FromFilesDlg",
            Self::WindowManager => "Window",
            Self::MacroSave => "ShortcutMapperSubDialg",
            Self::MacroRepeat => "MultiMacro",
            Self::Udl => "UserDefine",
            Self::UdlStyler => "UserDefine/StylerDialog",
            Self::UdlName => "UserDefine",
            Self::ColumnEditor => "ColumnEditor",
            Self::Close => "DoSaveOrNot",
            Self::SaveAll => "DoSaveAll",
            Self::Reload => "DoReloadOrNot",
            _ => "",
        }
    }

    fn title(self, owner: &MatrixWindow) -> &'static str {
        if self == Self::Find { return owner.find_dialog_title(); }
        if self == Self::UdlName { return owner.udl_name_title(); }
        if self == Self::Reload {
            return crate::localization::dialog_title(&owner.native_locale(),
                if owner.workspace.active().is_modified() { "DoReloadOrNotAndLooseChange" } else { "DoReloadOrNot" }).unwrap_or("Reload");
        }
        if self != Self::Help && let Some(title) = crate::localization::dialog_title(&owner.native_locale(), self.translation_path())
            .filter(|title| !title.is_empty()) {
            return title;
        }
        match self {
            Self::Find => "Find",
            Self::FindSwap => "",
            Self::FindInResults => "Find in search results",
            Self::FindInResultsProgress => owner.find_in_results_progress_title(),
            Self::GoTo => "转至...",
            Self::CharacterRange => "查找范围内的字符...",
            Self::Preferences => crate::localization::preference_title(&owner.native_locale(), "").unwrap_or("Preferences"),
            Self::TerminalConfig => owner.terminal_label("终端配置", "Terminal Configuration"),
            Self::TerminalPassword => owner.terminal_label("SSH 密码", "SSH Password"),
            Self::TerminalHostKey => owner.terminal_label("信任本机 SSH", "Trust Local SSH Host"),
            Self::StyleConfigurator => crate::localization::dialog_title(&owner.native_locale(), "StyleConfig").unwrap_or("Style Configurator"),
            Self::ShortcutMapper => owner.shortcut_mapper_title(),
            Self::About => "About Notepad Matrix",
            Self::Help => owner.help_title(),
            Self::HashText(algorithm) => owner.hash_text_title(algorithm),
            Self::HashFiles(algorithm) => owner.hash_files_title(algorithm),
            Self::WindowManager => "文件窗口",
            Self::MarkdownPreview => "Markdown Preview",
            Self::MacroSave => "Save Current Recorded Macro",
            Self::MacroRepeat => "Run a Macro Multiple Times",
            Self::Udl => "User Defined Language",
            Self::UdlStyler => "UDL Styler",
            Self::UdlName => owner.udl_name_title(),
            Self::ColumnEditor => "列编辑器",
            Self::DocumentSwitcher => "Document switcher",
            Self::Completion => "Completion",
            Self::Calltip => "Function parameters",
            Self::BackupFailure => "Backup error",
            Self::Close => crate::localization::dialog_title(&owner.native_locale(), "DoSaveOrNot").unwrap_or("Save"),
            Self::SaveAll => "Save all?",
            Self::Reload => "Reload?",
        }
    }

    fn window_title(self, owner: &MatrixWindow) -> String {
        if self == Self::WindowManager { owner.window_manager_title() } else { self.title(owner).to_owned() }
    }

    fn dimensions(self, owner: &MatrixWindow, window: &Window) -> gpui::Size<gpui::Pixels> {
        let client = match self {
            Self::Find => size(
                px(find_surface::DIALOG_WIDTH),
                px(find_surface::DIALOG_HEIGHT),
            ),
            Self::FindInResults => size(px(548.), px(186.)),
            Self::FindSwap => size(px(248.), px(74.)),
            Self::FindInResultsProgress => size(px(565.), px(95.)),
            Self::GoTo => goto_surface::dimensions(),
            Self::CharacterRange => size(px(468.), px(174.)),
            Self::Preferences => preferences_surface::dimensions(),
            Self::TerminalConfig => terminal_config_surface::dimensions(&owner.terminal_config),
            Self::TerminalPassword => size(px(400.), px(170.)),
            Self::TerminalHostKey => size(px(540.), px(190.)),
            Self::StyleConfigurator => style_config_surface::dimensions(),
            Self::ShortcutMapper => shortcut_surface::dimensions(),
            Self::About => about_surface::dimensions(),
            Self::Help => size(px(660.), px(440.)),
            Self::HashText(_) | Self::HashFiles(_) => hash_surface::dimensions(),
            Self::WindowManager => window_manager_surface::dimensions(),
            Self::MarkdownPreview => size(gpui::px(700.), gpui::px(560.)),
            Self::MacroSave => size(px(420.), px(200.)),
            Self::MacroRepeat => size(px(480.), px(360.)),
            Self::Udl => size(px(udl_surface::WIDTH), px(udl_surface::HEIGHT)),
            Self::UdlStyler => size(px(486.), px(457.5)),
            Self::UdlName => size(px(udl_surface::NAME_WIDTH), px(udl_surface::NAME_HEIGHT)),
            Self::ColumnEditor => column_editor_surface::dialog_dimensions(window),
            Self::DocumentSwitcher => size(gpui::px(380. * dialog_font::switcher_scale()), gpui::px(320. * dialog_font::switcher_scale())),
            Self::Completion => size(gpui::px(360.), gpui::px(240.)),
            Self::Calltip => size(gpui::px(520.), gpui::px(160.)),
            Self::BackupFailure => size(px(520.), px(260.)),
            Self::Close => owner.close_dialog_dimensions(window),
            Self::SaveAll | Self::Reload => size(px(480.), px(220.)),
        };
        size(client.width, client.height + if self.borderless_popup() { gpui::px(0.) } else { gpui::px(window_caption::HEIGHT) })
    }

    fn confirmation(self) -> bool {
        matches!(self, Self::Close | Self::SaveAll | Self::Reload)
    }

    fn borderless_popup(self) -> bool {
        matches!(self, Self::FindSwap | Self::Completion)
    }

    fn hide_modeless(self, owner: &mut MatrixWindow, cx: &mut Context<MatrixWindow>) {
        match self {
            Self::FindSwap => { owner.close_find_swap_menu(cx); }
            Self::FindInResults => owner.dismiss_find_in_results(cx),
            Self::FindInResultsProgress => owner.cancel_find_in_results_progress(cx),
            Self::Find => {
                owner.close_search_history();
                owner.find_visible = false;
                owner.files_search.visible = false;
            }
            Self::GoTo => owner.goto_visible = false,
            Self::CharacterRange => owner.character_range.visible = false,
            Self::Preferences => { owner.preferences_visible = false; owner.config_directory.close(); },
            Self::TerminalConfig => owner.hide_terminal_config(cx),
            Self::TerminalPassword => owner.terminal_config.dismiss_password(),
            Self::TerminalHostKey => owner.cancel_terminal_host_key(cx),
            Self::StyleConfigurator => owner.style_config.visible = false,
            Self::ShortcutMapper => owner.shortcut_mapper.visible = false,
            Self::About => owner.matrix_about_closed(cx),
            Self::Help => owner.dismiss_help(cx),
            Self::HashText(algorithm) => owner.hide_hash_text(algorithm, cx),
            Self::HashFiles(algorithm) => owner.hide_hash_files(algorithm, cx),
            Self::WindowManager => { owner.dismiss_window_manager(cx); }
            Self::MarkdownPreview => owner.hide_markdown_preview(cx),
            Self::MacroSave => owner.macro_dialogs.save_visible = false,
            Self::MacroRepeat => owner.macro_dialogs.repeat_visible = false,
            Self::Udl => {
                owner.finish_udl_styler(false, cx);
                owner.dismiss_udl_name(cx);
                owner.udl.visible = false;
            }
            Self::UdlStyler => owner.finish_udl_styler(false, cx),
            Self::UdlName => owner.dismiss_udl_name(cx),
            Self::ColumnEditor => owner.dismiss_column_editor(cx),
            Self::DocumentSwitcher => owner.dismiss_document_switcher(cx),
            Self::Completion => owner.dismiss_completion(cx),
            Self::Calltip => owner.dismiss_calltip(cx),
            Self::BackupFailure => owner.dismiss_backup_failure(cx),
            Self::Close | Self::SaveAll | Self::Reload => {}
        }
    }
}

pub(super) struct FloatingWindow {
    owner: gpui::WeakEntity<MatrixWindow>,
    kind: DialogKind,
    _changed: gpui::Subscription,
    _released: gpui::Subscription,
    _window_size_indicator: Option<window_size_indicator::WindowSizeIndicator>,
}

impl FloatingWindow {
    pub(super) fn is_about(&self) -> bool {
        self.kind == DialogKind::About
    }

    fn new(
        owner: gpui::Entity<MatrixWindow>,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.set_window_title(&kind.window_title(owner.read(cx)));
        let window_size_indicator = if !kind.borderless_popup() {
            if let Err(error) = window_caption::update_native_buttons(window, true) {
                eprintln!("auxiliary titlebar: {error}");
            }
            match window_size_indicator::WindowSizeIndicator::new(window) {
                Ok(indicator) => Some(indicator),
                Err(error) => {
                    eprintln!("could not install auxiliary size indicator: {error}");
                    None
                }
            }
        } else { None };
        if kind.borderless_popup()
            && let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            if let Some(host) = view.window() {
                host.setStyleMask(objc2_app_kit::NSWindowStyleMask::NonactivatingPanel);
            }
        }
        let changed = cx.observe_in(&owner, window, move |_, owner, window, cx| {
            let view = owner.read(cx);
            if kind.visible(view) {
                window_caption::update_title(window, &kind.window_title(view));
                if matches!(kind, DialogKind::Close | DialogKind::TerminalConfig) {
                    let dimensions = kind.dimensions(view, window);
                    if (f32::from(window.viewport_size().width - dimensions.width)).abs() > 1.
                        || (f32::from(window.viewport_size().height - dimensions.height)).abs() > 1.
                    {
                        window.resize(dimensions);
                    }
                }
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
            if matrix_surface::locked() {
                return;
            }
            if matches!(kind, DialogKind::Find | DialogKind::FindSwap | DialogKind::FindInResults) {
                let active = window.is_window_active();
                let active_window: gpui::AnyWindowHandle = Window::window_handle(window);
                let owner = activated_owner.clone();
                cx.defer(move |cx| {
                    if matrix_surface::locked() {
                        return;
                    }
                    if let Err(error) = owner.update(cx, |owner, cx| {
                        if kind == DialogKind::FindSwap {
                            if !active { owner.close_find_swap_menu(cx); }
                        } else if kind == DialogKind::FindInResults {
                            if !active {
                                owner.retire_find_in_results_popup(cx);
                            }
                        } else if active {
                            if !std::mem::take(&mut owner.find_activation_prepared) {
                                owner.activate_find_selection(cx);
                            }
                        } else {
                            if owner.close_search_history() { cx.notify(); }
                        }
                        if kind == DialogKind::Find {
                            if !active { owner.finish_find_transparency_drag(cx); }
                            if let Err(error) = active_window.update(cx, |_, window, _| owner.update_find_window(window)) {
                                eprintln!("search transparency lost its window: {error}");
                            }
                        }
                    }) {
                        eprintln!("search activation lost its document: {error}");
                    }
                });
            }
        })
        .detach();
        window.on_window_should_close(cx, move |window, cx| {
            if matrix_surface::locked() && kind != DialogKind::About {
                return false;
            }
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
            _window_size_indicator: window_size_indicator,
        }
    }

    fn with_owner(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut MatrixWindow, &mut Window, &mut Context<MatrixWindow>),
    ) {
        if matrix_surface::locked() {
            return;
        }
        if let Err(error) = self.owner.update(cx, |owner, cx| update(owner, window, cx)) {
            eprintln!("auxiliary window lost its document: {error}");
            window.remove_window();
        }
    }
}

impl Render for FloatingWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matrix_surface::locked() && !self.is_about() {
            return div().size_full().bg(rgb(0x000000)).into_any_element();
        }
        let kind = self.kind;
        if kind == DialogKind::Find
            && let Some(owner) = self.owner.upgrade()
        {
            owner.read(cx).update_find_window(window);
        }
        if kind == DialogKind::Udl
            && let Some(owner) = self.owner.upgrade()
        {
            owner.read(cx).update_udl_window(window);
        }
        let content = self.owner.update(cx, |owner, cx| {
            let locale = owner.native_locale();
            crate::localization::with_dialog(&locale, kind.translation_path(), || match kind {
            DialogKind::Find if owner.files_search.visible => owner.render_find_in_files(cx),
            DialogKind::Find => owner.render_find(cx),
            DialogKind::FindSwap => owner.render_find_swap_menu(cx),
            DialogKind::FindInResults => Some(owner.render_find_in_results(cx)),
            DialogKind::FindInResultsProgress => owner.render_find_in_results_progress(cx),
            DialogKind::GoTo => owner.render_goto(cx),
            DialogKind::CharacterRange => Some(owner.render_character_range(cx)),
            DialogKind::Preferences => owner.render_preferences(window, cx),
            DialogKind::TerminalConfig => owner.render_terminal_config(window, cx),
            DialogKind::TerminalPassword => owner.render_terminal_password(window, cx),
            DialogKind::TerminalHostKey => owner.render_terminal_host_key(cx),
            DialogKind::StyleConfigurator => owner.render_style_configurator(window, cx),
            DialogKind::ShortcutMapper => owner.render_shortcut_mapper(window, cx),
            DialogKind::About => owner.render_about(window, cx),
            DialogKind::Help => owner.render_help(cx),
            DialogKind::HashText(algorithm) => owner.render_hash_text(algorithm, window, cx),
            DialogKind::HashFiles(algorithm) => owner.render_hash_files(algorithm, window, cx),
            DialogKind::WindowManager => owner.render_window_manager(window, cx),
            DialogKind::MarkdownPreview => owner.render_markdown_preview(cx),
            DialogKind::MacroSave => owner.render_save_macro(cx),
            DialogKind::MacroRepeat => owner.render_repeat_macro(cx),
            DialogKind::Udl => owner.render_udl(window, cx),
            DialogKind::UdlStyler => owner.render_udl_styler(window, cx),
            DialogKind::UdlName => owner.render_udl_name(window, cx),
            DialogKind::ColumnEditor => owner.render_column_editor(window, cx),
            DialogKind::DocumentSwitcher => owner.render_document_switcher(cx),
            DialogKind::Completion => owner.render_completion(cx),
            DialogKind::Calltip => owner.render_calltip(cx),
            DialogKind::BackupFailure => owner.render_backup_failure(cx),
            DialogKind::Close => owner.render_close_dialog(window, cx),
            DialogKind::SaveAll => owner.render_save_all_dialog(cx),
            DialogKind::Reload => owner.render_reload_dialog(cx),
            })
        });
        let content = match content {
            Ok(content) => content,
            Err(error) => {
                eprintln!("auxiliary render lost its document: {error}");
                window.remove_window();
                None
            }
        };
        let caption = (!kind.borderless_popup()).then(|| {
            let title = self.owner.upgrade().map(|owner| kind.window_title(owner.read(cx))).unwrap_or_default();
            let (close_id, icon_id) = if kind == DialogKind::Close {
                ("confirmation-caption-close", "confirmation-close-icon")
            } else {
                ("auxiliary-caption-close", "auxiliary-close-icon")
            };
            window_caption::frame()
                .child(window_caption::title(title).on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, cx| {
                    cx.stop_propagation();
                    window.start_window_move();
                })))
                .when(matches!(kind, DialogKind::MarkdownPreview | DialogKind::WindowManager), |caption| {
                    let maximized = window.is_maximized();
                    caption.child(div().id("auxiliary-caption-maximize").role(Role::Button)
                        .aria_label(if maximized { "Restore" } else { "Maximize" })
                        .w(gpui::px(45.)).h(gpui::px(30.)).flex_shrink_0().flex().items_center().justify_center()
                        .hover(|style| style.bg(rgb(0x5596d7)))
                        .child(caption_icons::render(if maximized { caption_icons::Icon::Restore } else { caption_icons::Icon::Maximize }, "auxiliary-maximize-icon", window_caption::FOREGROUND))
                        .on_click(cx.listener(|_, _, window, cx| {
                            cx.stop_propagation();
                            if !matrix_surface::locked() { window.zoom_window(); }
                        })))
                })
                .child(window_caption::close_button(close_id, icon_id, "Close")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        cx.stop_propagation();
                        if matrix_surface::locked() && kind != DialogKind::About { return; }
                        if let Err(error) = view.owner.update(cx, |owner, cx| {
                            if kind == DialogKind::Close { owner.cancel_close(window, cx); }
                            else { owner.dismiss_floating(kind, window, cx); }
                        }) {
                            eprintln!("auxiliary caption close lost its document: {error}");
                            window.remove_window();
                        }
                    })))
        });
        let fixed_body = matches!(kind, DialogKind::Find | DialogKind::FindSwap | DialogKind::Preferences | DialogKind::StyleConfigurator | DialogKind::About | DialogKind::Close | DialogKind::TerminalConfig | DialogKind::TerminalPassword | DialogKind::TerminalHostKey);
        let content = content.map(|content| {
            div().id("matrix-auxiliary-body").relative().flex_1().min_h(gpui::px(0.)).w_full()
                .when(fixed_body, |body| body.overflow_hidden())
                .when(!fixed_body, |body| body.overflow_y_scroll())
                .child(content).into_any_element()
        });
        div()
            .id("matrix-auxiliary-window")
            .key_context("MatrixAuxiliary")
            .tab_group()
            .size_full()
            .flex().flex_col()
            .bg(rgb(0xf0f0f0))
            .text_color(rgb(0x202020))
            .font_family("Helvetica")
            .text_size(gpui::px(13.))
            .when(!matches!(kind, DialogKind::Completion | DialogKind::Calltip | DialogKind::MarkdownPreview), |element| {
                element.font_family(dialog_font::font_family()).text_size(px(13.))
            })
            .on_action(cx.listener(|view, action: &shortcut_surface::ShortcutAction, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| owner.apply_shortcut_action(action, window, cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusNext, window, cx| {
                if matrix_surface::locked() {
                    cx.stop_propagation();
                    return;
                }
                window.focus_next(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &DialogFocusPrevious, window, cx| {
                if matrix_surface::locked() {
                    cx.stop_propagation();
                    return;
                }
                window.focus_prev(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &CloseWindow, window, cx| {
                if matrix_surface::locked() && view.is_about() {
                    if let Err(error) = view.owner.update(cx, |owner, cx| {
                        owner.dismiss_floating(DialogKind::About, window, cx)
                    }) {
                        eprintln!("About close lost its document: {error}");
                    }
                    cx.stop_propagation();
                    return;
                }
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
            .on_action(cx.listener(|view, _: &FindNext, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| {
                    owner.find_next(false, window, cx)
                });
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &find_surface::FindOptionAction, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| owner.find_option_command(action.0, window, cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &mark_surface::SourceAction, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| owner.execute_source_command(action.0, window, cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &find_surface::FindButtonAction, window, cx| {
                view.with_owner(window, cx, |owner, window, cx| owner.find_button_command(action.0, action.1, window, cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|view, action: &find_surface::FindFieldWarning, window, cx| {
                view.with_owner(window, cx, |owner, _, cx| owner.find_field_warning(action.0, cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |view, _: &FindDialogNext, window, cx| {
                view.with_owner(window, cx, move |owner, window, cx| {
                    if kind == DialogKind::FindInResults {
                        owner.submit_find_in_results(window, cx);
                    } else if kind == DialogKind::CharacterRange {
                        owner.find_character_range(window, cx);
                    } else {
                        owner.activate_find_default(false, window, cx);
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
                    owner.activate_find_default(true, window, cx)
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
            .children(caption)
            .children(content)
            .into_any_element()
    }
}

impl MatrixWindow {
    pub(super) fn auxiliary_client_size(&self, window: &Window) -> gpui::Size<gpui::Pixels> {
        let viewport = window.viewport_size();
        if Window::window_handle(window) == self.document_window {
            viewport
        } else {
            size(viewport.width, (viewport.height - gpui::px(window_caption::HEIGHT)).max(gpui::px(0.)))
        }
    }

    pub(super) fn route_to_document(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        if matrix_surface::locked() {
            return true;
        }
        if Window::window_handle(window) == self.document_window {
            return false;
        }
        let root = self.document_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            if matrix_surface::locked() || !cx.windows().contains(&root) {
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
        if matrix_surface::locked() {
            return;
        }
        let root = self.document_window;
        cx.defer(move |cx| {
            if matrix_surface::locked() || !cx.windows().contains(&root) {
                return;
            }
            if let Err(error) = root.update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(action), cx)
            }) {
                eprintln!("auxiliary action could not reach its document: {error}");
            }
        });
    }

    pub(super) fn restore_document_focus(&self, cx: &mut Context<Self>) {
        if matrix_surface::locked() {
            return;
        }
        let focus = self.editor_focus.clone();
        let root = self.document_window;
        cx.defer(move |cx| {
            if matrix_surface::locked() || !cx.windows().contains(&root) {
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
        if matrix_surface::locked() && kind != DialogKind::About {
            return;
        }
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
            DialogKind::FindSwap => {
                self.close_find_swap_menu(cx);
                self.focus_find_swap_button(cx);
                return;
            }
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
                if self.close_find_swap_menu(cx) { return; }
                if self.cancel_active_file_search(cx) {
                    return;
                }
                self.close_search_history();
                self.finish_find_transparency_drag(cx);
                self.set_find_status(find_surface::FindStatus::NoMessage, "");
                self.find_visible = false;
                self.files_search.visible = false;
            }
            DialogKind::GoTo => self.clear_goto(window, cx),
            DialogKind::CharacterRange => self.character_range.visible = false,
            DialogKind::Preferences => {
                if !self.finish_preferences(window, cx) {
                    return;
                }
                self.preferences_visible = false;
                self.config_directory.close();
            }
            DialogKind::TerminalConfig => self.hide_terminal_config(cx),
            DialogKind::TerminalPassword => self.terminal_config.dismiss_password(),
            DialogKind::TerminalHostKey => self.cancel_terminal_host_key(cx),
            DialogKind::StyleConfigurator => {
                if self.style_config.visible { self.close_style_configurator(window, cx); return; }
            }
            DialogKind::ShortcutMapper => {
                if self.shortcut_mapper.visible { self.close_shortcut_mapper(window, cx); return; }
            }
            DialogKind::About => self.matrix_about_closed(cx),
            DialogKind::Help => self.dismiss_help(cx),
            DialogKind::HashText(algorithm) => self.hide_hash_text(algorithm, cx),
            DialogKind::HashFiles(algorithm) => self.hide_hash_files(algorithm, cx),
            DialogKind::WindowManager => { if !self.dismiss_window_manager(cx) { return; } }
            DialogKind::MarkdownPreview => self.hide_markdown_preview(cx),
            DialogKind::MacroSave => self.macro_dialogs.save_visible = false,
            DialogKind::MacroRepeat => {
                self.cancel_macro_repeat(window, cx);
                self.macro_dialogs.repeat_visible = false;
            }
            DialogKind::Udl => {
                self.finish_udl_styler(false, cx);
                self.dismiss_udl_name(cx);
                self.udl.visible = false;
            }
            DialogKind::UdlStyler => self.finish_udl_styler(false, cx),
            DialogKind::UdlName => self.dismiss_udl_name(cx),
            DialogKind::ColumnEditor => self.dismiss_column_editor(cx),
            DialogKind::DocumentSwitcher => self.dismiss_document_switcher(cx),
            DialogKind::Completion => self.dismiss_completion(cx),
            DialogKind::Calltip => self.dismiss_calltip(cx),
            DialogKind::BackupFailure => self.dismiss_backup_failure(cx),
        }
        cx.notify();
        if kind == DialogKind::TerminalHostKey && self.ssh_terminal.visible {
            self.restore_terminal_focus(cx);
        } else {
            self.restore_document_focus(cx);
        }
    }

    pub(super) fn present_floating(
        &mut self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matrix_surface::locked() && kind != DialogKind::About {
            return;
        }
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
        if matrix_surface::locked() && kind != DialogKind::About {
            return;
        }
        if matches!(kind, DialogKind::Completion | DialogKind::Calltip) {
            return;
        }
        if matches!(
            kind,
            DialogKind::ColumnEditor
                | DialogKind::HashText(_)
                | DialogKind::HashFiles(_)
                | DialogKind::DocumentSwitcher
                | DialogKind::BackupFailure
                | DialogKind::FindInResultsProgress
        ) {
            let owner = cx.weak_entity();
            cx.defer(move |cx| {
                if matrix_surface::locked() || !cx.windows().contains(&handle.into()) {
                    return;
                }
                match handle.update(cx, |_, window, cx| {
                    window.activate_window();
                    owner.update(cx, |view, cx| match kind {
                        DialogKind::ColumnEditor => view.focus_column_editor(window, cx),
                        DialogKind::HashText(algorithm) => view.focus_hash_text(algorithm, window, cx),
                        DialogKind::HashFiles(algorithm) => view.focus_hash_files(algorithm, window, cx),
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
            DialogKind::MacroSave => Some(self.macro_dialogs.name_input.clone()),
            DialogKind::MacroRepeat => Some(self.macro_dialogs.count_input.clone()),
            DialogKind::UdlName => Some(self.udl_name_input()),
            _ => None,
        };
        let focus = match kind {
            DialogKind::FindSwap => self.find_swap_popup_focus(),
            DialogKind::WindowManager => self.window_manager_focus(),
            DialogKind::Preferences => self.preferences_focus.clone(),
            DialogKind::TerminalConfig | DialogKind::TerminalPassword | DialogKind::TerminalHostKey => self.terminal_config.focus.clone(),
            DialogKind::StyleConfigurator => self.style_config.focus.clone(),
            DialogKind::ShortcutMapper => self.preferences_focus.clone(),
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
            if matrix_surface::locked() && kind != DialogKind::About {
                continue;
            }
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
                if kind == DialogKind::Find && self.find_controls.compact {
                    size(px(find_surface::DIALOG_WIDTH), px(find_surface::COMPACT_DIALOG_HEIGHT) + gpui::px(window_caption::HEIGHT))
                } else {
                    kind.dimensions(self, window)
                },
                cx,
            );
            if kind == DialogKind::WindowManager && let Some(previous) = self.window_manager_bounds() {
                bounds = previous;
            }
            if matches!(kind, DialogKind::FindSwap | DialogKind::Completion | DialogKind::Calltip) {
                let Some(positioned) = (if kind == DialogKind::FindSwap {
                    self.find_swap_popup_bounds()
                } else {
                    self.intelligence_popup_bounds(kind == DialogKind::Calltip, window, cx)
                })
                else {
                    self.floating_opening.remove(&kind);
                    continue;
                };
                bounds = positioned;
                if !kind.borderless_popup() {
                    bounds.size.height += gpui::px(window_caption::HEIGHT);
                }
                if kind != DialogKind::FindSwap && let Some(display) = window.display(cx) {
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
            let resizable = matches!(kind, DialogKind::MarkdownPreview | DialogKind::WindowManager);
            let minimum = if kind == DialogKind::WindowManager {
                let client = window_manager_surface::minimum_dimensions();
                size(client.width, client.height + gpui::px(window_caption::HEIGHT))
            } else { bounds.size };
            let content = if kind.borderless_popup() { Ok(bounds.size) } else {
                window_caption::content_size_for_frame(window, bounds.size, resizable, false)
            };
            let content = match content {
                Ok(content) => content,
                Err(error) => {
                    self.floating_opening.remove(&kind);
                    self.notify_editor_error(format!("无法计算弹窗尺寸：{error}"), cx);
                    continue;
                }
            };
            bounds.size = content;
            let options = gpui::WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: !matches!(kind, DialogKind::Completion | DialogKind::Calltip),
                kind: if kind.borderless_popup() { gpui::WindowKind::PopUp } else { gpui::WindowKind::Floating },
                titlebar: (!kind.borderless_popup()).then(|| TitlebarOptions {
                    title: Some(kind.window_title(self).into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                app_owns_titlebar_drag: !kind.borderless_popup(),
                is_movable: !kind.borderless_popup(),
                is_minimizable: false,
                is_resizable: resizable,
                window_min_size: Some(minimum),
                ..Default::default()
            };
            let weak = cx.weak_entity();
            let root = self.document_window;
            cx.defer(move |cx| {
                let Some(owner) = weak.upgrade() else {
                    return;
                };
                if (matrix_surface::locked() && kind != DialogKind::About)
                    || !cx.windows().contains(&root)
                    || !kind.visible(owner.read(cx))
                {
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
