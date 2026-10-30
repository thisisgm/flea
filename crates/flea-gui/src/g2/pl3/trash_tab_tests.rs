//! EY-301: Tabs.js dropOverlay closes Trash, so t inside it opens the new tab on the folder under it and shows the strip.
use super::join_tests::{call, draw, opened, root, shell};
use crate::list::bench::real_cx;

#[test]
fn ey301_t_inside_trash_opens_a_tab_on_the_folder_and_shows_the_strip() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    assert!(shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)));
    cx.simulate_keystrokes(window.into(), "t");
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "tabCount", &[]), "2");
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "the new tab leaves Trash, as Tabs.js dropOverlay closes it"
    );
    assert_eq!(call(&mut cx, window, "tabBarVisible", &[]), "true");
    assert!(
        shell(&mut cx, window, |shell, window, _| shell.focus.is_focused(window)),
        "the keyboard returns to the window, so [ reaches the tabs"
    );
    cx.simulate_keystrokes(window.into(), "[");
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "tabIndex", &[]), "0", "[ moves to the other tab");
}
