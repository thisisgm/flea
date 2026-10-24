use crate::{
    backend::{Backend, Event as BackendEvent},
    pane::swap::{Options, CAP_MS},
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

#[gpui::test]
fn rate_atomic_shell_clock_follows_executor_time(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    let shell = window.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let before = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
    cx.executor().advance_clock(Duration::from_millis(2500));
    let after = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
    assert_eq!(after - before, 2500);
}

pub(super) fn root(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<(usize, usize)>,
) {
    let (backend, requests) = Backend::test_channel();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| {
            shell.follow_frame_feedback(window, cx);
            window.focus(&shell.focus, cx);
        });
        Root { shell }
    });
    cx.update(|_, cx| cx.set_reduce_motion(true));
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    (shell, cx, requests)
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx);
        assert_eq!(
            window.presented_frames(),
            None,
            "headless draws are not presentations"
        );
    });
}

pub(super) fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, reply: Reply) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::Reply(reply), window, cx)
        })
    });
}

fn navigate(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str, supersede: bool) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.navigate(
                0,
                path.into(),
                None,
                Options {
                    supersede,
                    ..Options::default()
                },
                window,
                cx
            ));
        })
    });
    cx.run_until_parked();
}

pub(super) fn rows(listing: usize) -> Reply {
    Reply::Rows {
        start: 0,
        listing,
        rows: ["first", "second"]
            .into_iter()
            .map(|name| RowFacts {
                name: name.into(),
                ..Default::default()
            })
            .collect(),
    }
}

fn coherent(shell: &Entity<Shell>, cx: &mut VisualTestContext, landed: bool) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.path, if landed { "/next" } else { "/files" });
        assert_eq!(host.pane.path(), host.path);
        assert_eq!(shell.path_labels[0].as_ref(), host.path);
        assert_eq!(host.pane.held.total, if landed { 2 } else { 1000 });
        assert_eq!(host.pane.cursor, if landed { 0 } else { 4 });
        assert_eq!(
            host.selected.as_slice(),
            if landed { &[][..] } else { &[2, 4][..] }
        );
        assert_eq!(
            host.status.as_ref(),
            if landed { "2 items" } else { "1,000 items" }
        );
        if landed {
            assert_eq!(host.pane.held.rows[0].name, "first");
        }
        assert!(host.pane.swap.mark_since.is_none());
    });
}

#[gpui::test]
fn rate_atomic_full_shell_coherence_across_frame_schedules(cx: &mut TestAppContext) {
    let mut schedules: Vec<Vec<u64>> = [30, 60, 90, 120, 144, 165, 240]
        .map(|hz| (0..=24).map(|frame| frame * 1_000_000_000 / hz).collect())
        .into();
    schedules.push(vec![
        0,
        3_000_000,
        19_000_000,
        43_000_000,
        110_000_000,
        2_610_000_000,
    ]);
    for schedule in schedules {
        let (shell, cx, _requests) = root(cx);
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.changed(&shell.locale);
            host.pane.cursor = 4;
            host.selected = vec![2, 4];
        });
        navigate(&shell, cx, "/next", false);
        let mut events: Vec<_> = schedule.into_iter().map(|at| (at, 2)).collect();
        events.extend([(5_000_000, 0), (45_000_000, 1)]);
        events.sort();
        let mut previous = 0;
        let mut landed = false;
        for (at, event) in events {
            cx.executor()
                .advance_clock(Duration::from_nanos(at - previous));
            cx.run_until_parked();
            previous = at;
            match event {
                0 => reply(
                    &shell,
                    cx,
                    Reply::Listed {
                        path: "/next".into(),
                        total: 2,
                    },
                ),
                1 => {
                    reply(&shell, cx, rows(3));
                    landed = true;
                }
                _ => draw(cx),
            }
            coherent(&shell, cx, landed);
        }
    }
}

#[gpui::test]
fn rate_atomic_unpresented_loading_never_arms_minimum_or_waits_forever(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = root(cx);
    navigate(&shell, cx, "/next", false);
    cx.executor().advance_clock(Duration::from_millis(CAP_MS));
    cx.run_until_parked();
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let pane = shell.pane(0).unwrap();
        assert!(pane.swap.fell_back);
        assert!(pane.swap.mark.shown());
        assert!(
            pane.swap.mark_since.is_none(),
            "painting is not visible acknowledgment"
        );
        assert!(!pane.swap.observed.borrow().available);
    });
    cx.executor().advance_clock(Duration::from_millis(2500));
    cx.run_until_parked();
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/next".into(),
            total: 2,
        },
    );
    reply(&shell, cx, rows(3));
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(!shell.pane(0).unwrap().busy());
        assert_eq!(shell.pane(0).unwrap().path(), "/next");
    });
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: "swapState".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    let answer = receive.try_recv().unwrap().reply;
    assert_eq!(answer.code, 0);
    let data = flea::jsondoc::parse(&answer.stdout).unwrap();
    for (legacy, paint) in [
        ("blankFrames", "paintBlankFrames"),
        ("loadingFrames", "paintLoadingFrames"),
    ] {
        assert_eq!(legacy_integer(&data, legacy), legacy_integer(&data, paint));
    }
    assert_eq!(data.get("blinkBacks"), Some(&Json::Null));
    assert_eq!(
        data.get("presentation")
            .unwrap()
            .get("coverage")
            .and_then(Json::as_str),
        Some("unavailable")
    );
}

