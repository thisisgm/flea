use super::*;

pub(super) const LETTER: (f64, f64) = (612., 792.);
pub(super) const LANDSCAPE: (f64, f64) = (792., 612.);

pub(super) fn image() -> Arc<RenderImage> {
    let buffer = image::RgbaImage::from_raw(1, 1, vec![255; 4]).unwrap();
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

pub(super) fn renders(commands: &[Command]) -> Vec<(usize, u32)> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::Render { page, width, .. } => Some((*page, *width)),
            _ => None,
        })
        .collect()
}

// An idle press on `path` opened on the next frame as document `doc`, its sizes read and its frame laid out.
pub(super) fn opened(
    continuous: bool,
    path: &str,
    doc: DocId,
    sizes: Vec<(f64, f64)>,
    frame: (f64, f64),
) -> Surface {
    let mut s = Surface::new(continuous, 66, 100);
    s.set_active(true, 0);
    s.set_path(path, 0);
    s.frame();
    assert_eq!(
        s.take_commands(),
        vec![Command::Open(PathBuf::from(path))],
        "the frame opens the press"
    );
    s.opening(doc);
    s.set_frame(frame.0, frame.1, 1., 9.);
    s.reply(
        Reply::Opened {
            doc,
            read: sizes.len(),
            sizes,
        },
        0,
    );
    s
}

pub(super) fn land(s: &mut Surface, doc: DocId, page: usize, width: u32, now: u64) {
    s.reply(
        Reply::Rendered {
            doc,
            page,
            width,
            image: image(),
            took: Duration::from_millis(5),
        },
        now,
    );
}

#[test]
fn single_page_renders_its_neighbours_at_their_own_widths() {
    let mut s = opened(
        false,
        "/d/a.pdf",
        1,
        vec![LETTER, LANDSCAPE, LETTER],
        (600., 600.),
    );
    let asked = renders(&s.take_commands());
    let portrait = (600. * 612. / 792.0_f64).round() as u32;
    assert_eq!(
        asked,
        vec![(0, portrait), (1, 600)],
        "the page in view first, then the next page fitted as itself"
    );
    assert!(s.render_busy(), "the page in view is rendering");
    land(&mut s, 1, 1, 600, 3);
    assert!(s.render_busy(), "a neighbour landing never frees the press");
    assert_eq!(
        s.shown_page(),
        None,
        "and never shows before the page in view"
    );
    land(&mut s, 1, 0, portrait, 5);
    assert_eq!(
        (s.shown_page(), s.render_busy()),
        (Some(0), false),
        "the page in view shows when it lands"
    );
    assert!(s.single_image().is_some());
}

#[test]
fn a_turn_to_a_prefetched_page_shows_it_at_once_and_renders_nothing() {
    let mut s = opened(
        false,
        "/d/a.pdf",
        1,
        vec![LETTER, LANDSCAPE, LETTER],
        (600., 600.),
    );
    for (page, width) in renders(&s.take_commands()) {
        land(&mut s, 1, page, width, 5);
    }
    assert!(
        renders(&s.take_commands()).is_empty(),
        "page 1 and page 2 are held"
    );
    s.turn(1, 10);
    assert_eq!(
        (s.shown_page(), s.cap.fell_back, s.drawn_page()),
        (Some(1), false, 1),
        "the turn shows page 2 in the same call"
    );
    let portrait = (600. * 612. / 792.0_f64).round() as u32;
    assert_eq!(
        renders(&s.take_commands()),
        vec![(2, portrait)],
        "only page 3 renders ahead, page 1 is still held"
    );
    assert!(s.single_image().is_some());
    assert_eq!(s.render_width, 600, "the landscape page fills the width");
}

#[test]
fn a_turn_to_an_unrendered_page_falls_back_at_the_cap_and_holds_the_minimum_show() {
    let pages = vec![LETTER; 6];
    let mut s = opened(false, "/d/a.pdf", 1, pages, (600., 600.));
    let width = s.render_width;
    land(&mut s, 1, 0, width, 5);
    s.take_commands();
    s.turn(3, 10);
    let asked = renders(&s.take_commands());
    assert!(
        asked.contains(&(3, width)),
        "the far page renders: {asked:?}"
    );
    assert_eq!(
        s.shown_page(),
        Some(0),
        "the old page stays while the new one renders"
    );
    assert_eq!(s.drawn_page(), 0, "and the counter names the page drawn");
    assert_eq!(
        s.next_deadline(),
        Some(76),
        "the cap is due 66 ms after the turn"
    );
    assert!(s.tick(76), "the cap falls back");
    assert!(
        s.cap.fell_back && s.single_image().is_none(),
        "the loading state stands in for a stale page"
    );
    land(&mut s, 1, 3, width, 90);
    assert!(s.cap.fell_back, "the minimum show holds the loading state");
    assert!(s.tick(176), "until it ends");
    assert_eq!((s.cap.fell_back, s.shown_page()), (false, Some(3)));
}

