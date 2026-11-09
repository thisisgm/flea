//! The window-long device source: complete lsblk and gio answers feed the existing rail tail.
use super::devices::{Devices, Effect, Proc};
use super::listing::Listing;
use super::phones::Phones;
use super::process::{Runner, Slot};
use crate::pane::swap::Clock;
use flea::jsondoc::Json;
use flea::ui_logic_device_rail::{mounts::rail_groups_ready, Kind, RailEntry};
use flea::ui_logic_rail_places::{
    order::{Heading, RailSection},
    rail as places,
};
use gpui::{Context, Task};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[path = "source_jobs.rs"]
mod jobs;
#[path = "source_release.rs"]
mod release;
#[path = "source_seed.rs"]
pub mod seed;

#[cfg(test)]
#[path = "source_fixture.rs"]
pub(crate) mod fixture;

pub const SETTLE_MS: u64 = 800;

/// U20 supplies a complete network section and its discovery gate after its branch is joined.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkSnapshot {
    pub rows: Vec<places::RailEntry>,
    pub answered: bool,
}

pub struct Request {
    pub side: usize,
    pub event: Event,
}

pub enum Event {
    Device(Effect),
}

/// Whether the source acted on a rail activation, hands a phone to the GVFS opener, or the rail request owner keeps it.
#[derive(Clone, Debug, PartialEq)]
pub enum Activation {
    Handled,
    Phone(Box<RailEntry>),
    Unowned,
}

/// The original pl1 rows retain eject, unlock and size metadata that pl4's row type cannot carry.
pub struct Source {
    pub devices: Devices,
    pub phones: Phones,
    pub sections: Vec<RailSection>,
    listing: Listing,
    listing_generation: u64,
    network: NetworkSnapshot,
    show_devices: bool,
    show_network: bool,
    hostname_ready: bool,
    started: bool,
    active: bool,
    epoch: Clock,
    runner: Rc<dyn Runner>,
    slots: [Slot; jobs::SLOTS],
    readers: Vec<Task<()>>,
    timer: Option<Task<()>>,
    mount_side: usize,
    /// The effects after a Quiesce, held until the window has quiesced the releasing pane.
    after_quiesce: Vec<Effect>,
    quiesce_side: usize,
    /// Ctrl+E with the rail hidden waits for one listing: folder, system flag and side.
    hidden_release: Option<(String, bool, usize)>,
    events: RefCell<Vec<Request>>,
}

impl Source {
    /// A supplied hostname is the deterministic service seam; production reads /etc/hostname off thread.
    pub fn new(runner: Rc<dyn Runner>, hostname: Option<String>, state: &Json) -> Self {
        let mut source = Self {
            devices: Devices::new(&hostname_of(hostname.as_deref().unwrap_or(""))),
            phones: Phones::default(),
            sections: Vec::new(),
            listing: Listing::new(),
            listing_generation: 0,
            // No NETWORK producer is joined yet, so its empty section has already finished discovery.
            network: NetworkSnapshot {
                answered: true,
                ..NetworkSnapshot::default()
            },
            show_devices: true,
            show_network: true,
            hostname_ready: hostname.is_some(),
            started: false,
            active: false,
            epoch: Clock::default(),
            runner,
            slots: Default::default(),
            readers: Vec::new(),
            timer: None,
            mount_side: 0,
            after_quiesce: Vec::new(),
            quiesce_side: 0,
            hidden_release: None,
            events: RefCell::default(),
        };
        source.settings(state);
        source
    }

    fn now(&self, cx: &gpui::App) -> u64 {
        self.epoch.elapsed(cx.background_executor().now())
    }

    fn settings(&mut self, state: &Json) {
        let places = state.get("places");
        let shown = |key| places.and_then(|p| p.get(key)).and_then(Json::as_bool) != Some(false);
        self.show_devices = shown("showDevices");
        self.show_network = shown("showNetwork");
        self.devices.show_unmounted = shown("showUnmounted");
    }

    pub fn adopt_state(&mut self, state: &Json, cx: &mut Context<Self>) {
        self.settings(state);
        if !self.started {
            return;
        }
        let effects = self.devices.rebuild();
        self.apply(effects, cx);
    }

    /// The rail's host owns this gate; a reveal polls immediately and a withdrawal keeps its last rows.
    pub fn set_polling(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active && self.started {
            return;
        }
        self.active = active;
        self.devices.polling = active;
        self.listing.active = active;
        if !active && !self.started {
            return;
        }
        if !self.started {
            self.started = true;
            self.epoch = Clock::default();
            self.now(cx);
            if !self.hostname_ready {
                let read = cx.background_executor().spawn(async {
                    hostname_of(&std::fs::read_to_string("/etc/hostname").unwrap_or_default())
                });
                self.readers.push(cx.spawn(async move |this, cx| {
                    let hostname = read.await;
                    let _ = this.update(cx, |source, cx| {
                        source.devices.hostname = hostname;
                        source.hostname_ready = true;
                        let effects = source.devices.rebuild();
                        source.apply(effects, cx);
                    });
                }));
            }
        }
        self.advance(cx);
    }

    /// The launch's own lsblk answer, so DEVICES draws in the window's first frame (SW-07); the poll still lists.
    pub fn seed(&mut self, listing: String) {
        let _ = self.devices.list_exited(&super::process::Outcome {
            stdout: listing,
            ..super::process::Outcome::default()
        });
    }

    /// The unlock card's PollDevices: list now, so the unlocked drive's row arrives without waiting on the poll.
    pub fn poll_now(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let effects = self.devices.poll(now);
        self.apply(effects, cx);
        self.arm(cx);
    }

