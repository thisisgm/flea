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

// A drive's name refusal reads in German in the rename editor, the footer, a failed paste and a failed extract.
#[test]
fn a_drive_name_refusal_reads_in_german_wherever_the_terminal_says_it() {
    let (root, german) = german();
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    let mut m = Model::new(PathBuf::from("/listing"), &Json::Null);
    m.i18n = german.clone();
    m.prompts.i18n = german;
    let mut editor =
        crate::tui::editor::Editor::new("rename", "a:b".into(), PathBuf::from("/listing/ab"));
    editor.pending = true;
    m.editor = Some(editor);
    let refusal = r#"{"t":"error","where":"rename","path":"/listing/ab","msg":"':' is not allowed in a name on this vfat drive"}"#;
    m.receive(parse(refusal).unwrap(), &mut wire).unwrap();
    let editor = m.editor.as_ref().unwrap();
    assert!(!editor.pending);
    assert_eq!(
        editor.error,
        "„:“ ist in einem Namen auf diesem vfat-Laufwerk nicht erlaubt."
    );
    m.receive(parse(r#"{"t":"error","where":"mkdir","path":"/listing","msg":"'CON' is reserved on this exfat drive"}"#).unwrap(), &mut wire).unwrap();
    assert_eq!(
        m.error,
        "mkdir: „CON“ ist auf diesem exfat-Laufwerk ein reservierter Name"
    );
    // The language file changing after both refusals landed says them again in English, through the model's own switch.
    root.file("language", "en");
    assert!(m.refresh_locale(&[]));
    assert_eq!(
        m.editor.as_ref().unwrap().error,
        "':' is not allowed in a name on this vfat drive."
    );
    assert_eq!(m.error, "mkdir: 'CON' is reserved on this exfat drive");
    root.file("language", "de");
    assert!(m.refresh_locale(&[]));
    m.editor = None;
    m.errors.clear();
    m.error.clear();
    m.receive(parse(r#"{"t":"transferitem","id":1,"index":0,"name":"trail.","ok":false,"err":"a name cannot end in a dot on this vfat drive"}"#).unwrap(), &mut wire).unwrap();
    assert_eq!(
        m.error,
        "trail.: Ein Name darf auf diesem vfat-Laufwerk nicht mit einem Punkt enden"
    );
    m.errors.clear();
    m.error.clear();
    let extract = r#"{"t":"job","job":{"id":5,"kind":"extract","state":"failed","reason":"'set/clip: final.txt' cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive"},"unfinished":0}"#;
    super::receive(&mut m, &parse(extract).unwrap());
    assert_eq!(m.error, "„set/clip: final.txt“ kann nicht auf dieses Laufwerk entpackt werden: „:“ ist in einem Namen auf diesem vfat-Laufwerk nicht erlaubt.");
    crate::tui::wire::finish(wire, reader);
}

#[test]
fn fxi18n3_paste_summary_retranslates_the_held_terminal_error() {
    let (root, german) = german();
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    model.i18n = german;
    model.receive(parse(r#"{"t":"transferitem","id":1,"index":0,"name":"bundle","ok":false,"err":"2 items inside could not be copied; the first is sub/link: this drive cannot hold links"}"#).unwrap(), &mut wire).unwrap();
    assert_eq!(model.error, "bundle: 2 Elemente darin konnten nicht kopiert werden; das erste ist sub/link: Dieses Laufwerk kann keine Verknüpfungen speichern");
    root.file("language", "en");
    assert!(model.refresh_locale(&[]));
    assert_eq!(model.error, "bundle: 2 items inside could not be copied; the first is sub/link: this drive cannot hold links");
    crate::tui::wire::finish(wire, reader);
}
