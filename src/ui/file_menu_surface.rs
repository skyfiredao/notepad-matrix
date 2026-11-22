use super::*;
use crate::files::DiskFile;
use objc2::MainThreadOnly;
use objc2_app_kit::{NSAlert, NSTextField, NSWorkspace};
use objc2_foundation::{
    NSFileManager, NSFileManagerItemReplacementOptions, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use std::os::unix::fs::MetadataExt;

fn file_url(path: &std::path::Path) -> io::Result<objc2::rc::Retained<NSURL>> {
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "路径不是 UTF-8"))?;
    Ok(NSURL::fileURLWithPath(&NSString::from_str(path)))
}

pub(super) fn has_default_viewer(path: &std::path::Path) -> bool {
    file_url(path).is_ok_and(|url| {
        NSWorkspace::sharedWorkspace()
            .URLForApplicationToOpenURL(&url)
            .is_some()
    })
}

fn move_file(
    from: &std::path::Path,
    to: &std::path::Path,
    expected: Option<&std::fs::Metadata>,
) -> io::Result<()> {
    let _source_access = crate::file_access::directory(from.parent().unwrap_or_else(|| std::path::Path::new(".")))?;
    let _target_access = crate::file_access::directory(to.parent().unwrap_or_else(|| std::path::Path::new(".")))?;
    let manager = NSFileManager::defaultManager();
    let source = file_url(from)?;
    let target = file_url(to)?;
    let result = if let Some(expected) = expected {
        let current = std::fs::symlink_metadata(to)?;
        if current.dev() != expected.dev()
            || current.ino() != expected.ino()
            || current.len() != expected.len()
            || current.modified()? != expected.modified()?
        {
            return Err(io::Error::other("目标文件在确认后已改变，请重新选择"));
        }
        manager.replaceItemAtURL_withItemAtURL_backupItemName_options_resultingItemURL_error(
            &target,
            &source,
            None,
            NSFileManagerItemReplacementOptions::UsingNewMetadataOnly,
            None,
        )
    } else {
        manager.moveItemAtURL_toURL_error(&source, &target)
    };
    result.map_err(|error| io::Error::other(error.to_string()))
}

fn rename_tab_prompt(name: &str) -> io::Result<Option<String>> {
    let mtm = MainThreadMarker::new().ok_or_else(|| io::Error::other("重命名需要主线程"))?;
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("重命名当前标签页"));
    alert.setInformativeText(&NSString::from_str("新名称"));
    alert.addButtonWithTitle(&NSString::from_str("确定"));
    let cancel = alert.addButtonWithTitle(&NSString::from_str("取消"));
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let field = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(0., 0.), NSSize::new(320., 24.)),
    );
    field.setStringValue(&NSString::from_str(name));
    alert.setAccessoryView(Some(&field));
    alert.window().makeFirstResponder(Some(&field));
    Ok((alert.runModal() == 1000).then(|| field.stringValue().to_string()))
}

enum Renamed {
    Tab(String),
    File {
        from: PathBuf,
        to: PathBuf,
        file: DiskFile,
    },
}

