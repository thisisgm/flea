//! Stale Grid geometry cannot capture a replacement listing before its next paint.
use super::*;

#[test]
fn gcomp_batch7_grid_menu_rejects_replaced_listing_before_repaint() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    frame(&mut cx, handle);
    let drawn_hit = |shell: &Shell| {
        let host = shell.panes[0].as_ref().unwrap();
        let state = host.grid.as_ref().unwrap().state.borrow();
        let tile = &state.drawn()[1];
        (
            state.bounds.origin
                + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                ),
            tile.listing,
            state.frame_held.as_ref().unwrap().listing,
        )
    };
    let (backend, mut requests) = Backend::test_lines();
    let (stale_capture, stale_snapshots, next_listing) = edit(
        &mut cx,
        handle,
        |shell, window, cx| {
            let (at, row, old_listing) = drawn_hit(shell);
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.pane.held.listing, old_listing);
            assert_eq!(
                host.pane.held.row(row).unwrap().name,
                format!("file-{row}.txt")
            );
            let path = flea::json::escape(&host.path);
            let total = host.pane.held.total;
            let next_listing = old_listing + 1;
            host.backend = Some(backend);
            let rows = (0..total)
                .map(|index| {
                    format!(
                        r#"{{"n":"replacement-{index}.txt","d":false,"s":0,"m":0,"p":32768,"i":"text"}}"#
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            for line in [
                format!(r#"{{"t":"listed","path":"{path}","n":{total}}}"#),
                format!(r#"{{"t":"rows","start":0,"listing":{next_listing},"rows":[{rows}]}}"#),
            ] {
                shell.receive(0, Backend::test_reply_line(&line), window, cx);
            }
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.held.listing, next_listing);
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready);
            assert!(!host.pane.busy());
            assert!(!shell.input_blocked(cx));
            assert_eq!(
                host.pane.held.row(row).unwrap().name,
                format!("replacement-{row}.txt")
            );
            assert_eq!(drawn_hit(shell), (at, row, old_listing));
            // Keep receive and the real input handler in one update, before any frame pump.
            shell.menu_pointer_open(
                0,
                &gpui::MouseDownEvent {
                    position: at,
                    button: MouseButton::Right,
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx,
            );
            let captured = shell.menu.captured.as_ref().map(|capture| {
                (
                    capture.listing,
                    capture.rows.clone(),
                    shell.menu.context.row_name.clone(),
                )
            });
            let snapshots = std::iter::from_fn(&mut requests)
                .filter(|line| {
                    let document = flea::jsondoc::parse(line).unwrap();
                    document.get("c").and_then(Json::as_str) == Some("menuaction")
                        && document.get("op").and_then(Json::as_str) == Some("snapshot")
                })
                .collect::<Vec<_>>();
            // Exercise the fresh-frame control even when the stale click opened a menu.
            shell.retire_menu(window, cx);
            while requests().is_some() {}
            (captured, snapshots, next_listing)
        },
    );
    frame(&mut cx, handle);
    let (at, row, painted_listing) = edit(&mut cx, handle, |shell, _, _| drawn_hit(shell));
    assert_eq!(painted_listing, next_listing);
    right_press(&mut cx, handle, at);
    edit(&mut cx, handle, |shell, _, _| {
        let capture = shell
            .menu
            .captured
            .as_ref()
            .expect("fresh painted tile opens");
        assert_eq!(capture.listing, next_listing);
        assert_eq!(capture.rows, [row]);
        assert_eq!(
            shell.menu.context.row_name,
            format!("replacement-{row}.txt")
        );
    });
    let snapshot = std::iter::from_fn(&mut requests)
        .find(|line| {
            let document = flea::jsondoc::parse(line).unwrap();
            document.get("c").and_then(Json::as_str) == Some("menuaction")
                && document.get("op").and_then(Json::as_str) == Some("snapshot")
        })
        .expect("fresh painted tile requests its real snapshot");
    let document = flea::jsondoc::parse(&snapshot).unwrap();
    assert_eq!(document.get("c").and_then(Json::as_str), Some("menuaction"));
    assert_eq!(document.get("op").and_then(Json::as_str), Some("snapshot"));
    assert_eq!(
        flea::json::field_usize(&snapshot, "listing"),
        Some(next_listing)
    );
    assert_eq!(flea::json::field_usize_array(&snapshot, "rows"), [row]);
    assert!(
        stale_capture.is_none(),
        "right-click before repaint captured a replacement file: {stale_capture:?}"
    );
    assert!(
        stale_snapshots.is_empty(),
        "right-click before repaint sent replacement snapshots: {stale_snapshots:?}"
    );
}
