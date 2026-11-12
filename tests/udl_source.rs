use notepad_matrix::udl::*;

#[test]
fn source_defaults_and_lossless_json_are_transactional() {
    let d = Definition::default();
    assert!(!d.case_ignored && !d.fold_compact && !d.allow_fold_comments);
    assert_eq!(d.decimal_separator, DecimalSeparator::Dot);
    assert_eq!(d.line_comment_placement, LineCommentPlacement::Anywhere);
    assert_eq!(d.styles.len(), 24);
    assert_eq!(d.styles[0].name, "DEFAULT");
    assert_eq!(d.styles[15].name, "FOLDER IN COMMENT");
    assert_eq!(d.styles[23].name, "DELIMITERS8");
    assert_eq!(UdlStyleId::Delimiter8 as u8, 23);
    assert_eq!(UdlStyleId::Identifier as u8, 24);
    assert_eq!(Definition::from_json(&d.to_json()).unwrap(), d);
    let old = d.clone();
    assert!(Definition::from_json(&serde_json::json!({"case_ignored": "yes"})).is_err());
    assert_eq!(d, old);
}

fn style_at(result: &LexResult, byte: usize) -> UdlStyleId {
    result
        .spans
        .iter()
        .find(|s| s.range.contains(&byte))
        .unwrap()
        .style
}

#[test]
fn eight_keyword_groups_case_prefix_and_source_word_boundaries() {
    let mut d = Definition::default();
    for (i, word) in [
        "one", "two", "three", "four", "five", "six", "seven", "eight",
    ]
    .iter()
    .enumerate()
    {
        d.keywords[i] = word.to_string();
    }
    d.prefix_keywords[7] = true;
    d.case_ignored = true;
    d.operators[0] = "+".into();
    let text = "ONE two three four five six seven eightfold one+two one.two 中文";
    let r = lex(text, &d);
    for (word, id) in [
        ("ONE", 4),
        ("two", 5),
        ("three", 6),
        ("four", 7),
        ("five", 8),
        ("six", 9),
        ("seven", 10),
        ("eightfold", 11),
    ] {
        assert_eq!(style_at(&r, text.find(word).unwrap()) as u16, id, "{word}");
    }
    assert_eq!(
        style_at(&r, text.find("one.two").unwrap()),
        UdlStyleId::Default
    );
    for s in &r.spans {
        assert!(text.is_char_boundary(s.range.start) && text.is_char_boundary(s.range.end));
    }
}

#[test]
fn eight_delimiters_paired_alternatives_escape_and_multiline_nesting() {
    let mut d = Definition::default();
    for i in 0..8 {
        d.delimiters[i].open = format!("<{i}>");
        d.delimiters[i].close = format!("</{i}>");
    }
    d.delimiters[0].escape = "\\".into();
    d.styles[16].nesting = nesting::delimiter(1) | nesting::keyword(7);
    d.keywords[7] = "special".into();
    let text = "<0>甲\\界 <1>nested\n跨行</1> special </0> tail";
    let r = lex(text, &d);
    assert_eq!(
        style_at(&r, text.find('甲').unwrap()),
        UdlStyleId::Delimiter1
    );
    assert_eq!(
        style_at(&r, text.find("nested").unwrap()),
        UdlStyleId::Delimiter2
    );
    assert_eq!(
        style_at(&r, text.find("跨行").unwrap()),
        UdlStyleId::Delimiter2
    );
    assert_eq!(
        style_at(&r, text.find("special").unwrap()),
        UdlStyleId::Keyword8
    );
    assert_eq!(
        style_at(&r, text.find("tail").unwrap()),
        UdlStyleId::Default
    );
    for i in 0..8 {
        let t = format!("<{i}>x</{i}>");
        assert_eq!(
            style_at(&lex(&t, &d), t.find('x').unwrap()) as u16,
            16 + i as u16
        );
    }
    d.delimiters[0].open = "((a b)) c".into();
    d.delimiters[0].close = "x ((EOL))".into();
    assert_eq!(style_at(&lex("b yesx tail", &d), 2), UdlStyleId::Delimiter1);
    let r = lex("c first\r\nnext", &d);
    assert_eq!(style_at(&r, 3), UdlStyleId::Delimiter1);
    assert_eq!(style_at(&r, 9), UdlStyleId::Default);
    let r = lex("<7> not relevant", &d);
    assert_eq!(style_at(&r, 5), UdlStyleId::Delimiter8);
}

