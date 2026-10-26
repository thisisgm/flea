// Quick Look's card rect: tall kinds take the window height less the edge, the rest keep 82% by 82%.
use super::kinds::State;
use super::swap::js_round;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub kind: State,
    // False while the one fact (a PDF's page 1 size, an image's pixels) is unasked or unanswered.
    pub known: bool,
    pub w: f64,
    pub h: f64,
    pub orient: u32,
}

impl Shape {
    pub fn unknown(kind: State) -> Self { Self { kind, known: false, w: 0.0, h: 0.0, orient: 1 } }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub fraction: f64,
    pub edge: f64,
    pub bar: f64,
}

// ui/Theme.qml's Quick Look tokens at scale 1: 82% of the window, a 24 px edge and the 27 px bar.
pub const TOKENS: Tokens = Tokens { fraction: 0.82, edge: 24.0, bar: 27.0 };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

// The want before card_span rounds it; surface(A4 PDF, 1200, 760, false, TOKENS) is 484.06 by 712.
pub fn surface(shape: &Shape, win_w: f64, win_h: f64, expanded: bool, tokens: Tokens) -> Size {
    let today = Size { w: win_w * tokens.fraction, h: win_h * tokens.fraction };
    if expanded {
        return Size { w: win_w, h: win_h };
    }
    let tall_h = win_h - 2.0 * tokens.edge;
    let seen = shape.known && shape.w > 0.0 && shape.h > 0.0;
    match shape.kind {
        State::Text | State::Code | State::Archive => Size { w: today.w, h: tall_h },
        State::Pdf if seen => Size { w: today.w.min((tall_h - tokens.bar) * shape.w / shape.h), h: tall_h },
        State::Image if seen => {
            let (ow, oh) = if shape.orient >= 5 { (shape.h, shape.w) } else { (shape.w, shape.h) };
            // A portrait shorter than the tall rect keeps today's frame rather than being stretched to it.
            if oh > ow && oh >= tall_h {
                Size { w: today.w.min(tall_h * ow / oh), h: tall_h }
            } else {
                today
            }
        }
        _ => today,
    }
}

// ui/Theme.qml cardSpan: the want rounded up, never past the room.
pub fn card_span(want: f64, room: f64) -> f64 { want.ceil().min(room.floor()).max(0.0) }

// ui/Theme.qml cardOrigin: the card centred in its room.
pub fn card_origin(room: f64, span: f64) -> f64 { js_round((room - span) / 2.0) }

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(kind: State, w: f64, h: f64, orient: u32) -> Shape { Shape { kind, known: true, w, h, orient } }

    fn rect(s: Shape, expanded: bool) -> String {
        let r = surface(&s, 1200.0, 760.0, expanded, TOKENS);
        format!("{}x{}", card_span(r.w, 1200.0), card_span(r.h, 760.0))
    }

    // tests/js/quicklookrect.js, case for case.
    #[test]
    fn quicklookrect_js() {
        assert_eq!(rect(Shape::unknown(State::Text), false), "984x712");
        assert_eq!(rect(Shape::unknown(State::Archive), false), "984x712");
        assert_eq!(rect(shape(State::Pdf, 595.0, 842.0, 1), false), "485x712");
        assert_eq!(rect(Shape::unknown(State::Pdf), false), "984x624");
        assert_eq!(rect(shape(State::Pdf, 1000.0, 500.0, 1), false), "984x712");
        assert_eq!(rect(shape(State::Image, 1170.0, 2532.0, 1), false), "330x712");
        assert_eq!(rect(shape(State::Image, 4000.0, 3000.0, 1), false), "984x624");
        assert_eq!(rect(shape(State::Image, 4000.0, 3000.0, 6), false), "534x712");
        assert_eq!(rect(shape(State::Image, 3000.0, 4000.0, 8), false), "984x624");
        assert_eq!(rect(shape(State::Image, 1000.0, 1000.0, 1), false), "984x624");
        assert_eq!(rect(shape(State::Image, 200.0, 400.0, 1), false), "984x624");
        assert_eq!(rect(shape(State::Image, 356.0, 712.0, 1), false), "356x712");
        assert_eq!(rect(Shape::unknown(State::Image), false), "984x624");
        assert_eq!(rect(shape(State::Image, 0.0, 0.0, 1), false), "984x624");
        assert_eq!(rect(Shape::unknown(State::Video), false), "984x624");
        assert_eq!(rect(Shape::unknown(State::Audio), false), "984x624");
        assert_eq!(rect(Shape::unknown(State::Unsupported), false), "984x624");
        assert_eq!(rect(shape(State::Pdf, 595.0, 842.0, 1), true), "1200x760");
    }

    #[test]
    fn the_unsupported_board_card_sits_at_99_63() {
        let r = surface(&Shape::unknown(State::Unsupported), 1100.0, 700.0, false, TOKENS);
        let (w, h) = (card_span(r.w, 1100.0), card_span(r.h, 700.0));
        assert_eq!((card_origin(1100.0, w), card_origin(700.0, h), w, h), (99.0, 63.0, 902.0, 574.0));
        let t = surface(&Shape::unknown(State::Text), 1100.0, 700.0, false, TOKENS);
        assert_eq!((card_origin(700.0, card_span(t.h, 700.0)), card_span(t.h, 700.0)), (24.0, 652.0));
    }

    // EY-43: the GPUI card mounts code and HTML as State::Code, which ql-code.png draws as tall as text.
    #[test]
    fn a_code_card_is_as_tall_as_the_text_card() {
        let c = surface(&Shape::unknown(State::Code), 1100.0, 700.0, false, TOKENS);
        let (w, h) = (card_span(c.w, 1100.0), card_span(c.h, 700.0));
        assert_eq!(
            (card_origin(1100.0, w), card_origin(700.0, h), w, h),
            (99.0, 24.0, 902.0, 652.0)
        );
    }
}
