use super::rail::{
    host::{Event, Host},
    Handoff, Mode, CLOSE_MS, DWELL_MS, OPEN_MS, SETTLE_MS,
};
use gpui::{
    div, point, prelude::*, px, AnyView, Context, Entity, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, Window,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

// A row that takes its press for itself, as a places row does; `grabs` false is a plain rows view.
struct Rows {
    grabs: bool,
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = div().size_full();
        if self.grabs {
            rows.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        } else {
            rows
        }
    }
}

type Heard = Rc<RefCell<Vec<Event>>>;

fn open(mode: Mode, cx: &mut TestAppContext) -> (Entity<Host>, Heard, &mut VisualTestContext) {
    open_rows(mode, false, cx)
}

fn open_rows(
    mode: Mode,
    grabs: bool,
    cx: &mut TestAppContext,
) -> (Entity<Host>, Heard, &mut VisualTestContext) {
    let (host, cx) = cx.add_window_view(|_, cx| {
        Host::new(
            mode,
            192.,
            Box::new(move |_, cx| AnyView::from(cx.new(|_| Rows { grabs }))),
            cx,
        )
    });
    let heard: Heard = Rc::default();
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&host, move |_, event: &Event, _| {
            sink.borrow_mut().push(*event)
        })
        .detach()
    });
    cx.run_until_parked();
    (host, heard, cx)
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn moved(cx: &mut VisualTestContext, x: f32) {
    cx.simulate_mouse_move(point(px(x), px(100.)), None, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_hidden_rail_builds_no_rows_and_arms_no_timer(cx: &mut TestAppContext) {
    let (host, heard, cx) = open(Mode::Hidden, cx);
    moved(cx, 5.);
    wait(cx, 1000);
    host.read_with(cx, |host, _| {
        assert_eq!(host.constructed, 0);
        assert!(host.content().is_none() && !host.timer_armed());
        assert_eq!(host.rail.counters.timers, 0);
    });
    assert!(heard.borrow().is_empty());
}

#[gpui::test]
fn the_edge_reveals_after_the_dwell_and_the_rows_survive_the_hide(cx: &mut TestAppContext) {
    // The test pointer starts on the strip, so the switch to auto-hide comes once it stands over the listing.
    let (host, heard, cx) = open(Mode::Hidden, cx);
    moved(cx, 500.);
    host.update(cx, |host, cx| host.set_mode(Mode::Overlay, cx));
    cx.run_until_parked();
    host.read_with(cx, |host, _| {
        assert_eq!(host.constructed, 0);
        assert!(!host.rail.live() && !host.timer_armed());
    });
    moved(cx, 5.);
    host.read_with(cx, |host, _| {
        assert!(!host.rail.shown() && host.timer_armed());
        assert_eq!(host.constructed, 1);
    });
    wait(cx, DWELL_MS);
    host.read_with(cx, |host, _| assert!(host.rail.shown()));
    assert_eq!(*heard.borrow(), [Event::Polling(true)]);
    wait(cx, OPEN_MS);
    moved(cx, 600.);
    wait(cx, SETTLE_MS - 20);
    host.read_with(cx, |host, _| assert!(host.rail.shown()));
    wait(cx, 20 + CLOSE_MS);
    host.read_with(cx, |host, _| {
        assert!(!host.rail.shown() && host.content().is_some() && !host.timer_armed());
    });
    assert_eq!(
        *heard.borrow(),
        [Event::Polling(true), Event::Polling(false)]
    );
    moved(cx, 5.);
    wait(cx, DWELL_MS);
    host.read_with(cx, |host, _| {
        assert!(host.rail.shown());
        assert_eq!(host.constructed, 1);
    });
}

#[gpui::test]
fn a_press_on_the_overlay_rail_claims_the_keyboard(cx: &mut TestAppContext) {
    let (host, heard, cx) = open(Mode::Overlay, cx);
    host.update(cx, |host, cx| host.focus(true, cx));
    wait(cx, OPEN_MS);
    cx.simulate_mouse_down(
        point(px(100.), px(100.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(heard.borrow().contains(&Event::Focus(Handoff::Rail)));
    // A docked rail leaves the press to its rows.
    host.update(cx, |host, cx| host.set_mode(Mode::Docked, cx));
    cx.run_until_parked();
    heard.borrow_mut().clear();
    cx.simulate_mouse_down(
        point(px(100.), px(100.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(!heard.borrow().contains(&Event::Focus(Handoff::Rail)));
}

#[gpui::test]
fn hiding_a_docked_rail_drops_its_rows_and_returns_a_held_keyboard(cx: &mut TestAppContext) {
    let (host, heard, cx) = open(Mode::Docked, cx);
    host.read_with(cx, |host, _| assert_eq!(host.constructed, 1));
    host.update(cx, |host, cx| {
        host.focus(true, cx);
        host.set_mode(Mode::Hidden, cx);
    });
    cx.run_until_parked();
    host.read_with(cx, |host, _| assert!(host.content().is_none()));
    assert_eq!(
        *heard.borrow(),
        [Event::Focus(Handoff::List), Event::Polling(false)]
    );
    host.update(cx, |host, cx| host.set_mode(Mode::Docked, cx));
    cx.run_until_parked();
    host.read_with(cx, |host, _| assert_eq!(host.constructed, 2));
}

#[gpui::test]
fn a_row_that_takes_its_press_still_lets_the_overlay_rail_claim_the_keyboard(
    cx: &mut TestAppContext,
) {
    let (host, heard, cx) = open_rows(Mode::Overlay, true, cx);
    host.update(cx, |host, cx| host.focus(true, cx));
    wait(cx, OPEN_MS);
    heard.borrow_mut().clear();
    cx.simulate_mouse_down(
        point(px(100.), px(100.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(
        heard.borrow().contains(&Event::Focus(Handoff::Rail)),
        "{:?}",
        heard.borrow()
    );
}

// ui/PaneRail.qml railHover is a HoverHandler, which no key press unhovers, so a still pointer keeps the rail up.
#[gpui::test]
fn a_key_press_leaves_a_pointer_revealed_rail_up(cx: &mut TestAppContext) {
    let (host, _, cx) = open(Mode::Overlay, cx);
    moved(cx, 5.);
    wait(cx, DWELL_MS + OPEN_MS);
    moved(cx, 100.);
    wait(cx, SETTLE_MS + CLOSE_MS + 50);
    host.read_with(cx, |host, _| {
        assert!(host.rail.shown() && host.rail.wants().over)
    });
    cx.simulate_keystrokes("j");
    wait(cx, SETTLE_MS + CLOSE_MS + 50);
    host.read_with(cx, |host, app| {
        assert!(host.rail.shown(), "{:?}", host.rail.wants());
        assert!(host.rail.drawn(app.background_executor().now()));
    });
}

// The same for the strip: a key pressed during the dwell leaves the pointer's reveal due.
#[gpui::test]
fn a_key_press_during_the_dwell_keeps_the_strip_reveal(cx: &mut TestAppContext) {
    let (host, _, cx) = open(Mode::Hidden, cx);
    moved(cx, 500.);
    host.update(cx, |host, cx| host.set_mode(Mode::Overlay, cx));
    cx.run_until_parked();
    moved(cx, 5.);
    wait(cx, DWELL_MS / 2);
    cx.simulate_keystrokes("j");
    wait(cx, DWELL_MS);
    host.read_with(cx, |host, _| {
        assert!(
            host.rail.shown() && host.rail.wants().edge,
            "{:?}",
            host.rail.wants()
        )
    });
}
