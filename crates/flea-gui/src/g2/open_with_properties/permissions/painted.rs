//! permissionsState's painted half: the card, body and control boxes the latest frame laid out, and the notes it shows.
use super::Permissions;
use crate::{
    g2::open_with_properties::shell::{Block, Card, Ink},
    ipc::card_geometry::{text, CardGeometry},
};
use flea::{i18n::I18n, jsondoc::Json};

fn notes(blocks: &[Block], errors: &mut Vec<String>, summary: &mut Vec<String>) {
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
            Block::Stack(parts) => notes(parts, errors, summary),
            _ => {}
        }
    }
}

/// The probe id each `state()` control was drawn under.
fn drawn_id(name: &str, box_at: &mut usize) -> String {
    match name {
        "Octal" => "field".into(),
        "Cancel" => "button:0".into(),
        "Apply" => "button:1".into(),
        _ => {
            *box_at += 1;
            format!("check:{}", *box_at - 1)
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
        notes(&card.body, &mut errors, &mut summary);
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
                let focused = get("focused").and_then(Json::as_bool).unwrap_or(false);
                let enabled = get("enabled").and_then(Json::as_bool).unwrap_or(false);
                let Json::Obj(drawn) =
                    probe.control(&drawn_id(&name, &mut box_at), &name, focused, enabled)
                else {
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
            ("bodyRect".into(), text(probe.probe.rect("body"))),
            ("octalFrame".into(), text(probe.role("field"))),
        ]);
        state
    }
}
