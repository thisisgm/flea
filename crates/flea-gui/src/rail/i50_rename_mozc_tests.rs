//! Mozc conversion in the mounted rename field: underline and end caret, no wash, candidates under the caret.
use super::{close, draw, input, ready, TestAppContext, VIEWS};
use crate::path::field::marks::marks;
use gpui::{px, EntityInputHandler, PlatformInputHandler};

// fcitx5 sends Mozc's converted segment as the preedit cursor range (PreeditCursorPositionAtBeginning=False).
const CONVERSION: [(&str, std::ops::Range<usize>); 3] =
    [("め", 1..1), ("めも", 2..2), ("メモ", 0..2)];

#[gpui::test]
fn i50_mozc_rename_conversion_caret_and_candidates(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, _lines) = ready(cx, view, side);
        let frame = shell.read_with(cx, |s, _| {
            s.rename.edit.as_ref().unwrap().line.frame.unwrap()
        });
        let opened = cx.update(|w, _| marks(w, frame));
        assert_eq!(
            (opened.washes.len(), opened.carets.len()),
            (1, 0),
            "{view:?}/{side}: the opened stem is a real selection: {opened:?}"
        );
        for (text, cursor) in CONVERSION {
            input(cx, &shell, |s, w, cx| {
                s.replace_and_mark_text_in_range(None, text, Some(cursor), w, cx)
            });
            draw(cx);
        }
        let painted = cx.update(|w, _| marks(w, frame));
        let (&[], &[caret], &[underline]) = (
            &painted.washes[..],
            &painted.carets[..],
            &painted.underlines[..],
        ) else {
            panic!("{view:?}/{side}: an underlined preedit, one caret and no wash: {painted:?}");
        };
        assert_eq!(
            caret.left(),
            underline.right(),
            "{view:?}/{side}: the caret ends the composition"
        );
        let anchor = input(cx, &shell, |s, w, cx| {
            let marked = s.marked_text_range(w, cx);
            let selection = s.selected_text_range(true, w, cx).unwrap();
            PlatformInputHandler::compute_ime_candidate_bounds(marked, &selection, |range| {
                s.bounds_for_range(range, frame, w, cx)
            })
            .unwrap()
        });
        assert!(
            painted.lands(caret.left(), anchor.left()),
            "{view:?}/{side}: candidates open under the caret: {anchor:?}, {caret:?}"
        );
        assert_eq!(
            (anchor.top(), anchor.bottom()),
            (frame.top() - px(3.), frame.bottom() + px(3.)),
            "{view:?}/{side}: candidates open 3 px clear of the field"
        );
        close(&shell, cx);
    }
}
