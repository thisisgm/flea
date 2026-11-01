//! Actual lender slots through the production thumbnailPolicyState route.
use super::*;
use crate::ipc::server::Event as IpcEvent;
use flea::ipc::wire::Request;

fn state(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> Json {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, reply) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                IpcEvent::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "thumbnailPolicyState".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            let reply = reply.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0);
            flea::jsondoc::parse(&reply.stdout).unwrap()
        })
        .unwrap()
}

#[gpui::test]
fn policy_reader_cache_raw_file_survives_decode_and_failure_in_every_view(cx: &mut TestAppContext) {
    for view in [View::List, View::Dual, View::Grid, View::Columns] {
        let (handle, mut lines) = open(cx, view, "{}");
        first_settle(cx, handle, &mut lines);
        assert_eq!(
            state(cx, handle).get("files").unwrap().get("1"),
            Some(&Json::Null)
        );
        assert!(deliver(cx, handle, &thumbed(1, PICTURE)));
        assert_eq!(
            state(cx, handle)
                .get("files")
                .unwrap()
                .get("1")
                .and_then(Json::as_str),
            Some(PICTURE)
        );
        cx.run_until_parked();
        step(cx, handle);
        assert!(read(cx, handle, |shell| pictures(shell)
            .ready(&shell.panes[0].as_ref().unwrap().pane.held, 1)));
        let bad = concat!(env!("CARGO_MANIFEST_DIR"), "/missing-thumbnail-policy.png");
        assert!(deliver(cx, handle, &thumbed(2, bad)));
        cx.run_until_parked();
        step(cx, handle);
        assert_eq!(
            state(cx, handle)
                .get("files")
                .unwrap()
                .get("2")
                .and_then(Json::as_str),
            Some(bad)
        );
        assert_eq!(
            read(cx, handle, |shell| pictures(shell)
                .status(&shell.panes[0].as_ref().unwrap().pane.held, 2)),
            3
        );
    }
}

#[gpui::test]
fn policy_reader_cache_pending_class_and_current_identity_guards(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    step(cx, handle);
    drained(&mut lines);
    deliver(
        cx,
        handle,
        r#"{"t":"fsinfo","path":"/files","class":"network"}"#,
    );
    step(cx, handle);
    settle(cx, handle, 120);
    let state_now = state(cx, handle);
    assert_eq!(
        state_now
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some("cache-asked")
    );
    assert!(state_now
        .get("pending")
        .unwrap()
        .as_array()
        .unwrap()
        .contains(&Json::Num("1".into())));
    deliver(cx, handle, &thumbed(1, PICTURE));
    cx.run_until_parked();
    step(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            let mut rows = held.rows.clone();
            rows[1].mtime += 1;
            *held = Rc::new(crate::list::pane::Held { rows, ..**held });
        })
        .unwrap();
    assert!(
        state(cx, handle).get("files").unwrap().get("1").is_none(),
        "changed source cannot retain a cache identity"
    );
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().path = "/foreign".into()
        })
        .unwrap();
    assert_eq!(state(cx, handle).get("files").unwrap(), &Json::Obj(vec![]));
}

#[gpui::test]
fn policy_reader_tagged_preview_reply_does_not_answer_unrelated_cache_request(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(
        cx,
        handle,
        &format!(
            r#"{{"t":"thumbed","row":1,"file":"{PICTURE}","thumbToken":"foreign","thumbPath":"/files/photo.png","thumbListing":1}}"#
        ),
    );
    assert_eq!(
        state(cx, handle).get("files").unwrap().get("1"),
        Some(&Json::Null)
    );
    deliver(cx, handle, &thumbed(1, PICTURE));
    assert_eq!(
        state(cx, handle)
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some(PICTURE)
    );
}

#[gpui::test]
fn policy_reader_cache_respects_immediate_off_listing_and_disconnected_owner(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(1, PICTURE));
    assert_eq!(
        state(cx, handle)
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some(PICTURE)
    );
    handle
        .update(cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"preview":{"thumbnails":"off"}}"#).unwrap()
        })
        .unwrap();
    assert_eq!(state(cx, handle).get("files"), Some(&Json::Obj(vec![])));
    handle
        .update(cx, |shell, _, _| {
            shell.launch.settings.data = Json::Obj(vec![])
        })
        .unwrap();
    assert_eq!(
        state(cx, handle)
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some(PICTURE)
    );
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().backend = None
        })
        .unwrap();
    assert_eq!(state(cx, handle).get("files"), Some(&Json::Obj(vec![])));
}

