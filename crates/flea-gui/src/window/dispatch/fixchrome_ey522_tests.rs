//! EY-522: the selected row keeps its focused look while a transfer card shows.
use super::{
    rename_tests::{close, draw, receive},
    test_shell::{open_root, row},
};
use crate::backend::Event;
use gpui::TestAppContext;

/// EY-522: a copy's Jobs card leaves the selected row's focused look in place, as v0.3.12's current pane does.
#[gpui::test]
fn fixchrome_ey522_a_running_copy_keeps_the_selected_row_focused(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    // The selected row draws the focused look only in the active window; a TestWindow starts inactive.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    super::test_shell::land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    draw(cx);
    receive(&shell, cx, Event::Operations(r#"{"t":"job","job":{"id":7,"kind":"copy","head":"Copying","file":"notes.txt","state":"running"}}"#.into()));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    draw(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(shell.jobs_card_showing(cx), "the copy's card shows");
        assert!(
            shell.focus.is_focused(window),
            "a Jobs card takes no focus, so the window keeps it"
        );
    });
    shell.read_with(cx, |shell, cx| {
        assert!(shell.jobs_card_showing(cx), "the copy's card shows");
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        assert_eq!(list.drawn_cursor, Some(0));
        assert!(
            list.drawn_focused,
            "the selected row keeps its accent edge and accent plate"
        );
    });
    // With no modal, the window's own focus is the only input that can drop the row's focused look.
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(shell.jobs_card_showing(cx) && !shell.wheel_blocked(cx));
        assert!(!shell.focus.is_focused(window));
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        assert!(
            !list.drawn_focused,
            "a showing Jobs card alone never draws the row focused"
        );
    });
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    draw(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(
            shell.focus.is_focused(window),
            "the window has its focus back"
        );
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        assert!(
            list.drawn_focused,
            "refocused, the row draws focused again beside the card"
        );
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_overlay_copy_to(false, "/files", window, cx)
        })
    });
    draw(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(shell.jobs_card_showing(cx), "the copy's card still shows");
        assert!(
            shell.modal_card_holds_keyboard(cx),
            "the Copy To card is open"
        );
        assert!(
            !shell.focus.is_focused(window),
            "the Copy To field holds the keyboard"
        );
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        assert!(
            !list.drawn_focused,
            "a field opened over the Jobs card takes the row's focused look"
        );
    });
    close(&shell, cx);
}
