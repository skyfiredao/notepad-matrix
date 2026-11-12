use crate::intelligence::{IntelligenceCatalog, IntelligenceError};
use crate::syntax::Language;
use std::sync::{Arc, OnceLock};

pub fn catalog() -> Result<Arc<IntelligenceCatalog>, IntelligenceError> {
    static CATALOG: OnceLock<Result<Arc<IntelligenceCatalog>, IntelligenceError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| IntelligenceCatalog::bundled().map(Arc::new))
        .clone()
}

pub fn source_key(language: Language, udl_name: Option<&str>) -> Result<String, IntelligenceError> {
    match language {
        Language::NormalText => Ok("normal".into()),
        Language::Json => Ok("json".into()),
        Language::Builtin(id) => crate::languages::catalog()
            .map_err(|error| IntelligenceError(error.to_string()))?
            .by_id(id)
            .map(|profile| profile.name.clone())
            .ok_or_else(|| IntelligenceError(format!("Unknown source language profile {id}"))),
        Language::UserDefined(_) => {
            let Some(name) = udl_name else {
                return Ok(String::new());
            };
            let overrides: serde_json::Value =
                serde_json::from_str(include_str!("../../assets/intelligence/overrides.json"))
                    .map_err(|error| {
                        IntelligenceError(format!("Function List associations: {error}"))
                    })?;
            let associations = overrides["children"].as_array().ok_or_else(|| {
                IntelligenceError("Function List associationMap has no associations".into())
            })?;
            Ok(associations
                .iter()
                .find(|association| {
                    association["attributes"]["userDefinedLangName"].as_str() == Some(name)
                })
                .and_then(|association| association["attributes"]["id"].as_str())
                .unwrap_or("")
                .trim_end_matches(".xml")
                .to_owned())
        }
    }
}
