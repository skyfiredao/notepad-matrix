use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use serde_json::Value;

pub const SCHEMA_VERSION: u64 = 1;
pub const SOURCE_REVISION: &str = "d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e";
pub const EMBEDDED_CATALOG: &str = include_str!("../assets/languages.json");
pub const EMBEDDED_THEMES: &str = include_str!("../assets/languages.themes.json");

pub use crate::lexers::{FoldLevel, FoldRange, LexResult, Span as TokenSpan};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleProfile {
    pub id: u16,
    pub attributes: BTreeMap<String, String>,
    pub keywords: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageProfile {
    pub id: u16,
    pub name: String,
    pub label: String,
    pub description: String,
    pub source_symbol: String,
    pub source_lexer: String,
    pub effective_lexer: String,
    pub order: usize,
    pub internal: bool,
    pub attributes: BTreeMap<String, String>,
    pub extensions: Vec<String>,
    pub filenames: Vec<String>,
    pub keywords: BTreeMap<String, String>,
    pub keyword_sources: Vec<String>,
    pub numbered_keywords: Vec<String>,
    pub style_source: Option<String>,
    pub style_attributes: BTreeMap<String, String>,
    pub styles: Vec<StyleProfile>,
    pub effective_style_sources: Vec<String>,
    pub components: Vec<String>,
    pub properties: BTreeMap<String, String>,
    pub property_setter: Option<String>,
    pub comment_line: String,
    pub comment_start: String,
    pub comment_end: String,
}

impl LanguageProfile {
    pub fn key(&self) -> &str {
        &self.name
    }

    pub fn lexer(&self) -> &str {
        &self.effective_lexer
    }

    pub fn keywords(&self, index: usize) -> &str {
        self.numbered_keywords.get(index).map_or("", String::as_str)
    }

    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties.get(name).map(String::as_str)
    }

    pub fn keyword(&self, class: &str) -> Option<&str> {
        self.keywords.get(class).map(String::as_str)
    }

    pub fn style(&self, id: u16) -> Option<&StyleProfile> {
        self.styles.iter().find(|style| style.id == id)
    }
}

