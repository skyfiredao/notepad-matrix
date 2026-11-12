use notepad_matrix::languages::{LanguageDefinition, catalog};
use notepad_matrix::lexers::lex;

fn profile(key: &str) -> LanguageDefinition {
    catalog()
        .expect("catalog")
        .by_name(key)
        .expect("script profile")
        .clone()
}

fn styles(key: &str, text: &str) -> Vec<u16> {
    let result = lex(text, &profile(key)).expect("lex script");
    let mut styles = vec![0; text.len()];
    for span in result.spans {
        styles[span.range].fill(span.style);
    }
    styles
}

fn token(key: &str, text: &str, needle: &str, style: u16) {
    let start = text.find(needle).expect("test token");
    assert_eq!(
        &styles(key, text)[start..start + needle.len()],
        vec![style; needle.len()],
        "{key}: {needle:?}"
    );
}

#[test]
fn python_raw_triples_and_names() {
    let text = "class Café:\r\n    def run(self):\r\n        return r'''raw\\n\r\ntext'''\r\n# def fake\r\n";
    token("python", text, "class", 5);
    token("python", text, "Café", 8);
    token("python", text, "run", 9);
    token("python", text, "r'''raw\\n\r\ntext'''", 6);
    token("python", text, "# def fake", 1);
    token("python", "classify = 1", "classify", 11);
}

#[test]
fn gdscript_annotations_and_multiline_strings() {
    let text = "@export var speed = 4\nfunc move():\n    var s = \"\"\"a\nb\"\"\"\n# func hidden\n";
    token("gdscript", text, "@export", 15);
    token("gdscript", text, "func", 5);
    token("gdscript", text, "move", 9);
    token("gdscript", text, "\"\"\"a\nb\"\"\"", 7);
    token("gdscript", "functional = 'if'", "functional", 11);
}

#[test]
fn bash_quoted_heredoc_and_expansions() {
    let text =
        "cat <<-'END'\r\n\t$HOME # literal\r\n\tEND\r\necho ${x:-${y}} $1 \"$HOME\" 'if'\r\n";
    token("bash", text, "'END'", 12);
    token("bash", text, "$HOME # literal", 13);
    token("bash", text, "${x:-${y}}", 10);
    token("bash", text, "$1", 9);
    token("bash", text, "\"$HOME\"", 5);
    token("bash", text, "'if'", 6);
    token("bash", "echo foo#bar", "foo#bar", 8);
}

#[test]
fn batch_variables_labels_and_echo_context() {
    let text = "@echo off\r\n:again comment\r\nset x=%PATH%\r\necho if !x! %%a %~dp0\r\nrem echo %bad%\r\n";
    token("batch", text, "@", 4);
    token("batch", text, ":again", 3);
    token("batch", text, "%PATH%", 6);
    token("batch", text, "if", 0);
    token("batch", text, "!x!", 6);
    token("batch", text, "%%a", 6);
    token("batch", text, "%~dp0", 6);
    token("batch", text, "rem echo %bad%", 1);
}

#[test]
fn perl_quote_operators_regex_division_pod_and_data() {
    let text = "my $x = qq{a $name};\nmy $n = 8 / 2;\n$x =~ s{a}{b}g;\n=pod\ndoc\n=cut\n__DATA__\nmy raw\n";
    token("perl", text, "my", 5);
    token("perl", text, "$x", 12);
    token("perl", text, "$name", 64);
    token("perl", text, "/", 10);
    token("perl", text, "s{a}{b}g", 18);
    token("perl", text, "=pod\ndoc\n=cut", 3);
    token("perl", text, "my raw", 21);
}

#[test]
fn perl_heredocs_preserve_quote_mode() {
    let text = "print <<'END';\n$name\nEND\nprint <<\"NEXT\";\n$name\nNEXT\n";
    token("perl", text, "<<'END'", 22);
    token("perl", text, "$name", 23);
    let start = text.rfind("$name").expect("second interpolation");
    assert_eq!(&styles("perl", text)[start..start + 5], &[61; 5]);
}

#[test]
fn raku_types_sigils_nested_comments_and_q_language() {
    let text = "class Widget { my Int $value = 42; say qq{hi $value}; }\n#`[outer [inner]\nend]\nmy $x = 8 / 2;\n";
    token("raku", text, "class", 19);
    token("raku", text, "Widget", 28);
    token("raku", text, "Int", 22);
    token("raku", text, "$value", 23);
    token("raku", text, "hi ", 10);
    token("raku", text, "outer [inner]\nend]", 3);
    token("raku", text, "/", 18);
}

