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

#[path = "join_target.rs"]
mod target;

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
    pub target: Option<flea::protocol::RowFacts>,
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
    pub(crate) text: Option<(usize, u64)>,
    pub(crate) text_paste: bool,
    pub tags: super::tags::Tags,
    pub(crate) place: Option<super::place_host::Place>,
    pub menu: Rc<RefCell<Menu>>,
    pub captured: Option<Captured>,
    pub(crate) after_navigation: Option<super::target_action::AfterNavigation>,
    pub(crate) rail: Option<crate::window::dispatch::rail_menu::Target>,
    pub context: MenuContext,
    pub pending: Option<(String, String)>,
    pub(crate) from_sheet: bool,
    /// An outside `flea --action` waiting on this capture's row; retiring the menu unanswered refuses it.
    pub(crate) external: Option<crate::window::external_action::Waiting>,
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
    pub(crate) plugin_runtime: crate::g2::plugins::menu::Menus,
    // A test's stand-in for the detached Open in Agent spawn; None spawns the real agent.
    pub agent: Option<AgentLaunch>,
    pub(crate) providers: crate::g2::sd1::join::Senders,
    pub(crate) rclone: [Rc<RefCell<crate::g2::sd2::RcloneSend>>; 2],
    /// The flyout c or P asked for, opened once the cursor menu draws.
    pub flyout: Option<&'static str>,
    pub header: bool,
    pub tags_reading: bool,
    // Open With flyout launches still waiting on their answer: the side and the menu id whose snapshot they hold.
    pub(crate) launching: Vec<(usize, u64)>,
    // Each side's Flash to drive zip probe, asked as the target row changes and read when a menu opens.
    pub(crate) zip: [super::zip::ZipProbe; 2],
}

pub type AgentLaunch = Rc<dyn Fn(&std::path::Path) -> std::io::Result<()>>;

