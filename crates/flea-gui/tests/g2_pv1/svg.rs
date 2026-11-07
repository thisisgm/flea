//! #336's SVG raster guarantees: bounded, off the UI thread, tile-freeing, never a vector to img(path).
use super::boards::rect;
use super::scenes::ql_svg;
use super::*;
use flea::ui_logic_pv1::pixels::MAX_SVG_BYTES;
use gpui::{
    px, size, DevicePixels, Entity, ImgResourceLoader, RenderImage, Resource, SvgSize,
    TestAppContext, VisualTestContext, Window,
};
use std::{
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) use super::watchdog::watchdog;

fn mkfifo(path: &Path) {
    assert!(
        std::process::Command::new("mkfifo")
            .arg("--")
            .arg(path)
            .status()
            .unwrap()
            .success(),
        "mkfifo makes the fixture"
    );
}

pub(super) fn raster_of(
    cx: &VisualTestContext,
    look: &Entity<QuickLook>,
) -> Option<Arc<RenderImage>> {
    look.read_with(cx, |look, _| look.svg_raster())
}

fn in_atlas(cx: &mut VisualTestContext, image: &Arc<RenderImage>) -> bool {
    cx.update(|window, _| window.has_image_atlas_entry(image))
}

/// How many of the tracked rasters hold a sprite-atlas tile right now.
fn held(cx: &mut VisualTestContext, seen: &[Arc<RenderImage>]) -> usize {
    seen.iter()
        .filter(|image| cx.update(|window, _| window.has_image_atlas_entry(image)))
        .count()
}

/// Whether GPUI's asset cache holds the file: the probe that catches a vector handed to img(path).
pub(super) fn gpui_loaded(cx: &mut VisualTestContext, path: &Path) -> bool {
    let res = Resource::Path(Arc::from(path));
    cx.update(|_, cx| cx.has_asset::<ImgResourceLoader>(&res))
}

