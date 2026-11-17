use super::*;
use super::dialog_font::px;
use crate::shortcuts::{self, BindingSet, CommandKey, KeyCombo, Shortcut};
use std::path::Path;
use std::ptr::NonNull;
use std::cell::Cell;
use block2::RcBlock;
use objc2::{class, msg_send};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSView, NSWindow};
use crate::editor::{EditError, SelectionSet};
use std::ops::Range;

struct KeypadMonitor {
    token: Retained<AnyObject>,
    _handler: RcBlock<dyn Fn(NonNull<AnyObject>) -> *mut AnyObject>,
    _main_thread: MainThreadMarker,
}

impl KeypadMonitor {
    fn install(owner: gpui::WeakEntity<MatrixWindow>, document: gpui::AnyWindowHandle, app: Rc<RefCell<gpui::AsyncApp>>, marker: std::rc::Weak<()>) -> io::Result<Self> {
        let main_thread = MainThreadMarker::new().ok_or_else(|| io::Error::other("Keypad shortcuts require the AppKit main thread"))?;
        let handler = RcBlock::new(move |event: NonNull<AnyObject>| -> *mut AnyObject {
            let original = event.as_ptr();
            if marker.upgrade().is_none() { return original; }
            let key_code: u16 = unsafe { msg_send![event.as_ref(), keyCode] };
            let flags: usize = unsafe { msg_send![event.as_ref(), modifierFlags] };
            let Some(combo) = appkit_keypad_combo(key_code, flags) else { return original; };
            let event_window: *mut AnyObject = unsafe { msg_send![event.as_ref(), window] };
            if event_window.is_null() { return original; }
            let accepted = Cell::new(false);
            let result = (|| {
                let mut app = app.try_borrow_mut().map_err(|error| io::Error::other(format!("Keypad event context is already in use: {error}")))?;
                dispatch_appkit_keypad(&owner, document, event_window, combo, &mut app, &accepted)
            })();
            match result {
                Ok(true) => std::ptr::null_mut(),
                Ok(false) => original,
                Err(error) => { eprintln!("Keypad shortcut window update failed: {error}"); if accepted.get() { std::ptr::null_mut() } else { original } }
            }
        });
        let token: Option<Retained<AnyObject>> = unsafe { msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: 1usize << 10, handler: &*handler] };
        let token = token.ok_or_else(|| io::Error::other("AppKit could not register the local keypad event monitor"))?;
        Ok(Self { token, _handler: handler, _main_thread: main_thread })
    }
}

impl Drop for KeypadMonitor {
    fn drop(&mut self) {
        unsafe { let _: () = msg_send![class!(NSEvent), removeMonitor: &*self.token]; }
    }
}

fn appkit_keypad_combo(key_code: u16, flags: usize) -> Option<KeyCombo> {
    if flags & (1 << 23) != 0 { return None; }
    let key = shortcut_layout::legacy_key(key_code)?;
    Some(KeyCombo { ctrl: flags & (1 << 18) != 0, alt: flags & (1 << 19) != 0, shift: flags & (1 << 17) != 0, cmd: flags & (1 << 20) != 0, key })
}

fn appkit_window_matches(window: &Window, event_window: *mut AnyObject) -> io::Result<bool> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| io::Error::other(error.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return Err(io::Error::other("Keypad shortcuts require an AppKit window")); };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let host = view.window().ok_or_else(|| io::Error::other("Keypad shortcut window has no AppKit host"))?;
    Ok(std::ptr::eq((&*host as *const NSWindow).cast::<AnyObject>(), event_window.cast_const()))
}

fn dispatch_appkit_keypad(owner: &gpui::WeakEntity<MatrixWindow>, document: gpui::AnyWindowHandle, event_window: *mut AnyObject, combo: KeyCombo, app: &mut gpui::AsyncApp, accepted: &Cell<bool>) -> io::Result<bool> {
    let target = document.update(app, |_, window, cx| -> io::Result<Option<gpui::AnyWindowHandle>> {
        if appkit_window_matches(window, event_window)? { return Ok(Some(document)); }
        let floating = owner.update(cx, |view, _| view.floating_windows.values().map(|handle| gpui::AnyWindowHandle::from(*handle)).collect::<Vec<_>>()).map_err(|error| io::Error::other(error.to_string()))?;
        for handle in floating {
            let matched = handle.update(cx, |_, window, _| appkit_window_matches(window, event_window)).map_err(|error| io::Error::other(error.to_string()))??;
            if matched { return Ok(Some(handle)); }
        }
        Ok(None)
    }).map_err(|error| io::Error::other(error.to_string()))??;
    let Some(target) = target else { return Ok(false); };
    target.update(app, |_, window, cx| -> io::Result<bool> {
        if !appkit_window_matches(window, event_window)? { return Ok(false); }
        let result = owner.update(cx, |view, cx| view.apply_keypad_shortcut(combo, window, cx)).map_err(|error| io::Error::other(error.to_string()));
        if result.as_ref().is_ok_and(|consumed| *consumed) { accepted.set(true); }
        result
    }).map_err(|error| io::Error::other(error.to_string()))?
}

pub(super) fn dimensions() -> gpui::Size<gpui::Pixels> { size(px(787.5), px(617.75)) }

const GRID_ROW_HEIGHT: f32 = 24.;
const GRID_HEADER_HEIGHT: f32 = 26.;
const GRID_SCROLLBAR: f32 = 17.;

fn mapper_string(locale: &str, name: &str, fallback: &'static str) -> &'static str {
    crate::localization::dialog_name(locale, &format!("ShortcutMapper/{name}")).unwrap_or(fallback)
}

fn mapper_page(locale: &str, page: usize) -> &'static str {
    let names = ["MainMenuTab", "MacrosTab", "RunCommandsTab", "PluginCommandsTab", "ScintillaCommandsTab"];
    mapper_string(locale, names[page], shortcuts::PAGES[page])
}

fn mapper_category(locale: &str, item: &Shortcut) -> String {
    let CommandKey::Menu { id, .. } = item.key else { return item.category.clone(); };
    let (root, fallback) = match id {
        11001..=11009 => ("Window", "Window"),
        10001..=10006 | 42026 | 42027 | 50003 | 50004 => ("view", "View"),
        45001..=45003 | 50000..=50002 | 50006 | 50010 | 50011 => ("edit", "Edit"),
        46001 => ("settings", "Settings"),
        42018 | 42019 | 42021 | 42025 | 42032 | 50005 => ("macro", "Macro"),
        0..=41999 => ("file", "File"),
        42000..=42999 => ("edit", "Edit"),
        43000..=43999 => ("search", "Search"),
        44000..=44999 => ("view", "View"),
        45000..=45999 => ("encoding", "Encoding"),
        46000..=46999 => ("language", "Language"),
        47000..=47999 => return mapper_string(locale, "AboutCategory", "About").to_owned(),
        48000..=48499 => ("settings", "Settings"),
        48500..=48999 => ("tools", "Tools"),
        _ => ("run", "Run"),
    };
    crate::localization::menu_root(locale, root).unwrap_or(fallback).to_owned()
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = notepad_matrix, no_json)]
pub(super) struct ShortcutAction {
    key: CommandKey,
}

struct Edit {
    key: CommandKey,
    bindings: Vec<KeyCombo>,
    selected: usize,
    combo: KeyCombo,
    choose_key: bool,
    warning_visible: bool,
}

pub(super) struct State {
    pub(super) visible: bool,
    pub(super) bindings: BindingSet,
    pub(super) page: usize,
    selected: [Option<CommandKey>; 5],
    scroll: [gpui::ScrollHandle; 5],
    horizontal_scroll: [gpui::ScrollHandle; 5],
    info_scroll: gpui::ScrollHandle,
    scroll_grab: [Cell<Option<f32>>; 3],
    scroll_bounds: [Rc<Cell<gpui::Bounds<gpui::Pixels>>>; 3],
    focus: FocusHandle,
    grid_focus: FocusHandle,
    key_focus: FocusHandle,
    key_list_focus: FocusHandle,
    filter: gpui::Entity<text_field::TextField>,
    name: gpui::Entity<text_field::TextField>,
    filter_changed: Option<gpui::Subscription>,
    name_changed: Option<gpui::Subscription>,
    edit: Option<Edit>,
    confirm: Option<CommandKey>,
    context_menu: Option<gpui::Point<gpui::Pixels>>,
    context_selected: usize,
    message: String,
    defaults: Vec<(Option<CommandKey>, KeyBinding)>,
    extra: Vec<(CommandKey, KeyBinding)>,
    native_monitor: Option<KeypadMonitor>,
    owner: gpui::WeakEntity<MatrixWindow>,
    owner_marker: Rc<()>,
    app: Rc<RefCell<gpui::AsyncApp>>,
    scintilla_target: [Range<i128>; 2],
    scintilla_target_document: [Option<std::sync::Weak<()>>; 2],
    scintilla_macro_recorder: Option<fn(&mut MatrixWindow, u32)>,
    scintilla_geometry_refresher: Option<fn(&mut MatrixWindow, &Window)>,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            visible: false,
            bindings: BindingSet::default(),
            page: 0,
            selected: std::array::from_fn(|_| None),
            scroll: std::array::from_fn(|_| gpui::ScrollHandle::new()),
            horizontal_scroll: std::array::from_fn(|_| gpui::ScrollHandle::new()),
            info_scroll: gpui::ScrollHandle::new(),
            scroll_grab: std::array::from_fn(|_| Cell::new(None)),
            scroll_bounds: std::array::from_fn(|_| Rc::new(Cell::new(gpui::Bounds::default()))),
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            grid_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            key_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            key_list_focus: cx.focus_handle(),
            filter: cx.new(|cx| text_field::TextField::compact("Filter", "shortcut-filter", cx).without_history_arrow().with_compact_height(21.)),
            name: cx.new(|cx| text_field::TextField::compact("Name", "shortcut-name", cx).without_history_arrow().with_compact_height(18.)),
            filter_changed: None,
            name_changed: None,
            edit: None,
            confirm: None,
            context_menu: None,
            context_selected: 0,
            message: String::new(),
            defaults: Vec::new(),
            extra: Vec::new(),
            native_monitor: None,
            owner: cx.entity().downgrade(),
            owner_marker: Rc::new(()),
            app: Rc::new(RefCell::new(cx.to_async())),
            scintilla_target: [0..0, 0..0],
            scintilla_target_document: [None, None],
            scintilla_macro_recorder: None,
            scintilla_geometry_refresher: None,
        }
    }

    fn selected(&self) -> Option<&Shortcut> {
        let key = self.selected[self.page].as_ref()?;
        self.bindings.shortcuts.iter().find(|item| &item.key == key)
    }
}

