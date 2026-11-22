use super::*;
use super::dialog_font::px;
use super::preferences_surface::{Commit, FieldSpec, Page, pref_group, pref_label, pref_label_right, horizontal, horizontal_px};
use gpui::Entity;
use super::text_field::RenderPalette;
use serde_json::{Value, json};
use std::path::Path;

const URI_SCHEMES: &str = "svn:// cvs:// git:// imap:// irc:// irc6:// ircs:// ldap:// ldaps:// news: telnet:// gopher:// ssh:// sftp:// smb:// skype: snmp:// spotify: steam:// sms: slack:// chrome:// bitcoin:";

pub(super) const ASSOCIATION_GROUPS: &[(&str, &[&str])] = &[
    ("Notepad", &["txt", "log"]),
    ("ms ini/inf", &["ini", "inf"]),
    ("c, c++, objc", &["h", "hh", "hpp", "hxx", "c", "cpp", "cxx", "cc", "m", "mm", "vcxproj", "vcproj", "props", "vsprops", "manifest"]),
    ("java, c#, pascal", &["java", "cs", "pas", "pp", "inc"]),
    ("web script", &["html", "htm", "shtml", "shtm", "hta", "asp", "aspx", "css", "js", "json", "mjs", "jsm", "jsp", "php", "php3", "php4", "php5", "phps", "phpt", "phtml", "xml", "xhtml", "xht", "xul", "kml", "xaml", "xsml"]),
    ("public script", &["sh", "bsh", "bash", "bat", "cmd", "nsi", "nsh", "lua", "pl", "pm", "py"]),
    ("property script", &["rc", "as", "mx", "vb", "vbs"]),
    ("fortran, TeX, SQL", &["f", "for", "f90", "f95", "f2k", "tex", "sql"]),
    ("misc", &["nfo", "mak"]),
    ("customize", &[]),
];

pub(super) fn fields() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("openSaveDir.defaultDirPath", "", 259),
        FieldSpec::integer("recent_files_limit", 10, 0, 30, Commit::Blur),
        FieldSpec::integer("snapshot_backup_ms", 7, 1, i64::MAX / 1000, Commit::Immediate).with_scale(1000),
        FieldSpec::integer("backup_retention.days", 90, 1, i64::MAX / 86400, Commit::Blur),
        FieldSpec::text("backup_directory", "", 259),
        FieldSpec::text("insertDateTime.customizedFormat", "yyyy-MM-dd HH:mm:ss", 255),
        FieldSpec::text("sessionExt", "", 259),
        FieldSpec::text("workspaceExt", "", 259),
    ]
}

impl MatrixWindow {
    pub(super) fn finish_files_preference_fields(&mut self,window: &mut Window,cx: &mut Context<Self>) -> bool {
        if !self.flush_uri_schemes(window,cx) { return false; }
        if self.preferences.files.recent_length.read(cx).has_composition() || self.preferences.files.performance_size.read(cx).has_composition() { return false; }
        let current = self.pref_int("History.customLength",-1);
        if current > 0 && self.preferences.files.recent_length_value.is_some() {
            let entity = self.preferences.files.recent_length.clone();
            let raw = entity.read(cx).text();
            let value = if raw.is_empty() { current } else { match unsigned_dialog_integer(&raw) { 0 => 100, value => value.min(259) } };
            if value != current && !self.set_preference("History.customLength",json!(value),window,cx) { return false; }
            entity.update(cx,|field,cx| field.set_text(&value.to_string(),window,cx));
        }
        let entity = self.preferences.files.performance_size.clone();
        let raw = entity.read(cx).text();
        if self.preferences.files.performance_value.is_some() && (raw.is_empty() || unsigned_dialog_integer(&raw) == 0) {
            if !self.set_preference("largeFileRestriction.fileSizeMB",json!(200),window,cx) { return false; }
            self.preferences.files.performance_value = Some(200);
            self.preferences.files.performance_display = "200".into();
            entity.update(cx,|field,cx| field.set_text("200",window,cx));
        }
        self.preferences.files.recent_length_blur = None;
        self.preferences.files.performance_changed = None;
        self.preferences.files.performance_blur = None;
        self.preferences.files.uri_field = None;
        self.preferences.files.uri_task = None;
        true
    }

    fn render_preferences_associations(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.preferences.files.association_loaded && !self.preferences.files.association_busy {
            self.preferences_refresh_associations(window,cx);
        }
        if self.preferences.files.association_custom_changed.is_none() {
            let entity = self.preferences.files.custom_extension.clone();
            self.preferences.files.association_custom_changed = Some(cx.observe_in(&entity,window,|_,field,window,cx| {
                if field.read(cx).has_composition() { return; }
                let text = field.read(cx).text();
                if text.encode_utf16().count() == 1 && text != "." {
                    field.update(cx,|field,cx| {
                        field.set_text(&format!(".{text}"),window,cx);
                        field.collapse_to_end(window,cx);
                    });
                }
                cx.notify();
            }));
        }
        let palette = self.preferences_render_palette();
        let state = &self.preferences.files;
        let ready = state.association_loaded && state.association_available && !state.association_busy; let category_focus = self.preference_control_focus("pref-association-groups",cx).tab_stop(ready); let available_focus = self.preference_control_focus("pref-association-available",cx).tab_stop(ready); let registered_focus = self.preference_control_focus("pref-association-registered",cx).tab_stop(ready);
        let mut categories = at(18.,30.,150.,153.).id("pref-association-groups").role(Role::ListBox).track_focus(&category_focus).aria_label("支持的扩展名分类").bg(rgb(palette.background)).border_1().border_color(rgb(palette.control_edge(ready))).overflow_y_scroll().track_scroll(&state.association_scroll[0]).on_key_down(cx.listener(|view,event,_,cx| view.preferences_association_key(0,event,cx)));
        for (index,(name,_)) in ASSOCIATION_GROUPS.iter().enumerate() {
            let selected = state.association_group == Some(index); let focus = category_focus.clone();
            categories = categories.child(div().id(format!("pref-association-group-{index}")).h(px(14.)).line_height(px(14.)).role(Role::ListBoxOption).aria_selected(selected).bg(rgb(if selected { palette.hot_background } else { palette.background })).text_color(rgb(palette.control_text(ready))).child(*name).on_click(cx.listener(move |view,_,window,cx| {
                if !ready { return; }
                focus.focus(window,cx); view.preferences.files.association_group = Some(index);
                view.preferences.files.selected_extension = None;
                cx.notify();
            })));
        }
        let custom = state.association_group == Some(9);
        let mut available = at(176.,30.,100.,153.).id("pref-association-available").role(Role::ListBox).track_focus(&available_focus).aria_label("可关联的扩展名").bg(rgb(palette.background)).border_1().border_color(rgb(palette.control_edge(ready))).overflow_y_scroll().track_scroll(&state.association_scroll[1]).on_key_down(cx.listener(|view,event,_,cx| view.preferences_association_key(1,event,cx)))
            .on_scroll(cx.listener(|_,_,_,cx| cx.notify()));
        let mut available_count = 0;
        if let Some((_,extensions)) = state.association_group.and_then(|index| ASSOCIATION_GROUPS.get(index)) {
            for extension in *extensions {
                if state.associations.iter().any(|entry| entry.extension == *extension && entry.is_current_application) { continue; }
                available_count += 1;
                let selected = state.selected_extension.as_deref() == Some(*extension);
                let extension = extension.to_string(); let focus = available_focus.clone();
                available = available.child(div().id(format!("pref-association-available-{extension}")).h(px(14.)).line_height(px(14.)).role(Role::ListBoxOption).aria_selected(selected).bg(rgb(if selected { palette.hot_background } else { palette.background })).text_color(rgb(palette.control_text(ready))).child(format!(".{extension}")).on_click(cx.listener(move |view,event: &gpui::ClickEvent,window,cx| {
                    if !ready { return; }
                    focus.focus(window,cx); view.preferences.files.selected_extension = Some(extension.clone());
                    if event.click_count() == 2 { view.preferences_add_association(window,cx); }
                    cx.notify();
                })));
            }
        }
        let mut registered = at(324.,30.,118.,153.).id("pref-association-registered").role(Role::ListBox).track_focus(&registered_focus).aria_label("默认由本应用打开的扩展名").bg(rgb(palette.background)).border_1().border_color(rgb(palette.control_edge(ready))).overflow_y_scroll().track_scroll(&state.association_scroll[2]).on_key_down(cx.listener(|view,event,_,cx| view.preferences_association_key(2,event,cx)))
            .on_scroll(cx.listener(|_,_,_,cx| cx.notify()));
        let mut registered_count = 0;
        for entry in state.associations.iter().filter(|entry| entry.is_current_application) {
            registered_count += 1;
            let extension = entry.extension.clone();
            let selected = state.selected_registered.as_deref() == Some(extension.as_str());
            let restorable = entry.has_backup; let focus = registered_focus.clone();
            registered = registered.child(div().id(format!("pref-association-registered-{extension}")).h(px(14.)).line_height(px(14.)).role(Role::ListBoxOption).aria_selected(selected).bg(rgb(if selected { palette.hot_background } else { palette.background })).text_color(rgb(palette.control_text(ready))).child(format!(".{extension}")).on_click(cx.listener(move |view,event: &gpui::ClickEvent,window,cx| {
                if !ready { return; }
                focus.focus(window,cx); view.preferences.files.selected_registered = Some(extension.clone());
                if restorable && event.click_count() == 2 { view.preferences_remove_association(window,cx); }
                cx.notify();
            })));
        }
        let add = ready && if custom { MacAssociations::extension(&state.custom_extension.read(cx).text()).is_ok() } else { state.selected_extension.is_some() };
        let remove = ready && state.selected_registered.as_ref().is_some_and(|extension| state.associations.iter().any(|entry| entry.extension == *extension && entry.is_current_application && entry.has_backup));
        if available_count as f32 * 14. > 153. * 1.75 - 2. { available = available.pr(horizontal_px(12.)); }
        if registered_count as f32 * 14. > 153. * 1.75 - 2. { registered = registered.pr(horizontal_px(12.)); }
        let status = state.association_status.clone();
        let custom_field = state.custom_extension.clone();
        let mut body = div().relative().w(horizontal_px(805.)).h(px(358.75)).text_size(px(12.)).line_height(px(14.)).bg(rgb(palette.pure_background)).text_color(rgb(palette.text))
            .on_mouse_move(cx.listener(|view,event,_,cx| view.preferences_association_scroll_move(event,cx)))
            .on_mouse_up(MouseButton::Left,cx.listener(|view,_,_,_| {
                for grab in &view.preferences.files.association_scroll_grab { grab.set(None); }
            }))
            .on_mouse_up_out(MouseButton::Left,cx.listener(|view,_,_,_| {
                for grab in &view.preferences.files.association_scroll_grab { grab.set(None); }
            }))
            .child(preferences_surface::pref_value_label(&status,18. * 1.75,0.,424. * 1.75))
            .child(pref_label(crate::localization::current_preference_control(4009).unwrap_or("Supported extensions:"),18. * 1.75,18. * 1.75,258. * 1.75))
            .child(pref_label(crate::localization::current_preference_control(4010).unwrap_or("Registered extensions:"),324. * 1.75,18. * 1.75,118. * 1.75))
            .child(categories).child(registered)
            .child(self.pref_button("pref-association-add","->",add,286. * 1.75,86. * 1.75,28. * 1.75,Self::preferences_add_association,cx))
            .child(self.pref_button("pref-association-remove","<-",remove,286. * 1.75,108. * 1.75,28. * 1.75,Self::preferences_remove_association,cx));
        custom_field.update(cx,|field,cx| { if field.enabled() != ready { field.set_enabled(ready,cx); } });
        if custom { body = body.child(at(176.,98.,100.,12.).child(custom_field)); }
        else {
            body = body.child(available);
            if available_count as f32 * 14. > 153. * 1.75 - 2. {
                body = body.child(self.preferences_association_scrollbar(1,176.,100.,cx));
            }
        }
        if registered_count as f32 * 14. > 153. * 1.75 - 2. {
            body = body.child(self.preferences_association_scrollbar(2,324.,118.,cx));
        }
        body.into_any_element()
    }

