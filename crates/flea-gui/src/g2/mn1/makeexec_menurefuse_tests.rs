//! EY-164, EY-195, EY-222, EY-224: Copy As, Add to Shelf, Paste As and Tags leaves reach their owners, never "no longer available".
use super::fixture::{number, update, Fixture};
use crate::g2::mn1::join_tests::read;
use flea::jsondoc::{self, Json};
use flea::protocol::{Reply, RowFacts};
use gpui::TestAppContext;

// Fires one row or leaf on the open menu and answers its validate with the given paths; returns the requests sent after.
fn fire(
    cx: &mut TestAppContext,
    fixture: &mut Fixture,
    action: &str,
    leaf: &str,
    paths: &[String],
) -> Vec<Json> {
    update(cx, fixture.handle, |shell, window, cx| {
        shell.menu_outcome(
            crate::g2::mn1::menu::Outcome::Fire {
                action: action.into(),
                leaf: leaf.into(),
            },
            window,
            cx,
        )
    });
    let requests = fixture.drain();
    let Some(validate) = requests
        .iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
    else {
        let said = said(cx, fixture);
        panic!("{action}:{leaf} never validated; said {said:?}, sent {requests:?}")
    };
    let chosen = if leaf.is_empty() {
        action.to_owned()
    } else {
        format!("{action}:{leaf}")
    };
    let listed = paths
        .iter()
        .map(|path| format!("\"{}\"", flea::json::escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{},\"op\":\"validate\",\"ok\":true,\"action\":\"{chosen}\",\"paths\":[{listed}]}}", number(validate)));
    cx.run_until_parked();
    fixture.drain()
}

fn said(cx: &mut TestAppContext, fixture: &Fixture) -> String {
    read(cx, fixture.handle, "lastMessage")
        .stdout
        .trim_end()
        .to_owned()
}

fn settings(cx: &mut TestAppContext, fixture: &Fixture, data: &str) {
    update(cx, fixture.handle, |shell, _, _| {
        shell.launch.settings.data = jsondoc::parse(data).unwrap();
    });
}

fn keys(cx: &mut TestAppContext, fixture: &Fixture, keys: &str) {
    gpui::VisualTestContext::from_window(fixture.handle.into(), cx).simulate_keystrokes(keys);
    cx.run_until_parked();
}

// One typed character as the platform delivers it: the lowercase key, the character, and shift for a capital.
fn typed(cx: &mut TestAppContext, fixture: &Fixture, text: &str) {
    let event = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            key: text.to_lowercase(),
            key_char: Some(text.to_owned()),
            modifiers: gpui::Modifiers {
                shift: text.chars().any(char::is_uppercase),
                ..gpui::Modifiers::default()
            },
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    };
    gpui::VisualTestContext::from_window(fixture.handle.into(), cx).simulate_event(event);
    cx.run_until_parked();
}

// Answers the open menu's snapshot and applications reads, as the backend does.
fn answer_reads(cx: &mut TestAppContext, fixture: &mut Fixture) {
    let requests = fixture.drain();
    let snapshot = requests
        .iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("snapshot"))
        .expect("the menu reads its snapshot");
    let id = number(snapshot);
    fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    if fixture
        .drain()
        .iter()
        .any(|r| r.get("op").and_then(Json::as_str) == Some("applications"))
    {
        fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[]}}"));
    }
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
}

fn validate_reply(cx: &mut TestAppContext, fixture: &mut Fixture, chosen: &str, paths: &[&str]) {
    let requests = fixture.drain();
    let Some(validate) = requests
        .iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
    else {
        let said = said(cx, fixture);
        panic!("{chosen} never validated; said {said:?}, sent {requests:?}")
    };
    let listed = paths
        .iter()
        .map(|path| format!("\"{}\"", flea::json::escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{},\"op\":\"validate\",\"ok\":true,\"action\":\"{chosen}\",\"paths\":[{listed}]}}", number(validate)));
}

