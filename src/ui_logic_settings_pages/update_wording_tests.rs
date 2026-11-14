use super::*;
use crate::ui_logic_settings_pages::{about, test_support::say};

#[test]
fn wording_unchecked_packages_use_the_packaged_install_sentence() {
    for (owner, signed) in [
        ("flea", true),
        ("flea", false),
        ("flea-bin", false),
        ("flea-git", false),
    ] {
        let kind = about::install_kind(owner, signed);
        let status = answered(&idle(), &format!("unchecked {kind} 0.4.2-2 -"), 1);
        assert_eq!(
            say(&value(&status)),
            "Release build",
            "owner {owner}, signed {signed}"
        );
        assert_eq!(
            say(&note(&status).unwrap().0),
            "Omarchy update installs newer releases."
        );
        assert!(is_fact(&status));
        assert!(!acts(&status));
    }
}

#[test]
fn wording_unowned_build_keeps_the_unowned_state() {
    let kind = about::install_kind("", false);
    let status = answered(&idle(), &format!("unchecked {kind} - -"), 1);
    assert_eq!(kind, "unowned");
    assert_eq!(say(&value(&status)), "Built from source");
    assert_eq!(
        say(&note(&status).unwrap().0),
        "Rebuild from the checkout to update."
    );
    assert!(!acts(&status));
}

#[test]
fn wording_opr_checks_keep_their_update_state() {
    for (line, expected) in [
        ("current opr 0.4.2-2 -", "Up to date · 0.4.2"),
        ("available opr 0.4.2-2 0.4.3-1", "0.4.3 available"),
        ("failed opr 0.4.2-2 -", "Could not check"),
    ] {
        let status = answered(&idle(), line, 1);
        assert_eq!(say(&value(&status)), expected);
        assert!(acts(&status));
        assert!(!is_fact(&status));
    }
}
