//! Test-only mounted Shell fixture. All receipts/outcomes pass through the real Window backend.
use crate::{
    backend::Backend,
    pane::{
        presentation::{Content, Observed},
        states::Kind,
        swap::{Options, CAP_MS},
    },
    state::{tests::shell_with, Shell},
    window::{
        rate_atomic_tests::{reply, rows},
        root::Root,
    },
};
use flea::{jsondoc::Json, protocol::Reply};
use gpui::{
    AppContext, Entity, FrameOutcome, FrameTerminal, PresentationTime, RequestFrameOptions,
    TestAppContext, VisualTestContext,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

pub(super) struct Receipt {
    pub id: u64,
    pub draw: u64,
    pub at: Instant,
    pub content: Rc<Content>,
    pub observed: Rc<RefCell<Observed>>,
    pub loading: bool,
}

pub(super) fn mount(
    cx: &mut TestAppContext,
    moving: bool,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<(usize, usize)>,
) {
    let (backend, requests) = Backend::test_channel();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.enable_frame_feedback();
    cx.update(|window, cx| {
        cx.set_reduce_motion(!moving);
        shell.update(cx, |shell, cx| {
            shell.follow_frame_feedback(window, cx);
            window.focus(&shell.focus, cx);
            assert!(shell.presentation.observer.is_some());
            cx.notify();
        });
    });
    cx.run_until_parked();
    (shell, cx, requests)
}

pub(super) fn navigate(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    supersede: bool,
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.navigate(
                0,
                path.into(),
                None,
                Options {
                    supersede,
                    ..Default::default()
                },
                window,
                cx
            ));
        })
    });
    cx.run_until_parked();
}

pub(super) fn step(cx: &mut VisualTestContext, duration: Duration) {
    cx.executor().advance_clock(duration);
    cx.run_until_parked();
}

pub(super) fn until(cx: &mut VisualTestContext, at: Instant) {
    let now = cx.executor().now();
    assert!(at >= now);
    step(cx, at.duration_since(now));
}

pub(super) fn submit(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Receipt {
    let before = cx.update(|window, _| window.presented_frames().unwrap().committed);
    cx.simulate_frame_request(RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    let id = cx.update(|window, _| {
        assert_eq!(window.current_draw_id(), None);
        window.presented_frames().unwrap().committed
    });
    assert!(
        id > before,
        "the actual backend must issue a successful receipt"
    );
    shell.read_with(cx, |shell, _| {
        let frame = shell
            .presentation
            .pending
            .iter()
            .find(|frame| frame.id == id)
            .expect("Shell must associate the actual successful Window receipt");
        let pane = frame.draw.panes[0].as_ref().expect("mounted side zero");
        assert!(frame.draw.id > 0);
        assert!(frame.visible);
        Receipt {
            id,
            draw: frame.draw.id,
            at: frame.at,
            content: pane.content.clone(),
            observed: pane.observed.clone(),
            loading: pane.loading,
        }
    })
}

pub(super) fn terminal(cx: &mut VisualTestContext, receipt: &Receipt, outcome: FrameOutcome) {
    cx.simulate_frame_terminal(FrameTerminal {
        frame_id: receipt.id,
        outcome,
    });
    cx.run_until_parked();
}

pub(super) fn presented() -> FrameOutcome {
    FrameOutcome::Presented(PresentationTime {
        timestamp: None,
        clock_id: None,
    })
}

pub(super) fn ready_rows(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    listing: usize,
) {
    reply(
        shell,
        cx,
        Reply::Listed {
            path: path.into(),
            total: 2,
        },
    );
    reply(shell, cx, rows(listing));
    cx.run_until_parked();
}

pub(super) fn loading(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    schedule: &[Duration],
) -> Receipt {
    navigate(shell, cx, "/next", false);
    for duration in schedule.iter().cycle() {
        step(cx, *duration);
        let frame = submit(shell, cx);
        if frame.loading {
            assert!(shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now()) >= CAP_MS);
            whole_loading(shell, cx, &frame);
            return frame;
        }
        terminal(cx, &frame, presented());
    }
    unreachable!()
}

pub(super) fn whole_loading(shell: &Entity<Shell>, cx: &mut VisualTestContext, frame: &Receipt) {
    assert!(frame.loading);
    assert_eq!(frame.content.path.as_ref(), "/files");
    assert_eq!(frame.content.held.total, 0);
    assert!(frame.content.held.rows.is_empty());
    assert!(frame.content.painted_rows.is_empty());
    assert_eq!(frame.content.drawn_cursor, None);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.pane.busy());
        assert_eq!(host.pane.state.kind, Kind::Loading);
        assert_eq!(host.path, "/files");
        assert_eq!(shell.path_labels[0].as_ref(), host.path);
        assert_eq!(host.pane.held.total, 0);
        assert_eq!(host.pane.cursor, 0);
        assert!(host.selected.is_empty());
        assert_eq!(frame.content.status.as_deref(), Some(host.status.as_ref()));
        assert_eq!(
            host.status.as_ref(),
            shell.locale.catalog.tr("pane.reading")
        );
    });
    unknowns(&frame.observed.borrow());
}

pub(super) fn whole_ready(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    frame: &Receipt,
    path: &str,
) {
    assert!(!frame.loading);
    assert_eq!(frame.content.path.as_ref(), path);
    assert_eq!(frame.content.held.total, 2);
    assert_eq!(
        frame
            .content
            .held
            .rows
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert!(frame.content.painted_rows.contains(&0));
    assert!(frame.content.painted_rows.contains(&1));
    assert_eq!(frame.content.drawn_cursor, Some(0));
    assert!(frame.content.selected.is_empty());
    assert_eq!(frame.content.status.as_deref(), Some("2 items"));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.state.kind, Kind::Ready);
        assert_eq!(host.path, path);
        assert_eq!(host.pane.path(), path);
        assert_eq!(shell.path_labels[0].as_ref(), path);
        assert!(Rc::ptr_eq(&host.pane.held, &frame.content.held));
        assert_eq!(host.pane.cursor, 0);
        assert!(host.selected.is_empty());
        assert_eq!(host.status.as_ref(), "2 items");
    });
    let json = frame.content.json();
    assert_eq!(json.get("selection"), Some(&Json::Null));
    assert_eq!(
        json.get("selectionCoverage").and_then(Json::as_str),
        Some("uncomposed")
    );
    unknowns(&frame.observed.borrow());
}

pub(super) fn unknowns(observed: &Observed) {
    let json = observed.json();
    for field in ["nativePresentedNs", "nativeClockId", "blinkBacks"] {
        assert_eq!(json.get(field), Some(&Json::Null), "{field}");
    }
    assert_eq!(json.get("exhaustive"), Some(&Json::Bool(false)));
    assert_eq!(
        json.get("chronology").and_then(Json::as_str),
        Some("unavailable")
    );
}

pub(super) fn close(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.close(window, cx);
        })
    });
    shell.read_with(cx, |shell, _| {
        assert!(shell.presentation.observer.is_none());
        assert!(shell.presentation.pending.is_empty());
    });
}

pub(super) fn schedules() -> Vec<Vec<Duration>> {
    let mut all = [30, 60, 90, 120, 144, 165, 240]
        .into_iter()
        .map(|hz| vec![Duration::from_nanos(1_000_000_000 / hz)])
        .collect::<Vec<_>>();
    all.push([3, 19, 43, 55, 8, 110].map(Duration::from_millis).into());
    all
}
