use notepad_matrix::workspace;
use workspace::{CloseDisposition, ViewId, Workspace};

use closing_model::CloseBatch;
use workspace::closing as closing_model;

#[test]
fn selected_close_batch_prompts_once_when_all_clones_close_and_keeps_approved_prefix()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    workspace.active_mut().editor.insert_text("dirty")?;
    let shared = workspace.active().id();
    workspace.clone_to_other_view(shared)?;
    let targets = vec![(ViewId::Primary, shared), (ViewId::Secondary, shared)];
    let mut batch = CloseBatch::selected(targets.clone());
    assert_eq!(batch.next(), Some(shared));
    assert_eq!(batch.automatic_disposition(&workspace, shared), None);
    batch.approve(shared, CloseDisposition::DiscardConfirmed);
    assert_eq!(batch.next(), None);
    assert_eq!(
        batch
            .finish(true, ViewId::Primary)
            .iter()
            .map(|target| (target.view, target.id))
            .collect::<Vec<_>>(),
        targets
    );
    let one_clone = CloseBatch::selected(vec![(ViewId::Primary, shared)]);
    assert_eq!(
        one_clone.automatic_disposition(&workspace, shared),
        Some(CloseDisposition::IfUnmodified)
    );
    Ok(())
}

#[test]
fn named_empty_dirty_document_still_requires_approval() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("empty.txt");
    std::fs::write(&path, "")?;
    let (file, text) = notepad_matrix::files::DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    workspace
        .active_mut()
        .select_format(notepad_matrix::files::UnicodeFormat::Utf16LeBom);
    assert_eq!(
        closing_model::individual_close_disposition(&workspace, ViewId::Primary, id),
        None
    );
    assert_eq!(
        CloseBatch::all(&workspace).automatic_disposition(&workspace, id),
        None
    );
    Ok(())
}

#[test]
fn closing_clone_preserves_saved_body_history_and_shared_savepoint()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::files::DiskFile;
    use workspace::SaveCompletion;
    let home = tempfile::tempdir()?;
    let path = home.path().join("saved.txt");
    std::fs::write(&path, "A")?;
    let (file, text) = DiskFile::load_unicode(&path)?;
    let mut workspace = Workspace::new();
    let id = workspace.open_buffer(file, &text)?;
    workspace.active_mut().editor.insert_text("X")?;
    workspace.clone_to_other_view(id)?;
    let intent = workspace.active().save_intent();
    let file = workspace
        .active()
        .disk_file
        .as_ref()
        .ok_or("missing save target")?
        .save_with_format(intent.snapshot(), intent.format())?;
    workspace.complete_save(&intent, file, SaveCompletion::Primary)?;
    let disposition =
        closing_model::individual_close_disposition(&workspace, ViewId::Secondary, id)
            .ok_or("retained clone must need no question")?;
    workspace.close_in_view(ViewId::Secondary, id, disposition)?;
    assert_eq!(workspace.active().id(), id);
    assert!(!workspace.active().is_modified());
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.document().text(), "A");
    assert!(workspace.active().is_modified());
    assert!(workspace.active_mut().editor.redo());
    assert_eq!(workspace.active().editor.document().text(), "XA");
    assert!(!workspace.active().is_modified());
    assert_eq!(std::fs::read_to_string(path)?, "XA");
    Ok(())
}

#[test]
fn empty_untitled_bypasses_prompt_even_when_format_savepoint_is_dirty() {
    use notepad_matrix::files::{UnicodeFormat, Utf8Format};
    for format in [
        UnicodeFormat::Utf8(Utf8Format::Bom),
        UnicodeFormat::Utf16LeBom,
        UnicodeFormat::Utf16BeBom,
    ] {
        let mut workspace = Workspace::new();
        let id = workspace.active().id();
        assert!(workspace.active_mut().select_format(format));
        assert!(workspace.active().is_modified());
        assert!(!workspace.active().editor.is_modified());
        assert_eq!(workspace.active().editor.document().len_bytes(), 0);
        assert_eq!(
            closing_model::individual_close_disposition(&workspace, ViewId::Primary, id),
            Some(CloseDisposition::DiscardConfirmed)
        );
        for batch in [
            CloseBatch::all(&workspace),
            CloseBatch::conditional(ViewId::Primary, vec![id]),
        ] {
            assert_eq!(
                batch.automatic_disposition(&workspace, id),
                Some(CloseDisposition::DiscardConfirmed)
            );
        }
    }
}

