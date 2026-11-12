use notepad_matrix::{builtin_language, languages, syntax::StyleAppearance};

#[test]
fn source_style_attributes_are_applied_over_profile_with_zoom() {
    let mut definition = languages::catalog()
        .expect("catalog")
        .by_name("cpp")
        .expect("C++")
        .clone();
    let style = definition
        .styles
        .iter_mut()
        .find(|style| style.id == 5)
        .expect("keyword style");
    style
        .attributes
        .insert("fontName".into(), "Source Code Pro".into());
    style.attributes.insert("fontSize".into(), "17".into());
    style.attributes.insert("fontStyle".into(), "7".into());
    style.attributes.insert("colorStyle".into(), "1".into());
    let base = StyleAppearance::plain(false);
    let appearance = builtin_language::style_appearance(&definition, 5, false, &base, 2);
    assert_eq!(appearance.font_family, "Source Code Pro");
    assert_eq!(appearance.font_size, 19);
    assert_eq!(appearance.foreground, 0x0000ff);
    assert_eq!(appearance.background, base.background);
    assert!(appearance.bold && appearance.italic && appearance.underline);
}

#[test]
fn cached_runtime_updates_real_text_and_invalidates_profile_and_style_revision() {
    use builtin_language::{CacheUpdate, SyntaxCache};
    let catalog = languages::catalog().expect("catalog");
    let cpp = catalog.by_name("cpp").expect("C++");
    let text = "int one;\nint two;\nint three;\n";
    let mut cache = SyntaxCache::new(text, cpp, 1).expect("runtime");
    assert_eq!(
        cache.refresh(text, cpp, 1).expect("same snapshot"),
        CacheUpdate::Unchanged
    );
    assert_eq!(
        cache.refresh(text, cpp, 2).expect("style revision"),
        CacheUpdate::Restyled
    );
    let changed = "int one;\nint λtwo;\nint three;\n";
    let update = cache.refresh(changed, cpp, 2).expect("text edit");
    let CacheUpdate::Relexed(report) = update else {
        panic!("edit must visit lexer")
    };
    assert!(
        report
            .relexed_range
            .contains(&changed.find('λ').expect("unicode edit"))
    );
    assert_eq!(cache.text(), changed);
    assert_eq!(
        cache.result(),
        &notepad_matrix::lexers::lex(changed, cpp).expect("full independent result")
    );
    let python = catalog.by_name("python").expect("Python");
    assert!(matches!(
        cache.refresh(changed, python, 2).expect("profile change"),
        CacheUpdate::Relexed(_)
    ));
    assert_eq!(cache.definition().id, python.id);
}

#[test]
fn builtin_html_uses_actual_runtime_classes_and_rejects_unknown_profiles() {
    use notepad_matrix::{html_export, syntax::Language};
    let cpp = languages::catalog()
        .expect("catalog")
        .by_name("cpp")
        .expect("C++");
    let html = html_export::serialize_html(
        "int main() { return 7; }",
        Language::Builtin(cpp.id),
        false,
        4,
    )
    .expect("builtin export");
    assert!(html.contains("<span class=\"sc16\">int</span>"), "{html}");
    assert!(html.contains("<span class=\"sc5\">return</span>"), "{html}");
    assert!(
        html.contains(".sc5 {\r\n\tfont-weight: bold;\r\n\tcolor: #0000FF;"),
        "{html}"
    );
    let error = html_export::serialize_html("keep me", Language::Builtin(u16::MAX), false, 4)
        .expect_err("invalid profile must not export as normal text");
    assert!(error.to_string().contains("65535"));
}

#[test]
fn viewport_runs_clip_utf8_and_include_plain_profile_and_unused_style_metrics() {
    let mut definition = languages::catalog()
        .expect("catalog")
        .by_name("cpp")
        .expect("C++")
        .clone();
    definition
        .styles
        .iter_mut()
        .find(|style| style.id == 1)
        .expect("unused comment style")
        .attributes
        .insert("fontSize".into(), "40".into());
    let cache = builtin_language::SyntaxCache::new("int λ;\n", &definition, 0).expect("runtime");
    let profile = StyleAppearance::plain(false);
    let runs = cache
        .styled_runs(1..7, false, &profile, 2)
        .expect("visible runs");
    assert_eq!(runs.first().expect("first").range.start, 1);
    assert_eq!(runs.last().expect("last").range.end, 7);
    assert_eq!(runs.iter().map(|run| run.range.len()).sum::<usize>(), 6);
    assert!(cache.styled_runs(5..7, false, &profile, 2).is_err());
    let metrics = builtin_language::metric_appearances(&definition, false, &profile, 2);
    assert!(metrics.contains(&profile));
    assert!(metrics.iter().any(|appearance| appearance.font_size == 42));
}

#[test]
fn source_tag_matching_returns_name_tail_close_and_whole_attribute_ranges() {
    use builtin_language::TagMatchOptions;
    let html = languages::catalog()
        .expect("catalog")
        .by_name("html")
        .expect("HTML");
    let text = "<DIV key=\"a>b\"><div/><!-- </DIV> --><span></span></div>";
    let matched = builtin_language::matching_tags(text, html, 3, TagMatchOptions::default())
        .expect("matched outer tags");
    assert_eq!(matched.tags, vec![0..4, 14..15, 49..55]);
    assert_eq!(matched.attributes, vec![5..14]);
    assert_eq!(matched.content, Some(15..49));
    let self_closed = builtin_language::matching_tags(text, html, 18, TagMatchOptions::default())
        .expect("self closing");
    assert_eq!(self_closed.tags, vec![15..19, 19..21]);
    assert_eq!(self_closed.content, None);
    assert!(builtin_language::matching_tags(text, html, 24, TagMatchOptions::default()).is_none());
    assert!(
        builtin_language::matching_tags(
            text,
            html,
            3,
            TagMatchOptions {
                enabled: false,
                ..TagMatchOptions::default()
            }
        )
        .is_none()
    );
    let without_attributes = builtin_language::matching_tags(
        text,
        html,
        3,
        TagMatchOptions {
            attributes: false,
            ..TagMatchOptions::default()
        },
    )
    .expect("tags only");
    assert!(without_attributes.attributes.is_empty());
}

