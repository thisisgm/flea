//! Native file drag and drop over the G1 shell: a lift captures the carried rows' real paths, a drop resolves the folder under the pointer and hands one transfer or link to op1's collision host; flea::ui_logic_dg2 decides.
pub mod facts;
pub mod files;
pub mod join;
pub mod router;

#[cfg(test)]
mod drag_hig_tests;
#[cfg(test)]
mod feedback_tests;
#[cfg(test)]
mod rail_drop_tests;
#[cfg(test)]
mod router_tests;
#[cfg(test)]
mod tests;

pub use facts::Facts;
use flea::ui_logic_dg1::row_path;
use flea::ui_logic_dg2::{self as core, Dropped, Marker, PasteAsk, Proposed, Refusal, Seen};
use gpui::{
    ExternalDragPayload, MimePayload, MimeRepresentation, NativeDragAction, NativeDragActions,
    NativeDragPayload, SelectionIntent,
};
use std::sync::OnceLock;

/// Qt's startDragDistance: a press that travels less is a tap, and its drop is no drop.
pub const SLOP: f32 = 10.;

/// The pane facts a lift and a drop read; the join fills them from the mounted pane.
pub trait Rows {
    fn folder(&self) -> &str;
    fn listing(&self) -> usize;
    /// A row's name and whether it is a folder.
    fn row(&self, index: usize) -> Option<(&str, bool)>;
    /// Ascending selected rows.
    fn selected(&self) -> &[usize];
}

/// This process's drag identity, so another window tells its own lift from Flea's elsewhere.
pub fn instance() -> &'static str {
    static INSTANCE: OnceLock<String> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("{}-{nanos}", std::process::id())
    })
}

/// One drag this window started: its source pane, the carried rows and their paths captured at the lift.
#[derive(Clone, Debug, PartialEq)]
pub struct Lift {
    pub side: usize,
    pub listing: usize,
    pub rows: Vec<usize>,
    /// None when a carried row did not resolve, so no other app is handed a subset.
    pub paths: Option<Vec<String>>,
    pub marker: Marker,
    pub origin: (f32, f32),
    pub serial: u64,
    /// True once the pointer has left the slop, so a release back on the origin is still a drop (Qt's DragHandler stays active).
    pub left: bool,
}

/// The lift for a press on row index, or None when that row is gone.
pub fn lift(
    rows: &dyn Rows,
    side: usize,
    index: usize,
    origin: (f32, f32),
    ctrl: bool,
    shift: bool,
    serial: u64,
) -> Option<Lift> {
    rows.row(index)?;
    let carried = core::carried(rows.selected(), index);
    let paths = carried
        .iter()
        .map(|row| {
            rows.row(*row)
                .map(|(name, _)| row_path(rows.folder(), name))
        })
        .collect::<Option<Vec<_>>>();
    Some(Lift {
        side,
        listing: rows.listing(),
        marker: Marker {
            // This process and this lift, so a window tells its current lift from any earlier one.
            instance: format!("{}#{serial}", instance()),
            rows: carried.clone(),
            copy: ctrl,
            source: rows.folder().to_owned(),
            dev: 0,
            deletable: true,
            shift,
            listing: rows.listing() as u64,
        },
        rows: carried,
        paths,
        origin,
        serial,
        left: false,
    })
}

impl Lift {
    pub fn travelled(&self, at: (f32, f32)) -> bool {
        self.left || (at.0 - self.origin.0).hypot(at.1 - self.origin.1) >= SLOP
    }

    pub fn representations(&self) -> Vec<(&'static str, Vec<u8>)> {
        core::lift_payload(&self.marker, self.paths.as_deref())
    }

    /// What the platform is handed when the drag leaves the window: the marker and paths, with the lift's one action.
    pub fn payload(&self) -> ExternalDragPayload {
        let (copy, move_, preferred) = core::offer(self.marker.copy, self.marker.shift);
        ExternalDragPayload::Mime(NativeDragPayload {
            data: MimePayload::new(
                self.representations()
                    .into_iter()
                    .map(|(mime, bytes)| MimeRepresentation::new(mime, bytes)),
                SelectionIntent::Copy,
            ),
            actions: NativeDragActions {
                copy,
                move_,
                preferred: if preferred == core::Verb::Move {
                    NativeDragAction::Move
                } else {
                    NativeDragAction::Copy
                },
            },
            source_id: self.serial,
        })
    }

    /// Whether this lift carries row index of this listing, the one row a drop on itself must skip.
    pub fn carries(&self, listing: usize, index: usize) -> bool {
        self.listing == listing && self.rows.contains(&index)
    }
}

/// The folder a drop lands in: a folder row under the pointer that the drag does not carry, else the pane's folder.
pub fn spot(rows: &dyn Rows, hit: Option<usize>, own: Option<&Lift>) -> String {
    let row = hit
        .filter(|index| !own.is_some_and(|lift| lift.carries(rows.listing(), *index)))
        .and_then(|index| rows.row(index))
        .filter(|(_, dir)| *dir);
    match row {
        Some((name, _)) => row_path(rows.folder(), name),
        None => rows.folder().to_owned(),
    }
}

