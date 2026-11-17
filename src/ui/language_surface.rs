use super::*;

pub(super) struct UdlSyntaxCache {
    revision: crate::input::DisplayRevision,
    id: u64,
    definition_revision: u64,
    definition: crate::udl::Definition,
    lexed: crate::udl::LexResult,
    text_system: Option<std::rc::Rc<gpui::WindowTextSystem>>,
}

impl UdlSyntaxCache {
    pub(super) fn style_at(&self, position: usize) -> Option<crate::udl::UdlStyleId> {
        self.lexed
            .spans
            .iter()
            .find(|span| span.range.contains(&position))
            .map(|span| span.style)
    }
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
        if !self.workspace.smart_highlight_allowed() {
            self.preferences.appearance.smart_token = None;
        }
        if !self.workspace.syntax_highlighting_allowed() {
            let changed = self.syntax_cache.take().is_some()
                | self.udl.syntax_cache.take().is_some()
                | self.builtin.syntax_cache.take().is_some();
            if changed {
                self.editor_chrome.invalidate_layout();
                self.editor_painted_revision = None;
            }
            return;
        }
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

    pub(super) fn editor_style_runs(
        &self,
        start: usize,
        end: usize,
        marked: Option<&std::ops::Range<ByteOffset>>,
        base: gpui::TextRun,
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        let links = self.visible_editor_link_spans(start..end);
        self.editor_style_runs_with_links(start, end, marked, base, &links)
    }

    pub(super) fn editor_style_runs_with_links(
        &self,
        start: usize,
        end: usize,
        marked: Option<&std::ops::Range<ByteOffset>>,
        base: gpui::TextRun,
        links: &[std::ops::Range<usize>],
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        if !self.workspace.syntax_highlighting_allowed() {
            return self.plain_editor_style_runs(start, end, marked, base, links);
        }
        if matches!(
            self.workspace.active().language(),
            crate::syntax::Language::Builtin(_) | crate::syntax::Language::Json
        ) {
            return self.builtin_style_runs(start, end, marked, base, links);
        }
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
            for link in links {
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
                        self.preferences.files.links.underline().then_some(())?;
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
        if !self.workspace.syntax_highlighting_allowed() {
            return Vec::new();
        }
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

    pub(super) fn udl_fold_header_levels(&self) -> (bool, Vec<(usize, u32)>) {
        if !self.workspace.syntax_highlighting_allowed() {
            return (false, Vec::new());
        }
        if matches!(
            self.workspace.active().language(),
            crate::syntax::Language::Builtin(_) | crate::syntax::Language::Json
        ) {
            return self.builtin_fold_header_levels();
        }
        let headers = self
            .udl
            .syntax_cache
            .as_ref()
            .filter(|cache| {
                self.workspace.active().language() == crate::syntax::Language::UserDefined(cache.id)
                    && cache.revision == self.editor().display_revision()
                    && self.udl.registry.definition_revision(cache.id)
                        == Some(cache.definition_revision)
            })
            .map(|cache| {
                cache
                    .lexed
                    .levels
                    .iter()
                    .enumerate()
                    .filter(|(_, level)| level.header)
                    .map(|(line, level)| {
                        (line, (level.level.wrapping_add(0x400) & 0x0fff) as u32)
                    })
                    .collect()
            })
            .unwrap_or_default();
        (false, headers)
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
        let (family, size) = self.editor_line_number_font();
        let bits = self.editor_line_number_font_style();
        let mut line_number = profile.clone();
        line_number.font_family = family;
        line_number.font_size = f32::from(size).ceil().max(1.) as u32;
        line_number.bold = bits & 1 != 0;
        line_number.italic = bits & 2 != 0;
        line_number.underline = bits & 4 != 0;
        include(line_number);
        for id in [34, 35, 37] {
            include(self.style_config.appearance(None, id, self.dark, &profile, self.editor_zoom_delta()));
        }
        if self.workspace.syntax_highlighting_allowed()
            && let crate::syntax::Language::UserDefined(id) = self.workspace.active().language()
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
        profile.font_family = "Helvetica".into();
        profile.font_size = f32::from(self.editor_font_size()).round().max(1.) as u32;
        self.style_config
            .appearance(None, 32, self.dark, &profile, self.editor_zoom_delta())
    }

    pub(super) fn capture_udl_text_system(&mut self, window: &Window) {
        self.capture_builtin_text_system(window);
        if let Some(cache) = &mut self.udl.syntax_cache {
            cache.text_system = Some(window.text_system().clone());
        }
    }

    pub(super) fn plain_editor_style_runs(
        &self,
        start: usize,
        end: usize,
        marked: Option<&std::ops::Range<ByteOffset>>,
        base: gpui::TextRun,
        links: &[std::ops::Range<usize>],
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        let mut marks = self.editor().marked_ranges();
        if let Some(marked) = marked {
            marks.push(marked.clone());
        }
        let mut cuts = vec![start, end];
        for range in &marks {
            cuts.extend([range.start.0.clamp(start, end), range.end.0.clamp(start, end)]);
        }
        for range in links {
            cuts.extend([range.start.clamp(start, end), range.end.clamp(start, end)]);
        }
        cuts.sort_unstable();
        cuts.dedup();
        cuts.windows(2).map(|pair| {
            let mut run = base.clone();
            run.len = pair[1] - pair[0];
            if marks.iter().any(|mark| pair[0] >= mark.start.0 && pair[1] <= mark.end.0)
                || (self.preferences.files.links.underline()
                    && links.iter().any(|link| pair[0] >= link.start && pair[1] <= link.end))
            {
                run.underline = Some(gpui::UnderlineStyle {
                    color: Some(run.color), thickness: px(1.), wavy: false,
                });
            }
            (run, self.editor_font_size())
        }).collect()
    }

    pub(super) fn editor_link_at(&self, position: ByteOffset) -> Option<std::ops::Range<ByteOffset>> {
        if !self.workspace.clickable_links_allowed() {
            return None;
        }
        let text = self.editor().display_text();
        let range = self.preferences.files.links.link_at(&text, position.0)?;
        let projection = self.editor().document().projection();
        let start = projection.source_position(crate::byte_projection::DisplayByteOffset(range.start), crate::byte_projection::Affinity::Before).ok()?;
        let end = projection.source_position(crate::byte_projection::DisplayByteOffset(range.end), crate::byte_projection::Affinity::After).ok()?;
        Some(start..end)
    }

    pub(super) fn visible_editor_link_spans(
        &self,
        visible: std::ops::Range<usize>,
    ) -> Vec<std::ops::Range<usize>> {
        use ropey::LineType;
        if !self.workspace.clickable_links_allowed() || !self.preferences.files.links.enabled() {
            return Vec::new();
        }
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
        self.editor_link_spans_in_text(&text, start, visible.start - start..visible.end - start)
    }

    pub(super) fn editor_link_spans_in_text(
        &self,
        text: &str,
        start: usize,
        visible: std::ops::Range<usize>,
    ) -> Vec<std::ops::Range<usize>> {
        if !self.workspace.clickable_links_allowed() || !self.preferences.files.links.enabled() {
            return Vec::new();
        }
        self.preferences.files.links.link_spans(text, visible)
            .into_iter()
            .map(|range| range.start + start..range.end + start)
            .collect()
    }
}
