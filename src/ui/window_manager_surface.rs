use super::*;
use gpui::{Pixels, point};
use super::dialog_font::px;
use crate::workspace::{BufferId, ViewId};
use crate::localization::Text;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct Row {
    id: BufferId,
    name: String,
    path: String,
    full_path: String,
    language: String,
    size: usize,
    modified: Option<SystemTime>,
    dirty: bool,
    read_only: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation { Save, Close }

struct PendingSave {
    id: BufferId,
    name: String,
    error_revision: u64,
}

struct SaveFailure {
    name: String,
    errors: Vec<Text>,
}

struct Batch {
    operation: Operation,
    targets: VecDeque<BufferId>,
    restore: Option<BufferId>,
    first_selected: usize,
    original: BTreeSet<BufferId>,
    pending_save: Option<PendingSave>,
}

enum HeaderDrag {
    Resize { column: usize, start: Pixels, width: f32 },
    Move { column: usize, start: Pixels },
}

pub(super) struct State {
    visible: bool,
    bounds: Option<Bounds<Pixels>>,
    view: ViewId,
    rows: Vec<Row>,
    selected: BTreeSet<BufferId>,
    anchor: Option<BufferId>,
    cursor: Option<BufferId>,
    sort: Option<(usize, bool)>,
    columns: [usize; 5],
    widths: [f32; 5],
    last_table_width: f32,
    header_drag: Option<HeaderDrag>,
    focus: FocusHandle,
    button_focus: [FocusHandle; 5],
    scroll: gpui::UniformListScrollHandle,
    horizontal_scroll: gpui::ScrollHandle,
    context: Option<gpui::Point<Pixels>>,
    context_selected: usize,
    batch: Option<Batch>,
    advance_queued: bool,
    message: Option<Text>,
    save_errors: BTreeMap<BufferId, SaveFailure>,
    error_scroll: gpui::ScrollHandle,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false, bounds: None, view: ViewId::Primary, rows: Vec::new(), selected: BTreeSet::new(),
            anchor: None, cursor: None, sort: None, columns: [0, 1, 2, 3, 4],
            widths: [160., 162.5, 100., 100., 120.], last_table_width: 642.5,
            header_drag: None, focus: cx.focus_handle().tab_index(0).tab_stop(true),
            button_focus: std::array::from_fn(|_| cx.focus_handle().tab_index(0).tab_stop(true)),
            scroll: gpui::UniformListScrollHandle::new(), horizontal_scroll: gpui::ScrollHandle::new(),
            context: None, context_selected: 0, batch: None, advance_queued: false, message: None,
            save_errors: BTreeMap::new(), error_scroll: gpui::ScrollHandle::new(),
        }
    }
}

pub(super) fn dimensions() -> gpui::Size<Pixels> { size(px(787.5), px(525.)) }
pub(super) fn minimum_dimensions() -> gpui::Size<Pixels> { size(px(505.5), px(308.)) }

fn text_order(left: &str, right: &str) -> Ordering {
    let left = left.encode_utf16().collect::<Vec<_>>();
    let right = right.encode_utf16().collect::<Vec<_>>();
    let (mut l, mut r) = (0, 0);
    while l < left.len() && r < right.len() {
        if (48..=57).contains(&left[l]) && (48..=57).contains(&right[r]) {
            let (start_l, start_r) = (l, r);
            let (mut a, mut b) = (0_i32, 0_i32);
            while l < left.len() && (48..=57).contains(&left[l]) {
                a = a.saturating_mul(10).saturating_add(i32::from(left[l] - 48)); l += 1;
            }
            while r < right.len() && (48..=57).contains(&right[r]) {
                b = b.saturating_mul(10).saturating_add(i32::from(right[r] - 48)); r += 1;
            }
            let order = a.cmp(&b).then_with(|| (r - start_r).cmp(&(l - start_l)));
            if !order.is_eq() { return order; }
        } else {
            let fold = |ch: u16| if (65..=90).contains(&ch) { ch + 32 } else { ch };
            let order = fold(left[l]).cmp(&fold(right[r]));
            if !order.is_eq() { return order; }
            l += 1; r += 1;
        }
    }
    (left.len() - l).cmp(&(right.len() - r))
}

fn row_order(left: &Row, right: &Row, column: usize) -> Ordering {
    let primary = match column {
        0 => text_order(&left.name, &right.name),
        2 => text_order(&left.language, &right.language),
        3 => left.size.cmp(&right.size),
        4 => left.modified.cmp(&right.modified),
        _ => Ordering::Equal,
    };
    primary.then_with(|| text_order(&left.full_path, &right.full_path))
}

