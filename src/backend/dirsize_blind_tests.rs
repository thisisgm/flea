// The walk's two refusals to vouch for a size: a filesystem inotify cannot see, and a file named outside the tree.
use super::*;
use crate::backend::testdir::TestDir;
use std::cell::Cell;
use std::fs;

// One mountinfo line mounting `path` as `kind`, so the gate reads the kind the test names.
fn mounted_as(path: &Path, kind: &str) -> String {
    format!("36 25 0:99 / {} rw,relatime - {} share rw\n", path.display(), kind)
}

// Sample input: a host's virtiofs share, which a guest-side inotify watch never hears the host change.
#[test]
fn a_tree_on_a_vm_share_or_cluster_filesystem_arms_no_watch() {
    let d = TestDir::new("dirsize-blind");
    let tree = d.dir("tree");
    fs::create_dir(tree.join("inner")).unwrap();
    d.file("tree/inner/a.txt", "abc");
    let whole = walk_while(&tree, &|| false).bytes;
    for kind in ["virtiofs", "vboxsf", "gfs2", "ocfs2", "fuse.mergerfs"] {
        let armed = Cell::new(0);
        let arm = |_: &Path| armed.set(armed.get() + 1);
        let (size, linked) = walk_display_armed(&tree, &|| false, &mounted_as(&tree, kind), Some(&arm));
        assert_eq!(armed.get(), 0, "{} arms nothing, so its size is never kept", kind);
        assert_eq!((size.bytes, size.partial, linked), (whole, false, false), "{} still counts the whole tree", kind);
    }
    // Negative control: the same tree on a local disk arms both folders.
    let armed = Cell::new(0);
    let arm = |_: &Path| armed.set(armed.get() + 1);
    walk_display_armed(&tree, &|| false, &mounted_as(&tree, "ext4"), Some(&arm));
    assert_eq!(armed.get(), 2, "a local tree arms each folder it reads");
}

// Sample input: clip.bin also named outside the tree, where a write through that name fires no watch inside.
#[test]
fn a_walk_says_when_it_counted_a_file_with_another_name() {
    let d = TestDir::new("dirsize-linked");
    let tree = d.dir("tree");
    fs::create_dir(tree.join("inner")).unwrap();
    let clip = d.file("tree/inner/clip.bin", "x");
    std::os::unix::fs::symlink(&clip, tree.join("pointer")).unwrap();
    // A second name for the symlink itself, since btrfs counts no links on a folder and could not pin the regular-file test.
    fs::hard_link(tree.join("pointer"), d.join("pointer-elsewhere")).unwrap();
    assert!(fs::symlink_metadata(tree.join("pointer")).unwrap().nlink() > 1, "linkat links the symlink, not its target");
    let arm = |_: &Path| {};
    let local = mounted_as(&tree, "ext4");
    let (_, linked) = walk_display_armed(&tree, &|| false, &local, Some(&arm));
    assert!(!linked, "a folder's own link count and a linked symlink are not a file a write can grow");
    fs::hard_link(&clip, tree.join("copy.bin")).unwrap();
    let (_, linked) = walk_display_armed(&tree, &|| false, &local, Some(&arm));
    assert!(!linked, "both names inside the tree are both watched");
    fs::hard_link(&clip, d.join("elsewhere.bin")).unwrap();
    let (size, linked) = walk_display_armed(&tree, &|| false, &local, Some(&arm));
    assert!(linked && !size.partial, "the hard-linked file is counted and flagged");
    assert!(walk_counted(&tree, &|| false, None).linked, "the shared walk carries the same flag");
}

// Sample input: LINKED_CAP files each named twice inside the tree, then one more, which a bounded map cannot account for.
#[test]
fn a_tree_with_more_linked_files_than_the_cap_is_never_vouched_for() {
    let d = TestDir::new("dirsize-linked-cap");
    let tree = d.dir("tree");
    let add_pair = |n: usize| fs::hard_link(d.file(&format!("tree/f{n}"), "x"), tree.join(format!("g{n}"))).unwrap();
    (0..traversal::LINKED_CAP).for_each(add_pair);
    let arm = |_: &Path| {};
    let local = mounted_as(&tree, "ext4");
    let (size, linked) = walk_display_armed(&tree, &|| false, &local, Some(&arm));
    assert!(!size.partial && !linked, "every name inside and the map at its cap still vouches");
    add_pair(traversal::LINKED_CAP);
    let (size, linked) = walk_display_armed(&tree, &|| false, &local, Some(&arm));
    assert!(!size.partial && linked, "one linked file past the cap keeps the tree unkept");
}

// Sample input: clip.bin seen at the root with two names, then linked from outside before inner/ is read, so its second name reports three.
#[test]
fn a_link_count_that_moves_mid_walk_is_never_vouched_for() {
    let d = TestDir::new("dirsize-linked-moved");
    let tree = d.dir("tree");
    let inner = d.dir("tree/inner");
    let clip = d.file("tree/clip.bin", "x");
    fs::hard_link(&clip, inner.join("copy.bin")).unwrap();
    let outside = d.join("elsewhere.bin");
    let arm = |folder: &Path| if folder == inner { fs::hard_link(&clip, &outside).unwrap() };
    let (size, linked) = walk_display_armed(&tree, &|| false, &mounted_as(&tree, "ext4"), Some(&arm));
    assert!(outside.exists(), "the outside name was made between the two folder reads");
    assert!(!size.partial && linked, "both inside names seen, but the count they reported moved, so the tree is unkept");
}
