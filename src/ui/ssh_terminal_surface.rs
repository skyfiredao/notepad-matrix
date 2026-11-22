use super::*;
use crate::ssh_terminal::{Event, PasteButton, Profile, Session, Settings};
use gpui::{ContentMask, ElementInputHandler, Entity, EntityInputHandler, Point, TextRun, UTF16Selection, WeakEntity, fill, point};
use std::num::NonZeroU16;
use std::collections::{BTreeMap, VecDeque};
use std::ops::{Deref, DerefMut, Range};
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

const FOREGROUND: u32 = 0xdddddd;
const SCROLLBACK: usize = 10_000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CellPoint {
    row: usize,
    column: u16,
}

struct VisibleGrid {
    cells: Vec<(CellPoint, vt100::Cell)>,
    rows: u16,
    columns: u16,
    cursor: (u16, u16),
    cursor_style: vt100::CursorStyle,
    hide_cursor: bool,
    history: usize,
    scrollback: usize,
    first_row: usize,
}

impl VisibleGrid {
    fn new(screen: &vt100::Screen, history_base: usize) -> Self {
        let (rows, columns) = screen.size();
        let first_row = history_base.saturating_add(screen.scrollback_len().saturating_sub(screen.scrollback()));
        let mut cells = Vec::with_capacity(usize::from(rows.get()) * usize::from(columns.get()));
        for row in 0..rows.get() {
            for column in 0..columns.get() {
                if let Some(cell) = screen.cell(row, column) {
                    if !cell.is_wide_continuation() { cells.push((CellPoint { row: first_row.saturating_add(usize::from(row)), column }, cell.clone())); }
                }
            }
        }
        Self {
            cells,
            rows: rows.get(),
            columns: columns.get(),
            cursor: screen.cursor_position(),
            cursor_style: screen.cursor_style(),
            hide_cursor: screen.hide_cursor(),
            history: screen.scrollback_len(),
            scrollback: screen.scrollback(),
            first_row,
        }
    }
}

#[derive(Default)]
struct TerminalCallbacks {
    replies: Vec<u8>,
    discarded_rows: usize,
}

impl vt100::Callbacks for TerminalCallbacks {
    fn on_scroll(&mut self, _: vt100::capture::RowContents<'_>, alternate_screen: bool) {
        if !alternate_screen { self.discarded_rows = self.discarded_rows.saturating_add(1); }
    }

    fn unhandled_csi(&mut self, screen: &mut vt100::Screen, first: Option<u8>, _: Option<u8>, params: &[&[u16]], command: char) {
        let parameter = params.first().and_then(|value| value.first()).copied().unwrap_or(0);
        match (first, command, parameter) {
            (None, 'n', 5) => self.replies.extend_from_slice(b"\x1b[0n"),
            (None, 'n', 6) => {
                let (row, column) = screen.cursor_position();
                self.replies.extend_from_slice(format!("\x1b[{};{}R", u32::from(row) + 1, u32::from(column) + 1).as_bytes());
            }
            (None, 'c', 0) => self.replies.extend_from_slice(b"\x1b[?1;2c"),
            (Some(b'>'), 'c', 0) => self.replies.extend_from_slice(b"\x1b[>0;0;0c"),
            _ => {}
        }
    }
}

pub(super) struct State {
    pub(super) visible: bool,
    pub(super) profile_id: u64,
    pub(super) focus: FocusHandle,
    pub(super) paste_button: PasteButton,
    active: ConnectionState,
    parked: BTreeMap<u64, ConnectionState>,
    tabs: Vec<u64>,
    tab_scroll: gpui::ScrollHandle,
    reveal_tab: Option<u64>,
    toolbar_focus: [FocusHandle; 4],
    input: Entity<TerminalInput>,
    task: Option<gpui::Task<()>>,
    _keys: gpui::Subscription,
    height: f32,
    resize_drag: Option<(gpui::Pixels, f32)>,
    generation: u64,
    window_transparent: bool,
}

pub(super) struct ConnectionState {
    pub(super) session: Option<Session>,
    pub(super) fingerprint: Option<String>,
    pub(super) status: String,
    name: String,
    endpoint: String,
    font_size: u16,
    background_color: u32,
    transparency: u8,
    parser: vt100::Parser<TerminalCallbacks>,
    bounds: Option<Bounds<gpui::Pixels>>,
    cell_width: f32,
    scrollbar_drag: bool,
    selecting: bool,
    copy_on_release: bool,
    selection: Option<(CellPoint, CellPoint)>,
    blink: bool,
    pending_input: VecDeque<Vec<u8>>,
    pending_resize: Option<(u16, u16)>,
    history_base: usize,
}

impl Deref for State {
    type Target = ConnectionState;

    fn deref(&self) -> &Self::Target { &self.active }
}

