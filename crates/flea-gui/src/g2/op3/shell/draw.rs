//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::*;
use gpui::{
    div, prelude::*, px, relative, AnyElement, App, Div, FontWeight, MouseButton, Pixels, Window,
};
use std::rc::Rc;

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

    pub(super) fn check(&self, check: Check) -> AnyElement {
        let t = self.t;
        let on = check.value != Tri::Off;
        let frame = if on || check.focused {
            t.fg
        } else {
            t.muted_frame
        };
        let mut box_ = div()
            .size(self.px(CHECK))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .border_2()
            .border_color(frame)
            .rounded(px(self.corner / 2.))
            .when(on, |d| d.bg(t.fg));
        box_ = match check.value {
            Tri::On => box_.child(glyph(Glyph::Check, 12. * self.s, t.surface)),
            Tri::Mixed => box_.child(div().w(self.px(8.)).h(self.px(2.)).bg(t.surface)),
            Tri::Off => box_,
        };
        // A focused filled box takes a ring 2 outside it, since its own frame is already foreground.
        if check.focused && on {
            let ring = div()
                .absolute()
                .top(px(-2.))
                .left(px(-2.))
                .right(px(-2.))
                .bottom(px(-2.))
                .border_1()
                .border_color(t.fg)
                .rounded(px(self.corner / 2.));
            return div()
                .relative()
                .flex_none()
                .child(box_)
                .child(ring)
                .into_any_element();
        }
        box_.into_any_element()
    }

    pub(super) fn label(&self, text: &str) -> Div {
        self.text(CAPTION)
            .line_height(self.px(14.))
            .mb(self.px(5.))
            .text_color(self.t.muted)
            .child(text.to_uppercase())
    }

    pub(super) fn field(&self, field: &Field) -> Div {
        let t = self.t;
        let caret = || div().w(px(1.)).h(self.px(16.)).flex_none().bg(t.fg);
        let mut line = div()
            .h(self.px(BUTTON_HEIGHT))
            .flex()
            .items_center()
            .gap(self.px(8.))
            .px(self.px(BUTTON_PAD))
            .bg(t.ground)
            .border_1()
            .border_color(if field.focused {
                t.accent_frame
            } else {
                t.muted_frame
            })
            .rounded(px(self.corner))
            .text_color(t.fg)
            .whitespace_nowrap();
        if let Some(mark) = field.glyph {
            line = line.child(glyph(mark, 14. * self.s, t.muted));
        }
        let mut text = div().flex().items_center().min_w_0().overflow_hidden();
        if field.value.is_empty() {
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
            text = text.child(field.value.clone());
            if field.focused {
                text = text.child(caret());
            }
        }
        let line = line.child(text);
        match &field.label {
            Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
            None => line,
        }
    }

    pub(super) fn button(&self, button: &Button) -> Div {
        let t = self.t;
        let primary = button.primary && button.enabled;
        let frame = match (button.focused, primary) {
            (true, _) => t.fg,
            (false, true) => t.accent_frame,
            (false, false) => t.muted_frame,
        };
        self.text(BOARD_BODY)
            .h(self.px(BUTTON_HEIGHT))
            .px(self.px(BUTTON_PAD))
            .flex()
            .items_center()
            .border_1()
            .border_color(frame)
            .rounded(px(self.corner))
            .text_color(t.fg)
            .when(primary, |d| d.bg(t.accent.opacity(WASH_DEFAULT)))
            .when(!button.enabled, |d| d.opacity(DISABLED))
            .child(button.label.clone())
    }
}

/// A click on the button at an index, as the card listed them.
type Press = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

impl DialogShell for StandIn {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement {
        self.frame(card, palette, typeface, corner, room, None)
    }
}

impl StandIn {
    /// The same card with live buttons: a click on one reports its index, as Enter on it would press it.
    pub fn present_pressable(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        press: impl Fn(&usize, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.frame(card, palette, typeface, corner, room, Some(Rc::new(press)))
    }

    fn frame(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        press: Option<Press>,
    ) -> AnyElement {
        let s = f32::from(typeface.body) / BOARD_BODY;
        let draw = Draw {
            corner: crate::compositor::valid_corner(f64::from(corner)),
            t: Tokens::of(palette),
            s,
            typeface,
        };
        let t = draw.t;
        let head = draw
            .text(TITLE)
            .flex()
            .items_center()
            .gap(draw.px(ROW_GAP))
            .font_weight(FontWeight::BOLD)
            .text_color(t.fg)
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(card.title.clone()),
            )
            .child(
                draw.text(CAPTION)
                    .ml_auto()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(t.muted)
                    .child(card.esc.clone()),
            );
        let mut frame = div()
            .debug_selector(|| "op3-card".into())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .w(px(scaled(card.width, f32::from(typeface.body), room)))
            .flex()
            .flex_col()
            .gap(draw.px(GAP))
            .p(draw.px(PAD))
            .bg(t.surface)
            .border_1()
            .border_color(t.muted_frame)
            .rounded(px(draw.corner))
            .font_family(draw.typeface.font.family.clone())
            .text_size(draw.px(BOARD_BODY))
            .line_height(relative(1.2))
            .child(head)
            .child(draw.hairline().when(card.bleed, |d| d.mx(draw.px(-PAD))));
        for block in &card.body {
            frame = frame.child(draw.block(block));
        }
        let mut buttons = div()
            .mt(draw.px(ABOVE_BUTTONS))
            .flex()
            .justify_end()
            .items_center()
            .gap(draw.px(ROW_GAP));
        for (at, button) in card.buttons.iter().enumerate() {
            let drawn = draw
                .button(button)
                .debug_selector(|| format!("op3-button-{at}"));
            // A disabled button draws as before but takes no click.
            buttons = match &press {
                Some(press) if button.enabled => {
                    let press = press.clone();
                    buttons.child(
                        drawn
                            .id(("op3-button", at))
                            .on_click(move |_, window, cx| press(&at, window, cx)),
                    )
                }
                _ => buttons.child(drawn),
            };
        }
        // A pressable card keeps a press on itself from the scrim behind it; a press beside it reaches the scrim.
        let frame = frame.when(press.is_some(), |frame| {
            frame.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        });
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(frame.child(buttons))
            .into_any_element()
    }
}
