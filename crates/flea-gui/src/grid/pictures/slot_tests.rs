//! EY-526: a row's picture keeps its own pixels inside its mark slot and only a larger one is fitted down, smoothly.
use super::{decor::slot_rect, Pictures};
use crate::grid::element::{Identity, PictureKind, Thumbnail};
use gpui::{point, px, size, Bounds, Pixels, RenderImage};
use std::sync::Arc;

const SLOT: f32 = 24.;

fn picture(side: u32) -> Arc<Thumbnail> {
    let buffer = image::RgbaImage::from_fn(side, side, |x, y| {
        image::Rgba([(x * 255 / side.max(2)) as u8, (y % 2 * 255) as u8, 40, 255])
    });
    Arc::new(Thumbnail {
        identity: Identity {
            name: format!("{side}.png"),
            size: 1,
            mtime: 1,
        },
        kind: PictureKind::Image,
        image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
    })
}

fn device(rect: Bounds<Pixels>, scale: f32) -> (f32, f32, f32, f32) {
    let d = |value: Pixels| f32::from(value) * scale;
    (
        d(rect.origin.x),
        d(rect.origin.y),
        d(rect.size.width),
        d(rect.size.height),
    )
}

#[test]
fn fixpreview_ey526_a_small_raster_keeps_its_own_pixels_and_a_large_one_fits_the_slot() {
    let slot = Bounds::new(point(px(10.), px(40.)), size(px(SLOT), px(SLOT)));
    for scale in [1f32, 1.25, 2.] {
        let room = SLOT * scale;
        let slot_side = room.round() as u32;
        for (side, want) in [(1, 1.), (16, 16.), (slot_side, room), (slot_side * 4, room)] {
            let picture = picture(side);
            let pictures = Pictures::default();
            let drawn = pictures.slotted(Arc::clone(&picture), slot_side, 1.);
            let (x, y, w, h) = device(slot_rect(slot, drawn.decoded(), scale), scale);
            assert!(
                (w - want).abs() <= 1. && (h - want).abs() <= 1.,
                "{side}px at {scale}: draws {w}x{h} device pixels, wants {want}"
            );
            let centre = |start: f32, extent: f32| start + extent / 2.;
            let (sx, sy, _, _) = device(slot, scale);
            assert!(
                (centre(x, w) - centre(sx, room)).abs() <= 1.
                    && (centre(y, h) - centre(sy, room)).abs() <= 1.,
                "{side}px at {scale}: centred in the slot"
            );
            if side < slot_side {
                assert!(
                    (x - x.round()).abs() < 1e-3 && (y - y.round()).abs() < 1e-3,
                    "{side}px at {scale}: lands on the device grid, so one texel is one pixel"
                );
                assert!(
                    Arc::ptr_eq(&drawn, &picture),
                    "{side}px is drawn from the original"
                );
            }
            if side > slot_side {
                assert_eq!(
                    drawn.decoded(),
                    (slot_side, slot_side),
                    "{side}px at {scale}: reduced to the slot before the GPU samples it"
                );
            }
        }
    }
}

#[test]
fn fixpreview_ey526_a_wide_raster_larger_than_the_slot_fits_it_whole_and_keeps_its_aspect() {
    let slot = Bounds::new(point(px(10.), px(40.)), size(px(SLOT), px(SLOT)));
    for scale in [1f32, 2.] {
        let rect = slot_rect(slot, (96, 32), scale);
        let (w, h) = (f32::from(rect.size.width), f32::from(rect.size.height));
        assert!(
            (w - SLOT).abs() < 1e-3 && (h - SLOT / 3.).abs() < 1e-3,
            "at {scale}: {w}x{h} keeps 3:1 inside the slot"
        );
        let top = f32::from(rect.origin.y) - 40.;
        assert!(
            (top - (SLOT - h) / 2.).abs() < 1e-3,
            "at {scale}: centred down the slot, top {top}"
        );
    }
}