    fn preferences_association_scrollbar(&self,index: usize,x: f32,width: f32,cx: &mut Context<Self>) -> AnyElement {
        let palette = self.preferences_render_palette();
        let scroll = self.preferences.files.association_scroll[index].clone();
        let paint_scroll = scroll.clone();
        let grab = self.preferences.files.association_scroll_grab[index].clone();
        let height = 153. * 1.75 - 2.;
        at(x,30.,width,153.).child(div().absolute().right(horizontal_px(1.)).top(px(1.)).w(horizontal_px(12.)).h(px(height))
            .id(format!("pref-association-scrollbar-{index}")).role(Role::ScrollBar).aria_orientation(gpui::accesskit::Orientation::Vertical)
            .map(|element| preferences_editing_surface::scrollbar_accessibility(element, scroll.clone()))
            .on_mouse_down(MouseButton::Left,cx.listener(move |_,event: &gpui::MouseDownEvent,_,cx| {
                let viewport = dialog_font::logical(scroll.bounds().size.height);
                let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
                let thumb_height = (height * viewport / (viewport + maximum).max(1.)).max(18.).min(height);
                let travel = height - thumb_height;
                let position = dialog_font::logical(event.position.y - scroll.bounds().top()) - 1.;
                let offset = (-dialog_font::logical(scroll.offset().y)).clamp(0.,maximum);
                let top = if maximum > 0. { offset / maximum * travel } else { 0. };
                if position < top || position > top + thumb_height {
                    let offset = (offset + if position < top { -height } else { height }).clamp(0.,maximum);
                    scroll.set_offset(gpui::point(px(0.),px(-offset)));
                } else { grab.set(Some(position - top)); }
                cx.notify();
                cx.stop_propagation();
            }))
            .child(gpui::canvas(|_,_,_| (),move |bounds,_,window,_| {
                window.paint_quad(gpui::fill(bounds,rgb(palette.control_background(false))));
                let viewport = dialog_font::logical(paint_scroll.bounds().size.height);
                let maximum = dialog_font::logical(paint_scroll.max_offset().y).max(0.);
                let thumb_height = (height * viewport / (viewport + maximum).max(1.)).max(18.).min(height);
                let travel = height - thumb_height;
                let offset = (-dialog_font::logical(paint_scroll.offset().y)).clamp(0.,maximum);
                let top = if maximum > 0. { offset / maximum * travel } else { 0. };
                let thumb = gpui::Bounds::new(gpui::point(bounds.left() + horizontal_px(2.),bounds.top() + px(top)),gpui::size(horizontal_px(8.),px(thumb_height)));
                window.paint_quad(gpui::fill(thumb,rgb(palette.disabled_text)));
            }).size_full())).into_any_element()
    }

    fn preferences_association_scroll_move(&mut self,event: &gpui::MouseMoveEvent,cx: &mut Context<Self>) {
        for (index,grab) in self.preferences.files.association_scroll_grab.iter().enumerate() {
            if event.pressed_button != Some(MouseButton::Left) { grab.set(None); continue; }
            let Some(grab) = grab.get() else { continue; };
            let scroll = &self.preferences.files.association_scroll[index];
            let viewport = dialog_font::logical(scroll.bounds().size.height);
            let height = (viewport - 2.).max(0.);
            let maximum = dialog_font::logical(scroll.max_offset().y).max(0.);
            let thumb_height = (height * viewport / (viewport + maximum).max(1.)).max(18.).min(height);
            let travel = height - thumb_height;
            if travel > 0. {
                let position = ((dialog_font::logical(event.position.y - scroll.bounds().top()) - 1. - grab) / travel).clamp(0.,1.);
                scroll.set_offset(gpui::point(px(0.),px(-maximum * position)));
                cx.notify();
            }
        }
    }

    fn preferences_refresh_associations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(home) = self.settings_store.as_ref().and_then(|store| store.root().parent()).map(Path::to_path_buf) else {
            self.preferences.files.association_status = "无法读取应用配置目录".into();
            self.preferences.files.association_loaded = true;
            return;
        };
        self.preferences.files.association_busy = true;
        self.preferences.files.association_status = "正在查询 macOS 默认应用…".into();
        let executor = cx.background_executor().clone();
        cx.spawn_in(window,async move |view,cx| {
            let result = executor.spawn(async move { objc2::rc::autoreleasepool(|_| MacAssociations::query(&ConfigStore::under_home(&home))) }).await;
            if let Err(error) = view.update_in(cx,|view,_,cx| {
                view.preferences.files.association_busy = false;
                view.preferences.files.association_loaded = true;
                match result {
                    Ok(entries) => {
                        view.preferences.files.associations = entries;
                        view.preferences.files.association_available = true;
                        view.preferences.files.association_status = "macOS 默认应用；按内容类型关联，可能同时影响多个扩展名。".into();
                    }
                    Err(error) => {
                        view.preferences.files.associations.clear();
                        view.preferences.files.association_available = false;
                        view.preferences.files.association_status = format!("查询失败: {error}");
                        view.notify_editor_error(format!("查询文件关联失败: {error}"),cx);
                    }
                }
                cx.notify();
            }) { eprintln!("association query window unavailable: {error}"); }
        }).detach();
    }

    fn preferences_add_association(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preferences.files.association_busy { return; }
        let raw = if self.preferences.files.association_group == Some(9) {
            self.preferences.files.custom_extension.read(cx).text()
        } else {
            self.preferences.files.selected_extension.clone().unwrap_or_default()
        };
        let extension = match MacAssociations::extension(&raw) {
            Ok(extension) => extension,
            Err(error) => { self.notify_editor_error(error.to_string(),cx); return; }
        };
        self.preferences_change_association(extension,false,window,cx);
    }

    fn preferences_remove_association(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preferences.files.association_busy { return; }
        let Some(extension) = self.preferences.files.selected_registered.clone() else { return; };
        self.preferences_change_association(extension,true,window,cx);
    }

    fn preferences_change_association(&mut self, extension: String, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(home) = self.settings_store.as_ref().and_then(|store| store.root().parent()).map(Path::to_path_buf) else {
            self.notify_editor_error("无法读取应用配置目录".into(),cx);
            return;
        };
        self.preferences.files.association_busy = true;
        let executor = cx.background_executor().clone();
        cx.spawn_in(window,async move |view,cx| {
            let inspection_extension = extension.clone();
            let inspection_home = home.clone();
            let inspection = executor.spawn(async move { objc2::rc::autoreleasepool(|_| MacAssociations::inspect_with_store(&inspection_extension,&ConfigStore::under_home(&inspection_home))) }).await;
            let inspection = match inspection {
                Ok(inspection) => inspection,
                Err(error) => {
                    if let Err(update_error) = view.update_in(cx,|view,_,cx| {
                        view.preferences.files.association_busy = false;
                        view.notify_editor_error(error.to_string(),cx);
                        cx.notify();
                    }) { eprintln!("association inspection window unavailable: {update_error}"); }
                    return;
                }
            };
            let message = format!("扩展名: .{extension}\n内容类型: {}\n当前默认应用: {}\n\n本操作影响整个内容类型，不限于下列扩展名。已确认共享此类型的扩展名: {}\n此列表包含系统类型标签、本应用分类和已保存的自定义扩展名，不是全系统枚举。\n\n{}",inspection.content_type,inspection.handler.as_deref().unwrap_or("无"),inspection.extensions.iter().map(|extension| format!(".{extension}")).collect::<Vec<_>>().join("、"),if restore { "恢复此前保存的默认应用？" } else { "将 Notepad Matrix 设为默认打开应用，并保存此前的默认应用？" });
            let prompt = match view.update_in(cx,|_,window,cx| window.prompt(gpui::PromptLevel::Warning,"文件关联",Some(&message),&[gpui::PromptButton::ok("继续"),gpui::PromptButton::cancel("取消")],cx)) {
                Ok(prompt) => prompt,
                Err(error) => { eprintln!("association confirmation window unavailable: {error}"); return; }
            };
            let response = prompt.await;
            let result = match response {
                Ok(0) => {
                    let content_type = inspection.content_type.clone();
                    executor.spawn(async move { objc2::rc::autoreleasepool(|_| {
                        let store = ConfigStore::under_home(&home);
                        MacAssociations::change(&store,&extension,&content_type,restore)?;
                        MacAssociations::query(&store).map(Some)
                    }) }).await
                }
                Ok(_) => Ok(None),
                Err(error) => Err(io::Error::other(error.to_string())),
            };
            if let Err(error) = view.update_in(cx,|view,window,cx| {
                view.preferences.files.association_busy = false;
                match result {
                    Ok(Some(entries)) => {
                        view.preferences.files.associations = entries;
                        view.preferences.files.selected_extension = None;
                        view.preferences.files.selected_registered = None;
                        if !restore && view.preferences.files.association_group == Some(9) {
                            view.preferences.files.custom_extension.update(cx,|field,cx| field.set_text("",window,cx));
                        }
                    }
                    Ok(None) => {},
                    Err(error) => {
                        view.notify_editor_error(format!("修改文件关联失败: {error}"),cx);
                        view.preferences.files.association_loaded = false;
                        view.preferences_refresh_associations(window,cx);
                    }
                }
                cx.notify();
            }) { eprintln!("association change window unavailable: {error}"); }
        }).detach();
    }
}

struct AssociationInspection {
    content_type: String,
    handler: Option<String>,
    extensions: Vec<String>,
}

pub(super) struct MacAssociations;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSCopyDefaultRoleHandlerForContentType(content_type: *const std::ffi::c_void, roles: u32) -> *mut objc2_foundation::NSString;
    fn LSRegisterURL(url: *const std::ffi::c_void, update: u8) -> i32;
}

#[link(name = "UniformTypeIdentifiers", kind = "framework")]
unsafe extern "C" {}

