use std::collections::BTreeMap;
use std::sync::{Arc, atomic::AtomicBool};

use notepad_matrix::editor::ByteOffset;
use notepad_matrix::find_in_files::{FileSearchRequest, search_files};

#[test]
fn unicode_folded_file_search_keeps_source_byte_lengths_for_expanded_matches()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("fold.txt"), "É Straße\nSS ß")?;
    let result = search_files(
        FileSearchRequest {
            directory: root.path().to_path_buf(),
            filters: "*.txt".into(),
            query: "ss".into(),
            mode: notepad_matrix::search::SearchMode::Normal,
            match_case: false,
            whole_word: false,
            recursive: false,
            hidden_directories: false,
            open_documents: BTreeMap::new(),
        },
        &AtomicBool::new(false),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        result
            .matches
            .iter()
            .map(|hit| hit.range.clone())
            .collect::<Vec<_>>(),
        vec![
            ByteOffset(7)..ByteOffset(9),
            ByteOffset(11)..ByteOffset(13),
            ByteOffset(14)..ByteOffset(16)
        ]
    );
    Ok(())
}

#[test]
fn extended_file_search_decodes_controls_but_retains_the_original_query_header()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("controls.txt"), b"a\0b\tc")?;
    let request = FileSearchRequest {
        directory: root.path().to_path_buf(),
        filters: "*.txt".into(),
        query: r"\0".into(),
        mode: notepad_matrix::search::SearchMode::Extended,
        match_case: true,
        whole_word: false,
        recursive: false,
        hidden_directories: false,
        open_documents: BTreeMap::new(),
    };
    let result = search_files(request, &AtomicBool::new(false));
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].range, ByteOffset(1)..ByteOffset(2));
    let search = notepad_matrix::finder::from_file_search(
        r"\0",
        result,
        notepad_matrix::search::LiteralOptions {
            mode: notepad_matrix::search::SearchMode::Extended,
            match_case: true,
            ..Default::default()
        },
        true,
    );
    assert_eq!(
        search.title,
        "Search \"\\0\" (1 hit in 1 file of 1 searched) [Extended: Case]"
    );
    Ok(())
}

#[test]
fn immediately_cancelled_search_keeps_the_source_total_candidate_count()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let paths = vec![root.path().join("one.txt"), root.path().join("two.txt")];
    for path in &paths {
        std::fs::write(path, "foo")?;
    }
    let request = FileSearchRequest {
        mode: notepad_matrix::search::SearchMode::Normal,
        directory: root.path().to_path_buf(),
        filters: "*.txt".into(),
        query: "foo".into(),
        match_case: true,
        whole_word: false,
        recursive: false,
        hidden_directories: false,
        open_documents: BTreeMap::new(),
    };
    let result = search_files(request, &AtomicBool::new(true));
    assert!(result.cancelled);
    assert_eq!(result.files_searched, 2);
    assert!(result.matches.is_empty());
    let search = notepad_matrix::finder::from_file_search(
        "foo",
        result,
        notepad_matrix::search::LiteralOptions {
            match_case: true,
            ..Default::default()
        },
        true,
    );
    assert_eq!(
        search.title,
        "Search \"foo\" (0 hits in 0 files of 2 searched) [Normal: Case]"
    );
    Ok(())
}

#[test]
fn whole_word_file_search_uses_the_same_source_boundaries() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("words.txt"), "xfoo foo foo_")?;
    let request = FileSearchRequest {
        mode: notepad_matrix::search::SearchMode::Normal,
        directory: root.path().to_path_buf(),
        filters: "*.txt".into(),
        query: "foo".into(),
        match_case: true,
        whole_word: true,
        recursive: false,
        hidden_directories: false,
        open_documents: BTreeMap::new(),
    };
    let result = search_files(request, &AtomicBool::new(false));
    assert!(result.errors.is_empty());
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].range, ByteOffset(5)..ByteOffset(8));
    Ok(())
}

#[test]
fn cancelled_and_unreadable_searches_do_not_report_successful_zero_hits()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("bad.txt"), [0xff, 0x00, 0xff])?;
    let request = || FileSearchRequest {
        mode: notepad_matrix::search::SearchMode::Normal,
        directory: root.path().to_path_buf(),
        filters: String::new(),
        query: "needle".into(),
        match_case: true,
        whole_word: false,
        recursive: true,
        hidden_directories: false,
        open_documents: BTreeMap::new(),
    };
    let cancelled = search_files(request(), &AtomicBool::new(true));
    assert!(cancelled.cancelled);
    assert_eq!(cancelled.files_searched, 1);
    let failed = search_files(request(), &AtomicBool::new(false));
    assert_eq!(failed.errors.len(), 1);
    assert!(failed.matches.is_empty());
    assert_eq!(
        std::fs::read(root.path().join("bad.txt"))?,
        [0xff, 0x00, 0xff]
    );
    Ok(())
}

#[test]
fn normal_directory_search_filters_recurses_and_prefers_unsaved_open_text()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("child"))?;
    std::fs::create_dir(root.path().join(".hidden"))?;
    std::fs::write(root.path().join("open.txt"), "disk only")?;
    std::fs::write(root.path().join("skip.txt"), "中😀")?;
    std::fs::write(root.path().join("other.rs"), "中😀")?;
    std::fs::write(root.path().join("child/hit.txt"), "first\r\n中😀 end")?;
    std::fs::write(root.path().join(".hidden/hit.txt"), "中😀")?;
    let open = root.path().join("open.txt").canonicalize()?;
    let request = FileSearchRequest {
        mode: notepad_matrix::search::SearchMode::Normal,
        directory: root.path().to_path_buf(),
        filters: "*.txt !skip.*".into(),
        query: "中😀".into(),
        match_case: true,
        whole_word: false,
        recursive: true,
        hidden_directories: false,
        open_documents: BTreeMap::from([(open.clone(), Arc::from("unsaved 中😀"))]),
    };
    let result = search_files(request, &AtomicBool::new(false));
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.matches.len(), 2);
    let hit = result
        .matches
        .iter()
        .find(|hit| hit.path == open)
        .ok_or("open hit")?;
    assert_eq!(hit.range, ByteOffset(8)..ByteOffset(15));
    assert_eq!((hit.line, hit.column), (1, 9));
    let child = result
        .matches
        .iter()
        .find(|hit| hit.path.ends_with("child/hit.txt"))
        .ok_or("child hit")?;
    assert_eq!((child.line, child.column), (2, 1));
    assert_eq!(child.preview, "中😀 end");
    assert_eq!(std::fs::read_to_string(open)?, "disk only");
    Ok(())
}
