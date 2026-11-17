use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::Mutex;
use std::io;

use crate::style_config::Node;

const DEFINITIONS: &str = include_str!("../assets/localization-languages.txt");
const ENGLISH_MENU_NAMES: &str = include_str!("../assets/contextMenu-menuNames.xml");

macro_rules! sources {
    ($($filename:literal),* $(,)?) => {
        const SOURCES: &[(&str, &str)] = &[$(($filename, include_str!(concat!("../assets/nativeLang/", $filename)))),*];
    };
}

sources![
    "english.xml",
    "english_customizable.xml",
    "french.xml",
    "taiwaneseMandarin.xml",
    "chineseSimplified.xml",
    "korean.xml",
    "japanese.xml",
    "german.xml",
    "spanish.xml",
    "italian.xml",
    "portuguese.xml",
    "brazilian_portuguese.xml",
    "dutch.xml",
    "russian.xml",
    "polish.xml",
    "catalan.xml",
    "czech.xml",
    "hungarian.xml",
    "romanian.xml",
    "turkish.xml",
    "farsi.xml",
    "ukrainian.xml",
    "hebrew.xml",
    "nynorsk.xml",
    "norwegian.xml",
    "occitan.xml",
    "thai.xml",
    "friulian.xml",
    "arabic.xml",
    "finnish.xml",
    "lithuanian.xml",
    "greek.xml",
    "swedish.xml",
    "galician.xml",
    "slovenian.xml",
    "slovak.xml",
    "danish.xml",
    "extremaduran.xml",
    "samogitian.xml",
    "bulgarian.xml",
    "indonesian.xml",
    "albanian.xml",
    "croatian.xml",
    "georgian.xml",
    "basque.xml",
    "spanish_ar.xml",
    "belarusian.xml",
    "serbian.xml",
    "serbianCyrillic.xml",
    "malay.xml",
    "malayalam.xml",
    "luxembourgish.xml",
    "tagalog.xml",
    "afrikaans.xml",
    "kazakh.xml",
    "uzbek.xml",
    "uzbekCyrillic.xml",
    "kyrgyz.xml",
    "macedonian.xml",
    "latvian.xml",
    "tamil.xml",
    "azerbaijani.xml",
    "bosnian.xml",
    "esperanto.xml",
    "ligurian.xml",
    "hindi.xml",
    "sardinian.xml",
    "uyghur.xml",
    "telugu.xml",
    "aragonese.xml",
    "bengali.xml",
    "sinhala.xml",
    "kabyle.xml",
    "marathi.xml",
    "vietnamese.xml",
    "aranese.xml",
    "gujarati.xml",
    "mongolian.xml",
    "urdu.xml",
    "kannada.xml",
    "welsh.xml",
    "estonian.xml",
    "tajikCyrillic.xml",
    "tatar.xml",
    "punjabi.xml",
    "corsican.xml",
    "breton.xml",
    "kurdish.xml",
    "piglatin.xml",
    "zulu.xml",
    "venetian.xml",
    "irish.xml",
    "nepali.xml",
    "hongKongCantonese.xml",
    "abkhazian.xml",
];

pub(crate) struct Language {
    pub filename: &'static str,
    pub name: &'static str,
    xml: &'static str,
    translation: OnceLock<Result<Translation, String>>,
}

#[derive(Default)]
struct Dialog {
    name: Option<String>,
    title: Option<String>,
    message: Option<String>,
    controls: BTreeMap<u32, String>,
    combos: BTreeMap<u32, Vec<String>>,
}

struct Translation {
    dialogs: BTreeMap<String, Dialog>,
    commands: BTreeMap<u32, String>,
    menu_roots: BTreeMap<String, String>,
    menu_submenus: BTreeMap<String, String>,
    strings: BTreeMap<String, String>,
    rtl: Option<bool>,
}

