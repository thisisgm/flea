//! The Settings pane scrolled through real input: wheel notches, touchpad strokes and keys, drawn in a test window.
use super::geometry::{fade_shown, PANE_FOOT};
use super::tests::panel;
use super::view::SettingsView;
use gpui::{
    div, point, prelude::*, px, size, AnyView, Bounds, Context, Modifiers, Pixels, Point, Render,
    ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

// One wheel notch as the pinned platform sends it on Wayland and X11: three lines, down is negative.
pub(super) const NOTCH_DOWN: f32 = -3.;
// v0.3.12's FastScrollHandler: a notch is three lines of 24 px times 4, a touchpad moves 2.5 per raw pixel.
pub(super) const NOTCH_PX: f32 = 288.;
// gpui-pre-linux hands a touchpad's raw axis travel over gained by three.
pub(super) const RAW_PER_PLATFORM_PX: f32 = 1. / 3.;
pub(super) const TOUCH_GAIN: f32 = 2.5;

// One frame as the platform asks for it: no refresh, so cached views are reused unless something notified them.
pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

pub(super) fn rect(
    view: &gpui::Entity<SettingsView>,
    cx: &mut VisualTestContext,
    id: &str,
) -> Bounds<Pixels> {
    let text = view.read_with(cx, |view, _| view.probe.rect(id));
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

// The pane's centre, where a person's pointer sits when they scroll it.
pub(super) fn inside(
    view: &gpui::Entity<SettingsView>,
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    rect(view, cx, "pane").center()
}

// One platform scroll event at the pointer, with no motion before it; the window draws only if it asked to.
pub(super) fn send(
    cx: &mut VisualTestContext,
    at: Point<Pixels>,
    delta: ScrollDelta,
    phase: TouchPhase,
) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        modifiers: Modifiers::none(),
        touch_phase: phase,
    });
    cx.run_until_parked();
}

pub(super) fn notch(cx: &mut VisualTestContext, at: Point<Pixels>, notches: f32) {
    send(
        cx,
        at,
        ScrollDelta::Lines(point(0., NOTCH_DOWN * notches)),
        TouchPhase::Moved,
    );
}

// A finger stroke as the pinned platform sends it: a zero Started, its moves, a zero Ended at the lift.
pub(super) fn stroke(cx: &mut VisualTestContext, at: Point<Pixels>, moves: &[f32]) {
    let zero = ScrollDelta::Pixels(point(px(0.), px(0.)));
    send(cx, at, zero, TouchPhase::Started);
    for &y in moves {
        send(
            cx,
            at,
            ScrollDelta::Pixels(point(px(0.), px(y))),
            TouchPhase::Moved,
        );
    }
    send(cx, at, zero, TouchPhase::Ended);
}

pub(super) fn scroll(view: &gpui::Entity<SettingsView>, cx: &mut VisualTestContext) -> f32 {
    view.read_with(cx, |view, _| view.panel.scroll)
}

// How far the pane can scroll: its rows' heights less the pane's inside.
pub(super) fn most(view: &gpui::Entity<SettingsView>, cx: &mut VisualTestContext) -> f32 {
    view.read_with(cx, |view, _| {
        (view.panel.heights().iter().sum::<f32>() - view.panel.pane.1).max(0.)
    })
}

// A row's drawn top, from the frame the window last painted.
pub(super) fn drawn_top(
    view: &gpui::Entity<SettingsView>,
    cx: &mut VisualTestContext,
    at: usize,
) -> f32 {
    let id = view.read_with(cx, |view, _| view.panel.rows[at].id.clone());
    f32::from(rect(view, cx, &format!("row:{id}")).top())
}

// The results as the pane draws them while a query stands: each line's row id and the result it highlights.
pub(super) fn result_lines(
    view: &gpui::Entity<SettingsView>,
    cx: &mut VisualTestContext,
) -> Vec<(String, Option<usize>)> {
    view.read_with(cx, |view, _| {
        let panel = &view.panel;
        let mut out = Vec::new();
        let mut hit = 0;
        for group in &panel.results {
            let rows = panel.everything_rows(group.section);
            if let Some(heading) = group.heading {
                let own = group.hits.is_empty().then_some(hit);
                out.push((rows[heading].id.to_string(), own));
            }
            if group.hits.is_empty() {
                hit += 1;
            }
            for found in &group.hits {
                out.push((rows[found.row].id.to_string(), Some(hit)));
                hit += 1;
            }
        }
        out
    })
}

