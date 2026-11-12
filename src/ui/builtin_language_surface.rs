use super::*;

#[cfg(all(test, feature = "native-input-tests"))]
#[path = "builtin_language_surface_tests.rs"]
mod tests;

pub(super) struct BuiltinState {
    pub(super) syntax_cache: Option<BuiltinSyntaxCache>,
    pub(super) style_revision: u64,
    pub(super) tag_match_options: crate::builtin_language::TagMatchOptions,
}

impl BuiltinState {
    pub(super) fn new() -> Self {
        Self {
            syntax_cache: None,
            style_revision: 0,
            tag_match_options: crate::builtin_language::TagMatchOptions::default(),
        }
    }
}

pub(super) struct BuiltinSyntaxCache {
    document: crate::workspace::BufferId,
    revision: crate::input::DisplayRevision,
    runtime: crate::builtin_language::SyntaxCache,
    style_revision: u64,
    profile: crate::syntax::StyleAppearance,
    dark: bool,
    zoom_delta: i32,
    text_system: Option<std::rc::Rc<gpui::WindowTextSystem>>,
}

impl MatrixWindow {
    pub(super) fn refresh_builtin_syntax(&mut self) -> bool {
        self.refresh_builtin_syntax_update().is_some()
            || matches!(
                self.workspace.active().language(),
                crate::syntax::Language::Builtin(_) | crate::syntax::Language::Json
            )
    }

    pub(super) fn refresh_builtin_syntax_update(
        &mut self,
    ) -> Option<crate::builtin_language::CacheUpdate> {
        let id = match self.workspace.active().language() {
            crate::syntax::Language::Builtin(id) => id,
            crate::syntax::Language::Json => 57,
            _ => {
                self.builtin.syntax_cache = None;
                return None;
            }
        };
        let definition = match crate::languages::catalog()
            .map_err(|error| error.to_string())
            .and_then(|catalog| {
                catalog
                    .by_id(id)
                    .ok_or_else(|| format!("Built-in language {id} is unavailable"))
            }) {
            Ok(definition) => definition,
            Err(error) => {
                self.builtin.syntax_cache = None;
                self.editor_error = Some(error);
                return None;
            }
        };
        let document = self.workspace.active().id();
        let revision = self.editor().display_revision();
        let profile = self.editor_style_profile();
        let zoom_delta = self.editor_zoom_delta();
        let style_revision = self.builtin.style_revision;
        if self.builtin.syntax_cache.as_ref().is_some_and(|cache| {
            cache.document == document
                && cache.revision == revision
                && cache.runtime.definition() == definition
                && cache.style_revision == style_revision
                && cache.profile == profile
                && cache.dark == self.dark
                && cache.zoom_delta == zoom_delta
        }) {
            return Some(crate::builtin_language::CacheUpdate::Unchanged);
        }
        self.editor_chrome.invalidate_layout();
        self.editor_painted_revision = None;
        let text = self.editor().display_text();
        let mut previous = self.builtin.syntax_cache.take();
        let text_system = previous.as_mut().and_then(|cache| cache.text_system.take());
        let updated = if let Some(mut cache) = previous.filter(|cache| cache.document == document) {
            let presentation_changed = cache.profile != profile
                || cache.dark != self.dark
                || cache.zoom_delta != zoom_delta;
            cache
                .runtime
                .refresh(&text, definition, style_revision)
                .map(|update| {
                    let update = if presentation_changed
                        && update == crate::builtin_language::CacheUpdate::Unchanged
                    {
                        crate::builtin_language::CacheUpdate::Restyled
                    } else {
                        update
                    };
                    (cache.runtime, update)
                })
        } else {
            crate::builtin_language::SyntaxCache::new(&text, definition, style_revision).map(
                |runtime| {
                    (
                        runtime,
                        crate::builtin_language::CacheUpdate::Relexed(crate::lexers::RelexReport {
                            relexed_range: 0..text.len(),
                            reused_prefix_lines: 0,
                            reused_suffix_lines: 0,
                        }),
                    )
                },
            )
        };
        match updated {
            Ok((runtime, update)) => {
                self.builtin.syntax_cache = Some(BuiltinSyntaxCache {
                    document,
                    revision,
                    runtime,
                    style_revision,
                    profile,
                    dark: self.dark,
                    zoom_delta,
                    text_system,
                });
                if self
                    .editor_error
                    .as_ref()
                    .is_some_and(|error| error.starts_with("Built-in language "))
                {
                    self.editor_error = None;
                }
                Some(update)
            }
            Err(error) => {
                self.editor_error = Some(format!("Built-in language {id}: {error}"));
                None
            }
        }
    }

    fn active_builtin_cache(&self) -> Option<&BuiltinSyntaxCache> {
        let id = match self.workspace.active().language() {
            crate::syntax::Language::Builtin(id) => id,
            crate::syntax::Language::Json => 57,
            _ => return None,
        };
        self.builtin.syntax_cache.as_ref().filter(|cache| {
            cache.document == self.workspace.active().id()
                && cache.runtime.definition().id == id
                && cache.revision == self.editor().display_revision()
        })
    }

