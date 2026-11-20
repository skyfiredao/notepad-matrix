use super::*;
use super::dialog_font::px;
use crate::localization::{Text, localized_string};
use crate::udl_controls::{self, Control, Kind, Page};
use crate::udl_registry::{SaveReceipt, UdlRegistry};
use std::collections::{BTreeMap, VecDeque};
#[path = "../udl_association.rs"]
mod associations;
#[path = "udl_localization.rs"]
mod localization;
#[path = "udl_field.rs"]
mod field;
#[path = "udl_pages_surface.rs"]
mod pages;
#[path = "udl_styler_surface.rs"]
mod styler;
#[path = "udl_main_surface.rs"]
mod main;
#[path = "udl_docking_surface.rs"]
mod docking;

pub(super) fn du(value: f32) -> gpui::Pixels { px(value * 1.5) }

pub(super) const WIDTH: f32 = 760.;
pub(super) const HEIGHT: f32 = 760.;
pub(super) const NAME_WIDTH: f32 = 320.;
pub(super) const NAME_HEIGHT: f32 = 120.;

fn at((x, y, width, height): (f32, f32, f32, f32)) -> gpui::Div {
    div().absolute().left(du(x)).top(du(y)).w(du(width)).h(du(height))
}

struct SourceField {
    field: gpui::Entity<field::Field>,
    _changed: gpui::Subscription,
}

enum IoJob {
    Load,
    Save(UdlRegistry),
    Import(PathBuf),
    Export(PathBuf, serde_json::Value),
}
enum IoOutcome {
    Loaded(UdlRegistry),
    Saved(SaveReceipt),
    Imported(serde_json::Value),
    Exported,
}

pub(super) struct UdlState {
    pub registry: UdlRegistry,
    pub(super) syntax_cache: Option<super::language_surface::UdlSyntaxCache>,
    pub visible: bool,
    pub styler_visible: bool,
    pub(super) docked: bool,
    transparent: bool,
    opacity: u8,
    main: main::State,
    styler: styler::State,
    pub focus: FocusHandle,
    pub styler_focus: FocusHandle,
    selected: u64,
    page: Page,
    style: usize,
    source_fields: BTreeMap<String, SourceField>,
    radio_focus: BTreeMap<String, [FocusHandle; 3]>,
    name: gpui::Entity<text_field::TextField>,
    root: Option<PathBuf>,
    generation: u64,
    ready: bool,
    load_attempted: bool,
    busy: bool,
    pending_save: bool,
    jobs: VecDeque<IoJob>,
    task: Option<gpui::Task<()>>,
    prompt_task: Option<gpui::Task<()>>,
    prompting: bool,
    error: Option<Text>,
    message: Option<Text>,
    error_notice_source: Option<Text>,
    confirm_remove: bool,
    menu_languages: Option<Vec<(u64, String)>>,
    missing: Vec<(crate::workspace::BufferId, u64)>,
    missing_warning: Option<Text>,
    needs_style_invalidation: bool,
    close_requested: bool,
    close_resume_queued: bool,
    close_failed_revision: Option<u64>,
}
impl UdlState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            registry: UdlRegistry::default(),
            syntax_cache: None,
            visible: false,
            styler_visible: false,
            docked: false,
            transparent: false,
            opacity: 150,
            main: main::State::new(),
            styler: styler::State::new(),
            focus: cx.focus_handle(),
            styler_focus: cx.focus_handle(),
            selected: 0,
            page: Page::FolderDefault,
            style: 0,
            source_fields: BTreeMap::new(),
            radio_focus: BTreeMap::new(),
            name: cx.new(|cx| text_field::TextField::compact(crate::localization::dialog_control("english", "UserDefine", 20007).unwrap_or("User language"), "udl-name", cx).without_history_arrow().with_compact_height(dialog_font::logical(dialog_font::control_height())).with_max_utf16(63)),
            root: None,
            generation: 0,
            ready: false,
            load_attempted: false,
            busy: false,
            pending_save: false,
            jobs: VecDeque::new(),
            task: None,
            prompt_task: None,
            prompting: false,
            error: None,
            message: None,
            error_notice_source: None,
            confirm_remove: false,
            menu_languages: None,
            missing: Vec::new(),
            missing_warning: None,
            needs_style_invalidation: false,
            close_requested: false,
            close_resume_queued: false,
            close_failed_revision: None,
        }
    }
}

