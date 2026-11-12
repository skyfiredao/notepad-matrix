use notepad_matrix::languages;
use notepad_matrix::lexers;

fn profile(key: &str) -> &'static languages::LanguageDefinition {
    languages::catalog()
        .expect("catalog")
        .by_name(key)
        .expect("source profile")
}

fn styles(key: &str, text: &str) -> Vec<u16> {
    let result = lexers::lex(text, profile(key)).expect("dedicated lexer");
    let mut styles = vec![0; text.len()];
    for span in result.spans {
        styles[span.range].fill(span.style);
    }
    styles
}

#[test]
fn ada_attributes_labels_and_invalid_literals() {
    let text = "X'First; 'x' <<again>> 2#102# -- note\n";
    let got = styles("ada", text);
    assert_eq!(got[0], 2);
    assert_eq!(got[1], 4);
    assert_eq!(&got[9..12], &[5, 5, 5]);
    assert!(got[13..22].iter().all(|&s| s == 9));
    assert!(got[23..29].iter().all(|&s| s == 11));
    assert_eq!(got[text.find("--").expect("comment")], 10);
}

fn assert_tokens(key: &str, text: &str, expected: &[(&str, u16)]) {
    let got = styles(key, text);
    for &(token, style) in expected {
        let a = text.find(token).expect("fixture token");
        assert!(
            got[a..a + token.len()].iter().all(|&s| s == style),
            "{key} {token:?}: {:?}, expected {style}",
            &got[a..a + token.len()]
        );
    }
}

#[test]
fn functional_language_state_policies() {
    assert_tokens(
        "caml",
        "let x = `Tag (* outer (* inner *) outer *) 'a' 'type\n",
        &[
            ("let", 3),
            ("`Tag", 2),
            ("inner", 13),
            ("'a'", 9),
            ("'type", 1),
        ],
    );
    assert_tokens(
        "haskell",
        "module Main where\nx = {- outer {- nested -} -} 0xff\ny = \"s\"\n",
        &[
            ("module", 2),
            ("Main", 7),
            ("nested", 15),
            ("0xff", 3),
            ("\"s\"", 4),
        ],
    );
    assert_tokens(
        "lisp",
        "(defun x () #| block |# :key 'sym #\\Space 12)",
        &[
            ("defun", 3),
            ("block", 12),
            (":key", 5),
            ("sym", 5),
            ("#\\Space", 11),
            ("12", 2),
        ],
    );
    assert_tokens(
        "scheme",
        "(define x #| comment |# \"a\\\"b\" 42)",
        &[("define", 3), ("comment", 12), ("\"a\\\"b\"", 6), ("42", 2)],
    );
    assert_tokens(
        "erlang",
        "-module(m).\nf(X) -> ?MACRO, #record{}, $a, 'atom', 16#ff. % note\n",
        &[
            ("-module", 24),
            ("X", 2),
            ("?MACRO", 10),
            ("#record", 11),
            ("$a", 9),
            ("'atom'", 18),
            ("16#ff", 3),
            ("% note", 1),
        ],
    );
    assert_tokens(
        "nim",
        "proc thing() =\n  ## docs\n  #[ a #[ nested ]# ]#\n  let x = \"\"\"multi\nline\"\"\"\n",
        &[
            ("proc", 5),
            ("thing", 11),
            ("## docs", 15),
            ("nested", 1),
            ("let", 5),
            ("multi\nline", 7),
        ],
    );
    assert_tokens(
        "smalltalk",
        "self foo: true. x := #symbol. $a \"comment\" 'it''s'",
        &[
            ("self", 7),
            ("foo:", 13),
            ("true", 6),
            (":=", 14),
            ("#symbol", 4),
            ("$a", 15),
            ("\"comment\"", 3),
            ("'it''s'", 1),
        ],
    );
    assert_tokens(
        "visualprolog",
        "class foo\nX = _Anon, /* a /* nested */ b */ \"x\\n\". % note\n",
        &[
            ("class", 1),
            ("foo", 8),
            ("X", 9),
            ("_Anon", 10),
            ("nested", 4),
            ("x", 20),
            ("\\n", 17),
            ("% note", 5),
        ],
    );
}

