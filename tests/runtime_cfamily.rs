use notepad_matrix::{languages, lexers, syntax};

fn profile(key: &str) -> languages::LanguageDefinition {
    languages::catalog()
        .expect("catalog")
        .by_name(key)
        .expect(key)
        .clone()
}

fn style(key: &str, text: &str, needle: &str) -> u16 {
    let result = lexers::lex(text, &profile(key)).expect("lex");
    let position = text.find(needle).expect(needle);
    result
        .spans
        .iter()
        .find(|span| span.range.contains(&position))
        .expect("span")
        .style
}

#[test]
fn normal_and_unsupported_are_distinct() {
    let result = lexers::lex("hello λ\r\nworld", &profile("normal")).expect("normal");
    assert_eq!(result.spans[0].range, 0..15);
    assert_eq!(result.spans[0].style, 0);
    assert_eq!(result.line_states.len(), 2);
    let mut unknown = profile("normal");
    unknown.name = "not-a-lexer".into();
    assert!(matches!(
        lexers::lex("x", &unknown),
        Err(lexers::LexError::UnsupportedLanguage(_))
    ));
}

#[test]
fn cpp_raw_delimiter_and_documentation_are_source_states() {
    let text = "auto s = R\"tag(a\n\"b)tag\";\n/** @param value */\n";
    assert_eq!(style("cpp", text, "a\n"), 20);
    assert_eq!(style("cpp", text, "@param"), 17);
    assert_eq!(style("cpp", text, "auto"), 16);
}

#[test]
fn objc_directives_qualifiers_and_boxed_strings() {
    let text = "@interface Thing\n- (oneway void)send;\nNSString *s = @\"a\"\"b\ntext\";\n@end";
    assert_eq!(style("objc", text, "@interface"), 19);
    assert_eq!(style("objc", text, "oneway"), 20);
    assert_eq!(style("objc", text, "a\"\"b"), 13);
}

#[test]
fn cpp_properties_change_verbatim_triple_escapes_and_preprocessor() {
    let text = "\"\"\"a\nb\"\"\"\n\"a\\u0041\"\n#define X 1\n";
    let mut p = profile("cpp");
    p.properties
        .insert("lexer.cpp.triplequoted.strings".into(), "1".into());
    p.properties
        .insert("lexer.cpp.escape.sequence".into(), "1".into());
    p.properties
        .insert("styling.within.preprocessor".into(), "1".into());
    let result = lexers::lex(text, &p).expect("lex");
    let at = |needle: &str| {
        result
            .spans
            .iter()
            .find(|span| span.range.contains(&text.find(needle).expect(needle)))
            .expect("span")
            .style
    };
    assert_eq!(at("a\nb"), 21);
    assert_eq!(at("\\u0041"), 27);
    assert_eq!(at("X"), 11);
    assert_ne!(style("cpp", text, "a\nb"), 21);
}

#[test]
fn javascript_template_interpolation_and_typescript_raw_are_distinct() {
    let text = "const x = `hi ${true ? `${value}` : /[a/]+/g}`;";
    assert_eq!(style("javascript.js", text, "hi"), 20);
    assert_eq!(style("javascript.js", text, "${"), 10);
    assert_eq!(style("javascript.js", text, "value"), 11);
    assert_eq!(style("javascript.js", text, "/[a/]+/g"), 14);
    assert_eq!(style("typescript", text, "${"), 20);
}

#[test]
fn cpp_routed_source_languages_preserve_their_keyword_lists() {
    for (key, text, word, expected) in [
        ("actionscript", "function run() {}", "function", 5),
        ("c", "return x;", "return", 5),
        (
            "cs",
            "namespace A { string s = @\"a\n b\"; }",
            "namespace",
            5,
        ),
        ("java", "package a; class A {}", "package", 16),
        ("go", "package a; var s = `a\n b`", "a\n b", 20),
        ("swift", "func run() { return }", "func", 5),
        ("rc", "DIALOG 0,0,10,10", "DIALOG", 5),
        ("javascript", "function run() {}", "function", 5),
    ] {
        assert_eq!(style(key, text, word), expected, "{key}");
    }
}

#[test]
fn d_nested_comments_and_all_source_raw_forms() {
    let text = "/+ a\n /+ b +/ c\n+/ int n; auto a = r\"raw\ntext\"w; auto b = `back`d;";
    assert_eq!(style("d", text, "b +/"), 4);
    assert_eq!(style("d", text, "raw\ntext"), 19);
    assert_eq!(style("d", text, "back"), 18);
    let result = lexers::lex(text, &profile("d")).expect("lex");
    assert_ne!(result.line_states[0].state, 0);
    assert_ne!(result.line_states[1].state, 0);
}

