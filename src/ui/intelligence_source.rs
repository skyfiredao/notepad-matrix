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
            let catalog = catalog()?;
            Ok(catalog
                .parser_associations()
                .find(|(association, _)| *association == name)
                .map(|(_, key)| key)
                .unwrap_or("")
                .to_owned())
        }
    }
}