pub type LanguageDefinition = LanguageProfile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogError(pub String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Clone, Debug)]
pub struct LanguageCatalog {
    profiles: Vec<LanguageProfile>,
    themes: ThemeCatalog,
    pub source_revision: String,
    pub source_hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeLexer {
    pub attributes: BTreeMap<String, String>,
    pub styles: Vec<StyleProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeDefinition {
    pub source_path: String,
    pub lexers: BTreeMap<String, ThemeLexer>,
    pub global_styles: Vec<StyleProfile>,
}

#[derive(Clone, Debug)]
pub struct ThemeCatalog {
    light: ThemeDefinition,
    dark: ThemeDefinition,
    pub source_hashes: BTreeMap<String, String>,
}

impl ThemeCatalog {
    pub fn from_json(input: &str) -> Result<Self, CatalogError> {
        if input.len() > 8 * 1024 * 1024 {
            return Err(invalid("theme catalog exceeds 8 MiB"));
        }
        let root: Value =
            serde_json::from_str(input).map_err(|error| invalid(error.to_string()))?;
        if number(&root, "schema_version")? != SCHEMA_VERSION
            || string(&root, "source_revision")? != SOURCE_REVISION
        {
            return Err(invalid(
                "unsupported theme catalog schema or source version",
            ));
        }
        let frozen: Value = serde_json::from_str(EMBEDDED_THEMES)
            .map_err(|error| invalid(format!("embedded themes: {error}")))?;
        if root != frozen {
            return Err(invalid("theme data differs from the fixed source version"));
        }
        let themes = field(&root, "themes")?;
        Ok(Self {
            light: Self::parse_theme(field(themes, "light")?)?,
            dark: Self::parse_theme(field(themes, "dark")?)?,
            source_hashes: map(&root, "source_hashes")?,
        })
    }

    fn parse_theme(value: &Value) -> Result<ThemeDefinition, CatalogError> {
        let mut lexers = BTreeMap::new();
        for entry in field(value, "lexers")?
            .as_array()
            .ok_or_else(|| invalid("theme lexers must be an array"))?
        {
            let attributes: BTreeMap<String, String> = map(entry, "attributes")?;
            let name = attributes
                .get("name")
                .ok_or_else(|| invalid("theme lexer name missing"))?
                .clone();
            let styles = Self::parse_styles(entry, "styles")?;
            let ids: BTreeSet<u16> = styles.iter().map(|style| style.id).collect();
            if ids.len() != styles.len()
                || lexers
                    .insert(name, ThemeLexer { attributes, styles })
                    .is_some()
            {
                return Err(invalid("duplicate theme lexer or style id"));
            }
        }
        let global_styles = Self::parse_styles(value, "global_styles")?;
        if lexers.len() != 93
            || lexers
                .values()
                .map(|lexer| lexer.styles.len())
                .sum::<usize>()
                != 1774
            || global_styles.len() != 53
        {
            return Err(invalid("unexpected frozen theme style counts"));
        }
        Ok(ThemeDefinition {
            source_path: string(value, "source_path")?,
            lexers,
            global_styles,
        })
    }

    fn parse_styles(value: &Value, name: &str) -> Result<Vec<StyleProfile>, CatalogError> {
        field(value, name)?
            .as_array()
            .ok_or_else(|| invalid(format!("{name} must be an array")))?
            .iter()
            .map(|entry| {
                let id = u16::try_from(number(entry, "id")?)
                    .map_err(|_| invalid("theme style id out of range"))?;
                let attributes = map(entry, "attributes")?;
                if attributes
                    .get("styleID")
                    .and_then(|value| value.parse::<u16>().ok())
                    != Some(id)
                {
                    return Err(invalid("theme style id disagrees with attributes"));
                }
                Ok(StyleProfile {
                    id,
                    attributes,
                    keywords: string(entry, "keywords")?,
                })
            })
            .collect()
    }

    pub fn theme(&self, dark: bool) -> &ThemeDefinition {
        if dark { &self.dark } else { &self.light }
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, CatalogError> {
    value
        .get(name)
        .ok_or_else(|| CatalogError(format!("missing field {name}")))
}

fn string(value: &Value, name: &str) -> Result<String, CatalogError> {
    field(value, name)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| CatalogError(format!("{name} must be a string")))
}

fn number(value: &Value, name: &str) -> Result<u64, CatalogError> {
    field(value, name)?
        .as_u64()
        .ok_or_else(|| CatalogError(format!("{name} must be an unsigned integer")))
}

fn map(value: &Value, name: &str) -> Result<BTreeMap<String, String>, CatalogError> {
    serde_json::from_value(field(value, name)?.clone())
        .map_err(|error| CatalogError(format!("{name}: {error}")))
}

fn strings(value: &Value, name: &str) -> Result<Vec<String>, CatalogError> {
    serde_json::from_value(field(value, name)?.clone())
        .map_err(|error| CatalogError(format!("{name}: {error}")))
}

fn optional_string(value: &Value, name: &str) -> Result<Option<String>, CatalogError> {
    let field = field(value, name)?;
    if field.is_null() {
        Ok(None)
    } else {
        string(value, name).map(Some)
    }
}

fn invalid(message: impl Into<String>) -> CatalogError {
    CatalogError(message.into())
}

impl LanguageCatalog {
    pub fn from_json(input: &str) -> Result<Self, CatalogError> {
        if input.len() > 8 * 1024 * 1024 {
            return Err(invalid("catalog exceeds 8 MiB"));
        }
        let root: Value =
            serde_json::from_str(input).map_err(|error| invalid(error.to_string()))?;
        if number(&root, "schema_version")? != SCHEMA_VERSION {
            return Err(invalid("unsupported catalog schema version"));
        }
        if string(&root, "source_revision")? != SOURCE_REVISION {
            return Err(invalid("unexpected frozen source revision"));
        }
        if number(&root, "source_model_count")? != 96 || number(&root, "source_styler_count")? != 93
        {
            return Err(invalid("unexpected source model or styler count"));
        }
        let source_hashes = map(&root, "source_hashes")?;
        if source_hashes.len() != 10
            || source_hashes
                .values()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid("invalid source hashes"));
        }
        let entries = field(&root, "profiles")?
            .as_array()
            .ok_or_else(|| invalid("profiles must be an array"))?;
        if entries.len() != 96 {
            return Err(invalid("catalog must contain all 96 source models"));
        }
        let mut profiles = Vec::with_capacity(entries.len());
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            let id = u16::try_from(number(entry, "id")?)
                .map_err(|_| invalid("language id out of range"))?;
            if id > 96 || id == 15 || !ids.insert(id) {
                return Err(invalid("duplicate or non-model language id"));
            }
            let name = string(entry, "name")?;
            if name.is_empty() || !names.insert(name.clone()) {
                return Err(invalid("empty or duplicate language name"));
            }
            if number(entry, "order")? != index as u64 {
                return Err(invalid("source model order is not contiguous"));
            }
            let mut styles = Vec::new();
            let mut style_ids = BTreeSet::new();
            for value in field(entry, "styles")?
                .as_array()
                .ok_or_else(|| invalid("styles must be an array"))?
            {
                let id = u16::try_from(number(value, "id")?)
                    .map_err(|_| invalid("style id out of range"))?;
                if !style_ids.insert(id) {
                    return Err(invalid("duplicate style id"));
                }
                let attributes = map(value, "attributes")?;
                if attributes
                    .get("styleID")
                    .and_then(|id| id.parse::<u16>().ok())
                    != Some(id)
                {
                    return Err(invalid("styleID disagrees with style id"));
                }
                for key in ["fgColor", "bgColor"] {
                    if let Some(color) = attributes.get(key)
                        && (color.len() != 6 || !color.bytes().all(|b| b.is_ascii_hexdigit()))
                    {
                        return Err(invalid(format!("invalid {key}")));
                    }
                }
                for key in ["fontStyle", "fontSize", "colorStyle"] {
                    if let Some(number) = attributes.get(key)
                        && !number.is_empty()
                        && number.parse::<i32>().is_err()
                    {
                        return Err(invalid(format!("invalid {key}")));
                    }
                }
                styles.push(StyleProfile {
                    id,
                    attributes,
                    keywords: string(value, "keywords")?,
                });
            }
            let profile = LanguageProfile {
                id,
                name,
                label: string(entry, "label")?,
                description: string(entry, "description")?,
                source_symbol: string(entry, "source_symbol")?,
                source_lexer: string(entry, "lexer")?,
                effective_lexer: string(entry, "effective_lexer")?,
                order: index,
                internal: field(entry, "internal")?
                    .as_bool()
                    .ok_or_else(|| invalid("internal must be boolean"))?,
                attributes: map(entry, "attributes")?,
                extensions: strings(entry, "extensions")?,
                filenames: strings(entry, "filenames")?,
                keywords: map(entry, "keywords")?,
                keyword_sources: strings(entry, "keyword_sources")?,
                numbered_keywords: strings(entry, "numbered_keywords")?,
                style_source: optional_string(entry, "style_source")?,
                style_attributes: map(entry, "style_attributes")?,
                styles,
                effective_style_sources: strings(entry, "effective_style_sources")?,
                components: strings(entry, "components")?,
                properties: map(entry, "properties")?,
                property_setter: optional_string(entry, "property_setter")?,
                comment_line: string(entry, "comment_line")?,
                comment_start: string(entry, "comment_start")?,
                comment_end: string(entry, "comment_end")?,
            };
            if profile.attributes.get("name") != Some(&profile.name)
                || profile.attributes.get("ext").map(|ext| {
                    ext.split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                }) != Some(profile.extensions.clone())
            {
                return Err(invalid(
                    "model attributes disagree with profile identity or extensions",
                ));
            }
            for (key, actual) in [
                ("commentLine", &profile.comment_line),
                ("commentStart", &profile.comment_start),
                ("commentEnd", &profile.comment_end),
            ] {
                if profile.attributes.get(key).map_or("", String::as_str) != actual {
                    return Err(invalid("comment syntax disagrees with model attributes"));
                }
            }
            profiles.push(profile);
        }
        for profile in &profiles {
            for reference in profile
                .keyword_sources
                .iter()
                .chain(profile.components.iter())
                .chain(profile.style_source.iter())
                .chain(profile.effective_style_sources.iter())
            {
                if !names.contains(reference) {
                    return Err(invalid(format!("unknown source profile {reference}")));
                }
            }
        }
        let frozen: Value = serde_json::from_str(EMBEDDED_CATALOG)
            .map_err(|error| invalid(format!("embedded catalog: {error}")))?;
        if root != frozen {
            return Err(invalid(
                "catalog data differs from the fixed source version",
            ));
        }
        Ok(Self {
            profiles,
            themes: ThemeCatalog::from_json(EMBEDDED_THEMES)?,
            source_revision: SOURCE_REVISION.into(),
            source_hashes,
        })
    }

