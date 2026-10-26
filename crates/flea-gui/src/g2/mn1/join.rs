//! The window's menu mount and the selection captured before asynchronous producer replies.
use super::{lines::Env, menu::Menu};
use crate::{state::Shell, window::pane::PaneHost};
use flea::{
    jsondoc::Json,
    ui_logic_mn1::{
        build::{self, Context as MenuContext},
        Entry,
    },
};
use gpui::{Context, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Debug)]
pub struct Captured {
    pub side: usize,
    pub listing: usize,
    pub rows: Vec<usize>,
    pub cursor: usize,
    pub path: String,
    pub identity: String,
    pub id: u64,
    pub ready: bool,
    pub kinds: Vec<u32>,
    pub background: bool,
}

/// A checked choice handed to the producer that owns the action; every target remains captured.
#[derive(Clone, Debug)]
pub struct Chosen {
    pub captured: Captured,
    pub action: String,
    pub leaf: String,
    pub paths: Vec<String>,
    pub(crate) claimed: Rc<Cell<bool>>,
}

impl Chosen {
    /// A mounted producer claims the snapshot before routing it through its own effect lane.
    pub fn claim(&self) -> bool {
        !self.claimed.replace(true)
    }
}

impl gpui::EventEmitter<Chosen> for Shell {}

/// An unconsumed producer reply handed to the owner that claimed its menu snapshot.
#[derive(Clone, Debug)]
pub struct ProducerReply {
    pub side: usize,
    pub line: String,
}

impl gpui::EventEmitter<ProducerReply> for Shell {}

#[derive(Default)]
pub struct Joined {
    pub(crate) place: Option<super::place_host::Place>,
    pub menu: Rc<RefCell<Menu>>,
    pub captured: Option<Captured>,
    pub context: MenuContext,
    pub pending: Option<(String, String)>,
    pub(crate) from_sheet: bool,
    pub activating: bool,
    pub reading: bool,
    pub sequence: u64,
    pub pointer: Option<(f32, f32)>,
    pub resting: Option<(f32, f32)>,
    pub pointer_armed: bool,
    pub presentation: super::presentation::Presentation,
    pub shebang: super::shebang::Shebang,
    // Open in Agent and the Actions flyout keep their listing, replies and refusals here.
    pub actions: crate::g2::mn2::model::Actions,
    // A test's stand-in for the detached Open in Agent spawn; None spawns the real agent.
    pub agent: Option<AgentLaunch>,
    pub(crate) providers: crate::g2::sd1::join::Senders,
    /// The flyout c or P asked for, opened once the cursor menu draws.
    pub flyout: Option<&'static str>,
}

pub type AgentLaunch = Rc<dyn Fn(&std::path::Path) -> std::io::Result<()>>;

fn identity(host: &PaneHost, side: usize, background: bool) -> String {
    format!(
        "{side}:{}:{}:{:?}:{}:{background}",
        host.path, host.pane.held.listing, host.selected, host.pane.cursor
    )
}

pub(super) fn flag(data: &Json, key: &str) -> bool {
    data.get(key).and_then(Json::as_bool) == Some(true)
}

impl Shell {
    pub(crate) fn menu_opened(&self) -> bool {
        self.menu.menu.borrow().opened()
    }

    pub(crate) fn menu_identity(&self) -> String {
        if let Some(place) = &self.menu.place {
            return place.key.clone();
        }
        self.menu
            .captured
            .as_ref()
            .and_then(|capture| {
                self.panes[capture.side]
                    .as_ref()
                    .map(|host| identity(host, capture.side, capture.background))
            })
            .unwrap_or_default()
    }

    /// Nav.js lockedTarget: the denied folder and its mode while the pane draws the Locked state, else None.
    pub(crate) fn locked_folder(&self, side: usize) -> Option<(String, u32)> {
        let host = self.panes.get(side)?.as_ref()?;
        let target = host.pane.state.locked_target()?;
        let folder = if target.is_empty() {
            &host.path
        } else {
            target
        };
        Some((folder.to_owned(), host.pane.state.mode))
    }

    pub(crate) fn menu_entries(&self) -> Vec<Entry> {
        if let Some(entries) = self.place_menu_entries() {
            return entries;
        }
        let Some(capture) = &self.menu.captured else {
            return Vec::new();
        };
        self.listing_menu_entries(
            capture.side,
            capture.background,
            self.menu.context.clone(),
            if self.menu.from_sheet {
                Vec::new()
            } else {
                self.menu_hidden()
            },
        )
    }

