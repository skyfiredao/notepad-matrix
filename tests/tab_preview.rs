use notepad_matrix::workspace;

#[path = "../src/ui/tab_preview_model.rs"]
mod model;

use model::{DocumentPreview, PreviewHover, PreviewLocation};
use workspace::{BufferId, ViewId, Workspace};

#[test]
fn source_preview_suppresses_documents_active_in_either_pane() {
    let mut workspace = Workspace::new();
    let first = workspace.active().id();
    let location = PreviewLocation {
        pane: ViewId::Primary,
        id: first,
    };
    assert!(!location.allowed(&workspace));
    workspace.new_buffer().expect("second");
    assert!(location.allowed(&workspace));
    workspace
        .clone_to_other_view(first)
        .expect("clone into other pane");
    assert!(!location.allowed(&workspace));
}

#[test]
fn source_preview_hides_on_tab_leave_and_close_suppresses_reopening() {
    let target = PreviewLocation {
        pane: ViewId::Primary,
        id: BufferId(1),
    };
    let other = PreviewLocation {
        pane: ViewId::Primary,
        id: BufferId(2),
    };
    let mut hover = PreviewHover::default();
    hover.tab(Some(target));
    assert_eq!(hover.target(), Some(target));
    hover.tab(None);
    assert_eq!(hover.target(), None);
    hover.tab(Some(target));
    hover.dismiss();
    assert_eq!(hover.target(), None);
    hover.tab(Some(target));
    assert_eq!(hover.target(), None);
    hover.tab(Some(other));
    assert_eq!(hover.target(), Some(other));
    hover.tab(None);
    assert_eq!(hover.target(), None);
    hover.tab(Some(target));
    assert_eq!(hover.target(), Some(target));
    hover.tab(None);
    assert_eq!(hover.target(), None);
}

#[test]
fn preview_reads_target_membership_without_activation_selection_or_edits() {
    let mut workspace = Workspace::new();
    let original = workspace.active().id();
    workspace
        .active_mut()
        .editor
        .insert_text("first\nsecond\nthird")
        .expect("edit");
    let selection = workspace.active().editor.selection();
    let revision = workspace.active().editor.display_revision();
    workspace.new_buffer().expect("second");
    let active = workspace.active().id();
    let preview = DocumentPreview::capture(&workspace, ViewId::Primary, original).expect("preview");
    assert_eq!(preview.line(1).as_deref(), Some("second"));
    assert_eq!(preview.line_count(), 3);
    assert_eq!(workspace.active().id(), active);
    let editor = workspace
        .editor_in_view(ViewId::Primary, original)
        .expect("original");
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.display_revision(), revision);
    assert!(DocumentPreview::capture(&workspace, ViewId::Secondary, original).is_none());
}
