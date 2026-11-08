//! Where the rename field stands over the name each view drew: the list's label column, the active column's name cell or a tile's caption.
use crate::{
    columns::geometry::{cells_in, Slot},
    list::{ListState, Metrics},
    state::Shell,
    window::{pane::PaneHost, startup::View},
};
use gpui::{point, px, size, Bounds, Pixels};

/// Where the field stands in the element its side drew last: the frame, the area that clips it, the text's inset, and the top of the refusal's band.
pub(crate) struct Place {
    pub frame: Bounds<Pixels>,
    pub clip: Bounds<Pixels>,
    pub inset: Pixels,
    pub below: Pixels,
}

/// The edited row this frame: placed, scrolled wholly out of its view, or in a view that has not drawn yet.
pub(crate) enum Spot {
    At(Place),
    Out,
    Unready,
}

impl Spot {
    /// The edit drops only once `reach`, the whole row or tile, has left the clip.
    fn of(place: Place, reach: Bounds<Pixels>) -> Self {
        let clip = place.clip;
        if clip.size.height <= px(0.) {
            Spot::Unready
        } else if reach.bottom() <= clip.top() || reach.top() >= clip.bottom() {
            Spot::Out
        } else {
            Spot::At(place)
        }
    }
}

impl Shell {
    /// The field sits over the name the side's view drew: the list's label column, the active column's name cell, or a tile's caption.
    pub(crate) fn rename_place(&self, side: usize, index: usize) -> Spot {
        let Some(host) = self.panes[side].as_ref() else {
            return Spot::Unready;
        };
        let list = host.list.borrow();
        match host.drawn_view {
            Some(View::Columns) => {
                let (slot, index, chevron) = self.rename_neighbour_row(side).unwrap_or((
                    Slot::Active,
                    index,
                    host.pane.held.row(index).is_some_and(|row| row.dir),
                ));
                columns_place(host, &list, slot, index, chevron)
            }
            Some(View::Grid) => grid_place(host, &list, index),
            _ if host.drawn_list => list_place(&list, index),
            _ => Spot::Unready,
        }
    }
}

fn list_place(list: &ListState, index: usize) -> Spot {
    let drawn = &list.drawn;
    let metrics = drawn.metrics;
    let row = metrics.row;
    let slot = Metrics::for_typeface(&list.typeface).slot;
    let position = list
        .visible()
        .iter()
        .find(|shown| shown.index == index)
        .map_or(index, |shown| shown.view);
    let top = drawn.bounds.origin.y + px(drawn.grow.top(position, row) - drawn.scroll_y);
    let depth = list
        .drawn_rows
        .row(index)
        .map_or(0., |row| crate::list::outline::indent(row, &metrics));
    let frame = Bounds::new(
        point(
            drawn.bounds.origin.x + px(drawn.layout.name.x + depth),
            top + px((row - slot) / 2.),
        ),
        size(px((drawn.layout.name.width - depth).max(0.)), px(slot)),
    );
    let place = Place {
        frame,
        clip: drawn.bounds,
        inset: px(metrics.gap),
        below: frame.bottom() + px(metrics.gap),
    };
    let reach = Bounds::new(
        point(drawn.bounds.origin.x, top),
        size(
            drawn.bounds.size.width,
            px(drawn.grow.height(position, row)),
        ),
    );
    Spot::of(place, reach)
}