impl MatrixWindow {
    pub(super) fn shortcut_mapper_title(&self) -> &'static str {
        crate::localization::dialog_title(&self.native_locale(), "ShortcutMapper").unwrap_or("Shortcut mapper")
    }

    pub(super) fn set_shortcut_runtime_defaults(&mut self, defaults: Vec<(Option<CommandKey>, KeyBinding)>, extra: Vec<(CommandKey, KeyBinding)>, cx: &mut Context<Self>) -> io::Result<()> {
        let mut defaults = defaults;
        let predicate = Some(Rc::new(gpui::KeyBindingContextPredicate::parse("MatrixEditor").map_err(|error| io::Error::other(error.to_string()))?));
        for item in shortcuts::menu_catalog(self.menu_bar.definitions()) {
            if defaults.iter().any(|(key, _)| key.as_ref() == Some(&item.key)) { continue; }
            for combo in item.bindings {
                if combo.needs_native_adapter() { continue; }
                if let Some(source) = combo.keystroke() {
                    let binding = KeyBinding::load(&source, Box::new(ShortcutAction { key: item.key.clone() }), predicate.clone(), false, None, cx)
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    defaults.push((Some(item.key.clone()), binding));
                }
            }
        }
        for (_, id, menu_id, _, keys) in shortcuts::SCINTILLA_COMMANDS {
            let key = CommandKey::Scintilla { id: *id, menu_id: *menu_id };
            for source in *keys {
                if KeyCombo::parse_source(source).is_some_and(KeyCombo::needs_native_adapter) { continue; }
                if defaults.iter().any(|(candidate, binding)| candidate.as_ref() == Some(&key) && binding.keystrokes().len() == 1 && binding.keystrokes()[0].unparse(gpui::OperatingSystem::Mac) == *source) { continue; }
                let binding = KeyBinding::new(source, ShortcutAction { key: key.clone() }, Some("MatrixEditor"));
                defaults.push((Some(key.clone()), binding));
            }
        }
        if self.shortcut_mapper.native_monitor.is_none() {
            self.shortcut_mapper.native_monitor = Some(KeypadMonitor::install(self.shortcut_mapper.owner.clone(), self.document_window, self.shortcut_mapper.app.clone(), Rc::downgrade(&self.shortcut_mapper.owner_marker))?);
        }
        let mut templates = extra.clone();
        templates.extend(shortcut_templates(&self.shortcut_mapper.bindings));
        let bindings = self.shortcut_mapper.bindings.native_bindings(defaults.clone(), templates, cx)?;
        self.shortcut_mapper.defaults = defaults;
        self.shortcut_mapper.extra = extra;
        cx.clear_key_bindings();
        cx.bind_keys(bindings);
        Ok(())
    }

    pub(super) fn refresh_shortcuts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcut_mapper.native_monitor.is_none() {
            match KeypadMonitor::install(self.shortcut_mapper.owner.clone(), self.document_window, self.shortcut_mapper.app.clone(), Rc::downgrade(&self.shortcut_mapper.owner_marker)) {
                Ok(monitor) => self.shortcut_mapper.native_monitor = Some(monitor),
                Err(error) => { self.shortcut_mapper.message = error.to_string(); cx.notify(); return; }
            }
        }
        let Some(store) = self.settings_store.as_ref() else {
            self.shortcut_mapper.message = "The private configuration store is unavailable".into();
            cx.notify();
            return;
        };
        let library = match crate::macros::MacroLibrary::load(store) {
            Ok(library) => library,
            Err(error) => { self.shortcut_mapper.message = error.to_string(); cx.notify(); return; }
        };
        let names = library.macros.iter().map(|saved| saved.name.clone()).collect::<Vec<_>>();
        self.menu_bar.set_saved_macros(names.clone());
        let mut catalog = shortcuts::menu_catalog(self.menu_bar.definitions());
        catalog.extend(shortcuts::scintilla_catalog());
        if !self.shortcut_mapper.defaults.is_empty() {
            for item in &mut catalog {
                let bindings = self.shortcut_mapper.defaults.iter().filter(|(key, _)| key.as_ref() == Some(&item.key)).filter_map(|(_, binding)| {
                    if binding.keystrokes().len() != 1 { return None; }
                    let source = binding.keystrokes()[0].unparse(gpui::OperatingSystem::Mac);
                    KeyCombo::parse_source(&source)
                }).collect::<Vec<_>>();
                if !bindings.is_empty() {
                    let physical = item.bindings.iter().copied().filter(|combo| combo.needs_native_adapter()).collect::<Vec<_>>();
                    item.bindings = bindings;
                    item.bindings.extend(physical);
                }
                let mut used = std::collections::BTreeSet::new();
                item.bindings.retain(|combo| used.insert(*combo));
            }
        }
        match BindingSet::load(store, catalog, &names) {
            Ok(bindings) => {
                let native = if self.shortcut_mapper.defaults.is_empty() { Ok(None) } else {
                    let mut templates = self.shortcut_mapper.extra.clone();
                    templates.extend(shortcut_templates(&bindings));
                    bindings.native_bindings(self.shortcut_mapper.defaults.clone(), templates, cx).map(Some)
                };
                match native {
                    Ok(native) => {
                        if let Some(native) = native { cx.clear_key_bindings(); cx.bind_keys(native); }
                        self.shortcut_mapper.bindings = bindings;
                        self.macro_dialogs.library = library;
                        self.shortcut_mapper.message.clear();
                    }
                    Err(error) => { self.shortcut_mapper.bindings = bindings; self.macro_dialogs.library = library; self.shortcut_mapper.message = format!("Shortcuts were not applied: {error}"); }
                }
            }
            Err(error) => self.shortcut_mapper.message = format!("Could not load shortcuts.xml: {error}"),
        }
        for page in 0..5 {
            if !self.shortcut_mapper.bindings.shortcuts.iter().any(|item| item.key.page() == page && self.shortcut_mapper.selected[page].as_ref() == Some(&item.key)) {
                self.shortcut_mapper.selected[page] = self.shortcut_mapper.bindings.shortcuts.iter().find(|item| item.key.page() == page).map(|item| item.key.clone());
            }
        }
        self.reconcile_shortcut_selection(cx);
        cx.notify();
    }

    pub(super) fn show_shortcut_mapper(&mut self, page: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.macro_playing { return; }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.refresh_shortcuts(window, cx);
        self.shortcut_mapper.page = page.min(4);
        self.shortcut_mapper.visible = true;
        self.shortcut_mapper.edit = None;
        self.shortcut_mapper.confirm = None;
        let locale = self.native_locale();
        let filter_label = crate::localization::dialog_control(&locale, "ShortcutMapper", 2607).unwrap_or("Filter:");
        let name_label = crate::localization::dialog_control(&locale, "ShortcutMapperSubDialg", 5006).unwrap_or("Name:");
        self.shortcut_mapper.filter.update(cx, |field, cx| field.set_label(filter_label, cx));
        self.shortcut_mapper.name.update(cx, |field, cx| field.set_label(name_label, cx));
        if self.shortcut_mapper.filter_changed.is_none() {
            let field = self.shortcut_mapper.filter.clone();
            self.shortcut_mapper.filter_changed = Some(cx.observe_in(&field, window, |view, _, _, cx| { view.reconcile_shortcut_selection(cx); view.shortcut_mapper.message.clear(); cx.notify(); }));
        }
        if self.shortcut_mapper.name_changed.is_none() {
            let field = self.shortcut_mapper.name.clone();
            self.shortcut_mapper.name_changed = Some(cx.observe_in(&field, window, |_, _, _, cx| cx.notify()));
        }
        self.present_floating(floating_windows::DialogKind::ShortcutMapper, window, cx);
        self.shortcut_mapper.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_shortcut_mapper(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcut_mapper.edit = None;
        self.shortcut_mapper.confirm = None;
        self.shortcut_mapper.context_menu = None;
        self.shortcut_mapper.visible = false;
        for grab in &self.shortcut_mapper.scroll_grab { grab.set(None); }
        self.dismiss_floating(floating_windows::DialogKind::ShortcutMapper, window, cx);
    }

    pub(super) fn annotate_shortcut_bindings(&self, defaults: Vec<KeyBinding>) -> Vec<(Option<CommandKey>, KeyBinding)> {
        let catalog = shortcuts::menu_catalog(self.menu_bar.definitions());
        defaults.into_iter().map(|binding| {
            let key = key_for_binding(&binding).or_else(|| {
                if binding.keystrokes().len() != 1 || binding.predicate().is_some_and(|predicate| !format!("{predicate:?}").contains("MatrixEditor")) { return None; }
                let stroke = binding.keystrokes()[0].inner();
                let mut matching = catalog.iter().filter(|item| item.bindings.iter().any(|combo| combo.keystroke().and_then(|source| gpui::Keystroke::parse_with_os(&source, gpui::OperatingSystem::Mac).ok()).is_some_and(|key| key.key == stroke.key && key.modifiers == stroke.modifiers)));
                let key = matching.next()?.key.clone();
                if matching.next().is_some() { return None; }
                Some(key)
            });
            (key, binding)
        }).collect()
    }

    pub(super) fn shortcut_label(&self, command: MenuCommand) -> Option<String> {
        let items = self.shortcut_mapper.bindings.shortcuts.iter().filter(|item| item.command == Some(command)).collect::<Vec<_>>();
        if items.iter().any(|item| self.shortcut_mapper.bindings.override_for(&item.key).is_some()) { items.last().map(|item| item.label()) }
        else if let Some(id) = match command { MenuCommand::View(ViewCommand::ZoomIn) => Some(44023), MenuCommand::View(ViewCommand::ZoomOut) => Some(44024), MenuCommand::View(ViewCommand::ZoomReset) => Some(44033), _ => command.source_id() } {
            if let Some(item) = self.shortcut_mapper.bindings.shortcuts.iter().find(|item| matches!(item.key, CommandKey::Scintilla { menu_id, .. } if menu_id != 0 && menu_id == id)) { Some(item.label()) }
            else { self.shortcut_mapper.bindings.override_for_command(id).map(|keys| keys.iter().map(|key| key.label()).collect::<Vec<_>>().join(" or ")) }
        }
        else { None }
    }

    pub(super) fn apply_shortcut_action(&mut self, action: &ShortcutAction, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_shortcut_action(action, window, cx);
    }

    fn dispatch_shortcut_action(&mut self, action: &ShortcutAction, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.macro_playing || self.shortcut_mapper.visible { return false; }
        let Some(item) = self.shortcut_mapper.bindings.shortcuts.iter().find(|item| item.key == action.key).cloned() else { return false; };
        if let CommandKey::Scintilla { id, .. } = item.key {
            if supplemental_scintilla(id) { self.execute_scintilla_shortcut(id, window, cx); return true; }
            if let Some(action) = scintilla_action(id) { window.dispatch_action(action, cx); return true; }
            return false;
        }
        if let CommandKey::Run(index) = item.key {
            if let Some(command) = self.shortcut_mapper.bindings.run_commands.get(index) {
                let command = command.command.clone();
                self.run_input.update(cx, |field, cx| field.set_text(&command, window, cx));
                self.run_visible = true;
                self.confirm_run(window, cx);
                if self.run_visible { self.present_floating(floating_windows::DialogKind::Run, window, cx); }
                return true;
            }
            return false;
        }
        if let CommandKey::Menu { id: id @ 50003..=50005, .. } = item.key {
            match id {
                50003 => window.dispatch_action(Box::new(MruPreviousBuffer), cx),
                50004 => window.dispatch_action(Box::new(MruNextBuffer), cx),
                _ => self.macro_command(if self.macro_recording { crate::macros::MacroCommand::StopRecording } else { crate::macros::MacroCommand::StartRecording }, window, cx),
            }
            return true;
        }
        let Some(command) = item.command else { return false; };
        if !menu_surface::availability(self, window, cx).allows(command) { return false; }
        fn locate(entries: &[crate::menu::MenuEntry], command: MenuCommand, path: &mut Vec<usize>) -> bool {
            for (index, entry) in entries.iter().enumerate() {
                path.push(index);
                if entry.command() == Some(command) { return true; }
                if let Some(entries) = entry.submenu_entries() {
                    if locate(entries, command, path) { return true; }
                }
                path.pop();
            }
            false
        }
        let target = self.menu_bar.definitions().iter().enumerate().find_map(|(root, definition)| {
            let mut path = Vec::new();
            locate(&definition.entries, command, &mut path).then_some((root, path))
        });
        if let Some((root, path)) = target {
            self.menu_input(MenuEvent::OpenRoot(root), window, cx);
            for depth in 1..path.len() {
                self.menu_input(MenuEvent::Hover { root, path: path[..depth].to_vec() }, window, cx);
            }
            self.menu_input(MenuEvent::Click { root, path }, window, cx);
            return true;
        }
        false
    }

    fn apply_keypad_shortcut(&mut self, combo: KeyCombo, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shortcut_mapper.visible {
            let in_list = self.shortcut_mapper.edit.as_ref().is_some_and(|edit| edit.choose_key) && self.shortcut_mapper.key_list_focus.contains_focused(window, cx);
            if !self.shortcut_mapper.key_focus.is_focused(window) && !in_list { return false; }
            if let Some(edit) = &mut self.shortcut_mapper.edit { edit.combo.key = combo.key; edit.choose_key = false; edit.warning_visible = true; self.shortcut_mapper.key_focus.focus(window, cx); cx.notify(); return true; }
            return false;
        }
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.macro_playing || self.editor().has_composition() { return false; }
        let Some(item) = self.shortcut_mapper.bindings.shortcuts.iter().filter(|item| item.bindings.contains(&combo)).min_by_key(|item| item.key.page()).cloned() else { return false; };
        if !self.editor_focus.is_focused(window) {
            let global = self.shortcut_mapper.defaults.iter().any(|(key, binding)| key.as_ref() == Some(&item.key) && binding.predicate().is_none());
            if item.key.page() == 4 || !global { return false; }
        }
        match &item.key {
            CommandKey::Scintilla { id, .. } if scintilla_action(*id).is_none() && !supplemental_scintilla(*id) => return false,
            CommandKey::Run(index) if self.shortcut_mapper.bindings.run_commands.get(*index).is_none() || self.batch_editing_blocked() => return false,
            _ => {},
        }
        if let Some(command) = item.command {
            if !menu_surface::availability(self, window, cx).allows(command) { return false; }
        } else if !matches!(item.key, CommandKey::Run(_) | CommandKey::Scintilla { .. } | CommandKey::Menu { id: 50003..=50005, .. }) { return false; }
        self.dispatch_shortcut_action(&ShortcutAction { key: item.key }, window, cx)
    }

    pub(super) fn activate_shortcut_command(&mut self, key: CommandKey, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_shortcut_action(&ShortcutAction { key }, window, cx);
    }

    pub(super) fn set_scintilla_shortcut_target(&mut self, range: Range<ByteOffset>) -> Result<(), EditError> {
        self.set_scintilla_shortcut_raw_target(
            i128::try_from(range.start.0).map_err(shortcut_storage_error)?
                ..i128::try_from(range.end.0).map_err(shortcut_storage_error)?,
        );
        Ok(())
    }

    pub(super) fn set_scintilla_shortcut_raw_target(&mut self, range: Range<i128>) {
        self.ensure_scintilla_target_attachment();
        self.shortcut_mapper.scintilla_target[self.pane as usize] = range;
    }

    pub(super) fn ensure_scintilla_target_attachment(&mut self) {
        let identity = self.editor().document().document_identity();
        let pane = self.pane as usize;
        if self.shortcut_mapper.scintilla_target_document[pane].as_ref().is_none_or(|old| !std::sync::Weak::ptr_eq(old, &identity)) {
            self.shortcut_mapper.scintilla_target[pane] = 0..0;
            self.shortcut_mapper.scintilla_target_document[pane] = Some(identity);
        }
    }

    pub(super) fn reset_scintilla_shortcut_target(&mut self) {
        self.set_scintilla_shortcut_raw_target(0..0);
    }

    pub(super) fn set_scintilla_macro_recorder(&mut self, recorder: fn(&mut Self, u32)) { self.shortcut_mapper.scintilla_macro_recorder = Some(recorder); }

    pub(super) fn set_scintilla_geometry_refresher(&mut self, refresh: fn(&mut Self, &Window)) { self.shortcut_mapper.scintilla_geometry_refresher = Some(refresh); }

    pub(super) fn execute_scintilla_shortcut(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        if self.route_to_document(window, cx, move |view, window, cx| view.execute_scintilla_shortcut(id, window, cx)) { return; }
        if self.file_busy || self.close_dialog || self.save_all_dialog || self.macro_interaction_blocked() { return; }
        if !supplemental_scintilla(id) { if let Some(action) = scintilla_action(id) { window.dispatch_action(action, cx); } return; }
        if let Err(error) = self.editor_mut().commit_composition() { self.editor_changed(Err(error), window, cx); return; }
        self.refresh_editor_syntax();
        if let Some(refresh) = self.shortcut_mapper.scintilla_geometry_refresher { refresh(self, window); }
        if self.macro_recording && !self.macro_playing && crate::macros::MacroStep::scintilla_recordable(id) {
            if let Some(recorder) = self.shortcut_mapper.scintilla_macro_recorder { recorder(self, id); }
            else { self.record_macro_step(crate::macros::MacroStep::Scintilla { message: id, wparam: 0, lparam: 0 }); }
        }
        match id {
            2337 | 2455 => { self.copy_scintilla_lines(id == 2337, window, cx); self.macro_step_pending = false; return; }
            2339 => { let recording = self.macro_recording; self.macro_recording = false; self.text_transform_command(42011, window, cx); self.macro_recording = recording; return; }
            2404 => { let recording = self.macro_recording; self.macro_recording = false; self.duplicate_editor(true, window, cx); self.macro_recording = recording; return; }
            2169 => { self.editor_follow_caret = true; self.editor_focus.focus(window, cx); cx.notify(); return; }
            _ => {},
        }
        let result = match id {
            2004 => {
                let length = self.editor().document().len_bytes();
                let result = if self.editor().is_read_only() { Ok(()) } else { self.editor_mut().apply_transforms(&[crate::editor::TextTransform { range: ByteOffset(0)..ByteOffset(length), text: String::new() }]) };
                result.and_then(|()| self.editor_mut().set_selections(SelectionSet::single(Selection::default())))
            }
            2330 => self.editor_mut().type_text("\x0c"),
            2288 => { self.macro_step_pending = true; self.join_scintilla_target() },
            2344 => self.editor_mut().backspace_not_line(),
            2413..=2416 => self.move_scintilla_paragraph(id, window, cx),
            2311 | 2439..=2441 => self.move_scintilla_word_boundary(id),
            2312..=2315 | 2331 | 2332 | 2346 | 2348 | 2349 | 2430 | 2450 | 2652 | 2653 => self.move_scintilla_line_boundary(id),
            2342 | 2343 | 2401 => self.scroll_scintilla_line(id, window, cx),
            2435..=2438 => self.move_scintilla_stuttered_page(id, window, cx),
            _ => return,
        };
        if id == 2004 {
            self.restore_editor_viewport(None);
            self.editor_top_line = 0;
        }
        if !matches!(id, 2342 | 2343 | 2401 | 2435..=2438) { self.editor_follow_caret = true; self.editor_preferred_x = None; }
        let geometry = if matches!(id, 2342 | 2343 | 2401 | 2435..=2438) { Some((self.editor_top_line, self.editor_preferred_x, self.editor_follow_caret)) } else { None };
        self.editor_changed(result, window, cx);
        if let Some((top, x, follow)) = geometry { self.restore_editor_viewport(None); self.editor_top_line = top; self.editor_preferred_x = x; self.editor_follow_caret = follow; }
        self.editor_focus.focus(window, cx);
    }

    fn join_scintilla_target(&mut self) -> Result<(), EditError> {
        self.ensure_scintilla_target_attachment();
        let target = self.shortcut_mapper.scintilla_target[self.pane as usize].clone();
        if self.editor().is_read_only() || target.start >= target.end { return Ok(()); }
        let mut bytes = self.editor().document().byte_snapshot();
        let start = usize::try_from(target.start.max(0)).map_err(shortcut_storage_error)?.min(bytes.len());
        let row = bytes.line_index(start).map_err(shortcut_storage_error)?;
        let mut end = target.end;
        let mut selections = self.editor().selections();
        let mut edits = crate::editor::LineEdits::default();
        loop {
            let mut at = bytes.line_start(row).map_err(shortcut_storage_error)?;
            while at < bytes.len() && !bytes.is_line_end(at) { at += 1; }
            if i128::try_from(at).map_err(shortcut_storage_error)? >= end { break; }
            let previous = at.checked_sub(1).and_then(|at| bytes.byte(at));
            let range = bytes.deletion_range(at, false).map_err(shortcut_storage_error)?;
            let width = if at >= bytes.len() { 1 } else { range.len() };
            end -= i128::try_from(width).map_err(shortcut_storage_error)?;
            selections.map_edit(at, range.len(), 0);
            edits.change_bytes(&mut bytes, range, &[])?;
            if previous != Some(b' ') { selections.map_edit(at, 0, 1); edits.change_bytes(&mut bytes, at..at, b" ")?; end += 1; }
        }
        self.editor_mut().commit_prepared_bytes(bytes, selections, edits)?;
        self.set_scintilla_shortcut_raw_target(target.start..end);
        Ok(())
    }

    fn shortcut_visual_row(&self, position: usize) -> usize {
        let mut first = 0;
        let mut end = self.editor_visual_row_count();
        while first < end {
            let middle = first + (end - first) / 2;
            if self.editor_visual_row_source_range(middle).is_some_and(|range| range.start <= position) { first = middle + 1; } else { end = middle; }
        }
        first.saturating_sub(1)
    }

    fn shortcut_visible_position(&self, position: ByteOffset, forward: bool) -> ByteOffset {
        let row = self.shortcut_visual_row(position.0);
        if let Some(range) = self.editor_visual_row_source_range(row) {
            if position.0 <= range.end { return position; }
            if forward { if let Some(next) = self.editor_visual_row_source_range(row + 1) { return ByteOffset(next.start); } }
            return ByteOffset(range.end);
        }
        position
    }

    fn move_scintilla_word_boundary(&mut self, id: u32) -> Result<(), EditError> {
        let forward = matches!(id, 2311 | 2441);
        let extend = matches!(id, 2311 | 2440);
        let boundary = if id == 2311 { crate::editor::WordBoundary::Start } else { crate::editor::WordBoundary::End };
        let selections = self.editor().display_selections();
        if selections.is_rectangular() { let limit = shortcut_rectangle_limit(&selections, forward)?; return self.editor_mut().set_display_selections(limit); }
        let moved = selections.moved(extend, |_, selected| {
            let head = self.editor().word_boundary(selected.selection.head, forward, boundary)?;
            let head = self.shortcut_visible_position(head, forward);
            Ok(crate::editor::SelectionRange { selection: Selection { anchor: if extend { selected.selection.anchor } else { head }, head }, anchor_virtual: if extend { selected.anchor_virtual } else { 0 }, head_virtual: 0 })
        })?;
        self.editor_mut().set_display_selections(moved)
    }

    fn move_scintilla_paragraph(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) -> Result<(), EditError> {
        let down = matches!(id, 2413 | 2414);
        let extend = matches!(id, 2414 | 2416);
        if self.editor().selections().is_rectangular() { let recording = self.macro_recording; self.macro_recording = false; self.move_vertical(down, extend, window, cx); self.macro_recording = recording; return Ok(()); }
        let bytes = self.editor().document().byte_snapshot();
        let selected = self.editor().selection();
        let mut line = bytes.line_index(selected.head.0).map_err(shortcut_storage_error)?;
        let white = |line| -> Result<bool, EditError> {
            let start = bytes.line_start(line).map_err(shortcut_storage_error)?;
            let end = if line + 1 < bytes.line_count() { bytes.line_start(line + 1).map_err(shortcut_storage_error)? } else { bytes.len() };
            Ok(bytes.slice(start..end).map_err(shortcut_storage_error)?.to_vec().iter().all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')))
        };
        let head = if down {
            while line < bytes.line_count() && !white(line)? { line += 1; }
            while line < bytes.line_count() && white(line)? { line += 1; }
            if line < bytes.line_count() { bytes.line_start(line).map_err(shortcut_storage_error)? } else { bytes.len() }
        } else {
            let mut cursor = if selected.head.0 == bytes.line_start(line).map_err(shortcut_storage_error)? { line.checked_sub(1) } else { Some(line) };
            while let Some(line) = cursor { if !white(line)? { break; } cursor = line.checked_sub(1); }
            while let Some(line) = cursor { if white(line)? { break; } cursor = line.checked_sub(1); }
            bytes.line_start(cursor.map_or(0, |line| line + 1)).map_err(shortcut_storage_error)?
        };
        let projection = self.editor().document().projection();
        let display = projection.display_position(ByteOffset(head), crate::byte_projection::Affinity::After)?;
        let visible = self.shortcut_visible_position(ByteOffset(display.0), down);
        let head = projection.source_position(crate::byte_projection::DisplayByteOffset(visible.0), crate::byte_projection::Affinity::After)?;
        self.editor_mut().select(Selection { anchor: if extend { selected.anchor } else { head }, head })
    }

    fn move_scintilla_line_boundary(&mut self, id: u32) -> Result<(), EditError> {
        let text = self.editor().display_snapshot();
        let mut selections = self.editor().display_selections();
        let extend = matches!(id, 2313 | 2315 | 2332 | 2346 | 2348 | 2450 | 2653);
        let end = matches!(id, 2314 | 2315 | 2348);
        if id == 2430 || (matches!(id, 2313 | 2332 | 2315) && selections.is_rectangular()) {
            let source = self.editor().selections();
            let primary = source.primary();
            let first = if source.is_rectangular() { if source.primary_index() == 0 { source.ranges().last().copied().unwrap_or(primary) } else { source.ranges()[0] } } else { primary };
            let bytes = self.editor().document().byte_snapshot();
            let tabs = self.editor().tab_preferences().size;
            let anchor = crate::editor::ColumnPosition { line: bytes.line_index(first.selection.anchor.0).map_err(shortcut_storage_error)?, column: self.editor().document().display_column(first.selection.anchor, tabs)? + first.anchor_virtual };
            let line = bytes.line_index(primary.selection.head.0).map_err(shortcut_storage_error)?;
            let start = bytes.line_start(line).map_err(shortcut_storage_error)?;
            let mut line_end = start;
            while line_end < bytes.len() && !bytes.is_line_end(line_end) { line_end += 1; }
            let mut indentation = start;
            while indentation < line_end && matches!(bytes.byte(indentation), Some(b' ' | b'\t')) { indentation += 1; }
            let byte = match id { 2315 => line_end, 2332 => if primary.selection.head.0 == indentation { start } else { indentation }, _ => start };
            let head = crate::editor::ColumnPosition { line, column: self.editor().document().display_column(ByteOffset(byte), tabs)? };
            return self.editor_mut().select_rectangle(anchor, head, tabs);
        }
        if selections.is_rectangular() {
            selections = shortcut_rectangle_limit(&selections, end)?;
            if !matches!(id, 2312 | 2314 | 2331) { return self.editor_mut().set_display_selections(selections); }
        }
        let moved = selections.moved(extend, |_, selected| {
            let position = selected.selection.head.0;
            let line = text.byte_to_line_idx(position, ropey::LineType::LF_CR);
            let start = text.line_to_byte_idx(line, ropey::LineType::LF_CR);
            let mut logical_end = if line + 1 < text.len_lines(ropey::LineType::LF_CR) { text.line_to_byte_idx(line + 1, ropey::LineType::LF_CR) } else { text.len() };
            while logical_end > start && matches!(text.byte(logical_end - 1), b'\r' | b'\n') { logical_end -= 1; }
            let mut indentation = start;
            while indentation < logical_end && matches!(text.byte(indentation), b' ' | b'\t') { indentation += 1; }
            let row = self.shortcut_visual_row(position);
            let visual = self.editor_visual_row_source_range(row).ok_or(EditError::InvalidRange)?;
            let head = match id {
                2312 | 2313 => start,
                2314 | 2315 => logical_end,
                2331 | 2332 => if position == indentation { start } else { indentation },
                2346 => visual.start,
                2348 => if visual.end == logical_end { logical_end } else { self.editor().adjacent_position(ByteOffset(visual.end), false)?.0 },
                2349 | 2450 => if position <= visual.start { start } else { visual.start },
                2652 | 2653 => { let home = if position == indentation { start } else { indentation }; visual.start.max(home) },
                _ => return Err(EditError::InvalidRange),
            };
            let head = self.shortcut_visible_position(ByteOffset(head), end);
            Ok(crate::editor::SelectionRange { selection: Selection { anchor: if extend { selected.selection.anchor } else { head }, head }, anchor_virtual: if extend { selected.anchor_virtual } else { 0 }, head_virtual: 0 })
        })?;
        self.editor_mut().set_display_selections(moved)
    }

    fn seek_scintilla_visual_row(&mut self, target: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) -> Result<(), EditError> {
        let target = target.min(self.editor_visual_row_count().saturating_sub(1));
        let original = self.editor().selections();
        let original_view = (self.editor_top_line, self.editor_preferred_x, self.editor_follow_caret);
        let recording = self.macro_recording;
        self.macro_recording = false;
        let result = (|| {
            let primary = self.editor().selections().primary();
            self.editor_mut().set_selections(SelectionSet::new(vec![primary], 0)?)?;
            let row = self.shortcut_visual_row(self.editor().display_selection().head.0);
            if row == target { return Ok(()); }
            if self.editor_preferred_x.is_none() {
                self.move_vertical(target > row, extend, window, cx);
                if self.shortcut_visual_row(self.editor().display_selection().head.0) == target {
                    if extend { let mut moved = self.editor().selections().primary(); moved.selection.anchor = primary.selection.anchor; moved.anchor_virtual = primary.anchor_virtual; self.editor_mut().set_selections(SelectionSet::new(vec![moved], 0)?)?; }
                    return Ok(());
                }
            }
            let x = self.editor_preferred_x.ok_or(EditError::InvalidRange)?;
            let (adjacent, down) = if target == 0 { (1, false) } else { (target - 1, true) };
            let start = self.editor_visual_row_source_range(adjacent).ok_or(EditError::InvalidRange)?.start;
            let display = self.editor().display_selection();
            self.editor_mut().select_display(Selection { anchor: if extend { display.anchor } else { ByteOffset(start) }, head: ByteOffset(start) })?;
            self.editor_preferred_x = Some(x);
            self.move_vertical(down, extend, window, cx);
            if self.shortcut_visual_row(self.editor().display_selection().head.0) != target { return Err(EditError::InvalidRange); }
            if extend {
                let mut moved = self.editor().selections().primary();
                moved.selection.anchor = primary.selection.anchor;
                moved.anchor_virtual = primary.anchor_virtual;
                self.editor_mut().set_selections(SelectionSet::new(vec![moved], 0)?)?;
            }
            Ok(())
        })();
        self.macro_recording = recording;
        if let Err(error) = result {
            let restored = self.editor_mut().set_selections(original);
            self.restore_editor_viewport(None);
            self.editor_top_line = original_view.0;
            self.editor_preferred_x = original_view.1;
            self.editor_follow_caret = original_view.2;
            return match restored { Ok(()) => Err(error), Err(restore) => Err(EditError::Storage(format!("{error}; selection restore failed: {restore}"))) };
        }
        result
    }

    fn scroll_scintilla_line(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) -> Result<(), EditError> {
        let capacity = self.editor_visible_capacity();
        let count = self.editor_visual_row_count();
        let maximum = if self.pref_bool("ScintillaPrimaryView.scrollBeyondLastLine", true) { count.saturating_sub(1) } else { count.saturating_sub(capacity) };
        let top = match id { 2342 => self.editor_top_line.saturating_add(1).min(maximum), 2343 => self.editor_top_line.saturating_sub(1), _ => self.editor_top_line.min(maximum) };
        let row = self.shortcut_visual_row(self.editor().display_selection().head.0);
        let target = row.clamp(top, top.saturating_add(capacity.saturating_sub(1)));
        if row != target { self.seek_scintilla_visual_row(target, false, window, cx)?; }
        self.restore_editor_viewport(None);
        if self.editor_top_line != top { self.editor_painted_revision = None; }
        self.editor_top_line = top;
        self.editor_follow_caret = false;
        self.editor_scroll_remainder = 0.;
        Ok(())
    }

    fn move_scintilla_stuttered_page(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) -> Result<(), EditError> {
        let down = matches!(id, 2437 | 2438);
        let extend = matches!(id, 2436 | 2438);
        let top = self.editor_top_line;
        let row = self.shortcut_visual_row(self.editor().display_selection().head.0);
        let bottom = top.saturating_add(self.editor_visible_capacity().saturating_sub(1)).min(self.editor_visual_row_count().saturating_sub(1));
        let current_line = self.editor_logical_line_for_visual_row(row);
        let first_edge = if down { current_line < self.editor_logical_line_for_visual_row(bottom).saturating_sub(1) } else { current_line > top };
        if first_edge {
            self.seek_scintilla_visual_row(if down { bottom } else { top }, extend, window, cx)?;
            self.restore_editor_viewport(None);
            if self.editor_top_line != top { self.editor_painted_revision = None; }
            self.editor_top_line = top;
            self.editor_follow_caret = false;
        } else { let recording = self.macro_recording; self.macro_recording = false; self.move_editor_page(down, extend, window, cx); self.macro_recording = recording; }
        Ok(())
    }

    fn begin_shortcut_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.shortcut_mapper.selected().cloned() else { return; };
        let locale = self.native_locale();
        if !item.matches_localized(&self.shortcut_mapper.filter.read(cx).text(), &locale) { return; }
        let name = if matches!(item.key, CommandKey::Macro(_) | CommandKey::Run(_)) { item.name.clone() } else { item.localized_name(&locale) };
        self.shortcut_mapper.name.update(cx, |field, cx| {
            field.set_text(&name, window, cx);
            field.set_enabled(true, cx);
            field.set_read_only(!matches!(item.key, CommandKey::Macro(_) | CommandKey::Run(_)), cx);
        });
        let combo = item.bindings.first().copied().unwrap_or_default();
        let bindings = if item.key.page() == 4 && item.bindings.is_empty() { vec![KeyCombo::default()] } else { item.bindings };
        self.shortcut_mapper.edit = Some(Edit { key: item.key, bindings, selected: 0, combo, choose_key: false, warning_visible: false });
        self.shortcut_mapper.context_menu = None;
        self.shortcut_mapper.confirm = None;
        self.shortcut_mapper.message.clear();
        self.shortcut_mapper.key_focus.focus(window, cx);
        cx.notify();
    }

    fn request_shortcut_clear(&mut self, delete: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.shortcut_mapper.selected() else { return; };
        if !item.matches_localized(&self.shortcut_mapper.filter.read(cx).text(), &self.native_locale()) { return; }
        if (delete && !matches!(item.key, CommandKey::Macro(_) | CommandKey::Run(_))) || (!delete && item.key.page() == 4) { return; }
        let key = item.key.clone();
        let name = item.name.clone();
        self.shortcut_mapper.context_menu = None;
        if delete { self.shortcut_mapper.confirm = Some(key); self.shortcut_mapper.focus.focus(window, cx); }
        else { self.persist_shortcut_change(key, name, Vec::new(), false, window, cx); }
        cx.notify();
    }

    fn persist_shortcut_change(&mut self, key: CommandKey, name: String, bindings: Vec<KeyCombo>, delete: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.settings_store.as_ref() else { self.shortcut_mapper.message = "The private configuration store is unavailable".into(); cx.notify(); return; };
        let mut candidate = self.shortcut_mapper.bindings.clone();
        let result = if delete { candidate.delete(&key) } else { candidate.modify(&key, &name, bindings) };
        let result = result.and_then(|_| {
            if !self.shortcut_mapper.defaults.is_empty() { let mut templates = self.shortcut_mapper.extra.clone(); templates.extend(shortcut_templates(&candidate)); candidate.native_bindings(self.shortcut_mapper.defaults.clone(), templates, cx)?; }
            candidate.save(store)?;
            if let CommandKey::Macro(index) = key {
                let previous_name = self.macro_dialogs.library.macros.get(index).map(|saved| saved.name.clone()).ok_or_else(|| io::Error::other("Macro is no longer available"))?;
                let update = store.update_json(Path::new("macros.json"), &serde_json::json!({"schema_version": 1, "macros": []}), |document| {
                    let entries = document.get_mut("macros").and_then(serde_json::Value::as_array_mut).ok_or_else(|| io::Error::other("Invalid macro library"))?;
                    if entries.get(index).and_then(|entry| entry.get("name")).and_then(serde_json::Value::as_str) != Some(previous_name.as_str()) { return Err(io::Error::other("Macro library changed; reopen Shortcut mapper")); }
                    if delete { entries.remove(index); } else { entries[index]["name"] = name.clone().into(); }
                    Ok(())
                });
                if let Err(error) = update {
                    return match self.shortcut_mapper.bindings.save(store) { Ok(()) => Err(error), Err(rollback) => Err(io::Error::other(format!("{error}; shortcuts.xml rollback failed: {rollback}"))) };
                }
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                self.shortcut_mapper.bindings = candidate;
                self.shortcut_mapper.edit = None;
                self.shortcut_mapper.confirm = None;
                self.refresh_shortcuts(window, cx);
                self.shortcut_mapper.grid_focus.focus(window, cx);
            }
            Err(error) => self.shortcut_mapper.message = format!("Could not save shortcuts: {error}"),
        }
        cx.notify();
    }

    fn apply_shortcut_combo(&mut self, add: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.shortcut_mapper.edit.as_mut() else { return; };
        if edit.key.page() != 4 || !edit.combo.valid() { return; }
        if edit.bindings.contains(&edit.combo) || (!add && edit.combo.key == 0 && edit.bindings.len() > 1) { return; }
        edit.warning_visible = true;
        if add {
            if edit.combo.key != 0 && !edit.bindings.contains(&edit.combo) { if edit.bindings.len() == 1 && edit.bindings[0].key == 0 { edit.bindings[0] = edit.combo; edit.selected = 0; } else { edit.bindings.push(edit.combo); edit.selected = edit.bindings.len() - 1; } }
        } else if edit.combo.key == 0 {
            if edit.bindings.len() == 1 { edit.bindings[0] = KeyCombo::default(); }
        } else if edit.bindings.iter().enumerate().any(|(index, combo)| index != edit.selected && *combo == edit.combo) {
            self.shortcut_mapper.message = "This shortcut is already assigned to this command".into();
        } else if let Some(combo) = edit.bindings.get_mut(edit.selected) { *combo = edit.combo; }
        else { edit.bindings.push(edit.combo); edit.selected = 0; }
        cx.notify();
    }

    fn confirm_shortcut_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcut_mapper.name.read(cx).has_composition() { self.shortcut_mapper.message = "Commit the name before saving".into(); cx.notify(); return; }
        let name = self.shortcut_mapper.name.read(cx).text();
        let Some(edit) = self.shortcut_mapper.edit.as_mut() else { return; };
        if matches!(edit.key, CommandKey::Macro(_) | CommandKey::Run(_)) && name.is_empty() { return; }
        if edit.key.page() != 4 && !edit.combo.valid() { return; }
        if edit.key.page() != 4 { edit.bindings = if edit.combo.key == 0 { Vec::new() } else { vec![edit.combo] }; }
        let key = edit.key.clone();
        let bindings = edit.bindings.clone();
        self.persist_shortcut_change(key, name, bindings, false, window, cx);
    }

    fn reconcile_shortcut_selection(&mut self, cx: &mut Context<Self>) {
        let filter = self.shortcut_mapper.filter.read(cx).text();
        let locale = self.native_locale();
        for page in 0..5 {
            let mut rows = self.shortcut_mapper.bindings.shortcuts.iter().filter(|item| item.key.page() == page && item.matches_localized(&filter, &locale));
            let first = rows.next();
            let selected = self.shortcut_mapper.selected[page].as_ref();
            if !first.into_iter().chain(rows).any(|item| Some(&item.key) == selected) {
                self.shortcut_mapper.selected[page] = first.map(|item| item.key.clone());
                self.shortcut_mapper.scroll[page].scroll_to_item(0);
            }
        }
    }

    fn move_shortcut_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let filter = self.shortcut_mapper.filter.read(cx).text();
        let locale = self.native_locale();
        let page = self.shortcut_mapper.page;
        let rows = self.shortcut_mapper.bindings.shortcuts.iter().filter(|item| item.key.page() == page && item.matches_localized(&filter, &locale)).collect::<Vec<_>>();
        if rows.is_empty() { return; }
        let current = rows.iter().position(|item| self.shortcut_mapper.selected[page].as_ref() == Some(&item.key)).unwrap_or(0);
        let next = current.saturating_add_signed(delta).min(rows.len() - 1);
        self.shortcut_mapper.selected[page] = Some(rows[next].key.clone());
        self.shortcut_mapper.scroll[page].scroll_to_item(next);
        cx.notify();
    }

    fn activate_shortcut_button(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "shortcut-modify" => self.begin_shortcut_edit(window, cx),
            "shortcut-clear" => self.request_shortcut_clear(false, window, cx),
            "shortcut-delete" => self.request_shortcut_clear(true, window, cx),
            "shortcut-close" => self.close_shortcut_mapper(window, cx),
            "shortcut-filter-clear" => { self.shortcut_mapper.filter.update(cx, |field, cx| { field.set_text("", window, cx); field.focus(window, cx); }); cx.notify(); }
            "shortcut-confirm-ok" => {
                if let Some(key) = self.shortcut_mapper.confirm.clone() {
                    let name = self.shortcut_mapper.bindings.shortcuts.iter().find(|item| item.key == key).map(|item| item.name.clone()).unwrap_or_default();
                    self.persist_shortcut_change(key, name, Vec::new(), true, window, cx);
                }
            }
            "shortcut-confirm-cancel" => { self.shortcut_mapper.confirm = None; self.shortcut_mapper.grid_focus.focus(window, cx); cx.notify(); }
            "shortcut-edit-ok" => self.confirm_shortcut_edit(window, cx),
            "shortcut-edit-cancel" => { self.shortcut_mapper.edit = None; self.shortcut_mapper.grid_focus.focus(window, cx); cx.notify(); }
            "shortcut-scintilla-add" => self.apply_shortcut_combo(true, cx),
            "shortcut-scintilla-apply" => self.apply_shortcut_combo(false, cx),
            "shortcut-scintilla-remove" => {
                if let Some(edit) = &mut self.shortcut_mapper.edit {
                    if edit.bindings.len() > 1 && edit.selected < edit.bindings.len() { edit.bindings.remove(edit.selected); }
                    edit.selected = edit.selected.min(edit.bindings.len().saturating_sub(1));
                    edit.combo = edit.bindings.get(edit.selected).copied().unwrap_or_default();
                    edit.warning_visible = true;
                }
                cx.notify();
            }
            _ => {},
        }
    }

    fn shortcut_scrollbar(&self, axis: usize, cx: &Context<Self>) -> AnyElement {
        let horizontal = axis == 1;
        let page = self.shortcut_mapper.page;
        let scroll = match axis { 1 => self.shortcut_mapper.horizontal_scroll[page].clone(), 2 => self.shortcut_mapper.info_scroll.clone(), _ => self.shortcut_mapper.scroll[page].clone() };
        let paint_scroll = scroll.clone();
        let key_scroll = scroll.clone();
        let bounds = self.shortcut_mapper.scroll_bounds[axis].clone();
        let paint_bounds = bounds.clone();
        let dark = self.dark;
        let mut bar = div().id(match axis { 1 => "shortcut-horizontal-scrollbar", 2 => "shortcut-conflict-scrollbar", _ => "shortcut-vertical-scrollbar" }).role(Role::ScrollBar).aria_orientation(if horizontal { gpui::accesskit::Orientation::Horizontal } else { gpui::accesskit::Orientation::Vertical }).tab_index(-1).absolute().bg(rgb(if dark { 0x282828 } else { 0xf0f0f0 }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                let bounds = bounds.get();
                let length = dialog_font::logical(if horizontal { bounds.size.width } else { bounds.size.height });
                let position = dialog_font::logical(if horizontal { event.position.x - bounds.left() } else { event.position.y - bounds.top() });
                let viewport = dialog_font::logical(if horizontal { scroll.bounds().size.width } else { scroll.bounds().size.height });
                let maximum = dialog_font::logical(if horizontal { scroll.max_offset().x } else { scroll.max_offset().y }).max(0.);
                let track = (length - GRID_SCROLLBAR * 2.).max(0.);
                let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
                let offset = -dialog_font::logical(if horizontal { scroll.offset().x } else { scroll.offset().y });
                let top = GRID_SCROLLBAR + if maximum > 0. { offset / maximum * (track - thumb) } else { 0. };
                let delta = if position < GRID_SCROLLBAR { -GRID_ROW_HEIGHT } else if position > length - GRID_SCROLLBAR { GRID_ROW_HEIGHT } else if position < top { -viewport } else if position > top + thumb { viewport } else { view.shortcut_mapper.scroll_grab[axis].set(Some(position - top)); 0. };
                let next = px(-(offset + delta).clamp(0., maximum));
                scroll.set_offset(if horizontal { gpui::point(next, scroll.offset().y) } else { gpui::point(scroll.offset().x, next) });
                cx.stop_propagation(); cx.notify();
            }))
            .on_key_down(cx.listener(move |_, event: &gpui::KeyDownEvent, _, cx| {
                let viewport = dialog_font::logical(if horizontal { key_scroll.bounds().size.width } else { key_scroll.bounds().size.height });
                let delta = match event.keystroke.key.as_str() { "up" | "left" => -GRID_ROW_HEIGHT, "down" | "right" => GRID_ROW_HEIGHT, "pageup" => -viewport, "pagedown" => viewport, _ => return };
                let maximum = dialog_font::logical(if horizontal { key_scroll.max_offset().x } else { key_scroll.max_offset().y }).max(0.);
                let offset = -dialog_font::logical(if horizontal { key_scroll.offset().x } else { key_scroll.offset().y });
                let next = px(-(offset + delta).clamp(0., maximum));
                key_scroll.set_offset(if horizontal { gpui::point(next, key_scroll.offset().y) } else { gpui::point(key_scroll.offset().x, next) });
                cx.stop_propagation(); cx.notify();
            }))
            .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                paint_bounds.set(bounds);
                let length = dialog_font::logical(if horizontal { bounds.size.width } else { bounds.size.height });
                let viewport = dialog_font::logical(if horizontal { paint_scroll.bounds().size.width } else { paint_scroll.bounds().size.height });
                let maximum = dialog_font::logical(if horizontal { paint_scroll.max_offset().x } else { paint_scroll.max_offset().y }).max(0.);
                let track = (length - GRID_SCROLLBAR * 2.).max(0.);
                if maximum > 0. && track > 0. {
                    let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
                    let offset = -dialog_font::logical(if horizontal { paint_scroll.offset().x } else { paint_scroll.offset().y });
                    let top = GRID_SCROLLBAR + offset / maximum * (track - thumb);
                    let thumb_bounds = if horizontal { gpui::Bounds::new(gpui::point(bounds.left() + px(top), bounds.top() + px(2.)), size(px(thumb), px(GRID_SCROLLBAR - 4.))) } else { gpui::Bounds::new(gpui::point(bounds.left() + px(2.), bounds.top() + px(top)), size(px(GRID_SCROLLBAR - 4.), px(thumb))) };
                    window.paint_quad(gpui::fill(thumb_bounds, rgb(if dark { 0x666666 } else { 0xcdcdcd })));
                }
            }).size_full());
        for (end, label) in [(false, if horizontal { "‹" } else { "⌃" }), (true, if horizontal { "›" } else { "⌄" })] {
            let mut arrow = div().absolute().size(px(GRID_SCROLLBAR)).text_center().line_height(px(GRID_SCROLLBAR)).text_color(rgb(if dark { 0xb0b0b0 } else { 0x606060 })).child(label);
            arrow = if horizontal { arrow.top(px(0.)).when(end, |arrow| arrow.right(px(0.))).when(!end, |arrow| arrow.left(px(0.))) } else { arrow.left(px(0.)).when(end, |arrow| arrow.bottom(px(0.))).when(!end, |arrow| arrow.top(px(0.))) };
            bar = bar.child(arrow);
        }
        bar = if horizontal { bar.left(px(0.)).right(px(GRID_SCROLLBAR)).bottom(px(0.)).h(px(GRID_SCROLLBAR)) } else { bar.right(px(0.)).top(px(if axis == 2 { 0. } else { GRID_HEADER_HEIGHT })).bottom(px(0.)).w(px(GRID_SCROLLBAR)) };
        bar.into_any_element()
    }

    fn move_shortcut_scrollbar(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        for (axis, grab) in self.shortcut_mapper.scroll_grab.iter().enumerate() {
            if event.pressed_button != Some(MouseButton::Left) { grab.set(None); continue; }
            let Some(grab) = grab.get() else { continue; };
            let horizontal = axis == 1;
            let bounds = self.shortcut_mapper.scroll_bounds[axis].get();
            let scroll = match axis { 1 => &self.shortcut_mapper.horizontal_scroll[self.shortcut_mapper.page], 2 => &self.shortcut_mapper.info_scroll, _ => &self.shortcut_mapper.scroll[self.shortcut_mapper.page] };
            let length = dialog_font::logical(if horizontal { bounds.size.width } else { bounds.size.height });
            let viewport = dialog_font::logical(if horizontal { scroll.bounds().size.width } else { scroll.bounds().size.height });
            let maximum = dialog_font::logical(if horizontal { scroll.max_offset().x } else { scroll.max_offset().y }).max(0.);
            let track = (length - GRID_SCROLLBAR * 2.).max(0.);
            let thumb = (track * viewport / (viewport + maximum).max(1.)).max(18.).min(track);
            if track > thumb {
                let position = dialog_font::logical(if horizontal { event.position.x - bounds.left() } else { event.position.y - bounds.top() });
                let next = px(-maximum * ((position - GRID_SCROLLBAR - grab) / (track - thumb)).clamp(0., 1.));
                scroll.set_offset(if horizontal { gpui::point(next, scroll.offset().y) } else { gpui::point(scroll.offset().x, next) });
                cx.notify();
            }
        }
    }

    pub(super) fn render_shortcut_mapper(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shortcut_mapper.visible { return None; }
        let _key_mapping = shortcuts::KeyMappingScope::new();
        let state = &self.shortcut_mapper;
        let page = state.page;
        let filter = state.filter.read(cx).text();
        let locale = self.native_locale();
        let mapper_control = |id, fallback| crate::localization::dialog_control(&locale, "ShortcutMapper", id).unwrap_or(fallback);
        let edit_control = |id, fallback| crate::localization::dialog_control(&locale, "ShortcutMapperSubDialg", id).unwrap_or(fallback);
        let modify_label = mapper_control(2602, "Modify");
        let clear_label = mapper_control(2606, "Clear");
        let delete_label = mapper_control(2603, "Delete");
        let close_label = mapper_control(1, "Close");
        let ok_label = edit_control(1, "OK");
        let cancel_label = edit_control(2, "Cancel");
        let add_label = edit_control(5008, "Add");
        let remove_label = edit_control(5009, "Remove");
        let apply_label = edit_control(5010, "Apply");
        let name_label = edit_control(5006, "Name:");
        let key_label = edit_control(5004, "Key");
        let foreground = rgb(if self.dark { 0xe0e0e0 } else { 0x202020 });
        let background = rgb(if self.dark { 0x282828 } else { 0xf0f0f0 });
        let border = rgb(if self.dark { 0x606060 } else { 0x808080 });
        let button = |id: &'static str, label: &'static str, enabled: bool| div().id(id).role(Role::Button).aria_label(label).tab_index(if enabled { 0 } else { -1 }).a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).px(px(14.)).py(px(3.)).border_1().border_color(border).text_color(if enabled { foreground } else { rgb(0x808080) }).child(label).on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { if enabled && matches!(event.keystroke.key.as_str(), "enter" | "space") { view.activate_shortcut_button(id, window, cx); cx.stop_propagation(); } }));
        let mut tabs = div().id("shortcut-mapper-tabs").flex().items_end().role(Role::TabList).h(px(21.)).min_h(px(21.)).ml(px(3.));
        for index in 0..shortcuts::PAGES.len() {
            let label = mapper_page(&locale, index);
            tabs = tabs.child(div().id(gpui::SharedString::from(format!("shortcut-page-{index}"))).role(Role::Tab).aria_label(label).aria_selected(page == index).tab_index(0).flex_shrink_0().h(px(if page == index { 21. } else { 19. })).px(px(6.)).border_1().border_color(rgb(if self.dark { 0x606060 } else { 0xd0d0d0 })).bg(if page == index { background } else { rgb(if self.dark { 0x202020 } else { 0xf0f0f0 }) }).child(label).on_click(cx.listener(move |view, _, _, cx| { if view.shortcut_mapper.edit.is_some() || view.shortcut_mapper.confirm.is_some() { return; } view.shortcut_mapper.page = index; view.shortcut_mapper.context_menu = None; cx.notify(); })).on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| { if view.shortcut_mapper.edit.is_some() || view.shortcut_mapper.confirm.is_some() { return; } let target = match event.keystroke.key.as_str() { "space" | "enter" => index, "left" => view.shortcut_mapper.page.saturating_sub(1), "right" => (view.shortcut_mapper.page + 1).min(4), _ => return }; view.shortcut_mapper.page = target; view.shortcut_mapper.context_menu = None; cx.stop_propagation(); cx.notify(); })));
        }
        let third = match page { 0 => Some(mapper_string(&locale, "ColumnCategory", "Category")), 3 => Some(mapper_string(&locale, "ColumnPlugin", "Plugin")), _ => None };
        let measure = |text: &str| dialog_font::logical(window.text_system().shape_line(text.to_owned().into(), px(13.5), &[gpui::TextRun { len: text.len(), font: gpui::font(dialog_font::font_family()), color: foreground.into(), background_color: None, underline: None, strikethrough: None }], None).width()).ceil() + 15.;
        let mut widths: [f32; 4] = [30., 210., 154., third.map_or(0., |text| measure(text))];
        for item in state.bindings.shortcuts.iter().filter(|item| item.key.page() == page && item.matches_localized(&filter, &locale)) {
            widths[1] = widths[1].max(measure(&item.localized_name(&locale)));
            widths[2] = widths[2].max(measure(&item.label()));
            if third.is_some() { widths[3] = widths[3].max(measure(&mapper_category(&locale, item))); }
        }
        let last = if third.is_some() { 3 } else { 2 };
        let available = (dialog_font::logical(window.viewport_size().width) - 14. - GRID_SCROLLBAR).max(0.);
        widths[last] = widths[last].max(available - widths[..last].iter().sum::<f32>());
        let content_width: f32 = widths[..=last].iter().sum();
        let horizontal = content_width > available + 0.5;
        let column = |text: String, width: f32| div().w(px(width)).min_w(px(width)).flex_shrink_0().h_full().px(px(2.)).overflow_hidden().border_r_1().border_b_1().border_color(rgb(if self.dark { 0x484848 } else { 0xdcdcdc })).child(text);
        let title = |text: String, width: f32| column(text, width).flex().items_center().justify_center().text_size(px(16.5)).font_weight(gpui::FontWeight::BOLD).border_color(rgb(if self.dark { 0x606060 } else { 0x787878 }));
        let mut header = div().relative().flex().pl(px(widths[0])).h(px(GRID_HEADER_HEIGHT)).min_h(px(GRID_HEADER_HEIGHT)).line_height(px(22.)).bg(background).child(title(mapper_string(&locale, "ColumnName", "Name").into(), widths[1])).child(title(mapper_string(&locale, "ColumnShortcut", "Shortcut").into(), widths[2]));
        if let Some(third) = third { header = header.child(title(third.into(), widths[3])); }
        header = header.child(title(String::new(), widths[0]).absolute().left(-state.horizontal_scroll[page].offset().x).top(px(0.)).bg(background));
        let grid_focused = state.grid_focus.is_focused(window) && state.edit.is_none() && state.confirm.is_none();
        let mut rows = div().id("shortcut-grid-rows").role(Role::ListBox).track_focus(&state.grid_focus).flex().flex_col().overflow_y_scroll().track_scroll(&state.scroll[page]).text_size(px(13.5)).flex_1().min_h(px(0.)).on_scroll(cx.listener(|_, _, _, cx| cx.notify()));
        for (ordinal, item) in state.bindings.shortcuts.iter().filter(|item| item.key.page() == page && item.matches_localized(&filter, &locale)).enumerate() {
            let selected = state.selected[page].as_ref() == Some(&item.key);
            let conflicts = !state.bindings.conflicts(&item.key, &item.bindings).is_empty();
            let key = item.key.clone();
            let context_key = key.clone();
            let row_background = rgb(if selected { if conflicts { if grid_focused { 0xf40a14 } else { 0xe6c2be } } else if grid_focused { if self.dark { 0x30465a } else { 0x000080 } } else if self.dark { 0x383838 } else { 0xc8c8d2 } } else if conflicts { if self.dark { 0x633333 } else { 0xff8278 } } else if self.dark { 0x202020 } else { 0xffffff });
            let mut row = div().id(gpui::SharedString::from(format!("shortcut-row-{page}-{ordinal}"))).role(Role::ListBoxOption).aria_selected(selected).relative().flex().pl(px(widths[0])).flex_shrink_0().h(px(GRID_ROW_HEIGHT)).min_h(px(GRID_ROW_HEIGHT)).line_height(px(GRID_ROW_HEIGHT - 2.)).bg(row_background).text_color(if selected && grid_focused { rgb(0xffffff) } else { foreground }).child(column(item.localized_name(&locale), widths[1])).child(column(item.label(), widths[2]));
            if third.is_some() { row = row.child(column(mapper_category(&locale, item), widths[3])); }
            row = row.child(column((ordinal + 1).to_string(), widths[0]).absolute().left(-state.horizontal_scroll[page].offset().x).top(px(0.)).bg(background).text_color(foreground).text_right().border_color(rgb(if self.dark { 0x606060 } else { 0x787878 })));
            rows = rows.child(row.on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| { view.shortcut_mapper.context_menu = None; view.shortcut_mapper.selected[page] = Some(key.clone()); view.shortcut_mapper.grid_focus.focus(window, cx); if event.click_count == 2 { view.begin_shortcut_edit(window, cx); } cx.stop_propagation(); cx.notify(); })).on_mouse_down(MouseButton::Right, cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| { view.shortcut_mapper.selected[page] = Some(context_key.clone()); view.shortcut_mapper.context_menu = Some(event.position); view.shortcut_mapper.context_selected = 0; view.shortcut_mapper.grid_focus.focus(window, cx); cx.stop_propagation(); cx.notify(); })));
        }
        let selected = state.selected().filter(|item| item.matches_localized(&filter, &locale));
        let editing_bindings = state.edit.as_ref().map(|edit| [edit.combo]);
        let conflict_target = state.edit.as_ref().zip(editing_bindings.as_ref()).map(|(edit, bindings)| (&edit.key, bindings.as_slice())).or_else(|| selected.map(|item| (&item.key, item.bindings.as_slice())));
        let information = if !state.message.is_empty() { state.message.clone() } else if let Some((key, bindings)) = conflict_target {
            let conflicts = state.bindings.conflicts(key, bindings);
            if conflicts.is_empty() { if state.edit.is_some() { mapper_string(&locale, "ConflictInfoEditing", "No conflicts . . .").into() } else { mapper_string(&locale, "ConflictInfoOk", "No shortcut conflicts for this item.").into() } } else { conflicts.iter().map(|item| {
                let ordinal = state.bindings.shortcuts.iter().filter(|other| other.key.page() == item.key.page()).position(|other| other.key == item.key).unwrap_or(0) + 1;
                item.bindings.iter().enumerate().filter(|(_, other)| bindings.iter().any(|candidate| candidate.conflicts_with(**other))).map(|(index, combo)| format!("{}  |  {}{}   {}  ( {} )", mapper_page(&locale, item.key.page()), ordinal, if item.key.page() == 4 && index > 0 { "*" } else { "" }, item.localized_name(&locale), combo.label())).collect::<Vec<_>>().join("\n")
            }).collect::<Vec<_>>().join("\n") }
        } else { String::new() };
        let grid = div().relative().flex_1().min_h(px(0.)).mb(px(22.)).border_1().border_color(border)
            .child(div().absolute().inset_0().bottom(px(if horizontal { GRID_SCROLLBAR } else { 0. })).pr(px(GRID_SCROLLBAR)).flex()
                .child(div().id("shortcut-grid-horizontal").flex_1().min_w(px(0.)).h_full().overflow_x_scroll().track_scroll(&state.horizontal_scroll[page]).on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                    .child(div().w(px(content_width)).h_full().flex().flex_col().child(header).child(rows)))
                .child(self.shortcut_scrollbar(0, cx)))
            .when(horizontal, |grid| grid.child(self.shortcut_scrollbar(1, cx)));
        let information = div().relative().h(px(50.75)).min_h(px(50.75)).mb(px(10.5)).border_1().border_color(border).bg(rgb(if self.dark { 0x202020 } else { 0xffffff }))
            .child(div().id("shortcut-conflicts").h_full().mr(px(GRID_SCROLLBAR)).overflow_y_scroll().track_scroll(&state.info_scroll).p(px(3.)).on_scroll(cx.listener(|_, _, _, cx| cx.notify())).child(information))
            .child(self.shortcut_scrollbar(2, cx));
        let mut root = div().id("shortcut-mapper").role(Role::Dialog).aria_label(self.shortcut_mapper_title()).key_context("MatrixShortcutMapper").track_focus(&state.focus).relative().w_full().h_full().px(px(6.)).pt(px(10.5)).pb(px(12.5)).flex().flex_col().bg(background).text_color(foreground).text_size(px(12.)).line_height(px(16.)).occlude()
            .on_mouse_move(cx.listener(|view, event, _, cx| view.move_shortcut_scrollbar(event, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| { for grab in &view.shortcut_mapper.scroll_grab { grab.set(None); } }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _, _, _| { for grab in &view.shortcut_mapper.scroll_grab { grab.set(None); } }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key != "escape" && !(event.keystroke.key == "enter" && (view.shortcut_mapper.edit.is_some() || view.shortcut_mapper.confirm.is_some())) && !view.shortcut_mapper.focus.is_focused(window) && !view.shortcut_mapper.grid_focus.is_focused(window) { return; }
            if view.shortcut_mapper.context_menu.is_some() {
                let page = view.shortcut_mapper.page;
                let enabled = [0, 1, 2].into_iter().filter(|index| *index == 0 || (*index == 1 && matches!(page, 1 | 2)) || (*index == 2 && page != 4)).collect::<Vec<_>>();
                match event.keystroke.key.as_str() {
                    "escape" => view.shortcut_mapper.context_menu = None,
                    "up" | "down" => { let index = enabled.iter().position(|index| *index == view.shortcut_mapper.context_selected).unwrap_or(0); let next = if event.keystroke.key == "up" { (index + enabled.len() - 1) % enabled.len() } else { (index + 1) % enabled.len() }; view.shortcut_mapper.context_selected = enabled[next]; }
                    "enter" | "space" => { let id = ["shortcut-modify", "shortcut-delete", "shortcut-clear"][view.shortcut_mapper.context_selected]; view.shortcut_mapper.context_menu = None; view.activate_shortcut_button(id, window, cx); }
                    _ => return,
                }
                cx.stop_propagation(); cx.notify(); return;
            }
            match event.keystroke.key.as_str() {
                "escape" => { if view.shortcut_mapper.edit.is_some() || view.shortcut_mapper.confirm.is_some() { view.shortcut_mapper.edit = None; view.shortcut_mapper.confirm = None; view.shortcut_mapper.grid_focus.focus(window, cx); cx.notify(); } else { view.close_shortcut_mapper(window, cx); } cx.stop_propagation(); }
                "enter" if view.shortcut_mapper.confirm.is_some() => { view.activate_shortcut_button("shortcut-confirm-ok", window, cx); cx.stop_propagation(); }
                "enter" if view.shortcut_mapper.edit.is_some() => { view.confirm_shortcut_edit(window, cx); cx.stop_propagation(); }
                "up" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { view.move_shortcut_selection(-1, cx); cx.stop_propagation(); }
                "down" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { view.move_shortcut_selection(1, cx); cx.stop_propagation(); }
                "pageup" | "pagedown" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { let rows = (dialog_font::logical(view.shortcut_mapper.scroll[view.shortcut_mapper.page].bounds().size.height) / GRID_ROW_HEIGHT).floor().max(1.) as isize; view.move_shortcut_selection(if event.keystroke.key == "pageup" { -rows } else { rows }, cx); cx.stop_propagation(); }
                "home" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { view.move_shortcut_selection(isize::MIN, cx); cx.stop_propagation(); }
                "end" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { view.move_shortcut_selection(isize::MAX, cx); cx.stop_propagation(); }
                "delete" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { view.request_shortcut_clear(true, window, cx); cx.stop_propagation(); }
                "enter" if view.shortcut_mapper.focus.is_focused(window) => { view.close_shortcut_mapper(window, cx); cx.stop_propagation(); }
                "enter" if view.shortcut_mapper.edit.is_none() && view.shortcut_mapper.confirm.is_none() => { view.begin_shortcut_edit(window, cx); cx.stop_propagation(); }
                _ => {},
            }
        })).child(tabs).child(grid).child(information)
            .child(div().flex().items_center().h(px(21.)).min_h(px(21.)).mb(px(10.5)).gap(px(3.)).child(div().w(px(46.)).text_center().child(mapper_control(2607, "Filter:"))).child(div().flex_1().min_w(px(0.)).child(state.filter.clone())).child(button("shortcut-filter-clear", mapper_control(2609, "✕"), true).w(px(21.)).h(px(21.)).p(px(0.)).text_center().bg(background).on_click(cx.listener(|view, _, window, cx| view.activate_shortcut_button("shortcut-filter-clear", window, cx)))))
            .child(div().flex().flex_shrink_0().h(px(24.5)).justify_center().gap(px(14.))
                .children([("shortcut-modify", modify_label, selected.is_some()), ("shortcut-clear", clear_label, selected.is_some() && page != 4), ("shortcut-delete", delete_label, selected.is_some() && matches!(page, 1 | 2)), ("shortcut-close", close_label, true)].into_iter().map(|(id, label, enabled)| button(id, label, enabled).w(px(80.)).h(px(24.5)).p(px(0.)).flex().items_center().justify_center().bg(rgb(if self.dark { 0x383838 } else if enabled { 0xe1e1e1 } else { 0xcccccc })).when(id == "shortcut-close", |button| button.border_color(rgb(0x0078d7))).on_click(cx.listener(move |view, _, window, cx| { if enabled { view.activate_shortcut_button(id, window, cx); } })))));
        root = root.font_family(dialog_font::font_family());
        if let Some(position) = state.context_menu {
            let mut menu = div().id("shortcut-context-menu").absolute().left(position.x.min(px(620.))).top(position.y.min(px(420.))).w(px(150.)).bg(background).border_1().border_color(border).flex().flex_col().occlude();
            for (index, (id, label, enabled)) in [("shortcut-modify", mapper_string(&locale, "ModifyContextMenu", "Modify"), selected.is_some()), ("shortcut-delete", mapper_string(&locale, "DeleteContextMenu", "Delete"), selected.is_some() && matches!(page, 1 | 2)), ("shortcut-clear", mapper_string(&locale, "ClearContextMenu", "Clear"), selected.is_some() && page != 4)].into_iter().enumerate() {
                menu = menu.child(button(id, label, enabled).bg(if state.context_selected == index { rgb(0x0067c0) } else { background }).on_click(cx.listener(move |view, _, window, cx| { view.shortcut_mapper.context_menu = None; if enabled { view.activate_shortcut_button(id, window, cx); } cx.notify(); })));
            }
            root = root.child(div().id("shortcut-context-shield").absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, cx.listener(|view, _, _, cx| { view.shortcut_mapper.context_menu = None; cx.stop_propagation(); cx.notify(); }))).child(menu);
        }
        if state.edit.is_some() || state.confirm.is_some() { root = root.child(div().id("shortcut-modal-shield").absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())); }
        if let Some(key) = &state.confirm {
            let key = key.clone();
            let name = state.bindings.shortcuts.iter().find(|item| item.key == key).map(|item| item.name.clone()).unwrap_or_default();
            let title = crate::localization::dialog_title(&locale, "SCMapperDoDeleteOrNot").unwrap_or("Are you sure?");
            let message = crate::localization::dialog_message(&locale, "SCMapperDoDeleteOrNot").unwrap_or("Are you sure you want to delete this shortcut?");
            root = root.child(div().id("shortcut-confirm").role(Role::Alert).aria_label(title).absolute().left(px(130.)).top(px(170.)).w(px(520.)).bg(background).occlude().border_1().border_color(border).p(px(8.)).child(message).child(div().flex().gap(px(8.)).child(button("shortcut-confirm-ok", ok_label, true).on_click(cx.listener(move |view, _, window, cx| view.persist_shortcut_change(key.clone(), name.clone(), Vec::new(), true, window, cx)))).child(button("shortcut-confirm-cancel", cancel_label, true).on_click(cx.listener(|view, _, window, cx| view.activate_shortcut_button("shortcut-confirm-cancel", window, cx))))));
        }
        if let Some(edit) = &state.edit {
            let scintilla = edit.key.page() == 4;
            let offset = if scintilla { 178.5 } else { 0. };
            let mut controls = div().absolute().inset_0();
            for (index, label, checked) in [(0, edit_control(5001, "CTRL"), edit.combo.ctrl), (1, edit_control(5002, "ALT"), edit.combo.alt), (2, edit_control(5003, "SHIFT"), edit.combo.shift), (3, "CMD", edit.combo.cmd)] {
                let (left, top, width) = match index { 0 => (17.5, 40.5, 75.25), 1 => (17.5, 72., 75.25), 2 => (99.75, 40.5, 82.25), _ => (99.75, 72., 82.25) };
                controls = controls.child(div().id(gpui::SharedString::from(format!("shortcut-modifier-{index}"))).role(Role::CheckBox).aria_label(label).aria_toggled(checked.into()).tab_index(0).absolute().left(px(left + offset)).top(px(top)).w(px(width)).h(px(15.)).child(format!("{} {label}", if checked { "☑" } else { "☐" })).on_click(cx.listener(move |view, _, _, cx| { if let Some(edit) = &mut view.shortcut_mapper.edit { match index { 0 => edit.combo.ctrl = !edit.combo.ctrl, 1 => edit.combo.alt = !edit.combo.alt, 2 => edit.combo.shift = !edit.combo.shift, _ => edit.combo.cmd = !edit.combo.cmd }; if edit.key.page() == 4 { edit.warning_visible = true; } } cx.notify(); })).on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| { if event.keystroke.key == "space" { if let Some(edit) = &mut view.shortcut_mapper.edit { match index { 0 => edit.combo.ctrl = !edit.combo.ctrl, 1 => edit.combo.alt = !edit.combo.alt, 2 => edit.combo.shift = !edit.combo.shift, _ => edit.combo.cmd = !edit.combo.cmd }; if edit.key.page() == 4 { edit.warning_visible = true; } } cx.stop_propagation(); cx.notify(); } })));
            }
            controls = controls.child(div().absolute().left(px(78.75 + offset)).top(px(58.5)).child("+")).child(div().absolute().left(px(168. + offset)).top(px(58.5)).child("+"));
            controls = controls.child(div().id("shortcut-key-choice").role(Role::ComboBox).aria_label(key_label).aria_value(shortcuts::key_name(edit.combo.key)).aria_expanded(edit.choose_key).track_focus(&state.key_focus).absolute().left(px(192.5 + offset)).top(px(57.)).w(px(112.)).h(px(21.)).border_1().border_color(border).px(px(3.)).child(format!("{} ▾", shortcuts::key_name(edit.combo.key))).on_click(cx.listener(|view, _, window, cx| { view.shortcut_mapper.key_focus.focus(window, cx); if let Some(edit) = &mut view.shortcut_mapper.edit { edit.choose_key = !edit.choose_key; } cx.notify(); })).on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| { if let Some(edit) = &mut view.shortcut_mapper.edit { match event.keystroke.key.as_str() { "escape" if edit.choose_key => edit.choose_key = false, "escape" => return, "enter" if !edit.choose_key => return, "space" | "enter" => edit.choose_key = !edit.choose_key, "up" | "down" => { let keys = shortcuts::key_choices(); let index = keys.iter().position(|key| *key == edit.combo.key).unwrap_or(0); let delta = if event.keystroke.key == "up" { -1 } else { 1 }; edit.combo.key = keys[index.saturating_add_signed(delta).min(keys.len() - 1)]; edit.warning_visible = true; }, key => { let Some(combo) = KeyCombo::parse(key) else { return; }; edit.combo.key = combo.key; edit.choose_key = false; edit.warning_visible = true; } }; cx.stop_propagation(); cx.notify(); } })));
            let edit_title = crate::localization::dialog_title(&locale, "ShortcutMapperSubDialg").unwrap_or("Shortcut");
            let mut editor = div().id("shortcut-edit").role(Role::Dialog).aria_label(edit_title).relative().w(px(if scintilla { 493.5 } else { 315. })).h(px(if scintilla { 172.5 } else { 145.5 })).bg(background).occlude().border_1().border_color(border).child(div().absolute().left(px(if scintilla { 194.25 } else { 17.5 })).top(px(13.5)).w(px(75.25)).text_right().child(name_label)).child(div().absolute().left(px(if scintilla { 276.5 } else { 98. })).top(px(12.)).w(px(206.5)).h(px(18.)).child(state.name.clone())).child(controls);
            let mut key_list = None;
            if edit.choose_key {
                let mut keys = div().id("shortcut-key-list").track_focus(&state.key_list_focus).absolute().left(px(192.5 + offset)).top(px(78.)).w(px(112.)).max_h(px(180.)).bg(background).occlude().overflow_y_scroll().border_1().border_color(border).on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    let Some(edit) = &mut view.shortcut_mapper.edit else { return; };
                    match event.keystroke.key.as_str() {
                        "escape" | "enter" | "space" => edit.choose_key = false,
                        "up" | "down" => { let keys = shortcuts::key_choices(); let index = keys.iter().position(|key| *key == edit.combo.key).unwrap_or(0); let delta = if event.keystroke.key == "up" { -1 } else { 1 }; edit.combo.key = keys[index.saturating_add_signed(delta).min(keys.len() - 1)]; edit.warning_visible = true; }
                        key => { let Some(combo) = KeyCombo::parse(key) else { return; }; edit.combo.key = combo.key; edit.choose_key = false; edit.warning_visible = true; }
                    }
                    if edit.choose_key { view.shortcut_mapper.key_list_focus.focus(window, cx); } else { view.shortcut_mapper.key_focus.focus(window, cx); }
                    cx.stop_propagation(); cx.notify();
                }));
                for key in shortcuts::key_choices() {
                    keys = keys.child(div().id(gpui::SharedString::from(format!("shortcut-key-{key}"))).tab_index(0).bg(if key == edit.combo.key { rgb(0x0067c0) } else { background }).text_color(if key == edit.combo.key { rgb(0xffffff) } else { foreground }).child(shortcuts::key_name(key)).on_click(cx.listener(move |view, _, window, cx| { if let Some(edit) = &mut view.shortcut_mapper.edit { edit.combo.key = key; edit.choose_key = false; edit.warning_visible = true; } view.shortcut_mapper.key_focus.focus(window, cx); cx.notify(); })).on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| { if matches!(event.keystroke.key.as_str(), "space" | "enter") { if let Some(edit) = &mut view.shortcut_mapper.edit { edit.combo.key = key; edit.choose_key = false; edit.warning_visible = true; } view.shortcut_mapper.key_focus.focus(window, cx); cx.stop_propagation(); cx.notify(); } })));
                }
                key_list = Some(keys);
            }
            if scintilla {
                let mut list = div().id("shortcut-scintilla-bindings").absolute().left(px(10.5)).top(px(9.)).w(px(157.5)).h(px(108.)).overflow_y_scroll().border_1().border_color(border);
                for (index, combo) in edit.bindings.iter().enumerate() {
                    let combo = *combo;
                    list = list.child(div().id(gpui::SharedString::from(format!("shortcut-scintilla-binding-{index}"))).tab_index(0).bg(if edit.selected == index { rgb(0x0067c0) } else { background }).child(combo.label()).on_click(cx.listener(move |view, _, _, cx| { if let Some(edit) = &mut view.shortcut_mapper.edit { edit.selected = index; edit.combo = combo; edit.warning_visible = true; } cx.notify(); })));
                }
                list = list.on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                    if let Some(edit) = &mut view.shortcut_mapper.edit {
                        let delta = match event.keystroke.key.as_str() { "up" => -1, "down" => 1, _ => return };
                        edit.selected = edit.selected.saturating_add_signed(delta).min(edit.bindings.len().saturating_sub(1));
                        edit.combo = edit.bindings.get(edit.selected).copied().unwrap_or_default();
                        edit.warning_visible = true;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }));
                editor = editor.child(list).child(button("shortcut-scintilla-add", add_label, edit.combo.valid() && edit.combo.key != 0 && !edit.bindings.contains(&edit.combo)).absolute().left(px(10.5)).top(px(126.)).w(px(73.5)).h(px(21.)).on_click(cx.listener(|view, _, _, cx| view.apply_shortcut_combo(true, cx)))).child(button("shortcut-scintilla-remove", remove_label, edit.bindings.len() > 1).absolute().left(px(94.5)).top(px(126.)).w(px(73.5)).h(px(21.)).on_click(cx.listener(|view, _, window, cx| view.activate_shortcut_button("shortcut-scintilla-remove", window, cx)))).child(button("shortcut-scintilla-apply", apply_label, edit.combo.valid() && (edit.combo.key != 0 || edit.bindings.len() == 1) && !edit.bindings.contains(&edit.combo)).absolute().left(px(344.75)).top(px(99.)).w(px(87.5)).h(px(21.)).on_click(cx.listener(|view, _, _, cx| view.apply_shortcut_combo(false, cx))));
            }
            let conflicts = state.bindings.conflicts(&edit.key, &[edit.combo]);
            let disabled = edit.warning_visible && if scintilla { edit.bindings.first().is_none_or(|combo| combo.key == 0) } else { edit.combo.key == 0 };
            let warning = if disabled { edit_control(5007, if scintilla { "This will remove shortcut from this command" } else { "This will disable the accelerator" }) } else if conflicts.is_empty() { "" } else { edit_control(5012, "CONFLICT FOUND!") };
            let name_valid = !matches!(edit.key, CommandKey::Macro(_) | CommandKey::Run(_)) || !state.name.read(cx).text().is_empty();
            editor = editor.child(div().absolute().left(px(10.5)).top(px(if scintilla { 153. } else { 126. })).child(warning)).child(button("shortcut-edit-ok", ok_label, name_valid && (scintilla || edit.combo.valid())).absolute().left(px(if scintilla { 246.75 } else { 68.25 })).top(px(if scintilla { 126. } else { 99. })).w(px(87.5)).h(px(21.)).on_click(cx.listener(|view, _, window, cx| view.confirm_shortcut_edit(window, cx)))).child(button("shortcut-edit-cancel", cancel_label, true).absolute().left(px(if scintilla { 344.75 } else { 166.25 })).top(px(if scintilla { 126. } else { 99. })).w(px(87.5)).h(px(21.)).on_click(cx.listener(|view, _, window, cx| view.activate_shortcut_button("shortcut-edit-cancel", window, cx))));
            if let Some(keys) = key_list { editor = editor.child(keys); }
            root = root.child(div().absolute().inset_0().flex().items_center().justify_center().child(editor));
        }
        Some(root.into_any_element())
    }
}

