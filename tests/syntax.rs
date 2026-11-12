use notepad_matrix::syntax::{JsonStyle, json_styles};

#[test]
fn native_negative_descent_is_normalized_to_line_depth() {
    let mut metrics = notepad_matrix::syntax::StyleLineMetrics::new(20.);
    metrics.include(40., 41., -11.);
    assert_eq!(metrics.line_height(), 52.);
    assert_eq!(metrics.baseline(), 41.);
}

#[test]
fn mixed_udl_fonts_share_global_metrics_even_on_rows_without_the_largest_style() {
    use notepad_matrix::syntax::StyleLineMetrics;
    let mut metrics = StyleLineMetrics::new(20.);
    metrics.include(13., 11., 3.);
    metrics.include(30., 27., 8.);
    metrics.include(22., 18., 10.);
    assert_eq!(metrics.line_height(), 39.);
    assert_eq!(metrics.baseline(), 28.);
}

#[test]
fn profiled_udl_sizes_inherit_calibrated_base_but_explicit_sizes_only_add_zoom() {
    use notepad_matrix::syntax::{StyleAppearance, udl_appearance_with_profile};
    use notepad_matrix::udl::{Definition, UdlStyleId};
    let mut definition = Definition::default();
    let mut profile = StyleAppearance::plain(false);
    profile.font_family = "Helvetica".into();
    profile.font_size = 23;
    let inherited = udl_appearance_with_profile(&definition, UdlStyleId::Keyword1, &profile, 3);
    assert_eq!(inherited.font_family, "Helvetica");
    assert_eq!(inherited.font_size, 23);
    definition.styles[4].font_size = 0;
    assert_eq!(
        udl_appearance_with_profile(&definition, UdlStyleId::Keyword1, &profile, 3).font_size,
        23
    );
    definition.styles[4].font_size = 13;
    assert_eq!(
        udl_appearance_with_profile(&definition, UdlStyleId::Keyword1, &profile, 3).font_size,
        16
    );
    definition.styles[4].font_size = 26;
    definition.styles[4].font_name = "Explicit Family".into();
    let explicit = udl_appearance_with_profile(&definition, UdlStyleId::Keyword1, &profile, 3);
    assert_eq!(explicit.font_size, 29);
    assert_eq!(explicit.font_family, "Explicit Family");
    assert_eq!(
        udl_appearance_with_profile(&definition, UdlStyleId::Keyword1, &profile, -30).font_size,
        1
    );
}

#[test]
fn language_identity_round_trips_without_requiring_a_loaded_definition() {
    use notepad_matrix::syntax::Language;
    for (language, key) in [
        (Language::NormalText, "normal"),
        (Language::Json, "json"),
        (Language::UserDefined(0), "udl:0"),
        (Language::UserDefined(42), "udl:42"),
        (Language::UserDefined(u64::MAX), "udl:18446744073709551615"),
    ] {
        assert_eq!(language.key(), key);
        assert_eq!(Language::from_key(key), Some(language));
    }
    for key in [
        "",
        "xml",
        "udl:",
        "udl:-1",
        "udl:+1",
        "udl:01",
        "udl: 1",
        "udl:18446744073709551616",
    ] {
        assert_eq!(Language::from_key(key), None, "{key}");
    }
    assert_eq!(Language::UserDefined(42).label(), "User Defined");
}

#[test]
fn udl_appearance_resolves_color_masks_font_flags_and_zero_size_inheritance() {
    use notepad_matrix::syntax::udl_appearance;
    use notepad_matrix::udl::{Definition, UdlStyleId};
    let mut definition = Definition::default();
    definition.styles[0].foreground = Some(0x123456);
    definition.styles[0].background = Some(0x654321);
    definition.styles[0].font_name = "Source Font".into();
    definition.styles[0].font_size = 18;
    definition.styles[0].font_style = 7;
    let keyword = &mut definition.styles[4];
    keyword.color_style = 2;
    keyword.foreground = Some(0xffffff);
    keyword.background = Some(0xabcdef);
    keyword.font_style = 0;
    keyword.font_size = 0;
    let appearance = udl_appearance(&definition, UdlStyleId::Keyword1, false);
    assert_eq!(appearance.foreground, 0x202020);
    assert_eq!(appearance.background, 0xabcdef);
    assert_eq!(appearance.font_family, "Menlo");
    assert_eq!(appearance.font_size, 13);
    assert!(!appearance.bold && !appearance.italic && !appearance.underline);
    definition.styles[4].font_name = "Token Font".into();
    definition.styles[4].font_size = 24;
    let appearance = udl_appearance(&definition, UdlStyleId::Keyword1, true);
    assert_eq!(
        appearance.font_family, "Token Font",
        "fontEnabled is dialog metadata, not a font rendering gate"
    );
    assert_eq!(appearance.font_size, 24);
    definition.styles[4].font_style = -2;
    let appearance = udl_appearance(&definition, UdlStyleId::Keyword1, false);
    assert!(
        !appearance.bold && appearance.italic && appearance.underline,
        "only STYLE_NOT_USED (-1) suppresses font-style setters"
    );
}

#[test]
fn json_source_token_styles_include_properties_escapes_ld_uris_and_error_lines() {
    let text = "{\"key\":\"hi\\n\",\"n\":-12.5e+2,\"ok\":true,\"url\":\"https://x\",\"ld\":\"@id\",\"iri\":\"a:b\"}\nwrong\n\"unterminated\n[\u{b}1]\n{\"vt\"\u{b}:1}\n";
    let spans = json_styles(text);
    for (token, style) in [
        ("\"key\"", JsonStyle::Property),
        ("hi", JsonStyle::String),
        ("\\n", JsonStyle::Escape),
        ("-12.5e+2", JsonStyle::Number),
        ("true", JsonStyle::Keyword),
        ("https://x", JsonStyle::Uri),
        ("@id", JsonStyle::LdKeyword),
        ("\"a:b\"", JsonStyle::CompactIri),
        ("wrong", JsonStyle::Error),
        ("\"unterminated", JsonStyle::StringEol),
        ("\u{b}", JsonStyle::Default),
        ("\"vt\"", JsonStyle::Property),
    ] {
        let start = text.find(token).expect("literal fixture token");
        assert!(
            spans
                .iter()
                .filter(|span| span.range.start < start + token.len() && span.range.end > start)
                .all(|span| span.style == style),
            "source style for {token}"
        );
    }
    assert_eq!(spans.first().map(|span| span.range.start), Some(0));
    assert_eq!(spans.last().map(|span| span.range.end), Some(text.len()));
    assert!(
        spans
            .windows(2)
            .all(|pair| pair[0].range.end == pair[1].range.start)
    );
}
