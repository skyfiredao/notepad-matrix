use notepad_matrix::menu::{
    FileCommand, MenuBar, MenuCommand, MenuDefinition, MenuEvent, MenuMode, MenuOutcome,
};

#[test]
fn every_retained_transform_has_one_source_labeled_reachable_menu_action() {
    use notepad_matrix::menu::MenuEntry;
    fn visit(entries: &[MenuEntry], found: &mut Vec<(u32, String)>) {
        for entry in entries {
            match entry {
                MenuEntry::Action(MenuCommand::TextTransform(id)) => {
                    found.push((*id, entry.label().into()))
                }
                MenuEntry::Submenu { entries, .. } => visit(entries, found),
                _ => {}
            }
        }
    }
    let bar = MenuBar::default();
    let mut found = Vec::new();
    for menu in bar.definitions() {
        visit(&menu.entries, &mut found);
    }
    assert_eq!(found.len(), 52);
    for (_, id, _) in notepad_matrix::text_transforms::RETAINED_COMMANDS {
        let entries: Vec<_> = found
            .iter()
            .filter(|(candidate, _)| candidate == id)
            .collect();
        assert_eq!(entries.len(), 1, "source command {id}");
        assert!(!entries[0].1.starts_with("IDM_"));
    }
    assert!(found.contains(&(42016, "UPPERCASE".into())));
    assert!(found.contains(&(42022, "Toggle Single Line Comment".into())));
    assert!(found.contains(&(42023, "Block Comment".into())));
    assert!(
        bar.definitions()
            .iter()
            .flat_map(|menu| &menu.entries)
            .any(|entry| matches!(
                entry,
                MenuEntry::Submenu {
                    selector: "edit-paste-special",
                    ..
                }
            ))
    );
    assert!(
        bar.definitions()
            .iter()
            .flat_map(|menu| &menu.entries)
            .any(|entry| entry.label() == "Character Panel")
    );
    assert!(
        bar.definitions()
            .iter()
            .flat_map(|menu| &menu.entries)
            .any(|entry| matches!(
                entry,
                MenuEntry::Submenu {
                    selector: "edit-copy-to-clipboard",
                    ..
                }
            ))
    );
    let completion = bar
        .definitions()
        .iter()
        .flat_map(|menu| &menu.entries)
        .find_map(|entry| match entry {
            MenuEntry::Submenu {
                selector: "edit-menu-auto-completion",
                entries,
                ..
            } => Some(entries),
            _ => None,
        })
        .expect("source Auto-Completion menu");
    assert_eq!(
        completion
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Action(command) => command.source_id(),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [50000, 50001, 50002, 50010, 50011, 50006]
    );
}

#[test]
fn multiselection_menu_preserves_source_ids_options_and_unassigned_shortcuts() {
    use notepad_matrix::editor::OccurrenceOptions;
    use notepad_matrix::menu::{MenuEntry, MultiSelectionCommand};
    fn collect(entries: &[MenuEntry], out: &mut Vec<MenuCommand>) {
        for entry in entries {
            match entry {
                MenuEntry::Action(command) => out.push(*command),
                MenuEntry::Submenu { entries, .. } => collect(entries, out),
                _ => {}
            }
        }
    }
    let bar = MenuBar::default();
    let mut commands = Vec::new();
    for root in bar.definitions() {
        collect(&root.entries, &mut commands);
    }
    for (offset, options) in [
        OccurrenceOptions {
            match_case: false,
            whole_word: false,
        },
        OccurrenceOptions {
            match_case: true,
            whole_word: false,
        },
        OccurrenceOptions {
            match_case: false,
            whole_word: true,
        },
        OccurrenceOptions {
            match_case: true,
            whole_word: true,
        },
    ]
    .into_iter()
    .enumerate()
    {
        for (command, id) in [
            (
                MenuCommand::MultiSelection(MultiSelectionCommand::All(options)),
                42090 + offset as u32,
            ),
            (
                MenuCommand::MultiSelection(MultiSelectionCommand::Next(options)),
                42094 + offset as u32,
            ),
        ] {
            assert!(commands.contains(&command));
            assert_eq!(command.source_id(), Some(id));
            assert_eq!(command.shortcut(), "");
        }
    }
    for command in [
        MultiSelectionCommand::Undo,
        MultiSelectionCommand::Skip,
        MultiSelectionCommand::Rotate,
        MultiSelectionCommand::Swap,
    ] {
        assert!(commands.contains(&MenuCommand::MultiSelection(command)));
        assert_eq!(MenuCommand::MultiSelection(command).shortcut(), "");
    }
    assert!(commands.contains(&MenuCommand::ColumnEditor));
    assert_eq!(MenuCommand::ColumnEditor.shortcut(), "Alt+C");
    assert_eq!(MenuCommand::TextTransform(42010).shortcut(), "Ctrl+D");
}

