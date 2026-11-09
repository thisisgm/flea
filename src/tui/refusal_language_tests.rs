// The backend words an extract refusal and a trash cause in English; the terminal reads both in the shipped German catalog.
use super::super::model::Model;
use crate::backend::testdir::TestDir;
use crate::i18n::{I18n, Paths};
use crate::jsondoc::{parse, Json};
use std::path::PathBuf;

fn german() -> (TestDir, I18n) {
    let root = TestDir::new("tui-refusal-de");
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    for (code, text) in [
        ("en", include_str!("../../i18n/en/flea.json")),
        ("de", include_str!("../../i18n/de/flea.json")),
    ] {
        std::fs::create_dir_all(root.join(&format!("catalog/{code}"))).unwrap();
        root.file(&format!("catalog/{code}/flea.json"), text);
    }
    root.file("language", "de");
    let catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    (root, catalog)
}

#[test]
fn an_extract_refusal_and_a_read_only_trash_cause_read_in_german() {
    let (_root, german) = german();
    let mut m = Model::new(PathBuf::from("/listing"), &Json::Null);
    m.i18n = german.clone();
    m.prompts.i18n = german;
    let twins = r#"{"t":"job","job":{"id":4,"kind":"extract","state":"failed","reason":"'a.txt' and 'A.txt' would take one name on this drive, so nothing was extracted"},"unfinished":0}"#;
    super::receive(&mut m, &parse(twins).unwrap());
    assert_eq!(m.error, "„a.txt“ und „A.txt“ hätten auf diesem Laufwerk denselben Namen, daher wurde nichts entpackt.");
    m.errors.clear();
    m.error.clear();
    super::super::prompts::receipt(
        &mut m,
        &parse(r#"{"t":"trashed","id":1,"ok":0,"failed":1,"err":"the drive is read-only"}"#)
            .unwrap(),
    );
    let said = std::iter::once(m.error.as_str())
        .chain(m.errors.iter().map(String::as_str))
        .collect::<Vec<_>>();
    assert!(said.contains(&"1 Element konnte nicht in den Papierkorb verschoben werden: Das Laufwerk ist schreibgeschützt."), "{said:?}");
}
