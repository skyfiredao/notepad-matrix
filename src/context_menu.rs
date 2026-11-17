use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::ConfigStore;
use crate::menu::{MenuCommand, MenuDefinition, MenuEntry};
use crate::style_config::{Node, read_xml};

const MENU_NAMES_XML: &str = include_str!("../assets/contextMenu-menuNames.xml");
pub(crate) const DEFAULT_XML: &str = include_str!("../assets/contextMenu.xml");

#[derive(Clone, Debug)]
pub(crate) enum ContextMenuEntry {
    Command {
        command: MenuCommand,
        source_id: Option<u32>,
        label: String,
    },
    Submenu {
        label: String,
        entries: Vec<ContextMenuEntry>,
    },
    Separator,
}

pub(crate) fn into_menu_entries(entries: Vec<ContextMenuEntry>, definitions: &[MenuDefinition]) -> Vec<MenuEntry> {
    fn convert(entries: Vec<ContextMenuEntry>, definitions: &[MenuDefinition], prefix: &str) -> Vec<MenuEntry> {
        entries.into_iter().enumerate().map(|(index, entry)| match entry {
            ContextMenuEntry::Command { command, source_id, label } => {
                let shortcut = definitions.iter().find_map(|root| {
                    find_entry(&root.entries, &|entry| entry.command() == Some(command)
                        && source_id.is_none_or(|id| entry.command().and_then(MenuCommand::source_id) == Some(id)))
                }).map_or_else(|| command.shortcut().to_owned(), |entry| entry.shortcut().to_owned());
                MenuEntry::DynamicAction { command, label, shortcut }
            }
            ContextMenuEntry::Submenu { label, entries } => {
                let selector = format!("{prefix}-{index}");
                let mnemonic = MenuEntry::label_mnemonic(&label);
                let entries = convert(entries, definitions, &selector);
                MenuEntry::DynamicSubmenu { selector, label, mnemonic, entries }
            }
            ContextMenuEntry::Separator => MenuEntry::Separator,
        }).collect()
    }
    convert(entries, definitions, "context-menu-folder")
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ContextMenu {
    document: Node,
    english_names: BTreeMap<u32, String>,
    translated_folders: BTreeMap<String, String>,
}

impl ContextMenu {
    pub fn new() -> io::Result<Self> {
        Self::from_document(Node::parse(DEFAULT_XML)?)
    }

    pub fn load(store: Option<&ConfigStore>) -> io::Result<Self> {
        let Some(store) = store else { return Self::new(); };
        let path = Self::ensure_file(store)?;
        read_xml(&path).and_then(Self::from_document).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", path.display()))
        })
    }

    pub fn from_document(document: Node) -> io::Result<Self> {
        if document.name != "NotepadPlus" {
            return Err(invalid("Context menu root must be NotepadPlus"));
        }
        if document.child("ScintillaContextMenu").is_none() {
            return Err(invalid("Context menu has no ScintillaContextMenu"));
        }
        let english = Node::parse(MENU_NAMES_XML)?;
        let commands = english.child("Native-Langue")
            .and_then(|node| node.child("Menu"))
            .and_then(|node| node.child("Main"))
            .and_then(|node| node.child("Commands"))
            .ok_or_else(|| invalid("English menu resource has no Commands"))?;
        let english_names = commands.children.iter().filter_map(|node| {
            Some((node.attribute("id")?.parse().ok()?, purge_menu_label(node.attribute("name")?)))
        }).collect();
        let mut translated_folders = BTreeMap::new();
        collect_translations(&english, &mut translated_folders);
        Ok(Self { document, english_names, translated_folders })
    }

    pub fn ensure_file(store: &ConfigStore) -> io::Result<PathBuf> {
        store.document_path(Path::new("settings.json"))?;
        let path = store.root().join("contextMenu.xml");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => return Ok(path),
            Ok(_) => return Err(invalid("contextMenu.xml must be a regular private file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
        let mut temporary = tempfile::NamedTempFile::new_in(store.root())?;
        temporary.write_all(DEFAULT_XML.as_bytes())?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => Ok(path),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.is_file() && !metadata.file_type().is_symlink() { Ok(path) }
                else { Err(invalid("contextMenu.xml must be a regular private file")) }
            }
            Err(error) => Err(error.error),
        }
    }

    pub fn xml(&self) -> String {
        self.document.xml()
    }

    pub fn entries(&self, definitions: &[MenuDefinition]) -> Vec<ContextMenuEntry> {
        let Some(root) = self.document.child("ScintillaContextMenu") else { return Vec::new(); };
        let items = root.children.iter().filter(|node| node.name == "Item").filter_map(|node| {
            let id = node.attribute("id").and_then(|id| id.parse::<i32>().ok()).filter(|id| *id >= 0);
            let entry = if id == Some(0) {
                ContextMenuEntry::Separator
            } else {
                let (command, current_label) = if let Some(id) = id {
                    find_by_id(definitions, id as u32)?
                } else {
                    self.find_by_name(node, definitions)?
                };
                let label = node.attribute("ItemNameAs").filter(|label| !label.is_empty())
                    .map(str::to_owned).unwrap_or_else(|| localized_menu_label(current_label));
                ContextMenuEntry::Command { command, source_id: command.source_id(), label }
            };
            let folder = node.attribute("FolderName").unwrap_or("");
            let folder = node.attribute("TranslateID")
                .and_then(|id| self.translated_folders.get(id)).map(String::as_str).unwrap_or(folder);
            Some((folder.to_owned(), entry))
        }).collect::<Vec<_>>();
        let mut entries = Vec::new();
        let mut current_folder = String::new();
        let mut last_is_separator = false;
        for (index, (folder, entry)) in items.iter().enumerate() {
            if folder.is_empty() {
                current_folder.clear();
                if matches!(entry, ContextMenuEntry::Separator) {
                    if index != 0 && index + 1 != items.len() && !last_is_separator {
                        entries.push(entry.clone());
                    }
                    last_is_separator = true;
                } else {
                    entries.push(entry.clone());
                    last_is_separator = false;
                }
            } else {
                if *folder != current_folder {
                    current_folder = folder.clone();
                    entries.push(ContextMenuEntry::Submenu { label: folder.clone(), entries: Vec::new() });
                }
                if let Some(ContextMenuEntry::Submenu { entries, .. }) = entries.last_mut() {
                    entries.push(entry.clone());
                }
                last_is_separator = false;
            }
        }
        entries
    }

    fn find_by_name<'a>(&self, node: &Node, definitions: &'a [MenuDefinition]) -> Option<(MenuCommand, &'a str)> {
        let menu_name = node.attribute("MenuEntryName").unwrap_or("");
        let item_name = node.attribute("MenuItemName").unwrap_or("");
        if !menu_name.is_empty() && !item_name.is_empty() {
            let root = definitions.iter().find(|root| english_root(root.selector).eq_ignore_ascii_case(menu_name))?;
            return find_action(&root.entries, &|command, label| {
                command.source_id().and_then(|id| self.english_names.get(&id))
                    .map_or_else(|| purge_menu_label(label).to_lowercase() == item_name.to_lowercase(),
                        |name| name.to_lowercase() == item_name.to_lowercase())
            });
        }
        let plugin_name = node.attribute("PluginEntryName").filter(|name| !name.is_empty())?;
        let command_name = node.attribute("PluginCommandItemName").filter(|name| !name.is_empty())?;
        let plugins = definitions.iter().find(|root| english_root(root.selector) == "Plugins")?;
        let plugin = plugins.entries.iter().find_map(|entry| {
            (purge_menu_label(entry.label()).to_lowercase() == plugin_name.to_lowercase())
                .then(|| entry.submenu_entries()).flatten()
        })?;
        plugin.iter().find_map(|entry| match entry {
            MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. }
                if purge_menu_label(entry.label()).to_lowercase() == command_name.to_lowercase() => Some((*command, entry.label())),
            _ => None,
        })
    }
}

