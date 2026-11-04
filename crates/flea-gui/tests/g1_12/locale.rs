//! The bar's sentences: each key resolves, a missing translation falls back to English, and a language change reaches the open bar.
use super::{inputs, view::bar, HOME};
use flea::i18n::{I18n, Paths};
use flea_gui::path::{suggestions::Unranked, view::Out, Answer, Peeked};
use gpui::TestAppContext;
use std::{path::PathBuf, process::Command, sync::Arc};

fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-12");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-12\n").unwrap();
    root
}

fn catalog() -> (PathBuf, I18n) {
    let root = root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "en").unwrap();
    let de = r#"{"path.nothing_starts_with":"In %1 beginnt nichts damit","path.items_start_with":{"one":"%1 Eintrag beginnt damit","other":"%1 Einträge beginnen damit"}}"#;
    std::fs::write(root.join("catalog/de/flea.json"), de).unwrap();
    let catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    (root, catalog)
}

#[test]
fn the_path_keys_resolve_with_english_fallback_and_follow_a_language_change() {
    let (root, mut catalog) = catalog();
    assert_eq!(
        catalog.tr_args("path.nothing_starts_with", &["/etc"]),
        "Nothing in /etc starts with that"
    );
    assert_eq!(
        catalog.trn("path.items_start_with", 2),
        "2 items start with that"
    );
    std::fs::write(root.join("language.next"), "de\n").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed());
    assert_eq!(
        catalog.tr_args("path.nothing_starts_with", &["/etc"]),
        "In /etc beginnt nichts damit"
    );
    assert_eq!(
        catalog.trn("path.items_start_with", 1),
        "1 Eintrag beginnt damit"
    );
    assert_eq!(
        catalog.trn("path.items_start_with", 3),
        "3 Einträge beginnen damit"
    );
    assert_eq!(
        catalog.tr("path.refused_uri"),
        "That URI names a file on another host, not a path on this machine.",
        "English per-string fallback"
    );
    assert_eq!(
        catalog.tr("path.network_unavailable"),
        "That network address cannot be opened here."
    );
}

#[gpui::test]
fn a_language_change_reaches_the_bar_it_is_handed_to(cx: &mut TestAppContext) {
    let (root, mut catalog) = catalog();
    let mut bar = bar(cx, Box::new(Unranked), inputs(HOME));
    bar.update(|view, _, _| view.set_catalog(Arc::new(catalog.clone())));
    let tab = |bar: &mut super::view::Bar<'_>, typed: &str, dir: &str, dirs: &[&str]| {
        bar.update(|view, window, cx| view.open(window, cx));
        bar.cx.simulate_input(typed);
        bar.cx.simulate_keystrokes("tab");
        let peeked = Peeked {
            path: dir.into(),
            dirs: dirs.iter().map(|dir| dir.to_string()).collect(),
            ..Peeked::default()
        };
        bar.update(|view, window, cx| view.receive(Answer::Peeked(peeked), window, cx));
        let said: Vec<Out> = bar
            .take()
            .into_iter()
            .filter(|out| matches!(out, Out::Said(_)))
            .collect();
        bar.cx.simulate_keystrokes("escape");
        bar.take();
        said
    };
    assert_eq!(
        tab(&mut bar, "/e", "/", &["efi", "etc"]),
        [Out::Said("2 items start with that".into())]
    );
    std::fs::write(root.join("language.next"), "de\n").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed());
    bar.update(|view, _, _| view.set_catalog(Arc::new(catalog.clone())));
    assert_eq!(
        tab(&mut bar, "/e", "/", &["efi", "etc"]),
        [Out::Said("2 Einträge beginnen damit".into())]
    );
    assert_eq!(
        tab(&mut bar, "/zz", "/", &["etc"]),
        [Out::Said("In / beginnt nichts damit".into())]
    );
    // A key the new language lacks still speaks English.
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_input("file://otherbox/etc");
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(
        bar.take(),
        [
            Out::Closed,
            Out::Said("That URI names a file on another host, not a path on this machine.".into())
        ]
    );
}