#[test]
fn rust_raw_bytes_cstrings_lifetimes_macros_and_nested_docs() {
    let text = "/*! outer\n/* inner */ end */\nfn f<'a>(x: &'a str) { let a = br##\"a\n\"#b\"##; let b = cr\"c\"; println!(\"x\"); let c = b'x'; let d = r#type; }";
    for (needle, expected) in [
        ("inner", 3),
        ("'a", 18),
        ("a\n", 22),
        ("cr\"", 25),
        ("println!", 19),
        ("b'x'", 23),
        ("r#type", 17),
    ] {
        assert_eq!(style("rust", text, needle), expected, "{needle}");
    }
}

#[test]
fn avs_has_both_nested_comment_delimiters_and_triple_strings() {
    let text = "[* outer\n[* inner *] end *]\n/* outer /* inner */ end */\nclip = \"\"\"hello\nworld\"\"\"\n# end\n";
    assert_eq!(style("avs", text, "inner *]"), 2);
    assert_eq!(style("avs", text, "inner */"), 1);
    assert_eq!(style("avs", text, "hello"), 8);
    assert_eq!(style("avs", text, "# end"), 3);
}

#[test]
fn oscript_doc_blocks_globals_labels_and_members() {
    let text = "#ifdef DOC\nplain documentation\n#endif\nstart: $$global = obj.member\nobj.method('it''s')\n";
    for (needle, expected) in [
        ("plain", 3),
        ("#endif", 3),
        ("start:", 13),
        ("$$global", 10),
        ("member", 17),
        ("method", 18),
        ("it''s", 6),
    ] {
        assert_eq!(style("oscript", text, needle), expected, "{needle}");
    }
}

#[test]
fn escript_case_property_and_braces_are_source_specific() {
    let text = "IF (value) {\n \"multi\nline\"\n}\nENDIF";
    assert_eq!(style("escript", text, "IF"), 11);
    assert_eq!(style("escript", text, "{"), 9);
    assert_eq!(style("escript", text, "multi\nline"), 6);
    let mut p = profile("escript");
    p.properties
        .insert("escript.case.sensitive".into(), "1".into());
    let result = lexers::lex(text, &p).expect("lex");
    assert_eq!(result.spans[0].style, 8);
}

#[test]
fn gui4cli_classifies_only_command_heads_and_string_escapes() {
    let mut p = profile("gui4cli");
    p.numbered_keywords = vec![
        "GLOBAL".into(),
        "EVENT".into(),
        "ATTRIBUTE".into(),
        "CONTROL".into(),
        "COMMAND".into(),
    ];
    let text = "global command; command \"a\\\"b\"\nevent next\n";
    let result = lexers::lex(text, &p).expect("lex");
    let at = |position| {
        result
            .spans
            .iter()
            .find(|span| span.range.contains(&position))
            .expect("span")
            .style
    };
    assert_eq!(at(0), 3);
    assert_eq!(at(7), 0);
    assert_eq!(at(16), 7);
    assert_eq!(at(text.find("\\\"").expect("escape")), 9);
}

#[test]
fn baan_documentation_declarations_and_preprocessor_objects() {
    let text = "dllusage\nplain documentation\nenddllusage\ndomain custom.name item\nfunction custom.call()\n#include object.name\n#define SYMBOL 1\n| comment\n";
    for (needle, expected) in [
        ("plain", 2),
        ("custom.name", 21),
        ("custom.call", 22),
        ("object.name", 23),
        ("SYMBOL", 24),
        ("| comment", 1),
    ] {
        assert_eq!(style("baanc", text, needle), expected, "{needle}");
    }
}

#[test]
fn json_default_reuses_existing_exact_vectors_and_json5_only_enables_comments() {
    let text = "{\"@id\":\"https://example.test\",\"x\":\"a\\n\",\"b\":true}";
    let result = lexers::lex(text, &profile("json")).expect("json");
    for span in syntax::json_styles(text) {
        for position in span.range {
            assert_eq!(
                result
                    .spans
                    .iter()
                    .find(|item| item.range.contains(&position))
                    .expect("span")
                    .style,
                span.style as u16
            );
        }
    }
    let text = "{ /* allowed\n comment */ \"x\": true // end\n}";
    assert_eq!(style("json5", text, "allowed"), 7);
    assert_eq!(style("json5", text, "// end"), 6);
    assert_eq!(style("json5", "{'key': 1}", "'key'"), 13);
    assert_eq!(style("json", text, "allowed"), 13);
}

