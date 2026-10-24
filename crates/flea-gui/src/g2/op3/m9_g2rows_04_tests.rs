use super::*;
use crate::g2::op3::{lane, tests::english};
use std::path::PathBuf;

fn preview() -> BulkRename {
    let mut card = BulkRename::open(vec!["/fixture/alpha.txt".into(), "/fixture/beta.md".into()]);
    card.key(Key::Text("a".into()));
    card.key(Key::Tab);
    card.key(Key::Text("e".into()));
    assert_eq!(card.names, ["elphe.txt", "bete.md"]);
    card
}

fn tab_to(card: &mut BulkRename, stop: Stop) {
    for _ in logic::focus_stops(card.names.len()) {
        if card.focus == stop {
            return;
        }
        assert!(card.key(Key::Tab).is_empty());
    }
    panic!("Tab did not reach {stop:?}");
}

fn assert_rename_blocked(card: &BulkRename) {
    assert!(!card.card(&english()).buttons[2].enabled);
    for stop in [Stop::Find, Stop::Replace, Stop::Rename] {
        let mut keyed = card.clone();
        keyed.focus = stop;
        assert!(keyed.key(Key::Enter).is_empty(), "Enter at {stop:?}");
    }
    assert!(card.clone().press(Stop::Rename).is_empty());
}

fn editor_root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.superpowers/m9-g2rows-04")
        .canonicalize()
        .unwrap_or_else(|_| {
            let parent =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/m9-g2rows-04");
            std::fs::create_dir_all(&parent).unwrap();
            parent.canonicalize().unwrap()
        });
    let output = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("editor.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(&parent) && root != parent);
    std::fs::write(root.join(".flea-test-sandbox"), b"").unwrap();
    root
}

