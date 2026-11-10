//! U09 readers share the exact painted snapshots and boxes used by the Columns pointer handler.
use super::PaneHost;
use crate::{
    columns::{
        element::lifted_row,
        geometry::{active_content, cells_in, Slot},
        paint::{Drawn, TrackPaint},
        peeks::join_names,
        TrackRows,
    },
    ipc::{
        rows::{centre_of, rect_of, round},
        Adapter,
    },
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
        let row = drawn.row_height;
        let position = drawn.position_of(index).unwrap_or(index);
        let y =
            f32::from(bounds.origin.y) + drawn.grow.top(position, row) - drawn.scroll + row / 2.;
        // A part-drawn edge row's centre lies outside the track; it names no row a click can reach.
        if !crate::ipc::rows::centre_inside(bounds, (x, y, 0., 0.)) {
            return String::new();
        }
        format!("{} {}", x.round(), y.round())
    }

    /// Ipc.qml listAreaRect in Columns: Pane.listArea is the whole ColumnsArea, every drawn track left to right.
    fn area_rect(&self) -> String {
        let Some(columns) = self.host.columns.as_ref().filter(|_| self.visible) else {
            return String::new();
        };
        let Some(paint) = columns.painters.get(Slot::Active) else {
            return String::new();
        };
        let paint = paint.borrow();
        let Some(drawn) = &paint.drawn else {
            return String::new();
        };
        let width = drawn
            .tracks
            .iter()
            .map(|view| view.x + view.width)
            .fold(0., f32::max);
        crate::ipc::rows::rect_of(
            f32::from(drawn.bounds.origin.x) - drawn.view().x,
            f32::from(drawn.bounds.origin.y),
            width,
            f32::from(drawn.bounds.size.height),
        )
    }

    fn visible_rows(&self) -> String {
        let Some(columns) = self.host.columns.as_ref() else {
            return String::new();
        };
        let Some(paint) = columns.painters.get(Slot::Active) else {
            return String::new();
        };
        let paint = paint.borrow();
        paint
            .drawn
            .as_ref()
            .map(|drawn| {
                let rows = f32::from(drawn.bounds.size.height) / drawn.row_height;
                (rows.ceil() as usize).max(1).to_string()
            })
            .unwrap_or_default()
    }

    /// The active track's last frame, while Columns is the drawn view.
    fn active<T>(&self, read: impl FnOnce(&TrackPaint, &Drawn) -> Option<T>) -> Option<T> {
        let columns = self.host.columns.as_ref().filter(|_| self.visible)?;
        let paint = columns.painters.get(Slot::Active)?.borrow();
        let drawn = paint.drawn.as_ref()?;
        read(&paint, drawn)
    }

    /// FleaWindow.rectOf(visibleItemFor(i)): a drawn row's whole box, lane excluded, its edge rows uncut.
    fn row_box(
        &self,
        paint: &TrackPaint,
        drawn: &Drawn,
        index: usize,
    ) -> Option<(f32, f32, f32, f32)> {
        if !paint.visible().any(|at| at == index)
            || drawn.held.as_ref()?.listing != self.host.pane.held.listing
        {
            return None;
        }
        paint.prepared(index)?;
        let position = drawn.position_of(index)?;
        let row = drawn.row_height;
        let bounds = drawn.bounds;
        Some((
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y) + drawn.grow.top(position, row) - drawn.scroll.max(0.),
            (f32::from(bounds.size.width) - drawn.lane).max(0.),
            drawn.grow.height(position, row),
        ))
    }

    /// Ipc.rowNameItem(): the painted name's box and its laid-out width, none while the row is its own rename editor.
    fn name_box(&self, index: usize) -> Option<(f32, f32, f32, f32, f32)> {
        self.active(|paint, drawn| {
            if drawn.focus.renaming == Some(index) {
                return None;
            }
            let (x, y, _, _) = self.row_box(paint, drawn, index)?;
            let prepared = paint.prepared(index)?;
            let cursor = drawn.focus.cursor == Some(index);
            let lifted = !cursor && lifted_row(drawn.view(), drawn.held.as_deref(), index);
            let chevron = prepared.dir && (cursor || lifted);
            let line = prepared.name_for(chevron, cursor).line();
            let cells = cells_in(
                f32::from(drawn.bounds.size.width),
                drawn.lane,
                true,
                chevron,
            );
            let height = f32::from(line.ascent + line.descent);
            Some((
                x + cells.name_x,
                y + (drawn.band - height) / 2.,
                (cells.name_right - cells.name_x).max(0.),
                height,
                f32::from(line.width),
            ))
        })
    }

    /// ColumnPane's viewport: originY + contentHeight - height over the rows, the rename band and the footer.
    fn end_y(&self) -> Option<String> {
        self.active(|paint, drawn| {
            let total = match &drawn.shown {
                Some(rows) => rows.len(),
                None => drawn.held.as_ref()?.total,
            };
            let end = active_content(drawn.grow, total, drawn.row_height, &paint.typeface)
                - f32::from(drawn.bounds.size.height);
            Some(round(end).to_string())
        })
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
            "columnTrackRect" => crate::ipc::columns::slot_named(text)
                .map(|slot| self.rect(slot))
                .unwrap_or_default(),
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
            // ui/Ipc.qml reads the pane's own list in Columns from the active column, its listArea and visibleItemFor.
            "rowCentre" if self.visible => self.centre(Slot::Active, index),
            "rowHovered" if self.visible => self
                .active(|paint, drawn| {
                    Some(
                        paint.visible().any(|at| at == index)
                            && drawn.focus.hover == Some((Slot::Active, index)),
                    )
                })
                .unwrap_or(false)
                .to_string(),
            "listAreaRect" if self.visible => self.area_rect(),
            "visibleRows" if self.visible => self.visible_rows(),
            "rowRect" if self.visible => self
                .active(|paint, drawn| self.row_box(paint, drawn, index))
                .map(|(x, y, w, h)| rect_of(x, y, w, h))
                .unwrap_or_default(),
            // Ipc.qml rowNameText: the decorated name of a row the active column draws, "" for any other.
            "rowNameText" if self.visible => self
                .active(|paint, drawn| {
                    self.row_box(paint, drawn, index)?;
                    drawn.held.as_ref()?.row(index).map(|facts| {
                        crate::row::decorated_name(&crate::list::outline::display(facts))
                    })
                })
                .unwrap_or_default(),
            "rowNameRect" if self.visible => self
                .name_box(index)
                .map(|(x, y, w, h, _)| rect_of(x, y, w, h))
                .unwrap_or_default(),
            "rowNameCentre" if self.visible => self
                .name_box(index)
                .map(|(x, y, w, h, content)| {
                    format!("{} {}", round(x + w.min(content) / 2.), round(y + h / 2.))
                })
                .unwrap_or_default(),
            "viewEndY" if self.visible => self.end_y().unwrap_or_default(),
            // Ipc.qml listingBackgroundCentre: the pane's own column, never a peek, and an empty one too.
            "listingBackgroundCentre" if self.visible => self
                .active(|_, drawn| {
                    let b = drawn.bounds;
                    Some(centre_of(
                        f32::from(b.origin.x),
                        f32::from(b.origin.y),
                        f32::from(b.size.width),
                        f32::from(b.size.height),
                    ))
                })
                .unwrap_or_default(),
            // Ipc.qml headerTop: Columns has no columns to head, so the collapsed strip sits on the tracks' top.
            "headerTop" if self.visible => self
                .active(|_, drawn| Some(round(f32::from(drawn.bounds.origin.y)).to_string()))
                .unwrap_or_default(),
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
        let row = drawn.row_height;
        let at = drawn.position_of(host.pane.cursor)?;
        let y =
            f32::from(drawn.bounds.origin.y) + drawn.grow.top(at, row) - drawn.scroll + row / 2.;
        Some((f32::from(drawn.bounds.origin.x) + 14., y))
    }
}
