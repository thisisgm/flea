//! The Settings pane scrolled in the whole window as minipc runs it: Root over the cached Shell, Settings opened with its own key.
use super::geometry::PANE_FOOT;
use super::scroll_tests::{
    draw, drawn_top, inside, lone, most, near, notch, rect, result_lines, scroll, stroke, NOTCH_PX,
};
use super::view::SettingsView;
use crate::{
    backend::Backend,
    list::{bench, pane::Held},
    state::tests::shell_with,
    window::root::Root,
};
use gpui::{prelude::*, px, size, Entity, Modifiers, VisualTestContext, WindowHandle};
use std::rc::Rc;

struct Opened {
    handle: WindowHandle<Root>,
    view: Entity<SettingsView>,
    cx: VisualTestContext,
}

// The real font system at rem 14, a window of the given size, and a listed folder behind the card.
pub(super) fn window(width: f32, height: f32) -> (WindowHandle<Root>, VisualTestContext) {
    let mut app = bench::real_cx();
    app.update(|cx| {
        cx.set_global(crate::presentation::Fonts::from_sources(
            "[font]\nbase-size=14",
            "",
            "JetBrains Mono".into(),
        ))
    });
    let (backend, requests) = Backend::test_channel();
    std::mem::forget(requests);
    let handle = app.open_window(size(px(width), px(height)), |window, cx| {
        window.set_rem_size(crate::list::Typeface::omarchy_default().body);
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.launch.settings.data = flea::uischema::defaults();
            for host in shell.panes.iter_mut().flatten() {
                host.backend = None;
                host.pane.held = Rc::new(Held {
                    listing: host.pane.held.listing,
                    total: 100,
                    start: 0,
                    rows: bench::fixture("2k").into_iter().take(100).collect(),
                });
            }
            shell
        });
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    });
    let mut cx = VisualTestContext::from_window(handle.into(), &app);
    draw(&mut cx);
    (handle, cx)
}

fn opened(width: f32, height: f32, section: &'static str) -> Opened {
    let (handle, mut cx) = window(width, height);
    cx.simulate_keystrokes(",");
    draw(&mut cx);
    let view = handle
        .update(&mut cx, |root, _, cx| {
            root.shell
                .read(cx)
                .settings
                .panel
                .clone()
                .expect("settings opened")
        })
        .expect("window open");
    if view.read_with(&cx, |view, _| view.panel.section) != section {
        let rail = rect(&view, &mut cx, &format!("rail:{section}")).center();
        cx.simulate_click(rail, Modifiers::none());
        draw(&mut cx);
    }
    assert_eq!(view.read_with(&cx, |view, _| view.panel.section), section);
    Opened { handle, view, cx }
}

fn listing_scroll(opened: &mut Opened) -> f32 {
    opened
        .handle
        .update(&mut opened.cx, |root, _, cx| {
            root.shell.read(cx).panes[0].as_ref().unwrap().pane.scroll_y
        })
        .expect("window open")
}

// minipc's 2560 by 1440 output, less the bar: every section but Menus fits the card, and a search's results do not.
const MINIPC: (f32, f32) = (2560., 1400.);

#[test]
fn wheel_and_touchpad_scroll_the_search_results_in_a_minipc_window() {
    let mut opened = opened(MINIPC.0, MINIPC.1, "view");
    let listing = listing_scroll(&mut opened);
    let view = opened.view.clone();
    opened.cx.simulate_keystrokes("/ s");
    draw(&mut opened.cx);
    let lines = result_lines(&view, &mut opened.cx);
    let at = (0..lines.len())
        .find(|&at| lone(&lines, at))
        .expect("a result with its own id");
    let probe = format!("row:{}", lines[at].0);
    let top = f32::from(rect(&view, &mut opened.cx, &probe).top());
    let last = (0..lines.len())
        .rev()
        .find(|&at| lone(&lines, at))
        .expect("a last result");
    let foot = rect(&view, &mut opened.cx, &format!("row:{}", lines[last].0)).bottom();
    let pane = rect(&view, &mut opened.cx, "pane").bottom();
    assert!(
        f32::from(foot - pane) > NOTCH_PX + 40.,
        "the results overflow the pane: they end at {foot:?}, the pane at {pane:?}"
    );
    // The pointer stays where the hand left it; the wheel and the stroke arrive with no motion.
    let point = inside(&view, &mut opened.cx);
    notch(&mut opened.cx, point, 1.);
    let moved = f32::from(rect(&view, &mut opened.cx, &probe).top());
    assert!(
        near(moved, top - NOTCH_PX),
        "ST_SEARCH_WHEEL: a notch moves the drawn results {NOTCH_PX}, they moved {}",
        top - moved
    );
    stroke(&mut opened.cx, point, &[-12., -12., -24.]);
    let stroked = f32::from(rect(&view, &mut opened.cx, &probe).top());
    assert!(
        near(stroked, moved - 40.),
        "ST_SEARCH_TOUCH: the stroke moves the drawn results 40, they moved {}",
        moved - stroked
    );
    assert_eq!(
        listing_scroll(&mut opened),
        listing,
        "the listing behind never scrolls"
    );
}