impl MacAssociations {
    fn extension(raw: &str) -> io::Result<String> {
        let extension = raw.trim().trim_start_matches('.');
        if extension.is_empty() || extension.encode_utf16().count() > 16
            || extension.chars().any(|ch| ch.is_whitespace() || matches!(ch,'/' | '\\' | ':' | '\0' | '.')) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,"扩展名须为 1–16 个字符，不含点、路径分隔符或空白"));
        }
        Ok(extension.to_lowercase())
    }

    fn content_type(extension: &str) -> io::Result<String> {
        use objc2::{class, msg_send};
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSString;
        unsafe {
            let ty: *mut AnyObject = msg_send![class!(UTType), typeWithFilenameExtension: &*NSString::from_str(extension)];
            if ty.is_null() { return Err(io::Error::other(format!("macOS 无法解析 .{extension} 的内容类型"))); }
            let identifier: *const NSString = msg_send![ty, identifier];
            if identifier.is_null() { return Err(io::Error::other("内容类型没有标识符")); }
            Ok((&*identifier).to_string())
        }
    }

    fn default_handler(content_type: &str) -> Option<String> {
        let ty = objc2_foundation::NSString::from_str(content_type);
        unsafe {
            objc2::rc::Retained::from_raw(LSCopyDefaultRoleHandlerForContentType((&*ty as *const objc2_foundation::NSString).cast(),u32::MAX)).map(|handler| handler.to_string())
        }
    }

    fn current_application() -> io::Result<String> {
        use objc2::{class, msg_send};
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSString;
        unsafe {
            let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
            if bundle.is_null() { return Err(io::Error::other("当前进程没有应用包")); }
            let identifier: *const NSString = msg_send![bundle, bundleIdentifier];
            if identifier.is_null() { return Err(io::Error::other("文件关联需要带有 CFBundleIdentifier 的 .app 应用包")); }
            Ok((&*identifier).to_string())
        }
    }

    fn journal(store: &ConfigStore) -> io::Result<Value> {
        match store.load_json(Path::new("file-associations.json")) {
            Ok(journal) if journal["handlers"].is_object() => Ok(journal),
            Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData,"file-associations.json handlers must be an object")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({"schema_version":1,"handlers":{}})),
            Err(error) => Err(error),
        }
    }

    fn inspect(extension: &str) -> io::Result<AssociationInspection> {
        use objc2::{class, msg_send};
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSString;
        let content_type = Self::content_type(extension)?;
        let mut extensions = vec![extension.to_owned()];
        unsafe {
            let ty: *mut AnyObject = msg_send![class!(UTType), typeWithIdentifier: &*NSString::from_str(&content_type)];
            if !ty.is_null() {
                let tags: *mut AnyObject = msg_send![ty, tags];
                if !tags.is_null() {
                    let filenames: *mut AnyObject = msg_send![tags, objectForKey: &*NSString::from_str("public.filename-extension")];
                    if !filenames.is_null() {
                        let count: usize = msg_send![filenames, count];
                        for index in 0..count {
                            let tag: *const NSString = msg_send![filenames, objectAtIndex: index];
                            if !tag.is_null() { extensions.push((&*tag).to_string()); }
                        }
                    }
                }
            }
        }
        for (_,candidates) in ASSOCIATION_GROUPS {
            for candidate in *candidates {
                if Self::content_type(candidate)? == content_type { extensions.push((*candidate).to_owned()); }
            }
        }
        extensions.sort();
        extensions.dedup();
        Ok(AssociationInspection { handler: Self::default_handler(&content_type),content_type,extensions })
    }

    fn inspect_with_store(extension: &str,store: &ConfigStore) -> io::Result<AssociationInspection> {
        let mut inspection = Self::inspect(extension)?;
        let journal = Self::journal(store)?;
        if let Some(handlers) = journal["handlers"].as_object() {
            for record in handlers.values() {
                if let Some(extensions) = record["extensions"].as_array() {
                    for extension in extensions {
                        let Some(extension) = extension.as_str() else { return Err(io::Error::other("自定义关联扩展名记录无效")); };
                        if Self::content_type(extension)? == inspection.content_type { inspection.extensions.push(extension.to_owned()); }
                    }
                }
            }
        }
        inspection.extensions.sort();
        inspection.extensions.dedup();
        Ok(inspection)
    }

    pub(super) fn query(store: &ConfigStore) -> io::Result<Vec<Association>> {
        let application = Self::current_application()?;
        let journal = Self::journal(store)?;
        let mut extensions: Vec<String> = ASSOCIATION_GROUPS.iter().flat_map(|(_,extensions)| extensions.iter().map(|extension| (*extension).to_owned())).collect();
        let Some(handlers) = journal["handlers"].as_object() else { return Err(io::Error::other("关联备份无效")); };
        for backup in handlers.values() {
            if let Some(custom) = backup["extensions"].as_array() {
                for extension in custom {
                    let Some(extension) = extension.as_str() else { return Err(io::Error::other("关联备份扩展名无效")); };
                    extensions.push(Self::extension(extension)?);
                }
            }
        }
        extensions.sort();
        extensions.dedup();
        extensions.into_iter().map(|extension| {
            let content_type = Self::content_type(&extension)?;
            let handler = Self::default_handler(&content_type);
            let is_current_application = handler.as_ref().is_some_and(|handler| handler.eq_ignore_ascii_case(&application));
            let has_backup = handlers.get(&content_type).is_some_and(|record| record["previous_application"].as_str().is_some_and(|id| !id.is_empty()));
            Ok(Association { extension,is_current_application,has_backup })
        }).collect()
    }

    fn set_handler(content_type: &str, application: &str) -> io::Result<()> {
        use objc2::{class,msg_send};
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSString;
        let app = NSString::from_str(application);
        unsafe {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let url: *mut AnyObject = msg_send![workspace, URLForApplicationWithBundleIdentifier: &*app];
            if url.is_null() { return Err(io::Error::other(format!("默认应用 {application} 已卸载或未注册"))); }
            let ty: *mut AnyObject = msg_send![class!(UTType), typeWithIdentifier: &*NSString::from_str(content_type)];
            if ty.is_null() { return Err(io::Error::other("内容类型已不可用")); }
            let (sender,receiver) = std::sync::mpsc::channel::<Result<(),String>>();
            let completion = block2::RcBlock::new(move |error: *mut AnyObject| {
                let result = if error.is_null() { Ok(()) } else {
                    let description: *const NSString = msg_send![error, localizedDescription];
                    Err(if description.is_null() { "macOS 未返回错误说明".into() } else { (&*description).to_string() })
                };
                if let Err(error) = sender.send(result) { eprintln!("association completion receiver unavailable: {error}"); }
            });
            let _: () = msg_send![workspace, setDefaultApplicationAtURL: url, toOpenContentType: ty, completionHandler: &*completion];
            receiver.recv_timeout(std::time::Duration::from_secs(30)).map_err(|error| io::Error::other(format!("默认应用设置未完成，旧备份已保留: {error}")))?.map_err(io::Error::other)?;
        }
        if !Self::default_handler(content_type).is_some_and(|handler| handler.eq_ignore_ascii_case(application)) {
            return Err(io::Error::other("macOS 尚未确认新的默认应用；旧应用备份已保留"));
        }
        Ok(())
    }

    pub(super) fn change(store: &ConfigStore, extension: &str, content_type: &str, restore: bool) -> io::Result<()> {
        use objc2::{class,msg_send};
        use objc2::runtime::AnyObject;
        let application = Self::current_application()?;
        let journal = Self::journal(store)?;
        if Self::content_type(extension)? != content_type {
            return Err(io::Error::other("扩展名的内容类型已变化，请重新查询后操作"));
        }
        let current = Self::default_handler(content_type);
        if restore {
            if !current.as_ref().is_some_and(|handler| handler.eq_ignore_ascii_case(&application)) {
                return Err(io::Error::other("默认应用已被其他操作更改，不覆盖该更改"));
            }
            let previous = journal["handlers"][content_type]["previous_application"].as_str().ok_or_else(|| io::Error::other("没有可恢复的旧默认应用备份"))?;
            Self::set_handler(content_type,previous)?;
            store.update_json(Path::new("file-associations.json"),&json!({"schema_version":1,"handlers":{}}),|journal| {
                journal["handlers"].as_object_mut().ok_or_else(|| io::Error::other("关联备份无效"))?.remove(content_type);
                Ok(())
            })?;
            return Ok(());
        }
        if current.as_ref().is_some_and(|handler| handler.eq_ignore_ascii_case(&application)) {
            return Err(io::Error::other("此内容类型已经由本应用默认打开"));
        }
        let previous = current;
        let inspection = Self::inspect_with_store(extension,store)?;
        store.update_json(Path::new("file-associations.json"),&json!({"schema_version":1,"handlers":{}}),|journal| {
            if !journal["handlers"].is_object() { return Err(io::Error::other("关联备份无效")); }
            journal["handlers"][content_type] = json!({"previous_application":previous,"extensions":inspection.extensions,"application":application});
            Ok(())
        })?;
        unsafe {
            let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
            let url: *mut AnyObject = msg_send![bundle, bundleURL];
            if url.is_null() { return Err(io::Error::other("应用包缺少 URL")); }
            let status = LSRegisterURL(url.cast(),1);
            if status != 0 { return Err(io::Error::other(format!("应用包注册失败，OSStatus {status}；旧默认应用备份已保留"))); }
        }
        if Self::content_type(extension)? != content_type { return Err(io::Error::other("应用登记后此扩展名的内容类型发生变化，请重新查询后确认")); }
        let observed = Self::default_handler(content_type);
        if observed != previous && !observed.as_ref().is_some_and(|handler| handler.eq_ignore_ascii_case(&application)) { return Err(io::Error::other("默认应用已被其他操作更改，没有覆盖新默认应用")); }
        Self::set_handler(content_type,&application)
    }
}

pub(super) fn files_changed(path: &str, settings: &mut Value) -> io::Result<()> {
    normalize_files_settings(settings)?;
    match path {
        "remember_last_session" if settings[path].as_bool() == Some(false) => {
            settings["snapshot_mode"] = false.into();
        }
        "snapshot_mode" if settings[path].as_bool() == Some(true)
            && settings["remember_last_session"].as_bool() == Some(false) => {
            settings["snapshot_mode"] = false.into();
        }
        "document_switcher" if settings[path].as_bool() == Some(false) => {
            settings["mru_switching"] = false.into();
        }
        "mru_switching" if settings[path].as_bool() == Some(true)
            && settings["document_switcher"].as_bool() == Some(false) => {
            settings["mru_switching"] = false.into();
        }
        "SaveAllConfirm" => settings["save_all_confirm"] = settings[path].clone(),
        "save_all_confirm" => settings["SaveAllConfirm"] = settings[path].clone(),
        _ => {}
    }
    Ok(())
}

pub(super) fn normalize_files_settings(settings: &mut Value) -> io::Result<()> {
    for (path,default) in [("backup_retention.enabled",json!(true)),("backup_retention.days",json!(90))] {
        if super::preferences_surface::value_at(settings,path).is_null() {
            super::preferences_surface::write_value(settings,path,default)?;
        }
    }
    for (new,old) in [
        ("SaveAllConfirm","save_all_confirm"),
        ("KeepSessionAbsentFileEntries","keepSessionAbsentFileEntries"),
    ] {
        if settings[new].is_null() && !settings[old].is_null() { settings[new] = settings[old].clone(); }
    }
    if let Some(old) = settings.get("large_file_restriction").cloned() {
        for (new,old_key) in [("isEnabled","enabled"),("fileSizeMB","file_size_mb"),("allowAutoCompletion","allow_auto_completion")] {
            if settings["largeFileRestriction"][new].is_null() && !old[old_key].is_null() {
                super::preferences_surface::write_value(settings,&format!("largeFileRestriction.{new}"),old[old_key].clone())?;
            }
        }
    }
    Ok(())
}

