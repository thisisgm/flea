//! The Trash strip controls and the confirmation card's buttons, read from the bounds the last frame measured.
use super::readers::TrashAdapter;
use crate::g2::file_ops::{model::Dialog, press::Press};
use flea::ui_logic_file_ops::trash::Button;
use gpui::{Bounds, Pixels};

impl TrashAdapter<'_> {
    /// trashControlCentre: the strip control's centre, or the confirmation card's Cancel and Delete while that card is drawn.
    pub(super) fn control_centre(&self, arguments: &[String]) -> String {
        let Some(host) = self.host else {
            return String::new();
        };
        let id = match arguments.first().map(String::as_str) {
            Some("back") => "trash-back",
            Some("up") => "trash-up",
            Some("empty") => "trash-empty",
            Some("cancel") => return self.card_centre(Button::Cancel),
            Some("danger") => return self.card_centre(Button::Delete),
            _ => return String::new(),
        };
        let pane = host.pane.read(self.cx);
        if !pane.view().opened {
            return String::new();
        }
        let controls = pane.controls.borrow();
        controls
            .get(id)
            .map_or_else(String::new, |bounds| bounds_centre(*bounds))
    }

    /// The Trash confirmation card's button, read from the overlay's own measured frame.
    fn card_centre(&self, button: Button) -> String {
        let Some(overlay) = self.overlay else {
            return String::new();
        };
        let overlay = overlay.read(self.cx);
        if !matches!(overlay.drawn_dialog(), Some(Dialog::Delete { .. })) {
            return String::new();
        }
        overlay
            .frame
            .part(&format!("button-{}", Press::Delete(button).code()))
            .map_or_else(String::new, |part| bounds_centre(part.bounds))
    }

    /// trashEmptyState: "hovered|focused|pressed|available" of Empty Trash, read from the pane's own state.
    pub(super) fn empty_state(&self) -> String {
        let Some(host) = self.host else {
            return String::new();
        };
        let pane = host.pane.read(self.cx);
        if !pane.view().opened {
            return String::new();
        }
        let available = pane.empty_available();
        [
            pane.button.hover,
            pane.view().strip_focus && available,
            pane.button.pressed,
            available,
        ]
        .map(|on| on.to_string())
        .join("|")
    }
}

fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

fn bounds_centre(bounds: Bounds<Pixels>) -> String {
    format!(
        "{} {}",
        round(f32::from(bounds.origin.x + bounds.size.width / 2.)),
        round(f32::from(bounds.origin.y + bounds.size.height / 2.))
    )
}