#[test]
fn comments_placement_continuation_explicit_close_and_nested_blocks() {
    let mut d = Definition::default();
    d.comments.line_open = "// #".into();
    d.comments.line_continue = "\\ !".into();
    d.comments.line_close = "; END".into();
    d.comments.block_open = "/* (*".into();
    d.comments.block_close = "*/ *)".into();
    d.styles[1].nesting = nesting::COMMENT;
    let text = "code // a\\\r\ncontinued; tail\n/* outer (* inner\nend *) outer */ done";
    let r = lex(text, &d);
    for word in ["a\\", "continued"] {
        assert_eq!(
            style_at(&r, text.find(word).unwrap()),
            UdlStyleId::CommentLine
        );
    }
    for word in ["outer", "inner", "end", "outer */"] {
        assert_eq!(style_at(&r, text.find(word).unwrap()), UdlStyleId::Comment);
    }
    for word in ["tail", "done"] {
        assert_eq!(style_at(&r, text.find(word).unwrap()), UdlStyleId::Default);
    }
    let t = "//at start\n  //indented\ncode //inline\n#second!\nnextEND tail";
    d.line_comment_placement = LineCommentPlacement::BeginningOfLine;
    let r = lex(t, &d);
    assert_eq!(
        style_at(&r, t.find("at start").unwrap()),
        UdlStyleId::CommentLine
    );
    assert_eq!(
        style_at(&r, t.find("indented").unwrap()),
        UdlStyleId::Default
    );
    assert_eq!(
        style_at(&r, t.find("next").unwrap()),
        UdlStyleId::CommentLine
    );
    d.line_comment_placement = LineCommentPlacement::AfterWhitespace;
    let r = lex(t, &d);
    assert_eq!(
        style_at(&r, t.find("indented").unwrap()),
        UdlStyleId::CommentLine
    );
    assert_eq!(style_at(&r, t.find("inline").unwrap()), UdlStyleId::Default);
}

#[test]
fn number_prefix_suffix_extras_ranges_decimal_and_exponent_rules() {
    let mut d = Definition::default();
    d.numbers.prefix1 = "$".into();
    d.numbers.prefix2 = "0x".into();
    d.numbers.extras1 = "A B C D E F a b c d e f".into();
    d.numbers.extras2 = "A B C D E F".into();
    d.numbers.suffix1 = "h".into();
    d.numbers.suffix2 = "u L".into();
    d.numbers.range = "..".into();
    d.operators[0] = "+ - ;".into();
    let good = [
        "123", "-42", "+8", ".25", "1.5e-2", "$123", "-$123", "0xAF", "-0xF", "AFh", "12u", "1..5",
        "0xA..0xF",
    ];
    for token in good {
        let r = lex(token, &d);
        assert!(
            r.spans.iter().all(|s| s.style == UdlStyleId::Number),
            "{token}: {:?}",
            r.spans
        );
    }
    for token in ["0x", "$F", "AF", "1u2", "12unknown", "1.2.3", "1..2..3"] {
        let r = lex(token, &d);
        assert!(
            !r.spans.iter().any(|s| s.style == UdlStyleId::Number),
            "{token}: {:?}",
            r.spans
        );
    }
    d.decimal_separator = DecimalSeparator::Comma;
    assert_eq!(style_at(&lex("1,25", &d), 0), UdlStyleId::Number);
    assert_eq!(style_at(&lex("1.25", &d), 0), UdlStyleId::Default);
    d.decimal_separator = DecimalSeparator::Both;
    assert_eq!(style_at(&lex("1.25 1,25", &d), 0), UdlStyleId::Number);
    d.delimiters[7].open = "[".into();
    d.delimiters[7].close = "]".into();
    d.styles[23].nesting = nesting::NUMBERS;
    assert_eq!(style_at(&lex("[0xAF]", &d), 1), UdlStyleId::Number);
}

