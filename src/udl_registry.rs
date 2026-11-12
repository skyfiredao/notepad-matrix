use crate::{config::ConfigStore, udl::Definition};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io, path::Path};

#[derive(Clone, Debug)]
struct Entry {
    definition: Definition,
    revision: u64,
    extra: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct UdlRegistry {
    entries: BTreeMap<u64, Entry>,
    revision: u64,
    next_id: u64,
    extra: serde_json::Map<String, Value>,
    persisted_base: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct SaveReceipt {
    previous_base: Option<Value>,
    persisted: Value,
}

fn conflict() -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        "UDL save conflict: udl.json changed in another instance; the file was not overwritten. Export your draft before reloading.",
    )
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl Default for UdlRegistry {
    fn default() -> Self {
        let mut definition = Definition::default();
        definition.name.clear();
        Self {
            entries: BTreeMap::from([(
                0,
                Entry {
                    definition,
                    revision: 1,
                    extra: Default::default(),
                },
            )]),
            revision: 1,
            next_id: 1,
            extra: Default::default(),
            persisted_base: None,
        }
    }
}

impl UdlRegistry {
    pub fn definition(&self, id: u64) -> Option<&Definition> {
        self.entries.get(&id).map(|e| &e.definition)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn has_unsaved_changes(&self) -> bool {
        self.persisted_base
            .as_ref()
            .and_then(|base| base.get("revision"))
            .and_then(Value::as_u64)
            .unwrap_or(1)
            != self.revision
    }
    pub fn definition_revision(&self, id: u64) -> Option<u64> {
        self.entries.get(&id).map(|e| e.revision)
    }
    pub fn named(&self) -> impl Iterator<Item = (u64, &Definition)> {
        self.entries
            .iter()
            .filter(|(id, _)| **id != 0)
            .map(|(id, e)| (*id, &e.definition))
    }
    pub fn load(store: &ConfigStore) -> io::Result<Self> {
        match store.load_json(Path::new("udl.json")) {
            Ok(value) => {
                let mut registry = Self::from_json(&value)?;
                registry.persisted_base = Some(value);
                Ok(registry)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut registry = Self::default();
                registry
                    .import_json(&serde_json::from_str(include_str!("../assets/udls.json"))?)?;
                registry.revision = 1;
                for entry in registry.entries.values_mut() {
                    entry.revision = 1;
                }
                Ok(registry)
            }
            Err(error) => Err(error),
        }
    }
    pub fn save(&mut self, store: &ConfigStore) -> io::Result<SaveReceipt> {
        let snapshot = self.to_json();
        let persisted = store.update_json(Path::new("udl.json"), &snapshot, |current| {
            Self::from_json(current)?;
            let actual = match store.load_json(Path::new("udl.json")) {
                Ok(value) => Some(value),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if actual != self.persisted_base {
                return Err(conflict());
            }
            *current = snapshot.clone();
            Ok(())
        })?;
        let receipt = SaveReceipt {
            previous_base: self.persisted_base.clone(),
            persisted,
        };
        self.acknowledge_save(&receipt)?;
        Ok(receipt)
    }
    pub fn acknowledge_save(&mut self, receipt: &SaveReceipt) -> io::Result<()> {
        if self.persisted_base != receipt.previous_base {
            return Err(conflict());
        }
        self.persisted_base = Some(receipt.persisted.clone());
        Ok(())
    }
    pub fn from_json(value: &Value) -> io::Result<Self> {
        let mut extra = value
            .as_object()
            .cloned()
            .ok_or_else(|| invalid("UDL registry object required"))?;
        if extra.remove("schema_version").and_then(|v| v.as_u64()) != Some(1) {
            return Err(invalid("UDL registry requires schema_version 1"));
        }
        let revision = extra
            .remove("revision")
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("invalid UDL revision"))?;
        let next_id = extra
            .remove("next_id")
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("invalid UDL next_id"))?;
        let items = extra
            .remove("definitions")
            .ok_or_else(|| invalid("missing UDL definitions"))?;
        let mut registry = Self {
            entries: BTreeMap::new(),
            revision,
            next_id,
            extra,
            persisted_base: None,
        };
        for item in items
            .as_array()
            .ok_or_else(|| invalid("UDL definitions must be an array"))?
        {
            let mut fields = item
                .as_object()
                .cloned()
                .ok_or_else(|| invalid("UDL entry object required"))?;
            let id = fields
                .remove("id")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| invalid("invalid UDL identity"))?;
            let revision = fields
                .remove("revision")
                .and_then(|v| v.as_u64())
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("invalid definition revision"))?;
            let definition = Definition::from_json(
                &fields
                    .remove("definition")
                    .ok_or_else(|| invalid("missing UDL definition"))?,
            )?;
            if id >= next_id || registry.entries.contains_key(&id) {
                return Err(invalid("duplicate or unallocated UDL identity"));
            }
            if id != 0 {
                registry.check_name(&definition.name, None)?;
            } else if !definition.name.is_empty() {
                return Err(invalid("working definition must be unnamed"));
            }
            registry.entries.insert(
                id,
                Entry {
                    definition,
                    revision,
                    extra: fields,
                },
            );
        }
        if !registry.entries.contains_key(&0) {
            return Err(invalid("missing unnamed working definition"));
        }
        Ok(registry)
    }
    pub fn to_json(&self) -> Value {
        let mut value = self.extra.clone();
        value.extend(serde_json::Map::from_iter([
            ("schema_version".into(), json!(1)),
            ("revision".into(), json!(self.revision)),
            ("next_id".into(), json!(self.next_id)),
            (
                "definitions".into(),
                Value::Array(
                    self.entries
                        .iter()
                        .map(|(id, entry)| {
                            let mut fields = entry.extra.clone();
                            fields.extend(serde_json::Map::from_iter([
                                ("id".into(), json!(id)),
                                ("revision".into(), json!(entry.revision)),
                                ("definition".into(), entry.definition.to_json()),
                            ]));
                            Value::Object(fields)
                        })
                        .collect(),
                ),
            ),
        ]));
        Value::Object(value)
    }
    fn check_name(&self, name: &str, except: Option<u64>) -> io::Result<()> {
        if name.trim().is_empty() || name.trim() != name || name.chars().any(char::is_control) {
            return Err(invalid(
                "Enter a nonempty language name without surrounding spaces or control characters",
            ));
        }
        if self
            .named()
            .any(|(id, d)| Some(id) != except && d.name.to_lowercase() == name.to_lowercase())
        {
            return Err(invalid("A language with that name already exists"));
        }
        Ok(())
    }
    fn changed(&mut self) -> io::Result<()> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("UDL revision exhausted"))?;
        Ok(())
    }
    pub fn edit(
        &mut self,
        id: u64,
        edit: impl FnOnce(&mut Definition) -> io::Result<()>,
    ) -> io::Result<()> {
        let existing = self
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown language"))?;
        let mut candidate = existing.definition.clone();
        edit(&mut candidate)?;
        let candidate = Definition::from_json(&candidate.to_json())?;
        if id == 0 {
            if !candidate.name.is_empty() {
                return Err(invalid("Use Save As to name the working definition"));
            }
        } else {
            self.check_name(&candidate.name, Some(id))?;
        }
        if candidate == existing.definition {
            return Ok(());
        }
        let revision = existing
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Definition revision exhausted"))?;
        self.changed()?;
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.definition = candidate;
            entry.revision = revision;
        }
        Ok(())
    }
    fn insert(&mut self, mut definition: Definition, name: &str) -> io::Result<u64> {
        self.check_name(name, None)?;
        let id = self.next_id;
        let next = id
            .checked_add(1)
            .ok_or_else(|| invalid("UDL identities exhausted"))?;
        definition.name = name.to_owned();
        self.changed()?;
        self.next_id = next;
        self.entries.insert(
            id,
            Entry {
                definition,
                revision: 1,
                extra: Default::default(),
            },
        );
        Ok(id)
    }
    pub fn create(&mut self, name: &str) -> io::Result<u64> {
        self.save_as(0, name)
    }
    pub fn save_as(&mut self, id: u64, name: &str) -> io::Result<u64> {
        let definition = self
            .definition(id)
            .ok_or_else(|| invalid("Unknown language"))?
            .clone();
        self.insert(definition, name)
    }
    pub fn rename(&mut self, id: u64, name: &str) -> io::Result<()> {
        self.edit(id, |d| {
            d.name = name.into();
            Ok(())
        })
    }
    pub fn remove(&mut self, id: u64) -> io::Result<()> {
        if id == 0 || !self.entries.contains_key(&id) {
            return Err(invalid("Select a named language to remove"));
        }
        self.changed()?;
        self.entries.remove(&id);
        Ok(())
    }
    pub fn match_path(&self, path: &Path) -> Option<u64> {
        self.match_path_for_theme(path, false)
    }

    pub fn match_path_for_theme(&self, path: &Path, dark: bool) -> Option<u64> {
        let name = path.file_name()?.to_str()?.to_lowercase();
        let mut matched = None;
        for (id, definition) in self.named() {
            if definition.extensions.split_whitespace().any(|ext| {
                let ext = ext
                    .trim_start_matches("*.")
                    .trim_start_matches('.')
                    .to_lowercase();
                !ext.is_empty() && (name == ext || name.ends_with(&format!(".{ext}")))
            }) {
                matched = Some(id);
                if definition.dark_mode_theme == dark {
                    return matched;
                }
            }
        }
        matched
    }
    pub fn import_json(&mut self, value: &Value) -> io::Result<Vec<u64>> {
        let values = if value.get("definitions").is_some() {
            if value.get("schema_version").and_then(Value::as_u64) != Some(1) {
                return Err(invalid("Unsupported UDL bundle version"));
            }
            value["definitions"]
                .as_array()
                .ok_or_else(|| invalid("Invalid UDL bundle"))?
                .clone()
        } else {
            vec![value.clone()]
        };
        if values.is_empty() {
            return Err(invalid("UDL import contains no definitions"));
        }
        let mut candidate = self.clone();
        let mut ids = Vec::new();
        for value in values {
            let definition = Definition::from_json(&value)?;
            let name = definition.name.clone();
            ids.push(candidate.insert(definition, &name)?);
        }
        *self = candidate;
        Ok(ids)
    }
    pub fn export_json(&self, id: u64) -> io::Result<Value> {
        let definition = self
            .definition(id)
            .ok_or_else(|| invalid("Unknown language"))?;
        if id == 0 {
            return Err(invalid("Save As a named language before exporting"));
        }
        Ok(json!({"schema_version":1,"definitions":[definition.to_json()]}))
    }
}
