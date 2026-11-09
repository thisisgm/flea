//! GridTile's solid tag squares below the full caption box.
use super::Drawn;
use flea::{
    protocol::RowFacts,
    ui_logic_rail_places::tags::{swatches_that_fit, SWATCH_GAP, SWATCH_SIZE},
};
use gpui::{point, px, App, Pixels, Point, Window};

pub(super) fn paint(
    row: Option<&RowFacts>,
    drawn: &Drawn,
    origin: Point<Pixels>,
    window: &mut Window,
    cx: &App,
) {
    let Some(tags) = row.and_then(|row| row.tags.as_deref()) else {
        return;
    };
    let count = swatches_that_fit(tags.len(), drawn.caption.width);
    let width = (count as f32 * (SWATCH_SIZE + SWATCH_GAP) - SWATCH_GAP).max(0.);
    let theme = cx
        .try_global::<crate::palette::Theme>()
        .copied()
        .unwrap_or_default();
    crate::row::tags::paint(
        tags,
        point(
            origin.x + px(drawn.caption.x + (drawn.caption.width - width) / 2.),
            origin.y + px(drawn.caption.y + drawn.caption.height),
        ),
        drawn.caption.width,
        &theme.tag_listing_inks,
        theme.foreground,
        window,
    );
}