#[test]
fn operator_groups_and_quoted_multipart_words_follow_source_rules() {
    let mut d = Definition::default();
    d.operators[0] = "+ ==".into();
    d.operators[1] = "and 'or else'".into();
    d.keywords[0] = "else \"else if\"".into();
    d.keywords[1] = "'same line'".into();
    let t = "a+b and candy or\telse else\n if same\tline same\nline";
    let r = lex(t, &d);
    for token in ["+", "and", "or\telse"] {
        assert_eq!(style_at(&r, t.find(token).unwrap()), UdlStyleId::Operator);
    }
    assert_eq!(style_at(&r, t.find("candy").unwrap()), UdlStyleId::Default);
    assert_eq!(
        style_at(&r, t.find("else\n if").unwrap() + 5),
        UdlStyleId::Keyword1
    );
    assert_eq!(
        style_at(&r, t.find("same\tline").unwrap()),
        UdlStyleId::Keyword2
    );
    assert_eq!(
        style_at(&r, t.find("same\nline").unwrap()),
        UdlStyleId::Default
    );
    d.delimiters[0].open = "[".into();
    d.delimiters[0].close = "]".into();
    d.styles[16].nesting = nesting::OPERATORS1 | nesting::OPERATORS2;
    assert_eq!(style_at(&lex("[a+b and c]", &d), 2), UdlStyleId::Operator);
    assert_eq!(style_at(&lex("[a+b and c]", &d), 5), UdlStyleId::Operator);
}

#[test]
fn three_folding_groups_middle_points_levels_compact_and_comments() {
    let mut d = Definition::default();
    d.folding[0].open = "{".into();
    d.folding[0].close = "}".into();
    d.folding[1].open = "begin".into();
    d.folding[1].middle = "else".into();
    d.folding[1].close = "end".into();
    d.folding[2].open = "region".into();
    d.folding[2].close = "endregion".into();
    d.comments.line_open = "//".into();
    d.fold_compact = true;
    let t = "{\nbegin\nfirst\nelse\nsecond\nend\n}\n// region\n// body\n// endregion\n\n";
    let r = lex(t, &d);
    for (a, b) in [(0, 6), (1, 2), (3, 5), (7, 9)] {
        assert!(
            r.folds.iter().any(|f| f.start_line == a && f.end_line == b),
            "{a}..{b}: {:?}",
            r.folds
        );
    }
    assert!(r.levels[0].header && r.levels[1].header && r.levels[3].header);
    assert_eq!(r.levels[2].level, 2);
    assert!(r.levels[10].white);
    assert_eq!(
        style_at(&r, t.find('{').unwrap()),
        UdlStyleId::FolderInCode1
    );
    assert_eq!(
        style_at(&r, t.find("begin").unwrap()),
        UdlStyleId::FolderInCode2
    );
    assert_eq!(
        style_at(&r, t.find("region").unwrap()),
        UdlStyleId::FolderInComment
    );
    let comments = "// first\n// second\n// third\ncode";
    assert!(lex(comments, &d).folds.is_empty());
    d.allow_fold_comments = true;
    assert_eq!(
        lex(comments, &d).folds,
        vec![FoldRange {
            start_line: 0,
            end_line: 2
        }]
    );
    d.delimiters[0].open = "\"".into();
    d.delimiters[0].close = "\"".into();
    assert!(lex("\"{\nbegin\nend\n}\"", &d).folds.is_empty());
    d.styles[16].nesting = nesting::code2(0) | nesting::code2(2);
    assert_eq!(
        lex("\"begin\nx\nend\"", &d).folds,
        vec![FoldRange {
            start_line: 0,
            end_line: 2
        }]
    );
}

#[test]
fn multipart_prefix_consumes_next_word_not_a_suffix_on_last_part() {
    let mut d = Definition::default();
    d.keywords[0] = "\"else if\"".into();
    d.prefix_keywords[0] = true;
    let text = "else if nextWord tail else iffy";
    let r = lex(text, &d);
    assert_eq!(
        style_at(&r, text.find("nextWord").unwrap()),
        UdlStyleId::Keyword1
    );
    assert_eq!(
        style_at(&r, text.find("tail").unwrap()),
        UdlStyleId::Default
    );
    assert_eq!(
        style_at(&r, text.find("iffy").unwrap()),
        UdlStyleId::Default
    );
}

#[test]
fn nested_forward_tokens_precede_numbers_but_root_numbers_precede_operators() {
    let mut d = Definition::default();
    d.delimiters[0].open = "[".into();
    d.delimiters[0].close = "]".into();
    d.operators[0] = "-".into();
    d.styles[16].nesting = nesting::OPERATORS1 | nesting::NUMBERS;
    let r = lex("-12 [-12]", &d);
    assert_eq!(style_at(&r, 0), UdlStyleId::Number);
    assert_eq!(style_at(&r, 5), UdlStyleId::Operator);
    assert_eq!(style_at(&r, 6), UdlStyleId::Number);
}

