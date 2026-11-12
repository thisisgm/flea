use super::*;
use crate::backend::{menu_actions::Selected, menudelete, testdir::TestDir};
use std::cell::Cell;

const TEST_LIMIT: usize = 4;
const EMFILE: i32 = 24;
thread_local! {
    static LIMIT: Cell<Option<usize>> = const { Cell::new(None) };
    static FAIL_OPEN: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn limit(default: usize) -> usize {
    LIMIT.with(|limit| limit.get().unwrap_or(default))
}

pub(super) fn set_limit(limit: usize) {
    LIMIT.with(|value| value.set(Some(limit)));
}

pub(super) fn simulated_fuse() -> bool {
    LIMIT.with(|limit| limit.get().is_some())
}

pub(super) fn open_error() -> Option<std::io::Error> {
    FAIL_OPEN
        .with(|fail| fail.replace(false))
        .then(|| std::io::Error::from_raw_os_error(EMFILE))
}

// Each child owns its process-wide count; unrelated parallel tests cannot consume its permits.
fn in_child(name: &str) -> bool {
    const CHILD: &str = "FLEA_TEST_DELETE_HOLD_CHILD";
    if std::env::var_os(CHILD).as_deref() == Some(OsStr::new(name)) {
        LIMIT.with(|limit| limit.set(Some(TEST_LIMIT)));
        assert_eq!(held(), 0);
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("backend::trashdelete::file_claim::hold_tests::{name}"),
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

fn held() -> usize {
    HELD_REVIEW_HANDLES.load(Ordering::Relaxed)
}

fn files(d: &TestDir, prefix: &str, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|index| d.file(&format!("{prefix}-{index}"), "selected payload"))
        .collect()
}

fn selected(paths: &[PathBuf]) -> Vec<Selected> {
    paths
        .iter()
        .map(|path| Selected::inspect(path.to_str().unwrap()).unwrap())
        .collect()
}

fn descriptor_count(paths: &[PathBuf]) -> usize {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .filter(|path| paths.contains(path))
        .count()
}

fn prepare(d: &TestDir, path: &Path) -> PathReview {
    PathReview::prepare(
        path.to_path_buf(),
        &mut Manifest::new(d.path()).unwrap(),
        d.path(),
        &Cancellation::default(),
    )
    .unwrap()
    .0
}

#[test]
fn review_holds_are_bounded_and_released_after_success_and_refusal() {
    if in_child("review_holds_are_bounded_and_released_after_success_and_refusal") {
        return;
    }
    let d = TestDir::new("delete-review-hold-cap");
    let cancel = Cancellation::default();
    let paths = files(&d, "success", TEST_LIMIT * 3);
    let review = menudelete::Review::prepare(&selected(&paths), d.path(), &cancel).unwrap();
    assert_eq!(review.count, paths.len());
    assert_eq!(
        descriptor_count(&paths),
        TEST_LIMIT,
        "retained review descriptor count must equal the cap"
    );
    assert_eq!(held(), TEST_LIMIT);
    let answer = review.delete(&d.dir("recovery"), &cancel).unwrap();
    assert!(answer.contains(r#""deleted":12,"failed":0"#), "{answer}");
    assert!(paths.iter().all(|path| !path.exists()));
    assert_eq!(held(), TEST_LIMIT);
    drop(review);
    assert_eq!(held(), 0);

    let paths = files(&d, "refused", TEST_LIMIT * 3);
    let review = menudelete::Review::prepare(&selected(&paths), d.path(), &cancel).unwrap();
    assert_eq!(descriptor_count(&paths), TEST_LIMIT);
    std::fs::write(&paths[0], "changed after review").unwrap();
    assert!(review.delete(d.path(), &cancel).is_err());
    assert!(paths.iter().all(|path| path.exists()));
    assert_eq!(held(), TEST_LIMIT);
    drop(review);
    assert_eq!(held(), 0);
    assert_eq!(descriptor_count(&paths), 0);
}

#[test]
fn over_cap_reviews_keep_both_preclaim_and_claim_window_identity_checks() {
    if in_child("over_cap_reviews_keep_both_preclaim_and_claim_window_identity_checks") {
        return;
    }
    let d = TestDir::new("delete-review-hold-swap");
    let fill: Vec<_> = files(&d, "held", TEST_LIMIT)
        .iter()
        .map(|path| prepare(&d, path))
        .collect();
    assert_eq!(held(), TEST_LIMIT);
    for during_claim in [false, true] {
        let prefix = if during_claim { "during" } else { "before" };
        let path = d.file(prefix, "selected payload");
        let metadata = path.metadata().unwrap();
        let review = prepare(&d, &path);
        assert!(review.held.is_none());
        let replacement = d.file(&format!("{prefix}-replacement"), "different bytes!");
        File::open(&replacement)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        let saved = d.join(&format!("{prefix}-saved"));
        let swap = || {
            std::fs::rename(&path, &saved).unwrap();
            std::fs::rename(&replacement, &path).unwrap();
        };
        let refused = if during_claim {
            review.delete_after(&d.dir(&format!("{prefix}-recovery")), swap)
        } else {
            swap();
            review.delete(d.path())
        }
        .unwrap_err();
        assert_eq!(refused.outcome(), &Outcome::ItemChanged);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "different bytes!");
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "selected payload");
        assert_eq!(held(), TEST_LIMIT);
    }
    drop(fill);
    assert_eq!(held(), 0);
}

#[test]
fn pending_reviews_share_one_process_wide_hold_cap() {
    if in_child("pending_reviews_share_one_process_wide_hold_cap") {
        return;
    }
    let d = TestDir::new("delete-review-hold-shared");
    let first = files(&d, "first", TEST_LIMIT - 1);
    let second = files(&d, "second", TEST_LIMIT + 1);
    let cancel = Cancellation::default();
    let first_review = menudelete::Review::prepare(&selected(&first), d.path(), &cancel).unwrap();
    assert_eq!(held(), TEST_LIMIT - 1);
    let second_review = menudelete::Review::prepare(&selected(&second), d.path(), &cancel).unwrap();
    assert_eq!(held(), TEST_LIMIT);
    assert_eq!(descriptor_count(&first), TEST_LIMIT - 1);
    assert_eq!(descriptor_count(&second), 1);
    drop(first_review);
    assert_eq!(held(), 1);
    drop(second_review);
    assert_eq!(held(), 0);
}

#[test]
fn refused_prepare_releases_its_hold_and_reports_the_operating_system_error() {
    if in_child("refused_prepare_releases_its_hold_and_reports_the_operating_system_error") {
        return;
    }
    let d = TestDir::new("delete-review-hold-error");
    let path = d.file("chosen", "selected payload");
    let mut wrong = identity(&path).unwrap();
    wrong.ino ^= 1;
    let refusal = FileClaim::for_review(&path, &wrong).err().unwrap();
    assert_eq!(refusal.outcome(), &Outcome::ItemChanged);
    assert_eq!(held(), 0);
    FAIL_OPEN.with(|fail| fail.set(true));
    let error = menudelete::Review::prepare(
        &selected(std::slice::from_ref(&path)),
        d.path(),
        &Cancellation::default(),
    )
    .err()
    .unwrap();
    assert_eq!(
        error,
        "Could not prepare the deletion, so nothing was deleted: too many files are open."
    );
    assert_eq!(held(), 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "selected payload");
    assert_eq!(descriptor_count(&[path]), 0);
}
