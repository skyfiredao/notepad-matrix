use notepad_matrix::languages::{EMBEDDED_CATALOG, LanguageCatalog, catalog};
use notepad_matrix::syntax::Language;
use std::path::Path;

#[test]
fn frozen_dark_and_light_defaults_and_effective_alias_styles_are_distinct() {
    let catalog = catalog().expect("catalog");
    let dark = catalog.default_style(true).expect("dark default 32");
    assert_eq!(dark.id, 32);
    assert_eq!(dark.attributes["fgColor"], "DCDCCC");
    assert_eq!(dark.attributes["bgColor"], "3F3F3F");
    assert_eq!(dark.attributes["fontName"], "Consolas");
    assert_eq!(dark.attributes["fontSize"], "10");
    let light = catalog.default_style(false).expect("light default 32");
    assert_eq!(light.attributes["fgColor"], "000000");
    assert_eq!(light.attributes["bgColor"], "FFFFFF");
    assert_eq!(light.attributes["fontName"], "Courier New");
    assert_eq!(
        catalog
            .style_for_theme(3, 5, true)
            .expect("dark C++ keyword")
            .attributes["fgColor"],
        "DFC47D"
    );
    assert_eq!(
        catalog
            .style_for_theme(3, 5, false)
            .expect("light C++ keyword")
            .attributes["fgColor"],
        "0000FF"
    );
    for dark in [false, true] {
        assert_eq!(
            catalog.style_for_theme(86, 11, dark),
            catalog.style_for_theme(57, 11, dark)
        );
        assert!(catalog.style_for_theme(57, 11, dark).is_some());
        for id in [16, 55, 1] {
            for style in [1, 41, 118, 81] {
                assert!(catalog.style_for_theme(8, style, dark).is_some());
                assert_eq!(
                    catalog.style_for_theme(id, style, dark),
                    catalog.style_for_theme(8, style, dark)
                );
            }
        }
        assert_eq!(
            catalog.style_for_theme(19, 5, dark),
            catalog.style_for_theme(58, 5, dark)
        );
        assert_eq!(
            catalog.style_for_theme(0, 32, dark),
            catalog.default_style(dark)
        );
        assert!(catalog.style_for_theme(97, 5, dark).is_none());
        assert!(catalog.style_for_theme(3, u16::MAX, dark).is_none());
    }
}

#[test]
fn theme_runtime_retains_every_frozen_attribute_and_resolves_all_alias_records() {
    let catalog = catalog().expect("catalog");
    let source: serde_json::Value =
        serde_json::from_str(notepad_matrix::languages::EMBEDDED_THEMES)
            .expect("theme source JSON");
    for (name, dark) in [("light", false), ("dark", true)] {
        let theme = catalog.themes().theme(dark);
        assert_eq!(theme.lexers.len(), 93);
        assert_eq!(
            theme
                .lexers
                .values()
                .map(|lexer| lexer.styles.len())
                .sum::<usize>(),
            1774
        );
        assert_eq!(theme.global_styles.len(), 53);
        for expected in source["themes"][name]["lexers"]
            .as_array()
            .expect("source lexers")
        {
            let key = expected["attributes"]["name"]
                .as_str()
                .expect("source lexer name");
            let actual = &theme.lexers[key];
            assert_eq!(
                serde_json::to_value(&actual.attributes).expect("attributes"),
                expected["attributes"]
            );
            assert_eq!(
                actual.styles.len(),
                expected["styles"].as_array().expect("styles").len()
            );
            for (actual, expected) in actual
                .styles
                .iter()
                .zip(expected["styles"].as_array().expect("styles"))
            {
                assert_eq!(
                    u64::from(actual.id),
                    expected["id"].as_u64().expect("style id")
                );
                assert_eq!(
                    serde_json::to_value(&actual.attributes).expect("style attributes"),
                    expected["attributes"]
                );
                assert_eq!(
                    actual.keywords,
                    expected["keywords"].as_str().expect("style keyword text")
                );
            }
        }
        for (actual, expected) in theme.global_styles.iter().zip(
            source["themes"][name]["global_styles"]
                .as_array()
                .expect("global styles"),
        ) {
            assert_eq!(
                u64::from(actual.id),
                expected["id"].as_u64().expect("global id")
            );
            assert_eq!(
                serde_json::to_value(&actual.attributes).expect("global attributes"),
                expected["attributes"]
            );
            assert_eq!(
                actual.keywords,
                expected["keywords"].as_str().expect("global keyword text")
            );
        }
        for expected in &theme.lexers["json"].styles {
            assert_eq!(
                catalog.style_for_theme(86, expected.id, dark),
                Some(expected)
            );
        }
        for expected in &theme.lexers["javascript.js"].styles {
            assert_eq!(
                catalog.style_for_theme(19, expected.id, dark),
                Some(expected)
            );
        }
        let mut markup = std::collections::BTreeMap::new();
        for key in ["html", "javascript", "php", "asp"] {
            for style in &theme.lexers[key].styles {
                markup.insert(style.id, style);
            }
        }
        for id in [8, 1, 16, 55] {
            for (style_id, expected) in &markup {
                assert_eq!(
                    catalog.style_for_theme(id, *style_id, dark),
                    Some(*expected)
                );
            }
        }
    }
}

