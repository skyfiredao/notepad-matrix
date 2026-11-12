use notepad_matrix::intelligence::*;

#[test]
fn all_source_extraction_patterns_compile_and_known_comment_defect_is_reported() {
    let catalog = IntelligenceCatalog::bundled().expect("bundled catalog");
    assert_eq!(catalog.api_keys().count(), 34);
    assert_eq!(catalog.parser_keys().count(), 47);
    for key in catalog.parser_keys() {
        catalog
            .validate_extraction_patterns(key)
            .expect("every static extraction pattern must compile");
        if key == "fortran77" {
            assert!(catalog.validate_parser_patterns(key).is_err());
        } else {
            catalog
                .validate_parser_patterns(key)
                .expect("all other shipped patterns compile");
        }
    }
    let python = catalog.api("python").expect("python API");
    assert_eq!(python.environment.additional_word_chars, ".");
    let abs = python
        .keywords
        .iter()
        .find(|word| word.name == "abs")
        .expect("abs");
    assert!(abs.is_function);
    assert_eq!(
        abs.overloads[0].description,
        "abs(number) -> number\n\nReturn the absolute value of the argument."
    );
}

#[test]
fn python_classes_comments_strings_and_byte_ranges() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "# λ\n# def fake():\nvalue = \"def fake_string():\"\nclass Box:\n    def run(self, value):\n        return value\n\ndef free(x):\n    pass\n";
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("python", text).expect("parse")
    else {
        panic!("missing parser")
    };
    assert_eq!(
        symbols.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Box", "free(x)"]
    );
    assert_eq!(symbols[0].children[0].name, "run(self, value)");
    assert_eq!(
        &text[symbols[0].children[0].name_range.clone()],
        "run(self, value)"
    );
    assert_eq!(
        symbols[0].children[0].jump_byte,
        text.find("run(self").expect("position")
    );
    assert_eq!(
        catalog
            .parse_functions("no_original_parser", text)
            .expect("absent"),
        ParseOutcome::NoParser
    );
}

