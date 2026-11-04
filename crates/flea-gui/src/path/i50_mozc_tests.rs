//! Mozc conversion in the mounted path field: underline and end caret, no wash, candidates under the caret.
use super::*;
use crate::path::{field::marks::marks, suggestions::FRAME};
use gpui::PlatformInputHandler;

#[gpui::test]
fn i50_mozc_path_conversion_caret_and_candidates(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked));
    let frame = bar.read(|view| view.paint.borrow().frame.unwrap());
    let opened = bar.visual.update(|window, _| marks(window, frame));
    assert_eq!(
        (opened.washes.len(), opened.carets.len()),
        (1, 0),
        "the opened path is a real selection: {opened:?}"
    );
    // fcitx5 sends Mozc's converted segment as the preedit cursor range (PreeditCursorPositionAtBeginning=False).
    bar.update(|view, window, cx| {
        view.replace_text_in_range(None, "/tmp/", window, cx);
        for (text, cursor) in [("め", 1..1), ("めも", 2..2), ("メモ", 0..2)] {
            view.replace_and_mark_text_in_range(None, text, Some(cursor), window, cx);
        }
    });
    bar.visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let painted = bar.visual.update(|window, _| marks(window, frame));
    let (&[], &[caret], &[underline]) = (
        &painted.washes[..],
        &painted.carets[..],
        &painted.underlines[..],
    ) else {
        panic!("an underlined preedit, one caret and no wash: {painted:?}");
    };
    assert_eq!(
        caret.left(),
        underline.right() + px(FRAME),
        "the caret ends the composition"
    );
    let anchor = bar.update(|view, window, cx| {
        assert_eq!(view.entry.text, "/tmp/メモ");
        let marked = view.marked_text_range(window, cx);
        let selection = view.selected_text_range(true, window, cx).unwrap();
        PlatformInputHandler::compute_ime_candidate_bounds(marked, &selection, |range| {
            view.bounds_for_range(range, Default::default(), window, cx)
        })
        .unwrap()
    });
    assert!(
        painted.lands(underline.right(), anchor.left()),
        "candidates open at the caret, not at the composition's start: {anchor:?}, {underline:?}"
    );
    assert_eq!(
        (anchor.top(), anchor.bottom()),
        (frame.top() - px(3.), frame.bottom() + px(3.)),
        "candidates open 3 px clear of the field"
    );
}
