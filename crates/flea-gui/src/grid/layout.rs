//! The grid's placement in one pass: tile rows, the scroll extent, tile boxes, and the Contain reveal with its last-row gap.
use super::geometry::{
    cell_height_for, cell_width_for, centred_x, columns_for, grid_pad_y, Density, Metrics, Rect,
    ThumbSize, GLYPH_CAP,
};
use std::ops::Range;

/// One grid's geometry at one size; scroll is the content y at the view top, content y 0 being the top of the top margin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub metrics: Metrics,
    pub width: f32,
    pub height: f32,
    pub thumb: f32,
    pub pad_y: f32,
    pub columns: usize,
    pub cell_width: f32,
    pub cell_height: f32,
    pub total: usize,
}

impl Layout {
    /// GridArea.qml's columns, cellWidth and cellHeightPx: the scroll lane is one row pad, and a rename grows every cell alike.
    pub fn new(
        metrics: Metrics,
        size: (f32, f32),
        thumb: ThumbSize,
        density: Density,
        total: usize,
        rename_extra: f32,
    ) -> Self {
        let (width, height) = size;
        let pad_y = grid_pad_y(metrics.pad_x, density);
        let thumb = thumb.pixels();
        let columns = columns_for(
            width,
            metrics.min_cell,
            thumb,
            metrics.pad_x,
            metrics.gap,
            metrics.pad_x,
        );
        Self {
            metrics,
            width,
            height,
            thumb,
            pad_y,
            columns,
            cell_width: cell_width_for(width, columns, metrics.gap, metrics.pad_x),
            cell_height: cell_height_for(
                thumb,
                metrics.gap,
                metrics.caption_height().ceil(),
                pad_y,
            ) + rename_extra.max(0.),
            total,
        }
    }

    /// Tile rows the shown items fill, the partial last row counted whole.
    pub fn rows(&self) -> usize {
        self.total.div_ceil(self.columns)
    }
    /// GridArea.qml tileRows: never under one, so an empty grid still has a row to measure.
    pub fn tile_rows(&self) -> usize {
        self.rows().max(1)
    }
    /// GridArea.qml visibleTileRows: the rows a view height straddles at most.
    pub fn visible_rows(&self) -> usize {
        ((self.height / self.cell_height).ceil() as usize).max(1)
    }
    /// Both margins, the rows and the chrome-high footer the status bar floats over.
    pub fn extent(&self) -> f32 {
        2. * self.metrics.gap + self.rows() as f32 * self.cell_height + self.metrics.chrome
    }
    pub fn max_scroll(&self) -> f32 {
        (self.extent() - self.height).max(0.)
    }
    pub fn clamp(&self, scroll: f32) -> f32 {
        scroll.clamp(0., self.max_scroll())
    }
    pub fn row_top(&self, row: usize) -> f32 {
        self.metrics.gap + row as f32 * self.cell_height
    }

    /// GridView.Contain then keepLastRowClear in the same pass, so a cursor landing on the last row is lifted before any frame draws it flush.
    pub fn reveal(&self, scroll: f32, view: usize) -> f32 {
        let top = self.row_top(view / self.columns);
        let mut next = scroll;
        if top + self.cell_height > next + self.height {
            next = top + self.cell_height - self.height;
        }
        if top < next {
            next = top;
        }
        self.keep_last_row_clear(self.clamp(next), view)
    }

    /// Contain parks a last-row tile flush on the view bottom, so keep one gap below it; a row already clear stays put.
    pub fn keep_last_row_clear(&self, scroll: f32, view: usize) -> f32 {
        if self.extent() <= self.height || view / self.columns != self.tile_rows() - 1 {
            return scroll;
        }
        scroll.max(self.row_top(self.rows()) + self.metrics.gap - self.height)
    }

