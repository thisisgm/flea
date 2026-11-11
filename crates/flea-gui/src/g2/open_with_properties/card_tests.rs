//! Bulk rename, the run card, scaling, German and the edit lane, as plain data and real files.
use super::bulkrename::BulkRename;
use super::keys::Key;
use super::runcard::RunCard;
use super::shell::{scaled, Block, Card, Ink};
use super::tests::{buttons, english, language, notes, open_with, permissions};
use super::Effect;
use flea::ui_logic_open_with_properties::bulkrename::Stop as BulkStop;
use flea::ui_logic_open_with_properties::runfile::Button as RunButton;

fn names(card: &Card) -> (Vec<String>, Vec<(String, Ink, String)>, bool) {
    card.body
        .iter()
        .find_map(|b| match b {
            Block::Names {
                old, new, focused, ..
            } => Some((
                old.clone(),
                new.iter()
                    .map(|n| (n.text.clone(), n.ink, n.reason.clone()))
                    .collect(),
                *focused,
            )),
            _ => None,
        })
        .unwrap()
}

#[test]
fn bulk_rename_opens_unchanged_with_rename_unavailable() {
    let i18n = english();
    let card = BulkRename::open(vec!["/fx/b/alpha.txt".into(), "/fx/b/beta.md".into()]).card(&i18n);
    assert_eq!((card.title.as_str(), card.width), ("Rename 2 Items", 640.));
    let Block::Fields(find, replace) = &card.body[0] else {
        panic!()
    };
    assert_eq!(
        (
            find.label.as_deref(),
            find.focused,
            replace.label.as_deref(),
            replace.focused
        ),
        (Some("Find"), true, Some("Replace"), false)
    );
    let (old, new, _) = names(&card);
    assert_eq!(old, vec!["alpha.txt", "beta.md"]);
    assert_eq!(
        new,
        vec![
            ("alpha.txt".into(), Ink::Muted, String::new()),
            ("beta.md".into(), Ink::Muted, String::new())
        ]
    );
    assert!(
        matches!(&card.body[1], Block::Names { error, .. } if error.is_empty()),
        "the error line is held empty"
    );
    assert_eq!(
        notes(&card),
        vec![("0 renamed \u{b7} 2 unchanged".into(), Ink::Foreground)]
    );
    assert_eq!(buttons(&card), "Edit in Terminal\u{2026}|Cancel|Rename*~");
}

#[test]
fn bulk_rename_clash_board_names_the_later_line_and_drops_the_invalid_count() {
    let i18n = english();
    let mut card = BulkRename::open(vec!["/fx/b/alpha.txt".into(), "/fx/b/gamma.txt".into()]);
    for c in "alpha".chars() {
        card.key(Key::Text(c.to_string()));
    }
    card.key(Key::Tab);
    for c in "gamma".chars() {
        card.key(Key::Text(c.to_string()));
    }
    let drawn = card.card(&i18n);
    let (_, new, _) = names(&drawn);
    assert_eq!(
        new[0],
        ("gamma.txt".into(), Ink::Error, "same as line 2".into())
    );
    assert_eq!(new[1], ("gamma.txt".into(), Ink::Muted, String::new()));
    assert!(
        matches!(&drawn.body[1], Block::Names { error, .. } if error == "Give each item a different name.")
    );
    assert_eq!(notes(&drawn)[0].0, "0 renamed \u{b7} 1 unchanged");
    assert_eq!(buttons(&drawn), "Edit in Terminal\u{2026}|Cancel|Rename*~");
    let Block::Fields(_, replace) = &drawn.body[0] else {
        panic!()
    };
    assert!(replace.focused && replace.value == "gamma");
}

