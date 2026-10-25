//! The Plugins pane scrolled through real input: a list longer than the pane under the wheel, a stroke and the keys.
use super::page_tests::{builtins, english, listing, Recorder};
use super::seam::{PluginsWire, Tokens};
use super::view::PluginsPane;
use gpui::{
    point, px, size, Bounds, Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    TestAppContext, TouchPhase, VisualTestContext,
};
use std::rc::Rc;

// One wheel notch as gpui-pre-linux sends it, three lines down, and the 288px the Settings pane moves for it.
const NOTCH_DOWN: f32 = -3.;
const NOTCH_PX: f32 = 288.;
// The rows' foot padding under the last row.
const ROWS_FOOT: f32 = 8.;

fn tokens() -> Tokens {
    Tokens::interim(&crate::list::Palette::inherited(
        &gpui::TextStyle::default(),
        crate::palette::Theme::default(),
    ))
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn rect(pane: &Entity<PluginsPane>, cx: &mut VisualTestContext, id: &str) -> Bounds<Pixels> {
    let text = pane.read_with(cx, |pane, _| pane.probe.rect(id));
    let parts: Vec<f32> = text
        .split_whitespace()
        .map(|part| part.parse().expect("a probe number"))
        .collect();
    assert_eq!(parts.len(), 4, "{id} drawn: {text:?}");
    Bounds::new(
        point(px(parts[0]), px(parts[1])),
        size(px(parts[2]), px(parts[3])),
    )
}

// One platform scroll event at the pointer with no motion before it; the window draws only if it asked to.
fn send(cx: &mut VisualTestContext, at: Point<Pixels>, delta: ScrollDelta, phase: TouchPhase) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        modifiers: Modifiers::none(),
        touch_phase: phase,
    });
    cx.run_until_parked();
}

fn notch(cx: &mut VisualTestContext, at: Point<Pixels>, notches: f32) {
    let delta = ScrollDelta::Lines(point(0., NOTCH_DOWN * notches));
    send(cx, at, delta, TouchPhase::Moved);
}

fn stroke(cx: &mut VisualTestContext, at: Point<Pixels>, moves: &[f32]) {
    let zero = ScrollDelta::Pixels(point(px(0.), px(0.)));
    send(cx, at, zero, TouchPhase::Started);
    for &y in moves {
        let delta = ScrollDelta::Pixels(point(px(0.), px(y)));
        send(cx, at, delta, TouchPhase::Moved);
    }
    send(cx, at, zero, TouchPhase::Ended);
}

fn near(a: Pixels, b: Pixels) -> bool {
    (f32::from(a) - f32::from(b)).abs() <= 1.
}

// The boards' pane box with forty plugins installed, focused and drawn.
fn long(cx: &mut TestAppContext) -> (Entity<PluginsPane>, &mut VisualTestContext) {
    let wire = Rc::new(Recorder::default());
    let (pane, cx) = cx.add_window_view(move |_, cx| {
        PluginsPane::new(english(), wire as Rc<dyn PluginsWire>, tokens(), 8., cx)
    });
    cx.simulate_resize(size(px(462.), px(656.)));
    cx.update(|window, cx| pane.read(cx).focus_handle().clone().focus(window, cx));
    let base = builtins("{}").remove(0);
    let plugins = (0..40)
        .map(|n| {
            let mut plugin = base.clone();
            plugin.manifest.id = format!("test.plugin{n:02}");
            plugin.manifest.name = format!("Plugin {n:02}");
            plugin
        })
        .collect();
    cx.update(|_, cx| pane.update(cx, |pane, cx| pane.receive(&listing(1, plugins), cx)));
    draw(cx);
    (pane, cx)
}

