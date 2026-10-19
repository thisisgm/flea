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

#[path = "../src/backend/searchbits.rs"]
mod searchbits;
#[path = "../src/backend/searchnames.rs"]
mod searchnames;
#[path = "../src/backend/searchblocks.rs"]
mod searchblocks;

#[test]
fn review_fb1_storage_round_trip() {
    let mut names = searchnames::Names::new();
    let plain = names.intern(b"plain").unwrap();
    let raw = names.intern(b"dir\xff").unwrap();
    assert_eq!(names.intern(b"plain").unwrap(), plain);
    assert_eq!(names.len(), 2);
    let before = names.bytes(true);
    names.finish();
    assert!(names.bytes(true) <= before);
    assert!(names.bytes(true) >= names.bytes(false));
    assert_eq!(names.get(raw), b"dir\xff");
    let plain_bits = names.bits(plain as usize);
    assert_eq!(plain_bits, searchbits::mask(b"plain"));
    assert_eq!(names.group_bits(0), plain_bits | names.bits(raw as usize));
    assert!(searchbits::possible(searchbits::mask(b"plain"), plain_bits, 0));
    assert!(!searchbits::possible(searchbits::mask(b"plain/"), plain_bits, 0));

    let mut blocks = searchblocks::Blocks::new();
    let parent = blocks.directory(&names, searchnames::EMPTY, plain).unwrap();
    let child = blocks.directory(&names, parent, raw).unwrap();
    assert_eq!(blocks.directory(&names, parent, raw).unwrap(), child);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks.record(child), (raw, parent));
    assert_eq!(blocks.identity(child), Some((parent, raw)));
    assert_eq!(blocks.identity(searchnames::EMPTY), None);
    assert_eq!(blocks.text_len(child) as usize, String::from_utf8_lossy(b"plain/dir\xff").len());
    let mut ancestors = vec![searchnames::EMPTY];
    blocks.ancestors(child, &mut ancestors);
    assert_eq!(ancestors, [raw, plain]);
    assert_eq!(blocks.children(parent), [child]);
    let heads = blocks.bytes(false);
    blocks.set_head(parent, 0);
    blocks.set_head(child, 2);
    assert_eq!((blocks.head(parent), blocks.head(child)), (0, 2));
    assert_eq!(blocks.bytes(false), heads);
    assert!(blocks.bytes(true) >= blocks.bytes(false));
}

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