#[test]
fn ruby_percent_literals_interpolation_and_division() {
    let text = "class Widget\n  def call\n    s = %q{a {b} # no}\n    t = \"hi #{name}\"\n    n = 8 / 2\n    r = /[a/]+/i\n  end\nend\n";
    token("ruby", text, "Widget", 8);
    token("ruby", text, "call", 9);
    token("ruby", text, "%q{a {b} # no}", 24);
    token("ruby", text, "name", 11);
    token("ruby", text, "/", 10);
    token("ruby", text, "/[a/]+/i", 12);
}

#[test]
fn ruby_heredoc_and_documentation() {
    let text = "puts <<~'END'\n  #{literal}\n  END\n=begin\ndoc\n=end\n__END__\nraw\n";
    token("ruby", text, "<<~'END'", 20);
    token("ruby", text, "#{literal}", 21);
    token("ruby", text, "=begin\ndoc\n=end", 3);
    token("ruby", text, "raw", 19);
}

#[test]
fn powershell_here_strings_and_documentation() {
    let text = "<#\n.SYNOPSIS\nhelp\n#>\n$x = @\"\n$notStyledSeparately\n\"@\nGet-Item 'if'\n";
    token("powershell", text, ".SYNOPSIS", 16);
    token("powershell", text, "help", 13);
    token("powershell", text, "$x", 5);
    token("powershell", text, "$notStyledSeparately", 14);
    token("powershell", text, "Get-Item", 9);
    token("powershell", text, "'if'", 3);
    token("powershell", "identifier = 1", "identifier", 7);
}

#[test]
fn tcl_command_position_and_substitutions() {
    let text = "set x $name\nputs ${a b}; # inline\nset y \"$x [expr 1]\"\nputs set\n";
    token("tcl", text, "set", 12);
    token("tcl", text, "$name", 8);
    token("tcl", text, "a b", 9);
    token("tcl", text, "# inline", 1);
    let last = text.rfind("set").expect("argument");
    assert_eq!(&styles("tcl", text)[last..last + 3], &[7; 3]);
}

#[test]
fn lua_long_delimiters_labels_and_unterminated_strings() {
    let text = "local x = [==[a ]=] b\nend]==]\n--[=[comment\nend]=]\n::again::\nlocal s = 'bad\nreturn 1\n";
    token("lua", text, "local", 5);
    token("lua", text, "[==[a ]=] b\nend]==]", 8);
    token("lua", text, "--[=[comment\nend]=]", 1);
    token("lua", text, "::again::", 20);
    token("lua", text, "'bad", 12);
    token("lua", text, "return", 5);
    token("lua", "localize = \"end\"", "localize", 11);
}

#[test]
fn autoit_comments_macros_sendkeys_and_com_members() {
    let text = "#cs\nIf $hidden Then\n#ce\nLocal $x = @AppDataDir\nSend(\"{ENTER}\")\n$x.member\n";
    token("autoit", text, "If $hidden Then", 2);
    token("autoit", text, "Local", 5);
    token("autoit", text, "$x", 9);
    token("autoit", text, "@AppDataDir", 6);
    token("autoit", text, "{ENTER}", 10);
    token("autoit", text, "member", 14);
    token("autoit", "Ifish()", "Ifish", 0);
}

#[test]
fn coffee_triples_verbose_regex_and_properties() {
    let text = "class Thing\n  s = '''raw\n#{literal}'''\n  @value = /// a # note\n b ///gi\n  n = 6 / 2\n";
    token("coffeescript", text, "class", 5);
    token("coffeescript", text, "'''raw\n#{literal}'''", 7);
    token("coffeescript", text, "@value", 25);
    token("coffeescript", text, "# note", 24);
    token("coffeescript", "classy = 1", "classy", 11);
}

#[test]
fn makefile_targets_assignments_and_nested_variables() {
    let text = "target: dep\nNAME := $(outer $(inner))\n\techo # not comment\nBAD = $(unfinished\n# comment\n";
    token("makefile", text, "target", 5);
    token("makefile", text, "NAME", 3);
    token("makefile", text, ":=", 4);
    token("makefile", text, "$(outer $(inner))", 3);
    token("makefile", text, "# not comment", 0);
    token("makefile", text, "$(unfinished", 9);
    token("makefile", text, "# comment", 1);
}

