//! Real watcher acknowledgements and populated-row paint checks for the mounted font test.
use super::*;
use std::future::Future;

/// Waits for the named subscription (OEM or settings textSize) to deliver base, failing after 3 s of real time.
pub(super) fn wait_for_base(
    cx: &mut TestAppContext,
    handle: Handle,
    base: u32,
    subscription: &str,
) {
    let shell = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let (send, received) = async_channel::bounded(1);
    let observer = cx.update(|cx| {
        cx.observe(&shell, move |shell, cx| {
            if f32::from(shell.read(cx).active_typeface().base) == base as f32 {
                let _ = send.try_send(());
            }
        })
    });
    if edit(cx, handle, |shell, _, _| {
        f32::from(shell.active_typeface().base)
    }) != base as f32
    {
        // The test scheduler parks with no deadline, so a real-time sleeper ends a lost notification.
        let (expire, expired) = async_channel::bounded::<()>(1);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let _ = expire.try_send(());
        });
        let executor = cx.executor();
        executor.allow_parking();
        let delivered = cx.foreground_executor().block_test(async {
            let mut delivered = std::pin::pin!(received.recv());
            let mut expired = std::pin::pin!(expired.recv());
            std::future::poll_fn(|task| {
                if let std::task::Poll::Ready(sent) = delivered.as_mut().poll(task) {
                    return std::task::Poll::Ready(sent.is_ok());
                }
                expired.as_mut().poll(task).map(|_| false)
            })
            .await
        });
        executor.forbid_parking();
        assert!(
            delivered,
            "the actual {subscription} subscription did not deliver {base}"
        );
    }
    drop(observer);
    assert_eq!(
        edit(cx, handle, |shell, _, _| {
            f32::from(shell.active_typeface().base)
        }),
        base as f32,
        "the actual {subscription} subscription left base {base} unapplied"
    );
}

pub(super) fn populate(cx: &mut TestAppContext, handle: Handle, listing: usize) {
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/font-rows".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/font-rows".into(),
                total: 8,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing,
                rows: (0..8)
                    .map(|at| flea::protocol::RowFacts {
                        name: format!("font-row-{at}.txt"),
                        ..Default::default()
                    })
                    .collect(),
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
    });
    settle(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.state.kind, crate::pane::states::Kind::Ready);
        assert_eq!(host.list.borrow().drawn.listing, listing);
        assert!(host
            .list
            .borrow()
            .visible()
            .iter()
            .any(|row| row.prepared.is_some()));
    });
}

pub(super) fn assert_rows(cx: &mut TestAppContext, handle: Handle, base: u32) {
    let row_height = match base {
        12 => 26.,
        14 => 31.,
        16 => 35.,
        _ => unreachable!(),
    };
    let previous = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().list.borrow().drawn.frames
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        assert!(
            list.drawn.frames > previous,
            "populated List must paint after delivery"
        );
        assert_eq!(
            host.pane.row_height(),
            row_height,
            "subscriber left Pane metrics stale at {base}"
        );
        assert_eq!(
            list.drawn.metrics.row, row_height,
            "painted List row metrics at {base}"
        );
        assert!(
            list.visible()
                .iter()
                .filter(|row| row.prepared.is_some())
                .count()
                >= 2
        );
        let input = host.input.drawn.expect("populated List input frame");
        assert_eq!(input.frame.row, row_height);
        assert_eq!(input.frame.listing, host.pane.held.listing);
    });
    let rows = [0, 1].map(|at| {
        edit(cx, handle, |shell, window, cx| {
            let (answer, received) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "rowRect".into(),
                        arguments: vec![at.to_string()],
                    },
                    answer,
                },
                window,
                cx,
            );
            let reply = received.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0);
            // rowRect stdout at base 14, row 0: "192 54 1714 31\n" (x y width height).
            let rect: Vec<f32> = reply
                .stdout
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            assert_eq!(rect.len(), 4, "real painted rowRect must be populated");
            assert!(rect[2] > 0.);
            assert_eq!(rect[3], row_height);
            rect
        })
    });
    assert_eq!(rows[1][1] - rows[0][1], row_height);
}
