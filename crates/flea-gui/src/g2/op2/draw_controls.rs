//! The card's Format chips and checkbox, each answering a click as its key does.
use super::cards::*;
use super::flash::SEL_FORMAT_LABEL;
use crate::g2::op3::shell::glyph;
use gpui::{div, prelude::*, Div, Hsla, Stateful};

/// The chosen chip draws a foreground frame, an accent one while the chips hold the keyboard; the rest stay muted.
pub(super) fn chip_frame(t: &Tokens, on: bool, focused: bool) -> Hsla {
    match (on, focused) {
        (true, true) => t.accent_frame,
        (true, false) => t.fg,
        (false, _) => t.muted_frame,
    }
}

/// SettingsSegment.qml:68: the chosen chip's text in the foreground, every other one in Theme.color.muted.
pub(super) fn chip_ink(t: &Tokens, on: bool) -> Hsla {
    if on {
        t.fg
    } else {
        t.muted
    }
}

impl Draw<'_> {
    pub(super) fn chips(&self, chips: &Chips) -> Div {
        let t = self.t;
        let mut row = div().flex().gap(self.px(CHIP_GAP));
        for (at, option) in chips.options.iter().enumerate() {
            let on = at == chips.current;
            row = row.child(
                self.text(BOARD_BODY)
                    .debug_selector(move || format!("op2-chip-{at}"))
                    .id(("op2-chip", at))
                    .h(self.px(CHIP_HEIGHT))
                    .px(self.px(BUTTON_PAD))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(chip_frame(&t, on, chips.focused))
                    .rounded(self.px(self.r))
                    .text_color(chip_ink(&t, on))
                    .when_some(self.press.clone(), |chip, press| {
                        chip.on_click(move |_, window, cx| press(&Hit::Chip(at), window, cx))
                    })
                    .child(option.clone()),
            );
        }
        if chips.label.is_empty() {
            return row;
        }
        div()
            .flex()
            .flex_col()
            .child(
                self.label(&chips.label)
                    .debug_selector(|| SEL_FORMAT_LABEL.into()),
            )
            .child(row)
    }

    pub(super) fn checkline(&self, check: &CheckLine) -> Stateful<Div> {
        let t = self.t;
        let frame = if check.checked || check.focused {
            t.fg
        } else {
            t.muted_frame
        };
        let box_ = div()
            .size(self.px(CHECK))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .border_2()
            .border_color(frame)
            .rounded(self.px(self.r / 2.))
            .when(check.checked, |d| d.bg(t.fg))
            .when(check.checked, |d| {
                d.child(glyph(Glyph::Check, CHECK_GLYPH * self.s, t.surface))
            });
        div()
            .debug_selector(|| "op2-encrypt".into())
            .id("op2-encrypt")
            .flex()
            .items_center()
            .gap(self.px(ROW_GAP))
            .when_some(self.press.clone(), |row, press| {
                row.on_click(move |_, window, cx| press(&Hit::Check, window, cx))
            })
            .child(box_)
            .child(
                self.text(BOARD_BODY)
                    .text_color(t.fg)
                    .child(check.label.clone()),
            )
    }
}
