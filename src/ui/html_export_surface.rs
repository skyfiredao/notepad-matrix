use super::*;

impl MatrixWindow {
    pub(super) fn start_html_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.html_export_enabled
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().marked_range().is_some() {
            self.notify_editor_error(
                "HTML export during active composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        let text = self.editor().committed_text();
        let language = self.workspace.active().language();
        let profile = self.editor_style_profile();
        let exported = match language {
            crate::syntax::Language::Builtin(id) => {
                crate::html_export::serialize_builtin_html_with_profile(
                    &text,
                    id,
                    self.dark,
                    4,
                    &profile,
                    self.editor_zoom_delta(),
                )
            }
            crate::syntax::Language::UserDefined(id) => match self.udl.registry.definition(id) {
                Some(definition) => crate::html_export::serialize_udl_html_with_profile(
                    &text,
                    definition,
                    4,
                    &profile,
                    self.editor_zoom_delta(),
                ),
                None => Err(crate::html_export::HtmlExportError::MissingDefinition(id)),
            },
            _ => crate::html_export::serialize_html_with_profile(
                &text, language, self.dark, 4, &profile,
            ),
        };
        let html = match exported {
            Ok(html) => html,
            Err(error) => {
                self.notify_editor_error(error.to_string(), cx);
                return;
            }
        };
        let snapshot = crate::editor::Document::new(&html).save_snapshot();
        let directory = self
            .disk_file()
            .and_then(|file| file.path().parent())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = format!("{}.html", self.workspace.active().name());
        let prompt = cx.prompt_for_new_path(&directory, Some(&suggested));
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let selected = match prompt.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(format!(
                    "HTML export path chooser disconnected: {error}"
                ))),
            };
            let result = match selected {
                Ok(Some(path)) => {
                    executor
                        .spawn(async move {
                            crate::files::DiskFile::save_as_unicode(
                                &path,
                                &snapshot,
                                crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
                                crate::files::OverwritePolicy::Confirmed,
                            )
                            .map(Some)
                        })
                        .await
                }
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match result {
                    Ok(Some(_)) => view.editor_error = None,
                    Ok(None) => {}
                    Err(error) => {
                        view.notify_editor_error(format!("HTML export failed: {error}"), cx)
                    }
                }
                view.editor_focus.focus(window, cx);
                cx.notify();
            }) {
                eprintln!("HTML export completion could not reach its window: {error}");
            }
        })
        .detach();
    }
}
