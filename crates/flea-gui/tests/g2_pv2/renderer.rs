//! The in-process renderer on real documents: sizes, rotation, the white ground, refusals and the render thread's replies.
use super::fixture::{pixel, LANDSCAPE, LETTER, TURNED};
use super::{document, reaper_lock, root};
use flea_gui::g2::doc_preview::poppler::{
    self, raster_size, Document, Failure, MAX_EDGE, MAX_PIXELS,
};
use flea_gui::g2::doc_preview::render::{reap, Renderer, Reply};
use std::time::{Duration, Instant};

/// A renderer whose thread is reaped when the test ends, as main reaps it before the process exits.
pub(super) struct Scoped(Option<Renderer>);

impl Scoped {
    pub(super) fn spawn() -> Self {
        Scoped(Some(Renderer::spawn()))
    }
}

impl std::ops::Deref for Scoped {
    type Target = Renderer;

    fn deref(&self) -> &Renderer {
        self.0
            .as_ref()
            .expect("the renderer lives until the test ends")
    }
}

impl Drop for Scoped {
    fn drop(&mut self) {
        drop(self.0.take());
        reap();
    }
}

const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

#[test]
fn pv2_poppler_loads_in_process() {
    assert_eq!(
        poppler::available(),
        Ok(()),
        "libpoppler-glib, libgobject, libglib and libcairo load in this process"
    );
}

#[test]
fn pv2_pages_report_their_size_after_rotation() {
    let root = root();
    let path = document(&root, "three.pdf", &[LETTER, LANDSCAPE, TURNED]);
    let doc = Document::open(&path).expect("a valid document opens");
    assert_eq!(doc.page_count(), 3, "three pages");
    assert_eq!(
        doc.page_size(0),
        Some((612., 792.)),
        "a letter page is portrait"
    );
    assert_eq!(
        doc.page_size(1),
        Some((792., 612.)),
        "a landscape page is landscape"
    );
    assert_eq!(
        doc.page_size(2),
        Some((792., 612.)),
        "a page turned 90 degrees shows landscape"
    );
    assert_eq!(doc.page_size(3), None, "no page past the document");
}

#[test]
fn pv2_a_page_renders_on_white_paper_the_right_way_up() {
    let root = root();
    let path = document(&root, "three.pdf", &[LETTER, LANDSCAPE, TURNED]);
    let doc = Document::open(&path).unwrap();
    let raster = doc.render(0, 306).expect("page 1 renders");
    assert_eq!(
        (raster.width, raster.height),
        (306, 396),
        "the raster is the width asked, in the page's proportion"
    );
    assert_eq!(
        raster.bgra.len(),
        306 * 396 * 4,
        "four bytes a pixel, no row padding"
    );
    let at = |fx, fy| pixel(&raster.bgra, raster.width, raster.height, fx, fy);
    assert_eq!(at(0.25, 0.25), BLACK, "the box is at the page's top left");
    assert_eq!(
        at(0.75, 0.75),
        WHITE,
        "the paper is opaque white, the oracle's page ground"
    );
    assert_eq!(at(0.99, 0.01), WHITE, "the top right corner is paper");
    let turned = doc.render(2, 396).expect("the turned page renders");
    assert_eq!(
        (turned.width, turned.height),
        (396, 306),
        "a turned page renders landscape"
    );
    let at = |fx, fy| pixel(&turned.bgra, turned.width, turned.height, fx, fy);
    assert_eq!(
        at(0.75, 0.25),
        BLACK,
        "turned clockwise, the top left quarter shows at the top right"
    );
    assert_eq!(at(0.25, 0.25), WHITE, "and the top left is paper");
    assert!(
        doc.render(9, 300).is_none(),
        "a page past the document renders nothing"
    );
    assert!(doc.render(0, 0).is_none(), "no width renders nothing");
}