fn legacy_integer(data: &Json, field: &str) -> usize {
    let Some(Json::Num(value)) = data.get(field) else {
        panic!("swapState legacy {field} must remain an unsigned integer");
    };
    value
        .parse()
        .expect("legacy paint count must be an integer")
}

fn adapter_swap_state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    use crate::ipc::{pane::PaneAdapter, Adapter};
    shell.read_with(cx, |shell, _| {
        let answer = PaneAdapter {
            pane: shell.pane(0).unwrap(),
            catalog: &shell.locale.catalog,
        }
        .call("swapState", &[])
        .unwrap();
        assert_eq!(answer.code, 0);
        flea::jsondoc::parse(&answer.stdout).unwrap()
    })
}

#[gpui::test]
fn rate_atomic_legacy_paint_counters_stay_numeric_without_presentation(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = root(cx);
    navigate(&shell, cx, "/next", false);
    cx.executor().advance_clock(Duration::from_millis(CAP_MS));
    cx.run_until_parked();
    draw(cx);
    let before = adapter_swap_state(&shell, cx);
    let before_loading = legacy_integer(&before, "loadingFrames");
    assert!(before_loading > 0, "real Loading paint must be observed");
    shell.update(cx, |_, cx| cx.notify());
    draw(cx);
    let after = adapter_swap_state(&shell, cx);
    let after_loading = legacy_integer(&after, "loadingFrames");
    // tests/ui-noblank.sh:136-139 requires numeric equality, comparison and subtraction.
    assert_eq!(
        legacy_integer(&after, "blankFrames"),
        legacy_integer(&before, "blankFrames")
    );
    assert!(after_loading > before_loading);
    assert!(after_loading - before_loading > 0);
    for data in [&before, &after] {
        for (legacy, paint) in [
            ("blankFrames", "paintBlankFrames"),
            ("loadingFrames", "paintLoadingFrames"),
        ] {
            assert_eq!(legacy_integer(data, legacy), legacy_integer(data, paint));
        }
        for (field, value) in [
            ("legacyFrameCounterSemantics", "paint-invocations"),
            ("legacyFrameCounterCoverage", "paint-only"),
        ] {
            assert_eq!(data.get(field).and_then(Json::as_str), Some(value));
        }
        assert_eq!(data.get("blinkBacks"), Some(&Json::Null));
        let presentation = data.get("presentation").unwrap();
        assert_eq!(
            presentation.get("coverage").and_then(Json::as_str),
            Some("unavailable")
        );
        assert_eq!(presentation.get("exhaustive"), Some(&Json::Bool(false)));
        for field in [
            "frames",
            "blankFrames",
            "loadingFrames",
            "blinkBacks",
            "nativeClockId",
            "nativePresentedNs",
        ] {
            assert_eq!(
                presentation.get(field),
                Some(&Json::Null),
                "missing observer: {field} is unavailable, not zero"
            );
        }
    }
    shell.read_with(cx, |shell, _| {
        assert!(shell.pane(0).unwrap().swap.mark_since.is_none());
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.close(window, cx)));
}

#[gpui::test]
fn rate_atomic_superseded_rows_and_input_keep_the_current_snapshot(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = root(cx);
    navigate(&shell, cx, "/old", false);
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/old".into(),
            total: 2,
        },
    );
    navigate(&shell, cx, "/next", true);
    reply(&shell, cx, rows(3));
    draw(cx);
    assert_eq!(
        shell.read_with(cx, |s, _| s.pane(0).unwrap().path().to_owned()),
        "/files"
    );
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/next".into(),
            total: 2,
        },
    );
    reply(&shell, cx, rows(4));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("down").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            )
        })
    });
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.path, "/next");
        assert_eq!(host.pane.cursor, 1);
        assert_eq!(host.list.borrow().drawn_cursor, Some(1));
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.close(window, cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(2500));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(shell.presentation.observer.is_none())
    });
}