impl DerefMut for State {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.active }
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        let owner = cx.entity().downgrade();
        let input_focus = focus.clone();
        let input_owner = owner.clone();
        let input = cx.new(move |_| TerminalInput {
            owner: input_owner,
            focus: input_focus,
            marked: None,
            selected: 0..0,
            caret: None,
        });
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            if let Some(owner) = owner.upgrade() {
                let active = {
                    let view = owner.read(cx);
                    view.ssh_terminal.visible && view.ssh_terminal.focus.is_focused(window)
                };
                if active {
                    owner.update(cx, |view, cx| view.ssh_terminal_key(&event.keystroke, window, cx));
                }
            }
        });
        Self {
            visible: false,
            profile_id: 0,
            focus,
            paste_button: PasteButton::Right,
            active: ConnectionState::new(),
            parked: BTreeMap::new(),
            tabs: Vec::new(),
            tab_scroll: gpui::ScrollHandle::new(),
            reveal_tab: None,
            toolbar_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            input,
            task: None,
            _keys: keys,
            height: 260.,
            resize_drag: None,
            generation: 0,
            window_transparent: false,
        }
    }

    pub(super) fn activate_profile(&mut self, profile: &Profile) {
        if !self.tabs.contains(&profile.id) { self.tabs.push(profile.id); }
        if self.profile_id != profile.id {
            let active = self.parked.remove(&profile.id).unwrap_or_else(ConnectionState::new);
            let previous = std::mem::replace(&mut self.active, active);
            self.parked.insert(self.profile_id, previous);
            self.profile_id = profile.id;
        }
        self.active.set_appearance(profile);
        if self.active.endpoint.is_empty() { self.active.set_profile(profile); }
        self.active.bounds = None;
        self.active.scrollbar_drag = false;
        self.active.selecting = false;
        self.active.copy_on_release = false;
        self.resize_drag = None;
        self.visible = true;
        self.reveal_tab = Some(profile.id);
        if let Some(index) = self.tabs.iter().position(|id| *id == profile.id) { self.tab_scroll.scroll_to_item(index); }
    }

    fn switch_tab(&mut self, id: u64) -> bool {
        if !self.tabs.contains(&id) { return false; }
        if self.profile_id != id {
            let Some(next) = self.parked.remove(&id) else { return false; };
            let previous = std::mem::replace(&mut self.active, next);
            self.parked.insert(self.profile_id, previous);
            self.profile_id = id;
        }
        self.active.bounds = None;
        self.active.scrollbar_drag = false;
        self.active.selecting = false;
        self.active.copy_on_release = false;
        self.resize_drag = None;
        self.visible = true;
        self.reveal_tab = Some(id);
        if let Some(index) = self.tabs.iter().position(|tab| *tab == id) { self.tab_scroll.scroll_to_item(index); }
        true
    }

    fn close_tab(&mut self, id: u64) {
        let Some(index) = self.tabs.iter().position(|tab| *tab == id) else { return; };
        self.tabs.remove(index);
        if self.profile_id == id {
            self.active.close();
            if let Some(next) = self.tabs.get(index.min(self.tabs.len().saturating_sub(1))).copied() {
                self.switch_tab(next);
                self.parked.remove(&id);
            } else {
                self.parked.remove(&0);
                self.active = ConnectionState::new();
                self.profile_id = 0;
                self.visible = false;
                self.reveal_tab = None;
                self.tab_scroll.set_offset(point(px(0.), px(0.)));
            }
        } else { self.parked.remove(&id); }
        if self.visible { self.reveal_tab = Some(self.profile_id); }
    }

    pub(super) fn endpoint_label(&self) -> String {
        format!("{} · {}", self.active.name, self.active.endpoint)
    }

    pub(super) fn apply_settings(&mut self, settings: &Settings) {
        self.paste_button = settings.paste_button;
        if let Some(profile) = settings.profile(self.profile_id) { self.active.set_appearance(profile); }
        for (id, connection) in &mut self.parked {
            if let Some(profile) = settings.profile(*id) { connection.set_appearance(profile); }
        }
    }

    pub(super) fn sync_window_background(&mut self, window: &mut Window, hidden: bool) -> bool {
        let transparent = !hidden && self.visible && self.active.transparency > 0;
        if self.window_transparent != transparent {
            window.set_background_appearance(if transparent { gpui::WindowBackgroundAppearance::Transparent }
                else { gpui::WindowBackgroundAppearance::Opaque });
            self.window_transparent = transparent;
        }
        transparent
    }

    pub(super) fn retain_profiles(&mut self, settings: &Settings) {
        let removed = self.tabs.iter().copied().filter(|id| settings.profile(*id).is_none()).collect::<Vec<_>>();
        for id in removed { self.close_tab(id); }
        self.parked.retain(|id, _| *id == 0 || settings.profile(*id).is_some());
    }

    pub(super) fn close(&mut self) {
        self.active.close();
        self.resize_drag = None;
    }
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            session: None,
            fingerprint: None,
            status: String::new(),
            name: String::new(),
            endpoint: String::new(),
            font_size: 13,
            background_color: 0x000000,
            transparency: 0,
            parser: vt100::Parser::new_with_callbacks(vt100::Parser::DEFAULT_ROWS, vt100::Parser::DEFAULT_COLS, SCROLLBACK, TerminalCallbacks::default()),
            bounds: None,
            cell_width: 8.,
            scrollbar_drag: false,
            selecting: false,
            copy_on_release: false,
            selection: None,
            blink: true,
            pending_input: VecDeque::new(),
            pending_resize: None,
            history_base: 0,
        }
    }

    fn set_profile(&mut self, profile: &Profile) {
        self.name = profile.name.clone();
        self.endpoint = if profile.username.is_empty() {
            format!("{}:{}", profile.host, profile.port)
        } else {
            format!("{}@{}:{}", profile.username, profile.host, profile.port)
        };
        self.set_appearance(profile);
    }

    fn set_appearance(&mut self, profile: &Profile) {
        self.font_size = profile.font_size.clamp(8, 36);
        self.background_color = profile.background_color;
        self.transparency = profile.transparency;
    }

    fn font_size(&self) -> f32 { f32::from(self.font_size) }

    fn line_height(&self) -> f32 { self.font_size() * 18. / 13. }

    pub(super) fn feed(&mut self, data: &[u8]) {
        let offset = self.parser.screen().scrollback();
        let old_history = self.parser.screen().scrollback_len();
        let old_top = self.visible_top();
        let alternate = self.parser.screen().alternate_screen();
        self.parser.callbacks_mut().discarded_rows = 0;
        self.parser.process(data);
        let history = self.parser.screen().scrollback_len();
        if alternate != self.parser.screen().alternate_screen() {
            self.history_base = 0;
            self.selection = None;
            self.selecting = false;
            self.copy_on_release = false;
        } else {
            self.history_base = self.history_base.saturating_add(self.parser.callbacks().discarded_rows)
                .saturating_add(old_history.saturating_sub(history));
            if offset > 0 {
                let live_top = self.history_base.saturating_add(history);
                self.parser.screen_mut().set_scrollback(live_top.saturating_sub(old_top));
            }
            self.clamp_selection();
        }
        let replies = std::mem::take(&mut self.parser.callbacks_mut().replies);
        if !replies.is_empty() { self.send(replies); }
        self.blink = true;
    }

    fn start(&mut self, profile: &Profile, session: Session) {
        self.close();
        self.reset();
        self.set_profile(profile);
        self.session = Some(session);
        self.status = "Connecting…".into();
        let (rows, columns) = self.parser.screen().size();
        self.pending_resize = Some((columns.get(), rows.get()));
        self.flush();
    }

    pub(super) fn reset(&mut self) {
        let (rows, columns) = self.parser.screen().size();
        self.parser = vt100::Parser::new_with_callbacks(rows, columns, SCROLLBACK, TerminalCallbacks::default());
        self.history_base = 0;
        self.selection = None;
        self.fingerprint = None;
        self.selecting = false;
        self.copy_on_release = false;
        self.pending_input.clear();
        self.pending_resize = None;
        self.blink = true;
    }

    pub(super) fn close(&mut self) {
        if let Some(session) = self.session.take() { session.close(); }
        self.fingerprint = None;
        self.status = "Disconnected".into();
        self.scrollbar_drag = false;
        self.selecting = false;
        self.copy_on_release = false;
        self.pending_input.clear();
        self.pending_resize = None;
    }

    fn send(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() { return; }
        if self.session.is_none() { return; }
        self.pending_input.extend(bytes.chunks(65_536).map(<[u8]>::to_vec));
        self.flush();
    }

    fn flush(&mut self) {
        let Some(session) = &self.session else { return; };
        if let Some((columns, rows)) = self.pending_resize {
            match session.resize(columns, rows) {
                Ok(()) => self.pending_resize = None,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
                Err(error) => { self.status = error.to_string(); return; }
            }
        }
        for _ in 0..32 {
            let Some(bytes) = self.pending_input.front() else { break; };
            match session.input(bytes.clone()) {
                Ok(()) => { self.pending_input.pop_front(); }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => { self.status = error.to_string(); break; }
            }
        }
    }

    fn poll(&mut self) -> (bool, bool, bool) {
        let previous_status = self.status.clone();
        self.flush();
        let mut changed = previous_status != self.status;
        let mut host_key = false;
        let mut dismissed_host = false;
        for _ in 0..256 {
            let event = match self.session.as_ref() {
                Some(session) => session.events.try_recv(),
                None => break,
            };
            match event {
                Ok(Event::HostKey { fingerprint }) => {
                    self.fingerprint = Some(fingerprint);
                    self.status = "Awaiting host key confirmation".into();
                    host_key = true;
                }
                Ok(Event::Connected) => {
                    self.status = "Connected".into();
                    self.fingerprint = None;
                }
                Ok(Event::Data(data)) => self.feed(&data),
                Ok(Event::Error(error)) => self.status = error,
                Ok(Event::Closed) | Err(TryRecvError::Disconnected) => {
                    if let Some(session) = self.session.take() { session.close(); }
                    self.pending_input.clear();
                    self.pending_resize = None;
                    dismissed_host = self.fingerprint.take().is_some();
                    host_key = false;
                    if self.status == "Connected" || self.status == "Connecting…" || self.status == "Awaiting host key confirmation" {
                        self.status = "Disconnected".into();
                    }
                    changed = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
            changed = true;
        }
        (changed, host_key, dismissed_host)
    }

    fn scroll(&mut self, rows: isize) {
        self.scroll_selection(rows);
        self.selection = None;
    }

    fn scroll_selection(&mut self, rows: isize) {
        if self.parser.screen().alternate_screen() { return; }
        let offset = self.parser.screen().scrollback().saturating_add_signed(rows);
        self.parser.screen_mut().set_scrollback(offset);
        self.clamp_selection();
    }

    fn visible_top(&self) -> usize {
        self.history_base.saturating_add(self.parser.screen().scrollback_len().saturating_sub(self.parser.screen().scrollback()))
    }

    fn clamp_selection(&mut self) {
        let Some((anchor, head)) = self.selection else { return; };
        let (rows, columns) = self.parser.screen().size();
        let first = CellPoint { row: self.history_base, column: 0 };
        let last = CellPoint {
            row: self.history_base.saturating_add(self.parser.screen().scrollback_len()).saturating_add(usize::from(rows.get()) - 1),
            column: columns.get(),
        };
        if anchor.max(head) < first || anchor.min(head) > last {
            self.selection = None;
            self.selecting = false;
            self.copy_on_release = false;
        } else {
            self.selection = Some((anchor.clamp(first, last), head.clamp(first, last)));
        }
    }

    fn cell_at(&self, position: Point<gpui::Pixels>) -> Option<CellPoint> {
        let bounds = self.bounds?;
        let (rows, columns) = self.parser.screen().size();
        Some(CellPoint {
            row: self.visible_top().saturating_add(usize::from(((f32::from(position.y - bounds.top()) / self.line_height()).floor().max(0.) as u16).min(rows.get() - 1))),
            column: ((f32::from(position.x - bounds.left()) / self.cell_width).round().max(0.) as u16).min(columns.get()),
        })
    }

    fn scroll_to(&mut self, position: Point<gpui::Pixels>) {
        let Some(bounds) = self.bounds else { return; };
        let history = self.parser.screen().scrollback_len();
        let track = editor_surface::scrollbar_track(f32::from(bounds.size.height));
        let fraction = ((f32::from(position.y - bounds.top()) - track.start) / (track.end - track.start).max(1.)).clamp(0., 1.);
        self.parser.screen_mut().set_scrollback(((1. - fraction) * history as f32).round() as usize);
        self.selection = None;
    }

    fn word_selection_at(&self, position: Point<gpui::Pixels>) -> Option<(CellPoint, CellPoint)> {
        let at = self.cell_at(position)?;
        let bounds = self.bounds?;
        let screen = self.parser.screen();
        let columns = screen.size().1.get();
        let row = u16::try_from(at.row.saturating_sub(self.visible_top())).ok()?;
        let mut column = ((f32::from(position.x - bounds.left()) / self.cell_width).floor().max(0.) as u16).min(columns - 1);
        if screen.cell(row, column).is_some_and(|cell| cell.is_wide_continuation()) { column = column.saturating_sub(1); }
        let class = |mut column: u16| {
            if screen.cell(row, column).is_some_and(|cell| cell.is_wide_continuation()) { column = column.saturating_sub(1); }
            screen.cell(row, column).and_then(|cell| cell.contents().chars().next())
                .map(crate::unicode_class::classify).unwrap_or(crate::unicode_class::CharacterClass::Space)
        };
        let initial = class(column);
        let mut start = column;
        while start > 0 && class(start - 1) == initial { start -= 1; }
        let mut end = column + 1;
        while end < columns && class(end) == initial { end += 1; }
        Some((CellPoint { row: at.row, column: start }, CellPoint { row: at.row, column: end }))
    }

    fn copy(&self, cx: &mut App) {
        if let Some((anchor, head)) = self.selection.filter(|(anchor, head)| anchor != head) {
            let mut screen = self.parser.screen().clone();
            let (rows, columns) = screen.size();
            let history = screen.scrollback_len();
            let first = CellPoint { row: self.history_base, column: 0 };
            let last = CellPoint { row: self.history_base.saturating_add(history).saturating_add(usize::from(rows.get()) - 1), column: columns.get() };
            let (start, end) = if anchor <= head { (anchor, head) } else { (head, anchor) };
            if end < first || start > last { return; }
            let start = start.clamp(first, last);
            let end = end.clamp(first, last);
            if start == end { return; }
            let mut text = String::new();
            for logical_row in start.row..=end.row {
                let index = logical_row.saturating_sub(self.history_base);
                let row = if index < history {
                    screen.set_scrollback(history - index);
                    0
                } else {
                    screen.set_scrollback(0);
                    (index - history) as u16
                };
                let mut start_column = if logical_row == start.row { start.column } else { 0 };
                let mut end_column = if logical_row == end.row { end.column } else { columns.get() };
                if screen.cell(row, start_column).is_some_and(|cell| cell.is_wide_continuation()) { start_column = start_column.saturating_sub(1); }
                if screen.cell(row, end_column).is_some_and(|cell| cell.is_wide_continuation()) { end_column = end_column.saturating_add(1).min(columns.get()); }
                text.push_str(&screen.contents_between(row, start_column, row, end_column));
                if logical_row < end.row && !screen.row_wrapped(row) { text.push('\n'); }
            }
            if !text.is_empty() { cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)); }
        }
    }

    fn paste(&mut self, cx: &mut App) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return; };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let bytes = if self.parser.screen().bracketed_paste() {
            let text = text.replace('\x1b', "");
            format!("\x1b[200~{text}\x1b[201~").into_bytes()
        } else { text.replace('\n', "\r").into_bytes() };
        self.parser.screen_mut().set_scrollback(0);
        self.selection = None;
        self.selecting = false;
        self.copy_on_release = false;
        self.send(bytes);
    }
}

