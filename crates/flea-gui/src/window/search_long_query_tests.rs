//! S1A-2: a search query wider than the pane elides inside it, and the words around it stay whole.
use super::{
    super::tests::{edit, frame, key, open},
    answer_at,
};
use crate::g2::status_feedback::caption::INKED;
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn s1a2_a_long_search_query_elides_inside_the_pane(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    for _ in 0..300 {
        key(cx, handle, "q", Some("q"));
    }
    let generation = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().search.owner.session.gen as usize
    });
    answer_at(cx, handle, generation, "/files", &[]);
    INKED.with_borrow_mut(|inked| *inked = Some(Vec::new()));
    frame(cx, handle);
    let inked = INKED.with_borrow_mut(|inked| inked.take().unwrap());
    let (text, _, _, drawn) = inked
        .into_iter()
        .find(|(text, ..)| text.starts_with("NOTHING MATCHES"))
        .expect("the search headline painted");
    assert!(
        text.starts_with("NOTHING MATCHES q") && text.ends_with("q\u{2026}"),
        "the words stay and the query elides: {text}"
    );
    assert!(text.chars().count() < 300, "{text}");
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let slot = visual
        .debug_bounds("no-match-hero")
        .expect("the headline's box drew");
    assert!(
        drawn.origin.x >= slot.origin.x && drawn.right() <= slot.right(),
        "{drawn:?} stays inside {slot:?}"
    );
}
