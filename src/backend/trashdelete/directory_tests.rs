use super::*;
use crate::backend::testdir::TestDir;
use std::cell::Cell;
use std::cell::RefCell;

thread_local! {
    static FUSE: Cell<bool> = const { Cell::new(false) };
    static RENUMBER: Cell<bool> = const { Cell::new(false) };
    static RENAMES: Cell<usize> = const { Cell::new(0) };
    static ALIAS: Cell<bool> = const { Cell::new(false) };
    static SWAP: RefCell<Option<(PathBuf, PathBuf)>> = const { RefCell::new(None) };
    static INTERRUPT: Cell<Option<&'static str>> = const { Cell::new(None) };
    static DIRECTORY_INODE: Cell<u64> = const { Cell::new(0) };
    static CHANGE_DIRECTORY: Cell<bool> = const { Cell::new(false) };
    static DIRECTORY_CHANGED: Cell<bool> = const { Cell::new(false) };
    static DIRECTORY_SWAP: RefCell<Option<(PathBuf, PathBuf, PathBuf)>> = const { RefCell::new(None) };
    static STAGED_DIRECTORY: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(super) fn alias_inode(mut value: Identity) -> Identity {
    if DIRECTORY_CHANGED.with(Cell::get) && value.ino == DIRECTORY_INODE.with(Cell::get) {
        value.ino ^= 1 << 62;
    }
    if ALIAS.with(Cell::get) {
        value.ino = 1;
    }
    value
}

pub(super) fn before_node_claim(parent: &File, name: &OsStr) {
    if name != "leaf" {
        return;
    }
    if let Some((replacement, saved)) = SWAP.with(|value| value.borrow_mut().take()) {
        let source = std::fs::canonicalize(fd_path(parent, name)).unwrap();
        let root = saved.parent().unwrap();
        assert!(root.join(".flea-test-sandbox").is_file() && source.starts_with(root));
        std::fs::rename(&source, saved).unwrap();
        std::fs::rename(replacement, source).unwrap();
    }
}

pub(super) fn after_node_claim(name: &OsStr, path: &Path) {
    if name == "chosen" {
        if let Some(replacement) = STAGED_DIRECTORY.with(|value| value.borrow_mut().take()) {
            let root = replacement.parent().unwrap();
            let staged = std::fs::canonicalize(path).unwrap();
            assert!(root.join(".flea-test-sandbox").is_file() && staged.starts_with(root));
            std::fs::remove_dir(&staged).unwrap();
            std::fs::rename(replacement, staged).unwrap();
        }
    }
    if name == "leaf" {
        if let Some((source, replacement, kept)) =
            DIRECTORY_SWAP.with(|value| value.borrow_mut().take())
        {
            let root = kept.parent().unwrap();
            assert!(root.join(".flea-test-sandbox").is_file() && source.starts_with(root));
            std::fs::rename(&source, kept).unwrap();
            std::fs::rename(replacement, source).unwrap();
        }
    }
    if name == "leaf" && CHANGE_DIRECTORY.with(Cell::get) {
        DIRECTORY_CHANGED.with(|value| value.set(true));
    }
    if INTERRUPT
        .with(Cell::get)
        .is_some_and(|target| name == target)
    {
        INTERRUPT.with(|value| value.set(None));
        panic!("interrupted after the node claim");
    }
}

pub(super) fn simulated_fuse() -> bool {
    FUSE.with(Cell::get)
}

pub(super) fn renamed() {
    RENAMES.with(|count| count.set(count.get() + 1));
}

pub(super) fn renumber_staged(
    path: &Path,
    result: std::io::Result<Identity>,
) -> std::io::Result<Identity> {
    result.map(|mut value| {
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        if RENUMBER.with(Cell::get)
            && (name == "payload" || name == "identity-after" || name.starts_with("entry-"))
        {
            value.ino ^= 1 << 63;
        }
        value
    })
}

struct Mode;
impl Mode {
    fn start(fuse: bool, renumber: bool) -> Self {
        FUSE.with(|value| value.set(fuse));
        RENUMBER.with(|value| value.set(renumber));
        RENAMES.with(|value| value.set(0));
        Self
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        FUSE.with(|value| value.set(false));
        RENUMBER.with(|value| value.set(false));
        ALIAS.with(|value| value.set(false));
        SWAP.with(|value| value.borrow_mut().take());
        INTERRUPT.with(|value| value.set(None));
        CHANGE_DIRECTORY.with(|value| value.set(false));
        DIRECTORY_CHANGED.with(|value| value.set(false));
        DIRECTORY_INODE.with(|value| value.set(0));
        DIRECTORY_SWAP.with(|value| value.borrow_mut().take());
        STAGED_DIRECTORY.with(|value| value.borrow_mut().take());
    }
}

fn review(d: &TestDir) -> PathReview {
    PathReview::prepare(
        d.join("chosen"),
        &mut Manifest::new(d.path()).unwrap(),
        d.path(),
        &Cancellation::default(),
    )
    .unwrap()
    .0
}

#[test]
fn permanent_delete_directory_tree_accepts_renumbered_claims() {
    let d = TestDir::new("directory-renumber");
    d.dir("chosen/a/b/c");
    for relative in ["root.txt", "a/one.txt", "a/b/two.txt", "a/b/c/three.txt"] {
        d.file(&format!("chosen/{relative}"), "selected payload");
    }
    let _mode = Mode::start(true, true);
    let reviewed = review(&d);
    reviewed.delete(&d.dir("recovery")).unwrap();
    assert!(!d.join("chosen").exists());
    assert!(std::fs::read_dir(d.join("recovery"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn permanent_delete_changed_directory_refuses_before_any_data_rename() {
    let d = TestDir::new("directory-preflight");
    d.dir("chosen");
    d.file("chosen/leaf", "selected payload");
    let _mode = Mode::start(true, true);
    let reviewed = review(&d);
    let refusal = reviewed
        .delete_after(&d.dir("recovery"), || {
            std::fs::write(d.join("chosen/leaf"), "changed after review").unwrap();
        })
        .unwrap_err();
    assert_eq!(refusal.outcome(), &Outcome::ItemChanged);
    assert_eq!(
        RENAMES.with(Cell::get),
        0,
        "a refused review must not copy remote user data into staging"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("chosen/leaf")).unwrap(),
        "changed after review"
    );
}

#[test]
fn permanent_delete_stable_directory_keeps_strict_inode_checks() {
    let d = TestDir::new("directory-stable-strict");
    d.dir("chosen");
    d.file("chosen/leaf", "selected payload");
    let _mode = Mode::start(false, true);
    let reviewed = review(&d);
    assert_eq!(
        reviewed.delete(&d.dir("recovery")).unwrap_err().outcome(),
        &Outcome::ItemChanged
    );
    assert_eq!(
        std::fs::read_to_string(d.join("chosen/leaf")).unwrap(),
        "selected payload"
    );
}

fn same_time(path: &Path, other: &Metadata) {
    File::open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(other.modified().unwrap()))
        .unwrap();
}

#[test]
fn permanent_delete_directory_keeps_a_claim_window_replacement_with_an_aliased_inode() {
    let d = TestDir::new("directory-child-swap");
    d.dir("chosen");
    let leaf = d.file("chosen/leaf", "selected payload");
    let replacement = d.file("replacement", "different bytes!");
    same_time(&replacement, &leaf.symlink_metadata().unwrap());
    let _mode = Mode::start(true, true);
    ALIAS.with(|value| value.set(true));
    let reviewed = review(&d);
    SWAP.with(|value| *value.borrow_mut() = Some((replacement, d.join("selected-kept"))));
    let refusal = reviewed.delete(&d.dir("recovery")).unwrap_err();
    assert_eq!(refusal.outcome(), &Outcome::ItemChanged);
    assert_eq!(std::fs::read_to_string(&leaf).unwrap(), "different bytes!");
    assert_eq!(
        std::fs::read_to_string(d.join("selected-kept")).unwrap(),
        "selected payload"
    );
}

#[test]
fn permanent_delete_directory_refuses_a_replaced_root_before_renaming_user_data() {
    let d = TestDir::new("directory-root-swap");
    let source = d.dir("chosen");
    let leaf = d.file("chosen/leaf", "selected payload");
    let replacement = d.dir("replacement");
    let replacement_leaf = d.file("replacement/leaf", "different bytes!");
    same_time(&replacement_leaf, &leaf.symlink_metadata().unwrap());
    let metadata = source.symlink_metadata().unwrap();
    let _mode = Mode::start(true, true);
    ALIAS.with(|value| value.set(true));
    let reviewed = review(&d);
    let refusal = reviewed
        .delete_after(&d.dir("recovery"), || {
            std::fs::rename(&source, d.join("selected-kept")).unwrap();
            std::fs::rename(&replacement, &source).unwrap();
            same_time(&source, &metadata);
        })
        .unwrap_err();
    assert_eq!(refusal.outcome(), &Outcome::ItemChanged);
    assert_eq!(RENAMES.with(Cell::get), 0);
    assert_eq!(
        std::fs::read_to_string(source.join("leaf")).unwrap(),
        "different bytes!"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("selected-kept/leaf")).unwrap(),
        "selected payload"
    );
}

#[test]
fn permanent_delete_directory_recovers_an_interrupted_child_beside_the_original_root() {
    let d = TestDir::new("directory-interrupted-child");
    d.dir("chosen");
    let leaf = d.file("chosen/leaf", "selected payload");
    let _mode = Mode::start(true, false);
    let reviewed = review(&d);
    let recovery = d.dir("recovery");
    INTERRUPT.with(|value| value.set(Some("leaf")));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reviewed.delete(&recovery)))
            .is_err()
    );
    assert!(!leaf.exists());
    assert!(d.join("chosen").is_dir());
    let report = recover(&recovery).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.recovered, 1);
    assert_eq!(std::fs::read_to_string(leaf).unwrap(), "selected payload");
}

