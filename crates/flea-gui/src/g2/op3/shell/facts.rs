//! The Properties card's facts: one row per fact, the names in one label column, a long value wrapping in its own.
use super::*;
use flea::ui_logic_pl4::tags;
use gpui::{div, prelude::*, px, Div};

pub(super) fn is_card(card: &Card) -> bool {
    card.body
        .iter()
        .any(|block| matches!(block, Block::Facts(_)))
}

impl Draw<'_> {
    pub(super) fn facts(&self, facts: &[Fact]) -> Div {
        let t = self.t;
        let line = |size: f32| {
            self.text(size)
                .min_h(self.px(FACT_LINE))
                .line_height(self.px(FACT_LINE))
                .flex()
                .items_center()
        };
        let names: Vec<&String> = facts
            .iter()
            .map(|fact| match fact {
                Fact::Text { name, .. } | Fact::Tags { name, .. } => name,
            })
            .collect();
        let mut rows = div().flex().flex_col().w_full();
        for (at, fact) in facts.iter().enumerate() {
            let value = match fact {
                // A long value, a path, wraps inside the value column on the fact's own 24 px lines, never past the card.
                Fact::Text { value, tail, .. } if tail.is_empty() => self
                    .text(BOARD_BODY)
                    .whitespace_normal()
                    .min_h(self.px(FACT_LINE))
                    .line_height(self.px(FACT_LINE))
                    .text_color(t.fg)
                    .child(value.clone()),
                Fact::Text { value, tail, .. } => line(BOARD_BODY)
                    .text_color(t.fg)
                    .child(value.clone())
                    .child(div().text_color(t.muted).child(tail.clone())),
                Fact::Tags { tags: tagged, .. } => {
                    let mut value = line(BOARD_BODY).text_color(t.fg);
                    for (at, tag) in tagged.iter().enumerate() {
                        // A known tag leads with its solid 14 px square in the theme's surface ink; another name is plain text.
                        let gap = if at + 1 < tagged.len() { TAG_GAP } else { 0. };
                        if let Some(ink) = t.tag_ink(tag) {
                            value = value.child(
                                div()
                                    .debug_selector(move || format!("op3-tag-square-{at}"))
                                    .size(self.px(tags::SWATCH_SIZE))
                                    .flex_none()
                                    .mr(self.px(TAG_LEAD))
                                    .bg(ink),
                            );
                        }
                        value = value.child(div().mr(self.px(gap)).child(tag.clone()));
                    }
                    value
                }
            };
            // Every name sits in each label cell at no height, so the column takes the widest name on every row.
            let mut label = div().flex().flex_col().flex_none();
            for (other, name) in names.iter().enumerate() {
                label = label.child(if other == at {
                    line(CAPTION).text_color(t.muted).child((*name).clone())
                } else {
                    line(CAPTION)
                        .h(px(0.))
                        .min_h(px(0.))
                        .overflow_hidden()
                        .text_color(gpui::transparent_black())
                        .child((*name).clone())
                });
            }
            rows = rows.child(
                div()
                    .flex()
                    .items_start()
                    .gap(self.px(FACT_GAP))
                    .child(label)
                    .child(
                        div()
                            .debug_selector(move || format!("op3-fact-value-{at}"))
                            .flex_1()
                            .min_w_0()
                            .child(value.debug_selector(move || format!("op3-fact-{at}"))),
                    ),
            );
        }
        rows
    }

    pub(super) fn card_body(&self, card: &Card, frame: Stateful<Div>) -> Stateful<Div> {
        if is_card(card) {
            frame.child(
                div()
                    .id("op3-facts-body")
                    .debug_selector(|| "op3-facts-body".into())
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(self.px(GAP))
                    .children(
                        card.body
                            .iter()
                            .map(|block| div().flex_none().child(self.block(block))),
                    ),
            )
        } else {
            card.body
                .iter()
                .fold(frame, |frame, block| frame.child(self.block(block)))
        }
    }
}
