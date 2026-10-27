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
        fire(cx, &mut fixture, "copyAs", leaf, &[path.clone()]);
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

// EY-164 as the sweep drives it: c opens the lone Copy As flyout on the cursor row, down and Return choose Name.
#[gpui::test]
fn ey164_copy_as_name_with_return_copies_the_name(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });
    keys(cx, &fixture, "c");
    answer_reads(cx, &mut fixture);
    keys(cx, &fixture, "down enter");
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

static SHELVED: std::sync::Mutex<Vec<Vec<String>>> = std::sync::Mutex::new(Vec::new());

fn record_shelf(paths: &[String]) -> Result<(), String> {
    SHELVED.lock().unwrap().push(paths.to_vec());
    Ok(())
}

// EY-195: Add to Shelf on a file, a folder and in Columns runs one `flea shelf add` over the item and says "Added it to the shelf".
#[gpui::test]
fn ey195_add_to_shelf_adds_the_item(cx: &mut TestAppContext) {
    for case in ["file", "folder", "columns"] {
        SHELVED.lock().unwrap().clear();
        let mut fixture = Fixture::new(cx, 0o644);
        settings(cx, &fixture, r#"{"shelf":{"enabled":true}}"#);
        let name = if case == "folder" {
            "docs"
        } else {
            "notes.txt"
        };
        update(cx, fixture.handle, |shell, _, _| {
            shell.menu.shelve = Some(record_shelf);
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
        fire(cx, &mut fixture, "addToShelf", "", &[path.clone()]);
        let words = said(cx, &fixture);
        assert!(
            !words.contains("no longer available"),
            "{case} refused: {words:?}"
        );
        assert_eq!(
            *SHELVED.lock().unwrap(),
            vec![vec![path]],
            "{case} shelved the item"
        );
        assert_eq!(words, "Added it to the shelf", "{case}");
    }
}

fn fixture_root(shell: &crate::state::Shell) -> std::path::PathBuf {
    std::path::PathBuf::from(&shell.panes[0].as_ref().unwrap().path)
}

// EY-224: Red chosen in the Tags flyout sends one settags toggle over the row and words the answer; it never refuses.
#[gpui::test]
fn ey224_tag_leaf_tags_the_row(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    fixture.open(cx, 1, vec![]);
    let path = fixture
        .root
        .join("notes.txt")
        .to_string_lossy()
        .into_owned();
    let sent = fire(cx, &mut fixture, "tags", "tag1", &[path]);
    let words = said(cx, &fixture);
    assert!(!words.contains("no longer available"), "refused: {words:?}");
    let settags = sent
        .iter()
        .find(|r| r.get("c").and_then(Json::as_str) == Some("settags"))
        .unwrap_or_else(|| panic!("no settags sent: {sent:?}"));
    assert_eq!(settags.get("op").and_then(Json::as_str), Some("toggle"));
    assert_eq!(settags.get("tag").and_then(Json::as_str), Some("Red"));
    let rows: Vec<f64> = settags
        .get("rows")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(Json::as_f64)
        .collect();
    assert_eq!(rows, vec![1.]);
    fixture.receive(cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    assert_eq!(said(cx, &fixture), "Tagged 1 item Red · z undoes");
    assert!(
        fixture
            .drain()
            .iter()
            .any(|r| r.get("c").and_then(Json::as_str) == Some("tagstates")),
        "the answer re-reads the tags so the open flyout marks what is in effect"
    );
}

// A settagged answer with no menu toggle waiting is not the menu's, so the menu passes it on instead of swallowing it.
#[gpui::test]
fn ey224_stray_settagged_is_not_claimed_by_the_menu(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    fixture.open(cx, 1, vec![]);
    let reply = jsondoc::parse(r#"{"t":"settagged","ok":1,"op":"add"}"#).unwrap();
    let claimed = update(cx, fixture.handle, |shell, window, cx| {
        shell.menu_tag_line(0, &reply, window, cx)
    });
    assert!(
        !claimed,
        "no menu toggle was waiting, so the reply is someone else's"
    );
    assert!(!said(cx, &fixture).contains("Tagged"));
}

fn menu_open(cx: &mut TestAppContext, fixture: &Fixture) -> bool {
    read(cx, fixture.handle, "contextMenuVisible").stdout == "true\n"
}

// The listing number the pane holds, which a tagstates answer names as the numbering its rows resolved in.
fn held_listing(cx: &mut TestAppContext, fixture: &Fixture) -> usize {
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.held.listing
    })
}

fn flyout_open(cx: &mut TestAppContext, fixture: &Fixture) -> bool {
    update(cx, fixture.handle, |shell, _, _| {
        shell.menu.menu.borrow().flyout.is_some()
    })
}

// The check on the Red row of the open Tags flyout, None while it carries no mark.
fn red_mark(cx: &mut TestAppContext, fixture: &Fixture) -> Option<bool> {
    update(cx, fixture.handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        menu.flyout
            .as_ref()?
            .card
            .entries
            .iter()
            .find(|e| e.id == "tag1")?
            .check
    })
}

// Menu right on the Tags row opens its flyout, as the keyboard does.
fn open_tags_flyout(cx: &mut TestAppContext, fixture: &Fixture) {
    update(cx, fixture.handle, |shell, _, _| {
        let mut menu = shell.menu.menu.borrow_mut();
        let card = menu.card.as_mut().expect("the file menu is open");
        card.cursor = card.entries.iter().position(|e| e.action == "tags");
    });
    keys(cx, fixture, "right");
    assert!(
        flyout_open(cx, fixture),
        "right did not open the Tags flyout"
    );
}

fn settags_sent(requests: &[Json]) -> &Json {
    requests
        .iter()
        .find(|r| r.get("c").and_then(Json::as_str) == Some("settags"))
        .unwrap_or_else(|| panic!("no settags sent: {requests:?}"))
}

fn tagstates_asked(requests: &[Json]) -> u64 {
    let ask = requests
        .iter()
        .find(|r| r.get("c").and_then(Json::as_str) == Some("tagstates"))
        .unwrap_or_else(|| panic!("no tagstates read sent: {requests:?}"));
    number(ask)
}

// EY-224: a Tags choice keeps the menu and its flyout open, Red takes its mark once the read lands, and Escape still closes both.
#[gpui::test]
fn ey224_tags_choice_keeps_the_flyout_and_marks_the_tag(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    fixture.open(cx, 1, vec![]);
    // The menu's first show swap ends on the clock, as in a live window; a mark drawn before then waits for it.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    let path = fixture
        .root
        .join("notes.txt")
        .to_string_lossy()
        .into_owned();
    open_tags_flyout(cx, &fixture);
    keys(cx, &fixture, "1");
    validate_reply(cx, &mut fixture, "tags:tag1", &[path.as_str()]);
    cx.run_until_parked();
    let sent = fixture.drain();
    let settags = settags_sent(&sent);
    assert_eq!(settags.get("op").and_then(Json::as_str), Some("toggle"));
    assert_eq!(settags.get("tag").and_then(Json::as_str), Some("Red"));
    assert!(
        menu_open(cx, &fixture) && flyout_open(cx, &fixture),
        "the Red choice closed the menu or its Tags flyout"
    );
    fixture.receive(cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    let asked = tagstates_asked(&fixture.drain());
    let listing = held_listing(cx, &fixture);
    fixture.receive(
        cx,
        &format!(
            r#"{{"t":"tagstates","id":{asked},"applied":["Red"],"mixed":[],"listing":{listing}}}"#
        ),
    );
    assert_eq!(
        red_mark(cx, &fixture),
        Some(true),
        "Red has no mark once its read lands"
    );
    assert!(
        menu_open(cx, &fixture) && flyout_open(cx, &fixture),
        "the answer closed the menu or its flyout"
    );
    keys(cx, &fixture, "1");
    validate_reply(cx, &mut fixture, "tags:tag1", &[path.as_str()]);
    cx.run_until_parked();
    let sent = fixture.drain();
    let settags = settags_sent(&sent);
    assert_eq!(settags.get("op").and_then(Json::as_str), Some("toggle"));
    fixture.receive(cx, r#"{"t":"settagged","ok":1,"op":"remove"}"#);
    let asked = tagstates_asked(&fixture.drain());
    let listing = held_listing(cx, &fixture);
    fixture.receive(
        cx,
        &format!(r#"{{"t":"tagstates","id":{asked},"applied":[],"mixed":[],"listing":{listing}}}"#),
    );
    assert_eq!(
        red_mark(cx, &fixture),
        None,
        "Red keeps its mark after removal"
    );
    assert!(
        menu_open(cx, &fixture) && flyout_open(cx, &fixture),
        "the removal closed the menu or its flyout"
    );
    keys(cx, &fixture, "escape");
    assert!(
        menu_open(cx, &fixture) && !flyout_open(cx, &fixture),
        "Escape must close the flyout and keep the menu"
    );
    keys(cx, &fixture, "escape");
    assert!(!menu_open(cx, &fixture), "Escape must close the menu");
}