fn walk_dialogs(root: &Node, mut visit: impl FnMut(&Node, &str)) {
    let mut path = String::new();
    let mut pending = vec![(root.children.iter(), 0)];
    while let Some((children, parent_len)) = pending.last_mut() {
        path.truncate(*parent_len);
        let Some(node) = children.next() else {
            pending.pop();
            continue;
        };
        if matches!(node.name.as_str(), "Item" | "ComboBox" | "Element") {
            continue;
        }
        if !path.is_empty() { path.push('/'); }
        path.push_str(&node.name);
        visit(node, &path);
        pending.push((node.children.iter(), path.len()));
    }
}

static PRIVATE_LANGUAGE: Mutex<Option<(String, &'static Translation)>> = Mutex::new(None);

pub(crate) fn load_private(store: &crate::config::ConfigStore) -> io::Result<Option<&'static str>> {
    let path = store.root().join("nativeLang.xml");
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {},
        Ok(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "nativeLang.xml must be a regular private file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let document = match crate::style_config::read_xml(&path) {
        Ok(document) => document,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let filename = document.child("Native-Langue").and_then(|native| native.attribute("filename"))
        .and_then(canonical_filename).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Native-Langue filename is unknown"))?;
    let parsed = Translation::from_document(&document).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut private = PRIVATE_LANGUAGE.lock().map_err(|error| io::Error::other(error.to_string()))?;
    *private = Some((filename.to_owned(), Box::leak(Box::new(parsed))));
    Ok(Some(filename))
}

pub(crate) fn select_private(store: &crate::config::ConfigStore, filename: &str) -> io::Result<()> {
    let filename = validate(filename).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let language = languages().iter().find(|language| language.filename == filename)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Interface language is unavailable"))?;
    let parsed = language.translation.get_or_init(|| Translation::parse(language.xml)).as_ref()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.clone()))?;
    let mut private = PRIVATE_LANGUAGE.lock().map_err(|error| io::Error::other(error.to_string()))?;
    if private.as_ref().is_some_and(|(selected, _)| selected == filename) { return Ok(()); }
    let path = store.root().join("nativeLang.xml");
    store.document_path(std::path::Path::new("settings.json"))?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {},
        Ok(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "nativeLang.xml must be a regular private file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    if filename == "english.xml" {
        match std::fs::remove_file(path) {
            Ok(()) => {},
            Err(error) if error.kind() == io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
    } else {
        use std::io::Write;
        let mut temporary = tempfile::NamedTempFile::new_in(store.root())?;
        temporary.write_all(language.xml.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
    }
    *private = Some((filename.to_owned(), parsed));
    Ok(())
}

pub(crate) fn languages() -> &'static [Language] {
    static LANGUAGES: OnceLock<Vec<Language>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        SOURCES
            .iter()
            .filter_map(|(filename, xml)| {
                let name = DEFINITIONS.lines().find_map(|line| {
                    let (source_filename, name) = line.split_once('\t')?;
                    (source_filename == *filename).then_some(name)
                })?;
                Some(Language {
                    filename,
                    name,
                    xml,
                    translation: OnceLock::new(),
                })
            })
            .collect()
    })
}

pub(crate) fn canonical_filename(filename: &str) -> Option<&'static str> {
    let filename = if filename.is_empty() {
        "english.xml"
    } else {
        filename
    };
    languages()
        .iter()
        .find(|language| language.filename.eq_ignore_ascii_case(filename))
        .map(|language| language.filename)
}

pub(crate) fn validate(filename: &str) -> Result<&'static str, String> {
    let filename = canonical_filename(filename)
        .ok_or_else(|| format!("Unknown interface language: {filename}"))?;
    translation(filename)?;
    Ok(filename)
}

fn translation(filename: &str) -> Result<&'static Translation, String> {
    let filename = canonical_filename(filename)
        .ok_or_else(|| format!("Unknown interface language: {filename}"))?;
    if let Some(translation) = PRIVATE_LANGUAGE.lock().map_err(|error| error.to_string())?
        .as_ref().filter(|(selected, _)| selected == filename).map(|(_, translation)| *translation) {
        return Ok(translation);
    }
    catalog_translation(filename)
}

