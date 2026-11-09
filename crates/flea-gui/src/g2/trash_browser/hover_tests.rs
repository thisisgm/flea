//! The keyboard hides the pointer's row wash in Trash until the pointer really moves (#322).
use super::tests::{opened, row};
use crate::list::bench::real_cx;
use gpui::{point, px};

#[test]
fn a_key_hides_the_resting_pointer_wash_until_a_real_move() {
    let mut cx = real_cx();
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
