//! Dialog readers distinguish observable closed cards from unavailable window owners.
use super::Adapter;
use crate::{
    g2::op3::openwith::{OpenWith, Stop},
    state::Shell,
};
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct DialogAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
    /// The list holds the keyboard and no listing is in flight, the rest state of Pane.qml's permissions inputReady.
    pub input_ready: bool,
}

// An open card's painted bounds are not recorded, so its reader refuses instead of inventing them.
fn open_card(function: &str, fields: &str) -> Reply {
    Reply::error(
        3,
        format!("FLEA_IPC_ERROR flea.{function} unavailable fields: {fields} (GPUI does not record the card's painted bounds)"),
    )
}

impl Adapter for DialogAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            "dialogOpen" => self
                .shell
                .rail
                .as_ref()
                .and_then(|rail| rail.network.as_ref())
                .is_some_and(|network| network.card.read(self.cx).model.opened)
                .to_string(),
            "unlockState" => self
                .shell
                .rail
                .as_ref()
                .and_then(|rail| rail.unlock.as_ref())
                .and_then(|unlock| unlock.view.read(self.cx).reading())
                .unwrap_or_else(|| "{\"opened\":false}".into()),
            // The menu actions' card is Open With or the run card; with neither open the Root has no dialog to describe.
            "menuDialogState" => {
                if self.shell.open_with.is_some() || self.shell.run_card.is_some() {
                    return Some(open_card(function, "rect, controls, confirmation"));
                }
                "{\"opened\":false}".into()
            }
            // The card's model facts the design CI drives; every other ui/Ipc.qml field is named under unavailable, as a refusal would.
            "openWithState" => match &self.shell.open_with {
                Some(open) => open_with_state(&open.card),
                None => "{\"opened\":false}".into(),
            },
            "runCardState" => {
                if self.shell.run_card.is_some() {
                    return Some(open_card(function, "rect, controls"));
                }
                "{\"opened\":false}".into()
            }
            // An open card answers its own state; a closed one only the list's rest input state.
            "permissionsState" => match self.shell.permissions_state(self.cx) {
                Some(state) => flea::jsondoc::render(&state).trim_end().to_owned(),
                None => format!("{{\"opened\":false,\"inputReady\":{}}}", self.input_ready),
            },
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

/// The ui/Ipc.qml openWithState fields this reader does not send: the painted bounds GPUI does not record, and the catalogue facts it does not carry yet.
const OPEN_WITH_UNAVAILABLE: &str = "busy, kind, mime, name, error, rows, rect, rowHeight, eyebrowHeight, titleRuleRect, searchRect, gap, searchTrail, listRect, rowRect, controls[].visible, controls[].hovered, controls[].pressed, controls[].centre, controls[].rect";

fn open_with_state(card: &OpenWith) -> String {
    let controls = [
        ("Field", Stop::Search, true),
        ("Applications", Stop::List, true),
        ("Always", Stop::Always, true),
        ("Cancel", Stop::Cancel, true),
        ("Open", Stop::Open, card.can_open()),
    ]
    .map(|(name, stop, enabled)| {
        format!(
            r#"{{"name":"{name}","focused":{},"enabled":{enabled}}}"#,
            card.focus == stop
        )
    })
    .join(",");
    format!(
        r#"{{"opened":true,"committing":{},"always":{},"search":"{}","cursor":{},"controls":[{controls}],"unavailable":"{OPEN_WITH_UNAVAILABLE}"}}"#,
        card.opening(),
        card.always,
        flea::json::escape(&card.query),
        card.cursor
    )
}