#[test]
fn basic_dialects_are_not_interchangeable() {
    assert_tokens(
        "blitzbasic",
        ".label\nFunction f()\n; comment\nx = $ff + %101\nEnd Function\n",
        &[
            (".label", 15),
            ("Function", 3),
            ("; comment", 1),
            ("$ff", 17),
            ("%101", 18),
        ],
    );
    assert_tokens(
        "purebasic",
        "Procedure f()\nx = #constant + $af\n; comment\nEndProcedure\n",
        &[
            ("Procedure", 3),
            ("#constant", 13),
            ("$af", 17),
            ("; comment", 1),
        ],
    );
    assert_tokens(
        "freebasic",
        "sub f()\n/' block\ncomment '/\nx = &hff\n' comment\nend sub\n",
        &[
            ("sub", 3),
            ("block\ncomment", 19),
            ("&hff", 17),
            ("' comment", 1),
        ],
    );
    assert_tokens(
        "vb",
        "#If DEBUG Then\nDim x = #1/2/2000#\nRem note\nx = \"a\"\"b\"\nx = [Class]\n",
        &[
            ("#If DEBUG Then", 5),
            ("Dim", 3),
            ("#1/2/2000#", 8),
            ("Rem note", 1),
            ("\"a\"\"b\"", 4),
            ("[Class]", 7),
        ],
    );
    assert_tokens(
        "hollywood",
        "Function f()\n/* block */ x = [[multi\nline]]\n@INCLUDE \"file\"\n#CONST $ff\nEndFunction\n",
        &[
            ("Function", 4),
            ("block", 2),
            ("[[multi\nline]]", 9),
            ("@INCLUDE", 10),
            ("\"file\"", 8),
            ("#CONST", 13),
            ("$ff", 14),
        ],
    );
}

#[test]
fn scientific_lexers_and_fixed_columns() {
    assert_tokens(
        "fortran",
        "program p\nx = 'a' &\n  & // 'b' ! comment\nx = .true.\nend program\n",
        &[
            ("program", 8),
            ("'a'", 3),
            ("&", 14),
            ("! comment", 1),
            (".true.", 12),
        ],
    );
    assert_tokens(
        "fortran77",
        "C comment\n12345+PRINT *, 'text'\n      END\n",
        &[
            ("C comment", 1),
            ("12345", 13),
            ("+", 14),
            ("PRINT", 8),
            ("'text'", 3),
        ],
    );
    assert_tokens(
        "matlab",
        "x = A'; y = 'it''s'; z = \"s\"\n%{\nblock\n%}\nx = 1 ... continued\n",
        &[
            ("A", 7),
            ("'it''s'", 5),
            ("\"s\"", 8),
            ("block", 1),
            ("... continued", 1),
        ],
    );
    assert_tokens(
        "r",
        "if (TRUE) { x <- r\"---(raw\ntext)---\"; y <- `odd name` %in% 0xff } # note\n",
        &[
            ("if", 2),
            ("raw\ntext", 13),
            ("`odd name`", 12),
            ("%in%", 10),
            ("0xff", 5),
            ("# note", 1),
        ],
    );
    assert_tokens(
        "sas",
        "* statement comment;\n%let foo = 12; /* block */\nx = \"text\";\n",
        &[
            ("* statement comment", 1),
            ("%let", 12),
            ("12", 4),
            ("block", 3),
            ("\"text\"", 7),
        ],
    );
    assert_tokens(
        "spice",
        "* comment\nR1 in 0 1k\nV1 in 0 1e-3\n",
        &[("* comment", 8), ("R1", 1), ("1k", 5), ("1e-3", 5)],
    );
    assert_tokens(
        "csound",
        "instr 1\naout oscili p4, 440\nkval = 1\nendin ; note\n",
        &[
            ("instr", 6),
            ("aout", 11),
            ("oscili", 6),
            ("p4", 10),
            ("kval", 12),
            ("440", 2),
            ("; note", 1),
        ],
    );
}