fn catalog_translation(filename: &str) -> Result<&'static Translation, String> {
    let filename = canonical_filename(filename).ok_or_else(|| format!("Unknown interface language: {filename}"))?;
    let language = languages()
        .iter()
        .find(|language| language.filename == filename)
        .ok_or_else(|| format!("Interface language is unavailable: {filename}"))?;
    language
        .translation
        .get_or_init(|| Translation::parse(language.xml))
        .as_ref()
        .map_err(Clone::clone)
}

fn translated<T>(filename: &str, get: impl Fn(&'static Translation) -> Option<T>) -> Option<T> {
    let filename = canonical_filename(filename)?;
    let private = match PRIVATE_LANGUAGE.lock() {
        Ok(private) => private.as_ref().filter(|(selected, _)| selected == filename).map(|(_, translation)| *translation),
        Err(error) => {
            eprintln!("Private interface language cannot be read: {error}");
            None
        }
    };
    private.and_then(&get)
        .or_else(|| catalog_translation(filename).ok().and_then(&get))
        .or_else(|| catalog_translation("english.xml").ok().and_then(get))
}

impl Translation {
    fn parse(xml: &str) -> Result<Self, String> {
        let document = Node::parse(xml).map_err(|error| error.to_string())?;
        Self::from_document(&document)
    }

    fn from_document(document: &Node) -> Result<Self, String> {
        if document.name != "NotepadPlus" { return Err("Native language root must be NotepadPlus".to_owned()); }
        let native = document
            .child("Native-Langue")
            .ok_or_else(|| "Native-Langue is missing".to_owned())?;
        let mut translation = Self {
            dialogs: BTreeMap::new(),
            commands: BTreeMap::new(),
            menu_roots: BTreeMap::new(),
            menu_submenus: BTreeMap::new(),
            strings: BTreeMap::new(),
            rtl: native.attribute("RTL").map(|rtl| rtl == "yes"),
        };
        for category in ["Dialog", "MessageBox"] {
            if let Some(root) = native.child(category) {
                walk_dialogs(root, |node, path| translation.collect_dialog(node, path));
            }
        }
        if let Some(main) = native.child("Menu").and_then(|menu| menu.child("Main")) {
            if let Some(commands) = main.child("Commands") {
                translation.commands = control_names(commands);
            }
            for (tag, attribute, target) in [
                ("Entries", "menuId", &mut translation.menu_roots),
                ("SubEntries", "subMenuId", &mut translation.menu_submenus),
            ] {
                if let Some(entries) = main.child(tag) {
                    for item in &entries.children {
                        if let (Some(id), Some(name)) =
                            (item.attribute(attribute), item.attribute("name"))
                        {
                            target.insert(id.to_owned(), display_label(name));
                        }
                    }
                }
            }
        }
        if let Some(strings) = native.child("MiscStrings") {
            for item in &strings.children {
                if let Some(value) = item.attribute("value").or_else(|| item.attribute("name")) {
                    translation
                        .strings
                        .insert(item.name.clone(), value.to_owned());
                }
            }
        }
        Ok(translation)
    }

    fn collect_dialog(&mut self, node: &Node, path: &str) {
        let label: fn(&str) -> String = if path == "Preference/Print" { display_label } else { dialog_label };
        let mut dialog = Dialog {
            name: node.attribute("name").map(label),
            title: node.attribute("title").map(label),
            message: node.attribute("message").map(str::to_owned),
            controls: control_names_with(node, label),
            combos: BTreeMap::new(),
        };
        for combo in node
            .children
            .iter()
            .filter(|child| child.name == "ComboBox")
        {
            if let Some(id) = combo.attribute("id").and_then(|id| id.parse::<u32>().ok()) {
                dialog.combos.insert(
                    id,
                    combo
                        .children
                        .iter()
                        .filter_map(|element| element.attribute("name").map(display_label))
                        .collect(),
                );
            }
        }
        self.dialogs.insert(path.to_owned(), dialog);
    }
}

fn control_names(node: &Node) -> BTreeMap<u32, String> {
    control_names_with(node, display_label)
}

fn control_names_with(node: &Node, label: fn(&str) -> String) -> BTreeMap<u32, String> {
    node.children
        .iter()
        .filter(|child| child.name == "Item")
        .filter_map(|item| {
            Some((
                item.attribute("id")?.parse().ok()?,
                label(item.attribute("name")?),
            ))
        })
        .collect()
}

pub(crate) fn display_label(label: &str) -> String {
    let mut output = String::new();
    let mut chars = label.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                output.push('&');
            }
        } else {
            output.push(ch);
        }
    }
    output.trim().to_owned()
}