impl Drop for ConnectionState {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() { session.close(); }
    }
}

impl MatrixWindow {
    fn paste_ssh_mouse(&mut self, button: MouseButton, window: &mut Window, cx: &mut Context<Self>) {
        let selected = matches!((self.ssh_terminal.paste_button, button),
            (PasteButton::Middle, MouseButton::Middle) | (PasteButton::Right, MouseButton::Right));
        if selected {
            self.ssh_terminal.focus.focus(window, cx);
            self.ssh_terminal.paste(cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn switch_ssh_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_config.password_visible {
            self.present_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
            return;
        }
        if self.ssh_terminal.fingerprint.is_some() {
            self.show_terminal_host_key(window, cx);
            return;
        }
        let changed = self.ssh_terminal.profile_id != id;
        if !self.ssh_terminal.switch_tab(id) { return; }
        if changed {
            self.ssh_terminal.input.update(cx, |input, _| { input.marked = None; input.selected = 0..0; input.caret = None; });
            window.notify_text_input_changed(&self.ssh_terminal.focus);
        }
        self.ssh_terminal.focus.focus(window, cx);
        if self.ssh_terminal.fingerprint.is_some() { self.show_terminal_host_key(window, cx); }
        cx.notify();
    }

    fn close_ssh_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let changed = self.ssh_terminal.profile_id == id;
        self.terminal_config.cancel_password_for(id);
        if self.ssh_terminal.profile_id == id && self.ssh_terminal.fingerprint.is_some() { self.cancel_terminal_host_key(cx); }
        self.ssh_terminal.close_tab(id);
        if changed {
            self.ssh_terminal.input.update(cx, |input, _| { input.marked = None; input.selected = 0..0; input.caret = None; });
            window.notify_text_input_changed(&self.ssh_terminal.focus);
        }
        if self.terminal_config.password_visible {
            self.present_floating(floating_windows::DialogKind::TerminalPassword, window, cx);
        } else if self.ssh_terminal.fingerprint.is_some() {
            self.show_terminal_host_key(window, cx);
        } else if self.ssh_terminal.visible {
            self.ssh_terminal.focus.focus(window, cx);
        } else { self.editor_focus.focus(window, cx); }
        cx.notify();
    }

    fn activate_ssh_toolbar(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match index {
            0 => { self.ssh_terminal.copy(cx); self.ssh_terminal.focus.focus(window, cx); }
            1 => { self.ssh_terminal.paste(cx); self.ssh_terminal.focus.focus(window, cx); }
            2 => self.reconnect_terminal(window, cx),
            3 => self.close_ssh_tab(self.ssh_terminal.profile_id, window, cx),
            _ => return,
        }
        cx.notify();
    }

    fn ssh_toolbar_button(&self, index: usize, label: &'static str, window: &Window, cx: &Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        div().id(["ssh-terminal-copy", "ssh-terminal-paste", "ssh-terminal-reconnect", "ssh-terminal-close"][index])
            .role(Role::Button).aria_label(label).track_focus(&self.ssh_terminal.toolbar_focus[index])
            .h(px(22.)).min_w(px(if index == 2 { 84. } else { 48. })).px(px(8.)).flex_shrink_0()
            .flex().items_center().justify_center().font_family(dialog_font::font_family()).text_size(px(12.))
            .border_1().border_color(rgb(palette.edge)).bg(rgb(palette.button_background(true)))
            .when(self.ssh_terminal.toolbar_focus[index].is_focused(window), |button| button.border_color(rgb(palette.hot_edge)))
            .text_color(rgb(palette.text)).cursor_pointer()
            .hover(move |style| style.border_color(rgb(palette.hot_edge)).bg(rgb(palette.hot_background)))
            .on_mouse_down(MouseButton::Left, cx.listener(move |view, _, window, cx| {
                view.ssh_terminal.toolbar_focus[index].focus(window, cx);
                cx.stop_propagation();
            }))
            .on_click(cx.listener(move |view, _, window, cx| view.activate_ssh_toolbar(index, window, cx)))
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "space" | "enter" => view.activate_ssh_toolbar(index, window, cx),
                    "tab" => if event.keystroke.modifiers.shift { window.focus_prev(cx); } else { window.focus_next(cx); },
                    _ => return,
                }
                cx.stop_propagation();
            })).child(label).into_any_element()
    }

    pub(super) fn start_ssh_terminal(&mut self, profile: Profile, session: Session, window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_terminal.activate_profile(&profile);
        self.ssh_terminal.active.start(&profile, session);
        self.ssh_terminal.input.update(cx, |input, _| { input.marked = None; input.selected = 0..0; input.caret = None; });
        window.notify_text_input_changed(&self.ssh_terminal.focus);
        self.ssh_terminal.focus.focus(window, cx);
        self.poll_ssh_terminal(window, cx);
        cx.notify();
    }

    pub(super) fn activate_ssh_profile(&mut self, profile: &Profile, window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_terminal.activate_profile(profile);
        self.ssh_terminal.input.update(cx, |input, _| { input.marked = None; input.selected = 0..0; input.caret = None; });
        window.notify_text_input_changed(&self.ssh_terminal.focus);
        self.ssh_terminal.focus.focus(window, cx);
        if self.ssh_terminal.fingerprint.is_some() { self.show_terminal_host_key(window, cx); }
        cx.notify();
    }

    pub(super) fn poll_ssh_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_terminal.task = None;
        self.ssh_terminal.generation = self.ssh_terminal.generation.wrapping_add(1);
        let generation = self.ssh_terminal.generation;
        let executor = cx.background_executor().clone();
        self.ssh_terminal.task = Some(cx.spawn_in(window, async move |view, cx| {
            let mut ticks = 0usize;
            loop {
                executor.timer(Duration::from_millis(16)).await;
                ticks += 1;
                let active = view.update_in(cx, |view, window, cx| {
                    if view.ssh_terminal.generation != generation { return false; }
                    let (mut changed, host_key, dismissed_host) = view.ssh_terminal.active.poll();
                    for connection in view.ssh_terminal.parked.values_mut() {
                        connection.poll();
                    }
                    if dismissed_host { view.dismiss_floating(floating_windows::DialogKind::TerminalHostKey, window, cx); }
                    if host_key { view.show_terminal_host_key(window, cx); }
                    if ticks.is_multiple_of(30) {
                        view.ssh_terminal.blink = !view.ssh_terminal.blink;
                        changed |= view.ssh_terminal.focus.is_focused(window);
                    }
                    if changed { cx.notify(); }
                    view.ssh_terminal.session.is_some() || view.ssh_terminal.parked.values().any(|connection| connection.session.is_some())
                });
                if !matches!(active, Ok(true)) { break; }
            }
        }));
    }

    fn ssh_terminal_key(&mut self, key: &gpui::Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = key.modifiers;
        if key.is_modifier_key() { return; }
        if modifiers.alt { self.alt_used = true; }
        let selected = self.ssh_terminal.selection.is_some_and(|(anchor, head)| anchor != head);
        let copy = key.key == "c" && (modifiers.platform || modifiers.control && (modifiers.shift || selected));
        let paste = key.key == "v" && (modifiers.platform || modifiers.control && modifiers.shift)
            || key.key == "insert" && modifiers.shift;
        if copy { self.ssh_terminal.copy(cx); }
        else if paste { self.ssh_terminal.paste(cx); }
        else if modifiers.platform && key.key == "a" {
            let (rows, columns) = self.ssh_terminal.parser.screen().size();
            let first_row = self.ssh_terminal.visible_top();
            self.ssh_terminal.selection = Some((CellPoint { row: first_row, column: 0 }, CellPoint { row: first_row.saturating_add(usize::from(rows.get()) - 1), column: columns.get() }));
        } else if modifiers.shift && !modifiers.control && !modifiers.alt && matches!(key.key.as_str(), "pageup" | "pagedown") {
            let rows = self.ssh_terminal.parser.screen().size().0.get() as isize;
            self.ssh_terminal.scroll(if key.key == "pageup" { rows } else { -rows });
        } else if key.key == "f5" {
            self.open_terminal(window, cx);
        } else {
            let composing = self.ssh_terminal.input.read(cx).marked.is_some();
            if composing && !modifiers.control && !modifiers.alt && !modifiers.platform { return; }
            let Some(bytes) = encode_key(key, self.ssh_terminal.parser.screen().application_cursor()) else { return; };
            self.ssh_terminal.parser.screen_mut().set_scrollback(0);
            self.ssh_terminal.selection = None;
            self.ssh_terminal.send(bytes);
            self.ssh_terminal.blink = true;
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn render_ssh_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.ssh_terminal.visible { return None; }
        if self.ssh_terminal.reveal_tab.is_some() {
            let owner = cx.weak_entity();
            window.on_next_frame(move |_, cx| {
                if let Some(owner) = owner.upgrade() {
                    owner.update(cx, |view, cx| {
                        if let Some(id) = view.ssh_terminal.reveal_tab.take()
                            && let Some(index) = view.ssh_terminal.tabs.iter().position(|tab| *tab == id) {
                            let before = view.ssh_terminal.tab_scroll.offset();
                            view.ssh_terminal.tab_scroll.scroll_to_item(index);
                            if view.ssh_terminal.tab_scroll.offset() != before { cx.notify(); }
                        }
                    });
                }
            });
        }
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let dark = self.dark;
        let reduce = self.pref_bool("TabBar.reduce", true);
        let extra_height = if reduce { 0. } else { 8. };
        let tab_text_size = px(if reduce { 14. } else { 17.5 });
        let strip_height = (if dark { 27. } else if self.pref_bool("ScintillaPrimaryView.borderEdge", true) { 28. } else { 26. }) + extra_height;
        let tab_background = self.appearance_colour("DarkMode.customColorMain", 0xf0f0f0);
        let tab_selected = self.appearance_colour("DarkMode.customColorActive", 0xf0f0f0);
        let tab_inactive = if !self.pref_bool("TabBar.drawInactiveTab", true) { tab_selected }
            else if dark { tab_background } else { self.style_colour("Inactive tabs", "bgColor", 0xc0c0c0) };
        let inactive_text = if dark { self.appearance_colour("DarkMode.customColorDarkText", 0xc0c0c0) }
            else { self.style_colour("Inactive tabs", "fgColor", 0x808080) };
        let active_text = if dark { self.appearance_colour("DarkMode.customColorText", 0xe0e0e0) }
            else { self.style_colour("Active tab text", "fgColor", 0x000000) };
        let indicator = self.style_colour("Active tab focused indicator", "fgColor", 0xfaaa3c);
        let tab_font = window.text_system().resolve_font(&gpui::font("Helvetica"));
        let text_descent = f32::from(window.text_system().descent(tab_font, tab_text_size)).ceil() as i32;
        let hover_background = workspace_surface::tab_colour(tab_inactive, dark, false, true);
        let font_size = self.ssh_terminal.font_size();
        let line_height = self.ssh_terminal.line_height();
        let tabs = self.ssh_terminal.tabs.iter().filter_map(|id| {
            let connection = if *id == self.ssh_terminal.profile_id { Some(&self.ssh_terminal.active) }
                else { self.ssh_terminal.parked.get(id) }?;
            Some((*id, if *id == 0 { self.terminal_label("本机", "Local").to_owned() } else { connection.name.clone() }, connection.endpoint.clone()))
        }).collect::<Vec<_>>();
        let status = match self.ssh_terminal.status.as_str() {
            "" | "Connected" => None,
            "Connecting…" => Some(self.terminal_label("正在连接…", "Connecting…").to_owned()),
            "Disconnected" => Some(self.terminal_label("已断开连接", "Disconnected").to_owned()),
            "Awaiting host key confirmation" => Some(self.terminal_label("等待确认主机密钥", "Awaiting host key confirmation").to_owned()),
            status => Some(status.to_owned()),
        };
        Some(div().id("ssh-terminal-panel").w_full().h(px(self.ssh_terminal.height)).min_h(px(100.)).flex_shrink_0()
            .flex().flex_col().text_color(rgb(FOREGROUND))
            .child(div().id("ssh-terminal-resize").role(Role::Splitter).aria_label(self.terminal_label("调整 SSH 终端高度", "Resize SSH terminal"))
                .w_full().h(px(4.)).flex_shrink_0().bg(rgb(0x606060)).cursor(gpui::CursorStyle::ResizeUpDown)
                .on_mouse_down(MouseButton::Left, cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                    view.ssh_terminal.resize_drag = Some((event.position.y, view.ssh_terminal.height));
                    cx.stop_propagation();
                 })))
            .child(div().id("ssh-terminal-header").h(px(strip_height)).w_full().flex_shrink_0().flex().items_center().gap(px(6.)).pr(px(6.))
                .bg(rgb(tab_background))
                .text_color(rgb(if self.dark { FOREGROUND } else { 0x202020 })).text_size(px(12.))
                .child(div().id("ssh-terminal-tabs").role(Role::TabList).aria_label(self.terminal_label("终端连接", "Terminal connections"))
                    .h(px(strip_height)).flex_1().min_w(px(0.)).flex().overflow_x_scroll().track_scroll(&self.ssh_terminal.tab_scroll)
                .children(tabs.into_iter().map(|(id, name, endpoint)| {
                    let active = id == self.ssh_terminal.profile_id;
                    let tab_height = (if dark { if active { 25. } else { 20. } } else if active { 26. } else { 22. }) + extra_height;
                    div().id(gpui::SharedString::from(format!("ssh-terminal-tab-{id}"))).role(Role::Tab).aria_label(name.clone()).aria_selected(active)
                        .focusable().tab_stop(true).relative().h(px(tab_height)).mt(px(if dark { if active { 2. } else { 4. } } else if active { 0. } else { 2. }))
                        .min_w(px(90.)).max_w(px(240.)).px(px(if active { 5. } else { 3. })).flex_shrink_0()
                        .when(!dark && active, |tab| tab.ml(px(-2.)))
                        .flex().items_center().font_family("Helvetica").text_size(tab_text_size)
                        .text_color(rgb(if active { active_text } else { inactive_text })).cursor_pointer()
                        .bg(rgb(if active { tab_selected } else { tab_inactive }))
                        .border_1().when(!dark || active, |tab| tab.border_b_0())
                        .border_color(if dark { rgb(0x646464) } else { gpui::rgba(0) })
                        .when(!active && tab_inactive != tab_selected, |tab| tab.hover(move |style| style.bg(rgb(hover_background))))
                        .child(div().absolute().top(px(if dark { 0. } else { -1. })).left(px(if dark { 0. } else { -1. }))
                            .bottom(px(0.)).w(px(1.)).bg(rgb(if dark { 0x646464 } else { 0xffffff })))
                        .child(div().absolute().top(px(if dark { 0. } else { -1. })).left(px(if dark { 0. } else { -1. }))
                            .right(px(if dark { 0. } else { -1. })).h(px(1.)).bg(rgb(if dark { 0x646464 } else { 0xffffff })))
                        .when(!dark, |tab| tab.child(workspace_surface::classic_tab_frame(active, tab_background, tab_selected)))
                        .when(active, |tab| tab.child(div().absolute().top(px(if dark { 0. } else { 1. }))
                            .left(px(if dark { 0. } else { 1. })).right(px(if dark { 0. } else { 1. })).h(px(4.)).bg(rgb(indicator))))
                        .tooltip(move |_, cx| icon_tooltip::view(endpoint.clone(), dark, cx))
                        .on_click(cx.listener(move |view, _, window, cx| view.switch_ssh_tab(id, window, cx)))
                        .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                            match event.keystroke.key.as_str() {
                                "space" | "enter" => view.switch_ssh_tab(id, window, cx),
                                "tab" => if event.keystroke.modifiers.shift { window.focus_prev(cx); } else { window.focus_next(cx); },
                                _ => return,
                            }
                            cx.stop_propagation();
                        }))
                        .child(div().flex_1().min_w(px(0.)).truncate().line_height(px(20. + extra_height)).relative()
                            .top(px(2. + ((text_descent + i32::from(dark || !active)) / 2) as f32)).child(format!(" {name}")))
                })))
                .children(status.map(|status| div().id("ssh-terminal-status").role(Role::Status).aria_label(status.clone())
                    .max_w(px(240.)).min_w(px(0.)).truncate().child(status)))
                .child(div().flex().gap(px(6.)).flex_shrink_0()
                    .child(self.ssh_toolbar_button(0, self.terminal_label("复制", "Copy"), window, cx))
                    .child(self.ssh_toolbar_button(1, self.terminal_label("粘贴", "Paste"), window, cx))
                    .child(self.ssh_toolbar_button(2, self.terminal_label("重新连接", "Reconnect"), window, cx))
                    .child(self.ssh_toolbar_button(3, self.terminal_label("关闭", "Close"), window, cx))))
            .child(div().id("ssh-terminal-grid").role(Role::MultilineTextInput).aria_label(self.terminal_label("SSH 终端", "SSH terminal"))
                .track_focus(&self.ssh_terminal.focus).key_context("SshTerminal").flex_1().min_h(px(0.)).w_full().overflow_hidden().cursor(gpui::CursorStyle::IBeam)
                .on_modifiers_changed(cx.listener(|view, _, window, cx| {
                    if view.ssh_terminal.focus.is_focused(window) { cx.stop_propagation(); }
                }))
                .on_mouse_down(MouseButton::Left, cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                    view.ssh_terminal.focus.focus(window, cx);
                    let terminal = &mut view.ssh_terminal;
                    terminal.copy_on_release = false;
                    let scrollbar = terminal.bounds.filter(|bounds| event.position.x >= bounds.right());
                    terminal.scrollbar_drag = false;
                    if let Some(bounds) = scrollbar {
                        let track = editor_surface::scrollbar_track(f32::from(bounds.size.height));
                        let position = f32::from(event.position.y - bounds.top());
                        if position < track.start { terminal.scroll(1); }
                        else if position >= track.end { terminal.scroll(-1); }
                        else { terminal.scrollbar_drag = true; terminal.scroll_to(event.position); }
                    }
                    else if event.click_count == 2 {
                        terminal.selection = terminal.word_selection_at(event.position);
                        terminal.selecting = false;
                        terminal.copy_on_release = terminal.selection.is_some();
                    }
                    else if let Some(cell) = terminal.cell_at(event.position) {
                        let anchor = if event.modifiers.shift { terminal.selection.map_or(cell, |selection| selection.0) } else { cell };
                        terminal.selection = Some((anchor, cell)); terminal.selecting = true;
                    }
                    cx.notify(); cx.stop_propagation();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|view, _, window, cx| view.paste_ssh_mouse(MouseButton::Right, window, cx)))
                .on_mouse_down(MouseButton::Middle, cx.listener(|view, _, window, cx| view.paste_ssh_mouse(MouseButton::Middle, window, cx)))
                .on_scroll(cx.listener(|view, event: &gpui::ScrollEvent, _, cx| {
                    let line_height = view.ssh_terminal.line_height();
                    let delta = f32::from(event.delta.pixel_delta(px(line_height)).y) / line_height;
                    let rows = if delta < 0. { delta.floor() } else { delta.ceil() } as isize;
                    view.ssh_terminal.scroll(rows); cx.notify(); cx.stop_propagation();
                }))
                .child(gpui::canvas(move |bounds, window, cx| {
                    let run = TextRun { len: 1, font: gpui::font("Menlo"), color: rgb(FOREGROUND).into(), background_color: None, underline: None, strikethrough: None };
                    let width = f32::from(window.text_system().shape_line("M".into(), px(font_size), &[run], None).width()).max(1.);
                    entity.update(cx, |view, cx| {
                        let terminal = &mut view.ssh_terminal;
                         terminal.bounds = Some(Bounds::new(bounds.origin, size((bounds.size.width - px(editor_surface::SCROLLBAR_SIZE)).max(px(1.)), bounds.size.height)));
                        terminal.cell_width = width;
                         let columns = NonZeroU16::new((f32::from(bounds.size.width - px(editor_surface::SCROLLBAR_SIZE)) / width).floor().clamp(1., 1000.) as u16).unwrap_or(NonZeroU16::MIN);
                        let rows = NonZeroU16::new((f32::from(bounds.size.height) / line_height).floor().clamp(1., 1000.) as u16).unwrap_or(NonZeroU16::MIN);
                        if terminal.parser.screen().size() != (rows, columns) {
                            terminal.parser.callbacks_mut().discarded_rows = 0;
                            terminal.parser.set_size(rows, columns);
                            terminal.history_base = terminal.history_base.saturating_add(terminal.parser.callbacks().discarded_rows);
                            terminal.selection = None;
                            terminal.pending_resize = Some((columns.get(), rows.get()));
                            terminal.flush();
                            cx.notify();
                        }
                        VisibleGrid::new(terminal.parser.screen(), terminal.history_base)
                    })
                }, move |bounds, screen, window, cx| {
                    let (focus, input, selection, width, connected, blink, background_color, transparency) = {
                        let terminal = &paint_entity.read(cx).ssh_terminal;
                        (terminal.focus.clone(), terminal.input.clone(), terminal.selection,
                            terminal.cell_width, terminal.session.is_some(), terminal.blink, terminal.background_color, terminal.transparency)
                    };
                    let blinking = matches!(screen.cursor_style, vt100::CursorStyle::Default | vt100::CursorStyle::BlinkBlock | vt100::CursorStyle::BlinkUnderline | vt100::CursorStyle::BlinkVerticalBar);
                    let cursor_visible = connected && focus.is_focused(window) && (!blinking || blink);
                    let viewport = Bounds::new(bounds.origin, size((bounds.size.width - px(editor_surface::SCROLLBAR_SIZE)).max(px(1.)), bounds.size.height));
                    let (row, column) = screen.cursor;
                    let caret = Bounds::new(point(bounds.left() + px(width * f32::from(column)), bounds.top() + px(line_height * f32::from(row))), size(px(width), px(line_height)));
                    let marked = { input.read(cx).marked.clone() };
                    input.update(cx, |input, _| input.caret = Some(caret));
                    window.handle_input(&focus, ElementInputHandler::new(viewport, input), cx);
                    window.with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
                        paint_screen(&screen, selection, cursor_visible, width, font_size, line_height, viewport, background_color, transparency, marked.is_some(), window, cx);
                        if let Some(marked) = marked {
                            paint_text(&marked, caret.origin, rgb(FOREGROUND).into(), false, false, true, font_size, line_height, window, cx);
                        }
                    });
                    let history = screen.history;
                    let rows = usize::from(screen.rows);
                    let bar = Bounds::new(point(viewport.right(), bounds.top()), size(px(editor_surface::SCROLLBAR_SIZE), bounds.size.height));
                    let track = editor_surface::scrollbar_track(f32::from(bounds.size.height));
                    let axis = editor_surface::ScrollAxis::new(track.end - track.start, (history + rows) as f32, rows as f32, history.saturating_sub(screen.scrollback) as f32);
                    let thumb = axis.thumb();
                    let (track_color, thumb_color) = editor_surface::scrollbar_colors(dark);
                    window.paint_quad(fill(bar, rgb(track_color)));
                    window.paint_quad(fill(Bounds::new(point(bar.left() + px(2.), bar.top() + px(track.start + thumb.start)),
                        size(px(editor_surface::SCROLLBAR_SIZE - 4.), px(thumb.end - thumb.start))), rgb(thumb_color)));
                    editor_surface::paint_scroll_arrow(Bounds::new(bar.origin, size(bar.size.width, px(track.start))), true, true, screen.scrollback < history, window);
                    editor_surface::paint_scroll_arrow(Bounds::new(point(bar.left(), bar.top() + px(track.end)), size(bar.size.width, px(f32::from(bar.size.height) - track.end))), true, false, screen.scrollback > 0, window);
                    let owner = paint_entity.downgrade();
                    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                        if phase != gpui::DispatchPhase::Capture { return; }
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |view, cx| {
                                let terminal = &mut view.ssh_terminal;
                                if event.pressed_button != Some(MouseButton::Left) {
                                    terminal.resize_drag = None; terminal.scrollbar_drag = false; terminal.selecting = false; terminal.copy_on_release = false; return;
                                }
                                if let Some((start, height)) = terminal.resize_drag {
                                    let maximum = (f32::from(window.viewport_size().height) - 100.).max(100.);
                                    terminal.height = (height + f32::from(start - event.position.y)).clamp(100., maximum);
                                } else if terminal.scrollbar_drag { terminal.scroll_to(event.position); }
                                else if terminal.selecting {
                                    if let Some(bounds) = terminal.bounds {
                                        if event.position.y < bounds.top() { terminal.scroll_selection(1); }
                                        else if event.position.y > bounds.bottom() { terminal.scroll_selection(-1); }
                                    }
                                    if let (Some(cell), Some((anchor, _))) = (terminal.cell_at(event.position), terminal.selection) { terminal.selection = Some((anchor, cell)); }
                                } else { return; }
                                cx.notify(); cx.stop_propagation();
                            });
                        }
                    });
                    let owner = paint_entity.downgrade();
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                        if phase == gpui::DispatchPhase::Capture && event.button == MouseButton::Left {
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |view, cx| {
                                    let terminal = &mut view.ssh_terminal;
                                    let copy = terminal.selecting || terminal.copy_on_release;
                                    if terminal.selecting
                                        && let (Some(cell), Some((anchor, _))) = (terminal.cell_at(event.position), terminal.selection) {
                                        terminal.selection = Some((anchor, cell));
                                    }
                                    terminal.resize_drag = None;
                                    terminal.scrollbar_drag = false;
                                    terminal.selecting = false;
                                    terminal.copy_on_release = false;
                                    if copy { terminal.copy(cx); cx.notify(); }
                                });
                            }
                        }
                    });
                }).size_full()))
            .into_any_element())
    }
}