fn format_time(time: Option<SystemTime>) -> String {
    let Some(time) = time else { return String::new(); };
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(), Err(error) => -error.duration().as_secs_f64(),
    };
    let date = objc2_foundation::NSDate::dateWithTimeIntervalSince1970(seconds);
    let formatter = objc2_foundation::NSDateFormatter::new();
    formatter.setDateFormat(Some(&objc2_foundation::NSString::from_str("yyyy-MM-dd HH:mm:ss")));
    formatter.stringFromDate(&date).to_string()
}

fn label(locale: &str, id: u32, fallback: &'static str) -> String {
    crate::localization::dialog_control(locale, "Window", id).unwrap_or(fallback).replace('&', "")
}

fn named(locale: &str, name: &str, fallback: &'static str) -> String {
    crate::localization::dialog_name(locale, &format!("WindowsDlg/{name}")).unwrap_or(fallback).to_owned()
}

impl MatrixWindow {
    pub(super) fn window_manager_visible(&self) -> bool { self.window_manager.visible }
    pub(super) fn window_manager_blocks_document(&self) -> bool { self.window_manager.visible }

    pub(super) fn window_manager_title(&self) -> String {
        let locale = self.native_locale();
        format!("{} - {} {}", crate::localization::dialog_title(&locale, "Window").unwrap_or("Windows"),
            named(&locale, "NbDocsTotal", "total documents:"), self.workspace.view_buffer_ids(self.window_manager.view).len())
    }

    pub(super) fn window_manager_focus(&self) -> FocusHandle { self.window_manager.focus.clone() }
    pub(super) fn window_manager_bounds(&self) -> Option<Bounds<Pixels>> { self.window_manager.bounds }

    pub(super) fn dismiss_window_manager(&mut self, cx: &mut Context<Self>) -> bool {
        if self.window_manager.batch.is_some() { return false; }
        self.window_manager.visible = false;
        self.window_manager.context = None;
        self.window_manager.header_drag = None;
        cx.notify();
        true
    }

