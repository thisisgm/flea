//! A carry lands from a click the window dispatches, through the pane element's press hold, in every view.
use super::m9_g2rows_25_rig::{answer, edit, frame, mark, open, riding};
use super::m9_g2rows_25_target_rig::{target, view, Fixture};
use crate::{columns::geometry::Slot, window::startup::View};
use gpui::{Modifiers, MouseButton, VisualTestContext};

#[test]
fn carry_window_click_lands_on_a_folder_in_every_view() {
    for (mode, row, slot, dest) in [
        (View::List, Some(0), Slot::Active, "dest/folder"),
        (View::List, None, Slot::Active, "dest"),
        (View::Grid, Some(0), Slot::Active, "dest/folder"),
        (View::Columns, Some(0), Slot::Active, "dest/folder"),
        (View::Columns, None, Slot::Active, "dest"),
        (View::Columns, Some(0), Slot::Child, "dest/folder/nested"),
    ] {
        lands(mode, row, slot, dest);
    }
}

fn lands(mode: View, row: Option<usize>, slot: Slot, expected: &str) {
    let fixture = Fixture::new();
    let mut cx = crate::list::bench::real_cx();
    let (from, mut from_lines) = open(&mut cx, &fixture.path("src"), &["folder", "a.txt"]);
    let (to, mut to_lines) = open(&mut cx, &fixture.path("dest"), &["folder", "a.txt"]);
    view(&mut cx, to, mode);
    from_lines();
    to_lines();
    mark(&mut cx, from, vec![0]);
    edit(&mut cx, from, |shell, window, cx| {
        shell.carry_lift(window, cx)
    });
    answer(&mut cx, from, &[&fixture.path("src/folder")]);
    let at = target(&mut cx, to, row, slot);
    {
        let mut window = VisualTestContext::from_window(to.into(), &cx);
        window.simulate_mouse_move(at, None, Modifiers::none());
        window.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        window.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        window.run_until_parked();
    }
    frame(&mut cx, to);
    let sent = to_lines();
    let dest = fixture.path(expected);
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"collisions""#)
            && line.contains(&format!(r#""dest":"{dest}""#))
            && line.contains(&fixture.path("src/folder"))),
        "WINDOW_CLICK_CARRY_MUST_LAND {mode:?} {slot:?} {row:?}: {sent:?}"
    );
    assert!(riding(&mut cx).is_none(), "CARRY_STILL_RIDES {mode:?}");
    // The landing press releases its hold with the button, so the next re-read is not held off.
    assert!(!edit(&mut cx, to, |shell, _, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .nav
        .gate
        .pointer_held));
}
