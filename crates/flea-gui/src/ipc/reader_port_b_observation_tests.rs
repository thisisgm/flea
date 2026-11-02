//! Read the real close-control paint and the actual lifecycle transitions through Root dispatch.
use super::*;
use gpui::{Modifiers, MouseButton, VisualTestContext};

#[test]
fn frame_counters_credit_only_the_presented_stamp() {
    use crate::g2::pv1::{
        observations::Shared,
        observed_frames::{Frames, Stamp},
    };
    use gpui::{FrameFeedbackEvent as Event, FrameOutcome, FrameTerminal, PresentationTime};
    let observed = Shared::default();
    let mut frames = Frames::default();
    let stamp = Stamp {
        visible: true,
        ready: false,
        holding: false,
        loading: true,
    };
    frames.stage(1, stamp);
    frames.receive(
        Event::Submitted {
            draw_id: 1,
            frame_id: 10,
        },
        &observed,
    );
    assert_eq!(observed.borrow().swap.loading_frames, 0);
    frames.receive(
        Event::Terminal(FrameTerminal {
            frame_id: 10,
            outcome: FrameOutcome::Discarded,
        }),
        &observed,
    );
    assert_eq!(observed.borrow().swap.loading_frames, 0);
    frames.stage(2, stamp);
    frames.receive(
        Event::Submitted {
            draw_id: 2,
            frame_id: 11,
        },
        &observed,
    );
    observed.borrow_mut().landed = true;
    frames.receive(
        Event::Terminal(FrameTerminal {
            frame_id: 11,
            outcome: FrameOutcome::Presented(PresentationTime {
                timestamp: None,
                clock_id: None,
            }),
        }),
        &observed,
    );
    assert_eq!(observed.borrow().swap.loading_frames, 1);
    assert_eq!(observed.borrow().swap.mid_frames, 0);
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn markdown_and_html_close_readers_follow_painted_bounds_and_pointer_state(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    for (name, icon) in [("readme.md", "text-markdown"), ("page.html", "text-html")] {
        let (handle, _read) = open(cx);
        document(cx, handle, name, icon);
        edit(cx, handle, |shell, _, cx| {
            shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .update(cx, |view, cx| view.reveal_cap(cx));
        });
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        draw(&mut visual);
        let centre = visual
            .debug_bounds("pv1-close")
            .expect("the mounted document paints its close control")
            .center();
        let coordinate = format!(
            "{} {}",
            f32::from(centre.x).round(),
            f32::from(centre.y).round()
        );
        let state = flea::jsondoc::parse(&value(&mut visual, handle, "previewCloseState")).unwrap();
        assert_eq!(
            state.get("centre").and_then(flea::jsondoc::Json::as_str),
            Some(coordinate.as_str())
        );
        assert_eq!(
            state.get("pressed").and_then(flea::jsondoc::Json::as_bool),
            Some(false)
        );
        visual.simulate_mouse_move(centre, None, Modifiers::none());
        visual.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
        let state = flea::jsondoc::parse(&value(&mut visual, handle, "previewCloseState")).unwrap();
        assert_eq!(
            state.get("hovered").and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
        assert_eq!(
            state.get("pressed").and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
        visual.simulate_mouse_up(centre, MouseButton::Left, Modifiers::none());
        assert_eq!(value(&mut visual, handle, "previewCloseState"), "{}");
    }
}

#[gpui::test]
fn swap_reader_reports_cap_expiry_and_close_from_the_live_model(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    pdf(cx, handle);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(20));
    edit(cx, handle, |shell, _, cx| {
        shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .update(cx, |view, cx| view.reveal_cap(cx));
    });
    let state = flea::jsondoc::parse(&value(cx, handle, "previewSwapState")).unwrap();
    let look = state.get("look").unwrap();
    assert_eq!(
        look.get("holding").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        look.get("fallbacks").and_then(flea::jsondoc::Json::as_f64),
        Some(1.)
    );
    assert_eq!(
        look.get("last")
            .unwrap()
            .get("ms")
            .and_then(flea::jsondoc::Json::as_f64),
        Some(20.)
    );
    assert_eq!(
        look.get("last")
            .unwrap()
            .get("end")
            .and_then(flea::jsondoc::Json::as_str),
        Some("expired")
    );
    assert_eq!(
        state
            .get("lookOpen")
            .unwrap()
            .get("revealCaps")
            .and_then(flea::jsondoc::Json::as_f64),
        Some(1.)
    );
    edit(cx, handle, |shell, _, cx| {
        shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .update(cx, |view, cx| view.close(cx));
    });
    let state = flea::jsondoc::parse(&value(cx, handle, "previewSwapState")).unwrap();
    assert_eq!(
        state
            .get("lookVisible")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        state.get("lookOpen").unwrap().get("revealCapStart"),
        Some(&flea::jsondoc::Json::Null)
    );
}

#[gpui::test]
fn column_swap_reader_observes_hold_burst_expiry_and_hide(cx: &mut TestAppContext) {
    use crate::columns::preview_host::ThirdSwap;
    let (handle, _read) = open(cx);
    let preview = edit(cx, handle, |shell, window, cx| {
        shell.view = crate::window::startup::View::Columns;
        shell.sync_columns(window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane
            .receive(flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 1,
            })
            .unwrap();
        host.pane
            .receive(flea::protocol::Reply::Rows {
                start: 0,
                listing: 3,
                rows: vec![flea::protocol::RowFacts {
                    name: "pending.bin".into(),
                    mode: 0o100644,
                    ..Default::default()
                }],
            })
            .unwrap();
        shell.sync_columns(window, cx);
        shell.sync_selection_previews(window, cx);
        let preview = shell.selection_preview(0).unwrap();
        preview.update(cx, |view, _| {
            view.class = Some(("/files".into(), "local".into()))
        });
        shell.sync_selection_previews(window, cx);
        preview
    });
    let column = |cx: &mut TestAppContext| {
        flea::jsondoc::parse(&value(cx, handle, "previewSwapState"))
            .unwrap()
            .get("column")
            .unwrap()
            .clone()
    };
    let initial = column(cx);
    assert_eq!(
        initial
            .get("holding")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        initial.get("holds").and_then(flea::jsondoc::Json::as_f64),
        Some(1.)
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(20));
    preview.update(cx, |view, _| {
        view.port.hold("next-row");
        view.port.start(false);
    });
    assert_eq!(
        column(cx)
            .get("bursts")
            .and_then(flea::jsondoc::Json::as_f64),
        Some(1.)
    );
    let elapsed = crate::pane::swap::CAP_MS + 20;
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::CAP_MS));
    edit(cx, handle, |_, window, cx| {
        preview.update(cx, |view, cx| view.poll(window, cx))
    });
    preview.update(cx, |view, _| {
        assert!(
            view.visible,
            "cap expiry must keep the mounted preview visible"
        );
        assert!(
            view.pending.is_some(),
            "the real pane must retain the pending subject"
        );
    });
    let expired = column(cx);
    assert_eq!(
        expired
            .get("holding")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        expired
            .get("fellBack")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        expired
            .get("fallbacks")
            .and_then(flea::jsondoc::Json::as_f64),
        Some(1.)
    );
    let last = expired.get("last").unwrap();
    assert_eq!(
        last.get("ms").and_then(flea::jsondoc::Json::as_f64),
        Some(elapsed as f64)
    );
    assert_eq!(
        last.get("end").and_then(flea::jsondoc::Json::as_str),
        Some("expired")
    );
    edit(cx, handle, |_, window, cx| {
        preview.update(cx, |view, cx| view.hide(window, cx))
    });
    assert_eq!(column(cx).get("last"), Some(last));
}

