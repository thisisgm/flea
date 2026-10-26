use super::*;

fn identities(shell: &crate::state::Shell) -> Vec<u64> {
    shell.strip.tabs[0]
        .tabs
        .as_ref()
        .unwrap()
        .items
        .iter()
        .map(|tab| tab.identity)
        .collect()
}

#[gpui::test]
fn rev_u25_gesture_without_a_lift_offers_the_previous_lifts_payload(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            shell.end_tab_drag(window, cx);
            assert!(
                shell.strip.bar.out_payload.is_empty(),
                "the lift retired its offer"
            );
            assert!(
                shell.tab_native_payload(cx).is_none(),
                "a press on New tab or the blank strip must not offer the retired lift"
            );
            assert!(!shell.strip.bar.out_active);
            assert!(!cx
                .windows()
                .into_iter()
                .any(|handle| shell.strip.catchers.contains(handle)));
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn u25_a_finished_native_drag_retires_its_payload_until_the_ack(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            assert!(shell.tab_native_payload(cx).is_some());
            shell.strip.bar.out_finished(now);
            assert!(shell.strip.bar.out_payload.is_empty());
            assert!(
                !shell.strip.bar.lifts.is_empty(),
                "the lift still waits for its ack"
            );
            assert!(shell.tab_native_payload(cx).is_none());
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn u25_a_close_during_an_internal_drag_moves_the_held_tab(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for _ in 0..3 {
                key(shell, "t", window, cx);
            }
            shell.tab_tap_hit(crate::tabs::bar::Hit::Tab(0), false, window, cx);
            let before = identities(shell);
            assert_eq!(shell.strip.tabs[0].current(), 0);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 2, now));
            shell.strip.bar.drag_moved(0., 4);
            key(shell, "w", window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 3);
            assert_eq!(
                shell.strip.bar.drag_from,
                Some(1),
                "the held tab keeps its place in the strip"
            );
            shell.end_tab_drag(window, cx);
            let after = identities(shell);
            assert_eq!(after[0], before[2], "{before:?} -> {after:?}");
        })
    });
}

#[gpui::test]
fn u25_closing_the_held_tab_itself_drops_nothing(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for _ in 0..3 {
                key(shell, "t", window, cx);
            }
            let current = shell.strip.tabs[0].current();
            let before = identities(shell);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, current, now));
            shell.strip.bar.drag_moved(0., 4);
            key(shell, "w", window, cx);
            assert!(shell.strip.bar.drag_from.is_none());
            shell.end_tab_drag(window, cx);
            let mut left = before.clone();
            left.remove(current);
            assert_eq!(identities(shell), left, "no other tab moved");
        })
    });
}

#[test]
fn u25_strip_counters_hold_swap_and_blank_frames() {
    use crate::tabs::held::{Painted, StripFrames};
    use gpui::{FrameFeedbackEvent, FrameOutcome, FrameTerminal, PresentationTime};
    let mut frames = StripFrames::default();
    frames.follow(true);
    frames.staged.set(Some(Painted {
        draw: 1,
        version: 1,
        holding: true,
        complete: true,
    }));
    frames.feedback(FrameFeedbackEvent::Submitted {
        draw_id: 1,
        frame_id: 1,
    });
    assert!(
        frames.json().contains("\"presentedFrames\":0"),
        "submission is not presentation"
    );
    let presented = || {
        FrameOutcome::Presented(PresentationTime {
            timestamp: None,
            clock_id: None,
        })
    };
    frames.feedback(FrameFeedbackEvent::Terminal(FrameTerminal {
        frame_id: 1,
        outcome: presented(),
    }));
    frames.follow(false);
    frames.staged.set(Some(Painted {
        draw: 2,
        version: 2,
        holding: false,
        complete: true,
    }));
    frames.feedback(FrameFeedbackEvent::Submitted {
        draw_id: 2,
        frame_id: 2,
    });
    frames.feedback(FrameFeedbackEvent::Terminal(FrameTerminal {
        frame_id: 2,
        outcome: presented(),
    }));
    // A presented strip with no labels is a blank frame.
    frames.staged.set(Some(Painted {
        draw: 3,
        version: 3,
        holding: false,
        complete: false,
    }));
    frames.feedback(FrameFeedbackEvent::Submitted {
        draw_id: 3,
        frame_id: 3,
    });
    frames.feedback(FrameFeedbackEvent::Terminal(FrameTerminal {
        frame_id: 3,
        outcome: presented(),
    }));
    let json = frames.json();
    for field in [
        "\"holds\":1",
        "\"heldFrames\":1",
        "\"swappedFrames\":1",
        "\"presentedFrames\":3",
        "\"blankFrames\":1",
    ] {
        assert!(json.contains(field), "{json}");
    }
}

#[gpui::test]
fn u25_swap_state_carries_the_strip_counters_beside_the_pane_counters(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let flea::ipc::wire::Reply { code, stdout, .. } = read(shell, "swapState", window, cx);
            assert_eq!(code, 0);
            let (pane, strip) = stdout
                .split_once("\"strip\":")
                .expect("the strip counters follow the pane's");
            assert!(
                pane.contains("\"holds\":"),
                "the pane keeps its own counters: {stdout}"
            );
            for name in ["holds", "blankFrames", "heldFrames", "swappedFrames"] {
                assert!(strip.contains(&format!("\"{name}\":")), "{strip}");
            }
            assert!(strip.trim_end().ends_with("}}"), "{stdout}");
        })
    });
}

// The List view uses the row geometry it painted; mounted_tests pins Grid and Columns.
#[gpui::test]
fn u25_middle_click_list_row_uses_its_drawn_geometry(cx: &mut TestAppContext) {
    use crate::{pane::Kind, window::startup::View};
    use gpui::{point, px, Modifiers, MouseButton};
    let view = View::List;
    {
        let (backend, _requests) = Backend::test_channel();
        let (shell, cx) = cx.add_window_view(|_, cx| {
            let mut shell = shell_with(backend, cx);
            shell.view = view;
            let host = shell.panes[0].as_mut().unwrap();
            let held = std::rc::Rc::get_mut(&mut host.pane.held).unwrap();
            held.rows[0] = flea::protocol::RowFacts {
                name: "sub".into(),
                dir: true,
                ..Default::default()
            };
            shell
        });
        cx.run_until_parked();
        let click = |cx: &mut gpui::VisualTestContext| {
            let at = cx.update(|_, cx| {
                let shell = shell.read(cx);
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                let drawn = &list.drawn;
                assert!(drawn.frames > 0, "{view:?} drew no list");
                point(
                    drawn.bounds.origin.x + px(40.),
                    drawn.bounds.origin.y + px(drawn.metrics.row * 0.5),
                )
            });
            cx.simulate_mouse_down(at, MouseButton::Middle, Modifiers::default());
            cx.simulate_mouse_up(at, MouseButton::Middle, Modifiers::default());
            cx.run_until_parked();
        };
        let count = |cx: &mut gpui::VisualTestContext| {
            cx.update(|_, cx| shell.read(cx).strip.tabs[0].count())
        };
        cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.panes[0].as_mut().unwrap().pane.state.kind = Kind::Loading;
                cx.notify();
            })
        });
        click(cx);
        assert_eq!(count(cx), 1, "{view:?}: an unmounted list names no row");
        cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.panes[0].as_mut().unwrap().pane.state.kind = Kind::Ready;
                cx.notify();
            })
        });
        cx.run_until_parked();
        click(cx);
        assert_eq!(
            count(cx),
            2,
            "{view:?}: the row under the pointer opens a tab"
        );
    }
}
