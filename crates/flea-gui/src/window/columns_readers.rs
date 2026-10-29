//! U09 readers share the exact painted snapshots and boxes used by the Columns pointer handler.
use super::PaneHost;
use crate::{
    columns::{geometry::Slot, peeks::join_names, TrackRows},
    ipc::Adapter,
    list::columns::Column,
};
use flea::ipc::wire::Reply;

pub(crate) struct Reader<'a> {
    pub host: &'a PaneHost,
    pub visible: bool,
    pub cx: &'a gpui::App,
}

impl Reader<'_> {
    fn rect(&self, slot: Slot) -> String {
        let Some(columns) = self.host.columns.as_ref().filter(|_| self.visible) else {
            return String::new();
        };
        let Some(paint) = columns.painters.get(slot) else {
            return String::new();
        };
        let paint = paint.borrow();
        let Some(drawn) = &paint.drawn else {
            return String::new();
        };
        let bounds = drawn.bounds;
        let left = f32::from(bounds.origin.x).round();
        let top = f32::from(bounds.origin.y).round();
        let right = f32::from(bounds.origin.x + bounds.size.width).round();
        let bottom = f32::from(bounds.origin.y + bounds.size.height).round();
        format!("{left} {top} {} {}", right - left, bottom - top)
    }

    fn centre(&self, slot: Slot, index: usize) -> String {
        let Some(columns) = self.host.columns.as_ref().filter(|_| self.visible) else {
            return String::new();
        };
        let Some(paint) = columns.painters.get(slot) else {
            return String::new();
        };
        let paint = paint.borrow();
        let Some(drawn) = &paint.drawn else {
            return String::new();
        };
        if !paint.visible().any(|at| at == index) {
            return String::new();
        }
        let bounds = drawn.bounds;
        let x = f32::from(bounds.origin.x + bounds.size.width / 2.);
        let row = crate::list::pane::ROW_HEIGHT;
        let position = drawn.position_of(index).unwrap_or(index);
        let y =
            f32::from(bounds.origin.y) + drawn.grow.top(position, row) - drawn.scroll + row / 2.;
        format!("{} {}", x.round(), y.round())
    }

    /// ColumnsArea.childItemAt: a child row's name while the track draws that row on screen.
    fn row_name(&self, slot: Slot, index: usize) -> String {
        let Some(columns) = self.host.columns.as_ref().filter(|_| self.visible) else {
            return String::new();
        };
        let Some(paint) = columns.painters.get(slot) else {
            return String::new();
        };
        let paint = paint.borrow();
        if !paint.visible().any(|at| at == index) {
            return String::new();
        }
        match paint.drawn.as_ref().map(|drawn| &drawn.view().rows) {
            Some(TrackRows::Peek(rows)) => rows
                .get(index)
                .map(|row| row.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The child's empty tile mark as its last frame painted it, FleaWindow.rectOf of the mark item.
    fn child_mark_rect(&self) -> String {
        self.host
            .columns
            .as_ref()
            .filter(|_| self.visible)
            .and_then(|columns| columns.heroes[Slot::Child.index()].as_ref())
            .map(|hero| hero.read(self.cx))
            .filter(|hero| hero.painted().is_some_and(|(shown, _, _)| shown))
            .and_then(|hero| hero.painted.get("mark"))
            .map(|mark| {
                let b = mark.bounds;
                crate::ipc::rows::rect_of(
                    f32::from(b.origin.x),
                    f32::from(b.origin.y),
                    f32::from(b.size.width),
                    f32::from(b.size.height),
                )
            })
            .unwrap_or_default()
    }

    fn names(&self, slot: &str) -> String {
        let slot = match slot {
            "parent" => Slot::Parent,
            "child" => Slot::Child,
            _ => return String::new(),
        };
        let Some(columns) = self.host.columns.as_ref().filter(|_| self.visible) else {
            return String::new();
        };
        let Some(paint) = columns.painters.get(slot) else {
            return String::new();
        };
        let paint = paint.borrow();
        match paint.drawn.as_ref().map(|drawn| &drawn.view().rows) {
            Some(TrackRows::Peek(rows)) => join_names(rows),
            _ => String::new(),
        }
    }
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let text = arguments.first().map_or("", String::as_str);
        let index = text.parse().unwrap_or(usize::MAX);
        let value = match function {
            "columnWidths" => {
                let list = self.host.list.borrow();
                let entries: Vec<String> = Column::ALL
                    .into_iter()
                    .filter_map(|column| {
                        list.view
                            .stored
                            .get(column)
                            .map(|width| format!("\"{}\":{width}", column.key()))
                    })
                    .collect();
                format!("{{{}}}", entries.join(","))
            }
            // ui/Ipc.qml answers it from the drawn Columns view only; a hidden area draws no column.
            "columnCount" => self
                .host
                .columns
                .as_ref()
                .filter(|_| self.visible)
                .map_or(0, |columns| columns.area.tracks_built())
                .to_string(),
            "columnActiveRect" => self.rect(Slot::Active),
            "columnNames" => self.names(text),
            "columnPeekNames" => self
                .host
                .columns
                .as_ref()
                .map(|columns| {
                    let folders = self.host.nav.folders.borrow();
                    columns.area.peeks.names(&crate::columns::peeks::peek_key(
                        text,
                        folders.hidden,
                        folders.hidden_last,
                    ))
                })
                .unwrap_or_default(),
            "columnParentRowCentre" => self.centre(Slot::Parent, index),
            "columnGrandparentRowCentre" => self.centre(Slot::Grandparent, index),
            "columnGreatGrandparentRowCentre" => self.centre(Slot::GreatGrandparent, index),
            "columnChildRowCentre" => self.centre(Slot::Child, index),
            "columnChildRowName" => self.row_name(Slot::Child, index),
            "columnChildMarkRect" => self.child_mark_rect(),
            // ColumnsArea.activeContentY: in Columns the scroll is the active column's, as its last frame drew it.
            "viewContentY" if self.visible => self
                .host
                .columns
                .as_ref()
                .and_then(|columns| columns.painters.get(Slot::Active))
                .and_then(|paint| paint.borrow().drawn.as_ref().map(|drawn| drawn.scroll))
                .map(|scroll| scroll.round().to_string())
                .unwrap_or_default(),
            "columnChildEmpty" => self
                .host
                .columns
                .as_ref()
                .filter(|_| self.visible)
                .and_then(|columns| columns.heroes[Slot::Child.index()].as_ref())
                .and_then(|hero| hero.read(self.cx).painted())
                .map(|(shown, opacity, mark)| format!("{shown} {opacity:.2} {mark:.2}"))
                .unwrap_or_default(),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

impl crate::state::Shell {
    /// The active column's cursor row as the keyboard menu anchors on it, 14 px into the column; None outside Columns.
    pub(crate) fn columns_cursor_anchor(&self, side: usize) -> Option<(f32, f32)> {
        if self.shown_view() != crate::window::startup::View::Columns {
            return None;
        }
        let host = self.panes[side].as_ref()?;
        let paint = host
            .columns
            .as_ref()?
            .painters
            .get(crate::columns::geometry::Slot::Active)?
            .borrow();
        let drawn = paint.drawn.as_ref()?;
        let row = crate::list::pane::ROW_HEIGHT;
        let at = drawn.position_of(host.pane.cursor)?;
        let y =
            f32::from(drawn.bounds.origin.y) + drawn.grow.top(at, row) - drawn.scroll + row / 2.;
        Some((f32::from(drawn.bounds.origin.x) + 14., y))
    }
}
