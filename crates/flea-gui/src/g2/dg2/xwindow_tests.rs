//! EY-344 (tests/ui.sh xwdrag): a row lifted in one Flea window and released on another window's floor lands there.
use super::mounted_tests::{centre, frame, open, transfer, Root};
use super::router::{self, Native};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};
use std::sync::Arc;

#[test]
fn ey344_a_row_dragged_from_one_window_lands_on_the_other_windows_floor() {
    let root = Root::new("xwdrag");
    let other = format!("{}/other", root.src().trim_end_matches("/src"));
    std::fs::create_dir_all(&other).unwrap();
    let mut cx = bench::real_cx();
    let (a, _a_lines) = open(&mut cx, root.src());
    let (b, mut b_lines) = open(&mut cx, other.clone());
    for handle in [a, b] {
        handle
            .update(&mut cx, |shell, window, cx| {
                shell.follow_file_drags(window, cx)
            })
            .unwrap();
        frame(&mut cx, handle);
    }
    let file = centre(&mut cx, a, 1);
    let mut avcx = VisualTestContext::from_window(a.into(), &cx);
    avcx.simulate_mouse_move(file, None, Modifiers::default());
    avcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    // Window A's lift, as the platform carries it out of A.
    let offered = a
        .update(&mut avcx, |shell, window, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            shell.file_drag_lift(window, cx).unwrap().representations()
        })
        .unwrap();
    let mut bvcx = VisualTestContext::from_window(b.into(), &cx);
    let floor = b
        .update(&mut bvcx, |shell, _, _| {
            let bounds = shell.panes[0].as_ref().unwrap().input.frame.unwrap().bounds;
            point(bounds.origin.x + px(40.), bounds.bottom() - px(12.))
        })
        .unwrap();
    let types: Vec<String> = offered.iter().map(|(mime, _)| (*mime).to_owned()).collect();
    let get = |mime: &str| {
        offered
            .iter()
            .find(|(kind, _)| *kind == mime)
            .map(|(_, bytes)| Arc::<[u8]>::from(bytes.as_slice()))
    };
    b.update(&mut bvcx, |_, window, cx| {
        assert!(router::shared(cx).land(&Native::new(
            window.window_handle().window_id().as_u64(),
            (f32::from(floor.x), f32::from(floor.y)),
            true,
            &types,
            &get,
            None,
            false,
            false
        )));
    })
    .unwrap();
    bvcx.run_until_parked();
    let transfer = transfer(&mut bvcx, b, &mut b_lines);
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "{transfer}"
    );
    assert!(
        transfer.contains(&format!(r#""dest":"{other}""#)),
        "{transfer}"
    );
    avcx.simulate_mouse_up(file, MouseButton::Left, Modifiers::default());
}