#[test]
fn eol_delimiter_nested_in_line_comment_returns_eol_to_parent() {
    let mut d = Definition::default();
    d.comments.line_open = "//".into();
    d.styles[2].nesting = nesting::delimiter(0);
    d.delimiters[0].open = "@".into();
    d.delimiters[0].close = "((EOL))".into();
    let text = "// @child\r\nnext";
    let r = lex(text, &d);
    assert_eq!(
        style_at(&r, text.find("child").unwrap()),
        UdlStyleId::Delimiter1
    );
    assert_eq!(
        style_at(&r, text.find('\r').unwrap()),
        UdlStyleId::CommentLine
    );
    assert_eq!(
        style_at(&r, text.find("next").unwrap()),
        UdlStyleId::Default
    );
}

#[test]
fn every_style_attribute_and_unknown_metadata_survive_roundtrip() {
    let mut value = Definition::default().to_json();
    value["name"] = serde_json::json!("同名");
    value["extensions"] = serde_json::json!("one two odd.extension");
    value["udl_version"] = serde_json::json!("2.1");
    value["vendor"] = serde_json::json!({"nested":[null,true,42,"原样"]});
    value["numbers"]["vendor"] = serde_json::json!(["not interpreted"]);
    value["comments"]["vendor"] = serde_json::json!(false);
    value["folding"][2]["vendor"] = serde_json::json!("fold metadata");
    value["delimiters"][7]["vendor"] = serde_json::json!("delimiter metadata");
    for (i, s) in value["styles"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        s["foreground"] = serde_json::json!(0x123456 + i);
        s["background"] = serde_json::json!(0xfedcba - i);
        s["color_style"] = serde_json::json!(i % 4);
        s["font_enabled"] = serde_json::json!(true);
        s["font_name"] = serde_json::json!("UDL Test Font");
        s["font_style"] = serde_json::json!(7);
        s["font_size"] = serde_json::json!(240);
        s["nesting"] = serde_json::json!(u32::MAX);
        s["keyword_class"] = serde_json::json!(i);
        s["keywords"] = serde_json::json!("style-associated keywords");
        s["vendor"] = serde_json::json!({"unrecognized":i});
    }
    let d = Definition::from_json(&value).unwrap();
    assert_eq!(d.to_json(), value);
    let before = d.clone();
    for malformed in [
        serde_json::json!({"keywords": ["not eight"]}),
        serde_json::json!({"comments":{"line_open":42}}),
        serde_json::json!({"decimal_separator":3}),
        serde_json::json!({"line_comment_placement":3}),
        serde_json::json!({"delimiters":false}),
        serde_json::json!({"numbers":{"prefix1":null}}),
    ] {
        assert!(Definition::from_json(&malformed).is_err());
        assert_eq!(d, before);
    }
    let mut malformed = value.clone();
    malformed["styles"][23]["id"] = serde_json::json!(0);
    assert!(Definition::from_json(&malformed).is_err());
    malformed["styles"][23]["id"] = serde_json::json!(24);
    assert!(Definition::from_json(&malformed).is_err());
}

#[test]
fn signed_source_nesting_masks_are_accepted_as_bit_patterns() {
    let mut value = Definition::default().to_json();
    value["styles"][16]["nesting"] = serde_json::json!(-1);
    let d = Definition::from_json(&value).unwrap();
    assert_eq!(d.styles[16].nesting, u32::MAX);
}

#[test]
fn delimiter_escapes_and_nesting_masks_apply_to_each_of_eight_styles() {
    let mut d = Definition::default();
    for i in 0..8 {
        d.delimiters[i].open = format!("<{i}>");
        d.delimiters[i].close = format!("</{i}>");
        d.delimiters[i].escape = "!!".into();
        d.keywords[i] = format!("keyword{i}");
        d.styles[i + 16].nesting = nesting::keyword(i);
    }
    for i in 0..8 {
        let text = format!(
            "<{i}>!!</{i}> keyword{i} keyword{} !!界 </{i}>tail",
            (i + 1) % 8
        );
        let r = lex(&text, &d);
        assert_eq!(
            style_at(&r, text.find(&format!("</{i}>")).unwrap()) as u16,
            16 + i as u16
        );
        assert_eq!(
            style_at(&r, text.find(&format!("keyword{i}")).unwrap()) as u16,
            4 + i as u16
        );
        assert_eq!(
            style_at(&r, text.find(&format!("keyword{}", (i + 1) % 8)).unwrap()) as u16,
            16 + i as u16
        );
        assert_eq!(
            style_at(&r, text.find("tail").unwrap()),
            UdlStyleId::Default
        );
        let mut next = 0;
        for span in r.spans {
            assert_eq!(span.range.start, next);
            assert!(
                text.is_char_boundary(span.range.start) && text.is_char_boundary(span.range.end)
            );
            assert!(span.range.end > span.range.start);
            next = span.range.end;
        }
        assert_eq!(next, text.len());
    }
}

