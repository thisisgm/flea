//! FIX-FLYLEAF, EY-241: a flyout leaf key pressed after its flyout opened by key runs the leaf, never "no longer available".
use super::fixture::{number, update, Fixture};
use crate::{backend::Event, g2::context_menus::join_tests::read, window::navigate::Step};
use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use gpui::{TestAppContext, VisualTestContext};

fn keys(cx: &mut TestAppContext, fixture: &Fixture, keys: &str) {
    VisualTestContext::from_window(fixture.handle.into(), cx).simulate_keystrokes(keys);
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
    VisualTestContext::from_window(fixture.handle.into(), cx).simulate_event(event);
    cx.run_until_parked();
}

fn said(cx: &mut TestAppContext, fixture: &Fixture) -> String {
    read(cx, fixture.handle, "lastMessage")
        .stdout
        .trim_end()
        .to_owned()
}

// Answers the open row menu's snapshot and applications reads, as the backend does.
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

// Returns the validate request the menu sent for the chosen leaf; panics with what was said when none went out.
fn validate_of(cx: &mut TestAppContext, fixture: &mut Fixture, chosen: &str) -> Json {
    let requests = fixture.drain();
    let Some(validate) = requests
        .iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
    else {
        let said = said(cx, fixture);
        let state = state(cx, fixture);
        panic!("{chosen} never validated; said {said:?}, state {state}, sent {requests:?}")
    };
    assert_eq!(
        validate.get("action").and_then(Json::as_str),
        Some(chosen),
        "validate names the chosen leaf"
    );
    validate.clone()
}

fn answer_validate(
    cx: &mut TestAppContext,
    fixture: &mut Fixture,
    validate: &Json,
    chosen: &str,
    paths: &[&str],
) {
    let listed = paths
        .iter()
        .map(|path| format!("\"{}\"", flea::json::escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{},\"op\":\"validate\",\"ok\":true,\"action\":\"{chosen}\",\"paths\":[{listed}]}}", number(validate)));
}

// The menu state the keys left behind, quoted in a failure message.
fn state(cx: &mut TestAppContext, fixture: &Fixture) -> String {
    update(cx, fixture.handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        format!(
            "opened {} at {:?} kctx {:?} rows {:?} flyout {:?} pending {:?} activating {} reading {} ready {:?} captured {}",
            menu.opened(),
            shell.menu.presentation.at.is_some(),
            shell.keyboard.context,
            menu.card.as_ref().map(|c| c.entries.iter().map(|e| e.action.to_string()).collect::<Vec<_>>()),
            menu.flyout.as_ref().map(|f| (f.action.clone(), f.parent, f.card.cursor)),
            shell.menu.pending,
            shell.menu.activating,
            shell.menu.reading,
            shell.menu.captured.as_ref().map(|c| c.ready),
            shell.menu.captured.is_some(),
        )
    })
}

// c opens the lone Copy As flyout over the cursor row, then the Copy As path leaf p runs and copies the path.
#[gpui::test]
fn ey241_copy_as_path_leaf_after_c_copies_the_path(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });
    keys(cx, &fixture, "c");
    answer_reads(cx, &mut fixture);
    let opened = state(cx, &fixture);
    keys(cx, &fixture, "p");
    let words = said(cx, &fixture);
    assert!(
        !words.contains("no longer available"),
        "p refused after c: {words:?}; before p {opened}"
    );
    let path = fixture
        .root
        .join("notes.txt")
        .to_string_lossy()
        .into_owned();
    let validate = validate_of(cx, &mut fixture, "copyAs:copyPath");
    answer_validate(cx, &mut fixture, &validate, "copyAs:copyPath", &[&path]);
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(path.as_str())
    );
    // The sweep's copyas-query-path: after the path leaf, f opens search.
    keys(cx, &fixture, "f");
    let searching = update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().search.owner.active()
    });
    assert!(searching, "f after the Copy As path leaf opens search");
}

