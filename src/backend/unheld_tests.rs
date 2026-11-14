// A copy onto a drive that cannot hold some items lands every other item and names each one it could not hold.
use crate::backend::copyfile::{copy_any, move_cross_device, test_set_precheck, At, Progress};
use crate::backend::opsreq::{run_transfer, OpMsg};
use crate::backend::testdir::TestDir;
use crate::backend::undo::{Entry, Journal, Step};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;

const EPERM: i32 = 1;
const EEXIST: i32 = 17;
const EACCES: i32 = 13;
const EINVAL: i32 = 22;
const EOPNOTSUPP: i32 = 95;

// The FAT model every case here shares: symlink(2) answers EPERM and the destination reads as vfat.
fn fat(on: bool) {
    super::test_force_linkless(on);
}

// Sample tree: a.txt, link-file -> a.txt, sub/b.txt, sub/link-dir -> ../deep, deep/c.txt, broken -> missing, z.txt.
fn tree_with_links(d: &TestDir) -> PathBuf {
    let tree = d.dir("tree");
    std::fs::write(tree.join("a.txt"), "a").unwrap();
    std::fs::create_dir(tree.join("sub")).unwrap();
    std::fs::write(tree.join("sub/b.txt"), "b").unwrap();
    std::fs::create_dir(tree.join("deep")).unwrap();
    std::fs::write(tree.join("deep/c.txt"), "c").unwrap();
    std::fs::write(tree.join("z.txt"), "z").unwrap();
    std::os::unix::fs::symlink("a.txt", tree.join("link-file")).unwrap();
    std::os::unix::fs::symlink("../deep", tree.join("sub/link-dir")).unwrap();
    std::os::unix::fs::symlink("missing", tree.join("broken")).unwrap();
    tree
}

// Every plain file the tree holds, so a walk that stopped early leaves one of these missing.
fn assert_every_file_landed(out: &Path) {
    for (rel, body) in [("a.txt", "a"), ("sub/b.txt", "b"), ("deep/c.txt", "c"), ("z.txt", "z")] {
        assert_eq!(std::fs::read_to_string(out.join(rel)).ok().as_deref(), Some(body), "{rel} must land past every refused item");
    }
}

fn assert_no_link_landed(out: &Path) {
    for rel in ["link-file", "sub/link-dir", "broken"] {
        assert!(out.join(rel).symlink_metadata().is_err(), "{rel} cannot exist on a drive that holds no links");
    }
}

// The item lines and the done line one transfer sends, read off the channel the loop reads.
struct Sent {
    items: Vec<(String, bool, String)>,
    ok: usize,
    failed: usize,
    entry: Entry,
    refused: Vec<(String, String)>,
}

fn sent(moving: bool, srcs: &[&Path], dest: &Path) -> Sent {
    let (tx, rx) = channel();
    let paths = srcs.iter().map(|src| src.to_string_lossy().to_string()).collect();
    run_transfer(1, moving, paths, dest.to_path_buf(), Arc::new(AtomicBool::new(false)), tx);
    let (mut items, mut done) = (Vec::new(), None);
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { name, ok, err, .. } => items.push((name, ok, err)),
            OpMsg::TransferDone { ok, failed, entry, refused, .. } => done = Some((ok, failed, entry, refused)),
            _ => {}
        }
    }
    let (ok, failed, entry, refused) = done.expect("a done line");
    Sent { items, ok, failed, entry, refused }
}

fn transferred(src: &Path, dest: &Path) -> (Vec<(String, bool, String)>, usize, usize, Entry) {
    let s = sent(false, &[src], dest);
    (s.items, s.ok, s.failed, s.entry)
}

#[test]
fn a_copy_onto_a_drive_without_links_lands_every_other_item_and_names_the_links() {
    let d = TestDir::new("unheldlinks");
    let src = tree_with_links(&d);
    let dest = d.dir("out");
    fat(true);
    let (items, ok, failed, entry) = transferred(&src, &dest);
    fat(false);
    let out = dest.join("tree");
    assert_every_file_landed(&out);
    assert_no_link_landed(&out);
    assert_eq!((ok, failed), (0, 1), "a folder that could not land whole is a failed item, never a silent success");
    let (name, item_ok, err) = &items[0];
    assert!(name == "tree" && !item_ok, "the folder's own line fails: {items:?}");
    assert!(err.starts_with("3 items inside could not be copied; the first is ") && err.ends_with(": this drive cannot hold links"),
        "the item names how many it could not hold and the first of them: {err}");
    assert!(["link-file", "sub/link-dir", "broken"].iter().any(|rel| err.contains(&format!(" {rel}: "))), "the first one is named by its path: {err}");
    assert!(matches!(&entry.steps[..], [Step::Copied { to, manifest: Some(_), .. }] if to == &out),
        "what landed is journaled as a partial with its manifest: {:?}", entry.steps);
    let mut j = Journal::new();
    j.push(entry);
    assert_eq!(j.undo().expect("undo"), "copy");
    assert!(!out.exists(), "undo removes every path the partial copy created");
    assert!(src.join("link-file").symlink_metadata().unwrap().file_type().is_symlink(), "and leaves the source whole");
}

