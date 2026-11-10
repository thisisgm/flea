//! The keyboard hides the pointer's row wash in Trash until the pointer really moves (#322).
use super::tests::{opened, row};
use crate::list::bench::real_cx;
use crate::list::hover_wash::HoverWash;
use gpui::{point, px, AppContext};

#[test]
fn a_key_hides_the_resting_pointer_wash_until_a_real_move() {
    let mut cx = real_cx();
    cx.update(|cx| cx.set_global(HoverWash(true)));
    let (window, _calls) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    let at = point(px(40.), px(60.));
    window
        .update(&mut cx, |pane, _, cx| {
            pane.pointer_moved(at, Some(0), cx);
            assert_eq!(pane.paint.borrow().hover, Some(0));
            pane.action("cursorDown", false, cx);
            assert_eq!(pane.paint.borrow().hover, None, "the key quiets the wash");
            pane.pointer_moved(at, Some(0), cx);
            assert_eq!(
                pane.paint.borrow().hover,
                None,
                "a move to the same point is not a move"
            );
            pane.pointer_moved(point(px(42.), px(60.)), Some(0), cx);
            assert_eq!(
                pane.paint.borrow().hover,
                Some(0),
                "a real move brings it back"
            );
        })
        .unwrap();
}

// 0401-list-nohover: unticked Highlight on hover, the fresh-install look, never washes a Trash row under the pointer.
#[test]
fn an_unticked_highlight_on_hover_never_washes_a_trash_row() {
    let mut cx = real_cx();
    let (window, _calls) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| {
            pane.pointer_moved(point(px(40.), px(90.)), Some(1), cx);
            assert_eq!(pane.paint.borrow().hover, Some(1), "the pointer fact stays");
            assert_eq!(pane.paint.borrow().ground(1), None);
            cx.set_global(HoverWash(true));
            pane.pointer_moved(point(px(42.), px(90.)), Some(1), cx);
            assert!(
                pane.paint.borrow().ground(1).is_some(),
                "ticked, the row washes"
            );
        })
        .unwrap();
}

#[test]
fn a_stationary_trash_pointer_follows_setting_changes_on_the_next_frame() {
    let mut cx = real_cx();
    let (handle, _calls) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    handle
        .update(&mut cx, |pane, _, cx| {
            pane.pointer_moved(point(px(40.), px(90.)), Some(1), cx);
        })
        .unwrap();
    for on in [true, false, true] {
        cx.update_window(handle.into(), |_, window, cx| {
            cx.set_global(HoverWash(on));
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        handle
            .update(&mut cx, |pane, _, _| {
                let paint = pane.paint.borrow();
                assert_eq!(paint.hover, Some(1));
                assert_eq!(paint.ground(1).is_some(), on);
            })
            .unwrap();
    }
}