/// One drop as it reached a pane: this window's own lift, or another app's offer with its marker if any.
#[derive(Clone, Debug)]
pub struct Dropping<'a> {
    pub own: Option<&'a Lift>,
    pub marker: Option<&'a str>,
    pub urls: &'a [String],
    pub plain: Option<&'a str>,
    pub proposed: Proposed,
    pub ctrl: bool,
    pub shift: bool,
}

impl<'a> Dropping<'a> {
    pub fn own(lift: &'a Lift, ctrl: bool, shift: bool) -> Self {
        Self {
            own: Some(lift),
            marker: None,
            urls: &[],
            plain: None,
            proposed: Proposed::default(),
            ctrl,
            shift,
        }
    }

    pub fn native(
        marker: Option<&'a str>,
        urls: &'a [String],
        plain: Option<&'a str>,
        ctrl: bool,
        shift: bool,
        action: Option<NativeDragAction>,
    ) -> Self {
        // Keys held at the drop stand for the action they ask for, as a Qt proposedAction does; else the negotiated action.
        let proposed = match (ctrl, shift) {
            (true, true) => Proposed {
                copy: false,
                move_: false,
                link: true,
            },
            (false, true) => Proposed {
                copy: false,
                move_: true,
                link: false,
            },
            (true, false) => Proposed {
                copy: true,
                move_: false,
                link: false,
            },
            _ => Proposed {
                copy: action == Some(NativeDragAction::Copy),
                move_: action == Some(NativeDragAction::Move),
                link: false,
            },
        };
        Self {
            own: None,
            marker,
            urls,
            plain,
            proposed,
            ctrl,
            shift,
        }
    }
}

/// What a drop does: one ask for op1, a refusal sentence, or nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    Ask(PasteAsk),
    Refused(Refusal),
    Silent,
}

// A local modifier observes the complete key pair; with neither held, a foreign drop keeps the lift's bits (Drag.dropHeld).
fn seen(held: bool, observed: bool) -> Seen {
    match (held, observed) {
        (true, _) => Seen::Held,
        (false, true) => Seen::Released,
        (false, false) => Seen::Unseen,
    }
}

/// The decision for one drop into dest, with the source and destination devices read from disk.
pub fn decide(dropping: &Dropping<'_>, dest: &str, facts: Facts) -> Landing {
    let own_urls: Vec<String>;
    let (marker, urls) = match dropping.own {
        Some(lift) => {
            let Some(paths) = &lift.paths else {
                return Landing::Silent;
            };
            own_urls = paths.iter().map(|path| core::uri_for(path)).collect();
            (Some(lift.marker.clone()), own_urls.as_slice())
        }
        None => match dropping.marker {
            Some(text) => match Marker::parse(text) {
                Some(marker) => (Some(marker), dropping.urls),
                None => return Landing::Silent,
            },
            None => (None, dropping.urls),
        },
    };
    let marker = marker.map(|marker| Marker {
        dev: facts.src_dev,
        deletable: facts.deletable,
        ..marker
    });
    let keys_observed = dropping.own.is_some() || dropping.ctrl || dropping.shift;
    let input = Dropped {
        marker: marker.as_ref(),
        urls,
        plain: dropping.plain,
        proposed: dropping.proposed,
        ctrl: seen(dropping.ctrl, keys_observed),
        shift: seen(dropping.shift, keys_observed),
    };
    match core::drop_into(&input, dest, facts.dest_dev) {
        Ok(ask) => Landing::Ask(ask),
        Err(Some(refused)) => Landing::Refused(refused),
        Err(None) => Landing::Silent,
    }
}
#[cfg(test)]
mod mounted_negative_tests;
#[cfg(test)]
mod mounted_tests;

#[cfg(test)]
mod drop_aim_tests;
#[cfg(test)]
mod grid_drop_tests;
#[cfg(test)]
mod pb_devices_2_tests;

#[cfg(test)]
mod vf3_tests;

#[cfg(test)]
mod vf3_spring_cases;

#[cfg(test)]
mod vf3_wide_tests;

#[cfg(test)]
mod u_drag_drop_tests;

#[cfg(test)]
mod u_drag_drop_more_tests;

#[cfg(test)]
mod shelf_drop_tests;

#[cfg(test)]
mod floor_drop_tests;

#[cfg(test)]
mod edge_tests;

#[cfg(test)]
mod xwindow_tests;

#[cfg(test)]
mod u_drag_drop_cue_tests;

#[cfg(test)]
mod u_drag_drop_fidelity_tests;

#[cfg(test)]
mod u_drag_drop_spring_tests;

#[cfg(test)]
mod u_drag_drop_recent_tests;

#[cfg(test)]
mod native_start_tests;
