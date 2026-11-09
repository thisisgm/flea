//! The machine's AirDrop as one window sees it: its own radio or another window's, and what the receiver saved.
use super::{owly, Act, AirDrop, Proc, Radio, Seen, RECEIVED_GROUP_MS};
use flea::i18n::I18n;
use flea::ui_logic_send_share::airdrop::{Adapter, Readiness};
use flea::ui_logic_send_share::airdrop_ask::{self, Heard};

impl AirDrop {
    // The radio the machine has up: this window's own, else the one another window holds.
    pub(super) fn machine(&self) -> Radio {
        if self.radio != Radio::Off {
            self.radio
        } else {
            self.elsewhere
        }
    }

    pub fn on(&self) -> bool {
        self.machine() != Radio::Off
    }

    // The menu row shows while this window's doctor said ready or another window's radio is up.
    pub fn ready(&self) -> bool {
        self.adapter.ready() || self.elsewhere != Radio::Off
    }

    // Turn On AirDrop, or Receiving set to Everyone for 10 Minutes: the radio first, the receiver once awdl0 is up.
    pub fn turn_on(&mut self, folder: &str, i18n: &I18n, seen: &Seen) -> Vec<Act> {
        if self.radio != Radio::Off {
            return Vec::new();
        }
        // owly is still giving Wi-Fi back, and the radio is free once that run exits.
        if self.stopping.is_some() {
            return self.retry_later(folder, seen);
        }
        // The machine has one radio: one another window holds shows on here.
        let mut acts = self.look(seen);
        if self.elsewhere != Radio::Off {
            acts.push(Act::Refresh);
            return acts;
        }
        if self.owly.is_empty() || !self.adapter.ready() {
            acts.extend([
                Act::Say(i18n.tr("send.providers.menu_stale"), true),
                Act::Refresh,
            ]);
            return acts;
        }
        if !self.shared.claim(&self.adapter.card) {
            acts.extend(self.retry_later(folder, seen));
            return acts;
        }
        self.radio = Radio::Starting;
        self.folder = folder.to_string();
        self.said.clear();
        let generation = self.next_gen(Proc::Radio);
        acts.extend([
            Act::Start(Proc::Radio, generation, owly::radio_argv(&self.owly)),
            Act::Refresh,
        ]);
        acts
    }

    // Reads another window's radio; one that went down takes this window's browse and devices with it.
    pub fn look(&mut self, seen: &Seen) -> Vec<Act> {
        if self.radio != Radio::Off {
            return Vec::new();
        }
        let (radio, card) = (seen.radio, seen.card.clone());
        // The owner's doctor said ready before it took the radio, and doctor reads the lease while it is up.
        if radio != Radio::Off && !self.adapter.ready() {
            self.adapter = Adapter {
                readiness: Readiness::Ready,
                card,
            };
        }
        if std::mem::replace(&mut self.elsewhere, radio) == radio {
            return Vec::new();
        }
        let mut acts = Vec::new();
        if radio != Radio::On {
            acts.extend(self.quiet());
        }
        acts.push(Act::Refresh);
        acts
    }

    fn quiet(&mut self) -> Vec<Act> {
        self.devices.clear();
        if !std::mem::take(&mut self.browsing) {
            return Vec::new();
        }
        self.next_gen(Proc::Browse);
        vec![Act::Stop(Proc::Browse)]
    }

    // Receiving set to Off in a window that does not own the radio: the owner is asked, and stops it within a second.
    pub(super) fn stop_elsewhere(&mut self, waited: bool) -> Vec<Act> {
        if self.elsewhere == Radio::Off && !waited {
            return Vec::new();
        }
        self.elsewhere = Radio::Off;
        let mut acts = vec![Act::AskStop];
        acts.extend(self.quiet());
        acts.push(Act::Refresh);
        acts
    }

    // True once when a watch should start: the radio is up or waited for, and other windows can see or stop it.
    pub fn watch(&mut self) -> bool {
        if self.watching || !(self.on() || self.again.is_some()) || !self.shared.machine_wide() {
            return false;
        }
        self.watching = true;
        true
    }

    // Once a second: the owner answers a stop asked elsewhere, a reader follows the owner; false when nothing is left.
    pub fn tick(&mut self, now_ms: u64, i18n: &I18n, seen: &Seen) -> (Vec<Act>, bool) {
        if let Some(folder) = self.again.take() {
            return (self.turn_on(&folder, i18n, seen), true);
        }
        if self.radio != Radio::Off {
            let acts = if self.shared.stop_asked(seen) {
                self.turn_off()
            } else {
                Vec::new()
            };
            return (acts, true);
        }
        let mut acts = self.look(seen);
        // A reader's open menu lists devices once the owner's radio is up, unless its last browse failed.
        if self.browse_failed_ms.is_none() {
            acts.extend(self.browse(now_ms));
        }
        self.watching = self.elsewhere != Radio::Off;
        (acts, self.watching)
    }

    // The radio is going down: other windows read it Off at once, and the lock waits for the run's exit.
    pub(super) fn hold_until_exit(&mut self) {
        self.shared.withdraw();
        self.stopping = Some(self.gens[super::slot(Proc::Radio)]);
    }

    // The radio's run has exited, so the machine's radio is free, and a Receiving On asked meanwhile takes it.
    pub(super) fn let_go(&mut self, i18n: &I18n) -> Vec<Act> {
        self.stopping = None;
        self.shared.release();
        // This window held the lock until now, so no other window's radio was up to see.
        match self.again.take() {
            Some(folder) => self.turn_on(&folder, i18n, &Seen::default()),
            None => Vec::new(),
        }
    }

    // Receiving On while the machine's radio is still going down waits, and the watch tries again each second.
    pub(super) fn retry_later(&mut self, folder: &str, seen: &Seen) -> Vec<Act> {
        let mut acts = self.look(seen);
        if self.elsewhere == Radio::Off {
            self.again = Some(folder.to_string());
        }
        acts.push(Act::Refresh);
        acts
    }

    // One line the receiver printed after Accept: a saved file, a link (shown, never opened) or a failure.
    pub(super) fn received(&mut self, text: &str, now_ms: u64, i18n: &I18n) -> Vec<Act> {
        let Some(heard) = airdrop_ask::heard(text) else {
            return Vec::new();
        };
        let mut count = 1;
        if let Heard::File(_) = &heard {
            let earlier = self
                .received
                .take()
                .filter(|(at, _)| now_ms.saturating_sub(*at) < RECEIVED_GROUP_MS);
            count += earlier.map_or(0, |(_, n)| n);
            self.received = Some((now_ms, count));
        }
        let (said, error) = airdrop_ask::heard_said(i18n, &heard, count);
        vec![Act::Say(said, error)]
    }
}