#[test]
fn upstream_independent_function_list_fixtures() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../assets/intelligence/fixtures.json"))
            .expect("fixtures");
    let mut failures = Vec::new();
    for (key, fixture) in fixtures.as_object().expect("fixture object") {
        if !catalog.parser_keys().any(|k| k == key) {
            continue;
        }
        let text = fixture["text"].as_str().expect("text");
        let result = catalog.parse_functions(key, text);
        let symbols = match result {
            Ok(ParseOutcome::Parsed(symbols)) => symbols,
            other => {
                failures.push(format!("{key}: {other:?}"));
                continue;
            }
        };
        let mut actual = Vec::new();
        fn leaves(symbols: &[FunctionSymbol], actual: &mut Vec<String>) {
            for symbol in symbols {
                if symbol.kind == SymbolKind::Function {
                    actual.push(symbol.name.clone());
                }
                leaves(&symbol.children, actual);
            }
        }
        leaves(&symbols, &mut actual);
        let mut expected = Vec::new();
        fn expected_leaves(node: &serde_json::Value, expected: &mut Vec<String>) {
            if let Some(leaves) = node["leaves"].as_array() {
                for leaf in leaves {
                    expected.push(leaf.as_str().expect("leaf").into());
                }
            }
            if let Some(nodes) = node["nodes"].as_array() {
                for child in nodes {
                    expected_leaves(child, expected);
                }
            }
        }
        expected_leaves(&fixture["expected"], &mut expected);
        for value in &mut expected {
            *value = value.replace("\r\n", "\n");
        }
        for value in &mut actual {
            *value = value.replace("\r\n", "\n");
        }
        actual.sort();
        expected.sort();
        if actual != expected {
            failures.push(format!("{key}: actual {actual:?}, expected {expected:?}"));
        }
        fn actual_groups(
            symbols: &[FunctionSymbol],
            parent: &str,
            groups: &mut Vec<(String, String)>,
        ) {
            for symbol in symbols {
                if symbol.kind == SymbolKind::Class {
                    let name = if parent.is_empty() {
                        symbol.name.clone()
                    } else {
                        format!("{parent}.{}", symbol.name)
                    };
                    actual_groups(&symbol.children, &name, groups);
                } else {
                    groups.push((parent.into(), symbol.name.replace("\r\n", "\n")));
                    actual_groups(&symbol.children, parent, groups);
                }
            }
        }
        fn expected_groups(
            node: &serde_json::Value,
            parent: &str,
            groups: &mut Vec<(String, String)>,
        ) {
            if let Some(leaves) = node["leaves"].as_array() {
                for leaf in leaves {
                    groups.push((
                        parent.into(),
                        leaf.as_str().expect("leaf").replace("\r\n", "\n"),
                    ));
                }
            }
            if let Some(nodes) = node["nodes"].as_array() {
                for child in nodes {
                    let child_name = child["name"].as_str().expect("class");
                    let name = if parent.is_empty() {
                        child_name.into()
                    } else {
                        format!("{parent}.{child_name}")
                    };
                    expected_groups(child, &name, groups);
                }
            }
        }
        let mut actual_associations = Vec::new();
        let mut expected_associations = Vec::new();
        actual_groups(&symbols, "", &mut actual_associations);
        expected_groups(&fixture["expected"], "", &mut expected_associations);
        actual_associations.sort();
        expected_associations.sort();
        if actual_associations != expected_associations {
            failures.push(format!("{key}: class associations actual {actual_associations:?}, expected {expected_associations:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn completion_filters_words_orders_and_submits_only_prefix() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "printer print print_value 12345\npriSuffix";
    let caret = text.find("priSuffix").expect("prefix") + 3;
    let options = CompletionOptions {
        mode: CompletionMode::Words,
        ..CompletionOptions::default()
    };
    let result = catalog
        .complete("python", text, caret, &options)
        .expect("completion");
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        ["print", "print_value", "printer"]
    );
    let edit = result
        .submit(0, SubmitKey::Tab, &SubmitOptions::default())
        .expect("submit");
    assert_eq!(edit.replace_range, caret - 3..caret);
    assert_eq!(edit.text, "print");
    assert!(
        result
            .submit(
                0,
                SubmitKey::Enter,
                &SubmitOptions {
                    enter: false,
                    ..SubmitOptions::default()
                }
            )
            .is_none()
    );
    let api = catalog
        .complete(
            "python",
            "ab",
            2,
            &CompletionOptions {
                mode: CompletionMode::Api,
                ..CompletionOptions::default()
            },
        )
        .expect("API completion");
    assert!(
        api.candidates
            .iter()
            .any(|c| c.text == "abs" && c.kind == CompletionKind::Function)
    );
}

#[test]
fn signature_help_tracks_nested_calls_quotes_parameters_and_terminal() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "printf(\"x,(y)\", strlen(\"ab\"), ";
    let help = catalog
        .signature_help("c", text, text.len(), 0)
        .expect("help")
        .expect("printf help");
    assert_eq!(help.function_name, "printf");
    assert_eq!(help.active_parameter, 2);
    assert_eq!(
        help.signatures[0].label,
        "int printf(const char *format, ...)"
    );
    assert_eq!(
        &help.signatures[0].label[help.signatures[0].parameters[0].clone()],
        "const char *format"
    );
    let inner = "printf(\"x\", strlen(";
    assert_eq!(
        catalog
            .signature_help("c", inner, inner.len(), 0)
            .expect("inner")
            .expect("strlen")
            .function_name,
        "strlen"
    );
    assert!(
        catalog
            .signature_help("c", "printf(x);", 10, 0)
            .expect("closed")
            .is_none()
    );
    assert!(
        catalog
            .signature_help("c", "printf(x; ", 10, 0)
            .expect("terminated")
            .is_none()
    );
}

#[test]
fn remaining_source_parser_families_extract_positive_and_reject_negative() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let cases = [
        (
            "baanc",
            "function long calculate()\n{\n}\n",
            "calculate",
            "not_a_function()\n",
        ),
        (
            "cobol",
            "       COMPUTE-TOTAL SECTION.\n           DISPLAY 'X'.\n",
            "COMPUTE-TOTAL SECTION",
            "       EXIT SECTION.\n",
        ),
        (
            "cobol-free",
            "\nCOMPUTE-TOTAL SECTION.\n",
            "COMPUTE-TOTAL SECTION",
            "\nEXIT SECTION.\n",
        ),
        (
            "krl",
            "DEF Measure()\nEND\n",
            "Measure",
            "; DEF Ghost()\nEND\n",
        ),
        (
            "nppexec",
            ":: Actions\n:run\necho work\n:: Sentinel\n",
            "run",
            "echo :not_a_label\n",
        ),
        (
            "sinumerik",
            "%_N_SAMPLE_MPF\nN10 M30\n",
            "SAMPLE_MPF",
            "; %_N_GHOST_MPF\n",
        ),
        (
            "universe_basic",
            "Start:\nPRINT 'OK'\n",
            "Start",
            "REM Ghost:\nPRINT \"Ghost:\"\n",
        ),
    ];
    fn names(symbols: &[FunctionSymbol]) -> Vec<String> {
        symbols
            .iter()
            .flat_map(|symbol| std::iter::once(symbol.name.clone()).chain(names(&symbol.children)))
            .collect()
    }
    for (key, positive, expected, negative) in cases {
        let ParseOutcome::Parsed(symbols) = catalog.parse_functions(key, positive).expect(key)
        else {
            panic!("missing {key}")
        };
        assert!(
            names(&symbols).iter().any(|name| name.trim() == expected),
            "{key}: {symbols:?}"
        );
        let ParseOutcome::Parsed(symbols) = catalog.parse_functions(key, negative).expect(key)
        else {
            panic!("missing {key}")
        };
        assert!(symbols.is_empty(), "negative {key}: {symbols:?}");
    }
}

