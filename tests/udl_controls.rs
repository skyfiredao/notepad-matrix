#![allow(dead_code, unused_imports)]
#[path = "../src/udl.rs"]
mod udl;
#[path = "../src/udl_controls.rs"]
mod udl_controls;
use udl_controls::{Page, controls, style_controls};

#[test]
fn four_pages_cover_all_source_property_groups_without_editable_internal_ids() {
    let fields = Page::ALL.into_iter().flat_map(controls).collect::<Vec<_>>();
    for i in 0..8 {
        assert!(fields.iter().any(|f| f.path == format!("/keywords/{i}")));
        assert!(
            fields
                .iter()
                .any(|f| f.path == format!("/prefix_keywords/{i}"))
        );
        for key in ["open", "escape", "close"] {
            assert!(
                fields
                    .iter()
                    .any(|f| f.path == format!("/delimiters/{i}/{key}"))
            );
        }
    }
    for path in [
        "/numbers/range",
        "/numbers/extras2",
        "/comments/line_continue",
        "/comments/line_close",
        "/folding/2/middle",
        "/operators/1",
        "/decimal_separator",
    ] {
        assert!(fields.iter().any(|f| f.path == path), "missing {path}");
    }
    for style in 0..24 {
        let fields = style_controls(style);
        for key in [
            "foreground",
            "background",
            "font_name",
            "font_size",
            "nesting",
            "font_style",
            "color_style",
        ] {
            assert!(
                fields
                    .iter()
                    .any(|f| f.path == format!("/styles/{style}/{key}")),
                "missing style {key}"
            );
        }
        assert!(!fields.iter().any(|f| f.path.ends_with("/id")));
    }
}

#[test]
fn controls_apply_validated_colors_fonts_and_independent_nesting_bits() {
    let mut definition = udl::Definition::default();
    let style = style_controls(16);
    let foreground = style
        .iter()
        .find(|f| f.label == "Foreground (#RRGGBB; blank inherits)")
        .expect("color control");
    foreground.apply(&mut definition, "#12ab34").expect("color");
    assert_eq!(definition.styles[16].foreground, Some(0x12ab34));
    let bold = style.iter().find(|f| f.label == "Bold").expect("bold");
    bold.apply(&mut definition, "true").expect("enable bold");
    assert_eq!(definition.styles[16].font_style, 1);
    let nested = style
        .iter()
        .find(|f| f.label == "Allow keyword group 8")
        .expect("nesting control");
    nested.apply(&mut definition, "true").expect("nest");
    assert_eq!(definition.styles[16].nesting, udl::nesting::keyword(7));
    let before = definition.clone();
    assert!(foreground.apply(&mut definition, "not a color").is_err());
    assert_eq!(definition, before);
}

#[test]
fn every_visible_property_control_mutates_a_valid_definition() {
    use udl_controls::Kind;
    let mut definition = udl::Definition::default();
    let fields = Page::ALL
        .into_iter()
        .flat_map(controls)
        .chain((0..24).flat_map(style_controls))
        .chain(std::iter::once(udl_controls::extensions_control()));
    for field in fields {
        let value = match field.kind {
            Kind::Text => "source-value",
            Kind::Color => "#abcdef",
            Kind::Boolean | Kind::Bit(_) => "true",
            Kind::Choice(_) => "1",
            Kind::Integer { .. } => "14",
        };
        field
            .apply(&mut definition, value)
            .unwrap_or_else(|error| panic!("{} {}: {error}", field.group, field.label));
    }
    assert_eq!(definition.keywords[7], "source-value");
    assert_eq!(definition.delimiters[7].escape, "source-value");
    assert_eq!(definition.comments.line_continue, "source-value");
    assert_eq!(definition.numbers.suffix2, "source-value");
    assert_eq!(definition.operators[1], "source-value");
    assert_eq!(definition.folding[2].middle, "source-value");
    assert_eq!(definition.styles[23].font_style, 7);
    assert_eq!(definition.styles[23].font_size, 14);
    assert_eq!(definition.styles[23].foreground, Some(0xabcdef));
    let restored =
        udl::Definition::from_json(&definition.to_json()).expect("all UI controls roundtrip");
    assert_eq!(definition, restored);
}
