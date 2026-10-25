//! A pointer gesture that calls no Selection.js toggle, only, extendTo, all or clear keeps the landing (Selection.js:32-35, 47-67).
use super::listing_pointer_tests::{click, landed_on_beta_in, row_point, selected};
use super::test_shell::row;
use crate::backend::{Backend, Event as BackendEvent};
use crate::state::Shell;
use flea::protocol::{Reply, RowFacts};
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

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
    click(&shell, cx, beta, MouseButton::Right, 1, Modifiers::none());
    keeps_the_landing(&shell, cx, "a menu on a selected row");
}

#[gpui::test]
fn a_middle_click_that_opens_a_tab_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let alpha = row_point(&shell, cx, 0);
    click(&shell, cx, alpha, MouseButton::Middle, 1, Modifiers::none());
    keeps_the_landing(&shell, cx, "openTab");
}

#[gpui::test]
fn a_middle_click_on_a_file_does_nothing_and_keeps_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let notes = row_point(&shell, cx, 2);
    click(&shell, cx, notes, MouseButton::Middle, 1, Modifiers::none());
    keeps_the_landing(&shell, cx, "an effect-less click");
}

// The second tap of a modified click selects nothing, as Tap.js returns on any tap but the first.
fn second_tap(cx: &mut TestAppContext, modifiers: Modifiers, gesture: &str) {
    let (shell, cx) = landed_on_beta_in(cx, rows());
    let alpha = row_point(&shell, cx, 0);
    click(&shell, cx, alpha, MouseButton::Left, 2, modifiers);
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
    assert_eq!(landed(&shell, cx).0, armed, "reveal keeps the landing");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