#[test]
fn wheel_and_touchpad_scroll_menus_in_a_minipc_window() {
    let mut opened = opened(MINIPC.0, MINIPC.1, "menus");
    let listing = listing_scroll(&mut opened);
    let view = opened.view.clone();
    let at = inside(&view, &mut opened.cx);
    let top = drawn_top(&view, &mut opened.cx, 3);
    assert!(
        most(&view, &mut opened.cx) > NOTCH_PX + 40.,
        "Menus overflows the card"
    );
    notch(&mut opened.cx, at, 1.);
    assert!(
        near(drawn_top(&view, &mut opened.cx, 3), top - NOTCH_PX),
        "ST_WHEEL_DRAWN: a notch moves the drawn rows {NOTCH_PX}"
    );
    stroke(&mut opened.cx, at, &[-12., -12., -24.]);
    assert!(
        near(drawn_top(&view, &mut opened.cx, 3), top - NOTCH_PX - 40.),
        "ST_TOUCH_DRAWN: the stroke moves the drawn rows 40"
    );
    assert_eq!(
        listing_scroll(&mut opened),
        listing,
        "the listing behind never scrolls"
    );
}

#[test]
fn view_fits_a_minipc_window_and_holds_still_under_the_wheel() {
    let mut opened = opened(MINIPC.0, MINIPC.1, "view");
    let view = opened.view.clone();
    let at = inside(&view, &mut opened.cx);
    let top = drawn_top(&view, &mut opened.cx, 3);
    notch(&mut opened.cx, at, 1.);
    stroke(&mut opened.cx, at, &[-12.]);
    assert_eq!(
        scroll(&view, &mut opened.cx),
        0.,
        "the card is as tall as View"
    );
    assert_eq!(
        drawn_top(&view, &mut opened.cx, 3),
        top,
        "View's rows hold still"
    );
}

#[test]
fn a_resize_reveals_the_cursor_row_and_a_reopen_starts_at_the_top() {
    let mut opened = opened(900., 600., "menus");
    let view = opened.view.clone();
    let at = inside(&view, &mut opened.cx);
    notch(&mut opened.cx, at, 2.);
    assert!(scroll(&view, &mut opened.cx) > 0.);
    // v0.3.12's pane reveals the cursor row whenever its height changes.
    opened.cx.simulate_resize(size(px(900.), px(560.)));
    draw(&mut opened.cx);
    let id = view.read_with(&opened.cx, |view, _| {
        view.panel.rows[view.panel.cursor].id.clone()
    });
    let pane = rect(&view, &mut opened.cx, "pane");
    let row = rect(&view, &mut opened.cx, &format!("row:{id}"));
    assert!(
        row.top() >= pane.top() && row.bottom() <= pane.bottom() - px(PANE_FOOT),
        "the cursor row is drawn inside the pane after a resize: {row:?} in {pane:?}"
    );
    opened.cx.simulate_keystrokes("escape");
    draw(&mut opened.cx);
    opened.cx.simulate_keystrokes(",");
    draw(&mut opened.cx);
    assert_eq!(
        scroll(&view, &mut opened.cx),
        0.,
        "a reopen starts at the top"
    );
}