#[test]
fn hardware_and_column_sensitive_lexers() {
    assert_tokens(
        "asm",
        "mov eax, 0ffh ; note\nmylabel: db 'x', \"text\"\n",
        &[
            ("mov", 6),
            ("eax", 8),
            ("0ffh", 2),
            ("; note", 1),
            ("mylabel", 5),
            ("db", 9),
            ("'x'", 12),
            ("\"text\"", 3),
        ],
    );
    assert_tokens(
        "asn1",
        "DEFINITIONS ::= BEGIN\nmyId OBJECT IDENTIFIER ::= { iso 3 6 }\nx INTEGER ::= 42 -- note\nEND",
        &[
            ("DEFINITIONS", 6),
            ("iso", 2),
            ("3", 4),
            ("42", 4),
            ("-- note", 1),
        ],
    );
    assert_tokens(
        "cobol",
        "000100* comment\n       MOVE 123 TO MY-VAR *> note\n       DISPLAY \"text\"\n",
        &[
            ("* comment", 2),
            ("MOVE", 16),
            ("123", 4),
            ("MY-VAR", 11),
            ("*> note", 2),
            ("\"text\"", 6),
        ],
    );
    assert_tokens(
        "fcST",
        "PROGRAM main\nx := 16#FF; t := T#1s;\n(* block *) // note\ny := 'str'; {pragma}\nEND_PROGRAM\n",
        &[
            ("PROGRAM", 3),
            ("16", 7),
            ("#", 11),
            ("FF", 8),
            ("1s", 16),
            ("block", 1),
            ("// note", 2),
            ("'str'", 12),
            ("{pragma}", 9),
        ],
    );
    assert_tokens(
        "forth",
        ": square dup * ;\n( stack\ncomment )\n\\ note\n123 2dup { local }\n",
        &[
            (": square", 6),
            ("dup", 5),
            ("comment", 2),
            ("\\ note", 1),
            ("123", 9),
            ("2dup", 5),
            ("{ local }", 11),
        ],
    );
    assert_tokens(
        "mmixal",
        "Main ADD $1,$2,#ff comment\n* note\n",
        &[
            ("Main", 2),
            ("ADD", 5),
            ("$1", 13),
            ("#ff", 14),
            ("comment", 1),
            ("* note", 1),
        ],
    );
    assert_tokens(
        "verilog",
        "module m;\nwire x = 8'hff;\n//! note\n/* block */\nendmodule\n",
        &[
            ("module", 5),
            ("wire", 5),
            ("8'hff", 4),
            ("//! note", 3),
            ("block", 1),
            ("endmodule", 5),
        ],
    );
    assert_tokens(
        "vhdl",
        "entity e is\nport(x : in std_logic); --! note\nend entity;\nx <= '1'; /* block */\n",
        &[
            ("entity", 8),
            ("std_logic", 13),
            ("--! note", 2),
            ("'1'", 4),
            ("block", 15),
        ],
    );
}

#[test]
fn negative_identifiers_for_every_assigned_language() {
    for (key, style) in [
        ("ada", 2),
        ("asm", 5),
        ("asn1", 2),
        ("blitzbasic", 7),
        ("caml", 1),
        ("cobol", 11),
        ("csound", 5),
        ("erlang", 7),
        ("fcST", 15),
        ("forth", 3),
        ("fortran", 7),
        ("fortran77", 7),
        ("freebasic", 7),
        ("haskell", 1),
        ("hollywood", 12),
        ("lisp", 9),
        ("matlab", 7),
        ("mmixal", 6),
        ("nim", 11),
        ("purebasic", 7),
        ("r", 9),
        ("sas", 6),
        ("scheme", 9),
        ("smalltalk", 0),
        ("spice", 1),
        ("vb", 7),
        ("verilog", 11),
        ("vhdl", 6),
        ("visualprolog", 8),
    ] {
        assert_tokens(key, "      zxqword\n", &[("zxqword", style)]);
        assert!(
            lexers::lex("\u{1}\n", profile(key)).is_ok(),
            "ordinary unknown text {key}"
        );
    }
    let mut unknown = profile("ada").clone();
    unknown.name = "not-a-runtime-language".to_owned();
    assert!(matches!(
        lexers::lex("", &unknown),
        Err(lexers::LexError::UnsupportedLanguage(_))
    ));
}

