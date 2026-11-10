//! S11J-7, S11J-3 and S11J-2: once the keyboard leaves the Trash list and comes back, it is Trash's list again.
use super::join_tests::{draw, shell};
use super::trashview_tests::{enter, mounted, settle};
use crate::{list::bench::real_cx, window::root::Root};
use gpui::{TestAppContext, WindowHandle};

fn trash_holds_keys(cx: &mut TestAppContext, window: WindowHandle<Root>) -> bool {
    shell(cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        pane.focus.contains_focused(window, cx)
    })
}

fn edge(cx: &mut TestAppContext, window: WindowHandle<Root>) -> bool {
    draw(cx, window);
    shell(cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .paint
            .borrow()
            .edge
    })
}

fn stop(cx: &mut TestAppContext, window: WindowHandle<Root>) -> &'static str {
    shell(cx, window, |shell, window, cx| shell.focus_view(window, cx))
}

fn trash_page(cx: &mut TestAppContext) -> WindowHandle<Root> {
    let window = mounted(cx);
    cx.simulate_keystrokes(window.into(), "delete");
    draw(cx, window);
    enter(cx, window);
    settle(cx, window, 2);
    assert!(trash_holds_keys(cx, window), "Trash opens on its list");
    assert!(
        edge(cx, window),
        "the list holds the keyboard, so the cursor is lit"
    );
    window
}

// Delete, F2 and Menu act on the Trash list, never on file-1.txt under the page.
fn list_keys_stay_on_trash(cx: &mut TestAppContext, window: WindowHandle<Root>) {
    cx.simulate_keystrokes(window.into(), "f2");
    draw(cx, window);
    shell(cx, window, |shell, _, _| {
        assert_eq!(
            shell.renaming_row(0),
            None,
            "F2 opens no editor on the hidden folder"
        );
    });
    cx.simulate_keystrokes(window.into(), "menu");
    draw(cx, window);
    shell(cx, window, |shell, _, cx| {
        assert!(
            !shell.menu_opened(),
            "Menu opens no menu for the hidden folder's file"
        );
        let host = shell.trash.as_ref().unwrap();
        assert!(
            host.menu.borrow().opened(),
            "Menu opens the Trash row's menu"
        );
        assert!(host.pane.read(cx).view().opened);
    });
    cx.simulate_keystrokes(window.into(), "escape");
    draw(cx, window);
    let before = shell(cx, window, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.held.total
    });
    cx.simulate_keystrokes(window.into(), "delete");
    draw(cx, window);
    shell(cx, window, |shell, _, cx| {
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        assert!(view.opened, "Delete keeps the Trash page");
        assert_eq!(
            view.pending_op, "prepare",
            "Delete reviews the Trash row for permanent deletion"
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.held.total, before);
    });
}

#[test]
fn s11j7_the_ring_back_to_the_list_lands_on_trash_and_skips_the_hidden_toolbar() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx);
    cx.simulate_keystrokes(window.into(), "tab");
    draw(&mut cx, window);
    assert_eq!(
        stop(&mut cx, window),
        "rail",
        "Tab leaves the Trash list for the rail"
    );
    assert!(
        !edge(&mut cx, window),
        "the cursor drops its accent while the rail has the keyboard"
    );
    let mut stops = Vec::new();
    for _ in 0..4 {
        if trash_holds_keys(&mut cx, window) {
            break;
        }
        cx.simulate_keystrokes(window.into(), "tab");
        draw(&mut cx, window);
        stops.push(stop(&mut cx, window));
    }
    assert!(
        !stops.contains(&"chrome"),
        "the folder toolbar under the page is not a stop: {stops:?}"
    );
    assert!(
        trash_holds_keys(&mut cx, window),
        "the ring comes back to the Trash list: {stops:?}"
    );
    assert!(
        edge(&mut cx, window),
        "the cursor lights again with the keyboard"
    );
    list_keys_stay_on_trash(&mut cx, window);
}

#[test]
fn s11j7_escape_from_the_rail_returns_to_the_trash_list() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx);
    cx.simulate_keystrokes(window.into(), "tab");
    draw(&mut cx, window);
    assert_eq!(stop(&mut cx, window), "rail");
    cx.simulate_keystrokes(window.into(), "escape");
    draw(&mut cx, window);
    assert!(
        trash_holds_keys(&mut cx, window),
        "Escape hands the keyboard to Trash's list"
    );
    list_keys_stay_on_trash(&mut cx, window);
}

#[test]
fn s11j3_a_toolbar_glyph_holding_the_keys_under_the_page_hands_them_to_the_trash_list() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx);
    shell(&mut cx, window, |shell, window, cx| {
        window.focus(&shell.dual_focus.chrome, cx)
    });
    draw(&mut cx, window);
    assert!(
        trash_holds_keys(&mut cx, window),
        "the undrawn toolbar lets go: {}",
        stop(&mut cx, window)
    );
    list_keys_stay_on_trash(&mut cx, window);
}

#[test]
fn s11j3_escape_from_a_path_typed_under_the_page_hands_the_keys_to_the_trash_list() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx);
    cx.simulate_keystrokes(window.into(), "ctrl-l");
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        let chrome = shell.chrome.view.as_ref().expect("mounted chrome").read(cx);
        assert!(
            chrome.path.read(cx).editing(),
            "Ctrl+L opens the path editor"
        );
        assert!(
            shell.dual_focus.chrome.contains_focused(window, cx),
            "the open editor keeps the keys"
        );
    });
    cx.simulate_keystrokes(window.into(), "escape");
    draw(&mut cx, window);
    assert!(
        trash_holds_keys(&mut cx, window),
        "the closed editor lets go: {}",
        stop(&mut cx, window)
    );
    list_keys_stay_on_trash(&mut cx, window);
}

#[test]
fn s11j7_a_key_on_the_window_handle_reaches_the_trash_list_that_covers_the_folder() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx);
    shell(&mut cx, window, |shell, window, cx| {
        window.focus(&shell.focus, cx)
    });
    draw(&mut cx, window);
    list_keys_stay_on_trash(&mut cx, window);
    assert!(trash_holds_keys(&mut cx, window));
}

fn cursor_ground(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
) -> (Option<gpui::Hsla>, gpui::Hsla, gpui::Hsla) {
    draw(cx, window);
    shell(cx, window, |shell, _, cx| {
        let paint = shell.trash.as_ref().unwrap().pane.read(cx).paint.borrow();
        let ink = paint.ink;
        (paint.ground(paint.cursor), ink.foreground, ink.mark_fill)
    })
}

#[test]
fn ff3_the_cursor_row_greys_as_a_folder_row_does_while_the_rail_has_the_keys() {
    let mut cx = real_cx();
    let window = trash_page(&mut cx);
    cx.simulate_keystrokes(window.into(), "tab");
    assert_eq!(stop(&mut cx, window), "rail");
    let (ground, foreground, marked) = cursor_ground(&mut cx, window);
    assert_eq!(
        ground,
        Some(foreground.opacity(crate::list::GREY_CURSOR)),
        "a folder list's unfocused cursor is the foreground at 18%, not the 35% marked plate"
    );
    assert_ne!(ground, Some(marked));
    cx.simulate_keystrokes(window.into(), "escape");
    assert!(trash_holds_keys(&mut cx, window));
    assert!(
        edge(&mut cx, window),
        "the accent comes back with the keyboard"
    );
}
