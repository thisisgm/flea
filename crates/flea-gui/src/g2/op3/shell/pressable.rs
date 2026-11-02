//! The stand-in card with live buttons, and the same card recording its laid-out boxes for an IPC reader.
use super::*;
use gpui::{App, Window};

/// A pressable stand-in whose card and buttons record their laid-out boxes in a probe, for a card's IPC reader.
pub struct Probed<'a>(pub(crate) &'a crate::ipc::card_geometry::CardGeometry);

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
        StandIn.present_pressable_observed(
            card,
            palette,
            typeface,
            corner,
            room,
            (press, Some(self.0)),
        )
    }
}
