//! The keyboard sheet scrolled through real input in the whole window, over the shipped keymap at rem 14.
use super::scroll_mounted_tests::window;
use super::scroll_tests::{draw, near, notch, stroke, NOTCH_PX};
use super::sheet_view::{SheetView, KEY_ROW};
use crate::window::root::Root;
use gpui::{Bounds, Entity, Pixels, Point, VisualTestContext, WindowHandle};

// minipc's 2560 by 1440 output, less the bar.
const MINIPC: (f32, f32) = (2560., 1400.);

struct Open {
    handle: WindowHandle<Root>,
    view: Entity<SheetView>,
    cx: VisualTestContext,
}

// The sheet as `?` opens it over a listed folder, with the preset's real bindings.
fn opened() -> Open {
    let (handle, mut cx) = window(MINIPC.0, MINIPC.1);
    cx.simulate_keystrokes("?");
    draw(&mut cx);
    let view = handle
        .update(&mut cx, |root, _, cx| {
            root.shell
                .read(cx)
                .settings
                .sheet
                .clone()
                .expect("sheet opened")
        })
        .expect("window open");
    Open { handle, view, cx }
}

fn bounds(open: &Open, id: &str) -> Bounds<Pixels> {
    open.view
        .read_with(&open.cx, |view, _| view.probe.bounds(id))
        .unwrap_or_else(|| panic!("{id} drawn"))
}

// How far the drawn content sits above the body's top.
fn scrolled(open: &Open) -> f32 {
    f32::from(bounds(open, "body").top() - bounds(open, "content").top())
}

fn most(open: &Open) -> f32 {
    f32::from(bounds(open, "content").size.height - bounds(open, "body").size.height)
}

fn at(open: &Open) -> Point<Pixels> {
    bounds(open, "body").center()
}

fn listing(open: &mut Open) -> f32 {
    open.handle
        .update(&mut open.cx, |root, _, cx| {
            root.shell.read(cx).panes[0].as_ref().unwrap().pane.scroll_y
        })
        .expect("window open")
}

fn searched(query: &str) -> Open {
    let mut open = opened();
    open.cx.simulate_keystrokes(query);
    draw(&mut open.cx);
    open
}

fn wheel_and_stroke(name: &str, mut open: Open) {
    let behind = listing(&mut open);
    assert!(
        most(&open) > NOTCH_PX + 40.,
        "{name}: the shipped keymap runs past the card by {}",
        most(&open)
    );
    let point = at(&open);
    notch(&mut open.cx, point, 1.);
    assert!(
        near(scrolled(&open), NOTCH_PX),
        "ST_SHEET_WHEEL {name}: a notch moves the drawn sheet {NOTCH_PX}, it moved {}",
        scrolled(&open)
    );
    stroke(&mut open.cx, point, &[-12., -12., -24.]);
    assert!(
        near(scrolled(&open), NOTCH_PX + 40.),
        "ST_SHEET_TOUCH {name}: the stroke moves the drawn sheet 40 more, it is at {}",
        scrolled(&open)
    );
    assert_eq!(
        listing(&mut open),
        behind,
        "{name}: the listing behind never scrolls"
    );
}

fn both_ends(name: &str, mut open: Open) {
    let point = at(&open);
    for _ in 0..12 {
        notch(&mut open.cx, point, 1.);
    }
    let end = bounds(&open, "content").bottom();
    let body = bounds(&open, "body");
    assert!(
        scrolled(&open) > 0. && near(f32::from(end), f32::from(body.bottom())),
        "ST_SHEET_END {name}: the last row sits on the body's foot: content ends {end:?}, body {body:?}"
    );
    notch(&mut open.cx, point, 1.);
    stroke(&mut open.cx, point, &[-24.]);
    assert_eq!(
        bounds(&open, "content").bottom(),
        end,
        "{name}: the end holds"
    );
    for _ in 0..12 {
        notch(&mut open.cx, point, -1.);
    }
    stroke(&mut open.cx, point, &[24.]);
    assert_eq!(scrolled(&open), 0., "{name}: the top holds");
}

#[test]
fn a_notch_and_a_stroke_move_the_resting_sheet_as_far_as_v0312_does() {
    wheel_and_stroke("rest", opened());
}

#[test]
fn a_notch_and_a_stroke_move_the_sheet_results_as_far_as_v0312_does() {
    wheel_and_stroke("results", searched("s"));
}

#[test]
fn the_wheel_reaches_the_resting_sheet_end_and_holds_both_ends() {
    both_ends("rest", opened());
}