pub(super) struct State {
    pub(super) association_group: Option<usize>,
    pub(super) selected_extension: Option<String>,
    pub(super) selected_registered: Option<String>,
    pub(super) custom_extension: Entity<text_field::TextField>,
    association_custom_changed: Option<gpui::Subscription>,
    pub(super) associations: Vec<Association>,
    pub(super) association_loaded: bool,
    association_available: bool,
    pub(super) association_busy: bool,
    pub(super) association_status: String,
    association_scroll: [gpui::ScrollHandle; 3],
    association_scroll_grab: [std::rc::Rc<std::cell::Cell<Option<f32>>>; 3],
    detection_silent: Option<bool>,
    detection_end: Option<bool>,
    pub(super) recovery_task: Option<gpui::Task<()>>,
    recovery_options: Option<crate::recovery::RecoveryOptions>,
    pub(super) placeholder_decision_pending: bool,
    pub(super) loaded_session_path: Option<PathBuf>,
    pub(super) launch_no_session: bool,
    pub(super) startup_multi_instance: i64,
    pub(super) startup_finished: bool,
    pub(super) multi_instance: i64,
    pub(super) auto_changes: std::collections::VecDeque<crate::monitoring::AutoChange>,
    pub(super) auto_prompt_pending: bool,
    pub(super) auto_reload_follow: Option<(crate::workspace::BufferId,Option<std::time::SystemTime>)>,
    pub(super) links: crate::links::LinkOptions,
    pub(super) file_options: FileOptions,
    pub(super) follow_links: bool,
    pub(super) symlink_warning_shown: bool,
    pub(super) applied: bool,
    snapshot_ui: Option<bool>,
    recent_length: Entity<text_field::TextField>,
    recent_length_value: Option<i64>,
    recent_length_blur: Option<gpui::Subscription>,
    performance_size: Entity<text_field::TextField>,
    performance_value: Option<i64>,
    performance_display: String,
    performance_changed: Option<gpui::Subscription>,
    performance_blur: Option<gpui::Subscription>,
    uri_field: Option<NativeUriField>,
    uri_value: Option<String>,
    uri_task: Option<gpui::Task<()>>,
    backup_path: Entity<text_field::TextField>,
    backup_path_value: String,
}