#[test]
fn conditional_cancel_closes_only_approved_prefix_in_its_original_view() {
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    let second = workspace.new_buffer().unwrap();
    let mut batch = CloseBatch::conditional(ViewId::Primary, vec![second, first]);
    assert_eq!(batch.next(), Some(second));
    batch.approve(second, CloseDisposition::IfUnmodified);
    assert_eq!(batch.next(), Some(first));
    let targets = batch.finish(true, ViewId::Secondary);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].view, ViewId::Primary);
    assert_eq!(targets[0].id, second);
}

#[test]
fn close_all_prompts_once_per_clone_and_closes_noncurrent_view_first() {
    let mut workspace = Workspace::new();
    let cloned = workspace.active().id();
    workspace.active_mut().editor.insert_text("shared").unwrap();
    workspace.clone_to_other_view(cloned).unwrap();
    assert_eq!(
        CloseBatch::all(&workspace).automatic_disposition(&workspace, cloned),
        None
    );
    let secondary_only = workspace.new_buffer().unwrap();
    let mut batch = CloseBatch::all(&workspace);
    assert_eq!(batch.prompt_view(cloned), ViewId::Secondary);
    assert_eq!(batch.next(), Some(cloned));
    batch.approve(cloned, CloseDisposition::DiscardConfirmed);
    assert_eq!(batch.next(), Some(secondary_only));
    batch.approve(secondary_only, CloseDisposition::IfUnmodified);
    assert_eq!(batch.next(), None);
    let targets = batch.finish(false, ViewId::Secondary);
    assert_eq!(
        targets
            .iter()
            .map(|target| (target.view, target.id))
            .collect::<Vec<_>>(),
        vec![
            (ViewId::Primary, cloned),
            (ViewId::Secondary, secondary_only),
            (ViewId::Secondary, cloned),
        ]
    );
    for target in targets {
        workspace
            .close_in_view(target.view, target.id, target.disposition)
            .unwrap();
    }
    assert_eq!(workspace.active_view(), ViewId::Secondary);
    assert!(workspace.view_buffer_ids(ViewId::Primary).is_empty());
    assert_eq!(workspace.view_buffer_ids(ViewId::Secondary).len(), 1);
    assert_eq!(workspace.active().editor.document().text(), "");
}

#[test]
fn current_and_conditional_close_skip_dirty_clone_prompt_and_preserve_peer_undo() {
    let mut workspace = Workspace::new();
    let id = workspace.active().id();
    workspace
        .active_mut()
        .editor
        .insert_text("shared edit")
        .unwrap();
    workspace.clone_to_other_view(id).unwrap();
    assert_eq!(
        closing_model::individual_close_disposition(&workspace, ViewId::Secondary, id),
        Some(CloseDisposition::IfUnmodified)
    );
    let mut batch = CloseBatch::conditional(ViewId::Secondary, vec![id]);
    assert_eq!(batch.next(), Some(id));
    assert_eq!(
        batch.automatic_disposition(&workspace, id),
        Some(CloseDisposition::IfUnmodified)
    );
    batch.approve(id, CloseDisposition::IfUnmodified);
    let targets = batch.finish(false, workspace.active_view());
    assert_eq!(targets.len(), 1);
    workspace
        .close_in_view(targets[0].view, id, targets[0].disposition)
        .unwrap();
    assert_eq!(workspace.active().editor.document().text(), "shared edit");
    assert!(workspace.active().is_modified());
    assert_eq!(
        CloseBatch::conditional(ViewId::Primary, vec![id]).automatic_disposition(&workspace, id),
        None
    );
    assert!(workspace.active_mut().editor.undo());
    assert_eq!(workspace.active().editor.document().text(), "");
}

#[test]
fn close_all_cancel_keeps_clean_and_approved_dirty_memberships() {
    let mut workspace = Workspace::new();
    let clean = workspace.active().id();
    let dirty = workspace.new_buffer().unwrap();
    workspace
        .active_mut()
        .editor
        .insert_text("unsaved")
        .unwrap();
    workspace.clone_to_other_view(dirty).unwrap();
    let before = [
        workspace.view_buffer_ids(ViewId::Primary).to_vec(),
        workspace.view_buffer_ids(ViewId::Secondary).to_vec(),
    ];
    let mut batch = CloseBatch::all(&workspace);
    assert_eq!(batch.next(), Some(clean));
    batch.approve(clean, CloseDisposition::IfUnmodified);
    assert_eq!(batch.next(), Some(dirty));
    batch.approve(dirty, CloseDisposition::DiscardConfirmed);
    assert!(batch.finish(true, workspace.active_view()).is_empty());
    assert_eq!(workspace.view_buffer_ids(ViewId::Primary), before[0]);
    assert_eq!(workspace.view_buffer_ids(ViewId::Secondary), before[1]);
}
