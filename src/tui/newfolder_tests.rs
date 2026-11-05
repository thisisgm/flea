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

// Enter on the suggestion keeps the folder this key has always made: the suggested name carries
// the trailing slash that asks for one, and its base travels as defaultBase so the backend takes
// the first free name.
#[test]
fn newfolder_tui_return_submits_localized_default_as_the_folder_it_suggests() {
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
        actions::act(&mut m, "newFolder", &mut w).unwrap();
        let editor = m.editor.as_mut().unwrap();
        assert_eq!(editor.value, format!("{base}/"));
        actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
        let sent = drain(&mut w);
        let ask = sent.iter().find(|v| text(v, "c") == "mkdir").unwrap();
        assert_eq!(text(ask, "path"), "/listing");
        assert_eq!(text(ask, "name"), "");
        assert_eq!(text(ask, "defaultBase"), base);
        assert!(m.editor.as_ref().unwrap().pending);
        m.editor = None;
        wire::finish(w, reader);
    }
}

// One prompt makes either entry: a typed name without a trailing slash is a file, and the same
// name with one is the folder of that name, which is the rule yazi's create prompt follows.
#[test]
fn newfolder_tui_types_a_file_and_asks_a_folder_with_a_trailing_slash() {
    for (typed, kind, name) in [
        ("my folder", "newfile", "my folder"),
        ("notes.md", "newfile", "notes.md"),
        ("my folder/", "mkdir", "my folder"),
        ("notes.md/", "mkdir", "notes.md"),
    ] {
        let d = TestDir::new("newfolder-tui-slash");
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "default");
        m.prompts.i18n = catalog(&d, "New Folder");
        actions::act(&mut m, "newFolder", &mut w).unwrap();
        m.editor.as_mut().unwrap().replace(typed.into());
        actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
        let sent = drain(&mut w);
        let ask = sent
            .iter()
            .find(|v| text(v, "c") == kind)
            .unwrap_or_else(|| panic!("{typed}: no {kind} request: {sent:?}"));
        assert_eq!(text(ask, "name"), name, "{typed}");
        assert_eq!(text(ask, "path"), "/listing", "{typed}");
        assert!(ask.get("defaultBase").is_none(), "{typed}");
        assert!(m.editor.as_ref().unwrap().pending, "{typed}");
        m.editor = None;
        wire::finish(w, reader);
    }
}

// The yazi preset's own file prompt takes the same rule, so `a` creates a folder once the name
// ends in a slash, and a bare name still creates a file.
#[test]
fn newfile_tui_takes_the_same_trailing_slash_rule() {
    let d = TestDir::new("newfile-tui-slash");
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.prompts.i18n = catalog(&d, "New Folder");
    for (typed, kind, name) in [("exact.txt", "newfile", "exact.txt"), ("photos/", "mkdir", "photos")] {
        m.editor = Some(super::editor::Editor::named("newfile", typed.into(), m.path.clone()));
        actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
        let sent = drain(&mut w);
        let ask = sent.iter().find(|v| text(v, "c") == kind).unwrap();
        assert_eq!(text(ask, "name"), name, "{typed}");
        assert!(ask.get("defaultBase").is_none(), "{typed}");
        assert!(m.editor.as_ref().unwrap().pending, "{typed}");
        m.editor = None;
    }
    wire::finish(w, reader);
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
    // Only a trailing slash is a separator; a draft that walks out of the folder is still refused,
    // and so is one that hides a second slash behind the trailing one.
    for draft in ["../escape", "../escape/", "a//"] {
        actions::act(&mut m, "newFolder", &mut w).unwrap();
        m.editor.as_mut().unwrap().replace(draft.into());
        actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
        assert!(drain(&mut w).is_empty(), "{draft} asked for an entry");
        assert_eq!(m.editor.as_ref().unwrap().value, draft);
        assert!(!m.editor.as_ref().unwrap().pending);
        m.editor = None;
    }
    wire::finish(w, reader);
}
