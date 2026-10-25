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
        // A folder inside a mounted share is the network owner's to release.
        if !from_rail
            && entries.iter().any(|row| {
                row.group == Group::Network
                    && row.mounted == Some(true)
                    && !row.path.is_empty()
                    && inside(&path, &row.path)
            })
        {
            return;
        }
        let source = join.devices.clone();
        source.update(cx, |source, cx| {
            source.release(&path, from_rail, cursor.as_ref(), system_volume, side, cx)
        });
    }

    /// The eject mark's click or its keyboard frame names the row; the pane that asked is the one that quiesces.
    pub(crate) fn release_rail_row(&mut self, side: usize, row: &RailEntry, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            let source = join.devices.clone();
            source.update(cx, |source, cx| source.release_row(row, side, cx));
        }
    }

    /// v0.3.12 quiesces and moves only the releasing pane (ui/Sidebar.qml:271, ui/PaneRail.qml:148,225), then the release runs.
    pub(crate) fn quiesce_volume(
        &mut self,
        side: usize,
        volume: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = side.min(1);
        let mut failed = Vec::new();
        let on_volume = self.panes[side].as_ref().is_some_and(|host| {
            if let Some(Err(error)) = host.backend.as_ref().map(|backend| backend.quiesce()) {
                failed.push(self.locale.catalog.tr_args(error.key, &[&error.detail]));
            }
            !volume.is_empty() && inside(&host.path, volume)
        });
        let home = std::env::var("HOME").unwrap_or_default();
        if on_volume && !home.is_empty() {
            if let Err(refusal) = self.step(side, super::navigate::Step::OpenPlace(home), window, cx)
            {
                failed.push(self.locale.catalog.tr(refusal.key()));
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
                // OpenPlace supersedes a busy pane, so it never refuses.
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
