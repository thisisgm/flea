use super::*;
use crate::backend::testdir::TestDir;
use std::cell::Cell;

thread_local! {
    static RENUMBER: Cell<bool> = const { Cell::new(false) };
    static ALIAS: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn alias_inode(mut identity: Identity) -> Identity {
    if simulated_fuse() && identity.mode & 0o170000 == 0o100000 {
        identity.ino = 1;
    }
    identity
}

pub(super) fn simulated_fuse() -> bool {
    ALIAS.with(Cell::get)
}

struct Alias;
impl Drop for Alias {
    fn drop(&mut self) {
        ALIAS.with(|enabled| enabled.set(false));
    }
}

pub(super) fn renumber_staged(
    path: &Path,
    result: std::io::Result<Identity>,
) -> std::io::Result<Identity> {
    result.map(|mut value| {
        if RENUMBER.with(Cell::get)
            && matches!(
                path.file_name().and_then(OsStr::to_str),
                Some("payload" | "identity-after")
            )
        {
            value.ino ^= 1 << 63;
        }
        value
    })
}

#[test]
fn permanent_delete_rejects_a_staged_replacement_even_when_the_held_file_has_links() {
    let d = TestDir::new("path-delete-stage-swap");
    let path = d.file("chosen", "selected payload");
    let expected = identity(&path).unwrap();
    let parent = open_dir(d.path()).unwrap();
    let held =
        super::file_claim::FileClaim::open(&parent, OsStr::new("chosen"), &expected).unwrap();
    let stage_path = d.dir("stage");
    let stage = open_dir(&stage_path).unwrap();
    std::fs::hard_link(&path, d.join("chosen-link")).unwrap();
    let watched = File::open(&path).unwrap();
    rename(&parent, OsStr::new("chosen"), &stage, OsStr::new("payload")).unwrap();
    let replacement = d.file("replacement", "different bytes!");
    let file = File::open(&replacement).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(path_metadata_time(&expected)))
        .unwrap();
    std::fs::rename(&replacement, stage_path.join("payload")).unwrap();
    // The overwritten dentry reads like a FUSE invalidation; only the kernel inode watch tells it apart.
    let stage_location =
        std::fs::read_link(format!("/proc/self/fd/{}", stage.as_raw_fd())).unwrap();
    let mut invalidated = stage_location.join("payload").into_os_string();
    invalidated.push(" (deleted)");
    assert_eq!(
        std::fs::read_link(format!("/proc/self/fd/{}", watched.as_raw_fd())).unwrap(),
        PathBuf::from(invalidated)
    );
    assert!(!super::file_claim::renumbers(&stage).unwrap());
    assert!(!held.same_kernel_inode(&stage_path.join("payload")).unwrap());
    assert_eq!(
        held.remove(&stage).unwrap_err().outcome(),
        &Outcome::ItemChanged
    );
    assert_eq!(std::fs::read_dir(&stage_path).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_to_string(stage_path.join("payload")).unwrap(),
        "different bytes!"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("chosen-link")).unwrap(),
        "selected payload"
    );
}

fn path_metadata_time(expected: &Identity) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::new(expected.mtime as u64, expected.nanos as u32)
}

struct Renumber;
impl Renumber {
    fn start() -> Self {
        RENUMBER.with(|enabled| enabled.set(true));
        Self
    }
}
impl Drop for Renumber {
    fn drop(&mut self) {
        RENUMBER.with(|enabled| enabled.set(false));
    }
}

fn review(d: &TestDir, path: &Path) -> PathReview {
    assert!(path.is_absolute() && path.starts_with(d.path()));
    assert!(d.path().join(".flea-test-sandbox").is_file());
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
fn permanent_delete_accepts_a_rename_that_renumbers_the_staged_inode() {
    let d = TestDir::new("path-delete-renumber");
    let path = d.file("chosen", "selected payload");
    let reviewed = review(&d, &path);
    let _renumber = Renumber::start();
    reviewed.delete(&d.dir("recovery")).unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 2);
}

#[test]
fn permanent_delete_accepts_the_selected_inode_through_an_invalidated_dentry() {
    let d = TestDir::new("path-delete-invalidated");
    let path = d.file("chosen", "selected payload");
    let expected = identity(&path).unwrap();
    let parent = open_dir(d.path()).unwrap();
    let held =
        super::file_claim::FileClaim::open(&parent, OsStr::new("chosen"), &expected).unwrap();
    let retained = d.join("retained-link");
    std::fs::hard_link(&path, &retained).unwrap();
    let stage_path = d.dir("stage");
    let stage = open_dir(&stage_path).unwrap();
    rename(&parent, OsStr::new("chosen"), &stage, OsStr::new("payload")).unwrap();
    // Reattach the same chosen inode through another dentry, leaving the held dentry invalidated.
    let payload = stage_path.join("payload");
    std::fs::remove_file(&payload).unwrap();
    std::fs::hard_link(&retained, &payload).unwrap();
    let _renumber = Renumber::start();
    held.remove(&stage).unwrap();
    assert!(!payload.exists());
    assert_eq!(
        std::fs::read_to_string(retained).unwrap(),
        "selected payload"
    );
}

