use super::hero_tests::{english, frame, run};
use super::line::{ArmLost, MessageLine};
use flea::ui_logic_status_feedback::messages::MESSAGE_MS;
use flea::ui_logic_status_feedback::status::{Hint, SEP};
use gpui::{
    point, px, size, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle,
};
use std::{cell::Cell, cell::RefCell, rc::Rc, sync::Arc};

fn open(cx: &mut TestAppContext) -> WindowHandle<MessageLine> {
    let catalog = Arc::new(english());
    let handle = cx.add_window(|_, cx| MessageLine::new("default", catalog, cx));
    cx.run_until_parked();
    handle
}

fn drawn(
    cx: &mut TestAppContext,
    handle: WindowHandle<MessageLine>,
) -> (String, String, String, String) {
    handle
        .update(cx, |line, _, _| {
            let d = line.drawn();
            (
                d.primary.to_string(),
                d.hint.to_string(),
                d.extra.to_string(),
                d.undo.to_string(),
            )
        })
        .unwrap()
}

#[gpui::test]
fn a_notice_leaves_on_its_timer_and_the_lane_is_idle_after(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |line, _, cx| {
            line.update(cx, |m, now| {
                m.say("Renamed to notes.txt", false, "", Hint::None, now)
            })
        })
        .unwrap();
    assert_eq!(drawn(cx, handle).0, "Renamed to notes.txt");
    assert_eq!(run(cx, handle, MESSAGE_MS - 64), 0);
    assert_eq!(drawn(cx, handle).0, "Renamed to notes.txt");
    run(cx, handle, 128);
    assert_eq!(drawn(cx, handle).0, "");
    let (renders, timers) = handle
        .update(cx, |line, _, _| {
            (line.counters.renders.get(), line.counters.timers.get())
        })
        .unwrap();
    assert_eq!(run(cx, handle, 10_000), 0);
    assert_eq!(
        handle
            .update(cx, |line, _, _| (
                line.counters.renders.get(),
                line.counters.timers.get()
            ))
            .unwrap(),
        (renders, timers)
    );
}

#[gpui::test]
fn a_long_error_keeps_its_whole_hint_and_an_undo_notice_splits_its_target(cx: &mut TestAppContext) {
    let handle = open(cx);
    let long = format!("Copy failed: {}.txt · permission denied", "0".repeat(200));
    handle
        .update(cx, |line, _, cx| {
            line.update(cx, |m, now| m.say(&long, true, "", Hint::None, now))
        })
        .unwrap();
    run(cx, handle, 32);
    assert_eq!(
        drawn(cx, handle),
        (
            long.clone(),
            " · esc dismisses".into(),
            String::new(),
            String::new()
        )
    );
    handle
        .update(cx, |line, _, cx| {
            line.update(cx, |m, now| {
                m.dismiss(now);
                m.say("Copied 1 item", false, "", Hint::Undo, now)
            })
        })
        .unwrap();
    assert_eq!(
        drawn(cx, handle),
        (
            "Copied 1 item".into(),
            String::new(),
            String::new(),
            "z undoes".into()
        )
    );
    // An undo receipt leaves on its clock like any notice.
    run(cx, handle, MESSAGE_MS + 500);
    assert_eq!(drawn(cx, handle).0, "");
}

#[gpui::test]
fn the_state_sentence_drops_the_dismiss_key(cx: &mut TestAppContext) {
    let handle = open(cx);
    let sentence = "You don’t have permission to open this folder.";
    handle
        .update(cx, |line, _, cx| {
            line.set_state_message(sentence, cx);
            line.update(cx, |m, now| m.say(sentence, true, "", Hint::None, now))
        })
        .unwrap();
    assert_eq!(
        drawn(cx, handle),
        (sentence.into(), String::new(), String::new(), String::new())
    );
}

#[gpui::test]
fn an_arm_run_out_on_its_clock_is_reported_to_its_owner(cx: &mut TestAppContext) {
    let handle = open(cx);
    let lost = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&lost);
    let entity = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
    let _keep = cx.update(|cx| {
        cx.subscribe(&entity, move |_, event: &ArmLost, _| {
            seen.borrow_mut().push(event.0)
        })
    });
    let start = handle.update(cx, |line, _, cx| line.now(cx)).unwrap();
    handle
        .update(cx, |line, _, cx| {
            line.update(cx, |m, now| {
                m.set_arm_owner(5, now.max(1), now);
                m.say_arm(
                    "Press d again to move it to Trash, or Delete on its own.",
                    1500,
                    now,
                )
            })
        })
        .unwrap();
    assert!(drawn(cx, handle).0.starts_with("Press d again"));
    run(cx, handle, 1600);
    assert_eq!(lost.borrow().len(), 1);
    assert_eq!(lost.borrow()[0].owner, 5);
    assert_eq!(lost.borrow()[0].stamp, start.max(1));
    assert_eq!(drawn(cx, handle).0, "");
}

/// A 400x24 lane saying "Copied 1 item" with its undo target, and a counter of undo runs.
fn undo_lane(cx: &mut TestAppContext) -> (WindowHandle<MessageLine>, Rc<Cell<usize>>) {
    let handle = open(cx);
    let undone = Rc::new(Cell::new(0usize));
    let seen = Rc::clone(&undone);
    handle
        .update(cx, |line, _, cx| {
            line.on_undo(Rc::new(move |_, _| seen.set(seen.get() + 1)));
            line.update(cx, |m, now| {
                m.say("Copied 1 item", false, "", Hint::Undo, now)
            });
        })
        .unwrap();
    cx.simulate_window_resize(handle.into(), size(px(400.), px(24.)));
    frame(cx, handle);
    frame(cx, handle);
    (handle, undone)
}

