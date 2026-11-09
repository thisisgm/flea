use super::hero_tests::{english, frame};
use super::line::MessageLine;
use flea::ui_logic_status_feedback::status::Hint;
use gpui::{
    div, prelude::*, px, size, Context, Entity, Render, Subscription, TestAppContext,
    VisualTestContext, Window,
};
use std::sync::Arc;

/// The status composition's part of the seam: the strip, then the detail under it, redrawn when the lane changes.
struct Strip {
    line: Entity<MessageLine>,
    _watch: Subscription,
}

impl Render for Strip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().h(px(27.)).child(self.line.clone()))
            .children(self.line.read(cx).detail(window, cx))
    }
}

fn say_error(cx: &mut TestAppContext, line: &Entity<MessageLine>, detail: &str) {
    cx.update(|cx| {
        line.update(cx, |line, cx| {
            line.update(cx, |m, now| {
                m.say("Copy failed.", true, detail, Hint::None, now)
            })
        })
    });
}

/// StatusBar.qml draws an error's detail under the strip, wrapped in the caption size and capped at a third of the window.
#[gpui::test]
fn an_error_detail_is_drawn_wrapped_and_capped_at_a_third_of_the_window(cx: &mut TestAppContext) {
    let catalog = Arc::new(english());
    let handle = cx.add_window(|_, cx| {
        let line = cx.new(|cx| MessageLine::new("default", catalog, cx));
        let _watch = cx.observe(&line, |_, _, cx| cx.notify());
        Strip { line, _watch }
    });
    let line = handle.update(cx, |strip, _, _| strip.line.clone()).unwrap();
    cx.simulate_window_resize(handle.into(), size(px(400.), px(300.)));
    say_error(
        cx,
        &line,
        &"rsync: write failed on /mnt/nas/a.bin: No space left on device (28) ".repeat(40),
    );
    frame(cx, handle);
    frame(cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let detail = vcx.debug_bounds("fb1-detail").expect("the detail is drawn");
    assert_eq!((detail.origin.y, detail.size.width), (px(27.), px(400.)));
    assert_eq!(
        detail.size.height,
        px(100.),
        "a long detail stops at a third of the window and scrolls"
    );
    let text = vcx
        .debug_bounds("fb1-detail-text")
        .expect("the detail text is drawn");
    assert_eq!(
        (
            text.origin.x - detail.origin.x,
            text.origin.y - detail.origin.y,
            text.size.width
        ),
        (px(14.), px(7.), px(400. - 28.))
    );
    assert!(text.size.height > px(100.), "the text wraps past the cap");
    say_error(cx, &line, "disk full");
    cx.update(|cx| line.update(cx, |line, cx| line.update(cx, |m, now| m.dismiss(now))));
    frame(cx, handle);
    let short = VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("fb1-detail")
        .expect("the second error's detail");
    assert_eq!(
        short.size.height,
        px(7. + 16. + 7.),
        "one line of detail between the row padding"
    );
    cx.update(|cx| line.update(cx, |line, cx| line.update(cx, |m, now| m.dismiss(now))));
    frame(cx, handle);
    assert!(
        VisualTestContext::from_window(handle.into(), cx)
            .debug_bounds("fb1-detail")
            .is_none(),
        "no error, no detail"
    );
}

#[path = "detail_typography_tests.rs"]
pub(super) mod typography;
