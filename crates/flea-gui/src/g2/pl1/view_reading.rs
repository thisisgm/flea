//! unlockState in UnlockDialog.qml's shape, from the card model and what its last paint recorded.
use super::super::probes::{ButtonProbe, Painted};
use super::super::unlock::{Part, Phase};
use super::UnlockView;
use crate::ipc::rows::{centre_of, rect_of};
use flea::json::escape;

fn rect(painted: &Painted) -> String {
    painted.get().map_or(String::new(), |b| {
        let (x, y) = (f32::from(b.origin.x), f32::from(b.origin.y));
        rect_of(x, y, f32::from(b.size.width), f32::from(b.size.height))
    })
}

fn centre(painted: &Painted) -> String {
    painted.get().map_or(String::new(), |b| {
        let (x, y) = (f32::from(b.origin.x), f32::from(b.origin.y));
        centre_of(x, y, f32::from(b.size.width), f32::from(b.size.height))
    })
}

// Ipc.qml's controlState: an unavailable button reads not enabled, as Unlock does with an empty field.
fn control(name: &str, enabled: bool, focused: bool, probe: &ButtonProbe) -> String {
    format!(
        "{{\"name\":\"{name}\",\"visible\":true,\"enabled\":{enabled},\"focused\":{focused},\"hovered\":{},\"pressed\":{},\"centre\":\"{}\",\"rect\":\"{}\"}}",
        probe.hovered.get(),
        probe.pressed.get(),
        centre(&probe.painted),
        rect(&probe.painted),
    )
}

impl UnlockView {
    /// The open card's state, or None while closed; revealed stays false because GPUI's eye never reveals.
    pub fn reading(&self) -> Option<String> {
        let card = self.host.card.as_ref().filter(|c| c.opened)?;
        let phase = match card.phase {
            Phase::Typing => "typing",
            Phase::Unlocking => "unlocking",
            Phase::Wrong => "wrong",
        };
        let focus = match card.focus {
            Some(Part::Field) => "field",
            Some(Part::Cancel) => "cancel",
            Some(Part::Unlock) => "unlock",
            None => "",
        };
        let label = if card.label.is_empty() {
            &card.device
        } else {
            &card.label
        };
        let probes = &self.probes;
        Some(format!(
            "{{\"opened\":true,\"phase\":\"{phase}\",\"note\":\"{}\",\"noteIsError\":{},\"device\":\"{}\",\"label\":\"{}\",\"subtitle\":\"{}\",\"focus\":\"{focus}\",\"masked\":{},\"rect\":\"{}\",\"controls\":[{},{}],\"revealed\":false,\"eye\":\"{}\"}}",
            escape(&self.text.note),
            card.note.as_ref().is_some_and(|(_, error)| *error),
            escape(&card.device),
            escape(label),
            escape(&self.text.subtitle),
            card.field.dots() > 0,
            rect(&probes.card),
            control("cancel", true, focus == "cancel", &probes.cancel),
            control("unlock", card.unlock_available(), focus == "unlock", &probes.unlock),
            centre(&probes.eye),
        ))
    }
}