#[gpui::test]
fn policy_reader_terminal_raw_values_preserve_request_planning_and_source_identity(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    let asked_before = first_settle(cx, handle, &mut lines);
    let requests_before = asked(&asked_before);
    assert!(requests_before.contains(&1));
    let counts = read(cx, handle, |shell| pictures(shell).counters());
    deliver(cx, handle, &thumbed(1, ""));
    assert_eq!(
        state(cx, handle)
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some("")
    );
    assert!(!state(cx, handle)
        .get("pending")
        .unwrap()
        .as_array()
        .unwrap()
        .contains(&Json::Num("1".into())));
    step(cx, handle);
    settle(cx, handle, 120);
    assert!(
        !asked(&drained(&mut lines)).contains(&1),
        "a refusal remains answered for scheduling"
    );
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).counters().asked),
        counts.asked
    );
    handle
        .update(cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            let mut rows = held.rows.clone();
            rows[1].mtime += 1;
            *held = Rc::new(crate::list::pane::Held { rows, ..**held });
        })
        .unwrap();
    assert!(state(cx, handle).get("files").unwrap().get("1").is_none());
    step(cx, handle);
    settle(cx, handle, 120);
    assert!(
        !asked(&drained(&mut lines)).contains(&1),
        "terminal observation must not change Slot::identity/stale planning"
    );
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().path = "/foreign".into()
        })
        .unwrap();
    assert_eq!(state(cx, handle).get("files"), Some(&Json::Obj(vec![])));
}

#[gpui::test]
fn policy_reader_cache_miss_is_raw_cache_missed_without_a_decoder_or_new_request(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    step(cx, handle);
    drained(&mut lines);
    deliver(
        cx,
        handle,
        r#"{"t":"fsinfo","path":"/files","class":"network"}"#,
    );
    step(cx, handle);
    settle(cx, handle, 120);
    let requests = drained(&mut lines);
    assert!(requests
        .iter()
        .any(|line| line.contains(r#""cacheOnly":true"#)));
    let counts = read(cx, handle, |shell| pictures(shell).counters());
    deliver(cx, handle, &thumbed(1, ""));
    assert_eq!(
        state(cx, handle)
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some("cache-missed")
    );
    assert!(!state(cx, handle)
        .get("pending")
        .unwrap()
        .as_array()
        .unwrap()
        .contains(&Json::Num("1".into())));
    step(cx, handle);
    settle(cx, handle, 120);
    assert!(!asked(&drained(&mut lines)).contains(&1));
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).counters().asked),
        counts.asked
    );
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).counters().decoded),
        counts.decoded
    );

    handle
        .update(cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            let mut rows = held.rows.clone();
            rows[1].mtime += 1;
            *held = Rc::new(crate::list::pane::Held { rows, ..**held });
        })
        .unwrap();
    assert!(state(cx, handle).get("files").unwrap().get("1").is_none());
    step(cx, handle);
    settle(cx, handle, 120);
    assert!(
        !asked(&drained(&mut lines)).contains(&1),
        "observation cannot change stale planning"
    );
    handle
        .update(cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"preview":{"thumbNetwork":true}}"#).unwrap();
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let generated = drained(&mut lines);
    assert_eq!(
        asked(&generated),
        vec![1],
        "the original class switch requests the miss again"
    );
    assert!(generated
        .iter()
        .any(|line| line.contains(r#""cacheOnly":false"#)));
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).counters().asked),
        counts.asked + 1
    );
    deliver(cx, handle, &thumbed(1, ""));
    assert_eq!(
        state(cx, handle)
            .get("files")
            .unwrap()
            .get("1")
            .and_then(Json::as_str),
        Some(""),
        "only a genuine matching outcome authenticates the new source facts"
    );
}
