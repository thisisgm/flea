//! Folder suggestions activate on release within the pressed row.
use super::view::{ranked, Bar};
use flea_gui::path::view::Out;
use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent, TestAppContext};

fn row(bar: &mut Bar<'_>, index: usize) -> gpui::Point<gpui::Pixels> {
    bar.read(|view| {
        let paint = view.paint.borrow();
        let popup = paint.popup.expect("mounted suggestions");
        point(
            px(popup.x + popup.width / 2.),
            px(popup.y + 1. + popup.scale.pad + (index as f32 + 0.5) * popup.scale.row),
        )
    })
}

fn press(bar: &mut Bar<'_>, at: gpui::Point<gpui::Pixels>) {
    bar.cx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    assert!(bar.take().is_empty(), "a press must not choose a folder");
    assert!(bar.read(|view| view.editing() && view.jump.shown()));
}

#[gpui::test]
fn ey174_a_row_chooses_only_on_release_inside_it(cx: &mut TestAppContext) {
    let mut bar = ranked(cx, "o");
    let at = row(&mut bar, 1);
    press(&mut bar, at);
    bar.cx
        .simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    assert_eq!(
        bar.take(),
        [
            Out::Closed,
            Out::Open {
                side: 0,
                path: "/srv/omarchy".into()
            }
        ]
    );
    assert!(!bar.read(|view| view.editing()));
}

#[gpui::test]
fn ey174_dragging_to_another_row_does_not_choose_either(cx: &mut TestAppContext) {
    let mut bar = ranked(cx, "o");
    let from = row(&mut bar, 0);
    let to = row(&mut bar, 1);
    press(&mut bar, from);
    bar.cx
        .simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    bar.cx
        .simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    assert!(bar.take().is_empty());
    assert!(bar.read(|view| view.editing() && view.jump.shown()));
}

#[gpui::test]
fn ey174_release_outside_cancels_the_press_and_enter_still_chooses(cx: &mut TestAppContext) {
    let mut bar = ranked(cx, "o");
    let from = row(&mut bar, 1);
    press(&mut bar, from);
    let outside = point(px(900.), px(300.));
    bar.cx
        .simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    bar.cx
        .simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert!(bar.take().is_empty());
    assert!(bar.read(|view| view.editing() && view.jump.shown()));
    bar.cx
        .simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    assert!(
        bar.take().is_empty(),
        "the cancelled press must not survive"
    );
    bar.cx.simulate_keystrokes("down enter");
    assert_eq!(
        bar.take(),
        [
            Out::Closed,
            Out::Open {
                side: 0,
                path: "/srv/omarchy".into()
            }
        ]
    );
}
