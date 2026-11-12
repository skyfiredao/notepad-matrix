#[path = "../src/ui/column_editor_surface/numbers.rs"]
mod numbers;

#[test]
fn numeric_fields_follow_the_selected_radix_and_allow_source_empty_zero() {
    assert_eq!(numbers::parse_value("ff", 16), Ok(255));
    assert_eq!(numbers::parse_value("17", 8), Ok(15));
    assert_eq!(numbers::parse_value("101", 2), Ok(5));
    assert_eq!(numbers::parse_value("", 10), Ok(0));
    assert!(numbers::parse_value("2", 2).is_err());
}

#[test]
fn radix_changes_preserve_value_and_hexadecimal_case() {
    assert_eq!(numbers::format_value(255, 16, false), "ff");
    assert_eq!(numbers::format_value(255, 16, true), "FF");
    assert_eq!(numbers::format_value(15, 8, false), "17");
    assert_eq!(numbers::format_value(5, 2, false), "101");
    assert_eq!(numbers::format_value(255, 10, false), "255");
}
