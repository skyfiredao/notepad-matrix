#[path = "../src/ui/editor_surface/layout.rs"]
mod layout;

#[test]
fn collapsed_fold_removes_wrapped_fragments_but_keeps_header_and_following_line() {
    let mut rows = layout::VisualRows::default();
    rows.push_line(0, 0, std::slice::from_ref(&(0..4)), 0.);
    let wrapped = layout::wrap_ranges("abcdef", 3.1, 0., |byte| byte as f32);
    rows.push_line(1, 5, &wrapped, 0.);
    rows.push_line(2, 12, std::slice::from_ref(&(0..4)), 0.);
    rows.push_line(3, 17, std::slice::from_ref(&(0..4)), 0.);
    rows.hide_folded(&[(0, 2)]);
    assert_eq!(
        rows.rows()
            .iter()
            .map(|row| row.logical_line)
            .collect::<Vec<_>>(),
        vec![0, 3]
    );
    assert_eq!(rows.row_for_byte(9), 0);
    assert_eq!(rows.row_for_byte(17), 1);
}

#[test]
fn mixed_size_runs_share_the_global_style_baseline() {
    assert_eq!(layout::baseline_shift(30., 22., 10., 3.), 3.5);
    assert_eq!(layout::baseline_shift(30., 22., 20., 6.), 0.);
}

#[test]
fn proportional_tabs_advance_to_pixel_stops() {
    assert_eq!(layout::next_tab_stop(13., 5., 4), 20.);
    assert_eq!(layout::next_tab_stop(21., 5., 4), 40.);
    assert_eq!(layout::next_tab_stop(40., 5., 4), 60.);
}

#[test]
fn word_wrap_prefers_space_boundaries_and_never_splits_utf8_scalars() {
    let text = "one two three";
    let rows = layout::wrap_ranges(text, 9., 0., |byte| byte as f32);
    assert_eq!(rows, vec![0..8, 8..13]);
    let unicode = "中😀ab";
    let rows = layout::wrap_ranges(unicode, 2.5, 0., |byte| {
        unicode[..byte].chars().count() as f32
    });
    assert_eq!(rows, vec![0..7, 7..9]);
    assert_eq!(layout::wrap_ranges("", 0., 0., |_| 0.), vec![0..0]);
}

#[test]
fn visual_map_owns_wrap_boundaries_downstream_and_preserves_eol_gaps() {
    let mut map = layout::VisualRows::default();
    map.push_line(0, 0, &[0..4, 4..8], 2.);
    map.push_line(1, 10, std::slice::from_ref(&(0..0)), 0.);
    map.push_line(2, 11, std::slice::from_ref(&(0..3)), 0.);
    assert_eq!(map.len(), 4);
    assert_eq!(map.row_for_byte(4), 1);
    assert_eq!(map.row_for_byte(8), 1);
    assert_eq!(map.row_for_byte(10), 2);
    assert_eq!(map.rows()[1].indent, 2.);
    assert_eq!(map.rows()[1].range, 4..8);
    assert!(!map.rows()[0].last);
    assert!(map.rows()[1].last);
    assert_eq!(map.restored_top(0, 1, 2, true), 1);
    assert_eq!(map.restored_top(0, 1, 3, true), 0);
    map.hide_folded(&[(0, 2)]);
    assert_eq!(map.restored_top(1, 1, 2, true), 0);
    assert_eq!(map.restored_top(99, 1, 2, true), 0);
}

#[test]
fn continuation_indent_and_combining_sequences_do_not_stall_narrow_wrap() {
    let rows = layout::wrap_ranges("abcdefgh", 4.5, 2., |byte| byte as f32);
    assert_eq!(rows, vec![0..4, 4..6, 6..8]);
    let combined = "ae\u{301}z";
    let rows = layout::wrap_ranges(combined, 2., 0., |byte| {
        combined[..byte].chars().count() as f32
    });
    assert_eq!(rows, vec![0..1, 1..4, 4..5]);
    assert_eq!(
        layout::wrap_ranges("😀中", 0., 0., |byte| byte as f32),
        vec![0..4, 4..7]
    );
}