#[test]
fn column_loading_duration_uses_the_painted_loading_start() {
    use crate::window::selection_preview::{observation, SwapState};
    let mut state = SwapState {
        holding: true,
        loading_at: Some(13),
        ..Default::default()
    };
    state.observation.hold(0, false, "one");
    observation::release(&mut state, 10, "expired", true);
    state.holding = false;
    observation::release(&mut state, 53, "landed", false);
    assert_eq!(state.observation.shown_ms, 40);
    assert_eq!(state.observation.last_ms, 10);
    assert_eq!(state.observation.last_end, "expired");
    observation::release(&mut state, 90, "cancelled", false);
    assert_eq!(state.observation.shown_ms, 40);
}

#[gpui::test]
fn keyboard_reader_tracks_sequence_and_clipboard_owner_changes(cx: &mut TestAppContext) {
    use crate::window::input::Dispatch;
    use flea::keys::sequence::Seat;
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        let event = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                key: "y".into(),
                key_char: Some("y".into()),
                ..Default::default()
            },
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(
            shell.keyboard.resolve(
                "vim",
                &event,
                Modifiers::none(),
                false,
                Seat {
                    cursor: 4,
                    ..Default::default()
                }
            ),
            Dispatch::Armed
        );
        shell.keyboard.capture_sequence_path("/files");
        assert!(shell.clipboard_line(0, r#"{"t":"clip","op":"changed","clip":"cut","paths":["/files/a b\"c"],"token":"cut-token"}"#, window, cx));
    });
    let state = flea::jsondoc::parse(&value(cx, handle, "keyDeliveryState")).unwrap();
    assert_eq!(
        state
            .get("keySequence")
            .and_then(flea::jsondoc::Json::as_str),
        Some("copyArm")
    );
    assert_eq!(
        state
            .get("keySequenceIdentity")
            .and_then(flea::jsondoc::Json::as_str),
        Some(r#"["/files",4,0,"list"]"#)
    );
    let clipboard = state.get("clipboard").unwrap();
    assert_eq!(
        clipboard.get("cut").and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        clipboard.get("paths").unwrap().as_array().unwrap()[0].as_str(),
        Some("/files/a b\"c")
    );
    edit(cx, handle, |shell, _, _| shell.keyboard.disarm());
    let state = flea::jsondoc::parse(&value(cx, handle, "keyDeliveryState")).unwrap();
    assert_eq!(
        state
            .get("keySequence")
            .and_then(flea::jsondoc::Json::as_str),
        Some("")
    );
    assert_eq!(
        state
            .get("keySequenceIdentity")
            .and_then(flea::jsondoc::Json::as_str),
        Some("")
    );
}