fn find_by_id(definitions: &[MenuDefinition], id: u32) -> Option<(MenuCommand, &str)> {
    definitions.iter().find_map(|root| find_action(&root.entries, &|command, _| command.source_id() == Some(id)))
}

fn find_action<'a>(entries: &'a [MenuEntry], matches: &impl Fn(MenuCommand, &str) -> bool) -> Option<(MenuCommand, &'a str)> {
    entries.iter().find_map(|entry| match entry {
        MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } if matches(*command, entry.label()) => Some((*command, entry.label())),
        MenuEntry::Submenu { entries, .. } | MenuEntry::DynamicSubmenu { entries, .. } => find_action(entries, matches),
        _ => None,
    })
}

fn find_entry<'a>(entries: &'a [MenuEntry], matches: &impl Fn(&MenuEntry) -> bool) -> Option<&'a MenuEntry> {
    entries.iter().find_map(|entry| {
        if matches(entry) { Some(entry) }
        else { entry.submenu_entries().and_then(|entries| find_entry(entries, matches)) }
    })
}

fn english_root(selector: &str) -> &str {
    match selector {
        "file-menu-trigger" => "File",
        "edit-menu-trigger" => "Edit",
        "search-menu-trigger" => "Search",
        "view-menu-trigger" => "View",
        "encoding-menu-trigger" => "Encoding",
        "language-menu-trigger" => "Language",
        "settings-menu-trigger" => "Settings",
        "tools-menu-trigger" => "Tools",
        "macro-menu-trigger" => "Macro",
        "run-menu-trigger" => "Run",
        "plugins-menu-trigger" => "Plugins",
        "window-menu-trigger" => "Window",
        "help-menu-trigger" => "?",
        _ => "",
    }
}

fn purge_menu_label(label: &str) -> String {
    label.chars().take(63).take_while(|ch| *ch != '\t').filter(|ch| *ch != '&').collect()
}

fn localized_menu_label(label: &str) -> String {
    let mut label = purge_menu_label(label);
    let suffix = label.chars().rev().take(3).collect::<Vec<_>>();
    if suffix.len() == 3 && suffix[0] == ')' && suffix[1].is_ascii_alphabetic() && suffix[2] == '(' {
        label.truncate(label.len() - 3);
    }
    label
}

fn collect_translations(node: &Node, translations: &mut BTreeMap<String, String>) {
    if node.name.starts_with("contextMenu-") && let Some(value) = node.attribute("value") {
        translations.insert(node.name.clone(), value.to_owned());
    }
    for child in &node.children { collect_translations(child, translations); }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
