//! Real pointer presses on the production Open With card must reach its existing model and backend lane.
use super::menudeliver_tests::{open, wire, REG};
use super::menus_tests::{home, reply, settle};
use super::test_shell::land;
use crate::{g2::open_with_properties::openwith::Stop, state::Shell};
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

const APPS: &str = r#"{"id":"a.desktop","label":"A","icon":"","default":true},{"id":"b.desktop","label":"B","icon":"","default":false}"#;

#[path = "gap64_initial_tests.rs"]
mod gap64;

type Recorder = Box<dyn FnMut() -> Vec<String>>;

pub(super) fn prepared<'a>(
    cx: &'a mut TestAppContext,
    apps: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext, Recorder) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    shell.update(cx, |shell, cx| {
        shell.open_open_with(0, "/files/notes.txt".into(), cx)
    });
    let id = shell.read_with(cx, |shell, _| shell.open_with.as_ref().unwrap().id);
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"path":"/files/notes.txt","kind":"Plain text document","mime":"text/plain","applications":[{apps}],"installed":[]}}"#
        ),
    );
    settle(cx);
    sent();
    (shell, cx, Box::new(sent))
}

pub(super) fn click(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    settle(cx);
}

#[gpui::test]
fn openwith_pointer_row_launches_that_application_once(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = prepared(cx, APPS);
    let at = shell.read_with(cx, |shell, _| {
        shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("row1")
            .unwrap()
            .bounds
            .center()
    });
    click(cx, at);
    click(cx, at);
    let requests = sent();
    assert_eq!(
        requests.len(),
        1,
        "one pointer activation launches once while awaiting its reply: {requests:?}"
    );
    let request = flea::jsondoc::parse(&requests[0]).unwrap();
    assert_eq!(request.get("op").and_then(|v| v.as_str()), Some("openWith"));
    assert_eq!(
        request.get("application").and_then(|v| v.as_str()),
        Some("b.desktop")
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_pointer_filtered_row_uses_the_filtered_seat(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = prepared(cx, APPS);
    cx.simulate_keystrokes("b");
    settle(cx);
    let at = shell.read_with(cx, |shell, _| {
        let open = shell.open_with.as_ref().unwrap();
        assert_eq!(open.card.query, "b");
        assert!(open.frame.part("row1").is_none());
        open.frame.part("row0").unwrap().bounds.center()
    });
    click(cx, at);
    let requests = sent();
    assert_eq!(requests.len(), 1, "the filtered application launches");
    let request = flea::jsondoc::parse(&requests[0]).unwrap();
    assert_eq!(request.get("op").and_then(|v| v.as_str()), Some("openWith"));
    assert_eq!(
        request.get("application").and_then(|v| v.as_str()),
        Some("b.desktop")
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_pointer_always_is_used_by_the_launch(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = prepared(cx, APPS);
    let at = shell.read_with(cx, |shell, _| {
        let rule = shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("listRule")
            .unwrap()
            .bounds;
        rule.origin + gpui::point(gpui::px(25.), gpui::px(18.))
    });
    click(cx, at);
    assert!(
        shell.read_with(cx, |shell, _| shell.open_with.as_ref().unwrap().card.always),
        "a real press toggles Always"
    );
    let at = shell.read_with(cx, |shell, _| {
        shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("row0")
            .unwrap()
            .bounds
            .center()
    });
    click(cx, at);
    let requests = sent();
    assert_eq!(requests.len(), 1);
    let request = flea::jsondoc::parse(&requests[0]).unwrap();
    assert_eq!(request.get("always").and_then(|v| v.as_bool()), Some(true));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_fidelity_waiting_row_stays_grey_while_the_footer_holds_focus(cx: &mut TestAppContext) {
    use crate::g2::open_with_properties::shell::{AppRow, Block, Cursor};
    let (shell, cx, _sent) = prepared(cx, APPS);
    let at = shell.read_with(cx, |shell, _| {
        let rule = shell
            .open_with
            .as_ref()
            .unwrap()
            .frame
            .part("listRule")
            .unwrap()
            .bounds;
        rule.origin + gpui::point(gpui::px(25.), gpui::px(18.))
    });
    click(cx, at);
    for focus in [Stop::Always, Stop::Cancel, Stop::Open] {
        shell.read_with(cx, |shell, _| {
            let open = &shell.open_with.as_ref().unwrap().card;
            assert_eq!(open.focus, focus);
            let card = open.card(&shell.locale.catalog);
            let lit: Vec<_> = card
                .body
                .iter()
                .find_map(|block| match block {
                    Block::Apps(rows, _) => Some(rows),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("the application list is drawn while {focus:?} holds focus"))
                .iter()
                .filter_map(|row| match row {
                    AppRow::App { at, cursor, .. } if *cursor != Cursor::None => Some((*at, *cursor)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                lit,
                vec![(open.cursor, Cursor::Waiting)],
                "openwith-check/cancel/open: the app Open will use keeps the grey highlight while {focus:?} holds focus"
            );
        });
        cx.simulate_keystrokes("tab");
        settle(cx);
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_pointer_clear_restores_rows_and_search_focus(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = prepared(cx, APPS);
    cx.simulate_keystrokes("z z z");
    settle(cx);
    let clear = cx
        .debug_bounds("op3-clear")
        .expect("populated search has a live clear control");
    click(cx, clear.center());
    assert!(
        shell.read_with(cx, |shell, _| {
            let open = &shell.open_with.as_ref().unwrap().card;
            open.query.is_empty() && open.focus == Stop::Search && open.can_open()
        }),
        "clear restores the applications without launching"
    );
    assert!(cx.debug_bounds("op3-clear").is_none());
    assert!(sent().is_empty());
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |shell, _| shell.open_with.is_none()));
    assert_eq!(sent().len(), 1, "Escape still expires the snapshot");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
