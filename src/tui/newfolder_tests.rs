use super::{
    action_key_tests::{drain, listing},
    actions,
    input::Key,
    keymap::Map,
    wire::{self, text},
};
use crate::backend::testdir::TestDir;
use flea::i18n::{I18n, Paths};

fn catalog(d: &TestDir, base: &str) -> I18n {
    d.dir("catalog/en");
    d.file(
        "catalog/en/flea.json",
        &format!(r#"{{"prompt.newFolder":"{}"}}"#, crate::json::escape(base)),
    );
    I18n::load(Paths {
        omarchy: d.join("omarchy"),
        language_file: d.join("language"),
        catalogs: vec![d.join("catalog")],
    })
}

#[test]
fn newfolder_tui_return_submits_localized_default_and_keeps_custom_and_newfile_exact() {
    for base in [
        "Nouveau dossier",
        "新建文件夹",
        "مجلد جديد",
        "नया फ़ोल्डर",
        "Новая папка",
        "say \"hi\"",
    ] {
        let d = TestDir::new("newfolder-tui");
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "default");
        m.prompts.i18n = catalog(&d, base);
        for custom in [None, Some("my folder")] {
            actions::act(&mut m, "newFolder", &mut w).unwrap();
            let editor = m.editor.as_mut().unwrap();
            assert_eq!(editor.value, base);
            if let Some(custom) = custom {
                editor.replace(custom.into());
            }
            actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
            let sent = drain(&mut w);
            let ask = sent.iter().find(|v| text(v, "c") == "mkdir").unwrap();
            assert_eq!(text(ask, "path"), "/listing");
            assert_eq!(text(ask, "name"), custom.unwrap_or(""));
            assert_eq!(
                text(ask, "defaultBase"),
                if custom.is_none() { base } else { "" }
            );
            assert!(m.editor.as_ref().unwrap().pending);
            m.editor = None;
        }
        m.editor = Some(super::editor::Editor::named(
            "newfile",
            "exact.txt".into(),
            m.path.clone(),
        ));
        actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
        let sent = drain(&mut w);
        let ask = sent.iter().find(|v| text(v, "c") == "newfile").unwrap();
        assert_eq!(text(ask, "name"), "exact.txt");
        assert!(ask.get("defaultBase").is_none());
        wire::finish(w, reader);
    }
}

#[test]
fn newfolder_tui_keeps_captured_suggestion_parent_and_invalid_draft() {
    let d = TestDir::new("newfolder-tui-draft");
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    m.prompts.i18n = catalog(&d, "新建文件夹");
    actions::act(&mut m, "newFolder", &mut w).unwrap();
    m.path = d.path().to_path_buf();
    m.prompts.i18n = catalog(&d, "Nouveau dossier");
    actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
    let sent = drain(&mut w);
    let ask = sent.iter().find(|v| text(v, "c") == "mkdir").unwrap();
    assert_eq!(text(ask, "defaultBase"), "新建文件夹");
    assert_eq!(text(ask, "path"), "/listing");
    m.editor = None;
    actions::act(&mut m, "newFolder", &mut w).unwrap();
    m.editor.as_mut().unwrap().replace("../escape".into());
    actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
    assert!(drain(&mut w).is_empty());
    assert_eq!(m.editor.as_ref().unwrap().value, "../escape");
    assert!(!m.editor.as_ref().unwrap().pending);
    wire::finish(w, reader);
}