#[test]
fn document_and_project_commands_are_reachable_with_frozen_source_ids() {
    use notepad_matrix::menu::{DocumentCommand, MenuEntry, ViewCommand};
    use notepad_matrix::workspace::TabSort;
    fn collect(entries: &[MenuEntry], out: &mut Vec<MenuCommand>) {
        for entry in entries {
            match entry {
                MenuEntry::Action(command) => out.push(*command),
                MenuEntry::Submenu { entries, .. } => collect(entries, out),
                _ => {}
            }
        }
    }
    let bar = MenuBar::default();
    let mut commands = Vec::new();
    for root in bar.definitions() {
        collect(&root.entries, &mut commands);
    }
    let expected = [
        (DocumentCommand::Pin, 44048),
        (DocumentCommand::Color(Some(0)), 44111),
        (DocumentCommand::Color(Some(4)), 44115),
        (DocumentCommand::Color(None), 44110),
        (DocumentCommand::Multiline, 6119),
        (DocumentCommand::Vertical, 6120),
        (DocumentCommand::Mru, 6117),
        (DocumentCommand::Switcher, 6114),
        (DocumentCommand::RestoreLastClosed, 41021),
        (DocumentCommand::SaveSession, 41013),
        (DocumentCommand::LoadSession, 41012),
    ];
    for (command, id) in [
        (ViewCommand::MoveToNewInstance, 10003),
        (ViewCommand::OpenInNewInstance, 10004),
    ] {
        assert!(commands.contains(&MenuCommand::View(command)));
        assert_eq!(MenuCommand::View(command).source_id(), Some(id));
    }
    for command in [
        notepad_matrix::menu::ChangeHistoryCommand::Next,
        notepad_matrix::menu::ChangeHistoryCommand::Previous,
        notepad_matrix::menu::ChangeHistoryCommand::Clear,
    ] {
        let command = MenuCommand::ChangeHistory(command);
        assert!(commands.contains(&command));
        assert!(matches!(command.source_id(), Some(43067..=43069)));
        assert_eq!(command.shortcut(), "");
    }
    for (command, id) in expected {
        assert!(
            commands.contains(&MenuCommand::Document(command)),
            "{command:?}"
        );
        assert_eq!(MenuCommand::Document(command).source_id(), Some(id));
    }
    for (index, sort) in [
        TabSort::Name,
        TabSort::Path,
        TabSort::Type,
        TabSort::Size,
        TabSort::Modified,
    ]
    .into_iter()
    .enumerate()
    {
        for descending in [false, true] {
            let command = MenuCommand::Document(DocumentCommand::SortTabs(sort, descending));
            assert!(commands.contains(&command));
            assert_eq!(
                command.source_id(),
                Some(11002 + 2 * index as u32 + u32::from(descending))
            );
        }
    }
    for index in 0..3 {
        let command = MenuCommand::View(ViewCommand::ProjectPanel(index));
        assert!(commands.contains(&command));
        assert_eq!(command.source_id(), Some(44081 + index as u32));
    }
}

#[test]
fn builtin_language_refresh_keeps_source_order_names_aliases_and_udls()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::MenuEntry;
    use notepad_matrix::syntax::Language;
    let catalog = notepad_matrix::languages::catalog()?;
    let mut bar = MenuBar::default();
    bar.set_user_languages(vec![(7, "Team DSL".into())]);
    let revision = bar.revision();
    bar.set_builtin_languages(
        catalog
            .profiles()
            .iter()
            .map(|profile| (profile.id, profile.label.clone()))
            .collect(),
    );
    assert!(bar.revision() > revision);
    assert_eq!(bar.mode(), MenuMode::Closed);
    let menu = bar
        .definitions()
        .iter()
        .find(|menu| menu.label == "Language")
        .ok_or("Language menu")?;
    let a = menu
        .entries
        .iter()
        .find_map(|entry| match entry {
            MenuEntry::Submenu {
                label: "A",
                entries,
                ..
            } => Some(entries),
            _ => None,
        })
        .ok_or("A group")?;
    assert_eq!(
        a.iter().map(MenuEntry::label).collect::<Vec<_>>(),
        [
            "ActionScript",
            "Ada",
            "ASN.1",
            "ASP",
            "Assembly",
            "AutoIt",
            "AviSynth"
        ]
    );
    fn collect(entries: &[MenuEntry], out: &mut Vec<(MenuCommand, String)>) {
        for entry in entries {
            match entry {
                MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => {
                    out.push((*command, entry.label().into()))
                }
                MenuEntry::Submenu { entries, .. } => collect(entries, out),
                _ => {}
            }
        }
    }
    let mut entries = Vec::new();
    collect(&menu.entries, &mut entries);
    assert!(entries.contains(&(
        MenuCommand::Language(Language::Builtin(58)),
        "JavaScript".into()
    )));
    assert!(entries.contains(&(
        MenuCommand::Language(Language::Builtin(67)),
        "Blitzbasic".into()
    )));
    assert!(entries.contains(&(
        MenuCommand::Language(Language::UserDefined(7)),
        "Team DSL".into()
    )));
    assert_eq!(
        entries
            .iter()
            .filter(|(command, _)| matches!(
                command,
                MenuCommand::Language(Language::NormalText | Language::Json | Language::Builtin(_))
            ))
            .count(),
        94
    );
    assert_eq!(
        entries
            .iter()
            .filter(|(command, _)| *command == MenuCommand::Language(Language::Json))
            .count(),
        1
    );
    assert!(!entries.iter().any(|(command, _)| matches!(
        command,
        MenuCommand::Language(Language::Builtin(0 | 57 | 47 | 19))
    )));
    assert_eq!(
        MenuCommand::Language(Language::Builtin(58)).source_id(),
        Some(46007)
    );
    assert_eq!(
        MenuCommand::Language(Language::Json).source_id(),
        Some(46057)
    );
    assert_eq!(
        MenuCommand::Language(Language::NormalText).source_id(),
        Some(46016)
    );
    Ok(())
}

