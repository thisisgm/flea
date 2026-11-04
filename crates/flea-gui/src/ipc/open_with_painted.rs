//! Open With's painted parts as openWithState reports them: the rects the card recorded in its last frame and the board tokens at the window's scale, so the design CI asserts the openwith board's layout.
use crate::{
    g2::op1::measured::{rect, Frame, Part},
    g2::op3::shell::{APP_ROW, EYEBROW, FIRST_EYEBROW_AIR, GAP, LATER_EYEBROW_AIR},
    window::open_with::Open,
};
use flea::i18n::I18n;

/// Every part of one numbered kind the frame holds, in order, as a JSON array of rect strings.
fn numbered(frame: &Frame, kind: &str) -> String {
    let parts: Vec<String> = (0..)
        .map_while(|at| frame.part(&format!("{kind}{at}")))
        .map(|part| format!("\"{}\"", rect(Some(&part))))
        .collect();
    format!("[{}]", parts.join(","))
}

/// The fields openWithState adds after its model facts, each rect "x y width height" in logical pixels, empty when that part is not drawn.
pub(super) fn fields(open: &Open, scale: f32, i18n: &I18n) -> String {
    let frame = &open.frame;
    let part = |name: &str| rect(frame.part(name).as_ref());
    let no_match = flea::ui_logic_op3::openwith::no_match(i18n, &open.card.query);
    let note: Option<Part> = frame.part(&format!("note:{no_match}"));
    format!(
        r#""rect":"{}","titleRuleRect":"{}","searchRect":"{}","listRect":"{}","groupRuleRect":"{}","listRuleRect":"{}","rowRect":"{}","eyebrowRects":{},"rowRects":{},"noMatchRect":"{}","rowHeight":{},"eyebrowHeight":{},"gap":{},"searchTrail":{},"eyebrowAir":[{},{}]"#,
        part("card"),
        part("titleRule"),
        part("search"),
        part("list"),
        part("groupRule"),
        part("listRule"),
        part("row0"),
        numbered(frame, "eyebrow"),
        numbered(frame, "row"),
        rect(note.as_ref()),
        APP_ROW * scale,
        EYEBROW * scale,
        GAP * scale,
        GAP * scale,
        FIRST_EYEBROW_AIR * scale,
        LATER_EYEBROW_AIR * scale,
    )
}
