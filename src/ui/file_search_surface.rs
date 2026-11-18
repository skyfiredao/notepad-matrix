use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::*;

#[path = "finder_views.rs"]
mod finder_views;

fn choose_search_directory(initial: &std::path::Path) -> io::Result<Option<PathBuf>> {
    use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{MainThreadMarker, NSString, NSURL};
    let mtm = MainThreadMarker::new().ok_or_else(|| io::Error::other("目录选择需要主线程"))?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setTitle(Some(&NSString::from_str("选择要搜索的文件夹")));
    if initial.is_dir() {
        let url = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(&initial.to_string_lossy()),
            true,
        );
        panel.setDirectoryURL(Some(&url));
    }
    let response = panel.runModal();
    if response == NSModalResponseCancel {
        return Ok(None);
    }
    if response != NSModalResponseOK {
        return Err(io::Error::other("目录选择窗口未能完成"));
    }
    let path = panel
        .URL()
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| io::Error::other("目录选择未返回有效路径"))?;
    crate::file_access::selected(&path)?;
    Ok(Some(path))
}

#[derive(Clone, Copy)]
enum FinderCopy {
    Verbatim,
    Lines,
    Pathnames,
}

impl FinderCopy {
    fn label(self) -> &'static str {
        match self {
            Self::Verbatim => "Copy",
            Self::Lines => "Copy Selected Line(s)",
            Self::Pathnames => "Copy Selected Pathname(s)",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::Verbatim => "finder-copy-verbatim",
            Self::Lines => "finder-copy-selected-lines",
            Self::Pathnames => "finder-copy-selected-pathnames",
        }
    }
}

#[derive(Clone, Copy)]
enum FinderCommand {
    FoldAll,
    UnfoldAll,
    Copy(FinderCopy),
    SelectAll,
    ClearAll,
    TogglePurge,
    OpenSelectedPathnames,
    ToggleWrap,
    FindWithin,
    CloseResults,
}

impl FinderCommand {
    const ALL: [Self; 12] = [
        Self::FindWithin,
        Self::CloseResults,
        Self::FoldAll,
        Self::UnfoldAll,
        Self::Copy(FinderCopy::Verbatim),
        Self::Copy(FinderCopy::Lines),
        Self::Copy(FinderCopy::Pathnames),
        Self::SelectAll,
        Self::ClearAll,
        Self::OpenSelectedPathnames,
        Self::ToggleWrap,
        Self::TogglePurge,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::FoldAll => "Fold all",
            Self::UnfoldAll => "Unfold all",
            Self::Copy(copy) => copy.label(),
            Self::SelectAll => "Select all",
            Self::ClearAll => "Clear all",
            Self::TogglePurge => "Purge for every search",
            Self::OpenSelectedPathnames => "Open Selected Pathname(s)",
            Self::ToggleWrap => "Word wrap long lines",
            Self::FindWithin => "Find in these search results...",
            Self::CloseResults => "Close these search results",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::FoldAll => "finder-fold-all",
            Self::UnfoldAll => "finder-unfold-all",
            Self::Copy(copy) => copy.selector(),
            Self::SelectAll => "finder-select-all",
            Self::ClearAll => "finder-clear-all",
            Self::TogglePurge => "finder-purge-every-search",
            Self::OpenSelectedPathnames => "finder-open-selected-pathnames",
            Self::ToggleWrap => "finder-wrap-long-lines",
            Self::FindWithin => "finder-find-in-results",
            Self::CloseResults => "finder-close-results",
        }
    }
}


use crate::find_in_files::FileSearchRequest;

enum FileOperationResult {
    Search(crate::finder::Search, Vec<(usize, crate::search::RegexState)>),
    Replace(crate::find_in_files::FileReplaceResults),
}

pub(super) struct FileSearchState {
    views: finder_views::State,
    pub visible: bool,
    focus: FocusHandle,
    results_visible: bool,
    created: bool,
    directory: gpui::Entity<text_field::TextField>,
    filters: gpui::Entity<text_field::TextField>,
    recursive: bool,
    hidden_directories: bool,
    ignore_unsaved_changes: bool,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<crate::find_in_files::FileSearchProgress>>,
    results: crate::finder::Results,
    results_focus: FocusHandle,
    results_scroll: gpui::UniformListScrollHandle,
    wrapped_scroll: gpui::ListState,
    results_wrap: bool,
    wrapped_preferred_x: Option<gpui::Pixels>,
    results_selected: usize,
    results_width_item: Option<usize>,
    results_width: gpui::Pixels,
    style_revision: Option<(u64, bool)>,
    layout_cache: std::cell::RefCell<finder_layout::Cache>,
    results_cursor_x: Option<gpui::Pixels>,
    caret_affinity: Option<(u64, crate::finder::TextPoint, bool)>,
    text_anchor: Option<crate::finder::TextPoint>,
    text_head: Option<crate::finder::TextPoint>,
    dragging_text: bool,
    context: Option<gpui::Point<gpui::Pixels>>,
    context_focus: FocusHandle,
    context_selected: usize,
    results_purge: bool,
    results_one_per_line: bool,
    always_visible: bool,
    message: String,
}

impl FileSearchState {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            views: finder_views::State::new(cx),
            visible: false,
            focus: cx.focus_handle(),
            results_visible: false,
            created: false,
            directory: cx.new(|cx| text_field::TextField::compact("目录", "files-directory", cx).with_tab_index(4)),
            filters: cx.new(|cx| text_field::TextField::compact("文件类型", "files-filters", cx).with_tab_index(3)),
            recursive: true,
            hidden_directories: false,
            ignore_unsaved_changes: false,
            cancel: None,
            progress: None,
            results: crate::finder::Results::default(),
            results_focus: cx.focus_handle(),
            results_scroll: gpui::UniformListScrollHandle::new(),
            wrapped_scroll: gpui::ListState::new(0, gpui::ListAlignment::Top, px(100.)),
            results_wrap: false,
            wrapped_preferred_x: None,
            results_selected: 0,
            results_width_item: None,
            results_width: px(0.),
            style_revision: None,
            layout_cache: std::cell::RefCell::new(finder_layout::Cache::default()),
            results_cursor_x: None,
            caret_affinity: None,
            text_anchor: None,
            text_head: None,
            dragging_text: false,
            context: None,
            context_focus: cx.focus_handle(),
            context_selected: 0,
            results_purge: false,
            results_one_per_line: true,
            always_visible: false,
            message: String::new(),
        }
    }
}

impl Drop for FileSearchState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl MatrixWindow {
    pub(super) fn configure_file_search_preference_font(&mut self, monospaced: bool, cx: &mut Context<Self>) {
        self.files_search.directory.update(cx, |field, cx| field.set_monospaced(monospaced, cx));
        self.files_search.filters.update(cx, |field, cx| field.set_monospaced(monospaced, cx));
    }

    fn capture_open_search_documents(&self) -> (Vec<crate::editor::Document>, Vec<crate::find_in_files::OpenSearchDocument>) {
        let mut identities: Vec<crate::editor::Document> = Vec::new();
        let mut opened: Vec<crate::find_in_files::OpenSearchDocument> = Vec::new();
        if !self.files_search.ignore_unsaved_changes {
            for buffer in self.workspace.buffers() {
                let document = buffer.editor.document();
                let path = buffer.disk_file.as_ref().map_or_else(|| PathBuf::from(buffer.name()), |file| file.path().to_path_buf());
                if let Some(index) = identities.iter().position(|identity| identity.shares_document_with(document)) {
                    opened[index].paths.push(path);
                } else {
                    identities.push(document.clone_view());
                    opened.push(crate::find_in_files::OpenSearchDocument {
                        paths: vec![path], bytes: document.byte_snapshot(), regex_state: document.regex_state(),
                    });
                }
            }
        }
        (identities, opened)
    }

    fn write_search_regex_states(&mut self, identities: &[crate::editor::Document], updates: Vec<(usize, crate::search::RegexState)>) {
        for (index, state) in updates {
            if let Some(identity) = identities.get(index)
                && let Some(buffer) = self.workspace.buffers_mut().find(|buffer| buffer.editor.document().shares_document_with(identity))
            {
                buffer.editor.document_mut().set_regex_state(state);
            }
        }
    }

    pub(super) fn cancel_finder_drag(&mut self) {
        self.files_search.dragging_text = false;
    }

    pub(super) fn search_results_created(&self) -> bool {
        self.files_search.created
    }

    pub(super) fn navigate_search_results(
        &mut self,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_search.created { return; }
        if id == 43045 {
            if self.files_search.results_focus.is_focused(window) {
                self.editor_focus.focus(window, cx);
            } else {
                self.files_search.results_visible = true;
                self.files_search.results_focus.focus(window, cx);
            }
            cx.notify();
            return;
        }
        use crate::finder::{Row, TextPoint};
        let caret = self.finder_caret();
        let Some(caret_row) = caret.row else { return; };
        let search_index = match caret_row {
                Row::Search(index)
                | Row::Document(index, _)
                | Row::Line(index, _, _)
                | Row::Error(index, _) => index,
        };
        let row_key = |row| match row {
            Row::Search(_) => (0, 0),
            Row::Document(_, doc) => (doc + 1, 0),
            Row::Line(_, doc, line) => (doc + 1, line + 1),
            Row::Error(_, _) => (usize::MAX, usize::MAX),
        };
        let current_key = row_key(caret_row);
        let mut hits = Vec::new();
        if let Some(search) = self.files_search.results.searches.get(search_index) {
            for (document_index, document) in search.documents.iter().enumerate() {
                for (line_index, line) in document.lines.iter().enumerate() {
                    for (range, highlight) in line.ranges.iter().zip(&line.highlights) {
                        hits.push((
                            crate::finder::Row::Line(search_index, document_index, line_index),
                            PathBuf::from(&document.name),
                            range.clone(),
                            highlight.clone(),
                        ));
                    }
                }
            }
        }
        if hits.is_empty() {
            return;
        }
        let has_selection = self.files_search.text_anchor.zip(self.files_search.text_head)
            .is_some_and(|(anchor, head)| anchor != head);
        let same_row: Vec<_> = hits.iter().enumerate()
            .filter_map(|(index, (row, ..))| (*row == caret_row).then_some(index)).collect();
        let mut in_row = None;
        let mut last_end = 0;
        for (ordinal, index) in same_row.iter().copied().enumerate() {
            let highlight = &hits[index].3;
            if last_end <= caret.byte && caret.byte < highlight.start {
                in_row = if id == 43047 { ordinal.checked_sub(1).and_then(|at| same_row.get(at)).copied() }
                    else if ordinal == 0 && highlight.end >= 2048 { None }
                    else { Some(index) };
                break;
            }
            if highlight.start <= caret.byte && caret.byte <= highlight.end {
                in_row = if id == 43047 {
                    if caret.byte == highlight.end && caret.byte != highlight.start && !has_selection {
                        Some(index)
                    } else { ordinal.checked_sub(1).and_then(|at| same_row.get(at)).copied() }
                } else if caret.byte == highlight.start && !has_selection {
                    Some(index)
                } else { same_row.get(ordinal + 1).copied() };
                break;
            }
            if ordinal + 1 == same_row.len() && highlight.end < caret.byte && id == 43047 {
                in_row = Some(index);
            }
            last_end = highlight.end;
        }
        let mut index = in_row.unwrap_or_else(|| if id == 43047 {
            hits.iter().rposition(|(row, ..)| row_key(*row) < current_key).unwrap_or(hits.len() - 1)
        } else {
            hits.iter().position(|(row, ..)| row_key(*row) > current_key).unwrap_or(0)
        });
        if id == 43046 && matches!(caret_row, Row::Search(_) | Row::Document(_, _)) && hits[index].3.end >= 2048 {
            let skipped = row_key(hits[index].0);
            index = hits.iter().position(|(row, ..)| row_key(*row) > skipped).unwrap_or(0);
        }
        let (row, path, range, highlight) = hits.swap_remove(index);
        if self.files_search.results.searches[search_index].collapsed {
            self.files_search
                .results
                .toggle_row(Row::Search(search_index));
        }
        if let Row::Line(_, doc, _) = row
            && self.files_search.results.searches[search_index].documents[doc].collapsed
        {
            self.files_search
                .results
                .toggle_row(Row::Document(search_index, doc));
        }
        if let Some(caption) = self.files_search.results.caption_row(row) {
            let end = highlight.end.min(caption.len());
            if highlight.start < end && caption.get(highlight.start..end).is_some() {
                self.files_search.text_anchor = Some(TextPoint { row: Some(row), byte: highlight.start });
                self.files_search.text_head = Some(TextPoint { row: Some(row), byte: end });
            } else if row != caret_row {
                let byte = if id == 43047 { caption.len() } else { 0 };
                let point = TextPoint { row: Some(row), byte };
                self.files_search.text_anchor = Some(point);
                self.files_search.text_head = Some(point);
            }
        }
        self.files_search.results_cursor_x = None;
        self.files_search.results_width_item = None;
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        if let Some(index) = self
            .files_search
            .results
            .rows
            .iter()
            .position(|candidate| *candidate == row)
        {
            self.files_search.results_selected = index;
            self.reveal_finder_row(index, gpui::ScrollStrategy::Center);
        }
        self.files_search.results_visible = true;
        self.open_result_target(path, range, window, cx);
    }

