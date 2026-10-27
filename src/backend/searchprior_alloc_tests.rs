use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

struct Counting;

fn allocated() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() { count.set(Some(value + 1)); }
    });
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated();
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn searchprior_fix1_no_per_match_path_allocations() {
    let mut index = Index::new();
    for row in 0..100_000 { assert!(index.push(format!("near/report{row:06}.rs"), false)); }
    let mut hits: Vec<_> = (0..index.len()).map(|row| (3_500_000, row)).collect();
    let prior = Priors::new(vec![b"/work/near/unrelated.txt".to_vec()], Vec::new());
    ALLOCATIONS.with(|count| count.set(Some(0)));
    prior.apply(Path::new("/work"), Path::new("/work/near"), &index, &mut hits);
    let count = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    println!("FB3_FIX1 matches={} allocations={count}", hits.len());
    assert!(count <= 16, "100k matches may grow reusable scratch, never allocate a path for each match: {count}");
    assert_eq!(hits.len(), 100_000);
    assert!(hits.iter().all(|(score, _)| *score == 3_500_000 * SCALE + 23));
}

// Counts the calling thread only, so unrelated workers cannot move an allocation gate.
pub(crate) fn count_allocations<T>(work: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATIONS.with(|count| assert!(count.replace(Some(0)).is_none()));
    let result = work();
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    (result, allocations)
}

// A deterministic loaded snapshot for the query phase probe; production ranking is unchanged.
pub(crate) fn fixture_priors(files: Vec<Vec<u8>>, folders: Vec<Vec<u8>>) -> Priors {
    Priors::new(files, folders)
}

pub(crate) fn fixture_counts(priors: &Priors) -> (usize, usize) {
    (priors.files.len(), priors.folders.len())
}
