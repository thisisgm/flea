//! The card rhythm at the live Theme.font stop; hairlines keep their physical width.
use crate::list::Typeface;
use flea::ui_logic_op1::card::{self, Kind};

pub struct Metrics {
    pub body: f32,
    pub caption: f32,
    pub title: f32,
    pub padding: f32,
    pub gap: f32,
    pub buttons_margin: f32,
    pub button: f32,
    pub button_pad: f32,
    pub button_gap: f32,
    pub label_line: f32,
    pub label_gap: f32,
    pub field_gap: f32,
    pub selection: f32,
    pub caret: f32,
}

impl Metrics {
    pub fn new(face: &Typeface) -> Self {
        let body = f32::from(face.body);
        let small = f32::from(face.body_small);
        let caption = f32::from(face.caption);
        let scale = |value: f32| (value * body / card::BODY_SIZE).round();
        let gap = (8. * body / 12.).round();
        Self {
            body,
            caption,
            title: scale(card::TITLE_SIZE),
            padding: scale(card::PADDING),
            gap: scale(card::GAP),
            buttons_margin: scale(card::BUTTONS_MARGIN),
            button: (small * 1.8).round() + (6. * body / 12.).round(),
            button_pad: gap,
            button_gap: gap,
            label_line: scale(card::FIELD_LABEL_LINE),
            label_gap: scale(card::FIELD_LABEL_GAP),
            field_gap: scale(card::FIELD_GAP),
            selection: scale(card::FIELD_SELECTION),
            caret: scale(card::CARET_HEIGHT),
        }
    }

    pub fn width(&self, kind: Kind, window: f32) -> f32 {
        let width = if matches!(kind, Kind::CopyTo | Kind::NewFile) {
            // MenuActionDialog: Theme.space(420), the 9/8 dialog ratio, then cardSpan's ceil.
            ((420. * self.body / 12.).round() * 9. / 8.).ceil()
        } else {
            (card::width(kind) * self.body / card::BODY_SIZE).round()
        };
        width.min((window - 2. * self.padding).max(0.))
    }

    pub fn body_room(&self, window: f32) -> f32 {
        let chrome = 2. * card::BORDER
            + 2. * self.padding
            + (self.title * card::LINE).ceil()
            + 3. * self.gap
            + card::RULE
            + self.buttons_margin
            + self.button;
        (window - 2. * self.padding - chrome).max(0.)
    }
}