#[test]
fn permanent_delete_rejects_same_metadata_replacement_on_a_renumbering_mount() {
    let d = TestDir::new("path-delete-renumber-swap");
    let path = d.file("chosen", "selected payload");
    ALIAS.with(|enabled| enabled.set(true));
    let _alias = Alias;
    let reviewed = review(&d, &path);
    assert!(reviewed.held.is_some());
    let replacement = d.file("replacement", "different bytes!");
    let selected_metadata = path.metadata().unwrap();
    let replacement_file = File::open(&replacement).unwrap();
    replacement_file
        .set_times(std::fs::FileTimes::new().set_modified(selected_metadata.modified().unwrap()))
        .unwrap();
    assert_eq!(
        selected_metadata.len(),
        replacement.metadata().unwrap().len()
    );
    let _renumber = Renumber::start();
    let refused = reviewed
        .delete_after(&d.dir("recovery"), || {
            std::fs::rename(&path, d.join("chosen-saved")).unwrap();
            std::fs::rename(&replacement, &path).unwrap();
        })
        .unwrap_err();
    assert_eq!(
        refused.shown(),
        "The item changed after you confirmed, so nothing was deleted."
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "different bytes!");
    assert_eq!(
        std::fs::read_to_string(d.join("chosen-saved")).unwrap(),
        "selected payload"
    );
}

#[test]
fn permanent_delete_keeps_a_replacement_with_the_reviewed_pathname_inode() {
    let d = TestDir::new("path-delete-pathname-inode");
    let path = d.file("chosen", "selected payload");
    let selected_metadata = path.metadata().unwrap();
    ALIAS.with(|enabled| enabled.set(true));
    let _alias = Alias;
    let _renumber = Renumber::start();
    let reviewed = review(&d, &path);
    std::fs::rename(&path, d.join("chosen-saved")).unwrap();
    std::fs::write(&path, "different bytes!").unwrap();
    File::open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(selected_metadata.modified().unwrap()))
        .unwrap();
    assert_eq!(selected_metadata.len(), path.metadata().unwrap().len());
    reviewed.delete(&d.dir("recovery")).unwrap_err();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "different bytes!");
    assert_eq!(
        std::fs::read_to_string(d.join("chosen-saved")).unwrap(),
        "selected payload"
    );
}

#[test]
fn permanent_delete_reports_an_inode_watch_failure_as_the_system_error() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("path-delete-watch-error");
    let path = d.file("chosen", "selected payload");
    let replacement = d.file("replacement", "different bytes!");
    let parent = open_dir(d.path()).unwrap();
    let readable = identity(&path).unwrap();
    let probe =
        super::file_claim::FileClaim::open(&parent, OsStr::new("chosen"), &readable).unwrap();
    assert!(probe.same_kernel_inode(&path).unwrap());
    let missing = probe.same_kernel_inode(&d.join("missing")).unwrap_err();
    assert_eq!(missing.outcome(), &Outcome::ItemChanged);
    for locked in [&path, &replacement] {
        std::fs::set_permissions(locked, std::fs::Permissions::from_mode(0o200)).unwrap();
    }
    // As fixlogic2_menu_tests does: a reader that bypasses the mode (root) cannot fail the watch.
    if File::open(&path).is_ok() {
        return;
    }
    let expected = identity(&path).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(path_metadata_time(&expected)))
        .unwrap();
    let held =
        super::file_claim::FileClaim::open(&parent, OsStr::new("chosen"), &expected).unwrap();
    let stage_path = d.dir("stage");
    let stage = open_dir(&stage_path).unwrap();
    rename(&parent, OsStr::new("chosen"), &stage, OsStr::new("payload")).unwrap();
    std::fs::rename(&replacement, stage_path.join("payload")).unwrap();
    let refusal = held.remove(&stage).unwrap_err();
    assert_eq!(refusal.outcome(), &Outcome::ChildLeft);
    assert!(
        refusal
            .detail()
            .starts_with("Could not compare the staged item with the held one: Permission denied"),
        "{}",
        refusal.detail()
    );
    assert!(stage_path.join("payload").exists());
}
