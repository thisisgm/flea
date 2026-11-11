//! A local JPEG reaches sharp device pixels directly in the column and after a bounded first frame in Quick Look.
use super::previewtypes_tests::{file, local, meta, open, pump, thumbed};
use crate::{state::Shell, window::u07_tests::read};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(std::env::temp_dir().join("preview-reuse.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
        std::fs::write(root.join(".flea-test-sandbox"), "PQ-PERF").unwrap();
        image::RgbImage::from_fn(1500, 1000, |x, y| {
            let v = if (x / 3 + y / 5) % 2 == 0 { 230 } else { 25 };
            image::Rgb([v, 255 - v, (y % 256) as u8])
        })
        .save(root.join("photo.jpg"))
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
fn quality(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    read(shell, cx, "previewQuality", &[])
}
fn wait(shell: &Entity<Shell>, cx: &mut VisualTestContext, want: &str) -> String {
    let start = Instant::now();
    loop {
        draw(cx);
        let answer = quality(shell, cx);
        if answer.contains(want) {
            return answer;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let state = read(shell, cx, "previewState", &[]);
            panic!("{answer} state={state}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
// Sample input: {"decodes":1}
fn decode_count(answer: &str) -> Option<f64> {
    flea::jsondoc::parse(answer).ok()?.get("decodes")?.as_f64()
}

#[test]
fn preview_quality_reuse_decode_count_rejects_numeric_prefixes() {
    assert_eq!(decode_count(r#"{"decodes":1}"#), Some(1.));
    assert_ne!(decode_count(r#"{"decodes":10}"#), Some(1.));
}

fn landed(cx: &mut TestAppContext) -> (Root, Entity<Shell>, &mut VisualTestContext, Vec<String>) {
    cx.executor().allow_parking();
    let root = Root::new();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("photo.jpg", "image-jpeg", "JPEG image", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":1500,"h":1000"#);
    (root, shell, cx, lines)
}

#[gpui::test]
fn preview_quality_reuse_column_jpeg_draws_sharp_before_any_thumbnail(cx: &mut TestAppContext) {
    let (root, shell, cx, lines) = landed(cx);
    // No thumbed reply yet: the frame lands on the refinement the meta reply started.
    let first = wait(&shell, cx, r#""refined":true"#);
    assert!(
        first.starts_with(r#"{"surface":"column","kind":"image""#)
            && decode_count(&first) == Some(1.),
        "{first}"
    );
    thumbed(
        &shell,
        cx,
        &lines,
        0,
        &root.0.join("thumb.png").display().to_string(),
    );
    for _ in 0..5 {
        draw(cx);
    }
    assert_eq!(
        quality(&shell, cx),
        first,
        "a late thumbnail neither replaces nor re-reads"
    );
}

#[gpui::test]
fn preview_quality_reuse_column_thumbnail_drawn_first_keeps_its_geometry(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    // A thumbnail whose rounded aspect is not the source's: the sharp picture must take the drawn one's place.
    image::RgbaImage::from_pixel(48, 31, image::Rgba([240, 10, 10, 255]))
        .save(root.0.join("wide.png"))
        .unwrap();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("photo.jpg", "image-jpeg", "JPEG image", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, &dir);
    thumbed(
        &shell,
        cx,
        &lines,
        0,
        &root.0.join("wide.png").display().to_string(),
    );
    draw(cx);
    meta(&shell, cx, &lines, r#","w":1500,"h":1000"#);
    let refined = wait(&shell, cx, r#""refined":true"#);
    let value = flea::jsondoc::parse(&refined).unwrap();
    let size = value
        .get("requested")
        .and_then(|size| size.as_array())
        .unwrap();
    let aspect = size[0].as_f64().unwrap() / size[1].as_f64().unwrap();
    assert!(
        (aspect - 48. / 31.).abs() < (aspect - 1.5).abs(),
        "the refinement keeps the drawn thumbnail's geometry: {refined}"
    );
    assert!(decode_count(&refined) == Some(1.), "{refined}");
}

#[gpui::test]
fn preview_quality_reuse_column_going_back_draws_the_kept_picture(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new();
    image::RgbImage::from_pixel(30, 20, image::Rgb([20, 200, 90]))
        .save(root.0.join("other.jpg"))
        .unwrap();
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![
        file("photo.jpg", "image-jpeg", "JPEG image", 0o100644),
        file("other.jpg", "image-jpeg", "JPEG image", 0o100644),
    ];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":1500,"h":1000"#);
    let first = wait(&shell, cx, r#""refined":true"#);
    assert!(decode_count(&first) == Some(1.), "{first}");
    super::test_shell::set_cursor(&shell, cx, 1);
    let lines = pump(cx);
    meta(&shell, cx, &lines, r#","w":30,"h":20"#);
    let start = Instant::now();
    while quality(&shell, cx) == first || !quality(&shell, cx).contains(r#""refined":true"#) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the other file never drew"
        );
        draw(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    super::test_shell::set_cursor(&shell, cx, 0);
    let lines = pump(cx);
    meta(&shell, cx, &lines, r#","w":1500,"h":1000"#);
    // Going back draws the kept sharp picture with no second read: the very same reply, decodes still 1.
    let back = wait(&shell, cx, &first);
    assert_eq!(back, first);
}

#[gpui::test]
fn preview_quality_reuse_quick_look_jpeg_ends_on_sharp_full_source_frame(cx: &mut TestAppContext) {
    use super::preview_tests::{press, sent, token};
    cx.executor().allow_parking();
    let root = Root::new();
    // Larger than the card's device fit, so the first decode is already a reduction, as for a camera photo.
    image::RgbImage::from_fn(2400, 1600, |x, y| {
        let v = if (x / 3 + y / 5) % 2 == 0 { 230 } else { 25 };
        image::Rgb([v, 255 - v, (x % 256) as u8])
    })
    .save(root.0.join("big.jpg"))
    .unwrap();
    let (shell, cx, mut requests) = super::gh_preview_proof_tests::opened_at(cx, &root.0);
    let dir = root.0.to_string_lossy().into_owned();
    let row = super::preview_tests::file("big.jpg", "image-jpeg");
    super::test_shell::land(&shell, cx, &dir, vec![row], 1);
    super::test_shell::set_cursor(&shell, cx, 0);
    press(cx, "space");
    let asks = sent(&mut requests, "meta");
    assert_eq!(asks.len(), 1, "Quick Look asks its own meta");
    let reply = format!(
        r#"{{"t":"meta","row":0,"token":{},"w":2400,"h":1600,"orient":1}}"#,
        token(&asks[0])
    );
    super::preview_tests::answer(&shell, cx, reply);
    let refined = wait(&shell, cx, r#""refined":true"#);
    for _ in 0..5 {
        draw(cx);
    }
    let settled = quality(&shell, cx);
    assert_eq!(settled, refined, "{settled}");
    // GH336 requires one bounded first frame before the single full-source sharp decode.
    assert!(
        settled.starts_with(r#"{"surface":"look","kind":"image""#)
            && decode_count(&settled) == Some(2.),
        "one bounded first frame and one sharp refinement settle without another decode: {settled}"
    );
    let look = shell.read_with(cx, |shell, _| {
        shell.quick_look.as_ref().unwrap().view.clone()
    });
    let final_frame = look.read_with(cx, |look, _| look.svg_raster()).unwrap();
    let size = final_frame.size(0);
    let target = (size.width.0 as u32, size.height.0 as u32);
    assert!(target.0 < 2400 && target.1 < 1600, "a reduced camera photo");
    assert_eq!(
        look.read_with(cx, |look, _| look.quality_pixels()),
        Some((target, true)),
        "the mounted sharp frame has the requested device pixels"
    );
    let source = image::open(root.0.join("big.jpg")).unwrap().into_rgba8();
    let mut expected = image::imageops::resize(
        &source,
        target.0,
        target.1,
        image::imageops::FilterType::CatmullRom,
    );
    for pixel in expected.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    // A separate path keeps the negative control out of the preview's decode ledger.
    let reference = root.0.join("reference.jpg");
    std::fs::copy(root.0.join("big.jpg"), &reference).unwrap();
    let bounded =
        crate::g2::quick_look_overlay::raster::run(&crate::g2::quick_look_overlay::raster::Job {
            path: reference,
            size: target,
            epoch: 1,
            latest: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            raster: std::sync::Arc::new(|_, _| panic!("JPEG must use the bounded photo decoder")),
        });
    let crate::g2::quick_look_overlay::raster::RasterOutcome::Raster(bounded) = bounded else {
        panic!("the JPEG first-frame job decodes its bounded negative control");
    };
    assert_ne!(
        bounded.as_bytes(0).unwrap(),
        expected.as_raw().as_slice(),
        "the fixture distinguishes the bounded first frame from full-source sharp pixels"
    );
    assert_eq!(
        final_frame.as_bytes(0).unwrap(),
        expected.as_raw().as_slice(),
        "Quick Look ends on the full-source sharp pixels, never the bounded first frame"
    );
}