fn encode_key(key: &gpui::Keystroke, application_cursor: bool) -> Option<Vec<u8>> {
    let modifiers = key.modifiers;
    if modifiers.platform { return Some(Vec::new()); }
    let parameter = 1 + u8::from(modifiers.shift) + 2 * u8::from(modifiers.alt) + 4 * u8::from(modifiers.control);
    let cursor = match key.key.as_str() { "up" => Some('A'), "down" => Some('B'), "right" => Some('C'), "left" => Some('D'), "home" => Some('H'), "end" => Some('F'), _ => None };
    if let Some(final_byte) = cursor {
        return Some(if parameter > 1 { format!("\x1b[1;{parameter}{final_byte}") }
            else if application_cursor { format!("\x1bO{final_byte}") } else { format!("\x1b[{final_byte}") }.into_bytes());
    }
    let number = match key.key.as_str() {
        "insert" => Some(2), "delete" => Some(3), "pageup" => Some(5), "pagedown" => Some(6),
        "f5" => Some(15), "f6" => Some(17), "f7" => Some(18), "f8" => Some(19), "f9" => Some(20), "f10" => Some(21), "f11" => Some(23), "f12" => Some(24),
        "f13" => Some(25), "f14" => Some(26), "f15" => Some(28), "f16" => Some(29), "f17" => Some(31), "f18" => Some(32), "f19" => Some(33), "f20" => Some(34),
        _ => None,
    };
    if let Some(number) = number {
        return Some(if parameter > 1 { format!("\x1b[{number};{parameter}~") } else { format!("\x1b[{number}~") }.into_bytes());
    }
    let function = match key.key.as_str() { "f1" => Some('P'), "f2" => Some('Q'), "f3" => Some('R'), "f4" => Some('S'), _ => None };
    if let Some(final_byte) = function {
        return Some(if parameter > 1 { format!("\x1b[1;{parameter}{final_byte}") } else { format!("\x1bO{final_byte}") }.into_bytes());
    }
    let mut bytes = match key.key.as_str() {
        "enter" | "return" => vec![b'\r'],
        "backspace" => vec![if modifiers.control { 8 } else { 127 }],
        "escape" => vec![27],
        "tab" if modifiers.shift => return Some(b"\x1b[Z".to_vec()),
        "tab" => vec![b'\t'],
        "space" if modifiers.control => vec![0],
        _ if modifiers.control => {
            let character = key.key.chars().next()?;
            if key.key.chars().count() != 1 { return None; }
            let byte = match character {
                'a'..='z' => character as u8 - b'a' + 1,
                'A'..='Z' => character as u8 - b'A' + 1,
                '@' | '2' | '`' => 0, '[' | '3' => 27, '\\' | '4' => 28, ']' | '5' => 29,
                '^' | '6' => 30, '_' | '7' | '/' => 31, '?' | '8' => 127,
                _ => return Some(Vec::new()),
            };
            vec![byte]
        }
        _ if modifiers.alt => key.key_char.as_deref().unwrap_or(&key.key).as_bytes().to_vec(),
        _ => return None,
    };
    if modifiers.alt { bytes.insert(0, 27); }
    Some(bytes)
}