#[test]
fn partial_builtin_refresh_preserves_static_json_and_rejects_retired_language_clicks()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::MenuEntry;
    use notepad_matrix::syntax::Language;
    let mut bar = MenuBar::default();
    let root = bar
        .definitions()
        .iter()
        .position(|menu| menu.label == "Language")
        .ok_or("Language root")?;
    let old_revision = bar.revision();
    bar.set_builtin_languages(vec![(81, "Rust".into())]);
    bar.handle(MenuEvent::OpenSubmenu("language-json-group"), &|_| true);
    let json_path = bar.selected_path().to_vec();
    assert_eq!(json_path.len(), 2);
    assert_eq!(
        bar.handle(
            MenuEvent::ClickAtRevision {
                root,
                path: json_path,
                revision: old_revision
            },
            &|_| true
        ),
        MenuOutcome::None
    );
    assert_eq!(
        bar.handle(MenuEvent::Activate, &|_| true),
        MenuOutcome::Activate(MenuCommand::Language(Language::Json))
    );
    let menu = &bar.definitions()[root];
    assert!(matches!(
        menu.entries.first(),
        Some(MenuEntry::Action(MenuCommand::Language(
            Language::NormalText
        )))
    ));
    Ok(())
}

#[test]
fn transformation_availability_distinguishes_line_block_and_readonly_capabilities() {
    use notepad_matrix::menu::TransformationAvailability;
    let normal = TransformationAvailability {
        editable: true,
        line_comment: false,
        block_comment: false,
    };
    for id in [42022, 42035, 42036, 42023, 42047] {
        assert!(!normal.allows(id));
    }
    let line = TransformationAvailability {
        line_comment: true,
        ..normal
    };
    for id in [42022, 42035, 42036] {
        assert!(line.allows(id));
    }
    for id in [42023, 42047] {
        assert!(!line.allows(id));
    }
    let block = TransformationAvailability {
        block_comment: true,
        ..normal
    };
    for id in [42022, 42035, 42036] {
        assert!(!block.allows(id));
    }
    for id in [42023, 42047] {
        assert!(block.allows(id));
    }
    let readonly = TransformationAvailability {
        editable: false,
        line_comment: true,
        block_comment: true,
    };
    for (_, id, _) in notepad_matrix::text_transforms::RETAINED_COMMANDS {
        assert!(!readonly.allows(*id));
    }
    assert!(normal.allows(42016));
    assert!(!normal.allows(99999));
}

#[test]
fn priority_menu_actions_activate_and_recheck_disabled_and_retired_clicks() {
    use notepad_matrix::menu::MenuEntry;
    fn paths(entries: &[MenuEntry], prefix: &[usize], out: &mut Vec<(Vec<usize>, MenuCommand)>) {
        for (index, entry) in entries.iter().enumerate() {
            let mut path = prefix.to_vec();
            path.push(index);
            match entry {
                MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => {
                    if command.source_id().is_some() {
                        out.push((path, *command));
                    }
                }
                MenuEntry::Submenu { entries, .. } => paths(entries, &path, out),
                _ => {}
            }
        }
    }
    let mut bar = MenuBar::default();
    bar.set_user_languages(vec![(77, "Replacement DSL".into())]);
    let mut all = Vec::new();
    for (root, definition) in bar.definitions().iter().enumerate() {
        let mut found = Vec::new();
        paths(&definition.entries, &[], &mut found);
        all.extend(
            found
                .into_iter()
                .map(|(path, command)| (root, path, command)),
        );
    }
    assert!(all.len() >= 180);
    for (root, path, command) in all {
        let allow = |_| true;
        bar.handle(MenuEvent::OpenRoot(root), &allow);
        for depth in 1..path.len() {
            bar.handle(
                MenuEvent::Hover {
                    root,
                    path: path[..depth].to_vec(),
                },
                &allow,
            );
        }
        let click = MenuEvent::ClickAtRevision {
            root,
            path: path.clone(),
            revision: bar.revision(),
        };
        assert_eq!(
            bar.handle(click.clone(), &|candidate| candidate != command),
            MenuOutcome::None,
            "disabled {command:?}"
        );
        assert_eq!(
            bar.handle(click.clone(), &allow),
            MenuOutcome::Activate(command),
            "reachable {command:?}"
        );
        bar.handle(MenuEvent::OpenRoot(root), &allow);
        for depth in 1..path.len() {
            bar.handle(
                MenuEvent::Hover {
                    root,
                    path: path[..depth].to_vec(),
                },
                &allow,
            );
        }
        let retired = MenuEvent::ClickAtRevision {
            root,
            path: path.clone(),
            revision: bar.revision(),
        };
        bar.set_user_languages(vec![(77, "Replacement DSL".into())]);
        bar.handle(MenuEvent::OpenRoot(root), &allow);
        for depth in 1..path.len() {
            bar.handle(
                MenuEvent::Hover {
                    root,
                    path: path[..depth].to_vec(),
                },
                &allow,
            );
        }
        assert_eq!(
            bar.handle(retired, &allow),
            MenuOutcome::None,
            "retired {command:?}"
        );
    }
}