#[test]
fn a_switch_closes_the_old_document_and_releases_its_late_pages() {
    let mut s = opened(false, "/d/a.pdf", 1, vec![LETTER, LETTER], (600., 600.));
    let width = s.render_width;
    land(&mut s, 1, 0, width, 5);
    s.take_commands();
    s.set_path("/d/b.pdf", 500);
    s.frame();
    let commands = s.take_commands();
    assert_eq!(
        commands,
        vec![Command::Close(1), Command::Open(PathBuf::from("/d/b.pdf"))],
        "the old document closes first"
    );
    assert!(
        s.cache.is_empty() && s.shown_page().is_none(),
        "the old page is never drawn for the new document"
    );
    assert_eq!(
        s.cache.take_released().len(),
        1,
        "its image leaves the atlas"
    );
    s.opening(2);
    land(&mut s, 1, 1, width, 510);
    assert!(
        s.cache.is_empty(),
        "a late page of the old document is not held"
    );
    assert_eq!(s.cache.take_released().len(), 1, "and is released");
    assert_eq!(
        s.drawn_page(),
        0,
        "the new document starts at its first page"
    );
}

#[test]
fn stack_reads_sizes_once_and_refines_on_each_physical_resize() {
    let mut s = opened(true, "/d/big.pdf", 1, vec![LETTER; 500], (600., 800.));
    assert_eq!(s.counters.size_reads, 500, "500 sizes at the open");
    let first = renders(&s.take_commands());
    assert!(
        !first.is_empty() && first.len() <= 4,
        "only pages near the frame render: {first:?}"
    );
    assert!(first.iter().all(|&(_, w)| w == 600), "at the frame's width");
    s.set_frame(570., 800., 1., 9.);
    assert_eq!(
        s.render_width, 570,
        "a 5% narrower frame asks for its actual physical width"
    );
    let refined = renders(&s.take_commands());
    assert!(
        !refined.is_empty() && refined.iter().all(|&(_, width)| width == 570),
        "the resize renders again, at only the current physical width: {refined:?}"
    );
    s.set_frame(400., 800., 1., 9.);
    assert_eq!(s.render_width, 400, "a third narrower frame renders again");
    assert_eq!(s.counters.size_reads, 500, "without reading a size again");
    s.scroll_to(1e9);
    assert_eq!(
        s.view_top,
        s.layout.height - 800.,
        "the end of the stack shows the last page's foot"
    );
    assert_eq!(
        s.page,
        s.layout.page_at(s.view_top),
        "the page is the one under the frame's top"
    );
    assert_eq!(
        s.slots.last().map(|slot| slot.page),
        Some(499),
        "the last page is built"
    );
    let (first, last) = s.layout.near(s.view_top, 800.).unwrap();
    assert!(
        s.slots
            .iter()
            .all(|slot| (first..=last).contains(&slot.page) || slot.rendering),
        "a slot outside the near window lives only while its render is out"
    );
    assert_eq!(
        s.slots
            .iter()
            .filter(|slot| (first..=last).contains(&slot.page))
            .count(),
        last - first + 1
    );
    assert!(
        s.slots.len() <= 6 && s.cache.len() <= s.slots.len() + 2,
        "and it stays a few pages: {}",
        s.slots.len()
    );
}

#[test]
fn stack_fails_only_when_its_top_page_fails_before_the_first_draw() {
    let mut s = opened(true, "/d/a.pdf", 1, vec![LETTER; 3], (600., 2000.));
    let asked = renders(&s.take_commands());
    assert!(asked.len() >= 2);
    s.reply(
        Reply::RenderFailed {
            doc: 1,
            page: 1,
            width: 600,
        },
        5,
    );
    assert!(
        !s.failed(),
        "a later page failing settles its slot and leaves the column standing"
    );
    land(&mut s, 1, 0, 600, 6);
    land(&mut s, 1, 2, 600, 7);
    assert_eq!(
        s.shown_page(),
        Some(0),
        "the first draw waits for every page in view to settle"
    );
    let mut t = opened(true, "/d/b.pdf", 2, vec![LETTER; 3], (600., 2000.));
    t.take_commands();
    t.reply(
        Reply::RenderFailed {
            doc: 2,
            page: 0,
            width: 600,
        },
        5,
    );
    assert!(
        t.failed(),
        "the top page failing before the first draw fails the column"
    );
    t.set_path("/d/c.pdf", 500);
    t.frame();
    assert!(
        t.failed(),
        "with renders in flight the switch waits for the settle"
    );
    assert!(t.tick(500 + flea::ui_logic_pv2::pdf::DOCUMENT_SETTLE_MS));
    assert!(!t.failed(), "a switch clears the failure");
}

#[test]
fn an_open_failure_fails_the_surface_and_a_stale_one_does_not() {
    let mut s = opened(false, "/d/a.pdf", 1, vec![LETTER], (600., 600.));
    s.reply(
        Reply::OpenFailed {
            doc: 9,
            failure: super::super::poppler::Failure::Unreadable,
        },
        1,
    );
    assert!(!s.failed(), "another document's failure is not this one's");
    s.reply(
        Reply::OpenFailed {
            doc: 1,
            failure: super::super::poppler::Failure::Unreadable,
        },
        1,
    );
    assert!(s.failed());
}
