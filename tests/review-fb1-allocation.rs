use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, Ordering};

static REFUSE: AtomicBool = AtomicBool::new(false);
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if REFUSE.load(Ordering::Relaxed) { std::ptr::null_mut() } else { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { System.dealloc(ptr, layout); }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if REFUSE.load(Ordering::Relaxed) { std::ptr::null_mut() } else { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[path = "../src/backend/searchnames.rs"]
mod searchnames;
#[path = "../src/backend/searchblocks.rs"]
mod searchblocks;

#[test]
fn review_fb1_directory_allocation_child() {
    let Ok(case) = std::env::var("REVIEW_FB1_ALLOC_CASE") else { return; };
    let mut names = searchnames::Names::new();
    let bytes: &[u8] = if case == "raw" { b"dir\xff" } else { b"plain" };
    let id = names.intern(bytes).unwrap();
    let mut blocks = searchblocks::Blocks::new();
    REFUSE.store(true, Ordering::Relaxed);
    let result = blocks.directory(&names, searchnames::EMPTY, id);
    REFUSE.store(false, Ordering::Relaxed);
    assert_eq!(result, Err("allocation"));
}

#[test]
fn review_fb1_non_utf8_parent_reports_allocation_refusal() {
    for case in ["ascii", "raw"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "review_fb1_directory_allocation_child", "--nocapture"])
            .env("REVIEW_FB1_ALLOC_CASE", case).output().unwrap();
        eprintln!("case={case} status={} stderr={}", output.status, String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "{case} parent must return allocation refusal instead of aborting");
    }
}
