use notepad_matrix::html_export::serialize_plain_html;

#[test]
fn calibrated_html_profiles_preserve_real_default_and_explicit_udl_sizes()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::html_export::{
        serialize_html_with_profile, serialize_udl_html_with_profile,
    };
    use notepad_matrix::syntax::{Language, StyleAppearance};
    use notepad_matrix::udl::Definition;
    let mut profile = StyleAppearance::plain(false);
    profile.font_family = "Helvetica".into();
    profile.font_size = 20;
    for language in [Language::NormalText, Language::Json] {
        let html = serialize_html_with_profile("{\"key\":true}", language, false, 4, &profile)?;
        assert!(html.contains("font-family: 'Helvetica';\r\n\tfont-size: 20px;"));
        assert!(!html.contains("font-size: 13px;"));
        if language == Language::Json {
            assert!(html.contains(".sc4 {\r\n\tcolor: #8000FF;\r\n}"));
            assert!(html.contains(".sc11 {\r\n\tfont-weight: bold;\r\n\tcolor: #18AF8A;\r\n}"));
        }
    }
    let mut definition = Definition::default();
    definition.keywords[0] = "token".into();
    definition.styles[4].font_size = 13;
    definition.styles[4].font_name = "Explicit Family".into();
    let html = serialize_udl_html_with_profile("token plain", &definition, 4, &profile, 0)?;
    assert!(html.contains("font-family: 'Helvetica';\r\n\tfont-size: 20px;"));
    assert!(
        html.contains(".sc4 {\r\n\tfont-family: 'Explicit Family';\r\n\tfont-size: 13px;\r\n}")
    );
    profile.font_size = 23;
    let html = serialize_udl_html_with_profile("token plain", &definition, 4, &profile, 3)?;
    assert!(html.contains("font-size: 23px;"));
    assert!(html.contains("font-family: 'Explicit Family';\r\n\tfont-size: 16px;"));
    Ok(())
}

#[test]
fn missing_udl_definition_is_an_error_not_a_plain_text_export() {
    let result = notepad_matrix::html_export::serialize_html(
        "keyword",
        notepad_matrix::syntax::Language::UserDefined(73),
        false,
        4,
    );
    assert!(result.is_err());
    assert!(
        result
            .expect_err("missing definition error")
            .to_string()
            .contains("73")
    );
}

#[test]
fn udl_html_uses_lexed_styles_and_inherited_source_font_attributes()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::udl::{Definition, UdlStyleId};
    let mut definition = Definition::default();
    definition.keywords[0] = "token".into();
    let default = &mut definition.styles[0];
    default.foreground = Some(0x102030);
    default.background = Some(0xf0f1f2);
    default.font_name = "A'B<&>".into();
    default.font_enabled = true;
    default.font_size = 16;
    let keyword = &mut definition.styles[UdlStyleId::Keyword1 as usize];
    keyword.foreground = Some(0xabcdef);
    keyword.background = Some(0x010203);
    keyword.font_style = 7;
    keyword.font_name = "A'B<&>".into();
    keyword.font_size = 21;
    keyword.font_enabled = true;
    let html = notepad_matrix::html_export::serialize_udl_html(
        "token\t<&>中\r\nend",
        &definition,
        false,
        4,
    )?;
    assert!(html.contains("font-family: 'A\\27 B\\3C \\26 \\3E ';"));
    assert!(html.contains("font-size: 16px;"));
    assert!(html.contains(".sc4 {\r\n\tfont-weight: bold;\r\n\tfont-style: italic;\r\n\ttext-decoration: underline;\r\n\tfont-size: 21px;\r\n\tcolor: #ABCDEF;\r\n\tbackground: #010203;\r\n}"));
    assert!(html.contains("<span class=\"sc4\">token</span>"));
    let body = html
        .split("<div ")
        .nth(1)
        .and_then(|tail| tail.split_once('>'))
        .and_then(|(_, tail)| tail.split("</div>").next())
        .ok_or("HTML body")?;
    let mut in_tag = false;
    let visible = body
        .chars()
        .filter(|ch| match ch {
            '<' => {
                in_tag = true;
                false
            }
            '>' => {
                in_tag = false;
                false
            }
            _ => !in_tag,
        })
        .collect::<String>();
    assert_eq!(visible, "token   &lt;&amp;&gt;中\r\nend");
    assert!(!html.contains("A'B<&>"));
    Ok(())
}

#[test]
fn html_file_export_keeps_complete_document_and_omits_clipboard_fragments()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "{\"s\":\"hello &中\",\"n\":1}";
    let html = notepad_matrix::html_export::serialize_html(
        text,
        notepad_matrix::syntax::Language::Json,
        false,
        4,
    )?;
    assert!(html.contains("<span class=\"sc4\">\"s\"</span>"));
    assert!(html.contains("<span class=\"sc2\">\"hello &amp;中\"</span>"));
    assert!(html.contains("<span class=\"sc4\">\"n\"</span>"));
    assert!(html.contains("<span class=\"sc1\">1</span>"));
    assert!(html.contains("<html>\r\n<head>"));
    assert!(html.ends_with("</div></body>\r\n</html>\r\n"));
    assert!(!html.contains("<!--StartFragment-->"));
    assert!(!html.contains("<!--EndFragment-->"));
    Ok(())
}

#[test]
fn json_html_export_uses_real_style_runs_and_source_css_differences()
-> Result<(), Box<dyn std::error::Error>> {
    let html = notepad_matrix::html_export::serialize_html(
        "{\"key\":true,\"s\":\"<&>\\n\"}\nwrong",
        notepad_matrix::syntax::Language::Json,
        false,
        4,
    )?;
    assert!(html.contains(".sc4 {\r\n\tcolor: #8000FF;\r\n}"));
    assert!(html.contains(".sc11 {\r\n\tfont-weight: bold;\r\n\tcolor: #18AF8A;\r\n}"));
    assert!(html.contains(".sc13 {\r\n\tbackground: #FFA448;\r\n}"));
    assert!(html.contains("<span class=\"sc4\">\"key\"</span>"));
    assert!(html.contains("<span class=\"sc11\">true</span>"));
    assert!(html.contains("<span class=\"sc5\">\\n</span>"));
    assert!(html.contains("&lt;&amp;&gt;"));
    assert!(html.contains("<span class=\"sc13\">wrong</span>"));
    Ok(())
}

#[test]
fn plain_html_export_uses_source_escaping_byte_tab_columns_and_crlf_output()
-> Result<(), Box<dyn std::error::Error>> {
    let html = serialize_plain_html("<&>\t中\t😀\r\nB\rC\n\u{1}\tD", false, 4)?;
    assert!(html.starts_with("<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\""));
    assert!(html.contains("charset=UTF-8"));
    assert!(html.contains("font-family: 'Menlo'"));
    assert!(html.contains("color: #202020"));
    assert!(html.contains("background: #FFFFFF"));
    assert!(html.contains("<span class=\"sc0\">&lt;&amp;&gt; 中 😀\r\nB\r\nC\r\n   D</span>"));
    let body = html.split("<body>\r\n").nth(1).ok_or("HTML body")?;
    assert!(!body.contains('\t'));
    assert!(!html.contains('\u{1}'));
    let hostile = serialize_plain_html("<script>alert(\"x\")</script>&", true, 4)?;
    assert!(!hostile.contains("<script>"));
    assert!(hostile.contains("&lt;script&gt;alert(\"x\")&lt;/script&gt;&amp;"));
    assert!(hostile.contains("background: #202020"));
    assert!(serialize_plain_html("text", false, 0).is_err());
    Ok(())
}