    pub(super) fn builtin_style_runs(
        &self,
        start: usize,
        end: usize,
        marked: Option<&std::ops::Range<ByteOffset>>,
        base: gpui::TextRun,
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        let profile = self.editor_style_profile();
        let Some(cache) = self.active_builtin_cache() else {
            return vec![(base, self.editor_font_size())];
        };
        let spans = match cache.runtime.styled_runs(
            start..end,
            self.dark,
            &profile,
            self.editor_zoom_delta(),
        ) {
            Ok(spans) => spans,
            Err(error) => {
                eprintln!(
                    "Built-in language {} style run request failed: {error}",
                    cache.runtime.definition().id
                );
                return Vec::new();
            }
        };
        let text = cache.runtime.text();
        let line_start = text[..start].rfind(['\r', '\n']).map_or(0, |byte| byte + 1);
        let line_end = text[end..]
            .find(['\r', '\n'])
            .map_or(text.len(), |byte| end + byte);
        let links = crate::links::link_spans(
            &text[line_start..line_end],
            start - line_start..end - line_start,
        )
        .into_iter()
        .map(|range| range.start + line_start..range.end + line_start)
        .collect::<Vec<_>>();
        let mut marked_ranges = self.editor().marked_ranges();
        if let Some(marked) = marked {
            marked_ranges.push(marked.clone());
        }
        let mut runs = Vec::new();
        for span in spans {
            let appearance = span.appearance;
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
            let mut cuts = vec![span.range.start, span.range.end];
            for marked in &marked_ranges {
                cuts.extend([
                    marked.start.0.clamp(span.range.start, span.range.end),
                    marked.end.0.clamp(span.range.start, span.range.end),
                ]);
            }
            for link in &links {
                cuts.extend([
                    link.start.clamp(span.range.start, span.range.end),
                    link.end.clamp(span.range.start, span.range.end),
                ]);
            }
            cuts.sort_unstable();
            cuts.dedup();
            for pair in cuts.windows(2) {
                let mut run = run.clone();
                run.len = pair[1] - pair[0];
                run.underline = if appearance.underline
                    || marked_ranges
                        .iter()
                        .any(|mark| pair[0] >= mark.start.0 && pair[1] <= mark.end.0)
                {
                    Some(gpui::UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    })
                } else if links
                    .iter()
                    .any(|link| pair[0] >= link.start && pair[1] <= link.end)
                {
                    Some(gpui::UnderlineStyle {
                        color: Some(base.color),
                        thickness: px(1.),
                        wavy: false,
                    })
                } else {
                    None
                };
                runs.push((run, px(appearance.font_size as f32)));
            }
        }
        if runs.is_empty() {
            runs.push((base, px(profile.font_size as f32)));
        }
        runs
    }

    pub(super) fn builtin_fold_ranges(&self) -> Vec<(usize, usize)> {
        self.active_builtin_cache()
            .map(|cache| {
                cache
                    .runtime
                    .result()
                    .folds
                    .iter()
                    .map(|fold| (fold.start_line, fold.end_line))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn builtin_tag_match_ranges(
        &self,
    ) -> Option<crate::builtin_language::TagMatchRanges> {
        let cache = self.active_builtin_cache()?;
        let mut options = self.builtin.tag_match_options;
        options.xhtml |= self
            .disk_file()
            .and_then(|file| file.path().extension())
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xhtml"));
        cache
            .runtime
            .matching_tags(self.editor().selection().head.0, options)
    }

    pub(super) fn builtin_tag_indicators(&self) -> Vec<crate::builtin_language::TagIndicator> {
        self.builtin_tag_match_ranges()
            .map(|ranges| crate::builtin_language::tag_indicators(&ranges, self.dark))
            .unwrap_or_default()
    }

    pub(super) fn builtin_style_appearances(&self) -> Vec<crate::syntax::StyleAppearance> {
        self.active_builtin_cache()
            .map(|cache| {
                crate::builtin_language::metric_appearances(
                    cache.runtime.definition(),
                    self.dark,
                    &self.editor_style_profile(),
                    self.editor_zoom_delta(),
                )
            })
            .unwrap_or_default()
    }

    pub(super) fn include_builtin_style_metrics(
        &self,
        metrics: &mut crate::syntax::StyleLineMetrics,
    ) {
        let text_system = self.editor_text_system().or_else(|| {
            self.active_builtin_cache()
                .and_then(|cache| cache.text_system.as_deref())
        });
        for appearance in self.builtin_style_appearances() {
            let size = appearance.font_size as f32;
            metrics.include_size(size);
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
                    size,
                    f32::from(text_system.ascent(id, px(size))),
                    f32::from(text_system.descent(id, px(size))),
                );
            }
        }
    }

    pub(super) fn capture_builtin_text_system(&mut self, window: &Window) {
        if let Some(cache) = &mut self.builtin.syntax_cache {
            cache.text_system = Some(window.text_system().clone());
        }
    }
}
