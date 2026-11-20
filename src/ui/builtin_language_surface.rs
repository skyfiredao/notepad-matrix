use super::*;

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
        if !self.workspace.syntax_highlighting_allowed() {
            if self.builtin.syntax_cache.take().is_some() {
                self.editor_chrome.invalidate_layout();
                self.editor_painted_revision = None;
            }
            return None;
        }
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
                self.editor_error = Some(error.into());
                return None;
            }
        };
        let mut definition = self.style_config.effective_language(definition, self.dark);
        self.apply_editing_language_properties(&mut definition);
        let document = self.workspace.active().id();
        let revision = self.editor().display_revision();
        let profile = self.editor_style_profile();
        let zoom_delta = self.editor_zoom_delta();
        let style_revision = self.style_config.revision();
        if self.builtin.syntax_cache.as_ref().is_some_and(|cache| {
            cache.document == document
                && cache.revision == revision
                && cache.runtime.definition() == &definition
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
                .refresh(&text, &definition, style_revision)
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
            crate::builtin_language::SyntaxCache::new(&text, &definition, style_revision).map(
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
                    .is_some_and(|error| matches!(error, crate::localization::Text::Literal(text) if text.starts_with("Built-in language ")))
                {
                    self.editor_error = None;
                }
                Some(update)
            }
            Err(error) => {
                self.editor_error = Some(format!("Built-in language {id}: {error}").into());
                None
            }
        }
    }

    fn active_builtin_cache(&self) -> Option<&BuiltinSyntaxCache> {
        if !self.workspace.syntax_highlighting_allowed() {
            return None;
        }
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
        links: &[std::ops::Range<usize>],
    ) -> Vec<(gpui::TextRun, gpui::Pixels)> {
        let profile = self.editor_style_profile();
        let Some(cache) = self.active_builtin_cache() else {
            return self.plain_editor_style_runs(start, end, marked, base, links);
        };
        let text = cache.runtime.text();
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            eprintln!(
                "Built-in language {} style range is invalid",
                cache.runtime.definition().id
            );
            return Vec::new();
        }
        let spans = &cache.runtime.result().spans;
        let first = spans.partition_point(|span| span.range.end <= start);
        let spans = spans[first..]
            .iter()
            .take_while(|span| span.range.start < end)
            .cloned()
            .map(|mut span| {
                span.range = span.range.start.max(start)..span.range.end.min(end);
                span
            })
            .collect::<Vec<_>>();
        let mut marked_ranges = self.editor().marked_ranges();
        if let Some(marked) = marked {
            marked_ranges.push(marked.clone());
        }
        let mut runs = Vec::new();
        for span in spans {
            let appearance = self.style_config.appearance(
                Some(cache.runtime.definition()),
                span.style,
                self.dark,
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
            let mut cuts = vec![span.range.start, span.range.end];
            for marked in &marked_ranges {
                cuts.extend([
                    marked.start.0.clamp(span.range.start, span.range.end),
                    marked.end.0.clamp(span.range.start, span.range.end),
                ]);
            }
            for link in links {
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
                } else if self.preferences.files.links.underline() && links
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

    pub(super) fn builtin_fold_header_levels(&self) -> (bool, Vec<(usize, u32)>) {
        let Some(cache) = self.active_builtin_cache() else {
            return (false, Vec::new());
        };
        let indentation_based = matches!(
            cache.runtime.definition().lexer(),
            "python" | "coffeescript" | "haskell" | "nimrod" | "vb" | "yaml"
        );
        let headers = cache
            .runtime
            .result()
            .fold_levels
            .iter()
            .filter(|level| level.header)
            .map(|level| (level.line, level.level & 0x0fff))
            .collect();
        (indentation_based, headers)
    }

    pub(super) fn builtin_tag_match_ranges(
        &self,
    ) -> Option<crate::builtin_language::TagMatchRanges> {
        if !self.workspace.brace_match_allowed() {
            return None;
        }
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
            .map(|ranges| {
                ranges
                    .tags
                    .iter()
                    .map(|range| crate::builtin_language::TagIndicator {
                        range: range.clone(),
                        color: self.style_colour("Tags match highlighting", "bgColor", 0x8000ff),
                        alpha: 100,
                    })
                    .chain(ranges.attributes.iter().map(|range| {
                        crate::builtin_language::TagIndicator {
                            range: range.clone(),
                            color: self.style_colour("Tags attribute", "bgColor", 0xffff00),
                            alpha: 100,
                        }
                    }))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn builtin_brace_style(&self, byte: usize) -> Option<u16> {
        let spans = &self.active_builtin_cache()?.runtime.result().spans;
        let index = spans.partition_point(|span| span.range.end <= byte);
        spans
            .get(index)
            .filter(|span| span.range.contains(&byte))
            .map(|span| span.style)
    }

    pub(super) fn builtin_style_appearances(&self) -> Vec<crate::syntax::StyleAppearance> {
        self.active_builtin_cache()
            .map(|cache| {
                let definition = cache.runtime.definition();
                let profile = self.editor_style_profile();
                definition
                    .styles
                    .iter()
                    .map(|style| {
                        self.style_config.appearance(
                            Some(definition),
                            style.id,
                            self.dark,
                            &profile,
                            self.editor_zoom_delta(),
                        )
                    })
                    .collect()
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