fn dialog_label(label: &str) -> String {
    let mut text = String::new();
    let mut chars = label.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '(' && chars.peek() == Some(&'&') {
            let mut suffix = chars.clone();
            suffix.next();
            if suffix.next().is_some_and(|ch| ch.is_ascii_alphanumeric()) && suffix.next() == Some(')') {
                chars = suffix;
                continue;
            }
        }
        text.push(ch);
    }
    display_label(&text)
}

fn label_key(label: &str) -> String {
    let display = display_label(label);
    let mut chars = display.chars().peekable();
    let mut text = String::new();
    while let Some(ch) = chars.next() {
        if ch == '(' {
            let mut suffix = chars.clone();
            if suffix.next().is_some_and(|ch| ch.is_ascii_alphanumeric()) && suffix.next() == Some(')') {
                chars = suffix;
                continue;
            }
        }
        text.push(ch);
    }
    text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| match ch {
            '（' => '(',
            '）' => ')',
            '：' => ':',
            '，' | '、' => ',',
            _ => ch,
        })
        .collect()
}

pub(crate) fn preference_section(page: usize) -> Option<&'static str> {
    [
        "Global",
        "Toolbar",
        "Tabbar",
        "Scintillas",
        "Scintillas2",
        "DarkMode",
        "MarginsBorderEdge",
        "Highlighting",
        "NewDoc",
        "DefaultDir",
        "RecentFilesHistory",
        "FileAssoc",
        "Language",
        "Indentation",
        "Print",
        "Searching",
        "Backup",
        "AutoCompletion",
        "MultiInstance",
        "Delimiter",
        "Performance",
        "Cloud",
        "SearchEngine",
        "MISC",
    ]
    .get(page)
    .copied()
}

pub(crate) fn dialog_control(filename: &str, path: &str, id: u32) -> Option<&'static str> {
    translated(filename, |translation| translation.dialogs.get(path)?.controls.get(&id).map(String::as_str))
}

pub(crate) fn dialog_title(filename: &str, path: &str) -> Option<&'static str> {
    translated(filename, |translation| translation.dialogs.get(path)?.title.as_deref())
}

pub(crate) fn dialog_name(filename: &str, path: &str) -> Option<&'static str> {
    translated(filename, |translation| translation.dialogs.get(path)?.name.as_deref())
}

pub(crate) fn dialog_message(filename: &str, path: &str) -> Option<&'static str> {
    translated(filename, |translation| translation.dialogs.get(path)?.message.as_deref())
}

pub(crate) fn preference_control(filename: &str, section: &str, id: u32) -> Option<&'static str> {
    let path = if section.is_empty() {
        "Preference".to_owned()
    } else {
        format!("Preference/{section}")
    };
    dialog_control(filename, &path, id)
}

pub(crate) fn preference_title(filename: &str, section: &str) -> Option<&'static str> {
    let path = if section.is_empty() {
        "Preference".to_owned()
    } else {
        format!("Preference/{section}")
    };
    dialog_title(filename, &path)
}