#[test]
fn source_utf16_word_classes_are_not_rust_unicode_word_classes() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "class Δ:\n    pass\n\nclass A\u{301}:\n    pass\n\nclass \u{10400}:\n    pass\n";
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("python", text).expect("Unicode")
    else {
        panic!("missing")
    };
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["Δ"]
    );
}

#[test]
fn recursive_static_patterns_do_not_silently_truncate_at_engine_depth() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let c = format!("int {}deep(void) {{ return 0; }}\n", "*".repeat(64));
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("c", &c).expect("C") else {
        panic!("missing")
    };
    assert_eq!(symbols[0].name, "deep(void)");
    assert_eq!(symbols[0].range.start, 0);
    let nim = format!(
        "proc deep(a = {}1{}): int =\n  1\n",
        "(".repeat(64),
        ")".repeat(64)
    );
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("nim", &nim).expect("Nim") else {
        panic!("missing")
    };
    assert!(symbols[0].name.ends_with("): int"), "{}", symbols[0].name);
    let pascal = format!(
        "{}begin\nend;\n",
        (0..40)
            .map(|i| format!("procedure nested{i};\n"))
            .collect::<String>()
    );
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("pascal", &pascal).expect("Pascal")
    else {
        panic!("missing")
    };
    assert_eq!(symbols.len(), 40);
}

#[test]
fn invalid_definition_is_an_error_even_when_nothing_would_match() {
    let node = serde_json::json!({"tag":"parser","attributes":{"id":"broken"},"children":[{"tag":"function","attributes":{"mainExpr":"never_matches"},"children":[{"tag":"functionName","attributes":{},"children":[{"tag":"nameExpr","attributes":{"expr":"(?unknown)"},"children":[]}]}]}]});
    let parsers = serde_json::json!({"broken":{"definition":node}}).to_string();
    let catalog = IntelligenceCatalog::from_json("{}", &parsers).expect("valid JSON structure");
    assert!(catalog.parse_functions("broken", "unrelated text").is_err());
    let unknown = parsers.replace("functionName", "unknownDefinitionNode");
    assert!(IntelligenceCatalog::from_json("{}", &unknown).is_err());
    let recursive = serde_json::json!({"custom":{"definition":{"tag":"parser","attributes":{"id":"custom"},"children":[{"tag":"function","attributes":{"mainExpr":"(a(?1)?)"},"children":[]}]}}}).to_string();
    let catalog = IntelligenceCatalog::from_json("{}", &recursive).expect("recursive JSON");
    assert!(catalog.parse_functions("custom", "").is_err());
}

#[test]
fn malformed_shipped_fortran77_comment_expression_has_source_empty_zone_behavior() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "C COMMENT\n      FUNCTION Real(A) ! trailing comment\n      END\n";
    assert!(
        catalog
            .source_comment_zones("fortran77", text)
            .expect("source zones")
            .is_empty()
    );
    let diagnostics = catalog.pattern_diagnostics("fortran77");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].field, "parser@commentExpr");
    assert!(diagnostics[0].message.contains("Opening parenthesis"));
    let ParseOutcome::Parsed(symbols) = catalog
        .parse_functions("fortran77", text)
        .expect("main extraction still runs")
    else {
        panic!("missing")
    };
    assert_eq!(symbols[0].name, "Real");
    assert!(catalog.validate_patterns().is_err());
}