#[test]
fn folding_properties_and_utf8_boundaries_are_observable() {
    let text = "int main() {\r\n /* λ\r\n end */\r\n return 0;\r\n}\r\n";
    let p = profile("cpp");
    let result = lexers::lex(text, &p).expect("lex");
    assert!(
        result
            .folds
            .iter()
            .any(|fold| fold.start_line == 0 && fold.end_line == 4)
    );
    assert!(
        result
            .folds
            .iter()
            .any(|fold| fold.start_line == 1 && fold.end_line == 2)
    );
    for span in &result.spans {
        assert!(text.is_char_boundary(span.range.start));
        assert!(text.is_char_boundary(span.range.end));
    }
    let mut disabled = p;
    disabled.properties.insert("fold".into(), "0".into());
    assert!(lexers::lex(text, &disabled).expect("lex").folds.is_empty());
}

#[test]
fn json_keyword_lists_and_escape_property_are_runtime_inputs() {
    let mut p = profile("json5");
    p.numbered_keywords[0] = "custom".into();
    p.numbered_keywords[1] = "@custom".into();
    p.properties
        .insert("lexer.json.escape.sequence".into(), "0".into());
    let text = "{\"x\": custom, \"@custom\": \"a\\n\"}";
    let result = lexers::lex(text, &p).expect("lex");
    let at = |needle: &str| {
        result
            .spans
            .iter()
            .find(|span| span.range.contains(&text.find(needle).expect(needle)))
            .expect("span")
            .style
    };
    assert_eq!(at("custom,"), 11);
    assert_eq!(at("@custom"), 12);
    assert_eq!(at("\\n"), 2);
}

#[test]
fn incremental_edit_restarts_at_safe_line_and_preserves_full_scan_result() {
    let text =
        "int a;\nint b;\nint main() {\n const char *s = R\"tag(a\n b)tag\";\n return 0;\n}\n";
    let p = profile("cpp");
    let mut cache = lexers::IncrementalLexer::new(text, &p).expect("cache");
    let at = text.find("return 0").expect("return") + 7;
    let update = cache.replace(at..at + 1, "42").expect("replace");
    assert!(update.relexed_range.start > 0);
    assert!(update.reused_prefix_lines >= 2);
    assert_eq!(
        cache.result(),
        &lexers::lex(cache.text(), &p).expect("full scan")
    );
    let at = cache.text().find("b)tag").expect("raw body");
    cache.replace(at..at + 1, "c\n d").expect("replace raw");
    assert_eq!(
        cache.result(),
        &lexers::lex(cache.text(), &p).expect("full scan")
    );
    assert!(matches!(
        cache.replace(usize::MAX..usize::MAX, ""),
        Err(lexers::LexError::InvalidRange(_))
    ));
}

#[test]
fn cpp_prefix_literals_preprocessor_resume_and_user_literals() {
    let text = "L\"wide\" u8\"utf\" U'x' 10_km\n#define X /* note */ value\n\"broken\nnext";
    assert_eq!(style("cpp", text, "L\""), 6);
    assert_eq!(style("cpp", text, "u8\""), 6);
    assert_eq!(style("cpp", text, "U'"), 7);
    assert_eq!(style("cpp", text, "10_km"), 25);
    assert_eq!(style("cpp", text, "note"), 23);
    assert_eq!(style("cpp", text, "value"), 9);
    let result = lexers::lex(text, &profile("cpp")).expect("lex");
    let newline = text.find("broken\n").expect("broken") + 6;
    assert_eq!(
        result
            .spans
            .iter()
            .find(|span| span.range.contains(&newline))
            .expect("newline")
            .style,
        12
    );
}

#[test]
fn source_range_operators_do_not_become_fractional_numbers() {
    let text = "0..2";
    for (key, expected) in [("d", 13), ("rust", 16)] {
        let result = lexers::lex(text, &profile(key)).expect("lex");
        for position in 1..3 {
            assert_eq!(
                result
                    .spans
                    .iter()
                    .find(|span| span.range.contains(&position))
                    .expect("operator")
                    .style,
                expected,
                "{key}"
            );
        }
    }
}

#[test]
fn gui4cli_fold_headers_are_siblings_not_nested() {
    let mut p = profile("gui4cli");
    p.numbered_keywords = vec![
        "GLOBAL".into(),
        "EVENT".into(),
        String::new(),
        String::new(),
        String::new(),
    ];
    let result = lexers::lex("GLOBAL a\nbody\nEVENT b\nbody\n", &p).expect("lex");
    assert_eq!(result.fold_levels[0].level, 0x400);
    assert_eq!(result.fold_levels[2].level, 0x400);
    assert_eq!(result.fold_levels[3].level, 0x401);
    assert!(
        result
            .folds
            .iter()
            .any(|fold| fold.start_line == 0 && fold.end_line == 1)
    );
}

