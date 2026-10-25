//! #336's SVG raster guarantees: bounded, off the UI thread, tile-freeing, never a vector to img(path).
use super::boards::rect;
use super::scenes::ql_svg;
use super::*;
use flea::ui_logic_pv1::pixels::MAX_SVG_BYTES;
use gpui::{
    px, size, DevicePixels, Entity, ImgResourceLoader, RenderImage, Resource, SvgSize, TestAppContext,
    VisualTestContext, Window,
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

// A test that blocks past this ends the whole process, so a hang never holds the broker's lock.
pub(super) fn watchdog(secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        eprintln!("svg test watchdog: {secs} s passed, exiting 97");
        std::process::exit(97);
    });
}

fn mkfifo(path: &Path) {
    assert!(
        std::process::Command::new("mkfifo").arg("--").arg(path).status().unwrap().success(),
        "mkfifo makes the fixture"
    );
}

pub(super) fn raster_of(cx: &VisualTestContext, look: &Entity<QuickLook>) -> Option<Arc<RenderImage>> {
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
        assert!(start.elapsed() < Duration::from_secs(30), "{selector} draws");
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
        cx.simulate_resize(size(px(1100. - step as f32 * 13.), px(700. - step as f32 * 11.)));
        let next = wait_for_raster(cx, &look, Some(prev.id.0));
        assert!(in_atlas(cx, &next), "step {step}: the new raster draws");
        assert!(!in_atlas(cx, &prev), "step {step}: the replaced raster's tile is freed");
        seen.push(next.clone());
        assert_eq!(held(cx, &seen), 1, "step {step}: one tile held");
        prev = next;
    }
    look.update(cx, |look, cx| {
        look.action("expand", cx);
    });
    let full = wait_for_raster(cx, &look, Some(prev.id.0));
    assert!(in_atlas(cx, &full), "expand: the full-window raster draws");
    assert!(!in_atlas(cx, &prev), "expand: the smaller raster's tile is freed");
    seen.push(full.clone());
    assert_eq!(held(cx, &seen), 1, "expand: one tile held");
    prev = full;
    look.update(cx, |look, cx| look.close(cx));
    cx.run_until_parked();
    assert_eq!(held(cx, &seen), 0, "close frees the held raster");
    look.update(cx, |look, cx| look.open(ql_svg(), true, cx));
    let again = wait_for_raster(cx, &look, Some(prev.id.0));
    assert!(in_atlas(cx, &again), "reopen: the new raster draws");
    assert!(!in_atlas(cx, &prev), "reopen: the closed raster's tile stays freed");
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
    assert!(raster_of(cx, &look).is_none(), "the foreground update has not stored it yet");
    let landed = wait_for_raster(cx, &look, None);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the executor rasterized the vector once"
    );
    assert!(in_atlas(cx, &landed), "the landed raster draws");
    assert_eq!(landed.id.0, drawn.id.0, "the injected raster is the one drawn");
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
    assert!(!gpui_loaded(cx, &path), "a pending SVG was handed to img(path)");
    let _ = wait_for_raster(cx, &look, None);
    assert!(!gpui_loaded(cx, &path), "a drawn SVG raster was also handed to img(path)");
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
    assert!(cx.debug_bounds("pv1-bar").is_none(), "the too-large card has no unreadable error bar");
    assert!(!gpui_loaded(cx, &path), "a refused SVG was handed to img(path)");
    assert!(raster_of(cx, &look).is_none(), "no raster lands for the refused file");
    assert!(cx.debug_bounds("pv1-picture").is_none(), "a refused file draws no picture");
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
    assert!(cx.debug_bounds("pv1-bar").is_some(), "the unreadable card keeps its error bar");
    assert!(!gpui_loaded(cx, &path), "a failed SVG was handed to img(path)");
    assert!(raster_of(cx, &look).is_none(), "a failed raster leaves no image");
    assert!(cx.debug_bounds("pv1-picture").is_none(), "a failed raster draws no picture");
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
    assert!(raster_of(cx, &look).is_none(), "no raster lands for the refused file");
    assert!(!gpui_loaded(cx, &path), "an oversize SVG was handed to img(path)");
    free_sandbox(&root);
}

#[gpui::test]
fn a_fifo_named_svg_never_reaches_the_rasterizer(cx: &mut TestAppContext) {
    watchdog(100);
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
    assert!(!gpui_loaded(cx, &path), "a FIFO named .svg was handed to img(path)");
    free_sandbox(&root);
}