impl State {
    pub(super) fn new(cx: &mut Context<MatrixWindow>) -> Self {
        Self {
            association_group: None,
            selected_extension: None,
            selected_registered: None,
            custom_extension: cx.new(|cx| text_field::TextField::compact("自定义扩展名", "preferences-association-custom", cx).without_history_arrow().with_max_utf16(17)),
            association_custom_changed: None,
            associations: Vec::new(),
            association_loaded: false,
            association_available: false,
            association_busy: false,
            association_status: String::new(),
            association_scroll: std::array::from_fn(|_| gpui::ScrollHandle::new()),
            association_scroll_grab: std::array::from_fn(|_| std::rc::Rc::new(std::cell::Cell::new(None))),
            detection_silent: None,
            detection_end: None,
            recovery_task: None,
            recovery_options: None,
            placeholder_decision_pending: false,
            loaded_session_path: None,
            launch_no_session: false,
            startup_multi_instance: 0,
            startup_finished: false,
            multi_instance: 0,
            auto_changes: std::collections::VecDeque::new(),
            auto_prompt_pending: false,
            auto_reload_follow: None,
            links: crate::links::LinkOptions::default(),
            file_options: FileOptions::default(),
            follow_links: false,
            symlink_warning_shown: false,
            applied: false,
            snapshot_ui: None,
            recent_length: cx.new(|cx| text_field::TextField::compact("最近文件最大长度","pref-history-custom-length",cx).without_history_arrow()),
            recent_length_value: None,
            recent_length_blur: None,
            performance_size: cx.new(|cx| text_field::TextField::compact("大文件大小 MB","pref-large-file-size",cx).without_history_arrow()),
            performance_value: None,
            performance_display: String::new(),
            performance_changed: None,
            performance_blur: None,
            uri_field: None,
            uri_value: None,
            uri_task: None,
            backup_path: cx.new(|cx| text_field::TextField::compact("备份路径","pref-backup-root-path",cx).without_history_arrow().with_read_only()),
            backup_path_value: String::new(),
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct FileOptions {
    pub(super) open_dropped_folder_files: bool,
}

fn expand_directory(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            result.push_str(&rest[start..]);
            return result;
        };
        match std::env::var(&after[..end]) {
            Ok(value) => result.push_str(&value),
            Err(_) => result.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    result
}

impl MatrixWindow {
    fn flush_uri_schemes(&mut self,window: &mut Window,cx: &mut Context<Self>) -> bool {
        let Some(field) = &self.preferences.files.uri_field else { return true; };
        let Some(value) = field.value() else { return false; };
        let visible = utf16_prefix(&value,2048);
        if visible != value { field.set_value(&visible); }
        let value = utf16_prefix(&visible,2047);
        if self.preferences.files.uri_value.as_deref() != Some(value.as_str()) {
            if !self.set_preference("uriCustomizedSchemes",json!(value),window,cx) { return false; }
            self.preferences.files.uri_value = Some(value);
        }
        true
    }

    fn preferences_uri_schemes(&mut self,enabled: bool,window: &mut Window,cx: &mut Context<Self>) -> AnyElement {
        let value = self.pref_string("uriCustomizedSchemes",URI_SCHEMES);
        if self.preferences.files.uri_field.is_none() {
            match NativeUriField::new(&value) {
                Ok(field) => self.preferences.files.uri_field = Some(field),
                Err(error) => {
                    self.notify_editor_error(format!("URI 输入框无法创建: {error}"),cx);
                    let palette = self.preferences_render_palette();
                    return at(26.,91.,408.,70.).text_color(rgb(palette.disabled_text)).child(error.to_string()).into_any_element();
                }
            }
            self.preferences.files.uri_value = Some(value.clone());
        }
        if self.preferences.files.uri_value.as_deref() != Some(value.as_str()) {
            if let Some(field) = &self.preferences.files.uri_field { field.set_value(&value); }
            self.preferences.files.uri_value = Some(value);
        }
        if self.preferences.files.uri_task.is_none() {
            let executor = cx.background_executor().clone();
            self.preferences.files.uri_task = Some(cx.spawn_in(window,async move |view,cx| {
                loop {
                    executor.timer(std::time::Duration::from_millis(100)).await;
                    match view.update_in(cx,|view,window,cx| {
                        if !view.preferences_visible || view.preferences.page != Page::CloudLinks { return false; }
                        view.flush_uri_schemes(window,cx);
                        true
                    }) {
                        Ok(true) => {},
                        Ok(false) | Err(_) => break,
                    }
                }
            }));
        }
        let Some(field) = &self.preferences.files.uri_field else { return at(26.,91.,408.,70.).into_any_element(); };
        let palette = self.preferences_render_palette();
        field.set_enabled(enabled,palette);
        at(26.,91.,408.,70.).border_1().border_color(rgb(palette.control_edge(enabled))).bg(rgb(if enabled { palette.background } else { palette.pure_background })).child(field.element()).into_any_element()
    }

    fn preferences_recent_length(&mut self,value: i64,x: f32,window: &mut Window,cx: &mut Context<Self>) -> AnyElement {
        if self.preferences.files.recent_length_blur.is_none() {
            let focus = self.preferences.files.recent_length.read(cx).focus_handle();
            self.preferences.files.recent_length_blur = Some(cx.on_blur(&focus,window,|view,window,cx| {
                let current = view.pref_int("History.customLength",-1);
                if current <= 0 { return; }
                let entity = view.preferences.files.recent_length.clone();
                let raw = entity.read(cx).text();
                let value = if raw.is_empty() { current } else { match unsigned_dialog_integer(&raw) { 0 => 100, value => value.min(259) } };
                let value = if view.set_preference("History.customLength",json!(value),window,cx) { value } else { current };
                view.preferences.files.recent_length_value = Some(value);
                entity.update(cx,|field,cx| field.set_text(&value.to_string(),window,cx));
            }));
        }
        let entity = self.preferences.files.recent_length.clone();
        super::preferences_surface::sync_text_field_palette(&entity,self.preferences_render_palette(),cx);
        if self.preferences.files.recent_length_value != Some(value) {
            self.preferences.files.recent_length_value = Some(value);
            entity.update(cx,|field,cx| field.set_text(&value.max(0).to_string(),window,cx));
        }
        entity.update(cx,|field,cx| { if field.enabled() != (value > 0) { field.set_enabled(value > 0,cx); } });
        at(x,137.,32.,12.).child(entity).into_any_element()
    }

    fn preferences_performance_size(&mut self,enabled: bool,window: &mut Window,cx: &mut Context<Self>) -> AnyElement {
        if self.preferences.files.performance_changed.is_none() {
            let entity = self.preferences.files.performance_size.clone();
            self.preferences.files.performance_changed = Some(cx.observe_in(&entity,window,|view,entity,window,cx| {
                if entity.read(cx).has_composition() { return; }
                let text = entity.read(cx).text();
                if text == view.preferences.files.performance_display { return; }
                view.preferences.files.performance_display = text.clone();
                if !text.is_empty() {
                    let value = unsigned_dialog_integer(&text);
                    let value = value.min(2046);
                    view.preferences.files.performance_value = Some(value);
                    view.set_preference("largeFileRestriction.fileSizeMB",json!(value),window,cx);
                    if unsigned_dialog_integer(&text) > 2046 {
                        view.preferences.files.performance_display = value.to_string();
                        entity.update(cx,|field,cx| field.set_text(&value.to_string(),window,cx));
                    }
                }
            }));
            let focus = entity.read(cx).focus_handle();
            self.preferences.files.performance_blur = Some(cx.on_blur(&focus,window,|view,window,cx| {
                let entity = view.preferences.files.performance_size.clone();
                let text = entity.read(cx).text();
                if text.is_empty() || unsigned_dialog_integer(&text) == 0 {
                    if !view.set_preference("largeFileRestriction.fileSizeMB",json!(200),window,cx) { return; }
                    view.preferences.files.performance_value = Some(200);
                    view.preferences.files.performance_display = "200".into();
                    entity.update(cx,|field,cx| field.set_text("200",window,cx));
                } else if text.parse::<i64>().is_err() {
                    let value = view.pref_int("largeFileRestriction.fileSizeMB",200).to_string();
                    view.preferences.files.performance_display = value.clone();
                    entity.update(cx,|field,cx| field.set_text(&value,window,cx));
                }
            }));
        }
        let value = self.pref_int("largeFileRestriction.fileSizeMB",200);
        let entity = self.preferences.files.performance_size.clone();
        super::preferences_surface::sync_text_field_palette(&entity,self.preferences_render_palette(),cx);
        if self.preferences.files.performance_value != Some(value) {
            self.preferences.files.performance_value = Some(value);
            self.preferences.files.performance_display = value.to_string();
            entity.update(cx,|field,cx| field.set_text(&value.to_string(),window,cx));
        }
        entity.update(cx,|field,cx| { if field.enabled() != enabled { field.set_enabled(enabled,cx); } });
        at(201.,57.,24.,12.).child(entity).into_any_element()
    }

    fn preferences_backup_path(&mut self,enabled: bool,window: &mut Window,cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings_store.as_ref().map(|store| store.root().join("backup").display().to_string()).unwrap_or_default();
        let value = match crate::config_directory::snapshot() {
            Ok(snapshot) => snapshot.pending_root().join("backup").display().to_string(),
            Err(error) => {
                let message = crate::localization::Text::resource("ConfigDirectoryBackupReadFailed", "Could not read the backup directory: $ERROR$")
                    .replace("$ERROR$", crate::config_directory::error_text(&error, &self.native_locale()));
                self.notify_localized_editor_error(message,cx);
                current.clone()
            }
        };
        let details = if value == current { value.clone() } else {
            crate::localization::Text::resource("ConfigDirectoryBackupRestartDetails", "Current backup: $CURRENT$\nBackup after restart: $NEXT$\nRestart required. Existing backups will not be moved.")
                .replace("$CURRENT$", current).replace("$NEXT$", value.clone()).render(&self.native_locale())
        };
        let dark = self.dark;
        let entity = self.preferences.files.backup_path.clone();
        super::preferences_surface::sync_text_field_palette(&entity,self.preferences_render_palette(),cx);
        if self.preferences.files.backup_path_value != value {
            self.preferences.files.backup_path_value = value.clone();
            entity.update(cx,|field,cx| field.set_text(&value,window,cx));
        }
        entity.update(cx,|field,cx| { if field.enabled() != enabled { field.set_enabled(enabled,cx); } });
        at(92.,65.,342.,12.).id("pref-backup-root")
            .tooltip(move |_,cx| icon_tooltip::view(details.clone(),dark,cx)).child(entity).into_any_element()
    }

    pub(super) fn apply_file_preferences(&mut self,settings: &Value,window: &mut Window,cx: &mut Context<Self>) {
        let mut normalized = settings.clone();
        if let Err(error) = normalize_files_settings(&mut normalized) { self.notify_editor_error(error.to_string(),cx); return; }
        let settings = &normalized;
        self.preferences.settings = normalized.clone();
        let result = (|| -> io::Result<()> {
            self.workspace.configure_recent_files(settings).map_err(|error| io::Error::other(error.to_string()))?;
            self.workspace.configure_new_document(settings)?;
            self.workspace.synchronize_udl_names(self.udl.registry.named().map(|(id,definition)| (id,definition.name.clone())));
            match self.editing_new_document_language() {
                Ok(language) => self.workspace.set_new_document_language(language),
                Err(error) => self.notify_editor_error(format!("新文档语言不可用: {error}"),cx),
            }
            self.workspace.set_large_file_restriction(crate::workspace::LargeFileRestriction::from_settings(settings)?);
            self.preferences.files.links = crate::links::LinkOptions::from_settings(settings)?;
            self.preferences.files.file_options.open_dropped_folder_files = settings["isFolderDroppedOpenFiles"].as_bool().unwrap_or(false);
            self.configure_file_detection(settings,window,cx)?;
            self.configure_folder_file_preferences(settings,window,cx);
            self.preferences.files.multi_instance = settings["multiInst"]["setting"].as_i64().filter(|mode| (0..=2).contains(mode)).unwrap_or(0);
            self.apply_file_menu_preferences(settings);
            self.update_file_prompt_directory();
            self.document_switcher = settings["document_switcher"].as_bool().unwrap_or(true);
            self.mru_switching = self.document_switcher && settings["mru_switching"].as_bool().unwrap_or(true);
            if !self.document_switcher { self.workspace.cancel_mru_cycle(); }
            self.mute_sounds = settings["muteSounds"].as_bool().unwrap_or(false);
            self.save_all_confirm = settings["SaveAllConfirm"].as_bool().or_else(|| settings["save_all_confirm"].as_bool()).unwrap_or(true);
            if settings["remember_last_session"].as_bool() == Some(false) || settings["snapshot_mode"].as_bool() == Some(false) { self.preferences.files.snapshot_ui = Some(false); }
            self.set_tab_preview_enabled(settings["docPeekOnTab"].as_bool().unwrap_or(false),cx);
            let mut options = crate::recovery::RecoveryOptions::from_settings(settings)?;
            if self.preferences.files.launch_no_session { options.enabled = false; options.remember_last_session = false; }
            let previous = self.preferences.files.recovery_options;
            let restart = previous.is_none_or(|previous| previous.enabled != options.enabled || previous.interval != options.interval);
            if self.recovery.is_none() && options.remember_last_session {
                if let Some(home) = self.settings_store.as_ref().and_then(|store| store.root().parent()) {
                    self.recovery = Some(crate::recovery::RecoveryManager::create_for_workspace(home,options)?);
                }
            }
            if let Some(recovery) = &mut self.recovery { recovery.apply_options(options); }
            if restart {
                self.preferences.files.recovery_task = None;
                if options.enabled { self.start_recovery_timer(options.interval,cx); }
            }
            self.preferences.files.recovery_options = Some(options);
            self.preferences_apply_large_file_open_policy(window,cx);
            self.configure_backup_retention(settings,cx)?;
            Ok(())
        })();
        if let Err(error) = result { self.notify_editor_error(format!("文件首选项无法应用: {error}"),cx); }
        self.preferences.files.applied = true;
        cx.notify();
    }

    pub(super) fn startup_file_preferences(&mut self,settings: &Value,launch: &crate::launch::LaunchOptions,window: &mut Window,cx: &mut Context<Self>) {
        self.preferences.files.launch_no_session = launch.no_session;
        self.preferences.files.startup_multi_instance = settings["multiInst"]["setting"].as_i64().filter(|mode| (0..=2).contains(mode)).unwrap_or(0);
        self.apply_file_preferences(settings,window,cx);
        if let Err(error) = self.workspace.apply_initial_new_document_defaults() { self.notify_editor_error(format!("初始新文档无法配置: {error}"),cx); }
        self.workspace.validate_recent_files_at_startup(settings);
        if launch.no_session || launch.multi_instance {
            if let Some(state) = settings["multiInst"].get("panelState") {
                if let Err(error) = self.restore_panel_keep_state(state,settings,window,cx) { self.notify_editor_error(format!("面板状态恢复失败: {error}"),cx); }
            }
        }
        if launch.open_session && self.preferences.files.startup_multi_instance == 1 {
            self.preferences.files.loaded_session_path = launch.files.first().cloned();
        }
        self.confirm_absent_session_files(launch.open_session,window,cx);
    }

    pub(super) fn finish_file_preferences_startup(&mut self,settings: &Value,window: &mut Window,cx: &mut Context<Self>) {
        if self.preferences.files.startup_finished { return; }
        self.preferences.files.startup_finished = true;
        if settings["NewDocDefaultSettings"]["addNewDocumentOnStartup"].as_bool().unwrap_or(false) && settings["remember_last_session"].as_bool().unwrap_or(true) {
            match self.workspace.new_buffer() {
                Ok(_) => self.reset_active_view(window,cx),
                Err(error) => self.notify_editor_error(format!("启动新文档无法创建: {error}"),cx),
            }
        }
    }

    pub(super) fn preferences_apply_large_file_open_policy(&mut self,window: &mut Window,cx: &mut Context<Self>) {
        if self.workspace.take_large_file_word_wrap_disable() {
            let focused = self.pane;
            for pane in [crate::workspace::ViewId::Primary,crate::workspace::ViewId::Secondary] {
                if self.workspace.active_id_in_view(pane).is_some() && self.scope_editor_pane(pane) && self.editor_word_wrap_enabled() { self.toggle_editor_word_wrap(window,cx); }
            }
            self.scope_editor_pane(focused);
            self.workspace.disable_word_wrap_in_views();
        }
    }

    pub(super) fn file_preferences_runtime_busy(&self) -> bool {
        self.file_busy || self.close_dialog || self.save_all_dialog || self.save_all_restore.is_some() || self.reload_target.is_some() || self.reload_dialog || self.preferences.files.placeholder_decision_pending || self.preferences.files.auto_prompt_pending || self.batch_editing_blocked() || self.macro_interaction_blocked()
    }

    pub(super) fn open_native_file_paths(&mut self,paths: Vec<PathBuf>,during_launch: bool,window: &mut Window,cx: &mut Context<Self>) {
        if during_launch || self.preferences.files.multi_instance != 2 { self.start_open_paths(paths,window,cx); return; }
        let options = crate::launch::LaunchOptions { files:paths,multi_instance:true,no_session:true,..Default::default() };
        self.spawn_launch_options(options, window, cx);
    }
}

#[derive(Clone)]
pub(super) struct Association {
    pub(super) extension: String,
    pub(super) is_current_application: bool,
    pub(super) has_backup: bool,
}

fn at(x: f32, y: f32, w: f32, h: f32) -> gpui::Div {
    preferences_surface::source_rect(x * 1.75, y * 1.75, w * 1.75, h * 1.75)
}

impl MatrixWindow {
    pub(super) fn render_preferences_files(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if page != Page::CloudLinks && self.preferences.files.uri_field.is_some() {
            self.flush_uri_schemes(window,cx);
            self.preferences.files.uri_field = None;
            self.preferences.files.uri_task = None;
        }
        let palette = self.preferences_render_palette();
        let mut body = div().relative().w(horizontal_px(805.)).h(px(358.75)).text_size(px(12.)).line_height(px(14.)).bg(rgb(palette.pure_background)).text_color(rgb(palette.text));
        macro_rules! label {
            ($s:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(pref_label($s, $x * 1.75, $y * 1.75, $w * 1.75)); };
        }
        macro_rules! label_enabled {
            ($s:expr,$e:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(preferences_surface::pref_label_colour($s,$x * 1.75,$y * 1.75,$w * 1.75,palette.control_text($e))); };
        }
        macro_rules! label_right {
            ($id:expr,$s:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(pref_label_right(crate::localization::current_preference_control($id).unwrap_or($s),$x * 1.75,$y * 1.75,$w * 1.75)); };
        }
        macro_rules! label_right_enabled {
            ($id:expr,$s:expr,$e:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(at($x,$y,$w,8.).text_right().when(crate::localization::current_preference_rtl(),|element| element.text_left()).text_color(rgb(palette.control_text($e))).child(crate::localization::current_preference_control($id).unwrap_or($s))); };
        }
        macro_rules! group {
            ($s:expr,$x:expr,$y:expr,$w:expr,$h:expr) => { body = body.child(pref_group($s, $x * 1.75, $y * 1.75, $w * 1.75, $h * 1.75)); };
        }
        macro_rules! check {
            ($id:expr,$s:expr,$p:expr,$d:expr,$i:expr,$e:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(self.pref_check($id,$s,$p,$d,$i,$e,$x * 1.75,$y * 1.75,$w * 1.75,cx)); };
        }
        macro_rules! radio {
            ($id:expr,$s:expr,$p:expr,$d:expr,$v:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(self.pref_radio($id,$s,$p,json!($d),json!($v),true,$x * 1.75,$y * 1.75,$w * 1.75,cx)); };
        }
        macro_rules! input {
            ($p:expr,$e:expr,$x:expr,$y:expr,$w:expr) => { body = body.child(self.pref_input($p,$x * 1.75,$y * 1.75,$w * 1.75,$e,cx)); };
        }
        macro_rules! button {
            ($id:expr,$s:expr,$e:expr,$x:expr,$y:expr,$w:expr,$cb:expr) => { body = body.child(self.pref_button($id,$s,$e,$x * 1.75,$y * 1.75,$w * 1.75,$cb,cx)); };
        }
        match page {
            Page::Directories => {
                group!("默认打开/保存目录",18.,12.,424.,105.);
                radio!("pref-dir-current","跟随当前文件","openSaveDir.value",0,0,26.,34.,408.);
                radio!("pref-dir-last","记住上次使用的目录","openSaveDir.value",0,1,26.,54.,408.);
                radio!("pref-dir-custom","","openSaveDir.value",0,2,26.,76.,11.);
                let custom = self.pref_int("openSaveDir.value",0) == 2;
                input!("openSaveDir.defaultDirPath",custom,42.,75.,370.);
                button!("pref-dir-browse","...",custom,420.,74.,16.,Self::preferences_browse_default_directory);
                check!("pref-folder-drop","拖放文件夹时打开其中所有文件，而非文件夹工作区","isFolderDroppedOpenFiles",false,false,true,18.,136.,424.);
            }
            Page::RecentFiles => {
                group!("最近文件记录",18.,12.,424.,158.);
                check!("pref-history-validate","启动时不检查文件","validate_recent_files_at_startup",false,true,true,26.,29.,408.);
                label_right!(6306,"Max. number of entries:",26.,51.,154.);
                input!("recent_files_limit",true,186.,49.,28.);
                label!(crate::localization::current_preference_control(6462).unwrap_or("(0 – 30)"),224.,51.,100.);
                group!("显示",26.,72.,408.,88.);
                check!("pref-history-submenu","在子菜单中","History.inSubMenu",false,false,true,34.,87.,392.);
                radio!("pref-history-name","仅文件名","History.customLength",-1,0,34.,104.,392.);
                radio!("pref-history-path","完整文件路径","History.customLength",-1,-1,34.,121.,392.);
                let length = self.pref_int("History.customLength",-1);
                let length_label = crate::localization::current_preference_source_label("pref-history-length","自定义最大长度:");
                let length_label_width = dialog_font::logical(window.text_system().shape_line(length_label.to_owned().into(),px(12.),&[gpui::TextRun {
                    len: length_label.len(),font: gpui::font(dialog_font::font_family()),color: rgb(palette.text).into(),background_color: None,underline: None,strikethrough: None,
                }],None).width()).ceil();
                let length_radio_width = (16. + length_label_width) / 1.75;
                let length_input_x = 34. + length_radio_width + 6.;
                radio!("pref-history-length","自定义最大长度:","History.customLength",-1,if length > 0 { length } else { 100 },34.,138.,length_radio_width);
                body = body.child(self.preferences_recent_length(length,length_input_x,window,cx));
                label_enabled!(crate::localization::current_preference_control(6464).unwrap_or("(1 – 259)"),length > 0,length_input_x + 40.,139.,150.);
            }
            Page::Associations => return self.render_preferences_associations(window,cx),
            Page::Backup => {
                group!("会话快照与定时备份",18.,4.,424.,96.);
                let remember = self.pref_bool("remember_last_session",true);
                let snapshot = remember && self.preferences.files.snapshot_ui.unwrap_or(self.pref_bool("snapshot_mode",true) && !self.preferences.files.launch_no_session);
                self.preferences.files.snapshot_ui = Some(snapshot);
                check!("pref-backup-session","记住当前会话供下次启动使用","remember_last_session",true,false,true,26.,17.,408.);
                body = body.child(self.preferences_snapshot_check(snapshot,remember,cx));
                label_right_enabled!(6819,"Trigger backup on modification in every",snapshot,26.,49.,236.);
                input!("snapshot_backup_ms",snapshot,268.,47.,30.);
                label_enabled!(crate::localization::current_preference_control(6821).unwrap_or("seconds"),snapshot,306.,49.,120.);
                label_right_enabled!(6822,"Backup path:",snapshot,26.,67.,60.);
                body = body.child(self.preferences_backup_path(snapshot,window,cx));
                check!("pref-backup-absent","记住上次会话中无法访问的文件","KeepSessionAbsentFileEntries",false,false,true,26.,83.,408.);
                group!("保存时备份",18.,106.,208.,95.);
                radio!("pref-backup-none","无","save_backup","none","none",26.,123.,70.);
                radio!("pref-backup-simple","简单备份","save_backup","none","simple",102.,123.,116.);
                radio!("pref-backup-verbose","详细备份","save_backup","none","verbose",26.,138.,192.);
                let enabled = self.pref_string("save_backup","none") != "none";
                let custom = enabled && self.pref_bool("backup_use_directory",false);
                group!("",26.,166.,192.,27.);
                check!("pref-backup-directory","自定义备份目录","backup_use_directory",false,false,enabled,26.,154.,192.);
                label_right_enabled!(6803,"Directory:",custom,34.,178.,52.);
                input!("backup_directory",custom,92.,176.,101.);
                button!("pref-backup-browse","...",custom,202.,175.,16.,Self::preferences_browse_backup_directory);
                group!(crate::localization::current_preference_control(6830).unwrap_or("备份保留期限"),234.,106.,208.,95.);
                let retention = self.pref_bool("backup_retention.enabled",true);
                check!("pref-backup-retention-enabled","自动清理过期备份","backup_retention.enabled",true,false,true,242.,123.,192.);
                label_right_enabled!(6832,"保留备份:",retention,242.,145.,85.);
                input!("backup_retention.days",retention,334.,143.,56.);
                label_enabled!(crate::localization::current_preference_control(6833).unwrap_or("天"),retention,398.,145.,36.);
                body = body.child(at(242.,165.,192.,28.).id("pref-backup-retention-protection")
                    .text_size(px(12.)).line_height(px(14.)).overflow_hidden()
                    .child(crate::localization::current_preference_control(6834).unwrap_or("仍用于恢复未保存内容的备份不会删除")));
            }
            Page::Instances => {
                group!("多实例设置 *",11.,3.,268.,71.);
                radio!("pref-instance-mono","默认（单实例）","multiInst.setting",0,0,17.,16.,256.);
                radio!("pref-instance-multi","始终使用多实例模式","multiInst.setting",0,2,17.,31.,256.);
                radio!("pref-instance-session","在新实例中打开会话（退出时自动保存会话）","multiInst.setting",0,1,17.,46.,256.);
                group!("自定义插入日期时间",11.,80.,268.,112.);
                check!("pref-date-reverse","颠倒日期和时间的先后顺序（短格式与长格式）","insertDateTime.reverseDefaultOrder",false,false,true,17.,93.,256.);
                for (index,(format,example)) in [("yyyy-MM-dd HH:mm:ss","1985-10-26 16:24:42"),("H:m d/M/yyyy","16:24 26/10/1985"),("MMM d, yyyy  tt h:m","Oct 26, 1985  PM 4:24")].into_iter().enumerate() {
                    let y = 112. + index as f32 * 15.;
                    body = body.child(pref_label_right(format,17. * 1.75,y * 1.75,118. * 1.75))
                        .child(preferences_surface::pref_value_label(example,143. * 1.75,y * 1.75,130. * 1.75));
                }
                label_right!(6172,"Custom format:",14.,161.,77.);
                input!("insertDateTime.customizedFormat",true,92.,159.,175.);
                let preview = self.preferences_date_preview();
                let preview_details = preview.clone();
                body = body.child(at(17.,176.,256.,8.).id("pref-date-preview").text_ellipsis()
                    .tooltip(move |_,cx| icon_tooltip::view(preview_details.clone(),palette.dark,cx)).child(preview));
                group!("面板状态与 [-nosession] *",288.,3.,160.,181.);
                label!("在其他实例或使用 [-nosession] 参数时，记住已打开的面板状态。",294.,14.,150.);
                check!("pref-panel-doclist","文件列表","multiInst.documentList",false,false,true,294.,70.,134.);
                check!("pref-panel-characters","字符面板","multiInst.characterPanel",false,false,true,294.,85.,134.);
                check!("pref-panel-folder","文件夹工作区","multiInst.folderAsWorkspace",false,false,true,294.,100.,134.);
                check!("pref-panel-projects","项目面板","multiInst.projectPanels",false,false,true,294.,115.,134.);
                check!("pref-panel-map","文档结构图","multiInst.documentMap",false,false,true,294.,130.,134.);
                check!("pref-panel-functions","函数列表","multiInst.fuctionList",false,false,true,294.,145.,134.);
                check!("pref-panel-modules","模块面板","multiInst.pluginPanels",false,false,self.converter_enabled,294.,160.,134.);
                if !self.converter_enabled {
                    let module_note = crate::localization::current_string("ModulePanelsUnavailable").unwrap_or(if self.native_locale() == "chineseSimplified.xml" { "固化模块无可恢复面板" } else { "No module panels to restore" });
                    label!(module_note,294.,171.,150.);
                }
                label!("* 修改此设置需要重启 Notepad Matrix 才能生效",15.,197.,433.);
            }
            Page::Performance => {
                let enabled = self.pref_bool("largeFileRestriction.isEnabled",true);
                group!("大文件限制",18.,7.,286.,189.);
                body = body.child(at(282.,23.,14.,8.).id("pref-large-help").role(Role::Button).focusable().tab_index(0).text_color(rgb(palette.link_text)).child("(?)").on_click(cx.listener(|view,_,window,cx| view.preferences_large_file_help(window,cx))));
                check!("pref-large-enabled","启用大文件限制（不使用语法高亮）","largeFileRestriction.isEnabled",true,false,true,26.,41.,260.);
                label_right_enabled!(7144,"Define Large File Size:",enabled,26.,59.,165.);
                body = body.child(self.preferences_performance_size(enabled,window,cx));
                label_enabled!(crate::localization::current_preference_control(7146).unwrap_or("MB   (1 – 2046)"),enabled,228.,59.,70.);
                check!("pref-large-wrap","全局停用自动换行","largeFileRestriction.deactivateWordWrap",true,false,enabled,42.,80.,200.);
                check!("pref-large-completion","允许自动补全","largeFileRestriction.allowAutoCompletion",false,false,enabled,42.,95.,200.);
                check!("pref-large-highlight","允许智能高亮","largeFileRestriction.allowSmartHilite",false,false,enabled,42.,110.,200.);
                check!("pref-large-braces","允许括号匹配","largeFileRestriction.allowBraceMatch",false,false,enabled,42.,125.,200.);
                check!("pref-large-links","允许可点击链接","largeFileRestriction.allowClickableLink",false,false,enabled,42.,140.,200.);
                check!("pref-large-warning","打开 ≥2GB 文件时不显示警告","largeFileRestriction.suppress2GBWarning",false,false,true,26.,178.,260.);
            }
            Page::CloudLinks => {
                group!("超链接设置",18.,18.,424.,161.);
                let enabled = self.preferences_link_enabled();
                body = body.child(self.preferences_link_check("pref-link-enabled","启用","enabled",enabled,true,26.,36.,110.,cx));
                body = body.child(self.preferences_link_check("pref-link-underline","不显示下划线","noUnderline",self.preferences_link_no_underline(),enabled,150.,36.,280.,cx));
                body = body.child(self.preferences_link_check("pref-link-box","启用全框模式","fullBox",self.preferences_link_full_box(),enabled,150.,54.,280.,cx));
                label_enabled!(crate::localization::current_preference_control(6264).unwrap_or("URI customized schemes:"),enabled,26.,78.,408.);
                body = body.child(self.preferences_uri_schemes(enabled,window,cx));
            }
            Page::Miscellaneous => {
                group!("自动检测文件状态",12.,4.,214.,68.);
                let value = self.pref_int("FileAutoDetection",2);
                let detection = if value & 1 != 0 { 1 } else if value & 2 != 0 { 2 } else { 0 };
                let silent = *self.preferences.files.detection_silent.get_or_insert(value & 4 != 0);
                let end = *self.preferences.files.detection_end.get_or_insert(value & 8 != 0);
                let flags = if silent { 4 } else { 0 } | if end { 8 } else { 0 };
                body = body.child(self.pref_choice("FileAutoDetection",vec![(json!(2 | flags),"对当前文件启用".into()),(json!(1 | flags),"对所有打开文件启用".into()),(json!(0),"禁用".into())],json!(value),true,20. * 1.75,20. * 1.75,198. * 1.75,cx));
                body = body.child(self.preferences_detection_check("pref-file-silent","自动更新文件内容",4,silent,detection != 0,38.,cx));
                body = body.child(self.preferences_detection_check("pref-file-end","更新后定位到末行",8,end,detection != 0,52.,cx));
                group!("文档切换（Ctrl+TAB）",234.,4.,214.,39.);
                let switcher = self.pref_bool("document_switcher",true);
                check!("pref-switcher-enabled","启用","document_switcher",true,false,true,242.,15.,198.);
                check!("pref-switcher-mru","按最近使用顺序切换","mru_switching",true,false,switcher,242.,28.,198.);
                group!("文档速览",234.,47.,214.,39.);
                check!("pref-peek-tab","在标签中启用速览","docPeekOnTab",false,false,true,242.,58.,198.);
                check!("pref-peek-map","在文档结构图中启用速览","docPeekOnMap",false,false,true,242.,71.,198.);
                check!("pref-mute","全部静音","muteSounds",false,false,true,20.,132.,420.);
                check!("pref-detect-encoding","自动检测字符编码","DetectEncoding",true,false,true,20.,147.,420.);
                check!("pref-short-title","标题栏仅显示文件名","titleBar.short",false,false,true,20.,162.,420.);
                check!("pref-save-all-confirm","全部保存时弹出确认对话框","SaveAllConfirm",self.pref_bool("save_all_confirm",true),false,true,20.,177.,420.);
                check!("pref-folder-symlinks","允许文件夹工作区载入符号链接","isFawSymlinkAllowed",false,false,true,20.,192.,420.);
                label_right!(6322,"Session file ext.:",242.,97.,124.);
                input!("sessionExt",true,372.,94.,68.);
                label_right!(6337,"Workspace file ext.:",242.,115.,124.);
                input!("workspaceExt",true,372.,112.,68.);
            }
            _ => {}
        }
        body.into_any_element()
    }

    fn preferences_date_preview(&self) -> String {
        use objc2_foundation::{NSDateFormatter, NSLocale, NSString};
        let parser = NSDateFormatter::new();
        parser.setLocale(Some(&NSLocale::localeWithLocaleIdentifier(&NSString::from_str("en_US_POSIX"))));
        parser.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH:mm:ss")));
        let Some(date) = parser.dateFromString(&NSString::from_str("1985-10-26 16:24:42")) else { return String::new(); };
        super::edit_search_surface::format_custom_date_time(&self.pref_string("insertDateTime.customizedFormat","yyyy-MM-dd HH:mm:ss"), &date)
    }

    fn preferences_link_enabled(&self) -> bool {
        self.pref_int("URL",self.pref_int("URL.value",2)) != 0
    }

    fn preferences_link_no_underline(&self) -> bool {
        matches!(self.pref_int("URL",self.pref_int("URL.value",2)),1 | 3)
    }

    fn preferences_link_full_box(&self) -> bool {
        matches!(self.pref_int("URL",self.pref_int("URL.value",2)),3 | 4)
    }

    fn preferences_link_check(&self, id: &'static str, label: &'static str, key: &'static str, checked: bool, enabled: bool, x: f32, y: f32, w: f32, cx: &mut Context<Self>) -> AnyElement {
        let control = match key { "noUnderline" => 6320, "fullBox" => 6350, _ => 6319 };
        let label = crate::localization::current_preference_control(control).unwrap_or(label);
        let focus = self.preference_control_focus(id,cx).tab_stop(enabled);
        let palette = self.preferences_render_palette();
        at(x,y,w,10.).id(id).role(Role::CheckBox).track_focus(&focus).aria_label(label).aria_toggled(checked.into()).a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).flex().items_center().gap(horizontal_px(3.)).text_color(rgb(palette.control_text(enabled))).child(preferences_surface::pref_indicator(checked,false,enabled)).child(label).on_click(cx.listener(move |view,_,window,cx| {
            if !enabled { return; }
            focus.focus(window,cx);
            view.preferences_toggle_link(key,window,cx);
        })).on_key_down(cx.listener(move |view,event: &gpui::KeyDownEvent,window,cx| {
            if enabled && event.keystroke.key == "space" { view.preferences_toggle_link(key,window,cx); cx.stop_propagation(); }
        })).into_any_element()
    }

    fn preferences_snapshot_check(&self,checked: bool,enabled: bool,cx: &mut Context<Self>) -> AnyElement {
        let label = crate::localization::current_preference_control(6818).unwrap_or("Enable session snapshot and periodic backup");
        let focus = self.preference_control_focus("pref-backup-snapshot",cx).tab_stop(enabled);
        let palette = self.preferences_render_palette();
        at(26.,32.,408.,10.).id("pref-backup-snapshot").role(Role::CheckBox).track_focus(&focus).aria_label(label).aria_toggled(checked.into()).a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).flex().items_center().gap(horizontal_px(3.)).text_color(rgb(palette.control_text(enabled))).child(preferences_surface::pref_indicator(checked,false,enabled)).child(label).on_click(cx.listener(move |view,_,window,cx| {
            if !enabled { return; }
            focus.focus(window,cx);
            view.preferences_toggle_snapshot(window,cx);
        })).on_key_down(cx.listener(move |view,event: &gpui::KeyDownEvent,window,cx| {
            if enabled && event.keystroke.key == "space" { view.preferences_toggle_snapshot(window,cx); cx.stop_propagation(); }
        })).into_any_element()
    }

    fn preferences_toggle_snapshot(&mut self,window: &mut Window,cx: &mut Context<Self>) {
        let previous = self.preferences.files.snapshot_ui.unwrap_or(self.pref_bool("snapshot_mode",true) && !self.preferences.files.launch_no_session);
        let value = !previous;
        self.preferences.files.snapshot_ui = Some(value);
        if !self.set_preference("snapshot_mode",json!(value),window,cx) { self.preferences.files.snapshot_ui = Some(previous); }
    }

    fn preferences_toggle_link(&mut self,key: &str,window: &mut Window,cx: &mut Context<Self>) {
        let no_underline = self.preferences_link_no_underline();
        let full_box = self.preferences_link_full_box();
        let mode = match key {
            "enabled" => if self.preferences_link_enabled() { 0 } else { 2 },
            "noUnderline" => if full_box { if no_underline { 4 } else { 3 } } else if no_underline { 2 } else { 1 },
            "fullBox" => if no_underline { if full_box { 1 } else { 3 } } else if full_box { 2 } else { 4 },
            _ => return,
        };
        self.set_preference("URL",json!(mode),window,cx);
    }

    fn preferences_detection_check(&self,id: &'static str,label: &'static str,flag: i64,checked: bool,enabled: bool,y: f32,cx: &mut Context<Self>) -> AnyElement {
        let label = crate::localization::current_preference_control(if flag == 4 { 6313 } else { 6325 }).unwrap_or(label);
        let palette = self.preferences_render_palette();
        let focus = self.preference_control_focus(id,cx).tab_stop(enabled);
        at(20.,y,198.,10.).id(id).role(Role::CheckBox).track_focus(&focus).aria_label(label).aria_toggled(checked.into()).a11y_synthetic_children(move |builder| { if !enabled { builder.parent_node().set_disabled(); } }).flex().items_center().gap(horizontal_px(3.)).text_color(rgb(palette.control_text(enabled))).child(preferences_surface::pref_indicator(checked,false,enabled)).child(label).on_click(cx.listener(move |view,_,window,cx| {
            if !enabled { return; }
            focus.focus(window,cx);
            view.preferences_toggle_detection(flag,checked,window,cx);
        })).on_key_down(cx.listener(move |view,event: &gpui::KeyDownEvent,window,cx| {
            if enabled && event.keystroke.key == "space" { view.preferences_toggle_detection(flag,checked,window,cx); cx.stop_propagation(); }
        })).into_any_element()
    }

    fn preferences_toggle_detection(&mut self,flag: i64,checked: bool,window: &mut Window,cx: &mut Context<Self>) {
        let value = self.pref_int("FileAutoDetection",2);
        if self.set_preference("FileAutoDetection",json!(if checked { value & !flag } else { value | flag }),window,cx) {
            if flag == 4 { self.preferences.files.detection_silent = Some(!checked); }
            else { self.preferences.files.detection_end = Some(!checked); }
        }
    }

    fn preferences_browse_default_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences_browse_directory("openSaveDir.defaultDirPath",window,cx);
    }

