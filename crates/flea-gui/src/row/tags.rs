//! Solid tag squares shared by listing, column and grid painters, with no text fallback.
use flea::ui_logic_pl4::tags::{index_of, swatches_that_fit, SWATCH_GAP, SWATCH_SIZE};
use gpui::{fill, point, px, size, Bounds, Hsla, Pixels, Point, Window};

/// The name a test recorder files each painted swatch under.
#[cfg(test)]
pub const SWATCH_MARK: &str = "flea-tag:swatch";

/// Draw only whole swatches in the available room, with no counter or per-frame allocation.
pub fn paint(
    names: &[String],
    at: Point<Pixels>,
    room: f32,
    inks: &[Hsla; 7],
    foreground: Hsla,
    window: &mut Window,
) -> usize {
    let count = swatches_that_fit(names.len(), room);
    for (slot, name) in names.iter().take(count).enumerate() {
        let mut ink = index_of(name).map_or(foreground, |at| inks[at]);
        ink.a = 1.;
        let swatch = Bounds::new(
            point(at.x + px(slot as f32 * (SWATCH_SIZE + SWATCH_GAP)), at.y),
            size(px(SWATCH_SIZE), px(SWATCH_SIZE)),
        );
        #[cfg(test)]
        crate::bfx_cols_tests::record_mark(SWATCH_MARK, swatch);
        window.paint_quad(fill(swatch, ink));
    }
    count
}