// A result line whose row id no other line shares, so its probe rect is that line's.
pub(super) fn lone(lines: &[(String, Option<usize>)], at: usize) -> bool {
    lines.iter().filter(|(id, _)| *id == lines[at].0).count() == 1
}

pub(super) fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.
}

// A window-sized listing under the overlay that counts the wheel events reaching it.
struct Host {
    wheels: Rc<RefCell<usize>>,
    overlay: AnyView,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wheels = Rc::clone(&self.wheels);
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id("listing")
                    .absolute()
                    .size_full()
                    .on_scroll_wheel(move |_, _, _| *wheels.borrow_mut() += 1),
            )
            .child(div().absolute().size_full().child(self.overlay.clone()))
    }
}

type Hosted<'a> = (
    gpui::Entity<SettingsView>,
    Rc<RefCell<usize>>,
    &'a mut VisualTestContext,
);

// The Settings card alone over a listing in an 1100 by 700 window, opened on one section and drawn.
fn hosted<'a>(cx: &'a mut TestAppContext, section: &'static str) -> Hosted<'a> {
    let wheels: Rc<RefCell<usize>> = Rc::default();
    let seen = Rc::clone(&wheels);
    let built: Rc<RefCell<Option<gpui::Entity<SettingsView>>>> = Rc::default();
    let keep = Rc::clone(&built);
    let (_, cx) = cx.add_window_view(move |_, cx| {
        let overlay = cx.new(|cx| {
            let mut view = SettingsView::new(panel("{}"), cx);
            view.panel.show_section(section);
            view
        });
        *keep.borrow_mut() = Some(overlay.clone());
        Host {
            wheels: seen,
            overlay: overlay.into(),
        }
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    draw(cx);
    let view = built.borrow_mut().take().expect("settings built");
    (view, wheels, cx)
}

#[gpui::test]
fn a_wheel_notch_over_the_pane_moves_it_as_far_as_v0312_does(cx: &mut TestAppContext) {
    let (view, wheels, cx) = hosted(cx, "menus");
    let at = inside(&view, cx);
    assert!(
        most(&view, cx) > NOTCH_PX,
        "Menus scrolls further than one notch"
    );
    notch(cx, at, 1.);
    assert_eq!(scroll(&view, cx), NOTCH_PX, "one notch down");
    // A high-resolution wheel sends notches in parts, and the parts add up to the notch.
    notch(cx, at, -0.5);
    notch(cx, at, -0.5);
    assert_eq!(scroll(&view, cx), 0., "one notch back up in two halves");
    assert_eq!(*wheels.borrow(), 0, "the window beneath sees no wheel");
}

#[gpui::test]
fn a_touchpad_stroke_moves_the_pane_as_far_as_v0312_does(cx: &mut TestAppContext) {
    let (view, wheels, cx) = hosted(cx, "menus");
    let at = inside(&view, cx);
    stroke(cx, at, &[-12., -12., -24.]);
    let travel = 48. * RAW_PER_PLATFORM_PX * TOUCH_GAIN;
    assert!(
        (scroll(&view, cx) - travel).abs() < 0.01,
        "the stroke's raw travel, gained: {} for {travel}",
        scroll(&view, cx)
    );
    stroke(cx, at, &[30., 30.]);
    assert_eq!(scroll(&view, cx), 0., "back up and held at the top");
    assert_eq!(*wheels.borrow(), 0, "the window beneath sees no stroke");
}

#[gpui::test]
fn the_wheel_reaches_the_last_row_and_holds_both_ends(cx: &mut TestAppContext) {
    for section in ["menus", "view", "preview", "about"] {
        let (view, wheels, cx) = hosted(cx, section);
        let at = inside(&view, cx);
        let most = most(&view, cx);
        if most == 0. {
            continue;
        }
        for _ in 0..6 {
            notch(cx, at, 1.);
        }
        assert_eq!(
            scroll(&view, cx),
            most,
            "{section}: the wheel stops at the end"
        );
        notch(cx, at, 1.);
        stroke(cx, at, &[-40.]);
        assert_eq!(scroll(&view, cx), most, "{section}: no drift past the end");
        let (last, fade) = view.read_with(cx, |view, _| {
            let heights = view.panel.heights();
            (
                view.panel.rows.last().expect("rows").id.clone(),
                fade_shown(&heights, view.panel.scroll, view.panel.pane.1),
            )
        });
        assert!(!fade, "{section}: no fade at the end");
        let pane = rect(&view, cx, "pane");
        let row = rect(&view, cx, &format!("row:{last}"));
        // Probe rects are whole pixels, so the feet agree to a pixel.
        let gap = f32::from(pane.bottom()) - PANE_FOOT - f32::from(row.bottom());
        assert!(
            gap.abs() <= 1.,
            "{section}: the last row's foot sits on the pane's foot, {gap} px apart"
        );
        for _ in 0..6 {
            notch(cx, at, -1.);
        }
        stroke(cx, at, &[40.]);
        assert_eq!(scroll(&view, cx), 0., "{section}: no drift past the top");
        assert_eq!(
            *wheels.borrow(),
            0,
            "{section}: the window beneath sees nothing"
        );
    }
}

#[gpui::test]
fn keys_keep_the_drawn_cursor_row_inside_the_pane(cx: &mut TestAppContext) {
    let (view, _, cx) = hosted(cx, "menus");
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    draw(cx);
    let steps = view.read_with(cx, |view, _| view.panel.rows.len());
    for _ in 0..steps {
        cx.simulate_keystrokes("down");
        draw(cx);
        let id = view.read_with(cx, |view, _| view.panel.rows[view.panel.cursor].id.clone());
        let pane = rect(&view, cx, "pane");
        let row = rect(&view, cx, &format!("row:{id}"));
        assert!(
            row.top() >= pane.top() && row.bottom() <= pane.bottom() - px(PANE_FOOT),
            "{id} drawn inside the pane: {row:?} in {pane:?}"
        );
    }
    assert_eq!(scroll(&view, cx), most(&view, cx), "Down walks to the end");
    for _ in 0..steps {
        cx.simulate_keystrokes("up");
    }
    draw(cx);
    assert_eq!(scroll(&view, cx), 0., "Up walks back to the top");
}

#[gpui::test]
fn keys_bring_the_highlighted_result_into_view(cx: &mut TestAppContext) {
    let (view, wheels, cx) = hosted(cx, "view");
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.simulate_keystrokes("/ s");
    draw(cx);
    let lines = result_lines(&view, cx);
    let hits = lines
        .iter()
        .filter_map(|(_, hit)| *hit)
        .max()
        .expect("results")
        + 1;
    let pane = rect(&view, cx, "pane");
    let first = (0..lines.len())
        .find(|&at| lone(&lines, at))
        .expect("a first result");
    let first = format!("row:{}", lines[first].0);
    let top = rect(&view, cx, &first).top();
    let mut below = 0;
    for _ in 1..hits {
        cx.simulate_keystrokes("down");
        draw(cx);
        let hit = view.read_with(cx, |view, _| view.panel.hit);
        let Some(at) = lines.iter().position(|(_, line)| *line == Some(hit)) else {
            continue;
        };
        if !lone(&lines, at) {
            continue;
        }
        let row = rect(&view, cx, &format!("row:{}", lines[at].0));
        assert!(
            row.top() >= pane.top() && row.bottom() <= pane.bottom() - px(PANE_FOOT),
            "ST_SEARCH_KEYS: result {hit} drawn inside the pane: {row:?} in {pane:?}"
        );
        if rect(&view, cx, &first).top() < top {
            below += 1;
        }
    }
    assert!(
        below > 0,
        "Down walks past the first screen of {hits} results"
    );
    // The wheel holds the results' ends, as it holds a section's.
    let at = inside(&view, cx);
    for _ in 0..40 {
        notch(cx, at, 1.);
    }
    let last = (0..lines.len())
        .rev()
        .find(|&at| lone(&lines, at))
        .expect("a last result");
    let last = format!("row:{}", lines[last].0);
    let held = rect(&view, cx, &last);
    assert!(
        held.bottom() <= pane.bottom() - px(PANE_FOOT) + px(1.),
        "the wheel reaches the last result: {held:?} in {pane:?}"
    );
    notch(cx, at, 1.);
    stroke(cx, at, &[-40.]);
    assert_eq!(
        rect(&view, cx, &last),
        held,
        "the wheel holds the last result"
    );
    for _ in 1..hits {
        cx.simulate_keystrokes("up");
    }
    draw(cx);
    assert_eq!(
        rect(&view, cx, &first).top(),
        top,
        "Up walks back to the first result"
    );
    assert_eq!(*wheels.borrow(), 0, "the window beneath sees no wheel");
}
