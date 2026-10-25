//! A thumbnail answer draws in place of the kind mark in List, Columns and Dual rows, and a cut row draws its faded copy.
use super::*;

fn draws_the_answer_in_place_of_the_kind_mark(cx: &mut TestAppContext, view: View) {
    let (handle, mut lines) = open(cx, view, "{}");
    let first = first_settle(cx, handle, &mut lines);
    let rows = asked(&first);
    assert!(
        rows.contains(&1) && rows.contains(&5),
        "{view:?}: the photo and the video are asked: {first:?}"
    );
    assert!(!rows.contains(&3), "{view:?}: a folder is never asked");
    assert!(
        deliver(cx, handle, &thumbed(1, PICTURE)),
        "a thumbed line is the pictures'"
    );
    assert!(deliver(cx, handle, &thumbed(5, PICTURE)));
    step(cx, handle);
    assert_eq!(
        read(cx, handle, |shell| picture_at(shell, view, 1)),
        None,
        "{view:?}: the UI thread never decodes"
    );
    cx.run_until_parked();
    step(cx, handle);
    for row in [1, 5] {
        assert_eq!(
            read(cx, handle, |shell| picture_at(shell, view, row)),
            Some((1280, 64)),
            "{view:?}: row {row} draws its thumbnail answer, not its kind glyph"
        );
    }
    for row in [0, 2, 3] {
        assert_eq!(
            read(cx, handle, |shell| picture_at(shell, view, row)),
            None,
            "{view:?}: row {row} keeps its mark"
        );
    }
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!((counters.answered, counters.decoded), (2, 2), "{view:?}");
    assert_eq!(
        read(cx, handle, |shell| painted(shell, view)),
        2,
        "{view:?}: paint drew both pictures in their mark slots"
    );
}

#[gpui::test]
fn a_list_row_draws_its_thumbnail_answer_in_place_of_the_kind_mark(cx: &mut TestAppContext) {
    draws_the_answer_in_place_of_the_kind_mark(cx, View::List);
}

#[gpui::test]
fn a_columns_row_draws_its_thumbnail_answer_in_place_of_the_kind_mark(cx: &mut TestAppContext) {
    draws_the_answer_in_place_of_the_kind_mark(cx, View::Columns);
}

#[gpui::test]
fn a_dual_pane_row_draws_its_thumbnail_answer(cx: &mut TestAppContext) {
    draws_the_answer_in_place_of_the_kind_mark(cx, View::Dual);
}

#[test]
fn a_cut_row_draws_its_picture_faded_once_and_reuses_the_copy() {
    use crate::grid::element::{Identity, PictureKind, Thumbnail};
    let texels = vec![10, 20, 30, 255, 40, 50, 60, 100];
    let buffer = image::RgbaImage::from_raw(2, 1, texels).unwrap();
    let picture = std::sync::Arc::new(Thumbnail {
        identity: Identity {
            name: "photo.png".into(),
            size: 101,
            mtime: 1_700_000_000,
        },
        kind: PictureKind::Image,
        image: std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)])),
    });
    let pictures = Pictures::default();
    let faded = pictures.faded(std::sync::Arc::clone(&picture), 0.5);
    assert_eq!(
        faded.image.as_bytes(0).unwrap(),
        &[10, 20, 30, 128, 40, 50, 60, 50],
        "only the straight alpha is scaled"
    );
    assert_eq!(
        picture.image.as_bytes(0).unwrap()[3],
        255,
        "the lent picture is untouched"
    );
    let again = pictures.faded(std::sync::Arc::clone(&picture), 0.5);
    assert!(
        std::sync::Arc::ptr_eq(&faded, &again),
        "a still frame reuses the faded copy"
    );
    assert_eq!(
        faded.identity, picture.identity,
        "the faded copy keeps the source facts"
    );
}