#[test]
fn cmake_commands_variables_and_special_blocks() {
    let text = "IF(ON)\nmessage(\"${name} @other@\")\nset(x ${value})\nENDIF()\n# set\n";
    token("cmake", text, "IF", 11);
    token("cmake", text, "message", 5);
    token("cmake", text, "${name}", 13);
    token("cmake", text, "@other@", 2);
    token("cmake", text, "${value}", 7);
    token("cmake", text, "# set", 1);
    token("cmake", "setish(x)", "setish", 0);
}

#[test]
fn inno_sections_pascal_and_messages() {
    let text = "[Setup]\nAppName=Demo\nSource: {app}\n[Code]\nbegin\n{ comment\nAppName }\nend;\n[Messages]\nWelcome=\"literal\"\n";
    token("inno", text, "[Setup]", 4);
    token("inno", text, "AppName", 2);
    token("inno", text, "Source", 3);
    token("inno", text, "{app}", 6);
    token("inno", text, "begin", 8);
    token("inno", text, "{ comment\nAppName }", 7);
    token("inno", text, "\"literal\"", 0);
    token("inno", "[Unknown]\nAppName", "[Unknown]", 0);
}

#[test]
fn kix_macros_comments_and_multiline_quotes() {
    let text = "IF $x\n @USERID @unknown\n /* if\n end */\n \"multi\nline\"\nENDIF\n";
    token("kix", text, "IF", 7);
    token("kix", text, "$x", 5);
    token("kix", text, "@USERID", 6);
    token("kix", text, "@unknown", 0);
    token("kix", text, "/* if\n end */", 10);
    token("kix", text, "\"multi\nline\"", 2);
    token("kix", "ifish", "ifish", 31);
}

#[test]
fn nsis_case_sensitive_blocks_and_string_variables() {
    let text = "Section \"main\"\nStrCpy $0 \"${name} $INSTDIR\"\n/* multi\ncomment */\nSectionEnd\nsection\n";
    token("nsis", text, "Section", 9);
    token("nsis", text, "$0", 6);
    token("nsis", text, "${name}", 13);
    token("nsis", text, "$INSTDIR", 13);
    token("nsis", text, "/* multi\ncomment */", 18);
    token("nsis", text, "section", 0);
}

#[test]
fn nncrontab_task_environment_and_keyword_sets() {
    let text = "#( Task\nAction: Run\n* 12 \"%PATH%\"\n)#\n# comment\n";
    token("nncrontab", text, "#( Task", 2);
    token("nncrontab", text, "Action", 3);
    token("nncrontab", text, "*", 6);
    token("nncrontab", text, "12", 7);
    token("nncrontab", text, "%PATH%", 9);
    token("nncrontab", text, ")#", 2);
    token("nncrontab", "ordinary", "ordinary", 0);
}

#[test]
fn pascal_directives_strings_and_assembly() {
    let text = "{$IFDEF WIN}\nbegin\ns := 'it''s'; c := #$41; n := $FF;\nasm\n mov eax, 1\nend;\nend.\n{$ENDIF}\n";
    token("pascal", text, "{$IFDEF WIN}", 5);
    token("pascal", text, "begin", 9);
    token("pascal", text, "'it''s'", 10);
    token("pascal", text, "#$41", 12);
    token("pascal", text, "$FF", 8);
    token("pascal", text, "mov", 14);
    token("pascal", "beginning", "beginning", 1);
}

#[test]
fn rebol_preface_nested_braces_and_typed_literals() {
    let text = "preface\nREBOL []\ncomment {outer {inner}\nend}\n10x20 1.2.3 12:30 1-Jan-2026 #{ABCD} %file user@example.org https://example.org\n";
    token("rebol", text, "preface\n", 3);
    token("rebol", text, "{outer {inner}\nend}", 2);
    token("rebol", text, "10x20", 9);
    token("rebol", text, "1.2.3", 10);
    token("rebol", text, "12:30", 19);
    token("rebol", text, "1-Jan-2026", 18);
    token("rebol", text, "#{ABCD}", 11);
    token("rebol", text, "%file", 15);
    token("rebol", text, "user@example.org", 16);
    token("rebol", text, "https://example.org", 17);
    token("rebol", "REBOL [] ordinary", "ordinary", 20);
}

