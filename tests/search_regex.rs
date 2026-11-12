use notepad_matrix::editor::ByteOffset;
use notepad_matrix::search_regex::Utf16View;

#[test]
fn scoped_matching_keeps_original_document_base_and_treats_the_requested_end_as_matcher_last() {
    use notepad_matrix::search_regex::RegexProgram;
    let document = Utf16View::new("xabcZ");
    for (pattern, expected) in [
        (r"abc\z", Some(1..4)),
        ("abc$", Some(1..4)),
        (r"\Aabc", None),
        ("^abc", None),
        ("abcZ", None),
    ] {
        assert_eq!(
            RegexProgram::compile_utf8(pattern)
                .unwrap()
                .match_at_in_scope(&document, 1..4, false, 1000)
                .unwrap(),
            expected,
            "{pattern}"
        );
    }
    let line = RegexProgram::compile_utf8("^abc$").unwrap();
    assert_eq!(
        line.match_at_in_scope(&Utf16View::new("x\nabcZ"), 2..5, false, 1000)
            .unwrap(),
        Some(2..5)
    );
    let empty = RegexProgram::compile_utf8(r"\z").unwrap();
    assert_eq!(
        empty
            .match_at_in_scope(&document, 2..2, false, 1000)
            .unwrap(),
        Some(2..2)
    );
    assert_eq!(
        empty
            .match_at_in_scope(&document, std::ops::Range { start: 3, end: 2 }, false, 1000)
            .unwrap(),
        None
    );
    assert_eq!(
        empty
            .match_at_in_scope(&document, 0..usize::MAX, false, 1000)
            .unwrap(),
        None
    );
}

#[test]
fn alternatives_do_not_reuse_prior_repeat_targets_or_swallow_a_complexity_error() {
    use notepad_matrix::search_regex::{
        EscapeErrorCode, RegexEscapeError, RegexMatchError, RegexProgram,
    };
    for (pattern, position) in [("a|*", 3), ("a*|?", 4)] {
        assert_eq!(
            RegexProgram::compile_utf8(pattern),
            Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::BadRepeat,
                position,
                message: "Invalid preceding regular expression prior to repetition operator."
            })
        );
    }
    let program = RegexProgram::compile_utf8("a*a*a*b|a").unwrap();
    assert_eq!(
        program.match_at_with_state_limit(&Utf16View::new("aaaaaaaaaaaaaaaaaaaa"), 0, false, 10),
        Err(RegexMatchError::Complexity)
    );
    assert_eq!(
        program.match_at(&Utf16View::new("aaab"), 0).unwrap(),
        Some(0..4)
    );
}

#[test]
fn top_level_alternation_keeps_original_branch_order_empty_branches_and_repeat_backtracking() {
    use notepad_matrix::search_regex::RegexProgram;
    for (pattern, text, expected) in [
        ("a|ab", "ab", Some(0..1)),
        ("ab|a", "ab", Some(0..2)),
        ("x|ab", "ab", Some(0..2)),
        ("a|b", "x", None),
        ("|a", "a", Some(0..0)),
        ("a|", "b", Some(0..0)),
        ("a||b", "b", Some(0..0)),
        ("|", "", Some(0..0)),
        ("a*a|b", "aaa", Some(0..3)),
        ("a*+a|aaa", "aaa", Some(0..3)),
        ("a*?|a", "aaa", Some(0..0)),
        (r"\Qa|b\E|c", "a|b", Some(0..3)),
        ("^x$|^a$", "a", Some(0..1)),
    ] {
        assert_eq!(
            RegexProgram::compile_utf8(pattern)
                .unwrap()
                .match_at(&Utf16View::new(text), 0)
                .unwrap(),
            expected,
            "{pattern}"
        );
    }
}

#[test]
fn compiled_braced_ranges_use_explicit_traits_and_the_shared_source_repeat_matcher() {
    use notepad_matrix::search_regex::{CharacterClass, RegexProgram};
    for (pattern, text, expected) in [
        ("a{2,3}a", "aaa", Some(0..3)),
        ("a{2,3}?a", "aaaa", Some(0..3)),
        ("a{2,3}+a", "aaa", None),
        ("a{0}b", "b", Some(0..1)),
        ("a{2,}", "aaaa", Some(0..4)),
        ("a{0,922337203685477581}", "aaaa", Some(0..4)),
        ("a{ 2 , 3 }", "aaaa", Some(0..3)),
        ("a{x}", "a{x}", Some(0..4)),
        (r"\Qab\E{2}", "abb", Some(0..3)),
        ("^a{2}$", "aa", Some(0..2)),
    ] {
        let program = RegexProgram::compile_utf16_with_classification(
            Utf16View::new(pattern).units(),
            |unit, class| match class {
                CharacterClass::Space => matches!(unit, 9..=13 | 32),
                CharacterClass::Word => (u16::from(b'0')..=u16::from(b'9')).contains(&unit),
                _ => unreachable!(),
            },
        )
        .unwrap();
        assert_eq!(
            program.match_at(&Utf16View::new(text), 0).unwrap(),
            expected,
            "{pattern}"
        );
    }
}