#[test]
fn full_scan_recomputes_multiline_state_after_editing_a_close_token() {
    let mut d = Definition::default();
    d.delimiters[0].open = "<".into();
    d.delimiters[0].close = ">".into();
    d.folding[1].open = "begin".into();
    d.folding[1].close = "end".into();
    let before = "begin\r\n<中文\r\nbody>\r\nend";
    assert_eq!(
        lex(before, &d).folds,
        vec![FoldRange {
            start_line: 0,
            end_line: 3
        }]
    );
    let edited = before.replace('>', "");
    let after = lex(&edited, &d);
    assert_eq!(
        style_at(&after, edited.find("end").unwrap()),
        UdlStyleId::Delimiter1
    );
    assert_eq!(after.levels[3].next_level, 1);
    let restored = lex(before, &d);
    assert_eq!(
        style_at(&restored, before.find("end").unwrap()),
        UdlStyleId::FolderInCode2
    );
    assert_eq!(restored.levels[3].next_level, 0);
}

#[test]
fn empty_quoted_words_and_empty_subgroups_are_ignored_not_wildcards() {
    let mut d = Definition::default();
    d.keywords[0] = "\"   \" '  '".into();
    d.prefix_keywords[0] = true;
    assert_eq!(style_at(&lex("ordinary", &d), 0), UdlStyleId::Default);
    d.delimiters[0].open = "(( )) <".into();
    d.delimiters[0].close = ">".into();
    let r = lex("<body>tail", &d);
    assert_eq!(style_at(&r, 1), UdlStyleId::Delimiter1);
    assert_eq!(style_at(&r, 6), UdlStyleId::Default);
}

#[test]
fn forward_operator_group_expands_source_eol_tokens() {
    let mut d = Definition::default();
    d.operators[0] = "(( + EOL ))".into();
    let text = "a+b\r\nEOL\n((\re";
    let r = lex(text, &d);
    for (p, c) in text.char_indices() {
        assert_eq!(
            style_at(&r, p),
            match c {
                '+' => UdlStyleId::Operator,
                '\r' | '\n' => UdlStyleId::Identifier,
                _ => UdlStyleId::Default,
            }
        );
    }
    d.delimiters[0].open = "[".into();
    d.delimiters[0].close = "]".into();
    d.styles[16].nesting = nesting::OPERATORS1;
    assert_eq!(style_at(&lex("[a\nb]", &d), 2), UdlStyleId::Operator);
}

#[test]
fn escapes_do_not_act_as_number_terminators() {
    let mut d = Definition::default();
    d.delimiters[0].open = "\"".into();
    d.delimiters[0].close = "\"".into();
    d.delimiters[0].escape = "\\".into();
    d.styles[16].nesting = nesting::NUMBERS;
    assert_eq!(
        style_at(&lex("\"123\\x tail\"", &d), 1),
        UdlStyleId::Delimiter1
    );
    assert_eq!(style_at(&lex("\"123 tail\"", &d), 1), UdlStyleId::Number);
}

#[test]
fn nested_numbers_are_forward_matches_even_inside_an_identifier() {
    let mut d = Definition::default();
    d.delimiters[0].open = "[".into();
    d.delimiters[0].close = "]".into();
    d.styles[16].nesting = nesting::NUMBERS | nesting::keyword(0);
    d.keywords[0] = "abc".into();
    let r = lex("abc123 [abc123]", &d);
    assert_eq!(style_at(&r, 0), UdlStyleId::Default);
    assert_eq!(style_at(&r, 8), UdlStyleId::Keyword1);
    assert_eq!(style_at(&r, 11), UdlStyleId::Number);
}

