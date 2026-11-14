use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Document,
    Group,
    Paragraph,
    Heading(u8),
    Quote(Option<String>),
    List(Option<u64>),
    Item,
    CodeBlock(Option<String>),
    HtmlBlock,
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strike,
    Link(String),
    Image { destination: String, title: String },
    Text(String),
    Code(String),
    Html(String),
    SoftBreak,
    HardBreak,
    Rule,
    TaskMarker(bool),
}

impl Kind {
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            Self::Document
                | Self::Group
                | Self::Paragraph
                | Self::Heading(_)
                | Self::Quote(_)
                | Self::List(_)
                | Self::Item
                | Self::CodeBlock(_)
                | Self::HtmlBlock
                | Self::Table(_)
                | Self::TableHead
                | Self::TableRow
                | Self::TableCell
                | Self::Rule
        )
    }
}

#[derive(Debug)]
pub struct Node {
    pub kind: Kind,
    pub children: Vec<Node>,
    pub source: Range<usize>,
    pub fingerprint: u64,
}

impl Drop for Node {
    fn drop(&mut self) {
        let mut children = std::mem::take(&mut self.children);
        while let Some(mut child) = children.pop() {
            children.append(&mut child.children);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineSpan {
    pub range: Range<usize>,
    pub style: InlineStyle,
}

#[derive(Default)]
pub struct InlineText {
    pub text: String,
    pub spans: Vec<InlineSpan>,
}

impl InlineText {
    fn push(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        if let Some(previous) = self.spans.last_mut()
            && previous.style == style
        {
            previous.range.end = self.text.len();
        } else {
            self.spans.push(InlineSpan {
                range: start..self.text.len(),
                style,
            });
        }
    }
}

impl Node {
    pub fn plain_text(&self) -> String {
        self.inline_text().text
    }

    pub fn inline_text(&self) -> InlineText {
        inline_text(std::slice::from_ref(self))
    }

    pub fn images(&self) -> Vec<&Node> {
        let mut images = Vec::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if matches!(node.kind, Kind::Image { .. }) {
                images.push(node);
            }
            stack.extend(node.children.iter().rev());
        }
        images
    }
}

pub fn inline_text(nodes: &[Node]) -> InlineText {
    let mut output = InlineText::default();
    let mut stack: Vec<_> = nodes
        .iter()
        .rev()
        .map(|node| (node, InlineStyle::default()))
        .collect();
    while let Some((node, mut style)) = stack.pop() {
        match &node.kind {
            Kind::Text(text) => output.push(text, style),
            Kind::Code(text) | Kind::Html(text) => {
                style.code = true;
                output.push(text, style);
            }
            Kind::SoftBreak => output.push(" ", style),
            Kind::HardBreak => output.push("\n", style),
            Kind::TaskMarker(checked) => output.push(if *checked { "☑ " } else { "☐ " }, style),
            _ => {
                match &node.kind {
                    Kind::Emphasis => style.italic = true,
                    Kind::Strong => style.bold = true,
                    Kind::Strike => style.strike = true,
                    Kind::Link(destination) => style.link = Some(destination.clone()),
                    Kind::Image { destination, .. } if style.link.is_none() => {
                        style.link = Some(destination.clone())
                    }
                    _ => {}
                }
                stack.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|child| (child, style.clone())),
                );
            }
        }
    }
    output
}

fn tag_kind(tag: Tag<'_>) -> Kind {
    match tag {
        Tag::Paragraph => Kind::Paragraph,
        Tag::Heading { level, .. } => Kind::Heading(level as u8),
        Tag::BlockQuote(kind) => Kind::Quote(kind.map(|kind| format!("{kind:?}"))),
        Tag::CodeBlock(kind) => Kind::CodeBlock(match kind {
            CodeBlockKind::Indented => None,
            CodeBlockKind::Fenced(language) => Some(language.into_string()),
        }),
        Tag::HtmlBlock => Kind::HtmlBlock,
        Tag::List(start) => Kind::List(start),
        Tag::Item => Kind::Item,
        Tag::Table(alignments) => Kind::Table(
            alignments
                .into_iter()
                .map(|alignment| match alignment {
                    pulldown_cmark::Alignment::None => Alignment::None,
                    pulldown_cmark::Alignment::Left => Alignment::Left,
                    pulldown_cmark::Alignment::Center => Alignment::Center,
                    pulldown_cmark::Alignment::Right => Alignment::Right,
                })
                .collect(),
        ),
        Tag::TableHead => Kind::TableHead,
        Tag::TableRow => Kind::TableRow,
        Tag::TableCell => Kind::TableCell,
        Tag::Emphasis => Kind::Emphasis,
        Tag::Strong => Kind::Strong,
        Tag::Strikethrough => Kind::Strike,
        Tag::Link { dest_url, .. } => Kind::Link(dest_url.into_string()),
        Tag::Image {
            dest_url, title, ..
        } => Kind::Image {
            destination: dest_url.into_string(),
            title: title.into_string(),
        },
        _ => Kind::Group,
    }
}