#[test]
fn theme_import_rejects_modified_colors_fonts_versions_and_missing_dark_data() {
    use notepad_matrix::languages::{EMBEDDED_THEMES, ThemeCatalog};
    let original: serde_json::Value = serde_json::from_str(EMBEDDED_THEMES).expect("theme JSON");
    assert!(ThemeCatalog::from_json(&original.to_string()).is_ok());
    for field in ["fgColor", "bgColor", "fontName", "fontSize", "fontStyle"] {
        let mut altered = original.clone();
        altered["themes"]["dark"]["lexers"][0]["styles"][0]["attributes"][field] =
            "invented".into();
        assert!(
            ThemeCatalog::from_json(&altered.to_string()).is_err(),
            "{field}"
        );
    }
    for path in ["schema_version", "source_revision", "themes"] {
        let mut altered = original.clone();
        altered[path] = serde_json::Value::Null;
        assert!(
            ThemeCatalog::from_json(&altered.to_string()).is_err(),
            "{path}"
        );
    }
    let mut altered = original;
    altered["themes"]["dark"] = serde_json::Value::Null;
    assert!(ThemeCatalog::from_json(&altered.to_string()).is_err());
    assert!(ThemeCatalog::from_json("{broken").is_err());
}

#[test]
fn frozen_catalog_contains_every_source_model() {
    let catalog = catalog().expect("valid embedded source catalog");
    assert_eq!(catalog.profiles().len(), 96);
    assert_eq!(catalog.by_name("cpp").expect("C++").id, 3);
    assert_eq!(catalog.by_name("json").expect("JSON").id, 57);
}

#[test]
fn builtin_identity_round_trips_source_ids_without_changing_existing_keys() {
    assert_eq!(Language::NormalText.key(), "normal");
    assert_eq!(Language::Json.key(), "json");
    for profile in catalog().expect("catalog").profiles() {
        let expected = match profile.id {
            0 => Language::NormalText,
            57 => Language::Json,
            id => Language::Builtin(id),
        };
        assert_eq!(Language::from_key(&expected.key()), Some(expected));
        assert_eq!(expected.description(), profile.description);
        if profile.id != 0 && profile.id != 57 {
            assert_eq!(expected.label(), profile.label);
        }
    }
    for key in [
        "builtin:15",
        "builtin:97",
        "builtin:65536",
        "builtin:03",
        "builtin:-1",
        "builtin:",
    ] {
        assert_eq!(Language::from_key(key), None, "{key}");
    }
    assert_eq!(
        Language::from_path(Path::new("example.CPP")),
        Language::Builtin(3)
    );
    assert_eq!(
        Language::from_path(Path::new("example.json")),
        Language::Json
    );
    assert_eq!(
        Language::from_path(Path::new("unknown.unknown")),
        Language::NormalText
    );
}

#[test]
fn fixed_catalog_rejects_corrupted_or_invented_source_data() {
    let original: serde_json::Value = serde_json::from_str(EMBEDDED_CATALOG).expect("JSON");
    assert!(LanguageCatalog::from_json(&original.to_string()).is_ok());
    let mut altered = original.clone();
    altered["profiles"][18]["keywords"]["instre1"] = "invented-keyword".into();
    assert!(LanguageCatalog::from_json(&altered.to_string()).is_err());
    for (field, replacement) in [
        ("schema_version", serde_json::json!(2)),
        ("source_revision", serde_json::json!("other")),
        ("profiles", serde_json::json!([])),
    ] {
        let mut altered = original.clone();
        altered[field] = replacement;
        assert!(
            LanguageCatalog::from_json(&altered.to_string()).is_err(),
            "{field}"
        );
    }
    for field in [
        "id",
        "name",
        "order",
        "extensions",
        "properties",
        "styles",
        "comment_line",
        "style_source",
    ] {
        let mut altered = original.clone();
        altered["profiles"][18][field] = serde_json::Value::Null;
        assert!(
            LanguageCatalog::from_json(&altered.to_string()).is_err(),
            "{field}"
        );
    }
    assert!(LanguageCatalog::from_json("{broken").is_err());
    assert!(LanguageCatalog::from_json(&" ".repeat(8 * 1024 * 1024 + 1)).is_err());
}