#[test]
fn fold_policies_follow_the_frozen_lexers() {
    for (key, text) in [
        ("blitzbasic", "Function f()\nx=1\nEnd Function\n"),
        ("freebasic", "sub f()\nx=1\nend sub\n"),
        ("purebasic", "Procedure f()\nx=1\nEndProcedure\n"),
        ("fortran", "program p\nx=1\nend program\n"),
        ("csound", "instr 1\naout = 1\nendin\n"),
        ("r", "{\nx=1\n}\n"),
        ("lisp", "(progn\nx\n)\n"),
        ("scheme", "(begin\nx\n)\n"),
        ("fcST", "IF x THEN\ny:=1;\nEND_IF;\n"),
        ("verilog", "begin\nx=1;\nend\n"),
    ] {
        let result = lexers::lex(text, profile(key)).expect("fold lexer");
        assert!(result.fold_levels[0].header, "{key} header");
        assert!(
            result
                .folds
                .iter()
                .any(|f| f.start_line == 0 && f.end_line == 2),
            "{key} {:?}",
            result.folds
        );
        assert!(
            !result.fold_levels[2].header,
            "{key} close is not an opener"
        );
        let mut disabled = profile(key).clone();
        disabled
            .properties
            .insert("fold".to_owned(), "0".to_owned());
        let result = lexers::lex(text, &disabled).expect("disabled folds");
        assert!(result.folds.is_empty());
        assert!(
            result
                .fold_levels
                .iter()
                .all(|l| !l.header && l.level == 0x400)
        );
    }
    for (key, text) in [
        ("ada", "begin\nx\nend\n"),
        ("caml", "(* a\nb *)\n"),
        ("asn1", "x ::= {\n1\n}\n"),
        ("forth", "( a\nb )\n"),
        ("smalltalk", "[\nx\n]\n"),
        ("sas", "{\nx\n}\n"),
    ] {
        assert!(
            lexers::lex(text, profile(key))
                .expect("no fold source")
                .folds
                .is_empty(),
            "{key} has no corresponding source folder"
        );
    }
    assert!(
        !lexers::lex("Sub f()\nx=1\nEnd Sub\n", profile("vb"))
            .expect("VB")
            .fold_levels[0]
            .header
    );
    assert!(
        lexers::lex("Sub f()\n  x=1\nEnd Sub\n", profile("vb"))
            .expect("VB indent")
            .fold_levels[0]
            .header
    );
}

const KEYS: &[&str] = &[
    "ada",
    "asm",
    "asn1",
    "blitzbasic",
    "caml",
    "cobol",
    "csound",
    "erlang",
    "fcST",
    "forth",
    "fortran",
    "fortran77",
    "freebasic",
    "haskell",
    "hollywood",
    "lisp",
    "matlab",
    "mmixal",
    "nim",
    "purebasic",
    "r",
    "sas",
    "scheme",
    "smalltalk",
    "spice",
    "vb",
    "verilog",
    "vhdl",
    "visualprolog",
];

