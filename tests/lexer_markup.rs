use notepad_matrix::languages::{LanguageProfile, catalog};
use notepad_matrix::lexers::{self, LexResult, markup};

fn profile(key: &str) -> LanguageProfile {
    catalog().expect("catalog").by_name(key).expect(key).clone()
}

fn styled(key: &str, text: &str) -> LexResult {
    lexers::lex(text, &profile(key)).expect("lexing")
}

fn style(result: &LexResult, offset: usize) -> u16 {
    result
        .spans
        .iter()
        .find(|span| span.range.contains(&offset))
        .map_or(0, |span| span.style)
}

#[test]
fn properties_source_styles_and_initial_space_property() {
    let text = "[section]\nkey=value\n; comment\nplain text\n";
    let result = styled("ini", text);
    assert_eq!(style(&result, 0), 2);
    assert_eq!(style(&result, text.find("key").expect("key")), 5);
    assert_eq!(style(&result, text.find('=').expect("assignment")), 3);
    assert_eq!(style(&result, text.find(';').expect("comment")), 1);
    assert_eq!(style(&result, text.find("plain").expect("plain")), 0);
    let mut p = profile("props");
    p.properties
        .insert("lexer.props.allow.initial.spaces".into(), "0".into());
    let result = markup::lex(" key=value", &p).expect("lexing");
    assert_eq!(style(&result, 1), 0);
}

