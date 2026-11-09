//! The picker keeps a closed filter menu drawn for its 140 ms fade, drops it once the fade ends, and a reopen inside the fade keeps the same menu.
use super::{
    pointer_tests::seed,
    tests::fixture::{fixture, Fixture},
};
use gpui::{Modifiers, RequestFrameOptions, TestAppContext};
use std::time::Duration;

fn press_trigger(f: &mut Fixture) {
    let trigger = f.cx.debug_bounds("picker-filter").unwrap().center();
    f.cx.simulate_click(trigger, Modifiers::none());
    f.cx.run_until_parked();
}

// One platform frame request: queued frame callbacks run, then a dirty window draws.
fn frame(f: &mut Fixture) {
    // An inactive window throttles a frame that only runs callbacks, by the wall clock.
    f.cx.update(|window, _| window.activate_window());
    f.cx.simulate_frame_request(RequestFrameOptions::default());
    f.cx.run_until_parked();
}

// The frame that ends a fade, then the frame its last request asks for.
fn frames(f: &mut Fixture) {
    frame(f);
    frame(f);
}

#[gpui::test]
fn closed_filter_menu_fades_out_then_is_dropped_and_a_reopen_keeps_it(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d/sub","filters":[{"label":"Text","globs":["*.txt"]}]}"#,
        None,
    );
    seed(&mut f);
    press_trigger(&mut f);
    let first = f.read(|picker| picker.chrome.menu_view.clone()).unwrap();
    f.cx.executor().advance_clock(Duration::from_millis(200));
    frames(&mut f);
    // The card covers the trigger, so this press is its outside click.
    press_trigger(&mut f);
    assert!(
        f.read(|picker| picker.chrome.menu_view.is_none()
            && picker.chrome.menu_closing.as_ref() == Some(&first)),
        "a closed filter menu stays drawn through its fade"
    );
    frames(&mut f);
    f.cx.executor().advance_clock(Duration::from_millis(70));
    frames(&mut f);
    let stood = f.cx.update(|_, cx| first.read(cx).card_opacity());
    assert!(
        stood > 0. && stood < 1.,
        "halfway through the close the card stands at {stood}"
    );
    press_trigger(&mut f);
    assert!(
        f.read(|picker| picker.chrome.menu_view.as_ref() == Some(&first)
            && picker.chrome.menu_closing.is_none()),
        "a reopen inside the fade keeps the closing menu"
    );
    frames(&mut f);
    let reopened = f.cx.update(|_, cx| first.read(cx).card_opacity());
    assert!(
        (reopened - stood).abs() < 1e-3,
        "the reopened card fades back up from {stood}, not from 0: {reopened}"
    );
    f.cx.executor().advance_clock(Duration::from_millis(200));
    frames(&mut f);
    assert_eq!(
        f.cx.update(|_, cx| first.read(cx).card_opacity()),
        1.,
        "the reopened card finishes its open fade"
    );
    press_trigger(&mut f);
    frames(&mut f);
    f.cx.executor().advance_clock(Duration::from_millis(140));
    frames(&mut f);
    assert!(
        f.read(|picker| picker.chrome.menu_closing.is_none()),
        "the picker drops the menu once its fade ends, with no other input"
    );
}

#[gpui::test]
fn closed_filter_menu_is_dropped_under_reduced_motion(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d/sub","filters":[{"label":"Text","globs":["*.txt"]}]}"#,
        None,
    );
    f.cx.update(|_, cx| cx.set_reduce_motion(true));
    seed(&mut f);
    press_trigger(&mut f);
    frames(&mut f);
    press_trigger(&mut f);
    assert!(
        f.read(|picker| picker.chrome.menu_view.is_none()),
        "the outside press closes the menu"
    );
    frames(&mut f);
    assert!(
        f.read(|picker| picker.chrome.menu_closing.is_none()),
        "a snapped close leaves no menu mounted"
    );
}
