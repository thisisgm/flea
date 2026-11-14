//! Folder-size work survives viewport motion when its rows remain demanded.
use super::*;

#[test]
fn fx400_sub_row_scroll_keeps_visible_pending_walks() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    let replies: Vec<_> = (0..8).map(|_| next(&mut cx, handle)).collect();
    for scroll in [1., 2., 3.] {
        handle
            .update(&mut cx, |shell, _, _| {
                shell.panes[0].as_mut().unwrap().pane.scroll_y = scroll;
            })
            .unwrap();
        frame(&mut cx, handle);
        settle(&mut cx);
        quiet(&mut cx, handle);
    }
    for reply in replies {
        receive(&mut cx, handle, reply);
    }
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "243 B");
    assert_eq!(ask(&mut cx, handle, "dirSizeRequests", &[]), "1");
    assert_eq!(ask(&mut cx, handle, "dirSizeCancels", &[]), "0");
}

#[test]
fn fx400_viewport_exit_cancels_only_the_departing_row() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    settle(&mut cx);
    request(&mut cx, handle);
    let replies: Vec<_> = (0..8).map(|_| next(&mut cx, handle)).collect();
    handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = 31.;
        })
        .unwrap();
    frame(&mut cx, handle);
    let cancel = request(&mut cx, handle);
    assert_eq!(
        cancel.get("c").and_then(Json::as_str),
        Some("dirsizecancel")
    );
    assert_eq!(cancel.get("rows"), jsondoc::parse("[0]").ok().as_ref());
    settle(&mut cx);
    quiet(&mut cx, handle);
    for reply in replies {
        receive(&mut cx, handle, reply);
    }
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["1"]), ">244 B");
    handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = 0.;
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "\u{b7}");
    settle(&mut cx);
    let fresh = request(&mut cx, handle);
    assert_eq!(fresh.get("c").and_then(Json::as_str), Some("dirsize"));
    assert_eq!(fresh.get("rows"), jsondoc::parse("[0]").ok().as_ref());
    assert_eq!(ask(&mut cx, handle, "dirSizeRequests", &[]), "2");
}

#[test]
fn fx400_superseded_listing_reply_cannot_complete_a_reused_row() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    settle(&mut cx);
    request(&mut cx, handle);
    for _ in 0..8 {
        next(&mut cx, handle);
    }
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = Rc::new(Held {
                listing: 3,
                total: 2000,
                start: 0,
                rows: bench::fixture("2k"),
            });
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.held.listing, 3);
        })
        .unwrap();
    assert_eq!(
        request(&mut cx, handle).get("c").and_then(Json::as_str),
        Some("dirsizecancel")
    );
    assert_eq!(
        request(&mut cx, handle).get("c").and_then(Json::as_str),
        Some("fsinfo")
    );
    let fs = next(&mut cx, handle);
    receive(&mut cx, handle, fs);
    handle
        .update(&mut cx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.held.listing, 3);
        })
        .unwrap();
    frame(&mut cx, handle);
    settle(&mut cx);
    let mut command = request(&mut cx, handle);
    if command.get("c").and_then(Json::as_str) == Some("fsinfo") {
        // The picture planner may also classify the replacement listing.
        let fs = next(&mut cx, handle);
        receive(&mut cx, handle, fs);
        command = request(&mut cx, handle);
    }
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    receive(
        &mut cx,
        handle,
        Backend::test_reply_line(
            r#"{"t":"dirsized","listing":2,"row":0,"bytes":999,"partial":false}"#,
        ),
    );
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "\u{b7}");
    receive(
        &mut cx,
        handle,
        Backend::test_reply_line(
            r#"{"t":"dirsized","listing":3,"row":0,"bytes":11,"partial":false}"#,
        ),
    );
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "11 B");
}

#[test]
fn fx400_identical_inline_window_preserves_row_revision() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    settle(&mut cx);
    request(&mut cx, handle);
    for _ in 0..8 {
        let reply = next(&mut cx, handle);
        receive(&mut cx, handle, reply);
    }
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, window, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let revision = host.list.borrow().view.mark_revision(2);
            let old = &host.pane.held;
            let mut held = Held {
                listing: old.listing,
                total: old.total,
                start: old.start,
                rows: old.rows.clone(),
            };
            held.rows[0].inline = Some(243);
            host.pane.held = Rc::new(held);
            shell.settle_list_sizes(window, cx);
            assert_eq!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .view
                    .mark_revision(2),
                revision
            );
        })
        .unwrap();
}
