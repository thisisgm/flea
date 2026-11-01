//! EY-362: the mouse side buttons over Trash follow WindowBody.qml, Back closes Trash and Forward is refused.
use super::join_tests::{draw, opened, root, shell};
use crate::{list::bench::real_cx, navigation::history::Entry};
use gpui::{
    point, px, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, NavigationDirection,
    VisualTestContext,
};

fn press(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::window::root::Root>,
    direction: NavigationDirection,
) {
    let (position, button) = (point(px(550.), px(350.)), MouseButton::Navigate(direction));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_event(MouseDownEvent {
        position,
        button,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    visual.simulate_event(MouseUpEvent {
        position,
        button,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    visual.run_until_parked();
    draw(cx, window);
}

// The pane's place and its Back and Forward stacks.
fn travel(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::window::root::Root>,
) -> (String, Vec<String>, Vec<String>) {
    shell(cx, window, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let paths = |stack: &[Entry]| stack.iter().map(|entry| entry.path.clone()).collect();
        (
            host.pane.place().to_owned(),
            paths(&host.nav.history.back),
            paths(&host.nav.history.forward),
        )
    })
}

#[test]
fn ey362_mouse_forward_inside_trash_is_refused_and_back_only_closes_trash() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        let history = &mut shell.panes[0].as_mut().unwrap().nav.history;
        history.back.push(Entry::new("/older", "", 0));
        history.forward.push(Entry::new("/files/A", "", 0));
    });
    let before = travel(&mut cx, window);
    press(&mut cx, window, NavigationDirection::Forward);
    assert!(
        shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "Forward leaves Trash open"
    );
    assert_eq!(
        travel(&mut cx, window),
        before,
        "Forward inside Trash moves nothing under it, as Pane.qml goForward refuses"
    );
    press(&mut cx, window, NavigationDirection::Back);
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "Back closes Trash"
    );
    assert_eq!(
        travel(&mut cx, window),
        before,
        "closing Trash is all Back does, the folder under it stays"
    );
}