    /// NETWORK stays ahead of DEVICES, including when its source finishes after the devices did.
    pub fn set_network(&mut self, snapshot: NetworkSnapshot, cx: &mut Context<Self>) {
        self.network = snapshot;
        self.publish(cx);
    }

    pub fn drain_events(&self) -> Vec<Request> {
        std::mem::take(&mut *self.events.borrow_mut())
    }

    /// PickerPlaces shares one GIO poll between its network rows and phones.
    pub fn mount_listing(&self) -> &str {
        &self.listing.text
    }

    /// PickerPlaces always offers devices, independently of the main window's section toggles.
    pub fn picker_sections(&self) -> Vec<RailSection> {
        vec![RailSection {
            heading: Heading::Key("settings.places.devices"),
            rows: self
                .devices
                .entries
                .iter()
                .chain(&self.phones.entries)
                .map(rail_row)
                .collect(),
        }]
    }

    pub fn poll_mounts(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let effects = self.listing.poll(now);
        self.listing_effects(effects, cx);
        self.arm(cx);
    }

    pub(crate) fn mount_snapshot(&self) -> (u64, Listing) {
        (self.listing_generation, self.listing.clone())
    }

    /// Resolve the current row by identity; a vanished device never activates the next row instead.
    pub fn activate(
        &mut self,
        row: &places::RailEntry,
        side: usize,
        cx: &mut Context<Self>,
    ) -> Activation {
        if row.group != places::Group::Device {
            return Activation::Unowned;
        }
        let index = self
            .devices
            .entries
            .iter()
            .position(|entry| entry.device == row.device && !entry.device.is_empty());
        if let Some(index) = index {
            let entry = &self.devices.entries[index];
            // A locked row raises the typed unlock request the window's card answers.
            if !entry.locked && !entry.mounted && !self.devices.running(Proc::Mount) {
                self.mount_side = side;
            }
            let effects = self.devices.activate(index);
            self.apply_at(effects, side, cx);
            return Activation::Handled;
        }
        // A phone mounts, resolves and opens the way a share does (ui/Sidebar.qml:385).
        if let Some(phone) = self
            .phones
            .entries
            .iter()
            .find(|entry| entry.uri == row.uri)
        {
            return Activation::Phone(Box::new(phone.clone()));
        }
        Activation::Handled
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        let (network, devices) = rail_groups_ready(
            self.network.answered,
            // Phones join the end of DEVICES when gio answers; the drives never wait on its slower listing.
            self.devices.first_answered && self.hostname_ready,
            self.now(cx),
            SETTLE_MS,
        );
        let mut sections = Vec::new();
        if network && self.show_network && !self.network.rows.is_empty() {
            sections.push(RailSection {
                heading: Heading::Key("settings.places.network"),
                rows: self.network.rows.clone(),
            });
        }
        if devices && self.show_devices {
            let rows = self
                .devices
                .entries
                .iter()
                .chain(&self.phones.entries)
                .map(rail_row)
                .collect::<Vec<_>>();
            if !rows.is_empty() {
                sections.push(RailSection {
                    heading: Heading::Key("settings.places.devices"),
                    rows,
                });
            }
        }
        if sections != self.sections {
            self.sections = sections;
            cx.notify();
        }
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let effects = self.devices.tick(now);
        self.apply(effects, cx);
        let effects = self.listing.tick(now);
        self.listing_effects(effects, cx);
        self.publish(cx);
        self.arm(cx);
    }

    fn arm(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let gate = (now < SETTLE_MS).then_some(SETTLE_MS);
        let due = self
            .devices
            .deadline()
            .into_iter()
            .chain(gate)
            .chain(self.listing.due(now))
            .min();
        self.timer = due.map(|due| {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(due.saturating_sub(now).max(1)))
                    .await;
                let _ = this.update(cx, |source, cx| source.advance(cx));
            })
        });
    }
}

/// Both hostname sources pass here, so the machine row never carries the file's newline.
pub(super) fn hostname_of(text: &str) -> String {
    text.trim().to_owned()
}

fn rail_row(entry: &RailEntry) -> places::RailEntry {
    places::RailEntry {
        group: places::Group::Device,
        kind: match entry.kind {
            Kind::Disk => places::Kind::Disk,
            Kind::Phone => places::Kind::Phone,
            _ => places::Kind::Volume,
        },
        label: entry.label.clone(),
        path: entry.path.clone(),
        uri: entry.uri.clone(),
        device: entry.device.clone(),
        mounted: Some(entry.mounted),
        releasable: flea::ui_logic_device_rail::eject::releasable(entry),
        size: entry.size,
        glyph: match entry.glyph.as_str() {
            "lock" => "lock",
            "smartphone" => "smartphone",
            "camera" => "camera",
            _ => "drive",
        },
        ..places::RailEntry::default()
    }
}

/// Existing window unit fixtures have no hardware; device controls inject their own recording runner.
#[cfg(test)]
pub(crate) struct EmptyRunner;

#[cfg(test)]
impl Runner for EmptyRunner {
    fn start(&self, job: super::process::Job) -> super::process::Handle {
        use super::process::{Event, Handle, Outcome};
        let (handle, sender) = Handle::detached();
        let stdout = if job.argv.first().is_some_and(|program| program == "lsblk") {
            r#"{"blockdevices":[]}"#.into()
        } else {
            String::new()
        };
        let _ = sender.try_send(Event::Exited(Outcome {
            stdout,
            ..Outcome::default()
        }));
        handle
    }
}
