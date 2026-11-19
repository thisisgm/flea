//! The cloud sync badge on a row's mark: the backend's `sy` field, drawn on the mark's lower right like a file manager's emblem.
use crate::palette::Theme;
use flea::protocol::{RowFacts, SyncMark};
use gpui::{
    point, px, size, App, Bounds, Hsla, Pixels, SharedString, TransformationMatrix, Window,
};

/// The badge's side as a share of the mark it sits on, never under the size its glyph still reads at.
const SHARE: f32 = 0.5;
/// A 15 px list or columns mark would give a 7.5 px badge, a coloured dot whose glyph no longer reads.
const MIN_SIDE: f32 = 11.;
/// The tag swatch a finished sync reads in: the theme's green, lifted to 3:1 on the listing (tagtheme::ROLES[3]).
const GREEN: usize = 3;
/// The background ring that lifts the badge off the mark under it.
const HALO: f32 = 1.;

const DISC: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="12"/></svg>"#;
const RING: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9.5" fill="none" stroke="black" stroke-width="5"/></svg>"#;

/// One disc with its glyph masked out, a single sprite: a glyph painted over a disc of the same bounds ties the
/// disc's draw order and may land under it. The mask's black shapes are the hole the halo's ground shows through.
macro_rules! badge {
    ($shapes:expr) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><mask id="g">"#,
            r#"<rect width="24" height="24" fill="white"/>"#,
            $shapes,
            r#"</mask><circle cx="12" cy="12" r="12" mask="url(#g)"/></svg>"#
        )
        .as_bytes()
    };
}

/// A stroked glyph inside the badge mask.
macro_rules! stroke {
    ($d:literal) => {
        concat!(
            r#"<path d=""#,
            $d,
            r#"" fill="none" stroke="black" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/>"#
        )
    };
}

/// The badge sprite with its glyph cut out; an excluded row draws a hollow ring and no glyph.
pub fn glyph(mark: SyncMark) -> Option<(SharedString, &'static [u8])> {
    let (key, data) = match mark {
        SyncMark::Synced => (
            "flea-sync:synced",
            badge!(stroke!("M6.5 12.5l3.5 3.5 7.5-8")),
        ),
        SyncMark::Syncing => (
            "flea-sync:syncing",
            badge!(stroke!("M17.5 9A6 6 0 1 0 18 14 M18 4.5V9h-4.5")),
        ),
        SyncMark::Partial => ("flea-sync:partial", badge!(stroke!("M7 12h10"))),
        // Filled shapes, not a stroke: the bar and its dot keep their weight at every badge size.
        SyncMark::Conflict => (
            "flea-sync:conflict",
            badge!(
                r#"<rect x="10.25" y="5" width="3.5" height="9.5" rx="1.75"/><circle cx="12" cy="18" r="2"/>"#
            ),
        ),
        SyncMark::Excluded => return None,
    };
    Some((SharedString::new_static(key), data))
}

/// The theme role each state reads in: done in the tag green, moving in the accent, a problem in the error ink.
/// Not the executable role: a theme may draw that yellow, next to an accent that is orange.
pub fn ink(mark: SyncMark, theme: &Theme) -> Hsla {
    match mark {
        SyncMark::Synced => theme.tag_listing_inks[GREEN],
        SyncMark::Syncing => theme.accent,
        SyncMark::Partial => theme.metadata,
        SyncMark::Conflict => theme.error,
        SyncMark::Excluded => theme.muted,
    }
}

/// The badge square on the mark's lower right corner. A small mark's badge, held at MIN_SIDE, overhangs that
/// corner by half of what it outgrew, so it covers less of the mark and stays inside the mark slot's padding.
pub fn bounds(mark: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = f32::from(mark.size.width);
    let side = (width * SHARE).max(MIN_SIDE).min(width);
    let overhang = px((side - width * SHARE) / 2.);
    let side = px(side);
    Bounds::new(
        point(
            mark.right() - side + overhang,
            mark.bottom() - side + overhang,
        ),
        size(side, side),
    )
}

/// Paints the row's badge over the mark or thumbnail it already drew at `mark`; a row with no state paints nothing.
pub fn paint(row: Option<&RowFacts>, mark: Bounds<Pixels>, window: &mut Window, cx: &App) -> bool {
    let Some(state) = row.and_then(|row| row.sync) else {
        return false;
    };
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    let badge = bounds(mark);
    let halo = Bounds::new(
        badge.origin - point(px(HALO), px(HALO)),
        badge.size + size(px(2. * HALO), px(2. * HALO)),
    );
    let ground = Hsla {
        a: 1.,
        ..theme.background
    };
    let (key, data) = glyph(state).unwrap_or((SharedString::new_static("flea-sync:ring"), RING));
    // Each sprite in a layer of its own: inside the view's one layer every sprite shares an order and the scene
    // breaks the tie by atlas tile, so the mark, the halo and the badge would stack in whatever order they were cached.
    let mut sprite = |at: Bounds<Pixels>, key: SharedString, data: &'static [u8], ink: Hsla| {
        window.paint_layer(at, |window| {
            window
                .paint_svg(at, key, Some(data), TransformationMatrix::unit(), ink, cx)
                .is_ok()
        })
    };
    let painted = sprite(
        halo,
        SharedString::new_static("flea-sync:disc"),
        DISC,
        ground,
    ) & sprite(badge, key, data, ink(state, &theme));
    #[cfg(test)]
    PAINTED.with(|painted| painted.borrow_mut().push((state, badge)));
    painted
}

#[cfg(test)]
thread_local! {
    /// Every badge the last paints drew, so a view test reads what reached the scene without a pixel probe.
    pub static PAINTED: std::cell::RefCell<Vec<(SyncMark, Bounds<Pixels>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