#[test]
fn screenshot_view_and_language_menus_keep_sync_clone_and_udl_entrypoints() {
    use notepad_matrix::menu::MenuEntry;
    fn labels(entries: &[MenuEntry], out: &mut Vec<String>) {
        for entry in entries {
            out.push(entry.label().to_owned());
            if let MenuEntry::Submenu { entries, .. } = entry {
                labels(entries, out);
            }
        }
    }
    let bar = MenuBar::default();
    let mut all = Vec::new();
    for root in bar.definitions() {
        labels(&root.entries, &mut all);
    }
    for label in [
        "Synchronize Vertical Scrolling",
        "Synchronize Horizontal Scrolling",
        "Clone to Other View",
        "Move to Other View",
        "Define your language...",
    ] {
        assert!(
            all.iter().any(|entry| entry == label),
            "missing source command: {label}"
        );
    }
}

#[test]
fn saved_macro_names_follow_source_static_commands_and_execute_their_library_index()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::MenuEntry;

    let mut bar = MenuBar::default();
    bar.set_saved_macros(vec!["Insert α".into(), "Insert α".into(), "Trim".into()]);
    let root = bar
        .definitions()
        .iter()
        .position(|definition| definition.selector == "macro-menu-trigger")
        .ok_or("Macro root")?;
    let entries = &bar.definitions()[root].entries;
    assert_eq!(
        entries.iter().map(MenuEntry::label).collect::<Vec<_>>(),
        [
            "Start Recording",
            "Stop Recording",
            "Playback",
            "Save Current Recorded Macro...",
            "Run a Macro Multiple Times...",
            "",
            "Insert α",
            "Insert α",
            "Trim",
        ]
    );
    assert!(matches!(entries[5], MenuEntry::Separator));
    assert_eq!(entries[7].shortcut(), "");
    bar.handle(MenuEvent::OpenRoot(root), &|_| true);
    assert_eq!(
        bar.handle(
            MenuEvent::ClickAtRevision {
                root,
                path: vec![7],
                revision: bar.revision(),
            },
            &|_| true
        ),
        MenuOutcome::Activate(MenuCommand::SavedMacro(1))
    );
    bar.handle(MenuEvent::OpenRoot(root), &|_| true);
    bar.handle(MenuEvent::Last, &|_| true);
    assert_eq!(
        bar.handle(MenuEvent::Activate, &|_| true),
        MenuOutcome::Activate(MenuCommand::SavedMacro(2))
    );
    Ok(())
}

#[test]
fn saved_macro_refresh_rejects_retired_clicks_even_after_reopening_and_removing_items()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::MenuEntry;

    let mut bar = MenuBar::default();
    bar.set_saved_macros(vec!["Old name".into()]);
    let root = bar
        .definitions()
        .iter()
        .position(|definition| definition.selector == "macro-menu-trigger")
        .ok_or("Macro root")?;
    bar.handle(MenuEvent::OpenRoot(root), &|_| true);
    let retired = MenuEvent::ClickAtRevision {
        root,
        path: vec![6],
        revision: bar.revision(),
    };
    bar.set_saved_macros(vec!["Replacement".into()]);
    assert_eq!(bar.mode(), MenuMode::Closed);
    assert!(bar.selected_path().is_empty());
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root,
                path: vec![6]
            },
            &|_| true
        ),
        MenuOutcome::None
    );
    bar.handle(MenuEvent::OpenRoot(root), &|_| true);
    assert_eq!(bar.handle(retired.clone(), &|_| true), MenuOutcome::None);
    assert_eq!(
        bar.handle(
            MenuEvent::ClickAtRevision {
                root,
                path: vec![6],
                revision: bar.revision(),
            },
            &|_| true
        ),
        MenuOutcome::Activate(MenuCommand::SavedMacro(0))
    );
    bar.set_saved_macros(Vec::new());
    assert_eq!(bar.definitions()[root].entries.len(), 5);
    assert!(!matches!(
        bar.definitions()[root].entries.last(),
        Some(MenuEntry::Separator)
    ));
    bar.handle(MenuEvent::OpenRoot(root), &|_| true);
    assert_eq!(bar.handle(retired, &|_| true), MenuOutcome::None);
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root,
                path: vec![6]
            },
            &|_| true
        ),
        MenuOutcome::None
    );
    Ok(())
}