#[test]
fn braced_ranges_preserve_original_repeat_target_validation_after_consuming_the_range() {
    use notepad_matrix::search_regex::{
        CharacterClass, EscapeErrorCode, RegexEscapeError, RegexProgram,
    };
    for (pattern, position) in [("{2}", 3), ("^{2}", 4), (r"\Q\E{2}", 7)] {
        assert_eq!(
            RegexProgram::compile_utf16_with_classification(
                Utf16View::new(pattern).units(),
                |unit, class| class == CharacterClass::Space && matches!(unit, 9..=13 | 32)
            ),
            Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::BadRepeat,
                position,
                message: "Invalid preceding regular expression prior to repetition operator."
            }),
            "{pattern}"
        );
    }
}

#[test]
fn repeat_range_decoder_keeps_source_numeric_threshold_error_rewind_and_explicit_classes() {
    use notepad_matrix::search_regex::{
        CharacterClass, EscapeErrorCode, RegexEscapeError, RepeatRange, decode_repeat_range,
    };
    let classify = |unit, class| match class {
        CharacterClass::Space => matches!(unit, 9..=13 | 32 | 0xa0),
        CharacterClass::Word => (u16::from(b'0')..=u16::from(b'9')).contains(&unit),
        _ => unreachable!(),
    };
    for (pattern, open, position) in [("{3,2}", 0, 3), ("{3,2 }", 0, 5), ("😀{3,2}", 2, 5)] {
        assert_eq!(
            decode_repeat_range(Utf16View::new(pattern).units(), open, classify),
            Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::BadBrace,
                position,
                message: "Invalid content of repeat range."
            }),
            "{pattern}"
        );
    }
    assert_eq!(
        decode_repeat_range(Utf16View::new("{\u{a0}2,3}").units(), 0, classify),
        Ok(RepeatRange::Bounds {
            minimum: 2,
            maximum: Some(3),
            end: 6
        })
    );
    for pattern in ["{922337203685477581}", "{0,9223372036854775810}"] {
        assert_eq!(
            decode_repeat_range(Utf16View::new(pattern).units(), 0, classify),
            Ok(RepeatRange::Literal { end: 1 }),
            "{pattern}"
        );
    }
    assert_eq!(
        decode_repeat_range(
            Utf16View::new("{0,922337203685477580}").units(),
            0,
            classify
        ),
        Ok(RepeatRange::Bounds {
            minimum: 0,
            maximum: Some(922_337_203_685_477_580),
            end: 22
        })
    );
    assert_eq!(
        decode_repeat_range(&[], usize::MAX, |_, _| unreachable!()),
        Err(RegexEscapeError::InvalidPosition(usize::MAX))
    );
}

#[test]
fn repeat_range_decoder_keeps_source_whitespace_open_bounds_and_literal_fallback() {
    use notepad_matrix::search_regex::{CharacterClass, RepeatRange, decode_repeat_range};
    for (pattern, expected) in [
        (
            "{2}",
            RepeatRange::Bounds {
                minimum: 2,
                maximum: Some(2),
                end: 3,
            },
        ),
        (
            "{ 2 , 4 }Z",
            RepeatRange::Bounds {
                minimum: 2,
                maximum: Some(4),
                end: 9,
            },
        ),
        (
            "{0,}",
            RepeatRange::Bounds {
                minimum: 0,
                maximum: None,
                end: 4,
            },
        ),
        (
            "{0,922337203685477581}",
            RepeatRange::Bounds {
                minimum: 0,
                maximum: None,
                end: 22,
            },
        ),
        ("{", RepeatRange::Literal { end: 1 }),
        ("{2", RepeatRange::Literal { end: 1 }),
        ("{x}", RepeatRange::Literal { end: 1 }),
        ("{2,x}", RepeatRange::Literal { end: 1 }),
        ("{2,", RepeatRange::Literal { end: 1 }),
    ] {
        let result =
            decode_repeat_range(
                Utf16View::new(pattern).units(),
                0,
                |unit, class| match class {
                    CharacterClass::Space => matches!(unit, 9..=13 | 32),
                    CharacterClass::Word => (u16::from(b'0')..=u16::from(b'9')).contains(&unit),
                    _ => unreachable!(),
                },
            );
        assert_eq!(result, Ok(expected), "{pattern}");
    }
}

#[test]
fn utf8_pattern_entry_keeps_the_source_c_string_boundary_distinct_from_decoded_nul() {
    use notepad_matrix::search_regex::{EscapeErrorCode, RegexEscapeError, RegexProgram};
    let document = Utf16View::new("a\0b");
    let truncated = RegexProgram::compile_utf8("a\0[").unwrap();
    assert_eq!(truncated.match_at(&document, 0).unwrap(), Some(0..1));
    let escaped = RegexProgram::compile_utf8(r"a\0b").unwrap();
    assert_eq!(escaped.match_at(&document, 0).unwrap(), Some(0..3));
    let empty = RegexProgram::compile_utf8("\0*").unwrap();
    assert_eq!(empty.match_at(&document, 0).unwrap(), Some(0..0));
    let quoted = RegexProgram::compile_utf8("\\Qab\0[\\E").unwrap();
    assert_eq!(
        quoted.match_at(&Utf16View::new("ab"), 0).unwrap(),
        Some(0..2)
    );
    assert_eq!(
        RegexProgram::compile_utf8("😀\\xZ\0ignored"),
        Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::Escape,
            position: 2,
            message: "Escape sequence did not encode a valid character."
        })
    );
}