pub(crate) fn preference_label(
    filename: &str,
    section: &str,
    label: &str,
) -> Option<&'static str> {
    let path = if section.is_empty() {
        "Preference".to_owned()
    } else {
        format!("Preference/{section}")
    };
    let alias = match (section, label) {
        ("DefaultDir", "默认打开/保存目录") => Some(6413),
        ("DefaultDir", "跟随当前文件") => Some(6414),
        ("DefaultDir", "记住上次使用的目录") => Some(6415),
        ("DefaultDir", "拖放文件夹时打开其中所有文件，而非文件夹工作区") => Some(6431),
        ("RecentFilesHistory", "启动时不检查文件") => Some(6305),
        ("RecentFilesHistory", "最多记录数:") => Some(6306),
        ("Backup", "会话快照与定时备份") => Some(6817),
        ("Backup", "启用会话快照与定时备份") => Some(6818),
        ("Backup", "记住当前会话供下次启动使用") => Some(6309),
        ("Backup", "文件修改后每隔") => Some(6819),
        ("Backup", "秒备份一次") => Some(6821),
        ("Backup", "记住上次会话中无法访问的文件") => Some(6825),
        ("Backup", "无") => Some(6315),
        ("Backup", "详细备份") => Some(6317),
        ("Backup", "自定义备份目录") => Some(6804),
        ("MultiInstance", "在新实例中打开会话（退出时自动保存会话）") => Some(6152),
        ("MultiInstance", "颠倒日期和时间的先后顺序（短格式与长格式）") => Some(6175),
        ("MultiInstance", "面板状态与 [-nosession] *") => Some(6181),
        ("MultiInstance", "在其他实例或使用 [-nosession] 参数时，记住已打开的面板状态。") => Some(6182),
        ("MultiInstance", "文件列表") => Some(6184),
        ("MultiInstance", "字符面板") => Some(6185),
        ("MultiInstance", "模块面板") => Some(6190),
        ("MultiInstance", "* 修改此设置需要重启 Notepad Matrix 才能生效") => Some(6155),
        ("Performance", "启用大文件限制（不使用语法高亮）") => Some(7143),
        ("Performance", "全局停用自动换行") => Some(7150),
        ("Performance", "允许可点击链接") => Some(7151),
        ("Performance", "打开 ≥2GB 文件时不显示警告") => Some(7152),
        ("Performance", "MB   (1 – 2046)") => Some(7146),
        ("MISC", "文档速览") => Some(6344),
        ("MISC", "全部保存时弹出确认对话框") => Some(6361),
        ("MISC", "允许文件夹工作区载入符号链接") => Some(6366),
        ("Scintillas2", "启用多重编辑（Ctrl+鼠标点选）") => Some(6522),
        ("Scintillas2", "将外观设置应用到 C0、C1 以及 Unicode 行尾符") => Some(6258),
        ("Delimiter", "分隔符选择设置（Ctrl + 双击鼠标左键）") => Some(6251),
        _ => None,
    };
    if let Some(id) = alias { return dialog_control(filename, &path, id); }
    let key = label_key(label);
    let references = ["chineseSimplified.xml", "english.xml"].map(|filename| {
        catalog_translation(filename).ok().and_then(|translation| translation.dialogs.get(&path))
    });
    let mut id = None;
    for dialog in references.iter().flatten() {
        for (&candidate, _) in dialog.controls.iter().filter(|(_, name)| label_key(name) == key) {
            if id.is_some_and(|id| id != candidate) { return None; }
            id = Some(candidate);
        }
    }
    if let Some(id) = id {
        return dialog_control(filename, &path, id);
    }
    let mut element = None;
    for dialog in references.iter().flatten() {
        for (&id, elements) in &dialog.combos {
            for (index, _) in elements.iter().enumerate().filter(|(_, label)| label_key(label) == key) {
                let candidate = (id, index);
                if element.is_some_and(|element| element != candidate) { return None; }
                element = Some(candidate);
            }
        }
    }
    if let Some((id, index)) = element {
        return combo_element(filename, &path, id, index);
    }
    references.iter().flatten().any(|dialog| dialog.title.as_ref().is_some_and(|title| label_key(title) == key))
        .then(|| dialog_title(filename, &path)).flatten()
}

pub(crate) fn combo_element(
    filename: &str,
    path: &str,
    id: u32,
    index: usize,
) -> Option<&'static str> {
    translated(filename, |translation| translation.dialogs.get(path)?.combos.get(&id)?.get(index).map(String::as_str))
}

