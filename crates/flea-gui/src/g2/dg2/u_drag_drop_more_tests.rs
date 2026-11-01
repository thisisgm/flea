//! U-DRAG-DROP: the sub-slop drop gate remains silent before the ordinary tap.
use super::mounted_tests::{centre, drain, frame, open, Root};
use super::u_drag_drop_tests::{out_and_back, status};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};

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
