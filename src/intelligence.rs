use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, OnceLock};

use serde_json::Value;

include!(concat!(env!("OUT_DIR"), "/intelligence_generated.rs"));

pub mod auto_insert;
pub mod events;
pub mod function_list;

#[path = "intelligence/declarators.rs"]
mod declarators;
#[path = "intelligence/functions.rs"]
mod functions;
#[path = "intelligence/pattern.rs"]
mod pattern;
pub use functions::{FunctionSymbol, ParseOutcome, SymbolKind};
#[path = "intelligence/completion.rs"]
mod completion;
pub use completion::*;
#[path = "intelligence/signatures.rs"]
mod signatures;
pub use signatures::{CallSignature, SignatureHelp};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntelligenceError(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternDiagnostic {
    pub source_key: String,
    pub field: String,
    pub message: String,
}

impl fmt::Display for IntelligenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IntelligenceError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiEnvironment {
    pub ignore_case: bool,
    pub start_function: char,
    pub stop_function: char,
    pub parameter_separator: char,
    pub terminal: Option<char>,
    pub additional_word_chars: String,
}

impl Default for ApiEnvironment {
    fn default() -> Self {
        Self {
            ignore_case: true,
            start_function: '(',
            stop_function: ')',
            parameter_separator: ',',
            terminal: Some(';'),
            additional_word_chars: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiParameter {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiOverload {
    pub return_value: String,
    pub description: String,
    pub parameters: Vec<ApiParameter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiKeyword {
    pub name: String,
    pub is_function: bool,
    pub overloads: Vec<ApiOverload>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiDefinition {
    pub source_key: String,
    pub language: Option<String>,
    pub environment: ApiEnvironment,
    pub keywords: Vec<ApiKeyword>,
}

impl ApiDefinition {
    fn from_node(key: &str, mut node: DefinitionNode) -> Result<Self, IntelligenceError> {
        let mut environment = ApiEnvironment::default();
        if let Some(env) = node.child("Environment") {
            environment.ignore_case = env.attr("ignoreCase") != "no";
            environment.start_function = env.attr("startFunc").chars().next().unwrap_or('(');
            environment.stop_function = env.attr("stopFunc").chars().next().unwrap_or(')');
            environment.parameter_separator =
                env.attr("paramSeparator").chars().next().unwrap_or(',');
            environment.terminal = env.attr("terminal").chars().next().or(Some(';'));
            environment.additional_word_chars = env.attr("additionalWordChar").to_owned();
        }
        let mut keywords = Vec::new();
        for mut word in node.children.into_iter().filter(|n| n.tag == "KeyWord") {
            let name = word.attributes.remove("name").unwrap_or_default();
            if name.is_empty() {
                return Err(IntelligenceError(format!("missing API keyword name: {key}")));
            }
            let is_function = word.attr("func") == "yes";
            let mut overloads = Vec::new();
            for mut overload in word.children.into_iter().filter(|n| n.tag == "Overload") {
                let parameters = overload
                    .children
                    .into_iter()
                    .filter(|n| n.tag == "Param")
                    .map(|mut parameter| ApiParameter {
                        name: parameter.attributes.remove("name").unwrap_or_default(),
                    })
                    .collect();
                overloads.push(ApiOverload {
                    return_value: overload.attributes.remove("retVal").unwrap_or_default(),
                    description: overload.attributes.remove("descr").unwrap_or_default(),
                    parameters,
                });
            }
            keywords.push(ApiKeyword {
                name,
                is_function,
                overloads,
            });
        }
        Ok(Self {
            source_key: key.to_owned(),
            language: node.attributes.remove("language"),
            environment,
            keywords,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DefinitionNode {
    pub tag: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<DefinitionNode>,
}

impl DefinitionNode {
    fn from_json(value: &Value) -> Result<Self, IntelligenceError> {
        let tag = value["tag"]
            .as_str()
            .ok_or_else(|| IntelligenceError("missing definition tag".into()))?
            .to_owned();
        let attrs = value["attributes"]
            .as_object()
            .ok_or_else(|| IntelligenceError(format!("missing attributes: {tag}")))?;
        let attributes = attrs
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|s| (key.clone(), s.to_owned()))
                    .ok_or_else(|| IntelligenceError(format!("non-string attribute: {tag}/{key}")))
            })
            .collect::<Result<_, _>>()?;
        let children = value["children"]
            .as_array()
            .ok_or_else(|| IntelligenceError(format!("missing children: {tag}")))?
            .iter()
            .map(Self::from_json)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            tag,
            attributes,
            children,
        })
    }

    pub(crate) fn attr(&self, key: &str) -> &str {
        self.attributes.get(key).map(String::as_str).unwrap_or("")
    }

    pub(crate) fn child(&self, tag: &str) -> Option<&Self> {
        self.children.iter().find(|child| child.tag == tag)
    }

    fn validate_parser_node(&self) -> Result<(), IntelligenceError> {
        let (attributes, children): (&[&str], &[&str]) = match self.tag.as_str() {
            "parser" => (
                &["id", "displayName", "commentExpr"],
                &["classRange", "function"],
            ),
            "classRange" => (
                &["mainExpr", "openSymbole", "closeSymbole", "displayMode"],
                &["className", "function"],
            ),
            "function" => (&["mainExpr", "displayMode"], &["functionName", "className"]),
            "className" => (&[], &["nameExpr"]),
            "functionName" => (&[], &["nameExpr", "funcNameExpr"]),
            "nameExpr" | "funcNameExpr" => (&["expr"], &[]),
            _ => {
                return Err(IntelligenceError(format!(
                    "unknown parser node: {}",
                    self.tag
                )));
            }
        };
        if self
            .attributes
            .keys()
            .any(|key| !attributes.contains(&key.as_str()))
            || self
                .children
                .iter()
                .any(|child| !children.contains(&child.tag.as_str()))
        {
            return Err(IntelligenceError(format!(
                "unknown parser field or child: {}",
                self.tag
            )));
        }
        if ["classRange", "function"].contains(&self.tag.as_str())
            && self.attr("mainExpr").is_empty()
            || ["nameExpr", "funcNameExpr"].contains(&self.tag.as_str())
                && self.attr("expr").is_empty()
        {
            return Err(IntelligenceError(format!(
                "missing parser expression: {}",
                self.tag
            )));
        }
        if self.tag == "classRange" && self.child("function").is_none() {
            return Err(IntelligenceError(
                "classRange has no function definition".into(),
            ));
        }
        for child in &self.children {
            child.validate_parser_node()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct IntelligenceCatalog {
    apis: BTreeMap<String, Arc<ApiEntry>>,
    parsers: BTreeMap<String, DefinitionNode>,
    associations: BTreeMap<String, String>,
}

#[derive(Debug)]
struct ApiEntry {
    payload: Option<&'static str>,
    definition: OnceLock<Result<ApiDefinition, IntelligenceError>>,
}

impl ApiEntry {
    fn get(&self, key: &str) -> Result<&ApiDefinition, IntelligenceError> {
        self.definition
            .get_or_init(|| {
                let payload = self
                    .payload
                    .ok_or_else(|| IntelligenceError(format!("missing API payload: {key}")))?;
                let node = {
                    let value: Value = serde_json::from_str(payload).map_err(|error| {
                        IntelligenceError(format!("API {key}: {error}"))
                    })?;
                    DefinitionNode::from_json(&value)?
                };
                ApiDefinition::from_node(key, node)
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

impl IntelligenceCatalog {
    pub fn bundled() -> Result<Self, IntelligenceError> {
        let mut catalog = Self::from_json("{}", BUNDLED_PARSERS)?;
        catalog.apis = BUNDLED_APIS
            .iter()
            .map(|&(key, payload)| {
                (
                    key.to_owned(),
                    Arc::new(ApiEntry {
                        payload: Some(payload),
                        definition: OnceLock::new(),
                    }),
                )
            })
            .collect();
        let value: Value = serde_json::from_str(BUNDLED_ASSOCIATIONS)
            .map_err(|error| IntelligenceError(error.to_string()))?;
        let node = DefinitionNode::from_json(&value)?;
        if node.tag != "associationMap" {
            return Err(IntelligenceError("invalid parser association map".into()));
        }
        for association in &node.children {
            let key = canonical_key(association.attr("id"));
            if association.tag != "association" || !catalog.parsers.contains_key(&key) {
                return Err(IntelligenceError(
                    "invalid source parser association".into(),
                ));
            }
            let name = association.attr("userDefinedLangName");
            if !name.is_empty() {
                catalog.associations.insert(name.into(), key);
            }
        }
        Ok(catalog)
    }

    pub fn from_json(apis: &str, parsers: &str) -> Result<Self, IntelligenceError> {
        let api_json: Value =
            serde_json::from_str(apis).map_err(|error| IntelligenceError(error.to_string()))?;
        let parser_json: Value =
            serde_json::from_str(parsers).map_err(|error| IntelligenceError(error.to_string()))?;
        let api_map = api_json
            .as_object()
            .ok_or_else(|| IntelligenceError("API catalog must be an object".into()))?;
        let parser_map = parser_json
            .as_object()
            .ok_or_else(|| IntelligenceError("parser catalog must be an object".into()))?;
        let mut apis = BTreeMap::new();
        for (key, value) in api_map {
            let node = DefinitionNode::from_json(value)?;
            apis.insert(
                key.clone(),
                Arc::new(ApiEntry {
                    payload: None,
                    definition: OnceLock::from(Ok(ApiDefinition::from_node(key, node)?)),
                }),
            );
        }
        let mut parsers = BTreeMap::new();
        for (key, value) in parser_map {
            let node = DefinitionNode::from_json(&value["definition"])?;
            node.validate_parser_node()?;
            if node.tag != "parser"
                || node.attr("id").is_empty()
                || (node.child("function").is_none() && node.child("classRange").is_none())
            {
                return Err(IntelligenceError(format!(
                    "invalid parser definition: {key}"
                )));
            }
            parsers.insert(key.clone(), node);
        }
        Ok(Self {
            apis,
            parsers,
            associations: BTreeMap::new(),
        })
    }

    pub fn api_keys(&self) -> impl Iterator<Item = &str> {
        self.apis.keys().map(String::as_str)
    }

    pub fn parser_keys(&self) -> impl Iterator<Item = &str> {
        self.parsers.keys().map(String::as_str)
    }

    pub fn parser_associations(&self) -> impl Iterator<Item = (&str, &str)> {
        self.associations
            .iter()
            .map(|(name, key)| (name.as_str(), key.as_str()))
    }

    fn resolve_parser_key(&self, source_key: &str) -> Option<&str> {
        let key = canonical_key(source_key);
        if let Some((key, _)) = self.parsers.get_key_value(&key) {
            return Some(key);
        }
        if let Some((_, key)) = self
            .associations
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(source_key))
        {
            return Some(key);
        }
        self.parsers
            .iter()
            .find(|(_, parser)| {
                parser.attr("displayName").eq_ignore_ascii_case(source_key)
                    || parser.attr("id").eq_ignore_ascii_case(source_key)
            })
            .map(|(key, _)| key.as_str())
    }

    pub fn try_api(&self, source_key: &str) -> Result<Option<&ApiDefinition>, IntelligenceError> {
        let key = canonical_key(source_key);
        let key = if key == "javascript.js" {
            "javascript"
        } else {
            &key
        };
        self.apis.get(key).map(|entry| entry.get(key)).transpose()
    }

    pub fn parse_functions(
        &self,
        source_key: &str,
        text: &str,
    ) -> Result<ParseOutcome, IntelligenceError> {
        let Some(key) = self.resolve_parser_key(source_key) else {
            return Ok(ParseOutcome::NoParser);
        };
        let parser = self
            .parsers
            .get(key)
            .ok_or_else(|| IntelligenceError("parser association target is absent".into()))?;
        self.validate_extraction_patterns(key)?;
        functions::parse(key, parser, text)
    }

    pub fn compatibility_notes(&self) -> &'static [&'static str] {
        &[
            "FunctionList-only compatibility, not a general Boost regex implementation",
            "Malformed shipped fortran77 commentExpr is preserved: its source comment zones are empty; pattern_diagnostics reports the error",
            "Exact shipped C, Java, Nim and Pascal recursive patterns use structural profiles",
        ]
    }

    pub fn validate_patterns(&self) -> Result<(), IntelligenceError> {
        for key in self.parser_keys() {
            self.validate_parser_patterns(key)?;
        }
        Ok(())
    }

    pub fn validate_parser_patterns(&self, source_key: &str) -> Result<(), IntelligenceError> {
        if let Some(diagnostic) = self.pattern_diagnostics(source_key).into_iter().next() {
            return Err(IntelligenceError(format!(
                "{}/{}: {}",
                diagnostic.source_key, diagnostic.field, diagnostic.message
            )));
        }
        Ok(())
    }

    pub fn validate_extraction_patterns(&self, source_key: &str) -> Result<(), IntelligenceError> {
        if let Some(diagnostic) = self
            .pattern_diagnostics(source_key)
            .into_iter()
            .find(|diagnostic| diagnostic.field != "parser@commentExpr")
        {
            return Err(IntelligenceError(format!(
                "{}/{}: {}",
                diagnostic.source_key, diagnostic.field, diagnostic.message
            )));
        }
        Ok(())
    }

    pub fn source_comment_zones(
        &self,
        source_key: &str,
        text: &str,
    ) -> Result<Vec<std::ops::Range<usize>>, IntelligenceError> {
        functions::source_comment_zones(
            self.resolve_parser_key(source_key)
                .and_then(|key| self.parsers.get(key)),
            text,
        )
    }

    pub fn pattern_diagnostics(&self, source_key: &str) -> Vec<PatternDiagnostic> {
        let mut diagnostics = Vec::new();
        fn visit(key: &str, node: &DefinitionNode, diagnostics: &mut Vec<PatternDiagnostic>) {
            for (name, value) in &node.attributes {
                if [
                    "mainExpr",
                    "expr",
                    "commentExpr",
                    "openSymbole",
                    "closeSymbole",
                ]
                .contains(&name.as_str())
                    && !value.is_empty()
                    && let Err(error) = pattern::Pattern::compile(value)
                {
                    diagnostics.push(PatternDiagnostic {
                        source_key: key.into(),
                        field: format!("{}@{name}", node.tag),
                        message: error.to_string(),
                    });
                }
            }
            for child in &node.children {
                visit(key, child, diagnostics);
            }
        }
        if let Some(key) = self.resolve_parser_key(source_key)
            && let Some(parser) = self.parsers.get(key)
        {
            visit(key, parser, &mut diagnostics);
        }
        diagnostics
    }
}

fn canonical_key(source_key: &str) -> String {
    let lowercase = source_key.to_ascii_lowercase();
    let key = lowercase
        .strip_suffix(".xml")
        .unwrap_or(&lowercase)
        .to_owned();
    match key.as_str() {
        "c++" => "cpp".into(),
        "c#" => "cs".into(),
        "javascript" => "javascript.js".into(),
        "visualbasic" => "vb".into(),
        "shell" => "bash".into(),
        _ => key,
    }
}
