//! Releases a device row by identity: the eject mark, its keyboard frame and Ctrl+E all end here.
use super::*;
use crate::g2::device_rail::{devices::Moved, phones};
use flea::ui_logic_device_rail::{
    eject::{release, release_action, Release},
    menu::RailAction,
    mounts::{rail_key, row_by_key},
    Note,
};

impl Source {
    /// Producer order, as Sidebar.deviceEntries: block devices, then phones.
    pub fn rows(&self) -> Vec<RailEntry> {
        self.devices
            .entries
            .iter()
            .chain(&self.phones.entries)
            .cloned()
            .collect()
    }

    /// Whether the DEVICES section is on the rail, which the device readers answer from.
    pub fn devices_published(&self) -> bool {
        self.sections
            .iter()
            .any(|section| section.heading == Heading::Key("settings.places.devices"))
    }

    /// The drawn row resolved to the source's own entry: a drive by its device, a phone by its URI.
    fn owned(&self, row: &places::RailEntry) -> Option<RailEntry> {
        if row.group != places::Group::Device {
            return None;
        }
        self.devices
            .entries
            .iter()
            .chain(&self.phones.entries)
            .find(|entry| match entry.kind {
                Kind::Phone => !entry.uri.is_empty() && entry.uri == row.uri,
                _ => !entry.device.is_empty() && entry.device == row.device,
            })
            .cloned()
    }

    /// The drawn device row's own entry, which its rail menu reads its rows from.
    pub fn device_row(&self, row: &places::RailEntry) -> Option<RailEntry> {
        self.owned(row)
    }

    /// A rail menu's Unmount, Eject or phone Unmount, against the row its key names now.
    pub fn release_by(
        &mut self,
        action: RailAction,
        key: &str,
        side: usize,
        cx: &mut Context<Self>,
    ) {
        self.run(action, key, side, cx);
    }

    /// One click on the eject mark, or Return on its keyboard frame, as Sidebar.ejectRow: the row's own release.
    pub fn release_row(&mut self, row: &places::RailEntry, side: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.owned(row) else {
            return;
        };
        if let Some(action) = release_action(&entry) {
            self.run(action, &rail_key(&entry), side, cx);
        }
    }

    /// Ctrl+E as ui/js/Eject.js release: the rail cursor row, or the volume holding the pane's folder.
    pub fn release(
        &mut self,
        directory: &str,
        from_rail: bool,
        cursor: Option<&places::RailEntry>,
        system: bool,
        side: usize,
        cx: &mut Context<Self>,
    ) {
        let verdict = match (from_rail, cursor) {
            (true, Some(row)) if row.group == places::Group::Device => match self.owned(row) {
                Some(entry) => release(
                    Some((std::slice::from_ref(&entry), Some(0))),
                    directory,
                    true,
                    system,
                    false,
                ),
                None => return,
            },
            // A share belongs to the network owner, and a place is answered by the window, which knows its shown name.
            (true, Some(_)) => return,
            (true, None) => release(Some((&[], None)), directory, true, system, false),
            (false, _) if !self.active => {
                // A hidden rail lists once and resolves against that answer, as ui/PaneRail.qml ejectHidden.
                self.hidden_release = Some((directory.into(), system, side));
                let now = self.now(cx);
                let effects = self.devices.poll(now);
                self.apply_at(effects, side, cx);
                return;
            }
            (false, _) => {
                let rows = self.rows();
                release(Some((&rows, None)), directory, false, system, false)
            }
        };
        self.verdict(verdict, side, cx);
    }

    /// The window has quiesced the releasing pane, so the held release child may start; `moved` go back if it is refused.
    pub fn quiesced(&mut self, volume: &str, moved: Vec<Moved>, cx: &mut Context<Self>) {
        let effects = std::mem::take(&mut self.after_quiesce);
        if !effects.is_empty() {
            self.devices.moved = (volume.into(), moved);
            self.apply_at(effects, self.quiesce_side, cx);
        }
    }

    /// The window refused the release before quiescing, as a copy on the volume still runs: its child never starts.
    pub fn refused(&mut self, note: Note, cx: &mut Context<Self>) {
        self.after_quiesce.clear();
        self.devices.abandon();
        let side = self.quiesce_side;
        self.say(note, side, cx);
        self.publish(cx);
        self.arm(cx);
    }

    /// The release guard state in one line, read by ipc's deviceEjectState.
    pub fn eject_state(&self) -> String {
        self.devices.eject_state()
    }

    pub(super) fn hidden_listed(&mut self, cx: &mut Context<Self>) {
        let Some((directory, system, side)) = self.hidden_release.take() else {
            return;
        };
        if self.devices.list_timed_out {
            self.say(Note::new("device.no_volume", &[]), side, cx);
            return;
        }
        let rows = self.devices.entries.clone();
        let verdict = release(Some((&rows, None)), &directory, false, system, false);
        self.verdict(verdict, side, cx);
    }

    fn verdict(&mut self, verdict: Release, side: usize, cx: &mut Context<Self>) {
        match verdict {
            Release::Chosen(action, key) => self.run(action, &key, side, cx),
            Release::Say(note) => self.say(note, side, cx),
            Release::Hidden => {}
        }
    }

    fn say(&mut self, note: Note, side: usize, cx: &mut Context<Self>) {
        self.events.borrow_mut().push(Request {
            side,
            event: Event::Device(Effect::Message(note, false)),
        });
        cx.notify();
    }

    /// RailMenu.release: the action runs against the row its key names now, never a cached index.
    fn run(&mut self, action: RailAction, key: &str, side: usize, cx: &mut Context<Self>) {
        if let Some(at) = row_by_key(&self.phones.entries, key) {
            if action == RailAction::UnmountPhone {
                let phone = &self.phones.entries[at];
                let (uri, label) = (phone.uri.clone(), phone.label.clone());
                let key = key.to_owned();
                self.events.borrow_mut().push(Request {
                    side,
                    event: Event::Device(Effect::ReleasePhone { key, uri, label }),
                });
                cx.notify();
            }
            return;
        }
        let Some(at) = row_by_key(&self.devices.entries, key) else {
            return;
        };
        let now = self.now(cx);
        let effects = match action {
            RailAction::Eject => self.devices.eject(at, now),
            RailAction::UnmountVolume => self.devices.unmount(at),
            _ => return,
        };
        self.apply_at(effects, side, cx);
    }

    /// No transfer of the window reads or writes the phone, so its unmount child starts.
    pub fn unmount_phone(&mut self, key: &str, side: usize, cx: &mut Context<Self>) {
        let effects = self.phones.release(key);
        self.phone_effects(effects, side, cx);
    }

    pub(super) fn phone_effects(
        &mut self,
        effects: Vec<phones::Effect>,
        side: usize,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                phones::Effect::Start(job) => self.start(jobs::Child::Phone, job, cx),
                phones::Effect::Message(note, error) => {
                    self.events.borrow_mut().push(Request {
                        side,
                        event: Event::Device(Effect::Message(note, error)),
                    });
                    cx.notify();
                }
                // A released phone leaves the rail on the next listing, read now rather than at the next poll.
                phones::Effect::Released => {
                    let now = self.now(cx);
                    let effects = self.listing.poll(now);
                    self.listing_effects(effects, cx);
                }
            }
        }
        self.publish(cx);
        self.arm(cx);
    }
}