#[test]
fn m9_g2rows_04_enter_and_pointer_on_edit_share_one_effect() {
    let mut keyed = preview();
    let mut pointed = keyed.clone();
    tab_to(&mut keyed, Stop::Edit);
    let expected = vec![Effect::EditNames {
        folder: "/fixture".into(),
        names: vec!["elphe.txt".into(), "bete.md".into()],
    }];
    assert_eq!(keyed.key(Key::Enter), expected);
    assert_eq!(pointed.press(Stop::Edit), expected);
    for mut busy in [keyed, pointed] {
        assert!(busy.editing);
        assert!(!busy.card(&english()).buttons[0].enabled);
        assert!(busy.key(Key::Enter).is_empty());
        assert!(busy.press(Stop::Edit).is_empty());
        assert_rename_blocked(&busy);
        assert_eq!(busy.names, ["elphe.txt", "bete.md"]);
    }
    let mut rename = preview();
    tab_to(&mut rename, Stop::Rename);
    assert_eq!(rename.key(Key::Enter), vec![Effect::Send(r#"{"c":"bulkrename","paths":["/fixture/alpha.txt","/fixture/beta.md"],"names":["elphe.txt","bete.md"]}"#.into()), Effect::Close]);
}

#[test]
fn m9_g2rows_04_cancel_disabled_and_busy_enter_are_safe() {
    let i18n = english();
    let unchanged = BulkRename::open(vec!["/fixture/alpha.txt".into(), "/fixture/beta.md".into()]);
    assert_rename_blocked(&unchanged);
    assert_eq!(unchanged.card(&i18n).buttons[2].label, "Rename");
    let mut busy = preview();
    assert_eq!(busy.press(Stop::Edit).len(), 1);
    assert_rename_blocked(&busy);
    for card in [unchanged, preview(), busy] {
        let mut keyed = card.clone();
        tab_to(&mut keyed, Stop::Cancel);
        assert_eq!(keyed.key(Key::Enter), [Effect::Close]);
        assert_eq!(card.clone().press(Stop::Cancel), [Effect::Close]);
        assert_eq!(card.clone().key(Key::Escape), [Effect::Close]);
    }
    let invalid = [
        "gamma.txt\ngamma.txt\n".to_string(),
        "\nbeta.md\n".into(),
        "bad/name\nbeta.md\n".into(),
        ".\nbeta.md\n".into(),
        "..\nbeta.md\n".into(),
        "bad\0name\nbeta.md\n".into(),
        "bad\rname\nbeta.md\n".into(),
        format!("{}\nbeta.md\n", "x".repeat(logic::NAME_BYTES + 1)),
    ];
    for text in invalid {
        let mut card = preview();
        card.press(Stop::Edit);
        card.edited(&i18n, Ok(text));
        assert!(!card.editing);
        assert!(card.counts().invalid > 0);
        assert_rename_blocked(&card);
        assert!(
            matches!(&card.card(&i18n).body[1], Block::Names { new, .. } if new[0].ink == Ink::Error && !new[0].reason.is_empty())
        );
    }
    let mut valid = preview();
    assert!(valid.card(&i18n).buttons[2].enabled);
    assert!(matches!(
        valid.press(Stop::Rename).as_slice(),
        [Effect::Send(_), Effect::Close]
    ));
}

#[test]
fn m9_g2rows_04_editor_return_updates_preview_before_rename() {
    let i18n = english();
    let root = editor_root();
    let base = root.to_str().unwrap();
    let mut card = preview();
    let snapshot = card.names.clone();
    card.press(Stop::Edit);
    let outcome = lane::edit_names_with(base, &snapshot, |file| {
        let file = std::path::Path::new(file);
        assert!(file.is_absolute() && file.starts_with(&root));
        assert!(root.join(".flea-test-sandbox").is_file());
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "elphe.txt\nbete.md\n"
        );
        std::fs::write(file, "one.txt\ntwo.md\n").is_ok()
    });
    assert_eq!(
        card.names, snapshot,
        "the old complete preview waits for return"
    );
    assert_rename_blocked(&card);
    card.edited(&i18n, outcome);
    assert_eq!(card.names, ["one.txt", "two.md"]);
    assert_eq!(card.base, card.names);
    assert!(card.find.is_empty() && card.replace.is_empty());
    let drawn = card.card(&i18n);
    assert!(
        matches!(&drawn.body[0], Block::Fields(find, replace) if find.value.is_empty() && replace.value.is_empty())
    );
    assert!(
        matches!(&drawn.body[1], Block::Names { new, .. } if new.iter().map(|n| n.text.as_str()).collect::<Vec<_>>() == ["one.txt", "two.md"])
    );
    assert_eq!(drawn.buttons[2].label, "Rename 2");
    assert!(drawn.buttons[2].enabled);
    assert!(
        matches!(&drawn.body[2], Block::Note { text, ink: Ink::Foreground, .. } if text == "2 renamed \u{b7} 0 unchanged")
    );
    assert_eq!(card.clone().press(Stop::Rename), vec![Effect::Send(r#"{"c":"bulkrename","paths":["/fixture/alpha.txt","/fixture/beta.md"],"names":["one.txt","two.md"]}"#.into()), Effect::Close]);
    for (outcome, key) in [
        (Ok("only.txt\n".into()), "dialogs.file.bulk.err_count"),
        (
            Ok("one\ntwo\nthree\n".into()),
            "dialogs.file.bulk.err_count",
        ),
        (
            Ok("x".repeat(2 * logic::NAME_LINE_BYTES + 1)),
            "dialogs.file.bulk.err_limits",
        ),
        (
            lane::edit_names_with(base, &card.names, |_| false),
            "dialogs.file.bulk.err_editor",
        ),
    ] {
        let mut refused = preview();
        refused.press(Stop::Edit);
        refused.edited(&i18n, outcome);
        assert!(!refused.editing);
        assert_eq!(
            refused.names, snapshot,
            "a refused return preserves the preview"
        );
        assert_eq!(
            (&refused.find, &refused.replace),
            (&"a".to_string(), &"e".to_string())
        );
        assert!(
            matches!(&refused.card(&i18n).body[2], Block::Note { text, ink: Ink::Error, .. } if *text == i18n.tr(key))
        );
    }
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        1,
        "only the marker remains after both editor exits"
    );
}
