// Deep trees, read-only parents, changing sources and holes (soak S4-5 to S4-12).
use super::deep::{self, Chain};
use super::*;
use crate::backend::testdir::TestDir;
use std::ffi::OsStr;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::atomic::AtomicBool;

// A thread the size of a backend worker's, so a walk that recursed per level overflows it as the backend did.
const WORKER_STACK: usize = 2 * 1024 * 1024;
// st_blocks counts 512-byte units whatever the filesystem block size is.
const BLOCK_UNIT: u64 = 512;
const MIB: u64 = 1024 * 1024;

fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
    Progress {
        cancel: flag,
        on_bytes: sink,
        partial: None,
        tree: None,
        manifest: None,
        durability: None,
        for_move: false,
    }
}

// Sample input: deep_tree(root, 3) leaves root/a/a/a/leaf.txt holding "leaf", built through held descriptors past PATH_MAX.
pub(super) fn deep_tree(root: &Path, depth: usize) {
    std::fs::create_dir(root).unwrap();
    let mut chain = Chain::new(open_dir(root).unwrap());
    for _ in 0..depth {
        std::fs::create_dir(held_path(chain.top()).join("a")).unwrap();
        chain.push(OsStr::new("a")).unwrap();
    }
    std::fs::write(held_path(chain.top()).join("leaf.txt"), "leaf").unwrap();
}

// How many levels of "a" the tree holds, and its leaf's text.
pub(super) fn deep_depth(root: &Path) -> (usize, String) {
    let mut chain = Chain::new(open_dir(root).unwrap());
    while held_path(chain.top()).join("a").is_dir() {
        chain.push(OsStr::new("a")).unwrap();
    }
    (
        chain.depth(),
        std::fs::read_to_string(held_path(chain.top()).join("leaf.txt")).unwrap_or_default(),
    )
}

fn on_worker_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(work)
        .unwrap()
        .join()
        .expect("the walk must not overflow")
}

// S4-5: a 3,000-level copy aborted the backend with a stack overflow; S4-6: 400 levels ran out of descriptors.
#[test]
fn a_tree_three_thousand_folders_deep_copies_on_a_worker_sized_stack() {
    let d = TestDir::new("deepcopy");
    let (src, dst) = (d.join("src"), d.join("dst"));
    deep_tree(&src, 3000);
    let (from, to) = (src.clone(), dst.clone());
    let copied = on_worker_stack(move || {
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        copy_any(&from, &to, &mut quiet(&flag, &mut sink)).map_err(|e| e.msg)
    });
    assert_eq!(copied, Ok(()));
    assert_eq!(
        deep_depth(&dst),
        (3000, "leaf".to_string()),
        "every level and the leaf landed"
    );
    assert_eq!(
        deep_depth(&src),
        (3000, "leaf".to_string()),
        "the source is untouched"
    );
    deep::remove_deep(&dst, false).expect("a deep copy removes level by level");
    deep::remove_deep(&src, false).unwrap();
    assert!(!dst.exists() && !src.exists());
}

// A deep move confirms each folder it filled, and the deepest path passes PATH_MAX.
#[test]
fn a_folder_past_path_max_is_confirmed() {
    let d = TestDir::new("deepconfirm");
    let root = d.join("deep");
    deep_tree(&root, 3000);
    let deepest: PathBuf = std::iter::once(root.as_os_str())
        .chain(std::iter::repeat_n(OsStr::new("a"), 3000))
        .collect();
    let confirmed = crate::backend::durable::fsync_dir(&deepest).map_err(|e| e.to_string());
    deep::remove_deep(&root, false).unwrap();
    assert_eq!(confirmed, Ok(()), "the deepest folder opens and syncs");
}

// S4-5: Properties (contents) and the transfer's size sweep walk the same tree with the size walker.
#[test]
fn the_size_walk_counts_a_tree_three_thousand_folders_deep() {
    let d = TestDir::new("deepwalk");
    let src = d.join("src");
    deep_tree(&src, 3000);
    let walked = src.clone();
    let tally =
        on_worker_stack(move || crate::backend::dirsize::walk_counted(&walked, &|| false, None));
    assert!(!tally.partial, "a whole walk is exact");
    assert_eq!(tally.items, 3001, "3,000 folders and the leaf");
    deep::remove_deep(&src, false).unwrap();
}

