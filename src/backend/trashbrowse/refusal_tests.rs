// One test per Trash browse outcome: the sentence a person reads and the detail the log keeps are both produced.
use super::*;
use crate::backend::trashmanifest::{logged, Cancellation};
use std::os::unix::process::ExitStatusExt;

fn since(seen: usize) -> Vec<String> {
    logged().split_off(seen)
}
fn assert_logged(seen: usize, detail: &str) {
    assert!(since(seen).iter().any(|line| line.contains(detail)), "the log lacks {detail:?}: {:?}", since(seen));
}
fn reply(code: i32, stdout: &[u8], stderr: &str) -> std::io::Result<std::process::Output> {
    Ok(std::process::Output { status: std::process::ExitStatus::from_raw(code << 8), stdout: stdout.to_vec(), stderr: stderr.as_bytes().to_vec() })
}

#[test]
fn a_gio_that_fails_says_the_trash_could_not_be_read_and_logs_what_gio_said() {
    let seen = logged().len();
    let refused = answer(reply(1, b"", "Error: no such place\ngio: trash:///: Operation not supported\n")).unwrap_err();
    assert_eq!(refused.shown(), "The Trash could not be read.");
    assert_logged(seen, "gio: trash:///: Operation not supported");
    let seen = logged().len();
    assert_eq!(answer(reply(1, b"", "")).unwrap_err().shown(), "The Trash could not be read.");
    assert_logged(seen, "The gio Trash helper failed.");
    let seen = logged().len();
    assert_eq!(answer(reply(0, &[0xff, 0xfe], "")).unwrap_err().shown(), "The Trash could not be read.");
    assert_logged(seen, "The Trash helper returned invalid text.");
    let seen = logged().len();
    let missing = answer(Err(std::io::Error::from(std::io::ErrorKind::NotFound))).unwrap_err();
    assert_eq!(missing.shown(), "The Trash could not be read.");
    assert_logged(seen, "Could not start the Trash helper");
}

#[test]
fn a_listing_gio_garbled_says_the_trash_could_not_be_read_and_logs_which_line() {
    for (text, detail) in [("bad", "The Trash helper returned an unreadable listing."), ("file:///x\t/x", "The Trash helper returned an invalid item identity."),
        ("trash:///a\t/a\ntrash:///a\t/a", "The Trash helper returned duplicate item identities.")] {
        let seen = logged().len();
        assert_eq!(parse_list(text).unwrap_err().shown(), "The Trash could not be read.");
        assert_logged(seen, detail);
    }
}

#[test]
fn a_gio_that_hit_its_deadline_says_the_trash_took_too_long() {
    for code in [124, 137] {
        let seen = logged().len();
        let refused = answer(reply(code, b"", "")).unwrap_err();
        assert_eq!(refused.shown(), "The Trash took too long to answer.");
        assert_logged(seen, "The Trash helper exceeded its deadline.");
    }
}