#[test]
fn path_completion_resolves_relative_directories_filters_and_preserves_spaces() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::create_dir(temporary.path().join("child")).expect("child");
    std::fs::write(temporary.path().join("child/alpha file.rs"), "").expect("file");
    std::fs::write(temporary.path().join("child/beta.rs"), "").expect("file");
    std::fs::create_dir(temporary.path().join("child/alpha folder")).expect("folder");
    let text = "open \"child/alpha";
    let result = catalog
        .complete_path(text, text.len(), temporary.path())
        .expect("paths");
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["child/alpha file.rs", "child/alpha folder/"]
    );
    assert_eq!(result.replace_range, 6..text.len());
    assert_eq!(result.candidates[1].kind, CompletionKind::Directory);
    let directory = "child/";
    assert_eq!(
        catalog
            .complete_path(directory, directory.len(), temporary.path())
            .expect("directory")
            .candidates
            .len(),
        3
    );
    let existing_file = "child/beta.rs";
    assert!(
        catalog
            .complete_path(existing_file, existing_file.len(), temporary.path())
            .expect("existing file")
            .candidates
            .is_empty()
    );
    std::fs::write(temporary.path().join("child/alpha folder/inside.txt"), "")
        .expect("nested file");
    let existing_directory = "child/alpha folder";
    let nested = catalog
        .complete_path(
            &format!("\"{existing_directory}"),
            existing_directory.len() + 1,
            temporary.path(),
        )
        .expect("existing directory");
    assert_eq!(nested.candidates[0].text, "child/alpha folder/inside.txt");
    let context = path_context("open \"C:\\Program Files\\Not", 26)
        .expect("Windows context")
        .expect("path");
    assert_eq!(context.1, "C:\\Program Files\\Not");
}

#[test]
fn runtime_apis_preserve_every_source_keyword_overload_parameter_and_environment() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let raw: serde_json::Value =
        serde_json::from_str(include_str!("../assets/intelligence/apis.json")).expect("JSON");
    let mut total_keywords = 0;
    for (key, source) in raw.as_object().expect("sources") {
        let api = catalog.api(key).expect(key);
        let children = source["children"].as_array().expect("children");
        assert_eq!(
            api.language.as_deref(),
            source["attributes"]["language"].as_str()
        );
        let source_words = children
            .iter()
            .filter(|node| node["tag"] == "KeyWord")
            .collect::<Vec<_>>();
        assert_eq!(api.keywords.len(), source_words.len(), "{key}");
        for (word, source_word) in api.keywords.iter().zip(source_words) {
            total_keywords += 1;
            assert_eq!(
                word.name,
                source_word["attributes"]["name"].as_str().expect("name"),
                "{key}"
            );
            assert_eq!(word.is_function, source_word["attributes"]["func"] == "yes");
            let source_overloads = source_word["children"].as_array().expect("overloads");
            assert_eq!(word.overloads.len(), source_overloads.len());
            for (overload, source_overload) in word.overloads.iter().zip(source_overloads) {
                assert_eq!(
                    overload.return_value,
                    source_overload["attributes"]["retVal"]
                        .as_str()
                        .unwrap_or("")
                );
                assert_eq!(
                    overload.description,
                    source_overload["attributes"]["descr"]
                        .as_str()
                        .unwrap_or("")
                );
                let parameters = source_overload["children"].as_array().expect("parameters");
                assert_eq!(overload.parameters.len(), parameters.len());
                for (parameter, source_parameter) in overload.parameters.iter().zip(parameters) {
                    assert_eq!(
                        parameter.name,
                        source_parameter["attributes"]["name"]
                            .as_str()
                            .expect("parameter name")
                    );
                }
            }
        }
        if let Some(environment) = children.iter().find(|node| node["tag"] == "Environment") {
            let attrs = &environment["attributes"];
            assert_eq!(api.environment.ignore_case, attrs["ignoreCase"] != "no");
            assert_eq!(
                api.environment.start_function,
                attrs["startFunc"]
                    .as_str()
                    .unwrap_or("(")
                    .chars()
                    .next()
                    .unwrap_or('(')
            );
            assert_eq!(
                api.environment.stop_function,
                attrs["stopFunc"]
                    .as_str()
                    .unwrap_or(")")
                    .chars()
                    .next()
                    .unwrap_or(')')
            );
            assert_eq!(
                api.environment.parameter_separator,
                attrs["paramSeparator"]
                    .as_str()
                    .unwrap_or(",")
                    .chars()
                    .next()
                    .unwrap_or(',')
            );
            assert_eq!(
                api.environment.terminal,
                Some(
                    attrs["terminal"]
                        .as_str()
                        .unwrap_or(";")
                        .chars()
                        .next()
                        .unwrap_or(';')
                )
            );
            assert_eq!(
                api.environment.additional_word_chars,
                attrs["additionalWordChar"].as_str().unwrap_or("")
            );
        }
    }
    assert_eq!(total_keywords, 27_896);
}

