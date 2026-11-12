use serde_json::{Value, json};
use std::{collections::BTreeMap, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum UdlStyleId {
    Default = 0,
    Comment = 1,
    CommentLine = 2,
    Number = 3,
    Keyword1 = 4,
    Keyword2 = 5,
    Keyword3 = 6,
    Keyword4 = 7,
    Keyword5 = 8,
    Keyword6 = 9,
    Keyword7 = 10,
    Keyword8 = 11,
    Operator = 12,
    FolderInCode1 = 13,
    FolderInCode2 = 14,
    FolderInComment = 15,
    Delimiter1 = 16,
    Delimiter2 = 17,
    Delimiter3 = 18,
    Delimiter4 = 19,
    Delimiter5 = 20,
    Delimiter6 = 21,
    Delimiter7 = 22,
    Delimiter8 = 23,
    Identifier = 24,
}
impl UdlStyleId {
    pub const ALL: [Self; 25] = [
        Self::Default,
        Self::Comment,
        Self::CommentLine,
        Self::Number,
        Self::Keyword1,
        Self::Keyword2,
        Self::Keyword3,
        Self::Keyword4,
        Self::Keyword5,
        Self::Keyword6,
        Self::Keyword7,
        Self::Keyword8,
        Self::Operator,
        Self::FolderInCode1,
        Self::FolderInCode2,
        Self::FolderInComment,
        Self::Delimiter1,
        Self::Delimiter2,
        Self::Delimiter3,
        Self::Delimiter4,
        Self::Delimiter5,
        Self::Delimiter6,
        Self::Delimiter7,
        Self::Delimiter8,
        Self::Identifier,
    ];
    pub fn from_u16(id: u16) -> Option<Self> {
        Self::ALL.get(usize::from(id)).copied()
    }
    pub fn keyword(index: usize) -> Option<Self> {
        if index < 8 {
            Self::ALL.get(index + 4).copied()
        } else {
            None
        }
    }
    pub fn delimiter(index: usize) -> Option<Self> {
        if index < 8 {
            Self::ALL.get(index + 16).copied()
        } else {
            None
        }
    }
    pub fn name(self) -> &'static str {
        [
            "DEFAULT",
            "COMMENTS",
            "LINE COMMENTS",
            "NUMBERS",
            "KEYWORDS1",
            "KEYWORDS2",
            "KEYWORDS3",
            "KEYWORDS4",
            "KEYWORDS5",
            "KEYWORDS6",
            "KEYWORDS7",
            "KEYWORDS8",
            "OPERATORS",
            "FOLDER IN CODE1",
            "FOLDER IN CODE2",
            "FOLDER IN COMMENT",
            "DELIMITERS1",
            "DELIMITERS2",
            "DELIMITERS3",
            "DELIMITERS4",
            "DELIMITERS5",
            "DELIMITERS6",
            "DELIMITERS7",
            "DELIMITERS8",
            "IDENTIFIER",
        ][self as usize]
    }
}