#[test]
fn inline_comments_preserve_source_eof_acceptance_and_repeat_target_without_grouping() {
    use notepad_matrix::search_regex::{EscapeErrorCode, RegexEscapeError, RegexProgram};
    for (pattern, text, expected) in [
        ("a(?#ignored)b", "ab", Some(0..2)),
        ("a(?#ignored)*", "aaa", Some(0..3)),
        ("a(?#unterminated", "a", Some(0..1)),
        (r"(?#x\)b", "b", Some(0..1)),
        ("a*+(?#ignored)b", "aaab", Some(0..4)),
        ("a*+(?#unterminated", "aaa", Some(0..3)),
        ("(?#comment)", "", Some(0..0)),
    ] {
        assert_eq!(
            RegexProgram::compile_utf16(Utf16View::new(pattern).units())
                .unwrap()
                .match_at(&Utf16View::new(text), 0)
                .unwrap(),
            expected,
            "{pattern}"
        );
    }
    assert_eq!(
        RegexProgram::compile_utf16(Utf16View::new("a*+(?#x)*").units()),
        Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::BadRepeat,
            position: 8,
            message: "Invalid preceding regular expression prior to repetition operator."
        })
    );
}

#[test]
fn simple_repeat_errors_keep_original_modifier_consumption_and_assertion_rejection_positions() {
    use notepad_matrix::search_regex::{EscapeErrorCode, RegexEscapeError, RegexProgram};
    for (pattern, position, message) in [
        (
            "*a",
            0,
            "The repeat operator \"*\" cannot start a regular expression.",
        ),
        (
            "+a",
            0,
            "The repeat operator \"+\" cannot start a regular expression.",
        ),
        (
            "?a",
            0,
            "The repeat operator \"?\" cannot start a regular expression.",
        ),
        (
            "a**",
            3,
            "Invalid preceding regular expression prior to repetition operator.",
        ),
        (
            "a*??",
            4,
            "Invalid preceding regular expression prior to repetition operator.",
        ),
        (
            "a*+?",
            3,
            "Invalid preceding regular expression prior to repetition operator.",
        ),
        (
            "^*",
            2,
            "Invalid preceding regular expression prior to repetition operator.",
        ),
        (
            r"\A+",
            3,
            "Invalid preceding regular expression prior to repetition operator.",
        ),
        (
            r"\Q\E*",
            5,
            "Invalid preceding regular expression prior to repetition operator.",
        ),
    ] {
        assert_eq!(
            RegexProgram::compile_utf16(Utf16View::new(pattern).units()),
            Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::BadRepeat,
                position,
                message
            }),
            "{pattern}"
        );
    }
    for (pattern, expected) in [("a*a$", Some(0..3)), ("a*?a$", Some(0..3)), ("a*+a$", None)] {
        assert_eq!(
            RegexProgram::compile_utf16(Utf16View::new(pattern).units())
                .unwrap()
                .match_at(&Utf16View::new("aaa\n"), 0)
                .unwrap(),
            expected
        );
    }
}

#[test]
fn repeat_matching_reports_explicit_complexity_exhaustion_instead_of_not_found() {
    use notepad_matrix::search_regex::{RegexMatchError, RegexProgram};
    let program = RegexProgram::compile_utf16(Utf16View::new("a*a*a*b").units()).unwrap();
    let document = Utf16View::new("aaaaaaaaaaaaaaaaaaaa");
    assert_eq!(
        program.match_at_with_state_limit(&document, 0, false, 10),
        Err(RegexMatchError::Complexity)
    );
    assert_eq!(
        program.match_at_with_state_limit(&document, 0, false, 100_000),
        Ok(None)
    );
    assert_eq!(
        program.match_at_with_state_limit(&Utf16View::new("aaab"), 0, false, 100),
        Ok(Some(0..4))
    );
    let empty = RegexProgram::compile_utf16(&[]).unwrap();
    assert_eq!(
        empty.match_at_with_state_limit(&document, 0, false, 0),
        Ok(Some(0..0))
    );
}

#[test]
fn complexity_ceiling_is_checked_after_failure_not_on_an_otherwise_successful_path() {
    use notepad_matrix::search_regex::{RegexMatchError, RegexProgram};
    let program = RegexProgram::compile_utf16(Utf16View::new("a").units()).unwrap();
    assert_eq!(
        program.match_at_with_state_limit(&Utf16View::new("a"), 0, false, 0),
        Ok(Some(0..1))
    );
    assert_eq!(
        program.match_at_with_state_limit(&Utf16View::new("b"), 0, false, 0),
        Err(RegexMatchError::Complexity)
    );
    let repeat = RegexProgram::compile_utf16(Utf16View::new("a*b").units()).unwrap();
    assert_eq!(
        repeat.match_at_with_state_limit(&Utf16View::new("aaab"), 0, false, 0),
        Ok(Some(0..4))
    );
}

