//! Every card control's rest, hover, keyboard focus, pressed and disabled forms.
use super::boards::rect;
use super::scenes::*;
use super::*;
use flea_gui::g2::pv1::Hit;

fn english() -> flea::i18n::I18n {
    crate::test_catalogs::english()
}

fn centre(cx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.debug_bounds(selector).unwrap().center()
}

#[gpui::test]
fn close_rests_bare_rings_on_focus_shrinks_pressed_and_closes_on_release(cx: &mut TestAppContext) {
    let (cx, seen, look, events) = host(cx, ql_text(&english()), Arm::Fix);
    assert!(cx.debug_bounds("pv1-ring").is_none());
    look.update(cx, |look, cx| look.action("focusNext", cx));
    cx.run_until_parked();
    let (x, y, _, _) = rect(cx, "pv1-close");
    assert_eq!(rect(cx, "pv1-ring"), (x + 2., y + 3., 20., 20.));
    look.update(cx, |look, cx| look.action("focusNext", cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("pv1-ring").is_none());
    let at = centre(cx, "pv1-close");
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        look.read_with(cx, |look, _| look.pressed()),
        Some(Hit::Close)
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*events.borrow(), [Event::Closed]);
    assert_eq!((seen.borrow().cursor, seen.borrow().selection), (0, 0));
}

#[gpui::test]
fn a_press_released_off_close_closes_nothing_and_clears(cx: &mut TestAppContext) {
    let (cx, _, look, events) = host(cx, ql_text(&english()), Arm::Fix);
    let at = centre(cx, "pv1-close");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(
        at - point(px(200.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(look.read_with(cx, |look, _| look.pressed()), None);
    assert_eq!(*events.borrow(), []);
}

#[gpui::test]
fn the_pdf_strips_disabled_zooms_answer_nothing_and_fit_expands(cx: &mut TestAppContext) {
    let (cx, _, look, _) = host(cx, ql_error(&english()), Arm::Fix);
    let plus = centre(cx, "pv1-disabled-pv1-mark:plus");
    cx.simulate_click(plus, Modifiers::none());
    cx.run_until_parked();
    assert!(!look.read_with(cx, |look, _| look.is_expanded() || look.pressed().is_some()));
    let fit = centre(cx, "pv1-control-2");
    cx.simulate_click(fit, Modifiers::none());
    cx.run_until_parked();
    assert!(look.read_with(cx, |look, _| look.is_expanded()));
    assert_eq!(rect(cx, "pv1-card"), (0., 0., 1100., 700.));
}

#[gpui::test]
fn open_with_hovers_presses_focuses_and_answers(cx: &mut TestAppContext) {
    let (cx, _, look, events) = host(cx, ql_unsupported(&english()), Arm::Fix);
    let (bx, by, bw, bh) = rect(cx, "pv1-open-with");
    assert_eq!(rect(cx, "pv1-open-with-frame"), (bx, by, bw, bh));
    let at = centre(cx, "pv1-open-with");
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        look.read_with(cx, |look, _| look.hovered()),
        Some(Hit::OpenWith)
    );
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    // pressed, the frame insets by 2% of each side, rounded
    let dx = (bw * 0.02).round();
    assert_eq!(
        rect(cx, "pv1-open-with-frame"),
        (bx + dx, by + 1., bw - 2. * dx, bh - 2.)
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*events.borrow(), [Event::OpenWith]);
    // Tab reaches it with no bar to walk, and Return answers it
    look.update(cx, |look, cx| look.action("focusNext", cx));
    assert_eq!(
        look.read_with(cx, |look, _| look.focused()),
        Some(Hit::OpenWith)
    );
    look.update(cx, |look, cx| look.action("open", cx));
    assert_eq!(*events.borrow(), [Event::OpenWith, Event::OpenWith]);
}

#[gpui::test]
fn compare_walks_copy_patch_then_close(cx: &mut TestAppContext) {
    let (cx, _, look, events) = host(cx, compare_diff(&english()), Arm::Fix);
    let walk: Vec<_> = (0..3)
        .map(|_| {
            look.update(cx, |look, cx| {
                look.action("focusNext", cx);
                look.focused()
            })
        })
        .collect();
    assert_eq!(
        walk,
        [Some(Hit::CopyPatch), Some(Hit::Close), Some(Hit::CopyPatch)]
    );
    look.update(cx, |look, cx| look.action("open", cx));
    look.update(cx, |look, cx| look.action("copyPatch", cx));
    assert_eq!(*events.borrow(), [Event::CopyPatch, Event::CopyPatch]);
    look.update(cx, |look, cx| look.action("focusPrevious", cx));
    assert_eq!(
        look.read_with(cx, |look, _| look.focused()),
        Some(Hit::Close)
    );
    cx.run_until_parked();
    look.update(cx, |look, cx| look.action("open", cx));
    assert_eq!(events.borrow().last(), Some(&Event::Closed));
}

#[gpui::test]
fn ql_error_tab_walks_maximize_open_with_and_close(cx: &mut TestAppContext) {
    let (cx, _, look, events) = host(cx, ql_error(&english()), Arm::Fix);
    let walk: Vec<_> = (0..4)
        .map(|_| {
            look.update(cx, |look, cx| {
                look.action("focusNext", cx);
                look.focused()
            })
        })
        .collect();
    // PdfViewer's walk skips the disabled zooms; Open With, the pane's one action, sits before Close
    assert_eq!(
        walk,
        [
            Some(Hit::Control(2)),
            Some(Hit::OpenWith),
            Some(Hit::Close),
            Some(Hit::Control(2))
        ]
    );
    cx.run_until_parked();
    // the focused Maximize draws its ring inside its own slot
    let ring = cx.debug_bounds("pv1-ring").unwrap();
    assert!(cx
        .debug_bounds("pv1-control-2")
        .unwrap()
        .contains(&ring.center()));
    look.update(cx, |look, cx| look.action("open", cx));
    assert!(look.read_with(cx, |look, _| look.is_expanded()));
    look.update(cx, |look, cx| {
        look.action("focusNext", cx);
        look.action("open", cx)
    });
    assert_eq!(*events.borrow(), [Event::OpenWith]);
    look.update(cx, |look, cx| look.action("focusPrevious", cx));
    assert_eq!(
        look.read_with(cx, |look, _| look.focused()),
        Some(Hit::Control(2))
    );
    // Space closes every kind, the strip's focus notwithstanding
    look.update(cx, |look, cx| look.action("preview", cx));
    assert_eq!(events.borrow().last(), Some(&Event::Closed));
}