// EY-164: with the shipped hidden set (Copy As hidden), each Copy As leaf chosen on notes.txt puts v0.3.12's text on the clipboard.
#[gpui::test]
fn ey164_copy_as_leaves_copy_their_value(cx: &mut TestAppContext) {
    for (leaf, want) in [
        ("copyPath", "{path}"),
        ("copyRelative", "notes.txt"),
        ("copyName", "notes.txt"),
        ("copyStem", "notes"),
        ("copydirpath", "{root}"),
        ("copyUri", "file://{path}"),
        ("copyQuoted", "'{path}'"),
    ] {
        let mut fixture = Fixture::new(cx, 0o644);
        fixture.open(cx, 1, vec![]);
        let root = fixture.root.to_string_lossy().into_owned();
        let path = fixture
            .root
            .join("notes.txt")
            .to_string_lossy()
            .into_owned();
        let want = want.replace("{path}", &path).replace("{root}", &root);
        fire(
            cx,
            &mut fixture,
            "copyAs",
            leaf,
            std::slice::from_ref(&path),
        );
        let words = said(cx, &fixture);
        assert!(
            !words.contains("no longer available"),
            "{leaf} refused: {words:?}"
        );
        let text = cx.read_from_clipboard().and_then(|item| item.text());
        assert_eq!(
            text.as_deref(),
            Some(want.as_str()),
            "{leaf} copied the wrong text"
        );
        assert!(update(cx, fixture.handle, |shell, _, _| shell
            .menu
            .captured
            .is_none()));
    }
}

// EY-164 as the sweep drives it: c opens the lone Copy As flyout on the cursor row, down past Relative Path and Return choose Name.
#[gpui::test]
fn ey164_copy_as_name_with_return_copies_the_name(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });
    keys(cx, &fixture, "c");
    answer_reads(cx, &mut fixture);
    keys(cx, &fixture, "down down enter");
    let path = fixture
        .root
        .join("notes.txt")
        .to_string_lossy()
        .into_owned();
    validate_reply(cx, &mut fixture, "copyAs:copyName", &[&path]);
    let words = said(cx, &fixture);
    assert!(!words.contains("no longer available"), "refused: {words:?}");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("notes.txt")
    );
}

