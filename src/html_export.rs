use crate::syntax::{JsonStyle, Language, StyleAppearance};
use std::fmt;

#[derive(Debug)]
pub struct InvalidTabWidth;
impl fmt::Display for InvalidTabWidth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HTML export requires a positive tab width")
    }
}
impl std::error::Error for InvalidTabWidth {}

#[derive(Debug)]
pub enum HtmlExportError {
    InvalidTabWidth(InvalidTabWidth),
    MissingDefinition(u64),
    Builtin(String),
}
impl fmt::Display for HtmlExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(error) => formatter.write_str(error),
            Self::InvalidTabWidth(error) => error.fmt(formatter),
            Self::MissingDefinition(id) => write!(
                formatter,
                "User Defined language {id} is unavailable; HTML export requires its definition"
            ),
        }
    }
}
impl std::error::Error for HtmlExportError {}

struct ExportSpan {
    range: std::ops::Range<usize>,
    style: u16,
}

pub fn serialize_plain_html(
    text: &str,
    dark: bool,
    tab_width: usize,
) -> Result<String, InvalidTabWidth> {
    let default = StyleAppearance::plain(dark);
    let appearances = std::collections::BTreeMap::from([(0, default.clone())]);
    serialize_styled(
        text,
        tab_width,
        &[ExportSpan {
            range: 0..text.len(),
            style: 0,
        }],
        &default,
        &appearances,
    )
}

pub fn serialize_html(
    text: &str,
    language: Language,
    dark: bool,
    tab_width: usize,
) -> Result<String, HtmlExportError> {
    serialize_html_with_profile(
        text,
        language,
        dark,
        tab_width,
        &StyleAppearance::plain(dark),
    )
}

pub fn serialize_html_with_profile(
    text: &str,
    language: Language,
    dark: bool,
    tab_width: usize,
    profile: &StyleAppearance,
) -> Result<String, HtmlExportError> {
    if let Language::Builtin(id) = language {
        return serialize_builtin_html_with_profile(text, id, dark, tab_width, profile, 0);
    }
    if let Language::UserDefined(id) = language {
        return Err(HtmlExportError::MissingDefinition(id));
    }
    let mut appearances = std::collections::BTreeMap::new();
    let mut default = profile.clone();
    let spans = if language == Language::Json {
        crate::syntax::json_styles(text)
            .into_iter()
            .map(|span| {
                let (foreground, background, bold) = span.style.appearance(dark);
                appearances.insert(
                    span.style as u16,
                    StyleAppearance {
                        foreground,
                        background,
                        bold,
                        ..profile.clone()
                    },
                );
                ExportSpan {
                    range: span.range,
                    style: span.style as u16,
                }
            })
            .collect()
    } else {
        appearances.insert(0, default.clone());
        vec![ExportSpan {
            range: 0..text.len(),
            style: 0,
        }]
    };
    if language == Language::Json {
        let (foreground, background, bold) = JsonStyle::Default.appearance(dark);
        default.foreground = foreground;
        default.background = background;
        default.bold = bold;
    }
    serialize_styled(text, tab_width, &spans, &default, &appearances)
        .map_err(HtmlExportError::InvalidTabWidth)
}

pub fn serialize_builtin_html_with_profile(
    text: &str,
    id: u16,
    dark: bool,
    tab_width: usize,
    profile: &StyleAppearance,
    zoom_delta: i32,
) -> Result<String, HtmlExportError> {
    let catalog =
        crate::languages::catalog().map_err(|error| HtmlExportError::Builtin(error.to_string()))?;
    let definition = catalog.by_id(id).ok_or_else(|| {
        HtmlExportError::Builtin(format!(
            "Built-in language {id} is unavailable; HTML export requires its profile"
        ))
    })?;
    serialize_builtin_definition_html(text, definition, dark, tab_width, profile, zoom_delta)
}

