//! The unlock card's host: names a card opened on a bare device, and follows the unlocked drive in the rail.
use super::devices::{Devices, Effect as ListEffect, Proc as ListProc};
use super::process::{Job, Outcome};
use super::seams::RailView;
use super::unlock::{Card, Effect as CardEffect, Proc};
use flea::ui_logic_pl1::unlock::naming_row;
use flea::ui_logic_pl1::{Note, RailEntry};

/// What the host asks of the window around it.
#[derive(Debug)]
pub enum Effect {
    Card(CardEffect),
    OpenInPane(String),
    RailOpened(String),
    PollDevices,
    SetCursor(usize),
    Say(Note, bool),
    FocusPane,
    NameList(Job),
    /// The poller was dropped: kill its lsblk and never take its exit.
    NameKill,
    /// The 10 s bound: kill the hung lsblk, whose exit the poller still waits on to list again.
    NameBound,
}

#[derive(Debug, Default)]
pub struct Host {
    pub card: Option<Card>,
    pub pending_path: String,
    pub locked_cursor: Option<usize>,
    pub unnamed_device: String,
    name_poll_done: bool,
    poll_entries: Vec<RailEntry>,
    poller: Option<Devices>,
}

impl Host {
    pub fn opened(&self) -> bool {
        self.card.as_ref().is_some_and(|c| c.opened)
    }

    /// Opens the card over a locked row; the card is built on first use.
    pub fn open_for(
        &mut self,
        device: &str,
        label: &str,
        size: u64,
        fstype: &str,
        rail: Option<&RailView>,
    ) {
        self.locked_cursor = cursor_on_row(rail, device);
        let fsver = self
            .lookup_row(rail, device)
            .map(|r| r.fsver.clone())
            .unwrap_or_default();
        self.card
            .get_or_insert_with(Card::new)
            .open_for(device, label, size, fstype, &fsver);
    }

    /// flea --unlock before the rail's first listing: the card shows the device's leaf until a locked row names it.
    pub fn open_for_device(&mut self, device: &str, rail: Option<&RailView>) {
        let leaf = device.rsplit('/').next().unwrap_or(device).to_string();
        self.open_for(device, &leaf, 0, "LUKS", rail);
        self.name_poll_done = false;
        self.poll_entries.clear();
        self.unnamed_device = device.to_string();
        self.name_device(rail);
    }

    fn lookup_row<'a>(&'a self, rail: Option<&'a RailView>, device: &str) -> Option<&'a RailEntry> {
        naming_row(
            rail.map_or(&[][..], |r| &r.devices),
            &self.poll_entries,
            device,
        )
    }

    pub fn card_touched(&self) -> bool {
        self.card.as_ref().is_some_and(Card::touched)
    }

    /// Whether the one-shot name listing should run now.
    pub fn name_polling(&self) -> bool {
        !self.unnamed_device.is_empty()
            && self.opened()
            && !self.name_poll_done
            && !self.card_touched()
    }

    /// Runs the one-shot lsblk while the card waits on its name; a touched card drops it.
    pub fn name_tick(&mut self, now: u64) -> Vec<Effect> {
        if self.card_touched() && !self.unnamed_device.is_empty() {
            self.name_poll_done = true;
        }
        if !self.name_polling() {
            let running = self
                .poller
                .take()
                .is_some_and(|p| p.running(ListProc::List));
            return if running {
                vec![Effect::NameKill]
            } else {
                Vec::new()
            };
        }
        let poller = self.poller.get_or_insert_with(|| {
            let mut once = Devices::new("");
            once.polling = false;
            once
        });
        poller
            .tick(now)
            .into_iter()
            .filter_map(name_effect)
            .collect()
    }

    /// When the name poller next needs a tick.
    pub fn name_deadline(&self) -> Option<u64> {
        self.poller.as_ref().and_then(Devices::deadline)
    }

    /// Lists the poller has started, for the one-lsblk proofs.
    pub fn name_listings(&self) -> Option<u64> {
        self.poller.as_ref().map(|p| p.listings_started)
    }

    /// The poller's lsblk ended; a timed-out one lists again at once, a dropped poller's exit is ignored.
    pub fn name_exited(
        &mut self,
        outcome: &Outcome,
        now: u64,
        rail: Option<&RailView>,
    ) -> Vec<Effect> {
        let Some(poller) = self.poller.as_mut() else {
            return Vec::new();
        };
        poller.list_exited(outcome);
        let (entries, timed_out) = (poller.entries.clone(), poller.list_timed_out);
        if self.name_poll_answered(entries, timed_out, rail) {
            return self
                .poller
                .as_mut()
                .map(|p| p.poll(now))
                .unwrap_or_default()
                .into_iter()
                .filter_map(name_effect)
                .collect();
        }
        self.poller = None;
        Vec::new()
    }

