use notepad_matrix::search_history::{Field, History};

#[test]
fn command_history_capture_uses_source_utf16_buffer_sizes_and_nul_termination() {
    assert_eq!(Field::Find.capture(&"a".repeat(16384)).len(), 16383);
    assert_eq!(
        Field::Replace
            .capture(&"😀".repeat(8192))
            .encode_utf16()
            .count(),
        16383
    );
    assert_eq!(Field::Filter.capture(&"f".repeat(512)).len(), 511);
    assert_eq!(Field::Directory.capture(&"d".repeat(260)).len(), 259);
    assert_eq!(Field::Directory.capture("  \tpath\t  "), "\tpath\t");
    assert_eq!(Field::Find.capture("before\0after"), "before");
    assert_eq!(Field::Find.capture(r"\n\x41"), r"\n\x41");
}

#[test]
fn combo_history_moves_existing_items_to_front_without_live_count_truncation() {
    let mut history = History::load(
        Field::Find,
        &serde_json::json!({"nbMaxFindHistoryFind": 2, "finds": ["First", "second", "third"]}),
        str::eq_ignore_ascii_case,
    );
    history.record("SECOND", str::eq_ignore_ascii_case);
    history.record("", str::eq_ignore_ascii_case);
    assert_eq!(history.entries(), ["SECOND", "First", "third"]);
    assert_eq!(history.saved("not yet searched"), ["SECOND", "First"]);
    let mut replace = History::load(
        Field::Replace,
        &serde_json::json!({"nbMaxFindHistoryReplace": 1, "replaces": ["", "old"]}),
        str::eq_ignore_ascii_case,
    );
    assert_eq!(replace.initial_text(), "");
    assert_eq!(replace.entries(), ["old"]);
    assert_eq!(replace.saved(""), ["", "old"]);
    replace.record("new", str::eq_ignore_ascii_case);
    assert_eq!(replace.saved("new"), ["new"]);
}

#[test]
fn saved_history_uses_source_utf16_length_and_counts_skipped_slots() {
    let mut history = History::load(
        Field::Find,
        &serde_json::json!({"nbMaxFindHistoryFind": 2}),
        str::eq_ignore_ascii_case,
    );
    history.record("last", str::eq_ignore_ascii_case);
    history.record(&"😀".repeat(1024), str::eq_ignore_ascii_case);
    history.record("first", str::eq_ignore_ascii_case);
    assert_eq!(history.saved(""), ["first"]);
    assert_eq!(history.entries().len(), 3);
    let disabled = History::load(
        Field::Filter,
        &serde_json::json!({"nbMaxFindHistoryFilter": 0, "filters": ["*.rs"]}),
        str::eq_ignore_ascii_case,
    );
    assert!(disabled.entries().is_empty());
    assert_eq!(disabled.saved(""), [""]);
    let capped = History::load(
        Field::Filter,
        &serde_json::json!({"nbMaxFindHistoryFilter": 100, "filters": (0..35).map(|i| format!("*.{i}")).collect::<Vec<_>>()}),
        str::eq_ignore_ascii_case,
    );
    assert_eq!(capped.entries().len(), 20);
}
