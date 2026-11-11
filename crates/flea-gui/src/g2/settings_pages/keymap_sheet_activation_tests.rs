//! Enter on each sheet result reaches the same window owner as its original surface.
use super::tests::english;
use crate::{backend::Backend, state::Shell};
use flea::{
    ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::RailEntry,
        recent::Visit,
    },
    ui_logic_settings_pages::sheet::{self, Decision},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

fn opened(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Vec<String>,
) {
    let (backend, mut read) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.locale.catalog = english();
        shell.launch.settings.data = flea::uischema::defaults();
        let host = shell.panes[0].as_mut().unwrap();
        let held = &host.pane.held;
        host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: held.listing,
            total: held.total,
            start: held.start,
            rows: vec![
                flea::protocol::RowFacts {
                    name: "notes.txt".into(),
                    mode: 0o100644,
                    ..Default::default()
                };
                held.rows.len()
            ],
        });
        let join = crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        );
        join.places.update(cx, |producer, cx| {
            producer.fixture_home("/home/test", "", cx);
            producer.replace_sections(
                vec![RailSection {
                    heading: Heading::Key("places.heading.places"),
                    rows: vec![RailEntry {
                        label_key: "places.row.home",
                        path: "/home/test".into(),
                        ..Default::default()
                    }],
                }],
                cx,
            );
            producer.places.update(cx, |places, _| {
                places.history = vec![Visit {
                    path: "/files/photo.png".into(),
                    stamp: String::new(),
                }];
            });
        });
        shell.rail = Some(join);
        shell.open_sheet(window, cx);
        let service = shell.rail.as_ref().unwrap().places.read(cx).places.clone();
        service.update(cx, |places, cx| {
            places.reread_history(
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/g2/rail_places/recent-fixture.xbel"
                )
                .into(),
                cx,
            )
        });
        shell
    });
    cx.run_until_parked();
    (shell, cx, move || std::iter::from_fn(&mut read).collect())
}

fn query(shell: &Entity<Shell>, cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes(&text.chars().map(String::from).collect::<Vec<_>>().join(" "));
    shell.read_with(cx, |shell, cx| {
        assert_eq!(
            shell.settings.sheet.as_ref().unwrap().read(cx).sheet.query,
            text
        )
    });
}

fn pick(shell: &Entity<Shell>, cx: &mut VisualTestContext, wanted: Decision) {
    shell.update(cx, |shell, cx| {
        shell
            .settings
            .sheet
            .as_ref()
            .unwrap()
            .update(cx, |view, _| {
                view.sheet.cursor = view
                    .sheet
                    .ranked
                    .iter()
                    .position(|row| sheet::dispatch(row) == wanted)
                    .unwrap_or_else(|| panic!("sheet offers {wanted:?}: {:?}", view.sheet.ranked));
            });
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, _| shell.settings.sheet.is_none()),
        "Enter closes the sheet"
    );
}

fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    shell.update_in(cx, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx)
    });
    cx.run_until_parked();
}

fn validated(shell: &Entity<Shell>, cx: &mut VisualTestContext, action: &str) -> u64 {
    let id = shell.read_with(cx, |shell, _| {
        shell
            .menu
            .captured
            .as_ref()
            .expect("the sheet starts the menu snapshot")
            .id
    });
    reply(
        shell,
        cx,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[32768]}}"#),
    );
    reply(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"validate","id":{id},"ok":true,"action":"{action}","paths":["/files/notes.txt"]}}"#
        ),
    );
    id
}

#[gpui::test]
fn keymap_sheet_menu_enter_dispatches_the_current_selection(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = opened(cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![2]
    });
    shell.update(cx, |shell, _| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"menu":{"hidden":["duplicate"]}}"#).unwrap();
    });
    query(&shell, cx, "duplicate");
    pick(
        &shell,
        cx,
        Decision::Menu {
            menu_action: "duplicate".into(),
        },
    );
    let id = validated(&shell, cx, "duplicate");
    let request = shell.read_with(cx, |shell, _| shell.menu.sequence);
    let sent = sent();
    assert!(
        sent.iter().any(|line| line.contains(r#""rows":[2]"#)),
        "snapshot keeps selected rows: {sent:?}"
    );
    assert!(
        sent.iter().any(|line| line
            == &format!(
                r#"{{"c":"duplicate","path":"/files/notes.txt","menuId":{id},"request":{request}}}"#
            )),
        "menu owner ran: {sent:?}"
    );
}

#[gpui::test]
fn keymap_sheet_confirm_enter_opens_the_existing_confirmation(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = opened(cx);
    // A menu-only dangerous row exercises Confirm independently of an equivalent bound action.
    shell.update(cx, |shell, cx| {
        shell
            .settings
            .sheet
            .as_ref()
            .unwrap()
            .update(cx, |view, _| {
                view.sheet
                    .rows
                    .retain(|row| row.action != "deletePermanently");
            })
    });
    query(&shell, cx, "permanently");
    pick(
        &shell,
        cx,
        Decision::Confirm {
            menu_action: "deletePermanently".into(),
        },
    );
    let id = validated(&shell, cx, "deletePermanently");
    assert!(sent()
        .iter()
        .any(|line| line == &format!(r#"{{"c":"menuaction","op":"prepareDelete","id":{id}}}"#)));
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"prepareDelete","id":{id},"ok":true,"token":77,"count":1,"bytes":12}}"#
        ),
    );
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the existing confirm card owns the keyboard"
    );
}

