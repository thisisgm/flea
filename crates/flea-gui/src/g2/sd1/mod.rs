//! LocalSend discovery and the Taildrop/provider send picker; every process runs through the host as an Action.
mod activation;
pub(crate) mod airdrop;
pub(crate) mod airdrop_join;
pub(crate) mod airdrop_pids;
mod dropbox_status;
pub mod ipc;
pub(crate) mod join;
#[cfg(test)]
mod join_tests;
mod picker;
pub(crate) mod reader;
pub mod seams;
#[cfg(test)]
mod tests;
pub mod view;
#[cfg(test)]
mod view_split_tests;
#[cfg(test)]
mod view_tests;

use crate::list::{Palette, Typeface};
use flea::ui_logic_mn1::build::Peer;
use flea::ui_logic_sd1::providers::{self, Facts, Provider, Role};
use flea::ui_logic_sd1::{share, Send};
use seams::ProviderState;
use std::{cell::RefCell, rc::Rc};

/// One pane backend's provider send state, one per backend since each numbers its localsend requests from 1.
#[derive(Default)]
pub struct ProviderSend {
    db: dropbox_status::Daemon,
    next: u64,
    providers: Vec<Provider>,
    gated: bool,
    hidden: Vec<String>,
    // The facts the open menu was handed, kept so a plugin toggle re-derives rows without a spawn.
    facts: Facts,
    // LocalSend discovery: asked on menu open, kept warm, answered once.
    ls_asking: Option<u64>,
    ls_peers: Vec<Peer>,
    ls_reason: String,
    ls_checking: bool,
    ls_answered_once: bool,
    ls_asked_at_ms: Option<u64>,
    // Taildrop: asked on menu open like its machines; no warm window, as ui/Taildrop.qml asks.
    td_peers: Vec<Peer>,
    td_addresses: std::collections::HashMap<String, String>,
    td_reason: String,
    td_code: String,
    td_checking: bool,
    // The newest status ask's number, so an older exit can never stand for it.
    td_generation: u64,
    // A menu opened while the one status run flew; ask again when it lands, as refreshWaiting does.
    td_waiting: bool,
    menu_open: bool,
    send_command: String,
    // One LocalSend send per request id, newest last; each ends exactly once on its verdict.
    sends: Vec<Send>,
    // The ShareBrowser overlay state shared with its Element and its IPC read-back.
    browser: Rc<RefCell<share::ShareBrowser>>,
    rows: Rc<RefCell<view::ShapedShares>>,
    probe: Rc<RefCell<view::Probe>>,
    link: share::ShareLink,
    // Times the peer rows were rebuilt, for tests and the headless gate.
    pub builds: std::cell::Cell<usize>,
}

impl ProviderSend {
    fn request_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn provider(&self, role: Role) -> Provider {
        providers::provider(&self.providers, role)
            .cloned()
            .unwrap_or_default()
    }

    // One row per provider, suppressed while disabled or failed once the st2 join lands.
    fn derive_providers(&mut self, states: Option<&[ProviderState]>) {
        self.providers = [
            Role::Tailscale,
            Role::LocalSend,
            Role::Dropbox,
            Role::AirDrop,
        ]
        .iter()
        .map(|role| Provider {
            plugin: role.plugin_id().into(),
            installed: installed(*role, &self.facts),
            enabled: states
                .and_then(|states| states.iter().find(|s| s.plugin == role.plugin_id()))
                .map(|s| s.enabled)
                .unwrap_or(true),
            failed: states
                .and_then(|states| states.iter().find(|s| s.plugin == role.plugin_id()))
                .map(|s| s.failed)
                .unwrap_or(false),
        })
        .collect();
    }

    pub fn row_on(&self, role: Role) -> bool {
        if self.gated {
            self.provider(role).row_on(&self.hidden, role.menu_id())
        } else {
            // AirDrop ships off, so until the plugin listing lands its row stays out.
            role != Role::AirDrop
                && self.provider(role).installed
                && !self.hidden.iter().any(|id| id == role.menu_id())
        }
    }

    // The peers mn1 draws under the provider row, by device name; empty while reading.
    pub fn peers_for(&self, role: Role) -> Vec<Peer> {
        match role {
            Role::LocalSend => self.ls_peers.clone(),
            Role::Tailscale => self.td_peers.clone(),
            Role::Dropbox | Role::AirDrop => Vec::new(),
        }
    }

    pub fn localsend_checking(&self) -> bool {
        self.ls_checking
    }

