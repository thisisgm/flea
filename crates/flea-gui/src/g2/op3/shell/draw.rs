//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::*;
use crate::g2::fb1::caption::{tracking_for, Tracked};
use gpui::{div, prelude::*, px, relative, AnyElement, Div, FontWeight, Pixels, Stateful};

/// Draws a card centred with no scrim, until op1's shell replaces it.
pub struct StandIn;

impl Draw<'_> {
    pub(super) fn wired(
        &self,
        control: Control,
        element: gpui::Stateful<Div>,
    ) -> gpui::Stateful<Div> {
        let element = element.when_some(self.probe, |e, p| p.attach(e, format!("{control:?}")));
        match self.wire {
            Some(wire) => wire(control, element),
            None => element,
        }
    }

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

    pub(super) fn caption(&self, text: &str, line: f32, color: Hsla) -> Tracked {
        Tracked {
            text: flea::ui_logic_st1::caps::heading(text, self.language).into(),
            font: self.typeface.font.clone(),
            size: self.px(CAPTION),
            line: self.px(line),
            tracking: self.px(tracking_for(self.language)),
            color,
        }
    }

    pub(super) fn label(&self, text: &str) -> Div {
        self.text(CAPTION)
            .line_height(self.px(14.))
            .mb(self.px(5.))
            .text_color(self.t.muted)
            .child(self.caption(text, 14., self.t.muted))
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

impl DialogShell for StandIn {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement {
        present(card, palette, typeface, corner, room, None)
    }
}

impl DialogShell for OpenWithShell<'_> {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement {
        let live = Live {
            wire: self.wire,
            frame: None,
            surface: Some(self.surface),
            entry: None,
            probe: None,
            measure: Some(self.measure),
        };
        present(card, palette, typeface, corner, room, Some(&live))
    }
}

#[path = "pressable.rs"]
mod pressable;
pub use pressable::Probed;

impl DialogShell for Interactive<'_> {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement {
        let live = Live {
            wire: self.wire,
            frame: Some(self.frame),
            surface: Some(self.surface),
            entry: Some(self.entry),
            probe: None,
            measure: None,
        };
        present(card, palette, typeface, corner, room, Some(&live))
    }
}

/// The one live path every card takes: its wired controls, and the window's surface and text entry where it has them.
struct Live<'a> {
    wire: &'a Wire,
    frame: Option<&'a dyn Fn(Stateful<Div>) -> Stateful<Div>>,
    surface: Option<Hsla>,
    entry: Option<&'a crate::path::entry::Entry>,
    probe: Option<&'a crate::g2::st1::probe::Probe>,
    measure: Option<&'a crate::g2::op1::measured::Frame>,
}

fn present(
    card: &Card,
    palette: &Palette,
    typeface: &Typeface,
    corner: f32,
    room: f32,
    live: Option<&Live<'_>>,
) -> AnyElement {
    let s = f32::from(typeface.body) / BOARD_BODY;
    let mut draw = Draw {
        corner: crate::compositor::valid_corner(f64::from(corner)),
        t: Tokens::of(palette),
        s,
        typeface,
        language: &card.language,
        wire: live.map(|live| live.wire),
        entry: live.and_then(|live| live.entry),
        probe: live.and_then(|live| live.probe),
        measure: live.and_then(|live| live.measure),
    };
    if let Some(surface) = live.and_then(|live| live.surface) {
        draw.t.surface = surface;
    }
    let t = draw.t;
    let facts = super::facts::is_card(card);
    let head = draw
        .text(TITLE)
        .when(facts, |d| d.flex_none())
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
    let rule = draw.hairline().when(card.bleed, |d| d.mx(draw.px(-PAD)));
    let frame = div()
        .id("op3-frame")
        .debug_selector(|| "op3-card".into())
        .when(facts, |d| d.max_h_full())
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
        .occlude()
        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(head)
        .child(draw.track(rule, "titleRule"));
    let frame = draw.card_body(card, frame);
    let mut frame = match draw.probe {
        Some(probe) => probe.attach(frame, "card"),
        None => frame,
    };
    let mut buttons = div()
        .when(facts, |d| d.flex_none())
        .mt(draw.px(ABOVE_BUTTONS))
        .flex()
        .justify_end()
        .items_center()
        .gap(draw.px(ROW_GAP));
    for (at, button) in card.buttons.iter().enumerate() {
        buttons = buttons.child(
            draw.wired(
                Control::Button(at),
                draw.button(button)
                    .id(("op3-button", at))
                    .debug_selector(move || format!("op3-button-{at}")),
            ),
        );
    }
    frame = frame.child(buttons);
    if let Some(observe) = live.and_then(|live| live.frame) {
        frame = observe(frame);
    }
    div()
        .size_full()
        .when(facts, |d| d.p(draw.px(GAP)))
        .flex()
        .items_center()
        .justify_center()
        .child(draw.track(frame, "card"))
        .into_any_element()
}