fn terminal_color(color: vt100::Color, default: u32, bold: bool) -> u32 {
    const ANSI: [u32; 16] = [0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff];
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(red, green, blue) => u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue),
        vt100::Color::Idx(index) if index < 16 => ANSI[usize::from(if bold && index < 8 { index + 8 } else { index })],
        vt100::Color::Idx(index) if index < 232 => {
            let index = u32::from(index - 16);
            let component = |value| if value == 0 { 0 } else { 55 + value * 40 };
            component(index / 36) << 16 | component(index / 6 % 6) << 8 | component(index % 6)
        }
        vt100::Color::Idx(index) => { let gray = 8 + u32::from(index - 232) * 10; gray << 16 | gray << 8 | gray }
    }
}

fn background_color(color: u32, transparency: u8) -> gpui::Hsla {
    let mut color: gpui::Hsla = rgb(color).into();
    color.a = 1. - f32::from(transparency) / 100.;
    color
}

fn paint_screen(screen: &VisibleGrid, selection: Option<(CellPoint, CellPoint)>, cursor_visible: bool, width: f32, font_size: f32, line_height: f32, viewport: Bounds<gpui::Pixels>, default_background: u32, transparency: u8, composing: bool, window: &mut Window, cx: &mut App) {
    let origin = viewport.origin;
    let grid_width = px(width * f32::from(screen.columns)).min(viewport.size.width);
    let grid_height = px(line_height * f32::from(screen.rows)).min(viewport.size.height);
    let color = background_color(default_background, transparency);
    window.paint_quad(fill(Bounds::new(point(origin.x + grid_width, origin.y), size(viewport.size.width - grid_width, viewport.size.height)), color));
    window.paint_quad(fill(Bounds::new(point(origin.x, origin.y + grid_height), size(grid_width, viewport.size.height - grid_height)), color));
    let selected = selection.map(|(anchor, head)| if anchor <= head { (anchor, head) } else { (head, anchor) });
    for (cell_point, cell) in &screen.cells {
        let row = cell_point.row.saturating_sub(screen.first_row);
        let column = cell_point.column;
        let position = point(origin.x + px(width * f32::from(column)), origin.y + px(line_height * row as f32));
        let cell_width = if cell.is_wide() { width * 2. } else { width };
        let bounds = Bounds::new(position, size(px(cell_width), px(line_height)));
        let mut foreground = terminal_color(cell.fgcolor(), FOREGROUND, cell.bold());
        let mut background = terminal_color(cell.bgcolor(), default_background, false);
        if cell.inverse() { std::mem::swap(&mut foreground, &mut background); }
        let marked = composing && usize::from(screen.cursor.0) == row && column >= screen.cursor.1;
        if marked { background = default_background; }
        let cell_end = CellPoint { row: cell_point.row, column: column.saturating_add(if cell.is_wide() { 2 } else { 1 }) };
        let selected = selected.is_some_and(|(start, end)| start < end && *cell_point < end && start < cell_end);
        if selected { background = 0x264f78; foreground = 0xffffff; }
        let cursor = cursor_visible && !screen.hide_cursor && screen.scrollback == 0
            && usize::from(screen.cursor.0) == row && screen.cursor.1 == column;
        let block = cursor && matches!(screen.cursor_style, vt100::CursorStyle::Default | vt100::CursorStyle::BlinkBlock | vt100::CursorStyle::SteadyBlock);
        if block { std::mem::swap(&mut foreground, &mut background); }
        window.paint_quad(fill(bounds, background_color(background, if selected || block { 0 } else { transparency })));
        if cell.dim() { foreground = (foreground & 0xfefefe) >> 1; }
        if cell.has_contents() && !marked { paint_text(cell.contents(), position, rgb(foreground).into(), cell.bold(), cell.italic(), cell.underline(), font_size, line_height, window, cx); }
        if cursor && !block {
            let bounds = match screen.cursor_style {
                vt100::CursorStyle::BlinkUnderline | vt100::CursorStyle::SteadyUnderline => Bounds::new(point(position.x, position.y + px(line_height - 2.)), size(px(cell_width), px(2.))),
                _ => Bounds::new(position, size(px(2.), px(line_height))),
            };
            window.paint_quad(fill(bounds, rgb(FOREGROUND)));
        }
    }
}