#[test]
fn compiled_simple_repeats_keep_source_greedy_lazy_possessive_and_flat_quote_binding() {
    use notepad_matrix::search_regex::RegexProgram;
    for (pattern, text, expected) in [
        ("a*", "aaa", Some(0..3)),
        ("a*?", "aaa", Some(0..0)),
        ("a*+", "aaa", Some(0..3)),
        ("a*?+", "aaa", Some(0..0)),
        ("a*a", "aaa", Some(0..3)),
        ("a*?a", "aaa", Some(0..1)),
        ("a*+a", "aaa", None),
        ("a*?+a", "aaa", Some(0..1)),
        ("a+", "", None),
        ("a+?a", "aaa", Some(0..2)),
        ("aa?", "aaa", Some(0..2)),
        ("aa??", "aaa", Some(0..1)),
        ("a?+a", "a", None),
        (r"\Qab\E+", "abbb", Some(0..4)),
        (".*b", "aab", Some(0..3)),
        (".*+b", "aab", None),
        ("a*ab*bc", "aaabbc", Some(0..6)),
    ] {
        let program = RegexProgram::compile_utf16(Utf16View::new(pattern).units()).unwrap();
        assert_eq!(
            program.match_at(&Utf16View::new(text), 0).unwrap(),
            expected,
            "{pattern} in {text}"
        );
    }
}

#[test]
fn compiled_default_anchors_use_source_crlf_edges_and_all_trailing_separators_for_soft_end() {
    use notepad_matrix::search_regex::RegexProgram;
    let document = Utf16View::new("a\r\nb\n");
    for (pattern, positions) in [
        ("^", vec![0, 3, 5]),
        ("$", vec![1, 4, 5]),
        (r"\A", vec![0]),
        (r"\z", vec![5]),
        (r"\Z", vec![4, 5]),
    ] {
        let program = RegexProgram::compile_utf16(Utf16View::new(pattern).units()).unwrap();
        let matches: Vec<_> = (0..=5)
            .filter(|&at| program.match_at(&document, at).unwrap().is_some())
            .collect();
        assert_eq!(matches, positions, "{pattern}");
    }
    let soft = RegexProgram::compile_utf16(Utf16View::new(r"b\Z").units()).unwrap();
    let hard = RegexProgram::compile_utf16(Utf16View::new(r"b\z").units()).unwrap();
    let trailing = Utf16View::new("b\r\n\u{c}\u{85}\u{2028}\u{2029}");
    assert_eq!(soft.match_at(&trailing, 0).unwrap(), Some(0..1));
    assert_eq!(hard.match_at(&trailing, 0).unwrap(), None);
    assert_eq!(soft.match_at(&Utf16View::new("b\n\u{b}"), 0).unwrap(), None);
    let whole = RegexProgram::compile_utf16(Utf16View::new(r"\Aabc\z").units()).unwrap();
    assert_eq!(
        whole.match_at(&Utf16View::new("abc"), 0).unwrap(),
        Some(0..3)
    );
    assert_eq!(whole.match_at(&Utf16View::new("xabc"), 1).unwrap(), None);
    assert_eq!(whole.match_at(&Utf16View::new("abcx"), 0).unwrap(), None);
    for pattern in ["^$", r"\A\z", r"\A\Z"] {
        assert_eq!(
            RegexProgram::compile_utf16(Utf16View::new(pattern).units())
                .unwrap()
                .match_at(&Utf16View::new(""), 0)
                .unwrap(),
            Some(0..0)
        );
    }
}

#[test]
fn compiled_dot_and_capital_c_match_one_wide_unit_with_source_line_separator_flags() {
    use notepad_matrix::search_regex::RegexProgram;
    let document = Utf16View::new("\n\r\u{c}\u{85}\u{2028}\u{2029}\u{b}\0😀");
    for pattern in [".", r"\C"] {
        let program = RegexProgram::compile_utf16(Utf16View::new(pattern).units()).unwrap();
        for at in 0..6 {
            assert_eq!(
                program.match_at(&document, at).unwrap(),
                None,
                "{pattern} separator {at}"
            );
            assert_eq!(
                program
                    .match_at_with_dot_newline(&document, at, true)
                    .unwrap(),
                Some(at..at + 1)
            );
        }
        for at in 6..10 {
            assert_eq!(
                program.match_at(&document, at).unwrap(),
                Some(at..at + 1),
                "{pattern} wide unit {at}"
            );
        }
        assert_eq!(program.match_at(&document, 10).unwrap(), None);
    }
    let both = RegexProgram::compile_utf16(Utf16View::new("..Z").units()).unwrap();
    let text = Utf16View::new("😀Z");
    assert_eq!(both.match_at(&text, 0).unwrap(), Some(0..3));
    assert_eq!(
        text.byte_range(both.match_at(&text, 0).unwrap().unwrap()),
        Some(ByteOffset(0)..ByteOffset(5))
    );
}