    fn preferences_browse_backup_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preferences_browse_directory("backup_directory",window,cx);
    }

    fn preferences_browse_directory(&mut self, path: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.pref_string(path,"");
        let current = PathBuf::from(expand_directory(&current));
        let directory = if current.is_dir() { current } else {
            match self.file_prompt_directory() {
                Ok(directory) => directory,
                Err(error) => { self.notify_editor_error(format!("选择目录无法打开: {error}"),cx); return; }
            }
        };
        let filename = self.native_locale();
        let label = crate::localization::localized_string(&filename, if path == "backup_directory" { "backup-select-folder" } else { "default-open-save-select-folder" }).unwrap_or("Select a folder");
        let prompt = super::native_path_prompt::prompt_for_paths_in(&directory,gpui::PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(label.into()) },window,cx);
        cx.spawn_in(window,async move |view,cx| {
            let result = match prompt.await {
                Ok(result) => Ok(result.selection.and_then(|paths| paths.into_iter().next())),
                Err(error) => Err(error),
            };
            if let Err(error) = view.update_in(cx,|view,window,cx| match result {
                Ok(Some(directory)) => { view.set_preference(path,json!(directory.to_string_lossy()),window,cx); },
                Ok(None) => {},
                Err(error) => view.notify_editor_error(format!("选择目录失败: {error}"),cx),
            }) { eprintln!("preferences directory window unavailable: {error}"); }
        }).detach();
    }

    fn preferences_large_file_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filename = self.native_locale();
        let title = crate::localization::preference_control(&filename, "Performance", 7141).unwrap_or("Large File Restriction");
        let message = crate::localization::localized_string(&filename, "largeFileRestriction-tip").unwrap_or("Some features may slow performance in large files. These features can be auto-disabled on opening a large file. You can customize them here.\n\nNOTE:\n1. Modifying options here requires re-open currently opened large files to get proper behavior.\n\n2. If \"Deactivate Word Wrap globally\" is checked and you open a large file, \"Word Wrap\" will be disabled for all files. You can re-enable it via menu \"View->Word Wrap\"");
        let prompt = window.prompt(gpui::PromptLevel::Info,title,Some(message),&[gpui::PromptButton::ok("OK")],cx);
        cx.spawn(async move |_,_| { if let Err(error) = prompt.await { eprintln!("large file preference prompt failed: {error}"); } }).detach();
    }
}

