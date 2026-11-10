use super::*;
use crate::backend::testdir::TestDir;

// gio 2.90.1's target-uri listing for every files entry of one Trash, damaged ones and a claim's quarantine included.
fn reader(files: &Path) -> impl Fn(&[&str]) -> Result<String, Refusal> + '_ {
    move |args: &[&str]| {
        assert_eq!(
            args,
            [
                "list",
                "-u",
                "-h",
                "-a",
                "standard::target-uri",
                "trash:///"
            ]
        );
        let mut names: Vec<_> = std::fs::read_dir(files)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        Ok(names
            .iter()
            .map(|name| {
                let uri = crate::backend::thumbcache::uri_for(&files.join(name));
                format!(
                    "trash:///{}\t1\t(regular)\tstandard::target-uri={uri}\n",
                    uri["file:///".len()..].rsplit('/').next().unwrap()
                )
            })
            .collect())
    }
}
fn fixture(d: &TestDir) -> (PathBuf, Vec<Item>) {
    let trash = d.dir("Trash");
    d.dir("Trash/info");
    d.dir("Trash/files/folder/sub");
    d.file("Trash/files/folder/sub/y", "y");
    d.file("Trash/files/good.txt", "good");
    d.file(
        "Trash/info/good.txt.trashinfo",
        "[Trash Info]\nPath=/tmp/good.txt\nDeletionDate=2026-01-01T00:00:00\n",
    );
    d.file("Trash/files/no record.txt", "x");
    d.file("Trash/files/no path.txt", "x");
    d.file(
        "Trash/info/no path.txt.trashinfo",
        "[Trash Info]\nDeletionDate=2026-01-01T00:00:00\n",
    );
    d.file(
        "Trash/info/gone.txt.trashinfo",
        "[Trash Info]\nPath=/tmp/gone.txt\nDeletionDate=2026-01-01T00:00:00\n",
    );
    d.file("Trash/info/notes", "not a record");
    (
        trash,
        vec![Item {
            uri: "trash:///good.txt".into(),
            original: "/tmp/good.txt".into(),
        }],
    )
}
fn left(trash: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for folder in ["files", "info"] {
        for entry in std::fs::read_dir(trash.join(folder)).unwrap() {
            out.push(format!(
                "{folder}/{}",
                entry.unwrap().file_name().to_string_lossy()
            ));
        }
    }
    out.sort();
    out
}

// gio trash --empty on minipc's gio 2.90.1 removed every files entry with its record; the sweep does the same, plus records with no item.
#[test]
fn the_sweep_removes_every_left_out_entry_and_orphan_record_and_nothing_else() {
    let d = TestDir::new("trash-sweep");
    let (trash, listed) = fixture(&d);
    let quarantine = d.dir("Trash/files/.flea-delete-1-2");
    d.file("Trash/files/.flea-delete-1-2/payload", "held by a claim");
    let (scratch, recovery) = (d.dir("scratch"), d.join("recovery"));
    let mut trees = Manifest::new(&scratch).unwrap();
    let review = |trees: &mut Manifest| {
        Sweep::review(
            &listed,
            trees,
            &scratch,
            &Cancellation::default(),
            reader(&trash.join("files")),
        )
        .unwrap()
    };
    let sweep = review(&mut trees);
    assert_eq!(
        sweep
            .payloads
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>(),
        ["folder", "no path.txt", "no record.txt"].map(|name| trash.join("files").join(name)),
        "the listed item and the quarantine are not swept"
    );
    assert!(
        sweep.orphans.is_empty(),
        "a Trash holding a claim's quarantine keeps its records"
    );
    assert!(sweep.unchanged(&Cancellation::default()).unwrap());
    let mut thumbnails = ThumbnailPurge(Vec::new());
    assert_eq!(
        sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails),
        1,
        "the record kept beside the quarantine is a leftover the sweep could not remove"
    );
    assert_eq!(
        left(&trash),
        [
            "files/.flea-delete-1-2",
            "files/good.txt",
            "info/gone.txt.trashinfo",
            "info/good.txt.trashinfo",
            "info/notes"
        ]
    );
    assert_eq!(
        thumbnails.0.len(),
        3,
        "each swept item's thumbnails go in the request's one purge"
    );
    thumbnails.0.clear();
    // Once recovery has cleared the quarantine, the record with no item goes too.
    std::fs::remove_dir_all(&quarantine).unwrap();
    let sweep = review(&mut trees);
    assert_eq!(
        sweep
            .orphans
            .iter()
            .map(|o| o.path.clone())
            .collect::<Vec<_>>(),
        [trash.join("info/gone.txt.trashinfo")]
    );
    // A claim stuck between the review and the removal keeps the record as well.
    let late = d.dir("Trash/files/.flea-delete-3-4");
    assert_eq!(
        sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails),
        1
    );
    assert!(
        trash.join("info/gone.txt.trashinfo").exists(),
        "a quarantine that appeared after the review keeps its Trash's records"
    );
    std::fs::remove_dir(&late).unwrap();
    assert_eq!(
        sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails),
        0,
        "nothing is left over once every entry is gone"
    );
    assert_eq!(
        left(&trash),
        ["files/good.txt", "info/good.txt.trashinfo", "info/notes"]
    );
}

