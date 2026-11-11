//! The window side of a device release: Ctrl+E, the eject mark and the quiesce that comes before the release child.
use crate::backend::transfers::Kind;
use crate::g2::device_rail::devices::Moved;
use crate::state::Shell;
use flea::ui_logic_device_rail::{mounts::inside, Note};
use flea::ui_logic_rail_places::rail::{Group, RailEntry};
use gpui::{Context, Window};

impl Shell {
    /// Ctrl+E from the rail releases its cursor row; from a listing, the volume holding the folder.
    pub fn release_device(&mut self, system_volume: bool, cx: &mut Context<Self>) {
        let side = self.focus_side;
        let Some(path) = self.panes[side].as_ref().map(|host| host.path.clone()) else {
            return;
        };
        let system_volume = system_volume
            || (self.surfaces[side].filesystem.path == path
                && self.surfaces[side].filesystem.system);
        let Some(join) = &self.rail else {
            return;
        };
        let from_rail = self.keyboard.context == flea::keys::Context::Rail;
        let entries = join.places.read(cx).entries();
        let cursor = join
            .places
            .read(cx)
            .cursor()
            .and_then(|index| entries.get(index).cloned());
        // A share is the network owner's to release: the rail's cursor row, or the share holding the folder (Mounts.holding).
        let share = if from_rail {
            cursor.clone().filter(|row| row.group == Group::Network)
        } else {
            entries
                .iter()
                .find(|row| {
                    row.group == Group::Network && !row.path.is_empty() && inside(&path, &row.path)
                })
                .cloned()
        };
        if let Some(share) = share {
            self.release_share(side, &share, cx);
            return;
        }
        if let Some(place) = cursor
            .as_ref()
            .filter(|row| from_rail && row.group != Group::Device)
        {
            self.say_nothing_to_release(place, cx);
            return;
        }
        let source = join.devices.clone();
        source.update(cx, |source, cx| {
            source.release(&path, from_rail, cursor.as_ref(), system_volume, side, cx)
        });
    }

    /// A mounted share unmounts through the network owner, as the rail menu's Unmount does; any other share has nothing to release (ui/js/Eject.js:211-216).
    fn release_share(&mut self, side: usize, share: &RailEntry, cx: &mut Context<Self>) {
        let row = self
            .network_row(share)
            .filter(|row| row.mounted && row.kind == flea::ui_logic_device_rail::Kind::Share);
        if let Some(row) = row {
            let origin = self.network_origin(side);
            self.network_unmount(&row, origin, cx);
            return;
        }
        self.say_nothing_to_release(share, cx);
    }

    /// A rail row with nothing to release says so by the name the rail shows for it, a built-in place included.
    fn say_nothing_to_release(&mut self, row: &RailEntry, cx: &mut Context<Self>) {
        let label = crate::g2::rail_places::content::row_label(&self.locale.catalog, row);
        self.say_device_note(Note::new("device.nothing_to_release", &[&label]), cx);
    }

    fn say_device_note(&mut self, note: Note, cx: &mut Context<Self>) {
        let text = crate::g2::device_rail::words::say(&self.locale.catalog, &note);
        let hint = flea::ui_logic_status_feedback::status::Hint::None;
        self.status_messages(cx, |messages, now| {
            messages.say(&text, false, "", hint, now)
        });
    }

    /// A transfer of this window on the share refuses its unmount, as a drive's eject does; otherwise the row's own identity is released.
    pub(crate) fn network_unmount(
        &mut self,
        row: &flea::ui_logic_device_rail::RailEntry,
        origin: u64,
        cx: &mut Context<Self>,
    ) {
        let held = !row.path.is_empty();
        if let Some(kind) = held
            .then(|| self.transfer_where(&|path| inside(path, &row.path)))
            .flatten()
        {
            let label = if row.label.is_empty() {
                &row.path
            } else {
                &row.label
            };
            self.say_device_note(Note::new(kind.refusal(), &[label]), cx);
            return;
        }
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let listing = network.listing.text.clone();
        service.update(cx, |service, cx| service.unmount(row, &listing, origin, cx));
    }

    /// A phone unmounts only once no transfer of this window reads or writes its GVFS mount.
    pub(crate) fn release_phone(
        &mut self,
        side: usize,
        key: &str,
        uri: &str,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(kind) = self.transfer_where(&|path| on_phone(path, uri)) {
            self.say_device_note(Note::new(kind.refusal(), &[label]), cx);
            return;
        }
        if let Some(join) = &self.rail {
            let source = join.devices.clone();
            source.update(cx, |source, cx| source.unmount_phone(key, side, cx));
        }
    }

    /// The eject mark's click or its keyboard frame names the row to release.
    pub(crate) fn release_rail_row(
        &mut self,
        side: usize,
        row: &RailEntry,
        cx: &mut Context<Self>,
    ) {
        if let Some(join) = &self.rail {
            let source = join.devices.clone();
            source.update(cx, |source, cx| source.release_row(row, side, cx));
        }
    }

