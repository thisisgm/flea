//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::cards::*;
use super::draw_step::{DISABLED_OPACITY, WASH_PRESS};
use crate::g2::open_with_properties::shell::{card_heading, glyph};
use crate::g2::status_feedback::caption::Tracked;
use crate::list::{Palette, Typeface};
use gpui::{
    canvas, div, prelude::*, px, relative, AnyElement, App, Bounds, Div, FontWeight, MouseButton,
    Pixels, Window,
};
use std::{cell::Cell, rc::Rc};

/// Where a secret field's eye painted, for the pointer and archiveCardsState's eye centre.
pub type EyeProbe = Rc<Cell<Option<Bounds<Pixels>>>>;

/// A click on an enabled control reports what it landed on, as that control's key would answer.
pub type CardPress = Rc<dyn Fn(&Hit, &mut Window, &mut App)>;

/// The open card's eye probes, handed to its secret fields in body order, and the next field's place.
pub(super) struct Eyes<'a> {
    probes: &'a [EyeProbe],
    at: usize,
    field: usize,
}

impl Eyes<'_> {
    fn next(&mut self) -> Option<(usize, EyeProbe)> {
        let at = self.at;
        self.at += 1;
        self.probes.get(at).map(|probe| (at, probe.clone()))
    }

    fn next_field(&mut self) -> usize {
        self.field += 1;
        self.field - 1
    }
}

/// Draws a card centred with no scrim, until op1's shell replaces it.
pub struct StandIn;

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

    /// The eye glyph, recording its painted bounds into the probe when one is given.
    fn eye(&self, probe: Option<(usize, EyeProbe)>) -> Div {
        // DialogField's eye takes its own press, so holding it never moves the caret or the focus.
        let eye = div()
            .flex_none()
            .relative()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(glyph(
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

    /// The field at this place among the card's fields; a press on it while unfocused takes the keys, as Tab to it does.
    fn field(&self, field: &Field, at: usize, probe: Option<(usize, EyeProbe)>) -> Div {
        let t = self.t;
        let caret = || div().w(px(1.)).h(self.px(CARET)).flex_none().bg(t.fg);
        let mut line = div()
            .debug_selector(move || format!("op2-field-{at}"))
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
            line = line.child(crate::g2::file_ops::field::mount(
                input.clone(),
                crate::g2::file_ops::field::slot(&field.value, &field.placeholder, t.muted),
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
        if let Some(press) = self.press.clone().filter(|_| !field.focused) {
            line = line.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                press(&Hit::Field(at), window, cx)
            });
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

    pub(super) fn block(&self, block: &Block, eyes: &mut Eyes) -> AnyElement {
        match block {
            Block::Chips(chips) => self.chips(chips).into_any_element(),
            Block::Field(field) => {
                let at = eyes.next_field();
                let probe = (field.secret && field.eye).then(|| eyes.next()).flatten();
                self.field(field, at, probe).into_any_element()
            }
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
        press: Option<CardPress>,
    ) -> AnyElement {
        let s = f32::from(typeface.body) / BOARD_BODY;
        let draw = Draw {
            language: &card.language,
            t: Tokens::of(palette),
            s,
            typeface,
            r: radius,
            press: press.clone(),
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
        frame = draw.body(
            card,
            frame,
            &mut Eyes {
                probes,
                at: 0,
                field: 0,
            },
        );
        let mut buttons = div()
            .mt(draw.px(ABOVE_BUTTONS))
            .flex()
            .justify_end()
            .items_center()
            .gap(draw.px(ROW_GAP));
        for (at, button) in card.buttons.iter().enumerate() {
            let press = press.clone().filter(|_| button.enabled);
            buttons = buttons.child(draw.button(button, at).id(("op2-button", at)).when_some(
                press,
                |drawn, press| {
                    drawn.on_click(move |_, window, cx| press(&Hit::Button(at), window, cx))
                },
            ));
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