fn paint_text(text: &str, origin: Point<gpui::Pixels>, color: gpui::Hsla, bold: bool, italic: bool, underline: bool, font_size: f32, line_height: f32, window: &mut Window, cx: &mut App) {
    let mut font = gpui::font("Menlo");
    if bold { font.weight = gpui::FontWeight::BOLD; }
    if italic { font.style = gpui::FontStyle::Italic; }
    let run = TextRun { len: text.len(), font, color, background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(text.to_owned().into(), px(font_size), &[run], None);
    if let Err(error) = line.paint(origin, px(line_height), gpui::TextAlign::Left, None, window, cx) { eprintln!("SSH terminal paint failed: {error}"); }
    if underline { window.paint_quad(fill(Bounds::new(point(origin.x, origin.y + px(line_height - 2.)), size(line.width(), px(1.))), color)); }
}

struct TerminalInput {
    owner: WeakEntity<MatrixWindow>,
    focus: FocusHandle,
    marked: Option<String>,
    selected: Range<usize>,
    caret: Option<Bounds<gpui::Pixels>>,
}

impl TerminalInput {
    fn changed(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.notify_text_input_changed(&self.focus);
        if let Some(owner) = self.owner.upgrade() { owner.update(cx, |_, cx| cx.notify()); }
        cx.notify();
    }

    fn commit(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        self.selected = 0..0;
        if let Some(owner) = self.owner.upgrade() {
            owner.update(cx, |view, cx| {
                if view.ssh_terminal.visible && view.ssh_terminal.focus.is_focused(window) {
                    view.ssh_terminal.parser.screen_mut().set_scrollback(0);
                    view.ssh_terminal.selection = None;
                    view.ssh_terminal.send(text.as_bytes().to_vec());
                    view.ssh_terminal.blink = true;
                    cx.notify();
                }
            });
        }
        self.changed(window, cx);
    }
}

impl EntityInputHandler for TerminalInput {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && !window.modifiers().control && !window.modifiers().alt && !window.modifiers().platform
    }

    fn text_for_range(&mut self, range: Range<usize>, actual: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let text = self.marked.as_deref().unwrap_or("");
        let start = utf16_byte(text, range.start)?;
        let end = utf16_byte(text, range.end)?;
        *actual = Some(range);
        text.get(start..end).map(str::to_owned)
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.selected.clone(), reversed: false })
    }

    fn set_selected_text_range(&mut self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let length = self.marked.as_deref().unwrap_or("").encode_utf16().count();
        self.selected = range.start.min(length)..range.end.min(length);
        self.changed(window, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.marked.as_deref().unwrap_or("").encode_utf16().count())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.marked.take() { self.commit(&text, window, cx); }
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(text, window, cx);
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        let mut marked = self.marked.take().unwrap_or_default();
        let range = range.unwrap_or(0..marked.encode_utf16().count());
        if let (Some(start), Some(end)) = (utf16_byte(&marked, range.start), utf16_byte(&marked, range.end)) {
            if start <= end { marked.replace_range(start..end, text); }
        }
        let length = marked.encode_utf16().count();
        let selected = selected.unwrap_or(length..length);
        self.selected = selected.start.min(length)..selected.end.min(length);
        self.marked = Some(marked);
        self.changed(window, cx);
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<gpui::Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<gpui::Pixels>> { self.caret }

    fn selection_bounds_for_range(&mut self, _: Range<usize>, _: Bounds<gpui::Pixels>, _: &mut Window, _: &mut Context<Self>) -> Vec<Bounds<gpui::Pixels>> { self.caret.into_iter().collect() }

    fn character_index_for_point(&mut self, _: Point<gpui::Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> { Some(self.selected.end) }
}

fn utf16_byte(text: &str, offset: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units == offset { return Some(byte); }
        units += character.len_utf16();
        if units > offset { return None; }
    }
    (units == offset).then_some(text.len())
}
