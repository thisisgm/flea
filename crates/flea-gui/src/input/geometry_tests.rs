use super::*;

#[gpui::test]
fn u03_bar_hover_tracks_resize_without_a_mouse_move(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let lane = point(frame.bounds.right() - px(3.), frame.bounds.top() + px(50.));
    cx.simulate_mouse_move(lane, None, Modifiers::none());
    cx.run_until_parked();
    let wide = shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        assert!(input.in_lane);
        assert!(input.drawn.unwrap().shown);
        input.drawn.unwrap().knob.size.width
    });
    cx.simulate_resize(size(px(850.), px(600.)));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        assert!(
            !input.in_lane,
            "new hitbox is no longer under the stationary pointer"
        );
        let drawn = input.drawn.unwrap();
        assert!(drawn.frame.bounds.right() < lane.x);
        assert!(drawn.knob.size.width < wide);
        assert!(!drawn.shown);
    });
}