#[gpui::test]
fn an_unfed_fifo_named_svg_is_refused_without_blocking(cx: &mut TestAppContext) {
    watchdog(100);
    let root = sandbox("unfed-fifo");
    let path = root.join("pipe.svg");
    mkfifo(&path);
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.close(cx);
            look.open(at(&path), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    wait_for_selector(cx, "pv1-recipe");
    assert!(raster_of(cx, &look).is_none(), "no raster lands for the refused FIFO");
    assert!(!gpui_loaded(cx, &path), "an unfed FIFO named .svg was handed to img(path)");
    // The job slot clears with the refusal: a later SVG in the same window still draws.
    look.update(cx, |look, cx| {
        look.close(cx);
        look.open(ql_svg(), true, cx);
    });
    let landed = wait_for_raster(cx, &look, None);
    assert!(in_atlas(cx, &landed), "a later SVG draws after the refused FIFO");
    free_sandbox(&root);
}

#[gpui::test]
fn the_parse_reads_no_file_past_the_byte_bound(cx: &mut TestAppContext) {
    watchdog(120);
    let root = sandbox("feed");
    let feed = root.join("feed");
    mkfifo(&feed);
    let svg = root.join("ref.svg");
    let text = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"800\" height=\"600\"><rect width=\"800\" height=\"600\" fill=\"#3366cc\"/><image x=\"0\" y=\"0\" width=\"800\" height=\"600\" xlink:href=\"{}\"/></svg>",
        feed.display()
    );
    std::fs::write(&svg, &text).expect("a referring fixture");
    let svg_len = text.len();
    let fed = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    // The writer offers 64 MiB; a write returns only once the reader took the bytes.
    let writer = {
        let (fed, done, feed) = (fed.clone(), done.clone(), feed.clone());
        std::thread::spawn(move || {
            use std::io::Write;
            if let Ok(mut pipe) = std::fs::OpenOptions::new().write(true).open(&feed) {
                let chunk = vec![0u8; 1 << 20];
                for _ in 0..64 {
                    if pipe.write_all(&chunk).is_err() {
                        break;
                    }
                    fed.fetch_add(chunk.len(), Ordering::SeqCst);
                }
            }
            done.store(true, Ordering::SeqCst);
        })
    };
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let first = look.read_with(cx, |look, _| look.svg_raster().map(|i| i.id.0));
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.close(cx);
            look.open(at(&svg), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    let start = Instant::now();
    let landed = loop {
        cx.run_until_parked();
        let current = look.read_with(cx, |look, _| look.svg_raster());
        if current.as_ref().is_some_and(|image| Some(image.id.0) != first) {
            break current;
        }
        if start.elapsed() > Duration::from_secs(5) {
            break current;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // The parse returns at once past the FIFO href, so a fresh raster lands in seconds.
    let landed = landed.expect("a fresh raster lands within a few seconds");
    assert!(in_atlas(cx, &landed), "the fresh raster is in the atlas");
    if !done.load(Ordering::SeqCst) {
        // The writer still waits in open, so a nonblocking reader pairs with it and drops at once.
        for _ in 0..50 {
            if done.load(Ordering::SeqCst) {
                break;
            }
            drop(std::fs::OpenOptions::new().read(true).custom_flags(flea::oflags::O_NONBLOCK).open(&feed));
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    writer.join().expect("the feed writer ends");
    let taken = fed.load(Ordering::SeqCst);
    eprintln!("feed probe: svg file {svg_len} bytes; the parse took {taken} bytes; bound {MAX_SVG_BYTES}");
    free_sandbox(&root);
    assert!(
        taken as u64 <= MAX_SVG_BYTES,
        "a {svg_len}-byte SVG made the parse read {taken} bytes from a file it names"
    );
}

#[gpui::test]
fn an_svgz_inflates_and_draws_its_picture(cx: &mut TestAppContext) {
    watchdog(100);
    let root = sandbox("svgz");
    let path = root.join("mark.svgz");
    let out = std::fs::File::create(&path).expect("a svgz fixture");
    assert!(
        std::process::Command::new("gzip")
            .arg("-c")
            .arg("-n")
            .arg("--")
            .arg(repo("docs/images/glyphs.svg"))
            .stdout(out)
            .status()
            .expect("gzip runs")
            .success(),
        "gzip compresses the fixture"
    );
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.close(cx);
            look.open(at(&path), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    let landed = wait_for_raster(cx, &look, None);
    assert!(in_atlas(cx, &landed), "the inflated vector draws");
    assert!(!gpui_loaded(cx, &path), "a .svgz, a vector, was handed to img(path)");
    assert!(
        cx.debug_bounds("pv1-picture").is_some(),
        "the inflated vector draws its picture slot"
    );
    free_sandbox(&root);
}

#[gpui::test]
fn an_oversize_svgz_shows_the_too_large_card_and_loads_no_file(cx: &mut TestAppContext) {
    watchdog(100);
    let root = sandbox("svgz-over");
    let plain = root.join("over.svg");
    std::fs::write(&plain, oversize_svg()).expect("an oversize fixture");
    let path = root.join("over.svgz");
    let out = std::fs::File::create(&path).expect("a svgz fixture");
    assert!(
        std::process::Command::new("gzip")
            .arg("-c")
            .arg("-n")
            .arg("--")
            .arg(&plain)
            .stdout(out)
            .status()
            .expect("gzip runs")
            .success(),
        "gzip compresses the oversize fixture"
    );
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |_bytes, _size| {
                heard.fetch_add(1, Ordering::SeqCst);
                Err("the inflate bound should refuse the file first".into())
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
        "the inflate bound refused the file before the rasterizer"
    );
    assert!(cx.debug_bounds("pv1-bar").is_none(), "the too-large card has no unreadable error bar");
    assert!(!gpui_loaded(cx, &path), "a refused svgz was handed to img(path)");
    assert!(raster_of(cx, &look).is_none(), "no raster lands for the refused file");
    assert!(cx.debug_bounds("pv1-picture").is_none(), "a refused file draws no picture");
    free_sandbox(&root);
}

#[gpui::test]
fn a_svgz_bomb_is_refused_without_inflating_it_whole(cx: &mut TestAppContext) {
    // 3 GiB of zeros compress below the file bound to reach inflation; the take refuses in milliseconds, so the suite's one-second UI bound catches whole inflation.
    watchdog(300);
    let root = sandbox("svgz-bomb");
    let path = root.join("bomb.svgz");
    {
        use std::io::Write;
        let out = std::fs::File::create(&path).expect("a bomb fixture");
        let mut enc = flate2::write::GzEncoder::new(out, flate2::Compression::best());
        let zeros = vec![0u8; 1 << 20];
        for _ in 0..3072 {
            enc.write_all(&zeros).expect("zeros compress");
        }
        enc.finish().expect("a finished bomb");
    }
    let compressed = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    eprintln!("bomb: {compressed} compressed bytes inflate past the {MAX_SVG_BYTES}-byte bound");
    assert!(
        compressed < MAX_SVG_BYTES,
        "the bomb stays under the file bound so the open reaches the inflate"
    );
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    let start = Instant::now();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |_bytes, _size| {
                heard.fetch_add(1, Ordering::SeqCst);
                Err("the inflate bound should refuse the bomb first".into())
            }));
            look.close(cx);
            look.open(at(&path), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    wait_for_selector(cx, "pv1-recipe");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "the bomb was refused in {:?}, not inflated whole",
        start.elapsed()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the inflate bound refused the bomb before the rasterizer"
    );
    assert!(!gpui_loaded(cx, &path), "a refused bomb was handed to img(path)");
    assert!(raster_of(cx, &look).is_none(), "no raster lands for the refused bomb");
    free_sandbox(&root);
}

#[gpui::test]
fn a_raster_past_one_second_is_drawn(cx: &mut TestAppContext) {
    let (cx, _, look, _) = host(cx, ql_svg(), Arm::Fix);
    let first = wait_for_raster(cx, &look, None);
    let renderer = cx.update(|_, cx| cx.svg_renderer());
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    look.update(cx, |look, cx| {
        look.set_svg_raster(Arc::new(move |bytes, (w, h)| {
            // The rasterizer sleeps 1050 ms; the late raster still lands and draws.
            std::thread::sleep(Duration::from_millis(1050));
            heard.fetch_add(1, Ordering::SeqCst);
            let parsed = renderer.parse_svg(bytes).map_err(|e| e.to_string())?;
            let at = size(DevicePixels(w as i32), DevicePixels(h as i32));
            renderer
                .render_parsed(&parsed, SvgSize::ExactSize(at))
                .map_err(|e| e.to_string())
        }));
        look.close(cx);
        look.open(ql_svg(), true, cx);
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let landed = wait_for_raster(cx, &look, Some(first.id.0));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the rasterizer ran once");
    let (_, _, w, h) = rect(cx, "pv1-picture");
    let want = size(
        DevicePixels((f64::from(w) * 2.).ceil() as i32),
        DevicePixels((f64::from(h) * 2.).ceil() as i32),
    );
    assert_eq!(landed.size(0), want, "the late raster is drawn at the logical fit times scale");
    assert!(in_atlas(cx, &landed), "the late raster holds its tile");
}

struct Mount {
    look: Entity<QuickLook>,
}

impl Render for Mount {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().size_full().relative();
        if self.look.read(cx).is_open() {
            root = root.child(self.look.clone());
        }
        root
    }
}

fn card(cx: &mut Context<Mount>) -> Entity<QuickLook> {
    let look = cx.new(|_| QuickLook::new(Arc::new(flea::i18n::I18n::default())));
    cx.observe(&look, |_, _, cx| cx.notify()).detach();
    look
}

#[gpui::test]
fn a_closed_svg_frees_its_tile_with_a_host_that_unmounts_the_card(cx: &mut TestAppContext) {
    let (mount, cx) = cx.add_window_view(|_, cx| Mount { look: card(cx) });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let look = mount.read_with(cx, |mount, _| mount.look.clone());
    look.update(cx, |look, cx| look.open(ql_svg(), true, cx));
    let first = wait_for_raster(cx, &look, None);
    assert!(in_atlas(cx, &first), "the raster draws");
    look.update(cx, |look, cx| look.close(cx));
    cx.run_until_parked();
    assert!(
        !in_atlas(cx, &first),
        "close frees the tile even when the host never renders the closed card again"
    );
}