#[test]
fn a_link_refused_as_unsupported_on_any_drive_never_ends_the_copy() {
    let d = TestDir::new("unheldnotsup");
    let src = tree_with_links(&d);
    let dest = d.dir("out");
    super::test_refuse(&[("link-file", EOPNOTSUPP), ("link-dir", EOPNOTSUPP), ("broken", EOPNOTSUPP)]);
    let (items, ok, failed, _) = transferred(&src, &dest);
    super::test_refuse(&[]);
    let out = dest.join("tree");
    assert_every_file_landed(&out);
    assert_no_link_landed(&out);
    assert_eq!((ok, failed), (0, 1));
    assert!(items[0].2.ends_with(": this drive cannot hold links"), "EOPNOTSUPP says the drive cannot hold links: {:?}", items[0]);
}

#[test]
fn a_move_onto_a_drive_without_links_copies_the_rest_keeps_its_source_and_names_the_link() {
    let d = TestDir::new("unheldmove");
    let src = d.dir("tree");
    std::fs::write(src.join("a.txt"), "a").unwrap();
    std::fs::write(src.join("b.txt"), "b").unwrap();
    std::fs::write(src.join("c.txt"), "c").unwrap();
    std::os::unix::fs::symlink("a.txt", src.join("link")).unwrap();
    let moved = d.join("moved");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    fat(true);
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: None, for_move: true };
    let outcome = move_cross_device(&src, &moved, &mut p);
    fat(false);
    let error = outcome.expect_err("a move never drops a link to succeed");
    assert_eq!(error.msg, "link could not be moved: this drive cannot hold links");
    assert_eq!(p.partial.as_deref(), Some(moved.as_path()), "what landed is the partial the journal records");
    for leaf in ["a.txt", "b.txt", "c.txt"] {
        assert!(moved.join(leaf).is_file(), "{leaf} lands past the link");
        assert!(src.join(leaf).is_file(), "and its source stays");
    }
    assert!(src.join("link").symlink_metadata().unwrap().file_type().is_symlink(), "the refused link stays in the source");
}

// One of each other item FAT cannot hold: a refused name, a special file, a 4 GiB file and a name folded onto a sibling.
#[test]
fn every_item_fat_cannot_hold_is_skipped_and_counted_while_the_rest_lands() {
    let d = TestDir::new("unheldclass");
    let src = d.dir("tree");
    std::fs::write(src.join("a:b.txt"), "colon").unwrap();
    std::fs::write(src.join("CASE.txt"), "upper").unwrap();
    std::fs::write(src.join("case.txt"), "lower").unwrap();
    std::fs::File::create(src.join("big.iso")).unwrap().set_len(4 * 1024 * 1024 * 1024).unwrap();
    crate::backend::fifotest::mkfifo(&src.join("pipe"));
    for (rel, body) in [("a.txt", "a"), ("z.txt", "z")] {
        std::fs::write(src.join(rel), body).unwrap();
    }
    std::fs::create_dir(src.join("deep")).unwrap();
    std::fs::write(src.join("deep/c.txt"), "c").unwrap();
    let dest = d.dir("out");
    fat(true);
    super::test_refuse(&[("a:b.txt", EINVAL), ("pipe", EPERM), ("case.txt", EEXIST)]);
    test_set_precheck(Some(u64::MAX), Some(1), Some(0x4D44));
    let (items, ok, failed, _) = transferred(&src, &dest);
    test_set_precheck(None, None, None);
    super::test_refuse(&[]);
    fat(false);
    let out = dest.join("tree");
    for (rel, body) in [("a.txt", "a"), ("z.txt", "z"), ("deep/c.txt", "c"), ("CASE.txt", "upper")] {
        assert_eq!(std::fs::read_to_string(out.join(rel)).ok().as_deref(), Some(body), "{rel} must land past every refused item");
    }
    for rel in ["a:b.txt", "case.txt", "big.iso", "pipe"] {
        assert!(out.join(rel).symlink_metadata().is_err(), "{rel} is refused, not half-written");
    }
    assert_eq!((ok, failed), (0, 1));
    assert!(items[0].2.starts_with("4 items inside could not be copied; the first is "), "{:?}", items[0]);
}