#[test]
fn property_variants_change_only_source_controlled_behavior() {
    let mut p = profile("bash");
    p.properties
        .insert("lexer.bash.styling.inside.string".into(), "1".into());
    let text = "echo \"$name ${x}\"";
    let r = lex(text, &p).expect("bash property");
    assert!(
        r.spans
            .iter()
            .any(|span| span.style == 9 && &text[span.range.clone()] == "$name")
    );
    let mut p = profile("python");
    p.properties
        .insert("lexer.python.strings.b".into(), "0".into());
    let r = lex("b'bytes'", &p).expect("python property");
    assert_eq!(r.spans[0].style, 11);
    let mut p = profile("nsis");
    p.properties.insert("nsis.ignorecase".into(), "1".into());
    p.properties.insert("nsis.uservars".into(), "1".into());
    let r = lex("section $custom\nsectionend", &p).expect("nsis properties");
    assert_eq!(r.spans[0].style, 9);
    assert!(r.spans.iter().any(|span| span.style == 6));
    let mut p = profile("pascal");
    p.numbered_keywords[0].push_str(" read index name");
    let r = lex("read index name", &p).expect("smart pascal");
    assert!(r.spans.iter().all(|span| matches!(span.style, 0 | 1)));
    p.properties
        .insert("lexer.pascal.smart.highlighting".into(), "0".into());
    let r = lex("read index name", &p).expect("simple pascal");
    assert_eq!(r.spans[0].style, 9);
}

#[test]
fn folding_and_unicode_boundaries_for_every_script() {
    let keys = [
        "python",
        "gdscript",
        "bash",
        "batch",
        "perl",
        "raku",
        "ruby",
        "powershell",
        "tcl",
        "lua",
        "autoit",
        "coffeescript",
        "makefile",
        "cmake",
        "inno",
        "kix",
        "nsis",
        "nncrontab",
        "pascal",
        "rebol",
    ];
    let text = "é漢 = '💡'\r\n# note\rline\n";
    for key in keys {
        let r = lex(text, &profile(key)).expect(key);
        assert_eq!(r.fold_levels.len(), 4, "{key}");
        for span in &r.spans {
            assert!(
                text.is_char_boundary(span.range.start) && text.is_char_boundary(span.range.end),
                "{key}: {span:?}"
            );
        }
        assert_eq!(r.spans.first().map(|s| s.range.start), Some(0));
        assert_eq!(r.spans.last().map(|s| s.range.end), Some(text.len()));
        let mut p = profile(key);
        p.properties.insert("fold".into(), "0".into());
        assert!(
            lex(text, &p)
                .expect(key)
                .fold_levels
                .iter()
                .all(|l| !l.header && l.level == 0x400)
        );
    }
    for (key, text) in [
        ("python", "if True:\n    pass\n"),
        ("lua", "function x()\n return 1\nend\n"),
        ("cmake", "if(ON)\nset(x 1)\nendif()\n"),
        ("nsis", "Section\nDetailPrint hi\nSectionEnd\n"),
        ("pascal", "begin\nx := 1;\nend\n"),
    ] {
        assert!(
            lex(text, &profile(key)).expect(key).fold_levels[0].header,
            "{key}"
        );
    }
}

#[test]
fn gdscript_nodepaths_and_annotation_position() {
    let text = "$Parent/Child\nvar child = %Unique\n$\"Node with space\"/Child\nvar x = @export\n";
    token("gdscript", text, "$Parent/Child", 16);
    token("gdscript", text, "%Unique", 16);
    token("gdscript", text, "$\"Node with space\"/Child", 16);
    token("gdscript", text, "@", 10);
}

#[test]
fn python_fstrings_continue_after_nested_quoted_expressions() {
    let text = "f\"{data[\"key\"]} tail\"\nf'''{name!r:>10}\nend'''\n";
    token("python", text, "data", 11);
    token("python", text, "\"key\"", 3);
    token("python", text, "tail", 16);
    token("python", text, "end", 18);
}