pub fn parse(source: &str) -> Node {
    parse_with_cancel(source, || false).expect("uncancelled Markdown parse")
}

pub fn parse_with_cancel(source: &str, mut cancelled: impl FnMut() -> bool) -> Option<Node> {
    if cancelled() {
        return None;
    }
    let mut stack = vec![Node {
        kind: Kind::Document,
        children: Vec::new(),
        source: 0..source.len(),
        fingerprint: 0,
    }];
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_GFM;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        if cancelled() {
            return None;
        }
        let kind = match event {
            Event::Start(tag) => {
                stack.push(Node {
                    kind: tag_kind(tag),
                    children: Vec::new(),
                    source: range,
                    fingerprint: 0,
                });
                continue;
            }
            Event::End(_) => {
                let mut node = stack.pop().expect("balanced Markdown end event");
                node.source.end = node.source.end.max(range.end);
                if stack.len() == 1 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    source[node.source.clone()].hash(&mut hash);
                    node.fingerprint = hash.finish();
                }
                stack
                    .last_mut()
                    .expect("Markdown parent")
                    .children
                    .push(node);
                continue;
            }
            Event::Text(text) => Kind::Text(text.into_string()),
            Event::Code(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                Kind::Code(text.into_string())
            }
            Event::Html(text) | Event::InlineHtml(text) => Kind::Html(text.into_string()),
            Event::FootnoteReference(label) => Kind::Text(format!("[{label}]")),
            Event::SoftBreak => Kind::SoftBreak,
            Event::HardBreak => Kind::HardBreak,
            Event::Rule => Kind::Rule,
            Event::TaskListMarker(checked) => Kind::TaskMarker(checked),
        };
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        source[range.clone()].hash(&mut hash);
        stack
            .last_mut()
            .expect("Markdown root")
            .children
            .push(Node {
                kind,
                children: Vec::new(),
                source: range,
                fingerprint: hash.finish(),
            });
    }
    Some(stack.pop().expect("Markdown document"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Fragment(String),
    Local(PathBuf),
    External(String),
    Blocked,
}

pub fn heading_anchor(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|character| {
            if character.is_whitespace() {
                Some('-')
            } else if character.is_alphanumeric() || matches!(character, '-' | '_') {
                Some(character)
            } else {
                None
            }
        })
        .collect()
}

pub fn heading_targets(document: &Node) -> Vec<(String, usize)> {
    let mut used = std::collections::BTreeSet::new();
    let mut suffixes = std::collections::BTreeMap::<String, usize>::new();
    let mut targets = Vec::new();
    let mut stack: Vec<_> = document.children.iter().enumerate().rev().collect();
    while let Some((block, node)) = stack.pop() {
        if matches!(node.kind, Kind::Heading(_)) {
            let base = heading_anchor(&node.plain_text());
            let duplicate = suffixes.entry(base.clone()).or_default();
            let mut anchor = if *duplicate == 0 {
                base.clone()
            } else {
                format!("{base}-{duplicate}")
            };
            while used.contains(&anchor) {
                *duplicate += 1;
                anchor = format!("{base}-{duplicate}");
            }
            *duplicate += 1;
            used.insert(anchor.clone());
            targets.push((anchor, block));
        }
        stack.extend(node.children.iter().rev().map(|child| (block, child)));
    }
    targets
}

pub fn resolve_destination(destination: &str, base: Option<&Path>) -> Destination {
    if let Some(fragment) = destination.strip_prefix('#') {
        return Destination::Fragment(fragment.to_owned());
    }
    let resolved = url::Url::parse(destination).ok().or_else(|| {
        let base = if destination.starts_with('/') {
            url::Url::parse("file:///").ok()?
        } else {
            url::Url::from_directory_path(base?).ok()?
        };
        base.join(destination).ok()
    });
    let Some(resolved) = resolved else {
        return Destination::Blocked;
    };
    match resolved.scheme() {
        "http" | "https" | "mailto" => Destination::External(resolved.into()),
        "file" => resolved
            .to_file_path()
            .map_or(Destination::Blocked, Destination::Local),
        _ => Destination::Blocked,
    }
}

#[derive(Clone, Debug)]
pub struct Media {
    pub destination: String,
    pub navigation: Option<String>,
    pub alt: String,
    pub title: String,
}

#[derive(Clone)]
pub struct PreparedInline {
    pub text: Arc<str>,
    pub spans: Arc<[InlineSpan]>,
    pub media: Vec<Media>,
}

