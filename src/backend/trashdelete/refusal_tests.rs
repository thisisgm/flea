// One test per Trash outcome: the sentence a person reads and the detail the log keeps are both produced.
use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::trashmanifest::{logged, Cancellation, Manifest, Op, Outcome, Refusal};
use std::os::unix::fs::PermissionsExt;

fn guard(d: &TestDir, path: &Path) {
    assert!(path.is_absolute() && !path.as_os_str().is_empty() && path.starts_with(d.path()) && path != d.path());
}
fn review(d: &TestDir, path: &Path) -> Reviewed {
    guard(d, path);
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed = Reviewed::inspect(path.to_path_buf(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    reviewed
}
// A reviewed Trash file whose .trashinfo names `original`.
fn fixture(d: &TestDir, original: &Path) -> (PathBuf, Reviewed) {
    d.dir("files");
    d.dir("info");
    let path = d.file("files/item", "payload");
    d.file("info/item.trashinfo", &format!("[Trash Info]\nPath={}\n", original.display()));
    let reviewed = review(d, &path);
    (path, reviewed)
}
// The log lines written after `seen` lines were already there, so a test never reads another's.
fn since(seen: usize) -> Vec<String> {
    logged().split_off(seen)
}
fn assert_logged(seen: usize, detail: &str) {
    assert!(since(seen).iter().any(|line| line.contains(detail)), "the log lacks {detail:?}: {:?}", since(seen));
}
fn quarantine_in(parent: &Path) -> PathBuf {
    let found = std::fs::read_dir(parent).unwrap().flatten()
        .find(|entry| entry.file_name().to_string_lossy().starts_with(".flea-delete-") && entry.path().is_dir());
    found.expect("the quarantine exists while the claim runs").path()
}

#[test]
fn a_review_that_is_not_there_says_the_trash_changed() {
    let d = TestDir::new("refusal-changed");
    let (path, _) = fixture(&d, &d.join("restored"));
    let meta = path.symlink_metadata().unwrap();
    let unreviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    let seen = logged().len();
    let refused = unreviewed.delete(d.path()).unwrap_err();
    assert_eq!(refused.shown(), "The Trash changed, so nothing was done. Try again.");
    assert_logged(seen, "Trash contents have not been reviewed.");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}

#[test]
fn an_item_replaced_before_the_claim_asks_to_confirm_again() {
    let d = TestDir::new("refusal-confirm");
    let (path, reviewed) = fixture(&d, &d.join("restored"));
    std::fs::rename(&path, d.join("files/old")).unwrap();
    std::fs::write(&path, "replacement").unwrap();
    let seen = logged().len();
    let refused = reviewed.delete(d.path()).unwrap_err();
    assert_eq!(refused.shown(), "The Trash changed, so nothing was done; confirm again.");
    assert_logged(seen, "Trash item changed; confirm again.");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
}

#[test]
fn an_arrival_after_the_claim_says_nothing_was_deleted() {
    let d = TestDir::new("refusal-nothing-deleted");
    d.dir("files/item");
    d.dir("info");
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    d.file("files/item/child", "reviewed");
    let path = d.join("files/item");
    let reviewed = review(&d, &path);
    let seen = logged().len();
    let refused = reviewed.delete_with(d.path(), || {}, |claimed| std::fs::write(claimed.join("arrival"), "keep").unwrap()).unwrap_err();
    assert_eq!(refused.shown(), "The Trash changed, so nothing was deleted.");
    assert_logged(seen, "Trash contents changed; no deletion attempted.");
    assert_eq!(std::fs::read_to_string(path.join("arrival")).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(path.join("child")).unwrap(), "reviewed");
}

#[test]
fn a_path_replaced_after_the_confirmation_says_the_item_changed() {
    let d = TestDir::new("refusal-item-changed");
    let path = d.file("selected", "reviewed");
    let (reviewed, _) = PathReview::prepare(path.clone(), &mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    guard(&d, &path);
    let seen = logged().len();
    let refused = reviewed.delete_after(d.path(), || {
        std::fs::rename(&path, d.join("old-selected")).unwrap();
        std::fs::write(&path, "replacement").unwrap();
    }).unwrap_err();
    assert_eq!(refused.shown(), "The item changed after you confirmed, so nothing was deleted.");
    assert_logged(seen, "Item changed after confirmation; no deletion attempted.");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
}

#[test]
fn a_damaged_trash_record_says_nothing_was_restored() {
    let d = TestDir::new("refusal-damaged");
    let original = d.join("restored");
    let (path, _) = fixture(&d, &original);
    std::fs::write(d.join("info/item.trashinfo"), "[Trash Info]\nPath=/one\nPath=/two\n").unwrap();
    let reviewed = review(&d, &path);
    let seen = logged().len();
    let refused = reviewed.restore(&original, d.path()).unwrap_err();
    assert_eq!(refused.shown(), "The Trash record for this item is damaged, so nothing was restored.");
    assert_logged(seen, "Trash metadata contains duplicate original paths.");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}

#[test]
fn a_restore_to_another_place_says_the_original_folder_changed() {
    let d = TestDir::new("refusal-original");
    let (path, reviewed) = fixture(&d, &d.join("restored"));
    let wrong = d.join("wrong");
    let seen = logged().len();
    let refused = reviewed.restore(&wrong, d.path()).unwrap_err();
    assert_eq!(refused.shown(), "The item's original folder changed; select it again.");
    assert_logged(seen, "The Trash item's original location changed; refresh the selection.");
    assert!(!wrong.exists());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}

#[test]
fn a_name_taken_at_the_original_place_keeps_the_item_in_the_trash() {
    let d = TestDir::new("refusal-blocked");
    let original = d.file("restored", "keep");
    let (path, reviewed) = fixture(&d, &original);
    let seen = logged().len();
    let refused = reviewed.restore(&original, d.path()).unwrap_err();
    assert_eq!(refused.shown(), "The item could not be restored: already exists. It stays in the Trash.");
    assert_logged(seen, "without overwriting: already exists");
    assert_eq!(std::fs::read_to_string(original).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}

// corner: these three run as a plain user, where a folder without its write bit refuses the claim and the removals.
fn locked(d: &TestDir, name: &str) -> PathBuf {
    let path = d.dir(name);
    d.file(&format!("{name}/child.txt"), "survive");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o555)).unwrap();
    path
}

#[test]
fn a_locked_folder_refuses_the_claim_before_anything_is_deleted_or_restored() {
    let d = TestDir::new("refusal-prepare");
    d.dir("files");
    d.dir("info");
    let path = locked(&d, "files/item");
    d.file("info/item.trashinfo", &format!("[Trash Info]\nPath={}\n", d.join("restored").display()));
    let reviewed = review(&d, &path);
    let seen = logged().len();
    let deleted = reviewed.delete(d.path());
    let restored = reviewed.restore(&d.join("restored"), d.path());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (deleted, restored) = (deleted.unwrap_err(), restored.unwrap_err());
    assert_eq!(deleted.shown(), "Could not prepare the deletion, so nothing was deleted: permission denied.");
    assert_eq!(restored.shown(), "Could not prepare the restore, so nothing was restored: permission denied.");
    assert_logged(seen, "Could not claim Trash item: permission denied");
    assert_eq!(std::fs::read_to_string(path.join("child.txt")).unwrap(), "survive");
}

#[test]
fn a_locked_child_leaves_the_rest_in_the_trash_and_says_so() {
    let d = TestDir::new("refusal-partial");
    d.dir("files/item");
    d.dir("info");
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    let inner = locked(&d, "files/item/inner");
    let reviewed = review(&d, &d.join("files/item"));
    let seen = logged().len();
    let refused = reviewed.delete(d.path());
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755)).unwrap();
    let refused = refused.unwrap_err();
    assert_eq!(refused.shown(), "The item could not be fully deleted: permission denied. What is left stays in the Trash, and a folder may be partly deleted.");
    assert_logged(seen, "Trash child could not be claimed");
    assert_eq!(std::fs::read_to_string(inner.join("child.txt")).unwrap(), "survive");
}