#[test]
fn saved_macro_click_and_keyboard_recheck_current_availability()
-> Result<(), Box<dyn std::error::Error>> {
    let mut bar = MenuBar::default();
    bar.set_saved_macros(vec!["First".into(), "Second".into()]);
    let root = bar
        .definitions()
        .iter()
        .position(|definition| definition.selector == "macro-menu-trigger")
        .ok_or("Macro root")?;
    bar.handle(MenuEvent::OpenRoot(root), &|_| true);
    bar.handle(MenuEvent::Last, &|_| true);
    let unavailable = |command| !matches!(command, MenuCommand::SavedMacro(_));
    assert_eq!(
        bar.handle(MenuEvent::Activate, &unavailable),
        MenuOutcome::None
    );
    assert_eq!(
        bar.handle(
            MenuEvent::ClickAtRevision {
                root,
                path: vec![7],
                revision: bar.revision(),
            },
            &unavailable
        ),
        MenuOutcome::None
    );
    let only_first = |command| !matches!(command, MenuCommand::SavedMacro(index) if index >= 1);
    bar.handle(MenuEvent::Last, &only_first);
    assert_eq!(
        bar.handle(MenuEvent::Activate, &only_first),
        MenuOutcome::Activate(MenuCommand::SavedMacro(0))
    );
    Ok(())
}

#[test]
fn window_menu_uses_macos_operations_and_keeps_document_list_in_view()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::{MenuEntry, ViewCommand};

    let mut bar = MenuBar::default();
    let index = bar
        .definitions()
        .iter()
        .position(|definition| definition.selector == "window-menu-trigger")
        .ok_or("Window root")?;
    let entries = &bar.definitions()[index].entries;
    assert_eq!(
        entries.iter().map(MenuEntry::label).collect::<Vec<_>>(),
        ["Minimize", "Zoom", "", "Bring All to Front", "Sort By"]
    );
    assert!(matches!(entries[2], MenuEntry::Separator));
    assert!(bar.definitions().iter().any(|definition| {
        definition.selector == "view-menu-trigger"
            && definition.entries.iter().any(|entry| {
                matches!(
                    entry,
                    MenuEntry::Action(MenuCommand::View(ViewCommand::DocumentList))
                )
            })
    }));
    bar.handle(MenuEvent::OpenRoot(index), &|_| true);
    let MenuOutcome::Activate(command) = bar.handle(MenuEvent::Mnemonic('m'), &|_| true) else {
        return Err("Minimize mnemonic did not activate".into());
    };
    assert_eq!(command.label(), "Minimize");
    assert_eq!(command.shortcut(), "Cmd+M");
    Ok(())
}

#[test]
fn help_menu_exposes_only_new_identity_about_without_updater()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::MenuEntry;

    let mut bar = MenuBar::default();
    let index = bar
        .definitions()
        .iter()
        .position(|definition| definition.selector == "help-menu-trigger")
        .ok_or("Help root")?;
    let help = &bar.definitions()[index];
    assert_eq!(help.label, "Help");
    assert_eq!(help.mnemonic, 'h');
    assert_eq!(
        help.entries
            .iter()
            .map(MenuEntry::label)
            .collect::<Vec<_>>(),
        ["About Notepad Matrix"]
    );
    bar.handle(MenuEvent::OpenRoot(index), &|_| true);
    let MenuOutcome::Activate(command) = bar.handle(MenuEvent::Mnemonic('a'), &|_| true) else {
        return Err("About mnemonic did not activate".into());
    };
    assert_eq!(command.label(), "About Notepad Matrix");
    assert_eq!(command.shortcut(), "");
    bar.set_embedded_tools_enabled(false, false, false);
    assert_eq!(
        bar.definitions().last().map(|root| root.label),
        Some("Help")
    );
    Ok(())
}

#[test]
fn view_menu_keeps_source_display_groups_shortcuts_and_summary()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::MenuEntry;
    let bar = MenuBar::default();
    let view = bar
        .definitions()
        .iter()
        .find(|root| root.selector == "view-menu-trigger")
        .ok_or("View root")?;
    assert_eq!(
        view.entries
            .iter()
            .map(MenuEntry::label)
            .collect::<Vec<_>>(),
        [
            "Show Symbol",
            "Zoom",
            "Word wrap",
            "Move/Clone Current Document",
            "Synchronize Vertical Scrolling",
            "Synchronize Horizontal Scrolling",
            "",
            "Summary...",
            "",
            "Document Map",
            "Markdown Preview",
            "Document List",
            "Function List",
            "Folder as Workspace",
            "Monitoring (tail -f)",
            "Tab",
            "Project Panels"
        ]
    );
    let MenuEntry::Submenu {
        entries: symbols, ..
    } = &view.entries[0]
    else {
        return Err("Show Symbol submenu".into());
    };
    assert_eq!(
        MenuCommand::View(notepad_matrix::menu::ViewCommand::MarkdownPreview).source_id(),
        None
    );
    assert_eq!(
        symbols.iter().map(MenuEntry::label).collect::<Vec<_>>(),
        ["Show All Characters", "", "Show Indent Guide"]
    );
    let MenuEntry::Submenu { entries: zoom, .. } = &view.entries[1] else {
        return Err("Zoom submenu".into());
    };
    assert_eq!(
        zoom.iter().map(MenuEntry::label).collect::<Vec<_>>(),
        [
            "Zoom In (Ctrl+Mouse Wheel Up)",
            "Zoom Out (Ctrl+Mouse Wheel Down)",
            "Restore Default Zoom"
        ]
    );
    let shortcuts = zoom
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Action(command) => Some(command.shortcut()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shortcuts, ["Ctrl+Num +", "Ctrl+Num -", "Ctrl+Num /"]);
    for entry in symbols.iter().chain(&view.entries[2..]) {
        if let MenuEntry::Action(command) = entry {
            assert_eq!(command.shortcut(), "");
        }
    }
    Ok(())
}

