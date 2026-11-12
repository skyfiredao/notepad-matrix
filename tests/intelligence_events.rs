use notepad_matrix::{editor, input, intelligence, languages, syntax, workspace};

use intelligence::events::*;

#[test]
fn source_auto_insert_pairs_quotes_and_xml_share_committed_editing()
-> Result<(), Box<dyn std::error::Error>> {
    use intelligence::auto_insert::{Settings as PairSettings, Tracker};
    let settings = PairSettings::from_json(
        &serde_json::json!({"auto-insert":{"parentheses":true,"quotes":true,"doubleQuotes":true,"htmlXmlTag":true}}),
    );
    let mut editor = input::EditorSession::new("");
    let mut tracker = Tracker::default();
    for character in "(\"x\")".chars() {
        editor.type_text(&character.to_string())?;
        tracker.after_character(
            &mut editor,
            character,
            &settings,
            syntax::Language::NormalText,
        )?;
    }
    assert_eq!(editor.committed_text(), "(\"x\")");
    let xml = languages::catalog()?
        .by_name("xml")
        .ok_or("XML profile")?
        .id;
    let mut editor = input::EditorSession::new("<br");
    editor.select(editor::Selection {
        anchor: editor::ByteOffset(3),
        head: editor::ByteOffset(3),
    })?;
    editor.type_text(">")?;
    tracker.after_character(&mut editor, '>', &settings, syntax::Language::Builtin(xml))?;
    assert_eq!(editor.committed_text(), "<br></br>");
    assert!(!PairSettings::default().enabled());
    Ok(())
}
use intelligence::*;

fn snapshot(editor: &input::EditorSession, view: workspace::ViewId) -> Snapshot {
    Snapshot {
        buffer: workspace::BufferId(1),
        view,
        revision: editor.display_revision(),
        selection: editor.selection(),
        language: syntax::Language::Builtin(22),
    }
}

#[test]
fn completion_events_submit_to_the_original_view_and_reject_stale_results() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let mut editor = input::EditorSession::new("printer print\npri");
    let end = editor.document().len_bytes();
    editor
        .select(editor::Selection {
            anchor: editor::ByteOffset(end),
            head: editor::ByteOffset(end),
        })
        .expect("caret");
    let owner = snapshot(&editor, workspace::ViewId::Primary);
    let result = catalog
        .complete(
            "python",
            &editor.committed_text(),
            end,
            &CompletionOptions {
                mode: CompletionMode::Words,
                ..CompletionOptions::default()
            },
        )
        .expect("completion");
    let mut model = IntelligenceModel::default();
    assert!(model.accept_completion(owner.clone(), &owner, result));
    assert_eq!(
        model.completion().expect("popup").result.candidates[0].text,
        "print"
    );
    assert!(
        model
            .handle(Action::Down, &owner, &SubmitOptions::default())
            .consumed
    );
    let submitted = model.handle(Action::Enter, &owner, &SubmitOptions::default());
    assert!(submitted.consumed);
    assert_eq!(submitted.edit.expect("edit").text, "printer");
    assert!(
        !model
            .handle(Action::Enter, &owner, &SubmitOptions::default())
            .consumed
    );
    let result = catalog
        .complete(
            "python",
            &editor.committed_text(),
            end,
            &CompletionOptions::default(),
        )
        .expect("completion");
    let other = snapshot(&editor, workspace::ViewId::Secondary);
    assert!(!model.accept_completion(owner.clone(), &other, result));
    assert!(!model.completion_visible());
    let old = model.begin_request(owner.clone());
    let new = model.begin_request(owner.clone());
    assert!(!model.finish_request(&old, &owner));
    assert!(model.finish_request(&new, &owner));
    let cancelled = model.begin_request(owner.clone());
    assert!(
        !model
            .handle(Action::Escape, &owner, &SubmitOptions::default())
            .consumed
    );
    assert!(!model.finish_request(&cancelled, &owner));
}

#[test]
fn settings_follow_source_defaults_and_source_attribute_overrides() {
    let defaults = Settings::from_json(&serde_json::json!({}));
    assert_eq!(defaults.automatic, Some(CompletionMode::ApiAndWords));
    assert_eq!(defaults.completion.minimum_prefix, 1);
    assert!(defaults.completion.ignore_numbers);
    assert!(defaults.submit.enter && defaults.submit.tab && defaults.function_parameters);
    assert!(!defaults.brief);
    let settings = Settings::from_json(
        &serde_json::json!({"auto-completion":{"autoCAction":2,"triggerFromNbChar":15,"autoCIgnoreNumbers":false,"insertSelectedItemUseENTER":false,"insertSelectedItemUseTAB":true,"funcParams":false,"autoCBrief":true}}),
    );
    assert_eq!(settings.automatic, Some(CompletionMode::Words));
    assert_eq!(settings.completion.minimum_prefix, 9);
    assert!(!settings.completion.ignore_numbers);
    assert!(!settings.submit.enter && settings.submit.tab);
    assert!(!settings.function_parameters);
    assert!(settings.brief);
}

#[test]
fn nested_calltip_events_cycle_overloads_cancel_and_leave_enter_unconsumed() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "printf(\"x,(y)\", strlen(\"ab\"), ";
    let editor = input::EditorSession::new(text);
    let owner = snapshot(&editor, workspace::ViewId::Primary);
    let help = catalog
        .signature_help("c", text, text.len(), 0)
        .expect("help");
    let mut model = IntelligenceModel::default();
    assert!(model.accept_calltip(owner.clone(), &owner, help));
    let help = &model.calltip().expect("calltip").help;
    assert_eq!(help.function_name, "printf");
    assert_eq!(help.active_parameter, 2);
    assert!(
        !model
            .handle(Action::Enter, &owner, &SubmitOptions::default())
            .consumed
    );
    assert!(
        model
            .handle(Action::NextOverload, &owner, &SubmitOptions::default())
            .consumed
    );
    assert!(
        model
            .handle(Action::Escape, &owner, &SubmitOptions::default())
            .consumed
    );
    assert!(!model.calltip_visible());
    assert!(
        !model
            .handle(Action::Escape, &owner, &SubmitOptions::default())
            .consumed
    );
}