    fn reveal_finder_row(&self, index: usize, strategy: gpui::ScrollStrategy) {
        self.files_search
            .results_scroll
            .scroll_to_item(index, strategy);
        if self.files_search.results_wrap {
            if matches!(strategy, gpui::ScrollStrategy::Top) {
                self.files_search
                    .wrapped_scroll
                    .scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: px(0.),
                    });
            } else {
                self.files_search
                    .wrapped_scroll
                    .scroll_to_reveal_item(index);
            }
        }
    }
    pub(super) fn search_history_input(
        &self,
        field: crate::search_history::Field,
    ) -> gpui::Entity<text_field::TextField> {
        match field {
            crate::search_history::Field::Find => self.find_query.clone(),
            crate::search_history::Field::Replace => self.replace_input.clone(),
            crate::search_history::Field::Filter => self.files_search.filters.clone(),
            crate::search_history::Field::Directory => self.files_search.directory.clone(),
        }
    }

    pub(super) fn persist_find_option(
        &mut self,
        key: &'static str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> bool {
        self.persist_find_options(&[(key, value)], cx)
    }

    fn persist_find_options(
        &mut self,
        options: &[(&'static str, serde_json::Value)],
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(store) = &self.settings_store else {
            return true;
        };
        if let Err(error) = store.update_settings(|settings| {
            if !settings["FindHistory"].is_null() && !settings["FindHistory"].is_object() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "FindHistory must be an object",
                ));
            }
            for (key, value) in options {
                settings["FindHistory"][*key] = value.clone();
            }
            Ok(())
        }) {
            self.set_find_status(find_surface::FindStatus::NotFound,
                format!("Search settings could not be saved: {error}"));
            self.notify_editor_error(self.find_message.clone(), cx);
            return false;
        }
        true
    }

    pub(super) fn set_find_project_panels(&mut self, panels: [bool; 3], cx: &mut Context<Self>) -> bool {
        if self.find_project_panels == panels { return true; }
        let keys = ["fifProjectPanel1", "fifProjectPanel2", "fifProjectPanel3"];
        let options = std::array::from_fn::<_, 3, _>(|index| (keys[index], panels[index].into()));
        if !self.persist_find_options(&options, cx) { return false; }
        self.find_project_panels = panels;
        cx.notify();
        true
    }

    pub(super) fn sync_find_project_panels(&mut self, cx: &mut Context<Self>) {
        let panels = std::array::from_fn(|index| self.find_project_panels[index] && self.projects.visible(index));
        self.set_find_project_panels(panels, cx);
    }

    fn finder_style(&self, id: u16) -> crate::syntax::StyleAppearance {
        let base = self.style_config.appearance(None, 32, self.dark, &crate::syntax::StyleAppearance::plain(self.dark), 0);
        if id == 0 { return base; }
        let language = crate::languages::catalog().ok().and_then(|catalog| catalog.profiles().iter().find(|language| language.name == "searchResult"));
        self.style_config.appearance(language, id, self.dark, &base, 0)
    }

    fn finder_row_style(&self, row: crate::finder::Row) -> crate::syntax::StyleAppearance {
        self.finder_style(match row { crate::finder::Row::Search(_) => 1, crate::finder::Row::Document(..) => 2, _ => 0 })
    }

    fn finder_metric_runs(&self) -> finder_layout::Runs {
        [0, 1, 2, 3, 4].into_iter().map(|id| {
            let style = self.finder_style(id);
            let mut font = gpui::font(style.font_family);
            if style.bold { font = font.bold(); }
            if style.italic { font = font.italic(); }
            (gpui::TextRun {
                len: 2, font, color: rgb(style.foreground).into(), background_color: None,
                underline: None, strikethrough: None,
            }, px(style.font_size as f32 * 96. / 72.))
        }).collect()
    }

    fn finder_line_height(&self, window: &Window) -> gpui::Pixels {
        finder_layout::metrics(&self.finder_metric_runs(), window).0
    }

    fn invalidate_finder_style_layout(&mut self) {
        self.files_search.caret_affinity = None;
        self.files_search.layout_cache.borrow_mut().clear();
        self.files_search.results_width_item = None;
        self.files_search.results_width = px(0.);
        self.files_search.results_cursor_x = None;
        self.files_search.wrapped_preferred_x = None;
        let top = self.files_search.wrapped_scroll.logical_scroll_top();
        self.files_search.wrapped_scroll.reset(self.files_search.results.rows.len());
        self.files_search.wrapped_scroll.scroll_to(top);
    }

    fn finder_text_runs(&self, row: crate::finder::Row, caption: &str) -> finder_layout::Runs {
        let base_id = match row { crate::finder::Row::Search(_) => 1, crate::finder::Row::Document(..) => 2, _ => 0 };
        let mut spans = Vec::new();
        if let crate::finder::Row::Line(search, doc, line) = row {
            if let Some(start) = caption.find(|ch: char| ch.is_ascii_digit()) && let Some(end) = caption.find(':') && start < end { spans.push((start..end, 3)); }
            for range in &self.files_search.results.searches[search].documents[doc].lines[line].highlights {
                let end = range.end.min(caption.len());
                if range.start < end { spans.push((range.start..end, 4)); }
            }
        }
        let mut cuts = vec![0, caption.len()];
        for (range, _) in &spans { cuts.extend([range.start, range.end]); }
        cuts.retain(|byte| *byte <= caption.len() && caption.is_char_boundary(*byte));
        cuts.sort_unstable();
        cuts.dedup();
        cuts.windows(2).map(|pair| {
            let id = spans.iter().rev().find(|(range, _)| range.contains(&pair[0])).map_or(base_id, |(_, id)| *id);
            let style = self.finder_style(id);
            let foreground = style.foreground;
            let background = if id != base_id { Some(style.background) } else { None };
            let mut font = gpui::font(style.font_family);
            if style.bold { font = font.bold(); }
            if style.italic { font = font.italic(); }
            (gpui::TextRun {
                len: pair[1] - pair[0], font, color: rgb(foreground).into(), background_color: background.map(|colour| rgb(colour).into()),
                underline: style.underline.then_some(gpui::UnderlineStyle { color: Some(rgb(foreground).into()), thickness: px(1.), wavy: false }), strikethrough: None,
            }, px(style.font_size as f32 * 96. / 72.))
        }).collect()
    }

    fn finder_layout_source(&self, row: crate::finder::Row, caption: &str) -> std::rc::Rc<finder_layout::Source> {
        self.files_search.layout_cache.borrow_mut().source(
            row, caption, self.finder_text_runs(row, caption), self.finder_metric_runs(),
            (self.style_config.revision(), self.dark),
        )
    }

    fn finder_layout(&self, index: usize, wrapped: bool, window: &Window) -> Option<std::rc::Rc<finder_layout::FinderLayout>> {
        let row = *self.files_search.results.rows.get(index)?;
        let caption = self.files_search.results.caption(index)?;
        let width = wrapped.then(|| (self.files_search.wrapped_scroll.viewport_bounds().size.width - px(8.)).max(px(1.)));
        self.finder_layout_source(row, &caption).layout(width, window)
    }

    pub(super) fn configure_finder(&mut self, settings: &serde_json::Value) {
        self.find_project_panels = ["fifProjectPanel1", "fifProjectPanel2", "fifProjectPanel3"]
            .map(|key| settings["FindHistory"][key].as_bool().unwrap_or(false));
        self.files_search.recursive = settings["FindHistory"]["fifRecuisive"]
            .as_bool()
            .unwrap_or(true);
        self.files_search.hidden_directories = settings["FindHistory"]["fifInHiddenFolder"]
            .as_bool()
            .unwrap_or(false);
        self.files_search.ignore_unsaved_changes = settings["FinderConfig"]["FiF_ignoreunsavedChangesInOpenedFiles"].as_bool().unwrap_or(false);
        self.files_search.always_visible = settings["Searching"]["findDlgAlwaysVisible"]
            .as_bool()
            .unwrap_or(false);
        self.files_search.results_purge = settings["FinderConfig"]["purgeBeforeEverySearch"]
            .as_bool()
            .unwrap_or(false);
        self.files_search.results_wrap =
            settings["FinderConfig"]["wrap"].as_bool().unwrap_or(false);
        self.files_search.results_one_per_line =
            settings["FinderConfig"]["showOnlyOneEntryPerFoundLine"]
                .as_bool()
                .unwrap_or(true);
    }

    pub(super) fn find_all_documents(
        &mut self,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.find_all_documents(all, window, cx)
        }) {
            return;
        }
        if !self.find_visible
            || self.find_replace_mode
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
            || self.files_search.cancel.is_some()
        {
            return;
        }
        self.files_search.created = true;
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        self.record_search_history(crate::search_history::Field::Find, window, cx);
        let ids = if all {
            let mut seen = std::collections::BTreeSet::new();
            [
                crate::workspace::ViewId::Primary,
                crate::workspace::ViewId::Secondary,
            ]
            .into_iter()
            .flat_map(|pane| self.workspace.view_buffer_ids(pane).iter().copied())
            .filter(|id| seen.insert(*id))
            .collect::<Vec<_>>()
        } else {
            vec![self.workspace.active().id()]
        };
        let mut documents = Vec::new();
        let mut identities: Vec<crate::editor::Document> = Vec::new();
        for id in ids {
            let Some(buffer) = self
                .workspace
                .buffers()
                .iter()
                .find(|buffer| buffer.id() == id)
            else {
                continue;
            };
            if buffer.editor.has_composition() {
                self.set_find_status(find_surface::FindStatus::NotFound,
                    "Find All during active document composition is not implemented yet");
                cx.notify();
                return;
            }
            let path = buffer
                .disk_file
                .as_ref()
                .map(|file| file.path().to_path_buf());
            if identities.iter().any(|identity| identity.shares_document_with(buffer.editor.document())) { continue; }
            let identity = identities.len();
            identities.push(buffer.editor.document().clone_view());
            documents.push(crate::finder::SearchDocument {
                identity: Some(identity),
                regex_state: buffer.editor.document().regex_state(),
                document: crate::finder::Document {
                    name: path
                        .as_ref()
                        .map_or_else(|| buffer.name(), |path| path.display().to_string()),
                    bytes: buffer.editor.document().byte_snapshot(),
                    selection: (!all && self.find_in_selection_active)
                        .then(|| self.editor().document().selection()),
                },
            });
        }
        let mut updates = Vec::new();
        let result = crate::finder::search_documents_with_state(
            &query,
            documents,
            self.find_options,
            self.files_search.results_one_per_line,
            &mut updates,
        );
        self.write_search_regex_states(&identities, updates);
        match result {
            Ok(search) => {
                self.set_find_status(find_surface::FindStatus::NoMessage, "");
                self.publish_search_results(search, window, cx);
            }
            Err(error) => {
                self.set_find_error(&error);
                cx.notify();
            }
        }
    }

    fn publish_search_results(
        &mut self,
        mut search: crate::finder::Search,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_search.created = true;
        self.set_find_status(find_surface::FindStatus::NoMessage, "");
        self.activate_main_finder(cx);
        self.cancel_finder_drag();
        let hits = search.hits;
        let search_error = search.search_error.take();
        self.files_search
            .results
            .push(search, self.files_search.results_purge);
        self.files_search.results_selected = 0;
        self.files_search.text_anchor = None;
        self.files_search.text_head = None;
        self.files_search.context = None;
        self.files_search.results_width_item = None;
        self.files_search.results_width = px(0.);
        self.files_search.results_cursor_x = None;
        self.files_search.wrapped_preferred_x = None;
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.reveal_finder_row(0, gpui::ScrollStrategy::Top);
        self.files_search.results_visible = true;
        self.files_search.message = format!(
            "Search results - ({} {})",
            crate::finder::commafy(hits),
            if hits == 1 { "hit" } else { "hits" }
        );
        if hits > 0 {
            if search_error.is_none() && !self.files_search.always_visible {
                self.close_search_history();
                self.find_visible = false;
                self.files_search.visible = false;
            }
            if !self.close_dialog {
                window.activate_window();
                self.files_search.results_focus.focus(window, cx);
            }
        }
        if let Some(error) = search_error { self.set_find_status(find_surface::FindStatus::NotFound, error); }
        cx.notify();
    }

    pub(super) fn show_find_in_directory(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed = directory.clone();
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.show_find_in_directory(routed, window, cx)
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
        if self.editor().marked_range().is_some() {
            self.show_find_in_files(window, cx);
            return;
        }
        self.show_find_in_files(window, cx);
        if self.files_search.visible {
            let directory = directory.to_string_lossy().into_owned();
            self.files_search
                .directory
                .update(cx, |field, cx| field.set_text(&directory, window, cx));
            cx.notify();
        }
    }

    pub(super) fn show_find_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find_in_files(true, window, cx);
    }

    pub(super) fn open_find_in_files(
        &mut self,
        prefill: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_search_tab(prefill, false, window, cx);
    }

    pub(super) fn open_file_search_tab(
        &mut self,
        prefill: bool,
        projects: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.open_file_search_tab(prefill, projects, window, cx)
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
        if self.editor().marked_range().is_some() {
            self.notify_editor_error(
                "Find in Files during active composition is not implemented yet".into(),
                cx,
            );
            return;
        }
        self.close_search_history();
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.find_visible = false;
        self.find_project_mode = projects;
        self.sync_find_project_panels(cx);
        self.initialize_search_history(window, cx);
        if self.pref_bool("Searching.fillDirFieldFromActiveDoc", false)
            && let Some(parent) = self
                .workspace
                .active()
                .disk_file
                .as_ref()
                .and_then(|file| file.path().parent())
        {
            let directory = parent.to_string_lossy().into_owned();
            self.files_search
                .directory
                .update(cx, |field, cx| field.set_text(&directory, window, cx));
        }
        self.files_search.visible = true;
        self.find_created = true;
        if prefill {
            self.prepare_find_activation(cx);
        }
        self.present_floating(floating_windows::DialogKind::Find, window, cx);
        if prefill {
            self.fill_find_query(window, cx);
        }
        cx.notify();
    }

    pub(super) fn start_file_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_file_operation(None, window, cx);
    }

    fn confirm_file_replacement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.confirm_file_replacement(window, cx)
        }) {
            return;
        }
        if self.file_busy || self.files_search.cancel.is_some() {
            return;
        }
        self.set_find_status(find_surface::FindStatus::NoMessage, "");
        self.record_search_history(crate::search_history::Field::Filter, window, cx);
        let directory = self.captured_search_input(crate::search_history::Field::Directory, cx);
        if !self.find_project_mode && directory.is_empty() {
            self.focus_search_history_input(crate::search_history::Field::Directory, cx);
            cx.notify();
            return;
        }
        let filters = self.captured_search_input(crate::search_history::Field::Filter, cx);
        let message = if self.find_project_mode {
            "您是否要替换所选项目面板中所有文档中的所有匹配项？".to_owned()
        } else {
            format!("您是否要替换下列范围内的所有匹配项:\n\n{directory}\n\n对这些文件类型:\n\n{}",
                if filters.is_empty() { "*.*" } else { &filters })
        };
        let response = window.prompt(
            gpui::PromptLevel::Warning,
            if self.find_project_mode {
                "您确定吗？"
            } else {
                "确认"
            },
            Some(&message),
            &[
                gpui::PromptButton::ok("确定"),
                gpui::PromptButton::cancel("取消"),
            ],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            match response.await {
                Ok(0) => if let Err(error) = view.update_in(cx, |view, window, cx| {
                    let replacement =
                        view.captured_search_input(crate::search_history::Field::Replace, cx);
                    view.start_file_operation(Some(replacement), window, cx);
                }) { eprintln!("file replacement confirmation lost its document: {error}"); },
                Ok(_) => {},
                Err(error) => eprintln!("file replacement confirmation failed: {error}"),
            }
        })
        .detach();
    }

    pub(super) fn files_search_replace_message(&mut self, message: String, cx: &mut Context<Self>) {
        self.set_file_search_status(find_surface::FindStatus::Message, message);
        cx.notify();
    }

    fn set_file_search_status(&mut self, status: find_surface::FindStatus, message: impl Into<String>) {
        self.set_find_status(status, message);
    }

    pub(super) fn toggle_file_search_option(&mut self, id: u32, cx: &mut Context<Self>) {
        if !self.files_search.visible || self.find_controls.compact { return; }
        if matches!(id, 1662..=1664) {
            let index = (id - 1662) as usize;
            if !self.find_project_mode || !self.projects.visible(index) { return; }
            let mut panels = self.find_project_panels;
            panels[index] = !panels[index];
            self.set_find_project_panels(panels, cx);
            return;
        }
        if self.find_project_mode { return; }
        let (key, previous) = match id {
            1658 => ("fifRecuisive", self.files_search.recursive),
            1659 => ("fifInHiddenFolder", self.files_search.hidden_directories),
            _ => return,
        };
        if self.persist_find_option(key, (!previous).into(), cx) {
            if id == 1658 { self.files_search.recursive = !previous; }
            else { self.files_search.hidden_directories = !previous; }
        }
        cx.notify();
    }

    pub(super) fn file_search_button_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(id, "files-find-all" | "files-replace-all") && !self.file_search_operation_enabled() { return; }
        match id {
            "files-find-all" => self.start_file_search(window, cx),
            "files-replace-all" => self.confirm_file_replacement(window, cx),
            "files-browse" if !self.find_project_mode && !self.find_controls.compact => self.browse_search_directory(window, cx),
            "files-directory-from-document" if !self.find_project_mode && !self.find_controls.compact => self.use_document_search_directory(window, cx),
            "files-close" => { if !self.cancel_active_file_search(cx) { self.dismiss_floating(floating_windows::DialogKind::Find, window, cx); } }
            _ => {}
        }
    }

    fn file_search_operation_enabled(&self) -> bool {
        !self.file_busy && self.files_search.cancel.is_none() && (!self.find_project_mode || self.find_project_panels.iter().enumerate()
            .any(|(index, selected)| *selected && self.projects.visible(index)))
    }

    pub(super) fn cancel_active_file_search(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(cancel) = &self.files_search.cancel {
            cancel.store(true, Ordering::Relaxed);
            self.set_file_search_status(find_surface::FindStatus::Message, "正在停止…");
            cx.notify();
            true
        } else {
            false
        }
    }

    fn start_file_operation(
        &mut self,
        replacement: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let routed_replacement = replacement.clone();
        if self.route_to_document(window, cx, |view, window, cx| {
            view.start_file_operation(routed_replacement, window, cx)
        }) {
            return;
        }
        if !self.files_search.visible {
            return;
        }
        if self.files_search.cancel.is_some()
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.set_find_status(find_surface::FindStatus::NoMessage, "");
        let query = self.captured_search_input(crate::search_history::Field::Find, cx);
        self.record_search_histories(
            &[
                crate::search_history::Field::Filter,
                crate::search_history::Field::Find,
            ],
            window,
            cx,
        );
        let directory = self.captured_search_input(crate::search_history::Field::Directory, cx);
        if directory.is_empty() && !self.find_project_mode {
            self.focus_search_history_input(crate::search_history::Field::Directory, cx);
            cx.notify();
            return;
        }
        if !self.find_project_mode {
            self.record_search_history(crate::search_history::Field::Directory, window, cx);
            self.files_search.directory.update(cx, |field, cx| field.set_text(&directory, window, cx));
        }
        if self
            .workspace
            .buffers()
            .iter()
            .any(|buffer| buffer.editor.marked_range().is_some())
        {
            self.set_file_search_status(find_surface::FindStatus::NotFound,
                "Search with active document composition is not implemented yet");
            cx.notify();
            return;
        }
        let filters = self.captured_search_input(crate::search_history::Field::Filter, cx);
        let filters = if filters.split_ascii_whitespace().next().is_none() {
            self.files_search.filters.update(cx, |field, cx| field.set_text("*.*", window, cx));
            "*.*".to_owned()
        } else { filters };
        let request = FileSearchRequest {
            mode: self.find_options.mode,
            directory: PathBuf::from(directory),
            filters,
            query,
            match_case: self.find_options.match_case,
            whole_word: self.find_options.whole_word,
            recursive: self.files_search.recursive,
            hidden_directories: self.files_search.hidden_directories,
            open_documents: Default::default(),
        };
        let (identities, mut opened) = self.capture_open_search_documents();
        let project_paths = if self.find_project_mode {
            let mut paths = Vec::new();
            for (index, selected) in self.find_project_panels.iter().enumerate() {
                if *selected
                    && self.projects.visible(index)
                    && let Ok(panel) = self.projects.model.panel(index)
                {
                    paths.extend(panel.files().into_iter().filter(|path| {
                        crate::find_in_files::matches_filters(path, &request.filters)
                    }));
                }
            }
            if !self
                .find_project_panels
                .iter()
                .enumerate()
                .any(|(index, selected)| *selected && self.projects.visible(index))
            {
                self.set_file_search_status(find_surface::FindStatus::NotFound, "请选择要搜索的项目面板");
                cx.notify();
                return;
            }
            Some(paths)
        } else {
            None
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let query = request.query.clone();
        let options = self.find_options;
        let replacing = replacement.is_some();
        let one_per_line = self.files_search.results_one_per_line;
        self.files_search.cancel = Some(cancel.clone());
        self.file_busy = true;
        if replacing {
            self.record_search_history(crate::search_history::Field::Replace, window, cx);
        }
        self.set_file_search_status(find_surface::FindStatus::Message, if replacing {
            "正在替换文件…"
        } else {
            "正在查找文件…"
        });
        let progress = Arc::new(crate::find_in_files::FileSearchProgress::new(replacing, project_paths.is_none()));
        self.files_search.progress = Some(progress.clone());
        self.refresh_file_search_progress(progress.clone(), cx);
        let discovery_request = request.clone();
        let discovery_cancel = cancel.clone();
        let discovery_progress = progress.clone();
        let task = cx.background_executor().spawn(async move {
            if let Some(paths) = project_paths {
                crate::find_in_files::FileCandidates { paths, ..Default::default() }
            } else {
                crate::find_in_files::discover_files(&discovery_request, &discovery_cancel, Some(&discovery_progress))
            }
        });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let candidates = task.await;
            if candidates.cancelled || cancel.load(Ordering::Relaxed) || (candidates.paths.is_empty() && !candidates.errors.is_empty()) {
                if let Err(error) = view.update(cx, |view, cx| {
                    view.files_search.cancel = None;
                    view.files_search.progress = None;
                    view.file_busy = false;
                    view.set_file_search_status(if candidates.errors.is_empty() { find_surface::FindStatus::NoMessage } else { find_surface::FindStatus::NotFound }, candidates.errors.join("；"));
                    cx.notify();
                }) { eprintln!("file discovery completion lost its document: {error}"); }
                return;
            }
            let total = candidates.paths.len();
            progress.record(std::path::Path::new(""), 0, total, 0, false);
            if let Err(error) = view.update(cx, |view, cx| {
                if !replacing {
                    view.files_search.created = true;
                    view.activate_main_finder(cx);
                    view.cancel_finder_drag();
                    view.files_search.results.begin(view.files_search.results_purge);
                    view.files_search.text_anchor = None;
                    view.files_search.text_head = None;
                    view.files_search.results_width_item = None;
                    view.files_search.results_width = px(0.);
                    view.files_search.results_selected = 0;
                    view.files_search.results_cursor_x = None;
                    view.files_search.wrapped_preferred_x = None;
                    view.files_search.wrapped_scroll.reset(view.files_search.results.rows.len());
                    view.reveal_finder_row(0, gpui::ScrollStrategy::Top);
                    view.files_search.results_visible = true;
                }
                cx.notify();
            }) { eprintln!("file search could not begin in its document: {error}"); return; }
            let results = if let Some(replacement) = replacement {
                let mut results = crate::find_in_files::FileReplaceResults { errors: candidates.errors, ..Default::default() };
                for (index, path) in candidates.paths.into_iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) { results.cancelled = true; break; }
                    let opened = match view.update(cx, |view, _| view.replace_open_file_search_target(&path, &query, &replacement, options)) {
                        Ok(result) => result,
                        Err(error) => { results.errors.push(error.to_string()); break; }
                    };
                    let mut replaced = if let Some(result) = opened { result } else {
                        let request = request.clone();
                        let replacement = replacement.clone();
                        let cancel = cancel.clone();
                        let disk_path = path.clone();
                        cx.background_executor().spawn(async move {
                            crate::find_in_files::replace_file_list(&request, vec![disk_path], &replacement, &cancel)
                        }).await
                    };
                    results.replaced += replaced.replaced;
                    results.cancelled |= replaced.cancelled;
                    results.errors.append(&mut replaced.errors);
                    progress.record(&path, index + 1, total, results.replaced, false);
                    if let Some(error) = replaced.search_error { results.search_error = Some(error); break; }
                }
                FileOperationResult::Replace(results)
            } else {
                let progress = progress.clone();
                let (result, updates) = cx.background_executor().spawn(async move {
                    let mut updates = Vec::new();
                    let mut result = crate::find_in_files::search_file_list_with_state(request, candidates.paths, &cancel, Some(&progress), &mut opened, &mut updates);
                    result.errors.extend(candidates.errors);
                    (result, updates)
                }).await;
                FileOperationResult::Search(crate::finder::from_file_search(&query, result, options, one_per_line), updates)
            };
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.files_search.cancel = None;
                    view.files_search.progress = None;
                    view.file_busy = false;
                    match results {
                        FileOperationResult::Search(results, updates) => {
                            view.write_search_regex_states(&identities, updates);
                            view.publish_search_results(results, window, cx)
                        }
                        FileOperationResult::Replace(results) => {
                            view.editor_changed(Ok(()), window, cx);
                            if let Some(error) = results.search_error { view.set_file_search_status(find_surface::FindStatus::NotFound, error); }
                            else { view.files_search_replace_message(format!("在文件中替换: 已替换 {} 个匹配项。{}{}", results.replaced,
                                if results.cancelled { "已取消。" } else { "" }, results.errors.join("；")), cx); }
                        }
                    }
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("file search completion could not reach its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn replace_open_file_search_target(&mut self, path: &std::path::Path, query: &str, replacement: &str, options: crate::search::LiteralOptions) -> Option<crate::find_in_files::FileReplaceResults> {
        if self.files_search.ignore_unsaved_changes { return None; }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let buffer = self.workspace.buffers_mut().find(|buffer| buffer.disk_file.as_ref().is_some_and(|file| file.path() == canonical))?;
        let mut results = crate::find_in_files::FileReplaceResults::default();
        if buffer.editor.document().len_bytes() == 0 { return Some(results); }
        let mut editor = buffer.editor.clone_view();
        match editor.replace_all_literal(query, replacement, crate::search::LiteralOptions { wrap: true, ..options }) {
            Ok(0) => {}
            Ok(count) => {
                results.replaced = count;
                let intent = buffer.save_intent();
                if let Some(file) = &buffer.disk_file {
                    match file.save_with_format(intent.snapshot(), intent.format()) {
                        Ok(file) => {
                            if let Err(error) = self.workspace.complete_save(&intent, file, crate::workspace::SaveCompletion::Primary) {
                                results.errors.push(format!("{}: {error}", path.display()));
                            }
                        }
                        Err(error) => results.errors.push(format!("{}: 替换已保留在编辑器，但保存失败: {error}", path.display())),
                    }
                }
            }
            Err(crate::search::ReplaceError::Search(error)) => results.search_error = Some(error.to_string()),
            Err(error) => results.errors.push(format!("{}: {error}", path.display())),
        }
        Some(results)
    }

    fn refresh_file_search_progress(&self, progress: Arc<crate::find_in_files::FileSearchProgress>, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |view, cx| {
            let mut previous = progress.snapshot();
            loop {
                executor.timer(std::time::Duration::from_millis(50)).await;
                match view.update(cx, |view, cx| {
                    if !cx.windows().contains(&view.document_window) || !view.files_search.progress.as_ref().is_some_and(|current| Arc::ptr_eq(current, &progress)) { return false; }
                    let snapshot = progress.snapshot();
                    if snapshot != previous { previous = snapshot; cx.notify(); }
                    true
                }) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => { eprintln!("file search progress lost its document: {error}"); break; }
                }
            }
        }).detach();
    }

    fn use_document_search_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        let directory = self
            .workspace
            .active()
            .disk_file
            .as_ref()
            .filter(|file| file.path().is_file())
            .and_then(|file| file.path().parent())
            .filter(|path| path.is_dir())
            .map(|path| path.to_string_lossy().into_owned());
        let Some(directory) = directory else { return };
        self.close_search_history();
        self.files_search
            .directory
            .update(cx, |field, cx| field.set_text(&directory, window, cx));
        cx.notify();
    }

    fn browse_search_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| {
            view.browse_search_directory(window, cx)
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
        self.file_busy = true;
        self.close_search_history();
        let initial = PathBuf::from(self.files_search.directory.read(cx).text());
        cx.spawn_in(window, async move |view, cx| {
            let result = choose_search_directory(&initial);
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                match result {
                    Ok(Some(path)) => {
                        if path.is_dir() {
                            let directory = path.to_string_lossy().into_owned();
                            view.files_search
                                .directory
                                .update(cx, |field, cx| field.set_text(&directory, window, cx));
                        }
                    }
                    Ok(None) => {}
                    Err(error) => view.set_file_search_status(find_surface::FindStatus::NotFound,
                        format!("选择目录失败: {error}")),
                }
                cx.notify();
            }) {
                eprintln!("search directory picker lost its document: {error}");
            }
        })
        .detach();
        cx.notify();
    }

    fn open_result_target(
        &mut self,
        path: PathBuf,
        range: std::ops::Range<ByteOffset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
        {
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let opened = self
            .workspace
            .buffers()
            .iter()
            .find(|buffer| {
                buffer.disk_file.as_ref().map_or_else(
                    || std::path::Path::new(&buffer.name()) == path,
                    |file| file.path() == canonical,
                )
            })
            .map(|buffer| buffer.id());
        if let Some(id) = opened {
            if self.activate_buffer(id, window, cx) {
                let result = self.select_search_result(range, false, window, cx);
                self.editor_changed(result, window, cx);
            }
            return;
        }
        self.file_busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { crate::files::DiskFile::load_byte_buffer(&path) });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.file_busy = false;
                    match result {
                        Ok((file, bytes)) => {
                            if !view.cancel_view_composition(window, cx) {
                                return;
                            }
                            match view.workspace.open_buffer_bytes(file, bytes) {
                                Ok(_) => {
                                    view.reset_active_view(window, cx);
                                    let result = view.select_search_result(range, false, window, cx);
                                    view.editor_changed(result, window, cx);
                                }
                                Err(error) => view.notify_editor_error(error.to_string(), cx),
                            }
                        }
                        Err(error) => view
                            .notify_editor_error(format!("Open search result failed: {error}"), cx),
                    }
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("search navigation completion could not reach its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_find_in_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use super::dialog_font::px;
        if !self.files_search.visible {
            return None;
        }
        let busy = self.files_search.cancel.is_some();
        let operation_enabled = self.file_search_operation_enabled();
        Some(
            div()
                .id("find-in-files-dialog")
                .debug_selector(|| "find-in-files-dialog".into())
                .role(Role::Dialog)
                .aria_label(if self.find_project_mode {
                    "项目中查找"
                } else {
                    "文件中查找"
                })
                .track_focus(&self.files_search.focus)
                .key_context("MatrixFindInFiles")
                .size_full()
                .relative()
                .font_family(if self.editing_find_monospaced() { "Menlo".to_owned() } else { dialog_font::font_family() })
                .text_size(px(13.))
                .line_height(px(18.))
                .bg(rgb(0xffffff))
                .text_color(rgb(0x202020))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.render_search_tabs(cx))
                .child(self.render_find_swap(cx))
                .child(find_surface::input_row(
                    "查找目标(F):",
                    38.,
                    self.search_history_combo(crate::search_history::Field::Find, cx),
                ))
                .child(find_surface::input_row(
                    "替换为(L):",
                    69.,
                    self.search_history_combo(crate::search_history::Field::Replace, cx),
                ))
                .when(!self.find_controls.compact, |dialog| dialog.child(find_surface::input_row(
                    "文件类型(S):", 103., self.search_history_combo(crate::search_history::Field::Filter, cx))))
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(div().id("files-filter-tip")
                        .absolute().left(px(440.)).top(px(103.)).h(px(20.))
                        .text_color(rgb(0x0078ff)).child("(?)")
                        .tooltip(|_, cx| icon_tooltip::view(
                            "示例：在 cpp、cxx、h、hxx 和 hpp 中查找:\n*.cpp *.cxx *.h *.hxx *.hpp\n\n查找除 exe、obj 和 log 以外的所有文件:\n*.* !*.exe !*.obj !*.log\n\n查找所有文件但排除文件夹 tests、bin 和 bin64:\n*.* !\\tests !\\bin*\n\n查找所有文件但递归式排除所有叫 log 或 logs 的文件夹:\n*.* !+\\log*",
                            false, cx)))
                })
                .when(!self.find_project_mode && !self.find_controls.compact, |dialog| {
                    dialog.child(find_surface::input_row_with_label_width(
                        "目录(E):",
                        134.,
                        81.,
                        self.search_history_combo(crate::search_history::Field::Directory, cx),
                    ))
                })
                .when(!self.find_project_mode && !self.find_controls.compact, |dialog| {
                    dialog.child(
                        find_surface::source_button("files-browse", "...", 133.)
                            .right(px(258.))
                            .w(px(26.))
                            .aria_label("选择要搜索的文件夹")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.browse_search_directory(window, cx)
                            })),
                    )
                })
                .when(!self.find_project_mode && !self.find_controls.compact, |dialog| {
                    dialog.child(
                        find_surface::source_button("files-directory-from-document", "<<", 133.)
                            .right(px(222.))
                            .w(px(26.))
                            .aria_label("使用当前文档所在目录填充目录字段")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.use_document_search_directory(window, cx)
                            })),
                    )
                })
                .when(
                    !self.find_project_mode && !self.find_controls.compact,
                    |dialog| {
                        dialog.child(
                            find_surface::checkbox(
                                "files-recursive",
                                "包含子目录(B)",
                                self.files_search.recursive,
                            )
                            .focusable().tab_index(7)
                            .absolute()
                            .left(px(524.))
                            .top(px(165.))
                            .on_click(cx.listener(|view, _, _, cx| view.toggle_file_search_option(1658, cx)))
                            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "space" {
                                    view.toggle_file_search_option(1658, cx);
                                    cx.stop_propagation();
                                }
                            })),
                        )
                    },
                )
                .when(
                    !self.find_project_mode && !self.find_controls.compact,
                    |dialog| {
                        dialog.child(
                            find_surface::checkbox(
                                "files-hidden",
                                "包含隐藏目录(H)",
                                self.files_search.hidden_directories,
                            )
                            .focusable().tab_index(8)
                            .absolute()
                            .left(px(524.))
                            .top(px(185.))
                            .on_click(cx.listener(|view, _, _, cx| view.toggle_file_search_option(1659, cx)))
                            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "space" {
                                    view.toggle_file_search_option(1659, cx);
                                    cx.stop_propagation();
                                }
                            })),
                        )
                    },
                )
                .when(
                    self.find_project_mode && !self.find_controls.compact,
                    |dialog| {
                        dialog.children(
                            [
                                ("project-search-1", "项目面板1"),
                                ("project-search-2", "项目面板2"),
                                ("project-search-3", "项目面板3"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (id, label))| {
                                let enabled = self.projects.visible(index);
                                find_surface::checkbox(id, label, self.find_project_panels[index] && enabled)
                                    .when(enabled, |control| control.focusable().tab_index(9 + index as isize))
                                    .text_color(rgb(if enabled { 0x202020 } else { 0x808080 }))
                                    .a11y_synthetic_children(move |builder| {
                                        if !enabled {
                                            builder.parent_node().set_disabled();
                                        }
                                    })
                                    .absolute()
                                    .left(px(524.))
                                    .top(px(141. + 20. * index as f32))
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.toggle_file_search_option(1662 + index as u32, cx);
                                    }))
                                    .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                                        if enabled && event.keystroke.key == "space" {
                                            view.toggle_file_search_option(1662 + index as u32, cx);
                                            cx.stop_propagation();
                                        }
                                    }))
                            }),
                        )
                    },
                )
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(
                        div()
                            .absolute()
                            .left(px(21.))
                            .top(px(165.))
                            .flex()
                            .flex_col()
                            .child(self.render_find_check(
                                "files-whole-word",
                                find_surface::FindCheck::WholeWord,
                                cx,
                            ))
                            .child(self.render_find_check(
                                "files-match-case",
                                find_surface::FindCheck::MatchCase,
                                cx,
                            )),
                    )
                })
                .child(
                    find_surface::source_button_enabled("files-find-all", "全部查找", 38., operation_enabled)
                        .border_color(rgb(if operation_enabled { 0x0078d7 } else { 0xadadad }))
                        .bg(rgb(if operation_enabled { 0xe1e1e1 } else { 0xcccccc }))
                        .when(!operation_enabled, |button| button.hover(|style| style.bg(rgb(0xcccccc)).border_color(rgb(0xadadad))))
                        .text_color(rgb(if operation_enabled {
                            0x202020
                        } else {
                            0x808080
                        }))
                        .a11y_synthetic_children(move |builder| {
                            if !operation_enabled {
                                builder.parent_node().set_disabled();
                            }
                        })
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if operation_enabled {
                                view.start_file_search(window, cx);
                            }
                        })),
                )
                .child(
                    find_surface::source_button_enabled(
                        "files-replace-all",
                        if self.find_project_mode {
                            "在项目中替换"
                        } else {
                            "在文件中替换"
                        },
                        70.,
                        operation_enabled,
                    )
                    .bg(rgb(if operation_enabled { 0xe1e1e1 } else { 0xcccccc }))
                    .when(!operation_enabled, |button| button.hover(|style| style.bg(rgb(0xcccccc)).border_color(rgb(0xadadad))))
                    .text_color(rgb(if operation_enabled {
                        0x202020
                    } else {
                        0x808080
                    }))
                    .a11y_synthetic_children(move |builder| {
                        if !operation_enabled {
                            builder.parent_node().set_disabled();
                        }
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if operation_enabled {
                            view.confirm_file_replacement(window, cx);
                        }
                    })),
                )
                .child(
                    find_surface::source_button(
                        "files-close",
                        if busy { "停止" } else { "关闭" },
                        101.,
                    )
                    .on_click(cx.listener(|view, _, window, cx| {
                        if view.cancel_active_file_search(cx) {
                            return;
                        }
                        view.dismiss_floating(floating_windows::DialogKind::Find, window, cx);
                    })),
                )
                .when(!self.find_controls.compact, |dialog| {
                    dialog.child(find_surface::search_mode(self.find_options.mode, &self.find_mode_focus,
                        &self.find_controls.option_focus[8], cx))
                })
                .child(self.render_find_options(cx))
                .child(self.render_find_status("files-dialog-message"))
                .into_any_element(),
        )
    }

    fn toggle_finder_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(toggled) = self.files_search.results.rows.get(index).copied() else {
            return;
        };
        let selected = self
            .files_search
            .results
            .rows
            .get(self.files_search.results_selected)
            .copied();
        let width_row = self
            .files_search
            .results_width_item
            .and_then(|index| self.files_search.results.rows.get(index))
            .copied();
        self.files_search.results.toggle(index);
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.files_search.results_width_item = width_row.and_then(|row| {
            self.files_search
                .results
                .rows
                .iter()
                .position(|candidate| *candidate == row)
        });
        if self.files_search.results_width_item.is_none() {
            self.files_search.results_width = px(0.);
        }
        let retained = selected.and_then(|selected| {
            self.files_search
                .results
                .rows
                .iter()
                .position(|row| *row == selected)
        });
        self.files_search.results_selected = retained
            .or_else(|| {
                self.files_search
                    .results
                    .rows
                    .iter()
                    .position(|row| *row == toggled)
            })
            .unwrap_or(0);
        if retained.is_none() {
            self.files_search.results_cursor_x = None;
            self.files_search.wrapped_preferred_x = None;
            self.files_search.text_anchor = None;
            self.files_search.text_head = None;
        }
        self.reveal_finder_row(
            self.files_search.results_selected,
            gpui::ScrollStrategy::Nearest,
        );
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn finder_caret(&self) -> crate::finder::TextPoint {
        self.files_search
            .text_head
            .unwrap_or_else(|| crate::finder::TextPoint {
                row: self
                    .files_search
                    .results
                    .rows
                    .get(self.files_search.results_selected)
                    .copied(),
                byte: 0,
            })
    }

    fn finder_caret_affinity(&self) -> bool {
        self.files_search.results_cursor_x.is_some() && self.files_search.caret_affinity
            .is_some_and(|(active, point, trailing)| active == self.files_search.views.active && point == self.finder_caret() && trailing)
    }

    fn sync_finder_visible_row(&mut self) {
        let mut candidate = self.finder_caret().row;
        if candidate.is_none() {
            self.files_search.results_selected = self.files_search.results.rows.len();
            return;
        }
        while let Some(row) = candidate {
            if let Some(index) = self
                .files_search
                .results
                .rows
                .iter()
                .position(|visible| *visible == row)
            {
                self.files_search.results_selected = index;
                return;
            }
            candidate = match row {
                crate::finder::Row::Line(search, doc, _) => {
                    Some(crate::finder::Row::Document(search, doc))
                }
                crate::finder::Row::Document(search, _) | crate::finder::Row::Error(search, _) => {
                    Some(crate::finder::Row::Search(search))
                }
                crate::finder::Row::Search(_) => None,
            };
        }
        self.files_search.results_selected = 0;
    }

    fn delete_finder_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.finder_caret().row else {
            return;
        };
        let Some(caret) = self.files_search.results.delete_caret(row) else {
            return;
        };
        self.files_search.text_anchor = Some(caret);
        self.files_search.text_head = Some(caret);
        self.files_search.results_cursor_x = Some(px(8.));
        self.files_search.wrapped_preferred_x = None;
        self.sync_finder_visible_row();
        self.files_search
            .wrapped_scroll
            .reset(self.files_search.results.rows.len());
        self.cancel_finder_drag();
        self.files_search.results_width_item = None;
        self.files_search.results_width = px(0.);
        self.reveal_finder_row(
            self.files_search.results_selected,
            gpui::ScrollStrategy::Nearest,
        );
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn measure_finder_width(
        &mut self,
        range: std::ops::Range<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let widest = range
            .filter_map(|index| {
                let width = self.finder_layout(index, false, window)?.width;
                Some((index, width))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((index, width)) = widest
            && width > self.files_search.results_width
        {
            self.files_search.results_width = width;
            self.files_search.results_width_item = Some(index);
            cx.notify();
        }
    }

    fn activate_finder_row(
        &mut self,
        index: usize,
        x: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.files_search.results.rows.get(index).copied() else {
            return;
        };
        self.activate_finder_result(row, x, window, cx);
    }

    fn activate_finder_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.finder_caret().row;
        if let Some(row) = row {
            self.activate_finder_result(row, self.files_search.results_cursor_x, window, cx);
        }
    }

    fn activate_finder_result(
        &mut self,
        row: crate::finder::Row,
        x: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::finder::Row;
        let Row::Line(search, doc, line) = row else {
            if let Some(index) = self
                .files_search
                .results
                .rows
                .iter()
                .position(|candidate| *candidate == row)
            {
                self.toggle_finder_row(index, window, cx);
            } else {
                self.files_search.results.toggle_row(row);
                self.files_search
                    .wrapped_scroll
                    .reset(self.files_search.results.rows.len());
                self.files_search.results_width_item = None;
                self.files_search.results_width = px(0.);
                self.files_search.results_focus.focus(window, cx);
                cx.notify();
            }
            return;
        };
        let doc = &self.files_search.results.searches[search].documents[doc];
        let line = &doc.lines[line];
        let occurrence = x
            .and_then(|x| {
                let shape = self.finder_layout_source(row, &line.caption).layout(None, window)?;
                let byte = if self.finder_caret().row == Some(row) {
                    self.finder_caret().byte
                } else {
                    shape.closest_index_for_position(gpui::point(x - px(8.), px(0.))).byte
                };
                line.highlights
                    .iter()
                    .position(|range| range.start <= byte && byte <= range.end)
            })
            .unwrap_or(0);
        let Some(range) = line.ranges.get(occurrence).cloned() else {
            return;
        };
        let path = PathBuf::from(&doc.name);
        self.open_result_target(path, range, window, cx);
    }

    fn copy_finder(&mut self, command: FinderCopy, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.context = None;
        let selection = self
            .files_search
            .text_anchor
            .zip(self.files_search.text_head);
        let text = match command {
            FinderCopy::Verbatim => selection
                .filter(|(anchor, head)| anchor != head)
                .and_then(|(anchor, head)| self.files_search.results.copy_verbatim(anchor, head)),
            FinderCopy::Lines => self
                .finder_selected_rows()
                .and_then(|(anchor, head)| self.files_search.results.copy_lines(anchor, head)),
            FinderCopy::Pathnames => self
                .finder_selected_rows()
                .and_then(|(anchor, head)| self.files_search.results.copy_pathnames(anchor, head)),
        };
        if let Some(text) = text {
            let text = if matches!(command, FinderCopy::Verbatim) {
                text.split('\0').next().unwrap_or_default().to_owned()
            } else {
                text
            };
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn finder_selected_rows(&self) -> Option<(crate::finder::Row, crate::finder::Row)> {
        if let Some((anchor, head)) = self
            .files_search
            .text_anchor
            .zip(self.files_search.text_head)
            && anchor != head
        {
            self.files_search.results.selection_rows(anchor, head)
        } else {
            self.finder_caret().row.map(|row| (row, row))
        }
    }

    fn open_finder_selected_paths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.context = None;
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.batch_editing_blocked()
        {
            self.files_search.results_focus.focus(window, cx);
            cx.notify();
            return;
        }
        let paths: Vec<_> = self
            .finder_selected_rows()
            .and_then(|(anchor, head)| self.files_search.results.selected_pathnames(anchor, head))
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect();
        if paths.is_empty() {
            self.files_search.results_focus.focus(window, cx);
            cx.notify();
            return;
        }
        if !self.cancel_view_composition(window, cx) {
            return;
        }
        let targets: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let opened = self.workspace.buffers().iter().any(|buffer| {
                    buffer.disk_file.as_ref().map_or_else(
                        || std::path::Path::new(&buffer.name()) == path,
                        |file| file.path() == path,
                    )
                });
                (path, opened)
            })
            .collect();
        self.file_busy = true;
        let task = cx.background_executor().spawn(async move {
            targets
                .into_iter()
                .map(|(path, opened)| {
                    let result = if opened {
                        Ok(None)
                    } else {
                        crate::files::DiskFile::load_byte_buffer(&path).map(Some)
                    };
                    (path, result)
                })
                .collect::<Vec<_>>()
        });
        let document_window = self.document_window;
        cx.spawn_in(window, async move |view, cx| {
            let loaded = task.await;
            match document_window.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.file_busy = false;
                    let mut errors = Vec::new();
                    let mut opened_any = false;
                    for (path, result) in loaded {
                        let existing = view
                            .workspace
                            .buffers()
                            .iter()
                            .find(|buffer| {
                                buffer.disk_file.as_ref().map_or_else(
                                    || std::path::Path::new(&buffer.name()) == path,
                                    |file| file.path() == path,
                                )
                            })
                            .map(|buffer| buffer.id());
                        if let Some(id) = existing {
                            opened_any |= view.activate_buffer(id, window, cx);
                            continue;
                        }
                        match result {
                            Ok(Some((file, bytes))) => {
                                if !view.cancel_view_composition(window, cx) {
                                    return;
                                }
                                match view.workspace.open_buffer_bytes(file, bytes) {
                                    Ok(_) => {
                                        view.reset_active_view(window, cx);
                                        opened_any = true;
                                    }
                                    Err(error) => {
                                        errors.push(format!("{}: {error}", path.display()))
                                    }
                                }
                            }
                            Ok(None) => errors.push(format!(
                                "{}: result document is no longer open",
                                path.display()
                            )),
                            Err(error) => errors.push(format!("{}: {error}", path.display())),
                        }
                    }
                    if !opened_any {
                        view.files_search.results_focus.focus(window, cx);
                    }
                    if !errors.is_empty() {
                        view.notify_editor_error(
                            format!("Open search results failed: {}", errors.join("; ")),
                            cx,
                        );
                    }
                    cx.notify();
                })
            }) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!("open selected result paths could not reach its window: {error}")
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn select_all_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_search.wrapped_preferred_x = None;
        self.files_search.text_anchor =
            self.files_search
                .results
                .rows
                .first()
                .copied()
                .map(|row| crate::finder::TextPoint {
                    row: Some(row),
                    byte: 0,
                });
        self.files_search.text_head = Some(crate::finder::TextPoint { row: None, byte: 0 });
        self.files_search.results_selected = self.files_search.results.rows.len();
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn finder_has_selection(&self) -> bool {
        self.files_search
            .text_anchor
            .zip(self.files_search.text_head)
            .is_some_and(|(anchor, head)| anchor != head)
    }

    pub(super) fn finder_copy_available(&self, window: &Window) -> bool {
        self.files_search.results_focus.is_focused(window) && self.finder_has_selection()
    }

    fn finder_command_enabled(&self, command: FinderCommand) -> bool {
        if self.files_search.views.active != 0
            && matches!(
                command,
                FinderCommand::ClearAll | FinderCommand::TogglePurge
            )
        {
            return false;
        }
        if self.files_search.views.active == 0 && matches!(command, FinderCommand::CloseResults) {
            return false;
        }
        !matches!(command, FinderCommand::Copy(FinderCopy::Verbatim)) || self.finder_has_selection()
    }

    fn run_finder_command(
        &mut self,
        command: FinderCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files_search.context = None;
        match command {
            FinderCommand::FindWithin => self.show_find_in_results(window, cx),
            FinderCommand::CloseResults => self.close_active_finder(window, cx),
            FinderCommand::OpenSelectedPathnames => self.open_finder_selected_paths(window, cx),
            FinderCommand::TogglePurge | FinderCommand::ToggleWrap => {
                let wrapping = matches!(command, FinderCommand::ToggleWrap);
                let key = if wrapping {
                    "wrap"
                } else {
                    "purgeBeforeEverySearch"
                };
                let enabled = if wrapping {
                    !self.files_search.results_wrap
                } else {
                    !self.files_search.results_purge
                };
                if self.files_search.views.active == 0
                    && let Some(store) = &self.settings_store
                    && let Err(error) = store.update_settings(|settings| {
                        if settings
                            .get("FinderConfig")
                            .is_some_and(|config| !config.is_object())
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "FinderConfig must be an object",
                            ));
                        }
                        let current = settings
                            .get("FinderConfig")
                            .and_then(|config| config.get(key));
                        if current.is_some_and(|current| !current.is_boolean()) {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("{key} must be a boolean"),
                            ));
                        }
                        settings["FinderConfig"][key] = enabled.into();
                        Ok(())
                    })
                {
                    self.notify_editor_error(
                        format!("Finder setting could not be saved: {error}"),
                        cx,
                    );
                    self.files_search.results_focus.focus(window, cx);
                    return;
                }
                if wrapping {
                    self.files_search.wrapped_preferred_x = None;
                    self.files_search.results_wrap = enabled;
                    self.files_search
                        .wrapped_scroll
                        .reset(self.files_search.results.rows.len());
                    self.files_search
                        .results_scroll
                        .0
                        .borrow()
                        .base_handle
                        .set_offset(gpui::Point::default());
                    self.reveal_finder_row(
                        self.files_search.results_selected,
                        gpui::ScrollStrategy::Nearest,
                    );
                } else {
                    self.files_search.results_purge = enabled;
                }
                self.files_search.results_focus.focus(window, cx);
                cx.notify();
            }
            FinderCommand::Copy(copy) => self.copy_finder(copy, window, cx),
            FinderCommand::SelectAll => self.select_all_finder(window, cx),
            FinderCommand::FoldAll | FinderCommand::UnfoldAll => {
                let caret = self.finder_caret();
                if self.files_search.text_head.is_none() {
                    self.files_search.text_anchor = Some(caret);
                    self.files_search.text_head = Some(caret);
                }
                self.files_search
                    .results
                    .fold_all(matches!(command, FinderCommand::FoldAll));
                self.sync_finder_visible_row();
                self.files_search
                    .wrapped_scroll
                    .reset(self.files_search.results.rows.len());
                self.files_search.results_width_item = None;
                self.files_search.results_width = px(0.);
                self.reveal_finder_row(
                    self.files_search.results_selected,
                    gpui::ScrollStrategy::Nearest,
                );
                self.files_search.results_focus.focus(window, cx);
                self.cancel_finder_drag();
                cx.notify();
            }
            FinderCommand::ClearAll => {
                self.files_search.results.clear();
                self.files_search.wrapped_scroll.reset(0);
                self.files_search.results_selected = 0;
                self.files_search.results_cursor_x = None;
                self.files_search.wrapped_preferred_x = None;
                self.files_search.text_anchor = None;
                self.files_search.text_head = None;
                self.files_search.results_width_item = None;
                self.files_search.results_width = px(0.);
                self.reveal_finder_row(0, gpui::ScrollStrategy::Top);
                self.files_search.results_focus.focus(window, cx);
                self.cancel_finder_drag();
                cx.notify();
            }
        }
    }

    fn finder_point_at(
        &self,
        index: usize,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
    ) -> Option<(crate::finder::TextPoint, gpui::Pixels, bool)> {
        if self.files_search.results_wrap {
            let bounds = self.files_search.wrapped_scroll.bounds_for_item(index)?;
            let row = *self.files_search.results.rows.get(index)?;
            let layout = self.finder_layout(index, true, window)?;
            let hit = layout.closest_index_for_position(position - bounds.origin - gpui::point(px(8.), px(0.)));
            let unwrapped = self.finder_layout(index, false, window)?;
            return Some((crate::finder::TextPoint { row: Some(row), byte: hit.byte }, unwrapped.unwrapped_x_for_index(hit.byte, hit.trailing) + px(8.), hit.trailing));
        }
        let scroll = self.files_search.results_scroll.0.borrow();
        let x = position.x - scroll.base_handle.bounds().left() - scroll.base_handle.offset().x;
        drop(scroll);
        let (point, trailing) = self.finder_point_for_column(index, x, window)?;
        Some((point, x, trailing))
    }

    fn move_wrapped_finder(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let caret = self.finder_caret();
        let at_end = caret.row.is_none();
        if self.files_search.results.rows.is_empty() || (at_end && down) {
            return;
        }
        let index = if at_end {
            self.files_search.results.rows.len() - 1
        } else {
            self.files_search.results_selected
        };
        if !at_end && caret.row != self.files_search.results.rows.get(index).copied() {
            let target = if down {
                (index + 1).min(self.files_search.results.rows.len())
            } else {
                index
            };
            let row = self.files_search.results.rows.get(target).copied();
            let byte = if down {
                0
            } else {
                row.and_then(|row| self.files_search.results.caption_row(row))
                    .map_or(0, |caption| caption.len())
            };
            let point = crate::finder::TextPoint { row, byte };
            if !extend || self.files_search.text_anchor.is_none() {
                self.files_search.text_anchor = Some(if extend { caret } else { point });
            }
            self.files_search.text_head = Some(point);
            self.files_search.results_selected = target;
            self.files_search.wrapped_preferred_x = None;
            self.files_search.results_cursor_x = None;
            self.reveal_wrapped_finder_caret(window);
            cx.notify();
            return;
        }
        let Some(layout) = self.finder_layout(index, true, window) else {
            return;
        };
        let line_height = layout.line_height;
        let position = if at_end { gpui::point(px(0.), layout.height()) } else { layout.position_for_index(caret.byte, self.finder_caret_affinity()) };
        let x = self.files_search.wrapped_preferred_x.unwrap_or(position.x);
        self.files_search.wrapped_preferred_x = Some(x);
        let mut target_y = position.y + if down { line_height } else { -line_height };
        let total_height = layout.height();
        let target = if target_y >= px(0.) && target_y < total_height {
            index
        } else if down {
            (index + 1).min(self.files_search.results.rows.len())
        } else {
            index.saturating_sub(1)
        };
        let mut trailing = false;
        let point = if target == self.files_search.results.rows.len() {
            crate::finder::TextPoint { row: None, byte: 0 }
        } else {
            let Some(target_layout) = self.finder_layout(target, true, window) else {
                return;
            };
            if target != index {
                target_y = if down {
                    px(0.)
                } else {
                    target_layout.height() - target_layout.line_height
                };
            }
            let hit = target_layout.closest_index_for_position(gpui::point(x, target_y.max(px(0.))));
            let byte = hit.byte;
            trailing = hit.trailing;
            let Some(unwrapped) = self.finder_layout(target, false, window) else { return; };
            let column = unwrapped.unwrapped_x_for_index(byte, trailing) + px(8.);
            self.files_search.results_cursor_x = Some(column);
            crate::finder::TextPoint {
                row: self.files_search.results.rows.get(target).copied(),
                byte,
            }
        };
        if !extend || self.files_search.text_anchor.is_none() {
            self.files_search.text_anchor = Some(if extend { caret } else { point });
        }
        self.files_search.text_head = Some(point);
        self.files_search.caret_affinity = Some((self.files_search.views.active, point, trailing));
        self.files_search.results_selected = target;
        self.reveal_wrapped_finder_caret(window);
        cx.notify();
    }

    fn reveal_wrapped_finder_caret(&self, window: &Window) {
        let index = self.files_search.results_selected;
        if index == self.files_search.results.rows.len() {
            self.files_search.wrapped_scroll.scroll_to_end();
            return;
        }
        let bounds = self.files_search.wrapped_scroll.bounds_for_item(index);
        let Some(layout) = self.finder_layout(index, true, window) else {
            return;
        };
        let line_height = layout.line_height;
        let y = layout.position_for_index(self.finder_caret().byte, self.finder_caret_affinity()).y;
        let viewport = self.files_search.wrapped_scroll.viewport_bounds();
        let caret_top = bounds.map(|bounds| bounds.top() + y);
        let offset = if caret_top.is_none() {
            Some((y + line_height - viewport.size.height).max(px(0.)))
        } else if caret_top.is_some_and(|top| top < viewport.top()) {
            Some(y)
        } else if caret_top.is_some_and(|top| top + line_height > viewport.bottom()) {
            Some((y + line_height - viewport.size.height).max(px(0.)))
        } else {
            None
        };
        if let Some(offset_in_item) = offset {
            self.files_search.wrapped_scroll.scroll_to(gpui::ListOffset { item_ix: index, offset_in_item });
        }
    }

    fn finder_point_for_column(
        &self,
        index: usize,
        x: gpui::Pixels,
        window: &Window,
    ) -> Option<(crate::finder::TextPoint, bool)> {
        if index == self.files_search.results.rows.len() {
            return Some((crate::finder::TextPoint { row: None, byte: 0 }, false));
        }
        let row = *self.files_search.results.rows.get(index)?;
        let shape = self.finder_layout(index, false, window)?;
        let hit = shape.closest_index_for_position(gpui::point(x - px(8.), px(0.)));
        Some((crate::finder::TextPoint {
            row: Some(row),
            byte: hit.byte,
        }, hit.trailing))
    }

    fn select_finder_point(
        &mut self,
        index: usize,
        position: gpui::Point<gpui::Pixels>,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((point, x, trailing)) = self.finder_point_at(index, position, window) else {
            return;
        };
        if !extend || self.files_search.text_anchor.is_none() {
            self.files_search.text_anchor = Some(point);
        }
        self.files_search.text_head = Some(point);
        self.files_search.caret_affinity = Some((self.files_search.views.active, point, trailing));
        self.files_search.wrapped_preferred_x = None;
        self.files_search.results_selected = index;
        self.files_search.results_cursor_x = Some(x);
        self.files_search.context = None;
        self.files_search.results_focus.focus(window, cx);
        cx.notify();
    }

    fn render_finder_context(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let position = self.files_search.context?;
        Some(
            gpui::deferred(
                gpui::anchored().position(position).snap_to_window().child(
                    div()
                        .id("finder-context-menu")
                        .debug_selector(|| "finder-context-menu".into())
                        .role(Role::Menu)
                        .track_focus(&self.files_search.context_focus)
                        .w(px(250.))
                        .py(px(2.))
                        .bg(rgb(if self.dark { 0x303030 } else { 0xffffff }))
                        .text_color(rgb(if self.dark { 0xe0e0e0 } else { 0x202020 }))
                        .border_1()
                        .border_color(rgb(0x808080))
                        .occlude()
                        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                            view.files_search.context = None;
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                            match event.keystroke.key.as_str() {
                                "up" | "down" => {
                                    let length = FinderCommand::ALL.len();
                                    for _ in 0..length {
                                        view.files_search.context_selected =
                                            if event.keystroke.key == "up" {
                                                (view.files_search.context_selected + length - 1)
                                                    % length
                                            } else {
                                                (view.files_search.context_selected + 1) % length
                                            };
                                        if view.finder_command_enabled(
                                            FinderCommand::ALL[view.files_search.context_selected],
                                        ) {
                                            break;
                                        }
                                    }
                                    cx.notify();
                                }
                                "enter" => {
                                    let command =
                                        FinderCommand::ALL[view.files_search.context_selected];
                                    if view.finder_command_enabled(command) {
                                        view.run_finder_command(command, window, cx);
                                    }
                                }
                                "escape" => {
                                    view.files_search.context = None;
                                    view.files_search.results_focus.focus(window, cx);
                                    cx.notify();
                                }
                                _ => return,
                            }
                            cx.stop_propagation();
                        }))
                        .children(
                            FinderCommand::ALL
                                .into_iter()
                                .enumerate()
                                .filter(|(_, command)| {
                                    self.files_search.views.active != 0
                                        || !matches!(command, FinderCommand::CloseResults)
                                })
                                .map(|(index, command)| {
                                    let selector = command.selector();
                                    let enabled = self.finder_command_enabled(command);
                                    let checked = match command {
                                        FinderCommand::TogglePurge => {
                                            Some(self.files_search.results_purge)
                                        }
                                        FinderCommand::ToggleWrap => {
                                            Some(self.files_search.results_wrap)
                                        }
                                        _ => None,
                                    };
                                    div()
                                        .id(selector)
                                        .debug_selector(move || selector.into())
                                        .role(Role::MenuItem)
                                        .relative()
                                        .aria_label(command.label())
                                        .when_some(checked, |item, checked| {
                                            item.aria_toggled(checked.into())
                                        })
                                        .h(px(24.))
                                        .px(px(20.))
                                        .child(command.label())
                                        .when(checked == Some(true), |item| {
                                            item.child(
                                                div()
                                                    .absolute()
                                                    .left(px(4.))
                                                    .top(px(0.))
                                                    .w(px(12.))
                                                    .h(px(24.))
                                                    .child("✓"),
                                            )
                                        })
                                        .when(!enabled, |item| item.text_color(rgb(0x808080)))
                                        .a11y_synthetic_children(move |builder| {
                                            if !enabled {
                                                builder.parent_node().set_disabled();
                                            }
                                        })
                                        .when(self.files_search.context_selected == index, |item| {
                                            item.bg(rgb(0x0078d4)).text_color(rgb(0xffffff))
                                        })
                                        .on_mouse_move(cx.listener(move |view, _, _, cx| {
                                            if enabled {
                                                view.files_search.context_selected = index;
                                                cx.notify();
                                            }
                                        }))
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            if enabled {
                                                view.run_finder_command(command, window, cx);
                                            }
                                            cx.stop_propagation();
                                        }))
                                }),
                        ),
                ),
            )
            .priority(2)
            .into_any_element(),
        )
    }

    pub(super) fn render_file_results(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let revision = (self.style_config.revision(), self.dark);
        if self.files_search.style_revision != Some(revision) {
            self.invalidate_finder_style_layout();
            self.files_search.style_revision = Some(revision);
        }
        if !self.files_search.results_visible {
            return None;
        }
        let dark = self.dark;
        Some(
            div()
                .id("files-results")
                .debug_selector(|| "files-results".into())
                .role(Role::Group)
                .aria_label("Search results")
                .track_focus(&self.files_search.results_focus)
                .on_mouse_down_out(cx.listener(|view, _, _, _| view.cancel_finder_drag()))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.cancel_finder_drag()),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _, _, _| view.cancel_finder_drag()),
                )
                .key_context("MatrixFindResults")
                .h(px(156.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(rgb(0x808080))
                .bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
                .text_color(rgb(if self.dark { 0xe0e0e0 } else { 0x000000 }))
                .on_action(cx.listener(|view, _: &EditorSelectAll, window, cx| {
                    view.select_all_finder(window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(|view, _: &EditorCopy, window, cx| {
                    view.copy_finder(FinderCopy::Verbatim, window, cx);
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" => view.activate_finder_caret(window, cx),
                        "escape" => {
                            view.cancel_finder_drag();
                            view.files_search.results_visible = false;
                            view.editor_focus.focus(window, cx);
                            cx.notify();
                        }
                        "delete" => view.delete_finder_row(window, cx),
                        "up" | "down" => {
                            if view.files_search.results_wrap {
                                view.move_wrapped_finder(
                                    event.keystroke.key == "down",
                                    event.keystroke.modifiers.shift,
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                                return;
                            }
                            let last = view.files_search.results.rows.len();
                            let next = if event.keystroke.key == "up" {
                                view.files_search.results_selected.saturating_sub(1)
                            } else {
                                (view.files_search.results_selected + 1).min(last)
                            };
                            let x = view.files_search.results_cursor_x.unwrap_or(px(8.));
                            if let Some((point, trailing)) = view.finder_point_for_column(next, x, window) {
                                if view.files_search.text_anchor.is_none() {
                                    view.files_search.text_anchor =
                                        view.files_search.text_head.or_else(|| {
                                            view.finder_point_for_column(
                                                view.files_search.results_selected,
                                                x,
                                                window,
                                            ).map(|(point, _)| point)
                                        });
                                }
                                if !event.keystroke.modifiers.shift {
                                    view.files_search.text_anchor = Some(point);
                                }
                                view.files_search.text_head = Some(point);
                                view.files_search.caret_affinity = Some((view.files_search.views.active, point, trailing));
                                view.files_search.results_cursor_x = Some(x);
                            }
                            view.files_search.results_selected = next;
                            view.reveal_finder_row(
                                view.files_search.results_selected,
                                gpui::ScrollStrategy::Nearest,
                            );
                            cx.notify();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .h(px(22.))
                        .flex()
                        .justify_between()
                        .px(px(4.))
                        .bg(rgb(if self.dark { 0x303030 } else { 0xf0f0f0 }))
                        .child(
                            div()
                                .id("files-results-summary")
                                .role(Role::Status)
                                .aria_label(self.files_search.message.clone())
                                .child(self.files_search.message.clone()),
                        )
                        .when(
                            self.files_search.cancel.is_some()
                                && !self.find_in_results_search_active(),
                            |row| {
                                row.child(
                                    div()
                                        .id("files-cancel")
                                        .debug_selector(|| "files-cancel".into())
                                        .role(Role::Button)
                                        .aria_label("Cancel file search")
                                        .child("Cancel")
                                        .on_click(cx.listener(|view, _, _, _| {
                                            if let Some(cancel) = &view.files_search.cancel {
                                                cancel.store(true, Ordering::Relaxed);
                                            }
                                        })),
                                )
                            },
                        )
                        .child(
                            div()
                                .id("files-results-close")
                                .debug_selector(|| "files-results-close".into())
                                .role(Role::Button)
                                .aria_label("Close search results")
                                .tooltip(move |_, cx| {
                                    icon_tooltip::view("Close search results", dark, cx)
                                })
                                .child("×")
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_active_finder(window, cx);
                                })),
                        ),
                )
                .children(self.render_finder_tabs(cx))
                .child(if self.files_search.results_wrap {
                    if self.files_search.wrapped_scroll.item_count()
                        != self.files_search.results.rows.len()
                    {
                        self.files_search
                            .wrapped_scroll
                            .reset(self.files_search.results.rows.len());
                    }
                    gpui::list(
                        self.files_search.wrapped_scroll.clone(),
                        cx.processor(|view, index, window, cx| view.render_finder_row(index, window, cx)),
                    )
                    .flex_1()
                    .min_h(px(0.))
                    .into_any_element()
                } else {
                    gpui::uniform_list(
                        "files-result-rows",
                        self.files_search.results.rows.len(),
                        cx.processor(|view, range: std::ops::Range<usize>, window, cx| {
                            view.measure_finder_width(range.clone(), window, cx);
                            range
                                .map(|index| view.render_finder_row(index, window, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&self.files_search.results_scroll)
                    .with_width_from_item(self.files_search.results_width_item)
                    .with_horizontal_sizing_behavior(
                        gpui::ListHorizontalSizingBehavior::Unconstrained,
                    )
                    .flex_1()
                    .min_h(px(0.))
                    .into_any_element()
                })
                .children(self.render_finder_context(cx))
                .into_any_element(),
        )
    }

    fn render_finder_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        use crate::finder::Row;
        let row = self.files_search.results.rows[index];
        let appearance = self.finder_row_style(row);
        let font_size = px(appearance.font_size as f32 * 96. / 72.);
        let line_height = self.finder_line_height(window);
        let caption = self
            .files_search
            .results
            .caption(index)
            .unwrap_or_default()
            .into_owned();
        let (selector, collapsed) = match row {
            Row::Search(search) => {
                let search = &self.files_search.results.searches[search];
                (
                    format!("finder-search-{index}"),
                    Some(search.collapsed),
                )
            }
            Row::Document(search, doc) => {
                let found = &self.files_search.results.searches[search].documents[doc];
                (
                    format!("finder-document-{search}-{doc}"),
                    Some(found.collapsed),
                )
            }
            Row::Line(search, doc, line) => {
                (
                    format!("finder-hit-{search}-{doc}-{line}"),
                    None,
                )
            }
            Row::Error(search, error) => (
                format!("finder-error-{search}-{error}"),
                None,
            ),
        };
        let background = if index == self.files_search.results_selected {
            self.style_config.config.theme(self.dark).and_then(|theme| theme.lexers.get("searchResult"))
                .and_then(|lexer| lexer.styles.iter().find(|style| style.id == 6))
                .and_then(|style| style.attributes.get("bgColor"))
                .and_then(|value| u32::from_str_radix(value, 16).ok()).unwrap_or(0xe8e8ff)
        } else {
            appearance.background
        };
        let selected = self
            .files_search
            .text_anchor
            .zip(self.files_search.text_head)
            .and_then(|(anchor, head)| self.files_search.results.selected_bytes(row, anchor, head));
        let text = finder_layout::FinderText {
            source: self.finder_layout_source(row, &caption), wrapped: self.files_search.results_wrap, selection: selected,
            foreground: self.pref_bool("ScintillaPrimaryView.selectedTextForegroundSingleColor", false)
                .then(|| rgb(self.style_colour("Selected text colour", "fgColor", appearance.foreground)).into()),
            background: rgb(self.style_colour("Selected text colour", "bgColor", 0xc0c0c0)).into(),
            caret: (self.files_search.results_focus.is_focused(window) && self.finder_caret().row == Some(row))
                .then(|| (self.finder_caret().byte, self.finder_caret_affinity(), rgb(self.style_colour("Caret colour", "fgColor", appearance.foreground)).into())),
        };
        div()
            .id(("finder-row", index))
            .debug_selector(move || selector)
            .role(Role::Group)
            .aria_label(caption)
            .relative()
            .when(!self.files_search.results_wrap, |row| {
                row.h(line_height).whitespace_nowrap()
            })
            .when(self.files_search.results_wrap, |row| {
                row.w_full().min_h(line_height)
            })
            .pl(px(8.))
            .overflow_hidden()
            .font_family(appearance.font_family)
            .text_size(font_size)
            .line_height(line_height)
            .bg(rgb(background))
            .text_color(rgb(appearance.foreground))
            .when_some(collapsed, |row, collapsed| {
                row.child(
                    div()
                        .id(("finder-fold", index))
                        .role(Role::Button)
                        .aria_label(if collapsed {
                            "Expand results"
                        } else {
                            "Collapse results"
                        })
                        .absolute()
                        .left(px(0.))
                        .top(px(0.))
                        .w(px(7.))
                        .h(line_height)
                        .child(if collapsed { "+" } else { "−" })
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.toggle_finder_row(index, window, cx);
                            cx.stop_propagation();
                        })),
                )
            })
            .child(text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.select_finder_point(
                        index,
                        event.position,
                        event.modifiers.shift,
                        window,
                        cx,
                    );
                    view.files_search.dragging_text = true;
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(move |view, event: &gpui::MouseMoveEvent, window, cx| {
                    if event.pressed_button == Some(MouseButton::Left)
                        && view.files_search.dragging_text
                    {
                        view.select_finder_point(index, event.position, true, window, cx);
                        cx.stop_propagation();
                    } else if event.pressed_button.is_none() {
                        view.cancel_finder_drag();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.cancel_finder_drag();
                    let inside = view
                        .finder_point_at(index, event.position, window)
                        .is_some_and(|(point, _, _)| {
                            view.files_search
                                .text_anchor
                                .zip(view.files_search.text_head)
                                .is_some_and(|(anchor, head)| point.between(anchor, head))
                        });
                    if !inside {
                        view.select_finder_point(index, event.position, false, window, cx);
                    }
                    view.files_search.context = Some(event.position);
                    view.files_search.context_selected = 0;
                    view.files_search.context_focus.focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .on_click(
                cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                    view.files_search.results_selected = index;
                    let Some((_, x, _)) = view.finder_point_at(index, event.position(), window) else {
                        return;
                    };
                    view.files_search.results_cursor_x = Some(x);
                    view.files_search.results_focus.focus(window, cx);
                    if event.click_count() == 2 {
                        view.activate_finder_row(index, Some(x), window, cx);
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}
