//! EY-301: Tabs.js dropOverlay closes Trash, so t inside it opens the new tab on the folder under it and shows the strip.
use super::join_tests::{call, draw, opened, root, shell};
use crate::{
    list::bench::real_cx,
    window::{dispatch::test_shell::german_press, pane::PaneHost},
};
use flea::protocol::Reply;

fn place(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::window::root::Root>,
) -> String {
    shell(cx, window, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    })
}

#[test]
fn ey301_t_inside_trash_opens_a_tab_on_the_folder_and_shows_the_strip() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    assert!(shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)));
    assert_eq!(place(&mut cx, window), "/files");
    cx.simulate_keystrokes(window.into(), "t");
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "tabCount", &[]), "2");
    assert_eq!(call(&mut cx, window, "tabIndex", &[]), "1");
    assert_eq!(
        place(&mut cx, window),
        "/files",
        "the new tab opens on the folder under Trash"
    );
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "the new tab leaves Trash, as Tabs.js dropOverlay closes it"
    );
    assert_eq!(call(&mut cx, window, "tabBarVisible", &[]), "true");
    assert!(
        shell(&mut cx, window, |shell, window, _| shell
            .focus
            .is_focused(window)),
        "the keyboard returns to the window, so [ reaches the tabs"
    );
    cx.simulate_keystrokes(window.into(), "[");
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "tabIndex", &[]),
        "0",
        "[ moves to the other tab"
    );
}

#[test]
fn ey301_a_tab_switch_inside_trash_closes_it_and_a_tab_on_the_other_side_leaves_it() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "t");
    draw(&mut cx, window);
    opened(&mut cx, window);
    assert!(shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)));
    shell(&mut cx, window, |shell, window, cx| {
        let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
        right
            .pane
            .receive(Reply::Listed {
                path: "/right".into(),
                total: 8,
            })
            .unwrap();
        shell.panes[1] = Some(right);
        shell.focus_side = 1;
        assert!(shell.tab_action("tabNew", window, cx), "side 1 opens a tab");
        assert_eq!(shell.strip.tabs[1].count(), 2, "the new tab is on side 1");
        shell.focus_side = 0;
    });
    assert!(
        shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "a tab opened on the other side leaves this side's Trash"
    );
    cx.simulate_keystrokes(window.into(), "[");
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "tabIndex", &[]), "0");
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "switching tabs leaves Trash, as Tabs.js dropOverlay closes it"
    );
}

// German types [ with AltGr at the US 8 key, which is text, and ü at the US [ key, which is the [ binding.
#[test]
fn altgr_text_inside_trash_keeps_it_where_the_plain_bracket_switches_tabs() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "t");
    draw(&mut cx, window);
    opened(&mut cx, window);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), &cx);
    visual.simulate_event(german_press("[", "[", Some("8"), true));
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "tabIndex", &[]),
        "1",
        "AltGr+8 moves no tab"
    );
    assert!(
        shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "AltGr+8 types [ and keeps Trash"
    );
    visual.simulate_event(german_press("[", "ü", Some("["), false));
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "tabIndex", &[]),
        "0",
        "ü moves to the other tab"
    );
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "ü switches tabs and leaves Trash"
    );
}