pub(crate) fn identity(host: &PaneHost, side: usize, background: bool) -> String {
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
        self.menu_entries_keeping("")
    }

    /// The live rows a chosen leaf is checked against: MenuContext.js validateChoice keeps a hidden row whose flyout c or P opened alone.
    pub(crate) fn menu_choice_entries(&self, action: &str, leaf: &str) -> Vec<Entry> {
        let lone = !leaf.is_empty() && !flea::ui_logic_mn1::rows::flyout_entries(action).is_empty();
        self.menu_entries_keeping(if lone { action } else { "" })
    }

    fn menu_entries_keeping(&self, keep: &str) -> Vec<Entry> {
        if let Some(rows) = self.tab_menu_entries() {
            return rows;
        }
        if self.menu.text.is_some() {
            return self.rename_menu_entries();
        }
        if let Some(target) = &self.menu.rail {
            return target.rows();
        }
        if let Some(entries) = self.place_menu_entries() {
            return entries;
        }
        let Some(capture) = &self.menu.captured else {
            return Vec::new();
        };
        if self.menu.header {
            return self.header_menu_entries(capture.side);
        }
        let mut hidden = if self.menu.from_sheet {
            Vec::new()
        } else {
            self.menu_hidden()
        };
        hidden.retain(|id| id != keep);
        self.listing_menu_entries(
            capture.side,
            capture.background,
            self.menu.context.clone(),
            hidden,
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
            if let Some((_, mode)) = self.locked_folder(side).filter(|_| {
                self.menu
                    .captured
                    .as_ref()
                    .is_none_or(|capture| capture.path == host.path)
            }) {
                return flea::ui_logic_mn1::places::locked_entries(mode, &hidden);
            }
        }
        context.hidden = hidden;
        if !self.can_tag(side) {
            context.hidden.push("tags".into());
        }
        if !background {
            let base = self
                .menu
                .captured
                .as_ref()
                .map_or(host.path.as_str(), |capture| capture.path.as_str());
            self.provider_context(side, base, &mut context);
            context.rclone_remotes = self.menu.rclone[side]
                .borrow()
                .flyout()
                .into_iter()
                .map(|row| row.label)
                .collect();
        }
        context.show_hidden = host.nav.folders.borrow().hidden;
        self.menu_folder_facts(host, &mut context);
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
        let plugin_rows = self.plugin_action_rows(side, background, &context.hidden);
        if !plugin_rows.is_empty() {
            crate::g2::plugins::menu::insert_actions(&mut entries, plugin_rows);
        }
        self.plugin_send_rows(&mut entries, background, &context.hidden);
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
        self.open_menu_with(side, row, at, false, window, cx);
    }

    pub(crate) fn open_menu_with(
        &mut self,
        side: usize,
        row: Option<usize>,
        at: (f32, f32),
        header: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_menu(side, row, at, (header, None), window, cx);
    }

    /// Finder's right click in Columns: the menu takes the row, or the selection it is in, and the cursor stays.
    pub(crate) fn open_menu_in_place(
        &mut self,
        side: usize,
        row: usize,
        at: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_menu_kept(side, Some(row), at, (false, None, true), window, cx);
    }

    pub(crate) fn begin_menu(
        &mut self,
        side: usize,
        row: Option<usize>,
        at: (f32, f32),
        (header, target): (bool, Option<super::target::PathMenu>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_menu_kept(side, row, at, (header, target, false), window, cx);
    }

    /// keep holds the cursor and the selection, the menu taking the row or the selection it is in.
    fn begin_menu_kept(
        &mut self,
        side: usize,
        row: Option<usize>,
        at: (f32, f32),
        (header, target, keep): (bool, Option<super::target::PathMenu>, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        self.menu.header = header;
        // A locked pane draws the hero, never rows, so its menu is the folder's whatever rows it still holds.
        let row = row.filter(|_| target.is_some() || self.locked_folder(side).is_none());
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if host.stopped || host.pane.busy() {
            return;
        }
        if let Some(row) = row.filter(|_| target.is_none()) {
            if host.pane.held.row(row).is_none() {
                return;
            }
            if !keep {
                host.pane.cursor = row;
            }
        }
        let background = row.is_none();
        // ContextMenu.qml openLocked: with every locked row hidden no frame opens, and the status line names the switch.
        if background && target.is_none() {
            if let Some((_, mode)) = self.locked_folder(side) {
                let hidden = self.menu_hidden();
                if let Some(key) = flea::ui_logic_mn1::places::locked_refusal(mode, &hidden) {
                    let sentence = self.locale.catalog.tr(key);
                    self.menu_explain(sentence, cx);
                    return;
                }
                if let Some(key) = flea::ui_logic_mn1::places::locked_notice(&hidden) {
                    let sentence = self.locale.catalog.tr(key);
                    self.menu_explain(sentence, cx);
                }
            }
        }
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        // Tap.js tappedMenu: a right click opens the menu on its own pane and leaves the focus where it was.
        let cursor = if target.is_some() {
            0
        } else {
            row.filter(|_| keep).unwrap_or(host.pane.cursor)
        };
        let rows = if background {
            Vec::new()
        } else if target.is_some()
            || host.selected.is_empty()
            || (keep && !host.selected.contains(&cursor))
        {
            vec![cursor]
        } else {
            host.selected.clone()
        };
        self.menu.sequence += 1;
        self.menu.actions.listing_forgotten();
        let capture = Captured {
            side,
            listing: host.pane.held.listing,
            rows,
            cursor,
            path: target
                .as_ref()
                .map_or_else(|| host.path.clone(), |target| target.base.clone()),
            identity: identity(host, side, background),
            id: self.menu.sequence,
            ready: false,
            kinds: Vec::new(),
            background,
            target: target.and_then(|target| target.row),
        };
        // A right click that moves the cursor onto a zip asks about it ahead of the snapshot.
        self.check_menu_zip(side);
        self.menu.context = self.captured_menu_context(&capture);
        self.menu.captured = Some(capture.clone());
        if !header {
            self.prepare_plugin_menu(window, cx);
        }
        self.ask_tag_states(false, cx);
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
            self.menu_send(side, capture.snapshot_request(), cx);
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
            inset: (4. * f32::from(typeface.base) / 12.).round(),
        }
    }
}