pub(super) fn key_for_action(action: &dyn gpui::Action) -> Option<CommandKey> {
    if let Some(action) = action.as_any().downcast_ref::<ShortcutAction>() { return Some(action.key.clone()); }
    if let Some(key) = shortcuts::scintilla_key_for_action(action.name()) { return Some(key); }
    let action = action.name().rsplit("::").next().unwrap_or(action.name());
    let id = match action {
        "NewFile" => 41001, "OpenFile" => 41002, "CloseCurrentFile" => 41003, "CloseAllFiles" => 41004,
        "SaveFile" => 41006, "SaveAllFiles" => 41007, "SaveFileAs" => 41008, "PrintFile" => 41010, "ReloadFile" => 41014,
        "ReopenClosedFile" => 41021, "EditorCut" => 42001, "EditorCopy" => 42002, "EditorPaste" => 42005,
        "ShowFind" => 43001, "ShowReplace" => 43003, "ShowGoTo" => 43004, "ToggleBookmark" => 43005,
        "FindNext" => 43002, "FindPrevious" => 43010,
        "MruPreviousBuffer" => 50003, "MruNextBuffer" => 50004,
        "NextBookmark" => 43006, "PreviousBookmark" => 43007, "ShowFindInFiles" => 43013, "ShowRun" => 49000, "PlaybackMacro" => 42021,
        "ShowColumnEditor" => 42034,
        _ => return None,
    };
    Some(CommandKey::Menu { id, nth: 0 })
}

