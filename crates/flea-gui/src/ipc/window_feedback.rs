//! Share and footer readers observe the producers the normal window actually draws.
use super::Adapter;
use crate::state::Shell;
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct FeedbackAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
}

impl Adapter for FeedbackAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            "shareBrowserOpen" | "shareBrowserEntries" => {
                let browser = self
                    .shell
                    .rail
                    .as_ref()
                    .and_then(|rail| rail.network.as_ref())
                    .map(|network| &network.shares.read(self.cx).model);
                if function == "shareBrowserOpen" {
                    browser.is_some_and(|browser| browser.open).to_string()
                } else {
                    browser.map_or_else(String::new, |browser| browser.names.join("\n"))
                }
            }
            "statusBusy" | "statusSecondary" => {
                let Some(footer) = &self.shell.surfaces[0].footer else {
                    return Some(Reply::error(3, format!("FLEA_IPC_ERROR flea.{function} unavailable state: the window footer is not mounted")));
                };
                let footer = footer.read(self.cx);
                let entity = footer.message_line();
                let line = entity.read(self.cx).drawn();
                if function == "statusBusy" {
                    line.busy.to_string()
                } else {
                    format!(
                        "{}{}{}{}",
                        line.hint,
                        if line.undo.is_empty() {
                            ""
                        } else {
                            flea::ui_logic_fb1::status::SEP
                        },
                        line.undo,
                        line.extra,
                    )
                }
            }
            "contextMenuFocusState" => {
                return Some(Reply::error(3, "FLEA_IPC_ERROR flea.contextMenuFocusState unavailable field: activeItem (the oracle serializes a runtime QML object, with no GPUI equivalent)"));
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
