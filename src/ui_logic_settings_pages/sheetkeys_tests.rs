use super::*;

#[test]
fn bare_modifiers_are_the_six_modifier_keys() {
    for key in ["Shift", "Control", "Alt", "AltGr", "Meta", "CapsLock"] {
        assert!(is_bare_modifier(key), "{key}");
    }
    assert!(!is_bare_modifier("Delete"));
    assert!(!is_bare_modifier("Escape"));
    assert!(!is_bare_modifier("A"));
}

#[test]
fn printable_is_one_character_and_never_del() {
    assert!(is_printable("c"));
    assert!(is_printable(" "));
    assert!(!is_printable("\u{7f}"));
    assert!(!is_printable(""));
    assert!(!is_printable("ab"));
}

#[test]
fn escape_closes_with_or_without_a_query() {
    assert_eq!(sheet_key("co", "Escape", ""), SheetKey::Close);
    assert_eq!(sheet_key("", "Escape", ""), SheetKey::Close);
}

#[test]
fn up_down_and_return_act_only_with_a_query() {
    assert_eq!(sheet_key("c", "Up", ""), SheetKey::Up);
    assert_eq!(sheet_key("c", "Down", ""), SheetKey::Down);
    assert_eq!(sheet_key("c", "Return", ""), SheetKey::Activate);
    assert_eq!(sheet_key("c", "Enter", ""), SheetKey::Activate);
    assert_eq!(sheet_key("", "Up", ""), SheetKey::Close);
}

#[test]
fn backspace_shortens_or_closes_when_empty() {
    assert_eq!(sheet_key("c", "Backspace", ""), SheetKey::Backspace);
    assert_eq!(sheet_key("", "Backspace", ""), SheetKey::Close);
}

#[test]
fn a_printable_key_types() {
    assert_eq!(sheet_key("", "C", "c"), SheetKey::Type);
}

#[test]
fn shift_is_ignored_with_or_without_a_query() {
    assert_eq!(sheet_key("co", "Shift", ""), SheetKey::Ignore);
    assert_eq!(sheet_key("", "Shift", ""), SheetKey::Ignore);
}

#[test]
fn delete_never_types_del() {
    assert_eq!(sheet_key("co", "Delete", "\u{7f}"), SheetKey::Ignore);
}

#[test]
fn an_unbound_key_closes() {
    assert_eq!(sheet_key("co", "F1", ""), SheetKey::Close);
}

#[test]
fn cursor_wraps_at_both_ends() {
    assert_eq!(step_cursor(0, -1, 3), 2);
    assert_eq!(step_cursor(2, 1, 3), 0);
    assert_eq!(step_cursor(1, -1, 3), 0);
}

#[test]
fn no_rows_parks_the_cursor() {
    assert_eq!(step_cursor(0, 1, 0), 0);
}
