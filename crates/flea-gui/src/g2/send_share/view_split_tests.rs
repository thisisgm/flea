//! The ShareBrowser overlay beside another pane: a press that starts off the overlay never picks a row.
use super::view::{Probe, ShapedShares, ShareBrowserElement};
use crate::list::bench::{bench_typeface, real_cx};
use crate::list::Palette;
use flea::ui_logic_context_menus::fit::ROW;
use flea::ui_logic_send_share::share::ShareBrowser;
use gpui::{
    div, point, prelude::*, px, size, Context, IntoElement, Modifiers, MouseButton, Render,
    VisualTestContext, Window,
};
use std::{cell::Cell, cell::RefCell, rc::Rc};

fn palette() -> Palette {
    Palette::inherited(
        &gpui::TextStyle::default(),
        crate::palette::Theme::from_text(
            "background='#1a1b26'\nforeground='#a9b1d6'\naccent='#7aa2f7'",
        ),
    )
}

// A window with the other pane on the left (300 px) and the overlay over the right listing slot.
struct Split {
    browser: Rc<RefCell<ShareBrowser>>,
    rows: Rc<RefCell<ShapedShares>>,
    probe: Rc<RefCell<Probe>>,
    presses: Rc<Cell<usize>>,
}

impl Render for Split {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let overlay = ShareBrowserElement::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            bench_typeface(),
            palette(),
        );
        let presses = Rc::clone(&self.presses);
        div()
            .flex()
            .flex_row()
            .size_full()
            .child(
                div()
                    .w(px(300.))
                    .h_full()
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        presses.set(presses.get() + 1)
                    }),
            )
            .child(div().flex_1().h_full().child(overlay))
    }
}

fn draw(visual: &mut VisualTestContext) {
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

// r4-1: ui/MenuRow.qml ReleaseWithinBounds: a press dragged off or started off the overlay never picks a row.
#[test]
fn r4_a_a_press_that_started_off_the_overlay_never_picks_a_row() {
    let mut cx = real_cx();
    let browser = Rc::new(RefCell::new(ShareBrowser::default()));
    let probe = Rc::new(RefCell::new(Probe::default()));
    let presses = Rc::new(Cell::new(0));
    let (b, p, n) = (Rc::clone(&browser), Rc::clone(&probe), Rc::clone(&presses));
    let window = cx.open_window(size(px(900.), px(300.)), |_, _| Split {
        browser: b,
        rows: Rc::new(RefCell::new(ShapedShares::default())),
        probe: p,
        presses: n,
    });
    browser.borrow_mut().open(
        "smb://nas",
        "nas",
        &["media".to_string(), "docs".to_string()],
    );
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    draw(&mut visual);
    assert_eq!(probe.borrow().painted, 2, "the overlay is drawn");
    let row = point(px(600.), px(ROW as f32 * 1.5));
    let pane = point(px(100.), px(ROW as f32 * 1.5));
    let left = MouseButton::Left;
    let none = Modifiers::none();
    // Control: a press on the other pane, dragged onto row 1 and released there, picks nothing.
    visual.simulate_mouse_move(pane, None, none);
    draw(&mut visual);
    visual.simulate_mouse_down(pane, left, none);
    draw(&mut visual);
    visual.simulate_mouse_move(row, Some(left), none);
    draw(&mut visual);
    visual.simulate_mouse_up(row, left, none);
    draw(&mut visual);
    let control = (
        browser.borrow().cursor,
        probe.borrow().pick.clone(),
        presses.get(),
    );
    assert_eq!(
        control,
        (0, None, 1),
        "control: a pane press released over a row picked {control:?}"
    );
    // A press on row 1, dragged off the overlay and released on the other pane, picks nothing.
    visual.simulate_mouse_move(row, None, none);
    draw(&mut visual);
    visual.simulate_mouse_down(row, left, none);
    draw(&mut visual);
    visual.simulate_mouse_move(pane, Some(left), none);
    draw(&mut visual);
    visual.simulate_mouse_up(pane, left, none);
    draw(&mut visual);
    let dragged_off = (
        browser.borrow().cursor,
        probe.borrow().pick.clone(),
        probe.borrow().pressed,
    );
    // The control's gesture again: a press on the other pane released over row 1.
    visual.simulate_mouse_down(pane, left, none);
    draw(&mut visual);
    visual.simulate_mouse_move(row, Some(left), none);
    draw(&mut visual);
    visual.simulate_mouse_up(row, left, none);
    draw(&mut visual);
    let after = (browser.borrow().cursor, probe.borrow().pick.clone());
    assert_eq!(
        (dragged_off.0, dragged_off.1.clone(), after.clone()),
        (0, None, (0, None)),
        "a press that started off the overlay picked a row: after the off-overlay release {dragged_off:?}, after the pane press released over row 1 {after:?}"
    );
}
