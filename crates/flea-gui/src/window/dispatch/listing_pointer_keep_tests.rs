//! A pointer gesture that calls no Selection.js toggle, only, extendTo, all or clear keeps the landing (Selection.js:32-35, 47-67).
use super::listing_pointer_tests::{click, landed_on_beta_in, row_point, selected};
use super::test_shell::{asked, lists, row};
use crate::backend::{Backend, Event as BackendEvent};
use crate::state::Shell;
use crate::window::input::ActionEvent;
use flea::protocol::{Reply, RowFacts};
use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext,
};
use std::{cell::RefCell, rc::Rc};

fn rows() -> Vec<RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

fn landed(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Option<(usize, usize)>, usize) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.nav.landed, host.pane.held.listing)
    })
}

// The actions the click leaves unclaimed after the mounted families run.
fn handed_on(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    at: Point<Pixels>,
    button: MouseButton,
    count: usize,
    modifiers: Modifiers,
) -> Vec<&'static str> {
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let sink = emitted.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push(event.action)
        })
    });
    click(shell, cx, at, button, count, modifiers);
    let actions = emitted.borrow().clone();
    actions
}

// Whether a release has been counted as a tap on a row, which the click path does before it reads the binding.
fn tapped(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.tap.is_some()
    })
}

// The landing still names beta in the drawn listing, and beta is still the only row selected.
fn keeps_the_landing(shell: &Entity<Shell>, cx: &mut VisualTestContext, gesture: &str) {
    let (landed, listing) = landed(shell, cx);
    assert_eq!(landed, Some((listing, 1)), "{gesture} keeps the landing");
    assert_eq!(
        selected(shell, cx),
        vec![1],
        "{gesture} leaves the marks alone"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_right_click_on_the_selected_landed_row_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let beta = row_point(&shell, cx, 1);
    // The window root (window/root.rs:60-70) opens the menu in the capture phase, before the pane reads the click.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let down = MouseDownEvent {
                position: beta,
                button: MouseButton::Right,
                click_count: 1,
                modifiers: Modifiers::none(),
                first_mouse: false,
            };
            shell.menu_pointer_open(0, &down, window, cx);
        })
    });
    click(&shell, cx, beta, MouseButton::Right, 1, Modifiers::none());
    // The menu draws on the backend's inventory reply; the press's part is capturing the rows it addresses.
    let captured = shell.read_with(cx, |shell, _| {
        let capture = shell
            .menu
            .captured
            .as_ref()
            .expect("the right click asks for the menu");
        (capture.side, capture.rows.clone(), capture.cursor)
    });
    assert_eq!(
        captured,
        (0, vec![1], 1),
        "the menu addresses the selected row"
    );
    keeps_the_landing(&shell, cx, "a menu on a selected row");
}

// The pane's own menu arm (root opens no menu over the press); its effect is the menu order it hands on.
#[gpui::test]
fn the_pane_menu_arm_on_the_selected_landed_row_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let beta = row_point(&shell, cx, 1);
    let listing = landed(&shell, cx).1 as u64;
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let sink = emitted.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push((
                event.action,
                event.context,
                event.seat.listing,
                event.seat.cursor,
            ))
        })
    });
    click(&shell, cx, beta, MouseButton::Right, 1, Modifiers::none());
    assert_eq!(
        *emitted.borrow(),
        [("menu", flea::keys::Context::Listing, listing, 1)],
        "the arm asks for the menu on the landed row"
    );
    keeps_the_landing(&shell, cx, "the pane's menu arm on a selected row");
}

#[gpui::test]
fn a_middle_click_that_opens_a_tab_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let alpha = row_point(&shell, cx, 0);
    let actions = handed_on(&shell, cx, alpha, MouseButton::Middle, 1, Modifiers::none());
    assert!(
        actions.is_empty(),
        "the mounted tab family consumes the click"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| (
            shell.strip.tabs[0].count(),
            shell.strip.tabs[0].current()
        )),
        (2, 1),
        "the middle click on a folder opens and selects its new tab"
    );
    assert_eq!(
        asked(&shell, cx),
        "/files/alpha",
        "the pointed folder owns the new tab"
    );
    keeps_the_landing(&shell, cx, "openTab");
}

#[gpui::test]
fn a_middle_click_on_a_file_does_nothing_and_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let notes = row_point(&shell, cx, 2);
    let before = lists(&shell, cx);
    assert!(!tapped(&shell, cx), "no tap is counted before the click");
    let actions = handed_on(&shell, cx, notes, MouseButton::Middle, 1, Modifiers::none());
    assert!(tapped(&shell, cx), "the release reached the row's binding");
    assert_eq!(actions, Vec::<&str>::new(), "the click asks for nothing");
    assert_eq!(
        (asked(&shell, cx), lists(&shell, cx)),
        ("/files".into(), before)
    );
    keeps_the_landing(&shell, cx, "an effect-less click");
}

// The second tap of a modified click selects nothing, as Tap.js returns on any tap but the first.
fn second_tap(cx: &mut TestAppContext, modifiers: Modifiers, gesture: &str) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let alpha = row_point(&shell, cx, 0);
    let before = lists(&shell, cx);
    let actions = handed_on(&shell, cx, alpha, MouseButton::Left, 2, modifiers);
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert_eq!(cursor, 0, "{gesture} moves the cursor to the clicked row");
    assert_eq!(actions, Vec::<&str>::new(), "{gesture} asks for nothing");
    assert_eq!(
        (asked(&shell, cx), lists(&shell, cx)),
        ("/files".into(), before),
        "{gesture} opens nothing"
    );
    keeps_the_landing(&shell, cx, gesture);
}

#[gpui::test]
fn a_second_tap_of_a_ctrl_click_keeps_the_landing(cx: &mut TestAppContext) {
    second_tap(cx, Modifiers::control(), "ctrl left x2");
}

#[gpui::test]
fn a_second_tap_of_a_shift_click_keeps_the_landing(cx: &mut TestAppContext) {
    second_tap(cx, Modifiers::shift(), "shift left x2");
}

// A search result reveals on a tap; this pane's landing is armed by hand on the results' listing.
#[gpui::test]
fn a_tap_that_reveals_a_search_result_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("f s");
    let found = vec![row("notes.txt", false), row("sub/deep", true)];
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let line = r#"{"t":"listed","path":"/files","n":2,"gen":1}"#;
            shell.receive(0, Backend::test_reply_line(line), window, cx);
            let reply = Reply::Rows { start: 0, listing: 11, rows: found };
            shell.receive(0, BackendEvent::Session { generation: 1, reply }, window, cx);
            let line = r#"{"t":"searched","n":2,"scanned":9,"ms":1,"cancelled":false,"gen":1,"walking":false}"#;
            shell.receive(0, Backend::test_reply_line(line), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let armed = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(
            host.search.owner.session.reveals_on_activate(),
            "the rows are results"
        );
        host.nav.landed = Some((host.pane.held.listing, 1));
        host.nav.landed
    });
    let deep = row_point(&shell, cx, 1);
    click(&shell, cx, deep, MouseButton::Left, 1, Modifiers::none());
    assert_eq!(
        asked(&shell, cx),
        "/files/sub",
        "the tap reveals the result's folder"
    );
    assert_eq!(landed(&shell, cx).0, armed, "reveal keeps the landing");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
