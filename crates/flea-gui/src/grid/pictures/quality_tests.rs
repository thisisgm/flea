//! Grid refinement keeps drawn rows within the pixel budget and draws on whole device pixels.
use super::{
    policy::{Mode, Policy},
    quality::{bounds, evict},
    Map, Pictures, Slot,
};
use crate::grid::{
    element::{Identity, PictureKind, Thumbnail},
    geometry::Rect,
};
use gpui::RenderImage;
use std::sync::Arc;

fn named(name: &str, w: u32, h: u32, image: bool) -> Slot {
    let buffer = image::RgbaImage::new(w, h);
    Slot::Ready {
        file: String::new(),
        picture: Arc::new(Thumbnail {
            identity: Identity {
                name: name.into(),
                size: 1,
                mtime: 1,
            },
            kind: PictureKind::Image,
            image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
        }),
        image,
    }
}

fn ready(row: usize, w: u32, h: u32) -> Slot {
    named(&format!("{row}.png"), w, h, true)
}

#[test]
fn preview_quality_grid_eviction_keeps_drawn_rows_and_ends_under_budget() {
    let mut map = Map::default();
    for row in 0..8 {
        map.remember(row, ready(row, 64, 32));
    }
    let budget = 7 * 64 * 32;
    evict(&mut map, &[0, 1], budget);
    let kept: Vec<_> = map.order.iter().copied().collect();
    assert_eq!(
        kept,
        [0, 1, 3, 4, 5, 6, 7],
        "the oldest undrawn row gives way first"
    );
    evict(&mut map, &kept, 64 * 32);
    assert_eq!(
        map.order.len(),
        7,
        "drawn rows stay even over budget, and the loop ends"
    );
}

#[test]
fn preview_quality_grid_pictures_centre_on_whole_device_pixels() {
    let slot = Rect {
        x: 10.,
        y: 20.,
        width: 100.,
        height: 100.,
    };
    for device in [1., 1.25, 1.5, 2.] {
        let picture = bounds(false, slot, (300, 200), device);
        for side in [picture.width, picture.height] {
            let pixels = f64::from(side) * device;
            assert!((pixels - pixels.round()).abs() < 1e-3, "{side} at {device}");
        }
        let (left, right) = (
            picture.x - slot.x,
            slot.x + slot.width - picture.x - picture.width,
        );
        let (top, bottom) = (
            picture.y - slot.y,
            slot.y + slot.height - picture.y - picture.height,
        );
        assert!(
            (left - right).abs() <= 1. && (top - bottom).abs() <= 1.,
            "{picture:?} at {device}"
        );
    }
    let cached = bounds(true, slot, (48, 32), 2.);
    assert!(
        cached.width <= 24.01 && cached.height <= 16.01,
        "cache pixels stay 1:1 at scale 2"
    );
    // A tile holding its source's own pixels draws one per device pixel, centred, never enlarged to the slot.
    for device in [1.25, 1.6, 2.] {
        let own = bounds(true, slot, (64, 48), device);
        for (side, want) in [(own.width, 64.), (own.height, 48.)] {
            let pixels = f64::from(side) * device;
            assert!(
                pixels <= want + 1e-3 && pixels > want - 0.01,
                "{side} at {device}"
            );
        }
        assert!(((own.x - slot.x) * 2. - (slot.width - own.width)).abs() <= 1.);
        let filled = bounds(false, slot, (64, 48), device);
        assert!(
            filled.width > own.width,
            "a stand-in thumbnail still fills its slot"
        );
    }
}

#[test]
fn preview_quality_grid_replaced_picture_drops_its_basis_and_refinement() {
    let mut map = Map::default();
    map.remember(0, ready(0, 64, 32));
    map.bases.entry(0).or_default();
    map.refinements.entry(0).or_default();
    map.remember(0, ready(0, 300, 200));
    assert!(
        !map.bases.contains_key(&0) && !map.refinements.contains_key(&0),
        "a new picture for the row is never sized from the one it replaced"
    );
}

#[test]
fn preview_quality_grid_own_pixels_only_for_whole_originals() {
    let mut map = Map {
        policy: Some(Policy {
            mode: Mode::Media,
            usb: true,
            network: false,
            phone: false,
            languages: true,
        }),
        class: Some((String::new(), String::new())),
        ..Default::default()
    };
    map.remember(0, named("art.png", 64, 48, true));
    map.remember(1, named("photo.png", 256, 171, true));
    map.remember(2, named("logo.svg", 64, 48, true));
    map.remember(3, named("clip.mp4", 160, 120, false));
    let pictures = Pictures::default();
    *pictures.map.borrow_mut() = map;
    let own = |pictures: &Pictures| {
        (0..4)
            .map(|row| pictures.own_pixels(row))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        own(&pictures),
        [true, false, false, false],
        "only a still raster's thumbnail under the cache edge is its whole original"
    );
    pictures.map.borrow_mut().bases.entry(1).or_default();
    assert!(pictures.own_pixels(1), "a refinement holds its own pixels");
    pictures.map.borrow_mut().class = Some((String::new(), "network".into()));
    assert_eq!(own(&pictures), [true; 4], "a cache-only picture is final");
}

#[test]
fn fx477_record_eviction_keeps_visible_matches_and_pending_cancellation() {
    let mut map = Map {
        drawn: Some(super::surface::Want {
            span: (0, 894),
            only: Some(((0..150).map(|row| row * 6).collect(), 0, 150)),
        }),
        ..Default::default()
    };
    for row in 0..150 {
        map.remember(row, ready(row, 1, 1));
    }
    for row in (150..900).step_by(6) {
        map.remember(row, ready(row, 1, 1));
    }
    for row in (0..900).step_by(6) {
        assert!(
            matches!(map.slots.get(&row), Some(Slot::Ready { .. })),
            "visible row {row}"
        );
    }
    assert_eq!(map.order.len(), super::CAP);
    let pending = 1000;
    map.remember(
        pending,
        Slot::Asked {
            identity: Identity {
                name: "pending.png".into(),
                size: 1,
                mtime: 1,
            },
            image: true,
            cache: false,
        },
    );
    evict(&mut map, &[], 0);
    assert_eq!(
        map.pending().collect::<Vec<_>>(),
        [pending],
        "pixel eviction cannot lose a backend cancellation"
    );
}

#[test]
fn fx477_a_viewport_larger_than_the_record_cap_keeps_every_visible_picture() {
    let mut map = Map {
        drawn: Some(super::surface::Want {
            span: (0, super::CAP),
            only: None,
        }),
        ..Default::default()
    };
    for row in 0..=super::CAP {
        map.remember(row, ready(row, 1, 1));
    }
    assert_eq!(map.order.len(), super::CAP + 1);
    map.drawn = Some(super::surface::Want {
        span: (1, super::CAP),
        only: None,
    });
    map.trim();
    assert!(!map.slots.contains_key(&0));
    assert_eq!(map.order.len(), super::CAP);
}
