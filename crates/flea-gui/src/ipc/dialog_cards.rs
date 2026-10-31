//! Dialog readers distinguish observable closed cards from unavailable window owners.
use super::Adapter;
use crate::state::Shell;
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
            "unlockState" => {
                let open = self
                    .shell
                    .rail
                    .as_ref()
                    .and_then(|rail| rail.unlock.as_ref())
                    .is_some_and(|unlock| unlock.view.read(self.cx).host.opened());
                if open {
                    return Some(Reply::error(3, "FLEA_IPC_ERROR flea.unlockState unavailable fields: rect, controls, eye (UnlockView does not record painted control bounds or pointer feedback)"));
                }
                "{\"opened\":false}".into()
            }
            // The menu actions' card is Open With or the run card; with neither open the Root has no dialog to describe.
            "menuDialogState" => {
                if self.shell.open_with.is_some() || self.shell.run_card.is_some() {
                    return Some(open_card(function, "rect, controls, confirmation"));
                }
                "{\"opened\":false}".into()
            }
            "openWithState" => {
                if self.shell.open_with.is_some() {
                    return Some(open_card(
                        function,
                        "rect, titleRuleRect, searchRect, listRect, rowRect, controls",
                    ));
                }
                "{\"opened\":false}".into()
            }
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
