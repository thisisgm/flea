//! The ShareBrowser overlay in a real window: what a frame draws, what a still frame shapes, and what the IPC reads back.
use super::ipc::BrowserAdapter;
use super::view::{row_at, Probe, ShapedShares, ShareBrowserElement, ShareBrowserView};
use crate::ipc::Adapter;
use crate::list::bench::{bench_typeface, real_cx};
use crate::list::Palette;
use flea::ui_logic_mn1::fit::ROW;
use flea::ui_logic_sd1::share::ShareBrowser;
use gpui::{
    div, point, prelude::*, px, size, Bounds, Context, IntoElement, Modifiers, MouseButton, Render,
    ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, Window,
    WindowHandle,
};
use std::{cell::Cell, cell::RefCell, rc::Rc};

struct Frame {
    browser: Rc<RefCell<ShareBrowser>>,
    rows: Rc<RefCell<ShapedShares>>,
    probe: Rc<RefCell<Probe>>,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let element = ShareBrowserElement::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            bench_typeface(),
            palette(),
        );
        div().size_full().child(element)
    }
}

// The theme roles the overlay reads, in the test's own colours: ink on a dark ground with the accent.
fn palette() -> Palette {
    Palette::inherited(
        &gpui::TextStyle::default(),
        crate::palette::Theme::from_text(
            "background='#1a1b26'\nforeground='#a9b1d6'\naccent='#7aa2f7'",
        ),
    )
}

type Parts = (
    WindowHandle<Frame>,
    Rc<RefCell<ShareBrowser>>,
    Rc<RefCell<ShapedShares>>,
    Rc<RefCell<Probe>>,
);

fn open(cx: &mut TestAppContext) -> Parts {
    let browser = Rc::new(RefCell::new(ShareBrowser::default()));
    let rows = Rc::new(RefCell::new(ShapedShares::default()));
    let probe = Rc::new(RefCell::new(Probe::default()));
    let (held_browser, held_rows, held_probe) =
        (Rc::clone(&browser), Rc::clone(&rows), Rc::clone(&probe));
    let window = cx.open_window(size(px(900.), px(200.)), |_, _| Frame {
        browser: held_browser,
        rows: held_rows,
        probe: held_probe,
    });
    (window, browser, rows, probe)
}

fn frame<V: Render>(cx: &mut TestAppContext, window: WindowHandle<V>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the overlay window is open");
}

fn draw(visual: &mut VisualTestContext) {
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn open_browser(browser: &Rc<RefCell<ShareBrowser>>, names: &[&str]) {
    let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    browser.borrow_mut().open("smb://nas", "nas", &names);
}

#[test]
fn a_shut_overlay_draws_nothing_and_reports_no_rect() {
    let mut cx = real_cx();
    let (window, _, _, probe) = open(&mut cx);
    frame(&mut cx, window);
    let probe = probe.borrow();
    assert!(
        probe.bounds.is_none(),
        "a shut overlay is invisible, so it has no rect"
    );
    assert_eq!(probe.painted, 0);
    assert_eq!(probe.picked, None);
}

#[test]
fn an_open_overlay_draws_one_row_per_share_with_the_cursor_picked() {
    let mut cx = real_cx();
    let (window, browser, _, probe) = open(&mut cx);
    open_browser(&browser, &["media", "docs"]);
    frame(&mut cx, window);
    let probe = probe.borrow();
    assert!(probe.bounds.is_some());
    assert_eq!(probe.painted, 2, "one server-marked row per share");
    assert_eq!(probe.picked, Some(0), "the keyboard cursor is the pick");
    assert_eq!(probe.hovered, None);
}

#[test]
fn the_pointer_lifts_the_row_under_it_and_never_moves_the_pick() {
    let mut cx = real_cx();
    let (window, browser, _, probe) = open(&mut cx);
    open_browser(&browser, &["media", "docs"]);
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    draw(&mut visual);
    visual.simulate_mouse_move(
        point(px(450.), px(ROW as f32 * 1.5)),
        None,
        Modifiers::none(),
    );
    draw(&mut visual);
    let probe = probe.borrow();
    assert_eq!(probe.hovered, Some(1));
    assert_eq!(
        probe.picked,
        Some(0),
        "hover lifts a row, the cursor stays where the keys left it"
    );
}

// The listing under the overlay, counting any press or wheel that reaches it past the opaque ground.
struct Under {
    browser: Rc<RefCell<ShareBrowser>>,
    rows: Rc<RefCell<ShapedShares>>,
    probe: Rc<RefCell<Probe>>,
    left: Rc<Cell<usize>>,
    right: Rc<Cell<usize>>,
    wheels: Rc<Cell<usize>>,
}

impl Render for Under {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let overlay = ShareBrowserElement::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            bench_typeface(),
            palette(),
        );
        let (left, right, wheels) = (
            Rc::clone(&self.left),
            Rc::clone(&self.right),
            Rc::clone(&self.wheels),
        );
        div()
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, _| left.set(left.get() + 1))
            .on_mouse_down(MouseButton::Right, move |_, _, _| {
                right.set(right.get() + 1)
            })
            .on_scroll_wheel(move |_, _, _| wheels.set(wheels.get() + 1))
            .child(overlay)
    }
}

