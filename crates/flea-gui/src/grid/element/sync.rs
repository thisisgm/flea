//! GridTile's sync badge on the lower right of its thumbnail, or of its mark when the tile draws one.
use super::{paint::bounds_of, Drawn, GridElement};
use crate::grid::layout::Layout;
use gpui::{App, Pixels, Point, Window};

pub(super) fn paint(
    element: &GridElement,
    drawn: &Drawn,
    layout: &Layout,
    origin: Point<Pixels>,
    window: &mut Window,
    cx: &App,
) {
    let mark = drawn.picture.unwrap_or_else(|| layout.mark(drawn.slot));
    let row = element.held.row(drawn.listing);
    crate::row::sync::paint(row, bounds_of(origin, mark), window, cx);
}