#[gpui::test]
fn keymap_sheet_sources_find_home_and_a_recent_file(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx);
    query(&shell, cx, "home");
    shell.read_with(cx, |shell, cx| {
        let view = shell.settings.sheet.as_ref().unwrap().read(cx);
        assert!(view
            .sheet
            .ranked
            .iter()
            .any(|row| row.section == 2 && row.label.resolve(&view.sheet.catalog) == "Open Home"));
    });
    cx.simulate_keystrokes("backspace backspace backspace backspace");
    query(&shell, cx, "newer");
    shell.read_with(cx, |shell, cx| {
        assert!(shell
            .settings
            .sheet
            .as_ref()
            .unwrap()
            .read(cx)
            .sheet
            .ranked
            .iter()
            .any(|row| row.section == 3 && row.path == "/newer.txt"))
    });
}

#[gpui::test]
fn keymap_sheet_new_sources_requery_a_typed_query(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx);
    query(&shell, cx, "later");
    shell.update(cx, |shell, cx| {
        shell
            .settings
            .sheet
            .as_ref()
            .unwrap()
            .update(cx, |view, _| {
                let mut sources = view.sheet.sources.clone();
                sources.recent.push("/later.txt".into());
                view.sheet.set_sources(sources);
            })
    });
    shell.read_with(cx, |shell, cx| {
        let sheet = &shell.settings.sheet.as_ref().unwrap().read(cx).sheet;
        assert_eq!(sheet.query, "later", "the typed query survives new sources");
        assert!(
            sheet
                .ranked
                .iter()
                .any(|row| row.section == 3 && row.path == "/later.txt"),
            "a recent file that arrives after the query is typed is found without retyping"
        );
        assert!(sheet.sources.recent.iter().any(|path| path == "/newer.txt"));
        assert!(
            !sheet
                .ranked
                .iter()
                .any(|row| row.section == 3 && row.path == "/newer.txt"),
            "the typed query still filters the recent files"
        );
    });
}

#[gpui::test]
fn keymap_sheet_place_enter_uses_the_rail_activation(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = opened(cx);
    // Supply a candidate so missing source population cannot mask a broken Enter handoff.
    shell.update(cx, |shell, cx| {
        shell
            .settings
            .sheet
            .as_ref()
            .unwrap()
            .update(cx, |view, _| {
                view.sheet.sources.rail = vec![sheet::RailEntry {
                    label: "Home".into(),
                    identity: "Place(Home, Home, \"/home/test\")".into(),
                    ..Default::default()
                }];
            })
    });
    query(&shell, cx, "home");
    let decision = shell.read_with(cx, |shell, cx| {
        let view = shell.settings.sheet.as_ref().unwrap().read(cx);
        sheet::dispatch(
            view.sheet
                .ranked
                .iter()
                .find(|row| row.section == 2)
                .unwrap(),
        )
    });
    let before = sent();
    assert!(
        !before
            .iter()
            .any(|line| flea::json::field_str(line, "path").as_deref() == Some("/home/test")),
        "nothing before Enter opens Home"
    );
    pick(&shell, cx, decision);
    assert!(
        sent()
            .iter()
            .any(|line| flea::json::field_str(line, "path").as_deref() == Some("/home/test")),
        "rail activation opens Home"
    );
}

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn launch(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    OPENED.with(|opened| opened.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

#[gpui::test]
fn keymap_sheet_recent_enter_uses_the_existing_file_opener(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx);
    OPENED.with(|opened| opened.borrow_mut().clear());
    shell.update(cx, |shell, cx| {
        shell.opener[0].launch = launch;
        shell
            .settings
            .sheet
            .as_ref()
            .unwrap()
            .update(cx, |view, _| {
                view.sheet.sources.recent = vec!["/files/photo.png".into()]
            });
    });
    query(&shell, cx, "photo");
    pick(
        &shell,
        cx,
        Decision::Recent {
            path: "/files/photo.png".into(),
        },
    );
    OPENED.with(|opened| {
        let opened = opened.borrow();
        assert_eq!(opened.len(), 1, "exactly one opener invocation");
        assert_eq!(&opened[0][1..], ["--open", "/files/photo.png"]);
    });
}