fn prepare_inline(nodes: &[Node]) -> PreparedInline {
    let mut inline = InlineText::default();
    let mut media = Vec::new();
    let mut stack: Vec<_> = nodes
        .iter()
        .rev()
        .map(|node| (node, InlineStyle::default()))
        .collect();
    while let Some((node, mut style)) = stack.pop() {
        match &node.kind {
            Kind::Text(text) => inline.push(text, style),
            Kind::Code(text) | Kind::Html(text) => {
                style.code = true;
                inline.push(text, style);
            }
            Kind::SoftBreak => inline.push(" ", style),
            Kind::HardBreak => inline.push("\n", style),
            Kind::TaskMarker(checked) => inline.push(if *checked { "☑ " } else { "☐ " }, style),
            Kind::Image { destination, title } => media.push(Media {
                destination: destination.clone(),
                navigation: style.link,
                alt: node.plain_text(),
                title: title.clone(),
            }),
            _ => {
                match &node.kind {
                    Kind::Emphasis => style.italic = true,
                    Kind::Strong => style.bold = true,
                    Kind::Strike => style.strike = true,
                    Kind::Link(destination) => style.link = Some(destination.clone()),
                    _ => {}
                }
                stack.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|child| (child, style.clone())),
                );
            }
        }
    }
    PreparedInline {
        text: inline.text.into(),
        spans: inline.spans.into(),
        media,
    }
}

pub enum RowContent {
    Text {
        inline: PreparedInline,
        heading: Option<u8>,
    },
    Code {
        body: Arc<str>,
        range: Range<usize>,
        language: Option<Arc<str>>,
        first: bool,
        last: bool,
    },
    Mermaid {
        body: Arc<str>,
    },
    Table {
        cells: Vec<PreparedInline>,
        alignments: Arc<[Alignment]>,
        heading: bool,
    },
    Rule,
}

pub struct RenderRow {
    pub content: RowContent,
    pub quote_depth: usize,
    pub list_depth: usize,
    pub marker: Option<String>,
    pub fingerprint: u64,
}

pub struct RenderDocument {
    pub rows: Vec<RenderRow>,
    pub anchors: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Default)]
struct RowContext {
    quote_depth: usize,
    list_depth: usize,
    marker: Option<String>,
}