fn utf16_prefix(value: &str,max: usize) -> String {
    let mut count = 0;
    value.chars().take_while(|ch| { count += ch.len_utf16(); count <= max }).collect()
}

fn unsigned_dialog_integer(value: &str) -> i64 {
    let value = value.trim_start_matches(|ch: char| ch.is_ascii_whitespace());
    let mut number = 0u32;
    for digit in value.bytes().take_while(u8::is_ascii_digit) {
        let Some(next) = number.checked_mul(10).and_then(|number| number.checked_add(u32::from(digit - b'0'))) else { return 0; };
        number = next;
    }
    i64::from(number)
}

struct NativeUriField {
    scroll: objc2::rc::Retained<objc2::runtime::AnyObject>,
    text: objc2::rc::Retained<objc2::runtime::AnyObject>,
}

impl NativeUriField {
    fn new(value: &str) -> io::Result<Self> {
        use objc2::{class,msg_send};
        use objc2::rc::{Allocated,Retained};
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSPoint,NSRect,NSSize,NSString};
        unsafe {
            let frame = NSRect::new(NSPoint::new(0.,0.),NSSize::new(dialog_font::native_length(f64::from(horizontal(714.))),dialog_font::native_length(122.5)));
            let scroll: Allocated<AnyObject> = msg_send![class!(NSScrollView), alloc];
            if Allocated::as_ptr(&scroll).is_null() { return Err(io::Error::other("macOS could not allocate the URI scroll view")); }
            let scroll: Option<Retained<AnyObject>> = msg_send![scroll, initWithFrame: frame];
            let scroll = scroll.ok_or_else(|| io::Error::other("macOS could not initialize the URI scroll view"))?;
            let text: Allocated<AnyObject> = msg_send![class!(NSTextView), alloc];
            if Allocated::as_ptr(&text).is_null() { return Err(io::Error::other("macOS could not allocate the URI text view")); }
            let text: Option<Retained<AnyObject>> = msg_send![text, initWithFrame: frame];
            let text = text.ok_or_else(|| io::Error::other("macOS could not initialize the URI text view"))?;
            let _: () = msg_send![&*text, setRichText: false];
            let _: () = msg_send![&*text, setAllowsUndo: true];
            let _: () = msg_send![&*text, setAutomaticQuoteSubstitutionEnabled: false];
            let _: () = msg_send![&*text, setAutomaticDashSubstitutionEnabled: false];
            let _: () = msg_send![&*text, setAutomaticSpellingCorrectionEnabled: false];
            let _: () = msg_send![&*text, setHorizontallyResizable: false];
            let _: () = msg_send![&*text, setVerticallyResizable: true];
            let _: () = msg_send![&*text, setMinSize: NSSize::new(0.,dialog_font::native_length(122.5))];
            let _: () = msg_send![&*text, setMaxSize: NSSize::new(dialog_font::native_length(f64::from(horizontal(714.))),f64::MAX)];
            let _: () = msg_send![&*text, setTextContainerInset: NSSize::new(dialog_font::native_length(f64::from(horizontal(3.))),dialog_font::native_length(2.))];
            let container: *mut AnyObject = msg_send![&*text, textContainer];
            let _: () = msg_send![container, setWidthTracksTextView: true];
            let _: () = msg_send![container, setContainerSize: NSSize::new(dialog_font::native_length(f64::from(horizontal(714.))),f64::MAX)];
            let font = dialog_font::native_font();
            let _: () = msg_send![&*text, setFont: &*font];
            let _: () = msg_send![&*text, setAccessibilityLabel: &*NSString::from_str("自定义 URI 格式")];
            let _: () = msg_send![&*text, setString: &*NSString::from_str(value)];
            let _: () = msg_send![&*scroll, setBorderType: 0usize];
            let _: () = msg_send![&*scroll, setHasVerticalScroller: true];
            let _: () = msg_send![&*scroll, setHasHorizontalScroller: false];
            let _: () = msg_send![&*scroll, setAutohidesScrollers: false];
            let _: () = msg_send![&*scroll, setDocumentView: &*text];
            Ok(Self { scroll,text })
        }
    }

    fn value(&self) -> Option<String> {
        use objc2::msg_send;
        use objc2_foundation::NSString;
        unsafe {
            let marked: objc2::runtime::Bool = msg_send![&*self.text, hasMarkedText];
            if marked.as_bool() { return None; }
            let value: *const NSString = msg_send![&*self.text, string];
            value.as_ref().map(ToString::to_string)
        }
    }

    fn set_value(&self,value: &str) {
        use objc2::msg_send;
        unsafe { let _: () = msg_send![&*self.text, setString: &*objc2_foundation::NSString::from_str(value)]; }
    }

    fn set_enabled(&self,enabled: bool,palette: RenderPalette) {
        use objc2::{class,msg_send};
        use objc2::runtime::AnyObject;
        unsafe {
            let label = crate::localization::current_preference_control(6264).unwrap_or("Customized URI schemes:");
            let _: () = msg_send![&*self.text, setAccessibilityLabel: &*objc2_foundation::NSString::from_str(label)];
            let _: () = msg_send![&*self.text, setAlignment: if crate::localization::current_preference_rtl() { 1usize } else { 0usize }];
            let _: () = msg_send![&*self.text, setEditable: enabled];
            let _: () = msg_send![&*self.text, setSelectable: enabled];
            let colour = |value: u32| -> *mut AnyObject { msg_send![class!(NSColor), colorWithSRGBRed: f64::from((value >> 16) & 255) / 255., green: f64::from((value >> 8) & 255) / 255., blue: f64::from(value & 255) / 255., alpha: 1f64] };
            let foreground = colour(palette.control_text(enabled));
            let background = colour(if enabled { palette.background } else { palette.pure_background });
            let appearance_name = objc2_foundation::NSString::from_str(if palette.dark { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" });
            let appearance: *mut AnyObject = msg_send![class!(NSAppearance), appearanceNamed: &*appearance_name];
            let _: () = msg_send![&*self.scroll, setAppearance: appearance];
            let _: () = msg_send![&*self.scroll, setBackgroundColor: background];
            let _: () = msg_send![&*self.text, setTextColor: foreground];
            let _: () = msg_send![&*self.text, setInsertionPointColor: foreground];
            let _: () = msg_send![&*self.text, setBackgroundColor: background];
        }
    }

    fn element(&self) -> AnyElement {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSPoint,NSRect,NSSize};
        let scroll = self.scroll.clone();
        gpui::canvas(|_,_,_| (),move |bounds,_,window,_| {
            if let Ok(handle) = HasWindowHandle::window_handle(window) && let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                let top = f64::from(f32::from(bounds.top()));
                let height = f64::from(f32::from(bounds.size.height));
                let y = if view.isFlipped() { top } else { view.bounds().size.height - top - height };
                let frame = NSRect::new(NSPoint::new(f64::from(f32::from(bounds.left())),y),NSSize::new(f64::from(f32::from(bounds.size.width)),height));
                unsafe {
                    let _: () = msg_send![&*scroll, setFrame: frame];
                    let parent: *mut AnyObject = msg_send![&*scroll, superview];
                    if parent != (view as *const NSView).cast_mut().cast::<AnyObject>() { let _: () = msg_send![view, addSubview: &*scroll]; }
                }
            }
        }).size_full().into_any_element()
    }
}