    /// The one-shot listing answered; a timed-out one asks again, any other answer ends the wait.
    pub fn name_poll_answered(
        &mut self,
        entries: Vec<RailEntry>,
        timed_out: bool,
        rail: Option<&RailView>,
    ) -> bool {
        if timed_out {
            return true;
        }
        self.poll_entries = entries;
        self.name_poll_done = true;
        self.name_device(rail);
        false
    }

    /// Only an untouched card takes a name, so nothing the user started on changes the drive it names.
    pub fn name_device(&mut self, rail: Option<&RailView>) {
        if self.card_touched() {
            self.name_poll_done = true;
        }
        let Some(card) = self.card.as_ref() else {
            self.unnamed_device.clear();
            return;
        };
        if self.unnamed_device.is_empty()
            || !card.opened
            || card.device != self.unnamed_device
            || card.touched()
        {
            self.unnamed_device.clear();
            return;
        }
        let Some(row) = self.lookup_row(rail, &self.unnamed_device).cloned() else {
            if self.name_poll_done {
                self.unnamed_device.clear();
            }
            return;
        };
        self.unnamed_device.clear();
        if let Some(card) = self.card.as_mut() {
            card.describe(&row.label, row.size.unwrap_or(0), &row.fstype, &row.fsver);
        }
    }

    /// Lands the cursor on the unlocked row, matched by mountpoint; stashes the path for the next rebuild when asked.
    pub fn land_cursor(&mut self, rail: Option<&RailView>, stash: bool) -> Option<Effect> {
        let want = self.pending_path.clone();
        let Some(rail) = rail.filter(|_| !want.is_empty()) else {
            self.pending_path.clear();
            return None;
        };
        if let Some(i) = rail.devices.iter().position(|d| d.path == want) {
            self.pending_path.clear();
            return Some(Effect::SetCursor(rail.places + rail.network + i));
        }
        if !stash {
            self.pending_path.clear();
        }
        None
    }

    /// A rebuilt rail ends the follow when the cursor left the locked row, and lands it otherwise.
    pub fn rail_rebuilt(&mut self, rail: Option<&RailView>) -> Vec<Effect> {
        let mut out = Vec::new();
        if !self.pending_path.is_empty() {
            match rail {
                Some(r) if r.cursor.is_none() || r.cursor == self.locked_cursor => {
                    out.extend(self.land_cursor(rail, false))
                }
                _ => self.pending_path.clear(),
            }
        }
        self.name_device(rail);
        out
    }

    /// Routes the card's own effects, turning its endings into the window's.
    pub fn card_effects(
        &mut self,
        effects: Vec<CardEffect>,
        rail: Option<&RailView>,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                CardEffect::Closed => out.push(Effect::FocusPane),
                CardEffect::LockFailed(note) => out.push(Effect::Say(note, true)),
                CardEffect::Unlocked { path, note, .. } => {
                    if rail.is_none() {
                        out.push(Effect::OpenInPane(path));
                    } else {
                        out.push(Effect::RailOpened(path.clone()));
                        out.push(Effect::PollDevices);
                        self.pending_path = path;
                        out.extend(self.land_cursor(rail, true));
                    }
                    out.push(Effect::Say(note, false));
                }
                other => out.push(Effect::Card(other)),
            }
        }
        out
    }

    pub fn exited(
        &mut self,
        proc: Proc,
        outcome: &Outcome,
        now: u64,
        rail: Option<&RailView>,
    ) -> Vec<Effect> {
        let effects = self
            .card
            .as_mut()
            .map(|c| c.exited(proc, outcome, now))
            .unwrap_or_default();
        self.card_effects(effects, rail)
    }
}

fn name_effect(effect: ListEffect) -> Option<Effect> {
    match effect {
        ListEffect::Start(ListProc::List, job) => Some(Effect::NameList(job)),
        ListEffect::Kill(ListProc::List) => Some(Effect::NameBound),
        _ => None,
    }
}

/// The rail index of the cursor when it sits on this device's own row.
fn cursor_on_row(rail: Option<&RailView>, device: &str) -> Option<usize> {
    let rail = rail?;
    let cursor = rail.cursor?;
    let row = rail
        .devices
        .get(cursor.checked_sub(rail.places + rail.network)?)?;
    (row.device == device).then_some(cursor)
}