#[test]
fn source_aliases_and_embedded_properties_are_explicit() {
    let catalog = catalog().expect("catalog");
    let json5 = catalog.by_name("json5").expect("JSON5");
    assert_eq!(json5.style_source.as_deref(), Some("json"));
    assert_eq!(json5.keyword_sources, ["json"]);
    assert_eq!(json5.styles, catalog.by_name("json").expect("JSON").styles);
    assert_eq!(json5.properties["lexer.json.allow.comments"], "1");
    for name in ["html", "php", "asp", "jsp"] {
        let profile = catalog.by_name(name).expect("markup model");
        assert_eq!(profile.effective_lexer, "hypertext");
        assert_eq!(profile.properties["asp.default.language"], "2");
        assert_eq!(profile.components, ["html", "javascript", "php", "asp"]);
    }
    let javascript = catalog.by_name("javascript").expect("embedded JavaScript");
    assert_eq!(javascript.effective_style_sources, ["javascript.js"]);
    assert_eq!(javascript.keyword_sources, ["javascript.js", "cpp"]);
    assert!(catalog.by_name("normal").expect("normal").styles.is_empty());
    assert_eq!(
        catalog
            .profiles()
            .iter()
            .filter(|profile| profile.internal)
            .count(),
        1
    );
}

#[test]
fn source_extension_precedence_filenames_dotfiles_and_explicit_choice_are_preserved() {
    let catalog = catalog().expect("catalog");
    for (path, name) in [
        ("/work/main.CPP", "cpp"),
        ("/work/header.h", "cpp"),
        ("/work/main.mm", "objc"),
        ("/work/main.m", "matlab"),
        ("/work/template.vue", "javascript.js"),
        ("/work/file.tex", "tex"),
        ("/work/file.sty", "latex"),
        ("/work/config.jsonc", "json5"),
        ("/work/.bashrc", "bash"),
        ("/work/.profile", "bash"),
        ("/work/.editorconfig", "props"),
        ("/work/.gitconfig", "props"),
        ("/work/MAKEFILE", "makefile"),
        ("/work/GNUmakefile", "makefile"),
        ("/work/CMakeLists.txt", "cmake"),
        ("/work/SConstruct", "python"),
        ("/work/SConscript", "python"),
        ("/work/wscript", "python"),
        ("/work/Rakefile", "ruby"),
        ("/work/Vagrantfile", "ruby"),
        ("/work/crontab", "bash"),
        ("/work/PKGBUILD", "bash"),
        ("/work/APKBUILD", "bash"),
        ("/work/file.stx", "fcST"),
    ] {
        assert_eq!(
            catalog.by_path(Path::new(path)).expect(path).name,
            name,
            "{path}"
        );
    }
    assert_eq!(
        catalog
            .resolve(Path::new("main.rs"), Some(3))
            .expect("explicit C++")
            .name,
        "cpp"
    );
    assert_eq!(
        catalog
            .resolve(Path::new("main.rs"), Some(0))
            .expect("explicit normal")
            .name,
        "normal"
    );
    assert!(catalog.resolve(Path::new("main.rs"), Some(15)).is_err());
    assert_eq!(
        catalog
            .resolve(Path::new("no-extension"), None)
            .expect("fallback normal")
            .name,
        "normal"
    );
}

#[test]
fn lexer_adapter_uses_source_setkeyword_slots_and_style_additions() {
    let catalog = catalog().expect("catalog");
    let cpp: &notepad_matrix::languages::LanguageDefinition = catalog.by_name("cpp").expect("C++");
    assert_eq!(cpp.key(), "cpp");
    assert_eq!(cpp.lexer(), "cpp");
    assert_eq!(cpp.property("fold.cpp.comment.explicit"), Some("0"));
    assert_eq!(cpp.property("invented"), None);
    assert!(
        cpp.keywords(0)
            .split_whitespace()
            .any(|word| word == "return")
    );
    assert!(
        cpp.keywords(1)
            .split_whitespace()
            .any(|word| word == "char32_t")
    );
    assert!(
        cpp.keywords(2)
            .split_whitespace()
            .any(|word| word == "param")
    );
    assert_eq!(cpp.keywords(usize::MAX), "");
    let css = catalog.by_name("css").expect("CSS");
    assert!(
        css.keywords(4)
            .split_whitespace()
            .any(|word| word == "file-selector-button")
    );
    assert!(
        css.keywords(6)
            .split_whitespace()
            .any(|word| word == "first-letter")
    );
    assert_eq!(css.keywords(2), "");
    assert_eq!(css.keywords(3), "");
    let perl = catalog.by_name("perl").expect("Perl");
    assert!(perl.keywords(0).starts_with("carp croak "));
    let html = catalog.by_name("html").expect("HTML");
    assert!(
        html.keywords(2)
            .split_whitespace()
            .any(|word| word == "addhandler")
    );
    assert!(
        html.keywords(4)
            .split_whitespace()
            .any(|word| word == "__halt_compiler")
    );
    assert_eq!(html.keywords(5), " ATTLIST DOCTYPE ELEMENT ENTITY NOTATION");
    let json5 = catalog.by_name("json5").expect("JSON5");
    assert_eq!(json5.keywords(0), " false null true");
    assert_eq!(
        json5.keywords(1),
        " @id @context @type @value @language @container @list @set @reverse @index @base @vocab @graph"
    );
    assert_eq!(catalog.by_name("rc").expect("RC").keywords(2), "");
    assert_eq!(
        catalog
            .by_name("typescript")
            .expect("TypeScript")
            .keywords(3),
        ""
    );
}
