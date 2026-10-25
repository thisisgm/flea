#[path = "../src/perf/glyph_probe.rs"]
mod glyph_probe;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    rc::Rc,
};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn counted() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

// SAFETY: allocation calls preserve the System allocator's arguments and results.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        counted();
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        counted();
        System.alloc_zeroed(layout)
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        counted();
        System.realloc(pointer, layout, size)
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

#[test]
fn unchanged_glyph_runs_reuse_tiles_without_paint_allocation() {
    let mut app = glyph_probe::app();
    let cx = app.add_empty_window();
    cx.simulate_scale_factor_change(2.);
    let run = glyph_probe::line(cx, "bounds-AVAV-iiii-WWWW.txt", 14.);
    let color = glyph_probe::ink(cx);
    for _ in 0..4 {
        glyph_probe::paint(cx, Rc::clone(&run), 14., 60., color, allocations);
    }
    for y in [60., 61.5, 90., 40., 60.] {
        let sample = glyph_probe::paint(cx, Rc::clone(&run), 14., y, color, allocations);
        assert_eq!(
            sample.resolves, 1,
            "vertical scrolling must retain glyph tiles"
        );
        assert_eq!(
            sample.allocations, 0,
            "warm custom paint must allocate nothing"
        );
    }
}

#[test]
fn horizontal_position_and_scale_invalidate_retained_tiles() {
    let mut app = glyph_probe::app();
    let cx = app.add_empty_window();
    cx.simulate_scale_factor_change(2.);
    let run = glyph_probe::line(cx, "position-and-scale.txt", 14.);
    let color = glyph_probe::ink(cx);
    let first = glyph_probe::paint(cx, Rc::clone(&run), 14., 60., color, allocations);
    assert_eq!(first.resolves, 1);
    let moved = glyph_probe::paint(cx, Rc::clone(&run), 14.25, 60., color, allocations);
    assert_eq!(moved.resolves, 2);
    cx.simulate_scale_factor_change(1.5);
    let scaled = glyph_probe::paint(cx, Rc::clone(&run), 14.25, 60., color, allocations);
    assert_eq!(scaled.resolves, 3);
    assert_eq!(
        glyph_probe::paint(cx, run, 14.25, 61., color, allocations).resolves,
        3
    );
}

#[test]
fn changed_shaping_gets_fresh_tiles_without_reusing_the_old_line() {
    let mut app = glyph_probe::app();
    let cx = app.add_empty_window();
    let original = glyph_probe::line(cx, "original-name.txt", 14.);
    let changed = glyph_probe::line(cx, "changed-name.txt", 14.);
    let resized = glyph_probe::line(cx, "original-name.txt", 20.);
    let color = glyph_probe::ink(cx);
    assert!(!std::sync::Arc::ptr_eq(original.line(), changed.line()));
    assert!(!std::sync::Arc::ptr_eq(original.line(), resized.line()));
    for run in [original, changed, resized] {
        assert_eq!(run.resolves(), 0);
        let sample = glyph_probe::paint(cx, run, 14., 60., color, allocations);
        assert_eq!(sample.resolves, 1);
    }
}

#[test]
fn allocation_counter_has_a_positive_control() {
    let mut app = glyph_probe::app();
    let cx = app.add_empty_window();
    let sample = glyph_probe::observe(
        cx,
        |_| {
            let allocation = std::hint::black_box(Box::new([0_u8; 64]));
            drop(std::hint::black_box(allocation));
            0
        },
        allocations,
    );
    assert!(
        sample.allocations > 0,
        "a forced allocation inside paint must fail the zero count"
    );
}
