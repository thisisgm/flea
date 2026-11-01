//! U-DRAG-DROP: the sub-slop drop gate remains silent before the ordinary tap; a lift or a ground press ends the slow click.
use super::mounted_tests::{centre, drain, frame, open, Root};
use super::u_drag_drop_tests::{out_and_back, status};
use crate::list::bench;
use crate::state::Shell;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext, WindowHandle};

#[test]
fn ey395_a_press_that_never_left_the_slop_still_drops_nothing() {
    let root = Root::new("ey395tap");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let near = point(folder.x + px(4.), folder.y + px(3.));
    out_and_back(&mut vcx, folder, near, folder);
    let before = status(&mut vcx, handle);
    // Mount a live lift even though GPUI itself may ignore this small pointer motion.
    handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            shell.land_own_drag(0, window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    assert_eq!(
        status(&mut vcx, handle),
        before,
        "a sub-slop drop is ignored, never refused"
    );
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(
        drain(&mut lines)
            .iter()
            .all(|line| !line.contains("transfer") && !line.contains("collisions")),
        "a jitter may open the folder but never transfers it"
    );
}

// A lone tap on the sole marked name, recorded so the next slow tap there would rename it.
fn armed_record(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) {
    handle
        .update(vcx, |shell, window, cx| {
            let tap = crate::window::rename::slow::Tap {
                row: 0,
                on_name: true,
                modifiers: Modifiers::default(),
                was_sole: true,
            };
            shell.slow_tap(0, tap, window, cx);
            assert!(shell.rename.slow.is_some(), "the tap records itself");
        })
        .unwrap();
}

fn recorded(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> bool {
    handle
        .update(vcx, |shell, _, _| shell.rename.slow.is_some())
        .unwrap()
}

#[test]
fn pb_drag_1_a_lift_or_a_ground_press_cancels_the_pending_slow_click() {
    let root = Root::new("slowcancel");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let row = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    armed_record(&mut vcx, handle);
    vcx.simulate_mouse_move(row, None, Modifiers::default());
    vcx.simulate_mouse_down(row, MouseButton::Left, Modifiers::default());
    assert!(recorded(&mut vcx, handle), "a row press keeps the record");
    handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            assert!(
                shell.rename.slow.is_none(),
                "FileDrag.liftBegan cancels the slow click"
            );
            shell.escape_file_drag(window, cx);
        })
        .unwrap();
    vcx.simulate_mouse_up(row, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    armed_record(&mut vcx, handle);
    let ground = handle
        .update(&mut vcx, |shell, _, _| {
            let bounds = shell.panes[0].as_ref().unwrap().input.frame.unwrap().bounds;
            point(bounds.origin.x + px(40.), bounds.bottom() - px(12.))
        })
        .unwrap();
    vcx.simulate_mouse_move(ground, None, Modifiers::default());
    vcx.simulate_mouse_down(ground, MouseButton::Left, Modifiers::default());
    assert!(
        !recorded(&mut vcx, handle),
        "SelectionBand's ground press cancels the slow click"
    );
    vcx.simulate_mouse_up(ground, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
}