#[test]
fn malformed_inputs_preserve_shared_result_invariants() {
    for &key in KEYS {
        for text in [
            "",
            "<<",
            "`",
            "#",
            "0x",
            "\"",
            "'",
            "(*",
            "/*",
            "#[",
            "#|",
            "[|",
            "{|",
            "@\"",
            "\"a\\",
            "x\r\ny\rz\n",
            "é λ 😀\r\n",
            "      '",
            "      0",
            "      &",
            "      `",
        ] {
            let result = lexers::lex(text, profile(key))
                .expect("unrecognized source is not an unsupported language");
            let mut end = 0;
            for span in result.spans {
                assert_eq!(span.range.start, end, "{key} {text:?}");
                assert!(
                    text.is_char_boundary(span.range.start)
                        && text.is_char_boundary(span.range.end)
                );
                assert!(span.range.start < span.range.end);
                end = span.range.end;
            }
            assert_eq!(end, text.len(), "{key} {text:?}");
            assert_eq!(
                result.line_states.len(),
                lexers::shared::line_ranges(text).len(),
                "{key} {text:?}"
            );
            assert_eq!(result.fold_levels.len(), result.line_states.len());
        }
    }
}

fn with_property(key: &str, text: &str, property: &str, value: &str) -> Vec<u16> {
    let mut profile = profile(key).clone();
    profile
        .properties
        .insert(property.to_owned(), value.to_owned());
    let result = lexers::lex(text, &profile).expect("property lexer");
    let mut got = vec![0; text.len()];
    for span in result.spans {
        got[span.range].fill(span.style);
    }
    got
}

#[test]
fn source_property_switches() {
    assert_eq!(
        with_property("caml", "(*@rc text*)", "lexer.caml.magic", "1")[6],
        28
    );
    assert_eq!(
        with_property("asm", "!comment", "lexer.as.comment.character", "!")[0],
        1
    );
    assert_eq!(
        with_property("r", "\"a\\n\"", "lexer.r.escape.sequence", "1")[2..4],
        [15, 15]
    );
    assert_eq!(
        with_property("r", "\"a\\n\"", "lexer.r.escape.sequence", "0")[2..4],
        [6, 6]
    );
    assert_eq!(
        with_property(
            "visualprolog",
            "`text`",
            "lexer.visualprolog.backquoted.strings",
            "1"
        )[1..5],
        [20; 4]
    );
    assert_eq!(
        with_property("verilog", "input x;", "lexer.verilog.portstyling", "1")[..5],
        [21; 5]
    );
    assert_eq!(
        with_property(
            "verilog",
            "SOME_CONST",
            "lexer.verilog.allupperkeywords",
            "1"
        ),
        vec![19; 10]
    );
    assert_eq!(
        with_property("haskell", "#include x", "lexer.haskell.cpp", "1")[..8],
        [18; 8]
    );
    assert_ne!(
        with_property("haskell", "#include x", "lexer.haskell.cpp", "0")[0],
        18
    );
    assert_eq!(
        with_property("haskell", "foo#", "lexer.haskell.allow.hash", "1"),
        vec![1; 4]
    );
    assert_eq!(
        with_property("haskell", "foo#", "lexer.haskell.allow.hash", "0")[3],
        11
    );
    assert_eq!(
        with_property("haskell", "?foo", "lexer.haskell.allow.questionmark", "1"),
        vec![1; 4]
    );
}

#[test]
fn literal_boundaries_and_continuations_are_source_specific() {
    assert_tokens(
        "haskell",
        "123xyz 0xFF 1.5e-2 foo#\n",
        &[
            ("123", 3),
            ("xyz", 1),
            ("0xFF", 3),
            ("1.5e-2", 3),
            ("foo#", 1),
        ],
    );
    assert_tokens(
        "lisp",
        "-12 #xFF *special* 1/2\n",
        &[("-12", 9), ("#xFF", 11), ("*special*", 11), ("1/2", 9)],
    );
    assert_tokens(
        "smalltalk",
        "16rFF 2.5s2 1.2e-3 123xyz",
        &[
            ("16rFF", 2),
            ("2.5s2", 2),
            ("1.2e-3", 2),
            ("123", 2),
            ("xyz", 0),
        ],
    );
    assert_tokens(
        "r",
        "123xyz 2L 3i 0x1.5p+2",
        &[
            ("123", 5),
            ("xyz", 9),
            ("2L", 5),
            ("3i", 5),
            ("0x1.5p+2", 5),
        ],
    );
    assert_tokens(
        "vb",
        "x=123xyz\nx=\"a\"c\nx=$\"{name}\"\nx=<node/>\n",
        &[
            ("123", 2),
            ("xyz", 7),
            ("\"a\"c", 4),
            ("\"{name}\"", 4),
            ("node", 7),
        ],
    );
    assert_tokens(
        "fortran",
        "x = z'FF'\n100 continue\n",
        &[("z'FF'", 2), ("100", 13)],
    );
    assert_tokens(
        "erlang",
        "-12, 123xyz, f().\n",
        &[("-12", 3), ("123", 3), ("xyz", 7)],
    );
    assert_eq!(
        with_property("vb", "\"one\ntwo\"", "lexer.vb.strings.multiline", "1"),
        vec![4; 9]
    );
    assert_tokens("asm", "db \"one\\\r\ntwo\"\n", &[("\"one\\\r\ntwo\"", 3)]);
}

