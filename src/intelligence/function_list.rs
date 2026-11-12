use crate::intelligence::{FunctionSymbol, SymbolKind};
use std::collections::BTreeSet;

pub use crate::intelligence::ParseOutcome;

pub struct FunctionListModel {
    visible: bool,
    filter: String,
    sorted: bool,
    default_sort: bool,
    outcome: ParseOutcome,
    error: Option<String>,
    collapsed: BTreeSet<usize>,
}

impl Default for FunctionListModel {
    fn default() -> Self {
        Self {
            visible: false,
            filter: String::new(),
            sorted: false,
            default_sort: false,
            outcome: ParseOutcome::NoParser,
            error: None,
            collapsed: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionRow {
    pub name: String,
    pub depth: usize,
    pub jump_byte: usize,
    pub kind: SymbolKind,
    pub has_children: bool,
    pub expanded: bool,
}

impl FunctionListModel {
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn open(&mut self) {
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    pub fn sorted(&self) -> bool {
        self.sorted
    }

    pub fn toggle_sort(&mut self) {
        self.sorted = !self.sorted;
    }

    pub fn default_sort(&self) -> bool {
        self.default_sort
    }

    pub fn set_default_sort(&mut self, value: bool) {
        self.default_sort = value;
    }

    pub fn apply_default_sort(&mut self) {
        self.sorted = self.default_sort && matches!(&self.outcome, ParseOutcome::Parsed(_));
    }

    pub fn outcome(&self) -> ParseOutcome {
        self.outcome.clone()
    }

    pub fn apply_result(&mut self, result: Result<ParseOutcome, String>) {
        self.error = None;
        self.outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.error = Some(error);
                ParseOutcome::NoParser
            }
        };
        self.filter.clear();
        self.sorted = self.default_sort && matches!(&self.outcome, ParseOutcome::Parsed(_));
        self.collapsed.clear();
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn toggle_expanded(&mut self, jump_byte: usize) {
        if !self.collapsed.remove(&jump_byte) {
            self.collapsed.insert(jump_byte);
        }
    }

    pub fn rows(&self) -> Vec<FunctionRow> {
        fn append(
            model: &FunctionListModel,
            symbols: &[FunctionSymbol],
            depth: usize,
            filter: &str,
            inherited: bool,
            rows: &mut Vec<FunctionRow>,
        ) {
            fn matches(symbol: &FunctionSymbol, filter: &str) -> bool {
                symbol.name.to_lowercase().contains(filter)
                    || symbol.children.iter().any(|child| matches(child, filter))
            }
            let mut symbols = symbols.iter().collect::<Vec<_>>();
            if model.sorted {
                symbols.sort_by_key(|symbol| symbol.name.to_lowercase());
            }
            for symbol in symbols {
                if !inherited && !matches(symbol, filter) {
                    continue;
                }
                let expanded = !model.collapsed.contains(&symbol.jump_byte) || !filter.is_empty();
                rows.push(FunctionRow {
                    name: symbol.name.clone(),
                    depth,
                    jump_byte: symbol.jump_byte,
                    kind: symbol.kind,
                    has_children: !symbol.children.is_empty(),
                    expanded,
                });
                if expanded {
                    append(
                        model,
                        &symbol.children,
                        depth + 1,
                        filter,
                        inherited || symbol.name.to_lowercase().contains(filter),
                        rows,
                    );
                }
            }
        }
        let mut rows = Vec::new();
        if let ParseOutcome::Parsed(symbols) = &self.outcome {
            append(
                self,
                symbols,
                0,
                &self.filter.to_lowercase(),
                false,
                &mut rows,
            );
        }
        rows
    }
}
