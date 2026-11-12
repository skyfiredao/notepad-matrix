use std::sync::Arc;

use notepad_matrix::editor::{ByteOffset, Selection};
use notepad_matrix::finder::{Document, search_documents};
use notepad_matrix::search::LiteralOptions;

#[test]
fn find_within_progress_records_completed_candidates_and_accepted_hits()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{ResultSearchProgress, Results};
    use std::sync::atomic::AtomicBool;
    let mut source = Results::default();
    source.push(
        search_documents(
            "old",
            (0..3)
                .map(|index| Document {
                    name: format!("/file{index}"),
                    text: Arc::from("old"),
                    selection: None,
                })
                .collect(),
            LiteralOptions::default(),
            true,
        )?,
        false,
    );
    let progress = ResultSearchProgress::new(Arc::new(AtomicBool::new(false)));
    let filter = source.line_filter();
    let result = filter.search_with_progress(
        "new",
        LiteralOptions::default(),
        false,
        true,
        &progress,
        |path| match path {
            "/file0" => {
                assert_eq!(progress.snapshot(), (0, 0));
                Some(Document {
                    name: path.to_owned(),
                    text: Arc::from("new new"),
                    selection: None,
                })
            }
            "/file1" => {
                assert_eq!(progress.snapshot(), (1, 2));
                None
            }
            "/file2" => {
                assert_eq!(progress.snapshot(), (2, 2));
                assert_eq!(progress.percent(3), 33);
                Some(Document {
                    name: path.to_owned(),
                    text: Arc::from("new"),
                    selection: None,
                })
            }
            _ => unreachable!(),
        },
    )?;
    assert_eq!(progress.snapshot(), (3, 3));
    assert_eq!(
        progress.percent(3),
        66,
        "source reports the current zero-based file index, not an invented final100% update"
    );
    assert_eq!(result.hits, 3);
    assert_eq!(result.searched, 3);
    Ok(())
}

#[test]
fn find_within_cancellation_keeps_completed_results_and_the_full_source_candidate_count()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::Results;
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut source = Results::default();
    source.push(
        search_documents(
            "old",
            (0..3)
                .map(|index| Document {
                    name: format!("/file{index}"),
                    text: Arc::from("old"),
                    selection: None,
                })
                .collect(),
            LiteralOptions::default(),
            true,
        )?,
        false,
    );
    let filter = source.line_filter();
    let cancel = AtomicBool::new(false);
    let mut loaded = Vec::new();
    let found = filter.search_loaded(
        "new",
        LiteralOptions::default(),
        true,
        true,
        &cancel,
        |path| {
            loaded.push(path.to_owned());
            cancel.store(true, Ordering::Relaxed);
            Some(Document {
                name: path.to_owned(),
                text: Arc::from("new"),
                selection: None,
            })
        },
    )?;
    assert_eq!(loaded, ["/file0"]);
    assert_eq!(found.hits, 1);
    assert_eq!(found.searched, 3);
    assert_eq!(
        found.title,
        "Search \"new\" (1 hit in 1 file of 3 searched) [Normal] - Line Filter Mode: only display the filtered results"
    );
    let mut called = false;
    let empty = filter.search_loaded(
        "new",
        LiteralOptions::default(),
        false,
        true,
        &cancel,
        |_| {
            called = true;
            None
        },
    )?;
    assert!(!called);
    assert_eq!(empty.hits, 0);
    assert_eq!(empty.searched, 3);
    Ok(())
}

#[test]
fn find_within_retains_source_forward_metadata_scan_across_search_histories()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::Results;
    let mut source = Results::default();
    let original = "old\nnew\nnew\nold";
    source.push(
        search_documents(
            "old",
            vec![Document {
                name: "/one".into(),
                text: Arc::from(original),
                selection: None,
            }],
            LiteralOptions::default(),
            true,
        )?,
        false,
    );
    let earlier = source
        .rows
        .iter()
        .position(|row| *row == notepad_matrix::finder::Row::Line(0, 0, 1))
        .ok_or("last line")?;
    assert!(source.delete(earlier));
    source.push(
        search_documents(
            "old",
            vec![Document {
                name: "/one".into(),
                text: Arc::from("new\nnew\nnew\nold"),
                selection: None,
            }],
            LiteralOptions::default(),
            true,
        )?,
        false,
    );
    let current = || {
        vec![Document {
            name: "/one".into(),
            text: Arc::from("NEW\nNEW\nNEW\nNEW"),
            selection: None,
        }]
    };
    let result = source.search_within(
        r"\u006eew",
        current(),
        LiteralOptions {
            mode: notepad_matrix::search::SearchMode::Extended,
            ..Default::default()
        },
        true,
        true,
    )?;
    assert_eq!(
        result.hits, 1,
        "finding old line1 in the later history advances canFind past the earlier line4 record"
    );
    assert_eq!(result.documents[0].lines[0].number, 1);
    let all = source.search_within("new", current(), LiteralOptions::default(), false, true)?;
    assert_eq!(all.hits, 4);
    assert_eq!(source.source_pathnames(), ["/one"]);
    Ok(())
}