#[test]
fn a_locked_child_of_an_ordinary_path_stays_where_it_was() {
    let d = TestDir::new("refusal-partial-path");
    d.dir("selected");
    let inner = locked(&d, "selected/inner");
    let (reviewed, _) = PathReview::prepare(d.join("selected"), &mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    let seen = logged().len();
    let refused = reviewed.delete(d.path());
    std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755)).unwrap();
    let refused = refused.unwrap_err();
    assert_eq!(refused.shown(), "The item could not be fully deleted: permission denied. What is left stays where it was, and a folder may be partly deleted.");
    assert_logged(seen, "Trash child could not be claimed");
    assert_eq!(std::fs::read_to_string(inner.join("child.txt")).unwrap(), "survive");
}

#[test]
fn a_record_that_cannot_be_kept_stops_the_step_before_it_starts() {
    let d = TestDir::new("refusal-norecord");
    let original = d.join("restored");
    let (path, reviewed) = fixture(&d, &original);
    let seen = logged().len();
    let deleted = reviewed.delete(Path::new("relative-recovery")).unwrap_err();
    let restored = reviewed.restore(&original, Path::new("relative-recovery")).unwrap_err();
    assert_eq!(deleted.shown(), "Flea could not keep a record of this deletion, so nothing was deleted: the recovery folder is not valid.");
    assert_eq!(restored.shown(), "Flea could not keep a record of this restore, so nothing was restored: the recovery folder is not valid.");
    assert_logged(seen, "Recovery storage requires an absolute folder.");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}

