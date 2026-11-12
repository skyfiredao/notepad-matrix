use notepad_matrix::{languages, lexers};

fn profile() -> languages::LanguageDefinition {
    let mut p = languages::catalog()
        .expect("catalog")
        .by_name("cpp")
        .expect("cpp")
        .clone();
    p.properties
        .insert("lexer.cpp.track.preprocessor".into(), "1".into());
    p
}

fn at(result: &lexers::LexResult, text: &str, needle: &str) -> u16 {
    let position = text.find(needle).expect(needle);
    result
        .spans
        .iter()
        .find(|span| span.range.contains(&position))
        .expect("span")
        .style
}

#[test]
fn inactive_sections_preserve_parent_and_taken_branch_state() {
    let text =
        "#if 0\n#if 1\nint nested;\n#endif\n#elif 1\nint chosen;\n#else\nint late;\n#endif\n";
    let result = lexers::lex(text, &profile()).expect("lex");
    assert_eq!(at(&result, text, "nested"), 75);
    assert_eq!(at(&result, text, "chosen"), 11);
    assert_eq!(at(&result, text, "late"), 75);
    assert_eq!(at(&result, text, "#elif"), 9);
    let mut p = profile();
    p.properties
        .insert("lexer.cpp.track.preprocessor".into(), "0".into());
    assert_eq!(at(&lexers::lex(text, &p).expect("lex"), text, "nested"), 11);
}

#[test]
fn macro_updates_and_source_expression_rules_select_the_branch() {
    let text = "#define VER(a,b) a*10+b\n#define LOCAL 7\n#if defined(EXT) && VER(2,LOCAL) == 27\nint selected;\n#endif\n#if LOCAL == 7\nint local_value;\n#endif\n#undef EXT\n#ifdef EXT\nint removed;\n#endif\n";
    let mut p = profile();
    p.numbered_keywords[4] = "EXT=3".into();
    let result = lexers::lex(text, &p).expect("lex");
    assert_eq!(at(&result, text, "selected"), 11);
    assert_eq!(at(&result, text, "local_value"), 11);
    assert_eq!(at(&result, text, "removed"), 75);
    p.properties
        .insert("lexer.cpp.update.preprocessor".into(), "0".into());
    let result = lexers::lex(text, &p).expect("lex");
    assert_eq!(at(&result, text, "local_value"), 75);
    assert_eq!(at(&result, text, "removed"), 11);
    let text = "#if 1 || 0 && 0\nint precedence;\n#endif\n";
    assert_eq!(
        at(&lexers::lex(text, &p).expect("lex"), text, "precedence"),
        75
    );
}

#[test]
fn recursion_nesting_and_malformed_unicode_do_not_break_tracking() {
    let text = format!(
        "{}#if 0\nint beyond_limit;\n{}",
        "#if 1\n".repeat(31),
        "#endif\n".repeat(32)
    );
    assert_eq!(
        at(
            &lexers::lex(&text, &profile()).expect("lex"),
            &text,
            "beyond_limit"
        ),
        11
    );
    let text =
        "#define RECUR RECUR\n#if RECUR\nint recursive;\n#endif\n#ifdefλ\nint malformed;\n#endif\n";
    let result = lexers::lex(text, &profile()).expect("lex");
    assert_eq!(at(&result, text, "recursive"), 11);
    assert_eq!(at(&result, text, "malformed"), 75);
}

#[test]
fn continued_directives_and_incremental_macro_edits_remain_correct() {
    let text = "/* prefix */ #if \\\n0\nint hidden;\n#endif\n#define ENABLED 1\nint middle;\n#if ENABLED\nint selected;\n#endif\n";
    let p = profile();
    let mut cache = lexers::IncrementalLexer::new(text, &p).expect("cache");
    assert_eq!(at(cache.result(), text, "0\n"), 9);
    assert_eq!(at(cache.result(), text, "hidden"), 75);
    let start = text.find("ENABLED 1").expect("macro") + 8;
    cache.replace(start..start + 1, "0").expect("replace");
    assert_eq!(
        cache.result(),
        &lexers::lex(cache.text(), &p).expect("full")
    );
    assert_eq!(at(cache.result(), cache.text(), "selected"), 75);
}

#[test]
fn remaining_cpp_property_regressions() {
    let mut p = profile();
    p.properties
        .insert("styling.within.preprocessor".into(), "1".into());
    p.properties
        .insert("lexer.cpp.backquoted.strings".into(), "2".into());
    let text = "#custom_name 1\nR\"tag(${value})tag\"";
    let result = lexers::lex(text, &p).expect("lex");
    assert_eq!(at(&result, text, "_name"), 9);
    assert_eq!(at(&result, text, "value"), 11);
    p.properties
        .insert("lexer.cpp.triplequoted.strings".into(), "1".into());
    let text = "\"\"\"a\"\"\"\" tail";
    assert_eq!(at(&lexers::lex(text, &p).expect("lex"), text, "tail"), 11);
    p.properties.insert("fold.compact".into(), "1".into());
    assert!(!lexers::lex("", &p).expect("lex").fold_levels[0].white);
}