#[test]
fn find_within_results_searches_fresh_documents_and_filters_cached_line_numbers_not_old_ranges()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::Results;
    let mut source = Results::default();
    source.push(
        search_documents(
            "old",
            vec![
                Document {
                    name: "/one\nname".into(),
                    text: Arc::from("old\nother\nold"),
                    selection: None,
                },
                Document {
                    name: "/missing".into(),
                    text: Arc::from("old"),
                    selection: None,
                },
            ],
            LiteralOptions::default(),
            true,
        )?,
        false,
    );
    let documents = || {
        vec![
            Document {
                name: "/unrelated".into(),
                text: Arc::from("new"),
                selection: None,
            },
            Document {
                name: "/one\nname".into(),
                text: Arc::from("prefix new new\nnew\nnew"),
                selection: None,
            },
        ]
    };
    let filtered =
        source.search_within("new", documents(), LiteralOptions::default(), true, true)?;
    assert_eq!(
        filtered.title,
        "Search \"new\" (3 hits in 1 file of 2 searched) [Normal] - Line Filter Mode: only display the filtered results"
    );
    assert_eq!(filtered.documents[0].name, "/one\nname");
    assert_eq!(
        filtered.documents[0]
            .lines
            .iter()
            .map(|line| line.number)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(
        filtered.documents[0].lines[0].ranges,
        [
            ByteOffset(7)..ByteOffset(10),
            ByteOffset(11)..ByteOffset(14)
        ]
    );
    let unfiltered =
        source.search_within("new", documents(), LiteralOptions::default(), false, true)?;
    assert_eq!(unfiltered.hits, 4);
    assert_eq!(unfiltered.searched, 2);
    assert_eq!(source.searches[0].hits, 3);
    let empty = source.search_within("", documents(), LiteralOptions::default(), true, true)?;
    assert_eq!(empty.hits, 0);
    assert_eq!(empty.searched, 2);
    assert_eq!(
        empty.title,
        "Search \"\" (0 hits in 0 files of 2 searched) [Normal] - Line Filter Mode: only display the filtered results"
    );
    assert!(empty.documents.is_empty());
    Ok(())
}

#[test]
fn selected_result_pathnames_are_opaque_names_not_clipboard_lines()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let search = search_documents(
        "foo",
        vec![
            Document {
                name: "/one\nfile.txt".into(),
                text: Arc::from("foo\nfoo"),
                selection: None,
            },
            Document {
                name: "/two.txt".into(),
                text: Arc::from("foo"),
                selection: None,
            },
        ],
        LiteralOptions::default(),
        true,
    )?;
    let mut results = Results::default();
    results.push(search, false);
    assert_eq!(
        results.selected_pathnames(Row::Document(0, 0), Row::Line(0, 1, 0)),
        Some(vec!["/one\nfile.txt", "/two.txt"])
    );
    assert_eq!(
        results.selected_pathnames(Row::Search(0), Row::Search(0)),
        None
    );
    assert_eq!(
        results.selected_pathnames(Row::Document(0, 0), Row::Document(0, 0)),
        Some(vec!["/one\nfile.txt"])
    );
    Ok(())
}

#[test]
fn logical_delete_returns_the_next_physical_row_even_when_all_headers_are_folded()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row, TextPoint};
    let make = || {
        search_documents(
            "foo",
            vec![
                Document {
                    name: "/one".into(),
                    text: Arc::from("foo"),
                    selection: None,
                },
                Document {
                    name: "/two".into(),
                    text: Arc::from("foo\nfoo"),
                    selection: None,
                },
            ],
            LiteralOptions::default(),
            true,
        )
    };
    let mut results = Results::default();
    results.push(make()?, false);
    let mut search = make()?;
    search.errors.push("error".into());
    results.push(search, false);
    results.fold_all(true);
    for (row, next) in [
        (Row::Line(0, 0, 0), Some(Row::Document(0, 1))),
        (Row::Line(0, 1, 0), Some(Row::Line(0, 1, 0))),
        (Row::Document(0, 1), Some(Row::Error(0, 0))),
        (Row::Error(0, 0), Some(Row::Search(1))),
        (Row::Search(0), Some(Row::Search(0))),
        (Row::Search(0), None),
    ] {
        assert_eq!(
            results.delete_caret(row),
            Some(TextPoint { row: next, byte: 0 })
        );
    }
    assert!(results.searches.is_empty());
    assert_eq!(results.delete_caret(Row::Search(0)), None);
    Ok(())
}

