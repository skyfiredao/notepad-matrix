pub const BUILD_VERSION: &str = env!("NOTEPAD_MATRIX_BUILD_VERSION");

pub mod byte_buffer;
pub mod backup_retention;
mod byte_projection;
pub mod byte_text;
mod change_history;
pub mod character_panel;
pub mod clipboard;
pub mod codepages;
pub mod config;
pub mod config_directory;
pub mod context_menu;
pub mod converter;
pub mod editor;
pub mod file_access;
pub mod files;
pub mod find_in_files;
pub mod finder;
pub mod folder_workspace;
pub mod hashes;
pub mod html_export;
pub mod indentation;
pub mod input;
pub mod intelligence;
pub mod launch;
pub mod macros;
pub mod markdown;
pub mod menu;
pub mod mime;
pub mod monitoring;
#[cfg(all(target_os = "macos", feature = "native-ui"))]
mod native_binary_clipboard;
#[cfg(all(target_os = "macos", feature = "native-ui"))]
mod native_app_open;
#[cfg(all(target_os = "macos", feature = "native-ui"))]
pub(crate) mod ssh_terminal;
pub mod panel_layout;
pub mod projects;
pub mod recovery;
mod regex_input;
mod search_matcher;
#[cfg(all(target_os = "macos", feature = "native-ui"))]
pub(crate) mod child_process;
pub mod search;
pub mod search_history;
pub mod search_regex;
pub mod status;
pub mod style_config;
pub mod shortcuts;
pub mod syntax;
pub mod text_transforms;
mod unicode_case;
mod unicode_class;
mod unicode_grapheme;
mod unicode_mark;
mod unicode_reader;
mod unicode_writer;
pub mod workspace;

#[cfg(all(target_os = "macos", feature = "native-ui"))]
pub mod ui;

pub mod builtin_language;
pub mod languages;
pub mod localization;
pub mod lexers;
pub mod links;
pub mod udl;
pub mod udl_controls;
pub mod udl_registry;