pub(super) fn key_for_binding(binding: &KeyBinding) -> Option<CommandKey> {
    if binding.predicate().is_some_and(|predicate| format!("{predicate:?}").contains("MatrixIncremental")) && binding.keystrokes().len() == 1 {
        let source = binding.keystrokes()[0].unparse(gpui::OperatingSystem::Mac);
        let id = match source.as_str() { "f3" => Some(43002), "shift-f3" => Some(43010), "ctrl-alt-i" => Some(43011), _ => None };
        if let Some(id) = id { return Some(CommandKey::Menu { id, nth: 0 }); }
    }
    if binding.predicate().is_some_and(|predicate| !format!("{predicate:?}").contains("MatrixEditor")) { return None; }
    if binding.action().name().ends_with("::TransformAction") {
        let source = binding.keystrokes().first()?.unparse(gpui::OperatingSystem::Mac);
        let id = match source.as_str() {
            "ctrl-i" => 42012, "ctrl-j" => 42013, "ctrl-shift-up" => 42014, "ctrl-shift-down" => 42015,
            "ctrl-shift-u" => 42016, "ctrl-u" => 42017, "ctrl-q" => 42022, "ctrl-shift-q" => 42023,
            "ctrl-k" => 42035, "ctrl-shift-k" => 42036, "ctrl-alt-enter" => 42057, "ctrl-alt-shift-enter" => 42058,
            "alt-u" => 42067, "alt-shift-u" => 42068, "ctrl-alt-u" => 42069, "ctrl-alt-shift-u" => 42070,
            _ => return None,
        };
        return Some(CommandKey::Menu { id, nth: 0 });
    }
    let key = key_for_action(binding.action())?;
    if let CommandKey::Menu { id: id @ (42001 | 42002 | 42005), .. } = key {
        let stroke = binding.keystrokes().first()?.inner();
        let nth = if (id == 42001 && stroke.key == "delete") || (matches!(id, 42002 | 42005) && stroke.key == "insert") { 0 } else { 1 };
        return Some(CommandKey::Menu { id, nth });
    }
    Some(key)
}