impl Drop for NativeUriField {
    fn drop(&mut self) { use objc2::msg_send; unsafe { let _: () = msg_send![&*self.scroll, removeFromSuperview]; } }
}

impl MatrixWindow {
    fn preferences_association_key(&mut self,list: usize,event: &gpui::KeyDownEvent,cx: &mut Context<Self>) {
        let state = &mut self.preferences.files;
        if !state.association_loaded || !state.association_available || state.association_busy { return; }
        let options: Vec<String> = match list {
            0 => ASSOCIATION_GROUPS.iter().map(|(name,_)| (*name).to_owned()).collect(),
            1 => state.association_group.and_then(|index| ASSOCIATION_GROUPS.get(index)).map(|(_,extensions)| extensions.iter()
                .filter(|extension| !state.associations.iter().any(|entry| entry.extension == **extension && entry.is_current_application))
                .map(|extension| (*extension).to_owned()).collect()).unwrap_or_default(),
            2 => state.associations.iter().filter(|entry| entry.is_current_application).map(|entry| entry.extension.clone()).collect(),
            _ => return,
        };
        if options.is_empty() { return; }
        let current = match list {
            0 => state.association_group,
            1 => state.selected_extension.as_ref().and_then(|selected| options.iter().position(|option| option == selected)),
            _ => state.selected_registered.as_ref().and_then(|selected| options.iter().position(|option| option == selected)),
        };
        let scroll = &state.association_scroll[list];
        let viewport = dialog_font::logical(scroll.bounds().size.height);
        let page = (viewport / 14.).floor().max(1.) as usize;
        let last = options.len() - 1;
        let next = match event.keystroke.key.as_str() {
            "up" => current.unwrap_or(0).saturating_sub(1),
            "down" => current.map_or(0,|index| index.saturating_add(1).min(last)),
            "home" => 0,
            "end" => last,
            "pageup" => current.unwrap_or(0).saturating_sub(page),
            "pagedown" => current.map_or(0,|index| index.saturating_add(page).min(last)),
            _ => return,
        };
        match list {
            0 => { state.association_group = Some(next); state.selected_extension = None; }
            1 => state.selected_extension = Some(options[next].clone()),
            _ => state.selected_registered = Some(options[next].clone()),
        }
        let offset = -dialog_font::logical(scroll.offset().y);
        let top = next as f32 * 14.;
        let offset = if top < offset { top } else if top + 14. > offset + viewport { top + 14. - viewport } else { offset };
        scroll.set_offset(gpui::point(px(0.),px(-offset.clamp(0.,dialog_font::logical(scroll.max_offset().y).max(0.)))));
        cx.stop_propagation();
        cx.notify();
    }
}
