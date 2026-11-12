use super::*;
use crate::backend::testdir::TestDir;
use std::cell::RefCell;

struct Sequence {
    reads: usize,
    replacement: Option<PathBuf>,
    renumber_first: bool,
}

thread_local! {
    static SEQUENCE: RefCell<Option<Sequence>> = const { RefCell::new(None) };
}

pub(super) fn read(path: &Path, result: std::io::Result<Identity>) -> std::io::Result<Identity> {
    SEQUENCE.with_borrow_mut(|sequence| {
        let Some(sequence) = sequence else {
            return result;
        };
        let mut value = result?;
        if sequence.reads == 0 {
            if let Some(replacement) = sequence.replacement.take() {
                std::fs::rename(replacement, path)?;
            }
            if sequence.renumber_first {
                value.ino ^= 1 << 63;
            }
        } else {
            value.ino ^= 1 << 63;
        }
        sequence.reads += 1;
        Ok(value)
    })
}

fn in_child(name: &str) -> bool {
    const CHILD: &str = "FLEA_TEST_DELETE_CACHE_CHILD";
    if std::env::var_os(CHILD).as_deref() == Some(OsStr::new(name)) {
        assert_eq!(HELD_REVIEW_HANDLES.load(Ordering::Relaxed), 0);
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("backend::trashdelete::file_claim::cache_tests::{name}"),
            "--nocapture",
        ])
        .env(CHILD, name)
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && report.contains("1 passed"),
        "{report}"
    );
    true
}

fn exercise_late_transition(retained: bool, swap: bool, renumber_first: bool) {
    let d = TestDir::new("delete-late-cache");
    let chosen = d.file("chosen", "selected payload");
    let expected = identity(&chosen).unwrap();
    hold_tests::set_limit(usize::from(retained));
    let reviewed = FileClaim::for_review(&chosen, &expected).unwrap();
    assert_eq!(reviewed.is_some(), retained);
    let parent = open_dir(d.path()).unwrap();
    let claim = reviewed.as_ref().map_or_else(
        || FileClaim::open(&parent, OsStr::new("chosen"), &expected).unwrap(),
        |claim| claim.duplicate().unwrap(),
    );
    let stage_path = d.dir("stage");
    let stage = open_dir(&stage_path).unwrap();
    let payload = stage_path.join("payload");
    let saved = d.join("selected-link");
    std::fs::hard_link(&chosen, &saved).unwrap();
    rename(&parent, OsStr::new("chosen"), &stage, OsStr::new("payload")).unwrap();
    let replacement = swap.then(|| {
        let replacement = d.file("replacement", "different bytes!");
        File::open(&replacement)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH
                    + std::time::Duration::new(expected.mtime as u64, expected.nanos as u32),
            ))
            .unwrap();
        assert!(same_metadata(&expected, &identity(&replacement).unwrap()));
        replacement
    });
    SEQUENCE.with_borrow_mut(|sequence| {
        *sequence = Some(Sequence {
            reads: 0,
            replacement,
            renumber_first,
        })
    });
    let result = claim.remove(&stage);
    let reads = SEQUENCE.with_borrow_mut(|sequence| sequence.take().unwrap().reads);
    if swap {
        assert_eq!(result.unwrap_err().outcome(), &Outcome::ItemChanged);
        assert_eq!(
            std::fs::read_to_string(&payload).unwrap(),
            "different bytes!"
        );
    } else {
        result.unwrap();
        assert!(!payload.exists());
    }
    assert_eq!(reads, 2);
    assert_eq!(std::fs::read_to_string(saved).unwrap(), "selected payload");
    drop(reviewed);
    assert_eq!(HELD_REVIEW_HANDLES.load(Ordering::Relaxed), 0);
}

#[test]
fn permanent_delete_accepts_late_cached_inode_transition_held_and_over_cap() {
    if in_child("permanent_delete_accepts_late_cached_inode_transition_held_and_over_cap") {
        return;
    }
    for retained in [true, false] {
        exercise_late_transition(retained, false, false);
    }
}

#[test]
fn permanent_delete_refuses_late_staged_swap_held_and_over_cap() {
    if in_child("permanent_delete_refuses_late_staged_swap_held_and_over_cap") {
        return;
    }
    for retained in [true, false] {
        exercise_late_transition(retained, true, false);
    }
}

#[test]
fn permanent_delete_accepts_a_stale_probe_held_and_over_cap() {
    if in_child("permanent_delete_accepts_a_stale_probe_held_and_over_cap") {
        return;
    }
    for retained in [true, false] {
        exercise_late_transition(retained, false, true);
    }
}

#[test]
fn permanent_delete_refuses_staged_swaps_with_a_stale_probe_held_and_over_cap() {
    if in_child("permanent_delete_refuses_staged_swaps_with_a_stale_probe_held_and_over_cap") {
        return;
    }
    for retained in [true, false] {
        exercise_late_transition(retained, true, true);
    }
}
