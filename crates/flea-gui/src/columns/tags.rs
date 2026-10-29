//! ColumnRow's reserved slot and the solid swatches that hug the painted name.
use super::{Clip, Theme, CLIP, GAP, ROW_HEIGHT};
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

pub(super) fn paint(
    facts: &RowFacts,
    at: Point<Pixels>,
    name_width: f32,
    geometry: super::super::geometry::Cells,
    clip: Clip,
    theme: &Theme,
    window: &mut Window,
) {
    let Some(names) = &facts.tags else { return };
    let start =
        geometry.name_x + name_width + GAP + if clip == Clip::None { 0. } else { CLIP + GAP };
    crate::row::tags::paint(
        names,
        point(at.x + px(start), at.y + px((ROW_HEIGHT - SWATCH_SIZE) / 2.)),
        (geometry.name_right - start).min(ROW_SLOT),
        &theme.tag_listing_inks,
        theme.foreground,
        window,
    );
}