    /// Every pane on the volume releases its readers before the release child starts.
    pub(crate) fn quiesce_volume(
        &mut self,
        side: usize,
        volume: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = side.min(1);
        let source = self.rail.as_ref().map(|join| join.devices.clone());
        if let Some((label, kind)) = self.transfer_on(volume, cx) {
            let note = Note::new(kind.refusal(), &[&label]);
            if let Some(source) = source {
                source.update(cx, |source, cx| source.refused(note, cx));
            }
            return;
        }
        let mut failed = Vec::new();
        let mut moved = Vec::new();
        let home = std::env::var("HOME").unwrap_or_default();
        for pane_side in 0..self.panes.len() {
            let Some(host) = self.panes[pane_side].as_ref() else {
                continue;
            };
            let on_volume = !volume.is_empty()
                && (inside(&host.path, volume) || inside(host.pane.place(), volume));
            if pane_side != side && !on_volume {
                continue;
            }
            if let Some(Err(error)) = host.backend.as_ref().map(|backend| backend.quiesce()) {
                failed.push(self.locale.catalog.tr_args(error.key, &[&error.detail]));
            }
            if !on_volume {
                continue;
            }
            if let Some(preview) = host.preview.clone() {
                preview.update(cx, |preview, cx| preview.hide(window, cx));
            }
            if !home.is_empty() {
                let from = if inside(host.pane.place(), volume) {
                    host.pane.place().to_owned()
                } else {
                    host.path.clone()
                };
                match self.step(
                    pane_side,
                    super::navigate::Step::OpenPlace(home.clone()),
                    window,
                    cx,
                ) {
                    Ok(_) => moved.push(Moved {
                        side: pane_side,
                        from,
                        to: home.clone(),
                    }),
                    Err(refusal) => failed.push(self.locale.catalog.tr(refusal.key())),
                }
            }
        }
        // A failed quiesce or move is said, and the release still runs, as v0.3.12 never held it back.
        let hint = flea::ui_logic_status_feedback::status::Hint::None;
        for text in failed {
            self.status_messages(cx, |messages, now| messages.say(&text, true, "", hint, now));
        }
        if let Some(source) = source {
            source.update(cx, |source, cx| source.quiesced(volume, moved, cx));
        }
    }

    /// The shown name of a volume the release takes down that a transfer of this window reads or writes, and that transfer's kind, if any.
    fn transfer_on(&self, volume: &str, cx: &mut Context<Self>) -> Option<(String, Kind)> {
        let devices = &self.rail.as_ref()?.devices.read(cx).devices;
        let volumes = devices.release_volumes(volume);
        let (busy, kind) = volumes
            .iter()
            .filter(|volume| !volume.is_empty())
            .find_map(|volume| {
                Some((volume, self.transfer_where(&|path| inside(path, volume))?))
            })?;
        let label = devices
            .entries
            .iter()
            .find(|entry| entry.path == *busy)
            .map(|entry| entry.label.clone());
        Some((label.unwrap_or_else(|| busy.clone()), kind))
    }

    /// The kind of a transfer of this window, sent and not yet refused or ended, that names a path `on` accepts.
    fn transfer_where(&self, on: &dyn Fn(&str) -> bool) -> Option<Kind> {
        self.panes
            .iter()
            .flatten()
            .filter_map(|host| host.backend.as_ref())
            .find_map(|backend| backend.transfers_where(on))
    }

    /// A refused release puts each pane it moved back in the folder it left, unless the pane has been sent elsewhere since.
    pub(crate) fn restore_moved(
        &mut self,
        moved: &[Moved],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for pane in moved {
            let here = self
                .panes
                .get(pane.side)
                .and_then(Option::as_ref)
                .is_some_and(|host| host.pane.place() == pane.to);
            if here {
                let step = super::navigate::Step::OpenPlace(pane.from.clone());
                // OpenPlace supersedes a busy pane and reads no row, so it never refuses.
                let _ = self.step(pane.side, step, window, cx);
            }
        }
    }

    /// A phone row mounts, resolves and opens through the network owner's GVFS flight, as ui/Sidebar.qml openPhone.
    pub(crate) fn open_phone(
        &mut self,
        side: usize,
        phone: &flea::ui_logic_device_rail::RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_network(window, cx);
        let origin = self.network_origin(side);
        match crate::g2::network_shares::menus::activate(phone, origin) {
            crate::g2::network_shares::menus::Open::Direct { path, .. } => {
                // OpenPlace never refuses: it supersedes a busy pane (no Busy) and reads no row (no NotLoaded).
                let _ = self.step(side, super::navigate::Step::OpenPlace(path), window, cx);
            }
            crate::g2::network_shares::menus::Open::Remote(request) => {
                let Some(network) = self.network() else {
                    return;
                };
                let service = network.service.clone();
                service.update(cx, |service, cx| {
                    let now = service.now(cx);
                    service.open_share(request, now, cx)
                });
            }
        }
    }
}

/// Whether `path` lies in the GVFS FUSE mount of the phone at `uri`, as mtp://pixel/ mounts at /run/user/1000/gvfs/mtp:host=pixel.
fn on_phone(path: &str, uri: &str) -> bool {
    let Some((scheme, rest)) = uri.split_once("://") else {
        return false;
    };
    // An AFC service names its port after the host, and GVFS adds it as a parameter of its own.
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    let mount = path
        .strip_prefix("/run/user/")
        .and_then(|rest| rest.split_once('/'))
        .and_then(|(_, rest)| rest.strip_prefix("gvfs/"))
        .and_then(|rest| rest.split('/').next())
        .and_then(|mount| mount.strip_prefix(scheme)?.strip_prefix(':'));
    !host.is_empty()
        && mount.is_some_and(|params| {
            params
                .split(',')
                .any(|param| param.strip_prefix("host=") == Some(host))
        })
}

#[cfg(test)]
#[path = "devices_phone_tests.rs"]
mod phone_tests;
