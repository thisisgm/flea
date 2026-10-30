//! Properties chosen on the menu row: the card's own listing snapshot goes alone, then its facts.
use super::menudeliver_tests::{open, open_menu, select, wire, REG};
use super::menus_tests::{choose, home, validated};
use super::test_shell::land;
use gpui::TestAppContext;

#[gpui::test]
fn properties_from_the_menu_row_snapshots_its_listing_then_shows_the_facts(cx: &mut TestAppContext) {
    // EY-201: the card's snapshot names the listing it was read from and goes alone, as the backend's one menu slot needs.
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG]);
    choose(&shell, cx, "properties");
    // The menu's own snapshot carries the same rows, so only what the validated choice sends is the card's.
    sent();
    validated(&shell, cx, "properties", &["/files/notes.txt"]);
    let lines = sent();
    let snapshot = lines
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .unwrap_or_else(|| panic!("the card snapshots notes.txt: {lines:?}"));
    let id = flea::json::field_usize(snapshot, "id").unwrap();
    assert!(
        snapshot.contains(r#""listing":1,"rows":[2],"cursor":2"#),
        "the card's snapshot names its listing: {snapshot}"
    );
    assert!(
        !lines.iter().any(|line| line.contains(r#""op":"properties""#)),
        "Properties waits for the snapshot's answer: {lines:?}"
    );
    super::menus_tests::reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"properties","id":{id}}}"#)]
    );
    super::menus_tests::reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"properties","ok":true,"path":"/files/notes.txt","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000}}"#),
    );
    assert!(
        shell.read_with(cx, |shell, _| shell
            .properties
            .as_ref()
            .is_some_and(|open| open.ready())),
        "the card shows the facts"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            (host.pane.cursor, host.pane.held.row(2).map(|row| row.name.to_string()))
        }),
        (2, Some("notes.txt".into())),
        "the listing stays behind the card"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
