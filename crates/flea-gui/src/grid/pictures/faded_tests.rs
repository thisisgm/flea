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
    let faded = pictures.faded(Arc::clone(&old), 0.5);
    pictures.map.borrow_mut().remember(3, ready(&picture("b.png")));
    let map = pictures.map.borrow();
    assert_eq!(faded_count(&map), 0, "the replaced picture's faded copy leaves with it");
    assert!(map.retired.iter().any(|image| Arc::ptr_eq(image, &old.image)), "the replaced texture goes to the atlas drop");
    assert!(map.retired.iter().any(|image| Arc::ptr_eq(image, &faded.image)), "its faded texture goes too");
}

#[test]
fn a_release_leaves_no_faded_copy_behind() {
    let pictures = Pictures::default();
    let lent = picture("a.png");
    held(&pictures, 0, &lent);
    let _ = pictures.faded(Arc::clone(&lent), 0.5);
    // A copy made for a picture the map no longer holds, as a paint racing a listing change would.
    let _ = pictures.faded(picture("gone.png"), 0.5);
    let _ = pictures.map.borrow_mut().release();
    assert_eq!(faded_count(&pictures.map.borrow()), 0, "a hidden pane or a new folder keeps no faded texture");
}

#[test]
fn a_kept_faded_copy_holds_its_source_so_its_key_is_never_reused() {
    let pictures = Pictures::default();
    let source = picture("a.png");
    let key = image_key(&source.image);
    let alive = Arc::downgrade(&source.image);
    let _ = pictures.faded(source, 0.5);
    let kept = pictures.map.borrow().faded.contains_key(&key);
    assert!(!kept || alive.upgrade().is_some(), "a faded entry outliving its source lets a new picture at that address read the stale copy");
}

#[test]
fn the_cap_retires_the_oldest_faded_copy() {
    let pictures = Pictures::default();
    let first = picture("first.png");
    held(&pictures, 0, &first);
    let _ = pictures.faded(Arc::clone(&first), 0.5);
    for row in 1..=super::CAP {
        held(&pictures, row, &picture("filler.png"));
    }
    assert_eq!(faded_count(&pictures.map.borrow()), 0, "the cap's eviction retires the oldest picture's faded copy");
}