#[test]
fn finder_fold_all_changes_every_header_and_clear_all_removes_only_result_history()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let make = || {
        search_documents(
            "foo",
            vec![Document {
                name: "/one".into(),
                text: Arc::from("foo\nfoo"),
                selection: None,
            }],
            LiteralOptions::default(),
            true,
        )
    };
    let mut results = Results::default();
    results.push(make()?, false);
    results.push(make()?, false);
    results.fold_all(true);
    assert_eq!(results.rows, vec![Row::Search(0), Row::Search(1)]);
    assert!(
        results
            .searches
            .iter()
            .all(|search| search.collapsed && search.documents.iter().all(|doc| doc.collapsed))
    );
    results.fold_all(false);
    assert_eq!(results.rows.len(), 8);
    assert!(
        results
            .searches
            .iter()
            .all(|search| !search.collapsed && search.documents.iter().all(|doc| !doc.collapsed))
    );
    assert_eq!(results.searches[0].hits, 2);
    results.clear();
    assert!(results.rows.is_empty());
    assert!(results.searches.is_empty());
    results.fold_all(false);
    results.clear();
    assert!(results.rows.is_empty());
    Ok(())
}

#[test]
fn finder_line_copy_truncates_each_nul_caption_before_dedup_without_losing_later_rows()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let search = search_documents(
        "foo",
        vec![
            Document {
                name: "/one".into(),
                text: Arc::from("foo\0tail A"),
                selection: None,
            },
            Document {
                name: "/two".into(),
                text: Arc::from("foo\0tail B"),
                selection: None,
            },
            Document {
                name: "/three".into(),
                text: Arc::from("next foo"),
                selection: None,
            },
        ],
        LiteralOptions::default(),
        true,
    )?;
    let mut results = Results::default();
    results.push(search, false);
    assert_eq!(
        results
            .copy_lines(Row::Search(0), Row::Search(0))
            .as_deref(),
        Some("foo\r\nnext foo\r\n")
    );
    Ok(())
}

#[test]
fn finder_verbatim_copy_uses_caption_bytes_and_excludes_a_line_at_selection_end()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row, TextPoint};
    let search = search_documents(
        "中",
        vec![Document {
            name: "/one".into(),
            text: Arc::from("中 foo\n中 bar"),
            selection: None,
        }],
        LiteralOptions::default(),
        true,
    )?;
    let mut results = Results::default();
    results.push(search, false);
    let anchor = TextPoint {
        row: Some(Row::Line(0, 0, 0)),
        byte: 9,
    };
    let head = TextPoint {
        row: Some(Row::Line(0, 0, 1)),
        byte: 0,
    };
    assert_eq!(
        results.copy_verbatim(anchor, head).as_deref(),
        Some("中 foo\r\n")
    );
    assert_eq!(
        results.copy_verbatim(head, anchor).as_deref(),
        Some("中 foo\r\n")
    );
    assert_eq!(
        results.selection_rows(anchor, head),
        Some((Row::Line(0, 0, 0), Row::Line(0, 0, 0)))
    );
    assert_eq!(results.copy_verbatim(anchor, anchor), None);
    let invalid = TextPoint { byte: 10, ..anchor };
    assert_eq!(results.copy_verbatim(anchor, invalid), None);
    let start = TextPoint {
        row: Some(Row::Search(0)),
        byte: 0,
    };
    let end = TextPoint { row: None, byte: 0 };
    let entire = results
        .copy_verbatim(start, end)
        .ok_or("entire result text")?;
    assert!(entire.starts_with("Search \"中\""));
    assert!(entire.ends_with("\tLine 2: 中 bar\r\n"));
    assert_eq!(
        results.selection_rows(start, end),
        Some((Row::Search(0), Row::Line(0, 0, 1)))
    );
    Ok(())
}

