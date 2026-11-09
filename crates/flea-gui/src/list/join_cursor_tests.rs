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
            // The keyboard's window is the active one; a TestWindow starts inactive.
            window.activate_window();
        })
        .unwrap();
    cx.run_until_parked();
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

// HIG drag-cut-move: the window that is not active draws its cursor grey even though its Shell keeps the focus handle.
#[test]
fn an_inactive_window_draws_its_cursor_grey_while_it_keeps_the_focus_handle() {
    let mut cx = bench::real_cx();
    let source = open(&mut cx);
    let receiver = open(&mut cx);
    for handle in [source, receiver] {
        handle
            .update(&mut cx, |shell, window, cx| window.focus(&shell.focus, cx))
            .unwrap();
    }
    // The bench theme's accent equals its foreground, so a distinct accent tells the two washes apart.
    cx.update(|cx| {
        let accent = crate::palette::Theme::from_text("background='#00aa66'").background;
        let theme = cx.try_global::<crate::palette::Theme>().copied();
        cx.set_global(crate::palette::Theme {
            accent,
            accent_frame: accent,
            ..theme.unwrap_or_default()
        });
    });
    let mut painted_washes = Vec::new();
    for active in [source, receiver, source] {
        active
            .update(&mut cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.run_until_parked();
        for handle in [source, receiver] {
            let expected = handle == active;
            let (window_active, row_focused) = handle
                .update(&mut cx, |shell, window, cx| {
                    assert!(shell.focus.is_focused(window), "the focus handle stays");
                    (
                        window.is_window_active(),
                        shell.list_states(0, window, cx).focused,
                    )
                })
                .unwrap();
            assert_eq!(window_active, expected, "the real activation changed");
            assert_eq!(
                row_focused, expected,
                "an inactive window must not claim the keyboard cursor"
            );
            frame(&mut cx, handle);
            assert_eq!(painted(&mut cx, handle, 0), (Some(0), expected));
            let washes = handle
                .update(&mut cx, |shell, window, _| {
                    let palette = shell.panes[0].as_ref().unwrap().list.borrow().drawn.palette;
                    let palette = palette.expect("a painted palette");
                    let count = |wash: gpui::Hsla| {
                        let wash = format!("{wash:?}");
                        window
                            .painted_quads()
                            .iter()
                            .filter(|quad| format!("{:?}", quad.background).contains(&wash))
                            .count()
                    };
                    (
                        count(palette.accent.opacity(crate::list::CURSOR_WASH)),
                        count(palette.foreground.opacity(crate::list::GREY_CURSOR)),
                    )
                })
                .unwrap();
            painted_washes.push((handle, expected, washes));
        }
    }
    // The rail's lit place shares the grey plate, so the list cursor shows as one wash moving between the two counts.
    for handle in [source, receiver] {
        let leg = |active: bool| {
            painted_washes
                .iter()
                .find(|(each, expected, _)| *each == handle && *expected == active)
                .map(|(_, _, washes)| *washes)
                .expect("both legs drew")
        };
        let ((accent_on, grey_on), (accent_off, grey_off)) = (leg(true), leg(false));
        assert_eq!(
            accent_off, 0,
            "an inactive window painted an accent cursor wash"
        );
        assert_eq!(
            accent_on, 1,
            "the active window's cursor lost its accent wash"
        );
        assert_eq!(
            grey_off,
            grey_on + 1,
            "the inactive cursor did not take the grey wash"
        );
    }
}