pub mod nesting {
    pub const COMMENT: u32 = 0x100;
    pub const COMMENT_LINE: u32 = 0x200;
    pub const OPERATORS1: u32 = 0x1000000;
    pub const OPERATORS2: u32 = 0x2000000;
    pub const NUMBERS: u32 = 0x4000000;
    pub const fn delimiter(index: usize) -> u32 {
        1 << index
    }
    pub const fn keyword(index: usize) -> u32 {
        0x400 << index
    }
    pub const fn code2(action: usize) -> u32 {
        0x40000 << action
    }
    pub const fn comment_fold(action: usize) -> u32 {
        0x200000 << action
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    pub id: UdlStyleId,
    pub name: String,
    pub foreground: Option<u32>,
    pub background: Option<u32>,
    pub color_style: i32,
    pub font_enabled: bool,
    pub font_name: String,
    pub font_style: i32,
    pub font_size: i32,
    pub nesting: u32,
    pub keyword_class: i32,
    pub keywords: String,
    pub extra: BTreeMap<String, Value>,
}
impl Style {
    pub fn new(id: UdlStyleId) -> Self {
        Self {
            id,
            name: id.name().to_owned(),
            foreground: None,
            background: None,
            color_style: 3,
            font_enabled: false,
            font_name: String::new(),
            font_style: -1,
            font_size: -1,
            nesting: 0,
            keyword_class: -1,
            keywords: String::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCommentPlacement {
    #[default]
    Anywhere,
    BeginningOfLine,
    AfterWhitespace,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecimalSeparator {
    #[default]
    Dot,
    Comma,
    Both,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delimiter {
    pub open: String,
    pub escape: String,
    pub close: String,
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comments {
    pub line_open: String,
    pub line_continue: String,
    pub line_close: String,
    pub block_open: String,
    pub block_close: String,
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Numbers {
    pub prefix1: String,
    pub prefix2: String,
    pub extras1: String,
    pub extras2: String,
    pub suffix1: String,
    pub suffix2: String,
    pub range: String,
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldingGroup {
    pub open: String,
    pub middle: String,
    pub close: String,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub extensions: String,
    pub udl_version: String,
    pub dark_mode_theme: bool,
    pub case_ignored: bool,
    pub prefix_keywords: [bool; 8],
    pub line_comment_placement: LineCommentPlacement,
    pub decimal_separator: DecimalSeparator,
    pub allow_fold_comments: bool,
    pub fold_compact: bool,
    pub keywords: [String; 8],
    pub delimiters: [Delimiter; 8],
    pub comments: Comments,
    pub numbers: Numbers,
    pub operators: [String; 2],
    pub folding: [FoldingGroup; 3],
    pub styles: [Style; 24],
    pub extra: BTreeMap<String, Value>,
}
impl Default for Definition {
    fn default() -> Self {
        Self {
            name: "new user define".into(),
            extensions: String::new(),
            udl_version: String::new(),
            dark_mode_theme: false,
            case_ignored: false,
            prefix_keywords: [false; 8],
            line_comment_placement: LineCommentPlacement::Anywhere,
            decimal_separator: DecimalSeparator::Dot,
            allow_fold_comments: false,
            fold_compact: false,
            keywords: Default::default(),
            delimiters: Default::default(),
            comments: Comments::default(),
            numbers: Numbers::default(),
            operators: Default::default(),
            folding: Default::default(),
            styles: std::array::from_fn(|i| Style::new(UdlStyleId::ALL[i])),
            extra: BTreeMap::new(),
        }
    }
}

fn invalid(field: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("invalid UDL {field}"))
}
fn object(value: &Value) -> io::Result<BTreeMap<String, Value>> {
    value
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .ok_or_else(|| invalid("object"))
}
fn string(m: &mut BTreeMap<String, Value>, key: &str, dst: &mut String) -> io::Result<()> {
    if let Some(v) = m.remove(key) {
        *dst = v.as_str().ok_or_else(|| invalid(key))?.to_owned();
    }
    Ok(())
}
fn boolean(m: &mut BTreeMap<String, Value>, key: &str, dst: &mut bool) -> io::Result<()> {
    if let Some(v) = m.remove(key) {
        *dst = v.as_bool().ok_or_else(|| invalid(key))?;
    }
    Ok(())
}
fn integer(m: &mut BTreeMap<String, Value>, key: &str, dst: &mut i32) -> io::Result<()> {
    if let Some(v) = m.remove(key) {
        *dst = v
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| invalid(key))?;
    }
    Ok(())
}
fn bitmask(m: &mut BTreeMap<String, Value>, key: &str, dst: &mut u32) -> io::Result<()> {
    if let Some(v) = m.remove(key) {
        *dst = v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .or_else(|| {
                v.as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .map(|n| u32::from_ne_bytes(n.to_ne_bytes()))
            })
            .ok_or_else(|| invalid(key))?;
    }
    Ok(())
}
fn merge(extra: &BTreeMap<String, Value>, fields: Value) -> Value {
    let mut m: serde_json::Map<String, Value> = extra.clone().into_iter().collect();
    if let Value::Object(f) = fields {
        m.extend(f);
    }
    Value::Object(m)
}

macro_rules! string_object {
    ($ty:ty, $($field:ident),+) => {
        impl $ty {
            pub fn from_json(v: &Value) -> io::Result<Self> {
                let mut m = object(v)?; let mut out = Self::default();
                $(string(&mut m, stringify!($field), &mut out.$field)?;)+
                out.extra = m; Ok(out)
            }
            pub fn to_json(&self) -> Value { merge(&self.extra, json!({$(stringify!($field): self.$field),+})) }
        }
    }
}
string_object!(Delimiter, open, escape, close);
string_object!(
    Comments,
    line_open,
    line_continue,
    line_close,
    block_open,
    block_close
);
string_object!(
    Numbers, prefix1, prefix2, extras1, extras2, suffix1, suffix2, range
);
string_object!(FoldingGroup, open, middle, close);

impl Style {
    pub fn from_json(v: &Value) -> io::Result<Self> {
        let mut m = object(v)?;
        let id = m
            .remove("id")
            .and_then(|v| v.as_u64())
            .and_then(|n| u16::try_from(n).ok())
            .and_then(UdlStyleId::from_u16)
            .ok_or_else(|| invalid("style id"))?;
        let mut s = Self::new(id);
        string(&mut m, "name", &mut s.name)?;
        for (key, dst) in [
            ("foreground", &mut s.foreground),
            ("background", &mut s.background),
        ] {
            if let Some(v) = m.remove(key) {
                *dst = if v.is_null() || v.as_i64() == Some(-1) {
                    None
                } else {
                    Some(
                        v.as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or_else(|| invalid(key))?,
                    )
                };
            }
        }
        integer(&mut m, "color_style", &mut s.color_style)?;
        boolean(&mut m, "font_enabled", &mut s.font_enabled)?;
        string(&mut m, "font_name", &mut s.font_name)?;
        integer(&mut m, "font_style", &mut s.font_style)?;
        integer(&mut m, "font_size", &mut s.font_size)?;
        bitmask(&mut m, "nesting", &mut s.nesting)?;
        integer(&mut m, "keyword_class", &mut s.keyword_class)?;
        string(&mut m, "keywords", &mut s.keywords)?;
        s.extra = m;
        Ok(s)
    }
    pub fn to_json(&self) -> Value {
        merge(
            &self.extra,
            json!({"id":self.id as u16,"name":self.name,"foreground":self.foreground,
            "background":self.background,"color_style":self.color_style,"font_enabled":self.font_enabled,
            "font_name":self.font_name,"font_style":self.font_style,"font_size":self.font_size,
            "nesting":self.nesting,"keyword_class":self.keyword_class,"keywords":self.keywords}),
        )
    }
}

fn array<T, const N: usize>(
    v: Value,
    mut parse: impl FnMut(&Value) -> io::Result<T>,
) -> io::Result<[T; N]> {
    let items = v.as_array().ok_or_else(|| invalid("array"))?;
    if items.len() != N {
        return Err(invalid("array length"));
    }
    items
        .iter()
        .map(&mut parse)
        .collect::<io::Result<Vec<T>>>()?
        .try_into()
        .map_err(|_| invalid("array length"))
}
impl Definition {
    pub fn from_json(v: &Value) -> io::Result<Self> {
        let mut m = object(v)?;
        let mut d = Self::default();
        string(&mut m, "name", &mut d.name)?;
        string(&mut m, "extensions", &mut d.extensions)?;
        string(&mut m, "udl_version", &mut d.udl_version)?;
        boolean(&mut m, "dark_mode_theme", &mut d.dark_mode_theme)?;
        boolean(&mut m, "case_ignored", &mut d.case_ignored)?;
        boolean(&mut m, "allow_fold_comments", &mut d.allow_fold_comments)?;
        boolean(&mut m, "fold_compact", &mut d.fold_compact)?;
        if let Some(v) = m.remove("line_comment_placement") {
            d.line_comment_placement = match v.as_u64() {
                Some(0) => LineCommentPlacement::Anywhere,
                Some(1) => LineCommentPlacement::BeginningOfLine,
                Some(2) => LineCommentPlacement::AfterWhitespace,
                _ => return Err(invalid("line_comment_placement")),
            };
        }
        if let Some(v) = m.remove("decimal_separator") {
            d.decimal_separator = match v.as_u64() {
                Some(0) => DecimalSeparator::Dot,
                Some(1) => DecimalSeparator::Comma,
                Some(2) => DecimalSeparator::Both,
                _ => return Err(invalid("decimal_separator")),
            };
        }
        if let Some(v) = m.remove("prefix_keywords") {
            d.prefix_keywords =
                array(v, |x| x.as_bool().ok_or_else(|| invalid("prefix_keywords")))?;
        }
        if let Some(v) = m.remove("keywords") {
            d.keywords = array(v, |x| {
                x.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("keywords"))
            })?;
        }
        if let Some(v) = m.remove("operators") {
            d.operators = array(v, |x| {
                x.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("operators"))
            })?;
        }
        if let Some(v) = m.remove("delimiters") {
            d.delimiters = array(v, Delimiter::from_json)?;
        }
        if let Some(v) = m.remove("folding") {
            d.folding = array(v, FoldingGroup::from_json)?;
        }
        if let Some(v) = m.remove("comments") {
            d.comments = Comments::from_json(&v)?;
        }
        if let Some(v) = m.remove("numbers") {
            d.numbers = Numbers::from_json(&v)?;
        }
        if let Some(v) = m.remove("styles") {
            let styles: [Style; 24] = array(v, Style::from_json)?;
            let mut seen = [false; 24];
            for s in &styles {
                let id = s.id as usize;
                if id >= 24 || seen[id] {
                    return Err(invalid("duplicate/internal style id"));
                }
                seen[id] = true;
                d.styles[id] = s.clone();
            }
        }
        d.extra = m;
        Ok(d)
    }
    pub fn to_json(&self) -> Value {
        merge(
            &self.extra,
            json!({"name":self.name,"extensions":self.extensions,"udl_version":self.udl_version,
            "dark_mode_theme":self.dark_mode_theme,"case_ignored":self.case_ignored,"prefix_keywords":self.prefix_keywords,
            "line_comment_placement":self.line_comment_placement as u8,"decimal_separator":self.decimal_separator as u8,
            "allow_fold_comments":self.allow_fold_comments,"fold_compact":self.fold_compact,"keywords":self.keywords,
            "delimiters":self.delimiters.iter().map(Delimiter::to_json).collect::<Vec<_>>(),
            "comments":self.comments.to_json(),"numbers":self.numbers.to_json(),"operators":self.operators,
            "folding":self.folding.iter().map(FoldingGroup::to_json).collect::<Vec<_>>(),
            "styles":self.styles.iter().map(Style::to_json).collect::<Vec<_>>()}),
        )
    }
    pub fn style(&self, id: UdlStyleId) -> &Style {
        self.styles
            .iter()
            .find(|s| s.id == id)
            .unwrap_or(&self.styles[0])
    }
}