#[test]
fn copying_search_hits_ignores_error_rows_but_retains_source_empty_last_file_quirk()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let mut search = search_documents(
        "foo",
        vec![Document {
            name: "/one".into(),
            text: Arc::from("foo"),
            selection: None,
        }],
        LiteralOptions::default(),
        true,
    )?;
    search.errors.push("/unreadable: permission denied".into());
    let mut results = Results::default();
    results.push(search, false);
    results.toggle(0);
    assert_eq!(
        results
            .copy_lines(Row::Search(0), Row::Search(0))
            .as_deref(),
        Some("foo\r\n")
    );
    let search = search_documents(
        "foo",
        vec![
            Document {
                name: "/one".into(),
                text: Arc::from("foo"),
                selection: None,
            },
            Document {
                name: "/two".into(),
                text: Arc::from("foo"),
                selection: None,
            },
        ],
        LiteralOptions::default(),
        true,
    )?;
    results.push(search, false);
    let last = results
        .rows
        .iter()
        .position(|row| *row == Row::Line(0, 1, 0))
        .ok_or("last result")?;
    assert!(results.delete(last));
    assert_eq!(results.copy_lines(Row::Search(0), Row::Search(0)), None);
    Ok(())
}

#[test]
fn finder_copy_lines_expands_headers_and_preserves_source_duplicate_and_eol_rules()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let search = search_documents(
        "foo",
        vec![
            Document {
                name: "/one".into(),
                text: Arc::from("foo foo\nfoo"),
                selection: None,
            },
            Document {
                name: "/two".into(),
                text: Arc::from("foo foo\nother foo"),
                selection: None,
            },
        ],
        LiteralOptions::default(),
        false,
    )?;
    let mut results = Results::default();
    results.push(search, false);
    assert_eq!(
        results
            .copy_lines(Row::Document(0, 0), Row::Document(0, 0))
            .as_deref(),
        Some("foo foo\r\nfoo\r\n")
    );
    assert_eq!(
        results
            .copy_lines(Row::Search(0), Row::Search(0))
            .as_deref(),
        Some("foo foo\r\nfoo\r\nfoo foo\r\nother foo\r\n")
    );
    assert_eq!(
        results
            .copy_lines(Row::Line(0, 0, 1), Row::Line(0, 0, 0))
            .as_deref(),
        Some("foo foo\r\n")
    );
    results.toggle(1);
    assert_eq!(
        results
            .copy_lines(Row::Document(0, 0), Row::Document(0, 0))
            .as_deref(),
        Some("foo foo\r\nfoo\r\n")
    );
    assert_eq!(results.copy_pathnames(Row::Search(0), Row::Search(0)), None);
    assert_eq!(
        results
            .copy_pathnames(Row::Document(0, 0), Row::Document(0, 0))
            .as_deref(),
        Some("/one\r\n")
    );
    assert_eq!(
        results
            .copy_pathnames(Row::Search(0), Row::Document(0, 1))
            .as_deref(),
        Some("/one\r\n/two\r\n")
    );
    let empty = search_documents("missing", vec![], LiteralOptions::default(), true)?;
    results.push(empty, false);
    assert_eq!(results.copy_lines(Row::Search(0), Row::Search(0)), None);
    Ok(())
}

#[test]
fn deleting_result_rows_removes_subtrees_without_recounting_source_headers()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let search = search_documents(
        "foo",
        vec![Document {
            name: "new 1".into(),
            text: Arc::from("foo foo\nfoo"),
            selection: None,
        }],
        LiteralOptions::default(),
        true,
    )?;
    let mut results = Results::default();
    results.push(search, false);
    assert!(results.delete(2));
    assert_eq!(
        results.rows,
        vec![Row::Search(0), Row::Document(0, 0), Row::Line(0, 0, 0)]
    );
    assert_eq!(results.searches[0].documents[0].lines[0].number, 2);
    assert_eq!(results.searches[0].hits, 3);
    assert_eq!(results.searches[0].documents[0].hits, 3);
    assert_eq!(
        results.caption(0).as_deref(),
        Some("Search \"foo\" (3 hits in 1 file of 1 searched) [Normal]")
    );
    results.toggle(1);
    assert!(results.delete(1));
    assert!(results.searches[0].documents.is_empty());
    assert_eq!(results.rows, vec![Row::Search(0)]);
    assert!(results.delete(0));
    assert!(results.rows.is_empty());
    assert!(results.searches.is_empty());
    assert!(!results.delete(0));
    Ok(())
}