// The active track's name cell, from its mark to the size cell, as RenameField.qml spans the Columns name.
fn columns_place(
    host: &PaneHost,
    list: &ListState,
    slot: Slot,
    index: usize,
    chevron: bool,
) -> Spot {
    let Some(paint) = host
        .columns
        .as_ref()
        .and_then(|columns| columns.painters.get(slot))
    else {
        return Spot::Unready;
    };
    let paint = paint.borrow();
    let Some(drawn) = paint.drawn.as_ref() else {
        return Spot::Unready;
    };
    let Some(position) = drawn.position_of(index) else {
        return Spot::Out;
    };
    let metrics = Metrics::for_typeface(&list.typeface);
    let field_height = metrics.slot.min(drawn.band);
    let top = drawn.bounds.origin.y
        + px(drawn.grow.top(position, drawn.row_height) - drawn.scroll.max(0.));
    // The track draws a folder cursor row with its chevron, which narrows the name cell.
    let name = cells_in(
        f32::from(drawn.bounds.size.width),
        drawn.lane,
        slot == Slot::Active,
        chevron,
    );
    let frame = Bounds::new(
        point(
            drawn.bounds.origin.x + px(name.name_x),
            top + px((drawn.band - field_height) / 2.),
        ),
        size(
            px((name.name_right - name.name_x).max(0.)),
            px(field_height),
        ),
    );
    let place = Place {
        frame,
        clip: drawn.bounds,
        inset: px(metrics.gap),
        below: frame.bottom() + px(metrics.gap),
    };
    let reach = Bounds::new(
        point(drawn.bounds.origin.x, top),
        size(
            drawn.bounds.size.width,
            px(drawn.grow.height(position, drawn.row_height)),
        ),
    );
    Spot::of(place, reach)
}

// The tile's caption band, a gap in from each side of its cell, one field high on the caption's first line.
fn grid_place(host: &PaneHost, list: &ListState, index: usize) -> Spot {
    let Some(grid) = host.grid.as_ref() else {
        return Spot::Unready;
    };
    let state = grid.state.borrow();
    let Some(layout) = state.layout else {
        return Spot::Unready;
    };
    let Some(tile) = state.drawn_row(index) else {
        return if state.drawn().is_empty() {
            Spot::Unready
        } else {
            Spot::Out
        };
    };
    let metrics = Metrics::for_typeface(&list.typeface);
    let gap = layout.metrics.gap;
    let origin = state.bounds.origin;
    let frame = Bounds::new(
        point(
            origin.x + px(tile.cell.x + gap),
            origin.y + px(tile.caption.y),
        ),
        size(px((tile.cell.width - 2. * gap).max(0.)), px(metrics.slot)),
    );
    let cell = Bounds::new(
        point(origin.x + px(tile.cell.x), origin.y + px(tile.cell.y)),
        size(px(tile.cell.width), px(tile.cell.height)),
    );
    let place = Place {
        frame,
        clip: state.bounds,
        inset: px(metrics.gap),
        below: frame.bottom() + px(gap),
    };
    Spot::of(place, cell)
}

#[cfg(test)]
mod vf4_tests {
    use super::*;
    use crate::list::{
        columns::{resolve, StoredWidths, Switched, Tokens},
        Typeface,
    };

    #[test]
    fn vf4_dual_rename_uses_the_list_height_and_gapless_dual_name_span() {
        let mut list = ListState::new(Typeface::omarchy_default());
        list.view.dual = true;
        list.drawn.metrics = list.file_metrics();
        list.drawn.bounds = Bounds::new(point(px(0.), px(0.)), size(px(454.), px(620.)));
        list.drawn.layout = resolve(
            440.,
            &Tokens {
                pad: 14.,
                icon: 19.,
                gap: 9.,
                name_min: 100.,
                measured: [70., 70., 125., 120.],
                flush_size: false,
            }
            .for_dual(true),
            Switched::default().for_dual(true),
            StoredWidths::default(),
        );
        let Spot::At(place) = list_place(&list, 0) else {
            panic!("field must be in view")
        };
        assert_eq!(place.frame.size.height, px(23.));
        assert_eq!(place.frame.size.width, px(189.));
        assert_eq!(place.frame.origin.x, px(42.));
        let size = list
            .drawn
            .layout
            .cell(crate::list::columns::Column::Size)
            .unwrap();
        assert_eq!(
            place.frame.right(),
            px(size.x),
            "the editor cannot cover size text"
        );
    }
}