// Sample input: EPERM from symlink under vfat or exfat is the drive's limit; under ext4 it stays the copy's own error.
#[test]
fn eperm_means_cannot_hold_only_on_a_drive_with_no_operation_for_the_kind() {
    let d = TestDir::new("unheldreason");
    let named = d.join("l");
    let eperm = std::io::Error::from_raw_os_error(EPERM);
    assert_eq!(super::refused(super::Kind::Link, &eperm, At { at: &named, named: &named }, None).where_, "copy", "EPERM on a drive that holds links is not skipped");
    fat(true);
    let link = super::refused(super::Kind::Link, &eperm, At { at: &named, named: &named }, None);
    let node = super::refused(super::Kind::Node, &eperm, At { at: &named, named: &named }, None);
    fat(false);
    assert_eq!((link.where_.as_str(), link.msg.as_str()), (super::UNHELD, super::LINKS));
    assert_eq!((node.where_.as_str(), node.msg.as_str()), (super::UNHELD, super::NODES));
}

// Sample input: EINVAL is the drive's refusal under ntfs3 with windows_names, and the copy's own error where any name is taken.
#[test]
fn einval_names_a_refusal_only_on_a_drive_that_restricts_names() {
    let d = TestDir::new("unheldinvalid");
    let (named, invalid) = (d.join("plain.txt"), std::io::Error::from_raw_os_error(EINVAL));
    let refused = |options: Option<&str>| {
        let body = |options: &str| format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - ntfs3 /dev/b {options}\n", d.path().display());
        let _held = options.map(|options| crate::backend::iomount::test_hold_body(body(options)));
        super::refused(super::Kind::Entry, &invalid, At { at: &named, named: &named }, None)
    };
    fat(false);
    let (own, plain, restricted) = (refused(None), refused(Some("rw")), refused(Some("rw,windows_names")));
    assert_eq!(own.where_.as_str(), "copy", "EINVAL on a drive with no name rules is the copy's own error");
    assert_eq!(plain.where_.as_str(), "copy", "an ntfs3 mount that takes any name leaves EINVAL to the copy");
    assert_eq!((restricted.where_.as_str(), restricted.msg.as_str()), (super::UNHELD, super::NAME), "windows_names makes EINVAL the drive's refusal");
}

// Sample input: EEXIST on README beside the source sibling "readme." is the drive's under vfat or exfat, another writer's elsewhere.
#[test]
fn a_taken_name_is_the_drive_folding_only_on_vfat_or_exfat_beside_a_folding_sibling() {
    let d = TestDir::new("unheldfold");
    let named = d.join("l");
    let taken = std::io::Error::from_raw_os_error(EEXIST);
    let source = d.file("README", "u");
    assert_eq!(super::refused(super::Kind::Entry, &taken, At { at: &named, named: &named }, None).msg, "already exists", "a taken name at the top is the collide card's");
    fat(true);
    let lone = super::refused(super::Kind::Entry, &taken, At { at: &named, named: &named }, Some(&source));
    fat(false);
    assert_eq!(lone.where_, "copy", "with no folding sibling it is another writer's");
    d.file("readme.", "l");
    assert_eq!(super::refused(super::Kind::Entry, &taken, At { at: &named, named: &named }, Some(&source)).where_, "copy", "a drive that keeps both names folds neither, so it is another writer's");
    fat(true);
    let folded = super::refused(super::Kind::Entry, &taken, At { at: &named, named: &named }, Some(&source));
    fat(false);
    assert_eq!((folded.where_.as_str(), folded.msg.as_str()), (super::UNHELD, super::TAKEN), "on vfat or exfat beside a folding sibling it is the drive's");
}

// An item that ends on its own copy error answers that error, and the done line names no entry the error never counts.
#[test]
fn an_item_that_ends_on_its_own_error_names_no_refused_entry() {
    let d = TestDir::new("unheldplain");
    let src = d.dir("tree");
    std::os::unix::fs::symlink("x", src.join("p")).unwrap();
    std::os::unix::fs::symlink("x", src.join("q")).unwrap();
    // The walk meets the two in the source's own readdir order: the first is refused, then the second ends the item.
    let order: Vec<String> = std::fs::read_dir(&src).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    super::test_refuse(&[(order[0].as_str(), EOPNOTSUPP), (order[1].as_str(), EACCES)]);
    let s = sent(false, &[&src], &d.dir("out"));
    super::test_refuse(&[]);
    assert_eq!((s.ok, s.failed), (0, 1));
    assert_eq!(s.items, vec![("tree".to_string(), false, "permission denied".to_string())], "the item answers its own error");
    assert!(s.refused.is_empty(), "the done line names nothing the item's error does not count: {:?}", s.refused);
}