pub fn serialize_builtin_definition_html(
    text: &str,
    definition: &crate::languages::LanguageDefinition,
    dark: bool,
    tab_width: usize,
    profile: &StyleAppearance,
    zoom_delta: i32,
) -> Result<String, HtmlExportError> {
    crate::builtin_language::validate_definition(definition)
        .map_err(|error| HtmlExportError::Builtin(error.to_string()))?;
    let lexed = crate::lexers::lex(text, definition)
        .map_err(|error| HtmlExportError::Builtin(error.to_string()))?;
    let default =
        crate::builtin_language::style_appearance(definition, 0, dark, profile, zoom_delta);
    let mut appearances = std::collections::BTreeMap::from([(0, default.clone())]);
    for style in &definition.styles {
        appearances.insert(
            style.id,
            crate::builtin_language::style_appearance(
                definition, style.id, dark, profile, zoom_delta,
            ),
        );
    }
    let mut covered = 0;
    let mut spans = Vec::with_capacity(lexed.spans.len());
    for span in lexed.spans {
        if span.range.start != covered
            || span.range.start > span.range.end
            || span.range.end > text.len()
            || !text.is_char_boundary(span.range.start)
            || !text.is_char_boundary(span.range.end)
        {
            return Err(HtmlExportError::Builtin(format!(
                "Built-in language {} produced invalid HTML export spans",
                definition.id
            )));
        }
        covered = span.range.end;
        appearances.entry(span.style).or_insert_with(|| {
            crate::builtin_language::style_appearance(
                definition, span.style, dark, profile, zoom_delta,
            )
        });
        spans.push(ExportSpan {
            range: span.range,
            style: span.style,
        });
    }
    if covered != text.len() {
        return Err(HtmlExportError::Builtin(format!(
            "Built-in language {} produced incomplete HTML export spans",
            definition.id
        )));
    }
    serialize_styled(text, tab_width, &spans, &default, &appearances)
        .map_err(HtmlExportError::InvalidTabWidth)
}

pub fn serialize_udl_html(
    text: &str,
    definition: &crate::udl::Definition,
    dark: bool,
    tab_width: usize,
) -> Result<String, HtmlExportError> {
    serialize_udl_html_with_profile(
        text,
        definition,
        tab_width,
        &StyleAppearance::plain(dark),
        0,
    )
}

pub fn serialize_udl_html_with_profile(
    text: &str,
    definition: &crate::udl::Definition,
    tab_width: usize,
    profile: &StyleAppearance,
    zoom_delta: i32,
) -> Result<String, HtmlExportError> {
    let lexed = crate::udl::lex(text, definition);
    let mut appearances = std::collections::BTreeMap::new();
    let spans = lexed
        .spans
        .into_iter()
        .map(|span| {
            appearances.entry(span.style as u16).or_insert_with(|| {
                crate::syntax::udl_appearance_with_profile(
                    definition, span.style, profile, zoom_delta,
                )
            });
            ExportSpan {
                range: span.range,
                style: span.style as u16,
            }
        })
        .collect::<Vec<_>>();
    let default = crate::syntax::udl_appearance_with_profile(
        definition,
        crate::udl::UdlStyleId::Default,
        profile,
        zoom_delta,
    );
    serialize_styled(text, tab_width, &spans, &default, &appearances)
        .map_err(HtmlExportError::InvalidTabWidth)
}

