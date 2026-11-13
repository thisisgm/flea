//! Grid thumbnails refine only while their rows are drawn, keyed by file facts and physical size.
use super::{
    surface::{drawn_rows, Surface},
    Map, Slot,
};
use crate::{
    grid::element::Thumbnail, preview_quality::physical, state::Shell, window::pane::PaneHost,
};
use gpui::{Context, Window};
use std::sync::Arc;

// Ready pictures stay within what CAP thumbnails at the shared cache's own edge would hold.
const PIXELS: u64 = super::CAP as u64 * EDGE * EDGE;
const EDGE: u64 = flea::ui_logic_preview_core::thumbs::CACHE_SIZE as u64;

pub(super) fn refine(
    map: &mut Map,
    host: &PaneHost,
    active: bool,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    let allowed = map
        .policy
        .zip(map.class_for(&host.path))
        .is_some_and(|(policy, class)| !policy.cache_only(class));
    let wanted = if active && allowed {
        drawn_rows(host, Surface::Grid).map_or_else(Vec::new, |want| want.rows())
    } else {
        Vec::new()
    };
    let gone: Vec<_> = map
        .refinements
        .keys()
        .copied()
        .filter(|row| !wanted.contains(row))
        .collect();
    for row in gone {
        if let Some(mut refinement) = map.refinements.remove(&row) {
            refinement.clear(cx);
        }
    }
    let Some(side) = host
        .grid
        .as_ref()
        .map(|grid| grid.view.thumb.pixels() as f64)
        .filter(|_| active)
    else {
        return;
    };
    for &row in &wanted {
        let Some(facts) = host.pane.held.row(row) else {
            continue;
        };
        if !map.policy.is_some_and(|policy| policy.allowed(facts)) {
            continue;
        }
        let Some(Slot::Ready { picture, .. }) = map.slots.get(&row) else {
            continue;
        };
        if !picture.identity.is(facts) {
            continue;
        }
        let (w, h) = map
            .bases
            .get(&row)
            .map_or_else(|| picture.decoded(), |basis| basis.of(&picture.image));
        if w == 0 || h == 0 {
            continue;
        }
        let fit = (side / w as f64).min(side / h as f64);
        let size = physical(w as f64 * fit, h as f64 * fit, window.scale_factor().into());
        let seed = (facts.icon == "video-x-generic").then(|| picture.image.clone());
        let path = crate::navigation::history::join(&host.path, &facts.name).into();
        let refinement = map.refinements.entry(row).or_default();
        let Some(Ok(image)) =
            refinement.poll_with_seed(path, size, (facts.size, facts.mtime), seed, window, cx)
        else {
            continue;
        };
        let Some(Slot::Ready { picture, .. }) = map.slots.get_mut(&row) else {
            continue;
        };
        if Arc::ptr_eq(&picture.image, &image) {
            continue;
        }
        map.bases.entry(row).or_default().made(&image, (w, h));
        let next = Arc::new(Thumbnail {
            identity: picture.identity.clone(),
            kind: picture.kind,
            image,
        });
        let old = std::mem::replace(picture, next);
        map.retire(&old);
        cx.notify();
    }
    evict(map, &wanted, PIXELS);
}

/// Forgets the oldest rows no frame draws until Ready pixels fit `budget`; drawn rows always stay.
pub(super) fn evict(map: &mut Map, wanted: &[usize], budget: u64) {
    let used = |map: &Map| -> u64 {
        map.slots
            .values()
            .chain(map.carried.iter())
            .filter_map(|slot| match slot {
                Slot::Ready { picture, .. } => Some(picture.decoded()),
                _ => None,
            })
            .map(|(w, h)| u64::from(w) * u64::from(h))
            .sum()
    };
    while used(map) > budget {
        if map.evict_carried() {
            continue;
        }
        let Some(row) = map.order.iter().copied().find(|row| {
            !wanted.contains(row) && matches!(map.slots.get(row), Some(Slot::Ready { .. }))
        }) else {
            break;
        };
        map.forget(row);
    }
}

/// A picture holding its source's own pixels draws at most 1:1 device pixels; every raster draws on a device-sized rect.
pub(crate) fn picture_bounds(
    pictures: Option<&super::Pictures>,
    listing: usize,
    kind: Option<crate::grid::element::PictureKind>,
    slot: crate::grid::geometry::Rect,
    decoded: (u32, u32),
    window: &Window,
) -> crate::grid::geometry::Rect {
    let own = kind != Some(crate::grid::element::PictureKind::Model)
        && pictures.is_some_and(|pictures| pictures.own_pixels(listing));
    bounds(own, slot, decoded, window.scale_factor() as f64)
}

pub(super) fn bounds(
    own: bool,
    slot: crate::grid::geometry::Rect,
    decoded: (u32, u32),
    device: f64,
) -> crate::grid::geometry::Rect {
    let mut picture = crate::grid::geometry::fit_picture(slot, decoded.0, decoded.1);
    let at_own_size = own
        && decoded.0 > 0
        && decoded.1 > 0
        && f64::from(decoded.0) <= f64::from(picture.width) * device
        && f64::from(decoded.1) <= f64::from(picture.height) * device;
    let (w, h) = if at_own_size {
        crate::preview_quality::own_size(decoded, device)
    } else {
        crate::preview_quality::logical_size(picture.width as f64, picture.height as f64, device)
    };
    picture.x = slot.x + ((slot.width - w) / 2.).floor();
    picture.y = slot.y + ((slot.height - h) / 2.).floor();
    picture.width = w;
    picture.height = h;
    picture
}

pub(super) fn begin_trace(map: &mut Map, row: usize, directory: &str, name: &str) {
    if crate::bench_thumbs::enabled() {
        crate::bench_thumbs::begin(
            "thumbnail",
            &crate::navigation::history::join(directory, name),
        );
    }
    if let Some(trace) = crate::preview_trace::Trace::begin(
        "grid",
        &crate::navigation::history::join(directory, name),
    ) {
        map.traces.insert(row, trace);
    }
}