#[test]
fn permanent_delete_directory_recovers_an_interrupted_emptied_root_to_its_original_name() {
    let d = TestDir::new("directory-interrupted-root");
    let root = d.dir("chosen");
    let leaf = d.file("chosen/leaf", "selected payload");
    let _mode = Mode::start(true, false);
    let reviewed = review(&d);
    let recovery = d.dir("recovery");
    INTERRUPT.with(|value| value.set(Some("chosen")));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reviewed.delete(&recovery)))
            .is_err()
    );
    assert!(!leaf.exists() && !root.exists());
    let report = recover(&recovery).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.recovered, 1);
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
}

#[test]
fn permanent_delete_directory_keeps_kernel_identity_when_its_inode_changes_after_removing_children()
{
    let d = TestDir::new("directory-parent-attributes");
    let root = d.dir("chosen");
    d.file("chosen/leaf", "selected payload");
    let _mode = Mode::start(true, false);
    DIRECTORY_INODE.with(|value| value.set(root.symlink_metadata().unwrap().ino()));
    CHANGE_DIRECTORY.with(|value| value.set(true));
    let reviewed = review(&d);
    reviewed.delete(&d.dir("recovery")).unwrap();
    assert!(!root.exists());
}

#[test]
fn permanent_delete_directory_preserves_a_replacement_parent_after_removing_its_reviewed_child() {
    let d = TestDir::new("directory-parent-replacement");
    let source = d.dir("chosen/sub");
    d.file("chosen/sub/leaf", "selected payload");
    let replacement = d.dir("replacement");
    let _mode = Mode::start(true, true);
    ALIAS.with(|value| value.set(true));
    let reviewed = review(&d);
    DIRECTORY_SWAP.with(|value| {
        *value.borrow_mut() = Some((source.clone(), replacement, d.join("selected-kept")))
    });
    let refusal = reviewed.delete(&d.dir("recovery")).unwrap_err();
    assert_eq!(refusal.outcome(), &Outcome::PartialAtOriginal);
    assert!(
        source.is_dir(),
        "the unreviewed empty replacement must survive"
    );
    assert!(d.join("selected-kept").is_dir());
}

#[test]
fn permanent_delete_directory_preserves_an_unreviewed_staged_directory_with_aliased_metadata() {
    let d = TestDir::new("directory-staged-replacement");
    let source = d.dir("chosen");
    let replacement = d.dir("replacement");
    same_time(&replacement, &source.symlink_metadata().unwrap());
    let _mode = Mode::start(true, true);
    ALIAS.with(|value| value.set(true));
    let reviewed = review(&d);
    STAGED_DIRECTORY.with(|value| *value.borrow_mut() = Some(replacement));
    assert!(reviewed.delete(&d.dir("recovery")).is_err());
    assert!(
        source.is_dir(),
        "the unreviewed replacement must be returned and never removed"
    );
}

#[test]
fn permanent_delete_stable_fuse_directory_does_not_require_read_permission_on_files() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("directory-unreadable-file");
    let root = d.dir("chosen");
    let leaf = d.file("chosen/leaf", "selected payload");
    std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o200)).unwrap();
    let _mode = Mode::start(true, false);
    let reviewed = review(&d);
    reviewed.delete(&d.dir("recovery")).unwrap();
    assert!(!root.exists());
}