    /// The listing menu's rows for a context and hidden set; the keyboard sheet passes none hidden so it finds every row.
    pub(crate) fn listing_menu_entries(
        &self,
        side: usize,
        background: bool,
        mut context: MenuContext,
        hidden: Vec<String>,
    ) -> Vec<Entry> {
        let Some(host) = &self.panes[side] else {
            return Vec::new();
        };
        if background {
            if let Some((_, mode)) = self.locked_folder(side) {
                return flea::ui_logic_mn1::places::locked_entries(mode, &hidden);
            }
        }
        context.hidden = hidden;
        if !background {
            self.provider_context(side, &host.path, &mut context);
        }
        context.show_hidden = host.nav.folders.borrow().hidden;
        context.sort_by = host.nav.order.key.into();
        context.has_folder_sort =
            !host.pane.swap.session.open && host.nav.folders.borrow().has(&host.path);
        self.menu_live_facts(side, &mut context);
        // Actions takes Run Script's place under Run Script's switch, so the build is asked for that row whenever the listing holds an action.
        let actions = (!background)
            .then(|| self.menu.actions.actions_entry(&[]))
            .flatten();
        if actions.is_some() && context.scripts.is_empty() {
            context.scripts.push(build::Peer {
                id: String::new(),
                label: String::new(),
            });
        }
        let mut entries = build::listing_entries(&context);
        if let (Some(at), Some(row)) = (
            entries.iter().position(|e| e.id.as_ref() == "runScript"),
            actions,
        ) {
            entries[at] = row;
        }
        if background {
            if let Some(row) = self.menu.actions.agent_entry(&context.hidden) {
                let at = entries
                    .iter()
                    .position(|e| e.id.as_ref() == "openTerminal")
                    .map_or(entries.len(), |at| at + 1);
                entries.insert(at, row);
            }
        }
        entries
    }

    pub(crate) fn open_menu(
        &mut self,
        side: usize,
        row: Option<usize>,
        at: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        // A locked pane draws the hero, never rows, so its menu is the folder's whatever rows it still holds.
        let row = row.filter(|_| self.locked_folder(side).is_none());
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if host.stopped || host.pane.busy() {
            return;
        }
        if let Some(row) = row {
            if host.pane.held.row(row).is_none() {
                return;
            }
            host.pane.cursor = row;
        }
        let background = row.is_none();
        // ContextMenu.qml openLocked: with every locked row hidden no frame opens, and the status line names the switch.
        if background {
            if let Some((_, mode)) = self.locked_folder(side) {
                let hidden = self.menu_hidden();
                if let Some(key) = flea::ui_logic_mn1::places::locked_refusal(mode, &hidden) {
                    let sentence = self.locale.catalog.tr(key);
                    self.menu_explain(sentence, cx);
                    return;
                }
            }
        }
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        // Tap.js tappedMenu: a right click opens the menu on its own pane and leaves the focus where it was.
        let rows = if background {
            Vec::new()
        } else if host.selected.is_empty() {
            vec![host.pane.cursor]
        } else {
            host.selected.clone()
        };
        self.menu.sequence += 1;
        self.menu.actions.listing_forgotten();
        let capture = Captured {
            side,
            listing: host.pane.held.listing,
            rows,
            cursor: host.pane.cursor,
            path: host.path.clone(),
            identity: identity(host, side, background),
            id: self.menu.sequence,
            ready: false,
            kinds: Vec::new(),
            background,
        };
        self.menu.context =
            self.listing_menu_context(side, &capture.rows, capture.cursor, background);
        self.menu.captured = Some(capture.clone());
        self.check_menu_shebang(window, cx);
        self.archive_menu_context(side);
        self.menu.presentation.at = Some(at);
        self.menu.pointer = Some((
            window.mouse_position().x.into(),
            window.mouse_position().y.into(),
        ));
        self.menu.resting = self.menu.pointer;
        self.menu.pointer_armed = false;
        self.keyboard.context = flea::keys::Context::Menu;
        self.keyboard.disarm();
        window.focus(&self.focus, cx);
        if let Some(host) = &mut self.panes[side] {
            host.nav.gate.menu_open = true;
        }
        if !background {
            let rows = capture
                .rows
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            self.menu_send(side, format!("{{\"c\":\"menuaction\",\"op\":\"snapshot\",\"id\":{},\"listing\":{},\"rows\":[{rows}],\"cursor\":{}}}", capture.id, capture.listing, capture.cursor), cx);
            if self.menu.captured.is_some() {
                self.providers_menu_opened(side, capture.id, window, cx);
            }
            if self.menu.captured.is_some() {
                self.arm_menu_loading(window, cx);
            }
        } else {
            self.refresh_menu(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn menu_env<'a>(&'a self, typeface: &'a crate::list::Typeface) -> Env<'a> {
        Env {
            catalog: &self.locale.catalog,
            preset: self
                .launch
                .settings
                .data
                .get("keys")
                .and_then(Json::as_str)
                .unwrap_or("default"),
            key_hints: flag(&self.launch.settings.data, "keyHints"),
            typeface,
            inset: (4. * f32::from(typeface.body) / 14.).round(),
        }
    }
}
