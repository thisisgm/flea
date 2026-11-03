use flea::protocol::{Reply, RowFacts};
use flea_gui::{
    grid::{geometry::Metrics, FocusSnapshot, GridView, Shown},
    pane::Pane,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[path = "g1_08/captions.rs"]
mod captions;
#[path = "g1_08/frames.rs"]
mod frames;
#[path = "g1_08/geometry.rs"]
mod geometry;
#[path = "g1_08/landing.rs"]
mod landing;
#[path = "g1_08/pictures.rs"]
mod pictures;
#[path = "g1_08/tip.rs"]
mod tip;
#[path = "g1_08/view.rs"]
mod view;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

// SAFETY: every call forwards to the system allocator unchanged; the count is a thread-local add.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        System.alloc_zeroed(layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        System.realloc(ptr, layout, size)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Heap allocations this thread has made so far.
fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

fn named(index: usize) -> RowFacts {
    RowFacts {
        name: format!("g_{index:05}.txt"),
        icon: "text-plain".into(),
        mode: 0o100644,
        size: 26,
        mtime: 1_760_600_000,
        ..RowFacts::default()
    }
}

/// A pane on a listing of total rows with its first window held, as the backend answers a new folder.
fn listed(total: usize, height: f32) -> Pane {
    let mut pane = Pane::new("/fixture/grid".into(), height);
    assert!(!pane
        .receive_at(
            Reply::Listed {
                path: "/fixture/grid".into(),
                total
            },
            1
        )
        .unwrap());
    let first = pane.first_count().min(total);
    assert!(pane
        .receive_at(
            Reply::Rows {
                start: 0,
                rows: (0..first).map(named).collect(),
                listing: 1
            },
            1
        )
        .unwrap());
    pane
}

/// Answers the pane's outstanding window with real rows, the way the backend does.
fn answer(pane: &mut Pane) -> bool {
    let request = pane.requests.outstanding.expect("a window is out");
    let end = (request.start + request.count).min(pane.held.total);
    pane.receive_at(
        Reply::Rows {
            start: request.start,
            rows: (request.start..end).map(named).collect(),
            listing: request.listing,
        },
        2,
    )
    .unwrap()
}

/// The board grid at text size 14: 908 by 640, six columns of 147 by 134.
fn board_view() -> GridView {
    GridView::new(Metrics::board(), (908., 640.))
}

fn focus(cursor: usize) -> FocusSnapshot {
    FocusSnapshot {
        cursor,
        ..FocusSnapshot::default()
    }
}

fn all(pane: &Pane) -> Shown<'static> {
    Shown::All(pane.held.total)
}
