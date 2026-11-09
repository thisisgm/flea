//! Picker pointer regressions use the mounted chooser and platform mouse and wheel events.
use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase};

pub(super) fn seed(f: &mut Fixture) {
    f.cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":1000,"path":"/d/sub"}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
}

pub(super) fn path_click(f: &mut Fixture, padding: bool) {
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let bounds =
        f.cx.update(|_, cx| path.read(cx).paint.borrow().strips[0].unwrap());
    let x = if padding {
        bounds.right() - px(2.)
    } else {
        bounds.left() + px(5.)
    };
    f.cx.simulate_click(point(x, bounds.center().y), Modifiers::none());
    f.cx.run_until_parked();
    f.cx.update(|window, cx| {
        let path = path.read(cx);
        assert!(
            path.holds_keyboard(window),
            "a single click opens and focuses location entry"
        );
        assert_eq!(path.entry.text, "/d/sub/");
        assert_eq!(path.entry.selected, 0..path.entry.text.len());
    });
}

fn check_path(cx: &mut TestAppContext, save: bool) {
    let request = if save {
        r#"{"mode":"save","folder":"/d/sub","name":"report.pdf"}"#
    } else {
        r#"{"folder":"/d/sub"}"#
    };
    let mut f = fixture(cx, request, None);
    seed(&mut f);
    for mode in ["list", "grid"] {
        f.update(|picker, cx| picker.set_view(mode, cx));
        f.cx.run_until_parked();
        for padding in [false, true] {
            let sent = f.listing.sent().len();
            path_click(&mut f, padding);
            assert!(
                !f.listing.sent()[sent..]
                    .iter()
                    .any(|line| line.contains("\"c\":\"peek\"")),
                "clicking a parent run must not navigate"
            );
            let path = f.update(|picker, cx| picker.browse.path_view(cx));
            let at =
                f.cx.update(|_, cx| path.read(cx).paint.borrow().frame.unwrap().center());
            f.cx.simulate_click(at, Modifiers::none());
            f.cx.run_until_parked();
            f.cx.update(|window, cx| {
                let path = path.read(cx);
                assert!(
                    path.holds_keyboard(window),
                    "a second click keeps the entry focused"
                );
                assert!(
                    path.entry.selected.is_empty(),
                    "the field receives its own caret press"
                );
            });
            f.cx.simulate_keystrokes("escape");
            f.cx.run_until_parked();
            assert!(!f.read(|picker| picker.answered()));
            assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/sub");
            if save {
                assert_eq!(
                    f.read(|picker| picker.listing.save_name.clone()),
                    "report.pdf"
                );
            }
        }
    }
}

#[gpui::test]
fn bfx_picker_open_path_single_click_and_padding(cx: &mut TestAppContext) {
    check_path(cx, false);
}

#[gpui::test]
fn bfx_picker_save_path_single_click_preserves_filename(cx: &mut TestAppContext) {
    check_path(cx, true);
}

pub(super) fn wheel(f: &mut Fixture, delta: ScrollDelta, control: bool, phase: TouchPhase) {
    let at = f.cx.debug_bounds("pk2-listing").unwrap().center();
    let modifiers = Modifiers {
        control,
        ..Modifiers::none()
    };
    f.cx.simulate_mouse_move(at, None, modifiers);
    f.cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        modifiers,
        touch_phase: phase,
    });
    f.cx.run_until_parked();
}

pub(super) fn thumb(f: &mut Fixture) -> &'static str {
    f.update(|picker, cx| picker.browse.listing.read(cx).grid_view().thumb.name())
}

pub(super) fn notch(f: &mut Fixture, lines: f32, control: bool) {
    wheel(
        f,
        ScrollDelta::Lines(point(0., lines)),
        control,
        TouchPhase::Moved,
    );
}

pub(super) fn grid(f: &mut Fixture) {
    seed(f);
    f.update(|picker, cx| picker.set_view("grid", cx));
    f.cx.run_until_parked();
}

#[gpui::test]
fn bfx_picker_grid_wheel_accumulates_and_walks_all_six_stops(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d/sub","name":"report.pdf"}"#,
        None,
    );
    grid(&mut f);
    assert_eq!(thumb(&mut f), "medium");
    notch(&mut f, 1.5, true);
    assert_eq!(thumb(&mut f), "medium", "partial notch accumulates");
    notch(&mut f, 1.5, true);
    assert_eq!(thumb(&mut f), "large");
    for size in ["xlarge", "huge", "largest", "largest"] {
        notch(&mut f, 3., true);
        assert_eq!(thumb(&mut f), size);
    }
    for size in ["huge", "xlarge", "large", "medium", "small", "small"] {
        notch(&mut f, -3., true);
        assert_eq!(thumb(&mut f), size);
    }
    assert_eq!(f.read(|picker| picker.listing.cursor), 0);
    let filename = f.read(|picker| picker.browse.filename.clone().unwrap());
    f.cx.update(|window, cx| {
        let field = filename.read(cx);
        assert!(
            field.focus.is_focused(window),
            "wheel zoom keeps Filename focus"
        );
        assert_eq!(
            field.entry.selected,
            0..6,
            "wheel zoom keeps the selected stem"
        );
    });
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "report.pdf"
    );
    assert!(f.written().is_none());
}

