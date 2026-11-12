use super::{
    tests::{load, open, token, with},
    Phase,
};
use flea::ui_logic_model_preview::control::Surface;
use gpui::TestAppContext;

#[gpui::test]
fn a_bounded_model_uses_the_existing_large_sentence_and_the_next_preview_loads(
    cx: &mut TestAppContext,
) {
    for surface in [Surface::QuickLook, Surface::Column] {
        let (host, _) = open(cx, surface, true);
        let slot = if surface == Surface::QuickLook {
            "look"
        } else {
            "column"
        };
        for (reason, extra, sentence) in [
            (
                "new diagnostic wording",
                r#", "refusal":"too_large""#,
                "This file is too large to preview.",
            ),
            (
                "flea: XML structure exceeds its bound",
                "",
                "This file cannot be previewed.",
            ),
            (
                "new diagnostic wording",
                "",
                "This file cannot be previewed.",
            ),
            (
                "XML structure exceeds its bound",
                r#", "refusal":"future_code""#,
                "This file cannot be previewed.",
            ),
        ] {
            with(cx, host, |view, cx| {
                view.hide(cx);
                view.show("/p/large.3mf", 0, cx);
            });
            let failed = format!(
                r#"{{"t":"model3d","slot":"{slot}","token":{},"state":"failed","reason":"{reason}"{extra}}}"#,
                token(cx, host)
            );
            assert!(with(cx, host, |view, cx| view.on_line(&failed, cx)));
            assert_eq!(
                with(cx, host, |view, _| view.failed()).as_deref(),
                Some(sentence)
            );
        }
        load(cx, host, "/p/next.stl", "/c/next.3d", "stl");
        assert_eq!(with(cx, host, |view, _| view.phase.clone()), Phase::Ready);
        assert!(with(cx, host, |view, _| view.failed()).is_none());
    }
}
