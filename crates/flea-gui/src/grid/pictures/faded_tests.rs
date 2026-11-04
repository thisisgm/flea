//! A cut row's faded copy lives exactly as long as the picture it was made from.
use super::{image_key, Map, Pictures, Slot};
use crate::grid::element::{Identity, PictureKind, Thumbnail};
use gpui::RenderImage;
use std::sync::Arc;

fn picture(name: &str) -> Arc<Thumbnail> {
    let buffer = image::RgbaImage::from_raw(1, 1, vec![10, 20, 30, 255]).unwrap();
    Arc::new(Thumbnail {
        identity: Identity {
            name: name.into(),
            size: 1,
            mtime: 1,
        },
        kind: PictureKind::Image,
        image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
    })
}

fn ready(picture: &Arc<Thumbnail>) -> Slot {
    Slot::Ready {
        file: String::new(),
        picture: Arc::clone(picture),
        image: true,
    }
}

fn held(pictures: &Pictures, row: usize, picture: &Arc<Thumbnail>) {
    pictures.map.borrow_mut().remember(row, ready(picture));
}

fn faded_count(map: &Map) -> usize {
    map.faded.len()
}

#[test]
fn a_ready_slot_replaced_in_place_retires_its_faded_copy() {
    let pictures = Pictures::default();
    let old = picture("a.png");
    held(&pictures, 3, &old);
    let faded = pictures.slotted(Arc::clone(&old), 64, 0.5);
    pictures
        .map
        .borrow_mut()
        .remember(3, ready(&picture("b.png")));
    let map = pictures.map.borrow();
    assert_eq!(
        faded_count(&map),
        0,
        "the replaced picture's faded copy leaves with it"
    );
    assert!(
        map.retired
            .iter()
            .any(|image| Arc::ptr_eq(image, &old.image)),
        "the replaced texture goes to the atlas drop"
    );
    assert!(
        map.retired
            .iter()
            .any(|image| Arc::ptr_eq(image, &faded.image)),
        "its faded texture goes too"
    );
}

#[test]
fn a_release_leaves_no_faded_copy_behind() {
    let pictures = Pictures::default();
    let lent = picture("a.png");
    held(&pictures, 0, &lent);
    let _ = pictures.slotted(Arc::clone(&lent), 64, 0.5);
    // A copy made for a picture the map no longer holds, as a paint racing a listing change would.
    let _ = pictures.slotted(picture("gone.png"), 64, 0.5);
    let _ = pictures.map.borrow_mut().release();
    assert_eq!(
        faded_count(&pictures.map.borrow()),
        0,
        "a hidden pane or a new folder keeps no faded texture"
    );
}

#[test]
fn a_kept_faded_copy_holds_its_source_so_its_key_is_never_reused() {
    let pictures = Pictures::default();
    let source = picture("a.png");
    let key = image_key(&source.image);
    let alive = Arc::downgrade(&source.image);
    let _ = pictures.slotted(source, 64, 0.5);
    let kept = pictures.map.borrow().faded.contains_key(&key);
    assert!(
        !kept || alive.upgrade().is_some(),
        "a faded entry outliving its source lets a new picture at that address read the stale copy"
    );
}

#[test]
fn the_cap_retires_the_oldest_faded_copy() {
    let pictures = Pictures::default();
    let first = picture("first.png");
    held(&pictures, 0, &first);
    let _ = pictures.slotted(Arc::clone(&first), 64, 0.5);
    for row in 1..=super::CAP {
        held(&pictures, row, &picture("filler.png"));
    }
    assert_eq!(
        faded_count(&pictures.map.borrow()),
        0,
        "the cap's eviction retires the oldest picture's faded copy"
    );
}

// EY-411: a 256 px thumbnail drawn in a 20 px mark slot is filtered down on the CPU, so the GPU's linear sampler never steps it.
#[test]
fn a_picture_larger_than_its_mark_slot_is_filtered_to_the_slot_device_side() {
    // One-texel black and white columns: a nearest or two-tap sample keeps hard steps, a Catmull-Rom reduction lands near grey.
    let texels: Vec<u8> = (0..256 * 128)
        .flat_map(|at| {
            let level = if at % 2 == 0 { 0 } else { 255 };
            [level, level, level, 255]
        })
        .collect();
    let buffer = image::RgbaImage::from_raw(256, 128, texels).unwrap();
    let wide = Arc::new(Thumbnail {
        identity: Identity {
            name: "clip.mp4".into(),
            size: 1,
            mtime: 1,
        },
        kind: PictureKind::Image,
        image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
    });
    let pictures = Pictures::default();
    held(&pictures, 0, &wide);
    let slotted = pictures.slotted(Arc::clone(&wide), 20, 1.);
    assert_eq!(
        slotted.decoded(),
        (20, 10),
        "fitted whole inside the 20 px device side"
    );
    let bytes = slotted.image.as_bytes(0).unwrap();
    let (low, high) = bytes
        .chunks(4)
        .map(|texel| texel[0])
        .fold((255, 0), |(low, high), level| {
            (low.min(level), high.max(level))
        });
    assert!(
        low >= 96 && high <= 160,
        "every texel blends the stripes it covers, never a hard step: {low}..{high}"
    );
    assert!(
        Arc::ptr_eq(&slotted, &pictures.slotted(Arc::clone(&wide), 20, 1.)),
        "a still frame reuses the filtered copy"
    );
    let small = picture("small.png");
    assert!(
        Arc::ptr_eq(&pictures.slotted(Arc::clone(&small), 20, 1.), &small),
        "a picture at or below its slot is drawn as decoded"
    );
    pictures
        .map
        .borrow_mut()
        .remember(0, ready(&picture("b.png")));
    assert!(
        pictures
            .map
            .borrow()
            .retired
            .iter()
            .any(|image| Arc::ptr_eq(image, &slotted.image)),
        "the filtered copy is retired with its source"
    );
}

#[test]
fn slot_reduction_preserves_bgra_channels_and_cut_alpha() {
    let pixel = [9, 31, 211, 239];
    let buffer = image::RgbaImage::from_raw(256, 128, pixel.repeat(256 * 128)).unwrap();
    let source = Arc::new(Thumbnail {
        identity: Identity {
            name: "red.png".into(),
            size: 1,
            mtime: 1,
        },
        kind: PictureKind::Image,
        image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
    });
    let pictures = Pictures::default();
    held(&pictures, 0, &source);
    for (level, expected) in [(1., pixel), (0.5, [9, 31, 211, 120])] {
        let reduced = pictures.slotted(Arc::clone(&source), 20, level);
        assert_eq!(reduced.decoded(), (20, 10));
        assert!(reduced
            .image
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|got| got == &expected));
    }
}