#[test]
fn every_selection_that_no_longer_holds_says_the_selection_changed_and_logs_why() {
    let sentence = "The Trash selection changed; select the items again.";
    let mut session = Session { items: parse_list("trash:///a\t/tmp/a\n").unwrap(), ..Session::default() };
    let seen = logged().len();
    assert_eq!(session.targets(r#"{"uris":["trash:///a"],"identities":[]}"#).unwrap_err(), sentence);
    assert_eq!(session.targets(r#"{"uris":["trash:///a"],"identities":[""]}"#).unwrap_err(), sentence);
    assert_eq!(session.targets(r#"{"uris":["trash:///a"],"identities":["saved"],"selectionToken":4}"#).unwrap_err(), sentence);
    assert_eq!(session.targets(r#"{"uris":["trash:///gone"],"identities":["saved"]}"#).unwrap_err(), sentence);
    assert_logged(seen, "The reviewed identity for the Trash selection is missing.");
    assert_logged(seen, "The saved Trash selection is no longer current.");
    session.items.clear();
    let item = Detail { item: Item { uri: "trash:///a".into(), original: "/tmp/a".into() }, size: 1, deleted: String::new(),
        identity: "id".into(), directory: false, icon: String::new(), backing: None };
    let seen = logged().len();
    assert_eq!(require_selected_identity(&item, "").unwrap_err().shown(), sentence);
    assert_logged(seen, "Trash selection changed for trash:///a.");
}

#[test]
fn a_window_that_asks_if_a_selection_holds_logs_nothing() {
    let item = Detail { item: Item { uri: "trash:///a".into(), original: "/tmp/a".into() }, size: 1, deleted: String::new(),
        identity: "id".into(), directory: false, icon: String::new(), backing: None };
    let seen = logged().len();
    assert!(!selection_current(&item, ""));
    assert!(since(seen).is_empty());
}

// The flows that reach these two need a live gio Trash, so the tests call the constructors those flows use.
#[test]
fn items_that_cannot_be_told_apart_say_so_for_a_selection_and_for_an_action() {
    for (selecting, shown) in [(true, "Items in this Trash cannot be told apart, so they cannot be selected."),
        (false, "Items in this Trash cannot be told apart, so nothing was done.")] {
        let seen = logged().len();
        assert_eq!(indistinct(selecting).shown(), shown);
        assert_logged(seen, "The Trash provider did not report a stable item identity.");
    }
}

#[test]
fn a_trash_that_cannot_be_checked_names_what_nothing_could_do() {
    for (op, shown) in [(Op::Delete, "This Trash cannot be checked, so nothing can be deleted from it."),
        (Op::Restore, "This Trash cannot be checked, so nothing can be restored from it.")] {
        let seen = logged().len();
        assert_eq!(unchecked(op).shown(), shown);
        assert_logged(seen, "The backing identity for the Trash item is unavailable.");
    }
}

// One reviewed Trash file `name` under `d`, saved as the record a confirmation holds.
fn confirmed(d: &crate::backend::testdir::TestDir, trees: &mut Manifest, name: &str) -> (std::path::PathBuf, String) {
    use std::os::unix::fs::MetadataExt;
    d.dir("files");
    d.dir("info");
    let path = d.file(&format!("files/{name}"), "payload");
    d.file(&format!("info/{name}.trashinfo"), "[Trash Info]\nPath=/original\n");
    let meta = path.symlink_metadata().unwrap();
    let mut backing = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    backing.snapshot(trees, d.path(), &Cancellation::default()).unwrap();
    let item = Detail { item: Item { uri: format!("trash:///{name}"), original: format!("/original/{name}") }, size: 7, deleted: String::new(),
        identity: "id".into(), directory: false, icon: String::new(), backing: Some(backing) };
    (path, save_detail(&item).unwrap())
}

// A length that runs past the end of the file: the frame cannot be read forward, and the records behind it cannot be found that way.
fn damage(manifest: &Manifest, at: u64) {
    use std::os::unix::fs::FileExt;
    use std::os::unix::io::FromRawFd;
    let file = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(manifest.records().file_raw()) });
    file.write_all_at(&u64::MAX.to_le_bytes(), at).unwrap();
}
fn named(uri: &str) -> String {
    format!(r#"{{"uri":"{uri}","original":"/original/{}"}}"#, uri.rsplit('/').next().unwrap())
}
fn delete(confirmation: &Confirmation, d: &crate::backend::testdir::TestDir) -> String {
    delete_confirmed(confirmation, &Ok(d.path().to_path_buf()))
}

#[test]
fn a_record_that_will_not_read_after_the_confirmation_fails_its_item_and_the_rest_are_deleted() {
    let d = crate::backend::testdir::TestDir::new("browse-unread");
    let mut trees = Manifest::new(d.path()).unwrap();
    let (first, first_record) = confirmed(&d, &mut trees, "a");
    let (third, third_record) = confirmed(&d, &mut trees, "c");
    let mut items = Manifest::new(d.path()).unwrap();
    items.append(first_record.as_bytes()).unwrap();
    items.append(br#"{"uri":"trash:///b","original":"/original/b","size":7}"#).unwrap();
    items.append(third_record.as_bytes()).unwrap();
    let confirmation = Confirmation { token: 1, items, trees, listing: Vec::new(), targets: Manifest::new(d.path()).unwrap(), count: 3, nodes: 3, all: false };
    let reply = delete(&confirmation, &d);
    assert!(reply.starts_with(r#""done":2,"failed":1,"failures":["#), "{reply}");
    assert!(reply.contains(r#""uri":"trash:///b""#) && reply.contains("One Trash record could not be read, so that item was left in place."), "{reply}");
    assert!(!reply.contains("nothing was done"), "{reply}");
    assert!(!first.exists() && !third.exists());
}

#[test]
fn a_record_with_no_text_or_frame_fails_with_an_empty_uri() {
    let d = crate::backend::testdir::TestDir::new("browse-unread-bare");
    let mut trees = Manifest::new(d.path()).unwrap();
    let (first, first_record) = confirmed(&d, &mut trees, "a");
    let mut items = Manifest::new(d.path()).unwrap();
    items.append(first_record.as_bytes()).unwrap();
    items.append(&[0xff]).unwrap();
    let confirmation = Confirmation { token: 1, items, trees, listing: Vec::new(), targets: Manifest::new(d.path()).unwrap(), count: 2, nodes: 2, all: false };
    let reply = delete(&confirmation, &d);
    assert!(reply.starts_with(r#""done":1,"failed":1,"failures":[{"uri":"","#), "{reply}");
    assert!(!first.exists());
}

// Four confirmed files a to d, the item records in the order of the target records `order` names, with the frame of item number `damaged` broken.
fn four_confirmed(d: &crate::backend::testdir::TestDir, order: [&str; 4], damaged: usize, break_targets: &[usize]) -> (Confirmation, Vec<std::path::PathBuf>) {
    let mut trees = Manifest::new(d.path()).unwrap();
    let (mut items, mut targets) = (Manifest::new(d.path()).unwrap(), Manifest::new(d.path()).unwrap());
    let (mut paths, mut item_at, mut target_at) = (Vec::new(), Vec::new(), Vec::new());
    for name in order {
        let (path, record) = confirmed(d, &mut trees, name);
        item_at.push(items.len());
        items.append(record.as_bytes()).unwrap();
        target_at.push(targets.len());
        targets.append(named(&format!("trash:///{name}")).as_bytes()).unwrap();
        paths.push(path);
    }
    damage(&items, item_at[damaged]);
    for &at in break_targets { damage(&targets, target_at[at]); }
    (Confirmation { token: 1, items, trees, listing: Vec::new(), targets, count: 4, nodes: 4, all: false }, paths)
}

#[test]
fn a_damaged_frame_fails_every_confirmed_item_after_it_and_not_one_row() {
    let d = crate::backend::testdir::TestDir::new("browse-frame");
    let (confirmation, paths) = four_confirmed(&d, ["a", "b", "c", "d"], 1, &[]);
    let reply = delete(&confirmation, &d);
    assert!(reply.starts_with(r#""done":1,"failed":3,"failures":["#), "{reply}");
    assert!(["b", "c", "d"].iter().all(|name| reply.contains(&format!(r#""uri":"trash:///{name}""#))) && !reply.contains(r#""uri":"trash:///a""#), "{reply}");
    assert!(!paths[0].exists() && paths[1..].iter().all(|path| path.exists()));
}

// Targets written in an order that is not sorted, as Session::targets writes an explicit selection: the failures name the targets at the index the items reached, not the sorted ones.
#[test]
fn a_damaged_frame_names_the_targets_at_its_own_index_in_the_order_they_were_written() {
    let d = crate::backend::testdir::TestDir::new("browse-frame-order");
    let session = Session { items: parse_list("trash:///a\t/original/a\ntrash:///b\t/original/b\ntrash:///c\t/original/c\ntrash:///d\t/original/d\n").unwrap(),
        scratch: d.path().to_path_buf(), ..Session::default() };
    let written = session.targets(r#"{"uris":["trash:///d","trash:///b","trash:///c","trash:///a"],"identities":["x","x","x","x"]}"#).unwrap();
    let mut offset = 0;
    let mut order = Vec::new();
    while let Some(bytes) = written.records().next(&mut offset).unwrap() {
        order.push(record_item(&record_text(bytes).unwrap()).unwrap().uri);
    }
    assert_eq!(order, ["trash:///d", "trash:///b", "trash:///c", "trash:///a"]);
    let (mut confirmation, paths) = four_confirmed(&d, ["d", "b", "c", "a"], 1, &[]);
    confirmation.targets = written;
    let reply = delete(&confirmation, &d);
    assert!(reply.starts_with(r#""done":1,"failed":3,"failures":["#), "{reply}");
    assert!(["b", "c", "a"].iter().all(|name| reply.contains(&format!(r#""uri":"trash:///{name}""#))) && !reply.contains(r#""uri":"trash:///d""#), "{reply}");
    assert!(!paths[0].exists() && paths[1..].iter().all(|path| path.exists()));
}

#[test]
fn two_damaged_target_frames_still_name_what_can_be_read_and_count_the_rest() {
    let d = crate::backend::testdir::TestDir::new("browse-frame-targets");
    // Item b and target frames b and c are broken: a is deleted, d is named from the end back, and b and c are the two unnamed.
    let (confirmation, paths) = four_confirmed(&d, ["a", "b", "c", "d"], 1, &[1, 2]);
    let reply = delete(&confirmation, &d);
    assert!(reply.starts_with(r#""done":1,"failed":3,"failures":["#), "{reply}");
    assert!(reply.contains(r#""uri":"trash:///d""#) && !reply.contains(r#""uri":"trash:///a""#), "{reply}");
    assert!(reply.contains("2 Trash records could not be read, so those items were left in place."), "{reply}");
    assert!(!paths[0].exists() && paths[1..].iter().all(|path| path.exists()));
}

#[test]
fn a_damaged_frame_with_no_readable_targets_fails_every_item_it_left_in_one_row_and_counts_them() {
    let d = crate::backend::testdir::TestDir::new("browse-frame-bare");
    let (mut confirmation, _) = four_confirmed(&d, ["a", "b", "c", "d"], 0, &[]);
    confirmation.targets = Manifest::new(d.path()).unwrap();
    let reply = delete(&confirmation, &d);
    assert!(reply.starts_with(r#""done":0,"failed":4,"failures":[{"uri":"","#), "{reply}");
    assert!(reply.contains("4 Trash records could not be read, so those items were left in place."), "{reply}");
    assert_eq!(unread_many(1), unread(None));
}

#[test]
fn an_item_with_no_backing_review_says_nothing_can_be_deleted_from_this_trash() {
    let item = Detail { item: Item { uri: "trash:///a".into(), original: "/tmp/a".into() }, size: 1, deleted: String::new(),
        identity: "id".into(), directory: false, icon: String::new(), backing: None };
    let error = delete_one(&item, &Ok(std::path::PathBuf::from("/nonexistent-recovery")), &mut ThumbnailPurge(Vec::new())).unwrap_err();
    assert_eq!(error, "This Trash cannot be checked, so nothing can be deleted from it.");
    assert!(failure(&item.item, "", &error).contains(r#""error":"This Trash cannot be checked, so nothing can be deleted from it.""#));
}
