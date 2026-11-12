use super::*;

pub(super) struct UdlSyntaxCache {
    revision: crate::input::DisplayRevision,
    id: u64,
    definition_revision: u64,
    definition: crate::udl::Definition,
    lexed: crate::udl::LexResult,
    text_system: Option<std::rc::Rc<gpui::WindowTextSystem>>,
}

impl MatrixWindow {
    pub(super) fn select_language(
        &mut self,
        language: crate::syntax::Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        if self.editor().has_composition() {
            self.notify_editor_error(
                "Language changes during active composition are not implemented yet".into(),
                cx,
            );
            return;
        }
        self.workspace.active_mut().set_language(language);
        self.refresh_editor_syntax();
        self.capture_udl_text_system(window);
        self.invalidate_style_layout(window, cx);
        self.editor_preferred_x = None;
        window.notify_text_input_changed(&self.editor_focus);
        self.editor_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn refresh_editor_syntax(&mut self) {
        let builtin = self.refresh_builtin_syntax();
        if builtin
            && matches!(
                self.workspace.active().language(),
                crate::syntax::Language::Builtin(_)
            )
        {
            self.udl.syntax_cache = None;
            self.syntax_cache = None;
            return;
        }
        let language = self.workspace.active().language();
        let revision = self.editor().display_revision();
        if self.editor_error.as_ref().is_some_and(|error| {
            error.starts_with("User Defined language ")
                && error.ends_with("displaying default styling")
        }) {
            self.editor_error = None;
        }
        if let crate::syntax::Language::UserDefined(id) = language {
            let definition_revision = self.udl.registry.definition_revision(id).unwrap_or(0);
            let Some(definition) = self.udl.registry.definition(id) else {
                self.udl.syntax_cache = None;
                self.syntax_cache = Some((revision, language, Vec::new()));
                self.editor_error = Some(format!(
                    "User Defined language {id} is unavailable; its identity and document are retained, displaying default styling"
                ));
                return;
            };
            if self.udl.syntax_cache.as_ref().is_some_and(|cache| {
                cache.revision == revision
                    && cache.id == id
                    && cache.definition_revision == definition_revision
                    && cache.definition == *definition
            }) {
                return;
            }
            let definition = definition.clone();
            let text_system = self
                .udl
                .syntax_cache
                .as_ref()
                .and_then(|cache| cache.text_system.clone());
            let text = self.editor().display_text();
            let lexed = crate::udl::lex(&text, &definition);
            self.udl.syntax_cache = Some(UdlSyntaxCache {
                revision: revision.clone(),
                id,
                definition_revision,
                definition,
                lexed,
                text_system,
            });
            self.syntax_cache = Some((revision, language, Vec::new()));
            self.editor_painted_revision = None;
            return;
        }
        self.udl.syntax_cache = None;
        if self
            .syntax_cache
            .as_ref()
            .is_some_and(|(cached, lang, _)| cached == &revision && *lang == language)
        {
            return;
        }
        let spans = if language == crate::syntax::Language::Json {
            if let Some((_, text)) = self
                .editor_accessibility
                .as_ref()
                .filter(|(cached, _)| cached == &revision)
            {
                crate::syntax::json_styles(text)
            } else {
                crate::syntax::json_styles(&self.editor().display_text())
            }
        } else {
            Vec::new()
        };
        self.syntax_cache = Some((revision, language, spans));
    }

    #[cfg(all(test, feature = "native-input-tests"))]
    pub(super) fn styled_editor_runs(
        &self,
        start: usize,
        end: usize,
        marked: Option<&std::ops::Range<ByteOffset>>,
        base: gpui::TextRun,
    ) -> Vec<gpui::TextRun> {
        self.editor_style_runs(start, end, marked, base)
            .into_iter()
            .map(|(run, _)| run)
            .collect()
    }

    pub(super) fn editor_style_runs(
        &self,
        start: usize,
        end: usize,
        marked: Option<&std::ops::Range<ByteOffset>>,
        base: gpui::TextRun,
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        if matches!(
            self.workspace.active().language(),
            crate::syntax::Language::Builtin(_)
        ) {
            return self.builtin_style_runs(start, end, marked, base);
        }
        let links = self.visible_editor_link_spans(start..end);
        let default_foreground = base.color;
        let mut runs = Vec::new();
        let default_size = self.editor_font_size();
        let profile = self.editor_style_profile();
        let mut marked_ranges = self.editor().marked_ranges();
        if let Some(marked) = marked {
            marked_ranges.push(marked.clone());
        }
        let mut push = |a: usize, b: usize, mut run: gpui::TextRun, size: gpui::Pixels| {
            let mut cuts = vec![a, b];
            for marked in &marked_ranges {
                cuts.extend([marked.start.0.max(a).min(b), marked.end.0.max(a).min(b)]);
            }
            for link in &links {
                cuts.extend([link.start.max(a).min(b), link.end.max(a).min(b)]);
            }
            cuts.sort_unstable();
            cuts.dedup();
            let syntax_underline = run.underline;
            for pair in cuts.windows(2) {
                run.len = pair[1] - pair[0];
                let marked_underline = marked_ranges
                    .iter()
                    .any(|mark| pair[0] >= mark.start.0 && pair[1] <= mark.end.0)
                    .then_some(gpui::UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    });
                run.underline = marked_underline
                    .or_else(|| {
                        links
                            .iter()
                            .any(|link| pair[0] >= link.start && pair[1] <= link.end)
                            .then_some(gpui::UnderlineStyle {
                                color: Some(default_foreground),
                                thickness: px(1.),
                                wavy: false,
                            })
                    })
                    .or(syntax_underline);
                runs.push((run.clone(), size));
            }
        };
        if let Some((_, crate::syntax::Language::Json, spans)) = &self.syntax_cache {
            let first = spans.partition_point(|span| span.range.end <= start);
            for span in spans[first..]
                .iter()
                .take_while(|span| span.range.start < end)
            {
                let (fg, bg, bold) = span.style.appearance(self.dark);
                let mut run = base.clone();
                run.color = rgb(fg).into();
                run.background_color = Some(rgb(bg).into());
                if bold {
                    run.font = run.font.bold();
                }
                push(
                    span.range.start.max(start),
                    span.range.end.min(end),
                    run,
                    default_size,
                );
            }
        } else if let Some(cache) = self.udl.syntax_cache.as_ref().filter(|cache| {
            self.workspace.active().language() == crate::syntax::Language::UserDefined(cache.id)
        }) {
            let spans = &cache.lexed.spans;
            let first = spans.partition_point(|span| span.range.end <= start);
            for span in spans[first..]
                .iter()
                .take_while(|span| span.range.start < end)
            {
                let appearance = crate::syntax::udl_appearance_with_profile(
                    &cache.definition,
                    span.style,
                    &profile,
                    self.editor_zoom_delta(),
                );
                let mut run = base.clone();
                run.color = rgb(appearance.foreground).into();
                run.background_color = Some(rgb(appearance.background).into());
                run.font = gpui::font(appearance.font_family);
                if appearance.bold {
                    run.font = run.font.bold();
                }
                if appearance.italic {
                    run.font = run.font.italic();
                }
                if appearance.underline {
                    run.underline = Some(gpui::UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    });
                }
                let size = px(appearance.font_size as f32);
                push(
                    span.range.start.max(start),
                    span.range.end.min(end),
                    run,
                    size,
                );
            }
        } else {
            push(start, end, base.clone(), default_size);
        }
        if runs.is_empty() {
            runs.push((base, default_size));
        }
        runs
    }

    pub(super) fn udl_fold_ranges(&self) -> Vec<(usize, usize)> {
        if matches!(
            self.workspace.active().language(),
            crate::syntax::Language::Builtin(_) | crate::syntax::Language::Json
        ) {
            return self.builtin_fold_ranges();
        }
        self.udl
            .syntax_cache
            .as_ref()
            .filter(|cache| {
                self.workspace.active().language() == crate::syntax::Language::UserDefined(cache.id)
            })
            .map(|cache| {
                cache
                    .lexed
                    .folds
                    .iter()
                    .map(|fold| (fold.start_line, fold.end_line))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn editor_style_line_height(&self) -> gpui::Pixels {
        px(self.editor_style_metrics().line_height())
    }

    pub(super) fn editor_style_baseline(&self) -> gpui::Pixels {
        px(self.editor_style_metrics().baseline())
    }

    fn editor_style_metrics(&self) -> crate::syntax::StyleLineMetrics {
        let profile = self.editor_style_profile();
        let mut metrics = crate::syntax::StyleLineMetrics::new(profile.font_size as f32);
        let text_system = self.editor_text_system().or_else(|| {
            self.udl
                .syntax_cache
                .as_ref()
                .and_then(|cache| cache.text_system.as_deref())
        });
        let mut include = |appearance: crate::syntax::StyleAppearance| {
            let font_size = appearance.font_size as f32;
            metrics.include_size(font_size);
            if let Some(text_system) = text_system {
                let mut font = gpui::font(appearance.font_family);
                if appearance.bold {
                    font = font.bold();
                }
                if appearance.italic {
                    font = font.italic();
                }
                let id = text_system.resolve_font(&font);
                metrics.include(
                    font_size,
                    f32::from(text_system.ascent(id, px(font_size))),
                    f32::from(text_system.descent(id, px(font_size))),
                );
            }
        };
        include(profile.clone());
        if let crate::syntax::Language::UserDefined(id) = self.workspace.active().language()
            && let Some(definition) = self.udl.registry.definition(id)
        {
            for style in crate::udl::UdlStyleId::ALL {
                let appearance = crate::syntax::udl_appearance_with_profile(
                    definition,
                    style,
                    &profile,
                    self.editor_zoom_delta(),
                );
                include(appearance);
            }
        }
        self.include_builtin_style_metrics(&mut metrics);
        metrics
    }

    pub(super) fn editor_style_profile(&self) -> crate::syntax::StyleAppearance {
        let mut profile = crate::syntax::StyleAppearance::plain(self.dark);
        profile.font_family = self.editor_font_family().into();
        profile.font_size = f32::from(self.editor_font_size()).round().max(1.) as u32;
        profile
    }

    pub(super) fn capture_udl_text_system(&mut self, window: &Window) {
        self.capture_builtin_text_system(window);
        if let Some(cache) = &mut self.udl.syntax_cache {
            cache.text_system = Some(window.text_system().clone());
        }
    }

    fn visible_editor_link_spans(
        &self,
        visible: std::ops::Range<usize>,
    ) -> Vec<std::ops::Range<usize>> {
        use ropey::LineType;
        let snapshot = self.editor().display_snapshot();
        if visible.start >= visible.end
            || visible.end > snapshot.len()
            || !snapshot.is_char_boundary(visible.start)
            || !snapshot.is_char_boundary(visible.end)
        {
            return Vec::new();
        }
        let first = snapshot.byte_to_line_idx(visible.start, LineType::LF_CR);
        let last = snapshot.byte_to_line_idx(visible.end, LineType::LF_CR);
        let start = snapshot.line_to_byte_idx(first, LineType::LF_CR);
        let end = if last + 1 < snapshot.len_lines(LineType::LF_CR) {
            snapshot.line_to_byte_idx(last + 1, LineType::LF_CR)
        } else {
            snapshot.len()
        };
        let text = snapshot.slice(start..end).to_string();
        crate::links::link_spans(&text, visible.start - start..visible.end - start)
            .into_iter()
            .map(|range| range.start + start..range.end + start)
            .collect()
    }
}

#[cfg(all(test, feature = "native-input-tests"))]
mod link_tests {
    use super::*;
    use gpui::EntityInputHandler;

    fn base_run(len: usize) -> gpui::TextRun {
        gpui::TextRun {
            len,
            font: gpui::font("Menlo"),
            color: rgb(0x000000).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[gpui::test]
    fn udl_baseline_includes_an_unused_larger_style_on_every_row(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, "plain\nplain", window, cx);
            view.udl
                .registry
                .edit(0, |definition| {
                    definition.styles[23].font_size = 40;
                    Ok(())
                })
                .expect("configure unused delimiter style");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::UserDefined(0));
            view.refresh_editor_syntax();
            view.capture_udl_text_system(window);
            let font = gpui::font(view.editor_font_family());
            let id = window.text_system().resolve_font(&font);
            let size = px((40 + view.editor_zoom_delta()).max(1) as f32);
            let ascent = window.text_system().ascent(id, size);
            let descent = window.text_system().descent(id, size).abs();
            let height = px((f32::from(size) * 26. / 20.)
                .max(f32::from(ascent + descent))
                .ceil());
            assert_eq!(view.editor_style_line_height(), height);
            assert_eq!(
                view.editor_style_baseline(),
                (height - ascent - descent) / 2. + ascent
            );
            assert_eq!(
                view.editor_style_profile().font_family,
                view.editor_font_family()
            );
        });
    }

    #[gpui::test]
    fn udl_definition_edits_restyle_unchanged_text_and_keep_font_sizes_and_underlines(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            let source = "token https://host";
            EntityInputHandler::replace_text_in_range(view, None, source, window, cx);
            view.udl
                .registry
                .edit(0, |definition| {
                    definition.keywords[0] = "token".into();
                    definition.styles[4].foreground = Some(0x123456);
                    definition.styles[4].font_size = 13;
                    definition.styles[4].font_style = 7;
                    definition.styles[4].font_name = "Menlo".into();
                    Ok(())
                })
                .expect("configure working definition");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::UserDefined(0));
            view.refresh_editor_syntax();
            view.capture_udl_text_system(window);
            let revision = view.editor().display_revision();
            let marked = ByteOffset(1)..ByteOffset(3);
            let runs =
                view.editor_style_runs(0, source.len(), Some(&marked), base_run(source.len()));
            assert_eq!(
                runs.iter().map(|(run, _)| run.len).sum::<usize>(),
                source.len()
            );
            let mut byte = 0;
            for (run, size) in runs {
                if byte < 5 {
                    assert_eq!(size, px((13 + view.editor_zoom_delta()).max(1) as f32));
                    assert_eq!(run.color, gpui::Hsla::from(rgb(0x123456)));
                    assert_eq!(run.font.weight, gpui::FontWeight::BOLD);
                    assert_eq!(run.font.style, gpui::FontStyle::Italic);
                    assert_eq!(run.font.family.as_ref(), "Menlo");
                    assert!(
                        run.underline.is_some(),
                        "syntax underline survives IME cuts"
                    );
                } else {
                    assert_eq!(
                        size,
                        view.editor_font_size(),
                        "unset styles inherit the calibrated editor base"
                    );
                }
                byte += run.len;
            }
            assert!(view.editor_style_line_height() >= px(26.));
            view.udl
                .registry
                .edit(0, |definition| {
                    definition.styles[4].foreground = Some(0xabcdef);
                    definition.styles[4].font_size = 26;
                    Ok(())
                })
                .expect("edit style without editing document");
            view.refresh_editor_syntax();
            assert_eq!(view.editor().display_revision(), revision);
            let runs = view.editor_style_runs(0, source.len(), None, base_run(source.len()));
            assert_eq!(runs[0].0.color, gpui::Hsla::from(rgb(0xabcdef)));
            assert_eq!(runs[0].1, px((26 + view.editor_zoom_delta()).max(1) as f32));
            assert!(view.editor_style_line_height() >= px(34.));
        });
    }

    #[gpui::test]
    fn udl_fold_ranges_use_inclusive_logical_lines_and_missing_definition_keeps_identity(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        view.update_in(cx, |view, window, cx| {
            let source = "begin\r\ninside\r\nend";
            EntityInputHandler::replace_text_in_range(view, None, source, window, cx);
            view.udl
                .registry
                .edit(0, |definition| {
                    definition.folding[0].open = "begin".into();
                    definition.folding[0].close = "end".into();
                    Ok(())
                })
                .expect("configure folding");
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::UserDefined(0));
            view.refresh_editor_syntax();
            assert_eq!(view.udl_fold_ranges(), [(0, 2)]);
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::UserDefined(999));
            view.refresh_editor_syntax();
            assert_eq!(
                view.workspace.active().language(),
                crate::syntax::Language::UserDefined(999)
            );
            assert_eq!(view.editor().committed_text(), source);
            assert!(view.udl_fold_ranges().is_empty());
            assert!(
                view.editor_error
                    .as_ref()
                    .is_some_and(|error| error.contains("999"))
            );
        });
    }

    #[gpui::test]
    fn visible_url_is_underlined_in_default_foreground_without_touching_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let source = "pre https://host/a. tail";
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, source, window, cx);
            let before = view.editor().selection();
            let runs = view.styled_editor_runs(0, source.len(), None, base_run(source.len()));
            assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), source.len());
            let mut byte = 0;
            for run in runs {
                assert_eq!(run.color, gpui::Hsla::from(rgb(0x000000)));
                for position in byte..byte + run.len {
                    assert_eq!(run.underline.is_some(), (4..18).contains(&position));
                }
                byte += run.len;
            }
            assert_eq!(view.editor().selection(), before);
            assert_eq!(view.editor().committed_text(), source);
        });
    }

    #[gpui::test]
    fn url_decoration_keeps_json_foreground_background_and_ime_underline_priority(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| MatrixWindow::new(false, window, cx));
        let source = "{\"url\":\"https://host/path\",\"n\":12}";
        view.update_in(cx, |view, window, cx| {
            EntityInputHandler::replace_text_in_range(view, None, source, window, cx);
            view.workspace
                .active_mut()
                .set_language(crate::syntax::Language::Json);
            view.refresh_editor_syntax();
            let marked = ByteOffset(10)..ByteOffset(14);
            let runs =
                view.styled_editor_runs(0, source.len(), Some(&marked), base_run(source.len()));
            assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), source.len());
            let syntax = crate::syntax::json_styles(source);
            let mut byte = 0;
            for run in runs {
                for position in byte..byte + run.len {
                    let span = syntax
                        .iter()
                        .find(|span| span.range.contains(&position))
                        .expect("JSON token");
                    let (foreground, background, _) = span.style.appearance(false);
                    assert_eq!(run.color, gpui::Hsla::from(rgb(foreground)));
                    assert_eq!(
                        run.background_color,
                        Some(gpui::Hsla::from(rgb(background)))
                    );
                    if (10..14).contains(&position) {
                        assert_eq!(
                            run.underline.as_ref().expect("IME underline").color,
                            Some(run.color)
                        );
                    } else if (8..25).contains(&position) {
                        assert_eq!(
                            run.underline.as_ref().expect("link underline").color,
                            Some(gpui::Hsla::from(rgb(0x000000)))
                        );
                    } else {
                        assert!(run.underline.is_none());
                    }
                }
                byte += run.len;
            }
        });
    }
}