#[test]
fn active_dark_source_palette_is_not_a_light_palette_or_generic_dark_fallback() {
    let catalog = languages::catalog().expect("catalog");
    let cpp = catalog.by_name("cpp").expect("C++");
    let profile = StyleAppearance::plain(true);
    let keyword = builtin_language::style_appearance(cpp, 5, true, &profile, 0);
    assert_eq!(keyword.foreground, 0xdfc47d);
    assert_eq!(keyword.background, 0x3f3f3f);
    assert_eq!(keyword.font_family, profile.font_family);
    assert_eq!(keyword.font_size, profile.font_size);
    assert!(keyword.bold);
    let default = builtin_language::style_appearance(cpp, 0, true, &profile, 0);
    assert_eq!(
        (default.foreground, default.background),
        (0xdcdccc, 0x3f3f3f)
    );
    let html = notepad_matrix::html_export::serialize_html(
        "return 7;",
        notepad_matrix::syntax::Language::Builtin(cpp.id),
        true,
        4,
    )
    .expect("dark export");
    assert!(html.contains("color: #DFC47D;"));
    assert!(html.contains("background: #3F3F3F;"));
    assert!(!html.contains("#FFFFFF"));
}

#[test]
fn cached_tag_ranges_respect_xml_case_server_regions_and_source_indicator_colors() {
    let catalog = languages::catalog().expect("catalog");
    let xml = catalog.by_name("xml").expect("XML");
    let xml_cache = builtin_language::SyntaxCache::new("<A></a>", xml, 0).expect("XML runtime");
    assert!(
        xml_cache
            .matching_tags(1, builtin_language::TagMatchOptions::default())
            .is_none()
    );
    let php = catalog.by_name("php").expect("PHP");
    let text = "<div key='<?php echo 1; ?>'>body</div>";
    let mut cache = builtin_language::SyntaxCache::new(text, php, 0).expect("PHP runtime");
    assert!(
        cache
            .matching_tags(17, builtin_language::TagMatchOptions::default())
            .is_none()
    );
    let ranges = cache
        .matching_tags(2, builtin_language::TagMatchOptions::default())
        .expect("markup caret");
    let indicators = builtin_language::tag_indicators(&ranges, true);
    assert_eq!(indicators.len(), 4);
    assert_eq!(indicators[0].color, 0xc6c600);
    assert_eq!(indicators[0].alpha, 100);
    assert_eq!(indicators[3].color, 0x78926f);
    cache.refresh("<div></div>", php, 0).expect("edit");
    let ranges = cache
        .matching_tags(2, builtin_language::TagMatchOptions::default())
        .expect("updated cached tokens");
    assert_eq!(ranges.tags, vec![0..4, 4..5, 5..11]);
    assert!(ranges.attributes.is_empty());
}

#[test]
fn html_exports_full_utf8_documents_for_every_catalog_profile_and_rejects_bad_lexer() {
    let catalog = languages::catalog().expect("catalog");
    let text = "value = 7;\n\tλ <&\"'\nlast line\n";
    for definition in catalog.profiles() {
        for dark in [false, true] {
            let html = notepad_matrix::html_export::serialize_builtin_html_with_profile(
                text,
                definition.id,
                dark,
                4,
                &StyleAppearance::plain(dark),
                0,
            )
            .unwrap_or_else(|error| panic!("{} dark={dark}: {error}", definition.name));
            let body = html
                .split_once("<div ")
                .expect("document body")
                .1
                .split_once('>')
                .expect("body start")
                .1
                .split_once("</div>")
                .expect("body end")
                .0;
            let mut plain = String::new();
            let mut inside_span_tag = false;
            for ch in body.chars() {
                if ch == '<' {
                    inside_span_tag = true;
                } else if ch == '>' && inside_span_tag {
                    inside_span_tag = false;
                } else if !inside_span_tag {
                    plain.push(ch);
                }
            }
            assert_eq!(
                plain, "value = 7;\r\n    λ &lt;&amp;\"'\r\nlast line\r\n",
                "{} dark={dark}",
                definition.name
            );
            assert!(html.ends_with("</html>\r\n"), "{}", definition.name);
        }
    }
    let mut invalid = catalog.by_name("cpp").expect("C++").clone();
    invalid.name = "not-a-real-lexer".into();
    let error = notepad_matrix::html_export::serialize_builtin_definition_html(
        text,
        &invalid,
        false,
        4,
        &StyleAppearance::plain(false),
        0,
    )
    .expect_err("unsupported lexer cannot silently export Normal Text");
    assert!(error.to_string().contains("unsupported runtime lexer"));
}

#[test]
fn corrupted_effective_lexer_is_an_explicit_runtime_and_export_error() {
    let mut definition = languages::catalog()
        .expect("catalog")
        .by_name("cpp")
        .expect("C++")
        .clone();
    definition.effective_lexer = "not-a-real-lexer".into();
    assert!(builtin_language::SyntaxCache::new("int x;", &definition, 0).is_err());
    let error = notepad_matrix::html_export::serialize_builtin_definition_html(
        "int x;",
        &definition,
        false,
        4,
        &StyleAppearance::plain(false),
        0,
    )
    .expect_err("invalid lexer is not a plain-text fallback");
    assert!(error.to_string().contains("not-a-real-lexer"));
}