#[test]
fn dialect_context_and_nonfeatures_match_source() {
    assert_tokens(
        "haskell",
        "class Thing a where\nimport qualified Data.Map as M\nx = 'name\ny = :>\n",
        &[
            ("Thing", 8),
            ("qualified", 2),
            ("Data.Map", 7),
            ("M", 7),
            ("'name", 1),
            (":>", 8),
        ],
    );
    assert_tokens("vhdl", "x'event", &[("x", 6), ("'", 0), ("event", 10)]);
    assert_tokens("matlab", "A(end)", &[("A", 7), ("end", 6)]);
    let result = lexers::lex("If x\ny\nEndIf\n/* a\nb */\n", profile("hollywood"))
        .expect("Hollywood fold policy");
    assert!(result.folds.is_empty());
    let result =
        lexers::lex("PROGRAM x\ny:=1;\nEND_PROGRAM;\n", profile("fcST")).expect("ST fold policy");
    assert!(result.folds.is_empty());
    let text = "(* one (* two *) three *)";
    let got = styles("caml", text);
    assert_eq!(got[text.find("two").expect("nested")], 13);
    assert_eq!(got[text.find("three").expect("outer")], 12);
}

#[test]
fn indentation_and_explicit_fold_properties() {
    let result = lexers::lex("Sub f()\n    x=1\nEnd Sub\n", profile("vb")).expect("VB indentation");
    assert_eq!(result.fold_levels[1].level, 0x404);
    let mut profile = profile("haskell").clone();
    profile
        .properties
        .insert("fold.haskell.imports".to_owned(), "1".to_owned());
    let result = lexers::lex("import A\nimport B\nx = 1\n", &profile).expect("import folds");
    assert!(
        result
            .folds
            .iter()
            .any(|f| f.start_line == 0 && f.end_line == 1)
    );
    let mut profile = languages::catalog()
        .expect("catalog")
        .by_name("asm")
        .expect("ASM")
        .clone();
    profile
        .properties
        .insert("lexer.asm.comment.delimiter".to_owned(), "!".to_owned());
    let result = lexers::lex("comment !\none\ntwo\n!\n", &profile).expect("ASM directive");
    assert!(
        result
            .spans
            .iter()
            .any(|span| span.style == 15 && span.range.end >= 19)
    );
    assert!(result.fold_levels[0].header);
}

#[test]
fn hardware_directive_and_erlang_attribute_classes() {
    assert_tokens(
        "verilog",
        "`define ZXCONST 42\nwire x = `ZXCONST;\n",
        &[("`define", 9), ("ZXCONST", 11), ("42", 4), ("`ZXCONST", 9)],
    );
    assert_tokens(
        "erlang",
        "-export([f/1]).\n-define(X, 1).\nf() -> lists:map(F, X), 'node@host'.\n",
        &[
            ("-export", 24),
            ("-define", 12),
            ("lists:", 23),
            ("map", 8),
            ("'node@host'", 21),
        ],
    );
    let text = "%% @doc text\n%% @unknown text\n";
    assert_tokens("erlang", text, &[("@doc", 16), ("@unknown", 14)]);
}