#[test]
fn escapes_do_not_split_backward_keyword_words() {
    let mut d = Definition::default();
    d.delimiters[0].open = "\"".into();
    d.delimiters[0].close = "\"".into();
    d.delimiters[0].escape = "\\".into();
    d.styles[16].nesting = nesting::keyword(0);
    d.keywords[0] = "abc tail".into();
    let text = "\"abc\\xtail \\xtail tail\"";
    let r = lex(text, &d);
    assert_eq!(
        style_at(&r, text.find("abc").unwrap()),
        UdlStyleId::Delimiter1
    );
    assert_eq!(
        style_at(&r, text.find("tail").unwrap()),
        UdlStyleId::Delimiter1
    );
    assert_eq!(
        style_at(&r, text.rfind("tail").unwrap()),
        UdlStyleId::Keyword1
    );
    d.keywords[0] = "abc".into();
    d.prefix_keywords[0] = true;
    let r = lex("\"abc\\xtail\"", &d);
    assert_eq!(style_at(&r, 6), UdlStyleId::Keyword1);
}

#[test]
fn audit_root_numbers_end_at_source_close_separators() {
    let mut d = Definition::default();
    d.delimiters[0].open = "[".into();
    d.delimiters[0].close = "]".into();
    assert_eq!(style_at(&lex("1] ", &d), 0), UdlStyleId::Number);
    d.keywords[0] = "name".into();
    assert_eq!(style_at(&lex("name] ", &d), 0), UdlStyleId::Default);
}

#[test]
fn audit_root_numbers_do_not_end_at_delimiter_openers() {
    let mut d = Definition::default();
    d.delimiters[0].open = "[".into();
    d.delimiters[0].close = "]".into();
    let result = lex("1[foo] ", &d);
    assert_eq!(style_at(&result, 0), UdlStyleId::Default);
    assert_eq!(style_at(&result, 2), UdlStyleId::Delimiter1);
}

#[test]
fn audit_block_comment_folding_carries_blank_interior_lines() {
    for compact in [false, true] {
        let mut d = Definition::default();
        d.comments.block_open = "/*".into();
        d.comments.block_close = "*/".into();
        d.allow_fold_comments = true;
        d.fold_compact = compact;
        let result = lex("/* first\n\nlast */\ncode", &d);
        assert_eq!(
            result.folds,
            vec![FoldRange {
                start_line: 0,
                end_line: 2
            }],
            "compact={compact}"
        );
        assert!(result.levels[0].header);
        assert_eq!(result.levels[1].level, 1);
        assert_eq!(result.levels[1].next_level, 1);
        assert_eq!(result.levels[1].white, compact);
        assert_eq!(result.levels[3].level, 0);
        d.comments.line_open = "//".into();
        assert!(lex("// first\n\n// last\ncode", &d).folds.is_empty());
    }
}

#[test]
fn audit_mixed_quote_words_retain_each_separators_whitespace_mode() {
    for family in 0..3 {
        let mut d = Definition::default();
        let (raw, expected) = match family {
            0 => (&mut d.keywords[0], UdlStyleId::Keyword1),
            1 => (&mut d.operators[1], UdlStyleId::Operator),
            _ => (&mut d.folding[1].open, UdlStyleId::FolderInCode2),
        };
        *raw = "'same line'\" next\"".into();
        assert_eq!(
            style_at(&lex("same\nline next ", &d), 0),
            UdlStyleId::Default,
            "family {family}"
        );

        match family {
            0 => d.keywords[0] = "\"same line\"' next'".into(),
            1 => d.operators[1] = "\"same line\"' next'".into(),
            _ => d.folding[1].open = "\"same line\"' next'".into(),
        }
        assert_eq!(
            style_at(&lex("same\nline\tnext ", &d), 0),
            expected,
            "family {family}"
        );
        assert_eq!(
            style_at(&lex("same line\nnext ", &d), 0),
            UdlStyleId::Default,
            "family {family}"
        );

        match family {
            0 => d.keywords[0] = "'same line'\" and next\"".into(),
            1 => d.operators[1] = "'same line'\" and next\"".into(),
            _ => d.folding[1].open = "'same line'\" and next\"".into(),
        }
        assert_eq!(
            style_at(&lex("same\tlineand\nnext ", &d), 0),
            expected,
            "family {family}"
        );
        assert_eq!(
            style_at(&lex("same\nlineand next ", &d), 0),
            UdlStyleId::Default,
            "family {family}"
        );
    }
}
