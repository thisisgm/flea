//! Frame readiness through the real Trash window and IPC reads served before the next frame.
use super::join_tests::{answer, call, draw, root, shell};
use crate::list::bench::real_cx;
use std::time::Duration;

// Production GPUI draws only on the platform's frame callback, so both reads run inside the reply's own update, with no draw between.
#[test]
fn review_u32_r2_idle_before_the_next_frame() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
        shell.trash_action("openTrash", window, cx);
    });
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity"},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity"}]"#,
    );
    let (raw, centre) = shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        let id = host.pane.read(cx).view().request_id;
        let line = format!(
            r#"{{"t":"trashbrowse","id":{id},"op":"summary","ok":true, "bytes":11,"partial":false}}"#
        );
        shell.receive(
            host.side,
            crate::backend::Event::TrashLine(line),
            window,
            cx,
        );
        shell.flush_trash(window, cx);
        let mut read = |name: &str, arguments: Vec<String>| {
            let (answer, reply) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments,
                    },
                    answer,
                },
                window,
                cx,
            );
            reply
                .try_recv()
                .unwrap()
                .reply
                .stdout
                .trim_end()
                .to_string()
        };
        (
            read("trashState", vec![]),
            read("trashRowCentre", vec!["0".into()]),
        )
    });
    let state = flea::jsondoc::parse(&raw).unwrap();
    let idle = state.get("opened").and_then(flea::jsondoc::Json::as_bool) == Some(true)
        && state.get("total").and_then(flea::jsondoc::Json::as_f64) == Some(2.)
        && state.get("busy").and_then(flea::jsondoc::Json::as_bool) == Some(false);
    assert!(
        !idle || !centre.is_empty(),
        "idle before the first Trash frame reads an empty centre: {raw}"
    );
    draw(&mut cx, window);
    assert!(!call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
    let after = super::join_tests::state(&mut cx, window);
    assert_eq!(
        after.get("busy").and_then(flea::jsondoc::Json::as_bool),
        Some(false),
        "the drawn frame settles the same state to idle"
    );
}

// Review r3 proof: the thumbnail settle timer pumps an unchanged snapshot 70 ms after the rows; production can serve an IPC call before the next frame.
#[test]
fn review_u32_r3_a_settle_pump_withholds_a_drawn_centre() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    let idle = super::join_tests::state(&mut cx, window);
    let before = call(&mut cx, window, "trashRowCentre", &["0"]);
    let snapshot = |cx: &mut gpui::TestAppContext| {
        shell(cx, window, |shell, _, cx| {
            let pane = shell.trash.as_ref().unwrap().pane.read(cx);
            let paint = pane.paint.borrow();
            (
                paint.committed,
                paint.revision,
                paint.lines.clone(),
                paint.scroll_y,
            )
        })
    };
    let (committed, revision, lines, scroll) = snapshot(&mut cx);
    cx.executor().advance_clock(Duration::from_millis(70));
    cx.run_until_parked();
    let (committed_after, revision_after, lines_after, scroll_after) = snapshot(&mut cx);
    eprintln!(
        "REVIEW idle busy={:?} centre={before:?}; settle timer at 70 ms: committed {committed} to {committed_after}, revision {revision} to {revision_after}, lines same={}, scroll {scroll} to {scroll_after}",
        idle.get("busy"),
        lines == lines_after
    );
    assert_eq!(
        committed_after, committed,
        "an unchanged settle timer preserves the drawn layout"
    );
    assert_eq!(
        (revision, &lines, scroll),
        (revision_after, &lines_after, scroll_after)
    );
    let (raw, after) = shell(&mut cx, window, |shell, window, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.pump(cx));
        let mut read = |name: &str, arguments: Vec<String>| {
            let (answer, reply) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments,
                    },
                    answer,
                },
                window,
                cx,
            );
            reply
                .try_recv()
                .unwrap()
                .reply
                .stdout
                .trim_end()
                .to_string()
        };
        (
            read("trashState", vec![]),
            read("trashRowCentre", vec!["0".into()]),
        )
    });
    eprintln!("REVIEW after an unchanged pump, before the next frame: trashRowCentre={after:?} trashState={raw}");
    assert_eq!(
        idle.get("busy").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    let state = flea::jsondoc::parse(&raw).unwrap();
    assert_eq!(
        state.get("busy").and_then(flea::jsondoc::Json::as_bool),
        Some(false),
        "an unchanged pump preserves idle readiness"
    );
    assert_eq!(
        after, before,
        "trash_wait read idle with a centre, then an unchanged settle pump withholds the same centre until a frame"
    );
}