fn shortcut_templates(bindings: &BindingSet) -> Vec<(CommandKey, KeyBinding)> {
    bindings.shortcuts.iter().filter(|item| item.command.is_some() || matches!(item.key, CommandKey::Run(_) | CommandKey::Scintilla { .. } | CommandKey::Menu { id: 50003..=50005, .. })).map(|item| (item.key.clone(), KeyBinding::new("f24", ShortcutAction { key: item.key.clone() }, Some("MatrixEditor")))).collect()
}

fn shortcut_storage_error(error: impl std::fmt::Display) -> EditError { EditError::Storage(error.to_string()) }

fn shortcut_rectangle_limit(selections: &SelectionSet, forward: bool) -> Result<SelectionSet, EditError> {
    let mut positions = selections.ranges().iter().flat_map(|range| [(range.selection.anchor, range.anchor_virtual), (range.selection.head, range.head_virtual)]);
    let first = positions.next().ok_or(EditError::InvalidRange)?;
    let target = positions.fold(first, |target, position| if forward { target.max(position) } else { target.min(position) });
    SelectionSet::new(vec![crate::editor::SelectionRange { selection: Selection { anchor: target.0, head: target.0 }, anchor_virtual: target.1, head_virtual: target.1 }], 0)
}

fn supplemental_scintilla(id: u32) -> bool {
    matches!(id, 2004 | 2330 | 2288 | 2169 | 2401 | 2342 | 2343 | 2413..=2416 | 2311 | 2439..=2441 | 2312..=2315 | 2331 | 2332 | 2346 | 2348 | 2349 | 2450 | 2430 | 2652 | 2653 | 2435..=2438 | 2344 | 2337 | 2455 | 2339 | 2404)
}