    pub(super) fn show_window_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.show_window_manager(window, cx)) { return; }
        if self.window_manager.visible { self.present_floating(floating_windows::DialogKind::WindowManager, window, cx); return; }
        if self.window_manager_busy() || self.window_manager.batch.is_some() { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.window_manager.visible = true;
        self.window_manager.view = self.workspace.active_view();
        self.window_manager.sort = None;
        self.window_manager.columns = [0, 1, 2, 3, 4];
        self.window_manager.widths = [160., 162.5, 100., 100., 120.];
        self.window_manager.last_table_width = 642.5;
        self.window_manager.horizontal_scroll.set_offset(point(px(0.), px(0.)));
        self.window_manager.selected.clear();
        self.window_manager.selected.insert(self.workspace.active().id());
        self.window_manager.cursor = Some(self.workspace.active().id());
        self.window_manager.anchor = self.window_manager.cursor;
        self.window_manager.context = None;
        self.window_manager.message = None;
        self.window_manager.save_errors.clear();
        self.window_manager.error_scroll.set_offset(point(px(0.), px(0.)));
        self.refresh_window_manager_rows();
        self.scroll_window_manager_cursor();
        self.present_floating(floating_windows::DialogKind::WindowManager, window, cx);
        cx.notify();
    }

    fn window_manager_busy(&self) -> bool {
        self.file_busy || self.file_open_sequence_pending() || self.close_dialog || self.save_all_dialog
            || self.save_all_restore.is_some() || self.reload_target.is_some() || self.application_quit_pending || self.macro_interaction_blocked()
    }

    fn refresh_window_manager_rows(&mut self) {
        let buffers = self.workspace.buffers().iter().map(|buffer| (buffer.id(), buffer)).collect::<BTreeMap<_, _>>();
        let mut rows = self.workspace.view_buffer_ids(self.window_manager.view).iter().filter_map(|id| {
            let buffer = buffers.get(id)?;
            let name = buffer.name();
            let file = buffer.disk_file.as_ref();
            let full_path = file.map(|file| file.path().to_string_lossy().into_owned()).unwrap_or_else(|| name.clone());
            let path = file.and_then(|file| file.path().parent()).map(|path| {
                let mut path = path.to_string_lossy().into_owned();
                if !path.ends_with('/') { path.push('/'); }
                path
            }).unwrap_or_default();
            let language = match buffer.language() {
                crate::syntax::Language::NormalText => "normal".into(),
                crate::syntax::Language::Json => "json".into(),
                crate::syntax::Language::Builtin(id) => crate::languages::catalog().ok().and_then(|catalog| catalog.by_id(id)).map(|profile| profile.name.clone()).unwrap_or_default(),
                crate::syntax::Language::UserDefined(_) => String::new(),
            };
            Some(Row { id: *id, name, path, full_path, language, size: buffer.editor.document().len_bytes(),
                modified: buffer.window_modified_time(), dirty: buffer.is_modified(), read_only: buffer.editor.document().is_read_only() })
        }).collect::<Vec<_>>();
        if let Some((column, descending)) = self.window_manager.sort {
            rows.sort_by(|left, right| {
                let order = row_order(left, right, column);
                if descending { order.reverse() } else { order }
            });
        }
        let live = rows.iter().map(|row| row.id).collect::<BTreeSet<_>>();
        self.window_manager.selected.retain(|id| live.contains(id));
        if self.window_manager.cursor.is_some_and(|id| !live.contains(&id)) { self.window_manager.cursor = None; }
        if self.window_manager.anchor.is_some_and(|id| !live.contains(&id)) { self.window_manager.anchor = None; }
        self.window_manager.rows = rows;
    }

    pub(super) fn sync_window_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if Window::window_handle(window) != self.document_window { return; }
        if self.window_manager.visible || self.window_manager.batch.is_some() { self.refresh_window_manager_rows(); }
        if self.window_manager.batch.is_none() || self.window_manager_busy() { return; }
        self.capture_window_manager_save_error(cx);
        if self.window_manager.advance_queued { return; }
        self.window_manager.advance_queued = true;
        let root = self.document_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            if !cx.windows().contains(&root) { return; }
            match root.update(cx, |_, window, cx| owner.update(cx, |view, cx| {
                view.window_manager.advance_queued = false;
                view.advance_window_manager_batch(window, cx);
            })) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => eprintln!("File window continuation lost its document: {error}"),
            }
        });
    }

    fn advance_window_manager_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.window_manager.batch.is_none() || self.window_manager_busy() { return; }
        if !self.window_manager.visible {
            self.window_manager.batch = None;
            return;
        }
        loop {
            self.capture_window_manager_save_error(cx);
            let next = self.window_manager.batch.as_mut().and_then(|batch| batch.targets.pop_front().map(|id| (batch.operation, id)));
            let Some((operation, id)) = next else { self.finish_window_manager_batch(window, cx); return; };
            if !self.workspace.view_buffer_ids(self.window_manager.view).contains(&id) { continue; }
            if operation == Operation::Save && !self.workspace.buffers().iter().any(|buffer| buffer.id() == id && buffer.is_modified()) { continue; }
            if !self.activate_window_manager_buffer(id, window, cx) {
                self.window_manager.message = Some(self.editor_error.clone().unwrap_or_else(|| Text::resource("matrix-window-activation-failed", "Unable to switch to the selected file")));
                self.window_manager.batch = None;
                cx.notify();
                return;
            }
            match operation {
                Operation::Save => {
                    let pending = PendingSave { id, name: self.workspace.active().name(), error_revision: self.editor_error_revision };
                    if let Some(batch) = &mut self.window_manager.batch { batch.pending_save = Some(pending); }
                    self.start_save(false, window, cx);
                }
                Operation::Close => self.request_close_current(window, cx),
            }
            self.refresh_window_manager_rows();
            if self.window_manager_busy() { return; }
        }
    }

    fn capture_window_manager_save_error(&mut self, cx: &mut Context<Self>) {
        if self.window_manager_busy() { return; }
        let pending = self.window_manager.batch.as_mut().and_then(|batch| batch.pending_save.take());
        let Some(pending) = pending else { return; };
        if self.editor_error_revision != pending.error_revision {
            let Some(error) = self.editor_error.clone() else { return; };
            let failure = self.window_manager.save_errors.entry(pending.id).or_insert_with(|| SaveFailure { name: pending.name, errors: Vec::new() });
            failure.errors.push(error);
            cx.notify();
        }
    }

    fn window_manager_message(&self) -> String {
        let mut messages = self.window_manager.save_errors.iter().map(|(id, failure)| {
            format!("{} [#{}]：{}", failure.name, id.0, failure.errors.iter().map(|error| error.render(&self.native_locale())).collect::<Vec<_>>().join("；"))
        }).collect::<Vec<_>>();
        if let Some(message) = &self.window_manager.message { messages.push(message.render(&self.native_locale())); }
        messages.join("\n")
    }

    fn activate_window_manager_buffer(&mut self, id: BufferId, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.workspace.view_buffer_ids(self.window_manager.view).contains(&id) { return false; }
        if !self.cancel_view_composition(window, cx) { return false; }
        if self.workspace.active_view() == self.window_manager.view && self.workspace.active().id() == id { return true; }
        match self.workspace.activate_in_view(self.window_manager.view, id) {
            Ok(()) => { self.reset_active_view(window, cx); true }
            Err(error) => { self.notify_editor_error(error.to_string(), cx); false }
        }
    }

    fn finish_window_manager_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture_window_manager_save_error(cx);
        let Some(batch) = self.window_manager.batch.take() else { return; };
        if batch.operation == Operation::Save {
            if let Some(id) = batch.restore { self.activate_window_manager_buffer(id, window, cx); }
        } else {
            self.refresh_window_manager_rows();
            self.window_manager.selected.clear();
            if !self.window_manager.rows.iter().any(|row| batch.original.contains(&row.id)) {
                self.window_manager.visible = false;
            } else if let Some(row) = self.window_manager.rows.get(batch.first_selected.min(self.window_manager.rows.len().saturating_sub(1))) {
                self.window_manager.selected.insert(row.id);
                self.window_manager.cursor = Some(row.id);
                self.window_manager.anchor = Some(row.id);
            } else { self.window_manager.visible = false; }
        }
        self.scroll_window_manager_cursor();
        if self.window_manager.visible { self.focus_window_manager_dialog(cx); }
        cx.notify();
    }

    fn begin_window_manager_batch(&mut self, operation: Operation, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.begin_window_manager_batch(operation, window, cx)) { return; }
        if !self.window_manager.visible || self.window_manager_busy() || self.window_manager.batch.is_some() { return; }
        self.refresh_window_manager_rows();
        let targets = self.window_manager.rows.iter().filter(|row| self.window_manager.selected.contains(&row.id)).map(|row| row.id).collect::<VecDeque<_>>();
        if targets.is_empty() { return; }
        let first_selected = self.window_manager.rows.iter().position(|row| self.window_manager.selected.contains(&row.id)).unwrap_or(0);
        self.window_manager.context = None;
        self.window_manager.message = None;
        self.window_manager.batch = Some(Batch { operation, targets, restore: self.workspace.active_id_in_view(self.window_manager.view), first_selected,
            original: self.window_manager.rows.iter().map(|row| row.id).collect(), pending_save: None });
        self.advance_window_manager_batch(window, cx);
        cx.notify();
    }

    fn focus_window_manager_dialog(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.floating_windows.get(&floating_windows::DialogKind::WindowManager).copied() else { return; };
        let focus = self.window_manager.focus.clone();
        cx.defer(move |cx| {
            if !cx.windows().contains(&handle.into()) { return; }
            if let Err(error) = handle.update(cx, |_, window, cx| { window.activate_window(); focus.focus(window, cx); }) {
                eprintln!("File window focus failed: {error}");
            }
        });
    }

    fn activate_window_manager_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.activate_window_manager_selection(window, cx)) { return; }
        if self.window_manager_busy() || self.window_manager.batch.is_some() || self.window_manager.selected.len() != 1 { return; }
        let Some(id) = self.window_manager.selected.iter().next().copied() else { return; };
        if self.activate_window_manager_buffer(id, window, cx) {
            self.dismiss_floating(floating_windows::DialogKind::WindowManager, window, cx);
        } else {
            self.refresh_window_manager_rows();
            self.window_manager.message = Some(Text::resource("matrix-window-selection-unavailable", "The selected file is closed or cannot be activated"));
            cx.notify();
        }
    }

    fn sort_window_manager_column(&mut self, column: usize, cx: &mut Context<Self>) {
        if self.window_manager.batch.is_some() { return; }
        self.window_manager.sort = Some((column, self.window_manager.sort.is_some_and(|(previous, descending)| previous == column && !descending)));
        self.refresh_window_manager_rows();
        self.scroll_window_manager_cursor();
        cx.notify();
    }

    fn sort_window_manager_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, |view, window, cx| view.sort_window_manager_tabs(window, cx)) { return; }
        if self.window_manager_busy() || self.window_manager.batch.is_some() { return; }
        let Some(active) = self.workspace.active_id_in_view(self.window_manager.view) else { return; };
        if !self.activate_window_manager_buffer(active, window, cx) { return; }
        let (column, descending) = self.window_manager.sort.unwrap_or((0, false));
        self.window_manager.sort = Some((column, descending));
        let sort = match column {
            1 => crate::workspace::TabSort::Path, 2 => crate::workspace::TabSort::Type,
            3 => crate::workspace::TabSort::Size, 4 => crate::workspace::TabSort::Modified,
            _ => crate::workspace::TabSort::Name,
        };
        self.workspace.sort_tabs(sort, descending);
        self.reset_active_view(window, cx);
        self.refresh_window_manager_rows();
        self.scroll_window_manager_cursor();
        self.focus_window_manager_dialog(cx);
        cx.notify();
    }

    fn scroll_window_manager_cursor(&self) {
        if let Some(index) = self.window_manager.rows.iter().position(|row| Some(row.id) == self.window_manager.cursor) {
            self.window_manager.scroll.scroll_to_item(index, gpui::ScrollStrategy::Nearest);
        }
    }

    fn select_window_manager_row(&mut self, id: BufferId, extend: bool, toggle: bool, cx: &mut Context<Self>) {
        if self.window_manager.batch.is_some() { return; }
        let Some(index) = self.window_manager.rows.iter().position(|row| row.id == id) else { return; };
        if !self.workspace.view_buffer_ids(self.window_manager.view).contains(&id) { self.refresh_window_manager_rows(); cx.notify(); return; }
        if extend {
            let anchor = self.window_manager.rows.iter().position(|row| Some(row.id) == self.window_manager.anchor).unwrap_or(index);
            if !toggle { self.window_manager.selected.clear(); }
            for row in &self.window_manager.rows[anchor.min(index)..=anchor.max(index)] { self.window_manager.selected.insert(row.id); }
        } else {
            if toggle {
                if !self.window_manager.selected.remove(&id) { self.window_manager.selected.insert(id); }
            } else { self.window_manager.selected.clear(); self.window_manager.selected.insert(id); }
            self.window_manager.anchor = Some(id);
        }
        self.window_manager.cursor = Some(id);
        self.window_manager.context = None;
        self.scroll_window_manager_cursor();
        cx.notify();
    }

    fn copy_window_manager_selection(&mut self, full_path: bool, cx: &mut Context<Self>) {
        self.refresh_window_manager_rows();
        let mut text = String::new();
        for row in self.window_manager.rows.iter().filter(|row| self.window_manager.selected.contains(&row.id)) {
            text.push_str(if full_path { &row.full_path } else { &row.name });
            if !text.is_empty() && !text.ends_with("\r\n") { text.push_str("\r\n"); }
        }
        if !text.is_empty() { cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)); }
        self.window_manager.context = None;
        cx.notify();
    }

    fn window_manager_key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if key == "escape" {
            if self.window_manager.context.take().is_none() && self.window_manager.batch.is_none() {
                self.dismiss_floating(floating_windows::DialogKind::WindowManager, window, cx);
            }
            cx.notify(); cx.stop_propagation(); return;
        }
        if self.window_manager.batch.is_some() || self.window_manager_busy() { return; }
        if self.window_manager.context.is_some() {
            match key {
                "up" | "down" => self.window_manager.context_selected = 1 - self.window_manager.context_selected,
                "enter" | "space" => self.copy_window_manager_selection(self.window_manager.context_selected == 1, cx),
                _ => return,
            }
        } else if key == "f10" && modifiers.shift || key == "menu" {
            self.window_manager.context = Some(point(px(24.), px(44.)));
            self.window_manager.context_selected = 0;
        } else if modifiers.alt {
            match key {
                "a" => self.activate_window_manager_selection(window, cx),
                "s" => self.begin_window_manager_batch(Operation::Save, window, cx),
                "c" => self.begin_window_manager_batch(Operation::Close, window, cx),
                "t" => self.sort_window_manager_tabs(window, cx),
                "o" => self.dismiss_floating(floating_windows::DialogKind::WindowManager, window, cx),
                _ => return,
            }
        } else if (modifiers.control || modifiers.platform) && !modifiers.shift && key == "a" {
            self.window_manager.selected = self.window_manager.rows.iter().map(|row| row.id).collect();
        } else if (modifiers.control || modifiers.platform) && key == "c" {
            self.copy_window_manager_selection(true, cx);
        } else if key == "enter" {
            let button = self.window_manager.button_focus.iter().position(|focus| focus.is_focused(window));
            match button {
                Some(1) => self.begin_window_manager_batch(Operation::Save, window, cx),
                Some(2) => self.begin_window_manager_batch(Operation::Close, window, cx),
                Some(3) => self.sort_window_manager_tabs(window, cx),
                Some(4) => self.dismiss_floating(floating_windows::DialogKind::WindowManager, window, cx),
                _ => self.activate_window_manager_selection(window, cx),
            }
        } else if self.window_manager.focus.is_focused(window) {
            let count = self.window_manager.rows.len();
            if count == 0 { return; }
            let current = self.window_manager.rows.iter().position(|row| Some(row.id) == self.window_manager.cursor).unwrap_or(0);
            let page = ((dialog_font::logical(self.auxiliary_client_size(window).height) - 52.) / 22.).floor().max(1.) as usize;
            let index = match key {
                "up" => Some(current.saturating_sub(1)), "down" => Some((current + 1).min(count - 1)),
                "home" => Some(0), "end" => Some(count - 1),
                "pageup" => Some(current.saturating_sub(page)), "pagedown" => Some(current.saturating_add(page).min(count - 1)),
                _ => None,
            };
            if let Some(index) = index {
                let id = self.window_manager.rows[index].id;
                if (modifiers.control || modifiers.platform) && !modifiers.shift {
                    self.window_manager.cursor = Some(id); self.scroll_window_manager_cursor();
                } else { self.select_window_manager_row(id, modifiers.shift, modifiers.control || modifiers.platform, cx); }
            } else if key == "space" {
                let id = self.window_manager.rows[current].id;
                self.select_window_manager_row(id, modifiers.shift, modifiers.control || modifiers.platform, cx);
            } else if !modifiers.control && !modifiers.platform && !modifiers.alt && key.chars().count() == 1 {
                let first = key.to_lowercase();
                let matches = self.window_manager.rows.iter().enumerate().filter(|(_, row)| row.name.to_lowercase().starts_with(&first)).map(|(index, _)| index).collect::<Vec<_>>();
                let last = matches.iter().filter(|index| self.window_manager.selected.contains(&self.window_manager.rows[**index].id)).last().copied();
                let next = last.and_then(|last| matches.iter().find(|index| **index > last)).or_else(|| matches.first()).copied();
                if let Some(index) = next {
                    let id = self.window_manager.rows[index].id;
                    self.select_window_manager_row(id, false, false, cx);
                }
            } else { return; }
        } else { return; }
        cx.notify(); cx.stop_propagation();
    }

    fn finish_window_manager_header(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.window_manager.header_drag.take() else { return; };
        if let HeaderDrag::Move { column, start } = drag {
            if f32::from(position.x - start).abs() < 4. { self.sort_window_manager_column(column, cx); }
            else {
                let mut x = dialog_font::logical(position.x) - 14. - dialog_font::logical(self.window_manager.horizontal_scroll.offset().x);
                let target = self.window_manager.columns.iter().position(|column| { x -= self.window_manager.widths[*column]; x <= 0. }).unwrap_or(4);
                if let Some(source) = self.window_manager.columns.iter().position(|candidate| *candidate == column) {
                    let mut columns = self.window_manager.columns.to_vec();
                    columns.remove(source); columns.insert(target, column);
                    self.window_manager.columns.copy_from_slice(&columns);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn render_window_manager(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.window_manager.visible { return None; }
        self.window_manager.bounds = Some(window.window_bounds().get_bounds());
        self.refresh_window_manager_rows();
        let locale = self.native_locale();
        let background = if self.dark { 0x282828 } else { 0xf0f0f0 };
        let list_background = if self.dark { 0x202020 } else { 0xffffff };
        let border = if self.dark { 0x606060 } else { 0xaaaaaa };
        let enabled = !self.window_manager_busy() && self.window_manager.batch.is_none();
        let selected = self.window_manager.selected.len();
        let message = self.window_manager_message();
        let table_width = (dialog_font::logical(window.viewport_size().width) - 145.).max(360.);
        if (table_width - self.window_manager.last_table_width).abs() > 0.5 {
            self.window_manager.widths[1] = (self.window_manager.widths[1] + table_width - self.window_manager.last_table_width).max(40.);
            self.window_manager.last_table_width = table_width;
        }
        let widths = self.window_manager.widths;
        let columns = self.window_manager.columns;
        let total_width = widths.iter().sum::<f32>();
        let names = [named(&locale, "ColumnName", "Name"), named(&locale, "ColumnPath", "Path"), named(&locale, "ColumnType", "Type"), named(&locale, "ColumnSize", "Size"), named(&locale, "ColumnDateTime", "Modified time")];
        let sort = self.window_manager.sort;
        let header = div().id("window-manager-header").flex().flex_shrink_0().h(px(24.)).w(px(total_width)).bg(rgb(background))
            .children(columns.into_iter().map(|column| {
                let marker = match sort { Some((selected, true)) if selected == column => "△", Some((selected, false)) if selected == column => "▽", _ => "⇵" };
                div().id(("window-manager-column", column)).role(Role::ColumnHeader).aria_label(names[column].clone())
                    .relative().w(px(widths[column])).flex_shrink_0().h_full().border_r_1().border_b_1().border_color(rgb(border))
                    .px(px(5.)).flex().items_center().whitespace_nowrap().overflow_hidden().child(format!("{marker} {}", names[column]))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                        if view.window_manager.batch.is_none() { view.window_manager.context = None; view.window_manager.header_drag = Some(HeaderDrag::Move { column, start: event.position.x }); }
                        cx.stop_propagation();
                    }))
                    .child(div().id(("window-manager-column-edge", column)).absolute().right(px(0.)).top(px(0.)).w(px(5.)).h_full().cursor_col_resize()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                            view.window_manager.header_drag = Some(HeaderDrag::Resize { column, start: event.position.x, width: view.window_manager.widths[column] });
                            cx.stop_propagation();
                        })))
            }));
        let list = gpui::uniform_list("window-manager-rows", self.window_manager.rows.len(), cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
            range.filter_map(|index| {
                let row = view.window_manager.rows.get(index)?.clone();
                let id = row.id;
                let selected = view.window_manager.selected.contains(&id);
                let cursor = view.window_manager.cursor == Some(id);
                let marker = if row.dirty { "*".to_owned() } else if row.read_only {
                    format!(" [{}]", crate::localization::localized_string(&view.native_locale(), "matrix-read-only").unwrap_or("Read Only"))
                } else { String::new() };
                let values = [format!("{}{marker}", row.name), row.path, row.language, row.size.to_string(), format_time(row.modified)];
                Some(div().id(("window-manager-row", id.0)).role(Role::Row).aria_label(values[0].clone()).aria_selected(selected)
                    .h(px(22.)).w(px(total_width)).flex().whitespace_nowrap()
                    .bg(rgb(if selected { if view.dark { 0x35475d } else { 0xcce8ff } } else { list_background }))
                    .when(cursor, |row| row.border_1().border_color(rgb(0x808080)))
                    .children(columns.into_iter().map(|column| div().w(px(widths[column])).h_full().flex_shrink_0().px(px(5.)).overflow_hidden().flex().items_center()
                        .when(column >= 2, |cell| cell.justify_center()).child(values[column].clone())))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                        view.window_manager.focus.focus(window, cx);
                        view.select_window_manager_row(id, event.modifiers.shift, event.modifiers.control || event.modifiers.platform, cx);
                        if event.click_count == 2 { view.activate_window_manager_selection(window, cx); }
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                        if view.window_manager.batch.is_some() || view.window_manager_busy() { return; }
                        if !view.window_manager.selected.contains(&id) { view.select_window_manager_row(id, false, false, cx); }
                        view.window_manager.context_selected = 0;
                        let client = view.auxiliary_client_size(window);
                        view.window_manager.context = Some(point(event.position.x.min(client.width - px(180.)).max(px(0.)), (event.position.y - gpui::px(window_caption::HEIGHT)).min(client.height - px(54.)).max(px(0.))));
                        cx.notify(); cx.stop_propagation();
                    })))
            }).collect::<Vec<_>>()
        })).track_scroll(&self.window_manager.scroll).flex_1().min_h(px(0.)).w(px(total_width));
        let button = |index: usize, text: String, allowed: bool| {
            div().id(("window-manager-button", index)).role(Role::Button).aria_label(text.clone())
                .track_focus(&self.window_manager.button_focus[index]).w(px(105.)).h(px(24.5)).flex_shrink_0()
                .border_1().border_color(rgb(if self.window_manager.button_focus[index].is_focused(window) { 0x0067c0 } else { border }))
                .bg(rgb(if self.dark { 0x383838 } else { 0xe1e1e1 })).text_color(rgb(if allowed { if self.dark { 0xffffff } else { 0x202020 } } else { 0x808080 }))
                .a11y_synthetic_children(move |builder| { if !allowed { builder.parent_node().set_disabled(); } })
                .on_mouse_down(MouseButton::Left, cx.listener(move |view, _, window, cx| {
                    if allowed { view.window_manager.button_focus[index].focus(window, cx); }
                }))
                .flex().items_center().justify_center().child(text)
        };
        let mut root = div().id("window-manager-dialog").role(Role::Dialog).aria_label(self.window_manager_title())
            .key_context("MatrixWindowManager").relative().size_full().p(px(14.)).flex().gap(px(12.))
            .font_family(dialog_font::font_family()).text_size(px(13.)).text_color(rgb(if self.dark { 0xffffff } else { 0x202020 })).bg(rgb(background)).occlude()
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| view.window_manager_key(event, window, cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(|view, _, _, cx| { if view.window_manager.context.take().is_some() { cx.notify(); } }))
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
                if event.pressed_button != Some(MouseButton::Left) { view.window_manager.header_drag = None; return; }
                if let Some(HeaderDrag::Resize { column, start, width }) = &view.window_manager.header_drag {
                    view.window_manager.widths[*column] = (*width + dialog_font::logical(event.position.x - *start)).max(40.);
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, event: &gpui::MouseUpEvent, _, cx| view.finish_window_manager_header(event.position, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, _| view.window_manager.header_drag = None))
            .child(div().flex_1().min_w(px(0.)).h_full().flex().flex_col().gap(px(8.))
                .child(div().id("window-manager-table").role(Role::Table).track_focus(&self.window_manager.focus).flex_1().min_h(px(0.)).w_full()
                .border_1().border_color(rgb(border)).bg(rgb(list_background)).overflow_x_scroll().track_scroll(&self.window_manager.horizontal_scroll)
                .on_mouse_down(MouseButton::Left, cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                    if view.window_manager.batch.is_some() || view.window_manager_busy() { return; }
                    view.window_manager.focus.focus(window, cx);
                    if !event.modifiers.shift && !event.modifiers.control && !event.modifiers.platform { view.window_manager.selected.clear(); }
                    view.window_manager.context = None;
                    cx.notify();
                }))
                .child(div().w(px(total_width)).h_full().flex().flex_col().child(header).child(list)))
                .when(!message.is_empty(), |column| column.child(div().id("window-manager-errors").role(Role::Alert).aria_label(message.clone())
                    .w_full().min_h(px(44.)).max_h(px(144.)).flex_shrink_0().overflow_y_scroll().track_scroll(&self.window_manager.error_scroll)
                    .border_1().border_color(rgb(border)).p(px(6.)).text_color(rgb(if self.dark { 0xff9999 } else { 0xb00000 })).child(message))))
            .child(div().flex().flex_col().h_full().gap(px(8.))
                .child(button(0, label(&locale, 1, "Activate"), enabled && selected == 1)
                    .on_click(cx.listener(|view, _, window, cx| view.activate_window_manager_selection(window, cx))))
                .child(button(1, label(&locale, 7002, "Save"), enabled && selected > 0)
                    .on_click(cx.listener(|view, _, window, cx| view.begin_window_manager_batch(Operation::Save, window, cx))))
                .child(button(2, label(&locale, 7003, "Close window(s)"), enabled && selected > 0)
                    .on_click(cx.listener(|view, _, window, cx| view.begin_window_manager_batch(Operation::Close, window, cx))))
                .child(button(3, label(&locale, 7004, "Sort tabs"), enabled)
                    .on_click(cx.listener(|view, _, window, cx| view.sort_window_manager_tabs(window, cx))))
                .child(div().flex_1())
                .child(button(4, label(&locale, 2, "OK"), self.window_manager.batch.is_none())
                    .on_click(cx.listener(|view, _, window, cx| { if view.window_manager.batch.is_none() { view.dismiss_floating(floating_windows::DialogKind::WindowManager, window, cx); } }))));
        if let Some(position) = self.window_manager.context {
            root = root.child(div().id("window-manager-context").role(Role::Menu).absolute().left(position.x).top(position.y).w(px(180.)).py(px(2.))
                .border_1().border_color(rgb(border)).bg(rgb(background)).occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().id("window-manager-copy-name").role(Role::MenuItem).h(px(24.)).px(px(12.)).flex().items_center().text_color(rgb(if selected == 0 { 0x808080 } else if self.dark { 0xffffff } else { 0x202020 })).a11y_synthetic_children(move |builder| { if selected == 0 { builder.parent_node().set_disabled(); } }).when(self.window_manager.context_selected == 0, |row| row.bg(rgb(0x91c9f7))).hover(|row| row.bg(rgb(0x91c9f7))).child(named(&locale, "MenuCopyName", "Copy Name(s)"))
                    .on_click(cx.listener(|view, _, _, cx| view.copy_window_manager_selection(false, cx))))
                .child(div().id("window-manager-copy-path").role(Role::MenuItem).h(px(24.)).px(px(12.)).flex().items_center().text_color(rgb(if selected == 0 { 0x808080 } else if self.dark { 0xffffff } else { 0x202020 })).a11y_synthetic_children(move |builder| { if selected == 0 { builder.parent_node().set_disabled(); } }).when(self.window_manager.context_selected == 1, |row| row.bg(rgb(0x91c9f7))).hover(|row| row.bg(rgb(0x91c9f7))).child(named(&locale, "MenuCopyPath", "Copy Pathname(s)"))
                    .on_click(cx.listener(|view, _, _, cx| view.copy_window_manager_selection(true, cx)))));
        }
        Some(root.into_any_element())
    }
}