#[test]
fn the_wheel_reaches_the_last_result_and_holds_both_ends() {
    both_ends("results", searched("s"));
}

#[test]
fn down_past_the_edge_keeps_the_highlighted_result_in_view() {
    let mut open = searched("s");
    let count = open
        .view
        .read_with(&open.cx, |view, _| view.sheet.ranked.len());
    let body = bounds(&open, "body");
    assert!(count as f32 * KEY_ROW > f32::from(body.size.height) + KEY_ROW);
    let check = |open: &Open, step: &str| {
        let cursor = open.view.read_with(&open.cx, |view, _| view.sheet.cursor);
        let top = f32::from(bounds(open, "content").top()) + cursor as f32 * KEY_ROW;
        assert!(
            top >= f32::from(body.top()) - 0.5 && top + KEY_ROW <= f32::from(body.bottom()) + 0.5,
            "ST_SHEET_KEYS {step}: result {cursor} drawn at {top} inside {body:?}"
        );
    };
    for _ in 1..count {
        open.cx.simulate_keystrokes("down");
        draw(&mut open.cx);
        check(&open, "down");
    }
    // The wheel back to the top leaves the highlight on the last result; Up walks from there and brings it back.
    let point = at(&open);
    for _ in 0..12 {
        notch(&mut open.cx, point, -1.);
    }
    open.cx.simulate_keystrokes("up");
    draw(&mut open.cx);
    check(&open, "up after the wheel");
    // Down past the last result wraps to the first, and the sheet follows it to the top.
    for _ in 0..2 {
        open.cx.simulate_keystrokes("down");
        draw(&mut open.cx);
        check(&open, "down to the wrap");
    }
    assert_eq!(
        open.view.read_with(&open.cx, |view, _| view.sheet.cursor),
        0
    );
    assert_eq!(
        scrolled(&open),
        0.,
        "the wrap to the first result scrolls to the top"
    );
}

const FADES: [&str; 2] = ["sheet-fade-0", "sheet-fade-1"];
const MASKS: [&str; 2] = ["sheet-mask-0", "sheet-mask-1"];

// Each resting column fades at its own foot while rows run below the body, as the keymap board draws, and not at the end.
#[test]
fn each_resting_column_fades_at_its_own_foot_until_the_wheel_reaches_the_end() {
    let mut open = opened();
    let frame = |open: &mut Open| {
        open.cx.update(|window, cx| window.simulate_next_frame(cx));
        draw(&mut open.cx);
    };
    frame(&mut open);
    let body = bounds(&open, "body");
    let fades: Vec<_> = (0..2)
        .map(|side| {
            open.cx
                .debug_bounds(FADES[side])
                .unwrap_or_else(|| panic!("ST_SHEET_FADE: column {side} fades at its foot"))
        })
        .collect();
    assert!(
        fades[0].right() < fades[1].left(),
        "ST_SHEET_FADE: one fade under each column, not one across the body: {fades:?}"
    );
    for (side, fade) in fades.iter().enumerate() {
        let last = bounds(&open, ["last-0", "last-1"][side]);
        let column = bounds(&open, ["column-0", "column-1"][side]);
        assert_eq!(
            last.bottom(),
            column.bottom(),
            "column {side}'s probed last row ends its column"
        );
        assert!(
            fade.bottom() == body.bottom()
                || (fade.bottom() < last.bottom() && near(f32::from(fade.bottom()), f32::from(last.center().y))),
            "ST_SHEET_FADE: column {side} ends on the body's foot or its drawn last row's half: {fade:?} {last:?} in {body:?}"
        );
        let mask = open
            .cx
            .debug_bounds(MASKS[side])
            .expect("the cut hides the rest of the column");
        assert_eq!(
            (mask.top(), mask.bottom()),
            (fade.bottom(), column.bottom()),
            "ST_SHEET_FADE: column {side} shows nothing under its cut"
        );
    }
    assert!(
        fades.iter().any(|fade| fade.bottom() == body.bottom()),
        "ST_SHEET_FADE: the longer column runs to the body's foot"
    );
    let point = at(&open);
    for _ in 0..12 {
        notch(&mut open.cx, point, 1.);
    }
    frame(&mut open);
    for side in 0..2 {
        assert!(
            open.cx.debug_bounds(FADES[side]).is_none(),
            "ST_SHEET_FADE: no fade on column {side} once the last row sits on the body's foot"
        );
    }
}