    // The last peers reply's reason, which mn1 words when no device answered.
    pub fn localsend_reason(&self) -> &str {
        &self.ls_reason
    }

    // MenuContext.js dims the row only while a look is in flight before the first answer.
    pub fn localsend_answered(&self) -> bool {
        self.ls_answered_once
    }

    pub fn taildrop_refreshing(&self) -> bool {
        self.td_checking
    }

    pub fn taildrop_code(&self) -> &str {
        &self.td_code
    }

    // Live sends only: a terminal verdict is pruned in `line`, and backend death in `backend_dead`.
    pub fn live_sends(&self) -> usize {
        self.sends.len()
    }

    // The ShareBrowser overlay over the listing slot; the shares list arrives from pl2 at join.
    pub fn browser_shared(&self) -> Rc<RefCell<share::ShareBrowser>> {
        Rc::clone(&self.browser)
    }

    pub fn browser_probe(&self) -> Rc<RefCell<view::Probe>> {
        Rc::clone(&self.probe)
    }

    // The mountable overlay view for one Slots.states side: G1 mounts it, then drains browser_take_pick.
    pub fn browser_view(&self, typeface: Typeface, palette: Palette) -> view::ShareBrowserView {
        view::ShareBrowserView::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            typeface,
            palette,
        )
    }

    // The overlay Element over the listing slot, drawn from this host's browser state each frame.
    pub fn browser_element(
        &self,
        typeface: Typeface,
        palette: Palette,
    ) -> view::ShareBrowserElement {
        view::ShareBrowserElement::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            typeface,
            palette,
        )
    }

    // A row click makes that row the cursor and its share the pick; a gap or shut overlay picks nothing.
    pub fn browser_click(&mut self, row: Option<usize>) -> Option<(String, String)> {
        self.browser.borrow_mut().pick(row)
    }

    // The pick an overlay click published for the host to open, taken once.
    pub fn browser_take_pick(&self) -> Option<(String, String)> {
        self.probe.borrow_mut().pick.take()
    }

    pub fn browser_open(&mut self, uri: &str, label: &str, names: &[String]) {
        self.browser.borrow_mut().open(uri, label, names);
    }

    pub fn browser_close(&mut self) -> bool {
        self.browser.borrow_mut().close()
    }

    pub fn browser_move(&mut self, delta: i64) {
        self.browser.borrow_mut().move_cursor(delta);
    }

    pub fn browser_activate(&self) -> Option<(String, String)> {
        self.browser.borrow().activate()
    }

    pub fn browser_open_ipc(&self) -> bool {
        self.browser.borrow().active
    }

    pub fn browser_entries_ipc(&self) -> String {
        self.browser.borrow().entries_text()
    }

    pub fn browser_cursor_ipc(&self) -> i64 {
        self.browser.borrow().cursor_ipc()
    }
}

fn installed(role: Role, facts: &Facts) -> bool {
    match role {
        // LocalSend drives localsend-cli, never the app's own window, as providers.rs documents.
        Role::LocalSend => facts.localsend_installed || !facts.localsend_command.is_empty(),
        // Pane.qml taildropInstalled: an installed Tailscale that cannot run keeps its row, which then reads unavailable.
        Role::Tailscale => {
            facts.tailscale_installed
                || !facts.tailscale_command.is_empty()
                || facts.tailscale_unusable
        }
        Role::Dropbox => facts.dropbox_installed || !facts.dropbox_command.is_empty(),
        Role::AirDrop => !facts.owly_command.is_empty(),
    }
}

#[test]
fn uish_misc_unusable_provider_facts_keep_the_disabled_row_and_no_command() {
    let facts = providers::parse_facts(r#"{"taildrop":{"installed":true,"command":"","reason":"not executable"},"dropbox":{"installed":true,"command":"","reason":"not executable"},"dropboxInfo":"{}"}"#).unwrap();
    assert!(installed(Role::Tailscale, &facts));
    assert!(installed(Role::Dropbox, &facts));
    assert!(!installed(Role::LocalSend, &facts));
    assert!(facts.tailscale_command.is_empty() && facts.dropbox_command.is_empty());
    struct NoWire;
    impl seams::Wire for NoWire {
        fn send(&self, _: String) -> bool {
            panic!("an unusable provider must start no discovery")
        }
    }
    let mut sender = ProviderSend::default();
    assert!(sender.menu_opened(&NoWire, &facts, None, &[], 0).is_empty());
    assert!(sender.row_on(Role::Tailscale) && sender.row_on(Role::Dropbox));
    assert!(sender.dropbox_refresh().is_none());
}
