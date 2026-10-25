//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::cards::*;
use super::draw_step::{DISABLED_OPACITY, WASH_PRESS};
use super::flash::SEL_FORMAT_LABEL;
use crate::g2::op3::shell::glyph;
use crate::list::{Palette, Typeface};
use gpui::{div, prelude::*, px, relative, AnyElement, Div, FontWeight, Hsla, Pixels};

/// Draws a card centred with no scrim, until op1's shell replaces it.
pub struct StandIn;

/// The chosen chip draws a foreground frame, an accent one while the chips hold the keyboard; the rest stay muted.
pub(super) fn chip_frame(t: &Tokens, on: bool, focused: bool) -> Hsla {
    match (on, focused) {
        (true, true) => t.accent_frame,
        (true, false) => t.fg,
        (false, _) => t.muted_frame,
    }
}

impl Draw<'_> {
    pub(super) fn px(&self, board: f32) -> Pixels {
        px(board * self.s)
    }

    pub(super) fn text(&self, size: f32) -> Div {
        div().text_size(self.px(size)).whitespace_nowrap()
    }

    pub(super) fn hairline(&self) -> Div {
        div().h(px(1.)).flex_none().bg(self.t.rule)
    }

    pub(super) fn label(&self, text: &str) -> Div {
        self.text(CAPTION).line_height(self.px(LABEL_LINE)).mb(self.px(LABEL_GAP)).text_color(self.t.muted).child(text.to_uppercase())
    }

    fn chips(&self, chips: &Chips) -> Div {
        let t = self.t;
        let mut row = div().flex().gap(self.px(CHIP_GAP));
        for (at, option) in chips.options.iter().enumerate() {
            let on = at == chips.current;
            row = row.child(
                self.text(BOARD_BODY)
                    .h(self.px(CHIP_HEIGHT))
                    .px(self.px(BUTTON_PAD))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(chip_frame(&t, on, chips.focused))
                    .rounded(self.px(self.r))
                    .text_color(if on { t.fg } else { t.muted })
                    .child(option.clone()),
            );
        }
        if chips.label.is_empty() {
            return row;
        }
        div()
            .flex()
            .flex_col()
            .child(self.label(&chips.label).debug_selector(|| SEL_FORMAT_LABEL.into()))
            .child(row)
    }

    pub(super) fn field(&self, field: &Field) -> Div {
        let t = self.t;
        let caret = || div().w(px(1.)).h(self.px(CARET)).flex_none().bg(t.fg);
        let mut line = div()
            .h(self.px(BUTTON_HEIGHT))
            .flex()
            .items_center()
            .gap(self.px(FIELD_GAP))
            .px(self.px(BUTTON_PAD))
            .bg(t.ground)
            .border_1()
            .border_color(if field.error { t.error } else if field.focused { t.accent_frame } else { t.muted_frame })
            .rounded(self.px(self.r))
            .text_color(t.fg)
            .whitespace_nowrap();
        let drawn = field.drawn();
        let mut text = div().flex().items_center().min_w_0().overflow_hidden();
        if drawn.is_empty() {
            if field.focused {
                text = text.child(caret());
            }
            if !field.placeholder.is_empty() {
                text = text.child(div().ml(px(1.)).text_color(t.muted).child(field.placeholder.clone()));
            }
        } else {
            text = text.child(drawn);
            if field.focused {
                text = text.child(caret());
            }
        }
        line = line.child(text);
        if field.eye {
            line = line.child(glyph(Glyph::Eye, super::cards::EYE * self.s, t.muted_frame));
        }
        match &field.label {
            Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
            None => line,
        }
    }

    fn checkline(&self, check: &CheckLine) -> Div {
        let t = self.t;
        let frame = if check.checked || check.focused { t.fg } else { t.muted_frame };
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
            .when(check.checked, |d| d.child(glyph(Glyph::Check, CHECK_GLYPH * self.s, t.surface)));
        div().flex().items_center().gap(self.px(ROW_GAP)).child(box_).child(self.text(BOARD_BODY).text_color(t.fg).child(check.label.clone()))
    }

    fn block(&self, block: &Block) -> AnyElement {
        match block {
            Block::Chips(chips) => self.chips(chips).into_any_element(),
            Block::Field(field) => self.field(field).into_any_element(),
            Block::CheckLine(check) => self.checkline(check).into_any_element(),
            Block::Note { text, ink } => {
                if text.is_empty() {
                    div().into_any_element()
                } else {
                    self.text(CAPTION).text_color(self.t.ink(*ink)).child(text.clone()).into_any_element()
                }
            }
        }
    }

    pub(super) fn button(&self, button: &Button, at: usize) -> Div {
        let t = self.t;
        let primary = button.primary && button.available;
        let frame = match (button.focused, primary) {
            (true, _) => t.fg,
            (false, true) => t.accent_frame,
            (false, false) => t.muted_frame,
        };
        self.text(BOARD_BODY)
            .debug_selector(|| format!("op2-button-{at}"))
            .h(self.px(BUTTON_HEIGHT))
            .px(self.px(BUTTON_PAD))
            .flex()
            .items_center()
            .border_1()
            .border_color(frame)
            .rounded(self.px(self.r))
            .text_color(if button.available { t.fg } else { t.muted })
            .when(primary, |d| d.bg(t.accent.opacity(WASH_PRESS)))
            .when(!button.available, |d| d.opacity(DISABLED_OPACITY))
            .child(button.label.clone())
    }
}

