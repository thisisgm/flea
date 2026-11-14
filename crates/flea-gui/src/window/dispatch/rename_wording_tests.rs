use super::super::{close, editing, ready, receive, sent};
use crate::backend::Event;
use gpui::TestAppContext;

#[gpui::test]
fn wording_identity_refusal_uses_the_existing_chinese_sentence(cx: &mut TestAppContext) {
    let root = crate::window::dispatch::previewtypes_kinds_tests::Root::new("rename-wording-zh");
    let locale = root.0.join("locale");
    std::fs::create_dir_all(locale.join("default/i18n")).unwrap();
    std::fs::write(
        locale.join("default/i18n/locales.json"),
        include_str!("../../../../../src/i18n/fixtures/locales.json"),
    )
    .unwrap();
    std::fs::write(locale.join("language"), "zh-CN").unwrap();
    let shipped =
        flea::jsondoc::parse(include_str!("../../../../../i18n/zh-CN/flea.json")).unwrap();
    let sentence = shipped
        .get("jobs.rename.changed")
        .unwrap()
        .as_str()
        .unwrap();
    std::fs::create_dir_all(locale.join("catalogs/zh-CN")).unwrap();
    std::fs::write(
        locale.join("catalogs/zh-CN/flea.json"),
        format!(
            r#"{{"jobs.rename.changed":"{}"}}"#,
            flea::json::escape(sentence)
        ),
    )
    .unwrap();
    let catalog = std::sync::Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: locale.clone(),
        language_file: locale.join("language"),
        catalogs: vec![locale.join("catalogs")],
    }));
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| shell.locale.catalog = catalog);
    cx.simulate_keystrokes("f2");
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1);
    receive(
        &shell,
        cx,
        Event::Refused {
            operation: "rename".into(),
            path: "/files/notes.txt".into(),
            message: "Selected item changed; reopen the menu.".into(),
            mode: 0,
        },
    );
    let error = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().error.clone()
    });
    assert_eq!(error, "项目已改变；请重新打开“重命名”。");
    assert_eq!(editing(&shell, cx).unwrap().1, "draft.txt");
    close(&shell, cx);
}

#[gpui::test]
fn wording_keyboard_and_menu_rename_refusals_name_rename(cx: &mut TestAppContext) {
    for entry in ["r", "f2", "menu"] {
        for message in [
            "Selected item changed; reopen the menu.",
            "Selected item moved or disappeared; reopen the menu.",
            "Selected items changed; reopen the menu.",
        ] {
            let (shell, cx, mut lines) = ready(cx, "default");
            if entry == "menu" {
                shell.update_in(cx, |shell, window, cx| {
                    assert!(shell.rename_open(0, 17, window, cx));
                });
            } else {
                cx.simulate_keystrokes(entry);
            }
            cx.simulate_input("draft");
            cx.simulate_keystrokes("enter");
            assert_eq!(sent(&mut lines).len(), 1);
            receive(
                &shell,
                cx,
                Event::Refused {
                    operation: "rename".into(),
                    path: "/files/notes.txt".into(),
                    message: message.into(),
                    mode: 0,
                },
            );
            let expected = shell.read_with(cx, |shell, _| {
                shell.locale.catalog.tr("jobs.rename.changed")
            });
            let actual = shell.read_with(cx, |shell, _| {
                shell.rename.edit.as_ref().unwrap().error.clone()
            });
            assert_eq!(actual, expected, "entry {entry}, refusal {message}");
            assert_eq!(actual, "Item changed; reopen Rename.");
            assert_eq!(editing(&shell, cx).unwrap().1, "draft.txt");
            close(&shell, cx);
        }
    }
}
