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
        let on_volume = self.panes[side].as_ref().is_some_and(|host| {
            if let Some(backend) = &host.backend {
                let _ = backend.quiesce();
            }
            !volume.is_empty() && inside(&host.path, volume)
        });
        let home = std::env::var("HOME").unwrap_or_default();
        if on_volume && !home.is_empty() {
            let _ = self.step(side, super::navigate::Step::OpenPlace(home), window, cx);
        }
        if let Some(join) = &self.rail {
            let source = join.devices.clone();
            source.update(cx, |source, cx| source.quiesced(cx));
        }
    }
}
