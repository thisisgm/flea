//! EY-439: the mounted footer paints and retires its compact Omi with Starting activity.
use super::hero_tests::{english, frame, run};
use super::line::MessageLine;
use crate::palette::Theme;
use flea::ui_logic_status_feedback::status::Hint;
use gpui::{px, size, Bounds, Pixels, TestAppContext, VisualTestContext, WindowHandle};
use std::sync::Arc;

fn open(cx: &mut TestAppContext) -> WindowHandle<MessageLine> {
    let handle = cx.add_window(|_, cx| MessageLine::new("default", Arc::new(english()), cx));
    VisualTestContext::from_window(handle.into(), cx).simulate_resize(size(px(500.), px(27.)));
    handle
        .update(cx, |line, _, cx| {
            line.update(cx, |m, now| {
                m.set_activity(77, "Opening fixture", 0, false, true, now)
            })
        })
        .unwrap();
    frame(cx, handle);
    handle
}

fn slot(cx: &mut TestAppContext, handle: WindowHandle<MessageLine>) -> Option<Bounds<Pixels>> {
    VisualTestContext::from_window(handle.into(), cx).debug_bounds("fb1-busy")
}

fn assert_painted(cx: &mut TestAppContext, handle: WindowHandle<MessageLine>, theme: Theme) {
    let mark = slot(cx, handle).expect("a displayed Starting line must mount its Omi");
    assert_eq!(mark.size, size(px(22.), px(22.)));
    handle
        .update(cx, |line, window, _| {
            let primary = line.painted.get("primary").expect("Opening text");
            assert_eq!(primary.color, theme.foreground);
            assert_eq!(primary.bounds.left() - mark.right(), px(9.));
            let mark = mark.scale(window.scale_factor());
            assert!(
                window.painted_quads().iter().any(|q| {
                    q.bounds.is_contained_within(&mark)
                        && q.background.as_solid().is_some_and(|ink| {
                            (ink.h, ink.s, ink.l)
                                == (theme.accent.h, theme.accent.s, theme.accent.l)
                                && ink.a > 0.
                        })
                }),
                "the mounted Omi must actually paint in the GM-mandated theme accent"
            );
        })
        .unwrap();
}

#[gpui::test]
fn ey439_starting_footer_paints_omi_and_retires_after_activity_ends(cx: &mut TestAppContext) {
    for colors in [
        "foreground='#c6b395'\naccent='#d28a41'\nbackground='#1c120d'",
        "foreground='#4c4f69'\naccent='#1e66f5'\nbackground='#eff1f5'",
    ] {
        let theme = Theme::from_text(colors);
        cx.update(|cx| cx.set_global(theme));
        let handle = open(cx);
        assert_painted(cx, handle, theme);
        assert!(run(cx, handle, 128) > 0, "visible Omi must animate");
        handle
            .update(cx, |line, _, cx| {
                line.update(cx, |m, now| m.say("Notice", false, "", Hint::None, now))
            })
            .unwrap();
        run(cx, handle, 64);
        assert!(
            slot(cx, handle).is_some(),
            "the notice waits behind Opening"
        );
        assert_painted(cx, handle, theme);
        handle
            .update(cx, |line, _, cx| {
                line.update(cx, |m, now| m.set_activity(77, "", 0, false, false, now))
            })
            .unwrap();
        run(cx, handle, 64);
        assert!(slot(cx, handle).is_none());
        assert_eq!(
            run(cx, handle, 128),
            0,
            "terminal clear leaves the footer idle"
        );
    }
}

#[gpui::test]
fn ey439_reduced_motion_footer_keeps_the_omi_still(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let theme = Theme::default();
    let handle = open(cx);
    assert_painted(cx, handle, theme);
    assert_eq!(run(cx, handle, 1000), 0);
}
