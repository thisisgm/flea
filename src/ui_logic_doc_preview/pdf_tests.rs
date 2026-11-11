use super::*;

const GAP: f64 = 9.;

fn letter(count: usize) -> Vec<f64> {
    vec![ratio(612., 792.); count]
}

// tests/preview-pdf-flow.qml report(): twelve letter pages at the frame's inner width.
#[test]
fn pdf_layout_stacks_pages_one_gap_apart() {
    let inner = 271.;
    let layout = Layout::new(&letter(12), inner, GAP);
    let page = (inner * 792. / 612.).round();
    assert_eq!(
        layout.heights[0], page,
        "page 1 keeps the page's own proportion"
    );
    assert_eq!(layout.tops[0], 0., "page 1 starts at the frame's top");
    assert_eq!(
        layout.tops[1],
        layout.tops[0] + layout.heights[0] + GAP,
        "page 2 follows one gap under page 1"
    );
    assert_eq!(
        layout.height,
        12. * page + 11. * GAP,
        "the content is as tall as the twelve pages and eleven gaps"
    );
}

#[test]
fn pdf_layout_is_empty_without_width_or_pages() {
    assert_eq!(
        Layout::new(&letter(3), 0., GAP),
        Layout::default(),
        "no width lays out nothing"
    );
    let none = Layout::new(&[], 300., GAP);
    assert_eq!(none.height, 0., "no pages are no height");
    assert_eq!(none.near(0., 400.), None, "no pages have no near window");
    assert_eq!(none.in_view(0., 400.), 0..0, "no pages are in view");
    assert_eq!(none.turn_offset(0, 1, 400.), None, "no pages cannot turn");
}

#[test]
fn pdf_page_at_reads_the_band_under_an_offset() {
    let layout = Layout::new(&[1., 1., 1.], 100., 10.);
    assert_eq!(layout.page_at(-50.), 0, "above the stack is page 1");
    assert_eq!(layout.page_at(0.), 0, "the stack's top is page 1");
    assert_eq!(
        layout.page_at(105.),
        0,
        "the gap after a page belongs to it"
    );
    assert_eq!(layout.page_at(110.), 1, "the next page's top is that page");
    assert_eq!(layout.page_at(1000.), 2, "below the stack is the last page");
}

// A 500-page document: the slots stay with the frame however far it scrolls.
#[test]
fn pdf_near_window_stays_with_the_frame() {
    let layout = Layout::new(&letter(500), 271., GAP);
    let (first, last) = layout.near(0., 470.).expect("pages");
    assert_eq!(first, 0, "the near window starts at page 1 at the top");
    assert!(
        last <= 3,
        "it reaches only half a frame past the frame, got {last}"
    );
    let top = layout.tops[250];
    let (first, last) = layout.near(top, 470.).expect("pages");
    assert!(
        first >= 248 && last <= 254,
        "a deep offset keeps the window near the frame: {first}..{last}"
    );
    let (_, last) = layout.near(layout.height, 470.).expect("pages");
    assert_eq!(
        last, 499,
        "the window's last page is held inside the document"
    );
}

#[test]
fn pdf_turn_scrolls_to_a_page_top_inside_the_content() {
    let layout = Layout::new(&[1., 1., 1.], 100., 10.);
    assert_eq!(
        layout.turn_offset(0, 1, 50.),
        Some(110.),
        "a turn forward lands on the next page's top"
    );
    assert_eq!(
        layout.turn_offset(2, 5, 50.),
        Some(220.),
        "a turn past the end lands on the last page"
    );
    assert_eq!(
        layout.turn_offset(1, -9, 50.),
        Some(0.),
        "a turn before the start lands on page 1"
    );
    assert_eq!(
        layout.turn_offset(2, 0, 200.),
        Some(120.),
        "the last page's top is held so the frame stays full"
    );
    assert_eq!(clamp_page(3, 1, 0), 0, "an empty document stays on page 1");
}

#[test]
fn pdf_render_width_holds_through_small_resizes() {
    assert_eq!(render_width(0, 542), 542, "the first width is taken");
    assert_eq!(render_width(542, 560), 542, "a 3% change keeps the render");
    assert_eq!(render_width(542, 600), 600, "an 11% change renders again");
    assert_eq!(
        render_width(542, 480),
        480,
        "a shrink past 10% renders again"
    );
    assert_eq!(render_width(542, 0), 542, "no width keeps the render");
}

#[test]
fn pdf_single_page_fits_without_upscaling_past_the_frame() {
    assert_eq!(
        fit(400., 300., 0.5),
        (150., 300.),
        "a portrait page fills the height"
    );
    assert_eq!(
        fit(400., 300., 2.),
        (400., 200.),
        "a landscape page fills the width"
    );
    assert_eq!(aspect(612., 0.), 1., "a page with no height is square");
    assert_eq!(ratio(0., 792.), 0., "a page with no width has no ratio");
}

// tests/preview-pdf-flow.qml: the slots stay with the frame, a busy slot outlives the window, a failed page settles.
#[test]
fn pdf_slots_follow_the_near_window() {
    let mut slots = Vec::new();
    sync_slots(&mut slots, Some((0, 2)), 500);
    assert_eq!(
        slots.iter().map(|s| s.page).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "the near pages get slots"
    );
    slots[0].rendering = true;
    sync_slots(&mut slots, Some((5, 7)), 500);
    let pages: Vec<usize> = slots.iter().map(|s| s.page).collect();
    assert_eq!(
        pages,
        vec![0, 5, 6, 7],
        "a page still rendering keeps its slot until it lands"
    );
    slots[0].rendering = false;
    sync_slots(&mut slots, Some((5, 7)), 500);
    assert_eq!(slots.len(), 3, "a landed slot out of the window is dropped");
    sync_slots(&mut slots, Some((5, 9)), 6);
    assert_eq!(
        slots.iter().map(|s| s.page).max(),
        Some(5),
        "no slot past the document"
    );
    sync_slots(&mut slots, None, 0);
    assert!(slots.is_empty(), "no document holds no slots");
}

#[test]
fn pdf_first_frame_waits_for_every_page_it_shows() {
    let layout = Layout::new(&[1., 1., 1.], 100., 10.);
    let mut slots = Vec::new();
    sync_slots(&mut slots, layout.near(0., 150.), 3);
    assert!(
        !first_drawn(&slots, &layout, 0., 150.),
        "nothing landed holds the first frame"
    );
    slots[0].landed = true;
    assert!(
        !first_drawn(&slots, &layout, 0., 150.),
        "page 2 in view still holds it"
    );
    slots[1].errored = true;
    assert!(
        first_drawn(&slots, &layout, 0., 150.),
        "a failed page counts as settled"
    );
    assert!(
        first_drawn(&slots, &layout, 0., 50.),
        "a page out of view is never waited on"
    );
}