#[test]
fn an_item_that_cannot_go_back_names_where_the_rest_is_kept() {
    let d = TestDir::new("refusal-stuck");
    let (path, reviewed) = fixture(&d, &d.join("restored"));
    let seen = logged().len();
    let refused = reviewed.with_claimed(Op::Delete, d.path(), None, || {}, |_| {
        std::fs::write(&path, "squatter").unwrap();
        Err(Refusal::changed("forced"))
    }).unwrap_err();
    let shown = refused.shown();
    assert!(shown.starts_with("The deletion could not finish: the contents changed. The remaining items are kept at "), "{shown}");
    assert!(shown.contains(".flea-delete-") && shown.ends_with('.'), "{shown}");
    assert_logged(seen, "forced");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "squatter");
}

#[test]
fn a_leftover_in_the_quarantine_is_named_with_the_trash_or_the_original_place() {
    let d = TestDir::new("refusal-stuck-trash");
    let (_, reviewed) = fixture(&d, &d.join("restored"));
    let seen = logged().len();
    let refused = reviewed.with_claimed(Op::Restore, d.path(), None, || {}, |quarantine| {
        std::fs::write(fd_path(quarantine, OsStr::new("stray")), "x").unwrap();
        Err(Refusal::changed("forced"))
    }).unwrap_err();
    let shown = refused.shown();
    assert!(shown.starts_with("The restore could not finish: the contents changed. The remaining items are kept in the Trash and at "), "{shown}");
    assert_logged(seen, "forced");
    let path = d.file("selected", "reviewed");
    let (review, _) = PathReview::prepare(path.clone(), &mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    let refused = review.delete_after(d.path(), || {
        std::fs::write(quarantine_in(d.path()).join("stray"), "x").unwrap();
        std::fs::rename(&path, d.join("old-selected")).unwrap();
        std::fs::write(&path, "replacement").unwrap();
    }).unwrap_err();
    let shown = refused.shown();
    assert!(shown.starts_with("The deletion could not finish: the item changed. The remaining items are kept where they were and at "), "{shown}");
    assert_logged(seen, "Item changed after confirmation; no deletion attempted.");
}

#[test]
fn a_quarantine_that_still_holds_items_names_where_they_are_kept() {
    let d = TestDir::new("refusal-remains");
    let (path, reviewed) = fixture(&d, &d.join("restored"));
    let quarantine = d.dir("files/.flea-delete-remains");
    let (files, held) = (open_dir(&d.join("files")).unwrap(), open_dir(&quarantine).unwrap());
    let mut journal = Recovery::begin(&d.dir("recovery"), &path, &files, &reviewed.payload, None, &quarantine, &held, None, None).unwrap();
    d.file("files/.flea-delete-remains/stray", "x");
    let refused = journal.remove_empty(&quarantine, &held, &files).unwrap_err();
    assert_eq!(refused.shown(), format!("Flea found an interrupted Trash operation; its items are kept at {}.", quarantine.display()));
}

#[test]
fn a_recovery_record_that_changed_is_named_after_the_item_was_deleted() {
    let d = TestDir::new("refusal-record-left");
    let (path, reviewed) = fixture(&d, &d.join("restored"));
    let seen = logged().len();
    let refused = reviewed.with_claimed(Op::Delete, d.path(), None, || {}, |quarantine| {
        std::fs::remove_file(fd_path(quarantine, OsStr::new("payload"))).unwrap();
        let record = std::fs::read_dir(d.path()).unwrap().flatten().map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "review")).expect("the journal exists while the claim runs");
        std::fs::rename(&record, d.join("old.review")).unwrap();
        std::fs::write(&record, "replacement").unwrap();
        Ok(())
    }).unwrap_err();
    assert_eq!(refused.shown(), "The item was deleted, but Flea could not remove its recovery record: the record changed.");
    assert_logged(seen, "Recovery record changed; it was preserved.");
    assert!(!path.exists());
}

#[test]
fn a_failed_removal_keeps_its_own_sentence_when_its_record_will_not_clear() {
    let d = TestDir::new("refusal-settle");
    let (path, reviewed) = fixture(&d, &d.join("restored"));
    let seen = logged().len();
    let refused = reviewed.with_claimed(Op::Delete, d.path(), None, || {}, |_| {
        let record = std::fs::read_dir(d.path()).unwrap().flatten().map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "review")).expect("the journal exists while the claim runs");
        std::fs::rename(&record, d.join("old.review")).unwrap();
        std::fs::write(&record, "replacement").unwrap();
        Err(Refusal::changed("forced removal failure"))
    }).unwrap_err();
    assert_eq!(refused.shown(), "The item could not be fully deleted: the contents changed. What is left stays in the Trash, and a folder may be partly deleted.");
    assert_logged(seen, "Recovery record changed; it was preserved.");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}

