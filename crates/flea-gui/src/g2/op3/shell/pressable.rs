//! The stand-in card with live buttons, and the same card recording its laid-out boxes for an IPC reader.
use super::*;
use gpui::{App, Window};
use std::rc::Rc;

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
        let wire = pressable(card, press);
        let live = Live {
            wire: &wire,
            surface: None,
            mount: None,
            probe: None,
        };
        present(card, palette, typeface, corner, room, Some(&live))
    }
}

/// A pressable stand-in whose card and buttons record their laid-out boxes in a probe, for a card's IPC reader.
pub struct Probed<'a>(pub &'a crate::g2::st1::probe::Probe);

impl Probed<'_> {
    pub fn present_pressable(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        press: impl Fn(&usize, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let wire = pressable(card, press);
        let live = Live {
            wire: &wire,
            surface: None,
            mount: None,
            probe: Some(self.0),
        };
        present(card, palette, typeface, corner, room, Some(&live))
    }
}

// A click on an enabled button reports its index; a disabled button draws as before but takes no click.
fn pressable(
    card: &Card,
    press: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> impl Fn(Control, Stateful<Div>) -> Stateful<Div> {
    let press = Rc::new(press);
    let enabled: Vec<bool> = card.buttons.iter().map(|button| button.enabled).collect();
    move |control, element: Stateful<Div>| match control {
        Control::Button(at) if enabled.get(at).copied().unwrap_or(false) => {
            let press = press.clone();
            element.on_click(move |_, window, cx| press(&at, window, cx))
        }
        _ => element,
    }
}
