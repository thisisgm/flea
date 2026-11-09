//! S1A-2: a query wider than the pane elides inside it, in List, Grid and Columns, and a short one is drawn whole.
use super::filter;
use crate::{
    g2::status_feedback::caption::INKED,
    window::{columns_tests::host, startup::View},
};
use gpui::{Bounds, Pixels, VisualTestContext};

/// The headline's painted text and box from one frame, and the box it is centred in.
fn painted(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::state::Shell>,
    slot: &'static str,
) -> (String, Bounds<Pixels>, Bounds<Pixels>) {
    INKED.with_borrow_mut(|inked| *inked = Some(Vec::new()));
    host::frame(cx, window);
    let inked = INKED.with_borrow_mut(|inked| inked.take().unwrap());
    let (text, _, _, bounds) = inked
        .into_iter()
        .find(|(text, ..)| text.starts_with("NOTHING MATCHES"))
        .expect("the headline painted");
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let slot = visual.debug_bounds(slot).expect("the headline's box drew");
    (text.to_string(), bounds, slot)
}

#[test]
fn s1a2_a_long_filter_query_elides_inside_every_view() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 1100., 3);
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, window);
    let query = "q".repeat(300);
    host::change(&mut cx, window, |shell, _, cx| {
        filter(shell, &query, 2, 3, "", cx)
    });
    for (view, slot) in [
        (View::List, "no-match-hero"),
        (View::Grid, "no-match-hero"),
        (View::Columns, "columns-no-match"),
    ] {
        host::change(&mut cx, window, |shell, _, _| shell.view = view);
        let (text, drawn, slot) = painted(&mut cx, window, slot);
        assert!(
            text.starts_with("NOTHING MATCHES q") && text.ends_with("q\u{2026}"),
            "{view:?}: the words stay and the query elides: {text}"
        );
        assert!(text.chars().count() < 300, "{view:?}");
        assert!(
            drawn.origin.x >= slot.origin.x && drawn.right() <= slot.right(),
            "{view:?}: {drawn:?} stays inside {slot:?}"
        );
    }
    host::change(&mut cx, window, |shell, _, cx| {
        filter(shell, "zzz", 2, 3, "", cx)
    });
    host::change(&mut cx, window, |shell, _, _| shell.view = View::List);
    let (text, ..) = painted(&mut cx, window, "no-match-hero");
    assert_eq!(text, "NOTHING MATCHES zzz", "a short query is drawn whole");
}
