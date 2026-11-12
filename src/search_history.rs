#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Find,
    Replace,
    Filter,
    Directory,
}

impl Field {
    pub const ALL: [Self; 4] = [Self::Find, Self::Replace, Self::Filter, Self::Directory];

    pub fn capture(self, text: &str) -> String {
        let length = match self {
            Self::Find | Self::Replace => 16383,
            Self::Filter => 511,
            Self::Directory => 259,
        };
        let units: Vec<_> = text
            .encode_utf16()
            .take(length)
            .take_while(|unit| *unit != 0)
            .collect();
        let captured = String::from_utf16_lossy(&units);
        if self == Self::Directory {
            captured.trim_matches(' ').to_owned()
        } else {
            captured
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Find => 0,
            Self::Replace => 1,
            Self::Filter => 2,
            Self::Directory => 3,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Find => "finds",
            Self::Replace => "replaces",
            Self::Filter => "filters",
            Self::Directory => "paths",
        }
    }

    fn maximum(self) -> usize {
        match self {
            Self::Filter => 20,
            _ => 30,
        }
    }

    fn count_key(self) -> &'static str {
        match self {
            Self::Find => "nbMaxFindHistoryFind",
            Self::Replace => "nbMaxFindHistoryReplace",
            Self::Filter => "nbMaxFindHistoryFilter",
            Self::Directory => "nbMaxFindHistoryPath",
        }
    }

    fn save_empty(self) -> bool {
        matches!(self, Self::Replace | Self::Filter)
    }
}

pub struct History {
    entries: Vec<String>,
    field: Field,
    limit: usize,
    initial: String,
}

impl History {
    pub fn load(
        field: Field,
        settings: &serde_json::Value,
        equivalent: impl Fn(&str, &str) -> bool,
    ) -> Self {
        let configured = settings[field.count_key()].as_i64().unwrap_or(10);
        let limit = usize::try_from(configured)
            .unwrap_or(0)
            .min(field.maximum());
        let loaded: Vec<_> = settings[field.key()]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .take(if limit == 0 { 0 } else { field.maximum() })
            .collect();
        let mut history = Self {
            entries: Vec::new(),
            field,
            limit,
            initial: String::new(),
        };
        for value in loaded.iter().rev() {
            history.record(value, &equivalent);
        }
        history.initial = if loaded.first().is_some_and(|text| text.is_empty()) {
            String::new()
        } else {
            history.entries.first().cloned().unwrap_or_default()
        };
        history
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn initial_text(&self) -> &str {
        &self.initial
    }

    pub fn record(&mut self, text: &str, equivalent: impl Fn(&str, &str) -> bool) {
        if text.is_empty() {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| equivalent(entry, text))
        {
            self.entries.remove(index);
        }
        self.entries.insert(0, text.into());
    }

    pub fn saved(&self, current: &str) -> Vec<String> {
        let mut saved = Vec::new();
        if self.field.save_empty() && current.is_empty() {
            saved.push(String::new());
        }
        saved.extend(
            self.entries
                .iter()
                .take(self.limit)
                .filter(|text| text.encode_utf16().count() < 2048)
                .cloned(),
        );
        saved
    }
}