#[test]
fn shell_substitution_properties_and_arithmetic_context() {
    let mut p = profile("bash");
    p.properties
        .insert("lexer.bash.command.substitution".into(), "1".into());
    let text = "$(echo $name)";
    let result = lex(text, &p).expect("command substitution");
    assert!(
        result
            .spans
            .iter()
            .any(|span| span.style == 4 && &text[span.range.clone()] == "echo")
    );
    token("bash", "echo $((2 + 3))", "2", 3);
    token("bash", "cat <<EOF\ntext\nEOF\n", "<<EOF", 12);
}

#[test]
fn raku_unicode_quotes_and_heredoc_modes() {
    token("raku", "say ‘raw $name’;", "‘raw $name’", 8);
    token("raku", "say “hi $name”;", "$name", 12);
    let text = "say q:to/END/;\n$name\nEND\nsay qq:to/NEXT/;\n$name\nNEXT\n";
    token("raku", text, "q:to/END/", 9);
    token("raku", text, "$name", 6);
    let last = text.rfind("$name").expect("interpolation");
    assert_eq!(&styles("raku", text)[last..last + 5], &[12; 5]);
}

#[test]
fn make_and_cron_have_no_source_folding() {
    for (key, text) in [
        ("makefile", "target:\n\techo hi\n"),
        ("nncrontab", "#( task\nAction:\n)#\n"),
        ("batch", "if exist x (\necho x\n)\n"),
        ("kix", "if $x\nendif\n"),
    ] {
        let r = lex(text, &profile(key)).expect(key);
        assert!(r.folds.is_empty(), "{key}");
        assert!(r.fold_levels.iter().all(|l| !l.header), "{key}");
    }
}

#[test]
fn every_script_handles_mixed_ordinary_unicode_without_panics() {
    let keys = [
        "python",
        "gdscript",
        "bash",
        "batch",
        "perl",
        "raku",
        "ruby",
        "powershell",
        "tcl",
        "lua",
        "autoit",
        "coffeescript",
        "makefile",
        "cmake",
        "inno",
        "kix",
        "nsis",
        "nncrontab",
        "pascal",
        "rebol",
    ];
    let parts = [
        "é", "💡", "漢", "\r\n", "\r", "\n", "a", "7", "'", "\"", "`", "\\", "$", "@", "%", "{",
        "}", "[", "]", "(", ")", "<", ">", "#", "=", ":", ";", " ", "\t", "/", "-", "\0", "‘", "“",
    ];
    let mut seed = 1347u32;
    for _ in 0..50 {
        let mut text = String::new();
        for _ in 0..100 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            text.push_str(parts[seed as usize % parts.len()]);
        }
        for key in keys {
            let result = lex(&text, &profile(key)).expect(key);
            let mut end = 0;
            for span in result.spans {
                assert_eq!(span.range.start, end, "{key}: {text:?}");
                assert!(
                    text.is_char_boundary(span.range.start)
                        && text.is_char_boundary(span.range.end),
                    "{key}"
                );
                end = span.range.end;
            }
            assert_eq!(end, text.len(), "{key}");
        }
    }
}

#[test]
fn ruby_percent_array_style_ids_follow_the_frozen_source() {
    for (text, style) in [
        ("%w[a b]", 41),
        ("%W[a b]", 28),
        ("%i[a b]", 42),
        ("%I[a b]", 43),
        ("%s[name]", 44),
    ] {
        token("ruby", text, text, style);
    }
}

#[test]
fn python_prefix_rules_soft_keywords_and_binary_property() {
    token("python", "rr'no'", "rr", 11);
    token("python", "ru'no'", "ru", 11);
    token("python", "ur'yes'", "ur'yes'", 4);
    token("python", "match = 1", "match", 11);
    token("python", "x.case", "case", 20);
    token("python", "match value:", "match", 5);
    let mut p = profile("python");
    p.properties
        .insert("lexer.python.literals.binary".into(), "0".into());
    let r = lex("0b101", &p).expect("binary property");
    assert_eq!(
        r.spans
            .iter()
            .map(|span| (span.range.clone(), span.style))
            .collect::<Vec<_>>(),
        vec![(0..1, 2), (1..5, 11)]
    );
}

#[test]
fn lua_z_escapes_skip_newlines_without_leaking_string_state() {
    let text = "local s = \"a\\z\r\n  b\"\nreturn 1\n";
    token("lua", text, "\"a\\z\r\n  b\"", 6);
    token("lua", text, "return", 5);
}