    /// GridArea.qml coversCursor: the tile overlaps the viewport.
    pub fn covers(&self, scroll: f32, view: usize) -> bool {
        let top = self.row_top(view / self.columns);
        top < scroll + self.height && top + self.cell_height > scroll
    }

    /// Thumbs.viewport over tile rows, in items: the first may lie one row above 0 while the top margin shows, as Qt's view top does.
    pub fn viewport(&self, scroll: f32) -> (i64, i64) {
        let top = scroll - self.metrics.gap;
        let first = (top / self.cell_height).floor() as i64;
        let bottom = top + self.visible_rows() as f32 * self.cell_height;
        let last = (self.tile_rows() as i64 - 1).min((bottom / self.cell_height).ceil() as i64 - 1);
        let columns = self.columns as i64;
        (
            first * columns,
            (self.total as i64 - 1).min((last + 1) * columns - 1),
        )
    }

    /// The view indices whose cells meet the viewport, the partial row at either edge included.
    pub fn drawn(&self, scroll: f32) -> Range<usize> {
        if self.total == 0 {
            return 0..0;
        }
        let top = scroll - self.metrics.gap;
        let first = (top / self.cell_height).floor().max(0.) as usize;
        let last = ((top + self.height) / self.cell_height).ceil().max(0.) as usize;
        (first * self.columns).min(self.total)..(last * self.columns).min(self.total)
    }

    /// The cell of a view index, in view pixels from the grid's own top left.
    pub fn cell(&self, scroll: f32, view: usize) -> Rect {
        Rect {
            x: self.metrics.gap + (view % self.columns) as f32 * self.cell_width,
            y: self.row_top(view / self.columns) - scroll,
            width: self.cell_width,
            height: self.cell_height,
        }
    }
    /// The plate and cursor frame, inset one hairline so neighbours never share an edge.
    pub fn frame(&self, cell: Rect) -> Rect {
        let inset = self.metrics.hairline;
        Rect {
            x: cell.x + inset,
            y: cell.y + inset,
            width: cell.width - 2. * inset,
            height: cell.height - 2. * inset,
        }
    }
    /// The thumbnail slot, centred across the cell on GridTile.qml's whole pixel under the density's top pad.
    pub fn slot(&self, cell: Rect) -> Rect {
        Rect {
            x: cell.x + centred_x(cell.width, self.thumb),
            y: cell.y + self.pad_y,
            width: self.thumb,
            height: self.thumb,
        }
    }
    /// A mark never grows past the glyph cap, centred in the larger slots.
    pub fn mark(&self, slot: Rect) -> Rect {
        let side = slot.width.min(GLYPH_CAP);
        Rect {
            x: slot.x + (slot.width - side) / 2.,
            y: slot.y + (slot.height - side) / 2.,
            width: side,
            height: side,
        }
    }
    /// The caption label: one gap under the slot, a gap in from each side, and a clip mark's strip off the right.
    pub fn caption(&self, cell: Rect, clipped: bool, lines: usize) -> Rect {
        let gap = self.metrics.gap;
        let strip = if clipped {
            gap + self.metrics.clip_mark
        } else {
            0.
        };
        let line = self.metrics.caption_line;
        let height = if lines == 1 {
            line
        } else {
            self.metrics.caption_height()
        };
        Rect {
            x: cell.x + gap,
            y: cell.y + self.pad_y + self.thumb + gap,
            width: (cell.width - 2. * gap - strip).max(0.),
            height,
        }
    }

    /// The view index whose cell holds a grid point, None past the tiles or between the margins.
    pub fn index_at(&self, scroll: f32, x: f32, y: f32) -> Option<usize> {
        let (cx, cy) = (x - self.metrics.gap, y + scroll - self.metrics.gap);
        if cx < 0. || cy < 0. {
            return None;
        }
        let column = (cx / self.cell_width) as usize;
        let index = (cy / self.cell_height) as usize * self.columns + column;
        (column < self.columns && index < self.total).then_some(index)
    }
}
