//! One window's AirDrop when Omdrop runs it in place of owly. Omdrop owns its radio, receiver, audience and folder for
//! the whole machine, so the window reads `omdrop status`, asks it to turn on or off, looks for devices and sends.
use super::airdrop::{Act, Proc};
use super::airdrop_outbox::Outbox;
use super::seams::Wire;
use flea::i18n::I18n;
use flea::ui_logic_context_menus::build::Peer;
use flea::ui_logic_plugin_settings::airdrop::Page;
use flea::ui_logic_send_share::airdrop::{self as owly, Adapter, Readiness};
use flea::ui_logic_send_share::omdrop;

// A device lookup that ends sooner than this is not run again until a later menu open.
const LOOK_MIN_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Off,
    // `omdrop on` runs.
    Starting,
    // The radio is up: devices can be found and sent to.
    On,
    // `omdrop off` runs; Omdrop is still on until it returns.
    Stopping,
    // `omdrop on` or `off` returned and a fresh status will say what Omdrop did; its exit code does not.
    Confirming,
}

#[derive(Default)]
pub struct Omdrop {
    // omdrop's path, empty unless it runs AirDrop for this window.
    path: String,
    pub adapter: Adapter,
    // Omdrop's last status: its receiver, audience, name and folder, which the page shows apart from the radio.
    status: omdrop::Status,
    status_gen: u64,
    // The status generation whose answer is awaited, if one is.
    status_wait: Option<u64>,
    pub phase: Phase,
    gens: [u64; Proc::ALL.len()],
    // Omdrop's own sentences from `on` and `off`, for the one said when either fails.
    said: String,
    browsing: bool,
    look_at_ms: u64,
    menu_open: bool,
    // The plugin is switched off: no lookup starts, whatever answer comes in, until it is switched on.
    disabled: bool,
    devices: Vec<owly::Peer>,
}

impl Omdrop {
    pub fn active(&self) -> bool {
        !self.path.is_empty()
    }

    // The backend's facts or PATH name omdrop, or nothing when owly runs AirDrop or omdrop is missing.
    pub fn facts(&mut self, path: &str) -> Vec<Act> {
        if self.path == path {
            return Vec::new();
        }
        self.path = path.to_string();
        self.adapter = Adapter {
            readiness: if path.is_empty() {
                Readiness::Missing
            } else {
                Readiness::Checking
            },
            card: String::new(),
        };
        self.forget_status();
        self.status = omdrop::Status::default();
        // Nothing a run of the omdrop just replaced says lands: an on or off finishes by itself, a lookup is stopped.
        self.phase = Phase::Off;
        self.next_gen(Proc::Radio);
        self.drop_devices()
    }

    fn next_gen(&mut self, proc: Proc) -> u64 {
        self.gens[proc as usize] += 1;
        self.gens[proc as usize]
    }