impl MatrixWindow {
    pub(super) fn open_udl_folder(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.settings_store.as_ref() else { return; };
        if let Err(error) = crate::native_app_open::open_default(store.root()) {
            self.udl.error = Some(Text::resource("matrix-udl-error", "User-defined language error: $ERROR$")
                .replace("$ERROR$", error.to_string()));
            self.notify_udl_error(cx);
        }
    }

    pub(super) fn refresh_udl_registry(&mut self, cx: &mut Context<Self>) {
        let root = self
            .settings_store
            .as_ref()
            .map(|store| store.root().to_path_buf());
        if self.udl.root != root {
            if self.udl.styler_visible { self.finish_udl_styler(false, cx); }
            self.dismiss_udl_name(cx);
            self.udl.root = root;
            self.udl.generation = self.udl.generation.wrapping_add(1);
            self.udl.ready = false;
            self.udl.load_attempted = false;
            self.udl.pending_save = false;
            self.udl.jobs.clear();
            self.udl.registry = UdlRegistry::default();
            self.udl.syntax_cache = None;
            self.udl.needs_style_invalidation = true;
            self.udl.selected = 0;
            self.udl.source_fields.clear();
            self.udl.error = None;
            self.udl.close_failed_revision = None;
        }
        if self.udl.root.is_none() {
            self.udl.ready = true;
        } else if !self.udl.ready
            && !self.udl.load_attempted
            && !self.udl.busy
            && self.udl.jobs.is_empty()
        {
            self.udl.load_attempted = true;
            self.udl.jobs.push_back(IoJob::Load);
        }
        self.start_udl_io(cx);
        let languages = self
            .udl
            .registry
            .named()
            .map(|(id, d)| (id, d.name.clone()))
            .collect::<Vec<_>>();
        if self.udl.menu_languages.as_ref() != Some(&languages) {
            self.menu_bar.set_user_languages(languages.clone());
            self.udl.menu_languages = Some(languages);
        }
        if self.udl.ready {
            let result =
                associations::refresh(&self.udl.registry, self.workspace.buffers_mut(), self.dark);
            if result.changed {
                self.udl.needs_style_invalidation = true;
            }
            let missing_warning = if result.missing.is_empty() {
                None
            } else {
                Some(Text::resource(
                    "matrix-udl-missing-ids",
                    "Missing user-defined language ID(s): $IDS$. Document text and saved language identities were preserved. Restore the definitions or explicitly select another Language.",
                ).replace("$IDS$", result.missing.iter().map(|(_, id)| id.to_string()).collect::<Vec<_>>().join(", ")))
            };
            if result.missing != self.udl.missing {
                self.udl.missing = result.missing;
                if let Some(previous) = self.udl.missing_warning.take() {
                    self.clear_editor_error_source(&previous);
                }
                self.udl.missing_warning = missing_warning;
                if let Some(error) = self.udl.missing_warning.clone() {
                    self.notify_localized_editor_error(error, cx);
                }
            }
        }
        self.resume_udl_close_if_ready(cx);
    }

    pub(super) fn prepare_udl_close(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.udl.styler_visible { self.finish_udl_styler(false, cx); }
        self.dismiss_udl_name(cx);
        self.udl.close_requested = false;
        self.refresh_udl_registry(cx);
        self.udl.close_requested = true;
        if self.udl.ready
            && self.udl.root.is_some()
            && self.udl.registry.has_unsaved_changes()
            && self.udl.close_failed_revision != Some(self.udl.registry.revision())
        {
            self.udl.pending_save = true;
        }
        self.start_udl_io(cx);
        if self.udl_work_pending() {
            return false;
        }
        self.udl.close_requested = false;
        self.udl.close_failed_revision = None;
        true
    }

