//! The share overlay retains pointer and motion ownership.
use super::*;

struct MotionUnder {
    browser: Rc<RefCell<ShareBrowser>>,
    rows: Rc<RefCell<ShapedShares>>,
    probe: Rc<RefCell<Probe>>,
    moves: Rc<Cell<usize>>,
    hovered: Rc<Cell<bool>>,
}

impl Render for MotionUnder {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let overlay = ShareBrowserElement::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            bench_typeface(),
            palette(),
        );
        let (moves, hovered) = (Rc::clone(&self.moves), Rc::clone(&self.hovered));
        div()
            .id("under")
            .size_full()
            .on_mouse_move(move |_, _, _| moves.set(moves.get() + 1))
            .on_hover(move |now: &bool, _, _| hovered.set(*now))
            .child(overlay)
    }
}

type MotionParts = (
    VisualTestContext,
    Rc<RefCell<ShareBrowser>>,
    Rc<RefCell<Probe>>,
    Rc<Cell<usize>>,
    Rc<Cell<bool>>,
);

fn open_motion() -> MotionParts {
    let mut cx = real_cx();
    let browser = Rc::new(RefCell::new(ShareBrowser::default()));
    let probe = Rc::new(RefCell::new(Probe::default()));
    let (moves, hovered) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(false)));
    let (b, p, m, h) = (
        Rc::clone(&browser),
        Rc::clone(&probe),
        Rc::clone(&moves),
        Rc::clone(&hovered),
    );
    let window = cx.open_window(size(px(900.), px(300.)), |_, _| MotionUnder {
        browser: b,
        rows: Rc::new(RefCell::new(ShapedShares::default())),
        probe: p,
        moves: m,
        hovered: h,
    });
    let visual = VisualTestContext::from_window(window.into(), &cx);
    (visual, browser, probe, moves, hovered)
}

// r3-2: ui/ShareBrowser.qml's ground has hoverEnabled: true, so no motion or hover reaches the listing beneath.
#[test]
fn r3_a_the_open_overlay_keeps_motion_and_hover_from_the_listing_beneath() {
    let (mut visual, browser, probe, moves, hovered) = open_motion();
    draw(&mut visual);
    visual.simulate_mouse_move(point(px(450.), px(250.)), None, Modifiers::none());
    draw(&mut visual);
    let control = (moves.get(), hovered.get());
    assert!(
        control.0 > 0 && control.1,
        "control: a move over the shut overlay reaches the listing {control:?}"
    );
    browser.borrow_mut().open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    draw(&mut visual);
    let before = moves.get();
    visual.simulate_mouse_move(point(px(400.), px(260.)), None, Modifiers::none());
    draw(&mut visual);
    visual.simulate_mouse_move(
        point(px(450.), px(ROW as f32 * 1.5)),
        None,
        Modifiers::none(),
    );
    draw(&mut visual);
    let seen = (moves.get() - before, hovered.get(), probe.borrow().hovered);
    assert_eq!(
        seen,
        (0, false, Some(1)),
        "motion or hover reached the listing beneath the open overlay: {seen:?}"
    );
}

// r3-1: ui/MenuRow.qml ReleaseWithinBounds: a press alone or dragged off its row picks nothing.
#[test]
fn r3_b_a_press_alone_or_dragged_off_picks_no_share() {
    let (mut visual, browser, probe, _, _) = open_motion();
    browser.borrow_mut().open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    draw(&mut visual);
    let row = point(px(450.), px(ROW as f32 * 1.5));
    visual.simulate_mouse_move(row, None, Modifiers::none());
    draw(&mut visual);
    visual.simulate_mouse_down(row, MouseButton::Left, Modifiers::none());
    draw(&mut visual);
    let after_press = (browser.borrow().cursor, probe.borrow().pick.clone());
    let gap = point(px(450.), px(250.));
    visual.simulate_mouse_move(gap, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(gap, MouseButton::Left, Modifiers::none());
    draw(&mut visual);
    let after_release = (browser.borrow().cursor, probe.borrow().pick.clone());
    assert_eq!(
        (after_press.clone(), after_release.clone()),
        ((0, None), (0, None)),
        "a press picked before its release: after press {after_press:?}, after an off-row release {after_release:?}"
    );
    visual.simulate_mouse_move(row, None, Modifiers::none());
    draw(&mut visual);
    visual.simulate_mouse_down(row, MouseButton::Left, Modifiers::none());
    draw(&mut visual);
    visual.simulate_mouse_up(row, MouseButton::Left, Modifiers::none());
    draw(&mut visual);
    assert_eq!(
        (browser.borrow().cursor, probe.borrow().pick.clone()),
        (1, Some(("smb://nas/docs/".to_string(), "docs".to_string()))),
        "a release over the row the press started on picks its share"
    );
}

// r3-5: the mountable view draws the overlay for its Slots.states side, so the G1 mount shows the shares.
#[test]
fn r3_e_the_mountable_view_draws_the_overlay_for_its_slot() {
    let mut cx = real_cx();
    let browser = Rc::new(RefCell::new(ShareBrowser::default()));
    let rows = Rc::new(RefCell::new(ShapedShares::default()));
    let probe = Rc::new(RefCell::new(Probe::default()));
    let (b, r, p) = (Rc::clone(&browser), Rc::clone(&rows), Rc::clone(&probe));
    let window = cx.open_window(size(px(900.), px(200.)), |_, _| {
        ShareBrowserView::new(b, r, p, bench_typeface(), palette())
    });
    browser.borrow_mut().open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    frame(&mut cx, window);
    let probe = probe.borrow();
    assert!(
        probe.bounds.is_some(),
        "the mounted view has a rect for its slot"
    );
    assert_eq!(probe.painted, 2, "one server-marked row per share");
    assert_eq!(probe.picked, Some(0), "the keyboard cursor is the pick");
}
