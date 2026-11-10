//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::*;
use crate::g2::status_feedback::caption::Tracked;
use gpui::{
    div, prelude::*, px, relative, AnyElement, App, Div, FontWeight, Pixels, Stateful, Window,
};
use std::rc::Rc;

/// Draws a card centred with no scrim, until op1's shell replaces it.
pub struct StandIn;

impl Draw<'_> {
    pub(super) fn wired(
        &self,
        control: Control,
        element: gpui::Stateful<Div>,
    ) -> gpui::Stateful<Div> {
        let id = match control {
            Control::Field => "field".into(),
            Control::Check(at) => format!("check:{at}"),
            Control::Button(at) => format!("button:{at}"),
            Control::Clear => "clear".into(),
            Control::Applications => "applications".into(),
            Control::Application(at) => format!("application:{at}"),
        };
        let element = if let Some(measure) = self.measure {
            measure.observe(self.track(element.relative(), &id), &id)
        } else {
            element
        };
        let element = match self.probe {
            Some(probe) => probe.track(element, id),
            None => element,
        };
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
        Tracked::caption(
            text,
            self.language,
            self.typeface.font.clone(),
            self.px(CAPTION),
            self.px(line),
            color,
        )
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
        present(card, palette, typeface, corner, room, None, None)
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
            surface: Some(self.surface),
            mount: None,
            measure: Some(self.measure),
        };
        present(card, palette, typeface, corner, room, Some(&live), None)
    }
}

impl StandIn {
    pub fn present_pressable(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        press: impl Fn(&usize, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.present_pressable_observed(card, palette, typeface, corner, room, (press, None))
    }
    /// The same card with live buttons: a click on one reports its index, as Enter on it would press it.
    pub(crate) fn present_pressable_observed(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        controls: (
            impl Fn(&usize, &mut Window, &mut App) + 'static,
            Option<&crate::ipc::card_geometry::CardGeometry>,
        ),
    ) -> AnyElement {
        let (press, probe) = controls;
        let press = Rc::new(press);
        let enabled: Vec<bool> = card.buttons.iter().map(|button| button.enabled).collect();
        // A disabled button draws as before but takes no click.
        let wire = move |control, element: Stateful<Div>| match control {
            Control::Button(at) if enabled.get(at).copied().unwrap_or(false) => {
                let press = press.clone();
                element.on_click(move |_, window, cx| press(&at, window, cx))
            }
            _ => element,
        };
        let live = Live {
            wire: &wire,
            surface: None,
            mount: None,
            measure: None,
        };
        present(card, palette, typeface, corner, room, Some(&live), probe)
    }
}

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
            surface: Some(self.surface),
            mount: self.mount,
            measure: None,
        };
        present(card, palette, typeface, corner, room, Some(&live), None)
    }
}

impl Interactive<'_> {
    pub(crate) fn present_observed(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        probe: &crate::ipc::card_geometry::CardGeometry,
    ) -> AnyElement {
        let live = Live {
            wire: self.wire,
            surface: Some(self.surface),
            mount: self.mount,
            measure: None,
        };
        present(
            card,
            palette,
            typeface,
            corner,
            room,
            Some(&live),
            Some(probe),
        )
    }
}

/// The one live path every card takes: its wired controls, and the window's surface and mounted field where it has them.
struct Live<'a> {
    wire: &'a Wire,
    surface: Option<Hsla>,
    mount: Option<&'a Mount>,
    measure: Option<&'a crate::g2::file_ops::measured::Frame>,
}

mod frame;
use frame::present;

#[path = "pressable.rs"]
mod pressable;
pub use pressable::Probed;
