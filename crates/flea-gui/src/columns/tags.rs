//! ColumnRow's reserved slot and the solid swatches that hug the painted name.
use super::{Clip, Theme, GAP};
use flea::{
    protocol::RowFacts,
    ui_logic_pl4::tags::{ROW_SLOT, SWATCH_SIZE},
};
use gpui::{point, px, Pixels, Point, Window};

pub(super) fn reserved_cells(shown: bool, advance: f32) -> usize {
    if shown {
        ((ROW_SLOT + GAP) / advance.max(1.)).ceil() as usize
    } else {
        0
    }
}

/// The swatches centre on `mid`, the row's left edge at its centre line.
pub(super) fn paint(
    facts: &RowFacts,
    mid: Point<Pixels>,
    name_width: f32,
    geometry: super::super::geometry::Cells,
    clip: Clip,
    theme: &Theme,
    window: &mut Window,
) {
    let Some(names) = &facts.tags else { return };
    let start = geometry.name_x + name_width + GAP;
    let right = if clip == Clip::None {
        geometry.name_right
    } else {
        geometry.clip_x() - GAP
    };
    crate::row::tags::paint(
        names,
        point(mid.x + px(start), mid.y - px(SWATCH_SIZE / 2.)),
        (right - start).min(ROW_SLOT),
        &theme.tag_listing_inks,
        theme.foreground,
        window,
    );
}
