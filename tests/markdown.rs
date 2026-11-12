use notepad_matrix::markdown::{Destination, Kind, parse, resolve_destination};

#[test]
fn markdown_model_parses_rendered_blocks_and_inline_styles_without_changing_source() {
    let source = "# 标题\n\nHello **bold** and *italic* and ~~gone~~ `code` &amp;.\n\n> quote\n\n7. first\n8. second\n\n- [x] done\n- [ ] pending\n\n```rust\nlet x = \"**literal**\";\n```\n\n| left | right |\n| :--- | ---: |\n| a | b |\n\n[site](https://example.com) ![alt](image.png)\n\n<script>bad()</script>\n";
    let rendered = parse(source);
    assert!(matches!(rendered.children[0].kind, Kind::Heading(1)));
    assert_eq!(rendered.children[0].plain_text(), "标题");
    assert!(
        rendered
            .children
            .iter()
            .any(|node| matches!(node.kind, Kind::List(Some(7))))
    );
    assert!(
        rendered
            .children
            .iter()
            .any(|node| matches!(node.kind, Kind::Table(_)))
    );
    let paragraph = &rendered.children[1];
    let inline = paragraph.inline_text();
    assert_eq!(inline.text, "Hello bold and italic and gone code &.");
    assert!(
        inline
            .spans
            .iter()
            .any(|span| span.style.bold && &inline.text[span.range.clone()] == "bold")
    );
    assert!(
        inline
            .spans
            .iter()
            .any(|span| span.style.italic && &inline.text[span.range.clone()] == "italic")
    );
    assert!(
        inline
            .spans
            .iter()
            .any(|span| span.style.strike && &inline.text[span.range.clone()] == "gone")
    );
    assert!(
        inline
            .spans
            .iter()
            .any(|span| span.style.code && &inline.text[span.range.clone()] == "code")
    );
    let code = rendered
        .children
        .iter()
        .find(|node| matches!(node.kind, Kind::CodeBlock(_)))
        .unwrap();
    assert_eq!(code.plain_text(), "let x = \"**literal**\";\n");
    assert!(
        rendered
            .children
            .iter()
            .any(|node| matches!(node.kind, Kind::HtmlBlock))
    );
    assert_eq!(rendered.source, 0..source.len());
}

#[test]
fn markdown_destinations_resolve_local_paths_without_automatic_remote_or_script_access() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        resolve_destination("#标题", Some(home.path())),
        Destination::Fragment("标题".into())
    );
    assert_eq!(
        resolve_destination("image%20one.png", Some(home.path())),
        Destination::Local(home.path().join("image one.png"))
    );
    assert!(matches!(
        resolve_destination("https://example.com/image.png", Some(home.path())),
        Destination::External(_)
    ));
    assert_eq!(
        resolve_destination("javascript:bad()", Some(home.path())),
        Destination::Blocked
    );
    assert_eq!(
        resolve_destination("data:image/png;base64,bad", Some(home.path())),
        Destination::Blocked
    );
    assert_eq!(
        resolve_destination("relative.png", None),
        Destination::Blocked
    );
}

#[test]
fn markdown_cancellation_and_block_identity_do_not_rewrite_the_document() {
    use notepad_matrix::markdown::parse_with_cancel;
    assert!(parse_with_cancel("# title", || true).is_none());
    let mut events = 0;
    assert!(
        parse_with_cancel("# title\n\nbody", || {
            events += 1;
            events > 2
        })
        .is_none()
    );
    let old = parse("# title\n\nbody\n");
    let changed = parse("before\n\n# title\n\nbody\n");
    assert_eq!(old.children[0].fingerprint, changed.children[1].fingerprint);
    assert_eq!(old.children[1].fingerprint, changed.children[2].fingerprint);
    let headings = parse("# 标题\n\n# 标题\n\n> ## Nested **title**\n");
    assert_eq!(
        notepad_matrix::markdown::heading_targets(&headings),
        [
            ("标题".into(), 0),
            ("标题-1".into(), 1),
            ("nested-title".into(), 2)
        ]
    );
}

#[test]
fn linked_markdown_images_preserve_the_wrapping_navigation_target() {
    let document = parse("[![alt](image.png)](https://example.com)\n");
    let inline = document.children[0].inline_text();
    assert_eq!(inline.text, "alt");
    assert_eq!(
        inline.spans[0].style.link.as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn prepared_markdown_rows_cover_large_containers_deep_quotes_and_linked_images() {
    use notepad_matrix::markdown::{RowContent, render_document};
    let nested = format!("{}x\n", "> ".repeat(10_000));
    let quotes = render_document(&nested, || false).unwrap();
    assert_eq!(quotes.rows.len(), 1);
    assert_eq!(quotes.rows[0].quote_depth, 10_000);
    let list = render_document(&"- x\n".repeat(10_000), || false).unwrap();
    assert_eq!(list.rows.len(), 10_000);
    let table = format!("| a | b |\n| --- | --- |\n{}", "| x | y |\n".repeat(10_000));
    assert_eq!(
        render_document(&table, || false).unwrap().rows.len(),
        10_001
    );
    let headings = render_document(&"# x\n\n".repeat(10_000), || false).unwrap();
    assert_eq!(headings.anchors["x-9999"], 9_999);
    let linked = render_document(
        "- [![a](image.png)](https://example.com)\n\n| image |\n| --- |\n| ![b](image.png) |\n",
        || false,
    )
    .unwrap();
    let RowContent::Text { inline, .. } = &linked.rows[0].content else {
        panic!("tight list inline row");
    };
    assert_eq!(inline.media[0].destination, "image.png");
    assert_eq!(
        inline.media[0].navigation.as_deref(),
        Some("https://example.com")
    );
    let RowContent::Table { cells, .. } = &linked.rows[2].content else {
        panic!("table body row");
    };
    assert_eq!(cells[0].media[0].alt, "b");
}

#[test]
fn prepared_markdown_preserves_image_heading_anchors_and_empty_list_items() {
    use notepad_matrix::markdown::{RowContent, render_document};
    let document = render_document("# ![Logo](logo.png)\n\n-\n- x\n", || false).unwrap();
    assert_eq!(document.anchors["logo"], 0);
    assert_eq!(document.rows.len(), 3);
    assert_eq!(document.rows[1].marker.as_deref(), Some("•"));
    let RowContent::Text { inline, .. } = &document.rows[1].content else {
        panic!("empty list row");
    };
    assert!(inline.text.is_empty());
}
