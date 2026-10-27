//! The NETWORK heading's readers: networkBuilt, the plus ink, its hit target and the first row's indicator dot.
use super::super::rows::{centre_of, round};
use super::RailAdapter;
use crate::g2::pl4::rows::{add_hit, LaidRow};
use flea::ui_logic_pl4::rail::Group;

/// What networkBuilt answers: the places setting, the built service and its two reads.
#[derive(Clone, Copy, Default)]
pub struct NetworkBuilt {
    pub shown: bool,
    pub present: bool,
    pub listed: bool,
    pub mountinfo: bool,
}

impl NetworkBuilt {
    pub(super) fn ready(self) -> bool {
        self.shown && self.present && self.listed && self.mountinfo
    }
}

// FleaWindow.boxOf(): "x width centre", the edges rounded and the centre kept at three decimals.
fn box_of(x: f32, width: f32) -> String {
    format!("{} {} {:.3}", round(x), round(width), x + width / 2.)
}

impl RailAdapter<'_> {
    /// The NETWORK heading's plus ink as painted, in window coordinates.
    pub(super) fn network_add(&self) -> Option<crate::rail::geometry::Rect> {
        if !self.shown() {
            return None;
        }
        self.producer().and_then(|producer| {
            producer
                .laid
                .borrow()
                .headers
                .iter()
                .find_map(|header| header.add)
        })
    }

    /// The first drawn NETWORK row, so the mark readers aim at the row the heading's dot belongs to.
    pub(super) fn first_network_row(&self) -> Option<LaidRow> {
        let entries = self.entries();
        let position = entries
            .iter()
            .position(|entry| entry.group == Group::Network)?;
        self.producer()?.laid.borrow().row(position).cloned()
    }

    /// The plus ink, its hit target and the rail's own indicator dot, in ui/Ipc.qml's shape.
    pub(super) fn mark_geometry(&self) -> Option<String> {
        let add = self.network_add()?;
        let row = self.first_network_row()?;
        let hit = add_hit(add);
        Some(format!(
            "{}|{}|{}",
            box_of(add.x, add.width),
            box_of(hit.x, hit.width),
            box_of(row.indicator_rect.x, row.indicator_rect.width)
        ))
    }

    /// The hit target's own middle, in ui/Ipc.qml's shape.
    pub(super) fn mark_centre(&self) -> Option<String> {
        let hit = add_hit(self.network_add()?);
        Some(centre_of(hit.x, hit.y, hit.width, hit.height))
    }
}
