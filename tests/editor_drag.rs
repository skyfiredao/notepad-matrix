#[path = "../src/ui/editor_surface/drag.rs"]
mod drag;

#[test]
fn selection_drag_waits_for_threshold_and_tracks_drop_target() {
    let mut gesture = drag::SelectionDrag::new(10., 20., 4);
    gesture.update(11., 21., 5);
    assert_eq!(gesture.drop_target(), None);
    gesture.update(20., 20., 9);
    assert_eq!(gesture.drop_target(), Some(9));
    gesture.update(10., 20., 4);
    assert_eq!(gesture.drop_target(), Some(4));
}
