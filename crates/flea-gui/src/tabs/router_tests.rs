//! The tab strip and the file drop share the process router; MIME sets keep each client to its own drags.
use super::transfer::{self, TAB_MIME};
use crate::{
    backend::Backend,
    g2::dg2::router::{self, Native, Offered},
    state::tests::shell_with,
};
use gpui::{point, px, size, Bounds, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn u25_tab_and_file_clients_share_the_native_router(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.follow_file_drags(window, cx);
            shell.follow_tab_drags(window, cx);
            let zone = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(30.)));
            shell.strip.capacity.set(true);
            shell.strip.drop_zone.set(Some(zone));
            let id = window.window_handle().window_id().as_u64();
            let shared = router::shared(cx);
            let tab = [TAB_MIME.to_string()];
            let files = ["text/uri-list".to_string()];
            let offer = |types: &[String], y: f32| {
                shared.offer(&Offered {
                    window: id,
                    position: (10., y),
                    types,
                })
            };
            let moved = offer(&tab, 10.);
            assert!(
                moved.move_ && !moved.copy,
                "a tab over the strip is move-only"
            );
            // The fork asks only at Enter, so a tab entering off the strip is taken and refused at its drop.
            let outside = offer(&tab, 300.);
            assert!(
                outside.move_ && !outside.copy,
                "a tab entering off the strip may still reach it, and is never a file"
            );
            assert!(
                offer(&files, 10.).copy,
                "a file drag over the strip still reaches the file client"
            );
            let payload = transfer::payload("222", "lift-1", "/files/child", "list", "");
            let tab_data =
                |mime: &str| (mime == TAB_MIME).then(|| Arc::<[u8]>::from(payload.as_bytes()));
            let tab_drop = Native::new(id, (10., 300.), true, &tab, &tab_data, None, false, false);
            assert!(
                !shared.land(&tab_drop),
                "a tab dropped off the strip lands nowhere"
            );
            let file_data = |mime: &str| {
                (mime == "text/uri-list").then(|| Arc::<[u8]>::from(&b"file:///files/a\n"[..]))
            };
            let file_enter = Native::new(
                id,
                (10., 10.),
                false,
                &files,
                &file_data,
                None,
                false,
                false,
            );
            assert!(
                shared.land(&file_enter),
                "a file entering the strip is the file client's"
            );
            shell.stop_tabs();
            shell.strip.capacity.set(true);
            assert!(
                !offer(&tab, 10.).move_,
                "a dropped strip registration leaves the router"
            );
            assert!(offer(&files, 10.).copy, "file drags outlive the strip");
            shell.forget_file_drags(window, cx);
        })
    });
}

#[gpui::test]
fn u25_tab_drops_reach_both_registered_windows(cx: &mut TestAppContext) {
    let (backend_a, _requests_a) = Backend::test_channel();
    let (a, a_cx) = cx.add_window_view(|_, cx| shell_with(backend_a, cx));
    let zone = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(30.)));
    let id_a = a_cx.update(|window, cx| {
        a.update(cx, |shell, cx| {
            shell.follow_tab_drags(window, cx);
            shell.with_tabs(|bar, panes| bar.set_pane(panes, Some(0)));
            shell.strip.capacity.set(true);
            shell.strip.drop_zone.set(Some(zone));
        });
        window.window_handle().window_id().as_u64()
    });
    let (backend_b, _requests_b) = Backend::test_channel();
    let (b, b_cx) = cx.add_window_view(|_, cx| shell_with(backend_b, cx));
    let id_b = b_cx.update(|window, cx| {
        b.update(cx, |shell, cx| {
            shell.follow_tab_drags(window, cx);
            shell.with_tabs(|bar, panes| bar.set_pane(panes, Some(0)));
            shell.strip.capacity.set(true);
            shell.strip.drop_zone.set(Some(zone));
        });
        window.window_handle().window_id().as_u64()
    });
    assert_ne!(id_a, id_b);
    b_cx.update(|_, cx| {
        let shared = router::shared(cx);
        let types = [TAB_MIME.to_string()];
        for (id, token) in [(id_b, "to-b"), (id_a, "to-a")] {
            let payload = transfer::payload("222", token, "/files/child", "list", "");
            let data = |mime: &str| (mime == TAB_MIME).then(|| Arc::from(payload.as_bytes()));
            assert!(
                shared
                    .offer(&Offered {
                        window: id,
                        position: (10., 10.),
                        types: &types
                    })
                    .move_,
                "the destination window accepts its offer"
            );
            let event = Native::new(id, (10., 10.), true, &types, &data, None, false, false);
            assert!(shared.land(&event), "the drop must reach window {id}");
        }
    });
    b_cx.run_until_parked();
    b_cx.update(|_, cx| {
        assert_eq!(a.read(cx).strip.bar.pending.as_ref().unwrap().token, "to-a");
        assert_eq!(b.read(cx).strip.bar.pending.as_ref().unwrap().token, "to-b");
    });
}
