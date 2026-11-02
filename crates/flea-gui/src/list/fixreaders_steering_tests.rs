//! FIX-HARNESS-READERS: dirSizeState and dirSizeCancels over the steering fixture.
use super::*;

// The scroll join_steering_tests.rs uses to leave every waiting walk behind the viewport.
const PAST_THE_WALKS: f32 = 31. * 30.;

// EY-446: tests/ui-dirsortstale.sh reads dirSizeState as Pane.qml's JSON.stringify(dirSizeState); GPUI answers the pane's own walks.
#[test]
fn fixreaders_ey446_dir_size_state_reads_the_pane_walks() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    assert_eq!(
        ask(&mut cx, handle, "dirSizeState", &[]),
        r#"{"file":{},"order":[]}"#
    );
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    let asked = command.get("rows").and_then(Json::as_array).unwrap().len();
    let state = jsondoc::parse(&ask(&mut cx, handle, "dirSizeState", &[])).unwrap();
    assert_eq!(
        state.get("file").and_then(|file| file.get("0")),
        Some(&Json::Null),
        "an asked row waits as null"
    );
    let replies: Vec<_> = (0..asked).map(|_| next(&mut cx, handle)).collect();
    for event in replies {
        receive(&mut cx, handle, event);
    }
    frame(&mut cx, handle);
    let state = jsondoc::parse(&ask(&mut cx, handle, "dirSizeState", &[])).unwrap();
    let answer = |row: &str, key: &str| {
        state
            .get("file")
            .and_then(|file| file.get(row))
            .and_then(|size| size.get(key))
            .cloned()
    };
    assert_eq!(answer("0", "bytes"), Some(Json::Num("243".into())));
    assert_eq!(answer("0", "partial"), Some(Json::Bool(false)));
    assert_eq!(answer("1", "partial"), Some(Json::Bool(true)));
    assert_eq!(
        state.get("order").and_then(Json::as_array).map(<[_]>::len),
        Some(asked)
    );
}

// BOAT-LOOKS pf.dcw-rest.list-folder-sizes: a scroll that drops waiting walks is counted, beside dirSizeRequests.
#[test]
fn fixreaders_dir_size_cancels_count_a_scroll_cancel() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "dirSizeCancels", &[]), "0");
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    // The answers stay unread, so every walk is still waiting when the scroll lands.
    let asked = command.get("rows").and_then(Json::as_array).unwrap().len();
    let _stale: Vec<_> = (0..asked).map(|_| next(&mut cx, handle)).collect();
    handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = PAST_THE_WALKS
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(
        request(&mut cx, handle).get("c").and_then(Json::as_str),
        Some("dirsizecancel")
    );
    assert_eq!(ask(&mut cx, handle, "dirSizeCancels", &[]), "1");
    assert_eq!(ask(&mut cx, handle, "dirSizeRequests", &[]), "1");
}

// A cancel the stopped backend refused is not counted: dirSizeCancels reads only cancels the backend took.
#[test]
fn fixreaders_dir_size_cancels_skip_a_refused_cancel() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    classify(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    let asked = command.get("rows").and_then(Json::as_array).unwrap().len();
    let _stale: Vec<_> = (0..asked).map(|_| next(&mut cx, handle)).collect();
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.backend.as_ref().unwrap().quit();
            host.pane.scroll_y = PAST_THE_WALKS
        })
        .unwrap();
    frame(&mut cx, handle);
    // The scroll did drop every waiting walk, so the cancel was tried and refused, not skipped.
    assert_eq!(
        ask(&mut cx, handle, "dirSizeState", &[]),
        r#"{"file":{},"order":[]}"#
    );
    assert_eq!(ask(&mut cx, handle, "dirSizeCancels", &[]), "0");
}
