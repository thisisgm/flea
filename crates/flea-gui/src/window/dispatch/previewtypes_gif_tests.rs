//! The column preview animates a GIF and keeps a GIF past the raster budget a still.
use super::previewtypes_kinds_tests::Root;
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::window::u07_tests::read;
use gpui::{TestAppContext, VisualTestContext};

/// A two-frame GIF, red then blue, half a second a frame.
const ANIMATED_GIF: [u8; 101] = [
    71, 73, 70, 56, 57, 97, 8, 0, 8, 0, 240, 0, 0, 255, 0, 0, 255, 255, 255, 33, 255, 11, 78, 69,
    84, 83, 67, 65, 80, 69, 50, 46, 48, 3, 1, 0, 0, 0, 33, 249, 4, 0, 50, 0, 0, 0, 44, 0, 0, 0,
    0, 8, 0, 8, 0, 0, 2, 7, 132, 143, 169, 203, 237, 93, 0, 0, 33, 249, 4, 0, 50, 0, 0, 0, 44, 0,
    0, 0, 0, 8, 0, 8, 0, 128, 0, 0, 255, 255, 255, 255, 2, 7, 132, 143, 169, 203, 237, 93, 0, 0,
    59,
];

/// A 4 by 4 PNG, the still frame a thumbnailer caches for the GIF.
const STILL_PNG: [u8; 73] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 2, 0,
    0, 0, 38, 147, 9, 41, 0, 0, 0, 16, 73, 68, 65, 84, 120, 156, 99, 56, 81, 161, 1, 71, 12, 196,
    113, 0, 83, 82, 22, 129, 25, 170, 14, 207, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Frames and real time for a picture decode, which runs on the background executor.
fn settle_pictures(cx: &mut VisualTestContext) {
    for _ in 0..40 {
        cx.executor().advance_clock(std::time::Duration::from_millis(300));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[gpui::test]
fn an_animated_gif_animates_in_the_column_and_stops_when_the_preview_leaves_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("gifanim");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("anim.gif"), ANIMATED_GIF).unwrap();
    std::fs::write(root.0.join("notes.txt"), "hello\n").unwrap();
    let still = root.0.join("still.png");
    std::fs::write(&still, STILL_PNG).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![
        file("anim.gif", "image-gif", "GIF image", 0o100644),
        file("notes.txt", "text-plain", "Plain text document", 0o100644),
    ];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","frames":4,"w":8,"h":8"#);
    answer(&shell, cx, &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, still.display()));
    settle_pictures(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image");
    let frames = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview
            .read(cx)
            .drawn
            .as_ref()
            .and_then(|frame| frame.animation.as_ref().map(|image| image.frame_count()))
    });
    assert_eq!(frames, Some(2), "the GIF's own frames load over its still frame");
    assert!(
        cx.debug_bounds("column-preview-animation").is_some(),
        "the moving layer draws in the frame"
    );
    // The moving layer takes the still's place, so it keeps the still's never-enlarge fit for a raster GIF.
    let layer = cx.debug_bounds("column-preview-animation-image").expect("the moving layer draws its image");
    assert_eq!(
        (f32::from(layer.size.width), f32::from(layer.size.height)),
        (8., 8.),
        "an 8x8 GIF moves at its own 8x8 in the wide column frame, never enlarged"
    );
    let moving = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let frame = preview.read(cx).drawn.as_ref().expect("the GIF is drawn");
        std::sync::Arc::downgrade(frame.animation.as_ref().expect("its frames are in"))
    });
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":1,"lines":[[0,"hello"]]}}"#,
            field(&code, "token")
        ),
    );
    settle_pictures(cx);
    let after = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview.read(cx).drawn.as_ref().map(|frame| {
            (
                frame.subject.row.name.clone(),
                frame.animation.as_ref().map(|image| image.frame_count()),
            )
        })
    });
    assert_eq!(
        after,
        Some(("notes.txt".into(), None)),
        "leaving the GIF stops its animation with the frame"
    );
    assert!(
        cx.debug_bounds("column-preview-animation").is_none(),
        "no moving layer draws for the next file"
    );
    assert!(
        moving.upgrade().is_none(),
        "the retired GIF frame releases its decoded frames from the asset cache"
    );
}

#[gpui::test]
fn a_gif_past_the_raster_budget_stays_a_still_in_the_column(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("gifhuge");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("anim.gif"), ANIMATED_GIF).unwrap();
    let still = root.0.join("still.png");
    std::fs::write(&still, STILL_PNG).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("anim.gif", "image-gif", "GIF image", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","frames":4,"w":4000,"h":4000"#);
    answer(&shell, cx, &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, still.display()));
    settle_pictures(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image");
    let moving = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview.read(cx).drawn.as_ref().map(|frame| frame.animation_source.is_some())
    });
    assert_eq!(moving, Some(false), "four 4000 px frames are never decoded for the column");
    assert!(cx.debug_bounds("column-preview-animation").is_none(), "the still stands alone");
}