/// StatusBar.qml's undo TapHandler is ReleaseWithinBounds: a drag or marquee released over "z undoes" never runs undo.
#[gpui::test]
fn a_release_whose_press_began_elsewhere_never_runs_undo(cx: &mut TestAppContext) {
    let (handle, undone) = undo_lane(cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for y in (1..24).step_by(2) {
        for x in (0..400).step_by(2) {
            let at = point(px(x as f32), px(y as f32));
            vcx.simulate_mouse_down(point(px(1.), px(1.)), MouseButton::Left, Modifiers::none());
            vcx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
            vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        }
    }
    assert_eq!(
        undone.get(),
        0,
        "undo ran on releases whose press began at (1,1)"
    );
}

/// A press and a release on the target run undo once, and a press on it released elsewhere does not.
#[gpui::test]
fn a_click_on_the_undo_target_runs_undo_once(cx: &mut TestAppContext) {
    let (handle, undone) = undo_lane(cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let target = vcx
        .debug_bounds("fb1-undo")
        .expect("the undo target is drawn");
    vcx.simulate_click(target.center(), Modifiers::none());
    assert_eq!(undone.get(), 1);
    vcx.simulate_mouse_down(target.center(), MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_move(point(px(1.), px(1.)), MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_up(point(px(1.), px(1.)), MouseButton::Left, Modifiers::none());
    assert_eq!(undone.get(), 1);
}

/// StatusBar.qml: a retry line sharing the lane gives way first, so the notice stays whole until the rest is down to " · …".
#[gpui::test]
fn a_notice_stays_whole_while_the_retry_line_beside_it_gives_way(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |line, _, cx| {
            line.set_retry_line("Retry selected: zz-locked.txt", cx);
            line.update(cx, |m, now| {
                m.say("Copied 3 of 4 items · 1 failed", false, "", Hint::None, now)
            })
        })
        .unwrap();
    let floor = handle
        .update(cx, |line, window, _| {
            let font = line.typeface().font.clone();
            let caption = line.typeface().caption;
            let rest = format!("{SEP}\u{2026}").into();
            super::caption::measure(rest, font, caption, px(0.), window)
        })
        .unwrap();
    let mut lane = |width: f32| {
        cx.simulate_window_resize(handle.into(), size(px(width), px(24.)));
        frame(cx, handle);
        frame(cx, handle);
        handle
            .update(cx, |line, _, _| {
                let part = |name| line.painted.get(name).expect("drawn");
                (part("primary"), part("extra"))
            })
            .unwrap()
    };
    let width = |part: &super::observe::Text| f32::from(part.bounds.size.width);
    let (primary, extra) = lane(1000.);
    assert!(width(&primary) >= primary.implicit_width && width(&extra) >= extra.implicit_width);
    let (notice, rest) = (primary.implicit_width, extra.implicit_width);
    let (primary, extra) = lane((notice + rest / 2.).round());
    assert!(
        width(&primary) >= notice,
        "the notice is whole beside the retry line: {} of {notice}",
        width(&primary)
    );
    assert!(width(&extra) < rest, "the retry line gives way");
    let (primary, extra) = lane((notice / 2.).round());
    assert!(
        width(&primary) < notice,
        "a lane narrower than the notice elides it"
    );
    assert!(
        width(&extra) >= floor,
        "the retry line keeps its \" · …\" while the notice elides: {} of {floor}",
        width(&extra)
    );
}

#[gpui::test]
fn the_status_line_draws_an_error_in_the_red_lifted_on_the_surface(cx: &mut TestAppContext) {
    let theme = crate::palette::Theme::from_text(crate::window::settings::ink_tests::SPLIT);
    let palette = crate::list::Palette::inherited(&gpui::TextStyle::default(), theme);
    assert_ne!(palette.error, palette.error_on_surface);
    cx.update(|cx| cx.set_global(theme));
    let handle = open(cx);
    handle
        .update(cx, |line, _, cx| {
            line.update(cx, |m, now| m.say("Copy failed", true, "", Hint::None, now))
        })
        .unwrap();
    run(cx, handle, 32);
    let ink = handle
        .update(cx, |line, _, _| {
            line.painted.get("primary").map(|text| text.color)
        })
        .unwrap();
    assert_eq!(ink, Some(palette.error_on_surface));
}

#[test]
fn trash_font_a_drawn_status_line_never_carries_a_line_break() {
    use super::line::Drawn;
    use flea::ui_logic_status_feedback::messages::Line;
    let error = Drawn::from(Line {
        primary: "Deleted 0 of 1 item · 1 failed\nlocked failed: denied".into(),
        error: true,
        detail: "second failed: denied".into(),
        ..Default::default()
    });
    assert_eq!(error.primary.as_ref(), "Deleted 0 of 1 item · 1 failed");
    assert_eq!(
        error.detail.as_ref(),
        "locked failed: denied\nsecond failed: denied",
        "an error's later lines lead its detail strip"
    );
    let notice = Drawn::from(Line {
        primary: "Copied\n2 items".into(),
        hint: "esc\ndismisses".into(),
        extra: " · searching".into(),
        ..Default::default()
    });
    assert_eq!(notice.primary.as_ref(), "Copied 2 items");
    assert_eq!(notice.hint.as_ref(), "esc dismisses");
    assert_eq!(
        notice.extra.as_ref(),
        " · searching",
        "single-line text is untouched"
    );
    assert!(notice.detail.is_empty());
}
