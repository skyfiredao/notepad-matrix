use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use notepad_matrix::input::EditorSession;

struct CountedSystem;

thread_local! {
    static REQUESTED: Cell<Option<usize>> = const { Cell::new(None) };
}

fn account(size: usize) {
    REQUESTED.with(|requested| {
        if let Some(total) = requested.get() {
            requested.set(Some(total.saturating_add(size)));
        }
    });
}

unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        account(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        account(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        account(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;

#[test]
fn empty_finder_text_copy_does_not_materialize_retained_history()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Document, Results, Row, TextPoint, search_documents};
    let search = search_documents(
        "foo",
        vec![Document {
            name: "new 1".into(),
            text: std::sync::Arc::from("foo\n".repeat(1000)),
            selection: None,
        }],
        Default::default(),
        true,
    )?;
    let mut results = Results::default();
    results.push(search, false);
    let caret = TextPoint {
        row: Some(Row::Line(0, 0, 0)),
        byte: 0,
    };
    REQUESTED.with(|requested| requested.set(Some(0)));
    let text = results.copy_verbatim(caret, caret);
    let bytes = REQUESTED
        .with(|requested| requested.replace(None))
        .ok_or("counter not enabled")?;
    assert_eq!(text, None);
    assert_eq!(
        bytes, 0,
        "an empty caret copy must return before materializing any result rows"
    );
    let start = TextPoint { byte: 12, ..caret };
    let end = TextPoint { byte: 15, ..caret };
    REQUESTED.with(|requested| requested.set(Some(0)));
    let text = results.copy_verbatim(start, end);
    let bytes = REQUESTED
        .with(|requested| requested.replace(None))
        .ok_or("counter not enabled")?;
    assert_eq!(text.as_deref(), Some("foo"));
    assert_eq!(
        bytes, 3,
        "a same-row text copy only owns its selected output bytes"
    );
    Ok(())
}

fn finder_preview_allocations(hits: usize) -> Result<usize, Box<dyn std::error::Error>> {
    use notepad_matrix::finder::{Document, search_documents};
    let text: std::sync::Arc<str> =
        std::sync::Arc::from(format!("{}{}", "foo ".repeat(hits), "z".repeat(256 * 1024)));
    REQUESTED.with(|requested| requested.set(Some(0)));
    let search = search_documents(
        "foo",
        vec![Document {
            name: "new 1".into(),
            text,
            selection: None,
        }],
        notepad_matrix::search::LiteralOptions {
            match_case: true,
            ..Default::default()
        },
        true,
    );
    let bytes = REQUESTED
        .with(|requested| requested.replace(None))
        .ok_or("counter not enabled")?;
    let search = search?;
    assert_eq!(search.hits, hits);
    assert_eq!(search.documents[0].lines.len(), 1);
    assert!(search.documents[0].lines[0].caption.capacity() <= 2048);
    Ok(bytes)
}

#[test]
fn finder_long_line_preview_does_not_copy_the_entire_line_for_each_hit()
-> Result<(), Box<dyn std::error::Error>> {
    let small = finder_preview_allocations(32)?;
    let large = finder_preview_allocations(64)?;
    eprintln!("finder preview allocation bytes: 32={small}, 64={large}");
    assert!(
        large < small + small / 2,
        "fixed-size source previews must not scale whole-line copies with hit count: {small} -> {large}"
    );
    Ok(())
}

fn grouped_insertions(count: usize) -> Result<usize, Box<dyn std::error::Error>> {
    let mut editor = EditorSession::new("tail");
    let mut peer = editor.clone_view();
    REQUESTED.with(|requested| requested.set(Some(0)));
    editor.begin_undo_action();
    for _ in 0..count {
        editor.insert_text("X")?;
    }
    editor.end_undo_action();
    let bytes = REQUESTED
        .with(|requested| requested.replace(None))
        .ok_or("counter not enabled")?;
    assert_eq!(
        editor.committed_text(),
        format!("{}tail", "X".repeat(count))
    );
    assert!(peer.undo());
    assert_eq!(editor.committed_text(), "tail");
    assert!(!editor.document().can_undo());
    assert!(editor.redo());
    assert_eq!(peer.committed_text(), format!("{}tail", "X".repeat(count)));
    Ok(bytes)
}

#[test]
fn grouped_macro_replay_storage_grows_linearly_not_quadratically()
-> Result<(), Box<dyn std::error::Error>> {
    let small = grouped_insertions(2048)?;
    let large = grouped_insertions(4096)?;
    eprintln!("grouped allocation bytes: 2048={small}, 4096={large}");
    assert!(
        large < small * 3,
        "grouped replay must not copy its accumulated prefix: {small} -> {large}"
    );
    grouped_insertions(9999)?;
    Ok(())
}
