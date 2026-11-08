//! The Columns view's read-only IPC: the drawn tracks, their rows and the counters, with preview reads left to the preview producer.
use super::Adapter;
use crate::columns::{
    geometry::Slot,
    peeks::{join_names, Lens},
    preview_host::PreviewProducer,
    Area, TrackRows, TrackView,
};
use flea::ipc::wire::Reply;

/// What the shell knows about the drawn area beyond the model: where it sits, its height and the stored widths.
pub struct ColumnsAdapter<'a> {
    pub area: &'a Area,
    pub lens: Lens<'a>,
    /// The area's top-left corner in window pixels.
    pub origin: (f32, f32),
    pub height: f32,
    /// ViewState's columnWidths, already JSON.
    pub widths: &'a str,
    /// The child's empty tile as the states producer draws it: shown, its opacity and its mark's.
    pub child_empty: Option<(bool, f32, f32)>,
    pub preview: Option<&'a dyn PreviewProducer>,
}

fn slot_named(name: &str) -> Option<Slot> {
    match name {
        "greatGrandparent" => Some(Slot::GreatGrandparent),
        "grandparent" => Some(Slot::Grandparent),
        "parent" => Some(Slot::Parent),
        "active" => Some(Slot::Active),
        "child" => Some(Slot::Child),
        _ => None,
    }
}

impl ColumnsAdapter<'_> {
    fn track(&self, slot: Slot) -> Option<&TrackView> {
        self.area.tracks().iter().find(|view| view.slot == slot)
    }

    /// A peeked track's rows, only while the track draws them, as ColumnsArea's itemAtIndex answers.
    fn peek_rows(&self, slot: Slot) -> Option<&TrackView> {
        self.track(slot)
            .filter(|view| matches!(view.rows, TrackRows::Peek(_)))
    }

    /// The centre of a row the track has on screen, "x y" in whole window pixels; a row off screen has no item.
    fn row_centre(&self, slot: Slot, index: usize) -> String {
        let Some(view) = self.peek_rows(slot) else {
            return String::new();
        };
        let TrackRows::Peek(rows) = &view.rows else {
            return String::new();
        };
        let scroll = self.area.scroll(slot);
        let row = self.area.row();
        let top = index as f32 * row - scroll;
        if index >= rows.len() || top + row <= 0. || top >= self.height {
            return String::new();
        }
        let x = self.origin.0 + view.x + view.width / 2.;
        let y = self.origin.1 + top + row / 2.;
        format!("{} {}", x.round(), y.round())
    }

    fn row_name(&self, slot: Slot, index: usize) -> String {
        match self.peek_rows(slot).map(|view| &view.rows) {
            Some(TrackRows::Peek(rows)) => rows
                .get(index)
                .map(|row| row.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// drawnNames: the child while it shows a folder, the parent while it is drawn.
    fn drawn_names(&self, name: &str) -> String {
        let slot = match name {
            "child" => Slot::Child,
            "parent" => Slot::Parent,
            _ => return String::new(),
        };
        match self.peek_rows(slot).map(|view| &view.rows) {
            Some(TrackRows::Peek(rows)) if slot == Slot::Parent || self.area.host.shown.is_dir => {
                join_names(rows)
            }
            _ => String::new(),
        }
    }

    fn rect(&self, slot: Slot) -> String {
        let Some(view) = self.track(slot) else {
            return String::new();
        };
        let left = (self.origin.0 + view.x).round();
        let top = self.origin.1.round();
        let right = (self.origin.0 + view.x + view.width).round();
        let bottom = (self.origin.1 + self.height).round();
        format!("{left} {top} {} {}", right - left, bottom - top)
    }

    fn counters(&self) -> String {
        let area = self.area;
        format!(
            "{{\"count\":{},\"visible\":{},\"tracks\":{},\"constructed\":{},\"dropped\":{},\"peekRequests\":{},\"pending\":{},\"drawn\":{},\"peekVersion\":{}}}",
            area.count(),
            area.visible,
            area.tracks_built(),
            area.tracks_constructed(),
            area.tracks_dropped(),
            area.peek_requests,
            area.peeks.pending.len(),
            area.drawn.len(),
            area.peeks.version
        )
    }
}

impl Adapter for ColumnsAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let index = || {
            arguments
                .first()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(usize::MAX)
        };
        let text = || arguments.first().map_or("", String::as_str);
        let value = match function {
            "columnCount" => self.area.count().to_string(),
            "columnWidths" => self.widths.to_owned(),
            "columnNames" => self.drawn_names(text()),
            "columnPeekNames" => self.area.peeks.names(&self.lens.key(text())),
            "columnActiveRect" => self.rect(Slot::Active),
            "columnTrackRect" => slot_named(text())
                .map(|slot| self.rect(slot))
                .unwrap_or_default(),
            "columnParentRowCentre" => self.row_centre(Slot::Parent, index()),
            "columnGrandparentRowCentre" => self.row_centre(Slot::Grandparent, index()),
            "columnGreatGrandparentRowCentre" => self.row_centre(Slot::GreatGrandparent, index()),
            "columnChildRowCentre" => self.row_centre(Slot::Child, index()),
            "columnChildRowName" => self.row_name(Slot::Child, index()),
            "columnChildEmpty" => self
                .child_empty
                .map(|(shown, opacity, mark)| format!("{shown} {opacity:.2} {mark:.2}"))
                .unwrap_or_default(),
            "columnsState" => self.counters(),
            // The third column's preview reads are the producer's; with none registered the call falls through.
            _ => return self.preview?.ipc(function, arguments).map(Reply::value),
        };
        Some(Reply::value(value))
    }
}