impl MatrixWindow {
    pub(super) fn file_menu_command(
        &mut self,
        command: FileCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_busy
            || self.file_open_sequence_pending()
            || self.close_dialog
            || self.save_all_dialog
            || self.reload_target.is_some()
            || self.printing
            || self.batch_editing_blocked()
            || self.macro_interaction_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        match command {
            FileCommand::Terminal => {
                self.open_system_terminal(window, cx);
            }
            FileCommand::Reveal | FileCommand::DefaultViewer => {
                let Some(path) = self.disk_file().map(|file| file.path().to_path_buf()) else {
                    return;
                };
                let operation = match command {
                    FileCommand::Reveal => crate::folder_workspace::FolderShellCommand::Reveal,
                    _ => crate::folder_workspace::FolderShellCommand::Open,
                };
                if let Err(error) = crate::folder_workspace::folder_shell(&path, operation) {
                    self.notify_editor_error(format!("无法打开文件位置或默认程序：{error}"), cx);
                }
            }
            FileCommand::FolderWorkspace => self.add_workspace_folder(window, cx),
            FileCommand::ContainingWorkspace => {
                if let Some(directory) = self
                    .disk_file()
                    .and_then(|file| file.path().parent())
                    .map(PathBuf::from)
                    && let Err(error) = self.folder.tree.add_root(&directory)
                {
                    self.notify_editor_error(format!("无法打开文件夹工作区：{error}"), cx);
                    return;
                }
                self.show_panel(crate::panel_layout::PanelId::Folder, window, cx);
                self.refresh_workspace_folder(window, cx);
            }
            FileCommand::Rename => self.rename_current_file(window, cx),
            FileCommand::Trash => self.trash_current_file(window, cx),
            FileCommand::PrintNow => self.print_document_mode(false, window, cx),
            FileCommand::Recent(index) => {
                if let Some(path) = self.workspace.recently_closed().get(index).cloned() {
                    self.open_recent_paths(vec![path], window, cx);
                }
            }
            FileCommand::OpenAllRecent => {
                let paths = self
                    .workspace
                    .recently_closed()
                    .iter()
                    .rev()
                    .cloned()
                    .collect();
                self.open_recent_paths(paths, window, cx);
            }
            FileCommand::ClearRecent => {
                self.workspace.clear_recent_files();
                self.synchronize_recovery(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn open_recent_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_open_paths(paths, window, cx);
    }

    pub(super) fn apply_file_menu_preferences(&mut self, settings: &serde_json::Value) {
        self.file_prompt.recent_in_submenu = settings["History"]["inSubMenu"].as_bool().unwrap_or(false);
        self.file_prompt.recent_custom_length = settings["History"]["customLength"].as_i64().unwrap_or(-1);
    }

    pub(super) fn recent_file_menu_entries(&self) -> Vec<crate::menu::MenuEntry> {
        use crate::menu::{DocumentCommand, MenuEntry};
        let paths = self.workspace.recently_closed();
        if paths.is_empty() { return Vec::new(); }
        let mut entries = paths.iter().enumerate().map(|(index, path)| MenuEntry::DynamicAction {
            command: MenuCommand::File(FileCommand::Recent(index)),
            label: recent_file_label(path, index, self.file_prompt.recent_custom_length),
            shortcut: String::new(),
        }).collect::<Vec<_>>();
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::Action(MenuCommand::Document(DocumentCommand::RestoreLastClosed)));
        entries.push(MenuEntry::Action(MenuCommand::File(FileCommand::OpenAllRecent)));
        entries.push(MenuEntry::Action(MenuCommand::File(FileCommand::ClearRecent)));
        entries.push(MenuEntry::Separator);
        if self.file_prompt.recent_in_submenu {
            vec![MenuEntry::Submenu { selector: "file-recent-files", label: "最近文件", mnemonic: 'r', entries }, MenuEntry::Separator]
        } else { entries }
    }

    fn rename_current_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.workspace.active().id();
        let name = self.workspace.active().name();
        let original = self
            .disk_file()
            .filter(|file| file.path().is_file())
            .cloned();
        let others = self
            .workspace
            .buffers()
            .iter()
            .filter(|buffer| buffer.id() != id)
            .filter_map(|buffer| {
                buffer
                    .disk_file
                    .as_ref()
                    .map(|file| file.path().to_path_buf())
            })
            .collect::<Vec<_>>();
        let prompt = original.as_ref().map(|file| {
            let path = file.path();
            cx.prompt_for_new_path(
                path.parent().unwrap_or_else(|| std::path::Path::new(".")),
                path.file_name().and_then(|name| name.to_str()),
            )
        });
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.spawn_in(window, async move |view, cx| {
            let result: io::Result<Option<Renamed>> = async {
                if let (Some(file), Some(prompt)) = (original, prompt) {
                    let from = file.path().to_path_buf();
                    let selected = prompt
                        .await
                        .map_err(|error| io::Error::other(error.to_string()))?
                        .map_err(|error| io::Error::other(error.to_string()))?;
                    let Some(to) = selected else { return Ok(None) };
                    let to = file_actions::resolve_save_target(&to)?;
                    if from == to {
                        return Ok(None);
                    }
                    if others.contains(&to) {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "目标文件已在其他标签页打开",
                        ));
                    }
                    let source = from.clone();
                    let target = to.clone();
                    let expected = match std::fs::symlink_metadata(&to) {
                        Ok(metadata) => Some(metadata),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                        Err(error) => return Err(error),
                    };
                    let file = executor
                        .spawn(async move {
                            let access = crate::file_access::directory(target.parent().unwrap_or_else(|| std::path::Path::new(".")))?;
                            move_file(&source, &target, expected.as_ref())?;
                            file.after_move(&target, access)
                        })
                        .await?;
                    Ok(Some(Renamed::File { from, to, file }))
                } else {
                    rename_tab_prompt(&name).map(|name| name.map(Renamed::Tab))
                }
            }
            .await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                let result = result.and_then(|result| {
                    match result {
                        Some(Renamed::Tab(name)) => {
                            let old_name = view.workspace.active().name();
                            view.workspace.rename_untitled(id, &name)?;
                            view.synchronize_renamed_tab_backup(id, &old_name, cx);
                        }
                        Some(Renamed::File { from, to, file }) => {
                            crate::folder_workspace::relocate_open_paths(
                                &mut view.workspace,
                                &from,
                                &to,
                            );
                            view.folder.tree.relocate_paths(&from, &to);
                            if let Some(buffer) = view
                                .workspace
                                .buffers_mut()
                                .find(|buffer| buffer.id() == id)
                            {
                                buffer.disk_file = Some(file);
                                if !buffer.language_is_explicit() {
                                    buffer.set_detected_language(
                                        crate::syntax::Language::from_path(&to),
                                    );
                                }
                            }
                            view.synchronize_renamed_tab_backup(id, &name, cx);
                        }
                        None => {}
                    }
                    Ok(())
                });
                view.reset_active_view(window, cx);
                if let Err(error) = result {
                    view.notify_editor_error(format!("重命名失败：{error}"), cx);
                }
            }) {
                eprintln!("rename operation lost its document: {error}");
            }
        })
        .detach();
        cx.notify();
    }

    fn trash_current_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.disk_file().map(|file| file.path().to_path_buf()) else {
            return;
        };
        let id = self.workspace.active().id();
        let description = format!(
            "文件“{}”将移到废纸篓，并关闭两个编辑区中的此文档。继续？",
            path.display()
        );
        let response = window.prompt(
            gpui::PromptLevel::Warning,
            "从磁盘删除",
            Some(&description),
            &[
                gpui::PromptButton::ok("移到废纸篓"),
                gpui::PromptButton::cancel("取消"),
            ],
            cx,
        );
        let executor = cx.background_executor().clone();
        self.file_busy = true;
        cx.spawn_in(window, async move |view, cx| {
            let result: io::Result<bool> = async {
                if response
                    .await
                    .map_err(|error| io::Error::other(error.to_string()))?
                    != 0
                {
                    return Ok(false);
                }
                executor
                    .spawn(async move {
                        let url = file_url(&path)?;
                        NSFileManager::defaultManager()
                            .trashItemAtURL_resultingItemURL_error(&url, None)
                            .map_err(|error| io::Error::other(error.to_string()))
                    })
                    .await?;
                Ok(true)
            }
            .await;
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.file_busy = false;
                let mut removed_document = false;
                let result = result.and_then(|removed| {
                    if removed {
                        for pane in [
                            crate::workspace::ViewId::Primary,
                            crate::workspace::ViewId::Secondary,
                        ] {
                            if view.workspace.view_buffer_ids(pane).contains(&id) {
                                view.workspace
                                    .close_in_view(
                                        pane,
                                        id,
                                        crate::workspace::CloseDisposition::DiscardConfirmed,
                                    )
                                    .map_err(|error| io::Error::other(error.to_string()))?;
                            }
                        }
                        removed_document = true;
                    }
                    Ok(())
                });
                if result.is_ok() && removed_document && view.quit_after_last_document_close(id, window, cx) { return; }
                view.reset_active_view(window, cx);
                if let Err(error) = result {
                    view.notify_editor_error(format!("移到废纸篓失败：{error}"), cx);
                }
            }) {
                eprintln!("trash operation lost its document: {error}");
            }
        })
        .detach();
        cx.notify();
    }
}