fn scintilla_action(id: u32) -> Option<Box<dyn gpui::Action>> {
    let action: Box<dyn gpui::Action> = match id {
        2013 => Box::new(EditorSelectAll), 2180 => Box::new(EditorDelete), 2176 => Box::new(EditorUndo), 2011 => Box::new(EditorRedo),
        2329 => Box::new(EditorEnter), 2327 => Box::new(EditorTab), 2328 => Box::new(EditorBackTab),
        2333 => Box::new(ZoomIn), 2334 => Box::new(ZoomOut), 2373 => Box::new(ZoomReset), 2469 => Box::new(EditorDuplicateSelection), 2324 => Box::new(ToggleOverwrite),
        2300 => Box::new(EditorDown), 2301 => Box::new(EditorSelectDown), 2426 => Box::new(editor_surface::RectangleDown),
        2302 => Box::new(EditorUp), 2303 => Box::new(EditorSelectUp), 2427 => Box::new(editor_surface::RectangleUp),
        2304 => Box::new(EditorLeft), 2305 => Box::new(EditorSelectLeft), 2428 => Box::new(editor_surface::RectangleLeft),
        2306 => Box::new(EditorRight), 2307 => Box::new(EditorSelectRight), 2429 => Box::new(editor_surface::RectangleRight),
        2308 => Box::new(editor_surface::WordLeft), 2309 => Box::new(editor_surface::SelectWordLeft), 2310 => Box::new(editor_surface::WordRight), 2442 => Box::new(editor_surface::SelectWordRight),
        2390 => Box::new(editor_surface::WordPartLeft), 2391 => Box::new(editor_surface::SelectWordPartLeft), 2392 => Box::new(editor_surface::WordPartRight), 2393 => Box::new(editor_surface::SelectWordPartRight),
        2345 => Box::new(editor_surface::HomeDisplay), 2347 => Box::new(editor_surface::LineEndDisplay),
        2453 => Box::new(EditorHome), 2454 => Box::new(EditorSelectHome), 2431 => Box::new(editor_surface::RectangleHome),
        2451 => Box::new(EditorEnd), 2452 => Box::new(EditorSelectEnd), 2432 => Box::new(editor_surface::RectangleEnd),
        2316 => Box::new(EditorDocumentStart), 2317 => Box::new(EditorSelectDocumentStart), 2318 => Box::new(EditorDocumentEnd), 2319 => Box::new(EditorSelectDocumentEnd),
        2320 => Box::new(editor_surface::PageUp), 2321 => Box::new(editor_surface::SelectPageUp), 2433 => Box::new(editor_surface::RectanglePageUp),
        2322 => Box::new(editor_surface::PageDown), 2323 => Box::new(editor_surface::SelectPageDown), 2434 => Box::new(editor_surface::RectanglePageDown),
        2326 => Box::new(EditorBackspace), 2335 => Box::new(editor_surface::DeleteWordLeft), 2336 => Box::new(editor_surface::DeleteWordRight),
        2395 => Box::new(editor_surface::DeleteLineLeft), 2396 => Box::new(editor_surface::DeleteLineRight), 2338 => Box::new(EditorLineDelete), 2325 => Box::new(EditorEscape),
        2607 => Box::new(editor_surface::SwapMainAnchorCaret), 2606 => Box::new(editor_surface::RotateSelection),
        _ => return None,
    };
    Some(action)
}