#[test]
fn multiline_states_and_quote_comment_folding_are_observable() {
    for (key, text) in [
        ("python", "x = '''first\nsecond\nlast'''\n"),
        ("lua", "x = [=[first\nsecond]=]\n"),
        ("powershell", "<#first\nsecond#>\n"),
        ("ruby", "x = %Q{first\nsecond}\n"),
        ("raku", "q{first\nsecond}\n"),
        ("pascal", "{first\nsecond}\n"),
    ] {
        let r = lex(text, &profile(key)).expect(key);
        assert_ne!(r.line_states[0].state, 0, "{key}");
        assert_eq!(r.line_states.last().expect("final line").state, 0, "{key}");
    }
    let text = "x = '''first\nsecond\nlast'''\n";
    assert!(
        lex(text, &profile("python"))
            .expect("quote fold")
            .fold_levels[0]
            .header
    );
    let text = "# first\n# second\necho last\n";
    assert!(
        lex(text, &profile("bash"))
            .expect("comment fold")
            .fold_levels[0]
            .header
    );
    let mut p = profile("bash");
    p.properties.insert("fold.comment".into(), "0".into());
    assert!(!lex(text, &p).expect("comment fold disabled").fold_levels[0].header);
}

#[test]
fn interpolation_braces_inside_inner_strings_do_not_close_expressions() {
    let text = "f'{mapping[\"}\"]} suffix'";
    token("python", text, "mapping", 11);
    token("python", text, "\"}\"", 3);
    token("python", text, "suffix", 17);
    let text = "\"#{lookup('}')} suffix\"";
    token("ruby", text, "lookup", 11);
    token("ruby", text, "'}'", 7);
    token("ruby", text, "suffix", 6);
    let text = "\"#{1 + true} suffix\"";
    token("coffeescript", text, "1", 4);
    token("coffeescript", text, "true", 19);
    token("coffeescript", text, "suffix", 6);
}

#[test]
fn source_string_continuation_rules_for_cmake_and_autoit() {
    let text = "message(\"open\nmessage(next)\n";
    token("cmake", text, "next", 0);
    let pos = text.rfind("message").expect("second line command");
    assert_eq!(&styles("cmake", text)[pos..pos + 7], &[5; 7]);
    let text = "Local $s = \"first _\r\nsecond\"\r\nLocal $n = 1\r\n";
    token("autoit", text, "second", 7);
    token("autoit", text, "$n", 9);
}

#[test]
fn bash_special_parameter_property_and_heredoc_end_styles() {
    token("bash", "echo $_name $123", "$_name", 9);
    token("bash", "echo $_name $123", "$123", 9);
    let text = "cat <<EOF\nbody\nEOF\n";
    let last = text.rfind("EOF").expect("heredoc close");
    assert_eq!(&styles("bash", text)[last..last + 3], &[12; 3]);
    let mut p = profile("bash");
    p.properties
        .insert("lexer.bash.special.parameter".into(), "*@#?-$!.".into());
    let r = lex("echo $.", &p).expect("custom shell parameter");
    assert!(
        r.spans
            .iter()
            .any(|span| span.style == 9 && span.range == (5..7))
    );
}

#[test]
fn raku_unicode_quote_delimiters_preserve_interpolation_boundaries() {
    token("raku", "qq«hi $name»", "$name", 12);
    token("raku", "q«raw $name»", "$name", 9);
    token("raku", "qq«hi $name»", "»", 10);
}

#[test]
fn batch_tcl_and_cmake_continued_lines_preserve_their_own_contexts() {
    let text = "echo ^\r\nif %x%\r\nif exist x echo ok\r\n";
    token("batch", text, "if", 0);
    token("batch", text, "%x%", 6);
    let next = text.rfind("if").expect("new command");
    assert_eq!(&styles("batch", text)[next..next + 2], &[2; 2]);
    let text = "# continued\\\r\nset hidden 1\r\nset visible 2\r\n";
    token("tcl", text, "set hidden 1", 2);
    let next = text.rfind("set").expect("new command");
    assert_eq!(&styles("tcl", text)[next..next + 3], &[12; 3]);
    let text = "# continued\\\r\nmessage(hidden)\r\nmessage(visible)\r\n";
    token("cmake", text, "message(hidden)", 1);
    let next = text.rfind("message").expect("new command");
    assert_eq!(&styles("cmake", text)[next..next + 7], &[5; 7]);
}
