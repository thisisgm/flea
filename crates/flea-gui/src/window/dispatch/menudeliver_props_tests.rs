//! Properties chosen on the menu row: the card's own listing snapshot goes alone, then its facts, its kind word and a wrapped path.
use super::menudeliver_tests::{open, open_menu, select, wire, REG};
use super::menus_tests::{choose, home, validated};
use super::test_shell::land;
use crate::state::Shell;
use gpui::{Entity, TestAppContext, VisualTestContext};

#[gpui::test]
fn properties_from_the_menu_row_snapshots_its_listing_then_shows_the_facts(
    cx: &mut TestAppContext,
) {
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
        !lines
            .iter()
            .any(|line| line.contains(r#""op":"properties""#)),
        "Properties waits for the snapshot's answer: {lines:?}"
    );
    super::menus_tests::reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"properties","id":{id}}}"#
        )]
    );
    super::menus_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"properties","ok":true,"path":"/files/notes.txt","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000}}"#
        ),
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
            (
                host.pane.cursor,
                host.pane.held.row(2).map(|row| row.name.to_string()),
            )
        }),
        (2, Some("notes.txt".into())),
        "the listing stays behind the card"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// Opens Properties on notes.txt from the row menu and answers it with this path; the listing row's kind is `kind`.
fn properties_on<'a>(
    cx: &'a mut TestAppContext,
    kind: &str,
    path: &str,
    extra: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open(cx);
    let mut rows = home();
    rows[2].kind = kind.into();
    land(&shell, cx, "/files", rows, 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG]);
    choose(&shell, cx, "properties");
    validated(&shell, cx, "properties", &["/files/notes.txt"]);
    let id = sent()
        .iter()
        .rev()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .and_then(|line| flea::json::field_usize(line, "id"))
        .expect("the card snapshots notes.txt");
    for line in [
        format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
        format!(
            r#"{{"t":"menuaction","id":{id},"op":"properties","ok":true,"path":"{path}","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000{extra}}}"#
        ),
    ] {
        super::menus_tests::reply(&shell, cx, &line);
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (shell, cx)
}

#[gpui::test]
fn properties_kind_is_the_mime_word_the_listing_holds(cx: &mut TestAppContext) {
    // FIX-PROPS-2: Kind prints the MIME database's word, as the preview column does (HIG properties.png).
    let (shell, cx) = properties_on(cx, "Plain text document", "/files/notes.txt", "");
    let catalog = shell.read_with(cx, |shell, _| shell.locale.catalog.clone());
    let kind = shell.read_with(cx, |shell, _| {
        let card = shell.properties.as_ref().unwrap().card().card(&catalog);
        card.body.iter().find_map(|block| match block {
            crate::g2::op3::shell::Block::Facts(facts) => {
                facts.iter().find_map(|fact| match fact {
                    crate::g2::op3::shell::Fact::Text { name, value, .. } if name == "Kind" => {
                        Some(value.clone())
                    }
                    _ => None,
                })
            }
            _ => None,
        })
    });
    assert_eq!(kind.as_deref(), Some("Plain text document"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn properties_wraps_a_long_path_inside_the_card(cx: &mut TestAppContext) {
    // FIX-PROPS-2: a long Path wraps inside the value column, never past the card.
    let path = format!("/files/{}notes.txt", "a-rather-long-folder-name/".repeat(8));
    let (shell, cx) = properties_on(cx, "Plain text document", &path, "");
    let card = cx
        .debug_bounds("op3-card")
        .expect("the Properties card is drawn");
    let value = cx
        .debug_bounds("op3-fact-value-0")
        .expect("the Path value is drawn");
    let next = cx
        .debug_bounds("op3-fact-value-1")
        .expect("the Kind value is drawn");
    assert!(
        value.right() <= card.right(),
        "the Path stays inside the card: {value:?} in {card:?}"
    );
    assert!(
        f32::from(value.size.height) >= 48.,
        "the long Path wraps over more than one line: {value:?}"
    );
    assert_eq!(
        next.origin.x, value.origin.x,
        "every value starts on one column"
    );
    assert!(
        next.origin.y >= value.origin.y + value.size.height,
        "the next fact sits below the wrapped Path: {next:?} after {value:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn properties_tags_lead_with_solid_squares(cx: &mut TestAppContext) {
    // EY-335: each known tag draws its 14 px square before its name (HIG properties-tagged.png); a name Flea does not know has none.
    let (shell, cx) = properties_on(
        cx,
        "Plain text document",
        "/files/notes.txt",
        r#","tags":["Red","Blue","work"]"#,
    );
    for selector in ["op3-tag-square-0", "op3-tag-square-1"] {
        let square = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} draws its square"));
        assert_eq!(
            (f32::from(square.size.width), f32::from(square.size.height)),
            (14., 14.),
            "{selector}"
        );
    }
    assert!(
        cx.debug_bounds("op3-tag-square-2").is_none(),
        "an unknown tag name draws no square"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
