//! Share and footer readers observe the producers the normal window actually draws.
use super::{rows::list_area_rect, Adapter};
use crate::state::Shell;
use flea::ipc::wire::Reply;
use gpui::{App, Window};

/// The focus contextMenuFocusState reports.
pub(crate) struct MenuFocus<'a> {
    /// The focus view the window reports, "" when nothing holds focus.
    pub view: &'a str,
    /// The window's own focus, Pane.qml's activeFocus.
    pub pane: bool,
    /// The list's keyboard focus, Pane.qml's listArea.activeFocus.
    pub list: bool,
}

impl<'a> MenuFocus<'a> {
    pub(crate) fn read(shell: &Shell, view: &'a str, list: bool, window: &Window) -> Self {
        Self {
            view,
            pane: shell.focus.is_focused(window),
            list,
        }
    }
}

pub(crate) struct FeedbackAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
    pub focus: MenuFocus<'a>,
    pub share_focused: bool,
    /// IpcPreviewState.listingDropActive: a drag is over the listing's floor, past or between folder rows, landing in the shown folder.
    pub drop_active: bool,
}

impl Adapter for FeedbackAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            // GPUI-only: v0.3.12 kept Pane.clipboardText off IPC; this answers the system clipboard's text, "" with none.
            "clipboardText" => self
                .cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap_or_default(),
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
            "shareBrowserRect" => self.browser_rect(),
            "shareBrowserState" => {
                let browser = self
                    .shell
                    .rail
                    .as_ref()
                    .and_then(|rail| rail.network.as_ref())
                    .map(|network| &network.shares.read(self.cx).model);
                // v0.3.12 shareBrowserState: active and owner follow the browser even while its pane is hidden.
                let active = browser.is_some_and(|browser| browser.open);
                let owner = browser
                    .and_then(|browser| self.shell.network_origin_side(browser.origin))
                    .map_or(-1, |side| side as i32);
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
                let uri = browser.map_or("", |browser| browser.uri.as_str());
                let cursor = browser
                    .filter(|browser| browser.generation > 0)
                    .map_or(-1, |browser| browser.cursor as i64);
                let rect = self.browser_rect();
                format!("{{\"active\":{active},\"focused\":{},\"owner\":{owner},\"rect\":\"{rect}\",\"baseUri\":\"{}\",\"cursor\":{cursor},\"paneRects\":[{panes}]}}", self.share_focused, flea::json::escape(uri))
            }
            // Browser.close keeps the cursor, so a closed browser still answers the row it was left on.
            "shareBrowserCursor" => self
                .shell
                .rail
                .as_ref()
                .and_then(|rail| rail.network.as_ref())
                .map_or(-1, |network| {
                    network.shares.read(self.cx).model.cursor_ipc()
                })
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
            // An open row menu takes the keyboard, so menu follows opened and the opener (origin) holds no focus.
            // GPUI has no QML focus item to name, so activeItem stays empty.
            "contextMenuFocusState" => {
                let opened = self.shell.menu.menu.borrow().opened();
                format!(
                    "{{\"opened\":{opened},\"view\":\"{}\",\"menu\":{opened},\"pane\":{},\"list\":{},\"origin\":false,\"activeItem\":\"\"}}",
                    flea::json::escape(self.focus.view),
                    self.focus.pane,
                    self.focus.list,
                )
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

impl FeedbackAdapter<'_> {
    fn browser_rect(&self) -> String {
        let Some(network) = self
            .shell
            .rail
            .as_ref()
            .and_then(|rail| rail.network.as_ref())
        else {
            return String::new();
        };
        let browser = network.shares.read(self.cx);
        if self.shell.network_side(browser.model.origin).is_none() {
            return String::new();
        }
        browser.painted_bounds().map_or_else(String::new, |bounds| {
            super::rows::rect_of(
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            )
        })
    }
}

impl Shell {
    /// PaneWire.qml's floor DropInto: List and Grid only, under the rows, so a folder row's own target hides it.
    pub(crate) fn listing_floor_active(
        &self,
        host: &crate::window::pane::PaneHost,
        list: &crate::list::ListState,
        states: &crate::list::RowStates,
        window: &gpui::Window,
        cx: &App,
    ) -> bool {
        let floor = match host.drawn_view {
            Some(crate::window::startup::View::List) => Some(list.drawn.bounds),
            Some(crate::window::startup::View::Grid) => {
                host.grid.as_ref().map(|grid| grid.state.borrow().bounds)
            }
            _ => None,
        };
        cx.has_active_drag()
            && states.drop_target.is_none()
            && floor.is_some_and(|bounds| bounds.contains(&window.mouse_position()))
    }
}