// copy_any keeps what its walk refused only when it answers that summary, so a walk ending on its own error leaves nothing to name.
#[test]
fn a_walk_that_ends_on_its_own_error_keeps_no_refusal() {
    let d = TestDir::new("unheldkeep");
    let src = d.dir("tree");
    std::os::unix::fs::symlink("x", src.join("p")).unwrap();
    std::os::unix::fs::symlink("x", src.join("q")).unwrap();
    let order: Vec<String> = std::fs::read_dir(&src).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    super::test_refuse(&[(order[0].as_str(), EOPNOTSUPP), (order[1].as_str(), EACCES)]);
    let (flag, mut sink) = (AtomicBool::new(false), |_: u64, _: u64| {});
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: None, for_move: false };
    let r = copy_any(&src, &d.dir("out").join("tree"), &mut p);
    super::test_refuse(&[]);
    assert_eq!(r.map_err(|e| e.msg), Err("permission denied".to_string()), "the second entry ends the walk with its own error");
    assert!(super::take().is_empty(), "nothing the walk refused outlives the item's own error");
}

// Past unheld::NAMED refusals the done line stops naming, across items, while each item's sentence keeps its whole count.
#[test]
fn the_done_line_names_at_most_named_entries_and_each_item_counts_all_of_its_own() {
    let d = TestDir::new("unheldcap");
    let (big, small) = (d.dir("big"), d.dir("small"));
    for n in 0..super::NAMED + 5 {
        std::os::unix::fs::symlink("x", big.join(format!("l{n}"))).unwrap();
    }
    for n in 0..3 {
        std::os::unix::fs::symlink("x", small.join(format!("l{n}"))).unwrap();
    }
    fat(true);
    let s = sent(false, &[&big, &small], &d.dir("out"));
    fat(false);
    assert_eq!((s.ok, s.failed), (0, 2));
    let count = format!("{} items inside could not be copied; the first is l", super::NAMED + 5);
    assert!(s.items[0].2.starts_with(&count), "the first item counts every refusal past the cap: {:?}", s.items[0]);
    assert!(s.items[1].2.starts_with("3 items inside could not be copied; the first is l"), "{:?}", s.items[1]);
    assert_eq!(s.refused.len(), super::NAMED, "the done line names at most the cap, across items");
    assert!(s.refused.iter().all(|(name, _)| name.starts_with("big/l")), "and the first item's refusals fill it in walk order");
}

// A cross-device move of files and folders: the files stage in one batch, each folder moves alone and keeps its source.
#[test]
fn a_batched_move_onto_a_drive_without_links_removes_only_the_sources_that_landed_whole() {
    let _batch = crate::backend::movebatch::ForceCopyGuard::hold();
    let d = TestDir::new("unheldbatch");
    let src = d.dir("src");
    let files = ["a.txt", "b.txt", "c.txt", "e.txt"];
    for leaf in files {
        std::fs::write(src.join(leaf), leaf).unwrap();
    }
    let trees = [("t1", "t1/link"), ("t2", "t2/sub/link")];
    for (tree, link) in trees {
        std::fs::create_dir_all(src.join(tree).join("sub")).unwrap();
        std::fs::write(src.join(tree).join("sub/x.txt"), "x").unwrap();
        std::os::unix::fs::symlink("sub/x.txt", src.join(link)).unwrap();
    }
    let order: Vec<PathBuf> = ["a.txt", "b.txt", "t1", "c.txt", "t2", "e.txt"].iter().map(|leaf| src.join(leaf)).collect();
    let out = d.dir("out");
    fat(true);
    crate::backend::copyfile::test_force_exdev(true);
    let s = sent(true, &order.iter().map(PathBuf::as_path).collect::<Vec<_>>(), &out);
    crate::backend::copyfile::test_force_exdev(false);
    fat(false);
    let lines: Vec<(&str, bool)> = s.items.iter().map(|(name, ok, _)| (name.as_str(), *ok)).collect();
    assert_eq!(lines, [("a.txt", true), ("b.txt", true), ("t1", false), ("c.txt", true), ("t2", false), ("e.txt", true)],
        "two batches close around the folders, and each folder fails alone");
    assert_eq!((s.ok, s.failed), (4, 2));
    assert_eq!((s.items[2].2.as_str(), s.items[4].2.as_str()),
        ("link could not be moved: this drive cannot hold links", "sub/link could not be moved: this drive cannot hold links"));
    for leaf in files {
        assert!(!src.join(leaf).exists() && out.join(leaf).is_file(), "{leaf} moved through the batch");
    }
    for (tree, link) in trees {
        assert!(src.join(link).symlink_metadata().unwrap().file_type().is_symlink(), "{tree} keeps the link it could not move");
        assert!(src.join(tree).join("sub/x.txt").is_file(), "and every file it copied");
        assert_eq!(std::fs::read_to_string(out.join(tree).join("sub/x.txt")).unwrap(), "x", "{tree}'s files landed past the link");
        assert!(out.join(link).symlink_metadata().is_err(), "and its link is nowhere on the drive");
    }
    let mut names: Vec<&str> = s.refused.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["t1/link", "t2/sub/link"]);
    let mut j = Journal::new();
    j.push(s.entry);
    assert_eq!(j.undo().expect("undo"), "move");
    assert!(files.iter().all(|leaf| src.join(leaf).is_file()), "undo brings every moved file back");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "and takes every copy it landed off the drive");
}