#[gpui::test]
fn a_long_plugin_list_follows_the_wheel_and_a_stroke_and_holds_both_ends(cx: &mut TestAppContext) {
    let (pane, cx) = long(cx);
    let frame = rect(&pane, cx, "pane");
    let first = "row:test.plugin00";
    let last = "row:test.plugin39";
    let top = rect(&pane, cx, first).top();
    assert!(
        rect(&pane, cx, last).bottom() > frame.bottom() + px(NOTCH_PX + 40.),
        "forty plugins run past the pane"
    );
    let at = frame.center();
    notch(cx, at, 1.);
    let moved = rect(&pane, cx, first).top();
    assert!(
        near(moved, top - px(NOTCH_PX)),
        "ST_PLUGINS_WHEEL: a notch moves the drawn rows {NOTCH_PX}, they moved {:?}",
        top - moved
    );
    stroke(cx, at, &[-12., -12., -24.]);
    let stroked = rect(&pane, cx, first).top();
    assert!(
        near(stroked, moved - px(40.)),
        "ST_PLUGINS_TOUCH: the stroke moves the drawn rows 40, they moved {:?}",
        moved - stroked
    );
    for _ in 0..30 {
        notch(cx, at, 1.);
    }
    let held = rect(&pane, cx, last);
    assert!(
        near(held.bottom(), frame.bottom() - px(ROWS_FOOT)),
        "the wheel stops with the last plugin on the pane's foot: {held:?} in {frame:?}"
    );
    for _ in 0..30 {
        notch(cx, at, -1.);
    }
    assert_eq!(
        rect(&pane, cx, first).top(),
        top,
        "the wheel stops at the first plugin"
    );
}

#[gpui::test]
fn keys_keep_the_cursor_plugin_inside_the_pane(cx: &mut TestAppContext) {
    let (pane, cx) = long(cx);
    let frame = rect(&pane, cx, "pane");
    let top = rect(&pane, cx, "row:test.plugin00").top();
    for _ in 0..45 {
        cx.simulate_keystrokes("down");
        draw(cx);
        let id = pane.read_with(cx, |pane, _| {
            let page = pane.page();
            page.cursor()
                .and_then(|at| page.rows().get(at))
                .map(|row| row.id().to_string())
                .unwrap_or_default()
        });
        if id.is_empty() {
            continue;
        }
        let row = rect(&pane, cx, &format!("row:{id}"));
        assert!(
            row.top() >= frame.top() && row.bottom() <= frame.bottom(),
            "ST_PLUGINS_KEYS: {id} drawn inside the pane: {row:?} in {frame:?}"
        );
    }
    for _ in 0..45 {
        cx.simulate_keystrokes("up");
        draw(cx);
    }
    assert_eq!(
        rect(&pane, cx, "row:test.plugin00").top(),
        top,
        "Up walks back to the first plugin"
    );
}

#[gpui::test]
fn a_listing_that_shrinks_the_rows_pulls_the_scroll_back_on_the_next_frame(
    cx: &mut TestAppContext,
) {
    let (pane, cx) = long(cx);
    let frame = rect(&pane, cx, "pane");
    for _ in 0..30 {
        notch(cx, frame.center(), 1.);
    }
    assert!(
        pane.read_with(cx, |pane, _| pane.scroll) > 0.,
        "the wheel scrolled the rows"
    );
    let few = builtins("{}");
    // One explicit frame draws the shorter listing, its clamp still reading the longer rows' boxes.
    cx.update(|window, cx| {
        pane.update(cx, |pane, cx| pane.receive(&listing(2, few), cx));
        window.draw(cx).clear(cx);
    });
    assert!(
        rect(&pane, cx, "rows").size.height < frame.size.height,
        "the frame drew the shorter listing"
    );
    assert!(
        pane.read_with(cx, |pane, _| pane.scroll) > 0.,
        "the first frame after the listing keeps the scroll"
    );
    let notified = Rc::new(std::cell::Cell::new(0));
    let count = notified.clone();
    let _watch = cx.update(|_, cx| cx.observe(&pane, move |_, _| count.set(count.get() + 1)));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert!(
        notified.get() > 0,
        "ST_PLUGINS_SHRINK: the next frame asks the pane to draw again"
    );
    draw(cx);
    assert_eq!(
        pane.read_with(cx, |pane, _| pane.scroll),
        0.,
        "ST_PLUGINS_SHRINK: the frame after a shorter listing clamps against its own boxes"
    );
    let first = pane.read_with(cx, |pane, _| pane.page().rows()[0].id().to_string());
    assert_eq!(rect(&pane, cx, &format!("row:{first}")).top(), frame.top());
}
