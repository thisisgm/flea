//! Extracted mounted cases; shared fixture ownership stays in the parent test module.
use super::*;

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
    let (backend, mut windows) = crate::backend::Backend::test_channel();
    let (original, held) = update(cx, fixture.handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        (
            host.backend.replace(backend),
            std::rc::Rc::clone(&host.pane.held),
        )
    });
    fixture.receive(cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    assert_eq!(said(cx, &fixture), "Tagged 1 item Red · z undoes");
    assert!(
        update(cx, fixture.handle, |shell, _, _| {
            shell.menu.tags.asked_for_test().is_some()
        }),
        "the completion requests the captured tag state"
    );
    let (start, count) =
        windows().expect("the answer rereads the held rows so the red square shows");
    assert_eq!(start, held.start);
    assert!(count > 0);
    update(cx, fixture.handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().backend = original;
        let mut rows = held.rows.iter().take(count).cloned().collect::<Vec<_>>();
        rows[1].tags = Some(vec!["Red".into()]);
        shell.receive(
            0,
            crate::backend::Event::Reply(Reply::Rows {
                start,
                listing: held.listing,
                rows,
            }),
            window,
            cx,
        );
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .held
                .row(1)
                .unwrap()
                .tags
                .as_deref(),
            Some(&["Red".to_owned()][..])
        );
    });
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
            .tag_state
            .filter(|state| *state != flea::ui_logic_pl4::tags::State::Absent)
            .map(|state| state == flea::ui_logic_pl4::tags::State::Applied)
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
