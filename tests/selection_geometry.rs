#[path = "../src/ui/editor_surface/selection_geometry.rs"]
mod selection_geometry;

use selection_geometry::{RowGeometry, SelectionGeometry};

#[test]
fn rectangle_virtual_space_extends_from_shaped_eol() {
    let row = RowGeometry {
        start: 5,
        end: 9,
        last: true,
        space_width: 5.,
    };
    let selection = SelectionGeometry {
        anchor: 9,
        head: 9,
        anchor_virtual: 2,
        head_virtual: 5,
    };
    assert_eq!(
        row.highlight(selection, |byte| if byte == 9 { 37. } else { 0. }),
        Some(47.0..62.0)
    );
    assert_eq!(
        row.caret(selection, |byte| if byte == 9 { 37. } else { 0. }),
        Some(62.)
    );
}

#[test]
fn wrapped_unicode_selection_uses_shaped_boundaries_once() {
    let first = RowGeometry {
        start: 0,
        end: 5,
        last: false,
        space_width: 5.,
    };
    let second = RowGeometry {
        start: 5,
        end: 12,
        last: true,
        space_width: 5.,
    };
    let selection = SelectionGeometry {
        anchor: 12,
        head: 1,
        anchor_virtual: 0,
        head_virtual: 0,
    };
    let x = |byte| match byte {
        0 => 0.,
        1 => 8.,
        5 => 32.,
        12 => 65.,
        _ => unreachable!(),
    };
    assert_eq!(first.highlight(selection, x), Some(8.0..32.0));
    assert_eq!(second.highlight(selection, x), Some(32.0..65.0));
    assert_eq!(first.caret(selection, x), Some(8.));
    assert_eq!(second.caret(selection, x), None);
    let boundary = SelectionGeometry {
        anchor: 5,
        head: 5,
        anchor_virtual: 0,
        head_virtual: 0,
    };
    assert_eq!(first.caret(boundary, x), None);
    assert_eq!(second.caret(boundary, x), Some(32.));
}

#[test]
fn crlf_selection_paints_line_end_without_leaking_into_next_line() {
    let row = RowGeometry {
        start: 0,
        end: 3,
        last: true,
        space_width: 5.,
    };
    let selection = SelectionGeometry {
        anchor: 3,
        head: 5,
        anchor_virtual: 0,
        head_virtual: 0,
    };
    assert_eq!(row.highlight(selection, |_| 21.), Some(21.0..26.0));
    assert_eq!(
        row.highlight(selection, |byte| if byte > 3 { 40. } else { 21. }),
        Some(21.0..40.0)
    );
    let next = RowGeometry {
        start: 5,
        end: 8,
        last: true,
        space_width: 5.,
    };
    assert_eq!(next.highlight(selection, |_| 0.), None);
}

#[test]
fn rectangle_hit_uses_each_rows_shaped_width_and_virtual_space() {
    let wide = RowGeometry {
        start: 0,
        end: 4,
        last: true,
        space_width: 5.,
    };
    let short = RowGeometry {
        start: 6,
        end: 7,
        last: true,
        space_width: 5.,
    };
    assert_eq!(
        wide.hit(30., |_| 3, |byte| if byte == 4 { 40. } else { 30. }),
        (3, 0)
    );
    assert_eq!(short.hit(30., |_| 7, |_| 10.), (7, 4));
}

#[test]
fn control_click_hit_prioritizes_selected_characters_then_exact_carets() {
    let ranges = [
        SelectionGeometry {
            anchor: 2,
            head: 2,
            anchor_virtual: 0,
            head_virtual: 0,
        },
        SelectionGeometry {
            anchor: 5,
            head: 2,
            anchor_virtual: 0,
            head_virtual: 0,
        },
        SelectionGeometry {
            anchor: 8,
            head: 8,
            anchor_virtual: 0,
            head_virtual: 0,
        },
    ];
    assert_eq!(
        selection_geometry::selection_at_point(&ranges, 2, 0),
        Some(1)
    );
    assert_eq!(
        selection_geometry::selection_at_point(&ranges, 4, 0),
        Some(1)
    );
    assert_eq!(selection_geometry::selection_at_point(&ranges, 5, 0), None);
    assert_eq!(
        selection_geometry::selection_at_point(&ranges, 8, 0),
        Some(2)
    );
    assert_eq!(selection_geometry::selection_at_point(&ranges, 8, 1), None);
    let virtual_only = [SelectionGeometry {
        anchor: 8,
        head: 8,
        anchor_virtual: 0,
        head_virtual: 4,
    }];
    assert_eq!(
        selection_geometry::selection_at_point(&virtual_only, 8, 0),
        None
    );
}

#[test]
fn page_movement_uses_source_one_row_overlap_and_clamps_viewport() {
    assert_eq!(
        selection_geometry::page_rows(26, 20, 100, 10, true),
        (35, 29)
    );
    assert_eq!(
        selection_geometry::page_rows(26, 20, 100, 10, false),
        (17, 11)
    );
    assert_eq!(
        selection_geometry::page_rows(96, 90, 100, 10, true),
        (99, 90)
    );
    assert_eq!(selection_geometry::page_rows(3, 0, 100, 10, false), (0, 0));
    assert_eq!(selection_geometry::page_rows(2, 2, 5, 1, true), (3, 3));
}
