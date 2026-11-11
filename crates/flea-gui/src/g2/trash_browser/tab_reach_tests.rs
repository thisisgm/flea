//! TR2-b: Tab and Shift+Tab reach Empty Trash…, the stop just before the Trash list, and Return or Space reviews it.
use super::join_r5_tests::right_pane;
use super::join_tests::{draw, shell};
use super::trashview_tests::{enter, mounted, settle};
use crate::{list::bench::real_cx, window::root::Root};
use flea::jsondoc::Json;
use gpui::{TestAppContext, WindowHandle};

// Where the keyboard is: "empty" for Empty Trash…, "trash" for the Trash list, else the window's stop.
fn at(cx: &mut TestAppContext, window: WindowHandle<Root>) -> String {
    draw(cx, window);
    shell(cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        match (
            pane.focus.contains_focused(window, cx),
            pane.empty_focused(),
        ) {
            (true, true) => "empty".into(),
            (true, false) => "trash".into(),
            _ => shell.focus_view(window, cx).into(),
        }
    })
}

fn press(cx: &mut TestAppContext, window: WindowHandle<Root>, key: &str) -> String {
    cx.simulate_keystrokes(window.into(), key);
    at(cx, window)
}

fn trash_page(cx: &mut TestAppContext, total: usize) -> WindowHandle<Root> {
    let window = mounted(cx);
    cx.simulate_keystrokes(window.into(), "delete");
    draw(cx, window);
    enter(cx, window);
    settle(cx, window, total);
    assert_eq!(at(cx, window), "trash", "Trash opens on its list");
    window
}

fn strip_focus(cx: &mut TestAppContext, window: WindowHandle<Root>) -> bool {
    shell(cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .view()
            .strip_focus
    })
}

fn reviewing(cx: &mut TestAppContext, window: WindowHandle<Root>) -> bool {
    shell(cx, window, |shell, _, cx| {
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        view.confirming && view.pending_op == "prepare"
    })
}

#[test]
fn tr2b_tab_reaches_empty_trash_just_before_the_list() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx, 2);
    let mut stops = vec![press(&mut cx, window, "tab")];
    while stops.len() < 6 && stops.last().is_some_and(|stop| stop != "trash") {
        stops.push(press(&mut cx, window, "tab"));
    }
    assert_eq!(
        stops.first().map(String::as_str),
        Some("rail"),
        "Tab leaves the list for the rail"
    );
    assert_eq!(
        stops
            .len()
            .checked_sub(2)
            .map(|before| stops[before].as_str()),
        Some("empty"),
        "the ring reaches Empty Trash… and then the list: {stops:?}"
    );
    assert_eq!(
        stops.iter().filter(|stop| *stop == "empty").count(),
        1,
        "{stops:?}"
    );
}

#[test]
fn tr2b_shift_tab_walks_the_same_ring_backwards() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx, 2);
    assert_eq!(press(&mut cx, window, "shift-tab"), "empty");
    assert_eq!(
        press(&mut cx, window, "shift-tab"),
        "rail",
        "Shift+Tab leaves Empty Trash… for the stop before it"
    );
    assert!(
        !strip_focus(&mut cx, window),
        "the button lets its focus go with the keyboard"
    );
    assert_eq!(
        press(&mut cx, window, "shift-tab"),
        "trash",
        "the ring comes round to the list, never to Empty Trash… first"
    );
}

// The mac preset binds Return to rename and every preset binds Space to preview, so neither reaches the button through the table.
#[test]
fn tr2b_return_and_space_on_empty_trash_open_its_review() {
    for (preset, key) in flea::keys::PRESETS
        .iter()
        .flat_map(|preset| ["enter", "space"].map(|key| (*preset, key)))
    {
        let mut cx = real_cx();
        let window = trash_page(&mut cx, 2);
        shell(&mut cx, window, |shell, window, cx| {
            let patch = Json::Obj(vec![("keys".into(), Json::Str(preset.into()))]);
            shell.change_settings(patch, window, cx);
        });
        assert_eq!(press(&mut cx, window, "shift-tab"), "empty", "{preset}");
        assert!(!reviewing(&mut cx, window));
        cx.simulate_keystrokes(window.into(), key);
        draw(&mut cx, window);
        assert!(
            reviewing(&mut cx, window),
            "{preset}: {key} on Empty Trash… asks for the review a click asks for"
        );
    }
}

#[test]
fn tr2b_an_empty_trash_leaves_its_unavailable_button_out_of_the_ring() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx, 0);
    let mut stops = vec![press(&mut cx, window, "tab")];
    while stops.len() < 6 && stops.last().is_some_and(|stop| stop != "trash") {
        stops.push(press(&mut cx, window, "tab"));
    }
    assert_eq!(stops.first().map(String::as_str), Some("rail"), "{stops:?}");
    assert_eq!(stops.last().map(String::as_str), Some("trash"), "{stops:?}");
    assert!(!stops.iter().any(|stop| stop == "empty"), "{stops:?}");
    assert!(
        !strip_focus(&mut cx, window),
        "the ring enters an empty Trash on its list, with no focus kept on the button"
    );
    assert_eq!(
        press(&mut cx, window, "shift-tab"),
        "rail",
        "Shift+Tab passes the unavailable button"
    );
}

#[test]
fn tr2b_a_button_that_stops_being_live_is_no_longer_a_stop() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx, 2);
    assert_eq!(press(&mut cx, window, "shift-tab"), "empty");
    shell(&mut cx, window, |shell, _, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, _| {
            pane.host.view.pending_op = "delete".into();
            pane.host.view.operation_active = true;
        });
    });
    assert!(
        strip_focus(&mut cx, window),
        "a delete in flight keeps the focus the button held"
    );
    assert_eq!(at(&mut cx, window), "trash");
    assert_eq!(
        press(&mut cx, window, "tab"),
        "rail",
        "Tab leaves at once, with no press spent on a button that is no longer live"
    );
}

#[test]
fn tr2b_a_trash_that_empties_lets_the_button_focus_go_for_good() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx, 2);
    assert_eq!(press(&mut cx, window, "shift-tab"), "empty");
    cx.simulate_keystrokes(window.into(), "f5");
    settle(&mut cx, window, 0);
    assert!(
        !strip_focus(&mut cx, window),
        "an empty listing lets the focus the button held go"
    );
    cx.simulate_keystrokes(window.into(), "f5");
    settle(&mut cx, window, 2);
    assert_eq!(
        at(&mut cx, window),
        "trash",
        "items that come back find the keyboard on the list, never on Empty Trash…"
    );
}

#[test]
fn tr2b_a_dual_trash_steps_the_dual_ring_from_its_own_side() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx, 2);
    shell(&mut cx, window, |shell, _, _| right_pane(shell));
    let side = |cx: &mut TestAppContext| shell(cx, window, |shell, _, _| shell.focus_side);
    assert_eq!(
        (press(&mut cx, window, "tab"), side(&mut cx)),
        ("list".into(), 1),
        "Tab leaves the left side's Trash for the right list"
    );
    assert_eq!(
        (press(&mut cx, window, "shift-tab"), side(&mut cx)),
        ("trash".into(), 0),
        "Shift+Tab comes back onto the Trash list"
    );
    assert_eq!(press(&mut cx, window, "shift-tab"), "empty");
    assert_eq!(
        (press(&mut cx, window, "shift-tab"), side(&mut cx)),
        ("chrome".into(), 0),
        "Shift+Tab leaves Empty Trash… for the Dual ring's stop before the left list"
    );
}
