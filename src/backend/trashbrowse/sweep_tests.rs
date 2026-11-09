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
    sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails);
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
    sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails);
    assert!(
        trash.join("info/gone.txt.trashinfo").exists(),
        "a quarantine that appeared after the review keeps its Trash's records"
    );
    std::fs::remove_dir(&late).unwrap();
    sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails);
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
    sweep.remove(&recovery, &Cancellation::default(), &mut thumbnails);
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
    fresh.remove(&recovery, &cancelled, &mut ThumbnailPurge(Vec::new()));
    assert!(trash.join("files/no record.txt").exists());
}
