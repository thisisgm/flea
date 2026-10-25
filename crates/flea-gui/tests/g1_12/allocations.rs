//! A kept fit, a hit, a hover, a popup step and a repeated shape allocate nothing, with a positive control.
use super::{
    jump::{answer, Folders},
    HOME,
};
use flea_gui::path::{
    paint::{Paint, Shapes, Size},
    strip::{Hit, Strip, Tap},
    suggestions::{Jump, Popup, Suggestion},
};
use gpui::{px, GlyphRun, LineLayout};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    sync::Arc,
};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Allocator;

fn record() {
    if ENABLED.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn counted(operation: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    ENABLED.with(|enabled| enabled.set(true));
    operation();
    ENABLED.with(|enabled| enabled.set(false));
    ALLOCATIONS.with(Cell::get)
}

fn run() -> GlyphRun {
    GlyphRun::new(Arc::new(LineLayout::default()))
}

// A shaped piece seven pixels a character, as the test face draws ASCII.
fn shaped(text: &str) -> GlyphRun {
    GlyphRun::new(Arc::new(LineLayout {
        width: px(7. * text.chars().count() as f32),
        len: text.len(),
        ..LineLayout::default()
    }))
}

#[test]
fn a_kept_fit_a_hit_a_hover_a_press_and_a_popup_step_allocate_nothing() {
    assert!(
        counted(|| drop(black_box(Box::new(7)))) > 0,
        "allocator control did not count"
    );
    let path = "/home/gm/Work/claude";
    let mut strip = Strip::default();
    assert!(strip.fit(path, HOME, None, 120));
    strip.place([14., 35., 42.]);
    let (mut jump, mut ranker, mut out) = (Jump::default(), Folders::default(), Vec::new());
    jump.open(&mut out);
    jump.history_read(&mut out);
    jump.take(
        &answer(1, &["/a/one", "/b/done", "/c/ozone"]),
        &mut ranker,
        HOME,
        "o",
        &mut out,
    );
    assert_eq!(jump.entries.len(), 4);
    let popup = Popup::measure(super::jump::board(), 360., 80., 788., 9);
    let count = counted(|| {
        for step in 0..1000_usize {
            let at = step as f32;
            black_box(strip.fit(path, HOME, None, 120));
            let hit = strip.hit(at % 30. * 7.2);
            black_box(strip.hover(Some(hit)));
            black_box(strip.extent(step % 4));
            black_box(strip.press(Hit::Ground, 1 + step % 2, 0));
            black_box(strip.press(Hit::Crumb(2), 1, 0));
            black_box(popup.row_at(100., 30. + at % 300., 0., 9));
            black_box(popup.visible(at % 61., 9));
            black_box(popup.reveal(0., step % 9, 9));
            black_box(popup.fades(at % 61.));
            black_box(jump.step(if step % 2 == 0 { 1 } else { -1 }));
        }
    });
    assert_eq!(count, 0);
    assert_eq!(strip.built, 1, "a kept fit never rebuilds");
    strip.press(Hit::Ground, 1, 0);
    assert_eq!(
        strip.press(Hit::Ground, 2, 0),
        Tap::Edit,
        "the measured presses took the real table"
    );
}

#[test]
fn a_repeated_shape_is_a_hit_that_allocates_nothing_and_the_cache_stays_bounded() {
    let mut shapes = Shapes::new(4);
    shapes.get("Work/", Size::Caption, false, run);
    let count = counted(|| {
        for _ in 0..1000 {
            black_box(shapes.get("Work/", Size::Caption, false, || {
                unreachable!("a hit never shapes")
            }));
        }
    });
    assert_eq!(count, 0);
    assert_eq!((shapes.hits, shapes.misses), (1000, 1));
    shapes.get("Work/", Size::Caption, true, run);
    assert_eq!(shapes.misses, 2, "a bold piece is another shape");
    for text in ["a", "b", "c", "d", "e", "f"] {
        shapes.get(text, Size::Caption, false, run);
    }
    assert_eq!(shapes.len(), 4, "the least recently used piece gives way");
    shapes.get("f", Size::Caption, false, || {
        unreachable!("the newest piece is kept")
    });
    let misses = shapes.misses;
    shapes.get("a", Size::Caption, false, run);
    assert_eq!(
        shapes.misses,
        misses + 1,
        "the oldest piece was the one dropped"
    );
}

#[test]
fn a_redrawn_strip_and_popup_reuse_their_kept_runs_and_pieces_and_allocate_nothing() {
    assert!(
        counted(|| drop(black_box(Box::new(7)))) > 0,
        "allocator control did not count"
    );
    let mut paint = Paint::new();
    let mut strip = Strip::default();
    assert!(strip.fit("/home/gm/Work/claude", HOME, None, 120));
    let rows: Vec<Suggestion> = [
        "/srv/omarchy",
        "/home/gm/Documents/Notes",
        "/fx/v/work/Documents",
    ]
    .iter()
    .map(|path| Suggestion {
        path: path.to_string(),
        text: path.to_string(),
        leaf_start: path.rfind('/').unwrap() + 1,
        wash: 6..7,
    })
    .collect();
    // The first frame shapes every piece and sizes both buffers.
    paint.lay_strip(0, &mut strip, |_, text| shaped(text));
    paint.lay_menu(&rows, Some(0), (0, 3), |_, text, _| shaped(text));
    assert_eq!(
        (paint.runs[0].len(), strip.extent(1)),
        (3, Some((14., 35.)))
    );
    let pieces = paint.pieces.len();
    assert!(pieces >= 6, "{pieces}");
    let count = counted(|| {
        for _ in 0..1000 {
            black_box(strip.fit("/home/gm/Work/claude", HOME, None, 120));
            paint.lay_strip(0, &mut strip, |_, _| {
                unreachable!("an unchanged strip shapes nothing")
            });
            paint.lay_menu(&rows, Some(0), (0, 3), |_, _, _| {
                unreachable!("an unchanged popup shapes nothing")
            });
        }
    });
    assert_eq!(
        count, 0,
        "a redraw of an unchanged strip and popup allocates"
    );
    assert_eq!((paint.runs[0].len(), paint.pieces.len()), (3, pieces));
    // Positive control: a changed path shapes its new crumb, which allocates.
    assert!(strip.fit("/home/gm/Work/other", HOME, None, 120));
    assert!(counted(|| paint.lay_strip(0, &mut strip, |_, text| shaped(text))) > 0);
}
