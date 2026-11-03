//! Physical pixels must arrive through the production column and Grid, replacing a deliberately tiny thumbnail.
use super::previewtypes_tests::{answer, file, local, meta, open, thumbed};
use crate::{
    state::Shell,
    window::{startup::View, u07_tests::read},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) struct Root(pub(super) PathBuf);
impl Root {
    pub(super) fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(std::env::temp_dir().join("preview-quality-surface.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        std::fs::write(root.join(".flea-test-sandbox"), "PREVIEW-QUALITY").unwrap();
        image::RgbaImage::from_pixel(1200, 800, image::Rgba([40, 120, 200, 255]))
            .save(root.join("photo.png"))
            .unwrap();
        image::RgbaImage::from_pixel(48, 32, image::Rgba([240, 10, 10, 255]))
            .save(root.join("thumb.png"))
            .unwrap();
        Self(root)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(std::env::temp_dir())
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
pub(super) fn column(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<(u32, u32)> {
    shell.read_with(cx, |shell, cx| {
        let owner = shell.selection_preview(0)?;
        let view = owner.read(cx);
        let image = view.drawn.as_ref()?.image.as_ref()?;
        let size = image.size(0);
        Some((size.width.0 as u32, size.height.0 as u32))
    })
}
pub(super) fn wait(
    cx: &mut VisualTestContext,
    mut ready: impl FnMut(&mut VisualTestContext) -> bool,
) {
    let start = Instant::now();
    loop {
        draw(cx);
        if ready(cx) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "full physical pixels must replace the thumbnail"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[gpui::test]
fn preview_quality_column_replaces_upscaled_thumbnail_at_each_output_scale(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = Root::new();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("photo.png", "image-x-generic", "PNG image", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":1200,"h":800"#);
    thumbed(
        &shell,
        cx,
        &lines,
        0,
        &root.0.join("thumb.png").display().to_string(),
    );
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        wait(cx, |cx| {
            let Some(size) = column(&shell, cx) else {
                return false;
            };
            let Some(rect) = cx.debug_bounds("column-preview-image") else {
                return false;
            };
            size.0 as f32 >= f32::from(rect.size.width) * scale
                && size.1 as f32 >= f32::from(rect.size.height) * scale
        });
    }
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image");
    // The native gates read decode sizes through the registered IPC function, not the adapter alone.
    let quality = read(&shell, cx, "previewQuality", &[]);
    assert!(
        quality.starts_with(r#"{"surface":"column","kind":"image""#)
            && quality.contains(r#""refined":true"#)
            && quality.contains(r#""scale":2"#),
        "{quality}"
    );
}

#[gpui::test]
fn preview_quality_column_request_settles_after_its_refinement(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("photo.png", "image-x-generic", "PNG image", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":1200,"h":800"#);
    thumbed(
        &shell,
        cx,
        &lines,
        0,
        &root.0.join("thumb.png").display().to_string(),
    );
    // Fractional windows give the column fractional boxes, where a picture's own refinement could resize its request.
    for (width, height) in [(1001.3, 703.7), (987.6, 655.1), (1013.9, 690.45)] {
        for scale in [1.25, 1.6] {
            cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
            cx.simulate_scale_factor_change(scale);
            for _ in 0..3 {
                draw(cx);
            }
            let mut settled = String::new();
            wait(cx, |cx| {
                settled = read(&shell, cx, "previewQuality", &[]);
                settled.contains(r#""refined":true"#)
            });
            for _ in 0..6 {
                draw(cx);
            }
            assert_eq!(
                read(&shell, cx, "previewQuality", &[]),
                settled,
                "a refined column never asks again at {width}x{height} and scale {scale}"
            );
        }
    }
}

#[gpui::test]
fn preview_quality_grid_largest_replaces_the_cached_thumbnail(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("photo.png", "image-x-generic", "PNG image", 0o100644)],
        1,
    );
    shell.update(cx, |shell, _| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"preview":{"thumbnails":"media","thumbSize":"largest"}}"#)
                .unwrap();
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Grid, window, cx)));
    answer(
        &shell,
        cx,
        &format!(r#"{{"t":"fsinfo","path":"{dir}","class":""}}"#),
    );
    for _ in 0..3 {
        draw(cx);
    }
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"thumbed","row":0,"file":"{}"}}"#,
            root.0.join("thumb.png").display()
        ),
    );
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        wait(cx, |cx| {
            shell.read_with(cx, |shell, _| {
                let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
                let state = grid.state.borrow();
                state
                    .drawn_row(0)
                    .and_then(|row| row.decoded)
                    .is_some_and(|size| size.0 >= (256. * scale).ceil() as u32)
            })
        });
    }
}

#[gpui::test]
fn preview_quality_cache_only_never_reads_or_enlarges_the_original(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    cx.simulate_scale_factor_change(2.);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("photo.png", "image-x-generic", "PNG image", 0o100644)],
        1,
    );
    shell.update(cx, |shell, _| {
        shell.launch.settings.data = flea::jsondoc::parse(
            r#"{"preview":{"thumbnails":"media","thumbSize":"largest","thumbNetwork":false}}"#,
        )
        .unwrap();
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Grid, window, cx)));
    answer(
        &shell,
        cx,
        &format!(r#"{{"t":"fsinfo","path":"{dir}","class":"network"}}"#),
    );
    for _ in 0..3 {
        draw(cx);
    }
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"thumbed","row":0,"file":"{}"}}"#,
            root.0.join("thumb.png").display()
        ),
    );
    wait(cx, |cx| {
        shell.read_with(cx, |shell, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .drawn_row(0)
                .is_some_and(|row| row.decoded.is_some())
        })
    });
    for _ in 0..10 {
        draw(cx);
    }
    shell.read_with(cx, |shell, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let row = state.drawn_row(0).unwrap();
        assert_eq!(
            row.decoded,
            Some((48, 32)),
            "cache-only never decodes the original"
        );
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .pictures
                .quality_pixels(0)
                .is_none(),
            "cache-only never requests source refinement"
        );
        let picture = row.picture.unwrap();
        assert!(
            picture.width <= 24.01 && picture.height <= 16.01,
            "scale 2 keeps cache pixels at 1:1"
        );
    });
}
