//! Recent in Dual: a history ask answers the side that asked, and a Refresh on the unfocused pane leaves the rail's side alone.
use super::{
    railrecent_tests::{frame, rows},
    test_shell::{asked_on, land, land_on, open_with, row},
};
use crate::{state::Shell, window::navigate::Step};
use flea::ui_logic_rail_places::{order::recent_entry, rail::Kind};
use gpui::{Entity, TestAppContext, VisualTestContext};

// A Dual launch with both panes listed and the rail carrying the Recent row.
fn dual(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_with(cx, r#""keys":"default","view":"dual""#, None);
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    let right = asked_on(&shell, cx, 1);
    land_on(&shell, cx, 1, &right, vec![row("B", true)], 1, 2);
    rows(&shell, cx, vec![recent_entry()]);
    (shell, cx)
}

// The rail's Recent row asked for one side, without letting the history read land.
fn ask(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) {
    shell.update(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .update(cx, |producer, cx| {
                producer.side = side;
                let index = producer
                    .entries()
                    .iter()
                    .position(|entry| matches!(entry.kind, Kind::Recent))
                    .expect("the rail carries the Recent row");
                producer.activate(index, cx);
            });
    });
}

// The rail's Recent row activated for one side, and its history read landed.
fn activate(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) {
    ask(shell, cx, side);
    cx.run_until_parked();
    frame(cx);
}

// Focus moves to one pane the way the rail's focus change does, with no run in between so the history read stays pending.
fn focus_without_settling(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) {
    shell.update(cx, |shell, cx| {
        shell.focus_side = side;
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .update(cx, |producer, _| producer.side = side);
    });
}

fn step(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, step: Step) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(side, step, window, cx);
        })
    });
    frame(cx);
}

fn focus_side(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| shell.focus_side)
}

fn rail_side(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, cx| {
        shell.rail.as_ref().unwrap().places.read(cx).side
    })
}

fn recent_on(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> bool {
    shell.read_with(cx, |shell, _| shell.panes[side].as_ref().unwrap().recent.on)
}

// Recent asked for on the left, focus moved right, and the left navigated before the history read landed.
#[gpui::test]
fn railrecent_dual_late_reply_is_dropped_when_the_asking_side_navigates(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    ask(&shell, cx, 0);
    focus_without_settling(&shell, cx, 1);
    assert_eq!(
        rail_side(&shell, cx),
        1,
        "precondition: the rail follows focus to the right pane"
    );
    step(&shell, cx, 0, Step::Open("/work".into()));
    cx.run_until_parked();
    frame(cx);
    assert!(
        !recent_on(&shell, cx, 0),
        "the left pane's late reply finds no ticket"
    );
    assert!(
        !recent_on(&shell, cx, 1),
        "the right pane never asked for Recent"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Recent asked for on the left, focus moved right, and the right navigated: the left ask still lands on the left.
#[gpui::test]
fn railrecent_dual_navigating_the_other_pane_keeps_the_asking_side_ask(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    ask(&shell, cx, 0);
    focus_without_settling(&shell, cx, 1);
    assert_eq!(
        rail_side(&shell, cx),
        1,
        "precondition: the rail follows focus to the right pane"
    );
    step(&shell, cx, 1, Step::Open("/tmp".into()));
    cx.run_until_parked();
    frame(cx);
    assert!(
        recent_on(&shell, cx, 0),
        "the left ask survives a navigation on the right"
    );
    assert!(!recent_on(&shell, cx, 1));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A Refresh on the unfocused right pane while Recent stands there: the rail's side stays on the focused left pane.
#[gpui::test]
fn railrecent_dual_refresh_on_the_unfocused_pane_keeps_the_rail_side(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    activate(&shell, cx, 0);
    assert!(recent_on(&shell, cx, 0), "the left pane shows Recent");
    cx.simulate_keystrokes("tab");
    frame(cx);
    assert_eq!(focus_side(&shell, cx), 1, "focus moved to the right pane");
    rows(&shell, cx, vec![recent_entry()]);
    activate(&shell, cx, 1);
    assert!(recent_on(&shell, cx, 1), "the right pane shows Recent");
    land_on(&shell, cx, 1, "/", vec![row("tmp/alpha.txt", false)], 1, 3);
    cx.simulate_keystrokes("tab");
    frame(cx);
    assert_eq!(focus_side(&shell, cx), 0, "focus is back on the left pane");
    assert_eq!(
        rail_side(&shell, cx),
        0,
        "precondition: the rail side follows the focused pane"
    );
    let refreshed = cx
        .update(|window, cx| shell.update(cx, |shell, cx| shell.step(1, Step::Reread, window, cx)));
    assert!(
        matches!(refreshed, Ok(true)),
        "the Refresh runs on the unfocused right pane"
    );
    assert_eq!(
        rail_side(&shell, cx),
        focus_side(&shell, cx),
        "the rail's side follows the focused pane, not the refreshed one"
    );
    frame(cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
