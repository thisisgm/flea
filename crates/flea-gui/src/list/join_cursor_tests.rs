//! The keyboard's pane draws its cursor as the accent rung; a pane without the keyboard, or under a card, draws grey.
use super::{frame, open};
use crate::{backend::Backend, list::bench, state::Shell, window::startup::View};
use gpui::{TestAppContext, WindowHandle};

fn painted(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    side: usize,
) -> (Option<usize>, bool) {
    handle
        .update(cx, |shell, _, _| {
            let list = shell.panes[side].as_ref().unwrap().list.borrow();
            (list.drawn_cursor, list.drawn_focused)
        })
        .unwrap()
}

fn close_card(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            let overlay = shell.overlay.clone().expect("an overlay");
            overlay.update(cx, |card, cx| {
                let catalog = card.catalog.clone();
                card.model.key(&catalog, "escape", false);
                card.sync_focus(window, cx);
            });
            assert!(!shell.card_holds_keyboard(cx), "the card did not close");
            assert!(
                shell.overlay.is_some(),
                "the overlay entity must outlive its card"
            );
        })
        .unwrap();
}

#[test]
fn u04_the_painted_cursor_follows_the_keyboard_across_panes_and_cards() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    let (spare, _requests) = Backend::test_channel();
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.panes[1] = crate::state::tests::shell_with(spare, cx).panes[0].take();
            shell.panes[1].as_ref().unwrap().list.borrow_mut().typeface = bench::bench_typeface();
            shell.view = View::Dual;
            window.focus(&shell.focus, cx);
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(
        painted(&mut cx, handle, 0),
        (Some(0), true),
        "the keyboard's pane drew grey"
    );
    assert_eq!(
        painted(&mut cx, handle, 1),
        (Some(0), false),
        "the other pane drew the accent rung"
    );
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.open_overlay_copy_to(false, "/files", window, cx);
            assert!(shell.card_holds_keyboard(cx));
        })
        .unwrap();
    frame(&mut cx, handle);
    assert!(
        !painted(&mut cx, handle, 0).1,
        "a card holds the keyboard, yet the list drew the accent rung"
    );
    close_card(&mut cx, handle);
    frame(&mut cx, handle);
    assert_eq!(
        painted(&mut cx, handle, 0),
        (Some(0), true),
        "a closed card left the list grey"
    );
    handle
        .update(&mut cx, |shell, _, cx| shell.focus_pane(1, cx))
        .unwrap();
    frame(&mut cx, handle);
    assert!(!painted(&mut cx, handle, 0).1);
    assert!(painted(&mut cx, handle, 1).1);
}