#[gpui::test]
fn bfx_picker_grid_touchpad_zoom_and_plain_wheel_scroll(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    for phase in [TouchPhase::Started, TouchPhase::Moved] {
        wheel(
            &mut f,
            ScrollDelta::Pixels(point(px(0.), px(72.))),
            true,
            phase,
        );
        if phase == TouchPhase::Started {
            assert_eq!(thumb(&mut f), "medium");
        }
    }
    assert_eq!(
        thumb(&mut f),
        "large",
        "48 raw touchpad pixels form one zoom step"
    );
    notch(&mut f, -3., false);
    assert_eq!(thumb(&mut f), "large");
    let scrolled = f.update(|picker, cx| picker.browse.listing.read(cx).scroll_y());
    assert!(scrolled > 0., "ordinary wheel still scrolls");
    let stroke = -72.;
    wheel(
        &mut f,
        ScrollDelta::Pixels(point(px(0.), px(stroke as f32))),
        false,
        TouchPhase::Moved,
    );
    let moved = f.update(|picker, cx| picker.browse.listing.read(cx).scroll_y()) - scrolled;
    let travel = -crate::input::scroll::touch_distance(crate::input::scroll::touchpad_raw(stroke));
    assert_eq!(thumb(&mut f), "large");
    assert!(
        (moved - travel as f32).abs() < 0.01 && moved > 0.,
        "a plain two-finger stroke scrolls the grid {travel}px, downwards like the wheel, not {moved}px"
    );
}

// Sample sent line: {"c":"window","start":0,"count":256}
fn asked_window(line: &str) -> Option<(i64, i64)> {
    let rest = line.strip_prefix(r#"{"c":"window","start":"#)?;
    let (start, count) = rest.strip_suffix('}')?.split_once(r#","count":"#)?;
    Some((start.parse().ok()?, count.parse().ok()?))
}

#[gpui::test]
fn bfx_picker_grid_zoom_out_requests_newly_visible_rows(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    notch(&mut f, 12., true);
    assert_eq!(thumb(&mut f), "largest");
    // Fifty notches down first, so zooming out lands well past the rows a largest-size window would ask.
    notch(&mut f, -150., false);
    let from = f.listing.sent().len();
    notch(&mut f, -15., true);
    assert_eq!(thumb(&mut f), "small");
    let (start, count) = f.listing.sent()[from..]
        .iter()
        .rev()
        .find_map(|line| asked_window(line))
        .expect("zooming out asks for newly visible rows without requiring a scroll");
    let (first, last) = f.update(|picker, cx| {
        let view = picker.browse.listing.read(cx);
        let grid = view.grid_view();
        grid.layout(crate::grid::Shown::All(view.listing().total()))
            .viewport(grid.scroll)
    });
    assert!(first > 0, "the zoom-out lands below the folder top");
    assert!(
        start <= first.max(0) && start + count > last,
        "the ask {start}+{count} covers the small grid's visible items {first}..={last}"
    );
}

#[gpui::test]
fn bfx_picker_grid_zoom_respects_preference_and_surface(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    let off = flea::jsondoc::parse(r#"{"preview":{"ctrlZoom":false}}"#).unwrap();
    f.update(|picker, cx| picker.apply_settings(&off, cx));
    notch(&mut f, -3., true);
    assert_eq!(thumb(&mut f), "medium");
    assert!(
        f.update(|picker, cx| picker.browse.listing.read(cx).scroll_y()) > 0.,
        "disabling Ctrl+wheel zoom keeps ordinary scrolling"
    );
    let on = flea::jsondoc::parse(r#"{"preview":{"ctrlZoom":true}}"#).unwrap();
    f.update(|picker, cx| picker.apply_settings(&on, cx));
    f.cx.run_until_parked();
    let at = f.cx.debug_bounds("picker-grid-view").unwrap().center();
    let modifiers = Modifiers {
        control: true,
        ..Modifiers::none()
    };
    f.cx.simulate_mouse_move(at, None, modifiers);
    f.cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., 3.)),
        modifiers,
        ..Default::default()
    });
    f.cx.run_until_parked();
    assert_eq!(thumb(&mut f), "medium", "chrome does not own grid zoom");
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large");
    f.update(|picker, cx| picker.set_view("list", cx));
    f.cx.run_until_parked();
    notch(&mut f, 3., true);
    assert_eq!(thumb(&mut f), "large", "List does not own thumbnail zoom");
    f.update(|picker, cx| picker.set_view("grid", cx));
    f.update(|picker, cx| picker.backend_failed(cx));
    notch(&mut f, 3., true);
    assert_eq!(
        thumb(&mut f),
        "large",
        "an unavailable picker cannot change size"
    );
    let from = f.listing.sent().len();
    let at = f.cx.debug_bounds("picker-path-slot").unwrap().center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    assert!(!f.cx.update(|_, cx| path.read(cx).editing()));
    assert_eq!(
        f.listing.sent().len(),
        from,
        "an unavailable picker cannot open the location input"
    );
}