#[test]
fn status_eol_opens_existing_conversion_submenu_and_respects_read_only()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::input::LineEnding;
    use notepad_matrix::menu::EditCommand;
    let mut bar = MenuBar::default();
    assert_eq!(
        bar.handle(MenuEvent::OpenSubmenu("edit-menu-eol"), &|_| true),
        MenuOutcome::None
    );
    assert_eq!(bar.popup_count(), 2);
    assert_eq!(
        bar.popup_entries(1)
            .ok_or("EOL submenu")?
            .iter()
            .map(|entry| entry.label())
            .collect::<Vec<_>>(),
        ["Windows (CR LF)", "Unix (LF)", "Macintosh (CR)"]
    );
    bar.handle(MenuEvent::Down, &|_| true);
    assert_eq!(
        bar.handle(MenuEvent::Activate, &|_| true),
        MenuOutcome::Activate(MenuCommand::Edit(EditCommand::ConvertEol(LineEnding::Lf)))
    );
    bar.handle(MenuEvent::OpenSubmenu("edit-menu-eol"), &|command| {
        !matches!(command, MenuCommand::Edit(EditCommand::ConvertEol(_)))
    });
    assert_eq!(bar.mode(), MenuMode::Closed);
    Ok(())
}

#[test]
fn menu_document_controls_keep_source_order_and_reuse_existing_commands() {
    use notepad_matrix::menu::DocumentControl;
    assert_eq!(
        DocumentControl::ALL.map(|control| control.glyph()),
        ["＋", "▼", "✕"]
    );
    assert_eq!(
        DocumentControl::ALL.map(|control| control.command()),
        [
            MenuCommand::File(FileCommand::New),
            MenuCommand::DocumentDropdown,
            MenuCommand::File(FileCommand::Close),
        ]
    );
    assert_eq!(DocumentControl::DocumentList.label(), "Document list");
}

#[test]
fn menus_retain_plain_clipboard_and_html_file_export_without_excluded_commands() {
    use notepad_matrix::menu::{EditCommand, MenuEntry};

    fn commands(entries: &[MenuEntry]) -> Vec<MenuCommand> {
        entries
            .iter()
            .flat_map(|entry| match entry {
                MenuEntry::Action(command) | MenuEntry::DynamicAction { command, .. } => {
                    vec![*command]
                }
                MenuEntry::Submenu { entries, .. } => commands(entries),
                MenuEntry::Separator => Vec::new(),
            })
            .collect()
    }

    let bar = MenuBar::default();
    let all = bar
        .definitions()
        .iter()
        .flat_map(|definition| commands(&definition.entries))
        .collect::<Vec<_>>();
    for command in [EditCommand::Cut, EditCommand::Copy, EditCommand::Paste] {
        assert!(all.contains(&MenuCommand::Edit(command)));
    }
    assert!(all.contains(&MenuCommand::ExportHtml));
    for label in [
        "Copy HTML to clipboard",
        "Copy all formats to clipboard",
        "Paste HTML Content",
        "Clipboard History",
    ] {
        assert!(
            !all.iter().any(|command| command.label() == label),
            "{label}"
        );
    }
    let export = bar
        .definitions()
        .iter()
        .flat_map(|definition| &definition.entries)
        .find_map(|entry| match entry {
            MenuEntry::Submenu {
                selector: "npp-export-menu",
                entries,
                ..
            } => Some(commands(entries)),
            _ => None,
        });
    assert_eq!(export, Some(vec![MenuCommand::ExportHtml]));
}

#[test]
fn one_conditional_parent_keeps_left_right_and_exit_paths_stable() {
    use notepad_matrix::menu::{DocumentCommand, MenuEntry};
    use notepad_matrix::workspace::CloseCondition;
    let mut bar = MenuBar::default();
    let enabled = |_| true;
    assert_eq!(bar.definitions()[0].entries.len(), 14);
    let MenuEntry::Submenu {
        selector, entries, ..
    } = &bar.definitions()[0].entries[9]
    else {
        panic!("conditional submenu missing");
    };
    assert_eq!(*selector, "file-menu-close-multiple");
    assert_eq!(
        entries.iter().map(MenuEntry::label).collect::<Vec<_>>(),
        [
            "Close All but Active Document",
            "Close All but Pinned Documents",
            "Close All to the Left",
            "Close All to the Right",
            "Close All Unchanged",
        ]
    );
    for (index, condition, id) in [
        (0, CloseCondition::ExceptCurrent, 41005),
        (1, CloseCondition::ExceptPinned, 41026),
        (4, CloseCondition::Unchanged, 41024),
    ] {
        let command = MenuCommand::Document(DocumentCommand::Close(condition));
        assert!(matches!(entries[index], MenuEntry::Action(actual) if actual == command));
        assert_eq!(command.source_id(), Some(id));
        assert_eq!(command.shortcut(), "");
    }
    assert!(matches!(
        entries[2],
        MenuEntry::Action(MenuCommand::File(FileCommand::CloseLeft))
    ));
    assert!(matches!(
        entries[3],
        MenuEntry::Action(MenuCommand::File(FileCommand::CloseRight))
    ));
    assert!(matches!(
        bar.definitions()[0].entries[bar.definitions()[0].entries.len() - 1],
        MenuEntry::Action(MenuCommand::File(FileCommand::Exit))
    ));
    bar.handle(MenuEvent::OpenRoot(0), &enabled);
    bar.handle(
        MenuEvent::Hover {
            root: 0,
            path: vec![9],
        },
        &enabled,
    );
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 0,
                path: vec![9, 3]
            },
            &enabled
        ),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::CloseRight))
    );
    let no_right = |command| command != MenuCommand::File(FileCommand::CloseRight);
    bar.handle(MenuEvent::OpenRoot(0), &no_right);
    bar.handle(
        MenuEvent::Hover {
            root: 0,
            path: vec![9],
        },
        &no_right,
    );
    assert_eq!(bar.selected_path(), [9, 0]);
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 0,
                path: vec![9, 3]
            },
            &no_right
        ),
        MenuOutcome::None
    );
    assert_eq!(bar.selected_path(), [9, 0]);
}

