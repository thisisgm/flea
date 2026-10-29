//! The window side of a device release: Ctrl+E, the eject mark and the quiesce that comes before the release child.
use crate::state::Shell;
use flea::ui_logic_pl1::mounts::inside;
use flea::ui_logic_pl4::rail::{Group, RailEntry};
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
        let source = join.devices.clone();
        source.update(cx, |source, cx| {
            source.release(&path, from_rail, cursor.as_ref(), system_volume, side, cx)
        });
    }

    /// A mounted share unmounts through the network owner, as the rail menu's Unmount does; any other share has nothing to release (ui/js/Eject.js:211-216).
    fn release_share(&mut self, side: usize, share: &RailEntry, cx: &mut Context<Self>) {
        let row = self
            .network_row(share)
            .filter(|row| row.mounted && row.kind == flea::ui_logic_pl1::Kind::Share);
        if let Some(row) = row {
            let origin = self.network_origin(side);
            self.network_unmount(&row, origin, cx);
            return;
        }
        let note = flea::ui_logic_pl1::Note::new("device.nothing_to_release", &[&share.label]);
        let text = crate::g2::pl1::words::say(&self.locale.catalog, &note);
        let hint = flea::ui_logic_fb1::status::Hint::None;
        self.status_messages(cx, |messages, now| {
            messages.say(&text, false, "", hint, now)
        });
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
        let mut failed = Vec::new();
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
                if let Err(refusal) = self.step(
                    pane_side,
                    super::navigate::Step::OpenPlace(home.clone()),
                    window,
                    cx,
                ) {
                    failed.push(self.locale.catalog.tr(refusal.key()));
                }
            }
        }
        // A failed quiesce or move is said, and the release still runs, as v0.3.12 never held it back.
        for text in failed {
            self.status_messages(cx, |messages, now| {
                messages.say(&text, true, "", flea::ui_logic_fb1::status::Hint::None, now)
            });
        }
        if let Some(join) = &self.rail {
            let source = join.devices.clone();
            source.update(cx, |source, cx| source.quiesced(cx));
        }
    }

    /// A phone row mounts, resolves and opens through the network owner's GVFS flight, as ui/Sidebar.qml openPhone.
    pub(crate) fn open_phone(
        &mut self,
        side: usize,
        phone: &flea::ui_logic_pl1::RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_network(window, cx);
        let origin = self.network_origin(side);
        match crate::g2::pl2::menus::activate(phone, origin) {
            crate::g2::pl2::menus::Open::Direct { path, .. } => {
                // OpenPlace never refuses: it supersedes a busy pane (no Busy) and reads no row (no NotLoaded).
                let _ = self.step(side, super::navigate::Step::OpenPlace(path), window, cx);
            }
            crate::g2::pl2::menus::Open::Remote(request) => {
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