#[test]
fn literal_program_preserves_empty_expression_and_reports_unimplemented_syntax_instead_of_nonmatches()
 {
    use notepad_matrix::search_regex::{EscapeErrorCode, RegexEscapeError, RegexProgram};
    let document = Utf16View::new(" ]}#\n");
    let plain = RegexProgram::compile_utf16(Utf16View::new(" ]}#\n").units()).unwrap();
    assert_eq!(plain.match_at(&document, 0).unwrap(), Some(0..5));
    let empty = RegexProgram::compile_utf16(&[]).unwrap();
    assert_eq!(empty.match_at(&document, 0).unwrap(), Some(0..0));
    assert_eq!(empty.match_at(&document, 5).unwrap(), Some(5..5));
    assert_eq!(empty.match_at(&document, 6).unwrap(), None);
    assert_eq!(empty.match_at(&document, usize::MAX).unwrap(), None);
    for (pattern, position, symbol) in [("a{2}", 1, b'{'), ("a(b)", 1, b'('), (r"a\d", 1, b'\\')] {
        assert_eq!(
            RegexProgram::compile_utf16(Utf16View::new(pattern).units()),
            Err(RegexEscapeError::Unsupported {
                position,
                symbol: u16::from(symbol)
            }),
            "{pattern}"
        );
    }
    assert_eq!(
        RegexProgram::compile_utf16(Utf16View::new(r"a\xZ").units()),
        Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::Escape,
            position: 1,
            message: "Escape sequence did not encode a valid character."
        })
    );
}

#[test]
fn compiled_literal_regex_matches_quoted_escaped_and_surrogate_units_in_the_original_document() {
    use notepad_matrix::search_regex::RegexProgram;
    let pattern = Utf16View::new(r"\Q.a\E\x{D83D}\x{DE00}\0");
    let program = RegexProgram::compile_utf16(pattern.units()).expect("source literal grammar");
    let document = Utf16View::new("prefix .a😀\0 tail");
    assert_eq!(program.match_at(&document, 7).unwrap(), Some(7..12));
    assert_eq!(
        document.byte_range(program.match_at(&document, 7).unwrap().unwrap()),
        Some(ByteOffset(7)..ByteOffset(14))
    );
    assert_eq!(program.match_at(&document, 0).unwrap(), None);
    assert_eq!(program.match_at(&document, 12).unwrap(), None);
    let high = RegexProgram::compile_utf16(Utf16View::new(r"\x{D83D}").units()).unwrap();
    assert_eq!(high.match_at(&document, 9).unwrap(), Some(9..10));
    assert_eq!(
        document.byte_range(high.match_at(&document, 9).unwrap().unwrap()),
        Some(ByteOffset(9)..ByteOffset(9))
    );
}

#[test]
fn root_dispatch_uses_explicit_class_traits_without_assuming_the_host_locale() {
    use notepad_matrix::search_regex::{
        CharacterClass, RegexEscapeError, RootEscape, decode_root_escape,
        decode_root_escape_with_class_lookup, lookup_default_character_class,
    };
    assert_eq!(
        decode_root_escape_with_class_lookup(
            Utf16View::new(r"\p{digit}").units(),
            0,
            lookup_default_character_class
        ),
        Ok((
            RootEscape::Class {
                class: CharacterClass::Digit,
                negative: false
            },
            9
        ))
    );
    assert_eq!(
        decode_root_escape_with_class_lookup(
            Utf16View::new(r"\Qabc\E").units(),
            0,
            |_| unreachable!()
        ),
        Ok((RootEscape::LiteralRun(vec![97, 98, 99]), 7))
    );
    assert_eq!(
        decode_root_escape(Utf16View::new(r"\p{digit}").units(), 0),
        Err(RegexEscapeError::Unsupported {
            position: 0,
            symbol: u16::from(b'p')
        })
    );
    assert_eq!(
        decode_root_escape_with_class_lookup(&[], usize::MAX, |_| unreachable!()),
        Err(RegexEscapeError::InvalidPosition(usize::MAX))
    );
}

