//! Compressed, delayed and closing SVG reads preserve their bounds and release their tiles.
use super::*;

#[gpui::test]
fn an_unfed_fifo_named_svg_is_refused_without_blocking(cx: &mut TestAppContext) {
    let _watchdog = watchdog(100);
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
    assert!(
        raster_of(cx, &look).is_none(),
        "no raster lands for the refused FIFO"
    );
    assert!(
        !gpui_loaded(cx, &path),
        "an unfed FIFO named .svg was handed to img(path)"
    );
    // The job slot clears with the refusal: a later SVG in the same window still draws.
    look.update(cx, |look, cx| {
        look.close(cx);
        look.open(ql_svg(), true, cx);
    });
    let landed = wait_for_raster(cx, &look, None);
    assert!(
        in_atlas(cx, &landed),
        "a later SVG draws after the refused FIFO"
    );
    free_sandbox(&root);
}

#[gpui::test]
fn the_parse_reads_no_file_past_the_byte_bound(cx: &mut TestAppContext) {
    let _watchdog = watchdog(120);
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
        if current
            .as_ref()
            .is_some_and(|image| Some(image.id.0) != first)
        {
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
            drop(
                std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(flea::oflags::O_NONBLOCK)
                    .open(&feed),
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    writer.join().expect("the feed writer ends");
    let taken = fed.load(Ordering::SeqCst);
    eprintln!(
        "feed probe: svg file {svg_len} bytes; the parse took {taken} bytes; bound {MAX_SVG_BYTES}"
    );
    free_sandbox(&root);
    assert!(
        taken as u64 <= MAX_SVG_BYTES,
        "a {svg_len}-byte SVG made the parse read {taken} bytes from a file it names"
    );
}

#[gpui::test]
fn an_svgz_inflates_and_draws_its_picture(cx: &mut TestAppContext) {
    let _watchdog = watchdog(100);
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
    assert!(
        !gpui_loaded(cx, &path),
        "a .svgz, a vector, was handed to img(path)"
    );
    assert!(
        cx.debug_bounds("pv1-picture").is_some(),
        "the inflated vector draws its picture slot"
    );
    free_sandbox(&root);
}

#[gpui::test]
fn an_oversize_svgz_shows_the_too_large_card_and_loads_no_file(cx: &mut TestAppContext) {
    let _watchdog = watchdog(100);
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
    assert!(
        cx.debug_bounds("pv1-bar").is_none(),
        "the too-large card has no unreadable error bar"
    );
    assert!(
        !gpui_loaded(cx, &path),
        "a refused svgz was handed to img(path)"
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
fn a_svgz_bomb_is_refused_without_inflating_it_whole(cx: &mut TestAppContext) {
    // 3 GiB of zeros compress below the file bound to reach inflation; the take refuses in milliseconds, so the suite's one-second UI bound catches whole inflation.
    let _watchdog = watchdog(300);
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
    assert!(
        !gpui_loaded(cx, &path),
        "a refused bomb was handed to img(path)"
    );
    assert!(
        raster_of(cx, &look).is_none(),
        "no raster lands for the refused bomb"
    );
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
    assert_eq!(
        landed.size(0),
        want,
        "the late raster is drawn at the logical fit times scale"
    );
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
    let look = cx.new(|_| QuickLook::new(Arc::new(crate::test_catalogs::english())));
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
    cx.executor()
        .advance_clock(Duration::from_millis(flea::ui_logic_fb1::CLOSE_MS));
    cx.run_until_parked();
    assert!(
        !in_atlas(cx, &first),
        "close frees the tile even when the host never renders the closed card again"
    );
}
