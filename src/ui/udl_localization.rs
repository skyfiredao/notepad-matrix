use crate::udl::nesting;
use crate::udl_controls::{Control, Kind, Page};

pub(super) fn control_label(control: &Control, locale: &str) -> &'static str {
    if let Some(label) = control_source(control)
        .and_then(|(path, id)| crate::localization::dialog_control(locale, path, id))
    {
        return label;
    }
    let resource = match (control.path.as_str(), &control.kind) {
        ("/dark_mode_theme", _) => Some(("matrix-udl-dark-mode-language", "Dark mode language")),
        (path, kind) if path.starts_with("/styles/") => {
            match (path.rsplit('/').next(), kind) {
                (Some("color_style"), Kind::Bit(1)) => Some(("matrix-udl-use-foreground", "Use foreground")),
                (Some("color_style"), Kind::Bit(2)) => Some(("matrix-udl-use-background", "Use background")),
                (Some("font_enabled"), _) => Some(("matrix-udl-enable-font", "Enable font")),
                (Some("nesting"), Kind::Bit(bit)) => {
                    if *bit == nesting::code2(0) { Some(("matrix-udl-allow-code2-open", "Allow code 2 open")) }
                    else if *bit == nesting::code2(1) { Some(("matrix-udl-allow-code2-middle", "Allow code 2 middle")) }
                    else if *bit == nesting::code2(2) { Some(("matrix-udl-allow-code2-close", "Allow code 2 close")) }
                    else if *bit == nesting::comment_fold(0) { Some(("matrix-udl-allow-comment-fold-open", "Allow comment fold open")) }
                    else if *bit == nesting::comment_fold(1) { Some(("matrix-udl-allow-comment-fold-middle", "Allow comment fold middle")) }
                    else if *bit == nesting::comment_fold(2) { Some(("matrix-udl-allow-comment-fold-close", "Allow comment fold close")) }
                    else { None }
                }
                _ => None,
            }
        }
        _ => None,
    };
    resource.map(|(id, fallback)| crate::localization::localized_string(locale, id).unwrap_or(fallback))
        .unwrap_or(control.label)
}

pub(super) fn page_path(page: Page) -> &'static str {
    match page {
        Page::FolderDefault => "UserDefine/Folder",
        Page::Keywords => "UserDefine/Keywords",
        Page::CommentsNumbers => "UserDefine/Comment",
        Page::OperatorsDelimiters => "UserDefine/Operator",
    }
}

pub(super) fn control_source(control: &Control) -> Option<(&'static str, u32)> {
    let path = control.path.as_str();
    let direct = match path {
        "/extensions" => Some(("UserDefine", 20009)),
        "/case_ignored" => Some(("UserDefine", 20012)),
        "/fold_compact" => Some(("UserDefine/Folder", 21106)),
        "/line_comment_placement" => Some(("UserDefine/Comment", 23003)),
        "/allow_fold_comments" => Some(("UserDefine/Comment", 23001)),
        "/comments/line_open" => Some(("UserDefine/Comment", 23323)),
        "/comments/line_continue" => Some(("UserDefine/Comment", 23324)),
        "/comments/line_close" => Some(("UserDefine/Comment", 23325)),
        "/comments/block_open" => Some(("UserDefine/Comment", 23122)),
        "/comments/block_close" => Some(("UserDefine/Comment", 23123)),
        "/numbers/prefix1" => Some(("UserDefine/Comment", 23230)),
        "/numbers/prefix2" => Some(("UserDefine/Comment", 23232)),
        "/numbers/extras1" => Some(("UserDefine/Comment", 23234)),
        "/numbers/extras2" => Some(("UserDefine/Comment", 23236)),
        "/numbers/suffix1" => Some(("UserDefine/Comment", 23238)),
        "/numbers/suffix2" => Some(("UserDefine/Comment", 23240)),
        "/numbers/range" => Some(("UserDefine/Comment", 23242)),
        "/decimal_separator" => Some(("UserDefine/Comment", 23244)),
        "/operators/0" => Some(("UserDefine/Operator", 24116)),
        "/operators/1" => Some(("UserDefine/Operator", 24117)),
        _ => None,
    };
    if direct.is_some() { return direct; }
    let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["folding", index, field] => {
            let index = index.parse::<usize>().ok()?;
            let base = *[21224, 21324, 21424].get(index)?;
            let offset = match *field { "open" => 0, "middle" => 1, "close" => 2, _ => return None };
            Some(("UserDefine/Folder", base + offset))
        }
        ["keywords", index] | ["prefix_keywords", index] => {
            let index = index.parse::<usize>().ok()?;
            let base = *[22101, 22201, 22301, 22401, 22451, 22501, 22551, 22601].get(index)?;
            Some(("UserDefine/Keywords", base + if parts[0] == "prefix_keywords" { 20 } else { 0 }))
        }
        ["delimiters", index, field] => {
            let index = index.parse::<usize>().ok()?;
            let base = *[24220, 24320, 24420, 24470, 24520, 24570, 24620, 24670].get(index)?;
            let offset = match *field { "open" => 0, "escape" => 1, "close" => 2, _ => return None };
            Some(("UserDefine/Operator", base + offset))
        }
        ["styles", _, field] => {
            let id = match (*field, &control.kind) {
                ("foreground", _) => 25006,
                ("background", _) => 25007,
                ("font_name", _) => 25031,
                ("font_size", _) => 25032,
                ("font_style", Kind::Bit(1)) => 25001,
                ("font_style", Kind::Bit(2)) => 25002,
                ("font_style", Kind::Bit(4)) => 25003,
                ("nesting", Kind::Bit(bit)) => {
                    let delimiter = (0..8).find(|index| nesting::delimiter(*index) == *bit);
                    let keyword = (0..8).find(|index| nesting::keyword(*index) == *bit);
                    if let Some(index) = delimiter { 25008 + index as u32 }
                    else if let Some(index) = keyword { 25018 + index as u32 }
                    else if *bit == nesting::COMMENT { 25016 }
                    else if *bit == nesting::COMMENT_LINE { 25017 }
                    else if *bit == nesting::OPERATORS1 { 25026 }
                    else if *bit == nesting::OPERATORS2 { 25027 }
                    else if *bit == nesting::NUMBERS { 25028 }
                    else { return None; }
                }
                _ => return None,
            };
            Some(("UserDefine/StylerDialog", id))
        }
        _ => None,
    }
}

pub(super) fn choice_source(control: &Control, index: usize) -> Option<(&'static str, u32)> {
    let base = match control.path.as_str() {
        "/line_comment_placement" => 23004,
        "/decimal_separator" => 23245,
        _ => return None,
    };
    (index < 3).then_some(("UserDefine/Comment", base + index as u32))
}
