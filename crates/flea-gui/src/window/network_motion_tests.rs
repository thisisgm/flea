//! Card geometry rises on open and its last frame remains mounted during the close fade.
use super::*;
use crate::window::dispatch::test_shell::{land, open_root};

fn top(cx: &mut VisualTestContext, name: &'static str) -> f32 {
    f32::from(
        cx.debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} is drawn"))
            .origin
            .y,
    )
}

#[gpui::test]
fn pb_network_2_network_card_rises_and_keeps_its_close_frame(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    cx.simulate_keystrokes("a");
    frame(cx);
    let first = top(cx, "network-motion");
    cx.executor().advance_clock(Duration::from_millis(180));
    frame(cx);
    let rest = top(cx, "network-motion");
    assert!(
        (first - rest - 10.).abs() < 0.01,
        "open rises ten pixels: {first} {rest}"
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(top(cx, "network-motion"), rest, "close only fades");
    cx.executor().advance_clock(Duration::from_millis(140));
    frame(cx);
    assert!(
        cx.debug_bounds("network-motion").is_none(),
        "the close frame retires"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_network_2_share_browser_rises_and_keeps_its_close_frame(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            shell.network_service(
                &Effect::Shares {
                    uri: "smb://nas/".into(),
                    label: "NAS".into(),
                    names: vec!["Docs".into()],
                    origin,
                },
                window,
                cx,
            );
        })
    });
    frame(cx);
    let first = top(cx, "share-motion");
    cx.executor().advance_clock(Duration::from_millis(180));
    frame(cx);
    let rest = top(cx, "share-motion");
    assert!((first - rest - 10.).abs() < 0.01);
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(top(cx, "share-motion"), rest);
    cx.executor().advance_clock(Duration::from_millis(140));
    frame(cx);
    assert!(cx.debug_bounds("share-motion").is_none());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-397: WindowBody.qml placed the ShareBrowser over the owner's listSlot, so the list header stays drawn above the shares.
#[gpui::test]
fn ey397_share_browser_covers_the_listing_and_keeps_the_header(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            shell.network_service(
                &Effect::Shares {
                    uri: "smb://nas/".into(),
                    label: "NAS".into(),
                    names: vec!["flea".into(), "secret".into()],
                    origin,
                },
                window,
                cx,
            );
        })
    });
    frame(cx);
    cx.executor().advance_clock(Duration::from_millis(400));
    frame(cx);
    let shares = cx
        .debug_bounds("share-motion")
        .expect("the browser is drawn");
    let (pane, header) = cx.update(|window, cx| {
        let shell = shell.read(cx);
        let host = shell.panes[0].as_ref().unwrap();
        let header = crate::list::Metrics::for_typeface(&host.list.borrow().typeface).header;
        (shell.geometry(window).panes[0], header)
    });
    let top = f32::from(shares.origin.y);
    assert!(
        (top - (pane.y + header)).abs() < 0.5,
        "the shares start under the list header: {top} vs {} + {header}",
        pane.y
    );
    assert!((f32::from(shares.size.width) - pane.width).abs() < 0.5);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
