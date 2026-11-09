//! Compress's fields keep their released pitch inside the shared card parts.
use super::cards::{Block, BodyLayout, Card, Draw};
use super::draw::Eyes;
use gpui::{div, prelude::*, Div};

// HIG boards_cards.py stack()/b_compress(): a 65px field pitch and 13 more above the checkbox, outside shared GAP12.
const FIELD_GROUP_GAP: f32 = 16.;
const CHECK_TOP: f32 = 13.;

impl Draw<'_> {
    pub(super) fn body(&self, card: &Card, mut frame: Div, eyes: &mut Eyes) -> Div {
        let mut parts = card.body.iter();
        if card.body_layout == BodyLayout::Compress {
            let mut fields = div().flex().flex_col().gap(self.px(FIELD_GROUP_GAP));
            // Format, Name, Password and Confirm are one part; notes and the optional checkbox remain outside it.
            for block in parts.by_ref().take(4) {
                fields = fields.child(self.block(block, eyes));
            }
            frame = frame.child(fields);
        }
        for block in parts {
            let drawn = self.block(block, eyes);
            frame = if card.body_layout == BodyLayout::Compress
                && matches!(block, Block::CheckLine(_))
            {
                frame.child(div().mt(self.px(CHECK_TOP)).child(drawn))
            } else {
                frame.child(drawn)
            };
        }
        frame
    }
}