#[test]
fn rust_numeric_suffix_and_method_boundaries_follow_rust_handler() {
    let text = "1.foo() 0x 0b2 1u128 3f64 1e+";
    assert_eq!(style("rust", text, "foo"), 17);
    assert_eq!(style("rust", text, "0x"), 20);
    assert_eq!(style("rust", text, "0b"), 20);
    assert_eq!(style("rust", text, "1u128"), 5);
    assert_eq!(style("rust", text, "3f64"), 5);
    assert_eq!(style("rust", text, "1e+"), 20);
}

#[test]
fn multiline_javascript_template_folds_and_cache_match_fresh_lexing() {
    let text = "const a = 1;\nfunction f() {\n const t = `a\n ${true ? `${x}` : y}\n b`;\n return t;\n}\nconst z = 2;\n";
    let p = profile("javascript.js");
    let result = lexers::lex(text, &p).expect("lex");
    assert!(
        result
            .folds
            .iter()
            .any(|fold| fold.start_line == 1 && fold.end_line == 6)
    );
    for (position, ch) in text
        .char_indices()
        .filter(|(_, ch)| !matches!(ch, '\r' | '\n'))
    {
        let mut cache = lexers::IncrementalLexer::new(text, &p).expect("cache");
        cache
            .replace(position..position + ch.len_utf8(), "λ")
            .expect("edit");
        assert_eq!(
            cache.result(),
            &lexers::lex(cache.text(), &p).expect("fresh"),
            "position {position}"
        );
    }
}

#[test]
fn rust_source_recovery_retains_string_style_and_permissive_characters() {
    assert_eq!(style("rust", "\"unterminated", "unterminated"), 13);
    assert_eq!(style("rust", "'ab'", "'ab'"), 15);
    assert_eq!(style("rust", "b\"λ\"", "λ"), 17);
    assert_eq!(style("rust", "r###not_string", "r###"), 20);
}

#[test]
fn configurable_json_machine_matches_default_recovery_vectors() {
    let vectors = [
        "{\"key\":\"hi\\n\",\"n\":-12.5e+2,\"ok\":true}",
        "\"https://example.test/x\" \"@id\" \"a:b\"",
        "wrong\n\"unterminated\n[\u{b}1]\r\n{\"vt\"\u{b}:1}",
        "\"\\u0041\\z\"\ntrue 2E+4 null -1 .9",
        "{\"x\" /* not a property lookahead */ :1}",
    ];
    let mut p = profile("json");
    p.numbered_keywords[1].push_str(" @unused");
    for text in vectors {
        let result = lexers::lex(text, &p).expect("lex");
        for expected in syntax::json_styles(text) {
            for position in expected.range {
                assert_eq!(
                    result
                        .spans
                        .iter()
                        .find(|span| span.range.contains(&position))
                        .expect("span")
                        .style,
                    expected.style as u16,
                    "{text:?} at {position}"
                );
            }
        }
    }
}

#[test]
fn cpp_documentation_xml_tokens_and_email_boundaries() {
    let text = "/// mail a@example.test <summary>text</summary> @param,\n";
    let mut p = profile("cs");
    p.numbered_keywords[2].push_str(" summary>");
    let result = lexers::lex(text, &p).expect("lex");
    let at = |needle: &str| {
        result
            .spans
            .iter()
            .find(|span| span.range.contains(&text.find(needle).expect(needle)))
            .expect("span")
            .style
    };
    assert_eq!(at("@example"), 15);
    assert_eq!(at("summary>"), 17);
    assert_eq!(at("@param"), 18);
}

#[test]
fn baan_table_sql_and_library_templates_are_distinct() {
    let text = "abcde123 tabcde123 abcde123.field rcd.tabcde123 abcde.dll1234.call";
    for (needle, expected) in [
        ("abcde123 ", 18),
        ("tabcde123", 18),
        ("abcde123.field", 19),
        ("rcd.tabcde123", 18),
        ("abcde.dll1234.call", 20),
    ] {
        assert_eq!(style("baanc", text, needle), expected, "{needle}");
    }
    let mut p = profile("baanc");
    p.numbered_keywords[1] = "prefix~suffix".into();
    let result = lexers::lex("prefixmiddlesuffix", &p).expect("lex");
    assert_eq!(result.spans[0].style, 10);
}