enum PendingRow<'a> {
    Node(&'a Node, RowContext),
    Children(&'a [Node], RowContext),
    Inline(&'a [Node], RowContext),
}

fn row(content: RowContent, context: RowContext, raw: &str) -> RenderRow {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hash);
    context.quote_depth.hash(&mut hash);
    context.list_depth.hash(&mut hash);
    context.marker.hash(&mut hash);
    RenderRow {
        content,
        quote_depth: context.quote_depth,
        list_depth: context.list_depth,
        marker: context.marker,
        fingerprint: hash.finish(),
    }
}

pub fn render_document(
    source: &str,
    mut cancelled: impl FnMut() -> bool,
) -> Option<RenderDocument> {
    let document = parse_with_cancel(source, &mut cancelled)?;
    let mut output = RenderDocument {
        rows: Vec::new(),
        anchors: std::collections::BTreeMap::new(),
    };
    let mut used = std::collections::BTreeSet::new();
    let mut suffixes = std::collections::BTreeMap::<String, usize>::new();
    let mut pending = vec![PendingRow::Children(
        &document.children,
        RowContext::default(),
    )];
    while let Some(item) = pending.pop() {
        if cancelled() {
            return None;
        }
        match item {
            PendingRow::Inline(nodes, context) => {
                let inline = prepare_inline(nodes);
                let raw = source
                    .get(nodes.first()?.source.start..nodes.last()?.source.end)
                    .unwrap_or("");
                output.rows.push(row(
                    RowContent::Text {
                        inline,
                        heading: None,
                    },
                    context,
                    raw,
                ));
            }
            PendingRow::Children(nodes, mut context) => {
                if nodes.is_empty() && context.marker.is_some() {
                    output.rows.push(row(
                        RowContent::Text {
                            inline: prepare_inline(&[]),
                            heading: None,
                        },
                        context,
                        "",
                    ));
                    continue;
                }
                let mut children = Vec::new();
                let mut start = 0;
                for index in 0..=nodes.len() {
                    if index == nodes.len() || nodes[index].kind.is_block() {
                        if start < index {
                            let mut next = context.clone();
                            next.marker = context.marker.take();
                            children.push(PendingRow::Inline(&nodes[start..index], next));
                        }
                        if index < nodes.len() {
                            let mut next = context.clone();
                            next.marker = context.marker.take();
                            children.push(PendingRow::Node(&nodes[index], next));
                        }
                        start = index + 1;
                    }
                }
                pending.extend(children.into_iter().rev());
            }
            PendingRow::Node(node, mut context) => match &node.kind {
                Kind::Paragraph | Kind::Heading(_) | Kind::TableCell => {
                    let inline = prepare_inline(&node.children);
                    let heading = if let Kind::Heading(level) = node.kind {
                        Some(level)
                    } else {
                        None
                    };
                    if heading.is_some() {
                        let base = heading_anchor(&node.plain_text());
                        let duplicate = suffixes.entry(base.clone()).or_default();
                        let mut anchor = if *duplicate == 0 {
                            base.clone()
                        } else {
                            format!("{base}-{duplicate}")
                        };
                        while used.contains(&anchor) {
                            *duplicate += 1;
                            anchor = format!("{base}-{duplicate}");
                        }
                        *duplicate += 1;
                        used.insert(anchor.clone());
                        output.anchors.insert(anchor, output.rows.len());
                    }
                    output.rows.push(row(
                        RowContent::Text { inline, heading },
                        context,
                        &source[node.source.clone()],
                    ));
                }
                Kind::Quote(kind) => {
                    context.quote_depth += 1;
                    if let Some(kind) = kind {
                        let inline = PreparedInline {
                            text: kind.clone().into(),
                            spans: vec![InlineSpan {
                                range: 0..kind.len(),
                                style: InlineStyle {
                                    bold: true,
                                    ..Default::default()
                                },
                            }]
                            .into(),
                            media: Vec::new(),
                        };
                        let mut label_context = context.clone();
                        label_context.marker = context.marker.take();
                        output.rows.push(row(
                            RowContent::Text {
                                inline,
                                heading: None,
                            },
                            label_context,
                            kind,
                        ));
                    }
                    pending.push(PendingRow::Children(&node.children, context));
                }
                Kind::List(start) => {
                    if context.marker.is_some() {
                        let mut marker_context = context.clone();
                        marker_context.marker = context.marker.take();
                        output.rows.push(row(
                            RowContent::Text {
                                inline: prepare_inline(&[]),
                                heading: None,
                            },
                            marker_context,
                            "",
                        ));
                    }
                    pending.extend(node.children.iter().enumerate().rev().map(|(index, item)| {
                        PendingRow::Children(
                            &item.children,
                            RowContext {
                                list_depth: context.list_depth + 1,
                                marker: Some(start.map_or_else(
                                    || "•".into(),
                                    |start| format!("{}.", start + index as u64),
                                )),
                                ..context.clone()
                            },
                        )
                    }));
                }
                Kind::CodeBlock(_) | Kind::HtmlBlock => {
                    let language: Option<Arc<str>> = if let Kind::CodeBlock(language) = &node.kind {
                        language.clone().map(Into::into)
                    } else {
                        Some("HTML (not executed)".into())
                    };
                    let body: Arc<str> = node.plain_text().into();
                    if language.as_deref().is_some_and(|language| {
                        language
                            .split_whitespace()
                            .next()
                            .is_some_and(|name| name.eq_ignore_ascii_case("mermaid"))
                    }) {
                        output.rows.push(row(
                            RowContent::Mermaid { body },
                            context,
                            &source[node.source.clone()],
                        ));
                        continue;
                    }
                    let mut start = 0;
                    for (index, line) in body.split_inclusive('\n').enumerate() {
                        if cancelled() {
                            return None;
                        }
                        let end = start + line.len();
                        let mut line_context = context.clone();
                        if index != 0 {
                            line_context.marker = None;
                        }
                        output.rows.push(row(
                            RowContent::Code {
                                body: body.clone(),
                                range: start..end,
                                language: language.clone(),
                                first: index == 0,
                                last: end == body.len(),
                            },
                            line_context,
                            line,
                        ));
                        start = end;
                    }
                }
                Kind::Table(alignments) => {
                    let alignments: Arc<[Alignment]> = alignments.clone().into();
                    for table_row in &node.children {
                        if cancelled() {
                            return None;
                        }
                        let cells = table_row
                            .children
                            .iter()
                            .map(|cell| prepare_inline(&cell.children))
                            .collect();
                        let mut row_context = context.clone();
                        row_context.marker = context.marker.take();
                        output.rows.push(row(
                            RowContent::Table {
                                cells,
                                alignments: alignments.clone(),
                                heading: matches!(table_row.kind, Kind::TableHead),
                            },
                            row_context,
                            &source[table_row.source.clone()],
                        ));
                    }
                }
                Kind::Rule => {
                    output
                        .rows
                        .push(row(RowContent::Rule, context, &source[node.source.clone()]))
                }
                _ => pending.push(PendingRow::Children(&node.children, context)),
            },
        }
    }
    Some(output)
}