    pub(super) fn udl_work_pending(&self) -> bool {
        self.udl.busy || self.udl.pending_save || !self.udl.jobs.is_empty() || self.udl.prompting
    }

    fn resume_udl_close_if_ready(&mut self, cx: &mut Context<Self>) {
        if !self.udl.close_requested || self.udl.close_resume_queued || self.udl_work_pending() {
            return;
        }
        self.udl.close_resume_queued = true;
        let root = self.document_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            if !cx.windows().contains(&root) {
                return;
            }
            match root.update(cx, |_, window, cx| {
                owner.update(cx, |view, cx| {
                    view.udl.close_resume_queued = false;
                    if view.udl.close_requested && !view.udl_work_pending() {
                        view.request_close(window, cx);
                    }
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("UDL exit flush could not resume its document close: {error}")
                }
            }
        });
    }

    pub(super) fn flush_udl_style_invalidation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if std::mem::take(&mut self.udl.needs_style_invalidation) {
            self.invalidate_udl_styles(window, cx);
        }
    }

    fn start_udl_io(&mut self, cx: &mut Context<Self>) {
        if self.udl.busy || self.udl.styler_visible {
            return;
        }
        let root = self.udl.root.clone();
        let home = root
            .as_ref()
            .and_then(|root| root.parent())
            .map(PathBuf::from);
        if home.is_none() && self.udl.pending_save {
            self.udl.pending_save = false;
            self.udl.message = Some(Text::resource("matrix-udl-memory-only", "Definitions are in memory only (no configuration directory)"));
        }
        let job = self.udl.jobs.pop_front().or_else(|| {
            if self.udl.pending_save && self.udl.ready {
                self.udl.pending_save = false;
                Some(IoJob::Save(self.udl.registry.clone()))
            } else {
                None
            }
        });
        let Some(job) = job else {
            return;
        };
        let generation = self.udl.generation;
        let saving = matches!(&job, IoJob::Save(_));
        let file_export = match &job {
            IoJob::Import(_) => Some(false),
            IoJob::Export(_, _) => Some(true),
            _ => None,
        };
        self.udl.busy = true;
        let worker = cx.background_executor().spawn(async move {
            match job {
                IoJob::Load => {
                    let home = home
                        .as_ref()
                        .ok_or_else(|| io::Error::other("No configuration directory"))?;
                    UdlRegistry::load(&ConfigStore::under_home(home)).map(IoOutcome::Loaded)
                }
                IoJob::Save(mut registry) => {
                    let home = home
                        .as_ref()
                        .ok_or_else(|| io::Error::other("No configuration directory"))?;
                    registry
                        .save(&ConfigStore::under_home(home))
                        .map(IoOutcome::Saved)
                }
                IoJob::Import(path) => {
                    let _access = crate::file_access::read(&path)?;
                    let bytes = std::fs::read(path)?;
                    serde_json::from_slice(&bytes)
                        .map(IoOutcome::Imported)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
                }
                IoJob::Export(path, value) => {
                    let json = serde_json::to_string_pretty(&value)?;
                    let snapshot = crate::editor::Document::new(&json).save_snapshot();
                    crate::files::DiskFile::save_as_unicode(
                        &path,
                        &snapshot,
                        crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
                        crate::files::OverwritePolicy::Confirmed,
                    )?;
                    Ok(IoOutcome::Exported)
                }
            }
        });
        self.udl.task = Some(cx.spawn(async move |owner, cx| {
            let result = worker.await;
            if let Err(error) = owner.update(cx, |view, cx| {
                view.udl.busy = false;
                let mut save_failed = false;
                let same_root = view
                    .settings_store
                    .as_ref()
                    .map(|store| store.root().to_path_buf())
                    == root;
                if generation != view.udl.generation || !same_root {
                    if let Err(error) = result {
                        eprintln!("Stale UDL operation failed: {error}");
                    }
                    view.refresh_udl_registry(cx);
                    return;
                }
                match result {
                    Ok(IoOutcome::Loaded(registry)) => {
                        view.udl.registry = registry;
                        view.udl.needs_style_invalidation = true;
                        view.udl.ready = true;
                        view.udl.source_fields.clear();
                        view.udl.error = None;
                    }
                    Ok(IoOutcome::Imported(value)) => match view.udl.registry.import_json(&value) {
                        Ok(_) => {
                            view.udl.pending_save = true;
                            view.udl.error = None;
                            view.prompt_udl_file_result(false, true, None, cx);
                        }
                        Err(error) => {
                            view.udl.error = Some(Text::resource("matrix-udl-import-rejected", "Import rejected: $ERROR$").replace("$ERROR$", error.to_string()));
                            view.notify_udl_error(cx);
                            view.prompt_udl_file_result(false, false, Some(error.to_string()), cx);
                        }
                    },
                    Ok(IoOutcome::Saved(receipt)) => {
                        match view.udl.registry.acknowledge_save(&receipt) {
                            Ok(()) => {
                                if let Some(previous) = view.udl.error_notice_source.take() {
                                    view.clear_editor_error_source(&previous);
                                }
                                view.udl.error = None;
                                view.udl.message = Some(Text::resource("matrix-udl-definitions-saved", "Definitions saved"));
                            }
                            Err(error) => {
                                save_failed = true;
                                view.udl.error = Some(Text::resource("matrix-udl-error", "User-defined language error: $ERROR$").replace("$ERROR$", error.to_string()));
                                view.notify_udl_error(cx);
                            }
                        }
                    }
                    Ok(IoOutcome::Exported) => {
                        view.prompt_udl_file_result(true, true, None, cx);
                    }
                    Err(error) => {
                        save_failed = saving;
                        view.udl.error = Some(Text::resource("matrix-udl-file-failed", "UDL file operation failed: $ERROR$").replace("$ERROR$", error.to_string()));
                        view.notify_udl_error(cx);
                        if let Some(export) = file_export {
                            view.prompt_udl_file_result(export, false, Some(error.to_string()), cx);
                        }
                    }
                }
                if view.udl.close_requested && save_failed {
                    view.udl.close_failed_revision = Some(view.udl.registry.revision());
                    view.udl.pending_save = false;
                    view.udl.jobs.retain(|job| !matches!(job, IoJob::Save(_)));
                    if let Some(error) = &view.udl.error {
                        eprintln!("UDL exit save failed; existing udl.json was preserved: {}", error.render(&view.native_locale()));
                    }
                }
                if view.udl.pending_save && !view.udl.registry.has_unsaved_changes() {
                    view.udl.pending_save = false;
                }
                view.start_udl_io(cx);
                view.resume_udl_close_if_ready(cx);
                cx.notify();
            }) {
                eprintln!("UDL completion lost its owner: {error}");
            }
        }));
    }

    fn notify_udl_error(&mut self, cx: &mut Context<Self>) {
        if let Some(error) = self.udl.error.clone() {
            self.udl.error_notice_source = Some(error.clone());
            self.notify_localized_editor_error(error, cx);
        }
    }

    fn prompt_udl_file_result(
        &mut self,
        export: bool,
        success: bool,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let (key, fallback) = match (export, success) {
            (false, true) => ("UDL_importSuccessful", "Import successful."),
            (false, false) => ("UDL_importFails", "Failed to import."),
            (true, true) => ("UDL_exportSuccessful", "Export successful."),
            (true, false) => ("UDL_exportFails", "Failed to export."),
        };
        let locale = self.native_locale();
        let title = crate::localization::dialog_title(&locale, key).unwrap_or("User Defined Language");
        let mut message = crate::localization::dialog_message(&locale, key).unwrap_or(fallback).to_owned();
        if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
            message.push_str("\n\n");
            message.push_str(&detail);
        }
        let ok = localized_string(&locale, "common-ok").unwrap_or("OK");
        let root = self.document_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            if !cx.windows().contains(&root) { return; }
            match root.update(cx, |_, window, cx| {
                owner.update(cx, |_, cx| {
                    let level = if success { gpui::PromptLevel::Info } else { gpui::PromptLevel::Warning };
                    let response = window.prompt(level, title, Some(&message), &[gpui::PromptButton::ok(ok)], cx);
                    cx.spawn(async move |_, _| {
                        if let Err(error) = response.await {
                            eprintln!("UDL file result prompt disconnected: {error}");
                        }
                    }).detach();
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => eprintln!("UDL file result prompt lost its owner: {error}"),
            }
        });
    }

    pub(super) fn show_udl(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        if self.udl.visible {
            self.dismiss_floating(floating_windows::DialogKind::Udl, window, cx);
            return;
        }
        self.refresh_udl_registry(cx);
        self.udl.visible = true;
        if self.udl.docked {
            self.udl.focus.focus(window, cx);
        } else {
            self.present_floating(floating_windows::DialogKind::Udl, window, cx);
        }
        cx.notify();
    }

    pub(super) fn invalidate_udl_styles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.invalidate_style_layout(window, cx);
    }

    fn udl_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.udl.error = None;
        self.udl.message = None;
        self.udl.pending_save = true;
        self.invalidate_udl_styles(window, cx);
        self.start_udl_io(cx);
        self.resume_udl_close_if_ready(cx);
    }

    fn apply_udl_control(
        &mut self,
        control: &Control,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_udl_styler_control(control, text, cx) { return; }
        if self.udl.styler_visible { return; }
        if !self.udl.ready {
            return;
        }
        let revision = self.udl.registry.revision();
        match self.udl.registry.edit(self.udl.selected, |definition| {
            control.apply(definition, text)
        }) {
            Ok(()) if self.udl.registry.revision() != revision => self.udl_changed(window, cx),
            Ok(()) => {}
            Err(error) => {
                self.udl.error = Some(Text::resource("matrix-udl-error", "User-defined language error: $ERROR$").replace("$ERROR$", error.to_string()));
                cx.notify();
            }
        }
    }

    fn udl_select(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.udl.styler_visible || self.udl.registry.definition(id).is_none() {
            return;
        }
        self.udl.selected = id;
        self.udl.source_fields.clear();
        self.udl.confirm_remove = false;
        self.udl.error = None;
        let name = self
            .udl
            .registry
            .definition(id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.udl
            .name
            .update(cx, |field, cx| field.set_text(&name, window, cx));
        cx.notify();
    }

    fn udl_named_action(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.udl.styler_visible || !self.udl.ready {
            return;
        }
        let name = self.udl.name.read(cx).text();
        let selected = self.udl.selected;
        let result = match action {
            "Create" => self.udl.registry.create(&name),
            "Save As" => self.udl.registry.save_as(selected, &name),
            "Rename" => self.udl.registry.rename(selected, &name).map(|()| selected),
            "Remove" if !self.udl.confirm_remove => {
                self.udl.confirm_remove = true;
                cx.notify();
                return;
            }
            "Remove" => {
                let previous = self
                    .udl
                    .registry
                    .named()
                    .map(|(id, _)| id)
                    .take_while(|id| *id < selected)
                    .last()
                    .unwrap_or(0);
                self.udl.registry.remove(selected).map(|()| previous)
            }
            _ => return,
        };
        match result {
            Ok(id) => {
                if action == "Remove" {
                    associations::removed(self.workspace.buffers_mut(), selected);
                }
                self.udl_select(id, window, cx);
                self.udl_changed(window, cx);
            }
            Err(error) => {
                self.udl.error = Some(Text::resource("matrix-udl-error", "User-defined language error: $ERROR$").replace("$ERROR$", error.to_string()));
                cx.notify();
            }
        }
    }

    fn udl_file_action(&mut self, export: bool, cx: &mut Context<Self>) {
        if self.udl.styler_visible || !self.udl.ready || self.udl.prompting {
            return;
        }
        let snapshot = if export {
            match self.udl.registry.export_json(self.udl.selected) {
                Ok(value) => Some(value),
                Err(error) => {
                    self.udl.error = Some(Text::resource("matrix-udl-error", "User-defined language error: $ERROR$").replace("$ERROR$", error.to_string()));
                    self.notify_udl_error(cx);
                    self.prompt_udl_file_result(true, false, Some(error.to_string()), cx);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        self.udl.prompting = true;
        let root = self.udl.root.clone();
        let generation = self.udl.generation;
        if let Some(snapshot) = snapshot {
            let directory = root
                .clone()
                .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("."));
            let name = self
                .udl
                .registry
                .definition(self.udl.selected)
                .map(|d| d.name.as_str())
                .unwrap_or("user-defined");
            let suggested = format!("{}.json", name.replace(['/', '\\', ':'], "_"));
            let prompt = cx.prompt_for_new_path(&directory, Some(&suggested));
            self.udl.prompt_task = Some(cx.spawn(async move |owner, cx| {
                let selected = match prompt.await {
                    Ok(Ok(path)) => Ok(path),
                    Ok(Err(error)) => Err(Text::resource("matrix-udl-file-failed", "UDL file operation failed: $ERROR$").replace("$ERROR$", error.to_string())),
                    Err(error) => Err(Text::resource("matrix-udl-save-panel-disconnected", "UDL Save panel disconnected: $ERROR$").replace("$ERROR$", error.to_string())),
                };
                if let Err(error) = owner.update(cx, |view, cx| {
                    view.udl_path_chosen(root, generation, selected, Some(snapshot), cx)
                }) {
                    eprintln!("UDL Save panel lost its owner: {error}");
                }
            }));
        } else {
            let prompt = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(format!("{} (JSON)", crate::localization::dialog_control(&self.native_locale(), "UserDefine", 20015).unwrap_or("Import...")).into()),
            });
            self.udl.prompt_task = Some(cx.spawn(async move |owner, cx| {
                let selected = match prompt.await {
                    Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                    Ok(Err(error)) => Err(Text::resource("matrix-udl-file-failed", "UDL file operation failed: $ERROR$").replace("$ERROR$", error.to_string())),
                    Err(error) => Err(Text::resource("matrix-udl-open-panel-disconnected", "UDL Open panel disconnected: $ERROR$").replace("$ERROR$", error.to_string())),
                };
                if let Err(error) = owner.update(cx, |view, cx| {
                    view.udl_path_chosen(root, generation, selected, None, cx)
                }) {
                    eprintln!("UDL Open panel lost its owner: {error}");
                }
            }));
        }
        cx.notify();
    }

    fn udl_path_chosen(
        &mut self,
        root: Option<PathBuf>,
        generation: u64,
        selected: Result<Option<PathBuf>, Text>,
        snapshot: Option<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        self.udl.prompting = false;
        let current_root = self
            .settings_store
            .as_ref()
            .map(|store| store.root().to_path_buf());
        if root != current_root || generation != self.udl.generation {
            cx.notify();
            return;
        }
        let export = snapshot.is_some();
        match selected {
            Ok(Some(path)) => {
                if let Err(error) = crate::file_access::selected(&path) {
                    self.udl.error = Some(Text::resource("matrix-udl-file-failed", "UDL file operation failed: $ERROR$").replace("$ERROR$", error.to_string()));
                    self.notify_udl_error(cx);
                    self.prompt_udl_file_result(export, false, Some(error.to_string()), cx);
                    cx.notify();
                    return;
                }
                if snapshot.is_none()
                    && !path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                {
                    self.udl.error = Some(Text::resource("matrix-udl-choose-json", "Choose a JSON UDL definition file"));
                    self.notify_udl_error(cx);
                    let detail = self.udl.error.as_ref().map(|error| error.render(&self.native_locale()));
                    self.prompt_udl_file_result(false, false, detail, cx);
                } else {
                    self.udl.jobs.push_back(match snapshot {
                        Some(value) => IoJob::Export(path, value),
                        None => IoJob::Import(path),
                    });
                    self.start_udl_io(cx);
                }
            }
            Ok(None) => {}
            Err(error) => {
                let detail = error.render(&self.native_locale());
                self.udl.error = Some(error);
                self.notify_udl_error(cx);
                self.prompt_udl_file_result(export, false, Some(detail), cx);
            }
        }
        self.resume_udl_close_if_ready(cx);
        cx.notify();
    }

}
