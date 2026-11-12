use intelligence::function_list::{FunctionListModel, ParseOutcome};
use notepad_matrix::intelligence;

#[test]
fn supported_modes_reload_to_no_parser_and_clear_search_and_sort() {
    let catalog = intelligence::IntelligenceCatalog::bundled().expect("catalog");
    for key in ["normal", "json"] {
        let mut panel = FunctionListModel::default();
        panel.open();
        panel.set_filter("function".into());
        panel.toggle_sort();
        panel.apply_result(
            catalog
                .parse_functions(key, "")
                .map_err(|error| error.to_string()),
        );
        assert_eq!(panel.outcome(), ParseOutcome::NoParser);
        assert_eq!(panel.filter(), "");
        assert!(!panel.sorted());
        assert!(panel.visible());
    }
}

#[test]
fn closing_retains_snapshot_but_switching_back_reloads_no_parser_state() {
    let catalog = intelligence::IntelligenceCatalog::bundled().expect("catalog");
    let mut panel = FunctionListModel::default();
    panel.open();
    panel.set_filter("needle".into());
    panel.toggle_sort();
    panel.close();
    assert!(!panel.visible());
    assert_eq!(panel.filter(), "needle");
    assert!(panel.sorted());
    panel.open();
    panel.apply_result(
        catalog
            .parse_functions("json", "")
            .map_err(|error| error.to_string()),
    );
    assert_eq!(panel.filter(), "");
    assert!(!panel.sorted());
    assert_eq!(panel.outcome(), ParseOutcome::NoParser);
}

#[test]
fn default_sort_does_not_sort_a_missing_parser_tree_on_reload() {
    let catalog = intelligence::IntelligenceCatalog::bundled().expect("catalog");
    let mut panel = FunctionListModel::default();
    assert!(!panel.default_sort());
    panel.set_default_sort(true);
    panel.open();
    panel.toggle_sort();
    panel.apply_result(
        catalog
            .parse_functions("normal", "")
            .map_err(|error| error.to_string()),
    );
    assert!(panel.default_sort());
    assert!(!panel.sorted());
    panel.close();
    assert!(panel.default_sort());
}

#[test]
fn builtin_python_populates_grouped_filtered_sorted_rows_and_jump_bytes() {
    let catalog = intelligence::IntelligenceCatalog::bundled().expect("catalog");
    let text = "# λ\n# def fake():\nclass Box:\n    def zebra(self):\n        pass\n    def apple(self):\n        pass\ndef free():\n    pass\n";
    let mut panel = FunctionListModel::default();
    panel.set_default_sort(true);
    panel.apply_result(
        catalog
            .parse_functions("python", text)
            .map_err(|error| error.to_string()),
    );
    assert!(panel.sorted());
    assert!(panel.error().is_none());
    assert_eq!(
        panel
            .rows()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Box", "apple(self)", "zebra(self)", "free()"]
    );
    assert_eq!(panel.rows()[1].depth, 1);
    assert_eq!(
        panel.rows()[1].jump_byte,
        text.find("apple").expect("source position")
    );
    panel.set_filter("zeb".into());
    assert_eq!(
        panel
            .rows()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Box", "zebra(self)"]
    );
    assert_eq!(
        panel.rows()[1].jump_byte,
        text.find("zebra").expect("source position")
    );
    panel.set_filter(String::new());
    panel.toggle_expanded(text.find("Box").expect("class"));
    assert_eq!(
        panel
            .rows()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Box", "free()"]
    );
}
