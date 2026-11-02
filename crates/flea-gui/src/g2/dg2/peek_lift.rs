//! ColumnRow.qml peekSource: a drag from a row of a peek track lifts that row from the track's folder, with the device and writability its peek named.
use super::*;
use crate::columns::{geometry::Slot, TrackRows};
use crate::row::marks::Mark;
use flea::ui_logic_dg1::row_path;
use flea::ui_logic_dg2::Marker;

// A peek lift belongs to no pane listing, so no own-row or same-listing check ever takes it for the active track's rows.
const NO_LISTING: usize = usize::MAX;

impl Shell {
    /// The lift and drag flock for a drag pressed on a peek track's row, or None on the active track, off a row, in another view or after the tracks moved.
    pub(super) fn peek_lift(
        &self,
        side: usize,
        window: &Window,
        serial: u64,
    ) -> Option<(Lift, crate::g2::dg1::carry::Flock)> {
        let host = self.panes[side].as_ref()?;
        // Columns outlives a switch to another view, so a press recorded there must not lift a later list or grid drag.
        if host.drawn_view != Some(crate::window::startup::View::Columns) {
            return None;
        }
        let columns = host.columns.as_ref()?;
        let (hover, modifiers) = columns.pressed.as_ref()?;
        let tracks = columns.area.tracks();
        if hover.slot == Slot::Active || !hover.holds(tracks, host.pane.held.listing) {
            return None;
        }
        let view = tracks.iter().find(|view| view.slot == hover.slot)?;
        let TrackRows::Peek(rows) = &view.rows else {
            return None;
        };
        let row = rows.get(hover.index)?;
        let source = columns.area.peeks.source(&view.path);
        let carried = vec![hover.index];
        let lift = Lift {
            side,
            listing: NO_LISTING,
            marker: Marker {
                instance: format!("{}#{serial}", super::super::instance()),
                rows: carried.clone(),
                copy: modifiers.control,
                source: view.path.clone(),
                dev: source.as_ref().map_or(0, |source| source.dev),
                deletable: source.as_ref().is_none_or(|source| source.writable),
                shift: modifiers.shift,
                listing: NO_LISTING as u64,
            },
            rows: carried,
            paths: Some(vec![row_path(&view.path, &row.name)]),
            origin: at(window.mouse_position()),
            serial,
            left: false,
        };
        let languages = host.list.borrow().view.lang_marks;
        let picture = crate::g2::dg1::carry::Flock {
            marks: vec![Mark::for_file(&row.name, row.dir, 0, &row.icon, languages)],
            name: row.name.clone(),
        };
        Some((lift, picture))
    }
}