#[test]
fn generic_nested_navigation_skips_disabled_rows_and_escapes_one_level_at_a_time() {
    use notepad_matrix::input::LineEnding;
    use notepad_matrix::menu::{
        EditCommand, MenuBar, MenuCommand, MenuEvent, MenuMode, MenuOutcome,
    };
    let enabled = |_| true;
    let mut bar = MenuBar::default();
    let eol = bar.definitions()[1]
        .entries
        .iter()
        .position(|entry| {
            matches!(
                entry,
                notepad_matrix::menu::MenuEntry::Submenu {
                    selector: "edit-menu-eol",
                    ..
                }
            )
        })
        .expect("EOL submenu");
    bar.handle(MenuEvent::OpenSubmenu("edit-menu-eol"), &enabled);
    assert_eq!(bar.selected_path(), [eol, 0]);
    bar.handle(MenuEvent::Down, &enabled);
    assert_eq!(
        bar.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::Activate(MenuCommand::Edit(EditCommand::ConvertEol(LineEnding::Lf)))
    );
    bar.handle(MenuEvent::OpenSubmenu("edit-menu-eol"), &enabled);
    bar.handle(MenuEvent::Escape, &enabled);
    assert_eq!(bar.selected_path(), [eol]);
    assert_eq!(bar.popup_count(), 1);
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 1,
                path: vec![eol, 1]
            },
            &enabled
        ),
        MenuOutcome::None,
        "closed submenu accepted a stale click"
    );
    bar.handle(MenuEvent::Escape, &enabled);
    assert_eq!(bar.mode(), MenuMode::BarFocused);
    bar.handle(MenuEvent::Escape, &enabled);
    assert_eq!(bar.mode(), MenuMode::Closed);
    let only_lf = |command| command == MenuCommand::Edit(EditCommand::ConvertEol(LineEnding::Lf));
    bar.handle(MenuEvent::OpenRoot(1), &only_lf);
    assert_eq!(bar.selected_path(), [eol]);
    bar.handle(MenuEvent::Right, &only_lf);
    assert_eq!(bar.selected_path(), [eol, 1]);
    bar.handle(MenuEvent::Up, &only_lf);
    assert_eq!(bar.selected_path(), [eol, 1]);
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 1,
                path: vec![eol, 0]
            },
            &only_lf
        ),
        MenuOutcome::None
    );
    assert_eq!(bar.selected_path(), [eol, 1]);
    bar.handle(MenuEvent::OpenRoot(1), &|_| false);
    assert!(bar.selected_path().is_empty());
    bar.handle(MenuEvent::Down, &|_| false);
    assert_eq!(
        bar.handle(MenuEvent::Activate, &|_| false),
        MenuOutcome::None
    );
    assert_eq!(bar.mode(), MenuMode::Open);
}

#[test]
fn generic_roots_mnemonics_and_edges_preserve_command_identity() {
    use notepad_matrix::menu::{EditCommand, MenuBar, MenuCommand, MenuEvent, MenuOutcome};
    let enabled = |_| true;
    let mut bar = MenuBar::default();
    bar.handle(MenuEvent::ToggleBar, &enabled);
    bar.handle(MenuEvent::Mnemonic('E'), &enabled);
    assert_eq!(bar.active_root(), 1);
    assert_eq!(
        bar.handle(MenuEvent::Mnemonic('u'), &enabled),
        MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Undo))
    );
    bar.handle(MenuEvent::OpenRoot(0), &enabled);
    bar.handle(MenuEvent::Right, &enabled);
    assert_eq!(bar.active_root(), 1);
    bar.handle(MenuEvent::Left, &enabled);
    assert_eq!(bar.active_root(), 0);
    bar.handle(MenuEvent::Last, &enabled);
    assert_eq!(
        bar.selected_path(),
        [bar.definitions()[0].entries.len() - 1]
    );
    bar.handle(MenuEvent::First, &enabled);
    assert_eq!(bar.selected_path(), [0]);
    assert_eq!(
        bar.handle(MenuEvent::Mnemonic('s'), &enabled),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::Save))
    );
}

