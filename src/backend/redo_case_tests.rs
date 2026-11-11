// Redo on case-insensitive and exFAT drives: a case-only rename finds its own source, and exFAT moves a folder's birth time.
use super::*;
use crate::backend::testdir::TestDir;
use std::sync::mpsc::channel;

fn moved(from: &Path, to: &Path, dir: &Path) -> ReplayStep {
    let before = ItemIdentity::inspect(from).unwrap();
    let after = ItemIdentity::inspect(to).unwrap();
    let step = Step::Moved {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        before: before.clone(),
        after,
        rename_fat: false,
    };
    ReplayStep {
        step,
        input: Some(before),
        parent: Some((dir.to_path_buf(), ItemIdentity::inspect(dir).unwrap())),
    }
}

#[test]
fn a_destination_that_is_the_source_itself_is_free_for_a_redone_rename() {
    let sandbox = TestDir::new("redo-case-only");
    let lower = sandbox.file("name.txt", "payload");
    // A hard link stands in for the case-insensitive lookup: NAME.TXT answers with name.txt's own inode.
    let upper = sandbox.join("NAME.TXT");
    std::fs::hard_link(&lower, &upper).unwrap();
    let saved = moved(&lower, &upper, sandbox.path());
    assert!(
        Replay::check(&saved, false).is_ok(),
        "the source under its new spelling is not another item"
    );
    // The replay then takes the twin path, which refuses a real second name and keeps both.
    let replay = Replay {
        op: "rename".into(),
        steps: vec![saved],
    };
    let (tx, _rx) = channel();
    let (_, _, result) = replay.run(1, &AtomicBool::new(false), &tx);
    assert_eq!(
        result.unwrap_err().msg,
        "an item with that name is already here"
    );
    assert_eq!(std::fs::read_to_string(&lower).unwrap(), "payload");
    assert_eq!(std::fs::read_to_string(&upper).unwrap(), "payload");
}

#[test]
fn a_different_item_at_the_new_name_still_refuses_the_redo() {
    let sandbox = TestDir::new("redo-case-other");
    let lower = sandbox.file("name.txt", "payload");
    let upper = sandbox.file("NAME.TXT", "someone else");
    let saved = moved(&lower, &upper, sandbox.path());
    let refused = Replay::check(&saved, false).unwrap_err();
    assert_eq!(
        refused.msg,
        "an item with that name is already there, so redo left it in place"
    );
    assert_eq!(std::fs::read_to_string(&upper).unwrap(), "someone else");
}

#[test]
fn an_exfat_folder_whose_birth_time_moved_is_still_the_destination() {
    let folder =
        |ino, born| ItemIdentity::from_parts(7, ino, 0o040000, 4096, (1, 0), (1, 0), Some(born));
    let recorded = folder(42, (100, 0));
    let written = folder(42, (101, 0));
    assert!(
        same_folder(&recorded, &written, || Some(EXFAT_SUPER_MAGIC)),
        "exFAT moved the birth time itself"
    );
    assert!(
        !same_folder(&recorded, &written, || Some(0xEF53)),
        "everywhere else a new birth time is a new folder"
    );
    assert!(
        !same_folder(&recorded, &written, || None),
        "an unreadable filesystem keeps the strict rule"
    );
    assert!(
        !same_folder(&recorded, &folder(43, (101, 0)), || Some(EXFAT_SUPER_MAGIC)),
        "another inode is another folder on exFAT too"
    );
    let mut asked = false;
    assert!(same_folder(&recorded, &recorded.clone(), || {
        asked = true;
        None
    }));
    assert!(!asked, "an unchanged folder never pays the statfs");
}

#[test]
fn a_replace_trashing_a_txt_frees_a_txt_under_any_spelling_the_drive_resolves_to_it() {
    let sandbox = TestDir::new("redo-case-vacates");
    let lower = sandbox.file("a.txt", "old");
    // The hard link answers like the case-insensitive lookup of A.txt does: the trashed item itself.
    let upper = sandbox.join("A.txt");
    std::fs::hard_link(&lower, &upper).unwrap();
    let other = sandbox.file("b.txt", "other");
    let vacated: HashSet<&Path> = [lower.as_path()].into_iter().collect();
    assert!(vacates(&vacated, &lower));
    assert!(
        vacates(&vacated, &upper),
        "the incoming spelling lands where the trashed item was"
    );
    assert!(!vacates(&vacated, &other), "another item keeps its name");
    assert!(!vacates(&vacated, &sandbox.join("missing")));
}
