#[path = "../src/ui/editor_surface/chrome.rs"]
mod chrome;

use chrome::ScrollAxis;
use notepad_matrix::byte_text;

#[test]
fn unicode_row_entry_never_returns_interior_boundaries_for_noncharacters() {
    let row = chrome::DisplayRow::with_controls("\u{fffe}", false, false);
    assert_eq!(row.text, "\u{fffe}");
    for display in 0..=row.text.len() {
        assert!(matches!(row.source_index(display), 0 | 3));
    }
    assert_eq!(
        chrome::DisplayRow::with_bytes(b"\xef\xbf\xbe", false, false).text,
        "xEFxBFxBE"
    );
}

#[test]
fn raw_byte_rows_show_source_hex_labels_with_explicit_raw_display_boundaries() {
    let raw = b"A\xff\xe4\xb8\xad\t\0\r\n";
    let row = chrome::DisplayRow::with_bytes(raw, true, true);
    assert_eq!(row.text, "AxFF中 NULCRLF");
    assert_eq!(row.content_len(), 7);
    for (source, display) in [(0, 0), (1, 1), (2, 4), (5, 7), (6, 8), (7, 11)] {
        assert_eq!(row.display_index(source), display);
        assert_eq!(row.source_index(display), source);
    }
    assert_eq!(row.source_index(2), 1);
    assert_eq!(row.source_index(3), 2);
    let invalid = chrome::DisplayRow::with_bytes(b"\xed\xb0\x80\xf0\x9f\x98\x80\r\n", false, false);
    assert_eq!(invalid.text, "xEDxB0x80😀");
    assert_eq!(invalid.content_len(), 7);
    assert_eq!(invalid.display_index(3), 9);
    assert_eq!(
        chrome::DisplayRow::with_bytes(b"\xef\xbf\xbe", false, false).text,
        "xEFxBFxBE"
    );
    assert_eq!(raw, b"A\xff\xe4\xb8\xad\t\0\r\n");
}

#[test]
fn control_wheel_zooms_by_direction_but_plain_and_horizontal_wheels_do_not() {
    assert_eq!(chrome::wheel_zoom_delta(true, 120.), Some(1));
    assert_eq!(chrome::wheel_zoom_delta(true, -2.), Some(-1));
    assert_eq!(chrome::wheel_zoom_delta(true, 0.), None);
    assert_eq!(chrome::wheel_zoom_delta(false, 120.), None);
    assert_eq!(chrome::wheel_zoom_delta(true, f32::NAN), None);
}

#[test]
fn zoom_matches_scintilla_point_steps_limits_and_reset() {
    let mut display = chrome::EditorDisplay::default();
    assert!(
        display.indent_guides,
        "Parameters.h ScintillaViewParams default"
    );
    assert!(!display.all_characters);
    assert_eq!(display.font_size(), 20.);
    assert_eq!(display.line_height(), 26.);
    assert_eq!(display.zoom(), 0);
    assert_eq!(display.zoom_delta(1), 1);
    assert_eq!(display.font_size(), 21.);
    assert!(display.line_height() > 26.);
    assert_eq!(display.zoom_delta(100), 60);
    assert_eq!(display.zoom_delta(-100), -10);
    assert_eq!(display.font_size(), 10.);
    display.reset_zoom();
    assert_eq!(display.font_size(), 20.);
}

#[test]
fn source_default_margins_have_one_text_origin_and_scrollbar_geometry() {
    assert_eq!(chrome::DEFAULT_EDITOR_FONT_FAMILY, "Helvetica");
    assert_eq!(chrome::GUTTER_LEFT, 0.);
    assert_eq!(chrome::GUTTER_WIDTH, 71.);
    assert_eq!(chrome::TEXT_LEFT, 72.);
    assert_eq!(chrome::TEXT_RIGHT_PADDING, 4.);
    assert_eq!(chrome::TEXT_INSET, 76.);
    assert_eq!(chrome::TEXT_TOP, 3.);
    assert_eq!(chrome::TEXT_VERTICAL_PADDING, 6.);
    assert_eq!(chrome::SCROLLBAR_SIZE, 16.);
}