#[test]
fn cached_result_positions_follow_source_before_after_normalization() {
    use notepad_matrix::finder::normalize_cached_range;
    let unicode = ropey::Rope::from_str("中b");
    assert_eq!(
        normalize_cached_range(&unicode, ByteOffset(1)..ByteOffset(2)),
        ByteOffset(3)..ByteOffset(3)
    );
    assert_eq!(
        normalize_cached_range(&unicode, ByteOffset(0)..ByteOffset(3)),
        ByteOffset(0)..ByteOffset(3)
    );
    assert_eq!(
        normalize_cached_range(&unicode, ByteOffset(100)..ByteOffset(200)),
        ByteOffset(4)..ByteOffset(4)
    );
    let crlf = ropey::Rope::from_str("a\r\nb");
    assert_eq!(
        normalize_cached_range(&crlf, ByteOffset(1)..ByteOffset(2)),
        ByteOffset(1)..ByteOffset(3)
    );
    assert_eq!(
        normalize_cached_range(&crlf, ByteOffset(2)..ByteOffset(3)),
        ByteOffset(3)..ByteOffset(3)
    );
}

#[test]
fn finder_long_line_preview_is_source_bounded_and_does_not_retain_document_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let text: Arc<str> = Arc::from(format!("{} tail", "中".repeat(800)));
    let search = search_documents(
        "tail",
        vec![Document {
            name: "new 1".into(),
            text: text.clone(),
            selection: None,
        }],
        LiteralOptions {
            match_case: true,
            ..LiteralOptions::default()
        },
        true,
    )?;
    assert_eq!(Arc::strong_count(&text), 1);
    assert!(search.documents[0].lines[0].caption.ends_with(" ..."));
    assert!(search.documents[0].lines[0].caption.len() < 2048);
    assert_eq!(
        search.documents[0].lines[0].ranges,
        vec![ByteOffset(2401)..ByteOffset(2405)]
    );
    Ok(())
}

#[test]
fn finder_keeps_and_collapses_previous_searches_unless_source_purge_is_enabled()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Results, Row};
    let search = |query: &str| {
        search_documents(
            query,
            vec![Document {
                name: "new 1".into(),
                text: Arc::from("one two"),
                selection: None,
            }],
            LiteralOptions::default(),
            true,
        )
    };
    let mut results = Results::default();
    results.push(search("one")?, false);
    results.push(search("two")?, false);
    assert_eq!(results.searches.len(), 2);
    assert_eq!(results.rows.len(), 4);
    assert_eq!(results.rows[3], Row::Search(1));
    assert!(results.searches[1].collapsed);
    results.toggle(3);
    assert_eq!(results.rows.len(), 6);
    results.push(search("missing")?, true);
    assert_eq!(results.searches.len(), 1);
    assert_eq!(results.rows, vec![Row::Search(0)]);
    assert_eq!(
        results.searches[0].title,
        "Search \"missing\" (0 hits in 0 files of 1 searched) [Normal]"
    );
    Ok(())
}

#[test]
fn current_selection_find_all_uses_document_word_edges_and_source_scope_title()
-> Result<(), Box<dyn std::error::Error>> {
    let search = search_documents(
        "one",
        vec![Document {
            name: "new 1".into(),
            text: Arc::from("one one ONE"),
            selection: Some(Selection {
                anchor: ByteOffset(7),
                head: ByteOffset(4),
            }),
        }],
        LiteralOptions {
            whole_word: true,
            wrap: false,
            ..LiteralOptions::default()
        },
        true,
    )?;
    assert_eq!(
        search.title,
        "Search \"one\" (1 hit in 1 selection of 1 searched) [Normal: Word]"
    );
    assert_eq!(
        search.documents[0].lines[0].ranges,
        vec![ByteOffset(4)..ByteOffset(7)]
    );
    Ok(())
}

#[test]
fn normal_find_all_uses_entire_document_and_source_grouped_line_results()
-> Result<(), Box<dyn std::error::Error>> {
    let search = search_documents(
        "one",
        vec![Document {
            name: "new 1".into(),
            text: Arc::from("one one\nONE"),
            selection: None,
        }],
        LiteralOptions {
            wrap: false,
            ..LiteralOptions::default()
        },
        true,
    )?;
    assert_eq!(
        search.title,
        "Search \"one\" (3 hits in 1 file of 1 searched) [Normal]"
    );
    assert_eq!(search.hits, 3);
    assert_eq!(search.documents.len(), 1);
    let lines = &search.documents[0].lines;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].caption, "\tLine 1: one one");
    assert_eq!(lines[0].highlights, vec![9..12, 13..16]);
    assert_eq!(
        lines[0].ranges,
        vec![ByteOffset(0)..ByteOffset(3), ByteOffset(4)..ByteOffset(7)]
    );
    assert_eq!(lines[1].ranges, vec![ByteOffset(8)..ByteOffset(11)]);
    Ok(())
}