#[test]
fn lexical_comments_and_string_delimiters_do_not_hide_or_invent_symbols() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let java = "/*\nclass Ghost {\n public int fake() { return 0; }\n}\n*/\nclass Real {\n public int live() { return 1; }\n}\n";
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("java", java).expect("Java") else {
        panic!("missing")
    };
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Real");
    assert_eq!(symbols[0].children[0].name, "live");
    let javascript =
        "const url = \"http://host\"; function live() { const s = \"function ghost() {}\"; }\n";
    let ParseOutcome::Parsed(symbols) = catalog
        .parse_functions("javascript", javascript)
        .expect("JavaScript")
    else {
        panic!("missing")
    };
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["live"]
    );
    let css = "/*\n.fake {\n}\n*/\n.real { color: red; }\n";
    let ParseOutcome::Parsed(symbols) = catalog.parse_functions("css", css).expect("CSS") else {
        panic!("missing")
    };
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.name.trim())
            .collect::<Vec<_>>(),
        [".real"]
    );
}

#[test]
fn word_completion_matches_source_word_boundaries_and_submission_policies() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "folder/printer other-printable pri";
    let options = CompletionOptions {
        mode: CompletionMode::Words,
        order: CompletionOrder::Source,
        ..CompletionOptions::default()
    };
    let result = catalog
        .complete("text", text, text.len(), &options)
        .expect("words");
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["printer", "printable"]
    );
    let edit = result
        .submit(
            0,
            SubmitKey::Character('('),
            &SubmitOptions {
                fill_up_characters: "(".into(),
                ..SubmitOptions::default()
            },
        )
        .expect("fill-up");
    assert_eq!(edit.text, "printer(");
    assert_eq!(edit.caret_byte, result.replace_range.start + 8);
    assert!(
        result
            .submit(0, SubmitKey::Character(';'), &SubmitOptions::default())
            .is_none()
    );
    assert!(
        result
            .submit(0, SubmitKey::Click, &SubmitOptions::default())
            .is_some()
    );
    let numbers = catalog
        .complete("text", "12345 12", 8, &options)
        .expect("numbers");
    assert!(numbers.candidates.is_empty());
    assert!(catalog.complete("text", "λ", 1, &options).is_err());
}

#[test]
fn java_structural_profile_keeps_recursive_types_nested_classes_and_precise_ranges() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let text = "class Outer<T extends Map<String, List<Integer>>> {\n public Map<String, List<Map<String,Integer>>> build(final List<? extends Number> xs) throws Exception { return null; }\n class Inner {\n  public int count() { return 1; }\n }\n public other words invalid() { return 0; }\n}\n";
    let ParseOutcome::Parsed(symbols) = catalog
        .parse_functions("java", text)
        .expect("Java structural profile")
    else {
        panic!("missing")
    };
    assert_eq!(symbols.len(), 1);
    let outer = &symbols[0];
    assert!(outer.name.starts_with("Outer<"));
    assert_eq!(outer.range.end, text.trim_end().len());
    assert_eq!(
        outer
            .children
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["build", "Inner"]
    );
    assert_eq!(outer.children[1].children[0].name, "count");
    assert_eq!(
        &text[outer.children[1].children[0].name_range.clone()],
        "count"
    );
    assert_eq!(
        outer.children[0].jump_byte,
        text.find("build(").expect("build")
    );
}

#[test]
fn shipped_udl_override_names_resolve_to_their_original_parser_definitions() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    assert_eq!(catalog.parser_associations().count(), 4);
    let text = "Start:\nPRINT 'OK'\n";
    let ParseOutcome::Parsed(symbols) = catalog
        .parse_functions("UniVerse BASIC", text)
        .expect("UDL override")
    else {
        panic!("source definition exists")
    };
    assert_eq!(symbols[0].name, "Start");
    assert_eq!(
        catalog.parse_functions("UniVerse BASIC", text),
        catalog.parse_functions("universe_basic.xml", text)
    );
}

#[test]
fn raw_and_long_strings_do_not_create_function_symbols() {
    let catalog = IntelligenceCatalog::bundled().expect("catalog");
    let cases = [
        (
            "rust",
            "const S: &str = r###\"quote \"\nfn ghost() {}\n\"###;\nfn live() {}\n",
            "live",
        ),
        (
            "cpp",
            "const char* s = R\"tag(quote \"\nint ghost() {}\n)tag\";\nint live() {}\n",
            "live",
        ),
        (
            "lua",
            "local s = [==[\nfunction ghost()\nend\n]==]\nfunction live()\nend\n",
            "live",
        ),
    ];
    for (key, text, expected) in cases {
        let ParseOutcome::Parsed(symbols) = catalog.parse_functions(key, text).expect(key) else {
            panic!("missing")
        };
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            [expected],
            "{key}"
        );
    }
}