// The done line names each refused entry under the folder it was copied into, and says nothing extra when none was.
#[test]
fn the_done_line_names_each_refused_entry_and_its_reason() {
    let d = TestDir::new("unheldline");
    let src = tree_with_links(&d);
    let dest = d.dir("out");
    let (tx, rx) = channel();
    fat(true);
    run_transfer(4, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    fat(false);
    let refused = rx.iter().find_map(|m| match m { OpMsg::TransferDone { refused, .. } => Some(refused), _ => None }).expect("a done line");
    let mut names: Vec<&str> = refused.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["tree/broken", "tree/link-file", "tree/sub/link-dir"]);
    assert!(refused.iter().all(|(_, why)| why == super::LINKS));
    let line = crate::backend::opsreq::transferdone_line(4, crate::backend::opsreq::TransferCounts { ok: 0, failed: 1, skipped: 0 }, false, &[], false, "", &refused[..1]);
    assert!(line.ends_with(&format!(r#","refused":[{{"name":"{}","err":"this drive cannot hold links"}}]}}"#, refused[0].0)), "{line}");
    let clean = crate::backend::opsreq::transferdone_line(4, crate::backend::opsreq::TransferCounts { ok: 1, failed: 0, skipped: 0 }, false, &[], false, "", &[]);
    assert!(!clean.contains("refused"), "a transfer nothing was refused in keeps its line byte for byte: {clean}");
}

// A link at the top of the selection is the item's own failure, said plainly, with no folder to name it inside.
#[test]
fn a_link_at_the_top_fails_its_own_item_with_the_drive_reason() {
    let d = TestDir::new("unheldtop");
    std::fs::write(d.join("a.txt"), "a").unwrap();
    let link = d.join("link");
    std::os::unix::fs::symlink("a.txt", &link).unwrap();
    let dest = d.dir("out");
    fat(true);
    let (items, ok, failed, entry) = transferred(&link, &dest);
    fat(false);
    assert_eq!((ok, failed), (0, 1));
    assert_eq!(items, vec![("link".to_string(), false, super::LINKS.to_string())]);
    assert!(entry.steps.is_empty(), "nothing was created, so nothing is journaled");
}

// What landed of a tree with refused entries is still confirmed on a durable drive before the done line.
#[test]
fn what_landed_beside_the_refused_entries_is_confirmed_before_done() {
    use crate::backend::durable::{test_mark_durable, test_reset};
    test_reset();
    let d = TestDir::new("unhelddurable");
    let src = tree_with_links(&d);
    let dest = d.dir("out");
    test_mark_durable(&dest);
    let (tx, rx) = channel();
    fat(true);
    run_transfer(6, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    fat(false);
    let mut writing = false;
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Meta { line } if line.contains(r#""phase":"writing""#) => writing = true,
            OpMsg::TransferDone { durable, ok, failed, .. } => done = Some((durable, ok, failed)),
            _ => {}
        }
    }
    test_reset();
    assert!(writing, "the landed files go through the final flush");
    assert_eq!(done, Some((true, 0, 1)), "the item failed, and what landed of it is on the drive");
}
