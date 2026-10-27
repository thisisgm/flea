//! Dialog readers distinguish observable closed cards from unavailable window owners.
use super::Adapter;
use crate::state::Shell;
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct DialogAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
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
            "menuDialogState" | "openWithState" | "permissionsState" | "runCardState" => {
                return Some(Reply::error(3, format!("FLEA_IPC_ERROR flea.{function} unavailable state: the normal Root has no mounted owner for this dialog")));
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