/// Waits for a raster whose id is not `not`, draining the executors between polls.
pub(super) fn wait_for_raster(
    cx: &mut VisualTestContext,
    look: &Entity<QuickLook>,
    not: Option<usize>,
) -> Arc<RenderImage> {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        if let Some(image) = raster_of(cx, look) {
            if not != Some(image.id.0) {
                return image;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(30), "a raster lands");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits for a drawn element, draining the executors between polls.
pub(super) fn wait_for_selector(cx: &mut VisualTestContext, selector: &'static str) {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        if cx.debug_bounds(selector).is_some() {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{selector} draws"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The unit's marked scratch parent; tests run on West too, so it is created when missing.
fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-pv1/fix8")
}

/// A root of this run's own with its marker: every fixture and delete stays inside it.
fn sandbox(tag: &str) -> PathBuf {
    let parent = scratch();
    std::fs::create_dir_all(&parent).expect("the unit's fix8 root");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock")
        .as_nanos();
    let root = parent.join(format!("test-{tag}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&root).expect("a sandbox root");
    std::fs::write(root.join(".flea-test-sandbox"), "g2-pv1 svg test\n").expect("a marker");
    root
}

/// Removes only this run's marked root: absolute, non-empty, under the unit parent, marker present.
fn free_sandbox(root: &Path) {
    let parent = scratch().canonicalize().expect("the unit's fix8 root");
    let ours = root.is_absolute()
        && root.file_name().is_some()
        && root.join(".flea-test-sandbox").is_file()
        && root.parent().and_then(|path| path.canonicalize().ok()) == Some(parent);
    if ours {
        std::fs::remove_dir_all(root).expect("the sandbox root removes");
    }
}

fn at(path: &Path) -> Subject {
    let mut subject = ql_svg();
    if let Body::Image { path: slot, .. } = &mut subject.body {
        *slot = path.to_path_buf();
    }
    subject
}

fn oversize_svg() -> String {
    // A valid document padded past the byte bound inside a comment, so the parse itself is cheap.
    let head = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"600\"><rect width=\"800\" height=\"600\" fill=\"#3366cc\"/><!--";
    let tail = "--></svg>";
    let pad = MAX_SVG_BYTES as usize + 1024 - head.len() - tail.len();
    let mut text = String::with_capacity(MAX_SVG_BYTES as usize + 1024);
    text.push_str(head);
    text.extend(std::iter::repeat_n('x', pad));
    text.push_str(tail);
    text
}

#[gpui::test]
fn a_replaced_svg_raster_frees_its_atlas_tile(cx: &mut TestAppContext) {
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let first = wait_for_raster(cx, &look, None);
    assert!(in_atlas(cx, &first), "the drawn raster holds a tile");
    // The test window runs at scale 2, so the raster is exactly twice the logical fit.
    let (_, _, w, h) = rect(cx, "pv1-picture");
    let want = size(
        DevicePixels((f64::from(w) * 2.).ceil() as i32),
        DevicePixels((f64::from(h) * 2.).ceil() as i32),
    );
    assert_eq!(first.size(0), want, "the raster is twice the logical fit");
    let mut seen = vec![first.clone()];
    let mut prev = first;
    for step in 1..=20 {
        cx.simulate_resize(size(
            px(1100. - step as f32 * 13.),
            px(700. - step as f32 * 11.),
        ));
        let next = wait_for_raster(cx, &look, Some(prev.id.0));
        assert!(in_atlas(cx, &next), "step {step}: the new raster draws");
        assert!(
            !in_atlas(cx, &prev),
            "step {step}: the replaced raster's tile is freed"
        );
        seen.push(next.clone());
        assert_eq!(held(cx, &seen), 1, "step {step}: one tile held");
        prev = next;
    }
    look.update(cx, |look, cx| {
        look.action("expand", cx);
    });
    let full = wait_for_raster(cx, &look, Some(prev.id.0));
    assert!(in_atlas(cx, &full), "expand: the full-window raster draws");
    assert!(
        !in_atlas(cx, &prev),
        "expand: the smaller raster's tile is freed"
    );
    seen.push(full.clone());
    assert_eq!(held(cx, &seen), 1, "expand: one tile held");
    prev = full;
    look.update(cx, |look, cx| look.close(cx));
    cx.executor()
        .advance_clock(Duration::from_millis(flea::ui_logic_fb1::CLOSE_MS));
    cx.run_until_parked();
    assert_eq!(held(cx, &seen), 0, "close frees the held raster");
    look.update(cx, |look, cx| look.open(ql_svg(), true, cx));
    let again = wait_for_raster(cx, &look, Some(prev.id.0));
    assert!(in_atlas(cx, &again), "reopen: the new raster draws");
    assert!(
        !in_atlas(cx, &prev),
        "reopen: the closed raster's tile stays freed"
    );
    seen.push(again);
    assert_eq!(held(cx, &seen), 1, "reopen: one tile held");
}

#[gpui::test]
fn the_svg_raster_runs_off_the_ui_thread(cx: &mut TestAppContext) {
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let drawn = wait_for_raster(cx, &look, None);
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    let slow = drawn.clone();
    // One UI update and frame ask for the raster; the 400 ms work must belong to the executor after it.
    let start = Instant::now();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |_bytes, _size| {
                std::thread::sleep(Duration::from_millis(400));
                heard.fetch_add(1, Ordering::SeqCst);
                Ok(slow.clone())
            }));
            look.close(cx);
            look.open(ql_svg(), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    let frame = start.elapsed();
    assert!(
        frame < Duration::from_millis(150),
        "the UI update and frame took {frame:?}, not the 400 ms raster"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the UI frame rasterized the vector itself"
    );
    // Only background tasks run here, so a raster the foreground update ran would not have run yet.
    while cx.dispatcher.tick(true) {}
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the background executor rasterized the vector before any foreground update"
    );
    assert!(
        raster_of(cx, &look).is_none(),
        "the foreground update has not stored it yet"
    );
    let landed = wait_for_raster(cx, &look, None);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the executor rasterized the vector once"
    );
    assert!(in_atlas(cx, &landed), "the landed raster draws");
    assert_eq!(
        landed.id.0, drawn.id.0,
        "the injected raster is the one drawn"
    );
}

#[gpui::test]
fn a_pending_svg_is_never_handed_to_img_path(cx: &mut TestAppContext) {
    let path = repo("docs/images/glyphs.svg");
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let prior = wait_for_raster(cx, &look, None);
    // A slow raster keeps the next open pending while the first frame draws.
    look.update(cx, |look, cx| {
        let slow = prior.clone();
        look.set_svg_raster(Arc::new(move |_bytes, _size| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(slow.clone())
        }));
        look.close(cx);
        look.open(ql_svg(), true, cx);
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(
        !gpui_loaded(cx, &path),
        "a pending SVG was handed to img(path)"
    );
    let _ = wait_for_raster(cx, &look, None);
    assert!(
        !gpui_loaded(cx, &path),
        "a drawn SVG raster was also handed to img(path)"
    );
}

#[gpui::test]
fn a_refused_svg_shows_the_too_large_card_and_loads_no_file(cx: &mut TestAppContext) {
    let root = sandbox("refused");
    let path = root.join("over.svg");
    std::fs::write(&path, oversize_svg()).expect("an oversize fixture");
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |_bytes, _size| {
                heard.fetch_add(1, Ordering::SeqCst);
                Err("the byte bound should refuse the file first".into())
            }));
            look.close(cx);
            look.open(at(&path), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    wait_for_selector(cx, "pv1-recipe");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the byte bound refused the file before the rasterizer"
    );
    assert!(
        cx.debug_bounds("pv1-bar").is_none(),
        "the too-large card has no unreadable error bar"
    );
    assert!(
        !gpui_loaded(cx, &path),
        "a refused SVG was handed to img(path)"
    );
    assert!(
        raster_of(cx, &look).is_none(),
        "no raster lands for the refused file"
    );
    assert!(
        cx.debug_bounds("pv1-picture").is_none(),
        "a refused file draws no picture"
    );
    free_sandbox(&root);
}