#[test]
fn bulk_rename_counts_on_the_default_and_sends_the_names_shown() {
    let i18n = english();
    let mut card = BulkRename::open(vec!["/fx/b/alpha.txt".into(), "/fx/b/beta.md".into()]);
    card.key(Key::Text("a".into()));
    card.key(Key::Tab);
    card.key(Key::Text("o".into()));
    assert_eq!(
        names(&card.card(&i18n)).1[0],
        ("olpho.txt".into(), Ink::Foreground, String::new())
    );
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Edit in Terminal\u{2026}|Cancel|Rename 2*"
    );
    let effects = card.press(BulkStop::Rename);
    assert_eq!(effects, vec![Effect::Send(r#"{"c":"bulkrename","paths":["/fx/b/alpha.txt","/fx/b/beta.md"],"names":["olpho.txt","beto.md"]}"#.into()), Effect::Close]);
}

#[test]
fn edit_in_terminal_by_enter_and_by_click_send_the_same_captured_names() {
    let i18n = english();
    let open = || {
        let mut card = BulkRename::open(vec!["/fx/b/alpha.txt".into(), "/fx/b/beta.md".into()]);
        card.key(Key::Text("a".into()));
        card.key(Key::Tab);
        card.key(Key::Text("e".into()));
        card
    };
    let mut keyed = open();
    while keyed.focus != BulkStop::Edit {
        keyed.key(Key::Tab);
    }
    let by_enter = keyed.key(Key::Enter);
    let by_click = open().press(BulkStop::Edit);
    assert_eq!(
        by_enter, by_click,
        "#305: Enter on Edit in Terminal is the click, never Rename"
    );
    assert_eq!(
        by_enter,
        vec![Effect::EditNames {
            folder: "/fx/b".into(),
            names: vec!["elphe.txt".into(), "bete.md".into()]
        }]
    );
    assert!(keyed.key(Key::Enter).is_empty(), "one editor at a time");
    keyed.edited(&i18n, Ok("one.txt\ntwo.md\n".into()));
    assert_eq!(
        names(&keyed.card(&i18n))
            .1
            .iter()
            .map(|n| n.0.clone())
            .collect::<Vec<_>>(),
        vec!["one.txt", "two.md"]
    );
    keyed.edited(&i18n, Ok("one.txt\n".into()));
    assert_eq!(
        notes(&keyed.card(&i18n))[0],
        (
            "Bulk rename refused: the number of names changed.".into(),
            Ink::Error
        )
    );
    keyed.edited(&i18n, Err("dialogs.file.bulk.err_editor"));
    assert_eq!(
        notes(&keyed.card(&i18n))[0].0,
        "Bulk rename could not open an editor."
    );
}

const ASK: &str = r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/fx/v/work/hello.sh"}"#;

#[test]
fn the_run_card_draws_three_buttons_with_run_in_terminal_the_focused_default() {
    let i18n = english();
    let mut card = RunCard::open(&i18n, "/usr/bin/flea", "/fx/v/work/hello.sh", ASK).unwrap();
    let drawn = card.card(&i18n);
    assert_eq!(
        (drawn.title.as_str(), drawn.width),
        ("hello.sh is a program.", 500.)
    );
    assert_eq!(
        drawn.body,
        vec![Block::Program {
            glyph: super::shell::Glyph::Code,
            text: "Shell script \u{b7} 21 B".into()
        }]
    );
    assert_eq!(buttons(&drawn), "Cancel|Open as Text|Run in Terminal*@");
    card.shift = true;
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Cancel|Open as Text|Run*@",
        "Shift relabels it Run"
    );
    assert_eq!(
        card.key(Key::Enter),
        vec![
            Effect::Launch(
                [
                    "/usr/bin/flea",
                    "--run",
                    "/fx/v/work/hello.sh",
                    "8",
                    "9",
                    "21",
                    "11",
                    "12"
                ]
                .map(String::from)
                .to_vec()
            ),
            Effect::Close
        ]
    );
    card.shift = false;
    card.key(Key::Text("h".into()));
    assert_eq!(card.focus, RunButton::Text);
    assert_eq!(
        card.key(Key::Enter)[0],
        Effect::Launch(
            ["/usr/bin/flea", "--open-text", "/fx/v/work/hello.sh"]
                .map(String::from)
                .to_vec()
        )
    );
    assert_eq!(
        RunCard::open(&i18n, "flea", "/x", "{\"ask\":\"run\"}")
            .err()
            .as_deref(),
        Some("Flea could not check that file, so it was not run.")
    );
}

#[test]
fn cards_grow_with_the_body_size_and_stay_inside_the_window() {
    assert_eq!(scaled(630., 14., 1100.), 630.);
    assert_eq!(
        scaled(630., 16., 1100.),
        720.,
        "#310: a larger body grows the card"
    );
    assert_eq!(
        scaled(640., 14., 500.),
        476.,
        "a narrow window holds the card 12 inside each edge"
    );
}

#[test]
fn german_dialogs_take_omarchys_cancel_and_the_decimal_comma() {
    let de = language("language-de");
    assert_eq!(buttons(&open_with().card(&de)), "Abbrechen|Open*");
    let card = RunCard::open(
        &de,
        "flea",
        "/fx/v/work/hello.sh",
        &ASK.replace("\"size\":21", "\"size\":22100"),
    )
    .unwrap();
    assert!(
        matches!(&card.card(&de).body[0], Block::Program { text, .. } if text == "Shell script \u{b7} 22,1 kB")
    );
}

#[test]
fn the_edit_lane_writes_a_private_file_reads_it_back_and_removes_its_dir() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = std::env::temp_dir().join(format!("flea-op3-lane-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".flea-test-root"), b"").unwrap();
    let base = root.to_str().unwrap().to_string();
    let names = vec!["a.txt".to_string(), "b.txt".to_string()];
    let mut seen = String::new();
    let text = super::lane::edit_names_with(&base, &names, |file| {
        let mode = std::fs::metadata(file).unwrap().permissions().mode() & 0o777;
        let dir_mode = std::fs::metadata(std::path::Path::new(file).parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        seen = format!(
            "{mode:o} {dir_mode:o} {}",
            std::fs::read_to_string(file).unwrap().replace('\n', "/")
        );
        std::fs::write(file, "c.txt\nd.txt\n").is_ok()
    });
    assert_eq!(seen, "600 700 a.txt/b.txt/");
    assert_eq!(text.as_deref(), Ok("c.txt\nd.txt\n"));
    assert_eq!(
        super::lane::edit_names_with(&base, &names, |_| false),
        Err("dialogs.file.bulk.err_editor")
    );
    assert_eq!(
        super::lane::edit_names(&base, "/fx", None, &names),
        Err("dialogs.file.bulk.err_editor"),
        "no editor named"
    );
    let left: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, vec![".flea-test-root"], "every scratch dir is gone");
    assert!(root.is_absolute() && root.join(".flea-test-root").exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_lane_reads_an_exit_and_a_launch_that_never_started() {
    assert_eq!(
        super::lane::run(&["/bin/sh".into(), "-c".into(), "printf ask; exit 4".into()]),
        (Some(4), "ask".into())
    );
    assert_eq!(
        super::lane::run(&["/nonexistent/flea".into()]),
        (None, String::new())
    );
    assert_eq!(super::lane::run(&[]), (None, String::new()));
}

#[test]
fn a_held_enter_or_space_never_presses_again_while_held_arrows_repeat() {
    use super::keys::key_of;
    let event = |key: &str, is_held: bool| gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse(key).unwrap(),
        is_held,
        prefer_character_input: false,
    };
    assert_eq!(key_of(&event("enter", false)), Some(Key::Enter));
    assert_eq!(
        key_of(&event("enter", true)),
        None,
        "a held Enter runs nothing"
    );
    assert_eq!(key_of(&event("space", true)), None);
    assert_eq!(key_of(&event("down", true)), Some(Key::Down));
    assert_eq!(key_of(&event("shift-tab", false)), Some(Key::Backtab));
}

#[test]
fn only_open_withs_title_rule_runs_edge_to_edge() {
    let i18n = english();
    assert!(open_with().card(&i18n).bleed);
    assert!(!permissions().card(&i18n).bleed);
    assert!(!BulkRename::open(vec!["/fx/b/a".into()]).card(&i18n).bleed);
}