pub(crate) fn menu_command(filename: &str, id: u32) -> Option<&'static str> {
    static ENGLISH: OnceLock<BTreeMap<u32, String>> = OnceLock::new();
    translated(filename, |translation| translation.commands.get(&id).map(String::as_str))
        .or_else(|| {
            ENGLISH
                .get_or_init(|| match Node::parse(ENGLISH_MENU_NAMES) {
                    Ok(document) => document
                        .child("Native-Langue")
                        .and_then(|node| node.child("Menu"))
                        .and_then(|node| node.child("Main"))
                        .and_then(|node| node.child("Commands"))
                        .map(control_names)
                        .unwrap_or_default(),
                    Err(error) => {
                        eprintln!("English menu names cannot be read: {error}");
                        BTreeMap::new()
                    }
                })
                .get(&id).map(String::as_str)
        })
}

pub(crate) fn menu_root(filename: &str, id: &str) -> Option<&'static str> {
    translated(filename, |translation| translation.menu_roots.get(id).map(String::as_str))
}

pub(crate) fn menu_submenu(filename: &str, id: &str) -> Option<&'static str> {
    translated(filename, |translation| translation.menu_submenus.get(id).map(String::as_str))
}

pub(crate) fn localized_string(filename: &str, id: &str) -> Option<&'static str> {
    translated(filename, |translation| translation.strings.get(id).map(String::as_str))
}

pub(crate) fn is_rtl(filename: &str) -> bool {
    translated(filename, |translation| translation.rtl).unwrap_or(false)
}
thread_local! {
    static PREFERENCE_SCOPE: std::cell::RefCell<Option<(String, String)>> = const { std::cell::RefCell::new(None) };
}

struct PreferenceScope(Option<(String, String)>);

impl Drop for PreferenceScope {
    fn drop(&mut self) {
        PREFERENCE_SCOPE.with(|scope| *scope.borrow_mut() = self.0.take());
    }
}

pub(crate) fn with_preference<R>(filename: &str, section: &str, build: impl FnOnce() -> R) -> R {
    let previous = PREFERENCE_SCOPE
        .with(|scope| scope.replace(Some((filename.to_owned(), section.to_owned()))));
    let _scope = PreferenceScope(previous);
    build()
}

pub(crate) fn current_preference_label(chinese: &str) -> Option<&'static str> {
    let (filename, section) = PREFERENCE_SCOPE.with(|scope| scope.borrow().clone())?;
    preference_label(&filename, &section, chinese)
}

pub(crate) fn current_preference_control(id: u32) -> Option<&'static str> {
    let (filename, section) = PREFERENCE_SCOPE.with(|scope| scope.borrow().clone())?;
    preference_control(&filename, &section, id)
}

pub(crate) fn current_preference_combo(id: u32, index: usize) -> Option<&'static str> {
    let (filename, section) = PREFERENCE_SCOPE.with(|scope| scope.borrow().clone())?;
    combo_element(&filename, &format!("Preference/{section}"), id, index)
}

pub(crate) fn current_preference_rtl() -> bool {
    PREFERENCE_SCOPE.with(|scope| scope.borrow().as_ref().is_some_and(|(filename, _)| is_rtl(filename)))
}