// A change after the review invalidates the confirmation, and the sweep never removes what changed.
#[test]
fn a_left_out_entry_that_changed_after_the_review_is_kept() {
    let d = TestDir::new("trash-sweep-changed");
    let (trash, listed) = fixture(&d);
    let (scratch, recovery) = (d.dir("scratch"), d.join("recovery"));
    let mut trees = Manifest::new(&scratch).unwrap();
    let mut review = || {
        Sweep::review(
            &listed,
            &mut trees,
            &scratch,
            &Cancellation::default(),
            reader(&trash.join("files")),
        )
        .unwrap()
    };
    let sweep = review();
    d.replace_file(
        &trash.join("files/no record.txt"),
        "rewritten after the review",
    );
    assert!(
        !sweep.unchanged(&Cancellation::default()).unwrap(),
        "a left-out entry rewritten after the review is a change"
    );
    let later = review();
    assert!(later.unchanged(&Cancellation::default()).unwrap());
    // A trash in flight writes its record first: the item arriving under an orphan record makes that record live again.
    d.file("Trash/files/gone.txt", "arrived");
    assert!(
        !later.unchanged(&Cancellation::default()).unwrap(),
        "an orphan record whose item arrived is a change"
    );
    let mut thumbnails = ThumbnailPurge(Vec::new());
    assert_eq!(
        sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails),
        1,
        "the rewritten entry is left over; the record whose item arrived is a listed item again"
    );
    thumbnails.0.clear();
    assert_eq!(
        left(&trash),
        [
            "files/gone.txt",
            "files/good.txt",
            "files/no record.txt",
            "info/gone.txt.trashinfo",
            "info/good.txt.trashinfo",
            "info/notes"
        ]
    );
    assert_eq!(
        std::fs::read_to_string(trash.join("files/no record.txt")).unwrap(),
        "rewritten after the review"
    );
    // A cancelled request sweeps nothing.
    let fresh = review();
    let cancelled = Cancellation::operation(std::sync::Arc::new(
        std::sync::atomic::AtomicBool::new(true),
    ));
    assert_eq!(
        fresh.remove(&recovery, &cancelled, &mut ThumbnailPurge(Vec::new())),
        0,
        "the review left nothing here, so a cancel before the first entry counts nothing"
    );
    assert!(trash.join("files/no record.txt").exists());
}

// TR2-c: a left-out payload's record rewritten after the review stays on disk, so it counts as left over.
#[test]
fn a_record_rewritten_after_the_review_is_left_over() {
    let d = TestDir::new("trash-sweep-record");
    let (trash, listed) = fixture(&d);
    let (scratch, recovery) = (d.dir("scratch"), d.join("recovery"));
    let mut trees = Manifest::new(&scratch).unwrap();
    let sweep = Sweep::review(
        &listed,
        &mut trees,
        &scratch,
        &Cancellation::default(),
        reader(&trash.join("files")),
    )
    .unwrap();
    d.replace_file(
        &trash.join("info/no path.txt.trashinfo"),
        "[Trash Info]\nDeletionDate=2026-02-02T00:00:00\n",
    );
    assert_eq!(
        sweep.remove(
            &recovery,
            &Cancellation::default(),
            &mut ThumbnailPurge(Vec::new())
        ),
        1
    );
    assert!(
        !trash.join("files/no path.txt").exists(),
        "the payload itself is removed"
    );
    assert!(trash.join("info/no path.txt.trashinfo").exists());
}

// TR2-c: what the review had to leave is counted when the sweep runs, so a record gone or live again by then is not reported.
#[test]
fn a_record_the_review_left_counts_only_while_it_is_still_an_orphan_on_disk() {
    let d = TestDir::new("trash-sweep-held");
    let (trash, listed) = fixture(&d);
    let quarantine = d.dir("Trash/files/.flea-delete-1-2");
    let (scratch, recovery) = (d.dir("scratch"), d.join("recovery"));
    let mut trees = Manifest::new(&scratch).unwrap();
    let sweep = Sweep::review(
        &listed,
        &mut trees,
        &scratch,
        &Cancellation::default(),
        reader(&trash.join("files")),
    )
    .unwrap();
    let cancelled = Cancellation::operation(std::sync::Arc::new(
        std::sync::atomic::AtomicBool::new(true),
    ));
    let count =
        |cancel: &Cancellation| sweep.remove(&recovery, cancel, &mut ThumbnailPurge(Vec::new()));
    assert_eq!(
        count(&cancelled),
        1,
        "a cancel before the first entry still counts the record the review left"
    );
    assert!(
        trash.join("files/no record.txt").exists(),
        "a cancelled sweep tries nothing"
    );
    let item = d.file("Trash/files/gone.txt", "put back");
    assert_eq!(
        count(&cancelled),
        0,
        "a record whose item was put back describes a listed item again"
    );
    std::fs::remove_file(item).unwrap();
    std::fs::remove_dir(quarantine).unwrap();
    std::fs::remove_file(trash.join("info/gone.txt.trashinfo")).unwrap();
    assert_eq!(
        count(&Cancellation::default()),
        0,
        "a record the other delete removed before the sweep is not left over"
    );
    assert!(!trash.join("files/no record.txt").exists());
}

// TR2-c: a left-out entry the review could not read stays in place and is counted.
#[test]
fn an_entry_the_review_could_not_read_is_left_over() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("trash-sweep-unread");
    let (trash, listed) = fixture(&d);
    let locked = trash.join("files/folder/sub");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (scratch, recovery) = (d.dir("scratch"), d.join("recovery"));
    let mut trees = Manifest::new(&scratch).unwrap();
    let sweep = Sweep::review(
        &listed,
        &mut trees,
        &scratch,
        &Cancellation::default(),
        reader(&trash.join("files")),
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let sweep = sweep.unwrap();
    assert_eq!(
        sweep
            .payloads
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>(),
        ["no path.txt", "no record.txt"].map(|name| trash.join("files").join(name)),
        "the folder the review could not read is not swept"
    );
    let left_over = sweep.remove(
        &recovery,
        &Cancellation::default(),
        &mut ThumbnailPurge(Vec::new()),
    );
    assert_eq!(
        left_over, 1,
        "only the folder the review could not read is left"
    );
    assert!(trash.join("files/folder/sub/y").exists());
}
