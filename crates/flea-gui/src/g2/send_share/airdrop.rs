//! One window's AirDrop over owly (MIT, by t4t5): doctor, the radio's ten minutes, found devices and sends; airdrop_join.rs runs each Act.
use super::airdrop_shared::{Seen, Shared};
use super::seams::Wire;
use flea::i18n::I18n;
use flea::ui_logic_context_menus::build::Peer;
use flea::ui_logic_plugin_settings::airdrop::Page;
use flea::ui_logic_send_share::airdrop::{self as owly, Adapter, Readiness};

// A doctor answer is good this long; the card and owlyd rarely change under an open window.
const DOCTOR_FRESH_MS: u64 = 30_000;
// After a browse that failed, a menu opened sooner than this does not start another.
const BROWSE_GAP_MS: u64 = 1_000;
// Files saved this close together are one transfer, said as a count.
const RECEIVED_GROUP_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Radio {
    #[default]
    Off,
    // `owly radio up` runs but awdl0 is not up yet.
    Starting,
    On,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proc {
    Radio,
    Receive,
    Browse,
}

impl Proc {
    pub const ALL: [Proc; 3] = [Proc::Radio, Proc::Receive, Proc::Browse];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    // A captured `owly doctor --json`, answered through doctor_done with its generation.
    Doctor(u64, Vec<String>),
    // A long run whose lines come back through line() and whose exit code through ended(), under its generation.
    Start(Proc, u64, Vec<String>),
    // SIGTERM, so `owly radio up` gives Wi-Fi back before it exits.
    Stop(Proc),
    Say(String, bool),
    // The menu and the plugin page draw again.
    Refresh,
    // Another window owns the radio: its owner is asked to stop, off the window's thread.
    AskStop,
}

#[derive(Default)]
pub struct AirDrop {
    pub adapter: Adapter,
    owly: String,
    doctor_gen: u64,
    // The doctor generation whose answer is awaited, if one is.
    doctor_wait: Option<u64>,
    doctor_at_ms: Option<u64>,
    // This window's own radio; another window's is `elsewhere`, read from `shared`.
    pub radio: Radio,
    elsewhere: Radio,
    pub(crate) shared: Shared,
    // Each Receiving choice counts up, so a look taken before the latest one never lands.
    pub(super) asks: u64,
    // Each shut menu counts up, so a menu open still waiting for its look starts no browse.
    pub(super) menus: u64,
    // The folder of a Receiving On still waiting for its look, whose withdrawal an Off meanwhile asks again.
    pub(super) on_wait: Option<String>,
    watching: bool,
    // The radio run sent SIGTERM and not yet exited: the machine's lock stays held until it has.
    stopping: Option<u64>,
    // Receiving On asked while the machine's radio was still going down: the folder it starts with once free.
    again: Option<String>,
    // One generation per process kind, so a line or exit from a run already replaced changes nothing.
    gens: [u64; Proc::ALL.len()],
    // owly's own error lines from the radio and the receiver, for the sentence said when either fails.
    said: String,
    browsing: bool,
    browse_at_ms: u64,
    browse_failed_ms: Option<u64>,
    menu_open: bool,
    // When the last file was saved, and how many its transfer has saved so far.
    received: Option<(u64, usize)>,
    folder: String,
    devices: Vec<owly::Peer>,
    // The window's sends, Omdrop's too: one numbering, so no answer reaches the wrong device.
    pub(super) outbox: super::airdrop_outbox::Outbox,
}

fn slot(proc: Proc) -> usize {
    proc as usize
}

impl AirDrop {
    // The backend's facts name owly's path, or nothing when it is not on PATH.
    pub fn facts(&mut self, owly: &str) {
        self.owly = owly.to_string();
        if owly.is_empty() {
            self.adapter = Adapter {
                readiness: Readiness::Missing,
                card: String::new(),
            };
            self.doctor_at_ms = None;
            // A doctor still running answers for an owly that is gone, so its answer must not land.
            self.doctor_wait = None;
        } else if self.adapter.readiness == Readiness::Missing {
            self.adapter = Adapter::default();
        }
    }

    fn next_gen(&mut self, proc: Proc) -> u64 {
        self.gens[slot(proc)] += 1;
        self.gens[slot(proc)]
    }

    #[cfg(test)]
    pub(super) fn runs(&self, proc: Proc) -> u64 {
        self.gens[slot(proc)]
    }

    // Asks owly doctor unless an answer is fresh, one is in flight, or the radio is up (doctor then reports the lease).
    pub fn ask_doctor(&mut self, now_ms: u64) -> Vec<Act> {
        let fresh = self
            .doctor_at_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < DOCTOR_FRESH_MS);
        if self.owly.is_empty() || self.doctor_wait.is_some() || fresh || self.on() {
            return Vec::new();
        }
        self.doctor_gen += 1;
        self.doctor_wait = Some(self.doctor_gen);
        vec![Act::Doctor(self.doctor_gen, owly::doctor_argv(&self.owly))]
    }

    pub fn doctor_done(
        &mut self,
        generation: u64,
        exit_code: i32,
        stdout: &str,
        now_ms: u64,
    ) -> Vec<Act> {
        if self.doctor_wait != Some(generation) {
            return Vec::new();
        }
        self.doctor_wait = None;
        self.doctor_at_ms = Some(now_ms);
        self.adapter = owly::doctor(stdout, exit_code);
        vec![Act::Refresh]
    }

    // Any window browses while the machine's radio is up, its own or another window's.
    fn browse(&mut self, now_ms: u64) -> Vec<Act> {
        if self.browsing || !self.menu_open || self.machine() != Radio::On || self.owly.is_empty() {
            return Vec::new();
        }
        self.browsing = true;
        self.browse_at_ms = now_ms;
        let generation = self.next_gen(Proc::Browse);
        vec![Act::Start(
            Proc::Browse,
            generation,
            owly::browse_argv(&self.owly),
        )]
    }

    pub fn menu_opened(&mut self, now_ms: u64, seen: &Seen) -> Vec<Act> {
        self.menu_open = true;
        let mut acts = self.look(seen);
        acts.extend(self.ask_doctor(now_ms));
        let failed_late = self
            .browse_failed_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < BROWSE_GAP_MS);
        if !failed_late {
            self.browse_failed_ms = None;
            acts.extend(self.browse(now_ms));
        }
        acts
    }

    // The look a menu open waited for, with the shut-menu count it was asked under: a menu shut meanwhile starts nothing.
    pub(super) fn menu_looked(&mut self, now_ms: u64, seen: &Seen, menus: u64) -> Vec<Act> {
        if self.menus != menus {
            return Vec::new();
        }
        self.menu_opened(now_ms, seen)
    }

    // Nothing browses behind a shut menu; the devices found stay listed until the radio goes down.
    pub fn menu_closed(&mut self) -> Vec<Act> {
        self.menu_open = false;
        self.menus += 1;
        if !std::mem::take(&mut self.browsing) {
            return Vec::new();
        }
        self.next_gen(Proc::Browse);
        vec![Act::Stop(Proc::Browse)]
    }

    // Receiving On asked by hand: turn_on runs once its look off the window's thread is back.
    pub(super) fn ask_on(&mut self, folder: &str) {
        self.asks += 1;
        self.on_wait = Some(folder.to_string());
    }

    // Receiving set to Off, the plugin switched off or a run that exited: every run stops and Wi-Fi comes back.
    pub fn turn_off(&mut self) -> Vec<Act> {
        self.again = None;
        self.asks += 1;
        // An On still waiting for its look has withdrawn the last stop asked, so this Off asks it again.
        let waited = self.on_wait.take().is_some();
        if self.radio == Radio::Off {
            return self.stop_elsewhere(waited);
        }
        self.radio = Radio::Off;
        self.hold_until_exit();
        self.browsing = false;
        self.devices.clear();
        for proc in Proc::ALL {
            self.next_gen(proc);
        }
        vec![
            Act::Stop(Proc::Radio),
            Act::Stop(Proc::Receive),
            Act::Stop(Proc::Browse),
            Act::Refresh,
        ]
    }

    pub fn line(
        &mut self,
        proc: Proc,
        generation: u64,
        text: &str,
        now_ms: u64,
        i18n: &I18n,
    ) -> Vec<Act> {
        if generation != self.gens[slot(proc)] {
            return Vec::new();
        }
        match proc {
            Proc::Radio | Proc::Receive if text.starts_with("owly: ") => {
                self.said.push_str(text);
                self.said.push('\n');
                Vec::new()
            }
            Proc::Radio if self.radio == Radio::Starting && owly::radio_is_up(text) => {
                self.radio = Radio::On;
                self.shared.publish(Radio::On, &self.adapter.card);
                let generation = self.next_gen(Proc::Receive);
                let mut acts = vec![Act::Start(
                    Proc::Receive,
                    generation,
                    owly::receive_argv(&self.owly, &self.folder),
                )];
                acts.extend(self.browse(now_ms));
                acts.push(Act::Refresh);
                acts
            }
            Proc::Receive => self.received(text, now_ms, i18n),
            Proc::Browse => match owly::browse_line(text) {
                Some(peer) if !self.devices.iter().any(|d| d.id == peer.id) => {
                    self.devices.push(peer);
                    vec![Act::Refresh]
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    pub fn ended(
        &mut self,
        proc: Proc,
        generation: u64,
        exit_code: i32,
        now_ms: u64,
        i18n: &I18n,
    ) -> Vec<Act> {
        if proc == Proc::Radio && self.stopping == Some(generation) {
            return self.let_go(i18n);
        }
        if generation != self.gens[slot(proc)] {
            return Vec::new();
        }
        match proc {
            // The radio never came up or went down early; only its lease ending on time is quiet.
            Proc::Radio => {
                let expired = self.said.lines().any(|line| line == owly::RADIO_EXPIRED);
                let failed = self.radio == Radio::Starting || (exit_code != 0 && !expired);
                self.stopped(proc, failed, i18n)
            }
            // Nothing receives once the receiver exits, so the radio goes down too; only a failure says a sentence.
            Proc::Receive => self.stopped(proc, exit_code != owly::TIMEOUT_EXPIRED, i18n),
            // A browse that ran its seconds listens again; one that failed or ended early waits for a later menu open.
            Proc::Browse => {
                self.browsing = false;
                let whole = now_ms.saturating_sub(self.browse_at_ms) >= owly::BROWSE_SECS * 1_000;
                if exit_code == 0 && whole {
                    self.browse(now_ms)
                } else {
                    self.browse_failed_ms = Some(now_ms);
                    Vec::new()
                }
            }
        }
    }

    // A long run that exited takes AirDrop down with it, and a failure says owly's reason.
    fn stopped(&mut self, exited: Proc, failed: bool, i18n: &I18n) -> Vec<Act> {
        let mut acts = self.turn_off();
        acts.retain(|act| *act != Act::Stop(exited));
        // The radio's own exit leaves nothing to wait for.
        if exited == Proc::Radio {
            acts.extend(self.let_go(i18n));
        }
        if failed {
            acts.insert(0, Act::Say(owly::radio_failed(i18n, &self.said), true));
        }
        acts
    }

    // A device leaf chosen: the backend runs the send as a job; nothing goes out while the radio is down.
    pub fn send(&mut self, wire: &dyn Wire, peer: &str, paths: &[String], i18n: &I18n) -> Vec<Act> {
        let reachable: &[owly::Peer] = match self.machine() {
            Radio::On => &self.devices,
            _ => &[],
        };
        (self.outbox).send(reachable, wire, peer, paths, i18n)
    }

    // One backend line; only this window's own sends answer.
    pub fn backend_line(&mut self, text: &str, i18n: &I18n) -> Vec<Act> {
        self.outbox.answered(text, i18n)
    }

    // The flyout's devices in the order they answered, each with its phone mark.
    pub fn devices(&self) -> Vec<(Peer, bool)> {
        super::airdrop_outbox::listed(&self.devices)
    }

    pub fn page(&self, folder: &str, shown_as: &str) -> Page {
        Page {
            receiving: self.on() || self.again.is_some(),
            folder: folder.into(),
            shown_as: shown_as.into(),
            adapter: self.adapter.clone(),
            ..Page::default()
        }
    }
}

#[path = "airdrop_machine.rs"]
mod machine;

#[cfg(test)]
#[path = "airdrop_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "airdrop_machine_tests.rs"]
mod machine_tests;