#[test]
fn property_escape_retains_source_error_offsets_and_does_not_invent_unicode_aliases() {
    use notepad_matrix::search_regex::{
        EscapeErrorCode, RegexEscapeError, decode_property_escape, lookup_default_character_class,
    };
    for (pattern, slash, code, position, message) in [
        (
            r"\p",
            0,
            EscapeErrorCode::Escape,
            2,
            "Incomplete property escape found.",
        ),
        (
            r"\P{",
            0,
            EscapeErrorCode::Escape,
            3,
            "Closing } missing from property escape sequence.",
        ),
        (
            r"😀\p{digit",
            2,
            EscapeErrorCode::Escape,
            10,
            "Closing } missing from property escape sequence.",
        ),
        (
            r"\p{}",
            0,
            EscapeErrorCode::Ctype,
            4,
            "Escape sequence was neither a valid property nor a valid character class name.",
        ),
        (
            r"\p{Letter}",
            0,
            EscapeErrorCode::Ctype,
            10,
            "Escape sequence was neither a valid property nor a valid character class name.",
        ),
        (
            r"\p?Z",
            0,
            EscapeErrorCode::Ctype,
            3,
            "Escape sequence was neither a valid property nor a valid character class name.",
        ),
    ] {
        assert_eq!(
            decode_property_escape(
                Utf16View::new(pattern).units(),
                slash,
                lookup_default_character_class
            ),
            Err(RegexEscapeError::Syntax {
                code,
                position,
                message
            }),
            "{pattern}"
        );
    }
    for (pattern, slash) in [("", usize::MAX), (r"\", 0), (r"\d", 0), (r"p{digit}", 0)] {
        assert_eq!(
            decode_property_escape(Utf16View::new(pattern).units(), slash, |_| unreachable!()),
            Err(RegexEscapeError::InvalidPosition(slash))
        );
    }
    assert_eq!(
        decode_property_escape(Utf16View::new(r"\p{").units(), 0, |_| unreachable!()),
        Err(RegexEscapeError::Syntax {
            code: EscapeErrorCode::Escape,
            position: 3,
            message: "Closing } missing from property escape sequence."
        })
    );
}

#[test]
fn property_escape_passes_exact_wide_names_to_traits_and_consumes_one_unit_or_braces() {
    use notepad_matrix::search_regex::{
        CharacterClass, RootEscape, decode_property_escape, lookup_default_character_class,
    };
    for (pattern, slash, name, class, negative, end) in [
        (r"\pdZ", 0, "d", CharacterClass::Digit, false, 3),
        (r"\P{word}Z", 0, "word", CharacterClass::Word, true, 8),
        (r"\pL", 0, "L", CharacterClass::Lower, false, 3),
        (
            r"😀\p{unicode}Z",
            2,
            "unicode",
            CharacterClass::Unicode,
            false,
            13,
        ),
    ] {
        let view = Utf16View::new(pattern);
        let result = decode_property_escape(view.units(), slash, |units| {
            assert_eq!(units, Utf16View::new(name).units());
            let folded: Vec<_> = units
                .iter()
                .map(|&unit| {
                    if unit == u16::from(b'L') {
                        u16::from(b'l')
                    } else {
                        unit
                    }
                })
                .collect();
            lookup_default_character_class(&folded)
        });
        assert_eq!(
            result,
            Ok((RootEscape::Class { class, negative }, end)),
            "{pattern}"
        );
    }
    assert_eq!(
        decode_property_escape(&[92, 112, 0xd800, 65], 0, |name| {
            assert_eq!(name, [0xd800]);
            Some(CharacterClass::Alpha)
        }),
        Ok((
            RootEscape::Class {
                class: CharacterClass::Alpha,
                negative: false
            },
            3
        ))
    );
}

#[test]
fn default_class_names_use_the_frozen_w32_alias_catalog_not_unicode_property_names() {
    use notepad_matrix::search_regex::{CharacterClass, lookup_default_character_class};
    let aliases = [
        ("alnum", CharacterClass::Alnum),
        ("alpha", CharacterClass::Alpha),
        ("blank", CharacterClass::Blank),
        ("cntrl", CharacterClass::Control),
        ("d", CharacterClass::Digit),
        ("digit", CharacterClass::Digit),
        ("graph", CharacterClass::Graph),
        ("h", CharacterClass::Horizontal),
        ("l", CharacterClass::Lower),
        ("lower", CharacterClass::Lower),
        ("print", CharacterClass::Print),
        ("punct", CharacterClass::Punct),
        ("s", CharacterClass::Space),
        ("space", CharacterClass::Space),
        ("u", CharacterClass::Upper),
        ("unicode", CharacterClass::Unicode),
        ("upper", CharacterClass::Upper),
        ("v", CharacterClass::Vertical),
        ("w", CharacterClass::Word),
        ("word", CharacterClass::Word),
        ("xdigit", CharacterClass::HexDigit),
    ];
    for (name, class) in aliases {
        assert_eq!(
            lookup_default_character_class(Utf16View::new(name).units()),
            Some(class),
            "{name}"
        );
    }
    for name in ["L", "Letter", "Lu", "ASCII", "Digit", "", "ſpace"] {
        assert_eq!(
            lookup_default_character_class(Utf16View::new(name).units()),
            None,
            "{name}"
        );
    }
}

#[test]
fn root_quoted_literals_keep_source_end_rules_and_do_not_decode_inner_escapes() {
    use notepad_matrix::search_regex::{
        EscapeErrorCode, RegexEscapeError, RootEscape, decode_root_escape,
    };
    for (pattern, literals, end) in [
        (r"\Q", vec![], 2),
        (r"\Q\E", vec![], 4),
        (r"\Qab\EZ", vec![97, 98], 6),
        (r"\Qa\z", vec![97, 92, 122], 5),
        (r"\Qabc\\E", vec![97, 98, 99, 92], 8),
        (r"\Q.a*[x]\E", vec![46, 97, 42, 91, 120, 93], 10),
    ] {
        assert_eq!(
            decode_root_escape(Utf16View::new(pattern).units(), 0),
            Ok((RootEscape::LiteralRun(literals), end)),
            "{pattern}"
        );
    }
    for (pattern, slash, position) in [("\\Qabc\\", 0, 6), ("😀\\Qabc\\", 2, 8)] {
        assert_eq!(
            decode_root_escape(Utf16View::new(pattern).units(), slash),
            Err(RegexEscapeError::Syntax {
                code: EscapeErrorCode::Escape,
                position,
                message: "Unterminated \\Q...\\E sequence."
            })
        );
    }
}

#[test]
fn root_escape_dispatch_keeps_boost_classes_assertions_and_wide_unit_atoms_separate_from_literals()
{
    use notepad_matrix::search_regex::{
        CharacterClass, RegexAssertion, RootEscape, decode_root_escape,
    };
    for (pattern, class, negative) in [
        (r"\d", CharacterClass::Digit, false),
        (r"\D", CharacterClass::Digit, true),
        (r"\s", CharacterClass::Space, false),
        (r"\W", CharacterClass::Word, true),
        (r"\h", CharacterClass::Horizontal, false),
        (r"\V", CharacterClass::Vertical, true),
        (r"\v", CharacterClass::Vertical, false),
        (r"\l", CharacterClass::Lower, false),
        (r"\u0041", CharacterClass::Upper, false),
        (r"\U", CharacterClass::Upper, true),
    ] {
        assert_eq!(
            decode_root_escape(Utf16View::new(pattern).units(), 0),
            Ok((RootEscape::Class { class, negative }, 2)),
            "{pattern}"
        );
    }
    for (pattern, assertion) in [
        (r"\b", RegexAssertion::WordBoundary),
        (r"\B", RegexAssertion::WithinWord),
        (r"\<", RegexAssertion::WordStart),
        (r"\>", RegexAssertion::WordEnd),
        (r"\A", RegexAssertion::BufferStart),
        (r"\`", RegexAssertion::BufferStart),
        (r"\z", RegexAssertion::BufferEnd),
        (r"\'", RegexAssertion::BufferEnd),
        (r"\Z", RegexAssertion::SoftBufferEnd),
        (r"\G", RegexAssertion::RestartContinuation),
    ] {
        assert_eq!(
            decode_root_escape(Utf16View::new(pattern).units(), 0),
            Ok((RootEscape::Assertion(assertion), 2)),
            "{pattern}"
        );
    }
    for (pattern, atom, end) in [
        (r"\C", RootEscape::Any, 2),
        (r"\K", RootEscape::ResetMatchStart, 2),
        (r"\X", RootEscape::Combining, 2),
        (r"\R", RootEscape::LineEnding, 2),
        (r"\q", RootEscape::Literal(113), 2),
        (r"\E", RootEscape::Literal(69), 2),
        (r"\I", RootEscape::Literal(73), 2),
        (r"\x4", RootEscape::Literal(4), 3),
        (r"\N{NUL}", RootEscape::Literal(0), 7),
    ] {
        assert_eq!(
            decode_root_escape(Utf16View::new(pattern).units(), 0),
            Ok((atom, end)),
            "{pattern}"
        );
    }
}

#[test]
fn literal_named_collation_escape_preserves_distinct_source_errors_and_nearest_escape_rewind() {
    use notepad_matrix::search_regex::{
        EscapeErrorCode, LiteralEscapeError, decode_literal_escape,
    };
    for (pattern, code, position) in [
        (r"\N", EscapeErrorCode::Escape, 0),
        (r"\Nx", EscapeErrorCode::Escape, 0),
        (r"\N{", EscapeErrorCode::Escape, 0),
        (r"\N{ae}", EscapeErrorCode::Escape, 0),
        (r"\N{}", EscapeErrorCode::Collate, 0),
        (r"\N{nul}", EscapeErrorCode::Collate, 0),
        (r"\N{LATIN CAPITAL LETTER A}", EscapeErrorCode::Collate, 0),
        (r"\N{😀}", EscapeErrorCode::Collate, 0),
        (r"\N{bad\name}", EscapeErrorCode::Collate, 6),
        (r"\N{bad\name", EscapeErrorCode::Escape, 6),
    ] {
        let message = if code == EscapeErrorCode::Collate {
            "Invalid collation character."
        } else {
            "Invalid or unterminated escape sequence."
        };
        assert_eq!(
            decode_literal_escape(Utf16View::new(pattern).units(), 0),
            Err(LiteralEscapeError::Syntax {
                code,
                position,
                message
            }),
            "{pattern}"
        );
    }
    assert_eq!(
        decode_literal_escape(&[92, 78, 123, 0xd800, 125], 0),
        Ok((0xd800, 5))
    );
}

#[test]
fn literal_named_collation_escape_keeps_source_aliases_narrowing_and_single_unit_fallback() {
    use notepad_matrix::search_regex::decode_literal_escape;
    for (pattern, expected) in [
        (r"\N{NUL}", (0, 7)),
        (r"\N{newline}", (10, 11)),
        (r"\N{one}", (49, 7)),
        (r"\N{backslash}", (92, 13)),
        (r"\N{DEL}", (127, 7)),
        (r"\N{é}", (0xe9, 5)),
        (r"\N{Ł}", (65, 5)),
        (r"\N{ŎŕŌ}", (0, 7)),
    ] {
        assert_eq!(
            decode_literal_escape(Utf16View::new(pattern).units(), 0),
            Ok(expected),
            "{pattern}"
        );
    }
}

#[test]
fn literal_octal_escape_primitive_retains_leading_zero_four_digit_limit_and_utf16_error_positions()
{
    use notepad_matrix::search_regex::{
        EscapeErrorCode, LiteralEscapeError, decode_literal_escape,
    };
    for (pattern, slash, expected) in [
        (r"\0", 0, (0, 2)),
        (r"\07", 0, (7, 3)),
        (r"\0777", 0, (511, 5)),
        (r"\01234", 0, (83, 5)),
        (r"\08", 0, (0, 2)),
        (r"zz\0000", 2, (0, 7)),
    ] {
        assert_eq!(
            decode_literal_escape(Utf16View::new(pattern).units(), slash),
            Ok(expected),
            "{pattern}"
        );
    }
    for (pattern, slash, position, message) in [
        (r"\12", 0, 0, "Invalid octal escape sequence."),
        ("\\", 0, 1, "Escape sequence terminated prematurely."),
        (r"\c", 0, 0, "ASCII escape sequence terminated prematurely."),
        (
            r"😀\xZ",
            2,
            2,
            "Escape sequence did not encode a valid character.",
        ),
    ] {
        assert_eq!(
            decode_literal_escape(Utf16View::new(pattern).units(), slash),
            Err(LiteralEscapeError::Syntax {
                code: EscapeErrorCode::Escape,
                position,
                message
            }),
            "{pattern}"
        );
    }
    assert_eq!(
        decode_literal_escape(&[], usize::MAX),
        Err(LiteralEscapeError::InvalidPosition(usize::MAX))
    );
}

#[test]
fn literal_hex_escape_primitive_keeps_source_variable_width_and_u16_rejection_rules() {
    use notepad_matrix::search_regex::{
        EscapeErrorCode, LiteralEscapeError, decode_literal_escape,
    };
    for (pattern, expected) in [
        (r"\x4Z", (4, 3)),
        (r"\x41", (65, 4)),
        (r"\x{041}", (65, 7)),
        (r"\x{FFFF}", (65535, 8)),
        (r"\x{D800}", (0xd800, 8)),
        (r"\x0q", (0, 3)),
    ] {
        assert_eq!(
            decode_literal_escape(Utf16View::new(pattern).units(), 0),
            Ok(expected),
            "{pattern}"
        );
    }
    for (pattern, code, message) in [
        (
            r"\x",
            EscapeErrorCode::Escape,
            "Hexadecimal escape sequence terminated prematurely.",
        ),
        (
            r"\x{",
            EscapeErrorCode::Escape,
            "Missing } in hexadecimal escape sequence.",
        ),
        (
            r"\x{}",
            EscapeErrorCode::BadBrace,
            "Hexadecimal escape sequence was invalid.",
        ),
        (
            r"\x{41",
            EscapeErrorCode::BadBrace,
            "Hexadecimal escape sequence was invalid.",
        ),
        (
            r"\x{10000}",
            EscapeErrorCode::BadBrace,
            "Hexadecimal escape sequence was invalid.",
        ),
        (
            r"\xZ",
            EscapeErrorCode::Escape,
            "Escape sequence did not encode a valid character.",
        ),
    ] {
        assert_eq!(
            decode_literal_escape(Utf16View::new(pattern).units(), 0),
            Err(LiteralEscapeError::Syntax {
                code,
                position: 0,
                message
            }),
            "{pattern}"
        );
    }
}

#[test]
fn literal_escape_primitive_keeps_source_controls_unknown_literals_and_control_modulo() {
    use notepad_matrix::search_regex::decode_literal_escape;
    for (pattern, unit, end) in [
        (r"\a", 7, 2),
        (r"\e", 27, 2),
        (r"\f", 12, 2),
        (r"\n", 10, 2),
        (r"\r", 13, 2),
        (r"\t", 9, 2),
        (r"\v", 11, 2),
        (r"\b", 8, 2),
        (r"\q", 113, 2),
        (r"\.", 46, 2),
        (r"\\", 92, 2),
        (r"\cA", 1, 3),
        (r"\c?", 31, 3),
        (r"\c1", 17, 3),
    ] {
        let view = Utf16View::new(pattern);
        assert_eq!(
            decode_literal_escape(view.units(), 0),
            Ok((unit, end)),
            "{pattern}"
        );
    }
}

#[test]
fn regex_view_retains_original_utf16_units_and_source_iterator_byte_positions() {
    let view = Utf16View::new("Aé😀\r\n\0Z");
    assert_eq!(
        view.units(),
        [0x41, 0xe9, 0xd83d, 0xde00, 0x0d, 0x0a, 0, 0x5a]
    );
    let positions: Vec<_> = (0..=view.units().len())
        .map(|index| view.byte_position(index))
        .collect();
    assert_eq!(
        positions,
        [0, 1, 3, 3, 7, 8, 9, 10, 11].map(|byte| Some(ByteOffset(byte)))
    );
    assert_eq!(view.byte_range(2..3), Some(ByteOffset(3)..ByteOffset(3)));
    assert_eq!(view.byte_range(3..4), Some(ByteOffset(3)..ByteOffset(7)));
    assert_eq!(view.byte_range(2..4), Some(ByteOffset(3)..ByteOffset(7)));
    assert_eq!(view.byte_range(6..7), Some(ByteOffset(9)..ByteOffset(10)));
    assert_eq!(view.byte_range(0..9), None);
    assert_eq!(view.byte_range(std::ops::Range { start: 4, end: 2 }), None);
}

#[test]
fn regex_range_endpoints_move_forward_outside_utf8_characters_without_entering_a_surrogate_pair() {
    let view = Utf16View::new("Aé😀\r\n\0Z");
    assert_eq!(
        (0..=11)
            .map(|byte| view.unit_at_or_after_byte(ByteOffset(byte)))
            .collect::<Vec<_>>(),
        [0, 1, 2, 2, 4, 4, 4, 4, 5, 6, 7, 8].map(Some)
    );
    assert_eq!(view.unit_at_or_after_byte(ByteOffset(12)), None);
    let empty = Utf16View::new("");
    assert!(empty.units().is_empty());
    assert_eq!(empty.byte_position(0), Some(ByteOffset(0)));
    assert_eq!(empty.unit_at_or_after_byte(ByteOffset(0)), Some(0));
}