#[test]
fn html_markup_entities_comments_dtd_and_structural_ranges() {
    let text = "<!DOCTYPE html [<!ENTITY x 'value'>]>\n<div class=\"x\" data-id='1'>&amp;<!-- <fake> -->\n<span>text</span>\n</div>";
    let result = styled("html", text);
    for (needle, expected) in [
        ("DOCTYPE", 22),
        ("'value'", 25),
        ("div", 1),
        ("class", 3),
        ("\"x\"", 6),
        ("data-id", 3),
        ("&amp;", 10),
        ("<!--", 9),
        ("text", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    let tags = markup::structural_tokens(text, &profile("html"));
    assert_eq!(
        tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["div", "span", "span", "div"]
    );
    assert_eq!(&text[tags[0].name_range.clone()], "div");
    assert_eq!(tags[0].attributes.len(), 2);
    assert!(tags[3].closing);
    assert!(!result.folds.is_empty());
}

#[test]
fn xml_cdata_processing_instruction_script_property_and_case() {
    let text = "<?xml version='1.0'?>\n<Root attr=\"a\"><![CDATA[<fake>&amp;]]>\n<script>const x=1;</script>\n</Root>";
    let result = styled("xml", text);
    assert_eq!(style(&result, 0), 12);
    assert_eq!(style(&result, text.find("?>").expect("end")), 13);
    assert_eq!(style(&result, text.find("<![CDATA[").expect("cdata")), 17);
    assert_eq!(style(&result, text.find("const").expect("script")), 0);
    let mut p = profile("xml");
    p.properties
        .insert("lexer.xml.allow.scripts".into(), "1".into());
    let result = markup::lex(text, &p).expect("lexing");
    assert_eq!(style(&result, text.find("const").expect("script")), 46);
    let tags = markup::structural_tokens(text, &p);
    assert_eq!(tags[0].name, "Root");
    assert!(!tags.iter().any(|tag| tag.name == "fake"));
}

#[test]
fn php_strings_heredoc_variables_and_attribute_embedding() {
    let text = "<a title=\"<?= $name ?>\"><?php\n/* comment */\necho \"hello $user {$obj->name}\";\n$x = <<<'END'\n$literal ?>\nEND;\n?>after</a>";
    let result = styled("php", text);
    for (needle, expected) in [
        ("<?=", 18),
        ("$name", 123),
        ("/* comment", 124),
        ("echo", 121),
        ("hello", 119),
        ("$user", 126),
        ("{$obj", 104),
        ("$literal", 120),
        ("after", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert_eq!(markup::structural_tokens(text, &profile("php")).len(), 2);
    let mut p = profile("php");
    p.properties
        .insert("lexer.html.allow.php".into(), "0".into());
    let result = markup::lex("<?php echo 1 ?>", &p).expect("lexing");
    assert_ne!(style(&result, 6), 121);
}

#[test]
fn asp_and_jsp_source_server_languages_and_directives() {
    for key in ["asp", "jsp"] {
        let text = "<p><% Dim x\nx = \"%>\" ' comment\n%>plain</p><%-- ignored --%>";
        let result = styled(key, text);
        assert_eq!(style(&result, text.find("<%").expect("open")), 15, "{key}");
        assert_eq!(
            style(&result, text.find("Dim").expect("keyword")),
            84,
            "{key}"
        );
        assert_eq!(
            style(&result, text.find("\"%>\"").expect("string")),
            85,
            "{key}"
        );
        assert_eq!(
            style(&result, text.find("' comment").expect("comment")),
            82,
            "{key}"
        );
        assert_eq!(
            style(&result, text.find("plain").expect("plain")),
            0,
            "{key}"
        );
        assert_eq!(
            style(&result, text.find("<%--").expect("jsp comment")),
            20,
            "{key}"
        );
    }
    let text = "<%@ language=\"JavaScript\" %><% const x = 2; %>";
    let result = styled("asp", text);
    assert_eq!(style(&result, 0), 16);
    assert_eq!(style(&result, text.find("const").expect("js")), 62);
}

#[test]
fn html_javascript_and_css_embedded_contexts() {
    let text = "<script>\nconst x = /[a-z]+/g;\nlet s = `value`; // comment\nif (x) {\n x = 2;\n}\n</script><style>a:hover { color: red; }</style>tail";
    let result = styled("html", text);
    for (needle, expected) in [
        ("const", 47),
        ("/[a-z]+/g", 52),
        ("`value`", 53),
        ("// comment", 43),
        ("color", 6),
        ("red", 8),
        ("tail", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(!result.folds.is_empty());
}

#[test]
fn css_selectors_values_unknown_properties_nested_groups_and_scss() {
    let text = "@media screen {\na:hover::before, .box#main[data-x] { color: #fff; nonsense: 12px !important; }\n}";
    let result = styled("css", text);
    for (needle, expected) in [
        ("media", 22),
        ("a:hover", 1),
        ("hover", 3),
        ("before", 18),
        ("box", 2),
        ("main", 10),
        ("data-x", 16),
        ("color", 6),
        ("#fff", 8),
        ("nonsense", 7),
        ("important", 11),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    let mut p = profile("css");
    p.properties
        .insert("lexer.css.scss.language".into(), "1".into());
    let result = markup::lex("$x: 1; // comment\na { color: $x; }", &p).expect("lexing");
    assert_eq!(style(&result, 0), 23);
    assert_eq!(style(&result, 7), 9);
}

#[test]
fn yaml_nested_flow_quotes_scalar_and_errors() {
    let text = "---\nroot:\n  enabled: true\n  number: -1.5\n  refs: &anchor\n  flow: {key: false, other: 2}\n  quoted: \"# not comment\" # comment\n  body: |-\n    true # literal\n\tinvalid: x\nplain text";
    let result = styled("yaml", text);
    for (needle, expected) in [
        ("---", 6),
        ("root", 2),
        ("true", 3),
        ("-1.5", 4),
        ("&anchor", 5),
        ("key", 2),
        ("false", 3),
        ("# not comment", 0),
        ("# comment", 1),
        ("true # literal", 7),
        ("\tinvalid", 8),
        ("plain text", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(result.fold_levels[1].header);
}

#[test]
fn toml_multiline_nested_tables_datetime_and_string_errors() {
    let text = "[package]\nname = \"abc\\n\"\nactive = true\nnums = [1, 0xff, { nested = false }]\nwhen = 2026-11-10T12:30:00Z\ntext = \"\"\"first\n# literal\nlast\"\"\"\nliteral = 'no\\escape'\nbad = \"unterminated\n";
    let result = styled("toml", text);
    for (needle, expected) in [
        ("[package]", 5),
        ("name", 6),
        ("\"abc", 10),
        ("\\n", 13),
        ("true", 3),
        ("0xff", 4),
        ("nested", 6),
        ("false", 3),
        ("2026", 14),
        ("# literal", 12),
        ("'no", 9),
        ("\"unterminated", 15),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(!result.folds.is_empty());
}

#[test]
fn sql_dialect_properties_and_quoted_literals() {
    let text = "BEGIN\nSELECT 'a''b', \"text\", q'[quoted ' text]', int, 1.2e-3 FROM table_name;\n/* comment\n continuation */\nEND";
    let result = styled("sql", text);
    for (needle, expected) in [
        ("SELECT", 5),
        ("'a''b'", 7),
        ("\"text\"", 6),
        ("q'[", 24),
        ("1.2e-3", 4),
        ("table_name", 11),
        ("/* comment", 1),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    let mut p = profile("sql");
    p.properties
        .insert("lexer.sql.numbersign.comment".into(), "1".into());
    p.properties
        .insert("lexer.sql.backticks.identifier".into(), "1".into());
    let result = markup::lex("`column` # comment", &p).expect("lexing");
    assert_eq!(style(&result, 0), 23);
    assert_eq!(style(&result, 9), 15);
    assert!(!styled("sql", text).folds.is_empty());
}

#[test]
fn mssql_nesting_variables_and_source_keyword_classes() {
    let text = "SELECT [column], \"alias\", @local, @@error, 'a''b', 42, bigint, abs(x), sp_help\n/* outer\n/* nested */ still comment */\nidentifier";
    let result = styled("mssql", text);
    for (needle, expected) in [
        ("SELECT", 9),
        ("[column]", 16),
        ("\"alias\"", 8),
        ("@local", 7),
        ("@@error", 12),
        ("'a''b'", 4),
        ("42", 3),
        ("bigint", 10),
        ("abs", 13),
        ("sp_help", 14),
        ("still comment", 1),
        ("identifier", 6),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
}

#[test]
fn diff_line_formats_and_folding() {
    let text = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n context\n++nested add\n+-nested delete\n-+removed add\n--removed delete\n!changed\nplain";
    let result = styled("diff", text);
    for (needle, expected) in [
        ("diff", 2),
        ("---", 3),
        ("+++", 3),
        ("@@", 4),
        ("-old", 5),
        ("+new", 6),
        (" context", 0),
        ("++nested", 8),
        ("+-nested", 9),
        ("-+removed", 10),
        ("--removed", 11),
        ("!changed", 7),
        ("plain", 1),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(!result.folds.is_empty());
}

#[test]
fn errorlist_diagnostic_formats_and_negative_plain_output() {
    let text = "main.c:3:7: error: bad\nmain.cpp(12): error C1\n  File \"main.py\", line 8\n at a.Test.main(Test.java:3)\nscript.lua:4: bad\nplain successful output";
    let result = styled("errorlist", text);
    for (needle, expected) in [
        ("main.c:", 2),
        ("main.cpp", 3),
        ("  File", 1),
        (" at a", 20),
        ("script.lua", 8),
        ("plain", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    let mut p = profile("errorlist");
    p.properties
        .insert("lexer.errorlist.value.separate".into(), "1".into());
    let result = markup::lex("main.c:3: error: bad", &p).expect("lexing");
    assert_eq!(style(&result, 11), 21);
}

#[test]
fn escape_sequences_source_style_palette_and_reset() {
    let text = "plain \x1b[31mred\x1b[0m normal\n\x1b[1;32;40mbright\x1b[2Junknown\n\x1b[31";
    let result = styled("escseq", text);
    for (needle, expected) in [
        ("plain", 0),
        ("\x1b[31m", 171),
        ("red", 2),
        ("normal", 0),
        ("bright", 102),
        ("\x1b[2J", 172),
        ("\x1b[31", 171),
    ] {
        let at = if needle == "\x1b[31" {
            text.rfind(needle).expect(needle)
        } else {
            text.find(needle).expect(needle)
        };
        assert_eq!(
            style(&result, at),
            if needle == "\x1b[31" { 172 } else { expected },
            "{needle}"
        );
    }
    let mut p = profile("escseq");
    p.properties
        .insert("lexer.escseq.colour.text".into(), "0".into());
    let result = markup::lex("\x1b[31mred", &p).expect("lexing");
    assert_eq!(style(&result, 5), 0);
}

#[test]
fn registry_keys_values_types_guid_escape_and_continuation() {
    let text = "Windows Registry Editor Version 5.00\n; comment\n[HKEY_LOCAL_MACHINE\\Test]\n\"name\"=\"a\\\\b %1\"\n\"bytes\"=hex:01,ff,\\\n  02,03\n[-HKEY_LOCAL_MACHINE\\Removed]\n\"guid\"=\"{12345678-1234-1234-1234-123456789abc}\"";
    let result = styled("registry", text);
    for (needle, expected) in [
        ("Windows", 0),
        ("; comment", 1),
        ("[HKEY", 6),
        ("\"name\"", 2),
        ("a\\\\", 3),
        ("\\\\b", 8),
        ("%1", 11),
        ("hex", 5),
        ("01", 4),
        ("02,03", 4),
        ("[-HKEY", 7),
        ("{12345678", 10),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
}

#[test]
fn ihex_field_types_checksum_errors_and_extended_address_folds() {
    let text = ":020000040001F9\n:0400000001020304F2\n:00000001FF\n:00000001FE\nplain";
    let result = styled("ihex", text);
    assert_eq!(style(&result, 0), 1);
    assert_eq!(style(&result, 1), 4);
    assert_eq!(style(&result, 3), 6);
    assert_eq!(style(&result, 7), 2);
    assert_eq!(style(&result, 9), 11);
    assert_eq!(style(&result, 13), 16);
    assert_eq!(style(&result, text.find("01020304").expect("data")), 12);
    assert_eq!(style(&result, text.find("01020304").expect("data") + 2), 13);
    assert_eq!(style(&result, text.find("FE").expect("bad checksum")), 17);
    assert_eq!(style(&result, text.find("plain").expect("plain")), 0);
    assert!(!result.folds.is_empty());
}

#[test]
fn srec_and_tehex_record_layouts_and_invalid_fields() {
    let text = "S1137AF000020008000826290018538123410018B9\nS9030000FC\nS9030000FD\nplain";
    let result = styled("srec", text);
    for (offset, expected) in [(0, 1), (1, 2), (2, 4), (4, 7), (8, 12), (10, 13), (40, 16)] {
        assert_eq!(style(&result, offset), expected, "offset {offset}");
    }
    assert_eq!(style(&result, text.find("FC").expect("checksum")), 16);
    assert_eq!(style(&result, text.find("FD").expect("bad checksum")), 17);
    assert_eq!(style(&result, text.find("plain").expect("plain")), 0);
    let text = "%0E81E800000000\n%0E800800000000\nplain";
    let result = styled("tehex", text);
    for (offset, expected) in [(0, 1), (1, 4), (3, 2), (4, 16), (6, 9), (20, 17)] {
        assert_eq!(style(&result, offset), expected, "offset {offset}");
    }
    assert_eq!(style(&result, text.find("plain").expect("plain")), 0);
}

#[test]
fn nfo_is_source_literal_not_markup_or_terminal() {
    let text = "╔═══╗\n║ art ║ <tag> ;comment $var\n\x1b[31mplain\n";
    let result = styled("nfo", text);
    assert!(result.spans.iter().all(|span| span.style == 0));
    assert!(result.folds.is_empty());
}

#[test]
fn latex_math_environments_commands_verbatim_and_comments() {
    let text = "\\section[short]{Title}\n\\begin{equation}\na^2 + b^2\n\\end{equation}\n$x+1$ \\(y+2\\)\n% comment\n\\verb|$literal%|\n\\begin{verbatim}\n\\notcommand %literal\n\\end{verbatim}\nplain";
    let result = styled("latex", text);
    for (needle, expected) in [
        ("\\section", 1),
        ("[short]", 11),
        ("{equation}", 2),
        ("a^2", 6),
        ("x+1", 3),
        ("y+2", 3),
        ("% comment", 4),
        ("$literal", 8),
        ("\\notcommand", 8),
        ("plain", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(!result.folds.is_empty());
}

#[test]
fn tex_source_command_group_comment_and_interface_property() {
    let text = "\\def\\foo{Hello $x_1$} % ignored \\command\nplain";
    let result = styled("tex", text);
    for (needle, expected) in [
        ("\\def", 4),
        ("{", 2),
        ("Hello", 5),
        ("$", 2),
        ("_", 3),
        ("% ignored", 3),
        ("ignored", 0),
        ("plain", 5),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    let mut p = profile("tex");
    p.numbered_keywords[0] = "known".into();
    let result = markup::lex("\\known \\unknown", &p).expect("lexing");
    assert_eq!(style(&result, 0), 4);
    assert_eq!(style(&result, 7), 5);
}

#[test]
fn txt2tags_headers_inline_styles_options_and_multiline_code() {
    let text = "== Title ==\n**bold** //em// __under__ --strike-- ``code`` [link url]\n- item\n%!preproc x\n% comment\n```\n**literal**\n```\nplain";
    let result = styled("txt2tags", text);
    for (needle, expected) in [
        ("== Title", 7),
        ("**bold", 2),
        ("//em", 4),
        ("__under", 5),
        ("--strike", 16),
        ("``code", 20),
        ("[link", 18),
        ("- item", 13),
        ("%!preproc", 24),
        ("% comment", 22),
        ("**literal", 21),
        ("plain", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
}

#[test]
fn postscript_nested_strings_radix_names_dsc_hex_and_procedures() {
    let text = "%%Title: Example\n/name //immediate 16#ff 2.5 moveto\n{\n(a (nested) \\) text) <0a ff> <gg> <~87cURD]j7BEbo80~>\n}\nunknown";
    let result = styled("postscript", text);
    for (needle, expected) in [
        ("%%Title:", 2),
        ("Example", 3),
        ("/name", 7),
        ("//immediate", 8),
        ("16#ff", 4),
        ("2.5", 4),
        ("moveto", 6),
        ("{", 11),
        ("nested", 12),
        ("<0a", 13),
        ("gg", 15),
        ("<~", 14),
        ("unknown", 5),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(!result.folds.is_empty());
}

#[test]
fn search_result_source_headers_numbers_and_malformed_lines() {
    let text = "Search \"needle\" (2 hits)\n  /tmp/file.txt (2 hits)\n\tLine 12: needle\n\tmalformed result\n";
    let result = styled("searchResult", text);
    for (needle, expected) in [
        ("Search", 1),
        ("  /tmp", 2),
        ("12", 3),
        ("needle\n", 0),
        ("malformed", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    assert!(result.fold_levels[0].header);
    assert!(result.fold_levels[1].header);
}

#[test]
fn search_result_explicit_markings_are_safe_ranges_not_pointers() {
    let text = "Search needle\n  file.txt\n\tLine 1: a needle b\n";
    let start = text.rfind("needle").expect("match");
    let result = markup::search_results(
        text,
        &profile("searchResult"),
        std::slice::from_ref(&(start..start + 6)),
    )
    .expect("marked results");
    assert_eq!(style(&result, start), 4);
    assert_eq!(style(&result, start - 1), 0);
    assert!(
        markup::search_results(
            text,
            &profile("searchResult"),
            std::slice::from_ref(&(0..text.len() + 1)),
        )
        .is_err()
    );
}

#[test]
fn all_assigned_languages_are_utf8_safe_and_terminate_on_partial_input() {
    let keys = [
        "html",
        "xml",
        "asp",
        "php",
        "jsp",
        "css",
        "ini",
        "props",
        "yaml",
        "toml",
        "diff",
        "errorlist",
        "escseq",
        "registry",
        "ihex",
        "srec",
        "tehex",
        "nfo",
        "latex",
        "tex",
        "txt2tags",
        "postscript",
        "sql",
        "mssql",
        "searchResult",
    ];
    let samples = [
        "",
        "plain 😀 café 中文",
        "\\é \\😀",
        "<a x='é <?php echo \"😀\" ?>'>",
        "<?php $x = <<<'END'\n😀\nEND; ?>",
        "[a]\nkey = \"\"\"😀\ntext\"\"\"",
        "\x1b[31m😀\n\x1b[",
        "S1\n:0\n%0\n",
        "\\begin{verbatim}\n😀\n\\end{verbatim}",
        "a: [true, {nested: '😀#'}]\r\n",
    ];
    for key in keys {
        let p = profile(key);
        for sample in samples {
            for end in (0..=sample.len()).filter(|n| sample.is_char_boundary(*n)) {
                let text = &sample[..end];
                let result = lexers::lex(text, &p).expect(key);
                let mut covered = 0;
                for span in result.spans {
                    assert_eq!(span.range.start, covered, "{key}: {text:?}");
                    assert!(
                        text.is_char_boundary(span.range.start)
                            && text.is_char_boundary(span.range.end),
                        "{key}: {text:?}"
                    );
                    assert!(
                        span.range.end > span.range.start && span.range.end <= text.len(),
                        "{key}: {text:?}"
                    );
                    covered = span.range.end;
                }
                assert_eq!(covered, text.len(), "{key}: {text:?}");
            }
        }
    }
}

#[test]
fn multiline_states_and_fold_properties_use_framework_contract() {
    let text = "<div>\n<?php /* first\nsecond */ echo 1; ?>\n</div>\n";
    let result = styled("html", text);
    assert_eq!(result.fold_levels[0].level, 0x400);
    assert!(
        result
            .line_states
            .iter()
            .any(|line| line.state & 0xffff == 124)
    );
    let mut p = profile("html");
    p.properties.insert("fold".into(), "0".into());
    let result = markup::lex(text, &p).expect("lexing");
    assert!(result.folds.is_empty());
    assert!(
        result
            .fold_levels
            .iter()
            .all(|line| !line.header && line.level == 0x400)
    );
    p.properties.insert("fold".into(), "1".into());
    p.properties.insert("fold.html".into(), "0".into());
    let result = markup::lex("<div>\ntext\n</div>", &p).expect("lexing");
    assert!(result.folds.is_empty());
}

#[test]
fn errorlist_retains_less_common_source_diagnostic_states() {
    let cases = [
        ("cf90-123 error", 18),
        ("fortcom: Error: file.f, line 1", 17),
        ("Error 1 at (2:file.f) : problem", 16),
        ("Error E1 file.cpp 3: bad", 5),
        ("bad at line 12 file example.lua", 8),
        ("bad at example.pl line 2", 6),
        ("   at Method() in file.cs:line 5", 7),
        ("Line 2, file test.f", 15),
        ("line 3 column 5 - Warning: bad", 19),
        ("\tat Foo.method(Foo.java:42)", 20),
        ("In file included from file.h:2:", 22),
        ("NMAKE : fatal error U1077", 3),
        ("test.obj : error LNK2001: symbol", 3),
        ("script.sh: line 12: bad", 26),
        ("   73 |   expression", 25),
        ("symbol\tfile.c\t/^symbol$/", 9),
        ("< old diff line", 12),
        ("http://server:8080/path", 0),
        ("clock 12:34", 0),
    ];
    for (text, expected) in cases {
        assert_eq!(style(&styled("errorlist", text), 0), expected, "{text}");
    }
}

#[test]
fn toml_table_comments_escaped_triples_local_time_and_nested_folds() {
    let text = "[a] # comment\nx = 1\n[a.b]\nclock = 12:30:45\ntext = \"\"\"escaped \\\"\"\" still string\nlast\"\"\"\n[a.b.c]\nx = false\n";
    let result = styled("toml", text);
    assert_eq!(style(&result, text.find("# comment").expect("comment")), 1);
    assert_eq!(style(&result, text.find("12:30:45").expect("time")), 14);
    assert_eq!(
        style(&result, text.find("still string").expect("triple")),
        12
    );
    assert_eq!(result.fold_levels[2].level, 0x401);
    assert_eq!(result.fold_levels[6].level, 0x402);
}

#[test]
fn ansi_unknown_sequence_resets_text_style_and_erase_preserves_it() {
    let text = "\x1b[31mred\x1b[Kstill red\x1b[2Jplain";
    let result = styled("escseq", text);
    assert_eq!(style(&result, text.find("still red").expect("red")), 2);
    assert_eq!(style(&result, text.find("plain").expect("plain")), 0);
}

#[test]
fn css_source_punctuation_attribute_strings_and_scss_nested_selectors() {
    let text = "@media screen { .box:hover[data-x=\"a]b\"] { color: red !important; } }";
    let result = styled("css", text);
    for needle in ["@media", ".box", ":hover", "[data-x", "!important"] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            5,
            "{needle}"
        );
    }
    assert_eq!(
        style(&result, text.find("\"a]b\"").expect("attribute string")),
        13
    );
    let mut p = profile("css");
    p.properties
        .insert("lexer.css.scss.language".into(), "1".into());
    let text = "a {\n .nested:hover { color: red; }\n width: calc(2 * 1px);\n}";
    let result = markup::lex(text, &p).expect("lexing");
    assert_eq!(style(&result, text.find("nested").expect("nested")), 2);
    assert_eq!(style(&result, text.find("width").expect("property")), 6);
}

#[test]
fn embedded_python_respects_asp_language_property_and_multiline_strings() {
    let mut p = profile("asp");
    p.properties
        .insert("asp.default.language".into(), "3".into());
    p.numbered_keywords[3] = "def class return".into();
    let text = "<% def name():\n return \"\"\"%>\ntext\"\"\" # comment\n%>plain";
    let result = markup::lex(text, &p).expect("lexing");
    for (needle, expected) in [
        ("def", 111),
        ("name", 115),
        ("return", 111),
        ("text", 113),
        ("# comment", 107),
        ("plain", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
}

#[test]
fn props_source_values_keep_variables_and_continued_text_literal() {
    let text = "[vars]\nkey=$(value)\\\n continued value\n@=default\nunknown text";
    let result = styled("props", text);
    for (needle, expected) in [
        ("[vars]", 2),
        ("key", 5),
        ("=", 3),
        ("$(value)", 0),
        ("continued", 0),
        ("@", 4),
        ("default", 0),
        ("unknown", 0),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
}

#[test]
fn hex_source_required_field_sizes_unknown_types_and_garbage() {
    let result = styled("ihex", ":0100000401FA");
    assert_eq!(style(&result, 1), 5);
    let result = styled("ihex", ":00000001FFtrailing");
    assert_eq!(style(&result, 11), 18);
    let result = styled("ihex", ":00000006FA");
    assert_eq!(style(&result, 7), 3);
    assert_eq!(style(&result, 3), 10);
    let result = styled("srec", "S504000001FA");
    assert_eq!(style(&result, 2), 5);
    let result = styled("srec", "S4030000FC");
    assert_eq!(style(&result, 1), 3);
    let result = styled("tehex", "%0E91F800000000");
    assert_eq!(style(&result, 3), 3);
    assert_eq!(style(&result, 6), 10);
}

#[test]
fn sql_source_doc_keyword_classes_sqlplus_and_signed_numbers() {
    let mut p = profile("sql");
    p.numbered_keywords[3] = "param return".into();
    p.numbered_keywords[2] = "rem~ark pro~mpt".into();
    let text = "/** @param value @unknown value */\nSELECT -12, +3.5, 123abc;\nREM a comment\nPROMPT output";
    let result = markup::lex(text, &p).expect("lexing");
    for (needle, expected) in [
        ("/**", 3),
        ("@param", 17),
        ("@unknown", 18),
        ("-12", 4),
        ("+3.5", 4),
        ("abc", 11),
        ("REM", 8),
        ("a comment", 13),
        ("PROMPT", 8),
        ("output", 9),
    ] {
        assert_eq!(
            style(&result, text.find(needle).expect(needle)),
            expected,
            "{needle}"
        );
    }
    let result = styled("sql", "--! ordinary line comment");
    assert_eq!(style(&result, 0), 2);
}

#[test]
fn mssql_source_folding_uses_indentation() {
    let result = styled("mssql", "BEGIN\n  SELECT 1;\nEND");
    assert!(result.fold_levels[0].header);
    assert_eq!(result.fold_levels[1].level, 0x402);
    assert_eq!(result.fold_levels[2].level, 0x400);
}