// EY-222: a file on the clipboard, the folder empty, and the shipped hidden set: P opens the lone Paste As flyout and L, a and H send the link.
#[gpui::test]
fn ey222_paste_as_letters_send_the_link(cx: &mut TestAppContext) {
    for letter in ["L", "a", "H"] {
        let mut fixture = Fixture::empty(cx);
        let source = flea::json::escape(&fixture.root.join("../notes.txt").to_string_lossy());
        fixture.receive(cx, &format!(r#"{{"t":"clip","op":"changed","clip":"copy","paths":["{source}"],"token":"menurefuse","skipped":0}}"#));
        fixture.drain();
        typed(cx, &fixture, "P");
        let flyout = update(cx, fixture.handle, |shell, _, _| {
            shell
                .menu
                .menu
                .borrow()
                .flyout
                .as_ref()
                .map(|f| f.action.clone())
        });
        assert_eq!(
            flyout.as_deref(),
            Some("pasteAs"),
            "P opens the Paste As flyout; said {:?}",
            said(cx, &fixture)
        );
        fixture.drain();
        typed(cx, &fixture, letter);
        let words = said(cx, &fixture);
        let sent = fixture.drain();
        assert!(
            !words.contains("no longer available"),
            "{letter} refused: {words:?}; sent {sent:?}"
        );
        let dest = fixture.root.to_string_lossy().into_owned();
        assert!(
            sent.iter()
                .any(|r| r.get("c").and_then(Json::as_str) == Some("collisions")
                    && r.get("dest").and_then(Json::as_str) == Some(dest.as_str())),
            "{letter} sent no link request into the folder: {sent:?}; said {words:?}"
        );
    }
}

// EY-195: Add to Shelf on a file, a folder, in Columns and from the cursor row asks the shelf plugin once over the item and says "Added it to the shelf".
fn shelf_plugin() -> flea::ui_logic_st2::wire::Plugin {
    let (manifest, _) = flea::ui_logic_st2::builtin::manifests()
        .into_iter()
        .find(|(m, _)| m.id == flea::ui_logic_st2::builtin::SHELF)
        .unwrap();
    flea::ui_logic_st2::wire::Plugin {
        manifest,
        enabled: true,
        source: flea::ui_logic_st2::wire::Source::Builtin,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    }
}

#[gpui::test]
fn ey195_add_to_shelf_adds_the_item(cx: &mut TestAppContext) {
    for case in ["file", "folder", "columns", "cursor"] {
        let mut fixture = Fixture::new(cx, 0o644);
        settings(cx, &fixture, r#"{"shelf":{"enabled":true}}"#);
        let name = if case == "folder" {
            "docs"
        } else {
            "notes.txt"
        };
        let (owner, mut owner_lines) = crate::backend::Backend::test_lines();
        let answers = owner.test_output();
        update(cx, fixture.handle, |shell, window, cx| {
            // The row shows only while the owner lists an enabled shelf plugin.
            shell.follow_plugins(window, cx);
            let host = shell.plugins.host.clone().unwrap();
            host.update(cx, |host, cx| {
                host.test_attach(owner, cx);
                host.plugins = vec![shelf_plugin()];
            });
            if case == "columns" {
                shell.view = crate::window::startup::View::Columns;
            }
            if case == "folder" {
                std::fs::create_dir(fixture_root(shell).join("docs")).unwrap();
                let host = shell.panes[0].as_mut().unwrap();
                host.pane
                    .receive(Reply::Rows {
                        start: 1,
                        rows: vec![RowFacts {
                            name: "docs".into(),
                            mode: 0o040755,
                            ..RowFacts::default()
                        }],
                        listing: 2,
                    })
                    .unwrap();
            }
        });
        fixture.open(cx, 1, vec![]);
        let path = fixture.root.join(name).to_string_lossy().into_owned();
        // The cursor case validates no paths, so the cursor row under the listing's base is the target.
        let validated = if case == "cursor" {
            Vec::new()
        } else {
            vec![path.clone()]
        };
        fire(cx, &mut fixture, "addToShelf", "", &validated);
        let asked: Vec<Json> = std::iter::from_fn(&mut owner_lines)
            .map(|line| jsondoc::parse(&line).unwrap())
            .filter(|line| line.get("op").and_then(Json::as_str) == Some("runtime"))
            .collect();
        assert_eq!(asked.len(), 1, "{case} asked the owner once: {asked:?}");
        let request = asked[0].get("request").unwrap();
        assert_eq!(
            asked[0].get("plugin").and_then(Json::as_str),
            Some(flea::ui_logic_st2::builtin::SHELF)
        );
        assert_eq!(request.get("action").and_then(Json::as_str), Some("add"));
        assert_eq!(
            request.get("paths"),
            Some(&Json::Arr(vec![Json::Str(path)])),
            "{case} shelved the item"
        );
        answers
            .send_blocking(crate::backend::Backend::test_reply_line(&format!(
                r#"{{"t":"pluginruntime","id":{},"plugin":"flea.shelf","ok":true,"message":{{"op":"shelf"}},"progress":[],"code":"","error":""}}"#,
                number(&asked[0])
            )))
            .unwrap();
        cx.run_until_parked();
        let words = said(cx, &fixture);
        assert!(
            !words.contains("no longer available"),
            "{case} refused: {words:?}"
        );
        assert_eq!(words, "Added it to the shelf", "{case}");
    }
}

fn fixture_root(shell: &crate::state::Shell) -> std::path::PathBuf {
    std::path::PathBuf::from(&shell.panes[0].as_ref().unwrap().path)
}

// EY-224: Red chosen in the Tags flyout sends one settags toggle over the row and words the answer; it never refuses.
#[path = "makeexec_tags_tests.rs"]
mod extracted;
