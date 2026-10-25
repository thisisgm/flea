//! Move to Trash, Duplicate, Delete and New Folder chosen in the production Shell's context menu reach the backend.
use super::{
    fileops_tests::{reply, wire},
    test_shell::{land, row, set_cursor, status},
};
use crate::{
    backend::Backend,
    navigation::folder_state::Writer,
    state::Shell,
    window::{
        locale::Snapshot,
        root::Root,
        startup::{Arguments, Launch, Settings},
        u07_tests::read,
    },
};
use flea::jsondoc::Json;
use gpui::{
    point, px, size, AppContext, Entity, Modifiers, MouseButton, TestAppContext,
    VisualTestContext,
};
use std::sync::Arc;

// The production Shell under the window Root, painted and read, with Delete shown and Run Script hidden so the menu asks only for its snapshot and applications.
fn open(cx: &mut TestAppContext) -> (Entity<Shell>, VisualTestContext) {
    let document = flea::jsondoc::parse(
        r#"{"keys":"default","view":"list","places":{"rail":"shown","showHome":true,"showTrash":true},"menu":{"hidden":["runScript"]}}"#,
    )
    .unwrap();
    let launch = Launch::resolve(
        Settings::from_json(document, true),
        Arguments {
            path: Some("/files".into()),
            select: None,
        },
        &std::env::var("HOME").unwrap_or_else(|_| "/".into()),
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(flea::i18n::I18n::default()),
    };
    let (_sender, events) = async_channel::unbounded();
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| Shell::new(launch, locale, events, window, cx));
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        Root { shell }
    });
    let shell = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let mut cx = VisualTestContext::from_window(handle.into(), cx);
    // The folder memory writer only counts here, so no step reaches the real state file.
    shell.update(&mut cx, |shell, _| {
        *shell.panes[0].as_ref().unwrap().nav.writer.borrow_mut() = Writer::default();
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    (shell, cx)
}

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

fn id_of(line: &str) -> usize {
    flea::json::field_usize(line, "id").unwrap()
}

// The row menu on the cursor row, answered as the backend answers a regular file.
fn row_menu(shell: &Entity<Shell>, cx: &mut VisualTestContext, sent: &mut impl FnMut() -> Vec<String>) -> usize {
    cx.simulate_keystrokes("shift-f10");
    let asked = sent();
    let snapshot = asked
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .expect("the menu takes a snapshot");
    let id = id_of(snapshot);
    reply(shell, cx, &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#));
    for line in sent() {
        if line.contains(r#""op":"applications""#) {
            reply(shell, cx, &format!(r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#));
        }
    }
    id
}

// Walks the open menu's cursor to the row with this action and presses Return on it.
fn choose(shell: &Entity<Shell>, cx: &mut VisualTestContext, action: &str) {
    frame(cx);
    let model = flea::jsondoc::parse(&read(shell, cx, "contextMenuModel", &[])).unwrap();
    let rows = model.as_array().unwrap();
    let target = rows
        .iter()
        .position(|row| row.get("action").and_then(Json::as_str) == Some(action))
        .unwrap_or_else(|| panic!("the menu offers {action}: {model:?}"));
    for _ in 0..rows.len() {
        let cursor: usize = read(shell, cx, "contextMenuCursor", &[]).parse().unwrap();
        if cursor == target {
            break;
        }
        cx.simulate_keystrokes(if cursor < target { "down" } else { "up" });
        frame(cx);
    }
    cx.simulate_keystrokes("enter");
}

// The backend's validate answer for the chosen row, which delivers the choice to its family.
fn validated(shell: &Entity<Shell>, cx: &mut VisualTestContext, sent: &mut impl FnMut() -> Vec<String>, id: usize, action: &str) {
    let asked = sent();
    assert!(
        asked.iter().any(|line| line.contains(r#""op":"validate""#) && id_of(line) == id),
        "{asked:?}"
    );
    reply(shell, cx, &format!(r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"{action}","paths":["/files/notes.txt"],"dest":""}}"#));
}

fn opened(cx: &mut TestAppContext) -> (Entity<Shell>, VisualTestContext) {
    let (shell, mut cx) = open(cx);
    land(&shell, &mut cx, "/files", home(), 1);
    set_cursor(&shell, &mut cx, 2);
    (shell, cx)
}

#[gpui::test]
fn menu_move_to_trash_and_duplicate_run_on_the_captured_snapshot(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = wire(&shell, cx);
    let unavailable = shell.read_with(cx, |shell, _| shell.locale.catalog.tr("menu.refusal.unavailable"));
    let id = row_menu(&shell, cx, &mut sent);
    choose(&shell, cx, "trash");
    validated(&shell, cx, &mut sent, id, "trash");
    assert_eq!(sent(), [format!(r#"{{"c":"trash","rows":[2],"menuId":{id},"listing":1}}"#)]);
    assert_ne!(status(&shell, cx), unavailable, "the row is no longer refused");
    let id = row_menu(&shell, cx, &mut sent);
    choose(&shell, cx, "duplicate");
    validated(&shell, cx, &mut sent, id, "duplicate");
    assert_eq!(sent(), [format!(r#"{{"c":"duplicate","path":"/files/notes.txt","menuId":{id}}}"#)]);
    assert_ne!(status(&shell, cx), unavailable);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn menu_delete_reviews_the_menu_snapshot_and_opens_the_permanent_card(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = wire(&shell, cx);
    let id = row_menu(&shell, cx, &mut sent);
    choose(&shell, cx, "deletePermanently");
    validated(&shell, cx, &mut sent, id, "deletePermanently");
    assert_eq!(sent(), [format!(r#"{{"c":"menuaction","op":"prepareDelete","id":{id}}}"#)]);
    reply(&shell, cx, &format!(r#"{{"t":"menuaction","id":{id},"op":"prepareDelete","ok":true,"token":9,"count":1,"bytes":12}}"#));
    assert!(shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn background_new_folder_makes_a_folder_in_the_menu_folder(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = wire(&shell, cx);
    // Below the last row is the folder's empty space, whose menu is the background menu.
    let centre = read(&shell, cx, "rowCentre", &["2"]);
    let mut values = centre.split_whitespace().map(|value| value.parse::<f32>().unwrap());
    let (x, y) = (values.next().unwrap(), values.next().unwrap());
    cx.simulate_mouse_down(point(px(x), px(y + 200.)), MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    choose(&shell, cx, "newFolder");
    assert_eq!(sent(), [r#"{"c":"mkdir","path":"/files"}"#]);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