// The descriptor window holds HELD_LEVELS levels however deep the chain goes, and climbs back through "..".
#[test]
fn a_chain_holds_a_bounded_number_of_descriptors_and_climbs_back() {
    let d = TestDir::new("deepchain");
    let src = d.join("src");
    deep_tree(&src, 200);
    let mut chain = Chain::new(open_dir(&src).unwrap());
    for _ in 0..200 {
        chain.push(OsStr::new("a")).unwrap();
        assert!(chain.held() <= deep::HELD_LEVELS);
    }
    while chain.depth() > 0 {
        chain
            .pop()
            .expect("each parent reopens through .. as the same folder");
    }
    assert_eq!(
        deep::identity_of(chain.top()).unwrap(),
        deep::identity_of(&open_dir(&src).unwrap()).unwrap()
    );
    deep::remove_deep(&src, false).unwrap();
}

// A copy this run made takes the owner's bits back where its mode shuts the folder, so it can be removed.
#[test]
fn a_removal_allowed_to_fix_modes_opens_a_folder_its_mode_shut() {
    let d = TestDir::new("deepfix");
    let copy = d.dir("copy");
    std::fs::create_dir(copy.join("shut")).unwrap();
    std::fs::write(copy.join("shut/inside.txt"), "inside").unwrap();
    std::fs::set_permissions(copy.join("shut"), std::fs::Permissions::from_mode(0o000)).unwrap();
    let removed = deep::remove_deep(&copy, true);
    if copy.join("shut").exists() {
        std::fs::set_permissions(copy.join("shut"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    removed.expect("the shut folder opens once fixed");
    assert!(!copy.exists());
}

// Read-only folders past the descriptor window copy, and keep their modes, as the recursive walk did.
#[test]
fn read_only_folders_past_the_descriptor_window_copy_with_their_modes() {
    let d = TestDir::new("deepmodes");
    let (src, dst) = (d.join("src"), d.join("dst"));
    const DEPTH: usize = 40;
    const READ_ONLY_AT: usize = 3;
    // The read-only level leaves the descriptor window before the walk turns back to set its mode.
    const _: () = assert!(DEPTH > READ_ONLY_AT + deep::HELD_LEVELS);
    deep_tree(&src, DEPTH);
    let level = |root: &Path, n: usize| (0..n).fold(root.to_path_buf(), |at, _| at.join("a"));
    std::fs::create_dir(level(&src, DEPTH).join("shut")).unwrap();
    let modes = |root: &Path, shut: u32, read_only: u32| {
        std::fs::set_permissions(
            level(root, DEPTH).join("shut"),
            std::fs::Permissions::from_mode(shut),
        )
        .unwrap();
        std::fs::set_permissions(
            level(root, READ_ONLY_AT),
            std::fs::Permissions::from_mode(read_only),
        )
        .unwrap();
    };
    modes(&src, 0o400, 0o555);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let copied = copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).map_err(|e| e.msg);
    let mode = |path: PathBuf| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let kept = copied.is_ok().then(|| {
        (
            mode(level(&dst, READ_ONLY_AT)),
            mode(level(&dst, DEPTH).join("shut")),
        )
    });
    modes(&src, 0o755, 0o755);
    // A failed copy may lack these levels, and TestDir gives the owner's bits back to whatever landed.
    if kept.is_some() {
        modes(&dst, 0o755, 0o755);
    }
    assert_eq!(copied, Ok(()));
    assert_eq!(
        kept,
        Some((0o555, 0o400)),
        "each folder took its own mode once its children landed"
    );
}

// S4-12: a source truncated mid-copy reported success with a torn file; it now fails and the copy goes.
#[test]
fn a_source_cut_short_while_it_copies_fails_the_copy_and_leaves_no_torn_file() {
    let d = TestDir::new("torn");
    let src = d.join("big.bin");
    std::fs::write(&src, vec![7u8; 4 * MIB as usize]).unwrap();
    let dst = d.join("copy.bin");
    let flag = AtomicBool::new(false);
    let cut = src.clone();
    let mut sink = |_: u64, _: u64| {
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .open(&cut)
            .and_then(|f| f.set_len(1000));
    };
    let e = copy_any(&src, &dst, &mut quiet(&flag, &mut sink))
        .expect_err("the bytes read are no longer the file");
    assert_eq!(
        (e.where_.as_str(), e.msg.as_str()),
        ("copy", "changed during the copy")
    );
    assert!(!dst.exists(), "no torn copy is left under the name");
}

// S4-12 inside a tree: the torn file is named and skipped, and the tree is the partial the journal records.
#[test]
fn a_file_cut_short_inside_a_tree_is_named_and_left_out() {
    let d = TestDir::new("torntree");
    let tree = d.dir("tree");
    std::fs::write(tree.join("big.bin"), vec![7u8; 4 * MIB as usize]).unwrap();
    let dst = d.join("copy");
    let flag = AtomicBool::new(false);
    let cut = tree.join("big.bin");
    let mut sink = |_: u64, _: u64| {
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .open(&cut)
            .and_then(|f| f.set_len(1000));
    };
    let mut p = quiet(&flag, &mut sink);
    let e = copy_any(&tree, &dst, &mut p).expect_err("one file changed under the copy");
    assert_eq!(
        e.msg,
        "big.bin could not be copied: changed during the copy"
    );
    assert_eq!(
        p.partial.as_deref(),
        Some(dst.as_path()),
        "journaled as a partial copy"
    );
    assert!(!dst.join("big.bin").exists(), "the torn file was not kept");
}

// A moved tree that left a torn file out keeps its source, so the changed file is never removed.
#[test]
fn a_file_cut_short_inside_a_moved_tree_keeps_the_source() {
    let d = TestDir::new("torntreemove");
    let tree = d.dir("tree");
    std::fs::write(tree.join("big.bin"), vec![7u8; 4 * MIB as usize]).unwrap();
    let dst = d.join("moved");
    let flag = AtomicBool::new(false);
    let cut = tree.join("big.bin");
    let mut sink = |_: u64, _: u64| {
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .open(&cut)
            .and_then(|f| f.set_len(1000));
    };
    let mut p = quiet(&flag, &mut sink);
    p.for_move = true;
    test_force_exdev(true);
    let outcome = move_any(&tree, &dst, &mut p);
    test_force_exdev(false);
    let e = outcome.expect_err("one file changed under the move");
    assert_eq!(e.msg, "big.bin could not be moved: changed during the move");
    assert_eq!(p.partial.as_deref(), Some(dst.as_path()));
    assert_eq!(
        std::fs::metadata(tree.join("big.bin")).unwrap().len(),
        1000,
        "the source keeps the changed file"
    );
}

// S4-11: a sparse file landed dense; the copy now seeks over every hole and keeps the length and bytes.
#[test]
fn a_sparse_file_copies_with_its_holes() {
    use std::io::{Seek, SeekFrom, Write};
    let d = TestDir::new("sparse");
    let src = d.join("disk.img");
    let mut f = std::fs::File::create(&src).unwrap();
    f.write_all(&[1u8; 4096]).unwrap();
    f.seek(SeekFrom::Start(32 * MIB)).unwrap();
    f.write_all(b"middle").unwrap();
    f.set_len(64 * MIB).unwrap();
    drop(f);
    let allocated = |path: &Path| std::fs::metadata(path).unwrap().blocks() * BLOCK_UNIT;
    if allocated(&src) >= MIB {
        eprintln!("this filesystem keeps no holes, so there is nothing to keep");
        return;
    }
    let dst = d.join("copy.img");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("copy");
    assert_eq!(
        std::fs::metadata(&dst).unwrap().len(),
        64 * MIB,
        "the trailing hole keeps the length"
    );
    assert!(
        allocated(&dst) < MIB,
        "the holes stay holes: {} bytes allocated",
        allocated(&dst)
    );
    assert!(
        std::fs::read(&src).unwrap() == std::fs::read(&dst).unwrap(),
        "every byte, hole or data, reads the same"
    );
    let (duplicated, _) = crate::backend::ops::duplicate(&src);
    assert!(
        allocated(&duplicated.expect("duplicate")) < MIB,
        "Duplicate keeps them too"
    );
}

// A FUSE daemon such as gvfs may refuse a write past the end of an upload, so a sparse file lands there dense, as before.
#[test]
fn a_sparse_file_lands_dense_on_fuse() {
    use std::io::{Seek, SeekFrom, Write};
    let d = TestDir::new("sparsefuse");
    let src = d.join("disk.img");
    let mut f = std::fs::File::create(&src).unwrap();
    f.seek(SeekFrom::Start(8 * MIB)).unwrap();
    f.write_all(b"end").unwrap();
    drop(f);
    if std::fs::metadata(&src).unwrap().blocks() * BLOCK_UNIT >= MIB {
        eprintln!("this filesystem keeps no holes, so there is nothing to keep");
        return;
    }
    let dst = d.join("copy.img");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    test_set_precheck(
        Some(u64::MAX),
        Some(1),
        Some(super::super::copyread::FUSE_MAGIC),
    );
    let outcome = copy_any(&src, &dst, &mut quiet(&flag, &mut sink));
    test_set_precheck(None, None, None);
    outcome.expect("copy");
    assert!(
        std::fs::metadata(&dst).unwrap().blocks() * BLOCK_UNIT >= 8 * MIB,
        "every zero was written"
    );
    assert!(std::fs::read(&src).unwrap() == std::fs::read(&dst).unwrap());
}

// The S4-8 long-name copies, kept in their own file under the size cap.
#[path = "copyname_tests.rs"]
mod names;