pub(crate) fn current_preference_source_label<'a>(id: &str, label: &'a str) -> &'a str {
    let source = match id {
        "preferences-close" | "IDC_BUTTON_CLOSE" => Some(("", 6001)),
        "pref-hide-menu" | "IDC_CHECK_HIDEMENUBAR" => Some(("Global", 6122)),
        "pref-hide-menu-shortcuts" | "IDC_CHECK_HIDERIGHTSHORTCUTSOFMENUBAR" => Some(("Global", 6132)),
        "pref-hide-status" | "IDC_CHECK_HIDESTATUSBAR" => Some(("Global", 6134)),
        "pref-hide-toolbar" | "IDC_CHECK_HIDE" => Some(("Toolbar", 6102)),
        "pref-toolbar-small" | "IDC_RADIO_SMALLICON" => Some(("Toolbar", 6103)),
        "pref-toolbar-large" | "IDC_RADIO_BIGICON" => Some(("Toolbar", 6104)),
        "pref-toolbar-standard" | "IDC_RADIO_STANDARD" => Some(("Toolbar", 6105)),
        "pref-toolbar-small2" | "IDC_RADIO_SMALLICON2" => Some(("Toolbar", 6129)),
        "pref-toolbar-large2" | "IDC_RADIO_BIGICON2" => Some(("Toolbar", 6130)),
        "IDC_TOOLBAR_GB_COLORIZATION" => Some(("Toolbar", 6011)),
        "pref-toolbar-complete" | "IDC_RADIO_COMPLETE" => Some(("Toolbar", 6012)),
        "pref-toolbar-partial" | "IDC_RADIO_PARTIAL" => Some(("Toolbar", 6013)),
        "IDC_TOOLBAR_GB_COLORCHOICE" => Some(("Toolbar", 6014)),
        "pref-toolbar-default" | "IDC_RADIO_DEFAULTCOLOR" => Some(("Toolbar", 6015)),
        "pref-toolbar-red" | "IDC_RADIO_RED" => Some(("Toolbar", 6016)),
        "pref-toolbar-green" | "IDC_RADIO_GREEN" => Some(("Toolbar", 6017)),
        "pref-toolbar-blue" | "IDC_RADIO_BLUE" => Some(("Toolbar", 6018)),
        "pref-toolbar-purple" | "IDC_RADIO_PURPLE" => Some(("Toolbar", 6019)),
        "pref-toolbar-cyan" | "IDC_RADIO_CYAN" => Some(("Toolbar", 6020)),
        "pref-toolbar-olive" | "IDC_RADIO_OLIVE" => Some(("Toolbar", 6021)),
        "pref-toolbar-yellow" | "IDC_RADIO_YELLOW" => Some(("Toolbar", 6022)),
        "pref-toolbar-accent" | "IDC_RADIO_ACCENTCOLOR" => Some(("Toolbar", 6023)),
        "pref-toolbar-custom" | "IDC_RADIO_CUSTOMCOLOR" => Some(("Toolbar", 6024)),
        "pref-eol-color" | "IDC_CHECK_WITHCUSTOMCOLOR_CRLF" => Some(("Scintillas2", 6250)),
        "pref-npc-color" | "IDC_CHECK_NPC_COLOR" => Some(("Scintillas2", 6256)),
        "pref-mark-case" | "IDC_CHECK_MARKALLCASESENSITIVE" => Some(("Highlighting", 6352)),
        "pref-mark-word" | "IDC_CHECK_MARKALLWHOLEWORDONLY" => Some(("Highlighting", 6353)),
        "pref-smart-case" | "IDC_CHECK_SMARTHILITECASESENSITIVE" => Some(("Highlighting", 6332)),
        "pref-smart-word" | "IDC_CHECK_SMARTHILITEWHOLEWORDONLY" => Some(("Highlighting", 6338)),
        "pref-smart-enabled" | "IDC_CHECK_ENABLSMARTHILITE" => Some(("Highlighting", 6326)),
        "pref-tags-enabled" | "IDC_CHECK_ENABLTAGSMATCHHILITE" => Some(("Highlighting", 6327)),
        "IDC_SUPPORTEDEXTS_STATIC" => Some(("FileAssoc", 4009)),
        "IDC_REGISTEREDEXTS_STATIC" => Some(("FileAssoc", 4010)),
        "pref-print-line-numbers" | "IDC_CHECK_PRINTLINENUM" => Some(("Print", 6601)),
        "IDC_COLOUROPT_STATIC" => Some(("Print", 6602)),
        "pref-print-normal" | "IDC_RADIO_WYSIWYG" => Some(("Print", 6603)),
        "pref-print-invert" | "IDC_RADIO_INVERT" => Some(("Print", 6604)),
        "pref-print-bw" | "IDC_RADIO_BW" => Some(("Print", 6605)),
        "pref-print-no-bg" | "IDC_RADIO_NOBG" => Some(("Print", 6606)),
        "IDC_MARGESETTINGS_STATIC" => Some(("Print", 6607)),
        "pref-print-margin-left" | "IDC_ML_STATIC" => Some(("Print", 6612)),
        "pref-print-margin-top" | "IDC_MT_STATIC" => Some(("Print", 6613)),
        "pref-print-margin-right" | "IDC_MR_STATIC" => Some(("Print", 6614)),
        "pref-print-margin-bottom" | "IDC_MB_STATIC" => Some(("Print", 6615)),
        "pref-print-form-feed" | "IDC_CHECK_PRINTFORMFEED" => Some(("Print", 6616)),
        "pref-print-header-bold" | "IDC_CHECK_HBOLD" => Some(("Print", 6706)),
        "pref-print-header-italic" | "IDC_CHECK_HITALIC" => Some(("Print", 6707)),
        "IDC_HGB_STATIC" => Some(("Print", 6708)),
        "IDC_HL_STATIC" => Some(("Print", 6709)),
        "IDC_HM_STATIC" => Some(("Print", 6710)),
        "IDC_HR_STATIC" => Some(("Print", 6711)),
        "pref-print-footer-bold" | "IDC_CHECK_FBOLD" => Some(("Print", 6717)),
        "pref-print-footer-italic" | "IDC_CHECK_FITALIC" => Some(("Print", 6718)),
        "IDC_FGB_STATIC" => Some(("Print", 6719)),
        "IDC_FL_STATIC" => Some(("Print", 6720)),
        "IDC_FM_STATIC" => Some(("Print", 6721)),
        "IDC_FR_STATIC" => Some(("Print", 6722)),
        "pref-print-add-variable" | "IDC_BUTTON_ADDVAR" => Some(("Print", 6723)),
        "IDC_VAR_STATIC" => Some(("Print", 6725)),
        "pref-print-preview" | "IDC_WHICHPART_STATIC" => Some(("Print", 6727)),
        "IDC_HEADERFPPTER_GR_STATIC" => Some(("Print", 6728)),
        "pref-backup-snapshot" | "IDC_BACKUPDIR_RESTORESESSION_CHECK" => Some(("Backup", 6818)),
        "pref-completion-enable" | "IDD_AUTOC_ENABLECHECK" => Some(("AutoCompletion", 6808)),
        "pref-completion-functions" | "IDD_AUTOC_FUNCRADIO" => Some(("AutoCompletion", 6809)),
        "pref-completion-words" | "IDD_AUTOC_WORDRADIO" => Some(("AutoCompletion", 6810)),
        "IDD_AUTOC_STATIC_FROM" => Some(("AutoCompletion", 6811)),
        "IDD_AUTOC_STATIC_CHAR" => Some(("AutoCompletion", 6813)),
        "pref-completion-both" | "IDD_AUTOC_BOTHRADIO" => Some(("AutoCompletion", 6816)),
        _ => None,
    };
    source.and_then(|(section, id)| {
        let path = if section.is_empty() { "Preference".to_owned() } else { format!("Preference/{section}") };
        current_dialog_control(&path, id)
    }).or_else(|| id.parse().ok().and_then(current_preference_control))
        .or_else(|| current_preference_label(label)).unwrap_or(label)
}

pub(crate) fn current_dialog_control(path: &str, id: u32) -> Option<&'static str> {
    let filename = PREFERENCE_SCOPE.with(|scope| scope.borrow().as_ref().map(|(filename, _)| filename.clone()))?;
    dialog_control(&filename, path, id)
}

pub(crate) fn current_dialog_label<'a>(path: &str, label: &'a str) -> &'a str {
    let key = label_key(label);
    let id = catalog_translation("chineseSimplified.xml").ok().and_then(|translation| translation.dialogs.get(path))
        .and_then(|dialog| dialog.controls.iter().find(|(_, name)| label_key(name) == key).map(|(id, _)| *id));
    id.and_then(|id| current_dialog_control(path, id)).unwrap_or(label)
}

pub(crate) fn current_string(id: &str) -> Option<&'static str> {
    let filename = PREFERENCE_SCOPE.with(|scope| scope.borrow().as_ref().map(|(filename, _)| filename.clone()))?;
    localized_string(&filename, id)
}
