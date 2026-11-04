//! Open With over several selected files: lit with the applications every type shares, dimmed when none does, and one launch on the whole snapshot.
use super::menudeliver_tests::{open, select, wire, REG};
use super::menus_tests::{choose, deliver, home, reply, validated};
use super::test_shell::{land, status};
use crate::{state::Shell, window::u07_tests::read};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

const SHARED: &str = r#"[{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true},{"id":"helix.desktop","label":"Helix","icon":"helix","default":false}]"#;

/// Opens the row menu over notes.txt and todo.md, answers its snapshot and these shared applications; returns the wire, the menu id and the lines the menu sent.
fn menu_over_two(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    applications: &str,
) -> (impl FnMut() -> Vec<String>, u64, Vec<String>) {
    land(shell, cx, "/files", home(), 1);
    select(shell, cx, &[2, 3]);
    let mut sent = wire(shell, cx);
    cx.simulate_keystrokes("shift-f10");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG},{REG}]}}"#
        ),
    );
    let asked = sent();
    reply(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":{applications}}}"#
        ),
    );
    (sent, id, asked)
}

fn open_with_row(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    let model = jsondoc::parse(&read(shell, cx, "contextMenuModel", &[])).unwrap();
    model
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("action").and_then(Json::as_str) == Some("openWith"))
        .cloned()
        .expect("the several-item menu keeps Open With, as actions-menu.png draws it")
}

#[gpui::test]
fn several_files_light_open_with_with_the_applications_they_share(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let (_, id, asked) = menu_over_two(&shell, cx, SHARED);
    assert!(
        asked.contains(&format!(
            r#"{{"c":"menuaction","op":"applications","id":{id}}}"#
        )),
        "the menu asks the snapshot's shared applications: {asked:?}"
    );
    let row = open_with_row(&shell, cx);
    assert_eq!(row.get("disabled").and_then(Json::as_bool), Some(false));
    let leaves: Vec<(String, String)> = row
        .get("submenu")
        .and_then(Json::as_array)
        .expect("the row opens its flyout")
        .iter()
        .map(|leaf| {
            let text = |key| {
                leaf.get(key)
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            (text("id"), text("hint"))
        })
        .collect();
    // The default first with its caption, a separator, the rest; the one-file card's tail is not offered.
    assert_eq!(
        leaves,
        [
            ("nvim.desktop".into(), "default".into()),
            (String::new(), String::new()),
            ("helix.desktop".into(), String::new()),
        ]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn several_files_no_application_shares_dim_open_with(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let _ = menu_over_two(&shell, cx, "[]");
    let row = open_with_row(&shell, cx);
    assert_eq!(row.get("disabled").and_then(Json::as_bool), Some(true));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_shared_application_launches_once_on_the_whole_snapshot(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let (mut sent, id, _) = menu_over_two(&shell, cx, SHARED);
    choose(&shell, cx, "openWith");
    cx.simulate_keystrokes("enter");
    super::menus_tests::settle(cx);
    sent();
    validated(
        &shell,
        cx,
        "openWith:nvim.desktop",
        &["/files/notes.txt", "/files/todo.md"],
    );
    let lines = sent();
    assert_eq!(
        lines,
        [format!(
            r#"{{"c":"menuaction","op":"openWith","id":{id},"application":"nvim.desktop"}}"#
        )],
        "one launch names the menu's snapshot of both files"
    );
    assert_ne!(
        status(&shell, cx),
        "That action is no longer available; reopen the menu."
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
