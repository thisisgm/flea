use super::{frames, mounted};
use gpui::{px, size, TestAppContext};

pub(super) fn check(cx: &mut TestAppContext, corner: f32) {
    let (shell, card, cx) = mounted(cx, corner, |shell, window, cx| {
        shell.mount_rail(cx);
        shell.ensure_network(window, cx);
        shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .card
            .clone()
    });
    cx.simulate_resize(size(px(1100.), px(800.)));
    cx.update(|window, cx| card.update(cx, |card, cx| card.open(window, cx)));
    super::painted(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    frames(
        "network-card-protocols-fields-buttons",
        &shell,
        cx,
        corner,
        &[18.],
    );
}

#[gpui::test]
fn vf_frames_network_live(cx: &mut TestAppContext) {
    check(cx, 0.);
}
