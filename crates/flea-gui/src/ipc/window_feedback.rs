//! Share and footer readers observe the producers the normal window actually draws.
use super::{rows::list_area_rect, Adapter};
use crate::state::Shell;
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct FeedbackAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
    /// The focus view the window reports, "" when nothing holds focus.
    pub view: &'a str,
    /// The window's own focus, Pane.qml's activeFocus.
    pub pane_focused: bool,
    /// The list's keyboard focus, Pane.qml's listArea.activeFocus.
    pub list_focused: bool,
    /// The list draws a drop target this frame, so a drag is over one of its rows.
    pub drop_active: bool,
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
            // The ShareBrowser overlay is never opened or painted in GPUI (g2/sd1/mod.rs browser_open and browser_element have no production caller), so only active, from the card shareBrowserOpen reads, and paneRects carry state here.
            "shareBrowserRect" => String::new(),
            "shareBrowserState" => {
                let active = self
                    .shell
                    .rail
                    .as_ref()
                    .and_then(|rail| rail.network.as_ref())
                    .is_some_and(|network| network.shares.read(self.cx).model.open);
                let panes = self
                    .shell
                    .panes
                    .iter()
                    .map(|host| {
                        let rect = host
                            .as_ref()
                            .map_or_else(String::new, |host| list_area_rect(&host.list.borrow()));
                        format!("\"{rect}\"")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{{\"active\":{active},\"owner\":-1,\"rect\":\"\",\"baseUri\":\"\",\"cursor\":-1,\"paneRects\":[{panes}]}}")
            }
            // Browser.close keeps the cursor, so a closed browser still answers the row it was left on.
            "shareBrowserCursor" => self
                .shell
                .rail
                .as_ref()
                .and_then(|rail| rail.network.as_ref())
                .map_or(0, |network| network.shares.read(self.cx).model.cursor)
                .to_string(),
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
            "listingDropActive" => self.drop_active.to_string(),
            // An open row menu takes the keyboard, so menu follows opened. GPUI keeps no menu focus holder and no QML item to name, so origin and activeItem refuse while a menu is open and read idle when shut.
            "contextMenuFocusState" => {
                if self.shell.menu.menu.borrow().opened() {
                    return Some(Reply::error(3, "FLEA_IPC_ERROR flea.contextMenuFocusState unavailable fields: origin, activeItem (GPUI keeps no menu focus holder or QML focus item for an open menu)"));
                }
                format!(
                    "{{\"opened\":false,\"view\":\"{}\",\"menu\":false,\"pane\":{},\"list\":{},\"origin\":false,\"activeItem\":\"\"}}",
                    flea::json::escape(self.view),
                    self.pane_focused,
                    self.list_focused,
                )
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
