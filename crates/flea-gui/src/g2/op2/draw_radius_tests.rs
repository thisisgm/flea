use super::draw_flash_tests::LiveFlash;
use crate::list::bench::real_cx;
use gpui::{point, px, size, Modifiers, MouseButton, VisualTestContext, WindowHandle};

fn painted_radii(
    visual: &mut VisualTestContext,
    window: WindowHandle<LiveFlash>,
    radius: f32,
    wash: Option<&'static str>,
) {
    let frame = visual.debug_bounds("op2-flash-frame").unwrap();
    let wash = wash.map(|selector| {
        visual
            .debug_bounds(selector)
            .expect("the requested wash is drawn")
    });
    window
        .update(visual, |_, window, _| {
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            let border = quads
                .iter()
                .find(|q| q.bounds == frame.scale(scale) && q.border_widths.top.0 > 0.)
                .expect("the frame has a painted border quad");
            for value in [
                border.corner_radii.top_left,
                border.corner_radii.top_right,
                border.corner_radii.bottom_left,
                border.corner_radii.bottom_right,
            ] {
                assert_eq!(
                    value.0,
                    radius * scale,
                    "each painted frame corner uses the live radius"
                );
            }
            if let Some(bounds) = wash {
                let fill = quads
                    .iter()
                    .find(|q| q.bounds == bounds.scale(scale) && q.border_widths.top.0 == 0.)
                    .expect("the wash has a painted fill quad");
                let inner = (radius * scale - border.border_widths.top.0).max(0.);
                for value in [
                    fill.corner_radii.top_left,
                    fill.corner_radii.top_right,
                    fill.corner_radii.bottom_left,
                    fill.corner_radii.bottom_right,
                ] {
                    assert_eq!(
                        value.0, inner,
                        "each wash corner follows the matching inner radius"
                    );
                }
            }
        })
        .unwrap();
}

#[test]
fn r7_painted_radii_follow_every_action_state_and_device_scale() {
    for radius in [0., 8.] {
        for scale in [1., 2.] {
            for make in [
                LiveFlash::live_flash_action,
                LiveFlash::live_cancel,
                LiveFlash::failed_end,
                LiveFlash::done_end,
            ] {
                let mut cx = real_cx();
                cx.update(|cx| cx.set_reduce_motion(true));
                let window = cx.open_window(size(px(1100.), px(700.)), |window, _| {
                    window.set_scale_factor(scale);
                    let mut live = make();
                    live.radius = radius;
                    live
                });
                let mut visual = VisualTestContext::from_window(window.into(), &cx);
                visual.run_until_parked();
                let action = ["op2-flash-action", "op2-flash-cancel", "op2-flash-end"]
                    .iter()
                    .find_map(|selector| visual.debug_bounds(selector))
                    .unwrap();
                painted_radii(&mut visual, window, radius, None);
                visual.simulate_mouse_move(action.center(), None, Modifiers::none());
                visual.run_until_parked();
                painted_radii(&mut visual, window, radius, Some("op2-flash-wash-hover"));
                visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
                visual.run_until_parked();
                painted_radii(&mut visual, window, radius, Some("op2-flash-wash-press"));
                visual.simulate_mouse_move(
                    point(px(20.), px(20.)),
                    Some(MouseButton::Left),
                    Modifiers::none(),
                );
                visual.run_until_parked();
                painted_radii(&mut visual, window, radius, None);
                visual.simulate_mouse_move(
                    action.center(),
                    Some(MouseButton::Left),
                    Modifiers::none(),
                );
                visual.run_until_parked();
                painted_radii(&mut visual, window, radius, Some("op2-flash-wash-press"));
                visual.simulate_mouse_up(
                    point(px(-10.), px(-10.)),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                visual.run_until_parked();
                painted_radii(&mut visual, window, radius, None);
            }
        }
    }
}