#[test]
fn a_child_that_changed_is_named_as_the_cause() {
    let d = TestDir::new("refusal-child");
    d.dir("files/item");
    d.dir("info");
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    let child = d.file("files/item/child", "reviewed");
    let reviewed = review(&d, &d.join("files/item"));
    let tree = reviewed.tree.as_ref().unwrap();
    let mut offset = tree.start();
    let node = loop {
        let node = Node::decode(&tree.next(&mut offset).unwrap().unwrap()).unwrap();
        if !node.directory() { break node; }
    };
    let seen = logged().len();
    let refused = reviewed.with_claimed(Op::Delete, d.path(), None, || {}, |quarantine| {
        std::fs::write(fd_path(quarantine, OsStr::new("payload/child")), "changed size").unwrap();
        let (parent, name) = parent_at(quarantine, OsStr::new("payload"), &node.relative)?;
        node.claim_remove(&parent, &name, quarantine, 0)
    }).unwrap_err();
    assert_eq!(refused.shown(), "The item could not be fully deleted: an item in the Trash changed. What is left stays in the Trash, and a folder may be partly deleted.");
    assert_logged(seen, "Trash child changed; remaining data was preserved.");
    assert_eq!(std::fs::read_to_string(child).unwrap(), "changed size");
}

#[test]
fn a_superseded_review_says_it_was_cancelled_and_logs_it() {
    let d = TestDir::new("refusal-cancelled");
    let path = d.file("files-item", "payload");
    guard(&d, &path);
    let cancellation = Cancellation::default();
    let old = cancellation.next();
    cancellation.next();
    let seen = logged().len();
    let refused = PathReview::prepare(path.clone(), &mut Manifest::new(d.path()).unwrap(), d.path(), &old).err().unwrap();
    assert_eq!(refused.shown(), "Trash request cancelled.");
    assert_logged(seen, "Trash request cancelled.");
}

// The outcomes no filesystem fixture here can reach have their sentences pinned and their detail still reaches the log.
#[test]
fn the_outcomes_a_fixture_cannot_reach_still_say_one_sentence_and_log_their_detail() {
    let at = PathBuf::from("/trash/files/.flea-delete-1-1");
    let cases = [
        (Refusal::new(Outcome::Kept(at.clone()), "rollback failed"), "The Trash changed, so nothing was done. Kept items are in /trash/files/.flea-delete-1-1."),
        (Refusal::new(Outcome::ChildChanged, "child changed"), "An item in the Trash changed, so the rest was kept."),
        (Refusal::new(Outcome::ChildLeft, "child left").because("permission denied"), "Some items in the Trash folder could not be deleted: permission denied."),
        (Refusal::new(Outcome::RecordLeft(None), "record left").because("the record changed"), "The interrupted operation was recovered, but Flea could not remove its recovery record: the record changed."),
        (Refusal::new(Outcome::Held(at.clone()), "held"), "Flea found an interrupted Trash operation; its items are kept at /trash/files/.flea-delete-1-1."),
        (Refusal::new(Outcome::NothingMoved, "nothing moved"), "Flea found an interrupted Trash operation; nothing was moved."),
    ];
    for (refusal, sentence) in cases {
        let seen = logged().len();
        assert_eq!(refusal.shown(), sentence);
        assert_eq!(String::from(refusal.clone().logged()), sentence);
        assert_logged(seen, refusal.detail());
    }
}

#[path = "completion_refusal_tests.rs"]
mod completion;

// Base refused this item as not valid text; its review now saves, reloads and deletes the raw file and its record.
#[test]
fn a_raw_named_item_is_reviewed_saved_and_deleted_by_its_own_bytes() {
    let d = TestDir::new("trash-delete-raw");
    d.dir("files");
    d.dir("info");
    let path = d.join("files").join(OsStr::from_bytes(b"r\xff"));
    let info = d.join("info").join(OsStr::from_bytes(b"r\xff.trashinfo"));
    std::fs::write(&path, "payload").unwrap();
    std::fs::write(&info, "[Trash Info]\nPath=/original/r%FF\n").unwrap();
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    let mut manifest = Manifest::new(d.path()).unwrap();
    reviewed
        .snapshot(&mut manifest, d.path(), &Cancellation::default())
        .unwrap();
    let restored = Reviewed::from_saved(&reviewed.saved().unwrap(), &manifest).unwrap();
    assert!(
        restored
            .contents_unchanged(&Cancellation::default())
            .unwrap(),
        "the reloaded review names the raw file"
    );
    guard(&d, &path);
    restored.delete(d.path()).unwrap();
    assert!(
        !path.exists() && !info.exists(),
        "the raw item and its own record are gone"
    );
}
