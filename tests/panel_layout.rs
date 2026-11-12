#[path = "../src/panel_layout.rs"]
mod panel_layout;

use panel_layout::{DockSide, FloatBounds, PanelId, PanelLayout, Placement};

#[test]
fn retained_panels_restore_hidden_floating_geometry_and_dock_order() {
    let mut layout = PanelLayout::default();
    layout.show(PanelId::DocumentList);
    layout.dock(PanelId::DocumentList, DockSide::Right, 1);
    layout.resize(PanelId::DocumentList, 321.0).expect("resize");
    let bounds = FloatBounds {
        x: 41.0,
        y: 62.0,
        width: 360.0,
        height: 480.0,
    };
    layout.float(PanelId::DocumentList, bounds).expect("float");
    layout
        .set_bounds(PanelId::DocumentList, bounds)
        .expect("native bounds");
    layout.hide(PanelId::DocumentList);
    let restored = PanelLayout::from_json(&layout.to_json()).expect("restore");
    let state = restored.panel(PanelId::DocumentList);
    assert!(!state.visible);
    assert_eq!(state.placement, Placement::Floating);
    assert_eq!(state.bounds, bounds);
    assert_eq!(state.size, 321.0);
    let mut restored = restored;
    restored.show(PanelId::DocumentList);
    restored.redock(PanelId::DocumentList);
    assert_eq!(
        restored.panel(PanelId::DocumentList).placement,
        Placement::Docked(DockSide::Right)
    );
    assert_eq!(PanelId::ALL.len(), 8);
    assert_eq!(PanelId::Folder.title(), "Folder as Workspace");
    assert_eq!(DockSide::ALL.len(), 4);
}

#[test]
fn docking_reorders_retained_panels_and_rejects_corrupt_dimensions() {
    let mut layout = PanelLayout::default();
    layout.show(PanelId::DocumentList);
    layout.show(PanelId::Folder);
    layout.show(PanelId::FunctionList);
    layout.dock(PanelId::FunctionList, DockSide::Left, 0);
    assert_eq!(
        layout.docked(DockSide::Left),
        vec![
            PanelId::FunctionList,
            PanelId::DocumentList,
            PanelId::Folder
        ]
    );
    let before = layout.to_json();
    assert!(layout.resize(PanelId::Folder, -1.).is_err());
    assert!(
        layout
            .float(
                PanelId::Folder,
                FloatBounds {
                    width: f32::NAN,
                    ..FloatBounds::default()
                }
            )
            .is_err()
    );
    assert_eq!(layout.to_json(), before);
    let mut corrupt = before;
    corrupt["panels"]["folder"]["bounds"]["height"] = 0.into();
    assert!(PanelLayout::from_json(&corrupt).is_err());
}