/// The card's width in the window: the board width grown with the body size, held inside the room less the shell's gap.
pub fn scaled(board: f32, body: f32, room: f32) -> f32 {
    (board * body / BOARD_BODY).ceil().min((room - 2. * GAP).floor()).max(0.)
}

impl super::cards::DialogShell for StandIn {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        room: f32,
        radius: f32,
    ) -> AnyElement {
        let s = f32::from(typeface.body) / BOARD_BODY;
        let draw = Draw { t: Tokens::of(palette), s, typeface, r: radius };
        let t = draw.t;
        let head = draw
            .text(TITLE)
            .flex()
            .items_center()
            .gap(draw.px(ROW_GAP))
            .font_weight(FontWeight::BOLD)
            .text_color(t.fg)
            .child(div().min_w_0().overflow_hidden().text_ellipsis().child(card.title.clone()))
            .child(draw.text(CAPTION).ml_auto().font_weight(FontWeight::NORMAL).text_color(t.muted).child(card.esc.clone()));
        let mut frame = div()
            .debug_selector(|| "op2-card".into())
            .w(px(scaled(card.width, f32::from(typeface.body), room)))
            .flex()
            .flex_col()
            .gap(draw.px(GAP))
            .p(draw.px(PAD))
            .bg(t.surface)
            .border_1()
            .border_color(t.muted_frame)
            .rounded(draw.px(draw.r))
            .font_family(draw.typeface.font.family.clone())
            .text_size(draw.px(BOARD_BODY))
            .line_height(relative(1.2))
            .child(head)
            .child(draw.hairline());
        for block in &card.body {
            frame = frame.child(draw.block(block));
        }
        let mut buttons = div().mt(draw.px(ABOVE_BUTTONS)).flex().justify_end().items_center().gap(draw.px(ROW_GAP));
        for (at, button) in card.buttons.iter().enumerate() {
            buttons = buttons.child(draw.button(button, at));
        }
        div()
            .debug_selector(|| format!("op2-radius-{}", draw.r))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(frame.child(buttons))
            .into_any_element()
    }
}

impl StandIn {
    pub(super) fn section(&self, draw: &Draw, text: &str) -> Div {
        draw.text(CAPTION).font_weight(FontWeight::BOLD).text_color(draw.t.muted).child(text.to_uppercase())
    }
}
