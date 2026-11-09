//! A thumbnail answer draws in place of the kind mark in List, Columns and Dual rows, and a cut row draws its faded copy.
use super::*;

// The 1280 by 64 answer must fill the actual painted mark slot at device scale, preserving its aspect.
fn slot_size(cx: &mut TestAppContext, handle: WindowHandle<Shell>, view: View) -> (u32, u32) {
    handle
        .update(cx, |shell, window, _| {
            let metrics = shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .file_metrics();
            let side = match view {
                View::Columns => {
                    crate::columns::geometry::picture_box(crate::list::pane::ROW_HEIGHT).2
                }
                View::Dual => metrics.mark,
                _ => metrics.slot,
            };
            let width = (side * window.scale_factor()).ceil() as u32;
            (
                width,
                ((64. * f64::from(width) / 1280.).round() as u32).max(1),
            )
        })
        .unwrap()
}

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
    let expected = slot_size(cx, handle, view);
    for row in [1, 5] {
        let drawn = read(cx, handle, |shell| picture_at(shell, view, row));
        assert_eq!(
            drawn, Some(expected),
            "{view:?}: row {row} draws its thumbnail answer filtered to its mark slot, not its kind glyph: {drawn:?}"
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
    let faded = pictures.slotted(std::sync::Arc::clone(&picture), 64, 0.5);
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
    let again = pictures.slotted(std::sync::Arc::clone(&picture), 64, 0.5);
    assert!(
        std::sync::Arc::ptr_eq(&faded, &again),
        "a still frame reuses the faded copy"
    );
    assert_eq!(
        faded.identity, picture.identity,
        "the faded copy keeps the source facts"
    );
}

// EY-298 (b): the preview column's thumb for the cursor row shares its answer, so the row still draws its own thumbnail.
#[gpui::test]
fn a_columns_cursor_row_draws_its_thumbnail_beside_the_preview(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::Columns, "{}");
    first_settle(cx, handle, &mut lines);
    handle
        .update(cx, |shell, _, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 1;
            host.selected = vec![1];
            cx.notify();
        })
        .unwrap();
    // The column waits on the folder's class before it follows the cursor, as PreviewColumn.qml does.
    let class = Backend::test_reply_line(
        r#"{"t":"fsinfo","path":"/files","fs":"tmpfs","dev":1,"total":100,"free":90,"class":"local"}"#,
    );
    for _ in 0..2 {
        settle(cx, handle, 300);
    }
    handle
        .update(cx, |shell, window, cx| shell.receive(0, class, window, cx))
        .unwrap();
    for _ in 0..3 {
        settle(cx, handle, 300);
    }
    let previewed = read(cx, handle, |shell| {
        shell.panes[0].as_ref().unwrap().preview.clone()
    })
    .expect("the Columns preview column is mounted");
    let subject = cx.read(|cx| {
        let view = previewed.read(cx);
        view.pending
            .as_ref()
            .or(view.drawn.as_ref())
            .map(|frame| frame.subject.index)
    });
    assert_eq!(subject, Some(1), "the preview follows the cursor row");
    let event = Backend::test_reply_line(&thumbed(1, PICTURE));
    handle
        .update(cx, |shell, window, cx| shell.receive(0, event, window, cx))
        .unwrap();
    cx.run_until_parked();
    step(cx, handle);
    cx.run_until_parked();
    step(cx, handle);
    let drawn = read(cx, handle, |shell| picture_at(shell, View::Columns, 1));
    assert_eq!(
        drawn,
        Some(slot_size(cx, handle, View::Columns)),
        "the cursor row draws its thumbnail, not its kind glyph: {drawn:?}"
    );
}