// P opens the lone Paste As flyout over an empty folder, then each Paste As leaf key sends its link into the folder.
#[gpui::test]
fn ey241_paste_as_leaves_over_an_empty_folder_run(cx: &mut TestAppContext) {
    for letter in ["L", "a", "H"] {
        let mut fixture = Fixture::empty(cx);
        let source = flea::json::escape(&fixture.root.join("../notes.txt").to_string_lossy());
        fixture.receive(cx, &format!(r#"{{"t":"clip","op":"changed","clip":"copy","paths":["{source}"],"token":"flyleaf","skipped":0}}"#));
        fixture.drain();
        typed(cx, &fixture, "P");
        let opened = state(cx, &fixture);
        typed(cx, &fixture, letter);
        let words = said(cx, &fixture);
        let sent = fixture.drain();
        assert!(
            !words.contains("no longer available"),
            "{letter} refused after P: {words:?}; before {opened}; sent {sent:?}"
        );
        let dest = fixture.root.to_string_lossy().into_owned();
        assert!(
            sent.iter()
                .any(|r| r.get("c").and_then(Json::as_str) == Some("collisions")
                    && r.get("dest").and_then(Json::as_str) == Some(dest.as_str())),
            "{letter} sent no link request into the folder: {sent:?}"
        );
    }
}

// P opens the lone Paste As flyout over a file row, then each Paste As leaf key validates and is never refused.
#[gpui::test]
fn ey241_paste_as_leaves_over_a_row_run(cx: &mut TestAppContext) {
    for (letter, chosen) in [
        ("L", "pasteAs:pasteLink"),
        ("a", "pasteAs:pasteAbsoluteLink"),
        ("H", "pasteAs:pasteHardLink"),
    ] {
        let mut fixture = Fixture::new(cx, 0o644);
        let source = fixture
            .root
            .join("notes.txt")
            .to_string_lossy()
            .into_owned();
        fixture.receive(cx, &format!(r#"{{"t":"clip","op":"changed","clip":"copy","paths":["{}"],"token":"flyleaf","skipped":0}}"#, flea::json::escape(&source)));
        fixture.drain();
        update(cx, fixture.handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 1
        });
        typed(cx, &fixture, "P");
        answer_reads(cx, &mut fixture);
        let opened = state(cx, &fixture);
        typed(cx, &fixture, letter);
        let words = said(cx, &fixture);
        assert!(
            !words.contains("no longer available"),
            "{letter} refused after P over a row: {words:?}; before {opened}"
        );
        let validate = validate_of(cx, &mut fixture, chosen);
        answer_validate(cx, &mut fixture, &validate, chosen, &[&source]);
        cx.run_until_parked();
        let words = said(cx, &fixture);
        assert!(
            !words.contains("no longer available"),
            "{letter} refused on the answer: {words:?}"
        );
    }
}

// EY-241 as the sweep drives it: Return enters an empty folder, then P and each Paste As leaf key send the link into it.
#[gpui::test]
fn ey241_paste_as_leaves_after_return_into_an_empty_folder_run(cx: &mut TestAppContext) {
    for letter in ["L", "a", "H"] {
        let mut fixture = Fixture::new(cx, 0o644);
        let source = flea::json::escape(&fixture.root.join("notes.txt").to_string_lossy());
        fixture.receive(cx, &format!(r#"{{"t":"clip","op":"changed","clip":"copy","paths":["{source}"],"token":"flyleaf","skipped":0}}"#));
        fixture.drain();
        let folder = fixture.root.join("Empty");
        std::fs::create_dir_all(&folder).unwrap();
        let folder = folder.to_string_lossy().into_owned();
        update(cx, fixture.handle, |shell, window, cx| {
            assert_eq!(
                shell.step(0, Step::Open(folder.clone()), window, cx),
                Ok(true)
            );
        });
        update(cx, fixture.handle, |shell, window, cx| {
            for reply in [
                Reply::Listed {
                    path: folder.clone(),
                    total: 0,
                },
                Reply::Rows {
                    start: 0,
                    rows: Vec::<RowFacts>::new(),
                    listing: 3,
                },
            ] {
                shell.receive(0, Event::Reply(reply), window, cx);
            }
        });
        cx.run_until_parked();
        fixture.drain();
        typed(cx, &fixture, "P");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        cx.run_until_parked();
        let opened = state(cx, &fixture);
        typed(cx, &fixture, letter);
        let words = said(cx, &fixture);
        let sent = fixture.drain();
        assert!(
            !words.contains("no longer available"),
            "{letter} refused after P in Empty: {words:?}; before {opened}; sent {sent:?}"
        );
        assert!(
            sent.iter()
                .any(|r| r.get("c").and_then(Json::as_str) == Some("collisions")
                    && r.get("dest").and_then(Json::as_str) == Some(folder.as_str())),
            "{letter} sent no link request into Empty: {sent:?}"
        );
    }
}