#[gpui::test]
fn a_failed_svg_shows_the_unreadable_card_and_loads_no_file(cx: &mut TestAppContext) {
    let path = repo("docs/images/glyphs.svg");
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let _ = wait_for_raster(cx, &look, None);
    look.update(cx, |look, cx| {
        look.set_svg_raster(Arc::new(|_bytes, _size| Err("a parse that fails".into())));
        look.close(cx);
        look.open(ql_svg(), true, cx);
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    wait_for_selector(cx, "pv1-recipe");
    assert!(
        cx.debug_bounds("pv1-bar").is_some(),
        "the unreadable card keeps its error bar"
    );
    assert!(
        !gpui_loaded(cx, &path),
        "a failed SVG was handed to img(path)"
    );
    assert!(
        raster_of(cx, &look).is_none(),
        "a failed raster leaves no image"
    );
    assert!(
        cx.debug_bounds("pv1-picture").is_none(),
        "a failed raster draws no picture"
    );
}

#[gpui::test]
fn an_oversize_svg_never_reaches_the_rasterizer(cx: &mut TestAppContext) {
    // A sparse file over the byte bound: it takes no bytes on disk, and the bound refuses it before any read.
    let root = sandbox("oversize");
    let path = root.join("g2-pv1-oversize.svg");
    std::fs::File::create(&path)
        .and_then(|file| file.set_len(MAX_SVG_BYTES + 1))
        .expect("a sparse oversize fixture");
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |_bytes, _size| {
                heard.fetch_add(1, Ordering::SeqCst);
                Err("the byte bound should refuse the file first".into())
            }));
            look.close(cx);
            look.open(at(&path), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    while cx.dispatcher.tick(true) {}
    cx.run_until_parked();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the byte bound refused the file before the rasterizer"
    );
    assert!(
        raster_of(cx, &look).is_none(),
        "no raster lands for the refused file"
    );
    assert!(
        !gpui_loaded(cx, &path),
        "an oversize SVG was handed to img(path)"
    );
    free_sandbox(&root);
}

#[gpui::test]
fn a_fifo_named_svg_never_reaches_the_rasterizer(cx: &mut TestAppContext) {
    let _watchdog = watchdog(100);
    let root = sandbox("fifo");
    let path = root.join("pipe.svg");
    mkfifo(&path);
    let feed = path.clone();
    // Stat refuses the FIFO before any open, so the writer's nonblocking open fails at once.
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        let Ok(mut pipe) = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(flea::oflags::O_NONBLOCK)
            .open(&feed)
        else {
            return;
        };
        let chunk = vec![b' '; 1 << 20];
        for _ in 0..5 {
            if pipe.write_all(&chunk).is_err() {
                break;
            }
        }
    });
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let bytes = Arc::new(AtomicUsize::new(0));
    let heard = bytes.clone();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |data, _size| {
                heard.store(data.len(), Ordering::SeqCst);
                Err("a FIFO is never a picture".into())
            }));
            look.close(cx);
            look.open(at(&path), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    while cx.dispatcher.tick(true) {}
    cx.run_until_parked();
    writer.join().expect("the FIFO writer ends");
    wait_for_selector(cx, "pv1-recipe");
    assert_eq!(
        bytes.load(Ordering::SeqCst),
        0,
        "a FIFO named .svg reached the rasterizer with bytes, not the byte bound"
    );
    assert!(
        !gpui_loaded(cx, &path),
        "a FIFO named .svg was handed to img(path)"
    );
    free_sandbox(&root);
}

#[path = "svg_io.rs"]
mod io;