#[test]
fn scrollbar_thumb_track_reserves_real_arrow_endcaps_even_in_tiny_viewports() {
    assert_eq!(chrome::scrollbar_track(100.), 16.0..84.0);
    assert_eq!(chrome::scrollbar_track(20.), 10.0..10.0);
    let axis = ScrollAxis::new(68., 100., 10., 0.);
    assert_eq!(axis.step_by(1.), 1.);
    assert_eq!(axis.step_by(-1.), 0.);
    assert_eq!(axis.step_by(200.), 90.);
}

#[test]
fn visible_char_projection_keeps_unicode_and_ime_byte_boundaries() {
    let row = chrome::DisplayRow::with_controls("A\t中\0\r\n", true, true);
    assert_eq!(row.text, "A   中NULCRLF");
    assert_eq!(row.content_len(), 6);
    assert_eq!(row.display_index(2), 4);
    assert_eq!(row.display_index(5), 7);
    assert_eq!(row.display_index(6), 10);
    assert_eq!(row.source_index(2), 1);
    assert_eq!(row.source_index(3), 2);
    assert_eq!(row.source_index(8), 5);
    assert_eq!(row.source_index(12), 6);
    assert_eq!(
        chrome::DisplayRow::with_controls("x\u{200b}\u{85}", true, true).text,
        "xZWSPNEL"
    );
    assert_eq!(
        chrome::DisplayRow::with_controls("a\t中", false, false).text,
        "a   中"
    );
}

#[test]
fn indent_guides_follow_tab_stops_not_byte_count_and_skip_column_zero() {
    assert_eq!(chrome::indent_guide_columns("\t    body"), vec![4]);
    assert_eq!(chrome::indent_guide_columns("  \t\t中"), vec![4]);
    assert_eq!(chrome::indent_guide_columns("   body"), Vec::<usize>::new());
    assert_eq!(
        chrome::indent_guide_columns("body    tail"),
        Vec::<usize>::new()
    );
}

#[test]
fn show_all_includes_source_unicode_nonprinting_representations() {
    let raw = "\u{061c}\u{2000}\u{202f}\u{2066}\u{3000}\u{feff}\u{fff9}";
    let row = chrome::DisplayRow::with_controls(raw, true, true);
    assert_eq!(row.text, "ALMNQSPNNBSPLRIIDSPZWNBSPIAA");
    assert_eq!(row.source_index(row.text.len()), raw.len());
    assert_eq!(
        chrome::DisplayRow::with_controls(raw, false, false).text,
        raw
    );
}

#[test]
fn source_default_shows_controls_without_enabling_all_whitespace() {
    let display = chrome::EditorDisplay::default();
    assert!(display.control_characters);
    let row =
        chrome::DisplayRow::with_controls("x\0\u{85}\u{200b}\n", false, display.control_characters);
    assert_eq!(row.text, "xNULNEL\u{200b}");
}

#[test]
fn indent_guides_continue_through_empty_lines_using_both_neighbors() {
    assert_eq!(
        chrome::indent_guides_with_context("\n", Some("    outer"), Some("        inner")),
        vec![4]
    );
    assert_eq!(
        chrome::indent_guides_with_context(
            "        body",
            Some("            previous"),
            Some("next")
        ),
        vec![4]
    );
    assert_eq!(
        chrome::indent_guides_with_context("\t\n", None, None),
        Vec::<usize>::new()
    );
}

#[test]
fn proportional_thumb_and_drag_reach_both_document_edges() {
    let axis = ScrollAxis::new(200., 1000., 200., 400.);
    assert_eq!(axis.thumb(), 80.0..120.0);
    assert_eq!(axis.offset_for_thumb(0.), 0.);
    assert_eq!(axis.offset_for_thumb(160.), 800.);
    assert_eq!(axis.offset_for_thumb(80.), 400.);
    assert_eq!(axis.page_toward(150.), 600.);
    assert_eq!(axis.page_toward(30.), 200.);
}

#[test]
fn short_and_tiny_viewports_have_no_phantom_scroll_range() {
    let short = ScrollAxis::new(200., 40., 200., 80.);
    assert_eq!(short.thumb(), 0.0..200.0);
    assert_eq!(short.offset_for_thumb(100.), 0.);
    let tiny = ScrollAxis::new(8., 1000., 10., 990.);
    assert_eq!(tiny.thumb(), 0.0..8.0);
    assert_eq!(tiny.offset_for_thumb(f32::NAN), 0.);
    assert_eq!(
        ScrollAxis::new(200., 1000., 200., 900.).clamped_offset(),
        800.
    );
}