    fn busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Starting | Phase::Stopping | Phase::Confirming
        )
    }

    // Omdrop's state can change behind Flea, so its status is asked on every open, but not while on or off runs.
    pub fn ask_status(&mut self) -> Vec<Act> {
        let running = matches!(self.phase, Phase::Starting | Phase::Stopping);
        if !self.active() || self.status_wait.is_some() || running {
            return Vec::new();
        }
        self.status_gen += 1;
        self.status_wait = Some(self.status_gen);
        vec![Act::Doctor(
            self.status_gen,
            omdrop::status_argv(&self.path),
        )]
    }

    // A status asked before an `omdrop on` or `off` describes Omdrop before it, so its answer must not land.
    fn forget_status(&mut self) {
        self.status_gen += 1;
        self.status_wait = None;
    }

    // Omdrop's status sets the radio and the receiver as Omdrop has them, unless Flea's `omdrop on` or `off` still runs.
    pub fn status_done(
        &mut self,
        generation: u64,
        exit_code: i32,
        stdout: &str,
        now_ms: u64,
    ) -> Vec<Act> {
        if self.status_wait != Some(generation) {
            return Vec::new();
        }
        self.status_wait = None;
        let status = omdrop::status(stdout, exit_code);
        self.adapter = status.adapter.clone();
        let on = status.on;
        self.status = status;
        let mut acts = match (self.phase, on) {
            (Phase::Starting | Phase::Stopping, _) | (Phase::On, true) | (Phase::Off, false) => {
                Vec::new()
            }
            (Phase::Off, true) => {
                self.phase = Phase::On;
                self.browse(now_ms)
            }
            // Flea's own on just came up: its first lookup starts even with the menu shut, so the next open has names.
            (Phase::Confirming, true) => {
                self.phase = Phase::On;
                self.look(now_ms)
            }
            (Phase::On | Phase::Confirming, false) => {
                self.phase = Phase::Off;
                self.drop_devices()
            }
        };
        acts.push(Act::Refresh);
        acts
    }

    fn browse(&mut self, now_ms: u64) -> Vec<Act> {
        if !self.menu_open {
            return Vec::new();
        }
        self.look(now_ms)
    }

    // One device lookup, whether or not a menu is open; browse() asks for one only while it is.
    fn look(&mut self, now_ms: u64) -> Vec<Act> {
        if self.browsing || self.phase != Phase::On || self.disabled || !self.active() {
            return Vec::new();
        }
        self.browsing = true;
        self.look_at_ms = now_ms;
        let generation = self.next_gen(Proc::Browse);
        vec![Act::Start(
            Proc::Browse,
            generation,
            omdrop::peers_argv(&self.path),
        )]
    }

    pub fn menu_opened(&mut self, now_ms: u64) -> Vec<Act> {
        self.menu_open = true;
        let mut acts = self.ask_status();
        acts.extend(self.browse(now_ms));
        acts
    }

    // A lookup takes half a minute to name anyone, so a running one finishes behind a shut menu and is not restarted.
    pub fn menu_closed(&mut self) {
        self.menu_open = false;
    }

    fn drop_devices(&mut self) -> Vec<Act> {
        self.browsing = false;
        self.devices.clear();
        self.next_gen(Proc::Browse);
        vec![Act::Stop(Proc::Browse), Act::Refresh]
    }

    // The plugin switched off: Flea stops looking for good and Omdrop is left as the user set it; an on or off still finishes.
    pub fn disable(&mut self) -> Vec<Act> {
        if std::mem::replace(&mut self.disabled, true) {
            return Vec::new();
        }
        self.drop_devices()
    }

    pub fn enable(&mut self) {
        self.disabled = false;
    }

    // Turn On AirDrop, or Receiving set on: `omdrop on` unless the receiver already runs, even with the radio up.
    pub fn turn_on(&mut self, i18n: &I18n) -> Vec<Act> {
        if self.busy() || (self.phase == Phase::On && self.status.receiving) {
            return Vec::new();
        }
        if !self.adapter.ready() {
            return vec![
                Act::Say(i18n.tr("send.providers.menu_stale"), true),
                Act::Refresh,
            ];
        }
        self.forget_status();
        self.phase = Phase::Starting;
        self.said.clear();
        let generation = self.next_gen(Proc::Radio);
        vec![
            Act::Start(Proc::Radio, generation, omdrop::on_argv(&self.path)),
            Act::Refresh,
        ]
    }

    // Receiving set to Off: a receiver can run with the radio down, so either counts, whatever Omdrop's readiness.
    pub fn receiving_off(&mut self) -> Vec<Act> {
        if self.busy() || !(self.phase == Phase::On || self.status.receiving) {
            return Vec::new();
        }
        let mut acts = self.drop_devices();
        self.forget_status();
        self.phase = Phase::Stopping;
        self.said.clear();
        let generation = self.next_gen(Proc::Radio);
        acts.push(Act::Start(
            Proc::Radio,
            generation,
            omdrop::off_argv(&self.path),
        ));
        acts
    }

    pub fn line(&mut self, proc: Proc, generation: u64, text: &str) -> Vec<Act> {
        if generation != self.gens[proc as usize] {
            return Vec::new();
        }
        match proc {
            // `omdrop on` and `off` say why they failed in their own sentences, on stderr.
            Proc::Radio => {
                self.said.push_str(text);
                self.said.push('\n');
                Vec::new()
            }
            Proc::Browse => match omdrop::peer_line(text) {
                Some(peer) if !self.devices.iter().any(|d| d.id == peer.id) => {
                    self.devices.push(peer);
                    vec![Act::Refresh]
                }
                _ => Vec::new(),
            },
            Proc::Receive => Vec::new(),
        }
    }

    pub fn ended(
        &mut self,
        proc: Proc,
        generation: u64,
        code: i32,
        at_ms: u64,
        i18n: &I18n,
    ) -> Vec<Act> {
        if generation != self.gens[proc as usize] {
            return Vec::new();
        }
        match proc {
            Proc::Radio => self.on_or_off_ended(code, i18n),
            // A lookup that ran its course looks again for an open menu; one that ended early waits for a later open.
            Proc::Browse => {
                self.browsing = false;
                let whole = at_ms.saturating_sub(self.look_at_ms) >= LOOK_MIN_MS;
                if code == 0 && whole {
                    self.browse(at_ms)
                } else {
                    Vec::new()
                }
            }
            Proc::Receive => Vec::new(),
        }
    }

    // Omdrop can exit 0 having stopped a start part way, so only a fresh status says what is on; a failure says why.
    fn on_or_off_ended(&mut self, exit_code: i32, i18n: &I18n) -> Vec<Act> {
        if !matches!(self.phase, Phase::Starting | Phase::Stopping) {
            return Vec::new();
        }
        self.phase = Phase::Confirming;
        let mut acts = Vec::new();
        if exit_code != 0 {
            let said = match omdrop::reason(&self.said).as_str() {
                "" => i18n.tr("send.providers.airdrop_radio_failed"),
                why => i18n.tr_args("send.providers.airdrop_reason", &[why]),
            };
            acts.push(Act::Say(said, true));
        }
        acts.extend(self.ask_status());
        acts.push(Act::Refresh);
        acts
    }

    // A device leaf chosen: the backend runs the send as a job under the window's one numbering; nothing goes out while the radio is down.
    pub fn send(
        &self,
        outbox: &mut Outbox,
        wire: &dyn Wire,
        peer: &str,
        paths: &[String],
        i18n: &I18n,
    ) -> Vec<Act> {
        let reachable: &[owly::Peer] = match self.phase {
            Phase::On => &self.devices,
            _ => &[],
        };
        outbox.send(reachable, wire, peer, paths, i18n)
    }

    // The flyout shows Turn On AirDrop only while Omdrop is off; an on or off still running looks for devices.
    pub fn on(&self) -> bool {
        self.phase != Phase::Off
    }

    pub fn devices(&self) -> Vec<(Peer, bool)> {
        super::airdrop_outbox::listed(&self.devices)
    }

    // The page shows Omdrop's receiver, audience, name and folder; an on or off not yet confirmed shows as on.
    pub fn page(&self, folder: &str, shown_as: &str) -> Page {
        let own = |mine: &str, flea: &str| if mine.is_empty() { flea } else { mine }.to_string();
        Page {
            receiving: self.busy() || self.status.receiving,
            folder: own(&self.status.dir, folder),
            shown_as: own(&self.status.name, shown_as),
            adapter: self.adapter.clone(),
            omdrop: true,
            contacts: self.status.contacts,
        }
    }

    // Save received files to: Omdrop keeps its folder itself, so the choice becomes `omdrop dir`.
    pub fn folder_argv(&mut self, folder: &str) -> Vec<String> {
        self.status.dir = folder.to_string();
        omdrop::dir_argv(&self.path, folder)
    }
}

#[cfg(test)]
#[path = "airdrop_omdrop_tests.rs"]
mod tests;