// r2-2: ui/ShareBrowser.qml sinks gap clicks and wheels and picks a clicked row.
#[test]
fn r2_h_the_open_overlay_takes_its_own_clicks_wheel_and_hover() {
    let mut cx = real_cx();
    let browser = Rc::new(RefCell::new(ShareBrowser::default()));
    let (left, right, wheels) = (
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    let probe = Rc::new(RefCell::new(Probe::default()));
    let (b, l, r, w, p) = (
        Rc::clone(&browser),
        Rc::clone(&left),
        Rc::clone(&right),
        Rc::clone(&wheels),
        Rc::clone(&probe),
    );
    let window = cx.open_window(size(px(900.), px(300.)), |_, _| Under {
        browser: b,
        rows: Rc::new(RefCell::new(ShapedShares::default())),
        probe: p,
        left: l,
        right: r,
        wheels: w,
    });
    browser.borrow_mut().open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    draw(&mut visual);
    assert_eq!(probe.borrow().painted, 2, "the overlay is drawn");
    let gap = point(px(450.), px(250.));
    visual.simulate_click(gap, Modifiers::none());
    visual.simulate_event(ScrollWheelEvent {
        position: gap,
        delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    visual.simulate_mouse_down(gap, MouseButton::Right, Modifiers::none());
    visual.simulate_mouse_up(gap, MouseButton::Right, Modifiers::none());
    let second = point(px(450.), px(ROW as f32 * 1.5));
    visual.simulate_mouse_move(second, None, Modifiers::none());
    draw(&mut visual);
    visual.simulate_click(second, Modifiers::none());
    draw(&mut visual);
    let report = format!(
        "clicks through={} right through={} wheels through={} cursor={} hovered={:?}",
        left.get(),
        right.get(),
        wheels.get(),
        browser.borrow().cursor,
        probe.borrow().hovered
    );
    assert_eq!(
        (left.get(), right.get(), wheels.get(), browser.borrow().cursor, probe.borrow().hovered),
        (0, 0, 0, 1, Some(1)),
        "ui/ShareBrowser.qml sinks the gap click and the wheel, lifts the row under the pointer and picks a clicked row: {report}"
    );
    assert_eq!(
        probe.borrow().pick,
        Some(("smb://nas/docs/".to_string(), "docs".to_string())),
        "the clicked row's share is published for the host"
    );
}

#[test]
fn a_still_frame_shapes_nothing_and_a_new_list_shapes_once() {
    let mut cx = real_cx();
    let (window, browser, rows, _) = open(&mut cx);
    open_browser(&browser, &["media", "docs"]);
    frame(&mut cx, window);
    assert_eq!(rows.borrow().reshapes, 1);
    frame(&mut cx, window);
    assert_eq!(
        rows.borrow().reshapes,
        1,
        "an unchanged list is not shaped again"
    );
    open_browser(&browser, &["media", "docs", "archive"]);
    frame(&mut cx, window);
    assert_eq!(rows.borrow().reshapes, 2, "a changed list is shaped once");
    assert_eq!(rows.borrow().len(), 3);
}

#[test]
fn a_pointer_height_maps_to_its_row_and_misses_above_and_below() {
    let bounds = Bounds::new(point(px(0.), px(10.)), size(px(300.), px(200.)));
    let row = ROW as f32;
    assert_eq!(row_at(bounds, px(10.), 2), Some(0));
    assert_eq!(row_at(bounds, px(10. + row), 2), Some(1));
    assert_eq!(
        row_at(bounds, px(10. + 2. * row), 2),
        None,
        "past the last share"
    );
    assert_eq!(row_at(bounds, px(4.), 2), None, "above the overlay");
}

#[test]
fn the_ipc_readers_answer_the_drawn_overlay() {
    let mut cx = real_cx();
    let (window, browser, _, probe) = open(&mut cx);
    open_browser(&browser, &["media", "docs"]);
    frame(&mut cx, window);
    let browser_ref = browser.borrow();
    let probe_ref = probe.borrow();
    let adapter = BrowserAdapter {
        browser: &browser_ref,
        probe: &probe_ref,
        origin: (0., 0.),
    };
    let answer = |function: &str| {
        adapter
            .call(function, &[])
            .expect("the reader is answered")
            .stdout
    };
    assert_eq!(answer("shareBrowserOpen"), "true\n");
    assert_eq!(answer("shareBrowserEntries"), "media\ndocs\n");
    assert_eq!(answer("shareBrowserCursor"), "0\n");
    assert_eq!(answer("shareBrowserRect"), "0 0 900 200\n");
    assert_eq!(
        answer("shareBrowserState"),
        "{\"active\":true,\"baseUri\":\"smb://nas\",\"cursor\":0,\"rect\":\"0 0 900 200\"}\n"
    );
    assert!(adapter.call("shareBrowserMissing", &[]).is_none());
}

// The listing beneath the overlay, counting the pointer motion and hover reaching it past the ground.

#[path = "view_motion_tests.rs"]
mod motion;
