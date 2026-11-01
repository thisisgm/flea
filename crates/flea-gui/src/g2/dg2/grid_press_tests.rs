//! Grid keeps tap selection pending while the same press may still lift a drag cohort.
use super::{grid, lines_shown, tile, Root};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};

fn cohort(name: &str, selected: &[usize], modifiers: Modifiers, expected: &str) {
    let root = Root::new(name);
    let mut cx = bench::real_cx();
    let (handle, _lines) = grid(&mut cx, &root);
    handle
        .update(&mut cx, |shell, _, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.selected = selected.to_vec();
            host.input.selection.sync(&host.selected);
            host.pane.cursor = selected.first().copied().unwrap_or(0);
            cx.notify();
        })
        .unwrap();
    let folder = tile(&mut cx, handle, 0);
    let file = tile(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(file, None, modifiers);
    vcx.simulate_mouse_down(file, MouseButton::Left, modifiers);
    vcx.run_until_parked();
    handle
        .update(&mut vcx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().selected, selected);
        })
        .unwrap();
    vcx.simulate_mouse_move(
        file + point(px(24.), px(0.)),
        Some(MouseButton::Left),
        modifiers,
    );
    vcx.run_until_parked();
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), modifiers);
    vcx.run_until_parked();
    assert_eq!(lines_shown(&mut vcx, handle), [expected.to_owned()]);
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    vcx.simulate_mouse_up(folder, MouseButton::Left, modifiers);
    vcx.run_until_parked();
}

#[test]
fn uishlist_native_grid_ctrl_drag_lifts_only_the_unmarked_file() {
    cohort(
        "gridctrl",
        &[],
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
        "Copy 1 item to alpha",
    );
}

#[test]
fn uishlist_native_grid_plain_drag_keeps_the_selected_cohort() {
    cohort(
        "gridmarked",
        &[1, 2],
        Modifiers::default(),
        "Move 2 items to alpha · ctrl copies, shift moves",
    );
}