#[test]
fn generic_menu_rejects_stale_invalid_and_disabled_clicks_atomically() {
    use notepad_matrix::menu::{
        EditCommand, MenuBar, MenuCommand, MenuEvent, MenuMode, MenuOutcome,
    };
    let mut bar = MenuBar::default();
    let exit = bar.definitions()[0].entries.len() - 1;
    let enabled = |_| true;
    bar.handle(MenuEvent::OpenRoot(0), &enabled);
    bar.handle(
        MenuEvent::Hover {
            root: 0,
            path: vec![exit],
        },
        &enabled,
    );
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 0,
                path: vec![999]
            },
            &enabled
        ),
        MenuOutcome::None
    );
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 0,
                path: vec![exit]
            },
            &|command| command != MenuCommand::File(FileCommand::Exit)
        ),
        MenuOutcome::None
    );
    assert_eq!(bar.mode(), MenuMode::Open);
    bar.handle(MenuEvent::OpenRoot(1), &enabled);
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 0,
                path: vec![exit]
            },
            &enabled
        ),
        MenuOutcome::None
    );
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 1,
                path: vec![0]
            },
            &enabled
        ),
        MenuOutcome::Activate(MenuCommand::Edit(EditCommand::Undo))
    );
    assert_eq!(bar.mode(), MenuMode::Closed);
    assert_eq!(
        bar.handle(
            MenuEvent::Click {
                root: 1,
                path: vec![0]
            },
            &enabled
        ),
        MenuOutcome::None
    );
}

#[test]
fn save_as_shortcut_matches_notepad_plus_plus_default() {
    assert_eq!(FileCommand::SaveAs.shortcut(), "Ctrl+Alt+S");
    assert_eq!(FileCommand::SaveAll.shortcut(), "Ctrl+Shift+S");
    assert_eq!(FileCommand::SaveCopy.shortcut(), "");
    assert_eq!(FileCommand::Close.shortcut(), "Ctrl+W");
    assert_eq!(FileCommand::CloseAll.shortcut(), "Ctrl+Shift+W");
    assert_eq!(FileCommand::Reload.shortcut(), "Ctrl+R");
}

#[test]
fn closed_menu_cannot_execute_exit_and_bar_activation_opens_file() {
    let mut menu = MenuBar::default();
    let enabled = |_| true;
    assert_eq!(menu.mode(), MenuMode::Closed);
    assert_eq!(
        menu.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::None
    );
    menu.handle(MenuEvent::ToggleBar, &enabled);
    assert_eq!(menu.mode(), MenuMode::BarFocused);
    assert_eq!(
        menu.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::None
    );
    assert_eq!(menu.mode(), MenuMode::Open);
    assert_eq!(menu.active_root(), 0);
    assert_eq!(
        menu.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::New))
    );
}

#[test]
fn file_command_navigation_selects_real_commands_and_last_item_exits() {
    let mut menu = MenuBar::default();
    let enabled = |_| true;
    menu.handle(MenuEvent::OpenRoot(0), &enabled);
    assert_eq!(menu.selected_path(), [0]);
    menu.handle(MenuEvent::Down, &enabled);
    assert_eq!(
        menu.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::Open))
    );
    menu.handle(MenuEvent::OpenRoot(0), &enabled);
    assert_eq!(
        menu.handle(
            MenuEvent::Click {
                root: 0,
                path: vec![3]
            },
            &enabled
        ),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::SaveAs))
    );
    menu.handle(MenuEvent::OpenRoot(0), &enabled);
    menu.handle(MenuEvent::Last, &enabled);
    assert_eq!(
        menu.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::Exit))
    );
    assert_eq!(menu.mode(), MenuMode::Closed);
}

#[test]
fn escape_closes_popup_then_menu_mode_and_outside_click_dismisses() {
    let mut menu = MenuBar::default();
    let enabled = |_| true;
    menu.handle(MenuEvent::OpenRoot(0), &enabled);
    assert_eq!(menu.mode(), MenuMode::Open);
    menu.handle(MenuEvent::Escape, &enabled);
    assert_eq!(menu.mode(), MenuMode::BarFocused);
    menu.handle(MenuEvent::Escape, &enabled);
    assert_eq!(menu.mode(), MenuMode::Closed);
    menu.handle(MenuEvent::ToggleRoot(0), &enabled);
    menu.handle(MenuEvent::Dismiss, &enabled);
    assert_eq!(menu.mode(), MenuMode::Closed);
}

#[test]
fn one_root_horizontal_navigation_keeps_exit_selected() {
    let mut menu = MenuBar::new(vec![MenuDefinition::file_menu()]);
    let enabled = |_| true;
    menu.handle(MenuEvent::OpenRoot(0), &enabled);
    menu.handle(MenuEvent::Last, &enabled);
    for event in [MenuEvent::Left, MenuEvent::Right] {
        menu.handle(event, &enabled);
        assert_eq!(menu.active_root(), 0);
        assert_eq!(menu.mode(), MenuMode::Open);
        assert_eq!(menu.selected_path(), [10]);
    }
    assert_eq!(
        menu.handle(MenuEvent::Activate, &enabled),
        MenuOutcome::Activate(MenuCommand::File(FileCommand::Exit))
    );
}
