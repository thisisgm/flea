//! EY-200: the chooser's Up mark held for the press-and-hold interval is a long press, so its release never navigates.
use super::process::Line;
use super::tests::fixture::fixture;
use gpui::{Modifiers, MouseButton, TestAppContext};
use std::time::Duration;

// QStyleHints::mousePressAndHoldInterval, as path/tap.rs LONG_PRESS.
const LONG_PRESS: Duration = Duration::from_millis(800);

#[gpui::test]
fn ey200_chooser_up_held_past_the_long_press_interval_stays(cx: &mut TestAppContext) {
    for (held, location) in [
        (LONG_PRESS, "/d/sub"),
        (LONG_PRESS - Duration::from_millis(1), "/d"),
    ] {
        let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
        f.listing.say(Line::Reply(
            r#"{"t":"listed","n":0,"path":"/d/sub"}"#.into(),
        ));
        f.cx.run_until_parked();
        let at = f.cx.debug_bounds("picker-up").unwrap().center();
        f.cx.simulate_mouse_move(at, None, Modifiers::none());
        f.cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        f.cx.executor().advance_clock(held);
        f.cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        f.cx.run_until_parked();
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            location,
            "held {held:?}"
        );
    }
}
