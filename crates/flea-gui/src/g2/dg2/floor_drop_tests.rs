//! PB-DRAG-1: search results have no floor of their own, so a drop on their blank ground lands nowhere (PaneWire.qml).
use super::mounted_tests::{drain, frame, open, Root};
use super::router::{self, Native};
use crate::list::bench;
use flea::ui_logic_dg2::URILIST;
use gpui::{point, px, VisualTestContext};
use std::sync::Arc;

#[test]
fn pb_drag_1_a_drop_on_search_ground_lands_nowhere() {
    let root = Root::new("search-floor");
    let outside = format!("{}/../c.txt", root.src());
    std::fs::write(&outside, b"x").unwrap();
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let ground = handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(shell.search_action("search", window, cx));
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.search.owner.active() && !host.pane.busy());
            let bounds = host.input.frame.unwrap().bounds;
            point(bounds.origin.x + px(40.), bounds.bottom() - px(12.))
        })
        .unwrap();
    drain(&mut lines);
    let uris = flea::ui_logic_dg1::build_urilist(&[outside]);
    let types = vec![URILIST.to_owned()];
    let get = |mime: &str| (mime == URILIST).then(|| Arc::<[u8]>::from(uris.as_slice()));
    handle
        .update(&mut vcx, |shell, window, cx| {
            shell.follow_file_drags(window, cx);
            assert!(router::shared(cx).land(&Native::new(
                window.window_handle().window_id().as_u64(),
                (f32::from(ground.x), f32::from(ground.y)),
                true,
                &types,
                &get,
                None,
                false,
                false
            )));
        })
        .unwrap();
    vcx.run_until_parked();
    assert!(
        drain(&mut lines)
            .iter()
            .all(|line| !line.contains("collisions") && !line.contains("transfer")),
        "the search floor takes no drop"
    );
}