fn prefix_units(value: &str, maximum: usize) -> &str {
    let mut units = 0;
    let mut end = 0;
    for (offset, ch) in value.char_indices() {
        units += ch.len_utf16();
        if units > maximum { break; }
        end = offset + ch.len_utf8();
    }
    &value[..end]
}

fn suffix_units(value: &str, maximum: usize) -> &str {
    let mut units = 0;
    let mut start = value.len();
    for (offset, ch) in value.char_indices().rev() {
        units += ch.len_utf16();
        if units > maximum { break; }
        start = offset;
    }
    &value[start..]
}

fn compact_recent_path(path: &std::path::Path, maximum: usize) -> String {
    let full = path.to_string_lossy();
    if full.encode_utf16().count() <= maximum { return full.into_owned(); }
    if maximum <= 3 { return ".".repeat(maximum); }
    let filename = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_else(|| full.clone());
    let filename_length = filename.encode_utf16().count();
    if filename_length + 4 > maximum {
        return format!("{}...", prefix_units(&filename, maximum - 3));
    }
    let prefix_length = maximum - filename_length - 4;
    let prefix = prefix_units(&full, prefix_length);
    format!("{prefix}.../{filename}")
}

fn recent_file_label(path: &std::path::Path, index: usize, custom_length: i64) -> String {
    let name = if custom_length > 0 {
        compact_recent_path(path, custom_length.min(259) as usize)
    } else {
        let value = if custom_length == 0 {
            path.file_name().map(|name| name.to_string_lossy()).unwrap_or_else(|| path.as_os_str().to_string_lossy())
        } else { path.as_os_str().to_string_lossy() };
        if value.encode_utf16().count() < 260 { value.into_owned() }
        else { format!("{}...{}", prefix_units(&value, 127), suffix_units(&value, 130)) }
    };
    format!("{}: {name}", index + 1)
}
