//! The mounted field: a real Backspace after a refused try clears the held dots, and a secret the eye showed leaves no copy behind.
use super::{
    process::{Handle, Job, Outcome, Runner},
    unlock::Proc,
    view::UnlockView,
};
use gpui::TestAppContext;
use std::{rc::Rc, sync::Arc};

struct Quiet;
impl Runner for Quiet {
    fn start(&self, _: Job) -> Handle {
        Handle::probed().0
    }
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn backspace_after_a_refused_try_clears_the_held_dots(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        UnlockView::new(
            Arc::new(crate::test_catalogs::english()),
            Rc::new(Quiet),
            cx,
        )
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(cx, |v, w, cx| {
        v.open_for("/dev/test", "Test", 1024, "crypto_LUKS", w, cx)
    });
    cx.run_until_parked();
    draw(cx);
    cx.simulate_input("abcdefg");
    cx.simulate_keystrokes("enter");
    let refused = Outcome {
        code: 1,
        stdout: String::new(),
        stderr: "GDBus.Error:org.freedesktop.UDisks2.Error.Failed: x".into(),
    };
    view.update_in(cx, |v, _, cx| {
        v.host
            .card
            .as_mut()
            .unwrap()
            .exited(Proc::Unlock, &refused, 5);
        v.refresh(cx);
    });
    draw(cx);
    let dots = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |v, _| v.host.card.as_ref().unwrap().field.dots())
    };
    assert_eq!(dots(cx), 7, "the refused try stays drawn");
    cx.simulate_keystrokes("backspace");
    assert_eq!(dots(cx), 0, "Backspace clears the held dots in one press");
    cx.simulate_input("ab");
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        dots(cx),
        1,
        "then it deletes typed characters one at a time"
    );
}

fn revealed_ab(
    cx: &mut TestAppContext,
) -> (gpui::Entity<UnlockView>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        UnlockView::new(
            Arc::new(crate::test_catalogs::english()),
            Rc::new(Quiet),
            cx,
        )
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(cx, |v, w, cx| {
        v.open_for("/dev/test", "Test", 1024, "crypto_LUKS", w, cx)
    });
    cx.run_until_parked();
    draw(cx);
    cx.simulate_input("ab");
    view.update(cx, |v, cx| {
        v.host.card.as_mut().unwrap().eye_press();
        cx.notify();
    });
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.drawn.borrow().shaped_text.as_ref(),
            "ab",
            "the held eye draws the secret"
        )
    });
    (view, cx)
}

#[gpui::test]
fn a_paste_while_the_eye_is_held_hides_the_secret_again(cx: &mut TestAppContext) {
    let (view, cx) = revealed_ab(cx);
    cx.simulate_input("cd");
    draw(cx);
    view.read_with(cx, |v, _| {
        let field = &v.host.card.as_ref().unwrap().field;
        assert!(!field.revealed, "an edit releases the reveal");
        assert!(
            !field.entry.text.contains(['a', 'b', 'c', 'd']),
            "the entry holds only the mask"
        );
        assert!(!v.drawn.borrow().shaped_text.contains(['a', 'b', 'c', 'd']));
        assert_eq!(field.text.expose(), b"abcd", "the pasted text still lands");
    });
}

#[gpui::test]
fn closing_or_submitting_with_the_eye_held_drops_the_drawn_secret(cx: &mut TestAppContext) {
    for close in [true, false] {
        let (view, cx) = revealed_ab(cx);
        view.update(cx, |v, cx| if close { v.close(cx) } else { v.submit(cx) });
        cx.run_until_parked();
        view.read_with(cx, |v, _| {
            assert!(
                !v.drawn.borrow().shaped_text.contains(['a', 'b']),
                "no shaped copy of the secret outlives the field (close: {close})"
            )
        });
    }
}