    pub fn profiles(&self) -> &[LanguageProfile] {
        &self.profiles
    }

    pub fn themes(&self) -> &ThemeCatalog {
        &self.themes
    }

    pub fn default_style(&self, dark: bool) -> Option<&StyleProfile> {
        self.themes
            .theme(dark)
            .global_styles
            .iter()
            .find(|style| style.id == 32)
    }

    pub fn style_for_theme(
        &self,
        profile_id: u16,
        style_id: u16,
        dark: bool,
    ) -> Option<&StyleProfile> {
        let profile = self.by_id(profile_id)?;
        let theme = self.themes.theme(dark);
        profile
            .effective_style_sources
            .iter()
            .rev()
            .filter_map(|source| theme.lexers.get(source))
            .find_map(|lexer| lexer.styles.iter().find(|style| style.id == style_id))
            .or_else(|| {
                if style_id == 32 {
                    self.default_style(dark)
                } else {
                    None
                }
            })
    }

    pub fn by_id(&self, id: u16) -> Option<&LanguageProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn by_name(&self, name: &str) -> Option<&LanguageProfile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    pub fn by_path(&self, path: &Path) -> Option<&LanguageProfile> {
        let name = path.file_name().and_then(|value| value.to_str());
        let extension = name.and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension));
        let extension_profile = extension.and_then(|extension| {
            self.profiles.iter().rev().find(|profile| {
                profile
                    .extensions
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(extension))
            })
        });
        if extension_profile.is_some_and(|profile| profile.id != 0) {
            return extension_profile;
        }
        name.and_then(|name| {
            self.profiles.iter().find(|profile| {
                profile
                    .filenames
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(name))
            })
        })
        .or(extension_profile)
    }

    pub fn resolve(
        &self,
        path: &Path,
        explicit: Option<u16>,
    ) -> Result<&LanguageProfile, CatalogError> {
        if let Some(id) = explicit {
            return self
                .by_id(id)
                .ok_or_else(|| invalid(format!("unknown explicit language id {id}")));
        }
        self.by_path(path)
            .or_else(|| self.by_id(0))
            .ok_or_else(|| invalid("normal text model missing"))
    }
}

pub fn catalog() -> Result<&'static LanguageCatalog, &'static CatalogError> {
    static CATALOG: OnceLock<Result<LanguageCatalog, CatalogError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| LanguageCatalog::from_json(EMBEDDED_CATALOG))
        .as_ref()
}