fn css_font_name(name: &str) -> String {
    let mut escaped = String::new();
    for ch in name.chars() {
        if matches!(ch, '\'' | '"' | '\\' | '<' | '>' | '&') || ch.is_control() {
            escaped.push_str(&format!("\\{:X} ", ch as u32));
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

fn serialize_styled(
    text: &str,
    tab_width: usize,
    spans: &[ExportSpan],
    default: &StyleAppearance,
    appearances: &std::collections::BTreeMap<u16, StyleAppearance>,
) -> Result<String, InvalidTabWidth> {
    if tab_width == 0 {
        return Err(InvalidTabWidth);
    }
    let foreground = default.foreground;
    let background = default.background;
    let mut html = format!(
        concat!(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\" \"http://www.w3.org/TR/1999/REC-html401-19991224/strict.dtd\">\r\n",
            "<html>\r\n<head>\r\n<META http-equiv=Content-Type content=\"text/html; charset=UTF-8\">\r\n",
            "<title>Exported from Notepad++</title>\r\n<style type=\"text/css\">\r\n",
            "span {{\r\n\tfont-family: '{}';\r\n\tfont-size: {}px;\r\n\tcolor: #{:06X};\r\n"
        ),
        css_font_name(&default.font_family),
        default.font_size,
        foreground
    );
    if default.bold {
        html.push_str("\tfont-weight: bold;\r\n");
    }
    if default.italic {
        html.push_str("\tfont-style: italic;\r\n");
    }
    if default.underline {
        html.push_str("\ttext-decoration: underline;\r\n");
    }
    html.push_str("}\r\n");
    for (style, appearance) in appearances {
        let fg = appearance.foreground;
        let bg = appearance.background;
        html.push_str(&format!(".sc{style} {{\r\n"));
        if appearance.bold {
            html.push_str("\tfont-weight: bold;\r\n");
        } else if default.bold {
            html.push_str("\tfont-weight: normal;\r\n");
        }
        if appearance.italic {
            html.push_str("\tfont-style: italic;\r\n");
        } else if default.italic {
            html.push_str("\tfont-style: normal;\r\n");
        }
        if appearance.underline {
            html.push_str("\ttext-decoration: underline;\r\n");
        } else if default.underline {
            html.push_str("\ttext-decoration: none;\r\n");
        }
        if appearance.font_family != default.font_family {
            html.push_str(&format!(
                "\tfont-family: '{}';\r\n",
                css_font_name(&appearance.font_family)
            ));
        }
        if appearance.font_size != default.font_size {
            html.push_str(&format!("\tfont-size: {}px;\r\n", appearance.font_size));
        }
        if fg != foreground {
            html.push_str(&format!("\tcolor: #{fg:06X};\r\n"));
        }
        if bg != background {
            html.push_str(&format!("\tbackground: #{bg:06X};\r\n"));
        }
        html.push_str("}\r\n");
    }
    html.push_str("</style>\r\n</head>\r\n<body>\r\n");
    html.push_str(&format!("<div style=\"float: left; white-space: pre; line-height: 1; background: #{background:06X}; \">"));
    let mut span_index = 0;
    let mut last_style = None;
    let mut column = 0;
    for (byte, ch) in text.char_indices() {
        while span_index + 1 < spans.len() && spans[span_index].range.end <= byte {
            span_index += 1;
        }
        let style = spans
            .get(span_index)
            .filter(|span| span.range.contains(&byte))
            .map(|span| span.style)
            .unwrap_or(0);
        if last_style != Some(style) {
            if last_style.is_some() {
                html.push_str("</span>");
            }
            html.push_str(&format!("<span class=\"sc{style}\">"));
            last_style = Some(style);
        }
        match ch {
            '\r' => {
                if text.as_bytes().get(byte + 1) == Some(&b'\n') {
                    column += 1;
                } else {
                    html.push_str("\r\n");
                    column = 0;
                }
            }
            '\n' => {
                html.push_str("\r\n");
                column = 0;
            }
            '\t' => {
                let spaces = tab_width - column % tab_width;
                html.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '<' => {
                html.push_str("&lt;");
                column += 1;
            }
            '>' => {
                html.push_str("&gt;");
                column += 1;
            }
            '&' => {
                html.push_str("&amp;");
                column += 1;
            }
            ch if ch < ' ' => column += 1,
            ch => {
                html.push(ch);
                column += ch.len_utf8();
            }
        }
    }
    if last_style.is_some() {
        html.push_str("</span>");
    }
    html.push_str("</div>");
    html.push_str("</body>\r\n</html>\r\n");
    Ok(html)
}
