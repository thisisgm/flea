//! A real Backspace in the mounted field, after a refused try, clears the dots that stand for the submitted secret.
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
