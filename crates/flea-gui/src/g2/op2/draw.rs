//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::cards::*;
use super::draw_step::{DISABLED_OPACITY, WASH_PRESS};
use super::flash::SEL_FORMAT_LABEL;
use crate::g2::fb1::caption::Tracked;
use crate::g2::op3::shell::{card_heading, glyph};
use crate::list::{Palette, Typeface};
use gpui::{
    canvas, div, prelude::*, px, relative, AnyElement, App, Bounds, Div, FontWeight, Hsla,
    MouseButton, Pixels, Window,
};
use std::{cell::Cell, rc::Rc};

/// Where a secret field's eye painted, for the pointer and archiveCardsState's eye centre.
pub type EyeProbe = Rc<Cell<Option<Bounds<Pixels>>>>;

/// A click on an enabled button reports its index, as Enter on it would press it.
pub type ButtonPress = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// The open card's eye probes, handed to its secret fields in body order.
pub(super) struct Eyes<'a> {
    probes: &'a [EyeProbe],
    at: usize,
}

impl Eyes<'_> {
    fn next(&mut self) -> Option<(usize, EyeProbe)> {
        let at = self.at;
        self.at += 1;
        self.probes.get(at).map(|probe| (at, probe.clone()))
    }
}

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

/// SettingsSegment.qml:68: the chosen chip's text in the foreground, every other one in Theme.color.muted.
pub(super) fn chip_ink(t: &Tokens, on: bool) -> Hsla {
    if on {
        t.fg
    } else {
        t.muted
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
        self.text(CAPTION)
            .line_height(self.px(LABEL_LINE))
            .mb(self.px(LABEL_GAP))
            .text_color(self.t.muted)
            .child(Tracked::caption(
                text,
                self.language,
                self.typeface.font.clone(),
                self.px(CAPTION),
                self.px(LABEL_LINE),
                self.t.muted,
            ))
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
                    .text_color(chip_ink(&t, on))
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

    pub(super) fn field(&self, field: &Field) -> Div {
        self.field_probed(field, None)
    }

    /// The eye glyph, recording its painted bounds into the probe when one is given.
    fn eye(&self, probe: Option<(usize, EyeProbe)>) -> Div {
        let eye = div().flex_none().relative().child(glyph(
            Glyph::Eye,
            super::cards::EYE * self.s,
            self.t.muted_frame,
        ));
        match probe {
            Some((at, probe)) => eye.debug_selector(move || format!("op2-eye-{at}")).child(
                canvas(
                    |_, _, _| {},
                    move |painted, _, _, _| probe.set(Some(painted)),
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            ),
            None => eye,
        }
    }

    fn field_probed(&self, field: &Field, probe: Option<(usize, EyeProbe)>) -> Div {
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
            .border_color(if field.error {
                t.error
            } else if field.focused {
                t.accent_frame
            } else {
                t.muted_frame
            })
            .rounded(self.px(self.r))
            .text_color(t.fg)
            .whitespace_nowrap();
        if let Some(input) = &field.input {
            line = line.child(crate::g2::op1::field::mount(
                input.clone(),
                crate::g2::op1::field::slot(&field.value, &field.placeholder, t.muted),
                self.typeface,
            ));
            if field.eye {
                line = line.child(self.eye(probe));
            }
            return match &field.label {
                Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
                None => line,
            };
        }
        let drawn = field.drawn();
        // The text takes the free width, so the eye sits at the right edge whatever the field holds.
        let mut text = div()
            .flex()
            .flex_1()
            .items_center()
            .min_w_0()
            .overflow_hidden();
        if drawn.is_empty() {
            if field.focused {
                text = text.child(caret());
            }
            if !field.placeholder.is_empty() {
                text = text.child(
                    div()
                        .ml(px(1.))
                        .text_color(t.muted)
                        .child(field.placeholder.clone()),
                );
            }
        } else {
            text = text.child(drawn);
            if field.focused {
                text = text.child(caret());
            }
        }
        line = line.child(crate::locale_geometry::unmirrored(text));
        if field.eye {
            line = line.child(self.eye(probe));
        }
        match &field.label {
            Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
            None => line,
        }
    }

    fn checkline(&self, check: &CheckLine) -> Div {
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
            .flex()
            .items_center()
            .gap(self.px(ROW_GAP))
            .child(box_)
            .child(
                self.text(BOARD_BODY)
                    .text_color(t.fg)
                    .child(check.label.clone()),
            )
    }

    pub(super) fn block(&self, block: &Block, eyes: &mut Eyes) -> AnyElement {
        match block {
            Block::Chips(chips) => self.chips(chips).into_any_element(),
            Block::Field(field) if field.secret && field.eye => {
                self.field_probed(field, eyes.next()).into_any_element()
            }
            Block::Field(field) => self.field(field).into_any_element(),
            Block::CheckLine(check) => self.checkline(check).into_any_element(),
            Block::Note { text, ink } => {
                if text.is_empty() {
                    div().into_any_element()
                } else {
                    self.text(CAPTION)
                        .text_color(self.t.ink(*ink))
                        .child(text.clone())
                        .into_any_element()
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
    (board * body / BOARD_BODY)
        .ceil()
        .min((room - 2. * GAP).floor())
        .max(0.)
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
        self.present_probed(card, palette, typeface, room, radius, &[], None)
    }
}

impl StandIn {
    /// The card with each secret field's eye bounds recorded into its probe, in body order, and live buttons when pressable.
    #[allow(clippy::too_many_arguments)]
    pub fn present_probed(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        room: f32,
        radius: f32,
        probes: &[EyeProbe],
        press: Option<ButtonPress>,
    ) -> AnyElement {
        let s = f32::from(typeface.body) / BOARD_BODY;
        let draw = Draw {
            language: &card.language,
            t: Tokens::of(palette),
            s,
            typeface,
            r: radius,
        };
        let t = draw.t;
        let head = card_heading(
            &card.title,
            &card.esc,
            draw.text(TITLE)
                .font_weight(FontWeight::BOLD)
                .text_color(t.fg),
            draw.text(CAPTION)
                .font_weight(FontWeight::NORMAL)
                .text_color(t.muted),
            draw.px(ROW_GAP),
        );
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
            // A press on the card is the card's; only a press on the scrim around it answers as Cancel.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(head)
            .child(draw.hairline());
        frame = draw.body(card, frame, &mut Eyes { probes, at: 0 });
        let mut buttons = div()
            .mt(draw.px(ABOVE_BUTTONS))
            .flex()
            .justify_end()
            .items_center()
            .gap(draw.px(ROW_GAP));
        for (at, button) in card.buttons.iter().enumerate() {
            let press = press.clone().filter(|_| button.enabled);
            buttons = buttons.child(
                draw.button(button, at)
                    .id(("op2-button", at))
                    .when_some(press, |drawn, press| {
                        drawn.on_click(move |_, window, cx| press(&at, window, cx))
                    }),
            );
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
