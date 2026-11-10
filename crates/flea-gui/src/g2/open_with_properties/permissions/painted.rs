//! permissionsState's painted half: the card, body and control boxes the latest frame laid out, and the notes it shows.
use super::{Permissions, Stop};
use crate::{
    g2::open_with_properties::shell::{Block, Card, Ink},
    ipc::card_geometry::{text, CardGeometry},
};
use flea::{i18n::I18n, jsondoc::Json};

fn notes(
    blocks: &[Block],
    errors: &mut Vec<String>,
    summary: &mut Vec<String>,
    focus: &mut Option<Stop>,
) {
    for block in blocks {
        match block {
            Block::Section { lines, .. } => summary.extend(lines.iter().cloned()),
            Block::Note {
                text,
                ink: Ink::Error,
                ..
            } => errors.push(text.clone()),
            Block::Note {
                text,
                ink: Ink::Foreground,
                ..
            } => summary.push(text.clone()),
            Block::Field(field) if field.focused => *focus = Some(Stop::Octal),
            Block::Grid { rows, .. } => {
                if let Some(at) = rows
                    .iter()
                    .flat_map(|(_, checks)| checks)
                    .position(|check| check.focused)
                {
                    *focus = Some(Stop::Box(at));
                }
            }
            Block::Stack(parts) => notes(parts, errors, summary, focus),
            _ => {}
        }
    }
}

/// The probe id each `state()` control was drawn under.
fn drawn_id(name: &str, box_at: &mut usize) -> (String, Stop) {
    match name {
        "Octal" => ("field".into(), Stop::Octal),
        "Cancel" => ("button:0".into(), Stop::Cancel),
        "Apply" => ("button:1".into(), Stop::Apply),
        _ => {
            *box_at += 1;
            (format!("check:{}", *box_at - 1), Stop::Box(*box_at - 1))
        }
    }
}

impl Permissions {
    /// `state()` plus what the open card painted: its boxes, each control's box and pointer state, and its shown notes.
    pub(crate) fn painted_state(
        &self,
        i18n: &I18n,
        painted: Option<&Card>,
        probe: &CardGeometry,
    ) -> Json {
        let mut state = self.state();
        let card = painted.cloned().unwrap_or_else(|| self.card(i18n));
        let (mut errors, mut summary) = (Vec::new(), Vec::new());
        let mut focus = card
            .buttons
            .iter()
            .position(|button| button.focused)
            .map(|at| if at == 0 { Stop::Cancel } else { Stop::Apply });
        notes(&card.body, &mut errors, &mut summary, &mut focus);
        let Json::Obj(fields) = &mut state else {
            return state;
        };
        let mut box_at = 0;
        for (key, value) in fields.iter_mut() {
            let (true, Json::Arr(controls)) = (key == "controls", value) else {
                continue;
            };
            for control in controls.iter_mut() {
                let Json::Obj(own) = control else { continue };
                let get = |name: &str| own.iter().find(|(k, _)| k == name).map(|(_, v)| v);
                let name = get("name")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let enabled = get("enabled").and_then(Json::as_bool).unwrap_or(false);
                let (id, stop) = drawn_id(&name, &mut box_at);
                let focused = painted.is_some() && focus == Some(stop);
                for (_, value) in own.iter_mut().filter(|(key, _)| key.as_str() == "focused") {
                    *value = Json::Bool(focused);
                }
                let Json::Obj(drawn) = probe.control(&id, &name, focused, enabled) else {
                    continue;
                };
                own.extend(
                    drawn
                        .into_iter()
                        .filter(|(k, _)| !matches!(k.as_str(), "name" | "enabled" | "focused")),
                );
            }
        }
        fields.extend([
            ("title".into(), text(card.title)),
            ("displayedError".into(), text(errors.join("\n"))),
            ("displayedSummary".into(), text(summary.join("\n"))),
            ("rect".into(), text(probe.probe.rect("card"))),
            ("bodyRect".into(), text(probe.probe.rect("scroll"))),
            ("octalFrame".into(), text(probe.role("field"))),
        ]);
        state
    }
}