#[test]
fn pv2_unreadable_files_are_refused_before_poppler() {
    let root = root();
    let broken = root.join("broken.pdf");
    std::fs::write(&broken, b"%PDF-1.4\nthis is not a document\n").unwrap();
    assert_eq!(
        Document::open(&broken).err(),
        Some(Failure::Unreadable),
        "a malformed document is unreadable"
    );
    assert_eq!(
        Document::open(&root.join("missing.pdf")).err(),
        Some(Failure::Unreadable),
        "a missing file is unreadable"
    );
    assert_eq!(
        Document::open(&root).err(),
        Some(Failure::Unreadable),
        "a directory is unreadable"
    );
    let fifo = root.join("pipe.pdf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    let started = Instant::now();
    assert_eq!(
        Document::open(&fifo).err(),
        Some(Failure::Unreadable),
        "a fifo is refused"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "and refusing it never waits for a writer"
    );
    let link = root.join("link.pdf");
    std::os::unix::fs::symlink(document(&root, "target.pdf", &[LETTER]), &link).unwrap();
    assert_eq!(
        Document::open(&link).map(|d| d.page_count()).ok(),
        Some(1),
        "a link opens its target, as Quick Look shows it"
    );
}

#[test]
fn pv2_raster_size_holds_inside_its_caps() {
    assert_eq!(
        raster_size((612., 792.), 612),
        Some((612, 792)),
        "a page at its own width"
    );
    assert_eq!(raster_size((0., 792.), 612), None, "a page with no width");
    assert_eq!(raster_size((612., 792.), 0), None, "no width asked");
    let (w, h) = raster_size((612., 792.), 100_000).unwrap();
    assert!(
        w <= MAX_EDGE && h <= MAX_EDGE,
        "no edge past the cap: {w}x{h}"
    );
    assert!(
        u64::from(w) * u64::from(h) <= MAX_PIXELS + 2 * u64::from(MAX_EDGE),
        "no raster past the pixel cap: {w}x{h}"
    );
    let (w, h) = raster_size((10., 20000.), 400).unwrap();
    assert!(
        h <= MAX_EDGE && w >= 1,
        "a tall strip is held by its height: {w}x{h}"
    );
}

pub(super) fn next(renderer: &Renderer) -> Reply {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(reply) = renderer.replies.try_recv() {
            return reply;
        }
        assert!(
            Instant::now() < deadline,
            "the render thread answered within 20 s"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn pv2_render_thread_reads_sizes_once_and_renders_what_is_asked() {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "two.pdf", &[LETTER, LANDSCAPE]);
    let renderer = Scoped::spawn();
    let doc = renderer.open(path, true);
    match next(&renderer) {
        Reply::Opened {
            doc: opened,
            sizes,
            read,
        } => {
            assert_eq!(opened, doc, "the reply names the open");
            assert_eq!(
                (sizes.len(), read),
                (0, 0),
                "a column's sizes come after the open, in idle steps"
            );
        }
        _ => panic!("the open answers first"),
    }
    match next(&renderer) {
        Reply::Sizes {
            doc: sized,
            sizes,
            read,
        } => {
            assert_eq!((sized, read), (doc, 2), "every size is read once");
            assert_eq!(
                sizes,
                vec![(612., 792.), (792., 612.)],
                "the sizes are the page sizes"
            );
        }
        _ => panic!("the sizes answer on the idle step"),
    }
    renderer.render(doc, 1, 400);
    match next(&renderer) {
        Reply::Rendered {
            page, width, image, ..
        } => {
            assert_eq!((page, width), (1, 400), "the page and width asked");
            let size = image.size(0);
            assert_eq!(
                (size.width.0, size.height.0),
                (400, 309),
                "the image is the raster"
            );
        }
        _ => panic!("the render lands"),
    }
    renderer.render(doc, 7, 400);
    assert!(
        matches!(next(&renderer), Reply::RenderFailed { page: 7, .. }),
        "a page past the document fails alone"
    );
}

#[test]
fn pv2_render_thread_skips_a_closed_documents_work() {
    let _reaper = reaper_lock();
    let root = root();
    let a = document(&root, "a.pdf", &[LETTER; 4]);
    let b = document(&root, "b.pdf", &[LANDSCAPE]);
    let renderer = Scoped::spawn();
    let first = renderer.open(a, true);
    for page in 0..4 {
        renderer.render(first, page, 300);
    }
    renderer.close(first);
    let second = renderer.open(b, true);
    renderer.render(second, 0, 300);
    let mut rendered_first = 0;
    loop {
        match next(&renderer) {
            Reply::Rendered { doc, .. } if doc == first => rendered_first += 1,
            Reply::Rendered { doc, .. } if doc == second => break,
            Reply::OpenFailed { .. } | Reply::RenderFailed { .. } => panic!("nothing fails here"),
            _ => {}
        }
    }
    assert!(
        rendered_first < 4,
        "a closed document's queued renders are skipped, rendered {rendered_first}"
    );
    let bad = renderer.open(root.join("missing.pdf"), true);
    loop {
        match next(&renderer) {
            Reply::OpenFailed {
                doc,
                failure: Failure::Unreadable,
            } if doc == bad => break,
            Reply::Sizes { .. } | Reply::Opened { .. } => {}
            _ => panic!("a missing document fails its open"),
        }
    }
}

#[test]
fn pv2_render_thread_skips_pages_no_longer_wanted_and_reads_quick_look_sizes_after_its_open() {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "three.pdf", &[LETTER, LANDSCAPE, LANDSCAPE]);
    let renderer = Scoped::spawn();
    let doc = renderer.open(path, false);
    match next(&renderer) {
        Reply::Opened { sizes, read, .. } => {
            assert_eq!(read, 2, "Quick Look reads two sizes at the open");
            assert_eq!(
                sizes,
                vec![(612., 792.), (792., 612.), (612., 792.)],
                "the third stands in as the first page's"
            );
        }
        _ => panic!("the open answers first"),
    }
    renderer.want(doc, vec![(2, 300)]);
    renderer.render(doc, 0, 300);
    renderer.render(doc, 2, 300);
    let mut replies = Vec::new();
    while replies.len() < 2 {
        match next(&renderer) {
            Reply::Rendered { page, .. } => replies.push(format!("rendered {page}")),
            Reply::Sizes { sizes, read, .. } => {
                replies.push(format!("sizes {read} {:?}", sizes[2]))
            }
            _ => panic!("nothing fails here"),
        }
    }
    replies.sort();
    assert_eq!(
        replies,
        ["rendered 2", "sizes 1 (792.0, 612.0)"],
        "page 1 is no longer wanted, and the last size is read when idle"
    );
}

#[test]
fn pv2_quick_look_reads_two_sizes_before_its_first_render_and_the_rest_after() {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "long.pdf", &[LETTER; 2000]);
    let renderer = Scoped::spawn();
    let doc = renderer.open(path, false);
    match next(&renderer) {
        Reply::Opened { sizes, read, .. } => assert_eq!(
            (read, sizes.len()),
            (2, 2000),
            "two sizes read at the open, before any render"
        ),
        _ => panic!("the open answers first, before any size step or render"),
    }
    renderer.render(doc, 0, 300);
    let (mut rendered, mut rest) = (false, None);
    while !rendered || rest.is_none() {
        match next(&renderer) {
            Reply::Rendered { page: 0, .. } => rendered = true,
            Reply::Sizes { read, sizes, .. } => rest = Some((read, sizes.len())),
            _ => panic!("nothing else answers"),
        }
    }
    assert_eq!(
        rest,
        Some((1998, 2000)),
        "the other 1998 sizes are read once, when the thread is idle"
    );
}
