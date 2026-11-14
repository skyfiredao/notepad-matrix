use super::*;
use crate::udl_controls::{self, Control, Kind, Page, STYLE_NAMES};
use crate::udl_registry::{SaveReceipt, UdlRegistry};
use std::collections::{BTreeMap, VecDeque};
#[path = "../udl_association.rs"]
mod associations;

struct BoundField {
    field: gpui::Entity<text_field::TextField>,
    _changed: gpui::Subscription,
}

#[cfg(all(test, feature = "native-input-tests"))]
mod tests {
    use super::*;

    #[gpui::test]
    fn main_close_drains_in_flight_save_and_latest_edit_before_owner_release(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("close/restart home");
        let store = ConfigStore::under_home(home.path());
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, _, cx| {
            view.settings_store = Some(ConfigStore::under_home(home.path()));
            view.refresh_udl_registry(cx);
        })
        .expect("isolated registry");
        cx.run_until_parked();
        finish_io(cx, main);
        let id = main
            .update(cx, |view, window, cx| {
                let id = view
                    .udl
                    .registry
                    .create("Retained on exit")
                    .expect("first edit");
                view.udl_changed(window, cx);
                assert!(view.udl.busy, "first snapshot is in flight");
                view.udl
                    .registry
                    .edit(id, |d| {
                        d.keywords[7] = "latest edit before quit".into();
                        Ok(())
                    })
                    .expect("second edit");
                view.udl_changed(window, cx);
                assert!(view.udl.pending_save, "latest snapshot is coalesced");
                view.request_native_quit(window, cx);
                assert!(
                    view.application_quit_pending,
                    "flush must retain the original app-quit intent"
                );
                assert!(
                    view.udl.close_requested,
                    "main close awaits persistence, not owner release"
                );
                id
            })
            .expect("close with queued latest edit");
        assert!(cx.windows().contains(&main.into()));
        cx.run_until_parked();
        assert!(
            !cx.windows().contains(&main.into()),
            "main close did not resume after draining"
        );
        let restarted = UdlRegistry::load(&store).expect("restart registry");
        assert_eq!(
            restarted
                .definition(id)
                .expect("retained definition")
                .keywords[7],
            "latest edit before quit"
        );
    }

    #[gpui::test]
    fn exit_save_conflict_preserves_external_file_and_does_not_retry_or_refuse_close(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("exit conflict home");
        let store = ConfigStore::under_home(home.path());
        let mut initial = UdlRegistry::default();
        let id = initial.create("Existing").expect("existing definition");
        initial.save(&store).expect("initial disk registry");
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, _, cx| {
            view.settings_store = Some(ConfigStore::under_home(home.path()));
            view.refresh_udl_registry(cx);
        })
        .expect("isolated conflict registry");
        cx.run_until_parked();
        finish_io(cx, main);
        let mut external = initial.to_json();
        external["external"] = "preserve on failed exit".into();
        let bytes = serde_json::to_vec_pretty(&external).expect("external snapshot");
        std::fs::write(store.root().join("udl.json"), &bytes).expect("external writer");
        main.update(cx, |view, window, cx| {
            view.udl
                .registry
                .edit(id, |d| {
                    d.keywords[0] = "unsaved conflict".into();
                    Ok(())
                })
                .expect("draft edit");
            view.udl_changed(window, cx);
        })
        .expect("close after conflicting snapshot");
        assert!(
            !gpui::VisualTestContext::from_window(main.into(), cx).simulate_close(),
            "clean-document native close must await the UDL worker"
        );
        cx.run_until_parked();
        assert!(
            !cx.windows().contains(&main.into()),
            "source exit must not enter a retry/quit-refusal loop"
        );
        assert_eq!(
            std::fs::read(store.root().join("udl.json")).expect("old file"),
            bytes
        );
    }

    fn finish_io(cx: &mut gpui::TestAppContext, main: gpui::WindowHandle<MatrixWindow>) {
        let task = main
            .update(cx, |view, _, _| view.udl.task.take())
            .expect("UDL task owner")
            .expect("UDL file task");
        cx.executor().allow_parking();
        cx.foreground_executor().clone().block_test(task);
        cx.executor().forbid_parking();
        cx.run_until_parked();
    }

    #[gpui::test]
    fn native_import_panel_selects_json_and_cancellation_does_not_change_definitions(
        cx: &mut gpui::TestAppContext,
    ) {
        let home = tempfile::tempdir().expect("import home");
        let path = home.path().join("language.json");
        std::fs::write(&path, br#"{"name":"Imported","keywords":["one","two","three","four","five","six","seven","eight"]}"#).expect("import fixture");
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, _, cx| {
            view.settings_store = None;
            view.refresh_udl_registry(cx);
            view.udl_file_action(false, cx);
        })
        .expect("native import");
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|options| {
            assert!(options.files);
            assert!(!options.directories);
            assert!(!options.multiple);
            None
        });
        cx.run_until_parked();
        main.read_with(cx, |view, _| {
            assert_eq!(view.udl.registry.named().count(), 0)
        })
        .expect("cancel unchanged");
        main.update(cx, |view, _, cx| view.udl_file_action(false, cx))
            .expect("choose JSON");
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        finish_io(cx, main);
        main.read_with(cx, |view, _| {
            let (_, definition) = view
                .udl
                .registry
                .named()
                .next()
                .expect("actual imported definition");
            assert_eq!(definition.name, "Imported");
            assert_eq!(definition.keywords[7], "eight");
        })
        .expect("imported JSON");
    }

    #[gpui::test]
    fn native_export_save_panel_replaces_confirmed_existing_path(cx: &mut gpui::TestAppContext) {
        let home = tempfile::tempdir().expect("export home");
        let path = home.path().join("existing.json");
        std::fs::write(&path, b"old content").expect("existing export fixture");
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, _, cx| {
            view.settings_store = None;
            view.refresh_udl_registry(cx);
            view.udl.selected = view
                .udl
                .registry
                .create("Exported")
                .expect("exported definition");
            view.udl_file_action(true, cx);
        })
        .expect("native Save panel");
        assert!(cx.did_prompt_for_new_path());
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        finish_io(cx, main);
        let exported: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("replaced export"))
                .expect("JSON export");
        assert_eq!(exported["definitions"][0]["name"], "Exported");
        main.read_with(cx, |view, _| {
            assert_eq!(
                view.workspace.active().language(),
                crate::syntax::Language::NormalText
            );
            assert_eq!(view.editor().committed_text(), "");
        })
        .expect("export does not save or retarget document");
    }

    #[gpui::test]
    fn definition_and_styler_are_independent_windows_without_language_conversion(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("tab", DialogFocusNext, Some("MatrixAuxiliary")),
                KeyBinding::new("shift-tab", DialogFocusPrevious, Some("MatrixAuxiliary")),
                KeyBinding::new("escape", CloseWindow, Some("MatrixAuxiliary")),
            ])
        });
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, _, _| {
            view.settings_store = None;
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Json);
        })
        .expect("document fixture");
        let mut document = gpui::VisualTestContext::from_window(main.into(), cx);
        let toolbar = document
            .debug_bounds("toolbar-define-user-language")
            .expect("UDL toolbar");
        document.simulate_click(toolbar.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2, "UDL must be a real floating window");
        let definition_window = cx
            .windows()
            .into_iter()
            .find(|handle| *handle != main.into())
            .expect("definition window");
        let mut definition = gpui::VisualTestContext::from_window(definition_window, cx);
        let registry = main
            .read_with(cx, |view, _| {
                (view.udl.registry.revision(), view.udl.pending_save)
            })
            .expect("registry snapshot");
        definition.update(|window, cx| {
            main.read_with(cx, |view, cx| {
                assert!(view.udl.focus.contains_focused(window, cx))
            })
            .expect("UDL initial focus");
        });
        for key in ["tab", "shift-tab"] {
            let before = definition.update(|window, cx| window.focused(cx));
            definition.simulate_keystrokes(key);
            assert_ne!(
                definition.update(|window, cx| window.focused(cx)),
                before,
                "UDL traversal {key}"
            );
            definition.update(|window, cx| {
                main.read_with(cx, |view, cx| {
                    assert!(view.udl.focus.contains_focused(window, cx));
                    assert_eq!(
                        (view.udl.registry.revision(), view.udl.pending_save),
                        registry
                    );
                })
                .expect("navigation stays within unchanged UDL");
            });
        }
        main.update(cx, |view, window, cx| {
            view.udl.styler_visible = true;
            view.present_floating(floating_windows::DialogKind::UdlStyler, window, cx);
        })
        .expect("open independent Styler");
        cx.run_until_parked();
        assert_eq!(
            cx.windows().len(),
            3,
            "Styler must not overlay the definition page"
        );
        main.read_with(cx, |view, _| {
            assert_eq!(
                view.workspace.active().language(),
                crate::syntax::Language::Json
            );
            assert_eq!(view.editor().committed_text(), "");
        })
        .expect("unchanged document");
        let styler = cx
            .windows()
            .into_iter()
            .find(|handle| *handle != main.into() && *handle != definition_window)
            .expect("Styler window");
        cx.dispatch_action(styler, CloseWindow);
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
        main.read_with(cx, |view, _| assert!(view.udl.visible))
            .expect("definition remains open");
        let mut close = definition.debug_bounds("udl-close").expect("UDL Close");
        let viewport = definition.update(|window, _| window.viewport_size());
        if close.bottom() > viewport.height {
            definition.simulate_event(gpui::ScrollEvent {
                position: gpui::point(viewport.width - px(2.), viewport.height / 2.),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), -close.bottom())),
                ..Default::default()
            });
            close = definition
                .debug_bounds("udl-close")
                .expect("visible UDL Close");
        }
        assert!(close.top() >= px(0.) && close.bottom() <= viewport.height);
        definition.simulate_click(close.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(cx.windows(), vec![main.into()]);
        document.simulate_click(toolbar.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        let reopened = cx
            .windows()
            .into_iter()
            .find(|handle| *handle != main.into())
            .expect("reopened UDL");
        gpui::VisualTestContext::from_window(reopened, cx).simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(cx.windows(), vec![main.into()]);
        main.update(cx, |view, window, _| {
            assert!(view.editor_focus.is_focused(window));
            assert_eq!(
                view.workspace.active().language(),
                crate::syntax::Language::Json
            );
            assert_eq!(view.editor().committed_text(), "");
            assert!(!view.editor().document().can_undo());
            assert_eq!(
                (view.udl.registry.revision(), view.udl.pending_save),
                registry
            );
        })
        .expect("closing restores original unchanged document");
        document.simulate_input("!");
        main.read_with(cx, |view, _| {
            assert_eq!(view.editor().committed_text(), "!")
        })
        .expect("post-dismissal editor input");
    }

    #[gpui::test]
    fn eighth_keyword_field_updates_live_without_selecting_udl(cx: &mut gpui::TestAppContext) {
        let main = cx.add_window(|window, cx| MatrixWindow::new(false, window, cx));
        cx.run_until_parked();
        main.update(cx, |view, window, cx| {
            view.settings_store = None;
            view.udl.page = Page::Keywords;
            view.show_udl(window, cx);
        })
        .expect("show Keywords Lists");
        cx.run_until_parked();
        main.update(cx, |view, window, cx| {
            let field = view
                .udl
                .fields
                .iter()
                .find(|(key, _)| key.starts_with("0:/keywords/7:"))
                .expect("actual eighth keyword text input")
                .1
                .field
                .clone();
            field.update(cx, |field, cx| {
                field.set_text("eighth source keyword", window, cx)
            });
        })
        .expect("edit actual keyword field");
        cx.run_until_parked();
        main.read_with(cx, |view, _| {
            assert_eq!(
                view.udl
                    .registry
                    .definition(0)
                    .expect("working definition")
                    .keywords[7],
                "eighth source keyword"
            );
            assert_eq!(
                view.workspace.active().language(),
                crate::syntax::Language::NormalText
            );
        })
        .expect("live definition change without document conversion");
    }
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
    pub focus: FocusHandle,
    pub styler_focus: FocusHandle,
    selected: u64,
    page: Page,
    style: usize,
    fields: BTreeMap<String, BoundField>,
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
    error: Option<String>,
    message: Option<String>,
    confirm_remove: bool,
    menu_languages: Option<Vec<(u64, String)>>,
    missing: Vec<(crate::workspace::BufferId, u64)>,
    missing_warning: Option<String>,
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
            focus: cx.focus_handle(),
            styler_focus: cx.focus_handle(),
            selected: 0,
            page: Page::FolderDefault,
            style: 0,
            fields: BTreeMap::new(),
            name: cx.new(|cx| text_field::TextField::new("Language name", "udl-name", cx)),
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
    pub(super) fn refresh_udl_registry(&mut self, cx: &mut Context<Self>) {
        let root = self
            .settings_store
            .as_ref()
            .map(|store| store.root().to_path_buf());
        if self.udl.root != root {
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
            self.udl.fields.clear();
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
            if result.missing != self.udl.missing {
                self.udl.missing = result.missing;
                if self.editor_error == self.udl.missing_warning {
                    self.editor_error = None;
                }
                self.udl.missing_warning = if self.udl.missing.is_empty() {
                    None
                } else {
                    Some(format!(
                        "Missing user-defined language ID(s): {}. Document text and saved language identities were preserved. Restore the definitions or explicitly select another Language.",
                        self.udl
                            .missing
                            .iter()
                            .map(|(_, id)| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                };
                if let Some(error) = self.udl.missing_warning.clone() {
                    self.notify_editor_error(error, cx);
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

    fn udl_work_pending(&self) -> bool {
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
        if self.udl.busy {
            return;
        }
        let root = self.udl.root.clone();
        let home = root
            .as_ref()
            .and_then(|root| root.parent())
            .map(PathBuf::from);
        if home.is_none() && self.udl.pending_save {
            self.udl.pending_save = false;
            self.udl.message =
                Some("Definitions are in memory only (no configuration directory)".into());
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
                        view.udl.fields.clear();
                        view.udl.error = None;
                    }
                    Ok(IoOutcome::Imported(value)) => match view.udl.registry.import_json(&value) {
                        Ok(ids) => {
                            view.udl.pending_save = true;
                            view.udl.error = None;
                            view.udl.message = Some(format!("Imported {} language(s)", ids.len()));
                        }
                        Err(error) => view.udl.error = Some(format!("Import rejected: {error}")),
                    },
                    Ok(IoOutcome::Saved(receipt)) => {
                        match view.udl.registry.acknowledge_save(&receipt) {
                            Ok(()) => {
                                if view.editor_error == view.udl.error {
                                    view.editor_error = None;
                                }
                                view.udl.error = None;
                                view.udl.message = Some("Definitions saved".into());
                            }
                            Err(error) => {
                                save_failed = true;
                                let message = error.to_string();
                                view.udl.error = Some(message.clone());
                                view.notify_editor_error(message, cx);
                            }
                        }
                    }
                    Ok(IoOutcome::Exported) => {
                        view.udl.message = Some("Definition exported".into())
                    }
                    Err(error) => {
                        save_failed = saving;
                        let message = format!("UDL file operation failed: {error}");
                        view.udl.error = Some(message.clone());
                        view.notify_editor_error(message, cx);
                    }
                }
                if view.udl.close_requested && save_failed {
                    view.udl.close_failed_revision = Some(view.udl.registry.revision());
                    view.udl.pending_save = false;
                    view.udl.jobs.retain(|job| !matches!(job, IoJob::Save(_)));
                    if let Some(error) = &view.udl.error {
                        eprintln!("UDL exit save failed; existing udl.json was preserved: {error}");
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
        self.present_floating(floating_windows::DialogKind::Udl, window, cx);
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
                self.udl.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn udl_select(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.udl.registry.definition(id).is_none() {
            return;
        }
        self.udl.selected = id;
        self.udl.fields.clear();
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
        if !self.udl.ready {
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
                self.udl.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn udl_file_action(&mut self, export: bool, cx: &mut Context<Self>) {
        if !self.udl.ready || self.udl.prompting {
            return;
        }
        let snapshot = if export {
            match self.udl.registry.export_json(self.udl.selected) {
                Ok(value) => Some(value),
                Err(error) => {
                    self.udl.error = Some(error.to_string());
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
                    Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                    Err(error) => Err(io::Error::other(format!(
                        "UDL Save panel disconnected: {error}"
                    ))),
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
                prompt: Some("Import User Defined Language JSON".into()),
            });
            self.udl.prompt_task = Some(cx.spawn(async move |owner, cx| {
                let selected = match prompt.await {
                    Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                    Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                    Err(error) => Err(io::Error::other(format!(
                        "UDL Open panel disconnected: {error}"
                    ))),
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
        selected: io::Result<Option<PathBuf>>,
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
        match selected {
            Ok(Some(path)) => {
                if snapshot.is_none()
                    && !path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                {
                    self.udl.error = Some("Choose a JSON UDL definition file".into());
                } else {
                    self.udl.jobs.push_back(match snapshot {
                        Some(value) => IoJob::Export(path, value),
                        None => IoJob::Import(path),
                    });
                    self.start_udl_io(cx);
                }
            }
            Ok(None) => {}
            Err(error) => self.udl.error = Some(error.to_string()),
        }
        self.resume_udl_close_if_ready(cx);
        cx.notify();
    }

    fn render_udl_control(
        &mut self,
        control: Control,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = self
            .udl
            .registry
            .definition(self.udl.selected)
            .map(|d| control.text(d))
            .unwrap_or_default();
        let key = format!("{}:{}:{}", self.udl.selected, control.path, control.label);
        let mut row = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .w_full()
            .child(control.label);
        match control.kind.clone() {
            Kind::Boolean | Kind::Bit(_) => {
                let enabled = text == "true";
                row = div().flex().flex_col().child(
                    div()
                        .id(gpui::SharedString::from(key))
                        .role(Role::CheckBox)
                        .aria_label(control.label)
                        .aria_toggled(enabled.into())
                        .tab_index(0)
                        .cursor_pointer()
                        .child(format!(
                            "{} {}",
                            if enabled { "☑" } else { "☐" },
                            control.label
                        ))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.apply_udl_control(
                                &control,
                                if enabled { "false" } else { "true" },
                                window,
                                cx,
                            );
                        })),
                );
            }
            Kind::Choice(options) => {
                let mut choices = div().flex().gap(px(8.));
                for (index, label) in options.iter().enumerate() {
                    let chosen = text == index.to_string();
                    let control = control.clone();
                    choices = choices.child(
                        div()
                            .id(gpui::SharedString::from(format!("{key}-{index}")))
                            .role(Role::RadioButton)
                            .aria_label(*label)
                            .aria_toggled(chosen.into())
                            .tab_index(0)
                            .cursor_pointer()
                            .child(format!("{} {label}", if chosen { "◉" } else { "○" }))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.apply_udl_control(&control, &index.to_string(), window, cx)
                            })),
                    );
                }
                row = row.child(choices);
            }
            _ => {
                if !self.udl.fields.contains_key(&key) {
                    let field = cx.new(|cx| {
                        text_field::TextField::new(control.label, "udl-property-input", cx)
                    });
                    field.update(cx, |field, cx| field.set_text(&text, window, cx));
                    let selected = self.udl.selected;
                    let changed = cx.observe_in(&field, window, move |view, field, window, cx| {
                        if view.udl.selected != selected || field.read(cx).has_composition() {
                            return;
                        }
                        let text = field.read(cx).text();
                        let same = view
                            .udl
                            .registry
                            .definition(selected)
                            .is_some_and(|d| control.text(d) == text);
                        if !same {
                            view.apply_udl_control(&control, &text, window, cx);
                        }
                    });
                    self.udl.fields.insert(
                        key.clone(),
                        BoundField {
                            field,
                            _changed: changed,
                        },
                    );
                }
                if let Some(field) = self.udl.fields.get(&key) {
                    row = row.child(field.field.clone());
                }
            }
        }
        row.into_any_element()
    }

    fn render_udl_groups(
        &mut self,
        fields: Vec<Control>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div().flex().flex_col().gap(px(8.));
        let mut previous = String::new();
        for field in fields {
            if field.group != previous {
                previous = field.group.clone();
                let mut heading = div()
                    .flex()
                    .gap(px(12.))
                    .mt(px(8.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(previous.clone());
                let style = match previous.as_str() {
                    "Default" => Some(0),
                    "Block comments" => Some(1),
                    "Line comments" => Some(2),
                    "Numbers" => Some(3),
                    "Operators" => Some(12),
                    "Folding in code 1 (forward)" => Some(13),
                    "Folding in code 2 (words)" => Some(14),
                    "Folding in comments" => Some(15),
                    name => name
                        .strip_prefix("Keyword group ")
                        .and_then(|n| n.parse::<usize>().ok())
                        .filter(|n| (1..=8).contains(n))
                        .map(|n| n + 3)
                        .or_else(|| {
                            name.strip_prefix("Delimiter ")
                                .and_then(|n| n.parse::<usize>().ok())
                                .filter(|n| (1..=8).contains(n))
                                .map(|n| n + 15)
                        }),
                };
                if let Some(style) = style {
                    heading = heading.child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "udl-group-styler-{style}"
                            )))
                            .role(Role::Button)
                            .aria_label(format!("Styler for {}", previous))
                            .tab_index(0)
                            .border_1()
                            .px(px(6.))
                            .cursor_pointer()
                            .child("Styler…")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.udl.style = style;
                                view.udl.styler_visible = true;
                                view.present_floating(
                                    floating_windows::DialogKind::UdlStyler,
                                    window,
                                    cx,
                                );
                                cx.notify();
                            })),
                    );
                }
                body = body.child(heading);
            }
            body = body.child(self.render_udl_control(field, window, cx));
        }
        body.into_any_element()
    }

    pub(super) fn render_udl(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.udl.visible {
            return None;
        }
        let mut panel = div()
            .id("udl-definition-dialog")
            .role(Role::Dialog)
            .aria_label("User Defined Language")
            .track_focus(&self.udl.focus)
            .w_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.));
        panel = panel.child(
            "User Defined Language — edits apply live only to documents using this language",
        );
        let mut languages = div().flex().flex_wrap().gap(px(6.));
        let entries = std::iter::once((0, "User Defined (working copy)".to_owned()))
            .chain(
                self.udl
                    .registry
                    .named()
                    .map(|(id, d)| (id, d.name.clone())),
            )
            .collect::<Vec<_>>();
        for (id, name) in entries {
            let selected = self.udl.selected == id;
            languages = languages.child(
                div()
                    .id(gpui::SharedString::from(format!("udl-select-{id}")))
                    .role(Role::Button)
                    .aria_label(name.clone())
                    .tab_index(0)
                    .border_1()
                    .px(px(6.))
                    .py(px(3.))
                    .bg(rgb(if selected { 0xcddff5 } else { 0xffffff }))
                    .cursor_pointer()
                    .child(name)
                    .on_click(
                        cx.listener(move |view, _, window, cx| view.udl_select(id, window, cx)),
                    ),
            );
        }
        panel = panel
            .child(languages)
            .child("Language name (for Create / Save As / Rename)")
            .child(self.udl.name.clone());
        let mut actions = div().flex().gap(px(8.));
        for label in ["Create", "Save As", "Rename", "Remove"] {
            if self.udl.selected == 0 && matches!(label, "Rename" | "Remove") {
                continue;
            }
            actions = actions.child(
                div()
                    .id(label)
                    .role(Role::Button)
                    .aria_label(label)
                    .tab_index(0)
                    .border_1()
                    .px(px(8.))
                    .py(px(3.))
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.udl_named_action(label, window, cx)
                    })),
            );
        }
        panel = panel.child(actions);
        if self.udl.confirm_remove {
            panel = panel.child("Remove this named definition? Existing documents keep their text and switch to the unnamed User-Defined language.")
                .child(div().id("udl-remove-confirm").role(Role::Button).aria_label("Confirm remove").child("Confirm remove")
                    .on_click(cx.listener(|view, _, window, cx| view.udl_named_action("Remove", window, cx))))
                .child(div().id("udl-remove-cancel").role(Role::Button).aria_label("Cancel remove").child("Cancel")
                    .on_click(cx.listener(|view, _, _, cx| { view.udl.confirm_remove = false; cx.notify(); })));
        }
        if self.udl.ready {
            if self.udl.selected != 0 {
                panel = panel.child(self.render_udl_control(
                    udl_controls::extensions_control(),
                    window,
                    cx,
                ));
            }
            let mut tabs = div().flex().flex_wrap().gap(px(8.));
            for page in Page::ALL {
                tabs = tabs.child(
                    div()
                        .id(page.label())
                        .role(Role::Button)
                        .aria_label(page.label())
                        .tab_index(0)
                        .border_1()
                        .px(px(8.))
                        .py(px(4.))
                        .bg(rgb(if page == self.udl.page {
                            0xcddff5
                        } else {
                            0xffffff
                        }))
                        .cursor_pointer()
                        .child(page.label())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.udl.page = page;
                            cx.notify();
                        })),
                );
            }
            panel = panel.child(tabs).child(self.render_udl_groups(
                udl_controls::controls(self.udl.page),
                window,
                cx,
            ));
            panel = panel.child(
                div()
                    .id("udl-open-styler")
                    .role(Role::Button)
                    .aria_label("Styler")
                    .tab_index(0)
                    .border_1()
                    .px(px(8.))
                    .py(px(4.))
                    .cursor_pointer()
                    .child("Styler…")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.udl.styler_visible = true;
                        view.present_floating(floating_windows::DialogKind::UdlStyler, window, cx);
                        cx.notify();
                    })),
            );
        } else {
            panel = panel.child(if self.udl.busy {
                "Loading definitions…"
            } else {
                "Definitions unavailable; correct udl.json and retry."
            });
        }
        for (label, export) in [("Import JSON", false), ("Export JSON", true)] {
            panel = panel.child(
                div()
                    .id(label)
                    .role(Role::Button)
                    .aria_label(label)
                    .tab_index(0)
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |view, _, _, cx| view.udl_file_action(export, cx))),
            );
        }
        if let Some(error) = &self.udl.error {
            panel = panel.child(div().text_color(rgb(0xa02020)).child(error.clone()));
            panel = panel.child(
                div()
                    .id("udl-retry")
                    .role(Role::Button)
                    .aria_label("Retry UDL file operation")
                    .child("Retry")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.udl.error = None;
                        if view.udl.ready {
                            view.udl.pending_save = true;
                            view.start_udl_io(cx);
                        } else {
                            view.udl.load_attempted = false;
                            view.refresh_udl_registry(cx);
                        }
                        cx.notify();
                    })),
            );
        }
        if let Some(message) = &self.udl.message {
            panel = panel.child(message.clone());
        }
        if self.udl.busy || self.udl.pending_save {
            panel = panel.child("File operation in progress…");
        }
        panel = panel.child(
            div()
                .id("udl-close")
                .debug_selector(|| "udl-close".into())
                .role(Role::Button)
                .aria_label("Close User Defined Language")
                .tab_index(0)
                .child("Close")
                .on_click(cx.listener(|view, _, window, cx| {
                    view.dismiss_floating(floating_windows::DialogKind::Udl, window, cx)
                })),
        );
        Some(panel.into_any_element())
    }

    pub(super) fn render_udl_styler(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.udl.styler_visible {
            return None;
        }
        let mut panel = div()
            .id("udl-styler-dialog")
            .role(Role::Dialog)
            .aria_label("UDL Styler")
            .track_focus(&self.udl.styler_focus)
            .w_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.));
        let name = self
            .udl
            .registry
            .definition(self.udl.selected)
            .map(|d| d.name.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("User Defined (working copy)");
        panel = panel.child(format!("Styler — {name}"));
        let mut styles = div().flex().flex_wrap().gap(px(5.));
        for (index, name) in STYLE_NAMES.into_iter().enumerate() {
            styles = styles.child(
                div()
                    .id(gpui::SharedString::from(format!("udl-style-{index}")))
                    .role(Role::Button)
                    .aria_label(name)
                    .tab_index(0)
                    .border_1()
                    .px(px(5.))
                    .py(px(3.))
                    .bg(rgb(if self.udl.style == index {
                        0xcddff5
                    } else {
                        0xffffff
                    }))
                    .cursor_pointer()
                    .child(name)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.udl.style = index;
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(styles);
        if self.udl.ready {
            panel = panel.child(self.render_udl_groups(
                udl_controls::style_controls(self.udl.style),
                window,
                cx,
            ));
        }
        if let Some(error) = &self.udl.error {
            panel = panel.child(div().text_color(rgb(0xa02020)).child(error.clone()));
        }
        panel = panel.child(
            div()
                .id("udl-styler-close")
                .role(Role::Button)
                .aria_label("Close Styler")
                .tab_index(0)
                .child("Close")
                .on_click(cx.listener(|view, _, window, cx| {
                    view.dismiss_floating(floating_windows::DialogKind::UdlStyler, window, cx)
                })),
        );
        Some(panel.into_any_element())
    }
}
